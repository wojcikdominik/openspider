//! Offline AI fixtures (08, Konventionen). No provider connections are made.
mod answers;
mod batch;
mod response;

use super::MockApi;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, Method, Uri},
    response::{IntoResponse, Response},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    io,
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

pub use answers::{embedding, token_count, AnswerBook};

#[derive(Clone, Debug, Serialize)]
pub struct RequestRecord {
    pub timestamp_ms: u128,
    pub method: String,
    pub path: String,
    pub host: String,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}
#[derive(Default)]
struct Data {
    log: Vec<RequestRecord>,
    scenarios: HashMap<String, (String, usize)>,
    attempts: HashMap<(String, String, String, String), usize>,
    batches: HashMap<String, batch::Batch>,
    files: HashMap<String, String>,
    models: HashMap<String, Value>,
    next_id: usize,
}
#[derive(Clone)]
pub struct AiMock {
    data: Arc<Mutex<Data>>,
    answers: Arc<AnswerBook>,
}
impl AiMock {
    pub fn from_fragments(dir: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            data: Arc::new(Mutex::new(Data::default())),
            answers: Arc::new(AnswerBook::load(dir)?),
        })
    }
    pub fn fixtures() -> io::Result<Self> {
        Self::from_fragments(crate::fixtures_root().join("ai").join("mock-answers.d"))
    }
    /// Replace a provider catalog to exercise reloading live capabilities/prices.
    pub fn set_models(&self, provider: &str, models: Value) {
        self.data
            .lock()
            .unwrap()
            .models
            .insert(provider.into(), models);
    }
    pub fn requests(&self) -> Vec<RequestRecord> {
        self.data.lock().unwrap().log.clone()
    }
    /// All providers and controls share one state, also when mounted in fixture-apis.
    pub fn registrations(&self) -> Vec<Arc<dyn MockApi>> {
        ["anthropic", "openai", "openrouter", "ollama", "__mock"]
            .into_iter()
            .map(|name| {
                Arc::new(Registration {
                    name,
                    mock: self.clone(),
                }) as Arc<dyn MockApi>
            })
            .collect()
    }
}
struct Registration {
    name: &'static str,
    mock: AiMock,
}
impl MockApi for Registration {
    fn name(&self) -> &'static str {
        self.name
    }
    fn base_url_key(&self) -> Option<&'static str> {
        match self.name {
            "anthropic" => Some("api.anthropic.base_url"),
            "openai" => Some("api.openai.base_url"),
            "openrouter" => Some("api.openrouter.base_url"),
            "ollama" => Some("api.ollama.base_url"),
            _ => None,
        }
    }
    fn scenario_path(&self) -> &'static str {
        match self.name {
            "anthropic" | "openai" => "/v1/models",
            "openrouter" => "/api/v1/models",
            "ollama" => "/api/tags",
            _ => "/log",
        }
    }
    fn router(&self) -> Router {
        let provider = self.name.to_string();
        Router::new()
            .fallback(
                move |State(mock): State<AiMock>,
                      method: Method,
                      uri: Uri,
                      headers: HeaderMap,
                      body: Bytes| {
                    let provider = provider.clone();
                    async move { handle(mock, &provider, method, uri, headers, body).await }
                },
            )
            .with_state(self.mock.clone())
    }
}
#[derive(Deserialize)]
struct Scenario {
    provider: String,
    scenario: String,
    count: usize,
}
fn marker(text: &str, prefix: &str) -> Option<String> {
    text.split_once(prefix)
        .map(|(_, tail)| {
            tail.chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '='))
                .collect()
        })
        .filter(|s: &String| !s.is_empty())
}
async fn handle(
    mock: AiMock,
    provider: &str,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let text = String::from_utf8_lossy(&body).into_owned();
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    {
        let redacted = headers
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    if matches!(k.as_str(), "authorization" | "x-api-key" | "api-key") {
                        "[REDACTED]".into()
                    } else {
                        v.to_str().unwrap_or("[binary]").into()
                    },
                )
            })
            .collect();
        mock.data.lock().unwrap().log.push(RequestRecord {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            method: method.to_string(),
            path: format!("/{provider}{uri}"),
            host: host.clone(),
            headers: redacted,
            body: text.clone(),
        });
    }
    if provider == "__mock" {
        return match (method, uri.path()) {
            (Method::GET, "/log") => Json(json!(mock.requests())).into_response(),
            (Method::POST, "/scenario") => match serde_json::from_slice::<Scenario>(&body) {
                Ok(s)
                    if ["anthropic", "openai", "openrouter", "ollama"]
                        .contains(&s.provider.as_str()) =>
                {
                    let mut d = mock.data.lock().unwrap();
                    d.attempts.retain(|(p, _, _, _), _| p != &s.provider);
                    d.scenarios.insert(s.provider, (s.scenario, s.count));
                    Json(json!({"ok":true})).into_response()
                }
                _ => response::error(
                    provider,
                    400,
                    "invalid_request_error",
                    "Invalid scenario control",
                    None,
                ),
            },
            _ => response::error(provider, 404, "not_found", "Unknown control", None),
        };
    }
    let input: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let (scenario, attempt) = {
        let mut d = mock.data.lock().unwrap();
        let explicit = headers
            .get("x-scenario")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .or_else(|| marker(&text, "MOCK-SCENARIO:"));
        let scenario = explicit.unwrap_or_else(|| match d.scenarios.get_mut(provider) {
            Some((s, n)) if *n > 0 => {
                *n -= 1;
                s.clone()
            }
            _ => "ok".into(),
        });
        let n = d
            .attempts
            .entry((
                provider.into(),
                host.clone(),
                uri.path().into(),
                format!("{scenario}:{text}"),
            ))
            .or_default();
        let attempt = *n;
        *n += 1;
        (scenario, attempt)
    };
    if let Some(r) = response::scenario(provider, &host, &scenario, attempt).await {
        return r;
    }
    if method == Method::GET
        && (uri.path().ends_with("/models") || uri.path().ends_with("/tags"))
        && scenario != "empty"
    {
        if let Some(models) = mock.data.lock().unwrap().models.get(provider).cloned() {
            return Json(models).into_response();
        }
    }
    if let Some(r) = batch::handle(
        &mock,
        provider,
        &method,
        uri.path(),
        &headers,
        &text,
        &scenario,
    ) {
        return r;
    }
    response::success(
        &mock.answers,
        provider,
        &host,
        &method,
        uri.path(),
        &input,
        &scenario,
    )
}

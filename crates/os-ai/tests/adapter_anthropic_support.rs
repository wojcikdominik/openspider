//! Gemeinsame Helfer der Adapter-Tests M4-K01 (`adapter_anthropic.rs`, `adapter_openrouter.rs`); per `#[path]`
//! eingebunden, enthält selbst keine Tests.
//!
//! - ai-mock aus `os-testsite` (Fragmente `tests/fixtures/ai/mock-answers.d/*.json`, darunter `k01.json`) auf einem
//!   Loopback-Port; die Adapter erhalten die Mock-Adresse als Basis-URL (INTEG-019).
//! - Testuhr [`VirtualClock`]: Wartezeiten des Limiters (Backoff, `Retry-After`, Header-Pausen) vergehen virtuell;
//!   nur der Request-Timeout bleibt eine echte Wartezeit, weil der Mock über echtes HTTP antwortet.
//! - [`HeaderServer`]: minimaler HTTP-Server für Antworten mit Rate-Limit-Headern, die ai-mock nicht sendet.
#![allow(dead_code)]

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_ai::adapters::anthropic::{AdapterReply, AnthropicConfig, ApiKey};
use os_ai::adapters::openrouter::OpenRouterConfig;
use os_ai::limits::{Execution, FixedJitter, LimitClock, Limiter, LimitsConfig, ProviderLimits};
use os_ai::provider::{Capabilities, CapabilitySource, Input, Model, Request};
use os_ai::AiStatus;
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Testschlüssel aus AI-002 (darf in keinem Log erscheinen).
pub const KEY: &str = "sk-ant-TEST-AI-002";
/// Wanduhr der Testuhr: 2026-11-24T10:00:00Z.
pub const WALL: i64 = 1_795_514_400_000;
/// Timeout, der in normalen Tests nie erreicht wird.
pub const LONG_TIMEOUT: Duration = Duration::from_secs(600);

/// Testuhr: `sleep(d)` rückt die Uhr sofort um `d` vor – außer für genau den Request-Timeout, der echt wartet.
pub struct VirtualClock {
    now: Mutex<Duration>,
    wall_origin_ms: i64,
    real_timeout: Duration,
}

impl VirtualClock {
    pub fn new(real_timeout: Duration) -> Self {
        Self {
            now: Mutex::new(Duration::ZERO),
            wall_origin_ms: WALL,
            real_timeout,
        }
    }
}

impl LimitClock for VirtualClock {
    fn now(&self) -> Duration {
        *self.now.lock().unwrap()
    }

    fn wall_unix_millis(&self) -> i64 {
        self.wall_origin_ms + i64::try_from(self.now().as_millis()).unwrap()
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        if duration == self.real_timeout {
            return Box::pin(tokio::time::sleep(duration));
        }
        *self.now.lock().unwrap() += duration;
        Box::pin(tokio::task::yield_now())
    }
}

/// Limiter für Anthropic und OpenRouter mit `concurrent` gleichzeitigen Anfragen, Timeout und Testuhr; Jitter 0,5
/// (Backoff ohne Zufall: 1 s, 2 s, 4 s …).
pub fn limiter(timeout: Duration, concurrent: u32) -> Arc<Limiter> {
    limiter_with(timeout, concurrent, |_| {})
}

/// Wie [`limiter`], mit Anpassung der Konfiguration.
pub fn limiter_with(
    timeout: Duration,
    concurrent: u32,
    adjust: impl FnOnce(&mut LimitsConfig),
) -> Arc<Limiter> {
    let mut config = LimitsConfig::default();
    for provider in ["anthropic", "openrouter"] {
        config.providers.insert(
            provider.into(),
            ProviderLimits {
                concurrent_requests: concurrent,
                timeout,
            },
        );
    }
    adjust(&mut config);
    Arc::new(Limiter::with_jitter(
        config,
        Arc::new(VirtualClock::new(timeout)),
        Arc::new(FixedJitter(0.5)),
    ))
}

/// Laufender ai-mock.
pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
    pub http: reqwest::Client,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::fixtures().expect("Fragmente unter tests/fixtures/ai/mock-answers.d");
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Self {
            mock,
            server,
            http: reqwest::Client::new(),
        }
    }

    pub fn anthropic_config(&self) -> AnthropicConfig {
        AnthropicConfig::new(ApiKey::new(KEY)).with_base_url(self.server.base_url("anthropic"))
    }

    pub fn openrouter_config(&self) -> OpenRouterConfig {
        OpenRouterConfig::new(ApiKey::new(KEY))
            .with_base_url(format!("{}/api/v1", self.server.base_url("openrouter")))
    }

    /// `POST /__mock/scenario` (gilt für die nächsten `count` Anfragen an `provider`).
    pub async fn scenario(&self, provider: &str, scenario: &str, count: usize) {
        let response = self
            .http
            .post(format!("{}/scenario", self.server.base_url("__mock")))
            .header("content-type", "application/json")
            .body(json!({"provider": provider, "scenario": scenario, "count": count}).to_string())
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
    }

    /// Alle Anfragen an `path` (ohne Query), z. B. `/anthropic/v1/messages`.
    pub fn requests_to(&self, path: &str) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.path.split('?').next() == Some(path))
            .collect()
    }

    /// JSON-Körper der Anfragen an `path`.
    pub fn bodies(&self, path: &str) -> Vec<Value> {
        self.requests_to(path)
            .iter()
            .map(|r| serde_json::from_str(&r.body).unwrap())
            .collect()
    }
}

/// Modell mit den genannten Fähigkeiten.
pub fn model(provider: &str, id: &str, capabilities: Capabilities) -> Model {
    Model {
        provider: provider.into(),
        id: id.into(),
        endpoint: if provider == "anthropic" {
            "messages".into()
        } else {
            "chat-completions".into()
        },
        capabilities,
        capability_source: if provider == "anthropic" {
            CapabilitySource::AnthropicModels
        } else {
            CapabilitySource::OpenRouterModels
        },
        reasoning_levels: Vec::new(),
    }
}

/// Text, Structured Output, Vision, Batch.
pub fn full_caps() -> Capabilities {
    Capabilities {
        text: true,
        structured_output: true,
        vision: true,
        batch: true,
        ..Capabilities::default()
    }
}

pub fn anthropic_model() -> Model {
    model("anthropic", "mock-claude", full_caps())
}

pub fn openrouter_model() -> Model {
    model("openrouter", "mock/vision", full_caps())
}

/// Text-Aufgabe ohne Schema.
pub fn text_request(model: Model, text: &str) -> Request {
    Request {
        model,
        required: Capabilities {
            text: true,
            ..Capabilities::default()
        },
        input: vec![Input::Text { text: text.into() }],
        instructions: "Antworte knapp.".into(),
        questions: Vec::new(),
        response_schema: None,
        max_tokens: Some(256),
        temperature: None,
        effort: None,
        reasoning: Default::default(),
    }
}

/// Aufgabe mit Antwortschema.
pub fn schema_request(model: Model, text: &str, schema: Value) -> Request {
    let mut request = text_request(model, text);
    request.response_schema = Some(schema);
    request
}

/// Contract-Schema aus AI-001: `{"ok": true}`.
pub fn contract_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}},
        "required": ["ok"],
        "additionalProperties": false
    })
}

/// Schema der Antwort `k01-structured` aus `k01.json`.
pub fn label_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}, "label": {"type": "string"}},
        "required": ["ok", "label"],
        "additionalProperties": false
    })
}

/// Endstatus eines Eintrags nach AI-020.
pub fn status(execution: &Execution<AdapterReply>, schema: Option<&Value>) -> AiStatus {
    match &execution.result {
        Ok(reply) => match reply.clone().into_response(schema) {
            Ok(response) if response.refusal => AiStatus::Refusal,
            Ok(_) => AiStatus::Success,
            Err(error) => error.status().unwrap(),
        },
        Err(final_error) => final_error.status.clone(),
    }
}

/// Eine vorbereitete Antwort des [`HeaderServer`].
#[derive(Clone)]
pub struct Canned {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Minimaler HTTP/1.1-Server: beantwortet Anfragen der Reihe nach mit den vorbereiteten Antworten (die letzte
/// wiederholt sich) und merkt sich die Anfragezeilen.
pub struct HeaderServer {
    pub addr: SocketAddr,
    pub seen: Arc<Mutex<Vec<String>>>,
}

impl HeaderServer {
    pub async fn start(responses: Vec<Canned>) -> Self {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            let mut index = 0usize;
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let head_end = loop {
                    let n = stream.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break None;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(pos + 4);
                    }
                };
                let Some(head_end) = head_end else { continue };
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let length = head
                    .lines()
                    .find_map(|l| {
                        let (name, value) = l.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                while buf.len() < head_end + length {
                    let n = stream.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                log.lock()
                    .unwrap()
                    .push(head.lines().next().unwrap_or("").to_string());
                let canned = responses[index.min(responses.len() - 1)].clone();
                index += 1;
                let mut out = format!(
                    "HTTP/1.1 {} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
                    canned.status,
                    canned.body.len()
                );
                for (name, value) in &canned.headers {
                    out.push_str(&format!("{name}: {value}\r\n"));
                }
                out.push_str("\r\n");
                out.push_str(&canned.body);
                let _ = stream.write_all(out.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        Self { addr, seen }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn requests(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

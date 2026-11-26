//! Gemeinsame Hilfen der unabhängigen Abnahmetests M4-U01 (nur aus `docs/spec/*` und `docs/plan/M4.md` abgeleitet).
//!
//! - App mit eigenem Einstellungsordner und Schlüsselspeicher im Arbeitsspeicher (Testschlüssel, nie echte).
//! - Konfiguration des Fensters über `config.get`/`config.set` (wie der Konfigurationsdialog).
//! - ai-mock (`os-testsite`) und ein Test-Adapter, der den vom Wächter vorbereiteten Request an den Mock schickt; was im
//!   Request-Log des Mocks steht, hat „die Maschine verlassen“ (AI-018 „Mock-Zähler“).
//! - Live-Modelllisten aus dem ai-mock (Anthropic-Modelle A–D, OpenRouter) in der Form, die die Modellwahl erwartet.
// acceptance: independent
#![allow(dead_code, clippy::disallowed_types, clippy::disallowed_methods)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use os_ai::adapters::anthropic::AnthropicModel;
use os_ai::adapters::openrouter::OpenRouterModel;
use os_ai::catalog::reasoning::{
    anthropic_levels, anthropic_output_limit, openrouter_levels, openrouter_output_limit,
};
use os_ai::catalog::AvailableModel;
use os_ai::provider::{
    AiProvider, Capabilities, CapabilitySource, Model, ProviderError, ProviderFuture, Request,
    Response, Usage,
};
use os_app::dto::config::CrawlConfigDto;
use os_app::{App, AppConfig, CallCtx, CmdError};
use os_fetch::secrets::MemorySecretStore;
use os_model::CrawlConfig;
use os_testsite::mocks::{ai::AiMock, MockServer};
use serde_json::{json, Value};

/// Testschlüssel (nie ein echter Schlüssel; nur im Arbeitsspeicher).
pub const TEST_KEY: &str = "sk-TEST-U01-acceptance-0000000000";

pub fn repo_file(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

pub fn repo_json(rel: &str) -> Value {
    let path = repo_file(rel);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn ctx() -> CallCtx {
    CallCtx::for_window("main")
}

/// App mit Einstellungen unter `home` und Testschlüsseln für die genannten Anbieter (`ai.<anbieter>`, AI-009).
pub fn app_with_keys(home: &Path, providers: &[&str]) -> App {
    let secrets = Arc::new(MemorySecretStore::new());
    for p in providers {
        secrets.set(&format!("ai.{p}"), TEST_KEY);
    }
    App::new(AppConfig::new(home).with_secrets(secrets)).expect("App startet")
}

pub fn fresh_app(home: &Path) -> App {
    app_with_keys(home, &[])
}

pub async fn call(app: &App, name: &str, args: Value) -> Value {
    app.dispatch(ctx(), name, args)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

pub async fn try_call(app: &App, name: &str, args: Value) -> Result<Value, CmdError> {
    app.dispatch(ctx(), name, args).await
}

/// Schalter „KI (Vorschau)“ einschalten (App-Einstellung `aiApp.previewEnabled`).
pub async fn preview_on(app: &App) {
    call(
        app,
        "settings.set",
        json!({ "patch": { "aiApp": { "previewEnabled": true } } }),
    )
    .await;
}

pub async fn patch_settings(app: &App, patch: Value) {
    call(app, "settings.set", json!({ "patch": patch })).await;
}

/// Aktuelle Konfiguration des Fensters (`config.get`).
pub async fn window_config(app: &App) -> CrawlConfig {
    let dto = call(app, "config.get", Value::Null).await;
    serde_json::from_value(dto["config"].clone()).expect("config.get liefert eine CrawlConfig")
}

/// Ändert die Konfiguration des Fensters wie der Konfigurationsdialog (`config.set` mit dem ganzen Entwurf).
pub async fn edit_config(app: &App, f: impl FnOnce(&mut CrawlConfig)) {
    let mut cfg = window_config(app).await;
    f(&mut cfg);
    let dto = serde_json::to_value(CrawlConfigDto {
        schema_version: cfg.schema_version,
        generator: cfg.generator.clone(),
        config: serde_json::to_value(&cfg).unwrap(),
    })
    .unwrap();
    call(app, "config.set", dto).await;
}

/// Eine aktive Seitenfrage mit den Eingabefeldern `inputs` (AI-030 `inputs`) im Profil (Aufgabe `decision`).
pub fn with_question(cfg: &mut CrawlConfig, id: &str, inputs: &[&str]) {
    cfg.ai.enabled = true;
    cfg.ai.questions.retain(|q| q["id"] != id);
    cfg.ai.questions.push(json!({
        "id": id,
        "version": "1.0.0",
        "name": "U01 Abnahme",
        "type": "predicate",
        "instructions": "Abnahmefrage U01",
        "inputs": inputs,
    }));
    if !cfg.ai.active_questions.iter().any(|a| a == id) {
        cfg.ai.active_questions.push(id.to_string());
    }
}

/// Zeile einer Aufgabe aus einer Antwort mit `tasks` (`ai.settingsGet` bzw. `ai.settingsSet`).
pub fn row<'a>(view: &'a Value, task: &str) -> &'a Value {
    view["tasks"]
        .as_array()
        .expect("tasks ist eine Liste")
        .iter()
        .find(|r| r["task"] == task)
        .unwrap_or_else(|| panic!("Aufgabe {task} fehlt: {view}"))
}

pub fn model_ref(provider: &str, model: &str, endpoint: &str) -> Value {
    json!({ "provider": provider, "model": model, "endpoint": endpoint })
}

// ---------------------------------------------------------------------------------------------------------------
// ai-mock

pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::fixtures().expect("ai-mock fixtures");
        let server = MockServer::start(mock.registrations())
            .await
            .expect("ai-mock startet");
        Self { mock, server }
    }

    /// Requests an Anbieter-Routen ohne Steueraufrufe und ohne Modelllisten (die lädt der Kontenbereich, AI-010).
    pub fn content_requests(&self) -> usize {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| !r.path.starts_with("/__mock") && !r.path.ends_with("/models"))
            .count()
    }

    async fn get_json(&self, url: &str) -> Value {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = client
            .get(url)
            .header("x-api-key", TEST_KEY)
            .header("authorization", format!("Bearer {TEST_KEY}"))
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .unwrap_or_else(|e| panic!("{url}: {e}"));
        resp.json().await.expect("Modellliste ist JSON")
    }

    /// Anthropic-Modellliste des Mocks (Rohdaten).
    pub async fn anthropic_entries(&self) -> Vec<Value> {
        let url = format!("{}/v1/models", self.server.base_url("anthropic"));
        self.get_json(&url).await["data"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// OpenRouter-Modellliste des Mocks (Rohdaten).
    pub async fn openrouter_entries(&self) -> Vec<Value> {
        let url = format!("{}/api/v1/models", self.server.base_url("openrouter"));
        self.get_json(&url).await["data"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    pub fn provider(&self, id: &'static str) -> ForwardingProvider {
        ForwardingProvider::new(id, &self.server)
    }
}

/// Modell der Anthropic-Liste mit Fähigkeiten (AI-002) und Stufen/Ausgabegrenze (AI-011, Nachtrag).
pub fn anthropic_available(entry: &Value) -> AvailableModel {
    let mut model = AnthropicModel::from_json(entry)
        .expect("Anthropic-Eintrag lesbar")
        .to_model();
    model.reasoning_levels = anthropic_levels(entry);
    AvailableModel {
        model,
        max_output_tokens: anthropic_output_limit(entry),
    }
}

pub fn openrouter_available(entry: &Value) -> AvailableModel {
    let mut model = OpenRouterModel::from_json(entry)
        .expect("OpenRouter-Eintrag lesbar")
        .to_model();
    model.reasoning_levels = openrouter_levels(entry);
    AvailableModel {
        model,
        max_output_tokens: openrouter_output_limit(entry),
    }
}

/// Die Modelle A–D der Anthropic-Mockliste nach ihren Eigenschaften aus 08 „Konventionen“ (Nachtrag Q00):
/// A abschaltbar (`thinking.types.disabled`), B nicht abschaltbar mit `max_tokens = 4000`, C ohne `thinking`, D ohne
/// `capabilities`.
pub struct AnthropicSet {
    pub a: Value,
    pub b: Value,
    pub c: Value,
    pub d: Value,
}

pub fn anthropic_set(entries: &[Value]) -> AnthropicSet {
    let disabled =
        |e: &Value| e["capabilities"]["thinking"]["types"]["disabled"]["supported"] == true;
    let find = |pred: &dyn Fn(&Value) -> bool, what: &str| {
        entries
            .iter()
            .find(|e| pred(e))
            .cloned()
            .unwrap_or_else(|| panic!("Mockliste ohne Modell {what}"))
    };
    AnthropicSet {
        a: find(&|e| disabled(e) && e["max_tokens"] != 4000, "A"),
        b: find(
            &|e| e["max_tokens"] == 4000 && e["capabilities"].is_object() && !disabled(e),
            "B",
        ),
        c: find(
            &|e| {
                e["capabilities"].is_object() && e["capabilities"]["thinking"]["supported"] == false
            },
            "C",
        ),
        d: find(&|e| !e["capabilities"].is_object(), "D"),
    }
}

/// Test-Adapter: sendet `instructions` und `input` des Requests als JSON an den ai-mock.
pub struct ForwardingProvider {
    id: &'static str,
    url: String,
    client: reqwest::Client,
}

impl ForwardingProvider {
    pub fn new(id: &'static str, server: &MockServer) -> Self {
        let url = match id {
            "anthropic" => format!("{}/v1/messages", server.base_url("anthropic")),
            "openrouter" => format!("{}/api/v1/chat/completions", server.base_url("openrouter")),
            "ollama" => format!("{}/api/chat", server.base_url("ollama")),
            _ => format!("{}/v1/responses", server.base_url("openai")),
        };
        Self {
            id,
            url,
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        }
    }

    pub fn model(&self, model_id: &str) -> Model {
        Model {
            provider: self.id.to_string(),
            id: model_id.to_string(),
            endpoint: self.url.clone(),
            capabilities: Capabilities {
                text: true,
                structured_output: true,
                vision: true,
                ..Capabilities::default()
            },
            capability_source: CapabilitySource::ModelCatalog,
            reasoning_levels: Vec::new(),
        }
    }

    pub fn request(&self, model_id: &str) -> Request {
        Request {
            model: self.model(model_id),
            required: Capabilities {
                text: true,
                ..Capabilities::default()
            },
            input: Vec::new(),
            instructions: "Abnahme U01".into(),
            questions: Vec::new(),
            response_schema: None,
            max_tokens: None,
            temperature: None,
            effort: None,
            reasoning: Default::default(),
        }
    }
}

impl AiProvider for ForwardingProvider {
    fn provider_id(&self) -> &str {
        self.id
    }

    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async move { Ok(vec![self.model("u01-model")]) })
    }

    fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
        Box::pin(async move {
            let body = json!({
                "model": request.model.id,
                "instructions": request.instructions,
                "input": request.input,
            });
            let resp = self
                .client
                .post(&self.url)
                .header("content-type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .map_err(|e| ProviderError::Transport {
                    message: e.to_string(),
                })?;
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            if status >= 400 {
                return Err(ProviderError::Http {
                    status,
                    message: text,
                });
            }
            Ok(Response {
                content: serde_json::from_str::<Value>(&text).ok(),
                usage: Usage::default(),
                refusal: false,
                provider_request_id: None,
                duration_ms: 0,
            })
        })
    }
}

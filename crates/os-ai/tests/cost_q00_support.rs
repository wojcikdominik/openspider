//! Gemeinsame Helfer der Tests von M4-K10 (Nachträge 24.11.2026, Q00); wird per `#[path]` eingebunden und enthält
//! selbst keine Tests. Alle Anbieter laufen gegen ai-mock (`os_testsite::mocks::ai`), nie gegen echte Anbieter.
#![allow(dead_code)]

use std::sync::Arc;

use os_ai::adapters::anthropic::{AnthropicAdapter, AnthropicConfig, ApiKey};
use os_ai::adapters::ollama::{OllamaAdapter, OllamaConfig};
use os_ai::adapters::openai_responses::{self, OpenAiAccount, OpenAiConfig, OpenAiHttp};
use os_ai::adapters::openrouter::{self, OpenRouterAdapter, OpenRouterConfig};
use os_ai::cost::LogContext;
use os_ai::limits::{Limiter, LimitsConfig, TokioClock};
use os_ai::provider::{Capabilities, CapabilitySource, Input, Model, ReasoningLevel, Request};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const MESSAGES: &str = "/anthropic/v1/messages";
pub const RESPONSES: &str = "/openai/v1/responses";
pub const DECISIONS: &str = "/openai/v1/decisions";
pub const CHAT: &str = "/openrouter/api/v1/chat/completions";
pub const OLLAMA_CHAT: &str = "/ollama/api/chat";

pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::fixtures().expect("ai-mock-Fragmente");
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Self { mock, server }
    }
    pub fn requests_to(&self, path: &str) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.path.split('?').next() == Some(path))
            .collect()
    }
    pub fn bodies(&self, path: &str) -> Vec<Value> {
        self.requests_to(path)
            .iter()
            .map(|r| serde_json::from_str(&r.body).unwrap())
            .collect()
    }
    pub fn anthropic(&self) -> AnthropicAdapter {
        AnthropicAdapter::new(
            client(),
            AnthropicConfig::new(ApiKey::new("sk-ant-TEST-K10"))
                .with_base_url(self.server.base_url("anthropic")),
        )
    }
    pub fn openrouter(&self) -> OpenRouterAdapter {
        OpenRouterAdapter::new(
            client(),
            OpenRouterConfig::new(openrouter::ApiKey::new("sk-or-TEST-K10"))
                .with_base_url(format!("{}/api/v1", self.server.base_url("openrouter"))),
        )
    }
    pub fn openai_http(&self) -> OpenAiHttp {
        OpenAiHttp::new(
            client(),
            OpenAiConfig::new(openai_responses::ApiKey::new("sk-TEST-K10"))
                .with_base_url(format!("{}/v1", self.server.base_url("openai"))),
            OpenAiAccount::new(),
        )
    }
    pub fn ollama(&self) -> OllamaAdapter {
        OllamaAdapter::new(
            client(),
            OllamaConfig::new().with_base_url(self.server.base_url("ollama")),
        )
    }
    /// Globales Szenario für die nächsten `count` Requests eines Anbieters (ohne Marker im Text).
    pub async fn global_scenario(&self, provider: &str, scenario: &str, count: usize) {
        let r = client()
            .post(format!("{}/scenario", self.server.base_url("__mock")))
            .header("content-type", "application/json")
            .body(json!({"provider": provider, "scenario": scenario, "count": count}).to_string())
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success());
    }
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

pub fn limiter() -> Limiter {
    Limiter::new(LimitsConfig::default(), Arc::new(TokioClock::new()))
}

pub fn model(provider: &str, id: &str, endpoint: &str, levels: Vec<ReasoningLevel>) -> Model {
    Model {
        provider: provider.into(),
        id: id.into(),
        endpoint: endpoint.into(),
        capabilities: Capabilities {
            text: true,
            structured_output: true,
            vision: true,
            ..Capabilities::default()
        },
        capability_source: CapabilitySource::ModelCatalog,
        reasoning_levels: levels,
    }
}

pub fn request(
    model: Model,
    text: &str,
    schema: Option<Value>,
    level: ReasoningLevel,
    max_tokens: u64,
) -> Request {
    Request {
        model,
        required: Capabilities {
            text: true,
            ..Capabilities::default()
        },
        input: vec![Input::Text { text: text.into() }],
        instructions: "K10".into(),
        questions: Vec::new(),
        response_schema: schema,
        max_tokens: Some(max_tokens),
        temperature: None,
        effort: None,
        reasoning: level,
    }
}

pub fn ok_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}},
        "required": ["ok"],
        "additionalProperties": false
    })
}

pub fn log_ctx(task: &str, provider: &str, model: &str, endpoint: &str) -> LogContext {
    LogContext {
        run_id: 1,
        timestamp: "2026-11-26T10:00:00Z".into(),
        task: task.into(),
        provider: provider.into(),
        model: model.into(),
        endpoint: endpoint.into(),
        url_id: None,
        definition_version: None,
        batch_id: None,
        redactions: 0,
    }
}

/// Pfad unterhalb von `tests/fixtures/ai`.
pub fn fixture(parts: &[&str]) -> PathBuf {
    let mut p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("ai");
    for part in parts {
        p = p.join(part);
    }
    p
}

/// Temporäres Verzeichnis, das beim Verlassen gelöscht wird.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Laufende Nummer: Die Systemuhr allein trennt gleichzeitig startende Tests nicht sicher (macOS: Mikrosekunden).
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!(
            "os-ai-k10-{tag}-{}-{nanos}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Neue Crawl-Datei im aktuellen Schema mit einem KI-Lauf (`ai_runs.id = 1`) und einer URL (`urls.id = 1`).
pub fn crawl_file(dir: &Path) -> os_store::CrawlFile {
    use os_model::time::{IdSource, SeededIds};
    let created = os_model::Timestamp::from_db("2026-11-26T08:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: SeededIds::new(10).new_crawl_id(created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "K10".into(),
        name: "q00".into(),
        config: os_model::CrawlConfig::default(),
        created_at: created,
    };
    let f = os_store::CrawlFile::create(
        &dir.join("k10.ospider"),
        &init,
        os_store::StorageProfile::Balanced,
    )
    .unwrap();
    f.conn()
        .execute_batch(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-26T08:00:00Z', '{}');
             INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length)
               VALUES(1,'https://ai.test:8080/','https://ai.test:8080/','https://ai.test:8080',1,'Done','Seed',0,21);",
        )
        .unwrap();
    f
}

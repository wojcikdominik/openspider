//! Hilfen der unabhängigen Abnahmetests M4-K10 (Nachträge „24.11.2026, Q00“ und „25.11.2026“ an AI-010 … AI-021):
//! ai-mock starten und steuern, Request-Protokoll lesen, Adapter gegen den Mock bauen, eigene Preis- und
//! Crawl-Datei-Hilfen und ein kleiner eigener HTTP-Server für Header-Formen, die der Mock nicht liefert.
//!
//! Erwartungswerte stehen in den Tests selbst und kommen nur aus `docs/spec/08-ai.md`. Alle Verbindungen gehen an
//! `127.0.0.1`; kein echter Anbieter, nur erfundene Schlüssel.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_ai::adapters::anthropic::{AnthropicAdapter, AnthropicConfig};
use os_ai::adapters::ollama::{OllamaAdapter, OllamaConfig};
use os_ai::adapters::openai_decisions::OpenAiDecisions;
use os_ai::adapters::openai_responses::{OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses};
use os_ai::adapters::openrouter::{OpenRouterAdapter, OpenRouterConfig, OpenRouterModel};
use os_ai::cost::{LogContext, PriceSource, Rates};
use os_ai::limits::{BackoffPolicy, Limiter, LimitsConfig, ProviderLimits, TokioClock};
use os_ai::provider::{Capabilities, CapabilitySource, Input, Model, ReasoningLevel, Request};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Erfundene Schlüssel (keine echten Schlüssel).
pub const ANTHROPIC_KEY: &str = "sk-ant-TEST-K10-ACCEPTANCE";
pub const OPENAI_KEY: &str = "sk-TEST-K10-ACCEPTANCE";
pub const OPENROUTER_KEY: &str = "sk-or-TEST-K10-ACCEPTANCE";

/// Pfade des ai-mock (08 Konventionen).
pub const MESSAGES: &str = "/anthropic/v1/messages";
pub const RESPONSES: &str = "/openai/v1/responses";
pub const DECISIONS: &str = "/openai/v1/decisions";
pub const COMPLETIONS: &str = "/openrouter/api/v1/chat/completions";
pub const OLLAMA_CHAT: &str = "/ollama/api/chat";

/// 1 × 1 Pixel PNG.
pub const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// Alle acht Werte der Reasoning-Stufe (AI-011, Nachtrag).
pub const ALL_LEVELS: [ReasoningLevel; 8] = [
    ReasoningLevel::ModelDefault,
    ReasoningLevel::Off,
    ReasoningLevel::Minimal,
    ReasoningLevel::Low,
    ReasoningLevel::Medium,
    ReasoningLevel::High,
    ReasoningLevel::XHigh,
    ReasoningLevel::Max,
];

/// Statuswerte laut AI-020 (Tabelle) einschließlich der Nachträge (`Error: Connection failed`, K4-15;
/// `Error: Output limit reached`, Q00). Zeilen mit Platzhaltern (`Timeout nach <n> s`, `<HTTP-Code> <Meldung>`) prüfen
/// die Tests über ihr Präfix.
pub const SPEC_STATUS_VALUES: [&str; 24] = [
    "Success",
    "Success (cached)",
    "Refusal",
    "Pending",
    "Pending (batch)",
    "Pending (confirmation)",
    "Pending (consent)",
    "Skipped (scope)",
    "Skipped (segment)",
    "Skipped (empty input)",
    "Skipped (policy)",
    "Skipped (no capable model)",
    "Skipped (image too large)",
    "Skipped (unsupported format)",
    "Skipped (no declared language)",
    "Budget exceeded",
    "Quota exceeded",
    "Error: Missing API key",
    "Error: Invalid output",
    "Error: Missing answer",
    "Error: Batch expired",
    "Error: Connection failed",
    "Error: Output limit reached",
    // Platzhalter: Präfix von `Error: Timeout nach <n> s`
    "Error: Timeout nach ",
];

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Eigene Fixture dieses Pakets unter `tests/fixtures/ai/k10-acceptance/`.
pub fn fixture_path(name: &str) -> PathBuf {
    repo_root()
        .join("tests/fixtures/ai/k10-acceptance")
        .join(name)
}

pub fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name)).expect("Fixture lesbar")
}

/// HTTP-Client ohne System-Proxy.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("HTTP-Client")
}

/// Laufender ai-mock (gemeinsame Antwortfragmente aus `tests/fixtures/ai/mock-answers.d`).
pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::fixtures().expect("ai-mock-Fragmente lesbar");
        let server = MockServer::start(mock.registrations())
            .await
            .expect("ai-mock startet");
        Self { mock, server }
    }

    pub fn addr(&self) -> SocketAddr {
        self.server.addr()
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr())
    }

    pub fn anthropic(&self) -> AnthropicAdapter {
        AnthropicAdapter::new(
            client(),
            AnthropicConfig::new(os_ai::adapters::anthropic::ApiKey::new(ANTHROPIC_KEY))
                .with_base_url(self.url("/anthropic")),
        )
    }

    pub fn openrouter(&self) -> OpenRouterAdapter {
        OpenRouterAdapter::new(
            client(),
            OpenRouterConfig::new(os_ai::adapters::openrouter::ApiKey::new(OPENROUTER_KEY))
                .with_base_url(self.url("/openrouter/api/v1")),
        )
    }

    pub fn openai_http(&self) -> OpenAiHttp {
        openai_http(&self.url("/openai/v1"))
    }

    pub fn responses(&self) -> OpenAiResponses {
        OpenAiResponses::new(self.openai_http())
    }

    pub fn decisions(&self) -> OpenAiDecisions {
        OpenAiDecisions::new(self.openai_http())
    }

    pub fn ollama(&self) -> OllamaAdapter {
        OllamaAdapter::new(
            client(),
            OllamaConfig::new().with_base_url(self.url("/ollama")),
        )
    }

    /// Globales Szenario (`POST /__mock/scenario`) für die nächsten `count` Anfragen eines Anbieters.
    pub async fn scenario(&self, provider: &str, scenario: &str, count: usize) {
        let response = client()
            .post(self.url("/__mock/scenario"))
            .header("content-type", "application/json")
            .body(json!({"provider": provider, "scenario": scenario, "count": count}).to_string())
            .send()
            .await
            .expect("Szenario-Steuerung erreichbar");
        assert!(response.status().is_success());
    }

    /// Alle protokollierten Anbieter-Requests (ohne Steueraufrufe).
    pub fn log(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| !r.path.starts_with("/__mock"))
            .collect()
    }

    /// JSON-Körper aller POST-Requests an `path` (ohne Query).
    pub fn bodies(&self, path: &str) -> Vec<Value> {
        self.log()
            .into_iter()
            .filter(|r| r.method == "POST" && r.path.split('?').next() == Some(path))
            .map(|r| serde_json::from_str(&r.body).expect("JSON-Körper"))
            .collect()
    }

    /// JSON-Körper aller POST-Requests an `path`, deren Körper `marker` enthält.
    pub fn bodies_with(&self, path: &str, marker: &str) -> Vec<Value> {
        self.bodies(path)
            .into_iter()
            .filter(|b| b.to_string().contains(marker))
            .collect()
    }

    /// Rohe GET-Antwort des Mocks als JSON.
    pub async fn get_json(&self, path: &str) -> Value {
        client()
            .get(self.url(path))
            .send()
            .await
            .expect("Mock erreichbar")
            .json()
            .await
            .expect("JSON")
    }
}

/// OpenAI-HTTP-Zugang zu einer Basis-URL (einschließlich `/v1`).
pub fn openai_http(base_url: &str) -> OpenAiHttp {
    OpenAiHttp::new(
        client(),
        OpenAiConfig::new(os_ai::adapters::openai_responses::ApiKey::new(OPENAI_KEY))
            .with_base_url(base_url),
        OpenAiAccount::new(),
    )
}

/// Begrenzung mit kurzen Wartezeiten (Regeln INTEG-005 und Schutzschalter unverändert: 10 Fehler, 5 Minuten; nur die
/// Backoff-Basis ist verkürzt, eine Anfrage gleichzeitig).
pub fn fast_limiter() -> Limiter {
    let mut config = LimitsConfig {
        backoff: BackoffPolicy {
            base: Duration::from_millis(2),
            factor: 2.0,
            cap: Duration::from_millis(20),
            jitter: 0.0,
        },
        ..LimitsConfig::default()
    };
    let limits = ProviderLimits {
        concurrent_requests: 1,
        timeout: Duration::from_secs(20),
    };
    config.default_provider = limits;
    for value in config.providers.values_mut() {
        *value = limits;
    }
    Limiter::new(config, Arc::new(TokioClock::new()))
}

/// Schema `{"ok": boolean}`.
pub fn ok_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}},
        "required": ["ok"],
        "additionalProperties": false
    })
}

/// Modell für einen Request.
pub fn model(provider: &str, id: &str, endpoint: &str, levels: Vec<ReasoningLevel>) -> Model {
    Model {
        provider: provider.into(),
        id: id.into(),
        endpoint: endpoint.into(),
        capabilities: Capabilities {
            text: true,
            structured_output: true,
            vision: true,
            batch: provider != "openrouter" && provider != "ollama",
            embeddings: false,
            decisions: false,
            prices_via_api: provider == "openrouter",
            local: provider == "ollama",
        },
        capability_source: match provider {
            "anthropic" => CapabilitySource::AnthropicModels,
            "openrouter" => CapabilitySource::OpenRouterModels,
            _ => CapabilitySource::ModelCatalog,
        },
        reasoning_levels: levels,
    }
}

/// Request mit einem Textteil.
pub fn request(
    model: Model,
    text: &str,
    schema: Option<Value>,
    reasoning: ReasoningLevel,
    max_tokens: Option<u64>,
) -> Request {
    Request {
        model,
        required: Capabilities {
            text: true,
            structured_output: schema.is_some(),
            ..Capabilities::default()
        },
        input: vec![Input::Text { text: text.into() }],
        instructions: "K10 acceptance system text".into(),
        questions: Vec::new(),
        response_schema: schema,
        max_tokens,
        temperature: None,
        effort: None,
        reasoning,
    }
}

/// Stufen eines OpenAI-Katalogmodells mit `reasoning_efforts = ["none", "low", "medium", "high"]`.
pub fn luna_levels() -> Vec<ReasoningLevel> {
    vec![
        ReasoningLevel::Off,
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
    ]
}

/// Modell-ID des Anthropic-Mock-Modells mit der Rolle `role` (`display_name` A–D, 08 Konventionen).
pub async fn anthropic_model(
    mock: &Mock,
    role: &str,
) -> os_ai::adapters::anthropic::AnthropicModel {
    mock.anthropic()
        .list_models()
        .await
        .expect("Modellliste lädt")
        .into_iter()
        .find(|m| m.display_name.as_deref() == Some(role))
        .unwrap_or_else(|| panic!("Mock-Modell {role} fehlt"))
}

/// Rollen der OpenRouter-Mock-Modellliste (08 Konventionen, Nachtrag Q00), aus den Feldern der Liste ermittelt.
pub struct OpenRouterRoles {
    /// `M-R1`: `reasoning` mit `none`, nicht `mandatory`.
    pub r1: OpenRouterModel,
    /// `M-R3`: ohne `reasoning`, mit Structured Output.
    pub r3: OpenRouterModel,
    /// `M-LC`: `pricing.overrides`.
    pub lc: OpenRouterModel,
    /// `M-IR`: `pricing.internal_reasoning`.
    pub ir: OpenRouterModel,
    /// Vision-Modell mit `pricing.request = 0.001` und `pricing.image = 0.002`.
    pub vision: OpenRouterModel,
    /// Rohe Einträge der Liste.
    pub raw: Vec<Value>,
}

pub async fn openrouter_roles(mock: &Mock) -> OpenRouterRoles {
    let list = mock.get_json("/openrouter/api/v1/models").await;
    let raw = list["data"].as_array().cloned().expect("data");
    let models: Vec<OpenRouterModel> = raw
        .iter()
        .map(|m| OpenRouterModel::from_json(m).expect("Modell lesbar"))
        .collect();
    let find = |what: &str, f: &dyn Fn(&OpenRouterModel) -> bool| -> OpenRouterModel {
        models
            .iter()
            .find(|m| f(m))
            .cloned()
            .unwrap_or_else(|| panic!("OpenRouter-Mock-Modell {what} fehlt"))
    };
    let has = |m: &OpenRouterModel, p: &str| m.supported_parameters.iter().any(|x| x == p);
    OpenRouterRoles {
        r1: find("M-R1", &|m| {
            has(m, "reasoning")
                && m.reasoning_efforts.iter().any(|e| e == "none")
                && !m.reasoning_mandatory
        }),
        r3: find("M-R3", &|m| !has(m, "reasoning") && m.structured_output()),
        lc: find("M-LC", &|m| !m.overrides.is_empty()),
        ir: find("M-IR", &|m| m.internal_reasoning_pico > 0),
        vision: find("Vision mit Gebühren", &|m| {
            m.request_pico == 1_000_000_000 && m.image_pico == 2_000_000_000
        }),
        raw,
    }
}

/// Rohe Listeneinträge eines OpenRouter-Modells.
pub fn raw_entry<'a>(roles: &'a OpenRouterRoles, id: &str) -> &'a Value {
    roles
        .raw
        .iter()
        .find(|m| m["id"] == id)
        .expect("Eintrag vorhanden")
}

/// USD-Text (z. B. `"0.000002"`) als µUSD × 10^6 = pUSD (exakte Dezimalumrechnung ohne Gleitkomma).
pub fn usd_to_pico(text: &str) -> u64 {
    scaled(text, 12)
}

/// Dezimaltext × 10^digits als Ganzzahl; nur exakt darstellbare Werte.
pub fn scaled(text: &str, digits: u32) -> u64 {
    let (int, frac) = text.split_once('.').unwrap_or((text, ""));
    assert!(
        frac.len() as u32 <= digits,
        "{text} nicht exakt mit {digits} Stellen"
    );
    let int: u64 = if int.is_empty() {
        0
    } else {
        int.parse().unwrap()
    };
    let mut frac_v: u64 = if frac.is_empty() {
        0
    } else {
        frac.parse().unwrap()
    };
    for _ in frac.len() as u32..digits {
        frac_v *= 10;
    }
    int * 10u64.pow(digits) + frac_v
}

fn num_text(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => panic!("keine Zahl: {other}"),
    }
}

/// Sätze für (Anbieter, Modell, Endpunkt) aus `tests/fixtures/ai/pricing-test.json` (08 Konventionen), eigener Loader.
pub fn fixture_rates(provider: &str, model: &str, endpoint: &str) -> Rates {
    let path = repo_root().join("tests/fixtures/ai/pricing-test.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let e = doc["prices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["provider"] == provider && e["model"] == model && e["endpoint"] == endpoint)
        .unwrap_or_else(|| panic!("kein Preis für {provider}/{model}/{endpoint}"));
    let per_million = |k: &str| e.get(k).map(|v| scaled(&num_text(v), 6)).unwrap_or(0);
    Rates {
        input_pico: per_million("input_per_million"),
        output_pico: per_million("output_per_million"),
        cache_read_pico: per_million("cache_read_per_million"),
        cache_write_pico: per_million("cache_write_per_million"),
        batch_discount_ppm: 0,
        long_context: None,
        regional_uplift_ppm: 0,
        per_request_pico: 0,
        per_image_pico: 0,
        image_token_estimate: 0,
        source: PriceSource::Table,
    }
}

/// `mock-claude` (1 / 5 USD je 1 Mio.).
pub fn claude_rates() -> Rates {
    fixture_rates("anthropic", "mock-claude", "messages")
}

/// Temporäres Verzeichnis ohne zusätzliche Abhängigkeit; wird beim Drop gelöscht.
pub struct TempDir(pub PathBuf);
impl TempDir {
    pub fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "os-ai-k10-acc-{tag}-{}-{nanos}",
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

/// Neue Crawl-Datei mit einem KI-Lauf (id 1).
pub fn crawl_file(dir: &Path) -> os_store::CrawlFile {
    use os_model::time::{IdSource, SeededIds};
    let created = os_model::Timestamp::from_db("2026-11-26T10:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: SeededIds::new(1010).new_crawl_id(created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "Abnahme".into(),
        name: "K10".into(),
        config: os_model::CrawlConfig::default(),
        created_at: created,
    };
    let f = os_store::CrawlFile::create(
        &dir.join("acc.ospider"),
        &init,
        os_store::StorageProfile::Balanced,
    )
    .unwrap();
    f.conn()
        .execute_batch(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-26T10:00:00Z', '{}');",
        )
        .unwrap();
    f
}

pub fn now() -> os_model::Timestamp {
    os_model::Timestamp::from_db("2026-11-26T10:05:00.000Z").unwrap()
}

pub fn ctx(task: &str, provider: &str, model: &str, endpoint: &str) -> LogContext {
    LogContext {
        run_id: 1,
        timestamp: "2026-11-26T10:01:00Z".into(),
        task: task.into(),
        provider: provider.into(),
        model: model.into(),
        endpoint: endpoint.into(),
        url_id: None,
        definition_version: Some("1".into()),
        batch_id: None,
        redactions: 0,
    }
}

pub fn usage(input: u64, output: u64, reasoning: Option<u64>) -> os_ai::provider::Usage {
    os_ai::provider::Usage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: reasoning,
    }
}

/// Ob ein JSON-Wert irgendwo (rekursiv) einen Schlüssel `key` enthält.
pub fn contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(k, v)| k == key || contains_key(v, key)),
        Value::Array(items) => items.iter().any(|v| contains_key(v, key)),
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Eigener kleiner HTTP-Testserver (nur 127.0.0.1): beantwortet jede Verbindung mit derselben festen Antwort.

/// Feste Antwort.
#[derive(Clone)]
pub struct Canned {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Canned {
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.to_string(),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

/// Laufender Testserver.
pub struct Stub {
    addr: SocketAddr,
    count: Arc<Mutex<usize>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Stub {
    pub async fn start(answer: Canned) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Port");
        let addr = listener.local_addr().expect("Adresse");
        let count = Arc::new(Mutex::new(0));
        let seen = count.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let answer = answer.clone();
                let seen = seen.clone();
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let header_end = loop {
                        let Ok(n) = socket.read(&mut chunk).await else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        buffer.extend_from_slice(&chunk[..n]);
                        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                            break pos + 4;
                        }
                    };
                    let head = String::from_utf8_lossy(&buffer[..header_end]).to_lowercase();
                    let length = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    while buffer.len() < header_end + length {
                        let Ok(n) = socket.read(&mut chunk).await else {
                            return;
                        };
                        if n == 0 {
                            break;
                        }
                        buffer.extend_from_slice(&chunk[..n]);
                    }
                    *seen.lock().unwrap() += 1;
                    let mut response = format!(
                        "HTTP/1.1 {} K10\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
                        answer.status,
                        answer.body.len()
                    );
                    for (name, value) in &answer.headers {
                        response.push_str(&format!("{name}: {value}\r\n"));
                    }
                    response.push_str("\r\n");
                    response.push_str(&answer.body);
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self { addr, count, task }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    pub fn requests(&self) -> usize {
        *self.count.lock().unwrap()
    }
}

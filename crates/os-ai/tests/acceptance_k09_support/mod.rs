//! Hilfen der unabhängigen Abnahmetests M4-K09 (Nachträge „24.11.2026, Q00“ an AI-001 … AI-006 und Konventionen):
//! ai-mock starten und steuern, Request-Protokoll lesen, Adapter gegen den Mock bauen, ein kleiner eigener HTTP-Server
//! für Antwortformen, die der Mock nicht liefert, und eine eigene Fassung der Regel aus AI-011 (Nachtrag).
//!
//! Alle Verbindungen gehen an `127.0.0.1`; kein echter Anbieter, nur erfundene Schlüssel.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_ai::adapters::anthropic::{AnthropicAdapter, AnthropicConfig};
use os_ai::adapters::ollama::{OllamaAdapter, OllamaConfig};
use os_ai::adapters::openai_decisions::OpenAiDecisions;
use os_ai::adapters::openai_responses::{OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses};
use os_ai::adapters::openrouter::{OpenRouterAdapter, OpenRouterConfig};
use os_ai::limits::{BackoffPolicy, Limiter, LimitsConfig, ProviderLimits, TokioClock};
use os_ai::provider::{Capabilities, CapabilitySource, Input, Model, ReasoningLevel, Request};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Erfundene Schlüssel (keine echten Schlüssel).
pub const ANTHROPIC_KEY: &str = "sk-ant-TEST-K09-ACCEPTANCE";
pub const OPENAI_KEY: &str = "sk-TEST-K09-ACCEPTANCE";
pub const OPENROUTER_KEY: &str = "sk-or-TEST-K09-ACCEPTANCE";

/// Marker der Mock-Reasoning-Inhalte (08 Konventionen, Nachtrag Q00), die nie in einem Ergebnis stehen dürfen.
pub const REASONING_MARKERS: [&str; 3] =
    ["MOCK-REASONING-TEXT", "MOCK-ENCRYPTED", "MOCK-SIGNATURE"];

/// Mock-Modelle A–D der Anthropic-Liste (AI-002) – IDs aus der Modellliste des Mocks, Rolle über `display_name`.
pub const MODEL_A: &str = "A";
pub const MODEL_B: &str = "B";
pub const MODEL_C: &str = "C";
pub const MODEL_D: &str = "D";

/// Alle acht Werte von [`ReasoningLevel`].
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

/// Eigene Fassung der Regel aus AI-011 (Nachtrag), nur aus dem Spec-Wortlaut: Stufen aufsteigend `off`, `minimal`,
/// `low`, `medium`, `high`, `xhigh`, `max`; `model-default` = kein Parameter. Nicht verfügbare Stufe → nächstniedrigere
/// verfügbare, gibt es keine niedrigere, die niedrigste verfügbare. Ohne verfügbare Stufe nur `model-default`.
pub fn expected_effective(
    requested: ReasoningLevel,
    available: &[ReasoningLevel],
) -> ReasoningLevel {
    const SCALE: [ReasoningLevel; 7] = [
        ReasoningLevel::Off,
        ReasoningLevel::Minimal,
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
        ReasoningLevel::XHigh,
        ReasoningLevel::Max,
    ];
    let pos = |l: ReasoningLevel| SCALE.iter().position(|x| *x == l);
    let Some(wanted) = pos(requested) else {
        return ReasoningLevel::ModelDefault;
    };
    let have: Vec<usize> = (0..SCALE.len())
        .filter(|i| available.contains(&SCALE[*i]))
        .collect();
    if have.is_empty() {
        return ReasoningLevel::ModelDefault;
    }
    if have.contains(&wanted) {
        return requested;
    }
    match have.iter().rev().find(|i| **i < wanted) {
        Some(i) => SCALE[*i],
        None => SCALE[have[0]],
    }
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

    /// Request-Protokoll über `GET /__mock/log` (wie in 08 Konventionen beschrieben), ohne Steueraufrufe.
    pub async fn http_log(&self) -> Vec<Value> {
        let log: Value = client()
            .get(self.url("/__mock/log"))
            .send()
            .await
            .expect("Log erreichbar")
            .json()
            .await
            .expect("Log ist JSON");
        log.as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| !r["path"].as_str().unwrap_or("").starts_with("/__mock"))
            .collect()
    }

    /// Alle protokollierten Anbieter-Requests (ohne Steueraufrufe).
    pub fn log(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| !r.path.starts_with("/__mock"))
            .collect()
    }

    /// JSON-Körper aller POST-Requests an `path` (ohne Query), z. B. `/anthropic/v1/messages`.
    pub fn bodies(&self, path: &str) -> Vec<Value> {
        self.log()
            .into_iter()
            .filter(|r| r.method == "POST" && r.path.split('?').next() == Some(path))
            .map(|r| serde_json::from_str(&r.body).expect("JSON-Körper"))
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

    /// Roher POST an den Mock: (Status, Header, Körper als JSON bzw. `Null`).
    pub async fn post(&self, path: &str, body: Value) -> (u16, reqwest::header::HeaderMap, Value) {
        let response = client()
            .post(self.url(path))
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("Mock erreichbar");
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.expect("Körper");
        (
            status,
            headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
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

/// Begrenzung mit kurzen Wartezeiten (Regeln INTEG-005 unverändert, nur die Backoff-Basis ist verkürzt).
pub fn fast_limiter() -> Limiter {
    let mut config = LimitsConfig {
        backoff: BackoffPolicy {
            base: Duration::from_millis(5),
            factor: 2.0,
            cap: Duration::from_millis(40),
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

/// Schema `{"ok": boolean}` (Contract-Aufgabe aus AI-001).
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
            batch: true,
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
        instructions: "K09 acceptance system text".into(),
        questions: Vec::new(),
        response_schema: schema,
        max_tokens,
        temperature: None,
        effort: None,
        reasoning,
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

/// Ob ein Text einen der Reasoning-Marker enthält.
pub fn has_reasoning_marker(text: &str) -> bool {
    REASONING_MARKERS.iter().any(|m| text.contains(m))
}

/// Kalendertage seit 1970-01-01 für ein Datum `YYYY-MM-DD` (proleptischer gregorianischer Kalender).
pub fn days_from_civil(date: &str) -> i64 {
    let parts: Vec<i64> = date[..10]
        .split('-')
        .map(|p| p.parse().expect("Datum"))
        .collect();
    let (y, m, d) = (parts[0], parts[1], parts[2]);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Heute (UTC) als Tage seit 1970-01-01.
pub fn today_days() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Uhr")
        .as_secs();
    i64::try_from(secs / 86_400).expect("Tage")
}

// ---------------------------------------------------------------------------------------------------------------
// Eigener kleiner HTTP-Testserver (nur 127.0.0.1): beantwortet jede Verbindung mit derselben festen Antwort und
// zählt die Requests.

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
    bodies: Arc<Mutex<Vec<String>>>,
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
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let seen = bodies.clone();
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
                    let end = buffer.len().min(header_end + length);
                    seen.lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&buffer[header_end..end]).into_owned());
                    let mut response = format!(
                        "HTTP/1.1 {} K09\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
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
        Self { addr, bodies, task }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// Körper der empfangenen Requests.
    pub fn bodies(&self) -> Vec<String> {
        self.bodies.lock().unwrap().clone()
    }
}

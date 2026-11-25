//! Hilfen der unabhängigen Abnahmetests M4-K01 (AI-002, AI-005): ai-mock starten, Request-Protokoll lesen, Testuhr,
//! Begrenzung mit Testuhr und ein kleiner lokaler Testserver für Antworten, die der ai-mock nicht liefert.
//!
//! Alle Verbindungen gehen an `127.0.0.1`; es wird nie ein echter Anbieter angesprochen. Schlüssel sind erfundene
//! Zeichenketten.
#![allow(dead_code)]

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_ai::limits::{
    AttemptFailure, Execution, FixedJitter, LimitClock, Limiter, LimitsConfig, ProviderLimits,
    RequestSpec,
};
use os_ai::provider::{Capabilities, CapabilitySource, Input, Model, Request};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Erfundener Anthropic-Testschlüssel aus AI-002 (Volltextsuche nach diesem Wert).
pub const ANTHROPIC_TEST_KEY: &str = "sk-ant-TEST-AI-002";
/// Erfundener OpenRouter-Testschlüssel.
pub const OPENROUTER_TEST_KEY: &str = "sk-or-TEST-AI-005";

/// Request-Timeout der Tests. Die Testuhr lässt Wartezeiten dieser Länge nie virtuell verstreichen, damit nur echte
/// Hänger (nach [`SAFETY_TIMEOUT`] Echtzeit) als Timeout enden.
pub const TEST_TIMEOUT: Duration = Duration::from_secs(900);
/// Echtzeit-Notbremse für hängende Versuche.
pub const SAFETY_TIMEOUT: Duration = Duration::from_secs(20);
/// Feste Wanduhr des Testbeginns (2026-11-24T10:00:00Z).
pub const WALL0: i64 = 1_795_514_400_000;

/// Testuhr (08 Konventionen „Testuhr“): Wartezeiten unterhalb des Request-Timeouts vergehen sofort und virtuell; sie
/// werden aufgezeichnet.
#[derive(Default)]
pub struct TestClock {
    now: Mutex<Duration>,
    sleeps: Mutex<Vec<Duration>>,
}

impl TestClock {
    /// Alle virtuell vergangenen Wartezeiten (Backoff, `Retry-After`, Grenzen) in Reihenfolge.
    pub fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().unwrap().clone()
    }
}

impl LimitClock for TestClock {
    fn now(&self) -> Duration {
        *self.now.lock().unwrap()
    }

    fn wall_unix_millis(&self) -> i64 {
        WALL0 + i64::try_from(self.now().as_millis()).unwrap()
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        if duration >= TEST_TIMEOUT {
            return Box::pin(tokio::time::sleep(SAFETY_TIMEOUT));
        }
        *self.now.lock().unwrap() += duration;
        self.sleeps.lock().unwrap().push(duration);
        Box::pin(async {})
    }
}

/// Begrenzung mit Standardwerten der Spec (5 Wiederholungen, Backoff INTEG-005, Schutzschalter an), `concurrent`
/// gleichzeitigen Anfragen je Anbieter und Schutzschalter-Schwelle `breaker_threshold`; Jitter fest auf Mitte.
pub fn new_limiter(concurrent: u32, breaker_threshold: u32) -> (Arc<Limiter>, Arc<TestClock>) {
    let mut config = LimitsConfig::default();
    let provider = ProviderLimits {
        concurrent_requests: concurrent,
        timeout: TEST_TIMEOUT,
    };
    config.default_provider = provider;
    for limits in config.providers.values_mut() {
        *limits = provider;
    }
    config.breaker_threshold = breaker_threshold;
    let clock = Arc::new(TestClock::default());
    let limiter = Arc::new(Limiter::with_jitter(
        config,
        clock.clone(),
        Arc::new(FixedJitter(0.5)),
    ));
    (limiter, clock)
}

/// Führt einen Eintrag mit allen Versuchen über die Begrenzung aus.
pub async fn run_entry<T, F, Fut>(
    limiter: &Limiter,
    provider: &str,
    model: &str,
    attempt: F,
) -> Execution<T>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, AttemptFailure>>,
{
    let spec = RequestSpec::new(provider, model, 100);
    limiter.run(&spec, attempt).await
}

/// Laufender ai-mock mit Adresse.
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

    /// Basis-URL für den Anthropic-Adapter (ohne `/v1`).
    pub fn anthropic_base(&self) -> String {
        format!("http://{}/anthropic", self.addr())
    }

    /// Basis-URL für den OpenRouter-Adapter (einschließlich `/api/v1`).
    pub fn openrouter_base(&self) -> String {
        format!("http://{}/openrouter/api/v1", self.addr())
    }

    /// Globales Szenario für die nächsten `count` Anfragen eines Anbieters.
    pub async fn scenario(&self, provider: &str, scenario: &str, count: usize) {
        let response = client()
            .post(format!("http://{}/__mock/scenario", self.addr()))
            .header("content-type", "application/json")
            .body(json!({"provider": provider, "scenario": scenario, "count": count}).to_string())
            .send()
            .await
            .expect("Szenario-Steuerung erreichbar");
        assert!(response.status().is_success());
    }

    /// Alle protokollierten Requests ohne Steueraufrufe.
    pub fn log(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| !r.path.starts_with("/__mock"))
            .collect()
    }

    /// Protokollierte Requests mit Methode und Pfad (ohne Query).
    pub fn requests_to(&self, method: &str, path: &str) -> Vec<RequestRecord> {
        self.log()
            .into_iter()
            .filter(|r| r.method == method && r.path.split('?').next() == Some(path))
            .collect()
    }

    /// JSON-Körper der Requests mit Methode und Pfad.
    pub fn bodies_to(&self, method: &str, path: &str) -> Vec<Value> {
        self.requests_to(method, path)
            .iter()
            .map(|r| serde_json::from_str(&r.body).expect("JSON-Körper"))
            .collect()
    }
}

/// HTTP-Client ohne System-Proxy (Verbindungen nur zu `127.0.0.1`).
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("HTTP-Client")
}

/// Schema `{"ok": boolean}` im strikten Stil (AI-021).
pub fn ok_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}},
        "required": ["ok"],
        "additionalProperties": false
    })
}

/// Fähigkeiten einer reinen Textaufgabe.
pub fn text_caps() -> Capabilities {
    Capabilities {
        text: true,
        ..Capabilities::default()
    }
}

/// Modellbeschreibung für einen Request (die Adapter prüfen selbst keine Fähigkeiten).
pub fn model(provider: &str, id: &str, endpoint: &str, source: CapabilitySource) -> Model {
    Model {
        provider: provider.into(),
        id: id.into(),
        endpoint: endpoint.into(),
        capabilities: Capabilities {
            text: true,
            structured_output: true,
            vision: true,
            batch: true,
            ..Capabilities::default()
        },
        capability_source: source,
        reasoning_levels: Vec::new(),
    }
}

/// Request mit einem Textteil.
pub fn request(model: Model, text: &str, schema: Option<Value>) -> Request {
    Request {
        model,
        required: text_caps(),
        input: vec![Input::Text { text: text.into() }],
        instructions: "K01 acceptance system text".into(),
        questions: Vec::new(),
        response_schema: schema,
        max_tokens: Some(256),
        temperature: None,
        effort: None,
        reasoning: Default::default(),
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

/// Standard-Base64-Dekodierung (RFC 4648) für die Prüfung gesendeter Bilder.
pub fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        let value = ALPHABET
            .iter()
            .position(|c| *c == byte)
            .unwrap_or_else(|| panic!("kein Base64-Zeichen: {}", char::from(byte)));
        buffer = (buffer << 6) | u32::try_from(value).unwrap();
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xFF).unwrap());
        }
    }
    out
}

/// Pfad einer Fixture-Datei unter `tests/fixtures/`.
pub fn fixture(path: &str) -> std::path::PathBuf {
    let mut root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    root.push("tests");
    root.push("fixtures");
    for part in path.split('/') {
        root.push(part);
    }
    root
}

// ---------------------------------------------------------------------------------------------------------------
// Kleiner lokaler Testserver (nur Standardbibliothek/tokio) für Antworten, die der ai-mock nicht liefert.

/// Ein empfangener Request.
#[derive(Debug, Clone)]
pub struct StubRequest {
    pub method: String,
    pub path: String,
    /// Header-Namen in Kleinbuchstaben.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl StubRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("JSON-Körper")
    }
}

/// Antwort des Testservers.
#[derive(Debug, Clone)]
pub struct StubResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl StubResponse {
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

type Responder = dyn Fn(&StubRequest, usize) -> StubResponse + Send + Sync;

/// Laufender Testserver auf `127.0.0.1`.
pub struct Stub {
    pub addr: SocketAddr,
    requests: Arc<Mutex<Vec<StubRequest>>>,
}

impl Stub {
    /// Startet den Server; `responder` erhält den Request und die Zahl der davor empfangenen Requests.
    pub async fn start(
        responder: impl Fn(&StubRequest, usize) -> StubResponse + Send + Sync + 'static,
    ) -> Self {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("Testserver bindet");
        let addr = listener.local_addr().unwrap();
        let requests: Arc<Mutex<Vec<StubRequest>>> = Arc::default();
        let responder: Arc<Responder> = Arc::new(responder);
        let log = requests.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let log = log.clone();
                let responder = responder.clone();
                tokio::spawn(async move {
                    let _ = serve(stream, log, responder).await;
                });
            }
        });
        Self { addr, requests }
    }

    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn requests(&self) -> Vec<StubRequest> {
        self.requests.lock().unwrap().clone()
    }
}

async fn serve(
    mut stream: tokio::net::TcpStream,
    log: Arc<Mutex<Vec<StubRequest>>>,
    responder: Arc<Responder>,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let method = first.next().unwrap_or("").to_string();
    let path = first.next().unwrap_or("").to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = buffer[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let request = StubRequest {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    let index = {
        let mut log = log.lock().unwrap();
        log.push(request.clone());
        log.len() - 1
    };
    let response = responder(&request, index);
    let mut out = format!(
        "HTTP/1.1 {} Stub\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
        response.status,
        response.body.len()
    );
    for (name, value) in &response.headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(&response.body);
    stream.write_all(out.as_bytes()).await?;
    stream.shutdown().await
}

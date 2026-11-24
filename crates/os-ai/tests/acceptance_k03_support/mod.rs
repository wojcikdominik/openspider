//! Hilfen der unabhängigen Abnahmetests M4-K03 (AI-006, AI-008, INTEG-005 [M4]): ai-mock starten, Request-Protokoll
//! lesen, Testuhr, Begrenzung mit Testuhr und ein kleiner lokaler Testserver für Antworten und Messungen, die der
//! ai-mock nicht liefert (nicht normalisierte Vektoren, vertauschte Reihenfolge, gleichzeitig offene Anfragen).
//!
//! Alle Verbindungen gehen an `127.0.0.1`; es wird nie ein echter Anbieter und kein echter Ollama-Server angesprochen.
//! Schlüssel sind erfundene Zeichenketten.
#![allow(dead_code)]

use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use os_ai::limits::{FixedJitter, LimitClock, Limiter, LimitsConfig};
use os_ai::provider::{Capabilities, CapabilitySource, Input, Model, Request};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Erfundener Testschlüssel (OpenAI/OpenRouter); der Mock prüft ihn nicht.
pub const TEST_KEY: &str = "sk-TEST-K03-ACCEPTANCE";

/// Wartezeiten ab dieser Länge sind Request-Timeouts (INTEG-179: 120 s bzw. 300 s); die Testuhr lässt sie nie virtuell
/// verstreichen, sondern wartet höchstens [`SAFETY_TIMEOUT`] Echtzeit (Notbremse gegen echte Hänger). Backoff
/// (höchstens 60 s, INTEG-005) und `Retry-After` vergehen sofort und virtuell.
pub const TIMEOUT_THRESHOLD: Duration = Duration::from_secs(100);
/// Echtzeit-Notbremse.
pub const SAFETY_TIMEOUT: Duration = Duration::from_secs(20);

/// Testuhr (08 Konventionen „Testuhr“): kurze Wartezeiten vergehen virtuell und werden aufgezeichnet.
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
        1_795_514_400_000 + i64::try_from(self.now().as_millis()).unwrap()
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        if duration >= TIMEOUT_THRESHOLD {
            return Box::pin(tokio::time::sleep(SAFETY_TIMEOUT));
        }
        *self.now.lock().unwrap() += duration;
        self.sleeps.lock().unwrap().push(duration);
        Box::pin(async {})
    }
}

/// Begrenzung mit den Standardwerten der App-Einstellungen (Ollama: 1 gleichzeitige Anfrage, 300 s Timeout;
/// 5 Wiederholungen, Backoff INTEG-005), Testuhr und festem Jitter.
pub fn default_limiter() -> (Arc<Limiter>, Arc<TestClock>) {
    limiter_with(LimitsConfig::default())
}

/// Begrenzung mit eigener Konfiguration.
pub fn limiter_with(config: LimitsConfig) -> (Arc<Limiter>, Arc<TestClock>) {
    let clock = Arc::new(TestClock::default());
    let limiter = Arc::new(Limiter::with_jitter(
        config,
        clock.clone(),
        Arc::new(FixedJitter(0.5)),
    ));
    (limiter, clock)
}

/// Laufender ai-mock mit Adresse.
pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    /// ai-mock mit den Fragmenten des Projekts.
    pub async fn start() -> Self {
        Self::with(AiMock::fixtures().expect("ai-mock-Fragmente lesbar")).await
    }

    /// ai-mock nur mit den eigenen Fragmenten dieser Abnahme (`tests/fixtures/ai/k03-acceptance/mock-answers.d`).
    pub async fn start_with_own_fragments() -> Self {
        let dir = fixture("ai/k03-acceptance/mock-answers.d");
        Self::with(AiMock::from_fragments(dir).expect("eigene Fragmente lesbar")).await
    }

    async fn with(mock: AiMock) -> Self {
        let server = MockServer::start(mock.registrations())
            .await
            .expect("ai-mock startet");
        Self { mock, server }
    }

    pub fn addr(&self) -> SocketAddr {
        self.server.addr()
    }

    /// Server-URL für den Ollama-Adapter (ohne `/api`).
    pub fn ollama_base(&self) -> String {
        format!("http://{}/ollama", self.addr())
    }

    /// Basis-URL für OpenAI (einschließlich `/v1`).
    pub fn openai_base(&self) -> String {
        format!("http://{}/openai/v1", self.addr())
    }

    /// Basis-URL für OpenRouter (einschließlich `/api/v1`).
    pub fn openrouter_base(&self) -> String {
        format!("http://{}/openrouter/api/v1", self.addr())
    }

    /// Basis-URL für Anthropic (ohne `/v1`).
    pub fn anthropic_base(&self) -> String {
        format!("http://{}/anthropic", self.addr())
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

/// Modellbeschreibung eines Ollama-Modells.
pub fn ollama_model(id: &str) -> Model {
    Model {
        provider: "ollama".into(),
        id: id.into(),
        endpoint: "chat-completions".into(),
        capabilities: Capabilities {
            text: true,
            structured_output: true,
            embeddings: true,
            local: true,
            ..Capabilities::default()
        },
        capability_source: CapabilitySource::ModelCatalog,
    }
}

/// Request mit einem Textteil.
pub fn request(model: Model, text: &str, schema: Option<Value>) -> Request {
    Request {
        model,
        required: Capabilities {
            text: true,
            structured_output: schema.is_some(),
            ..Capabilities::default()
        },
        input: vec![Input::Text { text: text.into() }],
        instructions: "K03 acceptance system text".into(),
        questions: Vec::new(),
        response_schema: schema,
        max_tokens: Some(128),
        temperature: None,
        effort: None,
    }
}

/// Pfad unter `tests/fixtures/`.
pub fn fixture(path: &str) -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    root.push("tests");
    root.push("fixtures");
    for part in path.split('/') {
        root.push(part);
    }
    root
}

/// Eine freie Adresse auf `127.0.0.1`, auf der danach nichts lauscht („Server nicht erreichbar“).
pub fn closed_port() -> SocketAddr {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("Port reservieren");
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

/// L2-Norm in `f64`.
pub fn norm(v: &[f32]) -> f64 {
    v.iter()
        .map(|x| f64::from(*x) * f64::from(*x))
        .sum::<f64>()
        .sqrt()
}

// ---------------------------------------------------------------------------------------------------------------
// Kleiner lokaler Testserver (nur tokio) mit Zeitstempeln und Zähler offener Anfragen.

/// Ein empfangener Request.
#[derive(Debug, Clone)]
pub struct StubRequest {
    pub method: String,
    pub path: String,
    pub body: String,
    /// Eingang des Requests (vollständig gelesen).
    pub received: Instant,
}

impl StubRequest {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("JSON-Körper")
    }
}

/// Antwort des Testservers.
#[derive(Debug, Clone)]
pub struct StubResponse {
    pub status: u16,
    pub body: String,
    /// Echtzeit-Verzögerung vor dem Senden der Antwort.
    pub delay: Duration,
}

impl StubResponse {
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            body: body.to_string(),
            delay: Duration::ZERO,
        }
    }

    pub fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

type Responder = dyn Fn(&StubRequest, usize) -> StubResponse + Send + Sync;

/// Offenes Intervall einer Anfrage (Eingang bis Antwort gesendet).
#[derive(Debug, Clone, Copy)]
pub struct Interval {
    pub start: Instant,
    pub end: Instant,
}

#[derive(Default)]
struct StubState {
    requests: Mutex<Vec<StubRequest>>,
    intervals: Mutex<Vec<Interval>>,
    open: AtomicUsize,
    max_open: AtomicUsize,
}

/// Laufender Testserver auf `127.0.0.1`.
pub struct Stub {
    pub addr: SocketAddr,
    state: Arc<StubState>,
}

impl Stub {
    /// Startet den Server auf einem freien Port.
    pub async fn start(
        responder: impl Fn(&StubRequest, usize) -> StubResponse + Send + Sync + 'static,
    ) -> Self {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("Testserver bindet");
        Self::serve_on(listener, responder)
    }

    /// Startet den Server auf einer bestimmten Adresse (z. B. einem zuvor nicht erreichbaren Port).
    pub async fn start_on(
        addr: SocketAddr,
        responder: impl Fn(&StubRequest, usize) -> StubResponse + Send + Sync + 'static,
    ) -> Self {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("Testserver bindet auf dem gewünschten Port");
        Self::serve_on(listener, responder)
    }

    fn serve_on(
        listener: tokio::net::TcpListener,
        responder: impl Fn(&StubRequest, usize) -> StubResponse + Send + Sync + 'static,
    ) -> Self {
        let addr = listener.local_addr().unwrap();
        let state: Arc<StubState> = Arc::default();
        let responder: Arc<Responder> = Arc::new(responder);
        let shared = state.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let shared = shared.clone();
                let responder = responder.clone();
                tokio::spawn(async move {
                    let _ = serve(stream, shared, responder).await;
                });
            }
        });
        Self { addr, state }
    }

    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn requests(&self) -> Vec<StubRequest> {
        self.state.requests.lock().unwrap().clone()
    }

    /// Höchstzahl gleichzeitig offener Anfragen.
    pub fn max_open(&self) -> usize {
        self.state.max_open.load(Ordering::SeqCst)
    }

    /// Offene Intervalle aller beantworteten Anfragen.
    pub fn intervals(&self) -> Vec<Interval> {
        self.state.intervals.lock().unwrap().clone()
    }
}

async fn serve(
    mut stream: tokio::net::TcpStream,
    state: Arc<StubState>,
    responder: Arc<Responder>,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
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
    let length: usize = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0);
    let mut body = buffer[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let start = Instant::now();
    let open = state.open.fetch_add(1, Ordering::SeqCst) + 1;
    state.max_open.fetch_max(open, Ordering::SeqCst);
    let request = StubRequest {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
        received: start,
    };
    let index = {
        let mut log = state.requests.lock().unwrap();
        log.push(request.clone());
        log.len() - 1
    };
    let response = responder(&request, index);
    if !response.delay.is_zero() {
        tokio::time::sleep(response.delay).await;
    }
    let mut out = format!(
        "HTTP/1.1 {} Stub\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        response.status,
        response.body.len()
    );
    out.push_str(&response.body);
    let written = stream.write_all(out.as_bytes()).await;
    let _ = stream.flush().await;
    // Intervall endet, sobald die Antwort vollständig geschrieben ist.
    state.intervals.lock().unwrap().push(Interval {
        start,
        end: Instant::now(),
    });
    state.open.fetch_sub(1, Ordering::SeqCst);
    written?;
    stream.shutdown().await
}

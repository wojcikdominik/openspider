//! Hilfen der unabhängigen Abnahmetests M4-K02 (AI-003, AI-004): ai-mock mit eigenem Antwortfragment, Client mit
//! Host-Überschreibung `*.mock.test → 127.0.0.1`, kleiner eigener HTTP-Testserver für Antwortformen, die der Mock
//! nicht liefert, und Bauhilfen. Alle Schlüssel sind erfundene Zeichenketten; kein echter Anbieter wird angesprochen.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_ai::adapters::openai_responses::{ApiKey, OpenAiAccount, OpenAiConfig, OpenAiHttp};
use os_ai::limits::{BackoffPolicy, Limiter, LimitsConfig, ProviderLimits, TokioClock};
use os_testsite::mocks::{
    ai::{AiMock, RequestRecord},
    MockServer,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Erfundener Testschlüssel (kein echter Schlüssel).
pub const TEST_KEY: &str = "sk-k02-acceptance-invented-key";
pub const OPENAI_HOST: &str = "openai.mock.test";
pub const OPENAI_EU_HOST: &str = "eu.openai.mock.test";

/// Verzeichnis der Fixtures dieses Testpakets.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("ai")
        .join("k02-acceptance")
}

pub fn read_fixture_json(name: &str) -> Value {
    let path = fixture_dir().join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap()
}

/// ai-mock mit dem eigenen Antwortfragment dieses Pakets.
pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::from_fragments(fixture_dir().join("mock-answers.d")).unwrap();
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Self { mock, server }
    }

    pub fn addr(&self) -> SocketAddr {
        self.server.addr()
    }

    /// Basis-URL einschließlich `/v1` am globalen Mock-Host.
    pub fn base_url(&self) -> String {
        self.base_url_for(OPENAI_HOST)
    }

    pub fn base_url_for(&self, host: &str) -> String {
        format!("http://{host}:{}/openai/v1", self.addr().port())
    }

    /// Client mit Host-Überschreibung `*.mock.test → 127.0.0.1` und ohne System-Proxy.
    pub fn client(&self) -> reqwest::Client {
        client_for(self.addr())
    }

    /// OpenAI-HTTP-Zugang zum Mock (globaler Host, eigenes Konto).
    pub fn http(&self) -> OpenAiHttp {
        OpenAiHttp::new(
            self.client(),
            OpenAiConfig::new(ApiKey::new(TEST_KEY)).with_base_url(self.base_url()),
            OpenAiAccount::new(),
        )
    }

    /// Globales Szenario für die nächsten `count` Anfragen an OpenAI.
    pub async fn scenario(&self, scenario: &str, count: usize) {
        let r = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!("http://{}/__mock/scenario", self.addr()))
            .header("content-type", "application/json")
            .body(json!({"provider": "openai", "scenario": scenario, "count": count}).to_string())
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success());
    }

    /// Protokollierte Anfragen an einen OpenAI-Endpunkt (`responses`, `decisions`, `models`).
    pub fn requests_to(&self, endpoint: &str) -> Vec<RequestRecord> {
        let path = format!("/openai/v1/{endpoint}");
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.path == path)
            .collect()
    }

    pub fn bodies_to(&self, endpoint: &str) -> Vec<Value> {
        self.requests_to(endpoint)
            .iter()
            .map(|r| serde_json::from_str(&r.body).unwrap())
            .collect()
    }
}

pub fn client_for(addr: SocketAddr) -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .resolve(OPENAI_HOST, addr)
        .resolve(OPENAI_EU_HOST, addr)
        .build()
        .unwrap()
}

/// Limiter mit kurzen, deterministischen Wartezeiten (Regeln INTEG-005 unverändert: 5 Wiederholungen, Backoff
/// exponentiell; nur die Basis ist verkürzt, damit der Test nicht real Sekunden wartet).
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
    config.providers.insert(
        "openai".into(),
        ProviderLimits {
            concurrent_requests: 1,
            timeout: Duration::from_secs(20),
        },
    );
    Limiter::new(config, Arc::new(TokioClock::new()))
}

/// Limiter mit kurzem Timeout und ohne Wiederholung.
pub fn timeout_limiter(timeout: Duration) -> Limiter {
    let mut config = LimitsConfig {
        max_retries: 0,
        ..LimitsConfig::default()
    };
    config.providers.insert(
        "openai".into(),
        ProviderLimits {
            concurrent_requests: 1,
            timeout,
        },
    );
    Limiter::new(config, Arc::new(TokioClock::new()))
}

// ---------------------------------------------------------------------------------------------------------------
// Eigener kleiner HTTP-Testserver (nur 127.0.0.1, eine Antwort je Verbindung, `Connection: close`).

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    /// Header-Namen klein geschrieben.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == &name.to_ascii_lowercase())
            .map(|(_, v)| v.as_str())
    }
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

pub struct Canned {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Canned {
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.to_string().into_bytes(),
        }
    }
    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), content_type.into())],
            body,
        }
    }
}

type Handler = Arc<dyn Fn(&Recorded) -> Canned + Send + Sync>;

pub struct LocalServer {
    pub addr: SocketAddr,
    log: Arc<Mutex<Vec<Recorded>>>,
}

impl LocalServer {
    pub async fn start(handler: impl Fn(&Recorded) -> Canned + Send + Sync + 'static) -> Self {
        let handler: Handler = Arc::new(handler);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let handler = handler.clone();
                let log = log2.clone();
                tokio::spawn(async move {
                    let Some(rec) = read_request(&mut stream).await else {
                        return;
                    };
                    log.lock().unwrap().push(rec.clone());
                    let canned = handler(&rec);
                    let mut head = format!(
                        "HTTP/1.1 {} X\r\nconnection: close\r\ncontent-length: {}\r\n",
                        canned.status,
                        canned.body.len()
                    );
                    for (k, v) in &canned.headers {
                        head.push_str(&format!("{k}: {v}\r\n"));
                    }
                    head.push_str("\r\n");
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&canned.body).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Self { addr, log }
    }

    /// Basis-URL im Stil der OpenAI-API (`…/v1`).
    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.addr)
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.log.lock().unwrap().clone()
    }

    pub fn requests_to(&self, path: &str) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.path == path)
            .collect()
    }

    /// OpenAI-HTTP-Zugang zu diesem Server.
    pub fn http(&self) -> OpenAiHttp {
        OpenAiHttp::new(
            reqwest::Client::builder().no_proxy().build().unwrap(),
            OpenAiConfig::new(ApiKey::new(TEST_KEY)).with_base_url(self.base_url()),
            OpenAiAccount::new(),
        )
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<Recorded> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let path = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Some(Recorded {
        method,
        path,
        headers,
        body,
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Bilder und Base64 (eigene Umsetzung, unabhängig vom Produktionscode).

/// Kleines gültiges PNG (1 × 1 Pixel).
pub fn tiny_png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

/// Minimaler JPEG-Kopf (SOI + APP0), genügt der Formaterkennung.
pub fn tiny_jpeg() -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    v.extend_from_slice(b"JFIF\0");
    v.extend_from_slice(&[1, 1, 0, 0, 1, 0, 1, 0, 0, 0xFF, 0xD9]);
    v
}

/// PNG-Signatur gefolgt von Füllbytes bis zur Gesamtgröße `len`.
pub fn png_of_size(len: usize) -> Vec<u8> {
    let mut v = tiny_png();
    v.resize(len.max(v.len()), 0);
    v
}

pub fn b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Alle Werte von `image_url` (rekursiv) in einem JSON-Wert.
pub fn image_urls(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    if k == "image_url" {
                        match x {
                            Value::String(s) => out.push(s.clone()),
                            Value::Object(inner) => {
                                if let Some(Value::String(s)) = inner.get("url") {
                                    out.push(s.clone());
                                }
                            }
                            _ => out.push(x.to_string()),
                        }
                    }
                    walk(x, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(v, &mut out);
    out
}

/// Ob irgendwo im JSON-Wert ein Schlüssel `key` vorkommt.
pub fn has_key(v: &Value, key: &str) -> bool {
    match v {
        Value::Object(o) => o.iter().any(|(k, x)| k == key || has_key(x, key)),
        Value::Array(a) => a.iter().any(|x| has_key(x, key)),
        _ => false,
    }
}

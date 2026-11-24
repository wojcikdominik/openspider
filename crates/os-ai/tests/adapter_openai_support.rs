//! Gemeinsame Hilfen der Adaptertests M4-K02 gegen ai-mock (`os-testsite`). Wird von `adapter_openai_*.rs` per
//! `#[path]` eingebunden; als eigenes Testprogramm enthält die Datei keine Tests.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use os_ai::adapters::openai_responses::{ApiKey, OpenAiAccount, OpenAiConfig, OpenAiHttp};
use os_ai::limits::{
    BackoffPolicy, FixedJitter, Limiter, LimitsConfig, ProviderLimits, TokioClock,
};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use os_testsite::{SiteOptions, TestSite};
use serde_json::{json, Value};

/// Erfundener Testschlüssel; darf in keinem Log und keiner Debug-Ausgabe erscheinen.
pub const KEY: &str = "sk-TEST-K02-fixture-key";
pub const GLOBAL_HOST: &str = "openai.mock.test";
pub const EU_HOST: &str = "eu.openai.mock.test";

/// Laufender ai-mock auf `127.0.0.1` (ephemerer Port).
pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::fixtures().expect("ai-mock fragments load");
        let server = MockServer::start(mock.registrations())
            .await
            .expect("ai-mock starts");
        Self { mock, server }
    }

    pub fn addr(&self) -> SocketAddr {
        self.server.addr()
    }

    /// Basis-URL des OpenAI-Mocks unter `host` (Präfix `/openai/v1`).
    pub fn base(&self, host: &str) -> String {
        format!("http://{host}:{}/openai/v1", self.addr().port())
    }

    /// HTTP-Client mit Host-Überschreibung `*.mock.test → 127.0.0.1`, ohne System-Proxy.
    pub fn client(&self) -> reqwest::Client {
        client_for(&[GLOBAL_HOST, EU_HOST], self.addr())
    }

    pub fn config(&self, host: &str) -> OpenAiConfig {
        OpenAiConfig::new(ApiKey::new(KEY)).with_base_url(self.base(host))
    }

    /// Zugang mit eigenem Kontostand.
    pub fn http(&self, host: &str) -> OpenAiHttp {
        self.http_with(host, OpenAiAccount::new())
    }

    pub fn http_with(&self, host: &str, account: Arc<OpenAiAccount>) -> OpenAiHttp {
        OpenAiHttp::new(self.client(), self.config(host), account)
    }

    /// Alle Requests an den OpenAI-Mock (ohne Steueraufrufe).
    pub fn openai_requests(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.path.starts_with("/openai/"))
            .collect()
    }

    /// Requests an einen Pfad (`/openai/v1/decisions` …).
    pub fn requests_to(&self, path: &str) -> Vec<RequestRecord> {
        self.openai_requests()
            .into_iter()
            .filter(|r| r.path == path)
            .collect()
    }

    /// Bodies der Requests an `path` als JSON.
    pub fn bodies(&self, path: &str) -> Vec<Value> {
        self.requests_to(path)
            .iter()
            .map(|r| serde_json::from_str(&r.body).expect("request body is JSON"))
            .collect()
    }

    /// Globales Szenario für die nächsten `count` OpenAI-Requests (`POST /__mock/scenario`).
    pub async fn scenario(&self, scenario: &str, count: usize) {
        let r = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!("http://{}/__mock/scenario", self.addr()))
            .json(&json!({"provider": "openai", "scenario": scenario, "count": count}))
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success());
    }
}

/// Host eines protokollierten Requests ohne Port (der `Host`-Header trägt den ephemeren Port).
pub fn host_of(r: &RequestRecord) -> &str {
    r.host.split(':').next().unwrap_or("")
}

pub fn client_for(hosts: &[&str], addr: SocketAddr) -> reqwest::Client {
    let mut builder = reqwest::Client::builder().no_proxy();
    for host in hosts {
        builder = builder.resolve(host, addr);
    }
    builder.build().expect("client builds")
}

/// Limiter mit Standardgrenzen, aber kurzem Backoff (Wiederholungen nach INTEG-005 in Millisekunden statt Sekunden),
/// damit Tests gegen den echten HTTP-Mock schnell bleiben.
pub fn limiter() -> Limiter {
    limiter_with(LimitsConfig::default())
}

pub fn limiter_with(mut config: LimitsConfig) -> Limiter {
    config.backoff = BackoffPolicy {
        base: Duration::from_millis(5),
        factor: 2.0,
        cap: Duration::from_millis(40),
        jitter: 0.0,
    };
    Limiter::with_jitter(
        config,
        Arc::new(TokioClock::new()),
        Arc::new(FixedJitter(0.5)),
    )
}

/// Limiter mit Timeout `seconds` für OpenAI und ohne Wiederholung.
pub fn limiter_timeout(seconds: u64) -> Limiter {
    let mut config = LimitsConfig {
        max_retries: 0,
        ..LimitsConfig::default()
    };
    config.providers.insert(
        "openai".into(),
        ProviderLimits {
            concurrent_requests: 1,
            timeout: Duration::from_secs(seconds),
        },
    );
    limiter_with(config)
}

/// Fixture-Server mit eigenen Routen; die `scenario.toml` entsteht zur Laufzeit in einem Temp-Verzeichnis.
pub async fn route_site(name: &str, scenario_toml: &str) -> TestSite {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("os-ai-k02-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("scenario.toml"), scenario_toml).unwrap();
    TestSite::start_dir_with(dir, SiteOptions::default())
        .await
        .expect("route site starts")
}

/// Preise aus `tests/fixtures/ai/pricing-test.json` (Anbieter, Modell, Endpunkt) als Dezimaltexte
/// (Eingabe, Ausgabe) je 1 Mio. Tokens.
pub fn test_price(provider: &str, model: &str, endpoint: &str) -> (String, String) {
    let path = os_testsite::fixtures_root()
        .join("ai")
        .join("pricing-test.json");
    let file: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let entry = file["prices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["provider"] == provider && p["model"] == model && p["endpoint"] == endpoint)
        .expect("price entry");
    (
        entry["input_per_million"].to_string(),
        entry["output_per_million"].to_string(),
    )
}

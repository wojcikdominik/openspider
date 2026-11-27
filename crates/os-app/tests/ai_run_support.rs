//! Gemeinsame Hilfen der Tests `ai_run_*.rs` von M4-D05 (per `#[path]` eingebunden, ohne eigene Tests): ai-mock,
//! App mit erfundenem OpenAI-Schlüssel und Basis-URL auf den Mock, gecrawlte KI-Fixture-Site in der App geöffnet.
#![allow(dead_code, unused_imports)]
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use os_app::dto::{CrawlConfigDto, OpState};
use os_app::pool::CrawlKey;
use os_app::runtime::sessions::SessionInfo;
use os_app::{App, AppConfig, CallCtx, CmdError, ErrorCode, WindowId};
use os_fetch::{MemorySecretStore, SecretStoreMut};
use os_model::budget::MicroUsd;
use os_model::{Clock, CrawlConfig, CrawlState, SystemClock, Timestamp};
use os_store::{CrawlFile, OpenMode};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};

/// Schwelle, die nie greift (als TOML-Ganzzahl darstellbar).
pub const NO_CONFIRMATION: u64 = 1_000_000_000_000;

pub const DECISIONS: &str = "/openai/v1/decisions";

pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::fixtures().unwrap();
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Self { mock, server }
    }
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.server.addr())
    }
    pub fn log(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| !r.path.starts_with("/__mock") && !r.path.ends_with("/models"))
            .collect()
    }
    pub fn decisions(&self) -> usize {
        self.log().iter().filter(|r| r.path == DECISIONS).count()
    }
    pub fn held(&self) -> usize {
        self.log().iter().filter(|r| r.held && !r.aborted).count()
    }
    pub async fn control(&self, path: &str, body: Value) {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(self.url(path))
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap();
    }
    pub async fn hold(&self, count: usize) {
        self.control(
            "/__mock/scenario",
            json!({"provider": "openai", "scenario": "hold", "count": count}),
        )
        .await;
    }
    pub async fn release(&self, count: usize) {
        self.control(
            "/__mock/release",
            json!({"provider": "openai", "count": count}),
        )
        .await;
    }
    pub async fn until(&self, what: &str, pred: impl Fn(&Mock) -> bool) {
        for _ in 0..3000 {
            if pred(self) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("Mock erreicht nicht: {what}");
    }
}

pub fn ctx() -> CallCtx {
    CallCtx::for_window("main")
}

pub fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

pub fn question(rel: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(repo(rel)).unwrap()).unwrap()
}

pub fn page_type() -> Value {
    question("tests/fixtures/ai/decisions/questions/valid/page-type.question.json")
}

pub fn with_questions(cfg: &mut CrawlConfig, questions: &[Value]) {
    cfg.ai.enabled = true;
    cfg.ai.questions = questions.to_vec();
    cfg.ai.active_questions = questions
        .iter()
        .map(|q| q["id"].as_str().unwrap().to_string())
        .collect();
}

pub fn app(home: &Path) -> App {
    let secrets = Arc::new(MemorySecretStore::new());
    secrets.set("ai.openai", "sk-TEST-D05");
    App::new(AppConfig::new(home).with_secrets(secrets)).expect("App startet")
}

pub async fn call(app: &App, name: &str, args: Value) -> Result<Value, CmdError> {
    app.dispatch(ctx(), name, args).await
}

/// Schalter an, OpenAI auf den Mock, eine gleichzeitige Anfrage, Bestätigungsschwelle `threshold` µUSD.
/// App unter `home` mit App-Einstellungen im Home (wie `OPENSPIDER_HOME`): Schalter `on`, OpenAI auf den Mock, eine
/// gleichzeitige Anfrage, Bestätigungsschwelle `threshold` µUSD, Schwärzung aus; erfundener OpenAI-Schlüssel.
pub fn app_with(home: &Path, mock: &Mock, on: bool, threshold: u64) -> App {
    std::fs::create_dir_all(home).unwrap();
    let mut s = os_model::settings::AppSettings::load(home).unwrap_or_default();
    s.ai_app.preview_enabled = on;
    s.ai_app.confirmation_threshold = MicroUsd(threshold);
    s.ai_app.redact_personal_data = false;
    s.ai_app.provider_endpoints.insert(
        "openai".into(),
        os_model::config::ProviderEndpoint {
            timeout_seconds: 20,
            concurrent_requests: 1,
            base_url: Some(mock.url("/openai/v1")),
            ..Default::default()
        },
    );
    s.save(home).unwrap();
    app(home)
}

/// Einwilligung je Installation (AI-018) für die Felder der Fragen (Anbieter OpenAI, Aufgabe `decision`).
pub fn grant(app: &App, questions: &[Value]) {
    let gate = os_app::commands::ai_consent::consent_gate(app).unwrap();
    for q in questions {
        let def = os_ai::decisions::format::parse(&q.to_string()).unwrap();
        gate.grant(
            os_ai::policy::ConsentKey::new("openai", "decision", def.field_set()),
            1_764_000_000_000,
            None,
        )
        .unwrap();
    }
}

/// Crawl der KI-Fixture-Site (25 Seiten-URLs) ohne KI, Datei im Zustand `Completed`, in der App geöffnet.
pub async fn crawled(dir: &Path, cfg: &CrawlConfig) -> PathBuf {
    let fx = os_engine::testing::crawl_fixture_with(
        "ai/site",
        "http://ai.test/",
        cfg.clone(),
        os_engine::testing::test_deps(),
    )
    .await;
    let target = dir.join("crawl.ospider");
    fx.file.checkpoint_and_truncate().unwrap();
    drop(fx.file);
    std::fs::copy(fx.dir.path().join("crawl.ospider"), &target).unwrap();
    target
}

pub fn open_in(app: &App, path: &Path) {
    let cfg = CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .config()
        .unwrap();
    let key = CrawlKey("d05".into());
    app.register_crawl(key.clone(), path.to_path_buf());
    app.sessions().set_current(
        WindowId("main".into()),
        SessionInfo {
            key,
            path: path.to_path_buf(),
            config: Arc::new(cfg),
            state: CrawlState::Completed,
            writable: true,
            engine: None,
        },
    );
}

pub fn set_meta(path: &Path, pairs: &[(&str, String)]) {
    let f = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
    for (k, v) in pairs {
        f.set_meta(k, v).unwrap();
    }
    f.checkpoint_and_truncate().unwrap();
}

pub fn iso(offset_ms: i64) -> String {
    Timestamp::from_unix_millis(SystemClock.now().unix_millis() + offset_ms).to_db()
}

pub fn counts(path: &Path) -> std::collections::BTreeMap<String, u64> {
    let f = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    os_store::ai::answers::status_counts(f.conn(), "page-type", "1.0.0").unwrap()
}

pub fn logged(path: &Path) -> i64 {
    let f = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    f.conn()
        .query_row(
            "SELECT COUNT(*) FROM ai_requests WHERE cache_hit = 0",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

pub async fn wait_op(app: &App, handle: &Value) -> os_app::dto::OpStatus {
    let id = handle["opId"].as_str().unwrap().to_string();
    app.ops().wait(&id).await.expect("Operation bekannt")
}

pub async fn setup(threshold: u64) -> (tempfile::TempDir, Mock, App, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let mock = Mock::start().await;
    let mut cfg = CrawlConfig::default();
    // Ohne Warteschlange gecrawlt: der Seitentext kommt aus dem gespeicherten HTML (AI-032, Punkt 2).
    cfg.extraction.store_html = true;
    with_questions(&mut cfg, &[page_type()]);
    let path = crawled(dir.path(), &cfg).await;
    let app = app_with(&dir.path().join("home"), &mock, true, threshold);
    open_in(&app, &path);
    grant(&app, &[page_type()]);
    (dir, mock, app, path)
}

/// Startet einen Lauf mit gehaltenem Request 1 und liefert die Operation, sobald er gehalten wird.
pub async fn held_run(app: &App, mock: &Mock) -> Value {
    mock.hold(1).await;
    let op = call(
        app,
        "ai.decisionsRun",
        json!({"tasks": [], "confirmed": true}),
    )
    .await
    .unwrap();
    let id = op["opId"].as_str().unwrap().to_string();
    for _ in 0..3000 {
        if mock.held() >= 1 {
            return op;
        }
        if let Some(s) = app.ops().status(&id) {
            assert_eq!(
                s.state,
                OpState::Running,
                "Lauf vorzeitig beendet: {s:?}; {:?}",
                os_ai::run::notes()
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("kein gehaltener Request; {:?}", os_ai::run::notes());
}

pub fn assert_like_a_crash(path: &Path) {
    assert_eq!(logged(path), 0, "keine Protokollzeile");
    assert_eq!(
        counts(path).get("Pending"),
        Some(&22),
        "Eintrag bleibt Pending"
    );
    let f = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    let ledger =
        os_store::ai::budget::open_ledger(f.conn(), CrawlConfig::default().ai.budget, &[]).unwrap();
    let snapshot = os_model::budget::BudgetLedger::snapshot(&ledger).unwrap();
    assert_eq!(
        (snapshot.spent, snapshot.reserved),
        (MicroUsd(0), MicroUsd(0)),
        "Reservierung verfallen"
    );
    assert_eq!(f.get_meta("owner_pid").unwrap().unwrap_or_default(), "");
}

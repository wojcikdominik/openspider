//! Gemeinsame Hilfen der unabhängigen Abnahmetests M4-D05 (os-app).
//!
//! Erwartungen stammen ausschließlich aus `docs/spec/08-ai.md` (AI-012, AI-013, AI-020 mit den Nachträgen K4-13,
//! 26.11. und 27.11.2026, AI-032 mit dem Nachtrag „Quelle der Eingabefelder“, AI-039) und 01 CRAWL-126; der Code wurde
//! nur gelesen, um die öffentliche Schnittstelle aufzurufen (Command-Namen, DTOs, Tabellen der Crawl-Datei).
//!
//! * App mit eigenem Einstellungsordner, Testschlüssel nur im Arbeitsspeicher, Einwilligung als Vorbedingung (AI-018 ist
//!   nicht Gegenstand von D05), OpenAI-Endpunkt auf den ai-mock gelenkt (INTEG-019, App-Einstellung `base_url`).
//! * ai-mock mit `hold`/`release` (08 Konventionen, Nachtrag 26.11.2026); was im Request-Log steht, hat die Maschine
//!   verlassen.
//! * Fixture-Site `tests/fixtures/ai/site/` (Host `ai.test`) bzw. eine Site mit `tests/fixtures/ai/site/fields.html`.
// acceptance: independent
#![allow(dead_code, clippy::disallowed_types, clippy::disallowed_methods)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use os_ai::policy::{ConsentGate, ConsentKey, FieldSet};
use os_ai::run::{FileConsentStore, CONSENT_FILE};
use os_app::dto::{CmdError, OpState, StartRequest, StartScope, StartSource};
use os_app::{App, AppConfig, CallCtx, WindowId};
use os_fetch::secrets::MemorySecretStore;
use os_model::budget::MicroUsd;
use os_model::config::ProviderEndpoint;
use os_model::settings::AppSettings;
use os_model::{CrawlConfig, CrawlState};
use os_store::{CrawlFile, OpenMode};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use os_testsite::{SiteOptions, TestSite};
use serde_json::{json, Value};

/// Testschlüssel (erfunden, nur im Arbeitsspeicher).
pub const TEST_KEY: &str = "sk-TEST-D05-acceptance-0000000000";

/// Statuswerte aus 08 AI-020 (Tabelle samt Nachträgen 24.11. und 26.11.2026); `Error: Timeout nach <n> s` und
/// `Error: <HTTP-Code> <Meldung>` als Muster in [`is_spec_status`].
pub const SPEC_STATUS: [&str; 26] = [
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
    "Skipped (price unknown)",
    "Skipped (input unavailable)",
    "Budget exceeded",
    "Quota exceeded",
    "Error: Missing API key",
    "Error: Invalid output",
    "Error: Missing answer",
    "Error: Batch expired",
    "Error: Connection failed",
    "Error: Output limit reached",
    "Error: Timeout nach 30 s",
];

/// Wert aus der Tabelle von AI-020 (einschließlich der beiden Muster).
pub fn is_spec_status(s: &str) -> bool {
    if SPEC_STATUS.contains(&s) {
        return true;
    }
    if let Some(rest) = s.strip_prefix("Error: Timeout nach ") {
        return rest
            .strip_suffix(" s")
            .is_some_and(|n| n.parse::<u64>().is_ok());
    }
    if let Some(rest) = s.strip_prefix("Error: ") {
        let mut parts = rest.splitn(2, ' ');
        return parts
            .next()
            .is_some_and(|c| c.len() == 3 && c.parse::<u16>().is_ok());
    }
    false
}

// ---------------------------------------------------------------------------------------------------------------
// Ablauf

/// Die Tests eines Binaries laufen nacheinander: Crawls, ai-mock und Zeitmessungen (Heartbeat, Beenden im Zeitbudget)
/// sollen sich nicht gegenseitig verzögern.
pub static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn serial() -> tokio::sync::MutexGuard<'static, ()> {
    SERIAL.lock().await
}

pub fn ctx() -> CallCtx {
    CallCtx::for_window("main")
}

pub fn window() -> WindowId {
    WindowId("main".into())
}

pub async fn call(app: &App, name: &str, args: Value) -> Value {
    app.dispatch(ctx(), name, args)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

pub async fn try_call(app: &App, name: &str, args: Value) -> Result<Value, CmdError> {
    app.dispatch(ctx(), name, args).await
}

/// Wartet bis `cond` gilt (höchstens `limit`).
pub async fn wait_until(what: &str, limit: Duration, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < limit, "Zeitüberschreitung: {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Ergebnis einer Operation abwarten (Fehler der Operation als `Err`).
pub async fn wait_op(app: &App, op: &str) -> Result<Value, CmdError> {
    let status = tokio::time::timeout(Duration::from_secs(120), app.ops().wait(op))
        .await
        .expect("Operation endet in 120 s")
        .expect("Ergebnis der Operation");
    match status.error {
        Some(e) => Err(e),
        None => {
            assert_eq!(status.state, OpState::Done, "{op}");
            Ok(status.result.unwrap_or(Value::Null))
        }
    }
}

/// Ergebnis eines Commands; Operationen werden abgewartet.
pub async fn outcome(app: &App, name: &str, args: Value) -> Result<Value, CmdError> {
    let value = app.dispatch(ctx(), name, args).await?;
    match value.get("opId").and_then(Value::as_str) {
        Some(op) => wait_op(app, op).await,
        None => Ok(value),
    }
}

/// „KI-Analyse starten“ über `ai.decisionsRun` (der Kern hinter `startAiRun`), abgewartet.
pub async fn run_ai(app: &App, confirmed: bool) -> Result<Value, CmdError> {
    outcome(app, "ai.decisionsRun", json!({ "confirmed": confirmed })).await
}

/// Startet `ai.decisionsRun` und liefert die Kennung der Operation, ohne zu warten.
pub async fn start_ai(app: &App, confirmed: bool) -> String {
    let v = call(app, "ai.decisionsRun", json!({ "confirmed": confirmed })).await;
    v["opId"]
        .as_str()
        .unwrap_or_else(|| panic!("ai.decisionsRun liefert eine Operation: {v}"))
        .to_string()
}

// ---------------------------------------------------------------------------------------------------------------
// ai-mock

pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
    client: reqwest::Client,
}

impl Mock {
    pub async fn start() -> Mock {
        let mock = AiMock::fixtures().expect("ai-mock fixtures");
        let server = MockServer::start(mock.registrations())
            .await
            .expect("ai-mock startet");
        Mock {
            mock,
            server,
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        }
    }

    /// Basis-URL von OpenAI auf dem Mock (einschließlich `/v1`, 08 Konventionen).
    pub fn openai_base(&self) -> String {
        format!("{}/v1", self.server.base_url("openai"))
    }

    async fn control(&self, path: &str, body: Value) -> Value {
        let url = format!("{}{path}", self.server.base_url("__mock"));
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("{url}: {e}"));
        assert!(resp.status().is_success(), "{url}: {}", resp.status());
        resp.json().await.unwrap_or(Value::Null)
    }

    /// Globales Szenario für die nächsten `count` Requests eines Anbieters.
    pub async fn scenario(&self, provider: &str, scenario: &str, count: usize) {
        self.control(
            "/scenario",
            json!({ "provider": provider, "scenario": scenario, "count": count }),
        )
        .await;
    }

    /// Szenario `hold` für die nächsten `count` OpenAI-Requests.
    pub async fn hold(&self, count: usize) {
        self.scenario("openai", "hold", count).await;
    }

    /// Gibt gehaltene OpenAI-Requests frei (`None` = alle); Rückgabe `released`.
    pub async fn release(&self, count: Option<usize>) -> u64 {
        let body = match count {
            Some(n) => json!({ "provider": "openai", "count": n }),
            None => json!({ "provider": "openai" }),
        };
        self.control("/release", body).await["released"]
            .as_u64()
            .unwrap_or(0)
    }

    pub fn log(&self) -> Vec<RequestRecord> {
        self.mock.requests()
    }

    /// Requests an die Decisions API (gehaltene und abgebrochene eingeschlossen).
    pub fn decisions(&self) -> Vec<RequestRecord> {
        self.log()
            .into_iter()
            .filter(|r| r.method == "POST" && r.path.ends_with("/v1/decisions"))
            .collect()
    }

    /// Alle Requests an Anbieter-Routen (ohne Steueraufrufe und Modelllisten).
    pub fn provider_requests(&self) -> usize {
        self.log()
            .into_iter()
            .filter(|r| {
                r.method == "POST"
                    && !r.path.contains("__mock")
                    && !r.path.ends_with("/scenario")
                    && !r.path.ends_with("/release")
            })
            .count()
    }

    /// Zahl gehaltener Requests (Feld `held`).
    pub fn held(&self) -> usize {
        self.decisions().iter().filter(|r| r.held).count()
    }

    pub async fn wait_held(&self, n: usize) {
        let start = Instant::now();
        while self.held() < n {
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "kein gehaltener Request ({n} erwartet)"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Adressen (`URL: …`) der Decisions-Requests in Eingangsreihenfolge.
    pub fn decision_urls(&self) -> Vec<String> {
        self.decisions()
            .iter()
            .filter_map(|r| request_url(&body(r)))
            .collect()
    }
}

/// Gibt beim Verwerfen (auch nach einem fehlgeschlagenen Assert) alle gehaltenen Requests frei, damit ein laufender
/// KI-Lauf endet und das Testende nicht auf ihn wartet. Synchron über TCP, ohne Laufzeit.
impl Drop for Mock {
    fn drop(&mut self) {
        use std::io::{Read, Write};
        let body = r#"{"provider":"openai"}"#;
        let Ok(mut s) =
            std::net::TcpStream::connect_timeout(&self.server.addr(), Duration::from_secs(2))
        else {
            return;
        };
        let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
        let req = format!(
            "POST /__mock/release HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.server.addr(),
            body.len()
        );
        if s.write_all(req.as_bytes()).is_ok() {
            let mut buf = [0u8; 256];
            let _ = s.read(&mut buf);
        }
    }
}

pub fn body(r: &RequestRecord) -> Value {
    serde_json::from_str(&r.body).unwrap_or(Value::Null)
}

/// Text unter `input` eines Decisions-Requests (alle Textteile, ohne Bilder).
pub fn input_text(body: &Value) -> String {
    fn collect(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Array(items) => items.iter().for_each(|i| collect(i, out)),
            Value::Object(map) => {
                if map.get("type").and_then(Value::as_str) == Some("input_image") {
                    return;
                }
                for key in ["text", "content"] {
                    if let Some(v) = map.get(key) {
                        collect(v, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    collect(&body["input"], &mut out);
    out.join("\n")
}

/// Wert einer beschrifteten Zeile des Textblocks (AI-032: „`URL: …`, `Title: …`, …“).
pub fn labelled(text: &str, label: &str) -> Option<String> {
    let prefix = format!("{label}: ");
    text.lines()
        .find_map(|l| l.trim_start().strip_prefix(&prefix).map(str::to_string))
}

pub fn request_url(body: &Value) -> Option<String> {
    labelled(&input_text(body), "URL")
}

/// Namen der Fragen eines Decisions-Requests.
pub fn question_names(body: &Value) -> Vec<String> {
    body["questions"]
        .as_array()
        .map(|qs| {
            qs.iter()
                .map(|q| q["name"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------------------------
// App

/// Einstellungen der Test-App.
#[derive(Clone, Copy)]
pub struct Opts {
    /// Schalter „KI (Vorschau)“.
    pub preview: bool,
    /// „Bestätigung verlangen ab“ (AI-012) in µUSD.
    pub threshold: u64,
    /// Gleichzeitige Requests (OpenAI).
    pub concurrency: u16,
    /// Testschlüssel für OpenAI hinterlegt.
    pub key: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            preview: true,
            threshold: 1_000_000,
            concurrency: 1,
            key: true,
        }
    }
}

pub struct Home {
    pub dir: tempfile::TempDir,
    pub secrets: Arc<MemorySecretStore>,
    pub app: App,
}

/// Alle Eingabefelder aus AI-032 (Einwilligung als Vorbedingung über alle Felder).
pub const ALL_FIELDS: [&str; 14] = [
    "url",
    "title",
    "meta_description",
    "h1",
    "h2",
    "page_text",
    "word_count",
    "html_lang",
    "hreflang_self",
    "structured_data_types",
    "page_type",
    "image",
    "alt",
    "image_context",
];

/// Schreibt die Einwilligung (AI-018) der Installation für die Aufgabe `decision` über alle Felder.
pub fn grant_consent(settings_dir: &Path) {
    std::fs::create_dir_all(settings_dir).unwrap();
    let gate = ConsentGate::with_store(Arc::new(FileConsentStore::new(
        settings_dir.join(CONSENT_FILE),
    )))
    .unwrap();
    let fields = FieldSet::from_ids(ALL_FIELDS.iter().copied()).unwrap();
    for p in ["openai", "anthropic", "openrouter"] {
        gate.grant(ConsentKey::new(p, "decision", fields.clone()), 1, None)
            .unwrap();
    }
}

pub fn app_settings(mock: &Mock, opts: Opts) -> AppSettings {
    let mut s = AppSettings::default();
    s.general.language = os_model::UiLanguage::De;
    s.ai_app.preview_enabled = opts.preview;
    s.ai_app.confirmation_threshold = MicroUsd(opts.threshold);
    s.ai_app.provider_endpoints.insert(
        "openai".into(),
        ProviderEndpoint {
            base_url: Some(mock.openai_base()),
            concurrent_requests: opts.concurrency,
            ..ProviderEndpoint::default()
        },
    );
    s
}

fn new_app(dir: &Path, secrets: Arc<MemorySecretStore>) -> App {
    App::new(AppConfig::new(dir).with_secrets(secrets)).expect("App startet")
}

/// Neue App mit eigenem Ordner.
pub fn home(mock: &Mock, opts: Opts) -> Home {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemorySecretStore::new());
    if opts.key {
        secrets.set("ai.openai", TEST_KEY);
    }
    let config = AppConfig::new(dir.path());
    app_settings(mock, opts)
        .save(&config.paths.settings)
        .unwrap();
    grant_consent(&config.paths.settings);
    let app = new_app(dir.path(), secrets.clone());
    Home { dir, secrets, app }
}

impl Home {
    /// Neustart der App auf demselben Ordner (die bisherige Instanz wird verworfen).
    pub fn restart(self) -> Home {
        let Home { dir, secrets, app } = self;
        drop(app);
        let app = new_app(dir.path(), secrets.clone());
        Home { dir, secrets, app }
    }

    /// Zweite App-Instanz auf demselben Ordner (eigene Sitzungen, derselbe Prozess).
    pub fn second(&self) -> App {
        new_app(self.dir.path(), self.secrets.clone())
    }

    pub fn settings_dir(&self) -> PathBuf {
        self.app.config().paths.settings.clone()
    }

    /// Speichert das Standardprofil (wird beim Crawl-Start verwendet).
    pub fn profile(&self, cfg: &CrawlConfig) {
        cfg.save_default(&self.app.config().paths.settings).unwrap();
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Fixture-Sites

pub fn repo_file(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

/// Fixture-Site aus 08 Konventionen (|G| = 25, |S| = 22).
pub async fn ai_site() -> TestSite {
    TestSite::start_dir_with(
        repo_file("tests/fixtures/ai/site"),
        SiteOptions {
            port: 0,
            log_requests: true,
        },
    )
    .await
    .expect("Fixture-Site startet")
}

/// Fixture-Site wie [`ai_site`], aber mit einem echten Port in allen `{{PORT}}`-Platzhaltern der Routen: os-testsite
/// ersetzt Platzhalter nur in statischen Dateien, nicht im `body` einer Route in `scenario.toml` (`respond.rs`). Der Ordner
/// wird nach `dir` kopiert, der Port vorher gewählt und in die Kopie eingesetzt; sonst unverändert.
pub async fn ai_site_with_port(dir: &Path) -> TestSite {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let target = to.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                copy(&e.path(), &target);
            } else {
                std::fs::copy(e.path(), &target).unwrap();
            }
        }
    }
    let source = repo_file("tests/fixtures/ai/site");
    copy(&source, dir);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let scenario = std::fs::read_to_string(source.join("scenario.toml")).unwrap();
    std::fs::write(
        dir.join("scenario.toml"),
        scenario.replace("{{PORT}}", &port.to_string()),
    )
    .unwrap();
    TestSite::start_dir_with(
        dir,
        SiteOptions {
            port,
            log_requests: true,
        },
    )
    .await
    .expect("Fixture-Site mit Port startet")
}

/// Site mit Startseite und `tests/fixtures/ai/site/fields.html` unter `/fields` (das selbstreferenzierende hreflang der
/// Fixture zeigt auf `/fields`). Der Inhalt wird unverändert aus der Fixture übernommen.
pub async fn fields_site(dir: &Path) -> TestSite {
    let fields = std::fs::read_to_string(repo_file("tests/fixtures/ai/site/fields.html")).unwrap();
    assert!(!fields.contains("'''"));
    let index = "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\"><title>Start</title>\
        <meta name=\"description\" content=\"Startseite der Felder\"></head><body><main><h1>Start</h1>\
        <p>Startseite ohne strukturierte Daten. MOCK-ID:fields-start</p><a href=\"/fields\">Felder</a></main></body></html>";
    let scenario = format!(
        "[[route]]\nhost = \"ai.test\"\npath = \"/\"\nstatus = 200\ncontent_type = \"text/html; charset=utf-8\"\nbody = '''{index}'''\n\n\
         [[route]]\nhost = \"ai.test\"\npath = \"/fields\"\nstatus = 200\ncontent_type = \"text/html; charset=utf-8\"\nbody = '''{}'''\n",
        fields.trim_end()
    );
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("scenario.toml"), scenario).unwrap();
    TestSite::start_dir_with(
        dir,
        SiteOptions {
            port: 0,
            log_requests: true,
        },
    )
    .await
    .expect("Felder-Site startet")
}

/// Seitenabrufe der Site (ohne robots.txt).
pub fn page_fetches(site: &TestSite) -> usize {
    site.requests()
        .into_iter()
        .filter(|r| r.method == "GET" && r.path_and_query != "/robots.txt")
        .count()
}

/// Seitenfrage (AI-030) `predicate` mit den Feldern `inputs`.
pub fn question(id: &str, inputs: &[&str]) -> Value {
    json!({
        "id": id,
        "version": "1.0.0",
        "name": format!("D05 {id}"),
        "type": "predicate",
        "instructions": format!("Abnahmefrage {id}: trifft die Aussage auf die Seite zu?"),
        "subject": "page",
        "inputs": inputs,
    })
}

/// Profil für die Fixture-Site: ein Thread, keine automatische Crawl-Analyse, `*.test` → Site.
pub fn profile(site: &TestSite, questions: &[Value]) -> CrawlConfig {
    let mut c = CrawlConfig::default();
    c.analysis.auto_at_end = false;
    c.speed.threads = 1;
    c.network.resolve = vec![format!("*.test=127.0.0.1:{}", site.port())];
    if !questions.is_empty() {
        c.ai.enabled = true;
        c.ai.questions = questions.to_vec();
        c.ai.active_questions = questions
            .iter()
            .map(|q| q["id"].as_str().unwrap().to_string())
            .collect();
    }
    c
}

pub const START: &str = "http://ai.test/";

/// Startet einen Spider-Crawl (Standardprofil); Rückgabe der Antwort (`crawlId`).
pub async fn start_crawl(app: &App, url: &str) -> Value {
    let req = StartRequest {
        source: StartSource::Spider { url: url.into() },
        scope: StartScope::Domain,
        profile: None,
    };
    call(app, "crawl.start", serde_json::to_value(req).unwrap()).await
}

pub fn state(app: &App) -> Option<CrawlState> {
    app.sessions().current(&window()).map(|s| s.state)
}

/// Wartet (mit `session::poll`) auf einen Zustand.
pub async fn wait_state(app: &App, want: CrawlState, limit: Duration) {
    let start = Instant::now();
    loop {
        let _ = os_app::session::poll(app).await;
        if state(app) == Some(want) {
            return;
        }
        assert!(
            start.elapsed() < limit,
            "Zustand {want:?} nicht erreicht (ist {:?})",
            state(app)
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Crawl starten und bis „Fertig“ abwarten.
pub async fn crawl_to_end(app: &App, url: &str) -> Value {
    let started = start_crawl(app, url).await;
    wait_state(app, CrawlState::Completed, Duration::from_secs(90)).await;
    started
}

pub fn crawl_path(app: &App) -> PathBuf {
    app.sessions()
        .current(&window())
        .expect("Crawl geöffnet")
        .path
}

pub async fn open(app: &App, path: &Path) -> Value {
    call(app, "crawl.open", json!({ "kind": "path", "path": path })).await
}

/// Schließt die Crawl-Datei des Fensters, während ein KI-Lauf einen gehaltenen Request hat. Lehnt die App ab, werden die
/// gehaltenen Requests freigegeben (sonst wartet das Ende des Tests auf den Lauf) und der Fehler zurückgegeben.
pub async fn close_during_run(app: &App, mock: &Mock) -> Result<(), CmdError> {
    match try_call(app, "crawl.clear", Value::Null).await {
        Ok(_) => Ok(()),
        Err(e) => {
            mock.release(None).await;
            Err(e)
        }
    }
}

/// Schließt die Crawl-Datei des Fensters.
pub async fn close(app: &App) {
    call(app, "crawl.clear", Value::Null).await;
}

// ---------------------------------------------------------------------------------------------------------------
// Crawl-Datei (nur lesend, unabhängig von der App)

pub fn ro(path: &Path) -> CrawlFile {
    CrawlFile::open(path, OpenMode::ReadOnly).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn q_i64(path: &Path, sql: &str) -> i64 {
    ro(path)
        .conn()
        .query_row(sql, [], |r| r.get::<_, Option<i64>>(0))
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
        .unwrap_or(0)
}

pub fn meta(path: &Path, key: &str) -> Option<String> {
    ro(path).get_meta(key).unwrap()
}

/// Statuszählung einer Frage.
pub fn statuses(path: &Path, question: &str) -> BTreeMap<String, u64> {
    let f = ro(path);
    let mut stmt = f
        .conn()
        .prepare("SELECT status, COUNT(*) FROM ai_answers WHERE question_id = ?1 GROUP BY status")
        .unwrap();
    let rows = stmt
        .query_map([question], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
        })
        .unwrap()
        .collect::<Result<BTreeMap<_, _>, _>>()
        .unwrap();
    rows
}

pub fn count_status(path: &Path, question: &str, status: &str) -> u64 {
    statuses(path, question).get(status).copied().unwrap_or(0)
}

/// Status des Eintrags (Adresse, Frage).
pub fn status_of(path: &Path, address: &str, question: &str) -> Option<String> {
    let f = ro(path);
    f.conn()
        .query_row(
            "SELECT a.status FROM ai_answers a JOIN urls u ON u.id = a.url_id \
             WHERE u.address = ?1 AND a.question_id = ?2",
            [address, question],
            |r| r.get::<_, String>(0),
        )
        .ok()
}

/// Alle Statuswerte der Datei stehen in der Tabelle von AI-020.
pub fn assert_spec_statuses(path: &Path) {
    let f = ro(path);
    let mut stmt = f
        .conn()
        .prepare("SELECT DISTINCT status FROM ai_answers")
        .unwrap();
    let all: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for s in all {
        assert!(
            is_spec_status(&s),
            "Status außerhalb der Tabelle AI-020: {s}"
        );
    }
}

/// Interne HTML-Seiten mit Status 200, die indexierbar sind (Standard-Geltungsbereich S).
pub const IN_SCOPE: &str =
    "SELECT COUNT(*) FROM urls WHERE is_internal = 1 AND lower(content_class) = 'html' \
     AND status_code = 200 AND indexability = 'Indexable'";

/// Gecrawlte URLs.
pub const CRAWLED: &str = "SELECT COUNT(*) FROM urls WHERE crawl_timestamp IS NOT NULL";

/// Adressen der Seiten im Standard-Geltungsbereich.
pub fn in_scope_addresses(path: &Path) -> BTreeSet<String> {
    let f = ro(path);
    let mut stmt = f
        .conn()
        .prepare(
            "SELECT address FROM urls WHERE is_internal = 1 AND lower(content_class) = 'html' \
             AND status_code = 200 AND indexability = 'Indexable'",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<BTreeSet<_>, _>>()
        .unwrap();
    rows
}

/// Protokollzeilen (`ai_requests`) je Adresse.
pub fn logged_rows_for(path: &Path, address: &str) -> i64 {
    let f = ro(path);
    f.conn()
        .query_row(
            "SELECT COUNT(*) FROM ai_requests r JOIN urls u ON u.id = r.url_id WHERE u.address = ?1",
            [address],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
}

/// Budgetstand nach AI-013 beim Öffnen der Datei: (Verbrauch inkl. offener Reservierungen, offene Reservierungen,
/// Summe der protokollierten Kosten).
pub fn budget_on_open(path: &Path) -> (u64, u64, u64) {
    let f = ro(path);
    let ledger =
        os_store::ai::budget::open_ledger(f.conn(), os_model::budget::BudgetLimits::default(), &[])
            .unwrap();
    let snap = os_model::budget::BudgetLedger::snapshot(&ledger).unwrap();
    let logged: i64 = f
        .conn()
        .query_row(
            "SELECT COALESCE(SUM(cost_microusd), 0) FROM ai_requests",
            [],
            |r| r.get(0),
        )
        .unwrap();
    (snap.consumption().0, snap.reserved.0, logged as u64)
}

/// Erstes Vorkommen eines Schlüssels in einem JSON-Wert (Antworten, deren Hülle nicht Gegenstand des Tests ist).
pub fn find_key<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|x| find_key(x, key))),
        Value::Array(items) => items.iter().find_map(|x| find_key(x, key)),
        _ => None,
    }
}

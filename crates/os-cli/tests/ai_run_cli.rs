//! M4-D05: KI-Warteschlange im CLI-Prozess (08 AI-020 [M4], Nachtrag K4-13; Schritt 4 des Pakets: nur die KI-Warteschlange
//! in den `EngineDeps`, automatische Ausführung, SIGINT und `--resume` über die Engine). Gegen den ai-mock mit `hold`;
//! Schlüssel nur erfunden (`OPENSPIDER_AI_KEY_OPENAI`), Basis-URL und Schalter in den App-Einstellungen des Test-Homes,
//! Einwilligung je Installation in `ai-consent.json`.
//!
//! Ersetzen die Platzhalter `decisions_pending::cli_sigint_and_resume` und `decisions_pending::crash_by_sigkill` (siehe
//! Bericht d05).
#![cfg(unix)]
mod common;

use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::Env;
use os_model::CrawlConfig;
use os_store::{CrawlFile, OpenMode};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use os_testsite::TestSite;
use serde_json::{json, Value};

const DECISIONS: &str = "/openai/v1/decisions";

struct Mock {
    mock: AiMock,
    server: MockServer,
}

impl Mock {
    async fn start() -> Self {
        let mock = AiMock::fixtures().unwrap();
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Self { mock, server }
    }
    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.server.addr())
    }
    fn decisions(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.path == DECISIONS)
            .collect()
    }
    async fn control(&self, path: &str, body: Value) {
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
}

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn page_type() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(repo(
            "tests/fixtures/ai/decisions/questions/valid/page-type.question.json",
        ))
        .unwrap(),
    )
    .unwrap()
}

/// Test-Home: Schalter an, OpenAI auf den Mock, eine gleichzeitige Anfrage, Einwilligung für die Seitentyp-Frage.
fn prepare(env: &Env, mock: &Mock) -> PathBuf {
    let mut settings = os_model::settings::AppSettings::load(env.home.path()).unwrap();
    settings.ai_app.preview_enabled = true;
    settings.ai_app.redact_personal_data = false;
    settings.ai_app.confirmation_threshold = os_model::budget::MicroUsd(u64::MAX);
    settings.ai_app.provider_endpoints.insert(
        "openai".into(),
        os_model::config::ProviderEndpoint {
            timeout_seconds: 20,
            concurrent_requests: 1,
            base_url: Some(mock.url("/openai/v1")),
            ..Default::default()
        },
    );
    settings.save(env.home.path()).unwrap();
    let q = os_ai::decisions::format::parse(&page_type().to_string()).unwrap();
    let gate = os_ai::policy::ConsentGate::with_store(Arc::new(os_ai::run::FileConsentStore::new(
        env.home.path().join(os_ai::run::CONSENT_FILE),
    )))
    .unwrap();
    gate.grant(
        os_ai::policy::ConsentKey::new("openai", "decision", q.field_set()),
        1_764_000_000_000,
        None,
    )
    .unwrap();
    let mut cfg = CrawlConfig::default();
    cfg.ai.enabled = true;
    cfg.ai.questions = vec![page_type()];
    cfg.ai.active_questions = vec!["page-type".into()];
    cfg.speed.threads = 1;
    let profile = env.work.path().join("ai.osconfig");
    std::fs::write(&profile, cfg.to_osconfig_string().unwrap()).unwrap();
    profile
}

/// Langsame Kopie der KI-Fixture-Site (300 ms je Seite), damit der Crawl bei SIGINT noch läuft.
async fn slow_site(dir: &Path) -> TestSite {
    let src = std::fs::read_to_string(repo("tests/fixtures/ai/site/scenario.toml")).unwrap();
    std::fs::write(
        dir.join("scenario.toml"),
        src.replace("[[route]]", "[[route]]\ndelay_ms = 300"),
    )
    .unwrap();
    TestSite::start_dir(dir, 0).await.unwrap()
}

fn spawn(env: &Env, args: &[String]) -> Child {
    let mut cmd = env.command();
    cmd.args(args)
        .env("OPENSPIDER_AI_KEY_OPENAI", "sk-TEST-D05")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.spawn().expect("openspider startbar")
}

fn crawl_args(site: &TestSite, profile: &Path) -> Vec<String> {
    vec![
        "--crawl".into(),
        "http://ai.test/".into(),
        "--resolve".into(),
        format!("*.test=127.0.0.1:{}", site.port()),
        "--config".into(),
        profile.display().to_string(),
    ]
}

async fn wait_until(what: &str, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "Zeitüberschreitung: {what}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn exit_code(child: Child) -> (i32, String) {
    tokio::task::spawn_blocking(move || {
        let out = child.wait_with_output().unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    })
    .await
    .unwrap()
}

fn counts(path: &Path) -> std::collections::BTreeMap<String, u64> {
    let f = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    os_store::ai::answers::status_counts(f.conn(), "page-type", "1.0.0").unwrap()
}

fn logged(path: &Path) -> i64 {
    let f = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    f.conn()
        .query_row(
            "SELECT COUNT(*) FROM ai_requests WHERE cache_hit = 0",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn mock_ids(records: &[RequestRecord]) -> Vec<String> {
    records
        .iter()
        .filter(|r| !r.aborted)
        .filter_map(|r| {
            let at = r.body.find("MOCK-ID:")?;
            Some(
                r.body[at..]
                    .split(|c: char| c.is_whitespace() || c == '"' || c == '\\')
                    .next()?
                    .to_string(),
            )
        })
        .collect()
}

/// Startet den Crawl, wartet auf den gehaltenen ersten KI-Request und beendet den Prozess mit `signal`.
async fn interrupted(
    env: &Env,
    mock: &Mock,
    site: &TestSite,
    profile: &Path,
    signal: i32,
) -> (i32, PathBuf) {
    mock.control(
        "/__mock/scenario",
        json!({"provider": "openai", "scenario": "hold", "count": 1}),
    )
    .await;
    let child = spawn(env, &crawl_args(site, profile));
    wait_until("erster KI-Request gehalten", || {
        mock.decisions().iter().any(|r| r.held)
    })
    .await;
    let pid = i32::try_from(child.id()).unwrap();
    // SAFETY: Signal an einen eigenen Kindprozess.
    assert_eq!(unsafe { libc::kill(pid, signal) }, 0);
    let (code, _) = exit_code(child).await;
    let files = env.crawl_files();
    assert_eq!(files.len(), 1);
    (code, files[0].clone())
}

/// Fortsetzen (`--load-crawl <ID> --resume`) bis zum Ende; danach alle Einträge fertig, jeder genau einmal gesendet.
async fn resume_and_check(env: &Env, mock: &Mock, site: &TestSite, file: &Path) {
    let id = file.file_stem().unwrap().to_string_lossy().into_owned();
    let child = spawn(
        env,
        &[
            "--load-crawl".into(),
            id,
            "--resume".into(),
            "--resolve".into(),
            format!("*.test=127.0.0.1:{}", site.port()),
        ],
    );
    let (code, stderr) = exit_code(child).await;
    assert_eq!(code, 0, "{stderr}");
    let c = counts(file);
    assert_eq!(c.get("Success"), Some(&22), "{c:?}");
    assert_eq!(logged(file), 22, "je Eintrag genau eine Protokollzeile");
    let mut ids = mock_ids(&mock.decisions());
    let all = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), all, "kein Eintrag doppelt beantwortet");
}

// spec: AI-020 [M4]
// CLI: SIGINT während offener KI-Einträge (erster Request gehalten) → Exit-Code 130, Einträge offen (`Pending` bzw. noch
// ohne Zeile), keine Protokollzeile für den gehaltenen Request; `--load-crawl <ID> --resume` sendet genau die offenen
// Einträge und endet mit allen Einträgen fertig.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_sigint_and_resume() {
    let mock = Mock::start().await;
    let env = Env::new();
    let profile = prepare(&env, &mock);
    let site_dir = tempfile::tempdir().unwrap();
    let site = slow_site(site_dir.path()).await;
    let (code, file) = interrupted(&env, &mock, &site, &profile, libc::SIGINT).await;
    assert_eq!(code, 130);
    assert_eq!(
        logged(&file),
        0,
        "der gehaltene Request ist nicht übernommen"
    );
    assert_eq!(counts(&file).get("Success"), None);
    resume_and_check(&env, &mock, &site, &file).await;
}

// spec: AI-020 [M4]
// Absturz per SIGKILL, während der Mock Request 1 hält → nach dem Öffnen: Eintrag `Pending`, keine Protokollzeile, Verbrauch
// = Summe der Protokollzeilen (keine Reservierung); beim Öffnen 0 Requests; Fortsetzen sendet den Eintrag genau einmal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crash_by_sigkill() {
    let mock = Mock::start().await;
    let env = Env::new();
    let profile = prepare(&env, &mock);
    let site_dir = tempfile::tempdir().unwrap();
    let site = slow_site(site_dir.path()).await;
    let (_, file) = interrupted(&env, &mock, &site, &profile, libc::SIGKILL).await;
    let before = mock.decisions().len();
    // CRAWL-125: Ein abgestürzter Crawl lässt sich erst mehr als 10 s nach dem letzten Heartbeat fortsetzen.
    tokio::time::sleep(Duration::from_millis(10_500)).await;
    {
        let f = CrawlFile::open(&file, OpenMode::ReadOnly).unwrap();
        let ledger =
            os_store::ai::budget::open_ledger(f.conn(), CrawlConfig::default().ai.budget, &[])
                .unwrap();
        let s = os_model::budget::BudgetLedger::snapshot(&ledger).unwrap();
        assert_eq!((s.spent.0, s.reserved.0), (0, 0));
    }
    assert_eq!(logged(&file), 0);
    assert_eq!(mock.decisions().len(), before, "beim Öffnen 0 Requests");
    resume_and_check(&env, &mock, &site, &file).await;
}

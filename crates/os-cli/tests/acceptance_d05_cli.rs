//! Unabhängige Abnahme M4-D05 (CLI): KI-Warteschlange in den `EngineDeps` des CLI – SIGINT und Fortsetzen, Absturz per
//! SIGKILL.
//!
//! Erwartungen ausschließlich aus 08 AI-020, Nachtrag K4-13 („Pause und Fortsetzen“, „Die Pause durch SIGINT im CLI …
//! verhält sich ebenso“, „Absturz bzw. hartes Beenden“) mit den Kriterien „CLI: SIGINT während offener KI-Einträge →
//! Exit-Code 130, Einträge `Pending`; `--load-crawl <ID> --resume` sendet genau diese und endet mit allen Einträgen fertig“
//! und „Absturz: Prozess wird per SIGKILL beendet, während der Mock Request n hält → nach dem Öffnen: Eintrag n `Pending`,
//! keine Protokollzeile für ihn, Budgetverbrauch = Summe der vorhandenen Protokollzeilen …; beim Öffnen 0 Requests;
//! ‚Fortsetzen‘ sendet Eintrag n genau einmal; am Ende genau 1 Protokollzeile für Eintrag n“; 01 CRAWL-125/126/128.
//! Das Binary `openspider` läuft als eigener Prozess; App-Einstellungen (Schalter, Endpunkt), Einwilligung und
//! Standardprofil liegen in `OPENSPIDER_HOME`, der Testschlüssel kommt aus `OPENSPIDER_AI_KEY_OPENAI` (AI-009).
#![cfg(unix)]
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{Env, Run};
use os_ai::policy::{ConsentGate, ConsentKey, FieldSet};
use os_ai::run::{FileConsentStore, CONSENT_FILE};
use os_model::budget::MicroUsd;
use os_model::config::ProviderEndpoint;
use os_model::settings::AppSettings;
use os_model::CrawlConfig;
use os_store::{CrawlFile, OpenMode};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use os_testsite::{fixtures_root, SiteOptions, TestSite};
use serde_json::{json, Value};

const TEST_KEY: &str = "sk-TEST-D05-cli-0000000000";
const QUESTION: &str = "d05-cli";

struct Mock {
    mock: AiMock,
    server: MockServer,
    client: reqwest::Client,
}

impl Mock {
    async fn start() -> Mock {
        let mock = AiMock::fixtures().unwrap();
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Mock {
            mock,
            server,
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        }
    }

    async fn scenario(&self, scenario: &str, count: usize) {
        let url = format!("{}/scenario", self.server.base_url("__mock"));
        let resp = self
            .client
            .post(&url)
            .json(&json!({"provider": "openai", "scenario": scenario, "count": count}))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
    }

    fn decisions(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.method == "POST" && r.path.ends_with("/v1/decisions"))
            .collect()
    }

    fn urls(&self) -> Vec<String> {
        self.decisions().iter().filter_map(request_url).collect()
    }
}

/// `URL: …` aus dem beschrifteten Textblock (AI-032) eines Decisions-Requests.
fn request_url(r: &RequestRecord) -> Option<String> {
    fn collect(v: &Value, out: &mut String) {
        match v {
            Value::String(s) => {
                out.push_str(s);
                out.push('\n');
            }
            Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
            Value::Object(m) => {
                for k in ["text", "content"] {
                    if let Some(x) = m.get(k) {
                        collect(x, out);
                    }
                }
            }
            _ => {}
        }
    }
    let body: Value = serde_json::from_str(&r.body).ok()?;
    let mut text = String::new();
    collect(&body["input"], &mut text);
    text.lines()
        .find_map(|l| l.trim_start().strip_prefix("URL: ").map(str::to_string))
}

async fn ai_site() -> TestSite {
    TestSite::start_dir_with(
        fixtures_root().join("ai").join("site"),
        SiteOptions {
            port: 0,
            log_requests: true,
        },
    )
    .await
    .unwrap()
}

fn page_fetches(site: &TestSite) -> usize {
    site.requests()
        .into_iter()
        .filter(|r| r.method == "GET" && r.path_and_query != "/robots.txt")
        .count()
}

/// Einstellungen, Einwilligung und Standardprofil im Datenordner des CLI.
fn prepare(env: &Env, mock: &Mock, site: &TestSite) {
    let dir = env.home.path();
    let mut s = AppSettings::load(dir).unwrap_or_default();
    s.ai_app.preview_enabled = true;
    s.ai_app.confirmation_threshold = MicroUsd(1_000_000);
    s.ai_app.provider_endpoints.insert(
        "openai".into(),
        ProviderEndpoint {
            base_url: Some(format!("{}/v1", mock.server.base_url("openai"))),
            concurrent_requests: 1,
            ..ProviderEndpoint::default()
        },
    );
    s.save(dir).unwrap();
    let gate =
        ConsentGate::with_store(Arc::new(FileConsentStore::new(dir.join(CONSENT_FILE)))).unwrap();
    gate.grant(
        ConsentKey::new(
            "openai",
            "decision",
            FieldSet::from_ids(["url", "title"]).unwrap(),
        ),
        1,
        None,
    )
    .unwrap();
    let mut cfg = CrawlConfig::default();
    cfg.analysis.auto_at_end = false;
    cfg.speed.threads = 1;
    cfg.speed.max_urls_per_sec = Some(3.0);
    cfg.network.resolve = vec![format!("*.test=127.0.0.1:{}", site.port())];
    cfg.ai.enabled = true;
    cfg.ai.questions = vec![json!({
        "id": QUESTION,
        "version": "1.0.0",
        "name": "D05 CLI",
        "type": "predicate",
        "instructions": "Abnahmefrage D05 (CLI): trifft die Aussage auf die Seite zu?",
        "subject": "page",
        "inputs": ["url", "title"],
    })];
    cfg.ai.active_questions = vec![QUESTION.into()];
    cfg.save_default(dir).unwrap();
}

fn spawn_crawl(env: &Env) -> Child {
    let mut cmd = env.command();
    cmd.env("OPENSPIDER_AI_KEY_OPENAI", TEST_KEY)
        .args(["--crawl", "http://ai.test/"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.spawn().expect("openspider startbar")
}

async fn run(env: &Env, args: &[&str]) -> Run {
    let mut cmd = env.command();
    cmd.env("OPENSPIDER_AI_KEY_OPENAI", TEST_KEY).args(args);
    tokio::task::spawn_blocking(move || Run::from_output(cmd.output().unwrap()))
        .await
        .unwrap()
}

async fn finish(child: Child) -> Run {
    tokio::task::spawn_blocking(move || Run::from_output(child.wait_with_output().unwrap()))
        .await
        .unwrap()
}

fn signal(child: &Child, sig: i32) {
    let pid = i32::try_from(child.id()).unwrap();
    // SAFETY: Signal an einen eigenen Kindprozess.
    let rc = unsafe { libc::kill(pid, sig) };
    assert_eq!(rc, 0);
}

async fn wait_until(what: &str, limit: Duration, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < limit, "Zeitüberschreitung: {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn only_file(env: &Env) -> (PathBuf, String) {
    let files = env.crawl_files();
    assert_eq!(files.len(), 1, "{files:?}");
    let id = files[0].file_stem().unwrap().to_string_lossy().into_owned();
    (files[0].clone(), id)
}

fn ro(path: &Path) -> CrawlFile {
    CrawlFile::open(path, OpenMode::ReadOnly).unwrap()
}

fn statuses(path: &Path) -> BTreeMap<String, u64> {
    let f = ro(path);
    let mut stmt = f
        .conn()
        .prepare("SELECT status, COUNT(*) FROM ai_answers WHERE question_id = ?1 GROUP BY status")
        .unwrap();
    let rows = stmt
        .query_map([QUESTION], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
        })
        .unwrap()
        .collect::<Result<BTreeMap<_, _>, _>>()
        .unwrap();
    rows
}

fn status_of(path: &Path, address: &str) -> Option<String> {
    ro(path)
        .conn()
        .query_row(
            "SELECT a.status FROM ai_answers a JOIN urls u ON u.id = a.url_id WHERE u.address = ?1 AND a.question_id = ?2",
            [address, QUESTION],
            |r| r.get::<_, String>(0),
        )
        .ok()
}

fn in_scope(path: &Path) -> BTreeSet<String> {
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

fn logged_rows_for(path: &Path, address: &str) -> i64 {
    ro(path)
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM ai_requests r JOIN urls u ON u.id = r.url_id WHERE u.address = ?1",
            [address],
            |r| r.get(0),
        )
        .unwrap()
}

fn meta(path: &Path, key: &str) -> Option<String> {
    ro(path).get_meta(key).unwrap()
}

// spec: AI-020 [M4]
// spec: CRAWL-126 [M4]
// acceptance: independent
// (c) cli_sigint_and_resume: SIGINT während offener KI-Einträge (Antworten des Mocks mit 1 s Verzögerung, der Crawl ruft noch
// Seiten ab) → Exit-Code 130, Crawl „Pausiert“, nicht beantwortete Einträge `Pending`, laufende Requests zu Ende geführt;
// `--load-crawl <ID> --resume` → Exit 0, alle 22 Einträge `Success`, keine URL doppelt im Mock-Log.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_cli_sigint_and_resume() {
    let mock = Mock::start().await;
    let site = ai_site().await;
    let env = Env::new();
    prepare(&env, &mock, &site);
    mock.scenario("delay=1", 1_000).await;
    let child = spawn_crawl(&env);
    wait_until(
        "KI-Requests und Seitenabrufe",
        Duration::from_secs(60),
        || mock.decisions().len() >= 2 && page_fetches(&site) >= 8,
    )
    .await;
    assert!(page_fetches(&site) < 25, "der Crawl läuft noch");
    let sigint_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    signal(&child, libc::SIGINT);
    let out = finish(child).await;
    assert_eq!(out.code, 130, "{}", out.dump());
    let (path, id) = only_file(&env);
    assert_eq!(meta(&path, "state").as_deref(), Some("Paused"));
    let mut findings: Vec<String> = Vec::new();
    let st = statuses(&path);
    let success = st.get("Success").copied().unwrap_or(0);
    let pending = st.get("Pending").copied().unwrap_or(0);
    let scope = in_scope(&path).len() as u64;
    assert!(pending >= 1, "offene Einträge bleiben `Pending`: {st:?}");
    if success + pending != scope {
        findings.push(format!(
            "nach SIGINT: {} Einträge (fertig oder `Pending`) für {scope} gecrawlte Seiten im Geltungsbereich: {st:?}",
            success + pending
        ));
    }
    let sent = mock.decisions();
    let after_sigint = sent.iter().filter(|r| r.timestamp_ms > sigint_ms).count();
    if after_sigint > 0 {
        findings.push(format!(
            "{after_sigint} Request(s) nach dem SIGINT gesendet"
        ));
    }
    if success as usize != sent.len() {
        findings.push(format!(
            "{} Requests gesendet, {success} übernommen: laufende Requests nicht abgewartet ({st:?})",
            sent.len()
        ));
    }

    mock.scenario("ok", 1_000).await;
    let resumed = run(&env, &["--load-crawl", &id, "--resume"]).await;
    assert_eq!(resumed.code, 0, "{}", resumed.dump());
    let st = statuses(&path);
    assert_eq!(st.get("Success").copied(), Some(22), "{st:?}");
    assert_eq!(st.get("Pending").copied().unwrap_or(0), 0, "{st:?}");
    let urls = mock.urls();
    let set: BTreeSet<&String> = urls.iter().collect();
    if set.len() != urls.len() || urls.len() != 22 {
        findings.push(format!(
            "Mock-Log nach dem Fortsetzen: {} Requests für {} URLs (erwartet 22, keine doppelt)",
            urls.len(),
            set.len()
        ));
    }
    assert!(findings.is_empty(), "{findings:#?}");
}

// spec: AI-020 [M4]
// spec: CRAWL-125 [M4]
// acceptance: independent
// (c) crash_by_sigkill: SIGKILL, während der Mock Request n hält → Eintrag n `Pending`, keine Protokollzeile, Verbrauch =
// Summe der Protokollzeilen (keine Reservierung); nach dem Absturz 0 Requests; Fortsetzen des unterbrochenen Crawls (nach
// Ablauf des Heartbeats, CRAWL-125) sendet Eintrag n genau einmal, am Ende genau eine Protokollzeile für ihn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c_crash_by_sigkill() {
    let mock = Mock::start().await;
    let site = ai_site().await;
    let env = Env::new();
    prepare(&env, &mock, &site);
    let child = spawn_crawl(&env);
    wait_until(
        "zwei beantwortete Requests",
        Duration::from_secs(60),
        || mock.decisions().len() >= 2,
    )
    .await;
    mock.scenario("hold", 1).await;
    wait_until("gehaltener Request", Duration::from_secs(60), || {
        mock.decisions().iter().any(|r| r.held)
    })
    .await;
    let held = mock
        .decisions()
        .into_iter()
        .find(|r| r.held)
        .and_then(|r| request_url(&r))
        .expect("URL des gehaltenen Requests");
    signal(&child, libc::SIGKILL);
    let _ = finish(child).await;
    let (path, id) = only_file(&env);

    assert_eq!(
        status_of(&path, &held).as_deref(),
        Some("Pending"),
        "Eintrag n `Pending`"
    );
    assert_eq!(
        logged_rows_for(&path, &held),
        0,
        "keine Protokollzeile für Eintrag n"
    );
    {
        let f = ro(&path);
        let ledger = os_store::ai::budget::open_ledger(f.conn(), Default::default(), &[]).unwrap();
        let snap = os_model::budget::BudgetLedger::snapshot(&ledger).unwrap();
        let logged: i64 = f
            .conn()
            .query_row(
                "SELECT COALESCE(SUM(cost_microusd), 0) FROM ai_requests",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(snap.reserved.0, 0, "keine Reservierung mehr");
        assert_eq!(
            snap.consumption().0,
            logged as u64,
            "Verbrauch = Summe der Protokollzeilen"
        );
    }
    let sent = mock.decisions().len();
    let listed = run(&env, &["--list-crawls", "--json"]).await;
    assert_eq!(listed.code, 0, "{}", listed.dump());
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        mock.decisions().len(),
        sent,
        "nach dem Absturz und beim Öffnen 0 Requests"
    );

    // CRAWL-125: unterbrochen erst, wenn der Heartbeat älter als 10 000 ms ist.
    tokio::time::sleep(Duration::from_millis(11_000)).await;
    let resumed = run(&env, &["--load-crawl", &id, "--resume"]).await;
    assert_eq!(resumed.code, 0, "{}", resumed.dump());
    let n = mock.urls().iter().filter(|u| **u == held).count();
    assert_eq!(
        n, 2,
        "Eintrag n: einmal gehalten (abgebrochen), einmal neu gesendet"
    );
    assert_eq!(
        logged_rows_for(&path, &held),
        1,
        "genau eine Protokollzeile für Eintrag n"
    );
    assert_eq!(status_of(&path, &held).as_deref(), Some("Success"));
    let st = statuses(&path);
    assert_eq!(st.get("Success").copied(), Some(22), "{st:?}");
}

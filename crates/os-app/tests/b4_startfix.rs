//! B4-STARTFIX: Start-Dialog „per robots.txt gesperrt“ (UI-019) und Fehler-Mail bei einem regulär als „Fehlgeschlagen“ endenden
//! Lauf (OUT-281). Beide Fälle laufen über den Produktionsweg der App (Start, Abfrage, `settle`) gegen lokale Fixtures.
#[path = "session_common.rs"]
mod common;
use common::*;
use os_app::runtime::lifecycle_bus::CrawlEvent;
use os_app::{
    dto::{Notice, NoticeLevel},
    App, AppConfig,
};
use os_fetch::secrets::MemorySecretStore;
use os_model::{CrawlConfig, CrawlState, RobotsMode};
use os_sched::mail::{Account, AccountStore, Auth, Tls};
use os_testsite::mocks::smtp::{Config, SmtpMock};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const START_CODES: [&str; 6] = [
    "start-dns",
    "start-connection",
    "start-timeout",
    "start-tls",
    "start-robots",
    "crawl-failed",
];

fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn config(mode: RobotsMode) -> CrawlConfig {
    let mut c = cfg();
    c.robots.retries = 0;
    c.robots.mode = mode;
    c
}

/// Zählt die Lauf-Enden (`Completed`, `Stopped`, `Paused`). `settle` sendet sie erst nach seinen Meldungen; erst danach liegen
/// alle Meldungen des Laufs vor (der Zustand allein ist früher gesetzt).
fn ends(app: &App) -> Arc<AtomicUsize> {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    app.crawl_events().subscribe(Box::new(move |_, event| {
        if matches!(
            event,
            CrawlEvent::Completed(_) | CrawlEvent::Stopped(_) | CrawlEvent::Paused(_)
        ) {
            seen.fetch_add(1, Ordering::SeqCst);
        }
    }));
    count
}

/// Startet einen Spider-Crawl über den Produktionsweg der App und sammelt die Meldungen bis zum Ende des Laufs (`settle`).
async fn crawl(app: &App, start_url: &str, cfg: CrawlConfig) -> (CrawlState, Vec<Notice>) {
    let ended = ends(app);
    let fetch = os_app::session::control::fetcher(app, &cfg).unwrap();
    os_app::session::control::start_with_fetcher(app, ctx(), request(start_url.into()), cfg, fetch)
        .await
        .unwrap();
    settled(app, &ended).await
}

/// Fragt die App ab, bis der Lauf beendet ist (Ereignis aus [`ends`], Engine-Handle abgegeben, Zustand nicht mehr „Läuft“), und
/// sammelt die Meldungen. „URLs hinzufügen“ meldet das Ende zusätzlich selbst (`follow_completion`), unter Umständen vor
/// `settle`; deshalb wird danach noch eine Sekunde weiter gesammelt, damit eine Meldung aus `settle` nicht unbemerkt bleibt.
async fn settled(app: &App, ended: &AtomicUsize) -> (CrawlState, Vec<Notice>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut notices = Vec::new();
    let mut grace: Option<Instant> = None;
    loop {
        os_app::session::poll(app).await.unwrap();
        if let Some(tick) = app.assemble_tick() {
            notices.extend(tick.notices);
        }
        let session = app.sessions().current(&window()).unwrap();
        let released = session
            .engine
            .as_ref()
            .is_none_or(|e| e.handle.lock().unwrap().is_none());
        let done =
            ended.load(Ordering::SeqCst) > 0 && released && session.state != CrawlState::Running;
        match grace {
            Some(until) if Instant::now() >= until => return (session.state, notices),
            None if done => grace = Some(Instant::now() + Duration::from_secs(1)),
            _ => {}
        }
        assert!(Instant::now() < deadline, "kein Ende: {notices:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn start_notices(notices: &[Notice]) -> Vec<&Notice> {
    notices
        .iter()
        .filter(|n| START_CODES.contains(&n.code.as_str()))
        .collect()
}

// Fixture mit `Disallow: /`: Der Lauf endet als „Abgeschlossen“ (CRAWL-128, kein Fehlschlag), aber nicht still. Die Meldung
// nennt die Sperre und die sperrende robots.txt; der Kern ändert die Einstellung nicht.
// spec: UI-019
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_url_blocked_by_robots_completes_and_reports_the_block() {
    let site = os_testsite::TestSite::start("ui/fx-ui-robots-start")
        .await
        .unwrap();
    let base = site.base_url();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let (state, notices) = crawl(&app, &format!("{base}/"), config(RobotsMode::Respect)).await;
    assert_eq!(state, CrawlState::Completed);
    let found = start_notices(&notices);
    assert_eq!(found.len(), 1, "genau eine Startmeldung: {notices:?}");
    let n = found[0];
    assert_eq!(n.code, "start-robots");
    assert_eq!(n.level, NoticeLevel::Warning);
    assert_eq!(n.args["startUrl"], format!("{base}/").as_str());
    assert_eq!(n.args["url"], format!("{base}/robots.txt").as_str());
    // Zeilennummer und Regeltext getrennt, ohne sprachabhängigen Präfix (die Oberfläche formuliert).
    assert_eq!(n.args["lineNumber"], 2, "{n:?}");
    assert_eq!(n.args["line"], "Disallow: /", "{n:?}");
    // Nur ein Hinweis: die Konfiguration des Crawls bleibt „robots.txt beachten“.
    let session = app.sessions().current(&window()).unwrap();
    assert_eq!(session.config.robots.mode, RobotsMode::Respect);
}

// Je Crawl und Start-URL höchstens einmal (OS-Festlegung 09.11.2026): „URLs hinzufügen“ setzt denselben Crawl fort und endet
// erneut mit der gesperrten Start-URL, die Meldung erscheint aber nicht noch einmal.
// spec: UI-019
// spec: CRAWL-131
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_robots_dialog_appears_once_per_crawl_also_after_adding_urls() {
    let site = os_testsite::TestSite::start("ui/fx-ui-robots-start")
        .await
        .unwrap();
    let base = site.base_url();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let (state, notices) = crawl(&app, &format!("{base}/"), config(RobotsMode::Respect)).await;
    assert_eq!(state, CrawlState::Completed);
    assert_eq!(start_notices(&notices).len(), 1, "{notices:?}");

    let ended = ends(&app);
    let op = call(
        &app,
        "crawl.addUrls",
        json!({"urls":[format!("{base}/page.html")]}),
    )
    .await;
    let status = app.ops().wait(op["opId"].as_str().unwrap()).await.unwrap();
    assert!(status.error.is_none(), "{:?}", status.error);
    assert_eq!(status.result.unwrap(), json!({"added":1,"skipped":0}));
    let (state, notices) = settled(&app, &ended).await;
    assert_eq!(state, CrawlState::Completed);
    assert!(start_notices(&notices).is_empty(), "{notices:?}");
}

// Mit „robots.txt ignorieren“ wird die Start-URL abgerufen; es gibt keine Startmeldung.
// spec: UI-019
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ignoring_robots_crawls_the_start_url_without_a_start_dialog() {
    let site = os_testsite::TestSite::start("ui/fx-ui-robots-start")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let (state, notices) = crawl(
        &app,
        &format!("{}/", site.base_url()),
        config(RobotsMode::Ignore),
    )
    .await;
    assert_eq!(state, CrawlState::Completed);
    assert!(start_notices(&notices).is_empty(), "{notices:?}");
}

// Nur eine Sperre per `Disallow` meldet `start-robots`; eine nicht erreichbare robots.txt bleibt ein Fehlschlag (B4-UNREACH).
// spec: UI-019
// spec: CRAWL-128
#[test]
fn the_robots_notice_carries_start_url_robots_url_and_line() {
    let n = os_app::session::control::start_robots_notice(
        "https://site.test/",
        &os_engine::StartRobotsBlock {
            robots_url: "https://site.test/robots.txt".into(),
            line_number: Some(2),
            rule: Some("Disallow: /".into()),
        },
    );
    assert_eq!(n.code, "start-robots");
    assert_eq!(n.level, NoticeLevel::Warning);
    assert_eq!(n.args["startUrl"], "https://site.test/");
    assert_eq!(n.args["url"], "https://site.test/robots.txt");
    assert_eq!(n.args["lineNumber"], 2);
    assert_eq!(n.args["line"], "Disallow: /");
}

/// App mit Mail-Konto gegen den SMTP-Mock der Testsite und den Benachrichtigungs-Einstellungen.
async fn mail_app(
    home: &std::path::Path,
    mock: &SmtpMock,
    on_success: bool,
    on_failure: bool,
) -> App {
    let secrets = Arc::new(MemorySecretStore::new());
    let app = App::new(AppConfig::new(home).with_secrets(secrets.clone())).unwrap();
    AccountStore::new(&app.config().paths.settings)
        .save(
            &Account {
                id: "fixture".into(),
                label: "Mock".into(),
                host: "localhost".into(),
                port: mock.addr.port(),
                tls: Tls::None,
                username: "crawler".into(),
                from: "crawler@site.test".into(),
                auth: Auth::Password,
                allow_insecure: true,
                client_id: None,
            },
            Some("fixture-password"),
            secrets.as_ref(),
        )
        .unwrap();
    call(
        &app,
        "settings.set",
        json!({"patch":{"accounts":{"notifyAccount":"fixture","notifyTo":["recipient@site.test"],
            "notifyOnSuccess":on_success,"notifyOnFailure":on_failure},"network":{"proxy":{"mode":"none"}}}}),
    )
    .await;
    app
}

async fn messages_after(mock: &SmtpMock, wait: Duration, expected: usize) -> Vec<String> {
    let deadline = Instant::now() + wait;
    while mock.messages().len() < expected && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Eine zweite (Erfolgs-)Mail käme aus demselben Abschluss; kurz nachwarten, damit sie nicht unbemerkt bleibt.
    tokio::time::sleep(Duration::from_millis(500)).await;
    mock.messages().into_iter().map(|m| m.data).collect()
}

// Planer-Nutzer mit „Mail bei Erfolg“ aus und „bei Fehlschlag“ an (Standard): Die nicht erreichbare Start-URL beendet den Lauf
// als „Fehlgeschlagen“ (CRAWL-128) und erzeugt genau eine Fehler-Mail mit Ursache, keine Erfolgsmail.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_ending_as_failed_sends_exactly_one_failure_mail() {
    let mock = SmtpMock::start(Config::default()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = mail_app(dir.path(), &mock, false, true).await;
    let start = format!("http://127.0.0.1:{}/", closed_port());
    let (state, _) = crawl(&app, &start, config(RobotsMode::Ignore)).await;
    assert_eq!(state, CrawlState::Failed);
    let messages = messages_after(&mock, Duration::from_secs(10), 1).await;
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("[FAILED]"), "{}", messages[0]);
    assert!(
        messages[0].contains("connection_refused"),
        "{}",
        messages[0]
    );
}

// Gegenprobe: Mit „Mail bei Erfolg“ an und „bei Fehlschlag“ aus geht für den fehlgeschlagenen Lauf keine Mail hinaus – auch keine
// Erfolgsmail. Ein per robots.txt gesperrter Start ist kein Fehlschlag (CRAWL-128) und erhält die Erfolgsmail.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_run_sends_no_success_mail_but_a_robots_block_does() {
    let mock = SmtpMock::start(Config::default()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = mail_app(dir.path(), &mock, true, false).await;
    let start = format!("http://127.0.0.1:{}/", closed_port());
    let (state, _) = crawl(&app, &start, config(RobotsMode::Ignore)).await;
    assert_eq!(state, CrawlState::Failed);
    let messages = messages_after(&mock, Duration::from_secs(3), 1).await;
    assert!(messages.is_empty(), "{messages:?}");

    let site = os_testsite::TestSite::start("ui/fx-ui-robots-start")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = mail_app(dir.path(), &mock, true, false).await;
    let (state, _) = crawl(
        &app,
        &format!("{}/", site.base_url()),
        config(RobotsMode::Respect),
    )
    .await;
    assert_eq!(state, CrawlState::Completed);
    let messages = messages_after(&mock, Duration::from_secs(10), 1).await;
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(!messages[0].contains("[FAILED]"), "{}", messages[0]);
}

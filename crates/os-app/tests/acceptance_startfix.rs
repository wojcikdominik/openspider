//! Unabhängige Abnahmetests B4-STARTFIX (App-Kern): Verhalten, wenn ein Crawl an seiner Start-URL scheitert oder die robots.txt die
//! Start-URL sperrt.
//!
//! Erwartetes Verhalten stammt ausschließlich aus der Spezifikation (UI-019, CRAWL-128, CRAWL-131, OUT-281, OUT-282 samt der
//! OS-Festlegungen vom 09.11.2026); der Produktcode wurde nur gelesen, um zu wissen, wie die Schnittstellen aufgerufen werden.
//! Geprüft wird über den Weg der App (Start, Abfrage, Ende des Laufs) gegen lokale Fixtures und den SMTP-Mock der Testsite. Die
//! Namen der Meldungsargumente sind in der Spec nicht festgelegt; die Tests lesen deshalb Werte, nicht Schlüssel.
//!
//! Kriterienliste: `docs/plan/M3-reports/b4-startfix-tests.md`.
// Der Test liest die Crawl-Datei von außen (Beleg, dass „URLs hinzufügen“ wirklich abgerufen hat), wie die Abnahmetests anderer Pakete.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
#[path = "acceptance_startfix/fixtures.rs"]
mod fixtures;
#[path = "acceptance_startfix/mime.rs"]
mod mime;

use common::*;
use fixtures::{robots_500_fixture, robots_fixture, RULE, RULE_LINE};
use os_app::runtime::lifecycle_bus::CrawlEvent;
use os_app::{
    dto::{Notice, NoticeLevel},
    App, AppConfig,
};
use os_fetch::secrets::MemorySecretStore;
use os_model::{CrawlConfig, CrawlState, RobotsMode};
use os_sched::mail::{Account, AccountStore, Auth, Tls};
use os_testsite::mocks::smtp::{Config, SmtpMock};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

// ---------------------------------------------------------------------------------------------------------
// Hilfen: Netz, Fixtures
// ---------------------------------------------------------------------------------------------------------

fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Dauer, nach der das Betriebssystem einen Verbindungsaufbau zu dem nicht belegten Port ablehnt.
fn refusal_time(port: u16) -> Duration {
    let begin = Instant::now();
    let refused = std::net::TcpStream::connect(("127.0.0.1", port));
    assert!(refused.is_err(), "Port {port} ist belegt");
    begin.elapsed()
}

/// Nimmt Verbindungen an und antwortet nie. Hält fest, wann der Client jede Verbindung wieder schließt: Das ist das Ende des
/// jeweiligen Abrufversuchs (UI-019: „2 s nach dem Timeout, gemessen ab dem Ende des letzten Abrufversuchs“).
fn silent_server() -> (u16, Arc<Mutex<Vec<Instant>>>) {
    use std::io::Read;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let closes = Arc::new(Mutex::new(Vec::new()));
    let seen = closes.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let seen = seen.clone();
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut buf = [0u8; 1024];
                // Gelesen wird bis zum Ende der Verbindung; Anfragedaten des Clients sind uninteressant.
                while matches!(stream.read(&mut buf), Ok(n) if n > 0) {}
                seen.lock().unwrap().push(Instant::now());
            });
        }
    });
    (port, closes)
}

async fn robots_site() -> (tempfile::TempDir, os_testsite::TestSite) {
    let dir = robots_fixture();
    let site = os_testsite::TestSite::start_dir(dir.path(), 0)
        .await
        .unwrap();
    (dir, site)
}

fn config(mode: RobotsMode) -> CrawlConfig {
    let mut c = cfg();
    c.robots.retries = 0;
    c.robots.mode = mode;
    c
}

// ---------------------------------------------------------------------------------------------------------
// Hilfen: Lauf starten und bis zum Ende beobachten
// ---------------------------------------------------------------------------------------------------------

/// Zählt die Lauf-Enden (`Completed`, `Stopped`, `Paused`). Die Ereignisse folgen den Meldungen des Laufs; erst danach liegen
/// alle Meldungen vor.
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

struct Seen {
    state: CrawlState,
    /// Alle Meldungen des Laufs mit dem Zeitpunkt, zu dem die Abfrage sie erstmals lieferte.
    notices: Vec<(Instant, Notice)>,
}

impl Seen {
    fn all(&self) -> Vec<&Notice> {
        self.notices.iter().map(|(_, n)| n).collect()
    }
}

/// Fragt die App ab, bis der Lauf beendet ist (Ende-Ereignis, Engine-Handle abgegeben), und sammelt danach noch eine Sekunde
/// weiter, damit auch eine spät gesendete Meldung nicht unbemerkt bleibt (z. B. eine zweite Meldung desselben Laufs).
async fn watch(app: &App, ended: &AtomicUsize) -> Seen {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut notices: Vec<(Instant, Notice)> = Vec::new();
    let mut grace: Option<Instant> = None;
    loop {
        os_app::session::poll(app).await.unwrap();
        if let Some(tick) = app.assemble_tick() {
            let now = Instant::now();
            notices.extend(tick.notices.into_iter().map(|n| (now, n)));
        }
        let session = app.sessions().current(&window()).unwrap();
        let released = session
            .engine
            .as_ref()
            .is_none_or(|e| e.handle.lock().unwrap().is_none());
        let done =
            ended.load(Ordering::SeqCst) > 0 && released && session.state != CrawlState::Running;
        match grace {
            Some(until) if Instant::now() >= until => {
                return Seen {
                    state: session.state,
                    notices,
                }
            }
            None if done => grace = Some(Instant::now() + Duration::from_secs(1)),
            _ => {}
        }
        assert!(Instant::now() < deadline, "kein Ende: {notices:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Startet einen Spider-Crawl und beobachtet ihn bis zum Ende. Liefert zusätzlich den Zeitpunkt nach dem Start.
async fn crawl(app: &App, start_url: &str, cfg: CrawlConfig) -> (Seen, Instant) {
    let ended = ends(app);
    let fetch = os_app::session::control::fetcher(app, &cfg).unwrap();
    os_app::session::control::start_with_fetcher(app, ctx(), request(start_url.into()), cfg, fetch)
        .await
        .unwrap();
    let started = Instant::now();
    (watch(app, &ended).await, started)
}

/// „URLs hinzufügen“ für den aktuellen Crawl; wartet das Ende der Operation und des erneuten Laufs ab.
async fn add_urls(app: &App, urls: &[String]) -> Seen {
    let ended = ends(app);
    let op = call(app, "crawl.addUrls", json!({ "urls": urls })).await;
    let status = app.ops().wait(op["opId"].as_str().unwrap()).await.unwrap();
    assert!(status.error.is_none(), "{:?}", status.error);
    watch(app, &ended).await
}

// ---------------------------------------------------------------------------------------------------------
// Hilfen: Meldungen
// ---------------------------------------------------------------------------------------------------------

/// Alle Werte der Argumente (die Schlüssel legt die Spec nicht fest), Zeichenketten und Zahlen.
fn values(n: &Notice) -> Vec<Value> {
    let v = serde_json::to_value(n).unwrap();
    v["args"]
        .as_object()
        .map(|o| o.values().cloned().collect())
        .unwrap_or_default()
}

fn mentions(n: &Notice, needle: &str) -> bool {
    serde_json::to_value(n)
        .unwrap()
        .to_string()
        .to_lowercase()
        .contains(&needle.to_lowercase())
}

/// Meldungen, die eine robots.txt-Sperre der Start-URL nennen (die robots.txt-URL kommt im Wortlaut vor).
fn robots_notices<'a>(seen: &'a Seen, robots_url: &str) -> Vec<&'a Notice> {
    seen.all()
        .into_iter()
        .filter(|n| mentions(n, robots_url))
        .collect()
}

// ---------------------------------------------------------------------------------------------------------
// Hilfen: Mail
// ---------------------------------------------------------------------------------------------------------

async fn mail_app(
    home: &Path,
    mock: &SmtpMock,
    on_success: bool,
    on_failure: bool,
    recipients: &[&str],
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
        json!({"patch":{"accounts":{"notifyAccount":"fixture","notifyTo":recipients,
            "notifyOnSuccess":on_success,"notifyOnFailure":on_failure},"network":{"proxy":{"mode":"none"}}}}),
    )
    .await;
    app
}

/// Wartet auf `expected` Nachrichten (höchstens `wait`) und danach, ob weitere folgen (eine zweite Mail desselben Laufs).
async fn messages(mock: &SmtpMock, wait: Duration, expected: usize) -> Vec<mime::Message> {
    let deadline = Instant::now() + wait;
    while mock.messages().len() < expected && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    mock.messages()
        .into_iter()
        .map(|m| mime::parse(&m.data))
        .collect()
}

fn has_any(text: &str, needles: &[&str]) -> bool {
    let lower = text.to_lowercase();
    needles.iter().any(|n| lower.contains(&n.to_lowercase()))
}

const REFUSED: [&str; 4] = [
    "connection_refused",
    "connection refused",
    "verbindung abgelehnt",
    "abgelehnt",
];

/// Prüft eine Fehler-Mail: Betreff-Präfix, Ursache (Fehlerklasse) und Adresse in Klartext und HTML (OUT-281, OUT-282).
fn assert_failure_mail(m: &mime::Message, address: &str, class: &[&str]) {
    assert!(m.subject.starts_with("[FAILED]"), "Betreff: {}", m.subject);
    assert!(
        !m.plain.is_empty() && !m.html.is_empty(),
        "HTML und Klartext erwartet: {}",
        m.raw
    );
    for (kind, parts) in [("Klartext", &m.plain), ("HTML", &m.html)] {
        let text = parts.join("\n");
        assert!(text.contains(address), "{kind} ohne {address}: {text}");
        assert!(
            has_any(&text, class),
            "{kind} ohne Fehlerklasse {class:?}: {text}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------
// OUT-281 / CRAWL-128: Fehler-Mail
// ---------------------------------------------------------------------------------------------------------

// Start-URL nicht erreichbar: Der Lauf endet als „Fehlgeschlagen“ (CRAWL-128). Je Lauf genau eine Mail, und zwar die Fehler-Mail
// mit Ursache, Fehlerklasse und Adresse; nie zusätzlich eine Erfolgsmail. Ob überhaupt eine Mail geht, entscheidet allein
// „bei Fehlschlag“; „bei Erfolg“ ist für einen fehlgeschlagenen Lauf unerheblich.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_ending_as_failed_sends_exactly_one_failure_mail_when_failure_mails_are_on() {
    for on_success in [true, false] {
        let mock = SmtpMock::start(Config::default()).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = mail_app(
            dir.path(),
            &mock,
            on_success,
            true,
            &["recipient@site.test"],
        )
        .await;
        let port = closed_port();
        let (seen, _) = crawl(
            &app,
            &format!("http://127.0.0.1:{port}/"),
            config(RobotsMode::Ignore),
        )
        .await;
        assert_eq!(seen.state, CrawlState::Failed);
        let mails = messages(&mock, Duration::from_secs(10), 1).await;
        assert_eq!(mails.len(), 1, "bei Erfolg {on_success}: {mails:?}");
        assert_failure_mail(&mails[0], &format!("127.0.0.1:{port}"), &REFUSED);
        assert!(
            mock.messages()[0]
                .to
                .iter()
                .any(|t| t.contains("recipient@site.test")),
            "{:?}",
            mock.messages()[0].to
        );
    }
}

// Gegenprobe zur Mail-Technik: Ein Erfolg mit „bei Erfolg“ an sendet seine Mail (ohne Präfix) – sonst wären die Aussagen „keine
// Mail“ unten wertlos. Mit „bei Fehlschlag“ aus geht für den fehlgeschlagenen Lauf nichts hinaus, auch keine Erfolgsmail.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_run_sends_no_mail_when_failure_mails_are_off_even_with_success_mails_on() {
    let (_site_dir, site) = robots_site().await;
    for (on_success, on_failure) in [(true, false), (false, false)] {
        let mock = SmtpMock::start(Config::default()).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = mail_app(dir.path(), &mock, on_success, on_failure, &["r@site.test"]).await;

        // Kontrolle: ein erfolgreicher Lauf wird mit „bei Erfolg“ gemeldet.
        let (seen, _) = crawl(
            &app,
            &format!("{}/", site.base_url()),
            config(RobotsMode::Respect),
        )
        .await;
        assert_eq!(seen.state, CrawlState::Completed);
        let mails = messages(&mock, Duration::from_secs(10), usize::from(on_success)).await;
        assert_eq!(mails.len(), usize::from(on_success), "{mails:?}");
        if let Some(m) = mails.first() {
            assert!(!m.subject.starts_with("[FAILED]"), "{}", m.subject);
        }
        let before = mails.len();

        let (seen, _) = crawl(
            &app,
            &format!("http://127.0.0.1:{}/", closed_port()),
            config(RobotsMode::Ignore),
        )
        .await;
        assert_eq!(seen.state, CrawlState::Failed);
        let mails = messages(&mock, Duration::from_secs(3), before + 1).await;
        assert_eq!(
            mails.len(),
            before,
            "(Erfolg {on_success}, Fehlschlag {on_failure}) für den fehlgeschlagenen Lauf: {mails:?}"
        );
    }
}

// Ohne Empfänger wird keine Mail gesendet, auch nicht bei einem Fehlschlag.
// spec: OUT-281
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_run_without_recipients_sends_no_mail() {
    let mock = SmtpMock::start(Config::default()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = mail_app(dir.path(), &mock, true, true, &[]).await;
    let (seen, _) = crawl(
        &app,
        &format!("http://127.0.0.1:{}/", closed_port()),
        config(RobotsMode::Ignore),
    )
    .await;
    assert_eq!(seen.state, CrawlState::Failed);
    let mails = messages(&mock, Duration::from_secs(2), 1).await;
    assert!(mails.is_empty(), "{mails:?}");
}

// Auch eine nicht erreichbare robots.txt (Standard „robots.txt beachten“) lässt den Lauf fehlschlagen (CRAWL-128): eine
// Fehler-Mail, die die Adresse nennt.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreachable_robots_txt_also_sends_one_failure_mail_with_the_address() {
    let mock = SmtpMock::start(Config::default()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = mail_app(dir.path(), &mock, true, true, &["r@site.test"]).await;
    let port = closed_port();
    let (seen, _) = crawl(
        &app,
        &format!("http://127.0.0.1:{port}/"),
        config(RobotsMode::Respect),
    )
    .await;
    assert_eq!(seen.state, CrawlState::Failed);
    let mails = messages(&mock, Duration::from_secs(10), 1).await;
    assert_eq!(mails.len(), 1, "{mails:?}");
    assert_failure_mail(&mails[0], &format!("127.0.0.1:{port}"), &REFUSED);
}

// Eine andere Fehlerklasse (DNS): Die Mail nennt die Klasse und die Adresse. (`.invalid` ist nach RFC 6761 nie auflösbar.)
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_mail_names_the_dns_error_class_and_the_host() {
    let mock = SmtpMock::start(Config::default()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = mail_app(dir.path(), &mock, false, true, &["r@site.test"]).await;
    let (seen, _) = crawl(
        &app,
        "http://openspider-startfix-test.invalid/",
        config(RobotsMode::Ignore),
    )
    .await;
    assert_eq!(seen.state, CrawlState::Failed);
    let mails = messages(&mock, Duration::from_secs(10), 1).await;
    assert_eq!(mails.len(), 1, "{mails:?}");
    assert_failure_mail(
        &mails[0],
        "openspider-startfix-test.invalid",
        &["dns", "auflösung", "resolve"],
    );
}

// Die Sperre per robots.txt ist kein Fehlschlag (CRAWL-128): Der Lauf endet als „Abgeschlossen“ und wird wie ein Erfolg gemeldet
// (genau eine Mail, ohne Präfix `[FAILED]`); eine robots.txt, die mit 500 antwortet, ebenso.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_robots_block_and_a_robots_500_are_reported_as_successes_not_failures() {
    let (_dir, site) = robots_site().await;
    let dir500 = robots_500_fixture();
    let site500 = os_testsite::TestSite::start_dir(dir500.path(), 0)
        .await
        .unwrap();
    for start in [
        format!("{}/secret/index.html", site.base_url()),
        format!("{}/", site500.base_url()),
    ] {
        let mock = SmtpMock::start(Config::default()).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = mail_app(dir.path(), &mock, true, true, &["r@site.test"]).await;
        let (seen, _) = crawl(&app, &start, config(RobotsMode::Respect)).await;
        assert_eq!(seen.state, CrawlState::Completed, "{start}");
        let mails = messages(&mock, Duration::from_secs(10), 1).await;
        assert_eq!(mails.len(), 1, "{start}: {mails:?}");
        assert!(!mails[0].subject.starts_with("[FAILED]"), "{start}");
    }
}

// ---------------------------------------------------------------------------------------------------------
// UI-019: Dialog zur nicht erreichbaren Start-URL (Zeitrahmen, Ursache)
// ---------------------------------------------------------------------------------------------------------

// Nicht belegter Port: Die Meldung nennt die Fehlerklasse und erscheint innerhalb von 2 s nach dem Ende des Abrufs. Wie lange
// das Betriebssystem für die Ablehnung braucht, misst der Test vorab selbst (Linux und macOS lehnen sofort ab, Windows erst nach
// rund 2 s); dieser Zeitpunkt nach dem Start ist das früheste Ende des Abrufversuchs.
// spec: UI-019
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreachable_start_url_shows_a_notice_with_the_error_class_within_two_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let port = closed_port();
    let refusal = refusal_time(port);
    let (seen, started) = crawl(
        &app,
        &format!("http://127.0.0.1:{port}/"),
        config(RobotsMode::Ignore),
    )
    .await;
    let fetch_end = started + refusal;
    assert_eq!(seen.state, CrawlState::Failed);
    let found: Vec<_> = seen
        .notices
        .iter()
        .filter(|(_, n)| mentions(n, &format!("127.0.0.1:{port}")))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "genau eine Meldung zur Start-URL: {seen:?}",
        seen = seen.all()
    );
    let (at, notice) = found[0];
    assert!(
        matches!(notice.level, NoticeLevel::Error | NoticeLevel::Warning),
        "{notice:?}"
    );
    assert!(has_any(&format!("{notice:?}"), &REFUSED), "{notice:?}");
    assert!(
        at.saturating_duration_since(fetch_end) < Duration::from_secs(2),
        "Meldung {:?} nach dem Ende des Abrufs (Ablehnung dauert hier {refusal:?})",
        at.saturating_duration_since(fetch_end)
    );
}

// Server, der nie antwortet (Antwort-Timeout 1 s): Die Meldung erscheint höchstens 2 s nach dem Ende des letzten Abrufversuchs
// (der Client schließt die Verbindung), nicht erst 2 s nach dem Start des Crawls (OS-Festlegung 09.11.2026).
// spec: UI-019
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_timeout_notice_comes_within_two_seconds_of_the_last_fetch_attempt() {
    let (port, closes) = silent_server();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let mut c = config(RobotsMode::Ignore);
    c.advanced.response_timeout_s = 1;
    let (seen, _) = crawl(&app, &format!("http://127.0.0.1:{port}/"), c).await;
    assert_eq!(seen.state, CrawlState::Failed);
    let found: Vec<_> = seen
        .notices
        .iter()
        .filter(|(_, n)| mentions(n, &format!("127.0.0.1:{port}")))
        .collect();
    assert_eq!(found.len(), 1, "{:?}", seen.all());
    let (at, notice) = found[0];
    assert!(mentions(notice, "timeout"), "{notice:?}");
    let last_close = *closes
        .lock()
        .unwrap()
        .iter()
        .max()
        .expect("der Client hat sich verbunden");
    assert!(
        at.saturating_duration_since(last_close) < Duration::from_secs(2),
        "Meldung {:?} nach dem letzten Abruf",
        at.saturating_duration_since(last_close)
    );
}

// ---------------------------------------------------------------------------------------------------------
// UI-019 / CRAWL-128: Start-URL per robots.txt gesperrt
// ---------------------------------------------------------------------------------------------------------

// Sperrt die robots.txt die Start-URL, nennt die Meldung die robots.txt und die sperrende Zeile: Zeilennummer und Regeltext
// getrennt und ohne sprachabhängigen Präfix (Formulierung kommt aus der Oberfläche). Der Lauf bleibt „Abgeschlossen“.
// spec: UI-019
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blocked_start_url_reports_robots_txt_line_number_and_rule_separately() {
    let (_dir, site) = robots_site().await;
    let base = site.base_url();
    let robots_url = format!("{base}/robots.txt");
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let (seen, _) = crawl(
        &app,
        &format!("{base}/secret/index.html"),
        config(RobotsMode::Respect),
    )
    .await;
    assert_eq!(seen.state, CrawlState::Completed);
    let found = robots_notices(&seen, &robots_url);
    assert_eq!(found.len(), 1, "{:?}", seen.all());
    let vals = values(found[0]);
    // Zeilennummer als Zahl, Regeltext als eigener Wert.
    assert!(
        vals.iter().any(|v| v.as_u64() == Some(RULE_LINE)),
        "Zeilennummer {RULE_LINE} fehlt: {vals:?}"
    );
    assert!(
        vals.iter().any(|v| v.as_str() == Some(RULE)),
        "Regeltext {RULE:?} fehlt: {vals:?}"
    );
    // Kein sprachabhängiger Präfix im Regeltext und keine zusammengesetzte Anzeigezeile.
    for v in vals.iter().filter_map(Value::as_str) {
        let lower = v.to_lowercase();
        assert!(
            !(lower.starts_with("zeile ") || lower.starts_with("line "))
                && !v.contains(": Disallow"),
            "sprachabhängige Anzeigeform im Kern: {v:?}"
        );
    }
    // Nur ein Hinweis: Die Konfiguration bleibt „robots.txt beachten“ (UI-019).
    let session = app.sessions().current(&window()).unwrap();
    assert_eq!(session.config.robots.mode, RobotsMode::Respect);
}

// Gegenprobe: Ist die Start-URL erlaubt, auch wenn ein verlinkter Pfad gesperrt ist, oder erlaubt eine spezifischere Regel sie
// trotz Sperre des Verzeichnisses, gibt es keine Sperr-Meldung.
// spec: UI-019
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_allowed_start_url_shows_no_robots_notice() {
    let (_dir, site) = robots_site().await;
    let base = site.base_url();
    let robots_url = format!("{base}/robots.txt");
    for path in ["/", "/secret/open.html"] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let (seen, _) = crawl(&app, &format!("{base}{path}"), config(RobotsMode::Respect)).await;
        assert_eq!(seen.state, CrawlState::Completed, "{path}");
        assert!(
            robots_notices(&seen, &robots_url).is_empty(),
            "{path}: {:?}",
            seen.all()
        );
    }
}

// Ist die Sperre der Start-URL zwar vorhanden, aber „robots.txt ignorieren“ eingestellt, wird die Start-URL abgerufen: keine
// Meldung.
// spec: UI-019
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ignoring_robots_txt_crawls_the_blocked_start_url_without_a_notice() {
    let (_dir, site) = robots_site().await;
    let base = site.base_url();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let (seen, _) = crawl(
        &app,
        &format!("{base}/secret/index.html"),
        config(RobotsMode::Ignore),
    )
    .await;
    assert_eq!(seen.state, CrawlState::Completed);
    assert!(
        robots_notices(&seen, &format!("{base}/robots.txt")).is_empty(),
        "{:?}",
        seen.all()
    );
}

/// Die einzige Crawl-Datei unter `home`.
fn crawl_file(home: &Path) -> PathBuf {
    let mut found = Vec::new();
    let mut stack = vec![home.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "ospider") {
                found.push(path);
            }
        }
    }
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

fn status_of(file: &Path, address_suffix: &str) -> Option<i64> {
    let file = os_store::CrawlFile::open(file, os_store::OpenMode::ReadOnly).unwrap();
    let mut stmt = file
        .conn()
        .prepare("SELECT status_code FROM q_urls WHERE address LIKE ?1")
        .unwrap();
    let mut rows = stmt
        .query_map([format!("%{address_suffix}")], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .unwrap();
    rows.next().map(|r| r.unwrap().unwrap_or(-1))
}

// Der Dialog erscheint je Crawl und Start-URL höchstens einmal: nicht erneut nach „URLs hinzufügen“ für denselben Crawl und auch
// nicht, nachdem die App neu gestartet und der Crawl wieder geöffnet wurde (OS-Festlegung 09.11.2026). Dass „URLs hinzufügen“
// wirklich einen weiteren Lauf ausgelöst hat, belegen die abgerufenen Seiten in der Crawl-Datei.
// spec: UI-019
// spec: CRAWL-131
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_blocked_notice_appears_once_per_crawl_also_after_adding_urls_and_after_a_restart() {
    let (_dir, site) = robots_site().await;
    let base = site.base_url();
    let robots_url = format!("{base}/robots.txt");
    let home = tempfile::tempdir().unwrap();
    {
        let app = app(home.path());
        let (seen, _) = crawl(
            &app,
            &format!("{base}/secret/index.html"),
            config(RobotsMode::Respect),
        )
        .await;
        assert_eq!(seen.state, CrawlState::Completed);
        assert_eq!(
            robots_notices(&seen, &robots_url).len(),
            1,
            "{:?}",
            seen.all()
        );

        // „URLs hinzufügen“ im selben Lauf der App.
        let again = add_urls(&app, &[format!("{base}/open.html")]).await;
        assert_eq!(again.state, CrawlState::Completed);
        assert!(
            robots_notices(&again, &robots_url).is_empty(),
            "erneute Meldung nach „URLs hinzufügen“: {:?}",
            again.all()
        );
        assert_eq!(
            status_of(&crawl_file(home.path()), "/open.html"),
            Some(200),
            "die hinzugefügte URL wurde nicht abgerufen"
        );
    }

    // Neustart: neue App-Instanz auf demselben Datenordner, derselbe Crawl wird geöffnet.
    let app = app(home.path());
    let path = crawl_file(home.path());
    open(&app, &path).await;
    let after_restart = add_urls(&app, &[format!("{base}/secret/open.html")]).await;
    assert_eq!(after_restart.state, CrawlState::Completed);
    assert!(
        robots_notices(&after_restart, &robots_url).is_empty(),
        "erneute Meldung nach Neustart: {:?}",
        after_restart.all()
    );
    assert_eq!(
        status_of(&path, "/secret/open.html"),
        Some(200),
        "die nach dem Neustart hinzugefügte URL wurde nicht abgerufen"
    );
}

// „Je Crawl“: Ein neuer Crawl mit derselben gesperrten Start-URL meldet die Sperre wieder – einmal. (Schützt vor einer zu
// weitgehenden Unterdrückung, etwa über App-weite Merker.)
// spec: UI-019
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_new_crawl_with_a_blocked_start_url_reports_the_block_once() {
    let (_dir, site) = robots_site().await;
    let base = site.base_url();
    let robots_url = format!("{base}/robots.txt");
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let start = format!("{base}/secret/index.html");
    for round in 1..=2 {
        let (seen, _) = crawl(&app, &start, config(RobotsMode::Respect)).await;
        assert_eq!(seen.state, CrawlState::Completed, "Crawl {round}");
        assert_eq!(
            robots_notices(&seen, &robots_url).len(),
            1,
            "Crawl {round}: {:?}",
            seen.all()
        );
    }
}

// Eine robots.txt, die mit 500 antwortet, ist kein Fehlschlag (CRAWL-128): „Abgeschlossen“. Es gibt keinen Dialog zur nicht
// erreichbaren Start-URL.
// spec: CRAWL-128
// spec: UI-019
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_robots_txt_answering_500_completes_without_an_unreachable_notice() {
    let dir500 = robots_500_fixture();
    let site = os_testsite::TestSite::start_dir(dir500.path(), 0)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let (seen, _) = crawl(
        &app,
        &format!("{}/", site.base_url()),
        config(RobotsMode::Respect),
    )
    .await;
    assert_eq!(seen.state, CrawlState::Completed);
    assert!(
        !seen.all().iter().any(|n| {
            has_any(
                &format!("{n:?}"),
                &[
                    "refused",
                    "timeout",
                    "dns",
                    "start-connection",
                    "crawl-failed",
                ],
            )
        }),
        "{:?}",
        seen.all()
    );
}

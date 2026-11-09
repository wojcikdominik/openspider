//! B4-UNREACH: Dialog zur nicht erreichbaren Start-URL (UI-019). Endet ein Lauf als „Fehlgeschlagen“, weil die Start-URL oder
//! ihre robots.txt am Netz scheiterte (CRAWL-128), sendet os-app eine Meldung mit der Fehlerklasse aus der Crawl-Datei; eine
//! robots.txt mit 500 ist kein Fehlschlag und bleibt ohne diese Meldung.
#[path = "session_common.rs"]
mod common;
use common::*;
use os_app::dto::{CrawlPhase, Notice};
use os_model::{CrawlConfig, CrawlState, NoResponseReason, RobotsMode};
use std::time::{Duration, Instant};

const START_CODES: [&str; 4] = [
    "start-dns",
    "start-connection",
    "start-timeout",
    "start-tls",
];

fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Nimmt Verbindungen an und antwortet nie (Antwort-Timeout).
fn silent_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            held.push(stream);
        }
    });
    port
}

fn config(respect_robots: bool) -> CrawlConfig {
    let mut c = cfg();
    c.robots.retries = 0;
    if !respect_robots {
        c.robots.mode = RobotsMode::Ignore;
    }
    c
}

struct Outcome {
    notices: Vec<Notice>,
    state: CrawlState,
    /// Zeit vom Start bis zur Meldung (bzw. bis zum Endzustand ohne Meldung).
    elapsed: Duration,
    /// Zeit vom Start, bis der Fortschritt der Engine die abgeschlossene Start-URL meldete (Ende des Abrufs, z. B. Timeout).
    fetched: Option<Duration>,
    /// Zeit vom Start, bis der Fortschritt der Engine den Endzustand meldete.
    ended: Option<Duration>,
}

/// Startet einen Spider-Crawl über den Produktionsweg der App und sammelt die Meldungen, bis der Lauf beendet ist und – bei
/// „Fehlgeschlagen“ – die Startfehler-Meldung vorliegt.
async fn crawl(start_url: &str, cfg: CrawlConfig) -> Outcome {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let fetch = os_app::session::control::fetcher(&app, &cfg).unwrap();
    let started = Instant::now();
    os_app::session::control::start_with_fetcher(
        &app,
        ctx(),
        request(start_url.into()),
        cfg,
        fetch,
    )
    .await
    .unwrap();
    let marks = std::sync::Arc::new(std::sync::Mutex::new((None, None)));
    {
        let mut watch = app
            .sessions()
            .current(&window())
            .unwrap()
            .engine
            .unwrap()
            .handle
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .watch();
        let marks = marks.clone();
        tokio::spawn(async move {
            loop {
                {
                    let p = watch.borrow_and_update();
                    let mut m = marks.lock().unwrap();
                    if p.crawled >= 1 && m.0.is_none() {
                        m.0 = Some(started.elapsed());
                    }
                    if p.state != CrawlState::Running && m.1.is_none() {
                        m.1 = Some(started.elapsed());
                    }
                }
                if watch.changed().await.is_err() {
                    break;
                }
            }
        });
    }
    let mut notices = Vec::new();
    let mut phase = None;
    let deadline = started + Duration::from_secs(30);
    loop {
        os_app::session::poll(&app).await.unwrap();
        if let Some(tick) = app.assemble_tick() {
            notices.extend(tick.notices);
            if let Some(c) = tick.crawl {
                phase = Some(c.phase);
            }
        }
        let state = app.sessions().current(&window()).unwrap().state;
        let reported = notices
            .iter()
            .any(|n| START_CODES.contains(&n.code.as_str()) || n.code == "crawl-failed");
        if state == CrawlState::Completed || state == CrawlState::Failed && reported {
            if state == CrawlState::Failed {
                // Die Statusleiste zeigt dieselbe Phase wie der Dialog (UI-013).
                assert_eq!(phase, Some(CrawlPhase::Failed));
            }
            let (fetched, ended) = *marks.lock().unwrap();
            let o = Outcome {
                notices,
                state,
                elapsed: started.elapsed(),
                fetched,
                ended,
            };
            eprintln!(
                "{start_url}: Abruf beendet {:?}, Engine beendet {:?}, Meldung {:?}",
                o.fetched, o.ended, o.elapsed
            );
            return o;
        }
        assert!(
            Instant::now() < deadline,
            "kein Ende: {state:?} {notices:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Zeit vom Ende des Abrufs der Start-URL (bzw. ihrer robots.txt) bis zur Meldung.
fn after_fetch(o: &Outcome) -> Duration {
    o.elapsed
        .saturating_sub(o.fetched.or(o.ended).expect("die Engine meldete ein Ende"))
}

fn start_notice(o: &Outcome) -> &Notice {
    let found: Vec<_> = o
        .notices
        .iter()
        .filter(|n| START_CODES.contains(&n.code.as_str()) || n.code == "crawl-failed")
        .collect();
    assert_eq!(
        found.len(),
        1,
        "genau eine Startfehler-Meldung: {:?}",
        o.notices
    );
    found[0]
}

// Start-URL auf nicht belegtem Port, robots.txt ignoriert: Ursache aus der Start-URL-Zeile.
// spec: UI-019
// spec: CRAWL-128
#[tokio::test]
async fn a_closed_port_reports_the_refused_connection_of_the_start_url_within_two_seconds() {
    let start = format!("http://127.0.0.1:{}/", closed_port());
    let o = crawl(&start, config(false)).await;
    assert_eq!(o.state, CrawlState::Failed);
    let n = start_notice(&o);
    assert_eq!(n.level, os_app::dto::NoticeLevel::Error);
    assert_eq!(n.code, "start-connection");
    assert_eq!(n.args["reason"], "connection_refused");
    assert_eq!(n.args["robots"], false);
    assert_eq!(n.args["url"], start.as_str());
    assert_eq!(n.args["startUrl"], start.as_str());
    assert!(after_fetch(&o) < Duration::from_secs(2), "{:?}", o.elapsed);
    // Die Verweigerung kommt sofort: alles bis zum Dialog bleibt unter 2 s.
    assert!(o.elapsed < Duration::from_secs(2), "{:?}", o.elapsed);
}

// robots.txt per Netzfehler nicht erreichbar (Standard „robots.txt beachten“, Regel „Host sperren“): Ursache aus der Hilfszeile.
// spec: UI-019
// spec: CRAWL-128
#[tokio::test]
async fn an_unreachable_robots_txt_reports_its_own_row_and_url() {
    let base = format!("http://127.0.0.1:{}", closed_port());
    let o = crawl(&format!("{base}/"), config(true)).await;
    assert_eq!(o.state, CrawlState::Failed);
    let n = start_notice(&o);
    assert_eq!(n.code, "start-connection");
    assert_eq!(n.args["reason"], "connection_refused");
    assert_eq!(n.args["robots"], true);
    assert_eq!(n.args["url"], format!("{base}/robots.txt").as_str());
    assert_eq!(n.args["startUrl"], format!("{base}/").as_str());
    assert!(after_fetch(&o) < Duration::from_secs(2), "{:?}", o.elapsed);
    assert!(o.elapsed < Duration::from_secs(2), "{:?}", o.elapsed);
}

// Unbekannte Domain (`.invalid` ist nach RFC 6761 nie auflösbar).
// spec: UI-019
// spec: CRAWL-128
#[tokio::test]
async fn an_unknown_domain_reports_the_failed_dns_lookup() {
    for respect in [false, true] {
        let o = crawl("http://openspider-b4-unreach.invalid/", config(respect)).await;
        assert_eq!(o.state, CrawlState::Failed, "robots beachten: {respect}");
        let n = start_notice(&o);
        assert_eq!(n.code, "start-dns", "robots beachten: {respect}");
        assert_eq!(n.args["reason"], "dns_lookup_failed");
        assert_eq!(n.args["robots"], respect);
        let url = if respect {
            "http://openspider-b4-unreach.invalid/robots.txt"
        } else {
            "http://openspider-b4-unreach.invalid/"
        };
        assert_eq!(n.args["url"], url);
        assert!(after_fetch(&o) < Duration::from_secs(2), "{:?}", o.elapsed);
    }
}

// Zeitüberschreitung: der Dialog erscheint innerhalb von 2 s nach dem Timeout (Antwort-Timeout 1 s).
// spec: UI-019
// spec: CRAWL-128
#[tokio::test]
async fn a_timeout_is_reported_within_two_seconds_after_the_timeout() {
    let start = format!("http://127.0.0.1:{}/", silent_port());
    let mut c = config(false);
    c.advanced.response_timeout_s = 1;
    let o = crawl(&start, c).await;
    assert_eq!(o.state, CrawlState::Failed);
    let n = start_notice(&o);
    assert_eq!(n.code, "start-timeout");
    assert!(
        matches!(
            n.args["reason"].as_str(),
            Some("response_timeout" | "connection_timeout")
        ),
        "{n:?}"
    );
    assert_eq!(n.args["robots"], false);
    // Der Abruf endet erst nach den Wiederholungen des Abrufers mit dem letzten Timeout; ab da zählen die 2 s.
    assert!(after_fetch(&o) < Duration::from_secs(2), "{:?}", o.elapsed);
}

// robots.txt mit 500: der Host antwortet – kein Fehlschlag, keine Startfehler-Meldung.
// spec: UI-019
// spec: CRAWL-128
#[tokio::test]
async fn a_robots_txt_answering_500_is_no_failure_and_shows_no_start_dialog() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("scenario.toml"),
        "[[route]]\npath = \"/robots.txt\"\nstatus = 500\nbody = \"kaputt\"\n\n[[route]]\npath = \"/\"\nbody = \"x\"\n",
    )
    .unwrap();
    let site = os_testsite::TestSite::start_dir(dir.path(), 0)
        .await
        .unwrap();
    let o = crawl(&format!("http://{}/", site.addr()), config(true)).await;
    assert_eq!(o.state, CrawlState::Completed);
    assert!(
        !o.notices
            .iter()
            .any(|n| START_CODES.contains(&n.code.as_str()) || n.code == "crawl-failed"),
        "{:?}",
        o.notices
    );
}

// Zuordnung jeder Fehlerklasse zu ihrer Meldung; die Klasse selbst geht unverändert mit.
// spec: UI-019
// spec: CRAWL-128
#[test]
fn every_network_class_maps_to_its_dialog_code() {
    use NoResponseReason as R;
    for (reason, code) in [
        (R::DnsLookupFailed, "start-dns"),
        (R::ConnectionRefused, "start-connection"),
        (R::ConnectionError, "start-connection"),
        (R::ConnectionTimeout, "start-timeout"),
        (R::ResponseTimeout, "start-timeout"),
        (R::TlsError, "start-tls"),
    ] {
        let n = os_app::session::control::start_unreachable_notice(
            "https://site.test/",
            &os_engine::StartFailure {
                reason,
                url: "https://site.test/robots.txt".into(),
                robots: true,
            },
        );
        assert_eq!(n.code, code, "{reason:?}");
        assert_eq!(n.args["reason"], reason.as_str());
        assert_eq!(n.args["url"], "https://site.test/robots.txt");
        assert_eq!(n.args["startUrl"], "https://site.test/");
        assert_eq!(n.args["robots"], true);
    }
}

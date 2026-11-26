//! M4-K11, Kriterium (b), Teil Engine: Ein anderer Prozess führt einen abgeschlossenen bzw. pausierten Crawl als aktiver
//! Besitzer (KI-Lauf, simulierter Heartbeat): Fortsetzen, erneutes Crawlen, Entfernen, Konfigurationsänderung und
//! Stoppen ohne Engine werden mit „läuft noch (Prozess <pid>)“ abgewiesen; mehr als 10 s nach dem letzten Heartbeat
//! gelingt das Stoppen. Ein aktiver Besitzer mit der eigenen PID hält den eigenen Prozess nicht auf (08 AI-020 [M4],
//! Wirkung b; 01 CRAWL-125, Nachtrag K4-22). Zeit: `ManualClock`; für `remove_urls` (Systemuhr, keine `EngineDeps`)
//! ein Heartbeat in der Zukunft, der nach CRAWL-125 als aktuell gilt.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use os_engine::lifecycle::{apply_paused_config, stop_unowned};
use os_engine::rescan::{recrawl_urls, remove_urls};
use os_engine::resume_crawl;
use os_engine::testing::{test_deps, SimFetcher, SimSite};
use os_model::time::{IdSource, SeededIds};
use os_model::{Clock, CrawlConfig, CrawlMode, ManualClock, Timestamp, UrlId};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};

fn cases() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/crawl-files/active-owner/cases.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn t0() -> Timestamp {
    Timestamp::from_db(cases()["now"].as_str().unwrap()).unwrap()
}

fn foreign_pid() -> String {
    let pid = cases()["foreign_pid"].as_str().unwrap().to_string();
    assert_ne!(pid, std::process::id().to_string());
    pid
}

/// Crawl-Datei mit einer gecrawlten URL, Zustand `state` und Besitzer `pid` (Art `mcp-stdio`) mit Heartbeat `beat`.
fn crawl(dir: &Path, state: &str, pid: &str, beat: &str) -> PathBuf {
    let path = dir.join("crawl.ospider");
    let init = CrawlInit {
        crawl_id: SeededIds::new(11).new_crawl_id(t0()),
        mode: CrawlMode::Spider,
        start_url: Some("https://site.test/".into()),
        list_source: None,
        project: String::new(),
        name: "active-owner".into(),
        config: CrawlConfig::default(),
        created_at: t0(),
    };
    let file = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    file.conn()
        .execute(
            "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length) \
             VALUES (1,'https://site.test/','https://site.test/','https://site.test',1,'done','link',0,18)",
            [],
        )
        .unwrap();
    file.set_meta("state", state).unwrap();
    file.set_meta("started_at", &t0().to_db()).unwrap();
    if state == "Completed" {
        // Limit erreicht, Restwarteschlange: ohne Besitzer fortsetzbar.
        file.set_meta("end_reason", "limit_reached").unwrap();
        file.conn()
            .execute(
                "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length) \
                 VALUES (2,'https://site.test/a','https://site.test/a','https://site.test',1,'queued','link',0,19)",
                [],
            )
            .unwrap();
        file.conn()
            .execute(
                "INSERT INTO queue(url_id,seq,depth,priority) VALUES (2,1,1,0)",
                [],
            )
            .unwrap();
    }
    file.set_meta("owner_kind", "mcp-stdio").unwrap();
    file.set_meta("owner_pid", pid).unwrap();
    file.set_meta("heartbeat_at", beat).unwrap();
    path
}

fn deps(clock: &Arc<ManualClock>) -> os_engine::types::EngineDeps {
    let mut deps = test_deps();
    deps.clock = clock.clone();
    deps
}

fn meta(path: &Path, key: &str) -> String {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .get_meta(key)
        .unwrap()
        .unwrap_or_default()
}

fn assert_running_elsewhere(err: impl std::fmt::Display, pid: &str, what: &str) {
    let text = err.to_string();
    assert!(
        text.contains("läuft noch") && text.contains(&format!("Prozess {pid}")),
        "{what}: {text}"
    );
}

// spec: AI-020 [M4]
// spec: CRAWL-125
#[tokio::test]
async fn a_foreign_active_owner_rejects_resume_recrawl_config_and_stop_until_its_heartbeat_is_stale(
) {
    let pid = foreign_pid();
    for state in ["Completed", "Paused"] {
        let dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(ManualClock::new(t0()));
        let path = crawl(dir.path(), state, &pid, &t0().to_db());
        let fetcher = SimFetcher::new(SimSite::new());

        let err = resume_crawl(fetcher.clone(), path.clone(), None, deps(&clock))
            .await
            .err()
            .expect("Fortsetzen abgewiesen");
        assert_running_elsewhere(err, &pid, &format!("{state}: Fortsetzen"));
        let err = recrawl_urls(fetcher.clone(), &path, &[UrlId(1)], deps(&clock))
            .await
            .unwrap_err();
        assert_running_elsewhere(err, &pid, &format!("{state}: erneut crawlen"));
        let cfg = CrawlFile::open(&path, OpenMode::ReadOnly)
            .unwrap()
            .config()
            .unwrap();
        let err = apply_paused_config(&path, &cfg, &deps(&clock)).unwrap_err();
        assert_running_elsewhere(err, &pid, &format!("{state}: Konfiguration"));
        let err = stop_unowned(&path, &deps(&clock)).unwrap_err();
        assert_running_elsewhere(err, &pid, &format!("{state}: Stoppen ohne Engine"));
        assert_eq!(meta(&path, "state"), state, "nichts geschrieben");
        assert_eq!(meta(&path, "owner_pid"), pid);
        assert!(fetcher.requests().is_empty(), "keine Anfrage");

        // Grenze eingeschlossen: 10 000 ms alt ist noch aktiv.
        clock.advance(Duration::from_secs(10));
        assert_running_elsewhere(
            stop_unowned(&path, &deps(&clock)).unwrap_err(),
            &pid,
            &format!("{state}: 10 000 ms"),
        );
        // 11 s ohne neuen Heartbeat: kein aktiver Besitzer mehr; Stoppen gelingt (Paused) bzw. meldet den Zustand wie 1.0.
        clock.advance(Duration::from_secs(1));
        match state {
            "Paused" => {
                stop_unowned(&path, &deps(&clock)).unwrap();
                assert_eq!(meta(&path, "state"), "Completed");
                assert_eq!(meta(&path, "end_reason"), "stopped");
                assert_eq!(meta(&path, "owner_pid"), "");
            }
            _ => {
                let err = stop_unowned(&path, &deps(&clock)).unwrap_err().to_string();
                assert!(err.contains("bereits beendet"), "{err}");
            }
        }
    }
}

/// Der eigene Prozess als aktiver Besitzer (eigener KI-Lauf) hält Stoppen und Fortsetzen nicht auf.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[tokio::test]
async fn the_own_active_owner_does_not_block_the_own_process() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::new(t0()));
    let own = std::process::id().to_string();
    let path = crawl(dir.path(), "Paused", &own, &t0().to_db());
    let cfg = CrawlFile::open(&path, OpenMode::ReadOnly)
        .unwrap()
        .config()
        .unwrap();
    apply_paused_config(&path, &cfg, &deps(&clock)).unwrap();
    stop_unowned(&path, &deps(&clock)).unwrap();
    assert_eq!(meta(&path, "state"), "Completed");
    assert_eq!(meta(&path, "owner_pid"), "");
    assert_eq!(meta(&path, "heartbeat_at"), clock.now().to_db());
}

/// `remove_urls` prüft mit der Systemuhr; ein Heartbeat in der Zukunft gilt als aktuell (CRAWL-125, Hinweis iv).
// spec: AI-020 [M4]
// spec: CRAWL-125
#[test]
fn removing_urls_is_rejected_while_another_process_is_the_active_owner() {
    let dir = tempfile::tempdir().unwrap();
    let pid = foreign_pid();
    let path = crawl(dir.path(), "Completed", &pid, "2099-01-01T00:00:00.000Z");
    let err = remove_urls(&path, &[UrlId(1)]).unwrap_err();
    assert_running_elsewhere(err, &pid, "entfernen");
    let count: i64 = CrawlFile::open(&path, OpenMode::ReadOnly)
        .unwrap()
        .conn()
        .query_row("SELECT COUNT(*) FROM urls WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "nichts entfernt");
    // Ohne aktiven Besitzer (owner_pid leer) bleibt es beim Verhalten aus 1.0: Entfernen gelingt.
    CrawlFile::open(&path, OpenMode::ReadWrite)
        .unwrap()
        .set_meta("owner_pid", "")
        .unwrap();
    remove_urls(&path, &[UrlId(1)]).unwrap();
}

/// `os_engine::crawl::HEARTBEAT_INTERVAL` verweist auf den Wert in `os_store::owner` (Name und Wert unverändert).
// spec: AI-020 [M4]
// spec: CRAWL-125
#[test]
fn the_engine_interval_is_the_shared_constant() {
    assert_eq!(
        os_engine::crawl::HEARTBEAT_INTERVAL,
        os_store::owner::HEARTBEAT_INTERVAL
    );
    assert_eq!(os_engine::crawl::HEARTBEAT_INTERVAL, Duration::from_secs(2));
}

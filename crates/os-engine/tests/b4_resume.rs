//! B4-RESUME: „Gestoppt ist endgültig“ in der Engine (CRAWL-128, UI-003, AGENT-020), Stoppen eines pausierten Crawls
//! ohne laufende Engine und die Reihenfolge „Datei dauerhaft, dann Fortschritt“ am Crawl-Ende (CRAWL-126).

#[path = "lifecycle_support.rs"]
mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use os_engine::lifecycle::{add_urls_to_file, stop_unowned, STOPPED_IS_FINAL};
use os_engine::{resume_crawl, CrawlHandle, EngineDeps};
use os_model::{CrawlConfig, CrawlState, EndReason};
use support::*;

fn queue_len(path: &std::path::Path) -> i64 {
    scalar(&open_ro(path), "SELECT COUNT(*) FROM queue")
}

/// Wartet über den Fortschrittskanal (ohne Abfrageschleife), bis der Lauf einen Endzustand meldet, und liest in
/// diesem Augenblick die Datei: Zustand, Endgrund und `owner_pid` müssen dort schon stehen.
async fn file_at_reported_end(
    handle: &CrawlHandle,
    path: &std::path::Path,
) -> (String, String, String) {
    let mut watch = handle.watch();
    loop {
        if watch.borrow_and_update().state != CrawlState::Running {
            break;
        }
        watch
            .changed()
            .await
            .expect("der Lauf meldet einen Endzustand");
    }
    let file = open_ro(path);
    let get = |key: &str| file.get_meta(key).unwrap().unwrap_or_default();
    (get("state"), get("end_reason"), get("owner_pid"))
}

async fn stopped_crawl(
    site: &os_testsite::TestSite,
    work: &tempfile::TempDir,
) -> std::path::PathBuf {
    let path = crawl_file_in(work, "crawl.ospider");
    let handle = start(config_for(site, 1), &path, deps()).await;
    until_crawled(&handle, 5).await;
    handle.stop();
    let summary = run_to_end(handle).await;
    assert_eq!(summary.end_reason, Some(EndReason::Stopped));
    path
}

// spec: CRAWL-128
// spec: AGENT-020
// spec: UI-003
#[tokio::test]
async fn a_stopped_crawl_is_final_for_resume_and_for_added_urls_whatever_its_queue_holds() {
    let dir = slow_site_dir(50);
    let site = start_site(dir.path()).await;
    let work = tempfile::tempdir().unwrap();
    let path = stopped_crawl(&site, &work).await;
    let queued = queue_len(&path);
    assert!(queued > 0, "Rest der Warteschlange bleibt erhalten");
    let before = snapshot(&path);

    let cfg = resume_config(&path, &site, |_| {});
    let err = match resume_crawl(
        http_fetcher(&cfg, &deps()),
        path.clone(),
        Some(cfg.clone()),
        deps(),
    )
    .await
    {
        Ok(_) => panic!("ein gestoppter Crawl darf nicht fortgesetzt werden"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains(STOPPED_IS_FINAL), "{err}");
    // Ohne geänderte Konfiguration ebenso.
    let err = match resume_crawl(http_fetcher(&cfg, &deps()), path.clone(), None, deps()).await {
        Ok(_) => panic!("ein gestoppter Crawl darf nicht fortgesetzt werden"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains(STOPPED_IS_FINAL), "{err}");

    // CRAWL-131 setzt einen abgeschlossenen Crawl für zusätzliche URLs fort; ein gestoppter nimmt keine auf.
    let mut file = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
    let err = add_urls_to_file(&mut file, &[url("/neu.html")], &cfg)
        .unwrap_err()
        .to_string();
    assert!(err.contains(STOPPED_IS_FINAL), "{err}");
    drop(file);

    let meta = open_ro(&path).meta().unwrap();
    assert_eq!(meta.state, CrawlState::Completed);
    assert_eq!(meta.end_reason, Some(EndReason::Stopped));
    assert_eq!(queue_len(&path), queued, "Warteschlange unverändert");
    assert_eq!(snapshot(&path), before, "Datei unverändert");
}

// spec: CRAWL-128
// spec: CRAWL-126
#[tokio::test]
async fn a_limit_ended_crawl_still_resumes_after_raising_the_limit() {
    let dir = slow_site_dir(0);
    let site = start_site(dir.path()).await;
    let work = tempfile::tempdir().unwrap();
    let path = crawl_file_in(&work, "crawl.ospider");
    let mut cfg = config_for(&site, 1);
    cfg.limits.max_urls = 5;
    let summary = run_to_end(start(cfg, &path, deps()).await).await;
    assert_eq!(summary.end_reason, Some(EndReason::LimitReached));
    let cfg = resume_config(&path, &site, |c| c.limits.max_urls = 1_000);
    let summary = run_to_end(resume(cfg, &path, deps()).await).await;
    assert_eq!(summary.end_reason, Some(EndReason::QueueEmpty));
}

// spec: AGENT-020
// spec: CRAWL-126
#[tokio::test]
async fn the_reported_end_state_is_already_durable_with_the_owner_released() {
    let dir = slow_site_dir(50);
    let site = start_site(dir.path()).await;
    for stop in [false, true] {
        let work = tempfile::tempdir().unwrap();
        let path = crawl_file_in(&work, "crawl.ospider");
        let handle = start(config_for(&site, 2), &path, deps()).await;
        until_crawled(&handle, 3).await;
        if stop {
            handle.stop();
        } else {
            handle.pause();
        }
        let (state, reason, owner) = file_at_reported_end(&handle, &path).await;
        let expected = if stop {
            ("Completed", "stopped")
        } else {
            ("Paused", "")
        };
        assert_eq!((state.as_str(), reason.as_str()), expected, "stop={stop}");
        assert_eq!(
            owner, "",
            "stop={stop}: owner_pid ist freigegeben, sobald der Endzustand gemeldet ist"
        );
        run_to_end(handle).await;
    }
}

// spec: UI-003
// spec: AGENT-020
// spec: CRAWL-141
#[tokio::test]
async fn stopping_a_paused_crawl_without_an_engine_ends_it_for_good_and_runs_the_hook() {
    let dir = slow_site_dir(50);
    let site = start_site(dir.path()).await;
    let work = tempfile::tempdir().unwrap();
    let path = crawl_file_in(&work, "crawl.ospider");
    let handle = start(config_for(&site, 1), &path, deps()).await;
    until_crawled(&handle, 3).await;
    // Ein laufender Crawl gehört seiner Engine: abgewiesen.
    let err = stop_unowned(&path, &deps()).unwrap_err().to_string();
    assert!(err.contains("läuft noch"), "{err}");
    handle.pause();
    let paused = run_to_end(handle).await;
    assert_eq!(paused.state, CrawlState::Paused);
    let queued = queue_len(&path);
    assert!(queued > 0);

    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let deps = EngineDeps {
        on_finished: Some(Arc::new(move |s| {
            assert_eq!(s.end_reason, Some(EndReason::Stopped));
            counted.fetch_add(1, Ordering::SeqCst);
        })),
        ..deps()
    };
    let summary = stop_unowned(&path, &deps).unwrap();
    assert_eq!(summary.state, CrawlState::Completed);
    assert_eq!(summary.end_reason, Some(EndReason::Stopped));
    assert!(summary.crawled >= 3, "{}", summary.crawled);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "Analyse-Hook wie nach regulärem Ende"
    );

    let file = open_ro(&path);
    let meta = file.meta().unwrap();
    assert_eq!(meta.state, CrawlState::Completed);
    assert_eq!(meta.end_reason, Some(EndReason::Stopped));
    assert!(meta.ended_at.is_some());
    assert_eq!(file.get_meta("owner_pid").unwrap().as_deref(), Some(""));
    drop(file);
    assert_eq!(
        queue_len(&path),
        queued,
        "die Warteschlange bleibt gesichert"
    );

    let err = stop_unowned(&path, &deps).unwrap_err().to_string();
    assert!(err.contains("bereits beendet"), "{err}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let cfg = resume_config(&path, &site, |_| {});
    let err = match resume_crawl(http_fetcher(&cfg, &deps), path.clone(), None, deps.clone()).await
    {
        Ok(_) => panic!("gestoppt ist endgültig"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains(STOPPED_IS_FINAL), "{err}");
}

// ---------------------------------------------------------------------------------------------------
// Nicht erreichbare Start-URL (CRAWL-128, OUT-182; Befund 2 der Abnahmetests)

fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn site_with(scenario: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("scenario.toml"), scenario).unwrap();
    dir
}

// spec: CRAWL-128
// spec: OUT-182
#[tokio::test]
async fn an_unreachable_start_url_fails_keeps_its_row_and_cannot_be_resumed() {
    let work = tempfile::tempdir().unwrap();
    let path = crawl_file_in(&work, "crawl.ospider");
    let mut cfg = CrawlConfig::default();
    cfg.network.resolve = vec![format!("*.test=127.0.0.1:{}", closed_port())];
    cfg.robots.retries = 0;
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let deps = EngineDeps {
        on_finished: Some(Arc::new(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
        })),
        ..deps()
    };
    let handle = start(cfg.clone(), &path, deps.clone()).await;
    let summary = run_to_end(handle).await;
    assert_eq!(summary.state, CrawlState::Failed);
    assert_eq!(summary.end_reason, None);
    assert_eq!(summary.crawled, 1, "gültiges Ergebnis mit einer Zeile");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "Analyse und Exporte wie am regulären Ende"
    );
    let file = open_ro(&path);
    let meta = file.meta().unwrap();
    assert_eq!(meta.state, CrawlState::Failed);
    assert_eq!(meta.end_reason, None);
    assert!(meta.ended_at.is_some());
    assert_eq!(file.get_meta("owner_pid").unwrap().as_deref(), Some(""));
    drop(file);
    let err = match resume_crawl(http_fetcher(&cfg, &deps), path.clone(), None, deps).await {
        Ok(_) => panic!("„Fehlgeschlagen“ lässt sich nicht fortsetzen"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("fehlgeschlagen"), "{err}");
}

// Der Server antwortet oder es gibt weitere Ergebnisse: Das ist kein Fehlschlag.
// spec: CRAWL-128
#[tokio::test]
async fn a_start_url_that_answers_is_completed_even_with_an_error_or_a_robots_block() {
    for (label, scenario) in [
        (
            "Server-Fehler",
            "[[route]]\npath = \"/\"\nstatus = 500\nbody = \"kaputt\"\n",
        ),
        (
            "robots.txt sperrt",
            "[[route]]\npath = \"/robots.txt\"\ncontent_type = \"text/plain\"\nbody = \"User-agent: *\\nDisallow: /\\n\"\n\n[[route]]\npath = \"/\"\nbody = \"x\"\n",
        ),
    ] {
        let dir = site_with(scenario);
        let site = start_site(dir.path()).await;
        let work = tempfile::tempdir().unwrap();
        let path = crawl_file_in(&work, "crawl.ospider");
        let summary = run_to_end(start(config_for(&site, 1), &path, deps()).await).await;
        assert_eq!(summary.state, CrawlState::Completed, "{label}");
        assert_eq!(summary.end_reason, Some(EndReason::QueueEmpty), "{label}");
    }
}

// Listenmodus: eine nicht erreichbare Zeile ist ein Ergebnis, keine Start-URL.
// spec: CRAWL-128
#[tokio::test]
async fn an_unreachable_url_of_a_list_crawl_is_no_failed_crawl() {
    let work = tempfile::tempdir().unwrap();
    let path = crawl_file_in(&work, "crawl.ospider");
    let mut cfg = CrawlConfig::default();
    cfg.mode = os_model::CrawlMode::List;
    cfg.network.resolve = vec![format!("*.test=127.0.0.1:{}", closed_port())];
    cfg.robots.retries = 0;
    let d = deps();
    let handle = os_engine::start_crawl(
        http_fetcher(&cfg, &d),
        os_engine::CrawlRequest {
            config: cfg.clone(),
            source: os_engine::CrawlSource::List {
                items: vec![url("/")],
                label: "Liste".into(),
            },
            file: path.clone(),
            project: String::new(),
            name: "Liste".into(),
        },
        d,
    )
    .await
    .unwrap();
    assert_eq!(run_to_end(handle).await.state, CrawlState::Completed);
}

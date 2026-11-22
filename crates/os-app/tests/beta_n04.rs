#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use serde_json::json;
use std::time::{Duration, Instant};

struct ReleaseReaders(std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>);
impl Drop for ReleaseReaders {
    fn drop(&mut self) {
        *self.0 .0.lock().unwrap() = true;
        self.0 .1.notify_all();
    }
}

// spec: UI-018
// spec: UI-016
// spec: UI-030
// spec: UI-032
// spec: UI-033
// spec: UI-035
// spec: UI-145
// spec: CRAWL-126 [M2]
// spec: UI-124
#[tokio::test]
async fn synthetic_5000_live_details_filter_pause_resume_and_reopen() {
    tokio::time::timeout(Duration::from_secs(750), async {
    let site = os_testsite::TestSite::start_spec(
        "synthetic:n=5000,links=30,seed=7",
        os_testsite::SiteOptions::default(),
    )
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let first = app(dir.path());
    let mut config = cfg();
    config.speed.threads = 4;
    config.extraction.hash = true;
    config.duplicates.exact_enabled = true;
    config.save_default(&first.config().paths.settings).unwrap();
    let started = call(
        &first,
        "crawl.start",
        serde_json::to_value(request(site.url("/"))).unwrap(),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let n = first
                .read_pool()
                .run(key(&first), |f, _| {
                    Ok(f.conn().query_row(
                        // Wait for the same population as the Internal/HTML view below.
                        // Completed robots.txt rows are hidden (in_tables=0), so counting
                        // all done URLs can release the view check with only 199 HTML pages.
                        "SELECT COUNT(*) FROM urls WHERE state='done' AND is_internal=1 AND in_tables=1 AND content_class='HTML'",
                        [],
                        |r| r.get::<_, u64>(0),
                    )?)
                })
                .await
                .unwrap();
            if n >= 200 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    // Model busy aggregate/count workers with all general reader slots occupied.
    // Interactive details must have independent access to WAL snapshots.
    let release = ReleaseReaders(std::sync::Arc::new((
        std::sync::Mutex::new(false),
        std::sync::Condvar::new(),
    )));
    let mut release = Some(release);
    let mut blockers = Vec::new();
    for _ in 0..os_app::pool::DEFAULT_READ_CONNECTIONS {
        let pool = first.read_pool().clone();
        let key = key(&first);
        let wake = release.as_ref().unwrap().0.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        blockers.push(tokio::spawn(async move {
            pool.run(key, move |_, _| {
                let _ = started.send(());
                let guard = wake.0.lock().unwrap();
                let (_guard, timed) = wake.1.wait_timeout_while(guard, Duration::from_secs(30), |released| !*released).unwrap();
                assert!(!timed.timed_out(), "fixture reader was not released within 30 seconds");
                Ok(())
            })
            .await
            .unwrap()
        }));
        ready.await.unwrap();
    }
    // The real HTML-chip path asks for metadata before view.open. Exercising
    // only view.open would miss a remaining dependency on aggregate readers.
    tokio::time::timeout(
        Duration::from_secs(1),
        call(&first, "catalog.columns", json!({"tab":"Internal"})),
    )
    .await
    .expect("HTML filter metadata blocked by aggregate readers");
    for (command, args) in [
        ("detail.urlDetails", json!({"urlId":1})),
        ("detail.duplicates", json!({"urlId":1,"kind":"exact"})),
    ] {
        tokio::time::timeout(Duration::from_secs(1), call(&first, command, args))
            .await
            .expect("interactive detail blocked by aggregates");
    }
    for spec in [
        json!({"kind":"inlinks","urlIds":[1],"filter":{"id":"All"},"search":null,"sort":[]}),
        json!({"kind":"outlinks","urlIds":[1],"filter":{"id":"All"},"search":null,"sort":[]}),
        json!({"kind":"images","urlIds":[1],"search":null,"sort":[]}),
        json!({"kind":"tab","tab":"Internal","filter":"HTML","search":null,"sort":[]}),
    ] {
        let before = Instant::now();
        let mut handle = tokio::time::timeout(
            Duration::from_secs(1),
            call(&first, "view.open", json!({"spec":spec,"columns":[]})),
        )
        .await
        .expect("interactive details queued behind aggregate readers");
        if handle["state"] == "building" {
            let op = tokio::time::timeout(
                Duration::from_secs(1),
                first.ops().wait(handle["op"]["opId"].as_str().unwrap()),
            )
            .await
            .expect("table build blocked by aggregate readers")
            .unwrap();
            assert_eq!(op.state, os_app::dto::OpState::Done, "{:?}", op.error);
            handle = op.result.unwrap();
        }
        eprintln!(
            "live view {spec}: {:?}, total={}",
            before.elapsed(),
            handle["total"]
        );
        assert!(
            before.elapsed() < Duration::from_secs(1),
            "detail/filter blocked: {spec}"
        );
        if spec["kind"] == "tab" {
            assert!(handle["total"].as_u64().unwrap() >= 200);
        }
        if spec["kind"] != "tab" {
            let rows = tokio::time::timeout(Duration::from_secs(1), call(&first, "view.rows", json!({"viewId":handle["viewId"],"version":handle["version"],"offset":0,"limit":30,"columns":[]}))).await.expect("detail rows blocked by aggregates");
            if spec["kind"] == "inlinks" || spec["kind"] == "outlinks" {
                assert!(!rows["cells"].as_array().unwrap().is_empty());
            }
        }
        call(&first, "view.close", json!({"viewId":handle["viewId"]})).await;
    }
    let session = first.sessions().current(&window()).unwrap();
    os_app::session::feed::invalidate(&first, &session.key);
    tokio::time::timeout(
        Duration::from_secs(1),
        os_app::session::feed::refresh(&first, &session),
    )
    .await
    .expect("session progress blocked by aggregate readers")
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        call(&first, "analysis.status", json!(null)),
    )
    .await
    .expect("analysis metadata blocked by aggregate readers");
    for command in ["crawl.pause", "crawl.resume"] {
        tokio::time::timeout(Duration::from_secs(5), call(&first, command, json!(null)))
            .await
            .unwrap_or_else(|_| panic!("{command} blocked by aggregate readers"));
    }
    drop(release.take());
    for blocker in blockers.drain(..) {
        blocker.await.unwrap();
    }
    tokio::time::timeout(Duration::from_secs(600), async {
        loop {
            os_app::session::poll(&first).await.unwrap();
            if first.sessions().current(&window()).unwrap().state == os_model::CrawlState::Completed
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let path = std::path::PathBuf::from(started["file"].as_str().unwrap());
    os_app::session::close(&first, &window(), false);
    drop(first);
    let restarted = app(dir.path());
    let before = Instant::now();
    let summary = open(&restarted, &path).await;
    eprintln!(
        "reopen: {:?}, urls={}, bytes={}",
        before.elapsed(),
        summary.urls,
        summary.file_bytes
    );
    assert!(summary.urls >= 5000);
    assert_eq!(summary.phase, os_app::dto::CrawlPhase::Completed);
    assert!(before.elapsed() < Duration::from_secs(5));

    }).await.expect("N04 fixture exceeded its internal deadline");
}

// spec: UI-129
#[tokio::test]
async fn stopped_mcp_catalog_contains_all_tools_without_starting_server() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let before = call(&app, "mcp.status", json!({})).await;
        assert_eq!(before["kind"], "off");
        let tools = call(&app, "mcp.tools", json!({})).await;
        assert!(
            tools.as_array().unwrap().len()
                == os_mcp::tools::offered(os_ai::PreviewSwitch::OFF).len()
        );
        assert!(tools
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "query_urls"));
        assert_eq!(call(&app, "mcp.status", json!({})).await, before);
    })
    .await
    .expect("N04 fixture exceeded its internal deadline");
}

// spec: CRAWL-126 [M2]
// spec: UI-124
#[tokio::test]
async fn failed_open_reports_reason_as_well_as_returning_error() {
    tokio::time::timeout(Duration::from_secs(10), async {
        #[derive(Clone)]
        struct Logs(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
        impl std::io::Write for Logs {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let logs = Logs(bytes.clone());
        let _subscriber = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .without_time()
                .with_ansi(false)
                .with_max_level(tracing::Level::INFO)
                .with_writer(move || logs.clone())
                .finish(),
        );
        let dir = tempfile::tempdir().unwrap();
        let (app, _clock) = manual_app(dir.path());
        let sink = std::sync::Arc::new(os_app::events::CollectingSink::new());
        app.subscribe(window(), sink.clone());
        let path = dir.path().join("broken.ospider");
        std::fs::write(&path, b"invalid fixture").unwrap();
        let error = app
            .dispatch(ctx(), "crawl.open", json!({"kind":"path","path":path}))
            .await
            .unwrap_err();
        let mut notices = app
            .assemble_tick()
            .map(|tick| tick.notices)
            .unwrap_or_default();
        for event in sink.take() {
            if let os_app::dto::UiEvent::Tick { tick } = event {
                notices.extend(tick.notices);
            }
        }
        let failure = notices
            .iter()
            .find(|n| n.code == "crawl-open-failed")
            .expect("opening failure is visible");
        assert_eq!(failure.args["detail"], json!(error.detail));
        assert!(notices.iter().any(|n| n.code == "crawl-open-started"));
        let log = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(log.contains("Crawl-Datei öffnen: begonnen"), "{log}");
        assert!(log.contains("Crawl-Datei öffnen: fehlgeschlagen"), "{log}");
        assert!(log.contains("reason="), "{log}");
        assert!(log.contains("detail="), "{log}");
    })
    .await
    .expect("N04 fixture exceeded its internal deadline");
}

// spec: UI-129
#[tokio::test]
async fn tool_catalogue_does_not_wait_for_server_lifecycle_lock() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let service = app.ext::<os_app::mcp::Service>();
        let _lifecycle = service.state.lock().await;
        let tools = tokio::time::timeout(
            Duration::from_secs(1),
            app.dispatch(ctx(), "mcp.tools", json!({})),
        )
        .await
        .expect("local catalogue must not wait for a server transition")
        .unwrap();
        assert_eq!(
            tools.as_array().unwrap().len(),
            os_mcp::tools::offered(os_ai::PreviewSwitch::OFF).len()
        );
    })
    .await
    .expect("N04 fixture exceeded its internal deadline");
}

// spec: UI-124
#[tokio::test]
async fn remembering_a_crawl_preserves_recent_display_metadata() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let path = dir.path().join("fixture.ospider");
        drop(create(&path, cfg()));
        let path = std::fs::canonicalize(path).unwrap();
        app.uistate()
            .update(|saved| {
                saved.state = json!({ "recentCrawls": [
        {"path":path,"label":"http://127.0.0.1/fixture · 2026-10-29"},
        {"path":"older.ospider","label":"http://127.0.0.1/older · 2026-10-28"}
    ] })
            })
            .unwrap();
        open(&app, &path).await;
        let saved = app.uistate().load();
        assert_eq!(
            saved.state["recentCrawls"][0]["label"],
            "http://127.0.0.1/fixture · 2026-10-29"
        );
        assert_eq!(
            saved.state["recentCrawls"][1]["label"],
            "http://127.0.0.1/older · 2026-10-28"
        );
    })
    .await
    .expect("N04 fixture exceeded its internal deadline");
}

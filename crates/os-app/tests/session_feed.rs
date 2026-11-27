#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use os_app::{dto::*, session::feed};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

// spec: UI-013
#[tokio::test]
async fn durable_crawled_count_matches_visible_completed_urls_without_progress_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = manual_app(dir.path());
    let f = create(&dir.path().join("counts.ospider"), cfg());
    f.set_meta("state", "Paused").unwrap();
    for id in 1..=4 {
        insert(&f, id, 1);
    }
    f.conn()
        .execute_batch(
            "UPDATE urls SET in_tables = 0 WHERE id IN (2, 4);
             UPDATE urls SET state = 'queued' WHERE id IN (3, 4);
             INSERT INTO queue (url_id, seq) VALUES (3, 1), (4, 2);",
        )
        .unwrap();
    let counts = os_store::queries::session_counts(f.conn()).unwrap();
    let crawled: u64 = f
        .conn()
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM q_manifest WHERE key = 'urls_crawled'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(crawled, 1);
    assert_eq!(counts.crawled, crawled);
    assert_eq!(counts.discovered, 4);
    assert_eq!(counts.queued, 2);
    open(&app, f.path()).await;
    os_app::session::poll(&app).await.unwrap();
    let status = app.assemble_tick().unwrap().crawl.unwrap();
    assert_eq!(status.crawled, crawled);
    assert_eq!(status.discovered, 4);
    assert_eq!(status.queued, 2);
}

// spec: UI-003
#[tokio::test]
async fn opened_crawl_status_carries_its_start_url_for_the_header() {
    // B3-UI: after reopening a saved crawl the header shows its start URL; the status of the session carries it.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = manual_app(dir.path());
    let f = create(&dir.path().join("start-url.ospider"), cfg());
    f.set_meta("state", "Completed").unwrap();
    insert(&f, 1, 0);
    open(&app, f.path()).await;
    os_app::session::poll(&app).await.unwrap();
    let status = app.assemble_tick().unwrap().crawl.unwrap();
    assert_eq!(status.start_url.as_deref(), Some("https://fixture.test/"));
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["startUrl"], "https://fixture.test/");
}

// spec: UI-003
#[tokio::test]
async fn cleared_view_drops_the_start_url_of_the_closed_crawl() {
    // B3-UI (Fable): "Löschen" publishes the old status with an empty crawl id; it must not carry the start URL, or the
    // header would write it back over a URL the user typed meanwhile.
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = manual_app(dir.path());
    let f = create(&dir.path().join("cleared.ospider"), cfg());
    f.set_meta("state", "Completed").unwrap();
    insert(&f, 1, 0);
    open(&app, f.path()).await;
    os_app::session::poll(&app).await.unwrap();
    let before = app.assemble_tick().unwrap().crawl.unwrap();
    assert_eq!(before.start_url.as_deref(), Some("https://fixture.test/"));
    call(&app, "crawl.clear", serde_json::Value::Null).await;
    let status = app.assemble_tick().unwrap().crawl.unwrap();
    assert!(status.crawl_id.is_empty());
    assert_eq!(status.phase, CrawlPhase::Idle);
    assert_eq!(status.start_url, None);
    assert!(serde_json::to_value(&status)
        .unwrap()
        .get("startUrl")
        .is_none());
}

// spec: ONPAGE-193
// spec: UI-018
#[tokio::test]
async fn feed_includes_counter_only_changes_and_reset_without_unbounded_id_lists() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let f = create(&dir.path().join("feed.ospider"), cfg());
    f.set_meta("state", "Paused").unwrap();
    insert(&f, 1, 1);
    open(&app, f.path()).await;
    let changes = Arc::new(Mutex::new(Vec::new()));
    let copy = changes.clone();
    feed::subscribe(
        &app,
        Arc::new(move |_, c| copy.lock().unwrap().push(c.clone())),
    );
    os_app::session::poll(&app).await.unwrap();
    assert!(changes.lock().unwrap()[0].reset);
    f.conn()
        .execute(
            "UPDATE url_counters SET change_seq=2,inlinks=7 WHERE url_id=1",
            [],
        )
        .unwrap();
    f.set_meta("change_seq", "2").unwrap();
    clock.advance(Duration::from_secs(1));
    os_app::session::poll(&app).await.unwrap();
    assert_eq!(changes.lock().unwrap()[1].ids, vec![os_model::UrlId(1)]);
    f.conn().execute("DELETE FROM urls WHERE id=1", []).unwrap();
    f.set_meta("change_seq", "3").unwrap();
    f.set_meta("reset_seq", "3").unwrap();
    clock.advance(Duration::from_secs(1));
    os_app::session::poll(&app).await.unwrap();
    assert!(changes.lock().unwrap()[2].reset);
    f.conn().execute_batch("BEGIN").unwrap();
    for id in 2..=4100 {
        insert(&f, id, 4);
    }
    f.conn().execute_batch("COMMIT").unwrap();
    clock.advance(Duration::from_secs(1));
    os_app::session::poll(&app).await.unwrap();
    {
        let all = changes.lock().unwrap();
        let last = all.last().unwrap();
        assert!(last.reset);
        assert!(last.ids.is_empty());
    }
    clock.advance(Duration::from_secs(1));
    os_app::session::poll(&app).await.unwrap();
    assert_eq!(changes.lock().unwrap().len(), 4);
}

// spec: AGENT-020 [M2]
// spec: UI-013
#[tokio::test]
async fn live_foreign_crawl_is_read_only_and_uses_persisted_progress() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let f = create(&dir.path().join("foreign.ospider"), cfg());
    insert(&f, 1, 1);
    f.set_meta("state", "Running").unwrap();
    f.set_meta(
        "heartbeat_at",
        &os_model::Clock::now(clock.as_ref()).to_db(),
    )
    .unwrap();
    f.set_meta("owner_pid", "4294967294").unwrap();
    f.set_meta("owner_kind", "mcp-stdio").unwrap();
    let p = serde_json::json!({"state":"Running","end_reason":null,"crawled":12,"queued":20,"percent":37,"rate":2.5,"avg_response_time":0.05,"elapsed_s":30,"eta_s":8,"active_threads":2,"started_at":os_model::Clock::now(clock.as_ref()),"avg_rate":1.25,"paused_s":10,"discovered":32,"memory_stage":"warning","file_bytes":123456,"memory_used_bytes":800,"memory_limit_bytes":1000});
    f.set_meta("progress_snapshot", &p.to_string()).unwrap();
    let summary = open(&app, f.path()).await;
    assert!(!summary.owner.controllable);
    assert_eq!(summary.owner.kind, OwnerKind::Mcp);
    assert!(summary.owner.live);
    let s = next_crawl(&app).await;
    assert_eq!(s.crawled, 12);
    assert_eq!(s.avg_rate, 1.25);
    assert_eq!(s.memory_used_bytes, Some(800));
    assert_eq!(s.file_bytes, Some(123456));
    for cmd in [
        "crawl.pause",
        "crawl.resume",
        "crawl.stop",
        "analysis.start",
        "crawl.remove",
        "crawl.recrawl",
        "crawl.saveAs",
    ] {
        let args = match cmd {
            "analysis.start" => serde_json::json!({"steps":null}),
            "crawl.remove" | "crawl.recrawl" => {
                serde_json::json!({"selection":{"kind":"ids","ids":[1]}})
            }
            "crawl.saveAs" => serde_json::json!({"dest":dir.path().join("copy.ospider")}),
            _ => serde_json::Value::Null,
        };
        let err = app.dispatch(ctx(), cmd, args).await.unwrap_err();
        assert_eq!(err.code, os_app::ErrorCode::Unsupported, "{cmd}");
    }
    assert_eq!(
        f.get_meta("owner_pid").unwrap().as_deref(),
        Some("4294967294")
    );
    let write = app
        .write_queue()
        .submit_file(&key(&app), |_| Ok(()))
        .await
        .unwrap_err();
    assert_eq!(write.code, os_app::ErrorCode::NoCrawl);
    clock.advance(Duration::from_secs(11));
    // The first status published after the clock advanced is the stale one; no tick may still report it running.
    assert_eq!(next_crawl(&app).await.phase, CrawlPhase::Incomplete);
}

/// The next crawl status the session publishes. Besides this test's `poll`, the session's background loop
/// (`session::init`, every 100 ms of real time once a session is active) polls as well. Both take the session lock
/// with `try_lock`: while the loop holds it, `session::poll` returns at once without reading, and if the loop read the
/// file before the test changed the clock, it has nothing new to publish, so a single `poll` + `assemble_tick` can find
/// no crawl part (CI windows-x64, 26.11. and 27.11.2026). Poll until a status is published, bounded.
async fn next_crawl(app: &os_app::App) -> CrawlStatus {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            os_app::session::poll(app).await.unwrap();
            if let Some(crawl) = app.assemble_tick().and_then(|t| t.crawl) {
                return crawl;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("no crawl status published within 10 s")
}

// spec: UI-018
// spec: ONPAGE-193
#[tokio::test]
async fn counters_have_a_one_second_deadline_even_with_five_second_rows() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let f = create(&dir.path().join("cadence.ospider"), cfg());
    f.set_meta("state", "Paused").unwrap();
    insert(&f, 1, 1);
    app.set_live_interval_ms(5000).unwrap();
    open(&app, f.path()).await;
    let changes = Arc::new(Mutex::new(Vec::new()));
    let captured = changes.clone();
    feed::subscribe(
        &app,
        Arc::new(move |_, c| captured.lock().unwrap().push(c.clone())),
    );
    let sink = Arc::new(os_app::events::CollectingSink::new());
    app.subscribe(window(), sink.clone());
    os_app::session::poll(&app).await.unwrap();
    app.flush_tick();
    assert_eq!(changes.lock().unwrap().len(), 1);
    insert(&f, 2, 2);
    for _ in 0..10 {
        clock.advance(Duration::from_millis(100));
        os_app::session::poll(&app).await.unwrap();
        app.flush_tick();
    }
    let events = sink.take();
    assert_eq!(events.len(), 1);
    let UiEvent::Tick { tick } = &events[0] else {
        panic!()
    };
    assert!(tick.counts_version >= 2);
    assert_eq!(
        changes.lock().unwrap().len(),
        1,
        "table cadence remains five seconds"
    );
    clock.advance(Duration::from_secs(4));
    os_app::session::poll(&app).await.unwrap();
    assert_eq!(changes.lock().unwrap().len(), 2);
    assert_eq!(changes.lock().unwrap()[1].ids, vec![os_model::UrlId(2)]);
}

// spec: UI-003
// spec: UI-018
#[tokio::test]
async fn status_feed_does_not_wait_for_busy_table_readers() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let f = create(&dir.path().join("busy-readers.ospider"), cfg());
    f.set_meta("state", "Paused").unwrap();
    insert(&f, 1, 1);
    open(&app, f.path()).await;
    let session = app
        .sessions()
        .current(&os_app::WindowId("main".into()))
        .unwrap();
    clock.advance(Duration::from_secs(1));
    feed::invalidate(&app, &session.key);

    // Hold every ordinary reader until the status refresh has been attempted.
    // The separate feed connection must still publish the fixture's counters.
    let (entered_tx, mut entered_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut releases = Vec::new();
    let mut readers = Vec::new();
    for _ in 0..os_app::pool::DEFAULT_READ_CONNECTIONS {
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        releases.push(release_tx);
        let pool = app.read_pool().clone();
        let key = session.key.clone();
        let entered_tx = entered_tx.clone();
        readers.push(tokio::spawn(async move {
            pool.run(key, move |_, _| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
            .await
            .unwrap();
        }));
    }
    for _ in 0..readers.len() {
        entered_rx.recv().await.unwrap();
    }
    // Invalidate after all permits are held so a background poll during setup
    // cannot turn this refresh into a debounced no-op.
    feed::invalidate(&app, &session.key);
    let refreshed =
        tokio::time::timeout(Duration::from_secs(1), feed::refresh(&app, &session)).await;
    for release in releases {
        release.send(()).unwrap();
    }
    for reader in readers {
        reader.await.unwrap();
    }
    refreshed
        .expect("status polling must not queue behind table reads")
        .unwrap();
    assert_eq!(feed::snapshot(&app, session.key).await.unwrap().0.urls, 1);
    let status = app.assemble_tick().unwrap().crawl.unwrap();
    assert_eq!(status.crawled, 1);
    assert_eq!(status.phase, CrawlPhase::Paused);
}

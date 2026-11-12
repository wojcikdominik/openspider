#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

use crate::{dto::OpState, App, AppConfig, CallCtx};
use os_model::{CrawlConfig, CrawlMode, ManualClock, SeededIds, Timestamp};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

fn fixture() -> (tempfile::TempDir, App, Arc<ManualClock>, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("n21.ospider");
    let file = CrawlFile::create(
        &path,
        &CrawlInit {
            crawl_id: "00000000-0000-0000-0000-000000000001".parse().unwrap(),
            mode: CrawlMode::Spider,
            start_url: Some("https://site.test/".into()),
            list_source: None,
            project: "n21".into(),
            name: "n21".into(),
            config: CrawlConfig::default(),
            created_at: Timestamp::UNIX_EPOCH,
        },
        StorageProfile::Balanced,
    )
    .unwrap();
    file.conn()
        .execute_batch(include_str!(
            "../../../../apps/desktop/src/ipc/__fixtures__/k06/fx-ui-basic/scenario.sql"
        ))
        .unwrap();
    file.set_meta("state", "Completed").unwrap();
    let clock = Arc::new(ManualClock::new(Timestamp::UNIX_EPOCH));
    let app = App::new(
        AppConfig::new(dir.path())
            .with_clock(clock.clone())
            .with_ids(Arc::new(SeededIds::new(21))),
    )
    .unwrap();
    (dir, app, clock, path)
}

async fn call(app: &App, name: &str, args: Value) -> Value {
    app.dispatch(CallCtx::for_window("main"), name, args)
        .await
        .unwrap()
}
async fn open_crawl(app: &App, path: &std::path::Path) -> Value {
    call(app, "crawl.open", json!({"kind":"path", "path":path})).await
}
fn generation(app: &App, handle: &Value) -> u64 {
    let tab = crate::commands::view::get(
        app,
        &CallCtx::for_window("main"),
        handle["viewId"].as_str().unwrap(),
    )
    .unwrap()
    .tab
    .unwrap();
    let core = super::lock(&tab.state).core;
    core
}
async fn open_view(app: &App) -> Value {
    open_columns(app, json!(["Address"])).await
}
async fn open_columns(app: &App, columns: Value) -> Value {
    open_spec(
        app,
        json!({"kind":"tab","tab":"Internal","filter":"All","sort":[]}),
        columns,
    )
    .await
}
async fn open_spec(app: &App, spec: Value, columns: Value) -> Value {
    let h = call(app, "view.open", json!({"spec":spec,"columns":columns})).await;
    if h["state"] == "building" {
        let op = app
            .ops()
            .wait(h["op"]["opId"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(op.state, OpState::Done, "{op:?}");
        op.result.unwrap()
    } else {
        h
    }
}

/// Waits until the session feed has delivered its first change set after `crawl.open` and the maintenance it
/// started on the view behind `handle` has finished (M2-N30). The poll loop delivers that first change set as a
/// reset 0 to 100 ms after opening; a cache probe during the resulting maintenance is a miss by design
/// (`cache_probe_does_not_wait_for_live_maintenance`). The manual clock stays still, so the feed delivers no
/// further change set afterwards. The deadline only guards against a hang; the wait ends on the event.
async fn feed_maintenance_settled(app: &App, handle: &Value) {
    let tab = crate::commands::view::get(
        app,
        &CallCtx::for_window("main"),
        handle["viewId"].as_str().unwrap(),
    )
    .unwrap()
    .tab
    .unwrap();
    let key = app
        .sessions()
        .current(&CallCtx::for_window("main").window)
        .unwrap()
        .key;
    let delivered = || {
        app.ext::<crate::session::SessionService>()
            .feeds
            .lock()
            .unwrap()
            .contains_key(&key)
    };
    let idle = || {
        !tab.worker.load(std::sync::atomic::Ordering::SeqCst)
            && super::lock(&tab.pending).is_none()
            && tab.builder.try_lock().is_ok()
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !(delivered() && idle()) {
        assert!(
            std::time::Instant::now() < deadline,
            "feed delivery or view maintenance did not finish"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

// spec: UI-141
// spec: UI-142
#[tokio::test]
async fn cache_probe_does_not_wait_for_live_maintenance() {
    use futures_util::FutureExt;
    let (_dir, app, _clock, path) = fixture();
    open_crawl(&app, &path).await;
    let handle = open_view(&app).await;
    let tab = crate::commands::view::get(
        &app,
        &CallCtx::for_window("main"),
        handle["viewId"].as_str().unwrap(),
    )
    .unwrap()
    .tab
    .unwrap();
    let _maintenance = tab.builder.lock().await;
    assert!(matches!(tab.cached(&app).now_or_never(), Some(Ok(None))));
}

// spec: UI-140
// spec: UI-142
// spec: ONPAGE-015
#[tokio::test]
async fn percent_denominator_is_bound_to_the_view_snapshot_and_rebuilt_on_change() {
    let (_dir, app, _clock, path) = fixture();
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.conn()
        .execute_batch("UPDATE url_counters SET html_2xx_sources=2 WHERE url_id=1; UPDATE urls SET content_class='HTML',status_code=200 WHERE id IN (6,7);")
        .unwrap();
    drop(file);
    open_crawl(&app, &path).await;
    // Hidden internal HTML counts in the denominator; external HTML does not.
    for (expected, total) in [(50.0, 4), (66.67, 3)] {
        let h = open_columns(&app, json!(["Address", "% of Total"])).await;
        let rows = call(
            &app,
            "view.rows",
            json!({"viewId":h["viewId"],"version":h["version"],"offset":0,"limit":1,"columns":[]}),
        )
        .await;
        assert_eq!(rows["cells"][0][1], expected, "denominator {total}");
        call(&app, "view.close", json!({"viewId":h["viewId"]})).await;
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.conn().execute_batch("UPDATE urls SET status_code=404,change_seq=43 WHERE id=8; UPDATE crawl_meta SET value='43' WHERE key='change_seq';").unwrap();
    }
}

// spec: UI-147 [P1]
// spec: UI-141
// spec: UI-142
#[tokio::test]
async fn closed_fixture_tab_is_reused_and_released_after_sixty_seconds() {
    let (_dir, app, clock, path) = fixture();
    let summary = open_crawl(&app, &path).await;
    assert_eq!(summary["urls"], 8); // includes external and hidden discovered URLs
    let first = open_view(&app).await;
    feed_maintenance_settled(&app, &first).await;
    let core = generation(&app, &first);
    call(&app, "view.close", json!({"viewId":first["viewId"]})).await;
    assert!(app.dispatch(CallCtx::for_window("main"), "view.rows", json!({"viewId":first["viewId"],"version":first["version"],"offset":0,"limit":1,"columns":[]})).await.is_err());
    let second = open_view(&app).await;
    assert_ne!(second["viewId"], first["viewId"]);
    for field in ["total", "columns", "state", "version"] {
        assert_eq!(second[field], first[field]);
    }
    assert_eq!(
        generation(&app, &second),
        core,
        "reuse must not rebuild the core view"
    );
    call(&app, "view.close", json!({"viewId":second["viewId"]})).await;
    clock.advance(Duration::from_secs(60));
    super::release_idle(&app);
    let third = open_view(&app).await;
    assert_ne!(third["viewId"], first["viewId"]);
    assert_eq!(third["total"], first["total"]);
}

// spec: UI-142
#[tokio::test]
async fn dormant_fixture_never_reuses_rows_after_a_file_change_or_reset() {
    let (_dir, app, _clock, path) = fixture();
    open_crawl(&app, &path).await;
    let first = open_view(&app).await;
    call(&app, "view.close", json!({"viewId":first["viewId"]})).await;
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.conn().execute_batch("UPDATE urls SET in_tables=0,change_seq=43 WHERE id=1; UPDATE crawl_meta SET value='43' WHERE key='change_seq';").unwrap();
    drop(file);
    let second = open_view(&app).await;
    assert_eq!(second["version"], 43);
    assert_eq!(second["total"].as_u64().unwrap() + 1, first["total"]);
    call(&app, "view.close", json!({"viewId":second["viewId"]})).await;
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.set_meta("reset_seq", "43").unwrap();
    drop(file);
    let third = open_view(&app).await;
    assert_eq!(third["total"], second["total"]);
    assert_ne!(
        third["viewId"], second["viewId"],
        "reset invalidates a cache even at the same change_seq"
    );
    call(&app, "view.close", json!({"viewId":third["viewId"]})).await;
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.set_meta("reset_seq", "1").unwrap();
    file.set_meta("change_seq", "1").unwrap();
    drop(file);
    let reset = open_view(&app).await;
    assert_ne!(reset["viewId"], third["viewId"]);
    assert_eq!(reset["version"], 1);
}

// spec: UI-141
// spec: UI-142
#[tokio::test]
async fn filter_open_reuses_the_tab_session_but_keeps_closed_handles_revoked() {
    let (_dir, app, _clock, path) = fixture();
    open_crawl(&app, &path).await;
    let all = open_view(&app).await;
    let old = crate::commands::view::get(
        &app,
        &CallCtx::for_window("main"),
        all["viewId"].as_str().unwrap(),
    )
    .unwrap()
    .tab
    .unwrap();
    feed_maintenance_settled(&app, &all).await;
    call(&app, "view.close", json!({"viewId":all["viewId"]})).await;
    let html = open_spec(
        &app,
        json!({"kind":"tab","tab":"Internal","filter":"HTML","sort":[]}),
        json!(["Address"]),
    )
    .await;
    let current = crate::commands::view::get(
        &app,
        &CallCtx::for_window("main"),
        html["viewId"].as_str().unwrap(),
    )
    .unwrap()
    .tab
    .unwrap();
    assert!(Arc::ptr_eq(&old, &current));
    assert_ne!(all["viewId"], html["viewId"]);
    assert_eq!(all["total"], 6);
    assert_eq!(html["total"], 5);
    assert!(crate::commands::view::get(
        &app,
        &CallCtx::for_window("main"),
        all["viewId"].as_str().unwrap()
    )
    .is_err());
    let rows = call(&app, "view.rows", json!({"viewId":html["viewId"],"version":html["version"],"offset":0,"limit":200,"columns":[]})).await;
    assert_eq!(rows["rowKeys"], json!([1, 2, 3, 4, 8]));
}

// spec: UI-147 [P1]
#[tokio::test]
async fn grouped_catalog_width_keeps_population_and_directive_scope() {
    let (_dir, app, _clock, path) = fixture();
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    // An external URL with a wider list must not widen Internal's catalogue.
    file.conn().execute_batch("INSERT INTO directives(url_id,source,position,raw,tokens,applies,in_head) VALUES (6,'meta_robots',0,'a','a',1,1),(6,'meta_robots',1,'b','b',1,1),(6,'meta_robots',2,'c','c',1,1),(1,'meta_robots',2,'ignored','ignored',0,1);").unwrap();
    drop(file);
    open_crawl(&app, &path).await;
    let columns = call(&app, "catalog.columns", json!({"tab":"Internal"})).await;
    let keys: Vec<_> = columns
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["key"].as_str().unwrap())
        .collect();
    assert!(keys.contains(&"Meta Robots 2"));
    assert!(!keys.contains(&"Meta Robots 3"));
}

fn stored_keys(path: &std::path::Path) -> Vec<String> {
    let file = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    let mut stmt = file
        .conn()
        .prepare("SELECT key FROM view_cache WHERE key LIKE 'n21:%' ORDER BY key")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

// spec: UI-145
// spec: UI-147
#[tokio::test]
async fn opening_a_saved_crawl_stores_derivable_view_facts_for_the_next_open() {
    let (_dir, app, _clock, path) = fixture();
    open_crawl(&app, &path).await;
    open_view(&app).await;
    let overview = call(&app, "sidebar.overview", json!({})).await;
    let issues = call(&app, "sidebar.issues", json!({})).await;
    let structure = call(
        &app,
        "sidebar.siteStructure",
        json!({"view":"responseCodes","offset":0,"limit":200}),
    )
    .await;
    // Every tab's counts, the summary, the shared memberships and the sidebar aggregates.
    let tabs = {
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        os_analyze::tabs::configured_for_file(&CrawlConfig::default(), &file).len()
    };
    let complete = |keys: &[String]| {
        keys.iter().filter(|k| k.starts_with("n21:counts:")).count() == tabs
            && ["n21:membership", "n21:sidebar", "n21:summary"]
                .iter()
                .all(|k| keys.iter().any(|s| s == k))
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !complete(&stored_keys(&path)) && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(complete(&stored_keys(&path)), "{:?}", stored_keys(&path));

    // A second app serves the same answers for the stored data version.
    let dir = tempfile::tempdir().unwrap();
    let second = App::new(AppConfig::new(dir.path())).unwrap();
    open_crawl(&second, &path).await;
    assert_eq!(call(&second, "sidebar.overview", json!({})).await, overview);
    assert_eq!(call(&second, "sidebar.issues", json!({})).await, issues);
    assert_eq!(
        call(
            &second,
            "sidebar.siteStructure",
            json!({"view":"responseCodes","offset":0,"limit":200}),
        )
        .await,
        structure
    );
}

/// Waits until the quiet precomputation after opening has counted every tab of the current version.
async fn precomputed(path: &std::path::Path) {
    let file = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    let cfg = CrawlConfig::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !os_analyze::views::overview_progress(&file, &cfg, &Default::default(), 0)
        .unwrap()
        .complete()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "precomputation did not finish"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A new data version without a crawl: change_seq advances, nothing is stored for it yet.
fn new_version(path: &std::path::Path) -> u64 {
    let file = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
    let seq = os_store::queries::change_seq(file.conn()).unwrap() + 1;
    file.set_meta("change_seq", &seq.to_string()).unwrap();
    seq
}

// spec: UI-141
// spec: UI-145
#[tokio::test]
async fn cancelled_count_resumes_from_stored_tabs_without_recounting_them() {
    let (_dir, app, _clock, path) = fixture();
    open_crawl(&app, &path).await;
    let session = app
        .sessions()
        .current(&CallCtx::for_window("main").window)
        .unwrap();
    let tabs = {
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        os_analyze::tabs::configured_for_file(&CrawlConfig::default(), &file).len() as u64
    };
    precomputed(&path).await;
    new_version(&path);
    // An interrupted count of the new version: four tabs counted and stored, then cancelled.
    {
        let reader = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        let partial = os_analyze::views::overview_progress(
            &reader,
            &CrawlConfig::default(),
            &Default::default(),
            4,
        )
        .unwrap();
        assert_eq!(partial.done, 4);
        let writer = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        assert!(os_analyze::views::persist_facts(&writer).unwrap());
    }
    let job = crate::commands::sidebar::count_job(&app, &session);
    call(&app, "sidebar.countCancel", Value::Null).await;
    let result = job
        .finished()
        .wait_for(Option::is_some)
        .await
        .unwrap()
        .clone()
        .unwrap();
    if let Err(error) = &result {
        assert_eq!(error.message_key, "err-cancelled");
    }
    // Background derivable work: no operation that would block resume or quit.
    assert!(app.ops().active_statuses().is_empty());
    // Resuming starts at the stored tabs (first report before any counting), then completes.
    let again = crate::commands::sidebar::count_job(&app, &session);
    again.finished().wait_for(Option::is_some).await.unwrap();
    let first = again.first.lock().unwrap().unwrap();
    assert!(first.0 >= 4, "stored tabs are not counted again: {first:?}");
    assert_eq!(first.1, tabs);
    assert_eq!(*again.progress_for_test(), (tabs, tabs));
    let overview = call(&app, "sidebar.overview", json!({})).await;
    assert!(!overview["tabs"].as_array().unwrap().is_empty());
    // Finished counts are not kept.
    let status = call(&app, "sidebar.countStatus", Value::Null).await;
    assert_eq!(status, json!({"running": false, "done": 0, "total": 0}));
}

// spec: UI-141
// spec: UI-145
#[tokio::test]
async fn waiting_overview_ends_while_a_writer_keeps_changing_the_data_version() {
    let (_dir, app, _clock, path) = fixture();
    open_crawl(&app, &path).await;
    precomputed(&path).await;
    // A running crawl's feed: every few milliseconds a new change_seq, nothing stored meanwhile.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writer = {
        let (stop, path) = (stop.clone(), path.clone());
        std::thread::spawn(move || {
            let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
            file.set_meta("state", "Running").unwrap();
            let mut seq = os_store::queries::change_seq(file.conn()).unwrap();
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                seq += 1;
                file.set_meta("change_seq", &seq.to_string()).unwrap();
                std::thread::sleep(Duration::from_millis(1));
            }
            file.set_meta("state", "Completed").unwrap();
        })
    };
    // The request starts on a data version without stored counts.
    let start = {
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        os_store::queries::change_seq(file.conn()).unwrap()
    };
    loop {
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        if os_store::queries::change_seq(file.conn()).unwrap() > start + 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let answer = tokio::time::timeout(
        Duration::from_secs(60),
        call(&app, "sidebar.overview", json!({})),
    )
    .await;
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    writer.join().unwrap();
    let overview = answer.expect("the waiting request must end under a running feed");
    assert!(!overview["tabs"].as_array().unwrap().is_empty());
}

/// Adds an internal URL as the writer of a running crawl would (copy of an internal row, next `change_seq`).
fn add_internal_url(path: &std::path::Path) -> (os_model::UrlId, u64) {
    let file = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
    let seq = os_store::queries::change_seq(file.conn()).unwrap() + 1;
    let id: i64 = file
        .conn()
        .query_row("SELECT MAX(id) + 1 FROM urls", [], |r| r.get(0))
        .unwrap();
    file.conn()
        .execute_batch(&format!(
            "CREATE TEMP TABLE added AS SELECT * FROM urls WHERE is_internal = 1 ORDER BY id LIMIT 1;
             UPDATE added SET id = {id}, url = 'https://site.test/added-{id}', address = 'https://site.test/added-{id}',
                 change_seq = {seq};
             INSERT INTO urls SELECT * FROM added;
             DROP TABLE added;"
        ))
        .unwrap();
    file.set_meta("change_seq", &seq.to_string()).unwrap();
    (os_model::UrlId(id), seq)
}

// spec: UI-018
// spec: UI-142
// A view whose first build read the data before a URL was written, and which publishes only after the change feed
// reported that URL, still shows it: the feed reports every change once, so the view must not drop a report that
// arrives while it is being built (the table would otherwise stay one URL short for good, e.g. "18 of 20").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_reported_while_the_first_build_runs_reaches_the_view() {
    use futures_util::FutureExt;
    let (_dir, app, _clock, path) = fixture();
    open_crawl(&app, &path).await;
    // The session loop delivers no change set of its own while the test holds the mutation lock: the only report is
    // the one below.
    let service = app.ext::<crate::session::SessionService>();
    let _no_feed = service.mutation.lock().await;
    let session = app
        .sessions()
        .current(&CallCtx::for_window("main").window)
        .unwrap();
    let spec: crate::dto::ViewSpec = serde_json::from_value(
        json!({"kind":"tab","tab":"Internal","filter":"All","search":null,"sort":[]}),
    )
    .unwrap();
    let tab = super::TabView::new(&app, "first-build".into(), session.key.clone());
    let mut build = Box::pin(tab.build(
        &app,
        session.clone(),
        spec,
        vec!["Address".into()],
        os_model::CancelToken::default(),
    ));
    // One poll hands the read to the pool; the build publishes only when it is polled again.
    assert!(build.as_mut().now_or_never().is_none());
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let read_seq = loop {
        let seq = tab
            .session
            .try_lock()
            .ok()
            .and_then(|s| s.as_ref().and_then(|s| s.built_seq()));
        if let Some(seq) = seq {
            break seq;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "build read did not finish"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    };
    assert!(tab.published().is_none(), "precondition: not yet published");
    // The writer adds a URL after the build's read; the feed reports it before the build publishes.
    let (added, seq) = add_internal_url(&path);
    assert!(seq > read_seq);
    super::on_change(
        &app,
        &crate::session::feed::ChangeSet {
            key: session.key.clone(),
            seq,
            ids: vec![added],
            reset: false,
        },
    );
    let built = build.await.unwrap();
    assert_eq!(built.version, read_seq);
    super::settle(&app, Duration::from_secs(30)).await;
    let published = tab.published().unwrap();
    assert_eq!(
        (published.version, published.total),
        (seq, built.total + 1),
        "the URL reported during the first build is in the view"
    );
}

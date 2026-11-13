//! M2-K05: `view.*` auf den K-1-Sitzungen (Doppelpuffer, Abbruch, Pflege über den Änderungsfeed, Freigabe nach
//! 60 s, Suche und Segment-Schnittstelle).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "cmd_catalog_fixture.rs"]
mod fixture;
use fixture::*;
use os_analyze::{search::FilterSource, ViewError};
use os_app::{
    dto::{CmdError, CompareClass, ErrorCode, OpState, SegmentFilter},
    pool::CrawlKey,
    views::RestrictProvider,
    App, AppConfig, CallCtx, WindowId,
};
use os_model::{Clock, ManualClock, SeededIds, Timestamp};
use roaring::RoaringBitmap;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// Fixture with a manual clock (change feed cadence and idle release).
fn fx_clock() -> (Fx, Arc<ManualClock>, tempfile::TempDir) {
    let clock = Arc::new(ManualClock::new(Timestamp::UNIX_EPOCH));
    let (fx, home) = fx_with_clock(clock.clone());
    (fx, clock, home)
}
fn fx_with_clock(clock: Arc<dyn Clock>) -> (Fx, tempfile::TempDir) {
    let mut fx = Fx::new();
    let home = tempfile::tempdir().unwrap();
    let app = App::new(
        AppConfig::new(home.path())
            .with_ids(Arc::new(SeededIds::new(7)))
            .with_clock(clock.clone()),
    )
    .unwrap();
    app.register_crawl(CrawlKey("fx-ui-basic".into()), fx.path.clone());
    app.sessions()
        .set_current(main(), fx.app.sessions().current(&main()).unwrap());
    app.set_desktop_actions(fx.clipboard.clone());
    fx.app = app;
    (fx, home)
}
fn main() -> WindowId {
    WindowId("main".into())
}
async fn try_call(fx: &Fx, name: &str, args: Value) -> Result<Value, CmdError> {
    fx.app
        .dispatch(CallCtx::for_window("main"), name, args)
        .await
}
fn rows_args(h: &Value, version: &Value, offset: u64, limit: u32) -> Value {
    json!({"viewId":h["viewId"],"version":version,"offset":offset,"limit":limit,"columns":[]})
}
async fn wait_op(fx: &Fx, op: &Value) -> os_app::dto::OpStatus {
    let id = op["opId"].as_str().unwrap();
    let start = Instant::now();
    loop {
        let status = fx.app.ops().status(id).unwrap();
        if status.state != OpState::Running {
            return status;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "{status:?}");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
fn search(spec: Value) -> Value {
    let mut t = tab();
    t["search"] = spec;
    t
}
fn insert_url(fx: &Fx, id: i64, path: &str, seq: u64) {
    fx.write(&format!(
        "INSERT INTO urls(id,url,address,origin,is_internal,in_tables,state,discovered_via,folder_depth,url_length,content_class,status_code,status,indexability,crawl_depth,title_1,title_1_len,title_count,meta_desc_count,word_count,http_version,response_time_ms,canonical_self,effective_index,change_seq) \
         VALUES ({id},'https://site.test{path}','https://site.test{path}','https://site.test',1,1,'done','link',1,30,'HTML',200,'OK','Indexable',1,'T',1,1,0,10,'HTTP/2',10,1,1,{seq}); \
         INSERT OR IGNORE INTO url_counters(url_id) VALUES ({id}); \
         UPDATE crawl_meta SET value='{seq}' WHERE key='change_seq';"
    ));
}

/// Restriction that blocks its build until released (or a deadline passes).
struct Slow {
    ids: Vec<u32>,
    delay: Duration,
    release: AtomicBool,
}
impl FilterSource for Slow {
    fn key(&self) -> String {
        "test:slow".into()
    }
    fn bitmap(&self, _: &os_store::CrawlFile) -> Result<RoaringBitmap, ViewError> {
        let start = Instant::now();
        while !self.release.load(Ordering::SeqCst) && start.elapsed() < self.delay {
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(self.ids.iter().copied().collect())
    }
}
struct Provider<T>(Arc<T>);
impl<T: FilterSource + 'static> RestrictProvider for Provider<T> {
    fn restrict(
        &self,
        _: &CrawlKey,
        segment: Option<&SegmentFilter>,
        _: Option<&CompareClass>,
    ) -> Result<Vec<Arc<dyn FilterSource>>, CmdError> {
        assert!(matches!(segment, Some(SegmentFilter::Segment { name }) if name == "Blog"));
        Ok(vec![self.0.clone()])
    }
}
fn segment_spec() -> Value {
    let mut t = tab();
    t["segment"] = json!({"kind":"segment","name":"Blog"});
    t
}

// spec: UI-140
// spec: UI-142
#[tokio::test]
async fn tab_rows_come_from_the_session_with_random_access_and_locate() {
    let fx = Fx::new();
    let h = fx.open(tab(), json!(["Address", "Status Code"])).await;
    assert_eq!(h["state"], "ready");
    assert_eq!(h["version"], 42);
    assert_eq!(h["total"], 6);
    let all = fx.rows(&h, 0, 500, json!([])).await;
    let keys = all["rowKeys"].as_array().unwrap().clone();
    assert_eq!(keys.len(), 6);
    assert_eq!(all["columns"], json!(["Address", "Status Code"]));
    // Any window equals the slice of the full order (scrollbar jump, UI-140).
    for (offset, limit) in [(0u64, 1u32), (3, 2), (5, 10), (6, 1)] {
        let w = fx.rows(&h, offset, limit, json!([])).await;
        let end = (offset as usize + limit as usize).min(keys.len());
        let start = (offset as usize).min(keys.len());
        assert_eq!(w["rowKeys"].as_array().unwrap(), &keys[start..end]);
        assert_eq!(w["offset"], offset);
    }
    for (i, key) in keys.iter().enumerate() {
        let at = fx
            .call("view.locate", json!({"viewId":h["viewId"],"urlId":key}))
            .await;
        assert_eq!(at, i as u64);
    }
    // Columns other than the projection are read by row key, in view order.
    let other = fx.rows(&h, 1, 2, json!(["Address", "Word Count"])).await;
    assert_eq!(other["rowKeys"].as_array().unwrap(), &keys[1..3]);
    assert_eq!(other["columns"], json!(["Address", "Word Count"]));
    assert_eq!(other["cells"][0][0], all["cells"][1][0]);
    let e = try_call(&fx, "view.rows", rows_args(&h, &h["version"], 0, 0))
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidInput);
}

// spec: UI-142
#[tokio::test]
async fn visible_columns_in_the_projection_keep_the_published_window_during_a_crawl() {
    let fx = Fx::new();
    let visible = json!(["Address", "Word Count"]);
    let h = fx.open(tab(), visible.clone()).await;
    let before = fx.rows(&h, 0, 500, visible.clone()).await;
    insert_url(&fx, 30, "/blog/new", 43);
    // U02 must project every visible column; before feed maintenance they remain in the double buffer.
    assert_eq!(fx.rows(&h, 0, 500, visible).await, before);
    let mut args = rows_args(&h, &h["version"], 0, 500);
    args["columns"] = json!(["Address", "Status Code"]);
    // Non-projected columns use a live SQL read and can already be stale (documented client constraint).
    assert_eq!(
        try_call(&fx, "view.rows", args).await.unwrap_err().code,
        ErrorCode::Stale
    );
}

// spec: UI-011
// spec: UI-141
#[tokio::test]
async fn simple_search_is_a_case_insensitive_address_substring_on_top_of_the_filter() {
    let fx = Fx::new();
    let h = fx
        .open(
            search(json!({"kind":"simple","text":"BLOG"})),
            json!(["Address"]),
        )
        .await;
    assert_eq!(h["total"], 1);
    let rows = fx.rows(&h, 0, 10, json!([])).await;
    assert_eq!(rows["cells"][0][0], "https://site.test/blog/a");
    // The search narrows the active filter instead of replacing it.
    let mut spec = search(json!({"kind":"simple","text":"site.test"}));
    spec["filter"] = json!("HTML");
    let narrowed = fx
        .call("view.update", json!({"viewId":h["viewId"],"spec":spec}))
        .await;
    let html = fx
        .open(
            json!({"kind":"tab","tab":"Internal","filter":"HTML","search":null,"sort":[],"segment":null,"compare":null}),
            json!(["Address"]),
        )
        .await;
    assert_eq!(narrowed["total"], html["total"]);
    // Clearing the search restores the whole filter.
    let cleared = fx
        .call("view.update", json!({"viewId":h["viewId"],"spec":tab()}))
        .await;
    assert_eq!(cleared["total"], 6);
    assert_eq!(cleared["viewId"], h["viewId"]);
}

// spec: UI-011 [P1]
#[tokio::test]
async fn rule_trees_nest_and_or_and_honour_case_sensitivity() {
    let fx = Fx::new();
    insert_url(&fx, 20, "/shop/shop/schuhe", 42);
    insert_url(&fx, 21, "/shop/schuhe/shop", 42);
    let cond = |column: &str, op: &str, value: &str| json!({"kind":"cond","column":column,"op":op,"value":value});
    // Backreference: duplicated path segment (Konvention 01, fancy-regex).
    let tree =
        json!({"kind":"group","op":"and","children":[cond("Address","regex","/([^/]+)/\\1/")]});
    let h = fx
        .open(
            search(json!({"kind":"rules","tree":tree,"caseSensitive":false})),
            json!(["Address"]),
        )
        .await;
    assert_eq!(h["total"], 1);
    assert_eq!(fx.rows(&h, 0, 10, json!([])).await["rowKeys"], json!([20]));
    // (Address contains "blog" OR (Address contains "shop" AND Address ends with "shop")) – two levels.
    let tree = json!({"kind":"group","op":"or","children":[
        cond("Address","contains","blog"),
        {"kind":"group","op":"and","children":[cond("Address","contains","shop"),cond("Address","endsWith","shop")]}
    ]});
    let h = fx
        .call(
            "view.update",
            json!({"viewId":h["viewId"],"spec":search(json!({"kind":"rules","tree":tree,"caseSensitive":false}))}),
        )
        .await;
    let mut keys = fx.rows(&h, 0, 10, json!([])).await["rowKeys"]
        .as_array()
        .unwrap()
        .clone();
    keys.sort_by_key(|k| k.as_i64());
    assert_eq!(keys, vec![json!(2), json!(21)]);
    // Case-sensitive switch: "BLOG" only matches with it off.
    let tree = json!({"kind":"group","op":"and","children":[cond("Address","contains","BLOG")]});
    for (sensitive, total) in [(false, 1), (true, 0)] {
        let h = fx
            .open(
                search(json!({"kind":"rules","tree":tree,"caseSensitive":sensitive})),
                json!(["Address"]),
            )
            .await;
        assert_eq!(h["total"], total, "caseSensitive={sensitive}");
    }
}

// spec: UI-011 [P1]
// spec: UI-141
#[tokio::test]
async fn invalid_regex_is_rejected_without_changing_the_published_view() {
    let fx = Fx::new();
    let h = fx.open(tab(), json!(["Address"])).await;
    let before = fx.rows(&h, 0, 500, json!([])).await;
    let tree = json!({"kind":"group","op":"and","children":[{"kind":"cond","column":"Address","op":"regex","value":"(unclosed"}]});
    let e = try_call(
        &fx,
        "view.update",
        json!({"viewId":h["viewId"],"spec":search(json!({"kind":"rules","tree":tree,"caseSensitive":false}))}),
    )
    .await
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidInput, "{e:?}");
    assert_eq!(fx.rows(&h, 0, 500, json!([])).await, before);
}

// spec: UI-011 [P1]
// spec: UI-141
#[tokio::test]
async fn regex_step_limit_preserves_the_published_view() {
    let fx = Fx::new();
    insert_url(&fx, 22, &format!("/{}b", "a".repeat(40)), 42);
    let h = fx.open(tab(), json!(["Address"])).await;
    let before = fx.rows(&h, 0, 500, json!([])).await;
    // The backreference forces fancy-regex backtracking instead of a linear regex fast path.
    let tree = json!({"kind":"group","op":"and","children":[{"kind":"cond","column":"Address","op":"regex","value":r"^https://site.test/(a|aa)+\1$"}]});
    let result = try_call(
        &fx,
        "view.update",
        json!({"viewId":h["viewId"],"spec":search(json!({"kind":"rules","tree":tree,"caseSensitive":false}))}),
    )
    .await;
    // Depending on build speed, the step limit is reported directly or through an operation.
    let e = match result {
        Err(e) => e,
        Ok(v) if v["state"] == "building" => {
            let status = wait_op(&fx, &v["op"]).await;
            assert_eq!(status.state, OpState::Failed, "{status:?}");
            status
                .error
                .expect("failed search operation must report its error")
        }
        Ok(v) => panic!("expected a regex step-limit error or a building view, got {v:?}"),
    };
    assert_ne!(e.code, ErrorCode::Stale, "{e:?}");
    assert!(
        e.detail
            .as_deref()
            .is_some_and(|d| d.contains("Regex-Schrittlimit erreicht")),
        "{e:?}"
    );
    assert_eq!(fx.rows(&h, 0, 500, json!([])).await, before);
}

/// Stops the build's clock read in Cache::touch, after update_controlled's final cancellation check.
/// Other threads keep using the manual clock, so op.cancel and the app remain responsive.
struct CommitBarrier {
    clock: Arc<ManualClock>,
    ids: Vec<u32>,
    armed: AtomicBool,
    build_thread: Mutex<Option<std::thread::ThreadId>>,
    reached: tokio::sync::Notify,
    resume: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl FilterSource for CommitBarrier {
    fn key(&self) -> String {
        "test:commit-barrier".into()
    }
    fn bitmap(&self, _: &os_store::CrawlFile) -> Result<RoaringBitmap, ViewError> {
        if self.armed.swap(false, Ordering::SeqCst) {
            *self.build_thread.lock().unwrap() = Some(std::thread::current().id());
        }
        Ok(self.ids.iter().copied().collect())
    }
}
impl Clock for CommitBarrier {
    fn now(&self) -> Timestamp {
        self.clock.now()
    }
    fn monotonic(&self) -> Duration {
        let block = {
            let mut thread = self.build_thread.lock().unwrap();
            if *thread == Some(std::thread::current().id()) {
                thread.take();
                true
            } else {
                false
            }
        };
        if block {
            self.reached.notify_one();
            self.resume
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
        self.clock.monotonic()
    }
}

// spec: UI-141
// spec: UI-142
#[tokio::test]
async fn cancelled_core_commit_rebuilds_the_published_spec_after_idle_release() {
    // Cover different totals and equal totals with a changed order; rows and locate can each rebuild first.
    for (same_total, locate_first) in [(false, false), (false, true), (true, false), (true, true)] {
        let clock = Arc::new(ManualClock::new(Timestamp::UNIX_EPOCH));
        let (resume, receiver) = std::sync::mpsc::channel();
        let barrier = Arc::new(CommitBarrier {
            clock: clock.clone(),
            armed: AtomicBool::new(true),
            ids: if same_total {
                vec![1, 2, 3, 4, 5, 8]
            } else {
                vec![2]
            },
            build_thread: Mutex::new(None),
            reached: tokio::sync::Notify::new(),
            resume: Mutex::new(receiver),
        });
        let (fx, _home) = fx_with_clock(barrier.clone());
        os_app::views::set_restrict_provider(&fx.app, Arc::new(Provider(barrier.clone())));
        let h = fx.open(tab(), json!(["Address"])).await;
        let before = fx.rows(&h, 0, 500, json!([])).await;
        let mut spec = segment_spec();
        spec["sort"] = json!([{"column":"Address","dir":"desc"}]);
        let building = fx
            .call("view.update", json!({"viewId":h["viewId"],"spec":spec}))
            .await;
        assert_eq!(building["state"], "building");
        tokio::time::timeout(Duration::from_secs(5), barrier.reached.notified())
            .await
            .unwrap();
        fx.call("op.cancel", json!({"opId":building["op"]["opId"]}))
            .await;
        assert_eq!(
            wait_op(&fx, &building["op"]).await.state,
            OpState::Cancelled
        );
        resume.send(()).unwrap();
        // Drain the detached pool job: the core has now committed the cancelled specification.
        drop(
            fx.app
                .read_pool()
                .exclusive(&CrawlKey("fx-ui-basic".into()))
                .await,
        );
        assert_eq!(fx.rows(&h, 0, 500, json!([])).await, before);
        clock.advance(Duration::from_secs(60));
        os_app::views::release_idle(&fx.app);
        assert_eq!(os_app::views::retained_bytes(&fx.app), 0);
        let locate = || {
            fx.call(
                "view.locate",
                json!({"viewId":h["viewId"],"urlId":before["rowKeys"][0]}),
            )
        };
        if locate_first {
            assert_eq!(locate().await, 0);
        }
        assert_eq!(fx.rows(&h, 0, 500, json!([])).await, before);
        if !locate_first {
            assert_eq!(locate().await, 0);
        }
    }
}

// spec: UI-141
// spec: UI-142
#[tokio::test]
async fn slow_builds_run_as_cancellable_operations_and_cancel_restores_within_100ms() {
    let fx = Fx::new();
    let slow = Arc::new(Slow {
        ids: vec![2],
        delay: Duration::from_secs(30),
        release: AtomicBool::new(false),
    });
    os_app::views::set_restrict_provider(&fx.app, Arc::new(Provider(slow.clone())));
    let h = fx.open(tab(), json!(["Address"])).await;
    let before = fx.rows(&h, 0, 500, json!([])).await;
    let building = fx
        .call(
            "view.update",
            json!({"viewId":h["viewId"],"spec":segment_spec()}),
        )
        .await;
    assert_eq!(building["state"], "building");
    assert!(building["total"].is_null());
    let op = building["op"].clone();
    assert_eq!(op["cancellable"], true, "{op}");
    // Double buffer: the previous version stays readable while the build runs.
    // The fastest of three reads counts: a single read can be delayed by a loaded machine.
    let mut reads = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        assert_eq!(fx.rows(&h, 0, 500, json!([])).await, before);
        reads.push(t.elapsed());
    }
    assert!(
        reads.iter().min().unwrap() < &Duration::from_millis(100),
        "{reads:?}"
    );
    let t = Instant::now();
    fx.call("op.cancel", json!({"opId":op["opId"]})).await;
    let status = wait_op(&fx, &op).await;
    let cancelled = t.elapsed();
    assert_eq!(status.state, OpState::Cancelled, "{status:?}");
    assert_eq!(fx.rows(&h, 0, 500, json!([])).await, before);
    assert!(cancelled < Duration::from_millis(100), "{cancelled:?}");
    slow.release.store(true, Ordering::SeqCst);
}

// spec: UI-141
// Segment-Schnittstelle (RestrictProvider) für M2-K14/M2-C01
#[tokio::test]
async fn segment_restrictions_come_from_the_provider_and_finish_in_the_operation() {
    let fx = Fx::new();
    let e = try_call(
        &fx,
        "view.open",
        json!({"spec":segment_spec(),"columns":["Address"]}),
    )
    .await
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::Unsupported);
    let slow = Arc::new(Slow {
        ids: vec![2, 99],
        delay: Duration::from_millis(600),
        release: AtomicBool::new(false),
    });
    os_app::views::set_restrict_provider(&fx.app, Arc::new(Provider(slow)));
    let building = fx.open_raw(segment_spec(), json!(["Address"])).await;
    assert_eq!(building["state"], "building");
    let status = wait_op(&fx, &building["op"]).await;
    assert_eq!(status.state, OpState::Done, "{status:?}");
    let h = status.result.unwrap();
    assert_eq!(h["viewId"], building["viewId"]);
    assert_eq!(h["state"], "ready");
    assert_eq!(h["total"], 1);
    assert_eq!(fx.rows(&h, 0, 10, json!([])).await["rowKeys"], json!([2]));
}

// spec: UI-018
// spec: UI-142
#[tokio::test]
async fn change_feed_maintains_open_views_and_reports_view_changed() {
    let (fx, clock, _home) = fx_clock();
    let s = fx.app.sessions().current(&main()).unwrap();
    os_app::session::feed::refresh(&fx.app, &s).await.unwrap();
    let mut sorted = tab();
    sorted["sort"] = json!([{"column":"Word Count","dir":"asc"}]);
    let h = fx.open(sorted, json!(["Address", "Word Count"])).await;
    assert_eq!(h["total"], 6);
    while fx.app.assemble_tick().is_some() {}
    insert_url(&fx, 30, "/blog/new", 43);
    clock.advance(Duration::from_secs(1));
    os_app::session::feed::refresh(&fx.app, &s).await.unwrap();
    let start = Instant::now();
    let changed = loop {
        if let Some(tick) = fx.app.assemble_tick() {
            if let Some(v) = tick.views.iter().find(|v| v.view_id == h["viewId"]) {
                break v.clone();
            }
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    assert_eq!(changed.version, 43);
    assert_eq!(changed.total, Some(7));
    let e = try_call(&fx, "view.rows", rows_args(&h, &json!(42), 0, 1))
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Stale);
    let w = fx
        .call("view.rows", rows_args(&h, &json!(43), 0, 500))
        .await;
    // Word Count 10 is the smallest value: the new URL sorts first (sorted maintenance, K-1e).
    assert_eq!(w["rowKeys"][0], 30);
    assert_eq!(
        fx.call("view.locate", json!({"viewId":h["viewId"],"urlId":30}))
            .await,
        0
    );
}

// spec: UI-142
#[tokio::test]
async fn idle_views_release_their_memory_after_60_seconds_and_rebuild_lazily() {
    let (fx, clock, _home) = fx_clock();
    let mut sorted = tab();
    sorted["sort"] = json!([{"column":"Address","dir":"desc"}]);
    let h = fx.open(sorted, json!(["Address"])).await;
    let before = fx.rows(&h, 0, 500, json!([])).await;
    assert!(os_app::views::retained_bytes(&fx.app) > 0);
    clock.advance(Duration::from_secs(59));
    os_app::views::release_idle(&fx.app);
    assert!(os_app::views::retained_bytes(&fx.app) > 0);
    clock.advance(Duration::from_secs(1));
    os_app::views::release_idle(&fx.app);
    assert_eq!(os_app::views::retained_bytes(&fx.app), 0);
    // Same data: the next access rebuilds the identical version.
    assert_eq!(fx.rows(&h, 0, 500, json!([])).await, before);
    assert!(os_app::views::retained_bytes(&fx.app) > 0);
    // UI-142: closing retains the reusable tab for the idle window.
    fx.call("view.close", json!({"viewId":h["viewId"]})).await;
    assert!(os_app::views::retained_bytes(&fx.app) > 0);
}

// spec: UI-142
#[tokio::test]
async fn views_in_use_are_kept_beyond_60_seconds() {
    let (fx, clock, _home) = fx_clock();
    let h = fx.open(tab(), json!(["Address"])).await;
    for _ in 0..4 {
        clock.advance(Duration::from_secs(30));
        fx.rows(&h, 0, 1, json!([])).await;
        os_app::views::release_idle(&fx.app);
        assert!(os_app::views::retained_bytes(&fx.app) > 0);
    }
}

// spec: UI-032
// spec: UI-140
// spec: UI-142
#[tokio::test]
async fn inlink_views_page_a_materialized_id_list() {
    let fx = Fx::new();
    // 1 500 additional inlinks of URL 2 (K-1g: materialized link IDs instead of COUNT/OFFSET per window).
    fx.write(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 1500) \
         INSERT INTO links (id, source_id, target_id, target_raw, link_type, position, anchor, follow, link_position) \
         SELECT 10000 + i, 1, 2, '/blog/a', 'Hyperlink', 100 + i, printf('a%04d', i), 1, 'Content' FROM n",
    );
    let mut spec = links(true);
    spec["sort"] = json!([{"column":"Anchor Text","dir":"desc"}]);
    let h = fx.open(spec, json!(["From", "Anchor Text"])).await;
    let total = h["total"].as_u64().unwrap();
    assert!(total > 1500);
    let mut keys = Vec::new();
    let mut anchors = Vec::new();
    let mut offset = 0;
    while offset < total {
        let w = fx.rows(&h, offset, 500, json!([])).await;
        assert_eq!(w["offset"], offset);
        for (k, row) in w["rowKeys"]
            .as_array()
            .unwrap()
            .iter()
            .zip(w["cells"].as_array().unwrap())
        {
            keys.push(k.as_i64().unwrap());
            anchors.push(row[1].as_str().unwrap_or_default().to_string());
        }
        offset += 500;
    }
    assert_eq!(keys.len() as u64, total);
    let ours = anchors
        .iter()
        .filter(|a| a.len() == 5 && a.starts_with('a'))
        .collect::<Vec<_>>();
    assert_eq!(ours.len(), 1500);
    assert!(
        ours.windows(2).all(|w| w[0] > w[1]),
        "descending anchors across windows"
    );
    // Random access matches the sequential order; locate uses the same list.
    let w = fx.rows(&h, 777, 3, json!([])).await;
    assert_eq!(w["rowKeys"], json!(keys[777..780]));
    assert_eq!(
        fx.call(
            "view.locate",
            json!({"viewId":h["viewId"],"urlId":keys[1234]})
        )
        .await,
        1234
    );
    // K2-75a / A02-09 (K2-63): unrelated commits no longer invalidate a
    // detail generation. An explicit view refresh still rejects its old handle.
    fx.write("UPDATE crawl_meta SET value='43' WHERE key='change_seq'");
    assert_eq!(fx.rows(&h, 0, 1, json!([])).await["version"], h["version"]);
    fx.call(
        "view.update",
        json!({"viewId":h["viewId"],"spec":links(true)}),
    )
    .await;
    let e = try_call(&fx, "view.rows", rows_args(&h, &h["version"], 0, 1))
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Stale);
}

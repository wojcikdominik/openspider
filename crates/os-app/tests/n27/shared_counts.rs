//! M2-N27: one shared, throttled count per data version for Overview, Issues and filter counts
//! (UI-018 OS-Festlegung vom 02.11.2026, UI-145, UI-061). Included by `commands/sidebar/counts.rs`.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

use super::{Clock, CountFuture, Counted, Counter, Mode, SharedCounts, Throttle};
use crate::{runtime::sessions::SessionInfo, App, AppConfig, CallCtx};
use os_analyze::views::OverviewFacts;
use os_model::{CancelToken, CrawlConfig, CrawlMode, CrawlState, SeededIds, Timestamp};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

fn fixture() -> (tempfile::TempDir, App, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("n27.ospider");
    let file = CrawlFile::create(
        &path,
        &CrawlInit {
            crawl_id: "00000000-0000-0000-0000-000000000027".parse().unwrap(),
            mode: CrawlMode::Spider,
            start_url: Some("https://site.test/".into()),
            list_source: None,
            project: "n27".into(),
            name: "n27".into(),
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
    let app = App::new(AppConfig::new(dir.path()).with_ids(Arc::new(SeededIds::new(27)))).unwrap();
    (dir, app, path)
}

async fn call(app: &App, name: &str, args: Value) -> Value {
    app.dispatch(CallCtx::for_window("main"), name, args)
        .await
        .unwrap()
}

async fn opened(app: &App, path: &std::path::Path) -> SessionInfo {
    call(app, "crawl.open", json!({"kind":"path", "path":path})).await;
    app.sessions()
        .current(&CallCtx::for_window("main").window)
        .unwrap()
}

/// A new data version without a crawl: `change_seq` advances.
fn new_version(path: &std::path::Path) -> u64 {
    let file = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
    let seq = os_store::queries::change_seq(file.conn()).unwrap() + 1;
    file.set_meta("change_seq", &seq.to_string()).unwrap();
    seq
}

fn current_seq(path: &std::path::Path) -> u64 {
    let file = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    os_store::queries::change_seq(file.conn()).unwrap()
}

/// Deterministic counting: a manual clock in milliseconds, a queue of count durations (the count
/// advances the clock by its duration) and an optional gate that holds every count until released.
#[derive(Default)]
struct Fake {
    clock_ms: AtomicU64,
    durations_ms: Mutex<VecDeque<u64>>,
    calls: AtomicU64,
    gate: Option<Arc<tokio::sync::Semaphore>>,
}
impl Fake {
    fn install(self: &Arc<Self>, service: &SharedCounts) {
        let clock = self.clone();
        let clock: Arc<Clock> =
            Arc::new(move || Duration::from_millis(clock.clock_ms.load(Ordering::SeqCst)));
        service.set_clock(clock);
        let fake = self.clone();
        let counter: Arc<Counter> = Arc::new(
            move |_app: App,
                  _session: SessionInfo,
                  mode: Mode,
                  _token: CancelToken|
                  -> CountFuture {
                let fake = fake.clone();
                Box::pin(async move {
                    assert_eq!(
                        mode,
                        Mode::Live,
                        "the fake only stands in for running crawls"
                    );
                    if let Some(gate) = &fake.gate {
                        gate.acquire().await.unwrap().forget();
                    }
                    let duration = fake.durations_ms.lock().unwrap().pop_front().unwrap_or(0);
                    fake.clock_ms.fetch_add(duration, Ordering::SeqCst);
                    let n = fake.calls.fetch_add(1, Ordering::SeqCst) + 1;
                    Ok(Counted {
                        facts: Arc::new(OverviewFacts {
                            summary: vec![("Total URLs Encountered".into(), n)],
                            counts: vec![],
                        }),
                        seq: 1_000_000 + n,
                        stamp: String::new(),
                    })
                })
            },
        );
        service.set_counter(counter);
    }
    fn at(&self, ms: u64) {
        self.clock_ms.store(ms, Ordering::SeqCst);
    }
}

fn running(s: &SessionInfo) -> SessionInfo {
    SessionInfo {
        state: CrawlState::Running,
        ..s.clone()
    }
}

// spec: UI-018
#[test]
fn throttle_waits_max_of_two_seconds_and_three_times_the_last_duration() {
    let ms = Duration::from_millis;
    let mut t = Throttle::default();
    assert!(t.due(ms(0)), "nothing counted yet");
    t.record(ms(0), ms(100));
    assert_eq!(t.gap(), ms(2_000));
    assert!(!t.due(ms(2_099)));
    assert!(t.due(ms(2_100)));
    t.record(ms(5_000), ms(9_000));
    assert_eq!(t.gap(), ms(12_000));
    assert_eq!(t.next(), Some(ms(21_000)));
    assert!(!t.due(ms(20_999)));
    assert!(t.due(ms(21_000)));
}

// spec: UI-018
// spec: UI-145
// spec: UI-061
#[tokio::test]
async fn parallel_overview_issues_and_filter_counts_of_one_data_version_share_one_count() {
    let (_dir, app, path) = fixture();
    opened(&app, &path).await;
    let service = app.ext::<SharedCounts>();
    let before = service.started();
    let names = [
        "sidebar.overview",
        "sidebar.issues",
        "catalog.filterCounts",
        "sidebar.overview",
        "sidebar.issues",
        "catalog.filterCounts",
        "sidebar.overview",
        "sidebar.issues",
    ];
    let answers =
        futures_util::future::join_all(names.iter().map(|name| call(&app, name, json!({})))).await;
    assert_eq!(service.started() - before, 1, "one count for all requests");
    // Overview, Issues and the selection lists show the same counts.
    let overview = &answers[0];
    let counts = &answers[2];
    assert!(
        overview.get("ageMs").is_none(),
        "exact without a running crawl"
    );
    assert_eq!(counts["version"], json!(current_seq(&path)));
    let all = |tab: &str| {
        overview["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["tab"] == tab)
            .and_then(|t| {
                t["filters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|f| f["id"] == "All")
            })
            .map(|f| f["count"].clone())
            .unwrap()
    };
    let listed = counts["filters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["tab"] == "Internal" && r["filter"]["id"] == "All")
        .map(|r| r["filter"]["count"].clone())
        .unwrap();
    assert_eq!(all("Internal"), listed);
    assert_eq!(answers[1], answers[4]);
    // The same data version is answered from the shared count.
    call(&app, "catalog.filterCounts", json!({})).await;
    call(&app, "sidebar.issues", json!({})).await;
    assert_eq!(service.started() - before, 1);
    // A new data version is counted once more.
    new_version(&path);
    futures_util::future::join_all(names.iter().map(|name| call(&app, name, json!({})))).await;
    assert_eq!(service.started() - before, 2);
}

// spec: UI-018
// spec: UI-145
#[tokio::test]
async fn requests_during_a_running_count_join_it_and_a_cancelled_request_starts_none() {
    let (_dir, app, path) = fixture();
    let session = running(&opened(&app, &path).await);
    let service = app.ext::<SharedCounts>();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fake = Arc::new(Fake {
        gate: Some(gate.clone()),
        ..Fake::default()
    });
    fake.durations_ms.lock().unwrap().push_back(500);
    fake.install(&service);
    let waiting = (0..6)
        .map(|_| {
            let (app, session) = (app.clone(), session.clone());
            tokio::spawn(async move {
                super::shared_facts(&app, &session, &CancelToken::new())
                    .await
                    .map(|a| (a.seq, a.age))
            })
        })
        .collect::<Vec<_>>();
    let cancel = CancelToken::new();
    let cancelled = {
        let (app, session, cancel) = (app.clone(), session.clone(), cancel.clone());
        tokio::spawn(async move {
            super::shared_facts(&app, &session, &cancel)
                .await
                .map(|a| a.seq)
        })
    };
    while service.waiting(&session.key) < 7 {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let error = cancelled.await.unwrap().unwrap_err();
    assert_eq!(error.message_key, "err-cancelled");
    // The cancelled request neither ended the count nor started another one.
    assert!(service.flight(&session.key).is_some());
    assert_eq!(service.started(), 1);
    gate.add_permits(1);
    for task in waiting {
        let (seq, age) = task.await.unwrap().unwrap();
        assert_eq!(seq, 1_000_001);
        assert_eq!(age, Some(Duration::from_millis(500)));
    }
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    // Within the pause the next request gets the last count with its age, without counting.
    fake.at(1_000);
    let answer = super::shared_facts(&app, &session, &CancelToken::new())
        .await
        .unwrap();
    assert_eq!(
        (answer.seq, answer.age),
        (1_000_001, Some(Duration::from_secs(1)))
    );
    assert_eq!(service.started(), 1);
}

// spec: UI-018
// spec: UI-145
#[tokio::test]
async fn a_running_crawl_is_counted_at_most_every_max_2s_or_3x_the_last_duration() {
    let (_dir, app, path) = fixture();
    let session = running(&opened(&app, &path).await);
    let service = app.ext::<SharedCounts>();
    let fake = Arc::new(Fake::default());
    fake.durations_ms
        .lock()
        .unwrap()
        .extend([1_000, 100, 5_000, 0]);
    fake.install(&service);
    let ask = || async {
        let a = super::shared_facts(&app, &session, &CancelToken::new())
            .await
            .unwrap();
        (a.seq - 1_000_000, a.age.map(|d| d.as_millis() as u64))
    };
    // Count 1 takes 1 s: the next one may start 3 s after it ended (at 4 s).
    assert_eq!(ask().await, (1, Some(1_000)));
    fake.at(3_999);
    assert_eq!(ask().await, (1, Some(3_999)));
    fake.at(4_000);
    // Count 2 takes 0.1 s: 2 s pause (next at 6.1 s).
    assert_eq!(ask().await, (2, Some(100)));
    fake.at(6_099);
    assert_eq!(ask().await, (2, Some(2_099)));
    fake.at(6_100);
    // Count 3 takes 5 s: 15 s pause (next at 26.1 s).
    assert_eq!(ask().await, (3, Some(5_000)));
    fake.at(26_099);
    assert_eq!(ask().await, (3, Some(15_000 + 4_999)));
    fake.at(26_100);
    assert_eq!(ask().await, (4, Some(0)));
    assert_eq!(service.started(), 4);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 4);
}

// spec: UI-018
// spec: UI-061
#[tokio::test]
async fn after_a_pause_the_paused_version_is_counted_exactly_without_a_request() {
    let (_dir, app, path) = fixture();
    let session = opened(&app, &path).await;
    let service = app.ext::<SharedCounts>();
    let key = session.key.clone();
    // A running crawl: one live count of the current version.
    app.sessions().update_state(&key, CrawlState::Running);
    super::observe_state(&app, &key, CrawlState::Running);
    let live = super::shared_facts(&app, &running(&session), &CancelToken::new())
        .await
        .unwrap();
    assert!(
        live.age.is_some(),
        "a running crawl's count carries its age"
    );
    // The crawl writes on, then pauses. No request follows.
    let paused = new_version(&path);
    let announced = app
        .ext::<crate::session::SessionService>()
        .version
        .load(Ordering::SeqCst);
    let started = service.started();
    app.sessions().update_state(&key, CrawlState::Running);
    super::observe_state(&app, &key, CrawlState::Running);
    crate::session::state(&app, &key, CrawlState::Paused);
    tokio::time::timeout(Duration::from_secs(60), async {
        while service.last_seq(&key) != Some(paused) || service.flight(&key).is_some() {
            match service.flight(&key) {
                Some(flight) => flight.ended().await,
                None => tokio::task::yield_now().await,
            }
        }
        // The UI learns about the exact count through new versions of counts, issues and sidebar.
        while app
            .ext::<crate::session::SessionService>()
            .version
            .load(Ordering::SeqCst)
            <= announced
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the paused version is counted without a request");
    assert!(service.started() > started);
    // The next request (the UI's reload) is answered exactly and without a further count.
    let counted = service.started();
    let overview = call(&app, "sidebar.overview", json!({})).await;
    assert!(overview.get("ageMs").is_none(), "exact after the pause");
    let counts = call(&app, "catalog.filterCounts", json!({})).await;
    assert_eq!(counts["version"], json!(paused));
    assert_eq!(service.started(), counted);
}

/// `view_cache` entries stored for data version `seq`.
fn cached_at(path: &std::path::Path, seq: u64) -> i64 {
    let file = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    file.conn()
        .query_row(
            "SELECT COUNT(*) FROM view_cache WHERE built_seq = ?1",
            [seq as i64],
            |r| r.get(0),
        )
        .unwrap()
}

// spec: UI-018
// spec: CRAWL-126
#[tokio::test]
async fn a_request_right_after_a_pause_shares_the_one_count_and_writes_nothing() {
    let (_dir, app, path) = fixture();
    let session = opened(&app, &path).await;
    let service = app.ext::<SharedCounts>();
    let key = session.key.clone();
    // The quiet precomputation after opening stores pieces; it ends before the data version changes.
    call(&app, "sidebar.countCancel", Value::Null).await;
    let paused = new_version(&path);
    app.sessions().update_state(&key, CrawlState::Running);
    super::observe_state(&app, &key, CrawlState::Running);
    let (started, modes) = (service.started(), service.modes().len());
    crate::session::state(&app, &key, CrawlState::Paused);
    assert_eq!(
        service.settling(&key),
        1,
        "set before any request can count"
    );
    // The UI asks at once, racing the count that leaving `Running` started.
    let overview = call(&app, "sidebar.overview", json!({})).await;
    assert!(overview.get("ageMs").is_none(), "exact after the pause");
    tokio::time::timeout(Duration::from_secs(60), async {
        while service.settling(&key) > 0 || service.flight(&key).is_some() {
            match service.flight(&key) {
                Some(flight) => flight.ended().await,
                None => tokio::task::yield_now().await,
            }
        }
    })
    .await
    .expect("the count of the paused version ends");
    assert_eq!(
        service.started() - started,
        1,
        "one count for the request and the pause"
    );
    assert_eq!(
        service.modes()[modes..],
        [Mode::Settled],
        "taken without writing"
    );
    // Every write queued so far has run: nothing was stored for the paused version.
    app.write_queue()
        .submit_file(&key, |_| Ok(()))
        .await
        .unwrap();
    assert_eq!(
        cached_at(&path, paused),
        0,
        "no view_cache write while the WAL merges"
    );
    assert_eq!(service.last_seq(&key), Some(paused));
}

// spec: UI-018
// spec: CRAWL-141
#[tokio::test]
async fn a_final_version_changed_after_the_end_count_is_counted_before_the_end_status() {
    let (_dir, app, path) = fixture();
    let session = opened(&app, &path).await;
    let service = app.ext::<SharedCounts>();
    let key = session.key.clone();
    call(&app, "sidebar.countCancel", Value::Null).await;
    // The crawl ends: leaving `Running` counts the final version at once.
    let ended = new_version(&path);
    app.sessions().update_state(&key, CrawlState::Running);
    super::observe_state(&app, &key, CrawlState::Running);
    crate::session::state(&app, &key, CrawlState::Completed);
    tokio::time::timeout(Duration::from_secs(60), async {
        while service.settling(&key) > 0 || service.flight(&key).is_some() {
            match service.flight(&key) {
                Some(flight) => flight.ended().await,
                None => tokio::task::yield_now().await,
            }
        }
    })
    .await
    .expect("the count of the final version ends");
    assert_eq!(service.last_seq(&key), Some(ended));
    // The automatic analysis commits afterwards: a new final version, nobody has counted it yet.
    let analysed = new_version(&path);
    let (started, modes) = (service.started(), service.modes().len());
    tokio::time::timeout(
        Duration::from_secs(60),
        super::count_final(&app, std::slice::from_ref(&key)),
    )
    .await
    .expect("the final count ends");
    assert_eq!(
        service.last_seq(&key),
        Some(analysed),
        "the analysed version is counted"
    );
    assert_eq!(service.started() - started, 1);
    assert_eq!(
        service.modes()[modes..],
        [Mode::Settled],
        "taken without writing"
    );
    app.write_queue()
        .submit_file(&key, |_| Ok(()))
        .await
        .unwrap();
    assert_eq!(
        cached_at(&path, analysed),
        0,
        "no view_cache write right after the end"
    );
    // The UI's reload on the end status is answered from that count; an unchanged version is not counted again.
    let counted = service.started();
    let overview = call(&app, "sidebar.overview", json!({})).await;
    assert!(overview.get("ageMs").is_none(), "exact after the end");
    let counts = call(&app, "catalog.filterCounts", json!({})).await;
    assert_eq!(counts["version"], json!(analysed));
    super::count_final(&app, std::slice::from_ref(&key)).await;
    assert_eq!(service.started(), counted, "no further count");
}

/// A second crawl file beside the fixture's, opened in window `second`.
async fn second_crawl(dir: &std::path::Path, app: &App) -> (SessionInfo, std::path::PathBuf) {
    let path = dir.join("n27-second.ospider");
    let file = CrawlFile::create(
        &path,
        &CrawlInit {
            crawl_id: "00000000-0000-0000-0000-000000000028".parse().unwrap(),
            mode: CrawlMode::Spider,
            start_url: Some("https://site.test/".into()),
            list_source: None,
            project: "n27".into(),
            name: "n27-second".into(),
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
    drop(file);
    let ctx = CallCtx::for_window("second");
    app.dispatch(
        ctx.clone(),
        "crawl.open",
        json!({"kind":"path", "path":&path}),
    )
    .await
    .unwrap();
    (app.sessions().current(&ctx.window).unwrap(), path)
}

/// Waits until no count of `key` is pending or running.
async fn counts_idle(service: &SharedCounts, key: &crate::pool::CrawlKey) {
    tokio::time::timeout(Duration::from_secs(60), async {
        while service.settling(key) > 0 || service.flight(key).is_some() {
            match service.flight(key) {
                Some(flight) => flight.ended().await,
                None => tokio::task::yield_now().await,
            }
        }
    })
    .await
    .expect("the counts of the crawl end");
}

// spec: UI-018
#[tokio::test]
async fn the_final_count_of_an_ended_crawl_leaves_a_paused_crawl_of_another_window_alone() {
    let (dir, app, path) = fixture();
    let session = opened(&app, &path).await;
    let (other, other_path) = second_crawl(dir.path(), &app).await;
    let service = app.ext::<SharedCounts>();
    let (key, other_key) = (session.key.clone(), other.key.clone());
    assert_ne!(key, other_key);
    call(&app, "sidebar.countCancel", Value::Null).await;
    // The crawl of the second window was paused (its paused version is counted) and has written on since.
    new_version(&other_path);
    app.sessions().update_state(&other_key, CrawlState::Running);
    super::observe_state(&app, &other_key, CrawlState::Running);
    crate::session::state(&app, &other_key, CrawlState::Paused);
    counts_idle(&service, &other_key).await;
    let other_seq = service.last_seq(&other_key);
    let changed = new_version(&other_path);
    // The crawl of the main window ends, and its analysis changes the final version.
    new_version(&path);
    app.sessions().update_state(&key, CrawlState::Running);
    super::observe_state(&app, &key, CrawlState::Running);
    crate::session::state(&app, &key, CrawlState::Completed);
    counts_idle(&service, &key).await;
    let analysed = new_version(&path);
    let started = service.started();
    tokio::time::timeout(
        Duration::from_secs(60),
        super::count_final(&app, std::slice::from_ref(&key)),
    )
    .await
    .expect("the final count ends");
    assert_eq!(service.last_seq(&key), Some(analysed));
    assert_eq!(
        service.started() - started,
        1,
        "only the ended crawl is counted"
    );
    assert_eq!(
        service.last_seq(&other_key),
        other_seq,
        "the paused crawl is not counted"
    );
    assert_ne!(other_seq, Some(changed));
    assert!(service.flight(&other_key).is_none());
}

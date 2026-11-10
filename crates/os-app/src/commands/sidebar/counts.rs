//! One shared count per data version for Overview (`sidebar.overview`), Issues (`sidebar.issues`) and
//! the filter counts of the selection lists (`catalog.filterCounts`): UI-018 (OS-Festlegung vom
//! 02.11.2026, M2-N27), UI-145, UI-061.
//!
//! - Concurrent requests share one running count ([`Flight`]). The count owns its cancel token: a
//!   cancelled request only stops waiting, it neither ends the count nor starts another one.
//! - While the crawl runs (`CrawlState::Running`), the next count starts at the earliest
//!   max(2 s, 3 × duration of the last count) after the previous one ended ([`Throttle`]). Requests in
//!   between get the last count and its age. A running crawl is counted serially on one read
//!   connection, so the count uses at most about a quarter of one core.
//! - Without a running crawl the answer is exact: the current data version, counted through the
//!   stored tab pieces ([`super::exact_facts`], UI-141/UI-147). Leaving `Running` (pause, stop, end,
//!   also for crawls of another process) counts the paused or final version at once
//!   ([`observe_state`]) and announces new versions, so the UI reloads without user action.
//!
//! Incremental counting (UI-145 "nie Vollscan") replaces the periodic full count after the beta.
use crate::{
    dto::{CmdError, ErrorCode},
    pool::CrawlKey,
    runtime::{error::internal, sessions::SessionInfo, App},
};
use futures_util::future::BoxFuture;
use os_analyze::views::OverviewFacts;
use os_model::{CancelToken, CrawlState};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

/// Shortest pause between two counts of a running crawl.
pub(crate) const MIN_GAP: Duration = Duration::from_secs(2);
/// Pause between two counts of a running crawl as a multiple of the last count's duration.
pub(crate) const GAP_FACTOR: u32 = 3;

/// When the next count of a running crawl may start: max(2 s, 3 × last duration) after the end of the
/// previous count. Times are offsets of the service clock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Throttle {
    finished: Option<Duration>,
    last: Duration,
}
impl Throttle {
    /// Pause after the last count.
    pub(crate) fn gap(&self) -> Duration {
        MIN_GAP.max(self.last.saturating_mul(GAP_FACTOR))
    }
    /// Earliest start of the next count (`None`: nothing counted yet).
    pub(crate) fn next(&self) -> Option<Duration> {
        self.finished.map(|f| f + self.gap())
    }
    /// Whether a new count may start at `now`.
    pub(crate) fn due(&self, now: Duration) -> bool {
        self.next().is_none_or(|next| now >= next)
    }
    /// A count ran from `started` to `finished`.
    pub(crate) fn record(&mut self, started: Duration, finished: Duration) {
        self.last = finished.saturating_sub(started);
        self.finished = Some(finished);
    }
}

/// How a count is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Running crawl: all tabs in one serial pass over one snapshot, nothing written.
    Live,
    /// Right after leaving `Running`: the paused/final version in one snapshot with the parallel
    /// readers of the exact count, pieces kept in memory only (nothing written while the WAL merges).
    Settled,
    /// Exact data version through the stored tab pieces.
    Exact,
}

/// Facts of one data version.
#[derive(Debug)]
pub(crate) struct Counted {
    pub(crate) facts: Arc<OverviewFacts>,
    /// `change_seq` of the counted snapshot.
    pub(crate) seq: u64,
    /// Analysis/reset stamp of the counted snapshot.
    pub(crate) stamp: String,
}

/// Answer of the shared count.
pub(crate) struct Answer {
    pub(crate) facts: Arc<OverviewFacts>,
    /// `change_seq` of the counted snapshot (`FilterCounts.version`).
    pub(crate) seq: u64,
    /// Age of the count while the crawl runs; `None` for an exact answer.
    pub(crate) age: Option<Duration>,
}
impl Answer {
    pub(crate) fn age_ms(&self) -> Option<u64> {
        self.age
            .map(|age| u64::try_from(age.as_millis()).unwrap_or(u64::MAX))
    }
}

/// A running count.
pub(crate) type CountFuture = BoxFuture<'static, Result<Counted, CmdError>>;
/// Counting function (replaceable in tests: deterministic duration and results).
pub(crate) type Counter = dyn Fn(App, SessionInfo, Mode, CancelToken) -> CountFuture + Send + Sync;
/// Monotonic clock as offset since the service started (replaceable in tests).
pub(crate) type Clock = dyn Fn() -> Duration + Send + Sync;

#[derive(Clone)]
struct Snapshot {
    counted: Arc<Counted>,
    config: Arc<os_model::CrawlConfig>,
    /// Clock time at which the counted snapshot was taken (start of the count).
    taken: Duration,
}
impl Snapshot {
    fn matches(&self, version: &(u64, String)) -> bool {
        self.counted.seq == version.0 && self.counted.stamp == version.1
    }
}

/// One running count; every waiting request holds a receiver.
pub(crate) struct Flight {
    token: CancelToken,
    done: tokio::sync::watch::Sender<Option<Result<Snapshot, CmdError>>>,
}

#[derive(Default)]
struct Crawl {
    last: Option<Snapshot>,
    flight: Option<Arc<Flight>>,
    throttle: Throttle,
    observed: Option<CrawlState>,
    /// Counts of a paused/final version pending since `Running` ended ([`observe_state`]): until
    /// they are done, every count of this crawl is taken without writing (`Mode::Settled`), also
    /// one a request starts first (CRAWL-126: the WAL is merged right now).
    settling: u32,
}

/// Shared count state of all crawls of the app.
pub(crate) struct SharedCounts {
    crawls: Mutex<HashMap<CrawlKey, Crawl>>,
    clock: Mutex<Arc<Clock>>,
    counter: Mutex<Arc<Counter>>,
    started: AtomicU64,
    #[cfg(test)]
    modes: Mutex<Vec<Mode>>,
}
impl Default for SharedCounts {
    fn default() -> Self {
        let base = Instant::now();
        Self {
            crawls: Mutex::default(),
            clock: Mutex::new(Arc::new(move || base.elapsed())),
            counter: Mutex::new(Arc::new(
                |app: App, session: SessionInfo, mode: Mode, token: CancelToken| -> CountFuture {
                    Box::pin(async move {
                        match mode {
                            Mode::Live => live_count(&app, &session, token).await,
                            Mode::Settled => settled_count(&app, &session, token).await,
                            Mode::Exact => super::exact_facts(&app, &session, token).await,
                        }
                    })
                },
            )),
            started: AtomicU64::new(0),
            #[cfg(test)]
            modes: Mutex::default(),
        }
    }
}
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl SharedCounts {
    fn now(&self) -> Duration {
        let clock = lock(&self.clock).clone();
        clock()
    }
    fn answer(&self, snapshot: &Snapshot, live: bool) -> Answer {
        Answer {
            facts: snapshot.counted.facts.clone(),
            seq: snapshot.counted.seq,
            age: live.then(|| self.now().saturating_sub(snapshot.taken)),
        }
    }

    /// The running count of the crawl, or a new one.
    fn join_or_start(self: &Arc<Self>, app: &App, s: &SessionInfo, mode: Mode) -> Arc<Flight> {
        let mut crawls = lock(&self.crawls);
        let crawl = crawls.entry(s.key.clone()).or_default();
        if let Some(flight) = &crawl.flight {
            return flight.clone();
        }
        let (done, _) = tokio::sync::watch::channel(None);
        let flight = Arc::new(Flight {
            token: CancelToken::new(),
            done,
        });
        crawl.flight = Some(flight.clone());
        drop(crawls);
        let number = self.started.fetch_add(1, Ordering::SeqCst) + 1;
        #[cfg(test)]
        lock(&self.modes).push(mode);
        tracing::debug!(?mode, number, crawl = %s.key.0, "Gemeinsame Zählung gestartet");
        let counter = lock(&self.counter).clone();
        let (service, worker, session, task) =
            (self.clone(), app.clone(), s.clone(), flight.clone());
        app.handle().spawn(async move {
            let started = service.now();
            let result = counter(worker, session.clone(), mode, task.token.clone()).await;
            let finished = service.now();
            let result = result.map(|counted| Snapshot {
                counted: Arc::new(counted),
                config: session.config.clone(),
                taken: started,
            });
            {
                let mut crawls = lock(&service.crawls);
                // A forgotten crawl (closed, cleared, newly started) keeps nothing of this count.
                if let Some(crawl) = crawls
                    .get_mut(&session.key)
                    .filter(|c| c.flight.as_ref().is_some_and(|f| Arc::ptr_eq(f, &task)))
                {
                    crawl.flight = None;
                    crawl.throttle.record(started, finished);
                    if let Ok(snapshot) = &result {
                        crawl.last = Some(snapshot.clone());
                    }
                }
            }
            task.done.send_replace(Some(result));
        });
        flight
    }
}

/// Waits for the count; a cancelled request stops waiting, the count goes on.
async fn wait(flight: &Flight, cancel: &CancelToken) -> Result<Snapshot, CmdError> {
    let mut done = flight.done.subscribe();
    let result = crate::views::until_cancelled(cancel, async move {
        done.wait_for(Option::is_some).await.map(|r| {
            r.clone()
                .unwrap_or_else(|| Err(internal("sidebar.count", "leer")))
        })
    })
    .await?;
    result.unwrap_or_else(|_| Err(internal("sidebar.count", "Zählung beendet")))
}

/// `change_seq` and analysis stamp of the crawl's current data version (one small snapshot).
async fn version(app: &App, s: &SessionInfo) -> Result<(u64, String), CmdError> {
    app.read_pool()
        .run_snapshot(s.key.clone(), |file, _| {
            Ok((
                os_store::queries::change_seq(file.conn())?,
                os_analyze::views::filter_cache_stamp(file)?,
            ))
        })
        .await
}

/// Facts for Overview, Issues and filter counts: throttled while the crawl runs, exact otherwise.
pub(crate) async fn shared_facts(
    app: &App,
    s: &SessionInfo,
    cancel: &CancelToken,
) -> Result<Answer, CmdError> {
    let service = app.ext::<SharedCounts>();
    if s.state == CrawlState::Running {
        live(app, &service, s, cancel).await
    } else {
        exact(app, &service, s, cancel, Mode::Exact).await
    }
}

async fn live(
    app: &App,
    service: &Arc<SharedCounts>,
    s: &SessionInfo,
    cancel: &CancelToken,
) -> Result<Answer, CmdError> {
    enum Next {
        Serve(Snapshot),
        Join(Arc<Flight>),
        Due(Option<Snapshot>),
    }
    let now = service.now();
    let next = {
        let mut crawls = lock(&service.crawls);
        let crawl = crawls.entry(s.key.clone()).or_default();
        // A crawl started or resumed in this app is `Running` before the feed reports it.
        crawl.observed = Some(CrawlState::Running);
        let last = crawl.last.clone().filter(|l| *l.config == *s.config);
        match (&crawl.flight, last) {
            (Some(flight), _) => Next::Join(flight.clone()),
            (None, Some(last)) if !crawl.throttle.due(now) => Next::Serve(last),
            (None, last) => Next::Due(last),
        }
    };
    let flight = match next {
        Next::Serve(last) => return Ok(service.answer(&last, true)),
        Next::Join(flight) => flight,
        Next::Due(last) => {
            // An unchanged crawl (e.g. waiting for a slow host) needs no new count.
            if let Some(last) = last {
                if last.matches(&version(app, s).await?) {
                    return Ok(service.answer(&last, false));
                }
            }
            service.join_or_start(app, s, Mode::Live)
        }
    };
    let snapshot = wait(&flight, cancel).await?;
    Ok(service.answer(&snapshot, true))
}

/// Exact answer for the current data version; a missing count is taken with `mode`.
async fn exact(
    app: &App,
    service: &Arc<SharedCounts>,
    s: &SessionInfo,
    cancel: &CancelToken,
    mode: Mode,
) -> Result<Answer, CmdError> {
    let current = version(app, s).await?;
    for attempt in 0..2 {
        let (flight, mode) = {
            let mut crawls = lock(&service.crawls);
            let crawl = crawls.entry(s.key.clone()).or_default();
            if let Some(last) = crawl
                .last
                .as_ref()
                .filter(|l| *l.config == *s.config && l.matches(&current))
            {
                return Ok(service.answer(last, false));
            }
            // A request right after a pause or stop must not write either (CRAWL-126, FIX-WAL).
            let mode = if crawl.settling > 0 {
                Mode::Settled
            } else {
                mode
            };
            (crawl.flight.clone(), mode)
        };
        let flight = flight.unwrap_or_else(|| service.join_or_start(app, s, mode));
        let snapshot = wait(&flight, cancel).await?;
        // A joined count of an older version (e.g. the last live count before a pause) is followed
        // by one count of the current version; that one answers whatever version it counted.
        if attempt == 1 || (snapshot.matches(&current) && *snapshot.config == *s.config) {
            return Ok(service.answer(&snapshot, false));
        }
    }
    unreachable!("the second attempt always answers")
}

/// Serial count of a running crawl: summary and all filters of all tabs in one snapshot on the
/// `overview` read connection (one core; the throttle keeps it at about a quarter).
async fn live_count(app: &App, s: &SessionInfo, token: CancelToken) -> Result<Counted, CmdError> {
    let cfg = s.config.clone();
    app.read_pool()
        .run_cached(s.key.clone(), "overview", token, move |file, _| {
            let conn = file.conn();
            let _snapshot = conn.unchecked_transaction()?;
            let seq = os_store::queries::change_seq(conn)?;
            let stamp = os_analyze::views::filter_cache_stamp(file)?;
            let summary = os_store::queries::app_support::overview_summary(conn)?;
            let registry = os_analyze::tabs::configured_for_file(&cfg, file);
            Ok(
                os_analyze::views::filter_counts_in(&registry, file, &cfg).map(|counts| Counted {
                    facts: Arc::new(OverviewFacts { summary, counts }),
                    seq,
                    stamp,
                }),
            )
        })
        .await?
        .map_err(super::view_error)
}

/// Count of the paused/final version right after leaving `Running`: summary and all tabs in one
/// snapshot (parallel readers, UI-145), the pieces stay in memory; nothing is written.
async fn settled_count(
    app: &App,
    s: &SessionInfo,
    token: CancelToken,
) -> Result<Counted, CmdError> {
    let cfg = s.config.clone();
    app.read_pool()
        .run_cached(s.key.clone(), "overview", token, move |file, cancel| {
            let conn = file.conn();
            let _snapshot = conn.unchecked_transaction()?;
            let seq = os_store::queries::change_seq(conn)?;
            let stamp = os_analyze::views::filter_cache_stamp(file)?;
            Ok(
                os_analyze::views::overview_facts(file, &cfg, cancel).map(|(facts, _)| Counted {
                    facts,
                    seq,
                    stamp,
                }),
            )
        })
        .await?
        .map_err(super::view_error)
}

/// Whether a shared count of the crawl is running (`sidebar.countStatus`, UI-141).
pub(crate) fn counting(app: &App, key: &CrawlKey) -> bool {
    lock(&app.ext::<SharedCounts>().crawls)
        .get(key)
        .is_some_and(|c| c.flight.is_some())
}

/// The user cancels the running count (`sidebar.countCancel`); the last count stays visible.
pub(crate) fn cancel_flight(app: &App, key: &CrawlKey) -> bool {
    match lock(&app.ext::<SharedCounts>().crawls)
        .get(key)
        .and_then(|c| c.flight.as_ref())
    {
        Some(flight) => {
            flight.token.cancel();
            true
        }
        None => false,
    }
}

/// Forgets the crawl's counts (closed, cleared or newly started crawl) and cancels a running count.
pub(crate) fn forget(app: &App, key: &CrawlKey) -> bool {
    let removed = lock(&app.ext::<SharedCounts>().crawls).remove(key);
    match removed.and_then(|c| c.flight) {
        Some(flight) => {
            flight.token.cancel();
            true
        }
        None => false,
    }
}

/// Bumps the versions of counts, issues and sidebar: the UI reloads them with the next tick.
fn announce(app: &App) {
    let service = app.ext::<crate::session::SessionService>();
    let version = service.version.fetch_add(1, Ordering::SeqCst) + 1;
    let mut parts = lock(&service.parts);
    parts.counts_version = Some(version);
    parts.issues_version = Some(version);
    parts.sidebar_version = Some(version);
}

/// No exact count of a paused or final data version is pending for any crawl (UI-018, B4-LAGFIX2): the
/// session loop sends the end status of a crawl only after its exact final count was announced.
pub(crate) fn settled(app: &App) -> bool {
    lock(&app.ext::<SharedCounts>().crawls)
        .values()
        .all(|c| c.settling == 0)
}

/// Engine and feed transitions of a crawl (`session::state`). Leaving `Running` counts the paused or
/// final data version at once and announces it; no request or user action is needed.
pub(crate) fn observe_state(app: &App, key: &CrawlKey, state: CrawlState) {
    let service = app.ext::<SharedCounts>();
    let ended = {
        let mut crawls = lock(&service.crawls);
        let crawl = crawls.entry(key.clone()).or_default();
        let previous = crawl.observed.replace(state);
        let ended = previous == Some(CrawlState::Running) && state != CrawlState::Running;
        // Set synchronously, before any request of the new state can start a count.
        crawl.settling += u32::from(ended);
        ended
    };
    if !ended {
        return;
    }
    let Some(session) = app.sessions().all().into_iter().find(|s| &s.key == key) else {
        if let Some(crawl) = lock(&service.crawls).get_mut(key) {
            crawl.settling = crawl.settling.saturating_sub(1);
        }
        return;
    };
    let session = SessionInfo { state, ..session };
    let worker = app.clone();
    app.handle().spawn(async move {
        // One snapshot without writing: a paused or stopped crawl's WAL is merged right now (CRAWL-126,
        // FIX-WAL) and must stay empty; the stored pieces follow with the next exact request or opening.
        let result = exact(
            &worker,
            &service,
            &session,
            &CancelToken::new(),
            Mode::Settled,
        )
        .await;
        if let Some(crawl) = lock(&service.crawls).get_mut(&session.key) {
            crawl.settling = crawl.settling.saturating_sub(1);
        }
        match result {
            Ok(_) => announce(&worker),
            Err(error) if error.code == ErrorCode::Cancelled => {}
            Err(error) => tracing::debug!(?error, "Exakte Zählung nach Crawl-Ende fehlgeschlagen"),
        }
    });
}

#[cfg(test)]
impl SharedCounts {
    pub(crate) fn set_clock(&self, clock: Arc<Clock>) {
        *lock(&self.clock) = clock;
    }
    pub(crate) fn set_counter(&self, counter: Arc<Counter>) {
        *lock(&self.counter) = counter;
    }
    /// Pending counts of a paused/final version of `key`.
    pub(crate) fn settling(&self, key: &CrawlKey) -> u32 {
        lock(&self.crawls).get(key).map_or(0, |c| c.settling)
    }
    /// Modes of the counts started so far.
    pub(crate) fn modes(&self) -> Vec<Mode> {
        lock(&self.modes).clone()
    }
    /// Counts started so far.
    pub(crate) fn started(&self) -> u64 {
        self.started.load(Ordering::SeqCst)
    }
    /// Requests waiting for the running count of `key`.
    pub(crate) fn waiting(&self, key: &CrawlKey) -> usize {
        lock(&self.crawls)
            .get(key)
            .and_then(|c| c.flight.as_ref())
            .map_or(0, |f| f.done.receiver_count())
    }
    /// The running count of `key`.
    pub(crate) fn flight(&self, key: &CrawlKey) -> Option<Arc<Flight>> {
        lock(&self.crawls).get(key).and_then(|c| c.flight.clone())
    }
    /// `change_seq` of the last finished count of `key`.
    pub(crate) fn last_seq(&self, key: &CrawlKey) -> Option<u64> {
        lock(&self.crawls)
            .get(key)
            .and_then(|c| c.last.as_ref())
            .map(|l| l.counted.seq)
    }
}
#[cfg(test)]
impl Flight {
    /// Waits until the count has ended (any result).
    pub(crate) async fn ended(&self) {
        let _ = self.done.subscribe().wait_for(Option::is_some).await;
    }
}

#[cfg(test)]
#[path = "../../../tests/n27/shared_counts.rs"]
mod n27_tests;

//! Crawl sessions, bounded change feed and engine lifecycle (M2-K04).
pub mod analysis;
pub mod control;
pub mod feed;
pub mod merge;
pub mod recent;
pub mod selection;

use crate::{
    dto::*,
    events::{TickParts, TickSource},
    pool::CrawlKey,
    runtime::{lifecycle_bus::CrawlEvent, sessions::SessionInfo},
    App, CmdError, WindowId,
};
use os_model::CrawlState;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Default)]
pub struct SessionService {
    pub(crate) mutation: Arc<tokio::sync::Mutex<()>>,
    poll_cancel: Mutex<Option<os_model::CancelToken>>,
    pub(crate) feeds: Mutex<HashMap<CrawlKey, feed::Cursor>>,
    pub(crate) parts: Mutex<TickParts>,
    pub(crate) analysis: Mutex<HashMap<CrawlKey, AnalysisStatus>>,
    pub(crate) version: std::sync::atomic::AtomicU64,
    pub(crate) listeners: Mutex<Vec<Arc<feed::ChangeListener>>>,
    pub(crate) active: std::sync::atomic::AtomicBool,
    pub(crate) quitting: std::sync::atomic::AtomicBool,
    pub(crate) merges: merge::Merges,
}
impl TickSource for SessionService {
    fn collect(&self, parts: &mut TickParts) {
        let p = std::mem::take(&mut *self.parts.lock().unwrap_or_else(|p| p.into_inner()));
        if let Some(mut crawl) = p.crawl {
            // CRAWL-126: "Paused – merging file" until the background WAL merge has ended.
            crawl.merging = self.merges.active(&crawl.crawl_id).then_some(true);
            parts.crawl = Some(crawl);
        }
        if p.analysis.is_some() {
            parts.analysis = p.analysis;
        }
        if p.counts_version.is_some() {
            parts.counts_version = p.counts_version;
        }
        if p.issues_version.is_some() {
            parts.issues_version = p.issues_version;
        }
        if p.sidebar_version.is_some() {
            parts.sidebar_version = p.sidebar_version;
        }
        parts.views.extend(p.views);
        parts.notices.extend(p.notices);
    }
}

/// A due tick waits for the view maintenance its poll started until at most this long after the previous tick
/// (UI-018, B4-LAGFIX2), so ticks stay at most about one second apart. Measured in the debug E2E bridge on two
/// shared CPUs: maintenance median 120–180 ms, p90 260 ms, p99 670 ms. A slower run arrives with the next tick.
pub const TICK_SETTLE_UNTIL: Duration = Duration::from_millis(1000);
/// Shortest and longest wait of a due tick for its view maintenance.
pub const TICK_SETTLE_RANGE: (Duration, Duration) =
    (Duration::from_millis(100), Duration::from_millis(500));

/// Wait of the tick that carries the end status of a crawl for its final table rows (UI-018 „nach Crawl-Ende exakt“).
pub const TICK_SETTLE_ENDED: Duration = Duration::from_millis(1000);

/// Longest wait of the end status of a crawl for its exact final count (Overview, Issues, filter counts; UI-018
/// „nach Crawl-Ende exakt“). A larger crawl whose count takes longer sends it with a later tick.
/// B4-LAGFIX3: the same deadline also bounds the wait for the automatic crawl analysis (CRAWL-141) that the crawl
/// end started; its results are part of the final data version. A longer analysis reports itself as running in the
/// end tick and announces its rows with a later tick.
pub const TICK_SETTLE_COUNTS: Duration = Duration::from_millis(3000);

/// Wait budget of a due tick whose previous tick went out `since` ago (see [`TICK_SETTLE_UNTIL`]).
pub fn settle_budget(since: Duration) -> Duration {
    TICK_SETTLE_UNTIL
        .saturating_sub(since)
        .clamp(TICK_SETTLE_RANGE.0, TICK_SETTLE_RANGE.1)
}

/// Does the session loop deliver due ticks (a crawl session was attached)? Other periodic flushers stand back
/// then, so a tick is sent only after the session's aligned poll (UI-018, B4-LAGFIX2).
pub fn delivers_ticks(app: &App) -> bool {
    app.ext::<SessionService>()
        .active
        .load(std::sync::atomic::Ordering::SeqCst)
}

/// Installed once by the service initializer. Weak ownership lets App shut down.
pub fn init(app: &App) {
    app.add_tick_source(app.ext::<SessionService>());
    app.set_quit_hook(Arc::new(crate::quit::SessionQuit));
    let weak = app.downgrade();
    app.handle().spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut sent = tokio::time::Instant::now();
        // Der erste Tick kommt sofort: nicht gleich beim Start hochstufen (ein sofortiger Abbau durch den Besitzer
        // bleibt der letzte, OUT-229).
        interval.tick().await;
        loop {
            interval.tick().await;
            let Some(app) = weak.upgrade() else { break };
            if !app
                .ext::<SessionService>()
                .active
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                continue;
            }
            // UI-018 (B4-LAGFIX2): a tick carries the status and the table totals of the same poll.
            // Rows are published only when a tick is due (`feed::refresh`); the tick waits a bounded
            // time for the view maintenance this started. Periodic ticks are sent only from here while a
            // session is active (`delivers_ticks`); control commands send their own acknowledgement, and
            // while they hold the mutation lock `poll` returns at once.
            let due = app.tick_due();
            let finished = match poll_aligned(&app, due, false, false).await {
                Ok(finished) => finished,
                Err(e) => {
                    tracing::warn!(error = ?e, "Session poll failed");
                    Vec::new()
                }
            };
            let ended = !finished.is_empty();
            if ended {
                // The final rows of an ended crawl follow its end status promptly (≥ 100 ms, PERF-004).
                app.expedite_tick();
            }
            if due || ended {
                let budget = if ended {
                    TICK_SETTLE_ENDED
                } else {
                    settle_budget(sent.elapsed())
                };
                crate::views::settle(&app, budget).await;
                if ended {
                    // The end status goes out with the exact final count (UI-018, B4-LAGFIX2) and after the automatic
                    // crawl analysis the end started (CRAWL-141, B4-LAGFIX3): its commits change the data version, so a
                    // view opened on the end status would otherwise be out of date a moment later (OUT-020).
                    let until = tokio::time::Instant::now() + TICK_SETTLE_COUNTS;
                    let analysing = analysis::running(&app);
                    while (!crate::commands::sidebar::counts::settled(&app) || analysis::running(&app))
                        && tokio::time::Instant::now() < until
                    {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    if analysing && !analysis::running(&app) {
                        // The rows the analysis changed travel with the end status, not one tick later.
                        if let Err(e) = poll_final(&app).await {
                            tracing::warn!(error = ?e, "Session poll after the crawl analysis failed");
                        }
                        let rest = until.saturating_duration_since(tokio::time::Instant::now());
                        crate::views::settle(&app, rest.min(TICK_SETTLE_ENDED)).await;
                        // The analysis changed the final data version after its count: count it for the crawls that
                        // ended in this poll before the end status goes out (UI-018), within the same deadline; a longer
                        // count is joined by the UI's reload.
                        let rest = until.saturating_duration_since(tokio::time::Instant::now());
                        let _ = tokio::time::timeout(
                            rest,
                            crate::commands::sidebar::counts::count_final(&app, &finished),
                        )
                        .await;
                    }
                }
                app.flush_tick();
                sent = tokio::time::Instant::now();
            }
        }
    });
}

pub fn current(app: &App, window: &WindowId) -> Result<SessionInfo, CmdError> {
    app.sessions()
        .current(window)
        .ok_or_else(|| crate::runtime::error::no_crawl(&window.0))
}
pub fn writable(s: &SessionInfo) -> Result<(), CmdError> {
    if s.writable {
        Ok(())
    } else {
        Err(CmdError::new(
            ErrorCode::Unsupported,
            "err-crawl-foreign-owner",
        ))
    }
}
pub fn busy() -> CmdError {
    CmdError::new(ErrorCode::Busy, "err-crawl-busy").retryable()
}

/// Updates every window pointing at this crawl (including in-app MCP sessions).
pub fn state(app: &App, key: &CrawlKey, state: CrawlState) {
    app.sessions().update_state(key, state);
    // UI-018 (M2-N27): leaving `Running` recounts the paused/final data version without user action.
    crate::commands::sidebar::counts::observe_state(app, key, state);
}

/// One explicit poll of all sessions (UI-018). Waits for a running poll (the background loop of [`init`] or a control
/// command holding the session lock) and reads afterwards, so the state of the files at the time of the call is
/// published when it returns. Only tests call it; the background loop keeps skipping a busy lock (it may miss one tick).
/// Must not be called while the caller holds the session lock (the lock is not reentrant).
pub async fn poll(app: &App) -> Result<(), CmdError> {
    poll_aligned(app, app.tick_due(), false, true)
        .await
        .map(|_| ())
}

/// Poll after the automatic crawl analysis of an ended crawl (B4-LAGFIX3): the rows of every crawl that no longer runs
/// are published at once, like the final rows of the crawl end.
async fn poll_final(app: &App) -> Result<Vec<CrawlKey>, CmdError> {
    poll_aligned(app, true, true, false).await
}

/// One poll of all sessions; `due`: this poll belongs to a due tick (see `feed::refresh_aligned`). `Ok(keys)`:
/// the crawls that ended in this poll (their final rows were published). `last`: also publish the rows of crawls that no
/// longer run at once (see [`poll_final`]). `wait`: wait for the session lock instead of skipping this poll while it is
/// held (only the explicit [`poll`]).
async fn poll_aligned(
    app: &App,
    due: bool,
    last: bool,
    wait: bool,
) -> Result<Vec<CrawlKey>, CmdError> {
    let service = app.ext::<SessionService>();
    let _guard = if wait {
        service.mutation.lock().await
    } else {
        let Ok(guard) = service.mutation.try_lock() else {
            return Ok(Vec::new());
        };
        guard
    };
    let cancel = os_model::CancelToken::new();
    *service
        .poll_cancel
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(cancel.clone());
    let result = crate::runtime::with_cancel(cancel, async {
        let mut seen = std::collections::HashSet::new();
        let mut ended = Vec::new();
        for s in app.sessions().all() {
            if !seen.insert(s.key.clone()) {
                continue;
            }
            let finished = s.engine.as_ref().is_some_and(|e| {
                e.handle
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .as_ref()
                    .is_some_and(|h| h.progress().state != CrawlState::Running)
            });
            if finished {
                control::settle(app, &s, None).await?;
            }
            let last = last && s.state != CrawlState::Running;
            feed::refresh_aligned(app, &s, due || finished, finished || last).await?;
            if finished {
                ended.push(s.key.clone());
            }
        }
        Ok(ended)
    })
    .await;
    service
        .poll_cancel
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take();
    result
}

/// Control commands interrupt background reads before waiting for the mutation lock.
pub(crate) fn interrupt_poll(app: &App) {
    if let Some(cancel) = app
        .ext::<SessionService>()
        .poll_cancel
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
    {
        cancel.cancel();
        app.interrupt_hub().poke();
    }
}

pub fn close(app: &App, window: &WindowId, cleared: bool) {
    if let Some(s) = app.sessions().current(window) {
        app.sessions().clear(window);
        if cleared {
            let svc = app.ext::<SessionService>();
            let status = svc
                .feeds
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&s.key)
                .and_then(|c| c.status.clone());
            let mut parts = svc.parts.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(mut status) = status {
                status.crawl_id.clear();
                status.phase = CrawlPhase::Idle;
                status.crawled = 0;
                status.queued = 0;
                status.discovered = 0;
                status.percent = 0;
                status.rate = 0.0;
                status.avg_rate = 0.0;
                status.active_threads = 0;
                status.end_reason = None;
                status.eta_s = None;
                status.owner.live = false;
                // B3-UI (UI-003): a cleared view has no crawl; its start URL must not return to the header.
                status.start_url = None;
                parts.crawl = Some(status);
            }
            parts.analysis = Some(AnalysisStatus {
                steps: Vec::new(),
                running: false,
                current: None,
                percent: None,
            });
            let version = svc
                .version
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            parts.counts_version = Some(version);
            parts.issues_version = Some(version);
            parts.sidebar_version = Some(version);
        }
        app.crawl_events().emit(
            app,
            if cleared {
                CrawlEvent::Cleared(s.key.clone())
            } else {
                CrawlEvent::Closed(s.key.clone())
            },
        );
        if !app.sessions().all().iter().any(|other| other.key == s.key) {
            // M4-D05 (08 AI-020, Nachtrag K4-13): Schließen der Crawl-Datei bricht laufende KI-Requests ab (nicht
            // abwarten; Einträge `Pending`, keine Protokollzeile, Reservierung verfällt).
            os_ai::run::controls().abort(&s.path);
            app.unregister_crawl(&s.key);
            app.ext::<SessionService>()
                .feeds
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&s.key);
        }
    }
}

/// Activate event delivery when the first crawl session is attached.
pub fn activate(app: &App) {
    if !app
        .ext::<SessionService>()
        .active
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        app.set_tick_policy(Arc::new(crate::events::coalescer::Coalescer::default()));
    }
}

#[cfg(test)]
mod poll_tests {
    use super::*;

    // A poll that finds the session lock held waits for it and then publishes the state current at that time; it does
    // not return early with nothing published (CI windows-x64, `session_feed.rs:205`).
    // spec: UI-018
    #[tokio::test]
    async fn an_explicit_poll_waits_for_the_session_lock_and_then_publishes_the_new_state() {
        let mut cfg = os_model::CrawlConfig::default();
        cfg.analysis.auto_at_end = false;
        let fx = os_engine::testing::crawl_fixture("ui/fx-ui-dirs", cfg).await;
        let home = tempfile::tempdir().unwrap();
        let app = App::new(crate::AppConfig::new(home.path())).unwrap();
        let ctx = crate::CallCtx::for_window("main");
        app.dispatch(
            ctx.clone(),
            "crawl.open",
            serde_json::json!({"kind":"path","path":fx.file.path()}),
        )
        .await
        .unwrap();
        poll(&app).await.unwrap();
        let first = app.assemble_tick().and_then(|t| t.crawl).expect("status");
        assert_eq!(first.phase, CrawlPhase::Completed);
        let key = current(&app, &ctx.window).unwrap().key;

        // The lock is held (as by the background loop or a control command) while the file changes.
        let held = app
            .ext::<SessionService>()
            .mutation
            .clone()
            .lock_owned()
            .await;
        app.write_queue()
            .submit_file(&key, |f| {
                f.set_meta("state", "Paused")
                    .map_err(|e| crate::runtime::error::store_error(&e))
            })
            .await
            .unwrap();
        let waiting = {
            let app = app.clone();
            tokio::spawn(async move { poll(&app).await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!waiting.is_finished(), "poll waits for the lock");
        assert!(
            app.assemble_tick().and_then(|t| t.crawl).is_none(),
            "nothing is published while the lock is held"
        );
        drop(held);
        waiting.await.unwrap().unwrap();
        let after = app
            .assemble_tick()
            .and_then(|t| t.crawl)
            .expect("published");
        assert_eq!(after.phase, CrawlPhase::Paused);
    }
}

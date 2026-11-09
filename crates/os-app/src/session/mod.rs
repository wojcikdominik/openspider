//! Crawl sessions, bounded change feed and engine lifecycle (M2-K04).
pub mod analysis;
pub mod control;
pub mod feed;
pub mod merge;
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
            let ended = match poll_aligned(&app, due).await {
                Ok(ended) => ended,
                Err(e) => {
                    tracing::warn!(error = ?e, "Session poll failed");
                    false
                }
            };
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
                    // The end status goes out with the exact final count (UI-018, B4-LAGFIX2).
                    let until = tokio::time::Instant::now() + TICK_SETTLE_COUNTS;
                    while !crate::commands::sidebar::counts::settled(&app)
                        && tokio::time::Instant::now() < until
                    {
                        tokio::time::sleep(Duration::from_millis(20)).await;
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

pub async fn poll(app: &App) -> Result<(), CmdError> {
    poll_aligned(app, app.tick_due()).await.map(|_| ())
}

/// One poll of all sessions; `due`: this poll belongs to a due tick (see `feed::refresh_aligned`). `Ok(true)`:
/// a crawl ended in this poll and its final rows were published.
async fn poll_aligned(app: &App, due: bool) -> Result<bool, CmdError> {
    let service = app.ext::<SessionService>();
    let Ok(_guard) = service.mutation.try_lock() else {
        return Ok(false);
    };
    let cancel = os_model::CancelToken::new();
    *service
        .poll_cancel
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(cancel.clone());
    let result = crate::runtime::with_cancel(cancel, async {
        let mut seen = std::collections::HashSet::new();
        let mut ended = false;
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
            feed::refresh_aligned(app, &s, due || finished, finished).await?;
            ended |= finished;
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

//! Shared shutdown path for window close, menu and keyboard shortcuts (F-11).
pub use crate::dto::QuitDecision;
use crate::{
    dto::{ErrorCode, McpStatus, QuitReason},
    runtime::{error::parse_args, hooks::QuitHook},
    session::SessionService,
    App, CallCtx, CmdError,
};
use futures_util::future::BoxFuture;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Pause, checkpoint and WAL merge before exit (CRAWL-126: ≤ 3 s).
const QUIT_BUDGET: std::time::Duration = std::time::Duration::from_secs(3);

/// View-cache writes while quitting (CRAWL-126, OS-Festlegung vom 01.11.2026): writes queued before quitting began
/// still run, none are added afterwards, so no write follows the final WAL merge.
#[derive(Default)]
pub(crate) struct CacheGate {
    /// Set when quitting begins: no new view-cache submissions.
    closing: AtomicBool,
    /// Set on the writer thread once the jobs queued before it have run: a submission that raced with `closing` is
    /// skipped when it runs.
    sealed: Arc<AtomicBool>,
}

/// Permission to store derivable view facts (N21 counts and memberships, sidebar aggregates) through the write queue
/// (CRAWL-126). `None` once quitting has begun: callers submit nothing. A ticket taken just before is checked again on
/// the writer thread with [`CacheTicket::open`]; a skipped write is treated like a failed one (rebuilt later).
pub(crate) fn cache_ticket(app: &App) -> Option<CacheTicket> {
    let gate = app.ext::<CacheGate>();
    (!gate.closing.load(Ordering::SeqCst)).then(|| CacheTicket(gate.sealed.clone()))
}

/// See [`cache_ticket`].
pub(crate) struct CacheTicket(Arc<AtomicBool>);
impl CacheTicket {
    /// `false` once the quit drain has run the jobs queued before it: the write is skipped.
    pub(crate) fn open(&self) -> bool {
        !self.0.load(Ordering::SeqCst)
    }
}

/// Quitting begins: no new view-cache writes from here on (CRAWL-126).
fn close_cache(app: &App) {
    app.ext::<CacheGate>().closing.store(true, Ordering::SeqCst);
}

/// Quitting failed or was refused: the view cache may be written again.
fn reopen_cache(app: &App) {
    let gate = app.ext::<CacheGate>();
    gate.sealed.store(false, Ordering::SeqCst);
    gate.closing.store(false, Ordering::SeqCst);
}

/// CRAWL-126: first the app's own pending writes (write queue, view cache), then a WAL merge of every writable open
/// file, waited for within the rest of the quit budget. A merge still running at the end is logged; the file stays
/// consistent and is merged when it is opened next.
async fn drain(app: &App, started: std::time::Instant) {
    let sealed = app.ext::<CacheGate>().sealed.clone();
    // Runs after every job queued so far (strict order on the writer thread), like `flush`.
    let _ = app
        .write_queue()
        .submit_admin(move || {
            sealed.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;
    for s in app.sessions().all() {
        if s.writable {
            crate::session::merge::start(app, &s.key, s.path.clone());
        }
    }
    if !crate::session::merge::wait_all(app, QUIT_BUDGET.saturating_sub(started.elapsed())).await {
        tracing::warn!(
            "WAL merge still running at quit: the file is merged when it is opened next"
        );
    }
}

pub struct SessionQuit;
impl QuitHook for SessionQuit {
    fn request_quit(&self, app: App) -> BoxFuture<'static, QuitDecision> {
        Box::pin(async move {
            let mut reasons = Vec::new();
            if app.sessions().all().iter().any(|s| {
                s.engine
                    .as_ref()
                    .is_some_and(|e| e.handle.lock().unwrap_or_else(|p| p.into_inner()).is_some())
            }) {
                reasons.push(QuitReason::Crawl);
            }
            for op in app.ops().active_statuses() {
                let reason = match op.label_key.as_str() {
                    "op-analysis" => QuitReason::Analysis,
                    "op-export" => QuitReason::Export,
                    _ => QuitReason::Crawl,
                };
                if !reasons.contains(&reason) {
                    reasons.push(reason);
                }
            }
            if mcp_connected(&app).await {
                reasons.push(QuitReason::McpSessions);
            }
            if reasons.is_empty() {
                // CRAWL-126: pending own writes first, then every writable file is merged before the app exits.
                let started = std::time::Instant::now();
                close_cache(&app);
                drain(&app, started).await;
                QuitDecision::Quit
            } else {
                QuitDecision::Confirm { reasons }
            }
        })
    }
}
async fn mcp_connected(app: &App) -> bool {
    app.dispatch(
        CallCtx::for_window("main"),
        "mcp.status",
        serde_json::Value::Null,
    )
    .await
    .ok()
    .and_then(|v| parse_args::<McpStatus>(v).ok())
    .is_some_and(|s| matches!(s,McpStatus::Connected { clients,.. } if clients > 0))
}

pub async fn confirm(app: &App, accepted: bool) -> Result<(), CmdError> {
    if !accepted {
        return Ok(());
    }
    let service = app.ext::<SessionService>();
    if service.quitting.swap(true, Ordering::SeqCst) {
        return Err(crate::session::busy());
    }
    let started = std::time::Instant::now();
    close_cache(app);
    let result = async {
        // M4-D05 (08 AI-020, Nachtrag K4-13; CRAWL-126): laufende KI-Requests abbrechen, nicht abwarten – die Einträge
        // bleiben `Pending`, ohne Protokollzeile; ihre Reservierung verfällt.
        os_ai::run::controls().abort_all();
        let active = app.ops().active_statuses();
        app.ops().cancel_all();
        // Do not exit while an exporter/analysis is still writing. Fail safely if it
        // cannot cooperate; the user can retry. Request drain is handled below.
        for op in active {
            tokio::time::timeout(std::time::Duration::from_secs(3), app.ops().wait(&op.op_id))
                .await
                .map_err(|_| CmdError::new(ErrorCode::Busy, "err-quit-operation-running"))?;
        }
        // Stop scheduling work immediately, even while a background feed holds
        // the session lock. Durability is still awaited below before exit.
        for s in app.sessions().all() {
            if let Some(engine) = s.engine {
                if let Some(handle) = engine
                    .handle
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .as_ref()
                {
                    handle.pause();
                }
            }
        }
        crate::session::interrupt_poll(app);
        let _guard = service.mutation.lock().await;
        for s in app.sessions().all() {
            if s.engine
                .as_ref()
                .is_some_and(|e| e.handle.lock().unwrap_or_else(|p| p.into_inner()).is_some())
            {
                crate::session::control::settle(app, &s, Some(false)).await?;
            }
        }
        if mcp_connected(app).await {
            app.dispatch(
                CallCtx::for_window("main"),
                "mcp.stop",
                serde_json::Value::Null,
            )
            .await?;
        }
        // CRAWL-126: exit only after the own pending writes ran and the files are merged, within the quit budget.
        drain(app, started).await;
        Ok(())
    }
    .await;
    if result.is_ok() && crate::update::install::on_quit(app).await {
        if let Some(host) = app.window_host() {
            host.exit();
        }
    } else {
        reopen_cache(app);
    }
    service.quitting.store(false, Ordering::SeqCst);
    result
}

#[cfg(test)]
#[path = "../tests/fix_wal/gate.rs"]
mod gate_tests;

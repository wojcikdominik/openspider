//! Snapshot-consistent, bounded feed; overflow and reset_seq request a rebuild.
use super::SessionService;
use crate::{dto::*, pool::CrawlKey, runtime::sessions::SessionInfo, App, CmdError};
use os_model::{CrawlState, Progress, UrlId};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

pub const CHANGE_LIMIT: u32 = 4096;
/// Longest wait of a due poll for the engine to write the URLs it has already counted (UI-018, B4-LAGFIX2).
/// The request is only made while the writer has nothing queued; then one commit takes a few milliseconds.
pub const COUNTED_FLUSH_MAX: Duration = Duration::from_millis(100);
#[derive(Clone, Default)]
pub(crate) struct Cursor {
    seq: u64,
    reset: u64,
    last: Option<Duration>,
    rows_at: Option<Duration>,
    pending: Option<ChangeSet>,
    pub(super) status: Option<CrawlStatus>,
    memory_throttled: bool,
    /// Crawl-delay notices already pushed per origin, `(seconds, applied)`. Shared by every clone of the
    /// cursor, so concurrent refreshes (poll and control paths, e.g. around a resume) push each notice once
    /// (M2-N28, CRAWL-145). It lives as long as the cursor: pause and resume keep it.
    delays: Arc<std::sync::Mutex<std::collections::HashMap<String, (f64, bool)>>>,
    /// Parsed `Crawl-delay` per origin (`None`: none for the effective agent). A stored robots.txt does not
    /// change during a crawl; each origin is read and parsed once, not on every data version (UI-145).
    robots: Arc<std::sync::Mutex<std::collections::HashMap<String, Option<f64>>>>,
}
#[derive(Clone, Debug)]
pub struct ChangeSet {
    pub key: CrawlKey,
    pub seq: u64,
    pub ids: Vec<UrlId>,
    /// Deletion, new session or batch larger than CHANGE_LIMIT: rebuild the view.
    pub reset: bool,
}
pub type ChangeListener = dyn Fn(&App, &ChangeSet) + Send + Sync;
pub fn subscribe(app: &App, f: Arc<ChangeListener>) {
    app.ext::<SessionService>()
        .listeners
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push(f);
}

pub fn phase(s: CrawlState) -> CrawlPhase {
    match s {
        CrawlState::Running => CrawlPhase::Running,
        CrawlState::Paused => CrawlPhase::Paused,
        CrawlState::Completed => CrawlPhase::Completed,
        CrawlState::Incomplete => CrawlPhase::Incomplete,
        CrawlState::Failed => CrawlPhase::Failed,
    }
}
/// Prozent eines abgeschlossenen Crawls (UI-013: 100 % erst bei leerer Warteschlange). Ein gestoppter Crawl
/// (`EndReason::Stopped`) mit Rest zeigt den Anteil der erledigten URLs, höchstens 99; andere Gründe (Warteschlange
/// leer, Grenze erreicht) behalten den Wert der Engine, eine leere Warteschlange ergibt 100.
pub(crate) fn completed_percent(
    engine: u8,
    end_reason: Option<os_model::EndReason>,
    crawled: u64,
    queued: u64,
) -> u8 {
    if queued == 0 {
        return 100;
    }
    if end_reason == Some(os_model::EndReason::Stopped) {
        let total = crawled.saturating_add(queued);
        return (u128::from(crawled) * 100 / u128::from(total)).min(99) as u8;
    }
    engine
}
pub fn mode(m: os_model::CrawlMode) -> CrawlModeDto {
    match m {
        os_model::CrawlMode::Spider => CrawlModeDto::Spider,
        os_model::CrawlMode::List => CrawlModeDto::List,
        os_model::CrawlMode::Serp => CrawlModeDto::Serp,
    }
}
pub fn from_progress(id: String, m: CrawlModeDto, owner: CrawlOwner, p: &Progress) -> CrawlStatus {
    CrawlStatus {
        crawl_id: id,
        phase: phase(p.state),
        mode: m,
        owner,
        end_reason: p.end_reason.map(|r| r.as_str().into()),
        crawled: p.crawled,
        queued: p.queued,
        discovered: p.discovered,
        percent: p.percent,
        rate: p.rate,
        avg_rate: p.avg_rate,
        avg_response_time: p.avg_response_time,
        elapsed_s: p.elapsed_s,
        paused_s: p.paused_s,
        eta_s: p.eta_s,
        active_threads: p.active_threads,
        memory_stage: p.memory_stage.map(|s| match s {
            os_model::MemoryStage::Normal => MemoryStageDto::Normal,
            os_model::MemoryStage::Warning => MemoryStageDto::Warning,
            os_model::MemoryStage::Paused => MemoryStageDto::Paused,
        }),
        file_bytes: p.file_bytes,
        memory_used_bytes: p.memory_used_bytes,
        memory_limit_bytes: p.memory_limit_bytes,
        merging: None,
        start_url: None,
    }
}

/// Read metadata on a fixed lane, independent of aggregate readers; never opens a writing connection for foreign crawls.
pub async fn snapshot(
    app: &App,
    key: CrawlKey,
) -> Result<(CrawlSummaryDto, os_model::CrawlConfig, CrawlState), CmdError> {
    let clock = app.config().clock.clone();
    app.read_pool()
        .run_cached(
            key,
            "session-meta",
            crate::runtime::current_cancel(),
            move |f, _| {
                let m = f.meta()?;
                let state = f.effective_state(clock.as_ref())?;
                let pid = f.get_meta("owner_pid")?.and_then(|s| s.parse().ok());
                let live = state == CrawlState::Running;
                let owner = CrawlOwner {
                    kind: match f.get_meta("owner_kind")?.as_deref() {
                        Some("cli") => OwnerKind::Cli,
                        Some("mcp-stdio") => OwnerKind::Mcp,
                        Some("scheduler") => OwnerKind::Scheduler,
                        _ => OwnerKind::App,
                    },
                    pid,
                    live,
                    controllable: !live || pid == Some(std::process::id()),
                };
                // UI-147: opening needs only the URL total. Do not scan crawled
                // rows (including their table payload) for counters discarded here.
                // UI-061/CRAWL-130: the same total as Overview, report and status bar.
                let urls = os_store::counters::encountered(f.conn())?;
                let file_bytes = std::fs::metadata(f.path())
                    .map_err(|e| os_store::StoreError::Io {
                        path: f.path().to_path_buf(),
                        source: e,
                    })?
                    .len();
                Ok((
                    CrawlSummaryDto {
                        crawl_id: m.crawl_id.to_string(),
                        file: f.path().to_string_lossy().into_owned(),
                        name: m.name,
                        phase: phase(state),
                        mode: if f.get_meta("source_mode")?.as_deref() == Some("apis") {
                            CrawlModeDto::Apis
                        } else {
                            mode(m.mode)
                        },
                        started_at: m.started_at.unwrap_or_default().to_db(),
                        finished_at: m.ended_at.map(|t| t.to_db()),
                        urls,
                        file_bytes,
                        schema_version: m.schema_version,
                        owner,
                    },
                    f.config()?,
                    state,
                ))
            },
        )
        .await
}

pub async fn refresh(app: &App, s: &SessionInfo) -> Result<(), CmdError> {
    let service = app.ext::<SessionService>();
    // The entry is created here, so concurrent first refreshes share its notice state (`Cursor::delays`).
    let mut old = service
        .feeds
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .entry(s.key.clone())
        .or_default()
        .clone();
    let now = app.config().clock.monotonic();
    if old
        .last
        .is_some_and(|t| now.saturating_sub(t) < Duration::from_millis(50))
    {
        return Ok(());
    }
    let memory_throttled = s.engine.is_some() && os_engine::memory::sample().low_available();
    if old.memory_throttled != memory_throttled {
        app.push_notice(Notice {
            level: NoticeLevel::Info,
            code: if memory_throttled {
                "memory-throttled"
            } else {
                "memory-throttle-cleared"
            }
            .into(),
            args: Default::default(),
        });
        old.memory_throttled = memory_throttled;
    }
    let before = old.clone();
    let clock = app.config().clock.clone();
    let cfg = s.config.clone();
    // UI-018 (B4-LAGFIX2): in a poll whose tick is due the engine first writes every URL it has counted, so
    // the snapshot below contains them and the table totals of this tick reach the counter of this tick
    // (otherwise up to one writer interval, 100 ms, later and thus one tick late). Read the progress first.
    let due = app.tick_due();
    let (progress, counted) = s
        .engine
        .as_ref()
        .and_then(|e| {
            e.handle
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .as_ref()
                .map(|h| (h.progress(), due.then(|| h.flush_counted()).flatten()))
        })
        .map_or((None, None), |(p, f)| (Some(p), f));
    if let Some(counted) = counted {
        let _ = tokio::time::timeout(COUNTED_FLUSH_MAX, counted).await;
    }
    // A saturated catalog/count pool must not freeze progress or lifecycle delivery.
    // The fixed lane also retains the connection-local incremental aggregate cache.
    let (seq, reset, ids, mut status) = app
        .read_pool()
        .run_cached(
            s.key.clone(),
            "session-feed",
            crate::runtime::current_cancel(),
            move |f, _| {
                let tx = f.conn().unchecked_transaction()?;
                let seq = os_store::queries::change_seq(&tx)?;
                let reset = f
                    .get_meta("reset_seq")?
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let ids = if seq != before.seq {
                    os_store::queries::changed_since(&tx, before.seq, CHANGE_LIMIT + 1)?
                } else {
                    Vec::new()
                };
                let m = f.meta()?;
                // Resume returns before its first metadata batch is committed. The
                // local handle is authoritative until its durable completion is joined.
                let local = progress.is_some();
                let state = match &progress {
                    // A terminal watch value precedes the writer flush/checkpoint.
                    // Only settle() joining the handle may publish that terminal state.
                    Some(_) => CrawlState::Running,
                    None => f.effective_state(clock.as_ref())?,
                };
                let end_reason = match &progress {
                    Some(_) => None,
                    None => m.end_reason,
                };
                let pid = if local {
                    Some(std::process::id())
                } else {
                    f.get_meta("owner_pid")?.and_then(|v| v.parse().ok())
                };
                let owner = CrawlOwner {
                    kind: match f.get_meta("owner_kind")?.as_deref() {
                        Some("cli") => OwnerKind::Cli,
                        Some("mcp-stdio") => OwnerKind::Mcp,
                        Some("scheduler") => OwnerKind::Scheduler,
                        _ => OwnerKind::App,
                    },
                    pid,
                    live: state == CrawlState::Running,
                    controllable: state != CrawlState::Running || pid == Some(std::process::id()),
                };
                let p = progress.or(f
                    .get_meta("progress_snapshot")?
                    .and_then(|v| serde_json::from_str::<Progress>(&v).ok()));
                let mut status = if let Some(p) = p {
                    from_progress(m.crawl_id.to_string(), mode(m.mode), owner, &p)
                } else {
                    let mut st = before.status.unwrap_or(CrawlStatus {
                        crawl_id: m.crawl_id.to_string(),
                        phase: phase(state),
                        mode: if f.get_meta("source_mode")?.as_deref() == Some("apis") {
                            CrawlModeDto::Apis
                        } else {
                            mode(m.mode)
                        },
                        end_reason: None,
                        crawled: 0,
                        queued: 0,
                        discovered: 0,
                        percent: 0,
                        rate: 0.0,
                        avg_rate: 0.0,
                        avg_response_time: None,
                        elapsed_s: 0,
                        paused_s: 0,
                        eta_s: None,
                        active_threads: 0,
                        memory_stage: None,
                        file_bytes: None,
                        owner: owner.clone(),
                        memory_used_bytes: None,
                        memory_limit_bytes: None,
                        merging: None,
                        start_url: None,
                    });
                    if before.last.is_none() || seq != before.seq {
                        let counts = os_store::queries::session_counts(&tx)?;
                        st.discovered = counts.discovered;
                        st.queued = counts.queued;
                        st.crawled = counts.crawled;
                    }
                    st.owner = owner;
                    st
                };
                if state != CrawlState::Running && status.phase == CrawlPhase::Running {
                    let counts = os_store::queries::session_counts(&tx)?;
                    status.discovered = counts.discovered;
                    status.queued = counts.queued;
                    status.crawled = counts.crawled;
                }
                if state == CrawlState::Completed {
                    status.percent = completed_percent(
                        status.percent,
                        end_reason,
                        status.crawled,
                        status.queued,
                    );
                }
                if f.get_meta("source_mode")?.as_deref() == Some("apis") {
                    status.mode = CrawlModeDto::Apis;
                }
                status.phase = phase(state);
                status.end_reason = end_reason.map(|r| r.as_str().into());
                // UI-003 (B3-UI): the header shows the start URL of an opened crawl.
                status.start_url = m.start_url.clone();
                if state != CrawlState::Running {
                    status.active_threads = 0;
                    status.rate = 0.0;
                }
                tx.commit()?;
                if seq != before.seq && ids.len() <= CHANGE_LIMIT as usize {
                    // K03 maintains its read-side aggregate cache with its own transaction,
                    // directly after the snapshot read above has been closed (nested
                    // transactions are not possible on one connection).
                    os_analyze::aggregates::live::apply_changes(f, &cfg, &ids)
                        .map_err(|e| os_store::StoreError::InvalidInput(e.to_string()))?;
                }
                Ok((seq, reset, ids, status))
            },
        )
        .await?;
    status.owner.controllable &= s.writable;
    let state = match status.phase {
        CrawlPhase::Running => CrawlState::Running,
        CrawlPhase::Paused => CrawlState::Paused,
        CrawlPhase::Completed => CrawlState::Completed,
        CrawlPhase::Failed => CrawlState::Failed,
        _ => CrawlState::Incomplete,
    };
    if !app.sessions().refresh_state(&s.key, state) {
        return Ok(());
    }
    let changed = old.last.is_none() || old.seq != seq || old.reset != reset;
    if changed {
        let token = os_fetch::user_agents::resolve(&s.config.user_agent).robots_token;
        let known: Vec<String> = old
            .robots
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .keys()
            .cloned()
            .collect();
        let bodies = app
            .read_pool()
            .run_cached(
                s.key.clone(),
                "session-feed",
                crate::runtime::current_cancel(),
                move |f, _| os_analyze::aggregates::status::robots_bodies(f, &known),
            )
            .await?;
        let delays: Vec<(String, f64)> = {
            let mut robots = old.robots.lock().unwrap_or_else(|p| p.into_inner());
            for (origin, body) in bodies {
                let delay = os_fetch::robots::parse(&body).crawl_delay(&token);
                robots.insert(origin, delay);
            }
            let mut delays: Vec<_> = robots
                .iter()
                .filter_map(|(origin, delay)| delay.map(|seconds| (origin.clone(), seconds)))
                .collect();
            delays.sort_by(|a, b| a.0.cmp(&b.0));
            delays
        };
        let applied = s.config.robots.respect_crawl_delay;
        for (origin, seconds) in delays {
            let fresh = old
                .delays
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(origin.clone(), (seconds, applied))
                != Some((seconds, applied));
            if fresh {
                app.push_notice(Notice {
                    level: NoticeLevel::Info,
                    code: "crawl-delay".into(),
                    args: [
                        ("origin".into(), serde_json::json!(origin)),
                        ("seconds".into(), serde_json::json!(seconds)),
                        ("applied".into(), serde_json::json!(applied)),
                        (
                            "effectiveSeconds".into(),
                            serde_json::json!(if applied {
                                seconds.clamp(0.0, 60.0)
                            } else {
                                0.0
                            }),
                        ),
                        (
                            "capped".into(),
                            serde_json::json!(applied && seconds > 60.0),
                        ),
                    ]
                    .into(),
                });
            }
        }
        let rebuild = old.last.is_none()
            || old.reset != reset
            || seq < old.seq
            || ids.len() > CHANGE_LIMIT as usize;
        let change = ChangeSet {
            key: s.key.clone(),
            seq,
            ids: if rebuild { Vec::new() } else { ids },
            reset: rebuild,
        };
        if let Some(pending) = old.pending.as_mut() {
            pending.seq = change.seq;
            pending.reset |= change.reset;
            if !pending.reset {
                pending.ids.extend(change.ids);
                pending.ids.sort_unstable();
                pending.ids.dedup();
                pending.reset = pending.ids.len() > CHANGE_LIMIT as usize;
            }
            if pending.reset {
                pending.ids.clear();
            }
        } else {
            old.pending = Some(change);
        }
        let version = service.version.fetch_add(1, Ordering::SeqCst) + 1;
        let mut parts = service.parts.lock().unwrap_or_else(|p| p.into_inner());
        parts.counts_version = Some(version);
        parts.issues_version = Some(version);
        parts.sidebar_version = Some(version);
    }
    if old.status.as_ref() != Some(&status) {
        service
            .parts
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .crawl = Some(status.clone());
    }
    old.status = Some(status);
    old.seq = seq;
    old.reset = reset;
    old.last = Some(now);
    // Counters above are independent of the configurable table cadence (K2-39).
    // Accumulate IDs between row publications; overflow becomes one full reset.
    // UI-018 (B4-LAGFIX2): rows are published in a poll whose tick is due. View maintenance then
    // starts with the status it belongs to, and the session loop sends both in the same tick instead of
    // the totals following one tick later on an independent table clock. Should another flusher take
    // the due tick, rows still follow after two intervals.
    let interval = Duration::from_millis(u64::from(app.live_interval_ms()));
    if old.pending.is_some()
        && old.rows_at.is_none_or(|t| {
            let elapsed = now.saturating_sub(t);
            elapsed >= interval && (elapsed >= interval * 2 || app.tick_due())
        })
    {
        let change = old.pending.take().expect("pending change");
        let listeners = service
            .listeners
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        for f in listeners {
            f(app, &change);
        }
        old.rows_at = Some(now);
    }
    service
        .feeds
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(s.key.clone(), old);
    Ok(())
}

// Control acknowledgements use the joined engine's durable progress. They must
// not queue behind aggregate rebuilds or a process-wide memory sample (Windows).
// Keep the feed cursor's sequence untouched so the next poll still refreshes rows.
pub(super) fn settled(app: &App, s: &SessionInfo, progress: &Progress) {
    let service = app.ext::<SessionService>();
    let mut feeds = service.feeds.lock().unwrap_or_else(|p| p.into_inner());
    let cursor = feeds.entry(s.key.clone()).or_default();
    let mode = cursor
        .status
        .as_ref()
        .map_or(mode(s.config.mode), |s| s.mode);
    let start_url = cursor.status.as_ref().and_then(|s| s.start_url.clone());
    let mut status = from_progress(
        s.key.0.clone(),
        mode,
        CrawlOwner {
            kind: OwnerKind::App,
            pid: None,
            live: false,
            controllable: s.writable,
        },
        progress,
    );
    status.active_threads = 0;
    status.rate = 0.0;
    status.start_url = start_url;
    if progress.state == CrawlState::Completed {
        status.percent = completed_percent(
            status.percent,
            progress.end_reason,
            status.crawled,
            status.queued,
        );
    }
    cursor.status = Some(status.clone());
    cursor.last = None;
    service
        .parts
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .crawl = Some(status);
}

pub(super) fn stopped(app: &App, s: &SessionInfo) -> bool {
    let service = app.ext::<SessionService>();
    let mut feeds = service.feeds.lock().unwrap_or_else(|p| p.into_inner());
    let Some(cursor) = feeds.get_mut(&s.key) else {
        return false;
    };
    let Some(status) = cursor.status.as_mut() else {
        return false;
    };
    status.phase = CrawlPhase::Completed;
    status.end_reason = Some("stopped".into());
    status.percent = completed_percent(
        status.percent,
        Some(os_model::EndReason::Stopped),
        status.crawled,
        status.queued,
    );
    status.active_threads = 0;
    status.rate = 0.0;
    status.owner.live = false;
    status.owner.pid = None;
    cursor.last = None;
    service
        .parts
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .crawl = Some(status.clone());
    true
}

pub fn invalidate(app: &App, key: &CrawlKey) {
    if let Some(cursor) = app
        .ext::<SessionService>()
        .feeds
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_mut(key)
    {
        cursor.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::completed_percent;
    use os_model::EndReason;

    // spec: UI-013
    #[test]
    fn a_stopped_crawl_with_a_remaining_queue_never_reports_one_hundred_percent() {
        assert_eq!(completed_percent(100, Some(EndReason::Stopped), 11, 16), 40);
        assert_eq!(completed_percent(100, Some(EndReason::Stopped), 999, 1), 99);
        assert_eq!(completed_percent(100, Some(EndReason::Stopped), 11, 0), 100);
        assert_eq!(
            completed_percent(100, Some(EndReason::QueueEmpty), 27, 0),
            100
        );
        // Grenze erreicht: der Wert der Engine bleibt (CRAWL-130)
        assert_eq!(
            completed_percent(100, Some(EndReason::LimitReached), 10, 5),
            100
        );
    }
}

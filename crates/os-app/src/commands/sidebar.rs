//! Overview and issues share the analyze filter counts and issue resources.
use super::catalog::{self, session, view_error};
use crate::pool::CrawlKey;
use crate::{
    dto::*,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{
        error::{internal, parse_args, to_json},
        App, CallCtx,
    },
};
use futures_util::future::BoxFuture;

pub(crate) mod counts;
pub(crate) mod partition;

/// UI-061 summary shares one denominator and rounding rule with its export.
pub(crate) fn summary_rows(summary: Vec<(String, u64)>) -> Vec<SummaryRow> {
    let encountered = summary.first().map_or(0, |r| r.1);
    summary
        .into_iter()
        .map(|(key, count)| SummaryRow {
            key,
            count,
            pct: (encountered > 0)
                .then(|| (count as f64 * 1000.0 / encountered as f64).round() / 10.0),
        })
        .collect()
}
fn overview<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if args.get("comparison").and_then(serde_json::Value::as_bool) == Some(true) {
            return super::compare::ui::overview(app, ctx).await;
        }
        let args: ScopeArgs = parse_args(args)?;
        if let Some(result) = super::compare::change_overview(app, &ctx, &args, true).await {
            return result;
        }
        let change_rows = super::compare::change_counts(app, &ctx, args.compare).await?;
        let compare_scope = if matches!(
            args.compare,
            Some(CompareClass::Changed | CompareClass::NotChanged)
        ) {
            None
        } else {
            args.compare
        };
        catalog::check_scope(&args.segment, &compare_scope)?;
        let s = session(app, &ctx)?;
        // UI-018 (M2-N27): the shared count of Overview, Issues and filter counts.
        let shared = counts::shared_facts(app, &s, &ctx.cancel).await?;
        let age_ms = shared.age_ms();
        let facts = shared.facts;
        let preview = super::ai_preview::switch(app);
        app.read_pool()
            .run_snapshot(s.key, move |file, _| {
                let summary = summary_rows(facts.summary.clone());
                // Plan M4 E1 (E-2 b, M4-F02): ohne Schalter „KI (Vorschau)“ keine KI-Tabs in der Übersicht.
                let registry = super::ai_preview::visible_tabs(
                    preview,
                    os_analyze::tabs::configured_for_file(&s.config, file),
                );
                let mut rows = catalog::count_rows(&registry, &s.config, &facts.counts);
                rows.retain(|r| r.tab != "Change Detection");
                rows.extend(change_rows);
                let tabs = registry
                    .iter()
                    .filter_map(|t| {
                        let filters = rows
                            .iter()
                            .filter(|r| r.tab == t.id)
                            .map(|r| r.filter.clone())
                            .collect::<Vec<_>>();
                        (!filters.is_empty()).then(|| OverviewTab {
                            tab: t.id.into(),
                            // UI-102: which filters of this tab form a partition (no new count).
                            partition: partition::partition(t, &filters, &s.config),
                            filters,
                        })
                    })
                    .collect();
                Ok(to_json(&OverviewDto {
                    summary,
                    tabs,
                    age_ms,
                }))
            })
            .await?
    })
}
pub(crate) fn issue_rows(
    counts: &[os_analyze::views::FilterCount],
    args: &IssuesArgs,
) -> Vec<IssueRow> {
    let search = args.search.as_deref().unwrap_or("").to_lowercase();
    os_analyze::issues::issues_from_counts(counts)
        .into_iter()
        .map(|r| IssueRow {
            id: format!("{}/{}", r.tab, r.filter),
            tab: r.tab,
            filter: r.filter,
            issue_type: catalog::issue_type(r.issue_type),
            priority: catalog::priority(r.priority),
            count: r.count,
            pct: r.pct,
            segment_shares: vec![],
            delta: None,
        })
        .filter(|r| {
            args.issue_type.is_none_or(|t| t == r.issue_type)
                && args.priority.is_none_or(|p| p == r.priority)
                && format!("{}: {}", r.tab, r.filter)
                    .to_lowercase()
                    .contains(&search)
        })
        .collect()
}
fn issues<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: IssuesArgs = parse_args(args)?;
        catalog::check_scope(&args.segment, &args.compare)?;
        let s = session(app, &ctx)?;
        let facts = counts::shared_facts(app, &s, &ctx.cancel).await?.facts;
        to_json(&issue_rows(&facts.counts, &args))
    })
}

/// UI-145/UI-147: exact facts of the current data version (crawl not running, [`counts`]). Missing
/// pieces are counted by one cancellable background count per crawl ([`count_job`], progress and
/// cancel via `sidebar.countStatus`/`sidebar.countCancel`, UI-141); the stored pieces are reused.
/// Cancelling ends the count with `Cancelled`; the next count resumes from the stored pieces.
pub(crate) async fn exact_facts(
    app: &App,
    s: &crate::runtime::sessions::SessionInfo,
    cancel: os_model::CancelToken,
) -> Result<counts::Counted, CmdError> {
    let cfg = s.config.clone();
    let progress = app
        .read_pool()
        .run_cached(
            s.key.clone(),
            "overview",
            cancel.clone(),
            move |file, cancel| Ok(os_analyze::views::overview_progress(file, &cfg, cancel, 0)),
        )
        .await?
        .map_err(view_error)?;
    if !progress.complete() {
        let job = count_job(app, s);
        let mut finished = job.finished.subscribe();
        let result = crate::views::until_cancelled(&cancel, async move {
            finished
                .wait_for(Option::is_some)
                .await
                .map(|r| r.clone().unwrap_or(Ok(())))
        })
        .await?;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(error),
            Err(_) => return Err(internal("sidebar.count", "Zählung beendet")),
        }
    }
    let cfg = s.config.clone();
    let (facts, pending, seq, stamp) = app
        .read_pool()
        .run_cached(s.key.clone(), "overview", cancel, move |file, cancel| {
            // Data version and facts from one snapshot (M2-N27: the shared count keys on it).
            let _snapshot = file.conn().unchecked_transaction()?;
            let seq = os_store::queries::change_seq(file.conn())?;
            let stamp = os_analyze::views::filter_cache_stamp(file)?;
            Ok(os_analyze::views::overview_facts(file, &cfg, cancel)
                .map(|(facts, pending)| (facts, pending, seq, stamp)))
        })
        .await?
        .map_err(view_error)?;
    if pending {
        crate::views::persist_facts(app, &s.key);
    }
    Ok(counts::Counted { facts, seq, stamp })
}

/// One-time count of the missing Overview/Issues pieces of a crawl (UI-141, UI-145). Not an `Ops`
/// operation: it is derivable background work and must neither block resuming a crawl nor ask for
/// confirmation on quit. Progress and cancellation via `sidebar.countStatus`/`sidebar.countCancel`.
pub(crate) struct CountJob {
    token: os_model::CancelToken,
    progress: std::sync::Mutex<(u64, u64)>,
    finished: tokio::sync::watch::Sender<Option<Result<(), CmdError>>>,
    /// First report (stored pieces only): proves resuming without recounting.
    #[cfg(test)]
    pub(crate) first: std::sync::Mutex<Option<(u64, u64)>>,
}
impl CountJob {
    /// Completion of the count (`Some` once finished).
    #[cfg(test)]
    pub(crate) fn finished(&self) -> tokio::sync::watch::Receiver<Option<Result<(), CmdError>>> {
        self.finished.subscribe()
    }
    fn running(&self) -> bool {
        self.finished.borrow().is_none()
    }
    fn progress(&self) -> (u64, u64) {
        *self.progress.lock().unwrap_or_else(|p| p.into_inner())
    }
    #[cfg(test)]
    pub(crate) fn progress_for_test(&self) -> std::sync::MutexGuard<'_, (u64, u64)> {
        self.progress.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Running count per crawl.
#[derive(Default)]
pub(crate) struct CountJobs(
    std::sync::Mutex<std::collections::HashMap<CrawlKey, std::sync::Arc<CountJob>>>,
);
impl CountJobs {
    fn lock(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::HashMap<CrawlKey, std::sync::Arc<CountJob>>>
    {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Cancels the crawl's visible count and forgets its shared count state (closed, cleared or
/// started crawl). The quiet precomputation after opening shares the `overview` lane;
/// `sidebar.countCancel` stops it as well.
pub(crate) fn cancel_count(app: &App, key: &CrawlKey) -> bool {
    let shared = counts::forget(app, key);
    cancel_job(app, key) | shared
}

fn cancel_job(app: &App, key: &CrawlKey) -> bool {
    match app.ext::<CountJobs>().lock().get(key) {
        Some(job) if job.running() => {
            job.token.cancel();
            true
        }
        _ => false,
    }
}

/// Counts the missing Overview/Issues pieces: first report immediately (stored pieces only), then up
/// to four tabs per step (counted at once); every step is stored before the next, so an interrupted
/// count (cancel, closed crawl, ended process) resumes where it stopped.
pub(crate) async fn count_steps(
    app: &App,
    session: &crate::runtime::sessions::SessionInfo,
    token: &os_model::CancelToken,
    report: impl Fn(u64, u64),
) -> Result<(), CmdError> {
    let mut step = 0;
    let mut last = 0;
    loop {
        let cfg = session.config.clone();
        let progress = app
            .read_pool()
            .run_cached(
                session.key.clone(),
                "overview",
                token.clone(),
                move |file, cancel| {
                    Ok(os_analyze::views::overview_progress(
                        file, &cfg, cancel, step,
                    ))
                },
            )
            .await?
            .map_err(view_error)?;
        report(progress.done as u64, progress.total as u64);
        // Stored pieces are an optional cache: a read-only crawl keeps them in memory.
        // CRAWL-126: no new view-cache write once quitting has begun.
        let stored = match crate::quit::cache_ticket(app) {
            None => Ok(false),
            Some(ticket) => {
                app.write_queue()
                    .submit_file(&session.key, move |file| {
                        if !ticket.open() {
                            return Ok(false);
                        }
                        os_analyze::views::persist_facts(file).map_err(view_error)
                    })
                    .await
            }
        };
        if let Err(error) = stored {
            tracing::debug!(?error, "Overview-Zähler nicht gespeichert");
        }
        if progress.complete() {
            return Ok(());
        }
        if token.is_cancelled() {
            return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
        }
        if step > 0 && progress.done <= last {
            // UI-141: a new data version (running crawl) discarded the counted steps, and a
            // running crawl stores nothing. Finish in one snapshot instead of restarting forever.
            let cfg = session.config.clone();
            let progress = app
                .read_pool()
                .run_cached(
                    session.key.clone(),
                    "overview",
                    token.clone(),
                    move |file, cancel| {
                        Ok(os_analyze::views::overview_progress(
                            file,
                            &cfg,
                            cancel,
                            usize::MAX,
                        ))
                    },
                )
                .await?
                .map_err(view_error)?;
            report(progress.done as u64, progress.total as u64);
            return Ok(());
        }
        last = progress.done;
        step = 4;
    }
}

/// Starts the one-time count of the missing pieces, or joins the running one.
pub(crate) fn count_job(
    app: &App,
    s: &crate::runtime::sessions::SessionInfo,
) -> std::sync::Arc<CountJob> {
    let jobs = app.ext::<CountJobs>();
    let mut running = jobs.lock();
    if let Some(job) = running.get(&s.key).filter(|j| j.running()) {
        return job.clone();
    }
    let (finished, _) = tokio::sync::watch::channel(None);
    let job = std::sync::Arc::new(CountJob {
        token: os_model::CancelToken::new(),
        progress: std::sync::Mutex::new((0, 0)),
        finished,
        #[cfg(test)]
        first: std::sync::Mutex::new(None),
    });
    running.insert(s.key.clone(), job.clone());
    let (worker, session, task) = (app.clone(), s.clone(), job.clone());
    app.handle().spawn(async move {
        let result = count_steps(&worker, &session, &task.token, |done, total| {
            *task.progress.lock().unwrap_or_else(|p| p.into_inner()) = (done, total);
            #[cfg(test)]
            task.first
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get_or_insert((done, total));
        })
        .await;
        task.finished.send_replace(Some(result));
        // A finished count is not kept: waiting callers hold their own reference.
        let jobs = worker.ext::<CountJobs>();
        let mut jobs = jobs.lock();
        if jobs
            .get(&session.key)
            .is_some_and(|j| std::sync::Arc::ptr_eq(j, &task))
        {
            jobs.remove(&session.key);
        }
    });
    job
}

fn count_status<'a>(app: &'a App, ctx: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let s = session(app, &ctx)?;
        let job = app.ext::<CountJobs>().lock().get(&s.key).cloned();
        let (mut running, (mut done, mut total)) =
            job.map_or((false, (0, 0)), |j| (j.running(), j.progress()));
        if !running && counts::counting(app, &s.key) {
            // UI-141: a shared count of the running crawl counts all tabs in one pass, without steps.
            (running, done, total) = (true, 0, 0);
        }
        to_json(&CountStatus {
            running,
            done,
            total,
        })
    })
}

fn count_cancel<'a>(app: &'a App, ctx: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let s = session(app, &ctx)?;
        // The user's cancel keeps the last shared count visible (UI-018, M2-N27).
        cancel_job(app, &s.key);
        counts::cancel_flight(app, &s.key);
        super::sidebar_ext::cancel_precompute(app, &s.key);
        Ok(serde_json::Value::Null)
    })
}
fn explain<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: IssueExplainArgs = parse_args(args)?;
        let cfg = app
            .sessions()
            .current(&ctx.window)
            .map(|s| s.config)
            .unwrap_or_default();
        let entry = os_analyze::issues::catalog()
            .iter()
            .find(|e| {
                e.id == args.issue_id
                    || e.doc_key == args.issue_id
                    || format!("{}/{}", e.tab, e.filter) == args.issue_id
            })
            .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-issue-not-found"))?;
        let registry = os_analyze::tabs::configured(&cfg);
        let params = registry
            .iter()
            .find(|t| t.id == entry.tab)
            .map(|t| (t.params)(&cfg))
            .unwrap_or_default();
        let lang = if args.lang == "de" {
            os_analyze::issues::Lang::De
        } else {
            os_analyze::issues::Lang::En
        };
        let text = os_analyze::issues::texts(&entry.doc_key, lang, &params)
            .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-issue-text-not-found"))?;
        to_json(&IssueText {
            what: text.what,
            why: text.why,
            fix: text.fix,
            rule: Some(format!("{} / {}", entry.type_rule, entry.priority_rule)),
            learn_more_url: Some(format!("docs/issues/{}.md", entry.doc_key)),
        })
    })
}
pub fn register(r: &mut Registry) {
    for (name, args, ret, h) in [
        (
            "sidebar.overview",
            "ScopeArgs",
            "OverviewDto",
            overview as crate::registry::Handler,
        ),
        ("sidebar.issues", "IssuesArgs", "IssueRow[]", issues),
        (
            "sidebar.issueExplain",
            "IssueExplainArgs",
            "IssueText",
            explain,
        ),
        ("sidebar.countStatus", "void", "CountStatus", count_status),
        ("sidebar.countCancel", "void", "void", count_cancel),
    ] {
        r.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running: false,
            },
            h,
        );
    }
}

//! Analysis operations use the writer queue and wait for integration completion.
use super::SessionService;
use crate::{
    dto::*,
    ops::ProgressSink,
    pool::CrawlKey,
    runtime::{
        error::{internal, to_json},
        sessions::SessionInfo,
    },
    App, CmdError,
};
use os_analyze::analysis::{persist, runner, AnalysisOutcome};
use os_model::{AnalysisStep, CancelToken, StepState};

fn step_state(s: StepState) -> StepStateDto {
    match s {
        StepState::Pending => StepStateDto::Pending,
        StepState::Done => StepStateDto::Done,
        StepState::Skipped => StepStateDto::Skipped,
        StepState::NotApplicable => StepStateDto::NotApplicable,
    }
}
pub async fn status(app: &App, s: &SessionInfo) -> Result<AnalysisStatus, CmdError> {
    if let Some(st) = app
        .ext::<SessionService>()
        .analysis
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&s.key)
        .filter(|s| s.running)
        .cloned()
    {
        return Ok(st);
    }
    let cfg = s.config.clone();
    app.read_pool()
        .run_cached(
            s.key.clone(),
            "session-meta",
            crate::runtime::current_cancel(),
            move |f, _| {
                let data = os_analyze::aggregates::status::analysis_status(f, &cfg);
                // UI-082/ONPAGE-140: stale and needsRecrawl come from the K03 status, not from the freshness alone.
                let steps = crate::commands::sidebar_ext::step_statuses(&data)
                    .map_err(|e| os_store::StoreError::InvalidInput(e.message_key.to_string()))?;
                let state = f.get_meta("analysis_state")?;
                Ok(AnalysisStatus {
                    steps,
                    running: state.as_deref() == Some("running"),
                    current: None,
                    percent: (state.as_deref() == Some("completed")).then_some(100),
                })
            },
        )
        .await
}
/// Is a crawl analysis running (registered by [`start_operation`] until its final status)? The session loop holds the
/// end status of a crawl for the automatic analysis that the crawl end started (CRAWL-141, UI-018, B4-LAGFIX3).
pub(crate) fn running(app: &App) -> bool {
    app.ext::<SessionService>()
        .analysis
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .values()
        .any(|s| s.running)
}

fn publish(app: &App, key: &CrawlKey, status: AnalysisStatus) {
    let svc = app.ext::<SessionService>();
    svc.analysis
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(key.clone(), status.clone());
    svc.parts.lock().unwrap_or_else(|p| p.into_inner()).analysis = Some(status);
}

pub fn start_operation(
    app: &App,
    s: SessionInfo,
    steps: Option<Vec<String>>,
    automatic: bool,
) -> Result<OpHandle, CmdError> {
    super::writable(&s)?;
    let only = steps
        .map(|steps| {
            steps
                .into_iter()
                .map(|s| {
                    s.parse::<AnalysisStep>().map_err(|_| {
                        CmdError::invalid_input("err-analysis-step", "steps").with_arg("step", s)
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let initial = AnalysisStatus {
        steps: Vec::new(),
        running: true,
        current: None,
        percent: Some(0),
    };
    {
        let svc = app.ext::<SessionService>();
        let mut map = svc.analysis.lock().unwrap_or_else(|p| p.into_inner());
        if map.get(&s.key).is_some_and(|s| s.running) {
            return Err(super::busy());
        }
        map.insert(s.key.clone(), initial.clone());
    }
    publish(app, &s.key, initial);
    let app = app.clone();
    let ops = app.ops().clone();
    Ok(ops.spawn("op-analysis", true, move |cancel, progress| {
        Box::pin(finish_operation(app, s, only, automatic, cancel, progress))
    }))
}

async fn finish_operation(
    app: App,
    s: SessionInfo,
    only: Option<Vec<AnalysisStep>>,
    automatic: bool,
    cancel: CancelToken,
    progress: ProgressSink,
) -> Result<serde_json::Value, CmdError> {
    let result = execute(&app, &s, only, automatic, cancel.clone(), progress).await;
    let final_status = match &result {
        Ok(outcome) => AnalysisStatus {
            steps: outcome
                .steps
                .iter()
                .map(|(s, st)| AnalysisStepStatus {
                    step: s.as_str().into(),
                    state: step_state(*st),
                    stale: false,
                })
                .collect(),
            running: false,
            current: None,
            percent: Some(if outcome.complete { 100 } else { 0 }),
        },
        Err(_) => AnalysisStatus {
            steps: Vec::new(),
            running: false,
            current: None,
            percent: None,
        },
    };
    publish(&app, &s.key, final_status.clone());
    let cancelled = cancel.is_cancelled();
    app.push_notice(Notice {
        level: if result.is_err() && !cancelled {
            NoticeLevel::Error
        } else {
            NoticeLevel::Info
        },
        code: if cancelled {
            "analysis-cancelled"
        } else if result.is_err() {
            "analysis-failed"
        } else {
            "analysis-completed"
        }
        .into(),
        args: result
            .as_ref()
            .err()
            .map(|error| [("detail".into(), serde_json::json!(error.message_key))].into())
            .unwrap_or_default(),
    });
    if automatic {
        crate::commands::mail::after_analysis(&app, &s.key);
    }
    super::feed::invalidate(&app, &s.key);
    if cancel.is_cancelled() {
        return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
    }
    result?;
    to_json(&final_status)
}

async fn execute(
    app: &App,
    s: &SessionInfo,
    only: Option<Vec<AnalysisStep>>,
    automatic: bool,
    cancel: CancelToken,
    progress: ProgressSink,
) -> Result<AnalysisOutcome, CmdError> {
    if automatic {
        tokio::select! {
            _ = app.crawl_events().wait_barriers(&s.key) => {},
            _ = wait_cancel(&cancel) => return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled")),
        }
    }
    let svc = app.ext::<SessionService>();
    let _guard = svc.mutation.lock().await;
    if cancel.is_cancelled() {
        return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
    }
    super::control::ensure_idle(app, s).await?;
    // This operation holds the lifecycle guard: queuing behind catalog readers
    // would also prevent the final crawl status from reaching the window.
    let cfg = app
        .read_pool()
        .run_cached(s.key.clone(), "session-meta", cancel.clone(), |f, _| {
            f.config()
        })
        .await?;
    let a = app.clone();
    let key = s.key.clone();
    let clock = app.config().clock.clone();
    let outcome = app
        .write_queue()
        .submit_file(&s.key, move |f| {
            let options = runner::RunOptions {
                clock: clock.as_ref(),
                only: only.as_deref(),
                ..Default::default()
            };
            let total = only.as_ref().map_or(AnalysisStep::ALL.len(), Vec::len) as u64;
            let outcome = runner::run_with(
                f,
                &cfg,
                &cancel,
                &|step, pct| {
                    let mut st = a
                        .ext::<SessionService>()
                        .analysis
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get(&key)
                        .cloned()
                        .expect("registered analysis");
                    st.current = Some(step.as_str().into());
                    st.percent = Some((pct * 100.0) as u8);
                    let entry = AnalysisStepStatus {
                        step: step.as_str().into(),
                        state: if pct < 1.0 {
                            StepStateDto::Running
                        } else {
                            StepStateDto::Done
                        },
                        stale: false,
                    };
                    if let Some(old) = st.steps.iter_mut().find(|s| s.step == entry.step) {
                        *old = entry;
                    } else {
                        st.steps.push(entry);
                    }
                    progress.report(
                        st.steps
                            .iter()
                            .filter(|s| s.state == StepStateDto::Done)
                            .count() as u64,
                        Some(total),
                    );
                    publish(&a, &key, st);
                },
                &options,
            )
            .map_err(|e| internal("analysis", e.to_string()))?;
            // Same incomplete-crawl rule as M1's run_manual, also for selected steps.
            if f.meta()
                .map_err(|e| crate::runtime::error::store_error(&e))?
                .state
                != os_model::CrawlState::Completed
            {
                persist::write_run_state(f, persist::RunState::Incomplete, None)
                    .map_err(|e| internal("analysis", e.to_string()))?;
            }
            Ok(outcome)
        })
        .await;
    // CRAWL-141 Ablaufpunkt 7: the final checkpoint waited at most 100 ms for readers. Frames it left (and later
    // writes such as the incomplete state) are merged in the background like after a pause (CRAWL-126). Still under
    // the lifecycle guard, so a resume afterwards cancels this merge before the engine writes again.
    super::merge::start(app, &s.key, s.path.clone());
    outcome
}
async fn wait_cancel(cancel: &CancelToken) {
    while !cancel.is_cancelled() {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

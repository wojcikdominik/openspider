//! Crawl command transport; lifecycle and work live in session/.
use crate::{
    dto::*,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::error::{internal, parse_args, to_json},
    session::{self, control, SessionService},
    App, CallCtx, CmdError,
};
use futures_util::future::BoxFuture;

fn validate<'a>(_: &'a App, _: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let a: ValidateStartUrlArgs = parse_args(args)?;
        let url = os_url::parse_start_url(&a.input).map_err(|e| {
            CmdError::invalid_input("err-start-url", "input").with_detail(e.to_string())
        })?;
        to_json(&NormalizedUrl {
            normalized: url.encoded,
        })
    })
}
fn parse_list<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let a: ListSource = parse_args(args)?;
        let cfg = control::config(app, &ctx, None).await?;
        let preview = tokio::task::spawn_blocking(move || {
            let items = match a {
                ListSource::Text { text } => text.lines().map(str::to_owned).collect(),
                ListSource::Path { path } => {
                    os_engine::list_input::read_items(std::path::Path::new(&path))
                        .map_err(control::engine_error)?
                }
            };
            let plan = os_engine::list_input::plan(&items, &cfg).map_err(control::engine_error)?;
            Ok::<_, CmdError>(ListPreview {
                valid: plan.counts.to_crawl,
                invalid: plan.counts.invalid + plan.counts.ignored_lines,
                duplicates: plan.counts.duplicates,
                sample: plan
                    .entries
                    .into_iter()
                    .filter(|e| e.outcome == os_engine::list_input::ListOutcome::Ok)
                    .filter_map(|e| e.url.map(|u| u.encoded))
                    .take(20)
                    .collect(),
            })
        })
        .await
        .map_err(|e| internal("parse list", e.to_string()))??;
        to_json(&preview)
    })
}
fn start<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move { to_json(&control::start(app, ctx, parse_args(args)?).await?) })
}
fn pause<'a>(app: &'a App, ctx: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        control::pause_or_stop(app, ctx, false).await?;
        Ok(serde_json::Value::Null)
    })
}
fn stop<'a>(app: &'a App, ctx: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        control::pause_or_stop(app, ctx, true).await?;
        Ok(serde_json::Value::Null)
    })
}
fn resume<'a>(app: &'a App, ctx: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        control::resume(app, ctx).await?;
        Ok(serde_json::Value::Null)
    })
}
fn clear<'a>(app: &'a App, ctx: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let svc = app.ext::<SessionService>();
        let _guard = svc.mutation.lock().await;
        control::replace_allowed(app, &ctx).await?;
        let key = app.sessions().current(&ctx.window).map(|s| s.key);
        session::close(app, &ctx.window, true);
        // UI-003/OUT-228: a cleared crawl stays in the crawl management and must be deletable there (and by the file
        // manager) right away. Closing its write connection is queued behind pending writes; wait for it here.
        if key.is_some_and(|k| !app.read_pool().is_registered(&k)) {
            app.write_queue().flush().await;
        }
        Ok(serde_json::Value::Null)
    })
}
fn open<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let arg: OpenCrawlArgs = parse_args(args)?;
        let request = serde_json::to_value(&arg).unwrap_or_default();
        tracing::info!(request = %request, "Crawl-Datei öffnen: begonnen");
        // Progress is transient; completed headless opens must not replay startup traffic.
        let show_progress = app.subscribed_windows().contains(&ctx.window);
        if show_progress {
            app.push_notice(Notice {
                level: NoticeLevel::Info,
                code: "crawl-open-started".into(),
                args: [("request".into(), request)].into(),
            });
        }
        let started = std::time::Instant::now();
        let result = control::open(app, ctx, arg).await;
        let (code, level, args) = match &result {
            Ok(summary) => {
                tracing::info!(crawl_id = %summary.crawl_id, elapsed_ms = started.elapsed().as_millis(), "Crawl-Datei öffnen: abgeschlossen");
                (
                    "crawl-open-completed",
                    NoticeLevel::Info,
                    std::collections::BTreeMap::new(),
                )
            }
            Err(error) => {
                tracing::error!(code = ?error.code, reason = %error.message_key, detail = ?error.detail, elapsed_ms = started.elapsed().as_millis(), "Crawl-Datei öffnen: fehlgeschlagen");
                (
                    "crawl-open-failed",
                    NoticeLevel::Error,
                    [
                        ("messageKey".into(), serde_json::json!(error.message_key)),
                        ("args".into(), serde_json::json!(error.args)),
                        ("detail".into(), serde_json::json!(error.detail)),
                        ("code".into(), serde_json::json!(error.code)),
                    ]
                    .into(),
                )
            }
        };
        if show_progress || result.is_err() {
            app.push_notice(Notice {
                level,
                code: code.into(),
                args,
            });
        }
        to_json(&result?)
    })
}
fn save_as<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let a: SaveAsArgs = parse_args(args)?;
        let s = session::current(app, &ctx.window)?;
        control::ensure_idle(app, &s).await?;
        let app = app.clone();
        let ops = app.ops().clone();
        to_json(&ops.spawn("op-save-crawl", true, move |cancel, _| {
            Box::pin(async move {
                let svc = app.ext::<SessionService>();
                let _guard = svc.mutation.lock().await;
                control::ensure_idle(&app, &s).await?;
                let dest = std::path::PathBuf::from(a.dest);
                let value = app
                    .read_pool()
                    .run_with(s.key, cancel, move |f, _| {
                        f.save_as(&dest)?;
                        Ok(FileResult {
                            path: dest.to_string_lossy().into_owned(),
                            bytes: std::fs::metadata(&dest)
                                .map_err(|e| os_store::StoreError::Io {
                                    path: dest.clone(),
                                    source: e,
                                })?
                                .len(),
                        })
                    })
                    .await?;
                to_json(&value)
            })
        }))
    })
}
fn recrawl<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    mutate(app, ctx, args, false)
}
fn remove<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    mutate(app, ctx, args, true)
}
fn mutate<'a>(
    app: &'a App,
    ctx: CallCtx,
    args: serde_json::Value,
    remove: bool,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let a: SelectionArgs = parse_args(args)?;
        let s = session::current(app, &ctx.window)?;
        control::ensure_idle(app, &s).await?;
        let app = app.clone();
        let ops = app.ops().clone();
        to_json(&ops.spawn(
            if remove { "op-remove" } else { "op-recrawl" },
            true,
            move |cancel, progress| {
                Box::pin(async move {
                    let svc = app.ext::<SessionService>();
                    let _guard = svc.mutation.lock().await;
                    control::ensure_idle(&app, &s).await?;
                    let mut ctx = ctx;
                    ctx.cancel = cancel.clone();
                    let ids = session::selection::resolve(&app, &ctx, a.selection).await?;
                    let mut count = 0;
                    let mut processed = 0;
                    progress.report(0, Some(ids.len() as u64));
                    // Bound work between cancellation checks. A recrawl drains its current batch
                    // safely through the engine; it never abandons a writing worker.
                    for batch in ids.chunks(100) {
                        if cancel.is_cancelled() {
                            return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
                        }
                        processed += batch.len() as u64;
                        let batch = batch.to_vec();
                        let existing = app
                            .read_pool()
                            .run(s.key.clone(), move |f, _| {
                                let mut found = Vec::new();
                                for id in batch {
                                    if os_store::queries::url_by_id(f.conn(), id)?.is_some() {
                                        found.push(id);
                                    }
                                }
                                Ok(found)
                            })
                            .await?;
                        let n = existing.len() as u64;
                        if remove {
                            app.write_queue()
                                .submit_file(&s.key, move |f| {
                                    os_engine::remove_urls(f.path(), &existing)
                                        .map_err(control::engine_error)
                                })
                                .await?;
                        } else {
                            os_engine::recrawl_urls(
                                control::fetcher(&app, &s.config)?,
                                &s.path,
                                &existing,
                                control::deps(&app),
                            )
                            .await
                            .map_err(control::engine_error)?;
                        }
                        count += n;
                        progress.report(processed, Some(ids.len() as u64));
                    }
                    session::feed::invalidate(&app, &s.key);
                    to_json(&CountResult { count })
                })
            },
        ))
    })
}

pub fn register(r: &mut Registry) {
    for (name, args, ret, long_running, handler) in [
        (
            "crawl.validateStartUrl",
            "ValidateStartUrlArgs",
            "NormalizedUrl",
            false,
            validate as crate::registry::Handler,
        ),
        (
            "crawl.parseList",
            "ListSource",
            "ListPreview",
            false,
            parse_list,
        ),
        ("crawl.start", "StartRequest", "CrawlRef", false, start),
        ("crawl.pause", "void", "void", false, pause),
        ("crawl.resume", "void", "void", false, resume),
        ("crawl.stop", "void", "void", false, stop),
        ("crawl.clear", "void", "void", false, clear),
        (
            "crawl.open",
            "OpenCrawlArgs",
            "CrawlSummaryDto",
            false,
            open,
        ),
        ("crawl.saveAs", "SaveAsArgs", "FileResult", true, save_as),
        (
            "crawl.recrawl",
            "SelectionArgs",
            "CountResult",
            true,
            recrawl,
        ),
        ("crawl.remove", "SelectionArgs", "CountResult", true, remove),
    ] {
        r.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running,
            },
            handler,
        );
    }
}

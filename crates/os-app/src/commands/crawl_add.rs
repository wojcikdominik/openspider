//! URLs nachreichen (CRAWL-131 [P1]), laufend über den Engine-Kanal, sonst über die Schreibwarteschlange.
use crate::dto::{AddUrlsArgs, AddUrlsResult, CmdError, ErrorCode};
use crate::registry::{CmdResult, CommandMeta, Registry};
use crate::runtime::error::{parse_args, to_json};
use crate::runtime::sessions::EngineControl;
use crate::runtime::{App, CallCtx};
use futures_util::future::BoxFuture;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct AdmissionLocks(
    Mutex<std::collections::HashMap<crate::pool::CrawlKey, Arc<tokio::sync::Mutex<()>>>>,
);

fn engine_error(error: os_engine::EngineError) -> CmdError {
    CmdError::new(ErrorCode::Engine, "err-engine").with_detail(error.to_string())
}

fn perform<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: AddUrlsArgs = parse_args(args)?;
        let selected = app
            .sessions()
            .current(&ctx.window)
            .ok_or_else(|| CmdError::new(ErrorCode::NoCrawl, "err-no-crawl"))?;
        let locks = app.ext::<AdmissionLocks>();
        let lock = locks
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(selected.key.clone())
            .or_default()
            .clone();
        let _admission = lock.lock().await;
        let mut session = app
            .sessions()
            .current(&ctx.window)
            .filter(|current| current.key == selected.key)
            .ok_or_else(|| CmdError::new(ErrorCode::NoCrawl, "err-no-crawl"))?;
        if !session.writable {
            return Err(CmdError::new(ErrorCode::Permission, "err-crawl-readonly"));
        }
        if ctx.cancel.is_cancelled() {
            return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
        }
        if let Some(engine) = &session.engine {
            if let Some(handle) = engine
                .handle
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .as_ref()
            {
                session.state = handle.progress().state;
            }
        }
        let request = session.engine.as_ref().and_then(|engine| {
            let guard = engine.handle.lock().unwrap_or_else(|p| p.into_inner());
            guard
                .as_ref()
                .filter(|handle| handle.progress().state == os_model::CrawlState::Running)
                .map(|handle| handle.request_add_urls(args.urls.clone()))
        });
        let result = if let Some(request) = request {
            request.await.map_err(engine_error)?
        } else {
            // 08 AI-020 [M4] (Wirkung b): Führt ein anderer Prozess den Crawl inzwischen als aktiver Besitzer (z. B.
            // KI-Lauf nach dem schreibenden Öffnen), wird wie in einer nur lesend geöffneten Sitzung abgewiesen.
            let (summary, _, _) = crate::session::feed::snapshot(app, session.key.clone()).await?;
            if !summary.owner.controllable {
                return Err(CmdError::new(ErrorCode::Permission, "err-crawl-readonly"));
            }
            let cfg = session.config.clone();
            let summary = app
                .write_queue()
                .submit_file(&session.key, move |file| {
                    os_engine::lifecycle::add_urls_to_file(file, &args.urls, &cfg)
                        .map_err(engine_error)
                })
                .await?;
            if summary.added > 0 && session.state == os_model::CrawlState::Completed {
                let fetch_cfg = os_fetch::FetchConfig::from_crawl_config(
                    &session.config,
                    &*app.config().secrets,
                    app.config().clock.clone(),
                )
                .map_err(|e| {
                    CmdError::new(ErrorCode::Engine, "err-fetch-config").with_detail(e.to_string())
                })?;
                let fetcher = os_fetch::HttpFetcher::new(fetch_cfg).map_err(|e| {
                    CmdError::new(ErrorCode::Engine, "err-fetch").with_detail(e.to_string())
                })?;
                let deps = os_engine::EngineDeps {
                    clock: app.config().clock.clone(),
                    ids: app.config().ids.clone(),
                    secrets: app.config().secrets.clone(),
                    profile: os_store::StorageProfile::Balanced,
                    sink_factory: None,
                    on_finished: None,
                    owner_kind: "app",
                    renderer: None,
                    // M4-D05: KI-Warteschlange (ohne Schalter „KI (Vorschau)“ keine).
                    ai_queue: super::ai_run::app_queue(app),
                };
                // Plan M4 E1 (E-2 b, M4-F02): ohne Schalter „KI (Vorschau)“ keine KI-Schritte.
                let run = super::ai_preview::restricted_override(
                    super::ai_preview::switch(app),
                    &session.config,
                );
                let handle =
                    os_engine::resume_crawl(Arc::new(fetcher), session.path.clone(), run, deps)
                        .await
                        .map_err(engine_error)?;
                let progress = handle.watch();
                session.engine = Some(Arc::new(EngineControl {
                    handle: Mutex::new(Some(handle)),
                }));
                session.state = os_model::CrawlState::Running;
                app.sessions()
                    .set_current(ctx.window.clone(), session.clone());
                app.crawl_events().emit(
                    app,
                    crate::runtime::lifecycle_bus::CrawlEvent::Resumed(session.key.clone()),
                );
                follow_completion(app, &ctx, &session, progress);
            }
            summary
        };
        to_json(&AddUrlsResult {
            added: result.added,
            skipped: result.duplicates + result.invalid + result.excluded,
        })
    })
}

fn follow_completion(
    app: &App,
    ctx: &CallCtx,
    session: &crate::runtime::sessions::SessionInfo,
    mut progress: tokio::sync::watch::Receiver<os_model::Progress>,
) {
    let app = app.clone();
    let window = ctx.window.clone();
    let key = session.key.clone();
    let engine = session.engine.clone();
    app.handle().spawn(async move {
        while progress.changed().await.is_ok() {
            let state = progress.borrow().state;
            if state == os_model::CrawlState::Running {
                continue;
            }
            if let Some(mut current) = app.sessions().current(&window) {
                if current.key != key
                    || !current
                        .engine
                        .as_ref()
                        .zip(engine.as_ref())
                        .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
                {
                    return;
                }
                current.state = state;
                app.sessions().set_current(window.clone(), current);
                let event = if state == os_model::CrawlState::Paused {
                    crate::runtime::lifecycle_bus::CrawlEvent::Paused(key.clone())
                } else {
                    crate::runtime::lifecycle_bus::CrawlEvent::Completed(key.clone())
                };
                app.crawl_events().emit(&app, event);
            }
            break;
        }
    });
}

fn add<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let _: AddUrlsArgs = parse_args(args.clone())?;
        let owned = app.clone();
        let op = app
            .ops()
            .spawn("op-crawl-add-urls", true, move |cancel, _| {
                Box::pin(async move { perform(&owned, CallCtx { cancel, ..ctx }, args).await })
            });
        to_json(&op)
    })
}

pub fn register(r: &mut Registry) {
    r.register(
        CommandMeta {
            name: "crawl.addUrls",
            args: "AddUrlsArgs",
            ret: "AddUrlsResult",
            long_running: true,
        },
        add,
    );
}

//! Engine commands share a mutation lock; pause waits for the durable checkpoint.
use super::{feed, SessionService};
use crate::{
    dto::*,
    pool::CrawlKey,
    runtime::{
        error::{internal, parse_args},
        lifecycle_bus::CrawlEvent,
        sessions::{EngineControl, SessionInfo},
    },
    App, CallCtx, CmdError,
};
use os_engine::{CrawlHandle, EngineDeps};
use os_model::{CrawlConfig, CrawlState};
use std::{path::PathBuf, sync::Arc};

pub fn engine_error(e: os_engine::EngineError) -> CmdError {
    match e {
        os_engine::EngineError::Store(e) => crate::runtime::error::store_error(&e),
        os_engine::EngineError::Cancelled => CmdError::new(ErrorCode::Cancelled, "err-cancelled"),
        os_engine::EngineError::NotImplemented(owner) => CmdError::unsupported("crawl", owner),
        other => CmdError::new(ErrorCode::InvalidInput, "err-crawl").with_detail(other.to_string()),
    }
}
pub fn deps(app: &App) -> EngineDeps {
    EngineDeps {
        clock: app.config().clock.clone(),
        ids: app.config().ids.clone(),
        profile: match app.settings_snapshot().storage.performance_profile {
            os_model::PerformanceProfile::Balanced => os_store::StorageProfile::Balanced,
            os_model::PerformanceProfile::MaxSpeed => os_store::StorageProfile::MaxSpeed,
            os_model::PerformanceProfile::MinMemory => os_store::StorageProfile::MinMemory,
        },
        secrets: crate::commands::api::crawl_context(app, CrawlConfig::default()).secrets,
        sink_factory: None,
        on_finished: None,
        owner_kind: "app",
        renderer: None,
    }
    .with_form_profile_dir(app.config().paths.settings.join("auth"))
}
pub fn fetcher(app: &App, cfg: &CrawlConfig) -> Result<Arc<os_fetch::HttpFetcher>, CmdError> {
    os_fetch::FetchConfig::from_crawl_config(
        cfg,
        app.config().secrets.as_ref(),
        app.config().clock.clone(),
    )
    .and_then(os_fetch::HttpFetcher::new)
    .map(Arc::new)
    .map_err(|e| {
        CmdError::new(ErrorCode::InvalidInput, "err-fetch-config").with_detail(e.to_string())
    })
}
pub async fn config(
    app: &App,
    ctx: &CallCtx,
    profile: Option<String>,
) -> Result<CrawlConfig, CmdError> {
    if let Some(name) = profile {
        let dto: CrawlConfigDto = parse_args(
            app.dispatch(
                ctx.clone(),
                "profile.load",
                serde_json::json!({"name":name}),
            )
            .await?,
        )?;
        return parse_args(dto.config);
    }
    if let Some(s) = app.sessions().current(&ctx.window) {
        return Ok((*s.config).clone());
    }
    // A01-04: `config.set` without an open crawl edits the draft of the window; it applies to the next crawl.
    if let Some(draft) = crate::commands::config::draft(app, &ctx.window) {
        return Ok((*draft).clone());
    }
    let path = app.config().paths.settings.clone();
    tokio::task::spawn_blocking(move || {
        CrawlConfig::load_default(&path)
            .map(|c| c.config)
            .map_err(|e| {
                CmdError::new(ErrorCode::InvalidInput, "err-profile").with_detail(e.to_string())
            })
    })
    .await
    .map_err(|e| internal("load config", e.to_string()))?
}
pub async fn ensure_idle(app: &App, s: &SessionInfo) -> Result<(), CmdError> {
    super::writable(s)?;
    if s.engine
        .as_ref()
        .is_some_and(|e| e.handle.lock().unwrap_or_else(|p| p.into_inner()).is_some())
    {
        return Err(super::busy());
    }
    let (_, _, state) = feed::snapshot(app, s.key.clone()).await?;
    if state == CrawlState::Running {
        return Err(super::busy());
    }
    Ok(())
}
pub async fn replace_allowed(app: &App, ctx: &CallCtx) -> Result<(), CmdError> {
    if app
        .ext::<SessionService>()
        .quitting
        .load(std::sync::atomic::Ordering::SeqCst)
        || app.ops().running() > 0
    {
        return Err(super::busy());
    }
    if let Some(s) = app.sessions().current(&ctx.window) {
        // Foreign sessions can be detached, but an owned engine must first stop.
        if s.engine
            .as_ref()
            .is_some_and(|e| e.handle.lock().unwrap_or_else(|p| p.into_inner()).is_some())
        {
            return Err(super::busy());
        }
    }
    Ok(())
}

pub async fn start(app: &App, ctx: CallCtx, req: StartRequest) -> Result<CrawlRef, CmdError> {
    let mut cfg = config(app, &ctx, req.profile.clone()).await?;
    cfg.scope.all_subdomains = req.scope == StartScope::Subdomains;
    cfg.scope.crawl_outside_start_folder = req.scope != StartScope::Folder;
    if let StartSource::List { .. } = &req.source {
        cfg = os_engine::list_input::list_config(cfg);
    }
    // CRAWL-122: app-wide proxy settings reach the crawl; an unreachable proxy fails here, before any session or file exists.
    let fetch = Arc::new(crate::commands::settings_network::crawl_fetcher(app, &cfg).await?);
    let result = start_with_fetcher(app, ctx, req, cfg, fetch.clone()).await?;
    app.ext::<CrawlNetworks>()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(CrawlKey(result.crawl_id.clone()), fetch.network_snapshot());
    Ok(result)
}

/// The same production lifecycle with an injected fetcher for deterministic fixture tests.
pub async fn start_with_fetcher<F: os_fetch::Fetch>(
    app: &App,
    ctx: CallCtx,
    req: StartRequest,
    cfg: CrawlConfig,
    fetch: Arc<F>,
) -> Result<CrawlRef, CmdError> {
    let service = app.ext::<SessionService>();
    let _guard = service.mutation.lock().await;
    replace_allowed(app, &ctx).await?;
    let dir = crate::runtime::info::effective_crawls_dir(&app.settings_snapshot(), app.config());
    let id = app.config().ids.new_crawl_id(app.config().clock.now());
    let file = os_store::registry::path_for(&dir, &id);
    let source = match req.source {
        StartSource::Spider { url } => os_engine::CrawlSource::Spider {
            start_url: os_url::parse_start_url(&url)
                .map_err(|e| {
                    CmdError::invalid_input("err-start-url", "url").with_detail(e.to_string())
                })?
                .encoded,
        },
        StartSource::List { items, label } => os_engine::CrawlSource::List { items, label },
        StartSource::Apis { items } => return start_apis(app, ctx, cfg, file, id, items).await,
    };
    let name = match &source {
        os_engine::CrawlSource::Spider { start_url } => start_url.clone(),
        os_engine::CrawlSource::List { label, .. } => label.clone(),
        os_engine::CrawlSource::Serp { label, .. } => label.clone(),
    };
    let mut cfg = cfg;
    cfg.custom_js.output_dir = Some(dir);
    let request = os_engine::CrawlRequest {
        config: cfg,
        source,
        file: file.clone(),
        project: String::new(),
        name,
    };
    let mut deps = deps(app);
    deps.ids = Arc::new(FixedId(id));
    let runtime = app.handle();
    let h = tokio::task::spawn_blocking(move || {
        runtime.block_on(os_engine::start_crawl(fetch, request, deps))
    })
    .await
    .map_err(|e| internal("start crawl", e.to_string()))?
    .map_err(engine_error)?;
    let key = CrawlKey(h.crawl_id().to_string());
    app.register_crawl(key.clone(), file.clone());
    let (_, cfg, _) = feed::snapshot(app, key.clone()).await?;
    super::close(app, &ctx.window, false);
    attach(
        app,
        ctx,
        key.clone(),
        file.clone(),
        cfg,
        h,
        CrawlEvent::Started(key.clone()),
    );
    remember_crawl(app, file.to_string_lossy().into_owned()).await;
    Ok(CrawlRef {
        crawl_id: key.0,
        file: file.to_string_lossy().into_owned(),
    })
}
struct FixedId(os_model::CrawlId);
impl os_model::IdSource for FixedId {
    fn new_crawl_id(&self, _: os_model::Timestamp) -> os_model::CrawlId {
        self.0
    }
}

fn attach(
    app: &App,
    ctx: CallCtx,
    key: CrawlKey,
    path: PathBuf,
    cfg: CrawlConfig,
    h: CrawlHandle,
    ev: CrawlEvent,
) {
    app.sessions().start_current(
        ctx.window,
        SessionInfo {
            key: key.clone(),
            path,
            config: Arc::new(cfg),
            state: CrawlState::Running,
            writable: true,
            engine: Some(Arc::new(EngineControl {
                handle: std::sync::Mutex::new(Some(h)),
            })),
        },
    );
    super::activate(app);
    feed::invalidate(app, &key);
    app.crawl_events().emit(app, ev);
}

pub async fn open(
    app: &App,
    ctx: CallCtx,
    arg: OpenCrawlArgs,
) -> Result<CrawlSummaryDto, CmdError> {
    let service = app.ext::<SessionService>();
    let _guard = service.mutation.lock().await;
    let path = match arg {
        OpenCrawlArgs::Path { path } => PathBuf::from(path),
        OpenCrawlArgs::Id { crawl_id } => {
            let id = crawl_id
                .parse()
                .map_err(|_| CmdError::invalid_input("err-crawl-id", "crawlId"))?;
            let dir =
                crate::runtime::info::effective_crawls_dir(&app.settings_snapshot(), app.config());
            tokio::task::spawn_blocking(move || os_store::registry::find(&dir, &id))
                .await
                .map_err(|e| internal("find crawl", e.to_string()))?
                .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-crawl-not-found"))?
        }
    };
    let path = tokio::task::spawn_blocking(move || {
        std::fs::canonicalize(&path).map_err(|e| {
            crate::runtime::error::store_error(&os_store::StoreError::Io { path, source: e })
        })
    })
    .await
    .map_err(|e| internal("open", e.to_string()))??;
    if let Some(s) = app
        .sessions()
        .current(&ctx.window)
        .filter(|s| s.path == path)
    {
        let (summary, cfg, state) = feed::snapshot(app, s.key.clone()).await?;
        if s.writable != summary.owner.controllable {
            let mut updated = s;
            updated.writable = summary.owner.controllable;
            updated.state = state;
            updated.config = Arc::new(cfg);
            if updated.writable {
                app.write_queue()
                    .register(updated.key.clone(), updated.path.clone());
            } else {
                app.write_queue().unregister(&updated.key);
            }
            app.sessions().set_current(ctx.window, updated);
        }
        remember_crawl(app, summary.file.clone()).await;
        return Ok(summary);
    }
    replace_allowed(app, &ctx).await?;
    let temporary = CrawlKey(format!("opening:{}", path.display()));
    let report = app
        .read_pool()
        .prepare_open(temporary.clone(), path)
        .await?;
    let result = feed::snapshot(app, temporary.clone()).await;
    app.read_pool().unregister(&temporary);
    let (summary, cfg, state) = result?;
    let key = CrawlKey(summary.crawl_id.clone());
    let existing = app.sessions().all().into_iter().find(|s| s.key == key);
    super::close(app, &ctx.window, false);
    if let Some(s) = existing {
        app.sessions().set_current(ctx.window, s);
    } else {
        if summary.owner.controllable {
            app.write_queue().register(key.clone(), report.path.clone());
            // A copy made with `VACUUM INTO` (duplicate, import, migration) is in rollback-journal mode. The
            // first writer switches it to WAL, which needs the file exclusively: once the readers of this
            // session hold read transactions (overview precomputation), a resume right after opening waited
            // 5 s and failed with "database is locked" (M2-N25, CI windows-x64). Switch before any reader.
            if let Err(error) = app.write_queue().submit_file(&key, |_| Ok(())).await {
                tracing::warn!(?error, "Crawl-Datei nicht im WAL-Modus geöffnet");
            }
        }
        app.read_pool().register(key.clone(), report.path.clone());
        app.sessions().set_current(
            ctx.window,
            SessionInfo {
                key: key.clone(),
                path: report.path,
                config: Arc::new(cfg),
                state,
                writable: summary.owner.controllable,
                engine: None,
            },
        );
    }
    super::activate(app);
    remember_crawl(app, summary.file.clone()).await;
    app.crawl_events().emit(app, CrawlEvent::Opened(key));
    Ok(summary)
}

type NetworkSnapshot = (os_model::settings::NetworkSettings, Option<String>);

#[derive(Default)]
struct CrawlNetworks(std::sync::Mutex<std::collections::HashMap<CrawlKey, NetworkSnapshot>>);

pub async fn resume(app: &App, ctx: CallCtx) -> Result<(), CmdError> {
    let service = app.ext::<SessionService>();
    let _guard = service.mutation.lock().await;
    let s = super::current(app, &ctx.window)?;
    ensure_idle(app, &s).await?;
    if app.ops().running() > 0 {
        return Err(super::busy());
    }
    // A background WAL merge must not compete with the resumed writer (CRAWL-126).
    super::merge::cancel(app, &s.key).await;
    let cfg = app
        .read_pool()
        .run_cached(
            s.key.clone(),
            "session-meta",
            crate::runtime::current_cancel(),
            |f, _| {
                if f.meta()?.end_reason == Some(os_model::EndReason::Stopped) {
                    return Err(os_store::StoreError::InvalidInput(
                        "stopped crawl cannot resume".into(),
                    ));
                }
                f.config()
            },
        )
        .await?;
    let runtime = app.handle();
    let saved = app
        .ext::<CrawlNetworks>()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&s.key)
        .cloned();
    let fetch = match saved {
        Some((network, password)) => {
            let mut config = os_fetch::FetchConfig::from_crawl_config(
                &cfg,
                app.config().secrets.as_ref(),
                app.config().clock.clone(),
            )
            .map_err(|e| internal("resume fetch config", e.to_string()))?
            .with_network(&network);
            config.proxy_password = password;
            Arc::new(
                os_fetch::HttpFetcher::new(config)
                    .map_err(|e| internal("resume fetcher", e.to_string()))?,
            )
        }
        None => fetcher(app, &cfg)?,
    };
    let path = s.path.clone();
    let deps = deps(app);
    let h = tokio::task::spawn_blocking(move || {
        runtime.block_on(os_engine::resume_crawl(fetch, path, None, deps))
    })
    .await
    .map_err(|e| internal("resume crawl", e.to_string()))?
    .map_err(engine_error)?;
    if *s.config != cfg {
        app.sessions().update_config(&s.key, Arc::new(cfg.clone()));
        app.crawl_events().emit(
            app,
            CrawlEvent::ConfigChanged {
                key: s.key.clone(),
                old: s.config.clone(),
                new: Arc::new(cfg.clone()),
            },
        );
    }
    attach(
        app,
        ctx,
        s.key.clone(),
        s.path,
        cfg,
        h,
        CrawlEvent::Resumed(s.key),
    );
    Ok(())
}

pub async fn pause_or_stop(app: &App, ctx: CallCtx, stop: bool) -> Result<(), CmdError> {
    let requested = super::current(app, &ctx.window)?;
    super::writable(&requested)?;
    // Signal the engine before waiting for a background snapshot to release the
    // session lock; otherwise it keeps fetching throughout that wait.
    let signalled = requested.engine.as_ref().is_some_and(|engine| {
        let handle = engine.handle.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(handle) = handle.as_ref() {
            if stop {
                handle.stop();
            } else {
                handle.pause();
            }
            true
        } else {
            false
        }
    });
    super::interrupt_poll(app);
    let service = app.ext::<SessionService>();
    let _guard = service.mutation.lock().await;
    let s = super::current(app, &ctx.window)?;
    if s.key != requested.key {
        return Err(super::busy());
    }
    // The poller may have joined the signalled engine before we got the lock.
    let joined = s.engine.as_ref().is_none_or(|engine| {
        engine
            .handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_none()
    });
    let acknowledged = joined
        && match s.state {
            CrawlState::Running => false,
            CrawlState::Paused | CrawlState::Incomplete => {
                !stop && (signalled || requested.state == CrawlState::Running)
            }
            // The engine can finish naturally before a click on a stale Running
            // tick reaches us. There is nothing left to pause or stop in that case.
            CrawlState::Completed | CrawlState::Failed => true,
        };
    if !acknowledged {
        settle(app, &s, Some(stop)).await?;
    }
    feed::invalidate(app, &s.key);
    // The durable result was published by settle/stop_checkpoint. Aggregates
    // and row notifications belong to the next background poll, not the ack.
    app.flush_tick();
    Ok(())
}

pub async fn settle(app: &App, s: &SessionInfo, action: Option<bool>) -> Result<(), CmdError> {
    let h = s
        .engine
        .as_ref()
        .and_then(|e| e.handle.lock().unwrap_or_else(|p| p.into_inner()).take());
    let Some(h) = h else {
        if action == Some(true) {
            return stop_checkpoint(app, s).await;
        }
        return if action.is_some() {
            Err(CmdError::new(
                ErrorCode::InvalidInput,
                "err-crawl-not-running",
            ))
        } else {
            Ok(())
        };
    };
    if let Some(stop) = action {
        if stop {
            h.stop();
        } else {
            h.pause();
        }
    }
    let watch = h.watch();
    let result = h.wait().await.map_err(engine_error);
    let mut p = watch.borrow().clone();
    let state = result.as_ref().map_or(CrawlState::Failed, |r| r.state);
    super::state(app, &s.key, state);
    p.state = state;
    if result.is_err() {
        app.push_notice(Notice {
            level: NoticeLevel::Error,
            code: "crawl-failed".into(),
            args: Default::default(),
        });
    }
    // The engine's periodic snapshot may predate the final batch. Publish the final
    // progress only after wait() has acknowledged the durable checkpoint.
    let text = serde_json::to_string(&p).map_err(|e| internal("progress", e.to_string()))?;
    app.write_queue()
        .submit(s.key.clone(), move |conn| {
            // The engine already checkpointed the crawl. A single metadata write
            // does not need a second CrawlFile open, profile setup and writer stamp.
            os_store::meta::set_meta(conn, "progress_snapshot", &text)
        })
        .await?;
    // CRAWL-126: the engine's checkpoint waited only briefly for readers; finish the merge in the background.
    super::merge::start(app, &s.key, s.path.clone());
    if result.is_err() {
        crate::commands::mail::on_failed(app, &s.key);
    }
    let summary = result?;
    match summary.state {
        CrawlState::Failed => {
            let cause = notify_start_unreachable(app, s).await;
            // OUT-281: Ein Lauf, der als „Fehlgeschlagen“ endet (CRAWL-128), ist für die Mail ein Fehlschlag. Der Versand gilt
            // je Lauf einmal; das folgende `Completed` sendet deshalb keine Erfolgsmail mehr.
            crate::commands::mail::on_failed_with(app, &s.key, cause);
        }
        // CRAWL-128: Die Sperre per robots.txt ist kein Fehlschlag; der Lauf bleibt „Abgeschlossen“, aber nicht still (UI-019).
        CrawlState::Completed => notify_start_robots(app, s).await,
        _ => {}
    }
    feed::settled(app, s, &p);
    app.flush_tick();
    let event = match summary.state {
        CrawlState::Paused => CrawlEvent::Paused(s.key.clone()),
        _ if action == Some(true) => CrawlEvent::Stopped(s.key.clone()),
        _ => CrawlEvent::Completed(s.key.clone()),
    };
    app.crawl_events().emit(app, event);
    if summary.state == CrawlState::Completed
        && s.config.analysis.auto_at_end
        && !app
            .ext::<SessionService>()
            .quitting
            .load(std::sync::atomic::Ordering::SeqCst)
    {
        super::analysis::start_operation(app, s.clone(), None, true)?;
    }
    Ok(())
}

/// UI-019: Ein Lauf endet nur dann regulär als „Fehlgeschlagen“, wenn die Start-URL nicht erreichbar war (CRAWL-128). Der Dialog
/// nennt die Fehlerklasse aus der Crawl-Datei (Grund der Start-URL-Zeile bzw. der robots.txt-Hilfszeile) statt eines stillen Endes.
/// Liefert die Ursache als Text für die Fehler-Mail (OUT-281).
async fn notify_start_unreachable(app: &App, s: &SessionInfo) -> String {
    let found = app
        .read_pool()
        .run(s.key.clone(), |f, _| {
            Ok(f.meta()?.start_url.and_then(|start| {
                os_engine::start_failure(f, &start).map(|failure| (start, failure))
            }))
        })
        .await;
    let (notice, cause) = match found {
        Ok(Some((start_url, failure))) => {
            // Fehlerklasse, betroffene Adresse und Ursache, gleichlautend mit Kommandozeile und Planer (OUT-281).
            let cause = failure.describe(&start_url);
            (start_unreachable_notice(&start_url, &failure), cause)
        }
        other => {
            if let Err(error) = other {
                tracing::warn!(
                    ?error,
                    "Ursache der nicht erreichbaren Start-URL nicht lesbar"
                );
            }
            // Nie still: ohne lesbare Ursache bleibt die allgemeine Fehlermeldung.
            let notice = Notice {
                level: NoticeLevel::Error,
                code: "crawl-failed".into(),
                args: Default::default(),
            };
            (notice, "Crawl fehlgeschlagen".to_owned())
        }
    };
    app.push_notice(notice);
    cause
}

/// Metadatum: Start-URL, für die die Meldung `start-robots` dieses Crawls schon erschien (UI-019, höchstens einmal je Crawl und
/// Start-URL; OS-Festlegung 09.11.2026).
const START_ROBOTS_NOTICE_META: &str = "notice.start_robots";

/// UI-019: Sperrt robots.txt (`Disallow`) die Start-URL, endet der Lauf als „Abgeschlossen“ (CRAWL-128), aber nicht als stilles
/// „0 URLs“: Die Meldung `start-robots` nennt die Sperre; der Dialog verweist auf die Einstellung, ändert sie aber nicht. Je Crawl
/// und Start-URL erscheint sie höchstens einmal, auch nicht erneut nach „URLs hinzufügen“ (Merker in der Crawl-Datei).
async fn notify_start_robots(app: &App, s: &SessionInfo) {
    let found = app
        .read_pool()
        .run(s.key.clone(), |f, _| {
            let Some(start) = f.meta()?.start_url else {
                return Ok(None);
            };
            if f.get_meta(START_ROBOTS_NOTICE_META)?.as_deref() == Some(start.as_str()) {
                return Ok(None);
            }
            Ok(os_engine::start_robots_block(f, &start).map(|block| (start, block)))
        })
        .await;
    match found {
        Ok(Some((start_url, block))) => {
            app.push_notice(start_robots_notice(&start_url, &block));
            let marker = start_url.clone();
            if let Err(error) = app
                .write_queue()
                .submit(s.key.clone(), move |conn| {
                    os_store::meta::set_meta(conn, START_ROBOTS_NOTICE_META, &marker)
                })
                .await
            {
                tracing::warn!(?error, "Merker der robots-Meldung nicht speicherbar");
            }
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(?error, "Sperre der Start-URL per robots.txt nicht lesbar"),
    }
}

/// Meldung zur per robots.txt gesperrten Start-URL (UI-019): Code `start-robots`, Stufe Warnung (kein Fehlschlag, CRAWL-128),
/// dazu Start-URL, robots.txt-URL (`url`), Zeilennummer (`lineNumber`) und Regeltext (`line`) der sperrenden Zeile, ohne
/// sprachabhängigen Präfix (die Oberfläche formuliert per i18n).
pub fn start_robots_notice(start_url: &str, block: &os_engine::StartRobotsBlock) -> Notice {
    Notice {
        level: NoticeLevel::Warning,
        code: "start-robots".into(),
        args: [
            ("startUrl".into(), serde_json::json!(start_url)),
            ("url".into(), serde_json::json!(block.robots_url)),
            ("lineNumber".into(), serde_json::json!(block.line_number)),
            ("line".into(), serde_json::json!(block.rule)),
        ]
        .into(),
    }
}

/// Meldung zur nicht erreichbaren Start-URL (UI-019): Code nach Fehlerklasse (`start-dns`, `start-connection`, `start-timeout`,
/// `start-tls`), dazu die Fehlerklasse selbst (`reason`), die gescheiterte URL und ob es die robots.txt war.
pub fn start_unreachable_notice(start_url: &str, failure: &os_engine::StartFailure) -> Notice {
    use os_model::NoResponseReason as R;
    let code = match failure.reason {
        R::DnsLookupFailed => "start-dns",
        R::ConnectionTimeout | R::ResponseTimeout => "start-timeout",
        R::TlsError => "start-tls",
        _ => "start-connection",
    };
    Notice {
        level: NoticeLevel::Error,
        code: code.into(),
        args: [
            ("startUrl".into(), serde_json::json!(start_url)),
            ("url".into(), serde_json::json!(failure.url)),
            ("reason".into(), serde_json::json!(failure.reason.as_str())),
            ("robots".into(), serde_json::json!(failure.robots)),
        ]
        .into(),
    }
}

async fn start_apis(
    app: &App,
    ctx: CallCtx,
    cfg: CrawlConfig,
    path: PathBuf,
    id: os_model::CrawlId,
    items: Vec<String>,
) -> Result<CrawlRef, CmdError> {
    let mut api_ctx = crate::commands::api::crawl_context(app, cfg.clone());
    api_ctx.quota_ledger = Some(app.config().paths.settings.join("api-quotas.json"));
    os_integrations::crawl_glue::validate_apis(&api_ctx).map_err(|e| {
        CmdError::invalid_input("err-api-disconnected", "source").with_detail(e.to_string())
    })?;
    let api_clock = app.config().clock.clone();
    let plan = os_engine::list_input::api_plan(&items, &cfg, fetcher(app, &cfg)?.as_ref())
        .await
        .map_err(engine_error)?;

    let key = CrawlKey(id.to_string());
    let init = os_store::CrawlInit {
        crawl_id: id,
        mode: os_model::CrawlMode::List,
        start_url: None,
        list_source: Some("APIs".into()),
        project: String::new(),
        name: "APIs".into(),
        config: cfg.clone(),
        created_at: app.config().clock.now(),
    };
    let rows: Vec<_> = plan
        .entries
        .iter()
        .map(|entry| os_store::queries::api::InputRow {
            position: entry.position,
            original: entry.original.clone(),
            outcome: entry.outcome.as_str().into(),
            url: entry.url.as_ref().map(|url| url.encoded.clone()),
        })
        .collect();
    let counts = plan.counts;
    let mut initial = os_model::WriteBatch::default();
    for entry in plan
        .entries
        .into_iter()
        .filter(|e| e.outcome == os_engine::list_input::ListOutcome::Ok)
    {
        if let Some(url) = entry.url {
            initial.new_urls.push(os_model::NewUrl {
                id: os_model::UrlId(initial.new_urls.len() as i64 + 1),
                folder_depth: os_url::folder_depth(&url.url),
                url_length: os_url::url_length(&url.display),
                url: url.encoded,
                address: url.display,
                origin: url.origin,
                is_internal: true,
                in_tables: true,
                discovered_via: os_model::DiscoveryKind::ListInput,
                crawl_depth: None,
                state: os_model::UrlState::Discovered,
                not_crawled_reason: Some(os_model::NotCrawledReason::CrawlDisabled),
            });
        }
    }
    for (key, value) in [
        ("list_read", counts.read),
        ("list_invalid", counts.invalid),
        ("list_ignored_lines", counts.ignored_lines),
        ("list_duplicates", counts.duplicates),
        ("list_excluded", counts.excluded),
        ("list_not_included", counts.not_included),
        ("list_sitemaps", counts.sitemaps),
        ("list_to_crawl", counts.to_crawl),
    ] {
        initial.counters.push((key.into(), value as i64));
    }
    initial.meta.push(("source_mode".into(), "apis".into()));
    app.write_queue()
        .create_file(key.clone(), path.clone(), init, initial)
        .await?;
    let runtime = app.handle();
    let stored_cfg = cfg.clone();
    let result = app
        .write_queue()
        .submit_file(&key, move |f| {
            os_store::queries::api::store_input_rows(f.conn(), &rows)
                .map_err(|e| crate::runtime::error::store_error(&e))?;
            f.set_meta(
                "api_input_urls",
                &serde_json::to_string(&items)
                    .map_err(|e| internal("API inputs", e.to_string()))?,
            )
            .map_err(|e| crate::runtime::error::store_error(&e))?;
            let result = runtime.block_on(os_integrations::crawl_glue::start_apis_mode_with(
                f, api_ctx, api_clock,
            ));
            f.set_meta(
                "state",
                if result.is_ok() {
                    "Completed"
                } else {
                    "Failed"
                },
            )
            .map_err(|e| crate::runtime::error::store_error(&e))?;
            result.map_err(|e| match e {
                os_integrations::IntegError::NotImplemented(owner) => {
                    CmdError::unsupported("crawl.start", owner)
                }
                other => internal("API crawl", other.to_string()),
            })
        })
        .await;
    if let Err(e) = result {
        app.write_queue().unregister(&key);
        return Err(e);
    }
    super::close(app, &ctx.window, false);
    app.read_pool().register(key.clone(), path.clone());
    app.sessions().set_current(
        ctx.window,
        SessionInfo {
            key: key.clone(),
            path: path.clone(),
            config: Arc::new(stored_cfg),
            state: CrawlState::Completed,
            writable: true,
            engine: None,
        },
    );
    super::activate(app);
    app.crawl_events()
        .emit(app, CrawlEvent::Started(key.clone()));
    app.crawl_events()
        .emit(app, CrawlEvent::Completed(key.clone()));
    remember_crawl(app, path.to_string_lossy().into_owned()).await;
    Ok(CrawlRef {
        crawl_id: key.0,
        file: path.to_string_lossy().into_owned(),
    })
}

/// A restored pause has no engine handle. Finalize its durable checkpoint directly.
async fn stop_checkpoint(app: &App, s: &SessionInfo) -> Result<(), CmdError> {
    let now = app.config().clock.now().to_db();
    app.write_queue()
        .submit_file(&s.key, move |f| {
            // One atomic commit instead of five Windows disk synchronizations.
            // CRAWL-126: take the writer lock before reading. The background WAL merge holds it while it waits for
            // readers; a deferred read upgraded to a write would fail at once with SQLITE_BUSY ("database is
            // locked") instead of waiting via the busy timeout.
            let tx = rusqlite::Transaction::new_unchecked(
                f.conn(),
                rusqlite::TransactionBehavior::Immediate,
            )
            .map_err(|e| crate::runtime::error::store_error(&e.into()))?;
            let state = f
                .meta()
                .map_err(|e| crate::runtime::error::store_error(&e))?
                .state;
            if !matches!(state, CrawlState::Paused | CrawlState::Incomplete) {
                return Err(CmdError::new(
                    ErrorCode::InvalidInput,
                    "err-crawl-not-running",
                ));
            }
            for (key, value) in [
                ("state", "Completed"),
                ("end_reason", "stopped"),
                ("ended_at", &now),
                ("heartbeat_at", &now),
                ("owner_pid", ""),
            ] {
                f.set_meta(key, value)
                    .map_err(|e| crate::runtime::error::store_error(&e))?;
            }
            tx.commit()
                .map_err(|e| crate::runtime::error::store_error(&e.into()))?;
            Ok(())
        })
        .await?;
    // CRAWL-126 (OS-Festlegung "Stoppen" während der Rückführung): the stop commit leaves frames in the WAL; a running
    // merge takes another round, otherwise a new one starts. Stop itself neither waits for nor cancels it.
    super::merge::start(app, &s.key, s.path.clone());
    super::state(app, &s.key, CrawlState::Completed);
    if !feed::stopped(app, s) {
        feed::invalidate(app, &s.key);
        feed::refresh(app, s).await?;
    }
    app.flush_tick();
    app.crawl_events()
        .emit(app, CrawlEvent::Stopped(s.key.clone()));
    if s.config.analysis.auto_at_end {
        super::analysis::start_operation(app, s.clone(), None, true)?;
    }
    Ok(())
}

/// Persist every newly created/opened crawl, including starts outside the header.
async fn remember_crawl(app: &App, path: String) {
    let saved = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        saved.uistate().update(|ui| {
            let Some(state) = ui.state.as_object_mut() else {
                return;
            };
            let old = state
                .get("recentCrawls")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut paths = vec![path];
            for entry in &old {
                if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                    if !path.is_empty() && !paths.iter().any(|p| p == path) {
                        paths.push(path.to_owned());
                    }
                }
            }
            state.insert(
                "recentCrawls".into(),
                serde_json::json!(paths
                    .into_iter()
                    .take(10)
                    .map(|path| old
                        .iter()
                        .find(|entry| entry.get("path").and_then(|value| value.as_str())
                            == Some(path.as_str()))
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({"path":path})))
                    .collect::<Vec<_>>()),
            );
        })
    })
    .await;
    let error = match result {
        Ok(Ok(_)) => return,
        Ok(Err(error)) => error,
        Err(error) => internal("recent crawls", error.to_string()),
    };
    app.push_notice(Notice {
        level: NoticeLevel::Warning,
        code: error.message_key.to_string(),
        args: error.args,
    });
}

#[cfg(test)]
mod control_priority_tests {
    use super::*;
    use std::time::Duration;

    // spec: UI-003
    // spec: CRAWL-126 [M2]
    #[tokio::test]
    async fn a_control_click_after_natural_completion_does_not_open_an_error_dialog() {
        let home = tempfile::tempdir().unwrap();
        let app = App::new(crate::AppConfig::new(home.path())).unwrap();
        let ctx = CallCtx::for_window("main");
        app.sessions().set_current(
            ctx.window.clone(),
            SessionInfo {
                key: CrawlKey("completed".into()),
                path: home.path().join("completed.ospider"),
                config: Arc::new(CrawlConfig::default()),
                state: CrawlState::Completed,
                writable: true,
                engine: None,
            },
        );
        pause_or_stop(&app, ctx.clone(), false).await.unwrap();
        pause_or_stop(&app, ctx.clone(), true).await.unwrap();
        assert_eq!(
            super::super::current(&app, &ctx.window).unwrap().state,
            CrawlState::Completed
        );
    }

    // spec: UI-003
    // spec: CRAWL-126 [M2]
    #[tokio::test]
    async fn pause_signals_the_engine_while_a_background_snapshot_holds_the_session_lock() {
        let home = tempfile::tempdir().unwrap();
        let app = App::new(crate::AppConfig::new(home.path())).unwrap();
        let mut cfg = CrawlConfig::default();
        cfg.analysis.auto_at_end = false;
        cfg.speed.threads = 1;
        let links = (0..20)
            .map(|i| format!("<a href='/p/{i}'>Next</a>"))
            .collect::<String>();
        let mut site = os_engine::testing::SimSite::new()
            .delay(Duration::from_millis(100))
            .page("https://fixture.test/", &links);
        for i in 0..20 {
            site = site.page(&format!("https://fixture.test/p/{i}"), "Done");
        }
        let ctx = CallCtx::for_window("main");
        let req = serde_json::from_value(serde_json::json!({
            "source": {"kind":"spider", "url":"https://fixture.test/"}, "scope":"domain"
        }))
        .unwrap();
        start_with_fetcher(
            &app,
            ctx.clone(),
            req,
            cfg,
            os_engine::testing::SimFetcher::new(site),
        )
        .await
        .unwrap();
        let service = app.ext::<SessionService>();
        let guard = service.mutation.lock().await;
        let session = super::super::current(&app, &ctx.window).unwrap();
        let engine = session.engine.clone().unwrap();
        let mut progress = engine.handle.lock().unwrap().as_ref().unwrap().watch();
        let worker = app.clone();
        let pause = tokio::spawn(async move { pause_or_stop(&worker, ctx, false).await });
        let stopped = tokio::time::timeout(Duration::from_secs(2), async {
            while progress.borrow_and_update().state == CrawlState::Running {
                progress.changed().await.unwrap();
            }
            progress.borrow().state
        })
        .await;
        // A terminal engine watch is not yet a durable acknowledgement.
        feed::invalidate(&app, &session.key);
        feed::refresh(&app, &session).await.unwrap();
        assert_eq!(
            app.sessions()
                .current(&crate::WindowId("main".into()))
                .unwrap()
                .state,
            CrawlState::Running
        );
        // Emulate the poller joining the signalled handle before the command
        // acquires the lock. The acknowledgement must remain successful.
        settle(&app, &session, None).await.unwrap();
        // Occupy the aggregate/feed lane after the durable checkpoint. A control
        // acknowledgement must not wait for that lane to become available.
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let reader = app.clone();
        let key = session.key.clone();
        let blocked = tokio::spawn(async move {
            reader
                .read_pool()
                .run_cached(
                    key,
                    "session-feed",
                    os_model::CancelToken::new(),
                    move |_, _| {
                        let _ = entered_tx.send(());
                        let _ = release_rx.recv_timeout(Duration::from_secs(5));
                        Ok(())
                    },
                )
                .await
        });
        entered_rx.await.unwrap();
        drop(guard);
        let ack = tokio::time::timeout(Duration::from_secs(2), pause).await;
        release_tx.send(()).unwrap();
        blocked.await.unwrap().unwrap();
        ack.expect("pause ack waited for aggregate reader")
            .unwrap()
            .unwrap();
        assert_eq!(stopped.unwrap(), CrawlState::Paused);
        // A joined checkpoint is available to the UI without another feed read.
        let tick = app.assemble_tick().unwrap();
        assert_eq!(tick.crawl.unwrap().phase, CrawlPhase::Paused);
    }
}

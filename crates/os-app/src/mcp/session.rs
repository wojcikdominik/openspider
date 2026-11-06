//! The HTTP server owns ordinary app engine sessions, with connection-time target protection.
use crate::{
    pool::CrawlKey,
    runtime::{
        lifecycle_bus::CrawlEvent,
        sessions::{EngineControl, SessionInfo},
        WeakApp,
    },
    session::{self, control, SessionService},
    App, CallCtx, WindowId,
};
use os_mcp::{
    permissions::DomainPolicy, target::ProtectedFetcher, SessionAccess, SessionCrawl, SessionError,
    SessionSource, SessionStart, SessionStatus,
};
use os_model::{CrawlConfig, CrawlState};
use std::sync::{Arc, Mutex};

pub struct Access(pub WeakApp);
impl std::fmt::Debug for Access {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppSessionAccess")
    }
}
fn err(e: impl std::fmt::Display) -> SessionError {
    SessionError::Internal(e.to_string())
}
/// App command errors keep their retryable `busy` class for MCP clients (AGENT-020).
fn cmd_err(e: crate::CmdError) -> SessionError {
    match e.code {
        crate::ErrorCode::Busy => SessionError::Busy(e.to_string()),
        crate::ErrorCode::InvalidInput => SessionError::Invalid(e.to_string()),
        _ => err(e),
    }
}
fn dto(s: SessionInfo) -> SessionCrawl {
    SessionCrawl {
        crawl_id: s.key.0,
        name: "App crawl".into(),
        project: String::new(),
        file: s.path,
        state: s.state,
        controllable: s.writable,
    }
}
impl Access {
    fn app(&self) -> Result<App, SessionError> {
        self.0.upgrade().ok_or_else(|| err("App closed"))
    }
    async fn launch(
        app: &App,
        source: Option<SessionSource>,
        config: CrawlConfig,
        policy: DomainPolicy,
        resume: Option<SessionInfo>,
    ) -> Result<SessionCrawl, SessionError> {
        let service = app.ext::<SessionService>();
        let _guard = service.mutation.lock().await;
        if service.quitting.load(std::sync::atomic::Ordering::SeqCst) || app.ops().running() > 0 {
            return Err(SessionError::Busy("App is busy or shutting down".into()));
        }
        // Client and TLS-root setup is deep synchronous work: run it on a fresh blocking stack
        // instead of on top of this caller's frames (AGENT-020).
        let fetcher = {
            let (app, config, policy) = (app.clone(), config.clone(), policy.clone());
            tokio::task::spawn_blocking(move || protected_fetcher(&app, &config, &policy))
                .await
                .map_err(err)??
        };
        let (handle, file) =
            heap(|| Self::engine(app, source, config, policy, resume, fetcher)).await?;
        heap(|| Self::register(app, handle, file)).await
    }

    /// Starts or resumes the engine on its own blocking thread.
    async fn engine(
        app: &App,
        source: Option<SessionSource>,
        config: CrawlConfig,
        policy: DomainPolicy,
        resume: Option<SessionInfo>,
        fetcher: Arc<ProtectedFetcher>,
    ) -> Result<(os_engine::CrawlHandle, std::path::PathBuf), SessionError> {
        let mut deps = control::deps(app);
        if config.rendering.mode != os_model::RenderingMode::Text {
            deps.renderer = Some(
                os_mcp::render_guard::launch(&config, policy.clone())
                    .await
                    .map_err(err)?,
            );
        }
        let runtime = app.handle();
        if let Some(s) = resume {
            control::ensure_idle(app, &s).await.map_err(cmd_err)?;
            session::merge::cancel(app, &s.key).await;
            let file = s.path;
            let path = file.clone();
            let h = tokio::task::spawn_blocking(move || {
                runtime.block_on(heap(|| os_engine::resume_crawl(fetcher, path, None, deps)))
            })
            .await
            .map_err(err)?
            .map_err(err)?;
            return Ok((h, file));
        }
        let id = app.config().ids.new_crawl_id(app.config().clock.now());
        struct Fixed(os_model::CrawlId);
        impl os_model::IdSource for Fixed {
            fn new_crawl_id(&self, _: os_model::Timestamp) -> os_model::CrawlId {
                self.0
            }
        }
        deps.ids = Arc::new(Fixed(id));
        let dir =
            crate::runtime::info::effective_crawls_dir(&app.settings_snapshot(), app.config());
        let file = os_store::registry::path_for(&dir, &id);
        let source = match source.ok_or_else(|| err("Missing source"))? {
            SessionSource::Spider { url } => {
                policy.check_url(&url).map_err(SessionError::Invalid)?;
                os_engine::CrawlSource::Spider { start_url: url }
            }
            SessionSource::List { items, label } => {
                for url in &items {
                    policy.check_url(url).map_err(SessionError::Invalid)?;
                }
                os_engine::CrawlSource::List { items, label }
            }
        };
        let mut config = config;
        config.custom_js.output_dir = Some(dir);
        let request = os_engine::CrawlRequest {
            config,
            source,
            file: file.clone(),
            project: String::new(),
            name: "MCP".into(),
        };
        let h = tokio::task::spawn_blocking(move || {
            runtime.block_on(heap(|| os_engine::start_crawl(fetcher, request, deps)))
        })
        .await
        .map_err(err)?
        .map_err(err)?;
        Ok((h, file))
    }

    /// Publishes a started engine as the app session.
    async fn register(
        app: &App,
        handle: os_engine::CrawlHandle,
        file: std::path::PathBuf,
    ) -> Result<SessionCrawl, SessionError> {
        let key = CrawlKey(handle.crawl_id().to_string());
        app.register_crawl(key.clone(), file.clone());
        let (_, cfg, _) = session::feed::snapshot(app, key.clone())
            .await
            .map_err(err)?;
        let s = SessionInfo {
            key: key.clone(),
            path: file,
            config: Arc::new(cfg),
            state: CrawlState::Running,
            writable: true,
            engine: Some(Arc::new(EngineControl {
                handle: Mutex::new(Some(handle)),
            })),
        };
        app.sessions()
            .set_current(WindowId(format!("mcp:{}", key.0)), s.clone());
        let main = WindowId("main".into());
        if app
            .sessions()
            .current(&main)
            .is_none_or(|old| old.state != CrawlState::Running)
        {
            app.sessions().set_current(main, s.clone());
        }
        session::activate(app);
        session::feed::invalidate(app, &key);
        app.crawl_events().emit(app, CrawlEvent::Started(key));
        Ok(dto(s))
    }
}

/// Builds a future in this short-lived frame and moves it to the heap (AGENT-020).
///
/// In unoptimised builds `Box::pin(f(..))` still materialises the whole future in the caller's
/// frame, which stays on the stack while the nested futures run.
#[inline(never)]
fn heap<F: std::future::Future>(make: impl FnOnce() -> F) -> std::pin::Pin<Box<F>> {
    Box::pin(make())
}

/// The MCP fetcher with the connection-time target policy.
fn protected_fetcher(
    app: &App,
    config: &CrawlConfig,
    policy: &DomainPolicy,
) -> Result<Arc<ProtectedFetcher>, SessionError> {
    let network = app.settings_snapshot().network;
    let mut fetch_config = os_fetch::FetchConfig::from_crawl_config(
        config,
        app.config().secrets.as_ref(),
        app.config().clock.clone(),
    )
    .map_err(err)?
    .with_network(&network);
    fetch_config.proxy_password = app
        .config()
        .secrets
        .try_get(os_fetch::net::PROXY_PASSWORD)
        .map_err(err)?;
    Ok(Arc::new(
        ProtectedFetcher::new(
            fetch_config,
            policy.clone(),
            policy.allows_private_targets(),
        )
        .map_err(err)?,
    ))
}
impl SessionAccess for Access {
    fn current_config(&self) -> Option<CrawlConfig> {
        let app = self.app().ok()?;
        app.handle()
            .block_on(control::config(&app, &CallCtx::for_window("main"), None))
            .ok()
    }
    fn crawls(&self) -> Vec<SessionCrawl> {
        let Ok(app) = self.app() else { return vec![] };
        let mut seen = std::collections::HashSet::new();
        app.sessions()
            .all()
            .into_iter()
            .filter(|s| seen.insert(s.key.clone()))
            .map(dto)
            .collect()
    }
    fn status(&self, id: &str) -> Option<SessionStatus> {
        let app = self.app().ok()?;
        let s = app.sessions().all().into_iter().find(|s| s.key.0 == id)?;
        let progress = s.engine.and_then(|e| {
            e.handle
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .as_ref()
                .map(|h| h.progress())
        });
        Some(SessionStatus {
            // The terminal watch can precede the durable writer acknowledgement.
            state: if progress.is_some() {
                CrawlState::Running
            } else {
                s.state
            },
            progress,
        })
    }
    fn start_crawl(&self, start: SessionStart) -> Result<SessionCrawl, SessionError> {
        let app = self.app()?;
        let cfg = app
            .handle()
            .block_on(control::config(
                &app,
                &CallCtx::for_window("main"),
                start.profile.clone(),
            ))
            .map_err(err)?;
        self.start_configured(start, cfg, DomainPolicy::default())
    }
    fn start_configured(
        &self,
        start: SessionStart,
        config: CrawlConfig,
        policy: DomainPolicy,
    ) -> Result<SessionCrawl, SessionError> {
        let app = self.app()?;
        app.handle().block_on(heap(|| {
            Self::launch(&app, Some(start.source), config, policy, None)
        }))
    }
    fn stop_crawl(&self, id: &str) -> Result<(), SessionError> {
        let app = self.app()?;
        let s = app
            .sessions()
            .all()
            .into_iter()
            .find(|s| s.key.0 == id)
            .ok_or_else(|| SessionError::NotFound(id.into()))?;
        if !s.writable {
            return Err(SessionError::NotControllable(id.into()));
        }
        app.handle().block_on(async {
            let service = app.ext::<SessionService>();
            let _guard = service.mutation.lock().await;
            control::settle(&app, &s, Some(true)).await.map_err(cmd_err)
        })
    }
    fn pause_crawl(&self, id: &str) -> Result<(), SessionError> {
        let app = self.app()?;
        let s = app
            .sessions()
            .all()
            .into_iter()
            .find(|s| s.key.0 == id)
            .ok_or_else(|| SessionError::NotFound(id.into()))?;
        if !s.writable {
            return Err(SessionError::NotControllable(id.into()));
        }
        // Same path as the header's pause (CRAWL-126): returns once the paused state is durable.
        app.handle().block_on(async {
            let service = app.ext::<SessionService>();
            let _guard = service.mutation.lock().await;
            control::settle(&app, &s, Some(false))
                .await
                .map_err(cmd_err)
        })
    }
    fn resume_crawl(&self, id: &str, policy: DomainPolicy) -> Result<SessionCrawl, SessionError> {
        let app = self.app()?;
        let s = app
            .sessions()
            .all()
            .into_iter()
            .find(|s| s.key.0 == id)
            .ok_or_else(|| SessionError::NotFound(id.into()))?;
        // CRAWL-128, AGENT-020: a stopped crawl is final; same notice as the CLI and the engine.
        let stopped = app
            .handle()
            .block_on(app.read_pool().run(s.key.clone(), |f, _| f.meta()))
            .map_err(cmd_err)?
            .end_reason
            == Some(os_model::EndReason::Stopped);
        if stopped {
            return Err(SessionError::Invalid(format!(
                "Crawl {id} cannot be resumed: {} (stopped crawls are final; start a new crawl)",
                os_engine::lifecycle::STOPPED_IS_FINAL
            )));
        }
        let config = (*s.config).clone();
        app.handle()
            .block_on(heap(|| Self::launch(&app, None, config, policy, Some(s))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AGENT-020 [M2]
    #[tokio::test]
    async fn mcp_launch_future_has_bounded_size() {
        let home = tempfile::tempdir().unwrap();
        let app = App::new(crate::AppConfig::new(home.path())).unwrap();
        for source in [
            Some(SessionSource::Spider {
                url: "http://127.0.0.1/".into(),
            }),
            Some(SessionSource::List {
                items: vec![],
                label: "size".into(),
            }),
            None,
        ] {
            let future = Access::launch(
                &app,
                source,
                CrawlConfig::default(),
                DomainPolicy::default(),
                None,
            );
            let bytes = std::mem::size_of_val(&future);
            eprintln!("MCP launch future: {bytes} bytes");
            assert!(
                bytes <= 16 * 1024,
                "MCP launch future: {bytes} bytes exceeds 16 KiB"
            );
        }
    }
}

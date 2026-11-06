//! Headless crawl handles; the existing engine owns lifecycle, persistence and heartbeat.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use os_analyze::analysis::{run_after_crawl, CancelToken};
use os_engine::{CrawlHandle, CrawlRequest, CrawlSource, EngineDeps};
use os_fetch::FetchConfig;
use os_model::{Clock, CrawlConfig, CrawlId, IdSource, RandomIds, SystemClock};
use os_store::{registry, StorageProfile};
use serde_json::{json, Value};

use crate::target::ProtectedFetcher;
use crate::{ToolContext, ToolError};

#[derive(Default)]
pub(crate) struct CrawlService {
    handles: Mutex<HashMap<String, CrawlHandle>>,
    finished: Mutex<HashSet<String>>,
    /// Angeforderte, noch nicht abgeschlossene Pause bzw. Stopp je Crawl (`busy`, AGENT-020).
    settling: Mutex<HashMap<String, &'static str>>,
}

/// Was ein Steuerbefehl über einen Crawl dieses Prozesses wissen muss.
enum Live {
    /// Kein Engine-Handle dieses Prozesses (oder ein ganz beendeter Lauf).
    None,
    /// Die Engine arbeitet; `Some` = Pause/Stopp schon angefordert.
    Running(Option<&'static str>),
    /// Endzustand gemeldet und dauerhaft, der Lauf schließt die Datei aber noch ab (Checkpoint, Writer, Hook).
    Closing(os_model::CrawlState),
}

fn busy_text(id: &str, what: &str) -> ToolError {
    ToolError::Busy(format!(
        "Crawl {id} is {what}; retry when crawl_status no longer reports Running"
    ))
}
impl std::fmt::Debug for CrawlService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CrawlService")
    }
}

fn invalid(e: impl std::fmt::Display) -> ToolError {
    ToolError::InvalidArguments(e.to_string())
}

impl CrawlService {
    fn renderer(
        cfg: &CrawlConfig,
        ctx: &ToolContext,
    ) -> Result<Option<Arc<dyn os_engine::Renderer>>, ToolError> {
        if cfg.rendering.mode == os_model::RenderingMode::Text {
            return Ok(None);
        }
        tokio::runtime::Handle::current()
            .block_on(crate::render_guard::launch(cfg, ctx.target_policy.clone()))
            .map(Some)
            .map_err(invalid)
    }

    fn deps(&self, id: Option<CrawlId>) -> EngineDeps {
        struct Fixed(CrawlId);
        impl IdSource for Fixed {
            fn new_crawl_id(&self, _: os_model::Timestamp) -> CrawlId {
                self.0
            }
        }
        EngineDeps {
            clock: Arc::new(SystemClock),
            ids: id.map_or_else(
                || Arc::new(RandomIds) as Arc<dyn IdSource>,
                |id| Arc::new(Fixed(id)),
            ),
            profile: StorageProfile::Balanced,
            secrets: os_fetch::secrets::secret_store_from_env(),
            sink_factory: None,
            owner_kind: "mcp-stdio",
            renderer: None,
            on_finished: Some(Arc::new(|summary| {
                if let Err(e) = run_after_crawl(&summary.file, &CancelToken::new()) {
                    tracing::error!(error=%e,"crawl analysis failed");
                }
            })),
        }
    }

    pub fn start(
        &self,
        ctx: &ToolContext,
        source: CrawlSource,
        cfg: CrawlConfig,
    ) -> Result<Value, ToolError> {
        let id = RandomIds.new_crawl_id(SystemClock.now());
        std::fs::create_dir_all(&ctx.crawl_dir).map_err(invalid)?;
        let mut deps = self.deps(Some(id));
        deps.renderer = Self::renderer(&cfg, ctx)?;
        let fetch_cfg = FetchConfig::from_crawl_config(&cfg, &*deps.secrets, deps.clock.clone())
            .map_err(invalid)?;
        let fetcher = ProtectedFetcher::new(
            fetch_cfg,
            ctx.target_policy.clone(),
            ctx.target_policy.allows_private_targets(),
        )
        .map_err(invalid)?;
        let req = CrawlRequest {
            config: cfg,
            source,
            file: registry::path_for(&ctx.crawl_dir, &id),
            project: String::new(),
            name: format!("MCP {id}"),
        };
        let handle = tokio::runtime::Handle::current()
            .block_on(os_engine::start_crawl(Arc::new(fetcher), req, deps))
            .map_err(invalid)?;
        let mut handles = self.handles.lock().map_err(invalid)?;
        let mut finished = self.finished.lock().map_err(invalid)?;
        handles.retain(|id, h| {
            let running = h.progress().state == os_model::CrawlState::Running;
            if !running {
                finished.insert(id.clone());
            }
            running
        });
        handles.insert(id.to_string(), handle);
        Ok(json!({"crawl_id":id.to_string(),"state":"Running"}))
    }

    pub fn status(&self, id: &str) -> Option<Value> {
        let handles = self.handles.lock().ok()?;
        handles
            .get(id)
            .map(|h| json!({"crawl_id":id,"progress":h.progress()}))
    }

    /// Zustand des Engine-Handles dieses Prozesses; räumt erledigte Anforderungen auf.
    fn live(&self, id: &str) -> Result<Live, ToolError> {
        let handles = self.handles.lock().map_err(invalid)?;
        let mut settling = self.settling.lock().map_err(invalid)?;
        let Some(handle) = handles.get(id) else {
            settling.remove(id);
            return Ok(Live::None);
        };
        if handle.progress().state == os_model::CrawlState::Running {
            return Ok(Live::Running(settling.get(id).copied()));
        }
        settling.remove(id);
        Ok(if handle.is_finished() {
            Live::None
        } else {
            Live::Closing(handle.progress().state)
        })
    }

    /// Sendet Pause bzw. Stopp an die laufende Engine; eine zweite Anforderung während des Anhaltens ist `busy`.
    fn signal(&self, id: &str, what: &'static str) -> Result<bool, ToolError> {
        let handles = self.handles.lock().map_err(invalid)?;
        let mut settling = self.settling.lock().map_err(invalid)?;
        let Some(handle) = handles
            .get(id)
            .filter(|h| h.progress().state == os_model::CrawlState::Running)
        else {
            return Ok(false);
        };
        if let Some(pending) = settling.get(id) {
            return Err(busy_text(id, pending));
        }
        if what == "stopping" {
            handle.stop();
        } else {
            handle.pause();
        }
        settling.insert(id.to_owned(), what);
        Ok(true)
    }

    /// Fehler für einen Crawl ohne laufende Engine dieses Prozesses, nach dem Zustand seiner Datei.
    fn not_live(ctx: &ToolContext, id: &str, verb: &str) -> Result<ToolError, ToolError> {
        let crawl = super::tools::common::open_crawl(ctx, &json!({"crawl_id":id}))?;
        let state = crawl.file.effective_state(&SystemClock).map_err(invalid)?;
        Ok(match state {
            os_model::CrawlState::Running => {
                let kind = crawl
                    .file
                    .get_meta("owner_kind")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let pid = crawl
                    .file
                    .get_meta("owner_pid")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                ToolError::InvalidArguments(format!(
                    "Crawl {id} runs in another process ({kind}, PID {pid}); it can only be controlled there"
                ))
            }
            os_model::CrawlState::Paused => {
                ToolError::InvalidArguments(format!("Crawl {id} is already paused; cannot {verb}"))
            }
            other => ToolError::InvalidArguments(format!(
                "Crawl {id} already finished ({})",
                other.as_str()
            )),
        })
    }

    /// `pause_crawl` (AGENT-020, CRAWL-126): hält einen Crawl dieses Prozesses fortsetzbar an.
    pub fn pause(&self, ctx: &ToolContext, id: &str) -> Result<(), ToolError> {
        match self.live(id)? {
            Live::Running(Some(pending)) => return Err(busy_text(id, pending)),
            Live::Running(None) => {
                if self.signal(id, "pausing")? {
                    return Ok(());
                }
            }
            Live::Closing(_) | Live::None => {}
        }
        Err(Self::not_live(ctx, id, "pause")?)
    }

    /// `stop_crawl` (AGENT-020, UI-003): beendet einen laufenden Crawl dieses Prozesses oder einen pausierten bzw.
    /// unterbrochenen Crawl ohne Engine endgültig (Endgrund `stopped`).
    pub fn stop(&self, ctx: &ToolContext, id: &str) -> Result<Value, ToolError> {
        match self.live(id)? {
            Live::Running(Some(pending)) => return Err(busy_text(id, pending)),
            Live::Running(None) => {
                if self.signal(id, "stopping")? {
                    return Ok(json!({"crawl_id":id,"stopping":true}));
                }
            }
            Live::Closing(os_model::CrawlState::Paused) => {
                return Err(busy_text(id, "still saving its paused state"))
            }
            Live::Closing(_) | Live::None => {}
        }
        let crawl = super::tools::common::open_crawl(ctx, &json!({"crawl_id":id}))?;
        let state = crawl.file.effective_state(&SystemClock).map_err(invalid)?;
        if !matches!(
            state,
            os_model::CrawlState::Paused | os_model::CrawlState::Incomplete
        ) {
            return Err(Self::not_live(ctx, id, "stop")?);
        }
        let path = crawl.file.path().to_path_buf();
        drop(crawl);
        // Die Crawl-Analyse startet wie nach einem regulären Ende (UI-003, CRAWL-141), ohne die Antwort aufzuhalten.
        let mut deps = self.deps(None);
        deps.on_finished = Some(Arc::new(|summary| {
            let file = summary.file.clone();
            std::thread::spawn(move || {
                if let Err(e) = run_after_crawl(&file, &CancelToken::new()) {
                    tracing::error!(error=%e,"crawl analysis failed");
                }
            });
        }));
        os_engine::lifecycle::stop_unowned(&path, &deps).map_err(invalid)?;
        self.handles.lock().map_err(invalid)?.remove(id);
        self.finished.lock().map_err(invalid)?.insert(id.to_owned());
        Ok(json!({"crawl_id":id,"state":"Completed","end_reason":"stopped"}))
    }

    pub fn resume(&self, ctx: &ToolContext, id: &str) -> Result<Value, ToolError> {
        match self.live(id)? {
            Live::Running(Some(pending)) => return Err(busy_text(id, pending)),
            Live::Running(None) => {
                return Err(ToolError::InvalidArguments(format!(
                    "Crawl {id} is running; only a paused, interrupted or limit-ended crawl can be resumed"
                )))
            }
            Live::Closing(_) => return Err(busy_text(id, "still saving its final state")),
            Live::None => {}
        }
        let crawl = super::tools::common::open_crawl(ctx, &json!({"crawl_id":id}))?;
        // CRAWL-128: „Gestoppt ist endgültig“ – derselbe Hinweis wie in CLI und Engine, vor jedem Abruferaufbau.
        if crawl.meta.end_reason == Some(os_model::EndReason::Stopped) {
            return Err(ToolError::InvalidArguments(format!(
                "Crawl {id} cannot be resumed: {} (stopped crawls are final; start a new crawl)",
                os_engine::lifecycle::STOPPED_IS_FINAL
            )));
        }
        let mut deps = self.deps(None);
        deps.renderer = Self::renderer(&crawl.cfg, ctx)?;
        let fetch_cfg =
            FetchConfig::from_crawl_config(&crawl.cfg, &*deps.secrets, deps.clock.clone())
                .map_err(invalid)?;
        let fetcher = ProtectedFetcher::new(
            fetch_cfg,
            ctx.target_policy.clone(),
            ctx.target_policy.allows_private_targets(),
        )
        .map_err(invalid)?;
        let path = crawl.file.path().to_path_buf();
        drop(crawl);
        let handle = tokio::runtime::Handle::current()
            .block_on(os_engine::resume_crawl(Arc::new(fetcher), path, None, deps))
            .map_err(invalid)?;
        self.finished.lock().map_err(invalid)?.remove(id);
        self.handles
            .lock()
            .map_err(invalid)?
            .insert(id.to_owned(), handle);
        Ok(json!({"crawl_id":id,"state":"Running"}))
    }
}

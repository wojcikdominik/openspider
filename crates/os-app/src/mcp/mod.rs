//! In-process MCP service. Weak app references prevent listener/App ownership cycles.
pub mod clients;
pub mod session;
use crate::{
    dto::*,
    events::{TickParts, TickSource},
    App, CmdError,
};
use os_mcp::{
    http::{HttpServer, TokenStore},
    permissions::Permissions,
    server::McpServer,
    settings::{self, McpSettings},
    ToolContext,
};
use std::sync::Mutex;

#[derive(Default)]
pub struct Service {
    pub state: tokio::sync::Mutex<State>,
    status: Mutex<Option<McpStatus>>,
}
#[derive(Default)]
pub struct State {
    pub http: Option<HttpServer>,
    pub server: Option<McpServer>,
    pub token: Option<TokenStore>,
}
impl TickSource for Service {
    fn collect(&self, parts: &mut TickParts) {
        if let Some(status) = self.status.lock().unwrap_or_else(|p| p.into_inner()).take() {
            parts.mcp = Some(status);
        }
    }
}
pub fn error(e: impl std::fmt::Display) -> CmdError {
    CmdError::new(ErrorCode::InvalidInput, "err-mcp").with_detail(e.to_string())
}
pub fn load(app: &App) -> Result<McpSettings, CmdError> {
    settings::load(&settings::path_in(&app.config().paths)).map_err(error)
}
pub fn server(app: &App, settings: McpSettings) -> Result<McpServer, CmdError> {
    let permissions = Permissions::resolve(&[], false, &settings).map_err(error)?;
    let mut ctx = ToolContext::new(crate::runtime::info::effective_crawls_dir(
        &app.settings_snapshot(),
        app.config(),
    ));
    if let Ok(exe) = std::env::current_exe() {
        ctx.sql_worker = Some((
            exe.with_file_name(if cfg!(windows) {
                "openspider.exe"
            } else {
                "openspider"
            }),
            vec!["mcp".into(), "--sql-worker".into()],
        ));
    }
    ctx.clock = app.config().clock.clone();
    ctx.settings_dir = app.config().paths.settings.clone();
    ctx.api_environment = Some(os_mcp::tools::link_metrics::ApiEnvironment {
        secrets: app.config().secrets.clone(),
        network: app.settings_snapshot().network,
        allow_api_override: false,
    });
    ctx.settings = settings;
    ctx.session = Some(std::sync::Arc::new(session::Access(app.downgrade())));
    Ok(McpServer::new(os_mcp::tools::all(), permissions, ctx))
}
pub fn token(app: &App, state: &mut State) -> Result<TokenStore, CmdError> {
    if let Some(token) = &state.token {
        return Ok(token.clone());
    }
    let token = TokenStore::open(app.config().secrets.clone()).map_err(error)?;
    state.token = Some(token.clone());
    Ok(token)
}
pub async fn status(state: &State) -> McpStatus {
    match &state.http {
        None => McpStatus::Off,
        Some(http) => match http.clients().await {
            0 => McpStatus::Ready { port: http.port() },
            clients => McpStatus::Connected {
                port: http.port(),
                clients,
            },
        },
    }
}
pub async fn start(app: &App, state: &mut State, port: Option<u16>) -> Result<McpStatus, CmdError> {
    if state.http.is_some() {
        return Ok(status(state).await);
    }
    let mut settings = load(app)?;
    let server = server(app, settings.clone())?;
    let tokens = token(app, state)?;
    let http = HttpServer::bind(
        server.clone(),
        port.or(settings.http.port).unwrap_or(0),
        tokens,
    )
    .await
    .map_err(error)?;
    settings.http.port = Some(http.port());
    settings::save(&settings::path_in(&app.config().paths), &settings).map_err(error)?;
    state.server = Some(server);
    state.http = Some(http);
    Ok(status(state).await)
}
pub fn save(app: &App, state: &State, settings: &McpSettings) -> Result<(), CmdError> {
    let permissions = Permissions::resolve(&[], false, settings).map_err(error)?;
    settings::save(&settings::path_in(&app.config().paths), settings).map_err(error)?;
    if let Some(server) = &state.server {
        server.set_permissions(permissions);
    }
    app.broadcast(UiEvent::SettingsChanged {
        keys: vec!["mcp".into()],
    });
    Ok(())
}
pub fn init(app: &App) {
    app.add_tick_source(app.ext::<Service>());
    let startup = load(app);
    let weak = app.downgrade();
    app.handle().spawn(async move {
        let Some(app) = weak.upgrade() else { return };
        match startup {
            Ok(settings) if settings.http.autostart => {
                let service = app.ext::<Service>();
                if let Err(e) = start(&app, &mut *service.state.lock().await, None).await {
                    app.push_notice(Notice {
                        level: NoticeLevel::Error,
                        code: "mcp-start-failed".into(),
                        args: std::collections::BTreeMap::from([(
                            "detail".into(),
                            serde_json::json!(e.detail),
                        )]),
                    });
                    app.flush_tick();
                };
            }
            Err(_) => {
                app.push_notice(Notice {
                    level: NoticeLevel::Error,
                    code: "mcp-settings-invalid".into(),
                    args: Default::default(),
                });
                app.flush_tick();
            }
            _ => {}
        }
        drop(app);
        let mut previous = Some(McpStatus::Off);
        let mut timer = tokio::time::interval(std::time::Duration::from_millis(100));
        // Tokio's first interval tick is immediate. Startup already delivers its
        // notices above; periodic delivery must start after the first 100 ms,
        // rather than racing newly spawned operations during initialization.
        timer.tick().await;
        loop {
            timer.tick().await;
            let Some(app) = weak.upgrade() else { break };
            let service = app.ext::<Service>();
            let current = status(&*service.state.lock().await).await;
            if previous.as_ref() != Some(&current) {
                *service.status.lock().unwrap_or_else(|p| p.into_inner()) = Some(current.clone());
                previous = Some(current);
            }
            // Retry queued startup notices/status after the coalescer's delivery deadline,
            // including an unchanged Off status after a failed start. While a crawl session is
            // active its loop delivers every due tick (MCP status included); a second flusher would
            // send that tick before the session's poll and split status and table totals (UI-018).
            if !crate::session::delivers_ticks(&app) {
                app.flush_tick();
            }
        }
    });
}

/// Populate the prebuilt MCP preferences DTO from the shared headless/app settings file.
/// Missing files preserve the legacy settings DTO unchanged.
pub fn enrich_settings(dir: &std::path::Path, dto: &mut AppSettingsDto) {
    let path = dir.join(settings::FILE_NAME);
    if path.is_file() {
        if let Ok(settings) = settings::load(&path) {
            dto.mcp.autostart = settings.http.autostart;
            dto.mcp.port = settings.http.port;
            dto.mcp.classes = Some(
                settings
                    .classes
                    .iter()
                    .map(|c| format!("{c:?}").to_ascii_lowercase())
                    .collect(),
            );
        }
    }
}

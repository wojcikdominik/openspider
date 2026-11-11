//! Diagnostics commands (OUT-244). Usage statistics remain owned by M2-O02.
use crate::{
    diag,
    dto::*,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{
        error::{parse_args, to_json},
        info::app_info,
        App, CallCtx,
    },
};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::SystemTime,
};

type Entries = Vec<(String, Vec<u8>)>;
type TaskProvider = Arc<dyn Fn() -> Value + Send + Sync>;
#[derive(Default)]
struct BundleState {
    preview: Mutex<Option<Entries>>,
    tasks: Mutex<Option<TaskProvider>>,
}
/// Scheduling can provide its secret-free task listing without a dependency on its storage format.
pub fn set_task_provider(app: &App, provider: TaskProvider) {
    *app.ext::<BundleState>()
        .tasks
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(provider);
}
fn io_error(e: std::io::Error) -> CmdError {
    let code = if e.kind() == std::io::ErrorKind::Interrupted {
        ErrorCode::Cancelled
    } else {
        ErrorCode::Io
    };
    CmdError::new(code, "err-diagnostics").with_detail(diag::redact_text(&e.to_string()))
}
fn integrations(app: &App) -> Value {
    [
        "gsc",
        "ga4",
        "psi",
        "majestic",
        "ahrefs",
        "moz",
        "dataforseo",
    ]
    .iter()
    .map(|kind| {
        (
            kind.to_string(),
            json!(app.config().secrets.get(&format!("api/{kind}")).is_some()),
        )
    })
    .collect::<serde_json::Map<_, _>>()
    .into()
}
fn entries(app: &App) -> Result<Entries, CmdError> {
    let settings = app.settings_snapshot();
    let mut system = diag::system_info();
    system["app"] = serde_json::to_value(app_info(app.config(), &settings)).unwrap_or(Value::Null);
    system["resourceLimits"] = serde_json::to_value(&settings.storage).unwrap_or(Value::Null);
    system["integrations"] = integrations(app);
    let sessions = app.sessions().all();
    let configs: Vec<_> = sessions
        .iter()
        .map(|s| {
            diag::config_diff(
                &serde_json::to_value(s.config.as_ref()).unwrap_or(Value::Null),
                &serde_json::to_value(os_model::CrawlConfig::default()).unwrap_or(Value::Null),
            )
        })
        .collect();
    let diff = json!({"settings":diag::config_diff(&serde_json::to_value(settings).unwrap_or(Value::Null),&serde_json::to_value(os_model::settings::AppSettings::default()).unwrap_or(Value::Null)),"crawls":configs});
    let state = app.ext::<BundleState>();
    let provider = state
        .tasks
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let tasks = provider.map(|p| p()).unwrap_or_else(|| json!([]));
    diag::bundle_entries(
        &app.config().paths.logs,
        system,
        diff,
        tasks,
        SystemTime::now(),
    )
    .map_err(io_error)
}
fn preview<'a>(app: &'a App, _: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let owned_app = app.clone();
        let snapshot = tokio::task::spawn_blocking(move || entries(&owned_app))
            .await
            .map_err(|_| CmdError::new(ErrorCode::Internal, "err-diagnostics"))??;
        let result = BundlePreview {
            files: snapshot.iter().map(|(n, _)| n.clone()).collect(),
            warnings: Vec::new(),
        };
        *app.ext::<BundleState>()
            .preview
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(snapshot);
        to_json(&result)
    })
}
fn debug_info<'a>(app: &'a App, ctx: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let settings = app.settings_snapshot();
        let mut system = diag::system_info();
        system["integrations"] = integrations(app);
        let session = app.sessions().current(&ctx.window);
        let mut extra = Vec::new();
        if let Some(m) = system.as_object() {
            for (name, value) in m {
                extra.push(NameValue {
                    name: name.clone(),
                    value: Some(value.clone()),
                });
            }
        }
        extra.push(NameValue {
            name: "resourceLimits".into(),
            value: Some(diag::redact(
                &serde_json::to_value(&settings.storage).unwrap_or(Value::Null),
            )),
        });
        // CRAWL-151: eingestellter Wert des Crawls im Fenster (`auto` oder 1–6).
        if let Some(s) = &session {
            extra.push(NameValue {
                name: "connectionsPerHost".into(),
                value: Some(
                    serde_json::to_value(s.config.speed.connections_per_host)
                        .unwrap_or(Value::Null),
                ),
            });
        }
        let sys = sysinfo::System::new_all();
        let rss = sysinfo::get_current_pid()
            .ok()
            .and_then(|pid| sys.process(pid).map(|p| p.memory()));
        to_json(&DebugInfo {
            app: app_info(app.config(), &settings),
            os: system["os"].as_str().unwrap_or(std::env::consts::OS).into(),
            pid: std::process::id(),
            rss_engine_bytes: rss,
            rss_renderer_bytes: None,
            ram_limit_bytes: settings
                .storage
                .ram_limit_gb
                .map(|n| (n * 1024.0 * 1024.0 * 1024.0) as u64)
                .unwrap_or(0),
            crawl_file_bytes: session.and_then(|s| s.path.metadata().ok().map(|m| m.len())),
            free_disk_bytes: None,
            performance_profile: settings.storage.performance_profile.to_string(),
            sqlite_cache_kib: None,
            extra,
        })
    })
}
fn bundle<'a>(app: &'a App, _: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let a: BundleArgs = parse_args(args)?;
        let snapshot = app
            .ext::<BundleState>()
            .preview
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        let owned_app = app.clone();
        let handle = app
            .ops()
            .spawn("op-diagnostics", true, move |cancel, progress| {
                Box::pin(async move {
                    tokio::task::spawn_blocking(move || {
                        if cancel.is_cancelled() {
                            return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
                        }
                        let snapshot = match snapshot {
                            Some(s) => s,
                            None => entries(&owned_app)?,
                        };
                        let bytes = diag::write_bundle(Path::new(&a.dest), &snapshot, || {
                            cancel.is_cancelled()
                        })
                        .map_err(io_error)?;
                        progress.report(snapshot.len() as u64, Some(snapshot.len() as u64));
                        to_json(&FileResult {
                            path: a.dest,
                            bytes,
                        })
                    })
                    .await
                    .map_err(|_| CmdError::new(ErrorCode::Internal, "err-diagnostics"))?
                })
            });
        to_json(&handle)
    })
}
fn frontend<'a>(_: &'a App, _: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let a: LogFrontendArgs = parse_args(args)?;
        let message = diag::redact_text(&a.message);
        let context = diag::redact(&a.context.unwrap_or(Value::Null)).to_string();
        match a.level.as_str() {
            "error" => tracing::error!(target:"frontend",%message,%context),
            "warn" => tracing::warn!(target:"frontend",%message,%context),
            "info" => tracing::info!(target:"frontend",%message,%context),
            "debug" => tracing::debug!(target:"frontend",%message,%context),
            _ => return Err(CmdError::invalid_input("err-log-level", "level")),
        }
        Ok(Value::Null)
    })
}
fn open<'a>(app: &'a App, _: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        std::fs::create_dir_all(&app.config().paths.logs).map_err(io_error)?;
        app.desktop_actions()
            .ok_or_else(|| CmdError::new(ErrorCode::Unsupported, "err-desktop-actions"))?
            .reveal_paths(&[app.config().paths.logs.clone()])?;
        Ok(Value::Null)
    })
}
fn usage<'a>(_: &'a App, _: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async { Err(CmdError::unsupported("diag.usageStats", "M2-O02")) })
}
pub fn register(r: &mut Registry) {
    for (name, args, ret, long_running, h) in [
        (
            "diag.previewBundle",
            "void",
            "BundlePreview",
            false,
            preview as crate::registry::Handler,
        ),
        ("diag.debugInfo", "void", "DebugInfo", false, debug_info),
        ("diag.bundle", "BundleArgs", "FileResult", true, bundle),
        (
            "diag.logFrontend",
            "LogFrontendArgs",
            "void",
            false,
            frontend,
        ),
        ("diag.openLogs", "void", "void", false, open),
        (
            "diag.usageStats",
            "UsageStatsArgs",
            "UsageStats",
            false,
            usage,
        ),
    ] {
        r.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running,
            },
            h,
        );
    }
}

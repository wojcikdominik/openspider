//! Interactive form authentication; secrets remain in the encrypted profile (CRAWL-120, OUT-223).
use crate::{
    dto::*,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{
        error::{parse_args, to_json},
        App, CallCtx, WindowId,
    },
};
use futures_util::future::BoxFuture;
use os_render::login::{CapturedSession, LoginOpts, LoginSession};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tokio::sync::oneshot;

struct Pending {
    window: WindowId,
    finish: oneshot::Sender<()>,
    result: oneshot::Receiver<Result<CapturedSession, CmdError>>,
    url: String,
    profile: Option<String>,
}
/// Headless CDP override for fixture tests; production defaults to a visible window.
pub struct LoginBrowserMode {
    pub headless: std::sync::atomic::AtomicBool,
    /// Native CDP automation seam; never set from IPC arguments.
    pub on_ready: Mutex<Option<std::sync::Arc<LoginAutomation>>>,
}
impl Default for LoginBrowserMode {
    fn default() -> Self {
        Self {
            // Only the native E2E bridge may opt into headless browser automation.
            headless: std::sync::atomic::AtomicBool::new(
                cfg!(feature = "e2e-bridge")
                    && std::env::var_os("OPENSPIDER_LOGIN_HEADLESS").is_some(),
            ),
            on_ready: Mutex::new(None),
        }
    }
}
pub type LoginAutomation =
    dyn for<'a> Fn(&'a LoginSession) -> BoxFuture<'a, Result<(), CmdError>> + Send + Sync;
#[derive(Default)]
struct Logins(Mutex<HashMap<String, Pending>>);
#[derive(Default)]
struct Captures(Mutex<HashMap<WindowId, Vec<Value>>>);
fn error() -> CmdError {
    CmdError::new(ErrorCode::InvalidInput, "err-form-login")
}
fn cancelled() -> CmdError {
    CmdError::new(ErrorCode::Cancelled, "err-cancelled")
}

fn open<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let a: AuthOpenLoginArgs = parse_args(args)?;
        let url = os_url::parse_start_url(&a.url).map_err(|_| error())?;
        if !url.url.username().is_empty() || url.url.password().is_some() {
            return Err(error());
        }
        let cfg = crate::session::control::config(app, &ctx, None).await?;
        let network = app.settings_snapshot().network.clone();
        let proxy_credentials = match &network.proxy.username {
            Some(username) => Some(os_render::api::Credentials {
                username: username.clone(),
                password: app
                    .config()
                    .secrets
                    .try_get(os_fetch::net::PROXY_PASSWORD)
                    .map_err(|_| error())?
                    .unwrap_or_default(),
            }),
            None => None,
        };
        let profile = a.profile.clone();
        let initial = if let Some(name) = &profile {
            Some(
                os_engine::session_guard::read_profile(
                    &app.config().paths.settings.join("auth"),
                    name,
                    &*app.config().secrets,
                )
                .ok_or_else(error)?,
            )
        } else {
            None
        };
        let opts = LoginOpts {
            headless: app
                .ext::<LoginBrowserMode>()
                .headless
                .load(std::sync::atomic::Ordering::Relaxed),
            user_agent: Some(os_fetch::user_agents::resolve(&cfg.user_agent).http),
            viewport: a
                .viewport
                .map(|v| (v.width, v.height))
                .unwrap_or((1024, 768)),
            initial,
            launch: os_render::LaunchOpts {
                chromium_path: cfg.rendering.chromium_path.clone().map(Into::into),
                network,
                proxy_credentials,
                ..Default::default()
            },
        };
        let automation = app
            .ext::<LoginBrowserMode>()
            .on_ready
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let (finish, mut finished) = oneshot::channel();
        let (result, ready) = oneshot::channel();
        let login_url = url.encoded.clone();
        app.ext::<Logins>()
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|id, _| {
                app.ops()
                    .status(id)
                    .is_some_and(|s| s.state == OpState::Running)
            });
        let handle=app.ops().spawn("op-form-login",true,move |cancel,_| Box::pin(async move {
            let mut browser=tokio::select! {
                result=LoginSession::open(&login_url,opts)=>result.map_err(|_|error())?,
                _=async { while !cancel.is_cancelled() { tokio::time::sleep(Duration::from_millis(25)).await; } } => return Err(cancelled()),
            };
            // The E2E bridge can publish the actual endpoint in its private test directory.
            #[cfg(feature = "e2e-bridge")]
            if let Some(path) = std::env::var_os("OPENSPIDER_LOGIN_CDP_FILE") {
                std::fs::write(path, browser.websocket_address()).map_err(|_| error())?;
            }
            let work=async {
                if let Some(automation) = automation { automation(&browser).await?; }
                loop {
                tokio::select! {
                    done=&mut finished => {
                        if done.is_err() { break Err(cancelled()); }
                        break browser.capture().await.map_err(|_|error());
                    },
                    _=tokio::time::sleep(Duration::from_millis(50))=> {
                        if cancel.is_cancelled() { break Err(cancelled()); }
                        // Retain storage of origins encountered while the user navigates.
                        if browser.capture().await.is_err() {
                            // A navigation may temporarily destroy the execution context.
                            if browser.page().url().await.is_err() { break Err(cancelled()); }
                        }
                    }
                }
            } };
            let captured=tokio::select! {
                result=work=>result,
                _=async {while !cancel.is_cancelled() {tokio::time::sleep(Duration::from_millis(25)).await;}}=>Err(cancelled()),
            };
            let reply=captured.as_ref().map(|s|LoginCapture { final_url:s.final_url.clone(),cookies:s.cookies.len() as u32,local_storage:s.storage.iter().map(|s|s.local_storage.len() as u32).sum(),session_storage:s.storage.iter().map(|s|s.session_storage.len() as u32).sum() }).map_err(Clone::clone);
            // B4-FLAKY (CRAWL-120): „Sitzung übernehmen“ answers as soon as the session is captured. Closing the login
            // browser (CDP close with up to 2 s grace, process kill and wait, removal of the temporary profile) is cleanup
            // and must not delay the acknowledgement (Windows CI: 3.5 to 5 s from the click to the reply while it waited).
            let _=result.send(captured);
            browser.close().await;
            to_json(&reply?)
        }));
        app.ext::<Logins>()
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                handle.op_id.clone(),
                Pending {
                    window: ctx.window,
                    finish,
                    result: ready,
                    url: url.encoded,
                    profile,
                },
            );
        to_json(&handle)
    })
}
fn finish<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let a: AuthFinishLoginArgs = parse_args(args)?;
        let state = app.ext::<Logins>();
        let pending = {
            let mut entries = state.0.lock().unwrap_or_else(|p| p.into_inner());
            if entries.get(&a.op_id).is_none_or(|p| p.window != ctx.window) {
                return Err(error());
            }
            entries.remove(&a.op_id).ok_or_else(error)?
        };
        pending.finish.send(()).map_err(|_| cancelled())?;
        let captured = pending.result.await.map_err(|_| cancelled())??;
        let requested = a
            .save_as
            .or(pending.profile)
            .unwrap_or_else(|| format!("form-{}", a.op_id));
        let profiles = app
            .dispatch(ctx.clone(), "auth.listProfiles", Value::Null)
            .await?;
        // Never overwrite a profile used by the current configuration. Cancelling the
        // outer draft must preserve both its reference and its encrypted session.
        let name = if profiles
            .as_array()
            .is_some_and(|rows| rows.iter().any(|r| r["name"] == requested))
        {
            if requested.len() + a.op_id.len() < 120 {
                format!("{requested}-{}", a.op_id)
            } else {
                format!("form-{}", a.op_id)
            }
        } else {
            requested
        };
        let host = os_url::parse_start_url(&pending.url)
            .map_err(|_| error())?
            .url
            .host_str()
            .ok_or_else(error)?
            .to_string();
        let value=app.dispatch(ctx.clone(),"auth.saveProfile",json!({"profile":{"name":name,"kind":"form","host":host,"username":null,"secret":serde_json::to_string(&captured).map_err(|_|error())?}})).await?;
        // Profile saving is independent of activation, like profile.load. Only the
        // outer configuration dialog applies auth.form/session_guard via config.set.
        let row = json!({"profile":name,"loginUrl":pending.url,"state":"active",
            "cookies":captured.cookies.len(),"capturedAt":app.config().clock.now().to_db()});
        let captures = app.ext::<Captures>();
        let mut windows = captures.0.lock().unwrap_or_else(|p| p.into_inner());
        let rows = windows.entry(ctx.window).or_default();
        rows.retain(|r| r["loginUrl"] != row["loginUrl"]);
        rows.push(row);
        Ok(value)
    })
}
/// Secret-free live status, including expiry and persisted loss counters. Used by the form page/status bar.
fn status<'a>(app: &'a App, ctx: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let cfg = crate::session::control::config(app, &ctx, None).await?;
        let lost = if let Some(s) = app.sessions().current(&ctx.window) {
            app.read_pool()
                .run(s.key, |f, _| {
                    os_store::queries::form_session_losses(f.conn())
                })
                .await?
        } else {
            std::collections::BTreeSet::new()
        };
        let drafts = app
            .ext::<Captures>()
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&ctx.window)
            .cloned()
            .unwrap_or_default();
        let dir = app.config().paths.settings.join("auth");
        let secrets = app.config().secrets.clone();
        let now = app.config().clock.now().unix_millis() / 1000;
        tokio::task::spawn_blocking(move || {
            let mut rows:Vec<_>=cfg.auth.form.iter().map(|f| {
                let session=f.profile.as_deref().and_then(|name|os_engine::session_guard::read_profile(&dir,name,&*secrets));
                let state=if f.profile.as_ref().is_some_and(|p|lost.contains(p)) {"lost"} else if session.as_ref().is_some_and(|s|s.expired(now)) {"expired"} else if session.is_some(){"active"}else{"missing"};
                json!({"profile":f.profile,"loginUrl":f.login_url,"state":state,"cookies":f.cookie_count,"capturedAt":f.captured_at})
            }).collect();
            // Include unactivated metadata for the owning window's configuration draft.
            for mut draft in drafts {
                if !rows.iter().any(|row| row["profile"] == draft["profile"]) {
                    let session = draft["profile"].as_str().and_then(|name| os_engine::session_guard::read_profile(&dir, name, &*secrets));
                    draft["state"] = json!(if session.as_ref().is_some_and(|s| s.expired(now)) { "expired" } else if session.is_some() { "active" } else { "missing" });
                    rows.push(draft);
                }
            }
            Ok(Value::Array(rows))
        }).await.map_err(|_|error())?
    })
}
pub fn register(r: &mut Registry) {
    for (name, args, ret, long_running, handler) in [
        (
            "auth.openLogin",
            "AuthOpenLoginArgs",
            "LoginCapture",
            true,
            open as crate::registry::Handler,
        ),
        (
            "auth.finishLogin",
            "AuthFinishLoginArgs",
            "AuthProfileInfo | null",
            false,
            finish,
        ),
        ("auth.loginStatus", "void", "JsonValue", false, status),
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

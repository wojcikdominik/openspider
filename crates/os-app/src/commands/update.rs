//! Update check commands and app-local state; download/install are owned by U02.
use crate::{
    registry::{CmdResult, CommandMeta, Handler, Registry},
    runtime::{App, CallCtx},
};
use futures_util::future::BoxFuture;
use serde_json::Value;

fn download<'a>(app: &'a App, _: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move { crate::runtime::error::to_json(&crate::update::download::download(app)) })
}
fn install<'a>(app: &'a App, _: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: crate::dto::update::UpdateInstallArgs = crate::runtime::error::parse_args(args)?;
        crate::runtime::error::to_json(&crate::update::install::install(app, args.when).await)
    })
}

pub fn register(r: &mut Registry) {
    for (name, args, ret, handler) in [
        ("update.check", "void", "UpdateState", check as Handler),
        ("update.status", "void", "UpdateState", status as Handler),
        (
            "update.download",
            "void",
            "UpdateState",
            download as Handler,
        ),
        (
            "update.install",
            "UpdateInstallArgs",
            "UpdateState",
            install as Handler,
        ),
        (
            "update.set_auto",
            "UpdateSetAutoArgs",
            "void",
            set_auto as Handler,
        ),
    ] {
        r.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running: false,
            },
            handler,
        );
    }
}

/// App-local state, independent of every crawl session. U02 may consume the selected manifest.
pub struct UpdateService {
    pub gate: tokio::sync::Mutex<()>,
    pub state: std::sync::Mutex<crate::update::UpdateState>,
    pub manifest: std::sync::Mutex<Option<crate::update::UpdateManifest>>,
    last_attempt: std::sync::Mutex<Option<time::OffsetDateTime>>,
}
impl Default for UpdateService {
    fn default() -> Self {
        Self {
            gate: Default::default(),
            state: std::sync::Mutex::new(crate::update::UpdateState::Idle),
            manifest: Default::default(),
            last_attempt: Default::default(),
        }
    }
}
fn check<'a>(app: &'a App, _: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move { crate::runtime::error::to_json(&run_check(app, true).await) })
}
fn status<'a>(app: &'a App, _: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let service = app.ext::<UpdateService>();
        let state = service
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        crate::runtime::error::to_json(&state)
    })
}
fn set_auto<'a>(app: &'a App, _: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: crate::dto::update::UpdateSetAutoArgs = crate::runtime::error::parse_args(args)?;
        let service = app.ext::<UpdateService>();
        let _guard = service.gate.lock().await;
        app.settings()
            .apply_patch(&serde_json::json!({"updates":{"check":args.check}}))?;
        // B4-UPDDLG (A3): switching automatic checks off from the update dialog keeps a known release (available,
        // failed download, downloading or ready): its manifest stays, so "Download in background" still works.
        let release_known = service
            .manifest
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some();
        if !args.check
            && !release_known
            && !matches!(
                *service.state.lock().unwrap_or_else(|p| p.into_inner()),
                crate::update::UpdateState::Ready(_)
                    | crate::update::UpdateState::Downloading { .. }
            )
        {
            *service.state.lock().unwrap_or_else(|p| p.into_inner()) =
                crate::update::UpdateState::Disabled(crate::update::DisableReason::Settings);
            *service.manifest.lock().unwrap_or_else(|p| p.into_inner()) = None;
        }
        Ok(Value::Null)
    })
}
pub async fn run_check(app: &App, manual: bool) -> crate::update::UpdateState {
    let context = crate::update::UpdateContext {
        network: app.settings_snapshot().network,
        ..Default::default()
    };
    run_check_with_context(app, manual, context).await
}
/// Injected transport configuration for fixture checks; settings and clock remain app-owned.
pub async fn run_check_with_context(
    app: &App,
    manual: bool,
    mut context: crate::update::UpdateContext,
) -> crate::update::UpdateState {
    use crate::update::{self, policy::Outcome, UpdateState};
    let service = app.ext::<UpdateService>();
    // B3-UI: a background download holds the gate for its whole duration (after a sleep of the computer up to its
    // one-hour timeout). The state already answers a check then, so a manual check must not queue behind it.
    let current = service
        .state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if matches!(
        current,
        UpdateState::Downloading { .. } | UpdateState::Ready(_)
    ) {
        return current;
    }
    let _guard = service.gate.lock().await;
    let previous = service
        .state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if matches!(
        previous,
        UpdateState::Downloading { .. } | UpdateState::Ready(_)
    ) {
        return previous;
    }
    context.settings = app.settings_snapshot().updates;
    context.now = app.config().clock.now().0;
    context.last_attempt = std::sync::Mutex::new(
        *service
            .last_attempt
            .lock()
            .unwrap_or_else(|p| p.into_inner()),
    );
    if context.disabled().is_none() && context.network.proxy.username.is_some() {
        let secrets = app.config().secrets.clone();
        context.proxy_password = app
            .handle()
            .spawn_blocking(move || secrets.try_get(os_fetch::net::PROXY_PASSWORD))
            .await
            .ok()
            .and_then(Result::ok)
            .flatten();
    }
    // policy::check reserves the attempt before network I/O. Persist that timestamp even on errors.
    *service.state.lock().unwrap_or_else(|p| p.into_inner()) = UpdateState::Checking;
    let reserved = update::policy::reserve(&context, manual);
    if let Some(at) = *context
        .last_attempt
        .lock()
        .unwrap_or_else(|p| p.into_inner())
    {
        *service
            .last_attempt
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(at);
        let timestamp = os_model::Timestamp(at).to_db();
        if app
            .settings()
            .apply_patch(&serde_json::json!({"updates":{"lastCheck":timestamp}}))
            .is_err()
        {
            tracing::warn!("Zeitpunkt der Update-Prüfung konnte nicht gespeichert werden");
        }
    }
    let result = match reserved {
        Ok(None) => update::policy::fetch(&context).await,
        Ok(Some(outcome)) => Ok(outcome),
        Err(error) => Err(error),
    };
    let state = match result {
        // OUT-251 (Nutzerentscheidung 09.11.2026): after "Skip this version" automatic checks report only a version
        // newer than the skipped one (SemVer order, pre-releases included: 1.0.0-beta.4 < 1.0.0-rc.1 < 1.0.0); the
        // manual check reports it anyway.
        Ok(Outcome::Available(manifest))
            if !manual && skipped_covers(&context, &manifest.version) =>
        {
            UpdateState::UpToDate
        }
        Ok(Outcome::Available(manifest)) => {
            let state = UpdateState::Available(update::AvailableUpdate {
                version: manifest.version.to_string(),
                notes: manifest.notes.clone(),
                pub_date: manifest.pub_date.clone(),
            });
            *service.manifest.lock().unwrap_or_else(|p| p.into_inner()) = Some(manifest);
            state
        }
        Ok(Outcome::Throttled) => {
            // Do not erase an available update on a throttled startup check.
            *service.state.lock().unwrap_or_else(|p| p.into_inner()) = previous.clone();
            return previous;
        }
        Ok(Outcome::Current) => UpdateState::UpToDate,
        Ok(Outcome::Disabled(reason)) => {
            use update::policy::Disabled;
            UpdateState::Disabled(match reason {
                Disabled::Settings => update::DisableReason::Settings,
                Disabled::Environment => update::DisableReason::Environment,
                Disabled::MissingPublicKey => update::DisableReason::MissingPublicKey,
                Disabled::InvalidPublicKey => update::DisableReason::InvalidPublicKey,
                Disabled::UnknownInstaller => update::DisableReason::UnknownInstaller,
            })
        }
        Err(error) if manual => UpdateState::Failed(error),
        Err(_) => UpdateState::Idle, // startup failures produce logs only
    };
    if !matches!(state, UpdateState::Available(_)) {
        *service.manifest.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
    *service.state.lock().unwrap_or_else(|p| p.into_inner()) = state.clone();
    state
}
/// True if the offered version is not newer than the skipped one; an unreadable stored value skips nothing.
fn skipped_covers(context: &crate::update::UpdateContext, offered: &semver::Version) -> bool {
    context
        .settings
        .skipped_version
        .as_deref()
        .and_then(|skipped| semver::Version::parse(skipped.trim()).ok())
        .is_some_and(|skipped| *offered <= skipped)
}
pub fn on_startup(app: &App) {
    crate::update::install::init(app);
    if !app.config().check_updates_on_startup {
        return;
    }
    let app = app.clone();
    app.handle().spawn(async move {
        run_check(&app, false).await;
    });
}

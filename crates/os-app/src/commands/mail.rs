//! Mail account commands and the UI completion hook (OUT-280 … OUT-282).
use crate::{
    dto::*,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{
        error::{parse_args, to_json},
        lifecycle_bus::CrawlEvent,
        App, CallCtx,
    },
};
use futures_util::future::BoxFuture;
use os_sched::mail::{Account, AccountStore, Auth, Completion, MailError, Policy, Tls};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Mutex};
#[derive(Default)]
struct State {
    mutation: Mutex<()>,
    sent: Mutex<HashSet<String>>,
    pending: Mutex<HashSet<String>>,
}
fn error(e: MailError) -> CmdError {
    let mut err = CmdError::new(ErrorCode::InvalidInput, "err-mail").with_detail(e.message);
    if let Some(code) = e.smtp_code {
        err = err.with_arg("smtpCode", code);
    }
    err
}
fn store(app: &App) -> AccountStore {
    AccountStore::new(&app.config().paths.settings)
}
fn info(app: &App, a: Account) -> MailAccountInfo {
    MailAccountInfo {
        id: a.id.clone(),
        label: a.label,
        host: a.host,
        port: a.port,
        tls: match a.tls {
            Tls::Starttls => MailTlsDto::Starttls,
            Tls::Tls => MailTlsDto::Tls,
            Tls::None => MailTlsDto::None,
        },
        from: a.from,
        has_secret: app
            .config()
            .secrets
            .get(&format!("mail/{}", a.id))
            .is_some(),
    }
}
fn list<'a>(app: &'a App, _: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let app = app.clone();
        tokio::task::spawn_blocking(move || {
            let state = app.ext::<State>();
            let _g = state.mutation.lock().unwrap_or_else(|p| p.into_inner());
            to_json(
                &store(&app)
                    .list()
                    .map_err(error)?
                    .into_iter()
                    .map(|a| info(&app, a))
                    .collect::<Vec<_>>(),
            )
        })
        .await
        .map_err(|_| CmdError::new(ErrorCode::Internal, "err-mail"))?
    })
}
fn save<'a>(app: &'a App, _: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let input: MailSaveAccountArgs = parse_args(args)?;
        let app = app.clone();
        tokio::task::spawn_blocking(move || {
            let state = app.ext::<State>();
            let _g = state.mutation.lock().unwrap_or_else(|p| p.into_inner());
            let i = input.account;
            let previous =
                i.id.as_deref()
                    .map(|id| store(&app).get(id))
                    .transpose()
                    .ok()
                    .flatten();
            let auth = match i.auth.as_deref() {
                None => previous.as_ref().map_or(Auth::Password, |a| a.auth),
                Some("password") => Auth::Password,
                Some("gmail") => Auth::Gmail,
                Some("microsoft365") => Auth::Microsoft365,
                _ => return Err(error(MailError::new("Unbekannte Mail-Authentifizierung"))),
            };
            let id = i.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let account = Account {
                id,
                label: i.label,
                host: i.host,
                port: i.port,
                tls: match i.tls {
                    MailTlsDto::Starttls => Tls::Starttls,
                    MailTlsDto::Tls => Tls::Tls,
                    MailTlsDto::None => Tls::None,
                },
                username: i.username,
                from: i.from,
                auth,
                allow_insecure: i
                    .allow_insecure
                    .unwrap_or_else(|| previous.as_ref().is_some_and(|a| a.allow_insecure)),
                client_id: i
                    .client_id
                    .or_else(|| previous.as_ref().and_then(|a| a.client_id.clone())),
            };
            let mut password = i.password;
            if previous
                .as_ref()
                .is_some_and(|old| old.auth != account.auth)
                && password.is_none()
            {
                if account.auth == Auth::Password {
                    return Err(error(MailError::new(
                        "Beim Wechsel zu SMTP das Kennwort neu angeben",
                    )));
                }
                password = Some("{}".into());
            }
            if let Some(client_secret) = i.client_secret {
                if account.auth == Auth::Password {
                    return Err(error(MailError::new(
                        "OAuth-Client-Secret erfordert ein OAuth-Konto",
                    )));
                }
                let existing = password
                    .clone()
                    .or_else(|| app.config().secrets.get(&format!("mail/{}", account.id)));
                let mut credential: Value = existing
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .filter(Value::is_object)
                    .unwrap_or_else(|| json!({}));
                credential["client_secret"] = json!(client_secret);
                password = Some(credential.to_string());
            }
            store(&app)
                .save(&account, password.as_deref(), app.config().secrets.as_ref())
                .map_err(error)?;
            let mut ids = store(&app)
                .list()
                .map_err(error)?
                .into_iter()
                .map(|a| a.id)
                .collect::<Vec<_>>();
            ids.sort();
            app.apply_settings_patch(&json!({"accounts":{"mailAccounts":ids}}))?;
            to_json(&info(&app, account))
        })
        .await
        .map_err(|_| CmdError::new(ErrorCode::Internal, "err-mail"))?
    })
}
fn delete<'a>(app: &'a App, _: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let id: IdArgs = parse_args(args)?;
        let app = app.clone();
        tokio::task::spawn_blocking(move || {
            let state = app.ext::<State>();
            let _g = state.mutation.lock().unwrap_or_else(|p| p.into_inner());
            store(&app)
                .delete(&id.id, app.config().secrets.as_ref())
                .map_err(error)?;
            let ids = store(&app)
                .list()
                .map_err(error)?
                .into_iter()
                .map(|a| a.id)
                .collect::<Vec<_>>();
            let mut patch = json!({"accounts":{"mailAccounts":ids}});
            if app.settings_snapshot().accounts.notify_account.as_deref() == Some(&id.id) {
                patch["accounts"]["notifyAccount"] = Value::Null;
            }
            app.apply_settings_patch(&patch)?;
            Ok(Value::Null)
        })
        .await
        .map_err(|_| CmdError::new(ErrorCode::Internal, "err-mail"))?
    })
}
async fn test_now(app: App, args: MailTestArgs) -> Result<MailTestResult, CmdError> {
    let a = {
        let app = app.clone();
        let id = args.id.clone();
        tokio::task::spawn_blocking(move || store(&app).get(&id))
            .await
            .map_err(|_| CmdError::new(ErrorCode::Internal, "err-mail"))?
            .map_err(error)?
    };
    let saved = app.config().secrets.get(&format!("mail/{}", a.id));
    let credential: Value = saved
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    if a.auth != Auth::Password
        && !credential["access_token"].is_string()
        && !credential["refresh_token"].is_string()
    {
        let client_id = a
            .client_id
            .as_deref()
            .ok_or_else(|| error(MailError::new("OAuth-Client-ID erforderlich")))?;
        let endpoints = if a.auth == Auth::Gmail {
            os_integrations::oauth::Endpoints::default()
        } else {
            os_integrations::oauth::Endpoints {
                authorize: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
                token: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
                revoke: String::new(),
            }
        };
        let auth = os_integrations::oauth::Authorization::begin_mail(
            client_id,
            a.auth == Auth::Microsoft365,
            &endpoints,
        )
        .await
        .map_err(|_| error(MailError::new("OAuth-Anmeldung nicht verfügbar")))?;
        auth.open_browser()
            .map_err(|_| error(MailError::new("OAuth-Browser nicht verfügbar")))?;
        let proxy_password = app.config().secrets.get(os_fetch::net::PROXY_PASSWORD);
        let client = os_fetch::net::mail_oauth_client(
            &app.settings_snapshot().network,
            proxy_password.as_deref(),
        )
        .map_err(|_| error(MailError::new("OAuth-Netzwerk nicht verfügbar")))?;
        let credential = auth
            .finish(
                &client,
                client_id,
                credential["client_secret"]
                    .as_str()
                    .filter(|s| !s.is_empty()),
                &endpoints,
                app.config().clock.clone(),
            )
            .await
            .map_err(|_| error(MailError::new("OAuth-Anmeldung fehlgeschlagen")))?;
        app.config()
            .secrets
            .set(
                &format!("mail/{}", a.id),
                &serde_json::to_string(&credential)
                    .map_err(|_| error(MailError::new("OAuth-Token ungültig")))?,
            )
            .map_err(|_| error(MailError::new("OAuth-Token nicht speicherbar")))?;
    }
    let completion = Completion {
        project: "Testmail".into(),
        ..Default::default()
    };
    let result = os_sched::mail::send_with_context(
        &a,
        app.config().secrets.as_ref(),
        &app.settings_snapshot().network,
        &[args.to],
        &completion,
        &os_sched::mail::DeliveryContext {
            clock: app.config().clock.as_ref(),
            ..Default::default()
        },
    )
    .await;
    Ok(MailTestResult {
        ok: result.is_ok(),
        error: result.err().map(error),
    })
}
fn test<'a>(app: &'a App, _: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: MailTestArgs = parse_args(args)?;
        let owned = app.clone();
        let op=app.ops().spawn("mail.testAccount",true,move |cancel,_|Box::pin(async move {
        tokio::select! { r=test_now(owned,args)=>to_json(&r?), _=async {while !cancel.is_cancelled(){tokio::time::sleep(std::time::Duration::from_millis(20)).await;}}=>Err(CmdError::new(ErrorCode::Cancelled,"err-cancelled")) }
    }));
        to_json(&op)
    })
}
pub fn register(r: &mut Registry) {
    for (name, arg, result, long, handler) in [
        (
            "mail.listAccounts",
            "void",
            "MailAccountInfo[]",
            false,
            list as crate::registry::Handler,
        ),
        (
            "mail.saveAccount",
            "MailSaveAccountArgs",
            "MailAccountInfo",
            false,
            save,
        ),
        ("mail.deleteAccount", "IdArgs", "void", false, delete),
        (
            "mail.testAccount",
            "MailTestArgs",
            "MailTestResult",
            true,
            test,
        ),
    ] {
        r.register(
            CommandMeta {
                name,
                args: arg,
                ret: result,
                long_running: long,
            },
            handler,
        );
    }
}
/// Subscribe once at startup. Analysis completion explicitly releases pending mail.
pub fn on_startup(app: &App) {
    app.crawl_events().subscribe(Box::new(|app, event| {
        if let CrawlEvent::Started(key) | CrawlEvent::Resumed(key) = event {
            app.ext::<State>()
                .sent
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&key.0);
        }
        if let CrawlEvent::Completed(key) | CrawlEvent::Stopped(key) = event {
            let automatic = !app
                .ext::<crate::session::SessionService>()
                .quitting
                .load(std::sync::atomic::Ordering::SeqCst)
                && app.sessions().all().iter().any(|s| {
                    s.key == *key
                        && s.state == os_model::CrawlState::Completed
                        && s.config.analysis.auto_at_end
                });
            if automatic {
                app.ext::<State>()
                    .pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(key.0.clone());
            } else {
                queue_ui(app, key, None);
            }
        }
    }));
}
/// Called after automatic analysis has published its final status (K2-61).
pub fn after_analysis(app: &App, key: &crate::pool::CrawlKey) {
    if app
        .ext::<State>()
        .pending
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&key.0)
    {
        queue_ui(app, key, None);
    }
}
/// Engine failures do not emit Completed; this additive hook covers that exit (OUT-281).
pub fn on_failed(app: &App, key: &crate::pool::CrawlKey) {
    queue_ui(app, key, Some("Crawl fehlgeschlagen".into()));
}
/// A run that ends regularly as `Failed` (CRAWL-128, e.g. unreachable start URL) is a failure for the mail (OUT-281), with
/// `error` as the message. Delivery is once per run, so the following `Completed` event sends no success mail.
pub fn on_failed_with(app: &App, key: &crate::pool::CrawlKey, error: String) {
    queue_ui(app, key, Some(error));
}
fn queue_ui(app: &App, key: &crate::pool::CrawlKey, failure: Option<String>) {
    let settings = app.settings_snapshot();
    let policy = Policy {
        on_success: settings.accounts.notify_on_success,
        on_failure: settings.accounts.notify_on_failure,
        account: settings.accounts.notify_account.clone(),
        to: settings.accounts.notify_to.clone(),
    };
    if policy.account.is_none() || policy.to.iter().all(|s| s.trim().is_empty()) {
        return;
    }
    if !app
        .ext::<State>()
        .sent
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(key.0.clone())
    {
        return;
    }
    let Some(path) = app.read_pool().path_of(key) else {
        return;
    };
    let app = app.clone();
    let key = key.clone();
    app.handle().spawn(async move {
        app.crawl_events().wait_barriers(&key).await;
        let result = tokio::task::spawn_blocking(move || {
            os_analyze::mail_completion(&path, None, false, false)
        })
        .await;
        match result {
            Ok(Ok(value)) => match serde_json::from_value::<Completion>(value) {
                Ok(mut completion) => {
                    if let Some(error) = failure {
                        completion.error = Some(error);
                    }
                    if let Err(e) = os_sched::mail::notify_with_context(
                        &app.config().paths.settings,
                        app.config().secrets.as_ref(),
                        &settings.network,
                        &policy,
                        &completion,
                        &os_sched::mail::DeliveryContext {
                            clock: app.config().clock.as_ref(),
                            ..Default::default()
                        },
                    )
                    .await
                    {
                        tracing::warn!("Abschlussmail fehlgeschlagen: {e}");
                    }
                }
                Err(_) => tracing::warn!("Abschlussmail-Daten ungültig"),
            },
            _ => tracing::warn!("Abschlussmail-Daten nicht lesbar"),
        }
    });
}

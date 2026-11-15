//! API-Rahmen (M2-I01); Quellen können sich unabhängig über ApiState registrieren.
use crate::runtime::error::{parse_args, to_json};
use crate::{
    dto::*,
    pool::CrawlKey,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{hooks::CompletionBarrier, lifecycle_bus::CrawlEvent, App, CallCtx},
};
use futures_util::future::BoxFuture;
use os_integrations::{
    credentials::{Credential, Credentials},
    http, linkmetrics, oauth,
    queue::Queue,
    ApiCtx, ApiError, ApiSource, UrlRef,
};
use os_model::CrawlConfig;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

const KINDS: [ApiKind; 7] = [
    ApiKind::Ga4,
    ApiKind::Gsc,
    ApiKind::Psi,
    ApiKind::Majestic,
    ApiKind::Ahrefs,
    ApiKind::Moz,
    ApiKind::Drive,
];
#[derive(Clone)]
struct Connection {
    attempt: u64,
    state: ApiConnState,
    account: Option<ApiAccount>,
    error: Option<CmdError>,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            attempt: 0,
            state: ApiConnState::Disconnected,
            account: None,
            error: None,
        }
    }
}
#[derive(Default)]
pub struct ApiState {
    attempt: std::sync::atomic::AtomicU64,
    credentials: OnceLock<Arc<Credentials>>,
    quota: OnceLock<Arc<os_integrations::quota::QuotaLedger>>,
    connections: Mutex<BTreeMap<String, Connection>>,
    sources: Mutex<BTreeMap<String, Arc<dyn ApiSource>>>,
    queues: Mutex<BTreeMap<(String, String), Arc<Queue>>>,
    fallback: Mutex<CrawlConfig>,
    handled_changes: Mutex<std::collections::BTreeSet<(String, String)>>,
    /// Optionaler Browser-Einstieg für headless Hosts und lokale OAuth-Tests.
    browser: Mutex<Option<Arc<BrowserOpener>>>,
    /// Nur für freigegebene Debug-/Testendpunkte wirksam.
    endpoints: Mutex<Option<oauth::Endpoints>>,
    ahrefs_endpoints: Mutex<Option<linkmetrics::oauth::Endpoints>>,
}
type BrowserOpener = dyn Fn(&str) -> Result<(), ApiError> + Send + Sync;
impl ApiState {
    pub fn register_source(&self, source: Arc<dyn ApiSource>) {
        self.sources
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(source.kind().to_string(), source);
    }
    pub fn set_oauth_browser(&self, open: Arc<BrowserOpener>) {
        *self.browser.lock().unwrap_or_else(|p| p.into_inner()) = Some(open);
    }
    pub fn set_oauth_endpoints(&self, endpoints: oauth::Endpoints) {
        if cfg!(debug_assertions) {
            *self.endpoints.lock().unwrap_or_else(|p| p.into_inner()) = Some(endpoints);
        }
    }
    /// Lokaler Ahrefs-OAuth-Mock; Google-Endpunkte bleiben unabhängig.
    pub fn set_ahrefs_oauth_endpoints(&self, endpoints: linkmetrics::oauth::Endpoints) {
        if cfg!(debug_assertions) {
            *self
                .ahrefs_endpoints
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = Some(endpoints);
        }
    }
    fn quota(&self, app: &App) -> Arc<os_integrations::quota::QuotaLedger> {
        self.quota
            .get_or_init(|| {
                Arc::new(os_integrations::quota::QuotaLedger::new(
                    app.config().paths.settings.join("api-quotas.json"),
                ))
            })
            .clone()
    }
    fn credentials<'a>(&'a self, app: &App) -> &'a Arc<Credentials> {
        self.credentials
            .get_or_init(|| Arc::new(Credentials::new(app.config().secrets.clone())))
    }
    fn connection(&self, kind: ApiKind) -> Connection {
        self.connections
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(name(kind))
            .cloned()
            .unwrap_or_default()
    }
    fn set_connection(&self, kind: ApiKind, connection: Connection) {
        self.connections
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(name(kind).into(), connection);
    }
    fn queue(&self, key: &CrawlKey, kind: ApiKind) -> Arc<Queue> {
        self.queues
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry((key.0.clone(), name(kind).into()))
            .or_default()
            .clone()
    }
}
fn name(k: ApiKind) -> &'static str {
    match k {
        ApiKind::Ga4 => "ga4",
        ApiKind::Gsc => "gsc",
        ApiKind::Psi => "psi",
        ApiKind::Majestic => "majestic",
        ApiKind::Ahrefs => "ahrefs",
        ApiKind::Moz => "moz",
        ApiKind::Drive => "drive",
    }
}
fn model(k: ApiKind) -> os_model::ApiKind {
    match k {
        ApiKind::Ga4 => os_model::ApiKind::Ga4,
        ApiKind::Gsc => os_model::ApiKind::Gsc,
        ApiKind::Psi => os_model::ApiKind::Psi,
        ApiKind::Majestic => os_model::ApiKind::Majestic,
        ApiKind::Ahrefs => os_model::ApiKind::Ahrefs,
        ApiKind::Moz => os_model::ApiKind::Moz,
        ApiKind::Drive => os_model::ApiKind::Drive,
    }
}
fn error(e: ApiError) -> CmdError {
    match e {
        ApiError::NotImplemented(_) => {
            CmdError::new(ErrorCode::Unsupported, "err-api-source-unavailable")
        }
        ApiError::Provider {
            status: 401 | 403, ..
        }
        | ApiError::Unauthorized(_) => CmdError::new(ErrorCode::Permission, "err-api-reconnect"),
        // INTEG-132: Quelle Lokal ohne Lighthouse-Laufzeit hat einen eigenen Status.
        ApiError::NotConnected(ref m) if m == os_integrations::psi::local::RUNTIME_MISSING => {
            CmdError::new(ErrorCode::Permission, "err-api-lighthouse-missing")
        }
        ApiError::NotConnected(_) => CmdError::new(ErrorCode::Permission, "err-api-disconnected"),
        _ => CmdError::new(ErrorCode::Io, "err-api-request"),
    }
    .with_detail(match e.http_status() {
        Some(code) if !matches!(e, ApiError::Provider { .. }) => {
            format!("HTTP {code}: {}", e.public_message())
        }
        Some(_) => e.public_message(),
        None => e.public_message(),
    })
}
fn cfg(app: &App, ctx: &CallCtx) -> CrawlConfig {
    app.sessions()
        .current(&ctx.window)
        .map(|s| (*s.config).clone())
        .unwrap_or_else(|| {
            app.ext::<ApiState>()
                .fallback
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
        })
}
fn api_ctx(app: &App, mut cfg: CrawlConfig) -> ApiCtx {
    let state = app.ext::<ApiState>();
    for kind in KINDS {
        if let Some(account) = state.connection(kind).account {
            let id = Some(account.account_id);
            match kind {
                ApiKind::Ga4 => cfg.api.ga4.account = id,
                ApiKind::Gsc => cfg.api.gsc.account = id,
                ApiKind::Psi => cfg.api.psi.account = id,
                ApiKind::Majestic => cfg.api.majestic.account = id,
                ApiKind::Ahrefs => cfg.api.ahrefs.account = id,
                ApiKind::Moz => cfg.api.moz.account = id,
                _ => {}
            }
        }
    }
    ApiCtx {
        cfg,
        secrets: app.ext::<ApiState>().credentials(app).clone(),
        network: app.settings_snapshot().network,
        allow_api_override: false,
        quota_ledger: Some(app.config().paths.settings.join("api-quotas.json")),
    }
}
fn account(kind: ApiKind, c: &Credential) -> ApiAccount {
    ApiAccount {
        kind,
        account: c.label.clone(),
        scopes: c.scopes.clone(),
        expires_at: c.expires_at.map(|t| t.to_db()),
        account_id: c.account_id.clone(),
        key_suffix: c
            .api_key
            .as_deref()
            .map(os_integrations::credentials::key_suffix),
    }
}
fn endpoints(state: &ApiState) -> oauth::Endpoints {
    state
        .endpoints
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .unwrap_or_default()
}
fn client_id(cfg: &CrawlConfig, kind: ApiKind) -> Option<&str> {
    match kind {
        ApiKind::Drive => cfg.api.drive.oauth_client_id.as_deref(),
        ApiKind::Ga4 => cfg.api.ga4.oauth_client_id.as_deref(),
        ApiKind::Gsc => cfg.api.gsc.oauth_client_id.as_deref(),
        _ => None,
    }
}

async fn probe(
    app: &App,
    kind: ApiKind,
    config: CrawlConfig,
    credential: &Credential,
) -> Result<Credential, ApiError> {
    let source = app
        .ext::<ApiState>()
        .sources
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(name(kind))
        .cloned();
    let ctx = api_ctx(app, config);
    if kind == ApiKind::Drive {
        os_integrations::drive::DriveClient::new(
            &ctx,
            &credential.account_id,
            app.config().clock.now(),
        )?
        .test_connection()
        .await?;
        Ok(credential.clone())
    } else if let Some(source) = source {
        source.test_connection(&ctx, credential).await
    } else if kind == ApiKind::Psi && os_integrations::psi::is_local(&ctx.cfg) {
        // INTEG-132: lokal prüft „Verbinden“ die Lighthouse-Laufzeit, nicht die PSI-API.
        os_integrations::psi::source_for(&ctx.cfg, app.config().clock.clone())
            .test_connection(&ctx, credential)
            .await
    } else {
        http::probe(
            model(kind),
            &ctx,
            credential
                .api_key
                .as_deref()
                .or(credential.access_token.as_deref()),
        )
        .await?;
        Ok(credential.clone())
    }
}
fn quota_limits(config: &CrawlConfig, kind: ApiKind) -> (String, Option<u64>, Option<u64>) {
    let value = settings_value(config, kind);
    let provider = match kind {
        ApiKind::Psi => Some(25000),
        ApiKind::Gsc if config.api.gsc.url_inspection => Some(2000),
        _ => None,
    };
    let custom = value
        .pointer("/limits/quota_limit")
        .and_then(|v| v.as_u64());

    (
        value["property"]
            .as_str()
            .or(value["site"].as_str())
            .unwrap_or_default()
            .to_string(),
        provider,
        custom,
    )
}
async fn local_quotas(
    app: &App,
    config: &CrawlConfig,
    kind: ApiKind,
    crawl: Option<String>,
) -> Result<Vec<QuotaDto>, CmdError> {
    let (property, provider, custom) = quota_limits(config, kind);
    if provider.is_none() && custom.is_none() {
        return Ok(vec![]);
    }
    let ledger = app.ext::<ApiState>().quota(app);
    let now = app.config().clock.now();
    app.write_queue()
        .submit_admin(move || {
            let mut quotas = vec![];
            if let Some(limit) = custom {
                let left = if let Some(crawl) = crawl {
                    ledger
                        .remaining_period(name(kind), &property, &format!("crawl:{crawl}"), limit)
                        .map_err(error)?
                } else {
                    limit
                };
                quotas.push(QuotaDto {
                    used: limit.saturating_sub(left),
                    limit: Some(limit),
                    resets_at: None,
                    dimension: Some("urlsPerCrawl".into()),
                });
            }
            if let Some(limit) = provider {
                let left = ledger
                    .remaining(name(kind), &property, now, limit)
                    .map_err(error)?;
                quotas.push(QuotaDto {
                    used: limit.saturating_sub(left),
                    limit: Some(limit),
                    resets_at: Some(os_integrations::quota::next_reset(now).to_db()),
                    dimension: Some("urlsPerDay".into()),
                });
            }
            Ok(quotas)
        })
        .await
}
fn quota_left(quotas: &[QuotaDto]) -> Option<u64> {
    quotas
        .iter()
        .filter_map(|q| q.limit.map(|limit| limit.saturating_sub(q.used)))
        .min()
}

async fn authorize_ahrefs(
    app: &App,
    state: &ApiState,
    cfg: &CrawlConfig,
) -> Result<Credential, CmdError> {
    let id = cfg
        .api
        .ahrefs
        .options
        .oauth_client_id
        .as_deref()
        .ok_or_else(|| CmdError::invalid_input("err-api-client-id", "oauth_client_id"))?;
    let endpoints = state
        .ahrefs_endpoints
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .unwrap_or_default();
    let authorization = linkmetrics::oauth::Authorization::begin(id, &endpoints)
        .await
        .map_err(error)?;
    let open = state
        .browser
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if let Some(open) = open {
        open(authorization.url.as_str()).map_err(error)?;
    } else {
        authorization.open_browser().map_err(error)?;
    }
    authorization
        .finish(
            &http::client_for(&api_ctx(app, cfg.clone())).map_err(error)?,
            id,
            None,
            &endpoints,
            app.config().clock.clone(),
        )
        .await
        .map_err(error)
}

async fn connect_now(app: App, ctx: CallCtx, args: ApiConnectArgs) -> Result<ApiAccount, CmdError> {
    let state = app.ext::<ApiState>();
    let cfg = cfg(&app, &ctx);
    let attempt = state
        .attempt
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    {
        let mut connections = state.connections.lock().unwrap_or_else(|p| p.into_inner());
        if connections
            .get(name(args.kind))
            .is_some_and(|c| c.state == ApiConnState::Connecting)
        {
            return Err(CmdError::new(ErrorCode::Busy, "err-api-connecting"));
        }
        connections.insert(
            name(args.kind).into(),
            Connection {
                attempt,
                state: ApiConnState::Connecting,
                ..Default::default()
            },
        );
    }
    let _connecting = ConnectingGuard {
        state: state.clone(),
        kind: args.kind,
        attempt,
    };
    let result: Result<ApiAccount, CmdError> = async {
        let workspace = (args.kind == ApiKind::Ahrefs)
            .then_some(cfg.api.ahrefs.options.workspace.as_ref())
            .flatten();
        let saved = if let Some(id) = args.account_id.as_ref().or(workspace) {
            Some(
                state
                    .credentials(&app)
                    .list(model(args.kind))
                    .map_err(error)?
                    .into_iter()
                    .find(|c| &c.account_id == id)
                    .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-api-account"))?,
            )
        } else {
            None
        };
        let saved = saved.filter(|credential| {
            args.kind != ApiKind::Ahrefs
                || credential
                    .expires_at
                    .is_none_or(|at| at > app.config().clock.now())
        });
        let reused_ahrefs = args.kind == ApiKind::Ahrefs && saved.is_some();
        let mut credential = if let Some(credential) = saved {
            credential
        } else if args.kind == ApiKind::Drive {
            let id = client_id(&cfg, args.kind)
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| {
                    CmdError::invalid_input("err-api-client-id", "oauth_client_id")
                        .with_detail("Eigene Google-Client-ID erforderlich")
                })?;
            let context = api_ctx(&app, cfg.clone());
            let endpoints = endpoints(&state);
            let open = state
                .browser
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            let credentials = state.credentials(&app);
            if let Some(open) = open {
                os_integrations::drive::connect(
                    &context,
                    credentials,
                    id,
                    args.client_secret.as_deref(),
                    &endpoints,
                    app.config().clock.clone(),
                    args.remember.unwrap_or(true),
                    move |url| open(url),
                )
                .await
                .map_err(error)?
            } else {
                os_integrations::drive::connect_in_browser(
                    &context,
                    credentials,
                    id,
                    args.client_secret.as_deref(),
                    &endpoints,
                    app.config().clock.clone(),
                    args.remember.unwrap_or(true),
                )
                .await
                .map_err(error)?
            }
        } else if matches!(args.kind, ApiKind::Ga4 | ApiKind::Gsc) {
            let id = client_id(&cfg, args.kind)
                .ok_or_else(|| CmdError::invalid_input("err-api-client-id", "oauth_client_id"))?;
            let endpoints = endpoints(&state);
            let authorization = oauth::Authorization::begin(id, model(args.kind), &endpoints)
                .await
                .map_err(error)?;
            let open = state
                .browser
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            if let Some(open) = open {
                open(authorization.url.as_str()).map_err(error)?;
            } else {
                authorization.open_browser().map_err(error)?;
            }
            authorization
                .finish(
                    &http::client_for(&api_ctx(&app, cfg.clone())).map_err(error)?,
                    id,
                    args.client_secret.as_deref(),
                    &endpoints,
                    app.config().clock.clone(),
                )
                .await
                .map_err(error)?
        } else if args.kind == ApiKind::Ahrefs {
            authorize_ahrefs(&app, &state, &cfg).await?
        } else if args.kind == ApiKind::Psi && os_integrations::psi::is_local(&cfg) {
            // INTEG-130/132: Quelle Lokal braucht keinen Schlüssel; ein Remote-Konto bleibt unberührt.
            Credential {
                account_id: "local".into(),
                label: name(args.kind).into(),
                api_key: None,
                access_token: None,
                refresh_token: None,
                expires_at: None,
                client_secret: None,
                scopes: vec![],
            }
        } else {
            let key = args
                .api_key
                .filter(|s| !s.is_empty())
                .ok_or_else(|| CmdError::invalid_input("err-api-key", "apiKey"))?;
            Credential {
                account_id: "default".into(),
                label: name(args.kind).into(),
                api_key: Some(key),
                access_token: None,
                refresh_token: None,
                expires_at: None,
                client_secret: None,
                scopes: vec![],
            }
        };
        if args.kind == ApiKind::Drive {
            let id = client_id(&cfg, args.kind)
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| {
                    CmdError::invalid_input("err-api-client-id", "oauth_client_id")
                        .with_detail("Eigene Google-Client-ID erforderlich")
                })?;
            os_integrations::drive::refresh_account(
                &api_ctx(&app, cfg.clone()),
                state.credentials(&app),
                &credential.account_id,
                id,
                &endpoints(&state),
                app.config().clock.as_ref(),
            )
            .await
            .map_err(error)?;
            credential = os_integrations::credentials::read_account(
                state.credentials(&app).as_ref(),
                os_model::ApiKind::Drive,
                Some(&credential.account_id),
            )
            .map_err(error)?;
        } else if matches!(args.kind, ApiKind::Ga4 | ApiKind::Gsc) {
            oauth::refresh(
                &mut credential,
                &http::client_for(&api_ctx(&app, cfg.clone())).map_err(error)?,
                client_id(&cfg, args.kind).unwrap_or_default(),
                &endpoints(&state),
                app.config().clock.as_ref(),
            )
            .await
            .map_err(error)?;
        }
        let mut verified = tokio::time::timeout(
            Duration::from_secs(30),
            probe(&app, args.kind, cfg.clone(), &credential),
        )
        .await
        .map_err(|_| {
            CmdError::new(ErrorCode::Timeout, "err-api-connect-timeout").with_arg("seconds", 30)
        })?;
        if reused_ahrefs && matches!(verified, Err(ApiError::Unauthorized(_))) {
            credential = authorize_ahrefs(&app, &state, &cfg).await?;
            verified = tokio::time::timeout(
                Duration::from_secs(30),
                probe(&app, args.kind, cfg.clone(), &credential),
            )
            .await
            .map_err(|_| {
                CmdError::new(ErrorCode::Timeout, "err-api-connect-timeout").with_arg("seconds", 30)
            })?;
        }
        credential = verified.map_err(error)?;
        if state.connection(args.kind).attempt != attempt {
            return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
        }
        let remember = args.remember.unwrap_or(true);
        let persisted = state
            .credentials(&app)
            .put(model(args.kind), credential.clone(), remember)
            .map_err(error)?;
        if remember && !persisted {
            app.push_notice(Notice {
                level: NoticeLevel::Warning,
                code: "notice-api-session-only".into(),
                args: Default::default(),
            });
        }
        Ok(account(args.kind, &credential))
    }
    .await;
    if state.connection(args.kind).attempt == attempt {
        match &result {
            Ok(account) => state.set_connection(
                args.kind,
                Connection {
                    attempt,
                    state: ApiConnState::Connected,
                    account: Some(account.clone()),
                    error: None,
                },
            ),
            Err(err) => state.set_connection(
                args.kind,
                Connection {
                    attempt,
                    state: ApiConnState::Error,
                    error: Some(err.clone()),
                    account: None,
                },
            ),
        }
    }
    result
}
struct ConnectingGuard {
    state: Arc<ApiState>,
    kind: ApiKind,
    attempt: u64,
}
impl Drop for ConnectingGuard {
    fn drop(&mut self) {
        let mut connections = self
            .state
            .connections
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if connections
            .get(name(self.kind))
            .is_some_and(|c| c.attempt == self.attempt && c.state == ApiConnState::Connecting)
        {
            connections.insert(name(self.kind).into(), Connection::default());
        }
    }
}

fn connect<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ApiConnectArgs = parse_args(args)?;
        to_json(&spawn_long(
            app,
            "op-api-connect",
            connect_now(app.clone(), ctx, args),
        ))
    })
}
// Keep the review-authorized Drive branch separate from existing provider behavior.
#[expect(
    clippy::if_same_then_else,
    reason = "strictly additive Drive OAuth branch (K2-61)"
)]
fn list<'a>(app: &'a App, ctx: CallCtx, _args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let state = app.ext::<ApiState>();
        let config = cfg(app, &ctx);
        let mut rows = vec![];
        for kind in KINDS {
            let c = state.connection(kind);
            let mut local = local_quotas(
                app,
                &config,
                kind,
                app.sessions().current(&ctx.window).map(|s| s.key.0),
            )
            .await?
            .into_iter();
            let quota = local.next();
            let mut dimensions: Vec<_> = local.collect();
            let provider_quotas = state
                .sources
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(name(kind))
                .map(|source| source.quotas())
                .unwrap_or_default();
            rows.push(ApiSourceInfo {
                kind,
                label_key: format!("api-{}", name(kind)),
                auth: if kind == ApiKind::Drive {
                    ApiAuthKind::Oauth
                } else if matches!(kind, ApiKind::Ga4 | ApiKind::Gsc) {
                    ApiAuthKind::Oauth
                } else {
                    ApiAuthKind::ApiKey
                },
                connected: c.state == ApiConnState::Connected,
                account: c.account.as_ref().map(|a| a.account.clone()),
                quota,
                quota_dimensions: {
                    dimensions.extend(provider_quotas.into_iter().map(|q| QuotaDto {
                        used: q.used,
                        limit: q.limit,
                        resets_at: q.resets_at.map(|at| at.to_db()),
                        dimension: Some(q.dimension),
                    }));
                    if dimensions.is_empty() {
                        None
                    } else {
                        Some(dimensions)
                    }
                },
                auto_connect: app
                    .settings_snapshot()
                    .accounts
                    .auto_connect
                    .iter()
                    .any(|k| k == name(kind)),
                state: c.state,
                error: c.error,
                accounts: state
                    .credentials(app)
                    .list(model(kind))
                    .unwrap_or_default()
                    .iter()
                    .map(|c| account(kind, c))
                    .collect(),
            });
        }
        to_json(&rows)
    })
}
fn disconnect<'a>(
    app: &'a App,
    _ctx: CallCtx,
    args: serde_json::Value,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ApiKindArgs = parse_args(args)?;
        app.ext::<ApiState>()
            .set_connection(args.kind, Connection::default());
        Ok(serde_json::Value::Null)
    })
}
fn remove_account<'a>(
    app: &'a App,
    ctx: CallCtx,
    args: serde_json::Value,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ApiAccountArgs = parse_args(args)?;
        let state = app.ext::<ApiState>();
        let credential = state
            .credentials(app)
            .list(model(args.kind))
            .map_err(error)?
            .into_iter()
            .find(|c| c.account_id == args.account_id);
        state
            .credentials(app)
            .remove(model(args.kind), &args.account_id)
            .map_err(error)?;
        if state
            .connection(args.kind)
            .account
            .is_some_and(|c| c.account_id == args.account_id)
        {
            state.set_connection(args.kind, Connection::default());
        }
        if let Some(credential) = credential.filter(|c| c.refresh_token.is_some()) {
            if oauth::revoke(
                &credential,
                &http::client_for(&api_ctx(app, cfg(app, &ctx))).map_err(error)?,
                &endpoints(&state),
            )
            .await
            .is_err()
            {
                app.push_notice(Notice {
                    level: NoticeLevel::Warning,
                    code: "notice-api-revoke-failed".into(),
                    args: Default::default(),
                });
            }
        }
        Ok(serde_json::Value::Null)
    })
}
fn test_connection<'a>(
    app: &'a App,
    ctx: CallCtx,
    args: serde_json::Value,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ApiKindArgs = parse_args(args)?;
        let state = app.ext::<ApiState>();
        let connection = state.connection(args.kind);
        let credential = state
            .credentials(app)
            .list(model(args.kind))
            .map_err(error)?
            .into_iter()
            .find(|c| {
                connection
                    .account
                    .as_ref()
                    .is_some_and(|a| a.account_id == c.account_id)
            });
        let result = if let Some(c) = credential {
            tokio::time::timeout(
                Duration::from_secs(10),
                probe(app, args.kind, cfg(app, &ctx), &c),
            )
            .await
            .map_err(|_| CmdError::new(ErrorCode::Timeout, "err-api-test-timeout"))
            .and_then(|r| r.map_err(error))
        } else {
            Err(CmdError::new(ErrorCode::Permission, "err-api-disconnected"))
        };
        to_json(&ApiTestResult {
            ok: result.is_ok(),
            account: connection.account.map(|a| a.account),
            quota_remaining: quota_left(
                &local_quotas(
                    app,
                    &cfg(app, &ctx),
                    args.kind,
                    app.sessions().current(&ctx.window).map(|s| s.key.0),
                )
                .await?,
            ),
            error: result.err(),
        })
    })
}

fn settings_value(cfg: &CrawlConfig, kind: ApiKind) -> serde_json::Value {
    serde_json::to_value(&cfg.api).expect("API config")[name(kind)].clone()
}
fn get_settings<'a>(
    app: &'a App,
    ctx: CallCtx,
    args: serde_json::Value,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ApiKindArgs = parse_args(args)?;
        to_json(&ApiSourceSettings {
            kind: args.kind,
            values: settings_value(&cfg(app, &ctx), args.kind),
        })
    })
}
fn data_settings(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(object) = value.as_object_mut() {
        for key in [
            "enabled",
            "base_url",
            "limits",
            "account",
            "oauth_client_id",
        ] {
            object.remove(key);
        }
    }
    value
}
fn set_settings<'a>(
    app: &'a App,
    ctx: CallCtx,
    args: serde_json::Value,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ApiSetSettingsArgs = parse_args(args)?;
        if args.kind != args.settings.kind {
            return Err(CmdError::invalid_input("err-api-kind", "settings.kind"));
        }
        let old = cfg(app, &ctx);
        let mut serialized = serde_json::to_value(&old)
            .map_err(|_| CmdError::new(ErrorCode::Internal, "err-config"))?;
        // Geheimnisse dürfen auch über frei geformtes JSON niemals in ein Profil gelangen.
        reject_secrets(&args.settings.values)?;
        serialized["api"][name(args.kind)] = args.settings.values;
        let new: CrawlConfig = serde_json::from_value(serialized)
            .map_err(|_| CmdError::invalid_input("err-api-settings", "settings.values"))?;
        if !new.validate().errors.is_empty() {
            return Err(CmdError::invalid_input(
                "err-api-settings",
                "settings.values",
            ));
        }
        let changed = data_settings(settings_value(&old, args.kind))
            != data_settings(settings_value(&new, args.kind));
        let session = app.sessions().current(&ctx.window);
        let has_data = if let Some(s) = &session {
            let source = name(args.kind);
            app.read_pool()
                .run(s.key.clone(), move |file, _| {
                    os_store::queries::api::has_values(file.conn(), source)
                })
                .await?
        } else {
            false
        };
        let ask =
            changed && has_data && matches!(args.decision, None | Some(ApiSettingsDecision::Ask));
        if ask {
            return to_json(&ApiSettingsResult {
                invalidated: false,
                confirmation_required: true,
                data_stale: false,
            });
        }
        let remove = changed && has_data && args.decision == Some(ApiSettingsDecision::Reload);
        if let Some(s) = &session {
            if !s.writable {
                return Err(CmdError::new(ErrorCode::Permission, "err-crawl-readonly"));
            }
            let new_cfg = new.clone();
            let source = name(args.kind);
            app.write_queue()
                .submit_file(&s.key, move |file| {
                    let store = |e: rusqlite::Error| crate::runtime::error::rusqlite_error(&e);
                    // Configuration and invalidation in one commit (INTEG-010). IMMEDIATE takes the write lock
                    // within the busy timeout; a deferred upgrade would fail at once while the WAL merge after a
                    // pause holds it (CRAWL-126).
                    let tx = rusqlite::Transaction::new_unchecked(
                        file.conn(),
                        rusqlite::TransactionBehavior::Immediate,
                    )
                    .map_err(store)?;
                    file.set_config(&new_cfg)
                        .map_err(|e| crate::runtime::error::store_error(&e))?;
                    if changed {
                        os_store::queries::api::invalidate(file.conn(), source, remove)
                            .map_err(|e| crate::runtime::error::store_error(&e))?;
                    }
                    tx.commit().map_err(store)?;
                    Ok(())
                })
                .await?;
            app.sessions().update_config(&s.key, Arc::new(new.clone()));
            app.ext::<ApiState>()
                .handled_changes
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert((
                    s.key.0.clone(),
                    serde_json::to_string(&new.api).expect("api config"),
                ));
            app.crawl_events().emit(
                app,
                CrawlEvent::ConfigChanged {
                    key: s.key.clone(),
                    old: Arc::new(old),
                    new: Arc::new(new),
                },
            );
            if remove {
                spawn_long(
                    app,
                    "op-api-fetch",
                    fetch_now(
                        app.clone(),
                        ctx.clone(),
                        ApiFetchArgs {
                            kinds: Some(vec![args.kind]),
                            scope: ApiFetchScope::All,
                        },
                    ),
                );
            }
        } else {
            *app.ext::<ApiState>()
                .fallback
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = new;
        }
        to_json(&ApiSettingsResult {
            invalidated: changed && has_data,
            confirmation_required: false,
            data_stale: changed && has_data && !remove,
        })
    })
}
fn reject_secrets(value: &serde_json::Value) -> Result<(), CmdError> {
    match value {
        serde_json::Value::Object(o) => {
            for (key, value) in o {
                if [
                    "api_key",
                    "apikey",
                    "client_secret",
                    "clientsecret",
                    "access_token",
                    "refresh_token",
                    "password",
                    "authorization",
                ]
                .contains(&key.to_lowercase().as_str())
                {
                    return Err(CmdError::invalid_input(
                        "err-api-secret-in-settings",
                        "settings.values",
                    ));
                }
                reject_secrets(value)?;
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                reject_secrets(v)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn errors<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ApiErrorsArgs = parse_args(args)?;
        let s = app
            .sessions()
            .current(&ctx.window)
            .ok_or_else(|| CmdError::new(ErrorCode::NoCrawl, "err-no-crawl"))?;
        let rows = app
            .read_pool()
            .run(s.key, move |file, _| {
                os_store::queries::api::errors(
                    file.conn(),
                    name(args.kind),
                    args.offset,
                    args.limit,
                )
            })
            .await?;
        to_json(
            &rows
                .into_iter()
                .map(|r| ApiErrorRow {
                    at: r.at,
                    url: r.url,
                    status: r.code.parse().ok(),
                    message: r.message,
                })
                .collect::<Vec<_>>(),
        )
    })
}
fn status<'a>(app: &'a App, ctx: CallCtx, _args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let state = app.ext::<ApiState>();
        let session = app.sessions().current(&ctx.window);
        let stored = if let Some(s) = &session {
            app.read_pool()
                .run(s.key.clone(), |file, _| {
                    os_store::queries::api::statuses(file.conn())
                })
                .await?
        } else {
            vec![]
        };
        let mut rows = vec![];
        for kind in KINDS {
            let connected = state.connection(kind).state == ApiConnState::Connected;
            let stored = stored.iter().find(|s| s.source == name(kind));
            let live = session
                .as_ref()
                .and_then(|s| state.queue(&s.key, kind).status());
            if !connected && stored.is_none() && live.is_none() {
                continue;
            }
            let requested = live
                .as_ref()
                .map(|s| s.urls_total)
                .or(stored.map(|s| s.total))
                .unwrap_or(0);
            let completed = live
                .as_ref()
                .map(|s| s.urls_done)
                .or(stored.map(|s| s.done))
                .unwrap_or(0);
            let errors = stored
                .map(|s| s.failed)
                .unwrap_or_else(|| live.as_ref().map(|s| s.urls_failed).unwrap_or(0));
            let quotas = local_quotas(
                app,
                &cfg(app, &ctx),
                kind,
                app.sessions().current(&ctx.window).map(|s| s.key.0),
            )
            .await?;
            let provider = state
                .sources
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(name(kind))
                .map(|source| source.quotas())
                .unwrap_or_default();
            let provider_left = provider
                .iter()
                .filter_map(|q| q.limit.map(|limit| limit.saturating_sub(q.used)))
                .min();
            let local_left = quota_left(&quotas);
            let quota_left = match (provider_left, local_left) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let provider_reset = provider
                .iter()
                .filter_map(|q| q.resets_at)
                .min()
                .map(|at| at.to_db());
            rows.push(ApiRow {
                kind,
                label_key: format!("api-{}", name(kind)),
                connected,
                requested,
                received: completed.saturating_sub(errors),
                errors,
                quota_left,
                running: live.is_some_and(|s| s.state == os_integrations::ApiState::Fetching),
                completed,
                reset_at: quotas
                    .first()
                    .and_then(|q| q.resets_at.clone())
                    .or(provider_reset),
            });
        }
        to_json(&rows)
    })
}
async fn resolve_scope(
    app: &App,
    ctx: &CallCtx,
    scope: ApiFetchScope,
) -> Result<ApiFetchScope, CmdError> {
    if let ApiFetchScope::Selection {
        selection:
            Selection::All {
                view_id,
                version,
                except,
            },
    } = scope
    {
        let mut ids = std::collections::BTreeSet::new();
        let except = except
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let mut offset = 0;
        loop {
            let args = RowsArgs {
                view_id: view_id.clone(),
                version,
                offset,
                limit: 500,
                columns: vec!["Address".into()],
            };
            let rows: RowWindow = parse_args(
                app.dispatch(ctx.clone(), "view.rows", to_json(&args)?)
                    .await?,
            )?;
            if rows.version != version || rows.view_id != view_id {
                return Err(CmdError::new(ErrorCode::Stale, "err-view-stale"));
            }
            let count = rows.row_keys.len();
            ids.extend(rows.row_keys.into_iter().filter(|id| !except.contains(id)));
            offset += count as u64;
            if count < 500 {
                break;
            }
        }
        Ok(ApiFetchScope::Selection {
            selection: Selection::Ids {
                ids: ids.into_iter().collect(),
            },
        })
    } else {
        Ok(scope)
    }
}

struct LimitedSource {
    app: App,
    source: Arc<dyn ApiSource>,
    quota: Option<u64>,
    crawl_limit: Option<u64>,
    crawl_key: String,
    property: String,
    blocked: Mutex<Option<ApiError>>,
    token_lock: tokio::sync::Mutex<()>,
}
impl LimitedSource {
    async fn refresh_token(&self, ctx: &ApiCtx, force: bool) -> Result<(), ApiError> {
        let kind = match self.kind() {
            os_model::ApiKind::Ga4 => ApiKind::Ga4,
            os_model::ApiKind::Gsc => ApiKind::Gsc,
            _ => return Ok(()),
        };
        let _lock = self.token_lock.lock().await;
        let state = self.app.ext::<ApiState>();
        let id = state
            .connection(kind)
            .account
            .map(|a| a.account_id)
            .ok_or_else(|| ApiError::NotConnected(name(kind).into()))?;
        let mut credential = state
            .credentials(&self.app)
            .list(model(kind))?
            .into_iter()
            .find(|c| c.account_id == id)
            .ok_or_else(|| ApiError::NotConnected(name(kind).into()))?;
        let previous = (credential.access_token.clone(), credential.expires_at);
        if !force
            && credential
                .expires_at
                .is_some_and(|at| at > self.app.config().clock.now().plus(Duration::from_secs(60)))
        {
            return Ok(());
        }
        if force {
            credential.expires_at = None;
        }
        let result = oauth::refresh(
            &mut credential,
            &http::client_for(ctx)?,
            client_id(&ctx.cfg, kind).unwrap_or_default(),
            &endpoints(&state),
            self.app.config().clock.as_ref(),
        )
        .await;
        if let Err(e) = result {
            *self.blocked.lock().unwrap_or_else(|p| p.into_inner()) = Some(e.clone());
            state.set_connection(
                kind,
                Connection {
                    state: ApiConnState::Error,
                    error: Some(error(e.clone())),
                    account: None,
                    ..Default::default()
                },
            );
            return Err(e);
        }
        if previous == (credential.access_token.clone(), credential.expires_at) {
            return Ok(());
        }
        let remember = state.credentials(&self.app).is_persistent(model(kind), &id);
        state
            .credentials(&self.app)
            .put(model(kind), credential, remember)?;
        Ok(())
    }
}

impl ApiSource for LimitedSource {
    fn kind(&self) -> os_model::ApiKind {
        self.source.kind()
    }
    fn status(&self) -> os_integrations::ApiStatus {
        self.source.status()
    }
    fn fetch<'a>(
        &'a self,
        urls: &'a [UrlRef],
        ctx: &'a ApiCtx,
    ) -> os_integrations::BoxFuture<'a, Result<Vec<os_model::ApiValueWrite>, ApiError>> {
        Box::pin(async move {
            if let Some(error) = self
                .blocked
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
            {
                return Err(error);
            }
            let now = self.app.config().clock.now();
            if self.source.quotas().iter().any(|q| {
                q.limit.is_some_and(|limit| q.used >= limit)
                    && q.resets_at.is_none_or(|reset| reset > now)
            }) {
                return Err(ApiError::QuotaExceeded(
                    "Anbieterkontingent erschöpft".into(),
                ));
            }
            if self.quota.is_some() || self.crawl_limit.is_some() {
                let mut limits = vec![];
                let kind = self.kind().to_string();
                if let Some(limit) = self.quota {
                    limits.push((
                        kind.clone(),
                        self.property.clone(),
                        os_integrations::queue::pacific_day(now),
                        limit,
                    ));
                }
                if let Some(limit) = self.crawl_limit {
                    limits.push((
                        kind,
                        self.property.clone(),
                        format!("crawl:{}", self.crawl_key),
                        limit,
                    ));
                }
                let ledger = self.app.ext::<ApiState>().quota(&self.app);
                let allowed = self
                    .app
                    .write_queue()
                    .submit_admin(move || ledger.reserve_limits_at(&limits, now).map_err(error))
                    .await
                    .map_err(|_| ApiError::Transport("Kontingent nicht speicherbar".into()))?;
                if !allowed {
                    return Err(ApiError::QuotaExceeded("Quota exceeded".into()));
                }
            }
            self.refresh_token(ctx, false).await?;
            let mut result = self.source.fetch(urls, ctx).await;
            if matches!(result, Err(ApiError::Unauthorized(_)))
                && matches!(self.kind(), os_model::ApiKind::Ga4 | os_model::ApiKind::Gsc)
            {
                self.refresh_token(ctx, true).await?;
                result = self.source.fetch(urls, ctx).await;
            }
            if self.kind() == os_model::ApiKind::Ahrefs
                && matches!(result, Err(ApiError::Unauthorized(_)))
            {
                // Ahrefs hat keine Refresh-Tokens: nur diese Integration bis zur Neuanmeldung stoppen.
                if let Err(ref cause) = result {
                    *self.blocked.lock().unwrap_or_else(|p| p.into_inner()) = Some(cause.clone());
                    self.app.ext::<ApiState>().set_connection(
                        ApiKind::Ahrefs,
                        Connection {
                            state: ApiConnState::Error,
                            error: Some(error(cause.clone())),
                            ..Default::default()
                        },
                    );
                }
            }
            if matches!(result, Err(ApiError::QuotaExceeded(_))) {
                *self.blocked.lock().unwrap_or_else(|p| p.into_inner()) =
                    result.as_ref().err().cloned();
                if let Some(limit) = self.quota {
                    let ledger = self.app.ext::<ApiState>().quota(&self.app);
                    let property = self.property.clone();
                    let kind = self.kind().to_string();
                    let now = self.app.config().clock.now();
                    self.app
                        .write_queue()
                        .submit_admin(move || {
                            ledger.exhaust(&kind, &property, now, limit).map_err(error)
                        })
                        .await
                        .map_err(|_| ApiError::Transport("Kontingent nicht speicherbar".into()))?;
                }
            }
            result
        })
    }
}
async fn fetch_now(
    app: App,
    ctx: CallCtx,
    args: ApiFetchArgs,
) -> Result<ApiFetchSummary, CmdError> {
    let session = app
        .sessions()
        .current(&ctx.window)
        .ok_or_else(|| CmdError::new(ErrorCode::NoCrawl, "err-no-crawl"))?;
    if !session.writable {
        return Err(CmdError::new(ErrorCode::Permission, "err-crawl-readonly"));
    }
    let state = app.ext::<ApiState>();
    let kinds = args.kinds.unwrap_or_else(|| {
        KINDS
            .into_iter()
            .filter(|k| state.connection(*k).state == ApiConnState::Connected)
            .collect()
    });
    if matches!(args.scope, ApiFetchScope::Selection { .. })
        && kinds
            .iter()
            .any(|kind| matches!(kind, ApiKind::Ga4 | ApiKind::Gsc))
    {
        return Err(CmdError::invalid_input("err-api-report-selection", "scope"));
    }
    // Erst alle Quellen auflösen, damit Unsupported keine teilweise ausgeführte Operation hinterlässt.
    let mut sources = vec![];
    for kind in kinds {
        if state.connection(kind).state != ApiConnState::Connected {
            return Err(CmdError::new(ErrorCode::Permission, "err-api-disconnected"));
        }
        let source = state
            .sources
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(name(kind))
            .cloned()
            .ok_or_else(|| {
                CmdError::unsupported(
                    "api.fetch",
                    match kind {
                        ApiKind::Ga4 => "M2-I02",
                        ApiKind::Gsc => "M2-I03",
                        ApiKind::Psi => "M2-I04",
                        _ => "M2-I05",
                    },
                )
            })?;
        sources.push((kind, source));
    }
    let scope = resolve_scope(&app, &ctx, args.scope).await?;
    let mut jobs = vec![];
    for (kind, source) in sources {
        let app = app.clone();
        let session = session.clone();
        let scope = scope.clone();
        let state = state.clone();
        jobs.push(async move {
            let missing = matches!(scope, ApiFetchScope::Missing);
            let mut urls = app
                .read_pool()
                .run(session.key.clone(), move |file, _| {
                    if linkmetrics::is_linkmetrics(model(kind)) {
                        let file_config = file.config()?;
                        let include_external = match kind {
                            ApiKind::Majestic => &file_config.api.majestic,
                            ApiKind::Ahrefs => &file_config.api.ahrefs,
                            _ => &file_config.api.moz,
                        }
                        .options
                        .include_external;
                        os_store::queries::api::linkmetrics_urls(
                            file.conn(),
                            name(kind),
                            missing,
                            include_external,
                        )
                    } else {
                        os_store::queries::api::urls(file.conn(), name(kind), missing)
                    }
                })
                .await?;
            if let ApiFetchScope::Selection {
                selection: Selection::Ids { ids },
            } = &scope
            {
                urls.retain(|(id, _)| ids.contains(&id.0));
            }
            let urls: Vec<_> = urls
                .into_iter()
                .map(|(id, url)| UrlRef {
                    url_id: Some(id),
                    url,
                })
                .collect();
            let value = settings_value(&session.config, kind);
            let (_, quota, crawl_limit) = quota_limits(&session.config, kind);
            let limits: os_model::config::ApiLimitsConfig =
                serde_json::from_value(value["limits"].clone()).unwrap_or_default();
            let property = value["property"]
                .as_str()
                .or(value["site"].as_str())
                .unwrap_or_default()
                .to_string();
            let source = if linkmetrics::is_linkmetrics(model(kind)) {
                let context = api_ctx(&app, (*session.config).clone());
                let estimate = linkmetrics::estimate_cost(
                    model(kind),
                    &urls,
                    linkmetrics::config(&context, model(kind)),
                )
                .map_err(error)?;
                if source.quotas().iter().any(|q| {
                    q.limit
                        .is_some_and(|limit| estimate > limit.saturating_sub(q.used))
                }) {
                    app.push_notice(Notice {
                        level: NoticeLevel::Warning,
                        code: "notice-api-units-insufficient".into(),
                        args: [("estimated".into(), serde_json::json!(estimate))].into(),
                    });
                }
                linkmetrics::batch_source(source, urls.clone())
            } else {
                source
            };
            let source = Arc::new(LimitedSource {
                app: app.clone(),
                source,
                quota,
                crawl_limit,
                crawl_key: session.key.0.clone(),
                property,
                blocked: Default::default(),
                token_lock: Default::default(),
            });
            let queue = state.queue(&session.key, kind);
            // Ein expliziter Abruf ist auch während einer Crawl-Pause zulässig.
            queue.resume();
            let requested = urls.len() as u64;
            let counts = Arc::new(Mutex::new((0_u64, 0_u64)));
            queue
                .run_with(
                    source,
                    &urls,
                    &api_ctx(&app, (*session.config).clone()),
                    limits.concurrency as usize,
                    |url, outcome, status| {
                        let app = app.clone();
                        let key = session.key.clone();
                        let now = app.config().clock.now();
                        let counts = counts.clone();
                        async move {
                            let success = outcome.is_ok();
                            app.write_queue()
                                .submit(key, move |conn| {
                                    match outcome {
                                        Ok(values) => {
                                            let successful = url
                                                .url_id
                                                .map(|id| vec![(id, url.url.clone())])
                                                .unwrap_or_default();
                                            os_store::queries::api::replace_values(
                                                conn,
                                                name(kind),
                                                &successful,
                                                &values,
                                            )?;
                                        }
                                        Err(e) => os_store::queries::api::record_error(
                                            conn,
                                            name(kind),
                                            &url.url,
                                            &e.http_status()
                                                .map(|code| code.to_string())
                                                .unwrap_or_default(),
                                            &e.public_message(),
                                            now,
                                        )?,
                                    }
                                    let failed =
                                        os_store::queries::api::error_count(conn, name(kind))?;
                                    os_store::queries::api::set_status(
                                        conn,
                                        &os_store::queries::api::Status {
                                            source: name(kind).into(),
                                            state: if status.state
                                                == os_integrations::ApiState::Idle
                                            {
                                                "idle"
                                            } else {
                                                "fetching"
                                            }
                                            .into(),
                                            total: status.urls_total,
                                            done: status.urls_done,
                                            failed,
                                            detail: None,
                                        },
                                    )
                                })
                                .await
                                .map_err(|_| {
                                    ApiError::Transport("API-Ergebnis nicht speicherbar".into())
                                })?;
                            let mut counts = counts.lock().unwrap_or_else(|p| p.into_inner());
                            if success {
                                counts.0 += 1;
                            } else {
                                counts.1 += 1;
                            }
                            Ok(())
                        }
                    },
                )
                .await
                .map_err(error)?;
            let (fetched, failed) = *counts.lock().unwrap_or_else(|p| p.into_inner());
            let local =
                local_quotas(&app, &session.config, kind, Some(session.key.0.clone())).await?;
            let provider_left = state
                .sources
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(name(kind))
                .into_iter()
                .flat_map(|source| source.quotas())
                .filter_map(|q| q.limit.map(|limit| limit.saturating_sub(q.used)))
                .min();
            let remaining = [quota_left(&local), provider_left]
                .into_iter()
                .flatten()
                .min();
            Ok::<_, CmdError>((requested, fetched, failed, remaining))
        });
    }
    let mut summary = ApiFetchSummary {
        requested: 0,
        fetched: 0,
        failed: 0,
        quota_left: None,
    };
    for result in futures_util::future::join_all(jobs).await {
        let (requested, fetched, failed, remaining) = result?;
        summary.quota_left = [summary.quota_left, remaining].into_iter().flatten().min();
        summary.requested += requested;
        summary.fetched += fetched;
        summary.failed += failed;
    }
    Ok(summary)
}
fn fetch<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ApiFetchArgs = parse_args(args)?;
        to_json(&spawn_long(
            app,
            "op-api-fetch",
            fetch_now(app.clone(), ctx, args),
        ))
    })
}
fn spawn_long<F, T>(app: &App, name: &'static str, future: F) -> OpHandle
where
    F: std::future::Future<Output = Result<T, CmdError>> + Send + 'static,
    T: serde::Serialize,
{
    app.ops().spawn(name, true, move |cancel, _| {
        Box::pin(async move {
            tokio::select! {
                result = future => to_json(&result?),
                _ = cancelled(cancel) => Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled")),
            }
        })
    })
}

async fn cancelled(cancel: os_analyze::analysis::CancelToken) {
    while !cancel.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
struct Barrier(Arc<ApiState>);
impl CompletionBarrier for Barrier {
    fn wait(&self, key: CrawlKey) -> BoxFuture<'static, ()> {
        let state = self.0.clone();
        Box::pin(async move {
            let queues: Vec<_> = state
                .queues
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
                .filter(|((k, _), _)| *k == key.0)
                .map(|(_, q)| q.clone())
                .collect();
            for queue in queues {
                queue.wait().await;
            }
        })
    }
}
pub fn on_startup(app: &App) {
    let state = app.ext::<ApiState>();
    let _ = state.credentials(app);
    // I05: Quellen additiv registrieren; ausdrücklich registrierte Test-/Hostquellen haben Vorrang.
    {
        let mut sources = state.sources.lock().unwrap_or_else(|p| p.into_inner());
        for kind in [
            os_model::ApiKind::Majestic,
            os_model::ApiKind::Ahrefs,
            os_model::ApiKind::Moz,
        ] {
            sources.entry(kind.to_string()).or_insert_with(|| {
                Arc::new(linkmetrics::LinkMetrics::with_clock(
                    kind,
                    app.config().clock.clone(),
                ))
            });
        }
    }
    for credential in state
        .credentials(app)
        .list(os_model::ApiKind::Ahrefs)
        .unwrap_or_default()
    {
        if credential.expires_at.is_some_and(|expires| {
            expires
                <= app
                    .config()
                    .clock
                    .now()
                    .plus(Duration::from_secs(30 * 24 * 3600))
        }) {
            app.push_notice(Notice {
                level: NoticeLevel::Warning,
                code: "notice-api-token-expiring".into(),
                args: std::collections::BTreeMap::from([(
                    "kind".into(),
                    serde_json::json!("ahrefs"),
                )]),
            });
        }
    }
    app.crawl_events()
        .add_barrier("api", Arc::new(Barrier(state.clone())));
    app.crawl_events().subscribe(Box::new(move |app, event| {
        if let CrawlEvent::Closed(key) = event {
            let ledger = state.quota(app);
            let key = key.0.clone();
            let now = app.config().clock.now();
            let app = app.clone();
            tokio::spawn(async move {
                let result = app
                    .write_queue()
                    .submit_admin(move || ledger.close_crawl(&key, now).map_err(error))
                    .await;
                if result.is_err() {
                    app.push_notice(Notice {
                        level: NoticeLevel::Warning,
                        code: "notice-api-quota-cleanup-failed".into(),
                        args: BTreeMap::new(),
                    });
                }
            });
            return;
        }
        let (key, pause) = match event {
            CrawlEvent::Paused(key) => (key, true),
            CrawlEvent::Resumed(key) => (key, false),
            _ => return,
        };
        for ((crawl, _), queue) in state
            .queues
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
        {
            if crawl == &key.0 {
                if pause {
                    queue.pause();
                } else {
                    queue.resume();
                }
            }
        }
    }));
}
pub fn on_config_changed(
    app: &App,
    key: &CrawlKey,
    old: &Arc<CrawlConfig>,
    new: &Arc<CrawlConfig>,
) {
    if app
        .ext::<ApiState>()
        .handled_changes
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&(
            key.0.clone(),
            serde_json::to_string(&new.api).expect("api config"),
        ))
    {
        return;
    }
    for kind in [ApiKind::Ga4, ApiKind::Gsc] {
        if data_settings(settings_value(old, kind)) != data_settings(settings_value(new, kind)) {
            let app = app.clone();
            let key = key.clone();
            let handle = app.handle();
            handle.spawn(async move {
                let _ = app
                    .write_queue()
                    .submit(key, move |conn| {
                        os_store::queries::api::invalidate(conn, name(kind), false)?;
                        Ok(())
                    })
                    .await;
            });
        }
    }
}
pub fn register(r: &mut Registry) {
    for (name, args, ret, long_running, handler) in [
        (
            "api.removeAccount",
            "ApiAccountArgs",
            "void",
            false,
            remove_account as crate::registry::Handler,
        ),
        ("api.list", "void", "ApiSourceInfo[]", false, list),
        ("api.connect", "ApiConnectArgs", "ApiAccount", true, connect),
        ("api.disconnect", "ApiKindArgs", "void", false, disconnect),
        (
            "api.testConnection",
            "ApiKindArgs",
            "ApiTestResult",
            false,
            test_connection,
        ),
        (
            "api.getSettings",
            "ApiKindArgs",
            "ApiSourceSettings",
            false,
            get_settings,
        ),
        (
            "api.setSettings",
            "ApiSetSettingsArgs",
            "ApiSettingsResult",
            false,
            set_settings,
        ),
        ("api.status", "void", "ApiRow[]", false, status),
        ("api.fetch", "ApiFetchArgs", "ApiFetchSummary", true, fetch),
        (
            "api.errors",
            "ApiErrorsArgs",
            "ApiErrorRow[]",
            false,
            errors,
        ),
        ("sidebar.apiStatus", "void", "ApiRow[]", false, status),
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

// Additive export access to the same persisted/session-only accounts (M2-X03, K2-61).
pub fn export_credentials(app: &App) -> Arc<Credentials> {
    app.ext::<ApiState>().credentials(app).clone()
}

/// Context for the APIs-only lifecycle, preserving session credentials and app network settings.
pub(crate) fn crawl_context(app: &App, cfg: CrawlConfig) -> ApiCtx {
    api_ctx(app, cfg)
}

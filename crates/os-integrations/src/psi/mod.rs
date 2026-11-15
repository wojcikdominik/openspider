//! PageSpeed Insights und Lighthouse (INTEG-130 … INTEG-139, INTEG-142, INTEG-143; M2-I04).
//!
//! - [`columns`]: Spalten, Metrikgruppen und Filter des PageSpeed-Tabs (Metrikname = Spaltenname).
//! - [`lighthouse`]: Lighthouse-/PSI-Antwort → Spaltenwerte, für Remote und Lokal gleich (TECH-230).
//! - [`remote`]: PSI-API v5 (Schlüsselprüfung beim Verbinden, Request-Aufbau, Fehlergründe).
//! - [`local`]: lokaler Lighthouse-Lauf mit begrenzter Parallelität und Timeout je URL (INTEG-132, K2-09).
//! - [`settings`]: Quelle, Gerätetyp, Gruppenauswahl, Grenzen; [`eta`]: Restzeit-Schätzung.
//! - [`PsiSource`]: die [`ApiSource`] der Quelle `psi` (Drosselung über [`crate::http::Transport`], Wiederholung bei
//!   500 mit Backoff, Pause nach gehäuften 500ern, lokales Tageskontingent).
//!
//! `psi/mobile.rs` (Mobile, TECH-230 ff.) liefert M2-D23.

pub mod columns;
pub mod eta;
pub mod lighthouse;
pub mod local;
pub mod mobile;
pub mod remote;
pub mod settings;

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_model::config::ApiLimitsConfig;
use os_model::{ApiKind, ApiValueWrite, Clock, SystemClock, Timestamp};

use crate::credentials::{read_account, Credential};
use crate::http::Transport;
use crate::{ApiCtx, ApiError, ApiSource, ApiState, ApiStatus, BoxFuture, UrlRef};
use lighthouse::{Cell, Outcome};
use settings::{Source, BURST_LIMIT, BURST_PAUSE_S};

/// Name der Quelle in `api_values`/`api_errors`.
pub const SOURCE: &str = "psi";

/// SQL der abzufragenden URLs (INTEG-130): interne HTML-URLs mit Status 200 (keine Weiterleitungen, keine Ressourcen).
pub const ELIGIBLE_SQL: &str = "SELECT id, url FROM urls WHERE is_internal = 1 AND in_tables = 1 \
     AND content_class = 'HTML' AND status_code = 200 ORDER BY id";

/// Die abzufragenden URLs einer Crawl-Datei (INTEG-130).
pub fn eligible_urls(file: &os_store::CrawlFile) -> Result<Vec<UrlRef>, ApiError> {
    let failed = |_| ApiError::Transport("Crawl-Datei nicht lesbar".into());
    let mut stmt = file.conn().prepare(ELIGIBLE_SQL).map_err(failed)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(UrlRef {
                url_id: Some(os_model::UrlId(r.get(0)?)),
                url: r.get(1)?,
            })
        })
        .map_err(failed)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(failed)
}

/// Ist für `cfg` die Quelle Lokal gewählt (INTEG-130)?
pub fn is_local(cfg: &os_model::CrawlConfig) -> bool {
    Source::of(&cfg.api.psi) == Source::Local
}

/// Lokaler Läufer laut Crawl-Konfiguration (INTEG-132); ohne Laufzeit [`local::RUNTIME_MISSING`].
pub fn local_runner_for(cfg: &os_model::CrawlConfig) -> Result<Arc<local::LocalRunner>, ApiError> {
    local::LocalRunner::discover(&local::LocalOptions::from_config(&cfg.api.psi)).map(Arc::new)
}

/// Quelle `psi` für Crawl und Verbindungstest: bei Quelle Lokal mit dem gefundenen Läufer (INTEG-130/132). Fehlt die
/// Laufzeit, bleibt die Quelle ohne Läufer und meldet [`local::RUNTIME_MISSING`].
pub fn source_for(cfg: &os_model::CrawlConfig, clock: Arc<dyn Clock>) -> PsiSource {
    let source = PsiSource::new().with_clock(clock);
    match is_local(cfg).then(|| local_runner_for(cfg)) {
        Some(Ok(runner)) => source.with_local(runner),
        _ => source,
    }
}

/// Wartefunktion (Backoff, Pause); Tests ersetzen sie durch eine Testuhr.
pub type Sleep = Arc<dyn Fn(Duration) -> BoxFuture<'static, ()> + Send + Sync>;

#[derive(Default)]
struct Daily {
    day: String,
    used: u64,
}

/// Die Quelle `psi` (Remote oder Lokal nach `api.psi.local`).
pub struct PsiSource {
    clock: Arc<dyn Clock>,
    sleep: Sleep,
    local: Option<Arc<local::LocalRunner>>,
    local_headers: Vec<(String, String)>,
    transport: Mutex<Option<(ApiLimitsConfig, Arc<Transport>)>>,
    consecutive_5xx: AtomicU32,
    paused_until: Mutex<Option<Timestamp>>,
    daily: Mutex<Daily>,
    seed: AtomicU64,
    done: AtomicU64,
    failed: AtomicU64,
}

impl Default for PsiSource {
    fn default() -> Self {
        PsiSource::new()
    }
}

impl PsiSource {
    /// Quelle mit Systemuhr und echter Wartezeit, ohne lokale Laufzeit.
    pub fn new() -> PsiSource {
        PsiSource {
            clock: Arc::new(SystemClock),
            sleep: Arc::new(|d| Box::pin(tokio::time::sleep(d))),
            local: None,
            local_headers: Vec::new(),
            transport: Mutex::default(),
            consecutive_5xx: AtomicU32::new(0),
            paused_until: Mutex::default(),
            daily: Mutex::default(),
            seed: AtomicU64::new(0),
            done: AtomicU64::new(0),
            failed: AtomicU64::new(0),
        }
    }

    /// Setzt die Uhr (Tageskontingent, Pause).
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> PsiSource {
        self.clock = clock;
        self
    }

    /// Setzt die Wartefunktion.
    pub fn with_sleep(mut self, sleep: Sleep) -> PsiSource {
        self.sleep = sleep;
        self
    }

    /// Lokaler Läufer für Quelle Lokal.
    pub fn with_local(mut self, runner: Arc<local::LocalRunner>) -> PsiSource {
        self.local = Some(runner);
        self
    }

    /// Zusätzliche Header des lokalen Laufs (z. B. Basic-Auth aus dem Auth-Profil, INTEG-132).
    pub fn with_local_headers(mut self, headers: Vec<(String, String)>) -> PsiSource {
        self.local_headers = headers;
        self
    }

    /// Heute (Pazifikzeit) gesendete Remote-Anfragen.
    pub fn daily_used(&self) -> u64 {
        let day = crate::queue::pacific_day(self.clock.now());
        let daily = self.daily.lock().unwrap_or_else(|p| p.into_inner());
        if daily.day == day {
            daily.used
        } else {
            0
        }
    }

    /// Ende der laufenden Pause nach gehäuften 500ern.
    pub fn paused_until(&self) -> Option<Timestamp> {
        let until = *self.paused_until.lock().unwrap_or_else(|p| p.into_inner());
        until.filter(|u| *u > self.clock.now())
    }

    /// Lokaler Läufer (für Restzeit und Prozesszählung).
    pub fn local_runner(&self) -> Option<&Arc<local::LocalRunner>> {
        self.local.as_ref()
    }

    fn transport(&self, ctx: &ApiCtx) -> Result<Arc<Transport>, ApiError> {
        let mut limits = settings::effective_limits(&ctx.cfg.api.psi);
        // Wiederholungen steuert die Quelle selbst (Zählung aufeinanderfolgender 500er, INTEG-143).
        limits.retries = 0;
        let mut slot = self.transport.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((l, t)) = slot.as_ref() {
            if *l == limits {
                return Ok(t.clone());
            }
        }
        let t = Transport::for_ctx(ctx, limits.clone())?;
        *slot = Some((limits, t.clone()));
        Ok(t)
    }

    /// Reserviert eine Remote-Anfrage im Tageskontingent; `false`, wenn es erschöpft ist.
    fn reserve_daily(&self, quota: u64) -> bool {
        let day = crate::queue::pacific_day(self.clock.now());
        let mut daily = self.daily.lock().unwrap_or_else(|p| p.into_inner());
        if daily.day != day {
            *daily = Daily { day, used: 0 };
        }
        if daily.used >= quota {
            return false;
        }
        daily.used += 1;
        true
    }

    async fn wait_pause(&self) {
        while let Some(until) = self.paused_until() {
            let now = self.clock.now();
            let ms = (until.unix_millis() - now.unix_millis()).max(1) as u64;
            (self.sleep)(Duration::from_millis(ms)).await;
        }
    }

    async fn fetch_remote(&self, page: &str, ctx: &ApiCtx) -> Result<Outcome, ApiError> {
        let psi = &ctx.cfg.api.psi;
        let key = read_account(ctx.secrets.as_ref(), ApiKind::Psi, psi.account.as_deref())?
            .api_key
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| ApiError::NotConnected("PSI-API-Schlüssel fehlt".into()))?;
        let url = remote::request_url(&remote::base(ctx), page, &key, psi.device)?;
        let url = mobile::request_categories(url, &ctx.cfg);
        let transport = self.transport(ctx)?;
        let retries = settings::effective_limits(psi).retries;
        let quota = settings::daily_quota(psi);
        let mut attempt = 0;
        loop {
            self.wait_pause().await;
            if !self.reserve_daily(quota) {
                // INTEG-142: keine Anfrage mehr; die URL erhält einen Fehler.
                return Ok(Outcome::error(format!(
                    "Quota exceeded: lokales Tageskontingent von {quota} Anfragen erreicht"
                )));
            }
            let seed = self.seed.fetch_add(1, Ordering::SeqCst);
            let result = transport
                .execute(transport.client.get(url.clone()), self.clock.now(), seed)
                .await;
            match result {
                Ok(response) => {
                    self.consecutive_5xx.store(0, Ordering::SeqCst);
                    let body: serde_json::Value = response
                        .bytes()
                        .await
                        .ok()
                        .and_then(|b| serde_json::from_slice(&b).ok())
                        .ok_or_else(|| ApiError::Transport("PSI-Antwort unlesbar".into()))?;
                    // Laufzeitfehler (ERRORED_DOCUMENT_REQUEST …) sind seitenbedingt: keine Wiederholung.
                    return Ok(lighthouse::parse_psi(&body));
                }
                Err(ApiError::QuotaExceeded(message)) => {
                    self.consecutive_5xx.store(0, Ordering::SeqCst);
                    return Ok(Outcome::error(format!("Quota exceeded: {message}")));
                }
                Err(e) => {
                    let status = e.http_status();
                    let server = status.is_some_and(|s| (500..=504).contains(&s));
                    if server {
                        let n = self.consecutive_5xx.fetch_add(1, Ordering::SeqCst) + 1;
                        if n > BURST_LIMIT {
                            self.consecutive_5xx.store(0, Ordering::SeqCst);
                            let until = self.clock.now().plus(Duration::from_secs(BURST_PAUSE_S));
                            *self.paused_until.lock().unwrap_or_else(|p| p.into_inner()) =
                                Some(until);
                        }
                    } else {
                        self.consecutive_5xx.store(0, Ordering::SeqCst);
                    }
                    if !(server || status == Some(429)) || attempt >= retries {
                        return Err(e);
                    }
                    if self.paused_until().is_none() {
                        (self.sleep)(crate::http::backoff(attempt, seed)).await;
                    }
                    attempt += 1;
                }
            }
        }
    }

    async fn fetch_local(&self, page: &str, ctx: &ApiCtx) -> Result<Outcome, ApiError> {
        let runner = self
            .local
            .as_ref()
            .ok_or_else(|| ApiError::NotConnected(local::RUNTIME_MISSING.into()))?;
        let mut headers = local::config_headers(&ctx.cfg, page);
        headers.extend(self.local_headers.iter().cloned());
        if mobile::selected(&ctx.cfg) {
            return Ok(runner
                .run_selected(page, ctx.cfg.api.psi.device, &headers, true)
                .await);
        }
        Ok(runner.run(page, ctx.cfg.api.psi.device, &headers).await)
    }

    /// Ergebnis einer URL ohne Umwandlung (für Tests und Aufrufer außerhalb der Warteschlange).
    pub async fn fetch_outcome(&self, page: &str, ctx: &ApiCtx) -> Result<Outcome, ApiError> {
        let outcome = match Source::of(&ctx.cfg.api.psi) {
            Source::Remote => self.fetch_remote(page, ctx).await,
            Source::Local => self.fetch_local(page, ctx).await,
        };
        let outcome = outcome.map(|mut out| {
            mobile::apply_config(&mut out, &ctx.cfg);
            out
        });
        match &outcome {
            Ok(o) if o.is_success() => self.done.fetch_add(1, Ordering::SeqCst),
            _ => self.failed.fetch_add(1, Ordering::SeqCst),
        };
        outcome
    }
}

/// Wandelt ein Ergebnis in `api_values`-Zeilen der Quelle `psi` um.
pub fn to_values(url: &UrlRef, outcome: &Outcome, at: Timestamp) -> Vec<ApiValueWrite> {
    outcome
        .cells
        .iter()
        .map(|(metric, cell)| {
            let (v_real, v_text) = match cell {
                Cell::Num(n) => (Some(*n), None),
                Cell::Text(t) => (None, Some(t.clone())),
            };
            ApiValueWrite {
                url_id: url.url_id,
                url: url.url.clone(),
                source: SOURCE.into(),
                metric: metric.clone(),
                level: "url".into(),
                v_real,
                v_text,
                fetched_at: at,
            }
        })
        .collect()
}

impl ApiSource for PsiSource {
    fn kind(&self) -> ApiKind {
        ApiKind::Psi
    }

    fn fetch<'a>(
        &'a self,
        urls: &'a [UrlRef],
        ctx: &'a ApiCtx,
    ) -> BoxFuture<'a, Result<Vec<ApiValueWrite>, ApiError>> {
        Box::pin(async move {
            let mut values = Vec::new();
            for url in urls {
                let outcome = self.fetch_outcome(&url.url, ctx).await?;
                values.extend(to_values(url, &outcome, self.clock.now()));
            }
            Ok(values)
        })
    }

    fn test_connection<'a>(
        &'a self,
        ctx: &'a ApiCtx,
        credential: &'a Credential,
    ) -> BoxFuture<'a, Result<Credential, ApiError>> {
        Box::pin(async move {
            match Source::of(&ctx.cfg.api.psi) {
                Source::Remote => {
                    // Kein stilles Umschalten auf Lokal: ohne Schlüssel bleibt die Quelle getrennt (INTEG-130).
                    let key = credential.api_key.as_deref().unwrap_or_default();
                    remote::check_key(ctx, key).await?;
                }
                Source::Local => {
                    let runner = self
                        .local
                        .as_ref()
                        .ok_or_else(|| ApiError::NotConnected(local::RUNTIME_MISSING.into()))?;
                    let version = runner.version().await?;
                    let mut credential = credential.clone();
                    credential.label = format!("Lighthouse {version}");
                    return Ok(credential);
                }
            }
            Ok(credential.clone())
        })
    }

    fn status(&self) -> ApiStatus {
        let paused = self.paused_until();
        let done = self.done.load(Ordering::SeqCst);
        let failed = self.failed.load(Ordering::SeqCst);
        ApiStatus {
            kind: ApiKind::Psi,
            state: if paused.is_some() {
                ApiState::Paused
            } else {
                ApiState::Idle
            },
            urls_total: done + failed,
            urls_done: done + failed,
            urls_failed: failed,
            detail: paused.map(|until| {
                format!(
                    "PSI pausiert 10 Minuten nach gehäuften Serverfehlern (HTTP 500), bis {}",
                    until.to_db()
                )
            }),
        }
    }
}

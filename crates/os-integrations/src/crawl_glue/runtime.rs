use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use os_model::{ApiKind, ApiValueWrite, Clock, CrawlConfig};
use os_store::CrawlFile;

use super::db;
use crate::{ApiCtx, ApiError, ApiSource, UrlRef};

pub fn enabled(cfg: &CrawlConfig) -> Vec<ApiKind> {
    [
        (ApiKind::Ga4, cfg.api.ga4.enabled),
        (ApiKind::Gsc, cfg.api.gsc.enabled),
        (ApiKind::Psi, cfg.api.psi.enabled),
        (ApiKind::Majestic, cfg.api.majestic.enabled),
        (ApiKind::Ahrefs, cfg.api.ahrefs.enabled),
        (ApiKind::Moz, cfg.api.moz.enabled),
    ]
    .into_iter()
    .filter_map(|(kind, on)| on.then_some(kind))
    .collect()
}

/// Scheduling selects sources without replacing account, property or stream settings from the profile.
pub fn select(cfg: &mut CrawlConfig, sources: &[String]) -> Result<(), ApiError> {
    for source in sources {
        match source.as_str() {
            "ga4" => cfg.api.ga4.enabled = true,
            "gsc" => cfg.api.gsc.enabled = true,
            "psi" => cfg.api.psi.enabled = true,
            "majestic" => cfg.api.majestic.enabled = true,
            "ahrefs" => cfg.api.ahrefs.enabled = true,
            "moz" => cfg.api.moz.enabled = true,
            _ => {
                return Err(ApiError::NotConnected(format!(
                    "Unbekannte Integration: {source}"
                )))
            }
        }
    }
    Ok(())
}

fn account(cfg: &CrawlConfig, kind: ApiKind) -> Option<&str> {
    match kind {
        ApiKind::Ga4 => cfg.api.ga4.account.as_deref(),
        ApiKind::Gsc => cfg.api.gsc.account.as_deref(),
        ApiKind::Psi => cfg.api.psi.account.as_deref(),
        ApiKind::Majestic => cfg.api.majestic.account.as_deref(),
        ApiKind::Ahrefs => cfg.api.ahrefs.account.as_deref(),
        ApiKind::Moz => cfg.api.moz.account.as_deref(),
        _ => None,
    }
}

/// Quelle PSI mit Quelle Lokal: kein Konto, sondern die Lighthouse-Laufzeit entscheidet (INTEG-130/132).
fn psi_local(cfg: &CrawlConfig, kind: ApiKind) -> bool {
    kind == ApiKind::Psi && crate::psi::is_local(cfg)
}

pub(super) fn connected(ctx: &ApiCtx, kind: ApiKind) -> bool {
    if psi_local(&ctx.cfg, kind) {
        return crate::psi::local_runner_for(&ctx.cfg).is_ok();
    }
    crate::credentials::read_account(ctx.secrets.as_ref(), kind, account(&ctx.cfg, kind)).is_ok_and(
        |c| {
            c.api_key
                .as_ref()
                .or(c.access_token.as_ref())
                .is_some_and(|s| !s.is_empty())
        },
    )
}

/// No network request: missing selected accounts fail before a crawl can be created when required.
pub fn preflight(ctx: &ApiCtx, require: bool) -> Result<Vec<String>, ApiError> {
    check(ctx, require, |kind| connected(ctx, kind))
}

/// Warnung bzw. Fehler einer nicht bereiten Quelle; lokal nennt sie die fehlende Laufzeit (INTEG-132).
fn not_ready(cfg: &CrawlConfig, kind: ApiKind) -> String {
    if psi_local(cfg, kind) {
        return format!(
            "{kind}: PageSpeed läuft nicht lokal ({})",
            crate::psi::local::RUNTIME_MISSING
        );
    }
    format!(
        "{kind}: Konto {} nicht verbunden",
        account(cfg, kind).unwrap_or("Standard")
    )
}

fn check(
    ctx: &ApiCtx,
    require: bool,
    ready: impl Fn(ApiKind) -> bool,
) -> Result<Vec<String>, ApiError> {
    let mut warnings = Vec::new();
    for kind in enabled(&ctx.cfg) {
        if !ready(kind) {
            let message = not_ready(&ctx.cfg, kind);
            if require {
                return Err(ApiError::NotConnected(message));
            }
            warnings.push(message);
        }
    }
    Ok(warnings)
}

// Crawl callers pass a sidecar path; explicit ledger paths used by embedders remain supported.
fn ledger_path(path: PathBuf, settings: &std::path::Path) -> PathBuf {
    if path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(".api-quotas.json"))
    {
        settings.join("api-quotas.json")
    } else {
        path
    }
}

/// Sources live for the whole run so bulk reports are fetched only once.
pub struct ApiRun {
    ctx: ApiCtx,
    clock: Arc<dyn Clock>,
    ga4: Arc<crate::ga4::Ga4Source>,
    gsc: Arc<crate::gsc::GscSource>,
    sources: Vec<Arc<dyn ApiSource>>,
    /// Quelle PSI läuft lokal und hat einen Lighthouse-Läufer (INTEG-132).
    psi_local_ready: bool,
}
impl ApiRun {
    pub fn new(ctx: ApiCtx, clock: Arc<dyn Clock>, ledger: PathBuf) -> Self {
        let ledger = ctx
            .quota_ledger
            .clone()
            .unwrap_or_else(|| ledger_path(ledger, &os_store::paths::app_paths().settings));
        let ga4 = Arc::new(crate::ga4::Ga4Source::new(clock.clone()));
        let gsc = Arc::new(crate::gsc::GscSource::new(
            clock.clone(),
            Arc::new(crate::quota::QuotaLedger::new(ledger)),
        ));
        let mut psi_local_ready = false;
        let sources = enabled(&ctx.cfg)
            .into_iter()
            .map(|kind| -> Arc<dyn ApiSource> {
                match kind {
                    ApiKind::Ga4 => ga4.clone(),
                    ApiKind::Gsc => gsc.clone(),
                    ApiKind::Psi => {
                        // INTEG-132: bei Quelle Lokal hängt der Läufer der Crawl-Konfiguration an der Quelle.
                        let psi = crate::psi::source_for(&ctx.cfg, clock.clone());
                        psi_local_ready = psi.local_runner().is_some();
                        Arc::new(psi)
                    }
                    kind => Arc::new(crate::linkmetrics::LinkMetrics::with_clock(
                        kind,
                        clock.clone(),
                    )),
                }
            })
            .collect();
        Self {
            ctx,
            clock,
            ga4,
            gsc,
            sources,
            psi_local_ready,
        }
    }

    /// Ersetzt den lokalen Lighthouse-Läufer der Quelle PSI (Einbettung und Tests ohne installierte Laufzeit).
    pub fn with_psi_local(mut self, runner: Arc<crate::psi::local::LocalRunner>) -> Self {
        if !psi_local(&self.ctx.cfg, ApiKind::Psi) {
            return self;
        }
        for source in &mut self.sources {
            if source.kind() == ApiKind::Psi {
                *source = Arc::new(
                    crate::psi::PsiSource::new()
                        .with_clock(self.clock.clone())
                        .with_local(runner.clone()),
                );
                self.psi_local_ready = true;
            }
        }
        self
    }

    fn ready(&self, kind: ApiKind) -> bool {
        if psi_local(&self.ctx.cfg, kind) {
            return self.psi_local_ready;
        }
        connected(&self.ctx, kind)
    }

    pub fn validate(&mut self, require: bool) -> Result<Vec<String>, ApiError> {
        let warnings = check(&self.ctx, require, |kind| self.ready(kind))?;
        if require && self.sources.is_empty() {
            return Err(ApiError::NotConnected(
                "Mindestens eine Integration muss verbunden sein".into(),
            ));
        }
        let ready: Vec<ApiKind> = self
            .sources
            .iter()
            .map(|s| s.kind())
            .filter(|kind| self.ready(*kind))
            .collect();
        self.sources.retain(|s| ready.contains(&s.kind()));
        Ok(warnings)
    }

    async fn validate_ga4_selection(&self) -> Result<(), ApiError> {
        let Some(account) = &self.ctx.cfg.api.ga4.analytics_account else {
            return Ok(());
        };
        let name = if account.starts_with("accounts/") {
            account.clone()
        } else {
            format!("accounts/{account}")
        };
        let (property, _) = crate::ga4::settings::parse_property(
            self.ctx.cfg.api.ga4.property.as_deref().unwrap_or_default(),
        )
        .map_err(db)?;
        let tree = self.ga4.accounts(&self.ctx).await?;
        if tree.properties(&name).is_ok_and(|properties| {
            properties
                .iter()
                .any(|p| p.name == format!("properties/{property}"))
        }) {
            return Ok(());
        }
        Err(ApiError::NotConnected(
            "GA4-Property gehört nicht zum gewählten Analytics-Konto".into(),
        ))
    }

    /// Records every same-host API URL, including already linked URLs, as report provenance.
    /// Admission into the frontier remains the engine's job (rules, robots, limits).
    pub async fn discover(&self, file: &CrawlFile) -> Result<Vec<String>, ApiError> {
        let urls = urls(file)?;
        let mut hosts: BTreeSet<_> = urls.iter().filter_map(|u| host(&u.url)).collect();
        if let Some(start) = file
            .get_meta("start_url")
            .map_err(db)?
            .and_then(|u| host(&u))
        {
            hosts.insert(start);
        }
        let mut candidates = BTreeSet::new();
        for (kind, crawl_new) in [
            (ApiKind::Ga4, self.ctx.cfg.api.ga4.crawl_new_urls),
            (ApiKind::Gsc, self.ctx.cfg.api.gsc.crawl_new_urls),
        ] {
            if !self.sources.iter().any(|source| source.kind() == kind) {
                continue;
            }
            // A provider failure must not discard another provider's URLs or skip its request.
            let (addresses, error) = match self.discovery_urls(kind).await {
                Ok(result) => result,
                Err(error) => (BTreeSet::new(), Some(error)),
            };
            let source = kind.to_string();
            let capture = file.conn().unchecked_transaction().map_err(db)?;
            capture
                .execute(
                    "DELETE FROM api_errors WHERE source=?1 AND code='api_discovery'",
                    [&source],
                )
                .map_err(db)?;
            if let Some(error) = error {
                capture.execute(
                    "INSERT INTO api_errors(source,url,code,message,at) VALUES(?1,NULL,'api_discovery',?2,?3)",
                    (&source, error.public_message(), self.clock.now().to_db()),
                ).map_err(db)?;
            }
            for address in addresses {
                if host(&address).is_some_and(|h| hosts.contains(&h)) {
                    record_source(file, &address, &source, self.clock.now())?;
                    if crawl_new {
                        candidates.insert(address);
                    }
                }
            }
            capture.execute(
                "INSERT INTO crawl_meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                (format!("api_raw_hosts.{source}"), serde_json::to_string(&hosts).map_err(db)?),
            ).map_err(db)?;
            capture.commit().map_err(db)?;
        }
        Ok(candidates.into_iter().collect())
    }

    async fn discovery_urls(
        &self,
        kind: ApiKind,
    ) -> Result<(BTreeSet<String>, Option<ApiError>), ApiError> {
        if kind == ApiKind::Ga4 {
            self.validate_ga4_selection().await?;
            let report = self.ga4.report(&self.ctx).await?;
            return Ok((
                report
                    .rows
                    .values()
                    .flat_map(|rows| rows.keys().cloned())
                    .collect(),
                None,
            ));
        }
        let options =
            crate::gsc::options::GscOptions::from_config(&self.ctx.cfg.api.gsc).map_err(db)?;
        let token = crate::credentials::read_account(
            self.ctx.secrets.as_ref(),
            ApiKind::Gsc,
            self.ctx.cfg.api.gsc.account.as_deref(),
        )?
        .access_token;
        let http =
            crate::gsc::client::GscHttp::new(&self.ctx, &options, token, self.clock.clone())?;
        let report = self.gsc.report(&http, &options).await;
        Ok((
            report.rows.iter().map(|row| row.page.clone()).collect(),
            report.error.as_ref().map(|error| error.to_api_error()),
        ))
    }

    /// Completion barrier: all results and per-URL failures are durable before returning.
    pub async fn collect(&self, file: &CrawlFile) -> Result<(), ApiError> {
        let urls = urls(file)?;
        let apis = file.get_meta("source_mode").map_err(db)?.as_deref() == Some("apis");
        if !apis {
            let mut stmt = file.conn().prepare("SELECT id,url,content_class='HTML',indexability='Indexable' FROM urls WHERE is_internal=1 AND in_tables=1").map_err(db)?;
            let targets = stmt
                .query_map([], |r| {
                    Ok(crate::gsc::inspection::Target {
                        url_id: Some(os_model::UrlId(r.get(0)?)),
                        url: r.get(1)?,
                        is_html: r.get::<_, Option<bool>>(2)?.unwrap_or(false),
                        indexable: r.get::<_, Option<bool>>(3)?.unwrap_or(false),
                    })
                })
                .map_err(db)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db)?;
            self.gsc.set_targets(targets);
        }
        for source in &self.sources {
            if source.kind() == ApiKind::Ga4 {
                self.validate_ga4_selection().await?;
            }
            let kind = source.kind().to_string();
            let mut failed = 0;
            let mut done = 0;
            let eligible = if !apis && source.kind() == ApiKind::Psi {
                Some(crate::psi::eligible_urls(file)?)
            } else {
                None
            };
            let source_urls = eligible.as_deref().unwrap_or(&urls);
            let link_metrics = crate::linkmetrics::is_linkmetrics(source.kind());
            let source = if link_metrics {
                crate::linkmetrics::batch_source(source.clone(), source_urls.to_vec())
            } else {
                source.clone()
            };
            for chunk in source_urls.chunks(if link_metrics { 1 } else { 100 }) {
                let outcomes = if matches!(source.kind(), ApiKind::Ga4 | ApiKind::Gsc) {
                    let values = source.fetch(chunk, &self.ctx).await;
                    chunk
                        .iter()
                        .map(|url| {
                            (
                                url.clone(),
                                values
                                    .as_ref()
                                    .map(|values| {
                                        values
                                            .iter()
                                            .filter(|v| v.url_id == url.url_id)
                                            .cloned()
                                            .collect()
                                    })
                                    .map_err(Clone::clone),
                            )
                        })
                        .collect()
                } else {
                    source.fetch_outcomes(chunk, &self.ctx).await
                };
                for (url, outcome) in outcomes {
                    done += 1;
                    match outcome {
                        Ok(values) => persist(file, &kind, &url, &values)?,
                        Err(error) => {
                            failed += 1;
                            os_store::queries::api::record_error(
                                file.conn(),
                                &kind,
                                &url.url,
                                "api_request",
                                &error.public_message(),
                                self.clock.now(),
                            )
                            .map_err(db)?;
                        }
                    }
                    os_store::queries::api::set_status(
                        file.conn(),
                        &os_store::queries::api::Status {
                            source: kind.clone(),
                            state: if done == source_urls.len() {
                                "idle"
                            } else {
                                "fetching"
                            }
                            .into(),
                            total: source_urls.len() as u64,
                            done: done as u64,
                            failed,
                            detail: (failed > 0).then(|| "API-Abrufe mit Fehlern".into()),
                        },
                    )
                    .map_err(db)?;
                }
            }
        }
        Ok(())
    }
}

pub fn urls(file: &CrawlFile) -> Result<Vec<UrlRef>, ApiError> {
    file.conn()
        .prepare("SELECT id,url FROM urls WHERE is_internal=1 AND in_tables=1 ORDER BY id")
        .map_err(db)?
        .query_map([], |r| {
            Ok(UrlRef {
                url_id: Some(os_model::UrlId(r.get(0)?)),
                url: r.get(1)?,
            })
        })
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)
}
fn host(address: &str) -> Option<String> {
    let url = url::Url::parse(address).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.host_str().map(str::to_lowercase)
}
fn record_source(
    file: &CrawlFile,
    url: &str,
    source: &str,
    at: os_model::Timestamp,
) -> Result<(), ApiError> {
    file.conn()
        .execute(
            "INSERT OR IGNORE INTO api_urls(url,source,first_seen) VALUES(?1,?2,?3)",
            (url, source, at.to_db()),
        )
        .map_err(db)?;
    Ok(())
}
fn persist(
    file: &CrawlFile,
    source: &str,
    url: &UrlRef,
    values: &[ApiValueWrite],
) -> Result<(), ApiError> {
    let tx = file.conn().unchecked_transaction().map_err(db)?;
    tx.execute(
        "DELETE FROM api_values WHERE url_id=?1 AND source=?2",
        (url.url_id.map(|id| id.0), source),
    )
    .map_err(db)?;
    for value in values {
        if let Some(id) = value.url_id {
            tx.execute("INSERT OR REPLACE INTO api_values(url_id,source,metric,level,v_real,v_text,fetched_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", (id.0,&value.source,&value.metric,&value.level,value.v_real,&value.v_text,value.fetched_at.to_db())).map_err(db)?;
        }
    }
    os_store::queries::api::clear_error(&tx, source, &url.url).map_err(db)?;
    let seq = os_store::queries::change_seq(&tx).map_err(db)? + 1;
    tx.execute(
        "UPDATE urls SET change_seq=?1 WHERE id=?2",
        (seq, url.url_id.map(|id| id.0)),
    )
    .map_err(db)?;
    tx.execute(
        "UPDATE crawl_meta SET value=?1 WHERE key='change_seq'",
        [seq.to_string()],
    )
    .map_err(db)?;
    tx.commit().map_err(db)
}

#[cfg(test)]
mod quota_path_tests {
    use super::ledger_path;
    use crate::quota::QuotaLedger;
    use os_model::Timestamp;

    // spec: INTEG-006
    #[test]
    fn crawl_sidecars_share_the_settings_quota_and_reset_at_pacific_midnight() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings");
        let crawls = dir.path().join("crawls");
        let a = crawls.join("a.api-quotas.json");
        let b = crawls.join("b.api-quotas.json");
        let first = QuotaLedger::new(ledger_path(a.clone(), &settings));
        let second = QuotaLedger::new(ledger_path(b.clone(), &settings));
        let now = Timestamp::from_db("2026-03-09T06:59:59Z").unwrap();
        let reset = Timestamp::from_db("2026-03-09T07:00:00Z").unwrap();
        let property = include_str!("apis-mode-list.txt").lines().next().unwrap();
        assert!(first.reserve("gsc", property, now, 1).unwrap());
        assert!(!second.reserve("gsc", property, now, 1).unwrap());
        assert!(second.reserve("gsc", property, reset, 1).unwrap());
        assert!(settings.join("api-quotas.json").is_file());
        assert!(!a.exists() && !b.exists());
    }
}

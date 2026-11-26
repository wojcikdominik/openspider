//! „Neu crawlen“ und „Entfernen“ einzelner URLs (CRAWL-131).
//!
//! * **Neu crawlen** ([`recrawl_urls`]): Die URLs werden erneut abgerufen und ihre Daten über `PageWrite.replace`
//!   ersetzt – Status, Inhalte, Outlinks. Der Writer löscht dabei die alten Kindzeilen und korrigiert die
//!   Inlink-Zähler anderer URLs; neue Outlinks im Bereich werden in die Warteschlange der Datei eingereiht
//!   (sie werden beim Fortsetzen abgerufen), entfallene verschwinden. Der Lauf geht über denselben Writer wie
//!   ein Crawl und stempelt `change_seq` im Änderungsfeed. Mehrere URLs laufen gebündelt mit den
//!   `speed.threads` der Konfiguration.
//! * **Entfernen** ([`remove_urls`]): Die URL und ihre ausgehenden Links verschwinden; Links anderer Seiten
//!   auf sie bleiben als Links ohne Ziel (und damit ohne Statuscode) stehen, Zähler der betroffenen URLs werden
//!   neu berechnet. Die Änderung erhöht `crawl_meta.change_seq` und setzt `reset_seq` auf denselben Wert
//!   (Löschungen sind im Änderungsfeed sonst unsichtbar).
//!
//! Beide Aktionen machen die Crawl-Analyse **veraltet** (`analysis_stale = 1`, `analysis_state = stale`); sie
//! startet nicht von selbst (CRAWL-131, CRAWL-141 Punkt 6). Sie verlangen, dass kein Prozess den Crawl gerade
//! abarbeitet.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use os_fetch::Fetch;
use os_model::{CrawlMode, UrlId};
use os_store::{CrawlFile, OpenMode};
use os_url::Scope;
use tokio::task::JoinSet;

use crate::crawl::{self, SharedInit};
use crate::error::EngineError;
use crate::frontier::{CrawlScope, Purpose, QueueItem};
use crate::lifecycle::ensure_not_running;
use crate::resume::{self, PrimedFetch};
use crate::types::EngineDeps;
use crate::worker;

fn db<T, E: Into<os_store::StoreError>>(r: Result<T, E>) -> Result<T, EngineError> {
    r.map_err(|e| EngineError::Store(e.into()))
}

/// Markiert die Crawl-Analyse als veraltet (alle Teilschritte). Gleiche Schreibweise wie
/// `os_analyze::analysis::mark_stale` für alle Schritte: `analysis_stale = 1`, ein abgeschlossener oder
/// unvollständiger Lauf wechselt auf `stale`.
pub(crate) fn mark_analysis_stale(file: &CrawlFile) -> Result<(), EngineError> {
    file.set_meta("analysis_stale", "1")?;
    if matches!(
        file.get_meta("analysis_state")?.as_deref(),
        Some("completed" | "incomplete")
    ) {
        file.set_meta("analysis_state", "stale")?;
    }
    Ok(())
}

struct RecrawlPrep {
    cfg: Arc<os_model::CrawlConfig>,
    rules: os_url::RuleSet,
    restored: resume::Restored,
    scope: CrawlScope,
    start_encoded: String,
    items: Vec<QueueItem>,
    started_at: os_model::Timestamp,
}

fn prepare_recrawl(
    path: &Path,
    urls: &[UrlId],
    deps: &EngineDeps,
) -> Result<RecrawlPrep, EngineError> {
    let file = CrawlFile::open(path, OpenMode::ReadWrite)?;
    ensure_not_running(&file, deps)?;
    let meta = file.meta()?;
    let mut cfg = file.config()?;
    cfg.mode = meta.mode;
    crawl::check_rendering(&cfg)?;
    let rules = os_url::RuleSet::compile(
        &cfg.scope.include,
        &cfg.scope.exclude,
        cfg.scope.regex_partial_match,
    )
    .map_err(|e| EngineError::Invalid(e.to_string()))?;
    let mut restored = resume::restore(&file, &cfg, meta.mode, false)?;
    let (scope, start_encoded) = match meta.mode {
        CrawlMode::Serp => return Err(EngineError::Invalid(
            "SERP-Entwürfe werden offline importiert; für neue Werte die Datei erneut hochladen"
                .into(),
        )),
        CrawlMode::Spider => {
            let enc = meta.start_url.clone().ok_or_else(|| {
                EngineError::Invalid("Die Crawl-Datei enthält keine Start-URL".to_string())
            })?;
            let norm = os_url::parse_start_url(&enc)
                .map_err(|e| EngineError::Invalid(format!("Start-URL: {e}")))?;
            (
                CrawlScope::new(Scope::new(&norm, &cfg.scope).with_cdns(&cfg.cdns)),
                norm.encoded.clone(),
            )
        }
        CrawlMode::List => {
            let anchor = restored
                .anchor
                .as_deref()
                .and_then(|a| os_url::parse_start_url(a).ok())
                .ok_or_else(|| {
                    EngineError::Invalid("Die Crawl-Datei enthält keine Listen-URL".to_string())
                })?;
            let hosts = std::mem::take(&mut restored.list_hosts);
            (
                CrawlScope::with_hosts(Scope::new(&anchor, &cfg.scope).with_cdns(&cfg.cdns), hosts),
                String::new(),
            )
        }
    };

    let mut items = Vec::new();
    {
        let mut stmt = db(file.conn().prepare(
            "SELECT url, crawl_depth, is_internal, content_class FROM urls WHERE id = ?1",
        ))?;
        let mut seen = BTreeSet::new();
        for id in urls {
            if !seen.insert(id.0) {
                continue;
            }
            let mut rows = db(stmt.query([id.0]))?;
            let Some(row) = db(rows.next())? else {
                continue;
            };
            let url: String = db(row.get(0))?;
            let depth: Option<i64> = db(row.get(1))?;
            let internal: i64 = db(row.get(2))?;
            let class: String = db(row.get(3))?;
            let purpose = if internal == 0 {
                Purpose::External
            } else if matches!(class.as_str(), "HTML" | "Unknown" | "PDF" | "XML" | "Other") {
                Purpose::Page
            } else {
                Purpose::Resource
            };
            items.push(QueueItem {
                id: *id,
                url: url.into_boxed_str(),
                depth: depth.and_then(|d| u32::try_from(d).ok()),
                priority: 0,
                seq: 0,
                purpose,
                hops: 0,
                session: 0,
                replace: true,
                attempt: 0,
            });
        }
    }
    Ok(RecrawlPrep {
        cfg: Arc::new(cfg),
        rules,
        restored,
        scope,
        start_encoded,
        items,
        started_at: meta.started_at.unwrap_or_else(|| deps.clock.now()),
    })
}

/// Ruft die URLs erneut ab und ersetzt ihre Daten (`PageWrite.replace`).
pub async fn recrawl_urls<F: Fetch>(
    fetcher: Arc<F>,
    file: &Path,
    urls: &[UrlId],
    deps: EngineDeps,
) -> Result<(), EngineError> {
    if urls.is_empty() {
        return Ok(());
    }
    let prep = {
        let (path, ids, deps) = (file.to_path_buf(), urls.to_vec(), deps.clone());
        tokio::task::spawn_blocking(move || prepare_recrawl(&path, &ids, &deps))
            .await
            .map_err(|e| EngineError::Join(e.to_string()))??
    };
    let RecrawlPrep {
        cfg,
        rules,
        mut restored,
        scope,
        start_encoded,
        items,
        started_at,
    } = prep;
    if items.is_empty() {
        return Ok(());
    }
    let writer = crate::writer_thread::WriterThread::spawn(crawl::open_sink(&deps, file)?);
    let primed_origins = restored
        .primed
        .iter()
        .map(|(_, p)| p.origin.clone())
        .collect();
    let fetcher = Arc::new(PrimedFetch::new(fetcher, &restored.primed));
    // Das Neu-Crawlen ist unbegrenzt (kein URL-Limit) und darf die Warteschlange der Datei nicht abarbeiten.
    restored.core.max_urls = u64::MAX;
    let threads = cfg.speed.threads.max(1) as usize;
    let tracker = crawl::make_tracker(
        &cfg,
        &*deps.clock,
        started_at,
        std::time::Duration::ZERO,
        0,
        std::time::Duration::ZERO,
    );
    let (sh, _control) = crawl::make_shared(SharedInit {
        fetcher,
        cfg,
        clock: deps.clock.clone(),
        scope,
        rules,
        core: restored.core,
        writer: writer.sender(),
        tracker,
        start_encoded,
        primed_origins,
    });

    if let Some(renderer) = &deps.renderer {
        let _ = sh.render_session.injected.set(renderer.clone());
    }
    let mut set: JoinSet<()> = JoinSet::new();
    let mut pending = items.into_iter();
    loop {
        while set.len() < threads {
            let Some(item) = pending.next() else { break };
            {
                let mut core = sh.core.lock().unwrap_or_else(|p| p.into_inner());
                core.start_worker(item.priority == 0);
            }
            set.spawn(worker::process(Arc::clone(&sh), item));
        }
        match set.join_next().await {
            Some(Err(e)) if e.is_panic() => {
                sh.fail(EngineError::Join(format!("Worker abgestürzt: {e}")));
            }
            Some(_) => {}
            None => break,
        }
    }

    let flushed = writer.flush().await.is_ok() && writer.checkpoint().await.is_ok();
    let shared_error = sh.error.lock().unwrap_or_else(|p| p.into_inner()).take();
    drop(sh);
    let finished = writer.finish();
    if let Some(e) = shared_error {
        return Err(e);
    }
    finished.map_err(EngineError::Store)?;
    if !flushed {
        return Err(EngineError::Join(
            "Neu crawlen wegen eines Fehlers beim Schreiben abgebrochen".to_string(),
        ));
    }
    let handle = CrawlFile::open(file, OpenMode::ReadWrite)?;
    mark_analysis_stale(&handle)?;
    Ok(())
}

/// Tabellen mit einer Spalte, die auf eine URL zeigt und beim Entfernen der URL zeilenweise gelöscht wird.
const OWNED: &[(&str, &str)] = &[
    ("cookies", "url_id"),
    ("discovery_edges", "target_id"),
    ("links", "source_id"),
    ("image_instances", "page_id"),
    ("canonicals", "url_id"),
    ("directives", "url_id"),
    ("refresh", "url_id"),
    ("hreflang", "url_id"),
    ("pagination", "url_id"),
    ("http_headers", "url_id"),
    ("url_flags", "url_id"),
    ("queue", "url_id"),
    ("sitemap_entries", "url_id"),
    ("page_html", "url_id"),
];

/// Tabellen mit Verweisen auf eine URL, die beim Entfernen auf NULL gesetzt werden: `(Tabelle, Spalte)`.
const REFERENCES: &[(&str, &str)] = &[
    ("links", "target_id"),
    ("image_instances", "image_id"),
    ("canonicals", "target_id"),
    ("refresh", "target_id"),
    ("hreflang", "target_id"),
    ("pagination", "target_id"),
    ("sitemaps", "url_id"),
    ("list_input", "url_id"),
];

/// Ergebnisspalte einer Abfrage mit der Kennungsliste als JSON-Feld in `?1`.
fn column_ids(file: &CrawlFile, sql: &str, ids_json: &str) -> Result<Vec<i64>, EngineError> {
    let mut stmt = db(file.conn().prepare_cached(sql))?;
    let rows = db(stmt.query_map([ids_json], |r| r.get::<_, i64>(0)))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(db(row)?);
    }
    Ok(out)
}

/// Entfernt URLs aus der Crawl-Datei (Inlinks bleiben als Links ohne Status).
///
/// Jede Abfrage über `links.target_id` gilt der **ganzen** Menge der entfernten URLs und kostet damit höchstens
/// einen Lauf über `links` – mit dem Index `links_target` über die Kennungen, ohne ihn (K-74: pausierter oder
/// abgebrochener Crawl) sequenziell, nie ein Lauf je URL.
pub fn remove_urls(file: &Path, urls: &[UrlId]) -> Result<(), EngineError> {
    if urls.is_empty() {
        return Ok(());
    }
    let handle = CrawlFile::open(file, OpenMode::ReadWrite)?;
    // Systemuhr genügt: die Prüfung betrifft nur den Heartbeat der Datei.
    let probe = EngineDepsProbe::new();
    ensure_not_running_with(&handle, &probe)?;
    let conn = handle.conn();
    let removed: BTreeSet<i64> = urls.iter().map(|u| u.0).collect();
    let removed_json = format!(
        "[{}]",
        removed
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    const IN_REMOVED: &str = "IN (SELECT value FROM json_each(?1))";

    db(conn.execute_batch("BEGIN IMMEDIATE"))?;
    let result = (|| -> Result<(), EngineError> {
        let seq: i64 = handle
            .get_meta("change_seq")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
            + 1;
        // betroffene Nachbarn merken, bevor ihre Verbindungen verschwinden
        let mut affected: BTreeSet<i64> = BTreeSet::new();
        for sql in [
            format!("SELECT DISTINCT target_id FROM links WHERE source_id {IN_REMOVED} AND target_id IS NOT NULL"),
            format!("SELECT DISTINCT image_id FROM image_instances WHERE page_id {IN_REMOVED} AND image_id IS NOT NULL"),
            format!("SELECT DISTINCT source_id FROM links WHERE target_id {IN_REMOVED}"),
            format!("SELECT DISTINCT page_id FROM image_instances WHERE image_id {IN_REMOVED}"),
            format!("SELECT id FROM urls WHERE redirect_target_id {IN_REMOVED} OR canonical_target_id {IN_REMOVED}"),
        ] {
            affected.extend(column_ids(&handle, &sql, &removed_json)?);
        }
        for (table, column) in OWNED {
            db(conn.execute(
                &format!("DELETE FROM {table} WHERE {column} {IN_REMOVED}"),
                [&removed_json],
            ))?;
        }
        for (table, column) in REFERENCES {
            db(conn.execute(
                &format!("UPDATE {table} SET {column} = NULL WHERE {column} {IN_REMOVED}"),
                [&removed_json],
            ))?;
        }
        db(conn.execute(
            &format!(
                "UPDATE urls SET redirect_target_id = NULL WHERE redirect_target_id {IN_REMOVED}"
            ),
            [&removed_json],
        ))?;
        db(conn.execute(
            &format!(
                "UPDATE urls SET canonical_target_id = NULL WHERE canonical_target_id {IN_REMOVED}"
            ),
            [&removed_json],
        ))?;
        // der Trigger entfernt die Zeilen von `url_counters` mit
        db(conn.execute(
            &format!("DELETE FROM urls WHERE id {IN_REMOVED}"),
            [&removed_json],
        ))?;
        // Zähler der Nachbarn in einem Zug neu berechnen und im Änderungsfeed stempeln
        let neighbours: Vec<i64> = affected.difference(&removed).copied().collect();
        let recomputed = os_store::counters::recompute_many(conn, &neighbours)?;
        for (id, c) in recomputed {
            // alles Ganzzahlen: als Literale ins SQL
            db(conn.execute(
                &format!(
                    "UPDATE url_counters SET inlinks = {}, unique_inlinks = {}, self_inlinks = {}, inlinks_all = {}, \
                     outlinks = {}, unique_outlinks = {}, self_outlinks = {}, external_outlinks = {}, \
                     unique_external_outlinks = {}, img_inlinks = {}, html_2xx_sources = {}, \
                     image_link_inlinks = {}, change_seq = {seq} WHERE url_id = {id}",
                    c.inlinks,
                    c.unique_inlinks,
                    c.self_inlinks,
                    c.inlinks_all,
                    c.outlinks,
                    c.unique_outlinks,
                    c.self_outlinks,
                    c.external_outlinks,
                    c.unique_external_outlinks,
                    c.img_inlinks,
                    c.html_2xx_sources,
                    c.image_link_inlinks
                ),
                [],
            ))?;
        }
        handle.set_meta("change_seq", &seq.to_string())?;
        handle.set_meta("reset_seq", &seq.to_string())?;
        mark_analysis_stale(&handle)?;
        Ok(())
    })();
    match result {
        Ok(()) => db(conn.execute_batch("COMMIT")),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// Uhr für die Prüfung in [`remove_urls`] (die Funktion bekommt keine `EngineDeps`).
struct EngineDepsProbe(os_model::SystemClock);

impl EngineDepsProbe {
    fn new() -> Self {
        EngineDepsProbe(os_model::SystemClock)
    }
}

fn ensure_not_running_with(file: &CrawlFile, probe: &EngineDepsProbe) -> Result<(), EngineError> {
    if file.effective_state(&probe.0)? == os_model::CrawlState::Running {
        return Err(EngineError::Invalid(
            "Der Crawl läuft noch – pausieren oder beenden, dann entfernen".to_string(),
        ));
    }
    // 08 AI-020 [M4] (Wirkung b): ein anderer Prozess führt den Crawl als aktiver Besitzer (z. B. KI-Lauf).
    let occupancy = file.occupancy(&probe.0)?;
    if occupancy.foreign_active_owner(std::process::id()) {
        return Err(EngineError::Invalid(format!(
            "Der Crawl läuft noch (Prozess {}) – pausieren oder beenden, dann entfernen",
            occupancy.owner_pid.as_deref().unwrap_or_default()
        )));
    }
    Ok(())
}

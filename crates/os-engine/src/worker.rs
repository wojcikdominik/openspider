//! Worker: Abruf, Auswertung und Schreibmodell je URL (Datenfluss M1-Plan 3.7).
//!
//! Ablauf je URL (`process`):
//! 1. robots.txt prüfen (`RobotsGate`); beim Erstabruf je Origin entsteht eine URL-Zeile für die robots.txt
//!    (`DiscoveryKind::RobotsTxt`, zählt nicht). Gesperrte URL → Zeile mit Status 0, kein Abruf.
//! 2. Abrufen (`Fetch`), Fakten (`FetchFacts`) aus der Antwort bilden.
//! 3. Interne HTML-Seiten parsen (`parse_html`), CSS nach `url()`/`@import` durchsuchen, bei allem anderen nur
//!    die Header auswerten. Externe URLs und 3xx-Bodys werden nie geparst.
//! 4. Weiterleitung bestimmen: HTTP-3xx vor Refresh-Header vor Meta-Refresh.
//! 5. Alle Ziele (Links, Bilder, Canonical, hreflang, Paginierung, Weiterleitung) entdecken: Entscheidung über
//!    `decide::decide`, Include/Exclude, Tiefe; neue URLs bekommen ihre Kennung und werden eingeplant.
//! 6. `PageWrite` samt Kindzeilen und Indexierbarkeit an den Writer.
//!
//! **Reihenfolge im Kanal:** Die neuen URLs (und ihre Queue-Einträge) gehen in einer eigenen Nachricht vor der
//! Seite in den Kanal, und zwar *unter der Sperre* über einen vorab reservierten Platz. So kann kein anderer
//! Worker eine neue URL abrufen und ihre `PageWrite` vor der `NewUrl`-Zeile senden.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use os_fetch::robots::{RobotsDecision, RobotsGate};
use os_fetch::{content_class, Fetch, FetchOutcome, FetchRequest, FetchResult, HttpResponse};
use os_model::{
    CanonicalRow, Clock, ContentClass, CrawlConfig, DirectiveRow, DiscoveryKind, ElementPair,
    ExtractedLink, ExtractedText, FetchFacts, HeaderHints, HeaderRow, HintSource, HreflangRow,
    ImageRow, IndexabilityInput, LinkOrigin, LinkPosition, LinkRow, LinkType, NoResponseReason,
    NotCrawledReason, PageFacts, PageWrite, PaginationRow, ParsedPage, PathType, PdfMeta, QueueOp,
    RedirectType, RefreshRow, RefreshSource, Timestamp, UrlId, WriteBatch,
};
use os_parse::{ParseConfig, ParseInput};
use os_url::{NormUrl, ScopeClass};
use tokio::sync::Notify;
use url::Url;

use crate::decide::{self, LinkDecision};
use crate::diag;
use crate::error::EngineError;
use crate::frontier::{
    Cand, Core, CrawlScope, Purpose, QueueItem, COUNTER_REDIRECT_LIMIT, F_IN_TABLES,
};
use crate::progress::ProgressTracker;
use crate::redirect::{self, RedirectTarget};
use crate::robots_glue::record_robots;
#[cfg(test)]
use crate::sitemap_ingest::build_sitemap_batch;
use crate::sitemap_ingest::ingest_sitemaps;
use crate::writer_thread::{WriterGone, WriterSender};

/// Ab dieser Body-Größe wird auf einem Blocking-Thread geparst.
const BLOCKING_PARSE_THRESHOLD: usize = 1024 * 1024;

/// Steuerung von außen (`CrawlHandle`) und zwischen Worker und Steuerschleife.
pub(crate) type AddRequest = (
    Vec<String>,
    tokio::sync::oneshot::Sender<Result<crate::AddUrlsSummary, EngineError>>,
);

pub(crate) struct Control {
    pub add_urls: tokio::sync::mpsc::UnboundedSender<AddRequest>,
    /// Weckt die Steuerschleife (neue Arbeit, Worker fertig, Einstellung geändert).
    pub notify: Notify,
    /// Gewünschte Zahl paralleler Threads.
    pub threads: AtomicU32,
    /// Obergrenze durch Speicherschutz; u32::MAX bedeutet keine Drosselung.
    pub memory_threads: AtomicU32,
    /// `STOP_*`.
    pub stop: AtomicU8,
    /// Grund einer Pause, die die Engine selbst auslöst (Speicherplatz): `(Code, Meldung)`.
    pub reason: Mutex<Option<(&'static str, String)>>,
}

/// Kein Stopp angefordert.
pub(crate) const STOP_NONE: u8 = 0;
/// Beenden, Warteschlange bleibt erhalten.
pub(crate) const STOP_STOP: u8 = 1;
/// Pausieren (Zustand `Paused`, kein Abschluss-Hook).
pub(crate) const STOP_PAUSE: u8 = 2;

/// CRAWL-075: zero disables the page limit, including transport fallbacks.
pub(crate) fn page_body_limit(cfg: &CrawlConfig) -> u64 {
    match cfg.limits.max_page_kb {
        0 => u64::MAX,
        kb => kb.saturating_mul(1024),
    }
}

/// Zustand, den alle Worker und die Steuerschleife teilen.
pub(crate) struct Shared<F: Fetch> {
    pub render_session: crate::render_glue::Session,
    pub sitemap_ingest_lock: tokio::sync::Mutex<()>,
    pub add_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<AddRequest>>,
    pub fetcher: Arc<F>,
    pub cfg: Arc<CrawlConfig>,
    pub clock: Arc<dyn Clock>,
    pub scope: CrawlScope,
    pub rules: os_url::RuleSet,
    pub gate: RobotsGate,
    pub pcfg: ParseConfig,
    pub core: Mutex<Core>,
    pub ctl: Arc<Control>,
    pub writer: WriterSender,
    pub progress: Mutex<ProgressTracker>,
    /// Ein Worker hat einen nicht behebbaren Fehler gemeldet (oder der Writer ist ausgefallen).
    pub abort: AtomicBool,
    pub error: Mutex<Option<EngineError>>,
    pub host_pacer: crate::host_pace::HostPacer,
    /// URL Encoded Address der Start-URL (von Include/Exclude ausgenommen; im Listenmodus leer).
    pub start_url: String,
    /// Origins, deren robots.txt aus der Crawl-Datei übernommen wurde (Fortsetzen): kein erneuter Abruf, kein
    /// neues Schreiben (P19).
    pub primed_origins: std::collections::HashSet<String>,
}

impl<F: Fetch> Shared<F> {
    /// Meldet einen Fehler; der erste gewinnt, der Crawl endet.
    pub fn fail(&self, e: EngineError) {
        let mut slot = self.error.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_none() {
            *slot = Some(e);
        }
        self.abort.store(true, Ordering::SeqCst);
        self.ctl.notify.notify_one();
    }

    pub(crate) fn lock_core(&self) -> std::sync::MutexGuard<'_, Core> {
        self.core.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn clock_mono(&self) -> std::time::Duration {
        self.clock.monotonic()
    }
}

struct ActiveGuard<F: Fetch> {
    page: bool,
    sh: Arc<Shared<F>>,
}

impl<F: Fetch> Drop for ActiveGuard<F> {
    fn drop(&mut self) {
        self.sh.lock_core().finish_worker(self.page);
        self.sh.ctl.notify.notify_one();
    }
}

/// Bearbeitet eine URL der Warteschlange (der Abruf ist bereits reserviert, `active` erhöht).
pub(crate) async fn process<F: Fetch>(sh: Arc<Shared<F>>, item: QueueItem) {
    let _guard = ActiveGuard {
        page: item.priority == 0,
        sh: Arc::clone(&sh),
    };
    let started = std::time::Instant::now();
    // Keep the per-URL state machine off the polling thread's small stack.
    let outcome = crate::heap(|| run_item(&sh, &item)).await;
    diag::inc(&diag::STATS.items);
    diag::add(&diag::STATS.item_ns, started);
    match outcome {
        Ok(()) => {}
        Err(Failure::Writer) => {
            sh.abort.store(true, Ordering::SeqCst);
        }
        Err(Failure::Engine(e)) => sh.fail(e),
    }
}

pub(crate) enum Failure {
    /// Der Writer ist ausgefallen; die Ursache liefert `WriterThread::finish`.
    Writer,
    Engine(EngineError),
}

impl From<WriterGone> for Failure {
    fn from(_: WriterGone) -> Self {
        Failure::Writer
    }
}

async fn run_item<F: Fetch>(sh: &Arc<Shared<F>>, item: &QueueItem) -> Result<(), Failure> {
    let url = Url::parse(&item.url)
        .map_err(|e| Failure::Engine(EngineError::Invalid(format!("{}: {e}", item.url))))?;
    let cfg = &*sh.cfg;
    let t = std::time::Instant::now();
    let _host = sh.host_pacer.acquire(&url).await;
    diag::add(&diag::STATS.host_wait_ns, t);

    if sh.ctl.stop.load(Ordering::SeqCst) != STOP_NONE {
        sh.lock_core().requeue(item.clone());
        return Ok(());
    }
    // Optional fetcher policy: preserve an explicit target denial instead of masking it as robots failure.
    if crate::heap(|| record_target_denial(sh, item, &url)).await? {
        return Ok(());
    }
    // 1. robots.txt
    let t = std::time::Instant::now();
    let check = crate::heap(|| sh.gate.check(&*sh.fetcher, &url)).await;
    diag::add(&diag::STATS.robots_ns, t);
    let mut sitemap_seeds = Vec::new();
    if let Some((fetch_result, write)) = check.robots_fetch {
        sitemap_seeds = crate::heap(|| record_robots(sh, fetch_result, write)).await?;
    }
    if sh.ctl.stop.load(Ordering::SeqCst) != STOP_NONE {
        sh.lock_core().requeue(item.clone());
        return Ok(());
    }
    // DNS may have changed while robots.txt was fetched; check the policy again before its decision.
    if crate::heap(|| record_target_denial(sh, item, &url)).await? {
        return Ok(());
    }
    match &check.decision {
        RobotsDecision::Blocked { .. } => {
            crate::heap(|| crate::robots_glue::record_blocked(sh, item, &check.decision)).await?;
            return Ok(());
        }
        RobotsDecision::Allowed | RobotsDecision::ReportOnly { .. } => {}
    }

    // 2. Abruf: Tempo-Fachmodul erhält die inzwischen geladene robots.txt sowie die injizierte Uhr.
    sh.writer.wait_for_bytes().await;
    // Stop is checked again after byte backpressure, before opening a request.
    if sh.ctl.stop.load(Ordering::SeqCst) != STOP_NONE {
        sh.lock_core().requeue(item.clone());
        return Ok(());
    }
    sh.host_pacer
        .before_fetch(
            &url,
            sh.gate.crawl_delay(&url.origin().ascii_serialization()),
            cfg,
            &*sh.clock,
        )
        .await;
    if sh.ctl.stop.load(Ordering::SeqCst) != STOP_NONE {
        sh.lock_core().requeue(item.clone());
        return Ok(());
    }
    // CRAWL-113/126: wait for the transport's global start slot here, where pause and stop still
    // cancel the wait. Inside `fetch` the request would start after the pause had been requested.
    let start_delay = sh.fetcher.start_delay();
    if !start_delay.is_zero() {
        sh.host_pacer.sleep_unless_cancelled(start_delay).await;
        if sh.ctl.stop.load(Ordering::SeqCst) != STOP_NONE {
            sh.lock_core().requeue(item.clone());
            return Ok(());
        }
    }
    let cookies = sh.lock_core().session_jar(item.session);
    let t = std::time::Instant::now();
    let result = sh
        .fetcher
        .fetch(FetchRequest {
            url: url.clone(),
            purpose: item.purpose.fetch_purpose(),
            cookies,
            max_body_bytes: page_body_limit(cfg),
            attempt: item.attempt,
            body_limit: os_fetch::BodyLimit {
                max_decoded_bytes: page_body_limit(cfg),
                abort_media_after: Some(64 * 1024),
            },
        })
        .await;
    let fetched = diag::add_max(&diag::STATS.fetch_ns, &diag::STATS.fetch_max_ns, t);
    if fetched >= diag::SLOW_FETCH_NS {
        diag::inc(&diag::STATS.fetch_slow);
    }

    let fetched_bytes = match &result.outcome {
        FetchOutcome::Response(r) => r.body.len() as u64,
        FetchOutcome::NoResponse { detail, .. } => detail.capacity() as u64,
    };
    let mut fetched_charge = sh.writer.fetched_bytes(fetched_bytes);
    let fetch_stage = diag::StageGuard::new(
        &diag::STATS.mem_fetch_entries,
        &diag::STATS.mem_fetch_bytes,
        &diag::STATS.mem_fetch_entries_max,
        &diag::STATS.mem_fetch_bytes_max,
        fetched_bytes,
    );
    sh.host_pacer.on_result(&url, &result, cfg, &*sh.clock);
    if let crate::retry::RetryDecision::RetryAfter(delay) =
        crate::retry::decide(&result, item.attempt, cfg)
    {
        let mut retry = item.clone();
        retry.attempt = retry.attempt.saturating_add(1);
        sh.lock_core()
            .defer(retry, sh.clock_mono().saturating_add(delay));
        sh.ctl.notify.notify_one();
        return Ok(());
    }
    // 3. Auswertung
    let mut parse_stage = diag::StageGuard::new(
        &diag::STATS.mem_parse_entries,
        &diag::STATS.mem_parse_bytes,
        &diag::STATS.mem_parse_entries_max,
        &diag::STATS.mem_parse_bytes_max,
        0,
    );
    let t = std::time::Instant::now();
    let mut analysis = crate::heap(|| analyze(sh, item, &url, &result)).await;
    diag::add(&diag::STATS.analyze_ns, t);
    if let RobotsDecision::ReportOnly {
        would_block,
        matched_line,
        robots_url,
    } = &check.decision
    {
        analysis.facts.robots_blocked = *would_block;
        analysis.facts.robots_matched_line = matched_line.clone();
        analysis.facts.robots_txt_url = Some(robots_url.clone());
    }

    // Analysis owns its derived facts; release the response before producing rows.
    drop(result);
    drop(fetch_stage);
    let analysis_bytes = analysis.estimate_bytes();
    fetched_charge.resize(analysis_bytes);
    parse_stage.resize(analysis_bytes);

    // 4./5. Ziele entdecken und Schreibmodell bilden
    let response_ms = analysis
        .facts
        .response_time_ms
        .filter(|_| analysis.facts.status_code != 0);
    let t = std::time::Instant::now();
    let batch = crate::heap(|| assemble(sh, item, analysis)).await?;
    parse_stage.resize(crate::writer_thread::estimate_bytes(&batch));
    diag::add(&diag::STATS.assemble_ns, t);
    let t = std::time::Instant::now();
    crate::heap(|| sh.writer.send_admitted(batch, fetched_charge)).await?;
    drop(parse_stage);
    diag::add_max(&diag::STATS.send_ns, &diag::STATS.send_max_ns, t);
    sh.progress
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .record_completion(sh.clock_mono(), response_ms);
    sh.ctl.notify.notify_one();

    // 6. Sitemaps aus der robots.txt – nicht mehr, wenn schon pausiert oder gestoppt wird: nach der Pause
    // keine neuen Abrufe (CRAWL-126). Beim Fortsetzen leitet das robots-Gate die Adressen erneut ab.
    if !sitemap_seeds.is_empty() && sh.ctl.stop.load(Ordering::SeqCst) == STOP_NONE {
        crate::heap(|| ingest_sitemaps(sh, sitemap_seeds)).await?;
    }
    Ok(())
}

/// Additive policy hook; no effect for existing Fetch implementations (default: no denial).
async fn record_target_denial<F: Fetch>(
    sh: &Arc<Shared<F>>,
    item: &QueueItem,
    url: &Url,
) -> Result<bool, Failure> {
    let Some(result) = sh.fetcher.target_denial(url).await else {
        return Ok(false);
    };
    let analysis = crate::heap(|| analyze(sh, item, url, &result)).await;
    let batch = crate::heap(|| assemble(sh, item, analysis)).await?;
    crate::heap(|| sh.writer.send(batch)).await?;
    sh.progress
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .record_completion(sh.clock_mono(), None);
    sh.ctl.notify.notify_one();
    Ok(true)
}

// ---------------------------------------------------------------------------------------------------
// robots.txt

/// Legt die URL-Zeile der robots.txt an und gibt die `Sitemap:`-Ausgangspunkte zurück.
/// `PageWrite` ohne Auswertung (robots.txt, Sitemap-Datei).
pub(crate) fn plain_page(id: UrlId, result: &FetchResult, cfg: &CrawlConfig) -> PageWrite {
    let fetch = facts_from_result(result, cfg);
    let (indexability, indexability_status) = os_model::evaluate(&IndexabilityInput {
        fetch: &fetch,
        page: None,
        page_url: result.url.as_str(),
        refresh: &[],
        cfg,
        same_url: os_url::eq_for_compare,
    });
    PageWrite {
        url_id: id,
        replace: false,
        fetch,
        page: None,
        indexability,
        indexability_status,
        links: vec![],
        images: vec![],
        canonicals: vec![],
        directives: vec![],
        refresh: vec![],
        hreflang: vec![],
        pagination: vec![],
        headers: vec![],
        html: None,
    }
}

pub(crate) fn header_rows(result: &FetchResult, cfg: &CrawlConfig) -> Vec<HeaderRow> {
    if !cfg.extraction.http_headers {
        return Vec::new();
    }
    match &result.outcome {
        FetchOutcome::Response(r) => [(true, &r.request_headers), (false, &r.headers)]
            .into_iter()
            .flat_map(|(request, headers)| {
                headers
                    .iter()
                    .enumerate()
                    .map(move |(i, (name, value))| HeaderRow {
                        request,
                        position: i as u32,
                        name: name.clone(),
                        value: value.clone(),
                    })
            })
            .collect(),
        FetchOutcome::NoResponse { .. } => Vec::new(),
    }
}

// ---------------------------------------------------------------------------------------------------
// Fakten aus der Antwort

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Liest ein HTTP-Datum (`Wed, 21 Oct 2015 07:28:00 GMT`, RFC 9110).
pub(crate) fn parse_http_date(s: &str) -> Option<Timestamp> {
    use time::format_description::well_known::Rfc2822;
    time::OffsetDateTime::parse(s.trim(), &Rfc2822)
        .ok()
        .map(|t| Timestamp(t.to_offset(time::UtcOffset::UTC)))
}

/// Bildet die Fakten des Abrufs (CRAWL-011/012/022, K-02: `total_transferred = transferred`).
pub(crate) fn facts_from_result(res: &FetchResult, cfg: &CrawlConfig) -> FetchFacts {
    let mut f = FetchFacts {
        crawl_timestamp: res.started_at,
        ..FetchFacts::default()
    };
    match &res.outcome {
        FetchOutcome::NoResponse { reason, .. } => {
            f.status_code = 0;
            f.status = reason.status_text().to_string();
            f.no_response_reason = Some(*reason);
        }
        FetchOutcome::Response(r) => {
            f.status_code = r.status;
            if r.decode_error.is_some() {
                f.status = NoResponseReason::ConnectionError.status_text().to_string();
                f.no_response_reason = Some(NoResponseReason::ConnectionError);
            } else {
                f.status = r.status_text.clone();
            }
            let ct = header(&r.headers, "content-type").map(str::to_string);
            f.content_class = content_class(ct.as_deref(), cfg.advanced.assume_html);
            f.content_type = ct;
            f.http_version = Some(r.http_version.clone());
            f.size_bytes = if r.truncated {
                r.untruncated_bytes
                    .or(r.content_length)
                    .unwrap_or(r.size_bytes)
            } else {
                r.size_bytes
            };
            f.transferred_bytes = r.transferred_bytes;
            f.total_transferred_bytes = r.transferred_bytes;
            if cfg.extraction.response_time {
                f.response_time_ms = Some(r.response_time_ms);
            }
            if cfg.extraction.last_modified {
                f.last_modified_raw = header(&r.headers, "last-modified").map(str::to_string);
                f.last_modified = f.last_modified_raw.as_deref().and_then(parse_http_date);
            }
            // ONPAGE-018 (K-60): MD5 nur für HTML-Antworten (Inhaltsklasse HTML) mit Status 2xx, sonst leer. Die
            // Referenz hasht außerdem 2xx-Antworten ohne Content-Type (z. B. 204: Hash des leeren Körpers);
            // Fehlerseiten, Weiterleitungen, Bilder, CSS, JS, PDF und Text bleiben leer.
            let ok = (200..300).contains(&r.status);
            if cfg.extraction.hash
                && ok
                && (f.content_class == ContentClass::Html || f.content_type.is_none())
            {
                f.hash_md5 = Some(r.md5_hex.clone());
            }
        }
    }
    if !cfg.extraction.page_size {
        f.size_bytes = 0;
    }
    f
}

// ---------------------------------------------------------------------------------------------------
// Auswertung

#[derive(Default)]
struct Analysis {
    origins: HashMap<(LinkType, String), LinkOrigin>,
    extra: WriteBatch,
    html: Option<os_model::PageHtml>,
    pdf_word_count: Option<u32>,
    facts: FetchFacts,
    headers: Vec<HeaderRow>,
    parsed: Option<ParsedPage>,
    hints: Option<HeaderHints>,
    css_links: Vec<ExtractedLink>,
    pdf: Option<PdfMeta>,
    redirect: Option<RedirectTarget>,
    refresh: Vec<os_model::RefreshRef>,
}

impl Analysis {
    /// Capacity-based estimate of retained parse payload; excludes temporary DOM
    /// nodes and optional nested extension structures, like the writer estimate.
    fn estimate_bytes(&self) -> u64 {
        fn vec_bytes<T>(v: &Vec<T>) -> usize {
            v.capacity() * std::mem::size_of::<T>()
        }
        let opt = |v: &Option<String>| v.as_ref().map_or(0, String::capacity);
        let mut bytes = crate::writer_thread::estimate_bytes(&self.extra) as usize
            + vec_bytes(&self.headers)
            + vec_bytes(&self.css_links)
            + vec_bytes(&self.refresh);
        if let Some(html) = &self.html {
            bytes += html.body.capacity() + opt(&html.encoding);
        }
        for h in &self.headers {
            bytes += h.name.capacity() + h.value.capacity();
        }
        if let Some(p) = &self.parsed {
            bytes += p.base_url.capacity()
                + opt(&p.charset)
                + vec_bytes(&p.links)
                + vec_bytes(&p.images)
                + vec_bytes(&p.canonicals)
                + vec_bytes(&p.directives)
                + vec_bytes(&p.refresh)
                + vec_bytes(&p.hreflang)
                + vec_bytes(&p.pagination)
                + vec_bytes(&p.ext)
                + vec_bytes(&p.structured_data)
                + vec_bytes(&p.sd_findings)
                + vec_bytes(&p.forms)
                + vec_bytes(&p.custom_search)
                + vec_bytes(&p.custom_extraction);
            for texts in [
                &p.titles,
                &p.meta_descriptions,
                &p.meta_keywords,
                &p.h1,
                &p.h2,
            ] {
                bytes += vec_bytes(texts);
                bytes += texts.iter().map(|t| t.text.capacity()).sum::<usize>();
            }
            for l in &p.links {
                bytes += l.href_raw.capacity()
                    + opt(&l.resolved)
                    + opt(&l.anchor)
                    + opt(&l.alt_text)
                    + opt(&l.rel)
                    + opt(&l.target)
                    + l.xpath.capacity();
            }
            for image in &p.images {
                bytes += image.src_raw.capacity()
                    + opt(&image.resolved)
                    + opt(&image.alt)
                    + opt(&image.width_attr)
                    + opt(&image.height_attr)
                    + image.xpath.capacity();
            }
            bytes += p.minhash.as_ref().map_or(0, vec_bytes);
            for (_, value) in &p.ext {
                if let os_model::Value::Text(text) = value {
                    bytes += text.capacity();
                }
            }
        }
        bytes as u64
    }
}

async fn analyze<F: Fetch>(
    sh: &Arc<Shared<F>>,
    item: &QueueItem,
    url: &Url,
    result: &FetchResult,
) -> Analysis {
    let cfg = &*sh.cfg;
    let mut a = Analysis {
        facts: facts_from_result(result, cfg),
        headers: header_rows(result, cfg),
        ..Analysis::default()
    };
    if !cfg.custom_js.snippets.is_empty() {
        a.extra.ext.extend(os_render::snippets::trace(item.id, &[]));
    }
    let FetchOutcome::Response(resp) = &result.outcome else {
        return a;
    };
    a.extra.ext.extend(
        os_parse::response_ext(&os_parse::ResponseInput {
            url,
            internal: item.purpose != Purpose::External,
            purpose: item.purpose.fetch_purpose(),
            status: resp.status,
            headers: &resp.headers,
            body: &resp.body,
            content_length: resp.content_length,
            untruncated_bytes: resp.untruncated_bytes,
            truncated: resp.truncated,
            cfg: &sh.pcfg,
        })
        .into_iter()
        .map(|(key, value)| os_model::ExtWrite {
            url: item.id,
            key,
            value,
        }),
    );
    a.extra.ext.push(os_model::ExtWrite {
        url: item.id,
        key: os_model::ExtKey::from_static("crawl.truncated"),
        value: os_model::Value::Bool(resp.truncated),
    });
    if cfg.extraction.cookies {
        a.extra
            .cookies
            .extend(crate::cookies::collect(item.id, &resp.set_cookies));
    }
    if cfg.extraction.store_html
        && a.facts.content_class == ContentClass::Html
        && resp.decode_error.is_none()
    {
        let (_, encoding) = os_parse::charset::decode(
            &resp.body,
            header(&resp.headers, "content-type"),
            &resp.headers,
        );
        a.html = Some(os_model::PageHtml {
            encoding,
            body: resp.body.to_vec(),
        });
    }
    if resp.attempts > 1 {
        a.extra.ext.push(os_model::ExtWrite {
            url: item.id,
            key: os_model::ExtKey::from_static("http.attempts"),
            value: os_model::Value::Int(i64::from(resp.attempts)),
        });
    }
    // 3xx: nur Location auswerten, der Body wird nie geparst (Q5)
    if redirect::is_redirect_status(resp.status) {
        // ONPAGE-059: Header-Fakten auch neben Location behalten; das Weiterleitungsziel bleibt unverändert.
        a.refresh
            .extend(os_parse::parse_headers(url, &resp.headers, &sh.pcfg).refresh);
        if item.purpose != Purpose::External && a.facts.content_class == ContentClass::Html {
            let hits = os_parse::custom_search::run_on_body(&ParseInput {
                url,
                body: &resp.body,
                status: resp.status,
                content_type: header(&resp.headers, "content-type"),
                headers: &resp.headers,
                cfg: &sh.pcfg,
            });
            a.extra
                .custom_search
                .extend(hits.into_iter().map(|hit| os_model::CustomSearchWrite {
                    url_id: item.id,
                    def_index: hit.def_index,
                    count: hit.count,
                    error: hit.error,
                }));
        }
        a.redirect = redirect::http_target(url, &resp.headers);
        crate::session_guard::observe_redirect(
            &sh.lock_core(),
            a.redirect.as_ref(),
            sh.clock.now(),
            &mut a.extra,
        );
        if resp.synthetic == Some(os_fetch::SyntheticResponse::Hsts) {
            if let Some(target) = &mut a.redirect {
                target.kind = os_model::RedirectType::HstsPolicy;
            }
        }
        return a;
    }
    // defekte Kompression liefert keinen verlässlichen Inhalt
    if resp.decode_error.is_some() {
        return a;
    }
    // Externe URLs werden nicht ausgewertet, mit einer Ausnahme (ONPAGE-058/059): Refresh-Header und
    // Meta-Refresh externer HTML-Seiten. Daraus entstehen Redirect URL/Type wie bei internen Seiten; ob das
    // Ziel abgerufen wird, entscheidet `assemble` nach `decide` (K-40) und `spider.external.crawl`.
    if item.purpose == Purpose::External {
        if a.facts.content_class == ContentClass::Html {
            let refs = external_refresh(sh, url, resp).await;
            a.refresh = refs.clone();
            parsed_refresh(&mut a, url, &refs);
        } else {
            let mut hints = os_parse::parse_headers(url, &resp.headers, &sh.pcfg);
            os_parse::switches::filter_directives(&mut hints.directives, &sh.pcfg);
            if hints.hreflang_truncated > 0 {
                a.extra.ext.push(os_model::ExtWrite {
                    url: item.id,
                    key: os_model::ExtKey::from_static("hreflang.truncated"),
                    value: os_model::Value::Int(i64::from(hints.hreflang_truncated)),
                });
            }
            let refs = hints.refresh;
            a.refresh = refs.clone();
            parsed_refresh(&mut a, url, &refs);
        }
        return a;
    }
    if !cfg.custom_js.snippets.is_empty()
        && cfg.rendering.mode == os_model::RenderingMode::JavaScript
        && !matches!(
            a.facts.content_class,
            ContentClass::Html | ContentClass::Pdf
        )
    {
        let opts = crate::render_glue::options(sh, item, resp);
        if !opts.snippets.snippets.is_empty() {
            match sh.render_session.render(url.as_str(), &opts).await {
                Ok(rendered) => {
                    a.extra
                        .custom_js
                        .extend(os_render::snippets::writes(item.id, &rendered.snippets));
                    a.extra
                        .ext
                        .extend(os_render::snippets::trace(item.id, &rendered.snippets));
                }
                Err(error) => tracing::warn!(%error, "Custom JavaScript render failed"),
            }
        }
    }
    match a.facts.content_class {
        ContentClass::Html => {
            let raw = parse_html_page(sh, url, resp).await;
            a.facts.charset = raw.charset.clone();
            let parsed = if cfg.rendering.mode == os_model::RenderingMode::JavaScript {
                render_page(sh, item, url, resp, raw, &mut a).await
            } else {
                raw
            };
            a.refresh = parsed.refresh.clone();
            parsed_refresh(&mut a, url, &parsed.refresh);
            a.parsed = Some(parsed);
        }
        other => {
            let mut hints = os_parse::parse_headers(url, &resp.headers, &sh.pcfg);
            os_parse::switches::filter_directives(&mut hints.directives, &sh.pcfg);
            if hints.hreflang_truncated > 0 {
                a.extra.ext.push(os_model::ExtWrite {
                    url: item.id,
                    key: os_model::ExtKey::from_static("hreflang.truncated"),
                    value: os_model::Value::Int(i64::from(hints.hreflang_truncated)),
                });
            }
            a.refresh = hints.refresh.clone();
            parsed_refresh(&mut a, url, &hints.refresh);
            match other {
                ContentClass::Css => a.css_links = os_parse::parse_css(url, &resp.body),
                ContentClass::Pdf => {
                    let mut pdf = os_parse::pdf::parse(&resp.body, &sh.pcfg);
                    // M2-D19: Rechtschreib-/Grammatikbefunde des Textlayers nach `spelling_findings`.
                    a.extra
                        .spelling
                        .extend(os_parse::spelling::take_findings(item.id, &mut pdf.ext));
                    a.pdf_word_count = pdf.word_count;
                    a.pdf = Some(pdf.meta);
                    a.css_links.extend(pdf.links);
                    a.extra.ext.extend(pdf.ext.into_iter().map(|(key, value)| {
                        os_model::ExtWrite {
                            url: item.id,
                            key,
                            value,
                        }
                    }));
                }
                _ => {}
            }
            a.css_links.extend(hints.mobile_alternates.clone());
            a.hints = Some(hints);
        }
    }
    a
}

async fn render_page<F: Fetch>(
    sh: &Arc<Shared<F>>,
    item: &QueueItem,
    url: &Url,
    resp: &HttpResponse,
    raw: ParsedPage,
    a: &mut Analysis,
) -> ParsedPage {
    let opts = crate::render_glue::options(sh, item, resp);
    let mut render_stage = diag::StageGuard::new(
        &diag::STATS.mem_render_entries,
        &diag::STATS.mem_render_bytes,
        &diag::STATS.mem_render_entries_max,
        &diag::STATS.mem_render_bytes_max,
        0,
    );
    let outcome = sh.render_session.render(url.as_str(), &opts).await;
    let mut write = os_model::RenderWrite {
        url_id: item.id,
        status: "ok".into(),
        html: None,
        word_count: None,
        console_dropped: 0,
        timings: Default::default(),
        console: vec![],
        blocked: vec![],
        network: vec![],
        screenshots: vec![],
    };
    let page = match outcome {
        Ok(mut rendered) => {
            render_stage.resize(rendered.dom_html.capacity() as u64);
            a.extra
                .custom_js
                .extend(os_render::snippets::writes(item.id, &rendered.snippets));
            if !sh.cfg.custom_js.snippets.is_empty() {
                a.extra
                    .ext
                    .extend(os_render::snippets::trace(item.id, &rendered.snippets));
            }
            if let Some(payload) = &rendered.accessibility {
                match os_render::axe::decode(payload, item.id) {
                    Ok(result) => a.extra.a11y.push(result),
                    Err(error) => tracing::warn!(%error, "Invalid accessibility result"),
                }
            }
            if let Some(network) = rendered.body_network.take() {
                rendered.network = network;
                for (url, facts) in &mut rendered.images {
                    facts.bytes = rendered
                        .network
                        .iter()
                        .rev()
                        .find(|entry| &entry.url == url)
                        .map(|entry| entry.bytes);
                }
            }
            if let Some(jar) = crate::render_glue::cookie_jar(sh, item.session) {
                if let Some(cookies) = &rendered.cookies {
                    crate::render_glue::sync_cookies(&jar, &opts.cookies, cookies);
                } else {
                    jar.store_set_cookie(
                        url,
                        rendered.cookie_assignments.iter().map(String::as_str),
                    );
                }
            }
            if sh.cfg.extraction.cookies {
                let cookie_offset = a
                    .extra
                    .cookies
                    .iter()
                    .map(|c| c.position)
                    .max()
                    .map_or(0, |n| n.saturating_add(1));
                a.extra.cookies.extend(
                    crate::cookies::collect(item.id, &rendered.cookie_assignments)
                        .into_iter()
                        .map(|mut cookie| {
                            cookie.origin = os_model::CookieOrigin::JavaScript;
                            cookie.position += cookie_offset;
                            cookie
                        }),
                );
            }
            // UTF-8 serialization must not inherit the original response charset.
            let headers: Vec<_> = resp
                .headers
                .iter()
                .filter(|(k, _)| !k.eq_ignore_ascii_case("content-type"))
                .cloned()
                .collect();
            let dom_url = Url::parse(&rendered.final_url).unwrap_or_else(|_| url.clone());
            let mut dom = parse_now(
                &dom_url,
                rendered.dom_html.as_bytes(),
                resp.status,
                Some("text/html; charset=utf-8"),
                &headers,
                &sh.pcfg,
            );
            // Relative DOM links follow document.baseURI (including pushState). HTTP
            // annotations still belong to the original fetched URL (TECH-006).
            dom.canonicals.retain(|c| c.source != HintSource::Http);
            dom.canonicals.extend(
                raw.canonicals
                    .iter()
                    .filter(|c| c.source == HintSource::Http)
                    .cloned(),
            );
            dom.hreflang.retain(|h| h.source != HintSource::Http);
            dom.hreflang.extend(
                raw.hreflang
                    .iter()
                    .filter(|h| h.source == HintSource::Http)
                    .cloned(),
            );
            dom.pagination.retain(|p| p.source != HintSource::Http);
            dom.pagination.extend(
                raw.pagination
                    .iter()
                    .filter(|p| p.source == HintSource::Http)
                    .cloned(),
            );
            dom.refresh.retain(|r| r.source != RefreshSource::Header);
            dom.refresh.extend(
                raw.refresh
                    .iter()
                    .filter(|r| r.source == RefreshSource::Header)
                    .cloned(),
            );
            dom.links.retain(|l| l.doc_order != u32::MAX);
            dom.links.extend(
                raw.links
                    .iter()
                    .filter(|l| l.doc_order == u32::MAX)
                    .cloned(),
            );
            a.extra
                .ext
                .extend(crate::render_glue::comparison(&raw, &dom).into_iter().map(
                    |(key, value)| os_model::ExtWrite {
                        url: item.id,
                        key,
                        value,
                    },
                ));
            a.origins = crate::render_glue::merge(&raw, &mut dom);
            // Keep raw noindex authoritative even when a script removes it (TECH-006/105).
            if !raw.effective_index {
                dom.effective_index = false;
                for d in &raw.directives {
                    if d.applies
                        && d.tokens.iter().any(|t| t == "noindex")
                        && !dom.directives.contains(d)
                    {
                        dom.directives.push(d.clone());
                    }
                }
            }
            // TECH-106: original nofollow/none remains authoritative after rendering.
            if !raw.effective_follow {
                dom.effective_follow = false;
                for d in &raw.directives {
                    if d.applies
                        && d.tokens.iter().any(|t| t == "nofollow" || t == "none")
                        && !dom.directives.contains(d)
                    {
                        dom.directives.push(d.clone());
                    }
                }
            }
            let raw_images: std::collections::HashSet<_> =
                raw.images.iter().map(|i| i.resolved.clone()).collect();
            let dom_images: std::collections::HashSet<_> =
                dom.images.iter().map(|i| i.resolved.clone()).collect();
            let mut facts = rendered.images.iter().collect::<Vec<_>>();
            for image in &mut dom.images {
                image.origin = if raw_images.contains(&image.resolved) {
                    LinkOrigin::Both
                } else {
                    LinkOrigin::RenderedHtml
                };
                if let Some(index) = facts
                    .iter()
                    .position(|(target, _)| image.resolved.as_deref() == Some(target.as_str()))
                {
                    image.render = Some(facts.remove(index).1.clone());
                }
            }
            dom.images.extend(
                raw.images
                    .iter()
                    .filter(|i| !dom_images.contains(&i.resolved))
                    .cloned(),
            );
            crate::render_glue::add_resource_images(&mut dom, &rendered, &sh.cfg, &mut a.origins);
            // A document navigation is distinct from a history URL change.
            if let Some(target) = rendered
                .navigation_url
                .as_deref()
                .and_then(|s| Url::parse(s).ok())
            {
                if !os_url::eq_for_compare(url.as_str(), target.as_str())
                    && matches!(target.scheme(), "http" | "https")
                {
                    let refresh = redirect::refresh_target(url, &raw.refresh);
                    a.redirect = refresh
                        .redirect
                        .or(refresh.self_refresh)
                        .filter(|r| os_url::eq_for_compare(&r.target.encoded, target.as_str()))
                        .or_else(|| {
                            Some(RedirectTarget {
                                kind: RedirectType::JavaScript,
                                delay: None,
                                raw: target.to_string(),
                                target: crate::fragments::normalize(
                                    &target,
                                    sh.cfg.advanced.crawl_fragments,
                                ),
                                xpath: None,
                            })
                        });
                }
            }
            let mut resources = HashMap::<&str, u64>::new();
            for entry in &rendered.network {
                if !os_url::eq_for_compare(&entry.url, url.as_str()) {
                    resources
                        .entry(&entry.url)
                        .and_modify(|n| *n = (*n).max(entry.bytes))
                        .or_insert(entry.bytes);
                }
            }
            a.facts.total_transferred_bytes = resources
                .values()
                .fold(a.facts.transferred_bytes, |sum, n| sum.saturating_add(*n));
            write.status = if rendered.load_timed_out {
                a.facts.status = "Rendering fehlgeschlagen".into();
                "timeout"
            } else {
                "ok"
            }
            .into();
            write.html = sh
                .cfg
                .extraction
                .store_rendered_html
                .then(|| rendered.dom_html.into_bytes());
            write.word_count = dom.word_count;
            write.timings = rendered.timings;
            if sh.cfg.rendering.js_error_reporting {
                write.console = rendered.console;
                write.console_dropped = rendered.console_dropped;
            }
            write.blocked = rendered.blocked;
            write.network = rendered.network;
            if sh.cfg.rendering.screenshots.enabled {
                write.screenshots = rendered.screenshot_tiles;
            }
            dom
        }
        Err(error) => {
            // Test stdout is captured by the runner and shown on failure. Keep
            // protocol details out of persisted crawl data and production logs.
            #[cfg(feature = "testing")]
            eprintln!("render failed for {url}: {error:?}");
            write.status = match error {
                os_render::RenderError::Timeout(_) => "timeout",
                os_render::RenderError::Crashed(_) => "crashed",
                _ => "failed",
            }
            .into();
            a.facts.status = "Rendering fehlgeschlagen".into();
            // Error details deliberately exclude URLs/credentials from persistent logs.
            raw
        }
    };
    a.extra.render.push(write);
    page
}

fn parsed_refresh(a: &mut Analysis, url: &Url, refs: &[os_model::RefreshRef]) {
    let outcome = redirect::refresh_target(url, refs);
    a.facts.meta_refresh_delay = outcome.delay;
    // K-57: der Selbst-Refresh ist eine Weiterleitung auf sich selbst (Typ, Redirect URL, Linkzeile, Schleife)
    if a.redirect.is_none() {
        a.redirect = outcome.redirect.or(outcome.self_refresh);
    }
}

/// Refresh-Angaben einer externen HTML-Seite: der `Refresh`-Header vor den Meta-Tags (Quelltextreihenfolge).
/// Der Rest der Seite wird nicht ausgewertet.
async fn external_refresh<F: Fetch>(
    sh: &Arc<Shared<F>>,
    url: &Url,
    resp: &HttpResponse,
) -> Vec<os_model::RefreshRef> {
    let mut refs = os_parse::parse_headers(url, &resp.headers, &sh.pcfg).refresh;
    let content_type = header(&resp.headers, "content-type").map(str::to_string);
    let found = if resp.body.len() < BLOCKING_PARSE_THRESHOLD {
        meta_refresh(url, &resp.body, content_type.as_deref(), &resp.headers)
    } else {
        let url = url.clone();
        let body: Bytes = resp.body.clone();
        let headers = resp.headers.clone();
        tokio::task::spawn_blocking(move || {
            meta_refresh(&url, &body, content_type.as_deref(), &headers)
        })
        .await
        .unwrap_or_default()
    };
    let shift = refs.len() as u32;
    refs.extend(found.into_iter().map(|mut r| {
        r.order = r.order.saturating_add(shift);
        r
    }));
    refs
}

fn meta_refresh(
    url: &Url,
    body: &[u8],
    content_type: Option<&str>,
    headers: &[(String, String)],
) -> Vec<os_model::RefreshRef> {
    let (html, _) = os_parse::charset::decode(body, content_type, headers);
    let doc = os_parse::dom::Document::parse(&html);
    let base = os_parse::base::resolve_base(&doc, url);
    os_parse::refresh::extract(&doc, &base)
}

async fn parse_html_page<F: Fetch>(
    sh: &Arc<Shared<F>>,
    url: &Url,
    resp: &HttpResponse,
) -> ParsedPage {
    let content_type = header(&resp.headers, "content-type").map(str::to_string);
    if resp.body.len() < BLOCKING_PARSE_THRESHOLD {
        return parse_now(
            url,
            &resp.body,
            resp.status,
            content_type.as_deref(),
            &resp.headers,
            &sh.pcfg,
        );
    }
    let url = url.clone();
    let body: Bytes = resp.body.clone();
    let headers = resp.headers.clone();
    let pcfg = sh.pcfg.clone();
    let status = resp.status;
    tokio::task::spawn_blocking(move || {
        parse_now(
            &url,
            &body,
            status,
            content_type.as_deref(),
            &headers,
            &pcfg,
        )
    })
    .await
    .unwrap_or_default()
}

fn parse_now(
    url: &Url,
    body: &[u8],
    status: u16,
    content_type: Option<&str>,
    headers: &[(String, String)],
    cfg: &ParseConfig,
) -> ParsedPage {
    let mut detection = cfg.clone();
    detection.crawl.spider.store_uncrawlable_links = true;
    let mut page = os_parse::parse_html(&ParseInput {
        status,
        url,
        body,
        content_type,
        headers,
        cfg: &detection,
    });
    if !cfg.crawl.spider.store_uncrawlable_links {
        for link in &mut page.links {
            if matches!(
                link.uncrawlable_reason,
                Some(
                    os_model::UncrawlableReason::AnchorEmptyHref
                        | os_model::UncrawlableReason::AnchorFragmentOnly
                )
            ) {
                link.crawlable = true;
                link.uncrawlable_reason = None;
            }
        }
    }
    page.links
        .retain(|l| l.crawlable || cfg.crawl.spider.store_uncrawlable_links);
    page.links
        .extend(os_parse::alternates::from_headers(url, headers, cfg));
    page.links.sort_by_key(|l| l.doc_order);
    let max = cfg.crawl.limits.max_links_per_url;
    let total = page.links.len()
        + page.canonicals.len()
        + page.hreflang.len()
        + page.pagination.len()
        + page.refresh.len();
    let html = if max != 0 && total > max as usize {
        os_parse::charset::decode(body, content_type, headers).0
    } else {
        String::new()
    };
    os_parse::switches::limit(&mut page, &html, max);
    #[cfg(test)]
    let page = tests::PARSER_STUB.with(|stub| {
        let mut page = page;
        if let Some(stub) = stub.get() {
            stub(&mut page);
        }
        page
    });
    page
}

// ---------------------------------------------------------------------------------------------------
// Ziele

/// Eine Referenz auf ein Ziel (Link, Bild, Canonical, hreflang, Paginierung).
struct Ref {
    ext: ExtractedLink,
    /// Zeile in `links` anlegen (Links aus dem Dokument und synthetische für Canonical, hreflang …).
    row: bool,
    norm: Option<usize>,
    decision: LinkDecision,
    /// Security outlinks remain inspectable without enabling srcset discovery.
    captured_only: bool,
}

struct Targets {
    list: Vec<(NormUrl, bool)>,
    index: HashMap<String, usize>,
}

impl Targets {
    fn new() -> Self {
        Targets {
            list: Vec::new(),
            index: HashMap::new(),
        }
    }

    fn get<F: Fetch>(&mut self, sh: &Shared<F>, resolved: &str) -> Option<usize> {
        if let Some(i) = self.index.get(resolved) {
            return Some(*i);
        }
        let rewritten = os_url::rewrite::apply_discovered(&sh.cfg.rewrite, resolved);
        let url = Url::parse(&rewritten.url).ok()?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return None;
        }
        let norm = crate::fragments::normalize(&url, sh.cfg.advanced.crawl_fragments);
        let internal = sh.scope.classify(&norm) == ScopeClass::Internal;
        self.list.push((norm, internal));
        let i = self.list.len() - 1;
        self.index.insert(resolved.to_string(), i);
        Some(i)
    }
}

fn outside_start_folder<F: Fetch>(sh: &Shared<F>, norm: &NormUrl) -> bool {
    if !sh.cfg.scope.subfolder_only || sh.start_url.is_empty() {
        return false;
    }
    let Ok(start) = Url::parse(&sh.start_url) else {
        return false;
    };
    let path = start.path();
    let folder = path.rfind('/').map_or("/", |i| &path[..=i]);
    !norm.url.path().starts_with(folder)
}

fn path_type_of(raw: &str) -> PathType {
    let t = raw.trim();
    if t.starts_with("//") {
        PathType::ProtocolRelative
    } else if t.contains("://") {
        PathType::Absolute
    } else if t.starts_with('/') {
        PathType::RootRelative
    } else {
        PathType::PathRelative
    }
}

pub(crate) fn synth_link(link_type: LinkType, raw: &str, resolved: Option<&str>) -> ExtractedLink {
    synth_link_at(link_type, raw, resolved, None, true)
}

/// Wie `synth_link` für Verweise aus einem Element des Dokuments: Pfad `xpath` (`//head/link[3]` im Kopf,
/// OUT-031/TECH-131, K-63) und Position `Head` für Elemente im Kopf. Header-Quellen und Sitemaps haben keinen Pfad
/// (die Linkzeile bleibt ohne Pfad und Position).
fn synth_link_at(
    link_type: LinkType,
    raw: &str,
    resolved: Option<&str>,
    xpath: Option<&str>,
    in_head: bool,
) -> ExtractedLink {
    ExtractedLink {
        link_type,
        href_raw: raw.to_string(),
        resolved: resolved.map(str::to_string),
        anchor: None,
        alt_text: None,
        rel: None,
        nofollow_rel: false,
        target: None,
        path_type: path_type_of(raw),
        xpath: xpath.unwrap_or_default().to_string(),
        position: if xpath.is_some() && in_head {
            LinkPosition::Head
        } else {
            LinkPosition::Content
        },
        in_head,
        doc_order: 0,
        ..Default::default()
    }
}

/// Maßgebliche Canonical-URL (TECH-017): erster gültiger HTTP-Header-Eintrag, sonst erstes gültiges
/// HTML-Element im Head; problematische Annotationen bleiben unberücksichtigt.
pub(crate) fn pick_canonical(canonicals: &[os_model::CanonicalRef]) -> Option<usize> {
    let valid = |c: &os_model::CanonicalRef| {
        c.resolved.is_some() && !c.invalid_attribute && (c.source != HintSource::Html || c.in_head)
    };
    canonicals
        .iter()
        .position(|c| c.source == HintSource::Http && valid(c))
        .or_else(|| {
            canonicals
                .iter()
                .position(|c| c.source == HintSource::Html && c.in_head && valid(c))
        })
}

/// Wo in der Referenzliste welche Art von Verweis steht.
#[derive(Default)]
struct RefMap {
    links: Vec<usize>,
    canonicals: Vec<usize>,
    hreflang: Vec<usize>,
    pagination: Vec<usize>,
}

fn element_pair(texts: &[ExtractedText], enabled: bool) -> ElementPair {
    if !enabled {
        return ElementPair::default();
    }
    ElementPair {
        count: texts.len() as u32,
        first: texts.first().cloned(),
        second: texts.get(1).cloned(),
    }
}

fn pdf_pair(text: Option<&str>) -> ElementPair {
    match text.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => ElementPair {
            count: 1,
            first: Some(ExtractedText {
                text: t.to_string(),
                len: t.chars().count() as u32,
                px: None,
                in_head: false,
            }),
            second: None,
        },
        None => ElementPair::default(),
    }
}

/// Entdeckt die Ziele und bildet das Schreibmodell der Seite.
async fn assemble<F: Fetch>(
    sh: &Arc<Shared<F>>,
    item: &QueueItem,
    a: Analysis,
) -> Result<WriteBatch, Failure> {
    let cfg = &*sh.cfg;
    let Analysis {
        origins,
        mut extra,
        html,
        pdf_word_count,
        mut facts,
        headers,
        parsed,
        hints,
        css_links,
        pdf,
        mut redirect,
        refresh,
    } = a;
    if let Some(page) = &parsed {
        transport_page(item.id, page, &mut extra);
    }
    if let Some(target) = &mut redirect {
        let rewritten = os_url::rewrite::apply_discovered(&cfg.rewrite, &target.target.encoded);
        if let Ok(url) = Url::parse(&rewritten.url) {
            target.target = os_url::normalize(&url);
        }
    }
    let page_follow = parsed.as_ref().is_none_or(|p| p.effective_follow);

    let mut targets = Targets::new();
    let mut refs: Vec<Ref> = Vec::new();
    let mut map = RefMap::default();
    let add_ref =
        |refs: &mut Vec<Ref>, targets: &mut Targets, mut ext: ExtractedLink, row: bool| {
            let norm = ext.resolved.as_deref().and_then(|r| targets.get(&**sh, r));
            // Non-HTTP schemes have no URL row, but retain their raw link instance
            // when uncrawlable-link capture is enabled (CRAWL-053 / TECH-143).
            if norm.is_none()
                && Url::parse(ext.href_raw.trim()).is_ok_and(|url| {
                    matches!(
                        url.scheme(),
                        "mailto" | "tel" | "javascript" | "data" | "about" | "file"
                    )
                })
            {
                ext.crawlable = false;
            }
            let scope = match norm.map(|i| targets.list[i].1) {
                Some(true) => ScopeClass::Internal,
                _ => ScopeClass::External,
            };
            let decision = decide::decide(&ext, scope, cfg, page_follow);
            refs.push(Ref {
                ext,
                row,
                norm,
                decision,
                captured_only: false,
            });
            refs.len() - 1
        };

    // Links aus dem Dokument (HTML) bzw. aus der CSS-Datei
    if let Some(p) = &parsed {
        for l in &p.links {
            let i = add_ref(&mut refs, &mut targets, l.clone(), true);
            map.links.push(i);
        }
        // ONPAGE-033/037 inspect every resource reference even when CRAWL-055
        // disables discovery of additional image URLs. The parser retains image
        // instances in this mode; keep each raw candidate as a hidden, unqueued
        // target rather than enabling the extraction switch behind the user's back.
        if !cfg.extraction.srcset {
            for image in p
                .images
                .iter()
                .filter(|im| im.source == os_model::ImageSource::Srcset)
            {
                let ext = ExtractedLink {
                    link_type: LinkType::Image,
                    href_raw: image.src_raw.clone(),
                    resolved: image.resolved.clone(),
                    alt_text: image.alt.clone(),
                    path_type: path_type_of(&image.src_raw),
                    xpath: image.xpath.clone(),
                    position: image.link_position.clone().unwrap_or_default(),
                    doc_order: image.doc_order,
                    ..Default::default()
                };
                let i = add_ref(&mut refs, &mut targets, ext, true);
                refs[i].captured_only = true;
                refs[i].decision.crawl = false;
                map.links.push(i);
            }
        }
    }
    for l in &css_links {
        let i = add_ref(&mut refs, &mut targets, l.clone(), true);
        map.links.push(i);
    }
    // Canonical, hreflang, Paginierung (Dokument und Header)
    let (mut canonicals, mut hreflang, pagination, directives) = match (&parsed, &hints) {
        (Some(p), _) => (
            p.canonicals.clone(),
            p.hreflang.clone(),
            p.pagination.clone(),
            p.directives.clone(),
        ),
        (None, Some(h)) => (
            h.canonicals.clone(),
            h.hreflang.clone(),
            h.pagination.clone(),
            h.directives.clone(),
        ),
        (None, None) => Default::default(),
    };
    // SQ-04: Problembelege bleiben erhalten, wirksame Annotationen und Links nicht.
    let canonical_problems = os_parse::canonical::annotation_facts(&canonicals);
    let hreflang_outside = hreflang
        .iter()
        .any(|h| h.source == HintSource::Html && !h.in_head);
    canonicals.retain(|c| !c.invalid_attribute && (c.source != HintSource::Html || c.in_head));
    hreflang.retain(|h| h.source != HintSource::Html || h.in_head);
    for c in &canonicals {
        let lt = match c.source {
            HintSource::Http => LinkType::HttpCanonical,
            _ => LinkType::Canonical,
        };
        let ext = synth_link_at(
            lt,
            &c.raw,
            c.resolved.as_deref(),
            c.xpath.as_deref(),
            c.in_head,
        );
        let i = add_ref(&mut refs, &mut targets, ext, true);
        map.canonicals.push(i);
    }
    for h in &hreflang {
        let lt = match h.source {
            HintSource::Http => LinkType::HttpHreflang,
            _ => LinkType::Hreflang,
        };
        let ext = synth_link_at(
            lt,
            &h.href_raw,
            h.resolved.as_deref(),
            h.xpath.as_deref(),
            h.in_head,
        );
        let i = add_ref(&mut refs, &mut targets, ext, true);
        map.hreflang.push(i);
    }
    for p in &pagination {
        let lt = if p.rel_next {
            LinkType::RelNext
        } else {
            LinkType::RelPrev
        };
        let ext = synth_link_at(
            lt,
            &p.raw,
            p.resolved.as_deref(),
            p.xpath.as_deref(),
            p.in_head,
        );
        let i = add_ref(&mut refs, &mut targets, ext, true);
        map.pagination.push(i);
    }
    // Bildinstanzen: nur nachschlagen (die Bild-Links stehen schon in `links`)
    let image_targets: Vec<(Option<usize>, bool)> = parsed
        .as_ref()
        .map(|p| {
            p.images
                .iter()
                .map(|im| {
                    (
                        im.resolved.as_deref().and_then(|r| targets.get(&**sh, r)),
                        !cfg.extraction.srcset && im.source == os_model::ImageSource::Srcset,
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // Weiterleitung: Entscheidung und Limit (CRAWL-072)
    let redirect_plan = redirect.as_ref().map(|r| {
        let internal = sh.scope.classify(&r.target) == ScopeClass::Internal;
        targets.list.push((r.target.clone(), internal));
        (r, targets.list.len() - 1)
    });

    // Discovery unter der Sperre, Nachricht mit den neuen URLs vor der Seite in den Kanal
    let depth = item.depth.map(|d| d + 1);
    let t = std::time::Instant::now();
    let slot = sh.writer.reserve_admitted().await?;
    diag::add_max(&diag::STATS.reserve_ns, &diag::STATS.reserve_max_ns, t);
    let mut disc = WriteBatch::default();
    let mut ids: Vec<Option<UrlId>> = Vec::with_capacity(refs.len());
    // Referenzen ohne Linkzeile: nur per hreflang bekannte Ziele ohne Tabellen-Sichtbarkeit (K-62)
    let mut hidden_rows = vec![false; refs.len()];
    let mut image_ids: Vec<Option<UrlId>> = Vec::new();
    let mut redirect_id: Option<UrlId> = None;
    // Weiterleitungs-Flags (CRAWL-017, CRAWL-072) gehen mit der Seite in den Writer (M1.md 3.11).
    let mut url_flags: Vec<(UrlId, String, Option<String>)> = Vec::new();
    {
        let mut core = sh.lock_core();
        for (ri, r) in refs.iter().enumerate() {
            let d = &r.decision;
            let Some(ti) = r.norm else {
                ids.push(None);
                continue;
            };
            if !d.store && !d.crawl {
                ids.push(None);
                continue;
            }
            let (norm, internal) = &targets.list[ti];
            let lt = r.ext.link_type;
            let resource_like = lt.is_resource() || lt == LinkType::Other;
            let outside_folder = *internal && !resource_like && outside_start_folder(sh, norm);
            let follow_canonical = cfg.advanced.always_follow_canonicals
                && matches!(lt, LinkType::Canonical | LinkType::HttpCanonical);
            let canonical_limit = follow_canonical
                && norm.encoded != *item.url
                && item.hops >= cfg.limits.max_redirects;
            if canonical_limit {
                url_flags.push((item.id, redirect::FLAG_LIMIT.to_string(), None));
            }
            let purpose = if follow_canonical {
                Purpose::Page
            } else if !*internal || outside_folder {
                Purpose::External
            } else if resource_like {
                Purpose::Resource
            } else {
                Purpose::Page
            };
            // K-62 (N-08): ein nur per hreflang bekanntes Ziel bekommt eine Zeile, aber keine Tabellen-Sichtbarkeit;
            // seine hreflang-Linkzeile entsteht nur, wenn das Ziel schon anderweitig in den Tabellen steht.
            let hreflang_only = decide::is_hreflang_store_only(lt, d);
            let cand = Cand {
                norm,
                internal: *internal,
                kind: DiscoveryKind::Link,
                store: d.store
                    && !r.captured_only
                    && !hreflang_only
                    && (follow_canonical
                        || !outside_folder
                        || cfg.scope.crawl_outside_start_folder),
                crawl: d.crawl
                    && !canonical_limit
                    && (follow_canonical
                        || !outside_folder
                        || cfg.scope.crawl_outside_start_folder),
                reason: if canonical_limit {
                    Some(NotCrawledReason::RedirectLimit)
                } else if !follow_canonical
                    && outside_folder
                    && !cfg.scope.crawl_outside_start_folder
                {
                    Some(NotCrawledReason::OutsideScope)
                } else {
                    d.reason
                },
                resource_like,
                apply_rules: norm.encoded != sh.start_url,
                purpose,
                source: Some((item.id, lt, r.ext.anchor.clone())),
            };
            let hops = if follow_canonical {
                item.hops.saturating_add(1)
            } else {
                0
            };
            let id = core.discover(&cand, depth, hops, item.session, &sh.rules, &mut disc);
            if hreflang_only
                && !core
                    .lookup(&norm.encoded)
                    .is_some_and(|s| s.flags & F_IN_TABLES != 0)
            {
                hidden_rows[ri] = true;
            }
            ids.push(Some(id));
        }
        for (t, captured_only) in &image_targets {
            let id = t
                .and_then(|ti| core.lookup(&targets.list[ti].0.encoded))
                // Security outlinks may reference hidden URL rows without
                // turning disabled srcset candidates into extracted images.
                // A URL discovered visibly elsewhere keeps its existing ID.
                .filter(|s| !captured_only || s.flags & F_IN_TABLES != 0)
                .map(|s| s.id);
            image_ids.push(id);
        }
        if let Some((r, ti)) = &redirect_plan {
            let (norm, internal) = &targets.list[*ti];
            let (store, mut crawl, mut reason) = match r.kind {
                RedirectType::Http | RedirectType::JavaScript => (true, true, None),
                kind => {
                    // Meta-/Header-Refresh hängen am Schalter „Meta Refresh“ (K-40); Grund und Außenwirkung
                    // wie bei jedem anderen Linktyp aus der Matrix.
                    let link_type = if kind == RedirectType::MetaRefresh {
                        LinkType::MetaRefresh
                    } else {
                        LinkType::HttpRefresh
                    };
                    let ext = synth_link(link_type, &r.raw, Some(&norm.encoded));
                    let class = if *internal {
                        ScopeClass::Internal
                    } else {
                        ScopeClass::External
                    };
                    let d = decide::decide(&ext, class, cfg, true);
                    (d.store, d.crawl, d.reason)
                }
            };
            let internal = *internal;
            // K-57: Selbst-Refresh = Weiterleitung auf sich selbst. Das Ziel ist die eigene, bekannte URL: kein
            // Abruf, keine Entdeckung, nur die Schleife (Redirect Loop, Redirect-Chains-Zeile).
            if norm.encoded == *item.url {
                if store {
                    redirect_id = Some(item.id);
                    if let Some(members) = core.redirects.add_edge(item.id, item.id) {
                        url_flags.extend(
                            members
                                .into_iter()
                                .map(|m| (m, redirect::FLAG_LOOP.to_string(), None)),
                        );
                    }
                }
            } else {
                if !internal
                    && crawl
                    && !cfg.spider.external.crawl
                    && !cfg.advanced.always_follow_redirects
                {
                    crawl = false;
                    reason = Some(NotCrawledReason::ExternalNotChecked);
                }
                let limit_hit = item.hops >= cfg.limits.max_redirects;
                if limit_hit {
                    crawl = false;
                    reason = Some(NotCrawledReason::RedirectLimit);
                    disc.counters.push((COUNTER_REDIRECT_LIMIT.to_string(), 1));
                    core.redirects.flag_limit(item.id);
                    url_flags.push((item.id, redirect::FLAG_LIMIT.to_string(), None));
                }
                if store || crawl || limit_hit {
                    let cand = Cand {
                        norm,
                        internal,
                        kind: DiscoveryKind::Redirect,
                        store: store && !limit_hit,
                        crawl,
                        reason,
                        resource_like: false,
                        apply_rules: norm.encoded != sh.start_url,
                        purpose: if internal || cfg.advanced.always_follow_redirects {
                            Purpose::Page
                        } else {
                            Purpose::External
                        },
                        source: None,
                    };
                    let id = core.discover(
                        &cand,
                        depth,
                        item.hops + 1,
                        item.session,
                        &sh.rules,
                        &mut disc,
                    );
                    if let Some(members) = core.redirects.add_edge(item.id, id) {
                        url_flags.extend(
                            members
                                .into_iter()
                                .map(|m| (m, redirect::FLAG_LOOP.to_string(), None)),
                        );
                    }
                    redirect_id = Some(id);
                }
            }
        }
        // Der Batch mit den neuen Warteschlangeneinträgen liegt im Kanal, bevor die Sperre fällt: Das RAM-Fenster der
        // Warteschlange (K-78 b) verlässt sich darauf, dass jeder eingeplante Eintrag nach einem `flush` in der Datei
        // steht. `Slot::send` wartet nie.
        slot.send(disc);
    }
    sh.ctl.notify.notify_one();

    // Schreibmodell
    if let Some((r, ti)) = &redirect_plan {
        facts.redirect_url = Some(targets.list[*ti].0.encoded.clone());
        facts.redirect_type = Some(r.kind);
        facts.redirect_target = redirect_id;
    }
    let mut batch = extra;
    if cfg.spider.canonicals.store {
        batch.ext.extend(
            canonical_problems
                .into_iter()
                .map(|(key, value)| os_model::ExtWrite {
                    url: item.id,
                    key,
                    value,
                }),
        );
    }
    if cfg.spider.hreflang.store && hreflang_outside {
        batch.ext.push(os_model::ExtWrite {
            url: item.id,
            key: os_model::ExtKey::from_static("hreflang.outside_head"),
            value: os_model::Value::Bool(true),
        });
    }
    let own = item.url.as_ref();
    // INTEG-083: „Link-Positionen speichern“ aus → Linkzeilen ohne `Link Path` und `Link Position`.
    let store_link_paths = cfg.link_positions.store;
    let mut links: Vec<LinkRow> = Vec::new();
    let mut position = 0u32;
    let mut push_row = |links: &mut Vec<LinkRow>, r: &Ref, id: Option<UrlId>, follow: bool| {
        let e = &r.ext;
        links.push(LinkRow {
            target_id: id,
            // K-37: der Rohwert wie im Markup; das aufgelöste Ziel steht über `target_id`.
            target_raw: e.href_raw.clone(),
            link_type: e.link_type,
            position,
            anchor: e.anchor.clone(),
            alt_text: e.alt_text.clone(),
            follow,
            rel: e.rel.clone(),
            target_attr: e.target.clone(),
            path_type: Some(e.path_type),
            link_path: (store_link_paths && !e.xpath.is_empty()).then(|| e.xpath.clone()),
            link_position: (store_link_paths && !e.xpath.is_empty()).then(|| e.position.clone()),
            link_origin: origins
                .get(&crate::render_glue::link_key(e))
                .copied()
                .unwrap_or(LinkOrigin::Html),
            in_head: Some(e.in_head),
            crawlable: e.crawlable,
            uncrawlable_reason: e.uncrawlable_reason,
            pdf_page: e.pdf_page,
        });
        position += 1;
    };
    // Mobile Alternates ergänzen die Seitenmetadaten, ohne die Positionen bestehender Rohlinks
    // zu verschieben. Die Limit-Auswahl erfolgte bereits in Dokumentreihenfolge in `parse_now`.
    let ordered_links = map
        .links
        .iter()
        .filter(|&&i| refs[i].ext.link_type != LinkType::MobileAlternate)
        .chain(
            map.links
                .iter()
                .filter(|&&i| refs[i].ext.link_type == LinkType::MobileAlternate),
        );
    for &i in ordered_links {
        let r = &refs[i];
        if r.row && r.decision.store && !hidden_rows[i] {
            push_row(&mut links, r, ids[i], page_follow && !r.ext.nofollow_rel);
        }
    }
    for &i in map
        .canonicals
        .iter()
        .chain(&map.hreflang)
        .chain(&map.pagination)
    {
        let r = &refs[i];
        if r.row && r.decision.store && !hidden_rows[i] {
            push_row(&mut links, r, ids[i], true);
        }
    }
    if let (Some((r, _)), Some(id)) = (&redirect_plan, redirect_id) {
        let lt = match r.kind {
            RedirectType::Http => LinkType::HttpRedirect,
            RedirectType::MetaRefresh => LinkType::MetaRefresh,
            RedirectType::HttpRefresh => LinkType::HttpRefresh,
            RedirectType::HstsPolicy => LinkType::HstsPolicy,
            RedirectType::JavaScript => LinkType::JavaScriptRedirect,
        };
        links.push(LinkRow {
            target_id: Some(id),
            target_raw: r.raw.clone(),
            link_type: lt,
            position,
            anchor: None,
            alt_text: None,
            follow: true,
            rel: None,
            target_attr: None,
            path_type: Some(path_type_of(&r.raw)),
            // das Meta-Element im Kopf: Position Head und Pfad `//head/meta[n]`; Location und Header ohne beides
            link_path: r.xpath.clone().filter(|_| store_link_paths),
            link_position: r.xpath.as_ref().filter(|_| store_link_paths).map(|x| {
                if x.starts_with("//head/") {
                    LinkPosition::Head
                } else {
                    LinkPosition::Content
                }
            }),
            link_origin: if r.kind == RedirectType::JavaScript {
                LinkOrigin::RenderedHtml
            } else {
                origins
                    .get(&crate::render_glue::link_key(&synth_link(
                        lt,
                        &r.raw,
                        Some(&r.target.encoded),
                    )))
                    .copied()
                    .unwrap_or(LinkOrigin::Html)
            },
            in_head: r.xpath.as_ref().map(|x| x.starts_with("//head/")),
            ..Default::default()
        });
    }

    let images: Vec<ImageRow> = parsed
        .as_ref()
        .map(|p| {
            p.images
                .iter()
                .zip(&image_ids)
                .map(|(im, id)| ImageRow {
                    image_id: *id,
                    instance: im.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    // „Speichern“ aus (CRAWL-038, CRAWL-046, K-40): keine Detailzeilen und keine Spaltenwerte. Das Crawlen der
    // Ziele ist davon unabhängig und hat oben schon stattgefunden.
    let keep_canonicals = cfg.spider.canonicals.store;
    let keep_hreflang = cfg.spider.hreflang.store;
    let keep_pagination = cfg.spider.pagination.store;
    // M2-D04 (K2-61): transportiert Canonical-Attribute und den Prüfmarker des Dokumentbaums.
    if keep_canonicals {
        batch.ext.extend(
            os_parse::canonical::page_facts(
                &canonicals,
                cfg.advanced.html_validation
                    && (parsed.is_some() || facts.content_class != ContentClass::Html),
            )
            .into_iter()
            .map(|(key, value)| os_model::ExtWrite {
                url: item.id,
                key,
                value,
            }),
        );
    }
    let canon_rows: Vec<CanonicalRow> = if keep_canonicals {
        canonicals
            .iter()
            .enumerate()
            .map(|(n, c)| CanonicalRow {
                target_id: ids.get(map.canonicals[n]).copied().flatten(),
                r: c.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let hreflang_rows: Vec<HreflangRow> = if keep_hreflang {
        hreflang
            .iter()
            .enumerate()
            .map(|(n, h)| HreflangRow {
                target_id: ids.get(map.hreflang[n]).copied().flatten(),
                sitemap_id: None,
                r: h.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let pagination_rows: Vec<PaginationRow> = if keep_pagination {
        pagination
            .iter()
            .enumerate()
            .map(|(n, p)| PaginationRow {
                target_id: ids.get(map.pagination[n]).copied().flatten(),
                r: p.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let refresh_rows: Vec<RefreshRow> = refresh
        .iter()
        .map(|r| {
            let chosen = redirect_plan
                .as_ref()
                .is_some_and(|(t, _)| r.target_raw.as_deref() == Some(t.raw.as_str()))
                && matches!(
                    (r.source, redirect_plan.as_ref().map(|(t, _)| t.kind)),
                    (RefreshSource::Meta, Some(RedirectType::MetaRefresh))
                        | (RefreshSource::Header, Some(RedirectType::HttpRefresh))
                );
            RefreshRow {
                target_id: if chosen { redirect_id } else { None },
                r: r.clone(),
            }
        })
        .collect();

    // Seitenfakten
    let canon_pick = pick_canonical(&canonicals).filter(|_| keep_canonicals);
    let canonical_url = canon_pick.and_then(|i| canonicals[i].resolved.clone());
    let canonical_target = canon_pick.and_then(|i| canon_rows[i].target_id);
    let canonical_count = if keep_canonicals {
        canonicals.len() as u32
    } else {
        0
    };
    let has_rel_next = keep_pagination && pagination.iter().any(|x| x.rel_next);
    let has_rel_prev = keep_pagination && pagination.iter().any(|x| !x.rel_next);
    let hreflang_count = if keep_hreflang {
        hreflang.len() as u32
    } else {
        0
    };
    let canonical_self = canonical_url
        .as_deref()
        .map(|c| os_url::eq_for_compare(own, c));
    let (eff_index, eff_follow) = match &parsed {
        Some(p) => (p.effective_index, p.effective_follow),
        None => {
            let hdr: Vec<_> = directives
                .iter()
                .filter(|d| d.source == os_model::DirectiveSource::XRobotsTag)
                .cloned()
                .collect();
            os_parse::effective::compute(&[], &hdr)
        }
    };
    let has_hints = !(canonicals.is_empty()
        && directives.is_empty()
        && refresh.is_empty()
        && hreflang.is_empty()
        && pagination.is_empty());
    let page_facts = match (&parsed, &pdf) {
        (Some(p), _) => Some(PageFacts {
            title: element_pair(&p.titles, cfg.extraction.page_titles),
            meta_description: element_pair(&p.meta_descriptions, cfg.extraction.meta_description),
            meta_keywords: element_pair(&p.meta_keywords, cfg.extraction.meta_keywords),
            h1: element_pair(&p.h1, cfg.extraction.h1),
            h2: element_pair(&p.h2, cfg.extraction.h2),
            word_count: p.word_count,
            text_ratio: p.text_ratio,
            canonical_count,
            canonical_url,
            canonical_target,
            canonical_self,
            effective_index: eff_index,
            effective_follow: eff_follow,
            has_rel_next,
            has_rel_prev,
            hreflang_count,
        }),
        (None, pdf_meta) if pdf_meta.is_some() || (hints.is_some() && has_hints) => {
            let (title, keywords) = match pdf_meta {
                Some(m) => (
                    pdf_pair(m.title.as_deref()),
                    pdf_pair(m.keywords.as_deref()),
                ),
                None => (ElementPair::default(), ElementPair::default()),
            };
            Some(PageFacts {
                title,
                meta_description: ElementPair::default(),
                meta_keywords: keywords,
                h1: ElementPair::default(),
                h2: ElementPair::default(),
                word_count: pdf_word_count,
                text_ratio: None,
                canonical_count,
                canonical_url,
                canonical_target,
                canonical_self,
                effective_index: eff_index,
                effective_follow: eff_follow,
                has_rel_next,
                has_rel_prev,
                hreflang_count,
            })
        }
        _ => None,
    };
    let unavailable_after = directives
        .iter()
        .filter(|d| d.applies)
        .filter_map(|d| os_parse::directives::unavailable_after(&d.raw))
        .min();
    let (indexability, indexability_status) = os_model::indexability::evaluate_with_expiration(
        &IndexabilityInput {
            fetch: &facts,
            page: page_facts.as_ref(),
            page_url: own,
            refresh: &refresh,
            cfg,
            same_url: os_url::eq_for_compare,
        },
        unavailable_after,
    );
    let hidden = if cfg.advanced.respect_noindex && !eff_index {
        Some("noindex")
    } else if cfg.advanced.respect_canonical
        && page_facts
            .as_ref()
            .is_some_and(|p| p.canonical_self == Some(false))
    {
        Some("canonical")
    } else {
        None
    };
    if let Some(reason) = hidden {
        sh.lock_core().respect_hidden.insert(item.id);
        batch.url_updates.push(os_model::UrlUpdate::InTables {
            id: item.id,
            value: false,
        });
        url_flags.push((
            item.id,
            os_model::ext::FLAG_RESPECT_HIDDEN.to_string(),
            Some(reason.to_string()),
        ));
    }
    batch.pages.push(PageWrite {
        url_id: item.id,
        replace: item.replace,
        fetch: facts,
        page: page_facts,
        indexability,
        indexability_status,
        links,
        images,
        canonicals: canon_rows,
        directives: directives.into_iter().map(DirectiveRow).collect(),
        refresh: refresh_rows,
        hreflang: hreflang_rows,
        pagination: pagination_rows,
        headers,
        html,
    });
    batch.queue.push(QueueOp::Pop { id: item.id });
    batch.url_flags = url_flags;
    Ok(batch)
}

// ---------------------------------------------------------------------------------------------------
/// Transportiert alle Parser-Fakten in den Schreibbatch.
fn transport_page(url: UrlId, page: &ParsedPage, batch: &mut WriteBatch) {
    let mut facts = page.ext.clone();
    if let Some(validation) = &page.validation {
        facts.extend(os_parse::validation::to_ext(validation));
        #[cfg(test)]
        tests::VALIDATION_STUB.with(|stub| {
            if let Some(stub) = stub.get() {
                facts.extend(stub(validation));
            }
        });
    }
    // M2-D19: Rechtschreib-/Grammatikbefunde aus `spelling.findings` nach `spelling_findings`.
    batch
        .spelling
        .extend(os_parse::spelling::take_findings(url, &mut facts));
    if page.hreflang_truncated > 0 {
        facts.push((
            os_model::ExtKey::from_static("hreflang.truncated"),
            os_model::Value::Int(i64::from(page.hreflang_truncated)),
        ));
    }
    batch.ext.extend(
        facts
            .into_iter()
            .map(|(key, value)| os_model::ExtWrite { url, key, value }),
    );
    let (items, issues) = os_parse::sd::to_writes(url, &page.structured_data);
    batch.structured_data.extend(items);
    batch.sd_issues.extend(issues);
    // M2-D20 (K2-61): Befunde der schema.org- und Rich-Result-Validierung.
    batch
        .sd_issues
        .extend(os_parse::sd::validate::to_writes(url, &page.sd_findings));
    batch
        .forms
        .extend(page.forms.iter().cloned().map(|form| os_model::FormWrite {
            url_id: url,
            idx: form.idx,
            action: form.action,
            method: form.method,
        }));
    batch
        .custom_search
        .extend(
            page.custom_search
                .iter()
                .cloned()
                .map(|hit| os_model::CustomSearchWrite {
                    url_id: url,
                    def_index: hit.def_index,
                    count: hit.count,
                    error: hit.error,
                }),
        );
    batch
        .custom_extraction
        .extend(page.custom_extraction.iter().cloned().map(|value| {
            os_model::CustomExtractionWrite {
                url_id: url,
                def_index: value.def_index,
                number: value.number,
                value: value.value,
                error: value.error,
            }
        }));
    if let Some(signature) = &page.minhash {
        batch.minhash.push((url, signature.clone()));
    }
}

#[cfg(test)]
mod tests {
    type ValidationStub =
        fn(&os_model::ValidationFacts) -> Vec<(os_model::ExtKey, os_model::Value)>;
    thread_local! {
        pub(super) static PARSER_STUB: std::cell::Cell<Option<fn(&mut ParsedPage)>> = const { std::cell::Cell::new(None) };
        pub(super) static VALIDATION_STUB: std::cell::Cell<Option<ValidationStub>> = const { std::cell::Cell::new(None) };
    }

    use super::*;
    use crate::sitemaps::FetchedSitemap;
    use os_model::{CanonicalRef, SitemapDoc, SitemapLoc};
    use os_model::{HreflangRef, SitemapKind, UrlState};
    use os_url::Scope;

    fn response(status: u16, headers: &[(&str, &str)], body: &str) -> FetchResult {
        FetchResult {
            url: Url::parse("https://site.test/p").unwrap(),
            started_at: Timestamp::UNIX_EPOCH,
            outcome: FetchOutcome::Response(HttpResponse {
                status,
                reason_phrase: None,
                status_text: os_fetch::http::status_text(status, None),
                http_version: "HTTP/1.1".to_string(),
                headers: headers
                    .iter()
                    .map(|(n, v)| (n.to_string(), v.to_string()))
                    .collect(),
                body: Bytes::from(body.to_string()),
                size_bytes: body.len() as u64,
                transferred_bytes: 123,
                response_time_ms: 42,
                truncated: false,
                decode_error: None,
                md5_hex: "d41d8cd98f00b204e9800998ecf8427e".to_string(),
                request_headers: Vec::new(),
                set_cookies: Vec::new(),
                content_length: None,
                untruncated_bytes: None,
                synthetic: None,
                attempts: 1,
            }),
        }
    }

    // spec: INTEG-020, INTEG-040, ONPAGE-034
    // Infrastruktur (M2-F05a); Fachprüfung M2-D09/D11/D14/D15.
    fn payload_fixture() -> ParsedPage {
        let fixture = include_str!("../../../tests/fixtures/config/default.osconfig");
        ParsedPage {
            ext: vec![(
                os_model::ExtKey::from_static("content.fixture"),
                os_model::Value::Text(fixture.into()),
            )],
            forms: vec![os_model::FormRef {
                idx: 3,
                action: Some("/submit".into()),
                action_resolved: None,
                method: "post".into(),
            }],
            custom_search: vec![os_model::SearchHit {
                def_index: 2,
                count: 7,
                error: None,
            }],
            custom_extraction: vec![os_model::ExtractedValue {
                def_index: 1,
                number: 4,
                value: "fixture".into(),
                error: None,
            }],
            minhash: Some(vec![7, 8, 9]),
            structured_data: vec![os_model::SdItem {
                format: "json-ld".into(),
                type_path: "Thing".into(),
                json: "{}".into(),
                parse_error: Some("fixture-parse-error".into()),
            }],
            hreflang_truncated: 12,
            validation: Some(os_model::ValidationFacts {
                head_errors: 7,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    // spec: INTEG-020, INTEG-040, ONPAGE-034
    #[test]
    fn parser_extension_payloads_keep_their_url_and_values() {
        let page = payload_fixture();
        let mut batch = WriteBatch::default();
        transport_page(UrlId(42), &page, &mut batch);
        assert_eq!(batch.ext[0].url, UrlId(42));
        assert_eq!(batch.ext[0].value, page.ext[0].1);
        assert_eq!(batch.forms[0].idx, 3);
        assert_eq!(batch.forms[0].url_id, UrlId(42));
        assert_eq!(batch.custom_search[0].count, 7);
        assert_eq!(batch.custom_extraction[0].number, 4);
        assert_eq!(batch.minhash, vec![(UrlId(42), vec![7, 8, 9])]);
        assert_eq!(batch.structured_data[0].url_id, UrlId(42));
        assert_eq!(batch.structured_data[0].json, "{}");
        assert_eq!(batch.sd_issues[0].url_id, UrlId(42));
        assert_eq!(batch.sd_issues[0].code, "fixture-parse-error");
        // D10 liefert jetzt echte Validierungsfakten vor dem hreflang-Zähler.
        assert!(batch.ext.iter().any(|fact| {
            fact.key.as_str() == "validation.checked" && fact.value == os_model::Value::Bool(true)
        }));
        assert_eq!(
            batch
                .ext
                .iter()
                .find(|fact| fact.key.as_str() == "hreflang.truncated")
                .unwrap()
                .value,
            os_model::Value::Int(12)
        );
    }

    // spec: INTEG-020, INTEG-040, TECH-008
    // Infrastruktur (M2-F05a); Fachprüfung M2-D09/D10/D14/D15.
    #[tokio::test(start_paused = true)]
    async fn parser_stub_payloads_reach_sqlite_through_a_simfetcher_crawl() {
        use crate::testing::{SimFetcher, SimSite};
        use crate::{start_crawl, CrawlRequest, CrawlSource};
        use os_store::{CrawlFile, OpenMode};
        PARSER_STUB.set(Some(|page| *page = payload_fixture()));
        VALIDATION_STUB.set(Some(|facts| {
            vec![(
                os_model::ExtKey::from_static("validation.fixture"),
                os_model::Value::Int(i64::from(facts.head_errors)),
            )]
        }));
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                PARSER_STUB.set(None);
                VALIDATION_STUB.set(None);
            }
        }
        let _reset = Reset;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload.ospider");
        let cfg = CrawlConfig::default();
        let site = SimSite::new().page("https://site.test/", "<html><body>fixture</body></html>");
        start_crawl(
            SimFetcher::new(site),
            CrawlRequest {
                config: cfg,
                source: CrawlSource::Spider {
                    start_url: "https://site.test/".into(),
                },
                file: path.clone(),
                project: "fixture".into(),
                name: "payload".into(),
            },
            crate::testing::test_deps(),
        )
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        let conn = file.conn();
        for (sql, expected) in [
            (
                "SELECT v_int FROM url_ext WHERE key='validation.fixture'",
                7,
            ),
            ("SELECT idx FROM forms", 3),
            ("SELECT count FROM custom_search_hits", 7),
            ("SELECT number FROM custom_extraction_values", 4),
            ("SELECT COUNT(*) FROM sd_items WHERE json='{}'", 1),
            (
                "SELECT COUNT(*) FROM sd_issues WHERE code='fixture-parse-error'",
                1,
            ),
            ("SELECT COUNT(*) FROM minhash", 1),
        ] {
            assert_eq!(
                conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap(),
                expected,
                "{sql}"
            );
        }
        let ext: String = conn
            .query_row(
                "SELECT v_text FROM url_ext WHERE key='content.fixture'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            ext,
            include_str!("../../../tests/fixtures/config/default.osconfig")
        );
    }

    // spec: CRAWL-022
    #[test]
    fn http_date_is_parsed_to_utc() {
        let t = parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT").expect("gültig");
        assert_eq!(t.to_db(), "2015-10-21T07:28:00.000Z");
        assert!(parse_http_date("kein Datum").is_none());
    }

    // spec: ONPAGE-018
    #[test]
    fn hash_is_only_set_for_html_2xx_responses() {
        let cfg = CrawlConfig::default();
        let html = response(200, &[("content-type", "text/html")], "<html></html>");
        assert_eq!(
            facts_from_result(&html, &cfg)
                .hash_md5
                .as_deref()
                .map(str::len),
            Some(32)
        );
        // Fehlerseiten mit HTML-Körper, Bilder, CSS, Text und Weiterleitungen haben keinen Hash (K-60)
        let error = response(404, &[("content-type", "text/html")], "<html></html>");
        assert_eq!(facts_from_result(&error, &cfg).hash_md5, None);
        for ct in ["image/png", "text/css", "text/plain", "application/json"] {
            let res = response(200, &[("content-type", ct)], "abc");
            assert_eq!(facts_from_result(&res, &cfg).hash_md5, None, "{ct}");
        }
        let redirect = response(301, &[("location", "/x")], "");
        assert_eq!(facts_from_result(&redirect, &cfg).hash_md5, None);
        // 2xx ohne Content-Type (204): Hash des leeren Körpers wie in der Referenz
        let no_content = response(204, &[], "");
        assert_eq!(
            facts_from_result(&no_content, &cfg).hash_md5.as_deref(),
            Some("d41d8cd98f00b204e9800998ecf8427e")
        );
    }

    // spec: CRAWL-022
    #[test]
    fn facts_follow_the_response_and_k02() {
        let cfg = CrawlConfig::default();
        let res = response(
            200,
            &[
                ("content-type", "text/html; charset=utf-8"),
                ("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT"),
            ],
            "<html></html>",
        );
        let f = facts_from_result(&res, &cfg);
        assert_eq!(f.status_code, 200);
        assert_eq!(f.status, "OK");
        assert_eq!(f.content_class, ContentClass::Html);
        assert_eq!(f.transferred_bytes, 123);
        assert_eq!(f.total_transferred_bytes, 123, "K-02");
        assert_eq!(f.response_time_ms, Some(42));
        assert_eq!(
            f.last_modified.map(|t| t.to_db()).as_deref(),
            Some("2015-10-21T07:28:00.000Z")
        );
        assert_eq!(f.hash_md5.as_deref().map(str::len), Some(32));

        let mut off = CrawlConfig::default();
        off.extraction.response_time = false;
        off.extraction.hash = false;
        off.extraction.last_modified = false;
        let f = facts_from_result(&res, &off);
        assert_eq!(
            (f.response_time_ms, f.hash_md5, f.last_modified_raw),
            (None, None, None)
        );
    }

    // spec: CRAWL-011
    #[test]
    fn broken_compression_keeps_the_status_code_but_reads_connection_error() {
        let cfg = CrawlConfig::default();
        let mut res = response(200, &[("content-type", "text/html")], "<html>");
        if let FetchOutcome::Response(r) = &mut res.outcome {
            r.decode_error = Some("unerwartetes Ende".into());
        }
        let f = facts_from_result(&res, &cfg);
        assert_eq!(f.status_code, 200);
        assert_eq!(f.status, "Connection Error");
        assert_eq!(
            f.no_response_reason,
            Some(NoResponseReason::ConnectionError)
        );
    }

    // spec: CRAWL-013
    #[test]
    fn no_response_gives_status_zero_and_the_reason_text() {
        let cfg = CrawlConfig::default();
        let res = FetchResult {
            url: Url::parse("https://site.test/").unwrap(),
            started_at: Timestamp::UNIX_EPOCH,
            outcome: FetchOutcome::NoResponse {
                reason: NoResponseReason::DnsLookupFailed,
                detail: String::new(),
                elapsed_ms: 5,
            },
        };
        let f = facts_from_result(&res, &cfg);
        assert_eq!(f.status_code, 0);
        assert_eq!(f.status, "DNS Lookup Failed");
        assert_eq!(f.response_time_ms, None);
    }

    // spec: OUT-031
    #[test]
    fn path_type_distinguishes_the_four_forms() {
        assert_eq!(path_type_of("https://a.test/x"), PathType::Absolute);
        assert_eq!(path_type_of("//a.test/x"), PathType::ProtocolRelative);
        assert_eq!(path_type_of("/x"), PathType::RootRelative);
        assert_eq!(path_type_of("x/y"), PathType::PathRelative);
    }

    fn canon(source: HintSource, in_head: bool, resolved: Option<&str>) -> CanonicalRef {
        CanonicalRef {
            source,
            raw: resolved.unwrap_or("").to_string(),
            resolved: resolved.map(str::to_string),
            in_head,
            order: 0,
            xpath: None,
            ..Default::default()
        }
    }

    // spec: TECH-017
    #[test]
    fn authoritative_canonical_prefers_header_then_head_and_ignores_body() {
        let c = vec![
            canon(HintSource::Html, false, Some("https://s.test/body")),
            canon(HintSource::Html, true, Some("https://s.test/head")),
            canon(HintSource::Http, true, Some("https://s.test/header")),
        ];
        assert_eq!(pick_canonical(&c), Some(2));
        assert_eq!(pick_canonical(&c[..2]), Some(1));
        assert_eq!(pick_canonical(&c[..1]), None);
        let invalid = vec![
            canon(HintSource::Http, true, None),
            canon(HintSource::Html, true, Some("https://s.test/ok")),
        ];
        assert_eq!(pick_canonical(&invalid), Some(1));
        assert_eq!(pick_canonical(&[]), None);
    }

    // spec: CRAWL-005, TECH-210
    #[test]
    fn sitemap_batch_queues_loc_urls_without_depth_and_links_the_hierarchy() {
        let cfg = CrawlConfig::default();
        let start = os_url::parse_start_url("https://site.test/").unwrap();
        let scope = Scope::new(&start, &cfg.scope);
        let rules = os_url::RuleSet::compile(&[], &[], false).unwrap();
        let mut core = Core::new(0, None);
        let doc = |kind, locs: Vec<&str>| SitemapDoc {
            kind,
            locs: locs
                .into_iter()
                .map(|l| SitemapLoc {
                    loc_raw: l.to_string(),
                    url: Some(l.to_string()),
                    lastmod: Some("2026-01-01".to_string()),
                    hreflang: vec![],
                    ..Default::default()
                })
                .collect(),
            invalid_entries: 1,
            size_bytes: 77,
        };
        let fr = |u: &str| FetchResult {
            url: Url::parse(u).unwrap(),
            started_at: Timestamp::UNIX_EPOCH,
            outcome: response(200, &[("content-type", "application/xml")], "<x/>").outcome,
        };
        let fetched = vec![
            FetchedSitemap {
                url: "https://site.test/sitemap.xml".into(),
                parent: None,
                depth: 0,
                found_via: "config".into(),
                fetch: fr("https://site.test/sitemap.xml"),
                doc: Some(doc(SitemapKind::Index, vec!["https://site.test/s1.xml"])),
            },
            FetchedSitemap {
                url: "https://site.test/s1.xml".into(),
                parent: Some(0),
                depth: 1,
                found_via: "config".into(),
                fetch: fr("https://site.test/s1.xml"),
                doc: Some(doc(
                    SitemapKind::Urlset,
                    vec!["https://site.test/nur-sitemap", "https://other.test/extern"],
                )),
            },
        ];
        let batch = build_sitemap_batch(
            &mut core,
            &cfg,
            &scope,
            &rules,
            &fetched,
            Timestamp::UNIX_EPOCH,
        );
        assert_eq!(batch.sitemaps.len(), 2);
        assert_eq!(batch.sitemaps[0].parent, None);
        assert_eq!(batch.sitemaps[1].parent, Some(batch.sitemaps[0].id));
        assert_eq!(batch.sitemaps[0].kind, SitemapKind::Index);
        assert_eq!(batch.sitemaps[0].entries, 1);
        // K-41d: der Eintrag auf other.test zählt zu den ungültigen (1 aus der Datei + 1 fremder)
        assert_eq!(batch.sitemaps[1].invalid_entries, 2);
        assert_eq!(batch.sitemaps[1].entries, 2);
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].kind, "warn");
        assert_eq!(batch.events[0].code, "sitemap_foreign_entries");
        assert!(batch.events[0].message.starts_with("1 "));
        assert_eq!(batch.sitemaps[1].status_code, Some(200));
        assert_eq!(batch.sitemaps[1].found_via, "config");
        // nur der Eintrag desselben Ursprungs wird übernommen
        assert_eq!(batch.sitemaps[1].entry_urls.len(), 1);
        let (id, loc, lastmod) = &batch.sitemaps[1].entry_urls[0];
        assert_eq!(loc, "https://site.test/nur-sitemap");
        assert_eq!(lastmod.as_deref(), Some("2026-01-01"));
        let row = batch.new_urls.iter().find(|n| n.id == *id).unwrap();
        assert_eq!(row.crawl_depth, None, "ohne Tiefe");
        assert_eq!(row.discovered_via, DiscoveryKind::Sitemap);
        assert_eq!(row.state, UrlState::Queued);
        assert!(batch.queue.iter().any(
            |q| matches!(q, QueueOp::Push { id: i, priority: 1, depth: None, .. } if i == id)
        ));
        // die Sitemap-Dateien haben eine URL-Zeile mit Status
        assert!(batch.sitemaps.iter().all(|s| s.url_id.is_some()));
        assert_eq!(batch.pages.len(), 2);
        assert!(batch
            .new_urls
            .iter()
            .filter(|n| n.url.ends_with(".xml"))
            .all(|n| !n.in_tables));
    }

    fn alt(code: &str, href: &str) -> HreflangRef {
        HreflangRef {
            source: HintSource::Sitemap,
            code_raw: code.to_string(),
            code_norm: code.to_string(),
            code_valid: true,
            invalid_reason: None,
            href_raw: href.to_string(),
            resolved: Some(href.to_string()),
            in_head: false,
            order: 0,
            xpath: None,
        }
    }

    fn urlset_fetch(url: &str, locs: Vec<SitemapLoc>) -> FetchedSitemap {
        FetchedSitemap {
            url: url.to_string(),
            parent: None,
            depth: 0,
            found_via: "config".into(),
            fetch: FetchResult {
                url: Url::parse(url).unwrap(),
                started_at: Timestamp::UNIX_EPOCH,
                outcome: response(200, &[("content-type", "application/xml")], "<x/>").outcome,
            },
            doc: Some(SitemapDoc {
                kind: SitemapKind::Urlset,
                locs,
                invalid_entries: 0,
                size_bytes: 10,
            }),
        }
    }

    // spec: TECH-210
    #[test]
    fn sitemap_hreflang_is_numbered_per_url_across_sitemaps_and_targets_follow_decide() {
        let cfg = CrawlConfig::default();
        let start = os_url::parse_start_url("https://site.test/").unwrap();
        let scope = Scope::new(&start, &cfg.scope);
        let rules = os_url::RuleSet::compile(&[], &[], false).unwrap();
        let mut core = Core::new(0, None);
        let loc = |alts: Vec<HreflangRef>| SitemapLoc {
            loc_raw: "https://site.test/a".to_string(),
            url: Some("https://site.test/a".to_string()),
            lastmod: None,
            hreflang: alts,
            ..Default::default()
        };
        let fetched = vec![
            urlset_fetch(
                "https://site.test/s1.xml",
                vec![loc(vec![
                    alt("de", "https://site.test/a"),
                    alt("fr", "https://site.test/fr/a"),
                ])],
            ),
            urlset_fetch(
                "https://site.test/s2.xml",
                vec![loc(vec![alt("en", "https://other.test/en/a")])],
            ),
        ];
        let batch = build_sitemap_batch(
            &mut core,
            &cfg,
            &scope,
            &rules,
            &fetched,
            Timestamp::UNIX_EPOCH,
        );
        let orders = |i: usize| -> Vec<(u32, String)> {
            batch.sitemaps[i]
                .hreflang
                .iter()
                .map(|(_, h)| (h.r.order, h.r.code_norm.clone()))
                .collect()
        };
        assert_eq!(orders(0), [(0, "de".to_string()), (1, "fr".to_string())]);
        assert_eq!(
            orders(1),
            [(2, "en".to_string())],
            "die Nummer läuft über alle Sitemaps weiter, nichts wird überschrieben"
        );
        let a_id = batch.sitemaps[0].entry_urls[0].0;
        assert!(batch
            .sitemaps
            .iter()
            .flat_map(|s| &s.hreflang)
            .all(|(id, h)| *id == a_id
                && h.r.source == HintSource::Sitemap
                && h.target_id.is_some()));
        assert_eq!(
            batch.sitemaps[0].hreflang[0].1.sitemap_id,
            Some(batch.sitemaps[0].id)
        );
        // Standard: hreflang speichern an, crawlen aus – interne und externe Ziele haben eine Zeile ohne
        // Tabellen-Sichtbarkeit (K-62) und sind nicht eingeplant.
        let target = |url: &str| batch.new_urls.iter().find(|n| n.url == url).unwrap();
        let fr = target("https://site.test/fr/a");
        assert!(!fr.in_tables);
        assert_eq!(fr.not_crawled_reason, Some(NotCrawledReason::StoreOnly));
        let en = target("https://other.test/en/a");
        assert!(!en.is_internal && !en.in_tables);
        assert!(!batch
            .queue
            .iter()
            .any(|q| matches!(q, QueueOp::Push { id, .. } if *id == fr.id || *id == en.id)));

        // Speichern aus: keine Zeilen, keine Zielzeilen.
        let mut off = CrawlConfig::default();
        off.spider.hreflang = os_model::StoreCrawl::OFF;
        let mut core = Core::new(0, None);
        let batch = build_sitemap_batch(
            &mut core,
            &off,
            &scope,
            &rules,
            &fetched,
            Timestamp::UNIX_EPOCH,
        );
        assert!(batch.sitemaps.iter().all(|s| s.hreflang.is_empty()));
        assert!(batch.new_urls.iter().all(|n| !n.url.contains("/fr/")));
    }

    // spec: TECH-210
    #[test]
    fn sitemap_hreflang_with_store_off_and_crawl_on_queues_targets_but_writes_no_rows() {
        let cfg = {
            let mut c = CrawlConfig::default();
            c.spider.hreflang = os_model::StoreCrawl {
                store: false,
                crawl: true,
            };
            c
        };
        let start = os_url::parse_start_url("https://site.test/").unwrap();
        let scope = Scope::new(&start, &cfg.scope);
        let rules = os_url::RuleSet::compile(&[], &[], false).unwrap();
        let mut core = Core::new(0, None);
        let fetched = vec![urlset_fetch(
            "https://site.test/s1.xml",
            vec![SitemapLoc {
                loc_raw: "https://site.test/a".to_string(),
                url: Some("https://site.test/a".to_string()),
                lastmod: None,
                hreflang: vec![alt("fr", "https://site.test/fr/a")],
                ..Default::default()
            }],
        )];
        let batch = build_sitemap_batch(
            &mut core,
            &cfg,
            &scope,
            &rules,
            &fetched,
            Timestamp::UNIX_EPOCH,
        );
        assert!(batch.sitemaps[0].hreflang.is_empty(), "keine Zeilen");
        let fr = batch
            .new_urls
            .iter()
            .find(|n| n.url == "https://site.test/fr/a")
            .expect("Ziel über decide eingeplant");
        assert!(!fr.in_tables, "nicht gespeichert");
        assert!(batch
            .queue
            .iter()
            .any(|q| matches!(q, QueueOp::Push { id, .. } if *id == fr.id)));
    }
}

#[cfg(test)]
mod future_size_tests {
    use super::*;
    use std::{future::Future, mem::size_of};

    fn future_size<A, F: Future>(_: impl FnOnce(A) -> F) -> usize {
        size_of::<F>()
    }

    // spec: AGENT-020 [M2]
    // spec: CRAWL-001
    #[test]
    fn url_worker_futures_have_bounded_size() {
        let process_bytes = future_size(|(sh, item)| process::<os_fetch::HttpFetcher>(sh, item));
        let item_bytes = future_size(
            |(sh, item): (
                &'static Arc<Shared<os_fetch::HttpFetcher>>,
                &'static QueueItem,
            )| run_item(sh, item),
        );
        eprintln!("URL process future: {process_bytes} bytes; run_item: {item_bytes} bytes");
        // `process` is spawned per URL; `run_item` keeps its phases (robots, fetch, analysis,
        // batch) behind heap pointers instead of inline (13.9 KiB before, x86_64 debug).
        assert!(
            process_bytes <= 1024,
            "URL process future: {process_bytes} bytes exceeds 1 KiB"
        );
        assert!(
            item_bytes <= 8 * 1024,
            "URL item future: {item_bytes} bytes exceeds 8 KiB"
        );
    }
}

//! Aufruf der KI-Warteschlange aus der Engine (08 AI-020 [M4], Nachtrag K4-13; AI-039, E-6 a; Klärung K4-25
//! Variante A; M4-D05). Die Engine kennt nur `os_model::ai::hook`; die Umsetzung kommt über `EngineDeps::ai_queue`.
//!
//! * Start eines Laufs ([`begin`]): eine Sitzung je Crawl-Lauf (beim Fortsetzen mit dem Leser der bereits gecrawlten
//!   Seiten, [`resuming`]).
//! * Je interner Seiten-URL der Grundmenge G die Eingabefelder nach AI-032 ([`page_fields`], nach dem Schreiben der
//!   Zeile übergeben).
//! * Ende ([`EndGuard::finish`]): Pause → kein neuer Request, laufende werden übernommen; Stopp → kein weiterer
//!   Request; Ende ohne Stoppen → warten, bis die Warteschlange leer ist (erst danach `on_finished`); Fehler bzw. ein
//!   Ende ohne Meldung (Verwerfen des Laufs) → Abbruch.
use std::path::Path;
use std::sync::Arc;

use os_fetch::{FetchOutcome, FetchResult};
use os_model::ai::hook::{AiQueue, AiQueueEnd, AiQueueSession, AiQueueStart, PageAiFields};
use os_model::{CrawlConfig, CrawlState, EndReason, Indexability, WriteBatch};
use url::Url;

use crate::frontier::{Purpose, QueueItem};

/// KI-Warteschlange beim Fortsetzen eines Crawls (`lifecycle::resume_crawl`): dieselbe Laufzeile, offene Einträge der
/// bereits gecrawlten URLs (Leser [`crate::ai_input::load_pages`]).
pub fn resuming(queue: Arc<dyn AiQueue>) -> Arc<dyn AiQueue> {
    Arc::new(Resuming(queue))
}

struct Resuming(Arc<dyn AiQueue>);

impl AiQueue for Resuming {
    fn begin(&self, mut start: AiQueueStart) -> Option<Arc<dyn AiQueueSession>> {
        start.resumed = true;
        start.existing = Some(crate::ai_input::page_loader());
        self.0.begin(start)
    }
}

/// Beginnt die Sitzung eines Crawl-Laufs.
pub(crate) fn begin(
    queue: Option<&Arc<dyn AiQueue>>,
    file: &Path,
    cfg: &CrawlConfig,
    owner_kind: &'static str,
) -> Option<Arc<dyn AiQueueSession>> {
    queue?.begin(AiQueueStart {
        crawl_file: file.to_path_buf(),
        config: cfg.clone(),
        resumed: false,
        owner_kind,
        existing: None,
    })
}

/// Ende der Sitzung; ohne [`EndGuard::finish`] (Fehlerpfad, Verwerfen) gilt es als Abbruch.
pub(crate) struct EndGuard(Option<Arc<dyn AiQueueSession>>);

impl EndGuard {
    pub(crate) fn new(session: Option<Arc<dyn AiQueueSession>>) -> Self {
        Self(session)
    }

    /// Meldet das Ende des Crawl-Laufs; bei einem Ende ohne Stoppen wartet es, bis die Warteschlange leer ist.
    pub(crate) async fn finish(mut self, state: CrawlState, reason: Option<EndReason>) {
        let Some(session) = self.0.take() else {
            return;
        };
        let end = match (state, reason) {
            (CrawlState::Paused, _) => AiQueueEnd::Paused,
            (_, Some(EndReason::Stopped)) => AiQueueEnd::Stopped,
            (CrawlState::Completed, _) => AiQueueEnd::Drain,
            _ => AiQueueEnd::Abort,
        };
        session.end(end).await;
    }
}

impl Drop for EndGuard {
    fn drop(&mut self) {
        if let Some(session) = self.0.take() {
            // Die Meldung wirkt sofort; die Zukunft wird nicht abgewartet.
            drop(session.end(AiQueueEnd::Abort));
        }
    }
}

/// Eingabefelder einer abgerufenen URL für die Warteschlange; `None` außerhalb der Grundmenge G (Ressourcen, externe
/// URLs, Dateien) bzw. ohne Antwort.
pub(crate) fn page_fields(
    sh_cfg: &CrawlConfig,
    pcfg: &os_parse::ParseConfig,
    item: &QueueItem,
    url: &Url,
    result: &FetchResult,
    parsed: Option<&os_model::ParsedPage>,
    class: os_model::ContentClass,
) -> Option<PageAiFields> {
    if item.purpose != Purpose::Page {
        return None;
    }
    let FetchOutcome::Response(resp) = &result.outcome else {
        return None;
    };
    let status = resp.status;
    if !crate::ai_input::in_base_set(class, Some(status)) {
        return None;
    }
    let id = item.id.0;
    match parsed {
        Some(page) if class == os_model::ContentClass::Html => {
            let content_type = resp
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.as_str());
            let (html, _) = os_parse::charset::decode(&resp.body, content_type, &resp.headers);
            Some(crate::ai_input::from_page(
                id,
                url,
                status,
                false,
                &html,
                page,
                pcfg,
                crate::ai_input::structured_data_enabled(sh_cfg),
            ))
        }
        _ => Some(PageAiFields {
            url_id: id,
            url: url.to_string(),
            content_class: class.as_str().to_ascii_lowercase(),
            status_code: Some(status),
            ..PageAiFields::default()
        }),
    }
}

/// Übernimmt Status und Indexierbarkeit aus dem geschriebenen Datensatz der URL.
pub(crate) fn with_batch(mut page: PageAiFields, batch: &WriteBatch) -> PageAiFields {
    if let Some(p) = batch.pages.iter().find(|p| p.url_id.0 == page.url_id) {
        page.indexable = p.indexability == Indexability::Indexable;
        if p.fetch.status_code != 0 {
            page.status_code = Some(p.fetch.status_code);
        }
    }
    page
}

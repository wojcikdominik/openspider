//! Öffentliche Typen der Engine: Abhängigkeiten, Auftrag und Ergebnis (M1-Plan 3.6).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use os_fetch::SecretStore;
use os_model::{Clock, CrawlConfig, CrawlId, CrawlState, EndReason, IdSource};
use os_store::{CrawlSink, StorageProfile, StoreError};

/// Erzeugt die Senke, in die der Writer-Thread schreibt (Standard: `SqliteSink::open`).
pub type SinkFactory =
    Arc<dyn Fn(&Path, StorageProfile) -> Result<Box<dyn CrawlSink>, StoreError> + Send + Sync>;

/// Wird nach regulärem Ende, Limit oder Stop aufgerufen – nach dem abschließenden Checkpoint und vor der
/// Auflösung von `CrawlHandle::wait()`; nie bei Pause oder Absturz.
pub type FinishedHook = Arc<dyn Fn(&CrawlSummary) + Send + Sync>;

/// Einsteckbare Abhängigkeiten der Engine (Uhr, IDs, Speicherprofil, Geheimnisse, Senke, Hook).
#[derive(Clone)]
pub struct EngineDeps {
    /// Zeitquelle.
    pub clock: Arc<dyn Clock>,
    /// Quelle für Crawl-IDs.
    pub ids: Arc<dyn IdSource>,
    /// Speicherprofil der Crawl-Datei.
    pub profile: StorageProfile,
    /// Geheimnisse (Basic-Auth-Passwörter).
    pub secrets: Arc<dyn SecretStore>,
    /// `None` = `SqliteSink::open`; P28 setzt einen `SlowSink` (PERF-003), P15/P19 Prüf-Senken.
    pub sink_factory: Option<SinkFactory>,
    /// Hook am Crawl-Ende. `os-cli` (P26) übergibt hier `os_analyze::analysis::run_after_crawl` (CRAWL-141).
    pub on_finished: Option<FinishedHook>,
    /// Wer den Crawl besitzt, für `crawl_meta.owner_kind`: `"cli"` (Kommandozeile), `"app"` (Oberfläche),
    /// `"mcp-stdio"` (MCP-Server). Keine weitere Steuerung.
    pub owner_kind: &'static str,
    /// Optional renderer override. In JavaScript mode, None launches Chromium lazily;
    /// text mode never calls a renderer.
    pub renderer: Option<Arc<dyn crate::render_glue::Renderer>>,
    /// KI-Warteschlange während des Crawls (08 AI-020 [M4], AI-039, E-6 a; Klärung K4-25 Variante A, M4-D05).
    /// `None` = Verhalten von 1.0 (Schalter „KI (Vorschau)“ aus, Tests): keine Seitenfelder, kein KI-Request,
    /// keine gespeicherten KI-Eingaben.
    pub ai_queue: Option<Arc<dyn os_model::ai::hook::AiQueue>>,
}

/// Zusammenfassung eines beendeten Crawls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlSummary {
    /// Kennung des Crawls.
    pub crawl_id: CrawlId,
    /// Crawl-Datei.
    pub file: PathBuf,
    /// Endzustand.
    pub state: CrawlState,
    /// Grund des Endes.
    pub end_reason: Option<EndReason>,
    /// Anzahl abgeschlossener Abrufe (ohne robots.txt).
    pub crawled: u64,
}

/// Quelle der zu crawlenden URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrawlSource {
    /// Offline title/description drafts; never schedules HTTP requests.
    Serp {
        rows: Vec<crate::serp_input::SerpRow>,
        label: String,
    },
    /// Spider-Modus ab einer Start-URL.
    Spider {
        /// Start-URL (wie eingegeben).
        start_url: String,
    },
    /// Listenmodus.
    List {
        /// Eingegebene Zeilen bzw. URLs.
        items: Vec<String>,
        /// Bezeichnung der Quelle (z. B. Dateiname).
        label: String,
    },
}

/// Auftrag für einen neuen Crawl.
#[derive(Debug, Clone)]
pub struct CrawlRequest {
    /// Wirksame Konfiguration.
    pub config: CrawlConfig,
    /// Quelle.
    pub source: CrawlSource,
    /// Zu erzeugende Crawl-Datei.
    pub file: PathBuf,
    /// Projektname.
    pub project: String,
    /// Crawl-Name.
    pub name: String,
}

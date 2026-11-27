//! Schnittstelle der KI-Warteschlange im Crawl (08 AI-020 [M4], Nachtrag K4-13; Klärung K4-25 Variante A, M4-D05).
//!
//! Die Engine hängt nicht an `os-ai`. Sie kennt nur diese Typen: Je gecrawlter interner Seite übergibt sie die
//! Eingabefelder nach AI-032 ([`PageAiFields`]) an eine Sitzung der KI-Warteschlange ([`AiQueueSession::page`]); am
//! Ende des Laufs meldet sie, wie der Crawl endete ([`AiQueueEnd`]), und wartet bei einem Ende ohne Stoppen, bis die
//! Warteschlange leer ist (erst danach `on_finished`). Die Umsetzung liegt in `os_ai::run`; die Anwendung (App, CLI,
//! MCP STDIO) übergibt sie in `EngineDeps`. Ohne Schalter „KI (Vorschau)“ gibt es keine Warteschlange in den
//! `EngineDeps` (Verhalten von 1.0).
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use crate::CrawlConfig;

/// Höchstlänge von `image_context` (AI-032).
pub const IMAGE_CONTEXT_MAX_CHARS: usize = 300;
/// Speichergrenze des Inhaltsbereich-Texts in den gespeicherten KI-Eingaben (08 AI-032, Nachtrag K4-24,
/// bestätigt 26.11.2026).
pub const STORED_PAGE_TEXT_MAX_CHARS: usize = 20_000;

/// Bildkontext eines Bildes der Seite (AI-032 `image_context`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageAiContext {
    /// Aufgelöste Bildadresse (sonst der Rohwert von `src`).
    pub src: String,
    /// Alt-Text, falls vorhanden.
    pub alt: Option<String>,
    /// Bildunterschrift bzw. umgebender Text, höchstens [`IMAGE_CONTEXT_MAX_CHARS`] Zeichen.
    pub context: Option<String>,
}

/// Woher die Felder kommen, die 1.0 nicht in eigenen Spalten speichert (Text des Inhaltsbereichs, alle H2,
/// `html_lang`, Bildkontext; 08 AI-032, Nachtrag „Quelle der Eingabefelder“).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputSource {
    /// Aus der gerade geparsten Seite (während des Crawls).
    #[default]
    Parsed,
    /// Aus den gespeicherten KI-Eingaben der Crawl-Datei (Punkt 1).
    Stored,
    /// Aus gespeichertem HTML (`page_html`) neu abgeleitet (Punkt 2; nichts wird nachträglich gespeichert).
    Derived,
    /// Weder gespeichert noch ableitbar (Punkt 3): Fragen mit diesen Feldern erhalten `Skipped (input unavailable)`.
    Unavailable,
}

/// Eingabefelder einer Seiten-URL der Grundmenge G (AI-032) samt den Angaben für den Geltungsbereich.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageAiFields {
    /// Kennung in `urls`.
    pub url_id: i64,
    /// Adresse.
    pub url: String,
    /// Inhaltsklasse in Kleinschreibung (`html`, …); leer bei Antworten ohne verwertbaren Inhalt.
    pub content_class: String,
    pub status_code: Option<u16>,
    pub indexable: bool,
    pub title: Option<String>,
    pub meta_description: Option<String>,
    /// Alle H1 in Reihenfolge (aus der Seite bzw. den Spalten `h1_1`, `h1_2`).
    pub h1: Vec<String>,
    /// Alle H2 in Dokumentreihenfolge.
    pub h2: Vec<String>,
    /// Text des Inhaltsbereichs (ONPAGE-140; im Rendering-Modus aus dem gerenderten HTML).
    pub page_text: Option<String>,
    pub word_count: Option<u64>,
    /// Rohwert von `<html lang>`, ohne `lang` `xml:lang` (wie ONPAGE-153).
    pub html_lang: Option<String>,
    /// Sprachcodes der selbstreferenzierenden hreflang-Angaben.
    pub hreflang_self: Vec<String>,
    /// Typen der extrahierten strukturierten Daten; `None` = Extraktion war aus (nicht vorhanden), leere Liste =
    /// extrahiert, nichts gefunden (K4-26).
    pub structured_data_types: Option<Vec<String>>,
    /// Bildkontext je Bild der Seite.
    pub images: Vec<ImageAiContext>,
    /// Quelle der Felder ohne eigene Spalte.
    pub source: InputSource,
}

impl PageAiFields {
    /// Interne HTML-Seite mit Status 200: nur für sie werden KI-Eingaben gespeichert (AI-032, Punkt 1).
    pub fn is_html_200(&self) -> bool {
        self.content_class == "html" && self.status_code == Some(200)
    }

    /// Ob Text des Inhaltsbereichs, H2, `html_lang` und Bildkontext vorliegen (Quelle nicht [`InputSource::Unavailable`]).
    pub fn has_page_inputs(&self) -> bool {
        self.source != InputSource::Unavailable
    }
}

/// Liest die Eingabefelder aller Seiten-URLs einer Crawl-Datei (für das Fortsetzen; Umsetzung in der Engine).
pub type PageLoader =
    Arc<dyn Fn(&Path, &CrawlConfig) -> Result<Vec<PageAiFields>, String> + Send + Sync>;

/// Beginn einer Sitzung der KI-Warteschlange (Start bzw. Fortsetzen eines Crawls).
#[derive(Clone)]
pub struct AiQueueStart {
    /// Crawl-Datei.
    pub crawl_file: PathBuf,
    /// Wirksame Konfiguration des Laufs.
    pub config: CrawlConfig,
    /// Fortsetzen (CRAWL-126) statt neuer Crawl: dieselbe Laufzeile, offene Einträge der bereits gecrawlten URLs.
    pub resumed: bool,
    /// `owner_kind` des Prozesses (`app`, `cli`, `mcp-stdio`).
    pub owner_kind: &'static str,
    /// Leser der bereits gecrawlten Seiten (nur beim Fortsetzen gesetzt).
    pub existing: Option<PageLoader>,
}

impl std::fmt::Debug for AiQueueStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiQueueStart")
            .field("crawl_file", &self.crawl_file)
            .field("resumed", &self.resumed)
            .field("owner_kind", &self.owner_kind)
            .finish_non_exhaustive()
    }
}

/// Wie der Crawl endete (08 AI-020, Nachtrag K4-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiQueueEnd {
    /// Pause (CRAWL-126): kein neuer Request, laufende werden zu Ende geführt.
    Paused,
    /// „Gestoppt (endgültig)“ (CRAWL-128): kein weiterer Request, laufende werden zu Ende geführt.
    Stopped,
    /// Ende ohne Stoppen (`queue_empty`, `limit_reached`): die Warteschlange läuft, bis sie leer ist.
    Drain,
    /// Fehler, geordnetes Beenden, Schließen der Crawl-Datei: laufende Requests werden abgebrochen.
    Abort,
}

/// Wird fertig, wenn die Warteschlange das Ende umgesetzt hat (bei [`AiQueueEnd::Drain`]: leer).
pub type AiQueueDone = Pin<Box<dyn Future<Output = ()> + Send>>;

/// KI-Warteschlange einer Anwendung (Umsetzung `os_ai::run`).
pub trait AiQueue: Send + Sync {
    /// Beginnt eine Sitzung für einen Crawl-Lauf; `None` = nichts zu tun (keine aktiven Fragen, keine Speicherung).
    fn begin(&self, start: AiQueueStart) -> Option<Arc<dyn AiQueueSession>>;
}

/// Sitzung der KI-Warteschlange für einen Crawl-Lauf.
pub trait AiQueueSession: Send + Sync {
    /// Eine gecrawlte interne Seiten-URL (blockiert nicht).
    fn page(&self, page: PageAiFields);
    /// Ende des Crawl-Laufs; bei [`AiQueueEnd::Drain`] wird die Zukunft erst mit leerer Warteschlange fertig.
    fn end(&self, end: AiQueueEnd) -> AiQueueDone;
}

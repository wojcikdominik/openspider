//! Sitzungszugriff für Werkzeuge, die im App-Prozess laufen (K-17, `docs/plan/M2.md` 3.5).
//!
//! Der Streamable-HTTP-Server der App sieht dieselben Crawls wie die Oberfläche: Welche Crawls die Sitzung hält, ob einer
//! läuft und wie weit er ist, kennt `os-app`. Damit `os-mcp` nie von `os-app` abhängt (3.2: „Nie: os-mcp → os-app“),
//! reicht die App ein Trait-Objekt in den [`crate::ToolContext`]: `ToolContext.session: Option<Arc<dyn SessionAccess>>`.
//! Der STDIO-Server (`openspider mcp`) hat keine App-Sitzung (`None`); M01 verwaltet eigene Engine-Handles.
//!
//! M2-F02 defines the transport-independent contract; os-app::mcp supplies the live implementation.

use std::path::PathBuf;

use os_model::{CrawlState, Progress};

/// Ein Crawl, den die Sitzung kennt (geöffnet oder laufend).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCrawl {
    /// Kennung des Crawls (UUID als Text).
    pub crawl_id: String,
    /// Name laut Crawl-Verwaltung.
    pub name: String,
    /// Projekt.
    pub project: String,
    /// Pfad der Crawl-Datei.
    pub file: PathBuf,
    /// Zustand (wirksam: ein „Running“ ohne Heartbeat gilt als `Incomplete`).
    pub state: CrawlState,
    /// Der Crawl läuft im Prozess der App und lässt sich dort steuern.
    pub controllable: bool,
}

/// Quelle der URLs eines neuen Crawls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSource {
    /// Spider-Modus ab einer Start-URL.
    Spider {
        /// Start-URL.
        url: String,
    },
    /// Listenmodus.
    List {
        /// URLs bzw. Zeilen.
        items: Vec<String>,
        /// Bezeichnung der Quelle.
        label: String,
    },
}

/// Auftrag, einen Crawl im App-Prozess zu starten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStart {
    /// Quelle.
    pub source: SessionSource,
    /// Name eines Konfigurationsprofils; `None` = aktuelle Konfiguration der App.
    pub profile: Option<String>,
}

/// Live-Stand eines Crawls der Sitzung.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionStatus {
    /// Zustand.
    pub state: CrawlState,
    /// Fortschritt, solange der Crawl läuft.
    pub progress: Option<Progress>,
}

/// Fehler des Sitzungszugriffs.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// Der Crawl ist der Sitzung unbekannt.
    #[error("Crawl nicht gefunden: {0}")]
    NotFound(String),
    /// Der Crawl läuft in einem anderen Prozess und lässt sich hier nicht steuern.
    #[error("Crawl wird nicht von dieser Sitzung geführt: {0}")]
    NotControllable(String),
    /// Ungültiger Auftrag (Start-URL, Konfiguration).
    #[error("ungültiger Auftrag: {0}")]
    Invalid(String),
    /// Sonstiger Fehler der App.
    #[error("Sitzungsfehler: {0}")]
    Internal(String),
    /// Die App ist vorübergehend belegt (andere Operation, Crawl hält an); wiederholbar (AGENT-020, Code `busy`).
    #[error("App belegt: {0}")]
    Busy(String),
}

/// Zugriff auf die Sitzung der App. Die Implementierung liefert `os-app` (K-17).
pub trait SessionAccess: Send + Sync + std::fmt::Debug {
    fn current_config(&self) -> Option<os_model::CrawlConfig> {
        None
    }

    /// Start with validated safe overrides and the immutable MCP target policy.
    fn start_configured(
        &self,
        start: SessionStart,
        _config: os_model::CrawlConfig,
        _policy: crate::permissions::DomainPolicy,
    ) -> Result<SessionCrawl, SessionError> {
        self.start_crawl(start)
    }
    fn resume_crawl(
        &self,
        id: &str,
        _policy: crate::permissions::DomainPolicy,
    ) -> Result<SessionCrawl, SessionError> {
        Err(SessionError::NotControllable(id.into()))
    }

    /// Pausiert einen Crawl der Sitzung fortsetzbar (AGENT-020 `pause_crawl`, CRAWL-126).
    fn pause_crawl(&self, crawl_id: &str) -> Result<(), SessionError> {
        Err(SessionError::NotControllable(crawl_id.into()))
    }

    /// Die Crawls der Sitzung.
    fn crawls(&self) -> Vec<SessionCrawl>;

    /// Live-Stand eines Crawls; `None`, wenn die Sitzung ihn nicht kennt.
    fn status(&self, crawl_id: &str) -> Option<SessionStatus>;

    /// Startet einen Crawl im App-Prozess.
    fn start_crawl(&self, start: SessionStart) -> Result<SessionCrawl, SessionError>;

    /// Beendet einen laufenden oder pausierten Crawl endgültig (Endgrund `stopped`, UI-003; die Warteschlange bleibt
    /// gesichert, „Fortsetzen“ ist danach ausgeschlossen, CRAWL-128).
    fn stop_crawl(&self, crawl_id: &str) -> Result<(), SessionError>;
}

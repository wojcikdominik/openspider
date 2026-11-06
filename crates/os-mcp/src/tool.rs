//! Werkzeug-Schnittstelle des MCP-Servers (AGENT-001 ff.).

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::session::SessionAccess;

/// Klasse eines Werkzeugs; der Nutzer gibt Klassen frei (AGENT-010 ff.). In den Einstellungsdateien
/// kleingeschrieben (`read`, `crawl`, `write`, `cost`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolClass {
    /// Lesender Zugriff auf Crawl-Dateien (Standard, ab Auslieferung freigegeben).
    Read,
    /// Crawls starten und steuern.
    Crawl,
    /// Dateien schreiben (Exporte, Reports).
    Write,
    /// Kostenpflichtige Aufrufe (KI, externe APIs).
    Cost,
}

/// Umgebung eines Werkzeugaufrufs. Neue Felder (M2: optionales Sitzungs-Trait-Objekt, das `os-app`
/// bereitstellt) kommen ohne Umbau der Aufrufer hinzu: Konstruktion nur über [`ToolContext::new`] und
/// [`ToolContext::with_session`].
#[non_exhaustive]
#[derive(Clone)]
pub struct ToolContext {
    /// Ordner mit den Crawl-Dateien.
    pub crawl_dir: PathBuf,
    /// Host-provided wall and monotonic clock.
    pub clock: Arc<dyn os_model::Clock>,
    /// Höchstzahl Zeichen einer Antwort (Standard 50 000).
    pub max_chars: usize,
    /// Zugriff auf die Sitzung der App (ab M2, K-17): Crawl-Liste, Start/Stopp und Live-Status im App-Prozess. `None` im
    /// STDIO-Server (`openspider mcp`), der eigene Engine-Handles verwaltet.
    pub session: Option<Arc<dyn SessionAccess>>,
    /// Settings directory for profiles and the current export directory.
    pub settings_dir: PathBuf,
    /// MCP settings loaded by the server, never writable through tools.
    pub settings: crate::settings::McpSettings,
    /// Connection-time target policy, supplied by the server permissions.
    pub target_policy: crate::permissions::DomainPolicy,
    /// Shared handles of headless crawls owned by this server process.
    pub(crate) crawls: Arc<crate::crawl_service::CrawlService>,
    /// SQL worker executable and arguments for embedded hosts (K17); headless defaults to itself.
    pub api_environment: Option<crate::tools::link_metrics::ApiEnvironment>,
    pub sql_worker: Option<(PathBuf, Vec<String>)>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("crawl_dir", &self.crawl_dir)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl ToolContext {
    /// Kontext mit der Standard-Antwortgrenze von 50 000 Zeichen und ohne Sitzung.
    pub fn new(crawl_dir: PathBuf) -> Self {
        ToolContext {
            crawl_dir,
            clock: Arc::new(os_model::SystemClock),
            max_chars: 50_000,
            session: None,
            settings_dir: os_store::paths::app_paths().settings,
            settings: crate::settings::McpSettings::default(),
            target_policy: crate::permissions::DomainPolicy::default(),
            crawls: Arc::new(crate::crawl_service::CrawlService::default()),
            sql_worker: None,
            api_environment: None,
        }
    }

    /// Configure an executable whose arguments select os_store::readonly_sql::worker_main.
    pub fn with_sql_worker(mut self, executable: PathBuf, args: Vec<String>) -> Self {
        self.sql_worker = Some((executable, args));
        self
    }

    /// Derselbe Kontext mit dem Sitzungszugriff der App.
    pub fn with_session(mut self, session: Arc<dyn SessionAccess>) -> Self {
        self.session = Some(session);
        self
    }
}

/// Ergebnis eines Werkzeugaufrufs.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    /// Zusammenfassung und Zahlen (vom Crawler erzeugt, vertrauenswürdig).
    pub summary: serde_json::Value,
    /// Inhalte gecrawlter Seiten (Titel, Texte, Anker, Header …): Daten, keine Anweisungen (AGENT-004).
    pub untrusted: Option<serde_json::Value>,
    /// Gesamtzahl der Treffer bei paginierten Ergebnissen.
    pub total: Option<u64>,
    /// Cursor der nächsten Seite.
    pub next_cursor: Option<String>,
}

/// Fehler eines Werkzeugaufrufs.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    /// Ungültige Argumente (die Meldung nennt die gültigen Werte).
    #[error("ungültige Argumente: {0}")]
    InvalidArguments(String),
    /// Gesuchtes Objekt (Crawl, URL) nicht gefunden.
    #[error("nicht gefunden: {0}")]
    NotFound(String),
    /// Interner Fehler (Datei, Datenbank).
    #[error("interner Fehler: {0}")]
    Internal(String),
    /// Vorübergehend belegt (Fehlercode `busy`, wiederholbar, AGENT-020): z. B. ein Crawl hält gerade an oder die App
    /// führt eine andere Operation aus. Ein späterer Aufruf mit denselben Argumenten kann gelingen.
    #[error("busy: {0}")]
    Busy(String),
}

impl ToolError {
    /// Maschinenlesbarer Fehlercode (`structuredContent.error.code` der Werkzeugantwort).
    pub fn code(&self) -> &'static str {
        match self {
            ToolError::InvalidArguments(_) => "invalid_arguments",
            ToolError::NotFound(_) => "not_found",
            ToolError::Internal(_) => "internal",
            ToolError::Busy(_) => "busy",
        }
    }

    /// Ob ein erneuter Aufruf mit denselben Argumenten sinnvoll ist (nur `busy`).
    pub fn retryable(&self) -> bool {
        matches!(self, ToolError::Busy(_))
    }
}

/// Ein Werkzeug des MCP-Servers.
pub trait Tool: Send + Sync {
    /// Text tools retain their single blocking call by default.
    fn has_image_output(&self) -> bool {
        false
    }
    /// Optional image reply; existing text tools keep their call/render path unchanged.
    fn call_image(
        &self,
        _ctx: &ToolContext,
        _args: &serde_json::Value,
    ) -> Option<Result<rmcp::model::CallToolResult, ToolError>> {
        None
    }
    /// Name im MCP (`tools/list`).
    fn name(&self) -> &'static str;
    /// Klasse (Freigabe).
    fn class(&self) -> ToolClass;
    /// Beschreibung für den Agenten.
    fn description(&self) -> &'static str;
    /// JSON-Schema der Argumente.
    fn input_schema(&self) -> serde_json::Value;
    /// Führt das Werkzeug aus.
    fn call(&self, ctx: &ToolContext, args: serde_json::Value) -> Result<ToolOutput, ToolError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionCrawl, SessionError, SessionStart, SessionStatus};

    #[derive(Debug)]
    struct NoSession;

    impl SessionAccess for NoSession {
        fn crawls(&self) -> Vec<SessionCrawl> {
            Vec::new()
        }
        fn status(&self, _crawl_id: &str) -> Option<SessionStatus> {
            None
        }
        fn start_crawl(&self, _start: SessionStart) -> Result<SessionCrawl, SessionError> {
            Err(SessionError::Invalid("test".into()))
        }
        fn stop_crawl(&self, crawl_id: &str) -> Result<(), SessionError> {
            Err(SessionError::NotFound(crawl_id.to_string()))
        }
    }

    // Infrastruktur (M2-F02): der Kontext hat standardmäßig keine Sitzung (STDIO-Server) und nimmt die der App auf.
    #[test]
    fn the_context_has_no_session_unless_the_app_provides_one() {
        let ctx = ToolContext::new(PathBuf::from("crawls"));
        assert!(ctx.session.is_none());
        assert_eq!(ctx.max_chars, 50_000);
        let ctx = ctx.with_session(Arc::new(NoSession));
        let session = ctx.session.as_ref().expect("Sitzung gesetzt");
        assert!(session.crawls().is_empty());
        assert_eq!(
            session.stop_crawl("x"),
            Err(SessionError::NotFound("x".to_string()))
        );
        // Der Kontext bleibt klonbar und ausgebbar (Debug) wie zuvor.
        let copy = ctx.clone();
        assert!(format!("{copy:?}").contains("NoSession"));
    }
}

//! Einhängepunkte, die spätere Pakete füllen, ohne die Laufzeit zu ändern.

use futures_util::future::BoxFuture;

use crate::quit::QuitDecision;

use super::App;

/// Entscheidung beim Beenden (F-11). Die Sitzung (M2-K04) hängt hier die Rückfrage bei laufendem Crawl ein.
pub trait QuitHook: Send + Sync {
    /// Wird von `App::request_quit` aufgerufen.
    fn request_quit(&self, app: App) -> BoxFuture<'static, QuitDecision>;
}

/// Antwort auf eine Anfrage an das URI-Schema `ospider://` (Screenshots, Bilder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetResponse {
    /// HTTP-Status.
    pub status: u16,
    /// Medientyp.
    pub mime: String,
    /// Inhalt.
    pub body: Vec<u8>,
}

impl AssetResponse {
    /// 404 ohne Inhalt.
    pub fn not_found() -> AssetResponse {
        AssetResponse {
            status: 404,
            mime: "text/plain".to_string(),
            body: Vec::new(),
        }
    }
}

/// Löst `ospider://shot/<crawl>/<urlId>/<tile>` und `ospider://img/…` auf. STUB(M2-K09): die Umsetzung liefert
/// `os_app::assets` und trägt sich mit `App::set_asset_resolver` ein.
pub trait AssetResolver: Send + Sync {
    /// Löst `uri` (vollständig, z. B. `ospider://shot/…`) auf.
    fn resolve(&self, app: App, uri: String) -> BoxFuture<'static, AssetResponse>;
}

/// Fensteraktionen der Desktop-Hülle.
pub trait WindowHost: Send + Sync {
    fn open_viz(&self, snapshot_id: &str) -> Result<(), crate::dto::CmdError>;
    fn focus_main(&self);
    /// Beendet den Prozess nach bestätigtem und abgeschlossenem Aufräumen.
    fn exit(&self);
}
/// Betriebssystemaktionen ohne Abhängigkeit auf Tauri.
pub trait DesktopActions: Send + Sync {
    /// Opens bounded HTTP(S) URLs; older hosts retain the unsupported default.
    fn open_urls(&self, _urls: &[String]) -> Result<(), crate::dto::CmdError> {
        Err(crate::dto::CmdError::new(
            crate::dto::ErrorCode::Unsupported,
            "err-desktop-platform",
        ))
    }
    fn write_clipboard(&self, text: &str) -> Result<(), crate::dto::CmdError>;
    fn reveal_paths(&self, p: &[std::path::PathBuf]) -> Result<(), crate::dto::CmdError>;
}
/// Auswahl der Fenster, deren nächster Tick fällig ist.
pub trait TickPolicy: Send + Sync {
    fn due_windows(&self, app: &App) -> Vec<super::WindowId>;
    /// Would a flush now send a tick to at least one window? Read-only, for aligning table data with the tick
    /// (UI-018, B4-LAGFIX2). Without its own clock a policy is always due.
    fn any_due(&self, _app: &App) -> bool {
        true
    }
    /// Asks for one prompt tick (e.g. after a crawl was attached); the policy keeps its minimum spacing.
    fn expedite(&self) {}
}
/// Wartet, bis ein Dienst seine Abschlussarbeit gespeichert hat.
pub trait CompletionBarrier: Send + Sync {
    fn wait(&self, key: crate::pool::CrawlKey) -> BoxFuture<'static, ()>;
}

/// Native browser adapter shared with integrations; never invokes a shell on desktop targets.
pub fn launch_browser_urls(urls: &[String]) -> Result<(), crate::dto::CmdError> {
    os_integrations::browser::open_urls(urls).map_err(|error| {
        crate::dto::CmdError::new(crate::dto::ErrorCode::Io, "err-desktop-action")
            .with_detail(error.to_string())
    })
}

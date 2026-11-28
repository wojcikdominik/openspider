//! Einwilligung je Installation (AI-018) außerhalb der App: CLI und MCP STDIO lesen dieselbe Datei `ai-consent.json` im
//! Einstellungsordner wie die App (M4-U01, `FileConsentStore` der App). Ohne erteilte Einwilligung bleiben die Einträge
//! `Pending (consent)`; `--ai-consent` baut M4-C01.
use std::path::PathBuf;

use crate::policy::{ConsentBook, ConsentStore};

/// Dateiname im Einstellungsordner (wie die App).
pub const CONSENT_FILE: &str = "ai-consent.json";

/// Ablage der Einwilligung in einer JSON-Datei (Schreiben über eine temporäre Datei und Umbenennen).
#[derive(Debug, Clone)]
pub struct FileConsentStore {
    path: PathBuf,
}

impl FileConsentStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl ConsentStore for FileConsentStore {
    fn load(&self) -> Result<ConsentBook, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => ConsentBook::from_json(&text).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConsentBook::new()),
            Err(e) => Err(e.to_string()),
        }
    }
    fn save(&self, book: &ConsentBook) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, book.to_json()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

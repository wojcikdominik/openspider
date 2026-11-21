//! Schalter „KI (Vorschau)“ (Plan M4 E1, Nutzerentscheidung E-2 b): Bis zur Freigabe mit Release 1.1 bleibt
//! jede KI-Funktion hinter dieser App-Einstellung (Standard aus). Ist sie aus, schickt `os-ai` keinen Request
//! an einen Anbieter ([`crate::provider::execute`] ruft den Adapter nicht auf), und die KI-Schritte eines
//! Profils werden für den Lauf abgeschaltet ([`PreviewSwitch::restrict`]). Die Oberfläche dazu baut M4-F02.
use os_model::config::AiAppSettings;
use os_model::settings::AppSettings;
use os_model::CrawlConfig;

/// Zustand des Schalters; nur aus den App-Einstellungen ableitbar, nie aus einem portablen Profil.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreviewSwitch {
    enabled: bool,
}

impl PreviewSwitch {
    /// Schalter aus (Standard).
    pub const OFF: Self = Self { enabled: false };
    /// Schalter an.
    pub const ON: Self = Self { enabled: true };

    /// Liest den Schalter aus den App-Einstellungen.
    pub fn from_settings(settings: &AppSettings) -> Self {
        Self::from_ai_settings(&settings.ai_app)
    }

    /// Liest den Schalter aus dem KI-Abschnitt der App-Einstellungen.
    pub fn from_ai_settings(settings: &AiAppSettings) -> Self {
        Self {
            enabled: settings.preview_enabled,
        }
    }

    /// Ob KI-Funktionen laufen dürfen.
    pub fn is_on(self) -> bool {
        self.enabled
    }

    /// Schaltet bei ausgeschaltetem Schalter alle KI-Teile eines Profils für den Lauf ab: KI-Aufgaben,
    /// Embeddings und die Analyse-Schritte `embeddings` und `ai_search`. Das gespeicherte Profil bleibt
    /// unverändert; der Aufrufer wendet das auf seine Laufkopie an. Bei eingeschaltetem Schalter ändert
    /// sich nichts.
    pub fn restrict(self, config: &mut CrawlConfig) {
        if self.enabled {
            return;
        }
        config.ai.enabled = false;
        config.embeddings.enabled = false;
        config.analysis.embeddings = false;
        config.analysis.ai_search = false;
    }
}

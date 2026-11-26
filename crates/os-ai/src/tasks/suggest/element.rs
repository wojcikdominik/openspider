//! Elemente der Optimierungsvorschläge (AI-070). Die 1.1-Etappe von M4-V01 baut Title (AI-071) und Meta Description
//! (AI-072); H1 (AI-074) und Alt-Text (AI-073) ergänzt M4-V02 (eigene Dateien `h1.rs`, `alt.rs`).
use crate::catalog::TaskType;
use crate::policy::{FieldSet, InputField};
use serde::{Deserialize, Serialize};

/// Ein Element, für das Vorschläge erzeugt werden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    Title,
    MetaDescription,
}

impl Element {
    /// Alle Elemente der 1.1-Etappe.
    pub const ALL: [Element; 2] = [Element::Title, Element::MetaDescription];

    /// Kennung in `ai_suggestions.element` und in den Commands.
    pub fn id(self) -> &'static str {
        match self {
            Self::Title => os_store::ai::suggestions::ELEMENT_TITLE,
            Self::MetaDescription => os_store::ai::suggestions::ELEMENT_META_DESCRIPTION,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.id() == id)
    }

    /// Aufgabentyp (Modellwahl AI-011, Budget-Teilbudget, Protokollspalte `Task`, Cache-Schlüssel).
    pub fn task(self) -> TaskType {
        match self {
            Self::Title => TaskType::SuggestTitle,
            Self::MetaDescription => TaskType::SuggestMeta,
        }
    }

    /// Kanonischer Name in Exporten (`Element`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::MetaDescription => "Meta Description",
        }
    }

    /// Schriftgröße des Desktop-Profils nach UI-038 (Titel 20 px, Beschreibung 14 px, Regular).
    pub fn font_px(self) -> f64 {
        match self {
            Self::Title => 20.0,
            Self::MetaDescription => 14.0,
        }
    }

    /// Kennung der Vorlage (Cache-Schlüssel `definition_id`, AI-015).
    pub fn definition_id(self) -> &'static str {
        self.task().as_str()
    }

    /// Frage, deren Antwort `false` die URL in den Standard-Geltungsbereich aufnimmt (AI-054, AI-055).
    pub fn mismatch_question(self) -> &'static str {
        match self {
            Self::Title => "title-matches-content",
            Self::MetaDescription => "meta-description-matches-content",
        }
    }

    /// Felder, die ein Request dieses Elements senden darf (Einwilligung, AI-018; Kontext nach AI-070).
    pub fn fields(self) -> FieldSet {
        FieldSet::new([
            InputField::Url,
            InputField::Title,
            InputField::MetaDescription,
            InputField::H1,
            InputField::PageText,
            InputField::HtmlLang,
            InputField::PageType,
        ])
    }

    /// Aktueller Wert des Elements einer Seite.
    pub fn current<'a>(self, page: &'a super::run::PageInput) -> Option<&'a str> {
        match self {
            Self::Title => page.title.as_deref(),
            Self::MetaDescription => page.meta_description.as_deref(),
        }
    }
}

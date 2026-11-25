//! Modellkatalog (AI-010), Aufgabenprofile und Modellwahl je Aufgabe (AI-011).
//!
//! - [`pricing`]: Preistabelle `resources/ai/pricing.json` mit Preisschlüssel (Anbieter, Modell, Endpunkt), Quellen,
//!   Prüfdatum, Nutzerüberschreibungen sowie Fähigkeiten und Reasoning-Stufen der OpenAI-Katalogmodelle.
//! - [`task_defaults`]: Standardwerte je Aufgabentyp `resources/ai/task-defaults.json` (Reasoning-Stufe,
//!   Antwortanteil; Standardmodelle nach Bestätigung durch den Nutzer, E-4 b / K4-20).
//! - [`reasoning`]: verfügbare Stufen je Anbieter, effektive Stufe, Warnkennzeichen, `max_tokens`-Regel
//!   (AI-011, Nachtrag Q00; dieselbe Regel für Chat-Prompts aus 04, INTEG-162).
//! - [`limits`]: `resources/ai/provider-limits.json` (Schätzungsanteile aus der Live-Probe Q00, AI-012).
//! - [`choice`]: Modellwahl je Aufgabe mit Ersatzliste, Prüfung der Fähigkeiten, Profil-Export/-Import.
pub mod choice;
pub mod limits;
pub mod pricing;
pub mod reasoning;
pub mod task_defaults;

use serde::{Deserialize, Serialize};

pub use choice::{
    plan_task, validate_assignment, AvailableModel, ModelAvailability, ModelRef, ProviderState,
    TaskPlan, TaskSettings,
};
pub use limits::ProviderLimitsFile;
pub use pricing::{PriceCatalog, PriceEntry, PriceKey, PriceOverride, PriceTable, ResolvedPrice};
pub use reasoning::{
    chat_prompt_max_tokens, choose_reasoning, decision_answer_tokens, default_max_tokens,
    reasoning_reserve, AnswerTokens, OutputLimit, ReasoningChoice, ReasoningWarning,
};
pub use task_defaults::{TaskDefault, TaskDefaults, TaskType};

/// Endpunkt eines Preisschlüssels (AI-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Endpoint {
    Messages,
    Responses,
    Decisions,
    ChatCompletions,
    Embeddings,
}

impl Endpoint {
    pub const ALL: [Endpoint; 5] = [
        Self::Messages,
        Self::Responses,
        Self::Decisions,
        Self::ChatCompletions,
        Self::Embeddings,
    ];

    /// Name wie in `provider::Model::endpoint` und in der Preistabelle.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Responses => "responses",
            Self::Decisions => "decisions",
            Self::ChatCompletions => "chat-completions",
            Self::Embeddings => "embeddings",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.as_str() == text)
    }
}

/// Fehler beim Laden oder Auflösen von Katalogdaten. Enthält nie Schlüssel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    /// Datei verletzt Schema oder Regeln (Befunde mit JSON-Pointer).
    Invalid(String),
    /// Datei nicht lesbar.
    Io(String),
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(m) => write!(f, "Katalogdaten ungültig: {m}"),
            Self::Io(m) => write!(f, "Katalogdatei nicht lesbar: {m}"),
        }
    }
}

impl std::error::Error for CatalogError {}

/// Kalenderdatum `JJJJ-MM-TT` (Prüfdaten `verified_at`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl CivilDate {
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let days_in_month = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        (1..=days_in_month)
            .contains(&day)
            .then_some(Self { year, month, day })
    }

    /// Liest genau `JJJJ-MM-TT`.
    pub fn parse(text: &str) -> Option<Self> {
        let b = text.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let num = |s: &str| {
            s.bytes()
                .all(|c| c.is_ascii_digit())
                .then(|| s.parse::<u32>().ok())
                .flatten()
        };
        let year = i32::try_from(num(&text[..4])?).ok()?;
        Self::new(year, num(&text[5..7])?, num(&text[8..])?)
    }

    /// Tage seit 1970-01-01 (proleptisch gregorianisch).
    pub fn days_from_epoch(self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Tage von `earlier` bis `self` (negativ, wenn `earlier` später liegt).
    pub fn days_since(self, earlier: CivilDate) -> i64 {
        self.days_from_epoch() - earlier.days_from_epoch()
    }
}

impl std::fmt::Display for CivilDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-010
    #[test]
    fn civil_dates_parse_and_count_days() {
        let a = CivilDate::parse("2026-11-24").unwrap();
        assert_eq!(a.to_string(), "2026-11-24");
        assert_eq!(CivilDate::parse("1970-01-01").unwrap().days_from_epoch(), 0);
        assert_eq!(a.days_since(CivilDate::parse("2026-07-27").unwrap()), 120);
        assert_eq!(a.days_since(CivilDate::parse("2024-02-29").unwrap()), 999);
        assert!(CivilDate::parse("2026-02-29").is_none());
        assert!(CivilDate::parse("2026-1-01").is_none());
        assert!(CivilDate::parse("2026-13-01").is_none());
    }
}

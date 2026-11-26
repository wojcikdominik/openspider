//! Grenzwerte und Optionen der Vorschläge (AI-070, AI-075).
//!
//! Die Längen- und Pixelgrenzen kommen aus den Preferences der Crawl-Konfiguration (`title.*`, `meta_description.*`,
//! dieselben Werte wie die Filter aus ONPAGE-093 ff. bzw. ONPAGE-103 ff.). Ein Kandidat ist genau dann gültig nach den
//! Regeln (1) und (2), wenn er in keinem dieser Filter landen würde: `Over X` heißt `> max`, `Below X` heißt
//! `0 < Wert < min` (fehlende Werte gehören dort nicht dazu – ein leerer Kandidat ist eigens ein Verstoß).
use super::element::Element;
use os_model::CrawlConfig;
use serde::{Deserialize, Serialize};

/// Längen- und Pixelgrenzen eines Elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementLimits {
    pub min_chars: u32,
    pub max_chars: u32,
    pub min_px: u32,
    pub max_px: u32,
}

impl ElementLimits {
    /// Aktuelle Grenzwerte aus der Crawl-Konfiguration (Preferences).
    pub fn from_config(element: Element, cfg: &CrawlConfig) -> Self {
        let t = match element {
            Element::Title => &cfg.title,
            Element::MetaDescription => &cfg.meta_description,
        };
        Self {
            min_chars: t.min_chars,
            max_chars: t.max_chars,
            min_px: t.min_px,
            max_px: t.max_px,
        }
    }
}

/// Regeln aus AI-075, einzeln abschaltbar außer (2) Pixelbreite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleSet {
    /// (1) Zeichenlänge.
    pub length: bool,
    /// (3) Dubletten (Stufe 1 und Nachlauf Stufe 2, AI-071).
    pub duplicates: bool,
    /// (4) Platzhalter, Zeilenumbrüche, umschließende Anführungszeichen.
    pub format: bool,
    /// (5) Sprache gleich Seitensprache.
    pub language: bool,
    /// (6) Stil- und Markenregeln (AI-077, Release 1.2).
    pub style: bool,
}

impl Default for RuleSet {
    fn default() -> Self {
        Self {
            length: true,
            duplicates: true,
            format: true,
            language: true,
            style: true,
        }
    }
}

/// Einstellungen eines Vorschlagslaufs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SuggestOptions {
    /// Kandidaten je Request (Standard 3, Bereich 1–5).
    pub candidates: u8,
    /// Reparaturrunden nach AI-075 (Standard 2, Bereich 0–3).
    pub repair_rounds: u8,
    /// Reparaturversuche bei ungültiger Antwort nach AI-021 (Standard 1, Bereich 0–2).
    pub schema_repairs: u8,
    pub rules: RuleSet,
}

impl Default for SuggestOptions {
    fn default() -> Self {
        Self {
            candidates: 3,
            repair_rounds: 2,
            schema_repairs: 1,
            rules: RuleSet::default(),
        }
    }
}

/// Fehlerhafte Option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionError(pub String);

impl std::fmt::Display for OptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OptionError {}

impl SuggestOptions {
    /// Prüft die Bereiche aus AI-070, AI-075 und AI-021.
    pub fn validated(self) -> Result<Self, OptionError> {
        if !(1..=5).contains(&self.candidates) {
            return Err(OptionError(format!(
                "candidates must be 1–5, got {}",
                self.candidates
            )));
        }
        if self.repair_rounds > 3 {
            return Err(OptionError(format!(
                "repair rounds must be 0–3, got {}",
                self.repair_rounds
            )));
        }
        if self.schema_repairs > crate::schema::RepairAttempts::MAX {
            return Err(OptionError(format!(
                "schema repairs must be 0–2, got {}",
                self.schema_repairs
            )));
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-070 [M4]
    // spec: AI-075 [M4]
    #[test]
    fn defaults_and_ranges_follow_the_spec() {
        let d = SuggestOptions::default();
        assert_eq!((d.candidates, d.repair_rounds, d.schema_repairs), (3, 2, 1));
        assert!(d.validated().is_ok());
        for bad in [
            SuggestOptions { candidates: 0, ..d },
            SuggestOptions { candidates: 6, ..d },
            SuggestOptions { repair_rounds: 4, ..d },
            SuggestOptions { schema_repairs: 3, ..d },
        ] {
            assert!(bad.validated().is_err(), "{bad:?}");
        }
        let cfg = CrawlConfig::default();
        assert_eq!(
            ElementLimits::from_config(Element::Title, &cfg),
            ElementLimits {
                min_chars: 30,
                max_chars: 60,
                min_px: 200,
                max_px: 561
            }
        );
        assert_eq!(
            ElementLimits::from_config(Element::MetaDescription, &cfg),
            ElementLimits {
                min_chars: 70,
                max_chars: 155,
                min_px: 400,
                max_px: 985
            }
        );
    }
}

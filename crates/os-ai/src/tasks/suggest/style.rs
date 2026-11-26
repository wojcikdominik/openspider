//! Anschluss für Stil- und Markenregeln (AI-077, Release 1.2; M4-V01 1.2-Etappe).
//!
//! AI-077 verlangt, dass die Regeln in den Prompt einfließen **und** lokal geprüft werden (AI-075 Regel 6). Die
//! 1.1-Etappe baut nur die beiden Anschlussstellen: [`StyleRules::prompt_lines`] (Zeilen im Prompt) und
//! [`StyleRules::violations`] (lokale Prüfung, Codes wie `forbidden_word:<wort>`, `missing_brand`,
//! `missing_required:<begriff>`). [`NoStyleRules`] ist der 1.1-Stand: keine Regeln, keine Verstöße. Der Markenzusatz
//! („wird bei der Pixelprüfung mitgerechnet“) setzt in 1.2 an [`StyleRules::rendered`] an, damit Längen- und
//! Pixelprüfung den vollständigen Title messen.
use super::element::Element;

/// Stil- und Markenregeln eines Profils bzw. Segments.
pub trait StyleRules: Send + Sync {
    /// Zusätzliche Anweisungen für den Prompt (leer = keine).
    fn prompt_lines(&self, element: Element) -> Vec<String>;
    /// Verstöße eines Kandidaten (nach AI-075 Regel 6).
    fn violations(&self, element: Element, candidate: &str) -> Vec<String>;
    /// Text, der gemessen und übernommen wird (z. B. mit Markenzusatz); 1.1: unverändert.
    fn rendered(&self, _element: Element, candidate: &str) -> String {
        candidate.to_string()
    }
}

/// Keine Stilregeln (Release 1.1).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoStyleRules;

impl StyleRules for NoStyleRules {
    fn prompt_lines(&self, _element: Element) -> Vec<String> {
        Vec::new()
    }
    fn violations(&self, _element: Element, _candidate: &str) -> Vec<String> {
        Vec::new()
    }
}

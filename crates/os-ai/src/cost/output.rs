//! Ausgabe-Schätzung (AI-012, Nachtrag 24.11.2026, Q00-H1): Est. Output Tokens je Request = (A + E) ×
//! Ausgabe-Korrekturfaktor, A = Antwortanteil der Aufgabe (AI-011, [`crate::catalog::AnswerTokens`]), E = erwartete
//! Reasoning-Tokens der **effektiven** Stufe ([`expected_reasoning_tokens`]). Der Ausgabe-Korrekturfaktor wird wie der
//! Eingabefaktor gelernt (Median Ist/Schätzung der letzten 200 protokollierten Requests ab 20), aber je (Modell,
//! Aufgabe, effektive Stufe) und je Protokollzeile ([`output_calibration_from_log`], Schätzung in der internen Spalte
//! `estimated_output_tokens`). Reasoning-Tokens sind in der Ausgabe enthalten und werden
//! zum Ausgabepreis gerechnet. Über die Decisions API ist die Ausgabe 0.
use super::estimate::{Calibration, CALIBRATION_WINDOW};
use crate::provider::ReasoningLevel;

/// E: erwartete Reasoning-Tokens der effektiven Stufe (**OS-Festlegung** in AI-012, Kalibrierung in Q02). `Off` ist
/// „sicher aus“ (eine Stufe `Off` gibt es nur, wenn der Anbieter das Abschalten bestätigt).
pub fn expected_reasoning_tokens(level: ReasoningLevel) -> u64 {
    match level {
        ReasoningLevel::Off => 0,
        ReasoningLevel::Minimal => 64,
        ReasoningLevel::Low => 128,
        ReasoningLevel::Medium => 512,
        ReasoningLevel::High => 2_048,
        ReasoningLevel::XHigh => 4_096,
        ReasoningLevel::Max => 8_192,
        ReasoningLevel::ModelDefault => 512,
    }
}

/// A + E vor dem Korrekturfaktor.
pub fn raw_output_tokens(answer_tokens: u64, effective: ReasoningLevel) -> u64 {
    answer_tokens.saturating_add(expected_reasoning_tokens(effective))
}

/// Est. Output Tokens je Request = (A + E) × Ausgabe-Korrekturfaktor (aufgerundet).
pub fn estimate_output_tokens(
    answer_tokens: u64,
    effective: ReasoningLevel,
    calibration: &Calibration,
) -> u64 {
    calibration.apply(raw_output_tokens(answer_tokens, effective))
}

/// Ausgabe-Korrekturfaktor einer (Anbieter, Modell, Aufgabe, effektive Stufe) aus dem Protokoll `ai_requests`: Median
/// aus Ist-Ausgabe-Tokens / gespeicherter Ausgabeschätzung (`estimated_output_tokens` = A + E ohne Faktor) **je
/// Protokollzeile** der letzten 200 erfolgreichen Requests dieser Kombination, ab 20 (sonst unkalibriert; AI-012,
/// Nachtrag 26.11.2026). Damit ist der Faktor auch bei variablem Antwortanteil (`decision-w`) exakt. `effective = None`
/// steht für Zeilen ohne Stufe.
pub fn output_calibration_from_log(
    file: &os_store::CrawlFile,
    provider: &str,
    model: &str,
    task: &str,
    effective: Option<ReasoningLevel>,
) -> Result<Calibration, os_store::StoreError> {
    os_store::ai::requests::output_calibration_samples(
        file.conn(),
        provider,
        model,
        task,
        effective.map(ReasoningLevel::as_str),
        CALIBRATION_WINDOW,
    )
    .map(Calibration::from_samples)
}

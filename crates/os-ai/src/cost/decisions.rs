//! Eingabe-Tokens über die Decisions API je Frage (AI-012, Nachtrag 24.11.2026, Q00-M1). **Ersetzt** für den Weg
//! `decisions-api` die Regel „⌈Zeichen / 3,5⌉ + Bild-Schätzwert + fester Anteil für Anweisungen“:
//!
//! Est. Input Tokens = Σ über die Fragen q des Requests [ ⌈Z_E / 3,5⌉ + b × B + ⌈Z_q / 3,5⌉ + G ]
//!
//! Z_E = Zeichen des gesendeten Eingabetexts, B = Bilder, b = Bild-Schätzwert der Preistabelle, Z_q = Zeichen des
//! Fragentexts ([`question_text_chars`]), G = Grundanteil je Frage aus `provider-limits.json`
//! (`decisions_question_overhead_tokens`, [`crate::catalog::ProviderLimitsFile::decisions_question_overhead`]).
//! Dieselbe Schätzung gilt für Reservierung (AI-013) und TPM-Begrenzung (AI-017). Die Bündelung mehrerer Fragen in einen
//! Request spart Requests, aber keine Tokens. Ausgabe-Tokens über die Decisions API: 0.
use super::tokens::text_tokens;
use serde_json::Value;
use std::sync::OnceLock;

/// Standard des Grundanteils G, falls die ausgelieferte `provider-limits.json` nicht lesbar wäre (AI-012: 100).
pub const DEFAULT_QUESTION_OVERHEAD_TOKENS: u64 = 100;

/// G aus der ausgelieferten `provider-limits.json` (`decisions_question_overhead_tokens`), einmal gelesen.
pub fn bundled_question_overhead() -> u64 {
    static G: OnceLock<u64> = OnceLock::new();
    *G.get_or_init(|| {
        crate::catalog::ProviderLimitsFile::bundled()
            .map(|f| f.decisions_question_overhead())
            .unwrap_or(DEFAULT_QUESTION_OVERHEAD_TOKENS)
    })
}

fn chars(value: &Value) -> u64 {
    value.as_str().map_or(0, |s| s.chars().count() as u64)
}

/// Z_q einer Frage im Format des Requests (AI-004/AI-030): gesendeter `name` + `instructions` + je `choices`-Eintrag
/// `value` und `description` + je `levels`-Eintrag `label` und `description` (bei Vorlagen nach dem Einsetzen der
/// Parameter, also so, wie sie gesendet wird).
pub fn question_text_chars(question: &Value) -> u64 {
    let list = |field: &str, a: &str, b: &str| -> u64 {
        question[field]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| chars(&entry[a]) + chars(&entry[b]))
            .sum()
    };
    chars(&question["name"])
        + chars(&question["instructions"])
        + list("choices", "value", "description")
        + list("levels", "label", "description")
}

/// Anteil einer Frage: ⌈Z_E / 3,5⌉ + b × B + ⌈Z_q / 3,5⌉ + G.
pub fn question_input_tokens(
    input_chars: u64,
    images: u64,
    image_token_estimate: u64,
    question_chars: u64,
    question_overhead_tokens: u64,
) -> u64 {
    text_tokens(input_chars)
        .saturating_add(images.saturating_mul(image_token_estimate))
        .saturating_add(text_tokens(question_chars))
        .saturating_add(question_overhead_tokens)
}

/// Geschätzte Eingabe-Tokens eines Decisions-Requests (vor dem Korrekturfaktor): Summe der Anteile seiner Fragen.
pub fn decisions_input_tokens(
    input_chars: u64,
    images: u64,
    image_token_estimate: u64,
    question_chars: &[u64],
    question_overhead_tokens: u64,
) -> u64 {
    question_chars
        .iter()
        .map(|&z_q| {
            question_input_tokens(
                input_chars,
                images,
                image_token_estimate,
                z_q,
                question_overhead_tokens,
            )
        })
        .fold(0u64, u64::saturating_add)
}

//! Parameterformen der Reasoning-Stufe je Anbieter (AI-002, AI-003, AI-005, Nachtrag 24.11.2026, Q00-H1) – die eine
//! Stelle, an der die live noch nicht belegten Formen stehen (V-A6, Prüfung in M4-Q00b):
//!
//! - Anthropic `thinking: {"type": "disabled"}` bzw. `{"type": "adaptive"}` (Form nach der öffentlichen Anthropic-Doku
//!   „Extended thinking“/„Adaptive thinking“ zum Bauzeitpunkt; die Probe Q00 hat `thinking` nie gesendet).
//! - OpenAI Responses und OpenRouter `reasoning: {"effort": <wert>}` mit `off` → `"none"` (OpenAI-Doku „Reasoning
//!   models“ zum Bauzeitpunkt; ob die Responses API `"none"` für `gpt-6-luna` annimmt, ist nicht live belegt).
//!
//! Welche Stufen ein Modell hat und welche effektive Stufe gilt, entscheiden die Adapter mit
//! [`crate::provider::resolve_reasoning_level`]; hier stehen nur die Formen.
use serde_json::{json, Value};

use crate::provider::ReasoningLevel;

/// Anthropic-Feld `thinking` für die effektive Stufe: `off` → `{"type": "disabled"}`, `minimal` … `max` →
/// `{"type": "adaptive"}`, `model-default` → kein Feld. Nie `{"type": "enabled"}` oder `budget_tokens`
/// (OS-Festlegung, AI-002 Nachtrag).
pub fn anthropic_thinking(level: ReasoningLevel) -> Option<Value> {
    match level {
        ReasoningLevel::ModelDefault => None,
        ReasoningLevel::Off => Some(json!({"type": "disabled"})),
        _ => Some(json!({"type": "adaptive"})),
    }
}

/// Wert des Aufwands (`output_config.effort` bei Anthropic, `reasoning.effort` bei OpenAI und OpenRouter): `off` →
/// `"none"`, sonst der Stufenname; `model-default` → keiner.
pub fn effort_value(level: ReasoningLevel) -> Option<&'static str> {
    match level {
        ReasoningLevel::ModelDefault => None,
        ReasoningLevel::Off => Some("none"),
        other => Some(other.as_str()),
    }
}

/// Objekt `reasoning` für OpenAI Responses und OpenRouter; `model-default` → kein Feld.
pub fn reasoning_object(level: ReasoningLevel) -> Option<Value> {
    effort_value(level).map(|effort| json!({"effort": effort}))
}

/// Stufe zu einem Anbieterwert (`reasoning_efforts`, `supported_efforts`, Anthropic-`effort`-Knoten): `none` → `off`,
/// sonst gleichnamig; unbekannte Werte (und die OpenSpider-Namen `off`, `model-default`) → `None`.
pub fn level_from_effort(value: &str) -> Option<ReasoningLevel> {
    match value {
        "none" => Some(ReasoningLevel::Off),
        "off" | "model-default" => None,
        other => other.parse().ok(),
    }
}

/// Stufen aus Anbieterwerten, aufsteigend und ohne Doppelungen.
pub fn levels_from_efforts<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<ReasoningLevel> {
    let mut levels: Vec<ReasoningLevel> =
        values.into_iter().filter_map(level_from_effort).collect();
    levels.sort_by_key(|level| level.rank());
    levels.dedup();
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-002 [M4]
    // spec: AI-003 [M4]
    // spec: AI-005 [M4]
    #[test]
    fn forms_per_level() {
        use ReasoningLevel::*;
        assert_eq!(anthropic_thinking(ModelDefault), None);
        assert_eq!(anthropic_thinking(Off), Some(json!({"type": "disabled"})));
        for level in [Minimal, Low, Medium, High, XHigh, Max] {
            assert_eq!(anthropic_thinking(level), Some(json!({"type": "adaptive"})));
            assert_eq!(effort_value(level), Some(level.as_str()));
        }
        assert_eq!(effort_value(Off), Some("none"));
        assert_eq!(reasoning_object(ModelDefault), None);
        assert_eq!(reasoning_object(Off), Some(json!({"effort": "none"})));
        assert_eq!(
            levels_from_efforts(["high", "none", "low", "bogus", "low", "off"]),
            vec![Off, Low, High]
        );
    }
}

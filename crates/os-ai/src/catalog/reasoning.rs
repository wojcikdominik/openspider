//! Reasoning-Stufe und Ausgabegrenze je Aufgabe (AI-011, Nachtrag 24.11.2026, Q00).
//!
//! Die effektive Stufe kommt ausschließlich aus [`resolve_reasoning_level`] (M4-K09). Dieses Modul liefert die
//! verfügbaren Stufen je Anbieterquelle, die Warnkennzeichen für den Aufgabendialog (U01) und die Regel
//! `max_tokens = min(2 × A + R(effektive Stufe), Ausgabegrenze des Modells)`.
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::provider::{resolve_reasoning_level, ReasoningLevel};

/// Antwortanteil A für Chat-Prompts aus 04 (INTEG-162/INTEG-179, Nachtrag 24.11.2026).
pub const CHAT_PROMPT_ANSWER_TOKENS: u64 = 1_024;
/// Formel-Kennung des Antwortanteils der Entscheidungsaufgaben (A = 64 + 16 × W).
pub const DECISION_W: &str = "decision-w";

/// Reasoning-Reserve R der effektiven Stufe (AI-011, Nachtrag). `Off` ist immer „sicher aus“, weil eine Stufe `Off`
/// nur verfügbar ist, wenn der Anbieter das Abschalten bestätigt.
pub fn reasoning_reserve(level: ReasoningLevel) -> u64 {
    match level {
        ReasoningLevel::Off => 0,
        ReasoningLevel::Minimal => 512,
        ReasoningLevel::Low => 1_024,
        ReasoningLevel::Medium => 4_096,
        ReasoningLevel::High => 8_192,
        ReasoningLevel::XHigh => 16_384,
        ReasoningLevel::Max => 32_768,
        ReasoningLevel::ModelDefault => 4_096,
    }
}

/// Antwortanteil einer Aufgabe: feste Tokenzahl oder Formel `decision-w`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnswerTokens {
    Fixed(u64),
    Formula(AnswerFormula),
}

/// Formeln für den Antwortanteil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnswerFormula {
    /// A = 64 + 16 × W (W = Summe der Gewichte der Fragen eines Requests).
    #[serde(rename = "decision-w")]
    DecisionW,
}

impl AnswerTokens {
    /// A für einen Request; `weight` ist W (nur für `decision-w` nötig, fehlt er, gilt W = 0).
    pub fn tokens(self, weight: Option<u64>) -> u64 {
        match self {
            Self::Fixed(a) => a,
            Self::Formula(AnswerFormula::DecisionW) => decision_answer_tokens(weight.unwrap_or(0)),
        }
    }
}

/// A = 64 + 16 × W (AI-011, Nachtrag).
pub fn decision_answer_tokens(weight: u64) -> u64 {
    64u64.saturating_add(16u64.saturating_mul(weight))
}

/// Gewicht einer Frage (AI-030-Format): `predicate` 2, `choice` Anzahl Werte, `score` Anzahl Levels; unbekannter Typ 0.
pub fn question_weight(question: &Value) -> u64 {
    let len = |field: &str| question[field].as_array().map_or(0, |a| a.len() as u64);
    match question["type"].as_str() {
        Some("predicate") => 2,
        Some("choice") => len("choices"),
        Some("score") => len("levels"),
        _ => 0,
    }
}

/// W eines Requests: Summe der Fragengewichte.
pub fn request_weight(questions: &[Value]) -> u64 {
    questions.iter().map(question_weight).sum()
}

/// Standard-Ausgabegrenze: min(2 × A + R(effektive Stufe), Ausgabegrenze des Modells); ohne Grenze keine Kappung.
pub fn default_max_tokens(
    answer_tokens: u64,
    effective: ReasoningLevel,
    model_limit: Option<u64>,
) -> u64 {
    let standard = answer_tokens
        .saturating_mul(2)
        .saturating_add(reasoning_reserve(effective));
    model_limit.map_or(standard, |limit| standard.min(limit))
}

/// `max_tokens` für Chat-Prompts aus 04 (INTEG-162, Nachtrag): A = 1.024, Stufe `model-default` (kein
/// Reasoning-Parameter), gekappt auf die Ausgabegrenze des Modells – höchstens 6.144.
pub fn chat_prompt_max_tokens(model_limit: Option<u64>) -> u64 {
    default_max_tokens(
        CHAT_PROMPT_ANSWER_TOKENS,
        ReasoningLevel::ModelDefault,
        model_limit,
    )
}

/// Wirksame Ausgabegrenze einer Aufgabe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputLimit {
    /// Gesendeter Wert.
    pub value: u64,
    /// Standard nach der Regel.
    pub standard: u64,
    /// Wert stammt vom Nutzer.
    pub user_set: bool,
    /// Nutzerwert kleiner als der Standard → Hinweis „Ausgabegrenze knapp – Antworten können abbrechen
    /// (Error: Output limit reached)“.
    pub tight: bool,
}

/// Ausgabegrenze aus Standard und optionalem Nutzerwert (der Nutzerwert ersetzt den Standard).
pub fn output_limit(standard: u64, user: Option<u64>) -> OutputLimit {
    match user {
        Some(value) => OutputLimit {
            value,
            standard,
            user_set: true,
            tight: value < standard,
        },
        None => OutputLimit {
            value: standard,
            standard,
            user_set: false,
            tight: false,
        },
    }
}

/// Warnkennzeichen für den Aufgabendialog (U01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningWarning {
    /// „aus nicht verfügbar – niedrigste Stufe: <effective>“.
    OffUnavailable { effective: ReasoningLevel },
    /// Gespeicherte Stufe für das Modell nicht verfügbar; es gilt `effective` (der gespeicherte Wert bleibt).
    StoredLevelUnavailable {
        stored: ReasoningLevel,
        effective: ReasoningLevel,
    },
}

/// Stufenwahl einer Aufgabe auf einem Modell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningChoice {
    /// Gespeicherte bzw. Standardstufe.
    pub requested: ReasoningLevel,
    /// Nach [`resolve_reasoning_level`].
    pub effective: ReasoningLevel,
    /// Einträge des Dropdowns: `model-default` und die verfügbaren Stufen, aufsteigend.
    pub dropdown: Vec<ReasoningLevel>,
    pub warning: Option<ReasoningWarning>,
}

/// Dropdown-Einträge für ein Modell (AI-011, Nachtrag): `model-default` und die verfügbaren Stufen.
pub fn dropdown_levels(available: &[ReasoningLevel]) -> Vec<ReasoningLevel> {
    std::iter::once(ReasoningLevel::ModelDefault)
        .chain(normalize_levels(available.iter().copied()))
        .collect()
}

/// Effektive Stufe, Dropdown und Warnung für `requested` auf einem Modell mit `available`.
pub fn choose_reasoning(
    requested: ReasoningLevel,
    available: &[ReasoningLevel],
) -> ReasoningChoice {
    let effective = resolve_reasoning_level(requested, available);
    let warning = if effective == requested {
        None
    } else if requested == ReasoningLevel::Off && effective != ReasoningLevel::ModelDefault {
        Some(ReasoningWarning::OffUnavailable { effective })
    } else {
        Some(ReasoningWarning::StoredLevelUnavailable {
            stored: requested,
            effective,
        })
    };
    ReasoningChoice {
        requested,
        effective,
        dropdown: dropdown_levels(available),
        warning,
    }
}

/// Aufsteigend sortiert, ohne Dubletten, ohne `ModelDefault`.
fn normalize_levels(levels: impl IntoIterator<Item = ReasoningLevel>) -> Vec<ReasoningLevel> {
    let mut out: Vec<ReasoningLevel> = levels.into_iter().filter(|l| l.rank().is_some()).collect();
    out.sort_by_key(|l| l.rank());
    out.dedup();
    out
}

fn level_from_effort(name: &str) -> Option<ReasoningLevel> {
    match name {
        "none" => Some(ReasoningLevel::Off),
        "off" | "model-default" => None,
        other => other.parse().ok(),
    }
}

/// Stufen aus Anbieterwerten (`none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`); `none` → `off`, andere
/// Werte werden ignoriert. Quelle: OpenAI-Katalog `reasoning_efforts` (AI-010).
pub fn levels_from_efforts(efforts: &[String]) -> Vec<ReasoningLevel> {
    normalize_levels(efforts.iter().filter_map(|e| level_from_effort(e)))
}

fn supported(node: &Value) -> bool {
    node.get("supported").and_then(Value::as_bool) == Some(true)
}

/// Verfügbare Stufen eines Anthropic-Modells aus seinem Eintrag der Modellliste (AI-002, Nachtrag):
/// `off` nur bei `capabilities.thinking.supported` und `thinking.types.disabled.supported`; `minimal` … `max` aus
/// `capabilities.effort` (`effort.supported` und je Stufe `supported`). Ohne `capabilities` nur `model-default`.
pub fn anthropic_levels(entry: &Value) -> Vec<ReasoningLevel> {
    let caps = &entry["capabilities"];
    if !caps.is_object() {
        return Vec::new();
    }
    let thinking = &caps["thinking"];
    let mut levels = Vec::new();
    if supported(thinking) && supported(&thinking["types"]["disabled"]) {
        levels.push(ReasoningLevel::Off);
    }
    let effort = &caps["effort"];
    if supported(effort) {
        levels.extend(
            ReasoningLevel::EXPLICIT
                .into_iter()
                .filter(|l| *l != ReasoningLevel::Off && supported(&effort[l.as_str()])),
        );
    }
    normalize_levels(levels)
}

/// Ausgabegrenze eines Anthropic-Modells (`max_tokens` der Modellliste).
pub fn anthropic_output_limit(entry: &Value) -> Option<u64> {
    entry["max_tokens"].as_u64()
}

/// Verfügbare Stufen eines OpenRouter-Modells (AI-005, Nachtrag): nur mit `"reasoning"` in `supported_parameters` und
/// `reasoning.supported_efforts`; `none` → `off`, aber nicht bei `reasoning.mandatory = true`.
pub fn openrouter_levels(entry: &Value) -> Vec<ReasoningLevel> {
    let has_param = entry["supported_parameters"]
        .as_array()
        .is_some_and(|p| p.iter().any(|v| v.as_str() == Some("reasoning")));
    let Some(efforts) = entry["reasoning"]["supported_efforts"].as_array() else {
        return Vec::new();
    };
    if !has_param {
        return Vec::new();
    }
    let mandatory = entry["reasoning"]["mandatory"].as_bool() == Some(true);
    normalize_levels(
        efforts
            .iter()
            .filter_map(Value::as_str)
            .filter_map(level_from_effort)
            .filter(|l| !(mandatory && *l == ReasoningLevel::Off)),
    )
}

/// Ausgabegrenze eines OpenRouter-Modells (`top_provider.max_completion_tokens`, falls nicht `null`).
pub fn openrouter_output_limit(entry: &Value) -> Option<u64> {
    entry["top_provider"]["max_completion_tokens"].as_u64()
}

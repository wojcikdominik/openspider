//! Unabhängige Abnahmetests M4-K10: bestätigte Standardmodelle und `max_tokens`-Regel (AI-011, Nachträge 24.11.2026 und
//! 25.11.2026), soweit die Kosten- und Reservierungsrechnung von K10 darauf aufsetzt.
//!
//! Die Modellwahl selbst (nur ein Schlüssel hinterlegt, Ersatzliste) ist Gegenstand von K04/K4-20 und hier nicht geprüft.
// acceptance: independent

use os_ai::catalog::{
    default_max_tokens, AnswerTokens, Endpoint, PriceKey, PriceTable, TaskDefaults, TaskType,
};
use os_ai::provider::ReasoningLevel;

fn mref(provider: &str, model: &str, endpoint: Endpoint) -> (String, String, Endpoint) {
    (provider.into(), model.into(), endpoint)
}

// spec: AI-011 [M4]
// acceptance: independent
// Kriterium (Nachtrag 25.11.2026): Die ausgelieferte `task-defaults.json` entspricht der Tabelle (Standard und
// Reihenfolge der Ersatzliste je Aufgabe); jedes genannte Modell hat in `pricing.json` einen Eintrag für den genannten
// Endpunkt mit `source_url` und `verified_at`, oder ist ein OpenRouter-Modell. Stufe und Antwortanteil wie im Nachtrag
// vom 24.11.2026.
#[test]
fn delivered_defaults_match_the_confirmed_table() {
    use Endpoint::*;
    let luna_decisions = mref("openai", "gpt-6-luna", Decisions);
    let luna_responses = mref("openai", "gpt-6-luna", Responses);
    let haiku = mref("anthropic", "claude-haiku-5-5", Messages);
    let or_luna = mref("openrouter", "openai/gpt-6-luna", ChatCompletions);
    let sonnet = mref("anthropic", "claude-sonnet-5-5", Messages);
    let sol = mref("openai", "gpt-6-sol", Responses);
    let table = [
        (
            TaskType::Decision,
            luna_decisions.clone(),
            vec![haiku.clone(), or_luna.clone()],
            Some(ReasoningLevel::Off),
            Some(AnswerTokens::Formula(
                os_ai::catalog::reasoning::AnswerFormula::DecisionW,
            )),
        ),
        (
            TaskType::DecisionVision,
            luna_decisions,
            vec![haiku.clone(), or_luna],
            Some(ReasoningLevel::Off),
            Some(AnswerTokens::Formula(
                os_ai::catalog::reasoning::AnswerFormula::DecisionW,
            )),
        ),
        (
            TaskType::SuggestTitle,
            luna_responses.clone(),
            vec![haiku.clone()],
            Some(ReasoningLevel::Off),
            Some(AnswerTokens::Fixed(512)),
        ),
        (
            TaskType::SuggestMeta,
            luna_responses.clone(),
            vec![haiku.clone()],
            Some(ReasoningLevel::Off),
            Some(AnswerTokens::Fixed(512)),
        ),
        (
            TaskType::SuggestH1,
            luna_responses.clone(),
            vec![haiku.clone()],
            Some(ReasoningLevel::Off),
            Some(AnswerTokens::Fixed(512)),
        ),
        (
            TaskType::LinkAnchor,
            luna_responses.clone(),
            vec![haiku.clone()],
            Some(ReasoningLevel::Off),
            Some(AnswerTokens::Fixed(512)),
        ),
        (
            TaskType::RedirectConfirm,
            luna_responses.clone(),
            vec![haiku.clone()],
            Some(ReasoningLevel::Off),
            Some(AnswerTokens::Formula(
                os_ai::catalog::reasoning::AnswerFormula::DecisionW,
            )),
        ),
        (
            TaskType::SuggestAlt,
            luna_responses.clone(),
            vec![haiku],
            Some(ReasoningLevel::Off),
            Some(AnswerTokens::Fixed(512)),
        ),
        (
            TaskType::AskSql,
            sonnet.clone(),
            vec![sol.clone(), luna_responses.clone()],
            Some(ReasoningLevel::Medium),
            Some(AnswerTokens::Fixed(1_024)),
        ),
        (
            TaskType::AuditSummary,
            sonnet.clone(),
            vec![sol.clone(), luna_responses.clone()],
            Some(ReasoningLevel::Medium),
            Some(AnswerTokens::Fixed(8_192)),
        ),
        (
            TaskType::LlmsSummary,
            sonnet,
            vec![sol, luna_responses],
            Some(ReasoningLevel::Low),
            Some(AnswerTokens::Fixed(2_048)),
        ),
        (
            TaskType::Embedding,
            mref("openai", "text-embedding-3-small", Embeddings),
            vec![],
            None,
            None,
        ),
    ];
    let defaults = TaskDefaults::bundled().expect("task-defaults.json");
    let pricing = PriceTable::bundled().expect("pricing.json");
    for (task, standard, fallback, reasoning, answer) in table {
        let d = defaults.get(task);
        let m = d
            .model
            .as_ref()
            .unwrap_or_else(|| panic!("{task:?} ohne Standardmodell"));
        assert_eq!(
            (m.provider.clone(), m.model.clone(), m.endpoint),
            standard,
            "{task:?}"
        );
        let got: Vec<_> = d
            .fallback
            .iter()
            .map(|f| (f.provider.clone(), f.model.clone(), f.endpoint))
            .collect();
        assert_eq!(got, fallback, "Ersatzliste {task:?}");
        assert_eq!(d.reasoning, reasoning, "Stufe {task:?}");
        assert_eq!(d.answer_tokens, answer, "Antwortanteil {task:?}");
        for (provider, model, endpoint) in std::iter::once(standard).chain(fallback) {
            if provider == "openrouter" {
                continue;
            }
            let entry = pricing
                .get(&PriceKey::new(&provider, &model, endpoint))
                .unwrap_or_else(|| {
                    panic!("kein Preis für {provider}/{model}/{}", endpoint.as_str())
                });
            assert!(!entry.source_url.is_empty(), "{model}");
            assert!(!entry.verified_at.is_empty(), "{model}");
        }
    }
}

// spec: AI-011 [M4]
// spec: AI-013 [M4]
// acceptance: independent
// `max_tokens`-Regel (Grundlage der Reservierung): min(2 × A + R, Ausgabegrenze) – Prüfwerte aus AI-011: `suggest-title`
// auf A (off) 1.024, auf B (low) 2.048; Ersatzweg W = 14 → 576, W = 16 → 640; `ask-sql` (medium) 6.144;
// `audit-summary` auf B (Grenze 4.000) → 4.000 statt 20.480; Reserve R je Stufe.
#[test]
fn max_tokens_rule_examples() {
    assert_eq!(default_max_tokens(512, ReasoningLevel::Off, None), 1_024);
    assert_eq!(
        default_max_tokens(512, ReasoningLevel::Low, Some(4_000)),
        2_048
    );
    assert_eq!(
        default_max_tokens(64 + 16 * 14, ReasoningLevel::Off, None),
        576
    );
    assert_eq!(
        default_max_tokens(64 + 16 * 16, ReasoningLevel::Off, None),
        640
    );
    assert_eq!(
        default_max_tokens(1_024, ReasoningLevel::Medium, None),
        6_144
    );
    assert_eq!(
        default_max_tokens(8_192, ReasoningLevel::Medium, Some(4_000)),
        4_000
    );
    assert_eq!(
        default_max_tokens(8_192, ReasoningLevel::Medium, None),
        20_480
    );
    for (level, r) in [
        (ReasoningLevel::Off, 0u64),
        (ReasoningLevel::Minimal, 512),
        (ReasoningLevel::Low, 1_024),
        (ReasoningLevel::Medium, 4_096),
        (ReasoningLevel::High, 8_192),
        (ReasoningLevel::XHigh, 16_384),
        (ReasoningLevel::Max, 32_768),
        (ReasoningLevel::ModelDefault, 4_096),
    ] {
        assert_eq!(default_max_tokens(100, level, None), 200 + r, "{level}");
    }
}

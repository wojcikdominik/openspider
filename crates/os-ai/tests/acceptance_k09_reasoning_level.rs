//! Unabhängige Abnahmetests M4-K09: Typ `ReasoningLevel` und Regel der effektiven Stufe (AI-011 Nachtrag
//! 24.11.2026, Q00-H1; Vertrag in docs/plan/M4.md Abschnitt 7, „Auftragskern K09“ Punkt 1) sowie die neuen Felder des
//! gemeinsamen Request-, Modell- und `usage`-Typs (AI-001 Nachtrag).
mod acceptance_k09_support;

use std::collections::HashSet;

use acceptance_k09_support::{expected_effective, ALL_LEVELS};
use os_ai::provider::{resolve_reasoning_level, Model, ReasoningLevel, Request, Usage};
use proptest::prelude::*;
use serde_json::json;
use ReasoningLevel::*;

const NAMES: [(ReasoningLevel, &str); 8] = [
    (ModelDefault, "model-default"),
    (Off, "off"),
    (Minimal, "minimal"),
    (Low, "low"),
    (Medium, "medium"),
    (High, "high"),
    (XHigh, "xhigh"),
    (Max, "max"),
];

// spec: AI-011 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// Stufen `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max` und `model-default`; dieselben Namen für Serde,
// Anzeige und `FromStr`; Standard `model-default`.
#[test]
fn names_are_identical_for_serde_display_and_parsing() {
    for (level, name) in NAMES {
        assert_eq!(level.as_str(), name);
        assert_eq!(level.to_string(), name);
        assert_eq!(name.parse::<ReasoningLevel>().ok(), Some(level), "{name}");
        assert_eq!(serde_json::to_value(level).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<ReasoningLevel>(json!(name)).unwrap(),
            level
        );
    }
    assert_eq!(ReasoningLevel::default(), ModelDefault);
    // Anbieterwerte und abweichende Schreibweisen sind keine Stufennamen.
    for bad in [
        "none",
        "Off",
        "OFF",
        "x-high",
        "x_high",
        "model_default",
        "default",
        "",
        " low",
    ] {
        assert!(bad.parse::<ReasoningLevel>().is_err(), "{bad:?}");
        assert!(
            serde_json::from_value::<ReasoningLevel>(json!(bad)).is_err(),
            "{bad:?}"
        );
    }
    // Copy, Eq und Hash (Vertrag: derive … Hash): alle acht Werte sind verschieden.
    let set: HashSet<ReasoningLevel> = ALL_LEVELS.iter().copied().collect();
    assert_eq!(set.len(), 8);
}

// spec: AI-011 [M4]
// acceptance: independent
// Ordnung nur über `rank()`: `model-default` steht außerhalb der Skala; `EXPLICIT` sind die sieben Stufen aufsteigend.
#[test]
fn rank_orders_the_scale_and_model_default_has_none() {
    assert_eq!(ModelDefault.rank(), None);
    assert_eq!(
        ReasoningLevel::EXPLICIT,
        [Off, Minimal, Low, Medium, High, XHigh, Max]
    );
    assert_eq!(Off.rank(), Some(0));
    let ranks: Vec<u8> = ReasoningLevel::EXPLICIT
        .iter()
        .map(|l| l.rank().expect("Stufe der Skala hat einen Rang"))
        .collect();
    assert!(
        ranks.windows(2).all(|w| w[0] < w[1]),
        "aufsteigend: {ranks:?}"
    );
}

// spec: AI-011 [M4]
// acceptance: independent
// Prüfwerte aus der Spec (AI-002, AI-003, AI-005, AI-011 Nachträge), ausgedrückt über die Regel.
#[test]
fn resolve_matches_the_spec_examples() {
    // Leere Liste und model-default.
    for requested in ALL_LEVELS {
        assert_eq!(resolve_reasoning_level(requested, &[]), ModelDefault);
    }
    assert_eq!(
        resolve_reasoning_level(ModelDefault, &[Off, Low, Medium]),
        ModelDefault
    );
    // Anthropic-Modell B (nicht abschaltbar, ohne max): off → low, max → xhigh.
    let b = [Low, Medium, High, XHigh];
    assert_eq!(resolve_reasoning_level(Off, &b), Low);
    assert_eq!(resolve_reasoning_level(Max, &b), XHigh);
    // Anthropic-Modell A: medium bleibt medium.
    let a = [Off, Low, Medium, High, XHigh, Max];
    assert_eq!(resolve_reasoning_level(Medium, &a), Medium);
    // OpenAI-Katalog none/low/medium/high: off → off, high → high, max → high.
    let openai = [Off, Low, Medium, High];
    assert_eq!(resolve_reasoning_level(Off, &openai), Off);
    assert_eq!(resolve_reasoning_level(High, &openai), High);
    assert_eq!(resolve_reasoning_level(Max, &openai), High);
    // OpenRouter M-R2 (mandatory, ohne none): off → low.
    assert_eq!(resolve_reasoning_level(Off, &[Low, Medium, High]), Low);
    // Nächstniedrigere verfügbare Stufe, nicht die nächsthöhere.
    assert_eq!(resolve_reasoning_level(Medium, &[Off, Low, High]), Low);
    assert_eq!(resolve_reasoning_level(XHigh, &[Minimal, Max]), Minimal);
    // Keine niedrigere → die niedrigste verfügbare.
    assert_eq!(resolve_reasoning_level(Low, &[Medium, High]), Medium);
    assert_eq!(resolve_reasoning_level(Minimal, &[Max]), Max);
}

// spec: AI-011 [M4]
// acceptance: independent
// Vollständig: jede gewünschte Stufe × jede Teilmenge der sieben Stufen gegen die eigene Fassung der Regel.
#[test]
fn resolve_agrees_with_the_rule_for_every_subset() {
    for requested in ALL_LEVELS {
        for mask in 0u8..128 {
            let available: Vec<ReasoningLevel> = ReasoningLevel::EXPLICIT
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, l)| *l)
                .collect();
            let expected = expected_effective(requested, &available);
            assert_eq!(
                resolve_reasoning_level(requested, &available),
                expected,
                "{requested} auf {available:?}"
            );
            // Absteigende Reihenfolge der Liste ändert nichts.
            let mut reversed = available.clone();
            reversed.reverse();
            assert_eq!(resolve_reasoning_level(requested, &reversed), expected);
        }
    }
}

proptest! {
    // spec: AI-011 [M4]
    // acceptance: independent
    // Beliebige Listen (unsortiert, mit Doppelungen, auch mit `model-default` als Eintrag, der keine Stufe ist): das
    // Ergebnis ist `model-default` oder eine verfügbare Stufe und folgt der Regel.
    #[test]
    fn resolve_on_arbitrary_lists(
        requested in proptest::sample::select(ALL_LEVELS.to_vec()),
        available in proptest::collection::vec(proptest::sample::select(ALL_LEVELS.to_vec()), 0..12),
    ) {
        let effective = resolve_reasoning_level(requested, &available);
        prop_assert_eq!(effective, expected_effective(requested, &available));
        if effective != ModelDefault {
            prop_assert!(available.contains(&effective));
        }
    }
}

// spec: AI-001 [M4]
// acceptance: independent
// Gemeinsamer Request-Typ mit `reasoning` (Standard `model-default`), Modell mit `reasoning_levels` (Standard leer),
// einheitliche `usage` mit `reasoning_tokens` (fehlt → `null`, nicht 0).
#[test]
fn new_fields_default_to_model_default_empty_and_null() {
    let usage: Usage = serde_json::from_value(json!({
        "input_tokens": 10, "output_tokens": 5, "cached_input_tokens": null, "cache_write_tokens": null
    }))
    .unwrap();
    assert_eq!(usage.reasoning_tokens, None);
    let serialized = serde_json::to_value(Usage::default()).unwrap();
    assert_eq!(serialized["reasoning_tokens"], serde_json::Value::Null);
    assert!(serialized
        .as_object()
        .unwrap()
        .contains_key("reasoning_tokens"));

    let model: Model = serde_json::from_value(json!({
        "provider": "openai", "id": "m", "endpoint": "responses",
        "capabilities": {"text": true, "structured_output": false, "vision": false, "embeddings": false,
                         "decisions": false, "batch": false, "prices_via_api": false, "local": false},
        "capability_source": "model_catalog"
    }))
    .unwrap();
    assert!(model.reasoning_levels.is_empty());

    let capabilities = model.capabilities;
    let request: Request = serde_json::from_value(json!({
        "model": model, "required": capabilities, "input": [], "instructions": "",
        "questions": [], "response_schema": null, "max_tokens": null, "temperature": null, "effort": null
    }))
    .unwrap();
    assert_eq!(request.reasoning, ModelDefault);
    let mut with_level = serde_json::to_value(&request).unwrap();
    assert_eq!(with_level["reasoning"], json!("model-default"));
    with_level["reasoning"] = json!("xhigh");
    let parsed: Request = serde_json::from_value(with_level).unwrap();
    assert_eq!(parsed.reasoning, XHigh);
}

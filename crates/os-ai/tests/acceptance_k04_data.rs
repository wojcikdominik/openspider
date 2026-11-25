//! Unabhängige Abnahmetests M4-K04: Datendateien `task-defaults.json` und `provider-limits.json` (AI-011, AI-012
//! Nachtrag Q00) und die `max_tokens`-Regel für Chat-Prompts aus 04 (INTEG-162, Nachtrag 24.11.2026).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` AI-011 (Schema im CI, Standardmodelle in `pricing.json` für
//! den Endpunkt der Aufgabe oder als OpenRouter/Ollama gekennzeichnet; Nachtrag: Tabelle Stufe/Antwortanteil,
//! Formel-Kennung `decision-w`), AI-012 (Nachtrag: `decisions_question_overhead_tokens` = 100 mit Quelle
//! „Live-Probe Q00“ und `verified_at` 2026-11-24; `schema_overhead_tokens` Anthropic 350, OpenAI Responses 80,
//! OpenRouter `openai/*` 80, sonst 0), `docs/spec/04-custom-integrations.md` INTEG-162 (Nachtrag: min(2 × 1.024 +
//! 4.096, Modellgrenze), höchstens 6.144) und `docs/plan/M4.md` (E-4 b; seit K4-20 bestätigte Standardmodelle, AI-011
//! Nachtrag 25.11.2026 – der Zustand ohne Standardmodelle wird gegen eine Fixture geprüft).
// acceptance: independent

mod acceptance_k04_support;

use std::collections::BTreeMap;

use acceptance_k04_support::*;
use os_ai::catalog::reasoning::AnswerFormula;
use os_ai::catalog::task_defaults::validate_task_defaults;
use os_ai::catalog::{
    chat_prompt_max_tokens, plan_task, AnswerTokens, Endpoint, ModelRef, PriceTable,
    ProviderLimitsFile, TaskDefaults, TaskSettings, TaskType,
};
use os_ai::provider::ReasoningLevel::{self, *};
use os_ai::AiStatus;
use serde_json::{json, Value};

/// Tabelle aus AI-011 (Nachtrag Q00): Aufgabe → (Stufe, Antwortanteil; `None` = Formel `decision-w`).
fn spec_table() -> Vec<(TaskType, ReasoningLevel, Option<u64>)> {
    vec![
        (TaskType::Decision, Off, None),
        (TaskType::DecisionVision, Off, None),
        (TaskType::RedirectConfirm, Off, None),
        (TaskType::SuggestTitle, Off, Some(512)),
        (TaskType::SuggestMeta, Off, Some(512)),
        (TaskType::SuggestH1, Off, Some(512)),
        (TaskType::SuggestAlt, Off, Some(512)),
        (TaskType::LinkAnchor, Off, Some(512)),
        (TaskType::LlmsSummary, Low, Some(2048)),
        (TaskType::AskSql, Medium, Some(1024)),
        (TaskType::AuditSummary, Medium, Some(8192)),
    ]
}

// ---------------------------------------------------------------------------------------------------------------
// task-defaults.json

// spec: AI-011
// acceptance: independent
#[test]
fn task_types_are_exactly_the_twelve_of_the_spec() {
    let names: Vec<_> = TaskType::ALL.iter().map(|t| t.as_str()).collect();
    let mut expected = vec![
        "decision",
        "decision-vision",
        "suggest-title",
        "suggest-meta",
        "suggest-h1",
        "suggest-alt",
        "redirect-confirm",
        "link-anchor",
        "ask-sql",
        "audit-summary",
        "llms-summary",
        "embedding",
    ];
    let mut got = names.clone();
    got.sort();
    expected.sort();
    assert_eq!(got, expected);
    let doc = resource_json("task-defaults.json");
    let mut keys: Vec<_> = doc["tasks"].as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, expected);
}

// spec: AI-011
// acceptance: independent
#[test]
fn bundled_task_defaults_validate_and_match_the_table() {
    let doc = resource_json("task-defaults.json");
    validate_task_defaults(&doc).expect("task-defaults.json gültig");
    let defaults = TaskDefaults::bundled().expect("lädt");
    for (task, level, answer) in spec_table() {
        assert_eq!(defaults.get(task).reasoning, Some(level), "{task}");
        let want = match answer {
            Some(n) => AnswerTokens::Fixed(n),
            None => AnswerTokens::Formula(AnswerFormula::DecisionW),
        };
        assert_eq!(defaults.answer_tokens(task), Some(want), "{task}");
        // Rohwerte der Datei
        let raw = &doc["tasks"][task.as_str()];
        assert_eq!(raw["reasoning"], json!(level.as_str()), "{task}");
        match answer {
            Some(n) => assert_eq!(raw["answer_tokens"], json!(n), "{task}"),
            None => assert_eq!(raw["answer_tokens"], json!("decision-w"), "{task}"),
        }
    }
    // embedding: keine Stufe, kein Antwortanteil
    assert_eq!(defaults.get(TaskType::Embedding).answer_tokens, None);
    assert_eq!(doc["tasks"]["embedding"].get("answer_tokens"), None);
}

// spec: AI-011
// acceptance: independent
#[test]
fn bundled_task_defaults_validate_against_schema_file_in_resources() {
    let schema = resource_json("schemas/task-defaults.schema.json");
    let validator = jsonschema::validator_for(&schema).expect("Schema kompiliert");
    let doc = resource_json("task-defaults.json");
    let errors: Vec<String> = match validator.validate(&doc) {
        Ok(()) => Vec::new(),
        Err(errs) => errs.map(|e| e.to_string()).collect(),
    };
    assert!(errors.is_empty(), "{errors:?}");
    let filled = fixture_json("task-defaults-filled.json");
    assert!(validator.is_valid(&filled));
}

fn mutated(path: &[&str], value: Option<Value>) -> Value {
    let mut doc = resource_json("task-defaults.json");
    let mut node = &mut doc;
    for p in &path[..path.len() - 1] {
        node = &mut node[*p];
    }
    let last = path[path.len() - 1];
    match value {
        Some(v) => node[last] = v,
        None => {
            node.as_object_mut().unwrap().remove(last);
        }
    }
    doc
}

fn task_defaults_rejected(doc: &Value) -> bool {
    validate_task_defaults(doc).is_err() && TaskDefaults::from_json_str(&doc.to_string()).is_err()
}

// spec: AI-011
// acceptance: independent
#[test]
fn task_defaults_schema_negative_cases() {
    // jede Aufgabe außer embedding braucht reasoning aus der Stufenmenge und answer_tokens > 0 bzw. decision-w
    for task in [
        "suggest-title",
        "ask-sql",
        "decision",
        "redirect-confirm",
        "llms-summary",
    ] {
        assert!(
            task_defaults_rejected(&mutated(&["tasks", task, "reasoning"], None)),
            "{task} ohne reasoning"
        );
        assert!(
            task_defaults_rejected(&mutated(&["tasks", task, "answer_tokens"], None)),
            "{task} ohne answer_tokens"
        );
        assert!(
            task_defaults_rejected(&mutated(
                &["tasks", task, "reasoning"],
                Some(json!("turbo"))
            )),
            "{task} mit unbekannter Stufe"
        );
    }
    for bad in [json!(0), json!(-1), json!("512"), json!(1.5)] {
        assert!(
            task_defaults_rejected(&mutated(
                &["tasks", "suggest-title", "answer_tokens"],
                Some(bad.clone())
            )),
            "answer_tokens {bad}"
        );
    }
    // Entscheidungsaufgaben: Formel-Kennung decision-w
    for task in ["decision", "decision-vision", "redirect-confirm"] {
        assert!(
            task_defaults_rejected(&mutated(
                &["tasks", task, "answer_tokens"],
                Some(json!(128))
            )),
            "{task} mit fester Zahl"
        );
        assert!(
            task_defaults_rejected(&mutated(
                &["tasks", task, "answer_tokens"],
                Some(json!("decision-x"))
            )),
            "{task} mit unbekannter Formel"
        );
    }
    // fehlende Aufgabe, unbekannte Aufgabe
    assert!(task_defaults_rejected(&mutated(
        &["tasks", "link-anchor"],
        None
    )));
    assert!(task_defaults_rejected(&mutated(
        &["tasks", "suggest-everything"],
        Some(json!({ "reasoning": "off", "answer_tokens": 1, "model": null, "fallback": [] }))
    )));
    // Modellverweis mit unbekanntem Endpunkt
    assert!(task_defaults_rejected(&mutated(
        &["tasks", "suggest-title", "model"],
        Some(json!({ "provider": "openai", "model": "gpt-6-luna", "endpoint": "chat" }))
    )));
    assert!(TaskDefaults::from_json_str("{").is_err());
}

// spec: AI-011
// acceptance: independent
#[test]
fn default_models_must_exist_in_pricing_for_the_task_endpoint() {
    let pricing = acceptance_pricing();
    // gefüllte Testliste: alle Modelle vorhanden bzw. OpenRouter-gekennzeichnet
    assert_eq!(
        filled_defaults().check_against_pricing(&pricing),
        Vec::<String>::new()
    );
    // gpt-6-luna für suggest-title über chat-completions: in pricing nur responses/decisions → Befund
    let mut doc = fixture_json("task-defaults-filled.json");
    doc["tasks"]["suggest-title"]["model"] =
        json!({ "provider": "openai", "model": "gpt-6-luna", "endpoint": "chat-completions" });
    let d = TaskDefaults::from_json_str(&doc.to_string()).unwrap();
    let errors = d.check_against_pricing(&pricing);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("suggest-title"), "{errors:?}");
    // ein unbekanntes Ersatzmodell wird ebenfalls gemeldet
    let mut doc = fixture_json("task-defaults-filled.json");
    doc["tasks"]["ask-sql"]["fallback"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "provider": "anthropic", "model": "acc-unpriced", "endpoint": "messages" }));
    let d = TaskDefaults::from_json_str(&doc.to_string()).unwrap();
    assert_eq!(d.check_against_pricing(&pricing).len(), 1);
    // OpenRouter und Ollama sind gekennzeichnet und brauchen keinen Tabelleneintrag
    let mut doc = fixture_json("task-defaults-filled.json");
    doc["tasks"]["llms-summary"]["fallback"] = json!([
        { "provider": "openrouter", "model": "acc/not-in-table", "endpoint": "chat-completions" },
        { "provider": "ollama", "model": "acc-local", "endpoint": "chat-completions" }
    ]);
    let d = TaskDefaults::from_json_str(&doc.to_string()).unwrap();
    assert_eq!(d.check_against_pricing(&pricing), Vec::<String>::new());
}

// spec: AI-011
// acceptance: independent
#[test]
fn bundled_task_defaults_are_consistent_with_bundled_pricing() {
    let defaults = TaskDefaults::bundled().unwrap();
    let pricing = PriceTable::bundled().unwrap();
    assert_eq!(
        defaults.check_against_pricing(&pricing),
        Vec::<String>::new()
    );
}

fn filled_defaults() -> TaskDefaults {
    TaskDefaults::from_json_str(&fixture_text("task-defaults-filled.json")).unwrap()
}

// ---------------------------------------------------------------------------------------------------------------
// Datei ohne Standardmodelle (E-4 b). Seit K4-20 (AI-011, Nachtrag 25.11.2026) ist die ausgelieferte Datei bestätigt;
// das Kriterium „Aufgabe ohne Kandidaten“ gilt für eine Datei ohne Standardmodelle (Fixture).

/// `task-defaults.json` ohne Standardmodelle (Stand vor K4-20).
fn placeholder_defaults_text() -> String {
    let path = repo_root().join("tests/fixtures/ai/k04/task-defaults-placeholder.json");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

// spec: AI-011
// acceptance: independent
#[test]
fn placeholder_defaults_have_no_models_until_confirmed() {
    // Datei ohne bestätigte Standardmodelle: keine Modelle, leere Ersatzlisten; Stufen und Antwortanteile gesetzt.
    let text = placeholder_defaults_text();
    let doc: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["models_status"], "pending-confirmation");
    let defaults = TaskDefaults::from_json_str(&text).unwrap();
    for task in TaskType::ALL {
        assert_eq!(defaults.get(task).model, None, "{task}");
        assert!(defaults.get(task).fallback.is_empty(), "{task}");
    }
}

// spec: AI-011
// acceptance: independent
#[test]
fn bundled_placeholder_with_user_chosen_model_works() {
    // Bis zur Bestätigung funktioniert eine Aufgabe, wenn der Nutzer selbst ein Modell wählt; Stufe und
    // Ausgabegrenze kommen aus den ausgelieferten Standardwerten.
    let defaults = TaskDefaults::bundled().unwrap();
    let settings = TaskSettings {
        model: Some(ModelRef::new(
            "anthropic",
            "mock-claude-b",
            Endpoint::Messages,
        )),
        ..TaskSettings::default()
    };
    let p = plan_task(
        TaskType::SuggestTitle,
        &settings,
        &defaults,
        &providers(&["anthropic"]),
    )
    .unwrap();
    assert_eq!(p.reasoning.effective, Low);
    assert_eq!(p.output_limit(None).unwrap().value, 2048);
}

// spec: AI-011
// acceptance: independent
#[test]
fn bundled_placeholder_without_model_and_all_keys_is_not_missing_api_key() {
    // AI-011: `Error: Missing API key` nur bei fehlendem Schlüssel, `Skipped (no capable model)` bei vorhandenem
    // Schlüssel ohne geeignetes Modell. Mit der ausgelieferten Datei (keine Standardmodelle) und Schlüsseln für alle
    // Anbieter fehlt kein Schlüssel. Seit K4-20 gegen die Datei ohne Standardmodelle (Fixture), AI-011 Nachtrag 25.11.2026.
    let defaults = TaskDefaults::from_json_str(&placeholder_defaults_text()).unwrap();
    let all = providers(&["anthropic", "openai", "openrouter"]);
    let mut observed = BTreeMap::new();
    for task in TaskType::ALL {
        let status = plan_task(task, &TaskSettings::default(), &defaults, &all).unwrap_err();
        observed.insert(task.as_str(), status);
    }
    for (task, status) in &observed {
        assert_ne!(
            *status,
            AiStatus::MissingApiKey,
            "{task}: alle Schlüssel vorhanden, trotzdem `Error: Missing API key` (beobachtet: {observed:?})"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// provider-limits.json

// spec: AI-012 [M4]
// acceptance: independent
#[test]
fn bundled_provider_limits_carry_q00_values() {
    let limits = ProviderLimitsFile::bundled().expect("provider-limits.json lädt");
    assert_eq!(limits.decisions_question_overhead(), 100);
    let g = &limits.decisions_question_overhead_tokens;
    assert!(g.source_url.contains("Live-Probe Q00"), "{}", g.source_url);
    assert_eq!(g.verified_at, "2026-11-24");
    let s = &limits.schema_overhead_tokens;
    assert!(!s.source_url.trim().is_empty());
    assert_eq!(s.verified_at, "2026-11-24");
    // Schema-Zuschlag je Anbieter
    assert_eq!(
        limits.schema_overhead("anthropic", Endpoint::Messages, "mock-claude"),
        350
    );
    assert_eq!(
        limits.schema_overhead("openai", Endpoint::Responses, "gpt-6-luna"),
        80
    );
    assert_eq!(
        limits.schema_overhead("openrouter", Endpoint::ChatCompletions, "openai/gpt-6-luna"),
        80
    );
    // sonst 0
    assert_eq!(
        limits.schema_overhead(
            "openrouter",
            Endpoint::ChatCompletions,
            "anthropic/claude-x"
        ),
        0
    );
    assert_eq!(
        limits.schema_overhead("openrouter", Endpoint::ChatCompletions, "acc/openai-like"),
        0
    );
    assert_eq!(
        limits.schema_overhead("ollama", Endpoint::ChatCompletions, "llama"),
        0
    );
}

// spec: AI-012 [M4]
// acceptance: independent
#[test]
fn provider_limits_validate_against_schema_file_and_reject_bad_values() {
    let schema = resource_json("schemas/provider-limits.schema.json");
    let validator = jsonschema::validator_for(&schema).expect("Schema kompiliert");
    let doc = resource_json("provider-limits.json");
    let errors: Vec<String> = match validator.validate(&doc) {
        Ok(()) => Vec::new(),
        Err(errs) => errs.map(|e| e.to_string()).collect(),
    };
    assert!(errors.is_empty(), "{errors:?}");
    // jeder Wert mit source_url und verified_at
    for field in ["source_url", "verified_at"] {
        let mut bad = doc.clone();
        bad["decisions_question_overhead_tokens"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            ProviderLimitsFile::from_json_str(&bad.to_string()).is_err(),
            "ohne {field}"
        );
        let mut bad = doc.clone();
        bad["schema_overhead_tokens"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            ProviderLimitsFile::from_json_str(&bad.to_string()).is_err(),
            "ohne {field}"
        );
    }
    let mut bad = doc.clone();
    bad["decisions_question_overhead_tokens"]["value"] = json!(-1);
    assert!(ProviderLimitsFile::from_json_str(&bad.to_string()).is_err());
    let mut bad = doc.clone();
    bad["decisions_question_overhead_tokens"]["verified_at"] = json!("2026-13-01");
    assert!(ProviderLimitsFile::from_json_str(&bad.to_string()).is_err());
}

// ---------------------------------------------------------------------------------------------------------------
// INTEG-162 (Nachtrag): max_tokens für Chat-Prompts

// spec: INTEG-162 [M4]
// acceptance: independent
#[test]
fn chat_prompt_max_tokens_rule() {
    // min(2 × 1.024 + 4.096, Ausgabegrenze des Modells), höchstens 6.144
    assert_eq!(chat_prompt_max_tokens(None), 6144);
    let a = anthropic_model("mock-claude");
    assert_eq!(a.max_output_tokens, Some(8192));
    assert_eq!(chat_prompt_max_tokens(a.max_output_tokens), 6144);
    assert_eq!(chat_prompt_max_tokens(Some(6144)), 6144);
    assert_eq!(chat_prompt_max_tokens(Some(4000)), 4000);
    let b = anthropic_model("mock-claude-b");
    assert_eq!(chat_prompt_max_tokens(b.max_output_tokens), 4000);
    let or = openrouter_model("acc/m-4000");
    assert_eq!(chat_prompt_max_tokens(or.max_output_tokens), 4000);
    let table = acceptance_pricing();
    let small = openai_model(&table, "acc-small-limit", Endpoint::Responses);
    assert_eq!(chat_prompt_max_tokens(small.max_output_tokens), 3000);
}

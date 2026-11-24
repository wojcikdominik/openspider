//! Unabhängige Abnahmetests M4-K04: Modellwahl je Aufgabe mit Ersatzliste (AI-011) samt Nachtrag
//! „24.11.2026, Q00“ (Reasoning-Stufe und `max_tokens` je Aufgabe).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` AI-011 (Ersatzliste bei fehlendem Anbieter; nur
//! Anthropic-Schlüssel → erstes Anthropic-Modell der Ersatzliste, Ersatzweg; fehlende Fähigkeit nicht zuweisbar;
//! `Error: Missing API key` bzw. `Skipped (no capable model)`; Profil-Roundtrip; Nachtrag: Stufen `off` … `max`
//! und `model-default`, effektive Stufe = nächstniedrigere bzw. niedrigste verfügbare, `max_tokens` =
//! min(2 × A + R, Modellgrenze) mit den Prüfwerten 1024/2048/576/640/6144/4000, Decisions ohne Stufe),
//! AI-002/AI-005 (Quelle der Stufen je Anbieter, Mock-Modelle A–D, M-R1 … M-R3) und AI-010 (Katalog).
//! Gefüllte Standardliste nur als Testfixture (`task-defaults-filled.json`), weil die ausgelieferten
//! Standardmodelle noch nicht bestätigt sind (E-4 b, K4-20).
// acceptance: independent

mod acceptance_k04_support;

use std::collections::BTreeMap;

use acceptance_k04_support::*;
use os_ai::catalog::choice::{availability, settings_from_profile, settings_to_profile};
use os_ai::catalog::reasoning::{request_weight, ReasoningWarning};
use os_ai::catalog::{
    choose_reasoning, plan_task, validate_assignment, Endpoint, ModelAvailability, ModelRef,
    ProviderState, TaskDefaults, TaskSettings, TaskType,
};
use os_ai::provider::{ReasoningLevel, Request};
use os_ai::AiStatus;
use serde_json::{json, Value};

use ReasoningLevel::*;

fn filled() -> TaskDefaults {
    TaskDefaults::from_json_str(&fixture_text("task-defaults-filled.json"))
        .expect("gefüllte Testliste gültig")
}

fn plan(
    task: TaskType,
    settings: &TaskSettings,
    keys: &[&str],
) -> Result<os_ai::catalog::TaskPlan, AiStatus> {
    plan_task(task, settings, &filled(), &providers(keys))
}

fn on_model(_task: TaskType, provider: &str, model: &str, endpoint: Endpoint) -> TaskSettings {
    TaskSettings {
        model: Some(ModelRef::new(provider, model, endpoint)),
        ..TaskSettings::default()
    }
}

fn claude(model: &str) -> TaskSettings {
    TaskSettings {
        model: Some(ModelRef::new("anthropic", model, Endpoint::Messages)),
        ..TaskSettings::default()
    }
}

fn choice_question(n: usize) -> Value {
    let choices: Vec<Value> = (0..n)
        .map(|i| json!({ "value": format!("v{i}"), "description": format!("Wert {i}") }))
        .collect();
    json!({ "name": "page_type", "type": "choice", "instructions": "Seitentyp?", "choices": choices })
}

fn predicate() -> Value {
    json!({ "name": "is_product", "type": "predicate", "instructions": "Produktseite?" })
}

// ---------------------------------------------------------------------------------------------------------------
// Standard und Ersatzliste

// spec: AI-011
// acceptance: independent
#[test]
fn default_model_is_used_when_its_provider_is_connected() {
    let p = plan(
        TaskType::Decision,
        &TaskSettings::default(),
        &["openai", "anthropic"],
    )
    .unwrap();
    assert_eq!(
        p.model_ref,
        ModelRef::new("openai", "gpt-6-luna", Endpoint::Decisions)
    );
    assert_eq!(p.candidate_index, 0);
    assert!(p.uses_decisions_api());
    let p = plan(
        TaskType::SuggestTitle,
        &TaskSettings::default(),
        &["openai"],
    )
    .unwrap();
    assert_eq!(
        p.model_ref,
        ModelRef::new("openai", "gpt-6-luna", Endpoint::Responses)
    );
}

// spec: AI-011
// acceptance: independent
#[test]
fn only_anthropic_key_uses_first_anthropic_model_of_fallback() {
    let p = plan(TaskType::Decision, &TaskSettings::default(), &["anthropic"]).unwrap();
    assert_eq!(p.model_ref.provider, "anthropic");
    assert_eq!(p.model_ref.model, "mock-claude", "erstes Anthropic-Modell");
    assert_eq!(p.candidate_index, 1);
    assert!(!p.uses_decisions_api(), "Ersatzweg (self-reported, AI-034)");
    // tatsächlich verwendetes Modell für die Spalte `<Q> Model`
    assert_eq!(p.model.id, "mock-claude");
}

// spec: AI-011
// acceptance: independent
#[test]
fn only_openrouter_key_uses_openrouter_fallback() {
    let p = plan(
        TaskType::Decision,
        &TaskSettings::default(),
        &["openrouter"],
    )
    .unwrap();
    assert_eq!(
        p.model_ref,
        ModelRef::new("openrouter", "acc/m-r1", Endpoint::ChatCompletions)
    );
    assert_eq!(p.candidate_index, 3);
}

// spec: AI-011
// acceptance: independent
#[test]
fn no_key_at_all_gives_missing_api_key() {
    for task in TaskType::ALL {
        assert_eq!(
            plan(task, &TaskSettings::default(), &[]).unwrap_err(),
            AiStatus::MissingApiKey,
            "{task}"
        );
    }
    assert_eq!(
        AiStatus::MissingApiKey.to_string(),
        "Error: Missing API key"
    );
}

// spec: AI-011
// acceptance: independent
#[test]
fn key_present_but_no_capable_model_gives_skipped() {
    // suggest-alt: Ersatzliste C (ohne Vision/Structured Output), dann A. Nur C und D in der Liste → kein geeignetes
    let mut providers = providers(&[]);
    providers.insert(
        "anthropic".into(),
        ProviderState {
            has_key: true,
            models: vec![
                anthropic_model("mock-claude-c"),
                anthropic_model("mock-claude-d"),
            ],
        },
    );
    let err = plan_task(
        TaskType::SuggestAlt,
        &TaskSettings::default(),
        &filled(),
        &providers,
    )
    .unwrap_err();
    assert_eq!(err, AiStatus::SkippedNoCapableModel);
    assert_eq!(err.to_string(), "Skipped (no capable model)");
    // Nicht-Vision-Modell als gewähltes Modell mit leerer Ersatzliste
    let settings = TaskSettings {
        model: Some(ModelRef::new(
            "openai",
            "acc-no-effort",
            Endpoint::Responses,
        )),
        ..TaskSettings::default()
    };
    assert_eq!(
        plan(TaskType::SuggestAlt, &settings, &["openai"]).unwrap_err(),
        AiStatus::SkippedNoCapableModel
    );
}

// spec: AI-011
// acceptance: independent
#[test]
fn model_missing_from_live_list_falls_back_and_is_reported() {
    // Anbieter verbunden, Modell fehlt in der Live-Liste („nicht mehr verfügbar“, AI-010/INTEG-160)
    let mut providers = providers(&["anthropic"]);
    providers
        .get_mut("anthropic")
        .unwrap()
        .models
        .retain(|m| m.model.id != "mock-claude");
    let gone = ModelRef::new("anthropic", "mock-claude", Endpoint::Messages);
    assert_eq!(
        availability(&gone, &providers),
        ModelAvailability::NoLongerAvailable
    );
    let p = plan_task(
        TaskType::SuggestTitle,
        &TaskSettings::default(),
        &filled(),
        &providers,
    )
    .unwrap();
    assert_eq!(
        p.model_ref.model, "mock-claude-b",
        "nächstes Modell der Ersatzliste"
    );
    let luna = ModelRef::new("openai", "gpt-6-luna", Endpoint::Responses);
    assert_eq!(
        availability(&luna, &providers),
        ModelAvailability::ProviderNotConnected
    );
    assert_eq!(
        availability(
            &ModelRef::new("anthropic", "mock-claude-b", Endpoint::Messages),
            &providers
        ),
        ModelAvailability::Available
    );
}

// spec: AI-011
// acceptance: independent
#[test]
fn user_choice_overrides_default_and_its_fallback() {
    let settings = TaskSettings {
        model: Some(ModelRef::new(
            "anthropic",
            "mock-claude-b",
            Endpoint::Messages,
        )),
        fallback: vec![ModelRef::new(
            "openrouter",
            "acc/m-r1",
            Endpoint::ChatCompletions,
        )],
        ..TaskSettings::default()
    };
    let p = plan(
        TaskType::SuggestMeta,
        &settings,
        &["openai", "anthropic", "openrouter"],
    )
    .unwrap();
    assert_eq!(p.model_ref.model, "mock-claude-b");
    let p = plan(TaskType::SuggestMeta, &settings, &["openai", "openrouter"]).unwrap();
    assert_eq!(
        p.model_ref.model, "acc/m-r1",
        "eigene Ersatzliste, nicht die Standardliste"
    );
    // eigene Ersatzliste ohne verfügbares Modell und ohne Schlüssel der Kandidaten → Missing API key
    assert_eq!(
        plan(TaskType::SuggestMeta, &settings, &["openai"]).unwrap_err(),
        AiStatus::MissingApiKey
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Fähigkeitsprüfung

// spec: AI-011
// acceptance: independent
#[test]
fn model_without_required_capability_cannot_be_assigned() {
    let table = acceptance_pricing();
    let a = anthropic_model("mock-claude").model;
    let c = anthropic_model("mock-claude-c").model;
    let d = anthropic_model("mock-claude-d").model;
    let no_vision = openai_model(&table, "acc-no-effort", Endpoint::Responses).model;
    let decisions = openai_model(&table, "gpt-6-luna", Endpoint::Decisions).model;
    // Bildaufgabe braucht Vision
    assert!(validate_assignment(TaskType::SuggestAlt, &a).is_ok());
    let err = validate_assignment(TaskType::SuggestAlt, &no_vision).unwrap_err();
    assert!(err.missing.contains(&"vision"), "{err:?}");
    assert!(validate_assignment(TaskType::DecisionVision, &no_vision).is_err());
    // Schema-Aufgaben brauchen strukturierte Ausgabe (C: structured_outputs false, D: Fähigkeiten unbekannt)
    for task in [TaskType::SuggestTitle, TaskType::Decision, TaskType::AskSql] {
        assert!(validate_assignment(task, &a).is_ok(), "{task}");
        let err = validate_assignment(task, &c).unwrap_err();
        assert!(
            err.missing.contains(&"structured_output"),
            "{task}: {err:?}"
        );
        assert!(validate_assignment(task, &d).is_err(), "{task}");
    }
    // Decisions-Modell nur für Entscheidungsaufgaben
    assert!(validate_assignment(TaskType::Decision, &decisions).is_ok());
    assert!(validate_assignment(TaskType::DecisionVision, &decisions).is_ok());
    assert!(validate_assignment(TaskType::SuggestTitle, &decisions).is_err());
    // Embedding braucht ein Embedding-Modell
    assert!(validate_assignment(TaskType::Embedding, &a).is_err());
    let embed = openai_model(&table, "acc-embed", Endpoint::Embeddings).model;
    assert!(validate_assignment(TaskType::Embedding, &embed).is_ok());
}

// spec: AI-011
// acceptance: independent
#[test]
fn plan_skips_incapable_candidates_in_order() {
    // suggest-alt mit nur Anthropic: Ersatzliste C (ungeeignet) → A
    let p = plan(
        TaskType::SuggestAlt,
        &TaskSettings::default(),
        &["anthropic"],
    )
    .unwrap();
    assert_eq!(p.model_ref.model, "mock-claude");
    assert_eq!(p.candidate_index, 2);
}

// ---------------------------------------------------------------------------------------------------------------
// Reasoning-Stufe und max_tokens (Prüfwerte des Nachtrags)

// spec: AI-011
// acceptance: independent
#[test]
fn suggest_title_on_model_a_and_b() {
    let a = plan(
        TaskType::SuggestTitle,
        &claude("mock-claude"),
        &["anthropic"],
    )
    .unwrap();
    assert_eq!(a.reasoning.requested, Off);
    assert_eq!(a.reasoning.effective, Off);
    assert_eq!(a.reasoning.warning, None);
    assert_eq!(a.output_limit(None).unwrap().value, 1024);
    let mut req = request_template(&a);
    a.apply_to(&mut req, None);
    assert_eq!(req.max_tokens, Some(1024));
    assert_eq!(req.reasoning, Off);

    let b = plan(
        TaskType::SuggestTitle,
        &claude("mock-claude-b"),
        &["anthropic"],
    )
    .unwrap();
    assert_eq!(b.reasoning.effective, Low);
    assert_eq!(b.output_limit(None).unwrap().value, 2048);
    assert!(
        matches!(
            b.reasoning.warning,
            Some(ReasoningWarning::OffUnavailable { effective: Low })
        ),
        "Warnhinweis im Aufgabendialog: {:?}",
        b.reasoning.warning
    );
}

fn request_template(plan: &os_ai::catalog::TaskPlan) -> Request {
    Request {
        model: plan.model.clone(),
        required: Default::default(),
        input: Vec::new(),
        instructions: String::new(),
        questions: Vec::new(),
        response_schema: None,
        max_tokens: Some(4096),
        temperature: None,
        effort: None,
        reasoning: ReasoningLevel::ModelDefault,
    }
}

// spec: AI-011
// acceptance: independent
#[test]
fn decision_fallback_answer_tokens_from_question_weight() {
    let p = plan(TaskType::Decision, &TaskSettings::default(), &["anthropic"]).unwrap();
    assert_eq!(p.model_ref.model, "mock-claude");
    assert_eq!(p.reasoning.effective, Off);
    // choice mit 14 Werten: W = 14, A = 288, max_tokens = 576
    let one = [choice_question(14)];
    assert_eq!(request_weight(&one), 14);
    assert_eq!(p.answer_tokens.unwrap().tokens(Some(14)), 288);
    assert_eq!(
        p.output_limit(Some(request_weight(&one))).unwrap().value,
        576
    );
    // gebündelt mit predicate: W = 16, max_tokens = 2 × (64 + 256) = 640
    let two = [choice_question(14), predicate()];
    assert_eq!(request_weight(&two), 16);
    assert_eq!(
        p.output_limit(Some(request_weight(&two))).unwrap().value,
        640
    );
    // score: Anzahl Levels
    let score = json!({ "name": "q", "type": "score", "instructions": "?",
        "levels": [{"label":"a","description":"a"},{"label":"b","description":"b"},{"label":"c","description":"c"}] });
    assert_eq!(request_weight(&[score]), 3);
    // redirect-confirm folgt derselben Formel
    let r = plan(
        TaskType::RedirectConfirm,
        &TaskSettings::default(),
        &["anthropic"],
    )
    .unwrap();
    assert_eq!(r.output_limit(Some(14)).unwrap().value, 576);
}

// spec: AI-011
// acceptance: independent
#[test]
fn ask_sql_on_openai_catalog_model() {
    let p = plan(TaskType::AskSql, &TaskSettings::default(), &["openai"]).unwrap();
    assert_eq!(p.model_ref.model, "gpt-6-luna");
    assert_eq!(p.reasoning.effective, Medium);
    // 2 × 1.024 + 4.096
    assert_eq!(p.output_limit(None).unwrap().value, 6144);
    let mut req = request_template(&p);
    p.apply_to(&mut req, None);
    assert_eq!(req.reasoning, Medium);
    assert_eq!(req.max_tokens, Some(6144));
}

// spec: AI-011
// acceptance: independent
#[test]
fn audit_summary_on_model_b_is_capped_at_model_limit() {
    let p = plan(
        TaskType::AuditSummary,
        &claude("mock-claude-b"),
        &["anthropic"],
    )
    .unwrap();
    assert_eq!(p.reasoning.effective, Medium);
    assert_eq!(p.model_output_limit, Some(4000));
    assert_eq!(
        p.output_limit(None).unwrap().value,
        4000,
        "Kappung statt 20.480"
    );
    // ohne Kappung auf A (Grenze 8.192): 20.480 > 8.192 → 8.192
    let a = plan(
        TaskType::AuditSummary,
        &claude("mock-claude"),
        &["anthropic"],
    )
    .unwrap();
    assert_eq!(a.output_limit(None).unwrap().value, 8192);
    // OpenRouter top_provider.max_completion_tokens = 4000
    let s = on_model(
        TaskType::AuditSummary,
        "openrouter",
        "acc/m-4000",
        Endpoint::ChatCompletions,
    );
    let o = plan(TaskType::AuditSummary, &s, &["openrouter"]).unwrap();
    assert_eq!(o.output_limit(None).unwrap().value, 4000);
    // OpenRouter ohne Grenze (null) → keine Kappung: 2 × 8.192 + 4.096
    let s = on_model(
        TaskType::AuditSummary,
        "openrouter",
        "acc/m-r1",
        Endpoint::ChatCompletions,
    );
    let o = plan(TaskType::AuditSummary, &s, &["openrouter"]).unwrap();
    assert_eq!(o.model_output_limit, None);
    assert_eq!(o.output_limit(None).unwrap().value, 20_480);
    // OpenAI-Katalog max_output_tokens = 3000
    let s = on_model(
        TaskType::AskSql,
        "openai",
        "acc-small-limit",
        Endpoint::Responses,
    );
    let o = plan(TaskType::AskSql, &s, &["openai"]).unwrap();
    assert_eq!(o.output_limit(None).unwrap().value, 3000);
}

// spec: AI-011
// acceptance: independent
#[test]
fn reserve_per_effective_level_matches_table() {
    // R: off 0, minimal 512, low 1.024, medium 4.096, high 8.192, xhigh 16.384, max 32.768, model-default 4.096
    // auf einem Modell ohne Grenze mit allen Stufen (OpenRouter M-R1 hat none/low/medium/high).
    let expected = [
        (Off, Off, 0u64),
        (Low, Low, 1_024),
        (Medium, Medium, 4_096),
        (High, High, 8_192),
        (ModelDefault, ModelDefault, 4_096),
        // nicht verfügbare Stufen → nächstniedrigere verfügbare
        (Minimal, Off, 0),
        (XHigh, High, 8_192),
        (Max, High, 8_192),
    ];
    for (requested, effective, reserve) in expected {
        let s = TaskSettings {
            model: Some(ModelRef::new(
                "openrouter",
                "acc/m-r1",
                Endpoint::ChatCompletions,
            )),
            reasoning: Some(requested),
            ..TaskSettings::default()
        };
        let p = plan(TaskType::LlmsSummary, &s, &["openrouter"]).unwrap();
        assert_eq!(p.reasoning.effective, effective, "{requested:?}");
        assert_eq!(
            p.output_limit(None).unwrap().value,
            2 * 2048 + reserve,
            "{requested:?}"
        );
    }
    // minimal (512), xhigh (16.384), max (32.768) über ein Modell mit diesen Stufen ohne Grenze
    for (level, reserve) in [(Minimal, 512u64), (XHigh, 16_384), (Max, 32_768)] {
        let c = choose_reasoning(level, &[Off, Minimal, Low, Medium, High, XHigh, Max]);
        assert_eq!(c.effective, level);
        assert_eq!(
            os_ai::catalog::default_max_tokens(100, c.effective, None),
            200 + reserve
        );
        assert_eq!(os_ai::catalog::reasoning_reserve(level), reserve);
    }
}

// spec: AI-011
// acceptance: independent
#[test]
fn effective_level_fallback_rules() {
    // gewählte Stufe verfügbar → sie selbst
    assert_eq!(
        choose_reasoning(Medium, &[Low, Medium, High]).effective,
        Medium
    );
    // nicht verfügbar → nächstniedrigere verfügbare
    assert_eq!(choose_reasoning(Medium, &[Off, Low, High]).effective, Low);
    assert_eq!(
        choose_reasoning(Max, &[Low, Medium, High, XHigh]).effective,
        XHigh
    );
    // keine niedrigere → niedrigste verfügbare
    assert_eq!(choose_reasoning(Minimal, &[Low, High]).effective, Low);
    assert_eq!(choose_reasoning(Off, &[Low, Medium]).effective, Low);
    // Modell ohne Stufen → nur model-default
    assert_eq!(choose_reasoning(High, &[]).effective, ModelDefault);
    assert_eq!(choose_reasoning(Off, &[]).effective, ModelDefault);
    // model-default bleibt model-default
    assert_eq!(
        choose_reasoning(ModelDefault, &[Off, Low]).effective,
        ModelDefault
    );
}

// spec: AI-011
// acceptance: independent
#[test]
fn default_levels_per_task_type_on_capable_model() {
    // Standardstufe je Aufgabentyp (Tabelle im Nachtrag) auf Modell A (off, low … max verfügbar)
    let expected = [
        (TaskType::Decision, Off),
        (TaskType::DecisionVision, Off),
        (TaskType::RedirectConfirm, Off),
        (TaskType::SuggestTitle, Off),
        (TaskType::SuggestMeta, Off),
        (TaskType::SuggestH1, Off),
        (TaskType::SuggestAlt, Off),
        (TaskType::LinkAnchor, Off),
        (TaskType::LlmsSummary, Low),
        (TaskType::AskSql, Medium),
        (TaskType::AuditSummary, Medium),
    ];
    for (task, level) in expected {
        let p = plan(task, &claude("mock-claude"), &["anthropic"]).unwrap();
        assert_eq!(p.reasoning.requested, level, "{task}");
        assert_eq!(p.reasoning.effective, level, "{task}");
    }
}

// spec: AI-011
// acceptance: independent
#[test]
fn decisions_api_has_no_level_and_no_max_tokens() {
    let s = TaskSettings {
        reasoning: Some(High),
        max_tokens: Some(999),
        ..TaskSettings::default()
    };
    let p = plan(TaskType::Decision, &s, &["openai"]).unwrap();
    assert!(p.uses_decisions_api());
    assert_eq!(
        p.reasoning.effective, ModelDefault,
        "Decisions API hat keine Stufe"
    );
    assert_eq!(p.reasoning.warning, None);
    assert_eq!(p.output_limit(Some(16)), None);
    let mut req = request_template(&p);
    p.apply_to(&mut req, Some(16));
    assert_eq!(req.max_tokens, None);
    assert_eq!(req.reasoning, ModelDefault);
}

// spec: AI-011
// acceptance: independent
#[test]
fn embedding_has_no_level_and_no_max_tokens() {
    let p = plan(TaskType::Embedding, &TaskSettings::default(), &["openai"]).unwrap();
    assert_eq!(p.model_ref.endpoint, Endpoint::Embeddings);
    assert_eq!(p.reasoning.effective, ModelDefault);
    assert_eq!(p.output_limit(None), None);
}

// spec: AI-011
// acceptance: independent
#[test]
fn dropdown_levels_for_models_a_b_c_d_and_openrouter() {
    let dropdown = |task: TaskType, s: TaskSettings, keys: &[&str]| {
        plan(task, &s, keys).unwrap().reasoning.dropdown
    };
    // A: model-default, off, low … max
    let a = dropdown(TaskType::LlmsSummary, claude("mock-claude"), &["anthropic"]);
    assert!(a.contains(&ModelDefault) && a.contains(&Off) && a.contains(&Max));
    // B: kein off, kein max
    let b = dropdown(
        TaskType::LlmsSummary,
        claude("mock-claude-b"),
        &["anthropic"],
    );
    assert!(b.contains(&ModelDefault) && b.contains(&Low) && b.contains(&XHigh));
    assert!(!b.contains(&Off), "B bietet kein off: {b:?}");
    assert!(!b.contains(&Max));
    // C (thinking.supported = false, effort.supported = false) und D (ohne capabilities): nur model-default.
    // Für llms-summary genügt Text, daher sind C und D hier zuweisbar.
    let c = dropdown(
        TaskType::LlmsSummary,
        claude("mock-claude-c"),
        &["anthropic"],
    );
    assert_eq!(c, vec![ModelDefault]);
    let d = dropdown(
        TaskType::LlmsSummary,
        claude("mock-claude-d"),
        &["anthropic"],
    );
    assert_eq!(d, vec![ModelDefault]);
    // OpenRouter: M-R1 none … high → off, low, medium, high; M-R2 mandatory → ohne off; M-R3 ohne Parameter → nur
    // model-default
    let or = |id: &str| {
        dropdown(
            TaskType::LlmsSummary,
            on_model(
                TaskType::LlmsSummary,
                "openrouter",
                id,
                Endpoint::ChatCompletions,
            ),
            &["openrouter"],
        )
    };
    let mut r1 = or("acc/m-r1");
    r1.sort_by_key(|l| l.rank());
    let mut want = vec![ModelDefault, Off, Low, Medium, High];
    want.sort_by_key(|l| l.rank());
    assert_eq!(r1, want);
    let r2 = or("acc/m-r2");
    assert!(!r2.contains(&Off) && r2.contains(&Low) && r2.contains(&High));
    assert_eq!(or("acc/m-r3"), vec![ModelDefault]);
}

// spec: AI-011
// acceptance: independent
#[test]
fn stored_level_kept_after_model_switch_with_warning() {
    // gespeicherte Stufe max, Modellwechsel auf B (max nicht unterstützt) → effektiv xhigh, Warnung, Wert bleibt
    let s = TaskSettings {
        model: Some(ModelRef::new(
            "anthropic",
            "mock-claude-b",
            Endpoint::Messages,
        )),
        reasoning: Some(Max),
        ..TaskSettings::default()
    };
    let p = plan(TaskType::AskSql, &s, &["anthropic"]).unwrap();
    assert_eq!(p.reasoning.requested, Max);
    assert_eq!(p.reasoning.effective, XHigh);
    assert_eq!(
        p.reasoning.warning,
        Some(ReasoningWarning::StoredLevelUnavailable {
            stored: Max,
            effective: XHigh
        })
    );
    assert_eq!(s.reasoning, Some(Max), "gespeicherter Wert bleibt erhalten");
    // zurück auf A → max wieder effektiv, keine Warnung
    let s2 = TaskSettings {
        model: Some(ModelRef::new(
            "anthropic",
            "mock-claude",
            Endpoint::Messages,
        )),
        ..s.clone()
    };
    let p2 = plan(TaskType::AskSql, &s2, &["anthropic"]).unwrap();
    assert_eq!(p2.reasoning.effective, Max);
    assert_eq!(p2.reasoning.warning, None);
}

// spec: AI-011
// acceptance: independent
#[test]
fn user_max_tokens_replaces_standard_and_flags_tight_values() {
    let mut s = claude("mock-claude");
    s.max_tokens = Some(300);
    let p = plan(TaskType::SuggestTitle, &s, &["anthropic"]).unwrap();
    let l = p.output_limit(None).unwrap();
    assert_eq!(l.value, 300);
    assert_eq!(l.standard, 1024);
    assert!(l.user_set);
    assert!(
        l.tight,
        "kleiner als Standard → Hinweis „Ausgabegrenze knapp“"
    );
    s.max_tokens = Some(5000);
    let l = plan(TaskType::SuggestTitle, &s, &["anthropic"])
        .unwrap()
        .output_limit(None)
        .unwrap();
    assert_eq!(l.value, 5000);
    assert!(!l.tight);
    s.max_tokens = Some(1024);
    assert!(
        !plan(TaskType::SuggestTitle, &s, &["anthropic"])
            .unwrap()
            .output_limit(None)
            .unwrap()
            .tight
    );
}

// spec: AI-011
// acceptance: independent
#[test]
fn reset_to_default_clears_model_level_and_limit() {
    let mut s = TaskSettings {
        model: Some(ModelRef::new(
            "anthropic",
            "mock-claude-b",
            Endpoint::Messages,
        )),
        fallback: vec![ModelRef::new(
            "anthropic",
            "mock-claude",
            Endpoint::Messages,
        )],
        reasoning: Some(High),
        max_tokens: Some(123),
        ..TaskSettings::default()
    };
    s.reset_to_default();
    assert_eq!(s, TaskSettings::default());
    let p = plan(TaskType::LlmsSummary, &s, &["openai"]).unwrap();
    assert_eq!(p.reasoning.requested, Low);
    assert_eq!(p.model_ref.model, "gpt-6-luna");
}

// ---------------------------------------------------------------------------------------------------------------
// Profil-Export und -Import

// spec: AI-011
// acceptance: independent
#[test]
fn profile_export_import_roundtrip_keeps_assignment_level_and_limit() {
    let mut settings = BTreeMap::new();
    settings.insert(
        TaskType::Decision,
        TaskSettings {
            model: Some(ModelRef::new("openai", "gpt-6-luna", Endpoint::Decisions)),
            fallback: vec![
                ModelRef::new("anthropic", "mock-claude", Endpoint::Messages),
                ModelRef::new("openrouter", "acc/m-r1", Endpoint::ChatCompletions),
            ],
            reasoning: Some(Off),
            ..TaskSettings::default()
        },
    );
    settings.insert(
        TaskType::AskSql,
        TaskSettings {
            model: Some(ModelRef::new(
                "anthropic",
                "mock-claude-b",
                Endpoint::Messages,
            )),
            fallback: vec![],
            reasoning: Some(XHigh),
            max_tokens: Some(3333),
            temperature: Some(0.25),
            max_input_chars: Some(12_000),
        },
    );
    settings.insert(
        TaskType::LlmsSummary,
        TaskSettings {
            reasoning: Some(ModelDefault),
            ..TaskSettings::default()
        },
    );
    let mut config = os_model::config::CrawlConfig::default();
    config.ai.tasks = settings_to_profile(&settings);
    let toml = config.to_toml_string().unwrap();
    let loaded = os_model::config::CrawlConfig::from_toml_str(&toml).unwrap();
    let back = settings_from_profile(&loaded.ai.tasks).unwrap();
    assert_eq!(back, settings);
    // auch über JSON
    let json = serde_json::to_string(&config).unwrap();
    let loaded: os_model::config::CrawlConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(settings_from_profile(&loaded.ai.tasks).unwrap(), settings);
}

// spec: AI-011
// acceptance: independent
#[test]
fn profile_import_rejects_unknown_task_or_level() {
    let mut tasks = BTreeMap::new();
    tasks.insert(
        "suggest-everything".to_string(),
        os_model::config::TaskModelConfig::default(),
    );
    assert!(settings_from_profile(&tasks).is_err());
    let mut tasks = BTreeMap::new();
    tasks.insert(
        "ask-sql".to_string(),
        os_model::config::TaskModelConfig {
            effort: Some("turbo".into()),
            ..Default::default()
        },
    );
    assert!(settings_from_profile(&tasks).is_err());
}

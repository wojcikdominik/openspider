//! Modellwahl je Aufgabe (AI-011 samt Nachtrag Q00, AI-010 Nachtrag) gegen die Testdaten `tests/fixtures/ai/k04/`:
//! Anthropic-Modelle A–D und OpenRouter-Modelle `M-R1` … `M-R3` wie in 08 „Konventionen, Mock-Erweiterungen“
//! beschrieben (die Mock-Listen selbst baut M4-K09), OpenAI-Katalog aus `pricing-catalog.json`.
use std::collections::BTreeMap;
use std::path::PathBuf;

use os_ai::catalog::choice::{settings_from_profile, settings_to_profile, MissingCapability};
use os_ai::catalog::reasoning::{
    anthropic_levels, anthropic_output_limit, dropdown_levels, openrouter_levels,
    openrouter_output_limit, question_weight, request_weight,
};
use os_ai::catalog::{
    chat_prompt_max_tokens, choose_reasoning, default_max_tokens, plan_task, reasoning_reserve,
    validate_assignment, AvailableModel, Endpoint, ModelAvailability, ModelRef, PriceKey,
    PriceTable, ProviderState, ReasoningWarning, TaskDefaults, TaskPlan, TaskSettings, TaskType,
};
use os_ai::provider::{Capabilities, CapabilitySource, Model, ReasoningLevel, Request};
use os_model::ai::AiStatus;
use serde_json::{json, Value};
use ReasoningLevel::*;

fn fixture(name: &str) -> Value {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in ["..", "..", "tests", "fixtures", "ai", "k04", name] {
        p.push(part);
    }
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn defaults() -> TaskDefaults {
    TaskDefaults::from_json_str(&fixture("task-defaults-test.json").to_string()).unwrap()
}

fn pricing() -> PriceTable {
    PriceTable::from_json_str(&fixture("pricing-catalog.json").to_string()).unwrap()
}

fn supported(v: &Value) -> bool {
    v["supported"].as_bool() == Some(true)
}

/// Anthropic-Modell aus der Modellliste (Fähigkeiten nach AI-002, Stufen und Ausgabegrenze nach AI-011, Nachtrag).
fn anthropic(id: &str) -> AvailableModel {
    let list = fixture("anthropic-models.json");
    let entry = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .unwrap();
    let caps = &entry["capabilities"];
    AvailableModel {
        model: Model {
            provider: "anthropic".into(),
            id: id.into(),
            endpoint: "messages".into(),
            capabilities: Capabilities {
                text: caps.is_object(),
                structured_output: supported(&caps["structured_outputs"]),
                vision: supported(&caps["image_input"]),
                batch: supported(&caps["batch"]),
                ..Capabilities::default()
            },
            capability_source: CapabilitySource::AnthropicModels,
            reasoning_levels: anthropic_levels(entry),
        },
        max_output_tokens: anthropic_output_limit(entry),
    }
}

fn openrouter_entry(id: &str) -> Value {
    fixture("openrouter-models.json")["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .unwrap()
        .clone()
}

/// OpenAI-Modell mit Fähigkeiten, Stufen und Ausgabegrenze aus dem Katalog (AI-010).
fn openai(id: &str, endpoint: Endpoint) -> AvailableModel {
    let table = pricing();
    AvailableModel {
        model: Model {
            provider: "openai".into(),
            id: id.into(),
            endpoint: endpoint.as_str().into(),
            capabilities: table.capability_lookup("openai", endpoint)(id).unwrap_or_default(),
            capability_source: CapabilitySource::ModelCatalog,
            reasoning_levels: if endpoint == Endpoint::Responses {
                table.reasoning_lookup("openai", endpoint)(id)
            } else {
                Vec::new()
            },
        },
        max_output_tokens: table.max_output_tokens(&PriceKey::new("openai", id, endpoint)),
    }
}

fn all_anthropic() -> ProviderState {
    ProviderState {
        has_key: true,
        models: [
            "mock-claude",
            "mock-claude-b",
            "mock-claude-c",
            "mock-claude-d",
        ]
        .into_iter()
        .map(anthropic)
        .collect(),
    }
}

fn all_openai() -> ProviderState {
    ProviderState {
        has_key: true,
        models: vec![
            openai("gpt-6-luna", Endpoint::Responses),
            openai("gpt-6-luna", Endpoint::Decisions),
            openai("mock-reasoning", Endpoint::Responses),
            openai("mock-plain", Endpoint::Responses),
            openai("mock-embedding", Endpoint::Embeddings),
        ],
    }
}

fn providers(list: &[(&str, ProviderState)]) -> BTreeMap<String, ProviderState> {
    list.iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn on(model: ModelRef) -> TaskSettings {
    TaskSettings {
        model: Some(model),
        ..TaskSettings::default()
    }
}

fn a() -> ModelRef {
    ModelRef::new("anthropic", "mock-claude", Endpoint::Messages)
}
fn b() -> ModelRef {
    ModelRef::new("anthropic", "mock-claude-b", Endpoint::Messages)
}
fn c() -> ModelRef {
    ModelRef::new("anthropic", "mock-claude-c", Endpoint::Messages)
}
fn d() -> ModelRef {
    ModelRef::new("anthropic", "mock-claude-d", Endpoint::Messages)
}

fn request(model: Model) -> Request {
    Request {
        model,
        required: Capabilities::default(),
        input: Vec::new(),
        instructions: String::new(),
        questions: Vec::new(),
        response_schema: None,
        max_tokens: None,
        temperature: None,
        effort: None,
        reasoning: ReasoningLevel::ModelDefault,
    }
}

fn plan(task: TaskType, settings: &TaskSettings) -> TaskPlan {
    plan_task(
        task,
        settings,
        &defaults(),
        &providers(&[("anthropic", all_anthropic()), ("openai", all_openai())]),
    )
    .unwrap()
}

fn applied(plan: &TaskPlan, weight: Option<u64>) -> Request {
    let mut r = request(plan.model.clone());
    plan.apply_to(&mut r, weight);
    r
}

// spec: AI-011
// `suggest-title` auf Modell A (abschaltbar), Standardstufe `off` → effektiv `off`, `max_tokens = 1024` (2 × 512 + 0);
// auf Modell B (nicht abschaltbar) → effektive Stufe `low`, `max_tokens = 2048` (2 × 512 + 1.024) mit Warnhinweis.
#[test]
fn suggest_title_on_model_a_and_b() {
    let on_a = plan(TaskType::SuggestTitle, &on(a()));
    assert_eq!(on_a.reasoning.effective, Off);
    assert_eq!(on_a.reasoning.warning, None);
    let req = applied(&on_a, None);
    assert_eq!((req.reasoning, req.max_tokens), (Off, Some(1_024)));

    let on_b = plan(TaskType::SuggestTitle, &on(b()));
    assert_eq!(on_b.reasoning.effective, Low);
    assert_eq!(
        on_b.reasoning.warning,
        Some(ReasoningWarning::OffUnavailable { effective: Low })
    );
    let req = applied(&on_b, None);
    assert_eq!((req.reasoning, req.max_tokens), (Low, Some(2_048)));
}

// spec: AI-011
// Ersatzweg einer `choice`-Frage mit 14 Werten (W = 14) auf Modell A → A = 288, `max_tokens = 576`; gebündelt mit einer
// `predicate`-Frage → W = 16, `max_tokens = 2 × (64 + 256) = 640`.
#[test]
fn fallback_route_uses_the_decision_w_formula() {
    let choice = json!({"name": "Seitentyp", "type": "choice",
        "choices": (0..14).map(|i| json!({"value": format!("v{i}"), "description": "d"})).collect::<Vec<_>>()});
    let predicate = json!({"name": "Soft-404", "type": "predicate"});
    let score =
        json!({"type": "score", "levels": [{"label": "a"}, {"label": "b"}, {"label": "c"}]});
    assert_eq!(question_weight(&choice), 14);
    assert_eq!(question_weight(&predicate), 2);
    assert_eq!(question_weight(&score), 3);
    let fallback = plan(TaskType::Decision, &on(a()));
    assert!(!fallback.uses_decisions_api());
    let w14 = request_weight(std::slice::from_ref(&choice));
    assert_eq!(fallback.answer_tokens.unwrap().tokens(Some(w14)), 288);
    assert_eq!(applied(&fallback, Some(w14)).max_tokens, Some(576));
    let w16 = request_weight(&[choice, predicate]);
    assert_eq!(w16, 16);
    assert_eq!(applied(&fallback, Some(w16)).max_tokens, Some(640));
    assert_eq!(applied(&fallback, Some(w16)).reasoning, Off);
}

// spec: AI-011
// `ask-sql` mit einem OpenAI-Katalogmodell mit `reasoning_efforts` inkl. `medium` → effektive Stufe `medium`,
// `max_output_tokens = 6144` (2 × 1.024 + 4.096).
#[test]
fn ask_sql_on_openai_catalog_model() {
    let p = plan(TaskType::AskSql, &TaskSettings::default());
    assert_eq!(
        p.model_ref,
        ModelRef::new("openai", "mock-reasoning", Endpoint::Responses)
    );
    assert_eq!(p.model_output_limit, Some(32_000));
    let req = applied(&p, None);
    assert_eq!((req.reasoning, req.max_tokens), (Medium, Some(6_144)));
}

// spec: AI-011
// `audit-summary` auf Modell B (`max_tokens = 4000` in der Modellliste) → `max_tokens = 4000` statt 20.480; ebenso
// über OpenRouter mit `top_provider.max_completion_tokens = 4000`.
#[test]
fn audit_summary_is_capped_at_the_model_limit() {
    let p = plan(TaskType::AuditSummary, &TaskSettings::default());
    assert_eq!(p.model_ref, b());
    assert_eq!(p.reasoning.effective, Medium);
    let limit = p.output_limit(None).unwrap();
    assert_eq!((limit.standard, limit.value), (4_000, 4_000));
    assert_eq!(default_max_tokens(8_192, Medium, None), 20_480);
    let cap = openrouter_entry("mock/m-cap");
    assert_eq!(openrouter_output_limit(&cap), Some(4_000));
    assert_eq!(
        openrouter_output_limit(&openrouter_entry("mock/m-r1")),
        None
    );
    assert_eq!(
        default_max_tokens(8_192, Medium, openrouter_output_limit(&cap)),
        4_000
    );
}

// spec: AI-011
// Reasoning-Dropdown: Modell B ohne `off`, Modell D nur `model-default`; Modell A mit `off`; C ohne `thinking` und
// ohne `effort` nur `model-default`.
#[test]
fn reasoning_dropdown_per_anthropic_model() {
    let dd = |id| dropdown_levels(&anthropic(id).model.reasoning_levels);
    assert_eq!(
        dd("mock-claude"),
        vec![ModelDefault, Off, Low, Medium, High, Max]
    );
    assert_eq!(
        dd("mock-claude-b"),
        vec![ModelDefault, Low, Medium, High, XHigh]
    );
    assert!(!dd("mock-claude-b").contains(&Off));
    assert_eq!(dd("mock-claude-c"), vec![ModelDefault]);
    assert_eq!(dd("mock-claude-d"), vec![ModelDefault]);
    // AI-002 (Nachtrag): `max` auf B → `xhigh`, `medium` auf A → `medium`.
    assert_eq!(
        choose_reasoning(Max, &anthropic("mock-claude-b").model.reasoning_levels).effective,
        XHigh
    );
    assert_eq!(
        choose_reasoning(Medium, &anthropic("mock-claude").model.reasoning_levels).effective,
        Medium
    );
}

// spec: AI-010
// OpenAI-Modell mit `reasoning_efforts = ["none", "low", "medium", "high"]` → genau `off`, `low`, `medium`, `high` und
// `model-default`; ohne das Feld nur `model-default`.
#[test]
fn openai_catalog_levels_in_the_dropdown() {
    let with = openai("mock-reasoning", Endpoint::Responses);
    assert_eq!(
        dropdown_levels(&with.model.reasoning_levels),
        vec![ModelDefault, Off, Low, Medium, High]
    );
    let without = openai("mock-plain", Endpoint::Responses);
    assert_eq!(
        dropdown_levels(&without.model.reasoning_levels),
        vec![ModelDefault]
    );
    // Ohne Stufen gilt jede gespeicherte Stufe als `model-default` (kein Parameter) – mit Warnung.
    let choice = choose_reasoning(Medium, &without.model.reasoning_levels);
    assert_eq!(choice.effective, ModelDefault);
    assert_eq!(
        choice.warning,
        Some(ReasoningWarning::StoredLevelUnavailable {
            stored: Medium,
            effective: ModelDefault
        })
    );
}

// spec: AI-011
// OpenRouter-Stufen (AI-005, Nachtrag): `M-R1` mit `none` → `off` verfügbar; `M-R2` (`mandatory`) ohne `off`, Stufe
// `off` → `low`; `M-R3` ohne `"reasoning"` in `supported_parameters` → nur `model-default`.
#[test]
fn openrouter_levels_follow_supported_parameters_and_mandatory() {
    let r1 = openrouter_levels(&openrouter_entry("mock/m-r1"));
    assert_eq!(r1, vec![Off, Low, Medium, High]);
    let r2 = openrouter_levels(&openrouter_entry("mock/m-r2"));
    assert_eq!(r2, vec![Low, Medium, High]);
    assert_eq!(choose_reasoning(Off, &r2).effective, Low);
    assert_eq!(openrouter_levels(&openrouter_entry("mock/m-r3")), vec![]);
    assert_eq!(openrouter_levels(&openrouter_entry("mock/m-cap")), vec![]);
}

// spec: AI-011
// Ist nur ein Anthropic-Schlüssel hinterlegt, laufen Decision-Fragen über das erste Anthropic-Modell der Ersatzliste
// (Ersatzweg, `<Q> Source = self-reported`); mit OpenAI-Schlüssel über die Decisions API ohne Stufe und ohne
// `max_tokens`.
#[test]
fn decision_falls_back_to_the_first_anthropic_model() {
    let only_anthropic = providers(&[
        ("anthropic", all_anthropic()),
        ("openai", ProviderState::default()),
    ]);
    let p = plan_task(
        TaskType::Decision,
        &TaskSettings::default(),
        &defaults(),
        &only_anthropic,
    )
    .unwrap();
    assert_eq!(p.model_ref, a());
    assert_eq!(p.candidate_index, 1);
    assert!(!p.uses_decisions_api());
    let both = providers(&[("anthropic", all_anthropic()), ("openai", all_openai())]);
    let p = plan_task(
        TaskType::Decision,
        &TaskSettings::default(),
        &defaults(),
        &both,
    )
    .unwrap();
    assert!(p.uses_decisions_api());
    assert_eq!(p.candidate_index, 0);
    assert_eq!(p.reasoning.warning, None);
    let req = applied(&p, Some(16));
    assert_eq!((req.reasoning, req.max_tokens), (ModelDefault, None));
}

// spec: AI-011
// Kein Modell der Ersatzliste verfügbar: ohne passenden Schlüssel `Error: Missing API key`, mit Schlüssel, aber ohne
// geeignetes Modell `Skipped (no capable model)`.
#[test]
fn no_usable_candidate_yields_missing_key_or_no_capable_model() {
    let none = providers(&[]);
    for task in TaskType::ALL {
        assert_eq!(
            plan_task(task, &TaskSettings::default(), &defaults(), &none),
            Err(AiStatus::MissingApiKey),
            "{task}"
        );
    }
    // Nur OpenRouter verbunden: die Kandidaten der Entscheidung sind OpenAI/Anthropic → fehlender Schlüssel.
    let only_or = providers(&[(
        "openrouter",
        ProviderState {
            has_key: true,
            models: Vec::new(),
        },
    )]);
    assert_eq!(
        plan_task(
            TaskType::Decision,
            &TaskSettings::default(),
            &defaults(),
            &only_or
        ),
        Err(AiStatus::MissingApiKey)
    );
    // Anthropic-Schlüssel da, aber nur Modelle ohne Vision bzw. ohne Fähigkeiten in der Liste.
    let settings = TaskSettings {
        model: Some(c()),
        fallback: vec![d()],
        ..TaskSettings::default()
    };
    assert_eq!(
        plan_task(
            TaskType::SuggestAlt,
            &settings,
            &defaults(),
            &providers(&[("anthropic", all_anthropic())])
        ),
        Err(AiStatus::SkippedNoCapableModel)
    );
    // Schlüssel da, Modell fehlt in der Live-Liste.
    let gone = TaskSettings {
        model: Some(ModelRef::new(
            "anthropic",
            "mock-claude-retired",
            Endpoint::Messages,
        )),
        ..TaskSettings::default()
    };
    assert_eq!(
        plan_task(
            TaskType::SuggestTitle,
            &gone,
            &defaults(),
            &providers(&[("anthropic", all_anthropic())])
        ),
        Err(AiStatus::SkippedNoCapableModel)
    );
}

// spec: AI-011
// Ein Modell ohne benötigte Fähigkeit kann der Aufgabe nicht zugewiesen werden (z. B. Nicht-Vision-Modell für
// `suggest-alt`); die Wahl überspringt es zugunsten der Ersatzliste.
#[test]
fn models_without_required_capability_cannot_be_assigned() {
    let err =
        validate_assignment(TaskType::SuggestAlt, &anthropic("mock-claude-c").model).unwrap_err();
    assert_eq!(
        err,
        MissingCapability {
            task: TaskType::SuggestAlt,
            model: "mock-claude-c".into(),
            missing: vec!["structured_output", "vision"],
        }
    );
    assert!(validate_assignment(TaskType::SuggestAlt, &anthropic("mock-claude").model).is_ok());
    assert!(validate_assignment(
        TaskType::SuggestAlt,
        &openai("mock-plain", Endpoint::Responses).model
    )
    .is_err());
    assert!(validate_assignment(
        TaskType::Decision,
        &openai("gpt-6-luna", Endpoint::Decisions).model
    )
    .is_ok());
    assert!(validate_assignment(
        TaskType::SuggestTitle,
        &openai("gpt-6-luna", Endpoint::Decisions).model
    )
    .is_err());
    assert!(validate_assignment(
        TaskType::Embedding,
        &openai("mock-embedding", Endpoint::Embeddings).model
    )
    .is_ok());
    assert!(validate_assignment(TaskType::Embedding, &anthropic("mock-claude").model).is_err());
    let settings = TaskSettings {
        model: Some(c()),
        fallback: vec![a()],
        ..TaskSettings::default()
    };
    let p = plan(TaskType::SuggestAlt, &settings);
    assert_eq!((p.model_ref.clone(), p.candidate_index), (a(), 1));
}

// spec: AI-011
// Die gespeicherte Stufe bleibt nach einem Modellwechsel erhalten; es gilt die effektive Stufe mit Warnung.
// „Auf Standard zurücksetzen“ setzt Modell, Stufe und Ausgabegrenze zurück.
#[test]
fn stored_level_survives_model_change_and_reset_restores_defaults() {
    let mut settings = TaskSettings {
        model: Some(b()),
        reasoning: Some(Max),
        ..TaskSettings::default()
    };
    let p = plan(TaskType::LlmsSummary, &settings);
    assert_eq!(p.reasoning.effective, XHigh);
    assert_eq!(
        p.reasoning.warning,
        Some(ReasoningWarning::StoredLevelUnavailable {
            stored: Max,
            effective: XHigh
        })
    );
    assert_eq!(settings.reasoning, Some(Max));
    settings.model = Some(a());
    assert_eq!(
        plan(TaskType::LlmsSummary, &settings).reasoning.effective,
        Max
    );
    settings.max_tokens = Some(100);
    settings.reset_to_default();
    assert_eq!(settings, TaskSettings::default());
    let p = plan(TaskType::LlmsSummary, &settings);
    assert_eq!((p.model_ref.clone(), p.reasoning.effective), (a(), Low));
}

// spec: AI-011
// Ein vom Nutzer eingestellter Wert ersetzt den Standard; kleiner als der Standard → Kennzeichen „knapp“.
#[test]
fn user_max_tokens_replace_the_standard_and_flag_tight_values() {
    let tight = TaskSettings {
        model: Some(a()),
        max_tokens: Some(256),
        ..TaskSettings::default()
    };
    let p = plan(TaskType::SuggestTitle, &tight);
    let limit = p.output_limit(None).unwrap();
    assert_eq!(
        (limit.value, limit.standard, limit.user_set, limit.tight),
        (256, 1_024, true, true)
    );
    assert_eq!(applied(&p, None).max_tokens, Some(256));
    let roomy = TaskSettings {
        model: Some(a()),
        max_tokens: Some(3_000),
        ..TaskSettings::default()
    };
    let limit = plan(TaskType::SuggestTitle, &roomy)
        .output_limit(None)
        .unwrap();
    assert_eq!((limit.value, limit.tight), (3_000, false));
}

// spec: AI-011
// Reserve R je effektiver Stufe und die Regel min(2 × A + R, Ausgabegrenze).
#[test]
fn reasoning_reserves_follow_the_spec_table() {
    let expected = [
        (Off, 0),
        (Minimal, 512),
        (Low, 1_024),
        (Medium, 4_096),
        (High, 8_192),
        (XHigh, 16_384),
        (Max, 32_768),
        (ModelDefault, 4_096),
    ];
    for (level, reserve) in expected {
        assert_eq!(reasoning_reserve(level), reserve, "{level}");
        assert_eq!(default_max_tokens(512, level, None), 1_024 + reserve);
        assert_eq!(
            default_max_tokens(512, level, Some(1_000)),
            (1_024 + reserve).min(1_000)
        );
    }
}

// spec: INTEG-162 [M4]
// Chat-Prompts aus 04: `model-default` und `max_tokens` = min(2 × 1.024 + 4.096, Ausgabegrenze) – 6.144 auf Modell A
// (Grenze 8.192) und ohne Grenze, 4.000 auf einem Modell mit Grenze 4.000.
#[test]
fn chat_prompts_use_the_same_rule_with_1024_answer_tokens() {
    assert_eq!(
        chat_prompt_max_tokens(anthropic("mock-claude").max_output_tokens),
        6_144
    );
    assert_eq!(chat_prompt_max_tokens(None), 6_144);
    assert_eq!(
        chat_prompt_max_tokens(anthropic("mock-claude-b").max_output_tokens),
        4_000
    );
}

// spec: AI-011
// Profil-Export und -Import erhalten Zuordnung, Ersatzliste, Reasoning-Stufe und eingestellte Ausgabegrenze
// (Roundtrip über das Konfigurationsprofil `AiConfig.tasks`).
#[test]
fn profile_round_trip_keeps_assignment_reasoning_and_output_limit() {
    let mut settings = BTreeMap::new();
    settings.insert(
        TaskType::SuggestTitle,
        TaskSettings {
            model: Some(b()),
            fallback: vec![
                a(),
                ModelRef::new("openrouter", "mock/m-r1", Endpoint::ChatCompletions),
            ],
            reasoning: Some(High),
            max_tokens: Some(3_000),
            temperature: Some(0.2),
            max_input_chars: Some(8_000),
        },
    );
    settings.insert(TaskType::Embedding, TaskSettings::default());
    settings.insert(
        TaskType::AskSql,
        TaskSettings {
            reasoning: Some(ModelDefault),
            ..TaskSettings::default()
        },
    );
    let config = os_model::config::AiConfig {
        tasks: settings_to_profile(&settings),
        ..Default::default()
    };
    assert_eq!(
        config.tasks["suggest-title"].effort.as_deref(),
        Some("high")
    );
    let exported = serde_json::to_string(&config).unwrap();
    let imported: os_model::config::AiConfig = serde_json::from_str(&exported).unwrap();
    assert_eq!(settings_from_profile(&imported.tasks).unwrap(), settings);
    // Unbekannte Stufe oder Aufgabe im Profil wird beim Import gemeldet.
    let mut broken = imported.tasks.clone();
    broken.get_mut("suggest-title").unwrap().effort = Some("turbo".into());
    assert!(settings_from_profile(&broken).is_err());
    let mut broken = imported.tasks;
    broken.insert("summarize-all".into(), Default::default());
    assert!(settings_from_profile(&broken).is_err());
}

// spec: AI-010
// Ein gewähltes Modell, das in der Live-Liste fehlt, wird als „nicht mehr verfügbar“ erkannt.
#[test]
fn chosen_model_missing_from_live_list_is_no_longer_available() {
    let state = providers(&[
        ("anthropic", all_anthropic()),
        ("openai", ProviderState::default()),
    ]);
    assert_eq!(
        os_ai::catalog::choice::availability(&a(), &state),
        ModelAvailability::Available
    );
    assert_eq!(
        os_ai::catalog::choice::availability(
            &ModelRef::new("anthropic", "mock-claude-retired", Endpoint::Messages),
            &state
        ),
        ModelAvailability::NoLongerAvailable
    );
    assert_eq!(
        os_ai::catalog::choice::availability(
            &ModelRef::new("openai", "gpt-6-luna", Endpoint::Responses),
            &state
        ),
        ModelAvailability::ProviderNotConnected
    );
}

// spec: AI-011
// Die Kostenspalte nutzt den Preis des tatsächlich genutzten Endpunkts: `gpt-6-luna` in `decision` → `decisions`, in
// `suggest-title` → `responses` (verschiedene Preisschlüssel).
#[test]
fn plan_endpoint_selects_the_price_key() {
    let table = pricing();
    let decision = plan(TaskType::Decision, &TaskSettings::default());
    let title = plan(TaskType::SuggestTitle, &TaskSettings::default());
    let key = |p: &TaskPlan| {
        PriceKey::new(
            &p.model_ref.provider,
            &p.model_ref.model,
            p.model_ref.endpoint,
        )
    };
    assert_eq!(
        key(&decision),
        PriceKey::new("openai", "gpt-6-luna", Endpoint::Decisions)
    );
    assert_eq!(
        key(&title),
        PriceKey::new("openai", "gpt-6-luna", Endpoint::Responses)
    );
    let d = table.get(&key(&decision)).unwrap().rates(None).unwrap();
    let t = table.get(&key(&title)).unwrap().rates(None).unwrap();
    assert_eq!((d.output_pico, t.output_pico), (0, 500_000));
}

fn filled_defaults() -> TaskDefaults {
    TaskDefaults::from_json_str(&fixture("task-defaults-filled.json").to_string()).unwrap()
}

// spec: AI-011
// spec: AI-010
// Bestätigte Liste (Testdaten in der späteren Form): mit allen Schlüsseln läuft jede Aufgabe über ihr Standardmodell;
// fallen Anbieter aus, greift die Ersatzliste in Reihenfolge. Ein Modell ohne Preiseintrag ist wählbar, sein Preis
// bleibt unbekannt (Executable/„Preis unbekannt“ entscheidet der Voranschlag).
#[test]
fn confirmed_list_uses_defaults_then_fallbacks_including_unpriced_models() {
    let defaults = filled_defaults();
    let all = providers(&[("anthropic", all_anthropic()), ("openai", all_openai())]);
    for task in TaskType::ALL {
        let p = plan_task(task, &TaskSettings::default(), &defaults, &all).unwrap();
        assert_eq!(p.candidate_index, 0, "{task}");
        assert_eq!(
            Some(&p.model_ref),
            defaults.get(task).model.as_ref(),
            "{task}"
        );
    }
    // Nur Anthropic: Entscheidungen und Vorschläge über A (Ersatzweg), Embeddings ohne Schlüssel.
    let only_anthropic = providers(&[("anthropic", all_anthropic())]);
    for task in [TaskType::Decision, TaskType::SuggestTitle, TaskType::AskSql] {
        let p = plan_task(task, &TaskSettings::default(), &defaults, &only_anthropic).unwrap();
        assert_eq!((p.model_ref.clone(), p.candidate_index), (a(), 1), "{task}");
    }
    assert_eq!(
        plan_task(
            TaskType::Embedding,
            &TaskSettings::default(),
            &defaults,
            &only_anthropic
        ),
        Err(AiStatus::MissingApiKey)
    );
    // Nur OpenAI, `mock-reasoning` fehlt in der Live-Liste: `ask-sql` landet auf dem unbepreisten Modell.
    let mut openai = all_openai();
    openai.models.retain(|m| m.model.id != "mock-reasoning");
    let mut unpriced = openai.models[0].clone();
    unpriced.model.id = "mock-unpriced".into();
    unpriced.model.reasoning_levels = vec![Off, Low, Medium];
    openai.models.push(unpriced);
    let p = plan_task(
        TaskType::AskSql,
        &TaskSettings::default(),
        &defaults,
        &providers(&[("openai", openai)]),
    )
    .unwrap();
    assert_eq!(
        (p.model_ref.clone(), p.candidate_index),
        (
            ModelRef::new("openai", "mock-unpriced", Endpoint::Responses),
            2
        )
    );
    assert_eq!(p.output_limit(None).unwrap().value, 6_144);
    let catalog = os_ai::catalog::PriceCatalog::new(pricing());
    let key = PriceKey::new("openai", "mock-unpriced", Endpoint::Responses);
    assert!(catalog.resolve(&key, None).unwrap().is_none());
}

// spec: AI-011
// Festlegung des Orchestrators (25.11.2026, Befund B1 der Abnahme): Aufgabe ohne jeden Kandidaten (ausgelieferte
// `task-defaults.json` im Platzhalterzustand, keine Wahl im Profil) → `Skipped (no capable model)`, sobald ein
// Anbieter einen Schlüssel hat; nur ohne jeden Schlüssel `Error: Missing API key`.
#[test]
fn task_without_any_candidate_depends_on_any_key() {
    let bundled = TaskDefaults::bundled().unwrap();
    let keyed = |ids: &[&str]| -> BTreeMap<String, ProviderState> {
        ids.iter()
            .map(|id| {
                (
                    id.to_string(),
                    ProviderState {
                        has_key: true,
                        models: Vec::new(),
                    },
                )
            })
            .collect()
    };
    for task in TaskType::ALL {
        let none = plan_task(task, &TaskSettings::default(), &bundled, &providers(&[]));
        assert_eq!(none, Err(AiStatus::MissingApiKey), "{task}");
        let without_key = providers(&[("openai", ProviderState::default())]);
        assert_eq!(
            plan_task(task, &TaskSettings::default(), &bundled, &without_key),
            Err(AiStatus::MissingApiKey),
            "{task}"
        );
        for set in [
            &["anthropic"][..],
            &["openrouter"][..],
            &["anthropic", "openai", "openrouter"][..],
        ] {
            assert_eq!(
                plan_task(task, &TaskSettings::default(), &bundled, &keyed(set)),
                Err(AiStatus::SkippedNoCapableModel),
                "{task} {set:?}"
            );
        }
    }
    // Mit vollständigen Live-Listen ebenso: ohne Kandidat kein Modell, aber kein fehlender Schlüssel.
    let all = providers(&[("anthropic", all_anthropic()), ("openai", all_openai())]);
    assert_eq!(
        plan_task(
            TaskType::SuggestTitle,
            &TaskSettings::default(),
            &bundled,
            &all
        ),
        Err(AiStatus::SkippedNoCapableModel)
    );
}

// spec: AI-011
// Aufgabe mit Kandidaten (Auslegung K04 bleibt): `Error: Missing API key`, wenn keiner der Anbieter aus Wahl und
// Ersatzliste einen Schlüssel hat – auch wenn ein anderer Anbieter verbunden ist; sonst `Skipped (no capable model)`.
#[test]
fn task_with_candidates_depends_on_candidate_providers_only() {
    let settings = TaskSettings {
        model: Some(c()),
        fallback: vec![d()],
        ..TaskSettings::default()
    };
    let other_only = providers(&[("openai", all_openai())]);
    assert_eq!(
        plan_task(TaskType::SuggestAlt, &settings, &defaults(), &other_only),
        Err(AiStatus::MissingApiKey)
    );
    let with_anthropic = providers(&[("anthropic", all_anthropic()), ("openai", all_openai())]);
    assert_eq!(
        plan_task(
            TaskType::SuggestAlt,
            &settings,
            &defaults(),
            &with_anthropic
        ),
        Err(AiStatus::SkippedNoCapableModel)
    );
}

//! Unabhängige Abnahmetests K4-20: bestätigte Standardmodelle (AI-011, Nachtrag 25.11.2026) und Aufgabe ohne
//! Kandidaten (AI-011, Nachtrag 25.11.2026).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` AI-011: Tabelle „bestätigte Standardmodelle“ (Standard und
//! Reihenfolge der Ersatzliste je Aufgabe), Kriterien zu den Schlüsselkombinationen (nur OpenAI / nur Anthropic /
//! nur OpenRouter) samt „Berichtigung 25.11.2026“, Nachtrag „Aufgabe ohne Kandidaten“ gegen eine Datei ohne
//! Standardmodelle; AI-010 (Preis je genanntem Endpunkt mit `source_url` und `verified_at`). Live-Listen der
//! Anbieter als Fixtures (`tests/fixtures/ai/k4-20-acceptance/`), OpenAI-Fähigkeiten aus dem Katalog.
// acceptance: independent

mod acceptance_k4_20_support;

use acceptance_k4_20_support::*;
use os_ai::catalog::{
    plan_task, validate_assignment, CivilDate, Endpoint, ModelRef, PriceKey, PriceTable,
    TaskDefaults, TaskSettings, TaskType,
};
use os_ai::AiStatus;

fn r(provider: &str, model: &str, endpoint: Endpoint) -> ModelRef {
    ModelRef::new(provider, model, endpoint)
}

fn luna_decisions() -> ModelRef {
    r("openai", "gpt-6-luna", Endpoint::Decisions)
}
fn luna_responses() -> ModelRef {
    r("openai", "gpt-6-luna", Endpoint::Responses)
}
fn sol_responses() -> ModelRef {
    r("openai", "gpt-6-sol", Endpoint::Responses)
}
fn haiku() -> ModelRef {
    r("anthropic", "claude-haiku-5-5", Endpoint::Messages)
}
fn sonnet() -> ModelRef {
    r("anthropic", "claude-sonnet-5-5", Endpoint::Messages)
}
fn or_luna() -> ModelRef {
    r("openrouter", "openai/gpt-6-luna", Endpoint::ChatCompletions)
}
fn embed_small() -> ModelRef {
    r("openai", "text-embedding-3-small", Endpoint::Embeddings)
}

/// Tabelle „bestätigte Standardmodelle“ aus AI-011 (Nachtrag 25.11.2026).
fn spec_table() -> Vec<(TaskType, ModelRef, Vec<ModelRef>)> {
    let mut rows = Vec::new();
    for t in [TaskType::Decision, TaskType::DecisionVision] {
        rows.push((t, luna_decisions(), vec![haiku(), or_luna()]));
    }
    for t in [
        TaskType::SuggestTitle,
        TaskType::SuggestMeta,
        TaskType::SuggestH1,
        TaskType::LinkAnchor,
        TaskType::RedirectConfirm,
        TaskType::SuggestAlt,
    ] {
        rows.push((t, luna_responses(), vec![haiku()]));
    }
    for t in [
        TaskType::AskSql,
        TaskType::AuditSummary,
        TaskType::LlmsSummary,
    ] {
        rows.push((t, sonnet(), vec![sol_responses(), luna_responses()]));
    }
    rows.push((TaskType::Embedding, embed_small(), vec![]));
    rows
}

fn shipped() -> TaskDefaults {
    TaskDefaults::bundled().expect("ausgelieferte task-defaults.json lädt")
}

fn plan(task: TaskType, keys: &[&str]) -> Result<os_ai::catalog::TaskPlan, AiStatus> {
    plan_task(task, &TaskSettings::default(), &shipped(), &providers(keys))
}

// ---------------------------------------------------------------------------------------------------------------
// Ausgelieferte Liste = Tabelle

// spec: AI-011
// acceptance: independent
#[test]
fn shipped_defaults_equal_the_confirmed_table() {
    let defaults = shipped();
    let rows = spec_table();
    assert_eq!(
        rows.len(),
        TaskType::ALL.len(),
        "Tabelle deckt alle 12 Aufgaben"
    );
    for (task, model, fallback) in rows {
        let d = defaults.get(task);
        assert_eq!(d.model.as_ref(), Some(&model), "{task}: Standardmodell");
        assert_eq!(d.fallback, fallback, "{task}: Ersatzliste und Reihenfolge");
    }
}

// spec: AI-011
// acceptance: independent
#[test]
fn shipped_defaults_are_marked_confirmed() {
    // Hinweis „Vorschlag ausstehend“ erledigt: Liste bestätigt.
    let doc = resource_json("task-defaults.json");
    assert_eq!(doc["models_status"], "confirmed");
    assert!(CivilDate::parse(doc["verified_at"].as_str().unwrap()).is_some());
}

// spec: AI-011
// acceptance: independent
#[test]
fn every_listed_model_has_a_price_for_its_endpoint_with_source_and_date() {
    let table = PriceTable::bundled().unwrap();
    let mut checked = 0;
    for (task, model, fallback) in spec_table() {
        for m in std::iter::once(&model).chain(fallback.iter()) {
            if m.provider == "openrouter" {
                continue; // als OpenRouter-Modell gekennzeichnet, Preis aus der API
            }
            let entry = table
                .get(&PriceKey::new(&m.provider, &m.model, m.endpoint))
                .unwrap_or_else(|| {
                    panic!(
                        "{task}: {}/{}/{} ohne Preis",
                        m.provider,
                        m.model,
                        m.endpoint.as_str()
                    )
                });
            assert!(
                entry.source_url.starts_with("https://"),
                "{task}: source_url {:?} ist keine Anbieterseite",
                entry.source_url
            );
            assert!(
                CivilDate::parse(&entry.verified_at).is_some(),
                "{task}: verified_at {:?}",
                entry.verified_at
            );
            checked += 1;
        }
    }
    assert!(checked > 0);
    // dieselbe Regel über die Produktprüfung
    assert_eq!(
        shipped().check_against_pricing(&table),
        Vec::<String>::new()
    );
}

// spec: AI-011
// acceptance: independent
#[test]
fn every_default_model_has_the_capabilities_of_its_task() {
    // Kein Standard- bzw. Ersatzmodell, das der Aufgabe nicht zugewiesen werden könnte (AI-011).
    let all = providers(&["anthropic", "openai", "openrouter"]);
    for (task, model, fallback) in spec_table() {
        for m in std::iter::once(&model).chain(fallback.iter()) {
            let state = &all[&m.provider];
            let found = state
                .models
                .iter()
                .find(|a| a.model.id == m.model && a.model.endpoint == m.endpoint.as_str())
                .unwrap_or_else(|| panic!("{task}: {} fehlt in der Testliste", m.model));
            assert!(
                validate_assignment(task, &found.model).is_ok(),
                "{task}: {} nicht zuweisbar ({:?})",
                m.model,
                validate_assignment(task, &found.model)
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Schlüsselkombinationen (Kriterien des Nachtrags samt Berichtigung)

// spec: AI-011
// acceptance: independent
#[test]
fn all_keys_use_the_default_of_every_task() {
    for (task, model, _) in spec_table() {
        let p = plan(task, &["anthropic", "openai", "openrouter"]).unwrap();
        assert_eq!(p.model_ref, model, "{task}");
        assert_eq!(p.candidate_index, 0, "{task}");
    }
}

// spec: AI-011
// acceptance: independent
#[test]
fn only_openai_key() {
    let keys = ["openai"];
    let p = plan(TaskType::Decision, &keys).unwrap();
    assert_eq!(p.model_ref, luna_decisions());
    assert!(p.uses_decisions_api());
    assert_eq!(
        plan(TaskType::SuggestTitle, &keys).unwrap().model_ref,
        luna_responses()
    );
    let p = plan(TaskType::AskSql, &keys).unwrap();
    assert_eq!(
        p.model_ref,
        sol_responses(),
        "erstes verfügbares Ersatzmodell"
    );
    assert_eq!(p.candidate_index, 1);
    assert_eq!(
        plan(TaskType::Embedding, &keys).unwrap().model_ref,
        embed_small()
    );
}

// spec: AI-011
// acceptance: independent
#[test]
fn only_anthropic_key() {
    let keys = ["anthropic"];
    let d = plan(TaskType::Decision, &keys).unwrap();
    assert_eq!(d.model_ref, haiku());
    assert!(!d.uses_decisions_api(), "Ersatzweg mit Schema (AI-034)");
    assert_eq!(
        plan(TaskType::SuggestTitle, &keys).unwrap().model_ref,
        haiku()
    );
    assert_eq!(plan(TaskType::AskSql, &keys).unwrap().model_ref, sonnet());
    assert_eq!(
        plan(TaskType::Embedding, &keys).unwrap_err(),
        AiStatus::MissingApiKey
    );
}

// spec: AI-011
// acceptance: independent
#[test]
fn only_openrouter_key() {
    let keys = ["openrouter"];
    let d = plan(TaskType::Decision, &keys).unwrap();
    assert_eq!(d.model_ref, or_luna());
    assert!(!d.uses_decisions_api());
    for task in [
        TaskType::SuggestTitle,
        TaskType::AskSql,
        TaskType::Embedding,
    ] {
        assert_eq!(
            plan(task, &keys).unwrap_err(),
            AiStatus::MissingApiKey,
            "{task}: Kandidaten vorhanden, aber ohne Schlüssel (Berichtigung 25.11.2026)"
        );
    }
}

// spec: AI-011
// acceptance: independent
#[test]
fn no_key_gives_missing_api_key_for_every_task() {
    for task in TaskType::ALL {
        assert_eq!(
            plan(task, &[]).unwrap_err(),
            AiStatus::MissingApiKey,
            "{task}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Aufgabe ohne Kandidaten (gegen eine Datei ohne Standardmodelle)

fn no_models() -> TaskDefaults {
    TaskDefaults::from_json_str(&fixture_text("task-defaults-no-models.json"))
        .expect("Fixture ohne Standardmodelle gültig")
}

// spec: AI-011
// acceptance: independent
#[test]
fn task_without_candidates_is_skipped_when_any_key_exists() {
    let defaults = no_models();
    for task in TaskType::ALL {
        assert_eq!(defaults.get(task).model, None);
        assert!(defaults.get(task).fallback.is_empty());
    }
    for keys in [
        vec!["anthropic", "openai", "openrouter"],
        vec!["anthropic"],
        vec!["openai"],
        vec!["openrouter"],
    ] {
        let p = providers(&keys);
        for task in TaskType::ALL {
            assert_eq!(
                plan_task(task, &TaskSettings::default(), &defaults, &p).unwrap_err(),
                AiStatus::SkippedNoCapableModel,
                "{task} mit Schlüsseln {keys:?}"
            );
        }
    }
}

// spec: AI-011
// acceptance: independent
#[test]
fn task_without_candidates_and_without_any_key_is_missing_api_key() {
    let defaults = no_models();
    let p = providers(&[]);
    for task in TaskType::ALL {
        assert_eq!(
            plan_task(task, &TaskSettings::default(), &defaults, &p).unwrap_err(),
            AiStatus::MissingApiKey,
            "{task}"
        );
    }
}

// spec: AI-011
// acceptance: independent
#[test]
fn task_without_defaults_but_with_profile_choice_runs() {
    // „keine Wahl im Profil“ ist Teil der Bedingung: mit Wahl gilt das Grundkriterium.
    let defaults = no_models();
    let settings = TaskSettings {
        model: Some(haiku()),
        ..TaskSettings::default()
    };
    let p = plan_task(
        TaskType::SuggestTitle,
        &settings,
        &defaults,
        &providers(&["anthropic"]),
    )
    .unwrap();
    assert_eq!(p.model_ref, haiku());
    // gewählter Kandidat ohne Schlüssel seines Anbieters → Missing API key, auch wenn ein anderer Schlüssel da ist
    assert_eq!(
        plan_task(
            TaskType::SuggestTitle,
            &settings,
            &defaults,
            &providers(&["openai"])
        )
        .unwrap_err(),
        AiStatus::MissingApiKey
    );
}

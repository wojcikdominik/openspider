//! CI-Prüfung der ausgelieferten Datendateien unter `resources/ai/` (AI-010, AI-011, AI-012 Nachtrag Q00) und der
//! Testdaten `tests/fixtures/ai/k04/`.
use std::path::PathBuf;

use os_ai::catalog::limits::ProviderLimitsFile;
use os_ai::catalog::reasoning::{AnswerFormula, AnswerTokens};
use os_ai::catalog::task_defaults::{validate_task_defaults, ModelsStatus};
use os_ai::catalog::{CivilDate, Endpoint, PriceTable, TaskDefaults, TaskType};
use os_ai::provider::ReasoningLevel;
use serde_json::Value;

fn repo(parts: &[&str]) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in ["..", ".."].iter().chain(parts) {
        p.push(part);
    }
    p
}

fn read(parts: &[&str]) -> String {
    std::fs::read_to_string(repo(parts)).unwrap()
}

// spec: AI-010
// Die ausgelieferte `pricing.json` besteht den Schematest; jeder Eintrag trägt `source_url` und `verified_at`;
// Anthropic-Einträge haben keine Fähigkeits- und Stufenfelder; OpenAI-`responses`-Einträge führen `reasoning_efforts`.
#[test]
fn bundled_pricing_is_valid_and_sourced() {
    let table = PriceTable::bundled().unwrap();
    assert!(!table.file().test_only);
    assert!(!table.entries().is_empty());
    for entry in table.entries() {
        assert!(!entry.source_url.is_empty());
        assert!(CivilDate::parse(&entry.verified_at).is_some(), "{entry:?}");
        if entry.provider == "anthropic" {
            assert!(entry.capabilities.is_none());
            assert!(entry.reasoning_efforts.is_none());
            assert!(entry.max_output_tokens.is_none());
        }
        if entry.provider == "openai" && entry.endpoint == Endpoint::Responses {
            assert!(entry.reasoning_efforts.is_some(), "{}", entry.model);
        }
    }
    let doc: Value = serde_json::from_str(&read(&["resources", "ai", "pricing.json"])).unwrap();
    assert_eq!(os_ai::catalog::pricing::validate_pricing(&doc), Ok(()));
}

// spec: AI-010
// Werte aus AI-010 für `gpt-6-luna`: `responses` 0,10/0,50, Cache-Lesen 0,01, Batch 50 %, Langkontext ab 272.000 mit
// 0,20/0,75 (Faktoren 2 und 1,5); `decisions` nur Eingabe 0,10.
#[test]
fn bundled_luna_prices_match_the_spec() {
    let table = PriceTable::bundled().unwrap();
    let key = |e| os_ai::catalog::PriceKey::new("openai", "gpt-6-luna", e);
    let r = table
        .get(&key(Endpoint::Responses))
        .unwrap()
        .rates(None)
        .unwrap();
    assert_eq!(
        (r.input_pico, r.output_pico, r.cache_read_pico),
        (100_000, 500_000, 10_000)
    );
    assert_eq!(r.batch_discount_ppm, 500_000);
    let lc = r.long_context.clone().unwrap();
    assert_eq!(
        (
            lc.threshold_input_tokens,
            lc.input_factor_ppm,
            lc.output_factor_ppm
        ),
        (272_000, 2_000_000, 1_500_000)
    );
    let eu = table
        .get(&key(Endpoint::Responses))
        .unwrap()
        .rates(Some("eu"))
        .unwrap();
    assert_eq!(eu.regional_uplift_ppm, 100_000);
    let d = table
        .get(&key(Endpoint::Decisions))
        .unwrap()
        .rates(None)
        .unwrap();
    assert_eq!(
        (
            d.input_pico,
            d.output_pico,
            d.cache_read_pico,
            d.cache_write_pico
        ),
        (100_000, 0, 10_000, 125_000)
    );
    // AI-010, Nachtrag 25.11.2026 „Preise der Decisions API“: Cache und Langkontext wie `responses`, kein Batch.
    assert_eq!(d.long_context, r.long_context);
    assert_eq!(d.batch_discount_ppm, 0);
}

// spec: AI-011
// `task-defaults.json` validiert (Schema): jede Aufgabe außer `embedding` hat `reasoning` aus der Stufenmenge und
// `answer_tokens` > 0 bzw. für `decision`, `decision-vision`, `redirect-confirm` die Formel-Kennung `decision-w`; die
// ausgelieferten Standardwerte entsprechen der Tabelle aus AI-011 (Nachtrag).
#[test]
fn bundled_task_defaults_match_the_spec_table() {
    use ReasoningLevel::{Low, Medium, Off};
    let defaults = TaskDefaults::bundled().unwrap();
    let w = Some(AnswerTokens::Formula(AnswerFormula::DecisionW));
    let fixed = |n| Some(AnswerTokens::Fixed(n));
    let table = [
        (TaskType::Decision, Some(Off), w),
        (TaskType::DecisionVision, Some(Off), w),
        (TaskType::RedirectConfirm, Some(Off), w),
        (TaskType::SuggestTitle, Some(Off), fixed(512)),
        (TaskType::SuggestMeta, Some(Off), fixed(512)),
        (TaskType::SuggestH1, Some(Off), fixed(512)),
        (TaskType::SuggestAlt, Some(Off), fixed(512)),
        (TaskType::LinkAnchor, Some(Off), fixed(512)),
        (TaskType::LlmsSummary, Some(Low), fixed(2_048)),
        (TaskType::AskSql, Some(Medium), fixed(1_024)),
        (TaskType::AuditSummary, Some(Medium), fixed(8_192)),
        (TaskType::Embedding, None, None),
    ];
    assert_eq!(defaults.tasks.len(), TaskType::ALL.len());
    for (task, reasoning, answer) in table {
        let d = defaults.get(task);
        assert_eq!(d.reasoning, reasoning, "{task}");
        assert_eq!(d.answer_tokens, answer, "{task}");
    }
    let raw: Value =
        serde_json::from_str(&read(&["resources", "ai", "task-defaults.json"])).unwrap();
    assert_eq!(validate_task_defaults(&raw), Ok(()));
    assert_eq!(raw["tasks"]["decision"]["answer_tokens"], "decision-w");
}

// spec: AI-011
// E-4 b / K4-20: Bis zur Bestätigung durch den Nutzer enthält die Datei keine Standardmodelle (kein erfundener Wert).
// Jedes eingetragene Standard- oder Ersatzmodell muss in `pricing.json` für den Endpunkt der Aufgabe existieren oder
// ein OpenRouter- bzw. Ollama-Modell sein.
#[test]
fn bundled_task_defaults_reference_only_priced_models() {
    let defaults = TaskDefaults::bundled().unwrap();
    let pricing = PriceTable::bundled().unwrap();
    assert_eq!(
        defaults.check_against_pricing(&pricing),
        Vec::<String>::new()
    );
    if defaults.models_status == ModelsStatus::PendingConfirmation {
        for (task, d) in &defaults.tasks {
            assert!(
                d.model.is_none() && d.fallback.is_empty(),
                "{task}: Vorschlag ausstehend"
            );
        }
    }
}

// spec: AI-011
// Konsistenzprüfung gegen Testdaten: bestätigte Standardmodelle existieren für ihren Endpunkt; ein Modell ohne
// Eintrag für den Endpunkt der Aufgabe (z. B. Decisions-Modell über `responses` bepreist) fällt auf.
#[test]
fn task_defaults_check_against_pricing_finds_unpriced_models() {
    let pricing = PriceTable::from_json_str(&read(&[
        "tests",
        "fixtures",
        "ai",
        "k04",
        "pricing-catalog.json",
    ]))
    .unwrap();
    let text = read(&["tests", "fixtures", "ai", "k04", "task-defaults-test.json"]);
    let defaults = TaskDefaults::from_json_str(&text).unwrap();
    assert_eq!(defaults.models_status, ModelsStatus::Confirmed);
    assert_eq!(
        defaults.check_against_pricing(&pricing),
        Vec::<String>::new()
    );
    let mut doc: Value = serde_json::from_str(&text).unwrap();
    doc["tasks"]["decision"]["model"]["model"] = "mock-plain".into();
    doc["tasks"]["suggest-alt"]["model"] = serde_json::json!({"provider": "openai", "model": "mock-text-only", "endpoint": "responses"});
    let broken = TaskDefaults::from_json_str(&doc.to_string()).unwrap();
    let errors = broken.check_against_pricing(&pricing);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors
        .iter()
        .any(|e| e.contains("decision[0]") && e.contains("fehlt")));
    assert!(errors
        .iter()
        .any(|e| e.contains("suggest-alt[0]") && e.contains("Fähigkeit")));
}

// spec: AI-011
// Ungültige Varianten werden vom Schematest abgelehnt.
#[test]
fn task_defaults_schema_rejects_invalid_files() {
    let dir = repo(&["tests", "fixtures", "ai", "k04", "invalid"]);
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("task-defaults-"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 10, "{names:?}");
    for name in names {
        let text = std::fs::read_to_string(dir.join(&name)).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert!(
            validate_task_defaults(&doc).is_err(),
            "{name} gilt fälschlich als gültig"
        );
        assert!(TaskDefaults::from_json_str(&text).is_err(), "{name}");
    }
}

// spec: AI-012
// `provider-limits.json`: `decisions_question_overhead_tokens` = 100 und `schema_overhead_tokens` (Anthropic 350, OpenAI
// Responses 80, OpenRouter `openai/*` 80, sonst 0), je mit Quelle „Live-Probe Q00“ und `verified_at` 2026-11-24.
#[test]
fn bundled_provider_limits_carry_the_q00_values() {
    let limits = ProviderLimitsFile::bundled().unwrap();
    assert_eq!(limits.decisions_question_overhead(), 100);
    assert_eq!(
        limits.schema_overhead("anthropic", Endpoint::Messages, "claude-haiku-5-5"),
        350
    );
    assert_eq!(
        limits.schema_overhead("openai", Endpoint::Responses, "gpt-6-luna"),
        80
    );
    assert_eq!(
        limits.schema_overhead("openai", Endpoint::Decisions, "gpt-6-luna"),
        0
    );
    assert_eq!(
        limits.schema_overhead("openrouter", Endpoint::ChatCompletions, "openai/gpt-6-luna"),
        80
    );
    assert_eq!(
        limits.schema_overhead("openrouter", Endpoint::ChatCompletions, "anthropic/claude"),
        0
    );
    assert_eq!(
        limits.schema_overhead("ollama", Endpoint::ChatCompletions, "llama"),
        0
    );
    for (source, date) in [
        (
            &limits.decisions_question_overhead_tokens.source_url,
            &limits.decisions_question_overhead_tokens.verified_at,
        ),
        (
            &limits.schema_overhead_tokens.source_url,
            &limits.schema_overhead_tokens.verified_at,
        ),
    ] {
        assert!(
            source.contains("Live-Probe Q00") && source.contains("docs/plan/M4-reports/q00.md")
        );
        assert_eq!(date, "2026-11-24");
    }
    assert!(ProviderLimitsFile::from_json_str(
        r#"{"decisions_question_overhead_tokens":{"value":100}}"#
    )
    .is_err());
}

// spec: AI-011
// Vorbereitung E-4 b / K4-20: Das Eintragen der bestätigten Standardmodelle ist eine reine Datenänderung. Eine Liste in
// der späteren Form (Standard plus Ersatzliste je Aufgabe) in die ausgelieferte Datei übernommen besteht Schema und
// Regeln, ändert `reasoning`/`answer_tokens` nicht, und genau das Modell ohne Preiseintrag fällt in der Prüfung gegen
// `pricing.json` auf (OpenRouter und Ollama sind ausgenommen).
#[test]
fn filling_in_confirmed_models_is_a_pure_data_change() {
    let filled: Value = serde_json::from_str(&read(&[
        "tests",
        "fixtures",
        "ai",
        "k04",
        "task-defaults-filled.json",
    ]))
    .unwrap();
    // Ausgangspunkt: Datei ohne Standardmodelle (Stand vor K4-20, Fixture); die ausgelieferte Datei ist bestätigt.
    let mut doc: Value = serde_json::from_str(&read(&[
        "tests",
        "fixtures",
        "ai",
        "k04",
        "task-defaults-placeholder.json",
    ]))
    .unwrap();
    // Ohne Modelle darf der Status nicht `confirmed` sein.
    let mut premature = doc.clone();
    premature["models_status"] = "confirmed".into();
    let errors = validate_task_defaults(&premature).unwrap_err();
    assert_eq!(errors.len(), TaskType::ALL.len(), "{errors:?}");
    for task in TaskType::ALL {
        let t = task.as_str();
        assert!(!filled["tasks"][t]["model"].is_null(), "{t}");
        assert!(
            !filled["tasks"][t]["fallback"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{t}"
        );
        doc["tasks"][t]["model"] = filled["tasks"][t]["model"].clone();
        doc["tasks"][t]["fallback"] = filled["tasks"][t]["fallback"].clone();
    }
    doc["models_status"] = "confirmed".into();
    assert_eq!(validate_task_defaults(&doc), Ok(()));
    let confirmed = TaskDefaults::from_json_str(&doc.to_string()).unwrap();
    let bundled = TaskDefaults::bundled().unwrap();
    for task in TaskType::ALL {
        assert_eq!(confirmed.get(task).reasoning, bundled.get(task).reasoning);
        assert_eq!(
            confirmed.get(task).answer_tokens,
            bundled.get(task).answer_tokens
        );
    }
    let pricing = PriceTable::from_json_str(&read(&[
        "tests",
        "fixtures",
        "ai",
        "k04",
        "pricing-catalog.json",
    ]))
    .unwrap();
    let errors = confirmed.check_against_pricing(&pricing);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("ask-sql[2]") && errors[0].contains("openai/mock-unpriced/responses"),
        "{errors:?}"
    );
    // Die Fixture selbst lädt ebenso.
    let fixture = TaskDefaults::from_json_str(&filled.to_string()).unwrap();
    assert_eq!(fixture.check_against_pricing(&pricing), errors);
}

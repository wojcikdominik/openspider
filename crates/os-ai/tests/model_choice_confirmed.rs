//! Bestätigte Standardmodelle (AI-011, Nachtrag 25.11.2026, E-4 b / K4-20) in `resources/ai/task-defaults.json`, die
//! Preise dazu in `resources/ai/pricing.json` (AI-010) und ihr Prüfdatum (AI-025). Die Modelllisten der Anbieter
//! werden für die Modellwahl aus Testdaten aufgebaut (Fähigkeiten aus dem Katalog bzw. nach AI-002/AI-005); es gibt
//! keinen Request an einen Anbieter.
use std::collections::BTreeMap;

use os_ai::catalog::{
    plan_task, AvailableModel, CivilDate, Endpoint, ModelRef, PriceCatalog, PriceKey, PriceTable,
    ProviderState, TaskDefaults, TaskSettings, TaskType,
};
use os_ai::cost::rates::usd_per_million_to_pico;
use os_ai::provider::{Capabilities, CapabilitySource, Model, ReasoningLevel};
use os_model::ai::AiStatus;

const DECISIONS: Endpoint = Endpoint::Decisions;
const RESPONSES: Endpoint = Endpoint::Responses;
const MESSAGES: Endpoint = Endpoint::Messages;

fn r(provider: &str, endpoint: Endpoint, model: &str) -> ModelRef {
    ModelRef::new(provider, model, endpoint)
}

fn luna_decisions() -> ModelRef {
    r("openai", DECISIONS, "gpt-6-luna")
}
fn luna() -> ModelRef {
    r("openai", RESPONSES, "gpt-6-luna")
}
fn sol() -> ModelRef {
    r("openai", RESPONSES, "gpt-6-sol")
}
fn haiku() -> ModelRef {
    r("anthropic", MESSAGES, "claude-haiku-5-5")
}
fn sonnet() -> ModelRef {
    r("anthropic", MESSAGES, "claude-sonnet-5-5")
}
fn openrouter_luna() -> ModelRef {
    r("openrouter", Endpoint::ChatCompletions, "openai/gpt-6-luna")
}
fn embedding() -> ModelRef {
    r("openai", Endpoint::Embeddings, "text-embedding-3-small")
}

/// Tabelle aus AI-011, Nachtrag 25.11.2026: Aufgabe → (Standard, Ersatzliste in Reihenfolge).
fn confirmed_table() -> Vec<(TaskType, ModelRef, Vec<ModelRef>)> {
    use TaskType::*;
    let mut rows = Vec::new();
    for task in [Decision, DecisionVision] {
        rows.push((task, luna_decisions(), vec![haiku(), openrouter_luna()]));
    }
    for task in [
        SuggestTitle,
        SuggestMeta,
        SuggestH1,
        LinkAnchor,
        RedirectConfirm,
        SuggestAlt,
    ] {
        rows.push((task, luna(), vec![haiku()]));
    }
    for task in [AskSql, AuditSummary, LlmsSummary] {
        rows.push((task, sonnet(), vec![sol(), luna()]));
    }
    rows.push((Embedding, embedding(), vec![]));
    rows
}

fn today() -> CivilDate {
    CivilDate::parse("2026-11-25").unwrap()
}

// spec: AI-011
// Die ausgelieferte `task-defaults.json` entspricht der Tabelle (Standard und Reihenfolge der Ersatzliste je Aufgabe);
// Stufe und Antwortanteil bleiben unverändert (eigener Test in `catalog_resources.rs`).
#[test]
fn bundled_task_defaults_match_the_confirmed_table() {
    let defaults = TaskDefaults::bundled().unwrap();
    assert_eq!(
        serde_json::to_value(defaults.models_status).unwrap(),
        "confirmed"
    );
    assert_eq!(defaults.verified_at, "2026-11-25");
    let table = confirmed_table();
    assert_eq!(table.len(), TaskType::ALL.len());
    for (task, model, fallback) in table {
        let d = defaults.get(task);
        assert_eq!(d.model.as_ref(), Some(&model), "{task}");
        assert_eq!(d.fallback, fallback, "{task}");
    }
}

// spec: AI-011
// spec: AI-010
// Jedes genannte Modell hat in `pricing.json` einen Eintrag für den genannten Endpunkt mit `source_url` und
// `verified_at`, oder ist ein OpenRouter-Modell (Preis aus der Modellliste bzw. `usage.cost`).
#[test]
fn every_listed_model_is_priced_for_its_endpoint_or_marked_openrouter() {
    let defaults = TaskDefaults::bundled().unwrap();
    let pricing = PriceTable::bundled().unwrap();
    assert_eq!(
        defaults.check_against_pricing(&pricing),
        Vec::<String>::new()
    );
    let mut seen = 0;
    for (task, d) in &defaults.tasks {
        for m in d.model.iter().chain(d.fallback.iter()) {
            seen += 1;
            if m.provider == "openrouter" {
                let key = PriceKey::new(&m.provider, &m.model, m.endpoint);
                assert!(
                    pricing.get(&key).is_none(),
                    "{task}: OpenRouter ohne Tabelleneintrag"
                );
                continue;
            }
            let entry = pricing
                .get(&PriceKey::new(&m.provider, &m.model, m.endpoint))
                .unwrap_or_else(|| panic!("{task}: {m:?} ohne Preis"));
            assert!(entry.source_url.starts_with("https://"), "{entry:?}");
            assert_eq!(entry.verified_at, "2026-11-25", "{entry:?}");
        }
    }
    assert_eq!(seen, 2 * 3 + 6 * 2 + 3 * 3 + 1);
}

/// Erwartete Preise in USD je 1 Mio. Tokens (Eingabe, Ausgabe, Cache-Lesen, Cache-Schreiben), Batch-Rabatt in ppm.
struct Expected {
    key: PriceKey,
    input: &'static str,
    output: &'static str,
    cache_read: &'static str,
    cache_write: &'static str,
    batch_ppm: u64,
    /// (Schwelle, Eingabe-, Ausgabe-, Cache-Lese-Faktor in ppm).
    long_context: Option<(u64, u64, u64, u64)>,
    eu_uplift_ppm: u64,
}

// spec: AI-010
// Werte der Tabelle je (Anbieter, Modell, Endpunkt) wie auf den Preisseiten der Anbieter (abgerufen am 25.11.2026,
// Quellen im Bericht K4-20); Langkontext-Preise als Faktoren auf die Grundpreise.
#[test]
fn bundled_prices_match_the_documented_values() {
    let k = |p, m, e| PriceKey::new(p, m, e);
    let expected = [
        Expected {
            key: k("openai", "gpt-6-luna", RESPONSES),
            input: "0.10",
            output: "0.50",
            cache_read: "0.01",
            cache_write: "0.125",
            batch_ppm: 500_000,
            long_context: Some((272_000, 2_000_000, 1_500_000, 2_000_000)),
            eu_uplift_ppm: 100_000,
        },
        Expected {
            key: k("openai", "gpt-6-luna", DECISIONS),
            input: "0.10",
            output: "0",
            cache_read: "0",
            cache_write: "0",
            batch_ppm: 0,
            long_context: None,
            eu_uplift_ppm: 100_000,
        },
        Expected {
            key: k("openai", "gpt-6-sol", RESPONSES),
            input: "2.00",
            output: "10.00",
            cache_read: "0.20",
            cache_write: "2.50",
            batch_ppm: 500_000,
            long_context: Some((272_000, 2_000_000, 1_500_000, 2_000_000)),
            eu_uplift_ppm: 100_000,
        },
        Expected {
            key: k("openai", "text-embedding-3-small", Endpoint::Embeddings),
            input: "0.02",
            output: "0",
            cache_read: "0",
            cache_write: "0",
            batch_ppm: 0,
            long_context: None,
            eu_uplift_ppm: 0,
        },
        Expected {
            key: k("anthropic", "claude-haiku-5-5", MESSAGES),
            input: "0.10",
            output: "0.50",
            cache_read: "0.01",
            cache_write: "0.125",
            batch_ppm: 500_000,
            long_context: Some((100_001, 5_000_000, 5_000_000, 5_000_000)),
            eu_uplift_ppm: 0,
        },
        Expected {
            key: k("anthropic", "claude-sonnet-5-5", MESSAGES),
            input: "2",
            output: "10",
            cache_read: "0.10",
            cache_write: "2.50",
            batch_ppm: 500_000,
            long_context: None,
            eu_uplift_ppm: 0,
        },
    ];
    let table = PriceTable::bundled().unwrap();
    assert_eq!(table.entries().len(), expected.len());
    let pico = |t: &str| usd_per_million_to_pico(t).unwrap();
    for e in expected {
        let entry = table
            .get(&e.key)
            .unwrap_or_else(|| panic!("{:?} fehlt", e.key));
        let rates = entry.rates(None).unwrap();
        assert_eq!(rates.input_pico, pico(e.input), "{:?}", e.key);
        assert_eq!(rates.output_pico, pico(e.output), "{:?}", e.key);
        assert_eq!(rates.cache_read_pico, pico(e.cache_read), "{:?}", e.key);
        assert_eq!(rates.cache_write_pico, pico(e.cache_write), "{:?}", e.key);
        assert_eq!(rates.batch_discount_ppm, e.batch_ppm, "{:?}", e.key);
        assert_eq!(
            rates.long_context.map(|lc| (
                lc.threshold_input_tokens,
                lc.input_factor_ppm,
                lc.output_factor_ppm,
                lc.cache_read_factor_ppm
            )),
            e.long_context,
            "{:?}",
            e.key
        );
        assert_eq!(
            entry.rates(Some("eu")).unwrap().regional_uplift_ppm,
            e.eu_uplift_ppm,
            "{:?}",
            e.key
        );
    }
    // Stufen und Ausgabegrenze der OpenAI-Modelle laut Modellseiten.
    for model in ["gpt-6-luna", "gpt-6-sol"] {
        let entry = table.get(&k("openai", model, RESPONSES)).unwrap();
        assert_eq!(
            entry.reasoning_levels(),
            vec![
                ReasoningLevel::Off,
                ReasoningLevel::Low,
                ReasoningLevel::Medium,
                ReasoningLevel::High,
                ReasoningLevel::XHigh,
                ReasoningLevel::Max
            ],
            "{model}"
        );
        assert_eq!(entry.max_output_tokens, Some(128_000), "{model}");
    }
}

// spec: AI-025
// Release-Check-Regel auf die ausgelieferten Daten: Kein Standardmodell und kein verwendeter Preiseintrag hat am
// Prüftag ein Prüfdatum älter als 120 Tage; die Preise gelten nicht als „möglicherweise veraltet“ (AI-010).
#[test]
fn confirmed_defaults_and_prices_are_freshly_verified() {
    let defaults = TaskDefaults::bundled().unwrap();
    let verified = CivilDate::parse(&defaults.verified_at).unwrap();
    assert!(today().days_since(verified) <= 120);
    let catalog = PriceCatalog::new(PriceTable::bundled().unwrap());
    for (task, d) in &defaults.tasks {
        for m in d.model.iter().chain(d.fallback.iter()) {
            if m.provider == "openrouter" {
                continue;
            }
            let price = catalog
                .resolve(&PriceKey::new(&m.provider, &m.model, m.endpoint), None)
                .unwrap()
                .unwrap_or_else(|| panic!("{task}: {m:?}"));
            assert!(!price.possibly_stale(today()), "{task}: {m:?}");
            assert!(today().days_since(price.verified_at.unwrap()) <= 120);
            // Grenze des Release-Checks: 121 Tage nach dem Prüfdatum gilt der Preis als veraltet.
            let limit = CivilDate::parse("2027-03-26").unwrap();
            assert_eq!(limit.days_since(price.verified_at.unwrap()), 121);
            assert!(price.possibly_stale(limit));
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Modellwahl mit der bestätigten Liste

fn model(m: &ModelRef, capabilities: Capabilities, source: CapabilitySource) -> AvailableModel {
    AvailableModel {
        model: Model {
            provider: m.provider.clone(),
            id: m.model.clone(),
            endpoint: m.endpoint.as_str().to_string(),
            capabilities,
            capability_source: source,
            reasoning_levels: Vec::new(),
        },
        max_output_tokens: None,
    }
}

fn chat_caps() -> Capabilities {
    Capabilities {
        text: true,
        structured_output: true,
        vision: true,
        ..Capabilities::default()
    }
}

/// Live-Listen der drei Anbieter mit den Modellen der Tabelle; OpenAI-Fähigkeiten aus dem Katalog.
fn provider(name: &str, has_key: bool) -> ProviderState {
    if !has_key {
        return ProviderState::default();
    }
    let table = PriceTable::bundled().unwrap();
    let models = match name {
        "openai" => [luna_decisions(), luna(), sol(), embedding()]
            .iter()
            .map(|m| {
                let caps = table.capability_lookup("openai", m.endpoint)(m.model.as_str()).unwrap();
                model(m, caps, CapabilitySource::ModelCatalog)
            })
            .collect(),
        "anthropic" => [haiku(), sonnet()]
            .iter()
            .map(|m| model(m, chat_caps(), CapabilitySource::AnthropicModels))
            .collect(),
        "openrouter" => vec![model(
            &openrouter_luna(),
            chat_caps(),
            CapabilitySource::OpenRouterModels,
        )],
        other => panic!("{other}"),
    };
    ProviderState { has_key, models }
}

fn only(key: &str) -> BTreeMap<String, ProviderState> {
    ["anthropic", "openai", "openrouter"]
        .into_iter()
        .map(|p| (p.to_string(), provider(p, p == key)))
        .collect()
}

fn chosen(
    task: TaskType,
    providers: &BTreeMap<String, ProviderState>,
) -> Result<ModelRef, AiStatus> {
    let defaults = TaskDefaults::bundled().unwrap();
    plan_task(task, &TaskSettings::default(), &defaults, providers).map(|p| p.model_ref)
}

// spec: AI-011
// Nur ein OpenAI-Schlüssel, kein Modell im Profil → `decision` über Decisions `gpt-6-luna`, `suggest-title` über
// Responses `gpt-6-luna`, `ask-sql` über Responses `gpt-6-sol` (erstes verfügbares Modell der Ersatzliste),
// `embedding` über `text-embedding-3-small`.
#[test]
fn only_openai_key_uses_openai_models_of_the_list() {
    let p = only("openai");
    assert_eq!(chosen(TaskType::Decision, &p), Ok(luna_decisions()));
    assert_eq!(chosen(TaskType::SuggestTitle, &p), Ok(luna()));
    assert_eq!(chosen(TaskType::AskSql, &p), Ok(sol()));
    assert_eq!(chosen(TaskType::Embedding, &p), Ok(embedding()));
}

// spec: AI-011
// Nur ein Anthropic-Schlüssel → `decision` und `suggest-title` über `claude-haiku-5-5`, `ask-sql` über
// `claude-sonnet-5-5`; `embedding` erhält keinen Plan (Statuswert siehe Klärung im ignorierten Test unten).
#[test]
fn only_anthropic_key_uses_haiku_and_sonnet() {
    let p = only("anthropic");
    assert_eq!(chosen(TaskType::Decision, &p), Ok(haiku()));
    assert_eq!(chosen(TaskType::SuggestTitle, &p), Ok(haiku()));
    assert_eq!(chosen(TaskType::AskSql, &p), Ok(sonnet()));
    assert!(chosen(TaskType::Embedding, &p).is_err());
}

// spec: AI-011
// Nur ein OpenRouter-Schlüssel → `decision` über `openai/gpt-6-luna`; `suggest-title`, `ask-sql` und `embedding`
// erhalten keinen Plan (Statuswert siehe Klärung im ignorierten Test unten).
#[test]
fn only_openrouter_key_runs_decisions_over_openrouter() {
    let p = only("openrouter");
    assert_eq!(chosen(TaskType::Decision, &p), Ok(openrouter_luna()));
    for task in [
        TaskType::SuggestTitle,
        TaskType::AskSql,
        TaskType::Embedding,
    ] {
        assert!(chosen(task, &p).is_err(), "{task}");
    }
}

// spec: AI-011
// Statuswert der Aufgaben ohne verfügbaren Kandidaten aus den drei Kriterien des Nachtrags „bestätigte
// Standardmodelle“: dort `Skipped (no capable model)`, obwohl keiner der Kandidaten-Anbieter einen Schlüssel hat. Der
// Nachtrag „Aufgabe ohne Kandidaten“ vom selben Tag verlangt für Aufgaben mit Kandidaten in diesem Fall
// `Error: Missing API key` (so gebaut). Widerspruch gemeldet im Bericht K4-20.
#[test]
#[ignore = "Klärung K4-20: Skipped (no capable model) gegen Error: Missing API key bei Aufgabe mit Kandidaten ohne Schlüssel (AI-011, Nachträge 25.11.2026)"]
fn tasks_without_available_candidate_are_skipped_per_confirmed_list_criteria() {
    let anthropic = only("anthropic");
    assert_eq!(
        chosen(TaskType::Embedding, &anthropic),
        Err(AiStatus::SkippedNoCapableModel)
    );
    let openrouter = only("openrouter");
    for task in [
        TaskType::SuggestTitle,
        TaskType::AskSql,
        TaskType::Embedding,
    ] {
        assert_eq!(
            chosen(task, &openrouter),
            Err(AiStatus::SkippedNoCapableModel),
            "{task}"
        );
    }
}

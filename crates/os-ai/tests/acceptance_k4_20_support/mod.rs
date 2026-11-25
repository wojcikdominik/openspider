//! Gemeinsame Hilfen der unabhängigen Abnahmetests K4-20 (nur aus `docs/spec/08-ai.md` abgeleitet).
// acceptance: independent
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use os_ai::adapters::anthropic::AnthropicModel;
use os_ai::adapters::openrouter::OpenRouterModel;
use os_ai::catalog::reasoning::{
    anthropic_levels, anthropic_output_limit, openrouter_levels, openrouter_output_limit,
};
use os_ai::catalog::{AvailableModel, Endpoint, PriceKey, PriceTable, ProviderState};
use os_ai::provider::{CapabilitySource, Model};
use serde_json::Value;

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn fixture_text(name: &str) -> String {
    let path = repo_root()
        .join("tests/fixtures/ai/k4-20-acceptance")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn fixture_json(name: &str) -> Value {
    serde_json::from_str(&fixture_text(name)).expect("Fixture ist JSON")
}

pub fn resource_json(name: &str) -> Value {
    let path = repo_root().join("resources/ai").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("Ressource ist JSON")
}

fn entries(file: &str) -> Vec<Value> {
    fixture_json(file)["data"].as_array().unwrap().clone()
}

/// Anthropic-Liveliste (Fixture) als verfügbare Modelle.
pub fn anthropic_state() -> ProviderState {
    ProviderState {
        has_key: true,
        models: entries("anthropic-models.json")
            .iter()
            .map(|e| {
                let mut model = AnthropicModel::from_json(e).unwrap().to_model();
                model.reasoning_levels = anthropic_levels(e);
                AvailableModel {
                    model,
                    max_output_tokens: anthropic_output_limit(e),
                }
            })
            .collect(),
    }
}

pub fn openrouter_state() -> ProviderState {
    ProviderState {
        has_key: true,
        models: entries("openrouter-models.json")
            .iter()
            .map(|e| {
                let mut model = OpenRouterModel::from_json(e).unwrap().to_model();
                model.reasoning_levels = openrouter_levels(e);
                AvailableModel {
                    model,
                    max_output_tokens: openrouter_output_limit(e),
                }
            })
            .collect(),
    }
}

/// OpenAI-Modelle: alle OpenAI-Einträge der ausgelieferten Preistabelle; Fähigkeiten, Stufen und Ausgabegrenze aus
/// dem Katalog (AI-010: OpenAI meldet keine Fähigkeiten).
pub fn openai_state(table: &PriceTable) -> ProviderState {
    let models = table
        .entries()
        .iter()
        .filter(|e| e.provider == "openai")
        .map(|e| {
            let endpoint: Endpoint = e.endpoint;
            AvailableModel {
                model: Model {
                    provider: "openai".into(),
                    id: e.model.clone(),
                    endpoint: endpoint.as_str().into(),
                    capabilities: table.capability_lookup("openai", endpoint)(&e.model)
                        .unwrap_or_default(),
                    capability_source: CapabilitySource::ModelCatalog,
                    reasoning_levels: table.reasoning_lookup("openai", endpoint)(&e.model),
                },
                max_output_tokens: table
                    .max_output_tokens(&PriceKey::new("openai", &e.model, endpoint)),
            }
        })
        .collect();
    ProviderState {
        has_key: true,
        models,
    }
}

/// Anbieterzustand: nur die genannten Anbieter haben einen Schlüssel und eine geladene Liste.
pub fn providers(with_keys: &[&str]) -> BTreeMap<String, ProviderState> {
    let table = PriceTable::bundled().expect("ausgelieferte Preistabelle");
    let mut out = BTreeMap::new();
    for (name, state) in [
        ("anthropic", anthropic_state()),
        ("openai", openai_state(&table)),
        ("openrouter", openrouter_state()),
    ] {
        let state = if with_keys.contains(&name) {
            state
        } else {
            ProviderState::default()
        };
        out.insert(name.to_string(), state);
    }
    out
}

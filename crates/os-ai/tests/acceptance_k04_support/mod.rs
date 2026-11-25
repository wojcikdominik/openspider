//! Gemeinsame Hilfen der unabhängigen Abnahmetests M4-K04 (nur aus `docs/spec/*` abgeleitet).
// acceptance: independent
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use os_ai::adapters::anthropic::AnthropicModel;
use os_ai::adapters::openrouter::OpenRouterModel;
use os_ai::catalog::reasoning::{
    anthropic_levels, anthropic_output_limit, openrouter_levels, openrouter_output_limit,
};
use os_ai::catalog::{AvailableModel, Endpoint, PriceKey, PriceTable, ProviderState};
use os_ai::provider::{CapabilitySource, Model};
use os_fetch::secrets::{SecretBackend, SecretError, SecretStore, SecretStoreMut};
use serde_json::Value;

/// Testschlüssel nach AI-009 (Dateisystem-Scan nach `sk-TEST-AI-009`).
pub const TEST_KEY: &str = "sk-TEST-AI-009-acceptance-k04-0123456789-wxyz";

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn fixture_dir() -> PathBuf {
    repo_root().join("tests/fixtures/ai/k04-acceptance")
}

pub fn fixture_text(name: &str) -> String {
    let path = fixture_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn fixture_json(name: &str) -> Value {
    serde_json::from_str(&fixture_text(name)).expect("Fixture ist JSON")
}

pub fn resource_text(name: &str) -> String {
    let path = repo_root().join("resources/ai").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn resource_json(name: &str) -> Value {
    serde_json::from_str(&resource_text(name)).expect("Ressource ist JSON")
}

/// Ein Eintrag der Anthropic-Fixture-Liste (`mock-claude`, `mock-claude-b`, …).
pub fn anthropic_entry(id: &str) -> Value {
    fixture_json("anthropic-models.json")["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("Anthropic-Modell {id} fehlt in der Fixture"))
}

pub fn openrouter_entry(id: &str) -> Value {
    fixture_json("openrouter-models.json")["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("OpenRouter-Modell {id} fehlt in der Fixture"))
}

/// Anthropic-Modell aus der Modellliste: Fähigkeiten laut Adapter (AI-002), Stufen und Ausgabegrenze laut Katalog
/// (AI-011, Nachtrag).
pub fn anthropic_model(id: &str) -> AvailableModel {
    let entry = anthropic_entry(id);
    let mut model = AnthropicModel::from_json(&entry).unwrap().to_model();
    model.reasoning_levels = anthropic_levels(&entry);
    AvailableModel {
        model,
        max_output_tokens: anthropic_output_limit(&entry),
    }
}

pub fn openrouter_model(id: &str) -> AvailableModel {
    let entry = openrouter_entry(id);
    let mut model = OpenRouterModel::from_json(&entry).unwrap().to_model();
    model.reasoning_levels = openrouter_levels(&entry);
    AvailableModel {
        model,
        max_output_tokens: openrouter_output_limit(&entry),
    }
}

pub fn acceptance_pricing() -> PriceTable {
    PriceTable::from_json_str(&fixture_text("pricing-acceptance.json"))
        .expect("Abnahme-Preisdatei gültig")
}

/// OpenAI-Katalogmodell (Fähigkeiten, Stufen und Ausgabegrenze aus der Preistabelle, AI-010).
pub fn openai_model(table: &PriceTable, id: &str, endpoint: Endpoint) -> AvailableModel {
    let caps = table.capability_lookup("openai", endpoint)(id).unwrap_or_default();
    let levels = table.reasoning_lookup("openai", endpoint)(id);
    AvailableModel {
        model: Model {
            provider: "openai".into(),
            id: id.into(),
            endpoint: endpoint.as_str().into(),
            capabilities: caps,
            capability_source: CapabilitySource::ModelCatalog,
            reasoning_levels: levels,
        },
        max_output_tokens: table.max_output_tokens(&PriceKey::new("openai", id, endpoint)),
    }
}

pub fn all_anthropic() -> ProviderState {
    ProviderState {
        has_key: true,
        models: [
            "mock-claude",
            "mock-claude-b",
            "mock-claude-c",
            "mock-claude-d",
        ]
        .into_iter()
        .map(anthropic_model)
        .collect(),
    }
}

pub fn all_openai(table: &PriceTable) -> ProviderState {
    ProviderState {
        has_key: true,
        models: vec![
            openai_model(table, "gpt-6-luna", Endpoint::Responses),
            openai_model(table, "gpt-6-luna", Endpoint::Decisions),
            openai_model(table, "acc-no-effort", Endpoint::Responses),
            openai_model(table, "acc-small-limit", Endpoint::Responses),
            openai_model(table, "acc-embed", Endpoint::Embeddings),
        ],
    }
}

pub fn all_openrouter() -> ProviderState {
    ProviderState {
        has_key: true,
        models: ["acc/m-r1", "acc/m-r2", "acc/m-r3", "acc/m-4000"]
            .into_iter()
            .map(openrouter_model)
            .collect(),
    }
}

/// Anbieterzustand mit den angegebenen Schlüsseln (`anthropic`, `openai`, `openrouter`). Anbieter ohne Schlüssel
/// erscheinen ohne Modellliste (nicht verbunden).
pub fn providers(with_keys: &[&str]) -> BTreeMap<String, ProviderState> {
    let table = acceptance_pricing();
    let mut out = BTreeMap::new();
    for (name, state) in [
        ("anthropic", all_anthropic()),
        ("openai", all_openai(&table)),
        ("openrouter", all_openrouter()),
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

// ---------------------------------------------------------------------------------------------------------------
// Test-Implementierung des Schlüsselspeichers (Mock-Keychain-Backend, INTEG-002): protokolliert alle Zugriffe und
// kann gesperrt bzw. nicht verfügbar sein.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreCall {
    Get(String),
    Set(String, String),
    Delete(String),
}

#[derive(Default)]
pub struct RecordingStore {
    values: Mutex<HashMap<String, String>>,
    calls: Mutex<Vec<StoreCall>>,
    pub fail_reads: Mutex<bool>,
    pub fail_writes: Mutex<bool>,
}

impl RecordingStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Schlüsselbund gesperrt bzw. nicht verfügbar: Lesen und Schreiben scheitern.
    pub fn locked() -> Arc<Self> {
        let s = Self::default();
        *s.fail_reads.lock().unwrap() = true;
        *s.fail_writes.lock().unwrap() = true;
        Arc::new(s)
    }

    pub fn set_locked(&self, locked: bool) {
        *self.fail_reads.lock().unwrap() = locked;
        *self.fail_writes.lock().unwrap() = locked;
    }

    /// Direkt einen Eintrag ablegen (z. B. wie ihn ein anderer Bereich, 04, hinterlegt hätte).
    pub fn put(&self, account: &str, value: &str) {
        self.values
            .lock()
            .unwrap()
            .insert(account.to_string(), value.to_string());
    }

    pub fn entries(&self) -> BTreeMap<String, String> {
        self.values
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn calls(&self) -> Vec<StoreCall> {
        self.calls.lock().unwrap().clone()
    }

    pub fn sets(&self) -> Vec<(String, String)> {
        self.calls()
            .into_iter()
            .filter_map(|c| match c {
                StoreCall::Set(a, v) => Some((a, v)),
                _ => None,
            })
            .collect()
    }
}

impl std::fmt::Debug for RecordingStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecordingStore")
    }
}

impl SecretStore for RecordingStore {
    fn as_mut_store(&self) -> Option<&dyn SecretStoreMut> {
        Some(self)
    }
    fn get(&self, id: &str) -> Option<String> {
        self.try_get(id).ok().flatten()
    }
    fn try_get(&self, id: &str) -> Result<Option<String>, SecretError> {
        self.calls
            .lock()
            .unwrap()
            .push(StoreCall::Get(id.to_string()));
        if *self.fail_reads.lock().unwrap() {
            return Err(SecretError::new("Schlüsselbund gesperrt (Test)"));
        }
        Ok(self.values.lock().unwrap().get(id).cloned())
    }
}

impl SecretStoreMut for RecordingStore {
    fn set(&self, id: &str, value: &str) -> Result<(), SecretError> {
        self.calls
            .lock()
            .unwrap()
            .push(StoreCall::Set(id.to_string(), value.to_string()));
        if *self.fail_writes.lock().unwrap() {
            return Err(SecretError::new("Schlüsselbund gesperrt (Test)"));
        }
        self.put(id, value);
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), SecretError> {
        self.calls
            .lock()
            .unwrap()
            .push(StoreCall::Delete(id.to_string()));
        if *self.fail_writes.lock().unwrap() {
            return Err(SecretError::new("Schlüsselbund gesperrt (Test)"));
        }
        self.values.lock().unwrap().remove(id);
        Ok(())
    }
    fn backend(&self) -> SecretBackend {
        SecretBackend::Memory
    }
}

/// Umgebung aus einer festen Tabelle (die Prozessumgebung bleibt unberührt).
pub fn env_of(pairs: &[(&str, &str)]) -> os_ai::keys::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Arc::new(move |name: &str| map.get(name).cloned())
}

pub fn no_env() -> os_ai::keys::EnvLookup {
    env_of(&[])
}

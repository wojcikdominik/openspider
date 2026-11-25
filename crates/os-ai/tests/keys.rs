//! Schlüsselverwaltung (AI-009, INTEG-002; Entscheidung K4-12): Ablage über `SecretStoreMut` unter Dienst
//! `OpenSpider`, Konto `ai.<anbieter>`, Klartext. Alle Tests mit `MemorySecretStore` bzw. einem eigenen Test-Store;
//! kein Zugriff auf einen echten Schlüsselbund, kein Netz.
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use os_ai::catalog::choice::{settings_to_profile, TaskSettings};
use os_ai::catalog::Endpoint;
use os_ai::catalog::{ModelRef, TaskType};
use os_ai::keys::{
    AiKeyProvider, AiKeyStore, EnvLookup, KeyError, KeySource, KeyStatus, SaveOutcome,
    KEYCHAIN_SERVICE,
};
use os_fetch::secrets::{
    MemorySecretStore, SecretBackend, SecretError, SecretStore, SecretStoreMut,
};
use os_model::ai::AiStatus;

const KEY: &str = "sk-TEST-AI-009-abcd";

fn no_env() -> EnvLookup {
    Arc::new(|_: &str| None)
}

fn env_with(name: &'static str, value: &'static str) -> EnvLookup {
    Arc::new(move |n: &str| (n == name).then(|| value.to_string()))
}

fn memory() -> Arc<MemorySecretStore> {
    Arc::new(MemorySecretStore::new())
}

/// Ablage, die das Schreiben und Lesen protokolliert und optional verweigert (gesperrter Schlüsselbund).
#[derive(Default)]
struct RecordingStore {
    inner: MemorySecretStore,
    writes: Mutex<Vec<String>>,
    locked: bool,
}

impl SecretStore for RecordingStore {
    fn as_mut_store(&self) -> Option<&dyn SecretStoreMut> {
        Some(self)
    }
    fn get(&self, id: &str) -> Option<String> {
        self.try_get(id).ok().flatten()
    }
    fn try_get(&self, id: &str) -> Result<Option<String>, SecretError> {
        if self.locked {
            return Err(SecretError::new("Schlüsselbund gesperrt"));
        }
        Ok(SecretStore::get(&self.inner, id))
    }
}

impl SecretStoreMut for RecordingStore {
    fn set(&self, id: &str, value: &str) -> Result<(), SecretError> {
        if self.locked {
            return Err(SecretError::new("Schlüsselbund gesperrt"));
        }
        self.writes.lock().unwrap().push(id.to_string());
        SecretStoreMut::set(&self.inner, id, value)
    }
    fn delete(&self, id: &str) -> Result<(), SecretError> {
        if self.locked {
            return Err(SecretError::new("Schlüsselbund gesperrt"));
        }
        SecretStoreMut::delete(&self.inner, id)
    }
    fn backend(&self) -> SecretBackend {
        SecretBackend::Memory
    }
}

// spec: AI-009
// spec: INTEG-002 [M4]
// Ablage unter Dienst `OpenSpider`, Konto `ai.<anbieter>`, Wert = Schlüssel als Klartext (kein JSON, K4-12).
#[test]
fn keys_are_stored_as_plain_text_under_ai_provider_accounts() {
    assert_eq!(KEYCHAIN_SERVICE, "OpenSpider");
    let accounts: Vec<_> = AiKeyProvider::ALL.iter().map(|p| p.account()).collect();
    assert_eq!(accounts, ["ai.anthropic", "ai.openai", "ai.openrouter"]);
    let store = memory();
    let keys = AiKeyStore::with_env(store.clone(), no_env());
    for (i, provider) in AiKeyProvider::ALL.into_iter().enumerate() {
        let value = format!("{KEY}-{i}");
        assert_eq!(keys.save(provider, &value, true), Ok(SaveOutcome::Keychain));
        assert_eq!(SecretStore::get(&*store, provider.account()), Some(value));
    }
    // Kein Login oder sonstiger Name als Konto; nur die drei Einträge.
    for other in ["api/openai", "openai", "ai.OpenAI", "ai.ollama"] {
        assert_eq!(SecretStore::get(&*store, other), None, "{other}");
    }
}

// spec: AI-009
// spec: INTEG-002 [M4]
// Ein in 04 (INTEG-160) unter `ai.openai` hinterlegter Schlüssel ist in 08 ohne erneute Eingabe nutzbar und umgekehrt:
// beide Bereiche lesen denselben Eintrag.
#[test]
fn key_entered_in_04_is_usable_in_08_and_vice_versa() {
    let store = memory();
    // 04 schreibt direkt über SecretStoreMut unter der gemeinsamen Kennung.
    SecretStoreMut::set(&*store, "ai.openai", KEY).unwrap();
    let keys = AiKeyStore::with_env(store.clone(), no_env());
    let resolved = keys.resolve(AiKeyProvider::OpenAi).unwrap().unwrap();
    assert_eq!(resolved.secret.expose(), KEY);
    assert_eq!(resolved.source, KeySource::Keychain);
    // 08 speichert, 04 liest denselben Eintrag.
    keys.save(AiKeyProvider::OpenAi, "sk-TEST-AI-009-from-08", true)
        .unwrap();
    assert_eq!(
        SecretStore::get(&*store, "ai.openai").as_deref(),
        Some("sk-TEST-AI-009-from-08")
    );
}

// spec: INTEG-002 [M4]
// „merken“ an: nach Neustart (neuer Store-Zugriff auf dieselbe Ablage) nutzbar; „merken“ aus: nicht mehr vorhanden.
#[test]
fn remember_on_survives_restart_and_remember_off_does_not() {
    let store = memory();
    let first = AiKeyStore::with_env(store.clone(), no_env());
    first.save(AiKeyProvider::Anthropic, KEY, true).unwrap();
    assert_eq!(
        first.save(AiKeyProvider::OpenRouter, KEY, false),
        Ok(SaveOutcome::SessionOnly)
    );
    assert!(first.has_key(AiKeyProvider::OpenRouter));
    assert_eq!(
        first
            .resolve(AiKeyProvider::OpenRouter)
            .unwrap()
            .unwrap()
            .source,
        KeySource::Session
    );
    let restarted = AiKeyStore::with_env(store.clone(), no_env());
    assert!(restarted.has_key(AiKeyProvider::Anthropic));
    assert_eq!(
        restarted.status(AiKeyProvider::OpenRouter),
        KeyStatus::Missing
    );
    // „merken“ aus entfernt auch einen früher gespeicherten Eintrag.
    restarted
        .save(AiKeyProvider::Anthropic, KEY, false)
        .unwrap();
    assert_eq!(SecretStore::get(&*store, "ai.anthropic"), None);
}

// spec: AI-009
// Ein über Umgebungsvariable gesetzter Schlüssel wird verwendet (Vorrang) und nie in den Schlüsselbund geschrieben;
// die Anzeige kennt die Herkunft („aus Umgebungsvariable“).
#[test]
fn environment_key_wins_and_is_never_written_to_the_keychain() {
    assert_eq!(
        AiKeyProvider::ALL.map(|p| p.env_var()),
        [
            "OPENSPIDER_AI_KEY_ANTHROPIC",
            "OPENSPIDER_AI_KEY_OPENAI",
            "OPENSPIDER_AI_KEY_OPENROUTER"
        ]
    );
    let store = Arc::new(RecordingStore::default());
    SecretStoreMut::set(&*store, "ai.openai", "sk-TEST-AI-009-keychain").unwrap();
    store.writes.lock().unwrap().clear();
    let keys = AiKeyStore::with_env(
        store.clone(),
        env_with("OPENSPIDER_AI_KEY_OPENAI", "sk-TEST-AI-009-env-wxyz"),
    );
    let resolved = keys.resolve(AiKeyProvider::OpenAi).unwrap().unwrap();
    assert_eq!(resolved.source, KeySource::Environment);
    assert_eq!(resolved.secret.expose(), "sk-TEST-AI-009-env-wxyz");
    match keys.status(AiKeyProvider::OpenAi) {
        KeyStatus::Present { source, masked } => {
            assert_eq!(source, KeySource::Environment);
            assert!(masked.ends_with("wxyz"));
        }
        other => panic!("{other:?}"),
    }
    for p in AiKeyProvider::ALL {
        let _ = keys.status(p);
        let _ = keys.resolve(p);
    }
    assert!(
        store.writes.lock().unwrap().is_empty(),
        "kein Schreibzugriff"
    );
    assert_eq!(
        SecretStore::get(&store.inner, "ai.openai").as_deref(),
        Some("sk-TEST-AI-009-keychain")
    );
    // Leere Variable gilt als nicht gesetzt.
    let keys = AiKeyStore::with_env(store.clone(), env_with("OPENSPIDER_AI_KEY_OPENAI", "  "));
    assert_eq!(
        keys.resolve(AiKeyProvider::OpenAi).unwrap().unwrap().source,
        KeySource::Keychain
    );
}

// spec: INTEG-002 [M4]
// Gesperrter Schlüsselbund: verständliche Meldung, Schlüssel für die laufende Sitzung im Speicher nutzbar.
#[test]
fn locked_keychain_falls_back_to_the_session() {
    let store = Arc::new(RecordingStore {
        locked: true,
        ..RecordingStore::default()
    });
    let keys = AiKeyStore::with_env(store, no_env());
    match keys.status(AiKeyProvider::Anthropic) {
        KeyStatus::KeychainUnavailable { message } => assert!(message.contains("gesperrt")),
        other => panic!("{other:?}"),
    }
    match keys.save(AiKeyProvider::Anthropic, KEY, true).unwrap() {
        SaveOutcome::SessionFallback { message } => {
            assert!(message.contains("gesperrt"));
            assert!(!message.contains(KEY));
        }
        other => panic!("{other:?}"),
    }
    let resolved = keys.resolve(AiKeyProvider::Anthropic).unwrap().unwrap();
    assert_eq!(resolved.source, KeySource::Session);
    assert_eq!(resolved.secret.expose(), KEY);
}

// spec: AI-009
// Live-Probe Q00: Schlüssel können länger als 128 Zeichen sein – keine Längenannahme (200 Zeichen unverändert).
#[test]
fn keys_longer_than_128_characters_round_trip_unchanged() {
    let long = format!("sk-TEST-AI-009-{}Zq9", "x".repeat(182));
    assert_eq!(long.chars().count(), 200);
    let store = memory();
    let keys = AiKeyStore::with_env(store.clone(), no_env());
    keys.save(AiKeyProvider::OpenAi, &long, true).unwrap();
    assert_eq!(
        SecretStore::get(&*store, "ai.openai").as_deref(),
        Some(long.as_str())
    );
    let resolved = AiKeyStore::with_env(store, no_env())
        .resolve(AiKeyProvider::OpenAi)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.secret.expose(), long);
    assert_eq!(resolved.secret.expose().len(), 200);
}

// spec: AI-009
// spec: INTEG-002 [M4]
// Kein Schlüssel in Debug-Ausgaben, Statusanzeigen, Fehlertexten oder im exportierten Profil (Teststring
// `sk-TEST-AI-009`); die maskierte Anzeige zeigt nur die letzten vier Zeichen.
#[test]
fn keys_never_appear_in_debug_output_errors_or_profile() {
    let store = memory();
    let keys = AiKeyStore::with_env(
        store,
        env_with("OPENSPIDER_AI_KEY_OPENROUTER", "sk-TEST-AI-009-env"),
    );
    keys.save(AiKeyProvider::Anthropic, KEY, true).unwrap();
    keys.save(AiKeyProvider::OpenAi, KEY, false).unwrap();
    let mut texts = vec![format!("{keys:?}")];
    for p in AiKeyProvider::ALL {
        let resolved = keys.resolve(p).unwrap().unwrap();
        texts.push(format!("{resolved:?}"));
        texts.push(format!("{:?}", resolved.secret));
        texts.push(format!("{:?}", keys.status(p)));
        texts.push(resolved.secret.masked());
    }
    texts.push(format!("{:?}", keys.save(AiKeyProvider::OpenAi, "", true)));
    texts.push(KeyError::Empty.to_string());
    let mut profile = BTreeMap::new();
    profile.insert(
        TaskType::Decision,
        TaskSettings {
            model: Some(ModelRef::new("openai", "gpt-6-luna", Endpoint::Decisions)),
            ..TaskSettings::default()
        },
    );
    texts.push(serde_json::to_string(&settings_to_profile(&profile)).unwrap());
    for text in &texts {
        assert!(
            !text.contains("sk-TEST-AI-009"),
            "Schlüssel sichtbar in {text}"
        );
    }
    match keys.status(AiKeyProvider::Anthropic) {
        KeyStatus::Present { masked, .. } => {
            assert_eq!(
                masked,
                "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}abcd"
            )
        }
        other => panic!("{other:?}"),
    }
}

// spec: AI-009
// Fehlt der Schlüssel, lautet der Status der Einträge `Error: Missing API key`; ein leerer Schlüssel wird nicht
// gespeichert; Entfernen löscht den Eintrag.
#[test]
fn missing_key_maps_to_missing_api_key_status() {
    let store = memory();
    let keys = AiKeyStore::with_env(store.clone(), no_env());
    let status = keys.status(AiKeyProvider::OpenRouter);
    assert_eq!(status, KeyStatus::Missing);
    assert_eq!(status.missing_status(), Some(AiStatus::MissingApiKey));
    assert_eq!(
        keys.save(AiKeyProvider::OpenRouter, "   ", true),
        Err(KeyError::Empty)
    );
    keys.save(AiKeyProvider::OpenRouter, KEY, true).unwrap();
    assert_eq!(
        keys.status(AiKeyProvider::OpenRouter).missing_status(),
        None
    );
    keys.remove(AiKeyProvider::OpenRouter).unwrap();
    assert_eq!(SecretStore::get(&*store, "ai.openrouter"), None);
    assert!(!keys.has_key(AiKeyProvider::OpenRouter));
    assert_eq!(AiKeyProvider::from_id("ollama"), None);
    assert_eq!(
        AiKeyProvider::from_id("openrouter"),
        Some(AiKeyProvider::OpenRouter)
    );
}

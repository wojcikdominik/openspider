//! Unabhängige Abnahmetests M4-K04: Schlüsselverwaltung (AI-009) nach dem Ablageschema INTEG-002.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` AI-009 (Dienst `OpenSpider`, Konto `ai.<anbieter>`, Klartext
//! nach Entscheidung K4-12; Umgebungsvariablen `OPENSPIDER_AI_KEY_*` mit Vorrang, nie in den Schlüsselbund; kein
//! Schlüssel in Profil/Logs, Prüfstring `sk-TEST-AI-009`; fehlender Schlüssel → `Error: Missing API key`),
//! `docs/spec/04-custom-integrations.md` INTEG-002 („Schlüssel merken“ an/aus, gesperrter Schlüsselbund mit Meldung
//! und Sitzungsschlüssel, maskierte Anzeige `••••••••abcd`, Login-Wechsel überschreibt) und `docs/plan/M4.md`
//! (K4-12, E-7 a: auf Linux/Docker nur Umgebungsvariablen; Q00-Hinweis: Schlüssel länger als 128 Zeichen).
//! Schlüssel liegen nur im Test-Schlüsselspeicher (`RecordingStore`); kein echter Schlüsselbund.
// acceptance: independent

mod acceptance_k04_support;

use acceptance_k04_support::*;
use os_ai::keys::{
    AiKeyProvider, AiKeyStore, ApiKeySecret, KeyError, KeySource, KeyStatus, SaveOutcome,
    KEYCHAIN_SERVICE,
};
use os_ai::AiStatus;

const MASK8: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

fn store_with(store: &std::sync::Arc<RecordingStore>, env: os_ai::keys::EnvLookup) -> AiKeyStore {
    AiKeyStore::with_env(store.clone(), env)
}

/// Kein Schlüsselteil außer den letzten vier Zeichen (maskierte Anzeige) darf in `text` stehen.
fn assert_no_key(text: &str, key: &str) {
    assert!(!text.contains(key), "Schlüssel in Ausgabe: {text}");
    let head: String = key.chars().take(12).collect();
    assert!(!text.contains(&head), "Schlüsselanfang in Ausgabe: {text}");
    assert!(
        !text.contains("sk-TEST-AI-009"),
        "Prüfstring in Ausgabe: {text}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Ablageschema

// spec: AI-009
// acceptance: independent
#[test]
fn keychain_identifiers_follow_integ_002_schema() {
    assert_eq!(KEYCHAIN_SERVICE, "OpenSpider");
    let accounts: Vec<_> = AiKeyProvider::ALL.iter().map(|p| p.account()).collect();
    assert_eq!(accounts, ["ai.anthropic", "ai.openai", "ai.openrouter"]);
    let vars: Vec<_> = AiKeyProvider::ALL.iter().map(|p| p.env_var()).collect();
    assert_eq!(
        vars,
        [
            "OPENSPIDER_AI_KEY_ANTHROPIC",
            "OPENSPIDER_AI_KEY_OPENAI",
            "OPENSPIDER_AI_KEY_OPENROUTER"
        ]
    );
    // Ollama braucht keinen Schlüssel und hat keinen Eintrag.
    assert_eq!(AiKeyProvider::from_id("ollama"), None);
    assert_eq!(
        AiKeyProvider::from_id("openrouter"),
        Some(AiKeyProvider::OpenRouter)
    );
}

// spec: AI-009
// acceptance: independent
#[test]
fn save_with_remember_writes_one_plaintext_entry_per_provider() {
    let store = RecordingStore::new();
    let keys = store_with(&store, no_env());
    for (provider, value) in [
        (AiKeyProvider::Anthropic, "acc-anthropic-key-1111"),
        (AiKeyProvider::OpenAi, "acc-openai-key-2222"),
        (AiKeyProvider::OpenRouter, "acc-openrouter-key-3333"),
    ] {
        assert_eq!(
            keys.save(provider, value, true).unwrap(),
            SaveOutcome::Keychain
        );
    }
    let entries = store.entries();
    assert_eq!(
        entries.len(),
        3,
        "genau ein Eintrag je Anbieter: {entries:?}"
    );
    // Wert = Schlüssel als Klartext-Zeichenkette (kein JSON), K4-12.
    assert_eq!(entries["ai.anthropic"], "acc-anthropic-key-1111");
    assert_eq!(entries["ai.openai"], "acc-openai-key-2222");
    assert_eq!(entries["ai.openrouter"], "acc-openrouter-key-3333");
    for value in entries.values() {
        assert!(serde_json::from_str::<serde_json::Value>(value).is_err());
    }
}

// spec: INTEG-002 [M4]
// acceptance: independent
#[test]
fn saving_again_overwrites_the_existing_entry() {
    let store = RecordingStore::new();
    let keys = store_with(&store, no_env());
    keys.save(AiKeyProvider::OpenAi, "acc-first-key-aaaa", true)
        .unwrap();
    keys.save(AiKeyProvider::OpenAi, "acc-second-key-bbbb", true)
        .unwrap();
    let entries = store.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries["ai.openai"], "acc-second-key-bbbb");
    let resolved = keys.resolve(AiKeyProvider::OpenAi).unwrap().unwrap();
    assert_eq!(resolved.secret.expose(), "acc-second-key-bbbb");
}

// spec: AI-009
// acceptance: independent
#[test]
fn entry_written_elsewhere_is_used_without_new_input_and_survives_restart() {
    // Ein in 04 (INTEG-160) eingegebener OpenAI-Schlüssel liegt unter demselben Konto `ai.openai`.
    let store = RecordingStore::new();
    store.put("ai.openai", "acc-entered-in-04-cccc");
    let keys = store_with(&store, no_env());
    let r = keys.resolve(AiKeyProvider::OpenAi).unwrap().unwrap();
    assert_eq!(r.source, KeySource::Keychain);
    assert_eq!(r.secret.expose(), "acc-entered-in-04-cccc");
    // umgekehrt: in 08 gespeichert, nach „Neustart“ (neue Instanz) ohne erneute Eingabe vorhanden
    keys.save(AiKeyProvider::Anthropic, "acc-entered-in-08-dddd", true)
        .unwrap();
    drop(keys);
    let restarted = store_with(&store, no_env());
    assert_eq!(
        restarted
            .resolve(AiKeyProvider::Anthropic)
            .unwrap()
            .unwrap()
            .secret
            .expose(),
        "acc-entered-in-08-dddd"
    );
    assert!(restarted.has_key(AiKeyProvider::OpenAi));
    assert!(!restarted.has_key(AiKeyProvider::OpenRouter));
}

// ---------------------------------------------------------------------------------------------------------------
// Umgebungsvariablen

// spec: AI-009
// acceptance: independent
#[test]
fn environment_variable_takes_precedence_and_is_reported_as_such() {
    let store = RecordingStore::new();
    store.put("ai.openai", "acc-keychain-key-eeee");
    let keys = store_with(
        &store,
        env_of(&[("OPENSPIDER_AI_KEY_OPENAI", "acc-env-key-ffff")]),
    );
    let r = keys.resolve(AiKeyProvider::OpenAi).unwrap().unwrap();
    assert_eq!(r.source, KeySource::Environment);
    assert_eq!(r.secret.expose(), "acc-env-key-ffff");
    match keys.status(AiKeyProvider::OpenAi) {
        KeyStatus::Present { source, masked } => {
            assert_eq!(
                source,
                KeySource::Environment,
                "UI: „aus Umgebungsvariable“"
            );
            assert_eq!(masked, format!("{MASK8}ffff"));
        }
        other => panic!("erwartet Present, war {other:?}"),
    }
    // nur für den Anbieter der Variable: Anthropic kommt weiter aus dem Schlüsselbund bzw. fehlt
    assert!(!keys.has_key(AiKeyProvider::Anthropic));
    store.put("ai.anthropic", "acc-keychain-anthropic");
    assert_eq!(
        keys.resolve(AiKeyProvider::Anthropic)
            .unwrap()
            .unwrap()
            .source,
        KeySource::Keychain
    );
}

// spec: AI-009
// acceptance: independent
#[test]
fn environment_key_is_never_written_to_the_keychain() {
    let store = RecordingStore::new();
    let env_values = [
        ("OPENSPIDER_AI_KEY_ANTHROPIC", "acc-env-anthropic-1234"),
        ("OPENSPIDER_AI_KEY_OPENAI", "acc-env-openai-5678"),
        ("OPENSPIDER_AI_KEY_OPENROUTER", "acc-env-openrouter-9012"),
    ];
    let keys = store_with(&store, env_of(&env_values));
    for p in AiKeyProvider::ALL {
        keys.resolve(p).unwrap();
        keys.status(p);
        keys.has_key(p);
    }
    // auch ein Speichern eines anderen Wertes und Löschen dürfen den Umgebungswert nicht ablegen
    keys.save(AiKeyProvider::OpenAi, "acc-typed-key-0000", true)
        .unwrap();
    keys.remove(AiKeyProvider::OpenAi).unwrap();
    for (_, value) in store.sets() {
        assert!(
            env_values.iter().all(|(_, env)| *env != value),
            "Umgebungsschlüssel im Schlüsselbund abgelegt"
        );
    }
    assert!(store
        .entries()
        .values()
        .all(|v| env_values.iter().all(|(_, env)| env != v)));
    // Löschen lässt die Umgebungsvariable unberührt: Schlüssel weiter aus der Umgebung
    assert_eq!(
        keys.resolve(AiKeyProvider::OpenAi).unwrap().unwrap().source,
        KeySource::Environment
    );
}

// spec: AI-009
// acceptance: independent
#[test]
fn without_keychain_only_environment_keys_work() {
    // E-7 a (Linux/Docker): Schlüssel nur über Umgebungsvariablen; der Schlüsselbund ist nicht verfügbar.
    let store = RecordingStore::locked();
    let keys = store_with(
        &store,
        env_of(&[("OPENSPIDER_AI_KEY_OPENROUTER", "acc-headless-key-7777")]),
    );
    let r = keys.resolve(AiKeyProvider::OpenRouter).unwrap().unwrap();
    assert_eq!(r.source, KeySource::Environment);
    assert!(keys.has_key(AiKeyProvider::OpenRouter));
    assert!(
        store.sets().is_empty(),
        "nichts in den Schlüsselbund schreiben"
    );
    // ohne Variable: kein Schlüssel nutzbar, Statuswert für die Einträge `Error: Missing API key`
    let status = keys.status(AiKeyProvider::Anthropic);
    assert!(!status.is_present());
    assert_eq!(status.missing_status(), Some(AiStatus::MissingApiKey));
    assert_eq!(
        AiStatus::MissingApiKey.to_string(),
        "Error: Missing API key"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// „Schlüssel merken“, gesperrter Schlüsselbund, Löschen

// spec: INTEG-002 [M4]
// acceptance: independent
#[test]
fn remember_off_keeps_key_for_session_only() {
    let store = RecordingStore::new();
    let keys = store_with(&store, no_env());
    assert_eq!(
        keys.save(AiKeyProvider::Anthropic, "acc-session-key-4444", false)
            .unwrap(),
        SaveOutcome::SessionOnly
    );
    let r = keys.resolve(AiKeyProvider::Anthropic).unwrap().unwrap();
    assert_eq!(r.source, KeySource::Session);
    assert_eq!(r.secret.expose(), "acc-session-key-4444");
    assert!(store.sets().is_empty(), "„merken“ aus schreibt nicht");
    assert!(store.entries().is_empty());
    // nach Neustart nicht mehr vorhanden
    drop(keys);
    let restarted = store_with(&store, no_env());
    assert_eq!(restarted.resolve(AiKeyProvider::Anthropic).unwrap(), None);
    assert_eq!(
        restarted.status(AiKeyProvider::Anthropic),
        KeyStatus::Missing
    );
}

// spec: INTEG-002 [M4]
// acceptance: independent
#[test]
fn remember_off_after_remember_on_leaves_nothing_after_restart() {
    let store = RecordingStore::new();
    let keys = store_with(&store, no_env());
    keys.save(AiKeyProvider::OpenAi, "acc-remembered-5555", true)
        .unwrap();
    keys.save(AiKeyProvider::OpenAi, "acc-not-remembered-6666", false)
        .unwrap();
    assert_eq!(
        keys.resolve(AiKeyProvider::OpenAi)
            .unwrap()
            .unwrap()
            .secret
            .expose(),
        "acc-not-remembered-6666"
    );
    let restarted = store_with(&store, no_env());
    assert_eq!(restarted.resolve(AiKeyProvider::OpenAi).unwrap(), None);
}

// spec: INTEG-002 [M4]
// acceptance: independent
#[test]
fn locked_keychain_on_save_gives_message_and_session_key() {
    let store = RecordingStore::locked();
    let keys = store_with(&store, no_env());
    let outcome = keys
        .save(AiKeyProvider::OpenAi, TEST_KEY, true)
        .expect("kein harter Fehler");
    match &outcome {
        SaveOutcome::SessionFallback { message } => {
            assert!(!message.trim().is_empty(), "verständliche Meldung");
            assert_no_key(message, TEST_KEY);
        }
        other => panic!("erwartet SessionFallback, war {other:?}"),
    }
    let r = keys.resolve(AiKeyProvider::OpenAi).unwrap().unwrap();
    assert_eq!(r.source, KeySource::Session);
    assert_eq!(r.secret.expose(), TEST_KEY);
    assert!(store.entries().is_empty());
}

// spec: INTEG-002 [M4]
// acceptance: independent
#[test]
fn unreadable_keychain_reports_status_without_key() {
    let store = RecordingStore::new();
    store.put("ai.anthropic", TEST_KEY);
    store.set_locked(true);
    let keys = store_with(&store, no_env());
    let status = keys.status(AiKeyProvider::Anthropic);
    match &status {
        KeyStatus::KeychainUnavailable { message } => {
            assert!(!message.trim().is_empty());
            assert_no_key(message, TEST_KEY);
        }
        other => panic!("erwartet KeychainUnavailable, war {other:?}"),
    }
    assert_eq!(status.missing_status(), Some(AiStatus::MissingApiKey));
    let err = keys.resolve(AiKeyProvider::Anthropic).unwrap_err();
    assert_no_key(&err.to_string(), TEST_KEY);
    assert_no_key(&format!("{err:?}"), TEST_KEY);
    // nach Entsperren wieder nutzbar
    store.set_locked(false);
    assert!(keys.has_key(AiKeyProvider::Anthropic));
}

// spec: AI-009
// acceptance: independent
#[test]
fn remove_deletes_session_and_keychain_entry() {
    let store = RecordingStore::new();
    let keys = store_with(&store, no_env());
    keys.save(AiKeyProvider::OpenRouter, "acc-to-delete-8888", true)
        .unwrap();
    keys.save(AiKeyProvider::Anthropic, "acc-session-only-9999", false)
        .unwrap();
    keys.remove(AiKeyProvider::OpenRouter).unwrap();
    keys.remove(AiKeyProvider::Anthropic).unwrap();
    assert!(store.entries().is_empty());
    assert_eq!(keys.resolve(AiKeyProvider::OpenRouter).unwrap(), None);
    assert_eq!(keys.resolve(AiKeyProvider::Anthropic).unwrap(), None);
    assert_eq!(keys.status(AiKeyProvider::OpenRouter), KeyStatus::Missing);
    // Löschen eines nicht vorhandenen Eintrags ist kein Fehler
    keys.remove(AiKeyProvider::OpenAi).unwrap();
}

// spec: AI-009
// acceptance: independent
#[test]
fn remove_on_locked_keychain_reports_error_without_key() {
    let store = RecordingStore::new();
    let keys = store_with(&store, no_env());
    keys.save(AiKeyProvider::OpenAi, TEST_KEY, true).unwrap();
    store.set_locked(true);
    let err = keys.remove(AiKeyProvider::OpenAi).unwrap_err();
    assert!(matches!(err, KeyError::KeychainUnavailable(_)));
    assert_no_key(&err.to_string(), TEST_KEY);
}

// ---------------------------------------------------------------------------------------------------------------
// Länge und Leerraum

fn long_key(len: usize) -> String {
    let body: String = (0..len)
        .map(|i| char::from(b"abcdefghijklmnopqrstuvwxyz0123456789-_"[i % 38]))
        .collect();
    assert_eq!(body.chars().count(), len);
    body
}

// spec: AI-009
// acceptance: independent
#[test]
fn long_keys_roundtrip_unchanged_at_128_129_and_200_chars() {
    // Hinweis Q00: Schlüssel über 128 Zeichen sind üblich und dürfen nicht abgeschnitten werden.
    for len in [128usize, 129, 200] {
        let key = long_key(len);
        let store = RecordingStore::new();
        let keys = store_with(&store, no_env());
        keys.save(AiKeyProvider::OpenAi, &key, true).unwrap();
        assert_eq!(store.entries()["ai.openai"], key, "Länge {len}");
        let restarted = store_with(&store, no_env());
        let got = restarted.resolve(AiKeyProvider::OpenAi).unwrap().unwrap();
        assert_eq!(got.secret.expose(), key, "Länge {len}");
        assert_eq!(got.secret.expose().chars().count(), len);
        let tail: String = key.chars().skip(len - 4).collect();
        assert_eq!(got.secret.masked(), format!("{MASK8}{tail}"));
        // auch als Umgebungsvariable
        let env_keys = store_with(
            &RecordingStore::new(),
            env_of(&[("OPENSPIDER_AI_KEY_OPENAI", key.as_str())]),
        );
        assert_eq!(
            env_keys
                .resolve(AiKeyProvider::OpenAi)
                .unwrap()
                .unwrap()
                .secret
                .expose(),
            key
        );
    }
}

// spec: AI-009
// acceptance: independent
#[test]
fn whitespace_around_key_is_not_part_of_the_key() {
    // Auslegung (Spec offen, Bericht B2): eingefügter Leerraum am Rand gehört nicht zum Schlüssel; der Schlüssel
    // im Inneren bleibt unverändert.
    let store = RecordingStore::new();
    let keys = store_with(&store, no_env());
    keys.save(AiKeyProvider::Anthropic, "  \tacc-padded-key-abcd \n", true)
        .unwrap();
    assert_eq!(store.entries()["ai.anthropic"], "acc-padded-key-abcd");
    let env_keys = store_with(
        &RecordingStore::new(),
        env_of(&[("OPENSPIDER_AI_KEY_ANTHROPIC", " acc-env-padded-wxyz\n")]),
    );
    assert_eq!(
        env_keys
            .resolve(AiKeyProvider::Anthropic)
            .unwrap()
            .unwrap()
            .secret
            .expose(),
        "acc-env-padded-wxyz"
    );
}

// spec: AI-009
// acceptance: independent
#[test]
fn empty_key_is_rejected_and_nothing_is_written() {
    let store = RecordingStore::new();
    let keys = store_with(&store, no_env());
    for value in ["", "   ", "\n\t"] {
        assert_eq!(
            keys.save(AiKeyProvider::OpenAi, value, true),
            Err(KeyError::Empty)
        );
    }
    assert!(store.sets().is_empty());
    assert!(ApiKeySecret::new("  ").is_none());
}

// ---------------------------------------------------------------------------------------------------------------
// Kein Schlüssel in Ausgaben

// spec: INTEG-002 [M4]
// acceptance: independent
#[test]
fn masked_display_shows_eight_dots_and_last_four_chars() {
    let secret = ApiKeySecret::new("acc-mask-check-abcd").unwrap();
    assert_eq!(secret.masked(), format!("{MASK8}abcd"));
    assert_no_key(&secret.masked(), "acc-mask-check-abcd");
}

// spec: AI-009
// acceptance: independent
#[test]
fn no_key_in_debug_display_or_error_texts() {
    let store = RecordingStore::new();
    let keys = store_with(
        &store,
        env_of(&[("OPENSPIDER_AI_KEY_OPENROUTER", TEST_KEY)]),
    );
    keys.save(AiKeyProvider::Anthropic, TEST_KEY, false)
        .unwrap();
    keys.save(AiKeyProvider::OpenAi, TEST_KEY, true).unwrap();
    let mut outputs = vec![format!("{keys:?}")];
    for p in AiKeyProvider::ALL {
        let resolved = keys.resolve(p).unwrap().unwrap();
        outputs.push(format!("{resolved:?}"));
        outputs.push(format!("{:?}", resolved.secret));
        outputs.push(format!("{:#?}", resolved));
        let status = keys.status(p);
        outputs.push(format!("{status:?}"));
    }
    // Ergebnisse und Fehler der Ablage
    let locked = RecordingStore::locked();
    let locked_keys = store_with(&locked, no_env());
    let outcome = locked_keys
        .save(AiKeyProvider::OpenAi, TEST_KEY, true)
        .unwrap();
    outputs.push(format!("{outcome:?}"));
    outputs.push(format!("{locked_keys:?}"));
    let err = KeyError::KeychainUnavailable("gesperrt".into());
    outputs.push(err.to_string());
    outputs.push(format!("{err:?}"));
    outputs.push(KeyError::Empty.to_string());
    // Schlüsseltyp der Adapter, an den der Schlüssel weitergereicht wird
    outputs.push(format!(
        "{:?}",
        os_ai::adapters::anthropic::ApiKey::new(TEST_KEY)
    ));
    for text in outputs {
        assert_no_key(&text, TEST_KEY);
    }
}

// spec: AI-009
// acceptance: independent
#[test]
fn configuration_profile_never_contains_a_key() {
    use os_ai::catalog::choice::settings_to_profile;
    use os_ai::catalog::Endpoint;
    use os_ai::catalog::{ModelRef, TaskSettings, TaskType};
    use std::collections::BTreeMap;

    let store = RecordingStore::new();
    let keys = store_with(
        &store,
        env_of(&[("OPENSPIDER_AI_KEY_OPENROUTER", TEST_KEY)]),
    );
    keys.save(AiKeyProvider::OpenAi, TEST_KEY, true).unwrap();
    keys.save(AiKeyProvider::Anthropic, TEST_KEY, false)
        .unwrap();
    let mut settings = BTreeMap::new();
    settings.insert(
        TaskType::SuggestTitle,
        TaskSettings {
            model: Some(ModelRef::new("openai", "gpt-6-luna", Endpoint::Responses)),
            fallback: vec![ModelRef::new(
                "anthropic",
                "mock-claude",
                Endpoint::Messages,
            )],
            max_tokens: Some(700),
            ..TaskSettings::default()
        },
    );
    let mut config = os_model::config::CrawlConfig::default();
    config.ai.tasks = settings_to_profile(&settings);
    let toml = config.to_toml_string().unwrap();
    assert_no_key(&toml, TEST_KEY);
    let json = serde_json::to_string(&config).unwrap();
    assert_no_key(&json, TEST_KEY);
}

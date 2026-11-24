//! Unabhängiger Abnahmetest M4-K03: Die Ollama-Server-URL kommt allein aus der gemeinsamen Einstellung, die der
//! Aufrufer übergibt (AI-006 Kriterium 4, INTEG-160; Festlegung Orchestrator: der Adapter liest keine
//! Umgebungsvariable). Eigene Testdatei mit genau einem Test, weil er eine Umgebungsvariable des Prozesses setzt.
// acceptance: independent

mod acceptance_k03_support;

use acceptance_k03_support::*;
use os_ai::adapters::ollama::{OllamaAdapter, OllamaConfig};
use os_model::config::AiAppSettings;
use serde_json::json;

// spec: AI-006
// Kriterium: „Eine in 04 geänderte Server-URL gilt ohne weitere Eingabe auch für 08 (eine gemeinsame Einstellung).“
// – eine gesetzte Umgebungsvariable `OPENSPIDER_API_BASE_OLLAMA` lenkt den Adapter nicht um; maßgeblich ist die
// übergebene Einstellung.
#[tokio::test]
async fn adapter_ignores_environment_and_uses_shared_setting() {
    let from_env = Stub::start(|_, _| StubResponse::json(200, json!({"models": []}))).await;
    let from_setting = Stub::start(|_, _| {
        StubResponse::json(200, json!({"models": [{"name": "aus-der-einstellung"}]}))
    })
    .await;
    std::env::set_var("OPENSPIDER_API_BASE_OLLAMA", from_env.base());

    let defaults = OllamaConfig::from_app_settings(&AiAppSettings::default());
    assert_eq!(
        defaults.base_url, "http://localhost:11434",
        "ohne Einstellung gilt der Standard aus INTEG-160, nicht die Umgebungsvariable"
    );

    let mut settings = AiAppSettings::default();
    settings
        .provider_endpoints
        .get_mut("ollama")
        .expect("Ollama-Eintrag")
        .base_url = Some(from_setting.base());
    let adapter = OllamaAdapter::new(client(), OllamaConfig::from_app_settings(&settings));
    let models = adapter.connect().await.expect("Modellliste");
    std::env::remove_var("OPENSPIDER_API_BASE_OLLAMA");

    assert_eq!(models[0].name, "aus-der-einstellung");
    assert_eq!(from_setting.requests().len(), 1);
    assert_eq!(from_env.requests().len(), 0);
}

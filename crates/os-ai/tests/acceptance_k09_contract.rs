//! Unabhängige Abnahmetests M4-K09 für AI-001 (Nachtrag 24.11.2026, Q00-H1, N2): Contract-Test mit Szenario
//! `reasoning=40` über alle Adapter (gleiches geparstes Ergebnis, `reasoning_tokens` = 40 bei den drei Cloud-Adaptern,
//! `null` bei Ollama), Reasoning-Inhalte sind kein Ergebnis, Anbieter-Request-ID je Anbieter.
mod acceptance_k09_support;

use acceptance_k09_support::{has_reasoning_marker, model, ok_schema, request, Mock};
use os_ai::adapters::ollama;
use os_ai::provider::{AiProvider, ReasoningLevel, Response};
use serde_json::json;

/// Die Contract-Aufgabe („gib `{"ok": true}` nach Schema zurück“) über das gemeinsame Interface.
async fn contract(
    provider: &dyn AiProvider,
    model_id: &str,
    endpoint: &str,
    text: &str,
) -> Response {
    let r = request(
        model(provider.provider_id(), model_id, endpoint, Vec::new()),
        text,
        Some(ok_schema()),
        ReasoningLevel::ModelDefault,
        Some(256),
    );
    provider
        .execute(r)
        .await
        .unwrap_or_else(|e| panic!("{}: {e:?}", provider.provider_id()))
}

/// ID des Anthropic-Mock-Modells A.
async fn anthropic_a(mock: &Mock) -> String {
    mock.anthropic()
        .list_models()
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.display_name.as_deref() == Some("A"))
        .unwrap()
        .id
}

// spec: AI-001 [M4]
// acceptance: independent
// Kriterium: Contract-Test mit Szenario `reasoning=40`: alle drei Cloud-Adapter liefern `reasoning_tokens = 40` und
// dasselbe geparste Ergebnis wie ohne Szenario; Ollama liefert `reasoning_tokens = null`. Reasoning-Inhalte
// (`MOCK-REASONING-TEXT`, `MOCK-ENCRYPTED`, `MOCK-SIGNATURE`) stehen in keinem Ergebnis.
#[tokio::test]
async fn contract_with_reasoning_40() {
    let mock = Mock::start().await;
    let a = anthropic_a(&mock).await;
    let anthropic = mock.anthropic();
    let responses = mock.responses();
    let openrouter = mock.openrouter();
    let ollama = mock.ollama();
    let cloud: [(&dyn AiProvider, &str, &str); 3] = [
        (&anthropic, a.as_str(), "messages"),
        (&responses, "gpt-6-luna", "responses"),
        (&openrouter, "mock/text", "chat-completions"),
    ];
    for (provider, id, endpoint) in cloud {
        let plain = contract(provider, id, endpoint, "K09 contract plain").await;
        let thinking = contract(
            provider,
            id,
            endpoint,
            "K09 contract MOCK-SCENARIO:reasoning=40 x",
        )
        .await;
        let name = provider.provider_id();
        assert_eq!(plain.content, Some(json!({"ok": true})), "{name}");
        assert_eq!(thinking.content, plain.content, "{name}");
        assert!(!thinking.refusal, "{name}");
        assert_eq!(thinking.usage.reasoning_tokens, Some(40), "{name}");
        assert_eq!(plain.usage.reasoning_tokens, Some(0), "{name}");
        assert_eq!(
            thinking.usage.output_tokens,
            plain.usage.output_tokens.map(|o| o + 40),
            "{name}: Reasoning ist Teilmenge der Ausgabe"
        );
        assert!(
            !has_reasoning_marker(&format!("{thinking:?}")),
            "{name}: {thinking:?}"
        );
    }
    let plain = contract(
        &ollama,
        "mock-llama",
        ollama::ENDPOINT,
        "K09 contract plain",
    )
    .await;
    let thinking = contract(
        &ollama,
        "mock-llama",
        ollama::ENDPOINT,
        "K09 contract MOCK-SCENARIO:reasoning=40 x",
    )
    .await;
    assert_eq!(thinking.content, Some(json!({"ok": true})));
    assert_eq!(thinking.content, plain.content);
    assert_eq!(thinking.usage.reasoning_tokens, None);
    assert_eq!(plain.usage.reasoning_tokens, None);
}

// spec: AI-001 [M4]
// acceptance: independent
// Anbieter-Request-ID im gemeinsamen Ergebnis: Anthropic Header `request-id` (nie `msg_…`), OpenAI `x-request-id`,
// OpenRouter `x-generation-id`, Ollama keine (leer).
#[tokio::test]
async fn provider_request_ids_per_provider() {
    let mock = Mock::start().await;
    let a = anthropic_a(&mock).await;
    let anthropic = contract(&mock.anthropic(), &a, "messages", "K09 id").await;
    let responses = contract(&mock.responses(), "gpt-6-luna", "responses", "K09 id").await;
    let openrouter = contract(
        &mock.openrouter(),
        "mock/text",
        "chat-completions",
        "K09 id",
    )
    .await;
    let ollama = contract(&mock.ollama(), "mock-llama", ollama::ENDPOINT, "K09 id").await;
    let id = |r: &Response| r.provider_request_id.clone().unwrap_or_default();
    assert!(
        id(&anthropic).starts_with("req_mock_"),
        "{}",
        id(&anthropic)
    );
    assert!(!id(&anthropic).starts_with("msg_"));
    assert!(
        id(&responses).starts_with("req_mock_"),
        "{}",
        id(&responses)
    );
    assert!(
        id(&openrouter).starts_with("gen-mock-"),
        "{}",
        id(&openrouter)
    );
    assert_eq!(ollama.provider_request_id, None);
}

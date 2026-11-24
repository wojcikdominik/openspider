//! Contract-Test AI-001 mit Szenario `reasoning=40` (Nachtrag 24.11.2026, Q00; M4-K09): dieselbe Aufgabe („gib
//! `{"ok": true}` nach Schema zurück“) über alle Text-Adapter gegen den ai-mock – gleiches geparstes Ergebnis mit und
//! ohne Reasoning, `reasoning_tokens = 40` bei Anthropic, OpenAI Responses und OpenRouter, `null` bei Ollama; Reasoning-
//! Inhalte erscheinen in keinem Ergebnis.
use std::sync::Arc;

use os_ai::adapters::anthropic::{AnthropicAdapter, AnthropicConfig, ApiKey};
use os_ai::adapters::ollama::{self, OllamaAdapter, OllamaConfig};
use os_ai::adapters::openai_responses::{
    self, OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses,
};
use os_ai::adapters::openrouter::{OpenRouterAdapter, OpenRouterConfig};
use os_ai::provider::{
    self, AiProvider, Capabilities, CapabilitySource, Input, Model, Outcome, ReasoningLevel,
    Request, Response,
};
use os_ai::PreviewSwitch;
use os_testsite::mocks::ai::{AiMock, ENCRYPTED, REASONING_TEXT, SIGNATURE};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};

fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

fn caps() -> Capabilities {
    Capabilities {
        text: true,
        structured_output: true,
        ..Capabilities::default()
    }
}

fn model(provider: &str, id: &str, endpoint: &str) -> Model {
    Model {
        provider: provider.into(),
        id: id.into(),
        endpoint: endpoint.into(),
        capabilities: caps(),
        capability_source: CapabilitySource::ModelCatalog,
        reasoning_levels: Vec::new(),
    }
}

fn task(model: Model, scenario: Option<&str>) -> Request {
    let text = match scenario {
        Some(s) => format!("Gib ok zurück. MOCK-ID:home MOCK-SCENARIO:{s}"),
        None => "Gib ok zurück. MOCK-ID:home".to_string(),
    };
    Request {
        model,
        required: caps(),
        input: vec![Input::Text { text }],
        instructions: "Antworte nach Schema.".into(),
        questions: Vec::new(),
        response_schema: Some(json!({
            "type": "object",
            "properties": {"ok": {"type": "boolean"}},
            "required": ["ok"],
            "additionalProperties": false
        })),
        max_tokens: Some(1024),
        temperature: None,
        effort: None,
        reasoning: ReasoningLevel::ModelDefault,
    }
}

async fn completed(provider: &dyn AiProvider, request: Request) -> Response {
    match provider::execute(PreviewSwitch::ON, provider, request).await {
        Outcome::Completed(response) => response,
        other => panic!("{}: {other:?}", provider.provider_id()),
    }
}

// spec: AI-001 [M4]
// Contract-Test mit Szenario `reasoning=40`: alle drei Cloud-Adapter liefern `reasoning_tokens = 40` und dasselbe
// geparste Ergebnis wie ohne Szenario; Ollama liefert `reasoning_tokens = null`. Die Reasoning-Inhalte
// (`MOCK-REASONING-TEXT`, `MOCK-ENCRYPTED`, `MOCK-SIGNATURE`) kommen in keinem Ergebnis vor.
#[tokio::test]
async fn reasoning_40_gives_the_same_result_on_every_adapter() {
    let mock = AiMock::fixtures().unwrap();
    let server = MockServer::start(mock.registrations()).await.unwrap();
    let anthropic = AnthropicAdapter::new(
        client(),
        AnthropicConfig::new(ApiKey::new("sk-ant-TEST-K09"))
            .with_base_url(server.base_url("anthropic")),
    );
    let responses = OpenAiResponses::new(OpenAiHttp::new(
        client(),
        OpenAiConfig::new(openai_responses::ApiKey::new("sk-TEST-K09"))
            .with_base_url(format!("{}/v1", server.base_url("openai"))),
        OpenAiAccount::new(),
    ));
    let openrouter = OpenRouterAdapter::new(
        client(),
        OpenRouterConfig::new(ApiKey::new("sk-or-TEST-K09"))
            .with_base_url(format!("{}/api/v1", server.base_url("openrouter"))),
    );
    let ollama = OllamaAdapter::new(
        client(),
        OllamaConfig::new().with_base_url(server.base_url("ollama")),
    );
    let adapters: Vec<(Arc<dyn AiProvider>, Model, Option<u64>)> = vec![
        (
            Arc::new(anthropic),
            model("anthropic", "mock-claude", "messages"),
            Some(40),
        ),
        (
            Arc::new(responses),
            model("openai", "gpt-6-luna", "responses"),
            Some(40),
        ),
        (
            Arc::new(openrouter),
            model("openrouter", "mock/text", "chat-completions"),
            Some(40),
        ),
        (
            Arc::new(ollama),
            model("ollama", "mock-llama", ollama::ENDPOINT),
            None,
        ),
    ];
    for (adapter, model, expected) in adapters {
        let plain = completed(adapter.as_ref(), task(model.clone(), None)).await;
        let with = completed(adapter.as_ref(), task(model, Some("reasoning=40"))).await;
        let id = adapter.provider_id().to_string();
        assert_eq!(plain.content, Some(json!({"ok": true})), "{id}");
        assert_eq!(with.content, plain.content, "{id}");
        assert_eq!(with.usage.reasoning_tokens, expected, "{id}");
        if let (Some(w), Some(p)) = (with.usage.output_tokens, plain.usage.output_tokens) {
            assert_eq!(
                w,
                p + expected.unwrap_or(0),
                "{id}: Reasoning ist Teil von output_tokens"
            );
        }
        let rendered: Value = serde_json::to_value(&with).unwrap();
        let rendered = format!("{rendered} {with:?}");
        for marker in [REASONING_TEXT, ENCRYPTED, SIGNATURE] {
            assert!(!rendered.contains(marker), "{id}: {marker}");
        }
    }
    // Die Mock-Antworten enthielten die Reasoning-Inhalte tatsächlich (sonst wäre die Suche oben wertlos).
    assert!(mock
        .requests()
        .iter()
        .any(|r| r.body.contains("MOCK-SCENARIO:reasoning=40")));
}

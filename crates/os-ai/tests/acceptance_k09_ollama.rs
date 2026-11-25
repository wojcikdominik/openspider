//! Unabhängige Abnahmetests M4-K09 für AI-006 (Nachtrag 24.11.2026, Q00-H1): nur `model-default` (kein
//! Reasoning-Parameter), `reasoning_tokens` bleibt `null`, `done_reason: "length"` als Abbruch an der Ausgabegrenze mit
//! Grund `length`, genau 1 Request, Kosten 0; keine Anbieter-Request-ID.
mod acceptance_k09_support;

use acceptance_k09_support::{
    contains_key, fast_limiter, model, ok_schema, request, Mock, ALL_LEVELS,
};
use os_ai::adapters::ollama;
use os_ai::provider::{AiProvider, ReasoningLevel};
use os_ai::schema::RawAnswer;
use serde_json::Value;

const CHAT: &str = "/ollama/api/chat";

fn req(text: &str, level: ReasoningLevel, schema: Option<Value>) -> os_ai::provider::Request {
    request(
        model("ollama", "mock-llama", ollama::ENDPOINT, Vec::new()),
        text,
        schema,
        level,
        Some(128),
    )
}

// spec: AI-006 [M4]
// acceptance: independent
// Für Ollama gilt nur `model-default`: bei jeder Stufe geht kein Reasoning-Parameter hinaus; Modelle melden keine
// Stufen.
#[tokio::test]
async fn no_reasoning_parameter_at_any_level() {
    let mock = Mock::start().await;
    let adapter = mock.ollama();
    for level in ALL_LEVELS {
        let reply = adapter
            .send(&req(&format!("K09 ollama {level}"), level, None), None)
            .await
            .expect("Mock antwortet");
        assert!(!reply.is_output_limit());
    }
    let bodies = mock.bodies(CHAT);
    assert_eq!(bodies.len(), ALL_LEVELS.len());
    for body in &bodies {
        for key in [
            "think",
            "reasoning",
            "reasoning_effort",
            "thinking",
            "effort",
        ] {
            assert!(!contains_key(body, key), "{key}: {body}");
        }
    }
    let models = adapter.models().await.expect("Modellliste");
    assert!(!models.is_empty());
    assert!(models.iter().all(|m| m.reasoning_levels.is_empty()));
    assert!(adapter.to_model("mock-llama").reasoning_levels.is_empty());
}

// spec: AI-006 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// `reasoning_tokens` bleibt `null` (auch wenn ein Reasoning-Szenario gesetzt ist); keine Anbieter-Request-ID.
#[tokio::test]
async fn reasoning_tokens_stay_null_and_no_request_id() {
    let mock = Mock::start().await;
    let adapter = mock.ollama();
    for text in [
        "K09 ollama plain",
        "K09 ollama MOCK-SCENARIO:reasoning=40 x",
    ] {
        let reply = adapter
            .send(
                &req(text, ReasoningLevel::ModelDefault, Some(ok_schema())),
                None,
            )
            .await
            .unwrap();
        assert_eq!(reply.usage.reasoning_tokens, None, "{text}");
        assert!(reply.usage.output_tokens.is_some());
        assert_eq!(reply.provider_request_id, None);
    }
}

// spec: AI-006 [M4]
// acceptance: independent
// Kriterium: Ollama-Szenario `output-limit` → Abbruch an der Ausgabegrenze, `Error Code = length`, genau 1 Request,
// Kosten 0; der abgeschnittene Text wird nicht übernommen (`eval_count` = 16 aus dem Mock).
#[tokio::test]
async fn output_limit_is_length_with_one_request_and_no_cost() {
    let mock = Mock::start().await;
    let adapter = mock.ollama();
    let limiter = fast_limiter();
    for (text, schema) in [
        (
            "K09 ollama MOCK-SCENARIO:output-limit schema",
            Some(ok_schema()),
        ),
        ("K09 ollama MOCK-SCENARIO:output-limit free", None),
    ] {
        let execution = adapter
            .run(&limiter, &req(text, ReasoningLevel::Off, schema), None, 10)
            .await;
        assert_eq!(execution.attempts, 1, "{text}");
        let reply = execution.result.expect("HTTP 200");
        assert_eq!(reply.output_limit.as_deref(), Some("length"), "{text}");
        assert_eq!(reply.usage.output_tokens, Some(16));
        assert!(
            !matches!(reply.raw, RawAnswer::Text(_) | RawAnswer::Json(_)),
            "{text}"
        );
        let limit = reply
            .clone()
            .for_repair()
            .expect_err("kein Reparaturversuch");
        assert_eq!(limit.reason, "length");
        assert_eq!(
            limit.attempt_failure().error_code.as_deref(),
            Some("length")
        );
        assert!(reply.into_response(None).is_err(), "kein Teilergebnis");
        let sent = mock
            .bodies(CHAT)
            .into_iter()
            .filter(|b| b.to_string().contains(text))
            .count();
        assert_eq!(sent, 1, "{text}");
    }
    let rates = ollama::rates();
    assert_eq!(
        (
            rates.input_pico,
            rates.output_pico,
            rates.per_request_pico,
            rates.per_image_pico
        ),
        (0, 0, 0, 0),
        "Kosten 0"
    );
}

// spec: AI-001 [M4]
// spec: AI-006 [M4]
// acceptance: independent
// Ollama meldet keine Anbieter-Request-ID – auch im Fehlerweg bleibt sie leer.
#[tokio::test]
async fn ollama_errors_have_no_request_id() {
    let mock = Mock::start().await;
    let failure = mock
        .ollama()
        .send(
            &req(
                "K09 MOCK-SCENARIO:500-always o",
                ReasoningLevel::ModelDefault,
                None,
            ),
            None,
        )
        .await
        .expect_err("500");
    assert_eq!(failure.provider_request_id, None);
}

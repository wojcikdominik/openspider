//! Unabhängige Abnahmetests M4-K09 für AI-003 (Nachtrag 24.11.2026, Q00-H1, H2, N1): `store: false` in jedem
//! Responses-Request, nie `prompt_cache_retention`, Reasoning-Stufe nur mit Katalogstufen, Abbruch an der Ausgabegrenze
//! ohne Reparaturversuch, `reasoning_tokens` und `cache_write_tokens`, Verwerfen der `reasoning`-Elemente,
//! Anbieter-Request-ID aus `x-request-id` (AI-001 Nachtrag).
mod acceptance_k09_support;

use std::sync::Arc;

use acceptance_k09_support::{
    contains_key, fast_limiter, has_reasoning_marker, model, ok_schema, openai_http, request,
    Canned, Mock, Stub, ALL_LEVELS,
};
use os_ai::adapters::openai_responses::{OpenAiResponses, ResponsesOutput};
use os_ai::provider::{AiProvider, Input, ReasoningLevel, Request};
use os_ai::schema::{RawAnswer, RepairAttempts, ResponseSchema, StructuredOutcome};
use serde_json::{json, Value};
use ReasoningLevel::*;

const RESPONSES: &str = "/openai/v1/responses";
const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn req(levels: Vec<ReasoningLevel>, text: &str, level: ReasoningLevel) -> Request {
    request(
        model("openai", "gpt-6-luna", "responses", levels),
        text,
        None,
        level,
        Some(512),
    )
}

/// Katalogstufen aus `reasoning_efforts = ["none", "low", "medium", "high"]` (`none` → `off`).
fn catalog_levels() -> Vec<ReasoningLevel> {
    vec![Off, Low, Medium, High]
}

fn assert_store_false_everywhere(bodies: &[Value]) {
    assert!(!bodies.is_empty());
    for body in bodies {
        assert_eq!(body.get("store"), Some(&json!(false)), "{body}");
        assert!(!contains_key(body, "prompt_cache_retention"), "{body}");
        assert!(!contains_key(body, "previous_response_id"), "{body}");
    }
}

// spec: AI-003 [M4]
// acceptance: independent
// Kriterium (Property über Responses-Szenarien): jeder Request an `/openai/v1/responses` enthält `"store": false` – auch
// Reparaturversuche (`malformed-json`), Schema-, Bild-, Refusal-, Abbruch- und Wiederholungsfälle; nie
// `prompt_cache_retention` (V-A8: nichts senden) und kein `previous_response_id`.
#[tokio::test]
async fn store_false_in_every_responses_request() {
    let mock = Mock::start().await;
    let adapter = mock.responses();
    let limiter = fast_limiter();
    let schema = ResponseSchema::new(ok_schema()).unwrap();

    // Freitext, mehrere Szenarien.
    for (text, level) in [
        ("K09 plain", ModelDefault),
        ("K09 MOCK-SCENARIO:refusal r", Off),
        ("K09 MOCK-SCENARIO:reasoning=27 q", High),
        ("K09 MOCK-SCENARIO:output-limit o", Max),
        ("K09 MOCK-SCENARIO:429x2 w", Low),
    ] {
        let (_, _) = adapter
            .run(&limiter, &req(catalog_levels(), text, level), 10)
            .await;
    }
    // Schema-Aufgabe und Reparaturversuch nach unlesbarer Antwort.
    let structured = adapter
        .run_structured(
            &limiter,
            &req(catalog_levels(), "K09 schema", Off),
            &schema,
            RepairAttempts::new(1).unwrap(),
            10,
        )
        .await;
    assert!(matches!(structured.outcome, StructuredOutcome::Valid(_)));
    let repaired = adapter
        .run_structured(
            &limiter,
            &req(catalog_levels(), "K09 MOCK-SCENARIO:malformed-json m", Off),
            &schema,
            RepairAttempts::new(1).unwrap(),
            10,
        )
        .await;
    assert_eq!(repaired.requests.len(), 2, "erster Request + Reparatur");
    // Bildaufgabe.
    let mut image = req(catalog_levels(), "K09 image", Off);
    image.input.push(Input::Image {
        media_type: "image/png".into(),
        base64: PNG_1X1.into(),
    });
    let (result, _) = adapter.run(&limiter, &image, 10).await;
    assert!(result.is_ok(), "{result:?}");
    // Einzelversuch über das gemeinsame AiProvider-Interface.
    let mut contract = req(Vec::new(), "K09 contract", ModelDefault);
    contract.response_schema = Some(ok_schema());
    assert!(adapter.execute(contract).await.is_ok());

    let bodies = mock.bodies(RESPONSES);
    assert!(
        bodies.len() >= 11,
        "alle Versuche protokolliert: {}",
        bodies.len()
    );
    assert_store_false_everywhere(&bodies);
    assert!(bodies.iter().any(|b| contains_key(b, "input_image")));
    assert!(bodies.iter().any(|b| b.pointer("/text/format").is_some()));
}

// spec: AI-003 [M4]
// acceptance: independent
// Kriterium: Katalogmodell mit `reasoning_efforts = ["none", "low", "medium", "high"]`: `off` → `reasoning.effort =
// "none"`, `high` → `"high"`, `max` → `"high"`; ohne `reasoning_efforts` → kein Schlüssel `reasoning`; `model-default` →
// kein Schlüssel `reasoning`. Dazu: für `off` ohne `"none"` die niedrigste vorhandene Stufe.
#[tokio::test]
async fn reasoning_effort_only_with_catalog_levels() {
    let mock = Mock::start().await;
    let adapter = mock.responses();
    let limiter = fast_limiter();
    let cases: Vec<(Vec<ReasoningLevel>, ReasoningLevel, Option<&str>)> = vec![
        (catalog_levels(), Off, Some("none")),
        (catalog_levels(), High, Some("high")),
        (catalog_levels(), Max, Some("high")),
        (catalog_levels(), XHigh, Some("high")),
        (catalog_levels(), Low, Some("low")),
        (catalog_levels(), Medium, Some("medium")),
        // `minimal` fehlt im Katalog → nächstniedrigere vorhandene (`none`).
        (catalog_levels(), Minimal, Some("none")),
        (catalog_levels(), ModelDefault, None),
        (Vec::new(), Off, None),
        (Vec::new(), High, None),
        (Vec::new(), ModelDefault, None),
        (vec![Low, Medium, High], Off, Some("low")),
        (vec![Minimal, Low, Medium, High, XHigh], Max, Some("xhigh")),
    ];
    for (i, (levels, level, expected)) in cases.into_iter().enumerate() {
        let text = format!("K09 reasoning case {i}");
        let (result, _) = adapter
            .run(&limiter, &req(levels.clone(), &text, level), 10)
            .await;
        assert!(result.is_ok(), "{result:?}");
        let body = mock
            .bodies(RESPONSES)
            .into_iter()
            .find(|b| b.to_string().contains(&text))
            .expect("Request protokolliert");
        match expected {
            Some(effort) => assert_eq!(
                body.get("reasoning"),
                Some(&json!({"effort": effort})),
                "{levels:?} {level}: {body}"
            ),
            None => assert!(
                body.get("reasoning").is_none(),
                "{levels:?} {level}: {body}"
            ),
        }
        assert!(!contains_key(&body, "reasoning_effort"), "{body}");
        assert_eq!(body.get("store"), Some(&json!(false)));
    }
}

// spec: AI-003 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// Verfügbare Stufen eines OpenAI-Modells kommen aus dem Katalog (`with_reasoning_levels`) in die Modellbeschreibung;
// ohne Katalogeintrag leer (nur `model-default`).
#[tokio::test]
async fn model_list_carries_the_catalog_levels() {
    let mock = Mock::start().await;
    let adapter = mock.responses().with_reasoning_levels(Arc::new(|id: &str| {
        if id == "gpt-6-luna" {
            vec![Off, Low, Medium, High]
        } else {
            Vec::new()
        }
    }));
    let models = adapter.models().await.expect("Modellliste");
    let luna = models
        .iter()
        .find(|m| m.id == "gpt-6-luna")
        .expect("gpt-6-luna");
    assert_eq!(luna.reasoning_levels, vec![Off, Low, Medium, High]);
    assert!(models
        .iter()
        .filter(|m| m.id != "gpt-6-luna")
        .all(|m| m.reasoning_levels.is_empty()));
}

// spec: AI-003 [M4]
// acceptance: independent
// Kriterium: Szenario `output-limit` → Abbruch an der Ausgabegrenze, `Error Code = max_output_tokens`, genau 1 Request
// für den Eintrag (kein Reparaturversuch); die Ausgabegrenze geht als `max_output_tokens`.
#[tokio::test]
async fn output_limit_ends_the_entry_without_repair() {
    let mock = Mock::start().await;
    let adapter = mock.responses();
    let limiter = fast_limiter();
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let text = "K09 MOCK-SCENARIO:output-limit schema";
    let mut r = req(catalog_levels(), text, Off);
    r.max_tokens = Some(300);
    let run = adapter
        .run_structured(&limiter, &r, &schema, RepairAttempts::new(2).unwrap(), 10)
        .await;
    let StructuredOutcome::Failed(failure) = &run.outcome else {
        panic!("erwartet Abbruch ohne Reparatur: {:?}", run.outcome);
    };
    assert_eq!(
        failure.error.error_code.as_deref(),
        Some("max_output_tokens")
    );
    assert_eq!(failure.error.http_status, Some(200));
    assert_eq!(
        failure.meta.error_code.as_deref(),
        Some("max_output_tokens")
    );
    let usage = failure.meta.usage.clone().expect("Tokens gemeldet");
    assert_eq!(usage.output_tokens, Some(300));
    let sent: Vec<Value> = mock
        .bodies(RESPONSES)
        .into_iter()
        .filter(|b| b.to_string().contains(text))
        .collect();
    assert_eq!(sent.len(), 1, "kein Reparaturversuch");
    assert_eq!(sent[0]["max_output_tokens"], json!(300));
    assert!(!contains_key(&sent[0], "max_tokens"));

    // Freitext: Abbruch erkannt, kein Teilergebnis.
    let mut free = req(catalog_levels(), "K09 MOCK-SCENARIO:output-limit free", Off);
    free.max_tokens = Some(40);
    let (result, meta) = adapter.run(&limiter, &free, 10).await;
    let reply = result.expect("HTTP 200");
    assert_eq!(reply.output_limit(), Some("max_output_tokens"));
    assert!(!matches!(
        reply.raw_answer(),
        RawAnswer::Text(_) | RawAnswer::Json(_)
    ));
    assert_eq!(meta.error_code.as_deref(), Some("max_output_tokens"));
    assert_eq!(meta.attempts, 1);

    // Automatische Abbruchform: Reasoning über der Grenze bzw. abgeschnittene Nachricht ohne Reasoning.
    for (text, max) in [
        ("K09 MOCK-SCENARIO:reasoning=27 auto", 10u64),
        ("K09 short auto", 1),
    ] {
        let mut r = req(catalog_levels(), text, Low);
        r.max_tokens = Some(max);
        let (result, _) = adapter.run(&limiter, &r, 10).await;
        let reply = result.expect("HTTP 200");
        assert_eq!(reply.output_limit(), Some("max_output_tokens"), "{text}");
        assert!(matches!(reply.output, ResponsesOutput::OutputLimit(_)));
        assert!(
            adapter.execute(r.clone()).await.is_err(),
            "kein Teilergebnis: {text}"
        );
    }
}

// spec: AI-003 [M4]
// acceptance: independent
// Andere `incomplete_details.reason`-Werte bleiben bei der bisherigen Auslegung (unlesbar, Reparaturversuch, dann
// `Error: Invalid output`) – nur `max_output_tokens` ist der Abbruch an der Ausgabegrenze.
#[tokio::test]
async fn other_incomplete_reasons_keep_the_repair_attempt() {
    let stub = Stub::start(Canned::json(
        200,
        json!({"id": "resp_x", "object": "response", "status": "incomplete",
               "incomplete_details": {"reason": "content_filter"},
               "output": [{"type": "message", "role": "assistant",
                           "content": [{"type": "output_text", "text": "{\"ok\":"}]}],
               "usage": {"input_tokens": 5, "output_tokens": 2}}),
    ))
    .await;
    let adapter = OpenAiResponses::new(openai_http(&stub.url("/v1")));
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let run = adapter
        .run_structured(
            &fast_limiter(),
            &req(catalog_levels(), "x", Off),
            &schema,
            RepairAttempts::new(1).unwrap(),
            10,
        )
        .await;
    assert!(
        matches!(run.outcome, StructuredOutcome::InvalidOutput { .. }),
        "{:?}",
        run.outcome
    );
    assert_eq!(stub.bodies().len(), 2, "erster Request + Reparaturversuch");
    for body in stub.bodies() {
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body.get("store"), Some(&json!(false)));
    }
}

// spec: AI-003 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// Kriterium: Mock-`usage` mit `cache_write_tokens: 0` → 0 (nicht leer); Antwort ohne das Feld → leer; Szenario
// `reasoning=27` → `Reasoning Tokens = 27`. Das `reasoning`-Element ist kein Ergebnis.
#[tokio::test]
async fn usage_carries_reasoning_and_cache_write_tokens() {
    let mock = Mock::start().await;
    let adapter = mock.responses();
    let limiter = fast_limiter();
    let schema = ResponseSchema::new(ok_schema()).unwrap();

    let (plain, plain_meta) = adapter
        .run(&limiter, &req(catalog_levels(), "K09 usage plain", Off), 10)
        .await;
    let plain = plain.unwrap();
    let plain_usage = plain.usage.clone().expect("usage");
    assert_eq!(plain_usage.cache_write_tokens, Some(0));
    assert_eq!(plain_usage.cached_input_tokens, Some(0));
    assert_eq!(plain_usage.reasoning_tokens, Some(0));
    assert_eq!(plain_meta.usage, Some(plain_usage.clone()));

    let (thinking, _) = adapter
        .run(
            &limiter,
            &req(
                catalog_levels(),
                "K09 usage MOCK-SCENARIO:reasoning=27 x",
                Off,
            ),
            10,
        )
        .await;
    let thinking = thinking.unwrap();
    let usage = thinking.usage.clone().expect("usage");
    assert_eq!(usage.reasoning_tokens, Some(27));
    assert_eq!(
        usage.output_tokens,
        plain_usage.output_tokens.map(|o| o + 27)
    );
    assert!(
        !has_reasoning_marker(&format!("{thinking:?}")),
        "{thinking:?}"
    );
    assert_eq!(
        thinking.output, plain.output,
        "gleiches Ergebnis wie ohne Szenario"
    );

    let structured = adapter
        .run_structured(
            &limiter,
            &req(
                catalog_levels(),
                "K09 usage MOCK-SCENARIO:reasoning=27 s",
                Off,
            ),
            &schema,
            RepairAttempts::new(1).unwrap(),
            10,
        )
        .await;
    assert_eq!(
        structured.outcome,
        StructuredOutcome::Valid(json!({"ok": true}))
    );
    assert_eq!(structured.requests.len(), 1);

    let stub = Stub::start(Canned::json(
        200,
        json!({"id": "resp_x", "object": "response", "status": "completed",
               "output": [{"type": "message", "role": "assistant",
                           "content": [{"type": "output_text", "text": "hi"}]}],
               "usage": {"input_tokens": 5, "output_tokens": 2, "input_tokens_details": {"cached_tokens": 1}}}),
    ))
    .await;
    let (result, _) = OpenAiResponses::new(openai_http(&stub.url("/v1")))
        .run(&limiter, &req(Vec::new(), "x", ModelDefault), 10)
        .await;
    let usage = result.unwrap().usage.expect("usage");
    assert_eq!(usage.cached_input_tokens, Some(1));
    assert_eq!(usage.cache_write_tokens, None, "fehlendes Feld bleibt leer");
    assert_eq!(usage.reasoning_tokens, None, "fehlendes Feld bleibt leer");
}

// spec: AI-001 [M4]
// spec: AI-003 [M4]
// acceptance: independent
// Anbieter-Request-ID: Header `x-request-id`.
#[tokio::test]
async fn request_id_comes_from_x_request_id() {
    let stub = Stub::start(
        Canned::json(
            200,
            json!({"id": "resp_body_1", "object": "response", "status": "completed",
                   "output": [{"type": "message", "role": "assistant",
                               "content": [{"type": "output_text", "text": "hi"}]}]}),
        )
        .with_header("x-request-id", "req_mock_5"),
    )
    .await;
    let (result, meta) = OpenAiResponses::new(openai_http(&stub.url("/v1")))
        .run(&fast_limiter(), &req(Vec::new(), "x", ModelDefault), 10)
        .await;
    assert_eq!(
        result.unwrap().provider_request_id.as_deref(),
        Some("req_mock_5")
    );
    assert_eq!(meta.provider_request_id.as_deref(), Some("req_mock_5"));

    let mock = Mock::start().await;
    let (result, _) = mock
        .responses()
        .run(&fast_limiter(), &req(Vec::new(), "x", ModelDefault), 10)
        .await;
    let id = result.unwrap().provider_request_id.expect("Request-ID");
    assert!(id.starts_with("req_mock_"), "{id}");
}

// spec: AI-003 [M4]
// acceptance: independent
// Über alle Stufen und beide Katalogvarianten: `reasoning` ist entweder nicht vorhanden oder genau `{"effort": <Wert
// aus dem Katalog>}`; `store` immer `false`.
#[tokio::test]
async fn reasoning_values_are_always_catalog_values() {
    let mock = Mock::start().await;
    let adapter = mock.responses();
    let limiter = fast_limiter();
    let catalogs = [catalog_levels(), vec![Low, High], Vec::new(), vec![Max]];
    for (c, levels) in catalogs.iter().enumerate() {
        for level in ALL_LEVELS {
            let text = format!("K09 all {c} {level}");
            let (result, _) = adapter
                .run(&limiter, &req(levels.clone(), &text, level), 10)
                .await;
            assert!(result.is_ok());
            let body = mock
                .bodies(RESPONSES)
                .into_iter()
                .find(|b| b.to_string().contains(&text))
                .unwrap();
            assert_eq!(body.get("store"), Some(&json!(false)));
            match body.get("reasoning") {
                None => assert!(level == ModelDefault || levels.is_empty(), "{body}"),
                Some(reasoning) => {
                    let object = reasoning.as_object().expect("Objekt");
                    assert_eq!(object.len(), 1, "{body}");
                    let effort = reasoning["effort"].as_str().expect("effort");
                    let allowed: Vec<&str> = levels
                        .iter()
                        .map(|l| if *l == Off { "none" } else { l.as_str() })
                        .collect();
                    assert!(allowed.contains(&effort), "{effort} nicht in {allowed:?}");
                }
            }
        }
    }
}

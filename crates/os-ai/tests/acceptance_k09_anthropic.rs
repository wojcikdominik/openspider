//! Unabhängige Abnahmetests M4-K09 für AI-002 (Nachtrag 24.11.2026, Q00-H1): Thinking-Steuerung je Modell A–D,
//! `max_tokens` aus dem Request, `thinking_tokens` → `reasoning_tokens`, Verwerfen der `thinking`-Blöcke, Abbruch an
//! der Ausgabegrenze als eigene Antwortklasse ohne Reparaturversuch; Anbieter-Request-ID (AI-001 Nachtrag, N2).
//!
//! Den Statuswert `Error: Output limit reached` vergibt M4-K10; hier wird nur die Adapterebene geprüft.
mod acceptance_k09_support;

use acceptance_k09_support::{
    contains_key, has_reasoning_marker, model, ok_schema, request, Canned, Mock, Stub, ALL_LEVELS,
    ANTHROPIC_KEY, MODEL_A, MODEL_B, MODEL_C, MODEL_D,
};
use os_ai::adapters::anthropic::{
    self, AdapterReply, AnthropicAdapter, AnthropicConfig, ApiKey, OutputLimitReached,
};
use os_ai::provider::{AiProvider, ReasoningLevel, Request};
use os_ai::schema::{
    run_with_repair, RawAnswer, RepairAttempts, ResponseSchema, StructuredOutcome,
};
use serde_json::{json, Value};
use ReasoningLevel::*;

const MESSAGES: &str = "/anthropic/v1/messages";

/// Modell-IDs der Mock-Liste nach Rolle (`display_name` A–D).
async fn ids(mock: &Mock) -> [String; 4] {
    let models = mock
        .anthropic()
        .list_models()
        .await
        .expect("Modellliste lädt");
    let id = |role: &str| {
        models
            .iter()
            .find(|m| m.display_name.as_deref() == Some(role))
            .unwrap_or_else(|| panic!("Mock-Modell {role} fehlt"))
            .id
            .clone()
    };
    [id(MODEL_A), id(MODEL_B), id(MODEL_C), id(MODEL_D)]
}

fn req(id: &str, text: &str, level: ReasoningLevel, schema: Option<Value>) -> Request {
    request(
        model("anthropic", id, "messages", Vec::new()),
        text,
        schema,
        level,
        Some(1024),
    )
}

/// Sendet einen Request und liefert den gesendeten Körper (letzter POST an `/v1/messages`).
async fn sent_body(mock: &Mock, adapter: &AnthropicAdapter, request: &Request) -> Value {
    adapter.send(request, None).await.expect("Mock antwortet");
    mock.bodies(MESSAGES).pop().expect("Request protokolliert")
}

fn effort(body: &Value) -> Option<&str> {
    body.pointer("/output_config/effort")
        .and_then(Value::as_str)
}

fn thinking_type(body: &Value) -> Option<&str> {
    body.pointer("/thinking/type").and_then(Value::as_str)
}

// spec: AI-002 [M4]
// acceptance: independent
// Kriterium: Stufe `off` – A → `thinking.type = "disabled"` ohne `output_config.effort`; B → `adaptive` mit
// `output_config.effort = "low"`; C → kein `thinking`, kein `effort`; D → keines der beiden Felder (Mock-Log).
#[tokio::test]
async fn off_per_model_a_to_d() {
    let mock = Mock::start().await;
    let [a, b, c, d] = ids(&mock).await;
    let adapter = mock.anthropic();

    let body = sent_body(&mock, &adapter, &req(&a, "off A", Off, None)).await;
    assert_eq!(body["thinking"], json!({"type": "disabled"}), "A: {body}");
    assert_eq!(effort(&body), None, "A: {body}");

    let body = sent_body(&mock, &adapter, &req(&b, "off B", Off, None)).await;
    assert_eq!(thinking_type(&body), Some("adaptive"), "B: {body}");
    assert_eq!(effort(&body), Some("low"), "B: {body}");

    for id in [&c, &d] {
        let body = sent_body(&mock, &adapter, &req(id, "off C/D", Off, None)).await;
        assert!(!contains_key(&body, "thinking"), "{id}: {body}");
        assert_eq!(effort(&body), None, "{id}: {body}");
    }
}

// spec: AI-002 [M4]
// acceptance: independent
// Kriterium: `medium` auf A → `adaptive` + `effort = "medium"`; `max` auf B → `effort = "xhigh"` (nächstniedrigere
// unterstützte Stufe); `model-default` → keines der beiden Felder.
#[tokio::test]
async fn medium_on_a_max_on_b_and_model_default() {
    let mock = Mock::start().await;
    let [a, b, c, d] = ids(&mock).await;
    let adapter = mock.anthropic();

    let body = sent_body(&mock, &adapter, &req(&a, "medium A", Medium, None)).await;
    assert_eq!(body["thinking"], json!({"type": "adaptive"}), "{body}");
    assert_eq!(effort(&body), Some("medium"), "{body}");

    let body = sent_body(&mock, &adapter, &req(&b, "max B", Max, None)).await;
    assert_eq!(thinking_type(&body), Some("adaptive"), "{body}");
    assert_eq!(effort(&body), Some("xhigh"), "{body}");

    // `minimal` gibt es bei B nicht, und darunter liegt keine verfügbare Stufe → niedrigste verfügbare (`low`).
    let body = sent_body(&mock, &adapter, &req(&b, "minimal B", Minimal, None)).await;
    assert_eq!(thinking_type(&body), Some("adaptive"), "{body}");
    assert_eq!(effort(&body), Some("low"), "{body}");

    for id in [&a, &b, &c, &d] {
        let body = sent_body(&mock, &adapter, &req(id, "default", ModelDefault, None)).await;
        assert!(!contains_key(&body, "thinking"), "{id}: {body}");
        assert_eq!(effort(&body), None, "{id}: {body}");
    }
}

// spec: AI-002 [M4]
// acceptance: independent
// Kriterium (Property über alle Modelle A–D und alle Stufen): nie `thinking.type = "enabled"`, nie `budget_tokens`;
// außerdem nie ein Top-Level-Feld `effort`, `thinking` nur `disabled`/`adaptive`, bei B nie `effort = "max"`, bei C und
// D nie `thinking` oder `output_config.effort`.
#[tokio::test]
async fn no_enabled_thinking_and_no_budget_over_all_models_and_levels() {
    let mock = Mock::start().await;
    let ids = ids(&mock).await;
    let adapter = mock.anthropic();
    for (role, id) in ids.iter().enumerate() {
        for level in ALL_LEVELS {
            for schema in [None, Some(ok_schema())] {
                let body = sent_body(
                    &mock,
                    &adapter,
                    &req(id, &format!("prop {role} {level}"), level, schema),
                )
                .await;
                assert!(!contains_key(&body, "budget_tokens"), "{body}");
                assert!(body.get("effort").is_none(), "Top-Level effort: {body}");
                if let Some(kind) = thinking_type(&body) {
                    assert!(matches!(kind, "disabled" | "adaptive"), "{body}");
                }
                assert_ne!(thinking_type(&body), Some("enabled"), "{body}");
                if role == 1 {
                    assert_ne!(effort(&body), Some("max"), "B: {body}");
                }
                if role >= 2 {
                    assert!(!contains_key(&body, "thinking"), "C/D: {body}");
                    assert_eq!(effort(&body), None, "C/D: {body}");
                }
                // `off` wird nur bei bestätigtem Abschalten zu `disabled`; dann ohne Aufwand.
                if thinking_type(&body) == Some("disabled") {
                    assert_eq!(effort(&body), None, "{body}");
                }
                if level == ModelDefault {
                    assert!(!contains_key(&body, "thinking"), "{body}");
                    assert_eq!(effort(&body), None, "{body}");
                }
            }
        }
    }
}

// spec: AI-002 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// Verfügbare Stufen live aus `capabilities.thinking`/`capabilities.effort`: A mit `off` (abschaltbar), B ohne `off` und
// ohne `max`, C und D nur `model-default` (leere Liste).
#[tokio::test]
async fn reasoning_levels_come_live_from_the_model_list() {
    let mock = Mock::start().await;
    let [a, b, c, d] = ids(&mock).await;
    let models = mock.anthropic().models().await.expect("Modelle");
    let levels = |id: &str| {
        models
            .iter()
            .find(|m| m.id == id)
            .expect("Modell")
            .reasoning_levels
            .clone()
    };
    assert_eq!(levels(&a), vec![Off, Low, Medium, High, XHigh, Max]);
    assert_eq!(levels(&b), vec![Low, Medium, High, XHigh]);
    assert!(levels(&c).is_empty());
    assert!(levels(&d).is_empty());
}

// spec: AI-002 [M4]
// acceptance: independent
// Ohne `capabilities.thinking` (bzw. `thinking.supported = false`) nie `thinking`; die Stufen richten sich allein nach
// `effort` (`off` nicht verfügbar → niedrigste Aufwandsstufe). `adaptive` wird nur gesendet, wenn
// `thinking.types.adaptive.supported = true`; `disabled` nur bei `thinking.types.disabled.supported = true`.
#[tokio::test]
async fn thinking_follows_the_reported_types_only() {
    let mock = Mock::start().await;
    let effort_node = json!({"supported": true, "low": {"supported": true}, "medium": {"supported": true},
                             "high": {"supported": true}});
    mock.mock.set_models(
        "anthropic",
        json!({"data": [
            {"id": "k09-no-thinking", "display_name": "F", "max_tokens": 8192,
             "capabilities": {"effort": effort_node, "structured_outputs": {"supported": true}}},
            {"id": "k09-thinking-unsupported", "display_name": "G", "max_tokens": 8192,
             "capabilities": {"effort": effort_node, "thinking": {"supported": false,
                 "types": {"adaptive": {"supported": true}, "disabled": {"supported": true}}}}},
            {"id": "k09-disabled-only", "display_name": "E", "max_tokens": 8192,
             "capabilities": {"effort": effort_node, "thinking": {"supported": true,
                 "types": {"adaptive": {"supported": false}, "disabled": {"supported": true},
                           "enabled": {"supported": false}}}}}
        ], "has_more": false}),
    );
    let adapter = mock.anthropic();
    for id in ["k09-no-thinking", "k09-thinking-unsupported"] {
        let body = sent_body(&mock, &adapter, &req(id, "off", Off, None)).await;
        assert!(!contains_key(&body, "thinking"), "{id}: {body}");
        assert_eq!(effort(&body), Some("low"), "{id}: {body}");
        let body = sent_body(&mock, &adapter, &req(id, "medium", Medium, None)).await;
        assert!(!contains_key(&body, "thinking"), "{id}: {body}");
        assert_eq!(effort(&body), Some("medium"), "{id}: {body}");
    }
    let body = sent_body(&mock, &adapter, &req("k09-disabled-only", "off", Off, None)).await;
    assert_eq!(body["thinking"], json!({"type": "disabled"}), "{body}");
    assert_eq!(effort(&body), None, "{body}");
    let body = sent_body(
        &mock,
        &adapter,
        &req("k09-disabled-only", "medium", Medium, None),
    )
    .await;
    assert!(
        !contains_key(&body, "thinking"),
        "adaptive nicht unterstützt: {body}"
    );
    assert_eq!(effort(&body), Some("medium"), "{body}");
}

// spec: AI-002 [M4]
// acceptance: independent
// `max_tokens` kommt aus dem Request (Standard nach AI-011 liefert der Aufrufer), nicht aus einem festen Adapterwert.
#[tokio::test]
async fn max_tokens_is_taken_from_the_request() {
    let mock = Mock::start().await;
    let [a, b, ..] = ids(&mock).await;
    let adapter = mock.anthropic();
    for (id, max) in [(&a, 1024u64), (&b, 2048), (&b, 4000), (&a, 576)] {
        let mut r = req(id, &format!("max {max}"), Off, None);
        r.max_tokens = Some(max);
        let body = sent_body(&mock, &adapter, &r).await;
        assert_eq!(body["max_tokens"], json!(max), "{body}");
    }
}

// spec: AI-002 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// Kriterium: Szenario `reasoning=40` → `Reasoning Tokens = 40`, `Output Tokens` = Antwort-Tokens + 40; ohne Szenario
// meldet der Mock 0 → 0 (nicht leer); der `thinking`-Block ist kein Ergebnis (gleiches geparstes Ergebnis).
#[tokio::test]
async fn thinking_tokens_become_reasoning_tokens_and_thinking_is_discarded() {
    let mock = Mock::start().await;
    let [a, ..] = ids(&mock).await;
    let adapter = mock.anthropic();
    let schema = ok_schema();

    let plain = adapter
        .send(
            &req(&a, "K09 plain", ModelDefault, Some(schema.clone())),
            None,
        )
        .await
        .unwrap();
    let thinking = adapter
        .send(
            &req(
                &a,
                "K09 MOCK-SCENARIO:reasoning=40 x",
                ModelDefault,
                Some(schema.clone()),
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(plain.usage.reasoning_tokens, Some(0));
    assert_eq!(thinking.usage.reasoning_tokens, Some(40));
    assert_eq!(
        thinking.usage.output_tokens,
        plain.usage.output_tokens.map(|o| o + 40)
    );
    assert!(!thinking.is_output_limit());
    assert!(
        !has_reasoning_marker(&format!("{thinking:?}")),
        "{thinking:?}"
    );
    let plain_response = plain.into_response(Some(&schema)).unwrap();
    let thinking_response = thinking.into_response(Some(&schema)).unwrap();
    assert_eq!(thinking_response.content, Some(json!({"ok": true})));
    assert_eq!(thinking_response.content, plain_response.content);
    assert!(!has_reasoning_marker(&format!("{thinking_response:?}")));

    // Freitext: Ergebnis ist nur der Textblock.
    let text = adapter
        .send(
            &req(&a, "K09 MOCK-SCENARIO:reasoning=40 y", ModelDefault, None),
            None,
        )
        .await
        .unwrap()
        .into_response(None)
        .unwrap();
    let content = text.content.unwrap().to_string();
    assert!(!has_reasoning_marker(&content), "{content}");
}

// spec: AI-002 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// Fehlt `usage.output_tokens_details.thinking_tokens`, ist `reasoning_tokens` `null`, nicht 0.
#[tokio::test]
async fn missing_thinking_tokens_stay_null() {
    let stub = Stub::start(Canned::json(
        200,
        json!({"id": "msg_x", "type": "message", "role": "assistant", "model": "m",
               "content": [{"type": "text", "text": "{\"ok\":true}"}], "stop_reason": "end_turn",
               "usage": {"input_tokens": 12, "output_tokens": 3}}),
    ))
    .await;
    let adapter = AnthropicAdapter::new(
        acceptance_k09_support::client(),
        AnthropicConfig::new(ApiKey::new(ANTHROPIC_KEY)).with_base_url(stub.url("")),
    );
    let reply = adapter
        .send(&req("m", "x", ModelDefault, None), None)
        .await
        .unwrap();
    assert_eq!(reply.usage.output_tokens, Some(3));
    assert_eq!(reply.usage.reasoning_tokens, None);
}

/// Ein Eintrag mit lokaler Validierung und einem erlaubten Reparaturversuch (AI-021), über `for_repair`.
async fn structured_entry(
    adapter: &AnthropicAdapter,
    request: &Request,
) -> StructuredOutcome<Box<OutputLimitReached>> {
    let schema = ResponseSchema::new(request.response_schema.clone().unwrap()).unwrap();
    run_with_repair(
        &schema,
        RepairAttempts::new(1).unwrap(),
        |repair| async move {
            let reply: AdapterReply = adapter
                .send(request, repair.as_ref())
                .await
                .expect("Mock antwortet");
            reply.for_repair()
        },
    )
    .await
    .outcome
}

// spec: AI-002 [M4]
// acceptance: independent
// Kriterium: Szenario `output-limit` bei einer Schema-Aufgabe → Abbruch an der Ausgabegrenze, Grund `max_tokens`,
// `HTTP Status = 200`, genau 1 Request (kein Reparaturversuch), `Output Tokens` = gesendetes `max_tokens`, die Tokens
// werden gemeldet (Kosten > 0 möglich), kein Teilergebnis; dasselbe bei einer Freitext-Aufgabe.
#[tokio::test]
async fn output_limit_is_its_own_class_without_repair() {
    let mock = Mock::start().await;
    let [a, ..] = ids(&mock).await;
    let adapter = mock.anthropic();

    let mut schema_task = req(
        &a,
        "K09 schema MOCK-SCENARIO:output-limit",
        Off,
        Some(ok_schema()),
    );
    schema_task.max_tokens = Some(300);
    let outcome = structured_entry(&adapter, &schema_task).await;
    let StructuredOutcome::Failed(limit) = outcome else {
        panic!("erwartet Abbruch an der Ausgabegrenze ohne Reparatur: {outcome:?}");
    };
    assert_eq!(limit.reason, "max_tokens");
    assert_eq!(limit.http_status, 200);
    assert_eq!(limit.usage.output_tokens, Some(300));
    assert!(limit.usage.input_tokens.unwrap_or(0) > 0);
    let failure = limit.attempt_failure();
    assert_eq!(failure.error_code.as_deref(), Some("max_tokens"));
    assert_eq!(failure.http_status, Some(200));
    let sent: Vec<Value> = mock
        .bodies(MESSAGES)
        .into_iter()
        .filter(|b| {
            b.to_string()
                .contains("K09 schema MOCK-SCENARIO:output-limit")
        })
        .collect();
    assert_eq!(sent.len(), 1, "kein Reparaturversuch");

    let mut text_task = req(&a, "K09 text MOCK-SCENARIO:output-limit", Off, None);
    text_task.max_tokens = Some(64);
    let reply = adapter.send(&text_task, None).await.unwrap();
    assert_eq!(reply.output_limit.as_deref(), Some("max_tokens"));
    assert!(reply.is_output_limit());
    assert!(!matches!(
        reply.raw,
        RawAnswer::Text(_) | RawAnswer::Json(_)
    ));
    assert_eq!(reply.usage.output_tokens, Some(64));
    assert!(reply.clone().for_repair().is_err());
    assert!(reply.into_response(None).is_err(), "kein Teilergebnis");
}

// spec: AI-002 [M4]
// acceptance: independent
// Automatische Abbruchform des Mocks: Reasoning größer als die Grenze bzw. ohne Reasoning ein abgeschnittener Text –
// beides wird als Abbruch erkannt, ein abgeschnittener Text nie teilweise übernommen.
#[tokio::test]
async fn truncated_answers_are_never_partial_results() {
    let mock = Mock::start().await;
    let [a, ..] = ids(&mock).await;
    let adapter = mock.anthropic();

    let mut thinking = req(&a, "K09 MOCK-SCENARIO:reasoning=40 limit", Low, None);
    thinking.max_tokens = Some(20);
    let reply = adapter.send(&thinking, None).await.unwrap();
    assert_eq!(reply.output_limit.as_deref(), Some("max_tokens"));
    assert_eq!(reply.usage.output_tokens, Some(20));
    assert!(!has_reasoning_marker(&format!("{reply:?}")));
    assert!(reply.into_response(None).is_err());

    let mut short = req(&a, "K09 short limit", Off, None);
    short.max_tokens = Some(1);
    let reply = adapter.send(&short, None).await.unwrap();
    assert_eq!(reply.stop_reason.as_deref(), Some("max_tokens"));
    assert_eq!(reply.output_limit.as_deref(), Some("max_tokens"));
    assert!(
        reply.clone().into_response(None).is_err(),
        "abgeschnittener Freitext wird nicht übernommen"
    );
    let mut schema_short = req(&a, "K09 short schema", Off, Some(ok_schema()));
    schema_short.max_tokens = Some(1);
    let outcome = structured_entry(&adapter, &schema_short).await;
    assert!(
        matches!(outcome, StructuredOutcome::Failed(ref l) if l.reason == "max_tokens"),
        "{outcome:?}"
    );
}

// spec: AI-001 [M4]
// spec: AI-002 [M4]
// acceptance: independent
// Kriterium: Mock-Antwort mit Header `request-id: req_mock_7` und `id: msg_mock_7` → `Provider Request ID = req_mock_7`
// (nie die Antwort-`id`); ohne Header keine Ersatz-ID aus `msg_…`.
#[tokio::test]
async fn request_id_is_the_header_never_the_message_id() {
    let body = json!({"id": "msg_mock_7", "type": "message", "role": "assistant", "model": "m",
                      "content": [{"type": "text", "text": "hi"}], "stop_reason": "end_turn",
                      "usage": {"input_tokens": 1, "output_tokens": 1}});
    let stub =
        Stub::start(Canned::json(200, body.clone()).with_header("request-id", "req_mock_7")).await;
    let adapter = AnthropicAdapter::new(
        acceptance_k09_support::client(),
        AnthropicConfig::new(ApiKey::new(ANTHROPIC_KEY)).with_base_url(stub.url("")),
    );
    let reply = adapter
        .send(&req("m", "x", ModelDefault, None), None)
        .await
        .unwrap();
    assert_eq!(reply.provider_request_id.as_deref(), Some("req_mock_7"));

    let stub = Stub::start(Canned::json(200, body)).await;
    let adapter = AnthropicAdapter::new(
        acceptance_k09_support::client(),
        AnthropicConfig::new(ApiKey::new(ANTHROPIC_KEY)).with_base_url(stub.url("")),
    );
    let reply = adapter
        .send(&req("m", "x", ModelDefault, None), None)
        .await
        .unwrap();
    assert_ne!(reply.provider_request_id.as_deref(), Some("msg_mock_7"));

    // Gegen den ai-mock: Header `request-id: req_mock_<n>`, nie `msg_mock_<n>`.
    let mock = Mock::start().await;
    let [a, ..] = ids(&mock).await;
    let reply = mock
        .anthropic()
        .send(&req(&a, "id", ModelDefault, None), None)
        .await
        .unwrap();
    let id = reply.provider_request_id.expect("Request-ID");
    assert!(id.starts_with("req_mock_"), "{id}");
}

// spec: AI-001 [M4]
// spec: AI-002 [M4]
// acceptance: independent
// Kriterium: Anthropic-Fehlerantwort ohne Header, aber mit `request_id` im Körper → dieser Wert. Geprüft an der
// Ermittlungsfunktion und am Fehlerergebnis eines Versuchs (die ID muss dort für das Protokoll ankommen).
#[tokio::test]
async fn error_request_id_falls_back_to_the_body() {
    let error_body = json!({"type": "error", "error": {"type": "invalid_request_error", "message": "bad"},
                            "request_id": "req_err_9"});
    let headers = reqwest::header::HeaderMap::new();
    assert_eq!(
        anthropic::request_id(&headers, &error_body).as_deref(),
        Some("req_err_9")
    );
    let mut with_header = reqwest::header::HeaderMap::new();
    with_header.insert("request-id", "req_hdr_1".parse().unwrap());
    assert_eq!(
        anthropic::request_id(&with_header, &error_body).as_deref(),
        Some("req_hdr_1")
    );

    let stub = Stub::start(Canned::json(400, error_body)).await;
    let adapter = AnthropicAdapter::new(
        acceptance_k09_support::client(),
        AnthropicConfig::new(ApiKey::new(ANTHROPIC_KEY)).with_base_url(stub.url("")),
    );
    let failure = adapter
        .send(&req("m", "x", ModelDefault, None), None)
        .await
        .expect_err("400");
    assert!(
        format!("{failure:?}").contains("req_err_9"),
        "Das Fehlerergebnis trägt die Anbieter-Request-ID nicht: {failure:?}"
    );
}

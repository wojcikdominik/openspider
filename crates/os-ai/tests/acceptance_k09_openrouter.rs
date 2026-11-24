//! Unabhängige Abnahmetests M4-K09 für AI-005 (Nachtrag 24.11.2026, Q00-H1, M3, M4, N3): Reasoning nach
//! `supported_parameters`/`supported_efforts`/`mandatory`, nie `reasoning_effort`/`include_reasoning`, Abbruch an der
//! Ausgabegrenze (`length`), `reasoning_tokens`/`cache_write_tokens`, `usage.cost`/`cost_details`/`is_byok` im Ergebnis,
//! erweiterte Preisfelder (`overrides`, `internal_reasoning`, Cache-Preise), Fehlerform „unbekanntes Modell“,
//! Request-ID `x-generation-id`.
//!
//! Die Verbuchung (`Price Source = reported`, `Computed Cost`, `cost_mismatch`) ist M4-K10 und hier nicht geprüft.
mod acceptance_k09_support;

use acceptance_k09_support::{
    contains_key, has_reasoning_marker, model, ok_schema, request, Canned, Mock, Stub, ALL_LEVELS,
    OPENROUTER_KEY,
};
use os_ai::adapters::openrouter::{
    AdapterReply, ApiKey, OpenRouterAdapter, OpenRouterConfig, OpenRouterModel, OutputLimitReached,
};
use os_ai::provider::{AiProvider, Input, ProviderError, ReasoningLevel, Request};
use os_ai::schema::{
    run_with_repair, RawAnswer, RepairAttempts, ResponseSchema, StructuredOutcome,
};
use os_ai::AiStatus;
use serde_json::{json, Value};
use ReasoningLevel::*;

const COMPLETIONS: &str = "/openrouter/api/v1/chat/completions";
const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// Rollen der Mock-Modellliste (08 Konventionen, Nachtrag Q00), ermittelt aus den Feldern der Liste selbst.
struct Roles {
    r1: String,
    r2: String,
    r3: String,
    lc: String,
    ir: String,
    vision: String,
    all: Vec<Value>,
}

fn params(m: &Value) -> Vec<&str> {
    m["supported_parameters"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

async fn roles(mock: &Mock) -> Roles {
    let list = mock.get_json("/openrouter/api/v1/models").await;
    let all = list["data"].as_array().cloned().expect("data");
    let find = |what: &str, f: &dyn Fn(&Value) -> bool| -> String {
        all.iter()
            .find(|m| f(m))
            .unwrap_or_else(|| panic!("Mock-Modell {what} fehlt"))["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let efforts = |m: &Value| -> Vec<String> {
        m.pointer("/reasoning/supported_efforts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    };
    Roles {
        r1: find("M-R1", &|m| {
            params(m).contains(&"reasoning")
                && efforts(m).iter().any(|e| e == "none")
                && m.pointer("/reasoning/mandatory") == Some(&json!(false))
        }),
        r2: find("M-R2", &|m| {
            params(m).contains(&"reasoning")
                && !efforts(m).iter().any(|e| e == "none")
                && m.pointer("/reasoning/mandatory") == Some(&json!(true))
        }),
        r3: find("M-R3", &|m| {
            !params(m).contains(&"reasoning") && params(m).contains(&"response_format")
        }),
        lc: find("M-LC", &|m| {
            m.pointer("/pricing/overrides")
                .and_then(Value::as_array)
                .is_some_and(|o| !o.is_empty())
        }),
        ir: find("M-IR", &|m| {
            m.pointer("/pricing/internal_reasoning").is_some()
        }),
        vision: find("Vision mit Gebühren", &|m| {
            m.pointer("/pricing/request") == Some(&json!("0.001"))
                && m.pointer("/pricing/image") == Some(&json!("0.002"))
        }),
        all,
    }
}

fn req(id: &str, text: &str, level: ReasoningLevel, schema: Option<Value>) -> Request {
    request(
        model("openrouter", id, "chat-completions", Vec::new()),
        text,
        schema,
        level,
        Some(1024),
    )
}

async fn sent_body(mock: &Mock, adapter: &OpenRouterAdapter, request: &Request) -> Value {
    adapter.send(request, None).await.expect("Mock antwortet");
    mock.bodies(COMPLETIONS)
        .pop()
        .expect("Request protokolliert")
}

/// Listenpreis je Token in µUSD aus einem Preisfeld (`"0.000002"` → 2); die Mock-Preise sind ganze µUSD.
fn micro(price: &Value) -> u64 {
    let text = price.as_str().unwrap_or("0");
    let (int, frac) = text.split_once('.').unwrap_or((text, ""));
    let mut frac: String = frac.chars().take(6).collect();
    while frac.len() < 6 {
        frac.push('0');
    }
    int.parse::<u64>().unwrap() * 1_000_000 + frac.parse::<u64>().unwrap()
}

fn pricing<'a>(roles: &'a Roles, id: &str) -> &'a Value {
    &roles.all.iter().find(|m| m["id"] == id).expect("Modell")["pricing"]
}

// spec: AI-005 [M4]
// acceptance: independent
// Kriterium: `off` auf `M-R1` → `reasoning.effort = "none"`; auf `M-R2` (`mandatory`, ohne `none`) → `"low"`; auf `M-R3`
// → kein Schlüssel `reasoning`; `model-default` → kein Schlüssel `reasoning`; kein Request enthält `reasoning_effort`
// oder `include_reasoning` (Mock-Log).
#[tokio::test]
async fn reasoning_follows_supported_efforts_and_mandatory() {
    let mock = Mock::start().await;
    let roles = roles(&mock).await;
    let adapter = mock.openrouter();
    let cases = [
        (&roles.r1, Off, Some("none")),
        (&roles.r2, Off, Some("low")),
        (&roles.r3, Off, None),
        (&roles.r1, ModelDefault, None),
        (&roles.r2, ModelDefault, None),
        (&roles.r1, High, Some("high")),
        (&roles.r1, Max, Some("high")),
        (&roles.r1, Minimal, Some("none")),
        (&roles.r2, Minimal, Some("low")),
        (&roles.r2, Medium, Some("medium")),
        (&roles.r3, High, None),
    ];
    for (id, level, expected) in cases {
        let body = sent_body(
            &mock,
            &adapter,
            &req(id, &format!("{id} {level}"), level, None),
        )
        .await;
        match expected {
            Some(effort) => assert_eq!(
                body.get("reasoning"),
                Some(&json!({"effort": effort})),
                "{id} {level}: {body}"
            ),
            None => assert!(body.get("reasoning").is_none(), "{id} {level}: {body}"),
        }
    }
    let models = adapter.models().await.unwrap();
    let levels = |id: &str| {
        models
            .iter()
            .find(|m| m.id == id)
            .unwrap()
            .reasoning_levels
            .clone()
    };
    assert_eq!(levels(&roles.r1), vec![Off, Low, Medium, High]);
    assert_eq!(levels(&roles.r2), vec![Low, Medium, High]);
    assert!(levels(&roles.r3).is_empty());
}

// spec: AI-005 [M4]
// acceptance: independent
// Property über alle Modelle der Liste und alle Stufen (mit und ohne Schema): nie `reasoning_effort`, nie
// `include_reasoning`; `reasoning` nur als `{"effort": <Wert aus supported_efforts>}` und nur bei Modellen mit
// `"reasoning"` in `supported_parameters`.
#[tokio::test]
async fn one_reasoning_form_over_all_models_and_levels() {
    let mock = Mock::start().await;
    let roles = roles(&mock).await;
    let adapter = mock.openrouter();
    for m in &roles.all {
        let id = m["id"].as_str().unwrap();
        for level in ALL_LEVELS {
            for schema in [None, Some(ok_schema())] {
                let body = sent_body(&mock, &adapter, &req(id, "prop", level, schema)).await;
                assert!(!contains_key(&body, "reasoning_effort"), "{body}");
                assert!(!contains_key(&body, "include_reasoning"), "{body}");
                if let Some(reasoning) = body.get("reasoning") {
                    assert!(params(m).contains(&"reasoning"), "{id}: {body}");
                    assert_ne!(level, ModelDefault, "{body}");
                    assert_eq!(reasoning.as_object().unwrap().len(), 1, "{body}");
                    let effort = reasoning["effort"].as_str().unwrap();
                    let supported = m.pointer("/reasoning/supported_efforts").unwrap();
                    assert!(
                        supported.as_array().unwrap().contains(&json!(effort)),
                        "{effort}: {body}"
                    );
                    if m.pointer("/reasoning/mandatory") == Some(&json!(true)) {
                        assert_ne!(effort, "none", "{body}");
                    }
                }
            }
        }
    }
}

async fn structured_entry(
    adapter: &OpenRouterAdapter,
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

// spec: AI-005 [M4]
// acceptance: independent
// Kriterium: Szenario `output-limit` → Abbruch an der Ausgabegrenze, `Error Code = length`, genau 1 Request für den
// Eintrag; `content: null` bzw. abgeschnittener Text wird nie teilweise übernommen.
#[tokio::test]
async fn output_limit_is_length_without_repair() {
    let mock = Mock::start().await;
    let roles = roles(&mock).await;
    let adapter = mock.openrouter();
    let text = "K09 OR MOCK-SCENARIO:output-limit schema";
    let mut r = req(&roles.r1, text, Off, Some(ok_schema()));
    r.max_tokens = Some(200);
    let outcome = structured_entry(&adapter, &r).await;
    let StructuredOutcome::Failed(limit) = outcome else {
        panic!("erwartet Abbruch ohne Reparatur: {outcome:?}");
    };
    assert_eq!(limit.reason, "length");
    assert_eq!(limit.http_status, 200);
    assert_eq!(limit.usage.output_tokens, Some(200));
    assert_eq!(
        limit.attempt_failure().error_code.as_deref(),
        Some("length")
    );
    let sent = mock
        .bodies(COMPLETIONS)
        .into_iter()
        .filter(|b| b.to_string().contains(text))
        .count();
    assert_eq!(sent, 1, "kein Reparaturversuch");

    for (text, max) in [
        ("K09 OR MOCK-SCENARIO:output-limit free", 50u64),
        ("K09 OR MOCK-SCENARIO:reasoning=40 auto", 20),
        ("K09 OR short", 1),
    ] {
        let mut r = req(&roles.r1, text, Low, None);
        r.max_tokens = Some(max);
        let reply = adapter.send(&r, None).await.unwrap();
        assert_eq!(reply.output_limit.as_deref(), Some("length"), "{text}");
        assert!(
            !matches!(reply.raw, RawAnswer::Text(_) | RawAnswer::Json(_)),
            "{text}"
        );
        assert!(!has_reasoning_marker(&format!("{reply:?}")), "{reply:?}");
        assert!(
            reply.into_response(None).is_err(),
            "kein Teilergebnis: {text}"
        );
    }
}

// spec: AI-005 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// `usage.completion_tokens_details.reasoning_tokens` → `reasoning_tokens`; `prompt_tokens_details.cached_tokens` bzw.
// `cache_write_tokens` → `cached_input_tokens` bzw. `cache_write_tokens`; fehlende Felder `null`. `message.reasoning`
// und `message.reasoning_details` sind kein Ergebnis.
#[tokio::test]
async fn usage_fields_and_discarded_reasoning() {
    let mock = Mock::start().await;
    let roles = roles(&mock).await;
    let adapter = mock.openrouter();
    let schema = ok_schema();
    let plain = adapter
        .send(
            &req(
                &roles.r3,
                "K09 OR plain",
                ModelDefault,
                Some(schema.clone()),
            ),
            None,
        )
        .await
        .unwrap();
    let thinking = adapter
        .send(
            &req(
                &roles.r3,
                "K09 OR MOCK-SCENARIO:reasoning=40 x",
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
    assert!(
        !has_reasoning_marker(&format!("{thinking:?}")),
        "{thinking:?}"
    );
    let a = plain.into_response(Some(&schema)).unwrap();
    let b = thinking.into_response(Some(&schema)).unwrap();
    assert_eq!(b.content, Some(json!({"ok": true})));
    assert_eq!(a.content, b.content);

    let stub = Stub::start(Canned::json(
        200,
        json!({"id": "gen-1", "object": "chat.completion", "model": "m",
               "choices": [{"index": 0, "finish_reason": "stop",
                            "message": {"role": "assistant", "content": "hi"}}],
               "usage": {"prompt_tokens": 50, "completion_tokens": 3,
                         "prompt_tokens_details": {"cached_tokens": 5, "cache_write_tokens": 7}}}),
    ))
    .await;
    let reply = stub_adapter(&stub)
        .send(&req("m", "x", ModelDefault, None), None)
        .await
        .unwrap();
    assert_eq!(reply.usage.cached_input_tokens, Some(5));
    assert_eq!(reply.usage.cache_write_tokens, Some(7));
    assert_eq!(
        reply.usage.reasoning_tokens, None,
        "fehlendes Feld bleibt leer"
    );
}

fn stub_adapter(stub: &Stub) -> OpenRouterAdapter {
    OpenRouterAdapter::new(
        acceptance_k09_support::client(),
        OpenRouterConfig::new(ApiKey::new(OPENROUTER_KEY)).with_base_url(stub.url("")),
    )
}

// spec: AI-005 [M4]
// acceptance: independent
// Ist-Kosten im Ergebnis (Q00-M3; Verbuchung K10): Standardantwort → `usage.cost` = Listenpreis × Tokens (auf µUSD),
// `is_byok = false`, verwertbar; `cost-mismatch` → gemeldeter doppelter Betrag; `no-cost` → kein verwertbarer Betrag;
// `byok` → `is_byok = true`, nicht verwertbar (lokale Rechnung), `upstream_inference_cost` = berechneter Wert.
#[tokio::test]
async fn reported_cost_is_passed_through() {
    let mock = Mock::start().await;
    let roles = roles(&mock).await;
    let adapter = mock.openrouter();
    let prices = pricing(&roles, &roles.r3).clone();
    let computed = |reply: &AdapterReply| {
        reply.usage.input_tokens.unwrap() * micro(&prices["prompt"])
            + reply.usage.output_tokens.unwrap() * micro(&prices["completion"])
    };

    let reply = adapter
        .send(
            &req(&roles.r3, "K09 cost standard", ModelDefault, None),
            None,
        )
        .await
        .unwrap();
    let cost = reply.reported_cost.expect("usage.cost gemeldet");
    assert_eq!(cost.cost_micro_usd, Some(computed(&reply)));
    assert_eq!(
        cost.upstream_inference_cost_micro_usd,
        Some(computed(&reply))
    );
    assert_eq!(cost.is_byok, Some(false));
    assert_eq!(cost.usable(), Some(computed(&reply)));

    let reply = adapter
        .send(
            &req(
                &roles.r3,
                "K09 cost MOCK-SCENARIO:cost-mismatch",
                ModelDefault,
                None,
            ),
            None,
        )
        .await
        .unwrap();
    let cost = reply.reported_cost.expect("usage.cost gemeldet");
    assert_eq!(cost.usable(), Some(2 * computed(&reply)));

    let reply = adapter
        .send(
            &req(
                &roles.r3,
                "K09 cost MOCK-SCENARIO:no-cost",
                ModelDefault,
                None,
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(reply.reported_cost.and_then(|c| c.usable()), None);
    assert_eq!(reply.reported_cost.and_then(|c| c.cost_micro_usd), None);

    let reply = adapter
        .send(
            &req(&roles.r3, "K09 cost MOCK-SCENARIO:byok", ModelDefault, None),
            None,
        )
        .await
        .unwrap();
    let cost = reply.reported_cost.expect("Felder gemeldet");
    assert_eq!(cost.is_byok, Some(true));
    assert_eq!(cost.usable(), None, "BYOK → lokale Rechnung");
    assert_eq!(
        cost.upstream_inference_cost_micro_usd,
        Some(computed(&reply))
    );

    // Bildrequest über das Modell mit `pricing.request`/`pricing.image`: Gebühren sind im gemeldeten Betrag enthalten.
    let mut image = req(&roles.vision, "K09 cost image", ModelDefault, None);
    image.input.push(Input::Image {
        media_type: "image/png".into(),
        base64: PNG_1X1.into(),
    });
    let vision_prices = pricing(&roles, &roles.vision).clone();
    let reply = adapter.send(&image, None).await.unwrap();
    let tokens = reply.usage.input_tokens.unwrap() * micro(&vision_prices["prompt"])
        + reply.usage.output_tokens.unwrap() * micro(&vision_prices["completion"]);
    assert_eq!(
        reply.reported_cost.and_then(|c| c.usable()),
        Some(tokens + 1_000 + 2_000)
    );

    // Langkontext-Modell: der Mock rechnet ab `min_prompt_tokens` mit dem `overrides`-Eintrag.
    let long_text = format!("K09 long {}", "x".repeat(4_200));
    let reply = adapter
        .send(&req(&roles.lc, &long_text, ModelDefault, None), None)
        .await
        .unwrap();
    let input = reply.usage.input_tokens.unwrap();
    assert!(input >= 1_000, "{input}");
    assert_eq!(
        reply.reported_cost.and_then(|c| c.usable()),
        Some(input * 2 + reply.usage.output_tokens.unwrap() * 4)
    );
}

// spec: AI-005 [M4]
// acceptance: independent
// Ist-Kosten: Rundung kaufmännisch auf µUSD; negative oder nicht numerische Werte sind nicht verwertbar.
#[tokio::test]
async fn reported_cost_rounding_and_invalid_values() {
    for (cost, expected) in [
        (json!(0.0000058), Some(6u64)),
        (json!(0.0001809), Some(181)),
        (json!(0.0000014), Some(1)),
        (json!(0), Some(0)),
        (json!(-0.5), None),
        (json!("0.001"), None),
    ] {
        let stub = Stub::start(Canned::json(
            200,
            json!({"id": "gen-1", "object": "chat.completion", "model": "m",
                   "choices": [{"index": 0, "finish_reason": "stop",
                                "message": {"role": "assistant", "content": "hi"}}],
                   "usage": {"prompt_tokens": 5, "completion_tokens": 1, "cost": cost, "is_byok": false}}),
        ))
        .await;
        let reply = stub_adapter(&stub)
            .send(&req("m", "x", ModelDefault, None), None)
            .await
            .unwrap();
        assert_eq!(
            reply.reported_cost.and_then(|c| c.usable()),
            expected,
            "cost = {cost}"
        );
    }
}

// spec: AI-005 [M4]
// acceptance: independent
// Preisfelder (Q00-M4): `overrides` (ab ≥ `min_prompt_tokens`, fehlende Felder behalten den Grundpreis, bei mehreren
// der größte zutreffende), `internal_reasoning` (vorsichtig: max mit `completion`), `input_cache_read`,
// `input_cache_write`; `Long Context From` = kleinstes `min_prompt_tokens`. Prüfwerte aus AI-005: `M-LC` 1.000/100 →
// 2.400 µUSD, 999/100 → 1.199 µUSD; `M-IR` mit 1.000 Ausgabe-Tokens → 10.000 µUSD (nicht 2.000).
#[tokio::test]
async fn extended_price_fields_are_read() {
    let mock = Mock::start().await;
    let roles = roles(&mock).await;
    let models = mock.openrouter().list_models().await.unwrap();
    let get = |id: &str| -> OpenRouterModel { models.iter().find(|m| m.id == id).unwrap().clone() };

    let lc = get(&roles.lc);
    assert_eq!(lc.long_context_from(), Some(1_000));
    let cost = |m: &OpenRouterModel, input: u64, output: u64| -> u64 {
        let rates = m.rates_for(input).expect("Preise lesbar");
        (input * rates.input_pico + output * rates.output_pico) / 1_000_000
    };
    assert_eq!(cost(&lc, 1_000, 100), 2_400);
    assert_eq!(cost(&lc, 999, 100), 1_199);

    let ir = get(&roles.ir);
    let per_token = ir.reserve_output_pico(10).expect("Preise lesbar");
    assert_eq!(1_000 * per_token / 1_000_000, 10_000);
    assert_eq!(
        ir.rates_for(10).unwrap().output_pico,
        2_000_000,
        "Abrechnung bleibt bei completion"
    );

    // Eigene Liste: Cache-Preise, mehrere Overrides, Teilfelder.
    mock.mock.set_models(
        "openrouter",
        json!({"data": [{"id": "k09/prices", "name": "prices", "supported_parameters": ["response_format"],
            "pricing": {"prompt": "0.000001", "completion": "0.000002", "input_cache_read": "0.0000001",
                        "input_cache_write": "0.00000125", "internal_reasoning": "0.000001",
                        "overrides": [
                            {"min_prompt_tokens": 5000, "prompt": "0.000004"},
                            {"min_prompt_tokens": 2000, "prompt": "0.000003", "completion": "0.000006",
                             "input_cache_read": "0.0000003"}
                        ]}}]}),
    );
    let models = mock.openrouter().list_models().await.unwrap();
    let m = models.iter().find(|m| m.id == "k09/prices").unwrap();
    assert_eq!(m.long_context_from(), Some(2_000));
    let base = m.rates_for(1_999).unwrap();
    assert_eq!(
        (
            base.input_pico,
            base.output_pico,
            base.cache_read_pico,
            base.cache_write_pico
        ),
        (1_000_000, 2_000_000, 100_000, 1_250_000)
    );
    let at = m.rates_for(2_000).unwrap();
    assert_eq!(
        (
            at.input_pico,
            at.output_pico,
            at.cache_read_pico,
            at.cache_write_pico
        ),
        (3_000_000, 6_000_000, 300_000, 1_250_000)
    );
    let high = m.rates_for(5_000).unwrap();
    assert_eq!((high.input_pico, high.output_pico), (4_000_000, 2_000_000));
    // `internal_reasoning` kleiner als `completion` → `completion`.
    assert_eq!(m.reserve_output_pico(10), Some(2_000_000));
}

// spec: AI-005 [M4]
// acceptance: independent
// Kriterium: Szenario `unknown-model` → `Error: 400 <model> is not a valid model ID`, `Error Code` leer, kein
// `Quota exceeded`, kein Einfluss auf andere Einträge; `mock-user` steht nirgends im Ergebnis.
#[tokio::test]
async fn unknown_model_is_a_plain_400() {
    let mock = Mock::start().await;
    let roles = roles(&mock).await;
    let adapter = mock.openrouter();
    mock.scenario("openrouter", "unknown-model", 1).await;
    let failure = adapter
        .send(
            &req("k09/no-such-model", "K09 unknown", ModelDefault, None),
            None,
        )
        .await
        .expect_err("400");
    let message = "k09/no-such-model is not a valid model ID";
    assert_eq!(
        failure.error,
        ProviderError::Http {
            status: 400,
            message: message.into()
        }
    );
    assert_eq!(failure.error_code, None);
    assert_eq!(
        failure.error.status(),
        Some(AiStatus::HttpError {
            code: 400,
            message: message.into()
        })
    );
    assert_eq!(
        failure.error.status().unwrap().to_string(),
        format!("Error: 400 {message}")
    );
    assert!(!format!("{failure:?}").contains("mock-user"), "{failure:?}");
    assert!(!adapter.quota().is_tripped(), "kein Quota exceeded");
    // Der nächste Eintrag desselben Laufs läuft normal.
    let next = adapter
        .send(
            &req(&roles.r3, "K09 after unknown", ModelDefault, None),
            None,
        )
        .await
        .expect("weiterer Eintrag");
    assert!(next.into_response(None).is_ok());
}

// spec: AI-001 [M4]
// spec: AI-005 [M4]
// acceptance: independent
// Kriterium: OpenRouter → Wert von `x-generation-id`, ersatzweise die Antwort-`id`.
#[tokio::test]
async fn request_id_from_generation_header_or_body() {
    let body = json!({"id": "gen-body-1", "object": "chat.completion", "model": "m",
                      "choices": [{"index": 0, "finish_reason": "stop",
                                   "message": {"role": "assistant", "content": "hi"}}],
                      "usage": {"prompt_tokens": 1, "completion_tokens": 1}});
    let stub =
        Stub::start(Canned::json(200, body.clone()).with_header("x-generation-id", "gen-hdr-9"))
            .await;
    let reply = stub_adapter(&stub)
        .send(&req("m", "x", ModelDefault, None), None)
        .await
        .unwrap();
    assert_eq!(reply.provider_request_id.as_deref(), Some("gen-hdr-9"));

    let stub = Stub::start(Canned::json(200, body)).await;
    let reply = stub_adapter(&stub)
        .send(&req("m", "x", ModelDefault, None), None)
        .await
        .unwrap();
    assert_eq!(reply.provider_request_id.as_deref(), Some("gen-body-1"));

    let mock = Mock::start().await;
    let roles = roles(&mock).await;
    let reply = mock
        .openrouter()
        .send(&req(&roles.r3, "id", ModelDefault, None), None)
        .await
        .unwrap();
    let id = reply.provider_request_id.expect("Request-ID");
    assert!(id.starts_with("gen-mock-"), "{id}");
}

//! Unabhängige Abnahmetests M4-K09 für die ai-mock-Erweiterungen (08 „Konventionen“, Nachtrag 24.11.2026, Q00):
//! Szenarien `reasoning=<n>`, `output-limit`, `no-cost`, `cost-mismatch`, `byok`, `unknown-model`, automatische
//! Abbruchform an der Ausgabegrenze, OpenRouter- und Anthropic-Modelllisten, Decisions-Zählung je Frage,
//! `cache_write_tokens`, Request-IDs und Request-Protokoll. Geprüft mit rohen HTTP-Requests gegen den Mock.
mod acceptance_k09_support;

use acceptance_k09_support::{days_from_civil, today_days, Mock};
use serde_json::{json, Value};

const MESSAGES: &str = "/anthropic/v1/messages";
const RESPONSES: &str = "/openai/v1/responses";
const DECISIONS: &str = "/openai/v1/decisions";
const COMPLETIONS: &str = "/openrouter/api/v1/chat/completions";
const OLLAMA_CHAT: &str = "/ollama/api/chat";
/// Standardantwort des Mocks ohne `MOCK-ID`: `{"ok":true}` = 11 Zeichen → ⌈11 / 4⌉ = 3 Ausgabe-Tokens.
const ANSWER_TOKENS: u64 = 3;

fn anthropic_body(text: &str, max_tokens: u64) -> Value {
    json!({"model": "mock-claude", "max_tokens": max_tokens,
           "messages": [{"role": "user", "content": [{"type": "text", "text": text}]}]})
}

fn responses_body(text: &str, max_output_tokens: Option<u64>) -> Value {
    let mut body = json!({"model": "gpt-6-luna", "store": false,
                          "input": [{"role": "user", "content": [{"type": "input_text", "text": text}]}]});
    if let Some(max) = max_output_tokens {
        body["max_output_tokens"] = json!(max);
    }
    body
}

fn openrouter_body(model: &str, text: &str, max_tokens: Option<u64>) -> Value {
    let mut body = json!({"model": model,
                          "messages": [{"role": "user", "content": [{"type": "text", "text": text}]}]});
    if let Some(max) = max_tokens {
        body["max_tokens"] = json!(max);
    }
    body
}

fn approx(value: &Value, expected: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|v| (v - expected).abs() < 1e-12 + expected.abs() * 1e-9)
}

fn header<'a>(headers: &'a reqwest::header::HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| panic!("Header {name} fehlt"))
}

// spec: AI-002 [M4]
// spec: AI-003 [M4]
// spec: AI-005 [M4]
// spec: AI-006 [M4]
// acceptance: independent
// `reasoning=<n>` (global und als Markierung): Ausgabe-Tokens = ⌈Zeichen / 4⌉ + n und die Anbieterform der
// Reasoning-Felder; ohne Szenario dieselben Felder mit 0; Ollama ohne Reasoning-Felder.
#[tokio::test]
async fn reasoning_scenario_forms_per_provider() {
    let mock = Mock::start().await;

    // Anthropic, Markierung.
    let (status, _, body) = mock
        .post(
            MESSAGES,
            anthropic_body("MOCK-SCENARIO:reasoning=12 a", 1000),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["content"][0],
        json!({"type": "thinking", "thinking": "MOCK-REASONING-TEXT", "signature": "MOCK-SIGNATURE"})
    );
    assert_eq!(body["content"][1]["type"], "text");
    assert_eq!(body["usage"]["output_tokens"], json!(ANSWER_TOKENS + 12));
    assert_eq!(
        body["usage"]["output_tokens_details"]["thinking_tokens"],
        json!(12)
    );
    let (_, _, plain) = mock.post(MESSAGES, anthropic_body("plain a", 1000)).await;
    assert_eq!(
        plain["usage"]["output_tokens_details"]["thinking_tokens"],
        json!(0)
    );
    assert_eq!(plain["usage"]["output_tokens"], json!(ANSWER_TOKENS));
    assert!(plain["content"]
        .as_array()
        .unwrap()
        .iter()
        .all(|b| b["type"] == "text"));

    // OpenAI Responses, globales Szenario.
    mock.scenario("openai", "reasoning=27", 1).await;
    let (_, _, body) = mock.post(RESPONSES, responses_body("o", Some(1000))).await;
    assert_eq!(
        body["output"][0],
        json!({"type": "reasoning", "summary": [], "encrypted_content": "MOCK-ENCRYPTED"})
    );
    assert_eq!(body["output"][1]["type"], "message");
    assert_eq!(body["usage"]["output_tokens"], json!(ANSWER_TOKENS + 27));
    assert_eq!(
        body["usage"]["output_tokens_details"]["reasoning_tokens"],
        json!(27)
    );
    let (_, _, plain) = mock.post(RESPONSES, responses_body("o", Some(1000))).await;
    assert_eq!(
        plain["usage"]["output_tokens_details"]["reasoning_tokens"],
        json!(0)
    );
    assert!(plain["output"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["type"] != "reasoning"));

    // OpenRouter.
    let (_, _, body) = mock
        .post(
            COMPLETIONS,
            openrouter_body("mock/text", "MOCK-SCENARIO:reasoning=40 r", Some(1000)),
        )
        .await;
    let message = &body["choices"][0]["message"];
    assert_eq!(message["reasoning"], json!("MOCK-REASONING-TEXT"));
    assert_eq!(
        message["reasoning_details"],
        json!([{"type": "reasoning.encrypted", "data": "MOCK-ENCRYPTED"}])
    );
    assert_eq!(
        body["usage"]["completion_tokens"],
        json!(ANSWER_TOKENS + 40)
    );
    assert_eq!(
        body["usage"]["completion_tokens_details"]["reasoning_tokens"],
        json!(40)
    );
    let (_, _, plain) = mock
        .post(
            COMPLETIONS,
            openrouter_body("mock/text", "plain r", Some(1000)),
        )
        .await;
    assert_eq!(
        plain["usage"]["completion_tokens_details"]["reasoning_tokens"],
        json!(0)
    );

    // Ollama: keine Reasoning-Felder.
    let (_, _, body) = mock
        .post(
            OLLAMA_CHAT,
            json!({"model": "mock-llama", "stream": false,
                   "messages": [{"role": "user", "content": "MOCK-SCENARIO:reasoning=40 l"}]}),
        )
        .await;
    let text = body.to_string();
    assert!(
        !text.contains("reasoning") && !text.contains("thinking"),
        "{text}"
    );
    assert_eq!(body["eval_count"], json!(ANSWER_TOKENS));
}

// spec: AI-002 [M4]
// spec: AI-003 [M4]
// spec: AI-005 [M4]
// acceptance: independent
// Automatische Abbruchform: Ist ⌈Zeichen / 4⌉ + n größer als die gesendete Grenze, antwortet der Mock in der
// Abbruchform (Ausgabe-Tokens = Grenze); gleich der Grenze ist kein Abbruch. Mit n > 0 nur das Reasoning-Element, mit
// n = 0 ein abgeschnittener Text.
#[tokio::test]
async fn automatic_output_limit_forms() {
    let mock = Mock::start().await;

    // Anthropic: Grenze = Antwort + n → kein Abbruch; eins weniger → Abbruch nur mit `thinking`-Block.
    let (_, _, ok) = mock
        .post(
            MESSAGES,
            anthropic_body("MOCK-SCENARIO:reasoning=40 g", ANSWER_TOKENS + 40),
        )
        .await;
    assert_eq!(ok["stop_reason"], "end_turn");
    let (status, _, cut) = mock
        .post(
            MESSAGES,
            anthropic_body("MOCK-SCENARIO:reasoning=40 h", ANSWER_TOKENS + 39),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(cut["stop_reason"], "max_tokens");
    assert_eq!(cut["usage"]["output_tokens"], json!(ANSWER_TOKENS + 39));
    let blocks = cut["content"].as_array().unwrap();
    assert!(
        !blocks.is_empty() && blocks.iter().all(|b| b["type"] == "thinking"),
        "{cut}"
    );
    // n = 0: abgeschnittener Textblock.
    let (_, _, cut) = mock.post(MESSAGES, anthropic_body("short", 2)).await;
    assert_eq!(cut["stop_reason"], "max_tokens");
    assert_eq!(cut["usage"]["output_tokens"], json!(2));
    let text = cut["content"][0]["text"].as_str().expect("Textblock");
    assert!(text.len() < "{\"ok\":true}".len(), "{text}");
    assert!(
        serde_json::from_str::<Value>(text).is_err(),
        "abgeschnitten: {text}"
    );

    // OpenAI Responses: `max_output_tokens` zählt, `max_tokens` nicht.
    let (_, _, cut) = mock
        .post(
            RESPONSES,
            responses_body("MOCK-SCENARIO:reasoning=27 i", Some(20)),
        )
        .await;
    assert_eq!(cut["status"], "incomplete");
    assert_eq!(cut["incomplete_details"]["reason"], "max_output_tokens");
    assert_eq!(cut["usage"]["output_tokens"], json!(20));
    let items = cut["output"].as_array().unwrap();
    assert!(
        !items.is_empty() && items.iter().all(|i| i["type"] == "reasoning"),
        "{cut}"
    );
    let (_, _, cut) = mock.post(RESPONSES, responses_body("short", Some(1))).await;
    assert_eq!(cut["status"], "incomplete");
    let text = cut["output"][0]["content"][0]["text"]
        .as_str()
        .expect("Nachricht");
    assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
    let mut wrong_field = responses_body("short", None);
    wrong_field["max_tokens"] = json!(1);
    let (_, _, full) = mock.post(RESPONSES, wrong_field).await;
    assert_eq!(full["status"], "completed");

    // OpenRouter.
    let (_, _, cut) = mock
        .post(
            COMPLETIONS,
            openrouter_body("mock/text", "MOCK-SCENARIO:reasoning=40 j", Some(10)),
        )
        .await;
    assert_eq!(cut["choices"][0]["finish_reason"], "length");
    assert_eq!(cut["choices"][0]["message"]["content"], Value::Null);
    assert_eq!(cut["usage"]["completion_tokens"], json!(10));
    let (_, _, cut) = mock
        .post(COMPLETIONS, openrouter_body("mock/text", "short", Some(1)))
        .await;
    assert_eq!(cut["choices"][0]["finish_reason"], "length");
    let text = cut["choices"][0]["message"]["content"]
        .as_str()
        .expect("Text");
    assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
    let (_, _, ok) = mock
        .post(
            COMPLETIONS,
            openrouter_body("mock/text", "exact", Some(ANSWER_TOKENS)),
        )
        .await;
    assert_eq!(ok["choices"][0]["finish_reason"], "stop");
}

// spec: AI-002 [M4]
// spec: AI-003 [M4]
// spec: AI-005 [M4]
// spec: AI-006 [M4]
// acceptance: independent
// Szenario `output-limit` erzwingt die Abbruchform unabhängig von n (Reasoning-Tokens = Grenze); Ollama liefert
// `done_reason: "length"` mit `eval_count` = 16.
#[tokio::test]
async fn forced_output_limit_forms() {
    let mock = Mock::start().await;
    let (_, _, cut) = mock
        .post(MESSAGES, anthropic_body("MOCK-SCENARIO:output-limit", 300))
        .await;
    assert_eq!(cut["stop_reason"], "max_tokens");
    assert_eq!(cut["usage"]["output_tokens"], json!(300));
    assert_eq!(
        cut["usage"]["output_tokens_details"]["thinking_tokens"],
        json!(300)
    );
    assert!(cut["content"]
        .as_array()
        .unwrap()
        .iter()
        .all(|b| b["type"] == "thinking"));

    let (_, _, cut) = mock
        .post(
            RESPONSES,
            responses_body("MOCK-SCENARIO:output-limit", Some(256)),
        )
        .await;
    assert_eq!(cut["status"], "incomplete");
    assert_eq!(cut["incomplete_details"]["reason"], "max_output_tokens");
    assert_eq!(cut["usage"]["output_tokens"], json!(256));
    assert_eq!(
        cut["usage"]["output_tokens_details"]["reasoning_tokens"],
        json!(256)
    );

    let (_, _, cut) = mock
        .post(
            COMPLETIONS,
            openrouter_body("mock/text", "MOCK-SCENARIO:output-limit", Some(64)),
        )
        .await;
    assert_eq!(cut["choices"][0]["finish_reason"], "length");
    assert_eq!(cut["choices"][0]["message"]["content"], Value::Null);
    assert_eq!(cut["usage"]["completion_tokens"], json!(64));
    assert_eq!(
        cut["usage"]["completion_tokens_details"]["reasoning_tokens"],
        json!(64)
    );

    let (_, _, cut) = mock
        .post(
            OLLAMA_CHAT,
            json!({"model": "mock-llama", "stream": false,
                   "messages": [{"role": "user", "content": "MOCK-SCENARIO:output-limit"}]}),
        )
        .await;
    assert_eq!(cut["done_reason"], "length");
    assert_eq!(cut["eval_count"], json!(16));
}

// spec: AI-005 [M4]
// acceptance: independent
// OpenRouter-Ist-Kosten: jede erfolgreiche Antwort trägt `usage.cost` = Tokenpreise × gemeldete Tokens (+ Gebühren),
// `cost_details.upstream_inference_cost` gleich, `is_byok: false`; `no-cost` ohne `cost` und `cost_details`;
// `cost-mismatch` doppelt; `byok` mit `is_byok: true`, `cost: 0`, `upstream_inference_cost` = berechneter Wert.
#[tokio::test]
async fn openrouter_cost_scenarios() {
    let mock = Mock::start().await;
    // mock/text: prompt 0.000001, completion 0.000005 USD je Token (Modellliste des Mocks).
    let computed = |body: &Value| {
        body["usage"]["prompt_tokens"].as_f64().unwrap() * 0.000001
            + body["usage"]["completion_tokens"].as_f64().unwrap() * 0.000005
    };
    let list = mock.get_json("/openrouter/api/v1/models").await;
    let text_pricing = &list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "mock/text")
        .unwrap()["pricing"];
    assert_eq!(text_pricing["prompt"], "0.000001");
    assert_eq!(text_pricing["completion"], "0.000005");

    let (_, _, body) = mock
        .post(COMPLETIONS, openrouter_body("mock/text", "cost a", None))
        .await;
    let c = computed(&body);
    assert!(approx(&body["usage"]["cost"], c), "{body}");
    assert!(
        approx(&body["usage"]["cost_details"]["upstream_inference_cost"], c),
        "{body}"
    );
    assert_eq!(body["usage"]["is_byok"], json!(false));

    let (_, _, body) = mock
        .post(
            COMPLETIONS,
            openrouter_body("mock/text", "MOCK-SCENARIO:no-cost b", None),
        )
        .await;
    assert!(body["usage"].get("cost").is_none(), "{body}");
    assert!(body["usage"].get("cost_details").is_none(), "{body}");

    let (_, _, body) = mock
        .post(
            COMPLETIONS,
            openrouter_body("mock/text", "MOCK-SCENARIO:cost-mismatch c", None),
        )
        .await;
    assert!(
        approx(&body["usage"]["cost"], 2.0 * computed(&body)),
        "{body}"
    );

    let (_, _, body) = mock
        .post(
            COMPLETIONS,
            openrouter_body("mock/text", "MOCK-SCENARIO:byok d", None),
        )
        .await;
    assert_eq!(body["usage"]["is_byok"], json!(true));
    assert!(approx(&body["usage"]["cost"], 0.0), "{body}");
    assert!(
        approx(
            &body["usage"]["cost_details"]["upstream_inference_cost"],
            computed(&body)
        ),
        "{body}"
    );

    // Gebühren: Modell mit `pricing.request = "0.001"` und `pricing.image = "0.002"`, ein Bild.
    let vision = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["pricing"]["request"] == "0.001" && m["pricing"]["image"] == "0.002")
        .expect("Modell mit Gebühren")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let image_body = json!({"model": vision, "messages": [{"role": "user", "content": [
        {"type": "text", "text": "image"},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}]}]});
    let (_, _, body) = mock.post(COMPLETIONS, image_body).await;
    assert!(
        approx(&body["usage"]["cost"], computed(&body) + 0.001 + 0.002),
        "{body}"
    );
}

// spec: AI-005 [M4]
// acceptance: independent
// OpenRouter-Modellliste: weiterhin 8 Modelle, darunter `M-LC` (Override ab 1.000), `M-IR` (`internal_reasoning`),
// `M-R1`/`M-R2`/`M-R3`, ein Modell mit `top_provider.max_completion_tokens = 4000`, je ein Modell mit
// `expiration_date` in 30 Tagen, in 200 Tagen und `null`. Der Mock rechnet Kosten auf `M-LC` ab 1.000 Eingabe-Tokens
// mit dem Override.
#[tokio::test]
async fn openrouter_model_list_roles() {
    let mock = Mock::start().await;
    let list = mock.get_json("/openrouter/api/v1/models").await;
    let data = list["data"].as_array().unwrap();
    assert_eq!(data.len(), 8);
    let has = |f: &dyn Fn(&Value) -> bool| data.iter().filter(|m| f(m)).count();
    assert!(
        has(&|m| m["pricing"]["prompt"] == "0.000001"
            && m["pricing"]["completion"] == "0.000002"
            && m["pricing"]["overrides"]
                == json!([{"min_prompt_tokens": 1000, "prompt": "0.000002", "completion": "0.000004"}]))
            >= 1
    );
    assert!(
        has(&|m| m["pricing"]["completion"] == "0.000002"
            && m["pricing"]["internal_reasoning"] == "0.000010")
            >= 1
    );
    let params = |m: &Value| {
        m["supported_parameters"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    assert!(
        has(&|m| params(m).contains(&json!("reasoning"))
            && m["reasoning"]
                == json!({"supported_efforts": ["none", "low", "medium", "high"], "default_enabled": true,
                      "default_effort": "medium", "mandatory": false}))
            >= 1
    );
    assert!(
        has(&|m| params(m).contains(&json!("reasoning"))
            && m["reasoning"]
                == json!({"supported_efforts": ["low", "medium", "high"], "mandatory": true}))
            >= 1
    );
    assert!(has(&|m| !params(m).contains(&json!("reasoning"))) >= 1);
    assert!(has(&|m| m["top_provider"]["max_completion_tokens"] == json!(4000)) >= 1);
    let today = today_days();
    let in_days = |d: i64| {
        has(&|m| {
            m["expiration_date"]
                .as_str()
                .is_some_and(|s| days_from_civil(s) - today == d)
        })
    };
    assert_eq!(in_days(30), 1);
    assert_eq!(in_days(200), 1);
    assert!(has(&|m| m.get("expiration_date") == Some(&Value::Null)) >= 1);

    // Kosten auf M-LC: ab 1.000 Eingabe-Tokens 0,000002/0,000004 USD je Token.
    let lc = data
        .iter()
        .find(|m| m["pricing"]["overrides"].is_array())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, _, body) = mock
        .post(COMPLETIONS, openrouter_body(&lc, &"y".repeat(4_400), None))
        .await;
    let input = body["usage"]["prompt_tokens"].as_f64().unwrap();
    assert!(input >= 1000.0);
    let output = body["usage"]["completion_tokens"].as_f64().unwrap();
    assert!(
        approx(&body["usage"]["cost"], input * 0.000002 + output * 0.000004),
        "{body}"
    );
    let (_, _, body) = mock
        .post(COMPLETIONS, openrouter_body(&lc, "short", None))
        .await;
    let input = body["usage"]["prompt_tokens"].as_f64().unwrap();
    let output = body["usage"]["completion_tokens"].as_f64().unwrap();
    assert!(
        approx(&body["usage"]["cost"], input * 0.000001 + output * 0.000002),
        "{body}"
    );
}

// spec: AI-002 [M4]
// acceptance: independent
// Anthropic-Modellliste: A `thinking` adaptiv und abschaltbar (`enabled` nicht); B nur adaptiv mit `max_tokens = 4000`;
// C `thinking.supported = false`; D ohne `capabilities`; je ein Modell mit `retires_at` in 30 Tagen, 200 Tagen und
// `null`.
#[tokio::test]
async fn anthropic_model_list_thinking_variants() {
    let mock = Mock::start().await;
    let list = mock.get_json("/anthropic/v1/models").await;
    let data = list["data"].as_array().unwrap();
    let role = |name: &str| {
        data.iter()
            .find(|m| m["display_name"] == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    assert_eq!(
        role("A")["capabilities"]["thinking"],
        json!({"supported": true, "types": {"adaptive": {"supported": true}, "disabled": {"supported": true},
                                            "enabled": {"supported": false}}})
    );
    let b = role("B");
    assert_eq!(b["capabilities"]["thinking"]["supported"], json!(true));
    assert_eq!(
        b["capabilities"]["thinking"]["types"]["adaptive"]["supported"],
        json!(true)
    );
    assert_eq!(
        b["capabilities"]["thinking"]["types"]["disabled"]["supported"],
        json!(false)
    );
    assert_eq!(
        b["capabilities"]["thinking"]["types"]["enabled"]["supported"],
        json!(false)
    );
    assert_eq!(b["max_tokens"], json!(4000));
    assert_eq!(
        role("C")["capabilities"]["thinking"]["supported"],
        json!(false)
    );
    assert!(role("D").get("capabilities").is_none());
    let today = today_days();
    let retiring = |d: i64| {
        data.iter()
            .filter(|m| {
                m["retires_at"]
                    .as_str()
                    .is_some_and(|s| days_from_civil(s) - today == d)
            })
            .count()
    };
    assert_eq!(retiring(30), 1);
    assert_eq!(retiring(200), 1);
    assert!(data
        .iter()
        .any(|m| m.get("retires_at") == Some(&Value::Null)));
}

// spec: AI-005 [M4]
// acceptance: independent
// `unknown-model` → HTTP 400 `{"error": {"code": 400, "message": "<model> is not a valid model ID"}, "user_id":
// "mock-user"}`, ohne `error.type` und `error.metadata`.
#[tokio::test]
async fn unknown_model_form() {
    let mock = Mock::start().await;
    mock.scenario("openrouter", "unknown-model", 1).await;
    let (status, _, body) = mock
        .post(COMPLETIONS, openrouter_body("k09/nope", "unknown", None))
        .await;
    assert_eq!(status, 400);
    assert_eq!(
        body,
        json!({"error": {"code": 400, "message": "k09/nope is not a valid model ID"}, "user_id": "mock-user"})
    );
    // Danach wieder normal.
    let (status, _, _) = mock
        .post(COMPLETIONS, openrouter_body("mock/text", "after", None))
        .await;
    assert_eq!(status, 200);
}

// spec: AI-004 [M4]
// spec: AI-003 [M4]
// acceptance: independent
// Decisions: Eingabe-Tokens = Σ über die Fragen (⌈Z_E / 4⌉ + 85 je Bild + ⌈Z_q / 4⌉ + 100), Ausgabe 0,
// `reasoning_tokens = 0`, `cache_write_tokens` 0, Antwort ohne `id`; Responses meldet `cache_write_tokens` 0.
#[tokio::test]
async fn decisions_counting_and_openai_usage_fields() {
    let mock = Mock::start().await;
    let input = "x".repeat(159);
    let questions = json!([
        {"name": "is_product", "type": "predicate", "instructions": "Is this a product page?"},
        {"name": "page_type", "type": "choice", "instructions": "Which type?",
         "choices": [{"value": "product", "description": "single"}, {"value": "list", "description": "many"}]},
        {"name": "quality", "type": "score", "instructions": "Quality?",
         "levels": [{"label": "low", "description": "thin"}, {"label": "high", "description": "rich"}]}
    ]);
    let z_q = [
        "is_product".len() + "Is this a product page?".len(),
        "page_type".len()
            + "Which type?".len()
            + "product".len()
            + "single".len()
            + "list".len()
            + "many".len(),
        "quality".len()
            + "Quality?".len()
            + "low".len()
            + "thin".len()
            + "high".len()
            + "rich".len(),
    ];
    let expected: u64 = z_q
        .iter()
        .map(|z| 159u64.div_ceil(4) + (*z as u64).div_ceil(4) + 100)
        .sum();
    let (status, headers, body) = mock
        .post(
            DECISIONS,
            json!({"model": "gpt-6-luna", "input": input, "questions": questions}),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["usage"]["input_tokens"], json!(expected));
    assert_eq!(body["usage"]["output_tokens"], json!(0));
    assert_eq!(
        body["usage"]["output_tokens_details"]["reasoning_tokens"],
        json!(0)
    );
    assert_eq!(
        body["usage"]["input_tokens_details"]["cache_write_tokens"],
        json!(0)
    );
    assert!(body.get("id").is_none(), "{body}");
    assert!(header(&headers, "x-request-id").starts_with("req_mock_"));

    // Mit Bild: +85 je Frage.
    let image_input = json!([{"role": "user", "content": [
        {"type": "input_text", "text": "abcd"},
        {"type": "input_image", "image_url": "data:image/png;base64,AAAA"}]}]);
    let one = json!([{"name": "q", "type": "predicate", "instructions": ""}]);
    let (_, _, body) = mock
        .post(
            DECISIONS,
            json!({"model": "gpt-6-luna", "input": image_input, "questions": one}),
        )
        .await;
    assert_eq!(body["usage"]["input_tokens"], json!(1 + 85 + 1 + 100));

    let (_, _, body) = mock.post(RESPONSES, responses_body("usage", None)).await;
    assert_eq!(
        body["usage"]["input_tokens_details"]["cache_write_tokens"],
        json!(0)
    );
    assert_eq!(
        body["usage"]["input_tokens_details"]["cached_tokens"],
        json!(0)
    );
}

// spec: AI-001 [M4]
// acceptance: independent
// Request-IDs: Anthropic Header `request-id: req_mock_<n>` mit Antwort-`id` `msg_mock_<n>` (bei Fehlern zusätzlich
// `request_id` im Körper); OpenAI Header `x-request-id: req_mock_<n>`; OpenRouter `x-generation-id: gen-mock-<n>` und
// gleiche Antwort-`id`; laufende Nummern.
#[tokio::test]
async fn request_ids_per_provider() {
    let mock = Mock::start().await;
    let (_, headers, body) = mock.post(MESSAGES, anthropic_body("id 1", 100)).await;
    let id = header(&headers, "request-id").to_string();
    let n: u64 = id.strip_prefix("req_mock_").expect(&id).parse().unwrap();
    assert_eq!(body["id"], json!(format!("msg_mock_{n}")));
    let (status, headers, body) = mock
        .post(MESSAGES, anthropic_body("MOCK-SCENARIO:401 err", 100))
        .await;
    assert_eq!(status, 401);
    let err_id = header(&headers, "request-id").to_string();
    let m: u64 = err_id
        .strip_prefix("req_mock_")
        .expect(&err_id)
        .parse()
        .unwrap();
    assert!(m > n, "laufende Nummer: {n} → {m}");
    assert_eq!(body["request_id"], json!(err_id));

    let (_, headers, _) = mock.post(RESPONSES, responses_body("id", None)).await;
    assert!(header(&headers, "x-request-id").starts_with("req_mock_"));

    let (_, headers, body) = mock
        .post(COMPLETIONS, openrouter_body("mock/text", "id", None))
        .await;
    let generation = header(&headers, "x-generation-id").to_string();
    assert!(generation.starts_with("gen-mock-"), "{generation}");
    assert_eq!(body["id"], json!(generation));
}

// spec: AI-003 [M4]
// acceptance: independent
// Request-Protokoll unverändert: `GET /__mock/log` liefert den Body, über den `store`, `reasoning`, `thinking` und
// `output_config.effort` geprüft werden.
#[tokio::test]
async fn request_log_carries_the_body() {
    let mock = Mock::start().await;
    let mut body = responses_body("log", Some(100));
    body["reasoning"] = json!({"effort": "none"});
    mock.post(RESPONSES, body.clone()).await;
    let log = mock.http_log().await;
    let entry = log
        .iter()
        .find(|r| r["path"] == RESPONSES)
        .expect("Eintrag im Log");
    let logged: Value = serde_json::from_str(entry["body"].as_str().unwrap()).unwrap();
    assert_eq!(logged, body);
    assert_eq!(logged["store"], json!(false));
}

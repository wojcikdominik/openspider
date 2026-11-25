//! Selbsttests der ai-mock-Erweiterungen nach der Live-Probe Q00 (08 „Konventionen“, Nachtrag 24.11.2026, M4-K09):
//! Reasoning-Tokens, Abbruch an der Ausgabegrenze, OpenRouter-Ist-Kosten, Modelllisten, unbekanntes Modell,
//! Decisions-Zählung je Frage, `cache_write_tokens` und Request-IDs.
mod common;
use common::Resp;
use os_testsite::mocks::{
    ai::{decisions_input_tokens, token_count, AiMock, ENCRYPTED, REASONING_TEXT, SIGNATURE},
    MockServer,
};
use serde_json::{json, Value};
use tokio::{io::AsyncWriteExt, net::TcpStream};

const ANTHROPIC: &str = "/anthropic/v1/messages";
const RESPONSES: &str = "/openai/v1/responses";
const DECISIONS: &str = "/openai/v1/decisions";
const OPENROUTER: &str = "/openrouter/api/v1/chat/completions";
const OLLAMA: &str = "/ollama/api/chat";
/// Antwort `{"text":"…"}` aus `mock-answers.d/k09.json`: 286 Zeichen = 72 Tokens.
const LONG_TOKENS: u64 = 72;

async fn call(server: &MockServer, method: &str, path: &str, scenario: &str, body: &str) -> Resp {
    let mut stream = TcpStream::connect(server.addr()).await.unwrap();
    let head = format!("{method} {path} HTTP/1.1\r\nHost: mock.test\r\nConnection: close\r\nContent-Type: application/json\r\nX-Scenario: {scenario}\r\nContent-Length: {}\r\n\r\n", body.len());
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(body.as_bytes()).await.unwrap();
    let (raw, err) = common::read_all(&mut stream).await;
    assert!(err.is_none(), "{err:?}");
    common::parse(&raw).unwrap()
}
fn value(r: &Resp) -> Value {
    serde_json::from_slice(&r.body).unwrap()
}
async fn server() -> (AiMock, MockServer) {
    let mock = AiMock::fixtures().unwrap();
    let s = MockServer::start(mock.registrations()).await.unwrap();
    (mock, s)
}
fn anthropic_body(text: &str, max_tokens: u64) -> String {
    json!({"model":"mock-claude","max_tokens":max_tokens,"messages":[{"role":"user","content":[{"type":"text","text":text}]}]}).to_string()
}
fn responses_body(text: &str, max_output_tokens: Option<u64>) -> String {
    let mut body = json!({"model":"gpt-6-luna","store":false,"input":[{"role":"user","content":[{"type":"input_text","text":text}]}]});
    if let Some(max) = max_output_tokens {
        body["max_output_tokens"] = json!(max);
    }
    body.to_string()
}
fn openrouter_body(model: &str, text: &str, max_tokens: Option<u64>) -> String {
    let mut body =
        json!({"model":model,"messages":[{"role":"user","content":[{"type":"text","text":text}]}]});
    if let Some(max) = max_tokens {
        body["max_tokens"] = json!(max);
    }
    body.to_string()
}
fn ollama_body(text: &str) -> String {
    json!({"model":"mock-llama","stream":false,"messages":[{"role":"user","content":text}]})
        .to_string()
}

// spec: AI-001 [M4]
// spec: AI-002 [M4]
// spec: AI-003 [M4]
// spec: AI-005 [M4]
// Szenario `reasoning=<n>`: Reasoning-Tokens in der Anbieterform, Ausgabe-Tokens = Antwort + n, Reasoning-Inhalt vor
// der Antwort; ohne Szenario dieselben Felder mit 0. Ollama ohne Reasoning-Felder, Decisions mit 0.
#[tokio::test]
async fn reasoning_scenario_adds_tokens_and_reasoning_content_per_provider() {
    let (_, s) = server().await;
    let answer = 3; // {"ok":true}
    let a = value(
        &call(
            &s,
            "POST",
            ANTHROPIC,
            "reasoning=40",
            &anthropic_body("x", 1000),
        )
        .await,
    );
    assert_eq!(a["content"][0]["type"], "thinking");
    assert_eq!(a["content"][0]["thinking"], REASONING_TEXT);
    assert_eq!(a["content"][0]["signature"], SIGNATURE);
    assert_eq!(a["content"][1]["type"], "text");
    assert_eq!(a["usage"]["output_tokens_details"]["thinking_tokens"], 40);
    assert_eq!(a["usage"]["output_tokens"], answer + 40);
    assert_eq!(a["stop_reason"], "end_turn");
    let a = value(&call(&s, "POST", ANTHROPIC, "ok", &anthropic_body("x", 1000)).await);
    assert_eq!(a["usage"]["output_tokens_details"]["thinking_tokens"], 0);
    assert_eq!(a["content"].as_array().unwrap().len(), 1);

    let r = value(
        &call(
            &s,
            "POST",
            RESPONSES,
            "reasoning=40",
            &responses_body("x", None),
        )
        .await,
    );
    assert_eq!(
        r["output"][0],
        json!({"type":"reasoning","summary":[],"encrypted_content":ENCRYPTED})
    );
    assert_eq!(r["output"][1]["type"], "message");
    assert_eq!(r["usage"]["output_tokens_details"]["reasoning_tokens"], 40);
    assert_eq!(r["usage"]["output_tokens"], answer + 40);
    let r = value(&call(&s, "POST", RESPONSES, "ok", &responses_body("x", None)).await);
    assert_eq!(r["usage"]["output_tokens_details"]["reasoning_tokens"], 0);
    assert_eq!(r["output"].as_array().unwrap().len(), 1);

    let o = value(
        &call(
            &s,
            "POST",
            OPENROUTER,
            "reasoning=40",
            &openrouter_body("mock/text", "x", None),
        )
        .await,
    );
    let message = &o["choices"][0]["message"];
    assert_eq!(message["reasoning"], REASONING_TEXT);
    assert_eq!(
        message["reasoning_details"],
        json!([{"type":"reasoning.encrypted","data":ENCRYPTED}])
    );
    assert_eq!(message["content"], "{\"ok\":true}");
    assert_eq!(
        o["usage"]["completion_tokens_details"]["reasoning_tokens"],
        40
    );
    assert_eq!(o["usage"]["completion_tokens"], answer + 40);
    let o = value(
        &call(
            &s,
            "POST",
            OPENROUTER,
            "ok",
            &openrouter_body("mock/text", "x", None),
        )
        .await,
    );
    assert_eq!(
        o["usage"]["completion_tokens_details"]["reasoning_tokens"],
        0
    );
    assert!(o["choices"][0]["message"].get("reasoning").is_none());

    let l = value(&call(&s, "POST", OLLAMA, "reasoning=40", &ollama_body("x")).await);
    assert_eq!(l["eval_count"], answer);
    assert!(!l.to_string().contains(REASONING_TEXT));
    let d = value(
        &call(
            &s,
            "POST",
            DECISIONS,
            "reasoning=40",
            r#"{"input":"x","questions":[{"name":"q","type":"predicate"}]}"#,
        )
        .await,
    );
    assert_eq!(d["usage"]["output_tokens_details"]["reasoning_tokens"], 0);
    assert_eq!(d["usage"]["output_tokens"], 0);
}

// spec: AI-002 [M4]
// spec: AI-003 [M4]
// spec: AI-005 [M4]
// Automatische Abbruchform: ⌈Zeichen / 4⌉ + n > gesendete Grenze → Abbruchform des Anbieters, Ausgabe-Tokens = Grenze;
// mit Reasoning nur der Reasoning-Teil, bei n = 0 ein abgeschnittener Text. Innerhalb der Grenze keine Änderung.
#[tokio::test]
async fn exceeding_the_sent_limit_returns_the_providers_abort_form() {
    let (_, s) = server().await;
    let long = "MOCK-ID:k09-long";
    // Anthropic: mit Reasoning nur der thinking-Block.
    let a = value(
        &call(
            &s,
            "POST",
            ANTHROPIC,
            "reasoning=40",
            &anthropic_body(long, 100),
        )
        .await,
    );
    assert_eq!(a["stop_reason"], "max_tokens");
    assert_eq!(a["usage"]["output_tokens"], 100);
    assert_eq!(a["usage"]["output_tokens_details"]["thinking_tokens"], 40);
    assert_eq!(a["content"].as_array().unwrap().len(), 1);
    assert_eq!(a["content"][0]["type"], "thinking");
    // n = 0: abgeschnittener Textblock (Grenze × 4 Zeichen).
    let a = value(&call(&s, "POST", ANTHROPIC, "ok", &anthropic_body(long, 10)).await);
    assert_eq!(a["stop_reason"], "max_tokens");
    assert_eq!(a["usage"]["output_tokens"], 10);
    assert_eq!(
        a["content"][0]["text"].as_str().unwrap().chars().count(),
        40
    );
    // Genau an der Grenze: kein Abbruch.
    let a = value(
        &call(
            &s,
            "POST",
            ANTHROPIC,
            "ok",
            &anthropic_body(long, LONG_TOKENS),
        )
        .await,
    );
    assert_eq!(a["stop_reason"], "end_turn");

    let r = value(
        &call(
            &s,
            "POST",
            RESPONSES,
            "reasoning=200",
            &responses_body(long, Some(150)),
        )
        .await,
    );
    assert_eq!(r["status"], "incomplete");
    assert_eq!(r["incomplete_details"]["reason"], "max_output_tokens");
    assert_eq!(r["output"].as_array().unwrap().len(), 1);
    assert_eq!(r["output"][0]["type"], "reasoning");
    assert_eq!(r["usage"]["output_tokens"], 150);
    assert_eq!(r["usage"]["output_tokens_details"]["reasoning_tokens"], 150);
    let r = value(&call(&s, "POST", RESPONSES, "ok", &responses_body(long, Some(5))).await);
    assert_eq!(r["status"], "incomplete");
    assert_eq!(r["output"][0]["type"], "message");
    assert_eq!(
        r["output"][0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        20
    );

    let o = value(
        &call(
            &s,
            "POST",
            OPENROUTER,
            "reasoning=90",
            &openrouter_body("mock/text", long, Some(80)),
        )
        .await,
    );
    assert_eq!(o["choices"][0]["finish_reason"], "length");
    assert!(o["choices"][0]["message"]["content"].is_null());
    assert_eq!(o["usage"]["completion_tokens"], 80);
    let o = value(
        &call(
            &s,
            "POST",
            OPENROUTER,
            "ok",
            &openrouter_body("mock/text", long, Some(80)),
        )
        .await,
    );
    assert_eq!(o["choices"][0]["finish_reason"], "stop");
}

// spec: AI-002 [M4]
// spec: AI-003 [M4]
// spec: AI-005 [M4]
// spec: AI-006 [M4]
// Szenario `output-limit` erzwingt die Abbruchform unabhängig von n (Reasoning-Tokens = Grenze); Ollama
// `done_reason: "length"` mit `eval_count = 16`.
#[tokio::test]
async fn output_limit_scenario_forces_the_abort_form() {
    let (_, s) = server().await;
    let a = value(
        &call(
            &s,
            "POST",
            ANTHROPIC,
            "output-limit",
            &anthropic_body("x", 256),
        )
        .await,
    );
    assert_eq!(a["stop_reason"], "max_tokens");
    assert_eq!(a["usage"]["output_tokens"], 256);
    assert_eq!(a["usage"]["output_tokens_details"]["thinking_tokens"], 256);
    assert_eq!(a["content"][0]["type"], "thinking");
    let r = value(
        &call(
            &s,
            "POST",
            RESPONSES,
            "output-limit",
            &responses_body("x", Some(256)),
        )
        .await,
    );
    assert_eq!(r["incomplete_details"]["reason"], "max_output_tokens");
    assert_eq!(r["usage"]["output_tokens"], 256);
    let o = value(
        &call(
            &s,
            "POST",
            OPENROUTER,
            "output-limit",
            &openrouter_body("mock/text", "x", Some(256)),
        )
        .await,
    );
    assert_eq!(o["choices"][0]["finish_reason"], "length");
    assert!(o["choices"][0]["message"]["content"].is_null());
    assert_eq!(
        o["usage"]["completion_tokens_details"]["reasoning_tokens"],
        256
    );
    let l = value(
        &call(
            &s,
            "POST",
            OLLAMA,
            "output-limit",
            &ollama_body("MOCK-ID:k09-long"),
        )
        .await,
    );
    assert_eq!(l["done_reason"], "length");
    assert_eq!(l["eval_count"], 16);
    assert_eq!(
        l["message"]["content"].as_str().unwrap().chars().count(),
        64
    );
}

/// Kosten in µUSD aus einer gemeldeten JSON-Zahl (kaufmännisch gerundet).
fn micro(v: &Value) -> u64 {
    (v.as_f64().unwrap() * 1e6).round() as u64
}

// spec: AI-005 [M4]
// OpenRouter-Ist-Kosten: `usage.cost` = Tokenpreise × Tokens + `pricing.request` + `pricing.image` × Bilder,
// `cost_details.upstream_inference_cost` gleich, `is_byok: false`; Szenarien `no-cost`, `cost-mismatch`, `byok`;
// Langkontext über `overrides` (M-LC = mock/global-a).
#[tokio::test]
async fn openrouter_reports_cost_per_scenario() {
    let (_, s) = server().await;
    // mock/text: 1 / 5 µUSD je Token.
    let body = openrouter_body("mock/text", "MOCK-ID:k09-long", None);
    let input = token_count(&serde_json::from_str(&body).unwrap()) as u64;
    let expected = input + LONG_TOKENS * 5;
    let o = value(&call(&s, "POST", OPENROUTER, "ok", &body).await);
    assert_eq!(o["usage"]["prompt_tokens"], input);
    assert_eq!(micro(&o["usage"]["cost"]), expected);
    assert_eq!(
        micro(&o["usage"]["cost_details"]["upstream_inference_cost"]),
        expected
    );
    assert_eq!(o["usage"]["is_byok"], false);
    let o = value(&call(&s, "POST", OPENROUTER, "no-cost", &body).await);
    assert!(o["usage"].get("cost").is_none() && o["usage"].get("cost_details").is_none());
    let o = value(&call(&s, "POST", OPENROUTER, "cost-mismatch", &body).await);
    assert_eq!(micro(&o["usage"]["cost"]), 2 * expected);
    let o = value(&call(&s, "POST", OPENROUTER, "byok", &body).await);
    assert_eq!(o["usage"]["is_byok"], true);
    assert_eq!(micro(&o["usage"]["cost"]), 0);
    assert_eq!(
        micro(&o["usage"]["cost_details"]["upstream_inference_cost"]),
        expected
    );
    // Bild über mock/vision: + 0,001 je Request + 0,002 je Bild.
    let vision = json!({"model":"mock/vision","messages":[{"role":"user","content":[
        {"type":"text","text":"abcd"},
        {"type":"image_url","image_url":{"url":"data:image/png;base64,AAAA"}}]}]});
    let o = value(&call(&s, "POST", OPENROUTER, "ok", &vision.to_string()).await);
    let tokens = token_count(&vision) as u64;
    assert_eq!(micro(&o["usage"]["cost"]), tokens + 3 * 5 + 1_000 + 2_000);
    // M-LC: ab 1.000 Eingabe-Tokens 2 / 4 µUSD je Token statt 1 / 2.
    let lc = |chars: usize| openrouter_body("mock/global-a", &"y".repeat(chars), None);
    let o = value(&call(&s, "POST", OPENROUTER, "ok", &lc(4_000)).await);
    assert_eq!(o["usage"]["prompt_tokens"], 1_000);
    assert_eq!(micro(&o["usage"]["cost"]), 1_000 * 2 + 3 * 4);
    let o = value(&call(&s, "POST", OPENROUTER, "ok", &lc(3_996)).await);
    assert_eq!(o["usage"]["prompt_tokens"], 999);
    assert_eq!(micro(&o["usage"]["cost"]), 999 + 3 * 2);
}

// spec: AI-002 [M4]
// spec: AI-005 [M4]
// Modelllisten: Anthropic A–D mit `thinking`, B mit `max_tokens = 4000`, Abschaltdaten; OpenRouter weiterhin 8 Modelle
// mit M-LC, M-IR, M-R1 … M-R3, `max_completion_tokens = 4000` und Ablaufdaten (30/200 Tage/null); EU-Teilmenge 5.
#[tokio::test]
async fn model_lists_carry_thinking_reasoning_prices_and_retirement() {
    let (_, s) = server().await;
    let m = value(&call(&s, "GET", "/anthropic/v1/models", "ok", "").await);
    let data = m["data"].as_array().unwrap();
    assert_eq!(data.len(), 4);
    let thinking = |i: usize| &data[i]["capabilities"]["thinking"];
    assert_eq!(thinking(0)["types"]["adaptive"]["supported"], true);
    assert_eq!(thinking(0)["types"]["disabled"]["supported"], true);
    assert_eq!(thinking(0)["types"]["enabled"]["supported"], false);
    assert_eq!(thinking(1)["types"]["adaptive"]["supported"], true);
    assert_eq!(thinking(1)["types"]["disabled"]["supported"], false);
    assert_eq!(data[1]["max_tokens"], 4000);
    assert_eq!(thinking(2)["supported"], false);
    assert!(data[3].get("capabilities").is_none());
    let retires: Vec<&Value> = data.iter().filter_map(|d| d.get("retires_at")).collect();
    assert_eq!(retires.iter().filter(|r| r.is_null()).count(), 1);
    assert_eq!(retires.iter().filter(|r| r.is_string()).count(), 2);

    let m = value(&call(&s, "GET", "/openrouter/api/v1/models", "ok", "").await);
    let data = m["data"].as_array().unwrap();
    assert_eq!(data.len(), 8);
    let by_id = |id: &str| data.iter().find(|d| d["id"] == id).unwrap().clone();
    let r1 = by_id("mock/text-small");
    assert!(r1["supported_parameters"]
        .as_array()
        .unwrap()
        .contains(&json!("reasoning")));
    assert_eq!(
        r1["reasoning"]["supported_efforts"],
        json!(["none", "low", "medium", "high"])
    );
    assert_eq!(r1["reasoning"]["mandatory"], false);
    let r2 = by_id("mock/text-large");
    assert_eq!(r2["reasoning"]["mandatory"], true);
    assert_eq!(r2["top_provider"]["max_completion_tokens"], 4000);
    let r3 = by_id("mock/global-c");
    assert!(!r3["supported_parameters"]
        .as_array()
        .unwrap()
        .contains(&json!("reasoning")));
    let lc = by_id("mock/global-a");
    assert_eq!(lc["pricing"]["overrides"][0]["min_prompt_tokens"], 1000);
    assert_eq!(lc["pricing"]["overrides"][0]["prompt"], "0.000002");
    let ir = by_id("mock/global-b");
    assert_eq!(ir["pricing"]["internal_reasoning"], "0.000010");
    assert!(lc["expiration_date"].is_string() && ir["expiration_date"].is_string());
    assert!(lc["expiration_date"].as_str() < ir["expiration_date"].as_str());
    assert!(r3["expiration_date"].is_null());
    let vision = by_id("mock/vision");
    assert_eq!(vision["pricing"]["request"], "0.001");
}

// spec: AI-005 [M4]
// Szenario `unknown-model`: HTTP 400 mit numerischem `error.code`, ohne `error.type` und `error.metadata`.
#[tokio::test]
async fn openrouter_unknown_model_has_the_observed_error_form() {
    let (_, s) = server().await;
    let r = call(
        &s,
        "POST",
        OPENROUTER,
        "unknown-model",
        &openrouter_body("mock/nope", "x", None),
    )
    .await;
    assert_eq!(r.status, 400);
    assert_eq!(
        value(&r),
        json!({"error":{"code":400,"message":"mock/nope is not a valid model ID"},"user_id":"mock-user"})
    );
}

// spec: AI-004 [M4]
// Decisions: Eingabe-Tokens je Frage (⌈Eingabe / 4⌉ + 85 je Bild + ⌈Fragentext / 4⌉ + 100), Ausgabe 0,
// `cache_write_tokens` 0, keine Antwort-`id`; Prüfwert aus AI-012 (Nachtrag): 464.
#[tokio::test]
async fn decisions_count_input_per_question() {
    let (_, s) = server().await;
    let body = json!({"model":"gpt-6-luna","input":"e".repeat(350),"questions":[
        {"name":"a".repeat(40),"type":"predicate","instructions":"b".repeat(100)},
        {"name":"c".repeat(10),"type":"choice","instructions":"d".repeat(100),
         "choices":[{"value":"x".repeat(50),"description":"y".repeat(50)}]}]});
    let r = call(&s, "POST", DECISIONS, "ok", &body.to_string()).await;
    let d = value(&r);
    assert_eq!(d["usage"]["input_tokens"], 464);
    assert_eq!(decisions_input_tokens(&body), 464);
    assert_eq!(d["usage"]["output_tokens"], 0);
    assert_eq!(d["usage"]["input_tokens_details"]["cache_write_tokens"], 0);
    assert_eq!(d["usage"]["input_tokens_details"]["cached_tokens"], 0);
    assert!(d.get("id").is_none());
    // Bündelung spart keine Tokens: zwei Einzel-Requests ergeben dieselbe Summe.
    let single = |q: usize| {
        let mut b = body.clone();
        b["questions"] = json!([body["questions"][q].clone()]);
        decisions_input_tokens(&b)
    };
    assert_eq!(single(0) + single(1), 464);
    let r = value(&call(&s, "POST", RESPONSES, "ok", &responses_body("x", None)).await);
    assert_eq!(r["usage"]["input_tokens_details"]["cache_write_tokens"], 0);
}

// spec: AI-001 [M4]
// Request-IDs: Anthropic Header `request-id: req_mock_<n>` und `id: msg_mock_<n>`, bei Fehlern zusätzlich `request_id`
// im Körper; OpenAI Header `x-request-id: req_mock_<n>`; OpenRouter Header `x-generation-id: gen-mock-<n>` und gleiche
// Antwort-`id`; Ollama ohne Kennung.
#[tokio::test]
async fn request_ids_follow_the_provider_forms() {
    let (_, s) = server().await;
    let a = call(&s, "POST", ANTHROPIC, "ok", &anthropic_body("x", 100)).await;
    assert_eq!(a.header("request-id"), Some("req_mock_1"));
    assert_eq!(value(&a)["id"], "msg_mock_1");
    assert!(value(&a).get("request_id").is_none());
    let e = call(&s, "POST", ANTHROPIC, "529", &anthropic_body("x", 100)).await;
    assert_eq!(e.status, 529);
    assert_eq!(e.header("request-id"), Some("req_mock_2"));
    assert_eq!(value(&e)["request_id"], "req_mock_2");
    let r = call(&s, "POST", RESPONSES, "ok", &responses_body("x", None)).await;
    assert_eq!(r.header("x-request-id"), Some("req_mock_3"));
    let d = call(
        &s,
        "POST",
        DECISIONS,
        "ok",
        r#"{"input":"x","questions":[{"name":"q","type":"predicate"}]}"#,
    )
    .await;
    assert_eq!(d.header("x-request-id"), Some("req_mock_4"));
    let o = call(
        &s,
        "POST",
        OPENROUTER,
        "ok",
        &openrouter_body("mock/text", "x", None),
    )
    .await;
    assert_eq!(o.header("x-generation-id"), Some("gen-mock-5"));
    assert_eq!(value(&o)["id"], "gen-mock-5");
    let l = call(&s, "POST", OLLAMA, "ok", &ollama_body("x")).await;
    assert_eq!(l.header("x-request-id"), None);
    assert_eq!(l.header("request-id"), None);
    // Kaputte JSON-Antworten bleiben unverändert (nur Header).
    let m = call(&s, "POST", ANTHROPIC, "malformed-json", "{}").await;
    assert_eq!(m.body, b"{broken");
    assert_eq!(m.header("request-id"), Some("req_mock_7"));
}

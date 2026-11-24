//! M4-K02: Adapter OpenAI Responses API (AI-003) gegen ai-mock (`/openai/v1/responses`).
#[path = "adapter_openai_support.rs"]
mod support;

use std::sync::Arc;

use os_ai::adapters::openai_decisions::{
    DecideOptions, DecisionInput, OpenAiDecisions, Question, QuestionResult,
};
use os_ai::adapters::openai_responses::{
    ApiKey, OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses, ResponsesOutput,
    RESPONSES_CAPABILITIES,
};
use os_ai::cost::{request_record, LogContext, PriceSource, Rates};
use os_ai::limits::PauseReason;
use os_ai::provider::{
    execute, AiProvider, Capabilities, CapabilitySource, Input, Model, Outcome, Request,
};
use os_ai::schema::{RepairAttempts, ResponseSchema, StructuredOutcome};
use os_ai::{AiStatus, PreviewSwitch};
use os_model::budget::MicroUsd;
use os_testsite::mocks::ai::token_count;
use serde_json::{json, Value};
use support::{limiter, route_site, Mock, EU_HOST, GLOBAL_HOST, KEY};

const RESPONSES: &str = "/openai/v1/responses";

fn model() -> Model {
    Model {
        provider: "openai".into(),
        id: "gpt-6-luna".into(),
        endpoint: "responses".into(),
        capabilities: RESPONSES_CAPABILITIES,
        capability_source: CapabilitySource::ModelCatalog,
    }
}

fn request(text: &str) -> Request {
    Request {
        model: model(),
        required: Capabilities {
            text: true,
            ..Capabilities::default()
        },
        input: vec![Input::Text { text: text.into() }],
        instructions: "Classify the page.".into(),
        questions: vec![],
        response_schema: None,
        max_tokens: Some(200),
        temperature: None,
        effort: None,
    }
}

fn page_schema() -> ResponseSchema {
    ResponseSchema::new(json!({
        "type": "object",
        "properties": {
            "page_type": {"type": "string", "enum": ["product", "category", "other"]},
            "in_stock": {"type": "boolean"}
        },
        "required": ["page_type", "in_stock"],
        "additionalProperties": false
    }))
    .unwrap()
}

fn ctx() -> LogContext {
    LogContext {
        run_id: 1,
        timestamp: "2026-11-24T10:00:00Z".into(),
        task: "suggest-title".into(),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: "responses".into(),
        url_id: Some(1),
        definition_version: None,
        batch_id: None,
        redactions: 0,
    }
}

// spec: AI-003
// Schema-Aufgaben senden `text.format.type = "json_schema"` mit `strict: true`; das Ergebnis wird aus dem
// Ausgabetext geparst und gegen das Schema validiert (AI-021). Bearer-Token, Content-Type, Modell und Eingabe stehen im
// Request; der Schlüssel erscheint in keiner Debug-Ausgabe des Adapters.
#[tokio::test]
async fn schema_task_sends_strict_json_schema_and_validates_the_parsed_text() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http(GLOBAL_HOST));
    let limiter = limiter();
    let run = adapter
        .run_structured(
            &limiter,
            &request("Produktseite. MOCK-ID:k02-responses"),
            &page_schema(),
            RepairAttempts::default(),
            40,
        )
        .await;
    assert_eq!(
        run.outcome,
        StructuredOutcome::Valid(json!({"page_type": "product", "in_stock": true}))
    );
    assert_eq!(run.requests.len(), 1);
    assert_eq!(run.requests[0].meta.http_status, Some(200));
    let sent = mock.requests_to(RESPONSES);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method, "POST");
    assert_eq!(support::host_of(&sent[0]), GLOBAL_HOST);
    assert_eq!(sent[0].headers["authorization"], "[REDACTED]");
    assert!(sent[0].headers["content-type"].starts_with("application/json"));
    let body: Value = serde_json::from_str(&sent[0].body).unwrap();
    assert_eq!(body["model"], "gpt-6-luna");
    assert_eq!(body["instructions"], "Classify the page.");
    assert_eq!(body["max_output_tokens"], 200);
    let format = &body["text"]["format"];
    assert_eq!(format["type"], "json_schema");
    assert_eq!(format["strict"], true);
    assert_eq!(format["schema"], *page_schema().schema());
    assert!(format["name"]
        .as_str()
        .is_some_and(|n| !n.is_empty() && n.len() <= 64));
    assert_eq!(
        body["input"][0]["content"][0],
        json!({"type": "input_text", "text": "Produktseite. MOCK-ID:k02-responses"})
    );
    // Der Schlüssel taucht weder in Debug-Ausgaben noch im (maskierten) Mock-Log auf.
    assert!(!format!("{adapter:?}").contains(KEY));
    assert!(!format!("{:?}", mock.mock.requests()).contains(KEY));
}

// spec: AI-003
// Verstößt der Ausgabetext gegen das Schema, folgt genau ein Reparaturversuch mit der Validator-Meldung; danach
// `Error: Invalid output` mit 2 Requests (AI-021). Unlesbare Antwort einmalig → zweiter Request gültig.
#[tokio::test]
async fn schema_violation_is_repaired_once_and_then_invalid_output() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http(GLOBAL_HOST));
    let limiter = limiter();
    let run = adapter
        .run_structured(
            &limiter,
            &request("MOCK-SCENARIO:schema-violation MOCK-ID:k02-responses"),
            &page_schema(),
            RepairAttempts::default(),
            40,
        )
        .await;
    assert!(matches!(
        run.outcome,
        StructuredOutcome::InvalidOutput { .. }
    ));
    assert_eq!(run.outcome.status(), Some(AiStatus::InvalidOutput));
    assert_eq!(run.requests.len(), 2);
    assert!(run.requests.iter().all(|r| r.validator_error.is_some()));
    let bodies = mock.bodies(RESPONSES);
    assert_eq!(bodies.len(), 2);
    let repair = bodies[1]["input"][0]["content"].as_array().unwrap();
    assert_eq!(
        repair.len(),
        2,
        "repair = same input plus validator message"
    );
    assert!(repair[1]["text"]
        .as_str()
        .unwrap()
        .contains(run.requests[0].validator_error.as_deref().unwrap()));

    mock.scenario("malformed-json", 1).await;
    let run = adapter
        .run_structured(
            &limiter,
            &request("Zweiter Eintrag MOCK-ID:k02-responses"),
            &page_schema(),
            RepairAttempts::default(),
            40,
        )
        .await;
    assert_eq!(
        run.outcome,
        StructuredOutcome::Valid(json!({"page_type": "product", "in_stock": true}))
    );
    assert_eq!(run.requests.len(), 2);
    assert!(run.requests[0].validator_error.is_some());
    assert!(run.requests[1].validator_error.is_none());
}

// spec: AI-003
// Ausgabeteil `refusal` → Refusal-Kennzeichen, kein Parse-Versuch, keine Wiederholung; der Refusal-Text steht in der
// Protokollzeile (`Refusal Reason`, AI-014), nicht als Antwort.
#[tokio::test]
async fn refusal_part_is_flagged_and_its_text_goes_to_the_request_log_only() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http(GLOBAL_HOST));
    let limiter = limiter();
    let run = adapter
        .run_structured(
            &limiter,
            &request("MOCK-SCENARIO:refusal MOCK-ID:k02-responses"),
            &page_schema(),
            RepairAttempts::default(),
            40,
        )
        .await;
    assert_eq!(run.outcome, StructuredOutcome::Refusal);
    assert_eq!(
        run.requests.len(),
        1,
        "a refusal is neither repaired nor retried"
    );
    let meta = &run.requests[0].meta;
    assert_eq!(meta.attempts, 1);
    assert_eq!(meta.refusal_reason.as_deref(), Some("Fixture refusal"));
    let outcome = meta.request_outcome(AiStatus::Refusal, 40, 0, 0);
    let rates = Rates::per_million("0.1", "0.5", PriceSource::Table).unwrap();
    let row = request_record(&ctx(), &rates, MicroUsd(100), &outcome);
    assert_eq!(row.status, "Refusal");
    assert_eq!(row.refusal_reason.as_deref(), Some("Fixture refusal"));

    // Über den Contract-Weg (AI-001): Refusal ohne Inhalt.
    let mut req = request("MOCK-SCENARIO:refusal");
    req.response_schema = Some(page_schema().schema().clone());
    let response = adapter.execute(req).await.unwrap();
    assert!(response.refusal);
    assert_eq!(response.content, None);
    assert_eq!(
        execute(
            PreviewSwitch::ON,
            &adapter,
            request("MOCK-SCENARIO:refusal")
        )
        .await,
        Outcome::Status(AiStatus::Refusal)
    );
}

// spec: AI-003
// `usage.input_tokens`, `usage.output_tokens` und `usage.input_tokens_details.cached_tokens` werden übertragen;
// Cache-Schreiben meldet OpenAI nicht (`null`, AI-001). `input_tokens` enthält die gecachten Tokens bereits
// (Festlegung 24.11.2026). Ohne `usage` bleiben alle Tokenfelder `null` und die Kosten kommen aus der Schätzung.
#[tokio::test]
async fn usage_fields_are_transferred_and_missing_usage_stays_null() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http(GLOBAL_HOST));
    let limiter = limiter();
    let (reply, meta) = adapter
        .run(
            &limiter,
            &request("Ein Text mit Inhalt MOCK-ID:k02-responses"),
            30,
        )
        .await;
    let reply = reply.unwrap();
    let body: Value = serde_json::from_str(&mock.requests_to(RESPONSES)[0].body).unwrap();
    let expected_input = token_count(&body) as u64;
    let usage = reply.usage.clone().unwrap();
    assert_eq!(usage.input_tokens, Some(expected_input));
    let ResponsesOutput::Text(text) = &reply.output else {
        panic!("text output expected: {:?}", reply.output)
    };
    assert_eq!(
        usage.output_tokens,
        Some(text.chars().count().div_ceil(4) as u64)
    );
    assert_eq!(usage.cached_input_tokens, Some(0));
    assert_eq!(usage.cache_write_tokens, None);
    assert_eq!(meta.usage, Some(usage));
    assert_eq!(reply.provider_request_id.as_deref(), Some("mock-request"));

    // Gecachte Tokens laut Anbieter: unverändert übertragen, `input_tokens` enthält sie.
    let site = route_site(
        "responses-cached",
        r#"
[[route]]
path = "/v1/responses"
status = 200
content_type = "application/json"
body = '{"id":"resp_1","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"{\"ok\":true}"}]}],"usage":{"input_tokens":1200,"output_tokens":7,"input_tokens_details":{"cached_tokens":1024},"output_tokens_details":{"reasoning_tokens":0}}}'
"#,
    )
    .await;
    let http = OpenAiHttp::new(
        support::client_for(&[GLOBAL_HOST], site.addr()),
        OpenAiConfig::new(ApiKey::new(KEY))
            .with_base_url(format!("http://{GLOBAL_HOST}:{}/v1", site.port())),
        OpenAiAccount::new(),
    );
    let (reply, _) = OpenAiResponses::new(http)
        .run(&limiter, &request("x"), 10)
        .await;
    let usage = reply.unwrap().usage.unwrap();
    assert_eq!(usage.input_tokens, Some(1200));
    assert_eq!(usage.cached_input_tokens, Some(1024));
    assert_eq!(usage.output_tokens, Some(7));
    assert_eq!(usage.cache_write_tokens, None);

    let (reply, meta) = adapter
        .run(
            &limiter,
            &request("MOCK-SCENARIO:missing-usage MOCK-ID:k02-responses"),
            30,
        )
        .await;
    assert_eq!(reply.unwrap().usage, None);
    let outcome = meta.request_outcome(AiStatus::Success, 30, 10, 0);
    let rates = Rates::per_million("0.1", "0.5", PriceSource::Table).unwrap();
    let row = request_record(&ctx(), &rates, MicroUsd(100), &outcome);
    assert_eq!(row.input_tokens, None);
    assert_eq!(row.output_tokens, None);
    assert_eq!(row.cached_input_tokens, None);
    assert_eq!(row.price_source, PriceSource::Estimate);
}

// spec: AI-003
// HTTP 429 mit `error.type = "insufficient_quota"` → `Quota exceeded` ohne Wiederholung, `Error Code` aus `error.code`
// (`credit_balance_exhausted`); danach erhalten alle offenen OpenAI-Einträge – Responses und Decisions – `Quota
// exceeded`, ohne dass ein weiterer Request hinausgeht (Mock-Log: genau 1 Request).
#[tokio::test]
async fn insufficient_quota_stops_responses_and_decisions_without_further_requests() {
    let mock = Mock::start().await;
    let account = OpenAiAccount::new();
    let responses = OpenAiResponses::new(mock.http_with(GLOBAL_HOST, account.clone()));
    let decisions = OpenAiDecisions::new(mock.http_with(GLOBAL_HOST, account.clone()));
    let limiter = limiter();
    let (reply, meta) = responses
        .run(&limiter, &request("MOCK-SCENARIO:insufficient-quota"), 20)
        .await;
    let error = reply.unwrap_err();
    assert_eq!(error.status, AiStatus::QuotaExceeded);
    assert_eq!(meta.attempts, 1, "no retry");
    assert_eq!(meta.http_status, Some(429));
    assert_eq!(meta.error_code.as_deref(), Some("credit_balance_exhausted"));
    assert_eq!(
        account.quota_stop().unwrap().error_code.as_deref(),
        Some("credit_balance_exhausted")
    );
    let outcome = meta.request_outcome(error.status.clone(), 20, 0, 0);
    let row = request_record(
        &ctx(),
        &Rates::per_million("0.1", "0.5", PriceSource::Table).unwrap(),
        MicroUsd(100),
        &outcome,
    );
    assert_eq!(row.status, "Quota exceeded");
    assert_eq!(row.error_code.as_deref(), Some("credit_balance_exhausted"));
    assert_eq!(row.cost, MicroUsd(0));

    for i in 0..3 {
        let (reply, meta) = responses
            .run(&limiter, &request(&format!("weiterer Eintrag {i}")), 20)
            .await;
        assert_eq!(reply.unwrap_err().status, AiStatus::QuotaExceeded);
        assert_eq!(meta.http_status, None);
    }
    let run = decisions
        .decide(
            &limiter,
            &DecisionInput::text("Decision-Eintrag"),
            &[Question::predicate("is_product", "Is this a product page?")],
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(
        run.results[0].1,
        QuestionResult::NotAnswered {
            status: AiStatus::QuotaExceeded
        }
    );
    assert_eq!(mock.openai_requests().len(), 1);
    assert!(mock.requests_to("/openai/v1/decisions").is_empty());
    assert!(
        responses.models().await.is_err(),
        "nothing leaves after the stop"
    );
    assert_eq!(mock.openai_requests().len(), 1);
}

// spec: AI-003
// Ältere Antwortform `error.code = "insufficient_quota"` wird gleich behandelt; ein 429 ohne diesen Typ wird nach
// INTEG-005 wiederholt und endet mit `Success` (Szenario `429x2`).
#[tokio::test]
async fn legacy_quota_form_is_final_and_plain_429_is_retried() {
    let site = route_site(
        "legacy-quota",
        r#"
[[route]]
path = "/v1/responses"
status = 429
content_type = "application/json"
body = '{"error":{"message":"You exceeded your current quota.","type":"requests","param":null,"code":"insufficient_quota"}}'
"#,
    )
    .await;
    let account = OpenAiAccount::new();
    let http = OpenAiHttp::new(
        support::client_for(&[GLOBAL_HOST], site.addr()),
        OpenAiConfig::new(ApiKey::new(KEY))
            .with_base_url(format!("http://{GLOBAL_HOST}:{}/v1", site.port())),
        account.clone(),
    );
    let limiter = limiter();
    let (reply, meta) = OpenAiResponses::new(http)
        .run(&limiter, &request("x"), 10)
        .await;
    assert_eq!(reply.unwrap_err().status, AiStatus::QuotaExceeded);
    assert_eq!(meta.attempts, 1);
    assert_eq!(meta.error_code.as_deref(), Some("insufficient_quota"));
    assert!(account.quota_stop().is_some());

    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http(GLOBAL_HOST));
    let (reply, meta) = adapter
        .run(
            &limiter,
            &request("MOCK-SCENARIO:429x2 MOCK-ID:k02-responses"),
            10,
        )
        .await;
    assert!(reply.is_ok());
    assert_eq!(meta.attempts, 3);
    assert_eq!(meta.retries, 2);
    assert_eq!(meta.http_status, Some(200));
    assert_eq!(mock.requests_to(RESPONSES).len(), 3);
    assert!(adapter.http().account().quota_stop().is_none());
}

// spec: AI-003
// Bilder gehen ausschließlich als `data:<mime>;base64,…` hinaus, nie als HTTP-URL (Prüfung im Mock-Log).
#[tokio::test]
async fn images_are_sent_only_as_base64_data_urls() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http(GLOBAL_HOST));
    let mut req = request("Beschreibe das Bild.");
    req.required.vision = true;
    req.input.push(Input::Image {
        media_type: "image/png".into(),
        base64: "iVBORw0KGgo=".into(),
    });
    req.input.push(Input::Image {
        media_type: "image/jpeg".into(),
        base64: "/9j/4AAQ".into(),
    });
    let (reply, _) = adapter.run(&limiter(), &req, 200).await;
    assert!(reply.is_ok());
    let body = &mock.bodies(RESPONSES)[0];
    let images: Vec<&Value> = body["input"][0]["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["type"] == "input_image")
        .collect();
    assert_eq!(images.len(), 2);
    assert_eq!(images[0]["image_url"], "data:image/png;base64,iVBORw0KGgo=");
    assert_eq!(images[1]["image_url"], "data:image/jpeg;base64,/9j/4AAQ");
    for image in images {
        assert!(image["image_url"].as_str().unwrap().starts_with("data:"));
    }
    assert!(!mock.requests_to(RESPONSES)[0].body.contains("http://"));
    assert!(!mock.requests_to(RESPONSES)[0].body.contains("https://"));
}

// spec: AI-003
// Basis-URL ist Parameter (EU-Datenresidenz, AI-019): mit EU-Basis gehen alle Requests an `eu.openai.mock.test`,
// keiner an `openai.mock.test`. Organisation und Projekt als optionale Header.
#[tokio::test]
async fn base_url_parameter_and_optional_organisation_and_project_headers() {
    let mock = Mock::start().await;
    let http = OpenAiHttp::new(
        mock.client(),
        mock.config(EU_HOST)
            .with_organization("org-k02")
            .with_project("proj-k02"),
        OpenAiAccount::new(),
    );
    let adapter = OpenAiResponses::new(http);
    let (reply, _) = adapter
        .run(&limiter(), &request("MOCK-ID:k02-responses"), 10)
        .await;
    assert!(reply.is_ok());
    let sent = mock.requests_to(RESPONSES);
    assert_eq!(sent.len(), 1);
    assert_eq!(support::host_of(&sent[0]), EU_HOST);
    assert!(mock
        .openai_requests()
        .iter()
        .all(|r| support::host_of(r) == EU_HOST));
    assert_eq!(sent[0].headers["openai-organization"], "org-k02");
    assert_eq!(sent[0].headers["openai-project"], "proj-k02");

    let mock = Mock::start().await;
    OpenAiResponses::new(mock.http(GLOBAL_HOST))
        .run(&limiter(), &request("MOCK-ID:k02-responses"), 10)
        .await
        .0
        .unwrap();
    let sent = mock.requests_to(RESPONSES);
    assert!(!sent[0].headers.contains_key("openai-organization"));
    assert!(!sent[0].headers.contains_key("openai-project"));
}

// spec: AI-003
// Modellliste über `GET /v1/models` ohne Preise; Fähigkeiten kommen aus dem Modellkatalog (AI-001/AI-010), ohne
// Katalogeintrag keine Fähigkeit.
#[tokio::test]
async fn model_list_comes_from_get_models_with_catalog_capabilities() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http(GLOBAL_HOST)).with_capabilities(Arc::new(|id| {
        (id == "gpt-6-luna").then_some(RESPONSES_CAPABILITIES)
    }));
    let models = adapter.models().await.unwrap();
    assert_eq!(
        models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["gpt-6-luna", "mock-embedding"]
    );
    assert!(models.iter().all(|m| m.provider == "openai"
        && m.endpoint == "responses"
        && m.capability_source == CapabilitySource::ModelCatalog));
    assert_eq!(models[0].capabilities, RESPONSES_CAPABILITIES);
    assert_eq!(models[1].capabilities, Capabilities::default());
    let sent = mock.requests_to("/openai/v1/models");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method, "GET");
}

// spec: AI-017
// Rate-Limit-Header von OpenAI werden im Adapter über `Limiter::observe_headers` ausgewertet: Request-Restzahl 0 mit
// Reset in 20 s → Pause bis zum Reset. (Der ai-mock sendet keine Rate-Limit-Header; daher eine eigene Route.)
#[tokio::test]
async fn rate_limit_headers_are_handed_to_the_limiter() {
    let site = route_site(
        "ratelimit",
        r#"
[[route]]
path = "/v1/models"
status = 200
content_type = "application/json"
body = '{"object":"list","data":[{"id":"gpt-6-luna","object":"model"}]}'
headers = { "x-ratelimit-remaining-requests" = "0", "x-ratelimit-reset-requests" = "20s", "x-ratelimit-remaining-tokens" = "150000", "x-ratelimit-reset-tokens" = "6m0s" }
"#,
    )
    .await;
    let limiter = Arc::new(limiter());
    let http = OpenAiHttp::new(
        support::client_for(&[GLOBAL_HOST], site.addr()),
        OpenAiConfig::new(ApiKey::new(KEY))
            .with_base_url(format!("http://{GLOBAL_HOST}:{}/v1", site.port())),
        OpenAiAccount::new(),
    )
    .with_limiter(limiter.clone());
    assert!(limiter.pauses(0).is_empty());
    let models = OpenAiResponses::new(http).list_models().await.unwrap();
    assert_eq!(models.len(), 1);
    let pauses = limiter.pauses(0);
    assert_eq!(pauses.len(), 1, "{pauses:?}");
    assert_eq!(pauses[0].provider, "openai");
    assert_eq!(pauses[0].reason, PauseReason::RateLimit);
    let wall = limiter.clock().wall_unix_millis();
    let remaining_ms = pauses[0].until_unix_millis - wall;
    assert!((19_000..=20_000).contains(&remaining_ms), "{remaining_ms}");
}

// spec: AI-003
// Ohne Schlüssel geht kein Request hinaus (`Error: Missing API key`).
#[tokio::test]
async fn missing_key_sends_nothing() {
    let mock = Mock::start().await;
    let http = OpenAiHttp::new(
        mock.client(),
        OpenAiConfig::new(ApiKey::new("  ")).with_base_url(mock.base(GLOBAL_HOST)),
        OpenAiAccount::new(),
    );
    let (reply, meta) = OpenAiResponses::new(http)
        .run(&limiter(), &request("x"), 10)
        .await;
    assert_eq!(reply.unwrap_err().status, AiStatus::MissingApiKey);
    assert_eq!(meta.http_status, None);
    assert!(mock.openai_requests().is_empty());
}

// spec: AI-001
// Contract-Test über den gemeinsamen Einstieg `provider::execute`: „gib `{"ok": true}` nach Schema zurück“ liefert
// das geparste Ergebnis; `usage` in einheitlicher Form.
#[tokio::test]
async fn contract_task_returns_the_parsed_ok_result() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http(GLOBAL_HOST));
    let mut req = request("Return ok true");
    req.response_schema = Some(json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}},
        "required": ["ok"],
        "additionalProperties": false
    }));
    let Outcome::Completed(response) = execute(PreviewSwitch::ON, &adapter, req).await else {
        panic!("completed expected")
    };
    assert_eq!(response.content, Some(json!({"ok": true})));
    assert!(!response.refusal);
    assert!(response.usage.input_tokens.is_some());
    assert_eq!(response.usage.cache_write_tokens, None);
    assert_eq!(mock.requests_to(RESPONSES).len(), 1);
}

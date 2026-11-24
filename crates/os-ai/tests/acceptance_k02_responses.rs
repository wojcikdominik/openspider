//! Unabhängige Abnahmetests M4-K02 für den Adapter OpenAI Responses (AI-003), allein aus `docs/spec/08-ai.md`
//! abgeleitet (AI-003; dazu AI-001, AI-017, AI-020, AI-021). Gegen den ai-mock bzw. einen kleinen lokalen Testserver.
mod acceptance_k02_support;

use acceptance_k02_support::*;
use os_ai::adapters::openai_decisions::DecideOptions;
use os_ai::adapters::openai_decisions::DecisionInput;
use os_ai::adapters::openai_decisions::{OpenAiDecisions, Question};
use os_ai::adapters::openai_responses::{
    ApiKey, OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses, RESPONSES_CAPABILITIES,
};
use os_ai::provider::{AiProvider, Capabilities, CapabilitySource, Input, Model, Request, Usage};
use os_ai::schema::{RepairAttempts, ResponseSchema, StructuredOutcome};
use os_ai::AiStatus;
use serde_json::{json, Value};

fn model() -> Model {
    Model {
        provider: "openai".into(),
        id: "gpt-6-luna".into(),
        endpoint: "responses".into(),
        capabilities: RESPONSES_CAPABILITIES,
        capability_source: CapabilitySource::ModelCatalog,
        reasoning_levels: Vec::new(),
    }
}

fn request(input: Vec<Input>) -> Request {
    Request {
        model: model(),
        required: Capabilities::default(),
        input,
        instructions: "Classify the page.".into(),
        questions: Vec::new(),
        response_schema: None,
        max_tokens: None,
        temperature: None,
        effort: None,
        reasoning: Default::default(),
    }
}

fn text(t: &str) -> Input {
    Input::Text { text: t.into() }
}

fn page_type_schema() -> ResponseSchema {
    ResponseSchema::new(json!({
        "type": "object",
        "properties": {"page_type": {"type": "string", "enum": ["product", "category", "article"]}},
        "required": ["page_type"],
        "additionalProperties": false
    }))
    .unwrap()
}

fn ok_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}},
        "required": ["ok"],
        "additionalProperties": false
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 1: Schema-Aufgaben senden `text.format.type = "json_schema"` mit `strict: true`; das Ergebnis wird aus dem
// Ausgabetext geparst und gegen das Schema validiert (AI-021).

// spec: AI-003
// Kriterium 1 (Request-Aufbau): text.format mit type json_schema, name, schema und strict true an POST /v1/responses.
#[tokio::test]
async fn k1_schema_task_sends_text_format_json_schema_strict() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http());
    let limiter = fast_limiter();
    let schema = page_type_schema();
    let run = adapter
        .run_structured(
            &limiter,
            &request(vec![text("MOCK-ID:k02a-valid Produktseite")]),
            &schema,
            RepairAttempts::default(),
            100,
        )
        .await;
    assert_eq!(
        run.outcome,
        StructuredOutcome::Valid(json!({"page_type": "product"}))
    );
    let records = mock.requests_to("responses");
    assert_eq!(records.len(), 1, "genau ein Request");
    assert_eq!(records[0].method, "POST");
    let body: Value = serde_json::from_str(&records[0].body).unwrap();
    let format = &body["text"]["format"];
    assert_eq!(format["type"], "json_schema", "{body}");
    assert_eq!(format["strict"], json!(true), "{body}");
    assert_eq!(
        &format["schema"],
        schema.schema(),
        "Schema unverändert gesendet"
    );
    let name = format["name"].as_str().expect("text.format.name vorhanden");
    assert!(!name.is_empty());
    assert_eq!(body["model"], "gpt-6-luna");
}

// spec: AI-003
// spec: AI-021
// Kriterium 1 (lokale Validierung): eine dauerhaft schemawidrige Antwort wird nicht übernommen; nach genau einem
// Reparaturversuch (2 Requests) `Error: Invalid output`, Validator-Meldung für das Protokoll vorhanden.
#[tokio::test]
async fn k1_schema_violation_is_rejected_after_one_repair() {
    let mock = Mock::start().await;
    mock.scenario("schema-violation", 10).await;
    let adapter = OpenAiResponses::new(mock.http());
    let limiter = fast_limiter();
    let run = adapter
        .run_structured(
            &limiter,
            &request(vec![text("Beliebige Seite")]),
            &page_type_schema(),
            RepairAttempts::default(),
            100,
        )
        .await;
    match &run.outcome {
        StructuredOutcome::InvalidOutput { validator_error } => {
            assert!(!validator_error.is_empty())
        }
        other => panic!("erwartet Error: Invalid output, erhalten {other:?}"),
    }
    assert_eq!(run.outcome.status(), Some(AiStatus::InvalidOutput));
    assert_eq!(AiStatus::InvalidOutput.to_string(), "Error: Invalid output");
    assert_eq!(
        mock.requests_to("responses").len(),
        2,
        "1 Request + 1 Reparatur"
    );
    assert!(run.requests.iter().all(|r| r.validator_error.is_some()));
}

// spec: AI-003
// spec: AI-021
// Kriterium 1 (lokale Validierung): Enum-Wert außerhalb der erlaubten Werte (Seitentyp `shop`) ist ein Verstoß.
#[tokio::test]
async fn k1_enum_value_outside_schema_is_invalid_output() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http());
    let run = adapter
        .run_structured(
            &fast_limiter(),
            &request(vec![text("MOCK-ID:k02a-shop Shopseite")]),
            &page_type_schema(),
            RepairAttempts::default(),
            100,
        )
        .await;
    assert_eq!(
        run.outcome.status(),
        Some(AiStatus::InvalidOutput),
        "{:?}",
        run.outcome
    );
}

// spec: AI-003
// spec: AI-021
// Kriterium 1 (Parsen aus dem Ausgabetext): `malformed-json` einmalig, danach `ok` → gültiges Ergebnis, 2 Requests.
#[tokio::test]
async fn k1_malformed_json_once_then_ok_is_valid_with_two_requests() {
    let mock = Mock::start().await;
    mock.scenario("malformed-json", 1).await;
    let adapter = OpenAiResponses::new(mock.http());
    let run = adapter
        .run_structured(
            &fast_limiter(),
            &request(vec![text("MOCK-ID:k02a-valid Produktseite")]),
            &page_type_schema(),
            RepairAttempts::default(),
            100,
        )
        .await;
    assert_eq!(
        run.outcome,
        StructuredOutcome::Valid(json!({"page_type": "product"}))
    );
    assert_eq!(mock.requests_to("responses").len(), 2);
}

// spec: AI-003
// spec: AI-001
// Contract-Aufgabe „gib {"ok": true} nach Schema zurück“ über die gemeinsame Schnittstelle: geparstes Ergebnis.
#[tokio::test]
async fn contract_ok_true_via_common_interface() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http());
    let mut req = request(vec![text("Gib ok zurück.")]);
    req.response_schema = Some(ok_schema());
    let response = adapter.execute(req).await.unwrap();
    assert_eq!(response.content, Some(json!({"ok": true})));
    assert!(!response.refusal);
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 2: Ausgabeteil `refusal` → Refusal-Kennzeichen; der Refusal-Text wird im Protokoll (AI-014) gespeichert,
// nicht als Antwort.

// spec: AI-003
// Kriterium 2: Refusal-Kennzeichen, kein Parse-/Reparaturversuch, Refusal-Text nur im Protokoll.
#[tokio::test]
async fn k2_refusal_part_sets_flag_and_text_goes_to_log_not_answer() {
    let mock = Mock::start().await;
    mock.scenario("refusal", 1).await;
    let adapter = OpenAiResponses::new(mock.http());
    let run = adapter
        .run_structured(
            &fast_limiter(),
            &request(vec![text("Beliebige Seite")]),
            &page_type_schema(),
            RepairAttempts::default(),
            100,
        )
        .await;
    assert_eq!(run.outcome, StructuredOutcome::Refusal);
    assert_eq!(run.outcome.status(), Some(AiStatus::Refusal));
    assert_eq!(run.requests.len(), 1, "kein Reparaturversuch nach Refusal");
    assert_eq!(mock.requests_to("responses").len(), 1);
    let meta = &run.requests[0].meta;
    assert_eq!(meta.refusal_reason.as_deref(), Some("Fixture refusal"));
    // Protokollzeile (AI-014): Spalte Refusal Reason, Status Refusal.
    let outcome = meta.request_outcome(AiStatus::Refusal, 100, 0, 0);
    let record = os_ai::cost::request_record(
        &log_ctx("responses"),
        &os_ai::cost::Rates::per_million("0.10", "0.50", os_ai::cost::PriceSource::Table).unwrap(),
        os_model::budget::MicroUsd(0),
        &outcome,
    );
    assert_eq!(record.refusal_reason.as_deref(), Some("Fixture refusal"));
    assert_eq!(record.status, "Refusal");
}

// spec: AI-003
// spec: AI-001
// Kriterium 2 über die gemeinsame Schnittstelle: Refusal-Kennzeichen gesetzt, kein Inhalt übernommen.
#[tokio::test]
async fn k2_refusal_via_common_interface_has_no_content() {
    let mock = Mock::start().await;
    mock.scenario("refusal", 1).await;
    let adapter = OpenAiResponses::new(mock.http());
    let mut req = request(vec![text("Beliebige Seite")]);
    req.response_schema = Some(ok_schema());
    let response = adapter.execute(req).await.unwrap();
    assert!(response.refusal);
    assert_eq!(response.content, None, "Refusal-Text nicht als Antwort");
}

pub fn log_ctx(endpoint: &str) -> os_ai::cost::LogContext {
    os_ai::cost::LogContext {
        run_id: 1,
        timestamp: "2026-11-24T00:00:00Z".into(),
        task: "page-type".into(),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: endpoint.into(),
        url_id: Some(1),
        definition_version: None,
        batch_id: None,
        redactions: 0,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 3: `usage.input_tokens`, `usage.output_tokens` und `usage.input_tokens_details.cached_tokens` werden
// übertragen (einheitliche Felder, fehlende Felder `null`, AI-001).

fn responses_reply(usage: Value) -> Value {
    json!({
        "id": "resp_local",
        "object": "response",
        "status": "completed",
        "output": [{"type": "message", "role": "assistant",
                    "content": [{"type": "output_text", "text": "{\"ok\":true}"}]}],
        "usage": usage
    })
}

// spec: AI-003
// spec: AI-001
// Kriterium 3: input_tokens, output_tokens, input_tokens_details.cached_tokens → einheitliche Felder.
#[tokio::test]
async fn k3_usage_fields_are_transferred() {
    let server = LocalServer::start(|_| {
        Canned::json(
            200,
            responses_reply(json!({
                "input_tokens": 1234, "output_tokens": 56,
                "input_tokens_details": {"cached_tokens": 789},
                "output_tokens_details": {"reasoning_tokens": 0},
                "total_tokens": 1290
            })),
        )
    })
    .await;
    let adapter = OpenAiResponses::new(server.http());
    let (result, meta) = adapter
        .run(&fast_limiter(), &request(vec![text("x")]), 10)
        .await;
    assert!(result.is_ok(), "{result:?}");
    let expected = Usage {
        input_tokens: Some(1234),
        output_tokens: Some(56),
        cached_input_tokens: Some(789),
        cache_write_tokens: None,
        reasoning_tokens: None,
    };
    assert_eq!(meta.usage, Some(expected.clone()));
    let mut req = request(vec![text("x")]);
    req.response_schema = Some(ok_schema());
    let response = adapter.execute(req).await.unwrap();
    assert_eq!(response.usage, expected);
}

// spec: AI-003
// spec: AI-001
// Kriterium 3 / AI-001: fehlt `input_tokens_details.cached_tokens`, ist das einheitliche Feld `null`, nicht 0.
#[tokio::test]
async fn k3_missing_cached_tokens_is_null_not_zero() {
    let server = LocalServer::start(|_| {
        Canned::json(
            200,
            responses_reply(json!({"input_tokens": 10, "output_tokens": 2})),
        )
    })
    .await;
    let adapter = OpenAiResponses::new(server.http());
    let (_, meta) = adapter
        .run(&fast_limiter(), &request(vec![text("x")]), 10)
        .await;
    let usage = meta.usage.expect("usage vorhanden");
    assert_eq!(usage.input_tokens, Some(10));
    assert_eq!(usage.output_tokens, Some(2));
    assert_eq!(usage.cached_input_tokens, None);
    assert_eq!(usage.cache_write_tokens, None);
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 4: HTTP 429 mit `error.type = "insufficient_quota"` → `Quota exceeded` ohne Wiederholung, Grund aus
// `error.code` in `Error Code`; alle offenen OpenAI-Einträge (Responses und Decisions) ebenfalls `Quota exceeded`;
// ältere Form `error.code = "insufficient_quota"` gleich; 429 ohne diesen Typ → Backoff nach INTEG-005.

// spec: AI-003
// spec: AI-020
// Kriterium 4: insufficient_quota → Quota exceeded, genau 1 Request, Error Code mit credit_balance_exhausted.
#[tokio::test]
async fn k4_insufficient_quota_is_final_without_retry() {
    let mock = Mock::start().await;
    mock.scenario("insufficient-quota", 1).await;
    let adapter = OpenAiResponses::new(mock.http());
    let run = adapter
        .run_structured(
            &fast_limiter(),
            &request(vec![text("Beliebige Seite")]),
            &page_type_schema(),
            RepairAttempts::default(),
            100,
        )
        .await;
    let failure = match run.outcome {
        StructuredOutcome::Failed(f) => f,
        other => panic!("erwartet Quota exceeded, erhalten {other:?}"),
    };
    assert_eq!(failure.error.status, AiStatus::QuotaExceeded);
    assert_eq!(failure.error.status.to_string(), "Quota exceeded");
    assert_eq!(mock.requests_to("responses").len(), 1, "keine Wiederholung");
    assert_eq!(failure.meta.retries, 0);
    assert_eq!(failure.meta.http_status, Some(429));
    let code = failure.meta.error_code.clone().expect("Error Code gesetzt");
    assert!(
        code.contains("credit_balance_exhausted"),
        "konkreter Grund aus error.code: {code}"
    );
    // Protokollzeile (AI-014): Error Code, Kosten 0 (keine Antwort mit usage).
    let record = os_ai::cost::request_record(
        &log_ctx("responses"),
        &os_ai::cost::Rates::per_million("0.10", "0.50", os_ai::cost::PriceSource::Table).unwrap(),
        os_model::budget::MicroUsd(0),
        &failure
            .meta
            .request_outcome(AiStatus::QuotaExceeded, 100, 0, 0),
    );
    assert!(record
        .error_code
        .unwrap()
        .contains("credit_balance_exhausted"));
    assert_eq!(record.status, "Quota exceeded");
    assert_eq!(record.cost, os_model::budget::MicroUsd(0));
}

// spec: AI-003
// Kriterium 4: nach insufficient_quota erhalten alle weiteren offenen OpenAI-Einträge – Responses und Decisions –
// `Quota exceeded`, ohne dass ein weiterer Request an OpenAI geht.
#[tokio::test]
async fn k4_quota_stop_applies_to_all_open_openai_entries_without_requests() {
    let mock = Mock::start().await;
    mock.scenario("insufficient-quota", 1).await;
    let http = mock.http();
    let responses = OpenAiResponses::new(http.clone());
    let decisions = OpenAiDecisions::new(http.clone());
    let limiter = fast_limiter();
    let first = responses
        .run_structured(
            &limiter,
            &request(vec![text("Seite 1")]),
            &page_type_schema(),
            RepairAttempts::default(),
            100,
        )
        .await;
    assert!(
        matches!(first.outcome, StructuredOutcome::Failed(ref f) if f.error.status == AiStatus::QuotaExceeded)
    );
    let before = mock.mock.requests().len();

    // Weitere Responses-Einträge.
    for page in ["Seite 2", "Seite 3"] {
        let run = responses
            .run_structured(
                &limiter,
                &request(vec![text(page)]),
                &page_type_schema(),
                RepairAttempts::default(),
                100,
            )
            .await;
        match run.outcome {
            StructuredOutcome::Failed(f) => assert_eq!(f.error.status, AiStatus::QuotaExceeded),
            other => panic!("erwartet Quota exceeded, erhalten {other:?}"),
        }
    }
    // Decisions-Einträge desselben Kontos.
    let questions = vec![
        Question::predicate("q1", "Ist das eine Produktseite?"),
        Question::predicate("q2", "Ist die Seite ein Soft-404?"),
    ];
    let run = decisions
        .decide(
            &limiter,
            &DecisionInput::text("Seite 4"),
            &questions,
            &DecideOptions::default(),
        )
        .await;
    for (name, result) in &run.results {
        assert_eq!(result.status(), AiStatus::QuotaExceeded, "{name}");
    }
    // Über die gemeinsame Schnittstelle ebenfalls kein Request.
    let mut req = request(vec![text("Seite 5")]);
    req.response_schema = Some(ok_schema());
    let err = responses.execute(req).await.unwrap_err();
    assert_eq!(err.status(), Some(AiStatus::QuotaExceeded));

    assert_eq!(
        mock.mock.requests().len(),
        before,
        "kein weiterer Request an OpenAI nach dem Kontingentende"
    );
}

// spec: AI-003
// Kriterium 4: ältere Antwortform mit `error.code = "insufficient_quota"` wird gleich behandelt.
#[tokio::test]
async fn k4_legacy_error_code_insufficient_quota_is_treated_the_same() {
    let server = LocalServer::start(|_| {
        Canned::json(
            429,
            json!({"error": {"message": "You exceeded your current quota.",
                              "type": "requests", "param": null, "code": "insufficient_quota"}}),
        )
    })
    .await;
    let adapter = OpenAiResponses::new(server.http());
    let run = adapter
        .run_structured(
            &fast_limiter(),
            &request(vec![text("Beliebige Seite")]),
            &page_type_schema(),
            RepairAttempts::default(),
            100,
        )
        .await;
    match run.outcome {
        StructuredOutcome::Failed(f) => {
            assert_eq!(f.error.status, AiStatus::QuotaExceeded);
            assert_eq!(f.meta.retries, 0);
        }
        other => panic!("erwartet Quota exceeded, erhalten {other:?}"),
    }
    assert_eq!(
        server.requests_to("/v1/responses").len(),
        1,
        "keine Wiederholung"
    );
    // Konto gesperrt: weiterer Eintrag ohne Request.
    let (result, _) = adapter
        .run(&fast_limiter(), &request(vec![text("y")]), 10)
        .await;
    assert_eq!(result.unwrap_err().status, AiStatus::QuotaExceeded);
    assert_eq!(server.requests().len(), 1);
}

// spec: AI-003
// spec: AI-017
// Kriterium 4: HTTP 429 ohne `insufficient_quota` (Szenario `429x2`) → Backoff nach INTEG-005, endet mit `Success`.
#[tokio::test]
async fn k4_plain_429_is_retried_with_backoff_and_succeeds() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http());
    let run = adapter
        .run_structured(
            &fast_limiter(),
            &request(vec![text(
                "MOCK-ID:k02a-valid MOCK-SCENARIO:429x2 Produktseite",
            )]),
            &page_type_schema(),
            RepairAttempts::default(),
            100,
        )
        .await;
    assert_eq!(
        run.outcome,
        StructuredOutcome::Valid(json!({"page_type": "product"}))
    );
    assert_eq!(run.outcome.status(), Some(AiStatus::Success));
    assert_eq!(
        mock.requests_to("responses").len(),
        3,
        "2 × 429, dann Erfolg"
    );
    assert_eq!(run.requests.len(), 1, "eine Protokollzeile");
    assert_eq!(run.requests[0].meta.retries, 2, "Retries = 2");
    assert!(
        adapter.http().account().quota_stop().is_none(),
        "kein Kontingentende"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 5: Bilder werden ausschließlich als `data:<mime>;base64,…` gesendet, nie als HTTP-URL.

// spec: AI-003
// Kriterium 5: Bildeingabe als input_image mit Data-URL (Präfix `data:`), korrekter MIME-Typ, keine HTTP-URL.
#[tokio::test]
async fn k5_images_are_sent_only_as_data_urls() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.http());
    let png = tiny_png();
    let jpeg = tiny_jpeg();
    let mut req = request(vec![
        text("Beschreibe das Bild."),
        Input::Image {
            media_type: "image/png".into(),
            base64: b64(&png),
        },
        Input::Image {
            media_type: "image/jpeg".into(),
            base64: b64(&jpeg),
        },
    ]);
    req.required.vision = true;
    req.response_schema = Some(ok_schema());
    adapter.execute(req).await.unwrap();
    let bodies = mock.bodies_to("responses");
    assert_eq!(bodies.len(), 1);
    let urls = image_urls(&bodies[0]);
    assert_eq!(urls.len(), 2, "{}", bodies[0]);
    for url in &urls {
        assert!(url.starts_with("data:"), "nur Data-URLs: {url}");
        assert!(!url.contains("http://") && !url.contains("https://"));
    }
    assert_eq!(urls[0], format!("data:image/png;base64,{}", b64(&png)));
    assert_eq!(urls[1], format!("data:image/jpeg;base64,{}", b64(&jpeg)));
    assert!(!has_key(&bodies[0], "file_id"));
    // Teiltyp input_image.
    let parts = bodies[0].to_string();
    assert!(parts.contains("\"input_image\""), "{parts}");
}

// ---------------------------------------------------------------------------------------------------------------
// Beschreibung AI-003 / Eingaben: Bearer-Token, optionale Organisation/Projekt-Header, Basis-URL (EU, AI-019);
// AI-020: fehlender Schlüssel → `Error: Missing API key` ohne Request.

// spec: AI-003
// Beschreibung: POST /v1/responses mit Bearer-Token; optionale Header für Organisation und Projekt.
#[tokio::test]
async fn description_bearer_token_and_optional_org_project_headers() {
    let server = LocalServer::start(|_| {
        Canned::json(
            200,
            responses_reply(json!({"input_tokens": 1, "output_tokens": 1})),
        )
    })
    .await;
    let http = OpenAiHttp::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        OpenAiConfig::new(ApiKey::new(TEST_KEY))
            .with_base_url(server.base_url())
            .with_organization("org-k02-invented")
            .with_project("proj-k02-invented"),
        OpenAiAccount::new(),
    );
    let adapter = OpenAiResponses::new(http);
    adapter
        .run(&fast_limiter(), &request(vec![text("x")]), 10)
        .await
        .0
        .unwrap();
    let recorded = server.requests_to("/v1/responses");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].method, "POST");
    assert_eq!(
        recorded[0].header("authorization"),
        Some(format!("Bearer {TEST_KEY}").as_str())
    );
    assert_eq!(
        recorded[0].header("openai-organization"),
        Some("org-k02-invented")
    );
    assert_eq!(
        recorded[0].header("openai-project"),
        Some("proj-k02-invented")
    );
    // Ohne Angabe keine Organisations-/Projekt-Header.
    let plain = OpenAiResponses::new(server.http());
    plain
        .run(&fast_limiter(), &request(vec![text("y")]), 10)
        .await
        .0
        .unwrap();
    let second = &server.requests_to("/v1/responses")[1];
    assert_eq!(second.header("openai-organization"), None);
    assert_eq!(second.header("openai-project"), None);
}

// spec: AI-003
// spec: AI-019
// Eingaben: Basis-URL einstellbar (EU-Datenresidenz) – der Request geht an den konfigurierten Host.
#[tokio::test]
async fn description_base_url_selects_eu_host() {
    let mock = Mock::start().await;
    let http = OpenAiHttp::new(
        mock.client(),
        OpenAiConfig::new(ApiKey::new(TEST_KEY)).with_base_url(mock.base_url_for(OPENAI_EU_HOST)),
        OpenAiAccount::new(),
    );
    let adapter = OpenAiResponses::new(http);
    adapter
        .run(&fast_limiter(), &request(vec![text("x")]), 10)
        .await
        .0
        .unwrap();
    let records = mock.requests_to("responses");
    assert_eq!(records.len(), 1);
    assert!(
        records[0].host.starts_with(OPENAI_EU_HOST),
        "Host {}",
        records[0].host
    );
}

// spec: AI-003
// spec: AI-020
// Fehlender Schlüssel → `Error: Missing API key`, 0 Requests.
#[tokio::test]
async fn missing_api_key_sends_nothing() {
    let mock = Mock::start().await;
    let http = OpenAiHttp::new(
        mock.client(),
        OpenAiConfig::new(ApiKey::new("")).with_base_url(mock.base_url()),
        OpenAiAccount::new(),
    );
    let adapter = OpenAiResponses::new(http);
    let run = adapter
        .run_structured(
            &fast_limiter(),
            &request(vec![text("x")]),
            &page_type_schema(),
            RepairAttempts::default(),
            10,
        )
        .await;
    match run.outcome {
        StructuredOutcome::Failed(f) => {
            assert_eq!(f.error.status, AiStatus::MissingApiKey);
            assert_eq!(f.error.status.to_string(), "Error: Missing API key");
        }
        other => panic!("erwartet Missing API key, erhalten {other:?}"),
    }
    assert_eq!(mock.requests_to("responses").len(), 0);
}

// spec: AI-003
// spec: AI-001
// AI-001 Fähigkeiten: OpenAI Responses – Text, Structured Output, Vision, Batch (sonst nichts).
#[test]
fn capabilities_of_responses_match_matrix() {
    let c = RESPONSES_CAPABILITIES;
    assert!(c.text && c.structured_output && c.vision && c.batch);
    assert!(!c.embeddings && !c.decisions && !c.prices_via_api && !c.local);
}

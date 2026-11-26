//! Adapter Anthropic Messages (AI-002) gegen ai-mock (M4-K01). Szenarien aus 08 „Konventionen“, Antworten aus
//! `tests/fixtures/ai/mock-answers.d/` (u. a. `k01.json`), Bilder aus `tests/fixtures/ai/site/images/`.
#[path = "adapter_anthropic_support.rs"]
mod support;

use std::sync::Arc;
use std::time::Duration;

use os_ai::adapters::anthropic::{
    image_input, sniff_image_media_type, usage_from_json, AdapterReply, AnthropicAdapter,
    AnthropicConfig, AnthropicModel, ApiKey, Selectability,
};
use os_ai::adapters::openrouter::OpenRouterAdapter;
use os_ai::limits::{Execution, Limiter, RequestSpec};
use os_ai::provider::{self, AiProvider, Capabilities, Input, Outcome, ProviderError, Request};
use os_ai::schema::{
    run_with_repair, RawAnswer, RepairAttempts, ResponseSchema, StructuredOutcome,
};
use os_ai::{AiStatus, PreviewSwitch};
use serde_json::{json, Value};
use support::*;

const MESSAGES: &str = "/anthropic/v1/messages";
const MODELS: &str = "/anthropic/v1/models";

fn adapter(mock: &Mock) -> AnthropicAdapter {
    AnthropicAdapter::new(reqwest::Client::new(), mock.anthropic_config())
}

async fn run(
    limiter: &Limiter,
    adapter: &AnthropicAdapter,
    request: &Request,
) -> Execution<AdapterReply> {
    limiter
        .run(
            &RequestSpec::new("anthropic", request.model.id.clone(), 100),
            |_| adapter.send(request, None),
        )
        .await
}

// spec: AI-002
// Header `x-api-key`, `anthropic-version`, `content-type: application/json` (Mock-Log); der Schlüssel erscheint in
// keinem Log von OpenSpider (Debug-Ausgaben von Konfiguration, Adapter, Antworten und Fehlern).
#[tokio::test]
async fn requests_carry_key_version_and_json_content_type_and_the_key_never_leaks() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = limiter(LONG_TIMEOUT, 1);
    let ok = run(
        &limiter,
        &adapter,
        &text_request(anthropic_model(), "MOCK-ID:home Hallo"),
    )
    .await;
    let denied = run(
        &limiter,
        &adapter,
        &text_request(anthropic_model(), "MOCK-SCENARIO:401"),
    )
    .await;
    let models = adapter.list_models().await.unwrap();
    let requests = mock.requests_to(MESSAGES);
    assert_eq!(requests.len(), 2);
    for record in requests.iter().chain(mock.requests_to(MODELS).iter()) {
        assert!(record.headers.contains_key("x-api-key"), "{record:?}");
        assert_eq!(record.headers["anthropic-version"], "2023-06-01");
        assert_eq!(record.headers["content-type"], "application/json");
        assert!(!record.body.contains(KEY));
    }
    assert_eq!(status(&ok, None), AiStatus::Success);
    assert!(matches!(
        status(&denied, None),
        AiStatus::HttpError { code: 401, .. }
    ));
    let logs = format!(
        "{adapter:?} {:?} {ok:?} {denied:?} {models:?} {}",
        adapter.config(),
        denied.result.as_ref().unwrap_err().message
    );
    assert!(!logs.contains(KEY), "Schlüssel im Log: {logs}");
    assert!(logs.contains("ApiKey(***)"));
}

// spec: AI-002
// spec: AI-021
// Schema-Aufgaben senden `output_config.format` mit `type: "json_schema"` und dem Schema, nie `output_format`; der
// Systemtext steht im Top-Level-Feld `system`; die Antwort aus `k01.json` wird geparst und validiert.
#[tokio::test]
async fn schema_tasks_send_output_config_format_and_never_output_format() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let request = schema_request(
        anthropic_model(),
        "MOCK-ID:k01-structured Seite",
        label_schema(),
    );
    let response = adapter.execute(request.clone()).await.unwrap();
    assert_eq!(response.content, Some(json!({"ok": true, "label": "k01"})));
    assert!(!response.refusal);
    let text = adapter
        .execute(text_request(anthropic_model(), "Ohne Schema"))
        .await
        .unwrap();
    assert!(text.content.unwrap().is_string());
    let bodies = mock.bodies(MESSAGES);
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0]["output_config"]["format"]["type"], "json_schema");
    assert_eq!(
        bodies[0]["output_config"]["format"]["schema"],
        label_schema()
    );
    assert_eq!(bodies[0]["system"], "Antworte knapp.");
    assert_eq!(bodies[0]["messages"][0]["role"], "user");
    assert!(bodies[1].get("output_config").is_none());
    for record in mock.requests_to(MESSAGES) {
        assert!(!record.body.contains("output_format"), "{}", record.body);
    }
}

// spec: AI-002
// spec: AI-038
// `stop_reason: "refusal"` → Refusal-Kennzeichen, kein Parse-Versuch, keine Wiederholung (genau 1 Request).
#[tokio::test]
async fn refusal_is_flagged_without_parsing_or_retrying() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = limiter(LONG_TIMEOUT, 1);
    let request = schema_request(anthropic_model(), "MOCK-SCENARIO:refusal", label_schema());
    let execution = run(&limiter, &adapter, &request).await;
    assert_eq!(execution.attempts, 1);
    let reply = execution.result.as_ref().unwrap();
    assert!(reply.is_refusal());
    assert_eq!(reply.raw, RawAnswer::Refusal);
    assert_eq!(reply.stop_reason.as_deref(), Some("refusal"));
    assert_eq!(
        status(&execution, request.response_schema.as_ref()),
        AiStatus::Refusal
    );
    let response = reply
        .clone()
        .into_response(request.response_schema.as_ref())
        .unwrap();
    assert!(response.refusal);
    assert_eq!(response.content, None);
    assert_eq!(
        provider::execute(PreviewSwitch::ON, &adapter, request).await,
        Outcome::Status(AiStatus::Refusal)
    );
    assert_eq!(mock.requests_to(MESSAGES).len(), 2);
}

// spec: AI-002
// spec: AI-001
// `usage.input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens` → einheitliche
// Felder; `input_tokens` zählt einschließlich Cache-Lesen und -Schreiben (Festlegung 24.11.2026); fehlende Felder
// bleiben `null`.
#[tokio::test]
async fn usage_is_mapped_to_the_common_fields() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let text = "MOCK-ID:home Zähl mich";
    let reply = adapter
        .send(&text_request(anthropic_model(), text), None)
        .await
        .unwrap();
    let body = &mock.bodies(MESSAGES)[0];
    let expected_input = os_testsite::mocks::ai::token_count(body) as u64;
    assert_eq!(reply.usage.input_tokens, Some(expected_input));
    assert!(reply.usage.output_tokens.unwrap() > 0);
    assert_eq!(reply.usage.cached_input_tokens, Some(0));
    assert_eq!(reply.usage.cache_write_tokens, Some(0));

    let cached = usage_from_json(Some(&json!({
        "input_tokens": 200, "output_tokens": 30,
        "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 50
    })));
    assert_eq!(cached.input_tokens, Some(1250));
    assert_eq!(cached.output_tokens, Some(30));
    assert_eq!(cached.cached_input_tokens, Some(1000));
    assert_eq!(cached.cache_write_tokens, Some(50));
    let partial = usage_from_json(Some(&json!({"input_tokens": 7})));
    assert_eq!(partial.input_tokens, Some(7));
    assert_eq!(partial.output_tokens, None);
    assert_eq!(partial.cached_input_tokens, None);
    assert_eq!(partial.cache_write_tokens, None);
    assert_eq!(usage_from_json(None), Default::default());
}

// spec: AI-002
// spec: AI-021
// spec: AI-002 [M4]
// `stop_reason: "max_tokens"` bei einer Schema-Aufgabe: abgeschnittenes JSON wird nicht teilweise übernommen. Nachtrag
// 24.11.2026 (Q00-H1) ersetzt „→ `Error: Invalid output` nach Reparaturversuch“: Abbruch an der Ausgabegrenze als eigene
// Antwortklasse mit `Error Code = max_tokens`, kein Reparaturversuch (genau 1 Request). Den Status
// `Error: Output limit reached` (M4-K10, freigegeben 26.11.2026).
#[tokio::test]
async fn max_tokens_on_a_schema_task_is_an_output_limit_without_repair() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let request = schema_request(
        anthropic_model(),
        "MOCK-SCENARIO:max-tokens",
        contract_schema(),
    );
    let reply = adapter.send(&request, None).await.unwrap();
    assert_eq!(reply.stop_reason.as_deref(), Some("max_tokens"));
    assert_eq!(reply.output_limit.as_deref(), Some("max_tokens"));
    assert!(matches!(reply.raw, RawAnswer::Unparsable { .. }));
    assert_eq!(
        reply.into_response(request.response_schema.as_ref()),
        Err(ProviderError::output_limit("max_tokens"))
    );
    assert_eq!(
        provider::execute(PreviewSwitch::ON, &adapter, request.clone()).await,
        Outcome::Status(AiStatus::OutputLimitReached)
    );
    let before = mock.requests_to(MESSAGES).len();
    let schema = ResponseSchema::new(contract_schema()).unwrap();
    let run = run_with_repair(&schema, RepairAttempts::default(), |repair| {
        let adapter = &adapter;
        let request = &request;
        async move {
            match adapter.send(request, repair.as_ref()).await {
                Ok(reply) => reply.for_repair().map_err(|limit| limit.attempt_failure()),
                Err(failure) => Err(failure),
            }
        }
    })
    .await;
    let StructuredOutcome::Failed(failure) = run.outcome else {
        panic!("erwartet Abbruch ohne Reparatur: {:?}", run.outcome);
    };
    assert_eq!(failure.error_code.as_deref(), Some("max_tokens"));
    assert_eq!(failure.http_status, Some(200));
    assert_eq!(mock.requests_to(MESSAGES).len(), before + 1);
}

// spec: AI-002
// spec: INTEG-005
// HTTP 529 wird wie 503 nach INTEG-005 wiederholt: dauerhaft 529 → 1 + 5 Requests, einmalig 529 → `Success`.
#[tokio::test]
async fn overloaded_529_is_retried_like_503() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = limiter(LONG_TIMEOUT, 1);
    let always = run(
        &limiter,
        &adapter,
        &text_request(anthropic_model(), "MOCK-SCENARIO:529"),
    )
    .await;
    assert_eq!(always.attempts, 6);
    assert_eq!(mock.requests_to(MESSAGES).len(), 6);
    assert_eq!(always.http_status(), Some(529));
    assert!(matches!(
        status(&always, None),
        AiStatus::HttpError { code: 529, .. }
    ));
    // Backoff 1, 2, 4, 8, 16 s auf der Testuhr.
    let gaps: Vec<_> = always
        .attempt_starts
        .windows(2)
        .map(|w| (w[1] - w[0]).as_secs_f64().round() as u64)
        .collect();
    assert_eq!(gaps, vec![1, 2, 4, 8, 16]);

    mock.scenario("anthropic", "529", 1).await;
    let once = run(
        &limiter,
        &adapter,
        &text_request(anthropic_model(), "MOCK-ID:home"),
    )
    .await;
    assert_eq!(once.attempts, 2);
    assert_eq!(status(&once, None), AiStatus::Success);
    assert!(!adapter.quota().is_tripped());
}

async fn run_entries(
    limiter: Arc<Limiter>,
    adapter: Arc<AnthropicAdapter>,
    entries: usize,
) -> Vec<Execution<AdapterReply>> {
    let mut set = tokio::task::JoinSet::new();
    for i in 0..entries {
        let limiter = limiter.clone();
        let adapter = adapter.clone();
        set.spawn(async move {
            let request = text_request(anthropic_model(), &format!("MOCK-ID:home Eintrag {i}"));
            run(&limiter, &adapter, &request).await
        });
    }
    let mut out = Vec::new();
    while let Some(execution) = set.join_next().await {
        out.push(execution.unwrap());
    }
    out
}

// spec: AI-002
// spec: AI-020
// Kontingent-Szenarien → genau 1 Request bei „gleichzeitige Anfragen = 1“ (bei n gleichzeitigen höchstens n), alle
// offenen Anthropic-Einträge `Quota exceeded` ohne weiteren Request; andere Anbieter (OpenRouter) laufen weiter.
#[tokio::test]
async fn quota_scenarios_stop_every_open_anthropic_entry_after_one_request() {
    for (scenario, http, code) in [
        ("insufficient-quota", 400, "invalid_request_error"),
        ("spend-limit-user", 400, "invalid_request_error"),
        ("spend-limit-workspace", 400, "invalid_request_error"),
        ("402-billing", 402, "billing_error"),
        ("429-spend-cap", 429, "enforced_spend_limit_reached"),
    ] {
        let mock = Mock::start().await;
        mock.scenario("anthropic", scenario, 1000).await;
        let limiter = limiter(LONG_TIMEOUT, 1);
        let anthropic = Arc::new(adapter(&mock));
        let results = run_entries(limiter.clone(), anthropic.clone(), 5).await;
        assert_eq!(mock.requests_to(MESSAGES).len(), 1, "{scenario}");
        assert!(
            results
                .iter()
                .all(|e| status(e, None) == AiStatus::QuotaExceeded),
            "{scenario}: {results:?}"
        );
        let sent: Vec<_> = results
            .iter()
            .filter(|e| e.http_status().is_some())
            .collect();
        assert_eq!(sent.len(), 1, "{scenario}");
        assert_eq!(sent[0].http_status(), Some(http), "{scenario}");
        assert_eq!(sent[0].error_code(), Some(code), "{scenario}");
        assert!(
            results.iter().all(|e| e.attempts == 1),
            "{scenario}: kein Backoff"
        );
        let stop = anthropic.quota().stop().unwrap();
        assert_eq!(stop.http_status, http);
        assert!(!limiter.breaker_open("anthropic"));

        // Anderer Anbieter im selben Lauf.
        let openrouter = OpenRouterAdapter::new(reqwest::Client::new(), mock.openrouter_config());
        let request = text_request(openrouter_model(), "MOCK-ID:home weiter");
        let other = limiter
            .run(&RequestSpec::new("openrouter", "mock/vision", 10), |_| {
                openrouter.send(&request, None)
            })
            .await;
        assert_eq!(status(&other, None), AiStatus::Success, "{scenario}");
    }

    // n = 3 gleichzeitige Anfragen → höchstens 3 Requests für 9 Einträge.
    let mock = Mock::start().await;
    mock.scenario("anthropic", "402-billing", 1000).await;
    let results = run_entries(limiter(LONG_TIMEOUT, 3), Arc::new(adapter(&mock)), 9).await;
    let sent = mock.requests_to(MESSAGES).len();
    assert!((1..=3).contains(&sent), "{sent}");
    assert!(results
        .iter()
        .all(|e| status(e, None) == AiStatus::QuotaExceeded));
}

// spec: AI-002
// spec: AI-017
// `429-spend-cap` wird nicht wiederholt (kein Backoff, kein `Error: 429 …`, zählt nicht für den Schutzschalter); ein
// 429 ohne `enforced_spend_limit_reached` (`429x2`) wird wiederholt und endet mit `Success`.
#[tokio::test]
async fn spend_cap_429_is_final_but_a_plain_429_is_retried() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = limiter_with(LONG_TIMEOUT, 1, |c| c.breaker_threshold = 1);
    let cap = run(
        &limiter,
        &adapter,
        &text_request(anthropic_model(), "MOCK-SCENARIO:429-spend-cap"),
    )
    .await;
    assert_eq!(cap.attempts, 1);
    assert_eq!(cap.duration, Duration::ZERO);
    assert_eq!(status(&cap, None), AiStatus::QuotaExceeded);
    assert_eq!(cap.error_code(), Some("enforced_spend_limit_reached"));
    assert!(!limiter.breaker_open("anthropic"), "Spend-Cap zählt nicht");

    let mock = Mock::start().await;
    let adapter = AnthropicAdapter::new(reqwest::Client::new(), mock.anthropic_config());
    let limiter = support::limiter(LONG_TIMEOUT, 1);
    let plain = run(
        &limiter,
        &adapter,
        &text_request(anthropic_model(), "MOCK-SCENARIO:429x2"),
    )
    .await;
    assert_eq!(plain.attempts, 3);
    assert_eq!(mock.requests_to(MESSAGES).len(), 3);
    assert_eq!(status(&plain, None), AiStatus::Success);
    assert!(!adapter.quota().is_tripped());
}

fn level(body: &Value) -> Option<&str> {
    body.pointer("/output_config/effort")
        .and_then(Value::as_str)
}

// spec: AI-002
// spec: AI-001
// Modelle A–D aus `/anthropic/v1/models`: `output_config.effort` nur bei A und B, bei B nie `max`, bei C und D nie;
// nie ein Top-Level-Feld `effort`. Fähigkeiten live aus `capabilities`; D ohne `capabilities` = „Fähigkeiten
// unbekannt“.
#[tokio::test]
async fn effort_and_capabilities_come_live_from_the_model_list() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let models = adapter.list_models().await.unwrap();
    let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "mock-claude",
            "mock-claude-b",
            "mock-claude-c",
            "mock-claude-d"
        ]
    );
    let a = &models[0];
    assert_eq!(a.display_name.as_deref(), Some("A"));
    assert_eq!(a.max_input_tokens, Some(200_000));
    assert_eq!(a.max_tokens, Some(8192));
    // Nachtrag Q00 (AI-002): A und B melden zusätzlich `xhigh` (Prüfwert „`max` auf B → `xhigh`“).
    assert_eq!(a.effort_levels, ["low", "medium", "high", "xhigh", "max"]);
    assert_eq!(models[1].effort_levels, ["low", "medium", "high", "xhigh"]);
    assert!(!models[2].effort_supported && models[2].effort_levels.is_empty());
    assert!(!models[3].capabilities_known && !models[3].effort_supported);

    let mut sent = Vec::new();
    for model in &models {
        for wanted in ["max", "high"] {
            let mut request = text_request(model.to_model(), "MOCK-ID:home Aufwand");
            request.effort = Some(wanted.into());
            adapter.send(&request, None).await.unwrap();
            sent.push((model.id.clone(), wanted));
        }
    }
    let bodies = mock.bodies(MESSAGES);
    assert_eq!(bodies.len(), sent.len());
    let effort: Vec<_> = bodies.iter().map(level).collect();
    assert_eq!(
        effort,
        [
            Some("max"),
            Some("high"),
            None,
            Some("high"),
            None,
            None,
            None,
            None
        ]
    );
    assert!(bodies.iter().all(|b| b.get("effort").is_none()));
    assert!(models[1].effort_unsupported("max"));
    assert!(!models[0].effort_unsupported("max"));

    let schema_task = Capabilities {
        text: true,
        structured_output: true,
        ..Capabilities::default()
    };
    let image_task = Capabilities {
        text: true,
        vision: true,
        ..Capabilities::default()
    };
    for required in [schema_task, image_task] {
        assert_eq!(
            models[0].selectable_for(required),
            Selectability::Selectable
        );
        assert_eq!(
            models[1].selectable_for(required),
            Selectability::Selectable
        );
        assert_eq!(
            models[2].selectable_for(required),
            Selectability::Unavailable {
                capabilities_unknown: false
            }
        );
        assert_eq!(
            models[3].selectable_for(required),
            Selectability::Unavailable {
                capabilities_unknown: true
            }
        );
    }
    assert!(models[0].runs_in_batch() && !models[2].runs_in_batch() && !models[3].runs_in_batch());
    let generic: Vec<_> = adapter.models().await.unwrap();
    assert_eq!(generic[0].capabilities, a.capabilities());
    assert_eq!(generic[0].endpoint, "messages");
    assert!(!generic[2].capabilities.structured_output && !generic[3].capabilities.vision);

    // Schema-Aufgabe an C: kein Request (Skipped (no capable model)).
    let before = mock.requests_to(MESSAGES).len();
    let request = schema_request(models[2].to_model(), "MOCK-ID:home", contract_schema());
    assert_eq!(
        provider::execute(PreviewSwitch::ON, &adapter, request).await,
        Outcome::Status(AiStatus::SkippedNoCapableModel)
    );
    assert_eq!(mock.requests_to(MESSAGES).len(), before);
}

// spec: AI-002
// Ändert der Mock die `capabilities` eines Modells, gilt die Änderung nach erneutem Laden der Modellliste.
#[tokio::test]
async fn changed_capabilities_apply_after_reloading_the_model_list() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let before = adapter.list_models().await.unwrap();
    assert!(!before[2].structured_output);
    mock.mock.set_models(
        "anthropic",
        json!({"data": [{
            "id": "mock-claude-c", "display_name": "C",
            "capabilities": {
                "structured_outputs": {"supported": true},
                "image_input": {"supported": false},
                "batch": {"supported": true},
                "effort": {"supported": true, "low": {"supported": true}, "max": {"supported": false}}
            }
        }], "has_more": false}),
    );
    let after = adapter.list_models().await.unwrap();
    assert_eq!(after.len(), 1);
    assert!(after[0].structured_output && after[0].batch && !after[0].vision);
    assert_eq!(after[0].effort_levels, ["low"]);
    assert_eq!(
        adapter.cached_model("mock-claude-c"),
        Some(after[0].clone())
    );
    let mut request = text_request(after[0].to_model(), "MOCK-ID:home");
    request.effort = Some("low".into());
    adapter.send(&request, None).await.unwrap();
    assert_eq!(level(mock.bodies(MESSAGES).last().unwrap()), Some("low"));
}

// spec: AI-002
// Bilder als Inhaltsblock `image` mit Base64-Quelle und korrektem `media_type` (PNG, JPEG, WebP, GIF) aus der
// Fixture-Site; SVG ist kein unterstütztes Format.
#[tokio::test]
async fn images_are_sent_as_base64_blocks_with_their_media_type() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("ai")
        .join("site")
        .join("images");
    let files = [
        ("forest.png", "image/png"),
        ("trail.jpg", "image/jpeg"),
        ("boots.webp", "image/webp"),
        ("decoration.gif", "image/gif"),
    ];
    let mut input = vec![Input::Text {
        text: "MOCK-ID:home Beschreibe die Bilder".into(),
    }];
    let mut encoded = Vec::new();
    for (name, media_type) in files {
        let bytes = std::fs::read(dir.join(name)).unwrap();
        assert_eq!(sniff_image_media_type(&bytes), Some(media_type), "{name}");
        let image = image_input(&bytes).unwrap();
        if let Input::Image { base64, .. } = &image {
            encoded.push(base64.clone());
        }
        input.push(image);
    }
    assert_eq!(
        image_input(&std::fs::read(dir.join("map.svg")).unwrap()),
        None
    );

    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let mut request = text_request(anthropic_model(), "");
    request.input = input;
    request.required.vision = true;
    let reply = adapter.send(&request, None).await.unwrap();
    let body = &mock.bodies(MESSAGES)[0];
    let blocks = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(blocks.len(), 5);
    for (i, (_, media_type)) in files.iter().enumerate() {
        let block = &blocks[i + 1];
        assert_eq!(block["type"], "image");
        assert_eq!(block["source"]["type"], "base64");
        assert_eq!(block["source"]["media_type"], *media_type);
        assert_eq!(block["source"]["data"], encoded[i].as_str());
    }
    // Mock-Zählung: 85 Tokens je Bild.
    let text_only = os_testsite::mocks::ai::token_count(&json!({
        "system": body["system"], "messages": [{"content": [blocks[0].clone()]}]
    })) as u64;
    assert_eq!(reply.usage.input_tokens, Some(text_only + 4 * 85));
}

// spec: AI-021
// Über den echten Adapter: `malformed-json` einmalig → Reparatur, gültig nach 2 Requests; `schema-violation`
// dauerhaft → `Error: Invalid output` nach 2 Requests.
#[tokio::test]
async fn repair_runs_through_the_adapter() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let schema = ResponseSchema::new(label_schema()).unwrap();
    let request = schema_request(anthropic_model(), "MOCK-ID:k01-structured", label_schema());
    mock.scenario("anthropic", "malformed-json", 1).await;
    let repaired = run_with_repair(&schema, RepairAttempts::default(), |repair| {
        let adapter = &adapter;
        let request = &request;
        async move {
            adapter
                .send(request, repair.as_ref())
                .await
                .map(|reply| (reply.raw.clone(), reply))
        }
    })
    .await;
    assert_eq!(
        repaired.outcome,
        StructuredOutcome::Valid(json!({"ok": true, "label": "k01"}))
    );
    assert_eq!(repaired.requests.len(), 2);
    let bodies = mock.bodies(MESSAGES);
    assert!(bodies[1]
        .to_string()
        .contains("rejected by the JSON schema validator"));

    let violating = schema_request(
        anthropic_model(),
        "MOCK-SCENARIO:schema-violation",
        label_schema(),
    );
    let failed = run_with_repair(&schema, RepairAttempts::default(), |repair| {
        let adapter = &adapter;
        let request = &violating;
        async move {
            adapter
                .send(request, repair.as_ref())
                .await
                .map(|reply| (reply.raw.clone(), reply))
        }
    })
    .await;
    assert!(matches!(
        failed.outcome,
        StructuredOutcome::InvalidOutput { .. }
    ));
    assert_eq!(failed.requests.len(), 2);
    assert!(failed.requests[1].validator_error.is_some());
    assert_eq!(mock.requests_to(MESSAGES).len(), 4);
}

// spec: AI-017
// spec: INTEG-005
// ai-mock-Szenarien über den echten Adapter (Testuhr): `retry-after-7` → zweiter Versuch frühestens nach 7 s;
// `500-always` → 1 + 5 Requests, `Error: 500 …`; `hang` mit Timeout → `Error: Timeout nach 1 s` nach 1 + 5 Versuchen.
#[tokio::test]
async fn mock_scenarios_through_the_real_adapter() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = limiter(LONG_TIMEOUT, 1);
    let retry = run(
        &limiter,
        &adapter,
        &text_request(anthropic_model(), "MOCK-SCENARIO:retry-after-7"),
    )
    .await;
    assert_eq!(retry.attempts, 2);
    assert_eq!(
        retry.attempt_starts[1] - retry.attempt_starts[0],
        Duration::from_secs(7)
    );
    assert_eq!(status(&retry, None), AiStatus::Success);
    let failing = run(
        &limiter,
        &adapter,
        &text_request(anthropic_model(), "MOCK-SCENARIO:500-always"),
    )
    .await;
    assert_eq!(failing.attempts, 6);
    assert!(matches!(
        status(&failing, None),
        AiStatus::HttpError { code: 500, .. }
    ));
    assert_eq!(mock.requests_to(MESSAGES).len(), 2 + 6);

    let timeout = Duration::from_millis(1250);
    let limiter = support::limiter(timeout, 1);
    let hang = run(
        &limiter,
        &adapter,
        &text_request(anthropic_model(), "MOCK-SCENARIO:hang"),
    )
    .await;
    assert_eq!(hang.attempts, 6);
    assert_eq!(hang.http_status(), None);
    assert_eq!(status(&hang, None), AiStatus::Timeout { seconds: 1 });
}

// spec: AI-017
// Rate-Limit-Header einer Anthropic-Antwort gehen über `Limiter::observe_headers` in die Begrenzung:
// `anthropic-ratelimit-requests-remaining: 0`, Reset in 20 s → der nächste Request startet frühestens nach 20 s (Testuhr),
// es entsteht kein 429. (ai-mock sendet diese Header nicht; eigener Testserver.)
#[tokio::test]
async fn rate_limit_headers_of_a_response_pause_the_next_request() {
    let message = json!({
        "id": "msg_1", "type": "message", "role": "assistant",
        "content": [{"type": "text", "text": "ok"}], "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 1}
    })
    .to_string();
    let server = HeaderServer::start(vec![Canned {
        status: 200,
        headers: vec![
            ("anthropic-ratelimit-requests-remaining".into(), "0".into()),
            (
                "anthropic-ratelimit-requests-reset".into(),
                "2026-11-24T10:00:20Z".into(),
            ),
            ("anthropic-ratelimit-tokens-remaining".into(), "0".into()),
        ],
        body: message,
    }])
    .await;
    let limiter = limiter(LONG_TIMEOUT, 1);
    let config = AnthropicConfig::new(ApiKey::new(KEY)).with_base_url(server.base_url());
    let adapter =
        AnthropicAdapter::new(reqwest::Client::new(), config).with_limiter(limiter.clone());
    let request = text_request(anthropic_model(), "Hallo");
    let first = run(&limiter, &adapter, &request).await;
    let second = run(&limiter, &adapter, &request).await;
    assert_eq!(status(&first, None), AiStatus::Success);
    assert_eq!(status(&second, None), AiStatus::Success);
    assert_eq!(second.attempts, 1, "kein 429");
    assert_eq!(
        second.attempt_starts[0] - first.attempt_starts[0],
        Duration::from_secs(20)
    );
    assert_eq!(server.requests(), 2);
}

// spec: AI-002
// Optional `POST /v1/messages/count_tokens` (AI-012): liefert die Eingabe-Tokens des Requests.
#[tokio::test]
async fn count_tokens_returns_the_input_tokens() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let request = text_request(anthropic_model(), "MOCK-ID:home zwölf Zeichen");
    let counted = adapter.count_tokens(&request).await.unwrap();
    let body = &mock.bodies("/anthropic/v1/messages/count_tokens")[0];
    assert!(body.get("max_tokens").is_none());
    assert_eq!(counted, os_testsite::mocks::ai::token_count(body) as u64);
}

// spec: AI-001
// Contract-Test: dieselbe Aufgabe („gib `{"ok": true}` nach Schema zurück“) über Anthropic und OpenRouter ergibt
// dasselbe geparste Ergebnis.
#[tokio::test]
async fn contract_task_gives_the_same_result_on_anthropic_and_openrouter() {
    let mock = Mock::start().await;
    let anthropic = adapter(&mock);
    let openrouter = OpenRouterAdapter::new(reqwest::Client::new(), mock.openrouter_config());
    let text = "MOCK-ID:k01-contract Gib {\"ok\": true} zurück";
    let providers: [(&dyn AiProvider, Request); 2] = [
        (
            &anthropic,
            schema_request(anthropic_model(), text, contract_schema()),
        ),
        (
            &openrouter,
            schema_request(openrouter_model(), text, contract_schema()),
        ),
    ];
    for (provider, request) in providers {
        match provider::execute(PreviewSwitch::ON, provider, request).await {
            Outcome::Completed(response) => {
                assert_eq!(
                    response.content,
                    Some(json!({"ok": true})),
                    "{}",
                    provider.provider_id()
                );
                assert!(response.usage.input_tokens.is_some());
            }
            other => panic!("{}: {other:?}", provider.provider_id()),
        }
    }
}

// spec: AI-002
// Modelleintrag ohne `capabilities` bzw. mit fehlendem Knoten: Fähigkeit gilt als nicht vorhanden.
#[test]
fn missing_capability_nodes_count_as_unsupported() {
    let model = AnthropicModel::from_json(&json!({
        "id": "x", "capabilities": {"structured_outputs": {"supported": true}, "effort": {"supported": true}}
    }))
    .unwrap();
    assert!(model.capabilities_known && model.structured_output);
    assert!(!model.vision && !model.batch);
    assert!(model.effort_supported && model.effort_levels.is_empty());
    assert_eq!(model.effort_to_send("high"), None);
    assert!(AnthropicModel::from_json(&json!({"display_name": "ohne id"})).is_none());
}

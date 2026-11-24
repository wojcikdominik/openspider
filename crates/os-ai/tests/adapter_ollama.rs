//! Adapter Ollama (AI-006) und Ollama-Vorgabe „1 gleichzeitige Anfrage“ (INTEG-005 [M4]) gegen ai-mock (M4-K03).
//!
//! Der ai-mock läuft auf einem Loopback-Port (`/ollama/api/{chat, embed, tags}`); für den nicht erreichbaren Server
//! wird ein freier Port belegt und wieder freigegeben („Test ohne laufenden Mock-Port“). Antworten mit Markierung
//! `MOCK-ID:k03-…` stehen in `tests/fixtures/ai/mock-answers.d/k03.json`.
#[path = "adapter_anthropic_support.rs"]
mod support;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use os_ai::adapters::ollama::{
    self, ConnectionError, OllamaAdapter, OllamaConfig, CONCURRENT_REQUESTS_RANGE,
    DEFAULT_BASE_URL, DEFAULT_CONCURRENT_REQUESTS, DEFAULT_TIMEOUT_SECONDS,
};
use os_ai::cost::log::{request_record, LogContext, RequestOutcome};
use os_ai::embeddings::{embed_texts, EmbeddingSettings, NoEmbeddingCache};
use os_ai::limits::{FixedJitter, Limiter, LimitsConfig, ProviderLimits};
use os_ai::provider::{AiProvider, Capabilities, CapabilitySource, Input, Model, ProviderError};
use os_ai::schema::{
    run_with_repair, RepairAttempts, RepairRequest, ResponseSchema, StructuredOutcome,
};
use os_ai::AiStatus;
use os_model::budget::MicroUsd;
use os_model::config::{AiAppSettings, ProviderEndpoint};
use os_testsite::mocks::ai::token_count;
use serde_json::json;
use support::*;

const CHAT: &str = "/ollama/api/chat";
const EMBED: &str = "/ollama/api/embed";

fn ollama_config(mock: &Mock) -> OllamaConfig {
    OllamaConfig::new().with_base_url(mock.server.base_url("ollama"))
}

fn adapter(mock: &Mock) -> OllamaAdapter {
    OllamaAdapter::new(reqwest::Client::new(), ollama_config(mock))
}

fn ollama_model() -> Model {
    Model {
        provider: "ollama".into(),
        id: "mock-llama".into(),
        endpoint: ollama::ENDPOINT.into(),
        capabilities: ollama::default_capabilities(),
        capability_source: CapabilitySource::ModelCatalog,
    }
}

/// Limiter mit Testuhr, `concurrent` gleichzeitigen Anfragen für Ollama und OpenRouter.
fn test_limiter(concurrent: u32) -> Arc<Limiter> {
    limiter_with(LONG_TIMEOUT, concurrent, |config| {
        config.providers.insert(
            "ollama".into(),
            ProviderLimits {
                concurrent_requests: concurrent,
                timeout: LONG_TIMEOUT,
            },
        );
    })
}

/// Limiter mit den Standardwerten der App-Einstellungen (Ollama: 1 gleichzeitige Anfrage, 300 s Timeout).
fn default_limiter() -> Limiter {
    let timeout = Duration::from_secs(u64::from(DEFAULT_TIMEOUT_SECONDS));
    Limiter::with_jitter(
        LimitsConfig::default(),
        Arc::new(VirtualClock::new(timeout)),
        Arc::new(FixedJitter(0.5)),
    )
}

fn closed_port_url() -> String {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}")
}

// spec: AI-006
// spec: AI-021
// Schema-Aufgaben senden das Schema im Feld `format` (ohne Streaming); die Antwort wird lokal validiert.
#[tokio::test]
async fn schema_tasks_send_the_schema_in_format_and_validate_the_answer() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = test_limiter(1);
    let request = schema_request(
        ollama_model(),
        "Seite MOCK-ID:k03-structured",
        label_schema(),
    );
    let execution = adapter.run(&limiter, &request, None, 100).await;
    assert_eq!(execution.attempts, 1);
    let response = execution
        .result
        .unwrap()
        .into_response(request.response_schema.as_ref())
        .unwrap();
    assert_eq!(response.content, Some(json!({"ok": true, "label": "k03"})));
    assert!(!response.refusal);

    let bodies = mock.bodies(CHAT);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["format"], label_schema());
    assert_eq!(bodies[0]["stream"], json!(false));
    assert_eq!(bodies[0]["model"], json!("mock-llama"));
    assert_eq!(bodies[0]["messages"][0]["role"], json!("system"));
    assert_eq!(bodies[0]["options"]["num_predict"], json!(256));

    // Text-Aufgaben senden kein `format`.
    let text = text_request(ollama_model(), "Nur Text");
    let reply = adapter.send(&text, None).await.unwrap();
    assert!(mock.bodies(CHAT)[1].get("format").is_none());
    assert_eq!(
        reply.into_response(None).unwrap().content,
        Some(json!("{\"ok\":true}"))
    );
}

// spec: AI-006
// spec: AI-021
// Eine schemawidrige Antwort wird verworfen; nach dem Reparaturversuch (Standard 1) endet der Eintrag mit
// `Error: Invalid output`.
#[tokio::test]
async fn schema_violations_are_repaired_once_then_invalid_output() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    mock.scenario("ollama", "schema-violation", 2).await;
    let request = schema_request(ollama_model(), "Seite", contract_schema());
    let schema = ResponseSchema::new(contract_schema()).unwrap();
    let run = run_with_repair(
        &schema,
        RepairAttempts::default(),
        |repair: Option<RepairRequest>| {
            let adapter = &adapter;
            let request = &request;
            async move {
                adapter
                    .send(request, repair.as_ref())
                    .await
                    .map(|reply| (reply.raw, ()))
                    .map_err(|failure| failure.error)
            }
        },
    )
    .await;
    assert!(matches!(
        run.outcome,
        StructuredOutcome::InvalidOutput { .. }
    ));
    assert_eq!(run.outcome.status(), Some(AiStatus::InvalidOutput));
    let bodies = mock.bodies(CHAT);
    assert_eq!(bodies.len(), 2);
    let repair_text = bodies[1]["messages"][1]["content"].as_str().unwrap();
    assert!(repair_text.contains("rejected by the JSON schema validator"));
}

// spec: AI-001
// spec: AI-006
// Contract-Test aus AI-001 mit dem Ollama-Adapter: „gib {"ok": true} nach Schema zurück“; Routing über die
// gemeinsame Ausführung, Fähigkeiten aus dem Katalog (Local, Embeddings, Structured Output, Vision nur laut Katalog).
#[tokio::test]
async fn contract_task_returns_ok_true_through_the_common_interface() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let outcome = os_ai::provider::execute(
        os_ai::PreviewSwitch::ON,
        &adapter,
        schema_request(ollama_model(), "Contract", contract_schema()),
    )
    .await;
    match outcome {
        os_ai::provider::Outcome::Completed(response) => {
            assert_eq!(response.content, Some(json!({"ok": true})));
            assert!(response.usage.input_tokens.is_some());
            assert_eq!(response.usage.cached_input_tokens, None);
            assert_eq!(response.usage.cache_write_tokens, None);
        }
        other => panic!("unexpected outcome {other:?}"),
    }

    // Bildaufgabe an ein Modell ohne Vision: kein Request.
    let mut image = text_request(ollama_model(), "Bild");
    image.input.push(Input::Image {
        media_type: "image/png".into(),
        base64: "iVBORw0KGgo=".into(),
    });
    let before = mock.requests_to(CHAT).len();
    let outcome = os_ai::provider::execute(os_ai::PreviewSwitch::ON, &adapter, image.clone()).await;
    assert_eq!(
        outcome,
        os_ai::provider::Outcome::Status(AiStatus::SkippedNoCapableModel)
    );
    assert_eq!(mock.requests_to(CHAT).len(), before);

    // Laut Katalog Vision-fähig: Bild geht als Base64 in `images`.
    let vision = Capabilities {
        vision: true,
        ..ollama::default_capabilities()
    };
    let adapter = adapter.with_capabilities(BTreeMap::from([("mock-llava".to_string(), vision)]));
    let model = adapter.to_model("mock-llava");
    assert!(model.capabilities.vision && model.capabilities.local);
    assert_eq!(model.capability_source, CapabilitySource::ModelCatalog);
    image.model = model;
    adapter.send(&image, None).await.unwrap();
    let body = mock.bodies(CHAT).last().cloned().unwrap();
    assert_eq!(body["messages"][1]["images"], json!(["iVBORw0KGgo="]));

    // Modellliste aus /api/tags.
    let models = adapter.models().await.unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "mock-llama");
    assert!(models[0].capabilities.embeddings && models[0].capabilities.local);
    assert!(!models[0].capabilities.vision);
}

// spec: AI-006
// spec: AI-014
// Kostenprotokoll: `Cost = 0`, Tokenzahlen aus `prompt_eval_count` (Eingabe) und `eval_count` (Ausgabe); auch für
// Embeddings (`/api/embed`).
#[tokio::test]
async fn cost_log_shows_zero_cost_and_the_eval_counts() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = test_limiter(1);
    let request = text_request(ollama_model(), "Fasse die Seite zusammen.");
    let execution = adapter.run(&limiter, &request, None, 50).await;
    let reply = execution.result.unwrap();
    let body = mock.bodies(CHAT).remove(0);
    let expected_input = token_count(&body) as u64;
    let answer = "{\"ok\":true}";
    let expected_output = answer.chars().count().div_ceil(4) as u64;
    assert_eq!(reply.usage.input_tokens, Some(expected_input));
    assert_eq!(reply.usage.output_tokens, Some(expected_output));
    let ctx = LogContext {
        run_id: 1,
        timestamp: "2026-11-24T10:00:00Z".into(),
        task: "suggest-title".into(),
        provider: ollama::PROVIDER_ID.into(),
        model: "mock-llama".into(),
        endpoint: ollama::ENDPOINT.into(),
        url_id: Some(7),
        definition_version: None,
        batch_id: None,
        redactions: 0,
    };
    let outcome = RequestOutcome::success(reply.usage.clone(), 0, reply.duration_ms);
    let record = request_record(&ctx, &ollama::rates(), MicroUsd(0), &outcome);
    assert_eq!(record.cost, MicroUsd(0));
    assert_eq!(record.token_cost, MicroUsd(0));
    assert_eq!(record.input_tokens, Some(expected_input));
    assert_eq!(record.output_tokens, Some(expected_output));
    assert_eq!(record.status, "Success");
    assert!(!record.over_reservation);

    // Embeddings: `prompt_eval_count` → Eingabe-Tokens, Kosten 0.
    let texts = vec!["Laufschuhe".to_string(), "Trailschuhe".to_string()];
    let run = embed_texts(
        &adapter,
        &limiter,
        &NoEmbeddingCache,
        &EmbeddingSettings::new("mock-embed"),
        &texts,
    )
    .await
    .unwrap();
    assert_eq!(run.requests.len(), 1);
    let embed_body = mock.bodies(EMBED).remove(0);
    let outcome = run.requests[0].outcome();
    let record = request_record(
        &LogContext {
            task: "embedding".into(),
            endpoint: ollama::EMBEDDINGS_ENDPOINT.into(),
            ..ctx
        },
        &ollama::rates(),
        MicroUsd(0),
        &outcome,
    );
    assert_eq!(record.cost, MicroUsd(0));
    assert_eq!(record.input_tokens, Some(token_count(&embed_body) as u64));
    assert_eq!(record.output_tokens, None);
}

// spec: AI-006
// spec: AI-020
// Nicht erreichbarer Server (kein Mock-Port): alle offenen Ollama-Einträge erhalten `Error: <Meldung aus INTEG-160>`
// mit URL – gleicher Text wie der Verbindungsstatus; nach dem ersten endgültig gescheiterten Eintrag (Wiederholungen
// nach INTEG-005) geht kein weiterer Ollama-Request hinaus. Andere Anbieter laufen weiter.
#[tokio::test]
async fn unreachable_server_fails_all_open_ollama_entries_while_others_continue() {
    let mock = Mock::start().await;
    let url = closed_port_url();
    let adapter = OllamaAdapter::new(
        reqwest::Client::new(),
        OllamaConfig::new().with_base_url(url.clone()),
    );
    let limiter = test_limiter(1);
    let openrouter = os_ai::adapters::openrouter::OpenRouterAdapter::new(
        reqwest::Client::new(),
        mock.openrouter_config(),
    );

    // Verbindungsstatus nach INTEG-160.
    let connection = adapter.connect().await.unwrap_err();
    let expected = format!("Server nicht erreichbar ({url})");
    assert_eq!(
        connection,
        ConnectionError::Unreachable {
            message: expected.clone()
        }
    );
    assert_eq!(connection.to_string(), expected);

    let requests: Vec<_> = (0..3)
        .map(|i| text_request(ollama_model(), &format!("Eintrag {i}")))
        .collect();
    let other = text_request(openrouter_model(), "Anderer Anbieter");
    // Erster Eintrag: 1 Versuch + 5 Wiederholungen (Transportfehler, INTEG-005, K4-15), dann Merker.
    let first = adapter.run(&limiter, &requests[0], None, 10).await;
    assert_eq!(first.attempts, 6);
    // Die übrigen offenen Einträge laufen gleichzeitig mit einem OpenRouter-Eintrag.
    let (b, c, d) = tokio::join!(
        adapter.run(&limiter, &requests[1], None, 10),
        adapter.run(&limiter, &requests[2], None, 10),
        async {
            limiter
                .run(
                    &os_ai::limits::RequestSpec::new("openrouter", "mock/vision", 10),
                    |_| openrouter.send(&other, None),
                )
                .await
        },
    );
    assert_eq!((b.attempts, c.attempts), (0, 0));
    for execution in [&first, &b, &c] {
        let error = execution.result.as_ref().unwrap_err();
        assert_eq!(error.status, AiStatus::ConnectionFailed);
        assert_eq!(error.message, expected);
        assert_eq!(error.http_status, None);
        assert_eq!(error.error_code.as_deref(), Some("transport/connect"));
        assert_eq!(adapter.status_text(error), format!("Error: {expected}"));
        assert_eq!(adapter.status_text(error), format!("Error: {connection}"));
    }
    assert!(adapter.unreachable().is_tripped());
    assert_eq!(status(&d, None), AiStatus::Success);

    // Weitere Ollama-Einträge (Chat, Embeddings, Ausführung ohne Limiter) gehen nicht mehr hinaus.
    let later = adapter.run(&limiter, &requests[0], None, 10).await;
    assert_eq!(later.attempts, 0);
    assert_eq!(
        adapter.status_text(later.result.as_ref().unwrap_err()),
        format!("Error: {expected}")
    );
    let embeddings = embed_texts(
        &adapter,
        &limiter,
        &NoEmbeddingCache,
        &EmbeddingSettings::new("mock-embed"),
        &["Text".to_string()],
    )
    .await
    .unwrap();
    assert!(embeddings.requests.is_empty());
    let error = embeddings.results[0].error.as_ref().unwrap();
    assert_eq!(adapter.status_text(error), format!("Error: {expected}"));
    assert_eq!(
        adapter.execute(requests[1].clone()).await,
        Err(ProviderError::Transport {
            message: expected.clone()
        })
    );
    // OpenRouter hat genau einen Request erhalten.
    assert_eq!(
        mock.requests_to("/openrouter/api/v1/chat/completions")
            .len(),
        1
    );
}

// spec: AI-006
// Eine im Ollama-Konto (04, INTEG-160) geänderte Server-URL gilt ohne weitere Eingabe auch für 08: Der Adapter liest
// dieselbe Einstellung `provider_endpoints["ollama"].base_url`; ohne Angabe gilt `http://localhost:11434`.
#[tokio::test]
async fn server_url_comes_from_the_shared_ollama_setting() {
    let mock = Mock::start().await;
    let defaults = AiAppSettings::default();
    assert_eq!(
        OllamaConfig::from_app_settings(&defaults).base_url,
        DEFAULT_BASE_URL
    );
    assert_eq!(DEFAULT_BASE_URL, "http://localhost:11434");

    let mut settings = AiAppSettings::default();
    settings
        .provider_endpoints
        .get_mut("ollama")
        .unwrap()
        .base_url = Some(format!("{}/", mock.server.base_url("ollama")));
    let config = OllamaConfig::from_app_settings(&settings);
    assert_eq!(config.base_url, mock.server.base_url("ollama"));
    let adapter = OllamaAdapter::new(reqwest::Client::new(), config);
    let models = adapter.connect().await.unwrap();
    assert_eq!(models[0].name, "mock-llama");
    assert_eq!(models[0].parameter_size.as_deref(), Some("8B"));
    assert_eq!(mock.requests_to("/ollama/api/tags").len(), 1);
    // Die Meldung bei nicht erreichbarem Server nennt die geänderte URL.
    assert_eq!(
        adapter.unreachable_message(),
        format!(
            "Server nicht erreichbar ({})",
            mock.server.base_url("ollama")
        )
    );

    // Leere Angabe = Standard.
    settings.provider_endpoints.insert(
        "ollama".into(),
        ProviderEndpoint {
            base_url: Some("  ".into()),
            ..ProviderEndpoint::default()
        },
    );
    assert_eq!(
        OllamaConfig::from_app_settings(&settings).base_url,
        DEFAULT_BASE_URL
    );
}

fn chat_arrivals(mock: &Mock) -> Vec<u128> {
    let mut times: Vec<u128> = mock
        .requests_to(CHAT)
        .iter()
        .map(|r| r.timestamp_ms)
        .collect();
    times.sort_unstable();
    times
}

// spec: AI-006
// spec: INTEG-005 [M4]
// Ollama: Standard 1 gleichzeitige Anfrage (Vorgabe im Reiter „Erweitert“, Bereich 1–50; die Anzeige baut U01). Mit
// den Standardwerten ist laut Mock-Zeitstempeln höchstens 1 Anfrage gleichzeitig offen: Drei Einträge mit je 1 s
// Antwortzeit kommen im Abstand von mindestens 1 s an. Gegenprobe mit 3 gleichzeitigen Anfragen.
#[tokio::test]
async fn ollama_defaults_to_one_concurrent_request() {
    assert_eq!(DEFAULT_CONCURRENT_REQUESTS, 1);
    assert!(CONCURRENT_REQUESTS_RANGE.contains(&1));
    assert!(CONCURRENT_REQUESTS_RANGE.contains(&50));
    assert!(!CONCURRENT_REQUESTS_RANGE.contains(&0));
    assert!(!CONCURRENT_REQUESTS_RANGE.contains(&51));
    let settings = AiAppSettings::default();
    let endpoint = &settings.provider_endpoints["ollama"];
    assert_eq!(endpoint.concurrent_requests, DEFAULT_CONCURRENT_REQUESTS);
    assert_eq!(endpoint.timeout_seconds, DEFAULT_TIMEOUT_SECONDS);
    assert_eq!(
        LimitsConfig::default().provider("ollama"),
        ollama::default_limits()
    );
    assert_eq!(ollama::default_limits().concurrent_requests, 1);
    // Andere Anbieter behalten ihren eigenen Standard.
    assert_ne!(
        LimitsConfig::default()
            .provider("openai")
            .concurrent_requests,
        1
    );

    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = default_limiter();
    let requests: Vec<_> = (0..3)
        .map(|i| {
            text_request(
                ollama_model(),
                &format!("MOCK-SCENARIO:delay=1 Eintrag {i}"),
            )
        })
        .collect();
    let (a, b, c) = tokio::join!(
        adapter.run(&limiter, &requests[0], None, 10),
        adapter.run(&limiter, &requests[1], None, 10),
        adapter.run(&limiter, &requests[2], None, 10),
    );
    for execution in [&a, &b, &c] {
        assert!(execution.result.is_ok(), "{:?}", execution.result);
    }
    let times = chat_arrivals(&mock);
    assert_eq!(times.len(), 3);
    for pair in times.windows(2) {
        assert!(pair[1] - pair[0] >= 950, "overlapping requests: {times:?}");
    }

    // Gegenprobe: mit 3 gleichzeitigen Anfragen kommen alle sofort an.
    let mock = Mock::start().await;
    let adapter = OllamaAdapter::new(reqwest::Client::new(), ollama_config(&mock));
    let mut config = LimitsConfig::default();
    config.providers.insert(
        "ollama".into(),
        ProviderLimits {
            concurrent_requests: 3,
            ..ollama::default_limits()
        },
    );
    let limiter = Limiter::with_jitter(
        config,
        Arc::new(VirtualClock::new(Duration::from_secs(300))),
        Arc::new(FixedJitter(0.5)),
    );
    let _ = tokio::join!(
        adapter.run(&limiter, &requests[0], None, 10),
        adapter.run(&limiter, &requests[1], None, 10),
        adapter.run(&limiter, &requests[2], None, 10),
    );
    let times = chat_arrivals(&mock);
    assert_eq!(times.len(), 3);
    assert!(
        times[2] - times[0] < 900,
        "requests were serialized: {times:?}"
    );
}

// spec: AI-006
// Fehlerantworten des Servers (`{"error": …}`) werden nach AI-020 abgebildet; 404 (Modell fehlt) wird nicht
// wiederholt.
#[tokio::test]
async fn server_errors_map_to_http_status_without_retry_for_404() {
    let server = HeaderServer::start(vec![Canned {
        status: 404,
        headers: Vec::new(),
        body: json!({"error": "model \"missing\" not found, try pulling it first"}).to_string(),
    }])
    .await;
    let adapter = OllamaAdapter::new(
        reqwest::Client::new(),
        OllamaConfig::new().with_base_url(server.base_url()),
    );
    let execution = adapter
        .run(
            &test_limiter(1),
            &text_request(ollama_model(), "x"),
            None,
            1,
        )
        .await;
    assert_eq!(execution.attempts, 1);
    let error = execution.result.unwrap_err();
    assert_eq!(
        error.status,
        AiStatus::HttpError {
            code: 404,
            message: "model \"missing\" not found, try pulling it first".into()
        }
    );
    assert_eq!(adapter.status_text(&error), error.status.to_string());
    assert!(!adapter.unreachable().is_tripped());
    assert_eq!(server.requests(), 1);
}

//! Unabhängige Abnahmetests M4-K03: Adapter Ollama (AI-006) und das [M4]-Teilkriterium von INTEG-005 (Ollama:
//! Standard 1 gleichzeitige Anfrage).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-006, AI-020, AI-021, ai-mock-Konventionen) und
//! `docs/spec/04-custom-integrations.md` (INTEG-005, INTEG-160, INTEG-179). Nie ein echter Ollama-Server: ai-mock,
//! eigener Testserver auf `127.0.0.1` oder ein lokaler Port, auf dem nichts lauscht.
// acceptance: independent

mod acceptance_k03_support;

use std::time::Duration;

use acceptance_k03_support::*;
use os_ai::adapters::ollama::{
    self, ConnectionError, OllamaAdapter, OllamaConfig, DEFAULT_BASE_URL,
};
use os_ai::adapters::openai_responses::{ApiKey, OpenAiAccount, OpenAiConfig, OpenAiHttp};
use os_ai::cost::{request_record, LogContext, RequestOutcome};
use os_ai::embeddings::openai::{OpenAiEmbeddings, OpenRouterEmbeddings};
use os_ai::embeddings::{embed_texts, EmbeddingSettings, NoEmbeddingCache};
use os_ai::limits::{LimitsConfig, ProviderLimits};
use os_ai::schema::{run_with_repair, RepairAttempts, ResponseSchema, StructuredOutcome};
use os_ai::AiStatus;
use os_model::budget::MicroUsd;
use os_model::config::{AiAppSettings, ProviderEndpoint};
use serde_json::json;

fn adapter_at(base: &str) -> OllamaAdapter {
    OllamaAdapter::new(client(), OllamaConfig::new().with_base_url(base))
}

fn log_context(task: &str, endpoint: &str) -> LogContext {
    LogContext {
        run_id: 1,
        timestamp: "2026-11-24T12:00:00.000Z".into(),
        task: task.into(),
        provider: "ollama".into(),
        model: "mock-llama".into(),
        endpoint: endpoint.into(),
        url_id: None,
        definition_version: None,
        batch_id: None,
        redactions: 0,
    }
}

/// Führt einen Schema-Eintrag mit lokaler Validierung und einem Reparaturversuch (AI-021) über den Adapter aus.
async fn structured_entry(
    adapter: &OllamaAdapter,
    limiter: &os_ai::limits::Limiter,
    request: &os_ai::provider::Request,
) -> os_ai::schema::StructuredRun<(), os_ai::limits::FinalError> {
    let schema = ResponseSchema::new(request.response_schema.clone().unwrap()).unwrap();
    run_with_repair(
        &schema,
        RepairAttempts::new(1).unwrap(),
        |repair| async move {
            let execution = adapter.run(limiter, request, repair.as_ref(), 50).await;
            execution.result.map(|reply| (reply.raw, ()))
        },
    )
    .await
}

// ---------------------------------------------------------------------------------------------------------------
// AI-006 Kriterium 1: Schema-Aufgaben senden das Schema im Feld `format`; die Antwort wird validiert (AI-021).

// spec: AI-006
// Kriterium: „Schema-Aufgaben senden das Schema im Feld `format`; die Antwort wird validiert (AI-021).“
#[tokio::test]
async fn schema_task_sends_schema_in_format_field_and_gets_validated_answer() {
    let mock = Mock::start().await;
    let adapter = adapter_at(&mock.ollama_base());
    let (limiter, _clock) = default_limiter();
    let request = request(ollama_model("mock-llama"), "Seite A", Some(ok_schema()));

    let run = structured_entry(&adapter, &limiter, &request).await;

    match &run.outcome {
        StructuredOutcome::Valid(value) => assert_eq!(value, &json!({"ok": true})),
        other => panic!("erwartet gültige Antwort, erhalten {other:?}"),
    }
    let bodies = mock.bodies_to("POST", "/ollama/api/chat");
    assert_eq!(bodies.len(), 1, "genau 1 Request an /api/chat");
    assert_eq!(
        bodies[0]["format"],
        ok_schema(),
        "das JSON-Schema steht unverändert im Feld `format`"
    );
    assert_eq!(bodies[0]["model"], "mock-llama");
}

// spec: AI-006
// Kriterium: „… die Antwort wird validiert (AI-021)“ – schemawidrige Antwort dauerhaft → nach 2 Requests
// `Error: Invalid output` (AI-021: genau ein Reparaturversuch).
#[tokio::test]
async fn schema_violation_is_rejected_after_one_repair() {
    let mock = Mock::start().await;
    mock.scenario("ollama", "schema-violation", 10).await;
    let adapter = adapter_at(&mock.ollama_base());
    let (limiter, _clock) = default_limiter();
    let request = request(ollama_model("mock-llama"), "Seite B", Some(ok_schema()));

    let run = structured_entry(&adapter, &limiter, &request).await;

    assert!(
        matches!(run.outcome, StructuredOutcome::InvalidOutput { .. }),
        "schemawidrige Antwort darf nicht übernommen werden: {:?}",
        run.outcome
    );
    assert_eq!(
        run.outcome.status().map(|s| s.to_string()).as_deref(),
        Some("Error: Invalid output")
    );
    let bodies = mock.bodies_to("POST", "/ollama/api/chat");
    assert_eq!(bodies.len(), 2, "Erstversuch + genau 1 Reparatur");
    assert!(
        bodies.iter().all(|b| b["format"] == ok_schema()),
        "auch die Reparatur sendet das Schema in `format`"
    );
}

// spec: AI-006
// Kriterium: Validierung (AI-021) – nicht parsebares JSON einmalig, danach `ok` → gültiges Ergebnis nach 2 Requests.
#[tokio::test]
async fn malformed_json_once_then_ok_is_repaired() {
    let mock = Mock::start().await;
    mock.scenario("ollama", "malformed-json", 1).await;
    let adapter = adapter_at(&mock.ollama_base());
    let (limiter, _clock) = default_limiter();
    let request = request(ollama_model("mock-llama"), "Seite C", Some(ok_schema()));

    let run = structured_entry(&adapter, &limiter, &request).await;

    assert!(
        matches!(&run.outcome, StructuredOutcome::Valid(v) if v == &json!({"ok": true})),
        "{:?}",
        run.outcome
    );
    assert_eq!(mock.bodies_to("POST", "/ollama/api/chat").len(), 2);
}

// ---------------------------------------------------------------------------------------------------------------
// AI-006 Kriterium 2: Kostenprotokoll zeigt `Cost = 0` und die Tokenzahlen aus `prompt_eval_count`/`eval_count`.

// spec: AI-006
// Kriterium: „Kostenprotokoll zeigt `Cost = 0` und die Tokenzahlen aus `prompt_eval_count`/`eval_count`.“
#[tokio::test]
async fn chat_log_line_has_zero_cost_and_eval_counts() {
    let stub = Stub::start(|_, _| {
        StubResponse::json(
            200,
            json!({
                "model": "mock-llama",
                "message": {"role": "assistant", "content": "{\"ok\": true}"},
                "done": true,
                "done_reason": "stop",
                "prompt_eval_count": 123,
                "eval_count": 45
            }),
        )
    })
    .await;
    let adapter = adapter_at(&stub.base());
    let (limiter, _clock) = default_limiter();
    let request = request(ollama_model("mock-llama"), "Seite D", Some(ok_schema()));

    let execution = adapter.run(&limiter, &request, None, 50).await;
    let reply = execution.result.expect("Antwort");
    assert_eq!(stub.requests()[0].path, "/api/chat");
    assert_eq!(reply.usage.input_tokens, Some(123), "prompt_eval_count");
    assert_eq!(reply.usage.output_tokens, Some(45), "eval_count");

    let outcome = RequestOutcome::success(reply.usage.clone(), 0, reply.duration_ms);
    let record = request_record(
        &log_context("decision", ollama::ENDPOINT),
        &ollama::rates(),
        MicroUsd(0),
        &outcome,
    );
    assert_eq!(record.cost, MicroUsd(0), "Kosten 0");
    assert_eq!(record.token_cost, MicroUsd(0));
    assert_eq!(record.input_tokens, Some(123));
    assert_eq!(record.output_tokens, Some(45));
    assert!(!record.over_reservation);
}

// spec: AI-006
// Kriterium: Tokenzahlen aus `prompt_eval_count`/`eval_count` – gegen den ai-mock (Token-Zählung der Konventionen).
#[tokio::test]
async fn chat_usage_matches_mock_eval_counts() {
    let mock = Mock::start().await;
    let adapter = adapter_at(&mock.ollama_base());
    let (limiter, _clock) = default_limiter();
    let request = request(
        ollama_model("mock-llama"),
        "Ein Text mit Länge",
        Some(ok_schema()),
    );

    let reply = adapter
        .run(&limiter, &request, None, 50)
        .await
        .result
        .expect("Antwort");
    let body = &mock.bodies_to("POST", "/ollama/api/chat")[0];
    let expected_in = u64::try_from(os_testsite::mocks::ai::token_count(body)).unwrap();
    assert_eq!(reply.usage.input_tokens, Some(expected_in));
    // Antwort des Mocks: `{"ok":true}` → ⌈11 / 4⌉ = 3 Ausgabe-Tokens.
    assert_eq!(reply.usage.output_tokens, Some(3));
}

// spec: AI-006
// Kriterium: „Kostenprotokoll zeigt `Cost = 0` …“ auch für Ollama-Embeddings (`/api/embed`, Eingabe-Tokens aus
// `prompt_eval_count`).
#[tokio::test]
async fn embedding_log_line_has_zero_cost_and_prompt_eval_count() {
    let stub = Stub::start(|req, _| {
        let n = req.json()["input"].as_array().map_or(0, Vec::len);
        StubResponse::json(
            200,
            json!({
                "model": "mock-embed",
                "embeddings": vec![vec![1.0, 2.0, 2.0]; n],
                "prompt_eval_count": 77
            }),
        )
    })
    .await;
    let adapter = adapter_at(&stub.base());
    let (limiter, _clock) = default_limiter();
    let texts: Vec<String> = (0..3).map(|i| format!("Text {i}")).collect();
    let run = embed_texts(
        &adapter,
        &limiter,
        &NoEmbeddingCache,
        &EmbeddingSettings::new("mock-embed"),
        &texts,
    )
    .await
    .unwrap();
    assert_eq!(stub.requests()[0].path, "/api/embed");
    assert_eq!(run.requests.len(), 1);
    let record = request_record(
        &log_context("embedding", ollama::EMBEDDINGS_ENDPOINT),
        &ollama::rates(),
        MicroUsd(0),
        &run.requests[0].outcome(),
    );
    assert_eq!(record.cost, MicroUsd(0));
    assert_eq!(record.input_tokens, Some(77));
}

// ---------------------------------------------------------------------------------------------------------------
// AI-006 Kriterium 3: Nicht erreichbarer Server → alle offenen Ollama-Einträge erhalten
// `Error: <Meldung aus INTEG-160>` (inkl. URL); andere Anbieter laufen weiter. Festlegung Orchestrator: Statuswert
// `Error: Connection failed` (AI-020), Meldung mit Server-URL zusätzlich als Text abrufbar.

// spec: AI-006
// spec: AI-020
// Kriterium: „Nicht erreichbarer Server → alle offenen Ollama-Einträge erhalten `Error: …` (… inkl. URL)“; nach dem
// ersten endgültigen Fehler gehen für die übrigen Einträge keine Anfragen mehr an den Server.
#[tokio::test]
async fn unreachable_server_fails_all_open_entries_without_new_requests() {
    let addr = closed_port();
    let base = format!("http://{addr}");
    let adapter = adapter_at(&base);
    let (limiter, clock) = default_limiter();
    let request = request(ollama_model("mock-llama"), "Seite E", None);

    // Erster Eintrag: Verbindung abgelehnt → nach 1 + 5 Versuchen endgültig (AI-020, INTEG-005).
    let first = adapter.run(&limiter, &request, None, 50).await;
    let first_error = first.result.clone().expect_err("Server nicht erreichbar");
    assert_eq!(first_error.status, AiStatus::ConnectionFailed);
    assert_eq!(first_error.status.to_string(), "Error: Connection failed");
    assert_eq!(first_error.http_status, None, "HTTP Status bleibt leer");
    assert_eq!(first_error.error_code.as_deref(), Some("transport/connect"));
    assert_eq!(first.attempts, 6, "1 + 5 Versuche");
    assert_eq!(first.retries(), 5);
    assert_eq!(clock.sleeps().len(), 5, "5 Backoff-Wartezeiten");

    // Meldung nach INTEG-160 mit Server-URL; Statusspalte zeigt denselben Text wie der Verbindungsstatus.
    let connect_message = match adapter.connect().await {
        Err(ConnectionError::Unreachable { message }) => message,
        other => panic!("Verbinden mit nicht erreichbarem Server: {other:?}"),
    };
    assert!(connect_message.contains(&base), "{connect_message}");
    assert!(
        first_error.message.contains(&base),
        "{}",
        first_error.message
    );
    let first_text = adapter.status_text(&first_error);
    assert!(
        first_text.contains(&connect_message),
        "Statustext `{first_text}` enthält den Verbindungsstatus `{connect_message}`"
    );

    // Jetzt lauscht auf dem Port ein Testserver: Jede weitere Anfrage dieses Laufs würde dort gezählt.
    let stub = Stub::start_on(addr, |_, _| {
        StubResponse::json(
            200,
            json!({"message": {"role": "assistant", "content": "x"}, "done": true}),
        )
    })
    .await;
    for _ in 0..4 {
        let next = adapter.run(&limiter, &request, None, 50).await;
        let error = next
            .result
            .expect_err("offener Eintrag scheitert ebenfalls");
        assert_eq!(error.status, AiStatus::ConnectionFailed);
        assert!(error.message.contains(&base));
        assert_eq!(adapter.status_text(&error), first_text);
    }
    assert_eq!(
        stub.requests().len(),
        0,
        "nach dem ersten endgültigen Fehler keine weiteren Anfragen an den Server"
    );
    assert_eq!(clock.sleeps().len(), 5, "keine weiteren Wiederholungen");
}

// spec: AI-006
// Kriterium: wie oben, mit gleichzeitig wartenden Einträgen (Standard 1 gleichzeitige Anfrage): Nur der erste
// Eintrag versucht es (1 + 5 Versuche); alle scheitern mit der Meldung inkl. URL.
#[tokio::test]
async fn unreachable_server_with_queued_entries_retries_only_once() {
    let base = format!("http://{}", closed_port());
    let adapter = adapter_at(&base);
    let (limiter, clock) = default_limiter();
    let request = request(ollama_model("mock-llama"), "Seite F", None);

    let (a, b, c, d) = tokio::join!(
        adapter.run(&limiter, &request, None, 50),
        adapter.run(&limiter, &request, None, 50),
        adapter.run(&limiter, &request, None, 50),
        adapter.run(&limiter, &request, None, 50),
    );
    let mut attempts = Vec::new();
    for execution in [a, b, c, d] {
        attempts.push(execution.attempts);
        let error = execution.result.expect_err("scheitert");
        assert_eq!(error.status, AiStatus::ConnectionFailed);
        assert!(error.message.contains(&base), "{}", error.message);
    }
    assert_eq!(
        clock.sleeps().len(),
        5,
        "nur ein Eintrag durchläuft die Wiederholungen (keine Anfragen je Eintrag); Versuche je Eintrag: {attempts:?}"
    );
}

// spec: AI-006
// spec: AI-020
// Kriterium: nicht erreichbarer Server bei Embeddings – alle Texte scheitern, nur ein Eintrag mit Wiederholungen;
// Protokollzeile `HTTP Status` leer, `Error Code = transport/connect`, `Retries = 5`, `Cost = 0`.
#[tokio::test]
async fn unreachable_server_fails_all_embedding_texts() {
    let addr = closed_port();
    let base = format!("http://{addr}");
    let adapter = adapter_at(&base);
    let (limiter, clock) = default_limiter();
    let texts: Vec<String> = (0..130).map(|i| format!("Text {i}")).collect();

    let run = embed_texts(
        &adapter,
        &limiter,
        &NoEmbeddingCache,
        &EmbeddingSettings::new("mock-embed"),
        &texts,
    )
    .await
    .unwrap();

    assert_eq!(run.results.len(), 130);
    for result in &run.results {
        assert_eq!(result.status, AiStatus::ConnectionFailed);
        assert!(result.vector.is_none());
        let error = result.error.as_ref().expect("Fehler");
        assert!(error.message.contains(&base), "{}", error.message);
    }
    assert_eq!(clock.sleeps().len(), 5, "nur ein Bündel mit Wiederholungen");
    assert_eq!(
        run.requests.len(),
        1,
        "eine Protokollzeile für den gesendeten Eintrag"
    );
    let record = request_record(
        &log_context("embedding", ollama::EMBEDDINGS_ENDPOINT),
        &ollama::rates(),
        MicroUsd(0),
        &run.requests[0].outcome(),
    );
    assert_eq!(record.http_status, None);
    assert_eq!(record.error_code.as_deref(), Some("transport/connect"));
    assert_eq!(record.retries, 5);
    assert_eq!(record.cost, MicroUsd(0));
    assert_eq!(record.status, "Error: Connection failed");

    // Weitere Embedding-Einträge desselben Laufs gehen nicht mehr an den Server.
    let stub = Stub::start_on(addr, |_, _| {
        StubResponse::json(200, json!({"embeddings": [[1.0, 0.0]]}))
    })
    .await;
    let again = embed_texts(
        &adapter,
        &limiter,
        &NoEmbeddingCache,
        &EmbeddingSettings::new("mock-embed"),
        &["Noch ein Text".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(again.results[0].status, AiStatus::ConnectionFailed);
    assert_eq!(stub.requests().len(), 0);
}

// spec: AI-006
// Kriterium: „… andere Anbieter laufen weiter.“ – nach dem Ausfall von Ollama liefern OpenAI und OpenRouter über
// dieselbe Begrenzung weiter Ergebnisse.
#[tokio::test]
async fn other_providers_keep_running_when_ollama_is_unreachable() {
    let mock = Mock::start().await;
    let adapter = adapter_at(&format!("http://{}", closed_port()));
    let (limiter, _clock) = default_limiter();
    let request = request(ollama_model("mock-llama"), "Seite G", None);
    for _ in 0..12 {
        // Mehr als die Schwelle des Schutzschalters (10) – betrifft nur Ollama.
        let result = adapter.run(&limiter, &request, None, 50).await.result;
        assert_eq!(result.unwrap_err().status, AiStatus::ConnectionFailed);
    }

    let openai = OpenAiEmbeddings::new(OpenAiHttp::new(
        client(),
        OpenAiConfig::new(ApiKey::new(TEST_KEY)).with_base_url(mock.openai_base()),
        OpenAiAccount::new(),
    ));
    let openrouter = OpenRouterEmbeddings::new(
        client(),
        os_ai::adapters::openrouter::OpenRouterConfig {
            base_url: mock.openrouter_base(),
            ..os_ai::adapters::openrouter::OpenRouterConfig::new(
                os_ai::adapters::openrouter::ApiKey::new(TEST_KEY),
            )
        },
    );
    let texts = vec![
        "Anderer Anbieter 1".to_string(),
        "Anderer Anbieter 2".to_string(),
    ];
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(8);
    let a = embed_texts(&openai, &limiter, &NoEmbeddingCache, &settings, &texts)
        .await
        .unwrap();
    let b = embed_texts(&openrouter, &limiter, &NoEmbeddingCache, &settings, &texts)
        .await
        .unwrap();
    for run in [a, b] {
        assert!(
            run.results.iter().all(|r| r.status == AiStatus::Success),
            "{:?}",
            run.results.iter().map(|r| &r.status).collect::<Vec<_>>()
        );
    }
    assert_eq!(mock.requests_to("POST", "/openai/v1/embeddings").len(), 1);
    assert_eq!(
        mock.requests_to("POST", "/openrouter/api/v1/embeddings")
            .len(),
        1
    );
}

// ---------------------------------------------------------------------------------------------------------------
// AI-006 Kriterium 4: Eine in 04 geänderte Server-URL gilt ohne weitere Eingabe auch für 08 (eine gemeinsame
// Einstellung). Festlegung Orchestrator: die Server-URL kommt aus einer gemeinsamen Einstellung, die der Aufrufer
// übergibt.

fn settings_with_ollama_url(url: Option<&str>) -> AiAppSettings {
    let mut settings = AiAppSettings::default();
    let endpoint = settings
        .provider_endpoints
        .entry("ollama".into())
        .or_insert_with(ProviderEndpoint::default);
    endpoint.base_url = url.map(str::to_string);
    settings
}

// spec: AI-006
// Kriterium: „Eine in 04 geänderte Server-URL gilt ohne weitere Eingabe auch für 08 (eine gemeinsame Einstellung).“
#[tokio::test]
async fn changed_shared_server_url_is_used_without_further_input() {
    let first = Mock::start().await;
    let second =
        Stub::start(|_, _| StubResponse::json(200, json!({"models": [{"name": "other-model"}]})))
            .await;

    let mut settings = settings_with_ollama_url(Some(&first.ollama_base()));
    let adapter = OllamaAdapter::new(client(), OllamaConfig::from_app_settings(&settings));
    let models = adapter.connect().await.expect("Modellliste");
    assert!(models.iter().any(|m| m.name == "mock-llama"));
    assert_eq!(first.requests_to("GET", "/ollama/api/tags").len(), 1);

    // Änderung der gemeinsamen Einstellung (Konto-Reiter aus 04) – keine weitere Eingabe für 08.
    settings
        .provider_endpoints
        .get_mut("ollama")
        .unwrap()
        .base_url = Some(second.base());
    let adapter = OllamaAdapter::new(client(), OllamaConfig::from_app_settings(&settings));
    let (limiter, _clock) = limiter_with(LimitsConfig::from_app_settings(&settings));
    let _ = adapter
        .run(
            &limiter,
            &request(ollama_model("x"), "Seite H", None),
            None,
            50,
        )
        .await;
    let models = adapter.connect().await.expect("Modellliste");
    assert_eq!(models[0].name, "other-model");
    let paths: Vec<String> = second.requests().into_iter().map(|r| r.path).collect();
    assert_eq!(
        paths,
        vec!["/api/chat".to_string(), "/api/tags".to_string()]
    );
    assert_eq!(
        first.requests_to("GET", "/ollama/api/tags").len(),
        1,
        "die alte URL erhält nichts mehr"
    );
}

// spec: AI-006
// Kriterium: gemeinsame Einstellung ohne Angabe → Standard-Server-URL aus INTEG-160 (`http://localhost:11434`).
#[test]
fn shared_setting_without_url_falls_back_to_integ_160_default() {
    assert_eq!(DEFAULT_BASE_URL, "http://localhost:11434");
    assert_eq!(
        OllamaConfig::from_app_settings(&settings_with_ollama_url(None)).base_url,
        "http://localhost:11434"
    );
    assert_eq!(
        OllamaConfig::from_app_settings(&AiAppSettings::default()).base_url,
        "http://localhost:11434"
    );
    let custom =
        OllamaConfig::from_app_settings(&settings_with_ollama_url(Some("http://192.0.2.10:11434")));
    assert_eq!(custom.base_url, "http://192.0.2.10:11434");
}

// ---------------------------------------------------------------------------------------------------------------
// AI-006 Kriterium 5 und INTEG-005 [M4]: Standard höchstens 1 offene Anfrage.

async fn slow_ollama_stub() -> Stub {
    Stub::start(|_, _| {
        StubResponse::json(
            200,
            json!({
                "model": "mock-llama",
                "message": {"role": "assistant", "content": "{\"ok\":true}"},
                "done": true,
                "prompt_eval_count": 1,
                "eval_count": 1
            }),
        )
        .delayed(Duration::from_millis(150))
    })
    .await
}

// spec: AI-006
// spec: INTEG-005 [M4]
// Kriterium AI-006: „Standardmäßig ist höchstens 1 Anfrage gleichzeitig offen (Mock-Zeitstempel).“
#[tokio::test]
async fn by_default_at_most_one_ollama_request_is_open() {
    let stub = slow_ollama_stub().await;
    let adapter = adapter_at(&stub.base());
    let (limiter, _clock) = default_limiter();
    let request = request(ollama_model("mock-llama"), "Seite I", Some(ok_schema()));

    let (a, b, c, d) = tokio::join!(
        adapter.run(&limiter, &request, None, 50),
        adapter.run(&limiter, &request, None, 50),
        adapter.run(&limiter, &request, None, 50),
        adapter.run(&limiter, &request, None, 50),
    );
    for execution in [a, b, c, d] {
        execution.result.expect("Antwort");
    }
    assert_eq!(stub.requests().len(), 4);
    assert_eq!(stub.max_open(), 1, "höchstens 1 offene Anfrage");
    let mut intervals = stub.intervals();
    intervals.sort_by_key(|i| i.start);
    for pair in intervals.windows(2) {
        assert!(
            pair[1].start >= pair[0].end,
            "Zeitstempel überlappen: {pair:?}"
        );
    }
}

// spec: INTEG-005 [M4]
// Kriterium (Gegenprobe, INTEG-005): Bei Einstellung „gleichzeitige Anfragen = 2" hat der Server zu keinem Zeitpunkt
// mehr als 2 offene Ollama-Anfragen (und die Messung erkennt Gleichzeitigkeit).
#[tokio::test]
async fn ollama_concurrency_setting_two_allows_two_open_requests() {
    let stub = slow_ollama_stub().await;
    let adapter = adapter_at(&stub.base());
    let mut settings = AiAppSettings::default();
    settings
        .provider_endpoints
        .get_mut("ollama")
        .expect("Ollama-Eintrag in den Standard-Einstellungen")
        .concurrent_requests = 2;
    let (limiter, _clock) = limiter_with(LimitsConfig::from_app_settings(&settings));
    let request = request(ollama_model("mock-llama"), "Seite J", Some(ok_schema()));

    let (a, b, c, d) = tokio::join!(
        adapter.run(&limiter, &request, None, 50),
        adapter.run(&limiter, &request, None, 50),
        adapter.run(&limiter, &request, None, 50),
        adapter.run(&limiter, &request, None, 50),
    );
    for execution in [a, b, c, d] {
        execution.result.expect("Antwort");
    }
    assert_eq!(stub.max_open(), 2);
}

// spec: INTEG-005 [M4]
// spec: AI-006
// Kriterium INTEG-005: „[M4] Ollama: Standard 1“ (Konfigurationswert), dazu Timeout-Standard 300 s (INTEG-179).
#[test]
fn ollama_default_concurrency_is_one() {
    let settings = AiAppSettings::default();
    let endpoint = settings
        .provider_endpoints
        .get("ollama")
        .expect("Ollama-Eintrag in den Standard-Einstellungen");
    assert_eq!(endpoint.concurrent_requests, 1);
    assert_eq!(endpoint.timeout_seconds, 300);
    let limits: ProviderLimits = LimitsConfig::default().provider("ollama");
    assert_eq!(limits.concurrent_requests, 1);
    assert_eq!(limits.timeout, Duration::from_secs(300));
    assert_eq!(ollama::DEFAULT_CONCURRENT_REQUESTS, 1);
    assert_eq!(ollama::default_limits().concurrent_requests, 1);
}

// spec: AI-006
// Kriterium (Beschreibung, INTEG-160): Modellliste über `/api/tags`; Ollama als lokaler Anbieter mit Fähigkeiten
// Text, Structured Output, Embeddings, Local (AI-001).
#[tokio::test]
async fn model_list_comes_from_api_tags() {
    let mock = Mock::start().await;
    let adapter = adapter_at(&mock.ollama_base());
    let models = adapter.connect().await.expect("Modellliste");
    let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["mock-llama"]);
    assert_eq!(mock.requests_to("GET", "/ollama/api/tags").len(), 1);
    let model = adapter.to_model("mock-llama");
    assert_eq!(model.provider, "ollama");
    let caps = model.capabilities;
    assert!(caps.text && caps.structured_output && caps.embeddings && caps.local);
}

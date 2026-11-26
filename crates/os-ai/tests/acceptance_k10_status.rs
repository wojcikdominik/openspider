//! Unabhängige Abnahmetests M4-K10: neuer Status `Error: Output limit reached` (AI-020, Nachtrag 24.11.2026, Q00-H1)
//! je Anbieter, ohne Reparaturversuch und ohne Teilergebnis (AI-021, Nachtrag), nicht im Schutzschalter (AI-017),
//! `Error Code` je Anbieter und `HTTP Status = 200` (AI-002, AI-003, AI-005, AI-006, AI-014, Nachträge), verbuchte
//! Tokens. Gegenprobe: unlesbare bzw. schemawidrige Antwort bleibt `Error: Invalid output` mit Reparaturversuch.
//!
//! Den ganzen Lauf über die Aufgabenwarteschlange (Fixture-Site, `Skipped (scope)`) baut M4-D01; hier wird die
//! Zusammensetzung aus Adapter, `limits::Limiter`, `schema::run_with_repair` und `cost::log` gegen den ai-mock geprüft.
// acceptance: independent

mod acceptance_k10_support;

use acceptance_k10_support::*;
use os_ai::adapters::anthropic::{AdapterReply, AnthropicAdapter, OutputLimitReached};
use os_ai::adapters::ollama::OllamaAdapter;
use os_ai::adapters::openai_responses::RequestFailure;
use os_ai::adapters::openrouter::OpenRouterAdapter;
use os_ai::cost::{
    detailed_request_record, OutcomeDetails, PriceBasis, PriceSource, Rates, RequestOutcome,
};
use os_ai::limits::{AttemptFailure, BreakerEffect, FinalError, Limiter, RequestSpec};
use os_ai::provider::{AiProvider, ProviderError, ReasoningLevel, Request};
use os_ai::schema::{
    run_with_repair, RepairAttempts, RepairRequest, ResponseSchema, StructuredOutcome,
    StructuredRun,
};
use os_ai::AiStatus;
use os_model::budget::MicroUsd;
use serde_json::Value;

const OUTPUT_LIMIT: &str = "Error: Output limit reached";

/// Anbieter mit gemeinsamer Antwortform (`AdapterReply`).
enum Cloud<'a> {
    Anthropic(&'a AnthropicAdapter),
    OpenRouter(&'a OpenRouterAdapter),
    Ollama(&'a OllamaAdapter),
}

impl Cloud<'_> {
    fn id(&self) -> &'static str {
        match self {
            Self::Anthropic(_) => "anthropic",
            Self::OpenRouter(_) => "openrouter",
            Self::Ollama(_) => "ollama",
        }
    }

    async fn send(
        &self,
        request: &Request,
        repair: Option<&RepairRequest>,
    ) -> Result<AdapterReply, AttemptFailure> {
        match self {
            Self::Anthropic(a) => a.send(request, repair).await,
            Self::OpenRouter(a) => a.send(request, repair).await,
            Self::Ollama(a) => a.send(request, repair).await,
        }
    }

    /// Ein Versuch für den Limiter: ein Abbruch an der Ausgabegrenze ist ein nicht wiederholbarer Fehler des Versuchs.
    async fn attempt(
        &self,
        request: &Request,
        repair: Option<&RepairRequest>,
    ) -> Result<AdapterReply, AttemptFailure> {
        let reply = self.send(request, repair).await?;
        match reply.clone().for_repair() {
            Ok(_) => Ok(reply),
            Err(limit) => Err(limit.output_limit_failure()),
        }
    }

    /// Ein Eintrag über den Limiter (INTEG-005, Schutzschalter), ohne Schema.
    async fn entry(
        &self,
        limiter: &Limiter,
        request: &Request,
    ) -> (Result<AdapterReply, FinalError>, u32) {
        let spec = RequestSpec::new(self.id(), request.model.id.clone(), 10);
        let exec = limiter.run(&spec, |_| self.attempt(request, None)).await;
        (exec.result, exec.attempts)
    }

    /// Ein Schema-Eintrag mit lokaler Validierung und einem Reparaturversuch (AI-021), jeder Request über den Limiter.
    async fn structured(
        &self,
        limiter: &Limiter,
        request: &Request,
    ) -> StructuredRun<AdapterReply, FinalError> {
        let schema = ResponseSchema::new(request.response_schema.clone().expect("Schema")).unwrap();
        run_with_repair(&schema, RepairAttempts::default(), |repair| async move {
            let spec = RequestSpec::new(self.id(), request.model.id.clone(), 10);
            let exec = limiter
                .run(&spec, |_| self.attempt(request, repair.as_ref()))
                .await;
            exec.result.map(|reply| (reply.raw.clone(), reply))
        })
        .await
    }

    /// Abbruch an der Ausgabegrenze eines einzelnen Requests (für Protokollzeilen).
    async fn limit(&self, request: &Request) -> OutputLimitReached {
        *self
            .send(request, None)
            .await
            .expect("HTTP 200")
            .for_repair()
            .expect_err("Abbruch an der Ausgabegrenze")
    }
}

fn row_for(
    limit: &OutputLimitReached,
    task: &str,
    provider: &str,
    model: &str,
    endpoint: &str,
    rates: &Rates,
    level: ReasoningLevel,
) -> os_ai::cost::LoggedRow {
    detailed_request_record(
        &ctx(task, provider, model, endpoint),
        rates,
        MicroUsd(1_000_000),
        &RequestOutcome::output_limit(limit, 0),
        &OutcomeDetails {
            reasoning: Some(level),
            reported_cost: limit.reported_cost,
        },
    )
}

/// Ein normaler Request vorab, damit ein danach gesetztes globales Szenario nicht von einem Modelllisten-Abruf des
/// Adapters verbraucht wird.
async fn warm_up(cloud: &Cloud<'_>, model_id: &str) {
    let r = request(
        model(cloud.id(), model_id, "messages", Vec::new()),
        "K10 warm-up",
        None,
        ReasoningLevel::Off,
        Some(256),
    );
    cloud.send(&r, None).await.expect("Aufwärmen");
}

fn assert_output_limit_final(error: &FinalError, code: &str, what: &str) {
    assert_eq!(error.status, AiStatus::OutputLimitReached, "{what}");
    assert_eq!(error.status.to_string(), OUTPUT_LIMIT, "{what}");
    assert_eq!(error.http_status, Some(200), "HTTP Status {what}");
    assert_eq!(error.error_code.as_deref(), Some(code), "Error Code {what}");
}

// spec: AI-002 [M4]
// spec: AI-020 [M4]
// spec: AI-021 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Kriterium (AI-002): Szenario `output-limit` bei einer Schema-Aufgabe → `Error: Output limit reached`,
// `Error Code = max_tokens`, `HTTP Status = 200`, genau 1 Request (kein Reparaturversuch), `Output Tokens` = gesendetes
// `max_tokens`, `Cost` > 0, Ergebnisspalten leer; dasselbe bei einer Freitext-Aufgabe.
#[tokio::test]
async fn anthropic_output_limit_with_and_without_schema() {
    let mock = Mock::start().await;
    let a = anthropic_model(&mock, "A").await;
    let adapter = mock.anthropic();
    let cloud = Cloud::Anthropic(&adapter);
    let limiter = fast_limiter();

    for (marker, schema) in [
        (
            "K10 ant schema MOCK-SCENARIO:output-limit",
            Some(ok_schema()),
        ),
        ("K10 ant free MOCK-SCENARIO:output-limit", None),
    ] {
        let r = request(
            model("anthropic", &a.id, "messages", Vec::new()),
            marker,
            schema.clone(),
            ReasoningLevel::Off,
            Some(300),
        );
        if schema.is_some() {
            let run = cloud.structured(&limiter, &r).await;
            let StructuredOutcome::Failed(ref error) = run.outcome else {
                panic!(
                    "erwartet Abbruch ohne Reparatur, kein Ergebnis: {:?}",
                    run.outcome
                );
            };
            assert_output_limit_final(error, "max_tokens", marker);
            assert_eq!(run.outcome.final_status(), AiStatus::OutputLimitReached);
        } else {
            let (result, attempts) = cloud.entry(&limiter, &r).await;
            assert_eq!(attempts, 1, "keine automatische Wiederholung");
            assert_output_limit_final(&result.expect_err("kein Ergebnis"), "max_tokens", marker);
        }
        assert_eq!(
            mock.bodies_with(MESSAGES, marker).len(),
            1,
            "genau 1 Request für den Eintrag: {marker}"
        );

        // Protokollzeile: gemeldete Tokens verbucht, Kosten > 0.
        let limit = cloud.limit(&r).await;
        assert_eq!(
            limit.usage.output_tokens,
            Some(300),
            "Output Tokens = max_tokens"
        );
        let row = row_for(
            &limit,
            "suggest-title",
            "anthropic",
            &a.id,
            "messages",
            &claude_rates(),
            ReasoningLevel::Off,
        );
        assert_eq!(row.record.status, OUTPUT_LIMIT);
        assert_eq!(row.record.http_status, Some(200));
        assert_eq!(row.record.error_code.as_deref(), Some("max_tokens"));
        assert_eq!(row.record.output_tokens, Some(300));
        assert!(row.record.cost.0 > 0, "Cost > 0: {:?}", row.record);
        assert!(
            row.record.cost.0 >= 300 * 5,
            "Ausgabe zu 5 USD je 1 Mio. verbucht"
        );
    }

    // Gemeinsamer Weg des Adapters (AI-001): Fehlerklasse mit dem Status, kein Teilergebnis.
    let r = request(
        model("anthropic", &a.id, "messages", Vec::new()),
        "K10 ant execute MOCK-SCENARIO:output-limit",
        Some(ok_schema()),
        ReasoningLevel::Off,
        Some(200),
    );
    let error = adapter.execute(r).await.expect_err("kein Ergebnis");
    assert_eq!(error.status(), Some(AiStatus::OutputLimitReached));
}

// spec: AI-003 [M4]
// spec: AI-020 [M4]
// spec: AI-021 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Kriterium (AI-003): Szenario `output-limit` → `Error: Output limit reached`, `Error Code = max_output_tokens`, genau
// 1 Request für den Eintrag; dazu `HTTP Status = 200`, `Output Tokens` = gesendete Grenze, `Cost` > 0, mit und ohne
// Schema, jeder Request mit `"store": false`.
#[tokio::test]
async fn responses_output_limit_with_and_without_schema() {
    let mock = Mock::start().await;
    let adapter = mock.responses();
    let limiter = fast_limiter();
    let rates = fixture_rates("openai", "gpt-6-luna", "responses");

    let marker = "K10 resp schema MOCK-SCENARIO:output-limit";
    let r = request(
        model("openai", "gpt-6-luna", "responses", luna_levels()),
        marker,
        Some(ok_schema()),
        ReasoningLevel::Off,
        Some(300),
    );
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let run = adapter
        .run_structured(&limiter, &r, &schema, RepairAttempts::default(), 10)
        .await;
    let StructuredOutcome::Failed(RequestFailure {
        ref error,
        ref meta,
    }) = run.outcome
    else {
        panic!("erwartet Abbruch ohne Reparatur: {:?}", run.outcome);
    };
    assert_output_limit_final(error, "max_output_tokens", marker);
    assert_eq!(meta.attempts, 1);
    assert_eq!(meta.http_status, Some(200));
    assert_eq!(meta.error_code.as_deref(), Some("max_output_tokens"));
    let usage = meta.usage.clone().expect("gemeldete Tokens");
    assert_eq!(
        usage.output_tokens,
        Some(300),
        "Output Tokens = max_output_tokens"
    );
    let row = detailed_request_record(
        &ctx("suggest-title", "openai", "gpt-6-luna", "responses"),
        &rates,
        MicroUsd(1_000_000),
        &meta.request_outcome(error.status.clone(), 10, 0, 0),
        &OutcomeDetails {
            reasoning: Some(ReasoningLevel::Off),
            reported_cost: None,
        },
    );
    assert_eq!(row.record.status, OUTPUT_LIMIT);
    assert_eq!(row.record.http_status, Some(200));
    assert_eq!(row.record.error_code.as_deref(), Some("max_output_tokens"));
    assert_eq!(row.record.output_tokens, Some(300));
    assert_eq!(row.record.price_source, PriceSource::Table);
    assert!(row.record.cost.0 > 0, "Cost > 0: {:?}", row.record);
    assert_eq!(
        mock.bodies_with(RESPONSES, marker).len(),
        1,
        "kein Reparaturversuch"
    );

    // Freitext.
    let marker = "K10 resp free MOCK-SCENARIO:output-limit";
    let r = request(
        model("openai", "gpt-6-luna", "responses", luna_levels()),
        marker,
        None,
        ReasoningLevel::Off,
        Some(64),
    );
    let (result, meta) = adapter.run(&limiter, &r, 10).await;
    let reply = result.expect("HTTP 200");
    assert_eq!(reply.output_limit(), Some("max_output_tokens"));
    assert_eq!(meta.attempts, 1);
    assert_eq!(meta.http_status, Some(200));
    assert_eq!(meta.error_code.as_deref(), Some("max_output_tokens"));
    assert_eq!(meta.usage.as_ref().and_then(|u| u.output_tokens), Some(64));
    assert_eq!(mock.bodies_with(RESPONSES, marker).len(), 1);
    let error = adapter
        .execute(request(
            model("openai", "gpt-6-luna", "responses", luna_levels()),
            "K10 resp execute MOCK-SCENARIO:output-limit",
            None,
            ReasoningLevel::Off,
            Some(64),
        ))
        .await
        .expect_err("kein Teilergebnis");
    assert_eq!(error.status(), Some(AiStatus::OutputLimitReached));

    for body in mock.bodies(RESPONSES) {
        assert_eq!(body["store"], Value::Bool(false), "{body}");
    }
}

// spec: AI-005 [M4]
// spec: AI-020 [M4]
// spec: AI-021 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Kriterium (AI-005): Szenario `output-limit` → `Error: Output limit reached`, `Error Code = length`, genau 1 Request
// für den Eintrag; `HTTP Status = 200`, `Output Tokens` = gesendete Grenze, `Cost` > 0, mit und ohne Schema.
#[tokio::test]
async fn openrouter_output_limit_with_and_without_schema() {
    let mock = Mock::start().await;
    let roles = openrouter_roles(&mock).await;
    let adapter = mock.openrouter();
    let cloud = Cloud::OpenRouter(&adapter);
    let limiter = fast_limiter();
    let m = &roles.r3;

    for (marker, schema) in [
        (
            "K10 or schema MOCK-SCENARIO:output-limit",
            Some(ok_schema()),
        ),
        ("K10 or free MOCK-SCENARIO:output-limit", None),
    ] {
        let r = request(
            model("openrouter", &m.id, "chat-completions", Vec::new()),
            marker,
            schema.clone(),
            ReasoningLevel::ModelDefault,
            Some(200),
        );
        if schema.is_some() {
            let run = cloud.structured(&limiter, &r).await;
            let StructuredOutcome::Failed(ref error) = run.outcome else {
                panic!("erwartet Abbruch ohne Reparatur: {:?}", run.outcome);
            };
            assert_output_limit_final(error, "length", marker);
        } else {
            let (result, attempts) = cloud.entry(&limiter, &r).await;
            assert_eq!(attempts, 1);
            assert_output_limit_final(&result.expect_err("kein Ergebnis"), "length", marker);
        }
        assert_eq!(mock.bodies_with(COMPLETIONS, marker).len(), 1, "{marker}");

        let limit = cloud.limit(&r).await;
        assert_eq!(limit.usage.output_tokens, Some(200));
        let rates = PriceBasis::OpenRouter(m)
            .actual_rates(limit.usage.input_tokens.unwrap_or(0))
            .expect("Preis bekannt");
        let row = row_for(
            &limit,
            "suggest-title",
            "openrouter",
            &m.id,
            "chat-completions",
            &rates,
            ReasoningLevel::ModelDefault,
        );
        assert_eq!(row.record.status, OUTPUT_LIMIT);
        assert_eq!(row.record.http_status, Some(200));
        assert_eq!(row.record.error_code.as_deref(), Some("length"));
        assert_eq!(row.record.output_tokens, Some(200));
        assert!(row.record.cost.0 > 0, "Cost > 0: {:?}", row.record);
    }
}

// spec: AI-006 [M4]
// spec: AI-020 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Kriterium (AI-006): Ollama-Szenario `output-limit` → `Error: Output limit reached`, `Error Code = length`, genau
// 1 Request, Kosten 0; `HTTP Status = 200`; die gemeldeten Tokens (`eval_count` = 16 laut Konventionen) stehen im
// Protokoll; mit und ohne Schema.
#[tokio::test]
async fn ollama_output_limit_with_and_without_schema() {
    let mock = Mock::start().await;
    let adapter = mock.ollama();
    let cloud = Cloud::Ollama(&adapter);
    let limiter = fast_limiter();
    for (marker, schema) in [
        (
            "K10 ollama schema MOCK-SCENARIO:output-limit",
            Some(ok_schema()),
        ),
        ("K10 ollama free MOCK-SCENARIO:output-limit", None),
    ] {
        let r = request(
            model(
                "ollama",
                "mock-llama",
                os_ai::adapters::ollama::ENDPOINT,
                Vec::new(),
            ),
            marker,
            schema.clone(),
            ReasoningLevel::ModelDefault,
            Some(128),
        );
        if schema.is_some() {
            let run = cloud.structured(&limiter, &r).await;
            let StructuredOutcome::Failed(ref error) = run.outcome else {
                panic!("erwartet Abbruch ohne Reparatur: {:?}", run.outcome);
            };
            assert_output_limit_final(error, "length", marker);
        } else {
            let (result, attempts) = cloud.entry(&limiter, &r).await;
            assert_eq!(attempts, 1);
            assert_output_limit_final(&result.expect_err("kein Ergebnis"), "length", marker);
        }
        assert_eq!(mock.bodies_with(OLLAMA_CHAT, marker).len(), 1, "{marker}");

        let limit = cloud.limit(&r).await;
        assert_eq!(limit.usage.output_tokens, Some(16));
        let row = row_for(
            &limit,
            "suggest-title",
            "ollama",
            "mock-llama",
            os_ai::adapters::ollama::ENDPOINT,
            &Rates::free(PriceSource::Table),
            ReasoningLevel::ModelDefault,
        );
        assert_eq!(row.record.status, OUTPUT_LIMIT);
        assert_eq!(row.record.http_status, Some(200));
        assert_eq!(row.record.error_code.as_deref(), Some("length"));
        assert_eq!(row.record.output_tokens, Some(16));
        assert_eq!(row.record.cost, MicroUsd(0), "Kosten 0");
    }
}

// spec: AI-021 [M4]
// spec: AI-020 [M4]
// acceptance: independent
// Gegenprobe (AI-021 Nachtrag, erstes Kriterium): `malformed-json` einmalig, danach `ok` → unverändert 2 Requests,
// gültiges Ergebnis; `schema-violation` dauerhaft → nach 2 Requests `Error: Invalid output`. Der Reparaturversuch sendet
// dieselbe Reasoning-Stufe und dasselbe `max_tokens` wie der erste Request.
#[tokio::test]
async fn invalid_output_keeps_its_repair_attempt() {
    let mock = Mock::start().await;
    let a = anthropic_model(&mock, "A").await;
    let adapter = mock.anthropic();
    let cloud = Cloud::Anthropic(&adapter);
    let limiter = fast_limiter();
    warm_up(&cloud, &a.id).await;

    mock.scenario("anthropic", "malformed-json", 1).await;
    let r = request(
        model("anthropic", &a.id, "messages", Vec::new()),
        "K10 repair malformed",
        Some(ok_schema()),
        ReasoningLevel::Medium,
        Some(1500),
    );
    let run = cloud.structured(&limiter, &r).await;
    assert!(
        matches!(run.outcome, StructuredOutcome::Valid(_)),
        "{:?}",
        run.outcome
    );
    assert_eq!(run.outcome.final_status(), AiStatus::Success);
    let sent = mock.bodies_with(MESSAGES, "K10 repair malformed");
    assert_eq!(sent.len(), 2, "1 + 1 Reparaturversuch");

    mock.scenario("anthropic", "schema-violation", 2).await;
    let r = request(
        model("anthropic", &a.id, "messages", Vec::new()),
        "K10 repair violation",
        Some(ok_schema()),
        ReasoningLevel::Medium,
        Some(1500),
    );
    let run = cloud.structured(&limiter, &r).await;
    assert!(
        matches!(run.outcome, StructuredOutcome::InvalidOutput { .. }),
        "{:?}",
        run.outcome
    );
    assert_eq!(run.outcome.final_status(), AiStatus::InvalidOutput);
    assert_eq!(
        run.outcome.final_status().to_string(),
        "Error: Invalid output"
    );
    let sent = mock.bodies_with(MESSAGES, "K10 repair violation");
    assert_eq!(sent.len(), 2);
    for pointer in ["/thinking", "/output_config/effort", "/max_tokens"] {
        assert_eq!(
            sent[0].pointer(pointer),
            sent[1].pointer(pointer),
            "Reparaturversuch mit gleichem {pointer}"
        );
    }
    assert_eq!(sent[1].pointer("/max_tokens"), Some(&Value::from(1500)));

    // Dasselbe über OpenAI Responses (eigene Schleife des Adapters).
    let responses = mock.responses();
    responses
        .send(
            &request(
                model("openai", "gpt-6-luna", "responses", luna_levels()),
                "K10 warm-up responses",
                None,
                ReasoningLevel::Off,
                Some(256),
            ),
            None,
            None,
        )
        .await
        .expect("Aufwärmen");
    mock.scenario("openai", "malformed-json", 1).await;
    let r = request(
        model("openai", "gpt-6-luna", "responses", luna_levels()),
        "K10 repair responses",
        Some(ok_schema()),
        ReasoningLevel::Low,
        Some(900),
    );
    let run = responses
        .run_structured(
            &limiter,
            &r,
            &ResponseSchema::new(ok_schema()).unwrap(),
            RepairAttempts::default(),
            10,
        )
        .await;
    assert!(
        matches!(run.outcome, StructuredOutcome::Valid(_)),
        "{:?}",
        run.outcome
    );
    let sent = mock.bodies_with(RESPONSES, "K10 repair responses");
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].pointer("/reasoning"), sent[1].pointer("/reasoning"));
    assert_eq!(
        sent[0].pointer("/max_output_tokens"),
        sent[1].pointer("/max_output_tokens")
    );
}

// spec: AI-021 [M4]
// spec: AI-020 [M4]
// acceptance: independent
// Kriterium (AI-021 Nachtrag): `malformed-json` einmalig, danach `output-limit` → 2 Requests,
// `Error: Output limit reached`; beide Requests tragen dieselbe Reasoning-Stufe und dasselbe `max_tokens` (Mock-Log).
#[tokio::test]
async fn repair_that_ends_at_the_output_limit() {
    let mock = Mock::start().await;
    let a = anthropic_model(&mock, "A").await;
    let adapter = mock.anthropic();
    let cloud = Cloud::Anthropic(&adapter);
    let limiter = fast_limiter();
    let r = request(
        model("anthropic", &a.id, "messages", Vec::new()),
        "K10 repair then limit",
        Some(ok_schema()),
        ReasoningLevel::Low,
        Some(700),
    );
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    warm_up(&cloud, &a.id).await;
    mock.scenario("anthropic", "malformed-json", 1).await;
    let run = run_with_repair(&schema, RepairAttempts::default(), |repair| {
        let (cloud, limiter, r, mock) = (&cloud, &limiter, &r, &mock);
        async move {
            if repair.is_some() {
                mock.scenario("anthropic", "output-limit", 1).await;
            }
            let spec = RequestSpec::new("anthropic", r.model.id.clone(), 10);
            limiter
                .run(&spec, |_| cloud.attempt(r, repair.as_ref()))
                .await
                .result
                .map(|reply| (reply.raw.clone(), reply))
        }
    })
    .await;
    let StructuredOutcome::Failed(ref error) = run.outcome else {
        panic!("erwartet Abbruch im Reparaturversuch: {:?}", run.outcome);
    };
    assert_output_limit_final(error, "max_tokens", "Reparaturversuch");
    let sent = mock.bodies_with(MESSAGES, "K10 repair then limit");
    assert_eq!(
        sent.len(),
        2,
        "erster Request + Reparaturversuch, keine weiteren"
    );
    for pointer in ["/thinking", "/output_config/effort", "/max_tokens"] {
        assert_eq!(
            sent[0].pointer(pointer),
            sent[1].pointer(pointer),
            "{pointer}"
        );
    }
    assert_eq!(sent[1].pointer("/max_tokens"), Some(&Value::from(700)));
}

// spec: AI-020 [M4]
// spec: AI-017 [M4]
// acceptance: independent
// Kriterium (AI-020 Nachtrag): Seitentyp-Frage im Ersatzweg mit Szenario `output-limit` → 22 × `Error: Output limit
// reached`, Mock-Log 22 Requests (kein Reparaturversuch); der Schutzschalter greift nicht (auch nach mehr als 10 Fällen
// sendet der Anbieter weiter). Gegenprobe: 10 endgültige HTTP-Fehler öffnen den Schutzschalter in derselben
// Konfiguration.
#[tokio::test]
async fn output_limit_does_not_trip_the_circuit_breaker() {
    let mock = Mock::start().await;
    let a = anthropic_model(&mock, "A").await;
    let adapter = mock.anthropic();
    let cloud = Cloud::Anthropic(&adapter);
    let limiter = fast_limiter();
    let page_type = serde_json::json!({
        "type": "object",
        "properties": {"page_type": {"type": "string", "enum": ["product", "category", "article", "other"]}},
        "required": ["page_type"],
        "additionalProperties": false
    });
    let mut statuses = Vec::new();
    for i in 0..22 {
        let r = request(
            model("anthropic", &a.id, "messages", Vec::new()),
            &format!("K10 breaker page {i} MOCK-SCENARIO:output-limit"),
            Some(page_type.clone()),
            ReasoningLevel::Off,
            Some(2 * (64 + 16 * 4)),
        );
        let run = cloud.structured(&limiter, &r).await;
        statuses.push(run.outcome.final_status());
        assert!(
            !limiter.breaker_open("anthropic"),
            "Schutzschalter offen nach {} Fällen",
            i + 1
        );
    }
    assert_eq!(statuses.len(), 22);
    assert!(
        statuses.iter().all(|s| *s == AiStatus::OutputLimitReached),
        "{statuses:?}"
    );
    assert_eq!(mock.bodies_with(MESSAGES, "K10 breaker page").len(), 22);

    // Der Anbieter sendet danach weiter.
    let r = request(
        model("anthropic", &a.id, "messages", Vec::new()),
        "K10 breaker after",
        None,
        ReasoningLevel::Off,
        Some(256),
    );
    let (result, attempts) = cloud.entry(&limiter, &r).await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(attempts, 1);

    // 9 endgültige Fehler, dann ein Abbruch an der Ausgabegrenze: der Abbruch ist nicht der zehnte Fehler.
    let fresh = fast_limiter();
    let spec = RequestSpec::new("anthropic", a.id.clone(), 10);
    for _ in 0..9 {
        let exec = fresh
            .run(&spec, |_| async {
                Err::<(), _>(AttemptFailure::from(ProviderError::Http {
                    status: 400,
                    message: "bad".into(),
                }))
            })
            .await;
        assert!(exec.result.is_err());
    }
    let r = request(
        model("anthropic", &a.id, "messages", Vec::new()),
        "K10 breaker tenth MOCK-SCENARIO:output-limit",
        None,
        ReasoningLevel::Off,
        Some(64),
    );
    let (result, _) = cloud.entry(&fresh, &r).await;
    assert_eq!(
        result.expect_err("Abbruch").status,
        AiStatus::OutputLimitReached
    );
    assert!(
        !fresh.breaker_open("anthropic"),
        "Abbruch zählt nicht als zehnter Fehler"
    );

    // Gegenprobe: 10 endgültige Fehler öffnen den Schutzschalter.
    let control = fast_limiter();
    for _ in 0..10 {
        let _ = control
            .run(&spec, |_| async {
                Err::<(), _>(AttemptFailure::from(ProviderError::Http {
                    status: 400,
                    message: "bad".into(),
                }))
            })
            .await;
    }
    assert!(
        control.breaker_open("anthropic"),
        "Testaufbau: Schutzschalter muss öffnen"
    );
}

// spec: AI-020 [M4]
// spec: AI-017 [M4]
// acceptance: independent
// Fehlerklasse: `Error: Output limit reached` wird nicht automatisch wiederholt und zählt nicht für den Schutzschalter
// (AI-020 Nachtrag: „keine automatische Wiederholung“, „Zählt nicht für den Schutzschalter“).
#[test]
fn output_limit_error_class_is_final_and_not_counted() {
    for reason in ["max_tokens", "max_output_tokens", "length"] {
        let error = ProviderError::OutputLimitReached {
            reason: reason.into(),
        };
        assert_eq!(error.status(), Some(AiStatus::OutputLimitReached));
        assert!(
            !AttemptFailure::from(error.clone()).is_transient(),
            "{reason}"
        );
        assert_ne!(BreakerEffect::of(&error), BreakerEffect::Count, "{reason}");
    }
}

// spec: AI-020 [M4]
// acceptance: independent
// Kriterium (AI-020 Nachtrag): Der Property-Test „nur Werte aus der Tabelle“ kennt den neuen Wert; der Filter „Errors“
// (Muster `Error: …`, `Quota exceeded`, `Budget exceeded`) enthält diese Einträge.
#[test]
fn status_value_is_in_the_table_and_in_the_errors_filter() {
    let status = AiStatus::OutputLimitReached;
    assert_eq!(status.to_string(), OUTPUT_LIMIT);
    assert_eq!(
        serde_json::to_value(&status).unwrap(),
        Value::from(OUTPUT_LIMIT)
    );
    assert_eq!(
        serde_json::from_value::<AiStatus>(Value::from(OUTPUT_LIMIT)).unwrap(),
        status
    );
    assert!(SPEC_STATUS_VALUES.contains(&OUTPUT_LIMIT));
    assert!(
        status.is_error(),
        "Filter „Errors“ und „Fehlgeschlagene erneut anfragen“"
    );

    // Jeder Statuswert stammt aus der Tabelle; „Errors“ = genau `Error: …`, `Quota exceeded`, `Budget exceeded`.
    let all = [
        AiStatus::Success,
        AiStatus::SuccessCached,
        AiStatus::Refusal,
        AiStatus::Pending,
        AiStatus::PendingBatch,
        AiStatus::PendingConfirmation,
        AiStatus::PendingConsent,
        AiStatus::SkippedScope,
        AiStatus::SkippedSegment,
        AiStatus::SkippedEmptyInput,
        AiStatus::SkippedPolicy,
        AiStatus::SkippedNoCapableModel,
        AiStatus::SkippedImageTooLarge,
        AiStatus::SkippedUnsupportedFormat,
        AiStatus::SkippedNoDeclaredLanguage,
        AiStatus::BudgetExceeded,
        AiStatus::QuotaExceeded,
        AiStatus::MissingApiKey,
        AiStatus::InvalidOutput,
        AiStatus::MissingAnswer,
        AiStatus::BatchExpired,
        AiStatus::ConnectionFailed,
        AiStatus::OutputLimitReached,
        AiStatus::Timeout { seconds: 30 },
        AiStatus::HttpError {
            code: 500,
            message: "Temporary error".into(),
        },
    ];
    for s in &all {
        let text = s.to_string();
        let known = SPEC_STATUS_VALUES
            .iter()
            .any(|v| text == *v || (v.ends_with(' ') && text.starts_with(v)))
            || (text.starts_with("Error: ")
                && text[7..]
                    .split(' ')
                    .next()
                    .is_some_and(|c| c.parse::<u16>().is_ok()));
        assert!(known, "Statuswert außerhalb der Tabelle: {text}");
        let errors_filter =
            text.starts_with("Error: ") || text == "Quota exceeded" || text == "Budget exceeded";
        assert_eq!(s.is_error(), errors_filter, "{text}");
    }
}

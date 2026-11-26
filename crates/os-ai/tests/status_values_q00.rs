//! M4-K10: Status `Error: Output limit reached` (AI-020, AI-021, AI-017, AI-014; Nachträge 24.11.2026, Q00-H1) gegen
//! ai-mock. Die Adapter liefern seit M4-K09 eine eigene Antwortklasse für den Abbruch an der Ausgabegrenze
//! (`AdapterReply::for_repair` → `OutputLimitReached`, Responses `ResponsesReply::output_limit`); hier wird sie über
//! `OutputLimitReached::output_limit_failure` bzw. `ProviderError::output_limit` auf den Statuswert abgebildet und bis
//! in die Protokollzeile verfolgt.
#[path = "cost_q00_support.rs"]
mod support;

use os_ai::adapters::ollama;
use os_ai::adapters::openai_responses::OpenAiResponses;
use os_ai::cost::{
    cost_of, detailed_request_record, FeeCounts, OutcomeDetails, PriceSource, Rates,
    RequestOutcome, TokenCounts,
};
use os_ai::limits::{AttemptFailure, BreakerEffect, RequestSpec};
use os_ai::provider::{ProviderError, ReasoningLevel};
use os_ai::schema::{run_with_repair, RepairAttempts, ResponseSchema, StructuredOutcome};
use os_ai::AiStatus;
use os_model::budget::MicroUsd;
use serde_json::Value;
use support::{
    limiter, log_ctx, model, ok_schema, request, Mock, CHAT, MESSAGES, OLLAMA_CHAT, RESPONSES,
};

fn claude_rates() -> Rates {
    Rates::per_million("1", "5", PriceSource::Table).unwrap()
}

// spec: AI-020 [M4]
// Neuer Statuswert: kanonischer Text, Serde-Name, gehört zum Filter „Errors“ und zu „Fehlgeschlagene erneut anfragen“
// (`is_error`), Fehlerklasse `ProviderError::OutputLimitReached` bildet darauf ab.
#[test]
fn output_limit_status_is_an_error_value_with_its_canonical_text() {
    let status = AiStatus::OutputLimitReached;
    assert_eq!(status.to_string(), "Error: Output limit reached");
    assert_eq!(
        serde_json::to_value(&status).unwrap(),
        Value::String("Error: Output limit reached".into())
    );
    assert_eq!(
        serde_json::from_str::<AiStatus>("\"Error: Output limit reached\"").unwrap(),
        status
    );
    assert!(status.is_error(), "Filter „Errors“ und Wiederholung");
    let error = ProviderError::output_limit("max_tokens");
    assert_eq!(error.status(), Some(AiStatus::OutputLimitReached));
    assert_eq!(error.to_string(), "Error: Output limit reached");
    // Nicht wiederholbar (keine automatische Wiederholung) und neutral für den Schutzschalter.
    let failure = AttemptFailure::from(error.clone());
    assert!(!failure.is_transient());
    assert_eq!(BreakerEffect::of(&error), BreakerEffect::Neutral);
}

// spec: AI-002 [M4]
// spec: AI-020 [M4]
// spec: AI-021 [M4]
// spec: AI-014 [M4]
// Ersetzt `pending_m4_k09::anthropic_output_limit_status_in_log`: Szenario `output-limit` bei einer Schema- und einer
// Freitext-Aufgabe → `Error: Output limit reached`, `Error Code = max_tokens`, `HTTP Status = 200`, genau 1 Request
// (kein Reparaturversuch), `Output Tokens` = gesendetes `max_tokens`, `Cost` > 0, Ergebnisspalten leer.
#[tokio::test]
async fn anthropic_output_limit_status_in_log() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let a = model("anthropic", "mock-claude", "messages", Vec::new());
    for schema in [Some(ok_schema()), None] {
        let before = mock.requests_to(MESSAGES).len();
        let req = request(
            a.clone(),
            "MOCK-SCENARIO:output-limit x",
            schema.clone(),
            ReasoningLevel::ModelDefault,
            256,
        );
        let checker = ResponseSchema::new(schema.clone().unwrap_or_else(ok_schema)).unwrap();
        let limit_seen = std::cell::RefCell::new(None);
        let run = run_with_repair(&checker, RepairAttempts::default(), |repair| {
            let adapter = &adapter;
            let req = &req;
            let seen = &limit_seen;
            async move {
                let reply = adapter.send(req, repair.as_ref()).await?;
                reply.for_repair().map_err(|limit| {
                    *seen.borrow_mut() = Some(limit.clone());
                    limit.output_limit_failure()
                })
            }
        })
        .await;
        assert!(matches!(run.outcome, StructuredOutcome::Failed(_)));
        assert_eq!(run.outcome.final_status(), AiStatus::OutputLimitReached);
        assert!(
            run.requests.is_empty(),
            "keine Ergebniszeile, kein Reparaturversuch"
        );
        assert_eq!(
            mock.requests_to(MESSAGES).len(),
            before + 1,
            "genau 1 Request"
        );
        let limit = limit_seen.into_inner().expect("Abbruchklasse");
        let failure = limit.output_limit_failure();
        assert_eq!(failure.error, ProviderError::output_limit("max_tokens"));
        assert_eq!(failure.error_code.as_deref(), Some("max_tokens"));
        assert_eq!(failure.http_status, Some(200));
        assert!(failure.provider_request_id.is_some());
        let row = detailed_request_record(
            &log_ctx("suggest-title", "anthropic", "mock-claude", "messages"),
            &claude_rates(),
            MicroUsd(10_000),
            &RequestOutcome::output_limit(&limit, 0),
            &OutcomeDetails {
                reasoning: Some(ReasoningLevel::ModelDefault),
                reported_cost: None,
                estimated_output_tokens: None,
            },
        );
        let r = &row.record;
        assert_eq!(r.status, "Error: Output limit reached");
        assert_eq!(r.http_status, Some(200));
        assert_eq!(r.error_code.as_deref(), Some("max_tokens"));
        assert_eq!(
            r.output_tokens,
            Some(256),
            "Output Tokens = gesendete Grenze"
        );
        assert!(r.cost.0 > 0, "gemeldete Tokens werden verbucht");
        assert_eq!(r.provider_request_id, limit.provider_request_id);
        assert_eq!(row.details.reasoning_tokens, Some(256));
        assert_eq!(
            row.details.reasoning_level.as_deref(),
            Some("model-default")
        );
    }
}

// spec: AI-003 [M4]
// spec: AI-020 [M4]
// spec: AI-014 [M4]
// spec: AI-001 [M4]
// Ersetzt `pending_m4_k09::responses_output_limit_status_and_reasoning_column`: Szenario `output-limit` →
// `Error: Output limit reached`, `Error Code = max_output_tokens`, genau 1 Request; Szenario `reasoning=27` →
// Protokollspalte `Reasoning Tokens = 27`.
#[tokio::test]
async fn responses_output_limit_status_and_reasoning_column() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.openai_http());
    let limiter = support::limiter();
    let luna = model("openai", "gpt-6-luna", "responses", Vec::new());
    let rates = Rates::per_million("0.1", "0.5", PriceSource::Table).unwrap();
    let ctx = log_ctx("suggest-title", "openai", "gpt-6-luna", "responses");

    let req = request(
        luna.clone(),
        "MOCK-SCENARIO:output-limit x",
        None,
        ReasoningLevel::Off,
        256,
    );
    let (reply, meta) = adapter.run(&limiter, &req, 10).await;
    let reply = reply.unwrap();
    let reason = reply.output_limit().expect("Abbruch an der Ausgabegrenze");
    let status = ProviderError::output_limit(reason).status().unwrap();
    assert_eq!(status, AiStatus::OutputLimitReached);
    assert_eq!(mock.requests_to(RESPONSES).len(), 1, "genau 1 Request");
    let row = detailed_request_record(
        &ctx,
        &rates,
        MicroUsd(1_000),
        &meta.request_outcome(status, 10, 0, 0),
        &OutcomeDetails {
            reasoning: Some(ReasoningLevel::Off),
            reported_cost: None,
            estimated_output_tokens: None,
        },
    );
    assert_eq!(row.record.status, "Error: Output limit reached");
    assert_eq!(row.record.error_code.as_deref(), Some("max_output_tokens"));
    assert_eq!(row.record.http_status, Some(200));
    assert_eq!(row.record.output_tokens, Some(256));
    assert!(row.record.cost.0 > 0);
    assert_eq!(row.details.reasoning_level.as_deref(), Some("off"));

    let req = request(
        luna,
        "MOCK-ID:home MOCK-SCENARIO:reasoning=27 u",
        None,
        ReasoningLevel::Low,
        4_096,
    );
    let (reply, meta) = adapter.run(&limiter, &req, 10).await;
    assert!(reply.unwrap().output_limit().is_none());
    let row = detailed_request_record(
        &ctx,
        &rates,
        MicroUsd(1_000),
        &meta.request_outcome(AiStatus::Success, 10, 0, 0),
        &OutcomeDetails {
            reasoning: Some(ReasoningLevel::Low),
            reported_cost: None,
            estimated_output_tokens: None,
        },
    );
    assert_eq!(row.details.reasoning_tokens, Some(27));
    assert_eq!(row.details.reasoning_level.as_deref(), Some("low"));
    // Reasoning nicht doppelt: Kosten allein aus `Output Tokens`.
    let expected = cost_of(
        &TokenCounts::plain(
            row.record.input_tokens.unwrap(),
            row.record.output_tokens.unwrap(),
        ),
        FeeCounts::one_request(0),
        &rates,
        false,
    );
    assert_eq!(row.record.token_cost, expected.token_cost);
}

// spec: AI-005 [M4]
// spec: AI-020 [M4]
// Ersetzt `pending_m4_k09::openrouter_output_limit_status`: Szenario `output-limit` → `Error: Output limit reached`,
// `Error Code = length`, genau 1 Request (Schema-Aufgabe, kein Reparaturversuch).
#[tokio::test]
async fn openrouter_output_limit_status() {
    let mock = Mock::start().await;
    let adapter = mock.openrouter();
    let req = request(
        model("openrouter", "mock/text", "chat-completions", Vec::new()),
        "MOCK-SCENARIO:output-limit x",
        Some(ok_schema()),
        ReasoningLevel::ModelDefault,
        256,
    );
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let run = run_with_repair(&schema, RepairAttempts::default(), |repair| {
        let adapter = &adapter;
        let req = &req;
        async move {
            let reply = adapter.send(req, repair.as_ref()).await?;
            reply.for_repair().map_err(|l| l.output_limit_failure())
        }
    })
    .await;
    assert_eq!(run.outcome.final_status(), AiStatus::OutputLimitReached);
    let StructuredOutcome::Failed(failure) = run.outcome else {
        panic!("erwartet Abbruch");
    };
    assert_eq!(failure.error_code.as_deref(), Some("length"));
    assert_eq!(failure.http_status, Some(200));
    assert_eq!(mock.requests_to(CHAT).len(), 1);
}

// spec: AI-006 [M4]
// spec: AI-020 [M4]
// Ersetzt `pending_m4_k09::ollama_output_limit_status`: Ollama-Szenario `output-limit` → `Error: Output limit
// reached`, `Error Code = length`, genau 1 Request, Kosten 0.
#[tokio::test]
async fn ollama_output_limit_status() {
    let mock = Mock::start().await;
    let adapter = mock.ollama();
    let req = request(
        model("ollama", "mock-llama", ollama::ENDPOINT, Vec::new()),
        "MOCK-ID:k09-long MOCK-SCENARIO:output-limit",
        Some(ok_schema()),
        ReasoningLevel::ModelDefault,
        256,
    );
    let reply = adapter.send(&req, None).await.unwrap();
    let limit = reply.for_repair().unwrap_err();
    let failure = limit.output_limit_failure();
    assert_eq!(failure.error.status(), Some(AiStatus::OutputLimitReached));
    assert_eq!(failure.error_code.as_deref(), Some("length"));
    assert_eq!(mock.requests_to(OLLAMA_CHAT).len(), 1);
    let row = detailed_request_record(
        &log_ctx("suggest-title", "ollama", "mock-llama", ollama::ENDPOINT),
        &ollama::rates(),
        MicroUsd(0),
        &RequestOutcome::output_limit(&limit, 0),
        &OutcomeDetails::default(),
    );
    assert_eq!(row.record.status, "Error: Output limit reached");
    assert_eq!(row.record.error_code.as_deref(), Some("length"));
    assert_eq!(row.record.cost, MicroUsd(0));
    assert_eq!(row.details.reasoning_tokens, None);
}

// spec: AI-020 [M4]
// spec: AI-017 [M4]
// Seitentyp-Frage im Ersatzweg auf der Fixture-Site (|G| = 25, |S| = 22) mit Szenario `output-limit` →
// 22 × `Error: Output limit reached`, 3 × `Skipped (scope)`, Mock-Log 22 Requests (kein Reparaturversuch); der
// Schutzschalter greift nicht (nach mehr als 10 Fällen sendet der Anbieter weiter). Ein „Lauf“ ist eine Schleife über
// die Einträge über denselben Limiter (Schutzschalter an, Schwelle 10).
#[tokio::test]
async fn fallback_run_with_output_limit_never_trips_the_breaker() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let limiter = support::limiter();
    let a = model("anthropic", "mock-claude", "messages", Vec::new());
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(support::fixture(&["site", "manifest.json"])).unwrap(),
    )
    .unwrap();
    let pages = manifest["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 25);
    let mut statuses = Vec::new();
    for page in pages {
        // Geltungsbereich S: Status 200 und indexierbar (wie die übrigen KI-Tests der Fixture-Site).
        let in_scope = page["status"] == 200 && page["indexable"] == true;
        if !in_scope {
            statuses.push(AiStatus::SkippedScope);
            continue;
        }
        assert!(!limiter.breaker_open("anthropic"), "Schutzschalter offen");
        let req = request(
            a.clone(),
            &format!(
                "Seite {} MOCK-SCENARIO:output-limit",
                page["file"].as_str().unwrap()
            ),
            Some(ok_schema()),
            ReasoningLevel::Off,
            576,
        );
        let spec = RequestSpec::new("anthropic", "mock-claude", 100);
        let run = run_with_repair(&schema, RepairAttempts::default(), |repair| {
            let (adapter, req, limiter, spec) = (&adapter, &req, &limiter, &spec);
            async move {
                let repair = repair.as_ref();
                limiter
                    .run(spec, |_| async move {
                        let reply = adapter.send(req, repair).await?;
                        reply.for_repair().map_err(|l| l.output_limit_failure())
                    })
                    .await
                    .result
            }
        })
        .await;
        statuses.push(run.outcome.final_status());
    }
    let count = |s: &AiStatus| statuses.iter().filter(|x| *x == s).count();
    assert_eq!(count(&AiStatus::OutputLimitReached), 22);
    assert_eq!(count(&AiStatus::SkippedScope), 3);
    assert_eq!(
        mock.requests_to(MESSAGES).len(),
        22,
        "kein Reparaturversuch"
    );
    assert!(!limiter.breaker_open("anthropic"));
    // Filter „Errors“: alle 22, keiner der übersprungenen.
    assert_eq!(statuses.iter().filter(|s| s.is_error()).count(), 22);
}

// spec: AI-021 [M4]
// `malformed-json` einmalig, danach Abbruch an der Ausgabegrenze → 2 Requests, `Error: Output limit reached`; beide
// Requests tragen dieselbe Reasoning-Stufe und dasselbe `max_tokens` (Mock-Log). Die Grenze 1 lässt den Mock nach
// seiner Regel „Antwort + Reasoning > Grenze“ abbrechen.
#[tokio::test]
async fn repair_attempt_ending_at_the_limit_keeps_level_and_max_tokens() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let models = adapter.list_models().await.unwrap();
    let a = models
        .iter()
        .find(|m| m.id == "mock-claude")
        .unwrap()
        .to_model();
    mock.global_scenario("anthropic", "malformed-json", 1).await;
    let req = request(
        a,
        "MOCK-ID:home k10",
        Some(ok_schema()),
        ReasoningLevel::Medium,
        1,
    );
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let run = run_with_repair(&schema, RepairAttempts::default(), |repair| {
        let adapter = &adapter;
        let req = &req;
        async move {
            let reply = adapter.send(req, repair.as_ref()).await?;
            reply.for_repair().map_err(|l| l.output_limit_failure())
        }
    })
    .await;
    assert_eq!(run.requests.len(), 1, "erste Antwort verworfen");
    assert!(run.requests[0].validator_error.is_some());
    assert_eq!(run.outcome.final_status(), AiStatus::OutputLimitReached);
    let bodies = mock.bodies(MESSAGES);
    assert_eq!(bodies.len(), 2, "2 Requests");
    for key in ["/max_tokens", "/thinking", "/output_config/effort"] {
        assert_eq!(
            bodies[0].pointer(key),
            bodies[1].pointer(key),
            "{key}: Reparaturversuch mit derselben Stufe und Grenze"
        );
    }
    assert_eq!(bodies[1].pointer("/max_tokens"), Some(&Value::from(1)));
    assert_eq!(
        bodies[1]
            .pointer("/output_config/effort")
            .and_then(Value::as_str),
        Some("medium")
    );
}

// spec: AI-021 [M4]
// Gegenprobe: `malformed-json` einmalig, danach `ok` → unverändert 2 Requests, gültiges Ergebnis (ein echter
// Schemaverstoß löst weiter genau einen Reparaturversuch aus).
#[tokio::test]
async fn malformed_json_once_then_ok_still_repairs() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    mock.global_scenario("anthropic", "malformed-json", 1).await;
    let req = request(
        model("anthropic", "mock-claude", "messages", Vec::new()),
        "MOCK-ID:home k10",
        Some(ok_schema()),
        ReasoningLevel::ModelDefault,
        1_024,
    );
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let run = run_with_repair(&schema, RepairAttempts::default(), |repair| {
        let adapter = &adapter;
        let req = &req;
        async move {
            let reply = adapter.send(req, repair.as_ref()).await?;
            reply.for_repair().map_err(|l| l.output_limit_failure())
        }
    })
    .await;
    assert_eq!(run.outcome.final_status(), AiStatus::Success);
    assert_eq!(mock.requests_to(MESSAGES).len(), 2);
}

// spec: AI-017 [M4]
// `Error: Output limit reached` zählt nicht für den Schutzschalter: 9 endgültige 500er, dann 5 Abbrüche an der
// Ausgabegrenze, dann ein weiterer 500er → erst dieser zehnte zählende Fehler öffnet den Schalter (die Abbrüche haben
// weder gezählt noch die Zählung zurückgesetzt).
#[tokio::test(start_paused = true)]
async fn output_limit_is_neutral_between_counting_failures() {
    let limiter = support::limiter();
    let spec = RequestSpec::new("anthropic", "m", 10);
    let fail = |error: ProviderError| {
        let limiter = &limiter;
        let spec = &spec;
        async move {
            limiter
                .run(spec, |_| {
                    let error = error.clone();
                    async move { Err::<(), _>(AttemptFailure::from(error)) }
                })
                .await
        }
    };
    let http500 = || ProviderError::Http {
        status: 500,
        message: "boom".into(),
    };
    for _ in 0..9 {
        fail(http500()).await;
    }
    for _ in 0..5 {
        let exec = fail(ProviderError::output_limit("length")).await;
        assert_eq!(exec.attempts, 1, "keine automatische Wiederholung");
        let error = exec.result.unwrap_err();
        assert_eq!(error.status, AiStatus::OutputLimitReached);
        assert_eq!(error.error_code.as_deref(), Some("length"));
        assert!(!limiter.breaker_open("anthropic"));
    }
    fail(http500()).await;
    assert!(limiter.breaker_open("anthropic"));
}

// spec: AI-002 [M4]
// spec: AI-003 [M4]
// spec: AI-005 [M4]
// spec: AI-006 [M4]
// spec: AI-020 [M4]
// Standardwege der Adapter (Umstellung freigegeben 26.11.2026): Szenario `output-limit` je Anbieter →
// `provider::execute` liefert `Error: Output limit reached`; der Versuchsfehler (`OutputLimitReached::attempt_failure`
// bzw. Responses `run_structured`) trägt `Error Code` = Grund des Anbieters; genau 1 Request je Eintrag; Ollama
// Kosten 0.
#[tokio::test]
async fn standard_adapter_paths_give_the_output_limit_status() {
    use os_ai::provider::{execute, Outcome};
    use os_ai::PreviewSwitch;
    let mock = Mock::start().await;
    let text = "MOCK-ID:k09-long MOCK-SCENARIO:output-limit";

    let anthropic = mock.anthropic();
    let a = model("anthropic", "mock-claude", "messages", Vec::new());
    let req = request(
        a,
        text,
        Some(ok_schema()),
        ReasoningLevel::ModelDefault,
        256,
    );
    assert_eq!(
        execute(PreviewSwitch::ON, &anthropic, req.clone()).await,
        Outcome::Status(AiStatus::OutputLimitReached)
    );
    let limit = anthropic
        .send(&req, None)
        .await
        .unwrap()
        .for_repair()
        .unwrap_err();
    let failure = limit.attempt_failure();
    assert_eq!(failure.error.status(), Some(AiStatus::OutputLimitReached));
    assert_eq!(failure.error_code.as_deref(), Some("max_tokens"));
    assert_eq!(mock.requests_to(MESSAGES).len(), 2, "1 Request je Eintrag");

    let router = mock.openrouter();
    let req = request(
        model("openrouter", "mock/text", "chat-completions", Vec::new()),
        text,
        Some(ok_schema()),
        ReasoningLevel::ModelDefault,
        256,
    );
    assert_eq!(
        execute(PreviewSwitch::ON, &router, req.clone()).await,
        Outcome::Status(AiStatus::OutputLimitReached)
    );
    let limit = router
        .send(&req, None)
        .await
        .unwrap()
        .for_repair()
        .unwrap_err();
    assert_eq!(
        limit.attempt_failure().error_code.as_deref(),
        Some("length")
    );
    assert_eq!(mock.requests_to(CHAT).len(), 2);

    let llama = mock.ollama();
    let req = request(
        model("ollama", "mock-llama", ollama::ENDPOINT, Vec::new()),
        text,
        Some(ok_schema()),
        ReasoningLevel::ModelDefault,
        256,
    );
    assert_eq!(
        execute(PreviewSwitch::ON, &llama, req.clone()).await,
        Outcome::Status(AiStatus::OutputLimitReached)
    );
    let limit = llama
        .send(&req, None)
        .await
        .unwrap()
        .for_repair()
        .unwrap_err();
    assert_eq!(
        limit.attempt_failure().error_code.as_deref(),
        Some("length")
    );
    let cost = cost_of(
        &TokenCounts::from_usage(&limit.usage),
        FeeCounts::one_request(0),
        &ollama::rates(),
        false,
    );
    assert_eq!(cost.cost, MicroUsd(0));
    assert_eq!(mock.requests_to(OLLAMA_CHAT).len(), 2);

    let responses = OpenAiResponses::new(mock.openai_http());
    let req = request(
        model("openai", "gpt-6-luna", "responses", Vec::new()),
        text,
        None,
        ReasoningLevel::ModelDefault,
        256,
    );
    assert_eq!(
        execute(PreviewSwitch::ON, &responses, req.clone()).await,
        Outcome::Status(AiStatus::OutputLimitReached)
    );
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let run = responses
        .run_structured(&limiter(), &req, &schema, RepairAttempts::default(), 10)
        .await;
    let StructuredOutcome::Failed(failure) = run.outcome else {
        panic!("erwartet Abbruch");
    };
    assert_eq!(failure.error.status, AiStatus::OutputLimitReached);
    assert_eq!(
        failure.error.error_code.as_deref(),
        Some("max_output_tokens")
    );
    assert_eq!(failure.error.http_status, Some(200));
    assert_eq!(
        mock.requests_to(RESPONSES).len(),
        2,
        "kein Reparaturversuch"
    );
}

// spec: AI-017 [M4]
// Nachtrag 26.11.2026: 9 endgültige Fehler, dann `Error: Output limit reached`, dann 1 endgültiger Fehler → der
// Schutzschalter öffnet (Zählung 10); dieselbe Folge mit einem `Success` an der Stelle des Ausgabegrenzen-Falls → er
// öffnet nicht. Refusal (als Fehler und als erhaltene Antwort) und Spend-Cap verhalten sich wie die Ausgabegrenze.
#[tokio::test(start_paused = true)]
async fn only_success_resets_the_breaker() {
    let http500 = || ProviderError::Http {
        status: 500,
        message: "boom".into(),
    };
    let spec = RequestSpec::new("anthropic", "m", 10);
    // Folge mit einem neutralen Fall an Stelle 10.
    for neutral in [
        ProviderError::output_limit("max_tokens"),
        ProviderError::Refusal,
        ProviderError::QuotaExceeded,
    ] {
        let limiter = support::limiter();
        for _ in 0..9 {
            failing(&limiter, &spec, http500()).await;
        }
        failing(&limiter, &spec, neutral.clone()).await;
        assert!(!limiter.breaker_open("anthropic"), "{neutral:?}");
        failing(&limiter, &spec, http500()).await;
        assert!(
            limiter.breaker_open("anthropic"),
            "{neutral:?} zählt nicht und setzt nicht zurück"
        );
    }
    // Refusal als erhaltene Antwort (`run_classified`, wie `run_provider`): ebenfalls neutral.
    let limiter = support::limiter();
    for _ in 0..9 {
        failing(&limiter, &spec, http500()).await;
    }
    limiter
        .run_classified(
            &spec,
            |_| async { Ok::<_, AttemptFailure>(true) },
            |refusal: &bool| {
                if *refusal {
                    BreakerEffect::Neutral
                } else {
                    BreakerEffect::Reset
                }
            },
        )
        .await;
    failing(&limiter, &spec, http500()).await;
    assert!(limiter.breaker_open("anthropic"));
    // Dieselbe Folge mit `Success` an der Stelle → nicht offen.
    let limiter = support::limiter();
    for _ in 0..9 {
        failing(&limiter, &spec, http500()).await;
    }
    let ok = limiter
        .run(&spec, |_| async { Ok::<_, AttemptFailure>(()) })
        .await;
    assert!(ok.result.is_ok());
    failing(&limiter, &spec, http500()).await;
    assert!(!limiter.breaker_open("anthropic"), "Success setzt zurück");
}

async fn failing(limiter: &os_ai::limits::Limiter, spec: &RequestSpec, error: ProviderError) {
    limiter
        .run(spec, |_| {
            let error = error.clone();
            async move { Err::<(), _>(AttemptFailure::from(error)) }
        })
        .await;
}

//! Unabhängige Abnahmetests M4-K01, Teil 1: Adapter Anthropic Messages API (AI-002), allein aus
//! `docs/spec/08-ai.md` abgeleitet (dazu AI-001, AI-017, AI-020, AI-021, INTEG-005 und die Szenariotabelle des
//! ai-mock). Jeder Test nennt sein Akzeptanzkriterium als `Kn` (Reihenfolge der Kriterien in AI-002).
//!
//! Gegenstelle ist der ai-mock aus `os-testsite`; was er nicht liefert (echte Header-Werte, Cache-Tokens, 503), kommt
//! von einem kleinen lokalen Testserver. Wartezeiten laufen auf einer Testuhr.
// acceptance: independent

mod acceptance_k01_support;

use std::sync::Arc;

use acceptance_k01_support::*;
use os_ai::adapters::anthropic::{
    image_input, AdapterReply, AnthropicAdapter, AnthropicConfig, AnthropicModel, ApiKey,
    Selectability,
};
use os_ai::limits::{Execution, Limiter};
use os_ai::provider::{self, Capabilities, CapabilitySource, Input, Model, Outcome, Request};
use os_ai::schema::{run_with_repair, RepairAttempts, ResponseSchema, StructuredOutcome};
use os_ai::{AiStatus, PreviewSwitch};
use serde_json::{json, Value};

const MESSAGES: &str = "/anthropic/v1/messages";

fn adapter(base: &str) -> AnthropicAdapter {
    AnthropicAdapter::new(
        client(),
        AnthropicConfig::new(ApiKey::new(ANTHROPIC_TEST_KEY)).with_base_url(base),
    )
}

fn claude(id: &str) -> Model {
    model(
        "anthropic",
        id,
        "messages",
        CapabilitySource::AnthropicModels,
    )
}

async fn entry(
    limiter: &Limiter,
    adapter: &AnthropicAdapter,
    request: &Request,
) -> Execution<AdapterReply> {
    run_entry(limiter, "anthropic", &request.model.id, move |_| {
        adapter.send(request, None)
    })
    .await
}

/// Endstatus eines Eintrags nach AI-020 (bei Erfolg Refusal oder Success).
fn status(exec: &Execution<AdapterReply>) -> AiStatus {
    match &exec.result {
        Ok(reply) if reply.is_refusal() => AiStatus::Refusal,
        Ok(_) => AiStatus::Success,
        Err(error) => error.status.clone(),
    }
}

/// Schema-Aufgabe über Begrenzung und lokale Validierung mit Reparatur (AI-021).
async fn structured(
    limiter: &Limiter,
    adapter: &AnthropicAdapter,
    request: &Request,
) -> os_ai::schema::StructuredRun<(), os_ai::limits::FinalError> {
    let schema = ResponseSchema::new(request.response_schema.clone().unwrap()).unwrap();
    run_with_repair(&schema, RepairAttempts::default(), |repair| async move {
        let exec = run_entry(limiter, "anthropic", &request.model.id, |_| {
            let repair = repair.as_ref();
            adapter.send(request, repair)
        })
        .await;
        exec.result.map(|reply| (reply.raw, ()))
    })
    .await
}

fn message_body(text: &str, usage: Value) -> Value {
    json!({
        "id": "msg_k01",
        "type": "message",
        "role": "assistant",
        "model": "mock-claude",
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn",
        "usage": usage
    })
}

// ---------------------------------------------------------------------------------------------------------------
// K1: Pflicht-Header; Schlüssel in keinem Log

// spec: AI-002
// acceptance: independent
#[tokio::test]
async fn k1_jeder_request_enthaelt_api_key_version_und_content_type() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.anthropic_base());
    let request = request(claude("mock-claude"), "K1 header check", None);
    adapter.send(&request, None).await.expect("Antwort");
    adapter.list_models().await.expect("Modellliste");
    let _ = adapter.count_tokens(&request).await;

    let log: Vec<_> = mock
        .log()
        .into_iter()
        .filter(|r| r.path.starts_with("/anthropic/"))
        .collect();
    assert!(log.len() >= 2, "erwartet Requests an /anthropic/*: {log:?}");
    for record in &log {
        let header = |name: &str| record.headers.get(name).map(String::as_str);
        assert!(
            header("x-api-key").is_some_and(|v| !v.is_empty()),
            "x-api-key fehlt: {} {}",
            record.method,
            record.path
        );
        assert!(
            header("anthropic-version").is_some_and(|v| !v.trim().is_empty()),
            "anthropic-version fehlt: {} {}",
            record.method,
            record.path
        );
        assert_eq!(
            header("content-type").map(|v| v.split(';').next().unwrap().trim()),
            Some("application/json"),
            "content-type: {} {}",
            record.method,
            record.path
        );
    }
}

// spec: AI-002
// acceptance: independent
#[tokio::test]
async fn k1_x_api_key_traegt_den_schluessel_des_kontos() {
    // Der ai-mock maskiert den Header im Protokoll; der Wert selbst wird am eigenen Testserver geprüft.
    let stub = Stub::start(|_, _| {
        StubResponse::json(
            200,
            message_body("hi", json!({"input_tokens": 3, "output_tokens": 1})),
        )
    })
    .await;
    let adapter = adapter(&stub.base());
    adapter
        .send(&request(claude("mock-claude"), "K1 value", None), None)
        .await
        .expect("Antwort");
    let requests = stub.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/v1/messages");
    assert_eq!(requests[0].header("x-api-key"), Some(ANTHROPIC_TEST_KEY));
    assert!(requests[0].header("anthropic-version").is_some());
}

// spec: AI-002
// acceptance: independent
#[tokio::test]
async fn k1_schluessel_erscheint_in_keinem_log() {
    let mock = Mock::start().await;
    let (limiter, _) = new_limiter(1, 10);
    let adapter = adapter(&mock.anthropic_base());
    let ok = entry(
        &limiter,
        &adapter,
        &request(claude("mock-claude"), "K1 ok", None),
    )
    .await;
    let failed = entry(
        &limiter,
        &adapter,
        &request(claude("mock-claude"), "K1 MOCK-SCENARIO:401 denied", None),
    )
    .await;
    assert!(failed.result.is_err(), "401 muss fehlschlagen");

    let mut texts = vec![
        format!("{adapter:?}"),
        format!("{:?}", adapter.config()),
        format!("{ok:?}"),
        format!("{failed:?}"),
        status(&failed).to_string(),
        serde_json::to_string(&mock.mock.requests()).unwrap(),
    ];
    if let Err(error) = &failed.result {
        texts.push(error.message.clone());
        texts.push(error.error.to_string());
    }
    for text in texts {
        assert!(
            !text.contains(ANTHROPIC_TEST_KEY),
            "Schlüssel im Log/Debug-Text: {text}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// K2: strukturierte Ausgabe über output_config.format (nie output_format); Beschreibung: Systemtext in `system`

// spec: AI-002
// spec: AI-021
// acceptance: independent
#[tokio::test]
async fn k2_schema_aufgabe_sendet_output_config_format_json_schema() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.anthropic_base());
    let schema = ok_schema();
    let with_schema = request(claude("mock-claude"), "K2 schema", Some(schema.clone()));
    let outcome = provider::execute(PreviewSwitch::ON, &adapter, with_schema).await;
    match outcome {
        Outcome::Completed(response) => assert_eq!(response.content, Some(json!({"ok": true}))),
        other => panic!("Schema-Aufgabe sollte gelingen: {other:?}"),
    }
    adapter
        .send(&request(claude("mock-claude"), "K2 text", None), None)
        .await
        .expect("Antwort");

    let bodies = mock.bodies_to("POST", MESSAGES);
    assert_eq!(bodies.len(), 2);
    let format = &bodies[0]["output_config"]["format"];
    assert_eq!(format["type"], "json_schema", "Körper: {}", bodies[0]);
    assert_eq!(format["schema"], schema, "Körper: {}", bodies[0]);
    for body in &bodies {
        assert!(
            !contains_key(body, "output_format"),
            "output_format darf nie vorkommen: {body}"
        );
    }
    assert!(
        bodies[1]
            .get("output_config")
            .and_then(|c| c.get("format"))
            .is_none(),
        "Text-Aufgabe ohne Schema sendet kein Format: {}",
        bodies[1]
    );
}

// spec: AI-002
// acceptance: independent
#[tokio::test]
async fn beschreibung_systemtext_steht_im_top_level_feld_system() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.anthropic_base());
    let req = request(claude("mock-claude"), "K2 system", None);
    adapter.send(&req, None).await.expect("Antwort");
    let body = &mock.bodies_to("POST", MESSAGES)[0];
    assert_eq!(body["system"], json!(req.instructions));
    for message in body["messages"].as_array().expect("messages") {
        assert_ne!(
            message["role"], "system",
            "Systemtext nicht als Nachricht: {body}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// K3: stop_reason refusal → Refusal-Kennzeichen, kein Parse-Versuch

// spec: AI-002
// spec: AI-020
// acceptance: independent
#[tokio::test]
async fn k3_refusal_ergibt_kennzeichen_ohne_parse_versuch() {
    let mock = Mock::start().await;
    let (limiter, _) = new_limiter(1, 10);
    let adapter = adapter(&mock.anthropic_base());

    // Gemeinsamer Weg (AI-001): Status Refusal, nicht `Error: Invalid output` (der Refusal-Text ist kein JSON).
    let req = request(
        claude("mock-claude"),
        "K3 MOCK-SCENARIO:refusal a",
        Some(ok_schema()),
    );
    let outcome = provider::execute(PreviewSwitch::ON, &adapter, req).await;
    assert_eq!(outcome, Outcome::Status(AiStatus::Refusal));

    // Mit lokaler Validierung (AI-021): kein Reparaturversuch, genau 1 Request.
    let before = mock.requests_to("POST", MESSAGES).len();
    let req = request(
        claude("mock-claude"),
        "K3 MOCK-SCENARIO:refusal b",
        Some(ok_schema()),
    );
    let run = structured(&limiter, &adapter, &req).await;
    assert!(
        matches!(run.outcome, StructuredOutcome::Refusal),
        "erwartet Refusal: {:?}",
        run.outcome
    );
    assert_eq!(run.requests.len(), 1);
    assert_eq!(run.requests[0].validator_error, None);
    assert_eq!(mock.requests_to("POST", MESSAGES).len() - before, 1);
}

// ---------------------------------------------------------------------------------------------------------------
// K4: usage-Felder → einheitliche Felder (AI-001: fehlende Felder null, nicht 0)

// spec: AI-002
// spec: AI-001
// acceptance: independent
#[tokio::test]
async fn k4_usage_felder_werden_in_die_einheitlichen_felder_uebertragen() {
    let stub = Stub::start(|_, _| {
        StubResponse::json(
            200,
            message_body(
                "hi",
                json!({
                    "input_tokens": 100,
                    "output_tokens": 20,
                    "cache_read_input_tokens": 30,
                    "cache_creation_input_tokens": 40
                }),
            ),
        )
        .with_header("request-id", "req_k01")
    })
    .await;
    let reply = adapter(&stub.base())
        .send(&request(claude("mock-claude"), "K4", None), None)
        .await
        .expect("Antwort");
    assert_eq!(reply.usage.output_tokens, Some(20));
    assert_eq!(reply.usage.cached_input_tokens, Some(30));
    assert_eq!(reply.usage.cache_write_tokens, Some(40));
    // Ob `input_tokens` die Cache-Tokens einschließt, legt AI-002 nicht fest (Auslegungsfrage im Bericht); geprüft wird
    // nur, dass der Wert übertragen und nicht verkleinert wird.
    let input = reply.usage.input_tokens.expect("input_tokens übertragen");
    assert!(input >= 100, "input_tokens = {input}");
    // AI-001: gemeinsames Ergebnis mit Anbieter-Request-ID. Nachtrag 24.11.2026 (Q00-N2): Header `request-id`, nie die
    // Antwort-`id` (`msg_…`).
    assert_eq!(reply.provider_request_id.as_deref(), Some("req_k01"));
}

// spec: AI-002
// spec: AI-001
// acceptance: independent
#[tokio::test]
async fn k4_fehlende_usage_felder_ergeben_null_nicht_null_wert() {
    let stub = Stub::start(|_, _| {
        StubResponse::json(
            200,
            message_body("hi", json!({"input_tokens": 100, "output_tokens": 20})),
        )
    })
    .await;
    let reply = adapter(&stub.base())
        .send(&request(claude("mock-claude"), "K4 partial", None), None)
        .await
        .expect("Antwort");
    assert_eq!(reply.usage.input_tokens, Some(100));
    assert_eq!(reply.usage.output_tokens, Some(20));
    assert_eq!(reply.usage.cached_input_tokens, None);
    assert_eq!(reply.usage.cache_write_tokens, None);

    let mock = Mock::start().await;
    let reply = adapter(&mock.anthropic_base())
        .send(
            &request(
                claude("mock-claude"),
                "K4 MOCK-SCENARIO:missing-usage",
                None,
            ),
            None,
        )
        .await
        .expect("Antwort");
    assert_eq!(reply.usage.input_tokens, None);
    assert_eq!(reply.usage.output_tokens, None);
    assert_eq!(reply.usage.cached_input_tokens, None);
    assert_eq!(reply.usage.cache_write_tokens, None);
}

// ---------------------------------------------------------------------------------------------------------------
// K5: stop_reason max_tokens bei Schema-Aufgabe → kein Teilergebnis. Nachtrag 24.11.2026 (AI-002, Q00-H1) ersetzt
// „→ Error: Invalid output“ und K01-Auslegung 1 (Reparaturversuch): Abbruch an der Ausgabegrenze ohne Reparaturversuch,
// `Error Code = max_tokens`. Statuswert `Error: Output limit reached` folgt mit M4-K10 (bis dahin `Error: Invalid output`).

// spec: AI-002
// spec: AI-021
// spec: AI-002 [M4]
// acceptance: independent
#[tokio::test]
async fn k5_max_tokens_bei_schema_aufgabe_ohne_teilergebnis_und_ohne_reparatur() {
    let mock = Mock::start().await;
    let (limiter, _) = new_limiter(1, 10);
    let adapter = adapter(&mock.anthropic_base());
    // Der Mock liefert bei `max-tokens` einen Text, der für sich gültiges JSON wäre; er darf trotzdem nicht übernommen
    // werden.
    let req = request(
        claude("mock-claude"),
        "K5 MOCK-SCENARIO:max-tokens a",
        Some(ok_schema()),
    );
    let outcome = provider::execute(PreviewSwitch::ON, &adapter, req).await;
    assert_eq!(outcome, Outcome::Status(AiStatus::InvalidOutput));

    let req = request(
        claude("mock-claude"),
        "K5 MOCK-SCENARIO:max-tokens b",
        Some(ok_schema()),
    );
    let schema = ResponseSchema::new(req.response_schema.clone().unwrap()).unwrap();
    let (limiter, req, adapter) = (&*limiter, &req, &adapter);
    let run = run_with_repair(&schema, RepairAttempts::default(), |repair| async move {
        let exec = run_entry(limiter, "anthropic", &req.model.id, |_| {
            let repair = repair.as_ref();
            adapter.send(req, repair)
        })
        .await;
        match exec.result {
            Ok(reply) => reply
                .for_repair()
                .map(|(raw, _)| (raw, ()))
                .map_err(|limit| limit.reason),
            Err(error) => Err(format!("{error:?}")),
        }
    })
    .await;
    assert_eq!(
        run.outcome,
        StructuredOutcome::Failed("max_tokens".to_string())
    );
    let sent = mock
        .requests_to("POST", "/anthropic/v1/messages")
        .into_iter()
        .filter(|r| r.body.contains("K5 MOCK-SCENARIO:max-tokens b"))
        .count();
    assert_eq!(sent, 1, "kein Reparaturversuch");
}

// ---------------------------------------------------------------------------------------------------------------
// K6: HTTP 529 wird wie 503 nach INTEG-005 wiederholt

// spec: AI-002
// spec: INTEG-005
// acceptance: independent
#[tokio::test]
async fn k6_529_dauerhaft_ergibt_1_plus_5_requests_wie_503() {
    let mut runs = Vec::new();
    for code in [503u16, 529] {
        let stub = Stub::start(move |_, _| {
            StubResponse::json(
                code,
                json!({"type": "error", "error": {"type": "overloaded_error", "message": "busy"}}),
            )
        })
        .await;
        let (limiter, clock) = new_limiter(1, 100);
        let adapter = adapter(&stub.base());
        let exec = entry(
            &limiter,
            &adapter,
            &request(claude("mock-claude"), "K6", None),
        )
        .await;
        assert_eq!(stub.requests().len(), 6, "{code}: 1 + 5 Wiederholungen");
        assert_eq!(exec.attempts, 6);
        match status(&exec) {
            AiStatus::HttpError { code: c, .. } => assert_eq!(c, code),
            other => panic!("{code}: erwartet Error: {code} …, erhalten {other}"),
        }
        runs.push(clock.sleeps());
    }
    assert_eq!(runs[0], runs[1], "529 wartet wie 503 (Backoff INTEG-005)");
    assert_eq!(runs[0].len(), 5);
}

// spec: AI-002
// spec: INTEG-005
// acceptance: independent
#[tokio::test]
async fn k6_529_im_mock_wird_wiederholt_und_einmaliges_529_endet_mit_success() {
    let mock = Mock::start().await;
    let (limiter, _) = new_limiter(1, 100);
    let exec = entry(
        &limiter,
        &adapter(&mock.anthropic_base()),
        &request(claude("mock-claude"), "K6 MOCK-SCENARIO:529", None),
    )
    .await;
    assert_eq!(mock.requests_to("POST", MESSAGES).len(), 6);
    assert!(
        status(&exec).to_string().starts_with("Error: 529"),
        "{}",
        status(&exec)
    );

    let stub = Stub::start(|_, index| {
        if index == 0 {
            StubResponse::json(
                529,
                json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}),
            )
        } else {
            StubResponse::json(200, message_body("fine", json!({"input_tokens": 1, "output_tokens": 1})))
        }
    })
    .await;
    let (limiter, _) = new_limiter(1, 100);
    let exec = entry(
        &limiter,
        &adapter(&stub.base()),
        &request(claude("mock-claude"), "K6 once", None),
    )
    .await;
    assert_eq!(status(&exec), AiStatus::Success);
    assert_eq!(stub.requests().len(), 2);
}

// ---------------------------------------------------------------------------------------------------------------
// K7: Kontingent-Szenarien → genau 1 Request, alle weiteren Anthropic-Einträge Quota exceeded, andere Anbieter weiter

const QUOTA_SCENARIOS: [&str; 5] = [
    "insufficient-quota",
    "spend-limit-user",
    "spend-limit-workspace",
    "402-billing",
    "429-spend-cap",
];

// spec: AI-002
// spec: AI-020
// acceptance: independent
#[tokio::test]
async fn k7_kontingent_szenarien_genau_ein_request_und_alle_offenen_eintraege_quota_exceeded() {
    for scenario in QUOTA_SCENARIOS {
        let mock = Mock::start().await;
        let (limiter, _) = new_limiter(1, 100);
        let adapter = adapter(&mock.anthropic_base());
        let mut statuses = Vec::new();
        for i in 0..5 {
            // Nur der erste Eintrag löst das Szenario aus; die übrigen würden vom Mock mit `ok` beantwortet.
            let text = if i == 0 {
                format!("K7 MOCK-SCENARIO:{scenario} entry {i}")
            } else {
                format!("K7 entry {i}")
            };
            let exec = entry(
                &limiter,
                &adapter,
                &request(claude("mock-claude"), &text, None),
            )
            .await;
            statuses.push(status(&exec));
        }
        assert_eq!(
            mock.requests_to("POST", MESSAGES).len(),
            1,
            "{scenario}: genau 1 Request an Anthropic"
        );
        assert!(
            statuses.iter().all(|s| *s == AiStatus::QuotaExceeded),
            "{scenario}: {statuses:?}"
        );

        // Andere Anbieter laufen weiter (gleiche Begrenzung, gleicher Mock).
        let openrouter = os_ai::adapters::openrouter::OpenRouterAdapter::new(
            client(),
            os_ai::adapters::openrouter::OpenRouterConfig::new(ApiKey::new(OPENROUTER_TEST_KEY))
                .with_base_url(mock.openrouter_base()),
        );
        let req = request(
            model(
                "openrouter",
                "mock/text",
                "chat-completions",
                CapabilitySource::OpenRouterModels,
            ),
            "K7 other provider",
            None,
        );
        let exec = run_entry(&limiter, "openrouter", "mock/text", |_| {
            openrouter.send(&req, None)
        })
        .await;
        assert!(
            exec.result.is_ok(),
            "{scenario}: OpenRouter läuft weiter: {exec:?}"
        );
    }
}

// spec: AI-002
// acceptance: independent
#[tokio::test]
async fn k7_bei_n_gleichzeitigen_anfragen_hoechstens_n_requests() {
    let mock = Mock::start().await;
    mock.scenario("anthropic", "402-billing", 1000).await;
    let (limiter, _) = new_limiter(4, 100);
    let adapter = Arc::new(adapter(&mock.anthropic_base()));
    let mut tasks = tokio::task::JoinSet::new();
    for i in 0..12 {
        let adapter = adapter.clone();
        let limiter = limiter.clone();
        tasks.spawn(async move {
            let req = request(claude("mock-claude"), &format!("K7 parallel {i}"), None);
            let exec = entry(&limiter, &adapter, &req).await;
            status(&exec)
        });
    }
    let statuses = tasks.join_all().await;
    assert_eq!(statuses.len(), 12);
    assert!(
        statuses.iter().all(|s| *s == AiStatus::QuotaExceeded),
        "{statuses:?}"
    );
    let sent = mock.requests_to("POST", MESSAGES).len();
    assert!(
        (1..=4).contains(&sent),
        "höchstens 4 Requests, gesendet {sent}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// K8: 429-spend-cap nicht wiederholt; 429 ohne enforced_spend_limit_reached wiederholt

// spec: AI-002
// spec: INTEG-005
// acceptance: independent
#[tokio::test]
async fn k8_429_spend_cap_wird_nicht_wiederholt() {
    let mock = Mock::start().await;
    let (limiter, clock) = new_limiter(1, 100);
    let exec = entry(
        &limiter,
        &adapter(&mock.anthropic_base()),
        &request(
            claude("mock-claude"),
            "K8 MOCK-SCENARIO:429-spend-cap",
            None,
        ),
    )
    .await;
    assert_eq!(exec.attempts, 1);
    assert_eq!(mock.requests_to("POST", MESSAGES).len(), 1);
    assert!(
        clock.sleeps().is_empty(),
        "kein Backoff: {:?}",
        clock.sleeps()
    );
    assert_eq!(status(&exec), AiStatus::QuotaExceeded);
    assert!(!status(&exec).to_string().starts_with("Error: 429"));
}

// spec: AI-002
// spec: INTEG-005
// acceptance: independent
#[tokio::test]
async fn k8_429_ohne_spend_cap_wird_wiederholt_und_endet_mit_success() {
    let mock = Mock::start().await;
    let (limiter, clock) = new_limiter(1, 100);
    let exec = entry(
        &limiter,
        &adapter(&mock.anthropic_base()),
        &request(claude("mock-claude"), "K8 MOCK-SCENARIO:429x2", None),
    )
    .await;
    assert_eq!(status(&exec), AiStatus::Success);
    assert_eq!(mock.requests_to("POST", MESSAGES).len(), 3);
    assert_eq!(
        clock.sleeps().len(),
        2,
        "zwei Wartezeiten vor den Wiederholungen"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// K9: output_config.effort nur bei A und B, bei B nie max, bei C und D nie; nie Top-Level-`effort`

// spec: AI-002
// acceptance: independent
#[tokio::test]
async fn k9_effort_nur_bei_a_und_b_bei_b_nie_max_bei_c_und_d_nie() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.anthropic_base());
    let models = [
        ("mock-claude", "A"),
        ("mock-claude-b", "B"),
        ("mock-claude-c", "C"),
        ("mock-claude-d", "D"),
    ];
    let levels = ["low", "medium", "high", "max"];
    for (id, _) in models {
        for level in levels {
            let mut req = request(claude(id), &format!("K9 {id} {level}"), None);
            req.effort = Some(level.into());
            adapter.send(&req, None).await.expect("Antwort");
        }
    }
    let bodies = mock.bodies_to("POST", MESSAGES);
    assert_eq!(bodies.len(), models.len() * levels.len());
    for body in &bodies {
        assert!(
            body.get("effort").is_none(),
            "kein Top-Level-effort: {body}"
        );
        let id = body["model"].as_str().unwrap();
        let text = body.to_string();
        let level = levels
            .iter()
            .find(|l| text.contains(&format!("K9 {id} {l}")))
            .unwrap();
        let sent = body
            .get("output_config")
            .and_then(|c| c.get("effort"))
            .and_then(Value::as_str);
        let label = models.iter().find(|(m, _)| *m == id).unwrap().1;
        let expected = match (label, *level) {
            ("A", l) => Some(l),
            ("B", "max") => None,
            ("B", l) => Some(l),
            _ => None,
        };
        assert_eq!(sent, expected, "Modell {label}, Stufe {level}: {body}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// K10: C und D für Schema- und Bildaufgaben nicht wählbar (D: „Fähigkeiten unbekannt“); C im Batch synchron;
// geänderte capabilities gelten nach erneutem Laden

fn schema_task() -> Capabilities {
    Capabilities {
        text: true,
        structured_output: true,
        ..Capabilities::default()
    }
}

fn image_task() -> Capabilities {
    Capabilities {
        text: true,
        vision: true,
        ..Capabilities::default()
    }
}

fn find(models: &[AnthropicModel], id: &str) -> AnthropicModel {
    models
        .iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("Modell {id} fehlt"))
        .clone()
}

// spec: AI-002
// spec: AI-001
// acceptance: independent
#[tokio::test]
async fn k10_c_und_d_sind_fuer_schema_und_bildaufgaben_nicht_waehlbar() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.anthropic_base());
    let models = adapter.list_models().await.expect("Modellliste");
    let (a, c, d) = (
        find(&models, "mock-claude"),
        find(&models, "mock-claude-c"),
        find(&models, "mock-claude-d"),
    );
    for task in [schema_task(), image_task()] {
        assert_eq!(a.selectable_for(task), Selectability::Selectable);
        assert_eq!(
            c.selectable_for(task),
            Selectability::Unavailable {
                capabilities_unknown: false
            }
        );
        assert_eq!(
            d.selectable_for(task),
            Selectability::Unavailable {
                capabilities_unknown: true
            }
        );
    }
    // Fehlt `capabilities`, gilt jede Fähigkeit als nicht vorhanden (OS-Festlegung, konservativ).
    let caps = d.to_model().capabilities;
    assert!(
        !caps.structured_output && !caps.vision && !caps.batch,
        "{caps:?}"
    );
    // A meldet Structured Output, Vision und Batch laut capabilities (AI-001).
    let caps = a.to_model().capabilities;
    assert!(
        caps.text && caps.structured_output && caps.vision && caps.batch,
        "{caps:?}"
    );
}

// spec: AI-002
// spec: AI-001
// acceptance: independent
#[tokio::test]
async fn k10_schema_und_bildaufgaben_gehen_nie_an_c_oder_d() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.anthropic_base());
    let models = adapter.list_models().await.expect("Modellliste");
    for id in ["mock-claude-c", "mock-claude-d"] {
        let target = find(&models, id).to_model();
        let schema_req = request(target.clone(), "K10 schema", Some(ok_schema()));
        let outcome = provider::execute(PreviewSwitch::ON, &adapter, schema_req).await;
        assert_eq!(
            outcome,
            Outcome::Status(AiStatus::SkippedNoCapableModel),
            "{id}"
        );

        let png = std::fs::read(fixture("ai/site/images/forest.png")).unwrap();
        let mut image_req = request(target, "K10 image", None);
        image_req.input.push(image_input(&png).expect("PNG"));
        let outcome = provider::execute(PreviewSwitch::ON, &adapter, image_req).await;
        assert_eq!(
            outcome,
            Outcome::Status(AiStatus::SkippedNoCapableModel),
            "{id}"
        );
    }
    assert_eq!(mock.requests_to("POST", MESSAGES).len(), 0);
}

// spec: AI-002
// spec: AI-016
// acceptance: independent
#[tokio::test]
async fn k10_aufgaben_mit_c_laufen_im_batch_modus_synchron() {
    let mock = Mock::start().await;
    let models = adapter(&mock.anthropic_base())
        .list_models()
        .await
        .expect("Modellliste");
    assert!(!find(&models, "mock-claude-c").runs_in_batch());
    assert!(find(&models, "mock-claude").runs_in_batch());
}

// spec: AI-002
// acceptance: independent
#[tokio::test]
async fn k10_geaenderte_capabilities_gelten_nach_erneutem_laden() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.anthropic_base());
    let before = adapter.list_models().await.expect("Modellliste");
    assert_ne!(
        find(&before, "mock-claude-c").selectable_for(schema_task()),
        Selectability::Selectable
    );

    let all = |max: bool| {
        json!({
            "structured_outputs": {"supported": true},
            "image_input": {"supported": true},
            "batch": {"supported": true},
            "effort": {"supported": true, "low": {"supported": true}, "medium": {"supported": true},
                       "high": {"supported": true}, "max": {"supported": max}}
        })
    };
    mock.mock.set_models(
        "anthropic",
        json!({"data": [
            {"id": "mock-claude", "display_name": "A", "capabilities": all(true)},
            {"id": "mock-claude-b", "display_name": "B", "capabilities": all(true)},
            {"id": "mock-claude-c", "display_name": "C", "capabilities": all(false)},
            {"id": "mock-claude-d", "display_name": "D"}
        ], "has_more": false}),
    );
    let after = adapter.list_models().await.expect("Modellliste neu");
    assert_eq!(
        find(&after, "mock-claude-c").selectable_for(schema_task()),
        Selectability::Selectable
    );

    // B meldet jetzt auch `max`: nach dem Neuladen wird die Stufe gesendet.
    let mut req = request(claude("mock-claude-b"), "K10 reload max", None);
    req.effort = Some("max".into());
    adapter.send(&req, None).await.expect("Antwort");
    let bodies = mock.bodies_to("POST", MESSAGES);
    assert_eq!(bodies.last().unwrap()["output_config"]["effort"], "max");
}

// ---------------------------------------------------------------------------------------------------------------
// K11: Bilder als Base64 mit korrektem media_type (PNG, JPEG, WebP, GIF)

// spec: AI-002
// acceptance: independent
#[tokio::test]
async fn k11_bilder_werden_als_base64_mit_korrektem_media_type_gesendet() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.anthropic_base());
    let cases = [
        ("forest.png", "image/png"),
        ("trail.jpg", "image/jpeg"),
        ("boots.webp", "image/webp"),
        ("decoration.gif", "image/gif"),
    ];
    for (file, mime) in cases {
        let bytes = std::fs::read(fixture(&format!("ai/site/images/{file}"))).unwrap();
        let image = image_input(&bytes).unwrap_or_else(|| panic!("{file}: Bildformat erkannt"));
        match &image {
            Input::Image { media_type, .. } => assert_eq!(media_type, mime, "{file}"),
            other => panic!("{file}: {other:?}"),
        }
        let mut req = request(claude("mock-claude"), &format!("K11 {file}"), None);
        req.input.push(image);
        adapter.send(&req, None).await.expect("Antwort");

        let body = mock.bodies_to("POST", MESSAGES).pop().unwrap();
        let blocks = body["messages"][0]["content"].as_array().expect("content");
        let block = blocks
            .iter()
            .find(|b| b["type"] == "image")
            .unwrap_or_else(|| panic!("{file}: kein image-Block: {body}"));
        assert_eq!(block["source"]["type"], "base64", "{file}");
        assert_eq!(block["source"]["media_type"], mime, "{file}");
        let data = block["source"]["data"].as_str().expect("data");
        assert_eq!(
            base64_decode(data),
            bytes,
            "{file}: Base64 entspricht den Dateibytes"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// AI-017 (verwiesen aus AI-002): Spend-Cap-429 zählt nicht für den Schutzschalter

// spec: AI-017
// spec: AI-002
// acceptance: independent
#[tokio::test]
async fn ai017_spend_cap_429_zaehlt_nicht_fuer_den_schutzschalter() {
    // Schwelle 1: jeder gezählte endgültige Fehler öffnet den Schalter sofort.
    let mock = Mock::start().await;
    let (limiter, _) = new_limiter(1, 1);
    let exec = entry(
        &limiter,
        &adapter(&mock.anthropic_base()),
        &request(
            claude("mock-claude"),
            "AI017 MOCK-SCENARIO:429-spend-cap",
            None,
        ),
    )
    .await;
    assert_eq!(status(&exec), AiStatus::QuotaExceeded);
    assert!(!limiter.breaker_open("anthropic"), "Spend-Cap zählt nicht");

    // Gegenprobe: ein endgültiger 401 zählt.
    let (limiter, _) = new_limiter(1, 1);
    let exec = entry(
        &limiter,
        &adapter(&mock.anthropic_base()),
        &request(claude("mock-claude"), "AI017 MOCK-SCENARIO:401", None),
    )
    .await;
    assert!(exec.result.is_err());
    assert!(limiter.breaker_open("anthropic"), "Gegenprobe: 401 zählt");
}

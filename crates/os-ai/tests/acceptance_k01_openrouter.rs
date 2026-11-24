//! Unabhängige Abnahmetests M4-K01, Teil 2: Adapter OpenRouter (AI-005), allein aus `docs/spec/08-ai.md` abgeleitet
//! (dazu AI-001, AI-010, AI-017, AI-020, INTEG-005 und die Szenariotabelle des ai-mock). Jeder Test nennt sein
//! Akzeptanzkriterium als `Kn` (Reihenfolge der Kriterien in AI-005).
//!
//! Gegenstelle ist der ai-mock aus `os-testsite`; die Kontostands-Varianten aus AI-005 und Fehlerformen, die der Mock
//! nicht liefert, kommen von einem kleinen lokalen Testserver. Wartezeiten laufen auf einer Testuhr.
// acceptance: independent

mod acceptance_k01_support;

use std::sync::Arc;
use std::time::Duration;

use acceptance_k01_support::*;
use os_ai::adapters::openrouter::{
    format_micro_usd, resolve_rates, AdapterReply, ApiKey, OpenRouterAdapter, OpenRouterConfig,
    OpenRouterModel, Remaining, RoutingPreferences, Selectability,
};
use os_ai::cost::{PriceSource, Rates};
use os_ai::limits::{Execution, Limiter};
use os_ai::provider::{self, Capabilities, CapabilitySource, Model, Outcome, Request};
use os_ai::schema::{run_with_repair, RepairAttempts, ResponseSchema, StructuredOutcome};
use os_ai::{AiStatus, PreviewSwitch};
use serde_json::{json, Value};

const CHAT: &str = "/openrouter/api/v1/chat/completions";
const KEY: &str = "/openrouter/api/v1/key";

fn config(base: &str) -> OpenRouterConfig {
    OpenRouterConfig::new(ApiKey::new(OPENROUTER_TEST_KEY)).with_base_url(base)
}

fn adapter(base: &str) -> OpenRouterAdapter {
    OpenRouterAdapter::new(client(), config(base))
}

fn router_model(id: &str) -> Model {
    model(
        "openrouter",
        id,
        "chat-completions",
        CapabilitySource::OpenRouterModels,
    )
}

fn text_request(text: &str) -> Request {
    request(router_model("mock/text"), text, None)
}

fn schema_request(text: &str) -> Request {
    request(router_model("mock/text"), text, Some(ok_schema()))
}

async fn entry(
    limiter: &Limiter,
    adapter: &OpenRouterAdapter,
    request: &Request,
) -> Execution<AdapterReply> {
    run_entry(limiter, "openrouter", &request.model.id, |_| {
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

async fn structured(
    limiter: &Limiter,
    adapter: &OpenRouterAdapter,
    request: &Request,
) -> os_ai::schema::StructuredRun<(), os_ai::limits::FinalError> {
    let schema = ResponseSchema::new(request.response_schema.clone().unwrap()).unwrap();
    run_with_repair(&schema, RepairAttempts::default(), |repair| async move {
        let exec = run_entry(limiter, "openrouter", &request.model.id, |_| {
            adapter.send(request, repair.as_ref())
        })
        .await;
        exec.result.map(|reply| (reply.raw, ()))
    })
    .await
}

fn schema_task() -> Capabilities {
    Capabilities {
        text: true,
        structured_output: true,
        ..Capabilities::default()
    }
}

fn find(models: &[OpenRouterModel], id: &str) -> OpenRouterModel {
    models
        .iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("Modell {id} fehlt"))
        .clone()
}

/// Ob `needle` irgendwo (rekursiv) als Wert in `haystack` vorkommt.
fn contains_value(haystack: &Value, needle: &Value) -> bool {
    haystack == needle
        || match haystack {
            Value::Object(map) => map.values().any(|v| contains_value(v, needle)),
            Value::Array(items) => items.iter().any(|v| contains_value(v, needle)),
            _ => false,
        }
}

fn completion(text: &str) -> Value {
    json!({
        "id": "gen-k01",
        "object": "chat.completion",
        "model": "mock/text",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7}
    })
}

fn balance() -> Value {
    json!({"data": {"label": "k01", "limit": 10, "limit_remaining": 5.8, "limit_reset": "monthly", "usage": 42.0}})
}

// ---------------------------------------------------------------------------------------------------------------
// K1: Modellliste (8 Modelle), Preisumrechnung USD/Token → USD/1M, Gebühren je Request und Bild

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn k1_verbinden_laedt_8_modelle_und_rechnet_preise_um() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.openrouter_base());
    let models = adapter.list_models().await.expect("Modellliste");
    assert_eq!(models.len(), 8, "Mock-Modellliste mit 8 Modellen");

    // Mock: prompt "0.000001", completion "0.000005" je Token.
    let text = find(&models, "mock/text");
    assert_eq!(text.input_per_million(), "1.00");
    assert_eq!(text.output_per_million(), "5.00");
    // Vision-Modell mit pricing.request = "0.001" und pricing.image = "0.002".
    let vision = find(&models, "mock/vision");
    assert_eq!(vision.per_request(), "0.0010");
    assert_eq!(vision.per_image(), "0.0020");
}

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn k1_preis_0_000003_je_token_ergibt_3_00_je_million_fehlende_felder_null() {
    let mock = Mock::start().await;
    mock.mock.set_models(
        "openrouter",
        json!({"data": [{
            "id": "k01/priced",
            "name": "Priced",
            "context_length": 128000,
            "supported_parameters": ["response_format"],
            "pricing": {"prompt": "0.000003", "completion": "0.000015"}
        }]}),
    );
    let models = adapter(&mock.openrouter_base())
        .list_models()
        .await
        .expect("Modellliste");
    let priced = find(&models, "k01/priced");
    assert_eq!(priced.input_per_million(), "3.00");
    assert_eq!(priced.output_per_million(), "15.00");
    // Fehlende Felder = 0.
    assert_eq!(priced.per_request(), "0.0000");
    assert_eq!(priced.per_image(), "0.0000");
    assert_eq!(priced.context_length, Some(128_000));
}

// ---------------------------------------------------------------------------------------------------------------
// K2: Modelle ohne structured_outputs bzw. response_format sind für Schema-Aufgaben nicht wählbar

// spec: AI-005
// spec: AI-001
// acceptance: independent
#[tokio::test]
async fn k2_modelle_ohne_structured_outputs_oder_response_format_nicht_waehlbar() {
    let mock = Mock::start().await;
    let listed = |id: &str, params: Value| json!({"id": id, "supported_parameters": params, "pricing": {"prompt": "0", "completion": "0"}});
    mock.mock.set_models(
        "openrouter",
        json!({"data": [
            listed("k01/both", json!(["structured_outputs", "response_format", "temperature"])),
            listed("k01/structured-only", json!(["structured_outputs"])),
            listed("k01/format-only", json!(["response_format"])),
            listed("k01/neither", json!(["temperature", "tools"])),
            {"id": "k01/no-params", "pricing": {"prompt": "0", "completion": "0"}}
        ]}),
    );
    let models = adapter(&mock.openrouter_base())
        .list_models()
        .await
        .expect("Modellliste");
    for (id, selectable) in [
        ("k01/both", true),
        ("k01/structured-only", true),
        ("k01/format-only", true),
        ("k01/neither", false),
        ("k01/no-params", false),
    ] {
        let model = find(&models, id);
        assert_eq!(
            model.selectable_for(schema_task()) == Selectability::Selectable,
            selectable,
            "{id}"
        );
        assert_eq!(
            model.to_model().capabilities.structured_output,
            selectable,
            "{id}"
        );
    }
}

// spec: AI-005
// spec: AI-001
// acceptance: independent
#[tokio::test]
async fn k2_standardliste_embedding_modell_ist_fuer_schema_aufgaben_ausgegraut() {
    let mock = Mock::start().await;
    let models = adapter(&mock.openrouter_base())
        .list_models()
        .await
        .expect("Modellliste");
    assert!(matches!(
        find(&models, "mock/embedding").selectable_for(schema_task()),
        Selectability::Unavailable { .. }
    ));
    assert_eq!(
        find(&models, "mock/text").selectable_for(schema_task()),
        Selectability::Selectable
    );
    // AI-001: OpenRouter meldet Prices via API; Vision modellabhängig.
    let vision = find(&models, "mock/vision").to_model().capabilities;
    assert!(vision.vision && vision.prices_via_api, "{vision:?}");
    assert!(!find(&models, "mock/text").to_model().capabilities.vision);
}

// spec: AI-005
// spec: AI-001
// acceptance: independent
#[tokio::test]
async fn k2_schema_aufgabe_an_modell_ohne_structured_output_wird_nicht_gesendet() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.openrouter_base());
    let models = adapter.list_models().await.expect("Modellliste");
    let embedding = find(&models, "mock/embedding").to_model();
    let req = request(embedding, "K2 routing", Some(ok_schema()));
    let outcome = provider::execute(PreviewSwitch::ON, &adapter, req).await;
    assert_eq!(outcome, Outcome::Status(AiStatus::SkippedNoCapableModel));
    assert_eq!(mock.requests_to("POST", CHAT).len(), 0);
}

// ---------------------------------------------------------------------------------------------------------------
// K3: Schema-Aufgaben senden provider.require_parameters = true, Text-Aufgaben nicht

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn k3_require_parameters_nur_bei_schema_aufgaben() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock.openrouter_base());
    let outcome = provider::execute(PreviewSwitch::ON, &adapter, schema_request("K3 schema")).await;
    match outcome {
        Outcome::Completed(response) => assert_eq!(response.content, Some(json!({"ok": true}))),
        other => panic!("Schema-Aufgabe sollte gelingen: {other:?}"),
    }
    adapter
        .send(&text_request("K3 text"), None)
        .await
        .expect("Antwort");

    let bodies = mock.bodies_to("POST", CHAT);
    assert_eq!(bodies.len(), 2);
    let schema_body = &bodies[0];
    assert_eq!(
        schema_body["provider"]["require_parameters"], true,
        "{schema_body}"
    );
    assert_eq!(
        schema_body["response_format"]["type"], "json_schema",
        "{schema_body}"
    );
    assert!(
        contains_value(&schema_body["response_format"], &ok_schema()),
        "Schema in response_format: {schema_body}"
    );
    let text_body = &bodies[1];
    assert!(
        text_body
            .get("provider")
            .and_then(|p| p.get("require_parameters"))
            .is_none(),
        "Text-Aufgabe ohne require_parameters: {text_body}"
    );
    assert!(text_body.get("response_format").is_none(), "{text_body}");
}

// ---------------------------------------------------------------------------------------------------------------
// K4: „Anbieter mit Datensammlung ausschließen“ (Standard an) → provider.data_collection = "deny" in jedem Request

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn k4_datensammlung_ausschliessen_ist_standard_und_steht_in_jedem_request() {
    assert!(
        RoutingPreferences::default().deny_data_collection,
        "Standard an"
    );
    let mock = Mock::start().await;
    let adapter = adapter(&mock.openrouter_base());
    adapter
        .send(&schema_request("K4 a"), None)
        .await
        .expect("Antwort");
    adapter
        .send(&text_request("K4 b"), None)
        .await
        .expect("Antwort");
    let bodies = mock.bodies_to("POST", CHAT);
    assert_eq!(bodies.len(), 2);
    for body in &bodies {
        assert_eq!(body["provider"]["data_collection"], "deny", "{body}");
    }
}

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn beschreibung_routing_praeferenzen_und_app_kennung() {
    let mock = Mock::start().await;
    // Standard: kein ZDR, keine feste Reihenfolge, keine App-Kennung.
    adapter(&mock.openrouter_base())
        .send(&text_request("routing default"), None)
        .await
        .expect("Antwort");
    let default_body = mock.bodies_to("POST", CHAT).pop().unwrap();
    assert_ne!(default_body["provider"]["zdr"], true, "{default_body}");
    assert!(
        default_body["provider"].get("order").is_none(),
        "{default_body}"
    );
    let record = mock.requests_to("POST", CHAT).pop().unwrap();
    for header in ["http-referer", "x-title"] {
        assert!(
            !record.headers.contains_key(header),
            "App-Kennung standardmäßig aus: {header}"
        );
    }

    // ZDR an und feste Reihenfolge → zdr: true, order, allow_fallbacks: false.
    let mut cfg = config(&mock.openrouter_base());
    cfg.routing = RoutingPreferences {
        deny_data_collection: true,
        zero_data_retention: true,
        order: vec!["provider-a".into(), "provider-b".into()],
    };
    OpenRouterAdapter::new(client(), cfg)
        .send(&text_request("routing fixed"), None)
        .await
        .expect("Antwort");
    let body = mock.bodies_to("POST", CHAT).pop().unwrap();
    assert_eq!(body["provider"]["zdr"], true, "{body}");
    assert_eq!(
        body["provider"]["order"],
        json!(["provider-a", "provider-b"]),
        "{body}"
    );
    assert_eq!(body["provider"]["allow_fallbacks"], false, "{body}");
    assert_eq!(body["provider"]["data_collection"], "deny", "{body}");
}

// ---------------------------------------------------------------------------------------------------------------
// K5: 402-in-flight → wartet mindestens 3 s (Testuhr), erneut gesendet, Success; kein anderer Eintrag Quota exceeded

// spec: AI-005
// spec: INTEG-005
// acceptance: independent
#[tokio::test]
async fn k5_402_in_flight_wartet_mindestens_3_s_und_endet_mit_success() {
    let mock = Mock::start().await;
    let (limiter, _) = new_limiter(1, 100);
    let adapter = adapter(&mock.openrouter_base());
    let exec = entry(
        &limiter,
        &adapter,
        &text_request("K5 MOCK-SCENARIO:402-in-flight a"),
    )
    .await;
    assert_eq!(status(&exec), AiStatus::Success, "{exec:?}");
    assert_eq!(exec.attempts, 2);
    assert_eq!(mock.requests_to("POST", CHAT).len(), 2);
    let gap = exec.attempt_starts[1] - exec.attempt_starts[0];
    assert!(gap >= Duration::from_secs(3), "Wartezeit {gap:?} < 3 s");
    assert!(!adapter.quota().is_tripped());

    for i in 0..3 {
        let exec = entry(&limiter, &adapter, &text_request(&format!("K5 next {i}"))).await;
        assert_eq!(status(&exec), AiStatus::Success, "Eintrag {i}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// K6: 402-credits, 402-key-limit, 403-guardrail → alle offenen OpenRouter-Einträge Quota exceeded, keine
// Wiederholung, Kontostand neu geladen

// spec: AI-005
// spec: AI-020
// acceptance: independent
#[tokio::test]
async fn k6_endgueltige_kontingentfehler_beenden_alle_offenen_eintraege() {
    for scenario in ["402-credits", "402-key-limit", "403-guardrail"] {
        let mock = Mock::start().await;
        let (limiter, clock) = new_limiter(1, 100);
        let adapter = adapter(&mock.openrouter_base());
        let mut statuses = Vec::new();
        for i in 0..4 {
            let text = if i == 0 {
                format!("K6 MOCK-SCENARIO:{scenario} entry {i}")
            } else {
                format!("K6 entry {i}")
            };
            let exec = entry(&limiter, &adapter, &text_request(&text)).await;
            if i == 0 {
                assert_eq!(exec.attempts, 1, "{scenario}: keine Wiederholung");
            }
            statuses.push(status(&exec));
        }
        assert!(
            statuses.iter().all(|s| *s == AiStatus::QuotaExceeded),
            "{scenario}: {statuses:?}"
        );
        assert_eq!(
            mock.requests_to("POST", CHAT).len(),
            1,
            "{scenario}: genau 1 Request"
        );
        assert!(clock.sleeps().is_empty(), "{scenario}: kein Backoff");

        // Kontostandsanzeige neu geladen: GET /key nach dem auslösenden Request.
        let log = mock.log();
        let chat = log
            .iter()
            .position(|r| r.method == "POST" && r.path == CHAT)
            .unwrap();
        assert!(
            log.iter()
                .skip(chat + 1)
                .any(|r| r.method == "GET" && r.path == KEY),
            "{scenario}: Kontostand nicht neu geladen"
        );
        assert!(adapter.last_balance().is_some(), "{scenario}");
    }
}

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn k6_bei_n_gleichzeitigen_anfragen_hoechstens_n_requests() {
    let mock = Mock::start().await;
    mock.scenario("openrouter", "402-credits", 1000).await;
    let (limiter, _) = new_limiter(4, 100);
    let adapter = Arc::new(adapter(&mock.openrouter_base()));
    let mut tasks = tokio::task::JoinSet::new();
    for i in 0..12 {
        let adapter = adapter.clone();
        let limiter = limiter.clone();
        tasks.spawn(async move {
            let exec = entry(
                &limiter,
                &adapter,
                &text_request(&format!("K6 parallel {i}")),
            )
            .await;
            status(&exec)
        });
    }
    let statuses = tasks.join_all().await;
    assert!(
        statuses.iter().all(|s| *s == AiStatus::QuotaExceeded),
        "{statuses:?}"
    );
    let sent = mock.requests_to("POST", CHAT).len();
    assert!(
        (1..=4).contains(&sent),
        "höchstens 4 Requests, gesendet {sent}"
    );
}

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn beschreibung_402_ohne_limit_source_und_403_budgetquellen_sind_endgueltig() {
    let cases: [(u16, Value); 4] = [
        (
            402,
            json!({"error": {"code": 402, "message": "Payment required"}}),
        ),
        (
            403,
            json!({"error": {"code": 403, "message": "Workspace budget", "metadata": {"limit_source": "openrouter_workspace_budget"}}}),
        ),
        (
            403,
            json!({"error": {"code": 403, "message": "Key limit", "metadata": {"limit_source": "openrouter_key_limit"}}}),
        ),
        (
            403,
            json!({"error": {"code": 403, "message": "Guardrail", "metadata": {"limit_source": "openrouter_guardrail_budget"}}}),
        ),
    ];
    for (code, error) in cases {
        let error_body = error.clone();
        let stub = Stub::start(move |req, _| {
            if req.path.ends_with("/key") {
                StubResponse::json(200, balance())
            } else {
                StubResponse::json(code, error_body.clone())
            }
        })
        .await;
        let (limiter, _) = new_limiter(1, 100);
        let adapter = adapter(&format!("{}/api/v1", stub.base()));
        let mut statuses = Vec::new();
        for i in 0..3 {
            let exec = entry(&limiter, &adapter, &text_request(&format!("final {i}"))).await;
            statuses.push(status(&exec));
        }
        assert!(
            statuses.iter().all(|s| *s == AiStatus::QuotaExceeded),
            "{code} {error}: {statuses:?}"
        );
        let chats = stub
            .requests()
            .iter()
            .filter(|r| r.path.ends_with("/chat/completions"))
            .count();
        assert_eq!(chats, 1, "{code} {error}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// K7: 402-weight → nur dieser Eintrag Quota exceeded

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn k7_402_weight_betrifft_nur_diesen_eintrag() {
    let mock = Mock::start().await;
    let (limiter, _) = new_limiter(1, 100);
    let adapter = adapter(&mock.openrouter_base());
    let first = entry(
        &limiter,
        &adapter,
        &text_request("K7 MOCK-SCENARIO:402-weight big"),
    )
    .await;
    assert_eq!(status(&first), AiStatus::QuotaExceeded);
    assert_eq!(first.attempts, 1);
    for i in 0..3 {
        let exec = entry(&limiter, &adapter, &text_request(&format!("K7 small {i}"))).await;
        assert_eq!(status(&exec), AiStatus::Success, "Eintrag {i}");
    }
    assert_eq!(mock.requests_to("POST", CHAT).len(), 4);
}

// ---------------------------------------------------------------------------------------------------------------
// K8: Kontostand – limit_remaining direkt; null = unbegrenzt

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn k8_kontostand_zeigt_limit_remaining_direkt() {
    let stub = Stub::start(|_, _| {
        StubResponse::json(
            200,
            json!({"data": {
                "label": "k01", "limit": 10, "limit_remaining": 5.80, "limit_reset": "monthly",
                "usage": 42.00, "usage_daily": 1.5, "usage_weekly": 3.25, "usage_monthly": 7.0
            }}),
        )
    })
    .await;
    let adapter = adapter(&format!("{}/api/v1", stub.base()));
    let balance = adapter.key_balance().await.expect("Kontostand");
    assert_eq!(
        balance.remaining(),
        Remaining::MicroUsd(5_800_000),
        "nicht 10 − 42"
    );
    assert_eq!(format_micro_usd(5_800_000, 2), "5.80");
    assert_eq!(balance.limit_reset.as_deref(), Some("monthly"));
    assert_eq!(balance.limit, Some(10_000_000));
    assert_eq!(balance.usage, Some(42_000_000));
    assert_eq!(balance.usage_daily, Some(1_500_000));
    assert_eq!(balance.usage_weekly, Some(3_250_000));
    assert_eq!(balance.usage_monthly, Some(7_000_000));
    let requests = stub.requests();
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/api/v1/key");
    assert_eq!(
        requests[0].header("authorization"),
        Some(format!("Bearer {OPENROUTER_TEST_KEY}").as_str())
    );
}

// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn k8_kontostand_ohne_limit_ist_unbegrenzt() {
    for data in [
        json!({"limit": null, "limit_remaining": null, "usage": 42.0}),
        json!({"limit": null, "limit_remaining": 5.0, "usage": 1.0}),
        json!({"limit": 10, "limit_remaining": null, "usage": 1.0}),
    ] {
        let body = json!({ "data": data.clone() });
        let stub = Stub::start(move |_, _| StubResponse::json(200, body.clone())).await;
        let balance = adapter(&format!("{}/api/v1", stub.base()))
            .key_balance()
            .await
            .expect("Kontostand");
        assert_eq!(balance.remaining(), Remaining::Unlimited, "{data}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// K9: Preise aus der API haben Vorrang vor der Preistabelle; Preisquelle `api`

// spec: AI-005
// spec: AI-010
// acceptance: independent
#[tokio::test]
async fn k9_api_preise_haben_vorrang_und_preisquelle_ist_api() {
    let mock = Mock::start().await;
    let models = adapter(&mock.openrouter_base())
        .list_models()
        .await
        .expect("Modellliste");
    let api = find(&models, "mock/text")
        .rates()
        .expect("Preise aus der API");
    assert_eq!(api.source, PriceSource::Api);
    assert_eq!(api.source.as_str(), "api");
    let table = Rates::per_million("9.00", "9.00", PriceSource::Table).unwrap();
    let chosen = resolve_rates(Some(api.clone()), Some(table.clone())).unwrap();
    assert_eq!(chosen, api);
    assert_eq!(chosen.source.as_str(), "api");
    // Ohne API-Preis greift die Tabelle.
    assert_eq!(resolve_rates(None, Some(table.clone())), Some(table));
}

// ---------------------------------------------------------------------------------------------------------------
// K10: refusal (Form a) → Refusal, Grund = Refusal-Text, kein Parse-Versuch; refusal-filter-only → content_filter

// spec: AI-005
// spec: AI-020
// acceptance: independent
#[tokio::test]
async fn k10_refusal_form_a_mit_text_und_nur_filter() {
    for (scenario, reason) in [
        ("refusal", "Fixture refusal"),
        ("refusal-filter-only", "content_filter"),
    ] {
        let mock = Mock::start().await;
        let (limiter, _) = new_limiter(1, 100);
        let adapter = adapter(&mock.openrouter_base());

        let exec = entry(
            &limiter,
            &adapter,
            &schema_request(&format!("K10 MOCK-SCENARIO:{scenario} a")),
        )
        .await;
        assert_eq!(status(&exec), AiStatus::Refusal, "{scenario}");
        let reply = exec.result.as_ref().unwrap();
        assert_eq!(reply.refusal_reason.as_deref(), Some(reason), "{scenario}");
        assert_eq!(exec.attempts, 1);

        // Kein Parse-Versuch: lokale Validierung meldet Refusal, kein Reparaturversuch.
        let run = structured(
            &limiter,
            &adapter,
            &schema_request(&format!("K10 MOCK-SCENARIO:{scenario} b")),
        )
        .await;
        assert!(
            matches!(run.outcome, StructuredOutcome::Refusal),
            "{scenario}: {:?}",
            run.outcome
        );
        assert_eq!(run.requests.len(), 1, "{scenario}");

        let outcome = provider::execute(
            PreviewSwitch::ON,
            &adapter,
            schema_request(&format!("K10 MOCK-SCENARIO:{scenario} c")),
        )
        .await;
        assert_eq!(outcome, Outcome::Status(AiStatus::Refusal), "{scenario}");
        assert_eq!(
            mock.requests_to("POST", CHAT).len(),
            3,
            "{scenario}: je Eintrag genau 1 Request"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// K11: refusal-403 (Form b) → Refusal, keine Wiederholung, übrige Einträge Success, Schutzschalter zählt nicht

// spec: AI-005
// spec: AI-017
// acceptance: independent
#[tokio::test]
async fn k11_refusal_403_ist_refusal_und_kein_fehler() {
    let mock = Mock::start().await;
    // Schwelle 1: jeder gezählte endgültige Fehler würde den Schalter sofort öffnen.
    let (limiter, clock) = new_limiter(1, 1);
    let adapter = adapter(&mock.openrouter_base());
    let exec = entry(
        &limiter,
        &adapter,
        &schema_request("K11 MOCK-SCENARIO:refusal-403 a"),
    )
    .await;
    let st = status(&exec);
    assert_eq!(
        st,
        AiStatus::Refusal,
        "nicht Error: 403 …, nicht Quota exceeded"
    );
    assert_eq!(exec.attempts, 1);
    assert!(clock.sleeps().is_empty(), "kein Backoff");
    assert_eq!(mock.requests_to("POST", CHAT).len(), 1);
    assert!(!adapter.quota().is_tripped(), "kein Quota exceeded");
    assert!(
        !limiter.breaker_open("openrouter"),
        "Schutzschalter zählt nicht"
    );

    let outcome = provider::execute(
        PreviewSwitch::ON,
        &adapter,
        schema_request("K11 MOCK-SCENARIO:refusal-403 b"),
    )
    .await;
    assert_eq!(outcome, Outcome::Status(AiStatus::Refusal));

    for i in 0..3 {
        let exec = entry(
            &limiter,
            &adapter,
            &schema_request(&format!("K11 next {i}")),
        )
        .await;
        assert_eq!(status(&exec), AiStatus::Success, "Eintrag {i}");
    }
    assert_eq!(mock.requests_to("POST", CHAT).len(), 5);
}

// ---------------------------------------------------------------------------------------------------------------
// AI-017 (verwiesen aus AI-005): endgültiges OpenRouter-402 zählt nicht für den Schutzschalter

// spec: AI-017
// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn ai017_endgueltiges_402_zaehlt_nicht_fuer_den_schutzschalter() {
    let mock = Mock::start().await;
    let (limiter, _) = new_limiter(1, 1);
    let exec = entry(
        &limiter,
        &adapter(&mock.openrouter_base()),
        &text_request("AI017 MOCK-SCENARIO:402-credits"),
    )
    .await;
    assert_eq!(status(&exec), AiStatus::QuotaExceeded);
    assert!(!limiter.breaker_open("openrouter"));
}

// ---------------------------------------------------------------------------------------------------------------
// AI-001 (verwiesen): usage in einheitlicher Form, fehlende Felder null

// spec: AI-001
// spec: AI-005
// acceptance: independent
#[tokio::test]
async fn ai001_usage_einheitlich_fehlende_cache_felder_sind_null() {
    let stub = Stub::start(|_, _| StubResponse::json(200, completion("hello"))).await;
    let reply = adapter(&format!("{}/api/v1", stub.base()))
        .send(&text_request("usage"), None)
        .await
        .expect("Antwort");
    assert_eq!(reply.usage.input_tokens, Some(5));
    assert_eq!(reply.usage.output_tokens, Some(2));
    assert_eq!(reply.usage.cached_input_tokens, None);
    assert_eq!(reply.usage.cache_write_tokens, None);
    assert_eq!(reply.provider_request_id.as_deref(), Some("gen-k01"));
    let requests = stub.requests();
    assert_eq!(requests[0].path, "/api/v1/chat/completions");
    assert_eq!(
        requests[0].header("authorization"),
        Some(format!("Bearer {OPENROUTER_TEST_KEY}").as_str())
    );
}

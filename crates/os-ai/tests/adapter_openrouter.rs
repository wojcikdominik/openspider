//! Adapter OpenRouter (AI-005) gegen ai-mock (M4-K01). Szenarien aus 08 „Konventionen“; die Modellliste laut
//! Testfixture von AI-005 (3 ohne Structured Output, 2 mit Vision, 1 mit `pricing.request`/`pricing.image`) wird über
//! `AiMock::set_models` eingespielt, weil die eingebaute Mock-Liste davon abweicht (siehe Bericht K01).
#[path = "adapter_anthropic_support.rs"]
mod support;

use std::sync::Arc;
use std::time::Duration;

use os_ai::adapters::anthropic::{image_input, AdapterReply, AnthropicAdapter};
use os_ai::adapters::openrouter::{
    format_micro_usd, resolve_rates, usage_from_json, AppIdentity, KeyBalance, OpenRouterAdapter,
    OpenRouterModel, Remaining, Selectability,
};
use os_ai::cost::log::{request_record, LogContext, RequestOutcome};
use os_ai::cost::{PriceSource, Rates};
use os_ai::limits::{Execution, Limiter, RequestSpec};
use os_ai::provider::{AiProvider, Capabilities, CapabilitySource, Input, Request};
use os_ai::schema::RawAnswer;
use os_ai::AiStatus;
use os_model::budget::MicroUsd;
use serde_json::{json, Value};
use support::*;

const CHAT: &str = "/openrouter/api/v1/chat/completions";
const KEY_PATH: &str = "/openrouter/api/v1/key";

fn adapter(mock: &Mock) -> OpenRouterAdapter {
    OpenRouterAdapter::new(reqwest::Client::new(), mock.openrouter_config())
}

async fn run(
    limiter: &Limiter,
    adapter: &OpenRouterAdapter,
    request: &Request,
) -> Execution<AdapterReply> {
    limiter
        .run(
            &RequestSpec::new("openrouter", request.model.id.clone(), 100),
            |_| adapter.send(request, None),
        )
        .await
}

/// Modellliste laut Testfixture AI-005: 8 Modelle, davon 3 ohne Structured Output, 2 mit Vision, 1 mit
/// `pricing.request`/`pricing.image`.
fn spec_catalog() -> Value {
    let entry = |id: &str, params: &[&str], vision: bool, pricing: Value| {
        let modalities = if vision {
            json!(["text", "image"])
        } else {
            json!(["text"])
        };
        json!({
            "id": id, "name": id, "context_length": 128000,
            "architecture": {"input_modalities": modalities, "output_modalities": ["text"]},
            "supported_parameters": params,
            "pricing": pricing,
        })
    };
    let so = ["response_format", "structured_outputs", "temperature"];
    let plain = ["temperature", "max_tokens"];
    json!({"data": [
        entry("spec/a", &so, false, json!({"prompt": "0.000003", "completion": "0.000015"})),
        entry("spec/b", &["structured_outputs"], false, json!({"prompt": "0.000001", "completion": "0.000002"})),
        entry("spec/c", &["response_format"], false, json!({"prompt": "0.0000005", "completion": "0.0000015"})),
        entry("spec/d", &so, false, json!({"prompt": "0", "completion": "0"})),
        entry("spec/vision-priced", &so, true, json!({"prompt": "0.000001", "completion": "0.000005", "request": "0.001", "image": "0.002"})),
        entry("spec/vision-plain", &plain, true, json!({"prompt": "0.000001", "completion": "0.000005"})),
        entry("spec/plain-1", &plain, false, json!({"prompt": "0.000001", "completion": "0.000001"})),
        entry("spec/plain-2", &[], false, json!({})),
    ]})
}

fn schema_task() -> Capabilities {
    Capabilities {
        text: true,
        structured_output: true,
        ..Capabilities::default()
    }
}

// spec: AI-005
// Verbinden lädt die Mock-Modellliste (8 Modelle); Tokenpreise USD je Token → USD je 1 Mio. (`"0.000003"` →
// `3.00`); `pricing.request = "0.001"` → `Per Request $ 0.0010`, `pricing.image = "0.002"` → `Per Image $ 0.0020`.
#[tokio::test]
async fn connecting_loads_the_model_list_and_converts_prices() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let builtin = adapter.list_models().await.unwrap();
    assert_eq!(builtin.len(), 8);
    let vision = builtin.iter().find(|m| m.id == "mock/vision").unwrap();
    assert_eq!(vision.per_request(), "0.0010");
    assert_eq!(vision.per_image(), "0.0020");
    assert_eq!(vision.input_per_million(), "1.00");
    assert_eq!(vision.output_per_million(), "5.00");

    mock.mock.set_models("openrouter", spec_catalog());
    let models = adapter.list_models().await.unwrap();
    assert_eq!(models.len(), 8);
    let a = &models[0];
    assert_eq!(a.input_per_million(), "3.00");
    assert_eq!(a.output_per_million(), "15.00");
    assert_eq!(a.per_request(), "0.0000");
    assert_eq!(a.context_length, Some(128_000));
    assert_eq!(models[2].input_per_million(), "0.50");
    let priced = models
        .iter()
        .find(|m| m.id == "spec/vision-priced")
        .unwrap();
    assert_eq!(priced.per_request(), "0.0010");
    assert_eq!(priced.per_image(), "0.0020");
    // Fehlende Preisfelder = 0.
    let empty = models.iter().find(|m| m.id == "spec/plain-2").unwrap();
    assert_eq!((empty.prompt_pico, empty.request_pico), (0, 0));
    assert!(empty.prices_known);
    let generic = adapter.models().await.unwrap();
    assert_eq!(generic.len(), 8);
    assert!(generic.iter().all(|m| m.endpoint == "chat-completions"
        && m.capability_source == CapabilitySource::OpenRouterModels
        && m.capabilities.prices_via_api));
    assert_eq!(mock.requests_to("/openrouter/api/v1/models").len(), 3);
}

// spec: AI-005
// spec: AI-001
// Modelle ohne `structured_outputs` bzw. `response_format` in `supported_parameters` sind für Schema-Aufgaben nicht
// wählbar; Vision modellabhängig.
#[tokio::test]
async fn models_without_structured_output_are_not_selectable_for_schema_tasks() {
    let mock = Mock::start().await;
    mock.mock.set_models("openrouter", spec_catalog());
    let models = adapter(&mock).list_models().await.unwrap();
    let unavailable: Vec<_> = models
        .iter()
        .filter(|m| m.selectable_for(schema_task()) != Selectability::Selectable)
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(
        unavailable,
        ["spec/vision-plain", "spec/plain-1", "spec/plain-2"]
    );
    assert!(models.iter().all(|m| matches!(
        m.selectable_for(schema_task()),
        Selectability::Selectable
            | Selectability::Unavailable {
                capabilities_unknown: false
            }
    )));
    let vision: Vec<_> = models
        .iter()
        .filter(|m| m.vision())
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(vision, ["spec/vision-priced", "spec/vision-plain"]);
    let builtin = OpenRouterModel::from_json(&json!({
        "id": "mock/embedding",
        "architecture": {"input_modalities": ["text"], "output_modalities": ["embeddings"]},
        "supported_parameters": []
    }))
    .unwrap();
    assert!(builtin.embeddings() && !builtin.text() && !builtin.structured_output());
}

// spec: AI-005
// Schema-Aufgaben: `response_format` mit `type: "json_schema"` und `provider.require_parameters = true`; Text-Aufgaben
// ohne Schema enthalten es nicht. Standard „Anbieter mit Datensammlung ausschließen“ → `provider.data_collection =
// "deny"` in jedem Request; App-Kennung standardmäßig aus.
#[tokio::test]
async fn schema_requests_require_parameters_and_every_request_denies_data_collection() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let response = adapter
        .execute(schema_request(
            openrouter_model(),
            "MOCK-ID:k01-structured",
            label_schema(),
        ))
        .await
        .unwrap();
    assert_eq!(response.content, Some(json!({"ok": true, "label": "k01"})));
    adapter
        .execute(text_request(openrouter_model(), "MOCK-ID:home Text"))
        .await
        .unwrap();
    let bodies = mock.bodies(CHAT);
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0]["provider"]["require_parameters"], true);
    assert_eq!(bodies[0]["response_format"]["type"], "json_schema");
    assert_eq!(bodies[0]["response_format"]["json_schema"]["strict"], true);
    assert_eq!(
        bodies[0]["response_format"]["json_schema"]["schema"],
        label_schema()
    );
    assert!(bodies[1]["provider"].get("require_parameters").is_none());
    assert!(bodies[1].get("response_format").is_none());
    for body in &bodies {
        assert_eq!(body["provider"]["data_collection"], "deny");
        assert!(body["provider"].get("zdr").is_none());
        assert!(body["provider"].get("order").is_none());
        assert_eq!(body["messages"][0]["role"], "system");
    }
    for record in mock.requests_to(CHAT) {
        assert!(!record.headers.contains_key("http-referer"));
        assert!(!record.headers.contains_key("x-title"));
        assert!(record.headers.contains_key("authorization"));
        assert!(!record.body.contains(KEY));
    }
}

// spec: AI-005
// Routing-Präferenzen: Zero Data Retention → `zdr: true`; feste Reihenfolge → `order` und `allow_fallbacks: false`;
// Datensammlung erlaubt → kein `data_collection`; App-Kennung an → `HTTP-Referer`, `X-Title`.
#[tokio::test]
async fn routing_preferences_and_app_identity_are_sent_when_configured() {
    let mock = Mock::start().await;
    let mut config = mock.openrouter_config();
    config.routing.deny_data_collection = false;
    config.routing.zero_data_retention = true;
    config.routing.order = vec!["provider-x".into(), "provider-y".into()];
    config.app_identity = Some(AppIdentity {
        referer: "https://openspider.test".into(),
        title: "OpenSpider".into(),
    });
    let adapter = OpenRouterAdapter::new(reqwest::Client::new(), config);
    adapter
        .send(&text_request(openrouter_model(), "MOCK-ID:home"), None)
        .await
        .unwrap();
    let body = &mock.bodies(CHAT)[0];
    assert_eq!(
        body["provider"],
        json!({"zdr": true, "order": ["provider-x", "provider-y"], "allow_fallbacks": false})
    );
    let record = &mock.requests_to(CHAT)[0];
    assert_eq!(record.headers["http-referer"], "https://openspider.test");
    assert_eq!(record.headers["x-title"], "OpenSpider");
}

// spec: AI-005
// spec: AI-020
// `402-in-flight` → Eintrag wartet mindestens 3 s (Testuhr), wird erneut gesendet und endet mit `Success`; kein anderer
// Eintrag erhält `Quota exceeded`.
#[tokio::test]
async fn in_flight_402_waits_for_retry_after_and_succeeds() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = limiter(LONG_TIMEOUT, 1);
    let waiting = run(
        &limiter,
        &adapter,
        &text_request(openrouter_model(), "MOCK-SCENARIO:402-in-flight"),
    )
    .await;
    assert_eq!(waiting.attempts, 2);
    assert!(waiting.attempt_starts[1] - waiting.attempt_starts[0] >= Duration::from_secs(3));
    assert_eq!(status(&waiting, None), AiStatus::Success);
    assert!(!adapter.quota().is_tripped());
    let other = run(
        &limiter,
        &adapter,
        &text_request(openrouter_model(), "MOCK-ID:home"),
    )
    .await;
    assert_eq!(status(&other, None), AiStatus::Success);
    assert_eq!(mock.requests_to(CHAT).len(), 3);
    assert!(mock.requests_to(KEY_PATH).is_empty());
}

async fn run_entries(
    limiter: Arc<Limiter>,
    adapter: Arc<OpenRouterAdapter>,
    texts: Vec<String>,
) -> Vec<(String, Execution<AdapterReply>)> {
    let mut set = tokio::task::JoinSet::new();
    for text in texts {
        let limiter = limiter.clone();
        let adapter = adapter.clone();
        set.spawn(async move {
            let request = text_request(openrouter_model(), &text);
            let execution = run(&limiter, &adapter, &request).await;
            (text, execution)
        });
    }
    let mut out = Vec::new();
    while let Some(result) = set.join_next().await {
        out.push(result.unwrap());
    }
    out
}

// spec: AI-005
// spec: AI-020
// `402-credits`, `402-key-limit`, `403-guardrail` → betroffener Eintrag und alle offenen OpenRouter-Einträge `Quota
// exceeded`, keine Wiederholung (genau 1 Request bei „gleichzeitige Anfragen = 1“, bei n gleichzeitigen höchstens n);
// die Kontostandsanzeige wird neu geladen; andere Anbieter laufen weiter.
#[tokio::test]
async fn final_quota_errors_stop_every_open_openrouter_entry_and_reload_the_balance() {
    for (scenario, http, source) in [
        ("402-credits", 402, "openrouter_credits"),
        ("402-key-limit", 402, "openrouter_key_limit"),
        ("403-guardrail", 403, "openrouter_guardrail_budget"),
    ] {
        let mock = Mock::start().await;
        let limiter = limiter_with(LONG_TIMEOUT, 1, |c| c.breaker_threshold = 1);
        let openrouter = Arc::new(adapter(&mock));
        let texts = (0..5)
            .map(|i| format!("MOCK-SCENARIO:{scenario} Eintrag {i}"))
            .collect();
        let results = run_entries(limiter.clone(), openrouter.clone(), texts).await;
        assert_eq!(mock.requests_to(CHAT).len(), 1, "{scenario}");
        assert!(
            results
                .iter()
                .all(|(_, e)| status(e, None) == AiStatus::QuotaExceeded && e.attempts == 1),
            "{scenario}: {results:?}"
        );
        let sent: Vec<_> = results
            .iter()
            .filter(|(_, e)| e.http_status().is_some())
            .collect();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1.http_status(), Some(http));
        assert_eq!(sent[0].1.error_code(), Some(source));
        assert!(
            !limiter.breaker_open("openrouter"),
            "{scenario}: zählt nicht"
        );
        // Kontostand neu geladen.
        assert_eq!(mock.requests_to(KEY_PATH).len(), 1, "{scenario}");
        assert!(openrouter.last_balance().is_some());

        let anthropic = AnthropicAdapter::new(reqwest::Client::new(), mock.anthropic_config());
        let request = text_request(anthropic_model(), "MOCK-ID:home weiter");
        let other = limiter
            .run(&RequestSpec::new("anthropic", "mock-claude", 10), |_| {
                anthropic.send(&request, None)
            })
            .await;
        assert_eq!(status(&other, None), AiStatus::Success, "{scenario}");
    }

    let mock = Mock::start().await;
    let texts = (0..9)
        .map(|i| format!("MOCK-SCENARIO:402-credits {i}"))
        .collect();
    let results = run_entries(limiter(LONG_TIMEOUT, 3), Arc::new(adapter(&mock)), texts).await;
    let sent = mock.requests_to(CHAT).len();
    assert!((1..=3).contains(&sent), "{sent}");
    assert!(results
        .iter()
        .all(|(_, e)| status(e, None) == AiStatus::QuotaExceeded));
}

// spec: AI-005
// `402-weight` → nur dieser Eintrag `Quota exceeded`; die übrigen OpenRouter-Einträge laufen weiter.
#[tokio::test]
async fn weight_402_stops_only_its_own_entry() {
    let mock = Mock::start().await;
    let openrouter = Arc::new(adapter(&mock));
    let texts = vec![
        "MOCK-ID:home eins".to_string(),
        "MOCK-SCENARIO:402-weight zu groß".to_string(),
        "MOCK-ID:home zwei".to_string(),
        "MOCK-ID:home drei".to_string(),
    ];
    let results = run_entries(limiter(LONG_TIMEOUT, 1), openrouter.clone(), texts).await;
    for (text, execution) in &results {
        if text.contains("402-weight") {
            assert_eq!(status(execution, None), AiStatus::QuotaExceeded);
            assert_eq!(execution.attempts, 1);
            assert_eq!(execution.error_code(), Some("weight_exceeds_budget"));
        } else {
            assert_eq!(status(execution, None), AiStatus::Success, "{text}");
        }
    }
    assert!(!openrouter.quota().is_tripped());
    assert_eq!(mock.requests_to(CHAT).len(), 4);
}

// spec: AI-005
// Kontostand `GET /key`: `limit_remaining` wird direkt angezeigt (Mock `limit = 10`, `usage = 42.00`,
// `limit_remaining = 5.80`, `limit_reset = "monthly"` → `5.80`, nicht 10 − 42); `limit = null`/`limit_remaining = null`
// → unbegrenzt. Über HTTP gegen die eingebaute `/key`-Antwort des Mocks.
#[tokio::test]
async fn key_balance_shows_limit_remaining_directly() {
    let periodic = KeyBalance::from_json(&json!({"data": {
        "label": "k01", "limit": 10, "usage": 42.00, "limit_remaining": 5.80, "limit_reset": "monthly",
        "usage_daily": 1.5, "usage_weekly": 7, "usage_monthly": 30.25
    }}))
    .unwrap();
    assert_eq!(periodic.remaining(), Remaining::MicroUsd(5_800_000));
    assert_eq!(format_micro_usd(5_800_000, 2), "5.80");
    assert_eq!(periodic.limit_reset.as_deref(), Some("monthly"));
    assert_eq!(periodic.limit, Some(10_000_000));
    assert_eq!(periodic.usage, Some(42_000_000));
    assert_eq!(periodic.usage_daily, Some(1_500_000));
    assert_eq!(periodic.usage_weekly, Some(7_000_000));
    assert_eq!(periodic.usage_monthly, Some(30_250_000));
    let unlimited = KeyBalance::from_json(&json!({"data": {
        "limit": null, "limit_remaining": null, "usage": 3
    }}))
    .unwrap();
    assert_eq!(unlimited.remaining(), Remaining::Unlimited);
    let no_limit =
        KeyBalance::from_json(&json!({"data": {"limit": null, "limit_remaining": 4}})).unwrap();
    assert_eq!(no_limit.remaining(), Remaining::Unlimited);

    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let balance = adapter.key_balance().await.unwrap();
    assert_eq!(balance.remaining(), Remaining::MicroUsd(90_000_000));
    assert_eq!(balance.limit, Some(100_000_000));
    assert_eq!(balance.usage, Some(10_000_000));
    assert_eq!(adapter.last_balance(), Some(balance));
    let record = &mock.requests_to(KEY_PATH)[0];
    assert_eq!(record.method, "GET");
    assert!(record.headers.contains_key("authorization"));
}

// spec: AI-005
// spec: AI-014
// Preise aus der API haben Vorrang vor der Preistabelle; die Preisquelle im Protokoll lautet `api`; Gebühren je
// Request und Bild landen in `Fixed Fees`.
#[tokio::test]
async fn api_prices_take_precedence_and_are_logged_as_api() {
    let mock = Mock::start().await;
    let models = adapter(&mock).list_models().await.unwrap();
    let vision = models.iter().find(|m| m.id == "mock/vision").unwrap();
    let api = vision.rates().unwrap();
    assert_eq!(api.source, PriceSource::Api);
    assert_eq!(api.input_pico, 1_000_000);
    assert_eq!(api.per_request_pico, 1_000_000_000);
    assert_eq!(api.per_image_pico, 2_000_000_000);
    let table = Rates::per_million("9", "9", PriceSource::Table).unwrap();
    assert_eq!(
        resolve_rates(Some(api.clone()), Some(table.clone())),
        Some(api.clone())
    );
    assert_eq!(resolve_rates(None, Some(table.clone())), Some(table));

    let reply = adapter(&mock)
        .send(&text_request(openrouter_model(), "MOCK-ID:home Bild"), None)
        .await
        .unwrap();
    let ctx = LogContext {
        run_id: 1,
        timestamp: "2026-11-24T10:00:00Z".into(),
        task: "suggest-alt".into(),
        provider: "openrouter".into(),
        model: "mock/vision".into(),
        endpoint: "chat-completions".into(),
        url_id: None,
        definition_version: None,
        batch_id: None,
        redactions: 0,
    };
    let outcome = RequestOutcome::success(reply.usage.clone(), 1, reply.duration_ms);
    let record = request_record(&ctx, &api, MicroUsd(1_000_000), &outcome);
    assert_eq!(record.price_source.as_str(), "api");
    assert_eq!(record.fixed_fees, MicroUsd(3_000));
    let broken =
        OpenRouterModel::from_json(&json!({"id": "x", "pricing": {"prompt": "-1"}})).unwrap();
    assert!(!broken.prices_known && broken.rates().is_none());
}

// spec: AI-005
// spec: AI-038
// Form a: `refusal` → `Refusal`, Grund = Refusal-Text, kein Parse-Versuch; `refusal-filter-only` → Grund
// `content_filter`; jeweils genau 1 Request für den Eintrag.
#[tokio::test]
async fn refusal_in_the_response_body_is_a_refusal_with_its_reason() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let limiter = limiter(LONG_TIMEOUT, 1);
    for (scenario, reason) in [
        ("refusal", "Fixture refusal"),
        ("refusal-filter-only", "content_filter"),
    ] {
        let before = mock.requests_to(CHAT).len();
        let request = schema_request(
            openrouter_model(),
            &format!("MOCK-SCENARIO:{scenario}"),
            contract_schema(),
        );
        let execution = run(&limiter, &adapter, &request).await;
        assert_eq!(execution.attempts, 1);
        assert_eq!(mock.requests_to(CHAT).len(), before + 1);
        let reply = execution.result.as_ref().unwrap();
        assert_eq!(reply.raw, RawAnswer::Refusal);
        assert_eq!(reply.refusal_reason.as_deref(), Some(reason), "{scenario}");
        assert_eq!(reply.stop_reason.as_deref(), Some("content_filter"));
        assert_eq!(
            status(&execution, request.response_schema.as_ref()),
            AiStatus::Refusal
        );
    }
}

// spec: AI-005
// spec: AI-017
// spec: AI-038
// Form b: `refusal-403` → `Refusal` (nicht `Error: 403 …`, nicht `Quota exceeded`), genau 1 Request, keine
// Wiederholung; die übrigen OpenRouter-Einträge enden mit `Success`; der Schutzschalter zählt den Fall nicht.
#[tokio::test]
async fn refusal_403_is_a_refusal_without_retry_quota_or_breaker() {
    let mock = Mock::start().await;
    let openrouter = Arc::new(adapter(&mock));
    let limiter = limiter_with(LONG_TIMEOUT, 1, |c| c.breaker_threshold = 1);
    let texts = vec![
        "MOCK-SCENARIO:refusal-403 verweigert".to_string(),
        "MOCK-ID:home eins".to_string(),
        "MOCK-ID:home zwei".to_string(),
    ];
    let results = run_entries(limiter.clone(), openrouter.clone(), texts).await;
    for (text, execution) in &results {
        assert_eq!(execution.attempts, 1, "{text}");
        if text.contains("refusal-403") {
            assert_eq!(status(execution, None), AiStatus::Refusal);
            let reply = execution.result.as_ref().unwrap();
            assert_eq!(reply.http_status, 403);
            assert_eq!(
                reply.refusal_reason.as_deref(),
                Some("provider reported a model refusal as an error")
            );
        } else {
            assert_eq!(status(execution, None), AiStatus::Success, "{text}");
        }
    }
    assert!(!openrouter.quota().is_tripped());
    assert!(!limiter.breaker_open("openrouter"));
    assert_eq!(mock.requests_to(CHAT).len(), 3);
    assert!(mock.requests_to(KEY_PATH).is_empty());
}

// spec: AI-005
// spec: AI-001
// `usage` → einheitliche Felder: `prompt_tokens` (einschließlich gecachter Tokens), `completion_tokens`,
// `prompt_tokens_details.cached_tokens`/`cache_write_tokens`; fehlende Felder bleiben `null`.
#[tokio::test]
async fn usage_is_mapped_and_missing_fields_stay_null() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let reply = adapter
        .send(
            &text_request(openrouter_model(), "MOCK-ID:home Zählen"),
            None,
        )
        .await
        .unwrap();
    let body = &mock.bodies(CHAT)[0];
    assert_eq!(
        reply.usage.input_tokens,
        Some(os_testsite::mocks::ai::token_count(body) as u64)
    );
    assert!(reply.usage.output_tokens.unwrap() > 0);
    assert_eq!(reply.usage.cached_input_tokens, None);
    assert_eq!(reply.usage.cache_write_tokens, None);
    let cached = usage_from_json(Some(&json!({
        "prompt_tokens": 1200, "completion_tokens": 40,
        "prompt_tokens_details": {"cached_tokens": 1000, "cache_write_tokens": 100}
    })));
    assert_eq!(cached.input_tokens, Some(1200));
    assert_eq!(cached.output_tokens, Some(40));
    assert_eq!(cached.cached_input_tokens, Some(1000));
    assert_eq!(cached.cache_write_tokens, Some(100));
}

// spec: AI-005
// Bilder gehen als `image_url` mit Base64-Data-URL hinaus (Fixture-Site).
#[tokio::test]
async fn images_are_sent_as_data_urls() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("fixtures")
            .join("ai")
            .join("site")
            .join("images")
            .join("forest.png"),
    )
    .unwrap();
    let image = image_input(&bytes).unwrap();
    let Input::Image { base64, .. } = &image else {
        panic!("kein Bild")
    };
    let mock = Mock::start().await;
    let mut request = text_request(openrouter_model(), "MOCK-ID:home Alt-Text");
    request.input.push(image.clone());
    adapter(&mock).send(&request, None).await.unwrap();
    let body = &mock.bodies(CHAT)[0];
    let part = &body["messages"][1]["content"][1];
    assert_eq!(part["type"], "image_url");
    assert_eq!(
        part["image_url"]["url"],
        format!("data:image/png;base64,{base64}")
    );
}

// spec: AI-017
// spec: INTEG-005
// `429x2` über den echten Adapter → 3 Requests, `Success` (Testuhr).
#[tokio::test]
async fn plain_429_is_retried_through_the_adapter() {
    let mock = Mock::start().await;
    let adapter = adapter(&mock);
    let execution = run(
        &limiter(LONG_TIMEOUT, 1),
        &adapter,
        &text_request(openrouter_model(), "MOCK-SCENARIO:429x2"),
    )
    .await;
    assert_eq!(execution.attempts, 3);
    assert_eq!(status(&execution, None), AiStatus::Success);
    assert_eq!(mock.requests_to(CHAT).len(), 3);
}

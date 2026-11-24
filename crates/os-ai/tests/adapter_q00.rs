//! Adapter-Nachträge nach der Live-Probe Q00 (08 AI-001 … AI-006, Nachtrag 24.11.2026; M4-K09) gegen den ai-mock:
//! Reasoning-Parameter je Anbieter, `store: false`, Abbruch an der Ausgabegrenze als eigene Antwortklasse,
//! `reasoning_tokens`/`cache_write_tokens`, Request-IDs, OpenRouter-Ist-Kosten und Preisfelder, unbekanntes Modell.
//!
//! Den Status `Error: Output limit reached` und die Protokollspalten vergibt M4-K10; die zugehörigen Kriterien stehen als
//! Platzhalter in `pending_m4_k09.rs`.
use std::sync::Arc;

use os_ai::adapters::anthropic::{self, AnthropicAdapter, AnthropicConfig, ApiKey};
use os_ai::adapters::ollama::{self, OllamaAdapter, OllamaConfig};
use os_ai::adapters::openai_decisions::{DecideOptions, DecisionInput, OpenAiDecisions, Question};
use os_ai::adapters::openai_responses::{
    self, OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses, ResponsesOutput,
};
use os_ai::adapters::openrouter::{self, OpenRouterAdapter, OpenRouterConfig};
use os_ai::cost::{cost_of, request_record, FeeCounts, LogContext, TokenCounts};
use os_ai::limits::{Limiter, LimitsConfig, TokioClock};
use os_ai::provider::{
    self, Capabilities, CapabilitySource, Input, Model, Outcome, ProviderError, ReasoningLevel,
    Request,
};
use os_ai::schema::{RepairAttempts, ResponseSchema, StructuredOutcome};
use os_ai::{AiStatus, PreviewSwitch};
use os_model::budget::MicroUsd;
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use reqwest::header::HeaderMap;
use serde_json::{json, Value};

const MESSAGES: &str = "/anthropic/v1/messages";
const RESPONSES: &str = "/openai/v1/responses";
const DECISIONS: &str = "/openai/v1/decisions";
const CHAT: &str = "/openrouter/api/v1/chat/completions";
const OLLAMA_CHAT: &str = "/ollama/api/chat";
const ALL_LEVELS: [ReasoningLevel; 8] = [
    ReasoningLevel::ModelDefault,
    ReasoningLevel::Off,
    ReasoningLevel::Minimal,
    ReasoningLevel::Low,
    ReasoningLevel::Medium,
    ReasoningLevel::High,
    ReasoningLevel::XHigh,
    ReasoningLevel::Max,
];

struct Mock {
    mock: AiMock,
    server: MockServer,
}

impl Mock {
    async fn start() -> Self {
        let mock = AiMock::fixtures().expect("ai-mock-Fragmente");
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Self { mock, server }
    }
    fn requests_to(&self, path: &str) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.path.split('?').next() == Some(path))
            .collect()
    }
    fn bodies(&self, path: &str) -> Vec<Value> {
        self.requests_to(path)
            .iter()
            .map(|r| serde_json::from_str(&r.body).unwrap())
            .collect()
    }
    fn anthropic(&self) -> AnthropicAdapter {
        AnthropicAdapter::new(
            client(),
            AnthropicConfig::new(ApiKey::new("sk-ant-TEST-K09"))
                .with_base_url(self.server.base_url("anthropic")),
        )
    }
    fn openrouter(&self) -> OpenRouterAdapter {
        OpenRouterAdapter::new(
            client(),
            OpenRouterConfig::new(openrouter::ApiKey::new("sk-or-TEST-K09"))
                .with_base_url(format!("{}/api/v1", self.server.base_url("openrouter"))),
        )
    }
    fn openai_http(&self) -> OpenAiHttp {
        OpenAiHttp::new(
            client(),
            OpenAiConfig::new(openai_responses::ApiKey::new("sk-TEST-K09"))
                .with_base_url(format!("{}/v1", self.server.base_url("openai"))),
            OpenAiAccount::new(),
        )
    }
    fn ollama(&self) -> OllamaAdapter {
        OllamaAdapter::new(
            client(),
            OllamaConfig::new().with_base_url(self.server.base_url("ollama")),
        )
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

fn limiter() -> Limiter {
    Limiter::new(LimitsConfig::default(), Arc::new(TokioClock::new()))
}

fn model(provider: &str, id: &str, endpoint: &str, levels: Vec<ReasoningLevel>) -> Model {
    Model {
        provider: provider.into(),
        id: id.into(),
        endpoint: endpoint.into(),
        capabilities: Capabilities {
            text: true,
            structured_output: true,
            vision: true,
            ..Capabilities::default()
        },
        capability_source: CapabilitySource::ModelCatalog,
        reasoning_levels: levels,
    }
}

fn request(model: Model, text: &str, schema: Option<Value>, level: ReasoningLevel) -> Request {
    Request {
        model,
        required: Capabilities {
            text: true,
            ..Capabilities::default()
        },
        input: vec![Input::Text { text: text.into() }],
        instructions: "K09".into(),
        questions: Vec::new(),
        response_schema: schema,
        max_tokens: Some(256),
        temperature: None,
        effort: None,
        reasoning: level,
    }
}

fn ok_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}},
        "required": ["ok"],
        "additionalProperties": false
    })
}

fn contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(k, v)| k == key || contains_key(v, key)),
        Value::Array(items) => items.iter().any(|v| contains_key(v, key)),
        _ => false,
    }
}

fn log_ctx(provider: &str, endpoint: &str) -> LogContext {
    LogContext {
        run_id: 1,
        timestamp: "2026-11-24T10:00:00Z".into(),
        task: "suggest-title".into(),
        provider: provider.into(),
        model: "m".into(),
        endpoint: endpoint.into(),
        url_id: Some(1),
        definition_version: None,
        batch_id: None,
        redactions: 0,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// AI-002 Anthropic

/// (`thinking.type`, `output_config.effort`) eines gesendeten Requests.
fn anthropic_params(body: &Value) -> (Option<String>, Option<String>) {
    (
        body.pointer("/thinking/type")
            .and_then(Value::as_str)
            .map(str::to_string),
        body.pointer("/output_config/effort")
            .and_then(Value::as_str)
            .map(str::to_string),
    )
}

// spec: AI-002 [M4]
// Stufe `off`: A → `thinking.type = "disabled"` ohne Aufwand; B (nicht abschaltbar) → `adaptive` + `low`; C und D →
// keines der Felder. `medium` auf A → `adaptive` + `medium`; `max` auf B → `xhigh`; `model-default` → keines.
#[tokio::test]
async fn anthropic_levels_map_to_thinking_and_effort() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let models = adapter.list_models().await.unwrap();
    let find = |id: &str| models.iter().find(|m| m.id == id).unwrap().to_model();
    let cases = [
        ("mock-claude", ReasoningLevel::Off, Some("disabled"), None),
        (
            "mock-claude-b",
            ReasoningLevel::Off,
            Some("adaptive"),
            Some("low"),
        ),
        ("mock-claude-c", ReasoningLevel::Off, None, None),
        ("mock-claude-d", ReasoningLevel::Off, None, None),
        (
            "mock-claude",
            ReasoningLevel::Medium,
            Some("adaptive"),
            Some("medium"),
        ),
        (
            "mock-claude-b",
            ReasoningLevel::Max,
            Some("adaptive"),
            Some("xhigh"),
        ),
        ("mock-claude", ReasoningLevel::ModelDefault, None, None),
    ];
    for (id, level, thinking, effort) in cases {
        let req = request(find(id), "MOCK-ID:home Stufe", None, level);
        adapter.send(&req, None).await.unwrap();
        let body = mock.bodies(MESSAGES).pop().unwrap();
        assert_eq!(
            anthropic_params(&body),
            (thinking.map(str::to_string), effort.map(str::to_string)),
            "{id} {level}: {body}"
        );
        assert!(body.get("effort").is_none());
    }
    // Verfügbare Stufen: B ohne `off` (Dropdown), D nur `model-default`.
    let levels = |id: &str| find(id).reasoning_levels;
    assert_eq!(
        levels("mock-claude"),
        [
            ReasoningLevel::Off,
            ReasoningLevel::Low,
            ReasoningLevel::Medium,
            ReasoningLevel::High,
            ReasoningLevel::XHigh,
            ReasoningLevel::Max
        ]
    );
    assert!(!levels("mock-claude-b").contains(&ReasoningLevel::Off));
    assert!(levels("mock-claude-c").is_empty());
    assert!(levels("mock-claude-d").is_empty());
    // Ausgabegrenze von B aus der Modellliste (Wert für AI-011, M4-K04).
    let b = models.iter().find(|m| m.id == "mock-claude-b").unwrap();
    assert_eq!(b.max_tokens, Some(4000));
}

// spec: AI-002 [M4]
// Property über alle Modelle A–D und alle Stufen: kein Request enthält `thinking.type = "enabled"` oder `budget_tokens`,
// nie ein Top-Level-`effort`.
#[tokio::test]
async fn anthropic_never_sends_enabled_thinking_or_budget_tokens() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let models = adapter.list_models().await.unwrap();
    for m in &models {
        for level in ALL_LEVELS {
            for schema in [None, Some(ok_schema())] {
                let req = request(m.to_model(), "MOCK-ID:home P", schema, level);
                adapter.send(&req, None).await.unwrap();
            }
        }
    }
    let bodies = mock.bodies(MESSAGES);
    assert_eq!(bodies.len(), models.len() * ALL_LEVELS.len() * 2);
    for body in &bodies {
        assert_ne!(
            body.pointer("/thinking/type").and_then(Value::as_str),
            Some("enabled")
        );
        assert!(!contains_key(body, "budget_tokens"), "{body}");
        assert!(body.get("effort").is_none());
        if let Some(thinking) = body.get("thinking") {
            let kind = thinking["type"].as_str().unwrap();
            assert!(kind == "adaptive" || kind == "disabled", "{body}");
        }
    }
}

// spec: AI-002 [M4]
// spec: AI-021 [M4]
// Szenario `output-limit` bei einer Schema- und einer Freitext-Aufgabe: eigene Antwortklasse mit Grund `max_tokens`,
// kein Teilergebnis, `Output Tokens` = gesendetes `max_tokens`, `HTTP Status` 200, genau 1 Request (kein
// Reparaturversuch).
#[tokio::test]
async fn anthropic_output_limit_is_its_own_class_without_repair() {
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
        );
        let reply = adapter.send(&req, None).await.unwrap();
        assert_eq!(reply.output_limit.as_deref(), Some("max_tokens"));
        assert!(reply.is_output_limit());
        assert_eq!(
            reply.clone().into_response(schema.as_ref()),
            Err(ProviderError::InvalidOutput),
            "bis M4-K10 vorhandener Fehlerweg"
        );
        let limit = reply.for_repair().unwrap_err();
        assert_eq!(limit.reason, "max_tokens");
        assert_eq!(limit.http_status, 200);
        assert_eq!(limit.usage.output_tokens, Some(256));
        assert_eq!(limit.usage.reasoning_tokens, Some(256));
        let failure = limit.attempt_failure();
        assert_eq!(failure.error_code.as_deref(), Some("max_tokens"));
        assert_eq!(failure.http_status, Some(200));
        assert!(!failure.is_transient());
        assert_eq!(mock.requests_to(MESSAGES).len(), before + 1);
        // Die Tokens werden verbucht: Kosten > 0 aus `Output Tokens`.
        let rates =
            os_ai::cost::Rates::per_million("1", "5", os_ai::cost::PriceSource::Table).unwrap();
        let cost = cost_of(
            &TokenCounts::from_usage(&limit.usage),
            FeeCounts::one_request(0),
            &rates,
            false,
        );
        assert!(cost.cost.0 > 0);
    }
}

// spec: AI-002 [M4]
// spec: AI-001 [M4]
// Szenario `reasoning=40`: `reasoning_tokens = 40`, `output_tokens` = Antwort-Tokens + 40; `thinking`-Block verworfen,
// geparstes Ergebnis wie ohne Szenario. `Token Cost` aus `output_tokens` einmal (keine Doppelzählung).
#[tokio::test]
async fn anthropic_reasoning_tokens_are_part_of_output_tokens() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let a = model("anthropic", "mock-claude", "messages", Vec::new());
    let plain = adapter
        .send(
            &request(
                a.clone(),
                "MOCK-ID:home r",
                Some(ok_schema()),
                ReasoningLevel::ModelDefault,
            ),
            None,
        )
        .await
        .unwrap();
    let with = adapter
        .send(
            &request(
                a,
                "MOCK-ID:home MOCK-SCENARIO:reasoning=40 r",
                Some(ok_schema()),
                ReasoningLevel::ModelDefault,
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(plain.usage.reasoning_tokens, Some(0));
    assert_eq!(with.usage.reasoning_tokens, Some(40));
    assert_eq!(
        with.usage.output_tokens,
        plain.usage.output_tokens.map(|o| o + 40)
    );
    assert_eq!(with.raw, plain.raw);
    let schema = ok_schema();
    let (plain, with) = (
        plain.into_response(Some(&schema)).unwrap(),
        with.into_response(Some(&schema)).unwrap(),
    );
    assert_eq!(with.content, plain.content);
    let rendered = format!("{with:?} {}", serde_json::to_string(&with).unwrap());
    for marker in ["MOCK-REASONING-TEXT", "MOCK-SIGNATURE", "MOCK-ENCRYPTED"] {
        assert!(!rendered.contains(marker));
    }
    let rates = os_ai::cost::Rates::per_million("1", "5", os_ai::cost::PriceSource::Table).unwrap();
    let cost = cost_of(
        &TokenCounts::from_usage(&with.usage),
        FeeCounts::one_request(0),
        &rates,
        false,
    );
    let expected = cost_of(
        &TokenCounts::plain(
            with.usage.input_tokens.unwrap(),
            with.usage.output_tokens.unwrap(),
        ),
        FeeCounts::one_request(0),
        &rates,
        false,
    );
    assert_eq!(cost.token_cost, expected.token_cost);
}

// spec: AI-001 [M4]
// Request-ID Anthropic: Header `request-id: req_mock_<n>`, nie die Antwort-`id` `msg_mock_<n>`; Fehlerantwort ohne
// Header mit `request_id` im Körper → dieser Wert.
#[tokio::test]
async fn anthropic_request_id_comes_from_the_header() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let a = model("anthropic", "mock-claude", "messages", Vec::new());
    let reply = adapter
        .send(
            &request(a, "MOCK-ID:home id", None, ReasoningLevel::ModelDefault),
            None,
        )
        .await
        .unwrap();
    let id = reply.provider_request_id.clone().unwrap();
    assert!(id.starts_with("req_mock_"), "{id}");
    let mut headers = HeaderMap::new();
    assert_eq!(
        anthropic::request_id(
            &headers,
            &json!({"type": "error", "request_id": "req_mock_7"})
        ),
        Some("req_mock_7".into())
    );
    headers.insert("request-id", "req_mock_7".parse().unwrap());
    assert_eq!(
        anthropic::request_id(&headers, &json!({"id": "msg_mock_7"})),
        Some("req_mock_7".into())
    );
    assert_eq!(
        anthropic::request_id(&HeaderMap::new(), &json!({"id": "msg_mock_7"})),
        None
    );
}

// ---------------------------------------------------------------------------------------------------------------
// AI-003 OpenAI Responses

fn luna(levels: Vec<ReasoningLevel>) -> Model {
    model("openai", "gpt-6-luna", "responses", levels)
}

fn catalog_levels() -> Vec<ReasoningLevel> {
    // Katalog `reasoning_efforts = ["none", "low", "medium", "high"]` (`none` → `off`, M4-K04).
    vec![
        ReasoningLevel::Off,
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
    ]
}

// spec: AI-003 [M4]
// Property über alle Responses-Szenarien einschließlich `malformed-json` (Reparaturversuch): jeder Request an
// `/openai/v1/responses` enthält `"store": false`; nie `prompt_cache_retention`.
#[tokio::test]
async fn every_responses_request_sends_store_false() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.openai_http());
    let limiter = limiter();
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    for scenario in [
        "ok",
        "refusal",
        "malformed-json",
        "schema-violation",
        "output-limit",
        "reasoning=5",
        "missing-usage",
    ] {
        for level in [ReasoningLevel::ModelDefault, ReasoningLevel::Off] {
            let req = request(
                luna(catalog_levels()),
                &format!("MOCK-SCENARIO:{scenario} {level}"),
                None,
                level,
            );
            let _ = adapter
                .run_structured(&limiter, &req, &schema, RepairAttempts::default(), 10)
                .await;
            let _ = adapter.run(&limiter, &req, 10).await;
        }
    }
    let bodies = mock.bodies(RESPONSES);
    // `malformed-json` und `schema-violation` lösen je einen Reparaturversuch aus.
    assert!(bodies.len() >= 7 * 2 * 2 + 4, "{}", bodies.len());
    for body in &bodies {
        assert_eq!(body["store"], json!(false), "{body}");
        assert!(body.get("prompt_cache_retention").is_none());
    }
}

// spec: AI-003 [M4]
// Katalogmodell mit `reasoning_efforts = ["none", "low", "medium", "high"]`: `off` → `"none"`, `high` → `"high"`,
// `max` → `"high"`; Katalogmodell ohne `reasoning_efforts` → kein Schlüssel `reasoning`; `model-default` → keiner.
#[tokio::test]
async fn responses_reasoning_effort_follows_the_catalog() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.openai_http());
    let cases = [
        (catalog_levels(), ReasoningLevel::Off, Some("none")),
        (catalog_levels(), ReasoningLevel::High, Some("high")),
        (catalog_levels(), ReasoningLevel::Max, Some("high")),
        (catalog_levels(), ReasoningLevel::Minimal, Some("none")),
        (catalog_levels(), ReasoningLevel::ModelDefault, None),
        (Vec::new(), ReasoningLevel::High, None),
        (Vec::new(), ReasoningLevel::Off, None),
    ];
    for (levels, level, expected) in cases {
        adapter
            .send(
                &request(luna(levels.clone()), "MOCK-ID:home e", None, level),
                None,
                None,
            )
            .await
            .unwrap();
        let body = mock.bodies(RESPONSES).pop().unwrap();
        match expected {
            Some(effort) => assert_eq!(body["reasoning"], json!({"effort": effort}), "{level}"),
            None => assert!(
                body.get("reasoning").is_none(),
                "{level} {levels:?}: {body}"
            ),
        }
        assert!(body.get("reasoning_effort").is_none());
    }
    // Stufen aus dem Katalog über `with_reasoning_levels` (M4-K04) landen in der Modellliste.
    let adapter =
        OpenAiResponses::new(mock.openai_http()).with_reasoning_levels(Arc::new(|id: &str| {
            if id == "gpt-6-luna" {
                catalog_levels()
            } else {
                Vec::new()
            }
        }));
    let models = adapter.list_models().await.unwrap();
    let luna = models.iter().find(|m| m.id == "gpt-6-luna").unwrap();
    assert_eq!(luna.reasoning_levels, catalog_levels());
}

// spec: AI-003 [M4]
// spec: AI-021 [M4]
// Szenario `output-limit` → Abbruch mit `Error Code = max_output_tokens`, `HTTP Status` 200, genau 1 Request (kein
// Reparaturversuch), gemeldete Tokens im Protokoll (`Output Tokens` = gesendete Grenze).
#[tokio::test]
async fn responses_output_limit_ends_without_repair() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.openai_http());
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let req = request(
        luna(Vec::new()),
        "MOCK-SCENARIO:output-limit x",
        None,
        ReasoningLevel::ModelDefault,
    );
    let run = adapter
        .run_structured(&limiter(), &req, &schema, RepairAttempts::default(), 10)
        .await;
    let StructuredOutcome::Failed(failure) = run.outcome else {
        panic!("erwartet Abbruch: {:?}", run.outcome);
    };
    assert_eq!(
        failure.error.error_code.as_deref(),
        Some("max_output_tokens")
    );
    assert_eq!(failure.error.http_status, Some(200));
    assert_eq!(failure.error.status, AiStatus::InvalidOutput, "bis M4-K10");
    assert_eq!(failure.meta.http_status, Some(200));
    assert_eq!(
        failure.meta.error_code.as_deref(),
        Some("max_output_tokens")
    );
    let usage = failure.meta.usage.clone().unwrap();
    assert_eq!(usage.output_tokens, Some(256));
    assert_eq!(mock.requests_to(RESPONSES).len(), 1);
    // Freitext: dieselbe Antwortklasse, kein Teilergebnis.
    let reply = adapter.send(&req, None, None).await.unwrap();
    assert_eq!(
        reply.output,
        ResponsesOutput::OutputLimit("max_output_tokens".into())
    );
    assert_eq!(
        provider::execute(PreviewSwitch::ON, &adapter, req).await,
        Outcome::Status(AiStatus::InvalidOutput)
    );
}

// spec: AI-003 [M4]
// spec: AI-001 [M4]
// `cache_write_tokens: 0` → Protokoll `Cache Write Tokens = 0` (nicht leer); `reasoning=27` → `reasoning_tokens = 27`;
// das `reasoning`-Element wird verworfen, der Text kommt aus dem `message`-Element.
#[tokio::test]
async fn responses_usage_reads_cache_write_and_reasoning_tokens() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.openai_http());
    let limiter = limiter();
    let (reply, meta) = adapter
        .run(
            &limiter,
            &request(
                luna(Vec::new()),
                "MOCK-ID:home u",
                None,
                ReasoningLevel::ModelDefault,
            ),
            10,
        )
        .await;
    let plain = reply.unwrap();
    let usage = meta.usage.clone().unwrap();
    assert_eq!(usage.cache_write_tokens, Some(0));
    assert_eq!(usage.reasoning_tokens, Some(0));
    let row = request_record(
        &log_ctx("openai", "responses"),
        &os_ai::cost::Rates::per_million("0.1", "0.5", os_ai::cost::PriceSource::Table).unwrap(),
        MicroUsd(0),
        &meta.request_outcome(AiStatus::Success, 10, 10, 0),
    );
    assert_eq!(row.cache_write_tokens, Some(0));
    let (reply, meta) = adapter
        .run(
            &limiter,
            &request(
                luna(Vec::new()),
                "MOCK-ID:home MOCK-SCENARIO:reasoning=27 u",
                None,
                ReasoningLevel::ModelDefault,
            ),
            10,
        )
        .await;
    let with = reply.unwrap();
    assert_eq!(meta.usage.unwrap().reasoning_tokens, Some(27));
    assert_eq!(with.output, plain.output);
    assert!(!format!("{with:?}").contains("MOCK-ENCRYPTED"));
}

// ---------------------------------------------------------------------------------------------------------------
// AI-004 OpenAI Decisions

// spec: AI-004 [M4]
// Property über alle Decisions-Szenarien: kein Request an `/openai/v1/decisions` enthält `store` oder `reasoning`.
// Antwort ohne `id` → `Provider Request ID` = Header `x-request-id`; `cache_write_tokens: 0` → `Cache Write Tokens = 0`.
#[tokio::test]
async fn decisions_send_neither_store_nor_reasoning_and_use_the_header_id() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.openai_http());
    let limiter = limiter();
    let questions = vec![
        Question::predicate("q1", "Is it a product page?"),
        Question::choice(
            "q2",
            "Which type?",
            &[("product", "Product."), ("other", "Other.")],
        ),
    ];
    let mut ids = Vec::new();
    for scenario in [
        "ok",
        "refusal",
        "missing-answer",
        "missing-usage",
        "reasoning=40",
        "output-limit",
    ] {
        let run = adapter
            .decide(
                &limiter,
                &DecisionInput::text(format!("MOCK-SCENARIO:{scenario} Seite")),
                &questions,
                &DecideOptions::default(),
            )
            .await;
        let record = &run.requests[0];
        ids.push(record.meta.provider_request_id.clone());
        if scenario == "ok" {
            let usage = record.meta.usage.clone().unwrap();
            assert_eq!(usage.cache_write_tokens, Some(0));
            assert_eq!(usage.reasoning_tokens, Some(0));
            assert_eq!(usage.output_tokens, Some(0));
            let row = request_record(
                &log_ctx("openai", "decisions"),
                &os_ai::cost::Rates::per_million("0.1", "0", os_ai::cost::PriceSource::Table)
                    .unwrap(),
                MicroUsd(0),
                &record.request_outcome(),
            );
            assert_eq!(row.cache_write_tokens, Some(0));
            assert_eq!(row.provider_request_id, record.meta.provider_request_id);
        }
    }
    for body in mock.bodies(DECISIONS) {
        assert!(!contains_key(&body, "store"), "{body}");
        assert!(!contains_key(&body, "reasoning"), "{body}");
    }
    let records = mock.requests_to(DECISIONS);
    assert_eq!(records.len(), ids.len());
    for (n, id) in ids.iter().enumerate() {
        assert!(
            id.as_deref().is_some_and(|id| id.starts_with("req_mock_")),
            "{n}: {id:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// AI-005 OpenRouter

fn router(id: &str) -> Model {
    let mut m = model("openrouter", id, "chat-completions", Vec::new());
    m.capability_source = CapabilitySource::OpenRouterModels;
    m
}

// spec: AI-005 [M4]
// Reasoning: `off` auf M-R1 → `reasoning.effort = "none"`; auf M-R2 (`mandatory`, ohne `none`) → `"low"`; auf M-R3 →
// kein Schlüssel; `model-default` → kein Schlüssel; nie `reasoning_effort` oder `include_reasoning`.
#[tokio::test]
async fn openrouter_reasoning_follows_supported_efforts() {
    let mock = Mock::start().await;
    let adapter = mock.openrouter();
    let cases = [
        ("mock/text-small", ReasoningLevel::Off, Some("none")),
        ("mock/text-large", ReasoningLevel::Off, Some("low")),
        ("mock/global-c", ReasoningLevel::Off, None),
        ("mock/text-small", ReasoningLevel::ModelDefault, None),
        ("mock/text-small", ReasoningLevel::Max, Some("high")),
        ("mock/text-large", ReasoningLevel::Medium, Some("medium")),
    ];
    for (id, level, expected) in cases {
        adapter
            .send(&request(router(id), "MOCK-ID:home o", None, level), None)
            .await
            .unwrap();
        let body = mock.bodies(CHAT).pop().unwrap();
        match expected {
            Some(effort) => {
                assert_eq!(body["reasoning"], json!({"effort": effort}), "{id} {level}")
            }
            None => assert!(body.get("reasoning").is_none(), "{id} {level}: {body}"),
        }
    }
    for body in mock.bodies(CHAT) {
        assert!(body.get("reasoning_effort").is_none());
        assert!(!contains_key(&body, "include_reasoning"));
    }
    let models = adapter.list_models().await.unwrap();
    let levels = |id: &str| {
        models
            .iter()
            .find(|m| m.id == id)
            .unwrap()
            .reasoning_levels()
    };
    assert_eq!(levels("mock/text-small"), catalog_levels());
    assert_eq!(
        levels("mock/text-large"),
        [
            ReasoningLevel::Low,
            ReasoningLevel::Medium,
            ReasoningLevel::High
        ]
    );
    assert!(levels("mock/global-c").is_empty());
    let r2 = models.iter().find(|m| m.id == "mock/text-large").unwrap();
    assert_eq!(r2.max_completion_tokens, Some(4000));
    assert_eq!(r2.to_model().reasoning_levels, levels("mock/text-large"));
}

// spec: AI-005 [M4]
// spec: AI-021 [M4]
// Szenario `output-limit` → Grund `length`, kein Teilergebnis, genau 1 Request; `reasoning=40` → `reasoning_tokens`,
// `message.reasoning`/`reasoning_details` verworfen.
#[tokio::test]
async fn openrouter_output_limit_and_reasoning_tokens() {
    let mock = Mock::start().await;
    let adapter = mock.openrouter();
    let req = request(
        router("mock/text"),
        "MOCK-SCENARIO:output-limit x",
        Some(ok_schema()),
        ReasoningLevel::ModelDefault,
    );
    let reply = adapter.send(&req, None).await.unwrap();
    assert_eq!(reply.output_limit.as_deref(), Some("length"));
    let limit = reply.for_repair().unwrap_err();
    assert_eq!(limit.usage.output_tokens, Some(256));
    assert_eq!(
        limit.attempt_failure().error_code.as_deref(),
        Some("length")
    );
    assert_eq!(mock.requests_to(CHAT).len(), 1);

    let plain = adapter
        .send(
            &request(
                router("mock/text"),
                "MOCK-ID:home r",
                Some(ok_schema()),
                ReasoningLevel::ModelDefault,
            ),
            None,
        )
        .await
        .unwrap();
    let with = adapter
        .send(
            &request(
                router("mock/text"),
                "MOCK-ID:home MOCK-SCENARIO:reasoning=40 r",
                Some(ok_schema()),
                ReasoningLevel::ModelDefault,
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(with.usage.reasoning_tokens, Some(40));
    assert_eq!(plain.usage.reasoning_tokens, Some(0));
    assert_eq!(with.raw, plain.raw);
    let rendered = format!("{with:?}");
    assert!(!rendered.contains("MOCK-REASONING-TEXT") && !rendered.contains("MOCK-ENCRYPTED"));
}

// spec: AI-005 [M4]
// Ist-Kosten: Standardantwort → `usage.cost` (µUSD) = lokale Rechnung, verwertbar; `cost-mismatch` → doppelter Betrag;
// `no-cost` → keine gemeldeten Kosten; `byok` → nicht verwertbar (lokale Rechnung), `upstream_inference_cost` gemeldet.
#[tokio::test]
async fn openrouter_reported_cost_is_passed_through() {
    let mock = Mock::start().await;
    let adapter = mock.openrouter();
    let models = adapter.list_models().await.unwrap();
    let text = models.iter().find(|m| m.id == "mock/text").unwrap().clone();
    let computed = |usage: &os_ai::provider::Usage| {
        cost_of(
            &TokenCounts::plain(usage.input_tokens.unwrap(), usage.output_tokens.unwrap()),
            FeeCounts::one_request(0),
            &text.rates_for(usage.input_tokens.unwrap()).unwrap(),
            false,
        )
        .cost
        .0
    };
    let send = |scenario: &str| {
        let req = request(
            router("mock/text"),
            &format!("MOCK-ID:k09-long MOCK-SCENARIO:{scenario}"),
            None,
            ReasoningLevel::ModelDefault,
        );
        let adapter = &adapter;
        async move { adapter.send(&req, None).await.unwrap() }
    };
    let ok = send("ok").await;
    let cost = ok.reported_cost.unwrap();
    assert_eq!(cost.cost_micro_usd, Some(computed(&ok.usage)));
    assert_eq!(cost.usable(), Some(computed(&ok.usage)));
    assert_eq!(
        cost.upstream_inference_cost_micro_usd,
        Some(computed(&ok.usage))
    );
    assert_eq!(cost.is_byok, Some(false));
    let mismatch = send("cost-mismatch").await;
    assert_eq!(
        mismatch.reported_cost.unwrap().usable(),
        Some(2 * computed(&mismatch.usage))
    );
    let none = send("no-cost").await;
    assert_eq!(none.reported_cost.and_then(|c| c.usable()), None);
    let byok = send("byok").await;
    let c = byok.reported_cost.unwrap();
    assert_eq!(c.is_byok, Some(true));
    assert_eq!(c.usable(), None);
    assert_eq!(
        c.upstream_inference_cost_micro_usd,
        Some(computed(&byok.usage))
    );
    // Rundung: kaufmännisch auf µUSD; negative oder nicht numerische Werte sind nicht verwertbar.
    let parsed = |v: Value| openrouter::reported_cost_from_json(Some(&v)).unwrap();
    assert_eq!(parsed(json!({"cost": 5.8e-6})).cost_micro_usd, Some(6));
    assert_eq!(parsed(json!({"cost": 0.0001809})).cost_micro_usd, Some(181));
    assert_eq!(parsed(json!({"cost": -1.0})).usable(), None);
    assert_eq!(parsed(json!({"cost": "0.1"})).usable(), None);
    assert_eq!(
        openrouter::reported_cost_from_json(Some(&json!({"prompt_tokens": 1}))),
        None
    );
}

// spec: AI-005 [M4]
// Preisfelder: `overrides` (M-LC, Langkontext ab 1.000: 1.000 × 2 + 100 × 4 = 2.400 µUSD; 999 → 999 × 1 + 100 × 2 =
// 1.199 µUSD; `Long Context From 1000`), `internal_reasoning` (M-IR: Reservierung 1.000 × 10 µUSD = 10.000 µUSD),
// Cache-Preise gelesen.
#[tokio::test]
async fn openrouter_reads_overrides_internal_reasoning_and_cache_prices() {
    let mock = Mock::start().await;
    let models = mock.openrouter().list_models().await.unwrap();
    let find = |id: &str| models.iter().find(|m| m.id == id).unwrap().clone();
    let lc = find("mock/global-a");
    assert_eq!(lc.long_context_from(), Some(1000));
    let cost = |input: u64| {
        cost_of(
            &TokenCounts::plain(input, 100),
            FeeCounts::one_request(0),
            &lc.rates_for(input).unwrap(),
            false,
        )
        .cost
    };
    assert_eq!(cost(1000), MicroUsd(2_400));
    assert_eq!(cost(999), MicroUsd(1_199));
    let ir = find("mock/global-b");
    assert_eq!(ir.reserve_output_pico(0), Some(10_000_000));
    assert_eq!(
        ir.reserve_output_pico(0).unwrap() * 1_000 / 1_000_000,
        10_000
    );
    assert_eq!(find("mock/text").long_context_from(), None);
    let entry = json!({"id": "c", "pricing": {"prompt": "0.000001", "completion": "0.000002",
        "input_cache_read": "0.0000001", "input_cache_write": "0.00000125",
        "overrides": [{"min_prompt_tokens": 272000, "prompt": "0.0000002", "input_cache_write": "0.00000025"}]}});
    let m = openrouter::OpenRouterModel::from_json(&entry).unwrap();
    assert_eq!(m.cache_read_pico, 100_000);
    assert_eq!(m.cache_write_pico, 1_250_000);
    let big = m.rates_for(272_000).unwrap();
    assert_eq!(big.input_pico, 200_000);
    assert_eq!(big.cache_write_pico, 250_000);
    assert_eq!(
        big.output_pico, 2_000_000,
        "fehlende Felder behalten den Grundpreis"
    );
    assert_eq!(big.cache_read_pico, 100_000);
}

// spec: AI-005 [M4]
// Szenario `unknown-model` → `Error: 400 <model> is not a valid model ID`, `Error Code` leer, kein `Quota exceeded`,
// keine Sperre für andere Einträge; `user_id` wird nicht übernommen.
#[tokio::test]
async fn openrouter_unknown_model_is_a_plain_400() {
    let mock = Mock::start().await;
    let adapter = mock.openrouter();
    let req = request(
        router("mock/nope"),
        "MOCK-SCENARIO:unknown-model x",
        None,
        ReasoningLevel::ModelDefault,
    );
    let failure = adapter.send(&req, None).await.unwrap_err();
    assert_eq!(
        failure.error,
        ProviderError::Http {
            status: 400,
            message: "mock/nope is not a valid model ID".into()
        }
    );
    assert_eq!(failure.error_code, None);
    assert!(!format!("{failure:?}").contains("mock-user"));
    assert_eq!(
        failure.error.status(),
        Some(AiStatus::HttpError {
            code: 400,
            message: "mock/nope is not a valid model ID".into()
        })
    );
    assert!(!adapter.quota().is_tripped());
    let next = adapter
        .send(
            &request(
                router("mock/text"),
                "MOCK-ID:home weiter",
                None,
                ReasoningLevel::ModelDefault,
            ),
            None,
        )
        .await
        .unwrap();
    assert!(next.output_limit.is_none() && !next.is_refusal());
}

// spec: AI-001 [M4]
// Request-ID OpenRouter: Header `x-generation-id`, ersatzweise Antwort-`id`.
#[tokio::test]
async fn openrouter_request_id_comes_from_the_generation_header() {
    let mock = Mock::start().await;
    let reply = mock
        .openrouter()
        .send(
            &request(
                router("mock/text"),
                "MOCK-ID:home g",
                None,
                ReasoningLevel::ModelDefault,
            ),
            None,
        )
        .await
        .unwrap();
    let id = reply.provider_request_id.unwrap();
    assert!(id.starts_with("gen-mock-"), "{id}");
    let mut headers = HeaderMap::new();
    assert_eq!(
        openrouter::request_id(&json!({"id": "gen-body"}), &headers),
        Some("gen-body".into())
    );
    headers.insert("x-generation-id", "gen-header".parse().unwrap());
    assert_eq!(
        openrouter::request_id(&json!({"id": "gen-body"}), &headers),
        Some("gen-header".into())
    );
}

// ---------------------------------------------------------------------------------------------------------------
// AI-006 Ollama

// spec: AI-006 [M4]
// Ollama-Szenario `output-limit` → Grund `length`, genau 1 Request, Kosten 0; `reasoning_tokens` bleibt `null`, keine
// Request-ID.
#[tokio::test]
async fn ollama_output_limit_is_length_with_zero_cost() {
    let mock = Mock::start().await;
    let adapter = mock.ollama();
    let m = model("ollama", "mock-llama", ollama::ENDPOINT, Vec::new());
    let reply = adapter
        .send(
            &request(
                m.clone(),
                "MOCK-ID:k09-long MOCK-SCENARIO:output-limit",
                Some(ok_schema()),
                ReasoningLevel::ModelDefault,
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(reply.output_limit.as_deref(), Some("length"));
    assert_eq!(reply.provider_request_id, None);
    let limit = reply.for_repair().unwrap_err();
    assert_eq!(limit.usage.output_tokens, Some(16));
    assert_eq!(limit.usage.reasoning_tokens, None);
    assert_eq!(mock.requests_to(OLLAMA_CHAT).len(), 1);
    let cost = cost_of(
        &TokenCounts::from_usage(&limit.usage),
        FeeCounts::one_request(0),
        &ollama::rates(),
        false,
    );
    assert_eq!(cost.cost, MicroUsd(0));
    // Ollama: nur `model-default`; eine gesetzte Stufe ändert den Request nicht.
    adapter
        .send(
            &request(m, "MOCK-ID:home s", None, ReasoningLevel::Off),
            None,
        )
        .await
        .unwrap();
    let body =
        serde_json::from_str::<Value>(&mock.requests_to(OLLAMA_CHAT).pop().unwrap().body).unwrap();
    assert!(!contains_key(&body, "reasoning") && !contains_key(&body, "think"));
}

// ---------------------------------------------------------------------------------------------------------------
// Nacharbeit nach der Abnahme (Befund B1 und Entscheidungen des Orchestrators, 24.11.2026)

async fn global_scenario(mock: &Mock, provider: &str, scenario: &str, count: usize) {
    let r = client()
        .post(format!("{}/scenario", mock.server.base_url("__mock")))
        .header("content-type", "application/json")
        .body(json!({"provider": provider, "scenario": scenario, "count": count}).to_string())
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success());
}

// spec: AI-001 [M4]
// Befund B1: Auch Fehlerantworten tragen die Anbieter-Request-ID bis ins Fehlerergebnis eines Versuchs bzw. Eintrags –
// Anthropic Header `request-id`, OpenAI Responses und Decisions `x-request-id`, OpenRouter `x-generation-id`.
#[tokio::test]
async fn error_results_carry_the_provider_request_id() {
    let mock = Mock::start().await;
    let a = model("anthropic", "mock-claude", "messages", Vec::new());
    let failure = mock
        .anthropic()
        .send(
            &request(a, "MOCK-SCENARIO:401 x", None, ReasoningLevel::ModelDefault),
            None,
        )
        .await
        .unwrap_err();
    let id = failure
        .provider_request_id
        .clone()
        .expect("Anthropic-Request-ID");
    assert!(id.starts_with("req_mock_"), "{id}");

    let failure = mock
        .openrouter()
        .send(
            &request(
                router("mock/text"),
                "MOCK-SCENARIO:402-credits x",
                None,
                ReasoningLevel::ModelDefault,
            ),
            None,
        )
        .await
        .unwrap_err();
    let id = failure
        .provider_request_id
        .clone()
        .expect("OpenRouter-Request-ID");
    assert!(id.starts_with("gen-mock-"), "{id}");

    // OpenAI Responses über den Limiter: Fehlerergebnis und Protokolldaten tragen `x-request-id`.
    let responses = OpenAiResponses::new(mock.openai_http());
    let (result, meta) = responses
        .run(
            &limiter(),
            &request(
                luna(Vec::new()),
                "MOCK-SCENARIO:401 x",
                None,
                ReasoningLevel::ModelDefault,
            ),
            10,
        )
        .await;
    let error = result.unwrap_err();
    let id = error
        .provider_request_id
        .clone()
        .expect("Responses-Request-ID");
    assert!(id.starts_with("req_mock_"), "{id}");
    assert_eq!(meta.provider_request_id, Some(id));

    // Decisions (Q00-N6): Antwort ohne `id`, Kennung allein aus `x-request-id`.
    let run = OpenAiDecisions::new(mock.openai_http())
        .decide(
            &limiter(),
            &DecisionInput::text("MOCK-SCENARIO:401 Seite"),
            &[Question::predicate("q1", "Is it a product page?")],
            &DecideOptions::default(),
        )
        .await;
    let id = run.requests[0]
        .meta
        .provider_request_id
        .clone()
        .expect("Decisions-Request-ID");
    assert!(id.starts_with("req_mock_"), "{id}");
}

// spec: AI-002 [M4]
// spec: AI-003 [M4]
// spec: AI-005 [M4]
// Altes Feld `Request.effort` und ausdrückliche Reasoning-Stufe: die Stufe gewinnt, es entstehen nie widersprüchliche
// Parameter; ohne Stufe wirkt `effort` bei Anthropic wie bisher, OpenAI und OpenRouter senden es nie.
#[tokio::test]
async fn explicit_reasoning_level_wins_over_the_old_effort_field() {
    let mock = Mock::start().await;
    let anthropic = mock.anthropic();
    let models = anthropic.list_models().await.unwrap();
    let a = models
        .iter()
        .find(|m| m.id == "mock-claude")
        .unwrap()
        .to_model();
    let mut req = request(a.clone(), "MOCK-ID:home e1", None, ReasoningLevel::Low);
    req.effort = Some("max".into());
    anthropic.send(&req, None).await.unwrap();
    let body = mock.bodies(MESSAGES).pop().unwrap();
    assert_eq!(
        anthropic_params(&body),
        (Some("adaptive".into()), Some("low".into()))
    );
    let mut req = request(a.clone(), "MOCK-ID:home e2", None, ReasoningLevel::Off);
    req.effort = Some("high".into());
    anthropic.send(&req, None).await.unwrap();
    let body = mock.bodies(MESSAGES).pop().unwrap();
    assert_eq!(anthropic_params(&body), (Some("disabled".into()), None));
    let mut req = request(a, "MOCK-ID:home e3", None, ReasoningLevel::ModelDefault);
    req.effort = Some("high".into());
    anthropic.send(&req, None).await.unwrap();
    let body = mock.bodies(MESSAGES).pop().unwrap();
    assert_eq!(anthropic_params(&body), (None, Some("high".into())));

    let mut req = request(
        luna(catalog_levels()),
        "MOCK-ID:home e4",
        None,
        ReasoningLevel::Low,
    );
    req.effort = Some("high".into());
    OpenAiResponses::new(mock.openai_http())
        .send(&req, None, None)
        .await
        .unwrap();
    let body = mock.bodies(RESPONSES).pop().unwrap();
    assert_eq!(body["reasoning"], json!({"effort": "low"}));
    assert!(body.get("effort").is_none() && body.get("reasoning_effort").is_none());

    let mut req = request(
        router("mock/text-small"),
        "MOCK-ID:home e5",
        None,
        ReasoningLevel::Low,
    );
    req.effort = Some("high".into());
    mock.openrouter().send(&req, None).await.unwrap();
    let body = mock.bodies(CHAT).pop().unwrap();
    assert_eq!(body["reasoning"], json!({"effort": "low"}));
    assert!(body.get("effort").is_none() && body.get("reasoning_effort").is_none());
}

// spec: AI-005 [M4]
// OpenRouter ohne geladene Modellliste: kein `reasoning`-Parameter (konservativ), aber nicht still – der Zähler
// `reasoning_skipped_without_model_list` steigt (Ersatz für die Debug-Logzeile; schreibt der Aufrufer ins Log).
#[tokio::test]
async fn openrouter_counts_requests_sent_without_reasoning_for_lack_of_a_model_list() {
    let mock = Mock::start().await;
    let adapter = mock.openrouter();
    // Die erste Anfrage (Modellliste) scheitert, der eigentliche Request geht danach normal hinaus.
    global_scenario(&mock, "openrouter", "500-always", 1).await;
    adapter
        .send(
            &request(
                router("mock/text-small"),
                "MOCK-ID:home s",
                None,
                ReasoningLevel::Off,
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(adapter.reasoning_skipped_without_model_list(), 1);
    let body = mock.bodies(CHAT).pop().unwrap();
    assert!(body.get("reasoning").is_none());
    // Ohne ausdrückliche Stufe wird nichts gezählt.
    adapter
        .send(
            &request(
                router("mock/text-small"),
                "MOCK-ID:home t",
                None,
                ReasoningLevel::ModelDefault,
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(adapter.reasoning_skipped_without_model_list(), 1);
    // Mit geladener Liste geht `reasoning` hinaus, der Zähler bleibt.
    adapter
        .send(
            &request(
                router("mock/text-small"),
                "MOCK-ID:home u",
                None,
                ReasoningLevel::Off,
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        mock.bodies(CHAT).pop().unwrap()["reasoning"],
        json!({"effort": "none"})
    );
    assert_eq!(adapter.reasoning_skipped_without_model_list(), 1);
}

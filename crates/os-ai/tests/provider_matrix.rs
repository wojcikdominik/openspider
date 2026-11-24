//! Foundation contract against explicit in-memory stubs, never a live adapter.
use os_ai::provider::*;
use os_ai::{AiStatus, PreviewSwitch};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

fn fixture() -> Vec<Model> {
    serde_json::from_str(r#"[
{"provider":"anthropic","id":"mock","endpoint":"messages","capability_source":"anthropic_models","capabilities":{"text":true,"structured_output":true,"vision":true,"embeddings":false,"decisions":false,"batch":true,"prices_via_api":false,"local":false}},
{"provider":"openai","id":"mock","endpoint":"responses","capability_source":"model_catalog","capabilities":{"text":true,"structured_output":true,"vision":true,"embeddings":false,"decisions":false,"batch":true,"prices_via_api":false,"local":false}},
{"provider":"openai","id":"mock","endpoint":"decisions","capability_source":"model_catalog","capabilities":{"text":false,"structured_output":false,"vision":true,"embeddings":false,"decisions":true,"batch":false,"prices_via_api":false,"local":false}},
{"provider":"openrouter","id":"mock","endpoint":"chat-completions","capability_source":"open_router_models","capabilities":{"text":true,"structured_output":true,"vision":true,"embeddings":true,"decisions":false,"batch":false,"prices_via_api":true,"local":false}},
{"provider":"ollama","id":"mock","endpoint":"chat-completions","capability_source":"model_catalog","capabilities":{"text":true,"structured_output":true,"vision":true,"embeddings":true,"decisions":false,"batch":false,"prices_via_api":false,"local":true}},
{"provider":"gemini","id":"mock","endpoint":"responses","capability_source":"model_catalog","capabilities":{"text":true,"structured_output":true,"vision":true,"embeddings":true,"decisions":false,"batch":false,"prices_via_api":false,"local":false}}
]"#).unwrap()
}
struct Stub {
    model: Model,
    calls: AtomicUsize,
}
impl AiProvider for Stub {
    fn provider_id(&self) -> &str {
        &self.model.provider
    }
    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async { Ok(vec![self.model.clone()]) })
    }
    fn execute(&self, _: Request) -> ProviderFuture<'_, Response> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(Response {
                content: Some(json!({"ok":true})),
                usage: Usage::default(),
                refusal: false,
                provider_request_id: Some("stub-request".into()),
                duration_ms: 1,
            })
        })
    }
}
fn request(model: Model) -> Request {
    Request {
        model,
        required: Capabilities::default(),
        input: vec![Input::Text {
            text: "Return ok true".into(),
        }],
        instructions: "Return JSON".into(),
        questions: vec![],
        response_schema: None,
        max_tokens: Some(10),
        temperature: None,
        effort: None,
        reasoning: Default::default(),
    }
}
// spec: AI-001
#[tokio::test]
async fn identical_parsed_contract_for_all_stub_providers() {
    let schema = json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false});
    for model in fixture() {
        let stub = Stub {
            model: model.clone(),
            calls: AtomicUsize::new(0),
        };
        assert_eq!(stub.models().await.unwrap(), vec![model.clone()]);
        let mut req = request(model);
        if req.model.capabilities.decisions {
            req.required.decisions = true;
            req.questions =
                vec![json!({"name":"ok","type":"predicate","instructions":"Is this OK?"})];
        } else {
            req.response_schema = Some(schema.clone());
        }
        let Outcome::Completed(response) = execute(PreviewSwitch::ON, &stub, req).await else {
            panic!("contract failed");
        };
        assert_eq!(response.content, Some(json!({"ok":true})));
        assert!(jsonschema::is_valid(
            &schema,
            response.content.as_ref().unwrap()
        ));
        assert_eq!(
            serde_json::to_value(response.usage).unwrap(),
            json!({"input_tokens":null,"output_tokens":null,"cached_input_tokens":null,"cache_write_tokens":null})
        );
        assert_eq!(stub.calls.load(Ordering::SeqCst), 1);
    }
}
// spec: AI-001
#[tokio::test]
async fn image_and_schema_guards_prevent_any_incapable_or_wrong_provider_request() {
    for mut model in fixture() {
        model.capabilities.vision = false;
        let stub = Stub {
            model: model.clone(),
            calls: AtomicUsize::new(0),
        };
        let mut req = request(model.clone());
        req.input.push(Input::Image {
            media_type: "image/png".into(),
            base64: "fixture".into(),
        });
        assert_eq!(
            execute(PreviewSwitch::ON, &stub, req).await,
            Outcome::Status(AiStatus::SkippedNoCapableModel)
        );
        model.capabilities.structured_output = false;
        let mut req = request(model.clone());
        req.response_schema = Some(json!({"type":"object"}));
        assert_eq!(
            execute(PreviewSwitch::ON, &stub, req).await,
            Outcome::Status(AiStatus::SkippedNoCapableModel)
        );
        model.provider = "wrong".into();
        assert_eq!(
            execute(PreviewSwitch::ON, &stub, request(model)).await,
            Outcome::Status(AiStatus::SkippedNoCapableModel)
        );
        assert_eq!(stub.calls.load(Ordering::SeqCst), 0);
    }
}
// spec: AI-001
#[test]
fn missing_usage_fields_are_null_and_never_fabricated_zeroes() {
    let usage:Usage=serde_json::from_value(json!({"input_tokens":7,"output_tokens":null,"cached_input_tokens":0,"cache_write_tokens":null})).unwrap();
    assert_eq!(usage.input_tokens, Some(7));
    assert_eq!(usage.output_tokens, None);
    assert_eq!(usage.cached_input_tokens, Some(0));
    assert_eq!(
        serde_json::from_value::<Usage>(json!({})).unwrap(),
        Usage::default()
    );
}
// spec: AI-001
#[test]
fn every_capability_is_required_individually() {
    let all = Capabilities {
        text: true,
        structured_output: true,
        vision: true,
        embeddings: true,
        decisions: true,
        batch: true,
        prices_via_api: true,
        local: true,
    };
    for index in 0..8 {
        let mut missing = serde_json::to_value(all).unwrap();
        let key = missing
            .as_object()
            .unwrap()
            .keys()
            .nth(index)
            .unwrap()
            .clone();
        missing[&key] = json!(false);
        assert!(!serde_json::from_value::<Capabilities>(missing)
            .unwrap()
            .supports(all));
    }
}

// spec: AI-001
// spec: AI-020
#[tokio::test]
async fn refusal_and_provider_errors_return_status_without_crawl_control() {
    struct ResultStub {
        response: Result<Response, ProviderError>,
    }
    impl AiProvider for ResultStub {
        fn provider_id(&self) -> &str {
            "anthropic"
        }
        fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn execute(&self, _: Request) -> ProviderFuture<'_, Response> {
            Box::pin(async { self.response.clone() })
        }
    }
    let model = fixture().remove(0);
    let cases = [
        (Err(ProviderError::MissingApiKey), AiStatus::MissingApiKey),
        (Err(ProviderError::QuotaExceeded), AiStatus::QuotaExceeded),
        (
            Err(ProviderError::Timeout { seconds: 30 }),
            AiStatus::Timeout { seconds: 30 },
        ),
        (
            Ok(Response {
                content: None,
                usage: Usage::default(),
                refusal: true,
                provider_request_id: None,
                duration_ms: 0,
            }),
            AiStatus::Refusal,
        ),
    ];
    let transport = ProviderError::Transport {
        message: "fixture connection failure".into(),
    };
    assert_eq!(
        execute(
            PreviewSwitch::ON,
            &ResultStub {
                response: Err(transport.clone())
            },
            request(model.clone())
        )
        .await,
        Outcome::ProviderFailure(transport)
    );
    for (response, status) in cases {
        assert_eq!(
            execute(
                PreviewSwitch::ON,
                &ResultStub { response },
                request(model.clone())
            )
            .await,
            Outcome::Status(status)
        );
    }
}

// spec: AI-001
#[test]
fn resource_schema_fixtures_validate_without_remote_reference_resolution() {
    let response: serde_json::Value = serde_json::from_str(include_str!(
        "../../../resources/ai/schemas/contract-response.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&response).unwrap();
    assert!(validator.is_valid(&json!({"ok":true})));
    assert!(!validator.is_valid(&json!({"ok":true,"extra":false})));
    let pricing: serde_json::Value = serde_json::from_str(include_str!(
        "../../../resources/ai/schemas/pricing.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&pricing).unwrap();
    let mut entry = json!({"provider":"anthropic","model":"mock","endpoint":"messages","input_per_million":1,"output_per_million":5,"cache_read_per_million":0,"cache_write_per_million":0,"batch_discount":0.5,"regional_uplift_percent":0,"per_request":0,"per_image":0,"image_tokens":85,"valid_from":"2026-11-09","source_url":"https://mock.test/pricing","verified_at":"2026-11-09"});
    assert!(validator.is_valid(&json!([entry.clone()])));
    entry["vision"] = json!(true);
    assert!(!validator.is_valid(&json!([entry.clone()])));
    entry.as_object_mut().unwrap().remove("vision");
    entry["input_per_million"] = json!(-1);
    assert!(!validator.is_valid(&json!([entry])));
    let defaults: serde_json::Value = serde_json::from_str(include_str!(
        "../../../resources/ai/schemas/task-defaults.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&defaults).unwrap();
    assert!(validator.is_valid(&json!({"decision":{"model":{"provider":"openai","model":"mock","endpoint":"decisions"},"fallback":[]}})));
    assert!(!validator.is_valid(&json!({"decision":{"fallback":[]}})));
}

// spec: AI-001
// Plan M4 E1 (E-2 b): Schalter „KI (Vorschau)“ aus → kein Request an irgendeinen Anbieter.
#[tokio::test]
async fn preview_switch_off_sends_no_request_to_any_provider() {
    let settings = os_model::settings::AppSettings::default();
    let switch = PreviewSwitch::from_settings(&settings);
    assert_eq!(switch, PreviewSwitch::OFF);
    for model in fixture() {
        let stub = Stub {
            model: model.clone(),
            calls: AtomicUsize::new(0),
        };
        assert_eq!(
            execute(switch, &stub, request(model.clone())).await,
            Outcome::Disabled
        );
        // Auch eine an sich nicht routbare Anfrage endet vor dem Routing.
        let mut wrong = model;
        wrong.provider = "wrong".into();
        assert_eq!(
            execute(switch, &stub, request(wrong)).await,
            Outcome::Disabled
        );
        assert_eq!(stub.calls.load(Ordering::SeqCst), 0);
    }
    let mut on = settings;
    on.ai_app.preview_enabled = true;
    assert!(PreviewSwitch::from_settings(&on).is_on());
}

// spec: AI-001
// Plan M4 E1 (E-2 b): Schalter aus → kein KI-Schritt läuft, auch wenn das Profil KI einschaltet.
#[test]
fn preview_switch_off_disables_every_ai_step_of_a_profile() {
    let fixture = "schema_version = 2\n[ai]\nenabled = true\n[embeddings]\nenabled = true\n[analysis]\nembeddings = true\nai_search = true\n";
    let profile = os_model::CrawlConfig::load_str(fixture).unwrap().config;
    let mut run = profile.clone();
    PreviewSwitch::OFF.restrict(&mut run);
    assert!(!run.ai.enabled);
    assert!(!run.embeddings.enabled);
    for step in [
        os_model::AnalysisStep::Embeddings,
        os_model::AnalysisStep::AiSearch,
    ] {
        assert!(profile.analysis.step_enabled(step));
        assert!(!run.analysis.step_enabled(step));
    }
    let mut unchanged = profile.clone();
    PreviewSwitch::ON.restrict(&mut unchanged);
    assert_eq!(unchanged, profile);
}

// spec: AI-001
// OpenAI Decisions: „Decisions (Text und Bild)“ – ein Bild in einer Entscheidungsfrage braucht keine Vision-Fähigkeit,
// jede andere Bildanfrage weiterhin schon.
#[tokio::test]
async fn decision_requests_carry_images_without_vision_but_other_image_requests_still_need_it() {
    let model = fixture()
        .into_iter()
        .find(|m| m.endpoint == "decisions")
        .unwrap();
    let mut decisions_only = model.clone();
    decisions_only.capabilities = Capabilities {
        decisions: true,
        ..Capabilities::default()
    };
    let stub = Stub {
        model: decisions_only.clone(),
        calls: AtomicUsize::new(0),
    };
    let image = Input::Image {
        media_type: "image/png".into(),
        base64: "fixture".into(),
    };
    let mut decision = request(decisions_only.clone());
    decision.required.decisions = true;
    decision.questions = vec![json!({"name":"ok","type":"predicate","instructions":"Is this OK?"})];
    decision.input.push(image.clone());
    assert!(matches!(
        execute(PreviewSwitch::ON, &stub, decision).await,
        Outcome::Completed(_)
    ));
    let mut plain = request(decisions_only);
    plain.input.push(image);
    assert_eq!(
        execute(PreviewSwitch::ON, &stub, plain).await,
        Outcome::Status(AiStatus::SkippedNoCapableModel)
    );
    assert_eq!(stub.calls.load(Ordering::SeqCst), 1);
}

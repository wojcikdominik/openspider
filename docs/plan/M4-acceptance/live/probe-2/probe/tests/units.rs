//! Einzeltests: Anfragekörper (Formen aus 08 und `adapters/anthropic/reasoning.rs`), Beobachtungen, Modellwahl aus
//! der Liste, Kosten, Bereinigung.

use probe::plan::{self, FormSource, ModelNeed, Provider};
use probe::record::{self, non_disableable_anthropic, observe};
use probe::requests::body;
use serde_json::{json, Value};

fn b(id: &str) -> Value {
    body(plan::step_by_id(id), "m").expect("Körper")
}

// spec: AI-002 (Nachtrag 24.11.2026, Umsetzung der Reasoning-Stufe), V-A6 a
#[test]
fn anthropic_forms() {
    let schema = |v: &Value| v["output_config"]["format"]["type"] == "json_schema";
    let d = b("anthropic.default");
    assert!(schema(&d) && d.get("thinking").is_none());
    assert!(d["output_config"].get("effort").is_none());
    let off = b("anthropic.off");
    assert_eq!(off["thinking"], json!({"type": "disabled"}));
    assert!(off["output_config"].get("effort").is_none());
    let low = b("anthropic.low");
    assert_eq!(low["thinking"], json!({"type": "adaptive"}));
    assert_eq!(low["output_config"]["effort"], "low");
    assert!(schema(&low));
    assert!(b("anthropic.adaptive_only")["output_config"]
        .get("effort")
        .is_none());
    let eo = b("anthropic.effort_only");
    assert!(eo.get("thinking").is_none());
    assert_eq!(eo["output_config"]["effort"], "low");
    let conflict = b("anthropic.off_with_effort");
    assert_eq!(conflict["thinking"]["type"], "disabled");
    assert_eq!(conflict["output_config"]["effort"], "low");
    assert_eq!(
        b("anthropic.off_not_disableable")["thinking"]["type"],
        "disabled"
    );
    for v in [&d, &off, &low, &conflict] {
        assert!(v.get("budget_tokens").is_none() && v["thinking"].get("budget_tokens").is_none());
        assert_ne!(v["thinking"]["type"], "enabled");
        assert_eq!(v["max_tokens"], plan::SCHEMA_MAX_TOKENS);
    }
}

// spec: AI-003, AI-004 (Nachträge 24.11.2026), V-A6 b, V-A8
#[test]
fn openai_forms() {
    for id in [
        "openai.default",
        "openai.off",
        "openai.low",
        "openai.minimal",
        "openai.invalid_effort",
    ] {
        let v = b(id);
        assert_eq!(v["store"], false, "{id}");
        assert_eq!(v["text"]["format"]["strict"], true);
        assert!(
            v.get("prompt_cache_retention").is_none(),
            "V-A8: nichts senden"
        );
    }
    assert!(b("openai.default").get("reasoning").is_none());
    assert_eq!(b("openai.off")["reasoning"], json!({"effort": "none"}));
    assert_eq!(b("openai.low")["reasoning"], json!({"effort": "low"}));
    assert_eq!(b("openai.minimal")["reasoning"]["effort"], "minimal");
    assert_eq!(
        b("openai.invalid_effort")["reasoning"]["effort"],
        plan::INVALID_EFFORT
    );
    let plain = b("openai.decisions_plain");
    assert!(plain.get("store").is_none() && plain.get("reasoning").is_none());
    assert_eq!(plain["questions"][0]["type"], "predicate");
    assert_eq!(b("openai.decisions_store_false")["store"], false);
}

// spec: AI-005 (Nachtrag 24.11.2026), V-A6 c
#[test]
fn openrouter_forms() {
    for id in [
        "openrouter.default",
        "openrouter.off",
        "openrouter.low",
        "openrouter.enabled_false",
    ] {
        let v = b(id);
        assert_eq!(v["provider"]["require_parameters"], true);
        assert_eq!(v["provider"]["data_collection"], "deny");
        assert_eq!(v["response_format"]["type"], "json_schema");
        assert!(v.get("reasoning_effort").is_none() && v.get("include_reasoning").is_none());
    }
    assert!(b("openrouter.default").get("reasoning").is_none());
    assert_eq!(b("openrouter.off")["reasoning"], json!({"effort": "none"}));
    assert_eq!(b("openrouter.low")["reasoning"], json!({"effort": "low"}));
    assert_eq!(
        b("openrouter.enabled_false")["reasoning"],
        json!({"enabled": false})
    );
}

// spec: V-A6, V-A8 (Plan)
#[test]
fn plan_shape() {
    assert_eq!(plan::STEPS.len(), 21);
    for p in Provider::ALL {
        assert!(plan::steps(p).count() <= p.max_requests());
        assert_eq!(
            plan::steps(p)
                .filter(|s| s.model == ModelNeed::None)
                .count(),
            1
        );
    }
    let adapter = plan::STEPS
        .iter()
        .filter(|s| s.source == FormSource::Adapter)
        .count();
    assert_eq!(adapter, 10);
    for s in plan::STEPS {
        assert_eq!(body(s, "m").is_none(), s.method == "GET", "{}", s.id);
    }
}

// spec: AI-002, AI-003, AI-005 (usage-Felder für Reasoning)
#[test]
fn observations_per_provider() {
    let anthropic = json!({"content": [{"type": "thinking", "signature": "x"}, {"type": "text", "text": "{\"a\":1}"}],
        "stop_reason": "end_turn", "usage": {"output_tokens": 40, "output_tokens_details": {"thinking_tokens": 10}}});
    let o = observe(&anthropic);
    assert_eq!(o["reasoning_tokens"], 10);
    assert_eq!(o["answer_is_json"], true);
    assert_eq!(o["thinking_blocks"], 1);
    let openai = json!({"status": "incomplete", "incomplete_details": {"reason": "max_output_tokens"}, "store": false,
        "prompt_cache_retention": "24h", "reasoning": {"effort": "none"},
        "output": [{"type": "reasoning"}], "usage": {"output_tokens": 200, "output_tokens_details": {"reasoning_tokens": 200}}});
    let o = observe(&openai);
    assert_eq!(o["answer_present"], false);
    assert_eq!(o["incomplete_reason"], "max_output_tokens");
    assert_eq!(o["store"], false);
    assert_eq!(o["reasoning_echo"]["effort"], "none");
    let router = json!({"choices": [{"finish_reason": "length", "message": {"content": null}}],
        "usage": {"completion_tokens": 200, "completion_tokens_details": {"reasoning_tokens": 0}}});
    let o = observe(&router);
    assert_eq!(o["reasoning_tokens"], 0);
    assert_eq!(o["stop"], "length");
    assert_eq!(o["answer_present"], false);
    let error = json!({"error": {"type": "invalid_request_error", "code": "unsupported_value", "message": "bad"}});
    let o = observe(&error);
    assert_eq!(o["error_code"], "unsupported_value");
    assert!(o["reasoning_tokens"].is_null());
    let decisions =
        json!({"answers": [{"name": "x", "type": "predicate"}], "usage": {"input_tokens": 5}});
    assert_eq!(
        observe(&decisions)["decisions_answer_types"],
        json!(["predicate"])
    );
}

// spec: AI-002 (capabilities.thinking.types)
#[test]
fn non_disableable_model_from_the_list() {
    let body = json!({"data": [
        {"id": "a", "capabilities": {"thinking": {"supported": true, "types": {"disabled": {"supported": true}}}}},
        {"id": "b", "capabilities": {"thinking": {"supported": false}}},
        {"id": "c", "capabilities": {}},
        {"id": "d", "capabilities": {"thinking": {"supported": true, "types": {"adaptive": {"supported": true}}}}},
        {"id": "e", "capabilities": {"thinking": {"supported": true, "types": {"disabled": {"supported": false}}}}}
    ]});
    assert_eq!(non_disableable_anthropic(&body).unwrap().id, "d");
    assert!(non_disableable_anthropic(&json!({"data": [body["data"][0].clone()]})).is_none());
    let caps = record::thinking_caps(&body["data"][4]).unwrap();
    assert_eq!(caps.disabled, Some(false));
}

// spec: AI-013 (Kostenrechnung der Probe)
#[test]
fn costs() {
    let c = plan::cost_usd(&plan::CONSERVATIVE, 1_000_000, 0);
    assert!((c - 30.0).abs() < 1e-9);
    let models = plan::step_by_id("anthropic.models");
    assert_eq!(plan::worst_case_usd(models, 1000, &plan::CONSERVATIVE), 0.0);
    let dec = plan::step_by_id("openai.decisions_plain");
    let w = plan::worst_case_usd(dec, 200, &plan::CONSERVATIVE);
    assert!((w - 164.0 * 30e-6).abs() < 1e-12, "Decisions ohne Ausgabe");
}

// spec: AI-009 (Bereinigung wie Probe 1)
#[test]
fn sanitize_rules_carried_over() {
    let v = json!({"id": "claude-haiku-5-5", "request_id": "req_1", "workspace_id": "w", "signature": "s",
                   "store": false, "prompt_cache_retention": "24h", "label": "poor"});
    let s = record::sanitize(&v);
    assert_eq!(s["id"], "claude-haiku-5-5");
    assert_eq!(s["request_id"], "req_…(ausgelassen: Anbieter-ID)");
    assert_eq!(s["workspace_id"], record::OMITTED_ACCOUNT);
    assert!(s["signature"].as_str().unwrap().starts_with("(ausgelassen"));
    assert_eq!(s["store"], false);
    assert_eq!(s["prompt_cache_retention"], "24h");
    assert_eq!(record::sanitize(&s), s);
}

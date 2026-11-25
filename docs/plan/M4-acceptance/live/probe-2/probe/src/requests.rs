//! Anfragekörper der Probe Q00b: dieselbe kleine Schema-Aufgabe je Anbieter, nur der Reasoning- bzw.
//! Speicherparameter variiert. Grundform nach 08 (AI-002, AI-003, AI-004, AI-005 mit Nachträgen vom 24.11.2026);
//! Reasoning-Formen genau wie `adapters/anthropic/reasoning.rs` (Zweig ai/k09, `edca3073`) bzw. die benannten
//! Alternativen.

use crate::plan::{Provider, Step, INVALID_EFFORT};
use serde_json::{json, Value};

pub const SYSTEM_TEXT: &str =
    "You are the endpoint of an API compatibility probe. Answer as briefly as possible.";
pub const STRUCT_PROMPT: &str = "Page text: \"Red running shoe, 49 EUR, in stock, add to cart.\" Estimate calibrated probabilities for the page type.";
pub const DECISION_INPUT: &str = "URL: https://shop.example/red-running-shoe\nTitle: Red running shoe - Example Shop\nText: Red running shoe for road running. Price 49 EUR. In stock. Add to cart.";
pub const SCHEMA_NAME: &str = "openspider_probe";

/// Antwortschema des Fallbacks für eine `choice`-Frage (AI-034), wie Probe 1.
pub fn fallback_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "probabilities": {
                "type": "object",
                "properties": {
                    "product": {"type": "number"},
                    "category": {"type": "number"},
                    "other": {"type": "number"}
                },
                "required": ["product", "category", "other"],
                "additionalProperties": false
            },
            "abstain": {"type": "boolean"}
        },
        "required": ["probabilities", "abstain"],
        "additionalProperties": false
    })
}

/// Eine `predicate`-Frage (AI-004), Name nach AI-030.
pub fn decision_question() -> Value {
    json!([{
        "name": "soft_404",
        "type": "predicate",
        "instructions": "Answer true if the page tells the visitor that the requested product no longer exists, although the page itself loads normally."
    }])
}

fn variant(step: &Step) -> &'static str {
    step.id.split_once('.').map(|(_, v)| v).unwrap_or("")
}

/// Körper einer Anfrage (`None` für GET).
pub fn body(step: &Step, model: &str) -> Option<Value> {
    let v = variant(step);
    if v == "models" {
        return None;
    }
    Some(match step.provider {
        Provider::Anthropic => anthropic(v, model, step.max_output_tokens),
        Provider::OpenAi => openai(v, model, step.max_output_tokens),
        Provider::OpenRouter => openrouter(v, model, step.max_output_tokens),
    })
}

fn anthropic(v: &str, model: &str, max_tokens: u64) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": SYSTEM_TEXT,
        "messages": [{"role": "user", "content": STRUCT_PROMPT}],
        "output_config": {"format": {"type": "json_schema", "schema": fallback_schema()}},
    });
    let disabled = json!({"type": "disabled"});
    let adaptive = json!({"type": "adaptive"});
    match v {
        "off" | "off_not_disableable" => body["thinking"] = disabled,
        "low" => {
            body["thinking"] = adaptive;
            body["output_config"]["effort"] = json!("low");
        }
        "adaptive_only" => body["thinking"] = adaptive,
        "effort_only" => body["output_config"]["effort"] = json!("low"),
        "off_with_effort" => {
            body["thinking"] = disabled;
            body["output_config"]["effort"] = json!("low");
        }
        _ => {}
    }
    body
}

fn openai(v: &str, model: &str, max_tokens: u64) -> Value {
    if let Some(store) = v.strip_prefix("decisions_") {
        let mut body =
            json!({"model": model, "input": DECISION_INPUT, "questions": decision_question()});
        if store == "store_false" {
            body["store"] = json!(false);
        }
        return body;
    }
    let mut body = json!({
        "model": model,
        "instructions": SYSTEM_TEXT,
        "input": STRUCT_PROMPT,
        "max_output_tokens": max_tokens,
        "store": false,
        "text": {"format": {"type": "json_schema", "name": SCHEMA_NAME, "schema": fallback_schema(), "strict": true}},
    });
    let effort = match v {
        "off" => Some("none"),
        "low" => Some("low"),
        "minimal" => Some("minimal"),
        "invalid_effort" => Some(INVALID_EFFORT),
        _ => None,
    };
    if let Some(e) = effort {
        body["reasoning"] = json!({"effort": e});
    }
    body
}

fn openrouter(v: &str, model: &str, max_tokens: u64) -> Value {
    let mut body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_TEXT},
            {"role": "user", "content": STRUCT_PROMPT}
        ],
        "max_tokens": max_tokens,
        "response_format": {"type": "json_schema", "json_schema": {"name": SCHEMA_NAME, "strict": true, "schema": fallback_schema()}},
        "provider": {"data_collection": "deny", "require_parameters": true},
    });
    match v {
        "off" => body["reasoning"] = json!({"effort": "none"}),
        "low" => body["reasoning"] = json!({"effort": "low"}),
        "enabled_false" => body["reasoning"] = json!({"enabled": false}),
        _ => {}
    }
    body
}

/// Zeichen der Nutzlast (für die Obergrenze).
pub fn payload_chars(value: &Value) -> u64 {
    value.to_string().chars().count() as u64
}

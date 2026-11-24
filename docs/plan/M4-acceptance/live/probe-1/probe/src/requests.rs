//! Anfragekörper nach dem Wortlaut von 08 (roh) und die gleichwertigen `os-ai`-Requests (Adapter).
//!
//! Wo 08 einen Feldaufbau nicht nennt, folgt der Rohkörper der Auslegung des Adapters (K01/K02-Bericht); das steht im
//! README unter „Auslegungen der Probe“.

use crate::plan::{
    Provider, Step, Via, SPEC_DECISIONS_MODEL, STRUCTURED_MAX_TOKENS, TEXT_MAX_TOKENS,
};
use os_ai::adapters::openai_decisions::Question;
use os_ai::provider::{Capabilities, CapabilitySource, Input, Model, Request};
use serde_json::{json, Value};

pub const SYSTEM_TEXT: &str =
    "You are the endpoint of an API compatibility probe. Answer as briefly as possible.";
pub const TEXT_PROMPT: &str = "Reply with exactly one word: pong";
pub const STRUCT_PROMPT: &str = "Page text: \"Red running shoe, 49 EUR, in stock, add to cart.\" Estimate calibrated probabilities for the page type.";
pub const LIMITS_PROMPT: &str = "Page text: \"Red running shoe, 49 EUR, in stock, add to cart.\" Choose the page_type value that fits best.";
pub const IMAGE_PROMPT: &str = "What is the dominant colour of this image? Answer with one word.";
pub const DECISION_INPUT: &str = "URL: https://shop.example/red-running-shoe\nTitle: Red running shoe - Example Shop\nText: Red running shoe for road running. Price 49 EUR. In stock. Add to cart.";
pub const SCHEMA_NAME: &str = "openspider_probe";
pub const IMAGE_MEDIA_TYPE: &str = "image/png";

/// Antwortschema des Fallbacks für eine `choice`-Frage (AI-034): alle Properties `required`,
/// `additionalProperties: false`, `abstain` Pflichtfeld.
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

/// Text genau der Länge `chars` (ASCII) aus einem wiederholten Satz.
pub fn filler(chars: usize) -> String {
    const SENTENCE: &str = "Use only observable criteria from the page content to decide. ";
    SENTENCE.chars().cycle().take(chars).collect()
}

/// V-A5: Schema an der OS-Grenze aus AI-030 (20 Werte, 2.000 Zeichen Beschreibung), strikt-verträglich (AI-021).
pub fn limits_schema() -> Value {
    let values: Vec<String> = (1..=20).map(|i| format!("type_{i:02}")).collect();
    json!({
        "type": "object",
        "properties": {
            "page_type": {"type": "string", "enum": values, "description": filler(2_000)},
            "abstain": {"type": "boolean"}
        },
        "required": ["page_type", "abstain"],
        "additionalProperties": false
    })
}

/// Die drei Fragen aus AI-004 (je eine `predicate`, `choice`, `score`); Namen nach AI-030 (`-` → `_`).
pub fn decision_questions() -> Vec<Question> {
    vec![
        Question::predicate(
            "soft_404",
            "Answer true if the page tells the visitor that the requested product or content no longer exists, although the page itself loads normally.",
        ),
        Question::choice(
            "page_type",
            "Classify the type of the page from its observable content.",
            &[
                ("product", "A page that offers one single product for sale."),
                ("category", "A page that lists several products."),
                ("article", "An editorial article or guide."),
                ("other", "Any other kind of page."),
            ],
        ),
        Question::score(
            "content_quality",
            "Rate how helpful and specific the page content is for a visitor.",
            &[
                ("poor", "Thin or unhelpful content."),
                ("fair", "Some useful information, but incomplete."),
                ("good", "Helpful, complete and specific content."),
            ],
        ),
    ]
}

fn questions_json(questions: &[Question]) -> Value {
    serde_json::to_value(questions).unwrap_or(Value::Null)
}

fn data_url(image_b64: &str) -> String {
    format!("data:{IMAGE_MEDIA_TYPE};base64,{image_b64}")
}

/// Rohkörper einer Anfrage (`None` für GET).
pub fn raw_body(step: &Step, model: &str, image_b64: &str) -> Option<Value> {
    debug_assert_eq!(step.via, Via::Raw);
    let id = step.id.split_once(".raw.").map(|(_, rest)| rest)?;
    match step.provider {
        Provider::Anthropic => anthropic_body(id, model, image_b64),
        Provider::OpenAi => openai_body(id, model, image_b64),
        Provider::OpenRouter => openrouter_body(id, model, image_b64),
    }
}

fn anthropic_body(id: &str, model: &str, image_b64: &str) -> Option<Value> {
    let text_message = json!([{"role": "user", "content": TEXT_PROMPT}]);
    Some(match id {
        "models" => return None,
        "count_tokens" => json!({"model": model, "system": SYSTEM_TEXT, "messages": text_message}),
        "text" | "unknown_model" => json!({
            "model": model, "max_tokens": TEXT_MAX_TOKENS, "system": SYSTEM_TEXT, "messages": text_message,
        }),
        "structured" | "structured_limits" => {
            let (prompt, schema) = structured_parts(id);
            json!({
                "model": model,
                "max_tokens": STRUCTURED_MAX_TOKENS,
                "system": SYSTEM_TEXT,
                "messages": [{"role": "user", "content": prompt}],
                "output_config": {"format": {"type": "json_schema", "schema": schema}},
            })
        }
        "image" => json!({
            "model": model,
            "max_tokens": TEXT_MAX_TOKENS,
            "system": SYSTEM_TEXT,
            "messages": [{"role": "user", "content": [
                {"type": "image", "source": {"type": "base64", "media_type": IMAGE_MEDIA_TYPE, "data": image_b64}},
                {"type": "text", "text": IMAGE_PROMPT},
            ]}],
        }),
        _ => return None,
    })
}

fn structured_parts(id: &str) -> (&'static str, Value) {
    if id.ends_with("limits") {
        (LIMITS_PROMPT, limits_schema())
    } else {
        (STRUCT_PROMPT, fallback_schema())
    }
}

fn openai_body(id: &str, model: &str, image_b64: &str) -> Option<Value> {
    let image_message = |text: &str| {
        json!([{"role": "user", "content": [
            {"type": "input_text", "text": text},
            {"type": "input_image", "image_url": data_url(image_b64)},
        ]}])
    };
    Some(match id {
        "models" => return None,
        "responses_text" | "responses_unknown_model" => json!({
            "model": model, "instructions": SYSTEM_TEXT, "input": TEXT_PROMPT, "max_output_tokens": TEXT_MAX_TOKENS,
        }),
        "responses_structured" | "responses_structured_limits" => {
            let (prompt, schema) = structured_parts(id);
            json!({
                "model": model,
                "instructions": SYSTEM_TEXT,
                "input": prompt,
                "max_output_tokens": STRUCTURED_MAX_TOKENS,
                "text": {"format": {"type": "json_schema", "name": SCHEMA_NAME, "schema": schema, "strict": true}},
            })
        }
        "responses_image" => json!({
            "model": model,
            "instructions": SYSTEM_TEXT,
            "input": image_message(IMAGE_PROMPT),
            "max_output_tokens": TEXT_MAX_TOKENS,
        }),
        "decisions_3q" => json!({
            "model": model, "input": DECISION_INPUT, "questions": questions_json(&decision_questions()),
        }),
        "decisions_image" => json!({
            "model": model,
            "input": image_message("Image from a product page."),
            "questions": questions_json(&[Question::predicate(
                "is_red",
                "Answer true if the image is predominantly red.",
            )]),
        }),
        "decisions_11q" => {
            let questions: Vec<Question> = (1..=11)
                .map(|i| {
                    Question::predicate(
                        format!("check_{i:02}"),
                        format!("Answer true if the page text contains the number {i}."),
                    )
                })
                .collect();
            json!({"model": model, "input": DECISION_INPUT, "questions": questions_json(&questions)})
        }
        "decisions_long_instructions" => json!({
            "model": model,
            "input": DECISION_INPUT,
            "questions": questions_json(&[Question::predicate("long_instructions", filler(4_000))]),
        }),
        "decisions_unknown_model" => json!({
            "model": model,
            "input": DECISION_INPUT,
            "questions": questions_json(&decision_questions()[..1]),
        }),
        _ => return None,
    })
}

fn openrouter_body(id: &str, model: &str, image_b64: &str) -> Option<Value> {
    let system = json!({"role": "system", "content": SYSTEM_TEXT});
    Some(match id {
        "models" | "key" => return None,
        "text" | "unknown_model" => json!({
            "model": model,
            "messages": [system, {"role": "user", "content": TEXT_PROMPT}],
            "max_tokens": TEXT_MAX_TOKENS,
            "provider": {"data_collection": "deny"},
        }),
        "structured" | "structured_limits" => {
            let (prompt, schema) = structured_parts(id);
            json!({
                "model": model,
                "messages": [system, {"role": "user", "content": prompt}],
                "max_tokens": STRUCTURED_MAX_TOKENS,
                "response_format": {"type": "json_schema", "json_schema": {"name": SCHEMA_NAME, "strict": true, "schema": schema}},
                "provider": {"data_collection": "deny", "require_parameters": true},
            })
        }
        "image" => json!({
            "model": model,
            "messages": [system, {"role": "user", "content": [
                {"type": "text", "text": IMAGE_PROMPT},
                {"type": "image_url", "image_url": {"url": data_url(image_b64)}},
            ]}],
            "max_tokens": TEXT_MAX_TOKENS,
            "provider": {"data_collection": "deny"},
        }),
        _ => return None,
    })
}

fn endpoint(provider: Provider, decisions: bool) -> &'static str {
    match provider {
        Provider::Anthropic => "messages",
        Provider::OpenAi if decisions => "decisions",
        Provider::OpenAi => "responses",
        Provider::OpenRouter => "chat-completions",
    }
}

/// Gleichwertiger `os-ai`-Request eines Adapter-Schritts (nur `execute`-Schritte; `None` für Liste/Kontostand).
pub fn adapter_request(step: &Step, model: &str, image_b64: &str) -> Option<Request> {
    debug_assert_eq!(step.via, Via::Adapter);
    let id = step.id.split_once(".adapter.").map(|(_, rest)| rest)?;
    let kind = id.strip_prefix("responses_").unwrap_or(id);
    let decisions = kind == "decisions_3q";
    let image = Input::Image {
        media_type: IMAGE_MEDIA_TYPE.into(),
        base64: image_b64.into(),
    };
    let text = |t: &str| Input::Text { text: t.into() };
    let (input, schema, questions) = match kind {
        "text" | "unknown_model" => (vec![text(TEXT_PROMPT)], None, Vec::new()),
        "structured" => (
            vec![text(STRUCT_PROMPT)],
            Some(fallback_schema()),
            Vec::new(),
        ),
        "image" => (vec![image, text(IMAGE_PROMPT)], None, Vec::new()),
        "decisions_3q" => (
            vec![text(DECISION_INPUT)],
            None,
            decision_questions()
                .iter()
                .map(|q| serde_json::to_value(q).unwrap_or(Value::Null))
                .collect(),
        ),
        _ => return None,
    };
    let vision = input.iter().any(|i| matches!(i, Input::Image { .. }));
    let capabilities = Capabilities {
        text: !decisions,
        structured_output: !decisions,
        vision: !decisions,
        decisions,
        ..Capabilities::default()
    };
    let required = Capabilities {
        text: !decisions,
        structured_output: schema.is_some(),
        vision: vision && !decisions,
        decisions,
        ..Capabilities::default()
    };
    Some(Request {
        model: Model {
            provider: step.provider.name().into(),
            id: if decisions {
                SPEC_DECISIONS_MODEL.into()
            } else {
                model.into()
            },
            endpoint: endpoint(step.provider, decisions).into(),
            capabilities,
            capability_source: CapabilitySource::ModelCatalog,
        },
        required,
        input,
        instructions: if decisions {
            String::new()
        } else {
            SYSTEM_TEXT.into()
        },
        questions,
        response_schema: schema,
        max_tokens: (step.max_output_tokens > 0).then_some(step.max_output_tokens),
        temperature: None,
        effort: None,
    })
}

/// Zeichen der Nutzlast ohne Bilddaten und Zahl der Bilder (für die Obergrenze).
pub fn payload_size(value: &Value, image_b64: &str) -> (u64, u32) {
    match value {
        Value::String(s) if !image_b64.is_empty() && s.contains(image_b64) => (0, 1),
        Value::String(s) => (s.chars().count() as u64, 0),
        Value::Array(items) => items.iter().fold((2, 0), |(c, i), v| {
            let (vc, vi) = payload_size(v, image_b64);
            (c + vc + 1, i + vi)
        }),
        Value::Object(map) => map.iter().fold((2, 0), |(c, i), (k, v)| {
            let (vc, vi) = payload_size(v, image_b64);
            (c + k.chars().count() as u64 + 4 + vc, i + vi)
        }),
        other => (other.to_string().len() as u64, 0),
    }
}

use super::{token_count, AnswerBook};
use axum::{
    http::Method,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

pub(super) fn error(
    provider: &str,
    status: u16,
    kind: &str,
    message: &str,
    metadata: Option<Value>,
) -> Response {
    let mut e = json!({"type":kind,"message":message});
    if let Some(metadata) = metadata {
        e["metadata"] = metadata;
    }
    if kind == "insufficient_quota" {
        e["code"] = json!("credit_balance_exhausted");
    }
    if message == "enforced_spend_limit_reached" {
        e["details"] = json!({"error_code":message});
    }
    let body = if provider == "ollama" {
        json!({"error":message})
    } else if provider == "anthropic" {
        json!({"type":"error", "error":e})
    } else {
        json!({"error":e})
    };
    (
        axum::http::StatusCode::from_u16(status).unwrap(),
        Json(body),
    )
        .into_response()
}
pub(super) async fn scenario(
    provider: &str,
    host: &str,
    scenario: &str,
    attempt: usize,
) -> Option<Response> {
    if scenario == "hang" {
        std::future::pending::<()>().await;
    }
    if let Some(seconds) = scenario
        .strip_prefix("delay=")
        .and_then(|s| s.parse::<u64>().ok())
    {
        tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
    }
    let (status, kind, message, metadata, retry) = match scenario {
        "401" | "4xx" | "error" => (
            401,
            "authentication_error",
            "Invalid fixture credentials",
            None,
            None,
        ),
        "429" | "throttle" => (429, "rate_limit_error", "Rate limited", None, None),
        "429x2" if attempt < 2 => (429, "rate_limit_error", "Rate limited", None, None),
        "retry-after-7" if attempt == 0 => {
            (429, "rate_limit_error", "Rate limited", None, Some("7"))
        }
        "500-always" => (500, "api_error", "Temporary error", None, None),
        "529" => (529, "overloaded_error", "Overloaded", None, None),
        "context-length" => (
            400,
            "invalid_request_error",
            "maximum context length exceeded",
            None,
            None,
        ),
        "region-denied" if host.starts_with("eu.") => {
            (403, "permission_error", "Data region denied", None, None)
        }
        "region-no-endpoint" if provider == "openrouter" && host.starts_with("eu.") => (
            404,
            "not_found",
            "No endpoints found supporting your data region",
            None,
            None,
        ),
        "insufficient-quota" if provider == "anthropic" => (
            400,
            "invalid_request_error",
            "Your credit balance is too low to access the API",
            None,
            None,
        ),
        "insufficient-quota" => (429, "insufficient_quota", "Quota exceeded", None, None),
        "spend-limit-user" => (
            400,
            "invalid_request_error",
            "You have reached your specified API usage limits",
            None,
            None,
        ),
        "spend-limit-workspace" => (
            400,
            "invalid_request_error",
            "You have reached your specified workspace API usage limits",
            None,
            None,
        ),
        "402-billing" => (402, "billing_error", "Billing error", None, None),
        "429-spend-cap" => (
            429,
            "rate_limit_error",
            "enforced_spend_limit_reached",
            None,
            None,
        ),
        "402-credits" => (
            402,
            "payment_required",
            "Insufficient credits",
            Some(json!({"limit_source":"openrouter_credits"})),
            None,
        ),
        "402-key-limit" => (
            402,
            "payment_required",
            "Key limit",
            Some(json!({"limit_source":"openrouter_key_limit"})),
            None,
        ),
        "402-weight" => (
            402,
            "payment_required",
            "Request weight exceeds budget",
            Some(json!({"limit_source":"openrouter_credits","reason":"weight_exceeds_budget"})),
            None,
        ),
        "402-in-flight" if attempt == 0 => (
            402,
            "payment_required",
            "In-flight budget",
            Some(json!({"limit_source":"openrouter_in_flight_budget"})),
            Some("3"),
        ),
        "403-guardrail" => (
            403,
            "permission_error",
            "Guardrail budget",
            Some(json!({"limit_source":"openrouter_guardrail_budget"})),
            None,
        ),
        "refusal-403" => (
            403,
            "refusal",
            "provider reported a model refusal as an error",
            None,
            None,
        ),
        "malformed-json" => {
            return Some(([("content-type", "application/json")], "{broken").into_response())
        }
        _ => return None,
    };
    let mut r = error(provider, status, kind, message, metadata);
    if let Some(retry) = retry {
        r.headers_mut()
            .insert("retry-after", retry.parse().unwrap());
    }
    Some(r)
}
fn models(provider: &str) -> Value {
    match provider {
        "anthropic" => {
            let caps = |max: bool| json!({"structured_outputs":{"supported":true},"image_input":{"supported":true},"batch":{"supported":true},"effort":{"supported":true,"low":{"supported":true},"medium":{"supported":true},"high":{"supported":true},"max":{"supported":max}}});
            json!({"data":[{"id":"mock-claude","display_name":"A","max_input_tokens":200000,"max_tokens":8192,"capabilities":caps(true)}, {"id":"mock-claude-b","display_name":"B","capabilities":caps(false)}, {"id":"mock-claude-c","display_name":"C","capabilities":{"effort":{"supported":false},"structured_outputs":{"supported":false},"image_input":{"supported":false},"batch":{"supported":false}}}, {"id":"mock-claude-d","display_name":"D"}],"has_more":false})
        }
        "openrouter" => {
            json!({"data":[{"id":"mock/text","name":"Mock text","context_length":200000,"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"supported_parameters":["response_format","structured_outputs"],"pricing":{"prompt":"0.000001","completion":"0.000005","request":"0","image":"0"}}, {"id":"mock/vision","name":"Mock vision","architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"supported_parameters":["response_format","structured_outputs"],"pricing":{"prompt":"0.000001","completion":"0.000005","request":"0.001","image":"0.002"}}, {"id":"mock/embedding","architecture":{"input_modalities":["text"],"output_modalities":["embeddings"]},"supported_parameters":[],"pricing":{"prompt":"0.000001","completion":"0"}}]})
        }
        "ollama" => {
            json!({"models":[{"name":"mock-llama","model":"mock-llama","details":{"family":"llama","parameter_size":"8B","quantization_level":"Q4_0"}}]})
        }
        _ => {
            json!({"object":"list","data":[{"id":"gpt-6-luna","object":"model","owned_by":"openai"},{"id":"mock-embedding","object":"model","owned_by":"openai"}]})
        }
    }
}
pub(super) fn success(
    book: &AnswerBook,
    provider: &str,
    method: &Method,
    path: &str,
    input: &Value,
    scenario: &str,
) -> Response {
    let expected = match provider {
        "anthropic" => ["/v1/models", "/v1/messages", "/v1/messages/count_tokens"].as_slice(),
        "openai" => [
            "/v1/models",
            "/v1/responses",
            "/v1/decisions",
            "/v1/embeddings",
        ]
        .as_slice(),
        "openrouter" => [
            "/api/v1/models",
            "/api/v1/key",
            "/api/v1/chat/completions",
            "/api/v1/embeddings",
        ]
        .as_slice(),
        "ollama" => ["/api/tags", "/api/chat", "/api/embed"].as_slice(),
        _ => &[],
    };
    if !expected.contains(&path) {
        return error(provider, 404, "not_found", "Unknown AI endpoint", None);
    }
    let read = path.ends_with("models") || path.ends_with("tags") || path.ends_with("key");
    if (read && method != Method::GET) || (!read && method != Method::POST) {
        return error(
            provider,
            405,
            "method_not_allowed",
            "Wrong HTTP method",
            None,
        );
    }
    if scenario == "empty" {
        return Json(
            json!({"data":[],"models":[],"content":[],"output":[],"answers":[],"choices":[]}),
        )
        .into_response();
    }
    if path.ends_with("models") || path.ends_with("tags") {
        return Json(models(provider)).into_response();
    }
    if path.ends_with("key") {
        return Json(json!({"data":{"label":"fixture","limit":100,"limit_remaining":90,"usage":10,"is_free_tier":false,"rate_limit":{"requests":100,"interval":"1m"}}})).into_response();
    }
    let tokens = token_count(input);
    if path.ends_with("count_tokens") {
        return Json(json!({"input_tokens":tokens})).into_response();
    }
    if path.ends_with("embeddings") || path.ends_with("embed") {
        let dims = input
            .get("dimensions")
            .and_then(Value::as_u64)
            .unwrap_or(768) as usize;
        if dims == 0 || dims > 65536 {
            return error(
                provider,
                400,
                "invalid_request_error",
                "Invalid dimensions",
                None,
            );
        }
        let texts = input.get("input").cloned().unwrap_or(Value::Null);
        let texts = texts.as_array().cloned().unwrap_or_else(|| vec![texts]);
        let vectors: Vec<_> = texts
            .iter()
            .map(|s| book.vector(s.as_str().unwrap_or(""), dims))
            .collect();
        return Json(if provider == "ollama" { json!({"model":input["model"],"embeddings":vectors,"prompt_eval_count":tokens}) } else { json!({"object":"list","data":vectors.iter().enumerate().map(|(i,v)| json!({"object":"embedding","index":i,"embedding":v})).collect::<Vec<_>>(),"model":input["model"],"usage":{"prompt_tokens":tokens,"total_tokens":tokens}}) }).into_response();
    }
    let refusal = matches!(scenario, "refusal" | "refusal-filter-only");
    let task = input
        .get("task")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let answer = book.answer(input, task);
    let output = if scenario == "schema-violation" {
        json!({"unexpected": ["not the requested schema"]}).to_string()
    } else {
        answer.to_string()
    };
    let out_tokens = output.chars().count().div_ceil(4);
    let mut result = match provider {
        "anthropic" => {
            json!({"id":"msg_mock","type":"message","role":"assistant","model":input["model"],"content":[{"type":"text","text":output}],"stop_reason":if refusal {"refusal"} else if scenario == "max-tokens" {"max_tokens"} else {"end_turn"},"usage":{"input_tokens":tokens,"output_tokens":out_tokens,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}})
        }
        "openrouter" => {
            let message = if refusal {
                if scenario == "refusal-filter-only" {
                    json!({"role":"assistant","content":null})
                } else {
                    json!({"role":"assistant","content":null,"refusal":"Fixture refusal"})
                }
            } else {
                json!({"role":"assistant","content":output})
            };
            json!({"id":"chatcmpl_mock","object":"chat.completion","model":input["model"],"choices":[{"index":0,"message":message,"finish_reason":if refusal {"content_filter"} else {"stop"}}],"usage":{"prompt_tokens":tokens,"completion_tokens":out_tokens,"total_tokens":tokens+out_tokens}})
        }
        "ollama" => {
            json!({"model":input["model"],"message":{"role":"assistant","content":output},"done":true,"prompt_eval_count":tokens,"eval_count":out_tokens})
        }
        _ if path.ends_with("decisions") => {
            let questions = input
                .get("questions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if questions.len() > 10 {
                return error(
                    provider,
                    400,
                    "invalid_request_error",
                    "At most 10 questions per request",
                    None,
                );
            }
            let answers: Vec<_> = questions.iter().enumerate().filter(|(i,_)| !(scenario == "missing-answer" && *i == questions.len().saturating_sub(1))).map(|(i,q)| {
                let id = q.get("id").and_then(Value::as_str).unwrap_or("default");
                let mut a = book.answer(input,id);
                if !a.is_object() { a = json!({"value":a}); }
                if a.get("ok").is_some() { a = match q["type"].as_str().unwrap_or("predicate") { "choice" => json!({"type":"choice","choice":0,"probabilities":[0.8,0.2],"confidence":0.8}), "score" => json!({"type":"score","score":1.5,"probabilities":[0.1,0.2,0.7],"confidence":0.7}), _ => json!({"type":"predicate","probability":0.93,"value":true}) }; }
                if refusal && i == 0 { a = json!({"type":"refusal","refusal":"Fixture refusal"}); }
                a["id"] = json!(id); a
            }).collect();
            json!({"id":"decision_mock","answers":answers,"usage":{"input_tokens":tokens,"output_tokens":out_tokens}})
        }
        _ => {
            json!({"id":"resp_mock","object":"response","status":"completed","model":input["model"],"output":[{"type":"message","role":"assistant","content":[if refusal { json!({"type":"refusal","refusal":"Fixture refusal"}) } else { json!({"type":"output_text","text":output}) }]}],"usage":{"input_tokens":tokens,"output_tokens":out_tokens,"input_tokens_details":{"cached_tokens":0}}})
        }
    };
    if scenario == "missing-usage" {
        result.as_object_mut().unwrap().remove("usage");
    }
    let mut r = Json(result).into_response();
    r.headers_mut()
        .insert("x-request-id", "mock-request".parse().unwrap());
    r
}

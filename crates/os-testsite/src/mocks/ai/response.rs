use super::{q00, token_count, AnswerBook};
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
pub(super) fn models(provider: &str, host: &str) -> Value {
    match provider {
        "anthropic" => {
            // A–D (AI-002) mit `thinking` (Nachtrag Q00): A adaptiv und abschaltbar, B nur adaptiv mit `max_tokens = 4000`,
            // C ohne Thinking, D ohne `capabilities`; A und B mit `xhigh` (AI-002: `max` auf B → `xhigh`).
            let caps = |max: bool, thinking: Value| json!({"structured_outputs":{"supported":true},"image_input":{"supported":true},"batch":{"supported":true},"effort":{"supported":true,"low":{"supported":true},"medium":{"supported":true},"high":{"supported":true},"xhigh":{"supported":true},"max":{"supported":max}},"thinking":thinking});
            let thinking = |disabled: bool| json!({"supported":true,"types":{"adaptive":{"supported":true},"disabled":{"supported":disabled},"enabled":{"supported":false}}});
            let retires = |days: i64| json!(format!("{}T00:00:00Z", q00::date_in_days(days)));
            json!({"data":[
                {"id":"mock-claude","display_name":"A","max_input_tokens":200000,"max_tokens":8192,"retires_at":null,"capabilities":caps(true, thinking(true))},
                {"id":"mock-claude-b","display_name":"B","max_tokens":4000,"retires_at":retires(30),"capabilities":caps(false, thinking(false))},
                {"id":"mock-claude-c","display_name":"C","retires_at":retires(200),"capabilities":{"effort":{"supported":false},"structured_outputs":{"supported":false},"image_input":{"supported":false},"batch":{"supported":false},"thinking":{"supported":false}}},
                {"id":"mock-claude-d","display_name":"D"}
            ],"has_more":false})
        }
        "openrouter" => {
            let mut data = vec![
                json!({"id":"mock/text","name":"Mock text","context_length":200000,"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"supported_parameters":["response_format","structured_outputs"],"pricing":{"prompt":"0.000001","completion":"0.000005","request":"0","image":"0"}}),
                json!({"id":"mock/vision","name":"Mock vision","architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"supported_parameters":["response_format","structured_outputs"],"pricing":{"prompt":"0.000001","completion":"0.000005","request":"0.001","image":"0.002"}}),
                json!({"id":"mock/embedding","architecture":{"input_modalities":["text"],"output_modalities":["embeddings"]},"supported_parameters":[],"pricing":{"prompt":"0.000001","completion":"0"}}),
            ];
            for name in [
                "text-small",
                "text-large",
                "global-a",
                "global-b",
                "global-c",
            ] {
                let mut model = data[0].clone();
                model["id"] = json!(format!("mock/{name}"));
                model["name"] = json!(name);
                data.push(model);
            }
            // Rollen nach 08 Konventionen (Nachtrag Q00), weiterhin 8 Modelle:
            // M-R1 = mock/text-small, M-R2 = mock/text-large (auch `max_completion_tokens = 4000`),
            // M-LC = mock/global-a (Ablauf in 30 Tagen), M-IR = mock/global-b (Ablauf in 200 Tagen),
            // M-R3 = mock/global-c (`expiration_date = null`, ohne `reasoning`).
            let with_reasoning = |model: &mut Value, reasoning: Value| {
                model["supported_parameters"] =
                    json!(["response_format", "structured_outputs", "reasoning"]);
                model["reasoning"] = reasoning;
            };
            with_reasoning(
                &mut data[3],
                json!({"supported_efforts":["none","low","medium","high"],"default_enabled":true,"default_effort":"medium","mandatory":false}),
            );
            with_reasoning(
                &mut data[4],
                json!({"supported_efforts":["low","medium","high"],"mandatory":true}),
            );
            data[4]["top_provider"] = json!({"max_completion_tokens":4000});
            data[5]["pricing"] = json!({"prompt":"0.000001","completion":"0.000002","request":"0","image":"0","overrides":[{"min_prompt_tokens":1000,"prompt":"0.000002","completion":"0.000004"}]});
            data[5]["expiration_date"] = json!(q00::date_in_days(30));
            data[6]["pricing"] = json!({"prompt":"0.000001","completion":"0.000002","internal_reasoning":"0.000010","request":"0","image":"0"});
            data[6]["expiration_date"] = json!(q00::date_in_days(200));
            data[7]["expiration_date"] = Value::Null;
            if host.starts_with("eu.") {
                data.truncate(5);
            }
            json!({"data":data})
        }
        "ollama" => {
            json!({"models":[{"name":"mock-llama","model":"mock-llama","details":{"family":"llama","parameter_size":"8B","quantization_level":"Q4_0"}}]})
        }
        _ => {
            json!({"object":"list","data":[{"id":"gpt-6-luna","object":"model","owned_by":"openai"},{"id":"mock-embedding","object":"model","owned_by":"openai"}]})
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn success(
    book: &AnswerBook,
    provider: &str,
    host: &str,
    method: &Method,
    path: &str,
    input: &Value,
    scenario: &str,
    custom_models: Option<&Value>,
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
        return Json(models(provider, host)).into_response();
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
    if provider == "openrouter" && scenario == "unknown-model" {
        // Form laut Beobachtung 24.11.2026 (Q00-N3): HTTP 400, `error.code` als Zahl, ohne `type` und `metadata`.
        let model = input["model"].as_str().unwrap_or("");
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({"error":{"code":400,"message":format!("{model} is not a valid model ID")},"user_id":q00::MOCK_USER})),
        )
            .into_response();
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
    let out_tokens = if refusal {
        "Fixture refusal".chars().count().div_ceil(4)
    } else {
        output.chars().count().div_ceil(4)
    };
    // Nachtrag Q00: Reasoning-Tokens (`reasoning=<n>`) und Abbruch an der Ausgabegrenze.
    let n = q00::reasoning_tokens(scenario);
    let answer_tokens = out_tokens as u64;
    let abort = if refusal {
        None
    } else {
        q00::abort(
            scenario,
            q00::sent_limit(provider, path, input),
            answer_tokens,
            n,
        )
    };
    let (output_tokens, reasoning) = match abort {
        Some(a) => (a.limit, a.reasoning),
        None if refusal => (answer_tokens, 0),
        None => (answer_tokens + n, n),
    };
    let truncated = abort.map(|a| q00::truncate(&output, a.limit));
    let mut result = match provider {
        "anthropic" => {
            let thinking = json!({"type":"thinking","thinking":q00::REASONING_TEXT,"signature":q00::SIGNATURE});
            let content = match (&abort, &truncated) {
                (Some(a), _) if a.reasoning > 0 => vec![thinking],
                (Some(_), Some(text)) => vec![json!({"type":"text","text":text})],
                _ => {
                    let text = json!({"type":"text","text":if refusal { "Fixture refusal" } else { &output }});
                    if reasoning > 0 {
                        vec![thinking, text]
                    } else {
                        vec![text]
                    }
                }
            };
            let stop = if refusal {
                "refusal"
            } else if abort.is_some() || scenario == "max-tokens" {
                "max_tokens"
            } else {
                "end_turn"
            };
            json!({"id":"msg_mock","type":"message","role":"assistant","model":input["model"],"content":content,"stop_reason":stop,"usage":{"input_tokens":tokens,"output_tokens":output_tokens,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"output_tokens_details":{"thinking_tokens":reasoning}}})
        }
        "openrouter" => {
            let mut message = if refusal {
                if scenario == "refusal-filter-only" {
                    json!({"role":"assistant","content":null})
                } else {
                    json!({"role":"assistant","content":null,"refusal":"Fixture refusal"})
                }
            } else {
                match (&abort, &truncated) {
                    (Some(a), _) if a.reasoning > 0 => json!({"role":"assistant","content":null}),
                    (Some(_), Some(text)) => json!({"role":"assistant","content":text}),
                    _ => json!({"role":"assistant","content":output}),
                }
            };
            if reasoning > 0 {
                message["reasoning"] = json!(q00::REASONING_TEXT);
                message["reasoning_details"] =
                    json!([{"type":"reasoning.encrypted","data":q00::ENCRYPTED}]);
            }
            let finish = if refusal {
                "content_filter"
            } else if abort.is_some() {
                "length"
            } else {
                "stop"
            };
            let mut usage = json!({"prompt_tokens":tokens,"completion_tokens":output_tokens,"total_tokens":tokens as u64 + output_tokens,"completion_tokens_details":{"reasoning_tokens":reasoning}});
            let default_catalog;
            let catalog = match custom_models {
                Some(catalog) => catalog,
                None => {
                    default_catalog = models(provider, host);
                    &default_catalog
                }
            };
            let computed = q00::openrouter_cost_pico(
                catalog,
                input["model"].as_str().unwrap_or(""),
                tokens as u64,
                output_tokens,
                q00::image_parts(input),
            );
            q00::apply_openrouter_cost(&mut usage, scenario, computed);
            json!({"id":"chatcmpl_mock","object":"chat.completion","model":input["model"],"choices":[{"index":0,"message":message,"finish_reason":finish}],"usage":usage})
        }
        "ollama" => {
            let limited = scenario == "output-limit";
            let content = if refusal {
                "Fixture refusal".to_string()
            } else if limited {
                q00::truncate(&output, q00::FORCED_LIMIT)
            } else {
                output.clone()
            };
            let mut body = json!({"model":input["model"],"message":{"role":"assistant","content":content},"done":true,"prompt_eval_count":tokens,"eval_count":if limited { q00::FORCED_LIMIT } else { out_tokens as u64 }});
            if limited {
                body["done_reason"] = json!("length");
            }
            body
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
            let answers: Vec<_> = questions.iter().enumerate()
                .filter(|(i, _)| !(scenario == "missing-answer" && *i == questions.len().saturating_sub(1)))
                .map(|(i, q)| {
                    let name = q.get("name").or_else(|| q.get("id"))
                        .and_then(Value::as_str).unwrap_or("default");
                    let mut a = book.answer(input, name);
                    if !a.is_object() {
                        a = json!({"value":a});
                    }
                    if a.get("ok").is_some() {
                        a = match q["type"].as_str().unwrap_or("predicate") {
                            "choice" => {
                                let choices = q["choices"].as_array().cloned()
                                    .unwrap_or_else(|| vec![json!({"value":0}), json!({"value":1})]);
                                let n = choices.len();
                                let probabilities: Vec<_> = choices.iter().enumerate().map(|(i, c)| {
                                    let p = if n == 1 {1.0} else if i == 0 {0.8} else {0.2 / (n - 1) as f64};
                                    json!({"value":c["value"],"probability":p})
                                }).collect();
                                json!({"type":"choice","choice":choices.first().map(|c| &c["value"]),"probabilities":probabilities,"confidence":if n == 1 {1.0} else {0.8}})
                            }
                            "score" => {
                                let levels = q["levels"].as_array().cloned()
                                    .unwrap_or_else(|| vec![json!({"label":"a"}), json!({"label":"b"}), json!({"label":"c"})]);
                                let n = levels.len();
                                let probabilities: Vec<_> = levels.iter().enumerate().map(|(i, level)| {
                                    let p = if n == 3 {[0.1,0.3,0.6][i]} else {1.0 / n as f64};
                                    json!({"value":i,"label":level["label"],"probability":p})
                                }).collect();
                                let score: f64 = probabilities.iter().enumerate()
                                    .map(|(i, p)| i as f64 * p["probability"].as_f64().unwrap()).sum();
                                json!({"type":"score","score":score,"probabilities":probabilities,"confidence":if n == 3 {0.6} else {1.0 / n.max(1) as f64}})
                            }
                            _ => json!({"type":"predicate","probability":0.93}),
                        };
                    }
                    if refusal && i == 0 {
                        a = json!({"type":"refusal","refusal":"Fixture refusal"});
                    }
                    a["name"] = json!(name);
                    a["id"] = json!(name);
                    a
                }).collect();
            json!({"answers":answers,"model":input["model"],"usage":{"input_tokens":q00::decisions_input_tokens(input),"output_tokens":0,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":0}}})
        }
        _ => {
            let reasoning_item =
                json!({"type":"reasoning","summary":[],"encrypted_content":q00::ENCRYPTED});
            let message = |text: &str| json!({"type":"message","role":"assistant","content":[json!({"type":"output_text","text":text})]});
            let items = if refusal {
                vec![
                    json!({"type":"message","role":"assistant","content":[json!({"type":"refusal","refusal":"Fixture refusal"})]}),
                ]
            } else {
                match (&abort, &truncated) {
                    (Some(a), _) if a.reasoning > 0 => vec![reasoning_item],
                    (Some(_), Some(text)) => vec![message(text)],
                    _ if reasoning > 0 => vec![reasoning_item, message(&output)],
                    _ => vec![message(&output)],
                }
            };
            let mut body = json!({"id":"resp_mock","object":"response","status":"completed","model":input["model"],"output":items,"usage":{"input_tokens":tokens,"output_tokens":output_tokens,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":reasoning}}});
            if abort.is_some() {
                body["status"] = json!("incomplete");
                body["incomplete_details"] = json!({"reason":"max_output_tokens"});
            }
            body
        }
    };
    if scenario == "missing-usage" {
        result.as_object_mut().unwrap().remove("usage");
    }
    Json(result).into_response()
}

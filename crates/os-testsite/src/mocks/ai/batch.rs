use super::{response, AiMock};
use axum::{
    http::{HeaderMap, Method},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

pub(super) struct Batch {
    provider: String,
    polls: usize,
    scenario: String,
    requests: Vec<Value>,
}
fn multipart_file(headers: &HeaderMap, body: &str) -> String {
    let boundary = headers
        .get("content-type")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.split("boundary=").nth(1))
        .map(|s| s.trim_matches('"'));
    if let Some(boundary) = boundary {
        for part in body.split(&format!("--{boundary}")) {
            if let Some((head, data)) = part.split_once("\r\n\r\n") {
                if head.contains("name=\"file\"") {
                    return data.trim_end_matches("\r\n").into();
                }
            }
        }
    }
    body.into()
}
pub(super) fn handle(
    mock: &AiMock,
    provider: &str,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    text: &str,
    scenario: &str,
) -> Option<Response> {
    if !matches!(provider, "anthropic" | "openai") {
        return None;
    }
    let is_files = provider == "openai" && (path == "/v1/files" || path.starts_with("/v1/files/"));
    let base = if provider == "anthropic" {
        "/v1/messages/batches"
    } else {
        "/v1/batches"
    };
    if !is_files && path != base && !path.starts_with(&format!("{base}/")) {
        return None;
    }
    let mut d = mock.data.lock().unwrap();
    if is_files {
        if path == "/v1/files" && method == Method::POST {
            d.next_id += 1;
            let id = format!("file_mock_{}", d.next_id);
            let data = multipart_file(headers, text);
            d.files.insert(id.clone(), data.clone());
            return Some(Json(json!({"id":id,"object":"file","purpose":"batch","bytes":data.len(),"filename":"batch.jsonl","status":"processed"})).into_response());
        }
        let id = path
            .trim_start_matches("/v1/files/")
            .trim_end_matches("/content");
        return Some(match d.files.get(id) {
            Some(file) if method == Method::GET && path.ends_with("/content") => {
                ([("content-type", "application/jsonl")], file.clone()).into_response()
            }
            Some(_) if method == Method::GET => {
                Json(json!({"id":id,"object":"file","status":"processed"})).into_response()
            }
            _ => response::error(provider, 404, "not_found", "Unknown file", None),
        });
    }
    let input: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    if path == base && method == Method::POST {
        if provider == "openai"
            && !matches!(
                input["endpoint"].as_str(),
                Some("/v1/responses" | "/v1/embeddings")
            )
        {
            return Some(response::error(
                provider,
                400,
                "invalid_request_error",
                "Batch endpoint must be responses or embeddings",
                None,
            ));
        }
        let requests = if provider == "anthropic" {
            input["requests"].as_array().cloned().unwrap_or_default()
        } else {
            let Some(file) = input["input_file_id"]
                .as_str()
                .and_then(|id| d.files.get(id))
            else {
                return Some(response::error(
                    provider,
                    400,
                    "invalid_request_error",
                    "Unknown input file",
                    None,
                ));
            };
            file.lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        };
        d.next_id += 1;
        let id = format!("batch_mock_{}", d.next_id);
        d.batches.insert(
            id.clone(),
            Batch {
                provider: provider.into(),
                polls: 0,
                scenario: scenario.into(),
                requests,
            },
        );
        return Some(Json(json!({"id":id,"type":"message_batch","object":"batch","processing_status":"in_progress","status":"in_progress"})).into_response());
    }
    let tail = path
        .strip_prefix(base)
        .unwrap_or("")
        .trim_start_matches('/');
    let id = tail.split('/').next().unwrap_or("");
    let Some(batch) = d.batches.get_mut(id).filter(|b| b.provider == provider) else {
        return Some(response::error(
            provider,
            404,
            "not_found",
            "Unknown batch",
            None,
        ));
    };
    let expired = batch.scenario == "batch-expired";
    if tail.ends_with("cancel") && method == Method::POST {
        batch.scenario = "batch-expired".into();
        batch.polls = 3;
        return Some(
            Json(json!({"id":id,"status":"cancelled","processing_status":"ended"})).into_response(),
        );
    }
    if method != Method::GET {
        return Some(response::error(
            provider,
            405,
            "method_not_allowed",
            "Wrong HTTP method",
            None,
        ));
    }
    if tail.ends_with("results") {
        if batch.polls < 3 {
            return Some(response::error(
                provider,
                409,
                "invalid_request_error",
                "Batch still in progress",
                None,
            ));
        }
        return Some(
            (
                [("content-type", "application/jsonl")],
                results(batch, &mock.answers),
            )
                .into_response(),
        );
    }
    batch.polls += 1;
    let ended = batch.polls >= 3;
    let n = batch.requests.len();
    let failed = if expired {
        n
    } else if batch.scenario == "batch-partial" {
        n.div_ceil(10)
    } else {
        0
    };
    let output = if ended {
        Some(results(batch, &mock.answers))
    } else {
        None
    };
    let output_id = format!("{id}_output");
    let result = json!({"id":id,"object":"batch","type":"message_batch","processing_status":if ended {"ended"} else {"in_progress"},"status":if !ended {"in_progress"} else if expired {"expired"} else {"completed"},"results_url":if ended {Some(format!("/anthropic{base}/{id}/results"))} else {None},"output_file_id":if ended {Some(&output_id)} else {None},"request_counts":{"processing":if ended {0} else {n},"succeeded":if ended {n-failed} else {0},"errored":if ended && !expired {failed} else {0},"expired":if ended && expired {n} else {0},"total":n,"completed":if ended {n-failed} else {0},"failed":if ended {failed} else {0}}});
    if let Some(output) = output {
        d.files.insert(output_id, output);
    }
    Some(Json(result).into_response())
}
fn results(batch: &Batch, book: &super::AnswerBook) -> String {
    batch.requests.iter().enumerate().map(|(i,request)| {
        let expired = batch.scenario == "batch-expired";
        let failed = batch.scenario == "batch-partial" && i % 10 == 0;
        let body = if batch.provider == "anthropic" {&request["params"]} else {&request["body"]};
        let answer = book.answer(body,"default").to_string();
        let tokens = super::token_count(body);
        if batch.provider == "anthropic" {
            json!({"custom_id":request["custom_id"],"result":if expired {json!({"type":"expired"})} else if failed {json!({"type":"errored","error":{"type":"api_error","message":"Fixture batch error"}})} else {json!({"type":"succeeded","message":{"id":format!("msg_batch_{i}"),"type":"message","role":"assistant","content":[{"type":"text","text":answer}],"stop_reason":"end_turn","usage":{"input_tokens":tokens,"output_tokens":answer.chars().count().div_ceil(4)}}})}}).to_string()
        } else {
            let response_body = if request["url"].as_str() == Some("/v1/embeddings") {
                let texts = body["input"].as_array().cloned().unwrap_or_else(||vec![body["input"].clone()]);
                json!({"data":texts.iter().enumerate().map(|(i,t)|json!({"index":i,"embedding":book.vector(t.as_str().unwrap_or(""),body["dimensions"].as_u64().unwrap_or(768) as usize)})).collect::<Vec<_>>(),"usage":{"prompt_tokens":tokens,"total_tokens":tokens}})
            } else { json!({"id":format!("resp_batch_{i}"),"output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":answer}]}],"usage":{"input_tokens":tokens,"output_tokens":answer.chars().count().div_ceil(4)}}) };
            json!({"id":format!("result_{i}"),"custom_id":request["custom_id"],"response":if failed || expired {Value::Null} else {json!({"status_code":200,"request_id":format!("request_{i}"),"body":response_body})},"error":if failed || expired {json!({"code":if expired {"batch_expired"} else {"api_error"},"message":"Fixture batch error"})} else {Value::Null}}).to_string()
        }
    }).collect::<Vec<_>>().join("\n") + "\n"
}

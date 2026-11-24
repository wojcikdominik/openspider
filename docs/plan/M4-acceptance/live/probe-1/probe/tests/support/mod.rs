//! Lokaler Testserver (nur 127.0.0.1) mit Antworten in der Form, die 08 je Anbieter beschreibt. Er protokolliert
//! jede Anfrage und gibt den Testschlüssel absichtlich in Headern und Fehlermeldungen zurück, damit die Tests
//! zeigen, dass er in keiner Ausgabe der Probe landet.

#![allow(dead_code)]

use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone)]
pub struct Logged {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Logged {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Behavior {
    /// Wird in Headern und Fehlermeldungen zurückgegeben.
    pub echo_key: String,
    /// Gemeldete Eingabe-Tokens je Antwort.
    pub input_tokens: u64,
    /// Status für alle Anfragen, deren Körper den Text-Prompt („pong“) enthält.
    pub fail_text_with: Option<u16>,
}

impl Behavior {
    pub fn new(echo_key: &str) -> Self {
        Self {
            echo_key: echo_key.into(),
            input_tokens: 20,
            fail_text_with: None,
        }
    }
}

pub struct Server {
    pub addr: SocketAddr,
    pub log: Arc<Mutex<Vec<Logged>>>,
}

impl Server {
    pub async fn start(behavior: Behavior) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let log = Arc::new(Mutex::new(Vec::new()));
        let behavior = Arc::new(behavior);
        let log2 = log.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let log = log2.clone();
                let behavior = behavior.clone();
                tokio::spawn(async move {
                    let _ = handle(stream, &behavior, &log).await;
                });
            }
        });
        Self { addr, log }
    }

    pub fn endpoints(&self) -> probe::plan::Endpoints {
        probe::plan::Endpoints {
            anthropic: format!("http://{}/anthropic", self.addr),
            openai: format!("http://{}/openai/v1", self.addr),
            openrouter: format!("http://{}/openrouter/api/v1", self.addr),
        }
    }

    pub fn requests(&self) -> Vec<Logged> {
        self.log.lock().unwrap().clone()
    }

    pub fn requests_for(&self, prefix: &str) -> Vec<Logged> {
        self.requests()
            .into_iter()
            .filter(|r| r.path.starts_with(prefix))
            .collect()
    }
}

async fn handle(
    mut stream: TcpStream,
    behavior: &Behavior,
    log: &Mutex<Vec<Logged>>,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let method = first.next().unwrap_or("").to_string();
    let path = first.next().unwrap_or("").to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + len {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf[header_end..]).to_string();
    log.lock().unwrap().push(Logged {
        method: method.clone(),
        path: path.clone(),
        headers,
        body: body.clone(),
    });
    let (status, response) = route(&method, &path, &body, behavior);
    let text = response.to_string();
    let reply = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\
         x-echo-key: {}\r\nanthropic-ratelimit-requests-remaining: 49\r\nrequest-id: req_test\r\n\
         x-ratelimit-remaining-requests: 99\r\nx-request-id: xreq_test\r\n\r\n{text}",
        text.len(),
        behavior.echo_key
    );
    stream.write_all(reply.as_bytes()).await?;
    stream.shutdown().await
}

fn masked(key: &str) -> String {
    let tail: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("sk-...{tail}")
}

fn route(method: &str, path: &str, body: &str, b: &Behavior) -> (u16, Value) {
    let req: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let model = req.get("model").and_then(Value::as_str).unwrap_or("");
    let unknown = model == probe::plan::UNKNOWN_MODEL;
    if let Some(status) = b.fail_text_with {
        if body.contains("pong") {
            return (
                status,
                json!({"error": {"type": "server_error", "message": format!("injected {status}")}}),
            );
        }
    }
    let structured_text = || {
        if body.contains("page_type") {
            json!({"page_type": "type_01", "abstain": false}).to_string()
        } else {
            json!({"probabilities": {"product": 0.8, "category": 0.15, "other": 0.05}, "abstain": false})
                .to_string()
        }
    };
    let is_structured = body.contains("json_schema");
    let answer = if is_structured {
        structured_text()
    } else if body.contains("colour") {
        "Red".to_string()
    } else {
        "pong".to_string()
    };
    let path_only = path.split('?').next().unwrap_or(path);
    match (method, path_only) {
        ("GET", "/anthropic/v1/models") => (
            200,
            json!({"data": [
                {"type": "model", "id": "claude-test-sonnet", "display_name": "Test Sonnet", "created_at": "2026-01-01T00:00:00Z",
                 "max_input_tokens": 200000, "max_tokens": 64000,
                 "capabilities": {"structured_outputs": {"supported": true}, "image_input": {"supported": true}, "batch": {"supported": true},
                                  "effort": {"supported": true, "low": {"supported": true}, "max": {"supported": false}}}},
                {"type": "model", "id": "claude-test-haiku", "display_name": "Test Haiku", "created_at": "2026-01-01T00:00:00Z",
                 "max_input_tokens": 200000, "max_tokens": 64000,
                 "capabilities": {"structured_outputs": {"supported": true}, "image_input": {"supported": true}, "batch": {"supported": true}}}
             ], "has_more": false, "first_id": "claude-test-sonnet", "last_id": "claude-test-haiku"}),
        ),
        ("POST", "/anthropic/v1/messages/count_tokens") => (200, json!({"input_tokens": 12})),
        ("POST", "/anthropic/v1/messages") if unknown => (
            404,
            json!({"type": "error", "error": {"type": "not_found_error",
                   "message": format!("model: {model} (key {})", b.echo_key)}}),
        ),
        ("POST", "/anthropic/v1/messages") => (
            200,
            json!({"id": "msg_test", "type": "message", "role": "assistant", "model": model,
                   "content": [{"type": "thinking", "thinking": "", "signature": "SIGBLOB-0123456789"},
                               {"type": "text", "text": answer}], "stop_reason": "end_turn",
                   "usage": {"input_tokens": b.input_tokens, "output_tokens": 5,
                             "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}),
        ),
        ("GET", "/openai/v1/models") => (
            200,
            json!({"object": "list", "data": [
                {"id": "gpt-6-luna", "object": "model", "created": 1, "owned_by": "openai"},
                {"id": "gpt-test-nano", "object": "model", "created": 1, "owned_by": "openai"},
                {"id": "davinci:ft-acmecorp-2023-07-19", "object": "model", "created": 1, "owned_by": "acmecorp"}
            ]}),
        ),
        ("POST", "/openai/v1/responses") if unknown => (
            404,
            json!({"error": {"message": format!("The model `{model}` does not exist. Key {}", masked(&b.echo_key)),
                             "type": "invalid_request_error", "param": "model", "code": "model_not_found"}}),
        ),
        ("POST", "/openai/v1/responses") => (
            200,
            json!({"id": "resp_test", "object": "response", "status": "completed", "model": model,
                   "output": [{"type": "reasoning", "id": "rs_test", "encrypted_content": "ENCBLOB-0123456789", "summary": []},
                              {"type": "message", "role": "assistant", "id": "msg_test",
                               "content": [{"type": "output_text", "text": answer}]}],
                   "usage": {"input_tokens": b.input_tokens, "output_tokens": 5,
                             "input_tokens_details": {"cached_tokens": 0}}}),
        ),
        ("POST", "/openai/v1/decisions") if unknown => (
            404,
            json!({"error": {"message": "model not found", "type": "invalid_request_error", "code": "model_not_found"}}),
        ),
        ("POST", "/openai/v1/decisions") => {
            let answers: Vec<Value> = req
                .get("questions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|q| {
                    let name = q.get("name").cloned().unwrap_or(Value::Null);
                    match q.get("type").and_then(Value::as_str) {
                        Some("choice") => {
                            let probs: Vec<Value> = q["choices"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .enumerate()
                                .map(|(i, c)| json!({"value": c["value"], "probability": if i == 0 { 0.7 } else { 0.1 }}))
                                .collect();
                            json!({"name": name, "type": "choice", "choice": q["choices"][0]["value"],
                                   "probabilities": probs, "confidence": 0.7})
                        }
                        Some("score") => json!({"name": name, "type": "score", "score": 1.0,
                            "probabilities": [{"value": 0, "label": "poor", "probability": 0.2},
                                              {"value": 1, "label": "fair", "probability": 0.6},
                                              {"value": 2, "label": "good", "probability": 0.2}],
                            "confidence": 0.6}),
                        _ => json!({"name": name, "type": "predicate", "probability": 0.1}),
                    }
                })
                .collect();
            (
                200,
                json!({"id": "dec_test", "answers": answers, "usage": {"input_tokens": b.input_tokens}}),
            )
        }
        ("GET", "/openrouter/api/v1/models") => (
            200,
            json!({"data": [
                {"id": "vendor/cheap-vision", "name": "Cheap Vision", "context_length": 32000,
                 "pricing": {"prompt": "0.0000002", "completion": "0.0000004", "request": "0", "image": "0.0001"},
                 "supported_parameters": ["max_tokens", "response_format", "structured_outputs"],
                 "architecture": {"input_modalities": ["text", "image"], "output_modalities": ["text"]}},
                {"id": "vendor/cheapest-text", "name": "Cheapest Text", "context_length": 8000,
                 "pricing": {"prompt": "0.0000001", "completion": "0.0000001"},
                 "supported_parameters": ["max_tokens"],
                 "architecture": {"input_modalities": ["text"], "output_modalities": ["text"]}},
                {"id": "vendor/free:free", "name": "Free", "context_length": 8000,
                 "pricing": {"prompt": "0", "completion": "0"},
                 "supported_parameters": ["max_tokens", "response_format"],
                 "architecture": {"input_modalities": ["text", "image"], "output_modalities": ["text"]}},
                {"id": "vendor/luxury", "name": "Luxury", "context_length": 8000,
                 "pricing": {"prompt": "0.001", "completion": "0.002"},
                 "supported_parameters": ["max_tokens", "response_format"],
                 "architecture": {"input_modalities": ["text", "image"], "output_modalities": ["text"]}}
            ]}),
        ),
        ("GET", "/openrouter/api/v1/key") => (
            200,
            json!({"data": {"label": masked(&b.echo_key), "creator_user_id": "user_SECRETACCOUNT",
                            "workspace_id": "SECRETWORKSPACE-uuid", "organization_id": null,
                            "expires_at": "2026-12-01T16:45:26.171Z", "limit": null, "limit_remaining": null,
                            "limit_reset": null, "usage": 0.5, "usage_daily": 0.1, "usage_weekly": 0.2,
                            "usage_monthly": 0.5, "is_free_tier": false}}),
        ),
        ("POST", "/openrouter/api/v1/chat/completions") if unknown => (
            400,
            json!({"error": {"code": 400, "message": format!("{model} is not a valid model ID")}, "user_id": "user_SECRETACCOUNT"}),
        ),
        ("POST", "/openrouter/api/v1/chat/completions") => (
            200,
            json!({"id": "gen-test", "model": model,
                   "choices": [{"index": 0, "message": {"role": "assistant", "content": answer,
                       "reasoning_details": [{"type": "reasoning.encrypted", "data": "ENCBLOB-0123456789", "id": "rs_test"}]},
                       "finish_reason": "stop"}],
                   "usage": {"prompt_tokens": b.input_tokens, "completion_tokens": 5, "total_tokens": b.input_tokens + 5}}),
        ),
        _ => (
            404,
            json!({"error": {"message": format!("no route {method} {path}")}}),
        ),
    }
}

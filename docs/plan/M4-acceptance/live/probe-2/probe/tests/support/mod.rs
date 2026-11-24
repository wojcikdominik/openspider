//! Lokaler Testserver (nur 127.0.0.1) für Q00b. Antworten in der in 08 beschriebenen Form; Reasoning-Verhalten so,
//! wie die Probe es abfragt (Thinking-/Reasoning-Tokens 0 bei `disabled`/`none`, 400 für unbekannte Werte und für
//! `disabled` an ein nicht abschaltbares Modell). Gibt den Testschlüssel und Kontokennungen absichtlich in Headern
//! und Körpern zurück, damit die Tests die Bereinigung belegen.

#![allow(dead_code)]

use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub const NON_DISABLEABLE: &str = "claude-test-always-thinks";

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
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone)]
pub struct Behavior {
    pub echo_key: String,
    pub input_tokens: u64,
    /// Status für alle POST-Anfragen (Fehlerfall ohne Wiederholung).
    pub fail_posts_with: Option<u16>,
}

impl Behavior {
    pub fn new(echo_key: &str) -> Self {
        Self {
            echo_key: echo_key.into(),
            input_tokens: 20,
            fail_posts_with: None,
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
         x-ratelimit-remaining-requests: 99\r\nx-request-id: xreq_test\r\nopenai-organization: org-SECRETORG\r\n\r\n{text}",
        text.len(),
        behavior.echo_key
    );
    stream.write_all(reply.as_bytes()).await?;
    stream.shutdown().await
}

fn answer() -> String {
    json!({"probabilities": {"product": 0.9, "category": 0.05, "other": 0.05}, "abstain": false})
        .to_string()
}

fn route(method: &str, path: &str, body: &str, b: &Behavior) -> (u16, Value) {
    let req: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let model = req.get("model").and_then(Value::as_str).unwrap_or("");
    if method == "POST" {
        if let Some(status) = b.fail_posts_with {
            return (
                status,
                json!({"error": {"type": "server_error", "message": format!("injected {status}")}}),
            );
        }
    }
    let path_only = path.split('?').next().unwrap_or(path);
    match (method, path_only) {
        ("GET", "/anthropic/v1/models") => (
            200,
            json!({"data": [
                {"type": "model", "id": "claude-haiku-5-5", "display_name": "Haiku",
                 "capabilities": {"structured_outputs": {"supported": true},
                    "effort": {"supported": true, "low": {"supported": true}, "max": {"supported": true}},
                    "thinking": {"supported": true, "types": {"adaptive": {"supported": true}, "disabled": {"supported": true}, "enabled": {"supported": false}}}}},
                {"type": "model", "id": NON_DISABLEABLE, "display_name": "Always thinks",
                 "capabilities": {"structured_outputs": {"supported": true},
                    "effort": {"supported": true, "low": {"supported": true}},
                    "thinking": {"supported": true, "types": {"adaptive": {"supported": true}, "disabled": {"supported": false}}}}}
             ], "has_more": false}),
        ),
        ("POST", "/anthropic/v1/messages") => {
            let thinking = req.pointer("/thinking/type").and_then(Value::as_str);
            if thinking == Some("disabled") && model == NON_DISABLEABLE {
                return (
                    400,
                    json!({"type": "error", "error": {"type": "invalid_request_error",
                        "message": format!("thinking cannot be disabled for this model ({})", b.echo_key)},
                        "request_id": "req_body_test"}),
                );
            }
            let thinking_tokens = if thinking == Some("disabled") { 0 } else { 12 };
            let mut content = vec![];
            if thinking_tokens > 0 {
                content
                    .push(json!({"type": "thinking", "thinking": "", "signature": "SIGBLOB-0123"}));
            }
            content.push(json!({"type": "text", "text": answer()}));
            (
                200,
                json!({"id": "msg_test", "type": "message", "model": model, "content": content,
                       "stop_reason": "end_turn",
                       "usage": {"input_tokens": b.input_tokens, "output_tokens": 30 + thinking_tokens,
                                 "output_tokens_details": {"thinking_tokens": thinking_tokens}}}),
            )
        }
        ("GET", "/openai/v1/models") => (
            200,
            json!({"object": "list", "data": [
                {"id": "gpt-6-luna", "object": "model", "created": 1, "owned_by": "system"},
                {"id": "ft:gpt-6-luna:acmecorp:x:1", "object": "model", "created": 1, "owned_by": "acmecorp"}
            ]}),
        ),
        ("POST", "/openai/v1/responses") => {
            let effort = req.pointer("/reasoning/effort").and_then(Value::as_str);
            if matches!(effort, Some(e) if !["none", "minimal", "low", "medium", "high", "xhigh"].contains(&e))
            {
                return (
                    400,
                    json!({"error": {"message": format!("Invalid value: '{}'. Supported values are: 'none', 'low', 'medium', 'high' and 'xhigh'.", effort.unwrap_or("")),
                        "type": "invalid_request_error", "param": "reasoning.effort", "code": "unsupported_value"}}),
                );
            }
            let reasoning_tokens = if effort == Some("none") { 0 } else { 20 };
            (
                200,
                json!({"id": "resp_test", "object": "response", "status": "completed", "model": model,
                       "store": req.get("store").cloned().unwrap_or(json!(true)),
                       "prompt_cache_retention": "24h",
                       "reasoning": {"effort": effort.unwrap_or("medium"), "summary": null},
                       "output": [{"type": "reasoning", "id": "rs_test", "encrypted_content": "ENCBLOB-1", "summary": []},
                                  {"type": "message", "id": "msg_test", "role": "assistant",
                                   "content": [{"type": "output_text", "text": answer()}]}],
                       "usage": {"input_tokens": b.input_tokens, "output_tokens": 30 + reasoning_tokens,
                                 "output_tokens_details": {"reasoning_tokens": reasoning_tokens}}}),
            )
        }
        ("POST", "/openai/v1/decisions") => (
            200,
            json!({"model": model,
                   "answers": [{"name": "soft_404", "type": "predicate", "probability": 0.02}],
                   "usage": {"input_tokens": b.input_tokens, "output_tokens": 0,
                             "output_tokens_details": {"reasoning_tokens": 0}}}),
        ),
        ("GET", "/openrouter/api/v1/models") => (
            200,
            json!({"data": [
                {"id": "openai/gpt-6-luna", "name": "GPT-6 Luna",
                 "pricing": {"prompt": "0.0000001", "completion": "0.0000005"},
                 "supported_parameters": ["max_tokens", "reasoning", "response_format", "structured_outputs"],
                 "reasoning": {"default_enabled": true, "default_effort": "medium", "mandatory": false,
                               "supported_efforts": ["none", "low", "medium", "high"]},
                 "architecture": {"input_modalities": ["text", "image"], "output_modalities": ["text"]}},
                {"id": "vendor/other", "pricing": {"prompt": "0.0000001", "completion": "0.0000001"}}
            ]}),
        ),
        ("POST", "/openrouter/api/v1/chat/completions") => {
            let off = req.pointer("/reasoning/effort").and_then(Value::as_str) == Some("none")
                || req.pointer("/reasoning/enabled") == Some(&json!(false));
            let reasoning_tokens = if off { 0 } else { 25 };
            (
                200,
                json!({"id": "gen-test-1", "model": model, "user_id": "user_SECRETACCOUNT",
                       "choices": [{"index": 0, "finish_reason": "stop",
                                    "message": {"role": "assistant", "content": answer(),
                                        "reasoning_details": [{"type": "reasoning.encrypted", "data": "ENCBLOB-2", "id": "rs_test"}]}}],
                       "usage": {"prompt_tokens": b.input_tokens, "completion_tokens": 30 + reasoning_tokens,
                                 "completion_tokens_details": {"reasoning_tokens": reasoning_tokens},
                                 "cost": 0.00002}}),
            )
        }
        _ => (
            404,
            json!({"error": {"message": format!("no route {method} {path}")}}),
        ),
    }
}

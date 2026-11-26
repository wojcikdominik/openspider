//! Unabhängige Abnahme M4-D01 – Selbsttest der Mock-Erweiterung `hold` (08 „Konventionen“, Nachtrag 26.11.2026,
//! K4-13): die drei Selbsttest-Kriterien sowie Freigabe ohne gehaltenen Request und unbekannter Anbieter.
//! Erwartungen nur aus `docs/spec/08-ai.md`. Zeitgrenzen dienen nur als Abbruchschutz.
// acceptance: independent
mod common;

use std::net::SocketAddr;
use std::time::Duration;

use os_testsite::mocks::{ai::AiMock, MockServer};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

const DECISIONS: &str = "/openai/v1/decisions";
const GUARD: Duration = Duration::from_secs(15);

async fn server() -> (AiMock, MockServer) {
    let mock = AiMock::fixtures().expect("ai-mock-Fragmente");
    let server = MockServer::start(mock.registrations())
        .await
        .expect("ai-mock startet");
    (mock, server)
}

/// Ein HTTP/1.1-Request; Rückgabe Status und JSON-Körper.
async fn http(addr: SocketAddr, path: &str, body: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).await.expect("verbinden");
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: openai.mock.test\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(body.as_bytes()).await.unwrap();
    let (raw, err) = common::read_all(&mut stream).await;
    assert!(err.is_none(), "Lesefehler: {err:?}");
    let resp = common::parse(&raw).expect("HTTP-Antwort");
    let value = serde_json::from_slice(&resp.body).unwrap_or(Value::Null);
    (resp.status, value)
}

fn spawn_request(addr: SocketAddr, body: Value) -> JoinHandle<(u16, Value)> {
    tokio::spawn(async move { http(addr, DECISIONS, &body.to_string()).await })
}

async fn control(addr: SocketAddr, path: &str, body: Value) -> (u16, Value) {
    http(addr, path, &body.to_string()).await
}

async fn hold(addr: SocketAddr, count: usize) {
    let (status, _) = control(
        addr,
        "/__mock/scenario",
        json!({"provider": "openai", "scenario": "hold", "count": count}),
    )
    .await;
    assert_eq!(status, 200);
}

async fn release(addr: SocketAddr, count: Option<usize>) -> Value {
    let body = match count {
        Some(c) => json!({"provider": "openai", "count": c}),
        None => json!({"provider": "openai"}),
    };
    let (status, value) = control(addr, "/__mock/release", body).await;
    assert_eq!(status, 200);
    value
}

fn predicate_request() -> Value {
    json!({
        "model": "gpt-6-luna",
        "input": "First held request",
        "questions": [{"name": "first_q", "type": "predicate", "instructions": "Is it first?"}]
    })
}

fn choice_request() -> Value {
    json!({
        "model": "gpt-6-luna",
        "input": "Second held request with more text",
        "questions": [{
            "name": "second_q",
            "type": "choice",
            "instructions": "Which one?",
            "choices": [{"value": "a", "description": "A"}, {"value": "b", "description": "B"}]
        }]
    })
}

/// Anbieter-Requests an die Decisions-Route im Protokoll.
fn decisions_log(mock: &AiMock) -> Vec<os_testsite::mocks::ai::RequestRecord> {
    mock.requests()
        .into_iter()
        .filter(|r| r.path == DECISIONS)
        .collect()
}

async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..3000 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("Bedingung nicht erreicht: {what}");
}

/// Antwort, die derselbe Request ohne `hold` erhält (frischer Mock).
async fn unheld_answer(body: Value) -> Value {
    let (_mock, server) = server().await;
    let (status, value) = http(server.addr(), DECISIONS, &body.to_string()).await;
    assert_eq!(status, 200);
    value
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn hold_count_2_holds_two_third_is_answered_immediately() {
    let (mock, server) = server().await;
    let addr = server.addr();
    hold(addr, 2).await;
    let first = spawn_request(addr, predicate_request());
    let second = spawn_request(addr, choice_request());
    wait_for("zwei gehaltene Requests im Protokoll", || {
        decisions_log(&mock).iter().filter(|r| r.held).count() == 2
    })
    .await;
    // Das Protokoll (GET /__mock/log) führt beide mit `held: true`.
    let (_, log) = {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /__mock/log HTTP/1.1\r\nHost: mock.test\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let (raw, _) = common::read_all(&mut stream).await;
        let resp = common::parse(&raw).unwrap();
        (
            resp.status,
            serde_json::from_slice::<Value>(&resp.body).unwrap(),
        )
    };
    let held_in_log = log
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["path"] == DECISIONS && r["held"] == true)
        .count();
    assert_eq!(held_in_log, 2);
    assert!(
        !first.is_finished() && !second.is_finished(),
        "gehaltener Request beantwortet"
    );
    // Ein dritter Request wird sofort beantwortet.
    let (status, third) = tokio::time::timeout(
        GUARD,
        http(addr, DECISIONS, &predicate_request().to_string()),
    )
    .await
    .expect("dritter Request sofort beantwortet");
    assert_eq!(status, 200);
    assert!(third["answers"].is_array());
    assert!(
        !decisions_log(&mock).last().unwrap().held,
        "dritter Request fälschlich gehalten"
    );
    tokio::task::yield_now().await;
    assert!(!first.is_finished() && !second.is_finished());
    assert_eq!(release(addr, None).await["released"], 2);
    tokio::time::timeout(GUARD, first).await.unwrap().unwrap();
    tokio::time::timeout(GUARD, second).await.unwrap().unwrap();
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn release_one_answers_oldest_with_unheld_content() {
    let expected_first = unheld_answer(predicate_request()).await;
    let expected_second = unheld_answer(choice_request()).await;
    let (mock, server) = server().await;
    let addr = server.addr();
    hold(addr, 2).await;
    let first = spawn_request(addr, predicate_request());
    wait_for("erster Request gehalten", || {
        decisions_log(&mock).iter().filter(|r| r.held).count() == 1
    })
    .await;
    let second = spawn_request(addr, choice_request());
    wait_for("zweiter Request gehalten", || {
        decisions_log(&mock).iter().filter(|r| r.held).count() == 2
    })
    .await;
    assert_eq!(release(addr, Some(1)).await, json!({"released": 1}));
    let (status, body) = tokio::time::timeout(GUARD, first)
        .await
        .expect("älterer Request nach der Freigabe beantwortet")
        .unwrap();
    assert_eq!(status, 200);
    assert_eq!(body, expected_first, "Inhalt wie ohne hold");
    tokio::task::yield_now().await;
    assert!(!second.is_finished(), "jüngerer Request mit freigegeben");
    assert_eq!(release(addr, Some(1)).await, json!({"released": 1}));
    let (status, body) = tokio::time::timeout(GUARD, second)
        .await
        .expect("zweiter Request nach der zweiten Freigabe beantwortet")
        .unwrap();
    assert_eq!(status, 200);
    assert_eq!(body, expected_second);
    assert_eq!(release(addr, Some(1)).await, json!({"released": 0}));
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn aborted_held_request_is_logged_and_not_released() {
    let (mock, server) = server().await;
    let addr = server.addr();
    hold(addr, 1).await;
    let first = spawn_request(addr, predicate_request());
    wait_for("Request gehalten", || {
        decisions_log(&mock).iter().any(|r| r.held)
    })
    .await;
    // Client bricht die Verbindung ab.
    first.abort();
    let _ = first.await;
    wait_for("abgebrochener Request mit aborted: true", || {
        decisions_log(&mock).iter().any(|r| r.held && r.aborted)
    })
    .await;
    assert_eq!(release(addr, None).await, json!({"released": 0}));
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn release_without_held_and_unknown_provider() {
    let (_mock, server) = server().await;
    let addr = server.addr();
    assert_eq!(release(addr, None).await, json!({"released": 0}));
    let (status, _) = control(addr, "/__mock/release", json!({"provider": "nobody"})).await;
    assert_eq!(status, 400, "unbekannter Anbieter wie bei /__mock/scenario");
    let (status, _) = control(
        addr,
        "/__mock/scenario",
        json!({"provider": "nobody", "scenario": "hold", "count": 1}),
    )
    .await;
    assert_eq!(status, 400);
}

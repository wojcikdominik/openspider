//! Selbsttest der ai-mock-Erweiterung `hold` (08 Konventionen, Nachtrag 26.11.2026, K4-13; M4-D01): gehaltene
//! Requests stehen sofort mit `held: true` im Protokoll, werden erst nach `POST /__mock/release` beantwortet (mit der
//! Antwort, die ohne `hold` gekommen wäre) und erscheinen nach einem Verbindungsabbruch des Clients mit
//! `aborted: true`, ohne bei `release` mitzuzählen. Die Tests binden sich an den Zustand des Protokolls, nicht an die
//! Wanduhr (das Warten auf einen Protokollzustand hat nur eine großzügige Obergrenze gegen Hänger).
mod common;
use common::Resp;
use os_testsite::mocks::{ai::AiMock, MockServer};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::{io::AsyncWriteExt, net::TcpStream};

const DECISIONS: &str = "/openai/v1/decisions";

async fn server() -> (AiMock, MockServer) {
    let mock = AiMock::fixtures().unwrap();
    let s = MockServer::start(mock.registrations()).await.unwrap();
    (mock, s)
}

async fn send(stream: &mut TcpStream, path: &str, body: &str) {
    let head = format!("POST {path} HTTP/1.1\r\nHost: mock.test\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len());
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(body.as_bytes()).await.unwrap();
}

async fn call(addr: std::net::SocketAddr, path: &str, body: String) -> Resp {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    send(&mut stream, path, &body).await;
    let (raw, err) = common::read_all(&mut stream).await;
    assert!(err.is_none(), "{err:?}");
    common::parse(&raw).unwrap()
}

fn json_of(r: &Resp) -> Value {
    serde_json::from_slice(&r.body).unwrap()
}

fn decisions_body(marker: &str) -> String {
    json!({"model":"gpt-6-luna","input":format!("Page MOCK-ID:{marker}"),
        "questions":[{"name":"page-type","type":"choice","instructions":"Which type?",
            "choices":[{"value":"product","description":"p"},{"value":"other","description":"o"}]}]})
    .to_string()
}

/// Protokolleinträge an `path` als JSON (wie `GET /__mock/log`).
async fn log_entries(addr: std::net::SocketAddr, path: &str) -> Vec<Value> {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let head = "GET /__mock/log HTTP/1.1\r\nHost: mock.test\r\nConnection: close\r\n\r\n";
    stream.write_all(head.as_bytes()).await.unwrap();
    let (raw, _) = common::read_all(&mut stream).await;
    let log: Value = json_of(&common::parse(&raw).unwrap());
    log.as_array()
        .unwrap()
        .iter()
        .filter(|e| e["path"] == path)
        .cloned()
        .collect()
}

/// Wartet, bis `pred` auf dem Protokoll gilt (Obergrenze 20 s nur gegen Hänger).
async fn until(addr: std::net::SocketAddr, what: &str, pred: impl Fn(&[Value]) -> bool) {
    for _ in 0..2000 {
        if pred(&log_entries(addr, DECISIONS).await) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Protokollzustand nicht erreicht: {what}");
}

async fn control(addr: std::net::SocketAddr, path: &str, body: Value) -> Resp {
    call(addr, path, body.to_string()).await
}

// spec: AI-020 [M4]
// Mit `hold` und `count = 2` stehen nach zwei Requests beide mit `held: true` im Log, keiner ist beantwortet; ein
// dritter wird sofort beantwortet. `release` mit `count = 1` beantwortet genau den älteren mit dem Inhalt, den er ohne
// `hold` erhalten hätte; ein zweites `release` den anderen; ein drittes liefert `{"released": 0}`.
#[tokio::test]
async fn hold_keeps_requests_until_release_in_arrival_order() {
    let (_, s) = server().await;
    let addr = s.addr();
    // Vergleichswert ohne `hold`.
    let plain = json_of(&call(addr, DECISIONS, decisions_body("product-1")).await);
    assert_eq!(
        control(
            addr,
            "/__mock/scenario",
            json!({"provider":"openai","scenario":"hold","count":2})
        )
        .await
        .status,
        200
    );
    let first = tokio::spawn(call(addr, DECISIONS, decisions_body("product-1")));
    until(addr, "erster gehalten", |l| {
        l.iter().filter(|e| e["held"] == true).count() == 1
    })
    .await;
    let second = tokio::spawn(call(addr, DECISIONS, decisions_body("home")));
    until(addr, "zweiter gehalten", |l| {
        l.iter().filter(|e| e["held"] == true).count() == 2
    })
    .await;
    assert!(!first.is_finished() && !second.is_finished());
    // Dritter Request: sofort beantwortet, nicht gehalten.
    let third = call(addr, DECISIONS, decisions_body("home")).await;
    assert_eq!(third.status, 200);
    let log = log_entries(addr, DECISIONS).await;
    assert_eq!(log.len(), 4);
    assert!(log[3].get("held").is_none());
    assert!(!first.is_finished() && !second.is_finished());

    let r = json_of(&control(addr, "/__mock/release", json!({"provider":"openai","count":1})).await);
    assert_eq!(r, json!({"released": 1}));
    let first = first.await.unwrap();
    assert_eq!(first.status, 200);
    let mut a = json_of(&first);
    let mut b = plain.clone();
    // Nur die Antwortdaten vergleichen (die Request-ID im Header läuft mit).
    a.as_object_mut().unwrap().remove("id");
    b.as_object_mut().unwrap().remove("id");
    assert_eq!(a, b, "Antwort wie ohne hold");
    assert!(!second.is_finished());
    let r = json_of(&control(addr, "/__mock/release", json!({"provider":"openai","count":1})).await);
    assert_eq!(r, json!({"released": 1}));
    assert_eq!(second.await.unwrap().status, 200);
    let r = json_of(&control(addr, "/__mock/release", json!({"provider":"openai","count":1})).await);
    assert_eq!(r, json!({"released": 0}));
    // Unbekannter Anbieter → 400 wie bei `/__mock/scenario`.
    assert_eq!(
        control(addr, "/__mock/release", json!({"provider":"nope"})).await.status,
        400
    );
}

// spec: AI-020 [M4]
// Ein vom Client abgebrochener gehaltener Request erscheint mit `aborted: true` und zählt bei `release` nicht;
// `release` ohne `count` gibt alle übrigen frei.
#[tokio::test]
async fn aborted_held_request_is_marked_and_not_released() {
    let (_, s) = server().await;
    let addr = s.addr();
    control(
        addr,
        "/__mock/scenario",
        json!({"provider":"openai","scenario":"hold","count":2}),
    )
    .await;
    let mut doomed = TcpStream::connect(addr).await.unwrap();
    send(&mut doomed, DECISIONS, &decisions_body("home")).await;
    until(addr, "gehalten", |l| l.iter().any(|e| e["held"] == true)).await;
    let kept = tokio::spawn(call(addr, DECISIONS, decisions_body("product-1")));
    until(addr, "beide gehalten", |l| {
        l.iter().filter(|e| e["held"] == true).count() == 2
    })
    .await;
    drop(doomed);
    until(addr, "abgebrochen", |l| l.iter().any(|e| e["aborted"] == true)).await;
    let log = log_entries(addr, DECISIONS).await;
    assert_eq!(log[0]["aborted"], true);
    assert!(log[1].get("aborted").is_none());
    let r = json_of(&control(addr, "/__mock/release", json!({"provider":"openai"})).await);
    assert_eq!(r, json!({"released": 1}));
    assert_eq!(kept.await.unwrap().status, 200);
}

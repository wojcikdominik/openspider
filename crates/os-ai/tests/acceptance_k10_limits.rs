//! Unabhängige Abnahmetests M4-K10: Reset-Angaben der Rate-Limit-Header in Go-Schreibweise (AI-017, Nachtrag
//! 24.11.2026, Q00-N8).
//!
//! OpenAI liefert `x-ratelimit-reset-requests` und `x-ratelimit-reset-tokens` als Go-Dauer (`2ms`, `0s`, `6m0s`,
//! `1m30.5s`; Einheiten `h`, `m`, `s`, `ms`, `us`/`µs`, `ns`, auch kombiniert, Dezimalstellen erlaubt). Ein nicht lesbarer
//! Wert gilt als „keine Angabe“: keine Pause aus dem Header, kein Fehler. `anthropic-ratelimit-output-tokens-*` wird nicht
//! ausgewertet. Die Header kommen aus eigenen Fixtures (der ai-mock sendet keine Rate-Limit-Header); Zeitprüfungen laufen
//! mit angehaltener Tokio-Uhr als Testuhr.
// acceptance: independent

mod acceptance_k10_support;

use std::sync::Arc;
use std::time::Duration;

use acceptance_k10_support::*;
use os_ai::limits::headers::{from_header_map, parse_go_duration, parse_openai};
use os_ai::limits::{AttemptFailure, Limiter, LimitsConfig, RequestSpec, TokioClock};
use os_ai::provider::ReasoningLevel;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Value};

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    map
}

/// Wartezeit des nächsten Requests an `provider` nach Übernahme der Header einer Antwort.
async fn queued_after(provider: &str, pairs: &[(&str, &str)], estimated_tokens: u64) -> Duration {
    let limiter = Limiter::new(LimitsConfig::default(), Arc::new(TokioClock::new()));
    limiter.observe_headers(provider, &headers(pairs));
    let spec = RequestSpec::new(provider, "model", estimated_tokens);
    let exec = limiter
        .run(&spec, |_| async { Ok::<_, AttemptFailure>(()) })
        .await;
    assert!(exec.result.is_ok());
    assert_eq!(exec.attempts, 1);
    exec.queued
}

// spec: AI-017 [M4]
// acceptance: independent
// Go-Dauern laut Nachtrag: `1s`, `6m0s`, `1h2m3s`, `120ms`, Dezimalwerte, `us`/`µs`, `ns`, kombiniert (Fixture).
#[test]
fn go_durations_are_read() {
    let doc: Value = serde_json::from_str(&fixture_text("go-durations.json")).unwrap();
    for case in doc["valid"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let nanos = case["nanos"].as_u64().unwrap();
        assert_eq!(
            parse_go_duration(text),
            Some(Duration::from_nanos(nanos)),
            "{text}"
        );
    }
}

// spec: AI-017 [M4]
// acceptance: independent
// Ein nicht lesbarer Wert gilt als „keine Angabe“ (Negativfälle aus der Fixture, darunter `soon`).
#[test]
fn unreadable_durations_are_no_information() {
    let doc: Value = serde_json::from_str(&fixture_text("go-durations.json")).unwrap();
    for text in doc["invalid"].as_array().unwrap() {
        let text = text.as_str().unwrap();
        assert_eq!(parse_go_duration(text), None, "{text:?}");
    }
    let snapshot = parse_openai(|name: &str| match name {
        "x-ratelimit-remaining-requests" => Some("0".into()),
        "x-ratelimit-reset-requests" => Some("soon".into()),
        _ => None,
    });
    assert_eq!(snapshot.requests_remaining, Some(0));
    assert_eq!(snapshot.requests_reset_in, None, "keine Angabe");
}

// spec: AI-017 [M4]
// acceptance: independent
// Kriterium: Antwort mit `x-ratelimit-remaining-requests: 0` und `x-ratelimit-reset-requests: 1m30.5s` → nächster
// OpenAI-Request frühestens nach 90,5 s (Testuhr); `2ms` → nach 2 ms; `0s` → sofort; Wert `soon` → keine Pause.
#[tokio::test(start_paused = true)]
async fn openai_request_reset_pauses_the_next_request() {
    for (reset, expected) in [
        ("1m30.5s", Duration::from_millis(90_500)),
        ("2ms", Duration::from_millis(2)),
        ("0s", Duration::ZERO),
        ("soon", Duration::ZERO),
    ] {
        let queued = queued_after(
            "openai",
            &[
                ("x-ratelimit-remaining-requests", "0"),
                ("x-ratelimit-reset-requests", reset),
            ],
            10,
        )
        .await;
        assert!(queued >= expected, "{reset}: {queued:?} < {expected:?}");
        assert!(
            queued < expected + Duration::from_millis(500),
            "{reset}: {queued:?} deutlich über {expected:?}"
        );
    }
}

// spec: AI-017 [M4]
// acceptance: independent
// `x-ratelimit-reset-tokens` in Go-Schreibweise: Rest-Tokens 0 bei geschätzten Eingabe-Tokens > 0 → Pause bis zum Reset
// (`6m0s` = 360 s, `1h2m3s` = 3.723 s).
#[tokio::test(start_paused = true)]
async fn openai_token_reset_in_go_notation() {
    for (reset, expected) in [
        ("6m0s", Duration::from_secs(360)),
        ("1h2m3s", Duration::from_secs(3_723)),
    ] {
        let queued = queued_after(
            "openai",
            &[
                ("x-ratelimit-remaining-tokens", "0"),
                ("x-ratelimit-reset-tokens", reset),
            ],
            2_500,
        )
        .await;
        assert!(queued >= expected, "{reset}: {queued:?}");
        assert!(
            queued < expected + Duration::from_secs(1),
            "{reset}: {queued:?}"
        );
    }
}

// spec: AI-017 [M4]
// acceptance: independent
// Kriterium: `anthropic-ratelimit-output-tokens-remaining: 0` allein löst keine Pause aus; ebenso
// `x-ratelimit-remaining-input-images: 0` bei OpenAI (bewusst nicht ausgewertet).
#[tokio::test(start_paused = true)]
async fn output_token_and_image_headers_are_ignored() {
    let queued = queued_after(
        "anthropic",
        &[
            ("anthropic-ratelimit-output-tokens-remaining", "0"),
            (
                "anthropic-ratelimit-output-tokens-reset",
                "2099-01-01T00:00:00Z",
            ),
        ],
        2_500,
    )
    .await;
    assert_eq!(queued, Duration::ZERO);
    assert!(from_header_map(
        "anthropic",
        &headers(&[("anthropic-ratelimit-output-tokens-remaining", "0")]),
        0
    )
    .is_none_or(|s| s.is_empty()));

    let queued = queued_after(
        "openai",
        &[
            ("x-ratelimit-remaining-input-images", "0"),
            ("x-ratelimit-reset-input-images", "6m0s"),
        ],
        10,
    )
    .await;
    assert_eq!(queued, Duration::ZERO);
}

// spec: AI-017 [M4]
// acceptance: independent
// Kriterium: Wert `soon` → keine Pause, kein Fehler im Adapter (echte Antwort eines eigenen Testservers mit diesen
// Headern über den Responses-Adapter).
#[tokio::test]
async fn unreadable_reset_is_no_adapter_error() {
    let stub = Stub::start(
        Canned::json(
            200,
            json!({"id": "resp_k10", "object": "response", "status": "completed",
                   "output": [{"type": "message", "role": "assistant",
                               "content": [{"type": "output_text", "text": "Title"}]}],
                   "usage": {"input_tokens": 5, "output_tokens": 1}}),
        )
        .with_header("x-ratelimit-remaining-requests", "0")
        .with_header("x-ratelimit-reset-requests", "soon")
        .with_header("x-ratelimit-remaining-tokens", "0")
        .with_header("x-ratelimit-reset-tokens", "later"),
    )
    .await;
    let limiter = fast_limiter();
    let adapter =
        os_ai::adapters::openai_responses::OpenAiResponses::new(openai_http(&stub.url("/v1")));
    let r = request(
        model("openai", "gpt-6-luna", "responses", luna_levels()),
        "K10 headers",
        None,
        ReasoningLevel::Off,
        Some(64),
    );
    let reply = adapter
        .send(&r, None, Some(&limiter))
        .await
        .expect("kein Fehler im Adapter");
    assert_eq!(reply.http_status, 200);
    let started = std::time::Instant::now();
    let exec = limiter
        .run(&RequestSpec::new("openai", "gpt-6-luna", 10), |_| async {
            Ok::<_, AttemptFailure>(())
        })
        .await;
    assert!(exec.result.is_ok());
    assert!(exec.queued < Duration::from_secs(1), "{:?}", exec.queued);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(stub.requests(), 1);
}

//! M4-K10: Reset-Angaben der OpenAI-Rate-Limit-Header in Go-Schreibweise und nicht ausgewertete Zusatz-Header
//! (AI-017, Nachtrag 24.11.2026, Q00-N8). Eigene Header-Fixtures (der ai-mock sendet keine Rate-Limit-Header), Testuhr
//! über die angehaltene Tokio-Uhr.
use std::sync::Arc;
use std::time::Duration;

use os_ai::limits::headers::{from_header_map, parse_go_duration};
use os_ai::limits::{Limiter, LimitsConfig, RequestSpec, TokioClock};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in pairs {
        map.insert(
            HeaderName::from_bytes(k.as_bytes()).unwrap(),
            HeaderValue::from_str(v).unwrap(),
        );
    }
    map
}

/// Start des nächsten Requests nach einer Antwort mit `pairs` (Testuhr, Zeit ab Antwort).
async fn next_start(provider: &str, pairs: &[(&str, &str)]) -> Duration {
    let limiter = Limiter::new(
        LimitsConfig::default(),
        Arc::new(TokioClock::with_wall_origin(1_796_083_200_000)),
    );
    limiter.observe_headers(provider, &headers(pairs));
    let exec = limiter
        .run(&RequestSpec::new(provider, "m", 300), |_| async {
            Ok::<_, os_ai::limits::AttemptFailure>(())
        })
        .await;
    assert!(exec.result.is_ok(), "kein Fehler");
    exec.attempt_starts[0]
}

// spec: AI-017 [M4]
// `x-ratelimit-remaining-requests: 0` und `x-ratelimit-reset-requests: 1m30.5s` → nächster OpenAI-Request frühestens
// nach 90,5 s; `2ms` → nach 2 ms; `0s` → sofort; Wert `soon` → keine Pause, kein Fehler.
#[tokio::test(start_paused = true)]
async fn openai_reset_in_go_duration_delays_the_next_request() {
    let case = |reset: &'static str| {
        [
            ("x-ratelimit-remaining-requests", "0"),
            ("x-ratelimit-reset-requests", reset),
        ]
    };
    assert_eq!(
        next_start("openai", &case("1m30.5s")).await,
        Duration::from_millis(90_500)
    );
    assert_eq!(
        next_start("openai", &case("2ms")).await,
        Duration::from_millis(2)
    );
    assert_eq!(next_start("openai", &case("0s")).await, Duration::ZERO);
    assert_eq!(next_start("openai", &case("soon")).await, Duration::ZERO);
    // Token-Grenze mit Go-Dauer: Rest 100 < geschätzte 300 → Pause bis Reset.
    assert_eq!(
        next_start(
            "openai",
            &[
                ("x-ratelimit-remaining-tokens", "100"),
                ("x-ratelimit-reset-tokens", "6m0s"),
            ],
        )
        .await,
        Duration::from_secs(360)
    );
}

// spec: AI-017 [M4]
// Go-Schreibweise: Einheiten `h`, `m`, `s`, `ms`, `us`/`µs`, `ns`, kombiniert und mit Dezimalstellen; nicht lesbar →
// keine Angabe.
#[test]
fn go_durations_cover_all_units() {
    let ms = |v: u64| Some(Duration::from_millis(v));
    assert_eq!(parse_go_duration("1m30.5s"), ms(90_500));
    assert_eq!(parse_go_duration("2ms"), ms(2));
    assert_eq!(parse_go_duration("0s"), ms(0));
    assert_eq!(parse_go_duration("6m0s"), ms(360_000));
    assert_eq!(parse_go_duration("1h"), ms(3_600_000));
    assert_eq!(parse_go_duration("1.5h2m"), ms(5_520_000));
    assert_eq!(parse_go_duration("250us"), Some(Duration::from_micros(250)));
    assert_eq!(parse_go_duration("250µs"), Some(Duration::from_micros(250)));
    assert_eq!(parse_go_duration("40ns"), Some(Duration::from_nanos(40)));
    for bad in ["soon", "", "5", "1x", "m5", "-1s"] {
        assert_eq!(parse_go_duration(bad), None, "{bad:?}");
    }
    let snapshot = from_header_map(
        "openai",
        &headers(&[
            ("x-ratelimit-remaining-requests", "0"),
            ("x-ratelimit-reset-requests", "soon"),
        ]),
        0,
    )
    .unwrap();
    assert_eq!(snapshot.requests_remaining, Some(0));
    assert_eq!(snapshot.requests_reset_in, None, "keine Angabe");
}

// spec: AI-017 [M4]
// `anthropic-ratelimit-output-tokens-remaining: 0` allein löst keine Pause aus; ebenso `x-ratelimit-*-input-images`.
#[tokio::test(start_paused = true)]
async fn output_token_and_image_headers_are_ignored() {
    assert_eq!(
        next_start(
            "anthropic",
            &[
                ("anthropic-ratelimit-output-tokens-remaining", "0"),
                ("anthropic-ratelimit-output-tokens-reset", "2026-12-01T00:05:00Z"),
            ],
        )
        .await,
        Duration::ZERO
    );
    assert!(from_header_map(
        "anthropic",
        &headers(&[("anthropic-ratelimit-output-tokens-remaining", "0")]),
        0
    )
    .is_none());
    assert_eq!(
        next_start(
            "openai",
            &[
                ("x-ratelimit-remaining-input-images", "0"),
                ("x-ratelimit-reset-input-images", "1m0s"),
            ],
        )
        .await,
        Duration::ZERO
    );
}

//! Tokens und Anfragen je Minute und Modell, Rate-Limit-Header der Anbieter, Pausenhinweis (AI-017, INTEG-171).
//! Testuhr: angehaltene Tokio-Uhr.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use os_ai::limits::headers::{parse_anthropic, parse_openai};
use os_ai::limits::{
    AttemptFailure, FixedJitter, Limiter, LimitsConfig, ModelLimits, PauseReason, RequestSpec,
    TokenRounding, TokioClock,
};
use os_ai::provider::ProviderError;
use serde_json::Value;

const WALL: i64 = 1_795_514_400_000; // 2026-11-24T10:00:00Z

fn limiter(config: LimitsConfig) -> Arc<Limiter> {
    Arc::new(Limiter::with_jitter(
        config,
        Arc::new(TokioClock::with_wall_origin(WALL)),
        Arc::new(FixedJitter(0.5)),
    ))
}

/// Startet `n` Einträge gleichzeitig und liefert ihre Startzeitpunkte.
async fn burst(limiter: &Arc<Limiter>, spec: RequestSpec, n: usize) -> Vec<Duration> {
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..n {
        let (l, spec) = (limiter.clone(), spec.clone());
        tasks.spawn(async move {
            l.run(&spec, |_| async { Ok::<_, AttemptFailure>(()) })
                .await
                .attempt_starts[0]
        });
    }
    let mut starts = Vec::new();
    while let Some(start) = tasks.join_next().await {
        starts.push(start.unwrap());
    }
    starts.sort();
    starts
}

/// Größte Zahl von Starts in einem beliebigen 60-s-Fenster [t, t + 60 s).
fn max_per_window(starts: &[Duration]) -> usize {
    starts
        .iter()
        .map(|t| {
            starts
                .iter()
                .filter(|s| **s >= *t && **s < *t + Duration::from_secs(60))
                .count()
        })
        .max()
        .unwrap_or(0)
}

// spec: AI-017
// TPM 10.000 bei geschätzten 2.500 Tokens je Request → in keinem 60-s-Fenster mehr als 4 Requests an dieses Modell.
#[tokio::test(start_paused = true)]
async fn tokens_per_minute_cap_requests_per_window() {
    let mut config = LimitsConfig::default();
    config.set_model(
        "anthropic",
        "mock-claude",
        ModelLimits {
            tokens_per_minute: Some(10_000),
            ..ModelLimits::default()
        },
    );
    let limiter = limiter(config);
    let starts = burst(
        &limiter,
        RequestSpec::new("anthropic", "mock-claude", 2_500),
        12,
    )
    .await;
    assert_eq!(starts.len(), 12);
    assert_eq!(max_per_window(&starts), 4);
    assert_eq!(starts[4], Duration::from_secs(60), "fünfter erst nach 60 s");
    // Ein anderes Modell desselben Anbieters ist nicht betroffen.
    let other = burst(
        &limiter,
        RequestSpec::new("anthropic", "mock-claude-b", 2_500),
        6,
    )
    .await;
    assert!(other.iter().all(|s| *s == other[0]));
}

// spec: INTEG-171
// RPM 20 → in keinem 60-s-Fenster mehr als 20 Anfragen an dieses Modell; Modelle getrennt gedrosselt.
#[tokio::test(start_paused = true)]
async fn requests_per_minute_are_capped_per_model() {
    let mut config = LimitsConfig::default();
    for model in ["gpt-6-luna", "gpt-6-luna-mini"] {
        config.set_model(
            "openai",
            model,
            ModelLimits {
                requests_per_minute: Some(20),
                ..ModelLimits::default()
            },
        );
    }
    let limiter = limiter(config);
    let (a, b) = tokio::join!(
        burst(&limiter, RequestSpec::new("openai", "gpt-6-luna", 10), 50),
        burst(
            &limiter,
            RequestSpec::new("openai", "gpt-6-luna-mini", 10),
            20
        ),
    );
    assert_eq!(max_per_window(&a), 20);
    assert_eq!(a[20], Duration::from_secs(60));
    assert_eq!(a[40], Duration::from_secs(120));
    assert!(b.iter().all(|s| s.is_zero()), "zweites Modell ungebremst");
}

fn header_fixture() -> Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in [
        "..",
        "..",
        "tests",
        "fixtures",
        "ai",
        "k06",
        "ratelimit-headers.json",
    ] {
        path.push(part);
    }
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn header_map(headers: &Value) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    for (name, value) in headers.as_object().unwrap() {
        map.insert(
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.as_str().unwrap().parse().unwrap(),
        );
    }
    map
}

// spec: AI-017
// Rate-Limit-Header (Fixture): Anthropic Requests-Rest 0 und Reset in 20 s → nächster Request frühestens nach 20 s,
// ohne 429; Input-Tokens-Rest 0 bei 300 geschätzten Tokens → keine Pause (0 + 499 ≥ 300), bei 2.500 → Pause bis
// zum Reset; `anthropic-ratelimit-tokens-remaining: 0` allein → keine Pause; OpenAI exakt ohne Intervallregel.
#[tokio::test(start_paused = true)]
async fn provider_headers_pause_until_reset() {
    let fixture = header_fixture();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let provider = case["provider"].as_str().unwrap();
        let tokens = case["next_input_tokens"].as_u64().unwrap();
        let wait = Duration::from_secs(case["expected_wait_s"].as_u64().unwrap());
        let limiter = limiter(LimitsConfig::default());
        let origin = limiter.clock().now();
        limiter.observe_headers(provider, &header_map(&case["headers"]));
        let clock = limiter.clock().clone();
        // Wie der Anbieter: vor dem Reset gäbe es ein 429.
        let run = limiter
            .run(&RequestSpec::new(provider, "m", tokens), |_| {
                let clock = clock.clone();
                async move {
                    if clock.now() < origin + wait {
                        Err(AttemptFailure::from(ProviderError::RateLimited {
                            retry_after_seconds: None,
                        }))
                    } else {
                        Ok(())
                    }
                }
            })
            .await;
        assert_eq!(run.attempts, 1, "{name}: kein 429");
        assert_eq!(run.attempt_starts[0] - origin, wait, "{name}");
    }
}

// spec: AI-017
// Parser: Anthropic rundet auf Tausender (Intervallregel), OpenAI exakt; Reset als Zeitpunkt bzw. Dauer.
#[test]
fn header_parsers_read_remaining_and_reset() {
    let get = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.to_string())
        }
    };
    let a = parse_anthropic(
        get(&[
            ("anthropic-ratelimit-requests-remaining", "0"),
            ("anthropic-ratelimit-requests-reset", "2026-11-24T10:00:20Z"),
            ("anthropic-ratelimit-tokens-remaining", "0"),
        ]),
        WALL,
    );
    assert_eq!(a.requests_remaining, Some(0));
    assert_eq!(a.requests_reset_in, Some(Duration::from_secs(20)));
    assert_eq!(
        a.input_tokens_remaining, None,
        "kombinierter Header zählt nicht"
    );
    assert_eq!(a.rounding, TokenRounding::Thousands);
    assert_eq!(TokenRounding::Thousands.upper_bound(0), 499);
    let o = parse_openai(get(&[
        ("x-ratelimit-remaining-tokens", "2499"),
        ("x-ratelimit-reset-tokens", "6m0s"),
    ]));
    assert_eq!(o.input_tokens_remaining, Some(2499));
    assert_eq!(o.input_tokens_reset_in, Some(Duration::from_secs(360)));
    assert_eq!(TokenRounding::Exact.upper_bound(2499), 2499);
}

// spec: AI-017
// Hinweis „<Anbieter> pausiert bis <Uhrzeit> (Rate-Limit)“ für die API-Fortschrittsanzeige.
#[tokio::test(start_paused = true)]
async fn pause_notice_names_provider_and_end_time() {
    let limiter = limiter(LimitsConfig::default());
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "anthropic-ratelimit-requests-remaining",
        "0".parse().unwrap(),
    );
    headers.insert(
        "anthropic-ratelimit-requests-reset",
        "2026-11-24T10:00:20Z".parse().unwrap(),
    );
    limiter.observe_headers("anthropic", &headers);
    let pauses = limiter.pauses(100);
    assert_eq!(pauses.len(), 1);
    assert_eq!(pauses[0].provider, "anthropic");
    assert_eq!(pauses[0].reason, PauseReason::RateLimit);
    assert_eq!(pauses[0].until_unix_millis, WALL + 20_000);
    assert_eq!(
        pauses[0].rate_limit_text("Anthropic", "10:00:20"),
        "Anthropic pausiert bis 10:00:20 (Rate-Limit)"
    );
    tokio::time::sleep(Duration::from_secs(21)).await;
    assert!(limiter.pauses(100).is_empty());
}

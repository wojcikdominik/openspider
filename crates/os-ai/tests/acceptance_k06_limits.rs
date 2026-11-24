//! Unabhängige Abnahmetests M4-K06: Wiederholungen, Rate-Limits, Timeouts (AI-017) mit INTEG-005, INTEG-171,
//! INTEG-179 und dem Status `Error: Connection failed` (AI-020, Nachtrag 24.11.2026).
//!
//! Alle zeitabhängigen Tests laufen auf der angehaltenen Tokio-Uhr (`start_paused`) mit der Uhr des Pakets
//! (`TokioClock`); es wird nicht echt gewartet. Der Verbindungstest nutzt eine echte TCP-Verbindung zu einem
//! geschlossenen Loopback-Port.
// acceptance: independent

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_ai::limits::{
    headers, AttemptFailure, BackoffPolicy, Execution, FixedJitter, Limiter, LimitsConfig,
    ModelLimits, PauseReason, ProviderLimits, RandomJitter, RequestSpec, TokioClock,
    TransportCause,
};
use os_ai::provider::ProviderError;
use os_ai::AiStatus;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

/// Feste Wanduhr des Testbeginns: 2026-11-24T10:00:00Z.
const WALL0: i64 = 1_795_514_400_000;

fn ts(offset_s: i64) -> String {
    os_model::Timestamp::from_unix_millis(WALL0 + offset_s * 1000).to_db()
}

fn limiter_with(config: LimitsConfig, jitter: f64) -> Arc<Limiter> {
    Arc::new(Limiter::with_jitter(
        config,
        Arc::new(TokioClock::with_wall_origin(WALL0)),
        Arc::new(FixedJitter(jitter)),
    ))
}

fn limiter(jitter: f64) -> Arc<Limiter> {
    limiter_with(LimitsConfig::default(), jitter)
}

fn http(status: u16) -> AttemptFailure {
    AttemptFailure::from(ProviderError::Http {
        status,
        message: format!("status {status}"),
    })
}

async fn always_fail(l: &Limiter, spec: &RequestSpec, status: u16) -> Execution<()> {
    l.run(spec, |_| async move { Err::<(), _>(http(status)) })
        .await
}

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

fn gaps(starts: &[Duration]) -> Vec<f64> {
    starts.windows(2).map(|w| secs(w[1] - w[0])).collect()
}

/// Größte Zahl von Starts in einem beliebigen halboffenen 60-s-Fenster.
fn max_in_window(starts: &[Duration], window: Duration) -> usize {
    let mut s: Vec<Duration> = starts.to_vec();
    s.sort();
    (0..s.len())
        .map(|i| s[i..].iter().take_while(|t| **t < s[i] + window).count())
        .max()
        .unwrap_or(0)
}

fn headers_of(pairs: &[(&str, String)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    map
}

/// Start des ersten Versuchs auf der Uhr des Limiters (Ursprung = Anlegen des Limiters zu Testbeginn).
async fn first_start(l: &Limiter, spec: &RequestSpec) -> Duration {
    let exec = l.run(spec, |_| async { Ok::<_, AttemptFailure>(()) }).await;
    assert!(exec.result.is_ok());
    exec.attempt_starts[0]
}

// ---------------------------------------------------------------------------------------------------------------
// INTEG-005: Backoff, Wiederholungen, Retry-After

// spec: INTEG-005
// spec: AI-017
// acceptance: independent
#[test]
fn backoff_policy_parameters_are_fixed_as_specified() {
    let p = BackoffPolicy::INTEG_005;
    assert_eq!(p.base, Duration::from_secs(1));
    assert!((p.factor - 2.0).abs() < 1e-12);
    assert_eq!(p.cap, Duration::from_secs(60));
    assert!((p.jitter - 0.2).abs() < 1e-12);
    assert_eq!(LimitsConfig::default().max_retries, 5);
    for n in 1..=10u32 {
        let lo = (0.8 * 2f64.powi(n as i32 - 1)).min(60.0);
        let hi = (1.2 * 2f64.powi(n as i32 - 1)).min(60.0);
        for u in [0.0, 0.25, 0.5, 0.75, 0.999_999] {
            let d = secs(p.delay(n, u));
            assert!(
                d >= lo - 1e-9 && d <= hi + 1e-9,
                "Wiederholung {n}, unit {u}: {d} s nicht in [{lo}, {hi}]"
            );
        }
    }
}

// spec: INTEG-005
// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn waits_between_attempts_follow_backoff_window() {
    for jitter in [0.0, 0.5, 0.999_999] {
        let l = limiter(jitter);
        let exec = always_fail(&l, &RequestSpec::new("openai", "gpt-6-luna", 100), 500).await;
        assert_eq!(exec.attempts, 6, "1 + 5 Wiederholungen");
        let g = gaps(&exec.attempt_starts);
        assert_eq!(g.len(), 5);
        for (i, gap) in g.iter().enumerate() {
            let n = i as i32 + 1;
            let lo = 0.8 * 2f64.powi(n - 1);
            let hi = (1.2 * 2f64.powi(n - 1)).min(60.0);
            assert!(
                *gap >= lo - 1e-6 && *gap <= hi + 1e-6,
                "Jitter {jitter}: Abstand Versuch {n}→{}: {gap} s nicht in [{lo}, {hi}]",
                n + 1
            );
        }
    }
    // Zufälliger Jitter (Standard) bleibt im selben Fenster.
    let l = Arc::new(Limiter::with_jitter(
        LimitsConfig::default(),
        Arc::new(TokioClock::with_wall_origin(WALL0)),
        Arc::new(RandomJitter),
    ));
    for _ in 0..5 {
        let exec = always_fail(&l, &RequestSpec::new("x", "m", 1), 503).await;
        for (i, gap) in gaps(&exec.attempt_starts).iter().enumerate() {
            let n = i as i32 + 1;
            assert!(*gap >= 0.8 * 2f64.powi(n - 1) - 1e-6);
            assert!(*gap <= (1.2 * 2f64.powi(n - 1)).min(60.0) + 1e-6);
        }
    }
}

// spec: INTEG-005
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn backoff_is_capped_at_60_seconds() {
    let config = LimitsConfig {
        max_retries: 9,
        circuit_breaker: false,
        ..LimitsConfig::default()
    };
    let l = limiter_with(config, 0.999_999);
    let exec = always_fail(&l, &RequestSpec::new("openai", "m", 1), 502).await;
    assert_eq!(exec.attempts, 10);
    for gap in gaps(&exec.attempt_starts) {
        assert!(gap <= 60.0 + 1e-6, "Obergrenze 60 s überschritten: {gap}");
    }
}

// spec: INTEG-005
// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn retry_after_seven_waits_at_least_seven_seconds_for_this_provider() {
    let l = limiter(0.5);
    let spec = RequestSpec::new("anthropic", "claude-x", 100);
    let l2 = l.clone();
    let main = tokio::spawn(async move {
        l2.run(
            &RequestSpec::new("anthropic", "claude-x", 100),
            |n| async move {
                if n == 1 {
                    Err(http(429).with_retry_after(Duration::from_secs(7)))
                } else {
                    Ok(())
                }
            },
        )
        .await
    });
    // Ein weiterer Eintrag desselben Anbieters, der während der Pause startet.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let other = l
        .run(&spec, |_| async { Ok::<_, AttemptFailure>(()) })
        .await;
    let exec = main.await.unwrap();
    assert!(exec.result.is_ok());
    assert_eq!(exec.attempts, 2);
    let gap = secs(exec.attempt_starts[1] - exec.attempt_starts[0]);
    assert!(gap >= 7.0, "Retry-After 7: Wartezeit {gap} s < 7 s");
    assert!(
        secs(other.attempt_starts[0]) >= 7.0,
        "nächste Anfrage an diesen Anbieter vor Ablauf von Retry-After: {:?}",
        other.attempt_starts[0]
    );
    // Ein anderer Anbieter ist nicht betroffen.
    let free = l
        .run(&RequestSpec::new("openai", "m", 1), |_| async {
            Ok::<_, AttemptFailure>(())
        })
        .await;
    assert!(free.queued.is_zero());
}

// spec: INTEG-005
// acceptance: independent
#[test]
fn retry_after_accepts_seconds_and_http_date() {
    let now = WALL0;
    assert_eq!(
        os_ai::limits::parse_retry_after("7", now),
        Some(Duration::from_secs(7))
    );
    // 2026-11-24T10:00:07Z als IMF-fixdate (Dienstag).
    let d = os_ai::limits::parse_retry_after("Tue, 24 Nov 2026 10:00:07 GMT", now).unwrap();
    assert!(
        d >= Duration::from_secs(7) && d <= Duration::from_millis(7_001),
        "{d:?}"
    );
}

// spec: INTEG-005
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn two_429_then_200_succeeds() {
    let l = limiter(0.5);
    let exec = l
        .run(&RequestSpec::new("openai", "m", 10), |n| async move {
            if n <= 2 {
                Err(http(429))
            } else {
                Ok("gültige Daten")
            }
        })
        .await;
    assert_eq!(exec.result.as_ref().ok(), Some(&"gültige Daten"));
    assert_eq!(exec.attempts, 3);
    assert_eq!(exec.error_status(), None);
}

// spec: INTEG-005
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn client_errors_are_never_retried() {
    for status in [400u16, 401, 403, 404] {
        let l = limiter(0.5);
        let calls = AtomicU32::new(0);
        let exec = l
            .run(&RequestSpec::new("openai", "m", 10), |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                async move { Err::<(), _>(http(status)) }
            })
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1, "{status}: genau 1 Request");
        assert_eq!(exec.attempts, 1);
        assert_eq!(exec.http_status(), Some(status));
        let text = exec.error_status().unwrap().to_string();
        assert!(
            text.starts_with(&format!("Error: {status} ")),
            "Status `Error: <HTTP-Code> <Meldung>` erwartet, war {text:?}"
        );
    }
}

// spec: INTEG-005
// spec: AI-020
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn five_failed_retries_end_as_final_error_after_six_requests() {
    let l = limiter(0.5);
    let calls = AtomicU32::new(0);
    let exec = l
        .run(&RequestSpec::new("openai", "m", 10), |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(http(500)) }
        })
        .await;
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    assert_eq!(exec.attempts, 6);
    assert_eq!(exec.retries(), 5);
    assert_eq!(exec.http_status(), Some(500));
    assert!(exec
        .error_status()
        .unwrap()
        .to_string()
        .starts_with("Error: 500 "));
}

// spec: INTEG-005
// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn concurrent_requests_per_provider_never_exceed_setting() {
    for (setting, expect_reached) in [(Some(2u32), 2usize), (None, 4usize)] {
        let mut config = LimitsConfig::default();
        if let Some(n) = setting {
            config.providers.insert(
                "openai".into(),
                ProviderLimits {
                    concurrent_requests: n,
                    timeout: Duration::from_secs(120),
                },
            );
        }
        let l = limiter_with(config, 0.5);
        let open = Arc::new(AtomicU32::new(0));
        let max_open = Arc::new(AtomicU32::new(0));
        let mut tasks = Vec::new();
        for _ in 0..12 {
            let l = l.clone();
            let open = open.clone();
            let max_open = max_open.clone();
            tasks.push(tokio::spawn(async move {
                l.run(&RequestSpec::new("openai", "m", 10), |_| {
                    let open = open.clone();
                    let max_open = max_open.clone();
                    async move {
                        let now = open.fetch_add(1, Ordering::SeqCst) + 1;
                        max_open.fetch_max(now, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        open.fetch_sub(1, Ordering::SeqCst);
                        Ok::<_, AttemptFailure>(())
                    }
                })
                .await
            }));
        }
        for t in tasks {
            assert!(t.await.unwrap().result.is_ok());
        }
        let max = max_open.load(Ordering::SeqCst) as usize;
        assert!(
            max <= expect_reached,
            "Einstellung {setting:?}: {max} offene Anfragen > {expect_reached}"
        );
        assert_eq!(max, expect_reached, "Grenze wird ausgeschöpft (Standard 4)");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// INTEG-171 / AI-017: Anfragen und Tokens je Minute und Modell

// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn tpm_10000_with_2500_tokens_allows_at_most_4_per_window() {
    let mut config = LimitsConfig::default();
    config.set_model(
        "anthropic",
        "claude-x",
        ModelLimits {
            requests_per_minute: None,
            tokens_per_minute: Some(10_000),
        },
    );
    let l = limiter_with(config, 0.5);
    let starts = Arc::new(Mutex::new(Vec::new()));
    let mut tasks = Vec::new();
    for _ in 0..13 {
        let l = l.clone();
        let starts = starts.clone();
        tasks.push(tokio::spawn(async move {
            let exec = l
                .run(
                    &RequestSpec::new("anthropic", "claude-x", 2_500),
                    |_| async { Ok::<_, AttemptFailure>(()) },
                )
                .await;
            starts.lock().unwrap().extend(exec.attempt_starts);
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let s = starts.lock().unwrap().clone();
    assert_eq!(s.len(), 13);
    let max = max_in_window(&s, Duration::from_secs(60));
    assert!(
        max <= 4,
        "{max} Requests in einem 60-s-Fenster (TPM 10.000 / 2.500)"
    );
    assert_eq!(max, 4);
}

// spec: INTEG-171
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn rpm_20_limits_window_and_models_are_throttled_separately() {
    let mut config = LimitsConfig::default();
    config.set_model(
        "openai",
        "a",
        ModelLimits {
            requests_per_minute: Some(20),
            tokens_per_minute: None,
        },
    );
    let l = limiter_with(config, 0.5);
    let mut starts = Vec::new();
    for _ in 0..45 {
        let exec = l
            .run(&RequestSpec::new("openai", "a", 1), |_| async {
                Ok::<_, AttemptFailure>(())
            })
            .await;
        starts.extend(exec.attempt_starts);
    }
    let max = max_in_window(&starts, Duration::from_secs(60));
    assert!(max <= 20, "{max} Requests in einem 60-s-Fenster bei RPM 20");
    // Modell b desselben Anbieters ist nicht gedrosselt, obwohl Modell a sein Limit erreicht hat.
    let clock_now = l.clock().now();
    let b = l
        .run(&RequestSpec::new("openai", "b", 1), |_| async {
            Ok::<_, AttemptFailure>(())
        })
        .await;
    assert_eq!(
        b.attempt_starts[0], clock_now,
        "Modell b muss sofort starten"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// AI-017 (2): Rate-Limit-Header

// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn anthropic_requests_remaining_zero_pauses_until_reset() {
    let l = limiter(0.5);
    l.observe_headers(
        "anthropic",
        &headers_of(&[
            ("anthropic-ratelimit-requests-remaining", "0".into()),
            ("anthropic-ratelimit-requests-reset", ts(20)),
        ]),
    );
    let pauses = l.pauses(100);
    assert!(
        pauses
            .iter()
            .any(|p| p.provider == "anthropic" && p.reason == PauseReason::RateLimit),
        "Pause-Hinweis erwartet: {pauses:?}"
    );
    let start = first_start(&l, &RequestSpec::new("anthropic", "claude-x", 100)).await;
    assert!(
        secs(start) >= 20.0 - 1e-3,
        "nächster Anthropic-Request startet nach {start:?}, erwartet ≥ 20 s"
    );
    assert!(
        secs(start) <= 21.0,
        "nicht länger als bis zum Reset: {start:?}"
    );
}

// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn anthropic_input_tokens_rounded_interval_decides_pause() {
    let hdr = || {
        headers_of(&[
            ("anthropic-ratelimit-input-tokens-remaining", "0".into()),
            ("anthropic-ratelimit-input-tokens-reset", ts(30)),
        ])
    };
    // 300 geschätzte Tokens: 0 + 499 ≥ 300 → keine Pause.
    let l = limiter(0.5);
    l.observe_headers("anthropic", &hdr());
    let start = first_start(&l, &RequestSpec::new("anthropic", "claude-x", 300)).await;
    assert!(start.is_zero(), "300 Tokens: keine Pause, Start {start:?}");
    // Grenze: 499 → keine Pause; 500 → Pause.
    let l = limiter(0.5);
    l.observe_headers("anthropic", &hdr());
    let start = first_start(&l, &RequestSpec::new("anthropic", "claude-x", 499)).await;
    assert!(start.is_zero(), "499 Tokens: keine Pause, Start {start:?}");
    // 2.500 geschätzte Tokens → Pause bis zum Reset.
    let l = limiter(0.5);
    l.observe_headers("anthropic", &hdr());
    let start = first_start(&l, &RequestSpec::new("anthropic", "claude-x", 2_500)).await;
    assert!(
        secs(start) >= 30.0 - 1e-3 && secs(start) <= 31.0,
        "2.500 Tokens: Pause bis Reset (30 s) erwartet, Start {start:?}"
    );
}

// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn anthropic_combined_tokens_header_alone_never_pauses() {
    let l = limiter(0.5);
    l.observe_headers(
        "anthropic",
        &headers_of(&[
            ("anthropic-ratelimit-tokens-remaining", "0".into()),
            ("anthropic-ratelimit-tokens-reset", ts(40)),
        ]),
    );
    let start = first_start(&l, &RequestSpec::new("anthropic", "claude-x", 2_500)).await;
    assert!(
        start.is_zero(),
        "kombinierter Header allein: keine Pause, Start {start:?}"
    );
}

// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn openai_rate_limit_headers_pause_until_reset() {
    // Request-Restzahl 0, Reset in 20 s.
    let l = limiter(0.5);
    l.observe_headers(
        "openai",
        &headers_of(&[
            ("x-ratelimit-remaining-requests", "0".into()),
            ("x-ratelimit-reset-requests", "20s".into()),
        ]),
    );
    let start = first_start(&l, &RequestSpec::new("openai", "gpt", 10)).await;
    assert!(
        secs(start) >= 20.0 - 1e-3 && secs(start) <= 21.0,
        "{start:?}"
    );
    // Rest-Tokens kleiner als Schätzung → Pause bis Reset; gleich groß → keine Pause.
    let tok = |remaining: &str| {
        headers_of(&[
            ("x-ratelimit-remaining-tokens", remaining.into()),
            ("x-ratelimit-reset-tokens", "30s".into()),
        ])
    };
    let l = limiter(0.5);
    l.observe_headers("openai", &tok("299"));
    let start = first_start(&l, &RequestSpec::new("openai", "gpt", 300)).await;
    assert!(
        secs(start) >= 30.0 - 1e-3 && secs(start) <= 31.0,
        "299 < 300: {start:?}"
    );
    let l = limiter(0.5);
    l.observe_headers("openai", &tok("300"));
    let start = first_start(&l, &RequestSpec::new("openai", "gpt", 300)).await;
    assert!(start.is_zero(), "300 ≥ 300: keine Pause, {start:?}");
}

// spec: AI-017
// acceptance: independent
#[test]
fn header_parsing_reads_the_specified_headers() {
    let get_a = |name: &str| match name {
        "anthropic-ratelimit-requests-remaining" => Some("0".to_string()),
        "anthropic-ratelimit-requests-reset" => Some(ts(20)),
        "anthropic-ratelimit-input-tokens-remaining" => Some("2000".to_string()),
        "anthropic-ratelimit-input-tokens-reset" => Some(ts(30)),
        "anthropic-ratelimit-tokens-remaining" => Some("0".to_string()),
        _ => None,
    };
    let s = headers::parse_anthropic(get_a, WALL0);
    assert_eq!(s.requests_remaining, Some(0));
    assert_eq!(s.requests_reset_in, Some(Duration::from_secs(20)));
    assert_eq!(s.input_tokens_remaining, Some(2000));
    assert_eq!(s.input_tokens_reset_in, Some(Duration::from_secs(30)));
    // Gerundeter Wert r steht für [r − 500, r + 499].
    assert_eq!(s.rounding.upper_bound(2000), 2499);
    let get_o = |name: &str| match name {
        "x-ratelimit-remaining-requests" => Some("5".to_string()),
        "x-ratelimit-reset-requests" => Some("1m30s".to_string()),
        "x-ratelimit-remaining-tokens" => Some("1234".to_string()),
        "x-ratelimit-reset-tokens" => Some("120ms".to_string()),
        _ => None,
    };
    let o = headers::parse_openai(get_o);
    assert_eq!(o.requests_remaining, Some(5));
    assert_eq!(o.requests_reset_in, Some(Duration::from_secs(90)));
    assert_eq!(o.input_tokens_remaining, Some(1234));
    assert_eq!(o.input_tokens_reset_in, Some(Duration::from_millis(120)));
    assert_eq!(o.rounding.upper_bound(1234), 1234);
}

// ---------------------------------------------------------------------------------------------------------------
// AI-017 (3): Schutzschalter

// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn breaker_pauses_provider_five_minutes_after_ten_final_errors_other_provider_unaffected() {
    let l = limiter(0.5);
    let a = RequestSpec::new("openrouter", "m", 10);
    let mut last_end = Duration::ZERO;
    for i in 0..10 {
        let exec = always_fail(&l, &a, 500).await;
        assert_eq!(exec.attempts, 6, "Eintrag {i}: 1 + 5 Versuche (500-always)");
        last_end = l.clock().now();
    }
    assert!(
        l.breaker_open("openrouter"),
        "Schutzschalter nach 10 endgültigen Fehlern"
    );
    assert!(!l.breaker_open("openai"));
    // Anbieter B läuft unbeeinflusst weiter.
    let b = l
        .run(&RequestSpec::new("openai", "m", 10), |_| async {
            Ok::<_, AttemptFailure>(())
        })
        .await;
    assert!(b.result.is_ok());
    assert_eq!(b.attempt_starts[0], last_end, "Anbieter B startet sofort");
    // Anbieter A sendet 5 Minuten lang nichts.
    let calls = AtomicU32::new(0);
    let next = l
        .run(&a, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok::<_, AttemptFailure>(()) }
        })
        .await;
    assert!(
        next.attempt_starts[0] >= last_end + Duration::from_secs(300),
        "erster Request an A nach dem Schutzschalter bei {:?}, Pause begann {:?}",
        next.attempt_starts[0],
        last_end
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// spec: AI-017
// spec: AI-020
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn breaker_counts_connection_failures() {
    let l = limiter(0.5);
    let spec = RequestSpec::new("ollama", "m", 10);
    for _ in 0..10 {
        let exec = l
            .run(&spec, |_| async {
                Err::<(), _>(AttemptFailure::transport(
                    TransportCause::Connect,
                    "connection refused",
                ))
            })
            .await;
        assert_eq!(exec.error_status(), Some(&AiStatus::ConnectionFailed));
    }
    assert!(
        l.breaker_open("ollama"),
        "Connection failed zählt für den Schutzschalter"
    );
}

// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn breaker_ignores_spend_cap_openrouter_402_and_refusal_403() {
    let l = limiter(0.5);
    let spec = RequestSpec::new("anthropic", "m", 10);
    // 9 echte endgültige Fehler: Schalter noch zu.
    for _ in 0..9 {
        always_fail(&l, &spec, 500).await;
    }
    assert!(!l.breaker_open("anthropic"));
    // Spend-Cap-429 (`enforced_spend_limit_reached`, AI-002): endgültig, kein Retry, zählt nicht.
    for _ in 0..3 {
        let calls = AtomicU32::new(0);
        let exec = l
            .run(&spec, |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                async {
                    let mut f = AttemptFailure::from(ProviderError::QuotaExceeded)
                        .with_error_code("enforced_spend_limit_reached");
                    f.http_status = Some(429);
                    Err::<(), _>(f)
                }
            })
            .await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "Spend-Cap-429 wird nicht wiederholt"
        );
        assert_eq!(exec.error_status(), Some(&AiStatus::QuotaExceeded));
    }
    assert!(!l.breaker_open("anthropic"), "Spend-Cap zählt nicht");
    // OpenRouter: endgültiges 402 und 403 mit error.type = refusal.
    let or = RequestSpec::new("openrouter", "m", 10);
    for _ in 0..9 {
        always_fail(&l, &or, 503).await;
    }
    for _ in 0..3 {
        let exec = l
            .run(&or, |_| async {
                let mut f = AttemptFailure::from(ProviderError::QuotaExceeded);
                f.http_status = Some(402);
                Err::<(), _>(f)
            })
            .await;
        assert_eq!(exec.attempts, 1, "endgültiges 402 wird nicht wiederholt");
        let calls = AtomicU32::new(0);
        let exec = l
            .run(&or, |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                async {
                    let mut r = AttemptFailure::from(ProviderError::Refusal);
                    r.http_status = Some(403);
                    Err::<(), _>(r)
                }
            })
            .await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "Refusal-403: keine Wiederholung"
        );
        assert_eq!(exec.error_status(), Some(&AiStatus::Refusal));
    }
    assert!(
        !l.breaker_open("openrouter"),
        "402 und Refusal-403 zählen nicht als endgültige Fehler"
    );
}

// spec: AI-017
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn breaker_can_be_switched_off_and_defaults_on() {
    assert!(
        LimitsConfig::default().circuit_breaker,
        "Schutzschalter Standard an"
    );
    let config = LimitsConfig {
        circuit_breaker: false,
        ..LimitsConfig::default()
    };
    let l = limiter_with(config, 0.5);
    let spec = RequestSpec::new("openai", "m", 1);
    for _ in 0..12 {
        always_fail(&l, &spec, 500).await;
    }
    assert!(!l.breaker_open("openai"));
}

// ---------------------------------------------------------------------------------------------------------------
// INTEG-179 / AI-017: Timeout

// spec: AI-017
// spec: INTEG-179
// spec: AI-020
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn hang_with_timeout_30s_aborts_after_30s_retries_then_timeout_status() {
    let mut config = LimitsConfig::default();
    config.providers.insert(
        "openai".into(),
        ProviderLimits {
            concurrent_requests: 4,
            timeout: Duration::from_secs(30),
        },
    );
    let l = limiter_with(config, 0.5);
    let exec = l
        .run(&RequestSpec::new("openai", "m", 10), |_| {
            std::future::pending::<Result<(), AttemptFailure>>()
        })
        .await;
    assert_eq!(
        exec.attempts, 6,
        "Timeout wird nach INTEG-005 wiederholt (1 + 5)"
    );
    // Abstand = Abbruch nach 30 s (± 2 s) + Backoff (ohne Jitter 1, 2, 4, 8, 16 s).
    for (i, gap) in gaps(&exec.attempt_starts).iter().enumerate() {
        let backoff = 2f64.powi(i as i32);
        let abort = gap - backoff;
        assert!(
            (28.0..=32.0).contains(&abort),
            "Versuch {}: Abbruch nach {abort} s statt 30 ± 2 s",
            i + 1
        );
    }
    let status = exec.error_status().unwrap();
    assert_eq!(*status, AiStatus::Timeout { seconds: 30 });
    assert_eq!(status.to_string(), "Error: Timeout nach 30 s");
    assert_eq!(exec.http_status(), None);
    // Letzter Versuch endet ebenfalls nach 30 s.
    let last = *exec.attempt_starts.last().unwrap();
    let end = exec.attempt_starts[0] + exec.duration;
    assert!(
        (28.0..=32.0).contains(&secs(end - last)),
        "{:?}",
        end - last
    );
}

// spec: INTEG-179
// acceptance: independent
#[test]
fn default_timeouts_follow_integ_179() {
    let config = LimitsConfig::default();
    assert_eq!(config.provider("openai").timeout, Duration::from_secs(120));
    assert_eq!(
        config.provider("anthropic").timeout,
        Duration::from_secs(120)
    );
    assert_eq!(config.provider("ollama").timeout, Duration::from_secs(300));
}

// ---------------------------------------------------------------------------------------------------------------
// AI-020 (Nachtrag 24.11.2026): Anbieter nicht erreichbar

/// Echte Verbindung zu einem geschlossenen Loopback-Port, in einem eigenen Thread mit eigener (nicht angehaltener)
/// Laufzeit, damit die angehaltene Testuhr den Verbindungsversuch nicht beeinflusst.
fn connect_closed_port(port: u16) -> AttemptFailure {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let err = client
                .post(format!("http://127.0.0.1:{port}/openai/v1/responses"))
                .body("{}")
                .send()
                .await
                .expect_err("Port ist geschlossen");
            AttemptFailure::from_reqwest(&err)
        })
    })
    .join()
    .unwrap()
}

// spec: AI-020
// spec: INTEG-005
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn unreachable_provider_ends_connection_failed_after_one_plus_five_attempts() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }; // Listener geschlossen → Verbindung wird abgelehnt.
    let l = limiter(0.5);
    let calls = AtomicU32::new(0);
    let exec = l
        .run(&RequestSpec::new("openai", "gpt", 10), |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            let failure = connect_closed_port(port);
            async move { Err::<(), _>(failure) }
        })
        .await;
    assert_eq!(calls.load(Ordering::SeqCst), 6, "1 + 5 Versuche");
    assert_eq!(exec.attempts, 6);
    assert_eq!(exec.retries(), 5, "Protokollspalte Retries = 5");
    let status = exec.error_status().unwrap();
    assert_eq!(*status, AiStatus::ConnectionFailed);
    assert_eq!(status.to_string(), "Error: Connection failed");
    assert_eq!(exec.error_code(), Some("transport/connect"));
    assert_eq!(exec.http_status(), None, "HTTP Status bleibt leer");
}

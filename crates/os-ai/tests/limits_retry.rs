//! Wiederholungen, Backoff, `Retry-After`, gleichzeitige Anfragen und Timeouts (AI-017 mit INTEG-005, INTEG-179).
//!
//! Die Versuche folgen den Antwortfolgen der ai-mock-Szenarien aus 08 („Konventionen“, Szenarien `429x2`,
//! `retry-after-7`, `500-always`, `529`, `hang`, `401`, `429-spend-cap`), wie `os-testsite::mocks::ai` sie liefert,
//! übersetzt in die Fehlerklassen der Adapter (AI-001). Alle Zeiten laufen auf der angehaltenen Tokio-Uhr (Testuhr):
//! kein echtes Warten.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use os_ai::limits::{
    AttemptFailure, BackoffPolicy, Execution, FixedJitter, Limiter, LimitsConfig, ProviderLimits,
    RequestKind, RequestSpec, TokioClock,
};
use os_ai::provider::ProviderError;
use os_ai::AiStatus;

const WALL: i64 = 1_795_514_400_000; // 2026-11-24T10:00:00Z

/// Antwort des ai-mock-Szenarios auf Versuch `attempt` (1 = erster Versuch).
async fn mock(scenario: &'static str, attempt: u32) -> Result<&'static str, AttemptFailure> {
    let fail = |e: ProviderError| Err(AttemptFailure::from(e));
    match scenario {
        "ok" => Ok("ok"),
        "429x2" if attempt <= 2 => fail(ProviderError::RateLimited {
            retry_after_seconds: None,
        }),
        "429x2" => Ok("ok"),
        "retry-after-7" if attempt == 1 => fail(ProviderError::RateLimited {
            retry_after_seconds: Some(7),
        }),
        "retry-after-7" => Ok("ok"),
        "500-always" => fail(ProviderError::Http {
            status: 500,
            message: "Temporary error".into(),
        }),
        "502" | "503" | "504" => fail(ProviderError::Http {
            status: scenario.parse().unwrap(),
            message: "Gateway".into(),
        }),
        "529" => fail(ProviderError::Overloaded),
        "400" | "401" | "403" | "404" => fail(ProviderError::Http {
            status: scenario.parse().unwrap(),
            message: "Fixture".into(),
        }),
        "429-spend-cap" => Err(AttemptFailure::from(ProviderError::QuotaExceeded)
            .with_error_code("enforced_spend_limit_reached")),
        "hang" => std::future::pending().await,
        other => panic!("unbekanntes Szenario {other}"),
    }
}

fn clock() -> Arc<TokioClock> {
    Arc::new(TokioClock::with_wall_origin(WALL))
}

fn limiter_with(config: LimitsConfig, jitter: f64) -> Arc<Limiter> {
    Arc::new(Limiter::with_jitter(
        config,
        clock(),
        Arc::new(FixedJitter(jitter)),
    ))
}

async fn entry(
    limiter: &Limiter,
    spec: &RequestSpec,
    scenario: &'static str,
) -> Execution<&'static str> {
    limiter.run(spec, |n| mock(scenario, n)).await
}

fn spec(provider: &str) -> RequestSpec {
    RequestSpec::new(provider, "mock-model", 100)
}

fn gaps(starts: &[Duration]) -> Vec<f64> {
    starts
        .windows(2)
        .map(|w| (w[1] - w[0]).as_secs_f64())
        .collect()
}

// spec: INTEG-005
// spec: AI-017
// `429x2` → nach zwei 429 gültige Daten, kein Fehler; 3 Requests.
#[tokio::test(start_paused = true)]
async fn two_429_then_success_ends_without_error() {
    let limiter = Limiter::new(LimitsConfig::default(), clock());
    let run = entry(&limiter, &spec("anthropic"), "429x2").await;
    assert_eq!(run.result, Ok("ok"));
    assert_eq!(run.attempts, 3);
    assert_eq!(run.retries(), 2);
    assert_eq!(run.error_status(), None);
}

// spec: INTEG-005
// Ohne `Retry-After` liegen zwischen Versuch n und n+1 (n = 1…5) jeweils 0,8·2^(n−1) bis min(60, 1,2·2^(n−1)) s;
// nach 5 Wiederholungen (6 Requests) wird der Fehler gespeichert.
#[tokio::test(start_paused = true)]
async fn backoff_gaps_stay_within_integ_005_bounds_and_six_requests_end_in_error() {
    for _ in 0..20 {
        let limiter = Limiter::new(LimitsConfig::default(), clock());
        let run = entry(&limiter, &spec("openai"), "500-always").await;
        assert_eq!(run.attempts, 6);
        assert_eq!(run.retries(), 5);
        for (i, gap) in gaps(&run.attempt_starts).into_iter().enumerate() {
            let n = i as i32 + 1;
            let low = 0.8 * 2f64.powi(n - 1);
            let high = (1.2 * 2f64.powi(n - 1)).min(60.0);
            assert!(
                gap >= low - 1e-6 && gap <= high + 1e-6,
                "Versuch {n}: {gap} s"
            );
        }
        assert_eq!(
            run.error_status(),
            Some(&AiStatus::HttpError {
                code: 500,
                message: "Temporary error".into()
            })
        );
        assert_eq!(
            run.error_status().unwrap().to_string(),
            "Error: 500 Temporary error"
        );
        assert_eq!(run.http_status(), Some(500));
    }
}

// spec: INTEG-005
// Grenzen des Jitters exakt: untere Grenze 0,8·2^(n−1), obere min(60, 1,2·2^(n−1)), Obergrenze 60 s.
#[test]
fn backoff_policy_matches_the_fixed_parameters() {
    let p = BackoffPolicy::INTEG_005;
    assert_eq!(p.base, Duration::from_secs(1));
    assert_eq!(p.cap, Duration::from_secs(60));
    for n in 1..=5u32 {
        let nominal = 2f64.powi(n as i32 - 1);
        assert!((p.delay(n, 0.0).as_secs_f64() - 0.8 * nominal).abs() < 1e-9);
        assert!((p.delay(n, 0.5).as_secs_f64() - nominal).abs() < 1e-9);
        assert!(p.delay(n, 0.999_999).as_secs_f64() <= 1.2 * nominal + 1e-9);
    }
    assert_eq!(p.delay(7, 0.5), Duration::from_secs(60));
    assert_eq!(p.delay(30, 0.999), Duration::from_secs(60));
}

// spec: INTEG-005
// spec: AI-017
// `Retry-After: 7` → mindestens 7 s bis zur nächsten Anfrage an diesen Anbieter (auch für andere Einträge); hat
// Vorrang vor dem Backoff (1 s).
#[tokio::test(start_paused = true)]
async fn retry_after_takes_precedence_and_pauses_the_whole_provider() {
    let limiter = limiter_with(LimitsConfig::default(), 0.5);
    let l = limiter.clone();
    let first = tokio::spawn(async move { entry(&l, &spec("anthropic"), "retry-after-7").await });
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let other = entry(&limiter, &spec("anthropic"), "ok").await;
    let first = first.await.unwrap();
    assert_eq!(first.result, Ok("ok"));
    let gap = gaps(&first.attempt_starts)[0];
    assert!(gap >= 7.0, "{gap}");
    assert!(other.attempt_starts[0] >= first.attempt_starts[0] + Duration::from_secs(7));
    // Ein anderer Anbieter wartet nicht.
    let unaffected = limiter_with(LimitsConfig::default(), 0.5);
    let start = unaffected.clock().now();
    let b = entry(&unaffected, &spec("openai"), "ok").await;
    assert_eq!(b.attempt_starts[0], start);
}

// spec: INTEG-005
// `Retry-After` als HTTP-Datum 7 s in der Zukunft → mindestens 7 s Wartezeit.
#[tokio::test(start_paused = true)]
async fn retry_after_http_date_waits_until_that_time() {
    let limiter = limiter_with(LimitsConfig::default(), 0.5);
    let date = "Tue, 24 Nov 2026 10:00:07 GMT";
    let wait = os_ai::limits::parse_retry_after(date, limiter.clock().wall_unix_millis()).unwrap();
    assert_eq!(wait, Duration::from_secs(7));
    let run = limiter
        .run(&spec("openai"), |n| async move {
            if n == 1 {
                Err(AttemptFailure::from(ProviderError::Http {
                    status: 429,
                    message: "Rate limited".into(),
                })
                .with_retry_after(wait))
            } else {
                Ok(())
            }
        })
        .await;
    assert!(gaps(&run.attempt_starts)[0] >= 7.0);
}

// spec: INTEG-005
// 400/401/403/404 werden nicht wiederholt (genau 1 Request); 502/503/504 und 529 (AI-002) schon.
#[tokio::test(start_paused = true)]
async fn client_errors_are_final_and_server_errors_are_retried() {
    let limiter = limiter_with(LimitsConfig::default(), 0.5);
    for code in ["400", "401", "403", "404"] {
        let run = entry(&limiter, &spec("openai"), code).await;
        assert_eq!(run.attempts, 1, "{code}");
        assert_eq!(run.http_status(), Some(code.parse().unwrap()));
        assert_eq!(
            run.error_status().unwrap().to_string(),
            format!("Error: {code} Fixture")
        );
    }
    for code in ["502", "503", "504", "529"] {
        let run = entry(&limiter, &spec("anthropic"), code).await;
        assert_eq!(run.attempts, 6, "{code}");
    }
    let overloaded = entry(&limiter, &spec("anthropic"), "529").await;
    assert_eq!(overloaded.http_status(), Some(529));
}

// spec: AI-017
// Spend-Cap-429 (`enforced_spend_limit_reached`, vom Adapter als Kontingent erkannt) wird nicht wiederholt; der
// Fehlercode steht im Protokoll.
#[tokio::test(start_paused = true)]
async fn spend_cap_is_not_retried() {
    let limiter = limiter_with(LimitsConfig::default(), 0.5);
    let run = entry(&limiter, &spec("anthropic"), "429-spend-cap").await;
    assert_eq!(run.attempts, 1);
    assert_eq!(run.error_status(), Some(&AiStatus::QuotaExceeded));
    assert_eq!(run.error_code(), Some("enforced_spend_limit_reached"));
}

// spec: INTEG-005
// spec: AI-017
// „gleichzeitige Anfragen = 2“ → nie mehr als 2 offene Anfragen dieses Anbieters; Standard je Anbieter 4, Ollama 1.
#[tokio::test(start_paused = true)]
async fn concurrent_requests_per_provider_are_capped() {
    let mut config = LimitsConfig::default();
    assert_eq!(config.provider("anthropic").concurrent_requests, 4);
    assert_eq!(config.provider("ollama").concurrent_requests, 1);
    config.providers.insert(
        "openai".into(),
        ProviderLimits {
            concurrent_requests: 2,
            ..config.provider("openai")
        },
    );
    let limiter = limiter_with(config, 0.5);
    let open = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut tasks = tokio::task::JoinSet::new();
    for i in 0..8 {
        let (l, open, peak) = (limiter.clone(), open.clone(), peak.clone());
        let provider = if i % 2 == 0 { "openai" } else { "anthropic" };
        tasks.spawn(async move {
            let clock = l.clock().clone();
            l.run(&spec(provider), |_| {
                let (open, peak, clock) = (open.clone(), peak.clone(), clock.clone());
                async move {
                    if provider == "openai" {
                        let now = open.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        clock.sleep(Duration::from_secs(10)).await;
                        open.fetch_sub(1, Ordering::SeqCst);
                    } else {
                        clock.sleep(Duration::from_secs(10)).await;
                    }
                    Ok::<_, AttemptFailure>(())
                }
            })
            .await
        });
    }
    while let Some(done) = tasks.join_next().await {
        assert!(done.unwrap().result.is_ok());
    }
    assert_eq!(peak.load(Ordering::SeqCst), 2);
}

// spec: AI-017
// spec: INTEG-179
// Szenario `hang` mit Timeout 30 s → jeder Versuch nach 30 s (± 2 s) abgebrochen, Wiederholung nach INTEG-005,
// danach `Error: Timeout nach 30 s`.
#[tokio::test(start_paused = true)]
async fn hang_times_out_after_30_seconds_with_retries() {
    let mut config = LimitsConfig::default();
    config.default_provider.timeout = Duration::from_secs(30);
    let limiter = limiter_with(config, 0.5);
    let run = entry(&limiter, &spec("openai"), "hang").await;
    assert_eq!(run.attempts, 6);
    let expected_gaps = [1.0, 2.0, 4.0, 8.0, 16.0];
    for (gap, backoff) in gaps(&run.attempt_starts).iter().zip(expected_gaps) {
        let timeout = gap - backoff;
        assert!((timeout - 30.0).abs() <= 2.0, "Abbruch nach {timeout} s");
    }
    assert_eq!(run.error_status(), Some(&AiStatus::Timeout { seconds: 30 }));
    assert_eq!(
        run.error_status().unwrap().to_string(),
        "Error: Timeout nach 30 s"
    );
    assert_eq!(run.http_status(), None);
    assert!(run.error_status().unwrap().is_error());
}

// spec: INTEG-179
// Standard 120 s (Ollama 300 s); der Timeout eines Anbieters beeinflusst andere nicht: OpenAI mit 100 s Antwortzeit
// gelingt bei 120 s, auch wenn Ollama auf 30 s steht.
#[tokio::test(start_paused = true)]
async fn timeouts_are_per_provider() {
    let mut config = LimitsConfig::default();
    assert_eq!(config.provider("openai").timeout, Duration::from_secs(120));
    assert_eq!(
        config.provider("anthropic").timeout,
        Duration::from_secs(120)
    );
    assert_eq!(config.provider("ollama").timeout, Duration::from_secs(300));
    config.providers.insert(
        "ollama".into(),
        ProviderLimits {
            timeout: Duration::from_secs(30),
            ..config.provider("ollama")
        },
    );
    let limiter = limiter_with(config, 0.5);
    let clock = limiter.clock().clone();
    let l = limiter.clone();
    let ollama = tokio::spawn(async move { entry(&l, &spec("ollama"), "hang").await });
    let openai = limiter
        .run(&spec("openai"), |_| {
            let clock = clock.clone();
            async move {
                clock.sleep(Duration::from_secs(100)).await;
                Ok::<_, AttemptFailure>("slow but fine")
            }
        })
        .await;
    assert_eq!(openai.result, Ok("slow but fine"));
    assert_eq!(openai.attempts, 1);
    assert_eq!(
        ollama.await.unwrap().error_status(),
        Some(&AiStatus::Timeout { seconds: 30 })
    );
}

// spec: INTEG-179
// Bildgenerierung: nach einem Timeout genau 1 Wiederholung, danach Fehlerstatus.
#[tokio::test(start_paused = true)]
async fn image_generation_retries_a_timeout_only_once() {
    let mut config = LimitsConfig::default();
    config.default_provider.timeout = Duration::from_secs(60);
    let limiter = limiter_with(config, 0.5);
    let spec = RequestSpec {
        kind: RequestKind::ImageGeneration,
        ..spec("openai")
    };
    let run = entry(&limiter, &spec, "hang").await;
    assert_eq!(run.attempts, 2);
    assert_eq!(run.error_status(), Some(&AiStatus::Timeout { seconds: 60 }));
}

// spec: AI-017
// Ein Request mit mehreren Decision-Fragen ist ein Eintrag mit einem gemeinsamen Timeout: alle Fragen dieses Requests
// erhalten denselben Endstatus (Verteilung auf die Fragen: M4-D01, siehe pending_m4_k06).
#[tokio::test(start_paused = true)]
async fn one_timeout_covers_every_question_of_a_request() {
    let mut config = LimitsConfig::default();
    config.default_provider.timeout = Duration::from_secs(30);
    let limiter = limiter_with(config, 0.5);
    let questions = ["page-type", "search-intent", "soft-404"];
    let run = entry(&limiter, &spec("openai"), "hang").await;
    let statuses: Vec<(&str, AiStatus)> = questions
        .iter()
        .map(|q| (*q, run.error_status().cloned().unwrap()))
        .collect();
    assert_eq!(run.attempts, 6, "ein Request je Versuch für alle Fragen");
    assert!(statuses
        .iter()
        .all(|(_, s)| *s == AiStatus::Timeout { seconds: 30 }));
}

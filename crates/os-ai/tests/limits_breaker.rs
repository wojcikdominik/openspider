//! Schutzschalter je Anbieter (AI-017 Punkt 3) und Transportfehler als `Error: Connection failed` (AI-020, K4-15).
//! Testuhr: angehaltene Tokio-Uhr; Antwortfolgen wie die ai-mock-Szenarien `500-always`, `429-spend-cap`,
//! `402-credits`, `refusal-403`.
use std::sync::Arc;
use std::time::Duration;

use os_ai::limits::{
    AttemptFailure, BreakerEffect, Execution, FixedJitter, Limiter, LimitsConfig, PauseReason,
    RequestSpec, TokioClock, TransportCause,
};
use os_ai::provider::ProviderError;
use os_ai::AiStatus;

fn new_limiter(config: LimitsConfig) -> Arc<Limiter> {
    Arc::new(Limiter::with_jitter(
        config,
        Arc::new(TokioClock::with_wall_origin(1_795_514_400_000)),
        Arc::new(FixedJitter(0.5)),
    ))
}

async fn failing(limiter: &Limiter, provider: &str, error: AttemptFailure) -> Execution<()> {
    limiter
        .run(&RequestSpec::new(provider, "m", 100), |_| {
            let error = error.clone();
            async move { Err::<(), _>(error) }
        })
        .await
}

async fn ok(limiter: &Limiter, provider: &str) -> Execution<()> {
    limiter
        .run(&RequestSpec::new(provider, "m", 100), |_| async {
            Ok::<_, AttemptFailure>(())
        })
        .await
}

fn http500() -> AttemptFailure {
    ProviderError::Http {
        status: 500,
        message: "Temporary error".into(),
    }
    .into()
}

// spec: AI-017
// `500-always` für Anbieter A → nach 10 endgültigen Fehlern sendet A 5 Minuten lang nichts; Anbieter B läuft
// unbeeinflusst weiter.
#[tokio::test(start_paused = true)]
async fn ten_final_failures_pause_only_that_provider_for_five_minutes() {
    let limiter = new_limiter(LimitsConfig::default());
    let mut a_starts = Vec::new();
    for _ in 0..10 {
        let run = failing(&limiter, "openai", http500()).await;
        assert_eq!(run.attempts, 6);
        a_starts.extend(run.attempt_starts);
    }
    let opened = limiter.clock().now();
    assert!(limiter.breaker_open("openai"));
    assert!(!limiter.breaker_open("anthropic"));
    let pauses = limiter.pauses(0);
    assert!(pauses
        .iter()
        .any(|p| p.provider == "openai" && p.reason == PauseReason::CircuitBreaker));
    let l = limiter.clone();
    let eleventh = tokio::spawn(async move { ok(&l, "openai").await });
    tokio::task::yield_now().await;
    // B läuft sofort und mehrfach während der Pause.
    for _ in 0..3 {
        let b = ok(&limiter, "anthropic").await;
        assert!(b.result.is_ok());
        assert_eq!(b.queued, Duration::ZERO);
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    let eleventh = eleventh.await.unwrap();
    assert!(eleventh.result.is_ok());
    let first_after = eleventh.attempt_starts[0];
    assert!(
        first_after >= opened + Duration::from_secs(300),
        "{first_after:?}"
    );
    assert!(first_after <= opened + Duration::from_secs(301));
    assert_eq!(a_starts.len(), 60);
    assert!(a_starts.iter().all(|s| *s <= opened));
}

// spec: AI-017
// Spend-Cap-429 und endgültiges OpenRouter-402 (beide `Quota exceeded`) sowie OpenRouter-403 `refusal` zählen nicht
// als aufeinanderfolgende endgültige Fehler.
#[tokio::test(start_paused = true)]
async fn quota_and_refusal_do_not_trip_the_breaker() {
    let limiter = new_limiter(LimitsConfig::default());
    let spend_cap = AttemptFailure::from(ProviderError::QuotaExceeded)
        .with_error_code("enforced_spend_limit_reached");
    let credits =
        AttemptFailure::from(ProviderError::QuotaExceeded).with_error_code("openrouter_credits");
    for _ in 0..12 {
        let run = failing(&limiter, "anthropic", spend_cap.clone()).await;
        assert_eq!(
            (run.attempts, run.error_status()),
            (1, Some(&AiStatus::QuotaExceeded))
        );
        let run = failing(&limiter, "openrouter", credits.clone()).await;
        assert_eq!(run.attempts, 1);
        let run = failing(&limiter, "openrouter", ProviderError::Refusal.into()).await;
        assert_eq!(
            (run.attempts, run.error_status()),
            (1, Some(&AiStatus::Refusal))
        );
    }
    assert!(!limiter.breaker_open("anthropic"));
    assert!(!limiter.breaker_open("openrouter"));
    assert_eq!(
        BreakerEffect::of(&ProviderError::QuotaExceeded),
        BreakerEffect::Neutral
    );
    assert_eq!(
        BreakerEffect::of(&ProviderError::Refusal),
        BreakerEffect::Reset
    );
    // Neutrale Fälle unterbrechen die Folge nicht, ein Refusal setzt sie zurück.
    for _ in 0..9 {
        failing(&limiter, "openai", http500()).await;
    }
    failing(&limiter, "openai", ProviderError::QuotaExceeded.into()).await;
    assert!(!limiter.breaker_open("openai"));
    failing(&limiter, "openai", ProviderError::Refusal.into()).await;
    for _ in 0..9 {
        failing(&limiter, "openai", http500()).await;
    }
    assert!(!limiter.breaker_open("openai"), "nach Refusal neu gezählt");
    failing(&limiter, "openai", http500()).await;
    assert!(limiter.breaker_open("openai"));
}

// spec: AI-017
// Ein Erfolg setzt die Zählung zurück; Schutzschalter aus → keine Pause.
#[tokio::test(start_paused = true)]
async fn success_resets_and_switch_off_disables_the_breaker() {
    let limiter = new_limiter(LimitsConfig::default());
    for _ in 0..9 {
        failing(&limiter, "openai", http500()).await;
    }
    ok(&limiter, "openai").await;
    for _ in 0..9 {
        failing(&limiter, "openai", http500()).await;
    }
    assert!(!limiter.breaker_open("openai"));
    let off = new_limiter(LimitsConfig {
        circuit_breaker: false,
        ..LimitsConfig::default()
    });
    for _ in 0..12 {
        failing(
            &off,
            "openai",
            ProviderError::Http {
                status: 404,
                message: "x".into(),
            }
            .into(),
        )
        .await;
    }
    assert!(!off.breaker_open("openai"));
    let start = off.clock().now();
    assert_eq!(ok(&off, "openai").await.attempt_starts[0], start);
}

// spec: AI-020
// spec: AI-017
// K4-15: Transportfehler ohne HTTP-Antwort werden nach INTEG-005 wiederholt (1 + 5 Versuche), enden als
// `Error: Connection failed` mit `HTTP Status` leer und `Error Code = transport/<art>` und zählen für den
// Schutzschalter.
#[tokio::test(start_paused = true)]
async fn transport_errors_retry_end_as_connection_failed_and_count_for_the_breaker() {
    let limiter = new_limiter(LimitsConfig::default());
    for (cause, code) in [
        (TransportCause::Dns, "transport/dns"),
        (TransportCause::Connect, "transport/connect"),
        (TransportCause::Tls, "transport/tls"),
        (TransportCause::Closed, "transport/closed"),
    ] {
        let run = failing(
            &limiter,
            "openai",
            AttemptFailure::transport(cause, "fixture"),
        )
        .await;
        assert_eq!(run.attempts, 6);
        assert_eq!(run.retries(), 5);
        assert_eq!(run.error_status(), Some(&AiStatus::ConnectionFailed));
        assert_eq!(
            run.error_status().unwrap().to_string(),
            "Error: Connection failed"
        );
        assert!(run.error_status().unwrap().is_error());
        assert_eq!(run.http_status(), None);
        assert_eq!(run.error_code(), Some(code));
    }
    for _ in 0..6 {
        failing(
            &limiter,
            "openai",
            ProviderError::Transport {
                message: "tcp connect error: Connection refused".into(),
            }
            .into(),
        )
        .await;
    }
    assert!(
        limiter.breaker_open("openai"),
        "4 + 6 = 10 endgültige Transportfehler"
    );
}

// spec: AI-020
// Zuordnung von Meldungen ohne strukturierte Ursache.
#[test]
fn transport_messages_are_classified() {
    for (message, cause) in [
        (
            "error trying to connect: dns error: failed to lookup address",
            TransportCause::Dns,
        ),
        (
            "client error (Connect): tcp connect error: Connection refused (os error 61)",
            TransportCause::Connect,
        ),
        (
            "invalid peer certificate: UnknownIssuer",
            TransportCause::Tls,
        ),
        (
            "connection closed before message completed",
            TransportCause::Closed,
        ),
        ("operation was canceled", TransportCause::Closed),
    ] {
        assert_eq!(
            TransportCause::classify_message(message),
            cause,
            "{message}"
        );
    }
}

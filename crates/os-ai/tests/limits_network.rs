//! „Anbieter nicht erreichbar“ über echtes HTTP (AI-020, Nachtrag K4-15): Verbindung zu einem geschlossenen
//! Loopback-Port wird abgelehnt, keine HTTP-Antwort → nach 1 + 5 Versuchen `Error: Connection failed`,
//! `HTTP Status` leer, `Error Code = transport/connect`, `Retries = 5`.
//!
//! ai-mock hat für diesen Fall kein Szenario (es antwortet immer); deshalb ein geschlossener Port auf 127.0.0.1. Die
//! Backoff-Wartezeiten sind für diesen Test auf Millisekunden verkürzt (Basis 1 ms), damit kein echtes Warten im
//! Sekundenbereich entsteht; Regeln und Versuchszahl bleiben die aus INTEG-005.
use std::sync::Arc;
use std::time::Duration;

use os_ai::limits::{
    AttemptFailure, BackoffPolicy, FixedJitter, Limiter, LimitsConfig, RequestSpec, TokioClock,
    TransportCause,
};
use os_ai::AiStatus;

async fn closed_loopback_port() -> u16 {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

// spec: AI-020
// spec: AI-017
#[tokio::test]
async fn refused_connection_ends_as_connection_failed_after_six_attempts() {
    let port = closed_loopback_port().await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let url = format!("http://127.0.0.1:{port}/anthropic/v1/messages");
    let config = LimitsConfig {
        backoff: BackoffPolicy {
            base: Duration::from_millis(1),
            ..BackoffPolicy::INTEG_005
        },
        ..LimitsConfig::default()
    };
    let limiter = Limiter::with_jitter(
        config,
        Arc::new(TokioClock::new()),
        Arc::new(FixedJitter(0.5)),
    );
    let run = limiter
        .run(&RequestSpec::new("anthropic", "mock-claude", 100), |_| {
            let (client, url) = (client.clone(), url.clone());
            async move {
                match client.post(&url).body("{}").send().await {
                    Ok(response) => Ok(response.status().as_u16()),
                    Err(error) => {
                        assert_eq!(
                            TransportCause::from_reqwest(&error),
                            Some(TransportCause::Connect),
                            "{error:?}"
                        );
                        Err(AttemptFailure::from_reqwest(&error))
                    }
                }
            }
        })
        .await;
    assert_eq!(run.attempts, 6);
    assert_eq!(run.retries(), 5);
    assert_eq!(run.error_status(), Some(&AiStatus::ConnectionFailed));
    assert_eq!(run.http_status(), None);
    assert_eq!(run.error_code(), Some("transport/connect"));
}

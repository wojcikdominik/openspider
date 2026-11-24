//! Injizierbare Uhr für Ratenbegrenzung, Backoff und Timeouts (08 Konventionen „Testuhr“).
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// Zeitquelle der Ratenbegrenzung. `now()` ist monoton seit einem festen Ursprung, `wall_unix_millis()` die dazu
/// passende Wanduhrzeit (für Reset-Zeitpunkte der Anbieter und den Hinweis „pausiert bis <Uhrzeit>“).
pub trait LimitClock: Send + Sync {
    /// Monotone Zeit seit dem Ursprung der Uhr.
    fn now(&self) -> Duration;
    /// Wanduhrzeit in Unix-Millisekunden, konsistent mit [`LimitClock::now`].
    fn wall_unix_millis(&self) -> i64;
    /// Wartet `duration` auf dieser Uhr.
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

/// Uhr auf Basis der Tokio-Zeit. In Tests mit angehaltener Tokio-Uhr (`start_paused`) ist sie die Testuhr: Wartezeiten
/// vergehen virtuell, ohne echtes Warten.
#[derive(Debug, Clone)]
pub struct TokioClock {
    origin: tokio::time::Instant,
    wall_origin_ms: i64,
}

impl TokioClock {
    /// Ursprung jetzt; Wanduhrzeit aus der Systemuhr.
    pub fn new() -> Self {
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Self::with_wall_origin(wall)
    }

    /// Ursprung jetzt mit fest vorgegebener Wanduhrzeit (deterministische Tests).
    pub fn with_wall_origin(wall_origin_ms: i64) -> Self {
        Self {
            origin: tokio::time::Instant::now(),
            wall_origin_ms,
        }
    }
}

impl Default for TokioClock {
    fn default() -> Self {
        Self::new()
    }
}

impl LimitClock for TokioClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }

    fn wall_unix_millis(&self) -> i64 {
        let elapsed = i64::try_from(self.now().as_millis()).unwrap_or(i64::MAX);
        self.wall_origin_ms.saturating_add(elapsed)
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(tokio::time::sleep(duration))
    }
}

//! Backoff nach INTEG-005 und Auswertung von `Retry-After`.
use std::time::Duration;

/// Feste Backoff-Parameter (INTEG-005: „fest, nicht in der UI“).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackoffPolicy {
    /// Wartezeit vor der ersten Wiederholung ohne Jitter.
    pub base: Duration,
    /// Faktor je weiterer Wiederholung.
    pub factor: f64,
    /// Obergrenze.
    pub cap: Duration,
    /// Relativer Jitter (0,2 = ±20 %).
    pub jitter: f64,
}

impl BackoffPolicy {
    /// INTEG-005: Basis 1 s, Faktor 2, Obergrenze 60 s, Jitter ±20 %.
    pub const INTEG_005: Self = Self {
        base: Duration::from_secs(1),
        factor: 2.0,
        cap: Duration::from_secs(60),
        jitter: 0.2,
    };

    /// Wartezeit vor Wiederholung `retry` (1 = erste Wiederholung, also zwischen Versuch 1 und 2). `unit` ist eine
    /// Zufallszahl aus [0, 1) und wählt den Jitter gleichverteilt aus [1 − jitter, 1 + jitter].
    /// Ergebnis: min(cap, base · factor^(retry−1) · jitter).
    pub fn delay(&self, retry: u32, unit: f64) -> Duration {
        let exponent = i32::try_from(retry.saturating_sub(1)).unwrap_or(i32::MAX);
        let raw = self.base.as_secs_f64() * self.factor.powi(exponent);
        let unit = unit.clamp(0.0, 1.0);
        let jitter = 1.0 - self.jitter + 2.0 * self.jitter * unit;
        let secs = (raw * jitter).min(self.cap.as_secs_f64());
        if secs.is_finite() && secs > 0.0 {
            Duration::from_secs_f64(secs)
        } else {
            Duration::ZERO
        }
    }
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self::INTEG_005
    }
}

/// Quelle des Jitters, injizierbar für deterministische Tests.
pub trait JitterSource: Send + Sync {
    /// Zahl aus [0, 1).
    fn unit(&self) -> f64;
}

/// Zufälliger Jitter (Standard).
#[derive(Debug, Clone, Copy, Default)]
pub struct RandomJitter;

impl JitterSource for RandomJitter {
    fn unit(&self) -> f64 {
        rand::random::<f64>()
    }
}

/// Fester Jitter, z. B. 0,0 (untere Grenze), 0,5 (ohne Jitter) oder knapp unter 1 (obere Grenze).
#[derive(Debug, Clone, Copy)]
pub struct FixedJitter(pub f64);

impl JitterSource for FixedJitter {
    fn unit(&self) -> f64 {
        self.0
    }
}

/// Liest `Retry-After`: Sekunden (`7`) oder HTTP-Datum (`Wed, 21 Oct 2026 07:28:07 GMT`). Ein Datum in der
/// Vergangenheit ergibt 0. `now_unix_millis` ist die Wanduhrzeit der Antwort.
pub fn parse_retry_after(value: &str, now_unix_millis: i64) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    if let Ok(seconds) = value.parse::<f64>() {
        if seconds.is_finite() && seconds >= 0.0 {
            return Some(Duration::from_secs_f64(seconds));
        }
        return None;
    }
    let at = parse_http_date(value)?;
    let diff = at.saturating_mul(1000).saturating_sub(now_unix_millis);
    Some(Duration::from_millis(u64::try_from(diff).unwrap_or(0)))
}

/// HTTP-Datum im Format IMF-fixdate (RFC 9110, 5.6.7) als Unix-Sekunden.
pub fn parse_http_date(value: &str) -> Option<i64> {
    // "Sun, 06 Nov 1994 08:49:37 GMT"
    let rest = value.trim().split_once(", ")?.1;
    let mut parts = rest.split(' ');
    let day: u32 = parts.next()?.parse().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: i64 = parts.next()?.parse().ok()?;
    let mut hms = parts.next()?.split(':');
    let hour: i64 = hms.next()?.parse().ok()?;
    let minute: i64 = hms.next()?.parse().ok()?;
    let second: i64 = hms.next()?.parse().ok()?;
    if parts.next()? != "GMT" || parts.next().is_some() || hms.next().is_some() {
        return None;
    }
    if !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Tage seit 1970-01-01 für ein Datum im gregorianischen Kalender (H. Hinnant, „days_from_civil“).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: INTEG-005
    #[test]
    fn http_dates_and_seconds_parse() {
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(
            parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(784_111_777)
        );
        assert_eq!(parse_http_date("Sun, 06 Nov 1994 08:49:37 CET"), None);
        assert_eq!(parse_retry_after("7", 0), Some(Duration::from_secs(7)));
        assert_eq!(
            parse_retry_after("Sun, 06 Nov 1994 08:49:44 GMT", 784_111_777_000),
            Some(Duration::from_secs(7))
        );
        assert_eq!(
            parse_retry_after("Sun, 06 Nov 1994 08:49:30 GMT", 784_111_777_000),
            Some(Duration::ZERO)
        );
        assert_eq!(parse_retry_after("soon", 0), None);
    }
}

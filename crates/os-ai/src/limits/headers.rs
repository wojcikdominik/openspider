//! Rate-Limit-Header der Anbieter (AI-017 Punkt 2).
//!
//! Anthropic: `anthropic-ratelimit-requests-remaining`/`-reset` und `anthropic-ratelimit-input-tokens-remaining`/`-reset`
//! (Reset als RFC-3339-Zeitpunkt). Der kombinierte Header `anthropic-ratelimit-tokens-remaining` wird bewusst nicht
//! ausgewertet, weil er Ein- und Ausgabe zusammenfasst. Token-Restwerte rundet Anthropic auf Tausender; ein gemeldeter
//! Wert r gilt als Intervall [r − 500, r + 499] (OS-Festlegung).
//!
//! OpenAI: `x-ratelimit-remaining-requests`/`x-ratelimit-reset-requests` und
//! `x-ratelimit-remaining-tokens`/`x-ratelimit-reset-tokens` (Reset als Dauer, z. B. `1s`, `6m0s`, `120ms`).
use std::time::Duration;

/// Wie genau der gemeldete Token-Restwert ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenRounding {
    /// Exakter Wert (OpenAI).
    Exact,
    /// Auf Tausender gerundet (Anthropic): r steht für [r − 500, r + 499].
    Thousands,
}

impl TokenRounding {
    /// Größter Wert, für den der gemeldete Rest stehen kann.
    pub fn upper_bound(self, reported: u64) -> u64 {
        match self {
            Self::Exact => reported,
            Self::Thousands => reported.saturating_add(499),
        }
    }
}

/// Ausgewertete Rate-Limit-Angaben einer Antwort. Reset-Angaben sind relativ zur Antwort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitSnapshot {
    pub requests_remaining: Option<u64>,
    pub requests_reset_in: Option<Duration>,
    pub input_tokens_remaining: Option<u64>,
    pub input_tokens_reset_in: Option<Duration>,
    pub rounding: TokenRounding,
}

impl RateLimitSnapshot {
    /// `true`, wenn die Antwort keine auswertbare Angabe trug.
    pub fn is_empty(&self) -> bool {
        self.requests_remaining.is_none() && self.input_tokens_remaining.is_none()
    }
}

fn number(value: Option<String>) -> Option<u64> {
    value.and_then(|v| v.trim().parse::<u64>().ok())
}

/// Anthropic-Header; `get` liefert den Wert eines Headers (Groß-/Kleinschreibung egal), `now_unix_millis` die
/// Wanduhrzeit der Antwort.
pub fn parse_anthropic(
    get: impl Fn(&str) -> Option<String>,
    now_unix_millis: i64,
) -> RateLimitSnapshot {
    let reset = |name: &str| {
        let at = os_model::Timestamp::from_db(get(name)?.trim()).ok()?;
        let diff = at.unix_millis().saturating_sub(now_unix_millis);
        Some(Duration::from_millis(u64::try_from(diff).unwrap_or(0)))
    };
    RateLimitSnapshot {
        requests_remaining: number(get("anthropic-ratelimit-requests-remaining")),
        requests_reset_in: reset("anthropic-ratelimit-requests-reset"),
        input_tokens_remaining: number(get("anthropic-ratelimit-input-tokens-remaining")),
        input_tokens_reset_in: reset("anthropic-ratelimit-input-tokens-reset"),
        rounding: TokenRounding::Thousands,
    }
}

/// OpenAI-Header.
pub fn parse_openai(get: impl Fn(&str) -> Option<String>) -> RateLimitSnapshot {
    let reset = |name: &str| parse_go_duration(get(name)?.trim());
    RateLimitSnapshot {
        requests_remaining: number(get("x-ratelimit-remaining-requests")),
        requests_reset_in: reset("x-ratelimit-reset-requests"),
        input_tokens_remaining: number(get("x-ratelimit-remaining-tokens")),
        input_tokens_reset_in: reset("x-ratelimit-reset-tokens"),
        rounding: TokenRounding::Exact,
    }
}

/// Wertet die Header einer Antwort für `provider` (`anthropic`, `openai`) aus; andere Anbieter liefern `None`.
pub fn from_header_map(
    provider: &str,
    headers: &reqwest::header::HeaderMap,
    now_unix_millis: i64,
) -> Option<RateLimitSnapshot> {
    let get = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let snapshot = match provider {
        "anthropic" => parse_anthropic(get, now_unix_millis),
        "openai" => parse_openai(get),
        _ => return None,
    };
    (!snapshot.is_empty()).then_some(snapshot)
}

/// Dauer im Format `1h2m3.5s`, `6m0s`, `120ms`, `20s`.
pub fn parse_go_duration(text: &str) -> Option<Duration> {
    if text.is_empty() {
        return None;
    }
    let mut total_nanos = 0f64;
    let mut rest = text;
    while !rest.is_empty() {
        let digits = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        if digits == 0 {
            return None;
        }
        let value: f64 = rest[..digits].parse().ok()?;
        rest = &rest[digits..];
        let (factor, len) = if rest.starts_with("ms") {
            (1e6, 2)
        } else if rest.starts_with("us") || rest.starts_with("µs") {
            (1e3, rest.chars().take(2).map(char::len_utf8).sum())
        } else if rest.starts_with("ns") {
            (1.0, 2)
        } else if rest.starts_with('h') {
            (3.6e12, 1)
        } else if rest.starts_with('m') {
            (6e10, 1)
        } else if rest.starts_with('s') {
            (1e9, 1)
        } else {
            return None;
        };
        total_nanos += (value * factor).round();
        rest = &rest[len..];
    }
    (total_nanos.is_finite() && total_nanos >= 0.0 && total_nanos < u64::MAX as f64)
        .then(|| Duration::from_nanos(total_nanos as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-017
    #[test]
    fn go_durations_parse() {
        assert_eq!(parse_go_duration("1s"), Some(Duration::from_secs(1)));
        assert_eq!(parse_go_duration("6m0s"), Some(Duration::from_secs(360)));
        assert_eq!(parse_go_duration("120ms"), Some(Duration::from_millis(120)));
        assert_eq!(
            parse_go_duration("1h2m3.5s"),
            Some(Duration::from_millis(3_723_500))
        );
        assert_eq!(parse_go_duration("abc"), None);
        assert_eq!(parse_go_duration("5x"), None);
    }
}

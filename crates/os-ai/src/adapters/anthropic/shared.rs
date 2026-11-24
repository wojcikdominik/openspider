//! Gemeinsame Bausteine der HTTP-Adapter (M4-K01: Anthropic, OpenRouter): geheimer Schlüssel, Antwort eines Versuchs,
//! Kontingent-Sperre je Lauf, Bildkodierung und Hilfen für Fehlerantworten.
//!
//! Liegt unter `adapters::anthropic`, weil `adapters/mod.rs` nicht zum Paket K01 gehört; die OpenRouter-Anbindung
//! nutzt das Modul über diesen Pfad. Ein späteres Paket kann es nach `adapters::shared` verschieben (Hinweis im
//! Bericht K01).
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::header::HeaderMap;
use serde_json::Value;

use crate::limits::{parse_retry_after, AttemptFailure, Limiter};
use crate::provider::{Input, ProviderError, Response, Usage};
use crate::schema::{RawAnswer, ResponseSchema};

/// API-Schlüssel eines Anbieters. `Debug` und `Display` zeigen ihn nie (AI-002: der Schlüssel erscheint in keinem
/// Log von OpenSpider).
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// Übernimmt den Schlüssel (aus dem Schlüsselbund, AI-009).
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Der Schlüssel für den Request-Header.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(***)")
    }
}

impl fmt::Display for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// Antwort eines einzelnen Versuchs (ein HTTP-Request) im gemeinsamen Format (AI-001) mit den Angaben, die das
/// gemeinsame [`Response`] nicht trägt (Rohantwort für die Schema-Validierung, Refusal-Grund, Stoppgrund).
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterReply {
    /// Rohantwort für [`crate::schema::run_with_repair`]: bei Schema-Aufgaben `Json` bzw. `Unparsable`, sonst `Text`;
    /// `Refusal` ohne Parse-Versuch.
    pub raw: RawAnswer,
    /// Einheitliche Tokenzahlen; fehlende Felder des Anbieters bleiben `None` (AI-001).
    pub usage: Usage,
    /// Spalte `<Q> Refusal Reason` (AI-038): Kategorie bzw. Text des Anbieters.
    pub refusal_reason: Option<String>,
    /// Stoppgrund des Anbieters (`stop_reason` bzw. `finish_reason`).
    pub stop_reason: Option<String>,
    /// Anbieter-Request-ID (Antwort-ID bzw. Header).
    pub provider_request_id: Option<String>,
    /// HTTP-Status der Antwort.
    pub http_status: u16,
    /// Laufzeit des Requests.
    pub duration_ms: u64,
    /// Ob die Aufgabe ein Antwortschema hatte.
    pub structured: bool,
}

impl AdapterReply {
    /// Ob die Antwort eine Verweigerung ist.
    pub fn is_refusal(&self) -> bool {
        matches!(self.raw, RawAnswer::Refusal)
    }

    /// Gemeinsames Ergebnis (AI-001). Mit `schema` wird die Antwort lokal validiert (AI-021, ohne Reparatur);
    /// nicht lesbare oder schemawidrige Antworten ergeben `Error: Invalid output`.
    pub fn into_response(self, schema: Option<&Value>) -> Result<Response, ProviderError> {
        let content = match self.raw {
            RawAnswer::Refusal => None,
            RawAnswer::Json(value) => Some(value),
            RawAnswer::Text(text) if self.structured => Some(
                serde_json::from_str::<Value>(text.trim())
                    .map_err(|_| ProviderError::InvalidOutput)?,
            ),
            RawAnswer::Text(text) => Some(Value::String(crate::schema::normalize_text(&text))),
            RawAnswer::Unparsable { .. } => return Err(ProviderError::InvalidOutput),
        };
        let content = match (content, schema) {
            (Some(value), Some(schema)) => {
                let value = crate::schema::normalize_value(value);
                let checker = ResponseSchema::new(schema.clone())
                    .map_err(|_| ProviderError::InvalidOutput)?;
                checker
                    .validate(&value)
                    .map_err(|_| ProviderError::InvalidOutput)?;
                Some(value)
            }
            (content, _) => content,
        };
        Ok(Response {
            refusal: content.is_none(),
            content,
            usage: self.usage,
            provider_request_id: self.provider_request_id,
            duration_ms: self.duration_ms,
        })
    }
}

/// Grund, aus dem ein Anbieter für den Rest des Laufs gesperrt ist (endgültige Kontingent- oder Ausgabengrenze).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaStop {
    /// HTTP-Status der auslösenden Antwort.
    pub http_status: u16,
    /// Fehlercode für die Protokollspalte `Error Code` (z. B. `enforced_spend_limit_reached`, `openrouter_credits`).
    pub error_code: Option<String>,
    /// Meldung des Anbieters.
    pub message: String,
}

/// Kontingent-Sperre eines Adapters für einen Lauf (AI-002, AI-005, AI-020): Nach einer endgültigen Kontingentantwort
/// erhalten alle weiteren Einträge dieses Anbieters `Quota exceeded`, ohne dass ein weiterer Request hinausgeht. Bei n
/// gleichzeitigen Anfragen können höchstens die n bereits gesendeten Requests noch ankommen.
#[derive(Debug, Default, Clone)]
pub struct QuotaLatch(Arc<Mutex<Option<QuotaStop>>>);

impl QuotaLatch {
    /// Sperrt den Anbieter; ein früherer Grund bleibt erhalten.
    pub fn trip(&self, stop: QuotaStop) {
        let mut guard = self.0.lock().unwrap_or_else(|p| p.into_inner());
        guard.get_or_insert(stop);
    }

    /// Der Sperrgrund, falls gesperrt.
    pub fn stop(&self) -> Option<QuotaStop> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Ob gesperrt.
    pub fn is_tripped(&self) -> bool {
        self.stop().is_some()
    }

    /// Hebt die Sperre auf (neuer Lauf, z. B. nach Aufladen des Guthabens).
    pub fn reset(&self) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    /// Fehler eines nicht gesendeten Eintrags: `Quota exceeded` ohne HTTP-Status (es ging kein Request hinaus).
    pub(crate) fn blocked_failure(&self) -> Option<AttemptFailure> {
        self.stop().map(|stop| {
            let failure = AttemptFailure::from(ProviderError::QuotaExceeded);
            match stop.error_code {
                Some(code) => failure.with_error_code(code),
                None => failure,
            }
        })
    }
}

/// Endgültiger Kontingentfehler einer Antwort: Fehlerklasse `Quota exceeded` mit HTTP-Status und Fehlercode.
pub(crate) fn quota_failure(http_status: u16, error_code: Option<String>) -> AttemptFailure {
    let mut failure = AttemptFailure::from(ProviderError::QuotaExceeded);
    failure.http_status = Some(http_status);
    match error_code {
        Some(code) => failure.with_error_code(code),
        None => failure,
    }
}

/// Fehler „vorübergehend gedrosselt“ mit optionalem `Retry-After` (INTEG-005).
pub(crate) fn rate_limited(retry_after: Option<Duration>, http_status: u16) -> AttemptFailure {
    let mut failure = AttemptFailure::from(ProviderError::RateLimited {
        retry_after_seconds: retry_after.map(|d| d.as_secs() + u64::from(d.subsec_nanos() > 0)),
    });
    failure.retry_after = retry_after;
    failure.http_status = Some(http_status);
    failure
}

/// Sonstiger HTTP-Fehler (`Error: <HTTP-Code> <Meldung>`); 429/5xx werden vom Limiter wiederholt.
pub(crate) fn http_failure(
    http_status: u16,
    message: String,
    retry_after: Option<Duration>,
) -> AttemptFailure {
    let mut failure = AttemptFailure::from(ProviderError::Http {
        status: http_status,
        message,
    });
    failure.retry_after = retry_after;
    failure
}

/// `Retry-After` einer Antwort (Sekunden oder HTTP-Datum).
pub(crate) fn retry_after(headers: &HeaderMap, now_unix_millis: i64) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_retry_after(v, now_unix_millis))
}

/// Wanduhrzeit für `Retry-After`: die Uhr des Limiters, sonst die Systemuhr.
pub(crate) fn wall_millis(limiter: Option<&Arc<Limiter>>) -> i64 {
    match limiter {
        Some(limiter) => limiter.clock().wall_unix_millis(),
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0),
    }
}

/// Meldung eines Fehlerkörpers (`error.message`), sonst der kanonische Grund des Status.
pub(crate) fn error_message(body: &Value, status: reqwest::StatusCode) -> String {
    body.pointer("/error/message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| status.canonical_reason().unwrap_or("").to_string())
}

/// Liest eine Zahl aus JSON (`null` bzw. fehlend → `None`, nicht 0; AI-001).
pub(crate) fn opt_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

/// Bildformat aus den ersten Bytes (Signatur). Unterstützt sind PNG, JPEG, WebP und GIF (AI-002); andere Formate
/// (z. B. SVG) ergeben `None` (`Skipped (unsupported format)`).
pub fn sniff_image_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else {
        None
    }
}

/// Standard-Base64 (RFC 4648, mit Auffüllung).
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Bildeingabe aus Dateibytes: Format aus der Signatur, Inhalt als Base64 (AI-002). `None` bei nicht unterstütztem
/// Format.
pub fn image_input(bytes: &[u8]) -> Option<Input> {
    sniff_image_media_type(bytes).map(|media_type| Input::Image {
        media_type: media_type.to_string(),
        base64: base64_encode(bytes),
    })
}

/// Millisekunden seit `start`.
pub(crate) fn elapsed_ms(start: std::time::Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Rohantwort aus dem Antworttext: bei Schema-Aufgaben JSON (bzw. `Unparsable` mit Grund), sonst Text.
pub(crate) fn raw_from_text(text: String, structured: bool, truncated: bool) -> RawAnswer {
    if !structured {
        return RawAnswer::Text(text);
    }
    if truncated {
        // Abgeschnittenes JSON wird nie teilweise übernommen (AI-002).
        return RawAnswer::Unparsable {
            detail: "output truncated at max_tokens".into(),
        };
    }
    match serde_json::from_str::<Value>(text.trim()) {
        Ok(value) => RawAnswer::Json(value),
        Err(error) => RawAnswer::Unparsable {
            detail: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_4648_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(input.as_bytes()), expected);
        }
    }

    #[test]
    fn api_key_never_prints() {
        let key = ApiKey::new("sk-ant-TEST-AI-002");
        assert_eq!(format!("{key:?}"), "ApiKey(***)");
        assert_eq!(format!("{key}"), "***");
    }
}

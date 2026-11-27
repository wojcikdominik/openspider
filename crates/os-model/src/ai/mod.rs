//! Shared AI values; usable by analysis and storage without depending on os-ai.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub mod hook;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AiStatus {
    #[serde(rename = "Success")]
    Success,
    #[serde(rename = "Success (cached)")]
    SuccessCached,
    #[serde(rename = "Refusal")]
    Refusal,
    #[serde(rename = "Pending")]
    Pending,
    #[serde(rename = "Pending (batch)")]
    PendingBatch,
    #[serde(rename = "Pending (confirmation)")]
    PendingConfirmation,
    #[serde(rename = "Pending (consent)")]
    PendingConsent,
    #[serde(rename = "Skipped (scope)")]
    SkippedScope,
    #[serde(rename = "Skipped (segment)")]
    SkippedSegment,
    #[serde(rename = "Skipped (empty input)")]
    SkippedEmptyInput,
    #[serde(rename = "Skipped (policy)")]
    SkippedPolicy,
    #[serde(rename = "Skipped (no capable model)")]
    SkippedNoCapableModel,
    #[serde(rename = "Skipped (image too large)")]
    SkippedImageTooLarge,
    #[serde(rename = "Skipped (unsupported format)")]
    SkippedUnsupportedFormat,
    #[serde(rename = "Skipped (no declared language)")]
    SkippedNoDeclaredLanguage,
    #[serde(rename = "Budget exceeded")]
    BudgetExceeded,
    #[serde(rename = "Quota exceeded")]
    QuotaExceeded,
    #[serde(rename = "Error: Missing API key")]
    MissingApiKey,
    #[serde(rename = "Error: Invalid output")]
    InvalidOutput,
    #[serde(rename = "Error: Missing answer")]
    MissingAnswer,
    #[serde(rename = "Error: Batch expired")]
    BatchExpired,
    /// Keine HTTP-Antwort (DNS, Verbindungsaufbau, TLS, Verbindungsabbruch) nach allen Wiederholungen
    /// (AI-020, Nachtrag 24.11.2026, K4-15); die Ursache steht im Protokoll als `transport/<art>`.
    #[serde(rename = "Error: Connection failed")]
    ConnectionFailed,
    /// Die Antwort endete an der Ausgabegrenze (`max_tokens`, `max_output_tokens`, `length`); kein Teilergebnis, kein
    /// Reparaturversuch, keine Wiederholung, nicht im Schutzschalter (AI-020, Nachtrag 24.11.2026, Q00-H1).
    #[serde(rename = "Error: Output limit reached")]
    OutputLimitReached,
    /// Ein Eingabefeld der Frage ist für die URL weder gespeichert noch ableitbar (AI-020, Nachtrag 26.11.2026; AI-032,
    /// Nachtrag „Quelle der Eingabefelder“). Übersprungen, kein Request; zählt nicht zum Filter „Errors“.
    #[serde(rename = "Skipped (input unavailable)")]
    SkippedInputUnavailable,
    Timeout {
        seconds: u64,
    },
    HttpError {
        code: u16,
        message: String,
    },
}
impl AiStatus {
    /// Errors filter and retry selection have exactly the same membership (AI-020).
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            Self::BudgetExceeded
                | Self::QuotaExceeded
                | Self::MissingApiKey
                | Self::Timeout { .. }
                | Self::InvalidOutput
                | Self::MissingAnswer
                | Self::BatchExpired
                | Self::ConnectionFailed
                | Self::OutputLimitReached
                | Self::HttpError { .. }
        )
    }
}
impl std::fmt::Display for AiStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => f.write_str("Success"),
            Self::SuccessCached => f.write_str("Success (cached)"),
            Self::Refusal => f.write_str("Refusal"),
            Self::Pending => f.write_str("Pending"),
            Self::PendingBatch => f.write_str("Pending (batch)"),
            Self::PendingConfirmation => f.write_str("Pending (confirmation)"),
            Self::PendingConsent => f.write_str("Pending (consent)"),
            Self::SkippedScope => f.write_str("Skipped (scope)"),
            Self::SkippedSegment => f.write_str("Skipped (segment)"),
            Self::SkippedEmptyInput => f.write_str("Skipped (empty input)"),
            Self::SkippedPolicy => f.write_str("Skipped (policy)"),
            Self::SkippedNoCapableModel => f.write_str("Skipped (no capable model)"),
            Self::SkippedImageTooLarge => f.write_str("Skipped (image too large)"),
            Self::SkippedUnsupportedFormat => f.write_str("Skipped (unsupported format)"),
            Self::SkippedNoDeclaredLanguage => f.write_str("Skipped (no declared language)"),
            Self::BudgetExceeded => f.write_str("Budget exceeded"),
            Self::QuotaExceeded => f.write_str("Quota exceeded"),
            Self::MissingApiKey => f.write_str("Error: Missing API key"),
            Self::InvalidOutput => f.write_str("Error: Invalid output"),
            Self::MissingAnswer => f.write_str("Error: Missing answer"),
            Self::BatchExpired => f.write_str("Error: Batch expired"),
            Self::ConnectionFailed => f.write_str("Error: Connection failed"),
            Self::OutputLimitReached => f.write_str("Error: Output limit reached"),
            Self::SkippedInputUnavailable => f.write_str("Skipped (input unavailable)"),
            Self::Timeout { seconds } => write!(f, "Error: Timeout nach {seconds} s"),
            Self::HttpError { code, message } => write!(f, "Error: {code} {message}"),
        }
    }
}

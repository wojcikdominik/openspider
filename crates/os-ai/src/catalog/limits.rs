//! `resources/ai/provider-limits.json` (AI-012, Nachtrag Q00): Grundanteil je Decisions-Frage und Schema-Zuschlag je
//! Anbieter. Nur Daten und Nachschlagen; die Schätzung selbst baut M4-K10.
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{CatalogError, CivilDate, Endpoint};

/// JSON-Schema der Datei.
pub const PROVIDER_LIMITS_SCHEMA: &str = include_str!("schemas/provider-limits.schema.json");
/// Ausgelieferte Datei.
pub const BUNDLED_PROVIDER_LIMITS: &str =
    include_str!("../../../../resources/ai/provider-limits.json");

/// Ein Wert mit Quelle und Prüfdatum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcedValue {
    pub value: u64,
    pub source_url: String,
    pub verified_at: String,
}

/// Eine Regel des Schema-Zuschlags. Felder ohne Wert passen auf alles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaOverheadRule {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<Endpoint>,
    /// Präfix der Modell-ID (z. B. `openai/` bei OpenRouter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_prefix: Option<String>,
    pub tokens: u64,
}

/// Schema-Zuschlag: erste passende Regel, sonst `default_tokens`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaOverhead {
    pub default_tokens: u64,
    pub rules: Vec<SchemaOverheadRule>,
    pub source_url: String,
    pub verified_at: String,
}

/// Inhalt von `provider-limits.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderLimitsFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub decisions_question_overhead_tokens: SourcedValue,
    pub schema_overhead_tokens: SchemaOverhead,
}

impl ProviderLimitsFile {
    pub fn from_json_str(text: &str) -> Result<Self, CatalogError> {
        let doc: Value =
            serde_json::from_str(text).map_err(|e| CatalogError::Invalid(e.to_string()))?;
        let schema: Value =
            serde_json::from_str(PROVIDER_LIMITS_SCHEMA).expect("Schema ist gültiges JSON");
        crate::schema::ResponseSchema::new(schema)
            .expect("Schema kompiliert")
            .validate(&doc)
            .map_err(|e| CatalogError::Invalid(e.join("; ")))?;
        let file: Self =
            serde_json::from_value(doc).map_err(|e| CatalogError::Invalid(e.to_string()))?;
        for date in [
            &file.decisions_question_overhead_tokens.verified_at,
            &file.schema_overhead_tokens.verified_at,
        ] {
            if CivilDate::parse(date).is_none() {
                return Err(CatalogError::Invalid(format!(
                    "verified_at {date:?} ist kein Datum"
                )));
            }
        }
        Ok(file)
    }

    pub fn bundled() -> Result<Self, CatalogError> {
        Self::from_json_str(BUNDLED_PROVIDER_LIMITS)
    }

    /// Grundanteil G je Decisions-Frage (Standard 100).
    pub fn decisions_question_overhead(&self) -> u64 {
        self.decisions_question_overhead_tokens.value
    }

    /// Schema-Zuschlag für eine Schema-Aufgabe auf (Anbieter, Endpunkt, Modell).
    pub fn schema_overhead(&self, provider: &str, endpoint: Endpoint, model: &str) -> u64 {
        self.schema_overhead_tokens
            .rules
            .iter()
            .find(|r| {
                r.provider == provider
                    && r.endpoint.is_none_or(|e| e == endpoint)
                    && r.model_prefix
                        .as_deref()
                        .is_none_or(|p| model.starts_with(p))
            })
            .map_or(self.schema_overhead_tokens.default_tokens, |r| r.tokens)
    }
}

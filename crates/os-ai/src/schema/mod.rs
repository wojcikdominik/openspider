//! Strukturierte Ausgabe und lokale Schema-Validierung (AI-021).
//!
//! * [`ResponseSchema`] validiert jede Antwort lokal gegen das JSON-Schema der Aufgabe – unabhängig davon, ob der
//!   Anbieter selbst ein Schema erzwingt.
//! * [`run_with_repair`] führt einen Eintrag mit der konfigurierten Zahl Reparaturversuche aus ([`RepairAttempts`],
//!   Standard 1, Bereich 0–2): Bei Verstoß (oder nicht parsebarer Antwort) folgt ein weiterer Request mit derselben
//!   Eingabe plus der Validator-Meldung; scheitert auch der letzte, gilt `Error: Invalid output`.
//! * [`normalize_text`]/[`normalize_value`] normalisieren Antworttexte vor der Übernahme.
//! * Ein Abbruch an der Ausgabegrenze ist **kein** Schemaverstoß (AI-021, Nachtrag 24.11.2026, Q00-H1): `send` meldet
//!   ihn als Fehler (z. B. [`crate::adapters::anthropic::OutputLimitReached::output_limit_failure`]); der Eintrag endet
//!   ohne Reparaturversuch mit `Error: Output limit reached` ([`StructuredOutcome::final_status`]). Endet ein
//!   Reparaturversuch an der Grenze, gilt dasselbe.
//! * [`strict_violations`] prüft, ob ein Schema mit dem strikten Modus der Anbieter verträglich ist (jede Property in
//!   `required`, `additionalProperties: false` auf jeder Ebene).
use std::future::Future;

use os_model::ai::AiStatus;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Ein Schema ist selbst ungültig.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError(pub String);

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid JSON schema: {}", self.0)
    }
}

impl std::error::Error for SchemaError {}

/// Kompiliertes Antwortschema einer Aufgabe.
pub struct ResponseSchema {
    schema: Value,
    validator: jsonschema::Validator,
}

impl std::fmt::Debug for ResponseSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponseSchema")
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

impl ResponseSchema {
    /// Kompiliert `schema` (Entwurf aus `$schema`, sonst 2020-12).
    pub fn new(schema: Value) -> Result<Self, SchemaError> {
        let validator =
            jsonschema::validator_for(&schema).map_err(|e| SchemaError(e.to_string()))?;
        Ok(Self { schema, validator })
    }

    /// Das Schema.
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    /// Prüft eine Antwort. Fehler: Validator-Meldungen in stabiler Reihenfolge, je Meldung mit JSON-Pointer der Stelle.
    pub fn validate(&self, value: &Value) -> Result<(), Vec<String>> {
        match self.validator.validate(value) {
            Ok(()) => Ok(()),
            Err(errors) => {
                let mut messages: Vec<String> = errors
                    .map(|e| {
                        let path = e.instance_path.to_string();
                        if path.is_empty() {
                            e.to_string()
                        } else {
                            format!("{path}: {e}")
                        }
                    })
                    .collect();
                messages.sort();
                messages.dedup();
                Err(messages)
            }
        }
    }
}

/// Entfernt führenden und abschließenden Leerraum und alle Steuerzeichen (Unicode-Kategorie Cc) außer Zeilenumbruch
/// (`\n`) und Tabulator (`\t`) im Inneren; `\r` entfällt (CRLF → LF).
pub fn normalize_text(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    cleaned.trim().to_string()
}

/// Wendet [`normalize_text`] auf alle Zeichenketten eines JSON-Werts an (Objektschlüssel bleiben unverändert).
pub fn normalize_value(value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(normalize_text(&s)),
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_value).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, normalize_value(v)))
                .collect(),
        ),
        other => other,
    }
}

/// Rohantwort eines Requests vor der Validierung.
#[derive(Debug, Clone, PartialEq)]
pub enum RawAnswer {
    /// Bereits geparstes JSON (z. B. aus einer nativen Schema-Funktion).
    Json(Value),
    /// Antworttext, der JSON enthalten soll.
    Text(String),
    /// Die Antwort ließ sich nicht lesen (z. B. Adapter-Fehler `InvalidOutput`); `detail` ist die Parser-Meldung.
    Unparsable { detail: String },
    /// Verweigert (AI-038); wird nicht validiert und nicht repariert.
    Refusal,
}

/// Zahl der Reparaturversuche (AI-021: Standard 1, Bereich 0–2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct RepairAttempts(u8);

impl RepairAttempts {
    /// Größter zulässiger Wert.
    pub const MAX: u8 = 2;

    /// `None` außerhalb von 0–2.
    pub fn new(attempts: u8) -> Option<Self> {
        (attempts <= Self::MAX).then_some(Self(attempts))
    }

    /// Der Wert.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for RepairAttempts {
    fn default() -> Self {
        Self(1)
    }
}

impl TryFrom<u8> for RepairAttempts {
    type Error = String;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or_else(|| format!("repair attempts must be 0–2, got {value}"))
    }
}

impl From<RepairAttempts> for u8 {
    fn from(value: RepairAttempts) -> Self {
        value.0
    }
}

/// Hinweis für einen Reparaturversuch: dieselbe Eingabe plus die Meldung des Validators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairRequest {
    /// Nummer des Reparaturversuchs (1 …).
    pub attempt: u8,
    /// Validator- bzw. Parser-Meldung zur vorigen Antwort.
    pub validator_error: String,
}

impl RepairRequest {
    /// Zusatz zur Eingabe, der dem Modell den Verstoß mitteilt.
    pub fn instruction(&self) -> String {
        format!(
            "The previous answer was rejected by the JSON schema validator: {}. Answer again with JSON that \
             matches the schema exactly.",
            self.validator_error
        )
    }
}

/// Bewertung einer Antwort.
fn evaluate(schema: &ResponseSchema, raw: RawAnswer) -> Result<Option<Value>, String> {
    let value = match raw {
        RawAnswer::Refusal => return Ok(None),
        RawAnswer::Json(v) => v,
        RawAnswer::Text(text) => {
            let text = text.trim();
            serde_json::from_str::<Value>(text)
                .map_err(|e| format!("response is not valid JSON: {e}"))?
        }
        RawAnswer::Unparsable { detail } => {
            return Err(format!("response is not valid JSON: {detail}"))
        }
    };
    let value = normalize_value(value);
    schema
        .validate(&value)
        .map(|()| Some(value))
        .map_err(|errors| errors.join("; "))
}

/// Ein Request innerhalb von [`run_with_repair`] mit dem Begleitwert des Aufrufers (z. B. Protokolldaten).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedRequest<R> {
    pub meta: R,
    /// Validator- bzw. Parser-Meldung, wenn die Antwort dieses Requests verworfen wurde (für das Protokoll).
    pub validator_error: Option<String>,
}

/// Endergebnis eines Eintrags.
#[derive(Debug, Clone, PartialEq)]
pub enum StructuredOutcome<E> {
    /// Gültige, normalisierte Antwort.
    Valid(Value),
    /// Verweigert.
    Refusal,
    /// Auch nach allen Reparaturversuchen ungültig (`Error: Invalid output`); letzte Validator-Meldung.
    InvalidOutput { validator_error: String },
    /// Der Request selbst ist gescheitert (Fehler des Aufrufers, z. B. aus [`crate::limits`]).
    Failed(E),
}

impl<E> StructuredOutcome<E> {
    /// Status nach AI-020 für gültige Antworten, Refusals und ungültige Ausgabe; `None` bei `Failed` (den Status
    /// bestimmt dort der Aufrufer).
    pub fn status(&self) -> Option<AiStatus> {
        match self {
            Self::Valid(_) => Some(AiStatus::Success),
            Self::Refusal => Some(AiStatus::Refusal),
            Self::InvalidOutput { .. } => Some(AiStatus::InvalidOutput),
            Self::Failed(_) => None,
        }
    }
}

/// Endstatus des Fehlschlags eines Requests (AI-020), für [`StructuredOutcome::final_status`].
pub trait FailureStatus {
    /// Status nach AI-020.
    fn failure_status(&self) -> AiStatus;
}

impl FailureStatus for crate::provider::ProviderError {
    fn failure_status(&self) -> AiStatus {
        self.status().unwrap_or(AiStatus::ConnectionFailed)
    }
}

impl FailureStatus for crate::limits::AttemptFailure {
    fn failure_status(&self) -> AiStatus {
        self.error.failure_status()
    }
}

impl FailureStatus for crate::limits::FinalError {
    fn failure_status(&self) -> AiStatus {
        self.status.clone()
    }
}

impl FailureStatus for crate::adapters::anthropic::OutputLimitReached {
    fn failure_status(&self) -> AiStatus {
        AiStatus::OutputLimitReached
    }
}

impl<T: FailureStatus + ?Sized> FailureStatus for Box<T> {
    fn failure_status(&self) -> AiStatus {
        (**self).failure_status()
    }
}

impl<E: FailureStatus> StructuredOutcome<E> {
    /// Endstatus des Eintrags einschließlich gescheiterter Requests, z. B. `Error: Output limit reached` für einen
    /// Abbruch an der Ausgabegrenze (AI-021, Nachtrag Q00-H1).
    pub fn final_status(&self) -> AiStatus {
        match self {
            Self::Failed(error) => error.failure_status(),
            other => other.status().expect("Status ohne Fehlschlag"),
        }
    }
}

/// Ergebnis von [`run_with_repair`] mit allen Requests in Reihenfolge.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredRun<R, E> {
    pub outcome: StructuredOutcome<E>,
    pub requests: Vec<ValidatedRequest<R>>,
}

/// Führt einen Eintrag mit lokaler Validierung und genau `repairs` Reparaturversuchen aus. `send` erhält beim ersten
/// Request `None`, bei Reparaturen den [`RepairRequest`]; es liefert die Rohantwort und einen Begleitwert oder einen
/// Fehler (dann endet der Eintrag mit [`StructuredOutcome::Failed`]).
pub async fn run_with_repair<R, E, F, Fut>(
    schema: &ResponseSchema,
    repairs: RepairAttempts,
    mut send: F,
) -> StructuredRun<R, E>
where
    F: FnMut(Option<RepairRequest>) -> Fut,
    Fut: Future<Output = Result<(RawAnswer, R), E>>,
{
    let mut requests = Vec::new();
    let mut repair: Option<RepairRequest> = None;
    loop {
        let (raw, meta) = match send(repair.clone()).await {
            Ok(answer) => answer,
            Err(error) => {
                return StructuredRun {
                    outcome: StructuredOutcome::Failed(error),
                    requests,
                }
            }
        };
        match evaluate(schema, raw) {
            Ok(Some(value)) => {
                requests.push(ValidatedRequest {
                    meta,
                    validator_error: None,
                });
                return StructuredRun {
                    outcome: StructuredOutcome::Valid(value),
                    requests,
                };
            }
            Ok(None) => {
                requests.push(ValidatedRequest {
                    meta,
                    validator_error: None,
                });
                return StructuredRun {
                    outcome: StructuredOutcome::Refusal,
                    requests,
                };
            }
            Err(message) => {
                requests.push(ValidatedRequest {
                    meta,
                    validator_error: Some(message.clone()),
                });
                let used = repair.as_ref().map_or(0, |r| r.attempt);
                if used >= repairs.get() {
                    return StructuredRun {
                        outcome: StructuredOutcome::InvalidOutput {
                            validator_error: message,
                        },
                        requests,
                    };
                }
                repair = Some(RepairRequest {
                    attempt: used + 1,
                    validator_error: message,
                });
            }
        }
    }
}

/// Verstoß gegen den strikten Modus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictViolation {
    /// JSON-Pointer im Schema.
    pub pointer: String,
    pub kind: StrictViolationKind,
}

/// Art des Verstoßes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrictViolationKind {
    /// Objekt ohne `additionalProperties: false`.
    AdditionalPropertiesNotFalse,
    /// Property fehlt in `required` (optionale Felder sind im strikten Modus nicht erlaubt).
    NotRequired(String),
    /// `required` nennt eine Property, die nicht definiert ist.
    UnknownRequired(String),
}

fn escape_pointer(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn is_object_schema(schema: &serde_json::Map<String, Value>) -> bool {
    let typed_object = match schema.get("type") {
        Some(Value::String(t)) => t == "object",
        Some(Value::Array(types)) => types.iter().any(|t| t == "object"),
        _ => false,
    };
    typed_object
        || schema.contains_key("properties")
        || schema.contains_key("additionalProperties")
        || schema.contains_key("patternProperties")
}

/// Prüft ein Schema auf Verträglichkeit mit dem strikten Modus der Anbieter (AI-021): Jedes Objekt führt **alle**
/// Properties in `required` und hat `additionalProperties: false`, auf jeder Ebene (auch in `items`, `prefixItems`,
/// `anyOf`/`oneOf`/`allOf`, `$defs`/`definitions`, `not`, `if`/`then`/`else`). Leere Liste = verträglich.
pub fn strict_violations(schema: &Value) -> Vec<StrictViolation> {
    let mut out = Vec::new();
    walk(schema, String::new(), &mut out);
    out
}

fn walk(schema: &Value, pointer: String, out: &mut Vec<StrictViolation>) {
    let Value::Object(map) = schema else {
        return;
    };
    if is_object_schema(map) {
        if map.get("additionalProperties") != Some(&Value::Bool(false)) {
            out.push(StrictViolation {
                pointer: pointer.clone(),
                kind: StrictViolationKind::AdditionalPropertiesNotFalse,
            });
        }
        let required: Vec<&str> = map
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let properties = map.get("properties").and_then(Value::as_object);
        if let Some(properties) = properties {
            for name in properties.keys() {
                if !required.contains(&name.as_str()) {
                    out.push(StrictViolation {
                        pointer: pointer.clone(),
                        kind: StrictViolationKind::NotRequired(name.clone()),
                    });
                }
            }
        }
        for name in &required {
            if !properties.is_some_and(|p| p.contains_key(*name)) {
                out.push(StrictViolation {
                    pointer: pointer.clone(),
                    kind: StrictViolationKind::UnknownRequired((*name).to_string()),
                });
            }
        }
    }
    for keyword in ["properties", "$defs", "definitions", "patternProperties"] {
        if let Some(children) = map.get(keyword).and_then(Value::as_object) {
            for (name, child) in children {
                walk(
                    child,
                    format!("{pointer}/{keyword}/{}", escape_pointer(name)),
                    out,
                );
            }
        }
    }
    for keyword in [
        "items",
        "additionalProperties",
        "not",
        "if",
        "then",
        "else",
        "contains",
    ] {
        match map.get(keyword) {
            Some(child @ Value::Object(_)) => walk(child, format!("{pointer}/{keyword}"), out),
            Some(Value::Array(children)) if keyword == "items" => {
                for (i, child) in children.iter().enumerate() {
                    walk(child, format!("{pointer}/{keyword}/{i}"), out);
                }
            }
            _ => {}
        }
    }
    for keyword in ["prefixItems", "anyOf", "oneOf", "allOf"] {
        if let Some(children) = map.get(keyword).and_then(Value::as_array) {
            for (i, child) in children.iter().enumerate() {
                walk(child, format!("{pointer}/{keyword}/{i}"), out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-021
    #[test]
    fn normalization_trims_and_removes_control_characters() {
        assert_eq!(normalize_text("  Produkt\u{0}seite \r\n"), "Produktseite");
        assert_eq!(normalize_text("a\r\nb\tc\u{7}\u{1b}"), "a\nb\tc");
        assert_eq!(normalize_text("\u{85}ä\u{9f}"), "ä");
    }
}

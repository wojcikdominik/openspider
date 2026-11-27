//! Fragenformat der Decision-Engine (AI-030): Datenmodell, JSON-Schema, Prüfung beim Speichern bzw. Import,
//! parametrisierte Systemfragen (Vorlagen) und die Form, in der eine Frage an die Anbieter geht.
//!
//! * Datei `*.question.json`, Schema `resources/ai/questions/schema/question.schema.json` ([`SCHEMA_JSON`]). Das
//!   Schema prüft die Struktur; [`validate`] zusätzlich die Regeln, die JSON Schema nicht ausdrücken kann (eindeutige
//!   Werte und Labels, Platzhalter, Kandidaten-Vorlagen, Eingabefelder je `subject`). [`validate_active`] prüft eine
//!   Menge aktiver Fragen (eindeutige Namen, Reihenfolge bei `page_type`, AI-032).
//! * Vorlagen ([`QuestionDef::params`]): Platzhalter `{{<name>}}` nur in `instructions` und in `description` von
//!   `choices`/`levels`; eine Kandidaten-Vorlage hat genau die Werte `c1` … `ck` (k ≤ 10) plus `none`.
//!   [`instantiate`] setzt die Parameterwerte ein; jede Instanz ist wieder eine gültige Frage.
//! * Der an Anbieter gesendete Fragenname ist die `id` mit `_` statt `-` ([`wire_name`]).
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::adapters::openai_decisions::{ChoiceOption, Question, QuestionKind, ScoreLevel};
use crate::policy::{FieldSet, InputField};

/// JSON-Schema der Fragendatei.
pub const SCHEMA_JSON: &str =
    include_str!("../../../../../resources/ai/questions/schema/question.schema.json");

/// `id` der mitgelieferten Seitentyp-Frage (AI-050); Bezug für das Eingabefeld `page_type` (AI-032).
pub const PAGE_TYPE_QUESTION_ID: &str = "page-type";
/// Grenzen aus AI-030.
pub const MAX_ID_CHARS: usize = 48;
pub const MAX_NAME_CHARS: usize = 60;
pub const MAX_INSTRUCTIONS_CHARS: usize = 2_000;
pub const CHOICES: std::ops::RangeInclusive<usize> = 2..=20;
pub const LEVELS: std::ops::RangeInclusive<usize> = 2..=10;
/// Höchstzahl Kandidaten einer Kandidaten-Vorlage (AI-030).
pub const MAX_CANDIDATES: usize = 10;
/// Fester Wert „keiner der Kandidaten“.
pub const NONE_VALUE: &str = "none";

/// Feldfehler beim Speichern bzw. Import („Speicherfehler mit Feldangabe“, AI-030).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    /// Feld als JSON-Pointer (z. B. `/choices/2/value`); leer = ganze Datei.
    pub field: String,
    pub message: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.field.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.field, self.message)
        }
    }
}

/// SemVer-Version einer Frage (nur `MAJOR.MINOR.PATCH`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
    /// Minor-Erhöhung (inhaltliche Änderung, AI-031).
    pub fn bump_minor(self) -> Self {
        Self::new(self.major, self.minor + 1, 0)
    }
    /// Major-Erhöhung (Typwechsel, entfernter `choice`-Wert, AI-031).
    pub fn bump_major(self) -> Self {
        Self::new(self.major + 1, 0, 0)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for Version {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        let number = |p: &str| -> Result<u64, String> {
            let ok = !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p == "0" || !p.starts_with('0'));
            if !ok {
                return Err(format!("invalid version {s:?}"));
            }
            p.parse().map_err(|_| format!("invalid version {s:?}"))
        };
        match parts.as_slice() {
            [a, b, c] => Ok(Self::new(number(a)?, number(b)?, number(c)?)),
            _ => Err(format!(
                "invalid version {s:?} (expected MAJOR.MINOR.PATCH)"
            )),
        }
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Fragetyp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    Predicate,
    Choice,
    Score,
}

impl QuestionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Predicate => "predicate",
            Self::Choice => "choice",
            Self::Score => "score",
        }
    }
}

/// Gegenstand der Frage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subject {
    Page,
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceDef {
    pub value: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelDef {
    pub label: String,
    pub description: String,
}

/// Ein Eingabefeld (AI-032). In der Datei als Feldname (`"title"`) oder als Objekt mit Höchstlänge
/// (`{"field": "page_text", "max_chars": 8000}`; Auslegung von „`inputs.page_text.max_chars`“, Bericht d01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputSpec {
    pub field: InputField,
    /// Nur für `page_text`: Höchstlänge in Zeichen; fehlt sie, gilt die INTEG-172-Einstellung des Modells.
    pub max_chars: Option<u32>,
}

impl InputSpec {
    pub fn field(field: InputField) -> Self {
        Self {
            field,
            max_chars: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum InputSpecRepr {
    Name(InputField),
    Object {
        field: InputField,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_chars: Option<u32>,
    },
}

impl Serialize for InputSpec {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.max_chars {
            None => InputSpecRepr::Name(self.field),
            Some(max) => InputSpecRepr::Object {
                field: self.field,
                max_chars: Some(max),
            },
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for InputSpec {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match InputSpecRepr::deserialize(d)? {
            InputSpecRepr::Name(field) => Self::field(field),
            InputSpecRepr::Object { field, max_chars } => Self { field, max_chars },
        })
    }
}

/// Geltungsbereich (08 Konventionen „Grundmenge und Geltungsbereich“). Fehlende Felder: Standard-Geltungsbereich S
/// des Gegenstands ([`Scope::resolve`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_codes: Option<Vec<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexable_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment: Option<String>,
}

/// Aufgelöster Geltungsbereich.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// Inhaltsklassen (`html`, `image`, …).
    pub content_types: Vec<String>,
    pub status_codes: Vec<u16>,
    pub indexable_only: bool,
    pub segment: Option<String>,
}

impl Scope {
    /// Standard S: Seiten = interne HTML-URLs mit 200, indexierbar; Bilder = interne Bilder mit 200 (Auslegung für
    /// Bildfragen, Bericht d01).
    pub fn resolve(def: &ScopeDef, subject: Subject) -> Self {
        let (types, indexable) = match subject {
            Subject::Page => (vec!["html".to_string()], true),
            Subject::Image => (vec!["image".to_string()], false),
        };
        Self {
            content_types: def.content_types.clone().unwrap_or(types),
            status_codes: def.status_codes.clone().unwrap_or_else(|| vec![200]),
            indexable_only: def.indexable_only.unwrap_or(indexable),
            segment: def.segment.clone(),
        }
    }
}

/// Parametertyp einer Vorlage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    String,
    Candidates,
}

/// Parameter einer Vorlage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamDef {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ParamType,
    /// Feld oder Aufgabenwert, aus dem der Parameter befüllt wird.
    pub source: String,
}

/// Eine Frage (AI-030).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionDef {
    pub id: String,
    pub version: Version,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: QuestionType,
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<ChoiceDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<Vec<LevelDef>>,
    pub subject: Subject,
    pub inputs: Vec<InputSpec>,
    #[serde(default, skip_serializing_if = "is_default_scope")]
    pub scope: ScopeDef,
    /// Schwellwerte (AI-037, ausgewertet von M4-D02); keine inhaltliche Änderung (AI-031).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thresholds: Option<Value>,
    /// Issue-Definitionen (AI-041, M4-D04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues: Option<Value>,
    /// Segment-Definition (AI-040, M4-D04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment: Option<Value>,
    /// Parameter einer Vorlage; leer = keine Vorlage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ParamDef>,
}

fn is_default_scope(scope: &ScopeDef) -> bool {
    *scope == ScopeDef::default()
}

impl QuestionDef {
    /// An Anbieter gesendeter Fragenname.
    pub fn wire_name(&self) -> String {
        wire_name(&self.id)
    }
    /// Ob die Frage eine Vorlage ist.
    pub fn is_template(&self) -> bool {
        !self.params.is_empty()
    }
    /// Feldmenge der Eingabe (Teil des Einwilligungsschlüssels, AI-018).
    pub fn field_set(&self) -> FieldSet {
        self.inputs.iter().map(|i| i.field).collect()
    }
    /// Höchstlänge von `page_text`, falls angegeben.
    pub fn page_text_max_chars(&self) -> Option<u32> {
        self.inputs
            .iter()
            .find(|i| i.field == InputField::PageText)
            .and_then(|i| i.max_chars)
    }
    /// Aufgelöster Geltungsbereich.
    pub fn scope(&self) -> Scope {
        Scope::resolve(&self.scope, self.subject)
    }
    /// Werte bzw. Labels in definierter Reihenfolge.
    pub fn values(&self) -> Vec<String> {
        match self.kind {
            QuestionType::Choice => self
                .choices
                .iter()
                .flatten()
                .map(|c| c.value.clone())
                .collect(),
            QuestionType::Score => self
                .levels
                .iter()
                .flatten()
                .map(|l| l.label.clone())
                .collect(),
            QuestionType::Predicate => Vec::new(),
        }
    }
    /// Kanonisches JSON der Definition (Speicherung in `ai_questions`, Export, Vergleich).
    pub fn canonical_json(&self) -> String {
        crate::cache::canonical_json(&serde_json::to_value(self).unwrap_or(Value::Null))
    }
    /// Frage im Format des Decisions-Requests (AI-004): `name`, `type`, `instructions`, Werte.
    pub fn to_wire(&self) -> Question {
        let kind = match self.kind {
            QuestionType::Predicate => QuestionKind::Predicate,
            QuestionType::Choice => QuestionKind::Choice {
                choices: self
                    .choices
                    .iter()
                    .flatten()
                    .map(|c| ChoiceOption {
                        value: c.value.clone(),
                        description: c.description.clone(),
                    })
                    .collect(),
            },
            QuestionType::Score => QuestionKind::Score {
                levels: self
                    .levels
                    .iter()
                    .flatten()
                    .map(|l| ScoreLevel {
                        label: l.label.clone(),
                        description: l.description.clone(),
                    })
                    .collect(),
            },
        };
        Question {
            name: self.wire_name(),
            kind,
            instructions: self.instructions.clone(),
        }
    }
    /// [`QuestionDef::to_wire`] als JSON (für Fragengewicht und Token-Schätzung im Katalog bzw. Kostenkern).
    pub fn wire_json(&self) -> Value {
        serde_json::to_value(self.to_wire()).unwrap_or(Value::Null)
    }
}

/// Fragenname für Anbieter: `id` mit `-` → `_`.
pub fn wire_name(id: &str) -> String {
    id.replace('-', "_")
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some('a'..='z'))
        && id.chars().count() <= MAX_ID_CHARS
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
}

fn valid_value(value: &str) -> bool {
    (1..=32).contains(&value.chars().count())
        && value
            .chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '-'))
}

/// Platzhalter `{{name}}` eines Textes in Reihenfolge.
pub fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                out.push(after[..end].trim().to_string());
                rest = &after[end + 2..];
            }
            None => break,
        }
    }
    out
}

/// Liest eine Fragendatei: JSON, Schema, Datenmodell und [`validate`]. Fehler mit Feldangabe.
pub fn parse(text: &str) -> Result<QuestionDef, Vec<FieldError>> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| vec![FieldError::new("", format!("invalid JSON: {e}"))])?;
    parse_value(&value)
}

/// Wie [`parse`] für einen bereits gelesenen JSON-Wert.
pub fn parse_value(value: &Value) -> Result<QuestionDef, Vec<FieldError>> {
    schema_errors(value)?;
    let def: QuestionDef = serde_json::from_value(value.clone())
        .map_err(|e| vec![FieldError::new("", e.to_string())])?;
    validate(&def)?;
    Ok(def)
}

fn schema() -> &'static crate::schema::ResponseSchema {
    static SCHEMA: std::sync::OnceLock<crate::schema::ResponseSchema> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| {
        let value: Value = serde_json::from_str(SCHEMA_JSON).expect("question schema is JSON");
        crate::schema::ResponseSchema::new(value).expect("question schema compiles")
    })
}

/// Prüft nur gegen das JSON-Schema.
pub fn schema_errors(value: &Value) -> Result<(), Vec<FieldError>> {
    schema().validate(value).map_err(|messages| {
        messages
            .into_iter()
            .map(|m| match m.split_once(": ") {
                Some((pointer, msg)) if pointer.starts_with('/') => FieldError::new(pointer, msg),
                _ => FieldError::new("", m),
            })
            .collect()
    })
}

/// Felder, die eine Bildfrage verwenden darf: das Bild und seine Kontextfelder (Auslegung, Bericht d01).
pub const IMAGE_FIELDS: [InputField; 4] = [
    InputField::Image,
    InputField::Alt,
    InputField::ImageContext,
    InputField::Url,
];

/// Regeln einer einzelnen Frage (AI-030), die das Schema nicht abdeckt bzw. die beim Speichern immer gelten.
pub fn validate(def: &QuestionDef) -> Result<(), Vec<FieldError>> {
    let mut errors = core_errors(def);
    errors.extend(template_errors(def));
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Prüfung einer Instanz nach dem Einsetzen (AI-030): alle Regeln außer den Platzhalterregeln – eingesetzte Daten
/// (z. B. Adressen mit `{{…}}` im Query-String) sind keine Platzhalter mehr.
pub fn validate_instance(def: &QuestionDef) -> Result<(), Vec<FieldError>> {
    let errors = core_errors(def);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn core_errors(def: &QuestionDef) -> Vec<FieldError> {
    let mut errors = Vec::new();
    let mut err = |field: &str, message: String| errors.push(FieldError::new(field, message));
    if !valid_id(&def.id) {
        err(
            "/id",
            format!("id {:?} must match ^[a-z][a-z0-9-]{{0,47}}$", def.id),
        );
    }
    let name_chars = def.name.chars().count();
    if name_chars == 0 || name_chars > MAX_NAME_CHARS || def.name.trim().is_empty() {
        err(
            "/name",
            format!("name must be 1–{MAX_NAME_CHARS} characters"),
        );
    }
    let instr = def.instructions.chars().count();
    if instr == 0 || instr > MAX_INSTRUCTIONS_CHARS {
        err(
            "/instructions",
            format!("instructions must be 1–{MAX_INSTRUCTIONS_CHARS} characters, got {instr}"),
        );
    }
    match def.kind {
        QuestionType::Predicate => {
            if def.choices.is_some() {
                err("/choices", "a predicate has no choices".into());
            }
            if def.levels.is_some() {
                err("/levels", "a predicate has no levels".into());
            }
        }
        QuestionType::Choice => {
            if def.levels.is_some() {
                err("/levels", "a choice question has no levels".into());
            }
            let choices = def.choices.as_deref().unwrap_or(&[]);
            if !CHOICES.contains(&choices.len()) {
                err(
                    "/choices",
                    format!("a choice question needs 2–20 values, got {}", choices.len()),
                );
            }
            let mut seen = BTreeSet::new();
            for (i, c) in choices.iter().enumerate() {
                if !valid_value(&c.value) {
                    err(
                        &format!("/choices/{i}/value"),
                        format!("value {:?} must match ^[a-z0-9_-]{{1,32}}$", c.value),
                    );
                }
                if !seen.insert(c.value.as_str()) {
                    err(
                        &format!("/choices/{i}/value"),
                        format!("duplicate value {:?}", c.value),
                    );
                }
            }
        }
        QuestionType::Score => {
            if def.choices.is_some() {
                err("/choices", "a score question has no choices".into());
            }
            let levels = def.levels.as_deref().unwrap_or(&[]);
            if !LEVELS.contains(&levels.len()) {
                err(
                    "/levels",
                    format!("a score question needs 2–10 levels, got {}", levels.len()),
                );
            }
            let mut seen = BTreeSet::new();
            for (i, l) in levels.iter().enumerate() {
                if l.label.trim().is_empty() {
                    err(&format!("/levels/{i}/label"), "empty label".into());
                }
                if !seen.insert(l.label.as_str()) {
                    err(
                        &format!("/levels/{i}/label"),
                        format!("duplicate label {:?}", l.label),
                    );
                }
            }
        }
    }
    // Eingabefelder (AI-030, AI-032).
    if def.inputs.is_empty() {
        err("/inputs", "at least one input field is required".into());
    }
    let mut fields = BTreeSet::new();
    for (i, input) in def.inputs.iter().enumerate() {
        if !fields.insert(input.field) {
            err(
                &format!("/inputs/{i}"),
                format!("duplicate input field {}", input.field.id()),
            );
        }
        if input.max_chars.is_some() && input.field != InputField::PageText {
            err(
                &format!("/inputs/{i}/max_chars"),
                "max_chars is only allowed for page_text".into(),
            );
        }
        if input.max_chars == Some(0) {
            err(
                &format!("/inputs/{i}/max_chars"),
                "max_chars must be at least 1".into(),
            );
        }
        match def.subject {
            Subject::Image if !IMAGE_FIELDS.contains(&input.field) => err(
                &format!("/inputs/{i}"),
                format!(
                    "an image question only uses image, alt, image_context and url, not {}",
                    input.field.id()
                ),
            ),
            Subject::Page if matches!(input.field, InputField::Alt | InputField::ImageContext) => {
                err(
                    &format!("/inputs/{i}"),
                    format!("{} belongs to image questions", input.field.id()),
                )
            }
            _ => {}
        }
    }
    if def.subject == Subject::Image && !fields.contains(&InputField::Image) {
        err("/inputs", "an image question needs the field image".into());
    }
    if def.id == PAGE_TYPE_QUESTION_ID && fields.contains(&InputField::PageType) {
        err(
            "/inputs",
            "the page type question cannot use page_type".into(),
        );
    }
    errors
}

/// Regeln für Vorlagen und Platzhalter (AI-030).
fn template_errors(def: &QuestionDef) -> Vec<FieldError> {
    let mut errors = Vec::new();
    let params: BTreeMap<&str, ParamType> = def
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.kind))
        .collect();
    if params.len() != def.params.len() {
        errors.push(FieldError::new("/params", "duplicate parameter name"));
    }
    // Platzhalter dort, wo sie nie stehen dürfen.
    let forbidden: Vec<(String, &str)> = std::iter::once(("/id".to_string(), def.id.as_str()))
        .chain(std::iter::once(("/name".to_string(), def.name.as_str())))
        .chain(
            def.choices
                .iter()
                .flatten()
                .enumerate()
                .map(|(i, c)| (format!("/choices/{i}/value"), c.value.as_str())),
        )
        .chain(
            def.levels
                .iter()
                .flatten()
                .enumerate()
                .map(|(i, l)| (format!("/levels/{i}/label"), l.label.as_str())),
        )
        .collect();
    for (field, text) in forbidden {
        if text.contains("{{") {
            errors.push(FieldError::new(field, "placeholders are not allowed here"));
        }
    }
    let candidates: Vec<&str> = def
        .params
        .iter()
        .filter(|p| p.kind == ParamType::Candidates)
        .map(|p| p.name.as_str())
        .collect();
    // Erlaubte Stellen: instructions und description; Kandidatenparameter nur in der description von `ci`.
    let mut check = |field: String, text: &str, candidate_slot: bool| {
        for name in placeholders(text) {
            match params.get(name.as_str()) {
                None => errors.push(FieldError::new(
                    field.clone(),
                    format!("placeholder {{{{{name}}}}} has no entry in params"),
                )),
                Some(ParamType::Candidates) if !candidate_slot => errors.push(FieldError::new(
                    field.clone(),
                    format!("candidate list {{{{{name}}}}} only fills the description of c1 … ck"),
                )),
                _ => {}
            }
        }
    };
    check("/instructions".into(), &def.instructions, false);
    for (i, c) in def.choices.iter().flatten().enumerate() {
        let slot = candidate_index(&c.value).is_some();
        check(format!("/choices/{i}/description"), &c.description, slot);
    }
    for (i, l) in def.levels.iter().flatten().enumerate() {
        check(format!("/levels/{i}/description"), &l.description, false);
    }
    if candidates.len() > 1 {
        errors.push(FieldError::new(
            "/params",
            "at most one candidate list per template",
        ));
    }
    if !candidates.is_empty() {
        if def.kind != QuestionType::Choice {
            errors.push(FieldError::new(
                "/type",
                "a candidate template is a choice question",
            ));
        }
        let values: Vec<&str> = def
            .choices
            .iter()
            .flatten()
            .map(|c| c.value.as_str())
            .collect();
        let k = values.len().saturating_sub(1);
        let expected: Vec<String> = (1..=k)
            .map(|i| format!("c{i}"))
            .chain(std::iter::once(NONE_VALUE.to_string()))
            .collect();
        if !(1..=MAX_CANDIDATES).contains(&k) || values != expected {
            errors.push(FieldError::new(
                "/choices",
                "a candidate template has exactly the values c1 … ck (k ≤ 10) and none",
            ));
        }
    }
    errors
}

/// `ci` → `Some(i)` (ab 1).
pub fn candidate_index(value: &str) -> Option<usize> {
    value
        .strip_prefix('c')
        .filter(|n| !n.is_empty() && !n.starts_with('0') && n.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|n| n.parse().ok())
}

/// Regeln über die Menge aktiver Fragen in ihrer Reihenfolge: Namen eindeutig (AI-030), `page_type` nur unterhalb der
/// Seitentyp-Frage (AI-032, „Reihenfolgeabhängigkeit wie bei Segmenten“).
pub fn validate_active(active: &[QuestionDef]) -> Result<(), Vec<FieldError>> {
    let mut errors = Vec::new();
    let mut names = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let page_type_at = active.iter().position(|q| q.id == PAGE_TYPE_QUESTION_ID);
    for (i, q) in active.iter().enumerate() {
        if let Some(first) = names.insert(q.name.as_str(), i) {
            errors.push(FieldError::new(
                format!("/{i}/name"),
                format!(
                    "name {:?} is already used by active question {first}",
                    q.name
                ),
            ));
        }
        if !ids.insert(q.id.as_str()) {
            errors.push(FieldError::new(
                format!("/{i}/id"),
                format!("duplicate id {:?}", q.id),
            ));
        }
        let uses_page_type = q.inputs.iter().any(|f| f.field == InputField::PageType);
        if uses_page_type && !page_type_at.is_some_and(|p| p < i) {
            errors.push(FieldError::new(
                format!("/{i}/inputs"),
                "page_type needs the page type question above this question",
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Kandidat einer Kandidatenliste (AI-082): Adresse, Title, H1, Textauszug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub address: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub h1: String,
    #[serde(default)]
    pub excerpt: String,
}

impl Candidate {
    /// Text für die `description` von `ci`.
    pub fn description(&self) -> String {
        format!(
            "Address: {}\nTitle: {}\nH1: {}\nExcerpt: {}",
            self.address, self.title, self.h1, self.excerpt
        )
    }
}

/// Wert eines Vorlagenparameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    String(String),
    Candidates(Vec<Candidate>),
}

/// Eingesetzte Parameterwerte einer Instanz.
pub type ParamValues = BTreeMap<String, ParamValue>;

/// Kanonische JSON-Form der Parameterwerte (`ai_answers.params_json`, Cache-Schlüssel); `{}` ohne Parameter.
pub fn params_json(values: &ParamValues) -> Value {
    serde_json::to_value(values).unwrap_or_else(|_| Value::Object(Default::default()))
}

fn substitute(text: &str, values: &ParamValues, candidate: Option<&Candidate>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        out.push_str(&rest[..start]);
        let name = after[..end].trim();
        match values.get(name) {
            Some(ParamValue::String(v)) => out.push_str(v),
            Some(ParamValue::Candidates(_)) => {
                if let Some(c) = candidate {
                    out.push_str(&c.description());
                }
            }
            None => {}
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Setzt die Parameterwerte in eine Vorlage ein. Die Instanz hat keine `params` mehr und ist eine gültige Frage
/// (AI-030); bei Kandidaten-Vorlagen bleiben `c1` … `cn` (n = Zahl der Kandidaten, 1 ≤ n ≤ k) plus `none`. Die Version
/// bleibt die der Vorlage.
pub fn instantiate(
    template: &QuestionDef,
    values: &ParamValues,
) -> Result<QuestionDef, FieldError> {
    for p in &template.params {
        match (p.kind, values.get(&p.name)) {
            (ParamType::String, Some(ParamValue::String(_)))
            | (ParamType::Candidates, Some(ParamValue::Candidates(_))) => {}
            _ => {
                return Err(FieldError::new(
                    "/params",
                    format!("missing or mistyped value for parameter {:?}", p.name),
                ))
            }
        }
    }
    let mut out = template.clone();
    out.params.clear();
    out.instructions = substitute(&template.instructions, values, None);
    let candidates = template
        .params
        .iter()
        .find(|p| p.kind == ParamType::Candidates)
        .and_then(|p| match values.get(&p.name) {
            Some(ParamValue::Candidates(c)) => Some(c.as_slice()),
            _ => None,
        });
    if let Some(choices) = &template.choices {
        let mut instance = Vec::new();
        for choice in choices {
            match (candidates, candidate_index(&choice.value)) {
                (Some(list), Some(i)) => {
                    if let Some(c) = list.get(i - 1) {
                        instance.push(ChoiceDef {
                            value: choice.value.clone(),
                            description: substitute(&choice.description, values, Some(c)),
                        });
                    }
                }
                _ => instance.push(ChoiceDef {
                    value: choice.value.clone(),
                    description: substitute(&choice.description, values, None),
                }),
            }
        }
        if let Some(list) = candidates {
            let k = choices.len().saturating_sub(1);
            if list.is_empty() || list.len() > k {
                return Err(FieldError::new(
                    "/params",
                    format!("{} candidates do not fit c1 … c{k}", list.len()),
                ));
            }
        }
        out.choices = Some(instance);
    }
    if let Some(levels) = &template.levels {
        out.levels = Some(
            levels
                .iter()
                .map(|l| LevelDef {
                    label: l.label.clone(),
                    description: substitute(&l.description, values, None),
                })
                .collect(),
        );
    }
    validate_instance(&out).map_err(|mut e| e.remove(0))?;
    Ok(out)
}

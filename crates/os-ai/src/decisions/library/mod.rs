//! Fragenbibliothek, Benutzerfragen und Versionierung (AI-031).
//!
//! * Systemfragen liegen als `*.question.json` unter `resources/ai/questions/` (Inhalte: M4-D03) und sind
//!   schreibgeschützt; „Bearbeiten“ erzeugt eine Benutzerkopie mit Suffix `-custom` ([`Library::edit_copy`]).
//! * Jede inhaltliche Änderung (`instructions`, `choices`, `levels`, `inputs`, `type`, bei Vorlagen `params`) erhöht
//!   die Version: Minor (OS-Festlegung), Typwechsel oder entfernter `choice`-Wert Major; Anzeige- und
//!   Schwellwertänderungen nicht ([`next_version`]).
//! * Für jede in einem Crawl verwendete Version steht genau eine unveränderliche Kopie in `ai_questions`
//!   ([`record_used`]); ein Crawl behält seine Version, bis neu analysiert wird ([`update_notice`]).
//! * Export/Import als JSON mit Konfliktbehandlung „Ersetzen / als Kopie speichern“ ([`import`]).
use std::collections::BTreeMap;
use std::path::Path;

use os_store::ai::cache::Connection;
use os_store::ai::questions as store;
use os_store::StoreError;
use serde_json::Value;

use super::format::{self, FieldError, QuestionDef, Version};

/// Dateiendung der Fragendateien.
pub const FILE_SUFFIX: &str = ".question.json";
/// Suffix der Benutzerkopie einer Systemfrage.
pub const CUSTOM_SUFFIX: &str = "-custom";
/// Suffix einer beim Import „als Kopie“ gespeicherten Frage (Auslegung, Bericht d01).
pub const COPY_SUFFIX: &str = "-copy";

/// Herkunft einer Frage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Mitgeliefert, schreibgeschützt.
    System,
    /// Benutzerbibliothek bzw. Konfigurationsprofil.
    User,
}

/// Fehler der Bibliothek.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryError {
    /// Datei nicht lesbar.
    Io(String),
    /// Ungültige Frage (Datei, Feldfehler).
    Invalid {
        source: String,
        errors: Vec<FieldError>,
    },
    /// Zwei Systemfragen mit derselben `id`.
    DuplicateId(String),
    /// Systemfragen sind schreibgeschützt.
    ReadOnly(String),
    /// Unbekannte `id`.
    Unknown(String),
    /// Vorlagen sind nur als Systemfragen (bzw. deren Kopien) erlaubt (AI-030).
    TemplateNotAllowed(String),
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Invalid { source, errors } => {
                let list: Vec<String> = errors.iter().map(ToString::to_string).collect();
                write!(f, "{source}: {}", list.join("; "))
            }
            Self::DuplicateId(id) => write!(f, "duplicate question id {id}"),
            Self::ReadOnly(id) => write!(f, "library question {id} is read-only"),
            Self::Unknown(id) => write!(f, "unknown question {id}"),
            Self::TemplateNotAllowed(id) => {
                write!(
                    f,
                    "question {id}: templates are only allowed as system questions"
                )
            }
        }
    }
}

impl std::error::Error for LibraryError {}

/// Lädt alle `*.question.json` eines Verzeichnisses (nicht rekursiv), sortiert nach Dateiname; jede Datei wird gegen
/// Schema und Regeln geprüft (CI-Test der Bibliothek, AI-030).
pub fn load_dir(dir: &Path) -> Result<Vec<QuestionDef>, LibraryError> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| LibraryError::Io(format!("{}: {e}", dir.display())))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(FILE_SUFFIX))
        })
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let text = std::fs::read_to_string(p)
                .map_err(|e| LibraryError::Io(format!("{}: {e}", p.display())))?;
            format::parse(&text).map_err(|errors| LibraryError::Invalid {
                source: p.display().to_string(),
                errors,
            })
        })
        .collect()
}

/// Bibliothek mit Systemfragen und Benutzerfragen.
#[derive(Debug, Clone, Default)]
pub struct Library {
    system: BTreeMap<String, QuestionDef>,
    user: BTreeMap<String, QuestionDef>,
}

impl Library {
    /// Bibliothek aus Systemfragen (z. B. [`load_dir`]) und gespeicherten Benutzerfragen.
    pub fn new(system: Vec<QuestionDef>, user: Vec<QuestionDef>) -> Result<Self, LibraryError> {
        let mut lib = Self::default();
        for q in system {
            if lib.system.insert(q.id.clone(), q.clone()).is_some() {
                return Err(LibraryError::DuplicateId(q.id));
            }
        }
        for q in user {
            lib.save_user(q)?;
        }
        Ok(lib)
    }

    pub fn system(&self) -> impl Iterator<Item = &QuestionDef> {
        self.system.values()
    }
    pub fn user(&self) -> impl Iterator<Item = &QuestionDef> {
        self.user.values()
    }
    /// Frage nach `id` (Benutzerfragen und Systemfragen haben getrennte `id`s).
    pub fn get(&self, id: &str) -> Option<(Origin, &QuestionDef)> {
        self.system
            .get(id)
            .map(|q| (Origin::System, q))
            .or_else(|| self.user.get(id).map(|q| (Origin::User, q)))
    }

    /// Speichert eine Benutzerfrage (neu oder geändert). Eine Systemfrage kann so nicht überschrieben werden; eine
    /// Vorlage ist nur als Kopie einer Systemvorlage mit denselben `params` erlaubt (AI-030). Bei einer vorhandenen
    /// Benutzerfrage wird die Version nach [`next_version`] gesetzt; die übergebene Version gilt nur für neue Fragen.
    pub fn save_user(&mut self, mut question: QuestionDef) -> Result<&QuestionDef, LibraryError> {
        if self.system.contains_key(&question.id) {
            return Err(LibraryError::ReadOnly(question.id));
        }
        if question.is_template() && !self.system.values().any(|s| s.params == question.params) {
            return Err(LibraryError::TemplateNotAllowed(question.id));
        }
        format::validate(&question).map_err(|errors| LibraryError::Invalid {
            source: question.id.clone(),
            errors,
        })?;
        if let Some(old) = self.user.get(&question.id) {
            question.version = next_version(old, &question);
        }
        let id = question.id.clone();
        self.user.insert(id.clone(), question);
        Ok(&self.user[&id])
    }

    /// „Bearbeiten“ einer Systemfrage: Benutzerkopie mit `id` + `-custom` (gekürzt auf 48 Zeichen) und Name
    /// „<Name> (custom)“ (gekürzt auf 60 Zeichen, damit beide aktiv sein können; Auslegung). Eine vorhandene Kopie wird
    /// zurückgegeben, nicht überschrieben.
    pub fn edit_copy(&mut self, system_id: &str) -> Result<&QuestionDef, LibraryError> {
        let original = self
            .system
            .get(system_id)
            .ok_or_else(|| LibraryError::Unknown(system_id.to_string()))?;
        let id = suffixed(&original.id, CUSTOM_SUFFIX);
        if !self.user.contains_key(&id) {
            let mut copy = original.clone();
            copy.id = id.clone();
            copy.name = truncate_chars(
                &format!("{} (custom)", original.name),
                format::MAX_NAME_CHARS,
            );
            self.user.insert(id.clone(), copy);
        }
        Ok(&self.user[&id])
    }

    /// Entfernt eine Benutzerfrage.
    pub fn remove_user(&mut self, id: &str) -> Result<QuestionDef, LibraryError> {
        if self.system.contains_key(id) {
            return Err(LibraryError::ReadOnly(id.to_string()));
        }
        self.user
            .remove(id)
            .ok_or_else(|| LibraryError::Unknown(id.to_string()))
    }

    /// Import einer Fragendatei (AI-031): neue `id` → gespeichert; gleiche `id` und identische Definition → nichts zu
    /// tun; gleiche `id`, andere Definition → Rückfrage „Ersetzen / als Kopie speichern“ ([`Library::resolve`]).
    pub fn import(&mut self, text: &str) -> Result<Import, LibraryError> {
        let question = format::parse(text).map_err(|errors| LibraryError::Invalid {
            source: "import".into(),
            errors,
        })?;
        match self.get(&question.id) {
            None => {
                self.save_user(question.clone())?;
                Ok(Import::Added(Box::new(question)))
            }
            Some((_, existing)) if same_definition(existing, &question) => Ok(Import::Identical),
            Some((origin, existing)) => Ok(Import::Conflict {
                origin,
                existing: Box::new(existing.clone()),
                incoming: Box::new(question),
            }),
        }
    }

    /// Antwort auf die Rückfrage eines Imports. „Ersetzen“ ist nur bei Benutzerfragen möglich (Systemfragen sind
    /// schreibgeschützt) und übernimmt die Definition mit Versionsregel; „als Kopie“ speichert unter `id` + `-copy`
    /// (bzw. `-copy-2` …).
    pub fn resolve(
        &mut self,
        incoming: QuestionDef,
        choice: ImportChoice,
    ) -> Result<&QuestionDef, LibraryError> {
        match choice {
            ImportChoice::Replace => self.save_user(incoming),
            ImportChoice::SaveAsCopy => {
                let mut n = 1;
                let id = loop {
                    let suffix = if n == 1 {
                        COPY_SUFFIX.to_string()
                    } else {
                        format!("{COPY_SUFFIX}-{n}")
                    };
                    let id = suffixed(&incoming.id, &suffix);
                    if self.get(&id).is_none() {
                        break id;
                    }
                    n += 1;
                };
                let mut copy = incoming;
                copy.id = id;
                self.save_user(copy)
            }
        }
    }
}

/// Ergebnis eines Imports.
#[derive(Debug, Clone, PartialEq)]
pub enum Import {
    Added(Box<QuestionDef>),
    Identical,
    Conflict {
        origin: Origin,
        existing: Box<QuestionDef>,
        incoming: Box<QuestionDef>,
    },
}

/// Wahl bei einem Importkonflikt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportChoice {
    Replace,
    SaveAsCopy,
}

/// Export einer Frage (eingerücktes JSON in fester Feldreihenfolge); Import ergibt dieselbe Definition.
pub fn export(question: &QuestionDef) -> String {
    let value = serde_json::to_value(question).unwrap_or(Value::Null);
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

fn same_definition(a: &QuestionDef, b: &QuestionDef) -> bool {
    a.canonical_json() == b.canonical_json()
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// `id` + Suffix, gekürzt auf 48 Zeichen (ohne abschließenden Bindestrich vor dem Suffix).
fn suffixed(id: &str, suffix: &str) -> String {
    let keep = format::MAX_ID_CHARS.saturating_sub(suffix.chars().count());
    let base = truncate_chars(id, keep);
    format!("{}{suffix}", base.trim_end_matches('-'))
}

/// Art einer Änderung zwischen zwei Fassungen derselben Frage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeKind {
    /// Anzeige-, Geltungsbereichs- oder Schwellwertänderung: Version bleibt.
    None,
    /// Inhaltliche Änderung: Minor.
    Content,
    /// Typwechsel oder entfernter `choice`-Wert: Major.
    Breaking,
}

/// Bewertet die Änderung `old` → `new` (AI-031).
pub fn change_kind(old: &QuestionDef, new: &QuestionDef) -> ChangeKind {
    if old.kind != new.kind {
        return ChangeKind::Breaking;
    }
    let old_values: Vec<&str> = old
        .choices
        .iter()
        .flatten()
        .map(|c| c.value.as_str())
        .collect();
    let new_values: Vec<&str> = new
        .choices
        .iter()
        .flatten()
        .map(|c| c.value.as_str())
        .collect();
    if old_values.iter().any(|v| !new_values.contains(v)) {
        return ChangeKind::Breaking;
    }
    let content = |q: &QuestionDef| {
        serde_json::json!({
            "instructions": q.instructions,
            "choices": q.choices,
            "levels": q.levels,
            "inputs": q.inputs,
            "params": q.params,
        })
    };
    if content(old) != content(new) {
        ChangeKind::Content
    } else {
        ChangeKind::None
    }
}

/// Version nach einer Änderung (AI-031): 1.2.0 → 1.3.0 bei inhaltlicher Änderung, → 2.0.0 bei Typwechsel bzw.
/// entferntem `choice`-Wert, sonst unverändert.
pub fn next_version(old: &QuestionDef, new: &QuestionDef) -> Version {
    match change_kind(old, new) {
        ChangeKind::None => old.version,
        ChangeKind::Content => old.version.bump_minor(),
        ChangeKind::Breaking => old.version.bump_major(),
    }
}

/// Eine Feldänderung im Versionsvergleich.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    pub field: String,
    pub old: Value,
    pub new: Value,
}

/// Versionsvergleich zweier Fassungen (Diff auf Feldebene der obersten Ebene; Bibliotheksdialog AI-031).
pub fn diff(a: &QuestionDef, b: &QuestionDef) -> Vec<FieldChange> {
    let (va, vb) = (
        serde_json::to_value(a).unwrap_or(Value::Null),
        serde_json::to_value(b).unwrap_or(Value::Null),
    );
    let empty = serde_json::Map::new();
    let (ma, mb) = (
        va.as_object().unwrap_or(&empty),
        vb.as_object().unwrap_or(&empty),
    );
    let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|k| {
            let (x, y) = (
                ma.get(k).cloned().unwrap_or(Value::Null),
                mb.get(k).cloned().unwrap_or(Value::Null),
            );
            (x != y).then(|| FieldChange {
                field: k.clone(),
                old: x,
                new: y,
            })
        })
        .collect()
}

/// Hinweis, wenn Ergebnisse eines Crawls aus einer älteren Version stammen als die aktuelle Bibliotheksfrage:
/// „Frage aktualisiert (1.0.0 → 1.1.0) – neu analysieren?“ (AI-031). `None` bei gleicher oder neuerer Version.
pub fn update_notice(stored: Version, current: Version) -> Option<String> {
    (current > stored)
        .then(|| format!("Frage aktualisiert ({stored} → {current}) – neu analysieren?"))
}

/// Legt die unveränderliche Kopie einer verwendeten Version in `ai_questions` an (bei Vorlagen die Vorlage, nie eine
/// Instanz, AI-031). `true`, wenn sie neu war.
pub fn record_used(conn: &Connection, question: &QuestionDef) -> Result<bool, StoreError> {
    Ok(store::ensure(
        conn,
        &question.id,
        &question.version.to_string(),
        &question.canonical_json(),
    )? == store::Ensured::Inserted)
}

/// Liest eine gespeicherte Version aus `ai_questions` (z. B. zur Anzeige alter Ergebnisse).
pub fn stored_version(
    conn: &Connection,
    id: &str,
    version: &str,
) -> Result<Option<QuestionDef>, StoreError> {
    store::get(conn, id, version)?
        .map(|row| {
            format::parse(&row.definition_json).map_err(|e| {
                StoreError::InvalidInput(format!(
                    "ai_questions {id} {version}: {}",
                    e.first().map(ToString::to_string).unwrap_or_default()
                ))
            })
        })
        .transpose()
}

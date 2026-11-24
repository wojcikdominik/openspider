//! Standardwerte je Aufgabentyp `resources/ai/task-defaults.json` (AI-011).
//!
//! Die Felder `reasoning` und `answer_tokens` sind Spec (Tabelle in AI-011, Nachtrag 24.11.2026) und werden
//! ausgeliefert. Die Standardmodelle mit Ersatzliste schlägt der Orchestrator vor und bestätigt der Nutzer (E-4 b,
//! Plan K4-20); bis dahin trägt die Datei `"models_status": "pending-confirmation"` und `model: null`.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::provider::{Capabilities, ReasoningLevel};

use super::choice::ModelRef;
use super::pricing::{PriceKey, PriceTable};
use super::reasoning::AnswerTokens;
use super::{CatalogError, CivilDate, Endpoint};

/// JSON-Schema der Datei (CI-Test AI-011); liegt bei K04 (siehe Bericht, fremde Datei `resources/ai/schemas/`).
pub const TASK_DEFAULTS_SCHEMA: &str = include_str!("schemas/task-defaults.schema.json");
/// Ausgelieferte Standardwerte.
pub const BUNDLED_TASK_DEFAULTS: &str = include_str!("../../../../resources/ai/task-defaults.json");

/// Aufgabentypen (AI-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskType {
    Decision,
    DecisionVision,
    SuggestTitle,
    SuggestMeta,
    SuggestH1,
    SuggestAlt,
    RedirectConfirm,
    LinkAnchor,
    AskSql,
    AuditSummary,
    LlmsSummary,
    Embedding,
}

impl TaskType {
    pub const ALL: [TaskType; 12] = [
        Self::Decision,
        Self::DecisionVision,
        Self::SuggestTitle,
        Self::SuggestMeta,
        Self::SuggestH1,
        Self::SuggestAlt,
        Self::RedirectConfirm,
        Self::LinkAnchor,
        Self::AskSql,
        Self::AuditSummary,
        Self::LlmsSummary,
        Self::Embedding,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::DecisionVision => "decision-vision",
            Self::SuggestTitle => "suggest-title",
            Self::SuggestMeta => "suggest-meta",
            Self::SuggestH1 => "suggest-h1",
            Self::SuggestAlt => "suggest-alt",
            Self::RedirectConfirm => "redirect-confirm",
            Self::LinkAnchor => "link-anchor",
            Self::AskSql => "ask-sql",
            Self::AuditSummary => "audit-summary",
            Self::LlmsSummary => "llms-summary",
            Self::Embedding => "embedding",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == text)
    }

    /// Antwortanteil mit der Formel `decision-w` (AI-011, Nachtrag).
    pub fn uses_decision_formula(self) -> bool {
        matches!(
            self,
            Self::Decision | Self::DecisionVision | Self::RedirectConfirm
        )
    }

    /// Ob die Aufgabe über die Decisions API laufen kann (Standardweg E-3; sonst Ersatzweg AI-034).
    pub fn may_use_decisions(self) -> bool {
        self.uses_decision_formula()
    }

    /// Benötigte Fähigkeiten auf einem Endpunkt (AI-001, AI-011). Decisions deckt Text und Bild ab; auf anderen
    /// Endpunkten brauchen Entscheidungs- und Vorschlagsaufgaben strukturierte Ausgabe (AI-034, AI-070, AI-104),
    /// Bildaufgaben zusätzlich Vision. Auslegung K04 (Bericht): `audit-summary`, `llms-summary` nur Text.
    pub fn required_capabilities(self, endpoint: Endpoint) -> Capabilities {
        let mut caps = Capabilities::default();
        if endpoint == Endpoint::Decisions && self.may_use_decisions() {
            caps.decisions = true;
            return caps;
        }
        match self {
            Self::Embedding => caps.embeddings = true,
            Self::AuditSummary | Self::LlmsSummary => caps.text = true,
            Self::DecisionVision | Self::SuggestAlt => {
                caps.text = true;
                caps.structured_output = true;
                caps.vision = true;
            }
            _ => {
                caps.text = true;
                caps.structured_output = true;
            }
        }
        caps
    }
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Standard einer Aufgabe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDefault {
    /// Standard-Reasoning-Stufe; fehlt nur bei `embedding`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningLevel>,
    /// Antwortanteil A; fehlt nur bei `embedding`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_tokens: Option<AnswerTokens>,
    /// Standardmodell; `null` bis zur Bestätigung durch den Nutzer (K4-20).
    pub model: Option<ModelRef>,
    /// Geordnete Ersatzliste.
    pub fallback: Vec<ModelRef>,
}

/// Bestätigungsstand der Standardmodelle (E-4 b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelsStatus {
    /// Vorschlag ausstehend bzw. nicht bestätigt: keine Standardmodelle.
    PendingConfirmation,
    /// Vom Nutzer bestätigt: jede Aufgabe hat ein Standardmodell.
    Confirmed,
}

/// Inhalt von `task-defaults.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDefaults {
    pub models_status: ModelsStatus,
    pub source_url: String,
    pub verified_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub tasks: BTreeMap<TaskType, TaskDefault>,
}

/// Prüft ein Dokument gegen das Schema und die Regeln, die JSON-Schema nicht ausdrückt (Datum, Standardmodelle bei
/// `confirmed`). Liefert alle Befunde.
pub fn validate_task_defaults(doc: &Value) -> Result<(), Vec<String>> {
    let schema: Value =
        serde_json::from_str(TASK_DEFAULTS_SCHEMA).expect("Schema ist gültiges JSON");
    crate::schema::ResponseSchema::new(schema)
        .expect("Schema kompiliert")
        .validate(doc)?;
    let mut errors = Vec::new();
    if CivilDate::parse(doc["verified_at"].as_str().unwrap_or_default()).is_none() {
        errors.push("/verified_at: kein Datum JJJJ-MM-TT".to_string());
    }
    if doc["models_status"] == "confirmed" {
        for task in TaskType::ALL {
            if doc["tasks"][task.as_str()]["model"].is_null() {
                errors.push(format!(
                    "/tasks/{task}/model: Standardmodell fehlt (models_status confirmed)"
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

impl TaskDefaults {
    pub fn from_json_str(text: &str) -> Result<Self, CatalogError> {
        let doc: Value =
            serde_json::from_str(text).map_err(|e| CatalogError::Invalid(e.to_string()))?;
        validate_task_defaults(&doc).map_err(|e| CatalogError::Invalid(e.join("; ")))?;
        serde_json::from_value(doc).map_err(|e| CatalogError::Invalid(e.to_string()))
    }

    /// Die mitgelieferte Datei.
    pub fn bundled() -> Result<Self, CatalogError> {
        Self::from_json_str(BUNDLED_TASK_DEFAULTS)
    }

    /// Standard einer Aufgabe (nach dem Schema immer vorhanden).
    pub fn get(&self, task: TaskType) -> &TaskDefault {
        &self.tasks[&task]
    }

    /// Standard-Reasoning-Stufe (`embedding`: `model-default`).
    pub fn reasoning(&self, task: TaskType) -> ReasoningLevel {
        self.get(task).reasoning.unwrap_or_default()
    }

    /// Antwortanteil (`embedding`: keiner).
    pub fn answer_tokens(&self, task: TaskType) -> Option<AnswerTokens> {
        self.get(task).answer_tokens
    }

    /// Prüft, dass jedes Standard- und Ersatzmodell in `pricing.json` für den Endpunkt der Aufgabe existiert oder als
    /// OpenRouter- bzw. Ollama-Modell gekennzeichnet ist (AI-011), und dass die Fähigkeiten des Katalogs (falls
    /// angegeben) die Aufgabe tragen. Liefert alle Befunde.
    pub fn check_against_pricing(&self, pricing: &PriceTable) -> Vec<String> {
        let mut errors = Vec::new();
        for (task, default) in &self.tasks {
            let refs = default.model.iter().chain(default.fallback.iter());
            for (i, r) in refs.enumerate() {
                if matches!(r.provider.as_str(), "openrouter" | "ollama") {
                    continue;
                }
                let key = PriceKey::new(&r.provider, &r.model, r.endpoint);
                match pricing.get(&key) {
                    None => errors.push(format!(
                        "{task}[{i}]: {}/{}/{} fehlt in pricing.json",
                        r.provider,
                        r.model,
                        r.endpoint.as_str()
                    )),
                    Some(entry) => {
                        if let Some(caps) = entry.capabilities {
                            let caps: Capabilities = caps.into();
                            if !caps.supports(task.required_capabilities(r.endpoint)) {
                                errors.push(format!(
                                    "{task}[{i}]: {}/{} ohne benötigte Fähigkeit",
                                    r.provider, r.model
                                ));
                            }
                        }
                    }
                }
            }
        }
        errors
    }
}

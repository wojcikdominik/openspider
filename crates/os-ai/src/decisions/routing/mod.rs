//! Routing je Frage (AI-035): „automatisch“ (Standard), „immer Decisions API“ oder „immer strukturierte Ausgabe mit
//! Modell X“.
//!
//! * Automatisch: Decisions API, wenn ein OpenAI-Schlüssel vorhanden ist, die Richtlinie sie erlaubt und – bei
//!   Bildfragen – Bildeingabe unterstützt wird; sonst das erste verfügbare Modell der Ersatzliste der Aufgabe `decision`
//!   bzw. `decision-vision` (Modellwahl nach AI-011, [`plan_task`]).
//! * Immer Decisions API ohne OpenAI-Schlüssel → `Error: Missing API key` (kein stiller Wechsel auf den Ersatzweg).
//! * Unter „Nur lokal“ (AI-019) nur Ollama; ohne lokales Modell `Skipped (policy)`.
//! * Der Weg geht in den Cache-Schlüssel ein ([`Route::path`], AI-015).
use std::collections::BTreeMap;

use os_model::ai::AiStatus;
use serde::{Deserialize, Serialize};

use super::format::{QuestionDef, Subject};
use crate::adapters::ollama::PROVIDER_ID as OLLAMA;
use crate::adapters::openai_decisions::DEFAULT_MODEL as DECISIONS_MODEL;
use crate::adapters::openai_responses::PROVIDER_ID as OPENAI;
use crate::cache::ExecutionPath;
use crate::catalog::{
    plan_task, Endpoint, ModelRef, ProviderState, TaskDefaults, TaskPlan, TaskSettings, TaskType,
};

/// `<Q> Source` über die Decisions API.
pub const SOURCE_DECISIONS: &str = "decisions-api";
/// `<Q> Source` im Ersatzweg (Selbsteinschätzung des Modells, AI-034).
pub const SOURCE_SELF_REPORTED: &str = "self-reported";

/// Dropdown „Ausführung“ je Frage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum RoutingSetting {
    #[default]
    Automatic,
    DecisionsApi,
    StructuredOutput {
        model: ModelRef,
    },
}

/// Richtlinie (AI-019, Release 1.2; hier nur die Wirkung auf das Routing).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoutingPolicy {
    /// „Nur lokal“: nur Ollama.
    pub local_only: bool,
    /// Die Richtlinie verbietet die Decisions API (z. B. EU-Verarbeitung ohne EU-Endpunkt).
    pub decisions_forbidden: bool,
}

/// Aufgabentyp einer Frage: `decision` bzw. `decision-vision`.
pub fn task_of(question: &QuestionDef) -> TaskType {
    match question.subject {
        Subject::Page => TaskType::Decision,
        Subject::Image => TaskType::DecisionVision,
    }
}

/// Gewählter Weg einer Frage.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub task: TaskType,
    pub plan: TaskPlan,
}

impl Route {
    pub fn path(&self) -> ExecutionPath {
        if self.plan.uses_decisions_api() {
            ExecutionPath::DecisionsApi
        } else {
            ExecutionPath::StructuredOutput
        }
    }
    /// `<Q> Source`.
    pub fn source(&self) -> &'static str {
        match self.path() {
            ExecutionPath::DecisionsApi => SOURCE_DECISIONS,
            ExecutionPath::StructuredOutput => SOURCE_SELF_REPORTED,
        }
    }
    /// `<Q> Model`.
    pub fn model_id(&self) -> &str {
        &self.plan.model.id
    }
    pub fn provider(&self) -> &str {
        &self.plan.model.provider
    }
}

/// Eingaben des Routings: Standardwerte, Aufgabeneinstellungen (`decision`, `decision-vision`) und Anbieterzustand.
#[derive(Debug, Clone)]
pub struct RoutingContext<'a> {
    pub defaults: &'a TaskDefaults,
    pub settings: &'a BTreeMap<TaskType, TaskSettings>,
    pub providers: &'a BTreeMap<String, ProviderState>,
    pub policy: RoutingPolicy,
}

fn decisions_ref() -> ModelRef {
    ModelRef::new(OPENAI, DECISIONS_MODEL, Endpoint::Decisions)
}

/// Wählt den Weg einer Frage. Fehler ist der Status aller Einträge der Frage ohne Request.
pub fn route(
    question: &QuestionDef,
    setting: &RoutingSetting,
    ctx: &RoutingContext<'_>,
) -> Result<Route, AiStatus> {
    let task = task_of(question);
    let base = ctx.settings.get(&task).cloned().unwrap_or_default();
    let default = ctx.defaults.get(task);
    let mut settings = base.clone();
    match setting {
        RoutingSetting::Automatic => {
            // Gewähltes Modell bzw. Standard, dann die Ersatzliste (AI-011).
            let (primary, fallback) = match &base.model {
                Some(model) => (Some(model.clone()), base.fallback.clone()),
                None => (default.model.clone(), default.fallback.clone()),
            };
            let mut candidates: Vec<ModelRef> = primary.into_iter().chain(fallback).collect();
            if ctx.policy.decisions_forbidden {
                candidates.retain(|c| c.endpoint != Endpoint::Decisions);
            }
            if ctx.policy.local_only {
                return route_local(task, &base, ctx);
            }
            let mut iter = candidates.into_iter();
            settings.model = iter.next();
            settings.fallback = iter.collect();
            if settings.model.is_none() {
                return Err(AiStatus::SkippedNoCapableModel);
            }
        }
        RoutingSetting::DecisionsApi => {
            if ctx.policy.local_only || ctx.policy.decisions_forbidden {
                return Err(AiStatus::SkippedPolicy);
            }
            let model = match &base.model {
                Some(m) if m.endpoint == Endpoint::Decisions => m.clone(),
                _ => match &default.model {
                    Some(m) if m.endpoint == Endpoint::Decisions => m.clone(),
                    _ => decisions_ref(),
                },
            };
            settings.model = Some(model);
            settings.fallback = Vec::new();
        }
        RoutingSetting::StructuredOutput { model } => {
            if model.endpoint == Endpoint::Decisions {
                return Err(AiStatus::SkippedNoCapableModel);
            }
            if ctx.policy.local_only && model.provider != OLLAMA {
                return Err(AiStatus::SkippedPolicy);
            }
            settings.model = Some(model.clone());
            settings.fallback = Vec::new();
        }
    }
    let plan = plan_task(task, &settings, ctx.defaults, ctx.providers)?;
    Ok(Route { task, plan })
}

/// „Nur lokal“: erstes Ollama-Modell der Live-Liste mit den Fähigkeiten der Aufgabe; sonst `Skipped (policy)`.
fn route_local(
    task: TaskType,
    base: &TaskSettings,
    ctx: &RoutingContext<'_>,
) -> Result<Route, AiStatus> {
    let Some(state) = ctx.providers.get(OLLAMA).filter(|s| s.has_key) else {
        return Err(AiStatus::SkippedPolicy);
    };
    for available in &state.models {
        let Some(endpoint) = Endpoint::parse(&available.model.endpoint) else {
            continue;
        };
        let mut settings = base.clone();
        settings.model = Some(ModelRef::new(OLLAMA, &available.model.id, endpoint));
        settings.fallback = Vec::new();
        if let Ok(plan) = plan_task(task, &settings, ctx.defaults, ctx.providers) {
            return Ok(Route { task, plan });
        }
    }
    Err(AiStatus::SkippedPolicy)
}

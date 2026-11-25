//! Modellwahl je Aufgabe mit Ersatzliste (AI-011).
//!
//! Eingänge: Einstellungen der Aufgabe (Profil, [`TaskSettings`]), Standardwerte ([`TaskDefaults`]) und der Zustand
//! der Anbieter ([`ProviderState`]: Schlüssel vorhanden, Live-Modellliste mit Fähigkeiten, Reasoning-Stufen und
//! Ausgabegrenze). Ausgang: ein [`TaskPlan`] mit dem tatsächlich verwendeten Modell, der effektiven Reasoning-Stufe
//! und der Ausgabegrenze – oder der Statuswert für alle Einträge der Aufgabe (`Error: Missing API key` bzw.
//! `Skipped (no capable model)`).
use std::collections::BTreeMap;

use os_model::ai::AiStatus;
use os_model::config::{ModelChoice, TaskModelConfig};
use serde::{Deserialize, Serialize};

use crate::provider::{Model, ReasoningLevel, Request};

use super::reasoning::{
    choose_reasoning, default_max_tokens, output_limit, AnswerTokens, OutputLimit, ReasoningChoice,
};
use super::task_defaults::{TaskDefaults, TaskType};
use super::{CatalogError, Endpoint};

/// Verweis auf ein Modell (Anbieter, Modell, Endpunkt).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
    pub endpoint: Endpoint,
}

impl ModelRef {
    pub fn new(provider: &str, model: &str, endpoint: Endpoint) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            endpoint,
        }
    }

    fn matches(&self, model: &Model) -> bool {
        self.provider == model.provider
            && self.model == model.id
            && self.endpoint.as_str() == model.endpoint
    }
}

impl TryFrom<&ModelChoice> for ModelRef {
    type Error = CatalogError;
    fn try_from(c: &ModelChoice) -> Result<Self, Self::Error> {
        let endpoint = Endpoint::parse(&c.endpoint)
            .ok_or_else(|| CatalogError::Invalid(format!("Endpunkt {:?} unbekannt", c.endpoint)))?;
        Ok(Self::new(&c.provider, &c.model, endpoint))
    }
}

impl From<&ModelRef> for ModelChoice {
    fn from(r: &ModelRef) -> Self {
        ModelChoice {
            provider: r.provider.clone(),
            model: r.model.clone(),
            endpoint: r.endpoint.as_str().to_string(),
        }
    }
}

/// Ein Modell der Live-Liste mit seiner Ausgabegrenze (Anthropic `max_tokens`, OpenRouter
/// `top_provider.max_completion_tokens`, OpenAI-Katalog `max_output_tokens`; `None` = keine Kappung).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableModel {
    /// Mit Fähigkeiten und `reasoning_levels` (Quelle je Anbieter nach AI-002/AI-005/AI-010).
    pub model: Model,
    pub max_output_tokens: Option<u64>,
}

/// Zustand eines Anbieters für die Modellwahl.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderState {
    /// Schlüssel nutzbar (Ollama: immer `true`).
    pub has_key: bool,
    /// Live-Modellliste (leer, wenn nicht verbunden).
    pub models: Vec<AvailableModel>,
}

/// Einstellungen einer Aufgabe im Konfigurationsprofil. `None` = Standard aus `task-defaults.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TaskSettings {
    pub model: Option<ModelRef>,
    pub fallback: Vec<ModelRef>,
    /// Gespeicherte Reasoning-Stufe (Spalte „Reasoning“); bleibt bei Modellwechsel erhalten.
    pub reasoning: Option<ReasoningLevel>,
    /// Vom Nutzer eingestellte Ausgabegrenze.
    pub max_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub max_input_chars: Option<u64>,
}

impl TaskSettings {
    /// „Auf Standard zurücksetzen“: Modell, Ersatzliste, Reasoning-Stufe und Ausgabegrenze zurück auf den Standard.
    pub fn reset_to_default(&mut self) {
        *self = Self::default();
    }

    /// Aus dem Profil (`os_model::config::TaskModelConfig`). Die Reasoning-Stufe steht im Feld `effort`, das AI-011
    /// als „Aufwand“ führt und das der Nachtrag Q00 als Reasoning-Stufe konkretisiert (Auslegung K04).
    pub fn from_config(config: &TaskModelConfig) -> Result<Self, CatalogError> {
        let reasoning =
            match config.effort.as_deref() {
                None => None,
                Some(text) => Some(text.parse::<ReasoningLevel>().map_err(|e| {
                    CatalogError::Invalid(format!("Reasoning-Stufe im Profil: {e}"))
                })?),
            };
        Ok(Self {
            model: config.model.as_ref().map(ModelRef::try_from).transpose()?,
            fallback: config
                .fallback
                .iter()
                .map(ModelRef::try_from)
                .collect::<Result<_, _>>()?,
            reasoning,
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            max_input_chars: config.max_input_chars,
        })
    }

    /// Ins Profil (Export).
    pub fn to_config(&self) -> TaskModelConfig {
        TaskModelConfig {
            model: self.model.as_ref().map(ModelChoice::from),
            fallback: self.fallback.iter().map(ModelChoice::from).collect(),
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            effort: self.reasoning.map(|l| l.as_str().to_string()),
            max_input_chars: self.max_input_chars,
        }
    }
}

/// Liest alle Aufgaben eines Profils (`AiConfig::tasks`); unbekannte Aufgabentypen sind ein Fehler.
pub fn settings_from_profile(
    tasks: &BTreeMap<String, TaskModelConfig>,
) -> Result<BTreeMap<TaskType, TaskSettings>, CatalogError> {
    tasks
        .iter()
        .map(|(name, config)| {
            let task = TaskType::parse(name)
                .ok_or_else(|| CatalogError::Invalid(format!("Aufgabentyp {name:?} unbekannt")))?;
            Ok((task, TaskSettings::from_config(config)?))
        })
        .collect()
}

/// Schreibt alle Aufgaben ins Profil.
pub fn settings_to_profile(
    settings: &BTreeMap<TaskType, TaskSettings>,
) -> BTreeMap<String, TaskModelConfig> {
    settings
        .iter()
        .map(|(task, s)| (task.as_str().to_string(), s.to_config()))
        .collect()
}

/// Fehlende Fähigkeit bei der Zuweisung eines Modells (AI-011).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingCapability {
    pub task: TaskType,
    pub model: String,
    /// Namen der fehlenden Fähigkeiten (`vision`, `structured_output`, …).
    pub missing: Vec<&'static str>,
}

/// Ein Modell ohne benötigte Fähigkeit kann der Aufgabe nicht zugewiesen werden (AI-011).
pub fn validate_assignment(task: TaskType, model: &Model) -> Result<(), MissingCapability> {
    let endpoint = Endpoint::parse(&model.endpoint);
    let required = endpoint.map_or_else(
        || task.required_capabilities(Endpoint::Messages),
        |e| task.required_capabilities(e),
    );
    let have = model.capabilities;
    let missing: Vec<&'static str> = [
        ("text", required.text, have.text),
        (
            "structured_output",
            required.structured_output,
            have.structured_output,
        ),
        ("vision", required.vision, have.vision),
        ("embeddings", required.embeddings, have.embeddings),
        ("decisions", required.decisions, have.decisions),
    ]
    .into_iter()
    .filter(|(_, need, has)| *need && !*has)
    .map(|(name, _, _)| name)
    .collect();
    if missing.is_empty() && endpoint.is_some() {
        Ok(())
    } else {
        Err(MissingCapability {
            task,
            model: model.id.clone(),
            missing: if missing.is_empty() {
                vec!["endpoint"]
            } else {
                missing
            },
        })
    }
}

/// Verfügbarkeit eines gewählten Modells (Meldung „nicht mehr verfügbar“ wie INTEG-160, AI-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelAvailability {
    Available,
    /// Anbieter verbunden, Modell fehlt in der Live-Liste.
    NoLongerAvailable,
    /// Anbieter ohne Schlüssel bzw. ohne geladene Liste (keine Aussage über das Modell).
    ProviderNotConnected,
}

/// Verfügbarkeit eines Modells laut Anbieterzustand.
pub fn availability(
    model: &ModelRef,
    providers: &BTreeMap<String, ProviderState>,
) -> ModelAvailability {
    match providers.get(&model.provider) {
        Some(state) if state.has_key && !state.models.is_empty() => {
            if state.models.iter().any(|m| model.matches(&m.model)) {
                ModelAvailability::Available
            } else {
                ModelAvailability::NoLongerAvailable
            }
        }
        _ => ModelAvailability::ProviderNotConnected,
    }
}

/// Ausführungsplan einer Aufgabe.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskPlan {
    pub task: TaskType,
    /// Tatsächlich verwendetes Modell (Spalte `<…> Model`).
    pub model_ref: ModelRef,
    pub model: Model,
    /// 0 = gewähltes Modell, ab 1 = Position in der Ersatzliste.
    pub candidate_index: usize,
    pub reasoning: ReasoningChoice,
    pub answer_tokens: Option<AnswerTokens>,
    /// Ausgabegrenze des Modells.
    pub model_output_limit: Option<u64>,
    /// Vom Nutzer eingestellte Ausgabegrenze.
    pub user_max_tokens: Option<u64>,
}

impl TaskPlan {
    /// Läuft über die Decisions API (sonst Ersatzweg mit `<Q> Source = self-reported`, AI-034).
    pub fn uses_decisions_api(&self) -> bool {
        self.model_ref.endpoint == Endpoint::Decisions
    }

    /// Ausgabegrenze für einen Request; `weight` = W bei Aufgaben mit `decision-w`. `None` für Endpunkte ohne
    /// Ausgabegrenze (Decisions, Embeddings).
    pub fn output_limit(&self, weight: Option<u64>) -> Option<OutputLimit> {
        if matches!(
            self.model_ref.endpoint,
            Endpoint::Decisions | Endpoint::Embeddings
        ) {
            return None;
        }
        let answer = self.answer_tokens?.tokens(weight);
        let standard =
            default_max_tokens(answer, self.reasoning.effective, self.model_output_limit);
        Some(output_limit(standard, self.user_max_tokens))
    }

    /// Setzt Modell, effektive Stufe und `max_tokens` im gemeinsamen Request (Adapter übersetzen sie, K09).
    pub fn apply_to(&self, request: &mut Request, weight: Option<u64>) {
        request.model = self.model.clone();
        request.reasoning = self.reasoning.effective;
        request.max_tokens = self.output_limit(weight).map(|l| l.value);
    }
}

/// Wählt das Modell einer Aufgabe: gewähltes Modell (bzw. Standard), dann die Ersatzliste in Reihenfolge; ein
/// Kandidat zählt, wenn sein Anbieter einen Schlüssel hat, das Modell in der Live-Liste steht und die benötigten
/// Fähigkeiten hat. Kein Kandidat → `Error: Missing API key`, wenn keinem Kandidaten-Anbieter ein Schlüssel vorliegt,
/// sonst `Skipped (no capable model)` (AI-011).
pub fn plan_task(
    task: TaskType,
    settings: &TaskSettings,
    defaults: &TaskDefaults,
    providers: &BTreeMap<String, ProviderState>,
) -> Result<TaskPlan, AiStatus> {
    let default = defaults.get(task);
    let (primary, fallback) = match &settings.model {
        Some(model) => (Some(model), settings.fallback.as_slice()),
        None => (default.model.as_ref(), default.fallback.as_slice()),
    };
    let requested = settings
        .reasoning
        .unwrap_or_else(|| defaults.reasoning(task));
    let mut any_key = false;
    for (index, candidate) in primary.into_iter().chain(fallback.iter()).enumerate() {
        let Some(state) = providers.get(&candidate.provider) else {
            continue;
        };
        if !state.has_key {
            continue;
        }
        any_key = true;
        let Some(found) = state.models.iter().find(|m| candidate.matches(&m.model)) else {
            continue;
        };
        if validate_assignment(task, &found.model).is_err() {
            continue;
        }
        // Die Decisions API hat keine Stufe (AI-004, AI-011): kein Parameter, keine Warnung.
        let reasoning = if candidate.endpoint == Endpoint::Decisions {
            ReasoningChoice {
                requested,
                effective: ReasoningLevel::ModelDefault,
                dropdown: vec![ReasoningLevel::ModelDefault],
                warning: None,
            }
        } else {
            choose_reasoning(requested, &found.model.reasoning_levels)
        };
        return Ok(TaskPlan {
            task,
            model_ref: candidate.clone(),
            model: found.model.clone(),
            candidate_index: index,
            reasoning,
            answer_tokens: defaults.answer_tokens(task),
            model_output_limit: found.max_output_tokens,
            user_max_tokens: settings.max_tokens,
        });
    }
    Err(if any_key {
        AiStatus::SkippedNoCapableModel
    } else {
        AiStatus::MissingApiKey
    })
}

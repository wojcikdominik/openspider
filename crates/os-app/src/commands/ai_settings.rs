//! KI-Einstellungen der Service-Schicht (M4-U01): Modellwahl je Aufgabe mit Spalte „Reasoning“ (AI-011), Richtlinie
//! „Keine Einschränkung / EU-Verarbeitung erzwingen / Nur lokal“ (AI-019), Crawl-Budget (AI-013), Schwärzungsmuster
//! (AI-024) und die Vorgaben des Reiters „Erweitert“ (INTEG-005 [M4], Ollama Standard 1).
//!
//! Commands (Vertrag aus `ai_catalog`, Plan M4 3.3):
//! - `ai.settingsGet` (void → JSON [`SettingsView`]): Sicht auf die Konfiguration des Fensters (`config.get`), die
//!   App-Einstellungen und den Katalog. Effektive Stufe, Dropdown, Warnungen und `max_tokens` rechnet ausschließlich
//!   `os_ai::catalog` (`plan_task`, `choose_reasoning`, `output_limit`); die Oberfläche rechnet nichts nach.
//! - `ai.settingsSet` (JSON → JSON): `{ "ai": <Abschnitt ai eines Konfigurationsentwurfs> }` prüft den Entwurf des
//!   Konfigurationsdialogs und liefert die Aufgabenzeilen (schreibt nichts; gespeichert wird über `config.set` beim
//!   „OK“ des Dialogs); `{ "redactionPatterns": "<eine Regex je Zeile>" }` prüft die Musterliste (Zeile, Position) und
//!   speichert sie in den App-Einstellungen; `{ "raiseBudget": <BudgetLimits> }` speichert die Grenzen in der geöffneten
//!   Crawl-Datei über `os_store::ai::budget::raise_limits` („Budget erhöhen und fortsetzen“, Plan M4.md 7, Nachträge
//!   Welle 2 Punkt 2).
//!
//! Modelllisten: OpenAI-Modelle mit Fähigkeiten, Stufen und Ausgabegrenze kommen aus dem Katalog (`pricing.json`,
//! AI-010); Anthropic, OpenRouter und Ollama melden Fähigkeiten nur über ihre Live-Liste. Die Live-Listen trägt der
//! Kontenbereich (M4-P01) nach dem Verbinden in [`LiveModels`] ein; ohne Liste zeigt die Zeile die gespeicherte Stufe
//! ohne Prüfung (`levelsKnown = false`) und keine Warnung.
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use os_ai::catalog::reasoning::{choose_reasoning, output_limit, ReasoningWarning};
use os_ai::catalog::{
    plan_task, validate_assignment, AnswerTokens, AvailableModel, Endpoint, ModelRef, PriceTable,
    ProviderState, TaskDefaults, TaskSettings, TaskType,
};
use os_ai::keys::{AiKeyProvider, AiKeyStore, KeyStatus};
use os_ai::policy::redact::PatternList;
use os_ai::provider::{CapabilitySource, Model, ReasoningLevel};
use os_model::budget::BudgetLimits;
use os_model::config::{AiAppSettings, AiConfig, AiPolicy, CrawlConfig, ProviderEndpoint};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::dto::{CmdError, ErrorCode};
use crate::registry::{CmdResult, Handler};
use crate::runtime::error::{internal, parse_args, to_json};
use crate::{App, CallCtx};

/// AI-019 (Richtlinie „EU-Verarbeitung erzwingen“ / „Nur lokal“, EU-Endpunkte) in diesem Release verfügbar? In 1.1 aus:
/// Der Kern setzt die Richtlinie erst mit der 1.2-Etappe von M4-K07 durch (`release_split`); bis dahin erscheint keine Zusage
/// zu Datenort oder „nur lokal“, und `ai.settingsSet` lehnt eine andere Richtlinie als „Keine Einschränkung“ ab
/// (Entscheidung des Orchestrators, 26.11.2026). 1.2 schaltet hier ein.
pub const AI_019_ENABLED: bool = false;

/// EU-Basis-URLs laut AI-019 (vorbelegt; überschreibbar für Tests nach INTEG-019 über `base_url`).
pub const EU_ENDPOINTS: [(&str, &str); 2] = [
    ("openai", "https://eu.api.openai.com"),
    ("openrouter", "https://eu.openrouter.ai/api/v1"),
];

/// Anbieter des Bereichs in der Reihenfolge der Oberfläche.
pub const PROVIDERS: [&str; 4] = ["anthropic", "openai", "openrouter", "ollama"];

/// Bereich der Einstellung „gleichzeitige Anfragen“ im Reiter „Erweitert“ (INTEG-005).
pub const CONCURRENCY_RANGE: (u16, u16) = (1, 50);

/// Live-Modelllisten je Anbieter (eine je App; der Kontenbereich M4-P01 trägt sie nach dem Verbinden ein).
#[derive(Default)]
pub struct LiveModels(Mutex<BTreeMap<String, Vec<AvailableModel>>>);

impl LiveModels {
    /// Ersetzt die Liste eines Anbieters.
    pub fn set(&self, provider: &str, models: Vec<AvailableModel>) {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(provider.to_string(), models);
    }
    /// Alle geladenen Listen.
    pub fn all(&self) -> BTreeMap<String, Vec<AvailableModel>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// Die Live-Listen dieser App.
pub fn live_models(app: &App) -> Arc<LiveModels> {
    app.ext::<LiveModels>()
}

/// Herkunft eines wählbaren Modells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelSource {
    /// `pricing.json` (OpenAI, Fähigkeiten laut Katalog).
    Catalog,
    /// Live-Liste des Anbieters.
    Live,
    /// Nur als Standard-/Ersatzmodell bekannt; Fähigkeiten erst mit der Live-Liste.
    Defaults,
}

/// Bekannte Modelle je Anbieter (Katalog + Live-Listen) für Modellwahl und Plan.
#[derive(Debug, Clone, Default)]
pub struct ModelLists {
    /// Anbieter → Modelle mit Herkunft.
    pub by_provider: BTreeMap<String, Vec<(AvailableModel, ModelSource)>>,
}

fn catalog_model(entry: &os_ai::catalog::PriceEntry) -> Option<AvailableModel> {
    let caps = entry.capabilities?;
    Some(AvailableModel {
        model: Model {
            provider: entry.provider.clone(),
            id: entry.model.clone(),
            endpoint: entry.endpoint.as_str().to_string(),
            capabilities: caps.into(),
            capability_source: CapabilitySource::ModelCatalog,
            reasoning_levels: entry.reasoning_levels(),
        },
        max_output_tokens: entry.max_output_tokens,
    })
}

impl ModelLists {
    /// Katalogmodelle (Einträge mit Fähigkeiten) plus Live-Listen; ein Live-Eintrag ersetzt denselben Katalogeintrag.
    pub fn build(pricing: &PriceTable, live: &BTreeMap<String, Vec<AvailableModel>>) -> Self {
        let mut by_provider: BTreeMap<String, Vec<(AvailableModel, ModelSource)>> = BTreeMap::new();
        for entry in pricing.entries() {
            if let Some(m) = catalog_model(entry) {
                by_provider
                    .entry(m.model.provider.clone())
                    .or_default()
                    .push((m, ModelSource::Catalog));
            }
        }
        for (provider, models) in live {
            let list = by_provider.entry(provider.clone()).or_default();
            for m in models {
                list.retain(|(c, _)| {
                    !(c.model.id == m.model.id && c.model.endpoint == m.model.endpoint)
                });
                list.push((m.clone(), ModelSource::Live));
            }
        }
        Self { by_provider }
    }

    /// Hat dieser Anbieter eine geladene Liste (Katalog oder live)?
    pub fn list_known(&self, provider: &str) -> bool {
        self.by_provider
            .get(provider)
            .is_some_and(|l| !l.is_empty())
    }

    /// Modell zu einem Verweis.
    pub fn find(&self, r: &ModelRef) -> Option<&(AvailableModel, ModelSource)> {
        self.by_provider
            .get(&r.provider)?
            .iter()
            .find(|(m, _)| m.model.id == r.model && m.model.endpoint == r.endpoint.as_str())
    }

    /// Zustand der Anbieter für `plan_task` (Schlüssel und bekannte Modelle).
    pub fn provider_states(
        &self,
        keys: &BTreeMap<String, bool>,
    ) -> BTreeMap<String, ProviderState> {
        PROVIDERS
            .iter()
            .map(|p| {
                let models = self
                    .by_provider
                    .get(*p)
                    .map(|l| l.iter().map(|(m, _)| m.clone()).collect())
                    .unwrap_or_default();
                let has_key = keys.get(*p).copied().unwrap_or(false);
                (p.to_string(), ProviderState { has_key, models })
            })
            .collect()
    }
}

/// Ein wählbares Modell einer Aufgabe.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub provider: String,
    pub model: String,
    pub endpoint: Endpoint,
    pub source: ModelSource,
    /// Fähigkeiten bekannt (Katalog oder Live-Liste).
    pub capabilities_known: bool,
    /// Zuweisbar (AI-011: ohne benötigte Fähigkeit nicht wählbar).
    pub assignable: bool,
    /// Fehlende Fähigkeiten (`vision`, `structured_output`, …).
    pub missing: Vec<&'static str>,
    /// Verfügbare Stufen (ohne `model-default`).
    pub reasoning_levels: Vec<ReasoningLevel>,
    pub max_output_tokens: Option<u64>,
}

/// Warnung der Spalte „Reasoning“ (AI-011, Nachtrag).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WarningView {
    /// „aus nicht verfügbar – niedrigste Stufe: <effective>“.
    OffUnavailable { effective: ReasoningLevel },
    /// Gespeicherte Stufe nicht verfügbar, es gilt `effective` (der gespeicherte Wert bleibt).
    StoredUnavailable {
        stored: ReasoningLevel,
        effective: ReasoningLevel,
    },
}

impl From<ReasoningWarning> for WarningView {
    fn from(w: ReasoningWarning) -> Self {
        match w {
            ReasoningWarning::OffUnavailable { effective } => Self::OffUnavailable { effective },
            ReasoningWarning::StoredLevelUnavailable { stored, effective } => {
                Self::StoredUnavailable { stored, effective }
            }
        }
    }
}

/// Spalte „Reasoning“ einer Aufgabe.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningView {
    /// Gespeicherter Wert im Profil (`None` = Standard der Aufgabe).
    pub stored: Option<ReasoningLevel>,
    /// Gespeicherte bzw. Standardstufe.
    pub requested: ReasoningLevel,
    pub effective: ReasoningLevel,
    /// Einträge des Dropdowns (`model-default` zuerst, dann aufsteigend).
    pub dropdown: Vec<ReasoningLevel>,
    pub warning: Option<WarningView>,
    /// Stufen des Modells bekannt (sonst ohne Prüfung, keine Warnung).
    pub levels_known: bool,
}

/// Ausgabegrenze einer Aufgabe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaxTokensView {
    /// Eingestellter Nutzerwert.
    pub user: Option<u64>,
    /// Standard min(2 × A + R, Grenze); `None` bei `decision-w` (hängt von W ab).
    pub standard: Option<u64>,
    /// Gesendeter Wert (bei `decision-w` ohne Nutzerwert `None`).
    pub value: Option<u64>,
    /// Nutzerwert kleiner als der Standard → Hinweis „Ausgabegrenze knapp“.
    pub tight: bool,
    /// Antwortanteil A (`None` bei `decision-w`).
    pub answer_tokens: Option<u64>,
    /// A = 64 + 16 × W.
    pub decision_formula: bool,
    pub model_limit: Option<u64>,
}

/// Eine Zeile der Tabelle Aufgabe → Modell.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRow {
    pub task: TaskType,
    /// Gewähltes Modell im Profil (`None` = Standard).
    pub model: Option<ModelRef>,
    /// Ersatzliste im Profil (gilt nur mit gewähltem Modell, K04 Auslegung 7).
    pub fallback: Vec<ModelRef>,
    pub default_model: Option<ModelRef>,
    pub default_fallback: Vec<ModelRef>,
    /// Alles auf Standard (Modell, Ersatzliste, Stufe, Ausgabegrenze, Parameter).
    pub is_default: bool,
    /// Wirksames Modell der Zeile (gewählt bzw. Standard).
    pub effective_model: Option<ModelRef>,
    /// Wirksame Ersatzliste.
    pub effective_fallback: Vec<ModelRef>,
    /// Wählbare Modelle (inkl. der nicht zuweisbaren mit Grund).
    pub models: Vec<ModelOption>,
    /// `None` bei Decisions und Embeddings (keine Stufe).
    pub reasoning: Option<ReasoningView>,
    /// `None` bei Decisions und Embeddings.
    pub max_tokens: Option<MaxTokensView>,
    pub temperature: Option<f64>,
    pub max_input_chars: Option<u64>,
    /// Status aller Einträge, wenn kein Kandidat nutzbar ist (`Error: Missing API key`, `Skipped (no capable model)`).
    pub status: Option<String>,
    /// Modell, über das die Aufgabe jetzt liefe (`<…> Model`), falls ein Kandidat nutzbar ist.
    pub planned_model: Option<ModelRef>,
    /// Fehlende Fähigkeiten des gewählten Modells (Zuweisung ungültig, AI-011).
    pub assignment_error: Option<Vec<&'static str>>,
}

fn uses_no_level(endpoint: Endpoint) -> bool {
    matches!(endpoint, Endpoint::Decisions | Endpoint::Embeddings)
}

/// Wählbare Modelle einer Aufgabe: alle bekannten Modelle, deren Endpunkt zur Aufgabe passen kann, dazu Standard- und
/// Ersatzmodelle ohne Liste. Die Prüfung der Fähigkeiten macht `validate_assignment` (AI-011).
fn model_options(
    task: TaskType,
    defaults: &TaskDefaults,
    lists: &ModelLists,
    row_refs: &[&ModelRef],
) -> Vec<ModelOption> {
    let mut out: Vec<ModelOption> = Vec::new();
    for list in lists.by_provider.values() {
        for (m, source) in list {
            let Some(endpoint) = Endpoint::parse(&m.model.endpoint) else {
                continue;
            };
            // Embedding-Modelle nur für `embedding`, Decisions-Modelle nur für Aufgaben über die Decisions API.
            let fits = match endpoint {
                Endpoint::Embeddings => task == TaskType::Embedding,
                Endpoint::Decisions => task.may_use_decisions(),
                _ => task != TaskType::Embedding,
            };
            if !fits {
                continue;
            }
            let check = validate_assignment(task, &m.model);
            out.push(ModelOption {
                provider: m.model.provider.clone(),
                model: m.model.id.clone(),
                endpoint,
                source: *source,
                capabilities_known: true,
                assignable: check.is_ok(),
                missing: check.err().map(|e| e.missing).unwrap_or_default(),
                reasoning_levels: m.model.reasoning_levels.clone(),
                max_output_tokens: m.max_output_tokens,
            });
        }
    }
    let default = defaults.get(task);
    for r in default
        .model
        .iter()
        .chain(default.fallback.iter())
        .chain(row_refs.iter().copied())
    {
        let known = out
            .iter()
            .any(|o| o.provider == r.provider && o.model == r.model && o.endpoint == r.endpoint);
        if !known {
            out.push(ModelOption {
                provider: r.provider.clone(),
                model: r.model.clone(),
                endpoint: r.endpoint,
                source: ModelSource::Defaults,
                capabilities_known: false,
                assignable: true,
                missing: Vec::new(),
                reasoning_levels: Vec::new(),
                max_output_tokens: None,
            });
        }
    }
    out.sort_by(|a, b| {
        (&a.provider, &a.model, a.endpoint).cmp(&(&b.provider, &b.model, b.endpoint))
    });
    out
}

fn is_default(s: &TaskSettings) -> bool {
    *s == TaskSettings::default()
}

/// Die Zeile einer Aufgabe.
pub fn task_row(
    task: TaskType,
    settings: &TaskSettings,
    defaults: &TaskDefaults,
    lists: &ModelLists,
    providers: &BTreeMap<String, ProviderState>,
) -> TaskRow {
    let default = defaults.get(task);
    let effective_model = settings.model.clone().or_else(|| default.model.clone());
    let effective_fallback = if settings.model.is_some() {
        settings.fallback.clone()
    } else {
        default.fallback.clone()
    };
    let row_refs: Vec<&ModelRef> = settings
        .model
        .iter()
        .chain(settings.fallback.iter())
        .collect();
    let requested = settings
        .reasoning
        .unwrap_or_else(|| defaults.reasoning(task));
    let found = effective_model.as_ref().and_then(|r| lists.find(r));
    let assignment_error = settings
        .model
        .as_ref()
        .and_then(|r| lists.find(r))
        .and_then(|(m, _)| validate_assignment(task, &m.model).err())
        .map(|e| e.missing);

    let level_free = effective_model
        .as_ref()
        .map_or(task == TaskType::Embedding, |r| uses_no_level(r.endpoint));
    let reasoning = (!level_free).then(|| match found {
        Some((m, _)) => {
            let choice = choose_reasoning(requested, &m.model.reasoning_levels);
            ReasoningView {
                stored: settings.reasoning,
                requested,
                effective: choice.effective,
                dropdown: choice.dropdown,
                warning: choice.warning.map(WarningView::from),
                levels_known: true,
            }
        }
        None => {
            let mut dropdown = vec![ReasoningLevel::ModelDefault];
            if requested != ReasoningLevel::ModelDefault {
                dropdown.push(requested);
            }
            ReasoningView {
                stored: settings.reasoning,
                requested,
                effective: requested,
                dropdown,
                warning: None,
                levels_known: false,
            }
        }
    });

    let model_limit = found.and_then(|(m, _)| m.max_output_tokens);
    let max_tokens = match (&reasoning, defaults.answer_tokens(task)) {
        (Some(r), Some(answer)) => {
            let fixed = match answer {
                AnswerTokens::Fixed(a) => Some(a),
                AnswerTokens::Formula(_) => None,
            };
            let standard =
                fixed.map(|a| os_ai::catalog::default_max_tokens(a, r.effective, model_limit));
            let (value, tight) = match standard {
                Some(s) => {
                    let l = output_limit(s, settings.max_tokens);
                    (Some(l.value), l.tight)
                }
                None => (settings.max_tokens, false),
            };
            Some(MaxTokensView {
                user: settings.max_tokens,
                standard,
                value,
                tight,
                answer_tokens: fixed,
                decision_formula: fixed.is_none(),
                model_limit,
            })
        }
        _ => None,
    };

    let (status, planned_model) = match plan_task(task, settings, defaults, providers) {
        Ok(plan) => (None, Some(plan.model_ref)),
        Err(status) => (Some(status.to_string()), None),
    };

    TaskRow {
        task,
        model: settings.model.clone(),
        fallback: settings.fallback.clone(),
        default_model: default.model.clone(),
        default_fallback: default.fallback.clone(),
        is_default: is_default(settings),
        effective_model,
        effective_fallback,
        models: model_options(task, defaults, lists, &row_refs),
        reasoning,
        max_tokens,
        temperature: settings.temperature,
        max_input_chars: settings.max_input_chars,
        status,
        planned_model,
        assignment_error,
    }
}

/// Alle zwölf Aufgaben eines `ai`-Abschnitts; unbekannte Aufgabentypen oder Stufen im Profil sind ein Fehler.
pub fn task_rows(
    ai: &AiConfig,
    defaults: &TaskDefaults,
    lists: &ModelLists,
    keys: &BTreeMap<String, bool>,
) -> Result<Vec<TaskRow>, CmdError> {
    let settings = os_ai::catalog::choice::settings_from_profile(&ai.tasks).map_err(|e| {
        CmdError::invalid_input("err-ai-task-settings", "ai.tasks").with_detail(e.to_string())
    })?;
    let providers = lists.provider_states(keys);
    Ok(TaskType::ALL
        .iter()
        .map(|t| {
            let s = settings.get(t).cloned().unwrap_or_default();
            task_row(*t, &s, defaults, lists, &providers)
        })
        .collect())
}

/// Vorgaben des Reiters „Erweitert“ je KI-Anbieter (INTEG-005 [M4]): Standard aus den App-Vorgaben (Ollama 1, AI-006),
/// Bereich 1–50, und der eingestellte Wert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedRow {
    pub provider: String,
    pub concurrent_default: u16,
    pub concurrent_min: u16,
    pub concurrent_max: u16,
    pub concurrent: u16,
    pub timeout_default: u16,
    pub timeout: u16,
}

/// Zeilen des Reiters „Erweitert“.
pub fn advanced_rows(app: &AiAppSettings) -> Vec<AdvancedRow> {
    let defaults = AiAppSettings::default();
    PROVIDERS
        .iter()
        .map(|p| {
            let d = defaults
                .provider_endpoints
                .get(*p)
                .cloned()
                .unwrap_or_default();
            let cur = app
                .provider_endpoints
                .get(*p)
                .cloned()
                .unwrap_or_else(|| d.clone());
            AdvancedRow {
                provider: p.to_string(),
                concurrent_default: d.concurrent_requests,
                concurrent_min: CONCURRENCY_RANGE.0,
                concurrent_max: CONCURRENCY_RANGE.1,
                concurrent: cur.concurrent_requests,
                timeout_default: d.timeout_seconds,
                timeout: cur.timeout_seconds,
            }
        })
        .collect()
}

/// EU-Schalter eines Anbieters (AI-019).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EuRow {
    pub provider: String,
    pub eu_base_url: String,
    pub use_eu_endpoint: bool,
    pub eu_account_confirmed: bool,
    /// Unter „EU erzwingen“ zulässig (Schalter an und Konto bestätigt).
    pub allowed_under_eu: bool,
}

/// EU-Zeilen (OpenAI, OpenRouter).
pub fn eu_rows(app: &AiAppSettings) -> Vec<EuRow> {
    EU_ENDPOINTS
        .iter()
        .map(|(p, url)| {
            let e: ProviderEndpoint = app.provider_endpoints.get(*p).cloned().unwrap_or_default();
            EuRow {
                provider: p.to_string(),
                eu_base_url: url.to_string(),
                use_eu_endpoint: e.use_eu_endpoint,
                eu_account_confirmed: e.eu_account_confirmed,
                allowed_under_eu: e.use_eu_endpoint && e.eu_account_confirmed,
            }
        })
        .collect()
}

/// Sicht `ai.settingsGet`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub preview_enabled: bool,
    /// Anbieter → `present` | `missing` | `keychain-unavailable` | `local` (Ollama).
    pub keys: BTreeMap<String, String>,
    /// Anbieter mit geladener Modellliste (Katalog oder live).
    pub lists_loaded: BTreeMap<String, bool>,
    pub tasks: Vec<TaskRow>,
    pub policy: AiPolicy,
    /// Richtlinie und EU-Endpunkte (AI-019) wählbar ([`AI_019_ENABLED`]); sonst sind `eu` leer und die Oberfläche ohne Schalter.
    pub policy_available: bool,
    /// Grenzen im Profil des Fensters (`ai.budget`).
    pub budget: BudgetLimits,
    /// Grenzen der geöffneten Crawl-Datei (`api_budget`), falls dort gespeichert.
    pub crawl_budget: Option<BudgetLimits>,
    pub eu: Vec<EuRow>,
    pub advanced: Vec<AdvancedRow>,
    pub confirmation_threshold_micro_usd: u64,
    pub redact_personal_data: bool,
    pub redaction_patterns: Vec<String>,
}

/// Schlüsselzustand je Anbieter ohne Wert.
pub fn key_states(store: &AiKeyStore) -> (BTreeMap<String, String>, BTreeMap<String, bool>) {
    let mut text = BTreeMap::new();
    let mut has = BTreeMap::new();
    for p in AiKeyProvider::ALL {
        let (t, h) = match store.status(p) {
            KeyStatus::Present { .. } => ("present", true),
            KeyStatus::Missing => ("missing", false),
            KeyStatus::KeychainUnavailable { .. } => ("keychain-unavailable", false),
        };
        text.insert(p.id().to_string(), t.to_string());
        has.insert(p.id().to_string(), h);
    }
    text.insert("ollama".into(), "local".into());
    // Ollama braucht keinen Schlüssel; nutzbar ist es mit geladener Liste (AI-006).
    has.insert("ollama".into(), true);
    (text, has)
}

/// Baut die Sicht (reine Funktion; Tests rufen sie mit Fixtures auf).
#[allow(clippy::too_many_arguments)]
pub fn settings_view(
    app: &AiAppSettings,
    config: &CrawlConfig,
    crawl_budget: Option<BudgetLimits>,
    defaults: &TaskDefaults,
    lists: &ModelLists,
    keys: (BTreeMap<String, String>, BTreeMap<String, bool>),
) -> Result<SettingsView, CmdError> {
    let (key_text, has_key) = keys;
    Ok(SettingsView {
        preview_enabled: app.preview_enabled,
        lists_loaded: PROVIDERS
            .iter()
            .map(|p| (p.to_string(), lists.list_known(p)))
            .collect(),
        tasks: task_rows(&config.ai, defaults, lists, &has_key)?,
        keys: key_text,
        policy: config.ai.policy.clone(),
        policy_available: AI_019_ENABLED,
        budget: config.ai.budget.clone(),
        crawl_budget,
        eu: if AI_019_ENABLED {
            eu_rows(app)
        } else {
            Vec::new()
        },
        advanced: advanced_rows(app),
        confirmation_threshold_micro_usd: app.confirmation_threshold.0,
        redact_personal_data: app.redact_personal_data,
        redaction_patterns: app.redaction_patterns.clone(),
    })
}

fn catalog_error(e: os_ai::catalog::CatalogError) -> CmdError {
    internal("ai.settings", e.to_string())
}

/// Konfiguration des Fensters über `config.get` (geöffneter bzw. laufender Crawl, sonst Entwurf, sonst Standardprofil).
pub(crate) async fn window_config(app: &App, ctx: &CallCtx) -> Result<CrawlConfig, CmdError> {
    let dto = app.dispatch(ctx.clone(), "config.get", Value::Null).await?;
    serde_json::from_value(dto.get("config").cloned().unwrap_or(Value::Null))
        .map_err(|e| internal("ai.settings", e.to_string()))
}

/// Grenzen der geöffneten Crawl-Datei, falls gespeichert.
async fn crawl_limits(app: &App, ctx: &CallCtx) -> Result<Option<BudgetLimits>, CmdError> {
    let Some(session) = app.sessions().current(&ctx.window) else {
        return Ok(None);
    };
    app.read_pool()
        .run(session.key.clone(), |file, _| {
            os_store::ai::budget::load_limits(file.conn())
        })
        .await
}

fn lists_for(app: &App) -> Result<ModelLists, CmdError> {
    let pricing = PriceTable::bundled().map_err(catalog_error)?;
    Ok(ModelLists::build(&pricing, &live_models(app).all()))
}

fn settings_get<'a>(app: &'a App, ctx: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let config = window_config(app, &ctx).await?;
        let crawl_budget = crawl_limits(app, &ctx).await.unwrap_or(None);
        let defaults = TaskDefaults::bundled().map_err(catalog_error)?;
        let lists = lists_for(app)?;
        let keys = key_states(&AiKeyStore::new(app.config().secrets.clone()));
        let view = settings_view(
            &app.settings_snapshot().ai_app,
            &config,
            crawl_budget,
            &defaults,
            &lists,
            keys,
        )?;
        to_json(&view)
    })
}

/// Argumente von `ai.settingsSet` (genau eine Form je Aufruf).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsSetArgs {
    /// Abschnitt `ai` eines Konfigurationsentwurfs: prüfen und Zeilen liefern.
    pub ai: Option<Value>,
    /// Schwärzungsmuster, eine Regex je Zeile (AI-024): prüfen und speichern.
    pub redaction_patterns: Option<String>,
    /// Neue Grenzen der geöffneten Crawl-Datei („Budget erhöhen und fortsetzen“).
    pub raise_budget: Option<BudgetLimits>,
}

/// Prüft die Zuweisungen eines Entwurfs: ein Modell ohne benötigte Fähigkeit (AI-011) ist ein Fehler mit Feld
/// `ai.tasks.<aufgabe>.model`.
pub fn check_assignments(rows: &[TaskRow]) -> Result<(), CmdError> {
    for row in rows {
        if let Some(missing) = &row.assignment_error {
            return Err(CmdError::invalid_input(
                "err-ai-model-incapable",
                format!("ai.tasks.{}.model", row.task.as_str()),
            )
            .with_arg("task", row.task.as_str())
            .with_arg("missing", json!(missing)));
        }
    }
    Ok(())
}

/// In 1.1 ([`AI_019_ENABLED`] aus) ist nur „Keine Einschränkung“ zulässig: eine andere Richtlinie würde eine Zusage machen,
/// die kein Lauf durchsetzt (`InvalidInput`, `err-ai-policy-unavailable`, Feld `ai.policy`).
pub fn check_policy(policy: &AiPolicy) -> Result<(), CmdError> {
    if AI_019_ENABLED || *policy == AiPolicy::Unrestricted {
        return Ok(());
    }
    Err(
        CmdError::invalid_input("err-ai-policy-unavailable", "ai.policy")
            .with_arg(
                "policy",
                serde_json::to_value(policy).unwrap_or(Value::Null),
            )
            .with_detail("AI-019 (EU processing, local only) is not enforced in this release"),
    )
}

/// Prüft eine Musterliste; Fehler mit Zeile und Position (AI-024).
pub fn check_patterns(text: &str) -> Result<Vec<String>, CmdError> {
    PatternList::parse(text).map_err(|e| {
        CmdError::invalid_input("err-ai-pattern-invalid", "aiApp.redactionPatterns")
            .with_arg("line", e.line)
            .with_arg("position", e.position)
            .with_detail(e.message.clone())
    })?;
    Ok(text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect())
}

fn settings_set<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: SettingsSetArgs = parse_args(args)?;
        if let Some(ai) = args.ai {
            let ai: AiConfig = serde_json::from_value(ai)
                .map_err(|_| CmdError::invalid_input("err-config-type", "ai"))?;
            check_policy(&ai.policy)?;
            let defaults = TaskDefaults::bundled().map_err(catalog_error)?;
            let lists = lists_for(app)?;
            let (_, has) = key_states(&AiKeyStore::new(app.config().secrets.clone()));
            let rows = task_rows(&ai, &defaults, &lists, &has)?;
            check_assignments(&rows)?;
            return to_json(&json!({ "tasks": rows }));
        }
        if let Some(text) = args.redaction_patterns {
            let patterns = check_patterns(&text)?;
            app.dispatch(
                ctx.clone(),
                "settings.set",
                json!({ "patch": { "aiApp": { "redactionPatterns": patterns } } }),
            )
            .await?;
            return to_json(&json!({ "redactionPatterns": patterns }));
        }
        if let Some(limits) = args.raise_budget {
            let Some(session) = app.sessions().current(&ctx.window) else {
                return Err(CmdError::new(ErrorCode::NoCrawl, "err-no-crawl"));
            };
            if !session.writable {
                return Err(CmdError::new(ErrorCode::Permission, "err-db-read-only"));
            }
            let fallback = session.config.ai.budget.clone();
            let lifted = app
                .write_queue()
                .submit(session.key.clone(), move |conn| {
                    let ledger = os_store::ai::budget::open_ledger(conn, fallback, &[])?;
                    os_store::ai::budget::raise_limits(conn, &ledger, limits)
                })
                .await?;
            return to_json(&json!({ "lifted": lifted }));
        }
        Err(CmdError::invalid_input("err-ai-settings-args", "args"))
    })
}

/// Handler der U01-Commands dieses Bereichs (Eintrag im Katalog `ai_catalog`, M4-F02).
pub(crate) fn handler(name: &str) -> Option<Handler> {
    match name {
        "ai.settingsGet" => Some(settings_get),
        "ai.settingsSet" => Some(settings_set),
        _ => None,
    }
}

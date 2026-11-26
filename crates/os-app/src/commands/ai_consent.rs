//! Datenflussübersicht und Einwilligung in der Service-Schicht (M4-U01, AI-018; Richtlinie und Region AI-019).
//!
//! Die Einwilligung gilt **je Installation** (Nutzerentscheidung 24.11.2026, Plan M4.md 7): [`FileConsentStore`] legt
//! das `ConsentBook` von `os_ai::policy` als `ai-consent.json` im Einstellungsordner der App ab; [`consent_gate`]
//! liefert das eine, geteilte `ConsentGate` der App (Widerruf wirkt vor dem nächsten Request jedes Workers). Die
//! Ausführung (D01) prüft über dasselbe Gate in `policy::guard::execute`.
//!
//! Commands (Vertrag aus `ai_catalog`):
//! - `ai.consentStatus` (void → JSON [`ConsentView`]): Zeilen der Übersicht „KI-Datenfluss“ für die konfigurierten
//!   Aufgaben des Fensters, die offenen Anfragen des Dialogs „Datenfluss bestätigen“ (fehlende Einwilligung oder neues
//!   Feld), Stand der Schwärzung, Richtlinie und der CSV-Export.
//! - `ai.consentGrant` (JSON → JSON [`ConsentView`]): `{ "grant": [{ "provider", "task", "fields": [..] }] }` bestätigt
//!   die gezeigten Zeilen (mit der Zeile im Protokoll), `{ "revokeProvider": "<anbieter>" }` widerruft je Anbieter.
//!
//! Welche Felder eine Aufgabe sendet, wissen die Fachpakete: Fragen des Profils (`ai.questions`, Feld `inputs`, AI-030)
//! liest dieses Modul selbst; weitere Aufgaben melden ihre Feldmengen über [`register_flow_source`] an.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use os_ai::catalog::{plan_task, ModelRef, TaskDefaults, TaskSettings, TaskType};
use os_ai::keys::AiKeyStore;
use os_ai::policy::{
    ConsentBook, ConsentGate, ConsentKey, ConsentStore, DataFlowOverview, DataFlowRow, FieldSet,
    ProviderPolicies, TaskDataFlow, COLUMNS,
};
use os_model::config::{AiAppSettings, AiPolicy, CrawlConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ai_settings::{key_states, live_models, window_config, ModelLists, EU_ENDPOINTS};
use crate::dto::{CmdError, ErrorCode};
use crate::registry::{CmdResult, Handler};
use crate::runtime::error::{internal, parse_args, to_json};
use crate::{App, CallCtx};

/// Dateiname des Einwilligungsbuchs im Einstellungsordner (je Installation).
pub const CONSENT_FILE: &str = "ai-consent.json";

/// Standard-Basis-URLs der Anbieter (Konstanten der Adapter).
pub fn default_base_url(provider: &str) -> &'static str {
    match provider {
        "anthropic" => os_ai::adapters::anthropic::DEFAULT_BASE_URL,
        "openai" => os_ai::adapters::openai_responses::client::DEFAULT_BASE_URL,
        "openrouter" => os_ai::adapters::openrouter::DEFAULT_BASE_URL,
        "ollama" => os_ai::adapters::ollama::DEFAULT_BASE_URL,
        _ => "",
    }
}

/// Basis-URL und Region eines Anbieters laut App-Einstellungen: eigene `base_url` (Tests, INTEG-019) vor dem
/// EU-Endpunkt (Schalter „EU-Endpunkt verwenden“, AI-019) vor dem Standard. Region `EU` nur über den EU-Endpunkt.
pub fn endpoint_of(provider: &str, app: &AiAppSettings) -> (String, Option<String>) {
    let e = app
        .provider_endpoints
        .get(provider)
        .cloned()
        .unwrap_or_default();
    let eu = EU_ENDPOINTS
        .iter()
        .find(|(p, _)| *p == provider)
        .map(|(_, url)| *url);
    match (e.base_url, eu) {
        (Some(url), _) if !url.trim().is_empty() => {
            let region = (e.use_eu_endpoint && eu.is_some()).then(|| "EU".to_string());
            (url, region)
        }
        (_, Some(url)) if e.use_eu_endpoint => (url.to_string(), Some("EU".to_string())),
        _ => (default_base_url(provider).to_string(), None),
    }
}

/// Feldmenge einer Aufgabe (Eingabe der Übersicht).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFields {
    pub task: TaskType,
    pub fields: FieldSet,
    /// Höchstzahl Zeichen von `page_text`.
    pub max_chars: Option<u32>,
}

/// Quelle von Feldmengen eines Fachpakets.
pub type FlowSource = Arc<dyn Fn(&CrawlConfig) -> Vec<TaskFields> + Send + Sync>;

/// Angemeldete Quellen dieser App.
#[derive(Default)]
pub struct FlowSources(Mutex<Vec<FlowSource>>);

/// Meldet eine Quelle von Feldmengen an (z. B. Vorschläge V01, Fragenbibliothek D01).
pub fn register_flow_source(app: &App, source: FlowSource) {
    app.ext::<FlowSources>()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push(source);
}

/// Feldmengen der aktiven Fragen aus dem Profil (`ai.questions` mit `inputs`, AI-030/AI-032): Seitenfragen →
/// `decision`, Fragen mit `subject = image` → `decision-vision`. Fragen ohne gültige `inputs` fehlen (ihre Felder kennt
/// erst die Fragenbibliothek, D01). Nur bei `ai.enabled`.
pub fn profile_question_fields(config: &CrawlConfig) -> Vec<TaskFields> {
    if !config.ai.enabled {
        return Vec::new();
    }
    let mut by_task: BTreeMap<TaskType, FieldSet> = BTreeMap::new();
    for q in &config.ai.questions {
        let Some(id) = q.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !config.ai.active_questions.iter().any(|a| a == id) {
            continue;
        }
        let Some(inputs) = q.get("inputs").and_then(Value::as_array) else {
            continue;
        };
        let ids: Vec<&str> = inputs.iter().filter_map(Value::as_str).collect();
        let Ok(fields) = FieldSet::from_ids(ids) else {
            continue;
        };
        let task = if q.get("subject").and_then(Value::as_str) == Some("image") {
            TaskType::DecisionVision
        } else {
            TaskType::Decision
        };
        let set = by_task.entry(task).or_default();
        for f in fields.iter() {
            set.insert(f);
        }
    }
    by_task
        .into_iter()
        .map(|(task, fields)| TaskFields {
            task,
            fields,
            max_chars: None,
        })
        .collect()
}

/// Führt Feldmengen derselben Aufgabe zusammen (Vereinigung, größte Zeichengrenze).
pub fn merge_fields(items: Vec<TaskFields>) -> Vec<TaskFields> {
    let mut map: BTreeMap<TaskType, TaskFields> = BTreeMap::new();
    for item in items {
        match map.get_mut(&item.task) {
            Some(cur) => {
                for f in item.fields.iter() {
                    cur.fields.insert(f);
                }
                cur.max_chars = match (cur.max_chars, item.max_chars) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
            }
            None => {
                map.insert(item.task, item);
            }
        }
    }
    map.into_values().filter(|t| !t.fields.is_empty()).collect()
}

/// Modell einer Aufgabe für die Übersicht: das, über das sie jetzt liefe, sonst der erste Kandidat (gewählt bzw.
/// Standard); `None` ohne Kandidaten.
pub fn flow_model(
    task: TaskType,
    settings: &TaskSettings,
    defaults: &TaskDefaults,
    lists: &ModelLists,
    has_key: &BTreeMap<String, bool>,
) -> Option<ModelRef> {
    match plan_task(task, settings, defaults, &lists.provider_states(has_key)) {
        Ok(plan) => Some(plan.model_ref),
        Err(_) => settings
            .model
            .clone()
            .or_else(|| defaults.get(task).model.clone()),
    }
}

/// Eingaben der Übersicht je Aufgabe (`TaskDataFlow` mit Endpunktart `api`, damit `Stored by Provider` für OpenAI
/// Responses `no (store: false)` zeigt; Abnahme K10, Plan M4.md 7).
pub fn task_flows(
    config: &CrawlConfig,
    app: &AiAppSettings,
    fields: &[TaskFields],
    defaults: &TaskDefaults,
    lists: &ModelLists,
    has_key: &BTreeMap<String, bool>,
) -> Result<Vec<TaskDataFlow>, CmdError> {
    let settings =
        os_ai::catalog::choice::settings_from_profile(&config.ai.tasks).map_err(|e| {
            CmdError::invalid_input("err-ai-task-settings", "ai.tasks").with_detail(e.to_string())
        })?;
    Ok(fields
        .iter()
        .filter_map(|tf| {
            let s = settings.get(&tf.task).cloned().unwrap_or_default();
            let model = flow_model(tf.task, &s, defaults, lists, has_key)?;
            let (endpoint, region) = endpoint_of(&model.provider, app);
            Some(TaskDataFlow {
                task: tf.task.as_str().to_string(),
                provider: model.provider.clone(),
                model: model.model.clone(),
                endpoint,
                region,
                fields: tf.fields.clone(),
                max_chars: tf.max_chars,
                api: Some(model.endpoint.as_str().to_string()),
            })
        })
        .collect())
}

/// Schlüssel einer Zeile in der Form der Oberfläche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyView {
    pub provider: String,
    pub task: String,
    pub fields: Vec<String>,
}

impl From<&ConsentKey> for KeyView {
    fn from(k: &ConsentKey) -> Self {
        Self {
            provider: k.provider.clone(),
            task: k.task.clone(),
            fields: k.fields.ids().into_iter().map(str::to_string).collect(),
        }
    }
}

/// Eine Zeile der Datenflusstabelle (Zellen in der Reihenfolge von `COLUMNS`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowView {
    pub key: KeyView,
    pub cells: Vec<String>,
    pub pii_notice: bool,
    pub consent_given: bool,
}

impl RowView {
    fn of(row: &DataFlowRow) -> Self {
        Self {
            key: KeyView::from(&row.consent_key()),
            cells: row.cells().to_vec(),
            pii_notice: row.pii_notice(),
            consent_given: row.consent_given_at_ms.is_some(),
        }
    }
}

/// Eine offene Anfrage des Dialogs „Datenfluss bestätigen“.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestView {
    pub row: RowView,
    /// Felder der früheren Einwilligung (Kennungen), falls vorhanden.
    pub previous_fields: Option<Vec<String>>,
    /// Neue Felder (Kennungen; beim ersten Mal alle).
    pub added_fields: Vec<String>,
    pub pii_notice: bool,
}

/// Antwort von `ai.consentStatus` / `ai.consentGrant`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentView {
    pub columns: Vec<String>,
    pub rows: Vec<RowView>,
    pub requests: Vec<RequestView>,
    /// Stand der Schwärzung (AI-024).
    pub redaction: bool,
    /// Richtlinie (AI-019).
    pub policy: AiPolicy,
    /// Anbieter mit gespeicherter Einwilligung (für „Widerruf je Anbieter“).
    pub consented_providers: Vec<String>,
    /// Export der Übersicht (RFC 4180).
    pub csv: String,
}

/// Baut die Sicht (reine Funktion).
pub fn consent_view(
    flows: &[TaskDataFlow],
    policies: &ProviderPolicies,
    gate: &ConsentGate,
    redaction: bool,
    policy: AiPolicy,
) -> ConsentView {
    let overview = DataFlowOverview::build(flows, policies, gate, redaction);
    let requests = DataFlowOverview::consent_requests(flows, policies, gate, redaction)
        .into_iter()
        .map(|r| RequestView {
            row: RowView::of(&r.row),
            previous_fields: r
                .previous_fields
                .map(|f| f.ids().into_iter().map(str::to_string).collect()),
            added_fields: r
                .added_fields
                .ids()
                .into_iter()
                .map(str::to_string)
                .collect(),
            pii_notice: r.pii_notice,
        })
        .collect();
    let mut consented: Vec<String> = gate
        .book()
        .records()
        .iter()
        .map(|r| r.key.provider.clone())
        .collect();
    consented.sort();
    consented.dedup();
    ConsentView {
        columns: COLUMNS.iter().map(|c| c.to_string()).collect(),
        rows: overview.rows.iter().map(RowView::of).collect(),
        requests,
        redaction,
        policy,
        consented_providers: consented,
        csv: overview.to_csv(),
    }
}

/// Einwilligungsbuch als Datei (eine je Installation); schreibt über eine temporäre Datei und Umbenennen.
pub struct FileConsentStore {
    path: PathBuf,
}

impl FileConsentStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl ConsentStore for FileConsentStore {
    fn load(&self) -> Result<ConsentBook, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => ConsentBook::from_json(&text).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConsentBook::new()),
            Err(e) => Err(e.to_string()),
        }
    }
    fn save(&self, book: &ConsentBook) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, book.to_json()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

#[derive(Default)]
struct GateSlot(Mutex<Option<ConsentGate>>);

/// Das geteilte Einwilligungs-Gate dieser App (geladen aus `ai-consent.json` im Einstellungsordner).
pub fn consent_gate(app: &App) -> Result<ConsentGate, CmdError> {
    let slot = app.ext::<GateSlot>();
    let mut guard = slot.0.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(gate) = guard.as_ref() {
        return Ok(gate.clone());
    }
    let store = FileConsentStore::new(app.config().paths.settings.join(CONSENT_FILE));
    let gate = ConsentGate::with_store(Arc::new(store))
        .map_err(|e| CmdError::new(ErrorCode::Io, "err-ai-consent-store").with_detail(e))?;
    *guard = Some(gate.clone());
    Ok(gate)
}

async fn window_flows(
    app: &App,
    ctx: &CallCtx,
) -> Result<(CrawlConfig, Vec<TaskDataFlow>), CmdError> {
    let config = window_config(app, ctx).await?;
    let settings = app.settings_snapshot().ai_app;
    let mut fields = profile_question_fields(&config);
    let sources: Vec<FlowSource> = app
        .ext::<FlowSources>()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    for s in sources {
        fields.extend(s(&config));
    }
    let fields = merge_fields(fields);
    let defaults = TaskDefaults::bundled().map_err(|e| internal("ai.consent", e.to_string()))?;
    let pricing =
        os_ai::catalog::PriceTable::bundled().map_err(|e| internal("ai.consent", e.to_string()))?;
    let lists = ModelLists::build(&pricing, &live_models(app).all());
    let (_, has) = key_states(&AiKeyStore::new(app.config().secrets.clone()));
    let flows = task_flows(&config, &settings, &fields, &defaults, &lists, &has)?;
    Ok((config, flows))
}

async fn status_of(app: &App, ctx: &CallCtx) -> Result<(ConsentView, Vec<TaskDataFlow>), CmdError> {
    let (config, flows) = window_flows(app, ctx).await?;
    let gate = consent_gate(app)?;
    let redaction = app.settings_snapshot().ai_app.redact_personal_data;
    let view = consent_view(
        &flows,
        &ProviderPolicies::embedded(),
        &gate,
        redaction,
        config.ai.policy.clone(),
    );
    Ok((view, flows))
}

fn consent_status<'a>(app: &'a App, ctx: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move { to_json(&status_of(app, &ctx).await?.0) })
}

/// Argumente von `ai.consentGrant`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantArgs {
    #[serde(default)]
    pub grant: Vec<KeyView>,
    pub revoke_provider: Option<String>,
}

fn now_ms(app: &App) -> i64 {
    let nanos = app.config().clock.now().0.unix_timestamp_nanos();
    i64::try_from(nanos / 1_000_000).unwrap_or(i64::MAX)
}

/// Bestätigt `keys` mit der jeweils gezeigten Zeile; ein Schlüssel ohne passende Zeile der aktuellen Übersicht wird
/// abgelehnt (es wird nur bestätigt, was der Dialog zeigt).
pub fn grant_keys(
    gate: &ConsentGate,
    flows: &[TaskDataFlow],
    policies: &ProviderPolicies,
    redaction: bool,
    keys: &[KeyView],
    at_ms: i64,
) -> Result<(), CmdError> {
    for k in keys {
        let fields = FieldSet::from_ids(k.fields.iter().map(String::as_str))
            .map_err(|_| CmdError::invalid_input("err-ai-consent-fields", "grant.fields"))?;
        let key = ConsentKey::new(&k.provider, &k.task, fields);
        let Some(flow) = flows.iter().find(|f| f.consent_key() == key) else {
            return Err(CmdError::invalid_input("err-ai-consent-unknown", "grant")
                .with_arg("task", k.task.clone())
                .with_arg("provider", k.provider.clone()));
        };
        let row = DataFlowRow::new(flow, policies, redaction);
        gate.grant(key, at_ms, Some(row))
            .map_err(|e| CmdError::new(ErrorCode::Io, "err-ai-consent-store").with_detail(e))?;
    }
    Ok(())
}

fn consent_grant<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: GrantArgs = parse_args(args)?;
        let gate = consent_gate(app)?;
        let (_, flows) = status_of(app, &ctx).await?;
        let redaction = app.settings_snapshot().ai_app.redact_personal_data;
        let policies = ProviderPolicies::embedded();
        grant_keys(
            &gate,
            &flows,
            &policies,
            redaction,
            &args.grant,
            now_ms(app),
        )?;
        if let Some(provider) = args.revoke_provider {
            gate.revoke_provider(&provider)
                .map_err(|e| CmdError::new(ErrorCode::Io, "err-ai-consent-store").with_detail(e))?;
        }
        to_json(&status_of(app, &ctx).await?.0)
    })
}

/// Handler der U01-Commands dieses Bereichs (Eintrag im Katalog `ai_catalog`, M4-F02).
pub(crate) fn handler(name: &str) -> Option<Handler> {
    match name {
        "ai.consentStatus" => Some(consent_status),
        "ai.consentGrant" => Some(consent_grant),
        _ => None,
    }
}

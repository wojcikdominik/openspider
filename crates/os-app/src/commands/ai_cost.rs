//! Kosten der KI-Vorschau in der Service-Schicht (M4-U01): Statusleistenfeld `Kosten` (UI-013 [M4]) und der
//! gemeinsame Dialog „Kostenvoranschlag“ (AI-012) mit Abschnitt „KI“; den Abschnitt „DataForSEO“ ergänzt M5-U01 als
//! weiteren Eintrag in [`CostEstimateView::sections`].
//!
//! Commands (Vertrag aus `ai_catalog`):
//! - `ai.costBudget` (void → `AiCostSnapshot`): Stand des gemeinsamen `BudgetLedger` der geöffneten Crawl-Datei, nur
//!   über `os_store::ai::budget::open_ledger` (Plan M4.md 7, Nachträge Welle 2 Punkt 2); ohne Crawl die Grenzen der
//!   Konfiguration des Fensters ohne Verbrauch.
//! - `ai.costEstimate` (`AiRunArgs` → JSON [`CostEstimateView`]): Zeilen der angemeldeten Schätzer ([`Estimators`]; die
//!   Fachpakete der Aufgaben melden je Aufgabe ihren Schätzer an, weil nur sie die aufbereiteten Eingaben kennen,
//!   AI-032), Summenzeile, Crawl-Budget, Verbrauch, Rest und die Bestätigungsregel (`cost::confirmation_required`).
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use os_ai::cost::{confirmation_required, EstimateTotals, RequestTaskEstimate, TaskEstimate};
use os_ai::provider::ReasoningLevel;
use os_model::budget::{BudgetLedger, BudgetLimits, BudgetSnapshot, MicroUsd};
use serde::Serialize;
use serde_json::Value;

use crate::dto::ai::{AiCostSnapshot, AiRunArgs};
use crate::dto::CmdError;
use crate::registry::{CmdResult, Handler};
use crate::runtime::error::{internal, parse_args, to_json};
use crate::{App, CallCtx};

/// Bereichsname des KI-Verbrauchs im Ledger.
pub const AREA_AI: &str = "ai";
/// Bereichsname von DataForSEO (ab M5).
pub const AREA_DATAFORSEO: &str = "dataforseo";

/// Snapshot des Ledgers → DTO des Statusleistenfelds. `spent` ist der Verbrauch im Sinne von AI-013 (Protokolle aller
/// Bereiche + offene Reservierungen); der Tooltip zeigt die Aufteilung KI / DataForSEO / offene Reservierungen.
pub fn cost_snapshot(s: &BudgetSnapshot) -> AiCostSnapshot {
    AiCostSnapshot {
        spent_micro_usd: s.consumption().0,
        budget_micro_usd: (!s.limits.unlimited).then_some(s.limits.crawl.0),
        ai_micro_usd: s.area_spent(AREA_AI).0,
        data_for_seo_micro_usd: s.area_spent(AREA_DATAFORSEO).0,
        reserved_micro_usd: s.reserved.0,
    }
}

/// Snapshot ohne Crawl-Datei: Grenzen der Konfiguration, kein Verbrauch.
pub fn snapshot_without_crawl(limits: BudgetLimits) -> BudgetSnapshot {
    let remaining = (!limits.unlimited).then_some(limits.crawl);
    BudgetSnapshot {
        limits,
        spent: MicroUsd(0),
        reserved: MicroUsd(0),
        areas: Default::default(),
        remaining,
        warning: false,
        halted: false,
    }
}

/// Ledger-Stand des Fensters: geöffnete Crawl-Datei über `open_ledger` (Fallback-Grenzen = `ai.budget` des Crawls),
/// sonst die Grenzen der Fensterkonfiguration ohne Verbrauch.
pub async fn window_snapshot(app: &App, ctx: &CallCtx) -> Result<BudgetSnapshot, CmdError> {
    // M4-D05: Schätzer und Feldmengen der Decision-Fragen sind angemeldet, bevor der Voranschlag sie sammelt.
    super::ai_run::ensure_registered(app);
    match app.sessions().current(&ctx.window) {
        // M4-D05 (Befund B3-2 der Abnahme U01): Läuft ein KI-Lauf auf der Datei, ist sein Ledger die Quelle – mit den
        // offenen Reservierungen seiner laufenden Requests (eine gemeinsame Quelle für Lauf und Anzeige, AI-013).
        Some(session)
            if os_ai::run::controls()
                .active_ledger(&session.path)
                .is_some() =>
        {
            let ledger = os_ai::run::controls()
                .active_ledger(&session.path)
                .ok_or_else(|| internal("ai ledger", "KI-Lauf beendet"))?;
            ledger
                .snapshot()
                .map_err(|e| internal("ai ledger", e.to_string()))
        }
        Some(session) => {
            let fallback = session.config.ai.budget.clone();
            app.read_pool()
                .run(session.key.clone(), move |file, _| {
                    let ledger = os_store::ai::budget::open_ledger(file.conn(), fallback, &[])?;
                    ledger
                        .snapshot()
                        .map_err(|e| os_store::StoreError::InvalidInput(e.to_string()))
                })
                .await
        }
        None => {
            let config = super::ai_settings::window_config(app, ctx).await?;
            Ok(snapshot_without_crawl(config.ai.budget))
        }
    }
}

fn cost_budget<'a>(app: &'a App, ctx: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move { to_json(&cost_snapshot(&window_snapshot(app, &ctx).await?)) })
}

/// Eine Zeile des Abschnitts „KI“ (Spalten aus AI-012: `Task`, `Model`, `Reasoning` zwischen `Model` und `Items`
/// (Nachtrag 24.11.2026), `Items`, `Cached`, `Est. Input Tokens`, `Est. Output Tokens`, `Est. Cost (low/expected/high)`,
/// `Batch Savings`, `Executable`). Beträge in Mikro-USD, `None` = Preis unbekannt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateRow {
    pub task: String,
    pub model: String,
    /// Effektive Stufe; `None` über Decisions und Embeddings (Zelle leer).
    pub reasoning: Option<ReasoningLevel>,
    pub items: u64,
    pub cached: u64,
    pub est_input_tokens: u64,
    pub est_output_tokens: u64,
    pub low_micro_usd: Option<u64>,
    pub expected_micro_usd: Option<u64>,
    pub high_micro_usd: Option<u64>,
    pub batch_savings_micro_usd: Option<u64>,
    pub executable: bool,
    /// Grund (`Missing API key`, `Price unknown`), leer bei ausführbar.
    pub reason: String,
    pub calibrated: bool,
    /// Requests (Bündelung, AI-012 Nachtrag); `None`, wenn der Schätzer sie nicht kennt.
    pub requests: Option<u64>,
}

impl EstimateRow {
    /// Aus der Kernschätzung einer Aufgabe.
    pub fn from_task(
        t: &TaskEstimate,
        reasoning: Option<ReasoningLevel>,
        requests: Option<u64>,
    ) -> Self {
        Self {
            task: t.task.clone(),
            model: t.model.clone(),
            reasoning,
            items: t.items,
            cached: t.cached,
            est_input_tokens: t.est_input_tokens,
            est_output_tokens: t.est_output_tokens,
            low_micro_usd: t.low.map(|m| m.0),
            expected_micro_usd: t.expected.map(|m| m.0),
            high_micro_usd: t.high.map(|m| m.0),
            batch_savings_micro_usd: t.batch_savings.map(|m| m.0),
            executable: t.executable.is_yes(),
            reason: t.executable.reason().to_string(),
            calibrated: t.calibrated,
            requests,
        }
    }

    /// Aus der Request-Schätzung (Decisions-Weg und Schema-Aufgaben, K10).
    pub fn from_requests(r: &RequestTaskEstimate) -> Self {
        Self::from_task(&r.estimate, r.reasoning, Some(r.requests))
    }
}

/// Summen (niedrig, erwartet, hoch) in Mikro-USD.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub low_micro_usd: u64,
    pub expected_micro_usd: u64,
    pub high_micro_usd: u64,
}

impl From<EstimateTotals> for Totals {
    fn from(t: EstimateTotals) -> Self {
        Self {
            low_micro_usd: t.low.0,
            expected_micro_usd: t.expected.0,
            high_micro_usd: t.high.0,
        }
    }
}

/// Ein Abschnitt des Dialogs (`ai`; ab M5 `dataforseo` mit eigenen Zeilen, D4S-012).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateSection {
    pub id: String,
    /// Zeilen des Abschnitts; Form je Abschnitt (KI: [`EstimateRow`]).
    pub rows: Vec<Value>,
    pub totals: Totals,
}

/// Crawl-Budget, Verbrauch und Rest aus dem `BudgetLedger` (AI-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetLine {
    /// `None` = unbegrenzt.
    pub budget_micro_usd: Option<u64>,
    pub consumed_micro_usd: u64,
    /// `None` = unbegrenzt.
    pub remaining_micro_usd: Option<u64>,
    pub warning: bool,
    pub halted: bool,
}

/// Inhalt des Dialogs „Kostenvoranschlag“ (AI-012).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimateView {
    pub sections: Vec<EstimateSection>,
    /// Gemeinsame Summenzeile aller Abschnitte.
    pub totals: Totals,
    pub budget: BudgetLine,
    pub threshold_micro_usd: u64,
    /// Erwartete Summe über der Schwelle oder über 50 % des Restbudgets → kein KI-Request vor „Starten“.
    pub confirmation_required: bool,
}

/// Baut den Dialoginhalt aus den KI-Zeilen (Kernschätzungen), dem Ledger-Stand und der Bestätigungsschwelle. Weitere
/// Abschnitte (M5) hängt der Aufrufer mit [`CostEstimateView::with_section`] an.
pub fn estimate_view(
    ai_tasks: &[(TaskEstimate, Option<ReasoningLevel>, Option<u64>)],
    snapshot: &BudgetSnapshot,
    threshold: MicroUsd,
) -> CostEstimateView {
    let estimates: Vec<TaskEstimate> = ai_tasks.iter().map(|(t, _, _)| t.clone()).collect();
    let ai_totals = EstimateTotals::of_tasks(&estimates);
    let rows = ai_tasks
        .iter()
        .map(|(t, r, n)| {
            serde_json::to_value(EstimateRow::from_task(t, *r, *n)).unwrap_or(Value::Null)
        })
        .collect();
    let mut view = CostEstimateView {
        sections: vec![EstimateSection {
            id: AREA_AI.into(),
            rows,
            totals: ai_totals.into(),
        }],
        totals: ai_totals.into(),
        budget: BudgetLine {
            budget_micro_usd: (!snapshot.limits.unlimited).then_some(snapshot.limits.crawl.0),
            consumed_micro_usd: snapshot.consumption().0,
            remaining_micro_usd: snapshot.remaining.map(|m| m.0),
            warning: snapshot.warning,
            halted: snapshot.halted,
        },
        threshold_micro_usd: threshold.0,
        confirmation_required: false,
    };
    view.confirmation_required = confirmation_required(
        MicroUsd(view.totals.expected_micro_usd),
        threshold,
        snapshot.remaining,
    );
    view
}

impl CostEstimateView {
    /// Hängt einen weiteren Abschnitt an (M5-U01: DataForSEO mit `Cost Min/Max`) und rechnet Summenzeile und
    /// Bestätigung neu: niedrig += `low`, erwartet/hoch += `high` (konservativ, AI-012/D4S-012).
    pub fn with_section(
        mut self,
        section: EstimateSection,
        threshold: MicroUsd,
        remaining: Option<MicroUsd>,
    ) -> Self {
        self.totals = Totals {
            low_micro_usd: self
                .totals
                .low_micro_usd
                .saturating_add(section.totals.low_micro_usd),
            expected_micro_usd: self
                .totals
                .expected_micro_usd
                .saturating_add(section.totals.expected_micro_usd),
            high_micro_usd: self
                .totals
                .high_micro_usd
                .saturating_add(section.totals.high_micro_usd),
        };
        self.sections.push(section);
        self.confirmation_required = confirmation_required(
            MicroUsd(self.totals.expected_micro_usd),
            threshold,
            remaining,
        );
        self
    }
}

/// Schätzung einer Aufgabe für den Dialog: Kernschätzung, effektive Stufe, Requestzahl.
pub type EstimateEntry = (TaskEstimate, Option<ReasoningLevel>, Option<u64>);

/// Ein angemeldeter Schätzer (je Fachpaket, z. B. Decision Engine, Vorschläge): liefert die Zeilen seiner Aufgaben für
/// den Lauf `args` im Fenster `ctx`.
pub type Estimator = Arc<
    dyn Fn(App, CallCtx, AiRunArgs) -> BoxFuture<'static, Result<Vec<EstimateEntry>, CmdError>>
        + Send
        + Sync,
>;

/// Angemeldete Schätzer dieser App.
#[derive(Default)]
pub struct Estimators(Mutex<Vec<Estimator>>);

/// Meldet einen Schätzer an (Fachpakete der KI-Aufgaben).
pub fn register_estimator(app: &App, estimator: Estimator) {
    app.ext::<Estimators>()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push(estimator);
}

fn cost_estimate<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: AiRunArgs = if args.is_null() {
            AiRunArgs::default()
        } else {
            parse_args(args)?
        };
        let estimators: Vec<Estimator> = app
            .ext::<Estimators>()
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let mut entries = Vec::new();
        for e in estimators {
            entries.extend(e(app.clone(), ctx.clone(), args.clone()).await?);
        }
        let snapshot = window_snapshot(app, &ctx).await?;
        let threshold = app.settings_snapshot().ai_app.confirmation_threshold;
        let view = estimate_view(&entries, &snapshot, threshold);
        to_json(&view).map_err(|e| internal("ai.costEstimate", e.message_key))
    })
}

/// Handler der U01-Commands dieses Bereichs (Eintrag im Katalog `ai_catalog`, M4-F02).
pub(crate) fn handler(name: &str) -> Option<Handler> {
    match name {
        "ai.costBudget" => Some(cost_budget),
        "ai.costEstimate" => Some(cost_estimate),
        _ => None,
    }
}

//! KI-Lauf in der App (M4-D05): Command `ai.decisionsRun` (Katalog M4-F02, Besitzer M4-D05, lang laufend), die
//! KI-Warteschlange der App für die Engine (`EngineDeps::ai_queue`), der Schätzer für `ai.costEstimate` (AI-012) und die
//! Feldmengen der aktiven Fragen für den Dialog „Datenfluss bestätigen“ (AI-018).
//!
//! Ablauf eines Laufs aus der Oberfläche (`startAiRun`, `features/ai/run`): `requestConsent()` → `requestCostConfirmation()`
//! (zeigt den Kostenvoranschlag nur, wenn die Bestätigung nötig ist) → `ai.decisionsRun` mit `confirmed`. Der Command
//!
//! * läuft nur mit Schalter „KI (Vorschau)“ und auf einer schreibbar geöffneten Crawl-Datei, nicht während der eigene Crawl
//!   läuft (dort gilt die automatische Ausführung, AI-039);
//! * lehnt einen unterbrochenen Crawl mit „Crawl ist unterbrochen – zuerst fortsetzen oder stoppen“ ab
//!   (`err-ai-run-interrupted`, Datei byte-gleich, kein Request) und einen fremd geführten mit `busy`
//!   (`err-ai-run-foreign`, „gestartet von <kind> (PID <n>) – Steuerung nur dort“, AGENT-020);
//! * startet den Lauf als Operation (`op-ai-decisions`) über den Laufrahmen `os_ai::run` mit eigener Laufzeile
//!   (`ai_runs`, bestätigt → `confirmed_at`), dem Besitz des Laufs (`RunOwner`), eigenen Adapter-Instanzen, der
//!   Einwilligung der App (`ai_consent::consent_gate`) und dem gemeinsamen Ledger der Datei. Abbruch der Operation,
//!   Schließen der Crawl-Datei und Beenden der App brechen ihn ab (08 AI-020, Nachtrag K4-13).
use std::sync::Arc;

use futures_util::future::BoxFuture;
use os_ai::catalog::{PriceTable, TaskDefaults};
use os_ai::decisions::exec::Selection;
use os_ai::keys::AiKeyStore;
use os_ai::run::{
    DecisionQueue, DecisionsJob, JobConfirmation, JobError, ProviderSetup, RunEnv, TablePrices,
};
use os_model::ai::hook::AiQueue;
use os_model::CrawlConfig;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::dto::ai::AiRunArgs;
use crate::dto::{CmdError, ErrorCode};
use crate::registry::{CmdResult, Handler};
use crate::runtime::error::{internal, parse_args, store_error, to_json};
use crate::{App, CallCtx};

/// Bezeichnung der Operation eines KI-Laufs (`ai.decisionsRun`).
pub const OP_LABEL: &str = "op-ai-decisions";

/// Meldung (de) bei einem unterbrochenen Crawl.
pub use os_ai::run::{INTERRUPTED_DE, INTERRUPTED_EN};

/// Technische Ursache (`detail`, englisch, OUT-243) bei einem unterbrochenen Crawl.
pub const INTERRUPTED_DETAIL: &str =
    "crawl is interrupted (stored state Running with a stale heartbeat); resume or stop it first";

/// Argumente von `ai.decisionsRun`: `AiRunArgs` (Vertrag aus dem Katalog) plus die erteilte Kostenbestätigung.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DecisionsRunArgs {
    #[serde(flatten)]
    pub run: AiRunArgs,
    /// „Starten“ im Kostenvoranschlag geklickt bzw. keine Bestätigung nötig (AI-012); `confirmed_at` der Laufzeile.
    pub confirmed: bool,
}

/// Handler der Commands dieses Moduls.
pub fn handler(name: &str) -> Option<Handler> {
    match name {
        "ai.decisionsRun" => Some(decisions_run),
        _ => None,
    }
}

fn preview_required(app: &App) -> Result<(), CmdError> {
    if super::ai_preview::switch(app).is_on() {
        Ok(())
    } else {
        Err(CmdError::new(ErrorCode::Unsupported, "err-ai-preview-off"))
    }
}

/// Umgebung eines KI-Laufs der App für die Konfiguration `cfg`.
pub fn run_env(app: &App, cfg: &CrawlConfig) -> Result<RunEnv, String> {
    let settings = app.settings_snapshot().ai_app;
    let providers =
        ProviderSetup::from_store(&AiKeyStore::new(app.config().secrets.clone()), &settings);
    let prices = TablePrices::load()?;
    let pricing = PriceTable::load(&|k| std::env::var(k).ok()).map_err(|e| e.to_string())?;
    let lists = super::ai_settings::ModelLists::build(
        &pricing,
        &super::ai_settings::live_models(app).all(),
    );
    let keys = os_ai::run::env::PROVIDERS
        .iter()
        .map(|p| (p.to_string(), providers.has_key(p)))
        .collect();
    let consent = super::ai_consent::consent_gate(app).map_err(|e| e.message_key.clone())?;
    let base = RunEnv::from_settings(
        &settings,
        cfg,
        &AiKeyStore::new(app.config().secrets.clone()),
        consent,
        app.config().clock.clone(),
        "app",
    )?;
    Ok(RunEnv {
        provider_states: lists.provider_states(&keys),
        providers,
        prices: Arc::new(prices),
        defaults: TaskDefaults::bundled().map_err(|e| e.to_string())?,
        ..base
    })
}

/// KI-Warteschlange der App für `EngineDeps::ai_queue`; `None` mit Schalter „KI (Vorschau)“ aus (Verhalten von 1.0).
pub fn app_queue(app: &App) -> Option<Arc<dyn AiQueue>> {
    ensure_registered(app);
    if !super::ai_preview::switch(app).is_on() {
        return None;
    }
    let owned = app.clone();
    Some(Arc::new(DecisionQueue::new(Arc::new(
        move |cfg: &CrawlConfig, _kind: &'static str| run_env(&owned, cfg),
    ))))
}

#[derive(Default)]
struct Registered(std::sync::OnceLock<()>);

/// Meldet einmal je App den Schätzer der Decision-Fragen (`ai.costEstimate`) und die Feldmengen der aktiven Fragen
/// (Dialog „Datenfluss bestätigen“) an.
pub fn ensure_registered(app: &App) {
    let once = app.ext::<Registered>();
    once.0.get_or_init(|| {
        super::ai_cost::register_estimator(app, Arc::new(estimate));
        super::ai_consent::register_flow_source(
            app,
            Arc::new(|cfg: &CrawlConfig| library_fields(cfg)),
        );
    });
}

/// Feldmengen der aktiven Fragen, die nicht im Profil stehen (Bibliotheksfragen, D03); die Profilfragen meldet
/// `ai_consent::profile_question_fields` selbst.
fn library_fields(_cfg: &CrawlConfig) -> Vec<super::ai_consent::TaskFields> {
    // Die mitgelieferte Bibliothek ist bis M4-D03 leer; dann melden ihre Fragen hier ihre Felder.
    Vec::new()
}

/// Seiten der Grundmenge G der Crawl-Datei mit ihren Eingabefeldern (AI-032, Nachtrag „Quelle der Eingabefelder“).
fn pages(
    path: &std::path::Path,
    cfg: &CrawlConfig,
) -> Result<Vec<os_model::ai::hook::PageAiFields>, CmdError> {
    os_engine::ai_input::load_pages(path, cfg).map_err(|e| store_error(&e))
}

/// Schätzer für `ai.costEstimate`: Voranschlag der aktiven Decision-Fragen über die offenen Einträge der Crawl-Datei des
/// Fensters (AI-012, Abschnitt „KI“); sendet nichts.
fn estimate(
    app: App,
    ctx: CallCtx,
    args: AiRunArgs,
) -> BoxFuture<'static, Result<Vec<super::ai_cost::EstimateEntry>, CmdError>> {
    Box::pin(async move {
        let Some(s) = app.sessions().current(&ctx.window) else {
            return Ok(Vec::new());
        };
        if !super::ai_preview::switch(&app).is_on() || !s.writable {
            return Ok(Vec::new());
        }
        let cfg = (*s.config).clone();
        if os_ai::run::items::active_questions(&cfg, &args.tasks)
            .0
            .is_empty()
        {
            return Ok(Vec::new());
        }
        let env = run_env(&app, &cfg).map_err(|e| internal("ai run env", e))?;
        let path = s.path.clone();
        let tasks = args.tasks.clone();
        let estimate = tokio::task::spawn_blocking(move || {
            let pages = pages(&path, &cfg)?;
            let job = DecisionsJob {
                crawl_path: path,
                config: cfg,
                questions: tasks,
                pages,
                confirmation: JobConfirmation::Unconfirmed,
                selection: Selection::Open,
            };
            os_ai::run::estimate_decisions(&env, &job).map_err(job_error)
        })
        .await
        .map_err(|e| internal("ai estimate", e.to_string()))??;
        Ok(estimate
            .tasks
            .iter()
            .map(|t| (t.estimate.clone(), t.reasoning, Some(t.requests)))
            .collect())
    })
}

/// Fehler des Laufrahmens als Command-Fehler.
pub fn job_error(e: JobError) -> CmdError {
    match e {
        // OUT-243: `detail` ist die technische, englische Ursache; die Meldung zeigt die Oberfläche über den Schlüssel
        // (de/en). `args.en` bleibt für Aufrufer ohne Übersetzung.
        JobError::Interrupted => CmdError::new(ErrorCode::InvalidInput, "err-ai-run-interrupted")
            .with_detail(INTERRUPTED_DETAIL)
            .with_arg("en", INTERRUPTED_EN),
        JobError::Occupied { kind, pid } => CmdError::new(ErrorCode::Busy, "err-ai-run-foreign")
            .with_arg("kind", kind.clone())
            .with_arg("pid", pid.clone())
            .with_detail(format!(
                "crawl is led by another process ({kind}, PID {pid}) with a current heartbeat"
            )),
        JobError::NoQuestions(errors) => {
            CmdError::new(ErrorCode::InvalidInput, "err-ai-run-no-questions")
                .with_detail(errors.join("; "))
        }
        JobError::Store(detail) => {
            CmdError::new(ErrorCode::Store, "err-ai-run").with_detail(detail)
        }
    }
}

/// Vorprüfung ohne Schreibzugriff: unterbrochen → Meldung, fremd geführt → `busy` (die Datei bleibt byte-gleich).
async fn precheck(app: &App, s: &crate::runtime::sessions::SessionInfo) -> Result<(), CmdError> {
    let clock = app.config().clock.clone();
    let checked = app
        .read_pool()
        .run(s.key.clone(), move |file, _| {
            let state = file.effective_state(&*clock)?;
            let occupancy = file.occupancy(&*clock)?;
            Ok((state, occupancy))
        })
        .await?;
    let (state, occupancy) = checked;
    if state == os_model::CrawlState::Incomplete {
        return Err(job_error(JobError::Interrupted));
    }
    if occupancy.foreign_led(std::process::id()) {
        return Err(job_error(JobError::Occupied {
            kind: occupancy.owner_kind.unwrap_or_default(),
            pid: occupancy.owner_pid.unwrap_or_default(),
        }));
    }
    Ok(())
}

/// `ai.decisionsRun` (AiRunArgs + `confirmed`) → Operation; Ergebnis `{runId, end, requests, statuses, unavailable,
/// estimate}`.
fn decisions_run<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        preview_required(app)?;
        ensure_registered(app);
        let args: DecisionsRunArgs = if args.is_null() {
            DecisionsRunArgs::default()
        } else {
            parse_args(args)?
        };
        let s = app
            .sessions()
            .current(&ctx.window)
            .ok_or_else(|| CmdError::new(ErrorCode::NoCrawl, "err-no-crawl"))?;
        precheck(app, &s).await?;
        if !s.writable {
            return Err(CmdError::new(ErrorCode::Permission, "err-crawl-readonly"));
        }
        let engine_running = s
            .engine
            .as_ref()
            .is_some_and(|e| e.handle.lock().unwrap_or_else(|p| p.into_inner()).is_some());
        if engine_running || s.state == os_model::CrawlState::Running {
            // 08 AI-020, Nachtrag 28.11.2026 („Start während des eigenen laufenden Crawls“): abgewiesen – kein Request,
            // keine Laufzeile, Zustand unverändert. Einzige Ausnahme: Der KI-Teil dieses Crawls wartet auf die
            // Kostenbestätigung (AI-012, Einträge `Pending (confirmation)`); dann ist der bestätigte Auftrag die Antwort
            // auf den Voranschlag (`confirmed_at` in die Laufzeile dieses Crawls, die wartenden Einträge gehen hinaus).
            if args.confirmed && os_ai::run::confirm_running(&s.path) {
                return Ok(json!({ "end": "Confirmed" }));
            }
            return Err(CmdError::new(
                ErrorCode::CrawlRunning,
                "err-ai-run-crawl-running",
            ));
        }
        let cfg = (*s.config).clone();
        if os_ai::run::items::active_questions(&cfg, &args.run.tasks)
            .0
            .is_empty()
        {
            return Err(job_error(JobError::NoQuestions(Vec::new())));
        }
        let env = run_env(app, &cfg).map_err(|e| internal("ai run env", e))?;
        let path = s.path.clone();
        let handle = app.ops().spawn(OP_LABEL, true, move |token, _| {
            Box::pin(async move {
                let abort_path = path.clone();
                let _cancel = token.on_cancel(move || os_ai::run::controls().abort(&abort_path));
                let outcome = tokio::task::spawn_blocking(move || {
                    let pages = pages(&path, &cfg)?;
                    let job = DecisionsJob {
                        crawl_path: path,
                        config: cfg,
                        questions: args.run.tasks,
                        pages,
                        confirmation: if args.confirmed {
                            JobConfirmation::Confirmed
                        } else {
                            JobConfirmation::Unconfirmed
                        },
                        selection: Selection::Open,
                    };
                    os_ai::run::run_decisions_blocking(env, job).map_err(job_error)
                })
                .await
                .map_err(|e| internal("ai run", e.to_string()))??;
                Ok(json!({
                    "runId": outcome.run_id,
                    "end": format!("{:?}", outcome.end),
                    "requests": outcome.requests,
                    "statuses": outcome.statuses,
                    "unavailable": outcome.unavailable,
                    "estimate": outcome.estimate.map(|e| json!({
                        "expectedMicroUsd": e.totals.expected.0,
                        "tasks": e.tasks.len(),
                    })),
                }))
            })
        });
        to_json(&handle)
    })
}

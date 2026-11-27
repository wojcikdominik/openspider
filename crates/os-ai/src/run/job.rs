//! Ein KI-Lauf über die Decision-Fragen auf einer Crawl-Datei (08 AI-020 [M4], AI-012, AI-013, AI-039; M4-D05).
//!
//! Ablauf ([`run_decisions`]):
//! 1. Besitz übernehmen ([`RunOwner::start`], M4-K11) – **vor** jedem Schreibzugriff: Ein unterbrochener Crawl
//!    ([`StoreError::Interrupted`]) bzw. ein fremder aktiver Besitzer ([`StoreError::Occupied`]) lehnt ab, ohne Request
//!    und ohne die Datei zu verändern („Kein KI-Lauf auf einem unterbrochenen Crawl“, „Crawl in einem fremden
//!    Writer-Prozess“).
//! 2. Budget: Die Crawl-Datei ohne gespeicherte Grenzen erhält `ai.budget` (`save_limits`); der Ledger ist der
//!    gemeinsame der Datei ([`super::controls`]).
//! 3. Laufzeile (`ai_runs`, K4-27): neuer Lauf = neue Zeile; mit Bestätigung `confirmed_at` = jetzt und
//!    [`Confirmation::Granted`], ohne Bestätigung [`Confirmation::AskAbove`] (Einträge `Pending (confirmation)`, nichts
//!    gesendet). Fortsetzen = dieselbe (nicht beendete) Zeile, deren Bestätigung fortgilt – auch nach Neustart.
//! 4. `Skipped (input unavailable)` für Einträge ohne gespeicherte bzw. ableitbare Eingabe (AI-032, Punkt 3).
//! 5. [`DecisionRun`] mit eigenen Adapter-Instanzen (Auflage E8) und dem Besitz als [`OwnerLease`] (`finish` in jedem
//!    Ende-Pfad); Pause, Stopp und Abbruch über das Register ([`super::controls`]).
//! 6. Ende: fertig bzw. gestoppt → `finish_run` (die Zeile ist abgeschlossen); Pause, Abbruch und ausstehende
//!    Bestätigung lassen sie offen (Fortsetzen).
use std::collections::BTreeMap;
use std::path::PathBuf;

use os_model::ai::hook::PageAiFields;
use os_model::budget::{BudgetLedger, BudgetLimits, MicroUsd};
use os_model::CrawlConfig;
use os_store::ai::answers;
use os_store::ai::cache::Connection;
use os_store::{CrawlFile, OpenMode, StoreError};

use super::env::{AdapterSet, RunEnv};
use super::items;
use crate::decisions::exec::{
    ActiveQuestion, Confirmation, DecisionRun, OwnerLease, OwnerStart, RunControl, RunEnd,
    RunEstimate, RunItem, RunOptions, RunReport, Selection,
};
use crate::limits::Limiter;
use crate::policy::Redactor;
use crate::run_owner::RunOwner;

/// Meldung bei einem unterbrochenen Crawl (08 AI-020, „Kein KI-Lauf auf einem unterbrochenen Crawl“), deutsch.
pub const INTERRUPTED_DE: &str = "Crawl ist unterbrochen – zuerst fortsetzen oder stoppen";
/// Dieselbe Meldung, englisch.
pub const INTERRUPTED_EN: &str = "Crawl was interrupted – resume or stop it first";

/// Kostenbestätigung eines Laufs (AI-012, AI-020 Nachtrag 27.11.2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobConfirmation {
    /// Neuer Lauf, „Starten“ im Kostenvoranschlag geklickt (bzw. keine Bestätigung nötig laut Dialog).
    Confirmed,
    /// Neuer Lauf ohne Bestätigung: über der Schwelle warten die Einträge mit `Pending (confirmation)`.
    Unconfirmed,
    /// Fortsetzen: dieselbe Laufzeile, deren Bestätigung fortgilt; kein neuer Voranschlag.
    Resume,
}

/// Auftrag eines KI-Laufs.
#[derive(Debug, Clone)]
pub struct DecisionsJob {
    pub crawl_path: PathBuf,
    /// Konfiguration des Crawls (aktive Fragen, `ai.budget`, `ai.tasks`).
    pub config: CrawlConfig,
    /// Fragen-IDs; leer = alle aktiven.
    pub questions: Vec<String>,
    /// Seiten-URLs der Grundmenge G mit ihren Eingabefeldern (Engine: `os_engine::ai_input::load_pages`).
    pub pages: Vec<PageAiFields>,
    pub confirmation: JobConfirmation,
    pub selection: Selection,
}

/// Ergebnis eines KI-Laufs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobOutcome {
    pub run_id: i64,
    pub end: RunEnd,
    /// Gesendete Requests mit Protokollzeile (ohne Cache-Treffer).
    pub requests: u64,
    pub estimate: Option<RunEstimate>,
    /// Statuszählung je Frage (Kennung → Status → Zahl).
    pub statuses: BTreeMap<String, BTreeMap<String, u64>>,
    /// Einträge mit `Skipped (input unavailable)` aus diesem Lauf.
    pub unavailable: usize,
    pub commit_errors: Vec<String>,
}

/// Ablehnung bzw. Fehler eines KI-Laufs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobError {
    /// Unterbrochener Crawl: [`INTERRUPTED_DE`] bzw. [`INTERRUPTED_EN`].
    Interrupted,
    /// Ein anderer Prozess führt den Crawl (AGENT-020: „gestartet von <kind> (PID <n>) – Steuerung nur dort“).
    Occupied { kind: String, pid: String },
    /// Keine aktiven Fragen bzw. ungültige Fragen.
    NoQuestions(Vec<String>),
    /// Sonstiger Fehler der Crawl-Datei.
    Store(String),
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Interrupted => f.write_str(INTERRUPTED_DE),
            Self::Occupied { kind, pid } => {
                write!(f, "gestartet von {kind} (PID {pid}) – Steuerung nur dort")
            }
            Self::NoQuestions(e) if e.is_empty() => f.write_str("keine aktiven Fragen"),
            Self::NoQuestions(e) => write!(f, "keine ausführbaren Fragen: {}", e.join("; ")),
            Self::Store(e) => f.write_str(e),
        }
    }
}

impl From<StoreError> for JobError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Interrupted => Self::Interrupted,
            StoreError::Occupied { kind, pid } => Self::Occupied { kind, pid },
            other => Self::Store(other.to_string()),
        }
    }
}

/// Besitz eines KI-Laufs als `OwnerLease` des Decision-Laufs (Anbindung M4-K11 an M4-D01): `finish` in jedem Ende-Pfad
/// von `DecisionRun::run`, nie beim Verwerfen (= Absturz).
impl OwnerLease for RunOwner {
    fn finish(self: Box<Self>) -> Result<(), StoreError> {
        RunOwner::finish(*self)
    }
}

/// `DecisionRun::owner` für einen bereits übernommenen Besitz.
pub fn owner_of<'a>(owner: RunOwner) -> OwnerStart<'a> {
    Box::new(move || Ok(Box::new(owner) as Box<dyn OwnerLease>))
}

/// `DecisionRun::owner`, der den Besitz erst beim Start des Laufs übernimmt (`RunOwner::start`, synchron; aufgerufen
/// auf dem eigenen Thread des Laufs, nie auf einem Arbeitsthread der App).
pub fn owner_start<'a>(
    path: PathBuf,
    kind: &'static str,
    clock: std::sync::Arc<dyn os_model::Clock>,
) -> OwnerStart<'a> {
    Box::new(move || {
        RunOwner::start(&path, kind, clock).map(|o| Box::new(o) as Box<dyn OwnerLease>)
    })
}

/// Werkzeuge eines Laufs (je Lauf neu).
pub(crate) struct Tools {
    pub adapters: AdapterSet,
    pub limiter: Limiter,
    pub redactor: Redactor,
}

impl Tools {
    pub fn new(env: &RunEnv) -> Self {
        Self {
            adapters: env.providers.adapters(),
            limiter: env.limiter(),
            redactor: env.redactor(),
        }
    }
}

/// Ein Decision-Lauf über `items` (gemeinsam für App, CLI, MCP und die Warteschlange während des Crawls).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_chunk(
    env: &RunEnv,
    tools: &Tools,
    conn: &Connection,
    ledger: &dyn BudgetLedger,
    run_id: i64,
    questions: &[ActiveQuestion],
    items: &[RunItem],
    selection: Selection,
    confirmation: Confirmation,
    control: RunControl,
    owner: Option<OwnerStart<'_>>,
    keep_latches: bool,
) -> Result<RunReport, StoreError> {
    let now = || env.clock.now();
    let run = DecisionRun {
        conn,
        run_id,
        questions,
        items,
        routing: env.routing(),
        adapters: tools.adapters.view(),
        limiter: &tools.limiter,
        ledger,
        consent: &env.consent,
        switch: env.switch,
        redactor: &tools.redactor,
        prices: &*env.prices,
        control,
        options: RunOptions {
            concurrency: env.concurrency.max(1),
            selection,
            confirmation,
            keep_latches,
            ..RunOptions::default()
        },
        now: &now,
        owner,
        before_commit: None,
    };
    run.run().await
}

/// Laufzeile zum Fortsetzen: die jüngste nicht beendete Zeile (K4-27).
pub fn open_run(conn: &Connection) -> Result<Option<i64>, StoreError> {
    Ok(conn.query_row(
        "SELECT MAX(id) FROM ai_runs WHERE ended_at IS NULL",
        [],
        |r| r.get::<_, Option<i64>>(0),
    )?)
}

/// Neue Laufzeile mit Konfigurations-Snapshot und Budgetstand.
pub fn begin_run(
    conn: &Connection,
    env: &RunEnv,
    config: &CrawlConfig,
    ledger: &dyn BudgetLedger,
) -> Result<i64, StoreError> {
    let snapshot = serde_json::json!({
        "active_questions": config.ai.active_questions,
        "auto_run": config.ai.auto_run.get(),
        "owner_kind": env.owner_kind,
    });
    let budget = ledger
        .snapshot()
        .ok()
        .and_then(|s| s.remaining)
        .map(|m| m.0);
    answers::begin_run(
        conn,
        &env.clock.now().to_db(),
        budget,
        &snapshot.to_string(),
    )
}

/// Ledger der Datei: Grenzen aus der Datei bzw. – ohne gespeicherte Grenzen – aus `ai.budget`, das dabei gespeichert
/// wird (U01, „Benötigte Änderungen“ 4).
pub fn shared_ledger(
    path: &std::path::Path,
    conn: &Connection,
    fallback: &BudgetLimits,
) -> Result<super::control::LedgerGuard, StoreError> {
    if os_store::ai::budget::load_limits(conn)?.is_none() {
        os_store::ai::budget::save_limits(conn, fallback)?;
    }
    super::controls().ledger(path, || {
        os_store::ai::budget::open_ledger(conn, fallback.clone(), &[])
    })
}

/// Bestätigung eines Laufs aus der Laufzeile bzw. dem Auftrag.
pub fn confirmation_for(
    conn: &Connection,
    env: &RunEnv,
    run_id: i64,
    job: JobConfirmation,
) -> Result<Confirmation, StoreError> {
    match job {
        JobConfirmation::Confirmed => {
            answers::set_confirmed_at(conn, run_id, &env.clock.now().to_db())?;
            Ok(Confirmation::Granted)
        }
        JobConfirmation::Unconfirmed => Ok(Confirmation::AskAbove(env.threshold)),
        JobConfirmation::Resume => Ok(match answers::confirmed_at(conn, run_id)? {
            Some(_) => Confirmation::Granted,
            None => Confirmation::AskAbove(env.threshold),
        }),
    }
}

/// Statuszählung je Frage.
pub fn status_counts(
    conn: &Connection,
    questions: &[ActiveQuestion],
) -> Result<BTreeMap<String, BTreeMap<String, u64>>, StoreError> {
    let mut out = BTreeMap::new();
    for q in questions {
        out.insert(
            q.def.id.clone(),
            answers::status_counts(conn, &q.def.id, &q.def.version.to_string())?,
        );
    }
    Ok(out)
}

/// Führt einen KI-Lauf aus (siehe Moduldokumentation). Braucht eine tokio-Laufzeit (Heartbeat des Besitzes); die
/// Zukunft ist nicht `Send` (SQLite-Verbindung) – App, CLI und MCP rufen [`run_decisions_blocking`].
pub async fn run_decisions(env: &RunEnv, job: DecisionsJob) -> Result<JobOutcome, JobError> {
    let (questions, errors) = items::active_questions(&job.config, &job.questions);
    if questions.is_empty() {
        return Err(JobError::NoQuestions(errors));
    }
    // (1) Besitz vor jedem Schreibzugriff: unterbrochen bzw. fremd geführt → ablehnen, Datei unverändert.
    let owner = RunOwner::start(&job.crawl_path, env.owner_kind, env.clock.clone())?;
    let prepared = (|| -> Result<_, StoreError> {
        let file = CrawlFile::open(&job.crawl_path, OpenMode::ReadWrite)?;
        Ok(file)
    })();
    let file = match prepared {
        Ok(f) => f,
        Err(e) => {
            let _ = owner.finish();
            return Err(e.into());
        }
    };
    let conn = file.conn();
    let control = RunControl::new();
    let _registered = super::controls().register(&job.crawl_path, control.clone());
    // Ein sendender Decision-Lauf je Datei (siehe `queue::file_lock`).
    let lock = super::queue::file_lock(&job.crawl_path);
    let _turn = lock.lock().await;
    let started = async {
        let ledger = shared_ledger(&job.crawl_path, conn, &job.config.ai.budget)?;
        let run_id = match job.confirmation {
            JobConfirmation::Resume => match open_run(conn)? {
                Some(id) => id,
                None => begin_run(conn, env, &job.config, &*ledger)?,
            },
            _ => begin_run(conn, env, &job.config, &*ledger)?,
        };
        let confirmation = confirmation_for(conn, env, run_id, job.confirmation)?;
        let unavailable = items::mark_unavailable(conn, &questions, &job.pages)?;
        Ok::<_, StoreError>((ledger, run_id, confirmation, unavailable))
    }
    .await;
    let (ledger, run_id, confirmation, unavailable) = match started {
        Ok(v) => v,
        Err(e) => {
            let _ = owner.finish();
            return Err(e.into());
        }
    };
    let run_items: Vec<RunItem> = job.pages.iter().map(items::run_item).collect();
    let tools = Tools::new(env);
    let report = run_chunk(
        env,
        &tools,
        conn,
        &*ledger,
        run_id,
        &questions,
        &run_items,
        job.selection,
        confirmation,
        control,
        Some(owner_of(owner)),
        false,
    )
    .await?;
    if matches!(report.end, RunEnd::Completed | RunEnd::Stopped) {
        answers::finish_run(conn, run_id, &env.clock.now().to_db())?;
    }
    Ok(JobOutcome {
        run_id,
        end: report.end,
        requests: report.requests_logged,
        estimate: report.estimate,
        statuses: status_counts(conn, &questions)?,
        unavailable,
        commit_errors: report.commit_errors,
    })
}

/// Wie [`run_decisions`] auf einem eigenen Thread mit eigener (einfädiger) tokio-Laufzeit; blockiert bis zum Ende.
/// Aufruf aus `spawn_blocking` bzw. einem Thread außerhalb der Arbeitsthreads.
pub fn run_decisions_blocking(env: RunEnv, job: DecisionsJob) -> Result<JobOutcome, JobError> {
    std::thread::Builder::new()
        .name("os-ai-run".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| JobError::Store(e.to_string()))?;
            rt.block_on(run_decisions(&env, job))
        })
        .map_err(|e| JobError::Store(e.to_string()))?
        .join()
        .map_err(|_| JobError::Store("KI-Lauf abgestürzt".into()))?
}

/// Kostenvoranschlag eines neuen Laufs (AI-012) über dieselben Einträge, ohne Request und ohne Besitz. Schreibt wie
/// `DecisionRun::estimate` die Statuszeilen der geplanten Einträge; bei einem fremd geführten bzw. unterbrochenen Crawl
/// lehnt er wie ein Lauf ab, ohne zu schreiben.
pub fn estimate_decisions(env: &RunEnv, job: &DecisionsJob) -> Result<RunEstimate, JobError> {
    let (questions, errors) = items::active_questions(&job.config, &job.questions);
    if questions.is_empty() {
        return Err(JobError::NoQuestions(errors));
    }
    let probe = CrawlFile::open(&job.crawl_path, OpenMode::ReadOnly)?;
    let occupancy = probe.occupancy(&*env.clock)?;
    if occupancy.foreign_led(std::process::id()) {
        return Err(JobError::Occupied {
            kind: occupancy.owner_kind.clone().unwrap_or_default(),
            pid: occupancy.owner_pid.clone().unwrap_or_default(),
        });
    }
    if probe.effective_state(&*env.clock)? == os_model::CrawlState::Incomplete {
        return Err(JobError::Interrupted);
    }
    drop(probe);
    let file = CrawlFile::open(&job.crawl_path, OpenMode::ReadWrite)?;
    let conn = file.conn();
    let ledger = os_store::ai::budget::open_ledger(conn, job.config.ai.budget.clone(), &[])?;
    let run_items: Vec<RunItem> = job.pages.iter().map(items::run_item).collect();
    let tools = Tools::new(env);
    let now = || env.clock.now();
    let run = DecisionRun {
        conn,
        run_id: 0,
        questions: &questions,
        items: &run_items,
        routing: env.routing(),
        adapters: tools.adapters.view(),
        limiter: &tools.limiter,
        ledger: &ledger,
        consent: &env.consent,
        switch: env.switch,
        redactor: &tools.redactor,
        prices: &*env.prices,
        control: RunControl::new(),
        options: RunOptions {
            selection: job.selection,
            confirmation: Confirmation::AskAbove(MicroUsd(u64::MAX)),
            ..RunOptions::default()
        },
        now: &now,
        owner: None,
        before_commit: None,
    };
    Ok(run.estimate()?)
}

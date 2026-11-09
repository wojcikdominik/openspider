//! Ablauf eines Laufs: Crawl (neu, fortgesetzt oder geladen) → Crawl-Analyse → Exporte → Speichern.
//!
//! Phasen für `--json-progress`: `validate` (in `lib.rs`), `crawl`, `analysis`, `export`, `done`.
//! Reihenfolge der Schlusszeile: Sie ist immer die letzte Zeile auf stderr und nennt Ergebnis und
//! Exit-Code.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use os_analyze::analysis::persist::{self, RunState};
use os_analyze::analysis::{ensure_analysis, trigger};
use os_analyze::views::ViewQuery;
use os_engine::types::{CrawlRequest, CrawlSource, CrawlSummary, EngineDeps, FinishedHook};
use os_engine::{disk_guard, resume_crawl, start_crawl, CrawlHandle, EngineError};
use os_export::catalog::bulk_issues;
use os_export::{ExportFileInfo, Format};
use os_fetch::SecretStore;
use os_model::{
    Clock, CrawlConfig, CrawlId, CrawlState, IdSource, Progress, RandomIds, SystemClock, Timestamp,
};
use os_store::{queries, registry, CrawlFile, OpenMode, StorageProfile};
use tokio::time::{interval_at, Instant, MissedTickBehavior};

use crate::exit::{CliError, Exit};
use crate::plan::{Plan, Source};
use crate::registry::Resolved;
use crate::reporter::Reporter;
use crate::signals::Signals;

/// Ergebnis eines Laufs.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Exit-Code.
    pub exit: Exit,
    /// Kurzfassung für die Schlusszeile (ohne Exit-Code).
    pub summary: String,
    /// Crawl-Datei des Laufs, sobald es eine gibt (für die Laufhistorie, OUT-169).
    pub crawl_path: Option<PathBuf>,
    /// Crawl-ID des Laufs.
    pub crawl_id: Option<String>,
    /// Anzahl gecrawlter URLs.
    pub crawled: Option<u64>,
    /// Optional completion data retained before --no-store discards the database (OUT-281/282).
    pub mail_completion: Option<serde_json::Value>,
    /// Der Crawl endete regulär als `Failed` (CRAWL-128, z. B. Start-URL nicht erreichbar): Exit-Code und Historie bleiben
    /// unverändert (OUT-182), die Abschlussmail gilt aber als Fehlschlag (OUT-281). Inhalt: die Fehlermeldung.
    pub crawl_failed: Option<String>,
    /// Der Auto-Vergleich entfiel, weil der Crawl fehlschlug (`COMPARE_SKIPPED_FAILED`, OUT-152).
    pub compare_skipped: bool,
}

impl Outcome {
    /// Ergebnis ohne Angaben zum Crawl.
    pub fn new(exit: Exit, summary: impl Into<String>) -> Self {
        Outcome {
            exit,
            summary: summary.into(),
            crawl_path: None,
            crawl_id: None,
            crawled: None,
            mail_completion: None,
            crawl_failed: None,
            compare_skipped: false,
        }
    }

    fn with_crawl(mut self, path: &Path, crawled: Option<u64>) -> Self {
        self.crawl_id = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        self.crawl_path = Some(path.to_path_buf());
        self.crawled = crawled;
        self
    }
}

/// `IdSource`, die genau die vorab gewählte Crawl-ID liefert: Die Datei im Crawl-Ordner heißt
/// `<crawl_id>.ospider` und muss vor dem Start feststehen.
struct FixedId(CrawlId);

impl IdSource for FixedId {
    fn new_crawl_id(&self, _now: Timestamp) -> CrawlId {
        self.0
    }
}

/// Stand nach dem Crawl-Teil.
struct Crawled {
    path: PathBuf,
    crawled: u64,
    /// Der Lauf endete regulär als `Failed` (CRAWL-128).
    failed: bool,
}

tokio::task_local! {
    static EXECUTION_CLOCK: Arc<dyn Clock>;
}

fn execution_clock() -> Arc<dyn Clock> {
    EXECUTION_CLOCK
        .try_with(Arc::clone)
        .unwrap_or_else(|_| Arc::new(SystemClock))
}

/// Wie execute, mit einer nur für diesen Ablauf geltenden Uhr bis zur Crawl-Erzeugung.
/// Die Engine erhält dieselbe Arc explizit in ihren EngineDeps, auch in gestarteten Tasks.
pub async fn execute_with_clock(
    plan: Plan,
    rep: Arc<Reporter>,
    signals: Signals,
    clock: Arc<dyn Clock>,
) -> Outcome {
    EXECUTION_CLOCK
        .scope(clock, execute(plan, rep, signals))
        .await
}

/// Führt den Plan aus; jeder Fehler wird zu einem Ergebnis mit Exit-Code.
pub async fn execute(plan: Plan, rep: Arc<Reporter>, signals: Signals) -> Outcome {
    match run(&plan, &rep, &signals).await {
        Ok(outcome) => outcome,
        Err(e) => {
            rep.error(&e.message);
            Outcome::new(e.exit, e.headline().to_string())
        }
    }
}

async fn run(plan: &Plan, rep: &Arc<Reporter>, signals: &Signals) -> Result<Outcome, CliError> {
    if signals.is_interrupted() {
        return Ok(Outcome::new(
            Exit::Interrupted,
            "Unterbrochen vor dem Start des Crawls",
        ));
    }

    // INTEG-017 / OUT-201: credentials are validated against the host's store before any crawl.
    if plan.resume || !matches!(plan.source, Source::Load { .. }) {
        preflight_apis(plan, rep)?;
    }

    let crawled = match (&plan.source, plan.resume) {
        (Source::Load { path }, false) => open_only(path, rep)?,
        _ => {
            let result = crawl_phase(plan, rep, signals).await?;
            match result {
                CrawlEnd::Done(c) => c,
                CrawlEnd::Interrupted { id } => {
                    return Ok(Outcome::new(
                        Exit::Interrupted,
                        format!(
                            "Unterbrochen – der Crawl ist pausiert und gespeichert (ID {id}); \
                             fortsetzen mit: openspider --load-crawl {id} --resume"
                        ),
                    )
                    .with_crawl(&registry::path_for(&plan.paths.crawls, &id), None));
                }
            }
        }
    };

    // OUT-281: Ein regulär als `Failed` beendeter Lauf (CRAWL-128) ist für die Abschlussmail ein Fehlschlag.
    let mut crawl_failed = crawled.failed.then(|| "Crawl fehlgeschlagen".to_string());
    // Start-URL erreichbar? (nur neue Spider-Crawls)
    if let (Source::Spider { start_url }, false) = (&plan.source, plan.resume) {
        match start_url_problem(&crawled.path, start_url, crawled.failed) {
            Some(StartProblem::Unreachable(message)) => {
                if crawled.failed {
                    crawl_failed = Some(message.clone());
                }
                rep.warn(message.clone());
                if plan.fail_on_start_error {
                    // Exit 3 (OUT-182) mit Crawl-Angaben, damit die Fehler-Mail Start-URL, Projekt, Start und Ende trägt
                    // (OUT-281, OS-Festlegung 09.11.2026).
                    let error = format!("{message} – Abbruch wegen --fail-on-start-error");
                    rep.error(&error);
                    let mut outcome = Outcome::new(Exit::CrawlFailed, error)
                        .with_crawl(&crawled.path, Some(crawled.crawled));
                    outcome.crawl_failed = crawl_failed.clone();
                    return Ok(outcome);
                }
            }
            // CRAWL-128: Eine Sperre per robots.txt ist kein Fehlschlag – Warnung, aber nie Exit 3 durch
            // `--fail-on-start-error` (OS-Festlegung 09.11.2026).
            Some(StartProblem::Robots(detail)) => rep.warn(format!(
                "Start-URL {} durch robots.txt gesperrt: {detail}",
                os_fetch::redact::url(start_url)
            )),
            None => {}
        }
    }

    let mut failures = 0u32;
    let mut comparison_failed = false;
    if plan.compare_with_previous && crawled.failed {
        // Den Fehlschlag stuft der Crawl ein, nicht der Vergleich (OS-Festlegung 09.11.2026): ein als `Failed` beendeter
        // Crawl wird nicht verglichen und zählt nicht als fehlgeschlagener Vergleich.
        rep.warn(COMPARE_SKIPPED_FAILED);
    } else if plan.compare_with_previous {
        match os_analyze::compare::change::auto::run(
            &crawled.path,
            &plan.paths.crawls,
            &plan.paths.crawls.join("comparisons"),
            Some(&plan.config.compare),
            &signals.cancel_token(),
        ) {
            Ok(os_analyze::compare::change::auto::AutoResult::NoPrevious) => {
                rep.info("kein vorheriger Crawl")
            }
            Ok(os_analyze::compare::change::auto::AutoResult::Saved(path)) => {
                rep.info(format!("Vergleich gespeichert: {}", path.display()))
            }
            Err(e) => {
                failures += 1;
                comparison_failed = true;
                rep.error(format!("Auto-Vergleich fehlgeschlagen: {e}"));
            }
        }
    }
    let mut files = 0usize;
    if plan.output.is_some() && !comparison_failed {
        let start = std::time::Instant::now();
        let wal = format!("{}-wal", crawled.path.display());
        let before = std::fs::metadata(&wal).map_or(0, |m| m.len());
        let result = export_phase(plan, rep, signals, &crawled).await;
        if std::env::var_os("OS_SCALE_DIAG").is_some() {
            let after = std::fs::metadata(&wal).map_or(0, |m| m.len());
            eprintln!(
                "OS_SCALE_DIAG export duration_ms={} wal_before={} wal_after={}",
                start.elapsed().as_millis(),
                before,
                after
            );
        }
        match result {
            ExportEnd::Done {
                failures: f,
                files: n,
            } => {
                failures += f;
                files = n;
            }
            ExportEnd::Interrupted => {
                return Ok(Outcome::new(
                    Exit::Interrupted,
                    "Unterbrochen – die Exporte sind unvollständig",
                )
                .with_crawl(&crawled.path, Some(crawled.crawled)));
            }
        }
    }

    let mail_completion = if plan.no_store {
        crate::mail::capture_before_discard(&plan.paths.settings, &crawled.path)
    } else {
        None
    };
    if plan.no_store {
        discard(&crawled.path, rep);
    }

    rep.phase("notify");
    rep.phase("done");
    let warnings = rep.warnings();
    let base = format!(
        "{} URLs gecrawlt, {} Exportdateien, {} Warnungen",
        crawled.crawled, files, warnings
    );
    // Nach `--no-store` gibt es die Datei nicht mehr; ihre Kennung bleibt für die Historie.
    let crawl_info = |outcome: Outcome| {
        let mut outcome = outcome.with_crawl(&crawled.path, Some(crawled.crawled));
        outcome.mail_completion = mail_completion.clone();
        outcome.crawl_failed = crawl_failed.clone();
        outcome.compare_skipped = plan.compare_with_previous && crawled.failed;
        outcome
    };
    // Ein als `Failed` beendeter Crawl meldet seinen Fehler, nie „Crawl erfolgreich“ (OS-Festlegung 09.11.2026). Exit-Code wie
    // bisher (OUT-182: 0, bzw. 4 bei fehlgeschlagenen Exporten).
    if let Some(error) = &crawl_failed {
        let (exit, message) = if failures > 0 {
            (
                Exit::ExportFailed,
                format!("{error}; {failures} Export(e) fehlgeschlagen ({base})"),
            )
        } else if plan.strict && warnings > 0 {
            (
                Exit::ExportFailed,
                format!("{error}; Warnungen mit --strict als Fehler gewertet ({base})"),
            )
        } else {
            (Exit::Ok, format!("{error} ({base})"))
        };
        return Ok(crawl_info(Outcome::new(exit, message)));
    }
    if failures > 0 {
        let message = if comparison_failed {
            format!("Crawl erfolgreich, aber Auto-Vergleich fehlgeschlagen ({base})")
        } else {
            format!("Crawl erfolgreich, aber {failures} Export(e) fehlgeschlagen ({base})")
        };
        return Ok(crawl_info(Outcome::new(Exit::ExportFailed, message)));
    }
    if plan.strict && warnings > 0 {
        return Ok(crawl_info(Outcome::new(
            Exit::ExportFailed,
            format!("Warnungen mit --strict als Fehler gewertet ({base})"),
        )));
    }
    Ok(crawl_info(Outcome::new(
        Exit::Ok,
        format!("Fertig: {base}"),
    )))
}

// ---------------------------------------------------------------------------------------------------
// Laden

/// `--load-crawl` ohne `--resume`: nichts wird gecrawlt; ein unvollständiger Crawl erhält eine Warnung.
fn open_only(path: &Path, rep: &Reporter) -> Result<Crawled, CliError> {
    let file = CrawlFile::open(path, OpenMode::ReadWrite)
        .map_err(|e| CliError::usage(format!("Crawl-Datei nicht ladbar: {e}")))?;
    let meta = file.meta().map_err(store_err)?;
    let state = file
        .effective_state(&SystemClock)
        .map_err(store_err)
        .unwrap_or(meta.state);
    if state != CrawlState::Completed {
        rep.warn(format!(
            "crawl incomplete – der Crawl hat den Zustand '{}'; die Exporte zeigen den gespeicherten Stand \
             (fortsetzen mit --resume)",
            state.as_str()
        ));
    }
    let crawled = file
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE state = 'done' AND is_internal = 1",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    Ok(Crawled {
        path: path.to_path_buf(),
        crawled: u64::try_from(crawled).unwrap_or(0),
        failed: false,
    })
}

fn store_err(e: os_store::StoreError) -> CliError {
    CliError::internal(format!("Crawl-Datei: {e}"))
}

// ---------------------------------------------------------------------------------------------------
// Crawl

enum CrawlEnd {
    Done(Crawled),
    Interrupted { id: CrawlId },
}

fn deps(id: Option<CrawlId>, hook: FinishedHook) -> EngineDeps {
    let ids: Arc<dyn IdSource> = match id {
        Some(id) => Arc::new(FixedId(id)),
        None => Arc::new(RandomIds),
    };
    EngineDeps {
        clock: execution_clock(),
        ids,
        profile: StorageProfile::Balanced,
        secrets: os_fetch::secret_store_from_env(),
        sink_factory: None,
        on_finished: Some(hook),
        owner_kind: "cli",
        renderer: None,
    }
}

fn api_secrets(plan: &Plan) -> Arc<dyn SecretStore> {
    plan.api_secrets
        .as_ref()
        .map(|s| s.0.clone())
        .unwrap_or_else(|| os_fetch::secret_store_from_env())
}
fn deps_with_apis(plan: &Plan, id: Option<CrawlId>, hook: FinishedHook) -> EngineDeps {
    let mut value = deps(id, hook);
    value.secrets = api_secrets(plan);
    value
}
fn preflight_apis(plan: &Plan, rep: &Reporter) -> Result<(), CliError> {
    let ctx = os_integrations::ApiCtx {
        cfg: plan.config.clone(),
        secrets: os_integrations::crawl_glue::environment_view_for(api_secrets(plan), &plan.config),
        network: Default::default(),
        allow_api_override: false,
        quota_ledger: None,
    };
    for warning in os_integrations::crawl_glue::preflight(&ctx, plan.require_apis)
        .map_err(|e| CliError::usage(e.to_string()))?
    {
        rep.warn(warning);
    }
    Ok(())
}

/// Hook am Crawl-Ende (CRAWL-141): Die Crawl-Analyse läuft sofort danach, vor der Auflösung von `wait()`.
fn analysis_hook(rep: &Arc<Reporter>, signals: &Signals) -> FinishedHook {
    let rep = Arc::clone(rep);
    let cancel = signals.cancel_token();
    Arc::new(move |summary: &CrawlSummary| {
        rep.phase("analysis");
        let start = std::time::Instant::now();
        let wal = format!("{}-wal", summary.file.display());
        let before = std::fs::metadata(&wal).map_or(0, |m| m.len());
        let result = trigger::run_after_crawl(&summary.file, &cancel);
        if std::env::var_os("OS_SCALE_DIAG").is_some() {
            let after = std::fs::metadata(&wal).map_or(0, |m| m.len());
            eprintln!(
                "OS_SCALE_DIAG analysis duration_ms={} wal_before={} wal_after={}",
                start.elapsed().as_millis(),
                before,
                after
            );
        }
        match result {
            Ok(Some(outcome)) if !outcome.complete && !cancel.is_cancelled() => {
                rep.warn("Crawl-Analyse unvollständig – analyseabhängige Filter können leer sein");
            }
            Ok(_) => {}
            Err(e) => rep.warn(format!("Crawl-Analyse fehlgeschlagen: {e}")),
        }
    })
}

fn engine_error(e: EngineError, resuming: bool) -> CliError {
    match e {
        EngineError::Invalid(msg) if resuming && msg.contains("läuft noch") => {
            CliError::internal(format!(
                "{msg}. Ein abgestürzter Crawl lässt sich erst mehr als 10 Sekunden nach dem letzten \
                 Heartbeat fortsetzen."
            ))
        }
        EngineError::Invalid(msg) => CliError::usage(msg),
        EngineError::Join(msg) => CliError::internal(format!("interner Fehler: {msg}")),
        other => CliError::crawl(format!(
            "Der Crawl konnte nicht durchgeführt werden: {other}"
        )),
    }
}

async fn crawl_phase(
    plan: &Plan,
    rep: &Arc<Reporter>,
    signals: &Signals,
) -> Result<CrawlEnd, CliError> {
    plan.paths.create_all().map_err(|e| {
        CliError::crawl(format!(
            "Crawl-Datei nicht schreibbar – Datenordner nicht anlegbar: {e}"
        ))
    })?;
    let hook = analysis_hook(rep, signals);
    let clock = execution_clock();

    let (handle, path, id) = match &plan.source {
        Source::Load { path } => {
            // --resume
            crate::listing::validate_resume(path)?;
            let file = CrawlFile::open(path, OpenMode::ReadOnly)
                .or_else(|_| CrawlFile::open(path, OpenMode::ReadWrite))
                .map_err(|e| CliError::usage(format!("Crawl-Datei nicht ladbar: {e}")))?;
            let meta = file.meta().map_err(store_err)?;
            if matches!(meta.state, CrawlState::Failed | CrawlState::Completed) {
                let queued: i64 = file
                    .conn()
                    .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get(0))
                    .map_err(|e| CliError::crawl(e.to_string()))?;
                if meta.state == CrawlState::Failed || queued == 0 {
                    return Err(CliError::crawl("Der Crawl kann nicht fortgesetzt werden"));
                }
            }
            let stored = file.config().map_err(store_err)?;
            drop(file);
            let cfg = plan.resume_config.clone().unwrap_or(stored);
            let fetcher = plan.auth.fetcher(&cfg, clock.clone())?;
            warn_disk(path, rep);
            rep.phase("crawl");
            let handle = resume_crawl(
                fetcher,
                path.clone(),
                plan.resume_config.clone(),
                deps_with_apis(plan, None, hook),
            )
            .await
            .map_err(|e| engine_error(e, true))?;
            (handle, path.clone(), meta.crawl_id)
        }
        Source::Spider { .. } | Source::List { .. } => {
            let id = RandomIds.new_crawl_id(clock.now());
            let path = registry::path_for(&plan.paths.crawls, &id);
            let source = match &plan.source {
                Source::Spider { start_url } => CrawlSource::Spider {
                    start_url: start_url.clone(),
                },
                Source::List { items, label, .. } => CrawlSource::List {
                    items: items.clone(),
                    label: label.clone(),
                },
                Source::Load { .. } => unreachable!("oben behandelt"),
            };
            let fetcher = plan.auth.fetcher(&plan.config, clock.clone())?;
            warn_disk(&path, rep);
            rep.phase("crawl");
            let mut config = plan.config.clone();
            config.custom_js.output_dir = plan.output.clone();
            let request = CrawlRequest {
                config,
                source,
                file: path.clone(),
                project: plan.project.clone(),
                name: plan.name.clone(),
            };
            let handle = start_crawl(fetcher, request, deps_with_apis(plan, Some(id), hook))
                .await
                .map_err(|e| engine_error(e, false))?;
            (handle, path, id)
        }
    };

    let _segment_observer = if plan.config.segments.list.is_empty() {
        None
    } else {
        Some(
            os_analyze::segments::observe(path.clone(), signals.cancel_token())
                .map_err(|error| CliError::internal(format!("Segmentbeobachter: {error}")))?,
        )
    };
    let json_errors = rep.json();
    let summary = monitor(handle, signals, rep, &path, json_errors)
        .await
        .map_err(|e| engine_error(e, plan.resume))?;

    if let Source::List { originals, .. } = &plan.source {
        crate::upload::restore(&path, originals).map_err(CliError::internal)?;
    }
    match summary.state {
        CrawlState::Paused if signals.is_interrupted() => return Ok(CrawlEnd::Interrupted { id }),
        CrawlState::Paused => {
            return Err(CliError::crawl(format!(
                "Der Crawl wurde automatisch pausiert (Speicherplatz knapp) und lässt sich nach \
                 dem Freigeben von Platz mit `openspider --load-crawl {id} --resume` fortsetzen"
            )));
        }
        // Ein Lauf endet nur mit nicht erreichbarer Start-URL fehlerfrei als `Failed` (CRAWL-128); das bleibt ein gültiges
        // Ergebnis mit Warnung, `--fail-on-start-error` entscheidet über den Exit-Code (OUT-182). Andere Fehler sind `Err`.
        _ => {}
    }
    if signals.is_interrupted() {
        // Signal während der letzten Schritte: Analyse abgebrochen, keine Exporte.
        return Ok(CrawlEnd::Interrupted { id });
    }
    if summary.end_reason == Some(os_model::EndReason::LimitReached) {
        rep.info(format!(
            "Hinweis: URL-Limit erreicht; weiter crawlen mit `openspider --load-crawl {id} --resume --max-urls <n>`"
        ));
    }
    Ok(CrawlEnd::Done(Crawled {
        path,
        crawled: summary.crawled,
        failed: summary.state == CrawlState::Failed,
    }))
}

fn warn_disk(path: &Path, rep: &Reporter) {
    if let Some(w) = disk_guard::start_warning(path) {
        rep.warn(w);
    }
}

fn is_terminal(p: &Progress) -> bool {
    p.state != CrawlState::Running
}

/// Überwacht den Crawl: Fortschrittsausgabe, erstes Signal → Pause. Wartet danach auf das Ende (inkl.
/// Crawl-Analyse im Abschluss-Hook).
async fn monitor(
    handle: CrawlHandle,
    signals: &Signals,
    rep: &Reporter,
    path: &Path,
    count_errors: bool,
) -> Result<CrawlSummary, EngineError> {
    let mut progress = handle.watch();
    let mut first = signals.first_signal();
    let period = Duration::from_secs(if rep.json() { 1 } else { 5 });
    let mut tick = interval_at(Instant::now() + period, period);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut error_reader: Option<CrawlFile> = None;
    let mut paused_by_signal = false;

    if signals.is_interrupted() {
        handle.pause();
        paused_by_signal = true;
    }
    if !is_terminal(&progress.borrow()) {
        loop {
            tokio::select! {
                changed = first.changed(), if !paused_by_signal => {
                    if changed.is_ok() && *first.borrow() {
                        handle.pause();
                        paused_by_signal = true;
                    }
                }
                _ = tick.tick() => {
                    let snapshot = handle.progress();
                    let errors = count_errors.then(|| error_count(&mut error_reader, path));
                    rep.progress("crawl", &snapshot, errors.flatten());
                }
                changed = progress.changed() => {
                    if changed.is_err() || is_terminal(&progress.borrow()) {
                        break;
                    }
                }
            }
        }
    }
    rep.snapshot(
        &handle.progress(),
        count_errors
            .then(|| error_count(&mut error_reader, path))
            .flatten(),
    );
    drop(error_reader);
    handle.wait().await
}

/// Anzahl der URLs mit Fehlerstatus (0 oder ≥ 400) für `--json-progress`.
fn error_count(reader: &mut Option<CrawlFile>, path: &Path) -> Option<u64> {
    if reader.is_none() {
        *reader = CrawlFile::open(path, OpenMode::ReadOnly).ok();
    }
    let file = reader.as_ref()?;
    let n: i64 = file
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE status_code = 0 OR status_code >= 400",
            [],
            |r| r.get(0),
        )
        .ok()?;
    u64::try_from(n).ok()
}

/// Vermerk, wenn der Auto-Vergleich wegen eines fehlgeschlagenen Crawls entfällt (OS-Festlegung 09.11.2026).
pub const COMPARE_SKIPPED_FAILED: &str = "Auto-Vergleich übersprungen (Crawl fehlgeschlagen)";

/// Start-URL ohne Antwort (Status 0).
enum StartProblem {
    /// Nicht erreichbar bzw. ohne gültige Antwort, als vollständige Meldung; mit `--fail-on-start-error` Exit 3 (OUT-182).
    Unreachable(String),
    /// Per robots.txt gesperrt und kein Fehlschlag (CRAWL-128): robots.txt-URL und sperrende Zeile bzw. Grund.
    Robots(String),
}

/// Meldet, wenn die Start-URL keine Antwort lieferte (Statuscode 0). Endete der Crawl als `Failed`, nennt die Meldung
/// Fehlerklasse, betroffene Adresse und Ursache aus der Crawl-Datei (wie App und Planer, OUT-281).
fn start_url_problem(path: &Path, start_url: &str, failed: bool) -> Option<StartProblem> {
    let file = CrawlFile::open(path, OpenMode::ReadOnly).ok()?;
    if let Some(block) = os_engine::start_robots_block(&file, start_url) {
        let mut detail = os_fetch::redact::url(&block.robots_url);
        match (block.line_number, block.rule) {
            (Some(number), Some(rule)) => detail.push_str(&format!(", Zeile {number}: {rule}")),
            (None, Some(rule)) => detail.push_str(&format!(", Regel: {rule}")),
            _ => {}
        }
        return Some(StartProblem::Robots(detail));
    }
    if failed {
        if let Some(failure) = os_engine::start_failure(&file, start_url) {
            return Some(StartProblem::Unreachable(failure.describe(start_url)));
        }
    } else if let Some(robots) = os_engine::start_robots_unavailable(&file, start_url) {
        // Regel „robots.txt nicht erreichbar“, die robots.txt antwortete aber (5xx, 429): kein Fehlschlag; Grund = HTTP-Status.
        let reason = match (robots.status_code, robots.reason) {
            (Some(code), _) => format!("HTTP {code}"),
            (None, Some(reason)) => reason,
            (None, None) => "unbekannt".to_string(),
        };
        return Some(StartProblem::Robots(format!(
            "robots.txt {} nicht abrufbar (Grund: {reason})",
            os_fetch::redact::url(&robots.robots_url)
        )));
    }
    let row = queries::url_by_address(file.conn(), start_url).ok()??;
    if row.status_code != Some(0) {
        return None;
    }
    let status = row.status.unwrap_or_else(|| "keine Antwort".to_string());
    Some(StartProblem::Unreachable(format!(
        "Start-URL {} nicht erreichbar: {status}",
        os_fetch::redact::url(start_url)
    )))
}

// ---------------------------------------------------------------------------------------------------
// Exporte

enum ExportEnd {
    Done { failures: u32, files: usize },
    Interrupted,
}

fn describe(item: &Resolved) -> String {
    match item {
        Resolved::Tab { tab, filter } => format!("{tab}:{filter}"),
        Resolved::CompareTab { tab, filter, class } => {
            format!("{tab}:{filter}:{}", class.as_str())
        }
        Resolved::Bulk(n) | Resolved::Report(n) => n.clone(),
        Resolved::Issues => "Issues:All Issues".to_string(),
    }
}

/// Unterfilter-Export ohne vorhandenen Vergleich (erster Projektcrawl, OUT-151/152): Warnung, keine Datei.
pub(crate) fn skip_compare_tab(item: &Resolved, rep: &Reporter) {
    rep.warn(format!(
        "Export '{}' übersprungen: kein Vergleich vorhanden (OUT-151)",
        describe(item)
    ));
}

fn report_file(rep: &Reporter, info: &ExportFileInfo) {
    rep.note_export(&info.path);
    rep.info(format!(
        "Export: {} ({} Zeilen)",
        info.path.display(),
        info.rows
    ));
    for w in &info.warnings {
        rep.warn(w);
    }
}

async fn export_phase(
    plan: &Plan,
    rep: &Reporter,
    signals: &Signals,
    crawled: &Crawled,
) -> ExportEnd {
    let Some(out) = plan.output.as_deref() else {
        return ExportEnd::Done {
            failures: 0,
            files: 0,
        };
    };
    let final_out = out;
    let segment_workbook = plan.segment.is_some()
        && plan.batch.is_some_and(|b| b.single_workbook)
        && (plan.format == Format::Xlsx || plan.google.is_some());
    let staging = if (plan.xls || plan.google.is_some()) && !segment_workbook {
        match crate::staging::Staging::new(out) {
            Ok(dir) => Some(dir),
            Err(e) => {
                rep.error(format!("Export-Zwischenspeicher: {e}"));
                return ExportEnd::Done {
                    failures: 1,
                    files: 0,
                };
            }
        }
    } else {
        None
    };
    let out = staging.as_ref().map(|s| s.path.as_path()).unwrap_or(out);
    rep.phase("export");
    let file = match CrawlFile::open(&crawled.path, OpenMode::ReadWrite) {
        Ok(f) => f,
        Err(e) => {
            rep.error(format!("Crawl-Datei für die Exporte nicht lesbar: {e}"));
            return ExportEnd::Done {
                failures: 1,
                files: 0,
            };
        }
    };
    let cfg = match file.config() {
        Ok(c) => c,
        Err(e) => {
            rep.error(format!("Konfiguration der Crawl-Datei nicht lesbar: {e}"));
            return ExportEnd::Done {
                failures: 1,
                files: 0,
            };
        }
    };

    let mut failures = 0u32;
    let mut files = 0usize;

    if !plan.exports.is_empty() {
        let pending = !matches!(persist::read_run_state(&file), Ok(RunState::Completed));
        if pending {
            rep.info(
                "Hinweis: Die Crawl-Analyse ist nicht aktuell – sie wird vor den Exporten ausgeführt, \
                 damit analyseabhängige Filter nicht leer bleiben",
            );
        }
        rep.phase("analysis");
        match ensure_analysis(&file, &cfg, &signals.cancel_token()) {
            Ok(outcome) if !outcome.complete && !signals.is_interrupted() => {
                rep.warn("Crawl-Analyse unvollständig – analyseabhängige Filter können leer sein");
            }
            Ok(_) => {}
            Err(e) => rep.warn(format!("Crawl-Analyse fehlgeschlagen: {e}")),
        }
        rep.phase("export");
    }

    // CRAWL-134: loading without --resume has no observer; analysis can change membership.
    if let Err(error) = os_analyze::segments::refresh_if_stale(&file, &cfg, &signals.cancel_token())
    {
        rep.error(format!("Segmentzuordnung fehlgeschlagen: {error}"));
        return ExportEnd::Done {
            failures: 1,
            files: 0,
        };
    }

    // Initialize configured segments once; subsequent exports only consume the saved assignment.
    if !cfg.segments.list.is_empty() && matches!(file.get_meta("segments.definitions"), Ok(None)) {
        if let Err(error) = os_analyze::segments::reassign(&file, &cfg, &signals.cancel_token()) {
            rep.error(format!("Segmentzuordnung fehlgeschlagen: {error}"));
            return ExportEnd::Done {
                failures: 1,
                files: 0,
            };
        }
    }

    if segment_workbook {
        match crate::segment_workbook::export(&file, &cfg, plan, out, signals, rep).await {
            Ok(n) => files += n,
            Err(_) if signals.is_interrupted() => return ExportEnd::Interrupted,
            Err(e) => {
                failures += 1;
                rep.error(e);
            }
        }
    }
    if let (Some(batch), None) = (plan.batch, plan.segment.as_deref()) {
        if !plan.exports.is_empty() {
            match run_batch(&file, &cfg, plan, batch, out, signals, rep) {
                Ok(n) => files += n,
                Err(BatchEnd::Interrupted) => return ExportEnd::Interrupted,
                Err(BatchEnd::Failed(message)) => {
                    failures += 1;
                    rep.error(format!("Exporte fehlgeschlagen: {message}"));
                }
            }
        }
    }
    let one_by_one: &[Resolved] =
        if (plan.batch.is_some() && plan.segment.is_none()) || segment_workbook {
            &[]
        } else {
            &plan.exports
        };
    for item in one_by_one {
        if signals.is_interrupted() {
            return ExportEnd::Interrupted;
        }
        if plan.compare_with_previous || plan.explicit_previous.is_some() {
            if let Some(result) =
                comparison_export(&file, &cfg, item, out, plan, &signals.cancel_token(), rep)
            {
                match result {
                    Ok(infos) => {
                        for info in infos {
                            report_file(rep, &info);
                            files += 1;
                        }
                    }
                    Err(message) => {
                        failures += 1;
                        rep.error(message);
                    }
                }
                continue;
            }
        }
        if plan.segment.is_some() {
            match run_segment_export(&file, &cfg, item, out, plan, rep) {
                Ok(n) => files += n,
                Err(message) => {
                    failures += 1;
                    rep.error(format!(
                        "Export '{}' fehlgeschlagen: {message}",
                        describe(item)
                    ));
                }
            }
            continue;
        }
        match run_export(
            &file,
            &cfg,
            item,
            out,
            plan.format,
            rep,
            &plan.header_labels,
        ) {
            Ok(n) => files += n,
            Err(message) => {
                failures += 1;
                rep.error(format!(
                    "Export '{}' fehlgeschlagen: {message}",
                    describe(item)
                ));
            }
        }
    }

    if !plan.crawl_path_urls.is_empty() {
        let result = export_crawl_paths(&file, &cfg, plan, out, rep);
        match result {
            Ok(info) => {
                report_file(rep, &info);
                files += 1;
            }
            Err(message) => {
                failures += 1;
                rep.error(format!("Crawl Path Report: {message}"));
            }
        }
    }

    if let Some(staging) = &staging {
        let staged: Vec<_> = rep
            .exported_files()
            .into_iter()
            .filter(|p| p.starts_with(&staging.path))
            .collect();
        let mut csv = Vec::new();
        files = 0;
        for path in staged {
            if path.extension().is_some_and(|e| e == "csv") && plan.google.is_some() {
                csv.push(path);
                continue;
            }
            let relative = path.strip_prefix(&staging.path).expect("staging path");
            let mut destination = final_out.join(relative);
            if let Some(parent) = destination.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    failures += 1;
                    rep.error(e.to_string());
                    continue;
                }
            }
            let result = if plan.xls && path.extension().is_some_and(|e| e == "csv") {
                destination.set_extension("xls");
                crate::xls::convert_with(&path, &destination, &signals.cancel_token()).map(
                    |warnings| {
                        for w in warnings {
                            rep.warn(w);
                        }
                    },
                )
            } else {
                std::fs::copy(&path, &destination).map(|_| ())
            };
            match result {
                Ok(()) => {
                    files += 1;
                    rep.note_export(&destination);
                    rep.info(format!("Export: {}", destination.display()));
                }
                Err(e) => {
                    failures += 1;
                    rep.error(format!("Export: {e}"));
                }
            }
        }
        if let Some(target) = &plan.google {
            let upload = match target.client(&plan.config, &plan.paths) {
                Ok(client) => {
                    crate::google::upload_at(
                        &client,
                        &plan.project,
                        &plan.name,
                        &csv,
                        plan.batch.is_some_and(|b| b.single_workbook),
                        target.timestamp.as_deref(),
                        &signals.cancel_token(),
                        rep,
                    )
                    .await
                }
                Err(e) => Err(e.message),
            };
            match upload {
                Ok(n) => files += n,
                Err(e) => {
                    failures += 1;
                    rep.error(e);
                }
            }
        }
        rep.forget_staging(&staging.path);
    }
    let out = final_out;
    if signals.is_interrupted() {
        return ExportEnd::Interrupted;
    }
    for images in [false, true] {
        if if images {
            !plan.create_images_sitemap
        } else {
            !plan.create_sitemap
        } {
            continue;
        }
        let opts = os_export::types::SitemapOptions::default();
        let result = if images {
            os_export::sitemap_gen::generate_image_sitemap_with(
                &file,
                &opts,
                &cfg,
                None,
                &SystemClock,
                out,
            )
        } else {
            os_export::sitemap_gen::generate_sitemap_configured_with(
                &file,
                &opts,
                &cfg,
                None,
                &SystemClock,
                out,
            )
        };
        match result {
            Ok(report) => {
                files += report.files.len();
                report.files.iter().for_each(|info| report_file(rep, info));
            }
            Err(e) => {
                failures += 1;
                rep.error(format!("Sitemap: {e}"));
            }
        }
    }
    if plan.save_crawl {
        if signals.is_interrupted() {
            return ExportEnd::Interrupted;
        }
        let dest = out.join("crawl.ospider");
        if dest.exists() {
            let _ = std::fs::remove_file(&dest);
        }
        match file.save_as(&dest) {
            Ok(()) => {
                files += 1;
                rep.info(format!("Crawl gespeichert: {}", dest.display()));
            }
            Err(e) => {
                failures += 1;
                rep.error(format!("Crawl speichern fehlgeschlagen: {e}"));
            }
        }
    }
    ExportEnd::Done { failures, files }
}

fn export_crawl_paths(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    plan: &Plan,
    out: &Path,
    rep: &Reporter,
) -> Result<ExportFileInfo, String> {
    let mut ids = Vec::new();
    let mut stmt = file
        .conn()
        .prepare("SELECT id FROM urls WHERE url = ?1")
        .map_err(|e| e.to_string())?;
    for url in &plan.crawl_path_urls {
        let mut rows = stmt.query([url]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            ids.push(os_model::UrlId(row.get(0).map_err(|e| e.to_string())?));
        } else {
            rep.warn(format!("Crawl Path: URL nicht im Crawl: {url}"));
        }
    }
    let params = serde_json::json!({});
    let req = os_analyze::details::DetailReq {
        url_ids: &ids,
        params: &params,
        sort: &[],
        offset: 0,
        limit: u64::MAX,
    };
    let job = os_export::catalog::TableJob {
        entry_name: "Crawl Path Report",
        tabs: &[],
        file,
        cfg,
    };
    let extension = if plan.format == Format::Xlsx {
        "xlsx"
    } else {
        "csv"
    };
    os_export::reports::crawl_path::export_detail_with_options(
        &job,
        &req,
        &out.join(format!("crawl_path_report.{extension}")),
        plan.format,
        &labeled_writer_options(cfg, &plan.header_labels),
    )
    .map_err(|e| e.to_string())
}

fn run_export(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    item: &Resolved,
    out: &Path,
    format: Format,
    rep: &Reporter,
    labels: &std::collections::BTreeMap<String, String>,
) -> Result<usize, String> {
    let opts = labeled_writer_options(cfg, labels);
    match item {
        Resolved::CompareTab { .. } => {
            skip_compare_tab(item, rep);
            Ok(0)
        }
        Resolved::Tab { tab, filter } => {
            let query = ViewQuery::new(tab.clone(), filter.clone());
            let cancel = os_model::CancelToken::new();
            let mut ctx = os_export::types::ExportCtx {
                cancel: &cancel,
                progress: &|_| {},
                skip_empty: false,
                workbook: None,
                skipped: Vec::new(),
            };
            let info = os_export::tab_export::export_tab_with_writer_options(
                file, cfg, &query, out, format, true, &mut ctx, &opts,
            )
            .map_err(|e| e.to_string())?;
            if let Some(info) = info {
                report_file(rep, &info);
                Ok(1)
            } else {
                Ok(0)
            }
        }
        Resolved::Bulk(name) => {
            let infos =
                os_export::pipeline::export_bulk_with_options(file, cfg, name, out, format, &opts)
                    .map_err(|e| e.to_string())?;
            infos.iter().for_each(|i| report_file(rep, i));
            Ok(infos.len())
        }
        Resolved::Issues => {
            let infos = bulk_issues::export_with_options(file, cfg, out, format, &opts)
                .map_err(|e| e.to_string())?;
            infos.iter().for_each(|i| report_file(rep, i));
            Ok(infos.len())
        }
        Resolved::Report(name) => {
            let result =
                os_export::reports::save_report_with_options(file, cfg, name, out, format, &opts)
                    .map_err(|e| e.to_string())?;
            result.files.iter().for_each(|i| report_file(rep, i));
            for w in &result.warnings {
                rep.warn(w);
            }
            Ok(result.files.len())
        }
    }
}

enum BatchEnd {
    Interrupted,
    Failed(String),
}

/// Geplante Aufgaben: alle Tabellen in einem Stapel gegen denselben Lesestand (OUT-007, OUT-012).
fn run_batch(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    plan: &Plan,
    batch: crate::plan::BatchSettings,
    out: &Path,
    signals: &Signals,
    rep: &Reporter,
) -> Result<usize, BatchEnd> {
    let opts = labeled_writer_options(cfg, &plan.header_labels);
    if plan.explicit_previous.is_some()
        || (plan.compare_with_previous && file.get_meta("compare.latest").ok().flatten().is_some())
    {
        return comparison_batch(file, cfg, plan, batch, out, signals, rep);
    }
    let selections: Vec<os_export::types::ExportSelection> = plan
        .exports
        .iter()
        .filter_map(|item| {
            let (kind, name) = match item {
                Resolved::Tab { tab, filter } => ("tab", format!("{tab}:{filter}")),
                Resolved::CompareTab { .. } => {
                    skip_compare_tab(item, rep);
                    return None;
                }
                Resolved::Bulk(name) => ("bulk", name.clone()),
                Resolved::Issues => ("bulk", os_export::catalog::bulk_issues::BULK_NAME.into()),
                Resolved::Report(name) => ("report", name.clone()),
            };
            Some(os_export::types::ExportSelection {
                kind: kind.into(),
                name,
            })
        })
        .collect();
    let token = signals.cancel_token();
    let crawl_name = file
        .meta()
        .map(|m| m.name)
        .ok()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "crawl".to_string());
    let mut ctx = os_export::types::ExportCtx {
        cancel: &token,
        progress: &|_| {},
        skip_empty: batch.skip_empty,
        workbook: None,
        skipped: Vec::new(),
    };
    let options = os_export::pipeline::BatchOptions {
        format: plan.format,
        directory: out,
        crawl_name: &crawl_name,
        single_workbook: batch.single_workbook && plan.google.is_none(),
    };
    match os_export::pipeline::export_batch_with_options(
        file,
        cfg,
        &selections,
        &options,
        &mut ctx,
        &opts,
    ) {
        Ok(result) => {
            for info in &result.files {
                report_file(rep, info);
            }
            for warning in &result.warnings {
                rep.warn(warning);
            }
            for name in &result.skipped {
                rep.warn(format!("Export übersprungen: {name}"));
            }
            Ok(result.files.len())
        }
        Err(_) if signals.is_interrupted() => Err(BatchEnd::Interrupted),
        Err(e) => Err(BatchEnd::Failed(e.to_string())),
    }
}

/// `--no-store`: löscht die Crawl-Datei samt `-wal`/`-shm`.
fn discard(path: &Path, rep: &Reporter) {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        let candidate = PathBuf::from(name);
        if candidate.exists() {
            if let Err(e) = std::fs::remove_file(&candidate) {
                rep.warn(format!(
                    "Crawl-Datei {} nicht löschbar: {e}",
                    candidate.display()
                ));
            }
        }
    }
}

/// Segment-aware companion to the unfiltered export dispatcher (OUT-011).
fn run_segment_export(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    item: &Resolved,
    out: &Path,
    plan: &Plan,
    rep: &Reporter,
) -> Result<usize, String> {
    let opts = labeled_writer_options(cfg, &plan.header_labels);
    let (kind, name) = match item {
        Resolved::Tab { tab, filter } => ("tab", format!("{tab}:{filter}")),
        Resolved::CompareTab { .. } => {
            skip_compare_tab(item, rep);
            return Ok(0);
        }
        Resolved::Bulk(name) => ("bulk", name.clone()),
        Resolved::Issues => ("bulk", os_export::catalog::bulk_issues::BULK_NAME.into()),
        Resolved::Report(name) => ("report", name.clone()),
    };
    let selection = os_export::types::ExportSelection {
        kind: kind.into(),
        name,
    };
    let skip_empty = plan.batch.is_some_and(|settings| settings.skip_empty);
    let staging = skip_empty
        .then(|| crate::staging::Staging::new(out))
        .transpose()
        .map_err(|e| e.to_string())?;
    let target = staging.as_ref().map(|s| s.path.as_path()).unwrap_or(out);
    let files = os_export::segment::export_all_with_options(
        file,
        cfg,
        &selection,
        plan.segment.as_deref(),
        target,
        plan.format,
        &opts,
    )
    .map_err(|e| e.to_string())?;
    let mut count = 0;
    for mut info in files {
        if skip_empty && info.rows == 0 {
            rep.info(format!(
                "Leerer Export übersprungen: {}",
                info.path.display()
            ));
            continue;
        }
        if let Some(stage) = &staging {
            let relative = info
                .path
                .strip_prefix(&stage.path)
                .map_err(|e| e.to_string())?;
            let destination = out.join(relative);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&info.path, &destination).map_err(|e| e.to_string())?;
            info.path = destination;
        }
        report_file(rep, &info);
        count += 1;
    }
    Ok(count)
}

/// Vergleichs-Tabellen verwenden die gespeicherte Zuordnung und Ergebnisse (OUT-152).
fn comparison_export(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    item: &Resolved,
    out: &Path,
    plan: &Plan,
    cancel: &os_model::CancelToken,
    rep: &Reporter,
) -> Option<Result<Vec<ExportFileInfo>, String>> {
    let opts = labeled_writer_options(cfg, &plan.header_labels);
    let format = plan.format;
    let segment = plan.segment.as_deref();
    let explicit_previous = plan.explicit_previous.as_deref();
    if !matches!(
        item,
        Resolved::Tab { .. } | Resolved::CompareTab { .. } | Resolved::Report(_)
    ) {
        return None;
    }
    if explicit_previous.is_none() && file.get_meta("compare.latest").ok().flatten().is_none() {
        return None;
    }
    Some((|| {
        let scopes = os_export::segment::selections(file, segment).map_err(|e| e.to_string())?;
        if let Resolved::Report(name) = item {
            let mut files = Vec::new();
            for scope in scopes {
                // A fresh comparison keeps filter caches local to this segment.
                let (mut c, _) = load_comparison(
                    file,
                    explicit_previous,
                    plan.explicit_compare_config.as_ref(),
                    cancel,
                )?;
                let report = if let Some(scope) = scope {
                    os_export::compare::save_report_in_segment_with_options(
                        &mut c, name, &scope, out, format, cancel, &opts,
                    )
                } else {
                    os_export::compare::save_report_with_options(
                        &mut c, name, out, format, cancel, &opts,
                    )
                }
                .map_err(|e| e.to_string())?;
                for warning in report.warnings {
                    // File warnings are emitted by report_file at the call site.
                    if !report
                        .files
                        .iter()
                        .any(|info| info.warnings.contains(&warning))
                    {
                        rep.warn(warning);
                    }
                }
                files.extend(report.files);
            }
            return Ok(files);
        }
        let (mut c, config) = load_comparison(
            file,
            explicit_previous,
            plan.explicit_compare_config.as_ref(),
            cancel,
        )?;
        let Some((tab, filter, class)) = item.tab_parts() else {
            unreachable!()
        };
        let (tab, filter) = (tab.to_string(), filter.to_string());
        let mut files = Vec::new();
        for scope in scopes {
            let mut q = ViewQuery::new(&tab, &filter);
            let name = describe(item);
            let target = out.join(os_export::naming::file_name(
                &name,
                scope.as_ref().and_then(|s| s.name.as_deref()),
                format.extension(),
            ));
            let mut writer =
                os_export::open_writer(&target, format, &name, &opts).map_err(|e| e.to_string())?;
            let mut ctx = os_export::types::ExportCtx {
                cancel,
                progress: &|_| {},
                skip_empty: false,
                workbook: None,
                skipped: vec![],
            };
            if tab == "Change Detection" {
                let mut def = os_analyze::compare::change::tab_for_query(
                    &config.change_detection,
                    true,
                    &mut q,
                );
                if let Some(scope) = &scope {
                    def.population = os_analyze::defs::intern(&format!(
                        "({}) AND ({})",
                        def.population,
                        scope.population_sql()
                    ));
                }
                os_export::pipeline::write_view_to(
                    &[def],
                    &c.current().file,
                    cfg,
                    &q,
                    false,
                    writer.as_mut(),
                )
                .map_err(|e| e.to_string())?;
            } else {
                let spec = os_export::compare::TabSelection {
                    tab: tab.clone(),
                    filter: filter.clone(),
                    class,
                    columns: q.columns,
                };
                if let Some(scope) = &scope {
                    os_export::compare::write_tab_in_segment(
                        &mut c,
                        &spec,
                        scope,
                        writer.as_mut(),
                        &mut ctx,
                    )
                } else {
                    os_export::compare::write_tab(&mut c, &spec, writer.as_mut(), &mut ctx)
                }
                .map_err(|e| e.to_string())?;
            }
            files.push(writer.finish().map_err(|e| e.to_string())?);
        }
        Ok(files)
    })())
}

fn load_comparison(
    file: &CrawlFile,
    previous: Option<&Path>,
    override_config: Option<&os_model::CompareConfig>,
    cancel: &os_model::CancelToken,
) -> Result<(os_analyze::compare::Comparison, os_model::CompareConfig), String> {
    if let Some(previous) = previous {
        use os_analyze::compare::{change, Comparison, CrawlSide};
        let work = file.path().parent().ok_or("Crawl directory missing")?;
        let a = CrawlSide::open(file.path(), work).map_err(|e| e.to_string())?;
        let b = CrawlSide::open(previous, work).map_err(|e| e.to_string())?;
        let config = override_config.unwrap_or(&a.config.compare).clone();
        let c = Comparison::open(a, b, &config.mapping, cancel).map_err(|e| e.to_string())?;
        change::calculate(&c, &config.change_detection, cancel).map_err(|e| e.to_string())?;
        Ok((c, config))
    } else {
        let path = file
            .get_meta("compare.latest")
            .map_err(|e| e.to_string())?
            .ok_or("comparison missing")?;
        os_analyze::compare::change::auto::load(Path::new(&path), cancel).map_err(|e| e.to_string())
    }
}

/// Scheduling verwendet dieselben Vergleichs-Tabellen auch in Arbeitsmappen (OUT-152).
fn comparison_batch(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    plan: &Plan,
    batch: crate::plan::BatchSettings,
    out: &Path,
    signals: &Signals,
    rep: &Reporter,
) -> Result<usize, BatchEnd> {
    let opts = labeled_writer_options(cfg, &plan.header_labels);
    use os_export::types::TableWriter;
    let run = || -> Result<usize, String> {
        let token = signals.cancel_token();
        let (mut comparison, config) = load_comparison(
            file,
            plan.explicit_previous.as_deref(),
            plan.explicit_compare_config.as_ref(),
            &token,
        )?;
        let name = file.meta().map_err(|e| e.to_string())?.name;
        let destination = out.join(os_export::naming::file_name(
            &format!("{name} export"),
            None,
            "xlsx",
        ));
        let mut workbook = if batch.single_workbook && plan.format == Format::Xlsx {
            let mut w = os_export::xlsx::XlsxWriter::create(&destination, "Export", &opts)
                .map_err(|e| e.to_string())?;
            w.set_cancel(token.clone());
            Some(w)
        } else {
            None
        };
        let mut files = 0;
        for item in &plan.exports {
            if token.is_cancelled() {
                return Err("Export cancelled".into());
            }
            let mut ctx = os_export::types::ExportCtx {
                cancel: &token,
                progress: &|_| {},
                skip_empty: batch.skip_empty,
                workbook: workbook
                    .as_mut()
                    .map(|w| w as &mut dyn os_export::types::WorkbookTarget),
                skipped: vec![],
            };
            if let Some((tab, filter, class)) = item.tab_parts() {
                let (tab, filter) = (tab.to_string(), filter.to_string());
                let label = describe(item);
                let target = out.join(os_export::naming::file_name(
                    &label,
                    None,
                    plan.format.extension(),
                ));
                let result = if tab == "Change Detection" {
                    let mut q = ViewQuery::new(tab, filter);
                    let def = os_analyze::compare::change::tab_for_query(
                        &config.change_detection,
                        true,
                        &mut q,
                    );
                    os_export::pipeline::export_view_with_writer_options(
                        &[def],
                        &comparison.current().file,
                        cfg,
                        &q,
                        &target,
                        plan.format,
                        &label,
                        false,
                        &mut ctx,
                        &opts,
                    )
                } else {
                    os_export::output::write_table(
                        &target,
                        plan.format,
                        &label,
                        &opts,
                        &mut ctx,
                        |writer| {
                            let selection = os_export::compare::TabSelection {
                                tab: tab.clone(),
                                filter: filter.clone(),
                                class,
                                columns: os_analyze::views::ColumnSelection::Default,
                            };
                            let mut inner = os_export::types::ExportCtx {
                                cancel: &token,
                                progress: &|_| {},
                                skip_empty: false,
                                workbook: None,
                                skipped: vec![],
                            };
                            os_export::compare::write_tab(
                                &mut comparison,
                                &selection,
                                writer,
                                &mut inner,
                            )
                            .map(|_| ())
                        },
                    )
                }
                .map_err(|e| e.to_string())?;
                if let Some(info) = result {
                    report_file(rep, &info);
                    files += 1;
                }
            } else {
                let (kind, name) = match item {
                    Resolved::Bulk(n) => ("bulk", n.clone()),
                    Resolved::Issues => ("bulk", os_export::catalog::bulk_issues::BULK_NAME.into()),
                    Resolved::Report(n) => ("report", n.clone()),
                    _ => unreachable!(),
                };
                let selection = os_export::types::ExportSelection {
                    kind: kind.into(),
                    name: name.clone(),
                };
                let options = os_export::pipeline::BatchOptions {
                    format: plan.format,
                    directory: out,
                    crawl_name: &name,
                    single_workbook: false,
                };
                let result = os_export::pipeline::export_batch_with_options(
                    file,
                    cfg,
                    &[selection],
                    &options,
                    &mut ctx,
                    &opts,
                )
                .map_err(|e| e.to_string())?;
                for info in &result.files {
                    report_file(rep, info);
                }
                files += result.files.len();
                for warning in result.warnings {
                    rep.warn(warning);
                }
            }
        }
        if let Some(workbook) = workbook {
            if workbook.has_tables() {
                let info = Box::new(workbook).finish().map_err(|e| e.to_string())?;
                report_file(rep, &info);
                files += 1;
            }
        }
        Ok(files)
    };
    match run() {
        Ok(n) => Ok(n),
        Err(_) if signals.is_interrupted() => Err(BatchEnd::Interrupted),
        Err(e) => Err(BatchEnd::Failed(e)),
    }
}

#[cfg(test)]
#[path = "../../os-analyze/src/compare/change/cli_tests.rs"]
mod change_tests;

fn labeled_writer_options(
    cfg: &CrawlConfig,
    labels: &std::collections::BTreeMap<String, String>,
) -> os_export::types::WriterOptions {
    let mut opts = os_export::pipeline::writer_options(cfg);
    opts.header_labels = labels.clone();
    opts
}

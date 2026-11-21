//! Runner der Teilschritte (CRAWL-141): Schalter, Datengrundlage, Parallelität, Abbruch, Persistenz.
//!
//! Ablauf von [`run`]:
//! 1. Abgewählte Teilschritte (Schalter in `analysis.*` aus) → `Skipped`; Teilschritte ohne Datengrundlage
//!    (Bilder ohne JS-Rendering, Analytics/Search Console/Linkmetriken ohne Anbindung) → `NotApplicable`.
//! 2. Die übrigen Teilschritte sind voneinander unabhängig und laufen parallel, jeder auf einer eigenen
//!    Verbindung zur Crawl-Datei (SQLite erlaubt einen Schreiber zugleich; kurze Sperren löst `busy_timeout`).
//! 3. Jeder Schritt wird vor dem Start auf „ausstehend“ gesetzt und erst nach seinem Ende als `Done`
//!    gespeichert. Ein Abbruch lässt fertige Schritte gelten, abgebrochene bleiben ausstehend – es gibt nie
//!    Teilergebnisse als Endstand.
//! 4. Ergebnis, `analysis_state` und `analysis_at` stehen danach in der Crawl-Datei (`persist`).
//!
//! Ein Schritt meldet `Ok(StepState::Pending)`, wenn er wegen des Abbruchs nicht fertig wurde.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Barrier, Mutex};

use os_model::{AnalysisStep, Clock, CrawlConfig, RenderingMode, StepState, SystemClock};
use os_store::{CrawlFile, OpenMode};

use super::persist::{self, RunState};
use super::{AnalysisOutcome, CancelToken};
use crate::error::AnalyzeError;
use crate::state::step_config_hash;

/// Signatur eines Teilschritts (`steps::<name>::run`).
pub type StepFn = fn(&CrawlFile, &CrawlConfig, &CancelToken) -> Result<StepState, AnalyzeError>;

/// Standardzuordnung Teilschritt → Funktion. Schritte ohne eigene Umsetzung sind `NotApplicable`.
pub fn default_registry(step: AnalysisStep) -> StepFn {
    match step {
        AnalysisStep::Embeddings => super::steps::embeddings::run,
        AnalysisStep::AiSearch => super::steps::ai_search::run,
        AnalysisStep::Images => super::steps::images::run,
        AnalysisStep::Canonicals => super::steps::canonicals::run,
        AnalysisStep::Sitemaps => super::steps::sitemaps::run,
        // Stubs von M2 (M2-F02); die Fachpakete ersetzen die Funktion in ihrer Datei, diese Zeile bleibt.
        AnalysisStep::NearDuplicates => super::steps::near_duplicates::run,
        AnalysisStep::LinkScore => super::steps::link_score::run,
        AnalysisStep::Pagination => super::steps::pagination::run,
        AnalysisStep::Hreflang => super::steps::hreflang::run,
        AnalysisStep::Links => super::steps::links::run,
        _ => not_applicable,
    }
}

fn not_applicable(
    _file: &CrawlFile,
    _cfg: &CrawlConfig,
    _cancel: &CancelToken,
) -> Result<StepState, AnalyzeError> {
    Ok(StepState::NotApplicable)
}

/// Optionen eines Laufs; [`RunOptions::default`] entspricht [`run`].
#[derive(Clone, Copy)]
pub struct RunOptions<'a> {
    /// Uhr für `finished_at` und `analysis_at`.
    pub clock: &'a dyn Clock,
    /// Nur diese Teilschritte ausführen (`None` = alle).
    pub only: Option<&'a [AnalysisStep]>,
    /// Zuordnung Teilschritt → Funktion (Tests setzen eigene Schritte ein).
    pub registry: fn(AnalysisStep) -> StepFn,
    /// Höchstzahl gleichzeitig laufender Teilschritte (`0` = Anzahl der CPU-Kerne).
    pub max_parallel: usize,
}

static SYSTEM_CLOCK: SystemClock = SystemClock;

impl Default for RunOptions<'_> {
    fn default() -> Self {
        RunOptions {
            clock: &SYSTEM_CLOCK,
            only: None,
            registry: default_registry,
            max_parallel: 0,
        }
    }
}

/// Führt die Teilschritte aus und meldet den Fortschritt je Schritt (0.0 beim Start, 1.0 am Ende).
pub fn run(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    cancel: &CancelToken,
    progress: &dyn Fn(AnalysisStep, f32),
) -> Result<AnalysisOutcome, AnalyzeError> {
    run_with(file, cfg, cancel, progress, &RunOptions::default())
}

/// Führt nur die genannten Teilschritte aus (z. B. die ausstehenden oder veralteten, `ensure_analysis`).
pub fn run_only(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    cancel: &CancelToken,
    progress: &dyn Fn(AnalysisStep, f32),
    only: &[AnalysisStep],
) -> Result<AnalysisOutcome, AnalyzeError> {
    let opts = RunOptions {
        only: Some(only),
        ..RunOptions::default()
    };
    run_with(file, cfg, cancel, progress, &opts)
}

/// Vorab entscheidbarer Zustand eines Teilschritts: `Some` = nicht ausführen.
pub fn predetermined_state(step: AnalysisStep, cfg: &CrawlConfig) -> Option<StepState> {
    let js = cfg.rendering.mode == RenderingMode::JavaScript;
    // Bilder sind ohne JS-Rendering nicht wählbar (CRAWL-141): nicht anwendbar, auch bei gesetztem Schalter.
    if step == AnalysisStep::Images && !js && !cfg.images.parse_inline_css_backgrounds {
        return Some(StepState::NotApplicable);
    }
    if !cfg.analysis.step_enabled(step) {
        return Some(StepState::Skipped);
    }
    match step {
        // Keine Anbindung in M1: keine Datengrundlage.
        AnalysisStep::Analytics | AnalysisStep::SearchConsole | AnalysisStep::LinkMetrics => {
            Some(StepState::NotApplicable)
        }
        _ => None,
    }
}

/// Nachricht eines Arbeiters an den Hauptthread.
enum Event {
    Started(AnalysisStep),
    Finished(AnalysisStep, Result<StepState, AnalyzeError>),
}

/// Wie [`run`], mit Optionen.
pub fn run_with(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    cancel: &CancelToken,
    progress: &dyn Fn(AnalysisStep, f32),
    opts: &RunOptions<'_>,
) -> Result<AnalysisOutcome, AnalyzeError> {
    let _busy = AnalysisBusy::new(file.conn())?;
    let in_scope: Vec<AnalysisStep> = AnalysisStep::ALL
        .iter()
        .copied()
        .filter(|s| opts.only.is_none_or(|only| only.contains(s)))
        .collect();

    // K-74: `links_target` fehlt während des Crawls und nach einem Abbruch; die Analyse einer nicht laufenden Datei
    // baut ihn bei Bedarf auf (ein regulär beendeter Crawl hat ihn schon).
    os_store::link_index::ensure_if_idle(file)?;

    persist::write_run_state(file, RunState::Running, None)?;

    let mut results: Vec<(AnalysisStep, StepState)> = Vec::new();
    let mut runnable: Vec<AnalysisStep> = Vec::new();
    for &step in &in_scope {
        match predetermined_state(step, cfg) {
            Some(state) => {
                persist::write_step(
                    file,
                    step,
                    state,
                    Some(&opts.clock.now()),
                    Some(&step_config_hash(step, cfg)),
                )?;
                persist::clear_stale(file, step)?;
                results.push((step, state));
                progress(step, 1.0);
            }
            None => {
                persist::begin_step(file, step)?;
                results.push((step, StepState::Pending));
                runnable.push(step);
            }
        }
    }

    let mut first_error: Option<AnalyzeError> = None;
    if !runnable.is_empty() && !cancel.is_cancelled() {
        let workers = match opts.max_parallel {
            0 => std::thread::available_parallelism().map_or(1, usize::from),
            n => n,
        }
        .clamp(1, runnable.len());
        let open_lock = Mutex::new(());
        let opened = Barrier::new(workers);
        let next = AtomicUsize::new(0);
        let (tx, rx) = mpsc::channel::<Event>();
        let path = file.path().to_path_buf();

        std::thread::scope(|scope| {
            for _ in 0..workers {
                let tx = tx.clone();
                let (next, runnable, path) = (&next, &runnable, &path);
                let (open_lock, opened) = (&open_lock, &opened);
                scope.spawn(move || {
                    // Eigene Verbindung je Arbeiter; scheitert das Öffnen, scheitern seine Schritte.
                    let conn = {
                        let _open = open_lock.lock().unwrap_or_else(|e| e.into_inner());
                        CrawlFile::open(path, OpenMode::ReadWrite).and_then(|file| {
                            file.conn().pragma_update(None, "cache_size", -65_536)?;
                            file.conn().pragma_update(Some("temp"), "cache_size", -32_768)?;
                            file.conn().busy_timeout(std::time::Duration::from_secs(300))?;
                            Ok(file)
                        }).map_err(|e| e.to_string())
                    };
                    // PRAGMA journal_mode darf nicht gegen bereits laufende Schreiber antreten.
                    opened.wait();
                    loop {
                        if cancel.checkpoint().is_err() {
                            break;
                        }
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        let Some(&step) = runnable.get(i) else { break };
                        let _ = tx.send(Event::Started(step));
                        let diag_start = std::time::Instant::now();
                        let wal_path = format!("{}-wal", path.display());
                        let wal_before = std::fs::metadata(&wal_path).map_or(0, |m| m.len());
                        let outcome = match &conn {
                            Ok(f) => (opts.registry)(step)(f, cfg, cancel),
                            Err(msg) => Err(open_error(msg)),
                        };
                        if std::env::var_os("OS_SCALE_DIAG").is_some() {
                            let wal_after = std::fs::metadata(&wal_path).map_or(0, |m| m.len());
                            eprintln!("OS_SCALE_DIAG analysis_{step:?} duration_ms={} wal_before={} wal_after={} ok={}", diag_start.elapsed().as_millis(), wal_before, wal_after, u8::from(outcome.is_ok()));
                        }
                        // Jeder Teilschritt hat seine Statements/Transaktionen freigegeben.
                        // Große Veröffentlichungen vor dem nächsten Schritt zurückführen.
                        let outcome = match &conn {
                            Ok(f) => outcome.and_then(|state| {
                                checkpoint_phase(f, &format!("analysis_checkpoint_{step:?}"), false)?;
                                Ok(state)
                            }),
                            Err(_) => outcome,
                        };
                        let _ = tx.send(Event::Finished(step, outcome));
                    }
                });
            }
            drop(tx);
            // Hauptthread: Fortschritt melden und fertige Schritte sofort speichern.
            for event in rx {
                match event {
                    Event::Started(step) => progress(step, 0.0),
                    Event::Finished(step, outcome) => {
                        match settle(file, cfg, opts.clock, step, outcome) {
                            Ok(state) => {
                                set_result(&mut results, step, state);
                                cancel.set_progress(
                                    results
                                        .iter()
                                        .filter(|(_, s)| *s != StepState::Pending)
                                        .count() as f32
                                        / results.len().max(1) as f32,
                                );
                                if state != StepState::Pending {
                                    progress(step, 1.0);
                                }
                            }
                            Err(e) => {
                                first_error.get_or_insert(e);
                            }
                        }
                    }
                }
            }
        });
    }

    cancel.set_progress(
        results
            .iter()
            .filter(|(_, s)| *s != StepState::Pending)
            .count() as f32
            / results.len().max(1) as f32,
    );
    let complete = results.iter().all(|(_, s)| *s != StepState::Pending);
    let final_state = overall_state(file)?;
    persist::write_run_state(file, final_state, Some(&opts.clock.now()))?;

    checkpoint_phase(file, "analysis_checkpoint_final", true)?;

    if let Some(e) = first_error {
        return Err(e);
    }
    Ok(AnalysisOutcome {
        steps: results,
        complete,
    })
}

// SQLite hat einen Schreiber; parallele Rechenphasen dürfen auf dessen kurze
// Veröffentlichung länger als das 5-s-Verbindungsdefault warten. Danach wiederherstellen.
struct AnalysisBusy<'a> {
    conn: &'a rusqlite::Connection,
    previous: i64,
}
impl<'a> AnalysisBusy<'a> {
    fn new(conn: &'a rusqlite::Connection) -> Result<Self, rusqlite::Error> {
        let previous = conn.pragma_query_value(None, "busy_timeout", |r| r.get(0))?;
        conn.pragma_update(None, "busy_timeout", 300_000)?;
        Ok(Self { conn, previous })
    }
}
impl Drop for AnalysisBusy<'_> {
    fn drop(&mut self) {
        let _ = self.conn.pragma_update(None, "busy_timeout", self.previous);
    }
}

// Keine Lesetransaktion über Phasengrenzen: SQLite darf das WAL hier zurücksetzen.
fn checkpoint_phase(file: &CrawlFile, name: &str, always: bool) -> Result<(), AnalyzeError> {
    let mut wal = file.path().as_os_str().to_os_string();
    wal.push("-wal");
    let before = std::fs::metadata(&wal).map_or(0, |m| m.len());
    if !always && before < 64 * 1024 * 1024 {
        return Ok(());
    }
    let start = std::time::Instant::now();
    let timeout: i64 = file
        .conn()
        .pragma_query_value(None, "busy_timeout", |r| r.get(0))?;
    let result = if always {
        // CRAWL-141 Ablaufpunkt 7: Der abschließende Checkpoint wartet höchstens 100 ms (Wanduhr) auf Leser; die
        // Analyse ist mit ihrem letzten dauerhaften Commit abgeschlossen. Verbleibende Frames führt die App im
        // Hintergrund zurück (CRAWL-126).
        os_store::writer::checkpoint_within(
            file.conn(),
            os_store::writer::CHECKPOINT_READER_WAIT,
            "PRAGMA wal_checkpoint(TRUNCATE)",
            |r| r.get::<_, i64>(0),
        )
        .map_err(AnalyzeError::from)
    } else {
        file.conn().pragma_update(None, "busy_timeout", 5_000)?;
        file.conn()
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(AnalyzeError::from)
    };
    file.conn().pragma_update(None, "busy_timeout", timeout)?;
    let busy = result?;
    if std::env::var_os("OS_SCALE_DIAG").is_some() {
        let after = std::fs::metadata(&wal).map_or(0, |m| m.len());
        eprintln!(
            "OS_SCALE_DIAG {name} duration_ms={} wal_before={} wal_after={} busy={busy}",
            start.elapsed().as_millis(),
            before,
            after
        );
    }
    Ok(())
}

/// Fehler, wenn ein Arbeiter die Crawl-Datei nicht öffnen konnte.
fn open_error(msg: &str) -> AnalyzeError {
    AnalyzeError::Sql(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
        Some(format!(
            "Crawl-Datei für die Analyse nicht zu öffnen: {msg}"
        )),
    ))
}

fn set_result(results: &mut [(AnalysisStep, StepState)], step: AnalysisStep, state: StepState) {
    if let Some(slot) = results.iter_mut().find(|(s, _)| *s == step) {
        slot.1 = state;
    }
}

/// Speichert das Ergebnis eines Schritts und liefert dessen endgültigen Zustand.
fn settle(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    clock: &dyn Clock,
    step: AnalysisStep,
    outcome: Result<StepState, AnalyzeError>,
) -> Result<StepState, AnalyzeError> {
    let state = outcome?;
    match state {
        // Nicht fertig (abgebrochen): bleibt ausstehend.
        StepState::Pending => Ok(StepState::Pending),
        done => {
            persist::write_step(
                file,
                step,
                done,
                Some(&clock.now()),
                Some(&step_config_hash(step, cfg)),
            )?;
            if done != StepState::Pending {
                persist::clear_stale(file, step)?;
            }
            Ok(done)
        }
    }
}

/// `completed`, wenn jeder Teilschritt eine Entscheidung hat, sonst `incomplete`.
fn overall_state(file: &CrawlFile) -> Result<RunState, AnalyzeError> {
    let rows = persist::read_steps(file)?;
    let decided = AnalysisStep::ALL
        .iter()
        .all(|s| rows.get(s).is_some_and(|r| r.state != StepState::Pending));
    Ok(if decided {
        RunState::Completed
    } else {
        RunState::Incomplete
    })
}

#[cfg(test)]
mod scale_tests {
    use super::*;
    use os_model::time::{IdSource, SeededIds};
    use os_model::{CrawlMode, Timestamp};
    use os_store::{CrawlInit, StorageProfile};

    fn publish(
        file: &CrawlFile,
        _: &CrawlConfig,
        _: &CancelToken,
    ) -> Result<StepState, AnalyzeError> {
        let cache: i64 = file
            .conn()
            .pragma_query_value(None, "cache_size", |r| r.get(0))?;
        assert_eq!(cache, -65_536);
        let tx = file.conn().unchecked_transaction()?;
        for id in 1..=1000 {
            tx.execute(
                "INSERT INTO url_ext(url_id,key,v_text) VALUES(?1,'content.text',?2)",
                rusqlite::params![id, "x".repeat(1100)],
            )?;
        }
        tx.commit()?;
        Ok(StepState::Done)
    }

    // spec: PERF-003
    #[test]
    fn analysis_completion_truncates_the_wal_with_worker_connections_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("phases.ospider");
        let cfg = CrawlConfig::default();
        let file = CrawlFile::create(
            &path,
            &CrawlInit {
                crawl_id: SeededIds::new(7).new_crawl_id(Timestamp::UNIX_EPOCH),
                mode: CrawlMode::Spider,
                start_url: None,
                list_source: None,
                project: String::new(),
                name: String::new(),
                config: cfg.clone(),
                created_at: Timestamp::UNIX_EPOCH,
            },
            StorageProfile::Balanced,
        )
        .unwrap();
        run_with(
            &file,
            &cfg,
            &CancelToken::new(),
            &|_, _| {},
            &RunOptions {
                only: Some(&[AnalysisStep::LinkScore]),
                registry: |_| publish,
                max_parallel: 1,
                ..RunOptions::default()
            },
        )
        .unwrap();
        let mut wal = path.as_os_str().to_os_string();
        wal.push("-wal");
        assert_eq!(std::fs::metadata(wal).map_or(0, |m| m.len()), 0);
    }

    fn contend(
        file: &CrawlFile,
        _: &CrawlConfig,
        _: &CancelToken,
    ) -> Result<StepState, AnalyzeError> {
        let tx = file.conn().unchecked_transaction()?;
        tx.execute("INSERT INTO crawl_meta(key,value) VALUES('e07_probe','1') ON CONFLICT(key) DO UPDATE SET value=CAST(value AS INTEGER)+1", [])?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        tx.commit()?;
        Ok(StepState::Done)
    }

    // spec: PERF-003
    // spec: CRAWL-141
    #[test]
    fn parallel_writers_open_before_work_and_persist_every_completed_step() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parallel.ospider");
        let cfg = CrawlConfig::default();
        let file = CrawlFile::create(
            &path,
            &CrawlInit {
                crawl_id: SeededIds::new(8).new_crawl_id(Timestamp::UNIX_EPOCH),
                mode: CrawlMode::Spider,
                start_url: None,
                list_source: None,
                project: String::new(),
                name: String::new(),
                config: cfg.clone(),
                created_at: Timestamp::UNIX_EPOCH,
            },
            StorageProfile::Balanced,
        )
        .unwrap();
        file.conn().pragma_update(None, "busy_timeout", 17).unwrap();
        let selected = [
            AnalysisStep::LinkScore,
            AnalysisStep::Canonicals,
            AnalysisStep::Pagination,
            AnalysisStep::Hreflang,
            AnalysisStep::Links,
            AnalysisStep::Sitemaps,
            AnalysisStep::Content,
            AnalysisStep::Amp,
        ];
        let outcome = run_with(
            &file,
            &cfg,
            &CancelToken::new(),
            &|_, _| {},
            &RunOptions {
                only: Some(&selected),
                registry: |_| contend,
                max_parallel: 8,
                ..RunOptions::default()
            },
        )
        .unwrap();
        assert!(outcome.complete);
        assert!(outcome
            .steps
            .iter()
            .all(|(_, state)| *state == StepState::Done));
        let count: i64 = file
            .conn()
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM crawl_meta WHERE key='e07_probe'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, selected.len() as i64);
        let timeout: i64 = file
            .conn()
            .pragma_query_value(None, "busy_timeout", |r| r.get(0))
            .unwrap();
        assert_eq!(timeout, 17);
    }
}

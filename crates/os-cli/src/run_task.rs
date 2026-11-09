//! `openspider --run-task <task-id>` (OUT-209): lädt die Aufgabendatei und führt sie aus. Das ist der Einstieg
//! des OS-Planers (OUT-166) und von „Jetzt ausführen“ der App.
//!
//! Ein Lauf ist ein eigener Prozess ohne Oberfläche (OUT-163). Er
//!
//! 1. gleicht abgestürzte Läufe in der Historie ab und legt seinen eigenen Eintrag an (OUT-169),
//! 2. prüft die Überlappung mit einem laufenden Lauf derselben Aufgabe (OUT-167) und die Gültigkeit der Aufgabe
//!    (OUT-168),
//! 3. übersetzt die Aufgabe in dieselben Optionen wie ein Aufruf von Hand und führt Crawl, Analyse und Exporte
//!    über denselben Ablauf aus (`plan` / `exec`, daher identische Ergebnisse),
//! 4. erzeugt Sitemaps und ruft die Erweiterungen auf, die dieses Binary noch nicht selbst kennt
//!    (API-Abruf, Looker-Studio-Zeitreihe, Vergleich mit dem Vorlauf, Benachrichtigung – [`TaskHooks`]),
//! 5. schreibt Ergebnis, Dateien und Warnungen in die Historie und räumt auf.
//!
//! Abbruch (OUT-171): Eine Marke neben der Laufdatei oder ein Signal pausiert den Crawl, speichert ihn und
//! lässt die Exporte aus (Ergebnis `Aborted`, Exit-Code 130).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_analyze::analysis::CancelToken;
use os_export::timeseries::CellValue;
use os_fetch::SecretStore;
use os_model::{Clock, IdSource, RandomIds, SystemClock};
use os_sched::history::{HistoryStore, RunRecord, RunResult, HEARTBEAT_SECS, KEEP_RUNS};
use os_sched::tasks::model::{CustomSummary, ExportFormat, OutputMode, Overlap, Source};
use os_sched::tasks::store::{folder_name, valid_id};
use os_sched::tasks::validate::{self, FsEnv};
use os_sched::tasks::{StoreError, Task, TaskStore};
use os_store::paths::{app_paths, AppPaths};
use os_store::profile::ConfigProfiles;
use tracing_subscriber::prelude::*;

use crate::args::RunArgs;
use crate::exec::{self, Outcome};
use crate::exit::Exit;
use crate::logging::diagnostics;
use crate::plan::{self, BatchSettings};
use crate::reporter::Reporter;
use crate::signals::Signals;

/// SMTP-Abschlussmails mit dem gespeicherten Auto-Compare-Ergebnis (OUT-152).
#[path = "../../os-analyze/src/compare/change/mail_hooks.rs"]
pub mod comparison_mail;

/// Umgebungsvariable, mit der die App die Kennung des Laufs vorgibt („Jetzt ausführen“).
pub const RUN_ID_ENV: &str = "OPENSPIDER_TASK_RUN_ID";

/// Erweiterungen eines Aufgabenlaufs, die dieses Binary nicht selbst ausführt. Die Standardumsetzung
/// ([`DefaultHooks`]) meldet jede angeforderte Erweiterung als Warnung (der Lauf endet dann mit
/// „Completed with warnings“); Tests und spätere Pakete setzen eigene Umsetzungen ein.
pub trait TaskHooks: Send + Sync {
    /// Opt in to the built-in completion mail; custom hooks retain their own notification path.
    fn builtin_completion_mail(&self) -> bool {
        false
    }
    /// API-Quellen der Aufgabe (OUT-164): Abruf für den fertigen Crawl; Rückgabe = Warnungen.
    fn fetch_apis(&self, _task: &Task, _crawl: Option<&Path>) -> Vec<String> {
        // INTEG-017: the engine completes these requests before analysis and exports.
        Vec::new()
    }

    /// Vergleich mit dem vorherigen Crawl des Projekts (OUT-152).
    fn compare_with_previous(&self, _task: &Task, crawl: Option<&Path>) -> Result<(), String> {
        let crawl = crawl.ok_or("Auto comparison: no crawl file available")?;
        let dir = crawl
            .parent()
            .ok_or("Auto comparison: crawl directory missing")?;
        os_analyze::compare::change::auto::run(
            crawl,
            dir,
            &dir.join("comparisons"),
            None,
            &os_model::CancelToken::new(),
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Zeitreihen-Zeile für Looker Studio (OUT-210); Rückgabe = Hinweise (Warnungen) des Laufs.
    fn custom_summary(
        &self,
        _task: &Task,
        _summary: &CustomSummary,
        _crawl: Option<&Path>,
    ) -> Result<Vec<String>, String> {
        Err("Looker Studio export is not available in headless runs of this version".into())
    }

    /// Benachrichtigung nach dem Lauf (OUT-281).
    fn notify(&self, _task: &Task, _record: &RunRecord) -> Result<(), String> {
        Ok(())
    }
}

/// Tabelle, in die die Zeitreihe einer Aufgabe geschrieben wird (OUT-210): Umbenennen der Aufgabe führt zu
/// einer neuen Tabelle, die bisherige bleibt unverändert.
pub fn series_table(task: &Task) -> String {
    format!(
        "OpenSpider/{}/{}_custom_summary_report",
        task.project_name(),
        task.name
    )
}

/// Ablage der Zeitreihe (Google-Tabelle, Blatt `data`; die Anbindung liefert X03, Tests einen Speicher).
pub trait SeriesSink: Send + Sync {
    /// Kopfzeile der Tabelle; leer, wenn es sie noch nicht gibt.
    fn header(&self, table: &str) -> Result<Vec<String>, String>;
    /// Schreibt die (ggf. erweiterte) Kopfzeile und hängt eine Zeile an.
    fn append(&self, table: &str, header: &[String], row: &[CellValue]) -> Result<(), String>;
}

/// Misst den Crawl und hängt eine Zeile an die Zeitreihe der Aufgabe an (OUT-210 … OUT-212). Neue Metriken
/// kommen rechts dazu, entfernte bleiben als leere Spalte, die Kopfzeile bestimmt die Reihenfolge.
pub fn append_series(
    sink: &dyn SeriesSink,
    task: &Task,
    summary: &CustomSummary,
    crawl: &Path,
    zone: &dyn os_sched::tasks::Zone,
) -> Result<Vec<String>, String> {
    use os_store::{CrawlFile, OpenMode};
    let file = CrawlFile::open(crawl, OpenMode::ReadOnly).map_err(|e| e.to_string())?;
    let cfg = file.config().map_err(|e| e.to_string())?;
    let tabs = os_analyze::tabs::configured_for_file(&cfg, &file);
    let job = os_export::catalog::TableJob {
        entry_name: "summary",
        tabs: &tabs,
        file: &file,
        cfg: &cfg,
    };
    let mut measurement = os_export::timeseries::measure(&job).map_err(|e| e.to_string())?;
    // Die Auswahl darf die Schreibweise der Spec verwenden (`Client Error (4xx)`); der Katalog führt
    // `(4XX)`. Die gewählte Schreibweise bleibt Spaltenname, der Wert kommt aus dem Katalogeintrag.
    for name in &summary.metrics {
        if !measurement.values.contains_key(name) {
            let alias = measurement
                .values
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.clone());
            if let Some(value) = alias {
                measurement.values.insert(name.clone(), value);
            }
        }
    }
    let table = series_table(task);
    let existing = sink.header(&table)?;
    let offset = zone.offset_at(measurement.started_at.0);
    let row = os_export::timeseries::align(&existing, &summary.metrics, &measurement, offset)
        .map_err(|e| e.to_string())?;
    sink.append(&table, &row.header, &row.cells)?;
    Ok(row.warnings)
}

/// Erweiterungen mit einer Ablage für die Looker-Studio-Zeitreihe; alles andere wie [`DefaultHooks`].
pub struct SeriesHooks {
    /// Ablage.
    pub sink: Arc<dyn SeriesSink>,
}

impl TaskHooks for SeriesHooks {
    fn custom_summary(
        &self,
        task: &Task,
        summary: &CustomSummary,
        crawl: Option<&Path>,
    ) -> Result<Vec<String>, String> {
        let crawl = crawl.ok_or("Looker Studio export skipped: no crawl file available")?;
        append_series(
            self.sink.as_ref(),
            task,
            summary,
            crawl,
            &os_sched::tasks::SystemZone,
        )
    }
}

/// Standardumsetzung mit konfiguriertem Abschlussmail-Versand für Auto-Compare (OUT-152).
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultHooks;

impl TaskHooks for DefaultHooks {
    fn builtin_completion_mail(&self) -> bool {
        true
    }
    fn notify(&self, task: &Task, record: &RunRecord) -> Result<(), String> {
        comparison_mail::notify_configured(task, record)
    }
}

/// Umgebung eines Laufs.
pub struct RunCtx {
    /// Speicherorte (Einstellungen, Crawls, Logs).
    pub paths: AppPaths,
    /// Uhr.
    pub clock: Arc<dyn Clock>,
    /// Geheimnisse (Zugangsdaten der Integrationen und Konten).
    pub secrets: Arc<dyn SecretStore>,
    /// Vorgegebene Laufkennung (App); sonst neu.
    pub run_id: Option<String>,
    /// Vom Benutzer ausgelöst („Jetzt ausführen“) statt vom Planer.
    pub manual: bool,
    /// Lauf-Logdatei (für die Historie).
    pub log_path: Option<PathBuf>,
    /// Standard-Exportordner.
    pub export_dir: PathBuf,
    /// Fortschritt als JSON-Zeilen.
    pub json_progress: bool,
    /// Abstand der Herzschläge.
    pub heartbeat: Duration,
    /// Abstand der Abfrage der Abbruchmarke.
    pub poll: Duration,
    /// Faktor auf die Höchstlaufzeit der Aufgabe (nur Tests verkürzen sie; sonst 1).
    pub timeout_scale: f64,
}

impl RunCtx {
    /// Umgebung des Betriebs: Systemuhr, Schlüsselbund bzw. `OPENSPIDER_SECRET_STORE`.
    pub fn system(paths: AppPaths, args: &RunArgs) -> RunCtx {
        let run_id = std::env::var(RUN_ID_ENV).ok().filter(|v| is_safe(v));
        RunCtx {
            export_dir: default_export_dir(&paths),
            manual: run_id.is_some(),
            run_id,
            paths,
            clock: Arc::new(SystemClock),
            secrets: os_fetch::secret_store_from_env(),
            log_path: None,
            json_progress: args.json_progress || args.progress.as_deref() == Some("json"),
            heartbeat: Duration::from_secs(HEARTBEAT_SECS),
            poll: Duration::from_millis(500),
            timeout_scale: 1.0,
        }
    }
}

fn is_safe(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Standard-Exportordner (OUT-222): Einstellung, sonst `<Dokumente>/OpenSpider/Exports`.
pub fn default_export_dir(paths: &AppPaths) -> PathBuf {
    let configured = os_model::settings::AppSettings::load(&paths.settings)
        .ok()
        .and_then(|s| s.export.directory)
        .filter(|d| !d.trim().is_empty());
    match configured {
        Some(dir) => PathBuf::from(dir),
        None => os_sched::tasks::store::documents_export_dir()
            .unwrap_or_else(|| paths.settings.join("exports")),
    }
}

/// Ergebnis eines Aufgabenlaufs.
#[derive(Debug, Clone)]
pub struct TaskRun {
    /// Eintrag der Historie; `None`, wenn die Aufgabe unbekannt war (kein Eintrag möglich).
    pub record: Option<RunRecord>,
    /// Exit-Code des Prozesses.
    pub exit: Exit,
    /// Kurzmeldung für die Schlusszeile.
    pub summary: String,
}

fn config_path(paths: &AppPaths, reference: &str) -> PathBuf {
    let path = Path::new(reference);
    if path.extension().is_some_and(|e| e == "osconfig") || path.components().count() > 1 {
        path.to_path_buf()
    } else {
        ConfigProfiles::new(&paths.settings)
            .path(reference)
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn escape_name(name: &str) -> String {
    name.replace(',', "\\,")
}

/// Welche Exporte der Lauf ausführt: die Vorlage der Aufgabe (OUT-075), sonst die Auswahl im Dialog.
struct ExportPlan {
    items: Vec<(String, String)>,
    format: ExportFormat,
    dir: Option<String>,
    batch: BatchSettings,
    sitemap_pages: bool,
    sitemap_images: bool,
}

fn export_plan(task: &Task, paths: &AppPaths) -> Result<ExportPlan, String> {
    let effective = task.effective_exports();
    let mut plan = ExportPlan {
        items: effective.items,
        format: effective.format,
        dir: effective.dir,
        batch: BatchSettings {
            single_workbook: effective.single_workbook,
            skip_empty: effective.skip_empty,
        },
        sitemap_pages: effective.sitemap_pages,
        sitemap_images: effective.sitemap_images,
    };
    if let Some(name) = task.preset.as_deref().filter(|n| !n.trim().is_empty()) {
        let file = paths
            .settings
            .join("export-presets")
            .join(format!("{name}.ospreset"));
        let preset = os_export::types::ExportPreset::load(&file)
            .map_err(|e| format!("export preset '{name}' not readable: {e}"))?;
        plan.items = preset.items.into_iter().map(|s| (s.kind, s.name)).collect();
        plan.batch = BatchSettings {
            single_workbook: preset.single_workbook,
            skip_empty: preset.skip_empty,
        };
    }
    Ok(plan)
}

/// Übersetzt die Aufgabe in die Optionen eines Aufrufs von Hand.
fn to_run_args(task: &Task, paths: &AppPaths, exports: &ExportPlan, export_dir: &Path) -> RunArgs {
    let mut args = RunArgs {
        headless: true,
        project_name: Some(task.project_name().to_string()),
        task_name: Some(task.name.clone()),
        json_progress: false,
        ..RunArgs::default()
    };
    match &task.crawl.source {
        Source::Spider { url } => args.crawl = Some(url.trim().to_string()),
        Source::List { .. } => args.crawl_list = task.crawl.source.list_sources(),
    }
    args.auth_config = task
        .crawl
        .auth_profile
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|profile| crate::auth::profile_path(paths, profile));
    args.compare_with_previous = task.execution.auto_compare;
    args.config = task
        .crawl
        .config
        .as_deref()
        .filter(|c| !c.trim().is_empty())
        .map(|c| config_path(paths, c));
    args.set = task
        .crawl
        .overrides
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    // INTEG-017: use the same profile settings and lifecycle as interactive/CLI runs.
    for source in &task.apis {
        args.set.push(format!("api.{source}.enabled=true"));
    }
    for (kind, name) in &exports.items {
        let list = vec![escape_name(name)];
        match kind.to_ascii_lowercase().as_str() {
            "tab" | "view" => args.export_tabs.extend(list),
            "bulk" => args.bulk_export.extend(list),
            "report" => args.save_report.extend(list),
            _ => {}
        }
    }
    args.export_format = Some(
        match exports.format {
            ExportFormat::Csv => "csv",
            ExportFormat::Xlsx => "xlsx",
        }
        .into(),
    );
    args.save_crawl = task.output.save_crawl;
    args.segment = task.output.segment.clone();
    let base = exports
        .dir
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| export_dir.to_path_buf())
        .join(folder_name(task.project_name()))
        .join(folder_name(&task.name));
    args.output_folder = Some(base);
    match task.output.mode {
        OutputMode::Timestamped => args.timestamped_output = true,
        OutputMode::Overwrite => args.overwrite = true,
    }
    args
}

/// Gültigkeit vor dem Lauf (OUT-168).
fn validity(task: &Task, ctx: &RunCtx) -> Vec<validate::Issue> {
    let env = FsEnv {
        settings: ctx.paths.settings.clone(),
        secrets: Arc::clone(&ctx.secrets),
        store: TaskStore::in_settings(&ctx.paths.settings),
        clock: Arc::clone(&ctx.clock),
        zone: Box::new(os_sched::tasks::SystemZone),
        export_dir: ctx.export_dir.clone(),
    };
    let mut issues = validate::validate(task, &env);
    // Der Planer ruft auch Aufgaben mit abgelaufenem Einmal-Termin auf; das ist beim Lauf kein Fehler.
    issues.retain(|i| i.key != "tasks-once-future");
    // Beim Lauf zählt der Name nicht (die Aufgabe steht ja schon in der Ablage).
    issues.retain(|i| i.key != "tasks-issue-name-taken");
    issues
}

fn redact(text: &str) -> String {
    os_fetch::redact::text(text)
}

struct Live {
    record: Mutex<RunRecord>,
    history: HistoryStore,
}

impl Live {
    fn snapshot(&self) -> RunRecord {
        self.record
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn write(&self) {
        let record = self.snapshot();
        if let Err(e) = self.history.write(&record) {
            tracing::warn!(target: "os_cli::task", "Historie nicht schreibbar: {e}");
        }
    }

    fn update(&self, f: impl FnOnce(&mut RunRecord)) {
        f(&mut self.record.lock().unwrap_or_else(|p| p.into_inner()));
    }
}

fn start_message(task: &Task) -> String {
    let source = match &task.crawl.source {
        Source::Spider { url } => format!("mode=spider start_url={}", redact(url)),
        Source::List { path } => format!("mode=list source={}", redact(&path.replace('\n', ";"))),
    };
    format!(
        "task run started id={} name={:?} project={:?} {source} config={} apis={} exports={}",
        task.id,
        task.name,
        task.project_name(),
        task.crawl.config.as_deref().unwrap_or("default"),
        task.apis.join("+"),
        task.effective_exports().items.len()
    )
}

/// Führt die Aufgabe `task_id` aus (Kern von `--run-task`, ohne Prozessumgebung).
pub async fn run_in(
    ctx: &RunCtx,
    task_id: &str,
    hooks: &dyn TaskHooks,
    signals: Signals,
    rep: Arc<Reporter>,
) -> TaskRun {
    let history = HistoryStore::in_settings(&ctx.paths.settings);
    let store = TaskStore::in_settings(&ctx.paths.settings);
    let started = ctx.clock.now();
    history.reconcile(started);

    let loaded = store.get(task_id);
    let run_id = ctx
        .run_id
        .clone()
        .unwrap_or_else(|| RandomIds.new_crawl_id(started).to_string());
    let task = match loaded {
        Ok(task) => task,
        Err(StoreError::Parse { message, .. }) if valid_id(task_id) => {
            // Die Datei ist vorhanden, aber nicht lesbar: Der Lauf erscheint in der Historie.
            let mut record = RunRecord::start(&run_id, task_id, task_id, started);
            record.manual = ctx.manual;
            record.log_path = ctx
                .log_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned());
            record.message = Some(redact(&format!("Task file not readable: {message}")));
            record.finish(RunResult::Failed, Some(Exit::Usage.code()), started);
            let _ = history.write(&record);
            let summary = format!("Aufgabendatei von '{task_id}' nicht lesbar");
            rep.error(format!("{summary}: {message}"));
            return TaskRun {
                record: Some(record),
                exit: Exit::Usage,
                summary,
            };
        }
        Err(e) => {
            let summary = format!("Aufgabe '{task_id}' nicht gefunden");
            rep.error(format!("{summary} ({e})"));
            return TaskRun {
                record: None,
                exit: Exit::Usage,
                summary,
            };
        }
    };

    let mut record = RunRecord::start(&run_id, &task.id, &task.name, started);
    record.manual = ctx.manual;
    record.log_path = ctx
        .log_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());
    let live = Arc::new(Live {
        record: Mutex::new(record),
        history: history.clone(),
    });
    live.write();
    tracing::info!(target: "os_cli::task", "{}", start_message(&task));

    let finish = |live: &Live, result: RunResult, exit: Exit, message: Option<String>| -> TaskRun {
        let now = ctx.clock.now();
        live.update(|r| {
            r.warnings = rep.warning_messages().iter().map(|w| redact(w)).collect();
            r.exports = rep
                .exported_files()
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            if let Some(m) = message.clone() {
                r.message = Some(redact(&m));
            }
            r.finish(result, Some(exit.code()), now);
        });
        live.write();
        history.clear_cancel(&task.id, &run_id);
        history.prune(&task.id, KEEP_RUNS);
        let record = live.snapshot();
        tracing::info!(
            target: "os_cli::task",
            "task run finished result={} exit={} warnings={}",
            record.result.label(),
            exit.code(),
            record.warnings.len()
        );
        if hooks.builtin_completion_mail() && !task.execution.auto_compare {
            if let Err(e) = send_completion_mail(ctx, &task, &record) {
                rep.warn(format!("Benachrichtigung nicht gesendet: {}", redact(&e)));
            }
        }
        if let Err(e) = hooks.notify(&task, &record) {
            rep.warn(format!("Benachrichtigung nicht gesendet: {}", redact(&e)));
        }
        TaskRun {
            summary: message.unwrap_or_else(|| record.result.label().to_string()),
            record: Some(record),
            exit,
        }
    };

    // Überlappung (OUT-167)
    if task.execution.overlap == Overlap::Skip {
        let others: Vec<RunRecord> = history
            .active(&task.id, ctx.clock.now())
            .into_iter()
            .filter(|r| r.run_id != run_id)
            .collect();
        if !others.is_empty() {
            rep.info("Skipped: previous run still active");
            return finish(
                &live,
                RunResult::Skipped,
                Exit::Ok,
                Some("Skipped: previous run still active".into()),
            );
        }
    }
    if !task.enabled && !ctx.manual {
        return finish(
            &live,
            RunResult::Skipped,
            Exit::Ok,
            Some("Skipped: task is disabled".into()),
        );
    }

    // Gültigkeit (OUT-168)
    let issues = validity(&task, ctx);
    let errors: Vec<String> = issues
        .iter()
        .filter(|i| !i.warning)
        .map(|i| i.summary())
        .collect();
    for issue in issues.iter().filter(|i| i.warning) {
        rep.warn(issue.summary());
    }
    if !errors.is_empty() {
        let message = format!("Task is invalid: {}", errors.join("; "));
        rep.error(&message);
        return finish(&live, RunResult::Failed, Exit::Usage, Some(message));
    }

    let exports = match export_plan(&task, &ctx.paths) {
        Ok(e) => e,
        Err(message) => return finish(&live, RunResult::Failed, Exit::Usage, Some(message)),
    };
    // Gleichzeitige Läufe schreiben nie in dieselbe Exportdatei (OUT-167): Läuft derselbe Lauf schon, geht
    // ein Lauf im Überschreiben-Modus in einen Zeitstempel-Ordner.
    let mut effective = task.clone();
    if task.output.mode == OutputMode::Overwrite
        && history
            .active(&task.id, ctx.clock.now())
            .iter()
            .any(|r| r.run_id != run_id)
    {
        effective.output.mode = OutputMode::Timestamped;
        rep.warn("another run of this task is active – writing to a timestamped folder instead of overwriting");
    }
    let args = to_run_args(&effective, &ctx.paths, &exports, &ctx.export_dir);
    rep.phase("validate");
    let mut plan = match plan::build_for_task_with_clock(
        &args,
        ctx.paths.clone(),
        &rep,
        exports.batch,
        ctx.secrets.as_ref(),
        ctx.clock.as_ref(),
    ) {
        Ok(plan) => plan,
        Err(e) => return finish(&live, RunResult::Failed, e.exit, Some(e.message)),
    };
    plan.api_secrets = Some(os_integrations::crawl_glue::SecretSnapshot(
        ctx.secrets.clone(),
    ));
    if let Some(profile) = task
        .crawl
        .auth_profile
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        plan.config.auth.profile_refs.push(profile.to_string());
    }
    // Sitemaps brauchen einen Ausgabeordner, auch ohne Tabellenexporte.
    if plan.output.is_none() && (exports.sitemap_pages || exports.sitemap_images) {
        let mode = match effective.output.mode {
            OutputMode::Timestamped => os_export::output::OutputMode::Timestamped,
            OutputMode::Overwrite => os_export::output::OutputMode::Overwrite,
        };
        let base = args.output_folder.clone().unwrap_or_default();
        match os_export::output::prepare(&base, mode, ctx.clock.as_ref()) {
            Ok(dir) => plan.output = Some(dir),
            Err(e) => {
                return finish(
                    &live,
                    RunResult::Failed,
                    Exit::Usage,
                    Some(format!("Ausgabeordner: {e}")),
                )
            }
        }
    }
    let out_dir = plan.output.clone();
    live.update(|r| r.output_folder = out_dir.as_ref().map(|p| p.to_string_lossy().into_owned()));
    live.write();
    let plan_exports_for_timeout = (!plan.exports.is_empty(), args.clone());

    // Herzschlag und Abbruchmarke
    let timed_out = Arc::new(AtomicBool::new(false));
    let watcher = {
        let live = Arc::clone(&live);
        let history = history.clone();
        let signals = signals.clone();
        let clock = Arc::clone(&ctx.clock);
        let (task_id, run_id) = (task.id.clone(), run_id.clone());
        let (poll, beat) = (ctx.poll, ctx.heartbeat);
        let timeout = task
            .execution
            .timeout_min
            .map(|m| Duration::from_secs_f64(f64::from(m) * 60.0 * ctx.timeout_scale));
        let timed_out = Arc::clone(&timed_out);
        tokio::spawn(async move {
            let began = tokio::time::Instant::now();
            let mut last_beat = began;
            loop {
                tokio::time::sleep(poll).await;
                if history.cancel_requested(&task_id, &run_id) {
                    signals.request_stop();
                }
                if let Some(limit) = timeout {
                    if began.elapsed() >= limit && !timed_out.swap(true, Ordering::SeqCst) {
                        signals.request_stop();
                    }
                }
                if last_beat.elapsed() >= beat {
                    last_beat = tokio::time::Instant::now();
                    live.update(|r| r.heartbeat_at = clock.now().to_db());
                    live.write();
                }
            }
        })
    };

    let mut outcome =
        exec::execute_with_clock(plan, Arc::clone(&rep), signals.clone(), ctx.clock.clone()).await;
    watcher.abort();
    let _ = watcher.await;
    let timeout_hit = timed_out.load(Ordering::SeqCst) && outcome.exit == Exit::Interrupted;

    // Höchstlaufzeit erreicht (OUT-171 [P2]): Exporte des bisherigen Stands, Ergebnis mit Warnungen.
    if timeout_hit {
        if let (Some(path), true) = (outcome.crawl_path.clone(), plan_exports_for_timeout.0) {
            let mut again = plan_exports_for_timeout.1.clone();
            again.crawl = None;
            again.crawl_list.clear();
            again.load_crawl = Some(path.to_string_lossy().into_owned());
            again.output_folder = out_dir.clone();
            again.timestamped_output = false;
            again.overwrite = true;
            again.config = None;
            again.set.clear();
            if let Ok(second) = plan::build_for_task(&again, ctx.paths.clone(), &rep, exports.batch)
            {
                let crawled = outcome.crawled;
                let crawl_id = outcome.crawl_id.clone();
                let crawl_path = outcome.crawl_path.clone();
                let signals2 = Signals::manual(CancelToken::new());
                outcome = exec::execute(second, Arc::clone(&rep), signals2).await;
                outcome.crawled = outcome.crawled.or(crawled);
                outcome.crawl_id = outcome.crawl_id.or(crawl_id);
                outcome.crawl_path = outcome.crawl_path.or(crawl_path);
            }
        }
        rep.warn(format!(
            "max runtime of {} min reached – exports reflect the crawl state so far",
            task.execution.timeout_min.unwrap_or(0)
        ));
    }

    live.update(|r| {
        r.crawl_id = outcome.crawl_id.clone();
        r.urls_crawled = outcome.crawled;
    });

    let crawled_ok = matches!(outcome.exit, Exit::Ok | Exit::ExportFailed) || timeout_hit;
    if crawled_ok {
        post_phase(
            &task,
            &exports,
            &outcome,
            out_dir.as_deref(),
            hooks,
            &rep,
            ctx,
        );
    }

    let (result, exit) = match outcome.exit {
        Exit::Ok if timeout_hit => (RunResult::CompletedWithWarnings, Exit::Ok),
        _ if timeout_hit => (RunResult::CompletedWithWarnings, Exit::Ok),
        // Den Fehlschlag stuft der Crawl ein (CRAWL-128, z. B. Start-URL nicht erreichbar): Historie „Failed“, mit und ohne
        // Auto-Vergleich; der Exit-Code bleibt der des Laufs (OUT-182; OS-Festlegung 09.11.2026).
        other if outcome.crawl_failed.is_some() && other != Exit::Interrupted => {
            (RunResult::Failed, other)
        }
        Exit::Ok if rep.warnings() > 0 => (RunResult::CompletedWithWarnings, Exit::Ok),
        Exit::Ok => (RunResult::Completed, Exit::Ok),
        Exit::Interrupted => (RunResult::Aborted, Exit::Interrupted),
        other => (RunResult::Failed, other),
    };
    finish(&live, result, exit, Some(outcome.summary))
}

/// Sitemaps und Erweiterungen nach dem Crawl.
fn post_phase(
    task: &Task,
    exports: &ExportPlan,
    outcome: &Outcome,
    out_dir: Option<&Path>,
    hooks: &dyn TaskHooks,
    rep: &Reporter,
    ctx: &RunCtx,
) {
    let crawl = outcome.crawl_path.as_deref().filter(|p| p.is_file());
    if (exports.sitemap_pages || exports.sitemap_images) && crawl.is_some() {
        if let (Some(path), Some(out)) = (crawl, out_dir) {
            sitemaps(path, out, exports, rep, ctx);
        }
    } else if exports.sitemap_pages || exports.sitemap_images {
        rep.warn("XML sitemap skipped: no crawl file available");
    }
    for warning in hooks.fetch_apis(task, crawl) {
        rep.warn(warning);
    }
    // Ein fehlgeschlagener Crawl wird nicht verglichen; der Vermerk steht schon im Lauf (`exec::COMPARE_SKIPPED_FAILED`).
    if task.execution.auto_compare && outcome.crawl_failed.is_none() {
        if let Err(e) = hooks.compare_with_previous(task, crawl) {
            rep.warn(e);
        }
    }
    if let Some(summary) = &task.custom_summary {
        match hooks.custom_summary(task, summary, crawl) {
            Ok(notes) => notes.into_iter().for_each(|n| rep.warn(n)),
            Err(e) => rep.warn(e),
        }
    }
}

fn sitemaps(crawl: &Path, out: &Path, exports: &ExportPlan, rep: &Reporter, ctx: &RunCtx) {
    use os_store::{CrawlFile, OpenMode};
    let file = match CrawlFile::open(crawl, OpenMode::ReadOnly) {
        Ok(f) => f,
        Err(e) => {
            rep.warn(format!(
                "XML sitemap skipped: crawl file not readable ({e})"
            ));
            return;
        }
    };
    let opts = os_export::types::SitemapOptions::default();
    let config = match file.config() {
        Ok(c) => c,
        Err(e) => {
            rep.warn(format!(
                "XML sitemap skipped: crawl configuration not readable ({e})"
            ));
            return;
        }
    };
    if exports.sitemap_pages {
        match os_export::sitemap_gen::generate_sitemap_configured_with(
            &file,
            &opts,
            &config,
            None,
            ctx.clock.as_ref(),
            out,
        ) {
            Ok(report) => report.files.iter().for_each(|f| {
                rep.note_export(&f.path);
                rep.info(format!(
                    "Sitemap: {} ({} URLs)",
                    f.path.display(),
                    report.urls
                ));
            }),
            Err(e) => rep.warn(format!("XML sitemap failed: {e}")),
        }
    }
    if exports.sitemap_images {
        match os_export::sitemap_gen::generate_image_sitemap_with(
            &file,
            &opts,
            &config,
            None,
            ctx.clock.as_ref(),
            out,
        ) {
            Ok(report) => report.files.iter().for_each(|f| {
                rep.note_export(&f.path);
                rep.info(format!("Image sitemap: {}", f.path.display()));
            }),
            Err(e) => rep.warn(format!("Image sitemap failed: {e}")),
        }
    }
}

/// Lauf-Log `task-<id>-<zeitstempel>.log` mit Weiterleitung der `tracing`-Ereignisse (OUT-240).
fn init_task_log(paths: &AppPaths, task_id: &str, args: &RunArgs) -> PathBuf {
    let mut path = paths
        .logs
        .join(format!("task-{task_id}-{}.log", diagnostics::file_stamp()));
    if path.exists() {
        path = paths.logs.join(format!(
            "task-{task_id}-{}-{}.log",
            diagnostics::file_stamp(),
            std::process::id()
        ));
    }
    let file = diagnostics::LogFile::new(path.clone(), false);
    let directives = args
        .log_level
        .clone()
        .or_else(|| std::env::var("OPENSPIDER_LOG").ok())
        .unwrap_or_else(|| "info".into());
    let filter = tracing_subscriber::EnvFilter::try_new(format!("info,{directives}"))
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let json = args.log_format.as_deref() == Some("json");
    let _ = diagnostics::cleanup(
        &paths.logs,
        std::time::SystemTime::now(),
        30,
        500 * 1024 * 1024,
    );
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(diagnostics::LogLayer {
            file: file.clone(),
            json,
            stderr: false,
        });
    if subscriber.try_init().is_ok() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let message = diagnostics::redact_text(&format!(
                "{info}\n{}",
                std::backtrace::Backtrace::force_capture()
            ));
            let _ = file.write_line(&format!(
                "{} ERROR panic: {}",
                diagnostics::timestamp(),
                message.replace('\n', "\\n")
            ));
            previous(info);
        }));
    }
    path
}

/// Trägt alle OpenSpider-Aufgaben beim Planer aus und meldet die Anzahl auf stderr.
pub fn unregister_with(planner: &dyn os_sched::planner::OsPlanner) -> usize {
    os_sched::planner::uninstall_all(planner)
}

/// `openspider --unregister-tasks` (Deinstallation, OUT-166): trägt alle Aufgaben beim Planer des
/// Betriebssystems aus; die Aufgabendateien bleiben.
pub fn unregister_tasks(_args: &RunArgs) -> i32 {
    let paths = app_paths();
    let dir = paths.settings.join("schedules").join("planner");
    let Some(planner) = os_sched::planner::system(Arc::new(os_sched::planner::SystemRunner), &dir)
    else {
        eprintln!("openspider: auf diesem System gibt es keinen Planer (Exit-Code 0)");
        return Exit::Ok.code();
    };
    let removed = unregister_with(planner.as_ref());
    eprintln!("openspider: {removed} Planer-Einträge entfernt (Exit-Code 0)");
    Exit::Ok.code()
}

/// Einstieg von `openspider --run-task <id>`; liefert den Exit-Code.
pub fn run(task_id: &str, args: &RunArgs) -> i32 {
    run_with(task_id, args, &DefaultHooks)
}

/// Wie [`run`] mit eigenen Erweiterungen.
pub fn run_with(task_id: &str, args: &RunArgs, hooks: &dyn TaskHooks) -> i32 {
    let paths = app_paths();
    let mut ctx = RunCtx::system(paths.clone(), args);
    let known = valid_id(task_id)
        && TaskStore::in_settings(&paths.settings)
            .path(task_id)
            .is_ok_and(|p| p.is_file());
    if known {
        ctx.log_path = Some(init_task_log(&paths, task_id, args));
    } else {
        // Keine Aufgabe zuordenbar: Eintrag im globalen Log (OUT-209, OUT-240).
        crate::logging::init(args);
        let line = format!(
            "{} ERROR os_cli::task: --run-task: unknown task '{}'",
            diagnostics::timestamp(),
            diagnostics::redact_text(task_id).replace(['\n', '\r'], " ")
        );
        let _ =
            diagnostics::LogFile::new(paths.logs.join("openspider.log"), true).write_line(&line);
    }
    let rep = Reporter::task_configured(ctx.json_progress, args.quiet);
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Fehler: Laufzeit nicht startbar: {e}");
            return Exit::Internal.code();
        }
    };
    let run = runtime.block_on(async {
        let signals = match Signals::install(CancelToken::new(), Arc::clone(&rep)) {
            Ok(s) => s,
            Err(e) => {
                return TaskRun {
                    record: None,
                    exit: Exit::Internal,
                    summary: format!("Signalbehandlung nicht installierbar: {e}"),
                }
            }
        };
        run_in(&ctx, task_id, hooks, signals, Arc::clone(&rep)).await
    });
    runtime.shutdown_background();
    if args.quiet && run.exit == Exit::Ok {
        return run.exit.code();
    }
    let mut err = std::io::stderr().lock();
    use std::io::Write;
    let _ = writeln!(
        err,
        "openspider: {} (Exit-Code {})",
        run.summary,
        run.exit.code()
    );
    run.exit.code()
}

// Additive completion hook (OUT-281 / OUT-282, K2-61).
fn send_completion_mail(ctx: &RunCtx, task: &Task, record: &RunRecord) -> Result<(), String> {
    let dir = ctx.paths.settings.clone();
    let crawls = ctx.paths.crawls.clone();
    let task = task.clone();
    let record = record.clone();
    if task.notify.to.iter().all(|s| s.trim().is_empty())
        || !(task.notify.on_success || task.notify.on_failure)
    {
        return Ok(());
    }
    let secrets = ctx.secrets.clone();
    let clock = ctx.clock.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "Mail-Laufzeit nicht verfügbar".to_string())?;
        let current = record
            .crawl_id
            .as_ref()
            .map(|id| crawls.join(format!("{id}.ospider")))
            .filter(|p| p.exists());
        let previous = if task.execution.auto_compare {
            os_store::registry::list_crawls(&crawls)
                .unwrap_or_default()
                .into_iter()
                .filter(|c| {
                    Some(&c.path) != current.as_ref()
                        && c.meta.project == task.project_name()
                        && c.meta.state == os_model::CrawlState::Completed
                        && c.meta.started_at.is_some_and(|t| {
                            os_model::Timestamp::from_db(&record.started_at)
                                .is_ok_and(|start| t < start)
                        })
                })
                .max_by_key(|c| c.meta.started_at)
                .map(|c| c.path)
        } else {
            None
        };
        let completion = match current {
            Some(path) => serde_json::from_value(os_analyze::mail_completion(
                &path,
                previous.as_deref(),
                task.execution.auto_compare,
                task.export_options.as_ref().is_some_and(|o| o.skip_empty),
            )?)
            .map_err(|_| "Mail-Daten ungültig".to_string())?,
            None => os_sched::mail::Completion::default(),
        };
        runtime
            .block_on(os_sched::mail::notify_task_with_context(
                &dir,
                &crawls,
                secrets
                    .as_mut_store()
                    .ok_or("Mail-Schlüsselbund ist schreibgeschützt")?,
                &task,
                &record,
                completion,
                &os_sched::mail::DeliveryContext {
                    clock: clock.as_ref(),
                    ..Default::default()
                },
            ))
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .join()
    .map_err(|_| "Mail-Versand abgebrochen".to_string())?
}

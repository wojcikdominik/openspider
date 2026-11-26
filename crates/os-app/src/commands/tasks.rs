//! tasks.* (OUT-160 … OUT-171, OUT-209): Aufgaben verwalten, beim Betriebssystem eintragen, ausführen, Historie,
//! Export und Import (M2-T01).
//!
//! Die App führt Aufgaben nie selbst aus: „Jetzt ausführen“ startet denselben Prozess wie der OS-Planer
//! (`openspider --run-task <id>`, OUT-163), damit Ergebnis, Historie und Lauf-Log gleich sind und ein laufender
//! Crawl der App unberührt bleibt. Jeder laufende Lauf – ob von der App oder vom Planer gestartet – erscheint
//! als abbrechbare Operation ([`watch_run`]); `op.cancel` legt die Abbruchmarke des Laufs an (OUT-171).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use os_model::Timestamp;
use os_sched::history::{HistoryStore, RunRecord, RunResult};
use os_sched::planner::{self, OsPlanner, PlannerSpec, SystemRunner};
use os_sched::tasks::bundle;
use os_sched::tasks::model as m;
use os_sched::tasks::validate::{self, Credentials, Env, FsEnv, Issue};
use os_sched::tasks::{next_run, StoreError, SystemZone, Task, TaskStore, Zone};
use serde_json::Value;

use crate::dto::{
    ApiKind, CmdError, ErrorCode, ExportFormatDto, ExportMultiArgs, ExportSel, FileResult, IdArgs,
    OpHandle, PathArgs, PlannerBackend, PlannerFile, PlannerPreview, ScheduleDef, TaskArgs,
    TaskCrawl, TaskDef, TaskExecution, TaskExport, TaskExportKind, TaskInfo, TaskNotify,
    TaskOverlap, TaskRegisterArgs, TaskRunRow, TaskRunState, TaskSource, TasksExportArgs,
    TasksHistoryArgs, ValidationIssue,
};
use crate::registry::{CmdResult, CommandMeta, Handler, Registry};
use crate::runtime::error::{parse_args, to_json};
use crate::runtime::{App, CallCtx};

// ---------------------------------------------------------------------------------------------------------
// Zustand und Einhängepunkte

/// Ein gestarteter Aufgabenprozess.
pub trait RunningChild: Send {
    /// Exit-Code, sobald der Prozess beendet ist.
    fn try_wait(&mut self) -> std::io::Result<Option<i32>>;
    /// Beendet den Prozess hart.
    fn kill(&mut self) -> std::io::Result<()>;
}

/// Was ein Start braucht.
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    /// Aufgabe.
    pub task_id: String,
    /// Vorgegebene Laufkennung.
    pub run_id: String,
    /// Datenordner der App (`OPENSPIDER_HOME`).
    pub home: PathBuf,
}

/// Startet `openspider --run-task` (Einhängepunkt für Tests und die Desktop-Hülle).
pub trait TaskLauncher: Send + Sync {
    /// Startet den Lauf.
    fn launch(&self, request: LaunchRequest) -> std::io::Result<Box<dyn RunningChild>>;
}

/// Startet das `openspider`-Programm als Kindprozess, ohne Shell.
pub struct ProcessLauncher {
    /// Pfad des Programms.
    pub binary: PathBuf,
}

struct ProcessChild(std::process::Child);

impl RunningChild for ProcessChild {
    fn try_wait(&mut self) -> std::io::Result<Option<i32>> {
        Ok(self.0.try_wait()?.map(|s| s.code().unwrap_or(-1)))
    }
    fn kill(&mut self) -> std::io::Result<()> {
        self.0.kill()
    }
}

impl TaskLauncher for ProcessLauncher {
    fn launch(&self, request: LaunchRequest) -> std::io::Result<Box<dyn RunningChild>> {
        let child = std::process::Command::new(&self.binary)
            .arg("--run-task")
            .arg(&request.task_id)
            .env("OPENSPIDER_HOME", &request.home)
            .env("OPENSPIDER_TASK_RUN_ID", &request.run_id)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        Ok(Box::new(ProcessChild(child)))
    }
}

/// Zustand der Aufgabenverwaltung dieser App.
#[derive(Default)]
pub struct TasksState {
    // None: noch nicht aufgelöst; Some(None): auf diesem System kein Planer.
    planner: Mutex<Option<Option<Arc<dyn OsPlanner>>>>,
    launcher: Mutex<Option<Arc<dyn TaskLauncher>>>,
    zone: Mutex<Option<Arc<dyn Zone>>>,
    watchers: Mutex<HashMap<String, OpHandle>>,
}

impl TasksState {
    /// Setzt den Planer des Betriebssystems (Tests: Dry-Run; die Hülle: das System).
    pub fn set_planner(&self, planner: Arc<dyn OsPlanner>) {
        *self.planner.lock().unwrap_or_else(|p| p.into_inner()) = Some(Some(planner));
    }

    fn resolve_planner(
        &self,
        resolve: impl FnOnce() -> Option<Arc<dyn OsPlanner>>,
    ) -> Option<Arc<dyn OsPlanner>> {
        // Startabgleich und set_planner müssen dieselbe Sperre halten: eine bereits laufende
        // Systemauflösung darf den injizierten Planer nicht nachträglich überschreiben (Linux: None).
        let mut planner = self.planner.lock().unwrap_or_else(|p| p.into_inner());
        planner.get_or_insert_with(resolve).clone()
    }

    /// Setzt den Starter für „Jetzt ausführen“.
    pub fn set_launcher(&self, launcher: Arc<dyn TaskLauncher>) {
        *self.launcher.lock().unwrap_or_else(|p| p.into_inner()) = Some(launcher);
    }

    /// Setzt die Zeitzone der Terminanzeige (Tests).
    pub fn set_zone(&self, zone: Arc<dyn Zone>) {
        *self.zone.lock().unwrap_or_else(|p| p.into_inner()) = Some(zone);
    }

    fn zone(&self) -> Arc<dyn Zone> {
        self.zone
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .unwrap_or_else(|| Arc::new(SystemZone))
    }
}

fn settings_dir(app: &App) -> PathBuf {
    app.config().paths.settings.clone()
}

fn store(app: &App) -> TaskStore {
    TaskStore::in_settings(&settings_dir(app))
}

fn history(app: &App) -> HistoryStore {
    HistoryStore::in_settings(&settings_dir(app))
}

/// Der Planer: gesetzt, sonst nach `OPENSPIDER_PLANNER` (`system`, `dry`, `off`); in der E2E-Bridge Dry-Run.
fn planner_of(app: &App) -> Option<Arc<dyn OsPlanner>> {
    planner_in(&app.ext::<TasksState>(), &settings_dir(app))
}

/// Wie [`planner_of`], ohne die `App` (Hintergrundarbeit, die sie nicht am Leben halten soll).
fn planner_in(state: &TasksState, settings: &Path) -> Option<Arc<dyn OsPlanner>> {
    state.resolve_planner(|| {
        let dry_dir = settings.join("schedules").join("planner-dry");
        let choice = std::env::var("OPENSPIDER_PLANNER").unwrap_or_default();
        match choice.as_str() {
            "off" => None,
            "dry" => Some(planner::dry_run(planner::native_backend(), &dry_dir)),
            _ if cfg!(feature = "e2e-bridge") => {
                Some(planner::dry_run(planner::native_backend(), &dry_dir))
            }
            _ => planner::system(
                Arc::new(SystemRunner),
                &settings.join("schedules").join("planner"),
            ),
        }
    })
}

fn launcher_of(app: &App) -> Option<Arc<dyn TaskLauncher>> {
    let state = app.ext::<TasksState>();
    if let Some(l) = state
        .launcher
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
    {
        return Some(l);
    }
    if cfg!(feature = "e2e-bridge") {
        return None;
    }
    Some(Arc::new(ProcessLauncher {
        binary: planner::cli_binary(),
    }))
}

fn no_planner() -> CmdError {
    CmdError::new(ErrorCode::Unsupported, "err-planner-unsupported")
}

fn invalid(field: &str, key: &str) -> CmdError {
    CmdError::invalid_input(key, field)
}

fn io_error(error: impl std::fmt::Display) -> CmdError {
    CmdError::new(ErrorCode::Io, "err-io").with_detail(error.to_string())
}

fn store_error(error: StoreError) -> CmdError {
    match error {
        StoreError::NotFound(id) => {
            CmdError::new(ErrorCode::NotFound, "err-task-not-found").with_arg("id", id)
        }
        StoreError::InvalidId(id) => invalid("id", "err-task-id").with_arg("id", id),
        StoreError::NameTaken(name) => {
            invalid("name", "tasks-issue-name-taken").with_arg("name", name)
        }
        StoreError::NameEmpty => invalid("name", "tasks-issue-name-required"),
        other => io_error(other),
    }
}

// ---------------------------------------------------------------------------------------------------------
// Umwandlung zwischen DTO und Aufgabenmodell

fn schedule_to_dto(schedule: &m::Schedule) -> ScheduleDef {
    match schedule {
        m::Schedule::Once { at } => ScheduleDef::Once { at: at.clone() },
        m::Schedule::Daily { time, .. } => ScheduleDef::Daily { time: time.clone() },
        m::Schedule::Weekly { days, time, .. } => ScheduleDef::Weekly {
            days: days.clone(),
            time: time.clone(),
        },
        m::Schedule::Monthly { day, time, .. } => ScheduleDef::Monthly {
            day: *day,
            time: time.clone(),
        },
        // [P2]: Das Dialogmodell kennt nur vier Arten; die Uhrzeit des Startzeitpunkts bleibt sichtbar.
        m::Schedule::Hourly { start } | m::Schedule::EveryDays { start, .. } => {
            ScheduleDef::Daily {
                time: start
                    .split_once('T')
                    .map_or("00:00", |(_, t)| t.get(..5).unwrap_or("00:00"))
                    .to_string(),
            }
        }
    }
}

fn schedule_from_dto(dto: &ScheduleDef, existing: Option<&m::Schedule>) -> m::Schedule {
    // Ein Zeitplan, den das Dialogmodell nur verkürzt zeigt (stündlich, alle N Tage), bleibt unverändert,
    // solange der Dialog ihn nicht ändert.
    if let Some(old) = existing {
        if &schedule_to_dto(old) == dto {
            return old.clone();
        }
    }
    let from = match existing {
        Some(
            m::Schedule::Daily { from, .. }
            | m::Schedule::Weekly { from, .. }
            | m::Schedule::Monthly { from, .. },
        ) => from.clone(),
        _ => None,
    };
    match dto {
        ScheduleDef::Once { at } => m::Schedule::Once { at: at.clone() },
        ScheduleDef::Daily { time } => m::Schedule::Daily {
            time: time.clone(),
            from,
        },
        ScheduleDef::Weekly { days, time } => m::Schedule::Weekly {
            days: days.clone(),
            time: time.clone(),
            from,
        },
        ScheduleDef::Monthly { day, time } => m::Schedule::Monthly {
            day: *day,
            time: time.clone(),
            from,
        },
    }
}

fn api_name(kind: ApiKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn api_from_name(name: &str) -> Option<ApiKind> {
    serde_json::from_value(Value::String(name.to_string())).ok()
}

fn format_to_dto(format: m::ExportFormat) -> ExportFormatDto {
    match format {
        m::ExportFormat::Csv => ExportFormatDto::Csv,
        m::ExportFormat::Xlsx => ExportFormatDto::Xlsx,
    }
}

fn format_from_dto(format: ExportFormatDto) -> Result<m::ExportFormat, CmdError> {
    match format {
        ExportFormatDto::Csv => Ok(m::ExportFormat::Csv),
        ExportFormatDto::Xlsx => Ok(m::ExportFormat::Xlsx),
        ExportFormatDto::Json => Err(invalid("exports.format", "tasks-issue-format-unsupported")),
    }
}

fn override_value(value: &str) -> Value {
    if let Ok(n) = value.parse::<i64>() {
        return Value::from(n);
    }
    if let Ok(b) = value.parse::<bool>() {
        return Value::from(b);
    }
    Value::String(value.to_string())
}

fn export_kind_name(kind: TaskExportKind) -> &'static str {
    match kind {
        TaskExportKind::Tab => "tab",
        TaskExportKind::Bulk => "bulk",
        TaskExportKind::Report => "report",
        TaskExportKind::Sitemap => "sitemap",
    }
}

fn export_kind_from(name: &str) -> Option<TaskExportKind> {
    match name {
        "tab" | "view" => Some(TaskExportKind::Tab),
        "bulk" => Some(TaskExportKind::Bulk),
        "report" => Some(TaskExportKind::Report),
        "sitemap" => Some(TaskExportKind::Sitemap),
        _ => None,
    }
}

/// Aufgabe → Dialogmodell.
fn to_def(task: &Task) -> TaskDef {
    TaskDef {
        id: Some(task.id.clone()),
        name: task.name.clone(),
        project: Some(task.project_name().to_string()),
        enabled: task.enabled,
        schedule: schedule_to_dto(&task.schedule),
        crawl: TaskCrawl {
            source: match &task.crawl.source {
                m::Source::Spider { url } => TaskSource::Spider { url: url.clone() },
                m::Source::List { path } => TaskSource::List { path: path.clone() },
            },
            profile: task.crawl.config.clone(),
            auth_profile: task.crawl.auth_profile.clone(),
            overrides: (!task.crawl.overrides.is_empty()).then(|| {
                Value::Object(
                    task.crawl
                        .overrides
                        .iter()
                        .map(|(k, v)| (k.clone(), override_value(v)))
                        .collect(),
                )
            }),
        },
        execution: TaskExecution {
            overlap: (task.execution.overlap == m::Overlap::Skip).then_some(TaskOverlap::Skip),
            headless: true,
            timeout_min: task.execution.timeout_min,
            run_if_missed: task.execution.run_if_missed,
            auto_compare: task.execution.auto_compare,
        },
        apis: task.apis.iter().filter_map(|a| api_from_name(a)).collect(),
        exports: task
            .exports
            .iter()
            .filter_map(|e| {
                Some(TaskExport {
                    kind: export_kind_from(&e.kind)?,
                    name: e.name.clone(),
                    format: format_to_dto(e.format),
                    dir: e.dir.clone(),
                })
            })
            .collect(),
        notify: TaskNotify {
            on_success: task.notify.on_success,
            on_failure: task.notify.on_failure,
            account: task.notify.account.clone(),
            to: task.notify.to.clone(),
        },
        export_options: task.export_options.as_ref().map(|o| ExportMultiArgs {
            items: o
                .items
                .iter()
                .map(|(kind, name)| ExportSel {
                    kind: kind.clone(),
                    name: name.clone(),
                })
                .collect(),
            format: format_to_dto(o.format),
            dir: o.dir.clone(),
            single_workbook: o.single_workbook,
            skip_empty: o.skip_empty,
            preset: task.preset.clone(),
        }),
        export_preset: task.preset.clone(),
        custom_summary: task
            .custom_summary
            .as_ref()
            .map(|c| crate::dto::CustomSummaryArgs {
                account: c.account.clone(),
                target: c.target.clone(),
                metrics: c.metrics.clone(),
            }),
    }
}

/// Dialogmodell → Aufgabe. Felder, die das Dialogmodell nicht führt, kommen aus `existing`.
fn from_def(def: &TaskDef, existing: Option<&Task>) -> Result<Task, CmdError> {
    let overrides = match &def.crawl.overrides {
        None | Some(Value::Null) => BTreeMap::new(),
        Some(Value::Object(map)) => map
            .iter()
            .map(|(k, v)| {
                let text = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => {
                        return Err(invalid("crawl.overrides", "tasks-issue-override-invalid")
                            .with_arg("key", k.as_str()))
                    }
                };
                Ok((k.clone(), text))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(invalid("crawl.overrides", "tasks-issue-override-invalid")),
    };
    let mut exports = Vec::new();
    for e in &def.exports {
        exports.push(m::ExportItem {
            kind: export_kind_name(e.kind).to_string(),
            name: e.name.clone(),
            format: format_from_dto(e.format)?,
            dir: e.dir.clone().filter(|d| !d.trim().is_empty()),
        });
    }
    let export_options = match &def.export_options {
        None => None,
        Some(o) => Some(m::ExportOptions {
            items: o
                .items
                .iter()
                .map(|s| (s.kind.clone(), s.name.clone()))
                .collect(),
            format: format_from_dto(o.format)?,
            dir: o.dir.clone(),
            single_workbook: o.single_workbook,
            skip_empty: o.skip_empty,
        }),
    };
    let old_exec = existing.map(|t| t.execution.clone()).unwrap_or_default();
    Ok(Task {
        id: def.id.clone().unwrap_or_default(),
        name: def.name.trim().to_string(),
        project: def
            .project
            .clone()
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_default(),
        description: existing.map(|t| t.description.clone()).unwrap_or_default(),
        enabled: def.enabled,
        schedule: schedule_from_dto(&def.schedule, existing.map(|t| &t.schedule)),
        crawl: m::CrawlSpec {
            source: match &def.crawl.source {
                TaskSource::Spider { url } => m::Source::Spider { url: url.clone() },
                TaskSource::List { path } => m::Source::List { path: path.clone() },
            },
            config: def.crawl.profile.clone().filter(|p| !p.trim().is_empty()),
            auth_profile: def
                .crawl
                .auth_profile
                .clone()
                .filter(|p| !p.trim().is_empty()),
            overrides,
        },
        execution: m::Execution {
            overlap: match def.execution.overlap {
                Some(TaskOverlap::Skip) => m::Overlap::Skip,
                Some(TaskOverlap::Parallel) => m::Overlap::Parallel,
                None => old_exec.overlap,
            },
            run_if_missed: def.execution.run_if_missed,
            on_battery: old_exec.on_battery,
            wake: old_exec.wake,
            timeout_min: def.execution.timeout_min,
            auto_compare: def.execution.auto_compare,
        },
        apis: def.apis.iter().map(|a| api_name(*a)).collect(),
        output: existing.map(|t| t.output.clone()).unwrap_or_default(),
        exports,
        export_options,
        preset: def.export_preset.clone().filter(|p| !p.trim().is_empty()),
        custom_summary: def.custom_summary.as_ref().map(|c| m::CustomSummary {
            account: c.account.clone(),
            target: c.target.clone(),
            metrics: c.metrics.clone(),
        }),
        notify: m::Notify {
            on_success: def.notify.on_success,
            on_failure: def.notify.on_failure,
            account: def.notify.account.clone().filter(|a| !a.trim().is_empty()),
            to: def.notify.to.clone(),
        },
    })
}

fn run_state(result: RunResult) -> TaskRunState {
    match result {
        RunResult::Running => TaskRunState::Running,
        RunResult::Completed | RunResult::CompletedWithWarnings => TaskRunState::Succeeded,
        RunResult::Failed => TaskRunState::Failed,
        RunResult::Skipped => TaskRunState::Skipped,
        RunResult::Aborted => TaskRunState::Cancelled,
    }
}

/// Lauf → Zeile der Historie. „Completed with warnings“ ist `succeeded` mit Exit-Code 4 (Vereinbarung mit
/// der Oberfläche); ihre Meldung sind die Warnungen.
fn to_row(record: &RunRecord, operation: Option<OpHandle>) -> TaskRunRow {
    let warnings = record.result == RunResult::CompletedWithWarnings;
    TaskRunRow {
        run_id: record.run_id.clone(),
        task_id: record.task_id.clone(),
        started_at: record.started_at.clone(),
        ended_at: record.ended_at.clone(),
        state: run_state(record.result),
        exit_code: if warnings { Some(4) } else { record.exit_code },
        crawl_id: record.crawl_id.clone(),
        exports: record.exports.clone(),
        error: if warnings {
            (!record.warnings.is_empty()).then(|| record.warnings.join("; "))
        } else if matches!(
            record.result,
            RunResult::Failed | RunResult::Skipped | RunResult::Aborted
        ) {
            record.message.clone()
        } else {
            None
        },
        operation,
        urls_crawled: record.urls_crawled,
        output_folder: record.output_folder.clone(),
        log_path: record.log_path.clone(),
    }
}

fn issue_dto(issue: &Issue) -> ValidationIssue {
    ValidationIssue {
        path: issue.path.clone(),
        message_key: issue.key.clone(),
        args: issue
            .args
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect(),
        line: None,
        column: None,
        warning: issue.warning,
    }
}

// ---------------------------------------------------------------------------------------------------------
// Prüfumgebung

/// Zustände von Konten und Katalog, wie die App sie über ihre eigenen Commands kennt; was nicht
/// verfügbar ist, fällt auf Dateisystem und Schlüsselbund zurück.
struct AppEnv {
    inner: FsEnv,
    mail: Option<BTreeSet<String>>,
    connected: Option<BTreeSet<String>>,
    metrics: Option<BTreeSet<String>>,
}

impl Env for AppEnv {
    fn now(&self) -> Timestamp {
        self.inner.now()
    }
    fn zone(&self) -> &dyn Zone {
        self.inner.zone()
    }
    fn config_exists(&self, r: &str) -> bool {
        self.inner.config_exists(r)
    }
    fn auth_profile_exists(&self, r: &str) -> bool {
        self.inner.auth_profile_exists(r)
    }
    fn dir_writable(&self, p: &Path) -> bool {
        self.inner.dir_writable(p)
    }
    fn default_export_dir(&self) -> PathBuf {
        self.inner.default_export_dir()
    }
    fn credentials(&self, api: &str) -> Credentials {
        match &self.connected {
            Some(connected) if connected.contains(api) => Credentials::Ready,
            _ => self.inner.credentials(api),
        }
    }
    fn google_account_ready(&self, account: &str) -> bool {
        match &self.connected {
            Some(connected) if connected.contains("drive") => true,
            _ => self.inner.google_account_ready(account),
        }
    }
    fn mail_account_exists(&self, id: &str) -> bool {
        match &self.mail {
            Some(ids) => ids.contains(id),
            None => self.inner.mail_account_exists(id),
        }
    }
    fn metric_known(&self, name: &str) -> Option<bool> {
        self.metrics.as_ref().map(|m| m.contains(name))
    }
    fn preset_exists(&self, name: &str) -> bool {
        self.inner.preset_exists(name)
    }
    fn name_taken(&self, name: &str, except: Option<&str>) -> bool {
        self.inner.name_taken(name, except)
    }
}

/// Standard-Exportordner (OUT-222).
fn export_dir(app: &App) -> PathBuf {
    let settings = app.settings_snapshot();
    if let Some(dir) = settings.export.directory.filter(|d| !d.trim().is_empty()) {
        return PathBuf::from(dir);
    }
    app.config()
        .documents_dir
        .clone()
        .map(|d| d.join("OpenSpider").join("Exports"))
        .or_else(os_sched::tasks::store::documents_export_dir)
        .unwrap_or_else(|| settings_dir(app).join("exports"))
}

async fn app_env(app: &App, ctx: &CallCtx) -> AppEnv {
    let secrets: Arc<dyn os_fetch::SecretStore> = app.config().secrets.clone();
    let mut inner = FsEnv::system(&settings_dir(app), secrets, export_dir(app));
    inner.zone = Box::new(SystemZoneBridge(app.ext::<TasksState>().zone()));
    inner.clock = app.config().clock.clone();
    let ask = |name: &'static str| {
        let ctx = ctx.clone();
        async move { app.dispatch(ctx, name, Value::Null).await.ok() }
    };
    let mail = ask("mail.listAccounts").await.and_then(|v| {
        v.as_array().map(|list| {
            list.iter()
                .filter_map(|a| a.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
    });
    let connected = ask("api.list").await.and_then(|v| {
        v.as_array().map(|list| {
            list.iter()
                .filter(|a| a.get("connected").and_then(Value::as_bool) == Some(true))
                .filter_map(|a| a.get("kind").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
    });
    let metrics = ask("export.customSummaryMetrics").await.and_then(|v| {
        v.as_array().map(|list| {
            list.iter()
                .filter_map(|a| a.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
    });
    AppEnv {
        inner,
        mail,
        connected,
        metrics,
    }
}

struct SystemZoneBridge(Arc<dyn Zone>);

impl Zone for SystemZoneBridge {
    fn offset_at(&self, utc: time::OffsetDateTime) -> time::UtcOffset {
        self.0.offset_at(utc)
    }
}

async fn validate_task(app: &App, ctx: &CallCtx, task: &Task) -> Vec<Issue> {
    let env = app_env(app, ctx).await;
    let task = task.clone();
    tokio::task::spawn_blocking(move || validate::validate(&task, &env))
        .await
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------------------
// Planer

fn planner_spec(app: &App, task: &Task) -> PlannerSpec {
    planner_spec_for(planner_home(app), task)
}

/// Eigener Basisordner für den Planer-Eintrag, nur wenn er ausdrücklich gesetzt ist.
fn planner_home(app: &App) -> Option<PathBuf> {
    app.config()
        .home_override
        .as_ref()
        .map(|_| app.config().home.clone())
}

fn planner_spec_for(home: Option<PathBuf>, task: &Task) -> PlannerSpec {
    PlannerSpec {
        task_id: task.id.clone(),
        task_name: task.name.clone(),
        schedule: task.schedule.clone(),
        binary: planner::cli_binary(),
        home,
        run_if_missed: task.execution.run_if_missed,
        on_battery: task.execution.on_battery,
        wake: task.execution.wake,
    }
}

fn preview_dto(preview: planner::Preview) -> PlannerPreview {
    PlannerPreview {
        backend: match preview.backend {
            planner::Backend::Launchd => PlannerBackend::Launchd,
            planner::Backend::TaskScheduler => PlannerBackend::TaskScheduler,
        },
        files: preview
            .files
            .into_iter()
            .map(|f| PlannerFile {
                path: f.path.to_string_lossy().into_owned(),
                content: f.content,
            })
            .collect(),
        command: preview.command,
    }
}

fn planner_error(error: planner::PlannerError) -> CmdError {
    match error {
        planner::PlannerError::Unsupported(what) => {
            CmdError::new(ErrorCode::Unsupported, "err-planner-schedule").with_detail(what)
        }
        planner::PlannerError::Invalid(what) => {
            invalid("schedule", "err-planner-schedule").with_detail(what)
        }
        other => CmdError::new(ErrorCode::Io, "err-planner").with_detail(other.to_string()),
    }
}

/// Trägt eine eingeschaltete Aufgabe beim Planer ein bzw. aus (nach Speichern, Import, Neustart).
fn sync_registration(app: &App, task: &Task) -> Result<(), CmdError> {
    let Some(planner) = planner_of(app) else {
        return Ok(());
    };
    if task.enabled {
        planner
            .install(&planner_spec(app, task))
            .map_err(planner_error)?;
    } else if planner.is_registered(&task.id) {
        planner.uninstall(&task.id).map_err(planner_error)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------
// Beobachtung laufender Läufe

fn info_of(app: &App, task: &Task, now: Timestamp) -> TaskInfo {
    let state = app.ext::<TasksState>();
    let zone = state.zone();
    let last = history(app).last(&task.id).map(|record| {
        let operation = if record.result == RunResult::Running {
            Some(watch_run(app, &record.task_id, &record.run_id, None))
        } else {
            None
        };
        to_row(&record, operation)
    });
    TaskInfo {
        id: task.id.clone(),
        name: task.name.clone(),
        project: Some(task.project_name().to_string()),
        schedule: schedule_to_dto(&task.schedule),
        next_run: if task.enabled {
            next_run(&task.schedule, now, zone.as_ref()).map(|t| t.to_db())
        } else {
            None
        },
        last_run: last,
        enabled: task.enabled,
        registered: planner_of(app).is_some_and(|p| p.is_registered(&task.id)),
    }
}

/// Eine abbrechbare Operation, die einen Lauf beobachtet. Läuft für denselben Lauf schon eine, wird deren
/// Handle wiederverwendet. `op.cancel` legt die Abbruchmarke an; beendet sich die App (Beenden), bleibt der
/// Lauf unberührt.
fn watch_run(
    app: &App,
    task_id: &str,
    run_id: &str,
    child: Option<Box<dyn RunningChild>>,
) -> OpHandle {
    let state = app.ext::<TasksState>();
    {
        let mut watchers = state.watchers.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(handle) = watchers.get(run_id) {
            if app.ops().is_cancellable(&handle.op_id).is_some() {
                return handle.clone();
            }
            watchers.remove(run_id);
        }
    }
    let app2 = app.clone();
    let (task_id, run_id) = (task_id.to_string(), run_id.to_string());
    let key = run_id.clone();
    let handle = app
        .ops()
        .spawn("tasks-op-run", true, move |token, _progress| {
            Box::pin(async move {
                let result = follow_run(&app2, &task_id, &run_id, child, &token).await;
                app2.ext::<TasksState>()
                    .watchers
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&run_id);
                result
            })
        });
    state
        .watchers
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(key, handle.clone());
    handle
}

/// Wartet kurz auf das Ende des Kindprozesses, damit er nicht als Zombie liegen bleibt.
async fn reap(child: Option<&mut Box<dyn RunningChild>>) {
    let Some(child) = child else {
        return;
    };
    for _ in 0..20 {
        if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn follow_run(
    app: &App,
    task_id: &str,
    run_id: &str,
    mut child: Option<Box<dyn RunningChild>>,
    token: &os_analyze::analysis::CancelToken,
) -> CmdResult {
    let history = history(app);
    let clock = app.config().clock.clone();
    let mut cancel_sent: Option<tokio::time::Instant> = None;
    let mut child_gone: Option<tokio::time::Instant> = None;
    let mut last_reconcile = tokio::time::Instant::now();
    loop {
        if let Some(record) = history.get(task_id, run_id) {
            if record.result != RunResult::Running {
                reap(child.as_mut()).await;
                if cancel_sent.is_some() && record.result == RunResult::Aborted {
                    return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
                }
                return to_json(&to_row(&record, None));
            }
        }
        if last_reconcile.elapsed() >= Duration::from_secs(5) {
            last_reconcile = tokio::time::Instant::now();
            history.reconcile(clock.now());
        }
        if let Some(c) = child.as_mut() {
            if let Ok(Some(_)) = c.try_wait() {
                // Der Prozess ist beendet; sein letzter Eintrag kann einen Moment später sichtbar sein.
                let since = *child_gone.get_or_insert_with(tokio::time::Instant::now);
                if since.elapsed() > Duration::from_secs(2) {
                    let mut record = history
                        .get(task_id, run_id)
                        .unwrap_or_else(|| RunRecord::start(run_id, task_id, task_id, clock.now()));
                    if record.result == RunResult::Running {
                        record.message = Some("unexpected termination".into());
                        record.finish(RunResult::Failed, None, clock.now());
                    }
                    let _ = history.write(&record);
                    return to_json(&to_row(&record, None));
                }
            }
        }
        if token.is_cancelled() && cancel_sent.is_none() {
            if app.ops().shutting_down() {
                // Die App beendet sich: der Lauf ist ein eigener Prozess und läuft weiter.
                return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
            }
            let _ = history.request_cancel(task_id, run_id);
            cancel_sent = Some(tokio::time::Instant::now());
        }
        if let Some(at) = cancel_sent {
            if at.elapsed() > Duration::from_secs(20) {
                if let Some(c) = child.as_mut() {
                    let _ = c.kill();
                }
                if let Some(mut record) = history.get(task_id, run_id) {
                    if record.result == RunResult::Running {
                        record.message = Some("aborted".into());
                        record.finish(RunResult::Aborted, Some(130), clock.now());
                        let _ = history.write(&record);
                    }
                }
                return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

// ---------------------------------------------------------------------------------------------------------
// Commands

fn run<'a, F, Fut>(app: &'a App, f: F) -> BoxFuture<'a, CmdResult>
where
    F: FnOnce(&'a App) -> Fut + Send + 'a,
    Fut: std::future::Future<Output = CmdResult> + Send + 'a,
{
    Box::pin(f(app))
}

fn list<'a>(app: &'a App, _ctx: CallCtx, _args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let app2 = app.clone();
        let infos = tokio::task::spawn_blocking(move || {
            let now = app2.config().clock.now();
            history(&app2).reconcile(now);
            store(&app2)
                .list()
                .iter()
                .map(|t| info_of(&app2, t, now))
                .collect::<Vec<_>>()
        })
        .await
        .map_err(io_error)?;
        to_json(&infos)
    })
}

fn get<'a>(app: &'a App, _ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let IdArgs { id } = parse_args(args)?;
        let task = store(app).get(&id).map_err(store_error)?;
        to_json(&to_def(&task))
    })
}

fn validate_cmd<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let TaskArgs { task: def } = parse_args(args)?;
        let existing = def.id.as_deref().and_then(|id| store(app).get(id).ok());
        let task = match from_def(&def, existing.as_ref()) {
            Ok(task) => task,
            Err(e) => {
                let issue = ValidationIssue {
                    path: e.field.clone().unwrap_or_default(),
                    message_key: e.message_key.clone(),
                    args: e.args.clone(),
                    line: None,
                    column: None,
                    warning: false,
                };
                return to_json(&vec![issue]);
            }
        };
        let issues = validate_task(app, &ctx, &task).await;
        to_json(&issues.iter().map(issue_dto).collect::<Vec<_>>())
    })
}

fn save<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let TaskArgs { task: def } = parse_args(args)?;
        let existing = def.id.as_deref().and_then(|id| store(app).get(id).ok());
        let task = from_def(&def, existing.as_ref())?;
        let issues = validate_task(app, &ctx, &task).await;
        if let Some(first) = issues.iter().find(|i| !i.warning) {
            let mut error = invalid(&first.path, &first.key);
            for (k, v) in &first.args {
                error = error.with_arg(k, v.as_str());
            }
            return Err(error);
        }
        let app2 = app.clone();
        let (saved, registered_before) = tokio::task::spawn_blocking(move || {
            let saved = store(&app2).save(task).map_err(store_error)?;
            let before = planner_of(&app2).is_some_and(|p| p.is_registered(&saved.id));
            Ok::<_, CmdError>((saved, before))
        })
        .await
        .map_err(io_error)??;
        // Eine bereits eingetragene Aufgabe zeigt nach dem Bearbeiten den neuen Zeitplan (OUT-160); neue
        // Aufgaben trägt der Aufrufer mit `tasks.register` ein.
        if registered_before {
            sync_registration(app, &saved)?;
        }
        let now = app.config().clock.now();
        to_json(&info_of(app, &saved, now))
    })
}

fn delete<'a>(app: &'a App, _ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let IdArgs { id } = parse_args(args)?;
        store(app).get(&id).map_err(store_error)?;
        if let Some(planner) = planner_of(app) {
            if let Err(e) = planner.uninstall(&id) {
                tracing::warn!("Planer-Eintrag von '{id}' nicht entfernt: {e}");
            }
        }
        store(app).delete(&id).map_err(store_error)?;
        // Erzeugte Crawls und Exporte bleiben; die Historie gehört zur Aufgabe.
        history(app).delete_task(&id);
        Ok(Value::Null)
    })
}

fn run_now<'a>(app: &'a App, _ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let IdArgs { id } = parse_args(args)?;
        let task = store(app).get(&id).map_err(store_error)?;
        let launcher = launcher_of(app).ok_or_else(|| {
            CmdError::new(ErrorCode::Unsupported, "err-task-launcher-unavailable")
        })?;
        let run_id = app
            .config()
            .ids
            .new_crawl_id(app.config().clock.now())
            .to_string();
        let request = LaunchRequest {
            task_id: task.id.clone(),
            run_id: run_id.clone(),
            home: app.config().home.clone(),
        };
        let child = tokio::task::spawn_blocking(move || launcher.launch(request))
            .await
            .map_err(io_error)?
            .map_err(|e| {
                CmdError::new(ErrorCode::Io, "err-task-launch").with_detail(e.to_string())
            })?;
        let handle = watch_run(app, &task.id, &run_id, Some(child));
        to_json(&handle)
    })
}

fn history_cmd<'a>(app: &'a App, _ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let TasksHistoryArgs { id, offset, limit } = parse_args(args)?;
        let app2 = app.clone();
        let rows = tokio::task::spawn_blocking(move || {
            let h = history(&app2);
            h.reconcile(app2.config().clock.now());
            h.list(
                id.as_deref(),
                usize::try_from(offset).unwrap_or(usize::MAX),
                usize::try_from(limit).unwrap_or(100).min(1000),
            )
            .iter()
            .map(|r| {
                let operation = (r.result == RunResult::Running)
                    .then(|| watch_run(&app2, &r.task_id, &r.run_id, None));
                to_row(r, operation)
            })
            .collect::<Vec<_>>()
        })
        .await
        .map_err(io_error)?;
        to_json(&rows)
    })
}

fn open_log<'a>(app: &'a App, _ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let IdArgs { id } = parse_args(args)?;
        let record = history(app)
            .find(&id)
            .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-task-run-not-found"))?;
        let path = record
            .log_path
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-task-log-not-found"))?;
        let desktop = app
            .desktop_actions()
            .ok_or_else(|| CmdError::new(ErrorCode::Unsupported, "err-desktop-platform"))?;
        desktop.reveal_paths(&[path])?;
        Ok(Value::Null)
    })
}

fn register_task<'a>(app: &'a App, _ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let TaskRegisterArgs { id, dry_run } = parse_args(args)?;
        let task = store(app).get(&id).map_err(store_error)?;
        let planner = planner_of(app).ok_or_else(no_planner)?;
        let spec = planner_spec(app, &task);
        let preview = if dry_run {
            planner.preview(&spec)
        } else {
            planner.install(&spec)
        }
        .map_err(planner_error)?;
        to_json(&preview_dto(preview))
    })
}

fn unregister<'a>(app: &'a App, _ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let IdArgs { id } = parse_args(args)?;
        store(app).get(&id).map_err(store_error)?;
        let planner = planner_of(app).ok_or_else(no_planner)?;
        planner.uninstall(&id).map_err(planner_error)?;
        Ok(Value::Null)
    })
}

fn config_files(app: &App, tasks: &[Task]) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    for task in tasks {
        let Some(reference) = task.crawl.config.as_deref() else {
            continue;
        };
        let path = Path::new(reference);
        if path.extension().is_some_and(|e| e == "osconfig") || path.components().count() > 1 {
            continue;
        }
        let file = settings_dir(app)
            .join("profiles")
            .join(format!("{reference}.osconfig"));
        if file.is_file()
            && !files
                .iter()
                .any(|(n, _): &(String, PathBuf)| n == reference)
        {
            files.push((reference.to_string(), file));
        }
    }
    files
}

fn export_cmd<'a>(app: &'a App, _ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let TasksExportArgs { ids, path } = parse_args(args)?;
        if ids.is_empty() || path.trim().is_empty() {
            return Err(invalid("ids", "err-invalid-args"));
        }
        let app2 = app.clone();
        let handle = app
            .ops()
            .spawn("tasks-op-export", false, move |_token, _p| {
                Box::pin(async move {
                    tokio::task::spawn_blocking(move || write_export(&app2, &ids, Path::new(&path)))
                        .await
                        .map_err(io_error)?
                        .and_then(|r| to_json(&r))
                })
            });
        to_json(&handle)
    })
}

fn write_export(app: &App, ids: &[String], path: &Path) -> Result<FileResult, CmdError> {
    let store = store(app);
    let tasks: Vec<Task> = ids
        .iter()
        .map(|id| store.get(id).map_err(store_error))
        .collect::<Result<_, _>>()?;
    let text = bundle::export_text(&tasks).map_err(io_error)?;
    let zip = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io_error)?;
    }
    if zip {
        // Konfigurationsprofile der Aufgaben werden mitgepackt (OUT-170); Geheimnisse nie.
        let file = std::fs::File::create(&tmp).map_err(io_error)?;
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        use std::io::Write;
        writer
            .start_file("tasks.ostask", options)
            .map_err(io_error)?;
        writer.write_all(text.as_bytes()).map_err(io_error)?;
        for (name, file) in config_files(app, &tasks) {
            writer
                .start_file(format!("profiles/{name}.osconfig"), options)
                .map_err(io_error)?;
            writer
                .write_all(&std::fs::read(file).map_err(io_error)?)
                .map_err(io_error)?;
        }
        writer.finish().map_err(io_error)?;
    } else {
        std::fs::write(&tmp, text.as_bytes()).map_err(io_error)?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_error(e)
    })?;
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(FileResult {
        path: path.to_string_lossy().into_owned(),
        bytes,
    })
}

fn import_cmd<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    run(app, |app| async move {
        let PathArgs { path } = parse_args(args)?;
        if path.trim().is_empty() {
            return Err(invalid("path", "err-invalid-args"));
        }
        let app2 = app.clone();
        let handle = app
            .ops()
            .spawn("tasks-op-import", false, move |_token, _p| {
                Box::pin(async move {
                    let imported = {
                        let app3 = app2.clone();
                        tokio::task::spawn_blocking(move || read_import(&app3, Path::new(&path)))
                            .await
                            .map_err(io_error)??
                    };
                    // Eingeschaltete, gültige Aufgaben werden beim Planer eingetragen; die übrigen erscheinen als
                    // ungültig mit konkretem Grund (OUT-170).
                    let mut infos = Vec::new();
                    for task in &imported {
                        let valid = validate_task(&app2, &ctx, task)
                            .await
                            .iter()
                            .all(|i| i.warning);
                        if task.enabled && valid {
                            if let Err(e) = sync_registration(&app2, task) {
                                tracing::warn!("Aufgabe '{}' nicht eingetragen: {e}", task.id);
                            }
                        }
                        infos.push(info_of(&app2, task, app2.config().clock.now()));
                    }
                    to_json(&infos)
                })
            });
        to_json(&handle)
    })
}

fn read_import(app: &App, path: &Path) -> Result<Vec<Task>, CmdError> {
    let bytes = std::fs::read(path).map_err(io_error)?;
    let text = if bytes.starts_with(b"PK") {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| invalid("path", "err-task-file").with_detail(e.to_string()))?;
        let mut text = String::new();
        {
            use std::io::Read;
            let mut entry = archive
                .by_name("tasks.ostask")
                .map_err(|e| invalid("path", "err-task-file").with_detail(e.to_string()))?;
            entry.read_to_string(&mut text).map_err(io_error)?;
        }
        // Mitgepackte Profile: vorhandene bleiben unverändert.
        let profiles = settings_dir(app).join("profiles");
        for i in 0..archive.len() {
            let Ok(mut entry) = archive.by_index(i) else {
                continue;
            };
            let name = entry.name().map(|n| n.to_string()).unwrap_or_default();
            let Some(stem) = name
                .strip_prefix("profiles/")
                .and_then(|n| n.strip_suffix(".osconfig"))
            else {
                continue;
            };
            if stem.is_empty() || stem.contains(['/', '\\', ':']) || stem.contains("..") {
                continue;
            }
            let dest = profiles.join(format!("{stem}.osconfig"));
            if dest.exists() {
                continue;
            }
            use std::io::Read;
            let mut content = Vec::new();
            if entry.read_to_end(&mut content).is_ok() {
                let _ = std::fs::create_dir_all(&profiles);
                let _ = std::fs::write(dest, content);
            }
        }
        text
    } else {
        String::from_utf8(bytes)
            .map_err(|e| invalid("path", "err-task-file").with_detail(e.to_string()))?
    };
    bundle::import_into(&store(app), &text).map_err(|e| match e {
        bundle::BundleError::Store(s) => store_error(s),
        other => invalid("path", "err-task-file").with_detail(other.to_string()),
    })
}

/// Start der App: abgestürzte Läufe abgleichen und Planer-Einträge nachziehen, die auf ein anderes
/// `openspider`-Programm zeigen (nach einem Update, OUT-166).
///
/// Die Hintergrundarbeit hält die `App` nicht: Ein Abbau durch ihren Besitzer direkt nach dem Start bleibt der letzte
/// und wartet damit auf die Hintergrundspuren der Schreibwarteschlange, bevor der Besitzer Verzeichnisse entfernt
/// (OUT-229).
pub fn on_startup(app: &App) {
    let settings = settings_dir(app);
    let clock = app.config().clock.clone();
    let state = app.ext::<TasksState>();
    let home = planner_home(app);
    app.handle().spawn_blocking(move || {
        let h = HistoryStore::in_settings(&settings);
        let fixed = h.reconcile(clock.now());
        if !fixed.is_empty() {
            tracing::warn!("{} abgestürzte Aufgabenläufe abgeglichen", fixed.len());
        }
        let Some(planner) = planner_in(&state, &settings) else {
            return;
        };
        for task in TaskStore::in_settings(&settings).list() {
            if !task.enabled || !planner.is_registered(&task.id) {
                continue;
            }
            let spec = planner_spec_for(home.clone(), &task);
            if let Err(e) = planner::reregister::repair(planner.as_ref(), &spec) {
                tracing::warn!("Aufgabe '{}' nicht neu eingetragen: {e}", task.id);
            }
        }
    });
}

/// Trägt alle OpenSpider-Aufgaben beim Betriebssystem aus (Deinstallation, OUT-166); Anzahl.
pub fn unregister_all(planner: &dyn OsPlanner) -> usize {
    planner::uninstall_all(planner)
}

pub fn register(r: &mut Registry) {
    let table: [(&'static str, &'static str, &'static str, bool, Handler); 12] = [
        ("tasks.list", "void", "TaskInfo[]", false, list),
        ("tasks.get", "IdArgs", "TaskDef", false, get),
        (
            "tasks.validate",
            "TaskArgs",
            "ValidationIssue[]",
            false,
            validate_cmd,
        ),
        ("tasks.save", "TaskArgs", "TaskInfo", false, save),
        ("tasks.delete", "IdArgs", "void", false, delete),
        ("tasks.run", "IdArgs", "TaskRunRow", true, run_now),
        (
            "tasks.history",
            "TasksHistoryArgs",
            "TaskRunRow[]",
            false,
            history_cmd,
        ),
        // Schnittstellenbedarf M2-U27 / OUT-169: IdArgs.id ist die Laufkennung.
        ("tasks.openLog", "IdArgs", "void", false, open_log),
        (
            "tasks.register",
            "TaskRegisterArgs",
            "PlannerPreview",
            false,
            register_task,
        ),
        ("tasks.unregister", "IdArgs", "void", false, unregister),
        (
            "tasks.export",
            "TasksExportArgs",
            "FileResult",
            true,
            export_cmd,
        ),
        ("tasks.import", "PathArgs", "TaskInfo[]", true, import_cmd),
    ];
    for (name, args, ret, long_running, handler) in table {
        r.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running,
            },
            handler,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_sched::planner::Backend;
    use std::sync::mpsc;

    // spec: OUT-166
    // spec: OUT-170
    #[test]
    fn startup_resolution_cannot_overwrite_an_injected_planner() {
        for has_system_planner in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let state = Arc::new(TasksState::default());
            let injected = planner::dry_run(Backend::Launchd, &dir.path().join("injected"));
            let system = has_system_planner
                .then(|| planner::dry_run(Backend::TaskScheduler, &dir.path().join("system")));
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            std::thread::scope(|scope| {
                let resolving_state = state.clone();
                let resolving = scope.spawn(move || {
                    resolving_state.resolve_planner(|| {
                        entered_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        system
                    })
                });
                entered_rx.recv().unwrap();
                // Die Gate hält die Auflösung offen, ohne Sleeps oder einen echten OS-Planer.
                // Linux liefert None; auch diese Auflösung muss den Austausch sperren.
                let locked = state.planner.try_lock().is_err();
                let replacing = scope.spawn(|| state.set_planner(injected.clone()));
                release_tx.send(()).unwrap();
                assert_eq!(resolving.join().unwrap().is_some(), has_system_planner);
                replacing.join().unwrap();
                assert!(
                    locked,
                    "Auflösung und Austausch müssen dieselbe Sperre halten"
                );
            });
            let resolved = state
                .resolve_planner(|| panic!("injizierter Planer muss erhalten bleiben"))
                .unwrap();
            assert!(Arc::ptr_eq(&resolved, &injected));
        }
    }
}

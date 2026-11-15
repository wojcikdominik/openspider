//! App-Seite der Absturzberichte (OUT-246/OUT-247): Start (Aufbewahrung, Sitzungsmarke, Hinweisliste), Zustand für
//! den Panic-Hook, Fehlergrenzen der Oberfläche, Liste/Ansehen/Verwerfen/Löschen. Senden steht in [`super::send`].

use super::session::{self, Marker, ProcessTable, SystemProcesses};
use super::shared::{self, Context, Report, Sanitizer, Static};
use crate::dto::{CmdError, CrashFrontendArgs, CrashOverview, CrashReportSummary, ErrorCode};
use crate::pool::CrawlKey;
use crate::runtime::lifecycle_bus::CrawlEvent;
use crate::update::policy::{self, PolicySource};
use crate::App;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

/// Höchstens so viele `frontend`-Berichte je App-Lauf.
pub const MAX_FRONTEND_REPORTS: usize = 5;
/// Datei der verworfenen Berichte (Hinweis „Nicht senden“).
pub const DISMISSED_FILE: &str = "dismissed.json";

static SESSION_TRACKING: AtomicBool = AtomicBool::new(false);
/// Ordner und Inhalt der eigenen Sitzungsmarke (für den Panic-Hook und Zustandswechsel).
static MARKER: Mutex<Option<(PathBuf, Marker)>> = Mutex::new(None);
/// Marke wegen des Update-Ausgangs entfernt, Inhalt noch in `MARKER` (nur unter der Sperre von `MARKER` ändern).
/// Solange gesetzt, ändern Panic-Hook und Zustandswechsel nur den Inhalt, schreiben aber keine Datei.
static SUSPENDED: AtomicBool = AtomicBool::new(false);

/// Schaltet die Sitzungsmarke für diesen Prozess ein. Die Desktop-App ruft das vor `App::new` auf; Tests, Bridge und
/// eingebettete Apps legen ohne diesen Aufruf keine Marke an (in Debug-Builds zusätzlich über
/// `OPENSPIDER_CRASH_SESSION=1`, für E2E-Tests mit der Bridge).
pub fn enable_session_tracking() {
    SESSION_TRACKING.store(true, Ordering::SeqCst);
}

fn session_tracking() -> bool {
    SESSION_TRACKING.load(Ordering::SeqCst)
        || (cfg!(debug_assertions)
            && std::env::var("OPENSPIDER_CRASH_SESSION").as_deref() == Ok("1"))
}

/// Zustand je App.
#[derive(Default)]
pub struct CrashService {
    state: Mutex<State>,
    /// Sendungen laufen nacheinander.
    pub(crate) send_gate: tokio::sync::Mutex<()>,
}

#[derive(Default)]
struct State {
    context: Option<Arc<Context>>,
    /// Kennungen für den Hinweis: beim Start vorhandene, ungesendete und nicht verworfene Berichte.
    pending: Vec<String>,
    frontend_count: usize,
    frontend_seen: HashSet<String>,
    running: HashMap<CrawlKey, bool>,
    endpoint: Option<String>,
    timeout: Option<Duration>,
    policy: Option<Arc<dyn PolicySource + Send + Sync>>,
}

impl CrashService {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Ordner der Berichte dieser App.
pub fn dir(app: &App) -> PathBuf {
    app.config().paths.logs.join(shared::DIR)
}

fn homes(app: &App) -> Vec<PathBuf> {
    let mut homes: Vec<PathBuf> = dirs::home_dir().into_iter().collect();
    if let Some(h) = std::env::var_os("OPENSPIDER_HOME").filter(|h| !h.is_empty()) {
        homes.push(PathBuf::from(h));
    }
    if let Some(h) = &app.config().home_override {
        homes.push(PathBuf::from(h));
    }
    // Aufbau wie OPENSPIDER_HOME (Tests, Bridge): Basisordner enthält die Logs.
    let config = app.config();
    if config.paths.logs.starts_with(&config.home) {
        homes.push(config.home.clone());
    }
    homes
}

/// Start der App (aus `logging::init_for_app`, vor dem Log-Abonnenten): Kontext für den Panic-Hook, Prüfung der
/// Sitzungsmarken, Aufbewahrung, Liste für den Hinweis, Zustand laufender Crawls. Liefert die Kennung der
/// Startzeile, die der Aufrufer nach dem Einrichten des Logs schreibt.
pub fn on_app_start(app: &App) -> Option<String> {
    let logs = app.config().paths.logs.clone();
    let dir = logs.join(shared::DIR);
    let log = logs.join("openspider.log");
    let offset = fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
    let tracking = session_tracking();
    let context = Arc::new(Context {
        dir: dir.clone(),
        info: Static::current("app"),
        sanitizer: Sanitizer::new(homes(app)),
        log: Some((log.clone(), offset)),
        started: Instant::now(),
        on_written: tracking.then_some(mark_panic as fn(&Report)),
    });
    let mut session_id = None;
    if tracking {
        session::scan(&context, &SystemProcesses, &log);
        let pid = std::process::id();
        if let Some(start_time) = SystemProcesses.start_time(pid) {
            let id = shared::new_id();
            let marker = Marker {
                pid,
                start_time,
                version: context.info.app_version.clone(),
                build: context.info.build.clone(),
                log_offset: offset,
                session: id.clone(),
                crawl_running: false,
                rendering_active: false,
                panic_report: false,
            };
            if session::write_marker(&dir, &marker).is_ok() {
                let mut guard = MARKER.lock().unwrap_or_else(|p| p.into_inner());
                SUSPENDED.store(false, Ordering::SeqCst);
                *guard = Some((dir.clone(), marker));
                session_id = Some(id);
            }
        }
    }
    if let Err(e) = shared::enforce_retention(&dir, SystemTime::now()) {
        tracing::warn!(error = %e.kind(), "Aufbewahrung der Absturzberichte fehlgeschlagen");
    }
    let dismissed = read_dismissed(&dir);
    let pending = shared::list(&dir)
        .into_iter()
        .filter(|s| s.report.sent.is_none() && !dismissed.contains(&s.report.id))
        .map(|s| s.report.id)
        .collect();
    shared::install(context.clone());
    {
        let service = app.ext::<CrashService>();
        let mut state = service.lock();
        state.context = Some(context);
        state.pending = pending;
    }
    app.crawl_events().subscribe(Box::new(on_crawl_event));
    session_id
}

/// Geordnetes Beenden: eigene Sitzungsmarke entfernen.
pub fn on_orderly_exit() {
    let mut guard = MARKER.lock().unwrap_or_else(|p| p.into_inner());
    SUSPENDED.store(false, Ordering::SeqCst);
    if let Some((dir, marker)) = guard.take() {
        let _ = fs::remove_file(session::marker_path(&dir, marker.pid));
    }
}

/// Update-Ausgang (Windows: Installer startet, danach `process::exit(0)` ohne `RunEvent::Exit`): Markendatei
/// entfernen, Inhalt behalten, damit [`resume_after_update_exit`] sie wieder anlegen kann. Idempotent.
pub fn on_update_exit() {
    let guard = MARKER.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((dir, marker)) = guard.as_ref() {
        SUSPENDED.store(true, Ordering::SeqCst);
        let _ = fs::remove_file(session::marker_path(dir, marker.pid));
    }
}

/// Die App läuft nach [`on_update_exit`] doch weiter (Installer nicht gestartet): Markendatei aus dem aktuellen
/// Inhalt wieder anlegen. Ohne vorherigen Update-Ausgang oder doppelt aufgerufen wirkungslos.
pub fn resume_after_update_exit() {
    let guard = MARKER.lock().unwrap_or_else(|p| p.into_inner());
    if SUSPENDED.swap(false, Ordering::SeqCst) {
        if let Some((dir, marker)) = guard.as_ref() {
            let _ = session::write_marker(dir, marker);
        }
    }
}

fn mark_panic(_: &Report) {
    // Im Panic-Hook: nicht blockieren.
    for _ in 0..200 {
        match MARKER.try_lock() {
            Ok(mut guard) => {
                if let Some((dir, marker)) = guard.as_mut() {
                    marker.panic_report = true;
                    if !SUSPENDED.load(Ordering::SeqCst) {
                        let _ = session::write_marker(dir, marker);
                    }
                }
                return;
            }
            Err(std::sync::TryLockError::Poisoned(p)) => {
                if let Some((dir, marker)) = p.into_inner().as_mut() {
                    marker.panic_report = true;
                    if !SUSPENDED.load(Ordering::SeqCst) {
                        let _ = session::write_marker(dir, marker);
                    }
                }
                return;
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn on_crawl_event(app: &App, event: &CrawlEvent) {
    let service = app.ext::<CrashService>();
    let (running, rendering) = {
        let mut state = service.lock();
        match event {
            CrawlEvent::Started(key) | CrawlEvent::Resumed(key) => {
                let rendering = app.sessions().all().iter().any(|s| {
                    &s.key == key && s.config.rendering.mode == os_model::RenderingMode::JavaScript
                });
                state.running.insert(key.clone(), rendering);
            }
            CrawlEvent::Paused(key)
            | CrawlEvent::Stopped(key)
            | CrawlEvent::Completed(key)
            | CrawlEvent::Closed(key)
            | CrawlEvent::Cleared(key) => {
                state.running.remove(key);
            }
            _ => return,
        }
        (
            !state.running.is_empty(),
            state.running.values().any(|r| *r),
        )
    };
    shared::CRAWL_RUNNING.store(running, Ordering::Relaxed);
    shared::RENDERING_ACTIVE.store(rendering, Ordering::Relaxed);
    let mut guard = MARKER.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((dir, marker)) = guard.as_mut() {
        if marker.crawl_running != running || marker.rendering_active != rendering {
            marker.crawl_running = running;
            marker.rendering_active = rendering;
            // Nach dem Update-Ausgang keine Datei anlegen; `resume_after_update_exit` schreibt diesen Stand.
            if !SUSPENDED.load(Ordering::SeqCst) {
                let _ = session::write_marker(dir, marker);
            }
        }
    }
}

/// Testbefehl, nur in Debug-Builds: `OPENSPIDER_CRASH_TEST=panic` löst kurz nach dem Start einen Panic in einem
/// eigenen Thread aus (OUT-246, Kriterium 1).
/// Aufruf nach dem Einrichten des Panic-Hooks (`logging::init_for_app`).
#[cfg(debug_assertions)]
pub fn test_trigger() {
    if std::env::var("OPENSPIDER_CRASH_TEST").as_deref() == Ok("panic") {
        let _ = std::thread::Builder::new()
            .name("crash-test".into())
            .spawn(|| panic!("OpenSpider crash test (OPENSPIDER_CRASH_TEST=panic)"));
    }
}

// --- Verworfene Berichte ---

fn read_dismissed(dir: &Path) -> HashSet<String> {
    fs::read_to_string(dir.join(DISMISSED_FILE))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

fn write_dismissed(dir: &Path, ids: &HashSet<String>) -> std::io::Result<()> {
    let mut list: Vec<&String> = ids.iter().collect();
    list.sort();
    fs::create_dir_all(dir)?;
    let text = serde_json::to_vec_pretty(&list).map_err(std::io::Error::other)?;
    shared::write_atomic(&dir.join(DISMISSED_FILE), &text)
}

fn io_error(e: &std::io::Error) -> CmdError {
    CmdError::new(ErrorCode::Io, "err-crash-io").with_detail(e.kind().to_string())
}

// --- Commands (Logik) ---

/// Ist das Senden durch eine Richtlinie deaktiviert?
pub fn disabled_by_policy(app: &App) -> bool {
    let source = app.ext::<CrashService>().lock().policy.clone();
    match source {
        Some(source) => policy::crash_reports_disabled(source.as_ref()),
        None => policy::crash_reports_disabled(&policy::SystemPolicy),
    }
}

/// Übersicht für den Hinweis und Hilfe → „Absturzberichte…“.
pub fn overview(app: &App) -> CrashOverview {
    let dir = dir(app);
    let stored = shared::list(&dir);
    let dismissed = read_dismissed(&dir);
    let disabled = disabled_by_policy(app);
    let prompt = app.settings_snapshot().crash_reports.prompt;
    let snapshot: HashSet<String> = app
        .ext::<CrashService>()
        .lock()
        .pending
        .iter()
        .cloned()
        .collect();
    let pending = if disabled || !prompt {
        Vec::new()
    } else {
        stored
            .iter()
            .filter(|s| {
                snapshot.contains(&s.report.id)
                    && s.report.sent.is_none()
                    && !dismissed.contains(&s.report.id)
            })
            .map(|s| s.report.id.clone())
            .collect()
    };
    CrashOverview {
        reports: stored
            .into_iter()
            .map(|s| CrashReportSummary {
                id: s.report.id,
                kind: s.report.kind,
                time: s.report.time,
                process: s.report.process,
                message: s.report.message,
                sent: s.report.sent,
                server_ref: s.report.server_ref,
            })
            .collect(),
        pending,
        prompt_enabled: prompt,
        disabled_by_policy: disabled,
    }
}

/// Gespeicherter Bericht mit dieser Kennung.
pub fn find(app: &App, id: &str) -> Result<shared::Stored, CmdError> {
    shared::list(&dir(app))
        .into_iter()
        .find(|s| s.report.id == id)
        .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-crash-not-found").with_arg("id", id))
}

/// Genau der Text, der als `report` gesendet würde.
pub fn read(app: &App, id: &str) -> Result<String, CmdError> {
    find(app, id).map(|s| s.text)
}

/// „Nicht senden“: Hinweis für diese Berichte dauerhaft ausblenden; sie bleiben in der Liste.
pub fn dismiss(app: &App, ids: &[String]) -> Result<(), CmdError> {
    let dir = dir(app);
    let existing: HashSet<String> = shared::list(&dir)
        .into_iter()
        .map(|s| s.report.id)
        .collect();
    let mut dismissed: HashSet<String> = read_dismissed(&dir)
        .into_iter()
        .filter(|id| existing.contains(id))
        .collect();
    dismissed.extend(ids.iter().filter(|id| existing.contains(*id)).cloned());
    write_dismissed(&dir, &dismissed).map_err(|e| io_error(&e))
}

/// Löscht einen Bericht.
pub fn delete(app: &App, id: &str) -> Result<(), CmdError> {
    let stored = find(app, id)?;
    fs::remove_file(&stored.path).map_err(|e| io_error(&e))?;
    let dir = dir(app);
    let mut dismissed = read_dismissed(&dir);
    if dismissed.remove(id) {
        let _ = write_dismissed(&dir, &dismissed);
    }
    Ok(())
}

/// Kontext der App (für Berichte außerhalb des Panic-Hooks).
pub fn context(app: &App) -> Option<Arc<Context>> {
    app.ext::<CrashService>().lock().context.clone()
}

/// Funktionsnamen aus einem React-Komponenten-Stack (V8: `at Name (url)`, JavaScriptCore: `Name@url`).
pub fn component_frames(stack: &str) -> Vec<String> {
    stack
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let l = l
                .strip_prefix("at ")
                .or_else(|| l.strip_prefix("in "))
                .unwrap_or(l);
            let name = match (l.find(" ("), l.find('@')) {
                (Some(p), _) => &l[..p],
                (None, Some(p)) => &l[..p],
                (None, None) => l,
            };
            name.trim().to_string()
        })
        .filter(|n| !n.is_empty())
        .take(shared::MAX_FRAMES)
        .collect()
}

/// Fehler einer Fehlergrenze der Oberfläche → `frontend`-Bericht; höchstens 5 je Lauf, gleiche Meldung und Stelle
/// nur einmal. Liefert, ob ein Bericht geschrieben wurde.
pub fn report_frontend(app: &App, args: &CrashFrontendArgs) -> Result<bool, CmdError> {
    let frames = component_frames(&args.component_stack);
    let key = format!(
        "{}\n{}\n{}",
        shared::first_line(&args.message),
        args.region,
        frames.first().map(String::as_str).unwrap_or("")
    );
    let service = app.ext::<CrashService>();
    let context = {
        let mut state = service.lock();
        if state.frontend_count >= MAX_FRONTEND_REPORTS || state.frontend_seen.contains(&key) {
            return Ok(false);
        }
        let Some(context) = state.context.clone() else {
            return Ok(false);
        };
        state.frontend_count += 1;
        state.frontend_seen.insert(key);
        context
    };
    let tail = context
        .log
        .as_ref()
        .map(|(path, offset)| shared::log_tail(path, *offset, usize::MAX))
        .unwrap_or_default();
    let region: String = args
        .region
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(40)
        .collect();
    let thread = if region.is_empty() {
        "ui".to_string()
    } else {
        format!("ui:{region}")
    };
    let mut report = shared::build(
        &context,
        shared::KIND_FRONTEND,
        &thread,
        &args.message,
        "",
        frames,
        tail,
    );
    shared::write_report(&context.dir, &mut report).map_err(|e| io_error(&e))?;
    Ok(true)
}

// --- Test-Schnittstellen (keine Wirkung im normalen Betrieb) ---

/// Ersetzt Ziel-URL und Zeitlimit des Sendens (Mock-Server in Tests).
#[doc(hidden)]
pub fn set_transport_for_tests(app: &App, endpoint: &str, timeout: Option<Duration>) {
    let service = app.ext::<CrashService>();
    let mut state = service.lock();
    state.endpoint = Some(endpoint.to_string());
    state.timeout = timeout;
}

/// Ersetzt die Richtlinienquelle (Rangfolgetests ohne Registry).
#[doc(hidden)]
pub fn set_policy_source_for_tests(app: &App, source: Arc<dyn PolicySource + Send + Sync>) {
    app.ext::<CrashService>().lock().policy = Some(source);
}

pub(crate) fn transport(app: &App) -> (Option<String>, Option<Duration>) {
    let service = app.ext::<CrashService>();
    let state = service.lock();
    (state.endpoint.clone(), state.timeout)
}

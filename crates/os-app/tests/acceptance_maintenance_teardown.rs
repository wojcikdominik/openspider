//! Unabhängige Abnahme der Korrektur „Abbau der Wartungsspur“ (fix/maintenance-teardown): Aufbewahrung (OUT-229)
//! beim Start, stündlich und nach einer Einstellungsänderung, spätes Anlegen von Register und Index, verworfene
//! Wartungsaufträge, Startabgleich der Aufgaben (OUT-166) bei sofortigem Abbau der `App`.
//!
//! Geschrieben nur aus der Spec (`docs/spec/06-output-ops.md`, `docs/spec/01-crawl.md`) und der öffentlichen API,
//! ohne den Test des Bauers.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "runtime_common.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use os_app::commands::tasks::TasksState;
use os_app::dto::{CmdError, ErrorCode};
use os_app::settings_store::SettingsStore;
use os_app::write_queue::WriteQueue;
use os_app::{pool::CrawlKey, runtime::sessions::SessionInfo, App, AppConfig, WindowId};
use os_model::{CrawlConfig, CrawlState, IdSource, ManualClock, SeededIds, Timestamp};
use os_sched::history::{HistoryStore, RunRecord, RunResult};
use os_sched::planner::{self, Backend};
use os_store::{CrawlFile, OpenMode};
use serde_json::{json, Value};

const DAY_MS: i64 = 86_400_000;

/// Zusicherung (präzisiert 27.11.2026): Nach dem Abbau durch den Besitzer ist die `App` innerhalb von 1 s abgebaut. Takt-
/// Schleifen dürfen sie für einen synchronen Tick hochstufen; eine Momentaufnahme direkt nach dem Abbau kann das treffen.
/// Abbruch, sobald sie weg ist: die Grenze ist die Zusicherung, keine feste Wartezeit.
async fn gone_within_one_second(weak: &os_app::runtime::WeakApp) -> bool {
    let start = std::time::Instant::now();
    loop {
        if weak.upgrade().is_none() {
            return true;
        }
        if start.elapsed() >= Duration::from_secs(1) {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

fn app_with_clock(dir: &Path, clock: Arc<ManualClock>) -> App {
    let config = AppConfig::new(dir)
        .with_ids(Arc::new(SeededIds::new(7)))
        .with_clock(clock);
    App::new(config).expect("App::new")
}

fn days_ago(days: i64) -> Timestamp {
    Timestamp::from_unix_millis(common::created_at().unix_millis() - days * DAY_MS)
}

/// Crawl-Datei mit eigener Kennung in `dir`; `ended`: Ende des Crawls (Bezugszeitpunkt der Aufbewahrung).
fn crawl_in(
    dir: &Path,
    seed: u64,
    state: CrawlState,
    ended: Option<Timestamp>,
) -> (String, PathBuf) {
    std::fs::create_dir_all(dir).unwrap();
    let id = SeededIds::new(seed)
        .new_crawl_id(common::created_at())
        .to_string();
    let path = common::make_crawl(dir, &format!("{id}.ospider"));
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.set_meta("crawl_id", &id).unwrap();
    file.set_meta("state", state.as_str()).unwrap();
    file.set_meta("heartbeat_at", &common::created_at().to_db())
        .unwrap();
    if let Some(t) = ended {
        file.set_meta("ended_at", &t.to_db()).unwrap();
    }
    (id, path)
}

fn lock_file(path: &Path) {
    CrawlFile::open(path, OpenMode::ReadWrite)
        .unwrap()
        .set_meta("locked", "1")
        .unwrap();
}

fn registry_files(crawls: &Path) -> [PathBuf; 2] {
    [
        crawls.join(".registry.sqlite"),
        crawls.join(".index.sqlite"),
    ]
}

fn preset(dir: &Path, patch: Value) {
    let (settings, _) = SettingsStore::open(dir);
    settings.apply_patch(&patch).unwrap();
}

async fn call(app: &App, name: &str, args: Value) -> Value {
    app.dispatch(common::ctx(), name, args)
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

async fn listed_ids(app: &App) -> Vec<String> {
    let list = call(app, "crawls.list", json!({"show":"all"})).await;
    let mut ids: Vec<String> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            l["crawl"]["crawlId"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    ids.sort();
    ids
}

/// Wartet im pausierten Laufzeitsystem, bis `cond` gilt (Hängeschutz: Anzahl der Runden, keine Wanduhr im Ergebnis).
async fn until_paused(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..4000 {
        if cond() {
            return true;
        }
        tokio::task::spawn_blocking(|| std::thread::sleep(Duration::from_millis(2)))
            .await
            .unwrap();
    }
    cond()
}

// (ii) Standardrichtlinie: weder beim Start noch nach einer Stunde entstehen Register und Index; mit der ersten
// Verwaltungsaktion schon.
// spec: OUT-229
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn default_policy_creates_neither_registry_nor_index_before_the_first_management_action() {
    let dir = tempfile::tempdir().unwrap();
    let app = common::new_app(dir.path());
    let crawls = app.config().paths.crawls.clone();
    // Startrunde (Hintergrund) Gelegenheit geben zu laufen.
    tokio::time::sleep(Duration::from_secs(1)).await;
    for f in registry_files(&crawls) {
        assert!(!f.exists(), "nach dem Start vorhanden: {}", f.display());
    }
    // Eine simulierte Stunde: die stündliche Runde läuft, legt aber ebenfalls nichts an.
    tokio::time::sleep(Duration::from_secs(3601)).await;
    assert_eq!(
        os_app::commands::crawls::run_retention(&app).await.unwrap(),
        0
    );
    for f in registry_files(&crawls) {
        assert!(!f.exists(), "nach einer Stunde vorhanden: {}", f.display());
    }
    // Erste Verwaltungsaktion: Crawl-Liste.
    assert!(listed_ids(&app).await.is_empty());
    for f in registry_files(&crawls) {
        assert!(f.exists(), "nach crawls.list fehlt {}", f.display());
    }
}

// Stündliche Runde nach der Korrektur: ein Crawl, der nach dem Start fällig wird (6 → 8 Tage), wird spätestens eine
// Stunde später gelöscht; Bezug ist die App-Uhr.
// spec: OUT-229
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn hourly_round_deletes_a_crawl_that_became_due_after_the_start() {
    let dir = tempfile::tempdir().unwrap();
    preset(dir.path(), json!({"retention":{"policy":"one_week"}}));
    let clock = Arc::new(ManualClock::new(common::created_at()));
    let crawls = dir.path().join("crawls");
    let (_, six_days) = crawl_in(&crawls, 401, CrawlState::Completed, Some(days_ago(6)));
    let app = app_with_clock(dir.path(), Arc::clone(&clock));
    // Serialisiert hinter der Startrunde (Verwaltungs-Gate): danach ist sie beendet.
    assert_eq!(
        os_app::commands::crawls::run_retention(&app).await.unwrap(),
        0
    );
    assert!(six_days.exists(), "Crawl vor 6 Tagen bleibt");
    clock.advance(Duration::from_secs(2 * 86_400));
    tokio::time::sleep(Duration::from_secs(3601)).await;
    assert!(
        until_paused(|| !six_days.exists()).await,
        "stündliche Runde hat den inzwischen 8 Tage alten Crawl nicht gelöscht"
    );
    drop(app);
}

// (iii) Wechsel von „Nie“ auf eine Frist zur Laufzeit: Die Runde läuft sofort, Register und Index entstehen, nur der
// fällige Crawl verschwindet; geöffnete, laufende, gesperrte und junge Crawls bleiben.
// spec: OUT-229
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switching_from_never_to_one_week_runs_a_round_at_once_and_spares_protected_crawls() {
    let dir = tempfile::tempdir().unwrap();
    let app = common::new_app(dir.path());
    let crawls = app.config().paths.crawls.clone();
    let (_, expired) = crawl_in(&crawls, 501, CrawlState::Completed, Some(days_ago(8)));
    let (recent_id, recent) = crawl_in(&crawls, 502, CrawlState::Completed, Some(days_ago(6)));
    let (locked_id, locked) = crawl_in(&crawls, 503, CrawlState::Completed, Some(days_ago(400)));
    lock_file(&locked);
    let (open_id, open) = crawl_in(&crawls, 504, CrawlState::Completed, Some(days_ago(400)));
    let (running_id, running) = crawl_in(&crawls, 505, CrawlState::Running, None);
    std::fs::File::options()
        .write(true)
        .open(&running)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(946_684_800))
        .unwrap();
    app.sessions().set_current(
        WindowId("main".into()),
        SessionInfo {
            key: CrawlKey(open_id.clone()),
            path: open.clone(),
            config: Arc::new(CrawlConfig::default()),
            state: CrawlState::Completed,
            writable: true,
            engine: None,
        },
    );
    for f in registry_files(&crawls) {
        assert!(!f.exists(), "Standardrichtlinie legte {} an", f.display());
    }

    call(
        &app,
        "settings.set",
        json!({"patch":{"retention":{"policy":"one_week"}}}),
    )
    .await;
    assert!(
        common::eventually(Duration::from_secs(30), || !expired.exists()).await,
        "Einstellungsänderung löste keine Runde aus"
    );
    // Hinter der Runde serialisiert: sie ist vollständig gelaufen.
    os_app::commands::crawls::run_retention(&app).await.unwrap();
    for f in registry_files(&crawls) {
        assert!(f.exists(), "nach der Runde fehlt {}", f.display());
    }
    for p in [&recent, &locked, &open, &running] {
        assert!(p.exists(), "geschützt/jung, aber gelöscht: {}", p.display());
    }
    let mut expected = vec![recent_id, locked_id, open_id, running_id];
    expected.sort();
    assert_eq!(listed_ids(&app).await, expected);
}

fn five_crawls_one_locked(crawls: &Path) -> Vec<PathBuf> {
    // Älteste zuerst; der älteste ist gesperrt. Alle jünger als eine Woche: Löschgrund ist nur die Höchstzahl.
    let paths: Vec<PathBuf> = (0..5)
        .map(|i| {
            crawl_in(
                crawls,
                601 + i as u64,
                CrawlState::Completed,
                Some(days_ago(5 - i as i64)),
            )
            .1
        })
        .collect();
    lock_file(&paths[0]);
    paths
}

fn assert_keep_two_result(paths: &[PathBuf]) {
    assert!(paths[0].exists(), "gesperrter Crawl gelöscht");
    assert!(!paths[1].exists(), "ältester ungesperrter Crawl blieb");
    assert!(!paths[2].exists(), "zweitältester ungesperrter Crawl blieb");
    assert!(paths[3].exists(), "zweitjüngster Crawl gelöscht");
    assert!(paths[4].exists(), "jüngster Crawl gelöscht");
}

// (iv) „Höchstens 2“ bei Frist „Nie“ schon beim Start: die ältesten ungesperrten über N verschwinden.
// spec: OUT-229
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn keep_last_with_policy_never_deletes_the_oldest_beyond_n_at_start() {
    let dir = tempfile::tempdir().unwrap();
    preset(dir.path(), json!({"retention":{"keepLast":2}}));
    let paths = five_crawls_one_locked(&dir.path().join("crawls"));
    let app = common::new_app(dir.path());
    assert_eq!(
        app.settings_snapshot().retention.policy,
        os_model::RetentionPolicy::Never
    );
    assert!(
        common::eventually(Duration::from_secs(30), || !paths[1].exists()
            && !paths[2].exists())
        .await,
        "Startrunde mit „Höchstens 2“ hat nichts gelöscht"
    );
    os_app::commands::crawls::run_retention(&app).await.unwrap();
    assert_keep_two_result(&paths);
}

// (iv) dasselbe nach einer Einstellungsänderung zur Laufzeit.
// spec: OUT-229
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn setting_keep_last_with_policy_never_runs_a_round_that_deletes_the_oldest_beyond_n() {
    let dir = tempfile::tempdir().unwrap();
    let app = common::new_app(dir.path());
    let paths = five_crawls_one_locked(&app.config().paths.crawls);
    call(
        &app,
        "settings.set",
        json!({"patch":{"retention":{"keepLast":2}}}),
    )
    .await;
    assert!(
        common::eventually(Duration::from_secs(30), || !paths[1].exists()
            && !paths[2].exists())
        .await,
        "Einstellungsänderung „Höchstens 2“ löste keine Runde aus"
    );
    os_app::commands::crawls::run_retention(&app).await.unwrap();
    assert_keep_two_result(&paths);
}

// Spätes Anlegen von Register und Index (Frage A4): Nach einem Speicherortwechsel findet die Crawl-Verwaltung Crawls an
// beiden Orten (CRAWL-124). Vor der Korrektur hat jede Aufbewahrungsrunde – auch bei „Nie“ – den aktuellen
// Speicherort im Register vermerkt; die Runde läuft hier ausdrücklich (wie beim Start/stündlich/nach Änderung).
// spec: CRAWL-124
// spec: OUT-229
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crawls_at_the_previous_location_stay_listed_after_a_location_change_with_policy_never() {
    let dir = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let app = common::new_app(dir.path());
    call(
        &app,
        "settings.set",
        json!({"patch":{"storage":{"crawlsDir":first.path()}}}),
    )
    .await;
    // Ein Crawl, gespeichert am ersten Ort; danach eine Aufbewahrungsrunde (Standardrichtlinie).
    let (id, _) = crawl_in(first.path(), 701, CrawlState::Completed, Some(days_ago(1)));
    os_app::commands::crawls::run_retention(&app).await.unwrap();
    call(
        &app,
        "settings.set",
        json!({"patch":{"storage":{"crawlsDir":second.path()}}}),
    )
    .await;
    assert_eq!(
        listed_ids(&app).await,
        vec![id],
        "Crawl am bisherigen Speicherort fehlt in der Crawl-Verwaltung"
    );
}

// (vi) Ein verworfener Wartungsauftrag meldet einen Fehler statt zu hängen; Abbau aus dem Wartungsauftrag selbst hängt
// nicht; nach dem Abbau wird nichts mehr angenommen.
// spec: OUT-229
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_discarded_maintenance_job_reports_an_error_instead_of_hanging() {
    let queue = Arc::new(WriteQueue::new());
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let q = Arc::clone(&queue);
    let running = queue.enqueue_maintenance(move || {
        started_tx.send(()).unwrap();
        go_rx.recv().unwrap();
        // Abbau aus dem laufenden Wartungsauftrag heraus (eigener Thread der Spur).
        q.shutdown();
        Ok(1)
    });
    tokio::task::spawn_blocking(move || started_rx.recv().unwrap())
        .await
        .unwrap();
    let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&executed);
    let queued = queue.enqueue_maintenance(move || -> Result<i32, CmdError> {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(2)
    });
    go_tx.send(()).unwrap();
    let guard = Duration::from_secs(30);
    assert_eq!(
        tokio::time::timeout(guard, running)
            .await
            .expect("laufender Auftrag hängt")
            .unwrap(),
        1
    );
    let e = tokio::time::timeout(guard, queued)
        .await
        .expect("verworfener Auftrag hängt")
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Internal);
    assert!(
        e.detail
            .as_deref()
            .unwrap_or_default()
            .contains("verworfen"),
        "{e:?}"
    );
    assert!(
        !executed.load(std::sync::atomic::Ordering::SeqCst),
        "nicht begonnener Auftrag lief trotz Abbau"
    );
    let closed = tokio::time::timeout(guard, queue.submit_maintenance(|| Ok(3)))
        .await
        .expect("Auftrag nach dem Abbau hängt")
        .unwrap_err();
    assert_eq!(closed.code, ErrorCode::Internal);
}

// (vi/A2) Der letzte Abbau der `App` innerhalb eines Wartungsauftrags hängt nicht (kein Selbstbeitritt).
// spec: OUT-229
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_last_app_inside_a_maintenance_job_does_not_hang() {
    let dir = tempfile::tempdir().unwrap();
    let app = common::new_app(dir.path());
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let inner = app.clone();
    let job = app.write_queue().enqueue_maintenance(move || {
        go_rx.recv().unwrap();
        drop(inner);
        Ok(7)
    });
    let weak = app.downgrade();
    drop(app);
    go_tx.send(()).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(30), job)
            .await
            .expect("Abbau im Wartungsauftrag hängt")
            .unwrap(),
        7
    );
    assert!(
        gone_within_one_second(&weak).await,
        "App nach dem Abbau im Wartungsauftrag nicht innerhalb von 1 s abgebaut"
    );
}

// (v) Startabgleich der Aufgaben bei sofortigem Abbau der App: abgestürzte Läufe werden abgeglichen, Planer-Einträge
// auf ein altes Programm neu eingetragen – mit dem eigenen Basisordner (`home_override`) im Eintrag.
// spec: OUT-166
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_reconcile_completes_when_the_app_is_dropped_right_after_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig::new(dir.path())
        .with_ids(Arc::new(SeededIds::new(7)))
        .with_clock(Arc::new(ManualClock::new(common::created_at())));
    config.home_override = Some(dir.path().to_string_lossy().into_owned());
    let home = config.home.clone();
    let app = App::new(config).unwrap();
    let planner_dir = dir.path().join("planner");
    app.ext::<TasksState>()
        .set_planner(planner::dry_run(Backend::Launchd, &planner_dir));
    let text = include_str!("../../../tests/fixtures/out-m2-tasks/task-def-basic.json").replace(
        "{OUT}",
        &dir.path().join("x").to_string_lossy().replace('\\', "\\\\"),
    );
    let def: Value = serde_json::from_str(&text).unwrap();
    let id = call(&app, "tasks.save", json!({ "task": def })).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let planner = planner::dry_run(Backend::Launchd, &planner_dir);
    let task = os_sched::tasks::TaskStore::in_settings(dir.path())
        .get(&id)
        .unwrap();
    planner
        .install(&os_sched::planner::PlannerSpec {
            task_id: id.clone(),
            task_name: task.name.clone(),
            schedule: task.schedule.clone(),
            binary: PathBuf::from("/old/OpenSpider.app/Contents/MacOS/openspider"),
            home: None,
            run_if_missed: true,
            on_battery: true,
            wake: false,
        })
        .unwrap();
    let mut dead = RunRecord::start(
        "run-dead",
        &id,
        "Nightly",
        Timestamp::from_db("2025-09-09T01:00:00.000Z").unwrap(),
    );
    dead.heartbeat_at = "2025-09-09T01:00:05.000Z".into();
    HistoryStore::in_settings(dir.path()).write(&dead).unwrap();

    os_app::commands::tasks::on_startup(&app);
    let weak = app.downgrade();
    drop(app);
    assert!(
        gone_within_one_second(&weak).await,
        "Startabgleich hält die App länger als 1 s"
    );

    let current = planner::cli_binary();
    assert!(
        common::eventually(Duration::from_secs(20), || {
            planner.registered_binary(&id).as_deref() == Some(current.as_path())
        })
        .await,
        "Eintrag zeigt auf {:?}",
        planner.registered_binary(&id)
    );
    let plist = std::fs::read_to_string(planner::launchd::plist_path(&planner_dir, &id)).unwrap();
    assert!(
        plist.contains("OPENSPIDER_HOME") && plist.contains(&*home.to_string_lossy()),
        "Basisordner fehlt im erneuerten Eintrag:\n{plist}"
    );
    assert!(
        common::eventually(Duration::from_secs(20), || {
            HistoryStore::in_settings(dir.path())
                .get(&id, "run-dead")
                .is_some_and(|r| r.result == RunResult::Failed)
        })
        .await,
        "abgestürzter Lauf nicht abgeglichen"
    );
}

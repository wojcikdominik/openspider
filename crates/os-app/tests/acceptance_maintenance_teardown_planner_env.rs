//! Unabhängige Abnahme (fix/maintenance-teardown, Frage A5): Der Startabgleich der Aufgaben (OUT-166) beim echten
//! Start der `App` (ohne eingesetzten Planer) liefert mit den Planer-Varianten `dry` und `off` (`OPENSPIDER_PLANNER`)
//! dasselbe Ergebnis wie vorher, auch wenn die `App` direkt nach dem Start fallen gelassen wird.
//!
//! Eigene Testdatei (eigener Prozess), weil die Variante über eine Umgebungsvariable gewählt wird; beide Varianten
//! laufen nacheinander in einem Test. Die Variante `system` trägt in den echten Planer ein und wird nicht geprüft.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "runtime_common.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use os_app::{App, AppConfig};
use os_model::{ManualClock, SeededIds, Timestamp};
use os_sched::history::{HistoryStore, RunRecord, RunResult};
use os_sched::planner::{self, Backend};
use serde_json::{json, Value};

fn config(dir: &Path) -> AppConfig {
    let mut config = AppConfig::new(dir)
        .with_ids(Arc::new(SeededIds::new(7)))
        .with_clock(Arc::new(ManualClock::new(common::created_at())));
    config.home_override = Some(dir.to_string_lossy().into_owned());
    config
}

fn dead_run(settings: &Path, task: &str) {
    let mut dead = RunRecord::start(
        "run-dead",
        task,
        "Nightly",
        Timestamp::from_db("2025-09-09T01:00:00.000Z").unwrap(),
    );
    dead.heartbeat_at = "2025-09-09T01:00:05.000Z".into();
    HistoryStore::in_settings(settings).write(&dead).unwrap();
}

async fn reconciled(settings: &Path, task: &str) -> bool {
    common::eventually(Duration::from_secs(20), || {
        HistoryStore::in_settings(settings)
            .get(task, "run-dead")
            .is_some_and(|r| r.result == RunResult::Failed)
    })
    .await
}

/// Start und sofortiger Abbau; der Abbau darf nicht hängen und die App nicht überleben.
fn start_and_drop(dir: &Path) {
    let app = App::new(config(dir)).expect("App::new");
    let weak = app.downgrade();
    drop(app);
    assert!(weak.upgrade().is_none(), "Startarbeit hält die App");
}

// spec: OUT-166
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_reconcile_with_planner_dry_and_off_survives_an_immediate_drop() {
    // --- dry: Eintrag auf ein altes Programm wird beim Start erneuert, mit eigenem Basisordner.
    std::env::set_var("OPENSPIDER_PLANNER", "dry");
    let dir = tempfile::tempdir().unwrap();
    let dry_dir = dir.path().join("schedules").join("planner-dry");
    let id = {
        let app = App::new(config(dir.path())).expect("App::new");
        let text = include_str!("../../../tests/fixtures/out-m2-tasks/task-def-basic.json")
            .replace(
                "{OUT}",
                &dir.path().join("x").to_string_lossy().replace('\\', "\\\\"),
            );
        let def: Value = serde_json::from_str(&text).unwrap();
        app.dispatch(common::ctx(), "tasks.save", json!({ "task": def }))
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    // Neue Aufgaben trägt erst `tasks.register` ein; hier steht stattdessen ein Eintrag aus einer älteren Version.
    let planner = planner::dry_run(planner::native_backend(), &dry_dir);
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
    dead_run(dir.path(), &id);

    start_and_drop(dir.path());

    let current = planner::cli_binary();
    assert!(
        common::eventually(Duration::from_secs(20), || {
            planner.registered_binary(&id).as_deref() == Some(current.as_path())
        })
        .await,
        "Eintrag zeigt auf {:?}",
        planner.registered_binary(&id)
    );
    if planner::native_backend() == Backend::Launchd {
        let plist = std::fs::read_to_string(planner::launchd::plist_path(&dry_dir, &id)).unwrap();
        assert!(
            plist.contains("OPENSPIDER_HOME") && plist.contains(&*dir.path().to_string_lossy()),
            "Basisordner fehlt im erneuerten Eintrag:\n{plist}"
        );
    }
    assert!(
        reconciled(dir.path(), &id).await,
        "dry: abgestürzter Lauf nicht abgeglichen"
    );

    // --- off: kein Planer, aber der Abgleich abgestürzter Läufe findet trotzdem statt.
    std::env::set_var("OPENSPIDER_PLANNER", "off");
    let off = tempfile::tempdir().unwrap();
    dead_run(off.path(), "task-off");
    start_and_drop(off.path());
    assert!(
        reconciled(off.path(), "task-off").await,
        "off: abgestürzter Lauf nicht abgeglichen"
    );
    assert!(
        !off.path().join("schedules").join("planner-dry").exists()
            && !off.path().join("schedules").join("planner").exists(),
        "off: Planer-Ordner angelegt"
    );
    std::env::remove_var("OPENSPIDER_PLANNER");
}

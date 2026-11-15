//! `unclean_exit` über die Sitzungsmarke mit echten Prozessen (OUT-246): hartes Beenden → genau ein Bericht beim
//! nächsten Start, geordnetes Beenden → keiner, zwei gleichzeitig laufende App-Prozesse melden sich nicht gegenseitig.
//! Jeder App-Start läuft in einem Kindprozess (dieselbe Testdatei); der Elternprozess startet selbst keine App.
use os_app::crash::session::{write_marker, Marker};
use os_app::{App, AppConfig, CallCtx};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const MODE: &str = "OPENSPIDER_C01_SESSION_CHILD";
const HOME: &str = "OPENSPIDER_C01_SESSION_HOME";

fn command(mode: &str, home: &Path) -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(MODE, mode)
        .env(HOME, home)
        .env("OPENSPIDER_SECRET_STORE", "memory")
        .env_remove("OPENSPIDER_CRASH_TEST");
    cmd
}

/// Kindprozess-Einstieg; ohne gesetzte Umgebungsvariable ein leerer Test.
#[test]
fn child_entry() {
    let Ok(mode) = std::env::var(MODE) else {
        return;
    };
    let home = PathBuf::from(std::env::var_os(HOME).unwrap());
    os_app::crash::enable_session_tracking();
    let app = App::new(AppConfig::new(&home)).unwrap();
    match mode.as_str() {
        "run" => {
            tracing::info!(target: "os_app::crash_test", "child-run-marker pid={}", std::process::id());
            println!("READY");
            std::thread::sleep(std::time::Duration::from_secs(120));
        }
        "start" => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let overview = rt
                .block_on(app.dispatch(CallCtx::for_window("main"), "crash.overview", Value::Null))
                .unwrap();
            println!("OVERVIEW {overview}");
            os_app::crash::on_orderly_exit();
        }
        // Ausgang wie beim Windows-Update (tauri-plugin-updater): der Hook (`on_before_exit`) läuft, danach
        // `std::process::exit(0)` ohne `RunEvent::Exit` und ohne Destruktoren. „hard-exit“ ohne Hook als Gegenprobe.
        "update-exit" | "hard-exit" => {
            assert_eq!(markers(&home), 1, "eigene Sitzungsmarke vor dem Ausgang");
            if mode == "update-exit" {
                os_app::crash::on_update_exit();
                os_app::crash::on_update_exit();
                assert_eq!(markers(&home), 0);
            }
            std::process::exit(0);
        }
        // Installer startet nicht: Marke wieder anlegen, danach harter Ausgang.
        "update-resume-hard-exit" => {
            os_app::crash::on_update_exit();
            assert_eq!(markers(&home), 0);
            os_app::crash::resume_after_update_exit();
            assert_own_marker(&home);
            std::process::exit(0);
        }
        // Wiederanlegen ohne Update-Ausgang und doppelt: harmlos, geordnetes Beenden entfernt danach alles.
        "resume-noop" => {
            os_app::crash::resume_after_update_exit();
            os_app::crash::resume_after_update_exit();
            assert_own_marker(&home);
            os_app::crash::on_update_exit();
            os_app::crash::resume_after_update_exit();
            os_app::crash::resume_after_update_exit();
            assert_own_marker(&home);
            os_app::crash::on_orderly_exit();
            assert_eq!(markers(&home), 0);
            os_app::crash::resume_after_update_exit();
            assert_eq!(
                markers(&home),
                0,
                "nach geordnetem Beenden nichts wieder anlegen"
            );
            std::process::exit(0);
        }
        other => panic!("unbekannter Modus {other}"),
    }
}

struct Running(Child);
impl Running {
    fn start(home: &Path) -> Running {
        let mut child = command("run", home)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next() {
                // libtest schreibt „test child_entry ... “ ohne Zeilenende davor.
                Some(Ok(line)) if line.trim_end().ends_with("READY") => break,
                Some(Ok(_)) => continue,
                _ => panic!("Kindprozess nicht bereit"),
            }
        }
        Running(child)
    }
    /// Hartes Beenden (SIGKILL bzw. TerminateProcess).
    fn kill(mut self) {
        self.0.kill().unwrap();
        self.0.wait().unwrap();
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(home: &Path) -> Value {
    let output = command("start", home).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find_map(|l| l.split_once("OVERVIEW ").map(|(_, json)| json))
        .expect("Übersicht");
    serde_json::from_str(line).unwrap()
}

fn kinds(overview: &Value) -> Vec<String> {
    overview["reports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].as_str().unwrap().to_string())
        .collect()
}

/// Genau eine lesbare Marke, und zwar die des eigenen Prozesses.
fn assert_own_marker(home: &Path) {
    let found = os_app::crash::session::markers(&home.join("logs/crashes"));
    assert_eq!(found.len(), 1, "{found:?}");
    let marker = found[0].1.as_ref().expect("lesbare Marke");
    assert_eq!(marker.pid, std::process::id());
}

fn markers(home: &Path) -> usize {
    std::fs::read_dir(home.join("logs/crashes"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("session-"))
                .count()
        })
        .unwrap_or(0)
}

// spec: OUT-246
#[test]
fn hard_kill_then_restart_yields_exactly_one_unclean_exit_with_the_log_of_that_run() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path());
    let pid = running.0.id();
    assert_eq!(markers(home.path()), 1);
    running.kill();
    let first = start(home.path());
    assert_eq!(kinds(&first), ["unclean_exit"]);
    assert_eq!(
        first["pending"].as_array().unwrap().len(),
        1,
        "Hinweis beim nächsten Start"
    );
    let file = std::fs::read_dir(home.path().join("logs/crashes"))
        .unwrap()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("crash-"))
        .unwrap();
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(file.path()).unwrap()).unwrap();
    let tail: Vec<&str> = report["log_tail"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert!(
        tail.iter()
            .any(|l| l.contains(&format!("child-run-marker pid={pid}"))),
        "{tail:?}"
    );
    assert!(tail
        .iter()
        .all(|l| !l.contains("child-run-marker pid=") || l.contains(&format!("pid={pid}"))));
    assert_eq!(report["process"], "app");
    // Zweiter Neustart: kein weiterer Bericht; geordnetes Beenden hat keine Marke hinterlassen.
    let second = start(home.path());
    assert_eq!(kinds(&second), ["unclean_exit"]);
    assert_eq!(markers(home.path()), 0);
}

// spec: OUT-246
#[test]
fn orderly_exit_produces_no_report() {
    let home = tempfile::tempdir().unwrap();
    assert!(kinds(&start(home.path())).is_empty());
    assert!(kinds(&start(home.path())).is_empty());
    assert_eq!(markers(home.path()), 0);
}

fn exit_like_update(mode: &str, home: &Path) {
    let output = command(mode, home).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// spec: OUT-246
#[test]
fn update_exit_through_the_installer_hook_produces_no_report() {
    let home = tempfile::tempdir().unwrap();
    exit_like_update("update-exit", home.path());
    assert_eq!(markers(home.path()), 0);
    assert!(kinds(&start(home.path())).is_empty());
    assert_eq!(markers(home.path()), 0);
}

// spec: OUT-246
#[test]
fn process_exit_without_the_hook_still_counts_as_unclean_exit() {
    // Gegenprobe: ohne Hook bleibt die Marke liegen, der Folgestart meldet genau einen `unclean_exit`.
    let home = tempfile::tempdir().unwrap();
    exit_like_update("hard-exit", home.path());
    assert_eq!(markers(home.path()), 1);
    assert_eq!(kinds(&start(home.path())), ["unclean_exit"]);
}

// spec: OUT-246
#[test]
fn resumed_marker_after_failed_installer_start_still_reports_a_later_hard_exit() {
    let home = tempfile::tempdir().unwrap();
    exit_like_update("update-resume-hard-exit", home.path());
    assert_eq!(markers(home.path()), 1);
    assert_eq!(kinds(&start(home.path())), ["unclean_exit"]);
    assert_eq!(markers(home.path()), 0);
}

// spec: OUT-246
#[test]
fn resuming_without_update_exit_or_twice_is_harmless() {
    let home = tempfile::tempdir().unwrap();
    exit_like_update("resume-noop", home.path());
    assert_eq!(markers(home.path()), 0);
    assert!(kinds(&start(home.path())).is_empty());
}

// spec: OUT-246
#[test]
fn two_running_app_processes_do_not_report_each_other() {
    let home = tempfile::tempdir().unwrap();
    let a = Running::start(home.path());
    // Zweiter Prozess startet, während A läuft: A ist nicht abgestürzt.
    assert!(kinds(&start(home.path())).is_empty());
    assert_eq!(markers(home.path()), 1, "Marke von A bleibt");
    a.kill();
    assert_eq!(kinds(&start(home.path())), ["unclean_exit"]);
}

// spec: OUT-246
#[test]
fn marker_with_reused_pid_but_other_start_time_counts_as_crash() {
    let home = tempfile::tempdir().unwrap();
    // Die PID dieses (laufenden) Testprozesses mit falscher Startzeit.
    write_marker(
        &home.path().join("logs/crashes"),
        &Marker {
            pid: std::process::id(),
            start_time: 1,
            version: "0.0.1".into(),
            build: String::new(),
            log_offset: 0,
            session: String::new(),
            crawl_running: true,
            rendering_active: true,
            panic_report: false,
        },
    )
    .unwrap();
    let overview = start(home.path());
    assert_eq!(kinds(&overview), ["unclean_exit"]);
    let id = overview["reports"][0]["id"].as_str().unwrap().to_string();
    let path = std::fs::read_dir(home.path().join("logs/crashes"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| std::fs::read_to_string(p).is_ok_and(|t| t.contains(&id)))
        .unwrap();
    let report: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(report["app_version"], "0.0.1");
    assert_eq!(report["crawl_running"], true);
    assert_eq!(report["rendering_active"], true);
}

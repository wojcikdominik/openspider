//! Unabhängige Tests aus der Spec (OUT-246, `unclean_exit`): Ein geordnetes Ende der App – auch das Ende durch ein
//! App-Update – darf beim nächsten Start keinen `unclean_exit`-Bericht erzeugen; ein Prozess, der ohne geordnetes Ende
//! verschwindet, liefert weiterhin genau einen.
//!
//! Der Update-Ausgang unter Windows wird nachgestellt wie im Updater-Plugin: Der von der App gesetzte Hook
//! (`on_before_exit`, hier `os_app::crash::on_orderly_exit`) läuft auf einem Hintergrund-Thread, danach endet der
//! Prozess mit `std::process::exit(0)` – ohne `RunEvent::Exit` und ohne Destruktoren. Der macOS-Weg (Marke entfernen,
//! dann neu starten, danach evtl. noch `RunEvent::Exit`) wird als zweifacher Aufruf mit normalem Prozessende
//! nachgestellt. Jeder App-Lauf ist ein eigener Kindprozess dieser Testdatei.
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;

const MODE: &str = "OPENSPIDER_UPDEXIT_TEST_CHILD";
const HOME: &str = "OPENSPIDER_UPDEXIT_TEST_HOME";

fn command(mode: &str, home: &Path) -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "updexit_child",
        "--nocapture",
        "--test-threads=1",
    ])
    .env(MODE, mode)
    .env(HOME, home)
    .env("OPENSPIDER_SECRET_STORE", "memory")
    .env_remove("OPENSPIDER_CRASH_TEST");
    cmd
}

fn crashes(home: &Path) -> PathBuf {
    home.join("logs").join("crashes")
}

/// Hook-Signatur wie beim Updater-Plugin (`Fn() + Send + Sync + 'static`).
fn update_hook() -> Arc<dyn Fn() + Send + Sync + 'static> {
    Arc::new(os_app::crash::on_orderly_exit)
}

/// Hook auf einem anderen Thread ausführen (das Plugin installiert in einer blockierenden Hintergrundaufgabe).
fn run_hook_on_worker(hook: &Arc<dyn Fn() + Send + Sync + 'static>) {
    let hook = hook.clone();
    std::thread::Builder::new()
        .name("update-install".into())
        .spawn(move || hook())
        .unwrap()
        .join()
        .unwrap();
}

/// Kindprozess-Einstieg; ohne gesetzte Umgebungsvariable ein leerer Test.
#[test]
fn updexit_child() {
    let Ok(mode) = std::env::var(MODE) else {
        return;
    };
    let home = PathBuf::from(std::env::var_os(HOME).unwrap());
    if mode == "hook-without-session" {
        // Hook ohne App und ohne Sitzungsmarke: darf nicht scheitern.
        let hook = update_hook();
        run_hook_on_worker(&hook);
        hook();
        std::process::exit(0);
    }
    os_app::crash::enable_session_tracking();
    let app = os_app::App::new(os_app::AppConfig::new(&home)).unwrap();
    let own = crashes(&home).join(format!("session-{}.json", std::process::id()));
    match mode.as_str() {
        "run" => {
            println!("READY");
            std::thread::sleep(std::time::Duration::from_secs(120));
        }
        "start" => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let overview = rt
                .block_on(app.dispatch(
                    os_app::CallCtx::for_window("main"),
                    "crash.overview",
                    Value::Null,
                ))
                .unwrap();
            println!("OVERVIEW {overview}");
            os_app::crash::on_orderly_exit();
        }
        // Windows-Update: Hook auf dem Installationsthread, danach hartes `exit(0)`.
        "update-exit" => {
            assert!(own.exists(), "eigene Marke vor dem Update-Ausgang");
            run_hook_on_worker(&update_hook());
            assert!(!own.exists(), "Hook hat die eigene Marke nicht entfernt");
            std::process::exit(0);
        }
        // Hook mehrfach (Thread und Hauptthread), danach hartes `exit(0)`.
        "update-exit-twice" => {
            assert!(own.exists());
            let hook = update_hook();
            run_hook_on_worker(&hook);
            hook();
            run_hook_on_worker(&hook);
            std::process::exit(0);
        }
        // macOS-Weg: Marke im Update-Pfad entfernen, danach geordnetes Exit-Ereignis mit demselben Aufruf und
        // normalem Prozessende (Destruktoren laufen).
        "update-then-exit-event" => {
            assert!(own.exists());
            run_hook_on_worker(&update_hook());
            os_app::crash::on_orderly_exit();
            drop(app);
        }
        // Gegenprobe: hartes `exit(0)` ohne geordnetes Ende.
        "hard-exit" => {
            assert!(own.exists());
            std::process::exit(0);
        }
        other => panic!("unbekannter Modus {other}"),
    }
}

/// Kindprozess bis zum Ende laufen lassen; liefert PID und Ausgabe, verlangt Exit-Code 0.
fn run_child(mode: &str, home: &Path) -> (u32, Output) {
    let child = command(mode, home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Kind {mode} scheiterte: {:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (pid, output)
}

/// Folgestart der App: Übersicht der Berichte (der Start beendet sich geordnet).
fn start(home: &Path) -> Value {
    let (_, output) = run_child("start", home);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find_map(|l| l.split_once("OVERVIEW ").map(|(_, json)| json))
        .expect("Übersicht");
    serde_json::from_str(line).unwrap()
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
                Some(Ok(line)) if line.trim_end().ends_with("READY") => break,
                Some(Ok(_)) => continue,
                _ => panic!("Kindprozess nicht bereit"),
            }
        }
        Running(child)
    }
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

fn markers(home: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(crashes(home))
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("session-") && n.ends_with(".json"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn marker_name(pid: u32) -> String {
    format!("session-{pid}.json")
}

/// Alle Berichtsdateien (`crash-*.json`) mit Pfad und Inhalt.
fn report_files(home: &Path) -> Vec<(PathBuf, Value)> {
    let mut out: Vec<(PathBuf, Value)> = std::fs::read_dir(crashes(home))
        .map(|d| {
            d.flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    n.starts_with("crash-") && n.ends_with(".json")
                })
                .map(|e| {
                    let text = std::fs::read_to_string(e.path()).unwrap();
                    (e.path(), serde_json::from_str(&text).unwrap())
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn kinds(overview: &Value) -> Vec<String> {
    overview["reports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].as_str().unwrap().to_string())
        .collect()
}

fn ids(home: &Path) -> BTreeSet<String> {
    report_files(home)
        .into_iter()
        .map(|(_, r)| r["id"].as_str().unwrap().to_string())
        .collect()
}

/// Ändert in der liegengebliebenen Marke nur die Version (wie ein Lauf einer älteren App-Version).
fn set_marker_version(home: &Path, pid: u32, version: &str) -> String {
    let path = crashes(home).join(marker_name(pid));
    let mut marker: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let before = marker["version"].as_str().unwrap().to_string();
    marker["version"] = Value::String(version.into());
    std::fs::write(&path, serde_json::to_vec(&marker).unwrap()).unwrap();
    before
}

// spec: OUT-246
#[test]
fn update_exit_via_hook_then_process_exit_leaves_no_marker_and_no_report() {
    let home = tempfile::tempdir().unwrap();
    let (pid, _) = run_child("update-exit", home.path());
    assert!(
        !markers(home.path()).contains(&marker_name(pid)),
        "Marke des per Update beendeten Laufs liegt noch: {:?}",
        markers(home.path())
    );
    let overview = start(home.path());
    assert!(kinds(&overview).is_empty(), "{overview}");
    assert!(
        overview["pending"].as_array().unwrap().is_empty(),
        "kein Hinweis nach Update: {overview}"
    );
    assert!(report_files(home.path()).is_empty());
    assert!(
        markers(home.path()).is_empty(),
        "{:?}",
        markers(home.path())
    );
    // Auch ein weiterer Start findet nichts.
    assert!(kinds(&start(home.path())).is_empty());
    assert!(markers(home.path()).is_empty());
}

// spec: OUT-246
#[test]
fn update_hook_called_several_times_before_process_exit_is_harmless() {
    let home = tempfile::tempdir().unwrap();
    run_child("update-exit-twice", home.path());
    assert!(markers(home.path()).is_empty());
    assert!(kinds(&start(home.path())).is_empty());
    assert!(report_files(home.path()).is_empty());
    assert!(markers(home.path()).is_empty());
}

// spec: OUT-246
#[test]
fn update_path_then_regular_exit_event_is_harmless() {
    let home = tempfile::tempdir().unwrap();
    run_child("update-then-exit-event", home.path());
    assert!(markers(home.path()).is_empty());
    assert!(kinds(&start(home.path())).is_empty());
    assert!(report_files(home.path()).is_empty());
}

// spec: OUT-246
#[test]
fn update_hook_without_session_is_harmless() {
    let home = tempfile::tempdir().unwrap();
    run_child("hook-without-session", home.path());
    assert!(markers(home.path()).is_empty());
    assert!(kinds(&start(home.path())).is_empty());
}

// spec: OUT-246
#[test]
fn process_exit_without_orderly_end_yields_exactly_one_unclean_exit_with_marker_version() {
    let home = tempfile::tempdir().unwrap();
    let (pid, _) = run_child("hard-exit", home.path());
    assert_eq!(markers(home.path()), [marker_name(pid)]);
    // Folgestart als neue Version: der Bericht trägt die Version des beendeten Laufs aus dessen Marke.
    let original = set_marker_version(home.path(), pid, "0.9.9-vor-update");
    assert!(!original.is_empty(), "Marke ohne Version");
    let overview = start(home.path());
    assert_eq!(kinds(&overview), ["unclean_exit"], "{overview}");
    assert_eq!(overview["pending"].as_array().unwrap().len(), 1);
    let files = report_files(home.path());
    assert_eq!(files.len(), 1);
    let report = &files[0].1;
    assert_eq!(report["kind"], "unclean_exit");
    assert_eq!(report["app_version"], "0.9.9-vor-update");
    assert_eq!(report["process"], "app");
    assert!(
        markers(home.path()).is_empty(),
        "{:?}",
        markers(home.path())
    );
    // Zweiter Neustart: kein weiterer Bericht.
    assert_eq!(kinds(&start(home.path())), ["unclean_exit"]);
    assert_eq!(report_files(home.path()).len(), 1);
}

// spec: OUT-246
#[test]
fn process_exit_without_orderly_end_keeps_the_unchanged_marker_version() {
    let home = tempfile::tempdir().unwrap();
    let (pid, _) = run_child("hard-exit", home.path());
    let path = crashes(home.path()).join(marker_name(pid));
    let marker: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(marker["pid"], pid);
    let version = marker["version"].as_str().unwrap().to_string();
    assert_eq!(kinds(&start(home.path())), ["unclean_exit"]);
    assert_eq!(report_files(home.path())[0].1["app_version"], version);
}

// spec: OUT-246
#[test]
fn update_exit_then_hard_exit_reports_only_the_second_session() {
    let home = tempfile::tempdir().unwrap();
    let (a, _) = run_child("update-exit", home.path());
    assert!(markers(home.path()).is_empty());
    let (b, _) = run_child("hard-exit", home.path());
    assert_ne!(a, b);
    assert_eq!(markers(home.path()), [marker_name(b)]);
    set_marker_version(home.path(), b, "sitzung-b");
    let overview = start(home.path());
    assert_eq!(kinds(&overview), ["unclean_exit"], "{overview}");
    let files = report_files(home.path());
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].1["app_version"], "sitzung-b");
    assert!(markers(home.path()).is_empty());
}

// spec: OUT-246
#[test]
fn update_exit_removes_only_its_own_marker_not_that_of_a_running_instance() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path());
    let live = running.0.id();
    assert_eq!(markers(home.path()), [marker_name(live)]);
    let (updated, _) = run_child("update-exit", home.path());
    assert_eq!(
        markers(home.path()),
        [marker_name(live)],
        "Update-Ausgang von {updated} darf nur die eigene Marke entfernen"
    );
    // Start, während die andere Instanz läuft: kein Bericht, ihre Marke bleibt.
    assert!(kinds(&start(home.path())).is_empty());
    assert_eq!(markers(home.path()), [marker_name(live)]);
    running.kill();
    assert_eq!(kinds(&start(home.path())), ["unclean_exit"]);
    assert!(markers(home.path()).is_empty());
}

/// Legt `n` gültige Berichte an (Vorlage aus einem echten `unclean_exit`), 1 bis `n` Tage alt; Kennungen vom
/// neuesten zum ältesten.
fn seed_reports(home: &Path, n: u32) -> Vec<String> {
    run_child("hard-exit", home);
    assert_eq!(kinds(&start(home)), ["unclean_exit"]);
    let files = report_files(home);
    assert_eq!(files.len(), 1);
    let (template_path, template) = files.into_iter().next().unwrap();
    std::fs::remove_file(template_path).unwrap();
    let now = time::OffsetDateTime::now_utc();
    let mut seeded = Vec::new();
    for k in 1..=n {
        let t = now - time::Duration::days(i64::from(k));
        let stamp = format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            t.year(),
            u8::from(t.month()),
            t.day(),
            t.hour(),
            t.minute(),
            t.second()
        );
        let id = uuid::Uuid::new_v4().to_string();
        let mut report = template.clone();
        report["id"] = Value::String(id.clone());
        std::fs::write(
            crashes(home).join(format!("crash-{stamp}-unclean_exit.json")),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        seeded.push(id);
    }
    seeded
}

// spec: OUT-246
#[test]
fn update_exit_with_full_report_folder_adds_nothing_and_hard_exit_report_survives_retention() {
    let home = tempfile::tempdir().unwrap();
    let order = seed_reports(home.path(), 20);
    let seeded: BTreeSet<String> = order.iter().cloned().collect();
    assert_eq!(ids(home.path()), seeded);
    // Update-Ausgang bei vollem Ordner: kein neuer Bericht, nichts verdrängt.
    run_child("update-exit", home.path());
    let overview = start(home.path());
    assert_eq!(kinds(&overview).len(), 20, "{overview}");
    assert_eq!(
        ids(home.path()),
        seeded,
        "Update-Ausgang hat Berichte verändert"
    );
    assert!(markers(home.path()).is_empty());
    // Harter Ausgang bei vollem Ordner: der neue Bericht bleibt, der älteste geht (höchstens 20).
    run_child("hard-exit", home.path());
    start(home.path());
    let now_ids = ids(home.path());
    assert_eq!(now_ids.len(), 20, "{now_ids:?}");
    let new: Vec<&String> = now_ids.difference(&seeded).collect();
    assert_eq!(new.len(), 1, "genau ein neuer Bericht: {new:?}");
    let gone: Vec<&String> = seeded.difference(&now_ids).collect();
    assert_eq!(
        gone,
        [order.last().unwrap()],
        "der älteste Bericht wird verdrängt"
    );
    let newest = report_files(home.path())
        .into_iter()
        .find(|(_, r)| r["id"].as_str() == Some(new[0].as_str()))
        .unwrap();
    assert_eq!(newest.1["kind"], "unclean_exit");
    assert!(markers(home.path()).is_empty());
}

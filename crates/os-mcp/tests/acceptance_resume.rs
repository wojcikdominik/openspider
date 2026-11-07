//! Unabhängige Abnahmetests B4-RESUME (MCP): Crawls per MCP anhalten, fortsetzen und stoppen.
//!
//! Erwartetes Verhalten stammt ausschließlich aus der Spezifikation (AGENT-020, AGENT-001, AGENT-003, CRAWL-125,
//! CRAWL-126, CRAWL-128, UI-003, OUT-233); der Code wurde nur gelesen, um zu wissen, wie die Werkzeuge aufgerufen
//! werden. Geprüft wird von außen: echter MCP-Prozess über STDIO (JSON-RPC, eine Nachricht je Zeile), Fixtures
//! von `os-testsite` (synthetische Site mit Antwortverzögerung, damit Pause und Stopp garantiert vor dem natürlichen
//! Crawl-Ende greifen) und die stabile Leseschnittstelle der Crawl-Datei (`q_manifest`, `q_urls`, `crawl_meta`).
//!
//! Kriterienliste: `docs/plan/M3-reports/b4-resume-tests.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use os_store::{CrawlFile, OpenMode};
use os_testsite::{SiteOptions, TestSite};
use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_openspider-mcp");
/// Site mit 40 Seiten, jede Antwort 250 ms verzögert: mit einem Thread mindestens 10 s bis zum natürlichen Ende.
const SLOW: &str = "synthetic:n=40,links=4,seed=7,delay_ms=250";
/// Wie [`SLOW`], aber 1 s je Antwort: Mit der Standard-Threadzahl (parallele Anfragen) endet der Crawl nicht, bevor mehrfach
/// pausiert werden konnte (CRAWL-126: Tests, die „Pausiert“ erwarten, nutzen eine Fixture, die vor der Pause nicht enden kann).
const SLOW_PARALLEL: &str = "synthetic:n=40,links=4,seed=7,delay_ms=1000";
/// Dieselbe Site ohne Verzögerung (Referenz für den ununterbrochenen Lauf).
const FAST: &str = "synthetic:n=40,links=4,seed=7";
const WAIT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------------------------------------
// Hilfen: Testsite, MCP-Client, Crawl-Datei
// ---------------------------------------------------------------------------------------------------------

async fn site(spec: &str) -> TestSite {
    TestSite::start_spec(
        spec,
        SiteOptions {
            port: 0,
            log_requests: true,
        },
    )
    .await
    .expect("Testsite startbar")
}

/// Alle Seitenabrufe (GET) der Testsite ohne robots.txt und Hilfspfade, in Eingangsreihenfolge.
fn page_requests(site: &TestSite) -> Vec<String> {
    site.requests()
        .into_iter()
        .filter(|r| r.method == "GET")
        .map(|r| r.path_and_query)
        .filter(|p| p != "/robots.txt" && !p.starts_with("/__testsite") && !p.contains("sitemap"))
        .collect()
}

fn page_request_count(site: &TestSite) -> usize {
    page_requests(site).len()
}

/// Pfade, die mehr als einmal abgerufen wurden.
fn duplicated(site: &TestSite) -> BTreeSet<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for p in page_requests(site) {
        *counts.entry(p).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(p, _)| p)
        .collect()
}

/// CRAWL-125: nach SIGKILL sind „alle bis ≤ 2 s vor dem Abbruch gecrawlten URLs“ gespeichert. Diese Menge wird hier
/// aus dem Request-Log der Testsite abgeleitet statt aus einer festen Zahl: Der Schnappschuss enthält alle Seiten, die
/// bis jetzt angefragt wurden; jede davon ist spätestens nach der Antwortverzögerung der Fixture (250 ms) abgerufen.
/// Wer den Schnappschuss nimmt, beendet den Prozess frühestens [`KILL_AFTER_SNAPSHOT`] später; bis dahin sind die Seiten
/// des Schnappschusses mehr als 2 s vor dem Abbruch fertig (Reserve für Last auf dem Testrechner: rund 250 ms Verzögerung
/// plus 250 ms Sicherheitsabstand).
const KILL_AFTER_SNAPSHOT: Duration = Duration::from_millis(2500);

fn snapshot_pages_stored_by_kill_time(site: &TestSite) -> BTreeSet<String> {
    page_requests(site).into_iter().collect()
}

fn wait_until(what: &str, limit: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < limit, "Zeitüberschreitung: {what}");
        std::thread::sleep(Duration::from_millis(40));
    }
}

/// Eigener Heimat- und Crawl-Ordner je Test; `mcp.toml` erlaubt private Ziele (die Fixture läuft auf 127.0.0.1).
struct Env {
    home: tempfile::TempDir,
    crawls: PathBuf,
}

impl Env {
    fn new() -> Env {
        Env::with_config("allow_private_targets = true\n")
    }

    fn with_config(toml: &str) -> Env {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("mcp.toml"), toml).unwrap();
        let crawls = home.path().join("crawls");
        std::fs::create_dir_all(&crawls).unwrap();
        Env { home, crawls }
    }

    fn spawn(&self, args: &[&str]) -> Mcp {
        let mut cmd = Command::new(BIN);
        cmd.env("OPENSPIDER_HOME", self.home.path())
            .env("OPENSPIDER_SECRET_STORE", "memory")
            .env_remove("OPENSPIDER_LOG")
            .env_remove("RUST_LOG")
            .arg("--crawl-dir")
            .arg(&self.crawls)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().expect("openspider-mcp startbar");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut mcp = Mcp {
            child,
            stdin: Some(stdin),
            rx,
            next_id: 1,
        };
        let init = mcp.request(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "abnahme-resume", "version": "0"}}),
        );
        assert!(init.get("result").is_some(), "initialize: {init}");
        mcp.notify("notifications/initialized");
        mcp
    }

    fn crawl_file(&self, id: &str) -> PathBuf {
        self.crawls.join(format!("{id}.ospider"))
    }
}

struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    next_id: u64,
}

struct Reply {
    is_error: bool,
    text: String,
}

impl Reply {
    /// Wiederholbarer Fehler „busy“ (der Crawl schließt gerade ab); die Spezifikation legt dazu nichts fest.
    fn is_busy(&self) -> bool {
        self.is_error && self.text.starts_with("busy:")
    }

    /// `summary` des Erfolgsergebnisses.
    fn summary(&self) -> Value {
        assert!(!self.is_error, "Fehlerergebnis: {}", self.text);
        let v: Value = serde_json::from_str(&self.text)
            .unwrap_or_else(|e| panic!("kein JSON ({e}): {}", self.text));
        v["summary"].clone()
    }
}

impl Mcp {
    fn send(&mut self, v: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin offen");
        writeln!(stdin, "{v}").unwrap();
        stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({"jsonrpc": "2.0", "method": method}));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self
                .rx
                .recv_timeout(WAIT)
                .unwrap_or_else(|_| panic!("keine Antwort auf {method}"));
            let v: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("stdout-Zeile ist kein JSON ({e}): {line}"));
            if v["id"] == json!(id) {
                return v;
            }
        }
    }

    fn call(&mut self, tool: &str, args: Value) -> Reply {
        let v = self.request("tools/call", json!({"name": tool, "arguments": args}));
        let result = v
            .get("result")
            .unwrap_or_else(|| panic!("JSON-RPC-Fehler statt Werkzeugergebnis: {v}"));
        let text = result["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Reply {
            is_error: result["isError"] == json!(true),
            text,
        }
    }

    /// Wie [`Mcp::call`], wiederholt aber, solange der Server „busy“ meldet (Abschluss des Laufs noch nicht fertig).
    fn call_settled(&mut self, tool: &str, args: Value) -> Reply {
        let start = Instant::now();
        loop {
            let r = self.call(tool, args.clone());
            if !r.is_busy() || start.elapsed() > Duration::from_secs(30) {
                return r;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn tool_names(&mut self) -> BTreeSet<String> {
        let v = self.request("tools/list", json!({}));
        v["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("tools/list: {v}"))
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Wie `kill -9`.
    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Schließt stdin (normales Ende des Clients) und wartet auf das Ende des Servers.
    fn close(mut self) {
        drop(self.stdin.take());
        let start = Instant::now();
        while self.child.try_wait().unwrap().is_none() {
            if start.elapsed() > Duration::from_secs(20) {
                let _ = self.child.kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.wait();
    }

    fn state(&mut self, id: &str) -> String {
        let r = self.call("crawl_status", json!({"crawl_id": id}));
        r.summary()["state"].as_str().unwrap().to_owned()
    }

    fn wait_state(&mut self, id: &str, want: &str, limit: Duration) {
        let start = Instant::now();
        loop {
            let s = self.state(id);
            if s == want {
                return;
            }
            assert!(
                start.elapsed() < limit,
                "Crawl {id}: Zustand {want} nicht erreicht (zuletzt {s})"
            );
            std::thread::sleep(Duration::from_millis(60));
        }
    }

    fn start(&mut self, site: &TestSite, overrides: Value) -> String {
        let r = self.call(
            "start_crawl",
            json!({"url": site.url("/"), "overrides": overrides}),
        );
        r.summary()["crawl_id"].as_str().unwrap().to_owned()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn open(path: &Path) -> CrawlFile {
    CrawlFile::open(path, OpenMode::ReadOnly).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `q_manifest` (OUT-233) als Schlüssel/Wert-Tabelle.
fn manifest(path: &Path) -> BTreeMap<String, String> {
    let file = open(path);
    let mut stmt = file
        .conn()
        .prepare("SELECT key, value FROM q_manifest")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })
        .unwrap();
    rows.map(Result::unwrap).collect()
}

/// Vollständige Crawl-Metadaten (`crawl_meta`).
fn meta_dump(path: &Path) -> BTreeMap<String, String> {
    let file = open(path);
    let mut stmt = file
        .conn()
        .prepare("SELECT key, value FROM crawl_meta")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })
        .unwrap();
    rows.map(Result::unwrap).collect()
}

/// Gecrawlte interne URLs als (Pfad mit Query, Statuscode, Tiefe); Host und Port sind normalisiert.
fn crawled_set(path: &Path) -> BTreeSet<(String, i64, i64)> {
    let file = open(path);
    let mut stmt = file
        .conn()
        .prepare(
            "SELECT address, status_code, crawl_depth FROM q_urls WHERE crawled = 1 AND is_internal = 1",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?.unwrap_or(-1),
                r.get::<_, Option<i64>>(2)?.unwrap_or(-1),
            ))
        })
        .unwrap();
    rows.map(Result::unwrap)
        .map(|(a, s, d)| (path_of(&a), s, d))
        .filter(|(p, _, _)| p != "/robots.txt")
        .collect()
}

fn path_of(address: &str) -> String {
    let rest = address.split_once("://").map_or(address, |(_, r)| r);
    match rest.find('/') {
        Some(i) => rest[i..].to_owned(),
        None => "/".to_owned(),
    }
}

fn paths(set: &BTreeSet<(String, i64, i64)>) -> BTreeSet<String> {
    set.iter().map(|(p, _, _)| p.clone()).collect()
}

type Snapshot = (
    BTreeMap<String, String>,
    BTreeMap<String, String>,
    BTreeSet<(String, i64, i64)>,
);

/// Gesamter beobachtbarer Stand einer Crawl-Datei (zum Nachweis „Datei unverändert“).
///
/// Ein beendeter Crawl bekommt nach seinem Ende noch die automatische Crawl-Analyse (UI-003, CRAWL-141); der
/// Stand wird erst genommen, wenn sie durch ist, damit nur Änderungen durch den geprüften Aufruf auffallen.
fn snapshot(path: &Path) -> Snapshot {
    let start = Instant::now();
    loop {
        let m = manifest(path);
        let busy = m["state"] == "Completed"
            && matches!(m["analysis_state"].as_str(), "pending" | "running")
            && start.elapsed() < Duration::from_secs(30);
        if !busy {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    (manifest(path), meta_dump(path), crawled_set(path))
}

/// Ununterbrochener Lauf der schnellen Site über einen eigenen MCP-Server; liefert die gecrawlte Menge.
async fn reference_set() -> BTreeSet<(String, i64, i64)> {
    let site = site(FAST).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, json!({"speed.threads": 1}));
    mcp.wait_state(&id, "Completed", WAIT);
    let set = crawled_set(&env.crawl_file(&id));
    let m = manifest(&env.crawl_file(&id));
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    assert!(set.len() >= 30, "Referenzlauf zu klein: {}", set.len());
    set
}

fn crawl_file_count(env: &Env) -> usize {
    std::fs::read_dir(&env.crawls)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "ospider"))
        .count()
}

/// UI-003: nach „Stoppen“ startet eine eingestellte automatische Crawl-Analyse wie nach einem regulären Ende.
fn assert_analysis_runs_after_stop(file: &Path) {
    wait_until(
        "Crawl-Analyse nach dem Stopp",
        Duration::from_secs(30),
        || manifest(file)["analysis_state"] == "done",
    );
}

fn uint(m: &BTreeMap<String, String>, key: &str) -> u64 {
    m[key]
        .parse()
        .unwrap_or_else(|_| panic!("{key} = {:?}", m[key]))
}

fn threads1() -> Value {
    json!({"speed.threads": 1})
}

/// Fehlermeldung nennt den endgültigen Zustand (Hinweis laut CRAWL-128).
const STOPPED_HINT: &str = "gestoppt – endgültig beendet";

// ---------------------------------------------------------------------------------------------------------
// Werkzeug-Freigaben (AGENT-001, AGENT-003)
// ---------------------------------------------------------------------------------------------------------

const CRAWL_TOOLS: [&str; 5] = [
    "start_crawl",
    "crawl_status",
    "pause_crawl",
    "stop_crawl",
    "resume_crawl",
];

fn assert_no_crawl_tools(names: &BTreeSet<String>, label: &str) {
    for t in CRAWL_TOOLS {
        assert!(!names.contains(t), "{label}: {t} darf nicht gelistet sein");
    }
}

// spec: AGENT-001
// spec: AGENT-003
// spec: AGENT-020
// acceptance: independent
#[test]
fn crawl_tools_are_listed_only_with_the_crawl_class() {
    let env = Env::with_config("");
    // Auslieferungszustand: nur `read`.
    let mut mcp = env.spawn(&[]);
    let default = mcp.tool_names();
    assert_no_crawl_tools(&default, "ohne Schalter");
    assert!(default.contains("list_crawls"), "{default:?}");
    mcp.close();
    // `--read-only` ebenso.
    let mut mcp = env.spawn(&["--read-only"]);
    assert_no_crawl_tools(&mcp.tool_names(), "--read-only");
    mcp.close();
    // `--allow crawl`: die fünf Crawl-Werkzeuge, aber nichts aus `write`.
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let names = mcp.tool_names();
    for t in CRAWL_TOOLS {
        assert!(names.contains(t), "--allow crawl: {t} fehlt in {names:?}");
    }
    assert!(
        !names.contains("export"),
        "write-Werkzeug sichtbar: {names:?}"
    );
    mcp.close();
    // `--allow read` ändert nichts.
    let mut mcp = env.spawn(&["--allow", "read"]);
    assert_no_crawl_tools(&mcp.tool_names(), "--allow read");
}

// spec: AGENT-001
// spec: AGENT-003
// acceptance: independent
#[test]
fn app_configuration_enables_crawl_tools_and_read_only_overrides_it() {
    // AGENT-001: gespeicherte Klassen-Freigabe der App gilt ohne Schalter; `--read-only` überschreibt sie.
    let env = Env::with_config("classes = [\"read\", \"crawl\"]\n");
    let mut mcp = env.spawn(&[]);
    let names = mcp.tool_names();
    for t in CRAWL_TOOLS {
        assert!(names.contains(t), "App-Freigabe: {t} fehlt");
    }
    assert!(!names.contains("export"), "{names:?}");
    mcp.close();
    let mut mcp = env.spawn(&["--read-only"]);
    assert_no_crawl_tools(&mcp.tool_names(), "--read-only trotz App-Freigabe");
}

// spec: AGENT-003
// spec: AGENT-020
// acceptance: independent
#[test]
fn calling_a_crawl_tool_without_approval_is_an_mcp_error_with_a_hint() {
    let env = Env::with_config("");
    for flags in [&[][..], &["--read-only"][..]] {
        let mut mcp = env.spawn(flags);
        for tool in CRAWL_TOOLS {
            let args = if tool == "start_crawl" {
                json!({"url": "http://127.0.0.1:9/"})
            } else {
                json!({"crawl_id": "0199aaaa-0000-7000-8000-000000000001"})
            };
            let r = mcp.call(tool, args);
            assert!(r.is_error, "{tool} {flags:?}: kein MCP-Fehler: {}", r.text);
            let lower = r.text.to_lowercase();
            assert!(
                lower.contains("allow") && lower.contains("crawl"),
                "{tool} {flags:?}: Hinweis auf die Freigabe fehlt: {}",
                r.text
            );
        }
        mcp.close();
    }
}

// ---------------------------------------------------------------------------------------------------------
// Pause (AGENT-020 pause_crawl, CRAWL-126)
// ---------------------------------------------------------------------------------------------------------

// spec: AGENT-020
// spec: CRAWL-126
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_crawl_halts_new_requests_and_the_crawl_stays_paused() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("6 Seitenabrufe", WAIT, || page_request_count(&site) >= 6);

    let before = page_request_count(&site);
    let r = mcp.call("pause_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);
    // CRAWL-126: innerhalb von 1 s startet keine neue Anfrage mehr (höchstens Threads-viele laufende).
    std::thread::sleep(Duration::from_millis(1100));
    assert!(
        page_request_count(&site) <= before + 1,
        "nach der Pause starten weitere Anfragen: {} -> {}",
        before,
        page_request_count(&site)
    );
    mcp.wait_state(&id, "Paused", Duration::from_secs(20));
    let at_pause = page_request_count(&site);
    // „Pausiert“ bleibt stabil, es kommen keine weiteren Anfragen und kein natürliches Ende.
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(page_request_count(&site), at_pause);
    assert_eq!(mcp.state(&id), "Paused");
    let m = manifest(&env.crawl_file(&id));
    assert_eq!(m["state"], "Paused", "{m:?}");
    assert_eq!(m["completeness"], "paused", "{m:?}");
    // Die gesicherten URLs sind alle abgerufenen (laufende Anfragen wurden zu Ende geführt und gespeichert).
    let stored = crawled_set(&env.crawl_file(&id)).len();
    assert_eq!(
        stored, at_pause,
        "gespeichert {stored}, abgerufen {at_pause}"
    );
}

// spec: AGENT-020
// spec: CRAWL-126
// spec: CRAWL-130
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crawl_status_reports_state_progress_and_errors_while_running() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("3 Seitenabrufe", WAIT, || page_request_count(&site) >= 3);
    let r = mcp.call("crawl_status", json!({"crawl_id": id}));
    let s = r.summary();
    assert_eq!(s["state"], "Running", "{s}");
    assert!(s["errors"].is_number(), "Fehlerzähler fehlt: {s}");
    assert!(s["progress"].is_object(), "Fortschritt fehlt: {s}");
    // AGENT-020/Fremdprozess-Kriterium: die Engine schreibt Besitzer-Metadaten.
    let meta = meta_dump(&env.crawl_file(&id));
    assert_eq!(meta["owner_kind"], "mcp-stdio", "{meta:?}");
    assert_eq!(meta["owner_pid"], mcp.pid().to_string(), "{meta:?}");
    mcp.kill();
}

// spec: AGENT-020
// spec: CRAWL-126
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_and_resume_in_one_session_equals_an_uninterrupted_crawl() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("10 Seitenabrufe", WAIT, || page_request_count(&site) >= 10);
    assert!(!mcp.call("pause_crawl", json!({"crawl_id": id})).is_error);
    mcp.wait_state(&id, "Paused", Duration::from_secs(20));
    assert!(
        (page_request_count(&site)) < reference.len(),
        "Crawl endete schon vor der Pause"
    );

    let r = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);
    // CRAWL-130: Läuft -> Pausiert -> Läuft -> Abgeschlossen (die Spezifikation nennt keine Frist; 5 s als Grenze).
    let resumed_at = Instant::now();
    mcp.wait_state(&id, "Running", Duration::from_secs(5));
    eprintln!(
        "Zeit bis crawl_status „Running“ meldet: {:?}",
        resumed_at.elapsed()
    );
    mcp.wait_state(&id, "Completed", WAIT);

    let file = env.crawl_file(&id);
    // CRAWL-128: Pausieren und Fortsetzen ändern die Kennung nicht und legen keinen zweiten Crawl an.
    let m = manifest(&file);
    assert_eq!(m["crawl_id"], id, "{m:?}");
    assert_eq!(crawl_file_count(&env), 1);
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    assert_eq!(m["completeness"], "complete", "{m:?}");
    // Keine verlorenen und keine zusätzlichen URLs, Statuscodes und Tiefen wie im ununterbrochenen Lauf.
    assert_eq!(crawled_set(&file), reference);
    // Keine URL doppelt im Request-Log.
    assert_eq!(duplicated(&site), BTreeSet::new());
    // Jede Referenz-URL genau einmal abgerufen.
    let requested: Vec<String> = page_requests(&site);
    assert_eq!(requested.len(), reference.len(), "{requested:?}");
}

// spec: AGENT-020
// spec: CRAWL-126
// spec: UI-003
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paused_crawl_resumes_in_a_new_server_process_with_the_same_result() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("10 Seitenabrufe", WAIT, || page_request_count(&site) >= 10);
    assert!(!mcp.call("pause_crawl", json!({"crawl_id": id})).is_error);
    mcp.wait_state(&id, "Paused", Duration::from_secs(20));
    mcp.close();

    // Neuer Prozess: der pausierte Crawl ist aus der Datei bekannt und fortsetzbar.
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    assert_eq!(mcp.state(&id), "Paused");
    let r = mcp.call("resume_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);
    mcp.wait_state(&id, "Completed", WAIT);
    let file = env.crawl_file(&id);
    assert_eq!(manifest(&file)["end_reason"], "queue_empty");
    assert_eq!(crawled_set(&file), reference);
    assert_eq!(duplicated(&site), BTreeSet::new());
}

// ---------------------------------------------------------------------------------------------------------
// Stopp (AGENT-020 stop_crawl, UI-003, CRAWL-128)
// ---------------------------------------------------------------------------------------------------------

/// Wartet auf den gemeldeten Endzustand `Completed` und liest die Besitzer-Metadaten sofort danach.
fn wait_completed_and_owner(mcp: &mut Mcp, file: &Path, id: &str) -> BTreeMap<String, String> {
    mcp.wait_state(id, "Completed", Duration::from_secs(30));
    meta_dump(file)
}

fn assert_owner_released(meta: &BTreeMap<String, String>) {
    let pid = meta.get("owner_pid").map(String::as_str).unwrap_or("");
    assert!(
        pid.is_empty(),
        "owner_pid nach dem gemeldeten Endzustand noch gesetzt: {pid:?} ({meta:?})"
    );
}

/// `resume_crawl` auf einem gestoppten Crawl: MCP-Fehler mit Hinweis, Datei unverändert, keine Anfragen.
fn assert_resume_refused_for_stopped(mcp: &mut Mcp, site: &TestSite, file: &Path, id: &str) {
    let before = snapshot(file);
    let requests = page_request_count(site);
    let r = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
    assert!(r.is_error, "resume_crawl auf gestopptem Crawl: {}", r.text);
    assert!(r.text.contains(STOPPED_HINT), "Hinweis fehlt: {}", r.text);
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(snapshot(file), before, "die Datei wurde verändert");
    assert_eq!(
        page_request_count(site),
        requests,
        "es wurde weitergecrawlt"
    );
    assert_eq!(mcp.state(id), "Completed");
}

// spec: AGENT-020
// spec: UI-003
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_crawl_ends_a_running_crawl_for_good() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("6 Seitenabrufe", WAIT, || page_request_count(&site) >= 6);
    let r = mcp.call("stop_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);

    let file = env.crawl_file(&id);
    let meta = wait_completed_and_owner(&mut mcp, &file, &id);
    // Besitzer-Metadaten sind sauber, sobald der Endzustand gemeldet ist.
    assert_owner_released(&meta);
    let m = manifest(&file);
    assert_eq!(m["state"], "Completed", "{m:?}");
    assert_eq!(m["end_reason"], "stopped", "{m:?}");
    assert_eq!(m["completeness"], "stopped", "{m:?}");
    assert_analysis_runs_after_stop(&file);
    // Die Warteschlange wurde nicht abgearbeitet: der Crawl ist nicht regulär zu Ende gelaufen.
    assert!(uint(&m, "urls_crawled") < 40, "{m:?}");

    // Endgültig: kein Fortsetzen, weder im selben noch in einem neuen Prozess.
    assert_resume_refused_for_stopped(&mut mcp, &site, &file, &id);
    mcp.close();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    assert_eq!(mcp.state(&id), "Completed");
    assert_resume_refused_for_stopped(&mut mcp, &site, &file, &id);
}

// spec: AGENT-020
// spec: UI-003
// spec: CRAWL-126
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_crawl_after_a_pause_is_final_in_the_same_session() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("6 Seitenabrufe", WAIT, || page_request_count(&site) >= 6);
    assert!(!mcp.call("pause_crawl", json!({"crawl_id": id})).is_error);
    mcp.wait_state(&id, "Paused", Duration::from_secs(20));

    let r = mcp.call_settled("stop_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);
    let file = env.crawl_file(&id);
    let meta = wait_completed_and_owner(&mut mcp, &file, &id);
    assert_owner_released(&meta);
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "stopped", "{m:?}");
    assert_eq!(m["completeness"], "stopped", "{m:?}");
    assert_analysis_runs_after_stop(&file);
    assert_resume_refused_for_stopped(&mut mcp, &site, &file, &id);
}

// spec: AGENT-020
// spec: UI-003
// spec: CRAWL-126
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_crawl_on_a_paused_crawl_of_an_earlier_process_is_final() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("6 Seitenabrufe", WAIT, || page_request_count(&site) >= 6);
    assert!(!mcp.call("pause_crawl", json!({"crawl_id": id})).is_error);
    mcp.wait_state(&id, "Paused", Duration::from_secs(20));
    mcp.close();

    let mut mcp = env.spawn(&["--allow", "crawl"]);
    assert_eq!(mcp.state(&id), "Paused");
    let r = mcp.call("stop_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);
    let file = env.crawl_file(&id);
    let meta = wait_completed_and_owner(&mut mcp, &file, &id);
    assert_owner_released(&meta);
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "stopped", "{m:?}");
    assert_analysis_runs_after_stop(&file);
    assert_resume_refused_for_stopped(&mut mcp, &site, &file, &id);
}

// ---------------------------------------------------------------------------------------------------------
// Zustandsmatrix von resume_crawl (AGENT-020, CRAWL-128)
// ---------------------------------------------------------------------------------------------------------

// spec: AGENT-020
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_crawl_rejects_a_regularly_completed_crawl_without_touching_the_file() {
    let site = site(FAST).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    mcp.wait_state(&id, "Completed", WAIT);
    let file = env.crawl_file(&id);
    // Auf den vollständig gemeldeten Endzustand warten (Besitzer freigegeben).
    wait_until("Besitzer frei", Duration::from_secs(20), || {
        meta_dump(&file)
            .get("owner_pid")
            .is_none_or(String::is_empty)
    });
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    let before = snapshot(&file);
    let requests = page_request_count(&site);

    let r = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
    assert!(
        r.is_error,
        "regulär beendeter Crawl darf nicht fortgesetzt werden: {}",
        r.text
    );
    assert!(!r.text.contains("panicked"), "{}", r.text);
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(snapshot(&file), before, "die Datei wurde verändert");
    assert_eq!(page_request_count(&site), requests);
    // Das Ergebnis gilt auch in einem neuen Prozess.
    mcp.close();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    assert!(mcp.call("resume_crawl", json!({"crawl_id": id})).is_error);
    assert_eq!(snapshot(&file), before);
}

// spec: AGENT-020
// spec: CRAWL-126
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_crawl_rejects_a_running_crawl_and_the_crawl_carries_on() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("4 Seitenabrufe", WAIT, || page_request_count(&site) >= 4);
    let r = mcp.call("resume_crawl", json!({"crawl_id": id}));
    assert!(
        r.is_error,
        "laufender Crawl darf nicht „fortgesetzt“ werden: {}",
        r.text
    );
    // Der laufende Crawl wird dadurch weder beendet noch verdoppelt.
    let n = page_request_count(&site);
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        page_request_count(&site) > n,
        "der Crawl läuft nicht weiter"
    );
    assert_eq!(mcp.state(&id), "Running");
    assert_eq!(duplicated(&site), BTreeSet::new());
}

// spec: AGENT-020
// spec: CRAWL-126
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_crawl_ended_by_the_url_limit_can_be_resumed_but_ends_again_at_the_limit() {
    let site = site(FAST).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, json!({"speed.threads": 1, "limits.max_urls": 12}));
    mcp.wait_state(&id, "Completed", WAIT);
    let file = env.crawl_file(&id);
    wait_until("Besitzer frei", Duration::from_secs(20), || {
        meta_dump(&file)
            .get("owner_pid")
            .is_none_or(String::is_empty)
    });
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "limit_reached", "{m:?}");
    assert_eq!(m["completeness"], "limited", "{m:?}");
    assert_eq!(uint(&m, "urls_crawled"), 12, "{m:?}");
    let requests = page_request_count(&site);

    // Limit-beendete Crawls sind fortsetzbar (AGENT-020); ohne Erhöhung endet es sofort wieder am Limit.
    let r = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(mcp.state(&id), "Completed");
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "limit_reached", "{m:?}");
    assert_eq!(uint(&m, "urls_crawled"), 12, "{m:?}");
    assert_eq!(
        page_request_count(&site),
        requests,
        "über das Limit hinaus gecrawlt"
    );
}

// spec: AGENT-020
// spec: CRAWL-128
// acceptance: independent
#[test]
fn control_tools_report_unknown_crawl_ids_as_mcp_errors() {
    let env = Env::with_config("");
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    for id in ["0199aaaa-0000-7000-8000-000000000001", "nope", ""] {
        for tool in ["crawl_status", "pause_crawl", "stop_crawl", "resume_crawl"] {
            let r = mcp.call(tool, json!({"crawl_id": id}));
            assert!(r.is_error, "{tool}({id:?}) ohne MCP-Fehler: {}", r.text);
        }
    }
    // Fehlende ID ebenso.
    for tool in ["pause_crawl", "stop_crawl", "resume_crawl"] {
        assert!(mcp.call(tool, json!({})).is_error, "{tool} ohne crawl_id");
    }
}

// ---------------------------------------------------------------------------------------------------------
// Unterbrechung (CRAWL-125, CRAWL-128) und Fremdprozess (AGENT-020)
// ---------------------------------------------------------------------------------------------------------

fn copy_crawl_files(file: &Path, to: &Path) -> PathBuf {
    let name = file.file_name().unwrap();
    for suffix in ["", "-wal", "-shm"] {
        let mut src = file.as_os_str().to_owned();
        src.push(suffix);
        let src = PathBuf::from(src);
        if src.exists() {
            let mut dst = to.join(name).into_os_string();
            dst.push(suffix);
            std::fs::copy(&src, dst).unwrap();
        }
    }
    to.join(name)
}

// spec: AGENT-020
// spec: CRAWL-125
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_killed_server_leaves_an_incomplete_crawl_that_resumes_without_duplicates() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("10 Seitenabrufe", WAIT, || page_request_count(&site) >= 10);
    // CRAWL-125: Diese Seiten sind beim Abbruch länger als 2 s fertig und müssen gespeichert sein.
    let must_be_stored = snapshot_pages_stored_by_kill_time(&site);
    assert!(must_be_stored.len() >= 10, "Voraussetzung des Tests");
    std::thread::sleep(KILL_AFTER_SNAPSHOT);
    assert!(
        page_request_count(&site) < reference.len(),
        "Voraussetzung: der Crawl darf vor dem Abbruch nicht enden"
    );
    mcp.kill();

    let file = env.crawl_file(&id);
    // Die Datei ist nach SIGKILL konsistent (CRAWL-125: integrity_check = ok), geprüft an einer Kopie.
    let scratch = tempfile::tempdir().unwrap();
    let copy = copy_crawl_files(&file, scratch.path());
    let check = CrawlFile::open(&copy, OpenMode::ReadWrite).unwrap();
    let result: String = check
        .conn()
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(result, "ok");
    drop(check);

    let mut mcp = env.spawn(&["--allow", "crawl"]);
    // Nach dem Ausbleiben des Heartbeats gilt der Crawl als „Unterbrochen“.
    mcp.wait_state(&id, "Incomplete", Duration::from_secs(60));
    let stored_before = paths(&crawled_set(&file));
    let missing: Vec<&String> = must_be_stored.difference(&stored_before).collect();
    assert!(
        missing.is_empty(),
        "CRAWL-125: länger als 2 s vor dem Abbruch abgerufene URLs fehlen in der Datei: {missing:?}"
    );
    assert!(
        stored_before.len() < reference.len(),
        "der Abbruch kam erst nach dem Ende des Crawls"
    );
    assert_eq!(manifest(&file)["completeness"], "interrupted");

    let r = mcp.call("resume_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);
    mcp.wait_state(&id, "Completed", WAIT);
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    assert_eq!(crawled_set(&file), reference);
    // CRAWL-125: eine bereits gespeicherte URL wird nicht erneut gecrawlt.
    for dup in duplicated(&site) {
        assert!(
            !stored_before.contains(&dup),
            "gespeicherte URL erneut abgerufen: {dup}"
        );
    }
}

// spec: AGENT-020
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_server_cannot_control_a_crawl_run_by_another_process() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let mut owner = env.spawn(&["--allow", "crawl"]);
    let id = owner.start(&site, threads1());
    wait_until("4 Seitenabrufe", WAIT, || page_request_count(&site) >= 4);

    let file = env.crawl_file(&id);
    let meta = meta_dump(&file);
    assert_eq!(meta["owner_kind"], "mcp-stdio");
    assert_eq!(meta["owner_pid"], owner.pid().to_string());

    // Ein zweiter Prozess sieht den Crawl lesend, steuern kann er ihn nicht (Steuerung nur im Besitzerprozess).
    let mut other = env.spawn(&["--allow", "crawl"]);
    assert_eq!(other.state(&id), "Running");
    for tool in ["pause_crawl", "stop_crawl", "resume_crawl"] {
        let r = other.call(tool, json!({"crawl_id": id}));
        assert!(
            r.is_error,
            "{tool} im Fremdprozess ohne MCP-Fehler: {}",
            r.text
        );
    }
    // Der Crawl lief unbeeinflusst weiter und endet regulär mit dem vollständigen Ergebnis.
    owner.wait_state(&id, "Completed", WAIT);
    assert_eq!(manifest(&file)["end_reason"], "queue_empty");
    assert_eq!(crawled_set(&file), reference);
    assert_eq!(duplicated(&site), BTreeSet::new());
}

// ---------------------------------------------------------------------------------------------------------
// Zusätzliche Szenarien: mehrere Zyklen, Parallelität, Listenmodus, Stopp nach Fortsetzen
// ---------------------------------------------------------------------------------------------------------

/// Pfad und Statuscode (die Tiefe hängt bei mehreren Threads von der Abrufreihenfolge ab).
fn path_status(set: &BTreeSet<(String, i64, i64)>) -> BTreeSet<(String, i64)> {
    set.iter().map(|(p, s, _)| (p.clone(), *s)).collect()
}

// spec: AGENT-020
// spec: CRAWL-126
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_pause_and_resume_cycles_with_parallel_threads_lose_and_duplicate_nothing() {
    let reference = reference_set().await;
    let site = site(SLOW_PARALLEL).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    // Standard-Threadzahl (parallele Anfragen): beim Pausieren sind mehrere Anfragen unterwegs.
    let id = mcp.start(&site, json!({}));
    let mut seen = 0;
    for cycle in 0..3 {
        wait_until("weitere Seitenabrufe", WAIT, || {
            page_request_count(&site) >= seen + 4
        });
        let pause = mcp.call_settled("pause_crawl", json!({"crawl_id": id}));
        assert!(!pause.is_error, "Zyklus {cycle}: {}", pause.text);
        mcp.wait_state(&id, "Paused", Duration::from_secs(20));
        seen = page_request_count(&site);
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(
            page_request_count(&site),
            seen,
            "Zyklus {cycle}: Anfragen während der Pause"
        );
        let resume = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
        assert!(!resume.is_error, "Zyklus {cycle}: {}", resume.text);
    }
    mcp.wait_state(&id, "Completed", WAIT);
    let file = env.crawl_file(&id);
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    assert_eq!(path_status(&crawled_set(&file)), path_status(&reference));
    assert_eq!(duplicated(&site), BTreeSet::new());
    assert_eq!(crawl_file_count(&env), 1);
}

// spec: AGENT-020
// spec: CRAWL-126
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_list_crawl_survives_pause_and_resume() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let listed: Vec<String> = (1..=24)
        .map(|i| site.url(&format!("/s{i}/p{i}.html")))
        .collect();
    let r = mcp.call(
        "start_crawl",
        json!({"url_list": listed, "overrides": {"speed.threads": 1}}),
    );
    let id = r.summary()["crawl_id"].as_str().unwrap().to_owned();
    wait_until("8 Seitenabrufe", WAIT, || page_request_count(&site) >= 8);
    assert!(!mcp.call("pause_crawl", json!({"crawl_id": id})).is_error);
    mcp.wait_state(&id, "Paused", Duration::from_secs(20));
    assert!(
        page_request_count(&site) < 24,
        "Listenlauf war schon vor der Pause fertig"
    );
    let resume = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
    assert!(!resume.is_error, "{}", resume.text);
    mcp.wait_state(&id, "Completed", WAIT);
    let file = env.crawl_file(&id);
    let m = manifest(&file);
    assert_eq!(m["mode"], "list", "{m:?}");
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    let expected: BTreeSet<String> = (1..=24).map(|i| format!("/s{i}/p{i}.html")).collect();
    assert_eq!(paths(&crawled_set(&file)), expected);
    assert_eq!(duplicated(&site), BTreeSet::new());
    assert_eq!(page_request_count(&site), 24);
}

// spec: AGENT-020
// spec: UI-003
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_crawl_after_a_resume_is_final() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = env.spawn(&["--allow", "crawl"]);
    let id = mcp.start(&site, threads1());
    wait_until("5 Seitenabrufe", WAIT, || page_request_count(&site) >= 5);
    assert!(!mcp.call("pause_crawl", json!({"crawl_id": id})).is_error);
    mcp.wait_state(&id, "Paused", Duration::from_secs(20));
    let resume = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
    assert!(!resume.is_error, "{}", resume.text);
    let at_resume = page_request_count(&site);
    wait_until("Weitercrawlen", WAIT, || {
        page_request_count(&site) >= at_resume + 3
    });

    let stop = mcp.call_settled("stop_crawl", json!({"crawl_id": id}));
    assert!(!stop.is_error, "{}", stop.text);
    let file = env.crawl_file(&id);
    let meta = wait_completed_and_owner(&mut mcp, &file, &id);
    assert_owner_released(&meta);
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "stopped", "{m:?}");
    assert!(uint(&m, "urls_crawled") < 40, "{m:?}");
    assert_analysis_runs_after_stop(&file);
    assert_resume_refused_for_stopped(&mut mcp, &site, &file, &id);
}

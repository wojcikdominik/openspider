//! Unabhängige Abnahmetests B4-RESUME (CLI und `openspider mcp`): Crawls anhalten, fortsetzen und stoppen.
//!
//! Erwartetes Verhalten stammt ausschließlich aus der Spezifikation (CRAWL-125, CRAWL-126, CRAWL-128, UI-003,
//! OUT-204, OUT-228, OUT-233, AGENT-001, AGENT-003, AGENT-020); der Code wurde nur gelesen, um zu wissen, wie die
//! Schnittstellen aufgerufen werden. Geprüft wird von außen: das Binary `openspider` (Exit-Codes, Ausgaben,
//! SIGINT/SIGKILL), ein echter `openspider mcp`-Prozess über STDIO und Fixtures von `os-testsite` (synthetische Site
//! mit Antwortverzögerung, damit Pause, Stopp und Abbruch garantiert vor dem natürlichen Crawl-Ende greifen).
//!
//! Kriterienliste: `docs/plan/M3-reports/b4-resume-tests.md`.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use common::{Env, Run};
use os_store::{CrawlFile, OpenMode};
use os_testsite::{SiteOptions, TestSite};
use serde_json::{json, Value};

const SLOW: &str = "synthetic:n=40,links=4,seed=7,delay_ms=250";
const FAST: &str = "synthetic:n=40,links=4,seed=7";
const WAIT: Duration = Duration::from_secs(60);
const STOPPED_HINT: &str = "gestoppt – endgültig beendet";

// ---------------------------------------------------------------------------------------------------------
// Hilfen: Testsite, Crawl-Datei
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

fn wait_until(what: &str, limit: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < limit, "Zeitüberschreitung: {what}");
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn open(path: &Path) -> CrawlFile {
    CrawlFile::open(path, OpenMode::ReadOnly).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn key_values(path: &Path, sql: &str) -> BTreeMap<String, String> {
    let file = open(path);
    let mut stmt = file.conn().prepare(sql).unwrap();
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

/// `q_manifest` (OUT-233).
fn manifest(path: &Path) -> BTreeMap<String, String> {
    key_values(path, "SELECT key, value FROM q_manifest")
}

/// Vollständige Crawl-Metadaten.
fn meta_dump(path: &Path) -> BTreeMap<String, String> {
    key_values(path, "SELECT key, value FROM crawl_meta")
}

type UrlSet = BTreeSet<(String, i64, i64)>;

/// Gecrawlte interne URLs als (Pfad, Statuscode, Tiefe); Host und Port sind normalisiert.
fn crawled_set(path: &Path) -> UrlSet {
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

fn paths(set: &UrlSet) -> BTreeSet<String> {
    set.iter().map(|(p, _, _)| p.clone()).collect()
}

type Snapshot = (BTreeMap<String, String>, BTreeMap<String, String>, UrlSet);

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

fn uint(m: &BTreeMap<String, String>, key: &str) -> u64 {
    m[key]
        .parse()
        .unwrap_or_else(|_| panic!("{key} = {:?}", m[key]))
}

fn only_file(env: &Env) -> (PathBuf, String) {
    let files = env.crawl_files();
    assert_eq!(files.len(), 1, "{files:?}");
    let id = files[0].file_stem().unwrap().to_string_lossy().into_owned();
    (files[0].clone(), id)
}

/// Zeile der Crawl-Verwaltung (`--list-crawls --json`) für die ID.
fn listed_status(env: &Env, id: &str) -> String {
    let run = env.run(&["--list-crawls", "--json"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    let v: Value =
        serde_json::from_str(&run.stdout).unwrap_or_else(|e| panic!("{e}: {}", run.stdout));
    let row = v
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(id))
        .unwrap_or_else(|| panic!("{id} fehlt in --list-crawls: {v}"));
    row["status"].as_str().unwrap().to_owned()
}

fn crawl_args(site: &TestSite, extra: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = ["--crawl", &site.url("/"), "--set", "speed.threads=1"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    v.extend(extra.iter().map(|s| (*s).to_owned()));
    v
}

/// Vollständiger, ununterbrochener CLI-Lauf der schnellen Site; liefert die Menge.
async fn reference_set() -> UrlSet {
    let site = site(FAST).await;
    let env = Env::new();
    let run = env.run_async(crawl_args(&site, &[])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let (file, _) = only_file(&env);
    let set = crawled_set(&file);
    assert_eq!(manifest(&file)["end_reason"], "queue_empty");
    assert!(set.len() >= 30, "Referenzlauf zu klein: {}", set.len());
    set
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn resume_args(id: &str, extra: &[&str]) -> Vec<String> {
    let mut v = strings(&["--load-crawl", id, "--resume"]);
    v.extend(extra.iter().map(|s| (*s).to_owned()));
    v
}

fn spawn_cli(env: &Env, args: Vec<String>) -> Child {
    let mut cmd = env.command();
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.spawn().expect("openspider startbar")
}

async fn finish(child: Child) -> Run {
    tokio::task::spawn_blocking(move || Run::from_output(child.wait_with_output().unwrap()))
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------------------------------------
// MCP-Client über `openspider mcp`
// ---------------------------------------------------------------------------------------------------------

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

    fn summary(&self) -> Value {
        assert!(!self.is_error, "Fehlerergebnis: {}", self.text);
        let v: Value = serde_json::from_str(&self.text)
            .unwrap_or_else(|e| panic!("kein JSON ({e}): {}", self.text));
        v["summary"].clone()
    }
}

fn spawn_mcp(env: &Env, args: &[&str]) -> Mcp {
    // Freigabe privater Ziele: die Fixture läuft auf 127.0.0.1 (nur MCP-Crawls sind davon betroffen).
    let toml = env.home.path().join("mcp.toml");
    if !toml.exists() {
        std::fs::write(&toml, "allow_private_targets = true\n").unwrap();
    }
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_openspider"));
    cmd.arg("mcp")
        .env("OPENSPIDER_HOME", env.home.path())
        .env("OPENSPIDER_SECRET_STORE", "memory")
        .env_remove("OPENSPIDER_LOG")
        .env_remove("RUST_LOG")
        .arg("--crawl-dir")
        .arg(env.crawls_dir())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().expect("openspider mcp startbar");
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
    mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    mcp
}

impl Mcp {
    fn send(&mut self, v: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin offen");
        writeln!(stdin, "{v}").unwrap();
        stdin.flush().unwrap();
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

    /// Wie [`Mcp::call`], wiederholt aber, solange der Server „busy“ meldet.
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
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn send_sigint(child: &Child) {
    let pid = i32::try_from(child.id()).unwrap();
    // SAFETY: `kill` mit der PID eines eigenen Kindprozesses.
    let rc = unsafe { libc::kill(pid, libc::SIGINT) };
    assert_eq!(rc, 0, "kill(SIGINT) fehlgeschlagen");
}

/// Startet einen langsamen CLI-Crawl, wartet auf `n` Seitenabrufe und pausiert ihn per SIGINT.
/// Liefert (Datei, ID, Lauf).
async fn cli_paused_crawl(env: &Env, site: &TestSite, n: usize) -> (PathBuf, String, Run) {
    let child = spawn_cli(env, crawl_args(site, &[]));
    wait_until("Seitenabrufe", WAIT, || page_request_count(site) >= n);
    send_sigint(&child);
    let run = finish(child).await;
    assert_eq!(run.code, 130, "{}", run.dump());
    let (file, id) = only_file(env);
    (file, id, run)
}

// ---------------------------------------------------------------------------------------------------------
// CLI: SIGINT pausiert, --resume setzt fort (CRAWL-126, CRAWL-128, OUT-204, OUT-228)
// ---------------------------------------------------------------------------------------------------------

// spec: CRAWL-126
// spec: CRAWL-128
// spec: OUT-204
// spec: OUT-228
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigint_pauses_with_exit_130_and_resume_equals_an_uninterrupted_crawl() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let (file, id, run) = cli_paused_crawl(&env, &site, 10).await;
    // OUT-204: die letzte Zeile auf stderr fasst Ergebnis und Exit-Code zusammen.
    assert!(run.last_line().contains("130"), "{}", run.dump());
    // CRAWL-126/OUT-228: in der Verwaltung „Paused“, Datei gesichert, die Warteschlange ist nicht abgearbeitet.
    assert_eq!(listed_status(&env, &id), "Paused");
    let m = manifest(&file);
    assert_eq!(m["state"], "Paused", "{m:?}");
    assert_eq!(m["completeness"], "paused", "{m:?}");
    let stored = crawled_set(&file);
    assert!(
        stored.len() >= 8 && stored.len() < reference.len(),
        "{}",
        stored.len()
    );

    let resumed = env.run_async(resume_args(&id, &[])).await;
    assert_eq!(resumed.code, 0, "{}", resumed.dump());
    // Gleiche, stabile ID; Status „Completed“; Ergebnis wie ein ununterbrochener Crawl.
    assert_eq!(only_file(&env).1, id);
    assert_eq!(listed_status(&env, &id), "Completed");
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    assert_eq!(m["completeness"], "complete", "{m:?}");
    assert_eq!(crawled_set(&file), reference);
    // Keine URL doppelt im Request-Log, keine verloren.
    assert_eq!(duplicated(&site), BTreeSet::new());
    assert_eq!(page_request_count(&site), reference.len());
}

// spec: CRAWL-126
// spec: AGENT-020
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigint_stops_new_requests_promptly() {
    let site = site(SLOW).await;
    let env = Env::new();
    let child = spawn_cli(&env, crawl_args(&site, &[]));
    wait_until("6 Seitenabrufe", WAIT, || page_request_count(&site) >= 6);
    let before = page_request_count(&site);
    send_sigint(&child);
    let run = finish(child).await;
    assert_eq!(run.code, 130, "{}", run.dump());
    let after = page_request_count(&site);
    // Höchstens Threads-viele (1) laufende Anfragen werden noch zu Ende geführt.
    assert!(after <= before + 2, "{before} -> {after}");
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(page_request_count(&site), after);
    let (file, _) = only_file(&env);
    assert_eq!(
        crawled_set(&file).len(),
        after,
        "abgerufene Seiten sind gespeichert"
    );
}

// ---------------------------------------------------------------------------------------------------------
// CLI: SIGKILL → Unterbrochen → Fortsetzen (CRAWL-125, CRAWL-128, OUT-228)
// ---------------------------------------------------------------------------------------------------------

// spec: CRAWL-125
// spec: CRAWL-128
// spec: OUT-228
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_killed_cli_crawl_is_listed_as_incomplete_and_resumes_without_duplicates() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let mut child = spawn_cli(&env, crawl_args(&site, &[]));
    wait_until("10 Seitenabrufe", WAIT, || page_request_count(&site) >= 10);
    child.kill().unwrap(); // SIGKILL
    let _ = finish(child).await;
    let (file, id) = only_file(&env);

    // Nach dem Ausbleiben des Heartbeats: „Incomplete“ (CRAWL-128/OUT-228).
    let start = Instant::now();
    loop {
        let status = listed_status(&env, &id);
        if status == "Incomplete" {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "nach SIGKILL nicht Incomplete: {status}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
    assert_eq!(manifest(&file)["completeness"], "interrupted");
    let stored_before = paths(&crawled_set(&file));
    assert!(stored_before.len() >= 8 && stored_before.len() < reference.len());

    let resumed = env.run_async(resume_args(&id, &[])).await;
    assert_eq!(resumed.code, 0, "{}", resumed.dump());
    assert_eq!(listed_status(&env, &id), "Completed");
    assert_eq!(crawled_set(&file), reference);
    for dup in duplicated(&site) {
        assert!(
            !stored_before.contains(&dup),
            "gespeicherte URL erneut abgerufen: {dup}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------
// CLI: Zustandsmatrix für --resume (CRAWL-128, OUT-184-Zusatz „--resume“)
// ---------------------------------------------------------------------------------------------------------

/// `--load-crawl <id> --resume` muss mit Exit 1 abgelehnt werden, ohne Datei und Site anzufassen.
async fn assert_resume_refused(env: &Env, site: &TestSite, file: &Path, id: &str) -> Run {
    let before = snapshot(file);
    let requests = site.total_requests();
    let run = env.run_async(resume_args(id, &[])).await;
    assert_eq!(run.code, 1, "{}", run.dump());
    assert!(
        !run.stderr.trim().is_empty(),
        "keine Meldung: {}",
        run.dump()
    );
    assert_eq!(snapshot(file), before, "die Datei wurde verändert");
    assert_eq!(site.total_requests(), requests, "es wurde weitergecrawlt");
    run
}

// spec: CRAWL-128
// spec: AGENT-020
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_of_a_regularly_completed_crawl_exits_1_with_the_state_and_changes_nothing() {
    let site = site(FAST).await;
    let env = Env::new();
    let run = env.run_async(crawl_args(&site, &[])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let (file, id) = only_file(&env);
    assert_eq!(manifest(&file)["end_reason"], "queue_empty");
    let refused = assert_resume_refused(&env, &site, &file, &id).await;
    let lower = refused.stderr.to_lowercase();
    assert!(
        lower.contains("completed") || lower.contains("abgeschlossen"),
        "Meldung nennt den Zustand nicht: {}",
        refused.dump()
    );
}

// spec: CRAWL-128
// spec: CRAWL-126
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_limit_ended_crawl_resumes_with_a_raised_limit_and_stays_at_the_limit_without_one() {
    let reference = reference_set().await;
    let site = site(FAST).await;
    let env = Env::new();
    let run = env
        .run_async(crawl_args(&site, &["--max-urls", "12"]))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let (file, id) = only_file(&env);
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "limit_reached", "{m:?}");
    assert_eq!(m["completeness"], "limited", "{m:?}");
    assert_eq!(uint(&m, "urls_crawled"), 12, "{m:?}");

    // Ohne Erhöhung endet „Fortsetzen“ sofort wieder am Limit: nichts Neues wird abgerufen.
    let requests = page_request_count(&site);
    let again = env.run_async(resume_args(&id, &[])).await;
    assert_ne!(again.code, 2, "{}", again.dump());
    assert_eq!(page_request_count(&site), requests, "{}", again.dump());
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "limit_reached", "{m:?}");
    assert_eq!(uint(&m, "urls_crawled"), 12, "{m:?}");

    // Mit erhöhtem Limit setzt „Fortsetzen“ fort und erreicht das Ergebnis des vollständigen Laufs.
    let raised = env
        .run_async(resume_args(&id, &["--max-urls", "1000"]))
        .await;
    assert_eq!(raised.code, 0, "{}", raised.dump());
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    assert_eq!(crawled_set(&file), reference);
    assert_eq!(duplicated(&site), BTreeSet::new());
}

// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_of_a_failed_crawl_exits_1_with_the_state_and_changes_nothing() {
    // Datei im Zustand „Failed“ nach CRAWL-128 (generierte Crawl-Datei, Testfixture dieses Eintrags): ein
    // abgebrochener Lauf mit nicht leerer Warteschlange, der als fehlgeschlagen markiert ist.
    let site = site(SLOW).await;
    let env = Env::new();
    let mut child = spawn_cli(&env, crawl_args(&site, &[]));
    wait_until("5 Seitenabrufe", WAIT, || page_request_count(&site) >= 5);
    child.kill().unwrap();
    let _ = finish(child).await;
    let (file, id) = only_file(&env);
    {
        let f = CrawlFile::open(&file, OpenMode::ReadWrite).unwrap();
        f.set_meta("state", "Failed").unwrap();
    }
    assert_eq!(listed_status(&env, &id), "Failed");
    let refused = assert_resume_refused(&env, &site, &file, &id).await;
    let lower = refused.stderr.to_lowercase();
    assert!(
        lower.contains("failed") || lower.contains("fehlgeschlagen"),
        "Meldung nennt den Zustand nicht: {}",
        refused.dump()
    );
}

// spec: CRAWL-128
// spec: AGENT-020
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_resume_crawl_rejects_a_failed_crawl_without_touching_the_file() {
    // Generierte Datei im Zustand „Failed“ (siehe oben): nicht leere Warteschlange, Zustand gewinnt.
    let site = site(SLOW).await;
    let env = Env::new();
    let mut child = spawn_cli(&env, crawl_args(&site, &[]));
    wait_until("5 Seitenabrufe", WAIT, || page_request_count(&site) >= 5);
    child.kill().unwrap();
    let _ = finish(child).await;
    let (file, id) = only_file(&env);
    {
        let f = CrawlFile::open(&file, OpenMode::ReadWrite).unwrap();
        f.set_meta("state", "Failed").unwrap();
    }
    let before = snapshot(&file);
    let requests = site.total_requests();
    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let r = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
    assert!(
        r.is_error,
        "Failed-Crawl darf nicht fortgesetzt werden: {}",
        r.text
    );
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(snapshot(&file), before, "die Datei wurde verändert");
    assert_eq!(site.total_requests(), requests, "es wurde weitergecrawlt");
}

// spec: CRAWL-128
// spec: OUT-204
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_crawl_whose_start_url_is_unreachable_is_listed_as_failed() {
    // CRAWL-128: „Fehlgeschlagen: der Crawl konnte nicht starten oder brach mit fatalem Fehler ab
    // (z. B. Start-URL nicht erreichbar)“; OUT-204: mit `--fail-on-start-error` Exit-Code 3.
    let env = Env::new();
    let run = env
        .run_async(strings(&[
            "--crawl",
            "http://127.0.0.1:1/",
            "--fail-on-start-error",
        ]))
        .await;
    assert_eq!(run.code, 3, "{}", run.dump());
    let (_, id) = only_file(&env);
    assert_eq!(listed_status(&env, &id), "Failed", "{}", run.dump());
}

// spec: CRAWL-128
// spec: AGENT-020
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_of_a_running_crawl_of_another_process_is_refused_and_the_crawl_carries_on() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let child = spawn_cli(&env, crawl_args(&site, &[]));
    wait_until("4 Seitenabrufe", WAIT, || page_request_count(&site) >= 4);
    let (file, id) = only_file(&env);
    let meta = meta_dump(&file);
    assert_eq!(meta["owner_kind"], "cli", "{meta:?}");
    assert_eq!(meta["owner_pid"], child.id().to_string(), "{meta:?}");

    let refused = env.run_async(resume_args(&id, &[])).await;
    assert_ne!(refused.code, 0, "{}", refused.dump());

    let run = finish(child).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert_eq!(crawled_set(&file), reference);
    assert_eq!(duplicated(&site), BTreeSet::new());
}

// ---------------------------------------------------------------------------------------------------------
// Zusammenspiel CLI <-> MCP (AGENT-020: gleiches Ergebnis wie der CLI-Aufruf)
// ---------------------------------------------------------------------------------------------------------

// spec: AGENT-020
// spec: CRAWL-126
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cli_paused_crawl_is_resumed_by_mcp_with_the_same_result() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let (file, id, _) = cli_paused_crawl(&env, &site, 10).await;

    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    assert_eq!(mcp.state(&id), "Paused");
    let r = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);
    mcp.wait_state(&id, "Completed", WAIT);
    assert_eq!(manifest(&file)["end_reason"], "queue_empty");
    assert_eq!(crawled_set(&file), reference);
    assert_eq!(duplicated(&site), BTreeSet::new());
}

// spec: AGENT-020
// spec: CRAWL-126
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_mcp_paused_crawl_is_resumed_by_the_cli_with_the_same_result() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let id = mcp.start(&site, json!({"speed.threads": 1}));
    wait_until("10 Seitenabrufe", WAIT, || page_request_count(&site) >= 10);
    assert!(!mcp.call("pause_crawl", json!({"crawl_id": id})).is_error);
    mcp.wait_state(&id, "Paused", Duration::from_secs(20));
    mcp.close();

    let (file, only_id) = only_file(&env);
    assert_eq!(only_id, id);
    assert_eq!(listed_status(&env, &id), "Paused");
    let resumed = env.run_async(resume_args(&id, &[])).await;
    assert_eq!(resumed.code, 0, "{}", resumed.dump());
    assert_eq!(manifest(&file)["end_reason"], "queue_empty");
    assert_eq!(crawled_set(&file), reference);
    assert_eq!(duplicated(&site), BTreeSet::new());
}

fn assert_owner_released(meta: &BTreeMap<String, String>) {
    let pid = meta.get("owner_pid").map(String::as_str).unwrap_or("");
    assert!(
        pid.is_empty(),
        "owner_pid nach dem gemeldeten Endzustand noch gesetzt: {pid:?} ({meta:?})"
    );
}

/// Wartet, bis die automatische Crawl-Analyse nach dem Ende abgeschlossen ist (UI-003, CRAWL-141).
fn wait_analysis_done(file: &Path) {
    wait_until("Crawl-Analyse", Duration::from_secs(30), || {
        manifest(file)["analysis_state"] == "done"
    });
}

// spec: AGENT-020
// spec: UI-003
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_crawl_stopped_via_mcp_is_final_for_cli_and_mcp() {
    let site = site(SLOW).await;
    let env = Env::new();
    let (file, id, _) = cli_paused_crawl(&env, &site, 8).await;

    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let r = mcp.call("stop_crawl", json!({"crawl_id": id}));
    assert!(!r.is_error, "{}", r.text);
    mcp.wait_state(&id, "Completed", Duration::from_secs(30));
    // Besitzer-Metadaten sauber, sobald der Endzustand gemeldet ist.
    assert_owner_released(&meta_dump(&file));
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "stopped", "{m:?}");
    assert_eq!(m["completeness"], "stopped", "{m:?}");
    // UI-003: eine eingestellte automatische Crawl-Analyse startet wie nach einem regulären Ende.
    wait_analysis_done(&file);
    assert_eq!(listed_status(&env, &id), "Completed");

    // MCP: Fehler mit Hinweis, Datei unverändert.
    let before = snapshot(&file);
    let r = mcp.call_settled("resume_crawl", json!({"crawl_id": id}));
    assert!(r.is_error, "{}", r.text);
    assert!(r.text.contains(STOPPED_HINT), "{}", r.text);
    assert_eq!(snapshot(&file), before);
    mcp.close();

    // CLI: Exit 1 mit „gestoppt – endgültig beendet“, Datei unverändert, keine Anfragen.
    let refused = assert_resume_refused(&env, &site, &file, &id).await;
    assert!(refused.stderr.contains(STOPPED_HINT), "{}", refused.dump());
}

// spec: AGENT-020
// spec: UI-003
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_mcp_crawl_stopped_via_mcp_is_final_for_the_cli() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let id = mcp.start(&site, json!({"speed.threads": 1}));
    wait_until("6 Seitenabrufe", WAIT, || page_request_count(&site) >= 6);
    assert!(!mcp.call("stop_crawl", json!({"crawl_id": id})).is_error);
    mcp.wait_state(&id, "Completed", Duration::from_secs(30));
    let (file, _) = only_file(&env);
    assert_owner_released(&meta_dump(&file));
    assert_eq!(manifest(&file)["end_reason"], "stopped");
    wait_analysis_done(&file);
    mcp.close();
    let refused = assert_resume_refused(&env, &site, &file, &id).await;
    assert!(refused.stderr.contains(STOPPED_HINT), "{}", refused.dump());
}

// spec: AGENT-020
// spec: CRAWL-128
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_cannot_control_a_crawl_run_by_the_cli() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    let child = spawn_cli(&env, crawl_args(&site, &[]));
    wait_until("4 Seitenabrufe", WAIT, || page_request_count(&site) >= 4);
    let (file, id) = only_file(&env);

    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    // Lesend sichtbar als laufend; Pause, Stopp und Fortsetzen sind nur im Besitzerprozess möglich.
    assert_eq!(mcp.state(&id), "Running");
    for tool in ["pause_crawl", "stop_crawl", "resume_crawl"] {
        let r = mcp.call(tool, json!({"crawl_id": id}));
        assert!(
            r.is_error,
            "{tool} fremder Crawl ohne MCP-Fehler: {}",
            r.text
        );
    }
    mcp.close();
    let run = finish(child).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    assert_eq!(crawled_set(&file), reference);
    assert_eq!(duplicated(&site), BTreeSet::new());
}

// ---------------------------------------------------------------------------------------------------------
// Freigaben von `openspider mcp` (AGENT-001, AGENT-003)
// ---------------------------------------------------------------------------------------------------------

const CRAWL_TOOLS: [&str; 5] = [
    "start_crawl",
    "crawl_status",
    "pause_crawl",
    "stop_crawl",
    "resume_crawl",
];

// spec: AGENT-001
// spec: AGENT-003
// spec: AGENT-020
// acceptance: independent
#[test]
fn openspider_mcp_lists_crawl_tools_only_with_the_crawl_class_and_refuses_calls_otherwise() {
    let env = Env::new();
    std::fs::write(env.home.path().join("mcp.toml"), "").unwrap();
    for flags in [&[][..], &["--read-only"][..]] {
        let mut mcp = spawn_mcp(&env, flags);
        let names = mcp.tool_names();
        for t in CRAWL_TOOLS {
            assert!(!names.contains(t), "{flags:?}: {t} gelistet");
        }
        for tool in CRAWL_TOOLS {
            let r = mcp.call(tool, json!({"crawl_id": "0199aaaa-0000-7000-8000-000000000001", "url": "http://127.0.0.1:9/"}));
            assert!(r.is_error, "{tool} {flags:?} ohne MCP-Fehler: {}", r.text);
            let lower = r.text.to_lowercase();
            assert!(
                lower.contains("allow") && lower.contains("crawl"),
                "Hinweis zur Freigabe fehlt: {}",
                r.text
            );
        }
        mcp.close();
    }
    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let names = mcp.tool_names();
    for t in CRAWL_TOOLS {
        assert!(names.contains(t), "--allow crawl: {t} fehlt");
    }
    assert!(
        !names.contains("export"),
        "write-Werkzeug sichtbar: {names:?}"
    );
}

// spec: AGENT-001
// acceptance: independent
#[test]
fn read_only_together_with_allow_crawl_does_not_start() {
    let env = Env::new();
    let run = env.run(&["mcp", "--read-only", "--allow", "crawl"]);
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(
        run.stderr.contains("--read-only") && run.stderr.contains("--allow"),
        "{}",
        run.dump()
    );
    // `--read-only --allow read` startet normal: bei geschlossener stdin endet der Server mit Exit-Code 0.
    let run = env.run(&["mcp", "--read-only", "--allow", "read"]);
    assert_eq!(run.code, 0, "{}", run.dump());
}

// ---------------------------------------------------------------------------------------------------------
// Zusätzliche Szenarien: mehrere SIGINT-Zyklen mit parallelen Threads
// ---------------------------------------------------------------------------------------------------------

// spec: CRAWL-126
// spec: OUT-204
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_sigint_cycles_with_parallel_threads_lose_and_duplicate_nothing() {
    let reference = reference_set().await;
    let site = site(SLOW).await;
    let env = Env::new();
    // Erster Lauf mit mehreren Threads; SIGINT nach 5 Abrufen.
    let child = spawn_cli(
        &env,
        strings(&["--crawl", &site.url("/"), "--set", "speed.threads=5"]),
    );
    wait_until("5 Seitenabrufe", WAIT, || page_request_count(&site) >= 5);
    send_sigint(&child);
    let run = finish(child).await;
    assert_eq!(run.code, 130, "{}", run.dump());
    let (file, id) = only_file(&env);
    assert_eq!(listed_status(&env, &id), "Paused");

    // Fortsetzen und erneut mit SIGINT pausieren.
    let seen = page_request_count(&site);
    let child = spawn_cli(&env, resume_args(&id, &[]));
    wait_until("weitere Abrufe", WAIT, || {
        page_request_count(&site) >= seen + 5
    });
    send_sigint(&child);
    let run = finish(child).await;
    assert_eq!(run.code, 130, "{}", run.dump());
    assert_eq!(listed_status(&env, &id), "Paused");
    let after_second = page_request_count(&site);
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(
        page_request_count(&site),
        after_second,
        "Anfragen nach dem zweiten Pausieren"
    );

    // Zu Ende führen.
    let resumed = env.run_async(resume_args(&id, &[])).await;
    assert_eq!(resumed.code, 0, "{}", resumed.dump());
    assert_eq!(listed_status(&env, &id), "Completed");
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    let got: BTreeSet<(String, i64)> = crawled_set(&file)
        .into_iter()
        .map(|(p, s, _)| (p, s))
        .collect();
    let want: BTreeSet<(String, i64)> = reference.into_iter().map(|(p, s, _)| (p, s)).collect();
    assert_eq!(got, want);
    assert_eq!(duplicated(&site), BTreeSet::new());
    assert_eq!(
        only_file(&env).1,
        id,
        "Pausieren/Fortsetzen darf keinen zweiten Crawl anlegen"
    );
}

// ---------------------------------------------------------------------------------------------------------
// Crawl-Verwaltung neben einem endenden MCP-Crawl (OUT-228, CRAWL-125, AGENT-020 Fremdprozess-Crawls)
// ---------------------------------------------------------------------------------------------------------

fn integrity_check(file: &Path) -> String {
    let f = open(file);
    f.conn()
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap()
}

/// Ruft `--list-crawls` wiederholt auf, solange `window` läuft (eine reine Leseoperation, OUT-185/OUT-228).
fn list_crawls_for(env: &Env, window: Duration) {
    let start = Instant::now();
    while start.elapsed() < window {
        let _ = env.run(&["--list-crawls", "--json"]);
    }
}

// spec: OUT-228
// spec: CRAWL-125
// spec: AGENT-020
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listing_crawls_while_an_mcp_crawl_ends_regularly_keeps_the_file_intact() {
    // Die App erkennt Crawls anderer Prozesse über die Crawl-Verwaltung (AGENT-020); das Auflisten ist lesend
    // und darf die Datei des laufenden/endenden Crawls nicht beschädigen (CRAWL-125: Datei konsistent).
    let site = site(FAST).await;
    let env = Env::new();
    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let id = mcp.start(&site, json!({"speed.threads": 1}));
    list_crawls_for(&env, Duration::from_secs(3));
    std::thread::sleep(Duration::from_secs(2));
    mcp.close();
    let (file, only_id) = only_file(&env);
    assert_eq!(only_id, id);
    assert_eq!(integrity_check(&file), "ok");
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "queue_empty", "{m:?}");
    assert_eq!(m["analysis_state"], "done", "{m:?}");
    assert_eq!(listed_status(&env, &id), "Completed");
}

// spec: OUT-228
// spec: CRAWL-125
// spec: UI-003
// spec: AGENT-020
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listing_crawls_while_an_mcp_stop_is_finishing_keeps_the_file_intact() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let id = mcp.start(&site, json!({"speed.threads": 1}));
    wait_until("6 Seitenabrufe", WAIT, || page_request_count(&site) >= 6);
    assert!(!mcp.call("stop_crawl", json!({"crawl_id": id})).is_error);
    list_crawls_for(&env, Duration::from_secs(3));
    std::thread::sleep(Duration::from_secs(2));
    mcp.close();
    let (file, _) = only_file(&env);
    assert_eq!(integrity_check(&file), "ok");
    let m = manifest(&file);
    assert_eq!(m["end_reason"], "stopped", "{m:?}");
    assert_eq!(m["analysis_state"], "done", "{m:?}");
    assert_eq!(listed_status(&env, &id), "Completed");
}

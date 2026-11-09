//! Crawl-Verwaltung neben einem Crawl eines anderen Prozesses (OUT-228, CRAWL-125, AGENT-020 Fremdprozess-Crawls).
//!
//! Die ersten beiden Tests sind unverändert aus den unabhängigen Abnahmetests (`m3/resume-tests`, Befund 1). Die übrigen
//! decken je Aufrufweg ab, wer neben einem laufenden oder endenden Crawl die Datei berühren kann (B4-LISTFIX).

mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use common::Env;
use os_store::{CrawlFile, OpenMode};
use os_testsite::{SiteOptions, TestSite};
use serde_json::{json, Value};

const SLOW: &str = "synthetic:n=40,links=4,seed=7,delay_ms=250";
const FAST: &str = "synthetic:n=40,links=4,seed=7";
const WAIT: Duration = Duration::from_secs(60);

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

// ---------------------------------------------------------------------------------------------------------
// Weitere Aufrufwege (B4-LISTFIX): je Weg der echte Prozess, geprüft wird die Datei am Ende.
// ---------------------------------------------------------------------------------------------------------

fn assert_finished_intact(env: &Env, id: &str, reason: &str) {
    let (file, only_id) = only_file(env);
    assert_eq!(only_id, id);
    assert_eq!(integrity_check(&file), "ok");
    let m = manifest(&file);
    assert_eq!(m["end_reason"], reason, "{m:?}");
    assert_eq!(m["analysis_state"], "done", "{m:?}");
    assert_eq!(listed_status(env, id), "Completed");
}

// Ein Agent pollt `crawl_status` (zweite Verbindung im Prozess des Crawls), daneben läuft die Verwaltung.
// spec: OUT-228
// spec: CRAWL-125
// spec: AGENT-020
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listing_crawls_while_a_running_mcp_crawl_is_polled_keeps_the_file_intact() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let id = mcp.start(&site, json!({"speed.threads": 1}));
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(6) {
        assert!(!mcp.call("crawl_status", json!({"crawl_id": id})).is_error);
        let _ = env.run(&["--list-crawls", "--json"]);
    }
    wait_until("Ende des Crawls", WAIT, || {
        manifest(&only_file(&env).0)["analysis_state"] == "done"
    });
    mcp.close();
    assert_finished_intact(&env, &id, "queue_empty");
}

// Zwei MCP-Clients (zwei `openspider mcp`-Prozesse auf demselben Ordner): `list_crawls` im zweiten.
// spec: AGENT-010
// spec: CRAWL-125
// spec: AGENT-020
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_crawls_in_a_second_mcp_server_keeps_the_file_of_a_crawl_in_the_first_intact() {
    let site = site(FAST).await;
    let env = Env::new();
    let mut first = spawn_mcp(&env, &["--allow", "crawl"]);
    let mut second = spawn_mcp(&env, &[]);
    let id = first.start(&site, json!({"speed.threads": 1}));
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        assert!(!second.call("list_crawls", json!({})).is_error);
    }
    std::thread::sleep(Duration::from_secs(2));
    second.close();
    first.close();
    assert_finished_intact(&env, &id, "queue_empty");
}

// `list_crawls` im selben Server wie der Crawl, kein zweiter Prozess.
// spec: AGENT-010
// spec: CRAWL-125
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_crawls_in_the_server_of_the_crawl_keeps_the_file_intact() {
    let site = site(FAST).await;
    let env = Env::new();
    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let id = mcp.start(&site, json!({"speed.threads": 1}));
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        assert!(!mcp.call("list_crawls", json!({})).is_error);
    }
    std::thread::sleep(Duration::from_secs(2));
    mcp.close();
    assert_finished_intact(&env, &id, "queue_empty");
}

async fn listing_next_to_a_cli_crawl(spec: &str, extra: &[&str]) {
    let site = site(spec).await;
    let env = Env::new();
    let mut args: Vec<String> = ["--crawl", &site.url("/"), "--set", "speed.threads=1"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    args.extend(extra.iter().map(|s| (*s).to_owned()));
    let mut cmd = env.command();
    cmd.args(args).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = cmd.spawn().expect("openspider startbar");
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(start.elapsed() < WAIT, "CLI-Crawl endet nicht");
        if !env.crawl_files().is_empty() {
            let _ = env.run(&["--list-crawls", "--json"]);
        }
    }
    assert_eq!(child.wait().unwrap().code(), Some(0));
    let (_, id) = only_file(&env);
    assert_finished_intact(&env, &id, "queue_empty");
}

// Geplante Aufgaben laufen als `openspider --run-task` (derselbe CLI-Crawl). Daneben eine zweite Verwaltung.
// spec: OUT-228
// spec: CRAWL-125
// spec: OUT-163
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listing_crawls_while_a_cli_crawl_runs_keeps_the_file_intact() {
    listing_next_to_a_cli_crawl(FAST, &[]).await;
}

// Mit `--json-progress` hält die CLI eine zweite Verbindung zum Zählen der Fehler (CRAWL-130).
// spec: OUT-228
// spec: CRAWL-125
// spec: CRAWL-130
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listing_crawls_while_a_cli_crawl_reports_json_progress_keeps_the_file_intact() {
    listing_next_to_a_cli_crawl(SLOW, &["--json-progress"]).await;
}

// Der Lauf in Server A ist beendet, ein anderer Prozess hat den Crawl fortgesetzt und beendet: A meldet die Datei (Fable, A2).
// spec: AGENT-020
// spec: CRAWL-130
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crawl_status_follows_the_file_after_another_process_finished_the_crawl() {
    let site = site(SLOW).await;
    let env = Env::new();
    let mut mcp = spawn_mcp(&env, &["--allow", "crawl"]);
    let id = mcp.start(&site, json!({"speed.threads": 1}));
    wait_until("3 Seitenabrufe", WAIT, || page_request_count(&site) >= 3);
    assert!(!mcp.call("pause_crawl", json!({"crawl_id": id})).is_error);
    wait_until("Pausiert in A", WAIT, || {
        manifest(&only_file(&env).0)["completeness"] == "paused"
    });
    let status = mcp.call("crawl_status", json!({"crawl_id": id}));
    assert_eq!(status.summary()["state"], "Paused");

    let run = env
        .run_async(vec![
            "--load-crawl".to_string(),
            id.clone(),
            "--resume".into(),
        ])
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert_eq!(listed_status(&env, &id), "Completed");

    let status = mcp.call("crawl_status", json!({"crawl_id": id}));
    let summary = status.summary();
    assert_eq!(summary["state"], "Completed", "{summary}");
    assert!(
        summary.get("progress").is_none(),
        "kein veralteter Fortschritt: {summary}"
    );
    mcp.close();
}

//! Unabhängige Abnahme M4-K11 (os-mcp): `stop_crawl`, `pause_crawl` und `resume_crawl` bei einem aktiven Besitzer
//! außerhalb von `Running`; `list_crawls` behält den Zustand.
//!
//! Erwartetes Verhalten ausschließlich aus 08 AI-020, Nachtrag 26.11.2026, Unterpunkt „Aktiver Besitzer außerhalb
//! von `Running`“ (Wirkung b: „MCP `stop_crawl` mit dem Fehler ohne `busy` nach CRAWL-125“; Kriterium 2), 01
//! CRAWL-125, Nachtrag K4-22 („die MCP-Werkzeuge `pause_crawl` und `stop_crawl` antworten bei einem in einem anderen
//! Prozess laufenden Crawl mit einem MCP-Fehler, der `owner_kind` und `owner_pid` nennt, **ohne** den Code `busy`“)
//! und 10 AGENT-020 (`busy`: Text beginnt mit `busy:`, `structuredContent.error.code`). Echter MCP-Prozess über
//! STDIO; der Server liest die Systemuhr, der simulierte Heartbeat liegt 60 s in der Zukunft bzw. 11 s zurück.

#[path = "../../os-store/tests/acceptance_k11_support/mod.rs"]
mod k11;

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use k11::*;
use os_model::{Clock, CrawlState, SystemClock};
use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_openspider-mcp");
const WAIT: Duration = Duration::from_secs(60);

struct Server {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
    next_id: u64,
    _home: TempDir,
    crawls: PathBuf,
}

#[derive(Debug)]
struct Reply {
    is_error: bool,
    text: String,
    structured: Value,
}

impl Reply {
    fn assert_error_without_busy(&self, label: &str) {
        assert!(self.is_error, "{label}: kein Fehler: {}", self.text);
        assert!(
            !self.text.starts_with("busy:"),
            "{label}: busy: {}",
            self.text
        );
        assert_ne!(
            self.structured["error"]["code"], "busy",
            "{label}: structuredContent {}",
            self.structured
        );
    }
}

impl Server {
    fn start() -> Server {
        let home = TempDir::new("mcp");
        let crawls = home.path().join("crawls");
        std::fs::create_dir_all(&crawls).unwrap();
        let mut child = Command::new(BIN)
            .env("OPENSPIDER_HOME", home.path())
            .env("OPENSPIDER_SECRET_STORE", "memory")
            .env_remove("OPENSPIDER_LOG")
            .env_remove("RUST_LOG")
            .arg("--crawl-dir")
            .arg(&crawls)
            .args(["--allow", "crawl"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("openspider-mcp startbar");
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
        let mut s = Server {
            child,
            stdin,
            rx,
            next_id: 1,
            _home: home,
            crawls,
        };
        let init = s.request(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "abnahme-k11", "version": "0"}}),
        );
        assert!(init.get("result").is_some(), "initialize: {init}");
        s.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        s
    }

    fn send(&mut self, v: &Value) {
        writeln!(self.stdin, "{v}").unwrap();
        self.stdin.flush().unwrap();
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
            let v: Value = serde_json::from_str(&line).unwrap();
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
            .map(|b| {
                b.iter()
                    .filter_map(|x| x["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Reply {
            is_error: result["isError"] == json!(true),
            text,
            structured: result
                .get("structuredContent")
                .cloned()
                .unwrap_or(Value::Null),
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn future() -> os_model::Timestamp {
    ago(SystemClock.now(), -60_000)
}

fn find_state(v: &Value, id: &str) -> Option<String> {
    match v {
        Value::Object(m) => {
            if m.get("crawl_id").and_then(Value::as_str) == Some(id) {
                if let Some(s) = m.get("state").and_then(Value::as_str) {
                    return Some(s.to_string());
                }
            }
            m.values().find_map(|x| find_state(x, id))
        }
        Value::Array(a) => a.iter().find_map(|x| find_state(x, id)),
        _ => None,
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// spec: AGENT-020
// acceptance: independent
#[test]
fn stop_and_pause_on_a_foreign_active_owner_fail_with_kind_and_pid_without_busy() {
    // Kriterium (f) und Wirkung (b) für `Completed` und `Paused`.
    let mut s = Server::start();
    for (i, (state, kind)) in [
        (CrawlState::Completed, "cli"),
        (CrawlState::Paused, "mcp-stdio"),
    ]
    .into_iter()
    .enumerate()
    {
        let (id, path) = new_crawl(&s.crawls, 10 + i as u64, state);
        set_owner(&path, kind, FOREIGN_PID, Some(future()));
        let before = meta_dump(&path);
        let id = id.to_string();
        for tool in ["stop_crawl", "pause_crawl"] {
            let r = s.call(tool, json!({"crawl_id": id}));
            let label = format!("{tool} auf {state:?}");
            r.assert_error_without_busy(&label);
            assert!(r.text.contains(FOREIGN_PID), "{label}: {}", r.text);
            assert!(r.text.contains(kind), "{label}: {}", r.text);
            assert_eq!(meta_dump(&path), before, "{label}: Datei unverändert");
        }
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// spec: AGENT-020
// acceptance: independent
#[test]
fn resume_on_a_foreign_active_owner_is_rejected_as_still_running_without_busy() {
    // Kriterium 2: „Fortsetzen (`resume_crawl`, …) … mit „läuft noch“ abgewiesen“.
    let mut s = Server::start();
    for (i, state) in [CrawlState::Completed, CrawlState::Paused]
        .into_iter()
        .enumerate()
    {
        let (id, path) = new_crawl(&s.crawls, 20 + i as u64, state);
        set_owner(&path, "cli", FOREIGN_PID, Some(future()));
        let before = meta_dump(&path);
        let r = s.call("resume_crawl", json!({"crawl_id": id.to_string()}));
        let label = format!("resume_crawl auf {state:?}");
        r.assert_error_without_busy(&label);
        assert!(r.text.contains("läuft noch"), "{label}: {}", r.text);
        assert!(r.text.contains(FOREIGN_PID), "{label}: {}", r.text);
        assert_eq!(meta_dump(&path), before, "{label}");
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// spec: AGENT-020
// acceptance: independent
#[test]
fn stopping_succeeds_eleven_seconds_after_the_last_foreign_heartbeat_and_with_the_own_pid() {
    // Kriterium 2 („11 s später ohne neuen Heartbeat“) und „(b) Ist `owner_pid` nicht der eigene Prozess …“.
    let mut s = Server::start();
    let (stale, stale_path) = new_crawl(&s.crawls, 30, CrawlState::Paused);
    set_owner(
        &stale_path,
        "cli",
        FOREIGN_PID,
        Some(ago(SystemClock.now(), 11_000)),
    );
    let r = s.call("stop_crawl", json!({"crawl_id": stale.to_string()}));
    assert!(!r.is_error, "{}", r.text);
    assert_eq!(get(&stale_path, "end_reason").as_deref(), Some("stopped"));
    assert_eq!(get(&stale_path, "owner_pid").as_deref(), Some(""));

    let own = s.pid().to_string();
    let (mine, mine_path) = new_crawl(&s.crawls, 31, CrawlState::Paused);
    set_owner(&mine_path, "mcp-stdio", &own, Some(future()));
    let r = s.call("stop_crawl", json!({"crawl_id": mine.to_string()}));
    assert!(!r.is_error, "eigene PID: {}", r.text);
    assert_eq!(get(&mine_path, "end_reason").as_deref(), Some("stopped"));
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn list_crawls_keeps_the_stored_state_for_an_active_owner_and_crawl_125_for_files_from_1_0() {
    let mut s = Server::start();
    let mut expected = Vec::new();
    for (i, state) in [CrawlState::Completed, CrawlState::Paused]
        .into_iter()
        .enumerate()
    {
        let (id, path) = new_crawl(&s.crawls, 40 + i as u64, state);
        set_owner(&path, "cli", FOREIGN_PID, Some(future()));
        expected.push((id.to_string(), state, "aktiver Besitzer".to_string()));
    }
    let (_, cases) = v10_cases();
    assert_fixture_consistent(&cases);
    let now = SystemClock.now();
    for (i, case) in cases.iter().enumerate() {
        let Some(case) = case.for_system_clock() else {
            continue;
        };
        let (id, _) = case.build(&s.crawls, 100 + i as u64, now);
        expected.push((id.to_string(), case.oracle_state(), case.name.clone()));
    }
    let r = s.call("list_crawls", json!({"limit": 500}));
    assert!(!r.is_error, "{}", r.text);
    let v: Value = serde_json::from_str(&r.text).unwrap_or_else(|e| panic!("{e}: {}", r.text));
    for (id, state, label) in expected {
        assert_eq!(
            find_state(&v, &id).as_deref(),
            Some(state.as_str()),
            "{label}"
        );
    }
}

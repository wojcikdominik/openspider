//! Unabhängige Abnahmetests M4-F02: Schalter „KI (Vorschau)“ im MCP-Server `openspider-mcp` (AGENT-030).
//! acceptance: independent
//!
//! Erwartungen ausschließlich aus `docs/spec/10-agent-interface.md` AGENT-030 (OS-Festlegung Schalter „KI (Vorschau)“,
//! erstes Akzeptanzkriterium), AGENT-003 (Klasse `cost`) und `docs/spec/schemas/settings.schema.json`
//! (`ai_app.preview_enabled`, Standard `false`). Über den Prozess von außen: JSON-RPC über STDIO.
//!
//! Den Mock-Zähler 0 aus AGENT-030 prüft dieser Test nicht direkt; ohne Schalter muss der Aufruf schon an der
//! Werkzeugauswahl scheitern (gleiche Antwort wie ein unbekanntes Werkzeug), bevor ein Anbieter in Betracht kommt.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_openspider-mcp");
const WAIT: Duration = Duration::from_secs(60);
const AI_TOOLS: [&str; 4] = [
    "run_decisions",
    "suggest_metadata",
    "redirect_mapping",
    "internal_link_suggestions",
];
const UNKNOWN: &str = "gibt_es_nicht_f02";

/// Legt `settings.toml` im Datenordner an; `preview` setzt den Schalter.
fn home_with(preview: Option<bool>) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    if let Some(on) = preview {
        let mut settings = os_model::settings::AppSettings::default();
        settings.ai_app.preview_enabled = on;
        settings.save(home.path()).unwrap();
    }
    home
}

/// Startet den Server mit `args`, schickt `initialize`, `tools/list` und die Aufrufe `calls` (Name, Argumente) und
/// liefert die Antworten nach id.
fn session(home: &Path, args: &[&str], calls: &[(&str, Value)]) -> BTreeMap<i64, Value> {
    let mut child = Command::new(BIN)
        .args(args)
        .env("OPENSPIDER_HOME", home)
        .env("OPENSPIDER_SECRET_STORE", "memory")
        .env_remove("OPENSPIDER_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("openspider-mcp startet");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut requests = vec![
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "f02-abnahme", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    ];
    for (i, (name, arguments)) in calls.iter().enumerate() {
        requests.push(
            json!({"jsonrpc": "2.0", "id": 10 + i as i64, "method": "tools/call",
            "params": {"name": name, "arguments": arguments}}),
        );
    }
    for r in &requests {
        writeln!(stdin, "{r}").unwrap();
    }
    stdin.flush().unwrap();
    let expected = 2 + calls.len();
    let mut by_id = BTreeMap::new();
    while by_id.len() < expected {
        let line = rx.recv_timeout(WAIT).expect("Antwort auf stdout");
        let v: Value = serde_json::from_str(&line).expect("JSON-RPC");
        if let Some(id) = v["id"].as_i64() {
            by_id.insert(id, v);
        }
    }
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    by_id
}

fn tool_names(list: &Value) -> Vec<String> {
    list["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list ohne tools: {list}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

/// Antwort eines Aufrufs in vergleichbarer Form: Werkzeugname durch Platzhalter ersetzt.
fn normalized(answer: &Value, name: &str) -> String {
    answer
        .get("result")
        .or_else(|| answer.get("error"))
        .unwrap_or(answer)
        .to_string()
        .replace(name, "<TOOL>")
}

// spec: AGENT-030
// acceptance: independent
// Schalter aus (keine Einstellungsdatei = Standard): auch mit `--allow cost` nennt `tools/list` keines der vier
// KI-Werkzeuge.
#[test]
fn switch_off_by_default_hides_the_four_ai_tools_even_with_cost_allowed() {
    let home = home_with(None);
    let answers = session(home.path(), &["--allow", "cost"], &[]);
    let names = tool_names(&answers[&2]);
    for tool in AI_TOOLS {
        assert!(
            !names.iter().any(|n| n == tool),
            "{tool} in tools/list bei Schalter aus: {names:?}"
        );
    }
}

// spec: AGENT-030
// acceptance: independent
// Schalter an: `tools/list` enthält alle vier; Schalter aus: alle übrigen Werkzeuge unverändert.
#[test]
fn switch_on_lists_all_four_and_off_keeps_every_other_tool() {
    let on_home = home_with(Some(true));
    let off_home = home_with(Some(false));
    let on = tool_names(&session(on_home.path(), &["--allow", "cost"], &[])[&2]);
    let off = tool_names(&session(off_home.path(), &["--allow", "cost"], &[])[&2]);
    for tool in AI_TOOLS {
        assert_eq!(
            on.iter().filter(|n| *n == tool).count(),
            1,
            "{tool} genau einmal bei Schalter an: {on:?}"
        );
    }
    let mut expected: Vec<String> = on
        .iter()
        .filter(|n| !AI_TOOLS.contains(&n.as_str()))
        .cloned()
        .collect();
    let mut got = off.clone();
    expected.sort();
    got.sort();
    assert_eq!(got, expected, "übrige Werkzeuge müssen unverändert bleiben");
}

// spec: AGENT-030
// acceptance: independent
// Schalter aus: ein Aufruf eines KI-Werkzeugs (auch mit `confirm: true`) liefert den Fehler eines unbekannten
// Werkzeugs – dieselbe Antwort wie für einen Namen, den es nie gab.
#[test]
fn switch_off_rejects_ai_tool_calls_like_an_unknown_tool() {
    let home = home_with(Some(false));
    let mut calls: Vec<(&str, Value)> = vec![(UNKNOWN, json!({}))];
    for tool in AI_TOOLS {
        calls.push((tool, json!({ "crawl_id": "x", "confirm": true })));
    }
    let answers = session(home.path(), &["--allow", "cost"], &calls);
    let unknown = &answers[&10];
    assert!(
        unknown["result"]["isError"] == json!(true) || unknown.get("error").is_some(),
        "unbekanntes Werkzeug muss abgelehnt werden: {unknown}"
    );
    let reference = normalized(unknown, UNKNOWN);
    for (i, tool) in AI_TOOLS.iter().enumerate() {
        let answer = &answers[&(11 + i as i64)];
        assert_eq!(
            normalized(answer, tool),
            reference,
            "{tool} bei Schalter aus muss wie ein unbekanntes Werkzeug abgelehnt werden"
        );
    }
}

// spec: AGENT-030
// acceptance: independent
// Gegenprobe: mit Schalter an ist ein KI-Werkzeug kein unbekanntes Werkzeug mehr (die Antwort unterscheidet sich
// von der für einen unbekannten Namen).
#[test]
fn switch_on_ai_tools_are_known_tools() {
    let home = home_with(Some(true));
    let mut calls: Vec<(&str, Value)> = vec![(UNKNOWN, json!({}))];
    for tool in AI_TOOLS {
        calls.push((tool, json!({ "crawl_id": "x" })));
    }
    let answers = session(home.path(), &["--allow", "cost"], &calls);
    let reference = normalized(&answers[&10], UNKNOWN);
    for (i, tool) in AI_TOOLS.iter().enumerate() {
        let answer = &answers[&(11 + i as i64)];
        assert_ne!(
            normalized(answer, tool),
            reference,
            "{tool} bei Schalter an wie ein unbekanntes Werkzeug abgelehnt: {answer}"
        );
    }
}

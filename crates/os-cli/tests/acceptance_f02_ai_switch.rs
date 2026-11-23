//! Unabhängige Abnahmetests M4-F02: Schalter „KI (Vorschau)“ in der CLI (Hilfe, Namenslisten) und im MCP-Server
//! `openspider mcp`.
//! acceptance: independent
//!
//! Erwartungen ausschließlich aus `docs/plan/M4.md` Abschnitt 2 E1 (E-2 b: mit Schalter aus erscheint keine
//! KI-Oberfläche), `docs/spec/10-agent-interface.md` AGENT-030 (Schalter, gilt für den Server der App und für
//! `openspider-mcp` gleichermaßen), `docs/spec/05-ui.md` UI-004 (Namen der KI-Tabs), `docs/spec/06-output-ops.md`
//! OUT-201 und `docs/spec/08-ai.md` AI-022 (KI-Optionen der CLI) sowie `docs/spec/schemas/settings.schema.json`.
//!
//! Auslegung (siehe Bericht): „keine KI-Oberfläche“ gilt für die CLI als „Hilfe und Namenslisten nennen keine
//! KI-Einträge“. `--api-budget` ist das gemeinsame Budget aller bezahlten APIs (KI und DataForSEO) und gilt nicht als
//! reine KI-Option.

mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;

use common::Env;
use serde_json::{json, Value};

/// KI-Tabs nach UI-004 [M4].
const AI_TABS: [&str; 4] = ["AI", "AI Decisions", "AI Suggestions", "AI Search"];

/// KI-Optionen nach OUT-201 [M4] / AI-022.
const AI_FLAGS: [&str; 9] = [
    "--use-ai",
    "--use-embeddings",
    "--ai-tasks",
    "--ai-questions",
    "--ai-budget",
    "--ai-consent",
    "--ai-batch",
    "--ai-reuse-cache",
    "--export-query",
];

const AI_TOOLS: [&str; 4] = [
    "run_decisions",
    "suggest_metadata",
    "redirect_mapping",
    "internal_link_suggestions",
];

/// Umgebung mit Schalter „KI (Vorschau)“ wie angegeben (Sprache Englisch wie in `Env::new`).
fn env_with(preview: bool) -> Env {
    let env = Env::new();
    let mut settings = os_model::settings::AppSettings::default();
    settings.general.language = os_model::UiLanguage::En;
    settings.ai_app.preview_enabled = preview;
    settings.save(env.home.path()).unwrap();
    env
}

/// Erster Namensteil vor `:` (oder der ganze Name).
fn tab_part(line: &str) -> &str {
    line.split_once(':').map_or(line, |(t, _)| t).trim()
}

fn ai_lines(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| AI_TABS.contains(&tab_part(l)))
        .map(str::to_string)
        .collect()
}

/// Ob `help` die Option `flag` als eigenes Wort nennt (nicht als Präfix einer längeren Option).
fn mentions_flag(help: &str, flag: &str) -> bool {
    help.match_indices(flag).any(|(i, _)| {
        let next = help[i + flag.len()..].chars().next();
        !matches!(next, Some(c) if c.is_ascii_alphanumeric() || c == '-')
    })
}

// spec: UI-004 [M4]
// acceptance: independent
// Schalter aus (Standard, `Env::new` schreibt ihn nicht): keine Namensliste nennt einen KI-Tab oder einen Eintrag
// eines KI-Tabs.
#[test]
fn switch_off_lists_contain_no_ai_entries() {
    for env in [Env::new(), env_with(false)] {
        for args in [
            vec!["--list-exports"],
            vec!["--list-reports"],
            vec!["--help", "export-tabs"],
            vec!["--help", "bulk-export"],
            vec!["--help", "save-report"],
            vec!["--help", "export-custom-summary"],
        ] {
            let run = env.run(&args);
            assert_eq!(run.code, 0, "{args:?}: {}", run.dump());
            let found = ai_lines(&run.stdout);
            assert!(
                found.is_empty(),
                "{args:?} nennt bei Schalter aus KI-Einträge: {found:?}"
            );
        }
    }
}

// spec: UI-004 [M4]
// acceptance: independent
// Schalter an: jeder der vier KI-Tabs ist als Export-Tab angeboten (`--help export-tabs`, `--list-exports`).
#[test]
fn switch_on_lists_offer_every_ai_tab() {
    let env = env_with(true);
    for args in [vec!["--help", "export-tabs"], vec!["--list-exports"]] {
        let run = env.run(&args);
        assert_eq!(run.code, 0, "{}", run.dump());
        let found = ai_lines(&run.stdout);
        for tab in AI_TABS {
            assert!(
                found.iter().any(|l| tab_part(l) == tab),
                "{args:?}: kein Eintrag für {tab}; KI-Einträge: {found:?}"
            );
        }
    }
}

// spec: UI-004 [M4]
// acceptance: independent
// Schalter aus: die Gesamthilfe nennt keine KI-Option.
#[test]
fn switch_off_general_help_names_no_ai_option() {
    let env = env_with(false);
    let run = env.run(&["--help"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    let shown: Vec<&str> = AI_FLAGS
        .iter()
        .copied()
        .filter(|f| mentions_flag(&run.stdout, f))
        .collect();
    assert!(
        shown.is_empty(),
        "KI-Optionen in --help bei Schalter aus: {shown:?}"
    );
}

// spec: UI-004 [M4]
// acceptance: independent
// Schalter an: die Gesamthilfe nennt die KI-Optionen aus OUT-201/AI-022.
#[test]
fn switch_on_general_help_names_the_ai_options() {
    let env = env_with(true);
    let run = env.run(&["--help"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    let missing: Vec<&str> = AI_FLAGS
        .iter()
        .copied()
        .filter(|f| !mentions_flag(&run.stdout, f))
        .collect();
    assert!(
        missing.is_empty(),
        "fehlende KI-Optionen in --help bei Schalter an: {missing:?}"
    );
}

/// `openspider mcp <args>`: initialize, tools/list und Aufrufe; Antworten nach id.
fn mcp_session(env: &Env, args: &[&str], calls: &[(&str, Value)]) -> BTreeMap<i64, Value> {
    let mut cmd = env.command();
    cmd.arg("mcp")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().expect("openspider mcp startet");
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
    let mut by_id = BTreeMap::new();
    while by_id.len() < 2 + calls.len() {
        let line = rx
            .recv_timeout(Duration::from_secs(60))
            .expect("Antwort auf stdout");
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
// `openspider mcp --allow cost`: Schalter aus → keines der vier KI-Werkzeuge in `tools/list`, übrige Werkzeuge wie
// bei Schalter an; Schalter an → alle vier.
#[test]
fn openspider_mcp_follows_the_switch_in_tools_list() {
    let on = tool_names(&mcp_session(&env_with(true), &["--allow", "cost"], &[])[&2]);
    let off = tool_names(&mcp_session(&env_with(false), &["--allow", "cost"], &[])[&2]);
    for tool in AI_TOOLS {
        assert!(
            on.iter().any(|n| n == tool),
            "{tool} fehlt bei Schalter an: {on:?}"
        );
        assert!(
            !off.iter().any(|n| n == tool),
            "{tool} bei Schalter aus: {off:?}"
        );
    }
    let mut expected: Vec<String> = on
        .into_iter()
        .filter(|n| !AI_TOOLS.contains(&n.as_str()))
        .collect();
    let mut got = off;
    expected.sort();
    got.sort();
    assert_eq!(got, expected, "übrige Werkzeuge müssen unverändert bleiben");
}

// spec: AGENT-030
// acceptance: independent
// `openspider mcp --allow cost`, Schalter aus: KI-Werkzeug-Aufruf (auch mit `confirm: true`) = Antwort für ein
// unbekanntes Werkzeug.
#[test]
fn openspider_mcp_rejects_ai_tools_like_unknown_ones_when_off() {
    let env = env_with(false);
    let unknown = "gibt_es_nicht_f02";
    let mut calls: Vec<(&str, Value)> = vec![(unknown, json!({}))];
    for tool in AI_TOOLS {
        calls.push((tool, json!({ "crawl_id": "x", "confirm": true })));
    }
    let answers = mcp_session(&env, &["--allow", "cost"], &calls);
    let reference = normalized(&answers[&10], unknown);
    assert!(
        answers[&10]["result"]["isError"] == json!(true) || answers[&10].get("error").is_some(),
        "{}",
        answers[&10]
    );
    for (i, tool) in AI_TOOLS.iter().enumerate() {
        assert_eq!(
            normalized(&answers[&(11 + i as i64)], tool),
            reference,
            "{tool} bei Schalter aus muss wie ein unbekanntes Werkzeug abgelehnt werden"
        );
    }
}

// --- OUT-203: ausdrücklich genannte KI-Namen (OS-Festlegung Schalter „KI (Vorschau)“, Nutzerentscheidung 23.11.2026,
// Wortlaut vom Orchestrator übermittelt; in diesem Zweig noch nicht in docs/spec) ---

/// Themen von `--help <thema>` und die zugehörige Option. `--export-custom-summary` hat eigene Tests: Klarstellung in
/// OUT-203 (23.11.2026) – solange die Option insgesamt nicht unterstützt wird (OUT-200), endet sie immer mit Exit-Code 2.
const TOPICS: [(&str, &str); 3] = [
    ("export-tabs", "--export-tabs"),
    ("bulk-export", "--bulk-export"),
    ("save-report", "--save-report"),
];
const SUMMARY: [(&str, &str); 1] = [("export-custom-summary", "--export-custom-summary")];

/// Je Thema ein KI-Name aus der Liste bei eingeschaltetem Schalter; dazu `AI:All` (Beispiel der Festlegung).
fn ai_names_per_option() -> Vec<(&'static str, String)> {
    let mut out = vec![("--export-tabs", "AI:All".to_string())];
    out.extend(ai_names_for(&TOPICS));
    out
}

fn ai_names_for(topics: &[(&str, &'static str)]) -> Vec<(&'static str, String)> {
    let env = env_with(true);
    let mut out = Vec::new();
    for &(topic, flag) in topics {
        let run = env.run(&["--help", topic]);
        assert_eq!(run.code, 0, "{}", run.dump());
        if let Some(name) = ai_lines(&run.stdout).into_iter().find(|n| n != "AI:All") {
            out.push((flag, name));
        }
    }
    out
}

/// Nennt der Text (ohne die zitierte Eingabe) einen KI-Namen – als Listenzeile oder als zitierten Vorschlag?
fn names_ai_besides(text: &str, input: &str) -> Vec<String> {
    let stripped = text.replace(input, "<INPUT>");
    let mut hits: Vec<String> = ai_lines(&stripped);
    for tab in AI_TABS {
        for quoted in [format!("'{tab}:"), format!("'{tab}'")] {
            if stripped.contains(&quoted) {
                hits.push(quoted);
            }
        }
    }
    hits
}

fn crawl_args(site: &os_testsite::TestSite, env: &Env, flag: &str, name: &str) -> Vec<String> {
    vec![
        "--crawl".into(),
        "http://site.test/".into(),
        "--resolve".into(),
        common::resolve(site),
        "--output-folder".into(),
        env.out().to_string_lossy().into_owned(),
        flag.into(),
        name.into(),
    ]
}

// spec: OUT-203
// acceptance: independent
// Schalter aus: ein ausdrücklich genannter KI-Name (Tab-Export, Bulk-Export, Report, Kennzahl) → Exit-Code 2 vor dem
// Crawl (0 Requests an die Testsite) mit der Meldung für einen unbekannten Namen; weder Vorschlag noch Liste nennen
// einen KI-Namen.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switch_off_explicit_ai_names_are_unknown_and_rejected_before_the_crawl() {
    check_off(ai_names_per_option()).await;
}

async fn check_off(names: Vec<(&'static str, String)>) {
    assert!(!names.is_empty(), "keine KI-Namen bei Schalter an gelistet");
    let site = common::site("out-basic-site").await;
    let mut failures = Vec::new();
    for (flag, name) in &names {
        let env = env_with(false);
        site.reset_request_log();
        let run = env.run_async(crawl_args(&site, &env, flag, name)).await;
        let requests = site.requests().len();
        let unknown_line = run
            .stderr
            .lines()
            .any(|l| l.contains("unknown ") && l.contains(&format!("'{name}'")));
        let leaked = names_ai_besides(&run.stderr, &format!("'{name}'"));
        if run.code != 2 || !unknown_line || requests != 0 || !leaked.is_empty() {
            failures.push(format!(
                "{flag} {name:?}: exit {} (erwartet 2), Meldung „unknown … '{name}'“: {unknown_line}, \
                 Requests: {requests}, KI-Namen in Vorschlag/Liste: {leaked:?}\n{}",
                run.code,
                run.dump()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

// spec: OUT-203
// acceptance: independent
// Schalter an: dieselben Namen sind gültig – keine Meldung für einen unbekannten Namen, kein Exit-Code 2.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switch_on_explicit_ai_names_are_valid() {
    check_on(ai_names_per_option()).await;
}

async fn check_on(names: Vec<(&'static str, String)>) {
    assert!(!names.is_empty(), "keine KI-Namen bei Schalter an gelistet");
    let site = common::site("out-basic-site").await;
    let mut failures = Vec::new();
    for (flag, name) in &names {
        let env = env_with(true);
        let run = env.run_async(crawl_args(&site, &env, flag, name)).await;
        let unknown_line = run
            .stderr
            .lines()
            .any(|l| l.contains("unknown ") && l.contains(&format!("'{name}'")));
        if run.code == 2 || unknown_line {
            failures.push(format!(
                "{flag} {name:?} bei Schalter an abgelehnt\n{}",
                run.dump()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

// spec: OUT-203
// acceptance: independent
// Wie oben für `--export-custom-summary` (Kennzahlen; Klarstellung OUT-203 vom 23.11.2026): Schalter aus → KI-Kennzahl
// unbekannt, Exit-Code 2 mit der Meldung für einen unbekannten Namen (nicht „noch nicht unterstützt“), 0 Requests.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switch_off_explicit_ai_summary_metric_is_unknown() {
    check_off(ai_names_for(&SUMMARY)).await;
}

// spec: OUT-203
// acceptance: independent
// Klarstellung OUT-203 (23.11.2026): solange `--export-custom-summary` nicht unterstützt wird (OUT-200), endet die
// Option bei eingeschaltetem Schalter auch mit KI-Kennzahl mit Exit-Code 2 und der Meldung, dass die Option noch nicht
// unterstützt wird – nicht mit der Meldung für einen unbekannten Namen; vor dem Crawl (0 Requests).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switch_on_ai_summary_metric_ends_with_not_supported() {
    let names = ai_names_for(&SUMMARY);
    assert!(
        !names.is_empty(),
        "keine KI-Kennzahl bei Schalter an gelistet"
    );
    let site = common::site("out-basic-site").await;
    let mut failures = Vec::new();
    for (flag, name) in &names {
        let env = env_with(true);
        site.reset_request_log();
        let run = env.run_async(crawl_args(&site, &env, flag, name)).await;
        let requests = site.requests().len();
        let unknown_line = run
            .stderr
            .lines()
            .any(|l| l.contains("unknown ") && l.contains(&format!("'{name}'")));
        let not_supported = run.stderr.lines().any(|l| {
            l.contains("--export-custom-summary")
                && (l.contains("nicht unterstützt") || l.to_lowercase().contains("not supported"))
        });
        if run.code != 2 || unknown_line || !not_supported || requests != 0 {
            failures.push(format!(
                "{flag} {name:?}: exit {} (erwartet 2), Meldung „nicht unterstützt“: {not_supported}, \
                 Meldung „unknown“: {unknown_line}, Requests: {requests}\n{}",
                run.code,
                run.dump()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

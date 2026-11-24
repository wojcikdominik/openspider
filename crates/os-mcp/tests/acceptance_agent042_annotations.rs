//! Unabhängige Abnahmetests AGENT-042, Nachtrag 24.11.2026, viertes Kriterium: Werkzeug-Annotationen
//! (`title`, `readOnlyHint`, `destructiveHint`) und englische Werkzeugbeschreibungen/Server-Hinweise.
//! acceptance: independent
//!
//! Erwartungen ausschließlich aus `docs/spec/10-agent-interface.md` (AGENT-001/-003/-004/-020/-021/-030/-042 und
//! den Klassenangaben der lesenden Werkzeuge AGENT-010 … -019), `docs/spec/06-*.md` OUT-285 und den
//! Festlegungen des Orchestrators (Freigabeklassen unverändert, `crawl_status` bleibt Klasse `crawl`,
//! Laufzeit-Fehlermeldungen nicht Teil der Übersetzung, Marker `<<<UNTRUSTED_CONTENT>>>` wörtlich).
//! Die Einstufung der Werkzeuge (lesend / verändernd / unwiderruflich) ist hier selbst aus der Spec abgeleitet
//! (Tabellen [`READ_ONLY`], [`CHANGING`], [`MUST_BE_DESTRUCTIVE`]); bei `export` entscheidet das beobachtete
//! Verhalten (überschreibt es eine vorhandene Datei?).
//!
//! Die meisten Tests sprechen das Protokoll von außen mit dem Starter `openspider-mcp` (STDIO, rmcp-Client) und
//! lesen die Werkzeuge als JSON, wie ein Client sie sieht (`annotations.readOnlyHint` usw.).

#[path = "tools_support.rs"]
mod support;

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use os_mcp::cli::{build_server, McpArgs};
use os_mcp::tools;
use os_model::settings::AppSettings;
use os_store::paths::app_paths_with_home;
use rmcp::model::{CallToolRequestParams, ReadResourceRequestParams, ResourceContents};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{json, Value};
use support::{base_config, World};

const BIN: &str = env!("CARGO_BIN_EXE_openspider-mcp");
const WAIT: Duration = Duration::from_secs(180);
const MARKER: &str = "<<<UNTRUSTED_CONTENT>>>";

/// Nur lesende Werkzeuge laut Spec: AGENT-010 (`list_crawls`, `crawl_overview`), AGENT-011 (`query_urls`),
/// AGENT-012 (`url_details`), AGENT-013 (`list_issues`), AGENT-014 (`sql_query`, schreibgeschützte
/// Verbindung), AGENT-015 (`compare_crawls`, „Klasse `read`“), AGENT-016 (`list_catalog`), AGENT-017
/// (`url_content`, „ausschließlich aus der Crawl-Datei“), AGENT-018 (`url_screenshot`, „kein erneutes Rendern“)
/// und AGENT-020 `crawl_status` (liefert nur Fortschritt; AGENT-042 nennt es ausdrücklich).
const READ_ONLY: [&str; 11] = [
    "list_crawls",
    "crawl_overview",
    "query_urls",
    "url_details",
    "list_issues",
    "sql_query",
    "compare_crawls",
    "list_catalog",
    "url_content",
    "url_screenshot",
    "crawl_status",
];

/// Werkzeuge, die etwas starten, verändern, Kosten auslösen oder Dateien schreiben: AGENT-020 (`start_crawl`,
/// `pause_crawl`, `stop_crawl`, `resume_crawl`), AGENT-021 (`export` schreibt Dateien), AGENT-003 Klasse `cost`
/// (`fetch_link_metrics`, „derzeit Linkmetriken“) und AGENT-030 (die vier KI-Werkzeuge, Klasse `cost`).
const CHANGING: [&str; 10] = [
    "start_crawl",
    "pause_crawl",
    "stop_crawl",
    "resume_crawl",
    "export",
    "fetch_link_metrics",
    "run_decisions",
    "suggest_metadata",
    "redirect_mapping",
    "internal_link_suggestions",
];

/// `destructiveHint = true` verlangt AGENT-042 mindestens für `stop_crawl` (AGENT-020: „beendet den Crawl
/// endgültig“). `export` hängt vom Verhalten ab (eigener Test).
const MUST_BE_DESTRUCTIVE: [&str; 1] = ["stop_crawl"];

/// Klassen aus AGENT-003/-020/-021/-030 (Festlegung: `crawl_status` bleibt `crawl`).
const CRAWL_CLASS: [&str; 5] = [
    "start_crawl",
    "crawl_status",
    "pause_crawl",
    "stop_crawl",
    "resume_crawl",
];
const WRITE_CLASS: [&str; 1] = ["export"];
const COST_CLASS: [&str; 5] = [
    "fetch_link_metrics",
    "run_decisions",
    "suggest_metadata",
    "redirect_mapping",
    "internal_link_suggestions",
];

type Client = RunningService<RoleClient, ()>;

/// Datenordner mit `settings.toml`: Schalter „KI (Vorschau)“ und optional Exportordner.
fn home_with(preview: bool, export_dir: Option<&Path>) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let mut settings = AppSettings::default();
    settings.ai_app.preview_enabled = preview;
    if let Some(dir) = export_dir {
        settings.export.directory = Some(dir.to_string_lossy().into_owned());
    }
    settings.save(home.path()).unwrap();
    home
}

/// Startet `openspider-mcp` mit `args` und verbindet einen rmcp-Client über STDIO.
async fn spawn(home: &Path, crawls: &Path, args: &[&str]) -> Client {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let transport = TokioChildProcess::new(tokio::process::Command::new(BIN).configure(|cmd| {
        cmd.env("OPENSPIDER_HOME", home)
            .env("OPENSPIDER_SECRET_STORE", "memory")
            .env_remove("OPENSPIDER_LOG")
            .arg("--crawl-dir")
            .arg(crawls)
            .args(&args);
    }))
    .expect("openspider-mcp startet");
    tokio::time::timeout(WAIT, ().serve(transport))
        .await
        .expect("initialize rechtzeitig")
        .expect("initialize")
}

/// `tools/list` als JSON, wie es auf der Leitung steht.
async fn tools_json(client: &Client) -> Vec<Value> {
    tokio::time::timeout(WAIT, client.list_all_tools())
        .await
        .expect("tools/list rechtzeitig")
        .expect("tools/list")
        .iter()
        .map(|t| serde_json::to_value(t).unwrap())
        .collect()
}

fn names(tools: &[Value]) -> Vec<String> {
    tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

fn by_name(tools: &[Value]) -> BTreeMap<String, Value> {
    tools
        .iter()
        .map(|t| (t["name"].as_str().unwrap().to_string(), t.clone()))
        .collect()
}

/// Ergebnis eines Aufrufs: (abgelehnt/Fehler?, Textblöcke). Ein JSON-RPC-Fehler zählt ebenfalls als Fehler.
async fn call(client: &Client, name: &str, args: Value) -> (bool, Vec<String>) {
    let params = CallToolRequestParams::new(name.to_owned())
        .with_arguments(args.as_object().expect("Argumente als Objekt").clone());
    match tokio::time::timeout(WAIT, client.call_tool(params))
        .await
        .expect("tools/call rechtzeitig")
    {
        Ok(result) => {
            let wire = serde_json::to_value(&result).unwrap();
            let texts = wire["content"]
                .as_array()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| b["text"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            (wire["isError"] == json!(true), texts)
        }
        Err(e) => (true, vec![e.to_string()]),
    }
}

/// Erfolgreicher Aufruf (sonst Testabbruch mit der Meldung).
async fn call_ok(client: &Client, name: &str, args: Value) -> Vec<String> {
    let (failed, texts) = call(client, name, args.clone()).await;
    assert!(!failed, "{name}({args}) schlug fehl: {texts:?}");
    texts
}

/// Annotation `key` eines Werkzeugs als Wahrheitswert (Testabbruch, wenn sie fehlt oder kein bool ist).
fn hint(tool: &Value, key: &str) -> bool {
    tool["annotations"][key].as_bool().unwrap_or_else(|| {
        panic!(
            "{}: annotations.{key} fehlt oder ist kein Wahrheitswert: {}",
            tool["name"], tool["annotations"]
        )
    })
}

/// Häufige deutsche Wörter (ganze Wörter, klein geschrieben) als Stichprobe.
const GERMAN_WORDS: [&str; 46] = [
    "der",
    "die",
    "das",
    "des",
    "dem",
    "den",
    "und",
    "oder",
    "nicht",
    "mit",
    "wird",
    "werden",
    "ein",
    "eine",
    "einer",
    "einen",
    "für",
    "ist",
    "sind",
    "bei",
    "auf",
    "aus",
    "nach",
    "über",
    "zum",
    "zur",
    "vom",
    "von",
    "keine",
    "kein",
    "nur",
    "auch",
    "wenn",
    "sonst",
    "liefert",
    "gibt",
    "alle",
    "seite",
    "seiten",
    "werkzeug",
    "werkzeuge",
    "zu",
    "als",
    "wie",
    "bzw",
    "gekürzt",
];

/// Deutsche Spuren in `text`: Umlaute/ß und Wörter aus [`GERMAN_WORDS`].
fn german_traces(text: &str) -> Vec<String> {
    let mut hits: Vec<String> = text
        .chars()
        .filter(|c| "äöüÄÖÜß".contains(*c))
        .map(|c| c.to_string())
        .collect();
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let lower = word.to_lowercase();
        if GERMAN_WORDS.contains(&lower.as_str()) {
            hits.push(word.to_string());
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

/// Alle `description`- und `title`-Texte eines Eingabeschemas, auch verschachtelt (Pfad, Text).
fn schema_texts(value: &Value, path: &str, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let here = format!("{path}/{k}");
                if (k == "description" || k == "title") && v.is_string() {
                    out.push((here.clone(), v.as_str().unwrap().to_string()));
                }
                schema_texts(v, &here, out);
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                schema_texts(v, &format!("{path}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// Vollständige Freigabe aller Klassen; `preview` schaltet „KI (Vorschau)“.
async fn full_listing(preview: bool) -> Vec<Value> {
    let home = home_with(preview, None);
    let crawls = tempfile::tempdir().unwrap();
    let client = spawn(home.path(), crawls.path(), &["--allow", "crawl,write,cost"]).await;
    let tools = tools_json(&client).await;
    client.cancel().await.unwrap();
    tools
}

// spec: AGENT-042
// spec: AGENT-030
// acceptance: independent
// Prüfpunkt 1: Bei voller Freigabe (mit und ohne Schalter „KI (Vorschau)“) hat jedes Werkzeug
// `annotations.title` (nicht leer, englisch), `readOnlyHint` und `destructiveHint` als Wahrheitswerte; jedes
// gelistete Werkzeug ist in dieser Abnahme eingestuft, und mit Schalter an erscheinen alle 21.
#[tokio::test]
async fn every_tool_carries_title_read_only_and_destructive_hints() {
    for preview in [false, true] {
        let tools = full_listing(preview).await;
        assert!(!tools.is_empty());
        let mut problems = Vec::new();
        for tool in &tools {
            let name = tool["name"].as_str().unwrap();
            // Protokoll für den Abnahmebericht: gemeldete Werte je Werkzeug.
            eprintln!("ANNOT preview={preview} {name}: {}", tool["annotations"]);
            if !READ_ONLY.contains(&name) && !CHANGING.contains(&name) {
                problems.push(format!(
                    "{name}: nicht eingestuft (Abnahme kennt das Werkzeug nicht)"
                ));
            }
            let annotations = &tool["annotations"];
            if !annotations.is_object() {
                problems.push(format!("{name}: keine annotations"));
                continue;
            }
            match annotations["title"].as_str() {
                Some(t) if !t.trim().is_empty() => {
                    let german = german_traces(t);
                    if !german.is_empty() {
                        problems.push(format!("{name}: title {t:?} deutsch ({german:?})"));
                    }
                }
                other => problems.push(format!("{name}: title fehlt/leer: {other:?}")),
            }
            for key in ["readOnlyHint", "destructiveHint"] {
                if !annotations[key].is_boolean() {
                    problems.push(format!(
                        "{name}: {key} kein Wahrheitswert: {}",
                        annotations[key]
                    ));
                }
            }
        }
        assert!(problems.is_empty(), "preview={preview}: {problems:#?}");
        if preview {
            let mut got = names(&tools);
            got.sort();
            let mut expected: Vec<String> = READ_ONLY
                .iter()
                .chain(CHANGING.iter())
                .map(|s| s.to_string())
                .collect();
            expected.sort();
            assert_eq!(got, expected, "volle Werkzeugliste mit Schalter an");
        }
    }
}

// spec: AGENT-042
// spec: AGENT-020
// acceptance: independent
// Prüfpunkt 2: lesende Werkzeuge (einschließlich `crawl_status`) haben `readOnlyHint = true`, verändernde
// `readOnlyHint = false`; kein Werkzeug ist zugleich nur lesend und zerstörerisch.
#[tokio::test]
async fn read_only_hint_matches_the_spec_classification() {
    let tools = by_name(&full_listing(true).await);
    let mut problems = Vec::new();
    for name in READ_ONLY {
        let tool = tools.get(name).unwrap_or_else(|| panic!("{name} fehlt"));
        if !hint(tool, "readOnlyHint") {
            problems.push(format!("{name}: lesend laut Spec, readOnlyHint = false"));
        }
    }
    for name in CHANGING {
        let tool = tools.get(name).unwrap_or_else(|| panic!("{name} fehlt"));
        if hint(tool, "readOnlyHint") {
            problems.push(format!("{name}: verändernd laut Spec, readOnlyHint = true"));
        }
    }
    for (name, tool) in &tools {
        if hint(tool, "readOnlyHint") && hint(tool, "destructiveHint") {
            problems.push(format!(
                "{name}: readOnlyHint und destructiveHint zugleich true"
            ));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

// spec: AGENT-042
// spec: AGENT-020
// acceptance: independent
// Prüfpunkt 3a: `stop_crawl` beendet einen Crawl endgültig und ist als zerstörerisch markiert.
#[tokio::test]
async fn stop_crawl_is_destructive() {
    let tools = by_name(&full_listing(false).await);
    for name in MUST_BE_DESTRUCTIVE {
        let tool = tools.get(name).unwrap_or_else(|| panic!("{name} fehlt"));
        assert!(
            hint(tool, "destructiveHint"),
            "{name}: destructiveHint muss true sein"
        );
        assert!(
            !hint(tool, "readOnlyHint"),
            "{name}: readOnlyHint muss false sein"
        );
    }
}

// spec: AGENT-042
// spec: AGENT-021
// acceptance: independent
// Prüfpunkt 3b: Verhaltensprobe für `export`. Eine vorhandene Datei gleichen Namens (mit `path` und mit dem
// Standardnamen) wird vor den Aufruf gelegt; `destructiveHint` muss zum Verhalten passen (überschreibt →
// true). Zusätzlich verlangt AGENT-021, dass die vorhandene Datei byte-gleich bleibt. `export` schreibt Dateien
// und ist daher nie `readOnlyHint = true`.
#[tokio::test]
async fn export_destructive_hint_matches_observed_overwrite_behaviour() {
    let world = World::new();
    let stored = world.add("out-basic-site", "/", base_config(), 4201).await;
    let exports = tempfile::tempdir().unwrap();
    let home = home_with(false, Some(exports.path()));
    let client = spawn(home.path(), world.dir.path(), &["--allow", "write"]).await;
    let export = by_name(&tools_json(&client).await)
        .remove("export")
        .expect("export mit --allow write gelistet");

    const SENTINEL: &[u8] = b"vorhandene Datei - darf nicht verschwinden\n";
    let base =
        json!({"crawl_id": stored.id, "kind": "tab", "name": "Internal:All", "format": "csv"});

    // (a) ausdrücklicher Pfad auf eine vorhandene Datei
    let probe = exports.path().join("probe.csv");
    std::fs::write(&probe, SENTINEL).unwrap();
    let mut with_path = base.clone();
    with_path["path"] = json!("probe.csv");
    let (failed, texts) = call(&client, "export", with_path).await;
    let overwritten_explicit = std::fs::read(&probe).unwrap() != SENTINEL;

    // (b) Standardname: erster Export legt die Datei an, sie wird ersetzt, der zweite Export darf sie nicht
    // überschreiben.
    let first = call_ok(&client, "export", base.clone()).await;
    let summary: Value = serde_json::from_str(&first[0]).unwrap();
    let default_path = std::path::PathBuf::from(summary["summary"]["files"][0].as_str().unwrap());
    std::fs::write(&default_path, SENTINEL).unwrap();
    let second = call_ok(&client, "export", base.clone()).await;
    let overwritten_default = std::fs::read(&default_path).unwrap() != SENTINEL;
    client.cancel().await.unwrap();

    let overwrites = overwritten_explicit || overwritten_default;
    assert!(
        !hint(&export, "readOnlyHint"),
        "export schreibt Dateien: readOnlyHint muss false sein"
    );
    if overwrites {
        assert!(
            hint(&export, "destructiveHint"),
            "export überschreibt vorhandene Dateien (path: {overwritten_explicit}, Standardname: \
             {overwritten_default}), destructiveHint ist aber false"
        );
    }
    assert!(
        !failed,
        "Export auf vorhandenen Pfad schlug fehl: {texts:?}"
    );
    assert!(
        !overwrites,
        "AGENT-021: vorhandene Datei muss byte-gleich bleiben (path: {overwritten_explicit}, \
         Standardname: {overwritten_default}); Antworten: {texts:?} / {second:?}"
    );
}

/// Inhalt aller Crawl-Dateien (`*.ospider`) des Ordners.
fn crawl_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "ospider"))
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect()
}

// spec: AGENT-042
// spec: AGENT-010
// spec: AGENT-020
// acceptance: independent
// Prüfpunkt 4: Verhaltensprobe statt Etikett. Für als lesend markierte Werkzeuge (darunter `crawl_status`)
// bleiben nach je einem Aufruf gegen Fixture-Crawls die Crawl-Dateien byte-gleich und `list_crawls` liefert
// dasselbe wie vorher.
#[tokio::test]
async fn tools_marked_read_only_leave_crawl_files_and_listing_unchanged() {
    let world = World::new();
    let a = world.add("out-basic-site", "/", base_config(), 4202).await;
    let b = world.add("out-basic-site", "/", base_config(), 4203).await;
    let start = a.fx.site.url("/");
    let home = home_with(false, None);
    let client = spawn(home.path(), world.dir.path(), &["--allow", "crawl"]).await;
    let tools = by_name(&tools_json(&client).await);

    let files_before = crawl_files(world.dir.path());
    assert_eq!(files_before.len(), 2);
    let listing_before = call_ok(&client, "list_crawls", json!({})).await;

    let probes: Vec<(&str, Value)> = vec![
        ("crawl_status", json!({"crawl_id": a.id})),
        ("crawl_overview", json!({"crawl_id": a.id})),
        (
            "query_urls",
            json!({"crawl_id": a.id, "tab": "Internal", "filter": "All"}),
        ),
        ("url_details", json!({"crawl_id": a.id, "url": start})),
        ("list_issues", json!({"crawl_id": a.id})),
        (
            "sql_query",
            json!({"crawl_id": a.id, "sql": "SELECT COUNT(*) FROM urls"}),
        ),
        ("compare_crawls", json!({"a": a.id, "b": b.id})),
        ("list_catalog", json!({"kind": "tabs"})),
        ("list_crawls", json!({})),
    ];
    let mut probed = 0;
    for (name, args) in probes {
        let tool = tools
            .get(name)
            .unwrap_or_else(|| panic!("{name} nicht gelistet"));
        assert!(
            hint(tool, "readOnlyHint"),
            "{name}: als lesend erwartet (readOnlyHint = true)"
        );
        call_ok(&client, name, args).await;
        assert_eq!(
            crawl_files(world.dir.path()),
            files_before,
            "{name} hat eine Crawl-Datei verändert"
        );
        assert_eq!(
            call_ok(&client, "list_crawls", json!({})).await,
            listing_before,
            "Liste der Crawls nach {name} verändert"
        );
        probed += 1;
    }
    assert!(probed >= 3);
    client.cancel().await.unwrap();
}

// spec: AGENT-042
// spec: AGENT-003
// spec: AGENT-001
// acceptance: independent
// Prüfpunkt 5: Freigaben unverändert. Auslieferungszustand und `--read-only`: keine Crawl-, Export- oder
// Kostenwerkzeuge in `tools/list`, Aufrufe werden mit Freigabehinweis abgelehnt; alle lesenden Werkzeuge außer
// `crawl_status` sind da. `crawl_status` erscheint erst mit `--allow crawl` (dann ohne `export`/Kosten). Die
// Annotationen hängen nicht von der Freigabe ab.
#[tokio::test]
async fn permissions_are_unchanged_and_crawl_status_needs_the_crawl_class() {
    let full = by_name(&full_listing(true).await);
    for args in [&[][..], &["--read-only"][..]] {
        let home = home_with(true, None);
        let crawls = tempfile::tempdir().unwrap();
        let client = spawn(home.path(), crawls.path(), args).await;
        let tools = tools_json(&client).await;
        let listed = names(&tools);
        for name in CRAWL_CLASS.iter().chain(&WRITE_CLASS).chain(&COST_CLASS) {
            assert!(
                !listed.iter().any(|n| n == name),
                "{args:?}: {name} gelistet: {listed:?}"
            );
        }
        for name in READ_ONLY.iter().filter(|n| **n != "crawl_status") {
            assert!(
                listed.iter().any(|n| n == name),
                "{args:?}: {name} fehlt: {listed:?}"
            );
        }
        for tool in &tools {
            let name = tool["name"].as_str().unwrap();
            assert_eq!(
                tool["annotations"], full[name]["annotations"],
                "{name}: Annotationen hängen von der Freigabe ab"
            );
        }
        for (name, class) in [
            ("crawl_status", "crawl"),
            ("start_crawl", "crawl"),
            ("stop_crawl", "crawl"),
            ("export", "write"),
            ("fetch_link_metrics", "cost"),
            ("run_decisions", "cost"),
        ] {
            let (rejected, texts) = call(&client, name, json!({"crawl_id": "x"})).await;
            assert!(rejected, "{args:?}: {name} nicht abgelehnt: {texts:?}");
            let text = texts.join(" ");
            assert!(
                text.contains(&format!("--allow {class}")),
                "{args:?}: Ablehnung von {name} ohne Freigabehinweis: {text}"
            );
        }
        client.cancel().await.unwrap();
    }

    let home = home_with(true, None);
    let crawls = tempfile::tempdir().unwrap();
    let client = spawn(home.path(), crawls.path(), &["--allow", "crawl"]).await;
    let listed = names(&tools_json(&client).await);
    for name in CRAWL_CLASS {
        assert!(
            listed.iter().any(|n| n == name),
            "--allow crawl: {name} fehlt: {listed:?}"
        );
    }
    for name in WRITE_CLASS.iter().chain(&COST_CLASS) {
        assert!(
            !listed.iter().any(|n| n == name),
            "--allow crawl: {name} gelistet"
        );
    }
    client.cancel().await.unwrap();
}

// spec: AGENT-042
// spec: AGENT-004
// acceptance: independent
// Prüfpunkt 6: Server-Hinweise aus `initialize`, Werkzeugbeschreibungen, Annotation-Titel und alle
// Beschreibungen/Titel im Eingabeschema (auch verschachtelt) enthalten keine deutschen Texte. Der Hinweis auf
// nicht vertrauenswürdige Inhalte bleibt in den Server-Hinweisen und jeder Werkzeugbeschreibung; ein Ergebnis
// mit Seiteninhalten trägt weiter den Block `<<<UNTRUSTED_CONTENT>>>`.
#[tokio::test]
async fn descriptions_and_instructions_are_english_and_keep_the_untrusted_notice() {
    let world = World::new();
    let stored = world.add("out-basic-site", "/", base_config(), 4204).await;
    let home = home_with(true, None);
    let client = spawn(
        home.path(),
        world.dir.path(),
        &["--allow", "crawl,write,cost"],
    )
    .await;
    let mut problems = Vec::new();

    let instructions = client
        .peer_info()
        .and_then(|i| i.instructions.clone())
        .unwrap_or_default();
    if instructions.trim().is_empty() {
        problems.push("initialize: keine Server-Hinweise".to_string());
    }
    let german = german_traces(&instructions);
    if !german.is_empty() {
        problems.push(format!("initialize: deutsch {german:?}: {instructions}"));
    }
    if !instructions.contains(MARKER) {
        problems.push(format!("initialize: Marker {MARKER} fehlt: {instructions}"));
    }

    for tool in tools_json(&client).await {
        let name = tool["name"].as_str().unwrap().to_string();
        let description = tool["description"].as_str().unwrap_or_default();
        if description.trim().is_empty() {
            problems.push(format!("{name}: keine Beschreibung"));
        }
        let german = german_traces(description);
        if !german.is_empty() {
            problems.push(format!("{name}: Beschreibung deutsch {german:?}"));
        }
        let lower = description.to_lowercase();
        if !(description.contains(MARKER) || lower.contains("untrusted"))
            || !lower.contains("not instructions")
        {
            problems.push(format!(
                "{name}: Beschreibung ohne Hinweis auf nicht vertrauenswürdige Inhalte"
            ));
        }
        if let Some(title) = tool["annotations"]["title"].as_str() {
            let german = german_traces(title);
            if !german.is_empty() {
                problems.push(format!("{name}: annotations.title deutsch {german:?}"));
            }
        }
        let mut texts = Vec::new();
        schema_texts(&tool["inputSchema"], "", &mut texts);
        for (path, text) in texts {
            let german = german_traces(&text);
            if !german.is_empty() {
                problems.push(format!(
                    "{name}: inputSchema{path} deutsch {german:?}: {text}"
                ));
            }
        }
    }

    let texts = call_ok(
        &client,
        "query_urls",
        json!({"crawl_id": stored.id, "tab": "Internal", "filter": "All", "limit": 5}),
    )
    .await;
    if !texts.iter().any(|t| t.starts_with(MARKER)) {
        problems.push(format!("query_urls: kein Block {MARKER}: {texts:?}"));
    }
    client.cancel().await.unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
}

/// Abschnitte der Markdown-Beschreibung: (Werkzeugname aus `## `, Abschnittstext).
fn markdown_sections(markdown: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    for line in markdown.lines() {
        if let Some(name) = line.strip_prefix("## ") {
            sections.push((name.trim().to_string(), String::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    sections
}

/// Prüft die Markdown-Beschreibung gegen `tools/list`: gleiche Namen in gleicher Reihenfolge, gleiche Anzahl
/// Überschriften, Beschreibung und Parameterschema je Werkzeug.
fn check_markdown(markdown: &str, tools: &[Value], problems: &mut Vec<String>) {
    let sections = markdown_sections(markdown);
    let section_names: Vec<String> = sections.iter().map(|(n, _)| n.clone()).collect();
    if section_names != names(tools) {
        problems.push(format!(
            "Markdown-Überschriften {section_names:?} ≠ tools/list {:?}",
            names(tools)
        ));
        return;
    }
    for ((name, body), tool) in sections.iter().zip(tools) {
        let description = tool["description"].as_str().unwrap_or_default();
        if !body.contains(description) {
            problems.push(format!("{name}: Beschreibung im Markdown weicht ab"));
        }
        let Some(properties) = tool["inputSchema"]["properties"].as_object() else {
            continue;
        };
        for (param, schema) in properties {
            let row = body
                .lines()
                .find(|l| l.starts_with(&format!("| {param} |")));
            let Some(row) = row else {
                problems.push(format!("{name}: Parameter {param} fehlt im Markdown"));
                continue;
            };
            let shown = row
                .split_once('`')
                .and_then(|(_, rest)| rest.rsplit_once('`'))
                .map(|(inner, _)| inner.replace("\\|", "|"));
            let parsed: Option<Value> = shown.and_then(|s| serde_json::from_str(&s).ok());
            if parsed.as_ref() != Some(schema) {
                problems.push(format!(
                    "{name}: Schema von {param} im Markdown weicht ab: {row}"
                ));
            }
        }
    }
}

// spec: AGENT-042
// spec: OUT-285
// acceptance: independent
// Prüfpunkt 7a (über das Protokoll): Die Ressource `openspider://tools` (`openspider-mcp-tools.md`) stimmt in
// Namen, Anzahl, Beschreibungen und Parameterschemata mit `tools/list` überein – bei voller Freigabe mit
// Schalter an und im Auslieferungszustand.
#[tokio::test]
async fn markdown_resource_matches_tools_list() {
    let mut problems = Vec::new();
    for args in [&["--allow", "crawl,write,cost"][..], &[][..]] {
        let home = home_with(true, None);
        let crawls = tempfile::tempdir().unwrap();
        let client = spawn(home.path(), crawls.path(), args).await;
        let tools = tools_json(&client).await;
        let resource = client
            .read_resource(ReadResourceRequestParams::new("openspider://tools"))
            .await
            .expect("openspider://tools lesbar");
        let markdown = match &resource.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text.clone(),
            other => panic!("Markdown als Text erwartet: {other:?}"),
        };
        let before = problems.len();
        check_markdown(&markdown, &tools, &mut problems);
        for p in &mut problems[before..] {
            *p = format!("{args:?}: {p}");
        }
        client.cancel().await.unwrap();
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

// spec: AGENT-042
// spec: OUT-285
// acceptance: independent
// Prüfpunkt 7b (öffentliche Funktionen für Menü und Markdown): Der freigegebene Teil von `McpServer::catalog`
// (Werkzeugliste der Oberfläche) ist gleich der Antwort auf `tools/list` (Name, Beschreibung, Schema,
// Annotationen, Reihenfolge); `McpServer::markdown` stimmt mit `tools/list` überein.
#[tokio::test]
async fn ui_catalog_and_markdown_function_match_tools_list() {
    let crawls = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let paths = app_paths_with_home(Some(home.path().as_os_str()));
    let args = McpArgs {
        crawl_dir: Some(crawls.path().to_path_buf()),
        allow: vec!["crawl".into(), "write".into(), "cost".into()],
        ..McpArgs::default()
    };
    let server = build_server(&args, &paths, tools::all()).unwrap();

    let (server_io, client_io) = tokio::io::duplex(1 << 22);
    let serving = server.clone();
    tokio::spawn(async move {
        let running = serving.serve(server_io).await.expect("Server-Handshake");
        let _ = running.waiting().await;
    });
    let client: Client = ().serve(client_io).await.expect("Client-Handshake");
    let listed = tools_json(&client).await;

    let menu: Vec<Value> = server
        .catalog()
        .into_iter()
        .filter(|(_, _, enabled)| *enabled)
        .map(|(tool, _, _)| serde_json::to_value(tool).unwrap())
        .collect();
    assert_eq!(
        names(&menu),
        names(&listed),
        "Menü-Liste ≠ tools/list (Namen/Reihenfolge)"
    );
    for (m, l) in menu.iter().zip(&listed) {
        for key in ["description", "inputSchema", "annotations"] {
            assert_eq!(
                m[key], l[key],
                "{}: {key} in Menü-Liste ≠ tools/list",
                l["name"]
            );
        }
    }
    let mut problems = Vec::new();
    check_markdown(&server.markdown(), &listed, &mut problems);
    client.cancel().await.unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
}

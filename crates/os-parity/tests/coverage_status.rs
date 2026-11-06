//! Tests von `os-parity coverage` gegen einen Beispiel-Arbeitsbereich (`tests/data/coverage/ws`) und
//! eingecheckte Beispiel-Testausgaben (cargo test und nextest-JUnit).
//!
//! Die Quelldateien des Beispiels liegen als `*.rs.txt`/`Cargo.toml.txt` im Repository, damit der echte
//! Tag-Scan (`crates/**/*.rs`) sie nicht als Tests dieses Repositories sieht; der Test kopiert sie unter
//! ihrem richtigen Namen in ein temporäres Verzeichnis.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use os_parity::coverage::{run, CoverageArgs};
use serde_json::Value;

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/coverage")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let name = e.file_name().to_string_lossy().into_owned();
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &to.join(&name));
        } else {
            let target = name.strip_suffix(".txt").unwrap_or(&name).to_string();
            std::fs::copy(e.path(), to.join(target)).unwrap();
        }
    }
}

struct Outcome {
    ok: bool,
    json: Value,
    dir: tempfile::TempDir,
}

fn run_with(test_output: Option<&str>, junit: Option<&str>) -> Outcome {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&data().join("ws"), dir.path());
    let write = dir.path().join("docs/plan/M1-status.json");
    let ok = run(&CoverageArgs {
        milestone: os_parity::coverage::Milestone::M1,
        gate: false,
        wave: 0,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: Vec::new(),
        packages: Some(data().join("packages.json")),
        test_output: test_output.map(|f| data().join(f)),
        junit: junit.map(|f| data().join(f)),
        write: Some(write.clone()),
        root: dir.path().to_path_buf(),
        strict: false,
        parity_report: None,
        evidence: None,
    })
    .unwrap();
    let json: Value = serde_json::from_str(&std::fs::read_to_string(write).unwrap()).unwrap();
    Outcome { ok, json, dir }
}

fn statuses(o: &Outcome) -> BTreeMap<String, String> {
    o.json["specs"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v["status"].as_str().unwrap().to_string()))
        .collect()
}

#[test]
fn status_rules_on_cargo_test_output() {
    let o = run_with(Some("sample-test-output.txt"), None);
    let st = statuses(&o);
    let expect = [
        ("ALPHA-001", "implemented"),
        ("ALPHA-002", "implemented"),
        ("ALPHA-003", "open"),
        ("ALPHA-004", "implemented"),
        ("ALPHA-005", "deferred"),
        ("ALPHA-006", "open"),
        ("ALPHA-007", "open"),
        ("ALPHA-008", "open"),
        ("ALPHA-009", "implemented"),
        ("ALPHA-010", "implemented"),
        ("ALPHA-011", "implemented"),
        ("ALPHA-012", "open"),
        ("ALPHA-013", "open"),
        ("ALPHA-014", "deferred"),
        ("ALPHA-015", "open"),
        ("ALPHA-016", "open"),
        ("BETA-001", "implemented"),
        ("BETA-002", "implemented"),
        ("BETA-003", "implemented"),
        ("GAMMA-001", "open"),
        ("ROOT-001", "open"),
    ];
    for (id, want) in expect {
        assert_eq!(st[id], want, "{id}: {}", o.json["specs"][id]);
    }
    assert_eq!(
        st.len(),
        21,
        "nur Einträge mit kind=package, keine Orchestrator-Schritte"
    );
    assert!(!st.contains_key("ORCH-001"));
    assert!(!o.ok, "offene IDs -> Exit-Code != 0");
    let s = &o.json["summary"];
    assert_eq!(s["total"], 21);
    assert_eq!(s["implemented"], 9);
    assert_eq!(s["deferred"], 2);
    assert_eq!(s["open"], 10);
    assert_eq!(s["verified"], 0);
}

#[test]
fn same_function_name_in_lib_and_bin_of_one_package_is_told_apart() {
    let o = run_with(Some("sample-test-output.txt"), None);
    // `tests::shared_name` läuft im Lib-Binär (ok) und im Bin-Binär (FAILED).
    let lib = &o.json["specs"]["ALPHA-002"]["tests"][0];
    assert_eq!(lib["file"], "crates/alpha/src/lib.rs");
    assert_eq!(lib["result"], "ok");
    let bin = &o.json["specs"]["ALPHA-007"]["tests"][0];
    assert_eq!(bin["file"], "crates/alpha/src/main.rs");
    assert_eq!(bin["result"], "failed");
    // Gleicher Funktionsname in util::tests und tests liegt in verschiedenen Modulen.
    assert_eq!(
        o.json["specs"]["ALPHA-004"]["tests"][0]["file"],
        "crates/alpha/src/util.rs"
    );
    assert_eq!(
        o.json["specs"]["ALPHA-004"]["tests"][0]["test"],
        "tests::shared_name"
    );
}

#[test]
fn same_integration_test_name_in_several_crates_uses_test_sets_or_warns() {
    let o = run_with(Some("sample-test-output.txt"), None);
    // beta/tests/smoke.rs hat ein eigenes Testmengen-Profil -> eindeutig.
    assert_eq!(o.json["specs"]["BETA-001"]["tests"][0]["result"], "ok");
    // alpha und gamma haben identische Test-Mengen -> Warnung, keine Gutschrift.
    assert_eq!(
        o.json["specs"]["ALPHA-008"]["tests"][0]["result"],
        "no_result"
    );
    assert_eq!(
        o.json["specs"]["GAMMA-001"]["tests"][0]["result"],
        "no_result"
    );
    let warnings: Vec<String> = o.json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_string())
        .collect();
    let ambiguous: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("nicht eindeutig"))
        .collect();
    assert_eq!(ambiguous.len(), 1, "{warnings:#?}");
    assert!(ambiguous[0].contains("crates/alpha/tests/smoke.rs"));
    assert!(ambiguous[0].contains("crates/gamma/tests/smoke.rs"));
    assert!(
        warnings.iter().any(|w| w.contains("UNKNOWN-001")),
        "{warnings:#?}"
    );
}

#[test]
fn ok_failed_ignored_and_missing_results_are_distinguished() {
    let o = run_with(Some("sample-test-output.txt"), None);
    let tests = |id: &str| -> Vec<(String, String)> {
        o.json["specs"][id]["tests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                (
                    t["test"].as_str().unwrap().to_string(),
                    t["result"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    // FAILED neben ok -> open (ein roter Test des Kriteriums reicht).
    let t12 = tests("ALPHA-012");
    assert!(t12.contains(&("tests::passes_here".into(), "ok".into())));
    assert!(t12.contains(&("tests::fails_for_twelve".into(), "failed".into())));
    assert_eq!(o.json["specs"]["ALPHA-012"]["status"], "open");
    // ok + ignoriert (M2) -> implemented.
    let t11 = tests("ALPHA-011");
    assert_eq!(t11.len(), 2);
    assert!(t11.iter().any(|t| t.1 == "ok") && t11.iter().any(|t| t.1 == "ignored"));
    // Test steht im Quelltext, aber nicht in der Ausgabe -> no_result, open.
    assert_eq!(
        tests("ALPHA-016"),
        [(
            "tests::never_ran_on_this_platform".into(),
            "no_result".into()
        )]
    );
    assert!(o.json["specs"]["ALPHA-016"]["note"]
        .as_str()
        .unwrap()
        .contains("ohne Ergebnis"));
    // Ignore-Grund wird übernommen.
    assert_eq!(
        o.json["specs"]["ALPHA-005"]["tests"][0]["ignore_reason"],
        "M2: ALPHA-005 braucht JS-Rendering"
    );
    assert_eq!(
        o.json["specs"]["ALPHA-005"]["tests"][0]["deferred_hint"],
        true
    );
    // Tag nirgends -> open mit Hinweis.
    assert!(o.json["specs"]["ALPHA-013"]["note"]
        .as_str()
        .unwrap()
        .contains("kein Test"));
}

#[test]
fn ignored_tests_without_valid_reason_are_listed() {
    let o = run_with(Some("sample-test-output.txt"), None);
    let list = o.json["ignored_without_valid_reason"].as_array().unwrap();
    let names: Vec<String> = list
        .iter()
        .map(|e| e["test"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"silent_ignore".to_string()), "{names:?}");
    assert!(
        names.contains(&"tests::bare_ignore_with_tag".to_string()),
        "{names:?}"
    );
    assert!(
        names.iter().any(|n| n.ends_with("placeholder")),
        "Platzhalter 'wartet auf M1-Pxx' ist kein gültiger Grund"
    );
    assert!(
        !names.iter().any(|n| n.ends_with("bench_wall_clock")),
        "Benchmark mit Grund ohne Tag ist erlaubt"
    );
    assert!(!names
        .iter()
        .any(|n| n.ends_with("deferred_one") || n.ends_with("needs_clarification")));
    assert_eq!(list.len(), 3, "{list:#?}");
    let placeholder = list
        .iter()
        .find(|e| e["test"].as_str().unwrap().ends_with("placeholder"))
        .unwrap();
    assert_eq!(placeholder["file"], "crates/alpha/src/util.rs");
    assert_eq!(placeholder["spec_ids"][0], "ALPHA-006");
}

#[test]
fn junit_output_resolves_package_names_and_duplicate_test_files() {
    let o = run_with(None, Some("sample-nextest.xml"));
    let st = statuses(&o);
    // nextest nennt das Paket in der Binär-ID: alpha/gamma smoke sind eindeutig.
    assert_eq!(st["ALPHA-008"], "implemented");
    assert_eq!(st["GAMMA-001"], "implemented");
    assert_eq!(st["BETA-001"], "implemented");
    // Bin-Binär "alpha::bin/alpha": hier grün.
    assert_eq!(st["ALPHA-007"], "implemented");
    assert_eq!(st["ALPHA-003"], "open");
    assert_eq!(st["ALPHA-005"], "deferred");
    assert_eq!(st["ALPHA-006"], "open");
    assert_eq!(st["ALPHA-011"], "implemented");
    assert_eq!(st["ALPHA-012"], "open");
}

#[test]
fn both_inputs_can_be_combined() {
    let o = run_with(Some("sample-test-output.txt"), Some("sample-nextest.xml"));
    let st = statuses(&o);
    assert_eq!(
        st["ALPHA-008"], "implemented",
        "JUnit löst die Mehrdeutigkeit der Textausgabe auf"
    );
    assert_eq!(st["ALPHA-007"], "open", "FAILED in einem der Läufe genügt");
}

#[test]
fn no_results_means_every_id_is_open_with_a_warning() {
    let o = run_with(None, None);
    assert_eq!(o.json["summary"]["open"], 21);
    assert!(!o.ok);
    assert!(o.json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("keine Testergebnisse")));
}

#[test]
fn status_file_is_deterministic_and_verified_is_preserved() {
    let o = run_with(Some("sample-test-output.txt"), None);
    let path = o.dir.path().join("docs/plan/M1-status.json");
    let first = std::fs::read_to_string(&path).unwrap();
    // P29 setzt `verified` für eine implementierte ID.
    let mut doc: Value = serde_json::from_str(&first).unwrap();
    doc["specs"]["ALPHA-001"]["status"] = "verified".into();
    doc["specs"]["ALPHA-001"]["parity"] =
        serde_json::json!({"site": "out-basic-site", "export": "internal_all"});
    // Ein offener Eintrag wird nie durch `verified` überschrieben.
    doc["specs"]["ALPHA-003"]["status"] = "verified".into();
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let ok = run(&CoverageArgs {
        milestone: os_parity::coverage::Milestone::M1,
        gate: false,
        wave: 0,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: Vec::new(),
        packages: Some(data().join("packages.json")),
        test_output: Some(data().join("sample-test-output.txt")),
        junit: None,
        write: Some(path.clone()),
        root: o.dir.path().to_path_buf(),
        strict: false,
        parity_report: None,
        evidence: None,
    })
    .unwrap();
    assert!(!ok);
    let second: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(second["specs"]["ALPHA-001"]["status"], "verified");
    assert_eq!(
        second["specs"]["ALPHA-001"]["parity"]["site"],
        "out-basic-site"
    );
    assert_eq!(second["specs"]["ALPHA-003"]["status"], "open");
    assert_eq!(second["summary"]["verified"], 1);
    assert_eq!(second["summary"]["implemented"], 8);
    // Ohne Änderung der Eingaben ist die Datei byte-gleich.
    let o2 = run_with(Some("sample-test-output.txt"), None);
    assert_eq!(
        first,
        std::fs::read_to_string(o2.dir.path().join("docs/plan/M1-status.json")).unwrap()
    );
}

#[test]
fn strict_mode_fails_on_ignored_tests_without_reason() {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&data().join("ws"), dir.path());
    std::fs::write(
        dir.path().join("packages.json"),
        r#"[{"id":"P","kind":"package","spec_ids":["ALPHA-001"]}]"#,
    )
    .unwrap();
    let mut args = CoverageArgs {
        milestone: os_parity::coverage::Milestone::M1,
        gate: false,
        wave: 0,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: Vec::new(),
        packages: Some(dir.path().join("packages.json")),
        test_output: Some(data().join("sample-test-output.txt")),
        junit: None,
        write: None,
        root: dir.path().to_path_buf(),
        strict: false,
        parity_report: None,
        evidence: None,
    };
    assert!(
        run(&args).unwrap(),
        "ALPHA-001 ist implemented, nichts offen"
    );
    args.strict = true;
    assert!(
        !run(&args).unwrap(),
        "--strict: ignorierte Tests ohne Grund lassen den Lauf scheitern"
    );
}

#[test]
fn invalid_inputs_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let base = CoverageArgs {
        milestone: os_parity::coverage::Milestone::M1,
        gate: false,
        wave: 0,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: Vec::new(),
        packages: Some(dir.path().join("fehlt.json")),
        test_output: None,
        junit: None,
        write: None,
        root: dir.path().to_path_buf(),
        strict: false,
        parity_report: None,
        evidence: None,
    };
    assert!(run(&base).is_err());
    std::fs::write(dir.path().join("p.json"), "{\"nope\": 1}").unwrap();
    assert!(run(&CoverageArgs {
        milestone: os_parity::coverage::Milestone::M1,
        gate: false,
        wave: 0,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: Vec::new(),
        packages: Some(dir.path().join("p.json")),
        ..base.clone()
    })
    .is_err());
    std::fs::write(dir.path().join("p.json"), "[]").unwrap();
    std::fs::write(dir.path().join("bad.xml"), "<foo/>").unwrap();
    assert!(run(&CoverageArgs {
        milestone: os_parity::coverage::Milestone::M1,
        gate: false,
        wave: 0,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: Vec::new(),
        packages: Some(dir.path().join("p.json")),
        junit: Some(dir.path().join("bad.xml")),
        ..base
    })
    .is_err());
}

#[test]
fn parity_evidence_sets_verified_only_for_clean_implemented_ids() {
    let o = run_with(Some("sample-test-output.txt"), None);
    let dir = o.dir.path();
    let report = dir.join("parity-report.json");
    // ALPHA-001: sauberer Export. ALPHA-002: Spaltenfehler. ALPHA-003 ist offen und bleibt es.
    let clean = serde_json::json!({"outcome": "compared", "report": {
        "export": "good", "url_set_identical": true, "cells_compared": 40, "agreement_pct": 100.0,
        "columns": [{"ref_column": "A", "errors": 0}], "applied_deviations": [{"id": "DEV-001"}]}});
    let bad = serde_json::json!({"outcome": "compared", "report": {
        "export": "bad", "url_set_identical": true, "cells_compared": 40, "agreement_pct": 90.0,
        "columns": [{"ref_column": "A", "errors": 4}], "applied_deviations": []}});
    std::fs::write(
        &report,
        serde_json::json!({"sites": [{"site": "s", "exports": [clean, bad]}]}).to_string(),
    )
    .unwrap();
    let evidence = dir.join("evidence.toml");
    std::fs::write(
        &evidence,
        "[[evidence]]\nspecs = [\"ALPHA-001\"]\nexports = [\"good\"]\n[[evidence]]\nspecs = [\"ALPHA-002\", \"ALPHA-003\", \"UNBEKANNT-1\"]\nexports = [\"bad\"]\n",
    )
    .unwrap();
    let write = dir.join("status-with-parity.json");
    let ok = run(&CoverageArgs {
        milestone: os_parity::coverage::Milestone::M1,
        gate: false,
        wave: 0,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: Vec::new(),
        packages: Some(data().join("packages.json")),
        test_output: Some(data().join("sample-test-output.txt")),
        junit: None,
        write: Some(write.clone()),
        root: dir.to_path_buf(),
        strict: false,
        parity_report: Some(report),
        evidence: Some(evidence),
    })
    .unwrap();
    assert!(!ok, "ALPHA-003 bleibt offen");
    let json: Value = serde_json::from_str(&std::fs::read_to_string(write).unwrap()).unwrap();
    let specs = &json["specs"];
    assert_eq!(specs["ALPHA-001"]["status"], "verified");
    assert_eq!(specs["ALPHA-001"]["parity"]["exports"][0]["export"], "good");
    assert_eq!(
        specs["ALPHA-001"]["parity"]["exports"][0]["deviations"][0],
        "DEV-001"
    );
    assert_eq!(specs["ALPHA-002"]["status"], "implemented");
    assert!(specs["ALPHA-002"]["parity"]["not_verified"]
        .as_str()
        .unwrap()
        .contains("Spalte A"));
    assert_eq!(specs["ALPHA-003"]["status"], "open");
    assert_eq!(json["summary"]["verified"], 1);
    assert!(json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("UNBEKANNT-1")));
}

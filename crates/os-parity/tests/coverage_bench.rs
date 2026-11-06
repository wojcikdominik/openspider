//! `os-parity coverage`: Benchmark-Ignores (5.4 Punkt 4) und `extra_spec_ids`.
//!
//! Arbeitsbereich und Testausgabe liegen unter `tests/data/coverage_bench` (Quellen als `*.rs.txt`, damit
//! der Tag-Scan dieses Repositories sie nicht als eigene Tests sieht).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use os_parity::coverage::{is_benchmark_reason, load_packages, run, CoverageArgs};
use serde_json::Value;

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/coverage_bench")
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

fn run_with(packages: &Path, strict: bool) -> Outcome {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&data().join("ws"), dir.path());
    let write = dir.path().join("M1-status.json");
    let ok = run(&CoverageArgs {
        milestone: os_parity::coverage::Milestone::M1,
        gate: false,
        wave: 0,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: Vec::new(),
        packages: Some(packages.to_path_buf()),
        test_output: Some(data().join("test-output.txt")),
        junit: None,
        write: Some(write.clone()),
        root: dir.path().to_path_buf(),
        strict,
        parity_report: None,
        evidence: None,
    })
    .unwrap();
    let json: Value = serde_json::from_str(&std::fs::read_to_string(write).unwrap()).unwrap();
    Outcome { ok, json, dir }
}

fn default_run() -> Outcome {
    run_with(&data().join("packages.json"), false)
}

fn statuses(section: &Value) -> BTreeMap<String, String> {
    section
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v["status"].as_str().unwrap().to_string()))
        .collect()
}

fn warnings(o: &Outcome) -> Vec<String> {
    o.json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn benchmark_prefix_is_recognised() {
    assert!(is_benchmark_reason("Benchmark (P28): Durchsatz"));
    assert!(is_benchmark_reason("  Benchmark: x"));
    assert!(!is_benchmark_reason("benchmark: klein"));
    assert!(!is_benchmark_reason("M2: Benchmark"));
    assert!(!is_benchmark_reason(""));
}

#[test]
fn benchmark_tests_are_neutral_for_the_status() {
    let o = default_run();
    let st = statuses(&o.json["specs"]);
    // ok-Test + Benchmark -> implemented.
    assert_eq!(st["B-001"], "implemented");
    // Nur Benchmark-Tests -> open, mit Hinweis.
    assert_eq!(st["B-002"], "open");
    let note = o.json["specs"]["B-002"]["note"].as_str().unwrap();
    assert!(note.contains("Benchmark"), "{note}");
    // M2-Rückstellung + Benchmark -> deferred (der Benchmark blockiert nicht).
    assert_eq!(st["B-003"], "deferred");
    // Platzhalter + Benchmark -> open (der Benchmark macht den Platzhalter nicht gültig).
    assert_eq!(st["B-004"], "open");
    // Auch ein gelaufener Benchmark ändert nichts: der reguläre ok-Test trägt den Status.
    assert_eq!(st["B-005"], "implemented");
    let s = &o.json["summary"];
    assert_eq!(s["total"], 5);
    assert_eq!(s["implemented"], 2);
    assert_eq!(s["deferred"], 1);
    assert_eq!(s["open"], 2);
    assert!(!o.ok, "B-002 und B-004 sind offen");
    // Die Benchmark-Tests bleiben im Bericht sichtbar, mit Grund.
    let tests = o.json["specs"]["B-002"]["tests"].as_array().unwrap();
    assert_eq!(tests.len(), 2);
    assert!(tests.iter().all(|t| t["result"] == "ignored"
        && t["ignore_reason"]
            .as_str()
            .unwrap()
            .starts_with("Benchmark")));
}

#[test]
fn benchmark_ignores_are_valid_but_other_reasons_are_not() {
    let o = default_run();
    let list = o.json["ignored_without_valid_reason"].as_array().unwrap();
    let names: Vec<&str> = list.iter().map(|e| e["test"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 2, "{list:#?}");
    assert!(names.contains(&"placeholder"));
    assert!(
        names.contains(&"lowercase_bench"),
        "nur der Präfix `Benchmark` zählt"
    );
    assert!(!names
        .iter()
        .any(|n| n.contains("bench_") && *n != "lowercase_bench"));
}

#[test]
fn extra_spec_ids_get_their_own_section_and_no_unknown_tag_warning() {
    let o = default_run();
    let extra = statuses(&o.json["extra"]);
    assert_eq!(extra["X-001"], "implemented");
    assert_eq!(extra["X-002"], "open", "nur Benchmark-Tests");
    assert_eq!(extra["X-003"], "open", "kein Test");
    // Orchestrator-Schritte tragen keine Zusatz-IDs; IDs, die ein Paket als spec_ids führt, stehen nur dort.
    assert!(!extra.contains_key("X-004"));
    assert!(!extra.contains_key("B-005"));
    assert_eq!(extra.len(), 3);
    assert_eq!(
        o.json["extra"]["X-001"]["packages"],
        serde_json::json!(["M1-PB1", "M1-PB2"])
    );
    assert!(!statuses(&o.json["specs"]).contains_key("X-001"));
    // Z-999 steht in keinem Feld -> Warnung; X-001/X-002 nicht.
    let w = warnings(&o);
    let unknown: Vec<_> = w.iter().filter(|w| w.contains("in keinem Paket")).collect();
    assert_eq!(unknown.len(), 1, "{w:#?}");
    assert!(unknown[0].contains("Z-999"), "{unknown:?}");
    assert!(!unknown[0].contains("X-00"), "{unknown:?}");
}

#[test]
fn extra_ids_do_not_count_for_summary_exit_code_or_strict() {
    // Nur reguläre IDs, die grün bzw. zurückgestellt sind; die Zusatz-ID X-003 bleibt offen.
    let dir = tempfile::tempdir().unwrap();
    let pk = dir.path().join("p.json");
    std::fs::write(
        &pk,
        r#"[{"id":"M1-PB1","kind":"package","spec_ids":["B-001","B-003","B-005"],"extra_spec_ids":["X-003","X-002"]}]"#,
    )
    .unwrap();
    let o = run_with(&pk, false);
    assert!(o.ok, "offene Zusatz-IDs lassen den Lauf nicht scheitern");
    assert_eq!(o.json["summary"]["total"], 3);
    assert_eq!(o.json["summary"]["open"], 0);
    assert_eq!(statuses(&o.json["extra"])["X-003"], "open");

    // --strict: nur ungültige Ignores zählen. Arbeitsbereich ohne Platzhalter und ohne Kleinschreibung.
    let ws = o.dir.path();
    let src = ws.join("crates/bench/tests/bench.rs");
    let text = std::fs::read_to_string(&src).unwrap();
    let cleaned = text
        .split("\n\n")
        .filter(|blk| !blk.contains("fn placeholder") && !blk.contains("fn lowercase_bench"))
        .collect::<Vec<_>>()
        .join("\n\n");
    std::fs::write(&src, cleaned).unwrap();
    assert!(run(&CoverageArgs {
        milestone: os_parity::coverage::Milestone::M1,
        gate: false,
        wave: 0,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: Vec::new(),
        packages: Some(pk.clone()),
        test_output: Some(data().join("test-output.txt")),
        junit: None,
        write: None,
        root: ws.to_path_buf(),
        strict: true,
        parity_report: None,
        evidence: None,
    })
    .unwrap());
}

#[test]
fn load_packages_reads_extra_spec_ids_from_packages_only() {
    let p = load_packages(&data().join("packages.json")).unwrap();
    assert_eq!(p.specs.len(), 5);
    assert_eq!(
        p.extra.keys().map(String::as_str).collect::<Vec<_>>(),
        ["X-001", "X-002", "X-003"]
    );
    assert_eq!(p.extra["X-001"], ["M1-PB1", "M1-PB2"]);
}

#[test]
fn extra_section_is_deterministic() {
    let a = default_run();
    let b = default_run();
    assert_eq!(a.json, b.json);
    assert!(a.dir.path().exists());
}

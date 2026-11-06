//! M2-Q01 ist Infrastruktur ohne eigene Spec-IDs. Die Test-Tags in den Fixtures belegen die
//! Zuordnungsregeln aus M2.md B2/C9/K2-58; sie werden nicht als Produktabdeckung gutgeschrieben.

use os_parity::coverage::{run, CoverageArgs, Milestone};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/coverage_m2")
}
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let name = e.file_name().to_string_lossy().into_owned();
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &to.join(name));
        } else {
            std::fs::copy(
                e.path(),
                to.join(name.strip_suffix(".txt").unwrap_or(&name)),
            )
            .unwrap();
        }
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    args: CoverageArgs,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        copy(&data().join("ws"), dir.path());
        std::fs::copy(
            data().join("packages.json"),
            dir.path().join("docs/plan/M2-packages.json"),
        )
        .unwrap();
        let args = CoverageArgs {
            milestone: Milestone::M2,
            gate: false,
            wave: 1,
            merged: Vec::new(),
            merged_log: None,
            frontend_junit: vec![data().join("vitest.xml"), data().join("playwright.xml")],
            packages: None,
            root: dir.path().into(),
            test_output: Some(data().join("cargo.txt")),
            junit: None,
            write: Some(dir.path().join("M2-status.json")),
            strict: false,
            parity_report: None,
            evidence: None,
        };
        Self { dir, args }
    }
    fn check(&self) -> (bool, Value) {
        let ok = run(&self.args).unwrap();
        let value = serde_json::from_str(
            &std::fs::read_to_string(self.args.write.as_ref().unwrap()).unwrap(),
        )
        .unwrap();
        (ok, value)
    }
    fn append(&self, file: &str, content: &str) {
        let p = self.dir.path().join(file);
        let mut s = std::fs::read_to_string(&p).unwrap();
        s.push_str(content);
        std::fs::write(p, s).unwrap();
    }
}

#[test]
fn m2_development_reads_rust_vitest_playwright_measurements_and_appendix() {
    let f = Fixture::new();
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["summary"]["total"], 9);
    for id in ["CRAWL-001", "UI-001", "UI-002", "UI-003", "PERF-001"] {
        assert_eq!(doc["specs"][id]["status"], "implemented", "{id}");
    }
    assert_eq!(doc["specs"]["CRAWL-002"]["status"], "deferred");
    assert_eq!(doc["specs"]["UI-004"]["status"], "deferred");
    assert_eq!(doc["specs"]["CRAWL-003"]["status"], "open");
    assert_eq!(doc["placeholders"].as_array().unwrap().len(), 1);
    assert_eq!(
        doc["appendix_b"]["OUT-020"]["M2-D12"]["status"],
        "implemented"
    );
    assert_eq!(
        doc["appendix_b"]["OUT-020"]["M2-U10"]["status"],
        "implemented"
    );
    assert_eq!(doc["appendix_b"]["OUT-020"]["M2-U20"]["status"], "open");
    assert_eq!(
        doc["appendix_b"]["OUT-021"]["M2-U10"]["status"],
        "implemented"
    );
    assert!(doc["appendix_b"].get("CRAWL-049").is_none());
    let first = std::fs::read(f.args.write.as_ref().unwrap()).unwrap();
    f.check();
    assert_eq!(
        first,
        std::fs::read(f.args.write.as_ref().unwrap()).unwrap()
    );
}

#[test]
fn gate_requires_all_waves_and_rejects_deferred_and_c9() {
    let mut f = Fixture::new();
    f.args.gate = true;
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["blocking_appendix_b"], 1);
    for id in ["CRAWL-002", "CRAWL-003", "UI-004", "UI-090"] {
        assert!(doc["blocking_specs"]
            .as_array()
            .unwrap()
            .contains(&Value::from(id)));
    }
    assert_eq!(
        doc["ignored_without_valid_reason"][0]["reason"],
        "wartet auf M2-E03"
    );
}

#[test]
fn appendix_requires_the_assigned_package_and_each_marker() {
    let f = Fixture::new();
    let p = f.dir.path().join("docs/plan/M2-packages.json");
    let mut packages: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    packages[0]["extra_spec_ids"] =
        serde_json::json!(["OUT-020", "OUT-021", "TECH-001", "TECH-002"]);
    std::fs::write(p, packages.to_string()).unwrap();
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(
        doc["appendix_b"]["OUT-021"]["M2-D12"]["status"], "open",
        "UI-Tests zählen nicht für D12"
    );
    assert_eq!(
        doc["appendix_b"]["TECH-001"]["M2-D12"]["missing_markers"],
        serde_json::json!(["M2"])
    );
    assert_eq!(
        doc["appendix_b"]["TECH-002"]["M2-D12"]["missing_markers"],
        serde_json::json!(["P1"])
    );
}

#[test]
fn untagged_and_misplaced_placeholders_and_old_m2_ignores_are_invalid() {
    let f = Fixture::new();
    f.append(
        "crates/alpha/tests/check.rs",
        r#"
#[test]
#[ignore = "M2: alter Grund"]
fn old() {}
#[test]
#[ignore = "wartet auf M2-E03"]
fn misplaced() {}
#[test]
#[ignore]
fn bare() {}
"#,
    );
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(
        doc["ignored_without_valid_reason"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn failed_frontend_result_overrides_ok_and_missing_results_stay_open() {
    let mut f = Fixture::new();
    let xml = f.dir.path().join("failed.xml");
    std::fs::write(&xml, r#"<testsuite name="src/features/table/table.test.ts"><testcase name="Tabelle &gt; kopiert" classname="src/features/table/table.test.ts"><failure/></testcase></testsuite>"#).unwrap();
    f.args.frontend_junit.push(xml);
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["extra"]["OUT-020"]["status"], "open");
    assert_eq!(doc["appendix_b"]["OUT-020"]["M2-U10"]["status"], "open");
    f.args.frontend_junit.clear();
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["specs"]["UI-001"]["tests"][0]["result"], "no_result");
}

#[test]
fn ambiguous_frontend_titles_never_receive_credit() {
    let f = Fixture::new();
    f.append("apps/desktop/src/features/table/table.test.ts", "\ndescribe('Tabelle', () => {\n// spec: UI-001\ntest('zeigt Zeilen & Größen', () => {});\n});\n");
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["specs"]["UI-001"]["status"], "open");
    assert!(doc["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s.as_str().unwrap().contains("2 Kandidaten")));
}

#[test]
fn malformed_junit_is_an_error() {
    for broken in [
        "<foo/>",
        "<testsuite><testcase name='x'>",
        "<testsuite><testcase name='x'/></wrong>",
        "<testsuite><!--",
        "<testsuite><![CDATA[",
    ] {
        let mut f = Fixture::new();
        let p = f.dir.path().join("broken.xml");
        std::fs::write(&p, broken).unwrap();
        f.args.frontend_junit = vec![p];
        assert!(run(&f.args).is_err(), "{broken}");
    }
}

#[test]
fn all_new_ignore_prefixes_are_milestone_specific() {
    use os_parity::coverage::{m2, valid_defer_reason};
    for reason in [
        "M3: x",
        "M4: x",
        "M5: x",
        "P2: x",
        "Klärung K2-12: x",
        "Klärung K3-01: x",
    ] {
        assert!(m2::valid_defer_reason(reason), "{reason}");
    }
    for reason in [
        "M2: x",
        "Klärung K-12",
        "Klärung K2-x",
        "Klärung K2-12foo",
        "Klärung K3-x",
        "Klärung K4-01: x",
        "wartet auf M2-E03",
        "Benchmark: x",
        "",
    ] {
        assert!(!m2::valid_defer_reason(reason), "{reason}");
    }
    assert!(valid_defer_reason("M2: bleibt für M1 gültig"));
    assert!(!valid_defer_reason("M4: M1 unverändert"));
    // K2-68: M2-Klärungen bleiben im M1-Strict-Lauf gültige Zurückstellungen.
    assert!(valid_defer_reason(
        "Klärung K2-68: N01-C01, Besitzer M2-N01b"
    ));
    assert!(!valid_defer_reason("Klärung K2-x"));
    // K3-01: M3-Klärungen gelten im M1-Strict-Lauf ebenso.
    assert!(valid_defer_reason(
        "Klärung K3-01: Lighthouse wird nicht gebündelt"
    ));
    assert!(!valid_defer_reason("Klärung K3-x"));
}

#[test]
fn cli_defaults_and_multiple_frontend_reports() {
    let f = Fixture::new();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_os-parity"))
        .args(["coverage", "--milestone", "M2", "--root"])
        .arg(f.dir.path())
        .arg("--test-output")
        .arg(data().join("cargo.txt"))
        .arg("--frontend-junit")
        .arg(data().join("vitest.xml"))
        .arg("--frontend-junit")
        .arg(data().join("playwright.xml"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Anhang B"));
}

#[test]
fn typescript_regex_literals_do_not_corrupt_groups_or_create_tags() {
    use os_parity::coverage::frontend::scan_source;
    let src = r#"
const pattern = /"([^\"]+)"/g;
const other = /[/*]test('fake', function() {})/;
const escaped = /\/\/ spec: UI-999/;
const division = 12 / 3;
describe('Gruppe', () => {
  const regex = /[{}'"`]/;
  // spec: UI-001 [P1]
  it('danach', () => {});
});
// spec: UI-002
it('außerhalb', () => {});
"#;
    let scan = scan_source("apps/desktop/src/sample.test.ts", src);
    assert_eq!(scan.tests.len(), 2);
    assert_eq!(scan.tests[0].name, "Gruppe > danach");
    assert_eq!(scan.tests[0].ids, ["UI-001"]);
    assert_eq!(scan.tests[1].name, "außerhalb");
}

#[test]
fn repository_m2_report_includes_all_586_ids_including_spikes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let args = CoverageArgs {
        milestone: Milestone::M2,
        gate: false,
        wave: 1,
        merged: Vec::new(),
        merged_log: None,
        frontend_junit: vec![],
        packages: None,
        root: root.into(),
        test_output: None,
        junit: None,
        write: Some(dir.path().join("status.json")),
        strict: false,
        parity_report: None,
        evidence: None,
    };
    run(&args).unwrap();
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(args.write.unwrap()).unwrap()).unwrap();
    // 586 spec_ids + AGENT-016 … 019 (M2-M03-Nachtrag) = 590.
    assert_eq!(doc["summary"]["total"], 590);
    for id in ["UI-007", "UI-012", "UI-140", "PERF-001"] {
        assert!(doc["specs"].get(id).is_some(), "{id}");
    }
}

#[test]
fn junit_numeric_entities_and_windows_paths_match_static_titles() {
    let mut f = Fixture::new();
    let xml = std::fs::read_to_string(data().join("vitest.xml"))
        .unwrap()
        .replace(
            "src/features/table/table.test.ts",
            "src\\features\\table\\table.test.ts",
        )
        .replace("Größen", "Gr&#246;&#xDF;en");
    let p = f.dir.path().join("windows.xml");
    std::fs::write(&p, xml).unwrap();
    f.args.frontend_junit[0] = p;
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["specs"]["UI-001"]["status"], "implemented");
}

#[test]
fn a_complete_gate_can_pass_with_all_appendix_packages() {
    let mut f = Fixture::new();
    f.args.gate = true;
    f.args.strict = true;
    for file in [
        "crates/alpha/tests/check.rs",
        "crates/alpha/tests/pending_w1_d12.rs",
    ] {
        let p = f.dir.path().join(file);
        let src = std::fs::read_to_string(&p)
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with("#[ignore"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(p, src).unwrap();
    }
    let p = f.dir.path().join("apps/desktop/e2e/table/table.spec.ts");
    let src = std::fs::read_to_string(&p).unwrap().replace(
        "test.fixme(process.env.SKIP_BROWSER === '1', 'M4: Browserfunktion');",
        "",
    );
    std::fs::write(p, src).unwrap();
    let cargo = std::fs::read_to_string(data().join("cargo.txt"))
        .unwrap()
        .replace("ignored, M3: spätere Funktion", "ok")
        .replace("ignored, wartet auf M2-E03", "ok");
    let p = f.dir.path().join("cargo.txt");
    std::fs::write(&p, cargo).unwrap();
    f.args.test_output = Some(p);
    let xml = std::fs::read_to_string(data().join("playwright.xml"))
        .unwrap()
        .replace("<skipped message=\"M4: Browserfunktion\"/>", "");
    let p = f.dir.path().join("playwright.xml");
    std::fs::write(&p, xml).unwrap();
    f.args.frontend_junit[1] = p;
    let p = f.dir.path().join("later.xml");
    std::fs::write(&p, r#"<testsuite name="src/features/later/later.test.ts"><testcase name="zukünftig"/><testcase name="später kopieren"/></testsuite>"#).unwrap();
    f.args.frontend_junit.push(p);
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["summary"]["implemented"], 9);
    assert_eq!(doc["summary"]["open"], 0);
    assert_eq!(doc["blocking_appendix_b"], 0);
}

#[test]
fn c9_rejects_a_placeholder_in_a_crate_without_owned_tests() {
    let f = Fixture::new();
    let p = f.dir.path().join("docs/plan/M2-packages.json");
    let mut packages: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    packages[0]["owns"] = serde_json::json!(["crates/*/tests/pending_w1_d12.rs"]);
    std::fs::write(p, packages.to_string()).unwrap();
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["placeholders"], serde_json::json!([]));
    assert_eq!(
        doc["ignored_without_valid_reason"][0]["reason"],
        "wartet auf M2-E03"
    );
}

#[test]
fn c9_appendix_placeholders_warn_in_development_but_block_the_gate() {
    let mut f = Fixture::new();
    f.append(
        "crates/alpha/tests/pending_w1_d12.rs",
        r#"
// spec: TECH-001 [M2] [P1]
#[test]
#[ignore = "wartet auf M2-E03"]
fn pending_criterion() { assert_eq!(2+2,4); }
"#,
    );
    let p = f.dir.path().join("docs/plan/M2-packages.json");
    let mut packages: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    packages[0]["extra_spec_ids"] = serde_json::json!(["OUT-020", "TECH-001"]);
    std::fs::write(p, packages.to_string()).unwrap();
    let output = std::fs::read_to_string(data().join("cargo.txt")).unwrap()
        + "test pending_criterion ... ignored, wartet auf M2-E03\n";
    let p = f.dir.path().join("cargo.txt");
    std::fs::write(&p, output).unwrap();
    f.args.test_output = Some(p);
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["appendix_b"]["TECH-001"]["M2-D12"]["status"], "open");
    f.args.gate = true;
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["blocking_appendix_b"], 2);
}

#[test]
fn c9_recognizes_crate_wide_test_ownership() {
    let f = Fixture::new();
    let p = f.dir.path().join("docs/plan/M2-packages.json");
    let mut packages: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    packages[0]["owns"] = serde_json::json!(["crates/alpha/** (ohne Cargo.toml)"]);
    std::fs::write(p, packages.to_string()).unwrap();
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["placeholders"].as_array().unwrap().len(), 1);
}

#[test]
fn template_titles_group_cases_and_fail_if_any_instance_fails() {
    let mut f = Fixture::new();
    f.append(
        "apps/desktop/src/features/table/table.test.ts",
        r#"
describe(`Theme ${theme}`, () => {
  // spec: UI-001
  test(`Kontrast ${size}`, () => {});
});
"#,
    );
    let p = f.dir.path().join("templates.xml");
    std::fs::write(&p, r#"<testsuite name="src/features/table/table.test.ts"><testcase name="Theme light &gt; Kontrast small"/><testcase name="Theme dark &gt; Kontrast large"/></testsuite>"#).unwrap();
    f.args.frontend_junit.push(p.clone());
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["specs"]["UI-001"]["tests"][1]["result"], "ok");
    std::fs::write(&p, r#"<testsuite name="src/features/table/table.test.ts"><testcase name="Theme light &gt; Kontrast small"/><testcase name="Theme dark &gt; Kontrast large"><failure/></testcase></testsuite>"#).unwrap();
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["specs"]["UI-001"]["status"], "open");
    assert_eq!(doc["specs"]["UI-001"]["tests"][1]["result"], "failed");
}

#[test]
fn overlapping_template_patterns_are_ambiguous() {
    let mut f = Fixture::new();
    f.append(
        "apps/desktop/src/features/table/table.test.ts",
        r#"
// spec: UI-001
test(`Theme ${theme}`, () => {});
// spec: UI-002
test(`Theme ${other}`, () => {});
"#,
    );
    let p = f.dir.path().join("ambiguous.xml");
    std::fs::write(&p, r#"<testsuite name="src/features/table/table.test.ts"><testcase name="Theme light"/></testsuite>"#).unwrap();
    f.args.frontend_junit.push(p);
    let (_, doc) = f.check();
    for id in ["UI-001", "UI-002"] {
        assert_eq!(doc["specs"][id]["tests"][1]["result"], "no_result");
    }
    assert!(doc["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("2 Kandidaten")));
}

#[test]
fn development_defaults_to_wave_zero_and_requires_only_selected_packages() {
    let mut f = Fixture::new();
    f.args.wave = 0;
    f.args.test_output = None;
    f.args.frontend_junit.clear();
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["required_packages"], serde_json::json!([]));
    assert_eq!(doc["blocking_specs"], serde_json::json!([]));
    assert_eq!(doc["blocking_appendix_b"], 0);
    f.args.merged = vec!["d12".into()];
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["required_packages"], serde_json::json!(["M2-D12"]));
    assert_eq!(
        doc["blocking_specs"],
        serde_json::json!(["CRAWL-001", "CRAWL-002", "CRAWL-003"])
    );
    assert_eq!(doc["blocking_appendix_b"], 1);
    f.args.merged = vec!["M2-U20".into()];
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["blocking_specs"], serde_json::json!(["UI-090"]));
}

// B3-COV: keine Spec-ID (M2-Q01 ist Infrastruktur, siehe Dateikopf). Merge-Titel auf main nennen auch
// Zweige ohne Planpaket (M2-N00-SPEC-TAGS); solche IDs warnen und wählen kein Paket aus.
#[test]
fn unknown_merged_ids_warn_and_select_no_package() {
    let mut f = Fixture::new();
    f.args.wave = 0;
    f.args.test_output = None;
    f.args.frontend_junit.clear();
    f.args.merged = vec!["typo".into(), "N00".into()];
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["required_packages"], serde_json::json!([]));
    assert_eq!(doc["blocking_specs"], serde_json::json!([]));
    assert_eq!(doc["unknown_merged"], serde_json::json!(["typo", "N00"]));
    for id in ["typo", "N00"] {
        let warning = format!("unbekanntes gemergtes M2-Paket ignoriert: {id}");
        assert!(
            doc["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str() == Some(warning.as_str())),
            "{warning}"
        );
    }
    // Neben einer unbekannten ID wirkt ein bekanntes Paket wie allein.
    f.args.merged = vec!["N00".into(), "M2-U20".into()];
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["required_packages"], serde_json::json!(["M2-U20"]));
    assert_eq!(doc["blocking_specs"], serde_json::json!(["UI-090"]));
    assert_eq!(doc["unknown_merged"], serde_json::json!(["N00"]));
    f.args.merged = vec!["u20".into()];
    let (_, doc) = f.check();
    assert_eq!(doc["unknown_merged"], serde_json::json!([]));
}

// B4-M2CLOSE: keine Spec-ID (M2-Q01 ist Infrastruktur, siehe Dateikopf). node:test-Verträge unter
// integrations/tests (AGENT-041/042) zählen wie TS-Tests; ihr JUnit (in eine Suite mit dem Dateipfad gehüllt,
// wie im Workflow) liefert das Ergebnis. Andere .mjs-Dateien bleiben außen vor.
#[test]
fn node_test_contracts_under_integrations_are_scanned_and_credited() {
    let mut f = Fixture::new();
    f.args.wave = 0;
    f.args.test_output = None;
    f.args.merged = vec!["U20".into()];
    let tests = f.dir.path().join("integrations/tests");
    std::fs::create_dir_all(&tests).unwrap();
    std::fs::write(
        tests.join("contracts.test.mjs"),
        "import test from 'node:test';\n// spec: UI-090\ntest('skill format agrees', () => {});\n",
    )
    .unwrap();
    std::fs::write(
        tests.join("helper.mjs"),
        "// spec: UI-001\ntest('kein Testfile', () => {});\n",
    )
    .unwrap();
    let junit = f.dir.path().join("integrations-m2.xml");
    std::fs::write(
        &junit,
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<testsuites><testsuite name=\"integrations/tests/contracts.test.mjs\">\n\
         \t<testcase name=\"skill format agrees\" time=\"0.05\" classname=\"test\"/>\n\
         \t<!-- tests 1 -->\n</testsuite></testsuites>\n",
    )
    .unwrap();
    f.args.frontend_junit = vec![junit];
    let (_, doc) = f.check();
    assert_eq!(doc["blocking_specs"], serde_json::json!([]), "{doc:#}");
    let entries = doc["specs"]["UI-090"]["tests"].as_array().unwrap();
    assert!(entries
        .iter()
        .any(|t| t["file"] == "integrations/tests/contracts.test.mjs" && t["result"] == "ok"));
    assert!(!doc["specs"]["UI-001"]["tests"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["file"] == "integrations/tests/helper.mjs"));
}

// B4-M2CLOSE: keine Spec-ID (M2-Q01 ist Infrastruktur, siehe Dateikopf). Die Merge-Historie wählt Pakete über alle
// drei Titelformen und über `merged_via` aus; Integrations- und Hilfszweige bleiben außen vor, `unknown_merged` leer.
#[test]
fn merge_log_selects_packages_by_title_form_and_merged_via() {
    let mut f = Fixture::new();
    f.args.wave = 0;
    f.args.test_output = None;
    f.args.frontend_junit.clear();
    let p = f.dir.path().join("docs/plan/M2-packages.json");
    let mut packages: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let q03 = packages
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "M2-Q03")
        .unwrap();
    q03["merged_via"] =
        serde_json::json!({"commit": "5c70de74", "subject": "m2: Merge base-4r (Q03 freigegeben)"});
    std::fs::write(&p, packages.to_string()).unwrap();
    let log = f.dir.path().join("main-merges.txt");
    std::fs::write(
        &log,
        "\
1111111111111111111111111111111111111111\tm2: Integration Runde 6 – merge m2/u20
2222222222222222222222222222222222222222\tm2: Integration Runde 7 – D12 (C9-Auflösung)
3333333333333333333333333333333333333333\tmerge: m2/e03 (M2-E03) in Integrationsrunde 8
4444444444444444444444444444444444444444\tm2: Merge int-16
5c70de74ffffffffffffffffffffffffffffffff\tm2: Merge base-4r (Q03 freigegeben)
6666666666666666666666666666666666666666\tmerge: origin/main in m2/u10
",
    )
    .unwrap();
    f.args.merged_log = Some(log.clone());
    let (_, doc) = f.check();
    assert_eq!(
        doc["required_packages"],
        serde_json::json!(["M2-D12", "M2-E03", "M2-U20", "M2-Q03"]),
        "{doc:#}"
    );
    assert_eq!(doc["unknown_merged"], serde_json::json!([]));
    assert_eq!(
        doc["merged_packages"]["M2-Q03"],
        "merged_via 5c70de74: m2: Merge base-4r (Q03 freigegeben)"
    );
    assert_eq!(
        doc["merged_packages"]["M2-U20"],
        "m2: Integration Runde 6 – merge m2/u20"
    );
    assert_eq!(
        doc["other_merge_branches"],
        serde_json::json!({"base-4r": "m2: Merge base-4r (Q03 freigegeben)", "int-16": "m2: Merge int-16"})
    );
    assert_eq!(doc["merged_via_missing"], serde_json::json!([]));
    // Ein Beleg mit abweichendem Titel zählt nicht und wird gemeldet.
    std::fs::write(
        &log,
        "5c70de74ffffffffffffffffffffffffffffffff\tm2: Merge base-4r (anderer Titel)\n",
    )
    .unwrap();
    let (_, doc) = f.check();
    assert_eq!(doc["required_packages"], serde_json::json!([]));
    assert_eq!(doc["merged_via_missing"][0]["package"], "M2-Q03");
}

#[test]
fn legacy_ignores_warn_with_owners_until_merged_and_block_the_gate() {
    let mut f = Fixture::new();
    f.args.wave = 0;
    f.append(
        "crates/alpha/tests/check.rs",
        r#"
#[test]
#[ignore = "M2: Altgrund"]
fn legacy() {}
"#,
    );
    let path = f.dir.path().join("docs/plan/M2-packages.json");
    let mut packages: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    packages.as_array_mut().unwrap().push(serde_json::json!({
        "id": "M2-F05a", "wave": "0e", "owns": ["crates/alpha/**"]
    }));
    std::fs::write(path, packages.to_string()).unwrap();
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(
        doc["legacy_ignores"][0]["owners"],
        serde_json::json!(["M2-D12", "M2-Q03"])
    );
    assert!(doc["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("Altgrund, Besitzer M2-D12")));
    f.args.merged = vec!["D12".into()];
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["legacy_ignores"], serde_json::json!([]));
    assert_eq!(doc["ignored_without_valid_reason"][0]["test"], "legacy");
    f.args.merged.clear();
    f.args.gate = true;
    let (ok, doc) = f.check();
    assert!(!ok);
    assert!(doc["ignored_without_valid_reason"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["test"] == "legacy"));
}

#[test]
fn legacy_ignores_without_owners_are_never_waived() {
    let mut f = Fixture::new();
    f.args.wave = 0;
    std::fs::write(
        f.dir.path().join("crates/alpha/tests/unowned.rs"),
        r#"
#[test]
#[ignore = "M2: unbekannter Besitzer"]
fn unowned() {}
"#,
    )
    .unwrap();
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["ignored_without_valid_reason"][0]["test"], "unowned");
}

#[test]
fn failed_tests_of_unmerged_packages_and_untagged_tests_still_block() {
    let mut f = Fixture::new();
    f.args.wave = 0;
    let p = f.dir.path().join("cargo.txt");
    let output = std::fs::read_to_string(data().join("cargo.txt")).unwrap()
        + "test untagged_failure ... FAILED\n";
    std::fs::write(&p, output).unwrap();
    f.args.test_output = Some(p);
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["failed_tests"], serde_json::json!(["untagged_failure"]));
    let p = f.dir.path().join("failed.xml");
    std::fs::write(&p, r#"<testsuite name="src/features/later/later.test.ts"><testcase name="zukünftig"><failure/></testcase></testsuite>"#).unwrap();
    f.args.frontend_junit.push(p);
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["blocking_specs"], serde_json::json!(["UI-090"]));
}

#[test]
fn m1_ids_are_known_without_counting_towards_m2_but_typos_warn() {
    let f = Fixture::new();
    std::fs::write(
        f.dir.path().join("docs/plan/M1-packages.json"),
        r#"[
      {"id":"M1-P01","kind":"package","spec_ids":["ONPAGE-999"]}
    ]"#,
    )
    .unwrap();
    f.append(
        "crates/alpha/tests/check.rs",
        r#"
// spec: ONPAGE-999, ONPAGE-998
#[test]
fn inherited_and_typo() {}
"#,
    );
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["summary"]["total"], 9);
    assert!(doc["extra"].get("ONPAGE-999").is_none());
    let warning = doc["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|w| w.as_str().filter(|w| w.contains("in keinem Paket")))
        .unwrap();
    assert!(warning.contains("ONPAGE-998"));
    assert!(!warning.contains("ONPAGE-999"));
}

#[test]
fn junit_retries_use_the_last_identical_case_in_both_orders() {
    for (first, last, expected) in [("<failure/>", "", "ok"), ("", "<failure/>", "failed")] {
        let mut f = Fixture::new();
        let p = f.dir.path().join("retries.xml");
        std::fs::write(
            &p,
            format!(
                r#"<testsuite name="src/features/table/table.test.ts">
          <testcase name="Tabelle &gt; zeigt Zeilen &amp; Größen">{first}</testcase>
          <testcase name="Tabelle &gt; zeigt Zeilen &amp; Größen">{last}</testcase>
        </testsuite>"#
            ),
        )
        .unwrap();
        f.args.frontend_junit[0] = p;
        let (_, doc) = f.check();
        assert_eq!(doc["specs"]["UI-001"]["tests"][0]["result"], expected);
        assert_eq!(
            doc["failed_tests"].as_array().unwrap().is_empty(),
            expected == "ok"
        );
    }
}

#[test]
fn junit_browser_projects_are_independent_of_retry_order() {
    let mut f = Fixture::new();
    let p = f.dir.path().join("projects.xml");
    std::fs::write(
        &p,
        r#"<testsuites>
      <testsuite name="table/table.spec.ts" hostname="chromium">
        <testcase name="Browser › zeigt Browser"><failure/></testcase>
      </testsuite>
      <testsuite name="table/table.spec.ts" hostname="webkit">
        <testcase name="Browser › zeigt Browser"/>
      </testsuite>
    </testsuites>"#,
    )
    .unwrap();
    f.args.frontend_junit[1] = p;
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["specs"]["UI-003"]["status"], "open");
    assert_eq!(
        doc["failed_tests"],
        serde_json::json!(["Browser > zeigt Browser"])
    );
}

// spec: OUT-080
#[test]
fn sq01_one_bad_filter_does_not_block_its_siblings() {
    use serde_json::json;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let clean = json!({"export":"security_all", "url_set_identical":true,
        "missing_ref_columns":[], "missing_os_columns":[], "unmapped_ref_columns":[],
        "columns":[{"ref_column":"Status Code", "errors":0}], "applied_deviations":[]});
    let mut bad = clean.clone();
    bad["url_set_identical"] = json!(false);
    let export = |file: &str, os: &str, report: serde_json::Value| {
        json!({
        "ref_file":file,"mapping":"security_all","os":os,"status":"present","outcome":"compared","report":report})
    };
    let parity = json!({"sites":[{"site":"s","exports":[
        export("security_all.csv","tab:Security:All",clean.clone()),
        export("security_http_urls.csv","tab:Security:HTTP URLs",clean.clone()),
        export("security_bad_content_type.csv","tab:Security:Bad Content Type",bad)
    ]}]});
    let summary = os_parity::coverage::evidence::filter_summary(&root, &parity).unwrap();
    let ids = summary["verified_ids"].as_array().unwrap();
    assert!(ids.contains(&json!("ONPAGE-031")));
    assert!(!ids.contains(&json!("ONPAGE-043")));
    let mut failed_export = parity;
    failed_export["sites"][0]["exports"][1]["outcome"] = json!("error");
    let summary = os_parity::coverage::evidence::filter_summary(&root, &failed_export).unwrap();
    assert!(!summary["verified_ids"]
        .as_array()
        .unwrap()
        .contains(&json!("ONPAGE-031")));
}

// spec: ONPAGE-166
#[test]
fn historical_m1_pdf_exceptions_do_not_verify_m2_content() {
    use serde_json::json;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut parity = json!({"sites":[{"site":"legacy","exports":[{
        "ref_file":"content_all.csv","mapping":"content_all","os":"tab:Content:All",
        "status":"present","outcome":"compared","report":{
            "export":"content_all","url_set_identical":true,"columns":[],
            "missing_ref_columns":[],"missing_os_columns":[],"unmapped_ref_columns":[],
            "applied_deviations":[]
        }
    }]}]});
    let summary = os_parity::coverage::evidence::filter_summary(&root, &parity).unwrap();
    assert!(summary["verified_ids"]
        .as_array()
        .unwrap()
        .contains(&json!("ONPAGE-142")));
    for id in ["DEV-016", "DEV-022", "DEV-024"] {
        parity["sites"][0]["exports"][0]["report"]["applied_deviations"] = json!([{"id":id}]);
        let summary = os_parity::coverage::evidence::filter_summary(&root, &parity).unwrap();
        assert!(!summary["verified_ids"]
            .as_array()
            .unwrap()
            .contains(&json!("ONPAGE-142")));
    }
}

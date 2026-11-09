//! M3-Q01 ist Infrastruktur ohne eigene Spec-IDs (`spec_ids: []`). Die Tags in der Fixture
//! `tests/data/coverage_mx` belegen die Regeln aus M3.md D7/D8 (Ignore-Gründe, `[M3]`-Teilkriterien) und werden
//! nicht als Produktabdeckung gutgeschrieben.

use os_parity::coverage::{mx, run, CoverageArgs, Milestone};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/coverage_mx")
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
    fn new(milestone: Milestone) -> Self {
        let dir = tempfile::tempdir().unwrap();
        copy(&data().join("ws"), dir.path());
        let args = CoverageArgs {
            milestone,
            gate: false,
            wave: 1,
            merged: Vec::new(),
            merged_log: None,
            frontend_junit: vec![data().join("vitest.xml")],
            packages: None,
            root: dir.path().into(),
            test_output: Some(data().join("cargo.txt")),
            junit: None,
            write: Some(dir.path().join("status.json")),
            strict: false,
            parity_report: None,
            evidence: None,
        };
        Self { dir, args }
    }

    fn check(&self) -> (bool, Value) {
        let ok = run(&self.args).unwrap();
        let text = std::fs::read_to_string(self.args.write.as_ref().unwrap()).unwrap();
        (ok, serde_json::from_str(&text).unwrap())
    }

    fn append(&self, file: &str, content: &str) {
        let p = self.dir.path().join(file);
        let mut s = std::fs::read_to_string(&p).unwrap_or_default();
        s.push_str(content);
        std::fs::write(p, s).unwrap();
    }

    /// Testausgabe mit zusätzlichen Zeilen (eigene Datei im Temp-Verzeichnis).
    fn with_output(&mut self, extra: &str) {
        let mut s = std::fs::read_to_string(data().join("cargo.txt")).unwrap();
        s.push_str(extra);
        let p = self.dir.path().join("cargo-extra.txt");
        std::fs::write(&p, s).unwrap();
        self.args.test_output = Some(p);
    }
}

fn list(doc: &Value, key: &str) -> Vec<String> {
    doc[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} fehlt: {doc:#}"))
        .iter()
        .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
        .collect()
}

fn warnings(doc: &Value) -> String {
    list(doc, "warnings").join("\n")
}

// M3-Q01 Schritt 1 und 2: Status je spec_id wie M2, Teilkriterien [M3] je Paket, Rust und vitest.
#[test]
fn m3_development_reads_packages_statuses_and_partial_criteria() {
    let f = Fixture::new(Milestone::M3);
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(doc["milestone"], "M3");
    assert_eq!(doc["mode"], "development");
    assert_eq!(doc["summary"]["total"], 6);
    assert_eq!(doc["specs"]["OUT-250"]["status"], "implemented");
    // Rust-Test und vitest-Test tragen beide OUT-250.
    assert_eq!(
        doc["specs"]["OUT-250"]["tests"].as_array().unwrap().len(),
        2
    );
    assert_eq!(doc["specs"]["OUT-252"]["status"], "deferred");
    assert_eq!(
        doc["specs"]["OUT-263"]["status"], "deferred",
        "Manuell: ist gültig"
    );
    assert_eq!(doc["specs"]["OUT-251"]["status"], "open");
    assert_eq!(doc["specs"]["UI-098"]["status"], "open");
    assert_eq!(doc["specs"]["OUT-266"]["status"], "open");
    assert!(list(&doc, "blocking_specs").is_empty(), "{doc:#}");

    let parts = &doc["partial_criteria"];
    assert_eq!(parts["M3-F01"]["OUT-222"]["status"], "implemented");
    assert_eq!(parts["M3-U01"]["OUT-269"]["status"], "implemented");
    assert_eq!(
        parts["M3-U01"]["OUT-269"]["tests"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "Rust- und vitest-Test mit [M3]"
    );
    assert_eq!(parts["M3-U01"]["CRAWL-122"]["status"], "implemented");
    // Der Test ohne Marker prüft das M2-Kriterium und zählt nicht.
    assert_eq!(
        parts["M3-U01"]["CRAWL-122"]["tests"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(parts["M3-U02"]["UI-002"]["status"], "open");
    assert_eq!(parts["M3-U02"]["UI-002"]["blocking"], false);
    assert_eq!(parts["M3-Q03"]["OUT-225"]["status"], "manual");
    assert_eq!(doc["blocking_partial_criteria"], 0);

    // D7-Platzhalter im eigenen pending-File, Altgrund M3: als Warnung, fremde Gründe außerhalb.
    assert_eq!(doc["placeholders"].as_array().unwrap().len(), 1);
    assert_eq!(
        doc["placeholders"][0]["file"],
        "crates/alpha/tests/pending_m3_u02.rs"
    );
    assert_eq!(doc["legacy_ignores"].as_array().unwrap().len(), 1);
    assert_eq!(doc["legacy_ignores"][0]["test"], "help_menu_legacy");
    let outside = doc["out_of_scope_ignores"].as_array().unwrap();
    assert_eq!(outside.len(), 1, "{doc:#}");
    assert_eq!(outside[0]["test"], "foreign_reason");
    assert!(doc["ignored_without_valid_reason"]
        .as_array()
        .unwrap()
        .is_empty());
    // Bekannte IDs anderer Meilensteine (UI-004 aus M4, CRAWL-002 aus M1) sind keine Tippfehler, TEAM-001 schon.
    let w = warnings(&doc);
    assert!(
        w.contains("Tags mit IDs, die in keiner Paketliste M1–M5 stehen: TEAM-001\n")
            || w.ends_with("Tags mit IDs, die in keiner Paketliste M1–M5 stehen: TEAM-001"),
        "{w}"
    );
    assert!(list(&doc, "required_packages").contains(&"M3-R03".to_string()));
    assert!(list(&doc, "not_required_packages").contains(&"M3-U02".to_string()));

    // Deterministisch: zweiter Lauf schreibt dieselben Bytes.
    let first = std::fs::read(f.args.write.as_ref().unwrap()).unwrap();
    f.check();
    assert_eq!(
        first,
        std::fs::read(f.args.write.as_ref().unwrap()).unwrap()
    );
}

// M3-Q01: --gate verlangt alle Wellen, verbietet Platzhalter und Altgründe; Manuell: ersetzt keinen Test einer
// spec_id, wohl aber ein Teilkriterium; bedingte Pakete (Titel „Bedingt …“) zählen nur, wenn gemergt.
#[test]
fn m3_gate_requires_all_waves_and_rejects_placeholders_and_legacy_reasons() {
    let mut f = Fixture::new(Milestone::M3);
    f.args.gate = true;
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(doc["mode"], "gate");
    let blocking = list(&doc, "blocking_specs");
    for id in ["OUT-251", "OUT-252", "OUT-263", "UI-098"] {
        assert!(blocking.contains(&id.to_string()), "{id}: {blocking:?}");
    }
    assert!(!blocking.contains(&"OUT-250".to_string()));
    assert!(
        !blocking.contains(&"OUT-266".to_string()),
        "bedingtes Paket ohne Merge"
    );
    assert!(warnings(&doc).contains("offene ID OUT-266"));
    let bad: Vec<_> = doc["ignored_without_valid_reason"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["test"].as_str().unwrap().to_string())
        .collect();
    assert!(bad.contains(&"help_menu_legacy".to_string()), "{bad:?}");
    assert!(
        bad.contains(&"install_after_download".to_string()),
        "{bad:?}"
    );
    assert!(doc["placeholders"].as_array().unwrap().is_empty());
    assert_eq!(
        doc["partial_criteria"]["M3-U02"]["UI-002"]["blocking"],
        true
    );
    assert_eq!(
        doc["partial_criteria"]["M3-Q03"]["OUT-225"]["blocking"],
        false
    );
    // UI-002 (U02) und OUT-273 (R04, ohne Release-JUnit); OUT-225 von Q03 ist ein Protokoll.
    assert_eq!(doc["blocking_partial_criteria"], 2);
    assert_eq!(
        doc["partial_criteria"]["M3-R04"]["OUT-273"]["blocking"],
        true
    );

    // Mit Merge-Beleg zählt das bedingte Paket.
    f.args.merged = vec!["X01".into()];
    let (_, doc) = f.check();
    assert!(list(&doc, "blocking_specs").contains(&"OUT-266".to_string()));
}

// M3.md D7 / Paket M3-Q01 Schritt 3: gültige Ignore-Gründe je Meilenstein.
#[test]
fn valid_reasons_follow_d7_per_milestone() {
    for reason in [
        "M4: KI-Teil",
        "M5: DataForSEO",
        "P2: später",
        "Klärung K3-07: MSI-Version",
        "Klärung K4-02",
        "Klärung K5-01 (evnxt)",
        "Manuell: OUT-225 – Protokoll M3-Q03",
    ] {
        assert!(mx::valid_defer_reason(3, reason), "{reason}");
    }
    for reason in [
        "M3: eigener Meilenstein",
        "M2: alt",
        "Klärung K2-12: M2-Klärung",
        "Klärung K3-7x",
        "Klärung K6-01",
        "wartet auf M3-U01",
        "Benchmark (P28): neutral, getrennt behandelt",
        "",
    ] {
        assert!(!mx::valid_defer_reason(3, reason), "{reason}");
    }
    assert!(!mx::valid_defer_reason(4, "M4: eigener Meilenstein"));
    assert!(mx::valid_defer_reason(4, "M5: später"));
    assert!(!mx::valid_defer_reason(5, "M5: eigener Meilenstein"));
    assert!(mx::valid_defer_reason(5, "Klärung K5-02"));
}

// M3-Q01: Ungültige Gründe, falsche Platzhalter-Orte und Ignores ohne Grund machen den Lauf rot.
#[test]
fn invalid_reasons_and_misplaced_placeholders_fail_in_development() {
    let mut f = Fixture::new(Milestone::M3);
    f.append(
        "apps/desktop/e2e/viz/v.spec.ts",
        "// spec: UI-098\ntest.fixme('Kontextverlust', 'WebGL 2 fehlt in dieser Umgebung');\n",
    );
    f.append(
        "crates/alpha/tests/check.rs",
        "\n// spec: OUT-250\n#[test]\n#[ignore = \"wartet auf M3-U02\"]\nfn placeholder_in_wrong_file() {}\n\
         \n#[test]\n#[ignore]\nfn without_reason() {}\n",
    );
    f.with_output(
        "     Running tests/check.rs (target/debug/deps/check-abcd)\n\
         test placeholder_in_wrong_file ... ignored, wartet auf M3-U02\n\
         test without_reason ... ignored\n",
    );
    let (ok, doc) = f.check();
    assert!(!ok);
    let bad = doc["ignored_without_valid_reason"].as_array().unwrap();
    let problem = |test: &str| {
        bad.iter()
            .find(|b| b["test"] == test)
            .unwrap_or_else(|| panic!("{test} fehlt: {bad:#?}"))["problem"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(problem("Kontextverlust").contains("kein gültiger M3-Ignore-Grund"));
    assert!(problem("placeholder_in_wrong_file").contains("Platzhalter"));
    assert!(problem("without_reason").contains("ohne Grund"));
    // OUT-250 bleibt implementiert (der grüne Test genügt), UI-098 offen, aber ohne gemergten Besitzer.
    assert_eq!(doc["specs"]["OUT-250"]["status"], "implemented");
    assert!(list(&doc, "blocking_specs").is_empty());
}

// M3-Q01: Ein roter Test blockiert immer, auch wenn sein Paket noch nicht gemergt ist.
#[test]
fn failed_tests_block_even_for_unmerged_owners() {
    let mut f = Fixture::new(Milestone::M3);
    f.append(
        "crates/alpha/tests/pending_m3_u02.rs",
        "\n// spec: OUT-251\n#[test]\nfn installs() {}\n",
    );
    f.with_output(
        "     Running tests/pending_m3_u02.rs (target/debug/deps/pending_m3_u02-abcd)\n\
         test installs ... FAILED\n",
    );
    let (ok, doc) = f.check();
    assert!(!ok);
    assert_eq!(list(&doc, "blocking_specs"), ["OUT-251"]);
    assert_eq!(list(&doc, "failed_tests"), ["installs"]);
}

// M3-Q01 Schritt 1: gemergte Pakete aus der Merge-Historie; Reihe B bekannt, fremde Zweige gelistet.
#[test]
fn merged_log_selects_packages_and_knows_series() {
    let mut f = Fixture::new(Milestone::M3);
    f.args.wave = 0;
    f.args.merged_log = Some(data().join("merges.txt"));
    f.args.merged = vec!["B3-COV".into(), "ZZ9".into()];
    let (_, doc) = f.check();
    assert_eq!(
        doc["merged_packages"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["M3-U01", "M3-V02"]
    );
    assert_eq!(
        doc["series_packages"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["B4-LAGFIX"]
    );
    assert_eq!(
        doc["other_merge_branches"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["fix-e", "m2/n34"]
    );
    // Nur ZZ9 ist unbekannt; B3-COV gehört zur Reihe B.
    assert_eq!(list(&doc, "unknown_merged"), ["ZZ9"]);
    let required = list(&doc, "required_packages");
    assert!(required.contains(&"M3-U01".to_string()));
    assert!(required.contains(&"M3-V02".to_string()));
    assert!(!required.contains(&"M3-R03".to_string()));
    // V02 ist gemergt, UI-098 ohne Ergebnis blockiert jetzt.
    assert!(list(&doc, "blocking_specs").contains(&"UI-098".to_string()));
}

// M3-Q01 Schritt 2: dieselbe Fixture für M4 – nur [M4]-Marker zählen, M4:-Gründe sind dort Altgründe.
#[test]
fn m4_uses_its_own_package_list_marker_and_reasons() {
    let f = Fixture::new(Milestone::M4);
    let (ok, doc) = f.check();
    assert!(!ok, "AI-001 hat keinen Test");
    assert_eq!(doc["milestone"], "M4");
    assert_eq!(doc["summary"]["total"], 1);
    assert_eq!(list(&doc, "blocking_specs"), ["AI-001"]);
    assert_eq!(
        doc["partial_criteria"]["M4-F02"]["UI-004"]["status"],
        "implemented"
    );
    assert_eq!(doc["partial_criteria"]["M4-F02"]["UI-004"]["marker"], "M4");
    let legacy: Vec<_> = doc["legacy_ignores"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["test"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(legacy, ["signature_later"], "M4: ist für M4 ein Altgrund");
    // M3:-Gründe gehören zu M3 und erscheinen in M4 nicht.
    assert!(doc["ignored_without_valid_reason"]
        .as_array()
        .unwrap()
        .is_empty());
}

// M3-Q01: Ohne Paketliste meldet der Lauf einen lesbaren Fehler statt Panik.
#[test]
fn missing_package_list_is_an_error() {
    let f = Fixture::new(Milestone::M5);
    let err = os_parity::coverage::run(&f.args).unwrap_err().to_string();
    assert!(err.contains("M5-packages.json"), "{err}");
}

// M3-Q01: Vertragstests der Release-Skripte (node:test unter tests/release) belegen Teilkriterien, Fixture-Ordner
// zählen nicht; ohne JUnit gibt es keine Gutschrift.
#[test]
fn release_contract_tests_count_for_partial_criteria() {
    let mut f = Fixture::new(Milestone::M3);
    f.args.merged = vec!["R04".into()];
    let (ok, doc) = f.check();
    assert!(!ok);
    let entry = &doc["partial_criteria"]["M3-R04"]["OUT-273"];
    assert_eq!(entry["status"], "open");
    assert_eq!(entry["blocking"], true);
    assert_eq!(entry["tests"].as_array().unwrap().len(), 1, "{entry:#}");
    assert_eq!(entry["tests"][0]["file"], "tests/release/rel.test.mjs");

    f.args.frontend_junit.push(data().join("release.xml"));
    let (ok, doc) = f.check();
    assert!(ok, "{doc:#}");
    assert_eq!(
        doc["partial_criteria"]["M3-R04"]["OUT-273"]["status"],
        "implemented"
    );
}

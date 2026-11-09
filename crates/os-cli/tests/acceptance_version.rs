//! Unabhängige Abnahme B4-UPDDLG (Testagent; Grundlage allein die Spec, Bauer-Bericht nicht gelesen).
//!
//! Spec: OUT-181 „`--version` gibt `openspider <semver> (<git-hash>, <os>-<arch>)` aus; mit Build-Nummer (UI-127,
//! Nutzerentscheidung 09.11.2026) `openspider <semver> (Build <n>, <git-hash>, <os>-<arch>)`“, UI-127 „Die Build-Nummer ist
//! die Anzahl der Commits bis zum gebauten Stand … in Entwickler-Builds (Debug) fehlt die Build-Nummer, statt einer falschen
//! Zahl“; „Version entspricht der Paketversion“. Aufruf nur über das gebaute Binary.
//!
//! Die Nummer ist eine Build-Eingabe (`OPENSPIDER_BUILD_NUMBER` des Release-Workflows). Dieselbe Umgebungsvariable gilt beim
//! Bauen und beim Aufruf des Tests:
//!   `cargo test -p os-cli --test acceptance_version`                          → Entwickler-Build, ohne Nummer
//!   `OPENSPIDER_BUILD_NUMBER=2578 cargo test -p os-cli --test acceptance_version` → mit Nummer 2578

mod common;

use common::Env;

fn workspace_version() -> String {
    let cargo =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml")).unwrap();
    cargo
        .split("[workspace.package]")
        .nth(1)
        .and_then(|section| {
            section
                .lines()
                .find_map(|l| l.trim().strip_prefix("version = \""))
        })
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("version in [workspace.package]")
        .to_owned()
}

fn build_number_from_environment() -> Option<String> {
    std::env::var("OPENSPIDER_BUILD_NUMBER")
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Zerlegt `openspider <semver> (<inneres>)` und liefert (semver, Felder im Klammertext).
fn parse(line: &str) -> (String, Vec<String>) {
    let rest = line
        .strip_prefix("openspider ")
        .unwrap_or_else(|| panic!("beginnt mit „openspider “: {line}"));
    let (version, tail) = rest
        .split_once(" (")
        .unwrap_or_else(|| panic!("Version gefolgt von „ (“: {line}"));
    let inner = tail
        .strip_suffix(')')
        .unwrap_or_else(|| panic!("endet mit „)“: {line}"));
    (
        version.to_owned(),
        inner.split(", ").map(str::to_owned).collect(),
    )
}

// spec: OUT-181
// spec: UI-127
#[test]
fn version_line_is_one_line_with_package_version_hash_and_platform() {
    let env = Env::new();
    let run = env.run(&["--version"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    assert_eq!(run.stdout.trim().lines().count(), 1, "{}", run.dump());
    let (version, fields) = parse(run.stdout.trim());
    // „Version entspricht der Paketversion“ (UI-127): die Workspace-Version, kein zweiter Pflegeort.
    assert_eq!(version, workspace_version());
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    assert_eq!(fields.last(), Some(&platform), "{fields:?}");
    // <git-hash>: nicht leer, ohne Leerzeichen; direkt davor steht die optionale Build-Nummer.
    let hash = &fields[fields.len() - 2];
    assert!(
        !hash.is_empty() && !hash.contains(' ') && !hash.starts_with("Build"),
        "{fields:?}"
    );
}

// spec: OUT-181
// spec: UI-127
#[test]
fn build_number_appears_exactly_as_given_to_the_build_and_never_in_developer_builds() {
    let env = Env::new();
    let run = env.run(&["--version"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    let (_, fields) = parse(run.stdout.trim());
    match build_number_from_environment() {
        // Mit Nummer aus dem Release-Workflow: `(Build <n>, <hash>, <os>-<arch>)`, drei Felder.
        Some(number) => {
            assert_eq!(fields.len(), 3, "{fields:?}");
            assert_eq!(fields[0], format!("Build {number}"), "{fields:?}");
        }
        // Entwickler-Build (Debug, keine Nummer): `(<hash>, <os>-<arch>)` — keine Nummer statt einer falschen Zahl.
        None if cfg!(debug_assertions) => {
            assert_eq!(fields.len(), 2, "{fields:?}");
            assert!(!run.stdout.contains("Build"), "{}", run.dump());
        }
        // Release-Profil ohne Variable: Git zählt nur im vollständigen eigenen Checkout; beide Formen sind zulässig.
        None => assert!(fields.len() == 2 || fields.len() == 3, "{fields:?}"),
    }
}

// spec: OUT-181
// spec: UI-127
#[test]
fn a_build_number_is_a_positive_integer_without_leading_zero() {
    let env = Env::new();
    let run = env.run(&["--version"]);
    let (_, fields) = parse(run.stdout.trim());
    if let Some(number) = fields[0].strip_prefix("Build ") {
        assert!(
            !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit())
                && !number.starts_with('0'),
            "{fields:?}"
        );
    }
}

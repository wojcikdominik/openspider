//! Abdeckung M3 … M5 (M3-Q01): `os-parity coverage --milestone M3|M4|M5` über `docs/plan/M<n>-packages.json`.
//!
//! Status je `spec_id` wie in M2 (`implemented`/`verified`/`deferred`/`open`), Rust- und Frontend-Tags, JUnit aus
//! vitest/Playwright. Statt Anhang B gibt es die **Teilkriterien** je Paket: jede ID aus `extra_spec_ids` braucht
//! einen grünen Test mit `// spec: <ID> [M<n>]` in einer Datei des Pakets (M3.md D8, M4.md E7, M5.md G6).
//! Pakete mit `"human": true` belegen ihre Teilkriterien über ein Protokoll (`manual`, nie blockierend).
//!
//! Gültige Ignore-Gründe (M3.md D7, Paket M3-Q01): `M<k>:` für spätere Meilensteine, `P2:`,
//! `Klärung K3-nn`/`K4-nn`/`K5-nn`, `Benchmark` (neutral) und `Manuell:` (Protokoll M3-Q03 bzw. M<n>-Q02).
//! `M<n>:` (Zurückstellung auf den eigenen Meilenstein, z. B. aus M2) ist in der Entwicklung ein Altgrund
//! (`legacy_ignores`, Warnung) und im Gate ungültig. Platzhalter `wartet auf M<n>-xx` nur in
//! `crates/*/tests/pending_m<n>_<kurz-id>.rs` bzw. im eigenen E2E-Ordner, im Gate verboten.
//!
//! Geprüft werden Tests mit IDs dieses Meilensteins sowie Ignores mit `M<n>:`, `wartet auf M<n>-…` oder ohne
//! Grund. Ignores fremder Tests mit Gründen früherer Meilensteine prüfen deren Läufe (M1/M2); sie erscheinen
//! hier nur als `out_of_scope_ignores`. Die Merge-Historie (`--merged-log`) erkennt Pakete des Meilensteins und
//! die Reihen B3, B4 … (`merged.rs`). Bedingte Pakete (Titel beginnt mit „Bedingt“) sind auch im Gate nur Pflicht,
//! wenn sie gemergt sind. Alle Berichte sind deterministisch.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::merged::{self, MergedVia};
use super::{
    compute_with_reason, frontend, is_benchmark_reason, m2, results, scan, targets, BadIgnore,
    CoverageArgs, PackageSpecs, Status, TestEntry, TestState,
};
use crate::error::ParityError;

#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    wave: Value,
    #[serde(default)]
    spec_ids: Vec<String>,
    #[serde(default)]
    extra_spec_ids: Vec<String>,
    #[serde(default)]
    owns: Vec<String>,
    #[serde(default)]
    human: bool,
    #[serde(default)]
    merged_via: Option<MergedVia>,
}

impl Package {
    fn wave_number(&self) -> Option<u64> {
        self.wave.as_u64().or_else(|| {
            self.wave
                .as_str()?
                .trim_end_matches(char::is_alphabetic)
                .parse()
                .ok()
        })
    }
    fn short(&self, n: u8) -> &str {
        let prefix = format!("M{n}-");
        self.id
            .get(..prefix.len())
            .filter(|p| p.eq_ignore_ascii_case(&prefix))
            .map_or(self.id.as_str(), |_| &self.id[prefix.len()..])
    }
    fn is(&self, id: &str, n: u8) -> bool {
        self.id.eq_ignore_ascii_case(id) || self.short(n).eq_ignore_ascii_case(id)
    }
    /// Bedingte Pakete (Titel „Bedingt (K4-02): …“, M4.md B2 „bedingte IDs nur, wenn umgesetzt“).
    fn conditional(&self) -> bool {
        self.title.trim_start().starts_with("Bedingt")
    }
    fn required(&self, args: &CoverageArgs, n: u8) -> bool {
        let merged = args.merged.iter().any(|id| self.is(id, n));
        if self.conditional() {
            return merged;
        }
        args.gate
            || self
                .wave_number()
                .is_some_and(|w| w <= u64::from(args.wave))
            || merged
    }
    fn owns(&self, file: &str) -> bool {
        self.owns.iter().any(|p| m2::owns(p, file))
    }
}

fn load(path: &Path) -> Result<Vec<Package>, ParityError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ParityError::Invalid(format!("{} nicht lesbar: {e}", path.display())))?;
    let doc: Value = serde_json::from_str(&text)
        .map_err(|e| ParityError::Invalid(format!("{}: {e}", path.display())))?;
    serde_json::from_value(doc.get("packages").unwrap_or(&doc).clone())
        .map_err(|e| ParityError::Invalid(format!("{}: {e}", path.display())))
}

fn klaerung(reason: &str, k: u8) -> bool {
    reason
        .strip_prefix(&format!("Klärung K{k}-"))
        .is_some_and(|tail| {
            let d = tail.chars().take_while(char::is_ascii_digit).count();
            d > 0
                && tail[d..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric())
        })
}

/// Gültige Zurückstellung für Meilenstein `n` (3–5): `M<k>:` mit k > n, `P2:`, `Klärung K3-nn`/`K4-nn`/`K5-nn`,
/// `Manuell:`. `Benchmark` ist gültig, aber neutral und wird getrennt behandelt.
pub fn valid_defer_reason(n: u8, reason: &str) -> bool {
    let r = reason.trim();
    ((n + 1)..=5).any(|k| r.starts_with(&format!("M{k}:")))
        || r.starts_with("P2:")
        || r.starts_with("Manuell:")
        || (3..=5).any(|k| klaerung(r, k))
}

/// Grund eines früheren Meilensteins (`M1:` … `M<n-1>:`, `Klärung K-nn`/`K2-nn`, `wartet auf M1-/M2-…`).
fn earlier_reason(n: u8, reason: &str) -> bool {
    let r = reason.trim();
    (1..n).any(|k| r.starts_with(&format!("M{k}:")) || r.starts_with(&format!("wartet auf M{k}-")))
        || ["Klärung K-", "Klaerung K-", "Klärung K2-"]
            .iter()
            .any(|p| {
                r.strip_prefix(p)
                    .is_some_and(|t| t.starts_with(|c: char| c.is_ascii_digit()))
            })
        || super::m2::valid_defer_reason(r)
}

/// D7: `wartet auf M<n>-xx` nur in `crates/<crate>/tests/pending_m<n>_<kurz-id>.rs` eines anderen Pakets oder im
/// E2E-Ordner eines anderen Pakets (`apps/desktop/e2e/…`, über `owns`).
fn placeholder(file: &str, reason: &str, packages: &[Package], n: u8) -> bool {
    let Some(target) = reason
        .trim()
        .strip_prefix(&format!("wartet auf M{n}-"))
        .map(|t| {
            t.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .next()
                .unwrap_or("")
        })
    else {
        return false;
    };
    let Some(target) = packages.iter().find(|p| p.is(target, n)) else {
        return false;
    };
    let others = packages.iter().filter(|p| p.id != target.id);
    if let Some(rest) = file.strip_prefix("crates/") {
        let mut parts = rest.split('/');
        let (Some(_crate), Some("tests"), Some(name), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return false;
        };
        let Some(short) = name
            .strip_prefix(&format!("pending_m{n}_"))
            .and_then(|s| s.strip_suffix(".rs"))
        else {
            return false;
        };
        return others
            .into_iter()
            .any(|p| p.short(n).eq_ignore_ascii_case(short));
    }
    file.starts_with("apps/desktop/e2e/") && others.into_iter().any(|p| p.owns(file))
}

/// Status eines Teilkriteriums (`extra_spec_ids`, Tag `[M<n>]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum PartStatus {
    Implemented,
    Deferred,
    /// Nachweis über ein Protokoll des menschlichen Pakets (D11), kein Test verlangt.
    Manual,
    Open,
}

#[derive(Debug, Serialize)]
struct PartEntry {
    status: PartStatus,
    marker: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    tests: Vec<TestEntry>,
    required_now: bool,
    blocking: bool,
}

fn entries<'a>(report: &'a super::CoverageReport, id: &str) -> &'a [TestEntry] {
    report
        .specs
        .get(id)
        .or_else(|| report.extra.get(id))
        .map_or(&[], |e| e.tests.as_slice())
}

fn full_name(t: &scan::TestFn) -> String {
    t.module
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(t.name.as_str()))
        .collect::<Vec<_>>()
        .join("::")
}

fn has_marker(
    test: &TestEntry,
    id: &str,
    marker: &str,
    scans: &BTreeMap<String, scan::FileScan>,
) -> bool {
    scans
        .get(&test.file)
        .into_iter()
        .flat_map(|s| &s.tests)
        .any(|source| {
            full_name(source) == test.test
                && source
                    .criteria
                    .get(id)
                    .is_some_and(|ms| ms.iter().any(|m| m == marker))
        })
}

/// IDs aller Paketlisten M1 … M5 unter `root` (bekannte IDs, keine Tippfehlerwarnung).
fn known_ids(root: &Path) -> Result<BTreeSet<String>, ParityError> {
    let mut ids = BTreeSet::new();
    for k in 1..=5 {
        let path = root.join(format!("docs/plan/M{k}-packages.json"));
        if path.exists() {
            for p in load(&path)? {
                ids.extend(p.spec_ids.into_iter().chain(p.extra_spec_ids));
            }
        }
    }
    Ok(ids)
}

/// Vertragstests der Release-Skripte (node:test, `*.test.mjs` unter `tests/release` und `scripts/release`).
/// Sie tragen Teilkriterien von R04, P01 und Q02 (`// spec: OUT-261 [M3]`); nur M3 … M5 lesen sie, damit
/// `--milestone M2` unverändert bleibt. Ergebnisse kommen als JUnit mit dem Dateipfad als Suite (`--frontend-junit`).
pub const RELEASE_TEST_DIRS: [&str; 2] = ["tests/release", "scripts/release"];

fn release_scans(root: &Path) -> Result<BTreeMap<String, scan::FileScan>, ParityError> {
    fn walk(
        root: &Path,
        rel: &str,
        out: &mut BTreeMap<String, scan::FileScan>,
    ) -> Result<(), ParityError> {
        if !root.join(rel).is_dir() {
            return Ok(());
        }
        let mut entries = std::fs::read_dir(root.join(rel))?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("{rel}/{name}");
            if entry.file_type()?.is_dir() {
                if !matches!(name.as_str(), "node_modules" | "fixtures") {
                    walk(root, &path, out)?;
                }
            } else if name.ends_with(".test.mjs") {
                let text = std::fs::read_to_string(root.join(&path))?;
                out.insert(path.clone(), frontend::scan_source(&path, &text));
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    for dir in RELEASE_TEST_DIRS {
        walk(root, dir, &mut out)?;
    }
    Ok(out)
}

/// Führt die Abdeckung für Meilenstein `n` (3–5) aus. `Ok(true)` = nichts blockiert.
pub fn run(args: &CoverageArgs, n: u8) -> Result<bool, ParityError> {
    let root = &args.root;
    let ms = format!("M{n}");
    let path = args
        .packages
        .clone()
        .unwrap_or_else(|| root.join(format!("docs/plan/{ms}-packages.json")));
    let packages = load(&path)?;
    let detection = match &args.merged_log {
        Some(log) => Some(merged::detect_milestone(
            &std::fs::read_to_string(log)?,
            n,
            packages
                .iter()
                .map(|p| (p.id.as_str(), p.merged_via.as_ref())),
        )),
        None => None,
    };
    let mut effective = args.clone();
    if let Some(d) = &detection {
        effective.merged.extend(
            d.merged
                .keys()
                .filter(|id| !args.merged.contains(id))
                .cloned(),
        );
    }
    let args = &effective;
    // Unbekannte IDs aus --merged sind eine Warnung; Pakete der Reihen B3, B4 … sind bekannt.
    let unknown_merged: Vec<String> = args
        .merged
        .iter()
        .filter(|id| !merged::is_series_id(id) && !packages.iter().any(|p| p.is(id, n)))
        .cloned()
        .collect();

    let mut package_specs = PackageSpecs::default();
    for p in &packages {
        // Alle Planeinträge mit spec_ids zählen, auch Mess-, Spike- und Release-Pakete (wie M2).
        for (ids, dest) in [
            (&p.spec_ids, &mut package_specs.specs),
            (&p.extra_spec_ids, &mut package_specs.extra),
        ] {
            for id in ids {
                let owners = dest.entry(id.clone()).or_default();
                if !owners.contains(&p.id) {
                    owners.push(p.id.clone());
                }
            }
        }
    }
    package_specs
        .extra
        .retain(|id, _| !package_specs.specs.contains_key(id));
    let current: BTreeSet<&String> = package_specs
        .specs
        .keys()
        .chain(package_specs.extra.keys())
        .collect();

    let mut scans = BTreeMap::new();
    for dir in ["crates", "tests"] {
        for file in scan::rust_files(root, dir) {
            scans.insert(file.clone(), scan::scan_file(root, &file)?);
        }
    }
    let mut frontend_scans = frontend::scan(root)?;
    frontend_scans.extend(release_scans(root)?);
    let mut target_list = targets::discover_targets(root)?;
    target_list.extend(frontend::targets(&frontend_scans));
    scans.extend(frontend_scans.clone());

    let mut warnings = Vec::new();
    let mut test_results = Vec::new();
    if let Some(p) = &args.test_output {
        let bytes = std::fs::read(p)
            .map_err(|e| ParityError::Invalid(format!("{} nicht lesbar: {e}", p.display())))?;
        test_results.extend(results::parse_cargo_output(&String::from_utf8_lossy(
            &bytes,
        )));
    }
    if let Some(p) = &args.junit {
        test_results.extend(results::parse_junit(&std::fs::read_to_string(p)?)?);
    }
    for p in &args.frontend_junit {
        // B4-M2GATE: read_junit liefert das Browserprojekt mit (Skip-Regel in m2.rs); hier nur die Ergebnisse.
        // Ob die Regel für bedingte Skips auch hier gilt, entscheidet M3-Q01 (Folgepunkt).
        test_results.extend(
            frontend::read_junit(root, p, &frontend_scans, &mut warnings)?
                .into_iter()
                .map(|(result, _project)| result),
        );
    }
    // Statusdateien anderer Meilensteine geben keine Paritätsgutschrift.
    let previous: Option<Value> = args
        .write
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(|v| v["milestone"] == ms.as_str());

    // Nur IDs dieses Meilensteins sind für den Status sichtbar; IDs anderer Paketlisten sind bekannt.
    let known = known_ids(root)?;
    let mut unknown_tags = BTreeSet::new();
    let mut coverage_scans = scans.clone();
    for test in coverage_scans.values_mut().flat_map(|s| &mut s.tests) {
        for id in &test.ids {
            if !known.contains(id) && !current.contains(id) {
                unknown_tags.insert(id.clone());
            }
        }
        test.ids.retain(|id| current.contains(id));
    }
    let valid = |r: &str| valid_defer_reason(n, r);
    let mut report = compute_with_reason(
        &package_specs,
        &coverage_scans,
        target_list,
        &test_results,
        previous.as_ref(),
        &valid,
    );
    super::apply_parity(&mut report, args)?;
    report.warnings.extend(warnings);
    if !unknown_tags.is_empty() {
        report.warnings.push(format!(
            "Tags mit IDs, die in keiner Paketliste M1–M5 stehen: {}",
            unknown_tags.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    report.warnings.extend(
        unknown_merged
            .iter()
            .map(|id| format!("unbekanntes gemergtes {ms}-Paket ignoriert: {id}")),
    );
    if let Some(d) = &detection {
        report
            .warnings
            .extend(d.missing_via.iter().map(|(id, why)| format!("{id}: {why}")));
    }
    for entry in report.specs.values_mut().chain(report.extra.values_mut()) {
        if entry
            .note
            .as_deref()
            .is_some_and(|note| note.contains("Grund fehlt"))
        {
            entry.note = Some(format!(
                "alle getaggten Tests ignoriert, kein gültiger {ms}-Zurückstellungsgrund"
            ));
        }
    }

    // Ignores: Tests mit IDs dieses Meilensteins, eigene Altgründe, eigene Platzhalter, leere Gründe.
    report.ignored_without_valid_reason.clear();
    let own_prefix = format!("{ms}:");
    let mut placeholders = Vec::new();
    let mut legacy_ignores = Vec::new();
    let mut out_of_scope = Vec::new();
    for t in scans.values().flat_map(|s| &s.tests) {
        let name = full_name(t);
        // Zu diesem Meilenstein gehört ein Test mit einer eigenen spec_id oder mit einer Zusatz-ID samt
        // Marker `[M<n>]`; Tests einer Zusatz-ID ohne Marker prüfen Kriterien früherer Meilensteine.
        let tagged_current = t.ids.iter().any(|id| {
            package_specs.specs.contains_key(id)
                || (package_specs.extra.contains_key(id)
                    && t.criteria.get(id).is_some_and(|m| m.contains(&ms)))
        });
        let runtime = t
            .ids
            .iter()
            .flat_map(|id| entries(&report, id))
            .find(|e| e.file == t.file && e.test == name && e.result == TestState::Ignored);
        let empty = String::new();
        let runtime_reason = runtime.map(|e| e.ignore_reason.clone().unwrap_or_default());
        let Some(reason) = t
            .ignore
            .as_ref()
            .or(runtime_reason.as_ref())
            .or(runtime.is_some().then_some(&empty))
        else {
            continue;
        };
        let r = reason.trim();
        if is_benchmark_reason(r) || valid_defer_reason(n, r) {
            continue;
        }
        let bad = |problem: &str| BadIgnore {
            file: t.file.clone(),
            test: name.clone(),
            line: t.line,
            reason: reason.clone(),
            spec_ids: t.ids.clone(),
            problem: problem.into(),
        };
        if r.is_empty() {
            report
                .ignored_without_valid_reason
                .push(bad("#[ignore] bzw. test.fixme/skip ohne Grund"));
        } else if r.starts_with(&own_prefix) {
            if args.gate {
                report.ignored_without_valid_reason.push(bad(&format!(
                    "Altgrund {own_prefix} im Gate (Zurückstellung auf den eigenen Meilenstein)"
                )));
            } else {
                let owners: Vec<_> = t
                    .ids
                    .iter()
                    .filter_map(|id| package_specs.specs.get(id).or(package_specs.extra.get(id)))
                    .flatten()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                report.warnings.push(format!(
                    "Altgrund {own_prefix} {}:{}: {reason}",
                    t.file, t.line
                ));
                legacy_ignores.push(json!({"file": t.file, "test": name, "line": t.line,
                    "reason": reason, "spec_ids": t.ids, "owners": owners}));
            }
        } else if r.starts_with(&format!("wartet auf {ms}-")) {
            if !args.gate && placeholder(&t.file, r, &packages, n) {
                report
                    .warnings
                    .push(format!("D7-Platzhalter {}:{}: {reason}", t.file, t.line));
                placeholders.push(json!({"file": t.file, "test": name, "reason": reason}));
            } else {
                report
                    .ignored_without_valid_reason
                    .push(bad("Platzhalter außerhalb der D7-Orte oder im Gate"));
            }
        } else if tagged_current {
            report
                .ignored_without_valid_reason
                .push(bad(&format!("kein gültiger {ms}-Ignore-Grund (M3.md D7)")));
        } else if !earlier_reason(n, r) {
            out_of_scope.push(json!({"file": t.file, "test": name, "line": t.line,
                "reason": reason, "spec_ids": t.ids}));
        }
    }
    report
        .ignored_without_valid_reason
        .sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));

    let is_placeholder = |t: &TestEntry| {
        t.result == TestState::Ignored
            && t.ignore_reason
                .as_deref()
                .is_some_and(|r| placeholder(&t.file, r, &packages, n))
    };
    let mut blocking = Vec::new();
    for (id, entry) in &report.specs {
        let implemented = matches!(entry.status, Status::Implemented | Status::Verified);
        let required_now = packages
            .iter()
            .any(|p| p.spec_ids.contains(id) && p.required(args, n));
        let regular: Vec<_> = entry
            .tests
            .iter()
            .filter(|t| !t.ignore_reason.as_deref().is_some_and(is_benchmark_reason))
            .collect();
        let only_placeholders = !regular.is_empty() && regular.iter().all(|t| is_placeholder(t));
        if !implemented && (args.gate || entry.status == Status::Open) {
            if required_now && !(only_placeholders && !args.gate) {
                blocking.push(id.clone());
            } else {
                report.warnings.push(format!(
                    "offene ID {id}: Besitzer noch nicht gemergt oder D7-Platzhalter (Entwicklung)"
                ));
            }
        }
        // Ein tatsächlich fehlgeschlagener Test bleibt in jedem Modus ein Fehler.
        if entry.tests.iter().any(|t| t.result == TestState::Failed) && !blocking.contains(id) {
            blocking.push(id.clone());
        }
    }

    // Teilkriterien je Paket.
    let marker = ms.clone();
    let mut parts: BTreeMap<String, BTreeMap<String, PartEntry>> = BTreeMap::new();
    let mut parts_blocking = 0usize;
    for p in &packages {
        for id in &p.extra_spec_ids {
            let tests: Vec<TestEntry> = entries(&report, id)
                .iter()
                .filter(|t| p.owns(&t.file) && has_marker(t, id, &marker, &scans))
                .cloned()
                .collect();
            let regular: Vec<&TestEntry> = tests
                .iter()
                .filter(|t| !t.ignore_reason.as_deref().is_some_and(is_benchmark_reason))
                .collect();
            let failed = regular.iter().any(|t| t.result == TestState::Failed);
            let ok = regular.iter().any(|t| t.result == TestState::Ok);
            let deferred = !regular.is_empty()
                && regular.iter().all(|t| {
                    t.result == TestState::Ignored
                        && t.ignore_reason
                            .as_deref()
                            .is_some_and(|r| valid_defer_reason(n, r))
                });
            let manual_only = deferred
                && regular.iter().all(|t| {
                    t.ignore_reason
                        .as_deref()
                        .is_some_and(|r| r.trim().starts_with("Manuell:"))
                });
            let pending = !regular.is_empty() && regular.iter().all(|t| is_placeholder(t));
            let (status, note) = if failed {
                (
                    PartStatus::Open,
                    Some("Test mit Marker schlägt fehl (FAILED)".to_string()),
                )
            } else if ok {
                (PartStatus::Implemented, None)
            } else if deferred {
                (PartStatus::Deferred, None)
            } else if p.human {
                (
                    PartStatus::Manual,
                    Some(format!("Nachweis über das Protokoll von {} (D11)", p.id)),
                )
            } else if regular.is_empty() {
                (
                    PartStatus::Open,
                    Some(format!(
                        "kein Test mit `// spec: {id} [{marker}]` in einer Datei des Pakets"
                    )),
                )
            } else {
                (
                    PartStatus::Open,
                    Some("Tests mit Marker ohne grünes Ergebnis".to_string()),
                )
            };
            let required_now = p.required(args, n);
            let blocking_now = match status {
                PartStatus::Implemented | PartStatus::Manual => false,
                PartStatus::Deferred => args.gate && !manual_only,
                PartStatus::Open => failed || (required_now && !(pending && !args.gate)),
            };
            if blocking_now {
                parts_blocking += 1;
            } else if status == PartStatus::Open {
                report.warnings.push(format!(
                    "Teilkriterium {id} [{marker}] / {} offen (Besitzer noch nicht gemergt oder Platzhalter)",
                    p.id
                ));
            }
            parts.entry(p.id.clone()).or_default().insert(
                id.clone(),
                PartEntry {
                    status,
                    marker: marker.clone(),
                    note,
                    tests,
                    required_now,
                    blocking: blocking_now,
                },
            );
        }
    }

    // Informativ (nicht gewertet): Teilkriterium je ID über alle Dateien, ohne Besitzprüfung. Blockierend bleibt
    // die Sicht je Paket (Besitzregel wie Anhang B in M2); die Differenz ist eine Planentscheidung.
    let mut by_id: BTreeMap<String, Value> = BTreeMap::new();
    for id in packages.iter().flat_map(|p| &p.extra_spec_ids) {
        if by_id.contains_key(id) {
            continue;
        }
        let tests: Vec<&TestEntry> = entries(&report, id)
            .iter()
            .filter(|t| {
                has_marker(t, id, &marker, &scans)
                    && !t.ignore_reason.as_deref().is_some_and(is_benchmark_reason)
            })
            .collect();
        let owners: Vec<&String> = packages
            .iter()
            .filter(|p| p.extra_spec_ids.contains(id))
            .map(|p| &p.id)
            .collect();
        let status = if tests.iter().any(|t| t.result == TestState::Failed) {
            PartStatus::Open
        } else if tests.iter().any(|t| t.result == TestState::Ok) {
            PartStatus::Implemented
        } else if !tests.is_empty()
            && tests.iter().all(|t| {
                t.result == TestState::Ignored
                    && t.ignore_reason
                        .as_deref()
                        .is_some_and(|r| valid_defer_reason(n, r))
            })
        {
            PartStatus::Deferred
        } else if packages
            .iter()
            .filter(|p| p.extra_spec_ids.contains(id))
            .all(|p| p.human)
        {
            PartStatus::Manual
        } else {
            PartStatus::Open
        };
        let files: BTreeSet<&str> = tests.iter().map(|t| t.file.as_str()).collect();
        by_id.insert(
            id.clone(),
            json!({"status": status, "packages": owners, "files": files}),
        );
    }

    let failed_tests: BTreeSet<_> = test_results
        .iter()
        .filter(|t| t.outcome == results::Outcome::Failed)
        .map(|t| t.path.as_str())
        .collect();
    let passed = blocking.is_empty()
        && parts_blocking == 0
        && report.ignored_without_valid_reason.is_empty()
        && failed_tests.is_empty();
    let required: Vec<&String> = packages
        .iter()
        .filter(|p| p.required(args, n))
        .map(|p| &p.id)
        .collect();
    let not_required: Vec<&String> = packages
        .iter()
        .filter(|p| !p.required(args, n))
        .map(|p| &p.id)
        .collect();

    let mut doc = report.to_json();
    doc["milestone"] = json!(ms);
    doc["mode"] = json!(if args.gate { "gate" } else { "development" });
    doc["partial_criteria"] = json!(parts);
    doc["placeholders"] = json!(placeholders);
    doc["legacy_ignores"] = json!(legacy_ignores);
    doc["out_of_scope_ignores"] = json!(out_of_scope);
    doc["required_packages"] = json!(required);
    doc["not_required_packages"] = json!(not_required);
    doc["unknown_merged"] = json!(unknown_merged);
    if let Some(d) = &detection {
        doc["merged_packages"] = json!(d.merged);
        doc["series_packages"] = json!(d.series);
        doc["other_merge_branches"] = json!(d.other);
        doc["merged_via_missing"] = json!(d
            .missing_via
            .iter()
            .map(|(id, why)| json!({"package": id, "problem": why}))
            .collect::<Vec<_>>());
    }
    doc["blocking_specs"] = json!(blocking);
    doc["blocking_partial_criteria"] = json!(parts_blocking);
    doc["partial_criteria_by_id"] = json!(by_id);
    doc["failed_tests"] = json!(failed_tests);
    doc["passed"] = json!(passed);
    if let Some(out) = &args.write {
        if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            out,
            serde_json::to_string_pretty(&doc).map_err(|e| ParityError::Invalid(e.to_string()))?
                + "\n",
        )?;
    }

    if let Some(d) = &detection {
        println!(
            "Gemergte {ms}-Pakete laut Merge-Historie: {} (davon über merged_via: {}); Reihe B: {}; andere Merge-Zweige: {}",
            d.merged.len(),
            d.merged.values().filter(|b| b.starts_with("merged_via ")).count(),
            d.series.len(),
            d.other.len()
        );
        for (branch, subject) in &d.other {
            println!("  kein {ms}-Paket: {branch} ({subject})");
        }
    }
    println!(
        "Pflichtpakete ({}): {} von {}; noch nicht gemergt: {}",
        if args.gate { "Gate" } else { "Entwicklung" },
        required.len(),
        packages.len(),
        not_required
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    print!("{}", report.summary_text());
    for (id, e) in report
        .specs
        .iter()
        .filter(|(_, e)| e.status == Status::Deferred)
    {
        let reasons: BTreeSet<_> = e
            .tests
            .iter()
            .filter_map(|t| t.ignore_reason.as_deref())
            .collect();
        println!(
            "  deferred {id}: {}",
            reasons.into_iter().collect::<Vec<_>>().join(" | ")
        );
    }
    for name in &failed_tests {
        println!("  FAILED {name}");
    }
    for l in &legacy_ignores {
        println!(
            "  Altgrund {}:{} {}",
            l["file"].as_str().unwrap_or(""),
            l["line"],
            l["reason"].as_str().unwrap_or("")
        );
    }
    let count = |st: PartStatus| {
        parts
            .values()
            .flat_map(BTreeMap::values)
            .filter(|e| e.status == st)
            .count()
    };
    println!(
        "Teilkriterien [{marker}]: {} (Paket, ID)-Einträge – implemented {}, deferred {}, manual {}, open {}; {parts_blocking} blockierend",
        parts.values().map(BTreeMap::len).sum::<usize>(),
        count(PartStatus::Implemented),
        count(PartStatus::Deferred),
        count(PartStatus::Manual),
        count(PartStatus::Open),
    );
    for (package, entries) in &parts {
        let line: Vec<String> = entries
            .iter()
            .map(|(id, e)| {
                let st = serde_json::to_value(e.status)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default();
                format!(
                    "{id} {st}{}",
                    if e.blocking { " (blockierend)" } else { "" }
                )
            })
            .collect();
        println!("  {package}: {}", line.join(", "));
    }
    let open_by_id: Vec<&String> = by_id
        .iter()
        .filter(|(_, v)| v["status"] == "open")
        .map(|(id, _)| id)
        .collect();
    println!(
        "Teilkriterien [{marker}] je ID ohne Besitzprüfung (informativ, nicht gewertet): {} IDs, offen {}{}",
        by_id.len(),
        open_by_id.len(),
        if open_by_id.is_empty() {
            String::new()
        } else {
            format!(
                ": {}",
                open_by_id
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    );
    if !out_of_scope.is_empty() {
        println!(
            "Ignores außerhalb von {ms} (Gründe ohne gültiges Präfix, nicht gewertet): {}",
            out_of_scope.len()
        );
        for o in &out_of_scope {
            println!(
                "  {}:{} {}",
                o["file"].as_str().unwrap_or(""),
                o["line"],
                o["reason"].as_str().unwrap_or("")
            );
        }
    }
    println!(
        "{ms} {}: {} blockierende IDs, {parts_blocking} blockierende Teilkriterien, {} ungültige Ignores, {} rote Tests",
        if args.gate { "Gate" } else { "Entwicklung" },
        blocking.len(),
        report.ignored_without_valid_reason.len(),
        failed_tests.len()
    );
    Ok(passed)
}

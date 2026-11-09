//! M2-Abdeckung: Paket-/Wellenbesitz, C9-Platzhalter und Anhang B (K2-58).
//!
//! Entwicklung prüft ausgewählte gemergte Pakete (Standard Welle 0); übrige offene Einträge sind Warnungen.
//! --gate verlangt über alle Wellen implementierte IDs und vollständigen Anhang B. AMP bleibt nach
//! K2-22 ausgenommen. Alle Berichte sind deterministisch und enthalten keine Laufzeitstempel.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    compute_with_reason, frontend, is_benchmark_reason, results, scan, targets, BadIgnore,
    CoverageArgs, PackageSpecs, Status, TestEntry, TestState,
};
use crate::error::ParityError;

#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    #[serde(default)]
    wave: Value,
    #[serde(default)]
    spec_ids: Vec<String>,
    #[serde(default)]
    extra_spec_ids: Vec<String>,
    #[serde(default)]
    owns: Vec<String>,
    /// Beleg für Pakete ohne eigenen Merge-Titel (B4-M2CLOSE, `coverage/merged.rs`).
    #[serde(default)]
    merged_via: Option<super::merged::MergedVia>,
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
    fn required(&self, args: &CoverageArgs) -> bool {
        args.gate
            || self
                .wave_number()
                .is_some_and(|w| w <= u64::from(args.wave))
            || args.merged.iter().any(|id| {
                self.id.eq_ignore_ascii_case(id)
                    || self.id.trim_start_matches("M2-").eq_ignore_ascii_case(id)
            })
    }
    fn owns(&self, file: &str) -> bool {
        self.owns.iter().any(|p| owns(p, file))
    }
}

fn load(path: &Path) -> Result<Vec<Package>, ParityError> {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path)?)
        .map_err(|e| ParityError::Invalid(format!("{}: {e}", path.display())))?;
    serde_json::from_value(doc.get("packages").unwrap_or(&doc).clone())
        .map_err(|e| ParityError::Invalid(format!("{}: {e}", path.display())))
}

/// M2-Zurückstellungen; Benchmarks sind separat neutral. Platzhalter sind nie deferred.
pub fn valid_defer_reason(reason: &str) -> bool {
    let r = reason.trim();
    if ["M3:", "M4:", "M5:", "P2:"]
        .iter()
        .any(|p| r.starts_with(p))
    {
        return true;
    }
    // K3-01: M3-Klärungen (`Klärung K3-<Zahl>`) sind wie M2-Klärungen gültig.
    ["Klärung K2-", "Klärung K3-"].iter().any(|prefix| {
        r.strip_prefix(prefix).is_some_and(|tail| {
            let n = tail.chars().take_while(char::is_ascii_digit).count();
            n > 0
                && tail[n..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric())
        })
    })
}

// Kleiner Glob-Matcher für die plan-eigenen Besitzmuster: *, **, ?, {a,b}.
fn glob(pattern: &str, value: &str) -> bool {
    if let Some(open) = pattern.find('{') {
        if let Some(close) = pattern[open..].find('}').map(|c| c + open) {
            return pattern[open + 1..close].split(',').any(|part| {
                glob(
                    &format!("{}{part}{}", &pattern[..open], &pattern[close + 1..]),
                    value,
                )
            });
        }
    }
    fn go(p: &[u8], v: &[u8], memo: &mut BTreeMap<(usize, usize), bool>) -> bool {
        if let Some(b) = memo.get(&(p.len(), v.len())) {
            return *b;
        }
        let result = match p.first() {
            None => v.is_empty(),
            Some(b'*') => {
                let double = p.get(1) == Some(&b'*');
                let rest = &p[if double { 2 } else { 1 }..];
                go(rest, v, memo)
                    || (!v.is_empty() && (double || v[0] != b'/') && go(p, &v[1..], memo))
            }
            Some(b'?') => !v.is_empty() && v[0] != b'/' && go(&p[1..], &v[1..], memo),
            Some(c) => v.first() == Some(c) && go(&p[1..], &v[1..], memo),
        };
        memo.insert((p.len(), v.len()), result);
        result
    }
    go(pattern.as_bytes(), value.as_bytes(), &mut BTreeMap::new())
}

/// Besitzmuster eines `owns`-Eintrags (auch von `mx` genutzt, M3-Q01).
pub(super) fn owns(pattern: &str, file: &str) -> bool {
    let base = pattern.split(" (").next().unwrap_or(pattern);
    if !glob(base, file) {
        return false;
    }
    // Besitz-Ausnahmen sind relativ zur Crate bzw. zum ausgeschlossenen Unterordner.
    if let Some((_, exceptions)) = pattern.split_once("ohne ") {
        let prefix = base.split("/**").next().unwrap_or("");
        let relative = file
            .strip_prefix(prefix)
            .unwrap_or(file)
            .trim_start_matches('/');
        for part in exceptions
            .trim_end_matches(')')
            .split([',', ';'])
            .flat_map(|p| p.split(" und "))
        {
            let p = part.trim().trim_start_matches("ohne ");
            if glob(p, relative) || glob(p, file) {
                return false;
            }
        }
    }
    true
}

fn placeholder_target(reason: &str) -> Option<&str> {
    let target = reason.trim().strip_prefix("wartet auf ")?;
    let suffix = target.strip_prefix("M2-")?;
    (!suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
    .then_some(target)
}

fn in_e2e_dir(file: &str, dirs: &[&str]) -> bool {
    file.strip_prefix("apps/desktop/e2e/")
        .and_then(|p| p.split('/').next())
        .is_some_and(|dir| dirs.contains(&dir))
}

fn c9(file: &str, reason: &str, packages: &[Package]) -> bool {
    // Quelle: M2.md C9, Rev. 5 / K2-26 (17.10.2026), gespiegelt in M2-check.py P7.
    // owns allein beschreibt nicht die erlaubten Zielpakete. Bis zu einer gemeinsamen
    // Planer-Datei müssen Änderungen an dieser Tabelle und P7 zusammen geprüft werden.
    let Some(target_id) = placeholder_target(reason) else {
        return false;
    };
    let Some(target) = packages.iter().find(|p| p.id == target_id) else {
        return false;
    };
    packages
        .iter()
        .filter(|p| p.owns(file) && p.id != target_id)
        .any(|owner| {
            if owner.wave == 1 {
                if target.wave != 1 {
                    return false;
                }
                if file.starts_with("crates/") {
                    let short = owner.id.trim_start_matches("M2-").to_ascii_lowercase();
                    let suffix = format!("/tests/pending_w1_{short}.rs");
                    let Some(crate_dir) = file.strip_suffix(&suffix) else {
                        return false;
                    };
                    let tests = format!("{crate_dir}/tests/");
                    let owns_tests = owner.owns.iter().any(|p| {
                        !p.contains("pending_w1_")
                            && (p.starts_with(&tests)
                                || owns(p, &format!("{tests}__coverage_owner_probe.rs")))
                    });
                    owns_tests
                        && [
                            "M2-E01", "M2-E02", "M2-E03", "M2-E04", "M2-K02", "M2-K03", "M2-K04",
                            "M2-K06", "M2-K07", "M2-D08", "M2-D09",
                        ]
                        .contains(&target_id)
                } else {
                    [
                        "M2-U01", "M2-U03", "M2-U04", "M2-U10", "M2-U11", "M2-U12", "M2-U16",
                    ]
                    .contains(&owner.id.as_str())
                        && in_e2e_dir(
                            file,
                            &[
                                "shell",
                                "header",
                                "statusbar",
                                "menu",
                                "keyboard",
                                "table",
                                "detail",
                                "sidebar",
                                "export",
                            ],
                        )
                        && !target.id.starts_with("M2-U")
                }
            } else {
                owner.wave == 2
                    && target.wave == 2
                    && owner.id.starts_with("M2-U")
                    && !target.id.starts_with("M2-U")
                    && in_e2e_dir(
                        file,
                        &[
                            "config",
                            "settings-sections",
                            "crawls",
                            "table-advanced",
                            "detail-fach",
                            "sidebar-fach",
                            "viz",
                            "mcp",
                            "integrations",
                            "tasks",
                        ],
                    )
            }
        })
}

#[derive(Debug, Serialize)]
struct AppendixEntry {
    status: Status,
    required_markers: Vec<String>,
    missing_markers: Vec<String>,
    tests: Vec<TestEntry>,
    required_now: bool,
}

fn appendix_requirements(path: &Path) -> Result<BTreeMap<String, Vec<String>>, ParityError> {
    let text = std::fs::read_to_string(path)?;
    let section = text
        .split("## Anhang B")
        .nth(1)
        .ok_or_else(|| ParityError::Invalid(format!("{}: Anhang B fehlt", path.display())))?;
    let mut requirements = BTreeMap::new();
    for line in section.lines().take_while(|l| !l.starts_with("## ")) {
        let cols: Vec<_> = line.split('|').map(str::trim).collect();
        if cols.len() < 5 || !scan::is_spec_id(cols[1]) || cols[3].contains("Abschnitt 9") {
            continue;
        }
        let mut markers = Vec::new();
        if cols[2].contains("M1 7.3") {
            markers.push("M2".into());
        }
        if cols[2].contains("[P1]") {
            markers.push("P1".into());
        }
        if !markers.is_empty() {
            requirements.insert(cols[1].into(), markers);
        }
    }
    Ok(requirements)
}

fn entries<'a>(report: &'a super::CoverageReport, id: &str) -> &'a [TestEntry] {
    report
        .specs
        .get(id)
        .or_else(|| report.extra.get(id))
        .map_or(&[], |e| e.tests.as_slice())
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
            let name = source
                .module
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(source.name.as_str()))
                .collect::<Vec<_>>()
                .join("::");
            name == test.test
                && source
                    .criteria
                    .get(id)
                    .is_some_and(|ms| ms.iter().any(|m| m == marker))
        })
}

/// Führt die M2-Abdeckung aus; M1 bleibt im bisherigen Einstieg unverändert prüfbar.
pub fn run(args: &CoverageArgs) -> Result<bool, ParityError> {
    let root = &args.root;
    let path = args
        .packages
        .clone()
        .unwrap_or_else(|| root.join("docs/plan/M2-packages.json"));
    let packages = load(&path)?;
    // B4-M2CLOSE: gemergte Pakete aus der Merge-Historie (Titelformen und `merged_via`), zusätzlich zu --merged.
    let detection = match &args.merged_log {
        Some(log) => Some(super::merged::detect(
            &std::fs::read_to_string(log)?,
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
    // B3-COV: Merge-Titel auf main nennen auch Zweige, die kein Planpaket sind (z. B. M2-N00-SPEC-TAGS).
    // Unbekannte IDs sind eine Warnung und wählen kein Paket aus; sie brechen den Bericht nicht ab.
    let unknown_merged: Vec<String> = args
        .merged
        .iter()
        .filter(|id| {
            !packages.iter().any(|p| {
                p.id.eq_ignore_ascii_case(id)
                    || p.id.trim_start_matches("M2-").eq_ignore_ascii_case(id)
            })
        })
        .cloned()
        .collect();
    let mut package_specs = PackageSpecs::default();
    for p in &packages {
        // Alle Planeinträge mit spec_ids zählen, auch Messpakete und Spikes (586 insgesamt).
        for (ids, dest) in [
            (&p.spec_ids, &mut package_specs.specs),
            (&p.extra_spec_ids, &mut package_specs.extra),
        ] {
            for id in ids {
                dest.entry(id.clone()).or_default().push(p.id.clone());
            }
        }
    }
    package_specs
        .extra
        .retain(|id, _| !package_specs.specs.contains_key(id));
    let mut scans = BTreeMap::new();
    for dir in ["crates", "tests"] {
        for file in scan::rust_files(root, dir) {
            scans.insert(file.clone(), scan::scan_file(root, &file)?);
        }
    }
    let frontend_scans = frontend::scan(root)?;
    let mut target_list = targets::discover_targets(root)?;
    target_list.extend(frontend::targets(&frontend_scans));
    scans.extend(frontend_scans.clone());
    let mut warnings = Vec::new();
    let mut test_results = Vec::new();
    if let Some(p) = &args.test_output {
        test_results.extend(results::parse_cargo_output(&std::fs::read_to_string(p)?));
    }
    if let Some(p) = &args.junit {
        test_results.extend(results::parse_junit(&std::fs::read_to_string(p)?)?);
    }
    for p in &args.frontend_junit {
        test_results.extend(frontend::read_junit(
            root,
            p,
            &frontend_scans,
            &mut warnings,
        )?);
    }
    // Alte Statusdateien anderer Meilensteine geben keine M2-Paritätsgutschrift.
    let previous: Option<Value> = args
        .write
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(|v| v["milestone"] == "M2");
    // M1-Tags sind bekannt, zählen aber weder als M2-Pflicht noch als M2-Zusatz-ID.
    // Nur die für compute sichtbaren IDs filtern; der Ignore-Scan bleibt vollständig.
    let m1_path = root.join("docs/plan/M1-packages.json");
    let m1_ids: std::collections::BTreeSet<_> = if m1_path.exists() {
        let m1 = super::load_packages(&m1_path)?;
        m1.specs.into_keys().chain(m1.extra.into_keys()).collect()
    } else {
        Default::default()
    };
    let mut coverage_scans = scans.clone();
    for test in coverage_scans.values_mut().flat_map(|s| &mut s.tests) {
        test.ids.retain(|id| {
            !m1_ids.contains(id)
                || package_specs.specs.contains_key(id)
                || package_specs.extra.contains_key(id)
        });
    }
    let mut report = compute_with_reason(
        &package_specs,
        &coverage_scans,
        target_list,
        &test_results,
        previous.as_ref(),
        &valid_defer_reason,
    );
    super::apply_parity(&mut report, args)?;
    report.warnings.extend(warnings);
    report.warnings.extend(
        unknown_merged
            .iter()
            .map(|id| format!("unbekanntes gemergtes M2-Paket ignoriert: {id}")),
    );
    if let Some(d) = &detection {
        report
            .warnings
            .extend(d.missing_via.iter().map(|(id, why)| format!("{id}: {why}")));
    }
    report.ignored_without_valid_reason.clear();
    let mut placeholders = Vec::new();
    let mut legacy_ignores = Vec::new();
    for entry in report.specs.values_mut().chain(report.extra.values_mut()) {
        if entry
            .note
            .as_deref()
            .is_some_and(|n| n.contains("Grund fehlt"))
        {
            entry.note = Some(
                "alle getaggten Tests ignoriert, kein gültiger M2-Zurückstellungsgrund".into(),
            );
        }
    }
    for t in scans.values().flat_map(|s| &s.tests) {
        let name = t
            .module
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(t.name.as_str()))
            .collect::<Vec<_>>()
            .join("::");
        let runtime_reason = t
            .ids
            .iter()
            .flat_map(|id| entries(&report, id))
            .find(|e| e.file == t.file && e.test == name && e.result == TestState::Ignored)
            .and_then(|e| e.ignore_reason.clone());
        let reason = t.ignore.as_ref().or(runtime_reason.as_ref());
        let runtime_ignored = t
            .ids
            .iter()
            .flat_map(|id| entries(&report, id))
            .any(|e| e.file == t.file && e.test == name && e.result == TestState::Ignored);
        let empty_reason = String::new();
        let Some(reason) = reason.or(runtime_ignored.then_some(&empty_reason)) else {
            continue;
        };
        if is_benchmark_reason(reason) || valid_defer_reason(reason) {
            continue;
        }
        if !args.gate && c9(&t.file, reason, &packages) {
            placeholders.push(json!({"file": t.file, "test": name, "reason": reason}));
            report
                .warnings
                .push(format!("C9-Platzhalter {}:{}: {reason}", t.file, t.line));
        } else if !args.gate && reason.trim().starts_with("M2:") {
            let mut owners: Vec<_> = packages.iter().filter(|p| p.owns(&t.file)).collect();
            // F05 besitzt beim Vorbau ganze Crates. Sobald ein Fachbesitzer ab
            // Welle 1 existiert, entscheidet dessen Merge über die Altgründe.
            if owners
                .iter()
                .any(|p| p.wave_number().is_some_and(|w| w > 0))
            {
                owners.retain(|p| p.wave_number().is_none_or(|w| w > 0));
            }
            // Kein Besitzer oder mehrere Besitzer mit einem bereits gemergten Paket:
            // nicht stillschweigend freigeben (z. B. N01 als späterer Mitbesitzer).
            if !owners.is_empty() && owners.iter().all(|p| !p.required(args)) {
                let owners: Vec<_> = owners.iter().map(|p| p.id.as_str()).collect();
                report.warnings.push(format!(
                    "Altgrund, Besitzer {}: {}:{}: {reason}",
                    owners.join(", "),
                    t.file,
                    t.line
                ));
                legacy_ignores.push(json!({"file": t.file, "test": name, "line": t.line,
                    "reason": reason, "owners": owners, "spec_ids": t.ids}));
            } else {
                report.ignored_without_valid_reason.push(BadIgnore {
                    file: t.file.clone(),
                    test: name,
                    line: t.line,
                    reason: reason.clone(),
                    spec_ids: t.ids.clone(),
                    problem: "Altgrund eines gemergten oder unbekannten Besitzers".into(),
                });
            }
        } else {
            report.ignored_without_valid_reason.push(BadIgnore {
                file: t.file.clone(),
                test: name,
                line: t.line,
                reason: reason.clone(),
                spec_ids: t.ids.clone(),
                problem: "kein gültiger M2-Ignore-Grund oder unzulässiger C9-Platzhalter".into(),
            });
        }
    }
    report
        .ignored_without_valid_reason
        .sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    let mut blocking = Vec::new();
    for (id, entry) in &report.specs {
        let implemented = matches!(entry.status, Status::Implemented | Status::Verified);
        let required_now = packages
            .iter()
            .any(|p| p.spec_ids.contains(id) && p.required(args));
        let only_placeholders = !entry.tests.is_empty()
            && entry
                .tests
                .iter()
                .filter(|t| !t.ignore_reason.as_deref().is_some_and(is_benchmark_reason))
                .all(|t| {
                    t.result == TestState::Ignored
                        && t.ignore_reason
                            .as_deref()
                            .is_some_and(|r| c9(&t.file, r, &packages))
                })
            && entry.tests.iter().any(|t| {
                t.result == TestState::Ignored
                    && t.ignore_reason
                        .as_deref()
                        .is_some_and(|r| c9(&t.file, r, &packages))
            });
        if !implemented && (args.gate || entry.status == Status::Open) {
            if required_now && !(only_placeholders && !args.gate) {
                blocking.push(id.clone());
            } else {
                report.warnings.push(format!(
                    "offene ID {id}: Besitzer noch nicht gemergt oder C9-Platzhalter (Entwicklung)"
                ));
            }
        }
        // Ein tatsächlich fehlgeschlagener Test bleibt auch in späteren Wellen ein Fehler.
        if entry.tests.iter().any(|t| t.result == TestState::Failed) && !blocking.contains(id) {
            blocking.push(id.clone());
        }
    }
    let requirements = appendix_requirements(&root.join("docs/plan/M2.md"))?;
    let mut appendix: BTreeMap<String, BTreeMap<String, AppendixEntry>> = BTreeMap::new();
    let mut appendix_open = 0;
    for p in &packages {
        for id in &p.extra_spec_ids {
            let Some(markers) = requirements.get(id) else {
                continue;
            };
            let tests: Vec<_> = entries(&report, id)
                .iter()
                .filter(|t| p.owns(&t.file))
                .cloned()
                .collect();
            let missing: Vec<_> = markers
                .iter()
                .filter(|marker| {
                    !tests.iter().any(|t| {
                        if t.result != TestState::Ok
                            || t.ignore_reason.as_deref().is_some_and(is_benchmark_reason)
                        {
                            return false;
                        }
                        has_marker(t, id, marker, &scans)
                    })
                })
                .cloned()
                .collect();
            let required_now = p.required(args);
            let failed = tests.iter().any(|t| t.result == TestState::Failed);
            let status = if missing.is_empty() && !failed {
                Status::Implemented
            } else {
                Status::Open
            };
            if status == Status::Open {
                let pending_markers = !args.gate
                    && !missing.is_empty()
                    && missing.iter().all(|marker| {
                        tests.iter().any(|t| {
                            t.result == TestState::Ignored
                                && t.ignore_reason
                                    .as_deref()
                                    .is_some_and(|r| c9(&t.file, r, &packages))
                                && has_marker(t, id, marker, &scans)
                        })
                    });
                if failed || (required_now && !pending_markers) {
                    appendix_open += 1;
                } else {
                    report.warnings.push(format!(
                        "Anhang B {id} / {} offen (Besitzer noch nicht gemergt oder C9-Platzhalter)",
                        p.id
                    ));
                }
            }
            appendix.entry(id.clone()).or_default().insert(
                p.id.clone(),
                AppendixEntry {
                    status,
                    required_markers: markers.clone(),
                    missing_markers: missing,
                    tests,
                    required_now,
                },
            );
        }
    }
    let failed_tests: std::collections::BTreeSet<_> = test_results
        .iter()
        .filter(|t| t.outcome == results::Outcome::Failed)
        .map(|t| t.path.as_str())
        .collect();
    let ok = blocking.is_empty()
        && appendix_open == 0
        && report.ignored_without_valid_reason.is_empty()
        && failed_tests.is_empty();
    let mut doc = report.to_json();
    doc["milestone"] = json!("M2");
    doc["mode"] = json!(if args.gate { "gate" } else { "development" });
    doc["appendix_b"] = json!(appendix);
    doc["placeholders"] = json!(placeholders);
    doc["legacy_ignores"] = json!(legacy_ignores);
    doc["required_packages"] = json!(packages
        .iter()
        .filter(|p| p.required(args))
        .map(|p| &p.id)
        .collect::<Vec<_>>());
    doc["unknown_merged"] = json!(unknown_merged);
    if let Some(d) = &detection {
        doc["merged_packages"] = json!(d.merged);
        doc["other_merge_branches"] = json!(d.other);
        doc["merged_via_missing"] = json!(d
            .missing_via
            .iter()
            .map(|(id, why)| json!({"package": id, "problem": why}))
            .collect::<Vec<_>>());
    }
    doc["blocking_specs"] = json!(blocking);
    doc["blocking_appendix_b"] = json!(appendix_open);
    doc["passed"] = json!(ok);
    doc["failed_tests"] = json!(failed_tests);
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
            "Gemergte M2-Pakete laut Merge-Historie: {} (davon über merged_via: {}); andere Merge-Zweige: {}",
            d.merged.len(),
            d.merged
                .values()
                .filter(|b| b.starts_with("merged_via "))
                .count(),
            d.other.len()
        );
        for (branch, subject) in &d.other {
            println!("  kein M2-Paket: {branch} ({subject})");
        }
    }
    print!("{}", report.summary_text());
    for name in &failed_tests {
        println!("  FAILED {name}");
    }
    println!(
        "Anhang B: {} (ID, Paket)-Einträge, {appendix_open} blockierend offen",
        appendix.values().map(BTreeMap::len).sum::<usize>()
    );
    for (id, entries) in &appendix {
        for (package, entry) in entries {
            if entry.status == Status::Open {
                println!("  open {id} / {package}: fehlt {:?}", entry.missing_markers);
            }
        }
    }
    println!(
        "M2 {}: {} blockierende IDs, {} ungültige Ignores",
        if args.gate { "Gate" } else { "Entwicklung" },
        blocking.len(),
        report.ignored_without_valid_reason.len()
    );
    Ok(ok)
}

//! `os-parity coverage`: Status je Spec-ID aus Tests mit `// spec: …`-Tag und Testergebnissen (Abschnitt 5.4).
//!
//! 1. **Tag-Scan** (`scan`): `crates/**/*.rs` und `tests/**/*.rs` nach `// spec: ID[, ID] [M2]` über
//!    Testfunktionen und nach `#[ignore = "…"]`.
//! 2. **Testergebnisse** (`results`): Ausgabe von `cargo test --workspace --exclude openspider-desktop
//!    --no-fail-fast` (`--test-output`) und/oder JUnit von `cargo nextest` (`--junit`). Die Zuordnung
//!    Testname → Quelle läuft über das Test-Binär und die Moduldeklarationen (`targets`).
//! 3. **Status** je ID aus `spec_ids` der Einträge mit `kind = "package"` in `M1-packages.json`:
//!    - `implemented`: mindestens ein getaggter Test mit `ok` und keiner mit `FAILED`;
//!    - `deferred`: alle getaggten Tests `ignored` und jeder Grund beginnt mit `M2:`, `M3:` oder
//!      `Klärung K-xx`;
//!    - sonst `open` (kein Test, kein Ergebnis, `FAILED`, Platzhalter-Ignores wie `wartet auf M1-Pxx`).
//!    - `verified` bleibt erhalten, wenn es schon in der Statusdatei steht und der Test weiter `ok` ist
//!      (P29 setzt es aus dem Paritätsbericht).
//! 4. **Benchmarks und Zusatz-IDs** (5.4 Punkt 4): Ein Ignore-Grund mit Präfix `Benchmark` ist gültig;
//!    solche Tests sind für den Status neutral (weder `implemented` noch `deferred`), stehen nicht unter
//!    „ignoriert ohne gültigen Grund“, und eine ID nur mit Benchmark-Tests bleibt `open`. IDs im
//!    optionalen Feld `extra_spec_ids` eines Pakets erscheinen im Berichtsabschnitt `extra`, lösen keine
//!    Warnung „unbekannte IDs“ aus und zählen weder für die Zusammenfassung noch für `--strict`.

//!
//! M2 (--milestone M2): zusätzlich TS-Tags und vitest-/Playwright-JUnit, Messpakete,
//! M2-Ignore-Gründe und Anhang B je (ID, Paket). Entwicklung verlangt ausgewählte gemergte Pakete;
//! --gate verlangt alle Wellen ohne Platzhalter und ohne deferred Pflicht-IDs. Siehe Modul `m2`.

pub mod evidence;
pub mod frontend;
pub mod m2;
pub mod merged;
pub mod results;
pub mod scan;
pub mod targets;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use serde::Serialize;
use serde_json::{json, Value};

use crate::error::ParityError;
use results::{Outcome, TestResult};
use scan::{FileScan, TestFn};
use targets::Resolved;

/// Unterstützte Meilensteine der Spec-Abdeckung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Milestone {
    /// Unveränderte M1-Statusregeln.
    #[value(name = "M1")]
    M1,
    /// M2 einschließlich Frontend und Anhang B.
    #[value(name = "M2")]
    M2,
}

/// Argumente von `os-parity coverage`.
#[derive(Debug, Clone, clap::Args)]
pub struct CoverageArgs {
    /// Meilenstein; ohne --packages wird dessen kanonische Paketliste gelesen.
    #[arg(long, value_enum, default_value = "M1")]
    pub milestone: Milestone,
    /// M2-Abnahme: alle Wellen, keine Platzhalter, alle Pflicht-IDs implementiert.
    #[arg(long)]
    pub gate: bool,
    /// M2-Entwicklung: Pakete bis einschließlich dieser Welle als gemergt behandeln (Standard 0).
    #[arg(long, default_value_t = 0)]
    pub wave: u32,
    /// M2-Entwicklung: zusätzliche gemergte Paket-IDs, z. B. --merged D12,U10.
    #[arg(long, value_delimiter = ',')]
    pub merged: Vec<String>,
    /// M2-Entwicklung: Merge-Historie der Hauptlinie (`git log <main> --format='%H%x09%s'`); daraus erkannte
    /// Pakete gelten zusätzlich zu `--merged` als gemergt (Regeln in `coverage/merged.rs`).
    #[arg(long)]
    pub merged_log: Option<PathBuf>,
    /// JUnit aus vitest oder Playwright (wiederholbar).
    #[arg(long)]
    pub frontend_junit: Vec<PathBuf>,
    /// Paketliste (Standard: docs/plan/<Meilenstein>-packages.json unter --root).
    #[arg(long)]
    pub packages: Option<PathBuf>,
    /// Ausgabe von `cargo test --workspace --exclude openspider-desktop --no-fail-fast`.
    #[arg(long)]
    pub test_output: Option<PathBuf>,
    /// JUnit-XML aus `cargo nextest` (Alternative oder Ergänzung zu `--test-output`).
    #[arg(long)]
    pub junit: Option<PathBuf>,
    /// Statusdatei (z. B. docs/plan/M2-status.json).
    #[arg(long)]
    pub write: Option<PathBuf>,
    /// Wurzel des Repositories (enthält `crates/` und `tests/`).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Auch bei ignorierten Tests ohne gültigen Grund mit Exit-Code 1 enden.
    #[arg(long)]
    pub strict: bool,
    /// Bericht des Paritätslaufs (`report.json` von `os-parity run`) für den Status `verified`.
    #[arg(long)]
    pub parity_report: Option<PathBuf>,
    /// Zuordnung Spec-ID ↔ Export für `verified` (`tests/parity/evidence.toml`).
    #[arg(long)]
    pub evidence: Option<PathBuf>,
}

/// Status einer Spec-ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Mindestens ein getaggter Test ist grün.
    Implemented,
    /// Durch Paritätsbericht belegt (von P29 gesetzt).
    Verified,
    /// Zurückgestellt mit gültigem Grund.
    Deferred,
    /// Offen.
    Open,
}

/// Ergebnis eines getaggten Tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestState {
    /// `ok`.
    Ok,
    /// `FAILED`.
    Failed,
    /// `ignored`.
    Ignored,
    /// Kein Ergebnis in der Ausgabe.
    NoResult,
}

/// Ein getaggter Test im Bericht.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestEntry {
    /// Quelldatei.
    pub file: String,
    /// Vollständiger Name innerhalb der Datei (Inline-Module + Funktion).
    pub test: String,
    /// Ergebnis.
    pub result: TestState,
    /// Ignore-Grund (Quelle bzw. Ausgabe).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_reason: Option<String>,
    /// `[M2]`/`[M3]` im Tag.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deferred_hint: bool,
}

/// Status und Belege einer ID.
#[derive(Debug, Clone, Serialize)]
pub struct SpecEntry {
    /// Status.
    pub status: Status,
    /// Pakete, die die ID umsetzen.
    pub packages: Vec<String>,
    /// Begründung, wenn nicht `implemented`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Getaggte Tests.
    pub tests: Vec<TestEntry>,
    /// Beleg aus dem Paritätsbericht (aus der bestehenden Statusdatei übernommen).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parity: Option<Value>,
}

/// Ignorierter Test ohne gültigen Grund.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BadIgnore {
    /// Quelldatei.
    pub file: String,
    /// Testname.
    pub test: String,
    /// Zeile.
    pub line: u32,
    /// Angegebener Grund (leer = keiner).
    pub reason: String,
    /// IDs aus dem Tag.
    pub spec_ids: Vec<String>,
    /// Befund.
    pub problem: String,
}

/// Gesamtergebnis.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageReport {
    /// Zusammenfassung.
    pub summary: BTreeMap<String, usize>,
    /// Status je ID.
    pub specs: BTreeMap<String, SpecEntry>,
    /// Zusatz-IDs (`extra_spec_ids`): Status je ID, ohne Wirkung auf Zusammenfassung und `--strict`.
    pub extra: BTreeMap<String, SpecEntry>,
    /// Ignorierte Tests ohne gültigen Grund.
    pub ignored_without_valid_reason: Vec<BadIgnore>,
    /// Hinweise.
    pub warnings: Vec<String>,
}

/// Ist der Ignore-Grund ein gültiger Zurückstellungsgrund?
pub fn valid_defer_reason(reason: &str) -> bool {
    let r = reason.trim();
    if r.starts_with("M2:") || r.starts_with("M3:") {
        return true;
    }
    for prefix in ["Klärung K-", "Klaerung K-"] {
        if let Some(rest) = r.strip_prefix(prefix) {
            return rest.chars().next().is_some_and(|c| c.is_ascii_digit());
        }
    }
    // M2-/M3-Klärungen (`Klärung K2-<Zahl>`, `Klärung K3-<Zahl>`, z. B. K2-68, K3-01) sind auch
    // für den M1-Strict-Lauf gültig.
    ["Klärung K2-", "Klärung K3-"].iter().any(|prefix| {
        r.strip_prefix(prefix)
            .is_some_and(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
    })
}

/// Ist der Ignore-Grund ein Benchmark-Grund (`Benchmark (P28): …`)? Gültig, aber für den Status neutral.
pub fn is_benchmark_reason(reason: &str) -> bool {
    reason.trim().starts_with("Benchmark")
}

/// Spec-IDs aus `M1-packages.json`: `specs` (Feld `spec_ids`) und `extra` (Feld `extra_spec_ids`),
/// jeweils mit den Paketen, die sie führen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageSpecs {
    /// Spec-IDs der Pakete (Grundlage von A2/A9).
    pub specs: BTreeMap<String, Vec<String>>,
    /// Zusatz-IDs, die Tests eines Pakets mitprüfen; IDs, die auch in `specs` stehen, fehlen hier.
    pub extra: BTreeMap<String, Vec<String>>,
}

/// Spec-IDs je Paket aus `M1-packages.json` (nur Einträge mit `kind = "package"`).
pub fn load_package_specs(
    path: &std::path::Path,
) -> Result<BTreeMap<String, Vec<String>>, ParityError> {
    load_packages(path).map(|p| p.specs)
}

/// Liest `spec_ids` und das optionale Feld `extra_spec_ids` der Pakete (`kind = "package"`).
pub fn load_packages(path: &std::path::Path) -> Result<PackageSpecs, ParityError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ParityError::Invalid(format!("{} nicht lesbar: {e}", path.display())))?;
    let doc: Value = serde_json::from_str(&text)
        .map_err(|e| ParityError::Invalid(format!("{}: {e}", path.display())))?;
    let list = doc
        .as_array()
        .or_else(|| doc.get("packages").and_then(|p| p.as_array()))
        .ok_or_else(|| {
            ParityError::Invalid(format!("{}: Liste von Paketen erwartet", path.display()))
        })?;
    let mut specs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut extra: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in list {
        if p.get("kind").and_then(|k| k.as_str()) != Some("package") {
            continue;
        }
        let pkg = p.get("id").and_then(|i| i.as_str()).unwrap_or("?");
        for (field, target) in [("spec_ids", &mut specs), ("extra_spec_ids", &mut extra)] {
            for id in p
                .get(field)
                .and_then(|s| s.as_array())
                .into_iter()
                .flatten()
                .filter_map(|s| s.as_str())
            {
                let owners = target.entry(id.to_string()).or_default();
                if !owners.iter().any(|o| o == pkg) {
                    owners.push(pkg.to_string());
                }
            }
        }
    }
    extra.retain(|id, _| !specs.contains_key(id));
    Ok(PackageSpecs { specs, extra })
}

#[derive(Default, Clone)]
struct Agg {
    ok: bool,
    failed: bool,
    ignored: bool,
    reason: Option<String>,
}

/// Rechnet den Status aus Quellen und Ergebnissen.
pub fn compute(
    package_specs: &PackageSpecs,
    scans: &BTreeMap<String, FileScan>,
    targets_found: Vec<targets::Target>,
    results: &[TestResult],
    previous: Option<&Value>,
) -> CoverageReport {
    compute_with_reason(
        package_specs,
        scans,
        targets_found,
        results,
        previous,
        &valid_defer_reason,
    )
}

pub(crate) fn compute_with_reason(
    package_specs: &PackageSpecs,
    scans: &BTreeMap<String, FileScan>,
    targets_found: Vec<targets::Target>,
    results: &[TestResult],
    previous: Option<&Value>,
    valid_reason: &dyn Fn(&str) -> bool,
) -> CoverageReport {
    let tests: Vec<TestFn> = scans.values().flat_map(|s| s.tests.clone()).collect();
    let index = targets::build_index(targets_found, scans, &tests);
    let mut warnings: Vec<String> = Vec::new();
    let mut aggs: HashMap<usize, Agg> = HashMap::new();
    let mut unmapped = 0usize;
    let mut ambiguous: BTreeSet<String> = BTreeSet::new();
    // Pro Ausgabeblock die Testpfade sammeln, um gleichnamige Ziele verschiedener Pakete zu trennen.
    let mut block_paths: HashMap<usize, Vec<&str>> = HashMap::new();
    for r in results {
        block_paths
            .entry(r.block)
            .or_default()
            .push(r.path.as_str());
    }
    for r in results {
        let cands = index.narrow(index.candidates(&r.binary), &block_paths[&r.block]);
        if cands.is_empty() {
            unmapped += 1;
            continue;
        }
        match index.resolve(&cands, &r.path) {
            Resolved::None => unmapped += 1,
            Resolved::Ambiguous(ts) => {
                let names: Vec<String> = ts
                    .iter()
                    .map(|&t| format!("{}::{}", tests[t].file, tests[t].name))
                    .collect();
                ambiguous.insert(format!(
                    "Testname {:?} nicht eindeutig zuordenbar (Kandidaten: {}); keine Gutschrift",
                    r.path,
                    names.join(", ")
                ));
            }
            Resolved::One(t) => {
                let a = aggs.entry(t).or_default();
                match &r.outcome {
                    Outcome::Ok => a.ok = true,
                    Outcome::Failed => a.failed = true,
                    Outcome::Ignored(reason) => {
                        a.ignored = true;
                        if a.reason.is_none() {
                            a.reason = reason.clone();
                        }
                    }
                }
            }
        }
    }
    warnings.extend(ambiguous);

    // Tests je ID.
    let mut per_id: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, t) in tests.iter().enumerate() {
        for id in &t.ids {
            per_id.entry(id.as_str()).or_default().push(i);
        }
    }
    let state_of = |i: usize| -> TestState {
        match aggs.get(&i) {
            Some(a) if a.failed => TestState::Failed,
            Some(a) if a.ok => TestState::Ok,
            Some(a) if a.ignored => TestState::Ignored,
            _ => TestState::NoResult,
        }
    };
    let reason_of = |i: usize| -> Option<String> {
        let src = tests[i].ignore.clone().filter(|r| !r.is_empty());
        src.or_else(|| aggs.get(&i).and_then(|a| a.reason.clone()))
    };

    let is_bench = |i: usize| -> bool { reason_of(i).is_some_and(|r| is_benchmark_reason(&r)) };
    let build = |id: &String, packages: &Vec<String>| -> SpecEntry {
        let all_idxs = per_id.get(id.as_str()).cloned().unwrap_or_default();
        let entries: Vec<TestEntry> = all_idxs
            .iter()
            .map(|&i| TestEntry {
                file: tests[i].file.clone(),
                test: tests[i]
                    .module
                    .iter()
                    .map(String::as_str)
                    .chain(std::iter::once(tests[i].name.as_str()))
                    .collect::<Vec<_>>()
                    .join("::"),
                result: state_of(i),
                ignore_reason: reason_of(i),
                deferred_hint: tests[i].deferred_hint,
            })
            .collect();
        // Benchmark-Tests (Ignore-Grund `Benchmark …`) sind für den Status neutral.
        let idxs: Vec<usize> = all_idxs.iter().copied().filter(|&i| !is_bench(i)).collect();
        let bench_only = idxs.is_empty() && !all_idxs.is_empty();
        let states: Vec<TestState> = idxs.iter().map(|&i| state_of(i)).collect();
        let (mut status, note) = if bench_only {
            (
                Status::Open,
                Some(
                    "nur Benchmark-Tests (P28) getaggt; kein regulärer Test mit `// spec:`-Tag"
                        .to_string(),
                ),
            )
        } else if idxs.is_empty() {
            (
                Status::Open,
                Some("kein Test mit `// spec:`-Tag gefunden".to_string()),
            )
        } else if states.contains(&TestState::Failed) {
            (
                Status::Open,
                Some("getaggter Test schlägt fehl (FAILED)".to_string()),
            )
        } else if states.contains(&TestState::Ok) {
            (Status::Implemented, None)
        } else if states.iter().all(|s| *s == TestState::Ignored) {
            let all_valid = idxs
                .iter()
                .all(|&i| reason_of(i).is_some_and(|r| valid_reason(&r)));
            if all_valid {
                (Status::Deferred, None)
            } else {
                (
                    Status::Open,
                    Some(
                        "alle getaggten Tests ignoriert, Grund fehlt oder ist kein M2:/M3:/Klärung K-xx"
                            .to_string(),
                    ),
                )
            }
        } else {
            (
                Status::Open,
                Some("getaggte Tests ohne Ergebnis in der Testausgabe".to_string()),
            )
        };
        let mut parity = None;
        if status == Status::Implemented {
            let prev = previous
                .and_then(|p| p.get("specs"))
                .and_then(|s| s.get(id.as_str()));
            if prev.and_then(|p| p.get("status")).and_then(|s| s.as_str()) == Some("verified") {
                status = Status::Verified;
                parity = prev.and_then(|p| p.get("parity")).cloned();
            }
        }
        SpecEntry {
            status,
            packages: packages.clone(),
            note,
            tests: entries,
            parity,
        }
    };
    let specs: BTreeMap<String, SpecEntry> = package_specs
        .specs
        .iter()
        .map(|(id, pk)| (id.clone(), build(id, pk)))
        .collect();
    let extra: BTreeMap<String, SpecEntry> = package_specs
        .extra
        .iter()
        .map(|(id, pk)| (id.clone(), build(id, pk)))
        .collect();

    // Unbekannte IDs in Tags.
    let unknown: BTreeSet<&str> = per_id
        .keys()
        .copied()
        .filter(|id| {
            !package_specs.specs.contains_key(*id) && !package_specs.extra.contains_key(*id)
        })
        .collect();
    if !unknown.is_empty() {
        warnings.push(format!(
            "Tags mit IDs, die in keinem Paket stehen: {}",
            unknown.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if results.is_empty() {
        warnings.push(
            "keine Testergebnisse eingelesen: alle getaggten Tests gelten als ohne Ergebnis".into(),
        );
    }
    if unmapped > 0 {
        warnings.push(format!(
            "{unmapped} Testergebnisse ohne getaggte Quelle bzw. unbekanntes Test-Binär (ohne Wirkung)"
        ));
    }

    // Ignorierte Tests ohne gültigen Grund (alle Tests, nicht nur getaggte).
    let mut bad: Vec<BadIgnore> = Vec::new();
    for t in &tests {
        let Some(reason) = &t.ignore else { continue };
        let problem = if reason.trim().is_empty() {
            Some("#[ignore] ohne Grund")
        } else if is_benchmark_reason(reason) {
            None
        } else if !t.ids.is_empty() && !valid_defer_reason(reason) {
            Some("Grund beginnt nicht mit M2:/M3:/Klärung K-xx (Platzhalter zählen nicht als deferred)")
        } else {
            None
        };
        if let Some(p) = problem {
            bad.push(BadIgnore {
                file: t.file.clone(),
                test: t
                    .module
                    .iter()
                    .map(String::as_str)
                    .chain(std::iter::once(t.name.as_str()))
                    .collect::<Vec<_>>()
                    .join("::"),
                line: t.line,
                reason: reason.clone(),
                spec_ids: t.ids.clone(),
                problem: p.to_string(),
            });
        }
    }
    bad.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));

    let mut summary: BTreeMap<String, usize> = BTreeMap::new();
    summary.insert("total".into(), specs.len());
    for (name, st) in [
        ("implemented", Status::Implemented),
        ("verified", Status::Verified),
        ("deferred", Status::Deferred),
        ("open", Status::Open),
    ] {
        summary.insert(
            name.into(),
            specs.values().filter(|s| s.status == st).count(),
        );
    }
    CoverageReport {
        summary,
        specs,
        extra,
        ignored_without_valid_reason: bad,
        warnings,
    }
}

impl CoverageReport {
    /// JSON-Fassung (deterministisch, ohne Zeitstempel).
    pub fn to_json(&self) -> Value {
        json!({
            "generated_by": "os-parity coverage",
            "summary": self.summary,
            "specs": self.specs,
            "extra": self.extra,
            "ignored_without_valid_reason": self.ignored_without_valid_reason,
            "warnings": self.warnings,
        })
    }

    /// Anzahl offener IDs.
    pub fn open_count(&self) -> usize {
        self.summary.get("open").copied().unwrap_or(0)
    }

    /// Kurzfassung für die Konsole.
    pub fn summary_text(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let g = |k: &str| self.summary.get(k).copied().unwrap_or(0);
        let _ = writeln!(
            s,
            "Spec-Abdeckung: {} IDs – implemented {}, verified {}, deferred {}, open {}",
            g("total"),
            g("implemented"),
            g("verified"),
            g("deferred"),
            g("open")
        );
        let open: Vec<_> = self
            .specs
            .iter()
            .filter(|(_, e)| e.status == Status::Open)
            .collect();
        for (id, e) in open.iter().take(60) {
            let _ = writeln!(s, "  open {id}: {}", e.note.as_deref().unwrap_or(""));
        }
        if open.len() > 60 {
            let _ = writeln!(s, "  ... und {} weitere offene IDs", open.len() - 60);
        }
        if !self.extra.is_empty() {
            let count = |st: Status| self.extra.values().filter(|e| e.status == st).count();
            let _ = writeln!(
                s,
                "Zusatz-IDs (extra, nicht gewertet): {} – implemented {}, verified {}, deferred {}, open {}",
                self.extra.len(),
                count(Status::Implemented),
                count(Status::Verified),
                count(Status::Deferred),
                count(Status::Open)
            );
        }
        if !self.ignored_without_valid_reason.is_empty() {
            let _ = writeln!(
                s,
                "Ignorierte Tests ohne gültigen Grund: {}",
                self.ignored_without_valid_reason.len()
            );
            for b in self.ignored_without_valid_reason.iter().take(60) {
                let _ = writeln!(s, "  {}:{} {} – {}", b.file, b.line, b.test, b.problem);
            }
        }
        for w in &self.warnings {
            let _ = writeln!(s, "Warnung: {w}");
        }
        s
    }
}

/// Ermittelt den Status; `Ok(true)` = keine ID mit Status `open`.
pub fn run(args: &CoverageArgs) -> Result<bool, ParityError> {
    // Ohne evidence-Datei: eigenständige B3g-Paritätsmessung nach SQ-01.
    if args.evidence.is_none() {
        if let Some(path) = &args.parity_report {
            let parity: Value = serde_json::from_str(&std::fs::read_to_string(path)?)
                .map_err(|e| ParityError::Invalid(e.to_string()))?;
            let summary = evidence::filter_summary(&args.root, &parity)?;
            if let Some(path) = &args.write {
                std::fs::write(
                    path,
                    serde_json::to_string_pretty(&summary)
                        .map_err(|e| ParityError::Invalid(e.to_string()))?
                        + "\n",
                )?;
            }
            println!(
                "verified: {}/{} ({:.3} %)",
                summary["verified"],
                summary["denominator"],
                summary["pct"].as_f64().unwrap_or(0.0)
            );
            return Ok(true);
        }
    }

    if args.milestone == Milestone::M2 {
        return m2::run(args);
    }
    let packages = args
        .packages
        .clone()
        .unwrap_or_else(|| args.root.join("docs/plan/M1-packages.json"));
    let package_specs = load_packages(&packages)?;
    let root = &args.root;

    let mut scans: BTreeMap<String, FileScan> = BTreeMap::new();
    for dir in ["crates", "tests"] {
        for rel in scan::rust_files(root, dir) {
            let fs = scan::scan_file(root, &rel)?;
            scans.insert(rel, fs);
        }
    }
    let targets_found = targets::discover_targets(root)?;

    let mut results: Vec<TestResult> = Vec::new();
    if let Some(p) = &args.test_output {
        let bytes = std::fs::read(p)
            .map_err(|e| ParityError::Invalid(format!("{} nicht lesbar: {e}", p.display())))?;
        results.extend(results::parse_cargo_output(&String::from_utf8_lossy(
            &bytes,
        )));
    }
    if let Some(p) = &args.junit {
        let xml = std::fs::read_to_string(p)
            .map_err(|e| ParityError::Invalid(format!("{} nicht lesbar: {e}", p.display())))?;
        results.extend(results::parse_junit(&xml).map_err(|e| match e {
            ParityError::Invalid(m) => ParityError::Invalid(format!("{}: {m}", p.display())),
            other => other,
        })?);
    }

    let previous: Option<Value> = args
        .write
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok());
    let report = compute(
        &package_specs,
        &scans,
        targets_found,
        &results,
        previous.as_ref(),
    );

    let mut report = report;
    apply_parity(&mut report, args)?;

    if let Some(out) = &args.write {
        if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&report.to_json())
            .map_err(|e| ParityError::Invalid(format!("Status nicht serialisierbar: {e}")))?;
        std::fs::write(out, text + "\n")?;
    }
    print!("{}", report.summary_text());
    let strict_ok = !args.strict || report.ignored_without_valid_reason.is_empty();
    Ok(report.open_count() == 0 && strict_ok)
}

/// Ergänzt frische Paritätsbelege; gemeinsame Statusregeln für beide Meilensteine.
pub(crate) fn apply_parity(
    report: &mut CoverageReport,
    args: &CoverageArgs,
) -> Result<(), ParityError> {
    if let (Some(rep), Some(ev)) = (&args.parity_report, &args.evidence) {
        let parity: Value =
            serde_json::from_str(&std::fs::read_to_string(rep).map_err(|e| {
                ParityError::Invalid(format!("{} nicht lesbar: {e}", rep.display()))
            })?)
            .map_err(|e| ParityError::Invalid(format!("{}: {e}", rep.display())))?;
        let evidence = evidence::load_evidence(ev)?;
        for (id, res) in evidence::evaluate(&evidence, &parity) {
            let Some(entry) = report.specs.get_mut(&id) else {
                report
                    .warnings
                    .push(format!("evidence.toml nennt unbekannte ID {id}"));
                continue;
            };
            match res {
                Ok(proof) if entry.status == Status::Implemented => {
                    entry.status = Status::Verified;
                    entry.parity = Some(proof);
                }
                Ok(_) => {}
                Err(why) => {
                    if matches!(entry.status, Status::Implemented | Status::Verified) {
                        entry.status = Status::Implemented;
                        entry.parity = Some(json!({ "not_verified": why }));
                    }
                }
            }
        }
        let count = |st: Status| report.specs.values().filter(|s| s.status == st).count();
        let (implemented, verified) = (count(Status::Implemented), count(Status::Verified));
        report.summary.insert("implemented".into(), implemented);
        report.summary.insert("verified".into(), verified);
    }

    Ok(())
}

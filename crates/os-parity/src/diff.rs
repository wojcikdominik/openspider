//! `os-parity diff`: vergleicht einen Referenz-Export (Referenzprodukt, Free-Version) mit dem OpenSpider-Export
//! anhand eines Spaltenmappings (Abschnitt 5.2/5.4).
//!
//! Ablauf: beide CSVs tolerant einlesen (Beschreibungsspalten der Referenz verwerfen), Basis-URLs zu
//! `{BASE}`/`{EXT}` normalisieren, Zeilen über den (ggf. mehrfachen) Schlüssel mit laufender Nummer
//! zuordnen, Zellen nach den Spaltenoptionen vergleichen, dokumentierte Abweichungen anwenden und die
//! Schwellen prüfen.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::csvio::{read_output, read_reference, Table};
use crate::deviations::{Deviation, Deviations, Scope};
use crate::error::ParityError;
use crate::mapping::{Mapping, RefFormat, ORDINAL_KEY};
use crate::normalize::{BaseUrls, CellRules};
use crate::report::{
    max_examples, write_report, AppliedDeviation, ColumnReport, Example, IgnoredColumn, Report,
};
use crate::thresholds::Thresholds;

/// Argumente von `os-parity diff`.
#[derive(Debug, Clone, clap::Args)]
pub struct DiffArgs {
    /// CSV der Referenz.
    #[arg(long = "ref")]
    pub reference: PathBuf,
    /// CSV von OpenSpider.
    #[arg(long)]
    pub os: PathBuf,
    /// Spaltenmapping (TOML); der Dateiname ohne Endung ist der Export-Name für Schwellen und Abweichungen.
    #[arg(long)]
    pub mapping: PathBuf,
    /// Basis-URL der Referenz-Site (wird zu `{BASE}` normalisiert).
    #[arg(long)]
    pub ref_base: String,
    /// Basis-URL der OpenSpider-Site.
    #[arg(long)]
    pub os_base: String,
    /// Ausgabeordner für `report.json` und `report.md`.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Abweichungen (`deviations.toml`); ohne Angabe keine.
    #[arg(long)]
    pub deviations: Option<PathBuf>,
    /// Schwellen (`thresholds.toml`); ohne Angabe die Standardwerte aus A3.
    #[arg(long)]
    pub thresholds: Option<PathBuf>,
    /// Name der Site für `site`-Einschränkungen in den Abweichungen.
    #[arg(long)]
    pub site: Option<String>,
}

/// Eingaben des Vergleichs.
pub struct CompareInput<'a> {
    /// Export-Name (Mapping-Name).
    pub export: &'a str,
    /// Site.
    pub site: Option<&'a str>,
    /// Tabelle der Referenz (Beschreibungsspalten bereits verworfen).
    pub reference: &'a Table,
    /// Tabelle von OpenSpider.
    pub os: &'a Table,
    /// Mapping.
    pub mapping: &'a Mapping,
    /// Basis-URLs der Referenz.
    pub ref_bases: &'a BaseUrls,
    /// Basis-URLs von OpenSpider.
    pub os_bases: &'a BaseUrls,
    /// Für diesen Export geltende Abweichungen.
    pub deviations: &'a [Deviation],
}

struct KeyedRow {
    /// Eindeutige Kennung (Schlüsselwerte + laufende Nummer).
    id: String,
    /// Anzeigeform ohne laufende Nummer (Grundlage für `deviations.toml`).
    tuple: String,
    /// Anzeigeform mit laufender Nummer.
    display: String,
    /// Wievieltes Vorkommen dieses Schlüssels (0 = erstes).
    ordinal: usize,
}

struct ColumnPlan<'a> {
    map: &'a crate::mapping::ColumnMap,
    /// `None`: nummerierte Spalte, die die Referenz nur bei vorhandenen Daten schreibt – gilt als leer.
    ref_idx: Option<usize>,
    os_idx: Option<usize>,
    report: ColumnReport,
}

const SEP: char = '\u{1f}';

/// Nummerierte Spalte (`Title 2`, `H1-2`, `HTML hreflang 3 URL`): enthält ein Wort, das nur aus Ziffern besteht.
/// Die Referenz schreibt solche Spalten nur, wenn Daten vorliegen; fehlen sie, sind alle Zellen leer.
pub fn is_numbered_column(name: &str) -> bool {
    name.split(|c: char| c.is_whitespace() || c == '-')
        .any(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()))
}

// Nur der zusätzliche Endzielblock darf auf die Final-Spalten zeigen.
fn terminal_column<'a>(
    mapping: &Mapping,
    table: &Table,
    row: &[String],
    name: &'a str,
) -> Option<&'a str> {
    if !mapping.chain_end_target {
        return None;
    }
    let (kind, n) = name.rsplit_once(' ')?;
    let n: usize = n.parse().ok()?;
    let count = [
        "Number of Redirects",
        "Number of Redirects/Canonicals",
        "Number of Canonicals",
    ]
    .iter()
    .find_map(|c| table.index_of(c).and_then(|i| row[i].parse::<usize>().ok()))?;
    if n != count + 1 {
        return None;
    }
    match kind {
        "Status Code" => Some("Final Status Code"),
        "Status" => Some("Final Status"),
        _ => None,
    }
}

fn key_rows(
    table: &Table,
    key_cols: &[(usize, CellRules)],
    has_ordinal: bool,
    bases: &BaseUrls,
    is_reference: bool,
) -> Vec<KeyedRow> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    table
        .rows
        .iter()
        .map(|row| {
            let parts: Vec<String> = key_cols
                .iter()
                .map(|(i, rules)| {
                    let v = rules.prepare(&row[*i], bases);
                    let v = if is_reference {
                        rules.map_reference(&v)
                    } else {
                        v
                    };
                    if rules.ignore_case {
                        v.to_lowercase()
                    } else {
                        v
                    }
                })
                .collect();
            let tuple_id = parts.join(&SEP.to_string());
            let n = seen.entry(tuple_id.clone()).or_insert(0);
            let ordinal = *n;
            *n += 1;
            let tuple = parts.join(" | ");
            let display = if has_ordinal || ordinal > 0 {
                format!("{tuple} | #{}", ordinal + 1)
            } else {
                tuple.clone()
            };
            KeyedRow {
                id: format!("{tuple_id}{SEP}{ordinal}"),
                tuple,
                display,
                ordinal,
            }
        })
        .collect()
}

/// Blockformat: Zeile der Referenz, die kein M1-Filter ist (und deshalb in OpenSpider fehlen darf)?
/// Ohne Filterliste im Mapping gilt jede Zeile als relevant.
fn blocks_outside_m1(mapping: &Mapping, tuple: &str) -> bool {
    if mapping.format != RefFormat::Blocks {
        return false;
    }
    let Some(m1) = &mapping.blocks.m1_filters else {
        return false;
    };
    let mut parts = tuple.splitn(2, " | ");
    let (Some(section), Some(item)) = (parts.next(), parts.next()) else {
        return false;
    };
    !m1.contains(&(section.to_lowercase(), item.to_lowercase()))
}

fn clip(s: &str) -> String {
    if s.chars().count() > 200 {
        let cut: String = s.chars().take(197).collect();
        format!("{cut}...")
    } else {
        s.to_string()
    }
}

/// Führt den Vergleich aus. Schwellen werden mit [`evaluate`] angewandt.
pub fn compare(input: &CompareInput) -> Result<Report, ParityError> {
    // OUT-015 / OUT-085: upload-order exports retain the original input, including
    // duplicates and invalid URLs. Address can be empty or normalized and is not
    // the identity of an uploaded row. Keep older ordered formats compatible.
    if input.mapping.ordered
        && input
            .mapping
            .key
            .first()
            .is_some_and(|key| key == "Address")
        && input.reference.index_of("Original Url").is_some()
        && input.os.index_of("Original Url").is_some()
    {
        let mut mapping = input.mapping.clone();
        mapping.key[0] = "Original Url".into();
        return compare(&CompareInput {
            mapping: &mapping,
            ..*input
        });
    }
    let CompareInput {
        export,
        site,
        reference,
        os,
        mapping,
        ref_bases,
        os_bases,
        deviations,
    } = input;

    if reference.columns.is_empty() {
        return Err(ParityError::Invalid(
            "Referenz-Export ist leer (keine Kopfzeile)".into(),
        ));
    }
    if os.columns.is_empty() {
        return Err(ParityError::Invalid(
            "OpenSpider-Export ist leer (keine Kopfzeile)".into(),
        ));
    }

    // Schlüsselspalten auflösen.
    let has_ordinal = mapping.key.iter().any(|k| k == ORDINAL_KEY);
    let mut ref_keys = Vec::new();
    let mut os_keys = Vec::new();
    let mut key_ref_names: Vec<&str> = Vec::new();
    for k in mapping.key.iter().filter(|k| k.as_str() != ORDINAL_KEY) {
        let ref_i = reference.index_of(k).ok_or_else(|| {
            ParityError::Invalid(format!("Schlüsselspalte {k:?} fehlt in der Referenz"))
        })?;
        let os_name = mapping.os_name_of_key(k);
        let os_i = os.index_of(&os_name).ok_or_else(|| {
            ParityError::Invalid(format!(
                "Schlüsselspalte {os_name:?} fehlt im OpenSpider-Export"
            ))
        })?;
        let rules = mapping.key_rules(k);
        ref_keys.push((ref_i, rules.clone()));
        os_keys.push((os_i, rules));
        key_ref_names.push(k);
    }
    if ref_keys.is_empty() {
        return Err(ParityError::Invalid(
            "Schlüssel enthält keine Spalte (nur \"#\")".into(),
        ));
    }

    let ref_rows = key_rows(reference, &ref_keys, has_ordinal, ref_bases, true);
    let os_rows = key_rows(os, &os_keys, has_ordinal, os_bases, false);
    let os_index: HashMap<&str, usize> = os_rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.as_str(), i))
        .collect();
    let ref_ids: std::collections::HashSet<&str> = ref_rows.iter().map(|r| r.id.as_str()).collect();

    // Spaltenplan in Dateireihenfolge der Referenz.
    let mut plans: Vec<ColumnPlan> = Vec::new();
    let mut ignored_columns = Vec::new();
    let mut unmapped_ref_columns = Vec::new();
    let mut missing_os_columns = Vec::new();
    for (ref_idx, name) in reference.columns.iter().enumerate() {
        if key_ref_names.contains(&name.as_str()) {
            continue;
        }
        match mapping.column(name) {
            None => unmapped_ref_columns.push(name.clone()),
            Some(cm) if cm.ignore => ignored_columns.push(IgnoredColumn {
                column: name.clone(),
                reason: cm.reason.clone(),
            }),
            Some(cm) => {
                let os_idx = os.index_of(&cm.os);
                if os_idx.is_none()
                    && !(mapping.chain_end_target
                        && is_numbered_column(name)
                        && reference.rows.iter().all(|row| {
                            let value = row[ref_idx].trim();
                            value.is_empty()
                                || value.parse::<f64>() == Ok(0.0)
                                || terminal_column(mapping, reference, row, name).is_some()
                        }))
                {
                    missing_os_columns.push(cm.os.clone());
                }
                plans.push(ColumnPlan {
                    map: cm,
                    ref_idx: Some(ref_idx),
                    os_idx,
                    report: ColumnReport {
                        ref_column: cm.reference.clone(),
                        os_column: cm.os.clone(),
                        compared: 0,
                        errors: 0,
                        examples: Vec::new(),
                    },
                });
            }
        }
    }
    // Nummerierte Spalten, die die Referenz nicht schreibt, die OpenSpider aber liefert: Referenz = leer.
    for cm in mapping.columns.iter().filter(|c| {
        !c.ignore
            && !key_ref_names.contains(&c.reference.as_str())
            && reference.index_of(&c.reference).is_none()
            && is_numbered_column(&c.reference)
            && os.index_of(&c.os).is_some()
    }) {
        plans.push(ColumnPlan {
            map: cm,
            ref_idx: None,
            os_idx: os.index_of(&cm.os),
            report: ColumnReport {
                ref_column: cm.reference.clone(),
                os_column: cm.os.clone(),
                compared: 0,
                errors: 0,
                examples: Vec::new(),
            },
        });
    }
    let missing_ref_columns: Vec<String> = mapping
        .columns
        .iter()
        .filter(|c| {
            !c.ignore
                && !key_ref_names.contains(&c.reference.as_str())
                && reference.index_of(&c.reference).is_none()
                && !is_numbered_column(&c.reference)
                && !c.optional_ref
                && !plans.iter().any(|p| p.map.reference == c.reference)
                && !mapping
                    .drop_ref_columns
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case(&c.reference))
        })
        .map(|c| c.reference.clone())
        .collect();
    let mapped_os_names: Vec<String> = mapping
        .columns
        .iter()
        .map(|c| c.os.clone())
        .chain(key_ref_names.iter().map(|k| mapping.os_name_of_key(k)))
        .collect();
    let unmapped_os_columns: Vec<String> = os
        .columns
        .iter()
        .filter(|c| !mapped_os_names.contains(c))
        .cloned()
        .collect();

    let max_n = max_examples();
    // Abweichungszähler.
    let mut dev_hits: Vec<(u64, Vec<String>)> = vec![(0, Vec::new()); deviations.len()];
    let mut hit = |idx: usize, key: &str| {
        let h = &mut dev_hits[idx];
        h.0 += 1;
        if h.1.len() < max_n {
            h.1.push(key.to_string());
        }
    };

    let mut matched = 0u64;
    let mut cells_compared = 0u64;
    let mut cells_equal = 0u64;
    let mut missing_in_os = Vec::new();
    let mut ignored_ref_rows = Vec::new();
    let mut os_only_rows = Vec::new();
    for (ri, srow) in ref_rows.iter().enumerate() {
        if mapping.dedupe_reference && srow.ordinal > 0 {
            continue;
        }
        // Blockformat: Zeilen zu Filtern außerhalb von M1 werden nur gezählt, auch wenn OpenSpider ab M2 eine
        // eigene Zeile dazu liefert (Welle 1 setzt weitere Tabs um); der Vergleich folgt mit den M2-Mappings (P01).
        if blocks_outside_m1(mapping, &srow.tuple) {
            ignored_ref_rows.push(srow.display.clone());
            continue;
        }
        let Some(&oi) = os_index.get(srow.id.as_str()) else {
            match deviations
                .iter()
                .position(|d| d.excuses_row(Scope::MissingInOs, &srow.tuple))
            {
                Some(d) => hit(d, &srow.display),
                None => missing_in_os.push(srow.display.clone()),
            }
            continue;
        };
        matched += 1;
        let ref_cells = &reference.rows[ri];
        let os_cells = &os.rows[oi];
        for plan in plans.iter_mut() {
            let rules = &plan.map.rules;
            let ref_raw = plan.ref_idx.map_or("", |i| ref_cells[i].as_str());
            let ref_value = rules.map_reference(&rules.prepare(ref_raw, ref_bases));
            let terminal = terminal_column(mapping, reference, ref_cells, &plan.map.reference);
            let os_idx = terminal.and_then(|name| os.index_of(name)).or(plan.os_idx);
            let os_value = match os_idx {
                Some(i) => rules.prepare(&os_cells[i], os_bases),
                None => String::new(),
            };
            if rules.equal(&ref_value, &os_value)
                || (plan.os_idx.is_none()
                    && terminal.is_none()
                    && ref_value.parse::<f64>() == Ok(0.0))
                || (plan.ref_idx.is_none()
                    && (os_value.is_empty() || os_value.parse::<f64>() == Ok(0.0)))
            {
                plan.report.compared += 1;
                cells_compared += 1;
                cells_equal += 1;
                continue;
            }
            if let Some(d) = deviations.iter().position(|d| {
                d.excuses_cell(&srow.tuple, &plan.map.reference, &ref_value, &os_value)
            }) {
                hit(d, &srow.display);
                continue;
            }
            plan.report.compared += 1;
            plan.report.errors += 1;
            cells_compared += 1;
            if plan.report.examples.len() < max_n {
                plan.report.examples.push(Example {
                    key: srow.display.clone(),
                    reference: clip(&ref_value),
                    os: clip(&os_value),
                });
            }
        }
    }
    let mut extra_in_os = Vec::new();
    for orow in os_rows.iter().filter(|r| !ref_ids.contains(r.id.as_str())) {
        if mapping.blocks.os_only_rows_ok {
            os_only_rows.push(orow.display.clone());
            continue;
        }
        match deviations
            .iter()
            .position(|d| d.excuses_row(Scope::ExtraInOs, &orow.tuple))
        {
            Some(d) => hit(d, &orow.display),
            None => extra_in_os.push(orow.display.clone()),
        }
    }

    // Die Zeilenfolge von OpenSpider hängt von der Entdeckungsreihenfolge ab: Listen sortiert für stabile Berichte.
    extra_in_os.sort();
    os_only_rows.sort();
    let agreement = if cells_compared == 0 {
        if missing_in_os.is_empty() && extra_in_os.is_empty() {
            100.0
        } else {
            0.0
        }
    } else {
        cells_equal as f64 * 100.0 / cells_compared as f64
    };
    let applied_deviations = deviations
        .iter()
        .zip(dev_hits)
        .filter(|(_, (n, _))| *n > 0)
        .map(|(d, (count, mut keys))| {
            // Die Reihenfolge der Beispiele hängt von der Crawl-Reihenfolge ab: für stabile Berichte sortieren.
            keys.sort();
            AppliedDeviation {
                id: d.id.clone(),
                spec: d.spec.clone(),
                kind: d.kind.clone(),
                note: d.note.clone(),
                scope: d.scope,
                count,
                keys,
            }
        })
        .collect();

    Ok(Report {
        row_order_identical: mapping.ordered.then(|| {
            ref_rows
                .iter()
                .filter(|r| {
                    !deviations
                        .iter()
                        .any(|d| d.excuses_row(Scope::MissingInOs, &r.tuple))
                })
                .map(|r| &r.id)
                .eq(os_rows
                    .iter()
                    .filter(|r| {
                        !deviations
                            .iter()
                            .any(|d| d.excuses_row(Scope::ExtraInOs, &r.tuple))
                    })
                    .map(|r| &r.id))
        }),
        export: (*export).to_string(),
        site: site.map(str::to_string),
        ref_rows: reference.rows.len() as u64,
        os_rows: os.rows.len() as u64,
        matched,
        url_set_identical: missing_in_os.is_empty() && extra_in_os.is_empty(),
        missing_in_os,
        extra_in_os,
        cells_compared,
        cells_equal,
        agreement_pct: (agreement * 1000.0).round() / 1000.0,
        columns: plans.into_iter().map(|p| p.report).collect(),
        applied_deviations,
        dropped_ref_columns: reference.dropped.clone(),
        ignored_columns,
        unmapped_ref_columns,
        unmapped_os_columns,
        missing_ref_columns,
        missing_os_columns,
        ignored_ref_rows,
        os_only_rows,
        threshold_pct: 0.0,
        passed: false,
        failures: Vec::new(),
    })
}

/// Wendet die Schwellen an und setzt `threshold_pct`, `passed` und `failures`.
pub fn evaluate(report: &mut Report, thresholds: &Thresholds) {
    let need = thresholds.agreement_for(&report.export);
    report.threshold_pct = need;
    let mut failures = Vec::new();
    if report.row_order_identical == Some(false) {
        failures.push("Zeilenfolge weicht von der Upload-Reihenfolge ab (OUT-015)".into());
    }
    let enough = if report.cells_compared == 0 {
        report.url_set_identical
    } else {
        report.cells_equal as f64 * 100.0 >= need * report.cells_compared as f64 - 1e-9
    };
    if !enough {
        failures.push(format!(
            "Zellübereinstimmung {:.3} % liegt unter {:.3} %",
            report.agreement_pct, need
        ));
    }
    if thresholds.url_set_identical && !report.url_set_identical {
        failures.push(format!(
            "URL-Menge nicht identisch: {} fehlen in OpenSpider, {} zusätzlich",
            report.missing_in_os.len(),
            report.extra_in_os.len()
        ));
    }
    report.passed = failures.is_empty();
    report.failures = failures;
}

/// Name eines Mappings aus dem Dateinamen (`internal_all.toml` → `internal_all`).
pub fn export_name_of(mapping_path: &Path) -> String {
    mapping_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Führt den Vergleich aus; `Ok(true)` = alle Schwellen erfüllt.
pub fn run(args: &DiffArgs) -> Result<bool, ParityError> {
    let mapping = Mapping::load(&args.mapping)?;
    let reference = read_reference(&args.reference, &mapping)?;
    let os = read_output(&args.os, &mapping)?;
    let deviations = match &args.deviations {
        Some(p) => Deviations::load(p)?,
        None => Deviations::default(),
    };
    for w in &deviations.warnings {
        eprintln!("os-parity: Warnung: {w}");
    }
    let thresholds = match &args.thresholds {
        Some(p) => Thresholds::load(p)?,
        None => Thresholds::builtin(),
    };
    let export = export_name_of(&args.mapping);
    let devs = deviations.for_file(
        args.site.as_deref(),
        &export,
        args.reference.file_name().and_then(|f| f.to_str()),
    );
    let mut report = compare(&CompareInput {
        export: &export,
        site: args.site.as_deref(),
        reference: &reference,
        os: &os,
        mapping: &mapping,
        ref_bases: &BaseUrls::from_base(&args.ref_base),
        os_bases: &BaseUrls::from_base(&args.os_base),
        deviations: &devs,
    })?;
    evaluate(&mut report, &thresholds);
    if let Some(dir) = &args.out {
        write_report(dir, &report)?;
    }
    print!("{}", crate::report::render_markdown(&report));
    Ok(report.passed)
}

#[cfg(test)]
mod upload_format_tests {
    use super::*;

    // spec: OUT-015, OUT-085
    #[test]
    fn original_input_keys_preserve_duplicates_and_expose_changed_addresses() {
        let mapping = Mapping::parse(
            "ordered = true\nkey = ['Address', '#']\n[columns]\nAddress = 'Address'\n",
        )
        .unwrap();
        let reference = crate::csvio::parse_csv(
            b"Original Url,Address\nHTTP://example/a,http://example/a\nHTTP://example/a,http://example/a\nbad,\n",
            &[],
        )
        .unwrap();
        let mut os = reference.clone();
        os.rows[0][1] = "http://example/other".into();
        let bases = BaseUrls::from_base("http://example");
        let report = compare(&CompareInput {
            export: "list_internal",
            site: None,
            reference: &reference,
            os: &os,
            mapping: &mapping,
            ref_bases: &bases,
            os_bases: &bases,
            deviations: &[],
        })
        .unwrap();
        assert_eq!(report.matched, 3);
        assert!(report.url_set_identical);
        assert_eq!(report.row_order_identical, Some(true));
        assert_eq!(report.columns[0].ref_column, "Address");
        assert_eq!(report.columns[0].errors, 1);
        os.rows.swap(1, 2);
        let report = compare(&CompareInput {
            export: "list_internal",
            site: None,
            reference: &reference,
            os: &os,
            mapping: &mapping,
            ref_bases: &bases,
            os_bases: &bases,
            deviations: &[],
        })
        .unwrap();
        assert_eq!(report.row_order_identical, Some(false));
    }
}

//! Unabhängige Abnahmetests M4-V01 (1.1-Etappe): Exporte der Optimierungsvorschläge (AI-076) über die gemeinsame
//! Export-Pipeline (OUT-001 ff.), CSV (OUT-002), XLSX (OUT-003), Formel- und Injection-Schutz (OUT-016).
//!
//! Erwartungen aus `docs/spec/08-ai.md` AI-076: Export „Accepted“ mit `Address`, `Element`, `Current Value`,
//! `New Value`, `Length`, `Pixel Width`, `Origin` (`ai` / `edited`), `Model`, `Generated At` und genau den Zeilen mit
//! `Accepted` oder `Edited`; Export „All“ mit allen Kandidaten und Verstößen; „CSV-Injection-Schutz nach OUT-016“.
//! Exportnamen (Festlegung des Orchestrators): `AI Suggestions:Accepted Suggestions`, `AI Suggestions:All Suggestions`.
// acceptance: independent

#[path = "../../../tests/fixtures/ai/v01-acceptance/states.rs"]
mod states;

use calamine::{open_workbook, Data, Reader, Xlsx};
use os_export::{export_bulk, Format};
use os_model::CrawlConfig;
use states::*;

const ACCEPTED: &str = "AI Suggestions:Accepted Suggestions";
const ALL: &str = "AI Suggestions:All Suggestions";
const ACCEPTED_HEADER: [&str; 9] = [
    "Address",
    "Element",
    "Current Value",
    "New Value",
    "Length",
    "Pixel Width",
    "Origin",
    "Model",
    "Generated At",
];

fn config() -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.export.utf8_bom = false;
    cfg.export.line_ending = os_model::LineEnding::Lf;
    cfg
}

fn csv_records(text: &str) -> Vec<Vec<String>> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(text.trim_start_matches('\u{feff}').as_bytes())
        .records()
        .map(|r| r.unwrap().iter().map(str::to_owned).collect())
        .collect()
}

fn export(name: &str, format: Format) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let f = create(dir.path(), "export.ospider", config());
    let out = dir.path().join("out");
    let files =
        export_bulk(&f, &config(), name, &out, format).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    let path = files.last().expect("eine Datei").path.clone();
    (dir, path)
}

fn csv_of(name: &str) -> Vec<Vec<String>> {
    let (_d, path) = export(name, Format::Csv);
    csv_records(&std::fs::read_to_string(path).unwrap())
}

// spec: AI-076 [M4]
// spec: OUT-001 [M4]
// spec: OUT-016 [M4]
// acceptance: independent
// „Der Export „Accepted“ enthält genau die Zeilen mit `Accepted` oder `Edited`“, Spalten in der Reihenfolge der Spec,
// `Origin` `ai`/`edited`; Werte, die mit `=`, `+`, `-` oder `@` beginnen, sind in CSV mit `'` maskiert; Zahlenspalten
// bleiben Zahlen.
#[test]
fn accepted_csv_has_spec_columns_exact_rows_and_formula_protection() {
    let rows = csv_of(ACCEPTED);
    assert_eq!(rows[0], ACCEPTED_HEADER, "Kopfzeile");
    let body = &rows[1..];
    let mut got: Vec<String> = body.iter().map(|r| r[0].clone()).collect();
    got.sort();
    assert_eq!(got, addresses(&["/accepted/", "/edited/", "/formula/"]));
    let row = |p: &str| body.iter().find(|r| r[0] == address(p)).unwrap().clone();

    let acc = row("/accepted/");
    assert_eq!(acc[1], "Title");
    assert_eq!(acc[2], "Wanderschuhe");
    assert_eq!(acc[3], V_ACC);
    assert_eq!(acc[4], V_ACC.chars().count().to_string());
    assert_eq!(acc[5], "380");
    assert_eq!(acc[6], "ai");
    assert_eq!(acc[7], MODEL);
    assert_eq!(acc[8], GENERATED_AT);

    let ed = row("/edited/");
    assert_eq!(
        ed[3],
        format!("'{V_EDITED}"),
        "OUT-016: führendes = maskiert"
    );
    assert_eq!(ed[5], "600");
    assert_eq!(ed[6], "edited");

    let fo = row("/formula/");
    assert_eq!(
        fo[2], "'=HYPERLINK(\"http://evil\")",
        "aktueller Wert maskiert"
    );
    assert_eq!(fo[3], format!("'{V_FORMULA}"), "führendes + maskiert");
}

// spec: AI-076 [M4]
// spec: OUT-016 [M4]
// acceptance: independent
// Export „All“: jeder Kandidat der Fixture mit seinen Verstößen; Formelwerte (`+`, `@`, `-`) maskiert.
#[test]
fn all_csv_lists_every_candidate_with_violations() {
    let rows = csv_of(ALL);
    let header = &rows[0];
    assert_eq!(&header[..2], ["Address", "Element"]);
    let body = &rows[1..];
    for r in records() {
        let path = ROWS[(r.url_id - 1) as usize].path;
        for c in &r.candidates {
            let masked = if c.value.starts_with(['=', '+', '-', '@']) {
                format!("'{}", c.value)
            } else {
                c.value.clone()
            };
            let hit = body
                .iter()
                .find(|row| row[0] == address(path) && row.contains(&masked))
                .unwrap_or_else(|| panic!("Kandidat {masked:?} von {path} fehlt"));
            for v in &c.violations {
                assert!(
                    hit.iter().any(|cell| cell.contains(v.as_str())),
                    "Verstoß {v} von {path} fehlt: {hit:?}"
                );
            }
        }
    }
    for value in [V_FORMULA, V_FORMULA_2, V_FORMULA_3] {
        assert!(
            !body.iter().any(|r| r.contains(&value.to_string())),
            "unmaskierter Wert {value:?}"
        );
    }
}

fn xlsx_rows(name: &str) -> (Vec<Vec<Data>>, bool) {
    let (_d, path) = export(name, Format::Xlsx);
    let mut wb: Xlsx<_> = open_workbook(&path).unwrap();
    let sheet = wb.sheet_names()[0].clone();
    let range = wb.worksheet_range(&sheet).unwrap();
    let rows = range.rows().map(|r| r.to_vec()).collect();
    let formulas = wb.worksheet_formula(&sheet).unwrap();
    let no_formula = formulas.rows().all(|r| r.iter().all(|f| f.is_empty()));
    (rows, no_formula)
}

fn cell(d: &Data) -> String {
    match d {
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => f.to_string(),
        Data::Empty => String::new(),
        other => format!("{other:?}"),
    }
}

// spec: AI-076 [M4]
// spec: OUT-003 [M4]
// acceptance: independent
// XLSX „Accepted“: gleiche Kopfzeile und Zeilen; `Length`/`Pixel Width` als Zahlenzellen; kein Zellwert als Formel.
#[test]
fn accepted_xlsx_has_spec_columns_numbers_and_no_formulas() {
    let (rows, no_formula) = xlsx_rows(ACCEPTED);
    assert!(no_formula, "keine Formel in der Datei");
    let header: Vec<String> = rows[0].iter().map(cell).collect();
    assert_eq!(header, ACCEPTED_HEADER);
    let mut got: Vec<String> = rows[1..].iter().map(|r| cell(&r[0])).collect();
    got.sort();
    assert_eq!(got, addresses(&["/accepted/", "/edited/", "/formula/"]));
    let acc = rows[1..]
        .iter()
        .find(|r| cell(&r[0]) == address("/accepted/"))
        .unwrap();
    assert!(
        matches!(acc[4], Data::Int(_) | Data::Float(_)),
        "Length als Zahl: {:?}",
        acc[4]
    );
    assert!(
        matches!(acc[5], Data::Int(_) | Data::Float(_)),
        "Pixel Width als Zahl: {:?}",
        acc[5]
    );
}

// spec: OUT-003 [M4]
// spec: OUT-016 [M4]
// acceptance: independent
// OUT-016 ist ein Schutz „für CSV“; in XLSX gilt OUT-003: „Kein Zellwert wird je als Formel geschrieben; ein Title
// `=SUM(A1)` erscheint in Excel als Text `=SUM(A1)`“ – also ohne vorangestelltes `'`.
#[test]
fn xlsx_shows_formula_like_values_as_plain_text() {
    let (rows, no_formula) = xlsx_rows(ACCEPTED);
    assert!(no_formula);
    let ed = rows[1..]
        .iter()
        .find(|r| cell(&r[0]) == address("/edited/"))
        .unwrap();
    assert_eq!(cell(&ed[3]), V_EDITED, "Text ohne Maskierung");
    let fo = rows[1..]
        .iter()
        .find(|r| cell(&r[0]) == address("/formula/"))
        .unwrap();
    assert_eq!(cell(&fo[2]), "=HYPERLINK(\"http://evil\")");
}

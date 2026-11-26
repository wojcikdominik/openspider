//! Exporte der Optimierungsvorschläge (08 AI-076; M4-V01): „AI Suggestions: Accepted“ (übernommene Werte für die
//! Umsetzung im CMS) und „AI Suggestions: All“ (alle Kandidaten mit Verstößen).
//!
//! Kanonische Namen: `AI Suggestions:Accepted Suggestions` und `AI Suggestions:All Suggestions`. Die Namen aus der Spec
//! (`AI Suggestions: Accepted`/`: All`) ergäben dieselben Dateinamen wie die Tab-Exporte der Filter `Accepted`/`All`
//! (Kollisionsprüfung OUT-008, Hinweis aus M4-F01); Auslegung im Bericht M4-V01. Die Einträge stehen bei den
//! konfigurationsabhängigen Exporten ([`dynamic_entries`]), [`entries`] bleibt leer. Mit ausgeschaltetem Schalter
//! „KI (Vorschau)“ blendet os-app sie wie alle `AI Suggestions:…`-Namen aus.
//!
//! CSV-Injection-Schutz nach OUT-016 (AI-076): Textzellen mit Inhalten von Seite, Modell oder Nutzer (`Current Value`,
//! `New Value`, `Suggestion`), die mit `=`, `+`, `-` oder `@` beginnen, erhalten ein führendes `'`; Zahlenspalten
//! bleiben unverändert. Weil die allgemeine Exporteinstellung aus OUT-016 (P2) noch nicht existiert, gilt der Schutz
//! für diese beiden Exporte immer (Auslegung im Bericht M4-V01).
use os_analyze::defs::ColumnKind;
use os_model::{CrawlConfig, Value};
use os_store::ai::suggestions::{
    self, ExportRecord, ReviewStatus, ELEMENT_META_DESCRIPTION, ELEMENT_TITLE,
};

use super::{DynamicEntry, TableEntry, TableJob};
use crate::error::ExportError;
use crate::types::TableWriter;

/// Name des Exports der übernommenen Werte.
pub const ACCEPTED: &str = "AI Suggestions:Accepted Suggestions";
/// Name des Exports aller Kandidaten.
pub const ALL: &str = "AI Suggestions:All Suggestions";

/// Kopfzeile „Accepted“ (AI-076).
pub const ACCEPTED_HEADER: &[&str] = &[
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

/// Kopfzeile „All“: jede Zeile ein Kandidat (bzw. ein Eintrag ohne Kandidaten).
pub const ALL_HEADER: &[&str] = &[
    "Address",
    "Element",
    "Current Value",
    "Status",
    "Review Status",
    "Candidate",
    "Suggestion",
    "Length",
    "Pixel Width",
    "Valid",
    "Violations",
    "Round",
    "Selected",
    "Model",
    "Generated At",
];

/// Statische Einträge: keine (siehe Moduldokumentation).
pub fn entries() -> &'static [TableEntry] {
    &[]
}

/// Die beiden Exporte (unabhängig von der Konfiguration).
pub fn dynamic_entries(_cfg: &CrawlConfig) -> Vec<DynamicEntry> {
    vec![
        DynamicEntry {
            name: ACCEPTED.to_string(),
            label_key: None,
            file_stem: ACCEPTED.to_string(),
            header: ACCEPTED_HEADER.iter().map(|s| (*s).to_string()).collect(),
            run: Box::new(write_accepted),
        },
        DynamicEntry {
            name: ALL.to_string(),
            label_key: None,
            file_stem: ALL.to_string(),
            header: ALL_HEADER.iter().map(|s| (*s).to_string()).collect(),
            run: Box::new(write_all),
        },
    ]
}

const ELEMENTS: [(&str, &str); 2] = [
    (ELEMENT_TITLE, "Title"),
    (ELEMENT_META_DESCRIPTION, "Meta Description"),
];

fn records(job: &TableJob<'_>) -> Result<Vec<(&'static str, ExportRecord)>, ExportError> {
    let mut out = Vec::new();
    for (id, label) in ELEMENTS {
        for r in suggestions::export_records(job.file.conn(), id).map_err(ExportError::from)? {
            out.push((label, r));
        }
    }
    Ok(out)
}

fn text(v: Option<&str>) -> Value {
    v.map_or(Value::Null, |s| Value::Text(s.to_string()))
}

/// OUT-016: `=`, `+`, `-`, `@` am Zellanfang mit `'` maskieren.
pub fn mask(v: &str) -> String {
    if v.starts_with(['=', '+', '-', '@']) {
        format!("'{v}")
    } else {
        v.to_string()
    }
}

fn masked(v: Option<&str>) -> Value {
    v.map_or(Value::Null, |s| Value::Text(mask(s)))
}

fn int(v: Option<u32>) -> Value {
    v.map_or(Value::Null, |n| Value::Int(i64::from(n)))
}

fn generated(r: &ExportRecord) -> Value {
    text(
        r.record
            .generated_at
            .as_deref()
            .or(r.request_timestamp.as_deref()),
    )
}

/// Genau die Zeilen mit `Accepted` oder `Edited` (AI-076).
fn write_accepted(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    let kinds = [
        ColumnKind::Url,
        ColumnKind::Text,
        ColumnKind::Text,
        ColumnKind::Text,
        ColumnKind::Int,
        ColumnKind::Int,
        ColumnKind::Text,
        ColumnKind::Text,
        ColumnKind::Text,
    ];
    let mut count = 0;
    for (label, r) in records(job)? {
        if !matches!(r.record.review, ReviewStatus::Accepted | ReviewStatus::Edited) {
            continue;
        }
        let Some(a) = &r.record.accepted else {
            continue;
        };
        writer.checkpoint()?;
        writer.row(
            &kinds,
            &[
                Value::Text(r.address.clone()),
                Value::Text(label.to_string()),
                masked(r.current_value.as_deref()),
                Value::Text(mask(&a.value)),
                Value::Int(i64::from(a.length)),
                int(a.pixel_width),
                Value::Text(a.origin.as_str().to_string()),
                text(r.record.model.as_deref()),
                generated(&r),
            ],
        )?;
        count += 1;
    }
    Ok(count)
}

/// Alle Kandidaten mit Verstößen, in Antwortreihenfolge; Einträge ohne Kandidaten mit einer Zeile.
fn write_all(job: &TableJob<'_>, writer: &mut dyn TableWriter) -> Result<u64, ExportError> {
    let kinds = [
        ColumnKind::Url,
        ColumnKind::Text,
        ColumnKind::Text,
        ColumnKind::Text,
        ColumnKind::Text,
        ColumnKind::Int,
        ColumnKind::Text,
        ColumnKind::Int,
        ColumnKind::Int,
        ColumnKind::Bool,
        ColumnKind::Text,
        ColumnKind::Int,
        ColumnKind::Bool,
        ColumnKind::Text,
        ColumnKind::Text,
    ];
    let mut count = 0;
    for (label, r) in records(job)? {
        let rec = &r.record;
        let head = |writer: &mut dyn TableWriter, cells: [Value; 10]| -> Result<(), ExportError> {
            let mut values = vec![
                Value::Text(r.address.clone()),
                Value::Text(label.to_string()),
                masked(r.current_value.as_deref()),
                Value::Text(rec.status.clone()),
                Value::Text(rec.review.label().to_string()),
            ];
            values.extend(cells);
            writer.checkpoint()?;
            writer.row(&kinds, &values)
        };
        if rec.candidates.is_empty() {
            head(
                writer,
                [
                    Value::Null,
                    Value::Null,
                    Value::Null,
                    Value::Null,
                    Value::Null,
                    Value::Text(rec.violations.join("; ")),
                    Value::Null,
                    Value::Null,
                    text(rec.model.as_deref()),
                    generated(&r),
                ],
            )?;
            count += 1;
            continue;
        }
        for c in &rec.candidates {
            head(
                writer,
                [
                    Value::Int(i64::from(c.index)),
                    Value::Text(mask(&c.value)),
                    Value::Int(i64::from(c.length)),
                    int(c.pixel_width),
                    Value::Bool(c.valid),
                    Value::Text(c.violations.join("; ")),
                    Value::Int(i64::from(c.round)),
                    Value::Bool(rec.selected == Some(c.index)),
                    text(rec.model.as_deref()),
                    generated(&r),
                ],
            )?;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::export_bulk;
    use crate::types::Format;
    use os_store::ai::suggestions::{
        save, AcceptedValue, ElementRecord, Origin, StoredCandidate, STATUS_NO_VALID, STATUS_VALID,
    };

    fn golden_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/ai/suggest/golden/suggestions-accepted.csv")
    }

    fn file(dir: &std::path::Path) -> os_store::CrawlFile {
        use os_model::time::{IdSource, SeededIds};
        let created = os_model::Timestamp::from_db("2026-11-26T08:00:00.000Z").unwrap();
        let f = os_store::CrawlFile::create(
            &dir.join("export.ospider"),
            &os_store::CrawlInit {
                crawl_id: SeededIds::new(3).new_crawl_id(created),
                mode: os_model::CrawlMode::Spider,
                start_url: Some("http://ai.test:8080/".into()),
                list_source: None,
                project: "V01".into(),
                name: "export".into(),
                config: CrawlConfig::default(),
                created_at: created,
            },
            os_store::StorageProfile::Balanced,
        )
        .unwrap();
        f.conn()
            .execute_batch(
                "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length,
                   content_class,status_code,indexability,title_1,meta_desc_1) VALUES
                 (1,'http://ai.test:8080/a/','http://ai.test:8080/a/','http://ai.test:8080',1,'Done','Seed',1,23,'HTML',200,'Indexable','Wanderschuhe',NULL),
                 (2,'http://ai.test:8080/b/','http://ai.test:8080/b/','http://ai.test:8080',1,'Done','Seed',1,23,'HTML',200,'Indexable','=HYPERLINK(\"http://evil\")',NULL),
                 (3,'http://ai.test:8080/c/','http://ai.test:8080/c/','http://ai.test:8080',1,'Done','Seed',1,23,'HTML',200,'Indexable',NULL,NULL),
                 (4,'http://ai.test:8080/d/','http://ai.test:8080/d/','http://ai.test:8080',1,'Done','Seed',1,23,'HTML',200,'Indexable','Zelte',NULL);",
            )
            .unwrap();
        let cand = |index: u32, value: &str, valid: bool| StoredCandidate {
            index,
            value: value.into(),
            length: value.chars().count() as u32,
            pixel_width: Some(value.chars().count() as u32 * 9),
            valid,
            violations: if valid { Vec::new() } else { vec!["length<30".into()] },
            round: 1,
        };
        let base = |url_id: i64, status: &str, candidates: Vec<StoredCandidate>| {
            let mut r = ElementRecord::pending(url_id, ELEMENT_TITLE);
            r.status = status.into();
            r.selected = candidates.iter().find(|c| c.valid).map(|c| c.index);
            r.order = r.selected.into_iter().collect();
            r.candidates = candidates;
            r.model = Some("gpt-6-luna".into());
            r.generated_at = Some("2026-11-26T09:00:00Z".into());
            r
        };
        // /a/ übernommen (KI), /b/ bearbeitet mit Verstoß und Formelbeginn, /c/ offen, /d/ abgelehnt, Meta /a/ offen.
        let mut a = base(1, STATUS_VALID, vec![cand(1, "Leichte Wanderschuhe für Damen und Herren", true)]);
        a.review = ReviewStatus::Accepted;
        a.accepted = Some(AcceptedValue {
            value: "Leichte Wanderschuhe für Damen und Herren".into(),
            length: 41,
            pixel_width: Some(369),
            violations: Vec::new(),
            origin: Origin::Ai,
            candidate: Some(1),
            at: "2026-11-26T10:00:00Z".into(),
        });
        let mut b = base(2, STATUS_NO_VALID, vec![cand(1, "Kurz", false)]);
        b.review = ReviewStatus::Edited;
        b.accepted = Some(AcceptedValue {
            value: "=SUM(1,2) Rucksäcke".into(),
            length: 19,
            pixel_width: Some(171),
            violations: vec!["length<30".into(), "pixel_width<200".into()],
            origin: Origin::Edited,
            candidate: None,
            at: "2026-11-26T10:01:00Z".into(),
        });
        let c = base(3, STATUS_VALID, vec![cand(1, "Regenjacken für Wanderer: wasserdicht und leicht", true)]);
        let mut d = base(4, STATUS_VALID, vec![cand(1, "Zelte für Camping und Trekking im Vergleich", true)]);
        d.review = ReviewStatus::Rejected;
        let mut meta = base(1, STATUS_VALID, vec![cand(1, "-Rabatt: Wanderschuhe für Damen und Herren mit Testberichten und Pflegetipps.", true)]);
        meta.element = ELEMENT_META_DESCRIPTION.into();
        for r in [&a, &b, &c, &d, &meta] {
            save(f.conn(), r).unwrap();
        }
        f
    }

    // spec: AI-076 [M4]
    // Export „Accepted“ enthält genau die Zeilen mit `Accepted` oder `Edited` (Golden
    // `tests/fixtures/ai/suggest/golden/suggestions-accepted.csv`); CSV-Injection-Schutz nach OUT-016.
    #[test]
    fn accepted_export_has_exactly_accepted_and_edited_rows_and_masks_formulas() {
        let dir = tempfile::tempdir().unwrap();
        let f = file(dir.path());
        let out = export_bulk(&f, &CrawlConfig::default(), ACCEPTED, dir.path(), Format::Csv).unwrap();
        assert_eq!(out.len(), 1);
        let text = std::fs::read_to_string(&out[0].path).unwrap();
        let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[0].starts_with("Address,Element,Current Value,New Value,Length,Pixel Width,Origin,Model,Generated At"));
        assert!(text.contains("\"'=HYPERLINK(\"\"http://evil\"\")\""), "{text}");
        assert!(text.contains("'=SUM(1,2) Rucksäcke"));
        assert!(text.contains(",edited,"));
        assert!(!text.contains("Zelte für Camping"), "abgelehnt");
        os_testsite::golden::assert_golden(golden_path(), &text);
    }

    // spec: AI-076 [M4]
    // Export „All“ mit allen Kandidaten und Verstößen, je Kandidat eine Zeile; Zahlenspalten unmaskiert.
    #[test]
    fn all_export_lists_every_candidate_with_violations() {
        let dir = tempfile::tempdir().unwrap();
        let f = file(dir.path());
        let out = export_bulk(&f, &CrawlConfig::default(), ALL, dir.path(), Format::Csv).unwrap();
        let text = std::fs::read_to_string(&out[0].path).unwrap();
        let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
        assert_eq!(text.lines().count(), 6, "{text}");
        assert!(text.contains("Kurz"));
        assert!(text.contains("length<30"));
        assert!(text.contains("'-Rabatt"), "Formelbeginn auch in Kandidaten maskiert");
        assert!(crate::catalog::list(crate::catalog::ExportKind::Bulk).contains(&ALL.to_string()));
        assert_eq!(mask("-5"), "'-5");
        assert_eq!(mask("Wanderschuhe"), "Wanderschuhe");
    }
}

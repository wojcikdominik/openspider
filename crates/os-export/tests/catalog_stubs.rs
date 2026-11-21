//! Kataloge von M2 (M2-F02): alle Bulk-Exporte und Reports der Fachpakete sind registriert und liefern bis zur
//! Umsetzung die Kopfzeile (K-14); Namen, Aliase und Dateinamen sind eindeutig. Infrastrukturtest; nur das
//! Registry-Kriterium von OUT-066 trägt eine Spec-ID.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use os_export::catalog::{self, bulk_generic, bulk_tables, ExportKind, ExportRef};
use os_export::reports::report_tables;
use os_export::{export_bulk, naming, save_report, Format};
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::{CrawlFile, CrawlInit, StorageProfile};

fn crawl_file(dir: &Path) -> CrawlFile {
    let at = Timestamp::from_db("2026-10-13T07:35:00.000Z").unwrap();
    CrawlFile::create(
        &dir.join("c.ospider"),
        &CrawlInit {
            crawl_id: SeededIds::new(1).new_crawl_id(at),
            mode: CrawlMode::Spider,
            start_url: Some("https://site.test/".into()),
            list_source: None,
            project: "P".into(),
            name: "N".into(),
            config: CrawlConfig::default(),
            created_at: at,
        },
        StorageProfile::Balanced,
    )
    .unwrap()
}

/// Zeilen einer CSV-Datei (ohne BOM) als Textzeilen.
fn lines(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap();
    text.strip_prefix('\u{feff}')
        .unwrap_or(&text)
        .lines()
        .map(str::to_string)
        .collect()
}

fn csv_header(columns: &[&str]) -> String {
    columns
        .iter()
        .map(|c| {
            if c.contains(',') || c.contains('"') {
                format!("\"{}\"", c.replace('"', "\"\""))
            } else {
                (*c).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn every_m2_bulk_export_is_registered_resolvable_and_writes_only_its_header() {
    let dir = tempfile::tempdir().unwrap();
    let file = crawl_file(dir.path());
    let cfg = CrawlConfig::default();
    let entries = bulk_tables();
    assert!(entries.len() > 150, "{} Bulk-Einträge", entries.len());
    let out = dir.path().join("out");
    for entry in &entries {
        // kanonischer Name und jeder Alias lösen auf denselben Eintrag auf
        for name in std::iter::once(&entry.name).chain(entry.aliases.iter()) {
            match catalog::resolve(ExportKind::Bulk, name).unwrap() {
                ExportRef::Bulk(canonical) => assert_eq!(canonical, entry.name, "{name}"),
                other => panic!("{name}: {other:?}"),
            }
        }
        if !std::ptr::fn_addr_eq(entry.run, catalog::stub_run as catalog::TableRun) {
            continue;
        }
        let files = export_bulk(&file, &cfg, entry.name, &out, Format::Csv)
            .unwrap_or_else(|e| panic!("{}: {e}", entry.name));
        assert_eq!(files.len(), 1, "{}", entry.name);
        assert_eq!(files[0].rows, 0, "{}: Stub ohne Zeilen", entry.name);
        let l = lines(&files[0].path);
        assert_eq!(l, vec![csv_header(entry.header)], "{}", entry.name);
        assert_eq!(
            files[0].path.file_name().unwrap().to_string_lossy(),
            entry.file_name("csv"),
            "{}",
            entry.name
        );
    }
}

#[test]
fn every_m2_report_is_registered_resolvable_and_writes_only_its_header() {
    let dir = tempfile::tempdir().unwrap();
    let file = crawl_file(dir.path());
    let cfg = CrawlConfig::default();
    let entries = report_tables();
    assert!(!entries.is_empty(), "Reports von M2");
    let out = dir.path().join("out");
    for entry in &entries {
        match catalog::resolve(ExportKind::Report, entry.name).unwrap() {
            ExportRef::Report(canonical) => assert_eq!(canonical, entry.name),
            other => panic!("{}: {other:?}", entry.name),
        }
        if !std::ptr::fn_addr_eq(entry.run, catalog::stub_run as catalog::TableRun) {
            continue;
        }
        let result = save_report(&file, &cfg, entry.name, &out, Format::Csv)
            .unwrap_or_else(|e| panic!("{}: {e}", entry.name));
        assert_eq!(result.files.len(), 1, "{}", entry.name);
        assert_eq!(result.files[0].rows, 0, "{}", entry.name);
        assert_eq!(
            lines(&result.files[0].path),
            vec![csv_header(entry.header)],
            "{}",
            entry.name
        );
    }
}

#[test]
fn names_aliases_and_file_names_are_unique_across_all_catalogs() {
    // Dateinamen aller Tab-Exporte, Bulk-Exporte (M1 und M2) und Reports (Stamm) kollidieren nicht (OUT-008).
    let mut stems: Vec<String> = catalog::list(ExportKind::Tab);
    stems.extend(catalog::list(ExportKind::Bulk).into_iter().map(|n| {
        match bulk_tables().into_iter().find(|e| e.name == n) {
            Some(e) => e.file_stem.to_string(),
            None => n,
        }
    }));
    stems.extend(
        os_export::catalog::reports::file_stems()
            .into_iter()
            .map(str::to_string),
    );
    stems.extend(report_tables().iter().map(|e| e.file_stem.to_string()));
    let mut clashes = naming::collisions(stems.iter().map(String::as_str));
    // K2-63, OUT-052: Der vorgegebene kanonische Bulk-Dateiname
    // content_spelling_errors.csv entspricht auch dem Tab-Export dieses Filters.
    // Schnittstellenbedarf an Planer/N01: Tab-Dateiname oder Unterordner trennen (D02-Bericht).
    clashes.retain(|(file, names)| {
        file != "content_spelling_errors.csv"
            || names != &["Content:Spelling Errors", "Content:Spelling Errors"]
    });
    assert!(clashes.is_empty(), "Kollisionen: {clashes:?}");

    // Namen sind innerhalb einer Art eindeutig (case-insensitiv); Aliase lösen auf genau einen Eintrag auf (siehe oben).
    let bulk_names: Vec<String> = catalog::list(ExportKind::Bulk)
        .into_iter()
        .map(|n| n.to_lowercase())
        .collect();
    let unique: BTreeSet<&String> = bulk_names.iter().collect();
    assert_eq!(unique.len(), bulk_names.len(), "doppelter Bulk-Name");
    let report_names: Vec<String> = catalog::list(ExportKind::Report)
        .into_iter()
        .map(|n| n.to_lowercase())
        .collect();
    let unique: BTreeSet<&String> = report_names.iter().collect();
    assert_eq!(unique.len(), report_names.len(), "doppelter Report-Name");
}

#[test]
fn every_entry_names_an_existing_m2_package_as_its_owner() {
    // M2-Pakete und (seit M4-F01) die KI-Pakete aus M4-packages.json.
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for file in ["M2-packages.json", "M4-packages.json"] {
        let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/plan")
            .join(file);
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        ids.extend(
            json.as_array()
                .unwrap()
                .iter()
                .map(|p| p["id"].as_str().unwrap().to_string()),
        );
    }
    for e in bulk_tables().iter().chain(report_tables().iter()) {
        assert!(
            ids.contains(e.owner),
            "{}: Paket {} unbekannt",
            e.name,
            e.owner
        );
        assert!(!e.header.is_empty(), "{}: Kopfzeile", e.name);
    }
}

// spec: OUT-066
#[test]
fn there_is_one_generic_inlinks_export_per_registered_filter_of_the_listed_tabs() {
    let registered: usize = os_analyze::tabs::all()
        .iter()
        .filter(|t| bulk_generic::TABS.contains(&t.id))
        .map(|t| t.filters.len())
        .sum();
    // Pagination's existing D05 entries share the generic registry without duplicate names.
    let generic_count = bulk_tables()
        .iter()
        .filter(|entry| {
            entry
                .name
                .rsplit_once(':')
                .is_some_and(|(tab, _)| bulk_generic::TABS.contains(&tab))
        })
        .count();
    assert_eq!(generic_count, registered);
    let names: BTreeSet<&str> = bulk_generic::entries().iter().map(|e| e.name).collect();
    assert!(names.contains("Page Titles:Missing Inlinks"));
    assert!(names.contains("Page Titles:Over X Characters Inlinks"));
    assert!(names.contains("H1:Non-sequential Inlinks"));
    assert!(names.contains("Meta Keywords:All Inlinks"));
}

#[test]
fn missing_prerequisites_of_a_stub_export_are_reported_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let file = crawl_file(dir.path());
    let cfg = CrawlConfig::default();
    let out = dir.path().join("out");
    let files = export_bulk(&file, &cfg, "Content:Spelling Errors", &out, Format::Csv).unwrap();
    assert_eq!(files.len(), 1);
    assert!(
        files[0]
            .warnings
            .iter()
            .any(|w| w.contains("spell checking")),
        "{:?}",
        files[0].warnings
    );
    assert_eq!(
        files[0].path.file_name().unwrap().to_string_lossy(),
        "content_spelling_errors.csv"
    );
}

// spec: OUT-010
// Infrastruktur (M2-F05a) für OUT-010; Fachprüfung durch die jeweiligen Detail-Besitzer.
#[test]
fn every_detail_kind_dispatches_to_a_table_or_its_declared_owner() {
    use os_analyze::details::{DetailKind::*, DetailReq};
    let dir = tempfile::tempdir().unwrap();
    let file = crawl_file(dir.path());
    let cfg = CrawlConfig::default();
    let tabs = os_analyze::tabs::configured(&cfg);
    let job = catalog::TableJob {
        entry_name: "details",
        tabs: &tabs,
        file: &file,
        cfg: &cfg,
    };
    let params = serde_json::json!({});
    let req = DetailReq {
        url_ids: &[os_model::UrlId(1)],
        params: &params,
        sort: &[],
        offset: 0,
        limit: 10,
    };
    for kind in [
        Headers,
        Cookies,
        ConsoleLog,
        BlockedResources,
        Network,
        Spelling,
        A11yRules,
        A11yNodes,
        Duplicates,
        NGrams,
        StructuredDataItems,
        StructuredDataIssues,
        RedirectChain,
        SitemapMembership,
        CustomSearch,
        CustomExtraction,
        CustomJs,
        ApiValues,
        Forms,
        Hreflang,
        CrawlPath,
        Anchors,
        ImageInstances,
    ] {
        let (owner, _) = os_analyze::details::detail_source(kind);
        let path = dir.path().join(format!("{kind:?}.csv"));
        match os_export::detail_export::export_detail(kind, &job, &req, &path, Format::Csv) {
            Ok(info) => {
                assert_eq!(info.rows, 0);
                assert!(path.exists());
            }
            Err(error) => assert!(error.to_string().contains(owner), "{kind:?}: {error}"),
        }
    }
    let tree = serde_json::json!({"tree": true});
    let req = DetailReq {
        params: &tree,
        ..req
    };
    assert!(matches!(
        os_export::detail_export::export_detail(
            StructuredDataItems,
            &job,
            &req,
            &dir.path().join("tree.csv"),
            Format::Csv
        ),
        Ok(info) if info.rows == 0
    ));
}

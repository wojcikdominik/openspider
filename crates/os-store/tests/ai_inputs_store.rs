//! M4-D05: gespeicherte KI-Eingaben (08 AI-032, Nachträge K4-24 und K4-26) und die erteilte Kostenbestätigung in
//! `ai_runs.confirmed_at` (08 AI-020, Nachtrag 27.11.2026, K4-27) in der Crawl-Datei (Migration `0006_ai.sql`).
use os_model::ai::hook::STORED_PAGE_TEXT_MAX_CHARS;
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::ai::inputs::{self, StoredImage, StoredInputs};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};

fn fresh(dir: &std::path::Path) -> CrawlFile {
    let created = Timestamp::from_db("2026-11-27T10:00:00.000Z").unwrap();
    let init = CrawlInit {
        crawl_id: SeededIds::new(505).new_crawl_id(created),
        mode: CrawlMode::Spider,
        start_url: Some("http://ai.test/".into()),
        list_source: None,
        project: "D05".into(),
        name: "D05".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    CrawlFile::create(&dir.join("d05.ospider"), &init, StorageProfile::Balanced).unwrap()
}

// spec: AI-032 [M4]
// Tabelle der gespeicherten KI-Eingaben mit zstd-komprimiertem Inhalt (wie `page_html`), `html_lang` und
// `structured_data_types` (NULL = Extraktion aus, unterscheidbar von leerer Liste); Text höchstens 20.000 Zeichen an einer
// Zeichengrenze; Lesen nach Neuöffnen unverändert; die Tabellen von 1.0 ändern sich nicht.
#[test]
fn stored_inputs_roundtrip_with_limit_and_null_types() {
    let dir = tempfile::tempdir().unwrap();
    let file = fresh(dir.path());
    let text = "Wanderschuhe sind leicht. ".repeat(1_000);
    let a = StoredInputs {
        page_text: Some(text.clone()),
        h2: vec!["Pflege".into(), "Größen".into()],
        images: vec![StoredImage {
            src: "http://ai.test/a.png".into(),
            alt: Some("Schuh".into()),
            context: Some("Bildunterschrift".into()),
        }],
        html_lang: Some("de-DE".into()),
        structured_data_types: None,
    };
    let b = StoredInputs {
        structured_data_types: Some(Vec::new()),
        ..StoredInputs::default()
    };
    let c = StoredInputs {
        structured_data_types: Some(vec!["Product".into(), "Offer".into()]),
        ..StoredInputs::default()
    };
    let bytes = inputs::write_all(file.conn(), &[(1, a.clone()), (2, b), (3, c)]).unwrap();
    assert!(bytes < text.len(), "komprimiert: {bytes} < {}", text.len());
    drop(file);
    let file = CrawlFile::open(&dir.path().join("d05.ospider"), OpenMode::ReadOnly).unwrap();
    let got = inputs::read(file.conn(), 1).unwrap().unwrap();
    assert_eq!(
        got.page_text.as_ref().unwrap().chars().count(),
        STORED_PAGE_TEXT_MAX_CHARS
    );
    assert!(text.starts_with(got.page_text.as_deref().unwrap()));
    assert_eq!(got.h2, a.h2);
    assert_eq!(got.images, a.images);
    assert_eq!(got.html_lang.as_deref(), Some("de-DE"));
    assert_eq!(got.structured_data_types, None);
    assert_eq!(
        inputs::read(file.conn(), 2)
            .unwrap()
            .unwrap()
            .structured_data_types,
        Some(Vec::new())
    );
    assert_eq!(
        inputs::read(file.conn(), 3)
            .unwrap()
            .unwrap()
            .structured_data_types,
        Some(vec!["Product".to_string(), "Offer".to_string()])
    );
    let (compression, sd): (String, Option<String>) = file
        .conn()
        .query_row(
            "SELECT compression, structured_data_types FROM page_ai_inputs WHERE url_id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((compression.as_str(), sd), ("zstd", None));
    let size = inputs::size(file.conn()).unwrap();
    assert_eq!(size.entries, 3);
    assert_eq!(inputs::read_all(file.conn()).unwrap().len(), 3);
}

// spec: AI-020 [M4]
// K4-27: `confirmed_at` der Laufzeile – leer bis zur Bestätigung, danach der Zeitpunkt, auch nach Neuöffnen.
#[test]
fn confirmed_at_is_kept_in_the_run_row() {
    let dir = tempfile::tempdir().unwrap();
    let file = fresh(dir.path());
    let run =
        os_store::ai::answers::begin_run(file.conn(), "2026-11-27T10:00:00Z", None, "{}").unwrap();
    assert_eq!(
        os_store::ai::answers::confirmed_at(file.conn(), run).unwrap(),
        None
    );
    os_store::ai::answers::set_confirmed_at(file.conn(), run, "2026-11-27T10:00:05Z").unwrap();
    drop(file);
    let file = CrawlFile::open(&dir.path().join("d05.ospider"), OpenMode::ReadOnly).unwrap();
    assert_eq!(
        os_store::ai::answers::confirmed_at(file.conn(), run)
            .unwrap()
            .as_deref(),
        Some("2026-11-27T10:00:05Z")
    );
    assert_eq!(
        os_store::ai::answers::confirmed_at(file.conn(), run + 1).unwrap(),
        None
    );
}

// spec: AI-032 [M4]
// Die Archivdatei Schema v6 (neu erzeugt nach der Ergänzung von 0006) enthält die Tabelle der KI-Eingaben und die
// Spalte `confirmed_at`; eine Datei mit Schema v5 erhält beides bei der Migration.
#[test]
fn schema_v6_archive_and_migrated_v5_have_the_new_table_and_column() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/migrations");
    let dir = tempfile::tempdir().unwrap();
    for v in ["schema-v6", "schema-v5"] {
        let path = dir.path().join(format!("{v}.ospider"));
        std::fs::copy(root.join(v).join("crawl.ospider"), &path).unwrap();
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        let tables: i64 = file
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'page_ai_inputs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1, "{v}");
        let cols: i64 = file
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('ai_runs') WHERE name = 'confirmed_at'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cols, 1, "{v}");
    }
}

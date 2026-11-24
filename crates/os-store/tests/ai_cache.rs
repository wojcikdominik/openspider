//! Tabelle `ai_cache` (AI-015): Speichern nur von Erfolg und Refusal, Lesen, Leeren, Statistik und Übernahme aus einer
//! anderen Crawl-Datei, deren Bytes dabei unverändert bleiben.
mod store_common;

use std::path::{Path, PathBuf};

use os_store::ai::cache::{self, CacheRow};
use os_store::{CrawlFile, OpenMode, StorageProfile, StoreError};

fn row(key: &str, definition: &str, version: &str, status: &str) -> CacheRow {
    CacheRow {
        cache_key: key.into(),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: "https://openai.mock.test/openai/v1".into(),
        definition_id: definition.into(),
        definition_version: version.into(),
        input_hash: format!("input-{key}"),
        params_hash: "params".into(),
        result_json: r#"{"page_type":"product"}"#.into(),
        status: status.into(),
        created_at: "2026-11-24T10:00:00.000Z".into(),
    }
}

fn crawl(dir: &Path, name: &str) -> CrawlFile {
    let path = store_common::new_crawl(dir, name, StorageProfile::default());
    CrawlFile::open(&path, OpenMode::ReadWrite).unwrap()
}

fn companions(path: &Path) -> Vec<PathBuf> {
    ["-wal", "-shm", "-journal"]
        .iter()
        .map(|s| {
            let mut name = path.as_os_str().to_owned();
            name.push(s);
            PathBuf::from(name)
        })
        .filter(|p| p.exists())
        .collect()
}

// spec: AI-015
#[test]
fn rows_roundtrip_and_errors_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = crawl(dir.path(), "a.ospider");
    let conn = file.conn();
    let success = row("k1", "page-type", "1.0.0", cache::STATUS_SUCCESS);
    cache::put(conn, &success).unwrap();
    assert_eq!(cache::get(conn, "k1").unwrap(), Some(success.clone()));
    assert!(!success.is_refusal());
    assert_eq!(cache::get(conn, "missing").unwrap(), None);
    for status in [
        "Error: 500 Temporary error",
        "Error: Connection failed",
        "Success (cached)",
        "Pending",
        "",
    ] {
        let err = cache::put(conn, &row("k2", "page-type", "1.0.0", status)).unwrap_err();
        assert!(
            matches!(err, StoreError::InvalidInput(_)),
            "{status}: {err}"
        );
    }
    assert_eq!(cache::get(conn, "k2").unwrap(), None, "Fehler nie gecacht");
    let refusal = CacheRow {
        result_json: "null".into(),
        ..row("k3", "page-type", "1.0.0", cache::STATUS_REFUSAL)
    };
    cache::put(conn, &refusal).unwrap();
    assert!(cache::get(conn, "k3").unwrap().unwrap().is_refusal());
    // Ein erneut angefragtes Refusal ersetzt den Eintrag.
    cache::put(
        conn,
        &row("k3", "page-type", "1.0.0", cache::STATUS_SUCCESS),
    )
    .unwrap();
    assert!(!cache::get(conn, "k3").unwrap().unwrap().is_refusal());
    assert!(cache::remove(conn, "k3").unwrap());
    assert!(!cache::remove(conn, "k3").unwrap());
}

// spec: AI-015
#[test]
fn stats_and_clearing_per_question() {
    let dir = tempfile::tempdir().unwrap();
    let file = crawl(dir.path(), "a.ospider");
    let conn = file.conn();
    assert_eq!(
        cache::stats(conn).unwrap(),
        cache::CacheTableStats::default()
    );
    let rows: Vec<CacheRow> = [
        ("page-type", "1.0.0", cache::STATUS_SUCCESS),
        ("page-type", "1.0.0", cache::STATUS_REFUSAL),
        ("page-type", "1.1.0", cache::STATUS_SUCCESS),
        ("search-intent", "1.0.0", cache::STATUS_SUCCESS),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (definition, version, status))| row(&format!("k{i}"), definition, version, status))
    .collect();
    for r in &rows {
        cache::put(conn, r).unwrap();
    }
    let stats = cache::stats(conn).unwrap();
    assert_eq!((stats.entries, stats.refusals), (4, 1));
    let bytes: u64 = rows
        .iter()
        .map(|r| {
            [
                &r.cache_key,
                &r.provider,
                &r.model,
                &r.endpoint,
                &r.definition_id,
                &r.definition_version,
                &r.input_hash,
                &r.params_hash,
                &r.result_json,
                &r.status,
                &r.created_at,
            ]
            .iter()
            .map(|s| s.len() as u64)
            .sum::<u64>()
        })
        .sum();
    assert_eq!(stats.size_bytes, bytes);
    assert_eq!(
        cache::clear_definition(conn, "page-type", Some("1.1.0")).unwrap(),
        1
    );
    assert_eq!(cache::clear_definition(conn, "page-type", None).unwrap(), 2);
    assert_eq!(cache::stats(conn).unwrap().entries, 1);
    assert_eq!(cache::clear_all(conn).unwrap(), 1);
    assert_eq!(cache::stats(conn).unwrap().entries, 0);
}

// spec: AI-015
// Übernahme: fehlende Schlüssel werden übernommen, vorhandene bleiben; die Quelle bleibt byte-identisch und es
// entstehen keine neuen Begleitdateien neben ihr.
#[test]
fn import_copies_missing_entries_and_leaves_the_source_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = {
        let source = crawl(dir.path(), "old.ospider");
        for i in 0..5 {
            cache::put(
                source.conn(),
                &row(
                    &format!("k{i}"),
                    "page-type",
                    "1.0.0",
                    cache::STATUS_SUCCESS,
                ),
            )
            .unwrap();
        }
        cache::put(
            source.conn(),
            &row("r", "page-type", "1.0.0", cache::STATUS_REFUSAL),
        )
        .unwrap();
        source.path().to_path_buf()
    };
    let companions_before = companions(&source_path);
    let before = std::fs::read(&source_path).unwrap();
    let target = crawl(dir.path(), "new.ospider");
    let own = CacheRow {
        result_json: r#"{"page_type":"article"}"#.into(),
        ..row("k0", "page-type", "1.0.0", cache::STATUS_SUCCESS)
    };
    cache::put(target.conn(), &own).unwrap();
    let report = cache::import_from(target.conn(), &source_path).unwrap();
    assert_eq!(
        (report.read, report.imported, report.kept_existing),
        (6, 5, 1)
    );
    assert_eq!(
        cache::get(target.conn(), "k0").unwrap(),
        Some(own),
        "vorhandener Eintrag bleibt"
    );
    assert!(cache::get(target.conn(), "r")
        .unwrap()
        .unwrap()
        .is_refusal());
    assert_eq!(
        std::fs::read(&source_path).unwrap(),
        before,
        "byte-identisch"
    );
    assert_eq!(
        companions(&source_path),
        companions_before,
        "keine neuen Begleitdateien"
    );
    // Erneute Übernahme ist idempotent.
    let again = cache::import_from(target.conn(), &source_path).unwrap();
    assert_eq!((again.imported, again.kept_existing), (0, 6));
    assert_eq!(std::fs::read(&source_path).unwrap(), before);
}

// spec: AI-015
// Quelle, die gerade ein anderer Schreiber offen hat (WAL): gelesen wird unter Sperren, die Hauptdatei bleibt
// unverändert, und auch noch nicht zurückgeführte Einträge werden übernommen.
#[test]
fn import_from_a_file_with_an_active_writer_reads_without_modifying_it() {
    let dir = tempfile::tempdir().unwrap();
    let source = crawl(dir.path(), "live.ospider");
    source
        .conn()
        .query_row("PRAGMA wal_autocheckpoint = 0", [], |_| Ok(()))
        .unwrap();
    cache::put(
        source.conn(),
        &row("w1", "page-type", "1.0.0", cache::STATUS_SUCCESS),
    )
    .unwrap();
    let main_before = std::fs::read(source.path()).unwrap();
    let target = crawl(dir.path(), "new.ospider");
    let report = cache::import_from(target.conn(), source.path()).unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(std::fs::read(source.path()).unwrap(), main_before);
    assert_eq!(
        cache::get(source.conn(), "w1").unwrap().unwrap().cache_key,
        "w1"
    );
}

// spec: AI-015
// Ältere Crawl-Datei ohne `ai_cache` (Schema v5): nichts zu übernehmen, Datei unverändert; fremde und fehlende
// Dateien werden abgewiesen.
#[test]
fn import_from_older_foreign_or_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in [
        "..",
        "..",
        "tests",
        "fixtures",
        "migrations",
        "schema-v5",
        "crawl.ospider",
    ] {
        fixture.push(part);
    }
    let v5 = dir.path().join("v5.ospider");
    std::fs::copy(&fixture, &v5).unwrap();
    let before = std::fs::read(&v5).unwrap();
    let target = crawl(dir.path(), "new.ospider");
    let report = cache::import_from(target.conn(), &v5).unwrap();
    assert_eq!((report.read, report.imported), (0, 0));
    assert_eq!(std::fs::read(&v5).unwrap(), before);
    let foreign = dir.path().join("notes.txt");
    std::fs::write(&foreign, "keine Crawl-Datei, aber lang genug für einen SQLite-Kopf ....................................................................").unwrap();
    assert!(matches!(
        cache::import_from(target.conn(), &foreign),
        Err(StoreError::NotACrawlFile { .. })
    ));
    assert!(matches!(
        cache::import_from(target.conn(), &dir.path().join("missing.ospider")),
        Err(StoreError::NotFound(_))
    ));
}

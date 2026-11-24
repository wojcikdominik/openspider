//! Unabhängige Abnahmetests M4-K06: Tabelle `ai_cache` in der Crawl-Datei (AI-015, „Datenhaltung“).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md`: Erfolge und Refusals (mit Kennzeichen) werden gespeichert,
//! Fehler nie; Ergebnisse überstehen Schließen und Neuöffnen; die Übernahme aus einer anderen Crawl-Datei liest die
//! Quelle nur, die Quelle bleibt byte-identisch.
// acceptance: independent

use std::path::{Path, PathBuf};

use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::ai::cache::{self, CacheRow, STATUS_REFUSAL, STATUS_SUCCESS};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};
use sha2::{Digest, Sha256};

fn now() -> Timestamp {
    Timestamp::from_db("2026-11-24T12:00:00.000Z").unwrap()
}

fn create(path: &Path) -> CrawlFile {
    let init = CrawlInit {
        crawl_id: SeededIds::new(7).new_crawl_id(now()),
        mode: CrawlMode::Spider,
        start_url: Some("http://ai.test:8080/".into()),
        list_source: None,
        project: "K06-Abnahme".into(),
        name: "store".into(),
        config: CrawlConfig::default(),
        created_at: now(),
    };
    CrawlFile::create(path, &init, StorageProfile::default()).unwrap()
}

fn row(key: &str, status: &str) -> CacheRow {
    CacheRow {
        cache_key: key.into(),
        provider: "anthropic".into(),
        model: "claude-x".into(),
        endpoint: "https://anthropic.mock.test/anthropic/v1".into(),
        definition_id: "page-type".into(),
        definition_version: "1.0.0".into(),
        input_hash: format!("in-{key}"),
        params_hash: "p".into(),
        result_json: r#"{"page_type":"article"}"#.into(),
        status: status.into(),
        created_at: now().to_db(),
    }
}

fn file_hash(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap();
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn companions(path: &Path) -> Vec<(PathBuf, String)> {
    ["-wal", "-shm", "-journal"]
        .iter()
        .filter_map(|s| {
            let mut name = path.as_os_str().to_owned();
            name.push(s);
            let p = PathBuf::from(name);
            p.exists().then(|| {
                let h = file_hash(&p);
                (p, h)
            })
        })
        .collect()
}

// spec: AI-015
// spec: AI-020
// acceptance: independent
#[test]
fn only_success_and_refusal_rows_are_stored() {
    let dir = tempfile::tempdir().unwrap();
    let file = create(&dir.path().join("c.ospider"));
    let conn = file.conn();
    cache::put(conn, &row("ok", STATUS_SUCCESS)).unwrap();
    cache::put(conn, &row("ref", STATUS_REFUSAL)).unwrap();
    for (i, status) in [
        "Error: 500 Internal Server Error",
        "Error: Connection failed",
        "Error: Timeout nach 30 s",
        "Error: Invalid output",
        "Quota exceeded",
        "Budget exceeded",
    ]
    .iter()
    .enumerate()
    {
        let key = format!("err{i}");
        assert!(
            cache::put(conn, &row(&key, status)).is_err(),
            "{status} darf nicht gespeichert werden"
        );
        assert_eq!(cache::get(conn, &key).unwrap(), None);
    }
    let stats = cache::stats(conn).unwrap();
    assert_eq!(stats.entries, 2);
    assert_eq!(stats.refusals, 1, "Refusal mit Kennzeichen");
    assert!(stats.size_bytes > 0);
    assert!(cache::get(conn, "ref").unwrap().unwrap().is_refusal());
    assert!(!cache::get(conn, "ok").unwrap().unwrap().is_refusal());
}

// spec: AI-015
// acceptance: independent
#[test]
fn rows_survive_close_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.ospider");
    {
        let file = create(&path);
        cache::put(file.conn(), &row("k1", STATUS_SUCCESS)).unwrap();
        cache::put(file.conn(), &row("k2", STATUS_REFUSAL)).unwrap();
    }
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    assert_eq!(
        cache::get(file.conn(), "k1").unwrap(),
        Some(row("k1", STATUS_SUCCESS))
    );
    assert_eq!(
        cache::get(file.conn(), "k2").unwrap(),
        Some(row("k2", STATUS_REFUSAL))
    );
}

// spec: AI-015
// acceptance: independent
#[test]
fn clearing_one_question_leaves_other_questions() {
    let dir = tempfile::tempdir().unwrap();
    let file = create(&dir.path().join("c.ospider"));
    let conn = file.conn();
    cache::put(conn, &row("a", STATUS_SUCCESS)).unwrap();
    let mut other = row("b", STATUS_SUCCESS);
    other.definition_id = "search-intent".into();
    cache::put(conn, &other).unwrap();
    assert_eq!(cache::clear_definition(conn, "page-type", None).unwrap(), 1);
    assert_eq!(cache::get(conn, "a").unwrap(), None);
    assert_eq!(cache::get(conn, "b").unwrap(), Some(other));
}

// spec: AI-015
// acceptance: independent
#[test]
fn import_reads_source_only_and_source_stays_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("vorlauf.ospider");
    {
        let file = create(&source);
        for i in 0..22 {
            cache::put(file.conn(), &row(&format!("k{i:02}"), STATUS_SUCCESS)).unwrap();
        }
        cache::put(file.conn(), &row("refusal", STATUS_REFUSAL)).unwrap();
    }
    let before = file_hash(&source);
    let before_companions = companions(&source);
    let before_mtime = std::fs::metadata(&source).unwrap().modified().unwrap();

    let target = create(&dir.path().join("recrawl.ospider"));
    // Ein Eintrag existiert in der Zieldatei bereits und bleibt erhalten.
    let mut existing = row("k00", STATUS_SUCCESS);
    existing.result_json = r#"{"page_type":"product"}"#.into();
    cache::put(target.conn(), &existing).unwrap();
    let report = cache::import_from(target.conn(), &source).unwrap();
    assert_eq!(report.read, 23);
    assert_eq!(report.imported, 22);
    assert_eq!(report.kept_existing, 1);
    assert_eq!(cache::get(target.conn(), "k00").unwrap(), Some(existing));
    assert_eq!(
        cache::get(target.conn(), "k21").unwrap(),
        Some(row("k21", STATUS_SUCCESS))
    );
    assert!(cache::get(target.conn(), "refusal")
        .unwrap()
        .unwrap()
        .is_refusal());
    drop(target);

    assert_eq!(file_hash(&source), before, "Quelle bleibt byte-identisch");
    assert_eq!(
        companions(&source),
        before_companions,
        "keine Begleitdateien angelegt/geändert"
    );
    assert_eq!(
        std::fs::metadata(&source).unwrap().modified().unwrap(),
        before_mtime
    );
}

//! Unabhängige Abnahmetests M4-F01: Datenhaltung `ai_*` in der Crawl-Datei (Schemaversion 6).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` („Datenhaltung in der Crawl-Datei“, AI-013, AI-014,
//! AI-031) und `docs/plan/M4.md` (Migration 0006, Kompatibilität mit Backup). Fixture: Archivdatei
//! `tests/fixtures/migrations/schema-v5/crawl.ospider` (Stand vor M4).
// acceptance: independent

use std::fs;
use std::path::{Path, PathBuf};

use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::{backup_path, CrawlFile, CrawlInit, OpenMode, StorageProfile, SCHEMA_VERSION};
use rusqlite::Connection;

/// Neue Tabellen laut 08 „Datenhaltung in der Crawl-Datei“ (einschließlich Nachtrag 21.11.2026).
const AI_TABLES: [&str; 13] = [
    "ai_runs",
    "api_budget",
    "ai_requests",
    "ai_cache",
    "ai_questions",
    "ai_answers",
    "ai_suggestions",
    "ai_redirect_map",
    "ai_link_suggestions",
    "ai_batches",
    "ai_queries",
    "ai_prompt_results",
    "ai_embeddings",
];

/// Erkennt eine Spalte am (kleingeschriebenen) Namen.
type ColumnMatch = Box<dyn Fn(&str) -> bool>;

fn v5_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/migrations/schema-v5/crawl.ospider")
}

fn copy_v5(dir: &Path) -> PathBuf {
    let path = dir.join("crawl.ospider");
    fs::copy(v5_fixture(), &path).unwrap();
    path
}

fn tables(conn: &Connection) -> Vec<String> {
    let mut st = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    st.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn columns(conn: &Connection, table: &str) -> Vec<(String, String)> {
    let mut st = conn
        .prepare(&format!(
            "SELECT name, type FROM pragma_table_info('{table}')"
        ))
        .unwrap();
    st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Alle Zeilen einer Tabelle, sortiert (auch für WITHOUT-ROWID-Tabellen unabhängig von der Speicherreihenfolge).
fn dump(conn: &Connection, table: &str) -> Vec<String> {
    let mut st = conn.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
    let n = st.column_count();
    let mut rows: Vec<String> = st
        .query_map([], |r| {
            let mut cells = Vec::with_capacity(n);
            for i in 0..n {
                cells.push(format!("{:?}", r.get::<_, rusqlite::types::Value>(i)?));
            }
            Ok(cells.join("\u{1f}"))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.sort();
    rows
}

/// Schema als vergleichbare Menge (Typ, Name, Tabelle, normalisiertes SQL).
fn schema_objects(conn: &Connection) -> Vec<(String, String, String, String)> {
    let mut st = conn
        .prepare(
            "SELECT type, name, tbl_name, coalesce(sql, '') FROM sqlite_master \
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .unwrap();
    st.query_map([], |r| {
        let sql: String = r.get(3)?;
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            sql.split_whitespace().collect::<Vec<_>>().join(" "),
        ))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn user_version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn fresh(dir: &Path) -> PathBuf {
    let created = Timestamp::from_db("2026-11-21T08:00:00.000Z").unwrap();
    let path = dir.join("fresh.ospider");
    let init = CrawlInit {
        crawl_id: SeededIds::new(7).new_crawl_id(created),
        mode: CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "Abnahme".into(),
        name: "F01".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    drop(CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap());
    path
}

// spec: AI-013
// acceptance: independent
// Schemaversion 6 mit allen KI-Tabellen in einer neuen Crawl-Datei.
#[test]
fn fresh_file_has_schema_6_and_all_ai_tables() {
    assert_eq!(SCHEMA_VERSION, 6);
    let dir = tempfile::tempdir().unwrap();
    let path = fresh(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    assert_eq!(user_version(f.conn()), 6);
    let names = tables(f.conn());
    for t in AI_TABLES {
        assert!(names.iter().any(|n| n == t), "Tabelle {t} fehlt");
    }
}

// spec: AI-013
// acceptance: independent
// Migration v5 → v6 an der Archivdatei: Sicherungskopie byte-gleich, alle bisherigen Tabellen und Zeilen
// unverändert, neue Tabellen vorhanden und leer, Schemaversion 6.
#[test]
fn v5_archive_migrates_to_v6_without_losing_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_v5(dir.path());
    let original_bytes = fs::read(&path).unwrap();

    let (old_tables, old_rows) = {
        let c = Connection::open(&path).unwrap();
        assert_eq!(user_version(&c), 5, "Fixture muss Schema v5 sein");
        let t = tables(&c);
        assert!(!t.iter().any(|n| n.starts_with("ai_") || n == "api_budget"));
        let rows: Vec<Vec<String>> = t.iter().map(|n| dump(&c, n)).collect();
        (t, rows)
    };
    assert!(
        old_rows.iter().any(|r| !r.is_empty()),
        "Fixture enthält Daten"
    );

    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    assert_eq!(user_version(f.conn()), 6);
    assert_eq!(f.get_meta("schema_version").unwrap().as_deref(), Some("6"));
    assert_eq!(
        f.get_meta("crawl_id").unwrap().as_deref(),
        Some("0199aaaa-0000-7000-8000-000000000001")
    );

    let new_tables = tables(f.conn());
    for (t, before) in old_tables.iter().zip(&old_rows) {
        assert!(
            new_tables.contains(t),
            "Tabelle {t} nach Migration verschwunden"
        );
        if t == "crawl_meta" {
            // Bestehende Einträge bleiben erhalten (außer schema_version und Schreiber-Stempeln);
            // ergänzte Schlüssel sind kein Datenverlust.
            let after = dump(f.conn(), t);
            let lost: Vec<&String> = before
                .iter()
                .filter(|r| {
                    !r.contains("\"schema_version\"")
                        && !r.contains("writer")
                        && !r.contains("openspider_version")
                })
                .filter(|r| !after.contains(r))
                .collect();
            assert!(lost.is_empty(), "crawl_meta verloren/verändert: {lost:?}");
            continue;
        }
        assert_eq!(&dump(f.conn(), t), before, "Daten in {t} verändert");
    }
    for t in AI_TABLES {
        assert!(
            new_tables.iter().any(|n| n == t),
            "Tabelle {t} fehlt nach Migration"
        );
        assert!(
            dump(f.conn(), t).is_empty(),
            "{t} muss nach Migration leer sein"
        );
    }
    drop(f);

    let backup = backup_path(&path, 5);
    assert!(backup.exists(), "Sicherungskopie der v5-Datei fehlt");
    assert_eq!(fs::read(backup).unwrap(), original_bytes);

    // Zweites Öffnen migriert nicht erneut.
    let again = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    assert_eq!(user_version(again.conn()), 6);
}

// spec: AI-013
// acceptance: independent
// Frische DB = migrierte DB: gleiche Tabellen, Spalten, Indizes, Trigger und Sichten.
#[test]
fn fresh_and_migrated_schema_are_identical() {
    let dir = tempfile::tempdir().unwrap();
    let fresh_path = fresh(dir.path());
    let migrated_path = copy_v5(dir.path());
    drop(CrawlFile::open(&migrated_path, OpenMode::ReadWrite).unwrap());

    let a = CrawlFile::open(&fresh_path, OpenMode::ReadWrite).unwrap();
    let b = CrawlFile::open(&migrated_path, OpenMode::ReadWrite).unwrap();
    let sa = schema_objects(a.conn());
    let sb = schema_objects(b.conn());
    let only_fresh: Vec<_> = sa.iter().filter(|o| !sb.contains(o)).collect();
    let only_migrated: Vec<_> = sb.iter().filter(|o| !sa.contains(o)).collect();
    assert!(
        only_fresh.is_empty() && only_migrated.is_empty(),
        "Schema weicht ab.\nnur frisch: {only_fresh:#?}\nnur migriert: {only_migrated:#?}"
    );
    for t in AI_TABLES {
        assert_eq!(columns(a.conn(), t), columns(b.conn(), t), "{t}");
    }
}

// spec: AI-013
// acceptance: independent
// „API-Schlüssel werden nie in der Crawl-Datei gespeichert.“ Keine KI-Tabelle hat eine Schlüssel-/Geheimnisspalte.
#[test]
fn no_ai_table_stores_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let f = CrawlFile::open(&fresh(dir.path()), OpenMode::ReadOnly).unwrap();
    for t in AI_TABLES {
        for (col, _) in columns(f.conn(), t) {
            let c = col.to_lowercase();
            for bad in [
                "api_key",
                "apikey",
                "secret",
                "password",
                "credential",
                "authorization",
                "bearer",
            ] {
                assert!(!c.contains(bad), "{t}.{col} sieht nach Zugangsdaten aus");
            }
            assert_ne!(c, "key", "{t}.{col}");
        }
    }
}

// spec: AI-013
// acceptance: independent
// `api_budget` enthält nur Grenzwerte, keinen Verbrauch; Beträge als Ganzzahl (Mikro-USD).
#[test]
fn api_budget_holds_limits_only_as_integers() {
    let dir = tempfile::tempdir().unwrap();
    let f = CrawlFile::open(&fresh(dir.path()), OpenMode::ReadOnly).unwrap();
    let cols = columns(f.conn(), "api_budget");
    assert!(!cols.is_empty());
    for (col, ty) in &cols {
        let c = col.to_lowercase();
        for bad in ["spent", "consum", "used", "usage", "actual", "reserved"] {
            assert!(!c.contains(bad), "api_budget.{col} ist ein Verbrauchswert");
        }
        if c.contains("usd") || c.contains("limit") {
            assert_eq!(
                ty.to_uppercase(),
                "INTEGER",
                "api_budget.{col} muss Ganzzahl sein"
            );
        }
    }
}

// spec: AI-014
// acceptance: independent
// `ai_requests` ist das Protokoll je Anbieteranfrage: je Spalte aus AI-014 (plus `over_reservation` aus AI-013
// und `confidence_mismatch` aus AI-004) gibt es eine Spalte; Geldbeträge sind Ganzzahlen (µUSD).
#[test]
fn ai_requests_covers_the_ai_014_log_columns() {
    let dir = tempfile::tempdir().unwrap();
    let f = CrawlFile::open(&fresh(dir.path()), OpenMode::ReadOnly).unwrap();
    let cols = columns(f.conn(), "ai_requests");
    let names: Vec<String> = cols.iter().map(|(c, _)| c.to_lowercase()).collect();
    let has = |pred: &dyn Fn(&str) -> bool| names.iter().any(|n| pred(n));
    let expected: Vec<(&str, ColumnMatch)> = vec![
        (
            "Timestamp",
            Box::new(|n| n.contains("timestamp") || n == "at"),
        ),
        ("Run", Box::new(|n| n.starts_with("run"))),
        ("Task", Box::new(|n| n == "task")),
        ("Provider", Box::new(|n| n == "provider")),
        ("Model", Box::new(|n| n == "model")),
        ("Endpoint", Box::new(|n| n == "endpoint")),
        (
            "Address",
            Box::new(|n| n.contains("url") || n.contains("address")),
        ),
        (
            "Question/Prompt Version",
            Box::new(|n| n.contains("version")),
        ),
        ("Input Tokens", Box::new(|n| n == "input_tokens")),
        (
            "Cached Input Tokens",
            Box::new(|n| n.contains("cached_input")),
        ),
        (
            "Cache Write Tokens",
            Box::new(|n| n.contains("cache_write")),
        ),
        ("Output Tokens", Box::new(|n| n == "output_tokens")),
        ("Images", Box::new(|n| n.contains("image"))),
        ("Token Cost (USD)", Box::new(|n| n.contains("token_cost"))),
        ("Fixed Fees (USD)", Box::new(|n| n.contains("fee"))),
        ("Regional Uplift (USD)", Box::new(|n| n.contains("uplift"))),
        ("Cost (USD)", Box::new(|n| n.starts_with("cost"))),
        ("Price Source", Box::new(|n| n.contains("price_source"))),
        ("Batch", Box::new(|n| n.contains("batch"))),
        ("Cache Hit", Box::new(|n| n.contains("cache_hit"))),
        ("Status", Box::new(|n| n == "status")),
        ("HTTP Status", Box::new(|n| n.contains("http"))),
        ("Error Code", Box::new(|n| n.contains("error_code"))),
        ("Duration ms", Box::new(|n| n.contains("duration"))),
        (
            "Provider Request ID",
            Box::new(|n| n.contains("provider_request")),
        ),
        ("Retries", Box::new(|n| n.contains("retr"))),
        ("Refusal Reason", Box::new(|n| n.contains("refusal"))),
        (
            "over_reservation",
            Box::new(|n| n.contains("over_reservation")),
        ),
        (
            "confidence_mismatch",
            Box::new(|n| n.contains("confidence_mismatch")),
        ),
    ];
    let missing: Vec<&str> = expected
        .iter()
        .filter(|(_, p)| !has(p.as_ref()))
        .map(|(label, _)| *label)
        .collect();
    assert!(
        missing.is_empty(),
        "ai_requests ohne Spalte für: {missing:?}; Spalten: {names:?}"
    );
    for (col, ty) in &cols {
        if col.to_lowercase().contains("usd") {
            assert_eq!(
                ty.to_uppercase(),
                "INTEGER",
                "ai_requests.{col} muss µUSD-Ganzzahl sein"
            );
        }
    }
}

// spec: AI-014
// acceptance: independent
// `Price Source` ∈ {`api`, `table`, `user`, `estimate`}; andere Werte lehnt die Datei ab.
#[test]
fn price_source_accepts_exactly_the_five_spec_values() {
    let dir = tempfile::tempdir().unwrap();
    let f = CrawlFile::open(&fresh(dir.path()), OpenMode::ReadWrite).unwrap();
    let c = f.conn();
    c.execute(
        "INSERT INTO ai_runs (id, started_at, config_json) VALUES (1, '2026-11-21T08:00:00Z', '{}')",
        [],
    )
    .unwrap();
    let insert = |source: &str| {
        c.execute(
            "INSERT INTO ai_requests (run_id, timestamp, task, provider, model, endpoint, \
             token_cost_microusd, cost_microusd, price_source, status, duration_ms) \
             VALUES (1, '2026-11-21T08:00:01Z', 'decision', 'anthropic', 'mock-claude', 'messages', \
             2000, 2000, ?1, 'Success', 12)",
            [source],
        )
    };
    for ok in ["api", "table", "user", "estimate"] {
        insert(ok).unwrap_or_else(|e| panic!("{ok}: {e}"));
    }
    for bad in ["API", "guess", ""] {
        assert!(
            insert(bad).is_err(),
            "{bad:?} darf nicht gespeichert werden"
        );
    }
}

// spec: AI-031
// acceptance: independent
// `ai_questions`: unveränderliche Kopie je Version – eine gespeicherte Version lässt sich nicht ändern, eine neue
// Version derselben Frage wird zusätzlich gespeichert.
#[test]
fn stored_question_versions_are_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let f = CrawlFile::open(&fresh(dir.path()), OpenMode::ReadWrite).unwrap();
    let c = f.conn();
    c.execute(
        "INSERT INTO ai_questions (id, version, definition_json) VALUES ('page-type', '1.0.0', '{\"type\":\"choice\"}')",
        [],
    )
    .unwrap();
    assert!(c
        .execute(
            "UPDATE ai_questions SET definition_json = '{\"type\":\"predicate\"}' WHERE id = 'page-type'",
            [],
        )
        .is_err());
    c.execute(
        "INSERT INTO ai_questions (id, version, definition_json) VALUES ('page-type', '1.1.0', '{\"type\":\"choice\"}')",
        [],
    )
    .unwrap();
    let def: String = c
        .query_row(
            "SELECT definition_json FROM ai_questions WHERE id = 'page-type' AND version = '1.0.0'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(def, "{\"type\":\"choice\"}");
    let n: i64 = c
        .query_row("SELECT count(*) FROM ai_questions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2);
}

// spec: AI-013
// acceptance: independent
// Die neuen Tabellen sind in der migrierten Archivdatei mit deren bestehenden URLs nutzbar: Ergebnisse je
// (URL, Prompt, Version) und Vektoren je (URL, Embedding-Satz) mit Modell und Dimension (float32, INTEG-180).
#[test]
fn migrated_file_accepts_prompt_results_and_embeddings_for_existing_urls() {
    let dir = tempfile::tempdir().unwrap();
    let path = copy_v5(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let c = f.conn();
    let url_id: i64 = c
        .query_row("SELECT min(id) FROM urls", [], |r| r.get(0))
        .unwrap();
    c.execute(
        "INSERT INTO ai_prompt_results (url_id, prompt_id, prompt_version, result_json, status) \
         VALUES (?1, 'summary', '3', '{\"text\":\"x\"}', 'Success')",
        [url_id],
    )
    .unwrap();
    // Gleicher Schlüssel (URL, Prompt, Version) ist eindeutig.
    assert!(c
        .execute(
            "INSERT INTO ai_prompt_results (url_id, prompt_id, prompt_version, result_json, status) \
             VALUES (?1, 'summary', '3', NULL, 'Refusal')",
            [url_id],
        )
        .is_err());
    let vector: Vec<u8> = [0.5f32, -0.25, 1.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    c.execute(
        "INSERT INTO ai_embeddings (url_id, prompt_id, prompt_version, provider, model, dimensions, input_hash, vector) \
         VALUES (?1, 'emb', '1', 'ollama', 'nomic-embed-text', 3, 'h', ?2)",
        rusqlite::params![url_id, vector],
    )
    .unwrap();
    let (dims, blob): (i64, Vec<u8>) = c
        .query_row(
            "SELECT dimensions, vector FROM ai_embeddings WHERE url_id = ?1",
            [url_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(dims, 3);
    assert_eq!(blob, vector);
}

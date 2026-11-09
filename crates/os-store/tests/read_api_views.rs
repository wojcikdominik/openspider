//! Leseschnittstelle der Crawl-Datei, Sichten (OUT-234): Pflichtspalten, lückenlose Selbstbeschreibung `q_columns`,
//! Zugriff mit Standard-SQLite ohne eigene Kollationen und Funktionen, Zähler aus `url_counters`, Golden für eine neu
//! geschriebene und eine aus Schema v2 migrierte Datei.

mod store_common;

use std::path::Path;

use os_model::*;
use os_store::read_api::{self, VIEWS};
use os_store::{CrawlFile, CrawlSink, OpenMode, StorageProfile};
use os_testsite::golden::{assert_golden, normalize};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use store_common::*;

/// Pflichtspalten der Sichten laut OUT-234 in der Reihenfolge der Spec.
const REQUIRED: &[(&str, &[&str])] = &[
    ("q_manifest", &["key", "value"]),
    ("q_config", &["osconfig", "updated_at"]),
    (
        "q_urls",
        &[
            "url_id",
            "address",
            "is_internal",
            "crawled",
            "content_type",
            "status_code",
            "status",
            "indexability",
            "indexability_status",
            "title_1",
            "meta_description_1",
            "h1_1",
            "h2_1",
            "canonical_link_element_1",
            "word_count",
            "size_bytes",
            "transferred_bytes",
            "response_time",
            "crawl_depth",
            "folder_depth",
            "inlinks",
            "unique_inlinks",
            "outlinks",
            "unique_outlinks",
            "external_outlinks",
            "redirect_url",
            "redirect_type",
            "http_version",
            "hash",
            "last_modified",
            "crawl_timestamp",
        ],
    ),
    (
        "q_links",
        &[
            "source_url_id",
            "target_url_id",
            "source_address",
            "target_address",
            "type",
            "anchor",
            "alt_text",
            "follow",
            "rel",
            "target",
            "path_type",
            "link_path",
            "link_position",
            "link_origin",
            "target_status_code",
        ],
    ),
    (
        "q_sitemap_urls",
        &["sitemap_url", "url_id", "address", "lastmod"],
    ),
    ("q_columns", &["view", "column", "type", "description"]),
];

/// Ein Leser ohne OpenSpider-Code: Standard-SQLite, Nur-Lesen, keine registrierte Kollation.
fn plain_reader(path: &Path) -> Connection {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}

fn columns_of(conn: &Connection, view: &str) -> Vec<String> {
    conn.prepare(&format!("PRAGMA table_info({view})"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn paused_crawl_file(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("neu.ospider");
    build_paused_crawl(&path);
    path
}

fn migrated_v2_file(dir: &Path) -> std::path::PathBuf {
    let path = copy_fixture("schema-v2.ospider", dir);
    drop(CrawlFile::open(&path, OpenMode::ReadWrite).unwrap());
    path
}

// spec: OUT-234
#[test]
fn every_required_column_exists_in_spec_order_and_is_described() {
    let dir = tempfile::tempdir().unwrap();
    let path = paused_crawl_file(dir.path());
    let conn = plain_reader(&path);
    assert_eq!(
        REQUIRED.iter().map(|(v, _)| *v).collect::<Vec<_>>(),
        VIEWS,
        "Liste der Sichten"
    );
    for (view, required) in REQUIRED {
        let actual = columns_of(&conn, view);
        assert!(
            actual.len() >= required.len() && actual[..required.len()] == **required,
            "{view}: Pflichtspalten (Reihenfolge der Spec) {required:?}, vorhanden {actual:?}"
        );
    }
    let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let gaps = read_api::describe_gaps(file.conn()).unwrap();
    assert!(gaps.is_empty(), "q_columns hat Lücken: {gaps:?}");
    let docs = read_api::columns(file.conn()).unwrap();
    let total: usize = REQUIRED.iter().map(|(_, c)| c.len()).sum();
    assert_eq!(docs.len(), total, "genau eine Beschreibung je Spalte");
    for d in &docs {
        assert!(!d.description.trim().is_empty(), "{}.{}", d.view, d.column);
        assert!(matches!(d.sql_type.as_str(), "INTEGER" | "REAL" | "TEXT"));
    }
}

// spec: OUT-234
#[test]
fn a_plain_sqlite_reader_filters_and_sorts_every_column_of_every_view() {
    let dir = tempfile::tempdir().unwrap();
    let path = paused_crawl_file(dir.path());
    let conn = plain_reader(&path);
    for (view, cols) in REQUIRED {
        for col in *cols {
            let col = format!("\"{col}\"");
            conn.prepare(&format!(
                "SELECT {col} FROM {view} WHERE {col} IS NOT NULL ORDER BY {col}"
            ))
            .unwrap_or_else(|e| panic!("{view}.{col}: {e}"))
            .query_map([], |_| Ok(()))
            .unwrap_or_else(|e| panic!("{view}.{col}: {e}"))
            .for_each(|r| r.unwrap());
            conn.prepare(&format!("SELECT COUNT(*) FROM {view} WHERE {col} = {col}"))
                .unwrap()
                .query_row([], |_| Ok(()))
                .unwrap();
            conn.prepare(&format!(
                "SELECT {col} FROM {view} ORDER BY {col} DESC LIMIT 3"
            ))
            .unwrap()
            .query_map([], |_| Ok(()))
            .unwrap()
            .for_each(|r| r.unwrap());
        }
    }
    // Wirklich ohne Kollation: die Sichten verweisen auf keine.
    let sql: String = conn
        .query_row(
            "SELECT group_concat(sql, ' ') FROM sqlite_master WHERE type = 'view'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !sql.to_ascii_lowercase().contains("os_text"),
        "Sichten dürfen OS_TEXT nicht verwenden"
    );
}

// spec: OUT-234
#[test]
fn q_urls_has_one_row_per_table_url_and_reads_the_counters_from_url_counters() {
    let dir = tempfile::tempdir().unwrap();
    let path = paused_crawl_file(dir.path());
    let conn = plain_reader(&path);
    let n = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(n("SELECT COUNT(*) FROM q_urls"), 42);
    assert_eq!(n("SELECT COUNT(*) FROM q_urls WHERE is_internal = 0"), 2);
    assert_eq!(n("SELECT COUNT(*) FROM q_urls WHERE crawled = 1"), 25);
    // Die Zählerspalten in `urls` sind seit Schema v2 stillgelegt und bleiben 0; die Sicht liest `url_counters`.
    assert_eq!(n("SELECT inlinks FROM urls WHERE id = 1"), 0);
    assert!(n("SELECT inlinks FROM q_urls WHERE url_id = 1") > 0);
    assert_eq!(
        n("SELECT inlinks FROM q_urls WHERE url_id = 1"),
        n("SELECT inlinks FROM url_counters WHERE url_id = 1")
    );
    assert_eq!(
        n("SELECT unique_outlinks FROM q_urls WHERE url_id = 3"),
        n("SELECT unique_outlinks FROM url_counters WHERE url_id = 3")
    );
    // Einheiten und Werte wie der Export: Antwortzeit in Sekunden, Version ohne Präfix, Aufzählungsbezeichnung.
    let mut b = WriteBatch::default();
    let mut p = page(30, vec![]);
    p.fetch.response_time_ms = Some(1_234);
    p.fetch.http_version = Some("HTTP/1.1".into());
    p.fetch.redirect_url = Some("https://site.test/p31".into());
    p.fetch.redirect_type = Some(RedirectType::Http);
    p.fetch.status_code = 301;
    p.fetch.last_modified = Some(Timestamp::from_db("2026-10-01T08:30:00.000Z").unwrap());
    b.pages.push(p);
    drop(conn);
    let mut sink = sink(&path, StorageProfile::Balanced);
    sink.write(b).unwrap();
    let conn = plain_reader(&path);
    let row: (f64, String, String, String, String, String) = conn
        .query_row(
            "SELECT response_time, http_version, redirect_type, redirect_url, last_modified, crawl_timestamp \
             FROM q_urls WHERE url_id = 30",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(row.0, 1.234);
    assert_eq!(row.1, "1.1");
    assert_eq!(row.2, "HTTP Redirect");
    assert_eq!(row.3, "https://site.test/p31");
    assert_eq!(row.4, "2026-10-01T08:30:00+00:00");
    assert_eq!(row.5, "2026-10-13T07:35:00+00:00");
}

// spec: OUT-234
#[test]
fn q_links_mirrors_links_with_resolved_targets_and_target_status() {
    let dir = tempfile::tempdir().unwrap();
    let path = new_crawl(dir.path(), "l.ospider", StorageProfile::Balanced);
    let mut sink = sink(&path, StorageProfile::Balanced);
    let mut b = batch_with_urls([(1, true), (2, true), (3, false)]);
    let mut p1 = page(
        1,
        vec![
            hyperlink(2, 0),
            hyperlink(3, 1),
            link(None, LinkType::Hyperlink, 2),
        ],
    );
    p1.links[0].follow = false;
    p1.links[0].rel = Some("nofollow".into());
    p1.links[0].target_attr = Some("_blank".into());
    p1.links[0].link_position = Some(LinkPosition::Footer);
    b.pages.push(p1);
    let mut p2 = page(2, vec![]);
    p2.fetch.status_code = 404;
    b.pages.push(p2);
    sink.write(b).unwrap();
    drop(sink);
    let conn = plain_reader(&path);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM q_links", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    let row = |target: &str| -> (
        Option<i64>,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<i64>,
    ) {
        conn.query_row(
            "SELECT target_url_id, source_address, follow, target, link_position, target_status_code \
             FROM q_links WHERE target_address = ?1",
            [target],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap()
    };
    let to_p2 = row("https://site.test/p2");
    assert_eq!(to_p2.0, Some(2));
    assert_eq!(to_p2.1, "https://site.test/p1");
    assert_eq!(to_p2.2, 0, "follow ist 0/1");
    assert_eq!(to_p2.3.as_deref(), Some("_blank"));
    assert_eq!(to_p2.4.as_deref(), Some("Footer"));
    assert_eq!(to_p2.5, Some(404));
    // Ziel ohne Abruf: Statuscode NULL; Ziel ohne URL der Datei: Rohwert und target_url_id NULL.
    let to_p3 = row("https://other.test/p3");
    assert_eq!((to_p3.0, to_p3.5), (Some(3), None));
    let raw = row("mailto:x@y.test");
    assert_eq!((raw.0, raw.5), (None, None));
}

// spec: OUT-234
#[test]
fn q_sitemap_urls_has_one_row_per_sitemap_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = new_crawl(dir.path(), "s.ospider", StorageProfile::Balanced);
    let mut sink = sink(&path, StorageProfile::Balanced);
    let mut b = batch_with_urls([(1, true), (2, true)]);
    b.sitemaps.push(SitemapWrite {
        id: SitemapId(1),
        url: "https://site.test/sitemap.xml".into(),
        url_id: None,
        parent: None,
        kind: SitemapKind::Urlset,
        entries: 2,
        invalid_entries: 0,
        size_bytes: 10,
        status_code: Some(200),
        depth: 0,
        found_via: "robots".into(),
        entry_urls: vec![
            (UrlId(1), "/p1".into(), Some("2026-01-01".into())),
            (UrlId(2), "/p2".into(), None),
        ],
        hreflang: vec![],
        error: None,
        entry_meta: vec![],
    });
    sink.write(b).unwrap();
    drop(sink);
    let conn = plain_reader(&path);
    let rows: Vec<(String, i64, String, Option<String>)> = conn
        .prepare("SELECT sitemap_url, url_id, address, lastmod FROM q_sitemap_urls ORDER BY url_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "https://site.test/sitemap.xml".to_string(),
                1,
                "https://site.test/p1".to_string(),
                Some("2026-01-01".to_string())
            ),
            (
                "https://site.test/sitemap.xml".to_string(),
                2,
                "https://site.test/p2".to_string(),
                None
            ),
        ]
    );
}

fn cell(v: ValueRef<'_>) -> String {
    match v {
        ValueRef::Null => "NULL".into(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => format!("{f:?}"),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Blob(b) => format!("<{} Byte>", b.len()),
    }
}

/// Ausgabe aller `q_`-Sichten als Text. Versionen im Manifest (`openspider_version`, `last_writer_version`) werden
/// durch `<VERSION>` ersetzt, der SHA-256 und der Konfigurationstext durch Platzhalter (sie ändern sich mit jeder
/// Änderung der Konfigurationsvorgaben); Zeiten und Kennungen normalisiert `normalize`.
fn dump_views(path: &Path) -> String {
    let conn = plain_reader(path);
    let queries = [
        ("q_manifest", "SELECT * FROM q_manifest ORDER BY key"),
        ("q_config", "SELECT * FROM q_config"),
        ("q_urls", "SELECT * FROM q_urls ORDER BY url_id"),
        (
            "q_links",
            "SELECT * FROM q_links ORDER BY source_url_id, target_address, anchor, link_path",
        ),
        (
            "q_sitemap_urls",
            "SELECT * FROM q_sitemap_urls ORDER BY sitemap_url, url_id",
        ),
        ("q_columns", "SELECT * FROM q_columns"),
    ];
    let mut out = String::new();
    for (view, sql) in queries {
        out.push_str(&format!("## {view}\n"));
        let mut stmt = conn.prepare(sql).unwrap();
        let names: Vec<String> = stmt.column_names().map(|s| s.to_string()).collect();
        out.push_str(&names.join("|"));
        out.push('\n');
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let mut cells: Vec<String> = (0..names.len())
                .map(|i| cell(row.get_ref(i).unwrap()))
                .collect();
            // Werte, die von der Programmversion bzw. den Konfigurationsvorgaben abhängen, bleiben außerhalb des Goldens.
            if view == "q_manifest" {
                match cells[0].as_str() {
                    "openspider_version" | "last_writer_version" => cells[1] = "<VERSION>".into(),
                    "config_sha256" => cells[1] = "<SHA256>".into(),
                    "user_agent" => {
                        cells[1] = cells[1].replace(os_store::meta::OPENSPIDER_VERSION, "<VERSION>")
                    }
                    _ => {}
                }
            }
            if view == "q_config" {
                cells[0] = "<OSCONFIG>".into();
            }
            out.push_str(&cells.join("|"));
            out.push('\n');
        }
    }
    normalize(&out, &[])
}

// spec: OUT-234
#[test]
fn the_golden_of_all_views_is_identical_for_a_new_file_and_a_file_migrated_from_schema_v2() {
    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden/out-m2-read-api/paused-crawl.q.txt");
    let dir = tempfile::tempdir().unwrap();
    let new_file = paused_crawl_file(dir.path());
    let migrated = migrated_v2_file(dir.path());

    let new_dump = dump_views(&new_file);
    let migrated_dump = dump_views(&migrated);
    assert_eq!(
        new_dump, migrated_dump,
        "Migrationen dürfen die Leseschnittstelle nicht brechen"
    );
    assert_golden(&golden, &new_dump);
    assert_golden(&golden, &migrated_dump);
}

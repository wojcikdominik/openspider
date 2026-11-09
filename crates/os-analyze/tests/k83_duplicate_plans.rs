//! K-83 (M1-N27): „Exact Duplicates“ als unkorrelierte Gruppierung statt korreliertem `EXISTS`, und die Duplicate-Filter
//! der Tabs brauchen keinen der fünf Indizes `urls_title1`, `urls_md1`, `urls_h1`, `urls_h2`, `urls_hash`.
//!
//! * Gleichheit der Treffermenge alt gegen neu (zwei 2xx-Seiten mit gleichem Hash, Nicht-2xx-HTML mit dem Hash einer
//!   2xx-Seite, Einzelseite, `only_indexable` an und aus, Seiten außerhalb der Vergleichsmenge).
//! * `EXPLAIN QUERY PLAN`: die fünf Duplicate-Filter ohne korrelierte Unterabfrage über `urls`, auf einer Datei mit
//!   und ohne die fünf Indizes.
//! * Kein Filter irgendeines Tabs benutzt einen der fünf Indizes (Inventur der Leser, `docs/ARCHITECTURE.md`).

use std::collections::BTreeSet;

use os_analyze::tabs;
use os_analyze::views::{plan, query_view, ViewQuery};
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp, Value};
use os_store::url_index::WRITE_PHASE_DROPPED;
use os_store::{CrawlFile, CrawlInit, StorageProfile};

/// Der Filter „Exact Duplicates“ vor K-83: korreliertes `EXISTS` auf dem Index `urls_hash` (Wortlaut aus
/// `tabs/content.rs` auf bc65311, die Vergleichsmenge mit denselben benannten Parametern).
const OLD_EXACT_DUPLICATES: &str =
    "(u.content_class = 'HTML' AND u.hash_md5 IS NOT NULL AND u.hash_md5 <> '' AND \
     (:dup_only_indexable = 0 OR COALESCE(u.indexability, '') <> 'Non-Indexable') AND \
     EXISTS (SELECT 1 FROM urls d WHERE d.hash_md5 = u.hash_md5 AND d.id <> u.id AND \
     +d.is_internal = 1 AND +d.in_tables = 1 AND +d.status_code BETWEEN 200 AND 299 AND \
     +d.content_class = 'HTML' AND \
     (:dup_only_indexable = 0 OR COALESCE(d.indexability, '') <> 'Non-Indexable')))";

fn created_at() -> Timestamp {
    Timestamp::from_db("2026-10-16T07:35:00.000Z").unwrap()
}

fn empty_file(dir: &std::path::Path) -> CrawlFile {
    let init = CrawlInit {
        crawl_id: SeededIds::new(7).new_crawl_id(created_at()),
        mode: CrawlMode::Spider,
        start_url: Some("https://site.test/".into()),
        list_source: None,
        project: "Projekt".into(),
        name: "K-83".into(),
        config: CrawlConfig::default(),
        created_at: created_at(),
    };
    CrawlFile::create(&dir.join("k83.ospider"), &init, StorageProfile::Balanced).unwrap()
}

/// Eine Zeile von `urls`. Der Titel ist gleich dem Hash, damit die Duplicate-Filter der Tabs dieselben Gruppen bilden.
#[allow(clippy::too_many_arguments)]
fn row(
    file: &CrawlFile,
    id: i64,
    class: &str,
    status: Option<i64>,
    internal: bool,
    in_tables: bool,
    indexability: &str,
    hash: Option<&str>,
) {
    file.conn()
        .execute(
            "INSERT INTO urls (id, url, address, origin, is_internal, in_tables, state, discovered_via, \
             folder_depth, url_length, status_code, content_class, hash_md5, \
             title_count, title_1, title_1_len, meta_desc_count, meta_desc_1, meta_desc_1_len, \
             h1_count, h1_1, h1_1_len, h2_count, h2_1, h2_1_len, indexability) \
             VALUES (?1, ?2, ?2, 'https://site.test', ?3, ?4, 'done', 'Link', 0, 10, ?5, ?6, ?7, \
             ?8, ?7, ?9, ?8, ?7, ?9, ?8, ?7, ?9, ?8, ?7, ?9, ?10)",
            rusqlite::params![
                id,
                format!("https://site.test/p{id}"),
                i64::from(internal),
                i64::from(in_tables),
                status,
                class,
                hash,
                i64::from(hash.is_some()),
                hash.map(|h| h.chars().count() as i64),
                indexability,
            ],
        )
        .unwrap();
}

/// Fixture mit allen Fällen aus dem Paketauftrag. Hash `hN` und die Zeile, die ihn trägt:
///
/// | Hash | Zeilen | Erwartung |
/// |---|---|---|
/// | h1 | 1, 2 (2xx, indexierbar), 3 (404 mit dem Hash von 1/2) | 1, 2 Treffer; 3 außerhalb der Grundmenge |
/// | h3 | 4 (2xx Einzelseite), 5 (404 mit dem Hash von 4) | 4 kein Treffer; 5 (außerhalb) altes `EXISTS`: Treffer |
/// | h5 | 6, 7 (beide 404) | kein Treffer (kein Gegenüber in der Vergleichsmenge) |
/// | h6 | 8, 9 (2xx, nicht indexierbar), 10 (2xx, indexierbar) | `only_indexable`: keiner; sonst 8, 9, 10 |
/// | h7 | 11 (PDF), 12 (HTML 2xx) | kein Treffer (PDFs sind nicht in der Vergleichsmenge) |
/// | h8 | 13 (extern), 14 (intern) | 14 kein Treffer; 13 (außerhalb): altes `EXISTS` Treffer |
/// | - | 15 (ohne Hash), 16, 17 (leerer Hash) | nie |
/// | h9 | 18 (`in_tables = 0`), 19 (2xx) | 19 kein Treffer; 18 (außerhalb): Treffer |
/// | h10 | 20 (kein Status), 21 (2xx) | 21 kein Treffer; 20 (außerhalb): Treffer |
fn fixture(dir: &std::path::Path) -> CrawlFile {
    let f = empty_file(dir);
    let (ok, no) = ("Indexable", "Non-Indexable");
    row(&f, 1, "HTML", Some(200), true, true, ok, Some("h1"));
    row(&f, 2, "HTML", Some(200), true, true, ok, Some("h1"));
    row(&f, 3, "HTML", Some(404), true, true, ok, Some("h1"));
    row(&f, 4, "HTML", Some(200), true, true, ok, Some("h3"));
    row(&f, 5, "HTML", Some(404), true, true, ok, Some("h3"));
    row(&f, 6, "HTML", Some(404), true, true, ok, Some("h5"));
    row(&f, 7, "HTML", Some(404), true, true, ok, Some("h5"));
    row(&f, 8, "HTML", Some(200), true, true, no, Some("h6"));
    row(&f, 9, "HTML", Some(200), true, true, no, Some("h6"));
    row(&f, 10, "HTML", Some(200), true, true, ok, Some("h6"));
    row(&f, 11, "PDF", Some(200), true, true, ok, Some("h7"));
    row(&f, 12, "HTML", Some(200), true, true, ok, Some("h7"));
    row(&f, 13, "HTML", Some(200), false, true, ok, Some("h8"));
    row(&f, 14, "HTML", Some(200), true, true, ok, Some("h8"));
    row(&f, 15, "HTML", Some(200), true, true, ok, None);
    row(&f, 16, "HTML", Some(200), true, true, ok, Some(""));
    row(&f, 17, "HTML", Some(200), true, true, ok, Some(""));
    row(&f, 18, "HTML", Some(200), true, false, ok, Some("h9"));
    row(&f, 19, "HTML", Some(200), true, true, ok, Some("h9"));
    row(&f, 20, "HTML", None, true, true, ok, Some("h10"));
    row(&f, 21, "HTML", Some(200), true, true, ok, Some("h10"));
    f
}

/// Treffer eines Prädikats über allen Zeilen von `urls` (ohne Grundmenge), mit den Parametern des Tabs.
fn hits(file: &CrawlFile, predicate: &str, only_indexable: bool) -> BTreeSet<i64> {
    let sql = format!("SELECT u.id FROM urls u WHERE {predicate} ORDER BY u.id");
    let mut stmt = file.conn().prepare(&sql).unwrap();
    for name in [":dup_only_indexable", ":pdf_store", ":low_word_count"] {
        if let Some(idx) = stmt.parameter_index(name).unwrap() {
            let value = match name {
                ":dup_only_indexable" => i64::from(only_indexable),
                ":pdf_store" => 0,
                _ => 10,
            };
            stmt.raw_bind_parameter(idx, value).unwrap();
        }
    }
    let mut rows = stmt.raw_query();
    let mut out = BTreeSet::new();
    while let Some(r) = rows.next().unwrap() {
        out.insert(r.get::<_, i64>(0).unwrap());
    }
    out
}

fn new_predicate() -> String {
    plan(
        &ViewQuery::new("Content", "Exact Duplicates"),
        &CrawlConfig::default(),
    )
    .unwrap()
    .filter_sql
}

// spec: ONPAGE-143
#[test]
fn the_grouping_gives_the_same_hits_as_the_correlated_exists_for_every_row() {
    let dir = tempfile::tempdir().unwrap();
    let file = fixture(dir.path());
    let new = new_predicate();
    assert!(!new.contains("EXISTS"), "unkorreliert, kein EXISTS: {new}");
    for only_indexable in [true, false] {
        let old_hits = hits(&file, OLD_EXACT_DUPLICATES, only_indexable);
        let new_hits = hits(&file, &new, only_indexable);
        assert_eq!(
            new_hits, old_hits,
            "Treffermenge über alle Zeilen, only_indexable = {only_indexable}"
        );
    }
    // Die Fixture deckt die Fälle ab: der Vergleich ist nicht leer und nicht trivial.
    assert_eq!(
        hits(&file, &new, true),
        BTreeSet::from([1, 2, 3, 5, 13, 18, 20]),
        "Seiten außerhalb der Vergleichsmenge (3, 5, 13, 18, 20) genügt ein Treffer in ihr"
    );
    assert_eq!(
        hits(&file, &new, false),
        BTreeSet::from([1, 2, 3, 5, 8, 9, 10, 13, 18, 20])
    );
}

// spec: ONPAGE-143
#[test]
fn the_view_lists_the_same_pages_as_before_with_only_indexable_on_and_off() {
    let dir = tempfile::tempdir().unwrap();
    let file = fixture(dir.path());
    let ids = |cfg: &CrawlConfig| -> BTreeSet<i64> {
        let mut q = ViewQuery::new("Content", "Exact Duplicates");
        q.limit = 500;
        query_view(&file, cfg, &q)
            .unwrap()
            .row_keys
            .iter()
            .map(|k| k.0)
            .collect()
    };
    let mut cfg = CrawlConfig::default();
    assert!(cfg.duplicates.only_indexable);
    // Grundmenge: intern, in den Tabellen, 2xx, HTML: 3 (404), 5 (404), 13 (extern), 18, 20 fallen heraus
    assert_eq!(
        ids(&cfg),
        BTreeSet::from([1, 2]),
        "nur die Einzelfälle der Vergleichsmenge"
    );
    cfg.duplicates.only_indexable = false;
    assert_eq!(ids(&cfg), BTreeSet::from([1, 2, 8, 9, 10]));
    // Einzelseiten bleiben draußen, auch wenn eine Nicht-2xx-, Nicht-HTML- oder externe Seite ihren Hash trägt
    // (4 mit 404, 12 mit PDF, 14 mit externer Seite, 19 und 21 mit einer Zeile ohne Tabellen bzw. ohne Status)
    for single in [4, 12, 14, 19, 21] {
        assert!(!ids(&cfg).contains(&single), "{single}");
    }
}

/// Zeilen des `EXPLAIN QUERY PLAN`: (id, parent, detail).
fn explain(file: &CrawlFile, tab: &str, filter: &str) -> Vec<(i64, i64, String)> {
    let mut cfg = CrawlConfig::default();
    cfg.ui.parity_filters = false;
    let p = plan(&ViewQuery::new(tab, filter), &cfg).unwrap();
    let sql = format!(
        "EXPLAIN QUERY PLAN SELECT u.id FROM {} WHERE {} AND {}",
        os_analyze::views::URLS_FROM,
        p.population_sql,
        p.filter_sql
    );
    let mut stmt = file.conn().prepare(&sql).unwrap();
    for (name, value) in &p.params {
        if let Some(idx) = stmt.parameter_index(name).unwrap() {
            match value {
                Value::Int(i) => stmt.raw_bind_parameter(idx, *i).unwrap(),
                Value::Bool(b) => stmt.raw_bind_parameter(idx, i64::from(*b)).unwrap(),
                Value::Float(x) => stmt.raw_bind_parameter(idx, *x).unwrap(),
                Value::Text(t) => stmt.raw_bind_parameter(idx, t.as_str()).unwrap(),
                Value::Null => stmt.raw_bind_parameter(idx, rusqlite::types::Null).unwrap(),
            }
        }
    }
    let mut rows = stmt.raw_query();
    let mut out = Vec::new();
    while let Some(r) = rows.next().unwrap() {
        out.push((
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, String>(3).unwrap(),
        ));
    }
    out
}

/// Ob unterhalb einer korrelierten Unterabfrage ein Lauf über `urls` steht (ein Lauf je Zeile der äußeren Abfrage).
fn correlated_scan_of_urls(plan: &[(i64, i64, String)]) -> Option<String> {
    let under_correlated = |mut id: i64| -> bool {
        loop {
            let Some((_, parent, _)) = plan.iter().find(|(i, _, _)| *i == id) else {
                return false;
            };
            if *parent == 0 {
                return false;
            }
            if plan
                .iter()
                .any(|(i, _, d)| i == parent && d.starts_with("CORRELATED"))
            {
                return true;
            }
            id = *parent;
        }
    };
    plan.iter()
        .find(|(id, _, d)| {
            (d.starts_with("SCAN d") || d.starts_with("SCAN urls") || d.contains("SCAN TABLE urls"))
                && under_correlated(*id)
        })
        .map(|(_, _, d)| d.clone())
}

/// Schaltet die EXISTS-to-JOIN-Optimierung von SQLite für diese Verbindung ab (B4-DEPSEC). Seit dem gebündelten
/// SQLite 3.54 (rusqlite 0.41) wandelt der Planer ein `EXISTS` als oberstes `AND`-Glied in einen Join mit
/// automatischem Index um; die Gegenprobe sähe dann keine korrelierte Unterabfrage mehr und prüfte die Erkennung nicht.
/// Die Maske ist die Konstante `SQLITE_ExistsToJoin` aus `sqliteInt.h` (kein stabiles API): ändert SQLite sie, schlägt
/// die Gegenprobe fehl, statt leer durchzulaufen.
fn without_exists_to_join(conn: &rusqlite::Connection) {
    const SQLITE_EXISTS_TO_JOIN: u32 = 0x4000_0000;
    use rusqlite::ffi;
    let mut flags: std::ffi::c_int = 0;
    // SAFETY: gültiger Verbindungszeiger; GETOPT schreibt genau einen `int`, OPTIMIZATIONS liest ein `u32`.
    unsafe {
        let db = conn.handle();
        assert_eq!(
            ffi::sqlite3_test_control(
                ffi::SQLITE_TESTCTRL_GETOPT,
                db,
                &mut flags as *mut std::ffi::c_int
            ),
            ffi::SQLITE_OK
        );
        ffi::sqlite3_test_control(
            ffi::SQLITE_TESTCTRL_OPTIMIZATIONS,
            db,
            flags as u32 | SQLITE_EXISTS_TO_JOIN,
        );
    }
}

fn without_the_five_indexes(file: &CrawlFile) {
    for index in WRITE_PHASE_DROPPED {
        file.conn()
            .execute_batch(&format!("DROP INDEX IF EXISTS {}", index.name))
            .unwrap();
    }
    assert!(!os_store::url_index::present(file.conn()).unwrap());
}

const DUPLICATE_FILTERS: [(&str, &str); 5] = [
    ("Page Titles", "Duplicate"),
    ("Meta Description", "Duplicate"),
    ("H1", "Duplicate"),
    ("H2", "Duplicate"),
    ("Content", "Exact Duplicates"),
];

// spec: ONPAGE-143
#[test]
fn the_five_duplicate_filters_have_no_correlated_subquery_over_urls_with_and_without_the_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let file = fixture(dir.path());
    for with_indexes in [true, false] {
        if !with_indexes {
            without_the_five_indexes(&file);
        }
        for (tab, filter) in DUPLICATE_FILTERS {
            let details = explain(&file, tab, filter);
            let text: Vec<&str> = details.iter().map(|(_, _, d)| d.as_str()).collect();
            // (Eine korrelierte Unterabfrage über `pagination` je Zeile bleibt: sie sucht über den Primärschlüssel.)
            assert!(
                !text.iter().any(|d| d.contains("CORRELATED LIST SUBQUERY")),
                "{tab}:{filter} (Indizes: {with_indexes}): {text:?}"
            );
            assert_eq!(
                correlated_scan_of_urls(&details),
                None,
                "{tab}:{filter} (Indizes: {with_indexes}): {text:?}"
            );
            assert!(
                text.iter().any(|d| d.contains("LIST SUBQUERY")),
                "{tab}:{filter}: unkorrelierte Liste erwartet: {text:?}"
            );
            assert!(
                text.iter().any(|d| d.contains("GROUP BY")),
                "{tab}:{filter}: Gruppierung erwartet: {text:?}"
            );
        }
    }
}

// spec: ONPAGE-143
#[test]
fn no_filter_of_any_tab_searches_one_of_the_five_indexes_or_scans_urls_per_row() {
    let dir = tempfile::tempdir().unwrap();
    let file = fixture(dir.path());
    let names: Vec<&str> = WRITE_PHASE_DROPPED.iter().map(|i| i.name).collect();
    let mut checked = 0;
    for with_indexes in [true, false] {
        if !with_indexes {
            without_the_five_indexes(&file);
        }
        for tab in tabs::all() {
            for filter in &tab.filters {
                let details = explain(&file, tab.id, filter.id);
                let text: Vec<&str> = details.iter().map(|(_, _, d)| d.as_str()).collect();
                // 1. keiner der fünf Indizes im Plan, auch wenn er in der Datei steht: kein Leser braucht ihn
                assert!(
                    !text.iter().any(|d| names.iter().any(|n| d.contains(n))),
                    "{}:{} nutzt einen der fünf Indizes: {text:?}",
                    tab.id,
                    filter.id
                );
                // 2. ohne die Indizes kein Lauf über `urls` je Zeile
                assert_eq!(
                    correlated_scan_of_urls(&details),
                    None,
                    "{}:{}: {text:?}",
                    tab.id,
                    filter.id
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "alle Filter aller Tabs geprüft: {checked}");
}

// spec: ONPAGE-143
#[test]
fn the_check_would_have_caught_the_old_formulation() {
    // Gegenprobe: das frühere korrelierte `EXISTS` sucht über `urls_hash`, und ohne den Index läuft es je Zeile über `urls`.
    let dir = tempfile::tempdir().unwrap();
    let file = fixture(dir.path());
    let plan_of = |file: &CrawlFile| -> Vec<(i64, i64, String)> {
        let sql =
            format!("EXPLAIN QUERY PLAN SELECT u.id FROM urls u WHERE {OLD_EXACT_DUPLICATES}");
        let mut stmt = file.conn().prepare(&sql).unwrap();
        let idx = stmt
            .parameter_index(":dup_only_indexable")
            .unwrap()
            .unwrap();
        stmt.raw_bind_parameter(idx, 1).unwrap();
        let mut rows = stmt.raw_query();
        let mut out = Vec::new();
        while let Some(r) = rows.next().unwrap() {
            out.push((
                r.get(0).unwrap(),
                r.get(1).unwrap(),
                r.get::<_, String>(3).unwrap(),
            ));
        }
        out
    };
    let with = plan_of(&file);
    assert!(
        with.iter().any(|(_, _, d)| d.contains("urls_hash")),
        "mit Index: {with:?}"
    );
    without_the_five_indexes(&file);
    without_exists_to_join(file.conn());
    let without = plan_of(&file);
    assert!(
        correlated_scan_of_urls(&without).is_some(),
        "ohne Index: Lauf über urls je Zeile: {without:?}"
    );
}

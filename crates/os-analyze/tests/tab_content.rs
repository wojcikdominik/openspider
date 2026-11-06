//! Tab „Content“ gegen echte Crawls der Fixtures `onpage-content` und `onpage-content-dupes`: Spalten,
//! Exact Duplicates (live per Hash), Low Content Pages, Grundmenge und Konfigurationszustände (M1-P23).

use os_analyze::FilterKind;
use os_model::FilterState;

#[path = "framework_common.rs"]
mod store_fixture;

const TAB: &str = "Content";
const SCENARIO: &str = "onpage-content";
const DUPES: &str = "onpage-content-dupes";

// ---------------------------------------------------------------------------------------------------
// Gemeinsame Hilfen der Element-Tab-Tests (in jeder Testdatei des Pakets M1-P23 gleich: ein eigenes
// Hilfsmodul gehört nicht zum Besitz des Pakets).

use os_analyze::views::{filter_counts, query_view, ColumnSelection, ViewPage, ViewQuery};
use os_engine::testing::{crawl_fixture, FixtureCrawl};
use os_model::{CrawlConfig, Value};

/// Crawlt das Szenario vollständig (Testsite auf Loopback).
async fn crawl(scenario: &str, cfg: CrawlConfig) -> FixtureCrawl {
    crawl_fixture(scenario, cfg).await
}

/// Alle Zeilen einer Ansicht mit allen Spalten.
fn view(fx: &FixtureCrawl, cfg: &CrawlConfig, tab: &str, filter: &str) -> ViewPage {
    let mut q = ViewQuery::new(tab, filter);
    q.columns = ColumnSelection::All;
    q.limit = 500;
    query_view(&fx.file, cfg, &q).unwrap_or_else(|e| panic!("{tab}:{filter}: {e}"))
}

/// Adresse ohne Basis-URL (`/basic.html`; externe Adressen bleiben vollständig).
fn short(fx: &FixtureCrawl, address: &str) -> String {
    address
        .strip_prefix(&fx.site.base_url())
        .unwrap_or(address)
        .to_string()
}

/// Adressen einer Ansicht in Anzeigereihenfolge.
fn addresses(fx: &FixtureCrawl, cfg: &CrawlConfig, tab: &str, filter: &str) -> Vec<String> {
    view(fx, cfg, tab, filter)
        .rows
        .iter()
        .map(|r| match &r[0] {
            Value::Text(a) => short(fx, a),
            other => panic!("Address ist kein Text: {other:?}"),
        })
        .collect()
}

/// Adressen einer Ansicht, alphabetisch (Mengenvergleich).
fn sorted_addresses(fx: &FixtureCrawl, cfg: &CrawlConfig, tab: &str, filter: &str) -> Vec<String> {
    let mut v = addresses(fx, cfg, tab, filter);
    v.sort();
    v
}

/// Erwartete Adressen, alphabetisch.
fn expect(list: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    v.sort();
    v
}

/// Wert einer Zelle der Zeile mit der Adresse `path`.
fn cell(fx: &FixtureCrawl, page: &ViewPage, path: &str, column: &str) -> Value {
    let col = page
        .columns
        .iter()
        .position(|c| c == column)
        .unwrap_or_else(|| panic!("Spalte {column} fehlt in {:?}", page.columns));
    let row = page
        .rows
        .iter()
        .find(|r| matches!(&r[0], Value::Text(a) if short(fx, a) == path))
        .unwrap_or_else(|| panic!("Zeile {path} fehlt"));
    row[col].clone()
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

/// Zähler (Treffer, Anzeigename) eines Filters aus der Übersicht.
fn counted(fx: &FixtureCrawl, cfg: &CrawlConfig, tab: &str, filter_id: &str) -> (u64, String) {
    let counts = filter_counts(&fx.file, cfg).unwrap();
    let c = counts
        .iter()
        .find(|c| c.tab == tab && c.filter == filter_id)
        .unwrap_or_else(|| panic!("{tab}:{filter_id} fehlt in der Übersicht"));
    (c.count.expect("Filter ist bereit"), c.label.clone())
}

/// CSV-Wiedergabe einer Ansicht mit allen Spalten (Dateikopf, RFC 4180, UTF-8 mit BOM, `{BASE}`).
fn render_csv(fx: &FixtureCrawl, page: &ViewPage) -> String {
    fn field(v: &Value) -> String {
        let s = match v {
            Value::Null => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Text(t) => t.clone(),
        };
        if s.contains([',', '"', '\r', '\n']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s
        }
    }
    let mut out = String::from('\u{feff}');
    out.push_str(&page.columns.join(","));
    out.push('\n');
    for row in &page.rows {
        let cells: Vec<String> = row.iter().map(field).collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    os_testsite::golden::normalize(&out, &[&fx.site.base_url()])
}

/// Dateiname-Teil: klein, Buchstaben und Ziffern, sonst `_` (`Over 60 Characters` → `over_60_characters`).
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

/// Regressions-Goldens am kanonischen Ort (K2-63, ONPAGE-142/149).
fn assert_view_golden(
    fx: &FixtureCrawl,
    cfg: &CrawlConfig,
    scenario: &str,
    tab: &str,
    filter_id: &str,
) {
    let label = os_analyze::names::label(filter_id, cfg);
    let page = view(fx, cfg, tab, filter_id);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden")
        .join(scenario)
        .join(format!("{}__{}.csv", slug(tab), slug(&label)));
    os_testsite::golden::assert_golden(path, &render_csv(fx, &page));
}

fn hex32(v: &Value) -> bool {
    matches!(v, Value::Text(h) if h.len() == 32 && h.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
}

/// Alle internen 2xx-HTML-Seiten von `onpage-content` (ONPAGE-009), alphabetisch.
fn content_pages() -> Vec<String> {
    expect(&[
        "/",
        "/empty.html",
        "/footer-heavy.html",
        "/header-aside.html",
        "/noindex-50.html",
        "/words-199.html",
        "/words-200.html",
    ])
}

// spec: ONPAGE-142
#[tokio::test]
async fn columns_follow_the_spec_and_unavailable_functions_are_empty_and_hidden() {
    let fx = crawl(SCENARIO, CrawlConfig::default()).await;
    let cfg = fx.file.config().unwrap();
    let tab = os_analyze::tabs::content::tab();
    let keys: Vec<&str> = tab.columns.iter().map(|c| c.key).collect();
    assert_eq!(
        keys,
        [
            "Address",
            "Word Count",
            "Sentence Count",
            "Average Words Per Sentence",
            "Flesch Reading Ease Score",
            "Readability",
            "Closest Similarity Match",
            "No. Near Duplicates",
            "Closest Semantically Similar Address",
            "Semantic Similarity Score",
            "No. Semantically Similar",
            "Semantic Relevance Score",
            "Total Language Errors",
            "Spelling Errors",
            "Grammar Errors",
            "Language",
            "Hash",
            "Indexability",
            "Indexability Status",
            "Title 1",
            "Title 1 Length",
            "Title 1 Pixel Width",
            "H1-1",
            "H1-1 Length",
            "H2-1",
            "H2-1 Length",
            "Occurrences",
            "Matched Phrase",
        ]
    );
    // Verfügbare Teilanalysen sind sichtbar; die konfigurierte Ansicht blendet deaktivierte aus.
    let visible: Vec<&str> = tab
        .columns
        .iter()
        .filter(|c| c.default_visible)
        .map(|c| c.key)
        .collect();
    assert_eq!(
        visible,
        [
            "Address",
            "Word Count",
            "Sentence Count",
            "Average Words Per Sentence",
            "Flesch Reading Ease Score",
            "Readability",
            "Closest Similarity Match",
            "No. Near Duplicates",
            "Total Language Errors",
            "Spelling Errors",
            "Grammar Errors",
            "Language",
            "Hash",
            "Indexability",
            "Indexability Status"
        ]
    );
    assert!(tab.columns.last().unwrap().extension);
    assert_eq!(tab.columns[0].kind, os_analyze::ColumnKind::Url);

    let page = view(&fx, &cfg, TAB, "All");
    assert_eq!(page.columns.len(), keys.len());
    for column in &keys[6..16] {
        assert!(
            page.rows
                .iter()
                .all(|_| cell(&fx, &page, "/words-200.html", column) == Value::Null),
            "{column} ist leer"
        );
    }
    // Standardansicht: nur die sichtbaren Spalten.
    let mut q = ViewQuery::new(TAB, "All");
    q.limit = 500;
    assert_eq!(
        query_view(&fx.file, &cfg, &q).unwrap().columns,
        [
            "Address",
            "Word Count",
            "Sentence Count",
            "Average Words Per Sentence",
            "Flesch Reading Ease Score",
            "Readability",
            "Closest Similarity Match",
            "No. Near Duplicates",
            "Hash",
            "Indexability",
            "Indexability Status"
        ]
    );

    // Word Count aus dem Inhaltsbereich, Hash wie in der Crawl-Datei (Internal), Indexierbarkeit als Spalte.
    assert_eq!(
        cell(&fx, &page, "/words-200.html", "Word Count"),
        Value::Int(200)
    );
    assert_eq!(
        cell(&fx, &page, "/words-200.html", "Indexability"),
        text("Indexable")
    );
    let stored: String = fx
        .file
        .conn()
        .query_row(
            "SELECT hash_md5 FROM urls WHERE address LIKE '%/words-200.html'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let shown = cell(&fx, &page, "/words-200.html", "Hash");
    assert!(hex32(&shown), "Hash ist MD5 in Kleinbuchstaben: {shown:?}");
    assert_eq!(shown, Value::Text(stored));
}

// spec: ONPAGE-142 [M2]
#[tokio::test]
async fn hash_column_is_empty_when_hash_storage_is_off() {
    let mut crawl_cfg = CrawlConfig::default();
    crawl_cfg.extraction.hash = false;
    let fx = crawl(SCENARIO, crawl_cfg).await;
    let cfg = fx.file.config().unwrap();
    let page = view(&fx, &cfg, TAB, "All");
    assert_eq!(page.total as usize, content_pages().len());
    for path in content_pages() {
        assert_eq!(cell(&fx, &page, &path, "Hash"), Value::Null, "{path}");
    }
}

// spec: ONPAGE-142, ONPAGE-147, ONPAGE-148, ONPAGE-150, ONPAGE-151
#[tokio::test]
async fn base_set_is_internal_html_with_2xx() {
    let fx = crawl(SCENARIO, CrawlConfig::default()).await;
    let cfg = fx.file.config().unwrap();
    let all = sorted_addresses(&fx, &cfg, TAB, "All");
    assert_eq!(all, content_pages());
    // Bild, Weiterleitung (/old.html), 404 (/missing.html), 500, externe Seite und PDF (ohne pdf.store)
    // gehören nicht zur Grundmenge.
    for absent in [
        "/img/logo.png",
        "/old.html",
        "/missing.html",
        "/server-error.html",
        "/short.pdf",
    ] {
        assert!(!all.contains(&absent.to_string()), "{absent}");
    }
    assert!(
        all.iter().all(|a| a.starts_with('/')),
        "keine externe Adresse: {all:?}"
    );
    // Die Fixture enthält die Ausschlüsse tatsächlich (sie wurden gecrawlt, stehen nur nicht im Tab).
    let in_urls: i64 = fx
        .file
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE address LIKE '%/old.html' OR address LIKE '%/missing.html' \
             OR address LIKE '%/server-error.html' OR address LIKE '%/external.html'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(in_urls, 4);
    for f in os_analyze::tabs::content::tab().filters {
        let needs = match f.id {
            "Near Duplicates" => Some("duplicates.near_enabled"),
            "Spelling Errors" => Some("spelling.enabled"),
            "Grammar Errors" => Some("grammar.enabled"),
            _ => None,
        };
        if let Some(key) = needs {
            assert_eq!(
                os_analyze::views::state_of(&fx.file, &cfg, TAB, f.id).unwrap(),
                FilterState::NeedsConfig(key.into())
            );
            assert_eq!(view(&fx, &cfg, TAB, f.id).total, 0);
            continue;
        }
        assert!(matches!(f.kind, FilterKind::Live), "{}", f.id);
        let rows = sorted_addresses(&fx, &cfg, TAB, f.id);
        assert!(rows.iter().all(|a| all.contains(a)), "{} ⊆ All", f.id);
        assert_eq!(counted(&fx, &cfg, TAB, f.id).0 as usize, rows.len());
    }
}

// spec: ONPAGE-142
#[tokio::test]
async fn pdf_is_in_the_base_set_only_with_pdf_store() {
    let off = crawl(SCENARIO, CrawlConfig::default()).await;
    let cfg_off = off.file.config().unwrap();
    assert!(!sorted_addresses(&off, &cfg_off, TAB, "All").contains(&"/short.pdf".to_string()));

    let mut with_pdf = off.file.config().unwrap();
    with_pdf.pdf.store = true;
    let all = sorted_addresses(&off, &with_pdf, TAB, "All");
    assert!(all.contains(&"/short.pdf".to_string()), "{all:?}");
    // Ohne neuen Abruf: die Umstellung wirkt allein über die Konfiguration.
    assert_eq!(all.len(), content_pages().len() + 1);

    // Hash ist für PDFs leer (A-53, ONPAGE-018), das PDF steht nie in Exact Duplicates und nie in Low Content Pages
    // (50 Wörter lägen unter der Schwelle).
    let page = view(&off, &with_pdf, TAB, "All");
    assert_eq!(cell(&off, &page, "/short.pdf", "Hash"), Value::Null);
    let stored_hash: Option<String> = off
        .file
        .conn()
        .query_row(
            "SELECT hash_md5 FROM urls WHERE address LIKE '%/short.pdf'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        stored_hash.is_none(),
        "K-60 (ONPAGE-018): Hash nur für HTML-Antworten, auch in der Datei nicht für die PDF"
    );
    assert!(!sorted_addresses(&off, &with_pdf, TAB, "Low Content Pages")
        .contains(&"/short.pdf".to_string()));
    assert!(!sorted_addresses(&off, &with_pdf, TAB, "Exact Duplicates")
        .contains(&"/short.pdf".to_string()));
}

// spec: ONPAGE-146
#[tokio::test]
async fn low_content_pages_use_the_word_count_threshold() {
    let fx = crawl(SCENARIO, CrawlConfig::default()).await;
    let cfg = fx.file.config().unwrap();
    assert_eq!(cfg.content.low_word_count, 200);
    let page = view(&fx, &cfg, TAB, "All");
    assert_eq!(
        cell(&fx, &page, "/words-199.html", "Word Count"),
        Value::Int(199)
    );
    assert_eq!(
        cell(&fx, &page, "/footer-heavy.html", "Word Count"),
        Value::Int(150)
    );
    // SQ-09: nav/footer/header/aside zählen nicht.
    assert_eq!(
        cell(&fx, &page, "/header-aside.html", "Word Count"),
        Value::Int(100)
    );

    let low = sorted_addresses(&fx, &cfg, TAB, "Low Content Pages");
    // 199 Wörter: ja; 200: nein; 150 im main und 300 im footer: ja; leere Seite (0 Wörter): ja.
    // noindex-50 (50 Wörter) fällt bei ignore_non_indexable (Standard) heraus, siehe unten.
    assert_eq!(
        low,
        expect(&[
            "/empty.html",
            "/footer-heavy.html",
            "/header-aside.html",
            "/words-199.html",
        ])
    );
    assert!(!low.contains(&"/words-200.html".to_string()));
    assert!(!low.contains(&"/".to_string()));
    // Kein Golden für „All“: Die Startseite enthält die Port-Platzhalter, ihr Hash ist je Lauf anders.
    assert_view_golden(&fx, &cfg, SCENARIO, TAB, "Low Content Pages");

    // Die Schwelle ist eine Konfiguration: 200 → die 200-Wörter-Seite fällt auch bei 201 hinein, bei 199
    // fällt die 199-Wörter-Seite heraus – ohne neuen Abruf.
    let mut c201 = cfg.clone();
    c201.content.low_word_count = 201;
    assert!(sorted_addresses(&fx, &c201, TAB, "Low Content Pages")
        .contains(&"/words-200.html".to_string()));
    let mut c199 = cfg.clone();
    c199.content.low_word_count = 199;
    assert!(!sorted_addresses(&fx, &c199, TAB, "Low Content Pages")
        .contains(&"/words-199.html".to_string()));
}

// spec: ONPAGE-146
#[tokio::test]
async fn low_content_respects_ignore_non_indexable() {
    let fx = crawl(SCENARIO, CrawlConfig::default()).await;
    let cfg = fx.file.config().unwrap();
    assert!(cfg.issues.ignore_non_indexable);
    // noindex-50.html ist nach dem Crawl nicht indexierbar (echte Bewertung der Engine, TECH-001).
    let page = view(&fx, &cfg, TAB, "All");
    assert_eq!(
        cell(&fx, &page, "/noindex-50.html", "Indexability"),
        text("Non-Indexable")
    );
    assert_eq!(
        cell(&fx, &page, "/noindex-50.html", "Indexability Status"),
        text("noindex")
    );
    // Standard (true): nicht im Filter, aber weiter in „All“.
    assert!(!sorted_addresses(&fx, &cfg, TAB, "Low Content Pages")
        .contains(&"/noindex-50.html".to_string()));
    assert!(sorted_addresses(&fx, &cfg, TAB, "All").contains(&"/noindex-50.html".to_string()));
    // false: im Filter (A-38: Indexierbarkeit spielt sonst keine Rolle).
    let mut off = cfg.clone();
    off.issues.ignore_non_indexable = false;
    assert!(sorted_addresses(&fx, &off, TAB, "Low Content Pages")
        .contains(&"/noindex-50.html".to_string()));
    assert_eq!(counted(&fx, &off, TAB, "Low Content Pages").0, 5);
    assert_eq!(counted(&fx, &cfg, TAB, "Low Content Pages").0, 4);
}

// spec: ONPAGE-143
#[tokio::test]
async fn exact_duplicates_compare_the_hash_of_indexable_pages() {
    let fx = crawl(DUPES, CrawlConfig::default()).await;
    let cfg = fx.file.config().unwrap();
    let all = sorted_addresses(&fx, &cfg, TAB, "All");
    assert_eq!(
        all,
        expect(&[
            "/",
            "/a.html",
            "/a.html?ref=x",
            "/b.html",
            "/c.html",
            "/noidx.html",
            "/short-a.html",
            "/short-b.html",
            "/unique.html",
        ])
    );
    let dupes = sorted_addresses(&fx, &cfg, TAB, "Exact Duplicates");
    // /a und /b byte-identisch; /a?ref=x liefert dieselbe Datei; /c hat ein Leerzeichen mehr;
    // /noidx.html (identisch, aber noindex) ist nicht in der Vergleichsmenge; identische 404-Seiten
    // (/gone1, /gone2) sind nicht in der Grundmenge.
    assert_eq!(
        dupes,
        expect(&[
            "/a.html",
            "/a.html?ref=x",
            "/b.html",
            "/short-a.html",
            "/short-b.html"
        ])
    );
    assert!(!dupes.contains(&"/c.html".to_string()));
    assert!(!dupes.contains(&"/noidx.html".to_string()));
    assert!(!dupes.contains(&"/unique.html".to_string()));
    assert!(dupes.iter().all(|a| !a.contains("gone")));
    // Die 404-Seiten haben denselben Inhalt, aber keinen Hash (Hash nur für HTML mit 2xx, ONPAGE-018, K-60); die
    // Grundmenge schließt sie unabhängig davon aus.
    let (gone_pages, gone_hashes): (i64, i64) = fx
        .file
        .conn()
        .query_row(
            "SELECT COUNT(*), COUNT(hash_md5) FROM urls WHERE address LIKE '%/gone%.html' AND status_code = 404",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(gone_pages >= 2, "{gone_pages}");
    assert_eq!(gone_hashes, 0);
    assert_view_golden(&fx, &cfg, DUPES, TAB, "Exact Duplicates");
    assert_view_golden(&fx, &cfg, DUPES, TAB, "All");
}

// spec: ONPAGE-143
#[tokio::test]
async fn exact_duplicates_only_indexable_switch_works_without_new_requests() {
    let fx = crawl(DUPES, CrawlConfig::default()).await;
    let cfg = fx.file.config().unwrap();
    let requests = fx.site.total_requests();
    let mut all_pages = cfg.clone();
    all_pages.duplicates.only_indexable = false;
    let dupes = sorted_addresses(&fx, &all_pages, TAB, "Exact Duplicates");
    assert_eq!(
        dupes,
        expect(&[
            "/a.html",
            "/a.html?ref=x",
            "/b.html",
            "/noidx.html",
            "/short-a.html",
            "/short-b.html"
        ])
    );
    // Auch mit only_indexable = false nie 404-Seiten und /c.html.
    assert!(dupes.iter().all(|a| !a.contains("gone") && a != "/c.html"));
    // ignore_non_indexable (CRAWL-089) wirkt hier nicht: die Vergleichsmenge regelt nur only_indexable.
    assert!(all_pages.issues.ignore_non_indexable);
    assert_eq!(fx.site.total_requests(), requests);
}

// spec: ONPAGE-143
#[tokio::test]
async fn exact_duplicates_are_live_not_a_crawl_analysis_filter() {
    let fx = crawl(DUPES, CrawlConfig::default()).await;
    let cfg = fx.file.config().unwrap();
    let filter = os_analyze::tabs::content::tab()
        .filters
        .into_iter()
        .find(|f| f.id == "Exact Duplicates")
        .unwrap();
    assert_eq!(filter.kind, FilterKind::Live);
    // Es lief keine Crawl-Analyse, der Filter ist trotzdem bereit und liefert Treffer.
    let steps: i64 = fx
        .file
        .conn()
        .query_row("SELECT COUNT(*) FROM analysis_steps", [], |r| r.get(0))
        .unwrap();
    assert_eq!(steps, 0);
    assert_eq!(
        os_analyze::views::state_of(&fx.file, &cfg, TAB, "Exact Duplicates").unwrap(),
        FilterState::Ready
    );
    assert!(counted(&fx, &cfg, TAB, "Exact Duplicates").0 > 0);
}

// spec: ONPAGE-143
#[tokio::test]
async fn exact_duplicates_need_hash_storage_and_the_exact_switch() {
    // Speicher-Schalter „Hash“ aus: kein Hash in der Datei, der Filter zeigt den Zustand statt Treffern.
    let mut crawl_cfg = CrawlConfig::default();
    crawl_cfg.extraction.hash = false;
    let fx = crawl(DUPES, crawl_cfg).await;
    let cfg = fx.file.config().unwrap();
    assert!(!cfg.extraction.hash);
    assert_eq!(
        os_analyze::views::state_of(&fx.file, &cfg, TAB, "Exact Duplicates").unwrap(),
        FilterState::NeedsConfig("extraction.hash".to_string())
    );
    let page = view(&fx, &cfg, TAB, "Exact Duplicates");
    assert_eq!(page.total, 0);
    assert!(page.rows.is_empty());
    let counts = filter_counts(&fx.file, &cfg).unwrap();
    let c = counts
        .iter()
        .find(|c| c.tab == TAB && c.filter == "Exact Duplicates")
        .unwrap();
    assert_eq!(
        c.state,
        FilterState::NeedsConfig("extraction.hash".to_string())
    );
    assert_eq!(c.count, None, "nie 0 an Stelle des Zustands");
    // Die Spalte Hash ist leer.
    let all = view(&fx, &cfg, TAB, "All");
    assert!(all.rows.iter().all(|r| r[16] == Value::Null));

    // Nur der Filter aus (OS-Option), Hash bleibt gespeichert.
    let fx = crawl(DUPES, CrawlConfig::default()).await;
    let mut cfg = fx.file.config().unwrap();
    cfg.duplicates.exact_enabled = false;
    assert_eq!(
        os_analyze::views::state_of(&fx.file, &cfg, TAB, "Exact Duplicates").unwrap(),
        FilterState::NeedsConfig("duplicates.exact_enabled".to_string())
    );
    assert_eq!(view(&fx, &cfg, TAB, "Exact Duplicates").total, 0);
    let all = view(&fx, &cfg, TAB, "All");
    assert!(hex32(&cell(&fx, &all, "/a.html", "Hash")));
}

// spec: ONPAGE-142 [P1] [M2]
#[test]
fn pdf_row_has_word_count_and_readability() {
    let facts: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/onpage-m2-content/pdf-facts.json"
    ))
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let (_, file) = store_fixture::create(dir.path(), "pdf.ospider", 42);
    store_fixture::insert(
        &file,
        &[store_fixture::row(1, "text.pdf")
            .text("content_class", "PDF")
            .int("status_code", 200)
            .int("word_count", facts["word_count"].as_i64().unwrap())],
    );
    for (key, sql_value) in [
        ("sentences", rusqlite::types::Value::Integer(1)),
        ("average", 6.0.into()),
        ("flesch", 100.0.into()),
        ("label", "Very Easy".to_string().into()),
    ] {
        let column = match sql_value {
            rusqlite::types::Value::Integer(_) => "v_int",
            rusqlite::types::Value::Real(_) => "v_real",
            _ => "v_text",
        };
        file.conn()
            .execute(
                &format!("INSERT INTO url_ext(url_id,key,{column}) VALUES (1,?1,?2)"),
                rusqlite::params![format!("readability.{key}"), sql_value],
            )
            .unwrap();
    }
    let mut cfg = CrawlConfig::default();
    cfg.pdf.store = true;
    let mut q = ViewQuery::new("Content", "All");
    q.columns = ColumnSelection::All;
    let page = query_view(&file, &cfg, &q).unwrap();
    assert_eq!(page.total, 1);
    for (name, expected) in [
        ("Word Count", Value::Int(6)),
        ("Sentence Count", Value::Int(1)),
        ("Average Words Per Sentence", Value::Float(6.0)),
        ("Flesch Reading Ease Score", Value::Float(100.0)),
        ("Readability", Value::Text("Very Easy".into())),
        ("Hash", Value::Null),
        ("Closest Similarity Match", Value::Null),
    ] {
        let index = page.columns.iter().position(|n| n == name).unwrap();
        assert_eq!(page.rows[0][index], expected, "{name}");
    }
    cfg.extraction.readability = false;
    let page = query_view(&file, &cfg, &q).unwrap();
    assert_eq!(page.rows[0][2], Value::Null);
}

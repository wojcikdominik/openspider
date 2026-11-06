#[path = "framework_common.rs"]
mod common;
use os_analyze::views::{query_view, ViewQuery};
use os_model::CrawlConfig;
fn fixture() -> (tempfile::TempDir, os_store::CrawlFile) {
    let d = tempfile::tempdir().unwrap();
    let (_, f) = common::create(d.path(), "elements.ospider", 17);
    f.conn()
        .execute_batch(include_str!(
            "../../../tests/fixtures/onpage-m2-elements/store.sql"
        ))
        .unwrap();
    (d, f)
}
fn count(f: &os_store::CrawlFile, cfg: &CrawlConfig, tab: &str, filter: &str) -> usize {
    query_view(f, cfg, &ViewQuery::new(tab, filter))
        .unwrap()
        .rows
        .len()
}
// spec: ONPAGE-095
// spec: ONPAGE-096
// spec: ONPAGE-105
// spec: ONPAGE-106
#[test]
fn strict_pixel_boundaries_and_live_thresholds() {
    let (_d, f) = fixture();
    let mut cfg = CrawlConfig::default();
    for (tab, filter, n) in [
        ("Page Titles", "Over X Pixels", 1),
        ("Page Titles", "Below X Pixels", 1),
        ("Meta Description", "Over X Pixels", 1),
        ("Meta Description", "Below X Pixels", 1),
    ] {
        assert_eq!(count(&f, &cfg, tab, filter), n);
    }
    cfg.title.max_px = 562;
    cfg.title.min_px = 199;
    assert_eq!(count(&f, &cfg, "Page Titles", "Over X Pixels"), 0);
    assert_eq!(count(&f, &cfg, "Page Titles", "Below X Pixels"), 0);
}
// spec: ONPAGE-097
#[test]
fn same_h1_is_case_sensitive_nonempty_and_exposes_h1() {
    let (_d, f) = fixture();
    let cfg = CrawlConfig::default();
    assert_eq!(count(&f, &cfg, "Page Titles", "Same as H1"), 1);
    let p = query_view(&f, &cfg, &ViewQuery::new("Page Titles", "Same as H1")).unwrap();
    assert!(p.columns.iter().any(|c| c == "H1-1"));
}
// spec: ONPAGE-110
// spec: ONPAGE-111
// spec: ONPAGE-112
// spec: ONPAGE-113
#[test]
fn keywords_fixed_columns_missing_duplicates_and_multiple() {
    let (_d, f) = fixture();
    let cfg = CrawlConfig::default();
    assert_eq!(count(&f, &cfg, "Meta Keywords", "All"), 5);
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Missing"), 2);
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Duplicate"), 2);
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Multiple"), 1);
    let p = query_view(&f, &cfg, &ViewQuery::new("Meta Keywords", "All")).unwrap();
    assert!(!p.columns.iter().any(|c| c.contains("Pixel")));
}
// spec: ONPAGE-099
// spec: ONPAGE-108
// spec: TECH-008 [P1] [M2]
#[test]
fn outside_coverage_reports_unchecked_pages() {
    let (_d, f) = fixture();
    let mut cfg = CrawlConfig::default();
    cfg.advanced.html_validation = true;
    for tab in ["Page Titles", "Meta Description"] {
        let p = query_view(&f, &cfg, &ViewQuery::new(tab, "Outside <head>")).unwrap();
        assert_eq!(p.rows.len(), 1);
        let counts = os_analyze::views::filter_counts(&f, &cfg).unwrap();
        let c = counts
            .iter()
            .find(|c| c.tab == tab && c.filter == "Outside <head>")
            .unwrap();
        assert_eq!(c.state, os_model::FilterState::Incomplete { unchecked: 4 });
    }
    f.conn()
        .execute("DELETE FROM url_ext WHERE key = 'validation.checked'", [])
        .unwrap();
    let counts = os_analyze::views::filter_counts(&f, &cfg).unwrap();
    assert_eq!(
        counts
            .iter()
            .find(|c| c.tab == "Page Titles" && c.filter == "Outside <head>")
            .unwrap()
            .state,
        os_model::FilterState::NeedsRecrawl
    );
    cfg.advanced.html_validation = false;
    let counts = os_analyze::views::filter_counts(&f, &cfg).unwrap();
    let c = counts
        .iter()
        .find(|c| c.tab == "Page Titles" && c.filter == "Outside <head>")
        .unwrap();
    assert!(matches!(c.state, os_model::FilterState::NeedsConfig(_)));
    assert_eq!(c.count, None);
}
// spec: ONPAGE-125
// spec: ONPAGE-126
// spec: ONPAGE-135
#[test]
fn heading_filters_read_dom_facts() {
    let (_d, f) = fixture();
    let cfg = CrawlConfig::default();
    for (tab, filter) in [
        ("H1", "Alt Text in h1"),
        ("H1", "Non-sequential"),
        ("H2", "Non-sequential"),
    ] {
        assert_eq!(count(&f, &cfg, tab, filter), 1);
    }
}

// spec: ONPAGE-112
#[test]
fn keyword_duplicates_use_first_value_collation_and_live_comparison_switches() {
    let (_d, f) = fixture();
    let mut cfg = CrawlConfig::default();
    // The second value never creates a duplicate, and comma spacing remains significant.
    f.conn()
        .execute("UPDATE urls SET meta_kw_2='a, b' WHERE id=3", [])
        .unwrap();
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Duplicate"), 2);
    f.conn()
        .execute(
            "UPDATE urls SET indexability='Non-Indexable' WHERE id=2",
            [],
        )
        .unwrap();
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Duplicate"), 0);
    cfg.duplicates.only_indexable = false;
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Duplicate"), 1);
    cfg.issues.ignore_non_indexable = false;
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Duplicate"), 2);
    f.conn()
        .execute("UPDATE urls SET has_rel_prev=1 WHERE id=2", [])
        .unwrap();
    f.conn().execute("INSERT INTO pagination (url_id,rel,raw,url,source,in_head,position) VALUES (2,'prev','/1','https://site.test/1','http',0,0)", []).unwrap();
    cfg.duplicates.ignore_paginated = true;
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Duplicate"), 0);
    cfg.duplicates.ignore_paginated = false;
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Duplicate"), 2);
}

// spec: ONPAGE-110
// spec: ONPAGE-111
// spec: ONPAGE-113
#[test]
fn keyword_population_storage_switch_and_occurrence_cap() {
    use os_model::Value;
    let (_d, f) = fixture();
    let mut cfg = CrawlConfig::default();
    let p = query_view(&f, &cfg, &ViewQuery::new("Meta Keywords", "All")).unwrap();
    assert_eq!(
        p.columns,
        [
            "Address",
            "Occurrences",
            "Meta Keywords 1",
            "Meta Keywords 1 Length",
            "Meta Keywords 2",
            "Meta Keywords 2 Length",
            "Indexability",
            "Indexability Status"
        ]
    );
    assert_eq!(p.rows[1][1], Value::Int(2));
    // An empty first occurrence with a nonempty second occurrence is not missing.
    f.conn()
        .execute("UPDATE urls SET meta_kw_2='second' WHERE id=4", [])
        .unwrap();
    assert_eq!(count(&f, &cfg, "Meta Keywords", "Missing"), 1);
    for sql in [
        "UPDATE urls SET status_code=404 WHERE id=1",
        "UPDATE urls SET is_internal=0 WHERE id=2",
        "UPDATE urls SET content_class='PDF' WHERE id=3",
        "UPDATE urls SET in_tables=0 WHERE id=4",
    ] {
        f.conn().execute(sql, []).unwrap();
    }
    assert_eq!(count(&f, &cfg, "Meta Keywords", "All"), 1);
    cfg.extraction.meta_keywords = false;
    assert_eq!(count(&f, &cfg, "Meta Keywords", "All"), 0);
}

// spec: ONPAGE-097
#[test]
fn same_h1_uses_only_first_heading_and_keeps_it_hidden_in_all() {
    let (_d, f) = fixture();
    let cfg = CrawlConfig::default();
    f.conn()
        .execute("UPDATE urls SET h1_2=title_1 WHERE id IN (2,3,5)", [])
        .unwrap();
    assert_eq!(count(&f, &cfg, "Page Titles", "Same as H1"), 1);
    let p = query_view(&f, &cfg, &ViewQuery::new("Page Titles", "All")).unwrap();
    assert!(!p.columns.iter().any(|c| c == "H1-1"));
}

// spec: ONPAGE-095
// spec: ONPAGE-096
// spec: ONPAGE-097
// spec: ONPAGE-105
// spec: ONPAGE-106
// spec: ONPAGE-110
// spec: ONPAGE-111
// spec: ONPAGE-113
// spec: ONPAGE-125
// spec: ONPAGE-126
// spec: ONPAGE-135
#[tokio::test]
async fn real_crawl_transports_dom_facts_and_measured_widths_to_live_filters() {
    let cfg = CrawlConfig::default();
    let crawl = os_engine::testing::crawl_fixture("onpage-m2-elements", cfg.clone()).await;
    let f = &crawl.file;
    for (tab, filter, expected) in [
        ("Page Titles", "Over X Pixels", vec!["/wide.html"]),
        ("Page Titles", "Below X Pixels", vec!["/", "/facts.html"]),
        ("Meta Description", "Over X Pixels", vec!["/wide.html"]),
        ("Meta Description", "Below X Pixels", vec!["/facts.html"]),
        ("H1", "Alt Text in h1", vec!["/facts.html"]),
        ("H1", "Non-sequential", vec!["/facts.html"]),
        ("H2", "Non-sequential", vec!["/facts.html"]),
    ] {
        let p = query_view(f, &cfg, &ViewQuery::new(tab, filter)).unwrap();
        let actual: Vec<_> = p
            .rows
            .iter()
            .map(|r| match &r[0] {
                os_model::Value::Text(address) => address
                    .strip_prefix(&crawl.site.base_url())
                    .unwrap()
                    .to_owned(),
                other => panic!("Address: {other:?}"),
            })
            .collect();
        assert_eq!(actual, expected, "{tab}:{filter}");
    }
    assert_eq!(count(f, &cfg, "Page Titles", "Same as H1"), 5);
    assert_eq!(count(f, &cfg, "Meta Keywords", "All"), 8);
    assert_eq!(count(f, &cfg, "Meta Keywords", "Missing"), 6);
    assert_eq!(count(f, &cfg, "Meta Keywords", "Multiple"), 2);
    let widths: (i64, i64) = f
        .conn()
        .query_row(
            "SELECT title_1_px, meta_desc_1_px FROM urls WHERE address LIKE '%/max-boundary.html'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(widths, (561, 985));
}

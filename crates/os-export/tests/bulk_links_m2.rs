#[path = "links_support.rs"]
mod support;
#[path = "bulk_links_m2_support.rs"]
mod writer_support;
use os_export::{export_bulk, Format};
use os_model::{CrawlConfig, LinkType, UncrawlableReason, WriteBatch};
use os_store::{CrawlFile, CrawlSink, OpenMode, SqliteSink, StorageProfile};
use support::*;

fn fixture(dir: &std::path::Path) -> CrawlFile {
    let f = new_file(dir, "links.ospider");
    for i in 1..=12 {
        insert_url(
            &f,
            i,
            &format!("https://site.test/{i}"),
            true,
            &[
                ("content_class", "'HTML'"),
                ("status_code", "200"),
                ("indexability", "'Indexable'"),
            ],
        );
    }
    exec(
        &f,
        include_str!("../../../tests/fixtures/tech-m2-links/filters.sql"),
    );
    f
}
fn export(f: &CrawlFile, cfg: &CrawlConfig, name: &str, out: &std::path::Path) -> Vec<Vec<String>> {
    let result = export_bulk(f, cfg, &format!("Links:{name}"), out, Format::Csv).unwrap();
    parse_csv(&read_csv(&result[0].path))
}
// spec: OUT-033
#[test]
fn anchor_text_exports_only_hyperlinks_with_exact_columns_and_normalized_text() {
    let dir = tempfile::tempdir().unwrap();
    let f = fixture(dir.path());
    let cfg = CrawlConfig::default();
    let rows = export(&f, &cfg, "All Anchor Text", dir.path());
    assert_eq!(
        rows[0],
        [
            "Source",
            "Destination",
            "Anchor",
            "Alt Text",
            "Link Position",
            "Follow",
            "Status Code"
        ]
    );
    assert!(rows.iter().any(|r| r[2] == "Click Here!"));
    assert!(rows.iter().any(|r| r[2].is_empty() && r[3] == "Logo"));
    assert_eq!(rows.len(), 22); // All 21 stored Hyperlink rows, including raw/uncrawlable targets.
    let info = export_bulk(&f, &cfg, "Links:All Anchor Text", dir.path(), Format::Csv).unwrap();
    assert_golden("out-links/m2-all-anchor-text.csv", &read_csv(&info[0].path));
}
// spec: OUT-034
// spec: OUT-035
// spec: OUT-036
// spec: OUT-037
// spec: OUT-040
#[test]
fn link_level_exports_select_only_offending_links_and_include_resources_where_required() {
    let dir = tempfile::tempdir().unwrap();
    let f = fixture(dir.path());
    let cfg = CrawlConfig::default();
    for (name, count) in [
        ("External Links", 3),
        ("Internal Nofollow Outlinks", 3),
        ("Internal Outlinks With No Anchor Text", 1),
        ("Non-Descriptive Anchor Text In Internal Outlinks", 5),
        ("Outlinks To Localhost", 3),
    ] {
        let rows = export(&f, &cfg, name, dir.path());
        assert_eq!(rows.len() - 1, count, "{name}: {rows:?}");
        assert_eq!(rows[0], os_export::links_export::link_header());
    }
    let rows = export(&f, &cfg, "External Links", dir.path());
    assert!(rows.iter().any(|r| r[0] == "Image"));
    let mut custom = cfg.clone();
    custom.links.non_descriptive_anchors = vec!["foo".into()];
    assert_eq!(
        export(
            &f,
            &custom,
            "Non-Descriptive Anchor Text In Internal Outlinks",
            dir.path()
        )
        .len(),
        1
    );
}
// spec: OUT-038
// spec: OUT-039
// spec: OUT-068
#[test]
fn inlink_exports_share_analysis_filters_and_exclude_resources_and_self() {
    let dir = tempfile::tempdir().unwrap();
    let f = fixture(dir.path());
    let cfg = CrawlConfig::default();
    os_analyze::analysis::runner::run_only(
        &f,
        &cfg,
        &os_model::CancelToken::new(),
        &|_, _| {},
        &[os_model::AnalysisStep::Links],
    )
    .unwrap();
    for (name, counts) in [
        ("Follow & Nofollow Internal Inlinks To Page", 10),
        ("Internal Nofollow Inlinks Only", 2),
        ("Non-Indexable Page Inlinks Only", 4),
    ] {
        let rows = export(&f, &cfg, name, dir.path());
        assert_eq!(rows.len() - 1, counts, "{name}: {rows:?}");
        assert!(rows
            .iter()
            .skip(1)
            .all(|r| r[0] == "Hyperlink" && r[1] != r[2]));
        let q = os_analyze::views::query_view(
            &f,
            &cfg,
            &os_analyze::views::ViewQuery::new("Links", name),
        )
        .unwrap();
        let expected: std::collections::BTreeSet<_> = q
            .rows
            .iter()
            .map(|r| match &r[0] {
                os_model::Value::Text(s) => s.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(
            rows.iter()
                .skip(1)
                .map(|r| r[2].clone())
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
    }
}
// spec: OUT-041
#[test]
fn uncrawlable_export_includes_raw_targets_and_reason_and_is_gated_by_option() {
    let dir = tempfile::tempdir().unwrap();
    let f = fixture(dir.path());
    let mut cfg = CrawlConfig::default();
    assert_eq!(
        export(
            &f,
            &cfg,
            "Pages With Uncrawlable Internal Outlinks",
            dir.path()
        )
        .len(),
        1
    );
    cfg.spider.store_uncrawlable_links = true;
    let rows = export(
        &f,
        &cfg,
        "Pages With Uncrawlable Internal Outlinks",
        dir.path(),
    );
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][15], "Link Crawlability");
    assert!(rows.iter().any(|r| r[15] == "href on non-anchor element"));
    assert!(rows
        .iter()
        .any(|r| r[2] == "mailto:test@site.test" && r[15] == "missing href"));
}

// spec: TECH-142
// spec: OUT-068
#[test]
fn non_indexable_inlinks_export_includes_list_inputs_at_depth_zero() {
    let dir = tempfile::tempdir().unwrap();
    let f = fixture(dir.path());
    exec(
        &f,
        "UPDATE urls SET discovered_via='list_input', crawl_depth=0;
        UPDATE crawl_meta SET value='' WHERE key='start_url';
        UPDATE crawl_meta SET value='list' WHERE key='mode'",
    );
    let cfg = CrawlConfig::default();
    os_analyze::analysis::runner::run_only(
        &f,
        &cfg,
        &os_model::CancelToken::new(),
        &|_, _| {},
        &[os_model::AnalysisStep::Links],
    )
    .unwrap();
    let rows = export(&f, &cfg, "Non-Indexable Page Inlinks Only", dir.path());
    assert_eq!(rows.len(), 5);
    let destinations: std::collections::BTreeSet<_> =
        rows.iter().skip(1).map(|row| row[2].as_str()).collect();
    assert_eq!(
        destinations,
        ["https://site.test/5", "https://site.test/6"].into()
    );
}

// spec: OUT-031 [P1] [M2]
#[test]
fn raw_mailto_and_tel_targets_survive_the_writebatch_contract() {
    let dir = tempfile::tempdir().unwrap();
    let created = new_file(dir.path(), "raw.ospider");
    let path = created.path().to_owned();
    drop(created);
    let mut sink = SqliteSink::open(&path, StorageProfile::Balanced).unwrap();
    let mut links = Vec::new();
    for (i, raw) in ["mailto:hello@site.test", "tel:+493012345"]
        .iter()
        .enumerate()
    {
        let mut link = writer_support::link(None, LinkType::Hyperlink, i as u32);
        link.target_raw = (*raw).into();
        link.crawlable = false;
        link.uncrawlable_reason = Some(UncrawlableReason::AnchorMissingHref);
        links.push(link);
    }
    sink.write(WriteBatch {
        new_urls: vec![writer_support::new_url(1, true)],
        pages: vec![writer_support::page(1, links)],
        ..WriteBatch::default()
    })
    .unwrap();
    drop(sink);
    let f = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let rows = export(&f, &CrawlConfig::default(), "All Inlinks", dir.path());
    assert_eq!(rows.len(), 3);
    assert_eq!(
        (&rows[1][2], &rows[2][2]),
        (&"mailto:hello@site.test".into(), &"tel:+493012345".into())
    );
}

// spec: OUT-008
// spec: OUT-068
#[test]
fn bulk_spec_aliases_preserve_selection_with_distinct_tab_and_bulk_file_names() {
    use os_export::catalog::{bulk_links, ExportKind, ExportRef};
    let dir = tempfile::tempdir().unwrap();
    let f = fixture(dir.path());
    let cfg = CrawlConfig::default();
    let mut alias_count = 0;
    for entry in bulk_links::entries()
        .iter()
        .filter(|e| matches!(e.selection, bulk_links::Selection::LinkSpec(_)))
    {
        for alias in &entry.aliases {
            alias_count += 1;
            assert_eq!(entry.name, format!("{alias} (Bulk)"));
            assert_eq!(
                os_export::catalog::resolve(ExportKind::Bulk, alias).unwrap(),
                ExportRef::Bulk(entry.name.clone())
            );
            let tab_name = os_export::naming::file_name(alias, None, "csv");
            let infos = export_bulk(&f, &cfg, alias, dir.path(), Format::Csv).unwrap();
            assert_ne!(
                infos[0].path.file_name().unwrap().to_str().unwrap(),
                tab_name
            );
            let canonical = export_bulk(&f, &cfg, &entry.name, dir.path(), Format::Csv).unwrap();
            assert_eq!(infos[0].path, canonical[0].path);
        }
    }
    assert_eq!(alias_count, 8);
}

// spec: OUT-041
#[test]
fn uncrawlable_export_uses_model_labels_for_every_known_reason_and_preserves_unknowns() {
    let dir = tempfile::tempdir().unwrap();
    let f = fixture(dir.path());
    let mut cfg = CrawlConfig::default();
    cfg.spider.store_uncrawlable_links = true;
    for reason in UncrawlableReason::ALL {
        exec(
            &f,
            &format!(
                "UPDATE links SET uncrawlable_reason='{}' WHERE source_id=8 AND crawlable=0",
                reason.as_str()
            ),
        );
        let rows = export(
            &f,
            &cfg,
            "Pages With Uncrawlable Internal Outlinks",
            dir.path(),
        );
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().skip(1).all(|r| r[15] == reason.label()));
    }
    exec(
        &f,
        "UPDATE links SET uncrawlable_reason='future_reason' WHERE source_id=8 AND crawlable=0",
    );
    let rows = export(
        &f,
        &cfg,
        "Pages With Uncrawlable Internal Outlinks",
        dir.path(),
    );
    assert!(rows.iter().skip(1).all(|r| r[15] == "future_reason"));
}

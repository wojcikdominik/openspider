use os_analyze::views::{query_view, state_of, ColumnSelection, ViewQuery};
use os_model::{AnalysisStep, CrawlConfig, FilterState, StepState, Value};
mod support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/tech-m2-canonicals/support.rs"
    ));
}
use support::*;
fn addresses(fx: &Fixture, filter: &str) -> Vec<String> {
    let mut query = ViewQuery::new("Canonicals", filter);
    query.columns = ColumnSelection::Named(vec!["Address".into()]);
    query.limit = 500;
    let mut values: Vec<_> = query_view(&fx.file, &fx.cfg, &query)
        .unwrap()
        .rows
        .into_iter()
        .map(|row| match &row[0] {
            Value::Text(s) => s.trim_start_matches("https://site.test/").to_string(),
            _ => panic!("Adresse"),
        })
        .collect();
    values.sort();
    values
}

// spec: TECH-011 [M2]
#[test]
fn population_includes_pdf_without_canonical_but_other_resources_need_http_canonical() {
    let mut fx = fixture();
    let all = addresses(&fx, "All");
    for name in ["plain.pdf", "header.pdf", "header.png"] {
        assert!(all.contains(&name.into()), "{name}");
    }
    for name in [
        "plain.png",
        "error",
        "external",
        "unknown",
        "respect-hidden",
    ] {
        assert!(!all.contains(&name.into()), "{name}");
    }
    assert!(addresses(&fx, "Missing").contains(&"plain.pdf".into()));
    fx.cfg.spider.canonicals.store = false;
    assert!(addresses(&fx, "All").is_empty());
    // Temporäre Datei lebt während aller Abfragen.
    assert!(fx.dir.path().exists());
}

// spec: TECH-019
#[test]
fn relative_scheme_grammar_checks_every_raw_value_including_http() {
    let fx = fixture();
    let values = [
        "/a",
        "a",
        "../a",
        "?p=2",
        "//site.test/a",
        "https://site.test/a",
        "MAILTO:a@b",
        "custom+v1:a",
        "1bad:a",
        "a_b:c",
        ":a",
        "",
    ];
    for (i, raw) in values.iter().enumerate() {
        fx.file.conn().execute("INSERT INTO canonicals(url_id,source,position,raw,in_head) VALUES (4,'http',?1,?2,1)", rusqlite::params![i as i64+1, raw]).unwrap();
        let expected = !(5..8).contains(&i);
        assert_eq!(
            addresses(&fx, "Canonical Is Relative").contains(&"c".into()),
            expected,
            "{raw}"
        );
        fx.file
            .conn()
            .execute("DELETE FROM canonicals WHERE url_id=4 AND position > 0", [])
            .unwrap();
    }
}

// spec: TECH-020
#[test]
fn unlinked_ignores_annotation_redirect_and_external_inlinks_but_counts_noindex_hyperlinks() {
    let mut fx = fixture();
    assert_eq!(
        state_of(&fx.file, &fx.cfg, "Canonicals", "Unlinked").unwrap(),
        FilterState::NeedsCrawlAnalysis
    );
    analyse(&fx);
    let unlinked = addresses(&fx, "Unlinked");
    for name in ["b", "c", "hidden", "external-only-target"] {
        assert!(unlinked.contains(&name.into()), "{name}");
    }
    for name in ["linked-source", "to-unknown", "not-followed"] {
        assert!(!unlinked.contains(&name.into()), "{name}");
    }
    let seq = os_store::queries::change_seq(fx.file.conn()).unwrap();
    analyse(&fx);
    assert_eq!(os_store::queries::change_seq(fx.file.conn()).unwrap(), seq);
    fx.file.conn().execute("INSERT INTO links(source_id,target_id,target_raw,link_type,position,follow) VALUES (1,13,'/hidden','Hyperlink',100,1)",[]).unwrap();
    analyse(&fx);
    assert!(!addresses(&fx, "Unlinked").contains(&"hidden".into()));
    fx.cfg.spider.canonicals.crawl = false;
    analyse(&fx);
    assert!(addresses(&fx, "Unlinked").contains(&"external-only-target".into()));
    assert!(!addresses(&fx, "Unlinked").contains(&"to-unknown".into()));
}

// spec: TECH-021
// spec: TECH-022
#[test]
fn live_attribute_and_fragment_filters_strip_the_display_fragment() {
    let fx = fixture();
    assert_eq!(
        addresses(&fx, "Invalid Attribute In Annotation"),
        ["annotation"]
    );
    assert_eq!(addresses(&fx, "Contains Fragment URL"), ["annotation"]);
    let mut q = ViewQuery::new("Canonicals", "Contains Fragment URL");
    q.columns = ColumnSelection::Named(vec!["Canonical Link Element".into()]);
    let result = query_view(&fx.file, &fx.cfg, &q).unwrap();
    assert_eq!(
        result.rows[0][0],
        Value::Text("https://site.test/hidden".into())
    );
}

// spec: TECH-023
// spec: TECH-008 [P1] [M2]
#[test]
fn outside_head_coverage_tracks_disabled_missing_and_partial_validation() {
    let mut fx = fixture();
    assert_eq!(addresses(&fx, "Outside <head>"), ["annotation"]);
    fx.cfg.advanced.html_validation = false;
    assert!(matches!(
        state_of(&fx.file, &fx.cfg, "Canonicals", "Outside <head>").unwrap(),
        FilterState::NeedsConfig(_)
    ));
    assert!(addresses(&fx, "Outside <head>").is_empty());
    fx.cfg.advanced.html_validation = true;
    fx.file
        .conn()
        .execute("DELETE FROM url_ext WHERE key='canonical.checked'", [])
        .unwrap();
    assert_eq!(
        state_of(&fx.file, &fx.cfg, "Canonicals", "Outside <head>").unwrap(),
        FilterState::NeedsRecrawl
    );
    fx.file
        .conn()
        .execute(
            "INSERT INTO url_ext(url_id,key,v_int) VALUES (12,'canonical.checked',1)",
            [],
        )
        .unwrap();
    assert!(
        matches!(state_of(&fx.file,&fx.cfg,"Canonicals","Outside <head>").unwrap(),FilterState::Incomplete { unchecked } if unchecked > 0)
    );
    assert_eq!(addresses(&fx, "Outside <head>"), ["annotation"]);
    fx.file
        .conn()
        .execute(
            "DELETE FROM url_ext WHERE key='canonical.invalid_attribute'",
            [],
        )
        .unwrap();
    // Andere Fakten und Indexierbarkeit werden durch den Validierungsschalter nicht beeinflusst.
    assert!(addresses(&fx, "Contains Fragment URL").contains(&"annotation".into()));
}

// spec: TECH-011 [M2]
#[test]
fn respect_hidden_contract_exposes_canonicalised_only_in_its_filter() {
    let fx = fixture();
    assert!(addresses(&fx, "Canonicalised").contains(&"respect-hidden".into()));
    for filter in ["All", "Contains Canonical", "Canonical Is Relative"] {
        assert!(!addresses(&fx, filter).contains(&"respect-hidden".into()));
    }
}

// spec: TECH-020
// spec: TECH-026
#[test]
fn paused_and_cancelled_analysis_does_not_publish_partial_flags() {
    let fx = fixture();
    let cancel = os_model::CancelToken::new();
    cancel.pause();
    let wake = cancel.clone();
    let thread = std::thread::spawn(move || {
        assert!(wake.checkpoint().is_err());
    });
    cancel.cancel();
    thread.join().unwrap();
    let outcome = os_analyze::analysis::run(&fx.file, &fx.cfg, &cancel, &|_, _| {}).unwrap();
    assert!(!outcome.complete);
    assert!(!outcome
        .steps
        .contains(&(AnalysisStep::Canonicals, StepState::Done)));
    let flags: i64 = fx.file.conn().query_row("SELECT COUNT(*) FROM url_flags WHERE flag IN ('unlinked_canonical','non_indexable_canonical')",[],|r|r.get(0)).unwrap();
    assert_eq!(flags, 0);
}

// spec: TECH-019
// spec: TECH-020
// spec: TECH-021
// spec: TECH-022
// spec: TECH-023
#[test]
fn issues_have_spec_classification_localized_text_and_matching_docs() {
    use os_analyze::issues::{self, Lang};
    use os_model::{IssuePriority, IssueType};
    for (slug, typ, priority) in [
        (
            "canonical-is-relative",
            IssueType::Warning,
            IssuePriority::Low,
        ),
        ("unlinked", IssueType::Warning, IssuePriority::Medium),
        (
            "invalid-attribute-in-annotation",
            IssueType::Issue,
            IssuePriority::High,
        ),
        (
            "contains-fragment-url",
            IssueType::Issue,
            IssuePriority::High,
        ),
        ("outside-head", IssueType::Issue, IssuePriority::High),
    ] {
        let id = format!("canonicals-{slug}");
        let entry = issues::catalog().iter().find(|e| e.id == id).unwrap();
        assert_eq!((entry.issue_type, entry.priority), (typ, priority));
        for lang in [Lang::De, Lang::En] {
            assert!(issues::texts(&entry.doc_key, lang, &[]).is_some());
        }
        let doc = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/issues")
                .join(format!("{id}.md")),
        )
        .unwrap();
        assert_eq!(doc, issues::doc_page(entry));
    }
}

// spec: TECH-021
// spec: TECH-011 [M2]
#[tokio::test]
async fn real_crawl_transports_html_and_pdf_header_attributes_to_live_filters() {
    let cfg = os_model::CrawlConfig::default();
    let crawl = os_engine::testing::crawl_fixture("tech-m2-canonicals", cfg.clone()).await;
    let mut q = ViewQuery::new("Canonicals", "Invalid Attribute In Annotation");
    q.columns = ColumnSelection::Named(vec!["Address".into()]);
    let page = query_view(&crawl.file, &cfg, &q).unwrap();
    let mut paths: Vec<_> = page
        .rows
        .iter()
        .map(|r| match &r[0] {
            Value::Text(s) => s.replace(&crawl.site.base_url(), ""),
            _ => panic!("Adresse"),
        })
        .collect();
    paths.sort();
    assert_eq!(paths, ["/header.pdf", "/source"]);
    q.filter = "Missing".into();
    let page = query_view(&crawl.file, &cfg, &q).unwrap();
    assert!(page
        .rows
        .iter()
        .any(|r| r[0] == Value::Text(format!("{}/plain.pdf", crawl.site.base_url()))));
}

// spec: TECH-023
// spec: TECH-008 [P1]
#[tokio::test]
async fn real_crawl_marks_canonical_coverage_only_when_validation_is_enabled() {
    for enabled in [false, true] {
        let mut cfg = CrawlConfig::default();
        cfg.advanced.html_validation = enabled;
        let crawl = os_engine::testing::crawl_fixture("tech-m2-canonicals", cfg.clone()).await;
        let mut q = ViewQuery::new("Canonicals", "Outside <head>");
        q.columns = ColumnSelection::Named(vec!["Address".into()]);
        let page = query_view(&crawl.file, &cfg, &q).unwrap();
        assert_eq!(
            page.rows,
            if enabled {
                vec![vec![Value::Text(format!("{}/body", crawl.site.base_url()))]]
            } else {
                vec![]
            }
        );
        let checked: i64 = crawl
            .file
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM url_ext WHERE key='canonical.checked'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(checked > 0, enabled);
        let count: i64 = crawl
            .file
            .conn()
            .query_row(
                "SELECT canonical_count FROM urls WHERE address=?1",
                [format!("{}/body", crawl.site.base_url())],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}

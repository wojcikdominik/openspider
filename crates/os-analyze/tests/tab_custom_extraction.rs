use os_analyze::{
    tabs,
    views::{self, ColumnSelection, ViewQuery},
};
use os_model::Value;
use os_store::{CrawlFile, OpenMode};
#[path = "../../../tests/fixtures/integ-custom-extraction/store.rs"]
mod fixture;

// spec: INTEG-049
#[test]
fn columns_are_crawl_wide_live_and_retained_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("extract.ospider");
    let file = fixture::create(&path);
    let cfg = file.config().unwrap();
    let mut q = ViewQuery::new("Custom Extraction", "All");
    q.limit = 1;
    q.start = views::PageStart::Offset(1);
    let page = views::query_view(&file, &cfg, &q).unwrap();
    assert_eq!(
        page.columns,
        [
            "Address",
            "Content",
            "Status Code",
            "Status",
            "h2 1",
            "h2 2",
            "h2 3",
            "Extractor 2 1",
            "Extraction Warnings"
        ]
    );
    assert_eq!(
        &page.rows[0][4..7],
        &[
            Value::Text("Only".into()),
            Value::Text("".into()),
            Value::Text("".into())
        ]
    );
    file.conn().execute_batch("INSERT INTO custom_extraction_values(url_id,def_index,number,value) VALUES(3,0,1,'B'),(3,0,2,'C'),(3,0,3,'D'),(3,0,4,'E');").unwrap();
    let live = views::query_view(&file, &cfg, &q).unwrap();
    assert!(live.columns.contains(&"h2 5".into()));
    drop(file);
    let reopened = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    assert_eq!(
        views::query_view(&reopened, &reopened.config().unwrap(), &q)
            .unwrap()
            .rows,
        live.rows
    );
    let mut internal = ViewQuery::new("Internal", "All");
    internal.columns = ColumnSelection::Named(vec!["Address".into(), "h2".into()]);
    assert!(views::query_view(&reopened, &cfg, &internal)
        .unwrap()
        .columns
        .contains(&"h2 5".into()));
}

// spec: INTEG-053
#[test]
fn extractor_filters_hide_other_columns_and_exclude_empty_values() {
    let dir = tempfile::tempdir().unwrap();
    let file = fixture::create(&dir.path().join("extract.ospider"));
    let cfg = file.config().unwrap();
    let q = ViewQuery::new("Custom Extraction", "h2");
    let page = views::query_view(&file, &cfg, &q).unwrap();
    assert_eq!(page.total, 2);
    assert!(!page.columns.contains(&"Extractor 2 1".into()));
    let all = views::query_view(&file, &cfg, &ViewQuery::new("Custom Extraction", "All")).unwrap();
    assert_eq!(all.total, 3); // 404, external and PDF are outside the population.
    let counts = views::filter_counts_in(&tabs::configured(&cfg), &file, &cfg).unwrap();
    assert!(counts
        .iter()
        .any(|c| c.tab == "Custom Extraction" && c.filter == "h2" && c.count == Some(2)));
    assert_eq!(
        os_analyze::names::resolve_in(&tabs::configured(&cfg), "Custom Extraction:Extractor 2")
            .unwrap()
            .1,
        "Extractor 2"
    );
}

// spec: INTEG-052
// spec: INTEG-053
// spec: AGENT-004 [M2]
#[tokio::test(start_paused = true)]
async fn sim_crawl_transports_values_and_warnings_without_extracting_external_or_error_pages() {
    use os_engine::{
        start_crawl,
        testing::{test_deps, SimFetcher, SimPage, SimSite},
        CrawlRequest, CrawlSource,
    };
    use os_model::{CrawlConfig, Extractor, ExtractorMode};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scope.ospider");
    let mut cfg = CrawlConfig::default();
    cfg.custom_extraction.extractors = vec![Extractor {
        mode: ExtractorMode::Css,
        expression: "h1, li, #untrusted".into(),
        ..Default::default()
    }];
    cfg.spider.external.crawl = true;
    let body = include_str!("../../../tests/fixtures/integ-custom-extraction/page.html");
    let site = SimSite::new()
        .page("https://extract.test/", "<a href='/target'>ok</a><a href='/error'>bad</a><a href='/redirect'>redirect</a><a href='/many'>many</a><a href='https://external.test/'>external</a><a href='/asset'>asset</a>")
        .page("https://extract.test/target", body)
        .page("https://extract.test/many", include_str!("../../../tests/fixtures/integ-custom-extraction/many-matches.html"))
        .with("https://extract.test/error", SimPage { status: 404, headers: vec![("content-type".into(), "text/html".into())], body: body.into(), ..Default::default() })
        .redirect("https://extract.test/redirect", 301, "/target")
        .page("https://external.test/", body)
        .asset("https://extract.test/asset", "text/plain", "<h1>Not HTML</h1>");
    let fetcher = SimFetcher::new(site);
    start_crawl(
        fetcher.clone(),
        CrawlRequest {
            config: cfg,
            source: CrawlSource::Spider {
                start_url: "https://extract.test/".into(),
            },
            file: path.clone(),
            project: "D15".into(),
            name: "scope".into(),
        },
        test_deps(),
    )
    .await
    .unwrap()
    .wait()
    .await
    .unwrap();
    let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let cfg = file.config().unwrap();
    let page = views::query_view(
        &file,
        &cfg,
        &ViewQuery::new("Custom Extraction", "Extractor 1"),
    )
    .unwrap();
    assert_eq!(page.total, 2);
    let mut statement = file.conn().prepare("SELECT DISTINCT u.address FROM custom_extraction_values e JOIN urls u ON u.id=e.url_id ORDER BY u.address").unwrap();
    let addresses: Vec<String> = statement
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        addresses,
        ["https://extract.test/many", "https://extract.test/target"]
    );
    let warning: String = file
        .conn()
        .query_row(
            "SELECT v_text FROM url_ext WHERE key='custom_extract.warnings'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(warning, "Extraction limit reached");
    assert!(!fetcher
        .page_requests()
        .iter()
        .any(|r| r.url.contains("/geheim")));
}

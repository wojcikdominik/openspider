//! Reale Loopback-Crawls sowie Store-Fixtures für den Prüfstatus (TECH-008).
mod framework_common;

use framework_common::{create, insert, row};
use os_analyze::{
    tabs,
    views::{self, ColumnSelection, ViewQuery},
};
use os_engine::testing::{crawl_fixture_from, FixtureCrawl};
use os_model::{CrawlConfig, FilterState, Value};

fn config(enabled: bool) -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.advanced.html_validation = enabled;
    cfg.limits.max_page_kb = 8192;
    cfg
}

async fn crawl(enabled: bool) -> FixtureCrawl {
    crawl_fixture_from("tech-m2-validation", "http://site.test/", config(enabled)).await
}

fn view(file: &os_store::CrawlFile, cfg: &CrawlConfig, filter: &str) -> views::ViewPage {
    let mut q = ViewQuery::new("Validation", filter);
    q.columns = ColumnSelection::All;
    q.limit = 500;
    q.sort = vec![os_analyze::SortKey {
        column: "Address".into(),
        descending: false,
    }];
    views::query_view(file, cfg, &q).unwrap()
}

fn paths(file: &os_store::CrawlFile, cfg: &CrawlConfig, filter: &str) -> Vec<String> {
    view(file, cfg, filter)
        .rows
        .iter()
        .map(|r| match &r[0] {
            Value::Text(s) => s.strip_prefix("http://site.test").unwrap_or(s).to_owned(),
            v => panic!("Address: {v:?}"),
        })
        .collect()
}

// spec: TECH-281
#[tokio::test]
async fn population_columns_and_live_filters_match_source_fixture() {
    let fx = crawl(true).await;
    let cfg = config(true);
    let all = view(&fx.file, &cfg, "All");
    assert_eq!(
        all.columns,
        [
            "Address",
            "Content Type",
            "Status Code",
            "Status",
            "Indexability",
            "Indexability Status"
        ]
    );
    assert_eq!(all.total, 34);
    for included in [
        "/created.html",
        "/error.html",
        "/big.png",
        "/big.js",
        "/big.css",
    ] {
        assert!(
            paths(&fx.file, &cfg, "All").contains(&included.to_owned()),
            "{included}"
        );
    }
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/tech-m2-validation/expectations.json"
    ))
    .unwrap();
    for (code, filter) in [
        ("invalid-head-element", "Invalid HTML Elements In <head>"),
        ("head-not-first", "<head> Not First In <html> Element"),
        ("missing-head", "Missing <head> Tag"),
        ("multiple-head", "Multiple <head> Tags"),
        ("missing-body", "Missing <body> Tag"),
        ("multiple-body", "Multiple <body> Tags"),
        ("body-before-html", "<body> Element Preceding <html>"),
    ] {
        let mut expected: Vec<_> = golden
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["codes"].as_array().unwrap().iter().any(|v| v == code))
            .map(|c| format!("/{}", c["file"].as_str().unwrap()))
            .collect();
        expected.sort();
        assert_eq!(paths(&fx.file, &cfg, filter), expected, "{filter}");
        assert_eq!(
            views::state_of(&fx.file, &cfg, "Validation", filter).unwrap(),
            FilterState::Ready,
            "resources are not unchecked HTML"
        );
    }
    assert!(tabs::validation::tab()
        .filters
        .iter()
        .all(|f| matches!(f.kind, os_analyze::FilterKind::Live)));
}

// spec: TECH-289
// spec: TECH-290
// spec: CRAWL-140 [P1] [M2]
#[tokio::test]
async fn decoded_size_thresholds_reclassify_without_another_request() {
    let fx = crawl(false).await;
    let mut cfg = config(false);
    let requests = fx.site.requests().len();
    assert_eq!(
        paths(&fx.file, &cfg, "HTML Document Over 2MB"),
        ["/big-2mb-plus.html"]
    );
    assert_eq!(
        paths(&fx.file, &cfg, "Resource Over 2MB"),
        ["/big.css", "/big.js"]
    );
    let bytes: i64 = fx.file.conn().query_row("SELECT e.v_int FROM url_ext e JOIN urls u ON u.id=e.url_id WHERE u.address='http://site.test/big-2mb-plus.html' AND e.key='validation.body_bytes'",[],|r|r.get(0)).unwrap();
    assert_eq!(bytes, 2_097_153);
    cfg.validation.html_doc_max_bytes = 3 * 1_048_576;
    cfg.validation.resource_max_bytes = 3 * 1_048_576;
    assert_eq!(view(&fx.file, &cfg, "HTML Document Over 3MB").total, 0);
    assert_eq!(view(&fx.file, &cfg, "Resource Over 3MB").total, 0);
    assert_eq!(fx.site.requests().len(), requests);
    let counts = views::filter_counts(&fx.file, &cfg).unwrap();
    assert!(counts.iter().any(|c| c.tab == "Validation"
        && c.label == "HTML Document Over 3MB"
        && c.count == Some(0)));
}

// spec: TECH-008 [M2]
// spec: CRAWL-138
#[tokio::test]
async fn disabled_then_enabled_validation_needs_recrawl_and_keeps_metadata() {
    let fx = crawl(false).await;
    let cfg = config(false);
    let n: u64 = fx.file.conn().query_row("SELECT COUNT(*) FROM url_ext WHERE key LIKE 'validation.%' AND key NOT IN ('validation.body_bytes','validation.truncated')",[],|r|r.get(0)).unwrap();
    assert_eq!(n, 0, "no token data collected while disabled");
    let state = views::state_of(&fx.file, &cfg, "Validation", "Missing <head> Tag").unwrap();
    assert!(matches!(state, FilterState::NeedsConfig(_)));
    let counts = views::filter_counts(&fx.file, &cfg).unwrap();
    assert!(counts
        .iter()
        .any(|c| c.tab == "Validation" && c.filter == "Missing <head> Tag" && c.count.is_none()));
    assert_eq!(
        views::state_of(&fx.file, &config(true), "Validation", "Missing <head> Tag").unwrap(),
        FilterState::NeedsRecrawl
    );
    assert_eq!(view(&fx.file, &config(true), "Missing <head> Tag").total, 0);
    assert_eq!(view(&fx.file, &cfg, "All").total, 34);
    let on = crawl(true).await;
    let metadata = |file: &os_store::CrawlFile| {
        file.conn().prepare("SELECT address,title_1,effective_index,effective_follow,canonical_count,hreflang_count,indexability,indexability_status FROM urls WHERE content_class='HTML' ORDER BY address").unwrap()
            .query_map([],|r| Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<i64>>(2)?,r.get::<_,Option<i64>>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?,r.get::<_,String>(6)?,r.get::<_,Option<String>>(7)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap()
    };
    assert_eq!(metadata(&fx.file), metadata(&on.file));
}

// spec: TECH-008
#[test]
fn partial_coverage_counts_only_html_and_does_not_invent_missing_tags() {
    let dir = tempfile::tempdir().unwrap();
    let (_, file) = create(dir.path(), "coverage.ospider", 42);
    insert(
        &file,
        &[
            row(1, "/checked")
                .text("content_class", "HTML")
                .int("status_code", 200),
            row(2, "/unchecked")
                .text("content_class", "HTML")
                .int("status_code", 200),
            row(3, "/script")
                .text("content_class", "JavaScript")
                .int("status_code", 200),
            row(4, "/external")
                .external()
                .text("content_class", "HTML")
                .int("status_code", 200),
            row(5, "/created")
                .text("content_class", "HTML")
                .int("status_code", 201),
        ],
    );
    file.conn().execute_batch("INSERT INTO url_ext(url_id,key,v_int,change_seq) VALUES (1,'validation.checked',1,1),(1,'validation.head_count',0,1)").unwrap();
    let cfg = config(true);
    assert_eq!(
        views::state_of(&file, &cfg, "Validation", "Missing <head> Tag").unwrap(),
        FilterState::Incomplete { unchecked: 1 }
    );
    let page = view(&file, &cfg, "Missing <head> Tag");
    assert_eq!(page.row_keys, [os_model::UrlId(1)]);
    file.conn().execute_batch("INSERT INTO url_ext(url_id,key,v_int,change_seq) VALUES (2,'validation.checked',1,2),(2,'validation.head_count',1,2)").unwrap();
    assert_eq!(
        views::state_of(&file, &cfg, "Validation", "Missing <head> Tag").unwrap(),
        FilterState::Ready
    );
    assert_eq!(view(&file, &cfg, "Missing <head> Tag").total, 1);
}

// spec: TECH-289
// spec: TECH-290
#[test]
fn truncated_downloads_remain_listed_even_below_the_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let (_, file) = create(dir.path(), "truncated.ospider", 42);
    insert(
        &file,
        &[
            row(1, "/truncated")
                .text("content_class", "HTML")
                .int("status_code", 200),
            row(2, "/truncated.css")
                .text("content_class", "CSS")
                .int("status_code", 200),
            row(3, "/font")
                .text("content_class", "Fonts")
                .int("status_code", 200),
            row(4, "/external.js")
                .external()
                .text("content_class", "JavaScript")
                .int("status_code", 200),
            row(5, "/error.css")
                .text("content_class", "CSS")
                .int("status_code", 404),
        ],
    );
    for id in 1..=5 {
        file.conn().execute("INSERT INTO url_ext(url_id,key,v_int,change_seq) VALUES (?1,'validation.truncated',1,1)",[id]).unwrap();
    }
    assert_eq!(
        view(&file, &config(false), "HTML Document Over 2MB").row_keys,
        [os_model::UrlId(1)]
    );
    assert_eq!(
        view(&file, &config(false), "Resource Over 2MB").row_keys,
        [os_model::UrlId(5), os_model::UrlId(2)]
    );
}

// spec: TECH-008
// spec: CRAWL-138
#[tokio::test]
async fn enabling_in_a_pause_checks_only_later_pages_and_recrawl_replaces_markers() {
    use os_engine::testing::{test_deps, SimFetcher, SimSite};
    use os_engine::{resume_crawl, start_crawl, CrawlRequest, CrawlSource};
    use os_store::{CrawlFile, OpenMode};
    use std::time::Duration;
    let root = include_str!("../../../tests/fixtures/tech-m2-validation/header-element.html");
    let missing = include_str!("../../../tests/fixtures/tech-m2-validation/no-head.html");
    let links = (0..40)
        .map(|i| format!("<a href='/p{i}'>page</a>"))
        .collect::<String>();
    let mut site = SimSite::new()
        .page("https://site.test/", &format!("{root}{links}"))
        .delay(Duration::from_millis(10));
    for i in 0..40 {
        site = site.page(&format!("https://site.test/p{i}"), missing);
    }
    let fetcher = SimFetcher::new(site);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pause.ospider");
    let mut cfg = config(false);
    cfg.speed.threads = 1;
    let handle = start_crawl(
        fetcher.clone(),
        CrawlRequest {
            config: cfg.clone(),
            source: CrawlSource::Spider {
                start_url: "https://site.test/".into(),
            },
            file: path.clone(),
            project: "validation".into(),
            name: "pause".into(),
        },
        test_deps(),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while handle.progress().crawled < 2 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    handle.pause();
    let summary = handle.wait().await.unwrap();
    assert_eq!(summary.state, os_model::CrawlState::Paused);
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let unchecked: u64 = file
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE content_class='HTML' AND status_code=200",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!((2..41).contains(&unchecked));
    cfg.advanced.html_validation = true;
    drop(file);
    let resumed = resume_crawl(
        fetcher.clone(),
        path.clone(),
        Some(cfg.clone()),
        test_deps(),
    )
    .await
    .unwrap()
    .wait()
    .await
    .unwrap();
    assert_eq!(resumed.state, os_model::CrawlState::Completed);
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    assert_eq!(
        views::state_of(&file, &cfg, "Validation", "Missing <head> Tag").unwrap(),
        FilterState::Incomplete { unchecked }
    );
    let ids:Vec<os_model::UrlId>=file.conn().prepare("SELECT id FROM urls WHERE content_class='HTML' AND status_code=200 AND NOT EXISTS(SELECT 1 FROM url_ext e WHERE e.url_id=urls.id AND e.key='validation.checked')").unwrap()
        .query_map([],|r|r.get::<_,i64>(0).map(os_model::UrlId)).unwrap().collect::<Result<_,_>>().unwrap();
    drop(file);
    os_engine::rescan::recrawl_urls(fetcher.clone(), &path, &ids, test_deps())
        .await
        .unwrap();
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    assert_eq!(
        views::state_of(&file, &cfg, "Validation", "Missing <head> Tag").unwrap(),
        FilterState::Ready
    );
    assert_eq!(view(&file, &cfg, "Missing <head> Tag").total, 40);
    cfg.advanced.html_validation = false;
    file.set_config(&cfg).unwrap();
    let all: Vec<os_model::UrlId> = file
        .conn()
        .prepare("SELECT id FROM urls WHERE content_class='HTML' AND status_code=200")
        .unwrap()
        .query_map([], |r| r.get::<_, i64>(0).map(os_model::UrlId))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    drop(file);
    os_engine::rescan::recrawl_urls(fetcher, &path, &all, test_deps())
        .await
        .unwrap();
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let raw:u64=file.conn().query_row("SELECT COUNT(*) FROM url_ext WHERE key LIKE 'validation.%' AND key NOT IN ('validation.body_bytes','validation.truncated')",[],|r|r.get(0)).unwrap();
    assert_eq!(
        raw, 0,
        "recrawl while disabled removes previous token facts"
    );
    cfg.advanced.html_validation = true;
    assert_eq!(
        views::state_of(&file, &cfg, "Validation", "Missing <head> Tag").unwrap(),
        FilterState::NeedsRecrawl
    );
}

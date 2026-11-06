use os_engine::{
    testing::{crawl_fixture, test_deps, SimFetcher, SimSite},
    CrawlRequest, CrawlSource,
};
use os_model::{CrawlConfig, CrawlState};
use std::{sync::Arc, time::Duration};

// spec: CRAWL-131 [M2]
// spec: CRAWL-131 [P1], CRAWL-128
#[tokio::test]
async fn completed_crawl_accepts_only_new_valid_in_scope_urls_then_resumes() {
    let mut fx = crawl_fixture("crawl-m2-lifecycle", CrawlConfig::default()).await;
    let base = fx.site.url("");
    let mut cfg = fx.file.config().unwrap();
    cfg.scope.exclude = vec![".*/excluded".into()];
    let inputs = vec![
        format!("{base}/added"),
        format!("{base}/added#fragment"),
        format!("{base}/first"),
        "bad url".into(),
        "ftp://site.test/a".into(),
        "https://other.test/x".into(),
        format!("{base}/excluded"),
    ];
    let summary = os_engine::lifecycle::add_urls_to_file(&mut fx.file, &inputs, &cfg).unwrap();
    assert_eq!(
        (
            summary.added,
            summary.duplicates,
            summary.invalid,
            summary.excluded
        ),
        (1, 2, 2, 2)
    );
    assert_eq!(
        fx.file.get_meta("analysis_stale").unwrap().as_deref(),
        Some("1")
    );
    let cfg = fx.file.config().unwrap();
    let deps = test_deps();
    let fetcher = os_fetch::HttpFetcher::new(
        os_fetch::FetchConfig::from_crawl_config(&cfg, &*deps.secrets, deps.clock.clone()).unwrap(),
    )
    .unwrap();
    let completed =
        os_engine::resume_crawl(Arc::new(fetcher), fx.file.path().to_path_buf(), None, deps)
            .await
            .unwrap()
            .wait()
            .await
            .unwrap();
    assert_eq!(completed.state, CrawlState::Completed);
    let status: i64 = fx
        .file
        .conn()
        .query_row(
            "SELECT status_code FROM urls WHERE url=?1",
            [format!("{base}/added")],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, 200);
}

// spec: CRAWL-131 [P1] [M2]
#[tokio::test]
async fn running_crawl_uses_the_same_deduplicating_admission() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = SimFetcher::new(
        SimSite::new()
            .page("https://site.test/", "<html>start</html>")
            .page("https://site.test/added", "<html>added</html>")
            .delay(Duration::from_millis(150)),
    );
    let handle = os_engine::start_crawl(
        fetcher,
        CrawlRequest {
            config: CrawlConfig::default(),
            source: CrawlSource::Spider {
                start_url: "https://site.test/".into(),
            },
            file: dir.path().join("crawl.ospider"),
            project: "fixture".into(),
            name: "added".into(),
        },
        test_deps(),
    )
    .await
    .unwrap();
    let result = os_engine::add_urls(
        &handle,
        &[
            "https://site.test/added".into(),
            "https://site.test/added".into(),
        ],
    )
    .await
    .unwrap();
    assert_eq!((result.added, result.duplicates), (1, 1));
    assert_eq!(handle.wait().await.unwrap().crawled, 2);
}

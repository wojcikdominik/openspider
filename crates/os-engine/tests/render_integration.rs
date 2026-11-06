use futures_util::future::BoxFuture;
use os_engine::{
    testing::{crawl_fixture_with, test_deps, FixtureCrawl},
    Renderer,
};
use os_model::{CrawlConfig, RenderingMode};
use os_render::{RenderError, RenderOpts, RenderedPage};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

const RAW: &str = include_str!("../../../tests/fixtures/crawl-m2-render/raw.html");
const DOM: &str = include_str!("../../../tests/fixtures/crawl-m2-render/dom.html");

#[derive(Default)]
struct Mock {
    dom: Option<&'static str>,
    calls: Mutex<Vec<(String, RenderOpts)>>,
    failures: AtomicUsize,
    active: AtomicUsize,
    peak: AtomicUsize,
}
impl Renderer for Mock {
    fn render<'a>(
        &'a self,
        url: &'a str,
        opts: &'a RenderOpts,
    ) -> BoxFuture<'a, Result<RenderedPage, RenderError>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push((url.into(), opts.clone()));
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            if self
                .failures
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err(RenderError::Crashed("fixture".into()));
            }
            Ok(RenderedPage {
                dom_html: self.dom.unwrap_or(DOM).into(),
                navigation_url: Some(url.into()),
                network: vec![
                    os_model::NetworkEntry {
                        url: url.into(),
                        status: 200,
                        resource_type: "Document".into(),
                        bytes: 999,
                        duration_ms: None,
                    },
                    os_model::NetworkEntry {
                        url: format!("{url}asset"),
                        status: 200,
                        resource_type: "Image".into(),
                        bytes: 20000,
                        duration_ms: None,
                    },
                    os_model::NetworkEntry {
                        url: format!("{url}asset"),
                        status: 200,
                        resource_type: "Image".into(),
                        bytes: 20000,
                        duration_ms: None,
                    },
                ],
                ..Default::default()
            })
        })
    }
}
fn cfg() -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.rendering.mode = RenderingMode::JavaScript;
    cfg.rendering.ajax_timeout_s = 0.1;
    cfg.rendering.screenshots.enabled = false;
    cfg.extraction.store_html = true;
    cfg.extraction.store_rendered_html = true;
    cfg
}
fn int(fx: &FixtureCrawl, sql: &str) -> i64 {
    fx.file.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}
fn text(fx: &FixtureCrawl, sql: &str) -> String {
    fx.file.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}
fn search(cfg: &mut CrawlConfig) {
    cfg.custom_search.filters = ["Injected-Marker", "Removed-Marker"]
        .into_iter()
        .map(|expression| os_model::SearchFilter {
            expression: expression.into(),
            scope: os_model::SearchScope::PageText,
            ..Default::default()
        })
        .collect();
    cfg.custom_extraction.extractors = vec![os_model::Extractor {
        mode: os_model::ExtractorMode::Css,
        expression: "#late".into(),
        ..Default::default()
    }];
}

// spec: CRAWL-078
// spec: CRAWL-079
// spec: TECH-006
// spec: TECH-100
// spec: INTEG-030
// spec: INTEG-052
// spec: CRAWL-022 [P1] [M2]
// spec: ONPAGE-012 [P1] [M2]
// spec: ONPAGE-016
#[tokio::test]
async fn raw_and_dom_are_compared_merged_and_persisted_without_double_counting() {
    let renderer = Arc::new(Mock::default());
    let mut deps = test_deps();
    deps.renderer = Some(renderer.clone());
    let mut config = cfg();
    config.limits.max_depth = Some(0);
    search(&mut config);
    let fx = crawl_fixture_with("crawl-m2-render", "/", config, deps).await;
    assert_eq!(
        text(
            &fx,
            "SELECT title_1 FROM urls WHERE url LIKE '%/' AND in_tables=1"
        ),
        "DOM & Title"
    );
    assert_eq!(
        text(
            &fx,
            "SELECT indexability FROM urls WHERE in_tables=1 AND url LIKE '%/'"
        ),
        "Non-Indexable"
    );
    assert_eq!(
        text(
            &fx,
            "SELECT canonical_url FROM urls WHERE in_tables=1 AND url LIKE '%/'"
        ),
        format!("{}/canonical", fx.site.url("").trim_end_matches('/'))
    );
    assert_eq!(int(&fx,"SELECT COUNT(*) FROM links WHERE link_type='Hyperlink' AND link_origin='HTML & Rendered HTML'"),1);
    assert_eq!(
        int(
            &fx,
            "SELECT COUNT(*) FROM links WHERE link_type='Hyperlink' AND link_origin='HTML'"
        ),
        1
    );
    assert_eq!(int(&fx,"SELECT COUNT(DISTINCT target_id) FROM links l JOIN urls t ON t.id=l.target_id WHERE l.link_type='Hyperlink' AND l.link_origin='Rendered HTML' AND t.is_internal=1"),1);
    assert_eq!(int(&fx,"SELECT COUNT(DISTINCT target_id) FROM links l JOIN urls t ON t.id=l.target_id WHERE l.link_type='Hyperlink' AND l.link_origin='Rendered HTML' AND t.is_internal=0"),1);
    assert_eq!(int(&fx,"SELECT total_transferred_bytes-transferred_bytes FROM urls WHERE in_tables=1 AND url LIKE '%/'"),20000);
    assert_eq!(
        int(
            &fx,
            "SELECT count FROM custom_search_hits WHERE def_index=0"
        ),
        1
    );
    assert_eq!(
        int(
            &fx,
            "SELECT count FROM custom_search_hits WHERE def_index=1"
        ),
        0
    );
    assert_eq!(int(&fx, "SELECT COUNT(*) FROM custom_extraction_values"), 1);
    assert!(text(
        &fx,
        "SELECT v_text FROM url_ext WHERE key='render.raw_title'"
    )
    .contains("Raw"));
    assert_eq!(
        text(
            &fx,
            "SELECT v_text FROM url_ext WHERE key='render.rendered_title'"
        ),
        "DOM & Title"
    );
    assert_eq!(
        int(
            &fx,
            "SELECT COUNT(*) FROM render_pages WHERE html IS NOT NULL"
        ),
        1
    );
    let calls = renderer.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0]
        .1
        .main_response
        .as_ref()
        .unwrap()
        .body
        .windows(5)
        .any(|w| w == b"Raw</"));
    assert!(calls[0].1.resource_policy.is_some());
}

// spec: CRAWL-078
// spec: PERF-006
#[tokio::test]
async fn crash_is_retried_once_and_failure_preserves_raw_data() {
    for (failures, expected_status, expected_title) in
        [(1, "ok", "DOM & Title"), (2, "crashed", "Raw")]
    {
        let renderer = Arc::new(Mock {
            failures: AtomicUsize::new(failures),
            ..Default::default()
        });
        let mut deps = test_deps();
        deps.renderer = Some(renderer.clone());
        let mut config = cfg();
        config.limits.max_depth = Some(0);
        let fx = crawl_fixture_with("crawl-m2-render", "/", config, deps).await;
        assert_eq!(renderer.calls.lock().unwrap().len(), 2);
        assert_eq!(
            text(&fx, "SELECT status FROM render_pages"),
            expected_status
        );
        assert_eq!(
            text(
                &fx,
                "SELECT title_1 FROM urls WHERE in_tables=1 AND url LIKE '%/'"
            ),
            expected_title
        );
        if failures == 2 {
            assert_eq!(
                text(
                    &fx,
                    "SELECT status FROM urls WHERE in_tables=1 AND url LIKE '%/'"
                ),
                "Rendering fehlgeschlagen"
            );
        }
    }
}

// spec: CRAWL-077 [P1] [M2]
#[tokio::test]
async fn text_mode_never_calls_renderer_and_missing_chromium_fails_before_creation() {
    let renderer = Arc::new(Mock::default());
    let mut deps = test_deps();
    deps.renderer = Some(renderer.clone());
    let mut config = CrawlConfig::default();
    config.limits.max_depth = Some(0);
    search(&mut config);
    let fx = crawl_fixture_with("crawl-m2-render", "/", config, deps).await;
    assert!(renderer.calls.lock().unwrap().is_empty());
    assert_eq!(
        int(
            &fx,
            "SELECT count FROM custom_search_hits WHERE def_index=0"
        ),
        0
    );
    assert_eq!(
        int(
            &fx,
            "SELECT count FROM custom_search_hits WHERE def_index=1"
        ),
        1
    );
    assert_eq!(int(&fx, "SELECT COUNT(*) FROM render_pages"), 0);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("missing.ospider");
    let mut config = cfg();
    config.rendering.chromium_path =
        Some(dir.path().join("no-browser").to_string_lossy().into_owned());
    let result = os_engine::start_crawl(
        os_engine::testing::SimFetcher::new(os_engine::testing::SimSite::new()),
        os_engine::CrawlRequest {
            config,
            source: os_engine::CrawlSource::Spider {
                start_url: "https://site.test/".into(),
            },
            file: file.clone(),
            project: "test".into(),
            name: "missing".into(),
        },
        test_deps(),
    )
    .await;
    let error = result.err().expect("missing browser").to_string();
    assert!(error.contains("err-chromium-missing"));
    assert!(error.contains("OPENSPIDER_CHROMIUM"));
    assert!(!file.exists());
}

// spec: PERF-006
// spec: CRAWL-078
#[test]
fn tabs_obey_k2_36_limits() {
    let mut c = cfg();
    c.speed.threads = 16;
    assert_eq!(
        os_engine::render_glue::tab_limit(&c, 8, 900 * 1024 * 1024),
        3
    );
    c.rendering.tabs = Some(2);
    assert_eq!(
        os_engine::render_glue::tab_limit(&c, 8, 900 * 1024 * 1024),
        2
    );
    assert_eq!(os_engine::render_glue::tab_limit(&c, 8, 0), 1);
}

// spec: PERF-006
// spec: CRAWL-078
#[tokio::test]
async fn engine_caps_injected_renderer_concurrency_and_never_renders_external_pages() {
    let renderer = Arc::new(Mock::default());
    let mut deps = test_deps();
    deps.renderer = Some(renderer.clone());
    let mut config = cfg();
    config.speed.threads = 8;
    config.rendering.tabs = Some(2);
    config.limits.max_depth = Some(1);
    config.spider.external.crawl = true;
    let fx = crawl_fixture_with("crawl-m2-render", "/", config, deps).await;
    assert!(renderer.peak.load(Ordering::SeqCst) <= 2);
    assert!(renderer.calls.lock().unwrap().len() > 1);
    assert!(!renderer
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|(url, _)| url.contains("external.test")));
    assert!(
        int(
            &fx,
            "SELECT COUNT(*) FROM urls WHERE is_internal=0 AND status_code IS NOT NULL"
        ) > 0
    );
}

struct Hanging;
impl Renderer for Hanging {
    fn render<'a>(
        &'a self,
        _: &'a str,
        _: &'a RenderOpts,
    ) -> BoxFuture<'a, Result<RenderedPage, RenderError>> {
        Box::pin(std::future::pending())
    }
}
// spec: PERF-006
// spec: CRAWL-078
#[tokio::test]
async fn a_hanging_renderer_times_out_and_the_crawl_completes_with_raw_data() {
    let mut deps = test_deps();
    deps.renderer = Some(Arc::new(Hanging));
    let mut config = cfg();
    config.rendering.page_timeout_s = 1;
    config.limits.max_depth = Some(0);
    let fx = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        crawl_fixture_with("crawl-m2-render", "/", config, deps),
    )
    .await
    .unwrap();
    assert_eq!(text(&fx, "SELECT status FROM render_pages"), "timeout");
    assert_eq!(
        text(
            &fx,
            "SELECT title_1 FROM urls WHERE in_tables=1 AND url LIKE '%/'"
        ),
        "Raw"
    );
    assert_eq!(fx.summary.state, os_model::CrawlState::Completed);
}

// spec: CRAWL-077 [P1] [M2]
#[test]
fn a_started_crawl_cannot_change_its_rendering_mode() {
    let text = CrawlConfig::default();
    let js = cfg();
    for (old, new) in [(&text, &js), (&js, &text)] {
        let error = os_engine::lifecycle::check_config_change(old, new).unwrap_err();
        assert!(error.to_string().contains("Rendering-Modus"));
        assert!(error.to_string().contains("neuer Crawl"));
    }
}

// spec: CRAWL-063
// spec: CRAWL-082
// spec: CRAWL-084
#[tokio::test]
async fn disabled_retention_does_not_disable_rendering_or_comparison() {
    let renderer = Arc::new(Mock::default());
    let mut deps = test_deps();
    deps.renderer = Some(renderer.clone());
    let mut config = cfg();
    config.limits.max_depth = Some(0);
    config.extraction.store_html = false;
    config.extraction.store_rendered_html = false;
    config.rendering.js_error_reporting = false;
    let fx = crawl_fixture_with("crawl-m2-render", "/", config, deps).await;
    assert_eq!(
        text(
            &fx,
            "SELECT title_1 FROM urls WHERE in_tables=1 AND url LIKE '%/'"
        ),
        "DOM & Title"
    );
    assert_eq!(int(&fx, "SELECT COUNT(*) FROM page_html"), 0);
    assert_eq!(
        int(
            &fx,
            "SELECT COUNT(*) FROM render_pages WHERE html IS NOT NULL"
        ),
        0
    );
    assert_eq!(int(&fx, "SELECT COUNT(*) FROM screenshots"), 0);
    assert_eq!(int(&fx, "SELECT COUNT(*) FROM render_console"), 0);
    let calls = renderer.calls.lock().unwrap();
    assert!(!calls[0].1.screenshots);
    assert!(!calls[0].1.js_error_reporting);
    assert_eq!(
        text(
            &fx,
            "SELECT v_text FROM url_ext WHERE key='render.rendered_title'"
        ),
        "DOM & Title"
    );
}

// spec: TECH-100
// spec: TECH-006
#[tokio::test]
async fn comparison_normalizes_text_and_counts_the_configured_content_area() {
    let renderer = Arc::new(Mock::default());
    let mut deps = test_deps();
    deps.renderer = Some(renderer.clone());
    let mut config = cfg();
    config.limits.max_depth = Some(0);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("comparison.ospider");
    let site = os_engine::testing::SimSite::new().page("https://site.test/", RAW);
    os_engine::start_crawl(
        os_engine::testing::SimFetcher::new(site),
        os_engine::CrawlRequest {
            config,
            source: os_engine::CrawlSource::Spider {
                start_url: "https://site.test/".into(),
            },
            file: path.clone(),
            project: "test".into(),
            name: "comparison".into(),
        },
        deps,
    )
    .await
    .unwrap()
    .wait()
    .await
    .unwrap();
    let file = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    let title: String = file
        .conn()
        .query_row(
            "SELECT v_text FROM url_ext WHERE key='render.raw_title'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, "Raw & Title");
    for (key, count) in [
        // SQ-09: adjacent anchors each delimit words, also in render comparisons.
        ("render.raw_word_count", 7),
        ("render.rendered_word_count", 10),
    ] {
        let actual: i64 = file
            .conn()
            .query_row("SELECT v_int FROM url_ext WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(actual, count, "{key}");
    }
    assert_eq!(
        renderer.calls.lock().unwrap()[0]
            .1
            .main_response
            .as_ref()
            .unwrap()
            .body,
        RAW.as_bytes()
    );
}

// spec: CRAWL-079
// spec: TECH-006
#[tokio::test]
async fn a_refresh_inserted_into_the_dom_has_rendered_provenance() {
    let renderer = Arc::new(Mock {
        dom: Some(include_str!(
            "../../../tests/fixtures/crawl-m2-render/dom-refresh.html"
        )),
        ..Default::default()
    });
    let mut deps = test_deps();
    deps.renderer = Some(renderer);
    let mut config = cfg();
    config.limits.max_depth = Some(0);
    let fx = crawl_fixture_with("crawl-m2-render", "/", config, deps).await;
    assert_eq!(int(&fx,"SELECT COUNT(*) FROM links WHERE link_type='Meta Refresh' AND link_origin='Rendered HTML'"),1);
    assert_eq!(
        text(
            &fx,
            "SELECT redirect_type FROM urls WHERE in_tables=1 AND url LIKE '%/'"
        ),
        "meta_refresh"
    );
}

// spec: CRAWL-077 [P1]
#[tokio::test]
async fn existing_but_unlaunchable_browsers_fail_before_crawl_file_creation() {
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("invalid-browser");
    std::fs::write(&invalid, b"not a browser executable").unwrap();
    // The current test runner is executable, but cannot establish a Chromium CDP session.
    for (index, browser) in [invalid, std::env::current_exe().unwrap()]
        .into_iter()
        .enumerate()
    {
        let file = dir.path().join(format!("unlaunchable-{index}.ospider"));
        let mut config = cfg();
        config.rendering.chromium_path = Some(browser.to_string_lossy().into_owned());
        let started = std::time::Instant::now();
        let result = os_engine::start_crawl(
            os_engine::testing::SimFetcher::new(os_engine::testing::SimSite::new()),
            os_engine::CrawlRequest {
                config,
                source: os_engine::CrawlSource::Spider {
                    start_url: "https://site.test/".into(),
                },
                file: file.clone(),
                project: "test".into(),
                name: "unlaunchable".into(),
            },
            test_deps(),
        )
        .await;
        let error = result
            .err()
            .expect("unusable executable must fail the start check")
            .to_string();
        assert!(error.contains("Chromium nicht startbar"), "{error}");
        assert!(
            !file.exists(),
            "a failed browser start must not create a crawl file"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(25));
    }
}

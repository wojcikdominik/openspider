//! Engine-to-Chromium proof. These tests also run headless in linux-core.
use os_engine::testing::{crawl_fixture_with, test_deps, FixtureCrawl};
use os_model::{CrawlConfig, RenderingMode};

fn cfg() -> CrawlConfig {
    let mut c = CrawlConfig::default();
    c.rendering.mode = RenderingMode::JavaScript;
    // Integrationsrunde 8: 0,3 s reichten auf dem Windows-Runner nicht, bis die JavaScript-Abrufketten der Fixture-Seiten (Challenge,
    // Login-Ressourcen) abgeschlossen waren; die Renderer-Wartezeit nach dem Laden ist fest, mehr Zeit kostet nur Laufzeit.
    c.rendering.ajax_timeout_s = 1.5;
    c.rendering.screenshots.enabled = false;
    c.limits.max_depth = Some(0);
    c
}
fn int(fx: &FixtureCrawl, sql: &str) -> i64 {
    fx.file.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}
fn text(fx: &FixtureCrawl, sql: &str) -> String {
    fx.file.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}
fn assert_rendered(fx: &FixtureCrawl) {
    let mut stmt = fx
        .file
        .conn()
        .prepare("SELECT status FROM render_pages")
        .unwrap();
    let statuses: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(statuses, ["ok"], "crawl: {:?}", fx.summary);
}
static BROWSER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// spec: CRAWL-078
// spec: CRAWL-079
// spec: CRAWL-039 [M2]
// spec: CRAWL-022 [P1]
// spec: ONPAGE-012 [P1]
// spec: CRAWL-062
// spec: CRAWL-063
// spec: CRAWL-082
// spec: CRAWL-084
// spec: CRAWL-115
// spec: CRAWL-116
// spec: ONPAGE-172
// spec: OUT-273
#[tokio::test]
async fn headless_engine_reuses_response_and_stores_browser_facts() {
    let _guard = BROWSER.lock().await;
    let mut c = cfg();
    c.extraction.store_html = true;
    c.extraction.store_rendered_html = true;
    c.extraction.cookies = true;
    c.rendering.screenshots.enabled = true;
    c.http.headers.push(os_model::HeaderEntry {
        name: "X-Engine-Fixture".into(),
        value: "same-identity".into(),
        host: None,
    });
    c.http.accept_language = Some("de-DE,de;q=0.9".into());
    let fx = crawl_fixture_with("crawl-m2-render", "/", c, test_deps()).await;
    assert_rendered(&fx);
    assert_eq!(
        text(
            &fx,
            "SELECT title_1 FROM urls WHERE in_tables=1 AND url LIKE '%/'"
        ),
        "Rendered"
    );
    assert_eq!(
        text(
            &fx,
            "SELECT indexability FROM urls WHERE in_tables=1 AND url LIKE '%/'"
        ),
        "Non-Indexable"
    );
    let requests = fx.site.requests();
    assert_eq!(
        requests.iter().filter(|r| r.path_and_query == "/").count(),
        1,
        "main response reused"
    );
    assert!(!requests.iter().any(|r| r.path_and_query == "/blocked.js"));
    let image = requests
        .iter()
        .find(|r| r.path_and_query == "/lazy.png")
        .expect("lazy image loaded");
    let get = |name: &str| {
        image
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("X-Engine-Fixture"), Some("same-identity"));
    assert_eq!(get("Accept-Language"), Some("de-DE,de;q=0.9"));
    let main = requests.iter().find(|r| r.path_and_query == "/").unwrap();
    assert_eq!(
        get("User-Agent"),
        main.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("User-Agent"))
            .map(|(_, v)| v.as_str())
    );
    assert_eq!(
        int(
            &fx,
            "SELECT bytes FROM render_network WHERE url LIKE '%/lazy.png'"
        ),
        20000
    );
    assert_eq!(
        int(
            &fx,
            "SELECT COUNT(*) FROM render_blocked WHERE url LIKE '%/blocked.js' AND reason='robots'"
        ),
        1
    );
    assert!(int(&fx,"SELECT COUNT(*) FROM render_console WHERE text LIKE '%fixture-js-error%' AND level='error'")>=1);
    assert_eq!(
        int(
            &fx,
            "SELECT COUNT(*) FROM cookies WHERE name='js_session' AND origin='javascript'"
        ),
        1
    );
    assert_eq!(
        int(
            &fx,
            "SELECT COUNT(*) FROM render_pages WHERE status='ok' AND html IS NOT NULL"
        ),
        1
    );
    assert!(int(&fx, "SELECT COUNT(*) FROM screenshots") >= 1);
    assert_eq!(
        int(
            &fx,
            "SELECT COUNT(*) FROM image_instances WHERE real_w>0 AND display_w>0"
        ),
        1
    );
    let root = fx
        .file
        .conn()
        .query_row(
            "SELECT id FROM urls WHERE in_tables=1 AND url LIKE '%/'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap();
    let raw = os_store::queries::stored_html(&fx.file, os_model::UrlId(root))
        .unwrap()
        .unwrap();
    let dom = os_store::queries::stored_rendered_html(&fx.file, os_model::UrlId(root))
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&raw.bytes).contains("Removed-Marker"));
    let html = String::from_utf8_lossy(&dom.bytes);
    assert!(html.contains("Injected-Marker"));
    assert!(
        html.contains(r#"data-visible-cookies="js_session=active""#),
        "{html}"
    );
    assert_eq!(int(&fx, "SELECT COUNT(*) FROM cookies WHERE name='http_secret' AND http_only=1 AND origin='http'"),1);
}

// spec: CRAWL-016
#[tokio::test]
async fn navigation_is_a_redirect_but_pushstate_and_late_navigation_are_not() {
    let _guard = BROWSER.lock().await;
    for (path, redirects) in [
        ("/redirect", 1),
        ("/pushstate", 0),
        ("/late", 0),
        ("/meta-redirect", 0),
    ] {
        let fx = crawl_fixture_with("crawl-m2-render", path, cfg(), test_deps()).await;
        assert_rendered(&fx);
        assert_eq!(
            int(
                &fx,
                "SELECT COUNT(*) FROM urls WHERE redirect_type='javascript'"
            ),
            redirects,
            "{path}"
        );
        if path == "/meta-redirect" {
            assert_eq!(
                int(
                    &fx,
                    "SELECT COUNT(*) FROM urls WHERE redirect_type='meta_refresh'"
                ),
                1
            );
        }
        if redirects == 1 {
            assert_eq!(int(&fx,"SELECT COUNT(*) FROM links WHERE link_type='JavaScript Redirect' AND link_origin='Rendered HTML'"),1);
        }
    }
}

// spec: CRAWL-004
#[tokio::test]
async fn hash_routes_receive_separate_render_passes() {
    let _guard = BROWSER.lock().await;
    let mut c = cfg();
    c.advanced.crawl_fragments = true;
    c.limits.max_depth = Some(1);
    let fx = crawl_fixture_with("crawl-m2-render", "/fragment", c, test_deps()).await;
    assert_eq!(int(&fx, "SELECT COUNT(*) FROM render_pages"), 3);
    let mut stmt = fx
        .file
        .conn()
        .prepare("SELECT title_1 FROM urls WHERE url LIKE '%/fragment%' ORDER BY url")
        .unwrap();
    let titles = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(titles, vec!["none", "#a", "#b"]);
}

fn header<'a>(request: &'a os_testsite::RequestRecord, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

// spec: CRAWL-078
// spec: CRAWL-117
#[tokio::test]
async fn challenges_respect_the_actual_request_path() {
    let _guard = BROWSER.lock().await;
    let mut c = cfg();
    c.auth.basic.push(os_model::BasicAuth {
        prefix: "http://127.0.0.1/private/".into(),
        username: "fixture".into(),
        secret_ref: "fixture-password".into(),
    });
    let mut deps = test_deps();
    let secrets = std::sync::Arc::new(os_fetch::MemorySecretStore::new());
    secrets.set("fixture-password", "private-only");
    deps.secrets = secrets;
    let fx = crawl_fixture_with("crawl-m2-render", "/private/page", c, deps).await;
    assert_rendered(&fx);
    assert_eq!(
        text(
            &fx,
            "SELECT title_1 FROM urls WHERE url LIKE '%/private/page'"
        ),
        "private-authorized"
    );
    let requests = fx.site.requests();
    assert!(requests
        .iter()
        .any(|r| r.path_and_query == "/private/challenge" && header(r, "Authorization").is_some()));
    let public = requests
        .iter()
        .filter(|r| r.path_and_query == "/public/challenge")
        .collect::<Vec<_>>();
    assert!(
        !public.is_empty(),
        "must exercise an out-of-prefix challenge"
    );
    assert!(
        public.iter().all(|r| header(r, "Authorization").is_none()),
        "{public:?}"
    );
}

// spec: CRAWL-078
// spec: CRAWL-088
#[tokio::test]
async fn persistent_login_cookies_roundtrip_between_fetcher_and_browser() {
    let _guard = BROWSER.lock().await;
    let mut c = cfg();
    c.advanced.cookies = os_model::CookieMode::Persistent;
    c.speed.threads = 1;
    c.limits.max_depth = Some(1);
    let fx = crawl_fixture_with("crawl-m2-render", "/login", c, test_deps()).await;
    assert_eq!(
        int(&fx, "SELECT COUNT(*) FROM render_pages WHERE status='ok'"),
        2
    );
    let requests = fx.site.requests();
    let first = requests
        .iter()
        .find(|r| r.path_and_query == "/login-resource")
        .unwrap();
    assert!(header(first, "Cookie")
        .unwrap()
        .contains("login=authenticated"));
    for path in ["/login-next", "/login-check"] {
        let request = requests.iter().find(|r| r.path_and_query == path).unwrap();
        let cookies = header(request, "Cookie")
            .unwrap_or_else(|| panic!("login cookies shared for {path}: {requests:?}"));
        for expected in [
            "login=authenticated",
            "browser_http=active",
            "browser_js=active",
        ] {
            assert!(cookies.contains(expected), "{path}: {cookies}");
        }
        assert!(
            !cookies.contains("delete_me="),
            "browser deletion must reach HTTP fetcher: {cookies}"
        );
    }
}

// spec: CRAWL-078
// spec: CRAWL-088
#[tokio::test]
async fn cookie_none_blocks_javascript_and_resource_response_cookies() {
    let _guard = BROWSER.lock().await;
    let mut c = cfg();
    c.advanced.cookies = os_model::CookieMode::None;
    c.speed.threads = 1;
    c.limits.max_depth = Some(1);
    let fx = crawl_fixture_with("crawl-m2-render", "/login", c, test_deps()).await;
    assert_eq!(
        int(&fx, "SELECT COUNT(*) FROM render_pages WHERE status='ok'"),
        2
    );
    let requests = fx.site.requests();
    assert!(requests.iter().any(|r| r.path_and_query == "/login-check"));
    assert!(
        requests.iter().all(|r| header(r, "Cookie").is_none()),
        "{requests:?}"
    );
}

// spec: CRAWL-022 [P1] [M2]
// spec: ONPAGE-012 [P1]
// spec: CRAWL-039 [M2]
#[tokio::test]
async fn disabled_images_are_not_requested_or_counted_as_transferred() {
    let _guard = BROWSER.lock().await;
    let mut c = cfg();
    c.spider.images.crawl = false;
    let fx = crawl_fixture_with("crawl-m2-render", "/", c, test_deps()).await;
    assert_eq!(
        int(&fx, "SELECT COUNT(*) FROM render_pages WHERE status='ok'"),
        1
    );
    assert!(!fx
        .site
        .requests()
        .iter()
        .any(|r| r.path_and_query == "/lazy.png"));
    assert_eq!(
        int(
            &fx,
            "SELECT COUNT(*) FROM render_network WHERE url LIKE '%/lazy.png' AND bytes>0"
        ),
        0
    );
    assert_eq!(int(&fx, "SELECT total_transferred_bytes-transferred_bytes FROM urls WHERE in_tables=1 AND url LIKE '%/'"), 0);
    let main = fx
        .site
        .requests()
        .into_iter()
        .find(|r| r.path_and_query == "/")
        .unwrap();
    assert_eq!(
        int(
            &fx,
            "SELECT total_transferred_bytes FROM urls WHERE in_tables=1 AND url LIKE '%/'"
        ),
        main.sent_body_bytes as i64
    );
}

// spec: CRAWL-116
#[tokio::test]
async fn language_preferences_apply_before_scripts_and_default_header_is_absent() {
    let _guard = BROWSER.lock().await;
    for language in [Some("de-DE,de;q=0.9"), None] {
        let mut c = cfg();
        c.http.accept_language = language.map(str::to_owned);
        let fx = crawl_fixture_with("crawl-m2-render", "/language", c, test_deps()).await;
        assert_rendered(&fx);
        let requests = fx.site.requests();
        for path in ["/language", "/language-check"] {
            let request = requests.iter().find(|r| r.path_and_query == path).unwrap();
            assert_eq!(header(request, "Accept-Language"), language);
        }
        if language.is_some() {
            let title = text(&fx, "SELECT title_1 FROM urls WHERE url LIKE '%/language'");
            let values: serde_json::Value = serde_json::from_str(&title).unwrap();
            assert_eq!(
                values,
                serde_json::json!(["de-DE", ["de-DE", "de"], "de-DE"])
            );
        }
    }
}

// spec: CRAWL-062
#[tokio::test]
async fn analytics_hosts_follow_cookie_extraction_switch() {
    let _guard = BROWSER.lock().await;
    let hosts = [
        "google-analytics.com",
        "www.google-analytics.com",
        "googletagmanager.com",
        "analytics.google.com",
        "notgoogle-analytics.com",
    ];
    for capture in [false, true] {
        let site = os_testsite::TestSite::start("crawl-m2-render")
            .await
            .unwrap();
        let mut c = cfg();
        c.extraction.cookies = capture;
        c.spider.external.crawl = true;
        for host in hosts {
            c.network.resolve.push(format!("{host}={}", site.addr()));
        }
        let deps = test_deps();
        let fetcher = os_engine::testing::site_fetcher(&site, &c, &deps);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("analytics.ospider");
        let summary = os_engine::start_crawl(
            fetcher,
            os_engine::CrawlRequest {
                config: c,
                source: os_engine::CrawlSource::Spider {
                    start_url: site.url("/analytics"),
                },
                file: file.clone(),
                project: "test".into(),
                name: "analytics".into(),
            },
            deps,
        )
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
        let crawl = os_store::CrawlFile::open(&file, os_store::OpenMode::ReadWrite).unwrap();
        assert_eq!(
            crawl
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM render_pages WHERE status='ok'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1,
            "capture={capture}, crawl: {summary:?}"
        );
        let requests = site.requests();
        for host in hosts {
            assert_eq!(
                requests
                    .iter()
                    .any(|r| r.host == host && r.path_and_query == "/analytics.js"),
                capture || host == "notgoogle-analytics.com",
                "{host}, capture={capture}: {requests:?}"
            );
        }
    }
}

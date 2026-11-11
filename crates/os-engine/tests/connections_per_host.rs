//! CRAWL-151 Getrennte Verbindungen pro Host: Akzeptanzkriterien gegen den Fixture-Server, der TCP-Verbindungen
//! und gleichzeitige Anfragen je Verbindung zählt (`crawl-http2`, `crawl-site-100` mit `delay_ms`).

#[path = "lifecycle_support.rs"]
mod support;

use std::path::{Path, PathBuf};

use os_engine::testing::{site_fetcher, test_deps};
use os_engine::{resume_crawl, start_crawl, CrawlRequest, CrawlSource, CrawlSummary};
use os_model::{ConnectionsPerHost, CrawlConfig, CrawlState, HttpVersion};
use os_store::{CrawlFile, OpenMode};
use os_testsite::TestSite;

/// Kopie einer Fixture; `top` steht vor dem Inhalt ihrer `scenario.toml` (Schlüssel der obersten Ebene).
fn copy_fixture(name: &str, top: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    support::copy_dir(&os_testsite::fixtures_root().join(name), dir.path());
    let scenario = dir.path().join("scenario.toml");
    let old = std::fs::read_to_string(&scenario).unwrap_or_default();
    std::fs::write(&scenario, format!("{top}\n{old}")).unwrap();
    dir
}

fn https(site: &TestSite, host: &str, path: &str) -> String {
    format!("https://{host}:{}{path}", site.port())
}

fn http(site: &TestSite, host: &str, path: &str) -> String {
    format!("http://{host}:{}{path}", site.port())
}

/// Ohne eigene Auflösungsregel: `site_fetcher` ergänzt `*.test` → Testsite (eine zweite gleiche Regel ergäbe die
/// Adresse doppelt, und der Verbindungsaufbau versuchte nach einer Ablehnung die zweite).
fn config(_site: &TestSite, threads: u32, connections: ConnectionsPerHost) -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.speed.threads = threads;
    cfg.speed.connections_per_host = connections;
    cfg
}

fn spider(url: String) -> CrawlSource {
    CrawlSource::Spider { start_url: url }
}

struct Done {
    summary: CrawlSummary,
    path: PathBuf,
    _dir: tempfile::TempDir,
}

async fn crawl(site: &TestSite, cfg: CrawlConfig, source: CrawlSource) -> Done {
    let deps = test_deps();
    let fetcher = site_fetcher(site, &cfg, &deps);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c151.ospider");
    let handle = start_crawl(
        fetcher,
        CrawlRequest {
            config: cfg,
            source,
            file: path.clone(),
            project: "c151".into(),
            name: "c151".into(),
        },
        deps,
    )
    .await
    .unwrap();
    let summary = handle.wait().await.unwrap();
    Done {
        summary,
        path,
        _dir: dir,
    }
}

fn scalar(path: &Path, sql: &str) -> i64 {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .conn()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}

/// Verbindungen mit mindestens einem Request (eine Verbindung ohne Request zählt der Server ebenfalls mit).
fn used(site: &TestSite) -> Vec<os_testsite::ConnectionStat> {
    site.connection_stats()
        .into_iter()
        .filter(|c| c.requests > 0)
        .collect()
}

fn describe(site: &TestSite) -> String {
    format!(
        "{} Verbindungen, abgelehnt {}: {:?}",
        site.connections(),
        site.rejected_connections(),
        site.connection_stats()
    )
}

// spec: CRAWL-151
#[tokio::test]
async fn http2_five_threads_auto_opens_five_connections_and_never_more_than_five_requests() {
    let dir = support::slow_site_dir(200);
    let site = TestSite::start_dir(dir.path(), 0).await.unwrap();
    let cfg = config(&site, 5, ConnectionsPerHost::Auto);
    let done = crawl(&site, cfg, spider(https(&site, "site.test", "/"))).await;
    assert_eq!(done.summary.state, CrawlState::Completed);
    assert!(done.summary.crawled >= 100, "{}", done.summary.crawled);
    assert_eq!(site.connections(), 5, "{}", describe(&site));
    assert!(
        site.connection_stats().iter().all(|c| c.h2),
        "{}",
        describe(&site)
    );
    assert!(site.max_concurrency() <= 5, "{}", site.max_concurrency());
}

// spec: CRAWL-151
#[tokio::test]
async fn http2_twenty_threads_auto_uses_at_most_six_connections_with_at_most_four_requests_each() {
    let dir = support::slow_site_dir(200);
    let site = TestSite::start_dir(dir.path(), 0).await.unwrap();
    let cfg = config(&site, 20, ConnectionsPerHost::Auto);
    let done = crawl(&site, cfg, spider(https(&site, "site.test", "/"))).await;
    assert_eq!(done.summary.state, CrawlState::Completed);
    let conns = site.connections();
    assert!((2..=6).contains(&conns), "{}", describe(&site));
    for c in site.connection_stats() {
        assert!(c.max_concurrent <= 4, "{}", describe(&site));
    }
    assert!(
        site.max_concurrency() > 6,
        "20 Threads nutzen HTTP/2-Multiplexing: {}",
        site.max_concurrency()
    );
}

// spec: CRAWL-151
#[tokio::test]
async fn one_connection_per_host_under_http2_keeps_exactly_one_tcp_connection() {
    let dir = support::slow_site_dir(50);
    let site = TestSite::start_dir(dir.path(), 0).await.unwrap();
    let cfg = config(&site, 5, ConnectionsPerHost::Fixed(1));
    let done = crawl(&site, cfg, spider(https(&site, "site.test", "/"))).await;
    assert_eq!(done.summary.state, CrawlState::Completed);
    assert_eq!(site.connections(), 1, "{}", describe(&site));
    assert!(
        site.connection_stats()[0].max_concurrent >= 2,
        "{}",
        describe(&site)
    );
}

// spec: CRAWL-151
#[tokio::test]
async fn http1_ten_threads_two_connections_never_exceed_two_connections_or_two_requests() {
    let dir = support::slow_site_dir(50);
    let site = TestSite::start_dir(dir.path(), 0).await.unwrap();
    let mut cfg = config(&site, 10, ConnectionsPerHost::Fixed(2));
    cfg.http.version = HttpVersion::Http1;
    let done = crawl(&site, cfg, spider(http(&site, "site.test", "/"))).await;
    assert_eq!(done.summary.state, CrawlState::Completed);
    assert!(done.summary.crawled >= 100);
    assert!(site.connections() <= 2, "{}", describe(&site));
    assert!(site.max_concurrency() <= 2, "{}", site.max_concurrency());
    assert_eq!(site.max_concurrency_for_host("site.test"), 2);
}

// spec: CRAWL-151
// spec: CRAWL-011
#[tokio::test]
async fn one_thread_uses_at_most_two_connections_for_twenty_pages() {
    let site = TestSite::start("crawl-site-100").await.unwrap();
    for start in [
        http(&site, "site.test", "/"),
        https(&site, "site.test", "/"),
    ] {
        site.reset_request_log();
        let mut cfg = config(&site, 1, ConnectionsPerHost::Auto);
        cfg.limits.max_urls = 20;
        let done = crawl(&site, cfg, spider(start.clone())).await;
        assert!(done.summary.crawled >= 20, "{start}");
        assert!(site.connections() <= 2, "{start}: {}", describe(&site));
    }
}

// spec: CRAWL-151
#[tokio::test]
async fn two_hosts_in_one_crawl_have_separate_limits() {
    let site = TestSite::start("crawl-http2").await.unwrap();
    let mut items = Vec::new();
    for host in ["a.test", "b.test"] {
        items.push(https(&site, host, "/"));
        for n in 1..=40 {
            items.push(https(&site, host, &format!("/page-{n}.html")));
        }
    }
    let cfg = config(&site, 10, ConnectionsPerHost::Fixed(2));
    let done = crawl(
        &site,
        cfg,
        CrawlSource::List {
            items,
            label: "two hosts".into(),
        },
    )
    .await;
    assert_eq!(done.summary.state, CrawlState::Completed);
    for host in ["a.test", "b.test"] {
        let n = used(&site)
            .iter()
            .filter(|c| c.host.as_deref() == Some(host))
            .count();
        assert_eq!(n, 2, "{host}: {}", describe(&site));
    }
    assert_eq!(site.connections(), 4, "{}", describe(&site));
}

// spec: CRAWL-151
#[tokio::test]
async fn a_server_accepting_one_connection_per_client_is_crawled_completely_without_status_zero() {
    let dir = copy_fixture("crawl-http2", "max_connections_per_client = 1");
    let site = TestSite::start_dir(dir.path(), 0).await.unwrap();
    let cfg = config(&site, 5, ConnectionsPerHost::Auto);
    let done = crawl(&site, cfg, spider(https(&site, "site.test", "/"))).await;
    assert_eq!(done.summary.state, CrawlState::Completed);
    assert_eq!(
        scalar(
            &done.path,
            "SELECT COUNT(*) FROM urls WHERE status_code = 200 AND content_class = 'HTML'"
        ),
        41
    );
    assert_eq!(
        scalar(
            &done.path,
            "SELECT COUNT(*) FROM urls WHERE status_code = 0"
        ),
        0
    );
    // genau ein abgelehnter Versuch über die eine Verbindung hinaus, danach keiner mehr
    assert_eq!(site.rejected_connections(), 1, "{}", describe(&site));
    assert_eq!(site.connections(), 1, "{}", describe(&site));
}

fn results(path: &Path) -> Vec<(String, i64, Option<String>)> {
    let file = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    let mut stmt = file
        .conn()
        .prepare("SELECT url, status_code, hash_md5 FROM urls ORDER BY url")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

// spec: CRAWL-151
#[tokio::test]
async fn one_connection_and_auto_give_the_same_urls_statuses_and_contents() {
    let site = TestSite::start("crawl-site-100").await.unwrap();
    let start = https(&site, "site.test", "/");
    let one = crawl(
        &site,
        config(&site, 5, ConnectionsPerHost::Fixed(1)),
        spider(start.clone()),
    )
    .await;
    let auto = crawl(
        &site,
        config(&site, 5, ConnectionsPerHost::Auto),
        spider(start),
    )
    .await;
    let (one, auto) = (results(&one.path), results(&auto.path));
    assert!(one.len() > 100, "{}", one.len());
    assert_eq!(one, auto);
}

/// Millisekunden des Tages aus `RequestRecord::ts` (`…THH:MM:SS.mmmZ`).
fn ms_of_day(ts: &str) -> i64 {
    let t = &ts[11..23];
    let n = |r: std::ops::Range<usize>| t[r].parse::<i64>().unwrap();
    ((n(0..2) * 60 + n(3..5)) * 60 + n(6..8)) * 1000 + n(9..12)
}

// spec: CRAWL-151
// spec: CRAWL-113
#[tokio::test]
async fn rate_limit_stays_at_two_requests_per_second_with_auto_connections() {
    let site = TestSite::start("crawl-site-100").await.unwrap();
    let mut cfg = config(&site, 5, ConnectionsPerHost::Auto);
    cfg.speed.max_urls_per_sec = Some(2.0);
    cfg.limits.max_urls = 12;
    let done = crawl(&site, cfg, spider(https(&site, "site.test", "/"))).await;
    assert!(done.summary.crawled >= 12);
    let mut times: Vec<i64> = site.requests().iter().map(|r| ms_of_day(&r.ts)).collect();
    times.sort_unstable();
    assert!(times.len() >= 12, "{}", times.len());
    // höchstens zwei Anfragebeginne je Sekunde: der übernächste Beginn liegt mindestens 1 s später
    // (50 ms Spielraum für die Laufzeit bis zum Server und die Millisekunden-Auflösung des Logs)
    for w in times.windows(3) {
        assert!(
            w[2] - w[0] >= 950,
            "drei Anfragen in {} ms: {times:?}",
            w[2] - w[0]
        );
    }
}

// spec: CRAWL-151
// spec: CRAWL-126
#[tokio::test]
async fn the_value_changed_during_the_pause_applies_to_the_requests_after_resume() {
    let dir = support::slow_site_dir(20);
    let site = TestSite::start_dir(dir.path(), 0).await.unwrap();
    let cfg = config(&site, 5, ConnectionsPerHost::Auto);
    let deps = test_deps();
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("c151.ospider");
    let handle = start_crawl(
        site_fetcher(&site, &cfg, &deps),
        CrawlRequest {
            config: cfg,
            source: spider(https(&site, "site.test", "/")),
            file: path.clone(),
            project: "c151".into(),
            name: "c151".into(),
        },
        deps,
    )
    .await
    .unwrap();
    support::until_crawled(&handle, 25).await;
    handle.pause();
    assert_eq!(handle.wait().await.unwrap().state, CrawlState::Paused);
    assert!(
        site.connections() >= 2,
        "vor der Pause: {}",
        describe(&site)
    );

    // In der Pause auf 1 gesetzt: der beim Fortsetzen gebaute Abrufer hält genau eine Verbindung, auch im Anlauf.
    site.reset_request_log();
    let mut cfg = CrawlFile::open(&path, OpenMode::ReadOnly)
        .unwrap()
        .config()
        .unwrap();
    cfg.speed.connections_per_host = ConnectionsPerHost::Fixed(1);
    let deps = test_deps();
    let resumed = resume_crawl(
        site_fetcher(&site, &cfg, &deps),
        path.clone(),
        Some(cfg),
        deps,
    )
    .await
    .unwrap()
    .wait()
    .await
    .unwrap();
    assert_eq!(resumed.state, CrawlState::Completed);
    assert_eq!(
        site.connections(),
        1,
        "nach dem Fortsetzen: {}",
        describe(&site)
    );
    let stored = CrawlFile::open(&path, OpenMode::ReadOnly)
        .unwrap()
        .config()
        .unwrap();
    assert_eq!(
        stored.speed.connections_per_host,
        ConnectionsPerHost::Fixed(1)
    );
}

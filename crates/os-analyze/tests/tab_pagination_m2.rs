//! M2-D05: Tabellen-/Analysevertrag gegen deterministische Store-Fixture.
use os_analyze::analysis::{runner::run_only, steps::pagination, CancelToken};
use os_analyze::views::{filter_counts, query_view, ColumnSelection, ViewQuery};
use os_model::{
    AnalysisStep, CrawlConfig, CrawlMode, FilterState, IdSource, SeededIds, StepState, Timestamp,
    Value,
};
use os_store::{queries, CrawlFile, CrawlInit, StorageProfile};
use std::collections::BTreeMap;

fn fixture() -> (tempfile::TempDir, CrawlFile) {
    let dir = tempfile::tempdir().unwrap();
    let at = Timestamp::from_db("2026-10-13T07:35:00.000Z").unwrap();
    let init = CrawlInit {
        crawl_id: SeededIds::new(5).new_crawl_id(at),
        mode: CrawlMode::Spider,
        start_url: Some("https://site.test/".into()),
        list_source: None,
        project: "pagination".into(),
        name: "D05".into(),
        config: CrawlConfig::default(),
        created_at: at,
    };
    let file = CrawlFile::create(
        &dir.path().join("pagination.ospider"),
        &init,
        StorageProfile::Balanced,
    )
    .unwrap();
    file.conn()
        .execute_batch(include_str!(
            "../../../tests/fixtures/tech-m2-pagination/crawl.sql"
        ))
        .unwrap();
    (dir, file)
}
fn analyze(file: &CrawlFile) {
    let outcome = run_only(
        file,
        &CrawlConfig::default(),
        &CancelToken::new(),
        &|_, _| {},
        &[AnalysisStep::Pagination],
    )
    .unwrap();
    assert_eq!(outcome.steps, [(AnalysisStep::Pagination, StepState::Done)]);
}
fn names(file: &CrawlFile, cfg: &CrawlConfig, filter: &str) -> Vec<String> {
    let mut q = ViewQuery::new("Pagination", filter);
    q.columns = ColumnSelection::Named(vec!["Address".into()]);
    let mut result: Vec<_> = query_view(file, cfg, &q)
        .unwrap()
        .rows
        .into_iter()
        .map(|r| {
            let Value::Text(s) = &r[0] else {
                panic!("address")
            };
            s.strip_prefix("https://site.test/").unwrap().to_string()
        })
        .collect();
    result.sort();
    result
}
// spec: TECH-032
// spec: TECH-033
// spec: TECH-034
// spec: TECH-035
// spec: TECH-036
// spec: TECH-037
// spec: TECH-038
// spec: TECH-039
// spec: TECH-040
// spec: TECH-041
#[test]
fn all_filters_match_the_pagination_fixture() {
    let (_dir, file) = fixture();
    analyze(&file);
    let expected: BTreeMap<String, Vec<String>> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/tech-m2-pagination/filters.json"
    ))
    .unwrap();
    assert_eq!(expected.len(), 10);
    for (filter, expected) in expected {
        assert_eq!(
            names(&file, &CrawlConfig::default(), &filter),
            expected,
            "{filter}"
        );
    }
}

// spec: TECH-031
#[test]
fn population_and_numbered_columns_follow_the_pagination_contract() {
    let (_dir, file) = fixture();
    let cfg = CrawlConfig::default();
    let all = names(&file, &cfg, "All");
    for present in ["about", "created"] {
        assert!(all.contains(&present.into()), "{present}");
    }
    for absent in [
        "unknown",
        "not-found",
        "redirect",
        "blocked",
        "document.pdf",
        "external-html",
        "external-target",
    ] {
        assert!(!all.contains(&absent.into()), "{absent}");
    }
    // Only Respect Next/Prev grants an exception to table hiding.
    file.conn()
        .execute("UPDATE urls SET in_tables = 0 WHERE id = 2", [])
        .unwrap();
    assert!(!names(&file, &cfg, "All").contains(&"second".into()));
    for (reason, visible) in [
        ("canonical", false),
        ("excluded", false),
        ("pagination", true),
    ] {
        file.conn().execute(
            "INSERT OR REPLACE INTO url_flags (url_id, flag, value) VALUES (2, 'respect_hidden', ?1)",
            [reason],
        ).unwrap();
        for filter in ["All", "Contains Pagination", "Paginated 2+ Pages"] {
            let addresses = names(&file, &cfg, filter);
            assert_eq!(
                addresses.contains(&"second".into()),
                visible,
                "{reason}: {filter}"
            );
            let counts = filter_counts(&file, &cfg).unwrap();
            let count = counts
                .iter()
                .find(|c| c.tab == "Pagination" && c.filter == filter)
                .unwrap();
            assert_eq!(count.count, Some(addresses.len() as u64));
        }
    }
    let mut q = ViewQuery::new("Pagination", "Multiple Pagination URLs");
    q.columns = ColumnSelection::Named(vec![
        "Address".into(),
        "Occurrences".into(),
        "rel=\"next\"".into(),
        "rel=\"prev\"".into(),
        "Canonical Link Element".into(),
        "HTTP Canonical".into(),
        "Meta Robots".into(),
        "X-Robots-Tag".into(),
    ]);
    let page = query_view(&file, &cfg, &q).unwrap();
    assert!(page.columns.contains(&"rel=\"next\" 2".into()));
    let row = page
        .rows
        .iter()
        .find(|r| r[0] == Value::Text("https://site.test/multiple".into()))
        .unwrap();
    assert_eq!(row[1], Value::Int(3));
    assert_eq!(row[2], row[3]);
    let mut off = cfg;
    off.spider.pagination.store = false;
    assert!(names(&file, &off, "All").is_empty());
}

// spec: TECH-037
// spec: TECH-040
// spec: TECH-041
#[test]
fn analysis_states_repeatability_change_feed_and_removal() {
    let (_dir, file) = fixture();
    let cfg = CrawlConfig::default();
    let counts = filter_counts(&file, &cfg).unwrap();
    let state = counts
        .iter()
        .find(|c| c.tab == "Pagination" && c.filter == "Pagination Loop")
        .unwrap();
    assert_eq!(
        (state.state.clone(), state.count),
        (FilterState::NeedsCrawlAnalysis, None)
    );
    let before = queries::change_seq(file.conn()).unwrap();
    analyze(&file);
    let seq = queries::change_seq(file.conn()).unwrap();
    assert!(seq > before);
    assert!(!queries::changed_since(file.conn(), before, 500)
        .unwrap()
        .is_empty());
    analyze(&file);
    assert_eq!(queries::change_seq(file.conn()).unwrap(), seq);
    let token = CancelToken::new();
    token.cancel();
    file.conn()
        .execute("DELETE FROM pagination WHERE url_id = 22", [])
        .unwrap();
    assert_eq!(
        pagination::run(&file, &cfg, &token).unwrap(),
        StepState::Pending
    );
    assert!(names(&file, &cfg, "Pagination Loop").contains(&"self".into()));
    analyze(&file);
    assert!(!names(&file, &cfg, "Pagination Loop").contains(&"self".into()));
    assert!(queries::change_seq(file.conn()).unwrap() > seq);
    let mut off = cfg;
    off.spider.pagination.store = false;
    pagination::run(&file, &off, &CancelToken::new()).unwrap();
    let n: i64 = file
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM url_flags WHERE flag LIKE 'pagination_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
}

// spec: TECH-037
// spec: TECH-040
// spec: TECH-041
#[test]
fn pause_during_flag_updates_resumes_or_cancels_with_atomic_rollback() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::Duration;

    for abort in [false, true] {
        let (_dir, file) = fixture();
        let before = queries::change_seq(file.conn()).unwrap();
        let token = CancelToken::new();
        let control = token.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let once = AtomicBool::new(false);
        file.conn()
            .create_collation("pagination_pause", move |left, right| {
                if !once.swap(true, Ordering::SeqCst) {
                    control.pause();
                    entered_tx.send(()).unwrap();
                }
                left.cmp(right)
            })
            .unwrap();
        // Inject the pause after the first actual write inside the transaction.
        file.conn()
            .execute_batch(
                "CREATE TEMP TRIGGER pause_pagination AFTER INSERT ON url_flags
             WHEN NEW.flag LIKE 'pagination_%'
             BEGIN SELECT 'left' = 'right' COLLATE pagination_pause; END;",
            )
            .unwrap();
        let worker_token = token.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let state = pagination::run(&file, &CrawlConfig::default(), &worker_token).unwrap();
            done_tx.send(state).unwrap();
            (file, state)
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let waiting = done_rx.recv_timeout(Duration::from_millis(50));
        if abort {
            token.cancel();
        } else {
            token.resume();
        }
        let (file, state) = worker.join().unwrap();
        assert!(matches!(waiting, Err(mpsc::RecvTimeoutError::Timeout)));
        assert!(file.conn().is_autocommit());
        let count: i64 = file
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM url_flags WHERE flag LIKE 'pagination_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if abort {
            assert_eq!(state, StepState::Pending);
            assert_eq!(count, 0);
            assert_eq!(queries::change_seq(file.conn()).unwrap(), before);
            let stamped: i64 = file
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM urls WHERE change_seq > ?1",
                    [before as i64],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(stamped, 0);
        } else {
            assert_eq!(state, StepState::Done);
            assert!(count > 0);
            assert!(queries::change_seq(file.conn()).unwrap() > before);
        }
        analyze(&file);
        assert!(names(&file, &CrawlConfig::default(), "Pagination Loop").contains(&"self".into()));
    }
}

// spec: TECH-031
#[test]
#[ignore = "Klärung K2-32: P2 CRAWL-093 (Respect Next/Prev)"]
fn respect_next_prev_crawls_and_hides_following_pages_except_in_pagination() {
    // Full integration belongs to P2; the tab exception is covered above.
    let (_dir, file) = fixture();
    let cfg = CrawlConfig::default();
    assert!(names(&file, &cfg, "All").contains(&"second".into()));
}

// spec: TECH-030
// spec: TECH-031
// spec: TECH-035
// spec: TECH-037
#[tokio::test]
async fn fixture_crawl_connects_parser_facts_discovery_storage_and_analysis() {
    for crawl in [false, true] {
        let mut cfg = CrawlConfig::default();
        cfg.spider.pagination.crawl = crawl;
        cfg.spider.follow_internal_nofollow = true;
        let fx = os_engine::testing::crawl_fixture("tech-m2-pagination", cfg.clone()).await;
        let outcome = run_only(
            &fx.file,
            &cfg,
            &CancelToken::new(),
            &|_, _| {},
            &[AnalysisStep::Pagination],
        )
        .unwrap();
        assert!(outcome
            .steps
            .contains(&(AnalysisStep::Pagination, StepState::Done)));
        let mut q = ViewQuery::new("Pagination", "Contains Pagination");
        q.columns = ColumnSelection::Named(vec!["Address".into()]);
        let page = query_view(&fx.file, &cfg, &q).unwrap();
        assert_eq!(page.rows.len(), if crawl { 3 } else { 2 });
        let p2 = format!("{}/p2", fx.site.base_url());
        let mut q = ViewQuery::new("Pagination", "Pagination URL Not In Anchor Tag");
        q.columns = ColumnSelection::Named(vec!["Address".into()]);
        assert_eq!(
            query_view(&fx.file, &cfg, &q).unwrap().rows,
            [vec![Value::Text(p2.clone())]]
        );
        q.filter = "Unlinked Pagination URLs".into();
        let rows = query_view(&fx.file, &cfg, &q).unwrap().rows;
        assert_eq!(
            rows,
            if crawl {
                vec![vec![Value::Text(p2)]]
            } else {
                vec![]
            }
        );
        q.filter = "Sequence Error".into();
        assert!(query_view(&fx.file, &cfg, &q).unwrap().rows.is_empty());
    }
}

// spec: TECH-031
#[tokio::test]
async fn prev_fact_is_stored_for_html_and_headers_even_without_pagination_storage() {
    let mut cfg = CrawlConfig::default();
    cfg.spider.pagination.store = false;
    cfg.spider.pagination.crawl = true;
    let fx = os_engine::testing::crawl_fixture("tech-m2-pagination", cfg.clone()).await;
    let count: i64 = fx
        .file
        .conn()
        .query_row("SELECT COUNT(*) FROM pagination", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    let count: i64 = fx
        .file
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM url_ext WHERE key = 'pagination.has_prev' AND v_int = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
    assert!(names(&fx.file, &cfg, "All").is_empty());
}

// spec: TECH-036
#[test]
fn a_target_status_change_rechecks_the_source_and_stamps_its_change_feed() {
    use os_model::{ContentClass, FetchFacts, Indexability, PageWrite, UrlId, WriteBatch};
    use os_store::{CrawlSink, SqliteSink};
    let (_dir, file) = fixture();
    let cfg = CrawlConfig::default();
    assert!(names(&file, &cfg, "Non-200 Pagination URLs").contains(&"broken".into()));
    let before = queries::change_seq(file.conn()).unwrap();
    let mut sink = SqliteSink::open(file.path(), StorageProfile::Balanced).unwrap();
    sink.write(WriteBatch {
        pages: vec![PageWrite {
            url_id: UrlId(7),
            replace: true,
            fetch: FetchFacts {
                status_code: 200,
                status: "OK".into(),
                content_class: ContentClass::Html,
                ..Default::default()
            },
            page: None,
            indexability: Indexability::Indexable,
            indexability_status: None,
            links: vec![],
            images: vec![],
            canonicals: vec![],
            directives: vec![],
            refresh: vec![],
            hreflang: vec![],
            pagination: vec![],
            headers: vec![],
            html: None,
        }],
        ..Default::default()
    })
    .unwrap();
    assert!(queries::changed_since(file.conn(), before, 500)
        .unwrap()
        .contains(&UrlId(6)));
    assert!(!names(&file, &cfg, "Non-200 Pagination URLs").contains(&"broken".into()));
}

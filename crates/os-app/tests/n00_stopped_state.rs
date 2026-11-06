//! M3-N00 (M2-backlog 5, N02a): after "Stop" the end state stays "stopped" in every place that shows it –
//! the live status, the status of the crawl reopened after a restart (header and status bar read it) and the
//! crawl manager (no resume). A crawl that ends on its own keeps reporting "queue_empty" ("Completed").
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use serde_json::json;
use std::path::Path;

/// Chain of pages that can only end after `CHAIN` × 50 ms, far beyond the stop (B3-FLAKES lesson: a stop or a
/// pause must never race the natural end of the fixture).
fn chain_site() -> os_engine::testing::SimSite {
    const CHAIN: usize = 400;
    let mut site = os_engine::testing::SimSite::new().delay(std::time::Duration::from_millis(50));
    for n in 0..CHAIN {
        let url = if n == 0 {
            "https://fixture.test/".to_owned()
        } else {
            format!("https://fixture.test/p/{n}")
        };
        site = site.page(&url, &format!("<a href='/p/{}'>weiter</a>", n + 1));
    }
    site
}

async fn status_json(app: &os_app::App) -> serde_json::Value {
    os_app::session::poll(app).await.unwrap();
    serde_json::to_value(app.assemble_tick().unwrap().crawl.unwrap()).unwrap()
}

// spec: UI-003
// spec: CRAWL-126
// spec: CRAWL-128
#[tokio::test]
async fn a_stopped_crawl_reports_stopped_live_and_after_reopening_in_a_restarted_app() {
    let dir = tempfile::tempdir().unwrap();
    let first = app(dir.path());
    let started = os_app::session::control::start_with_fetcher(
        &first,
        ctx(),
        request("https://fixture.test/".into()),
        cfg(),
        os_engine::testing::SimFetcher::new(chain_site()),
    )
    .await
    .unwrap();
    call(&first, "crawl.stop", json!(null)).await;

    let live = status_json(&first).await;
    assert_eq!(live["phase"], "completed");
    assert_eq!(live["endReason"], "stopped", "live status: {live}");

    first.read_pool().unregister(&key(&first));
    first.write_queue().unregister(&key(&first));
    drop(first);

    let restarted = app(dir.path());
    let listings = call(&restarted, "crawls.list", json!({"show":"all"})).await;
    let items = listings.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["crawl"]["phase"], "completed");
    // The rest of the chain is still queued; only the stop makes the crawl final (UI-003).
    assert_eq!(items[0]["canResume"], false);
    // B4-RESUME (CRAWL-128): the management shows „Abgeschlossen (gestoppt)“ from this additive field.
    assert_eq!(items[0]["endReason"], "stopped");

    open(&restarted, Path::new(&started.file)).await;
    let reopened = status_json(&restarted).await;
    assert_eq!(reopened["phase"], "completed");
    assert_eq!(
        reopened["endReason"], "stopped",
        "reopened status: {reopened}"
    );
    assert!(restarted
        .dispatch(ctx(), "crawl.resume", json!(null))
        .await
        .is_err());
}

// spec: UI-003
// spec: CRAWL-126
#[tokio::test]
async fn a_crawl_that_ends_on_its_own_reports_queue_empty_not_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let fetch = os_engine::testing::SimFetcher::new(
        os_engine::testing::SimSite::new()
            .page("https://fixture.test/", "<a href='/a'>A</a>")
            .page("https://fixture.test/a", "A"),
    );
    os_app::session::control::start_with_fetcher(
        &app,
        ctx(),
        request("https://fixture.test/".into()),
        cfg(),
        fetch,
    )
    .await
    .unwrap();
    completed(&app).await;
    let status = status_json(&app).await;
    assert_eq!(status["phase"], "completed");
    assert_eq!(status["endReason"], "queue_empty", "status: {status}");
}

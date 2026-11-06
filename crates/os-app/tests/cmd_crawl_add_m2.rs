use os_app::{pool::CrawlKey, runtime::sessions::SessionInfo, App, AppConfig, CallCtx, WindowId};
use os_model::{CrawlConfig, CrawlState};
use serde_json::json;
use std::sync::Arc;

// spec: CRAWL-131 [P1] [M2]
#[tokio::test]
async fn command_queues_and_resumes_a_completed_crawl_as_an_operation() {
    let fx = os_engine::testing::crawl_fixture("crawl-m2-lifecycle", CrawlConfig::default()).await;
    let home = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(home.path())).unwrap();
    let key = CrawlKey(fx.summary.crawl_id.to_string());
    let path = fx.file.path().to_path_buf();
    app.register_crawl(key.clone(), path.clone());
    app.sessions().set_current(
        WindowId("main".into()),
        SessionInfo {
            key,
            path,
            config: Arc::new(fx.file.config().unwrap()),
            state: CrawlState::Completed,
            writable: true,
            engine: None,
        },
    );
    let op = app
        .dispatch(
            CallCtx::for_window("main"),
            "crawl.addUrls",
            json!({"urls":[fx.site.url("/added"),fx.site.url("/first"),"bad url"]}),
        )
        .await
        .unwrap();
    let status = app.ops().wait(op["opId"].as_str().unwrap()).await.unwrap();
    assert!(status.error.is_none(), "{:?}", status.error);
    assert_eq!(status.result.unwrap(), json!({"added":1,"skipped":2}));
    let session = app.sessions().current(&WindowId("main".into())).unwrap();
    let handle = session
        .engine
        .unwrap()
        .handle
        .lock()
        .unwrap()
        .take()
        .unwrap();
    assert_eq!(handle.wait().await.unwrap().state, CrawlState::Completed);
    assert_eq!(fx.site.count("127.0.0.1", "/added"), 1);
}

// spec: CRAWL-131 [P1]
#[tokio::test]
async fn command_reports_no_crawl_and_honors_readonly_session() {
    let home = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(home.path())).unwrap();
    let op = app
        .dispatch(
            CallCtx::for_window("main"),
            "crawl.addUrls",
            json!({"urls":["https://site.test/"]}),
        )
        .await
        .unwrap();
    let result = app.ops().wait(op["opId"].as_str().unwrap()).await.unwrap();
    assert_eq!(result.error.unwrap().code, os_app::dto::ErrorCode::NoCrawl);
    app.sessions().set_current(
        WindowId("main".into()),
        SessionInfo {
            key: CrawlKey("fixture".into()),
            path: home.path().join("unopened"),
            config: Arc::new(CrawlConfig::default()),
            state: CrawlState::Completed,
            writable: false,
            engine: None,
        },
    );
    let op = app
        .dispatch(
            CallCtx::for_window("main"),
            "crawl.addUrls",
            json!({"urls":["https://site.test/"]}),
        )
        .await
        .unwrap();
    let result = app.ops().wait(op["opId"].as_str().unwrap()).await.unwrap();
    assert_eq!(
        result.error.unwrap().code,
        os_app::dto::ErrorCode::Permission
    );
}

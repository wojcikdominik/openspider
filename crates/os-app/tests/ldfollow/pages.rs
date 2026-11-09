//! Private snapshot test, compiled as a child of commands::view.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
use super::ldfollow_fixture::Fx;
use super::*;
use serde_json::json;

// spec: UI-012
// spec: UI-018
#[tokio::test]
async fn detail_range_pages_reject_a_changed_generation_and_retry_without_gaps() {
    let fx = Fx::new();
    fx.write("DELETE FROM http_headers; WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<1100) INSERT INTO http_headers(url_id,direction,position,name,value) SELECT 1,'response',x,'header-'||x,'value' FROM n;");
    let h = fx
        .open(
            json!({"kind":"detail","detail":"headers","urlIds":[1],"params":{},"sort":[]}),
            json!([]),
        )
        .await;
    let v = get(
        &fx.app,
        &CallCtx::for_window("main"),
        h["viewId"].as_str().unwrap(),
    )
    .unwrap();
    assert!(v.links.is_none());
    let path = fx.path.clone();
    *fx.app.ext::<RangePageWrite>().0.lock().unwrap() = Some(Box::new(move || {
        let writer = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
        writer.conn().execute_batch("DELETE FROM http_headers WHERE position=1; UPDATE crawl_meta SET value='43' WHERE key='change_seq';").unwrap();
    }));
    let args = RangeArgs {
        view_id: h["viewId"].as_str().unwrap().into(),
        from: 1,
        to: 1098,
    };
    let error = detail_range(&fx.app, &v, &args)
        .await
        .expect_err("mixed generations must fail");
    assert_eq!(error.code, ErrorCode::Stale);
    let path = fx.path.clone();
    *fx.app.ext::<RangePageWrite>().0.lock().unwrap() = Some(Box::new(move || {
        let writer = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
        writer.conn().execute_batch("DELETE FROM http_headers WHERE position=2; UPDATE crawl_meta SET value='44' WHERE key='change_seq';").unwrap();
    }));
    let result = fx
        .call("view.range", serde_json::to_value(args).unwrap())
        .await;
    assert_eq!(result["total"], 1098);
    assert_eq!(result["keys"], json!((1..=1098).collect::<Vec<_>>()));
}

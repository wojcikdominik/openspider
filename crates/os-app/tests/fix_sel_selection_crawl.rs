#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
//! fix-sel (1.0.2): URL ids gelten je Crawl. `crawl.recrawl`/`crawl.remove` nehmen die Crawl-ID der Ansicht mit, aus der
//! die Auswahl stammt; hält die Sitzung inzwischen einen anderen Crawl, lehnt der Kern ab und ändert nichts. Ohne die ID
//! gilt das bisherige Verhalten. `view.open` nennt den Crawl, dessen Zeilen die View zeigt.
#[path = "session_common.rs"]
mod common;
use common::*;
use os_app::dto::*;
use serde_json::json;

async fn urls(app: &os_app::App) -> i64 {
    app.read_pool()
        .run(key(app), |f, _| {
            Ok(f.conn()
                .query_row("SELECT COUNT(*) FROM urls", [], |r| r.get::<_, i64>(0))?)
        })
        .await
        .unwrap()
}

// spec: UI-018
// spec: CRAWL-131
#[tokio::test]
async fn a_selection_from_another_crawl_is_rejected_and_changes_nothing() {
    let site = os_testsite::TestSite::start_dir(fixture(), 0)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    cfg().save_default(&app.config().paths.settings).unwrap();
    call(
        &app,
        "crawl.start",
        serde_json::to_value(request(site.url("/"))).unwrap(),
    )
    .await;
    completed(&app).await;
    let crawl_a = key(&app).0;
    // Another start replaces crawl A by crawl B in this window's session.
    call(
        &app,
        "crawl.start",
        serde_json::to_value(request(site.url("/"))).unwrap(),
    )
    .await;
    completed(&app).await;
    let crawl_b = key(&app).0;
    assert_ne!(crawl_a, crawl_b);
    let before = urls(&app).await;
    assert!(before > 1);
    for name in ["crawl.remove", "crawl.recrawl"] {
        let err = app
            .dispatch(
                ctx(),
                name,
                json!({"selection":{"kind":"ids","ids":[1]},"crawlId":crawl_a}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, os_app::ErrorCode::NotFound, "{name}");
        assert_eq!(err.message_key, "err-selection-other-crawl", "{name}");
    }
    // Nothing was started and crawl B is unchanged.
    assert!(app
        .ops()
        .active_statuses()
        .iter()
        .all(|op| op.label_key != "op-remove" && op.label_key != "op-recrawl"));
    assert_eq!(urls(&app).await, before);

    // The same crawl: as before.
    let result = operation(
        &app,
        "crawl.remove",
        json!({"selection":{"kind":"ids","ids":[1]},"crawlId":crawl_b}),
    )
    .await;
    assert_eq!(result.state, OpState::Done, "{:?}", result.error);
    assert_eq!(result.result.unwrap()["count"], 1);
    assert_eq!(urls(&app).await, before - 1);

    // Without a crawl id (older callers): as before.
    let result = operation(
        &app,
        "crawl.recrawl",
        json!({"selection":{"kind":"ids","ids":[2]}}),
    )
    .await;
    assert_eq!(result.state, OpState::Done, "{:?}", result.error);
    assert_eq!(result.result.unwrap()["count"], 1);
}

// spec: UI-018
// spec: UI-142
#[tokio::test]
async fn view_handles_name_the_crawl_they_show() {
    let site = os_testsite::TestSite::start_dir(fixture(), 0)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    cfg().save_default(&app.config().paths.settings).unwrap();
    call(
        &app,
        "crawl.start",
        serde_json::to_value(request(site.url("/"))).unwrap(),
    )
    .await;
    completed(&app).await;
    let crawl = key(&app).0;
    let tab: ViewHandle = serde_json::from_value(
        call(
            &app,
            "view.open",
            json!({"spec":{"kind":"tab","tab":"Internal","filter":"All","search":null,"sort":[]},"columns":["Address"]}),
        )
        .await,
    )
    .unwrap();
    assert_eq!(tab.crawl_id.as_deref(), Some(crawl.as_str()));
    let updated: ViewHandle = serde_json::from_value(
        call(
            &app,
            "view.update",
            json!({"viewId":tab.view_id,"spec":{"kind":"tab","tab":"Internal","filter":"HTML","search":null,"sort":[]}}),
        )
        .await,
    )
    .unwrap();
    assert_eq!(updated.crawl_id.as_deref(), Some(crawl.as_str()));
}

// spec: UI-018
// spec: OUT-021
#[tokio::test]
async fn a_detail_export_of_another_crawls_selection_writes_no_file() {
    let site = os_testsite::TestSite::start_dir(fixture(), 0)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    cfg().save_default(&app.config().paths.settings).unwrap();
    for _ in 0..2 {
        call(
            &app,
            "crawl.start",
            serde_json::to_value(request(site.url("/"))).unwrap(),
        )
        .await;
        completed(&app).await;
    }
    let current = key(&app).0;
    let args = |path: &std::path::Path, crawl: Option<&str>| {
        let mut a = json!({
            "spec":{"kind":"outlinks","urlIds":[1],"filter":{"id":"All"},"search":null,"sort":[]},
            "selection":{"kind":"ids","ids":[1]},
            "path":path,
            "format":"csv"
        });
        if let Some(id) = crawl {
            a["crawlId"] = json!(id);
        }
        a
    };
    let refused = dir.path().join("refused.csv");
    let err = app
        .dispatch(
            ctx(),
            "export.detail",
            args(&refused, Some("another-crawl")),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, os_app::ErrorCode::NotFound);
    assert_eq!(err.message_key, "err-selection-other-crawl");
    assert!(!refused.exists());

    // The crawl of the session, and no crawl id (older callers): as before.
    for (name, crawl) in [("same.csv", Some(current.as_str())), ("none.csv", None)] {
        let path = dir.path().join(name);
        let result = operation(&app, "export.detail", args(&path, crawl)).await;
        assert_eq!(result.state, OpState::Done, "{name}: {:?}", result.error);
        assert!(path.exists(), "{name}");
    }
}

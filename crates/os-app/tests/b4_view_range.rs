//! B4-LIVEDETAIL (Abnahmebefunde F2, F3): Bereichsauswahl und Crawl-Pfad-Ansicht im laufenden Crawl über den Kern.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::time::Duration;

async fn open_view(app: &os_app::App, spec: Value, columns: Value) -> Value {
    let mut handle = call(app, "view.open", json!({"spec":spec,"columns":columns})).await;
    if handle["state"] == "building" {
        let op = app.ops().wait(handle["op"]["opId"].as_str().unwrap()).await;
        handle = op.unwrap().result.unwrap();
    }
    handle
}

async fn start_live(app: &os_app::App, base: String) {
    let mut config = cfg();
    config.speed.threads = 4;
    config.speed.max_urls_per_sec = Some(200.0);
    config.save_default(&app.config().paths.settings).unwrap();
    call(
        app,
        "crawl.start",
        serde_json::to_value(request(base)).unwrap(),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let done = app
                .read_pool()
                .run(key(app), |f, _| {
                    Ok(f.conn().query_row(
                        "SELECT COUNT(*) FROM urls WHERE state='done'",
                        [],
                        |r| r.get::<_, u64>(0),
                    )?)
                })
                .await
                .unwrap();
            if done >= 200 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

// spec: UI-012
// spec: UI-018
// A range is resolved in one state of the view, however quickly the running crawl changes the version.
#[tokio::test]
async fn a_range_stays_exact_while_the_running_crawl_changes_the_view_version() {
    let site = os_testsite::TestSite::start_spec(
        "synthetic:n=3000,links=30,seed=7",
        os_testsite::SiteOptions::default(),
    )
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    start_live(&app, site.url("/")).await;
    let handle = open_view(
        &app,
        json!({"kind":"tab","tab":"Internal","filter":"All","search":null,"sort":[]}),
        json!(["Address"]),
    )
    .await;
    let id = handle["viewId"].clone();
    let mut versions = BTreeSet::new();
    for round in 0..120u64 {
        os_app::session::poll(&app).await.unwrap();
        let current = call(
            &app,
            "view.update",
            json!({"viewId":id,"spec":{"kind":"tab","tab":"Internal","filter":"All","search":null,"sort":[]}}),
        )
        .await;
        let window = call(
            &app,
            "view.rows",
            json!({"viewId":id,"version":current["version"],"offset":0,"limit":60,"columns":["Address"]}),
        )
        .await;
        let keys = window["rowKeys"].as_array().unwrap().clone();
        let (a, b) = (5 + (round as usize % 10), 40 + (round as usize % 15));
        // The opposite direction (Shift+click above the anchor) yields the same table order.
        for (from, to) in [(a, b), (b, a)] {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let range = call(
                &app,
                "view.range",
                json!({"viewId":id,"from":keys[from],"to":keys[to]}),
            )
            .await;
            versions.insert(range["version"].as_u64().unwrap());
            assert_eq!(range["fromRow"].as_u64(), Some(from as u64));
            assert_eq!(range["toRow"].as_u64(), Some(to as u64));
            assert_eq!(range["all"], false);
            assert_eq!(
                range["keys"].as_array().unwrap(),
                &keys[a.min(b)..=a.max(b)],
                "range in table order, round {round}"
            );
            assert!(range["total"].as_u64().unwrap() >= 60);
        }
        // An endpoint that is not in the view yields no keys instead of an error.
        let missing = call(
            &app,
            "view.range",
            json!({"viewId":id,"from":keys[0],"to":999_999_999}),
        )
        .await;
        assert_eq!(missing["toRow"], Value::Null);
        assert_eq!(missing["keys"], json!([]));
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        versions.len() >= 3,
        "the crawl must change the version during the test: {versions:?}"
    );
    call(&app, "crawl.stop", json!(null)).await;
}

// spec: UI-012
// Inlinks use their materialized link list; the keys are link IDs in table order.
#[tokio::test]
async fn a_range_in_a_detail_table_uses_its_materialized_order() {
    let site = os_testsite::TestSite::start_spec(
        "synthetic:n=400,links=30,seed=7",
        os_testsite::SiteOptions::default(),
    )
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let mut config = cfg();
    config.speed.threads = 4;
    config.save_default(&app.config().paths.settings).unwrap();
    call(
        &app,
        "crawl.start",
        serde_json::to_value(request(site.url("/"))).unwrap(),
    )
    .await;
    completed(&app).await;
    let handle = open_view(
        &app,
        json!({"kind":"inlinks","urlIds":[1],"filter":{"id":"All"},"search":null,"sort":[]}),
        json!(["From"]),
    )
    .await;
    let window = call(
        &app,
        "view.rows",
        json!({"viewId":handle["viewId"],"version":handle["version"],"offset":0,"limit":10,"columns":["From"]}),
    )
    .await;
    let keys = window["rowKeys"].as_array().unwrap();
    assert!(keys.len() >= 4, "fixture needs several inlinks");
    let range = call(
        &app,
        "view.range",
        json!({"viewId":handle["viewId"],"from":keys[3],"to":keys[1]}),
    )
    .await;
    assert_eq!(range["keys"].as_array().unwrap(), &keys[1..=3]);
    assert_eq!(range["fromRow"], 3);
    assert_eq!(range["toRow"], 1);
}

// spec: UI-050
// spec: UI-018
// The Crawl Path view opens and reads repeatedly during a crawl without any error (finding F2: one "file operation failed").
#[tokio::test]
async fn the_crawl_path_view_reads_without_error_during_a_running_crawl() {
    let site = os_testsite::TestSite::start_spec(
        "synthetic:n=3000,links=30,seed=7",
        os_testsite::SiteOptions::default(),
    )
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    start_live(&app, site.url("/")).await;
    for round in 0..150i64 {
        os_app::session::poll(&app).await.unwrap();
        let url = 2 + round % 100;
        let spec =
            json!({"kind":"detail","detail":"crawlPath","urlIds":[url],"params":{},"sort":[]});
        let handle = call(
            &app,
            "view.open",
            json!({"spec":spec,"columns":["Crawl Depth","Address","Anchor","Link Type"]}),
        )
        .await;
        let rows = call(
            &app,
            "view.rows",
            json!({"viewId":handle["viewId"],"version":handle["version"],"offset":0,"limit":200,"columns":["Crawl Depth","Address","Anchor","Link Type"]}),
        )
        .await;
        assert_eq!(
            rows["cells"].as_array().unwrap().len() as u64,
            handle["total"].as_u64().unwrap()
        );
        // UI-050: the list reads from the start URL (depth 0) to the selected URL.
        let depths: Vec<i64> = rows["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|row| row[0].as_i64())
            .collect();
        assert!(depths.windows(2).all(|d| d[0] < d[1]), "{depths:?}");
        if let Some(first) = depths.first() {
            assert_eq!(*first, 0);
        }
        call(&app, "view.close", json!({"viewId":handle["viewId"]})).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    call(&app, "crawl.stop", json!(null)).await;
}

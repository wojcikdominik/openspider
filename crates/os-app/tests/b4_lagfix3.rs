//! B4-LAGFIX3 (UI-018, OUT-020, CRAWL-141): Ist der Endstatus eines Crawls beim Client, ändert sich der Datenstand nicht
//! mehr ohne neue Daten. Die automatische Crawl-Analyse, die das Crawl-Ende startet, schreibt vor dem End-Tick; eine auf
//! den Endstatus hin geöffnete Ansicht lässt sich danach exportieren.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use os_app::dto::{CrawlPhase, OpState, UiEvent};
use os_app::events::CollectingSink;
use os_store::{CrawlFile, OpenMode};
use serde_json::json;
use std::{path::Path, sync::Arc, time::Duration};

fn change_seq(path: &Path) -> u64 {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .get_meta("change_seq")
        .unwrap()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

// spec: UI-018
// spec: OUT-020
// spec: CRAWL-141
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_end_status_follows_the_automatic_analysis_and_its_view_stays_exportable() {
    let site = os_testsite::TestSite::start_spec(
        "synthetic:n=30,links=4,seed=11",
        os_testsite::SiteOptions::default(),
    )
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = app(&dir.path().join("home"));
    let sink = Arc::new(CollectingSink::new());
    app.subscribe(window(), sink.clone());
    let mut config = cfg();
    config.analysis.auto_at_end = true;
    config.save_default(&app.config().paths.settings).unwrap();
    let started = call(
        &app,
        "crawl.start",
        serde_json::to_value(request(site.url("/"))).unwrap(),
    )
    .await;
    let file = std::path::PathBuf::from(started["file"].as_str().unwrap());

    // Only the running session loop delivers ticks here (no test-side poll or flush), as in the app.
    let mut analysis_running = None;
    let end = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            for event in sink.take() {
                let UiEvent::Tick { tick } = event else {
                    continue;
                };
                if let Some(a) = &tick.analysis {
                    analysis_running = Some(a.running);
                }
                if tick
                    .crawl
                    .as_ref()
                    .is_some_and(|c| c.phase == CrawlPhase::Completed)
                {
                    return *tick;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("end status within 30 s");
    // The client sees the end status: open a view at once, as the export dialog does (OUT-020).
    let seq_at_end = change_seq(&file);
    let mut handle = call(
        &app,
        "view.open",
        json!({"spec":{"kind":"tab","tab":"Internal","filter":"All","search":null,"sort":[]},"columns":["Address"]}),
    )
    .await;
    if handle["state"] == "building" {
        let op = app.ops().wait(handle["op"]["opId"].as_str().unwrap()).await;
        handle = op.unwrap().result.unwrap();
    }
    assert_eq!(
        analysis_running,
        Some(false),
        "the end tick (seq {}) reports the automatic analysis as finished",
        end.seq
    );
    // Give a late writer (the analysis) ample time; nothing may change the data version any more.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        change_seq(&file),
        seq_at_end,
        "the data version changed after the end status reached the client"
    );
    assert_eq!(handle["version"].as_u64(), Some(seq_at_end));
    let out = dir.path().join("internal.csv");
    let export = operation(
        &app,
        "export.view",
        json!({"viewId":handle["viewId"],"version":handle["version"],"path":out,"format":"csv","allColumns":false}),
    )
    .await;
    assert_eq!(
        export.state,
        OpState::Done,
        "export of the view opened on the end status: {:?}",
        export.error
    );
    assert!(std::fs::read_to_string(&out)
        .unwrap()
        .contains(&site.url("/")));
}

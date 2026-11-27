//! Vergleichsübersicht der Seitenleiste und Schalter „KI (Vorschau)“ (M4-V01, Plan M4 E1 E-2 b): Auch für Crawls,
//! die mit aktiver KI liefen, zeigt `sidebar.overview` im Vergleich ohne Schalter keine Zeile eines KI-Tabs; mit
//! Schalter erscheinen die Zeilen von `AI Suggestions`. Fixtures: `out-m2-compare/v1` und `v2`.
// Testcode legt Crawl-Dateien an: die Ausnahme von `clippy.toml` gilt hier, nicht in Quellcode.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "../../os-analyze/tests/compare_common.rs"]
mod fixture;
use fixture::*;
use os_app::{App, AppConfig, CallCtx};
use serde_json::json;

async fn ai_rows(preview: bool) -> Vec<String> {
    let mut config = cfg();
    config.ai.enabled = true;
    let (old, new) = (v1_with(config.clone()).await, v2_with(config).await);
    let home = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig::new(home.path())).unwrap();
    let main = || CallCtx::for_window("main");
    if preview {
        app.dispatch(
            main(),
            "settings.set",
            json!({ "patch": { "aiApp": { "previewEnabled": true } } }),
        )
        .await
        .expect("settings.set");
    }
    app.dispatch(
        main(),
        "compare.open",
        json!({"current":new.file.path(),"previous":old.file.path(),"mapping":[],"elements":[]}),
    )
    .await
    .expect("compare.open");
    let overview = app
        .dispatch(main(), "sidebar.overview", json!({"comparison":true}))
        .await
        .expect("sidebar.overview");
    overview["comparison"]["overview"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["tab"].as_str().unwrap().to_string())
        .filter(|tab| os_analyze::compare::is_ai_task_tab(tab))
        .collect()
}

// spec: AI-070 [M4]
// spec: OUT-142
#[tokio::test(flavor = "multi_thread")]
async fn comparison_overview_has_no_ai_row_with_the_switch_off() {
    let rows = ai_rows(false).await;
    assert!(rows.is_empty(), "{rows:?}");
}

// spec: AI-070 [M4]
// spec: OUT-142
#[tokio::test(flavor = "multi_thread")]
async fn comparison_overview_lists_ai_suggestions_with_the_switch_on() {
    let rows = ai_rows(true).await;
    assert!(rows.iter().any(|t| t == "AI Suggestions"), "{rows:?}");
}

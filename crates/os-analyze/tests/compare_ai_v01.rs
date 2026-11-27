//! Vergleichsübersicht und KI-Tabs (M4-V01, Plan M4 E1 E-2 b): Ohne aktive KI im Crawl (Laufkopie bei Schalter
//! „KI (Vorschau)“ aus: `ai.enabled = false`) enthält [`Comparison::overview`] und damit jede Zusammenfassung darüber
//! (Mail-Abschluss, Vergleichsbericht, Benachrichtigung, MCP `compare_crawls`) keine Zeile eines KI-Tabs; mit aktiver
//! KI erscheinen die Zeilen des Tabs `AI Suggestions`. Fixture: `out-m2-compare/v1` und `out-m2-compare/v2`.

#[path = "compare_common.rs"]
mod common;

use common::*;
use os_analyze::compare::is_ai_task_tab;
use os_model::CancelToken;

async fn ai_rows(enabled: bool) -> Vec<String> {
    let mut config = cfg();
    config.ai.enabled = enabled;
    let (old, new) = (v1_with(config.clone()).await, v2_with(config).await);
    let mut cmp = compare(&old, &new, &[]);
    assert_eq!(cmp.current().config.ai.enabled, enabled);
    cmp.overview(&CancelToken::new())
        .unwrap()
        .into_iter()
        .filter(|r| is_ai_task_tab(&r.tab))
        .map(|r| format!("{} / {}", r.tab, r.filter))
        .collect()
}

// spec: AI-070 [M4]
// spec: OUT-142
#[tokio::test(flavor = "multi_thread")]
async fn ai_switch_off_leaves_no_ai_row_in_the_comparison_overview() {
    let rows = ai_rows(false).await;
    assert!(rows.is_empty(), "{rows:?}");
}

// spec: AI-070 [M4]
// spec: OUT-142
#[tokio::test(flavor = "multi_thread")]
async fn ai_switch_on_lists_the_ai_suggestions_rows() {
    let rows = ai_rows(true).await;
    for filter in ["All", "Titles", "Meta Descriptions", "Accepted", "Rejected"] {
        let row = format!("AI Suggestions / {filter}");
        assert!(rows.contains(&row), "{row}: {rows:?}");
    }
}

// spec: AI-070 [M4]
#[test]
fn ai_task_tabs_are_exactly_the_four_ai_tabs() {
    for tab in ["AI", "AI Decisions", "AI Suggestions", "AI Search"] {
        assert!(is_ai_task_tab(tab), "{tab}");
    }
    for tab in [
        "Internal",
        "Page Titles",
        "Change Detection",
        "Custom Search",
    ] {
        assert!(!is_ai_task_tab(tab), "{tab}");
    }
}

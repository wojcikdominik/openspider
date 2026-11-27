//! Unabhängige Abnahmetests M4-V01 (1.1-Etappe): Sichtbarkeit von „AI Suggestions“ außerhalb des Tabs bei
//! ausgeschaltetem Schalter „KI (Vorschau)“ – Mail-Zusammenfassung nach Crawl-Ende und der darin enthaltene
//! Vergleichsbericht (Auto-Vergleich, OUT-282).
//!
//! Grundlage: Plan M4 E1 (E-2 b) „mit Schalter aus erscheint keine KI-Oberfläche“, `docs/spec/05-ui.md` UI-004 [M4] (die
//! KI-Tabs erscheinen erst mit dem Meilenstein und nur mit Schalter). Der Bauer behebt die Mail-Zusammenfassung in einem
//! Folge-Commit; bis dahin ist dieser Test erwartbar rot (Bericht `docs/plan/M4-reports/v01-tests.md`).
// acceptance: independent
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

#[path = "../../../tests/fixtures/ai/v01-acceptance/states.rs"]
mod states;

use serde_json::Value;

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["name"].as_str().map(str::to_string))
        .collect()
}

// spec: UI-004 [M4]
// acceptance: independent
// Mail-Zusammenfassung (Übersicht, Probleme, Änderungen des Auto-Vergleichs) und gerenderter Text nennen bei
// Standardeinstellungen (Schalter aus) keinen Eintrag „AI Suggestions“, auch wenn die Crawl-Datei Vorschläge enthält.
#[test]
fn mail_summary_and_comparison_show_no_ai_suggestions_with_switch_off() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("prev");
    let b = dir.path().join("curr");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let previous = states::create(&a, "previous.ospider", os_model::CrawlConfig::default())
        .path()
        .to_path_buf();
    let current = states::create(&b, "current.ospider", os_model::CrawlConfig::default())
        .path()
        .to_path_buf();
    let data = os_analyze::mail_completion(&current, Some(&previous), true, false).unwrap();
    let mut listed = names(&data["changes"]);
    listed.extend(names(&data["issues"]));
    let hits: Vec<&String> = listed
        .iter()
        .filter(|n| n.starts_with("AI Suggestions"))
        .collect();
    assert!(hits.is_empty(), "Mail-Zusammenfassung/Vergleich: {hits:?}");
    let completion: os_sched::mail::Completion = serde_json::from_value(data).unwrap();
    let text = completion.render().0;
    assert!(
        !text.contains("AI Suggestions"),
        "gerenderte Mail nennt AI Suggestions"
    );
}

//! Unabhängige Abnahmetests M4-F01: Register-Stubs der KI-Tabs.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-020, AI-039, AI-070, AI-127) und
//! `docs/spec/04-custom-integrations.md` (INTEG-174).
// acceptance: independent

use os_analyze::tabs::all;

// spec: AI-020
// acceptance: independent
// Tabs „AI“ (INTEG-174), „AI Decisions“ (AI-039), „AI Suggestions“ (AI-070), „AI Search“ (AI-127) sind registriert;
// je KI-Tab mit KI-Statusspalten gibt es den Filter „Errors“ (AI-020, INTEG-174).
#[test]
fn ai_tabs_are_registered_with_an_errors_filter() {
    let tabs = all();
    for id in ["AI", "AI Decisions", "AI Suggestions", "AI Search"] {
        assert_eq!(
            tabs.iter().filter(|t| t.id == id).count(),
            1,
            "Tab {id} genau einmal"
        );
    }
    for id in ["AI", "AI Decisions", "AI Suggestions"] {
        let tab = tabs.iter().find(|t| t.id == id).unwrap();
        assert!(
            tab.filters.iter().any(|f| f.id == "Errors"),
            "Tab {id} ohne Filter Errors"
        );
    }
}

// spec: AI-070
// acceptance: independent
// Tab „AI Suggestions“ mit den Filtern `Titles`, `Meta Descriptions`, `H1`, `Alt Text`, `No Valid Suggestion`,
// `Accepted`, `Open`, `Rejected`.
#[test]
fn ai_suggestions_tab_has_the_spec_filters() {
    let tabs = all();
    let tab = tabs.iter().find(|t| t.id == "AI Suggestions").unwrap();
    for f in [
        "Titles",
        "Meta Descriptions",
        "H1",
        "Alt Text",
        "No Valid Suggestion",
        "Accepted",
        "Open",
        "Rejected",
    ] {
        assert!(tab.filters.iter().any(|d| d.id == f), "Filter {f} fehlt");
    }
}

//! Platzhalter M4-S01 (E4): Kriterien, die an AI-122 (Filter und Issues zu AI-Crawlern, Release 1.2) hängen.
//! In der 1.1-Etappe von M4-S01 baut das Paket nur AI-120 und AI-121 (`release_split`, K4-18); die Tests bleiben
//! bis zur 1.2-Etappe ignoriert und werden dann mit unverändertem Inhalt scharfgeschaltet.

use os_analyze::issues::catalog;
use os_analyze::tabs::all;
use os_model::{IssuePriority, IssueType};

const AI_SEARCH_FILTERS: [&str; 4] = [
    "Blocked For AI Search Bots",
    "Blocked For AI User Fetchers",
    "Blocked For AI Training Only",
    "AI Search Blocked But Training Allowed",
];

// spec: UI-004 [M4]
// spec: AI-122
// Tab „AI Search“ (AI-122 ff.) mit den Filtern aus AI-122, umgesetzt statt Platzhalter.
#[test]
#[ignore = "wartet auf M4-S01 (1.2-Etappe, K4-18): AI-122 Filter im Tab AI Search"]
fn ai_search_tab_has_the_ai_122_filters() {
    let tabs = all();
    let tab = tabs
        .iter()
        .find(|t| t.id == "AI Search")
        .expect("Tab AI Search");
    for id in AI_SEARCH_FILTERS {
        let f = tab
            .filters
            .iter()
            .find(|f| f.id == id)
            .unwrap_or_else(|| panic!("Filter {id} fehlt"));
        assert!(!f.is_stub(), "Filter {id} ist noch ein Platzhalter");
    }
}

// spec: UI-064 [M4]
// spec: AI-122
// Kategorie „AI Search“: `Blocked For AI Search Bots` (Warning, Medium, T-W1/P-M1) und `AI Search Blocked But
// Training Allowed` (Warning, Low, T-W1/P-L1) sind Issues; `Blocked For AI Training Only` ist nur ein Filter.
#[test]
#[ignore = "wartet auf M4-S01 (1.2-Etappe, K4-18): AI-122 Issues im Katalog (UI-063/UI-064)"]
fn ai_search_issue_category_matches_ai_122() {
    let entries = catalog();
    let find = |filter: &str| {
        entries
            .iter()
            .find(|e| e.tab == "AI Search" && e.filter == filter)
    };
    let search = find("Blocked For AI Search Bots").expect("Issue Blocked For AI Search Bots");
    assert_eq!(search.issue_type, IssueType::Warning);
    assert_eq!(search.priority, IssuePriority::Medium);
    assert_eq!(
        (search.type_rule.as_str(), search.priority_rule.as_str()),
        ("T-W1", "P-M1")
    );
    let mixed = find("AI Search Blocked But Training Allowed")
        .expect("Issue AI Search Blocked But Training Allowed");
    assert_eq!(mixed.issue_type, IssueType::Warning);
    assert_eq!(mixed.priority, IssuePriority::Low);
    assert_eq!(
        (mixed.type_rule.as_str(), mixed.priority_rule.as_str()),
        ("T-W1", "P-L1")
    );
    assert!(
        find("Blocked For AI Training Only").is_none(),
        "nur Filter, kein Issue"
    );
}

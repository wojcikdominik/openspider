//! STUB(M4-P02): AI definitions and predicates.
use crate::defs::{FilterDef, FilterKind, TabDef};
pub fn tab() -> TabDef {
    let mut tab = super::stub_tab("AI");
    tab.filters = ["All", "Errors"]
        .into_iter()
        .map(|id| FilterDef::placeholder(id, FilterKind::Live, &[]))
        .collect();
    tab
}

#[cfg(test)]
mod tests {
    // spec: AI-020
    #[test]
    fn filter_fixture_matches_registered_empty_tabs() {
        let fixture: toml::Value =
            toml::from_str(include_str!("../../../../docs/plan/M4-filters.toml")).unwrap();
        let tabs = super::super::all();
        for entry in fixture["tab"].as_array().unwrap() {
            let id = entry["id"].as_str().unwrap();
            let tab = tabs.iter().find(|t| t.id == id).unwrap();
            // Von Fachpaketen umgesetzte KI-Tabs sind keine Stubs mehr (M4-V01: AI Suggestions).
            let implemented = ["AI Suggestions"].contains(&id);
            if !implemented {
                assert_eq!(tab.population, "0");
            }
            let expected: Vec<_> = entry["filters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["id"].as_str().unwrap())
                .collect();
            assert_eq!(
                tab.filters.iter().map(|f| f.id).collect::<Vec<_>>(),
                expected
            );
            assert!(implemented || tab.filters.iter().all(|f| f.id == "All" || f.is_stub()));
        }
    }
}

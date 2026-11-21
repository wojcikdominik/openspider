//! STUB(M4-V01): AI Suggestions definitions and predicates.
use crate::defs::{FilterDef, FilterKind, TabDef};
pub fn tab() -> TabDef {
    let mut tab = super::stub_tab("AI Suggestions");
    tab.filters = [
        "All",
        "Titles",
        "Meta Descriptions",
        "H1",
        "Alt Text",
        "No Valid Suggestion",
        "Accepted",
        "Open",
        "Rejected",
        "Errors",
    ]
    .into_iter()
    .map(|id| FilterDef::placeholder(id, FilterKind::Live, &[]))
    .collect();
    tab
}

//! STUB(M4-D02): AI Decisions definitions and predicates.
use crate::defs::{FilterDef, FilterKind, TabDef};
pub fn tab() -> TabDef {
    let mut tab = super::stub_tab("AI Decisions");
    tab.filters = [
        "All",
        "Refusals",
        "Errors",
        "Pending",
        "Self-reported Probabilities",
    ]
    .into_iter()
    .map(|id| FilterDef::placeholder(id, FilterKind::Live, &[]))
    .collect();
    tab
}

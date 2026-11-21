//! STUB(M4-S01): AI Search definitions and predicates.
use crate::defs::{FilterDef, FilterKind, TabDef};
pub fn tab() -> TabDef {
    let mut tab = super::stub_tab("AI Search");
    tab.filters = [
        "All",
        "Blocked For AI Search Bots",
        "Blocked For AI User Fetchers",
        "Blocked For AI Training Only",
        "AI Search Blocked But Training Allowed",
        "Missing llms.txt",
        "Invalid llms.txt",
        "llms.txt With Broken Links",
        "llms.txt Links To Non-Indexable",
        "Blocked For AI Crawlers By Server",
        "Low Content Without JavaScript",
        "Links Only Via JavaScript",
        "Headings Only Via JavaScript",
        "Structured Data Only Via JavaScript",
    ]
    .into_iter()
    .map(|id| FilterDef::placeholder(id, FilterKind::Live, &[]))
    .collect();
    tab
}

//! Teilschritte der Crawl-Analyse. Alle übrigen Schritte sind `NotApplicable`; die Stubs von M2 (`near_duplicates`,
//! `link_score`, `pagination`, `hreflang`, `links`) sind angemeldet und gehören den Fachpaketen D02, D05, D07, D08.

pub mod canonicals;
pub mod hreflang;
pub mod link_score;
pub mod links;
pub mod near_duplicates;
pub mod pagination;
pub mod sitemaps;

pub mod images;

/// STUB(M4-E01): embedding step, implementation in semantic.
pub mod embeddings {
    pub use crate::semantic::run;
}
/// STUB(M4-S02): AI Search step, implementation in ai_search.
pub mod ai_search {
    pub use crate::ai_search::run;
}

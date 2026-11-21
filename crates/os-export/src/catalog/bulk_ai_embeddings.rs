//! STUB(M4-E02): AI export entries, replaced by M4-E02.
use super::{stub_run, TableEntry};
pub fn entries() -> &'static [TableEntry] {
    ENTRIES
}
const ENTRIES: &[TableEntry] = &[TableEntry {
    name: "AI:Embeddings",
    aliases: &[],
    file_stem: "AI:Embeddings",
    header: &[
        "Address",
        "Prompt",
        "Embedding Model",
        "Dimensions",
        "Embedding",
    ],
    header_fn: None,
    requires: &[],
    owner: "M4-E02",
    run: stub_run,
}];

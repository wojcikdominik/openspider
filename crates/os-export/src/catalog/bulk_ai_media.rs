//! STUB(M4-P03): AI export entries, replaced by M4-P03.
use super::{stub_run, TableEntry};
pub fn entries() -> &'static [TableEntry] {
    ENTRIES
}
const ENTRIES: &[TableEntry] = &[
    TableEntry {
        name: "AI:All Generated Images",
        aliases: &[],
        file_stem: "AI:All Generated Images",
        header: &["Address", "Prompt", "File"],
        header_fn: None,
        requires: &[],
        owner: "M4-P03",
        run: stub_run,
    },
    TableEntry {
        name: "AI:All Generated Audio",
        aliases: &[],
        file_stem: "AI:All Generated Audio",
        header: &["Address", "Prompt", "File"],
        header_fn: None,
        requires: &[],
        owner: "M4-P03",
        run: stub_run,
    },
];

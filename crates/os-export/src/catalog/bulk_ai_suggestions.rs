//! STUB(M4-V01): AI export entries, replaced by M4-V01.
use super::{stub_run, TableEntry};
pub fn entries() -> &'static [TableEntry] {
    ENTRIES
}
const ENTRIES: &[TableEntry] = &[
    TableEntry {
        name: "AI Suggestions:Accepted",
        aliases: &[],
        file_stem: "AI Suggestions:Accepted",
        header: &[
            "Address",
            "Element",
            "Current Value",
            "New Value",
            "Length",
            "Pixel Width",
            "Origin",
            "Model",
            "Generated At",
        ],
        header_fn: None,
        requires: &[],
        owner: "M4-V01",
        run: stub_run,
    },
    TableEntry {
        name: "AI Suggestions:All",
        aliases: &[],
        file_stem: "AI Suggestions:All",
        header: &["Address", "Element", "Suggestion", "Status", "Violations"],
        header_fn: None,
        requires: &[],
        owner: "M4-V01",
        run: stub_run,
    },
];

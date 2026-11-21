//! STUB(M4-D02): AI export entries, replaced by M4-D02.
use super::{stub_run, TableEntry};
pub fn entries() -> &'static [TableEntry] {
    ENTRIES
}
const ENTRIES: &[TableEntry] = &[TableEntry {
    name: "AI Decisions:All Answers",
    aliases: &[],
    file_stem: "AI Decisions:All Answers",
    header: &[
        "Address",
        "Question ID",
        "Question Version",
        "Type",
        "Value",
        "Probability",
        "Score",
        "Confidence",
        "Source",
        "Model",
        "Status",
    ],
    header_fn: None,
    requires: &[],
    owner: "M4-D02",
    run: stub_run,
}];

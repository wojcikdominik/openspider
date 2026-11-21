//! STUB(M4-M01): AI export entries, replaced by M4-M01.
use super::{stub_run, TableEntry};
pub fn entries() -> &'static [TableEntry] {
    ENTRIES
}
const ENTRIES: &[TableEntry] = &[TableEntry {
    name: "AI:Action Plan",
    aliases: &[],
    file_stem: "AI:Action Plan",
    header: &["Priority", "Action", "Evidence"],
    header_fn: None,
    requires: &[],
    owner: "M4-M01",
    run: stub_run,
}];

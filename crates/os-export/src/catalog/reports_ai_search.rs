//! STUB(M4-S02): AI export entries, replaced by M4-S02.
use super::{stub_run, TableEntry};
pub fn entries() -> &'static [TableEntry] {
    ENTRIES
}
const ENTRIES: &[TableEntry] = &[
    TableEntry {
        name: "AI:AI Crawler Accessibility",
        aliases: &[],
        file_stem: "AI:AI Crawler Accessibility",
        header: &["Address", "Crawler", "Status", "Block Type", "Note"],
        header_fn: None,
        requires: &[],
        owner: "M4-S02",
        run: stub_run,
    },
    TableEntry {
        name: "AI:AI Search Readiness",
        aliases: &[],
        file_stem: "AI:AI Search Readiness",
        header: &["Metric", "Value"],
        header_fn: None,
        requires: &[],
        owner: "M4-S02",
        run: stub_run,
    },
];

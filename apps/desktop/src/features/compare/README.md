# M2-U23 comparison UI

`TopTabs` mounts the selection/mapping controls and the comparison tabs. `UpperTable`, `SidebarFrame`, and
`DetailFrame` select the comparison components while a comparison is active. Closing restores their normal
single-crawl components. Both the Mode → Compare menu and the crawl-management two-selection action reach
this feature. New comparisons and role swaps clear the old selection and refresh every count.

The UI does not classify links or URLs. Rust's `Comparison` supplies the same bitmaps, mapped partners and link
rows that C01 exports use. `ComparisonSource` (a `ViewDataSource`) shows the complete population in one virtualized
grid (UI-140): it fetches 200-row windows on demand, there are no page buttons; selected IDs retain their crawl side.
Column kinds, widths and localized headers come from `catalog.columns` of the tab (UI-007); `Comparison Status` and
`Previous Address` follow `Address`. Every comparison panel fills its region like the ordinary table (B4-CMPUI). Auto link
source follows the upper source, including its mapped partner ID. Link identities and normalized anchor sets
are owned by C01. The three comparison columns are retained in CSV exports of the complete active projection.

The additive read contract uses existing command names and leaves generated IPC files unchanged:

- `sidebar.overview({ comparison: true })` adds `comparison: { overview, issues, notes }` to the ordinary response
  envelope. A missing count remains `null`, never a fabricated zero.
- `view.rows({ viewId, version, offset, limit, columns, comparison: { current, previous, tab, filter, side,
  class?, urlId?, source?, change?, search?, sort?: { column, dir: "asc" | "desc" } } })` opts into stateless comparison paging. Both crawl IDs and the
  comparison version must still match the calling window. The response adds `total`, `side`, `partnerKeys`, and
  `filtersEnabled` to the standard row-window fields. These reads run inside C01's blocking comparison worker.
- Calls without the opt-in fields retain the existing command behavior. There are no new registry commands.

`ui_change.rs` projects the selected Change Detection elements in batches of at most 500 paired internal HTML
URLs. It uses the ordinary column registry and URL counters, plus stored MinHash signatures for Content.
Missing Content data stays blank. Numeric percentages stay blank for a zero previous value.
Change Detection column headers toggle sorting in the core over the complete filtered population before paging.
Empty values sort as positive infinity (last ascending, first descending); equal values retain ascending URL ID order.
The sorting pass retains only one scalar sort value and ID per row, then projects the requested page in 500-row batches.
CSV uses the same sorted query. A link direction switch, a new sort or filter starts again at row 0.

Fixture contract tests live beside this feature; `e2e/compare/bridge.spec.ts` crawls two versions of a loopback
site through the actual bridge and tests both browser engines. The package report is
[`docs/plan/M2-reports/u23.md`](../../../../../docs/plan/M2-reports/u23.md).

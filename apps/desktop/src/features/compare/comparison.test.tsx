import { runAction } from "../../ui/actions";
import { ComparisonDetail } from "./ComparisonDetail";
import { comparisonCsv } from "./export";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createClient, setClient } from "../../ipc/client";
import { setLanguage } from "../../i18n";
import { ComparisonControls } from "./ComparisonControls";
import { ComparisonNumbers } from "./ComparisonNumbers";
import { ComparisonLinks } from "./ComparisonLinks";
import { ComparisonTable } from "./ComparisonTable";
import { UpperTable } from "../table/UpperTable";
import { useTabs } from "../../state/tabs";
import { closeComparison, comparisonRows, navigateComparison, openComparison, selectComparisonUrl, useComparison } from "./model";
import { comparisonFixture, current, previous, session, snapshot, pagedLinks } from "./fixtures/out-compare/core";
// Contract harness: preserve the existing population/number assertions independently
// of canvas geometry. Real DataGrid gestures are covered in comparison-grid.test.tsx.
vi.mock("../../grid/DataGrid", async importOriginal => {
  const actual = await importOriginal<typeof import("../../grid/DataGrid")>();
  return { ...actual, DataGrid: (props: import("../../grid/DataGrid").DataGridProps) =>
    <table aria-label={props.ariaLabel}><thead><tr>{props.columns.filter(c=>c.visible).map(c=><th key={c.key} aria-sort={c.sort === "asc" ? "ascending" : c.sort === "desc" ? "descending" : undefined}>
      <button type="button" onClick={()=>props.onSortChange([{column:c.key,dir:c.sort === "asc" ? "desc" : "asc"}])}>{c.key}</button>
    </th>)}</tr></thead><tbody>{Array.from({length:props.source.total},(_,row)=><tr key={row}>{props.columns.filter(c=>c.visible).map((c,index)=><td key={c.key}>{index===0 ?
      <button type="button" onClick={()=>{const key=props.source.rowKey(row);if(key!==undefined)props.onSelectionChange(props.selection.select(key));}}>{String(props.source.getCell(props.source.columns.findIndex(m=>m.key===c.key),row) ?? "")}</button> :
      String(props.source.getCell(props.source.columns.findIndex(m=>m.key===c.key),row) ?? "")}</td>)}</tr>)}</tbody></table> };
});
let fixture:ReturnType<typeof comparisonFixture>;
const calls=(cmd:string)=>fixture.core.calls.filter(c=>c.cmd===cmd);
beforeEach(async()=>{
  fixture=comparisonFixture();setClient(createClient(fixture.core.transport()));await setLanguage("en");
  useComparison.setState({dialog:false,session:null,snapshot:null,busy:false,error:null,revision:0,query:{tab:"Internal",filter:"All",side:"Current"},selected:null});
});
afterEach(()=>{cleanup();setClient(null);useComparison.setState({session:null,dialog:false,busy:false,error:null});vi.restoreAllMocks();});
function active(){useComparison.setState({session,snapshot});}
// spec: OUT-140
it("selects two distinct fixture crawls, defaults to the newer Current, and opens the core comparison",async()=>{
  render(<ComparisonControls />);fireEvent.click(screen.getByRole("button",{name:"Compare crawls"}));
  await waitFor(()=>expect(screen.getByLabelText("Current")).toHaveValue(current.crawlId));
  expect(screen.getByLabelText("Previous")).toHaveValue(previous.crawlId);
  fireEvent.change(screen.getByLabelText("Previous"),{target:{value:current.crawlId}});
  expect(screen.getByRole("button",{name:"Compare"})).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Previous"),{target:{value:previous.crawlId}});
  await waitFor(()=>expect(screen.getByRole("button",{name:"Compare"})).toBeEnabled());
  fireEvent.click(screen.getByRole("button",{name:"Compare"}));
  await waitFor(()=>expect(useComparison.getState().session).toEqual(session));
  expect(calls("compare.open")[0]?.args).toMatchObject({current:"current",previous:"previous",mapping:[]});
  expect(useComparison.getState().snapshot).toEqual(snapshot);
});
// spec: OUT-141
it("swaps through the core and refreshes all comparison data, then closes the session",async()=>{
  active();render(<ComparisonControls />);fireEvent.click(screen.getByRole("button",{name:"Swap Current / Previous"}));
  await waitFor(()=>expect(useComparison.getState().session?.current.crawlId).toBe("previous"));
  expect(calls("sidebar.overview")).toHaveLength(1);
  await act(()=>closeComparison());expect(calls("compare.close")).toHaveLength(1);expect(useComparison.getState().snapshot).toBeNull();
});
// spec: OUT-142
it("routes each Overview category and each source count to exactly that tab/filter population",()=>{
  active();render(<ComparisonNumbers />);
  const row=within(screen.getByRole("rowheader",{name:"Internal: HTML"}).closest("tr")!);
  const buttons=row.getAllByRole("button");
  for(const [i,category] of ["Added","New","Removed","Missing"].entries()){
    fireEvent.click(buttons[i+2]!);expect(useComparison.getState().query).toEqual({tab:"Internal",filter:"HTML",class:category,side:"Current"});
  }
  fireEvent.click(buttons[1]!);expect(useComparison.getState().query.side).toBe("Previous");
});
// spec: OUT-143
it("keeps Previous-only issues with Current zero, sorts Change both ways, and routes an issue count",()=>{
  active();render(<ComparisonNumbers issues />);
  let rows=screen.getAllByRole("row").slice(1);expect(rows[0]).toHaveTextContent("Page Titles: Missing");
  fireEvent.click(screen.getByRole("button",{name:"Change ↓"}));
  // UI-121: the row title uses the ordinary tab and filter names (en canon: „Client Error (4XX)“).
  rows=screen.getAllByRole("row").slice(1);expect(rows[0]).toHaveTextContent("Response Codes: Client Error (4XX)");
  expect(within(rows[0]!).getAllByRole("button")[0]).toHaveTextContent("0");
  fireEvent.click(within(rows[0]!).getAllByRole("button")[4]!);
  expect(useComparison.getState().query).toMatchObject({tab:"Response Codes",filter:"Client Error (4xx)",class:"Removed"});
});
// spec: OUT-146
it("tests ordered mappings through the core, blocks invalid patterns and allows rule reorder/removal",async()=>{
  render(<ComparisonControls />);fireEvent.click(screen.getByRole("button",{name:"Compare crawls"}));
  await screen.findByLabelText("Current");fireEvent.click(screen.getByRole("button",{name:"Add mapping rule"}));
  fireEvent.change(screen.getByLabelText("Pattern 1"),{target:{value:"["}});
  await screen.findByText("invalid regular expression at position 1");expect(screen.getByRole("button",{name:"Compare"})).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Pattern 1"),{target:{value:"first"}});
  fireEvent.click(screen.getByRole("button",{name:"Add mapping rule"}));fireEvent.change(screen.getByLabelText("Pattern 2"),{target:{value:"second"}});
  fireEvent.click(screen.getByRole("button",{name:"Move rule up 2"}));expect(screen.getByLabelText("Pattern 1")).toHaveValue("second");
  fireEvent.click(screen.getByRole("button",{name:"Remove rule 2"}));
  await waitFor(()=>expect(calls("compare.testMapping").at(-1)?.args).toMatchObject({rules:[{pattern:"second",replacement:""}]}));
});
// spec: CRAWL-144
it("Auto follows the upper source, explicit source overrides it, and link changes keep the canonical columns",async()=>{
  active();render(<ComparisonLinks direction="outlinks" urlId={1} />);
  await screen.findByText("Neu");expect(screen.queryByText("https://site.test/b")).toBeNull();
  act(()=>useComparison.setState({query:{tab:"Internal",filter:"All",side:"Previous"}}));await screen.findByText("https://site.test/b");
  fireEvent.change(screen.getByLabelText("Source"),{target:{value:"Current"}});await screen.findByText("Neu");
  fireEvent.change(screen.getByLabelText("Link change"),{target:{value:"Anchor Changed"}});
  await waitFor(()=>expect(screen.getAllByRole("row")).toHaveLength(2));expect(screen.getByText("Neu")).toBeVisible();expect(screen.getByText("Alt")).toBeVisible();
  for(const name of ["Link Change","Crawl","Previous Anchor"])expect(screen.getByRole("columnheader",{name})).toBeVisible();
});
// spec: OUT-149
it("shows removed links from Previous even while Current is the source, in both directions",async()=>{
  active();const mounted=render(<ComparisonLinks direction="inlinks" urlId={1} />);await screen.findByText("Neu");
  fireEvent.change(screen.getByLabelText("Link change"),{target:{value:"Removed"}});await screen.findByText("https://site.test/b");expect(screen.getByRole("cell",{name:"Previous"})).toBeVisible();
  expect(calls("view.rows").at(-1)?.args).toMatchObject({comparison:{tab:"inlinks",side:"Current",source:"Auto",change:"Removed"}});
  mounted.rerender(<ComparisonLinks direction="outlinks" urlId={1} />);await waitFor(()=>expect(calls("view.rows").at(-1)?.args).toMatchObject({comparison:{tab:"outlinks",change:"Removed"}}));
});
// spec: CRAWL-144
it("disables all change categories for a one-sided URL and still shows its existing links",async()=>{
  active();render(<ComparisonLinks direction="outlinks" urlId={99} />);await screen.findByText("https://site.test/a");
  for(const name of ["Added","Removed","Anchor Changed","Unchanged"])expect(screen.getByRole("option",{name})).toBeDisabled();
  expect(screen.getByRole("combobox",{name:"Link change"})).toBeDisabled();
});
// spec: TECH-301
// The tab itself is chosen in the ordinary tab bar (toptabs.test.tsx, K2-77).
it("sends the Change Detection element and Changed population to the paired view",async()=>{
  active();render(<ComparisonTable />);
  act(()=>navigateComparison("Change Detection","All"));fireEvent.change(screen.getByLabelText("Filter"),{target:{value:"Page Titles"}});
  fireEvent.change(screen.getByLabelText("Comparison status"),{target:{value:"changed"}});
  await waitFor(()=>expect(calls("view.rows").at(-1)?.args).toMatchObject({comparison:{tab:"Change Detection",filter:"Page Titles",class:"changed"}}));
});
// spec: CRAWL-144
it("ignores an old link response after a source switch",async()=>{
  active();let resolve!: (value:unknown)=>void;
  const normal=fixture.core.handlers.get("view.rows")!;let first=true;
  fixture.core.handlers.set("view.rows",args=>{if(first){first=false;return new Promise(r=>{resolve=r;});}return normal(args, {window:"main",core:fixture.core});});
  render(<ComparisonLinks direction="outlinks" urlId={1} />);
  fireEvent.change(screen.getByLabelText("Source"),{target:{value:"Previous"}});await screen.findByText("https://site.test/b");
  await act(async()=>{resolve({viewId:"late",version:1,offset:0,rowKeys:[],columns:["Address"],cells:[["late-response"]],total:1,side:"Current",filtersEnabled:true});});
  expect(screen.queryByText("late-response")).toBeNull();
});
// spec: OUT-140
it("serializes double submissions and preserves the current view when opening fails",async()=>{
  active();fixture.core.handlers.set("compare.open",async()=>{throw new Error("fixture failure");});
  await openComparison({current:"current",previous:"previous",mapping:[],elements:[]});expect(useComparison.getState().session).toEqual(session);expect(useComparison.getState().error).toBeTruthy();
  selectComparisonUrl(1,"Current");navigateComparison("Internal","All");expect(useComparison.getState().selected).toBeNull();
  await comparisonRows({tab:"Internal",filter:"All",side:"Previous"});expect(calls("view.rows").at(-1)?.args).toMatchObject({version:1,limit:100,comparison:{current:"current",previous:"previous",side:"Previous"}});
});

// spec: OUT-149
it("exports the active link projection with all three comparison columns and the previous anchor",async()=>{
  active();const csv=await comparisonCsv({tab:"outlinks",filter:"All",side:"Current",urlId:1,source:"Auto",change:"Anchor Changed"});
  expect(csv.split("\r\n")[0]).toBe("Source,Destination,Type,Anchor,Link Change,Crawl,Previous Anchor");
  expect(csv).toContain("https://site.test/c,Hyperlink,Neu,Anchor Changed,Current,Alt");
  expect(csv).not.toContain("https://site.test/b");
});

// spec: CRAWL-144
it("keeps the mapped partner selected when the upper source changes and Auto follows without another click",async()=>{
  active();render(<><ComparisonTable /><ComparisonDetail /></>);
  const table=await screen.findByRole("table",{name:"Comparison table"});
  fireEvent.click(within(table).getByRole("button",{name:"https://site.test/start"}));
  await screen.findByRole("region",{name:"inlinks"});
  fireEvent.change(screen.getAllByLabelText("Source")[0]!,{target:{value:"Previous"}});
  await waitFor(()=>expect(calls("view.rows").at(-1)?.args).toMatchObject({comparison:{tab:"inlinks",side:"Previous",urlId:101,source:"Auto"}}));
  expect(useComparison.getState().selected).toBe(101);
});
// spec: OUT-140
it("suppresses a second open while the first core comparison is pending",async()=>{
  let resolve!:(value:unknown)=>void;fixture.core.handlers.set("compare.open",()=>new Promise(r=>{resolve=r;}));
  const args={current:"current",previous:"previous",mapping:[],elements:[]};const first=openComparison(args);await openComparison(args);
  expect(calls("compare.open")).toHaveLength(1);resolve(session);await first;expect(useComparison.getState().busy).toBe(false);
});

// spec: OUT-140
it("reopens the selection from Mode → Compare after the dialog was cancelled",async()=>{
  render(<ComparisonControls />);act(()=>{runAction("mode.compare");});await screen.findByLabelText("Current");
  fireEvent.click(screen.getByRole("button",{name:"Cancel"}));await waitFor(()=>expect(screen.queryByLabelText("Current")).toBeNull());
  act(()=>{runAction("mode.compare");});await screen.findByLabelText("Current");
});

// spec: CRAWL-144
// spec: UI-140
it("shows all 105 Inlinks in one virtualized table and starts at row 0 again after switching to three Outlinks and back",async()=>{
  active();
  fixture.core.handlers.set("view.rows",args=>{
    const a=args as {comparison:{tab:"inlinks"|"outlinks"};offset:number;limit:number};
    const all=pagedLinks[a.comparison.tab];
    // Like the core: every row window carries one key per row (UI-142).
    const cells=all.slice(a.offset,a.offset+a.limit);
    return {viewId:"paged-links",version:1,offset:a.offset,columns:["Source","Destination"],rowKeys:cells.map((_,i)=>a.offset+i+1),cells,total:all.length,side:"Current",filtersEnabled:true};
  });
  const mounted=render(<ComparisonLinks direction="inlinks" urlId={1} />);
  await screen.findByText("https://site.test/in-0");
  // UI-140: one scrollable population instead of client pages; the row 101 is reachable without a page button.
  expect(screen.getByText("https://site.test/in-104")).toBeInTheDocument();
  expect(screen.getAllByRole("row")).toHaveLength(106);
  expect(screen.getByText("105 rows")).toBeVisible();
  expect(screen.queryByRole("button",{name:"Next page"})).toBeNull();
  expect(screen.queryByRole("button",{name:"Previous page"})).toBeNull();
  mounted.rerender(<ComparisonLinks direction="outlinks" urlId={1} />);
  await screen.findByText("https://site.test/out-0");
  expect(screen.getAllByRole("row")).toHaveLength(4);
  expect(screen.getByText("3 rows")).toBeVisible();
  expect(calls("view.rows").at(-1)?.args).toMatchObject({offset:0,comparison:{tab:"outlinks"}});
  mounted.rerender(<ComparisonLinks direction="inlinks" urlId={1} />);
  await screen.findByText("https://site.test/in-0");
  expect(calls("view.rows").at(-1)?.args).toMatchObject({offset:0,comparison:{tab:"inlinks"}});
  expect(screen.getAllByRole("row")).toHaveLength(106);
});
// spec: TECH-301
it("toggles Change % sorting through the core, reloads from row 0, and exports the sorted query",async()=>{
  active();navigateComparison("Change Detection","All");
  fixture.core.handlers.set("view.rows",args=>{
    const a=args as {offset:number;comparison:{sort?:{dir:string}}};
    const rows=a.comparison.sort?.dir==="desc" ? [["empty",null],["positive",50],["negative",-50]] : [["negative",-50],["positive",50],["empty",null]];
    return {viewId:"sort-fixture",version:1,offset:a.offset,columns:["Address","Word Count Change %"],cells:rows,rowKeys:[1,2,3],partnerKeys:[101,102,103],total:103,side:"Current",filtersEnabled:true};
  });
  render(<ComparisonTable />);
  await screen.findByRole("button",{name:"Word Count Change %"});
  expect(screen.queryByRole("button",{name:"Next page"})).toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"Word Count Change %"}));
  await waitFor(()=>expect(calls("view.rows").at(-1)?.args).toMatchObject({offset:0,comparison:{sort:{column:"Word Count Change %",dir:"asc"}}}));
  expect(screen.getByRole("columnheader",{name:"Word Count Change %"})).toHaveAttribute("aria-sort","ascending");
  fireEvent.click(screen.getByRole("button",{name:"Word Count Change %"}));
  await waitFor(()=>expect(calls("view.rows").at(-1)?.args).toMatchObject({offset:0,comparison:{sort:{column:"Word Count Change %",dir:"desc"}}}));
  expect(screen.getByRole("columnheader",{name:"Word Count Change %"})).toHaveAttribute("aria-sort","descending");
  expect(screen.getAllByRole("row")[1]).toHaveTextContent("empty");
  const query={...useComparison.getState().query};
  // The production numeric ordering is asserted against real crawl fixtures in u23_comparison.rs.
  fixture.core.handlers.set("view.rows",()=>({viewId:"export",version:1,offset:0,columns:["Address","Word Count Change %"],cells:[["empty",null],["positive",50],["negative",-50]],rowKeys:[1,2,3],total:3,side:"Current",filtersEnabled:true}));
  const csv=await comparisonCsv(query);
  expect(csv).toContain("empty,\r\npositive,50\r\nnegative,-50");
  expect(calls("view.rows").at(-1)?.args).toMatchObject({comparison:{sort:{column:"Word Count Change %",dir:"desc"}}});
});

// spec: UI-004
// spec: OUT-140
it("opens comparison with the accessible compact launcher beside normal crawl tabs",async()=>{
  render(<ComparisonControls compact />);
  const launcher=screen.getByRole("button",{name:"Compare crawls"});
  expect(launcher.textContent).toBe("");
  fireEvent.click(launcher);
  await screen.findByLabelText("Current");
  await waitFor(()=>expect(screen.getByLabelText("Previous")).toHaveValue(previous.crawlId));
  await waitFor(()=>expect(screen.getByRole("button",{name:"Compare"})).toBeEnabled());
  fireEvent.click(screen.getByRole("button",{name:"Compare"}));
  await waitFor(()=>expect(useComparison.getState().session).toEqual(session));
});

// spec: OUT-142
// spec: OUT-143
// spec: UI-121
// B4-CMPUI: the overview speaks the UI language (titles, column heads, numbers); clipped count columns get a visible hint.
it("shows the overview with localized titles and numbers and hints at clipped columns", async () => {
  await setLanguage("de");
  useComparison.setState({ session, snapshot: { overview: [{ tab: "Internal", filter: "All", label: "All", current: 4438, previous: 4811, change: -373, classes: [0, 616, 0, 989] }], issues: [], notes: [] } });
  const wide = vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockReturnValue(280);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(280);
  const mounted = render(<ComparisonNumbers />);
  const row = within(screen.getByRole("rowheader", { name: "Intern: Alle" }).closest("tr")!);
  expect(row.getAllByRole("cell").map(c => c.textContent)).toEqual(["4.438", "4.811", "-373", "0", "616", "0", "989"]);
  expect(screen.getAllByRole("columnheader").map(c => c.textContent)).toEqual(["Filter", "Current", "Previous", "Änderung", "Hinzugefügt", "Neu", "Entfernt", "Fehlend"]);
  expect(screen.queryByTestId("compare-scroll-hint")).toBeNull();
  // A sidebar narrower than the seven count columns: the table scrolls sideways and says so.
  wide.mockReturnValue(900); mounted.unmount(); render(<ComparisonNumbers />);
  expect(screen.getByTestId("compare-scroll-hint")).toHaveTextContent("Weitere Spalten rechts – horizontal scrollen");
});

// spec: OUT-149
// spec: OUT-142
// spec: UI-121
it("shows localized select values and keeps the canonical values for the core", async () => {
  active(); await setLanguage("de");
  render(<><ComparisonTable /><ComparisonLinks direction="outlinks" urlId={1} /></>);
  await screen.findByRole("table", { name: "Vergleich der Links" });
  const change = screen.getByRole("combobox", { name: "Linkänderung" }) as HTMLSelectElement;
  expect([...change.options].map(o => [o.value, o.textContent])).toEqual([["All", "Alle"], ["Added", "Hinzugefügt"], ["Removed", "Entfernt"], ["Anchor Changed", "Ankertext geändert"], ["Unchanged", "Unverändert"]]);
  const sources = screen.getAllByRole("combobox", { name: "Quelle" }) as HTMLSelectElement[];
  expect([...sources[1]!.options].map(o => [o.value, o.textContent])).toEqual([["Auto", "Automatisch"], ["Current", "Current"], ["Previous", "Previous"]]);
  const category = screen.getByRole("combobox", { name: "Vergleichsstatus" }) as HTMLSelectElement;
  expect([...category.options].map(o => [o.value, o.textContent])).toEqual([["", "Alle"], ["Added", "Hinzugefügt"], ["New", "Neu"], ["Removed", "Entfernt"], ["Missing", "Fehlend"]]);
  fireEvent.change(change, { target: { value: "Anchor Changed" } });
  await waitFor(() => expect(calls("view.rows").at(-1)?.args).toMatchObject({ comparison: { tab: "outlinks", change: "Anchor Changed" } }));
  const filter = screen.getByRole("combobox", { name: "Filter" }) as HTMLSelectElement;
  expect([...filter.options].find(o => o.value === "All")?.textContent).toBe("Alle");
});

// spec: UI-004
// spec: UI-150
// B4-CMPUI2 (A8): „Übersicht“ during a comparison shows the hint in the panel the tab bar names (`data-panel`).
it("shows the overview hint of a comparison inside the data panel", () => {
  active(); useTabs.setState({ view: "overview" });
  render(<UpperTable />);
  const panel = document.getElementById("data-panel");
  expect(panel).toHaveTextContent("Not available in comparison mode.");
  useTabs.setState({ view: "tabs" });
});

import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useEffect, useState } from "react";
import { createClient, setClient } from "../../../ipc/client";
import { IpcError } from "../../../ipc/errors";
import { useSession } from "../../../state/session";
import { App, registerMainRegions } from "../../../App";
import { eventBus } from "../../../ipc/events";
import { SelectionModel } from "../../../state/selection";
import { setLanguage } from "../../../i18n";
import type { DataGridProps } from "../../../grid/DataGrid";
import type { ViewSpec } from "../../../ipc/gen/ViewSpec";
import type { DetailActions } from "./types";
import { DetailFrame } from "./DetailFrame";
import deepPath from "./fixtures/u11-depth-contract.json";
import { detailFixtureCore } from "./fixtures/core";
import { DEFAULT_LINK_OPTIONS, linkSpec } from "../anchors/model";
import { URL_DETAIL_FIELDS } from "../url-details/UrlDetails";
import { headerPreview } from "../headers/Headers";
import { COOKIE_COLUMNS } from "../cookies/Cookies";
import { IMAGE_COLUMNS } from "../images/Images";
import { updateDetailState } from "./persistence";
import { normalizeDetailPreferences } from "./preferences";
import { DETAIL_NAVIGATE_EVENT, setDetailSelection } from "./selection";
import { DETAIL_OPEN_BROWSER_EVENT, requestBrowser } from "./browser";
import { DETAIL_TABS } from "./types";

// Keep the shared bounded data source; substitute only the canvas when testing the feature in jsdom.
vi.mock("../../../grid/DataGrid", async importOriginal => {
  const original = await importOriginal<typeof import("../../../grid/DataGrid")>();
  return { ...original, DataGrid: function FixtureGrid(p: DataGridProps) {
    const [, refresh] = useState(0);
    useEffect(() => p.source.subscribe(() => refresh(n => n + 1)), [p.source]);
    return <table aria-label={p.ariaLabel} data-total={p.source.total}><thead><tr>{p.columns.filter(c => c.visible).map(c => <th key={c.key}>{c.key}</th>)}</tr></thead><tbody>
      {Array.from({ length: Math.min(p.source.total, 4) }, (_, row) => <tr key={row} data-testid={`detail-row-${row}`}
        onClick={() => { const id = p.source.rowKey(row); if (id !== undefined) p.onSelectionChange(SelectionModel.empty().select(id)); }}
        onDoubleClick={() => { const id = p.source.rowKey(row); if (id !== undefined) p.onActivate(id, "From"); }}>
        {p.columns.filter(c => c.visible).map(c => <td key={c.key} onContextMenu={e => { e.preventDefault(); p.onContextMenu({ column: c.key, row, x: 1, y: 1 }); }}>{String(p.source.getCell(p.source.columns.findIndex(meta => meta.key === c.key), row) ?? "")}</td>)}
      </tr>)}
    </tbody></table>;
  } };
});
let fixture: ReturnType<typeof detailFixtureCore>;
let actions: DetailActions;
const selection = { urlIds: [1, 2, 3], focus: 2, topTab: "Internal", crawlId: "fixture" };
async function settle(ms = 100) { await act(async () => { await vi.advanceTimersByTimeAsync(ms); }); }
async function selectTab(name: string) { fireEvent.click(screen.getByRole("tab", { name, hidden: true })); await settle(); }
const opens = () => fixture.core.calls.filter(c => c.cmd === "view.open");
const lastSpec = () => (opens().at(-1)!.args as { spec: ViewSpec }).spec;
beforeEach(async () => {
  vi.useFakeTimers(); fixture = detailFixtureCore(); setClient(createClient(fixture.core.transport()));
  actions = { navigate: vi.fn(), back: vi.fn(), openBrowser: vi.fn(), export: vi.fn(), reportError: vi.fn() };
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: vi.fn().mockResolvedValue(undefined) } });
  await setLanguage("en");
});
afterEach(async () => { cleanup(); await settle(0); setClient(null); vi.useRealTimers(); vi.restoreAllMocks(); setDetailSelection({ urlIds: [] }); useSession.setState({ seq: -1, crawl: null, views: {} }); });

describe("detail frame and URL properties", () => {
  // spec: UI-018
  // spec: UI-145
  it("keeps detail panels stable across crawl progress ticks", async () => {
    const crawl = { crawlId: "fixture", mode: "spider" as const, phase: "running" as const,
      crawled: 10, queued: 90, discovered: 100, percent: 10, rate: 10, avgRate: 10,
      elapsedS: 1, pausedS: 0, activeThreads: 4,
      owner: { kind: "app" as const, live: true, controllable: true, pid: 1 } };
    act(() => useSession.setState({ crawl }));
    const panel = vi.fn(() => <p>Selected URL fixture</p>);
    render(<DetailFrame selection={selection} actions={actions} plugins={[{ id: "urlDetails", Component: panel }]} />);
    await settle();
    const before = panel.mock.calls.length;
    for (let i = 1; i <= 10; i++) {
      act(() => useSession.setState({ crawl: { ...crawl, crawled: 10 + i, percent: 10 + i, elapsedS: 1 + i } }));
      await settle(0);
    }
    expect(panel).toHaveBeenCalledTimes(before);
    // A new crawl still updates the panel and reloads its configuration.
    act(() => useSession.setState({ crawl: { ...crawl, crawlId: "next-fixture" } }));
    await settle();
    expect(panel.mock.calls.length).toBeGreaterThan(before);
  });

  // spec: UI-030
  it("keeps the shell default and registers details as a separate region outside the upper table", async () => {
    const scaffold = render(<App />); await settle();
    expect(screen.getByText("Select a URL to see its details.")).toBeInTheDocument();
    scaffold.unmount();
    registerMainRegions({ detail: DetailFrame });
    try {
      render(<App />); await settle();
      expect(screen.queryByText("Select a URL to see its details.")).toBeNull();
      expect(screen.getByRole("region", { name: "Details" }).closest("main")).toBeNull();
      expect(screen.getByRole("main")).toBeInTheDocument();
    } finally { registerMainRegions({ detail: undefined }); }
  });
  // spec: UI-030
  it("does not request data without a selection and preserves the standard tab order", async () => {
    render(<DetailFrame actions={actions} />); await settle();
    expect(screen.getByText("Select a row in the table above to see its details.")).toBeInTheDocument();
    expect(fixture.core.calls.filter(c => c.cmd.startsWith("detail.") || c.cmd.startsWith("view.")).length).toBe(0);
    const list = screen.getByRole("tablist"); expect(within(list).getAllByRole("tab", { hidden: true }).slice(0, 5).map(e => e.textContent)).toEqual(["URL Details", "Inlinks", "Outlinks", "Image Details", "Resources"]);
    expect(within(list).getAllByRole("tab", { hidden: true }).map(e => e.getAttribute("data-tab-id"))).toEqual(DETAIL_TABS);
  });
  // spec: UI-145
  it("debounces fast changes for 100 ms and only requests the visible tab", async () => {
    const { rerender } = render(<DetailFrame selection={selection} actions={actions} />);
    await settle(50); rerender(<DetailFrame selection={{ ...selection, focus: 3 }} actions={actions} />);
    await settle(99); expect(fixture.core.calls.filter(c => c.cmd === "detail.urlDetails")).toHaveLength(0);
    await settle(1); expect(fixture.core.calls.filter(c => c.cmd === "detail.urlDetails").map(c => c.args)).toEqual([{ urlId: 3 }]); expect(opens()).toHaveLength(0);
  });
  // spec: UI-031
  it("shows every canonical property in order including empty fields and navigates redirects", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle();
    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("rowheader").map(e => e.textContent)).toEqual(URL_DETAIL_FIELDS);
    expect(within(table).getByRole("rowheader", { name: "Content Type" }).parentElement?.textContent?.trim()).toBe("Content Type");
    fireEvent.click(screen.getByRole("button", { name: "https://site.test/final" })); expect(actions.navigate).toHaveBeenCalledWith({ address: "https://site.test/final", tab: "auto" });
  });
  // spec: UI-081
  it("displays the Link Score provided by the core", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle();
    expect(screen.getByRole("rowheader", { name: "Link Score" }).parentElement).toHaveTextContent("73");
  });
  // spec: UI-031
  it("copies a property value and its name through the property context menu", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle();
    const row = screen.getByRole("rowheader", { name: "Link Score" }).parentElement!;
    fireEvent.contextMenu(within(row).getByText("73")); await settle(0);
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy name and value" })); await settle(0);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("Link Score\t73");
  });
  // spec: UI-145
  it("rejects a late response from the previous selection", async () => {
    let complete!: (value: unknown) => void;
    fixture.core.handlers.set("detail.urlDetails", () => new Promise(resolve => { complete = resolve; }));
    const { rerender } = render(<DetailFrame selection={selection} actions={actions} />); await settle();
    rerender(<DetailFrame selection={{ ...selection, focus: 99 }} actions={actions} />);
    fixture.core.handlers.set("detail.urlDetails", () => [{ name: "Address", value: "current" }]);
    await act(async () => complete([{ name: "Address", value: "obsolete" }])); await settle();
    expect(screen.getByText("current")).toBeInTheDocument(); expect(screen.queryByText("obsolete")).toBeNull();
  });
  // spec: ONPAGE-192 [M2]
  it("uses the shared URL selection from any top tab and shows a non-applicable empty state", async () => {
    setDetailSelection({ ...selection, topTab: "Response Codes" });
    render(<DetailFrame actions={actions} />); await settle(); await selectTab("Inlinks");
    expect(lastSpec()).toMatchObject({ kind: "inlinks", urlIds: [1, 2, 3] });
    await act(async () => setDetailSelection({ ...selection, topTab: "Page Titles" })); await settle();
    await act(async () => setDetailSelection({ ...selection, related: false })); await settle();
    expect(document.querySelector('[data-empty-state="NO_SELECTION"]')).not.toBeNull();
  });
  // spec: UI-030
  it("persists reordered tabs after one second and preserves unrelated state", async () => {
    fixture.core.uiState = { version: 1, state: { unrelated: { x: 7 } } };
    render(<DetailFrame selection={selection} actions={actions} />); await settle();
    fireEvent.keyDown(screen.getByRole("tablist").parentElement!, { key: "ArrowRight", altKey: true, shiftKey: true }); await settle(1000);
    expect(fixture.core.uiState.state).toMatchObject({ unrelated: { x: 7 } });
    expect((fixture.core.uiState.state as { detail: { order: string[] } }).detail.order.slice(0, 2)).toEqual(["inlinks", "urlDetails"]);
    const normalized = normalizeDetailPreferences({ order: ["outlinks", "outlinks", "unknown", "crawlPath"], hidden: ["urlDetails", "cookies", "crawlPath"] });
    expect(normalized.order.slice(0, 4)).toEqual(["outlinks", "urlDetails", "inlinks", "images"]); expect(normalized.hidden).toEqual(["cookies"]);
    expect(normalized.order).toHaveLength(17); expect(normalized.order).not.toContain("crawlPath");
  });
});

describe("paged tables", () => {
  // spec: UI-145
  // spec: UI-030
  it("loads an explicit tab immediately but still debounces a new URL selection", async () => {
    const { rerender } = render(<DetailFrame selection={selection} actions={actions} />); await settle();
    fireEvent.click(screen.getByRole("tab", { name: "Inlinks", hidden: true }));
    await settle(0);
    expect(opens()).toHaveLength(1);
    const next = { ...selection, urlIds: [8], focus: 8 };
    rerender(<DetailFrame selection={next} actions={actions} />);
    await settle(99); expect(opens()).toHaveLength(1);
    await settle(1); expect(opens()).toHaveLength(2);
    expect(lastSpec()).toMatchObject({ urlIds: [8] });
    fireEvent.click(screen.getByRole("tab", { name: "Outlinks", hidden: true })); await settle(0);
    expect(opens()).toHaveLength(3);
    // Changing request urgency must not dispose an already retained panel.
    fireEvent.click(screen.getByRole("tab", { name: "Inlinks", hidden: true })); await settle(0);
    expect(opens()).toHaveLength(3);
    expect(screen.getByRole("table", { name: "Inlinks" })).toHaveTextContent("https://site.test/source");
    // Revisiting a previously urgent selection is still a new URL selection.
    rerender(<DetailFrame selection={selection} actions={actions} />);
    await settle(99); expect(opens()).toHaveLength(3);
    await settle(1); expect(opens()).toHaveLength(4);
  });

  // spec: UI-145
  it("sends the request of a clicked tab within the click, without waiting for a timer", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle();
    const before = opens().length;
    await act(async () => { fireEvent.click(screen.getByRole("tab", { name: "Inlinks", hidden: true })); await Promise.resolve(); });
    // No timer has been advanced: the request left with the click (microtask after its commit).
    expect(opens()).toHaveLength(before + 1);
    expect(lastSpec()).toMatchObject({ kind: "inlinks", urlIds: [1, 2, 3] });
  });

  // spec: UI-030
  // spec: UI-145
  it("does not re-render a hidden retained panel when another tab is clicked", async () => {
    const inlinks = vi.fn(() => <p>retained inlinks</p>);
    render(<DetailFrame selection={selection} actions={actions} plugins={[{ id: "inlinks", Component: inlinks }]} />); await settle();
    await selectTab("Inlinks");
    const shown = inlinks.mock.calls.length;
    expect(shown).toBeGreaterThan(0);
    await selectTab("Outlinks");
    await selectTab("URL Details");
    expect(inlinks).toHaveBeenCalledTimes(shown);
    await selectTab("Inlinks");
    expect(screen.getByText("retained inlinks")).toBeVisible();
    expect(inlinks).toHaveBeenCalledTimes(shown);
  });

  // spec: UI-032
  it("loads the union of selected URLs and keeps repeated inlinks as separate rows", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Inlinks");
    expect(lastSpec()).toMatchObject({ kind: "inlinks", urlIds: [1, 2, 3] });
    const table = screen.getByRole("table", { name: "Inlinks" }); expect(within(table).getAllByText("https://site.test/source")).toHaveLength(2);
    expect(within(table).queryByRole("columnheader", { name: "Link Crawlability" })).toBeNull();
    fireEvent.doubleClick(screen.getByTestId("detail-row-0")); await settle(0);
    expect(actions.navigate).toHaveBeenCalledWith({ address: "https://site.test/source", tab: "auto" });
  });
  // spec: UI-033
  it("shows external and uncrawlable destinations with status, Target and a crawlability filter", async () => {
    render(<DetailFrame selection={selection} actions={actions} storeUncrawlable />); await settle(); await selectTab("Outlinks");
    expect(screen.getByRole("columnheader", { name: "Target" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Link Crawlability" })).toBeInTheDocument();
    expect(screen.getAllByText("Uncrawlable")).toHaveLength(2);
    fireEvent.doubleClick(screen.getByTestId("detail-row-0")); await settle(0);
    expect(actions.navigate).toHaveBeenCalledWith({ address: "https://outside.test/a?x=2", tab: "auto" });
    fireEvent.keyDown(screen.getByRole("region", { name: "Details" }), { key: "ArrowLeft", altKey: true }); expect(actions.back).toHaveBeenCalledOnce();
  });
  // spec: ONPAGE-036
  it("displays the link's Target value", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Outlinks"); expect(screen.getByText("_blank")).toBeInTheDocument();
  });
  // spec: CRAWL-103
  it("displays Original URL alongside the rewritten destination", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Outlinks");
    expect(screen.getByText("https://outside.test/a?sid=1&x=2")).toBeInTheDocument(); expect(screen.getByText("https://outside.test/a?x=2")).toBeInTheDocument();
  });
  // spec: INTEG-084
  it("includes Link Path, custom Link Position and sends the position filter to the core", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Outlinks");
    expect(screen.getByRole("columnheader", { name: "Link Path" })).toBeInTheDocument(); expect(screen.getAllByText("Editorial").length).toBeGreaterThan(0);
    const spec = linkSpec("inlinks", [1], { ...DEFAULT_LINK_OPTIONS, position: "Editorial", excludeNofollow: true }, "", []);
    expect(spec).toMatchObject({ filter: { position: "Editorial", excludeNofollow: true } });
  });
  // spec: UI-034
  it("provides the Links/Anchors switch", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Inlinks");
    expect(screen.getByRole("button", { name: /View/ })).toBeInTheDocument();
  });
  // spec: TECH-146
  it("requests anchor grouping and deterministic sorting without folding case", () => {
    expect(linkSpec("outlinks", [1, 2], { ...DEFAULT_LINK_OPTIONS, mode: "anchors", excludeSelf: true }, "", [])).toMatchObject({
      kind: "outlinks", urlIds: [1, 2], filter: { mode: "anchors", excludeSelf: true, excludeNofollow: false }, sort: [{ column: "Links", dir: "desc" }, { column: "Anchor Text", dir: "asc" }],
    });
    expect(fixture.fixture.anchors.map(row => row["Anchor Text"])).toEqual(["Produkte", "produkte"]);
  });
  // spec: UI-145
  it("loads a bounded first window for one million links and retains it until unmount", async () => {
    fixture.options.total = 1_000_000; render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Inlinks");
    const reads = fixture.core.calls.filter(c => c.cmd === "view.rows").map(c => c.args as { offset: number; limit: number });
    expect(reads.length).toBeGreaterThan(0); expect(reads.every(read => read.limit <= 200 && read.offset < 1000)).toBe(true);
    await selectTab("URL Details"); expect(fixture.views.size).toBe(1);
    cleanup(); await settle(0); expect(fixture.views.size).toBe(0); expect(fixture.core.calls.some(c => c.cmd === "view.close")).toBe(true);
  });
  // spec: TECH-089
  it("shows annotation confirmation statuses and Address for a multi-selection in Hreflang", async () => {
    render(<DetailFrame selection={{ ...selection, topTab: "Hreflang" }} actions={actions} />); await settle();
    expect(lastSpec()).toMatchObject({ kind: "detail", detail: "hreflang", urlIds: [1, 2, 3] });
    expect(screen.getByRole("columnheader", { name: "Address" })).toBeInTheDocument(); expect(screen.getByText("Non-200")).toBeInTheDocument(); expect(screen.getByText("Confirmed")).toBeInTheDocument();
  });
  // spec: UI-036
  it("shows blocked rendering resources and the responsible block reason", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Resources");
    expect(lastSpec()).toMatchObject({ kind: "resources", urlIds: [1, 2, 3] }); expect(screen.getByText("robots.txt")).toBeInTheDocument(); expect(screen.getByText("Rendered HTML")).toBeInTheDocument();
  });
  // spec: UI-043
  it("shows focus cookies with default hidden Path/Max-Age and HTTP/JavaScript attribution", async () => {
    render(<DetailFrame selection={selection} actions={actions} rendering={false} />); await settle(); await selectTab("Cookies");
    expect(lastSpec()).toMatchObject({ kind: "detail", detail: "cookies", urlIds: [2] });
    expect(screen.queryByRole("columnheader", { name: "Max-Age" })).toBeNull(); expect(screen.queryByRole("columnheader", { name: "Path" })).toBeNull();
    expect(screen.getByText("JavaScript cookies require JavaScript rendering.")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "First-Party" })).toBeInTheDocument(); expect(screen.getByRole("columnheader", { name: "SameSite" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Export details" }));
    expect(actions.export).toHaveBeenCalledWith(expect.objectContaining({ detail: "cookies" }), { kind: "ids", ids: [2] });
    expect(COOKIE_COLUMNS).toContain("Max-Age");
  });
  // spec: UI-111
  it("uses full From/To values and the stored XPath in context actions", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Outlinks");
    fireEvent.contextMenu(screen.getByText("https://outside.test/a?x=2")); await settle(0);
    fireEvent.click(screen.getByRole("menuitem", { name: "Show link element in source" })); await settle(0);
    expect(actions.navigate).toHaveBeenCalledWith({ address: "https://site.test/redirect", tab: "auto", source: true, linkPath: "/html[1]/body[1]/main[1]/a[1]" });
  });
  // spec: UI-050
  it("shows a crawl path with redirects and the direct-upload hint", async () => {
    render(<DetailFrame selection={selection} actions={actions} listMode />); await settle();
    fireEvent.click(screen.getByRole("button", { name: "Crawl Path" })); await settle();
    expect(screen.getByText("HTTP Redirect")).toBeInTheDocument(); expect(screen.getByText(/Directly uploaded/)).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Crawl Path" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Close" })); await settle();
    expect(screen.getByRole("rowheader", { name: "Link Score" })).toBeInTheDocument();
    expect(fixture.core.calls.filter(c => c.cmd === "uistate.save")).toHaveLength(0);
  });
  // spec: OUT-023
  it("exports a crawl-path report for the selected URLs in selection order", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle();
    fireEvent.click(screen.getByRole("button", { name: "Crawl Path" })); await settle(); fireEvent.click(screen.getByRole("button", { name: "Export details" }));
    expect(actions.export).toHaveBeenCalledWith(expect.objectContaining({ kind: "detail", detail: "crawlPath", urlIds: [1, 2, 3] }), { kind: "ids", ids: [1, 2, 3] });
  });
});

describe("view lifecycle and preferences", () => {
  // spec: UI-030
  it("restores saved column order and widths for the matching detail table", async () => {
    fixture.core.uiState = { version: 1, state: { detailColumns: { "inlinks:links": [
      { key: "To", width: 250, pinned: false, visible: true },
      { key: "From", width: 180, pinned: false, visible: true },
    ] } } };
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Inlinks");
    expect(within(screen.getByRole("table", { name: "Inlinks" })).getAllByRole("columnheader").slice(0, 2).map(e => e.textContent)).toEqual(["To", "From"]);
  });
  // spec: UI-050
  it("shows all eight discovery steps for a URL at depth seven", async () => {
    fixture.fixture.crawlPath = deepPath;
    render(<DetailFrame selection={selection} actions={actions} />); await settle();
    fireEvent.click(screen.getByRole("button", { name: "Crawl Path" })); await settle();
    expect(screen.getByRole("table", { name: "Crawl Path" })).toHaveAttribute("data-total", "8");
    expect(fixture.fixture.crawlPath.at(-1)?.Address).toBe("https://site.test/");
  });
  // spec: UI-030
  it("merges simultaneous tab and column writes without losing unrelated UI state", async () => {
    fixture.core.uiState = { version: 1, state: { other: 42 } };
    await Promise.all([
      updateDetailState(state => ({ ...state, detail: { hidden: ["cookies"] } })),
      updateDetailState(state => ({ ...state, detailColumns: { inlinks: [] } })),
    ]);
    expect(fixture.core.uiState.state).toEqual({ other: 42, detail: { hidden: ["cookies"] }, detailColumns: { inlinks: [] } });
  });
  // spec: UI-145
  it("renders a building view when its row count arrives in a live tick", async () => {
    const open = fixture.core.handlers.get("view.open")!;
    fixture.core.handlers.set("view.open", async (args, ctx) => ({ ...await open(args, ctx) as object, total: null, state: "building" }));
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Inlinks");
    expect(screen.queryByRole("table", { name: "Inlinks" })).toBeNull();
    const viewId = [...fixture.views.keys()][0];
    await act(async () => useSession.setState({ views: { [viewId]: { viewId, version: 2, total: 2 } } }));
    await settle(0);
    expect(screen.getByRole("table", { name: "Inlinks" })).toBeInTheDocument();
    expect(screen.getByText("Mehr")).toBeInTheDocument();
  });
  // spec: UI-145
  it("closes a late retained view handle when the selection is removed", async () => {
    const open = fixture.core.handlers.get("view.open")!;
    let complete!: () => void;
    fixture.core.handlers.set("view.open", (args, ctx) => new Promise(resolve => { complete = () => resolve(open(args, ctx)); }));
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Inlinks");
    await selectTab("URL Details");
    cleanup();
    await act(async () => complete()); await settle(0);
    expect(fixture.views.size).toBe(0); expect(fixture.core.calls.filter(c => c.cmd === "view.close")).toHaveLength(1);
  });
  // spec: TECH-089
  it("shows the server's hreflang truncation count above the annotations", async () => {
    fixture.core.handlers.set("detail.urlDetails", () => [{ name: "Hreflang Truncated", value: 10 }]);
    render(<DetailFrame selection={{ ...selection, urlIds: [1], topTab: "Hreflang" }} actions={actions} />); await settle();
    expect(screen.getByText("10 additional annotations were not evaluated.")).toBeInTheDocument();
  });
  // spec: UI-030
  it("persists each detail table's column visibility without changing export columns", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Cookies");
    fireEvent.click(screen.getByText("Columns", { selector: "summary" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Cookie Value" })); await settle(1000);
    expect(screen.queryByRole("columnheader", { name: "Cookie Value" })).toBeNull();
    expect(fixture.core.uiState.state).toMatchObject({ detailColumns: { cookies: expect.arrayContaining([expect.objectContaining({ key: "Cookie Value", visible: false })]) } });
    await selectTab("URL Details"); await selectTab("Cookies");
    expect(screen.queryByRole("columnheader", { name: "Cookie Value" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Export details" }));
    expect(actions.export).toHaveBeenCalledWith(expect.objectContaining({ detail: "cookies" }), { kind: "ids", ids: [2] });
  });
});

describe("headers and images", () => {
  // spec: UI-035, UI-145
  it("keeps a row-selected preview and consent through crawl ticks and equal selections, but clears it for a new selection", async () => {
    fixture.options.stored = false;
    const { rerender } = render(<DetailFrame selection={selection} actions={actions} />);
    await settle(); await selectTab("Image Details");
    fireEvent.click(screen.getByTestId("detail-row-0")); await settle(0); await settle();
    fireEvent.click(screen.getByRole("button", { name: "Load image from server…" }));
    const image = screen.getByRole("img"); expect(image).toHaveAttribute("src", "ospider://img/fixture/7");
    const previewCalls = fixture.core.calls.filter(c => c.cmd === "detail.imagePreview").length;
    await act(async () => useSession.getState().applyTick({
      seq: 1, at: "2026-10-19T12:00:00Z", countsVersion: 0, issuesVersion: 0, sidebarVersion: 0, views: [], ops: [], notices: [],
      crawl: { crawlId: "fixture", phase: "running", mode: "spider", crawled: 10, queued: 20, discovered: 30, percent: 33, rate: 2, avgRate: 2, elapsedS: 5, pausedS: 0, activeThreads: 2, owner: { kind: "app", live: true, controllable: true } },
    }));
    rerender(<DetailFrame selection={{ ...selection, urlIds: [...selection.urlIds] }} actions={actions} />); await settle(500);
    expect(screen.getByRole("img")).toBe(image); expect(image).toHaveAttribute("src", "ospider://img/fixture/7");
    expect(screen.queryByRole("button", { name: "Load image from server…" })).toBeNull();
    expect(fixture.core.calls.filter(c => c.cmd === "detail.imagePreview")).toHaveLength(previewCalls);
    rerender(<DetailFrame selection={{ ...selection, focus: 3 }} actions={actions} />); await settle();
    expect(screen.queryByRole("img")).toBeNull();
  });
  // spec: UI-033, UI-043
  it("reloads rendering and storage capabilities on settingsChanged and unsubscribes on unmount", async () => {
    const listeners = eventBus.listenerCount("settingsChanged");
    const { unmount } = render(<DetailFrame selection={selection} actions={actions} />);
    await settle(); await selectTab("Cookies");
    expect(screen.queryByText("JavaScript cookies require JavaScript rendering.")).toBeNull();
    fixture.core.handlers.set("config.get", () => ({ config: { rendering: { mode: "text" }, extraction: { store_html: false }, spider: { store_uncrawlable_links: false } } }));
    await act(async () => eventBus.emit({ kind: "settingsChanged", keys: ["config"] })); await settle();
    expect(screen.getByText("JavaScript cookies require JavaScript rendering.")).toBeInTheDocument();
    await selectTab("Outlinks");
    fireEvent.contextMenu(screen.getByText("https://outside.test/a?x=2")); await settle(0);
    expect(screen.getByRole("menuitem", { name: "Show link element in source" })).toHaveAttribute("aria-disabled", "true");
    unmount(); expect(eventBus.listenerCount("settingsChanged")).toBe(listeners);
  });
  // spec: UI-042
  it("shows headers of the focus redirect preserving repeated names and searches without case sensitivity", async () => {
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("HTTP Headers");
    expect(screen.getByText("HTTP/2 301")).toBeInTheDocument(); expect(screen.getAllByText("set-cookie")).toHaveLength(3);
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "X-ROBOTS" } });
    expect(screen.getByText("noindex")).toBeInTheDocument(); expect(screen.queryByText("set-cookie")).toBeNull();
    expect(headerPreview("ä".repeat(5000))).toHaveLength(4097);
  });
  // spec: UI-030
  // spec: UI-016
  it("names the missing prerequisite instead of a generic error when header storage is off (A01-11)", async () => {
    fixture.core.handlers.set("detail.headers", () => { throw new IpcError({ code: "Unsupported", messageKey: "err-detail-unavailable", args: { state: { code: "FEATURE_DISABLED", arg: "http_headers", action: { kind: "openSetting", anchor: "extraction.http_headers" } } }, retryable: false }); });
    render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("HTTP Headers"); await settle();
    expect(screen.getByText(/HTTP headers are not stored/)).toBeInTheDocument();
    expect(screen.queryByText("This function is not available yet.")).toBeNull();
  });
  // spec: UI-035
  it("does not load an unstored image until explicit consent, using an img element for safe SVG display", async () => {
    fixture.options.stored = false; render(<DetailFrame selection={{ ...selection, topTab: "Images" }} actions={actions} />); await settle(); await selectTab("Image Details");
    expect(screen.queryByRole("img")).toBeNull(); fireEvent.click(screen.getByRole("button", { name: "Load image from server…" }));
    expect(screen.getByRole("img")).toHaveAttribute("src", "ospider://img/fixture/7"); expect(screen.queryByRole("object")).toBeNull();
  });
  // spec: ONPAGE-172
  it("shows image instances in parity order with empty non-rendered dimensions and hidden extensions", async () => {
    render(<DetailFrame selection={selection} actions={actions} rendering={false} />); await settle(); await selectTab("Image Details");
    const table = screen.getByRole("table", { name: "Image Details" });
    expect(within(table).getAllByRole("columnheader").map(e => e.textContent)).toEqual(IMAGE_COLUMNS);
    expect(screen.getByText("400x300")).toBeInTheDocument(); expect(screen.getByText("HTML")).toBeInTheDocument();
    expect(fixture.core.calls.some(c => c.cmd === "detail.imagePreview")).toBe(false);
  });
  // spec: UI-111
  it("publishes an address navigation request for the top table to resolve Internal/External", async () => {
    const listener = vi.fn(); document.addEventListener(DETAIL_NAVIGATE_EVENT, listener);
    render(<DetailFrame selection={selection} />); await settle(); fireEvent.click(screen.getByRole("button", { name: "https://site.test/final" }));
    expect(listener.mock.calls[0][0].detail).toEqual({ address: "https://site.test/final", tab: "auto" }); document.removeEventListener(DETAIL_NAVIGATE_EVENT, listener);
  });
  // spec: UI-035, UI-111
  it("routes image and From/To browser actions through the native handoff in Tauri", async () => {
    setClient(createClient({ ...fixture.core.transport(), name: "tauri" }));
    const popup = vi.spyOn(window, "open").mockReturnValue(null);
    const listener = vi.fn(); document.addEventListener(DETAIL_OPEN_BROWSER_EVENT, listener);
    const { openBrowser: _openBrowser, ...otherActions } = actions;
    const { rerender } = render(<DetailFrame selection={selection} actions={otherActions} />);
    try {
      await settle(); await selectTab("Outlinks");
      fireEvent.contextMenu(screen.getByText("https://outside.test/a?x=2")); await settle(0);
      fireEvent.click(screen.getByRole("menuitem", { name: "Open To in browser" })); await settle(0);
      expect(listener.mock.calls[0][0].detail).toEqual({ address: "https://outside.test/a?x=2" });
      rerender(<DetailFrame selection={{ ...selection, urlIds: [7], topTab: "Images", focus: 7 }} actions={otherActions} />);
      await selectTab("Image Details");
      fireEvent.click(screen.getByRole("button", { name: "Open original in browser" }));
      expect(listener.mock.calls[1][0].detail).toEqual({ address: "https://site.test/image.svg" });
      expect(popup).not.toHaveBeenCalled();
    } finally { document.removeEventListener(DETAIL_OPEN_BROWSER_EVENT, listener); }
  });
  // spec: UI-111
  it("allows a consumed browser request to suppress the mock/bridge popup fallback and rejects non-web URLs", () => {
    const popup = vi.spyOn(window, "open").mockReturnValue(null);
    requestBrowser("https://site.test/"); expect(popup).toHaveBeenCalledOnce();
    const listener = vi.fn((event: Event) => event.preventDefault());
    document.addEventListener(DETAIL_OPEN_BROWSER_EVENT, listener);
    try {
      requestBrowser("https://site.test/handled"); requestBrowser("javascript:alert(1)"); requestBrowser("file:///local");
      expect(listener).toHaveBeenCalledOnce(); expect(popup).toHaveBeenCalledOnce();
    } finally { document.removeEventListener(DETAIL_OPEN_BROWSER_EVENT, listener); }
  });
});

// spec: UI-030
// spec: UI-032
// spec: UI-033
// spec: UI-145
it("loads the first bounded window before publishing the detail grid", async () => {
  const original = fixture.core.handlers.get("view.rows")!;
  let complete!: () => void;
  fixture.core.handlers.set("view.rows", (args, ctx) => new Promise(resolve => {
    complete = () => resolve(original(args, ctx));
  }));
  render(<DetailFrame selection={selection} actions={actions} />); await settle();
  await selectTab("Inlinks");
  expect(screen.queryByRole("table", { name: "Inlinks" })).toBeNull();
  await act(async () => complete()); await settle(0);
  expect(screen.getByRole("table", { name: "Inlinks" })).toHaveTextContent("https://site.test/source");
});

// spec: UI-030
// spec: UI-032
// spec: UI-033
// spec: UI-145
it("retains loaded link rows immediately when returning to a detail tab", async () => {
  render(<DetailFrame selection={selection} actions={actions} />); await settle();
  await selectTab("Inlinks"); await settle();
  const count = opens().length;
  await selectTab("Outlinks"); await settle();
  fireEvent.click(screen.getByRole("tab", { name: "Inlinks", hidden: true }));
  expect(screen.getByRole("table", { name: "Inlinks" })).toBeInTheDocument();
  expect(opens()).toHaveLength(count + 1);
});

// spec: UI-018
// spec: UI-030
// spec: UI-032
it("refreshes retained links after the final crawl tick happened while the tab was hidden", async () => {
  render(<DetailFrame selection={selection} actions={actions} />); await settle();
  await selectTab("Inlinks"); await settle();
  const incoming = [...fixture.views.keys()][0];
  await selectTab("Outlinks"); await settle();
  act(() => useSession.setState({ countsVersion: 123, sidebarVersion: 123 }));
  await selectTab("Inlinks"); await settle(500);
  expect(fixture.core.calls.some(call => call.cmd === "view.update" && (call.args as { viewId: string }).viewId === incoming)).toBe(true);
});

// spec: UI-030
// spec: UI-145
it("defers a retained tab's debounced request until that tab is visible again", async () => {
  render(<DetailFrame selection={selection} actions={actions} />); await settle();
  fireEvent.click(screen.getByRole("tab", { name: "Inlinks", hidden: true }));
  fireEvent.click(screen.getByRole("tab", { name: "Outlinks", hidden: true }));
  await settle(200);
  expect(opens().map(call => (call.args as { spec: ViewSpec }).spec.kind)).toEqual(["outlinks"]);
  await selectTab("Inlinks"); await settle(100);
  expect(opens().map(call => (call.args as { spec: ViewSpec }).spec.kind)).toEqual(["outlinks", "inlinks"]);
});

// spec: UI-030
// spec: UI-145
it("retains a late hidden handle but waits until return before reading its rows", async () => {
  const original = fixture.core.handlers.get("view.open")!;
  let complete!: () => void;
  fixture.core.handlers.set("view.open", (args, ctx) => (args as { spec: ViewSpec }).spec.kind === "inlinks"
    ? new Promise(resolve => { complete = () => resolve(original(args, ctx)); }) : original(args, ctx));
  render(<DetailFrame selection={selection} actions={actions} />); await settle(); await selectTab("Inlinks");
  await selectTab("Outlinks");
  await act(async () => complete()); await settle(0);
  const incoming = [...fixture.views.keys()].find(id => id !== [...fixture.views.keys()][0])!;
  const incomingReads = () => fixture.core.calls.filter(call => call.cmd === "view.rows" && (call.args as { viewId: string }).viewId === incoming);
  expect(incomingReads()).toHaveLength(0);
  await selectTab("Inlinks"); await settle(100);
  expect(incomingReads().length).toBeGreaterThan(0);
});

// spec: UI-018
// spec: UI-030
// spec: UI-032
it("retries a final revision discarded when a pending update's tab becomes hidden", async () => {
  render(<DetailFrame selection={selection} actions={actions} />); await settle();
  await selectTab("Inlinks"); await settle();
  const incoming = [...fixture.views.keys()][0];
  const record = fixture.views.get(incoming)!;
  const originalRow = record.rows[0];
  const updatedAddress = "https://site.test/final-crawl-tick";
  let complete!: () => void;
  let updates = 0;
  fixture.core.handlers.set("view.update", raw => {
    const { viewId } = raw as { viewId: string };
    const handle = { viewId, version: 2, total: record.rows.length, state: "ready" };
    if (viewId !== incoming) return handle;
    updates++;
    if (updates === 1) return new Promise(resolve => { complete = () => resolve(handle); });
    return handle;
  });
  act(() => useSession.setState(state => ({ countsVersion: state.countsVersion + 1 })));
  await settle(500);
  expect(updates).toBe(1);
  await selectTab("Outlinks");
  record.rows = [{ ...originalRow, From: updatedAddress }, ...record.rows.slice(1)];
  await act(async () => complete()); await settle(0);
  // No further count/sidebar revision arrives after the last crawl tick.
  await selectTab("Inlinks"); await settle(1000);
  expect(updates).toBe(2);
  expect(within(screen.getByRole("table", { name: "Inlinks" })).getByText(updatedAddress)).toBeInTheDocument();
}, 5000);

// spec: UI-018
// spec: UI-030
it("clears a failed refresh message when the next 500-ms refresh succeeds", async () => {
  render(<DetailFrame selection={selection} actions={actions} />); await settle();
  await selectTab("Inlinks"); await settle();
  const incoming = [...fixture.views.keys()][0];
  const record = fixture.views.get(incoming)!;
  let updates = 0;
  fixture.core.handlers.set("view.update", () => {
    if (++updates === 1) throw new IpcError({ code: "Io", messageKey: "err-io", args: {}, detail: "fixture refresh failure", retryable: true });
    return { viewId: incoming, version: 2, total: record.rows.length, state: "ready" };
  });
  act(() => useSession.setState(state => ({ countsVersion: state.countsVersion + 1 })));
  await settle(500);
  expect(screen.getByRole("alert")).toHaveTextContent("fixture refresh failure");
  await settle(500);
  expect(updates).toBe(2);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

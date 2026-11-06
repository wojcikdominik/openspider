import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { uistate } from "../../ipc/uistate";
import type { RuleNode } from "../../ipc/gen/RuleNode";
import type { ViewSpec } from "../../ipc/gen/ViewSpec";
import { setLanguage } from "../../i18n";
import { applyTemplateColumns, normalizeColumns, useColumns } from "../../state/columns";
import { SelectionModel } from "../../state/selection";
import { useSession } from "../../state/session";
import { useTabs, type RuleQuery } from "../../state/tabs";
import { flushUiState } from "../../state/uistate";
import { contrast } from "../../ui/tokens/color";
import { segmentLabelColor } from "../../ui/tokens/tokens";
import { UpperTable } from "../table/UpperTable";
import { TopTabs } from "../toptabs/TopTabs";
import { ColumnConfigurator, moveKeys, nextSelection, reorderVisible } from "./ColumnConfigurator";
import { advancedContextEntries } from "./contextEntries";
import { advancedFixture, advancedTabs, internalColumns, segmentDefs } from "./fixtures/core";
import { appendAt, depth, emptyCond, formatRule, normalizeNumber, operatorsFor, pushHistory, removeAt } from "./query";
import { useSearchHistory, useSegments } from "./stores";
import { TabsConfigDialog, tabContextEntries } from "./TabControls";

const capture = vi.hoisted(() => ({ grid: undefined as DataGridProps | undefined }));
vi.mock("../../grid/DataGrid", async original => {
  const lib = await original<typeof import("../../grid/DataGrid")>();
  return { ...lib, DataGrid: (props: DataGridProps) => { capture.grid = props; return <div data-testid="grid" />; } };
});
let fx: ReturnType<typeof advancedFixture>;
const t = (key: string, args?: Record<string, unknown>) => args ? `${key}:${JSON.stringify(args)}` : key;
beforeEach(async () => {
  fx = advancedFixture(); setClient(createClient(fx.core.transport()));
  useTabs.setState({ catalog: advancedTabs, order: advancedTabs.map(x => x.id), active: "Internal", hidden: [], queries: {}, loaded: true, focusId: undefined, focusRequest: 0, error: null });
  useColumns.setState({ metadata: {}, byTab: {}, templates: [] });
  useSegments.setState({ segments: [], filter: null, error: null, revision: 0 });
  useSearchHistory.setState({ entries: [] });
  useSession.setState({ ops: {} }); capture.grid = undefined;
  await setLanguage("en");
});
afterEach(async () => { await flushUiState(); cleanup(); setClient(null); vi.restoreAllMocks(); });
const tabSpecs = () => fx.specs().filter((s): s is Extract<ViewSpec, { kind: "tab" }> => s.kind === "tab");
const lastSpec = () => tabSpecs().at(-1)!;
async function mounted() { render(<UpperTable />); await screen.findByTestId("grid"); await waitFor(() => expect(capture.grid?.source.total).toBe(5)); }
const counter = () => screen.getByText(/Showing:/);
const select = (label: string, value: string, scope: HTMLElement = document.body) => fireEvent.change(within(scope).getAllByLabelText(label).at(-1)!, { target: { value } });

describe("advanced search", () => {
  // spec: UI-011 [P1]
  it("prints the rule tree as text syntax and edits nested AND/OR groups of at least two levels", () => {
    const tree: RuleNode = { kind: "group", op: "and", children: [{ kind: "cond", column: "Address", op: "contains", value: "/blog/" },
      { kind: "group", op: "or", children: [{ kind: "cond", column: "Word Count", op: "lessThan", value: "200" }, { kind: "cond", column: null, op: "isEmpty", value: null }] }] };
    expect(formatRule(tree)).toBe('Address ~ "/blog/" AND ("Word Count" < "200" OR * IS EMPTY)');
    expect(formatRule({ kind: "cond", column: "Address", op: "regex", value: '/([^/]+)/\\1/"' })).toBe('Address REGEX "/([^/]+)/\\\\1/\\""');
    expect(depth(tree)).toBe(2);
    const deeper = appendAt(tree, [1], { kind: "group", op: "and", children: [emptyCond()] });
    expect(depth(deeper)).toBe(3);
    expect(depth(removeAt(deeper, [1, 2]))).toBe(2);
  });

  // spec: UI-011 [P1]
  it("offers numeric operators only on numeric columns and parses numbers in the UI language", () => {
    const text = internalColumns.find(c => c.key === "Title 1")!; const number = internalColumns.find(c => c.key === "Word Count")!;
    expect(operatorsFor(text)).not.toContain("greaterThan"); expect(operatorsFor(text)).not.toContain("lessThan");
    expect(operatorsFor(null)).not.toContain("lessThan");
    expect(operatorsFor(number)).toEqual(expect.arrayContaining(["greaterThan", "lessThan"]));
    expect(operatorsFor(number)).not.toContain("regex");
    expect(normalizeNumber("1,5", "de")).toBe("1.5"); expect(normalizeNumber("1.5", "de")).toBe("1.5");
    expect(normalizeNumber("1.5", "en")).toBe("1.5"); expect(normalizeNumber("1,5", "en")).toBeNull(); expect(normalizeNumber("abc", "de")).toBeNull();
  });

  // spec: UI-011 [P1]
  it("keeps the last 20 searches, most recent first and without duplicates", () => {
    const entry = (i: number): RuleQuery => ({ text: `Address ~ "${i}"`, caseSensitive: false, tree: { kind: "cond", column: "Address", op: "contains", value: String(i) } });
    let history: RuleQuery[] = [];
    for (let i = 0; i < 25; i++) history = pushHistory(history, entry(i));
    expect(history).toHaveLength(20); expect(history[0].text).toBe('Address ~ "24"');
    history = pushHistory(history, entry(10));
    expect(history).toHaveLength(20); expect(history[0].text).toBe('Address ~ "10"');
    expect(history.filter(h => h.text === 'Address ~ "10"')).toHaveLength(1);
  });

  // spec: UI-011 [P1]
  it("builds the rule „Word Count < 50 OR Status Code = 404“ in the tree editor, runs it in the core and records it", async () => {
    await mounted();
    fireEvent.click(screen.getByRole("button", { name: "Advanced search" }));
    const panel = await screen.findByRole("region", { name: "Advanced search" });
    expect(within(panel).getByRole("checkbox", { name: "Case sensitive" })).not.toBeChecked();
    select("Match", "or", panel);
    select("Column", "Word Count", panel); select("Operator", "lessThan", panel);
    fireEvent.change(within(panel).getByLabelText("Value"), { target: { value: "50" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Add condition" }));
    select("Column", "Status Code", panel); select("Operator", "equals", panel);
    fireEvent.change(within(panel).getAllByLabelText("Value").at(-1)!, { target: { value: "404" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Search" }));
    await waitFor(() => expect(capture.grid!.source.total).toBe(3));
    expect(lastSpec().search).toEqual({ kind: "rules", caseSensitive: false, tree: { kind: "group", op: "or", children: [
      { kind: "cond", column: "Word Count", op: "lessThan", value: "50" }, { kind: "cond", column: "Status Code", op: "equals", value: "404" }] } });
    expect(screen.getByLabelText("Search query (text syntax)")).toHaveValue('"Word Count" < "50" OR "Status Code" = "404"');
    expect(counter()).toHaveTextContent("Search active");
    await waitFor(() => expect(useSearchHistory.getState().entries[0]?.text).toBe('"Word Count" < "50" OR "Status Code" = "404"'));
    await flushUiState();
    expect((await uistate.load()).state).toMatchObject({ searchHistory: [{ text: '"Word Count" < "50" OR "Status Code" = "404"' }] });
  });

  // spec: UI-011 [P1]
  it("translates typed text syntax through search.parse and leaves the view unchanged on syntax errors", async () => {
    await mounted();
    fireEvent.click(screen.getByRole("button", { name: "Advanced search" }));
    const field = screen.getByLabelText("Search query (text syntax)");
    const opened = tabSpecs().length;
    fireEvent.change(field, { target: { value: "Address ~" } });
    fireEvent.keyDown(field, { key: "Enter", code: "Enter", charCode: 13 });
    expect(await screen.findByText("Query syntax error at character 9")).toBeInTheDocument();
    expect(field.closest("[data-invalid]")).not.toBeNull();
    expect(tabSpecs()).toHaveLength(opened);
    fireEvent.change(field, { target: { value: 'Address ~ "/blog/" AND "Word Count" < 200' } });
    fireEvent.keyDown(field, { key: "Enter", code: "Enter", charCode: 13 });
    await waitFor(() => expect(lastSpec().search?.kind).toBe("rules"));
    expect(fx.core.calls.filter(c => c.cmd === "search.parse").map(c => (c.args as { text: string }).text)).toContain('Address ~ "/blog/" AND "Word Count" < 200');
    await waitFor(() => expect(capture.grid!.source.total).toBe(1));
    const panel = screen.getByRole("region", { name: "Advanced search" });
    expect(within(panel).getAllByLabelText("Column").map(e => (e as HTMLSelectElement).value)).toEqual(["Address", "Word Count"]);
  });

  // spec: UI-011 [P1]
  it("marks an invalid regex red with the core's message and does not touch the current view", async () => {
    await mounted();
    fireEvent.click(screen.getByRole("button", { name: "Advanced search" }));
    const panel = screen.getByRole("region", { name: "Advanced search" });
    select("Operator", "regex", panel);
    fireEvent.change(within(panel).getByLabelText("Value"), { target: { value: "(unclosed" } });
    const opened = tabSpecs().length;
    fireEvent.click(within(panel).getByRole("button", { name: "Search" }));
    expect(await within(panel).findByText("Invalid regular expression: unclosed group")).toBeInTheDocument();
    expect(within(panel).getByLabelText("Value")).toHaveAttribute("data-invalid", "true");
    expect(tabSpecs()).toHaveLength(opened); expect(capture.grid!.source.total).toBe(5);
    expect(fx.core.calls.some(c => c.cmd === "search.validateRegex")).toBe(true);
  });

  // spec: UI-011 [P1]
  it("searches case-insensitively by default and case-sensitively on request, supporting back references", async () => {
    await mounted();
    act(() => useTabs.getState().query({ search: "blog" }));
    await waitFor(() => expect(capture.grid!.source.total).toBe(3));
    fireEvent.click(screen.getByRole("button", { name: "Advanced search" }));
    const panel = screen.getByRole("region", { name: "Advanced search" });
    fireEvent.change(within(panel).getByLabelText("Value"), { target: { value: "Blog" } });
    fireEvent.click(within(panel).getByRole("checkbox", { name: "Case sensitive" }));
    fireEvent.click(within(panel).getByRole("button", { name: "Search" }));
    await waitFor(() => expect(capture.grid!.source.total).toBe(1));
    expect(lastSpec().search).toMatchObject({ kind: "rules", caseSensitive: true });
    select("Operator", "regex", panel);
    fireEvent.change(within(panel).getByLabelText("Value"), { target: { value: "/([^/]+)/\\1/" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Search" }));
    await waitFor(() => expect(lastSpec().search).toMatchObject({ tree: { children: [{ op: "regex" }] } }));
    await waitFor(() => expect(capture.grid!.source.rowKey(0)).toBe(4));
    expect(capture.grid!.source.total).toBe(1);
  });

  // spec: UI-011 [P1]
  it("restores a search from the history and reports too expensive expressions", async () => {
    await mounted();
    const tree: RuleNode = { kind: "cond", column: "Address", op: "regex", value: "(a+)+$" };
    useSearchHistory.setState({ entries: [{ text: 'Address REGEX "(a+)+$"', caseSensitive: false, tree }] });
    fireEvent.click(screen.getByRole("button", { name: "Advanced search" }));
    const panel = screen.getByRole("region", { name: "Advanced search" });
    select("Recent searches", "0", panel);
    expect(within(panel).getByLabelText("Query")).toHaveValue('Address REGEX "(a+)+$"');
    expect(within(panel).getByLabelText("Value")).toHaveValue("(a+)+$");
    fireEvent.click(within(panel).getByRole("button", { name: "Search" }));
    expect(await screen.findByText("Expression too expensive – the search was stopped")).toBeInTheDocument();
  });
});

describe("column configurator", () => {
  // spec: UI-009
  it("moves several selected columns in one step and keeps Address first", () => {
    const keys = ["Address", "A", "B", "C", "D"];
    expect(moveKeys(keys, new Set(["C", "D"]), "top")).toEqual(["Address", "C", "D", "A", "B"]);
    expect(moveKeys(keys, new Set(["A", "C"]), "down")).toEqual(["Address", "B", "A", "D", "C"]);
    expect(moveKeys(keys, new Set(["A", "B"]), "up")).toEqual(keys);
    expect(moveKeys(keys, new Set(["A"]), "bottom")).toEqual(["Address", "B", "C", "D", "A"]);
    expect([...nextSelection(["A", "B", "C", "D"], new Set(["A"]), "A", "C", { shift: true, toggle: false })]).toEqual(["A", "B", "C"]);
    expect([...nextSelection(["A", "B", "C"], new Set(["A"]), "A", "C", { shift: false, toggle: true })]).toEqual(["A", "C"]);
    expect([...nextSelection(["A", "B", "C"], new Set(["A"]), "A", "B", { shift: false, toggle: false })]).toEqual(["B"]);
  });

  // spec: UI-008 [P1]
  it("re-adds a hidden column at its last position", () => {
    const columns = normalizeColumns(internalColumns.slice(0, 5));
    const hidden = columns.map(c => c.key === "Status Code" ? { ...c, visible: false } : c);
    const moved = reorderVisible(hidden, ["Address", "Title 1", "Word Count"]);
    expect(moved.map(c => c.key)).toEqual(["Row", "Address", "Status Code", "Title 1", "Word Count"]);
    expect(moved.find(c => c.key === "Status Code")!.visible).toBe(false);
  });

  // spec: UI-009
  it("filters both lists, multi-selects with Shift/Ctrl, applies on OK and discards on Cancel", async () => {
    await useColumns.getState().load("Internal");
    const { rerender } = render(<ColumnConfigurator tab="Internal" tabs={advancedTabs.map(x => ({ id: x.id, label: x.id }))} isOpen onOpenChange={() => undefined} label={id => id} />);
    const available = await screen.findByRole("listbox", { name: "Available columns" });
    const displayed = screen.getByRole("listbox", { name: "Displayed columns" });
    expect(within(available).getAllByRole("option").length + within(displayed).getAllByRole("option").length).toBeGreaterThanOrEqual(60);
    fireEvent.change(screen.getByLabelText("Find columns"), { target: { value: "METRIC 0" } });
    expect(within(displayed).getAllByRole("option").map(o => o.textContent)).toEqual(["Metric 01", "Metric 02", "Metric 03", "Metric 04"]);
    expect(within(available).getAllByRole("option").map(o => o.textContent)).toEqual(["Metric 05", "Metric 06", "Metric 07", "Metric 08", "Metric 09"]);
    fireEvent.click(within(available).getByRole("option", { name: "Metric 05" }));
    fireEvent.click(within(available).getByRole("option", { name: "Metric 07" }), { shiftKey: true });
    fireEvent.click(within(available).getByRole("option", { name: "Metric 09" }), { ctrlKey: true });
    expect(within(available).getAllByRole("option", { selected: true }).map(o => o.textContent)).toEqual(["Metric 05", "Metric 06", "Metric 07", "Metric 09"]);
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("button", { name: "To top" }));
    fireEvent.change(screen.getByLabelText("Find columns"), { target: { value: "" } });
    expect(within(displayed).getAllByRole("option").slice(0, 5).map(o => o.textContent)).toEqual(["Address", "Metric 05", "Metric 06", "Metric 07", "Metric 09"]);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(useColumns.getState().byTab.Internal.filter(c => c.visible).map(c => c.key)).not.toContain("Metric 05");
    rerender(<ColumnConfigurator tab="Internal" tabs={[]} isOpen={false} onOpenChange={() => undefined} label={id => id} />);
    rerender(<ColumnConfigurator tab="Internal" tabs={[]} isOpen onOpenChange={() => undefined} label={id => id} />);
    const list = await screen.findByRole("listbox", { name: "Available columns" });
    fireEvent.click(within(list).getByRole("option", { name: "Metric 10" }));
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(useColumns.getState().byTab.Internal.filter(c => c.visible).map(c => c.key)).toContain("Metric 10"));
  });

  // spec: UI-009
  // spec: ONPAGE-011 [M2]
  it("restores defaults for the current tab only and labels columns of switched-off elements", async () => {
    await useColumns.getState().load("Internal"); await useColumns.getState().load("External");
    await useColumns.getState().change("External", useColumns.getState().byTab.External.map(c => c.key === "Content Type" ? { ...c, visible: false } : c));
    await useColumns.getState().change("Internal", useColumns.getState().byTab.Internal.map(c => c.key === "Word Count" ? { ...c, visible: false } : c));
    render(<ColumnConfigurator tab="Internal" tabs={[]} isOpen onOpenChange={() => undefined} label={id => id} />);
    const available = await screen.findByRole("listbox", { name: "Available columns" });
    expect(within(available).getByRole("option", { name: /Hash/ })).toHaveTextContent("Hash – disabled in the configuration (crawl.storeHash)");
    fireEvent.click(screen.getByRole("button", { name: "Restore defaults" }));
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(useColumns.getState().byTab.Internal.find(c => c.key === "Word Count")!.visible).toBe(true));
    expect(useColumns.getState().byTab.Internal.map(c => c.key)).toEqual(normalizeColumns(internalColumns).map(c => c.key));
    expect(useColumns.getState().byTab.External.find(c => c.key === "Content Type")!.visible).toBe(false);
  });

  // spec: UI-009
  it("saves, loads, renames and deletes named templates and reports missing columns on another tab", async () => {
    await useColumns.getState().load("Internal");
    const basis = ["Address", "Status Code", "Word Count", "Title 1", "Segment", "Metric 01", "Metric 02", "Metric 03"];
    await useColumns.getState().change("Internal", applyTemplateColumns(useColumns.getState().byTab.Internal, basis).columns);
    render(<ColumnConfigurator tab="Internal" tabs={advancedTabs.map(x => ({ id: x.id, label: x.id }))} isOpen onOpenChange={() => undefined} label={id => id} />);
    await screen.findByRole("listbox", { name: "Available columns" });
    fireEvent.change(screen.getByLabelText("Template name"), { target: { value: "Basis" } });
    fireEvent.click(screen.getByRole("button", { name: "Save as template" }));
    await waitFor(() => expect(useColumns.getState().templates).toEqual([{ name: "Basis", columns: basis }]));
    select("Apply to tab", "External");
    fireEvent.click(screen.getByRole("button", { name: "Apply template to tab" }));
    expect(await screen.findByText("Applied to External; 6 columns are not available there and were skipped: Word Count, Title 1, Segment, Metric 01, Metric 02, Metric 03")).toHaveRole("status");
    expect(useColumns.getState().byTab.External.filter(c => c.visible).map(c => c.key)).toEqual(["Row", "Address", "Status Code"]);
    fireEvent.change(screen.getByLabelText("Template name"), { target: { value: "Basis 2" } });
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    await waitFor(() => expect(useColumns.getState().templates.map(x => x.name)).toEqual(["Basis 2"]));
    fireEvent.click(screen.getByRole("button", { name: "Load template" }));
    expect(screen.getByRole("status")).toHaveTextContent("Template Basis 2 loaded");
    await flushUiState();
    expect((await uistate.load()).state).toMatchObject({ columnTemplates: [{ name: "Basis 2", columns: basis }] });
    fireEvent.click(screen.getByRole("button", { name: "Delete template" }));
    await waitFor(() => expect(useColumns.getState().templates).toEqual([]));
  });

  // spec: UI-021
  it("resets every table to the shipped layout but keeps templates, tab order, filters and searches", async () => {
    await useColumns.getState().load("Internal"); await useColumns.getState().load("External");
    await useTabs.getState().reorder(["Images", "Internal"]);
    useTabs.setState({ queries: { Internal: { filter: "All", search: "blog", sort: [{ column: "Word Count", dir: "asc" }] } } });
    await useColumns.getState().change("Internal", useColumns.getState().byTab.Internal.map(c => c.key === "Title 1" ? { ...c, visible: false, width: 400 } : c.key === "Address" ? { ...c, pinned: false } : c));
    await useColumns.getState().saveTemplate("Basis", ["Address", "Status Code"]);
    await useColumns.getState().resetAll();
    expect(useColumns.getState().byTab.Internal).toEqual(normalizeColumns(internalColumns));
    expect(useTabs.getState().queries.Internal).toEqual({ filter: "All", search: "blog", sort: [] });
    await flushUiState();
    const state = (await uistate.load()).state as Record<string, unknown>;
    expect(state.tableColumns).toBeUndefined();
    expect(state).toMatchObject({ columnTemplates: [{ name: "Basis" }], topTabs: { order: expect.arrayContaining(["Images"]) } });
    expect((fx.config().config as { ui: { columns: unknown } }).ui.columns).toEqual({});
    useColumns.setState({ byTab: {} }); await useColumns.getState().load("Internal");
    expect(useColumns.getState().byTab.Internal).toEqual(normalizeColumns(internalColumns));
  });

  // spec: UI-021
  it("asks before resetting all tables from the header menu; Cancel changes nothing", async () => {
    await mounted();
    await waitFor(() => expect(capture.grid!.columns.length).toBeGreaterThan(0));
    const before = useColumns.getState().byTab.Internal;
    act(() => capture.grid!.onContextMenu({ column: "Address", x: 1, y: 1 }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Reset columns of all tables" }));
    const dialog = await screen.findByRole("dialog", { name: "Reset columns of all tables" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(useColumns.getState().byTab.Internal).toBe(before);
  });
});

describe("tab bar", () => {
  // spec: UI-112
  it("closes a tab, the others or all of them, but never the mandatory tab", () => {
    const hide = vi.fn();
    const layout = { order: ["Internal", "External", "Images", "AMP"], hidden: ["AMP"], mandatory: "Internal" };
    const entries = tabContextEntries(t, "Images", layout, { hide, configure: vi.fn(), reset: vi.fn() });
    expect(entries.map(e => e.id)).toEqual(["tab-close", "tab-close-others", "tab-close-all", "tab-configure", "tab-reset"]);
    entries[1].onAction!(); expect(hide).toHaveBeenLastCalledWith(["External"]);
    entries[2].onAction!(); expect(hide).toHaveBeenLastCalledWith(["External", "Images"]);
    const mandatory = tabContextEntries(t, "Internal", layout, { hide, configure: vi.fn(), reset: vi.fn() });
    expect(mandatory[0]).toMatchObject({ isDisabled: true, disabledReason: "u20-tab-mandatory" });
    const details = tabContextEntries(t, "Inlinks", { order: ["URL Details", "Inlinks"], hidden: [], mandatory: "URL Details" }, { hide, configure: vi.fn(), reset: vi.fn() });
    details[2].onAction!(); expect(hide).toHaveBeenLastCalledWith(["Inlinks"]);
  });

  // spec: UI-112
  // spec: UI-004 [P1]
  it("opens the context menu on a top tab, closes all but Internal, and shows a closed tab again from the dropdown", async () => {
    // K2-77: the pills show the tabs of the active group (URLs: Internal, External); close/configure work within the group.
    render(<TopTabs />);
    const external = await screen.findByRole("tab", { name: "External" });
    fireEvent.contextMenu(external);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Close all tabs" }));
    await waitFor(() => expect(screen.getAllByRole("tab").map(x => x.textContent)).toEqual(["Internal"]));
    expect(useTabs.getState().hidden).toEqual(["External"]);
    fireEvent.keyDown(screen.getByRole("tab", { name: "Internal" }), { key: "F10", shiftKey: true });
    expect(await screen.findByRole("menuitem", { name: "Close tab" })).toHaveAttribute("aria-disabled", "true");
    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: "Configure tabs" }));
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "External" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "External", hidden: true })).toBeInTheDocument());
    expect(useTabs.getState().hidden).not.toContain("External");
  });

  // spec: UI-004 [P1]
  it("configures order and visibility in the tabs dialog; Internal cannot be hidden", async () => {
    render(<TopTabs />);
    fireEvent.contextMenu(await screen.findByRole("tab", { name: "External" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Configure tabs…" }));
    const dialog = await screen.findByRole("dialog", { name: "Configure tabs" });
    expect(within(dialog).getAllByRole("checkbox").map(c => c.closest("label")?.textContent)).toEqual(["Internal", "External"]);
    expect(within(dialog).getByRole("checkbox", { name: "Internal" })).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("checkbox", { name: "External" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Move External up" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.getAllByRole("tab").map(x => x.textContent)).toEqual(["Internal"]));
    await flushUiState();
    // Only the group's slots change; the other groups keep their order.
    expect((await uistate.load()).state).toMatchObject({ topTabs: { order: ["External", "Internal", "Images", "AMP", "Page Titles"], hidden: ["External"] } });
  });
});

describe("segments", () => {
  // spec: UI-020
  it("colours segment labels with readable text (WCAG AA) and filters table and counts by segment", async () => {
    await mounted();
    await waitFor(() => expect(screen.getByLabelText("Segment")).toBeInTheDocument());
    const colors = capture.grid!.cellColors!;
    for (const segment of segmentDefs) {
      const label = colors("Segment", segment.name)!;
      expect(label.background).toBe(segment.color);
      expect(contrast(label.background, label.text)).toBeGreaterThanOrEqual(4.5);
      expect(label.text).toBe(segmentLabelColor(segment.color));
    }
    expect(colors("Address", "Blog")).toBeUndefined(); expect(colors("Segment", null)).toBeUndefined();
    select("Segment", "segment:Blog");
    await waitFor(() => expect(capture.grid!.source.total).toBe(1));
    expect(lastSpec().segment).toEqual({ kind: "segment", name: "Blog" });
    await waitFor(() => expect(fx.core.calls.some(c => c.cmd === "catalog.filterCounts" && JSON.stringify(c.args).includes("Blog"))).toBe(true));
    expect(screen.getByRole("status", { name: "Segment filter: Blog" })).toBeInTheDocument();
    select("Segment", "none");
    await waitFor(() => expect(capture.grid!.source.total).toBe(2));
    expect(screen.getByRole("status", { name: "Segment filter: No segment" })).toBeInTheDocument();
    select("Segment", "");
    await waitFor(() => expect(capture.grid!.source.total).toBe(5));
    expect(screen.queryByRole("status", { name: /Segment filter/ })).toBeNull();
  });

  // spec: UI-020
  it("shows reassignment progress and reloads segments, view and counts when the core reports completion", async () => {
    await mounted();
    await waitFor(() => expect(useSegments.getState().segments).toHaveLength(2));
    act(() => useSession.setState({ ops: { r: { opId: "r", labelKey: "op-segment-reassign", done: 2, total: 5, state: "running" } } }));
    expect(await screen.findByRole("progressbar", { name: "Reassigning segments" })).toBeInTheDocument();
    const opens = tabSpecs().length;
    fx.setSegments([{ ...segmentDefs[0], color: "#000000" }]);
    act(() => useSession.setState({ ops: { r: { opId: "r", labelKey: "op-segment-reassign", done: 5, total: 5, state: "done" } } }));
    await waitFor(() => expect(useSegments.getState().segments).toHaveLength(1));
    await waitFor(() => expect(tabSpecs().length).toBeGreaterThan(opens));
    expect(screen.queryByRole("progressbar", { name: "Reassigning segments" })).toBeNull();
    expect(capture.grid!.cellColors!("Segment", "Blog")).toEqual({ background: "#000000", text: "#FFFFFF" });
  });
});

describe("row context menu", () => {
  // spec: UI-110 [P1]
  it("enables screenshot, visualisations, crawl path, check-with and segment entries only for one URL", () => {
    const actions = { run: vi.fn(), viz: vi.fn(), filterSegment: vi.fn() };
    const has = (id: string) => id !== "url.screenshot";
    const one = advancedContextEntries(t, { count: 1, hasSegments: true, hasAction: has }, actions);
    expect(one.map(e => e.id)).toEqual(["url.screenshot", "viz-crawlTree", "viz-crawlGraph", "viz-directoryTree", "viz-directoryGraph", "viz-crawlGraph3d",
      "viz-directoryGraph3d", "viz-anchorWords", "viz-bodyWords", "url.crawlPath", "url.checkWith", "segment-filter", "segment.jump"]);
    expect(one.filter(e => e.isDisabled).map(e => [e.id, e.disabledReason])).toEqual([["url.screenshot", "u20-ctx-unavailable"]]);
    one.find(e => e.id === "viz-directoryTree")!.onAction!(); expect(actions.viz).toHaveBeenCalledWith("directoryTree");
    one.find(e => e.id === "url.crawlPath")!.onAction!(); expect(actions.run).toHaveBeenCalledWith("url.crawlPath");
    const three = advancedContextEntries(t, { count: 3, hasSegments: true, hasAction: () => true }, actions);
    expect(three.every(e => e.isDisabled && e.disabledReason === "u20-ctx-single")).toBe(true);
    const noSegments = advancedContextEntries(t, { count: 1, hasSegments: false, hasAction: () => true }, actions);
    expect(noSegments.at(-1)).toMatchObject({ id: "segment-filter", isDisabled: true, disabledReason: "u20-ctx-no-segments" });
  });

  // spec: UI-110 [P1]
  it("opens a visualisation rooted at the marked URL and sets the segment filter from its row", async () => {
    await mounted();
    await waitFor(() => expect(useSegments.getState().segments).toHaveLength(2));
    act(() => capture.grid!.onContextMenu({ column: "Address", urlId: 4, row: 3, x: 1, y: 1 }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Crawl tree" }));
    await waitFor(() => expect(fx.core.calls.find(c => c.cmd === "viz.open")?.args).toEqual({ kind: "crawlTree", rootUrlId: 4 }));
    act(() => capture.grid!.onContextMenu({ column: "Address", urlId: 4, row: 3, x: 1, y: 1 }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Filter by this URL's segment" }));
    await waitFor(() => expect(useSegments.getState().filter).toEqual({ kind: "segment", name: "Shop" }));
    act(() => capture.grid!.onSelectionChange(SelectionModel.empty().select(1).select(2, true)));
    act(() => capture.grid!.onContextMenu({ column: "Address", urlId: 2, row: 0, x: 1, y: 1 }));
    expect(await screen.findByRole("menuitem", { name: "Crawl tree" })).toHaveAttribute("aria-disabled", "true");
  });
});

describe("closed dialogs (M2-N32)", () => {
  // spec: UI-145
  // spec: UI-143
  it("renders neither the column nor the tab configuration while closed and opens them unchanged", async () => {
    await useColumns.getState().load("Internal");
    const layout = { order: ["a", "b"], hidden: ["b"], mandatory: "a" };
    const closed = render(<><ColumnConfigurator tab="Internal" tabs={[]} isOpen={false} onOpenChange={() => undefined} label={id => id} />
      <TabsConfigDialog isOpen={false} onOpenChange={() => undefined} layout={layout} label={id => id} onApply={() => undefined} /></>);
    expect(closed.container.innerHTML).toBe("");
    expect(screen.queryByRole("dialog")).toBeNull();
    closed.rerender(<><ColumnConfigurator tab="Internal" tabs={[]} isOpen onOpenChange={() => undefined} label={id => id} />
      <TabsConfigDialog isOpen={false} onOpenChange={() => undefined} layout={layout} label={id => id} onApply={() => undefined} /></>);
    expect(await screen.findByRole("listbox", { name: "Available columns" })).toBeInTheDocument();
    closed.unmount();
    const onApply = vi.fn();
    render(<TabsConfigDialog isOpen onOpenChange={() => undefined} layout={layout} label={id => id} onApply={onApply} />);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getAllByRole("checkbox").map(c => (c as HTMLInputElement).checked)).toEqual([true, false]);
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    expect(onApply).toHaveBeenCalledWith(["a", "b"], ["b"]);
  });
});

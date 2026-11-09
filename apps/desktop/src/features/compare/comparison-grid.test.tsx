import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createClient, setClient } from "../../ipc/client";
import { setLanguage } from "../../i18n";
import { ComparisonTable } from "./ComparisonTable";
import { ComparisonLinks } from "./ComparisonLinks";
import { ComparisonDetail } from "./ComparisonDetail";
import { ComparisonSource } from "./ComparisonGrid";
import { LOADING } from "../../data/ViewDataSource";
import { comparisonFixture, session, snapshot } from "./fixtures/out-compare/core";
import { type ComparisonPage, useComparison } from "./model";
let fixture: ReturnType<typeof comparisonFixture>;
beforeEach(async () => {
  fixture = comparisonFixture(); setClient(createClient(fixture.core.transport())); await setLanguage("en");
  useComparison.setState({ session, snapshot, revision: 0, query: { tab: "Internal", filter: "All", side: "Current" }, selected: null, selectedPartner: null });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(1800);
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1800);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(360);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, width: 1800, height: 360, top: 0, left: 0, right: 1800, bottom: 360, toJSON() {} });
});
afterEach(() => { cleanup(); setClient(null); vi.restoreAllMocks(); });
const lastRows = () => fixture.core.calls.filter(c => c.cmd === "view.rows").at(-1)?.args;
// spec: CRAWL-144
it("configures link columns with the main table menu and retains them across sorting", async () => {
  render(<ComparisonLinks direction="outlinks" urlId={1} />);
  const grid = await screen.findByRole("grid", { name: "Link comparison table" });
  await screen.findByRole("columnheader", { name: "Previous Anchor" });
  fireEvent.click(screen.getByRole("button", { name: "Configure columns" }));
  const dialog = screen.getByRole("dialog");
  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Previous Anchor" }));
  fireEvent.click(within(dialog).getByRole("checkbox", { name: "Select Anchor for moving" }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Down" }));
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  expect(within(grid).queryByRole("columnheader", { name: "Previous Anchor" })).toBeNull();
  const names = within(grid).getAllByRole("columnheader").map(c => c.textContent);
  expect(names.indexOf("Anchor")).toBeGreaterThan(names.indexOf("Link Change"));
  fireEvent.keyDown(grid, { key: "ArrowUp" }); fireEvent.keyDown(grid, { key: "Enter" });
  await waitFor(() => expect(lastRows()).toMatchObject({ comparison: { sort: { column: "Source", dir: "asc" } } }));
  await screen.findByRole("grid");
  expect(screen.queryByRole("columnheader", { name: "Previous Anchor" })).toBeNull();
});
// spec: OUT-140
it("sorts ordinary comparison URLs through the core in both directions", async () => {
  render(<ComparisonTable />);
  let grid = await screen.findByRole("grid");
  fireEvent.keyDown(grid, { key: "ArrowUp" }); fireEvent.keyDown(grid, { key: "Enter" });
  await waitFor(() => expect(lastRows()).toMatchObject({ offset: 0, comparison: { sort: { column: "Address", dir: "asc" } } }));
  grid = await screen.findByRole("grid");
  expect(within(grid).getByRole("columnheader", { name: "Address" })).toHaveAttribute("aria-sort", "ascending");
  fireEvent.keyDown(grid, { key: "ArrowUp" }); fireEvent.keyDown(grid, { key: "Enter" });
  await waitFor(() => expect(lastRows()).toMatchObject({ comparison: { sort: { column: "Address", dir: "desc" } } }));
});
// spec: TECH-301
it("selects a URL through DataGrid and opens its existing paired detail", async () => {
  render(<><ComparisonTable /><ComparisonDetail /></>);
  const grid = await screen.findByRole("grid", { name: "Comparison table" });
  fireEvent.keyDown(grid, { key: "Home" });
  await waitFor(() => expect(useComparison.getState().selected).toBe(1));
  expect(useComparison.getState().selectedPartner).toBe(101);
  await screen.findByRole("grid", { name: "Link comparison table" });
  expect(lastRows()).toMatchObject({ comparison: { tab: "inlinks", urlId: 1, side: "Current" } });
});
// spec: TECH-301
// spec: UI-140
it("keeps stable link keys separate from URL ids, preserves exact numeric cell contents and loads further windows on demand", async () => {
  const rows = Array.from({ length: 202 }, (_, i) => i === 0 ? ["https://site.test/a", 1.23456] : i === 1 ? ["https://site.test/b", null] : [`https://site.test/${i}`, i]);
  const keys = rows.map((_, i) => i === 0 ? -5 : i === 1 ? 204 : 1000 + i);
  const page = (offset: number, limit: number): ComparisonPage => ({ viewId: "comparison", version: 1, offset, total: rows.length, columns: ["Address", "Change %"],
    cells: rows.slice(offset, offset + limit), rowKeys: keys.slice(offset, offset + limit), partnerKeys: keys.slice(offset, offset + limit).map(k => k === 204 ? 304 : null), side: "Previous", filtersEnabled: true });
  const load = vi.fn(async (offset: number, limit: number) => page(offset, limit));
  const source = new ComparisonSource(page(0, 200), load);
  expect(source.total).toBe(202);
  expect(source.rowKey(0)).toBe(-5); expect(await source.locate(204)).toBe(1); expect(source.partner(204)).toBe(304);
  expect(await source.keys(0, 1)).toEqual([-5, 204]);
  expect(source.getCell(1, 0)).toBe(1.23456); expect(source.getCell(1, 1)).toBeNull();
  expect(await source.locate(1000)).toBeNull();
  // Rows beyond the first window are fetched from the core when they become visible, never paged by buttons.
  expect(source.getCell(0, 201)).toBe(LOADING);
  await source.ensureRange(195, 201);
  expect(load).toHaveBeenCalledWith(200, 200, expect.anything());
  expect(source.getCell(0, 201)).toBe("https://site.test/201");
  expect(await source.locate(1201)).toBe(201);
  await act(() => source.dispose());
});

// spec: OUT-140
it("does not offer core sorting for columns appended after the ordinary URL query", () => {
  const source = new ComparisonSource({ viewId: "comparison", version: 1, offset: 0, total: 0, columns: ["Address", "Comparison Status", "Previous Address"],
    cells: [], rowKeys: [], side: "Current", filtersEnabled: true }, async () => { throw new Error("no rows"); });
  expect(source.columns.map(c => c.sortable)).toEqual([true, false, false]);
});

// spec: OUT-142
// spec: OUT-151
// spec: UI-007
it("shows the tab's ordinary columns with catalog kinds and localized headers, the comparison status directly after Address", async () => {
  fixture.core.handlers.set("catalog.columns", args => (args as { tab: string }).tab === "Internal" ? [
    { key: "Address", kind: { kind: "url" }, defaultVisible: true, defaultWidth: 320, align: "left", sortable: true, searchable: [], extension: false },
    { key: "Status Code", kind: { kind: "integer" }, defaultVisible: true, defaultWidth: 90, align: "right", sortable: true, searchable: [], extension: false },
    { key: "Title 1 Length", kind: { kind: "integer" }, defaultVisible: false, defaultWidth: 110, align: "right", sortable: true, searchable: [], extension: false },
    // The catalog may name a column like the comparison status; the comparison's own column stays text and unsortable.
    { key: "Comparison Status", kind: { kind: "integer" }, defaultVisible: false, defaultWidth: 10, align: "right", sortable: true, searchable: [], extension: false },
  ] : []);
  fixture.core.handlers.set("view.rows", args => ({ viewId: "fixture-comparison", version: (args as { version: number }).version, offset: 0,
    columns: ["Address", "Status Code", "Title 1 Length", "Comparison Status"], cells: [["https://site.test/start", 200, 56, "Unchanged"]], rowKeys: [1], partnerKeys: [101], total: 1, side: "Current", filtersEnabled: true }));
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(360);
  await setLanguage("de");
  render(<ComparisonTable />);
  const grid = await screen.findByRole("grid", { name: "Vergleichstabelle" });
  await waitFor(() => expect(within(grid).getAllByRole("columnheader").map(c => c.textContent)).toEqual(["Adresse", "Comparison Status", "Statuscode", "Titel 1 Länge"]));
  expect(fixture.core.calls.filter(c => c.cmd === "catalog.columns").at(-1)?.args).toEqual({ tab: "Internal" });
  // UI-007: numbers keep their type (formatted and right-aligned by the grid), not stringified text.
  await waitFor(() => expect(within(grid).getAllByRole("gridcell").map(c => c.textContent)).toEqual(["https://site.test/start", "Unchanged", "200", "56"]));
});

// spec: CRAWL-144
it("hides and restores URL columns with the shared menu without changing their population", async () => {
  render(<ComparisonTable />);
  await screen.findByRole("grid", { name: "Comparison table" });
  fireEvent.click(screen.getByRole("button", { name: "Configure columns" }));
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("checkbox", { name: "Comparison Status" }));
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  expect(screen.queryByRole("columnheader", { name: "Comparison Status" })).toBeNull();
  expect(screen.getByRole("grid")).toHaveAttribute("aria-rowcount", "2");
  fireEvent.click(screen.getByRole("button", { name: "Configure columns" }));
  fireEvent.click(screen.getByRole("button", { name: "Reset columns" }));
  expect(screen.getByRole("columnheader", { name: "Comparison Status" })).toBeInTheDocument();
});

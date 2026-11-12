import { StrictMode, useState } from "react";
import { act, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { DataGrid, defaultColumns, type DataGridProps, type GridController } from "./DataGrid";
import { ViewDataSource } from "../data/ViewDataSource";
import { SelectionModel } from "../state/selection";
import { resolveTokens } from "../ui/tokens/tokens";
import type { ViewHandle } from "../ipc/gen/ViewHandle";
import { IpcError, localError } from "../ipc/errors";
import fixture from "./fixtures/fx-ui-basic.json";

afterEach(() => vi.restoreAllMocks());
function setup() {
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(1_000_000);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, width: 800, height: 400, top: 0, left: 0, bottom: 400, right: 800, toJSON() {} });
  const copy = vi.fn(async () => ({ rows: 1, bytes: 100 }));
  const source = new ViewDataSource(fixture as ViewHandle, {
    rows: vi.fn(async a => ({ ...a, rowKeys: Array.from({ length: a.limit }, (_, i) => a.offset + i + 1),
      cells: Array.from({ length: a.limit }, (_, i) => [a.offset + i + 1, ...fixture.row.slice(1)]) })),
    locate: vi.fn(async a => a.urlId - 1), copy, close: vi.fn(),
  });
  const props: DataGridProps = { source, columns: defaultColumns(source.columns), selection: SelectionModel.empty().select(1),
    onSelectionChange: vi.fn(), onColumnsChange: vi.fn(), onSortChange: vi.fn(), onContextMenu: vi.fn(), onActivate: vi.fn(), onError: vi.fn(),
    confirmLargeCopy: vi.fn(), onExportSelection: vi.fn(), rowHeight: 26,
    theme: { tokens: resolveTokens("light", "#2f855a"), fontFamily: "system-ui", fontSize: 12, zebra: true }, ariaLabel: "Fixture", locale: "de" };
  return { props, source, copy };
}
// spec: UI-007
// spec: UI-012
// spec: UI-140
it("exposes actual fixture cells under StrictMode and delegates selection and copy", async () => {
  const { props, copy, source } = setup();
  const mounted = render(<StrictMode><DataGrid {...props} /></StrictMode>);
  const grid = mounted.getByRole("grid");
  expect(grid).toHaveAttribute("aria-rowcount", "5000001");
  expect(grid).toHaveAttribute("aria-readonly", "true");
  await waitFor(() => expect(mounted.getAllByRole("gridcell").some(c => c.textContent === "12.345")).toBe(true));
  expect(mounted.getAllByRole("row").length).toBeLessThan(300);
  fireEvent.keyDown(grid, { key: "c", ctrlKey: true });
  await waitFor(() => expect(copy).toHaveBeenCalledWith({ viewId: source.viewId, selection: props.selection.value, columns: source.columns.map(c => c.key), withHeader: true }));
  fireEvent.keyDown(grid, { key: "a", metaKey: true });
  expect(vi.mocked(props.onSelectionChange).mock.lastCall![0].value).toEqual({ kind: "all", viewId: source.viewId, version: 1, except: [] });
  fireEvent.keyDown(grid, { key: "End", shiftKey: true });
  await waitFor(() => expect(vi.mocked(props.onSelectionChange).mock.lastCall![0].focus).toBe(5_000_000));
  expect(vi.mocked(props.onSelectionChange).mock.lastCall![0].value.kind).toBe("all");
  expect(vi.mocked(source.api.rows).mock.calls.every(([args]) => args.columns.length > 0)).toBe(true);
  await waitFor(() => expect(mounted.getAllByRole("row").at(-1)).toHaveAttribute("aria-rowindex", "5000001"));
  expect(document.getElementById(grid.getAttribute("aria-activedescendant")!)).toHaveTextContent("5.000.000");
  expect(props.onError).not.toHaveBeenCalled();
  mounted.unmount(); await source.dispose();
});
// spec: UI-140
it("keeps exact middle jumps despite compressed scrollbar rounding and reconnects after unmount", async () => {
  const { props, source } = setup(); let controller: GridController | null = null;
  props.controller = c => { controller = c; };
  const mounted = render(<StrictMode><DataGrid {...props} /></StrictMode>);
  await waitFor(() => expect(controller).not.toBeNull());
  for (const row of [400, 2_500_000, 4_999_999]) {
    act(() => controller!.scrollTo(row));
    await waitFor(() => expect(source.rowKey(row)).toBe(row + 1));
    const region = controller!.visible();
    expect(region.y).toBeLessThanOrEqual(row);
    expect(region.y + region.height).toBeGreaterThan(row);
    fireEvent.scroll(mounted.container.querySelector(".os-grid-scroll")!);
    expect(controller!.visible().y).toBe(region.y);
  }
  const scroll = mounted.container.querySelector(".os-grid-scroll")!;
  scroll.scrollLeft = 500; fireEvent.scroll(scroll);
  expect(controller!.bounds(2, controller!.visible().y)).toMatchObject({ x: -90, width: 120 });
  expect(controller!.bounds(999, 0)).toBeUndefined();
  mounted.unmount(); expect(controller).toBeNull(); expect(source.onScrollTo).toBeUndefined();
  await source.dispose();
});
// spec: UI-012
it("handles keyboard header sorting, horizontal navigation, context menus and activation", async () => {
  const { props, source } = setup(); const mounted = render(<DataGrid {...props} />);
  const grid = mounted.getByRole("grid");
  await waitFor(() => expect(source.rowKey(0)).toBe(1));
  fireEvent.keyDown(grid, { key: "ArrowRight" });
  fireEvent.keyDown(grid, { key: "Enter" });
  expect(props.onActivate).toHaveBeenCalledWith(1, "Address");
  fireEvent.keyDown(grid, { key: "F10", shiftKey: true });
  expect(props.onContextMenu).toHaveBeenCalledWith(expect.objectContaining({ column: "Address", row: 0, urlId: 1 }));
  fireEvent.keyDown(grid, { key: "ArrowUp" }); fireEvent.keyDown(grid, { key: "Enter" });
  expect(props.onSortChange).toHaveBeenCalledWith([{ column: "Address", dir: "asc" }]);
  mounted.unmount(); await source.dispose();
});
// spec: UI-140
// spec: UI-012
it("handles an empty fixture and cancels a pending row selection on unmount", async () => {
  const { props, source } = setup(); source.total = 0;
  const mounted = render(<DataGrid {...props} selection={SelectionModel.empty()} />);
  expect(mounted.getByRole("grid")).toHaveAttribute("aria-rowcount", "1");
  expect(mounted.queryAllByRole("gridcell")).toHaveLength(0);
  fireEvent.keyDown(mounted.getByRole("grid"), { key: "End" });
  expect(source.api.rows).not.toHaveBeenCalled();
  mounted.unmount(); await source.dispose();
  const pending = setup();
  let finish!: (window: Awaited<ReturnType<typeof source.api.rows>>) => void;
  vi.mocked(pending.source.api.rows).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  const second = render(<DataGrid {...pending.props} />);
  fireEvent.keyDown(second.getByRole("grid"), { key: "ArrowDown" });
  second.unmount();
  await act(async () => finish({ viewId: pending.source.viewId, version: 1, offset: 0, columns: pending.source.columns.map(c => c.key),
    rowKeys: [1, 2], cells: [fixture.row, [2, ...fixture.row.slice(1)]] }));
  expect(pending.props.onSelectionChange).not.toHaveBeenCalled();
  await pending.source.dispose();
});
// spec: UI-145
// spec: UI-143
it("repaints the canvas and the accessibility rows only when their content changes (M2-N32)", async () => {
  const { props, source } = setup();
  let fills = 0;
  const state = new Map<PropertyKey, unknown>();
  const context = new Proxy({}, {
    get: (_, key) => state.has(key) ? state.get(key) : key === "fillText" ? () => { fills++; } : () => undefined,
    set: (_, key, value) => { state.set(key, value); return true; },
  });
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context as unknown as CanvasRenderingContext2D);
  const mounted = render(<DataGrid {...props} />);
  await waitFor(() => expect(mounted.getAllByRole("gridcell").some(c => c.textContent === "12.345")).toBe(true));
  await act(async () => { await new Promise(r => requestAnimationFrame(r)); });
  const painted = fills;
  expect(painted).toBeGreaterThan(0);
  const firstRow = mounted.getAllByRole("row")[1]!;
  const cell = firstRow.querySelector("span[hidden]")!;
  // A parent re-render with equal content (new theme object, new callbacks) neither repaints nor rebuilds rows.
  mounted.rerender(<DataGrid {...props} theme={{ ...props.theme, tokens: { ...props.theme.tokens } }} onError={vi.fn()} />);
  expect(fills).toBe(painted);
  expect(mounted.getAllByRole("row")[1]!.querySelector("span[hidden]")).toBe(cell);
  // Changed content repaints: another selection and another theme colour.
  mounted.rerender(<DataGrid {...props} selection={SelectionModel.empty().select(2)} />);
  expect(fills).toBeGreaterThan(painted);
  const reselected = fills;
  mounted.rerender(<DataGrid {...props} selection={SelectionModel.empty().select(2)} theme={{ ...props.theme, tokens: resolveTokens("dark", "#2f855a") }} />);
  expect(fills).toBeGreaterThan(reselected);
  mounted.unmount(); await source.dispose();
});
// spec: UI-012
// spec: UI-018
it("keeps a Shift range when the view version changes while the range is resolved, also from Shift+arrow keys (F3)", async () => {
  const { props, source } = setup();
  let answer: (value: Awaited<ReturnType<NonNullable<typeof source.api.range>>>) => void = () => undefined;
  const range = vi.fn(() => new Promise<Awaited<ReturnType<NonNullable<typeof source.api.range>>>>(resolve => { answer = resolve; }));
  (source.api as { range?: unknown }).range = range;
  const mounted = render(<DataGrid {...props} />);
  await waitFor(() => expect(mounted.getAllByRole("gridcell").some(c => c.textContent === "12.345")).toBe(true));
  fireEvent.keyDown(mounted.getByRole("grid"), { key: "ArrowDown", shiftKey: true });
  await waitFor(() => expect(range).toHaveBeenCalledWith({ viewId: source.viewId, from: 1, to: 2 }));
  await act(async () => { await source.onViewChanged(source.version + 1, source.total + 10); });
  await act(async () => { answer({ viewId: source.viewId, version: source.version, fromRow: 0, toRow: 1, total: source.total, keys: [1, 2], all: false, exportRequired: false }); });
  await waitFor(() => expect(vi.mocked(props.onSelectionChange).mock.lastCall?.[0].value).toEqual({ kind: "ids", ids: [1, 2] }));
  expect(vi.mocked(props.onSelectionChange).mock.lastCall![0].focus).toBe(2);
  expect(props.onError).not.toHaveBeenCalled();
  mounted.unmount(); await source.dispose();
});
// spec: UI-012
// spec: UI-018
// spec: UI-142
it("a Shift range on its way finishes against the silent replacement of its view (RC1-SEL)", async () => {
  const { props, source } = setup();
  let fail: (error: unknown) => void = () => undefined;
  (source.api as { range?: unknown }).range = vi.fn(() => new Promise((_, reject) => { fail = reject; }));
  const successor = new ViewDataSource({ ...(fixture as ViewHandle), viewId: "replacement" }, {
    ...source.api,
    range: vi.fn(async (args: { from: number; to: number }) => ({ viewId: "replacement", version: 1, fromRow: args.from - 1, toRow: args.to - 1, total: source.total, keys: [args.from, args.to], all: false, exportRequired: false })),
  }, source);
  const mounted = render(<DataGrid {...props} />);
  await waitFor(() => expect(mounted.getAllByRole("gridcell").some(c => c.textContent === "12.345")).toBe(true));
  fireEvent.keyDown(mounted.getByRole("grid"), { key: "ArrowDown", shiftKey: true });
  await waitFor(() => expect(source.api.range).toHaveBeenCalledWith({ viewId: source.viewId, from: 1, to: 2 }));
  // The table installs the replacement and closes the released view: its pending range answers NotFound.
  mounted.rerender(<DataGrid {...props} source={successor} />);
  await act(async () => { fail(new IpcError(localError("NotFound", "err-view-not-found"))); });
  await waitFor(() => expect(vi.mocked(props.onSelectionChange).mock.lastCall?.[0].value).toEqual({ kind: "ids", ids: [1, 2] }));
  expect(vi.mocked(props.onSelectionChange).mock.lastCall![0].focus).toBe(2);
  expect(successor.api.range).toHaveBeenCalledWith({ viewId: "replacement", from: 1, to: 2 });
  expect(props.onError).not.toHaveBeenCalled();
  mounted.unmount(); await successor.dispose(); await source.dispose();
});
// spec: UI-012
// spec: UI-142
it("a different view ends a Shift range on its way", async () => {
  const { props, source } = setup();
  let answer: (value: unknown) => void = () => undefined;
  (source.api as { range?: unknown }).range = vi.fn(() => new Promise(resolve => { answer = resolve; }));
  const other = new ViewDataSource({ ...(fixture as ViewHandle), viewId: "other" }, source.api);
  const mounted = render(<DataGrid {...props} />);
  await waitFor(() => expect(mounted.getAllByRole("gridcell").some(c => c.textContent === "12.345")).toBe(true));
  fireEvent.keyDown(mounted.getByRole("grid"), { key: "ArrowDown", shiftKey: true });
  await waitFor(() => expect(source.api.range).toHaveBeenCalled());
  mounted.rerender(<DataGrid {...props} source={other} />);
  await act(async () => { answer({ viewId: source.viewId, version: 1, fromRow: 0, toRow: 1, total: source.total, keys: [1, 2], all: false, exportRequired: false }); });
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
  expect(props.onSelectionChange).not.toHaveBeenCalledWith(expect.objectContaining({ value: { kind: "ids", ids: [1, 2] } }));
  mounted.unmount(); await other.dispose(); await source.dispose();
});
// spec: UI-012
it("Shift+arrow keys in quick succession continue from the keyboard row while earlier ranges are answered (F3)", async () => {
  const { props, source } = setup();
  const answers: (() => void)[] = [];
  (source.api as { range?: unknown }).range = vi.fn((args: { from: number; to: number }) => new Promise(resolve => { answers.push(() => resolve({ viewId: source.viewId, version: source.version, fromRow: args.from - 1, toRow: args.to - 1, total: source.total,
    keys: Array.from({ length: args.to - args.from + 1 }, (_, i) => args.from + i), all: false })); }));
  const Owner = () => { const [selection, setSelection] = useState(props.selection); return <DataGrid {...props} selection={selection} onSelectionChange={setSelection} />; };
  const mounted = render(<Owner />);
  await waitFor(() => expect(mounted.getAllByRole("gridcell").some(c => c.textContent === "12.345")).toBe(true));
  const grid = mounted.getByRole("grid");
  for (let press = 0; press < 3; press++) {
    fireEvent.keyDown(grid, { key: "ArrowDown", shiftKey: true });
    await waitFor(() => expect(answers.length).toBe(press + 1));
    // An older answer arrives while the next key is under way: it must not move the keyboard row back.
    if (press === 1) { await act(async () => { answers[0](); }); await act(async () => { await source.onViewChanged(source.version + 1, source.total); }); }
  }
  await act(async () => { answers.forEach(answer => answer()); });
  await waitFor(() => expect(mounted.getAllByRole("row").filter(r => r.getAttribute("aria-selected") === "true")).toHaveLength(4));
  mounted.unmount(); await source.dispose();
});

// spec: UI-012
it("shows the export hint for an oversized partial range without selecting the whole view", async () => {
  const { props, source } = setup();
  (source.api as { range?: unknown }).range = vi.fn(async () => ({ viewId: source.viewId, version: 1, fromRow: 0, toRow: 20000, total: source.total, keys: [], all: false, exportRequired: true }));
  const mounted = render(<DataGrid {...props} />);
  await waitFor(() => expect(source.rowKey(1)).toBe(2));
  fireEvent.keyDown(mounted.getByRole("grid"), { key: "ArrowDown", shiftKey: true });
  await waitFor(() => expect(props.onError).toHaveBeenCalledWith(expect.objectContaining({ messageKey: "err-range-export" })));
  expect(props.onSelectionChange).not.toHaveBeenCalled();
  mounted.unmount(); await source.dispose();
});

// spec: UI-012
// spec: UI-018
it("binds an all-range to the settled view even when a newer version overtakes its reply", async () => {
  const { props, source } = setup();
  let answer!: (value: Awaited<ReturnType<NonNullable<typeof source.api.range>>>) => void;
  (source.api as { range?: unknown }).range = vi.fn(() => new Promise(resolve => { answer = resolve; }));
  const mounted = render(<DataGrid {...props} />);
  await waitFor(() => expect(source.rowKey(1)).toBe(2));
  fireEvent.keyDown(mounted.getByRole("grid"), { key: "ArrowDown", shiftKey: true });
  await waitFor(() => expect(answer).toBeDefined());
  await act(async () => { await source.onViewChanged(3, source.total + 10); });
  await act(async () => { answer({ viewId: source.viewId, version: 2, fromRow: 0, toRow: source.total - 1, total: source.total - 5, keys: [], all: true, exportRequired: false }); });
  await waitFor(() => expect(vi.mocked(props.onSelectionChange).mock.lastCall?.[0].value).toEqual({ kind: "all", viewId: source.viewId, version: 3, except: [] }));
  expect(props.onError).not.toHaveBeenCalled();
  mounted.unmount(); await source.dispose();
});

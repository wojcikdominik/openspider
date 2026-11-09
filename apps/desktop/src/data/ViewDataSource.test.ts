import { describe, expect, it, vi } from "vitest";
import { LOADING, ViewDataSource, type ViewApi } from "./ViewDataSource";
import { IpcError, localError } from "../ipc/errors";
import type { ViewHandle } from "../ipc/gen/ViewHandle";
import type { RowWindow } from "../ipc/gen/RowWindow";
import fixture from "../grid/fixtures/fx-ui-basic.json";
function setup() {
  const api: ViewApi = {
    rows: vi.fn(async (a: Parameters<ViewApi["rows"]>[0]) => ({ ...a, rowKeys: Array.from({ length: Math.min(a.limit, fixture.total - a.offset) }, (_, i) => a.offset + i + 1),
      cells: Array.from({ length: Math.min(a.limit, fixture.total - a.offset) }, (_, i) => a.columns.map((_, col) => col === 0 ? a.offset + i + 1 : fixture.row[col])) })),
    locate: vi.fn(async a => a.urlId - 1), copy: vi.fn(), close: vi.fn(),
  };
  return { source: new ViewDataSource(fixture as ViewHandle, api), api };
}
describe("gen-ui-large", () => {
  // spec: UI-145
  // spec: UI-140
  it("uses the open response's bounded initial window without a second IPC request", async () => {
    const { api } = setup();
    const initial: RowWindow = { viewId: fixture.viewId, version: fixture.version, offset: 0,
      columns: fixture.columns.map(c => c.key),
      rowKeys: Array.from({ length: 200 }, (_, i) => i + 1),
      cells: Array.from({ length: 200 }, (_, i) => fixture.columns.map((_, col) => col === 0 ? i + 1 : "ready in open")) };
    const source = new ViewDataSource({ ...fixture, initialRows: initial } as ViewHandle, api);
    expect(source.getCell(1, 0)).toBe("ready in open");
    await source.ensureRange(0, 29);
    expect(api.rows).not.toHaveBeenCalled();
    await source.ensureRow(200);
    expect(api.rows).toHaveBeenCalledTimes(1);
    await source.onViewChanged(2, fixture.total);
    expect(source.getCell(1, 0)).not.toBe("ready in open");
  });
  // spec: UI-145
  it("rejects an initial window belonging to a different generation", () => {
    const { api } = setup();
    const initial: RowWindow = { viewId: fixture.viewId, version: fixture.version + 1, offset: 0,
      columns: fixture.columns.map(c => c.key), rowKeys: [1], cells: [fixture.row] };
    expect(() => new ViewDataSource({ ...fixture, initialRows: initial } as ViewHandle, api)).toThrow("Invalid initial row window");
  });
  // spec: UI-140
  it("deduplicates aligned 200-row blocks and fetches two screens of buffer", async () => {
    const { source, api } = setup();
    expect(source.getCell(0, 199)).toBe(LOADING);
    await Promise.all([source.ensureRange(190, 210), source.ensureRange(190, 210)]);
    expect(vi.mocked(api.rows).mock.calls.map(([a]) => [a.offset, a.limit])).toEqual([[0, 200], [200, 200]]);
    expect(source.getCell(0, 199)).toBe(200);
    await source.ensureRange(1000, 1099);
    expect(vi.mocked(api.rows).mock.calls.map(([a]) => a.offset)).toEqual([0, 200, 1000, 800, 1200]);
  });
  // spec: UI-140
  it("bounds cache at 20,000 rows and preserves recently read blocks", async () => {
    const { source } = setup();
    await source.ensureRange(0, 0);
    for (let i = 1; i <= 110; i++) { source.getCell(0, 0); await source.ensureRange(i * 600, i * 600); }
    expect(source.cachedRows).toBeLessThanOrEqual(20000);
    expect(source.getCell(0, 0)).toBe(1);
    expect(source.getCell(0, 600)).toBe(LOADING);
    await source.ensureRange(4_999_999, 4_999_999);
    expect(source.getCell(0, 4_999_999)).toBe(5_000_000);
  });
  // spec: UI-140
  it("discards late responses from an older version even if cancellation is ignored", async () => {
    const { source, api } = setup();
    let resolve!: (r: RowWindow) => void;
    vi.mocked(api.rows).mockImplementationOnce(() => new Promise(r => { resolve = r; }));
    const old = source.ensureRange(0, 0);
    await source.onViewChanged(2, fixture.total);
    resolve({ viewId: fixture.viewId, version: 1, offset: 0, columns: fixture.columns.map(c => c.key), rowKeys: [999], cells: [[999]] });
    await old;
    expect(source.rowKey(0)).toBe(1);
    expect(source.version).toBe(2);
  });
  // spec: UI-140
  it("relocates the focus anchor at its screen offset and follows tail only until manual upward scroll", async () => {
    const { source, api } = setup();
    const scroll = vi.fn(); source.onScrollTo = scroll;
    await source.ensureRange(500, 519); source.setFocus(511, 510);
    vi.mocked(api.locate).mockResolvedValue(610);
    await source.onViewChanged(2, fixture.total);
    expect(scroll).toHaveBeenLastCalledWith(600);
    await source.ensureRange(fixture.total - 20, fixture.total - 1);
    await source.onViewChanged(3, fixture.total + 10);
    expect(scroll).toHaveBeenLastCalledWith(fixture.total - 10);
    await source.ensureRange(100, 119);
    await source.onViewChanged(4, fixture.total + 20);
    expect(scroll).not.toHaveBeenLastCalledWith(fixture.total);
  });
  // spec: UI-140
  it("does not tail in sorted views, ignores older ticks, and releases all cached state on close", async () => {
    const { source, api } = setup(); const scroll = vi.fn(); source.onScrollTo = scroll;
    source.setSorted(true); await source.ensureRange(fixture.total - 20, fixture.total - 1);
    await source.onViewChanged(2, fixture.total + 10);
    expect(scroll).toHaveBeenLastCalledWith(fixture.total - 20);
    await source.onViewChanged(1, 0); expect(source.total).toBe(fixture.total + 10);
    await source.dispose(); await source.dispose();
    expect(source.cachedRows).toBe(0); expect(api.close).toHaveBeenCalledTimes(1);
  });
  // spec: UI-142
  it("keeps the previous rows on screen until the new version is complete and then swaps rows, total and scroll together (A01-07)", async () => {
    const { source, api } = setup();
    const scroll = vi.fn(); source.onScrollTo = scroll;
    await source.ensureRange(0, 19);
    expect(source.getCell(0, 5)).toBe(6);
    let release!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const original = vi.mocked(api.rows).getMockImplementation()!;
    vi.mocked(api.rows).mockImplementationOnce(async a => { await gate; return original(a); });
    const change = source.onViewChanged(2, fixture.total + 3);
    await Promise.resolve();
    // While the new window is on its way nothing is cleared: no placeholders, old version and total stay in place.
    expect(source.getCell(0, 5)).toBe(6);
    expect(source.version).toBe(1); expect(source.total).toBe(fixture.total);
    expect(scroll).not.toHaveBeenCalled();
    release(); await change;
    expect(source.version).toBe(2); expect(source.total).toBe(fixture.total + 3);
    expect(source.getCell(0, 5)).toBe(6);
    expect(scroll).toHaveBeenCalledTimes(1);
  });
  // spec: UI-142
  it("coalesces announcements that arrive during a swap and ends on the latest version (A01-20)", async () => {
    const { source, api } = setup();
    await source.ensureRange(0, 19);
    let release!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const original = vi.mocked(api.rows).getMockImplementation()!;
    vi.mocked(api.rows).mockImplementationOnce(async a => { await gate; return original(a); });
    const first = source.onViewChanged(2, fixture.total + 1);
    const second = source.onViewChanged(3, fixture.total + 2);
    const third = source.onViewChanged(4, fixture.total + 3);
    release(); await Promise.all([first, second, third]);
    expect(source.version).toBe(4); expect(source.total).toBe(fixture.total + 3);
    expect(source.getCell(0, 5)).toBe(6);
  });
});

// spec: UI-033
// spec: UI-145
it("requests a detail reload on stale pages and keeps cached rows until replacement", async () => {
  const { source, api } = setup();
  source.onStale = vi.fn(); source.onError = vi.fn();
  await source.ensureRow(0);
  vi.mocked(api.rows).mockRejectedValueOnce({ code: "Stale", messageKey: "err-view-stale" });
  await source.ensureRow(200);
  expect(source.onStale).toHaveBeenCalledTimes(1);
  expect(source.onError).not.toHaveBeenCalled();
  expect(source.getCell(0, 0)).toBe(1);
  expect(source.getCell(0, 200)).toBe(LOADING);
  await source.onViewChanged(2, fixture.total + 1);
  expect(source.version).toBe(2);
  expect(source.rowKey(0)).toBe(1);
  await source.dispose();
});
  // spec: UI-018
  it("does not jump back when the user leaves the tail while a swap is running (F5)", async () => {
    const { source, api } = setup();
    await source.ensureRange(4_999_970, 4_999_999);
    const release: (() => void)[] = [];
    const rows = vi.mocked(api.rows).getMockImplementation()!;
    vi.mocked(api.rows).mockImplementation(args => new Promise(resolve => { release.push(() => resolve(rows(args))); }));
    const scrolled: number[] = []; source.onScrollTo = row => scrolled.push(row);
    const swap = source.onViewChanged(fixture.version + 1, 5_000_100);
    await vi.waitFor(() => expect(release.length).toBeGreaterThan(0));
    // Mouse wheel upwards: the source learns it at once, before the grid has rendered the new position.
    source.userScrolled(true);
    while (release.length) release.shift()!();
    vi.mocked(api.rows).mockImplementation(rows);
    await swap;
    expect(scrolled).toEqual([4_999_970]);
  });
  // spec: UI-012
  it("resolves a range through the core, retries a stale answer and reports a missing command as unavailable (F3)", async () => {
    const { source, api } = setup();
    expect(await source.range(1, 2)).toBeUndefined();
    const answer = { viewId: fixture.viewId, version: 1, fromRow: 0, toRow: 1, total: 2, keys: [1, 2], all: false, exportRequired: false };
    const range = vi.fn().mockRejectedValueOnce(new IpcError(localError("Stale", "err-stale"))).mockResolvedValue(answer);
    (api as { range?: unknown }).range = range;
    await expect(source.range(1, 2)).resolves.toEqual(answer);
    expect(range).toHaveBeenCalledTimes(2);
    range.mockRejectedValueOnce(new IpcError(localError("NotFound", "err-unknown-command")));
    await expect(source.range(1, 2)).resolves.toBeUndefined();
  });
// spec: UI-018
it("a selected row holds the table at the bottom until the next scroll gesture", async () => {
  const { source } = setup();
  const scrolled: number[] = []; source.onScrollTo = row => scrolled.push(row);
  await source.ensureRange(4_999_970, 4_999_999);
  source.holdPosition();
  await source.ensureRange(4_999_970, 4_999_999);
  await source.onViewChanged(fixture.version + 1, 5_000_010);
  expect(scrolled.at(-1)).toBe(4_999_970);
  source.userScrolled(false);
  await source.ensureRange(4_999_980, 5_000_009);
  await source.onViewChanged(fixture.version + 2, 5_000_020);
  expect(scrolled.at(-1)).toBe(4_999_990);
});

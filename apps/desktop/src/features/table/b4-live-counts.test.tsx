import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { eventBus } from "../../ipc/events";
import { IpcError, localError } from "../../ipc/errors";
import type { CrawlStatus } from "../../ipc/gen/CrawlStatus";
import type { RowWindow } from "../../ipc/gen/RowWindow";
import type { Tick } from "../../ipc/gen/Tick";
import { useTabs } from "../../state/tabs";
import { useColumns } from "../../state/columns";
import { attachSession, useSession } from "../../state/session";
import { setLanguage } from "../../i18n";
import { flushUiState } from "../../state/uistate";
import fixtureTick from "../../ipc/__fixtures__/Tick.json";
import { UpperTable } from "./UpperTable";
import { fixtureTabs, tableFixture } from "./fixtures/core";

const capture = vi.hoisted(() => ({ grid: undefined as DataGridProps | undefined, renders: 0 }));
vi.mock("../../grid/DataGrid", async original => {
  const lib = await original<typeof import("../../grid/DataGrid")>();
  return { ...lib, DataGrid: (props: DataGridProps) => { capture.grid = props; capture.renders++; return <div data-testid="grid" />; } };
});
let fx: ReturnType<typeof tableFixture>;
let detach: () => void;
const tick = (seq: number, total: number): Tick => ({ seq, at: fixtureTick.at,
  views: [{ viewId: capture.grid!.source.viewId, version: seq, total }],
  countsVersion: 1, issuesVersion: 1, sidebarVersion: 1, ops: [], notices: [] });
beforeEach(async () => {
  fx = tableFixture(); setClient(createClient(fx.core.transport()));
  useTabs.setState({ catalog: fixtureTabs, order: fixtureTabs.map(t => t.id), active: "Internal", hidden: [], queries: {}, loaded: true, focusId: undefined, focusRequest: 0, error: null });
  useColumns.setState({ metadata: {}, byTab: {} });
  useSession.setState({ seq: -1, views: {}, ops: {}, crawl: { ...(fixtureTick.crawl as CrawlStatus), phase: "running" } });
  capture.grid = undefined; capture.renders = 0;
  detach = attachSession(eventBus);
  await setLanguage("en");
});
afterEach(async () => { detach(); await flushUiState(); cleanup(); setClient(null); vi.restoreAllMocks(); });
async function mounted() {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBe(1));
  // Prime the shared counts watermark; subsequent ticks contain only new view metadata.
  act(() => eventBus.emit({ kind: "tick", tick: tick(1, 3) }));
  await waitFor(() => expect(fx.core.calls.filter(c => c.cmd === "catalog.filterCounts").length).toBeGreaterThan(1));
  await act(async () => { await capture.grid!.source.settled(); });
}

// spec: UI-018, UI-145
it("publishes footer, chip and active list count while the fixture row window is still pending, without rendering the grid", async () => {
  await mounted();
  let rejectRows!: (error: unknown) => void;
  fx.core.handlers.set("view.rows", () => new Promise<RowWindow>((_resolve, reject) => { rejectRows = reject; }));
  const source = capture.grid!.source;
  const renders = capture.renders;
  act(() => eventBus.emit({ kind: "tick", tick: tick(2, 4) }));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Showing: 4 of 4"));
  expect(screen.getByRole("button", { name: "All (4)" })).toBeVisible();
  expect(source.version).toBe(1); expect(source.total).toBe(3);
  expect(capture.renders).toBe(renders);
  // F7: a row request can outlive its version. Count metadata remains valid after Stale.
  await act(async () => { rejectRows(new IpcError(localError("Stale", "err-view-stale"))); await source.settled(); });
  expect(source.version).toBe(1);
  expect(screen.getByRole("status")).toHaveTextContent("Showing: 4 of 4");
  fireEvent.click(screen.getByRole("button", { name: /Filter/ }));
  expect(await screen.findByRole("option", { name: /All.*4/ })).toBeVisible();
});

// spec: UI-018
it("uses only the current view's metadata and falls back to its row total for an older announcement", async () => {
  await mounted();
  const source = capture.grid!.source;
  act(() => useSession.setState({ views: { foreign: { viewId: "foreign", version: 99, total: 99 },
    [source.viewId]: { viewId: source.viewId, version: 0, total: 99 } } }));
  expect(screen.getByRole("status")).toHaveTextContent("Showing: 3 of 3");
  expect(screen.getByRole("button", { name: "All (3)" })).toBeVisible();
});

// spec: UI-018
it("retains the known All denominator before the shared fixture count has arrived", async () => {
  fx.core.handlers.set("catalog.filterCounts", () => ({ version: 1,
    filters: fixtureTabs.flatMap(tab => tab.filters.map(filter => ({ tab: tab.id, filter: { ...filter, count: null } }))) }));
  await mounted();
  expect(screen.getByRole("status")).toHaveTextContent("Showing: 3 of 3");
  expect(screen.getByRole("button", { name: "All (3)" })).toBeVisible();
});

// B4-LAGFIX2: Nach dem Start eines neuen Crawls öffnet die obere Tabelle ihre View, sobald der erste Tick angewendet ist,
// und wartet dafür nicht auf die Spaltenliste des Katalogs. Die Spalten der Registerkarte aus der vorigen Öffnung
// starten die View; die Katalogantwort bestätigt sie nur. Fuß und Chip zeigen die Zahl des neuen Crawls.
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { eventBus } from "../../ipc/events";
import type { CrawlStatus } from "../../ipc/gen/CrawlStatus";
import type { Tick } from "../../ipc/gen/Tick";
import type { ColumnMeta } from "../../ipc/gen/ColumnMeta";
import type { ViewOpenArgs } from "../../ipc/gen/ViewOpenArgs";
import { useTabs } from "../../state/tabs";
import { useColumns } from "../../state/columns";
import { attachSession, useSession } from "../../state/session";
import { setLanguage } from "../../i18n";
import { flushUiState } from "../../state/uistate";
import fixtureTick from "../../ipc/__fixtures__/Tick.json";
import { UpperTable } from "./UpperTable";
import { fixtureColumns, fixtureTabs, tableFixture } from "./fixtures/core";

const capture = vi.hoisted(() => ({ grid: undefined as DataGridProps | undefined }));
vi.mock("../../grid/DataGrid", async original => {
  const lib = await original<typeof import("../../grid/DataGrid")>();
  return { ...lib, DataGrid: (props: DataGridProps) => { capture.grid = props; return <div data-testid="grid" />; } };
});
let fx: ReturnType<typeof tableFixture>;
let detach: () => void;
const running = (crawlId: string, crawled: number): CrawlStatus => ({ ...(fixtureTick.crawl as CrawlStatus), crawlId, phase: "running", crawled });
const tick = (seq: number, crawl: CrawlStatus): Tick => ({ seq, at: fixtureTick.at, crawl, views: [], countsVersion: seq, issuesVersion: 1, sidebarVersion: 1, ops: [], notices: [] });
beforeEach(async () => {
  fx = tableFixture(); setClient(createClient(fx.core.transport()));
  useTabs.setState({ catalog: fixtureTabs, order: fixtureTabs.map(t => t.id), active: "Internal", hidden: [], queries: {}, loaded: true, focusId: undefined, focusRequest: 0, error: null });
  useColumns.setState({ metadata: {}, byTab: {} });
  useSession.setState({ seq: -1, views: {}, ops: {}, crawl: running("crawl-a", 3) });
  capture.grid = undefined;
  detach = attachSession(eventBus);
  await setLanguage("en");
});
afterEach(async () => { detach(); await flushUiState(); cleanup(); setClient(null); vi.restoreAllMocks(); });

// spec: UI-018
it("opens the new crawl's view on its first tick without waiting for the column catalog", async () => {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBe(1));
  const first = capture.grid!.source.viewId;
  // From now on the catalog does not answer column requests: the crawl switch must not depend on them.
  const pending: ((value: ColumnMeta[]) => void)[] = [];
  fx.core.handlers.set("catalog.columns", () => new Promise<ColumnMeta[]>(resolve => { pending.push(resolve); }));
  fx.core.handlers.set("catalog.filterCounts", () => new Promise(() => undefined));
  const opens = () => fx.core.calls.filter(c => c.cmd === "view.open").map(c => c.args as ViewOpenArgs);
  const before = opens().length;
  act(() => eventBus.emit({ kind: "tick", tick: tick(10, running("crawl-b", 0)) }));
  // The open is requested synchronously with the tick, before any rendering effect, and only once.
  expect(opens().length).toBe(before + 1);
  expect(opens().at(-1)!.columns).toEqual(fixtureColumns.map(c => c.key));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first));
  expect(screen.getByRole("status")).toHaveTextContent("Showing: 3 of 3");
  expect(screen.getByRole("button", { name: "All (3)" })).toBeVisible();
  // The late catalog answer with unchanged columns keeps the view.
  await act(async () => { for (const resolve of pending) resolve(fixtureColumns); await Promise.resolve(); });
  expect(opens().length).toBe(before + 1);
});

// spec: UI-018
it("reopens with the catalog's columns when they changed for the new crawl", async () => {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBe(1));
  const changed = fixtureColumns.slice(0, -1);
  fx.core.handlers.set("catalog.columns", () => changed);
  const opens = () => fx.core.calls.filter(c => c.cmd === "view.open").map(c => c.args as ViewOpenArgs);
  const before = opens().length;
  act(() => eventBus.emit({ kind: "tick", tick: tick(10, running("crawl-b", 0)) }));
  await waitFor(() => expect(opens().length).toBe(before + 2));
  expect(opens().at(-1)!.columns).toEqual(changed.map(c => c.key));
  await waitFor(() => expect(capture.grid?.source.viewId).toBe(`fixture-${opens().length}`));
  expect(fx.core.calls.some(c => c.cmd === "view.close")).toBe(true);
});

// spec: UI-018
it("loading the new crawl's segments does not open the table a second time", async () => {
  fx.core.handlers.set("segment.list", () => []);
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBe(1));
  const opens = () => fx.core.calls.filter(c => c.cmd === "view.open").length;
  const lists = () => fx.core.calls.filter(c => c.cmd === "segment.list").length;
  const before = opens(); const listed = lists();
  act(() => eventBus.emit({ kind: "tick", tick: tick(10, running("crawl-b", 0)) }));
  await waitFor(() => expect(lists()).toBeGreaterThan(listed));
  await waitFor(() => expect(capture.grid?.source.viewId).toBe(`fixture-${before + 1}`));
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
  expect(opens()).toBe(before + 1);
  expect(screen.getByRole("button", { name: "All (3)" })).toBeVisible();
});

// spec: UI-018
it("a query change while the early open of a new crawl is in flight leaves exactly one open view", async () => {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBe(1));
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const open = fx.core.handlers.get("view.open")!;
  fx.core.handlers.set("view.open", async (...call: Parameters<typeof open>) => { await gate; return open(...call); });
  act(() => eventBus.emit({ kind: "tick", tick: tick(10, running("crawl-b", 0)) }));
  act(() => useTabs.getState().query({ filter: "Client Error (4xx)" }));
  await act(async () => { release(); await new Promise(r => setTimeout(r, 50)); });
  await waitFor(() => expect(capture.grid?.source.total).toBe(1));
  const opened = fx.core.calls.filter(c => c.cmd === "view.open").length;
  const closed = new Set(fx.core.calls.filter(c => c.cmd === "view.close").map(c => (c.args as { viewId: string }).viewId));
  expect(opened - closed.size).toBe(1);
});

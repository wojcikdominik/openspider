// rc.1 (Handtest beta.4): Während eines laufenden Crawls zeigte die obere Tabelle leere Zeilen und „Diese Ansicht ist
// nicht mehr verfügbar.“. Der Kern gibt eine View frei, wenn ihre Crawl-Sitzung ersetzt oder geleert wird (`view.*`
// antwortet dann `NotFound`/`err-view-not-found`), und meldet `Stale`, solange eine View zu einer veralteten
// Sitzung gehört. Die Tabelle holt sich in beiden Fällen selbst eine neue View, ohne Meldung und ohne leere Zeilen.
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { IpcError, localError } from "../../ipc/errors";
import { eventBus } from "../../ipc/events";
import type { CrawlStatus } from "../../ipc/gen/CrawlStatus";
import type { Tick } from "../../ipc/gen/Tick";
import type { ViewCloseArgs } from "../../ipc/gen/ViewCloseArgs";
import { useTabs } from "../../state/tabs";
import { useColumns } from "../../state/columns";
import { attachSession, useSession } from "../../state/session";
import { setLanguage } from "../../i18n";
import { flushUiState } from "../../state/uistate";
import fixtureTick from "../../ipc/__fixtures__/Tick.json";
import { UpperTable } from "./UpperTable";
import { STALE_RECOVERY_MS } from "./controller";
import { fixtureTabs, tableFixture } from "./fixtures/core";

const capture = vi.hoisted(() => ({ grid: undefined as DataGridProps | undefined }));
vi.mock("../../grid/DataGrid", async original => {
  const lib = await original<typeof import("../../grid/DataGrid")>();
  return { ...lib, DataGrid: (props: DataGridProps) => { capture.grid = props; return <div data-testid="grid" />; } };
});
let fx: ReturnType<typeof tableFixture>;
let detach: () => void;
/** View IDs the core has released; it answers every `view.*` call for them like `crates/os-app` does. */
let released: Set<string>;
/** View IDs whose session is outdated in the core: rows and locate answer `Stale` for every version. */
let outdated: Set<string>;
const gone = () => new IpcError(localError("NotFound", "err-view-not-found"));
const running = (crawlId: string, crawled: number): CrawlStatus => ({ ...(fixtureTick.crawl as CrawlStatus), crawlId, phase: "running", crawled });
const tick = (seq: number, crawl: CrawlStatus, views: Tick["views"] = []): Tick => ({ seq, at: fixtureTick.at, crawl, views, countsVersion: seq, issuesVersion: 1, sidebarVersion: 1, ops: [], notices: [] });
const banner = () => screen.queryByText("This view is no longer available.");
beforeEach(async () => {
  fx = tableFixture(); released = new Set(); outdated = new Set();
  for (const name of ["view.rows", "view.locate", "view.close"]) {
    const handler = fx.core.handlers.get(name)!;
    fx.core.handlers.set(name, (args, ctx) => {
      const { viewId } = args as ViewCloseArgs;
      if (released.has(viewId)) throw gone();
      if (outdated.has(viewId) && name !== "view.close") throw new IpcError(localError("Stale", "err-view-stale"));
      return handler(args, ctx);
    });
  }
  setClient(createClient(fx.core.transport()));
  useTabs.setState({ catalog: fixtureTabs, order: fixtureTabs.map(t => t.id), active: "Internal", hidden: [], queries: {}, loaded: true, focusId: undefined, focusRequest: 0, error: null });
  useColumns.setState({ metadata: {}, byTab: {} });
  useSession.setState({ seq: -1, views: {}, ops: {}, crawl: running("crawl-a", 3) });
  capture.grid = undefined;
  detach = attachSession(eventBus);
  await setLanguage("en");
});
afterEach(async () => { detach(); await flushUiState(); cleanup(); setClient(null); vi.restoreAllMocks(); });

// spec: UI-142
// spec: UI-018
it("starting a new crawl releases the old view in the core; the table shows the new crawl without a message", async () => {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBe(1));
  const old = capture.grid!.source.viewId;
  // crawl.start closes the old session first: the core forgets every view of crawl-a before the window learns crawl-b.
  released.add(old);
  act(() => eventBus.emit({ kind: "tick", tick: tick(10, running("crawl-b", 0)) }));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(old));
  await waitFor(() => expect(capture.grid!.source.rowKey(0)).toBe(1));
  // Closing the released view answered NotFound: that must not surface as an error above the table.
  await waitFor(() => expect(fx.core.calls.some(c => c.cmd === "view.close" && (c.args as ViewCloseArgs).viewId === old)).toBe(true));
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
  expect(banner()).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

// spec: UI-142
// spec: UI-018
it("a view the core released while the crawl runs is replaced silently; the rows stay on screen", async () => {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBe(1));
  const first = capture.grid!.source;
  const opens = () => fx.core.calls.filter(c => c.cmd === "view.open").length;
  const before = opens();
  released.add(first.viewId);
  // The next live update of the released view fails in the core.
  act(() => eventBus.emit({ kind: "tick", tick: tick(11, running("crawl-a", 4), [{ viewId: first.viewId, version: 2, total: 3 }]) }));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId));
  const replacement = capture.grid!.source;
  // The replacement arrives with its visible rows; the released view kept its rows until then.
  expect(replacement.rowKey(0)).toBe(1);
  expect(replacement.getCell(0, 0)).not.toBeNull();
  expect(opens()).toBe(before + 1);
  expect(banner()).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

// spec: UI-142
it("a view that stays Stale without a newer version (outdated session) is replaced instead of showing empty rows", async () => {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBe(1));
  const first = capture.grid!.source;
  // E.g. the crawl configuration changed: the core answers Stale for every request of this handle.
  outdated.add(first.viewId);
  act(() => eventBus.emit({ kind: "tick", tick: tick(12, running("crawl-a", 4), [{ viewId: first.viewId, version: 2, total: 3 }]) }));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId), { timeout: STALE_RECOVERY_MS + 2000 });
  expect(capture.grid!.source.rowKey(0)).toBe(1);
  expect(screen.queryByRole("alert")).toBeNull();
});

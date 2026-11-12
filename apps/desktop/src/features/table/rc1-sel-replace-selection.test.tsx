// rc.1 (RC1-SEL, CI ui-e2e seit 3534baff): Auf langsamen Rechnern bleibt die View bei 300 URL/s länger als
// STALE_RECOVERY_MS `Stale`, die Tabelle ersetzt sie still (UI-142). Der Ersatz galt der Tabelle als neue Ansicht: Sie
// sprang zur Fokuszeile und reduzierte eine Mehrfachauswahl auf diese eine Zeile; der nächste Umschalt+Klick ging von dort
// aus und markierte eine Zeile zu viel oder zu wenig. Ein stiller Ersatz derselben Ansicht lässt Auswahl, Anker und
// Scrollposition stehen.
import { act, cleanup, render, waitFor } from "@testing-library/react";
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
import { SelectionModel } from "../../state/selection";
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
/** View IDs whose session is outdated in the core: rows and locate answer `Stale` for every version. */
let outdated: Set<string>;
const running = (crawlId: string, crawled: number): CrawlStatus => ({ ...(fixtureTick.crawl as CrawlStatus), crawlId, phase: "running", crawled });
const tick = (seq: number, crawl: CrawlStatus, views: Tick["views"] = []): Tick => ({ seq, at: fixtureTick.at, crawl, views, countsVersion: seq, issuesVersion: 1, sidebarVersion: 1, ops: [], notices: [] });
beforeEach(async () => {
  fx = tableFixture(); outdated = new Set();
  for (const name of ["view.rows", "view.locate"]) {
    const handler = fx.core.handlers.get(name)!;
    fx.core.handlers.set(name, (args, ctx) => {
      if (outdated.has((args as ViewCloseArgs).viewId)) throw new IpcError(localError("Stale", "err-view-stale"));
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
// spec: UI-012
// spec: UI-018
it("a silent replacement keeps a Shift range with its anchor and does not scroll to the focus row", async () => {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(2)).toBe(3));
  const scrollTo = vi.fn();
  act(() => capture.grid!.controller?.({ scrollTo, bounds: () => undefined, visible: () => ({ x: 0, y: 0, width: 1, height: 3 }) }));
  // Click on URL 1, Shift+click on URL 3: three rows, anchor 1, focus 3.
  act(() => capture.grid!.onSelectionChange(SelectionModel.empty().select(1).range([1, 2, 3], 3)));
  const first = capture.grid!.source;
  // The view stays Stale without a newer version (slow machine at a high crawl rate): the table replaces it.
  outdated.add(first.viewId);
  act(() => eventBus.emit({ kind: "tick", tick: tick(20, running("crawl-a", 4), [{ viewId: first.viewId, version: 2, total: 3 }]) }));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId), { timeout: STALE_RECOVERY_MS + 2000 });
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
  const selection = capture.grid!.selection;
  expect(selection.value).toEqual({ kind: "ids", ids: [1, 2, 3] });
  // The next Shift+click extends from the original anchor, not from the focus row.
  expect(selection.anchor).toBe(1);
  expect(selection.focus).toBe(3);
  expect(scrollTo).not.toHaveBeenCalled();
});

// spec: UI-142
// spec: UI-018
it("a focus request still moves to the focus row after a silent replacement", async () => {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(2)).toBe(3));
  const scrollTo = vi.fn();
  act(() => capture.grid!.controller?.({ scrollTo, bounds: () => undefined, visible: () => ({ x: 0, y: 0, width: 1, height: 3 }) }));
  const first = capture.grid!.source;
  outdated.add(first.viewId);
  act(() => eventBus.emit({ kind: "tick", tick: tick(21, running("crawl-a", 4), [{ viewId: first.viewId, version: 2, total: 3 }]) }));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId), { timeout: STALE_RECOVERY_MS + 2000 });
  act(() => useTabs.setState({ focusId: 2, focusRequest: 1 }));
  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith(1));
  expect(capture.grid!.selection.value).toEqual({ kind: "ids", ids: [2] });
});

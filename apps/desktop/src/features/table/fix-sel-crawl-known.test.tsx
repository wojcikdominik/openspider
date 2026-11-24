// fix-sel (1.0.2, UI-018): Ein kurzer Crawl meldet seinen Status dem Fenster erst mit Crawl-Ende und automatischer Analyse.
// Die Tabelle zeigte seine Zeilen schon vorher; eine dort gewählte Zeile ging verloren, als die Crawl-ID ankam. Die erste
// Crawl-ID setzt die View auf dem Schirm nur fort, wenn dieses Fenster genau diesen Crawl begonnen hat (crawl.start/open
// mit dieser ID beantwortet) und die View danach geöffnet wurde. Jeder Beginn in diesem Fenster leert die Auswahl sofort;
// Inhalt von außen oder ein anderer Crawl gilt als neuer Crawl (Auswahl leer) – URL-IDs bezeichnen je Crawl andere URLs.
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { crawl } from "../../ipc/crawl";
import { crawlBegin, type CrawlBegin } from "../../ipc/crawlBegin";
import { eventBus } from "../../ipc/events";
import type { CrawlStatus } from "../../ipc/gen/CrawlStatus";
import type { Tick } from "../../ipc/gen/Tick";
import { useTabs } from "../../state/tabs";
import { useColumns } from "../../state/columns";
import { attachSession, useSession } from "../../state/session";
import { SelectionModel } from "../../state/selection";
import { setLanguage } from "../../i18n";
import { flushUiState } from "../../state/uistate";
import { runAction } from "../../ui/actions";
import fixtureTick from "../../ipc/__fixtures__/Tick.json";
import { useDetailSelection } from "../detail/frame/selection";
import { UpperTable } from "./UpperTable";
import { continuesCrawl } from "./controller";
import { useTableSelection } from "./selection";
import { fixtureTabs, tableFixture } from "./fixtures/core";

const capture = vi.hoisted(() => ({ grid: undefined as DataGridProps | undefined }));
vi.mock("../../grid/DataGrid", async original => {
  const lib = await original<typeof import("../../grid/DataGrid")>();
  return { ...lib, DataGrid: (props: DataGridProps) => { capture.grid = props; return <div data-testid="grid" />; } };
});
let fx: ReturnType<typeof tableFixture>;
let detach: () => void;
/** The crawl id the fixture core answers to the next `crawl.start`. */
let started: string;
const completed = (crawlId: string): CrawlStatus => ({ ...(fixtureTick.crawl as CrawlStatus), crawlId, phase: "completed", crawled: 3 });
const tick = (seq: number, crawl: CrawlStatus): Tick => ({ seq, at: fixtureTick.at, crawl, views: [], countsVersion: seq, issuesVersion: 1, sidebarVersion: 1, ops: [], notices: [] });
const opens = () => fx.core.calls.filter(c => c.cmd === "view.open").length;
const start = () => act(async () => { await crawl.start({ source: { kind: "list", items: ["http://example.test/"], label: "fix-sel" }, scope: "domain" }); });
const selectedInTable = async (id: number) => {
  await waitFor(() => expect(capture.grid?.source.rowKey(2)).toBe(3));
  act(() => capture.grid!.onSelectionChange(SelectionModel.empty().select(id)));
  expect(useDetailSelection.getState().selection.urlIds).toEqual([id]);
  return capture.grid!.source;
};
const expectNoSelection = () => {
  expect(capture.grid!.selection.value).toEqual({ kind: "ids", ids: [] });
  expect(useTableSelection.getState().selection.count(3)).toBe(0);
  expect(useDetailSelection.getState().selection.urlIds).toEqual([]);
  // Shift+F10 finds no focus row: no context menu, so Recrawl/Remove cannot act on any row.
  let opened = true;
  act(() => { opened = runAction("context.menu"); });
  expect(opened).toBe(false);
  expect(screen.queryByRole("menuitem", { name: "Recrawl" })).toBeNull();
};

describe("first crawl id for a view on screen", () => {
  beforeEach(async () => {
    fx = tableFixture(); started = "crawl-a";
    fx.core.handlers.set("crawl.start", () => ({ crawlId: started, file: `${started}.openspider` }));
    crawlBegin.reset();
    setClient(createClient(fx.core.transport()));
    useTabs.setState({ catalog: fixtureTabs, order: fixtureTabs.map(t => t.id), active: "Internal", hidden: [], queries: {}, loaded: true, focusId: undefined, focusRequest: 0, error: null });
    useColumns.setState({ metadata: {}, byTab: {} });
    useSession.setState({ seq: -1, views: {}, ops: {}, crawl: null, analysis: null });
    capture.grid = undefined;
    detach = attachSession(eventBus);
    await setLanguage("en");
  });
  afterEach(async () => { detach(); await flushUiState(); cleanup(); setClient(null); crawlBegin.reset(); vi.restoreAllMocks(); });

  // spec: UI-018
  it("continues only the crawl this window began, in a view opened after the answer", () => {
    const begun: CrawlBegin = { revision: 1, crawlId: "crawl-a" };
    expect(continuesCrawl("crawl-a", begun, begun)).toBe(true);
    // No begin in this window (content from elsewhere).
    expect(continuesCrawl("crawl-a", { revision: 0, crawlId: undefined }, { revision: 0, crawlId: undefined })).toBe(false);
    // View opened before the core answered the begin.
    expect(continuesCrawl("crawl-a", begun, { revision: 1, crawlId: undefined })).toBe(false);
    // View opened before a later begin.
    expect(continuesCrawl("crawl-b", { revision: 2, crawlId: "crawl-b" }, begun)).toBe(false);
    // Another crawl than the one begun.
    expect(continuesCrawl("crawl-b", begun, begun)).toBe(false);
    expect(continuesCrawl(undefined, begun, begun)).toBe(false);
    expect(continuesCrawl("crawl-a", begun, undefined)).toBe(false);
  });

  // spec: UI-018
  // spec: UI-142
  // spec: UI-030
  it("keeps selection, focus row and detail selection when the id of the crawl just begun arrives after the selection", async () => {
    await start();
    render(<UpperTable />);
    const scrollTo = vi.fn();
    await waitFor(() => expect(capture.grid?.source.rowKey(2)).toBe(3));
    act(() => capture.grid!.controller?.({ scrollTo, bounds: () => undefined, visible: () => ({ x: 0, y: 0, width: 1, height: 3 }) }));
    const first = await selectedInTable(2);
    const before = opens();
    act(() => eventBus.emit({ kind: "tick", tick: tick(10, completed("crawl-a")) }));
    // The view is opened again for the crawl now known and continues the one on screen.
    await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId));
    expect(opens()).toBe(before + 1);
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
    expect(capture.grid!.source.continues(first)).toBe(true);
    expect(capture.grid!.selection.value).toEqual({ kind: "ids", ids: [2] });
    expect(capture.grid!.selection.focus).toBe(2);
    expect(useTableSelection.getState().viewId).toBe(capture.grid!.source.viewId);
    expect(useDetailSelection.getState().selection).toMatchObject({ urlIds: [2], focus: 2, crawlId: "crawl-a" });
    expect(scrollTo).not.toHaveBeenCalled();
  });

  // spec: UI-018
  // spec: UI-110
  it("empties the selection when another crawl begins while rows of a crawl whose status never arrived are selected", async () => {
    // Rows of crawl A on screen (A's status never reached the window, no begin here), row 2 selected.
    render(<UpperTable />);
    const first = await selectedInTable(2);
    started = "crawl-b";
    await start();
    // The begin ends the selection at once, before B's id is known.
    expectNoSelection();
    act(() => eventBus.emit({ kind: "tick", tick: tick(12, completed("crawl-b")) }));
    await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId));
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
    // The view of A does not continue as B's view, and nothing is selected in B.
    expect(capture.grid!.source.continues(first)).toBe(false);
    expectNoSelection();
  });

  // spec: UI-018
  it("does not continue a view for a crawl id from elsewhere (no begin in this window)", async () => {
    render(<UpperTable />);
    const first = await selectedInTable(2);
    act(() => eventBus.emit({ kind: "tick", tick: tick(13, completed("crawl-a")) }));
    await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId));
    expect(capture.grid!.source.continues(first)).toBe(false);
    expectNoSelection();
  });

  // spec: UI-018
  it("still empties the selection for a crawl that replaces a known one", async () => {
    useSession.setState({ crawl: completed("crawl-a") });
    render(<UpperTable />);
    const first = await selectedInTable(2);
    act(() => eventBus.emit({ kind: "tick", tick: tick(11, completed("crawl-b")) }));
    await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId));
    expect(capture.grid!.source.continues(first)).toBe(false);
    expectNoSelection();
  });
});

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
import { IpcError, localError } from "../../ipc/errors";
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
import { continuesCrawl, type ViewOrigin } from "./controller";
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
  it("continues only the crawl this window began, in a view requested after the answer", () => {
    const begun: CrawlBegin = { revision: 1, crawlId: "crawl-a" };
    const after: ViewOrigin = { revision: 1, answer: "crawl-a", settled: true, proof: false, crawl: undefined };
    expect(continuesCrawl("crawl-a", begun, after)).toBe(true);
    // No begin in this window (content from elsewhere).
    expect(continuesCrawl("crawl-a", { revision: 0, crawlId: undefined }, { ...after, revision: 0, answer: undefined })).toBe(false);
    // View requested before the core answered the begin (also after it saw the core switch).
    expect(continuesCrawl("crawl-a", begun, { ...after, answer: undefined, settled: false, proof: true })).toBe(false);
    // View requested before a later begin.
    expect(continuesCrawl("crawl-b", { revision: 2, crawlId: "crawl-b" }, { ...after, answer: "crawl-b" })).toBe(false);
    // Another crawl than the one begun.
    expect(continuesCrawl("crawl-b", begun, after)).toBe(false);
    expect(continuesCrawl(undefined, begun, after)).toBe(false);
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

  // spec: UI-018
  // spec: UI-114
  it("hides the old rows from the begin until the answer and shows the new crawl's rows only from a view requested after it", async () => {
    let answer!: (value: unknown) => void;
    fx.core.handlers.set("crawl.start", () => new Promise(resolve => { answer = resolve; }));
    const { container } = render(<UpperTable />);
    const first = await selectedInTable(2);
    let pending: Promise<unknown> = Promise.resolve();
    act(() => { pending = crawl.start({ source: { kind: "list", items: ["http://example.test/"], label: "fix-sel" }, scope: "domain" }); });
    // The rows of the old content are out of reach at once; the table shows the state without a crawl.
    await waitFor(() => expect(container.querySelector('[data-empty-state="NO_DATA_YET"]')).not.toBeNull());
    expect(screen.getByTestId("grid").closest("[hidden]")).not.toBeNull();
    expectNoSelection();
    // A click into the hidden old rows selects nothing.
    act(() => capture.grid!.onSelectionChange(SelectionModel.empty().select(2)));
    expectNoSelection();
    const opensBefore = opens();
    answer({ crawlId: "crawl-b", file: "crawl-b.openspider" });
    await act(async () => { await pending; });
    // A view requested after the answer shows the new content; the old one is gone.
    await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId));
    expect(opens()).toBe(opensBefore + 1);
    await waitFor(() => expect(container.querySelector('[data-empty-state="NO_DATA_YET"]')).toBeNull());
    expect(screen.getByTestId("grid").closest("[hidden]")).toBeNull();
    expectNoSelection();
  });

  // spec: UI-018
  it("brings back the old rows without selection when the begin fails (the core keeps its content)", async () => {
    fx.core.handlers.set("crawl.start", () => { throw new IpcError(localError("InvalidInput", "err-start-url")); });
    const { container } = render(<UpperTable />);
    const first = await selectedInTable(2);
    const opensBefore = opens();
    await act(async () => { await crawl.start({ source: { kind: "spider", url: "bad" }, scope: "domain" } as never).catch(() => undefined); });
    await waitFor(() => expect(screen.getByTestId("grid").closest("[hidden]")).toBeNull());
    expect(container.querySelector('[data-empty-state="NO_DATA_YET"]')).toBeNull();
    // The same view: the core never released it.
    expect(capture.grid!.source.viewId).toBe(first.viewId);
    expect(opens()).toBe(opensBefore);
    expectNoSelection();
    act(() => capture.grid!.onSelectionChange(SelectionModel.empty().select(3)));
    expect(capture.grid!.selection.value).toEqual({ kind: "ids", ids: [3] });
  });

  // spec: UI-018
  it("a view request that failed while the begin was outstanding does not stop the continuation", async () => {
    let answer!: (value: unknown) => void;
    fx.core.handlers.set("crawl.start", () => new Promise(resolve => { answer = resolve; }));
    const open = fx.core.handlers.get("view.open")!;
    let noCrawl = false;
    fx.core.handlers.set("view.open", (args, ctx) => {
      if (noCrawl) throw new IpcError(localError("NoCrawl", "err-no-crawl"));
      return open(args, ctx);
    });
    render(<UpperTable />);
    await waitFor(() => expect(capture.grid?.source.rowKey(2)).toBe(3));
    let pending: Promise<unknown> = Promise.resolve();
    act(() => { pending = crawl.start({ source: { kind: "list", items: ["http://example.test/"], label: "fix-sel" }, scope: "domain" }); });
    // While the answer is outstanding the user switches the filter; the core answers that request with an error.
    noCrawl = true;
    const failed = fx.core.calls.filter(c => c.cmd === "view.open").length;
    act(() => useTabs.getState().select("Internal", "Client Error (4xx)"));
    await waitFor(() => expect(fx.core.calls.filter(c => c.cmd === "view.open").length).toBeGreaterThan(failed));
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
    noCrawl = false;
    answer({ crawlId: "crawl-b", file: "crawl-b.openspider" });
    await act(async () => { await pending; });
    // The view requested after the answer shows crawl B (4xx: URL 2); a choice in it stays when B's status arrives.
    await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBe(2));
    act(() => capture.grid!.onSelectionChange(SelectionModel.empty().select(2)));
    const before = capture.grid!.source.viewId;
    act(() => eventBus.emit({ kind: "tick", tick: tick(14, completed("crawl-b")) }));
    await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(before));
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
    expect(capture.grid!.selection.value).toEqual({ kind: "ids", ids: [2] });
  });
});

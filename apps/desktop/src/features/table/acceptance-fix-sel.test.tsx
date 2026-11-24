// Abnahme fix-sel (unabhängiger Testagent, nur aus Anforderung und Spec UI-018/UI-142/UI-114): Auswahl der oberen Tabelle
// bei Crawl-Wechseln. Punkt 1: Eine Auswahl im Crawl, den dieses Fenster begonnen hat, bleibt, wenn dessen Status später
// ankommt. Punkt 2: Nie eine Auswahl in einen anderen Crawl mitnehmen; im Zweifel leeren. Punkt 3: bisheriges Verhalten.
// Der Kern wird mit der Fixture nachgestellt: Er gibt beim Beginn neuen Inhalts die Views des alten frei (NotFound) und
// liefert danach die Zeilen des neuen Crawls mit denselben URL-Nummern (URL-Nummern gelten je Crawl).
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { crawl } from "../../ipc/crawl";
import { crawlBegin } from "../../ipc/crawlBegin";
import { IpcError, localError } from "../../ipc/errors";
import { eventBus } from "../../ipc/events";
import type { AnalysisStatus } from "../../ipc/gen/AnalysisStatus";
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
import { useDetailSelection } from "../detail/frame/selection";
import { UpperTable } from "./UpperTable";
import { useTableSelection } from "./selection";
import { fixtureTabs, tableFixture } from "./fixtures/core";

const capture = vi.hoisted(() => ({ grid: undefined as DataGridProps | undefined }));
vi.mock("../../grid/DataGrid", async original => {
  const lib = await original<typeof import("../../grid/DataGrid")>();
  return { ...lib, DataGrid: (props: DataGridProps) => { capture.grid = props; return <div data-testid="grid" />; } };
});

type Row = { id: number; Address: string; "Status Code": number; "Word Count": number | null; "Title 1": string; "Meta Keywords 1": string };
const row = (id: number, address: string, status = 200): Row => ({ id, Address: address, "Status Code": status, "Word Count": 1, "Title 1": address, "Meta Keywords 1": "" });
/** Another crawl: the same URL ids 1–4 stand for other addresses. */
const rowsB: Row[] = [row(1, "https://b.invalid/1"), row(2, "https://b.invalid/2", 404), row(3, "https://b.invalid/3"), row(4, "https://b.invalid/4", 404)];
const rowsC: Row[] = [row(1, "https://c.invalid/1"), row(2, "https://c.invalid/2", 404), row(3, "https://c.invalid/3"), row(4, "https://c.invalid/4"), row(5, "https://c.invalid/5")];

let fx: ReturnType<typeof tableFixture>;
let detach: () => void;
let released: Set<string>;
let seq = 10;
/** Gate for `view.open`: while set, every open waits for it. */
let viewGate: Promise<void> | undefined;
const status = (crawlId: string, phase: CrawlStatus["phase"] = "running"): CrawlStatus => ({ ...(fixtureTick.crawl as CrawlStatus), crawlId, phase });
const analysisDone: AnalysisStatus = { steps: [], running: false };
const tick = (crawlStatus: CrawlStatus | null, views: Tick["views"] = [], analysis: AnalysisStatus | null = null): Tick =>
  ({ seq: seq++, at: fixtureTick.at, crawl: crawlStatus, analysis, views, countsVersion: seq, issuesVersion: 1, sidebarVersion: 1, ops: [], notices: [] });
const gone = () => new IpcError(localError("NotFound", "err-view-not-found"));
function deferred<T>() {
  let resolve!: (value: T) => void; let reject!: (error: unknown) => void;
  const promise = new Promise<T>((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
}
const settle = () => act(async () => { await new Promise(resolve => setTimeout(resolve, 30)); });

beforeEach(async () => {
  fx = tableFixture(); released = new Set(); viewGate = undefined;
  for (const name of ["view.rows", "view.locate", "view.close"]) {
    const handler = fx.core.handlers.get(name)!;
    fx.core.handlers.set(name, (args, ctx) => {
      if (released.has((args as ViewCloseArgs).viewId)) throw gone();
      return handler(args, ctx);
    });
  }
  const open = fx.core.handlers.get("view.open")!;
  fx.core.handlers.set("view.open", async (args, ctx) => { if (viewGate) await viewGate; return open(args, ctx); });
  setClient(createClient(fx.core.transport()));
  crawlBegin.reset();
  useTabs.setState({ catalog: fixtureTabs, order: fixtureTabs.map(t => t.id), active: "Internal", hidden: [], queries: {}, loaded: true, focusId: undefined, focusRequest: 0, error: null });
  useColumns.setState({ metadata: {}, byTab: {} });
  useSession.setState({ seq: -1, views: {}, ops: {}, crawl: status("crawl-a", "completed"), analysis: null });
  capture.grid = undefined;
  detach = attachSession(eventBus);
  await setLanguage("en");
});
afterEach(async () => { detach(); await flushUiState(); cleanup(); setClient(null); crawlBegin.reset(); vi.restoreAllMocks(); });

async function mounted() {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(2)).toBe(3));
  const scrollTo = vi.fn();
  act(() => capture.grid!.controller?.({ scrollTo, bounds: () => undefined, visible: () => ({ x: 0, y: 0, width: 1, height: 3 }) }));
  return scrollTo;
}
const select = (selection: SelectionModel) => act(() => capture.grid!.onSelectionChange(selection));
const pick = (...ids: number[]) => select(ids.length === 1 ? SelectionModel.empty().select(ids[0]!) : SelectionModel.empty().select(ids[0]!).range(ids, ids[ids.length - 1]!));
const ids = () => { const value = capture.grid!.selection.value; return value.kind === "ids" ? value.ids : "all"; };
async function expectKept(expected: number[], focus: number) {
  expect(ids()).toEqual(expected);
  expect(capture.grid!.selection.focus).toBe(focus);
  expect(useTableSelection.getState().selection.value).toEqual({ kind: "ids", ids: expected });
  await waitFor(() => expect(useDetailSelection.getState().selection).toMatchObject({ urlIds: expected, focus }));
}
async function expectEmpty() {
  expect(ids()).toEqual([]);
  expect(capture.grid!.selection.focus).toBeUndefined();
  expect(useTableSelection.getState().selection.value).toEqual({ kind: "ids", ids: [] });
  await waitFor(() => expect(useDetailSelection.getState().selection.urlIds).toEqual([]));
}
/** The core switched to other content and released the old view; the window notices on its next request. */
async function coreSwitches(rows: Row[]) {
  const old = capture.grid!.source;
  released.add(old.viewId); fx.replaceRows(rows);
  act(() => eventBus.emit({ kind: "tick", tick: tick(null, [{ viewId: old.viewId, version: old.version + 50, total: rows.length }]) }));
  await waitFor(() => expect(capture.grid!.source.viewId).not.toBe(old.viewId));
  await waitFor(() => expect(capture.grid!.source.total).toBe(rows.length));
}
/** The window learns a crawl's status (id); waits until the table has reopened its view for it. */
async function statusArrives(crawlStatus: CrawlStatus, analysis: AnalysisStatus | null = null) {
  const before = capture.grid!.source.viewId;
  act(() => eventBus.emit({ kind: "tick", tick: tick(crawlStatus, [], analysis) }));
  expect(useSession.getState().crawl?.crawlId).toBe(crawlStatus.crawlId);
  await waitFor(() => expect(capture.grid!.source.viewId).not.toBe(before));
  await settle();
}
/** The user opens a view after the fact (filter there and back). */
async function reopenView() {
  let before = capture.grid!.source.viewId;
  act(() => useTabs.getState().select("Internal", "Client Error (4xx)"));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(before));
  before = capture.grid!.source.viewId;
  act(() => useTabs.getState().select("Internal", "All"));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(before));
  await waitFor(() => expect(capture.grid!.source.rowKey(0)).toBe(1));
}

// ---------------------------------------------------------------- Punkt 1 (A, D) ----------------------------------------

// spec: UI-018
// spec: UI-142
it("P1: a crawl begun here, its view opened after the answer: selection, focus row and position stay when its status arrives", async () => {
  const scrollTo = await mounted();
  fx.core.handlers.set("crawl.start", () => ({ crawlId: "crawl-b", file: "/b" }));
  await crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never);
  await coreSwitches(rowsB);
  await reopenView();
  scrollTo.mockClear();
  pick(2, 3);
  await statusArrives(status("crawl-b", "completed"), analysisDone);
  await expectKept([2, 3], 3);
  expect(scrollTo).not.toHaveBeenCalled();
});

// spec: UI-018
// spec: UI-142
it("P1: a crawl begun here, shown through the silent replacement after the answer: the selection made in it stays when its status arrives", async () => {
  // The usual course for the second crawl of a window: the core releases the old view, the table replaces it silently
  // (UI-142) and shows the new crawl's rows; the user selects; the status arrives with the end and the analysis.
  const scrollTo = await mounted();
  fx.core.handlers.set("crawl.start", () => ({ crawlId: "crawl-b", file: "/b" }));
  await crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never);
  await coreSwitches(rowsB);
  scrollTo.mockClear();
  pick(2, 3);
  await statusArrives(status("crawl-b", "completed"), analysisDone);
  await expectKept([2, 3], 3);
  expect(scrollTo).not.toHaveBeenCalled();
});

// spec: UI-018
it("P1: two starts in a row, the first answer arrives late: the selection in the second crawl stays", async () => {
  await mounted();
  const first = deferred<unknown>(); const second = deferred<unknown>(); let n = 0;
  fx.core.handlers.set("crawl.start", () => (++n === 1 ? first : second).promise);
  const one = crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never);
  const two = crawl.start({ source: { kind: "spider", url: "https://c.invalid/" }, scope: "domain" } as never);
  second.resolve({ crawlId: "crawl-c", file: "/c" }); await two;
  await coreSwitches(rowsC);
  await reopenView();
  pick(4);
  first.resolve({ crawlId: "crawl-b", file: "/b" }); await one;
  expect(crawlBegin.get()).toMatchObject({ crawlId: "crawl-c" });
  await statusArrives(status("crawl-c", "completed"));
  await expectKept([4], 4);
});

// ---------------------------------------------------------------- Punkt 2 (B, C) ----------------------------------------

// spec: UI-018
it("P2: starting, opening and clearing in this window empty the selection at once", async () => {
  await mounted();
  fx.core.handlers.set("crawl.start", () => ({ crawlId: "crawl-b", file: "/b" }));
  fx.core.handlers.set("crawl.open", () => ({ crawlId: "crawl-c", file: "/c" }));
  fx.core.handlers.set("crawl.clear", () => null);
  for (const begin of [
    () => crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never),
    () => crawl.open({ kind: "id", crawlId: "crawl-c" } as never),
    () => crawl.clear(),
  ]) {
    pick(2, 3);
    let pending: Promise<unknown> = Promise.resolve();
    act(() => { pending = begin(); });
    // Empty before the core answered.
    await expectEmpty();
    await act(async () => { await pending; });
    await expectEmpty();
  }
});

// spec: UI-018
it("P2: a row clicked in the old rows after the start was sent is not carried into the new crawl's rows", async () => {
  await mounted();
  const answer = deferred<unknown>();
  fx.core.handlers.set("crawl.start", () => answer.promise);
  pick(3);
  let pending: Promise<unknown> = Promise.resolve();
  act(() => { pending = crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never); });
  await expectEmpty();
  // The old rows are still on screen; the user clicks one of them (crawl A, URL 2).
  pick(2);
  answer.resolve({ crawlId: "crawl-b", file: "/b" }); await pending;
  await coreSwitches(rowsB);
  // The table now shows crawl B; URL 2 of crawl B was never chosen by the user.
  await expectEmpty();
  await statusArrives(status("crawl-b", "completed"), analysisDone);
  await expectEmpty();
});

// spec: UI-018
it("P2: the crawl id arrives before the answer to crawl.start: the selection is emptied; the late answer keeps a later choice", async () => {
  await mounted();
  const answer = deferred<unknown>();
  fx.core.handlers.set("crawl.start", () => answer.promise);
  const pending = crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never);
  await coreSwitches(rowsB);
  pick(2);
  await statusArrives(status("crawl-b"));
  await expectEmpty();
  // Now the window knows crawl B; a choice in it survives the late answer and further ticks of crawl B.
  pick(3);
  answer.resolve({ crawlId: "crawl-b", file: "/b" }); await act(async () => { await pending; });
  act(() => eventBus.emit({ kind: "tick", tick: tick(status("crawl-b", "completed"), [], analysisDone) }));
  await settle();
  await expectKept([3], 3);
});

// spec: UI-018
it("P2: the view was requested before the answer, the status arrives later: no selection survives (when in doubt, empty)", async () => {
  await mounted();
  const answer = deferred<unknown>();
  fx.core.handlers.set("crawl.start", () => answer.promise);
  const pending = crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never);
  await coreSwitches(rowsB);
  // A filter change requests a view while the answer is outstanding; the view arrives only after the answer.
  const gate = deferred<void>(); viewGate = gate.promise;
  const before = capture.grid!.source.viewId;
  const opens = fx.core.calls.filter(c => c.cmd === "view.open").length;
  act(() => useTabs.getState().select("Internal", "Client Error (4xx)"));
  await waitFor(() => expect(fx.core.calls.filter(c => c.cmd === "view.open").length).toBeGreaterThan(opens));
  answer.resolve({ crawlId: "crawl-b", file: "/b" }); await act(async () => { await pending; });
  viewGate = undefined; gate.resolve();
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(before));
  await waitFor(() => expect(capture.grid!.source.rowKey(0)).toBe(2));
  pick(2);
  await statusArrives(status("crawl-b", "completed"));
  await expectEmpty();
});

// spec: UI-018
it("P2: the first of two starts answers while the second is outstanding: the first crawl's status never keeps a selection", async () => {
  await mounted();
  const first = deferred<unknown>(); const second = deferred<unknown>(); let n = 0;
  fx.core.handlers.set("crawl.start", () => (++n === 1 ? first : second).promise);
  const one = crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never);
  const two = crawl.start({ source: { kind: "spider", url: "https://c.invalid/" }, scope: "domain" } as never);
  first.resolve({ crawlId: "crawl-b", file: "/b" }); await act(async () => { await one; });
  expect(crawlBegin.get().crawlId).toBeUndefined();
  await coreSwitches(rowsB);
  await reopenView();
  pick(2);
  await statusArrives(status("crawl-b", "completed"));
  await expectEmpty();
  second.resolve({ crawlId: "crawl-c", file: "/c" }); await act(async () => { await two; });
  await coreSwitches(rowsC);
  await statusArrives(status("crawl-c"));
  await expectEmpty();
});

// spec: UI-018
it("P2: a failed start empties the selection; a later choice in the unchanged crawl stays, another crawl from outside empties it", async () => {
  await mounted();
  fx.core.handlers.set("crawl.start", () => { throw new IpcError(localError("InvalidInput", "err-start-url")); });
  pick(2);
  const error = await crawl.start({ source: { kind: "spider", url: "bad" }, scope: "domain" } as never).then(() => undefined, (e: unknown) => e);
  expect(error).toBeInstanceOf(IpcError);
  await expectEmpty();
  pick(3);
  act(() => eventBus.emit({ kind: "tick", tick: tick(status("crawl-a", "completed"), [], analysisDone) }));
  await settle();
  await expectKept([3], 3);
  await coreSwitches(rowsB);
  await statusArrives(status("crawl-x", "completed"));
  await expectEmpty();
});

// spec: UI-018
it("P2: clear followed by start: nothing of the cleared crawl comes back; a choice in the new crawl stays", async () => {
  await mounted();
  fx.core.handlers.set("crawl.clear", () => null);
  fx.core.handlers.set("crawl.start", () => ({ crawlId: "crawl-b", file: "/b" }));
  pick(2, 3);
  await act(async () => { await crawl.clear(); });
  await expectEmpty();
  await act(async () => { await crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never); });
  await coreSwitches(rowsB);
  await expectEmpty();
  await reopenView();
  pick(1);
  await statusArrives(status("crawl-b", "completed"));
  await expectKept([1], 1);
});

// spec: UI-018
it("P2: content from outside (no begin in this window) empties the selection when its status arrives", async () => {
  await mounted();
  pick(2, 3);
  await coreSwitches(rowsB);
  await statusArrives(status("crawl-b", "completed"), analysisDone);
  await expectEmpty();
});

// spec: UI-018
it("P2: after a begin here, content from outside that replaces it still empties the selection", async () => {
  await mounted();
  fx.core.handlers.set("crawl.start", () => ({ crawlId: "crawl-b", file: "/b" }));
  await act(async () => { await crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never); });
  await coreSwitches(rowsB);
  await reopenView();
  // Another window (or CLI/MCP, scheduler) replaces the crawl before this window learned crawl B.
  pick(2);
  await coreSwitches(rowsC);
  await statusArrives(status("crawl-c", "completed"));
  await expectEmpty();
});

// ---------------------------------------------------------------- Punkt 3 (D) -------------------------------------------

// spec: UI-018
it("P3: switching between two known crawls empties the selection, also back to a crawl begun here", async () => {
  await mounted();
  fx.core.handlers.set("crawl.start", () => ({ crawlId: "crawl-b", file: "/b" }));
  await act(async () => { await crawl.start({ source: { kind: "spider", url: "https://b.invalid/" }, scope: "domain" } as never); });
  await coreSwitches(rowsB);
  await reopenView();
  await statusArrives(status("crawl-b", "completed"));
  pick(2);
  // Another window opens crawl A, later crawl B again: each switch empties.
  fx.replaceRows(rowsC);
  await statusArrives(status("crawl-a", "completed"));
  await expectEmpty();
  pick(3);
  fx.replaceRows(rowsB);
  await statusArrives(status("crawl-b", "completed"));
  await expectEmpty();
});

// spec: UI-018
// spec: UI-114
it("P3: analysis, resume of a paused crawl and recrawl of a URL keep the selection", async () => {
  const scrollTo = await mounted();
  useSession.setState({ crawl: status("crawl-a", "paused") });
  pick(1, 2);
  scrollTo.mockClear();
  act(() => eventBus.emit({ kind: "tick", tick: tick(status("crawl-a", "running")) }));
  await settle();
  fx.core.handlers.set("analysis.start", () => ({ opId: "analysis-1", labelKey: "analysis", cancellable: true }));
  const source = capture.grid!.source;
  fx.replaceRows([row(1, "https://fixture.invalid/a"), row(2, "https://fixture.invalid/b", 404), row(3, "https://fixture.invalid/c")]);
  act(() => eventBus.emit({ kind: "tick", tick: tick(status("crawl-a", "completed"), [{ viewId: source.viewId, version: source.version + 1, total: 3 }], { steps: [], running: true }) }));
  await settle();
  act(() => eventBus.emit({ kind: "tick", tick: tick(status("crawl-a", "completed"), [], analysisDone) }));
  await settle();
  await act(async () => { await crawl.recrawl({ selection: { kind: "ids", ids: [1, 2] } } as never); });
  fx.core.handlers.set("crawl.resume", () => undefined);
  await act(async () => { await crawl.resume(); });
  await settle();
  expect(crawlBegin.get().revision).toBe(0);
  await expectKept([1, 2], 2);
  expect(scrollTo).not.toHaveBeenCalled();
});

// spec: UI-012
// spec: UI-018
it("P3: a filter switch keeps a single selection only if the URL is in the new view", async () => {
  await mounted();
  pick(2);
  let before = capture.grid!.source.viewId;
  act(() => useTabs.getState().select("Internal", "Client Error (4xx)"));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(before));
  await settle();
  await expectKept([2], 2);
  before = capture.grid!.source.viewId;
  act(() => useTabs.getState().select("Internal", "All"));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(before));
  pick(1);
  before = capture.grid!.source.viewId;
  act(() => useTabs.getState().select("Internal", "Client Error (4xx)"));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(before));
  await settle();
  await expectEmpty();
});

// spec: UI-012
// spec: UI-018
it("P3: a filter switch keeps of a multi-selection only the URLs in the new view", async () => {
  await mounted();
  pick(1, 2);
  const before = capture.grid!.source.viewId;
  act(() => useTabs.getState().select("Internal", "Client Error (4xx)"));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(before));
  await settle();
  // Only URL 2 (404) is in the 4xx view.
  expect(ids()).not.toContain(1);
});

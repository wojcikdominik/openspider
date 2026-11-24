// Abnahme fix-sel, Runde Kern-Absicherung (unabhängiger Testagent, Festlegung des Orchestrators vom 24.11.2026): Aktionen
// auf einer Zeilenauswahl tragen die Crawl-ID der View (`ViewHandle.crawlId`); eine Ablehnung des Kerns
// (`err-selection-other-crawl`) erscheint in der Fehlerzeile in allen vorhandenen Sprachen; erscheint eine View eines
// anderen Crawls (auch ohne Beginn in diesem Fenster und vor jedem Status), ist die Auswahl leer.
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { crawlBegin } from "../../ipc/begin/crawlBegin";
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
import { runAction } from "../../ui/actions";
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
const rowsB: Row[] = [row(1, "https://b.invalid/1"), row(2, "https://b.invalid/2", 404), row(3, "https://b.invalid/3"), row(4, "https://b.invalid/4", 404)];
const OTHER = { en: "The selected rows belong to a crawl that is no longer open. Nothing was changed.",
  de: "Die markierten Zeilen gehören zu einem Crawl, der nicht mehr geöffnet ist. Es wurde nichts geändert." };

let fx: ReturnType<typeof tableFixture>;
let detach: () => void;
let released: Set<string>;
/** The crawl the core names for views opened from now on. */
let viewCrawl: string | undefined;
let seq = 10;
const status = (crawlId: string, phase: CrawlStatus["phase"] = "completed"): CrawlStatus => ({ ...(fixtureTick.crawl as CrawlStatus), crawlId, phase });
const tick = (crawl: CrawlStatus | null, views: Tick["views"] = []): Tick =>
  ({ seq: seq++, at: fixtureTick.at, crawl, analysis: null, views, countsVersion: seq, issuesVersion: 1, sidebarVersion: 1, ops: [], notices: [] });
const settle = () => act(async () => { await new Promise(resolve => setTimeout(resolve, 30)); });

beforeEach(async () => {
  fx = tableFixture(); released = new Set(); viewCrawl = "crawl-a";
  for (const name of ["view.rows", "view.locate", "view.close"]) {
    const handler = fx.core.handlers.get(name)!;
    fx.core.handlers.set(name, (args, ctx) => {
      if (released.has((args as ViewCloseArgs).viewId)) throw new IpcError(localError("NotFound", "err-view-not-found"));
      return handler(args, ctx);
    });
  }
  const open = fx.core.handlers.get("view.open")!;
  fx.core.handlers.set("view.open", async (args, ctx) => {
    const handle = await open(args, ctx) as Record<string, unknown>;
    return viewCrawl ? { ...handle, crawlId: viewCrawl } : handle;
  });
  setClient(createClient(fx.core.transport()));
  crawlBegin.reset();
  useTabs.setState({ catalog: fixtureTabs, order: fixtureTabs.map(t => t.id), active: "Internal", hidden: [], queries: {}, loaded: true, focusId: undefined, focusRequest: 0, error: null });
  useColumns.setState({ metadata: {}, byTab: {} });
  useSession.setState({ seq: -1, views: {}, ops: {}, crawl: status("crawl-a"), analysis: null });
  capture.grid = undefined;
  detach = attachSession(eventBus);
  await setLanguage("en");
});
afterEach(async () => { detach(); await flushUiState(); cleanup(); setClient(null); crawlBegin.reset(); await setLanguage("en"); vi.restoreAllMocks(); });

async function mounted() {
  render(<UpperTable />);
  await waitFor(() => expect(capture.grid?.source.rowKey(2)).toBe(3));
}
const pick = (...ids: number[]) => act(() => capture.grid!.onSelectionChange(
  ids.length === 1 ? SelectionModel.empty().select(ids[0]!) : SelectionModel.empty().select(ids[0]!).range(ids, ids[ids.length - 1]!)));
const ids = () => { const value = capture.grid!.selection.value; return value.kind === "ids" ? value.ids : "all"; };
async function expectEmpty() {
  expect(ids()).toEqual([]);
  expect(useTableSelection.getState().selection.value).toEqual({ kind: "ids", ids: [] });
  await waitFor(() => expect(useDetailSelection.getState().selection.urlIds).toEqual([]));
}
/** The core replaced the content (released the old view); the window notices on its next request, without any status. */
async function coreSwitches(rows: Row[]) {
  const old = capture.grid!.source;
  released.add(old.viewId); fx.replaceRows(rows);
  act(() => eventBus.emit({ kind: "tick", tick: tick(null, [{ viewId: old.viewId, version: old.version + 50, total: rows.length }]) }));
  await waitFor(() => expect(capture.grid!.source.viewId).not.toBe(old.viewId));
  await waitFor(() => expect(capture.grid!.source.total).toBe(rows.length));
}
const callsOf = (cmd: string) => fx.core.calls.filter(c => c.cmd === cmd).map(c => c.args);

// spec: UI-018
// spec: UI-114
it("recrawl and remove send the crawl id of the view the selection comes from", async () => {
  await mounted();
  expect(capture.grid!.source.crawlId).toBe("crawl-a");
  pick(2, 3);
  act(() => { runAction("crawl.recrawl"); });
  await waitFor(() => expect(callsOf("crawl.recrawl")).toHaveLength(1));
  expect(callsOf("crawl.recrawl")[0]).toEqual({ selection: { kind: "ids", ids: [2, 3] }, crawlId: "crawl-a" });
  // Remove by keyboard always asks first.
  act(() => eventBus.emit({ kind: "tick", tick: { ...tick(null), ops: [{ opId: "crawl.recrawl-1", state: "done", result: { count: 2 } }] as never } }));
  await settle();
  act(() => { runAction("table.remove"); });
  const confirm = await screen.findByRole("button", { name: "Remove" });
  act(() => confirm.click());
  await waitFor(() => expect(callsOf("crawl.remove")).toHaveLength(1));
  expect(callsOf("crawl.remove")[0]).toMatchObject({ crawlId: "crawl-a" });
});

// spec: UI-018
// spec: UI-114
// spec: UI-121
it("the core's refusal appears in the error line, in every available language", async () => {
  await mounted();
  fx.core.handlers.set("crawl.recrawl", () => { throw new IpcError(localError("NotFound", "err-selection-other-crawl")); });
  pick(2);
  act(() => { runAction("crawl.recrawl"); });
  await waitFor(() => expect(screen.getByText(OTHER.en)).toBeTruthy());
  await act(async () => { await setLanguage("de"); });
  await waitFor(() => expect(screen.getByText(OTHER.de)).toBeTruthy());
  // Every language file of the error messages has the key.
  const files = import.meta.glob<string>("/src/i18n/errors/i18n/*.ftl", { query: "?raw", import: "default", eager: true });
  expect(Object.keys(files).length).toBeGreaterThanOrEqual(2);
  for (const [file, text] of Object.entries(files)) expect(text, file).toMatch(/^err-selection-other-crawl = \S/m);
});

// spec: UI-018
// spec: UI-142
it("content replaced from outside: a view of another crawl empties the selection before any status arrives", async () => {
  await mounted();
  pick(2, 3);
  viewCrawl = "crawl-b";
  await coreSwitches(rowsB);
  expect(useSession.getState().crawl?.crawlId).toBe("crawl-a");
  await expectEmpty();
  expect(fx.core.calls.filter(c => c.cmd === "crawl.recrawl" || c.cmd === "crawl.remove")).toEqual([]);
});

// spec: UI-018
// spec: UI-142
it("a silent replacement by a view of the same crawl keeps the selection", async () => {
  await mounted();
  pick(2, 3);
  await coreSwitches([row(1, "https://fixture.invalid/a"), row(2, "https://fixture.invalid/b", 404), row(3, "https://fixture.invalid/c")]);
  await settle();
  expect(ids()).toEqual([2, 3]);
  await waitFor(() => expect(useDetailSelection.getState().selection.urlIds).toEqual([2, 3]));
});

// spec: UI-018
// spec: UI-012
it("a filter change that lands on a view of another crawl starts without selection", async () => {
  await mounted();
  pick(2);
  viewCrawl = "crawl-b"; fx.replaceRows(rowsB);
  const before = capture.grid!.source.viewId;
  act(() => useTabs.getState().select("Internal", "Client Error (4xx)"));
  await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(before));
  await settle();
  // URL 2 exists in the new view, but it is URL 2 of crawl B.
  await expectEmpty();
});

// spec: UI-018
it("the status of another crawl arriving afterwards keeps the selection empty", async () => {
  await mounted();
  pick(1);
  viewCrawl = "crawl-b";
  await coreSwitches(rowsB);
  act(() => eventBus.emit({ kind: "tick", tick: tick(status("crawl-b")) }));
  await settle();
  await expectEmpty();
});

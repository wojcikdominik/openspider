// fix-sel (1.0.2, UI-018): Ein kurzer Crawl meldet seinen Status dem Fenster erst mit Crawl-Ende und automatischer Analyse.
// Die Tabelle zeigte seine Zeilen schon vorher; eine dort gewählte Zeile ging verloren, als die Crawl-ID ankam: Die Tabelle
// hielt die erste ID für einen neuen Crawl, leerte die Auswahl und öffnete die View als neue Ansicht. Kommt die erste
// Crawl-ID zu einer View, die schon auf dem Schirm steht, setzt die neue View die alte fort (Auswahl, Fokuszeile, Lage).
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { eventBus } from "../../ipc/events";
import type { CrawlStatus } from "../../ipc/gen/CrawlStatus";
import type { Tick } from "../../ipc/gen/Tick";
import { useTabs } from "../../state/tabs";
import { useColumns } from "../../state/columns";
import { attachSession, useSession } from "../../state/session";
import { SelectionModel } from "../../state/selection";
import { setLanguage } from "../../i18n";
import { flushUiState } from "../../state/uistate";
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
const completed = (crawlId: string): CrawlStatus => ({ ...(fixtureTick.crawl as CrawlStatus), crawlId, phase: "completed", crawled: 3 });
const tick = (seq: number, crawl: CrawlStatus): Tick => ({ seq, at: fixtureTick.at, crawl, views: [], countsVersion: seq, issuesVersion: 1, sidebarVersion: 1, ops: [], notices: [] });
const opens = () => fx.core.calls.filter(c => c.cmd === "view.open").length;

describe("first crawl id for a view on screen", () => {
  beforeEach(async () => {
    fx = tableFixture();
    setClient(createClient(fx.core.transport()));
    useTabs.setState({ catalog: fixtureTabs, order: fixtureTabs.map(t => t.id), active: "Internal", hidden: [], queries: {}, loaded: true, focusId: undefined, focusRequest: 0, error: null });
    useColumns.setState({ metadata: {}, byTab: {} });
    useSession.setState({ seq: -1, views: {}, ops: {}, crawl: null, analysis: null });
    capture.grid = undefined;
    detach = attachSession(eventBus);
    await setLanguage("en");
  });
  afterEach(async () => { detach(); await flushUiState(); cleanup(); setClient(null); vi.restoreAllMocks(); });

  // spec: UI-018
  it("decides by the crawl known before and the view on screen", () => {
    expect(continuesCrawl(undefined, "crawl-a", true)).toBe(true);
    expect(continuesCrawl(undefined, "crawl-a", false)).toBe(false);
    expect(continuesCrawl("crawl-a", "crawl-b", true)).toBe(false);
    expect(continuesCrawl("", "crawl-b", true)).toBe(false);
    expect(continuesCrawl("crawl-a", undefined, true)).toBe(false);
  });

  // spec: UI-018
  // spec: UI-142
  // spec: UI-030
  it("keeps selection, focus row and detail selection when the crawl id arrives after the selection", async () => {
    render(<UpperTable />);
    await waitFor(() => expect(capture.grid?.source.rowKey(2)).toBe(3));
    const scrollTo = vi.fn();
    act(() => capture.grid!.controller?.({ scrollTo, bounds: () => undefined, visible: () => ({ x: 0, y: 0, width: 1, height: 3 }) }));
    act(() => capture.grid!.onSelectionChange(SelectionModel.empty().select(2)));
    const first = capture.grid!.source;
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
    // A continued view keeps the position: no jump to the focus row.
    expect(scrollTo).not.toHaveBeenCalled();
  });

  // spec: UI-018
  it("still empties the selection for a crawl that replaces a known one", async () => {
    useSession.setState({ crawl: completed("crawl-a") });
    render(<UpperTable />);
    await waitFor(() => expect(capture.grid?.source.rowKey(2)).toBe(3));
    act(() => capture.grid!.onSelectionChange(SelectionModel.empty().select(2)));
    const first = capture.grid!.source;
    act(() => eventBus.emit({ kind: "tick", tick: tick(11, completed("crawl-b")) }));
    await waitFor(() => expect(capture.grid?.source.viewId).not.toBe(first.viewId));
    expect(capture.grid!.source.continues(first)).toBe(false);
    expect(capture.grid!.selection.value).toEqual({ kind: "ids", ids: [] });
    expect(useDetailSelection.getState().selection.urlIds).toEqual([]);
  });
});

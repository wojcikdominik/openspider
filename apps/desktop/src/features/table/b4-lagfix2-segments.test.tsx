// B4-LAGFIX2: Beim Start eines neuen Crawls lädt die Segmentliste neu. Das öffnete die obere Tabelle ein zweites Mal
// (Fuß und Chip wieder leer; fiel es in eine laufende View-Erstellung, blieb im Kern eine zweite View zurück). Die
// Segmente eines anderen Crawls laden jetzt ohne Revisionssprung; nach einer Neuzuordnung öffnet die Tabelle weiter neu.
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { eventBus } from "../../ipc/events";
import type { CrawlStatus } from "../../ipc/gen/CrawlStatus";
import type { Tick } from "../../ipc/gen/Tick";
import { useTabs } from "../../state/tabs";
import { useColumns } from "../../state/columns";
import { attachSession, useSession } from "../../state/session";
import { setLanguage } from "../../i18n";
import { flushUiState } from "../../state/uistate";
import fixtureTick from "../../ipc/__fixtures__/Tick.json";
import { UpperTable } from "./UpperTable";
import { fixtureTabs, tableFixture } from "./fixtures/core";

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

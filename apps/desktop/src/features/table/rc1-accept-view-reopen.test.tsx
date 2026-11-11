// Unabhängige Abnahme rc.1, UI-142 Nachtrag 11.11.2026 (Vitest, jsdom).
// acceptance: independent
// Erwartungen allein aus der Spec (UI-142, Nachtrag „Fehlerbehebung rc.1“):
// - Gibt der Kern eine View frei, die die Oberfläche noch anzeigt (neuer Crawl, Leeren, Freigabe nach 60 s), oder meldet
//   er sie dauerhaft als veraltet, öffnet die Oberfläche dieselbe Ansicht (Tab, Filter, Suche, Sortierung) von selbst neu.
// - Dabei erscheint keine Fehlermeldung.
// - Die bisher sichtbaren Zeilen bleiben stehen, bis die neuen geladen sind; leere Zeilen sind nie zu sehen.
// - Das Schließen einer bereits freigegebenen View ist kein Fehler.
// Schnittstelle zum Kern (öffentlicher IPC-Vertrag): eine freigegebene View beantwortet `view.*` mit `NotFound` /
// `err-view-not-found`, eine veraltete mit `Stale` / `err-view-stale`. Die Spec nennt keine Frist für „dauerhaft veraltet“;
// hier wird bis 10 s gewartet.
// Das Canvas-Raster wird durch eine schlanke Attrappe ersetzt, die sich wie das echte Raster an die Datenquelle hängt
// (Fehler → `onError`, Nachladen des sichtbaren Bereichs bei Quellen-/Versionswechsel) und bei jedem Zeichnen festhält,
// ob sichtbare Zeilen leer (noch nicht geladen) sind.
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { DataGridProps } from "../../grid/DataGrid";
import { createClient, setClient } from "../../ipc/client";
import { IpcError, localError } from "../../ipc/errors";
import { eventBus } from "../../ipc/events";
import type { Tick } from "../../ipc/gen/Tick";
import type { ViewSpec } from "../../ipc/gen/ViewSpec";
import type { MockHandler } from "../../ipc/transport/mock";
import { useTabs, type TabQuery } from "../../state/tabs";
import { useColumns } from "../../state/columns";
import { useSession } from "../../state/session";
import { setLanguage } from "../../i18n";
import { ViewDataSource } from "../../data/ViewDataSource";
import { flushUiState } from "../../state/uistate";
import { UpperTable } from "./UpperTable";
import { fixtureTabs, tableFixture } from "./fixtures/core";

const capture = vi.hoisted(() => ({
  grid: undefined as DataGridProps | undefined,
  recording: false,
  /** Je Zeichnen: View und Zahl sichtbarer Zeilen ohne Daten. */
  renders: [] as { viewId: string; empty: number }[],
  unmounts: 0,
}));
vi.mock("../../grid/DataGrid", async original => {
  const lib = await original<typeof import("../../grid/DataGrid")>();
  const React = await import("react");
  const { LOADING } = await import("../../data/ViewDataSource");
  function FakeGrid(props: DataGridProps) {
    const [, repaint] = React.useState(0);
    capture.grid = props;
    const source = props.source;
    React.useEffect(() => {
      const off = source.subscribe(() => repaint(n => n + 1));
      source.onError = error => props.onError?.(error);
      return () => { off(); source.onError = undefined; };
    }, [source]);
    React.useEffect(() => { if (source.total > 0) void source.ensureRange(0, Math.min(source.total, 30) - 1); }, [source, source.version]);
    React.useEffect(() => () => { if (capture.recording) capture.unmounts++; }, []);
    let empty = 0;
    for (let row = 0; row < Math.min(source.total, 30); row++) if (source.getCell(0, row) === LOADING) empty++;
    if (capture.recording) capture.renders.push({ viewId: source.viewId, empty });
    return <div data-testid="grid" data-view={source.viewId} />;
  }
  return { ...lib, DataGrid: FakeGrid };
});

let fx: ReturnType<typeof tableFixture>;
/** Views, die der Kern freigegeben hat: jede `view.*`-Anfrage dafür antwortet `NotFound`. */
let released: Set<string>;
/** Views, die der Kern dauerhaft als veraltet meldet. */
let outdated: Set<string>;
const gone = () => new IpcError(localError("NotFound", "err-view-not-found"));
const stale = () => new IpcError(localError("Stale", "err-view-stale"));

function coreAware() {
  for (const cmd of ["view.rows", "view.locate", "view.copy", "view.close", "view.range"]) {
    const original = fx.core.handlers.get(cmd);
    fx.core.handlers.set(cmd, (args, ctx) => {
      const id = (args as { viewId?: string }).viewId ?? "";
      if (released.has(id)) throw gone();
      if (outdated.has(id) && cmd !== "view.close") throw stale();
      if (!original) throw new IpcError(localError("Unsupported", "err-unknown-command"));
      return original(args, ctx);
    });
  }
}
function wrap(cmd: string, fn: (args: unknown, next: () => unknown) => unknown) {
  const original = fx.core.handlers.get(cmd)!;
  fx.core.handlers.set(cmd, ((args, ctx) => fn(args, () => original(args, ctx))) as MockHandler);
}

beforeEach(async () => {
  fx = tableFixture(); released = new Set(); outdated = new Set(); coreAware();
  setClient(createClient(fx.core.transport()));
  useTabs.setState({ catalog: fixtureTabs, order: fixtureTabs.map(t => t.id), active: "Internal", hidden: [], queries: {}, loaded: true, focusId: undefined, focusRequest: 0, error: null });
  useColumns.setState({ metadata: {}, byTab: {} });
  useSession.setState({ ops: {}, views: {}, crawl: null });
  capture.grid = undefined; capture.recording = false; capture.renders = []; capture.unmounts = 0;
  await setLanguage("en");
});
afterEach(async () => { await flushUiState(); cleanup(); setClient(null); vi.restoreAllMocks(); });

const tick = (seq: number, views: Tick["views"] = []): Tick => ({ seq, at: "2026-11-11T10:00:00Z", views, countsVersion: seq, issuesVersion: seq, sidebarVersion: seq, ops: [], notices: [] });
const opens = () => fx.core.calls.filter(c => c.cmd === "view.open").map(c => (c.args as { spec: ViewSpec }).spec);
const shownView = () => capture.grid?.source.viewId;
const QUERY: TabQuery = { filter: "Client Error (4xx)", search: "fixture.invalid", sort: [{ column: "Word Count", dir: "desc" }] };

/** Tabelle mit Filter, Suche und Sortierung öffnen; ab dann wird jedes Zeichnen aufgezeichnet. */
async function mounted(query: TabQuery = QUERY) {
  useTabs.setState({ queries: { Internal: query } });
  render(<UpperTable />);
  await screen.findByTestId("grid");
  await waitFor(() => expect(capture.grid?.source.rowKey(0)).toBeDefined());
  capture.recording = true;
  return { viewId: shownView()!, spec: opens().at(-1)!, rows: capture.grid!.source.total };
}
/** Keine Fehlermeldung, kein Verlust der Tabelle, nie leere sichtbare Zeilen. */
function expectSilentAndFull() {
  expect(screen.queryByRole("alert")).toBeNull();
  expect(document.body.textContent).not.toMatch(/no longer available|not available|err-view|outdated|Stale|NotFound/i);
  expect(capture.unmounts).toBe(0);
  expect(capture.renders.filter(r => r.empty > 0)).toEqual([]);
}
function deferred<T = unknown>() {
  let resolve!: (value: T) => void; let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

// spec: UI-142
it("Fixture u10-contract: freigegebene View wird mit Tab, Filter, Suche und Sortierung still neu geöffnet, alte Zeilen bleiben bis zum Nachladen", async () => {
  const before = await mounted();
  expect(before.rows).toBe(1);
  const gate = deferred<void>();
  wrap("view.open", async (_args, next) => { if (opens().length > 1) await gate.promise; return next(); });
  released.add(before.viewId);
  act(() => eventBus.emit({ kind: "tick", tick: tick(2, [{ viewId: before.viewId, version: 2, total: 1 }]) }));
  // Die Oberfläche öffnet von selbst neu …
  await waitFor(() => expect(opens().length).toBe(2));
  // … und zeigt bis dahin weiter die bisherigen Zeilen ohne Meldung.
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
  expect(screen.getByTestId("grid")).toBeInTheDocument();
  expect(shownView()).toBe(before.viewId);
  expect(capture.grid!.source.rowKey(0)).toBe(2);
  expectSilentAndFull();
  gate.resolve();
  await waitFor(() => expect(shownView()).not.toBe(before.viewId));
  await waitFor(() => expect(capture.grid!.source.rowKey(0)).toBe(2));
  expect(opens().at(-1)).toEqual(before.spec);
  expect(opens().at(-1)).toMatchObject({ kind: "tab", tab: "Internal", filter: QUERY.filter, sort: QUERY.sort });
  expect(JSON.stringify(opens().at(-1))).toContain(QUERY.search);
  expectSilentAndFull();
});

// spec: UI-142
it("Freigabe genau während des Nachladens einer neuen Version: neu öffnen, keine Meldung, keine leeren Zeilen", async () => {
  const before = await mounted({ filter: "All", search: "", sort: [] });
  expect(before.rows).toBe(3);
  const hanging: ReturnType<typeof deferred>[] = [];
  wrap("view.rows", (args, next) => {
    const a = args as { viewId: string; version: number };
    if (a.viewId === before.viewId && a.version === 2) { const d = deferred(); hanging.push(d); return d.promise; }
    return next();
  });
  act(() => eventBus.emit({ kind: "tick", tick: tick(2, [{ viewId: before.viewId, version: 2, total: 3 }]) }));
  await waitFor(() => expect(hanging.length).toBeGreaterThan(0));
  expect(shownView()).toBe(before.viewId);
  // Der Kern gibt die View frei, während die Zeilen der neuen Version unterwegs sind.
  released.add(before.viewId);
  for (const d of hanging) d.reject(gone());
  await waitFor(() => expect(shownView()).not.toBe(before.viewId));
  await waitFor(() => expect(capture.grid!.source.rowKey(2)).toBe(3));
  expect(opens()).toHaveLength(2);
  expect(opens()[1]).toEqual(before.spec);
  expectSilentAndFull();
});

// spec: UI-142
it("auch die Ersatz-View wird während des Nachladens freigegeben: erneut neu öffnen, nie leere Zeilen, keine Meldung", async () => {
  const before = await mounted({ filter: "All", search: "", sort: [] });
  let reopened = 0;
  wrap("view.open", (_args, next) => {
    const handle = next() as { viewId: string };
    // Die erste Ersatz-View verschwindet sofort wieder (z. B. Leeren direkt nach dem neuen Crawl).
    if (++reopened === 1 && opens().length > 1) released.add(handle.viewId);
    return handle;
  });
  // `reopened` zählt ab jetzt nur Neuöffnungen.
  reopened = 0;
  released.add(before.viewId);
  act(() => eventBus.emit({ kind: "tick", tick: tick(2, [{ viewId: before.viewId, version: 2, total: 3 }]) }));
  await waitFor(() => expect(opens().length).toBeGreaterThanOrEqual(3), { timeout: 5000 });
  await waitFor(() => expect(released.has(shownView()!)).toBe(false), { timeout: 5000 });
  await waitFor(() => expect(capture.grid!.source.rowKey(2)).toBe(3));
  for (const spec of opens().slice(1)) expect(spec).toEqual(before.spec);
  expectSilentAndFull();
});

// spec: UI-142
it("dauerhaft als veraltet gemeldete View wird von selbst neu geöffnet, ohne Meldung und ohne leere Zeilen", async () => {
  const before = await mounted();
  outdated.add(before.viewId);
  act(() => eventBus.emit({ kind: "tick", tick: tick(2, [{ viewId: before.viewId, version: 2, total: 1 }]) }));
  // Bis zur Neuöffnung bleibt die bisherige Zeile stehen.
  expect(capture.grid!.source.rowKey(0)).toBe(2);
  await waitFor(() => expect(opens().length).toBe(2), { timeout: 10_000 });
  await waitFor(() => expect(shownView()).not.toBe(before.viewId), { timeout: 10_000 });
  await waitFor(() => expect(capture.grid!.source.rowKey(0)).toBe(2));
  expect(opens()[1]).toEqual(before.spec);
  expectSilentAndFull();
});

// spec: UI-142
it("neuer Crawl: freigegebene View wird mit derselben Ansicht neu geöffnet, alte Zeilen bleiben bis zum Nachladen, keine Meldung", async () => {
  useSession.setState({ crawl: { crawlId: "crawl-old", phase: "completed" } as never });
  const before = await mounted();
  const gate = deferred<void>();
  wrap("view.open", async (_args, next) => { if (opens().length > 1) await gate.promise; return next(); });
  released.add(before.viewId);
  act(() => useSession.setState({ crawl: { crawlId: "crawl-new", phase: "running" } as never }));
  await waitFor(() => expect(opens().length).toBe(2));
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
  // Bis die neue View geladen ist, bleibt die Tabelle mit den bisherigen Zeilen stehen.
  expect(screen.queryByTestId("grid")).toBeInTheDocument();
  expect(capture.grid!.source.rowKey(0)).toBe(2);
  gate.resolve();
  await waitFor(() => expect(shownView()).not.toBe(before.viewId));
  await waitFor(() => expect(capture.grid!.source.rowKey(0)).toBe(2));
  expect(opens()[1]).toEqual(before.spec);
  expectSilentAndFull();
});

// spec: UI-142
it("Schließen einer bereits freigegebenen View (Tabwechsel) ist kein Fehler", async () => {
  const before = await mounted({ filter: "All", search: "", sort: [] });
  released.add(before.viewId);
  act(() => useTabs.getState().select("External"));
  await waitFor(() => expect(shownView()).not.toBe(before.viewId));
  await waitFor(() => expect(fx.core.calls.some(c => c.cmd === "view.close" && (c.args as { viewId: string }).viewId === before.viewId)).toBe(true));
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 50)); });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(document.body.textContent).not.toMatch(/no longer available|err-view/i);
});

// spec: UI-142
it("Schließen einer freigegebenen View über die Datenquelle wirft keinen Fehler", async () => {
  const before = await mounted({ filter: "All", search: "", sort: [] });
  const handle = { viewId: "released-view", version: 1, total: 3, state: "ready", columns: capture.grid!.source.columns } as ConstructorParameters<typeof ViewDataSource>[0];
  released.add("released-view");
  const source = new ViewDataSource(handle);
  await expect(source.dispose()).resolves.toBeUndefined();
  expect(fx.core.calls.some(c => c.cmd === "view.close" && (c.args as { viewId: string }).viewId === "released-view")).toBe(true);
  expect(shownView()).toBe(before.viewId);
});

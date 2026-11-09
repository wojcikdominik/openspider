// Unabhängige Abnahme B4-CMPUI (Komponentenebene): Vergleichsmodus der Desktop-App.
// Erwartungen stammen allein aus docs/spec (OUT-140 bis OUT-151, OS-Festlegungen vom 09.11.2026, UI-007, UI-013, UI-121, CRAWL-127).
// Der Produktcode wird nur zum Aufrufen benutzt. Die echte Canvas-Geometrie prüft e2e/acceptance/b4-cmpui.spec.ts.
// acceptance: independent
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createClient, setClient } from "../../ipc/client";
import { createEventBus } from "../../ipc/events";
import { setLanguage } from "../../i18n";
import { attachSession, useSession } from "../../state/session";
import { useColumns } from "../../state/columns";
import { Header } from "../header/Header";
import { Statusbar } from "../statusbar/Statusbar";
import { setupHeader } from "../header/fixtures/setup";
import { ComparisonNumbers } from "./ComparisonNumbers";
import { ComparisonTable } from "./ComparisonTable";
import { comparisonCsv } from "./export";
import { comparisonFixture, session, snapshot } from "./fixtures/out-compare/core";
import { useComparison, type Snapshot } from "./model";
import type { JsonValue } from "../../ipc/gen/serde_json/JsonValue";

const ordinaryColumns = [
  { key: "Address", kind: { kind: "url" }, defaultVisible: true, defaultWidth: 320, align: "left", sortable: true, searchable: [], extension: false },
  { key: "Status Code", kind: { kind: "int" }, defaultVisible: true, defaultWidth: 90, align: "right", sortable: true, searchable: [], extension: false },
  { key: "Word Count", kind: { kind: "int" }, defaultVisible: true, defaultWidth: 100, align: "right", sortable: true, searchable: [], extension: false },
];
const STATUSES = ["Added", "New", "Removed", "Missing", "Unchanged"];
const GERMAN_STATUS: Record<string, string> = { Added: "Hinzugefügt", New: "Neu", Removed: "Entfernt", Missing: "Fehlend", Unchanged: "Unverändert" };

let fixture: ReturnType<typeof comparisonFixture>;
beforeEach(() => {
  fixture = comparisonFixture();
  setClient(createClient(fixture.core.transport()));
  useComparison.setState({ session, snapshot, revision: 0, query: { tab: "Internal", filter: "All", side: "Current" }, selected: null, selectedPartner: null });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(1800);
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1800);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(360);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(360);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, width: 1800, height: 360, top: 0, left: 0, right: 1800, bottom: 360, toJSON() {} });
});
afterEach(async () => {
  cleanup();
  vi.restoreAllMocks();
  setClient(null);
  useComparison.setState({ session: null, snapshot: null, error: null });
  useColumns.setState({ metadata: {}, byTab: {} });
  await setLanguage("en");
});

/** Der Kern liefert die gewöhnlichen Spalten des Tabs und hängt die Vergleichsspalten an (OUT-142, OUT-151). */
function core(rows: string[][], mapping = false) {
  const comparison = mapping ? ["Comparison Status", "Previous Address"] : ["Comparison Status"];
  fixture.core.handlers.set("catalog.columns", () => ordinaryColumns);
  fixture.core.handlers.set("config.get", () => ({ schemaVersion: 2, generator: "fixture", config: {} }));
  fixture.core.handlers.set("view.rows", (args) => {
    const a = args as { version: number; offset: number };
    const columns = ["Address", "Status Code", "Word Count", ...comparison];
    const cells = rows.map((r, i) => [`https://site.test/${i}`, 200, [5, 120, 1234][i % 3], r[0], ...(mapping ? [`https://staging.site.test/${i}`] : [])]);
    return { viewId: "fixture-comparison", version: a.version, offset: a.offset, columns, cells, rowKeys: cells.map((_, i) => i + 1), partnerKeys: cells.map((_, i) => 101 + i), total: cells.length, side: "Current", filtersEnabled: true };
  });
}

describe("Deutsche Oberfläche des Vergleichs", () => {
  // spec: OUT-142
  // spec: UI-121
  // spec: UI-007
  it("Übersicht: deutscher Titel, deutsche Spaltenköpfe, Zahlen mit Tausenderpunkt (4.438)", async () => {
    await setLanguage("de");
    const big: Snapshot = { overview: [{ tab: "Internal", filter: "HTML", label: "HTML", current: 4438, previous: 4000, change: 438, classes: [0, 500, 12, 50] }], issues: [], notes: [] };
    useComparison.setState({ snapshot: big });
    render(<ComparisonNumbers />);
    const table = screen.getByRole("table");
    expect(table.getAttribute("aria-label") ?? "").toMatch(/übersicht/i);
    expect(table.getAttribute("aria-label")).not.toMatch(/overview/i);
    const heads = within(table).getAllByRole("columnheader").map(c => c.textContent?.trim());
    for (const name of ["Hinzugefügt", "Neu", "Entfernt", "Fehlend"]) expect(heads, `Spaltenkopf ${name}`).toContain(name);
    for (const english of ["Added", "New", "Removed", "Missing"]) expect(heads).not.toContain(english);
    expect(within(table).getByRole("button", { name: "4.438" })).toBeVisible();
    expect(table.textContent).not.toMatch(/4438|4,438/);
    expect(within(table).getByText("438")).toBeVisible();
  });

  // spec: OUT-142
  // spec: UI-007
  it("Übersicht auf Englisch: 4,438 mit Komma als Tausendertrenner", async () => {
    await setLanguage("en");
    useComparison.setState({ snapshot: { overview: [{ tab: "Internal", filter: "HTML", label: "HTML", current: 4438, previous: 4000, change: 438, classes: [0, 500, 12, 50] }], issues: [], notes: [] } });
    render(<ComparisonNumbers />);
    expect(within(screen.getByRole("table")).getByRole("button", { name: "4,438" })).toBeVisible();
  });

  // spec: OUT-142
  // spec: UI-121
  it("Auswahlwerte (Vergleichsstatus-Filter) erscheinen deutsch, die Werte selbst bleiben kanonisch", async () => {
    await setLanguage("de");
    core([["Unchanged"]]);
    render(<ComparisonTable />);
    await screen.findByRole("grid");
    const select = screen.getAllByRole("combobox").find(c => within(c).queryByRole("option", { name: "Hinzugefügt" }))!;
    expect(select, "Auswahl mit den vier Kategorien").toBeTruthy();
    const options = within(select).getAllByRole("option") as HTMLOptionElement[];
    const labels = options.map(o => o.textContent);
    for (const name of ["Hinzugefügt", "Neu", "Entfernt", "Fehlend"]) expect(labels).toContain(name);
    for (const english of ["Added", "New", "Removed", "Missing"]) expect(labels).not.toContain(english);
    expect(options.map(o => o.value)).toEqual(expect.arrayContaining(["Added", "New", "Removed", "Missing"]));
  });
});

describe("Spalten der Vergleichstabelle", () => {
  // spec: OUT-142
  // spec: OUT-151
  it("Vergleichsstatus steht direkt nach Address, Vorherige Adresse direkt danach (Deutsch)", async () => {
    await setLanguage("de");
    core([["Unchanged"]], true);
    render(<ComparisonTable />);
    const grid = await screen.findByRole("grid");
    await waitFor(() => expect(within(grid).getAllByRole("columnheader").length).toBeGreaterThanOrEqual(4));
    const heads = within(grid).getAllByRole("columnheader").map(c => c.textContent!.trim());
    const status = heads.indexOf("Vergleichsstatus");
    const previous = heads.indexOf("Vorherige Adresse");
    expect(status, `Köpfe: ${heads.join(" | ")}`).toBeGreaterThan(0);
    // Die Spalte davor ist die Adressspalte (die feste Zeilennummer „Row“ kann davor stehen).
    expect(heads[status - 1]).not.toMatch(/^(Statuscode|Status Code|Wortanzahl|Word Count)$/);
    expect(heads[status - 1]).toMatch(/^Adresse$|^Address$/);
    expect(previous).toBe(status + 1);
    expect(heads.indexOf("Statuscode")).toBeGreaterThan(previous);
  });

  // spec: OUT-142
  it("Englische Oberfläche: kanonische Titel Comparison Status und Previous Address", async () => {
    await setLanguage("en");
    core([["Unchanged"]], true);
    render(<ComparisonTable />);
    const grid = await screen.findByRole("grid");
    await waitFor(() => expect(within(grid).getAllByRole("columnheader").length).toBeGreaterThanOrEqual(4));
    const heads = within(grid).getAllByRole("columnheader").map(c => c.textContent!.trim());
    expect(heads.indexOf("Comparison Status")).toBe(heads.indexOf("Address") + 1);
    expect(heads.indexOf("Previous Address")).toBe(heads.indexOf("Comparison Status") + 1);
  });

  // spec: OUT-142
  // spec: OUT-151
  it("Statuswerte in den Zellen deutsch, Export bleibt englisch kanonisch", async () => {
    await setLanguage("de");
    core(STATUSES.map(s => [s]));
    render(<ComparisonTable />);
    const grid = await screen.findByRole("grid");
    await waitFor(() => expect(within(grid).getAllByRole("gridcell").length).toBeGreaterThan(5));
    const cells = within(grid).getAllByRole("gridcell").map(c => c.textContent!.trim());
    for (const status of STATUSES) {
      expect(cells, `Zelle ${GERMAN_STATUS[status]}`).toContain(GERMAN_STATUS[status]);
      expect(cells, `keine englische Zelle ${status}`).not.toContain(status);
    }
    const csv = await comparisonCsv(useComparison.getState().query, ["Address", "Status Code", "Word Count"]);
    const [head, ...lines] = csv.trim().split("\r\n");
    expect(head).toContain("Comparison Status");
    expect(head).not.toContain("Vergleichsstatus");
    expect(lines.map(l => l.split(",").at(-1))).toEqual(STATUSES);
    for (const german of Object.values(GERMAN_STATUS)) expect(csv).not.toContain(german);
  });
});

describe("Modusanzeige", () => {
  // spec: UI-013
  // spec: OUT-140
  it("Kopf und Statusleiste zeigen im Vergleich „Vergleich“ statt „Spider“", async () => {
    setupHeader();
    await setLanguage("de");
    useComparison.setState({ session });
    render(<><Header /><Statusbar /></>);
    const mode = screen.getByRole("combobox", { name: "Modus" });
    expect(mode).toHaveDisplayValue("Vergleich");
    expect(screen.getByText("Modus: Vergleich")).toBeVisible();
    expect(screen.queryByText("Modus: Spider")).toBeNull();
  });

  // spec: UI-013
  // spec: OUT-140
  it("nach dem Schließen des Vergleichs gilt wieder der gewählte Modus", async () => {
    setupHeader();
    await setLanguage("de");
    useComparison.setState({ session });
    render(<><Header /><Statusbar /></>);
    expect(screen.getByText("Modus: Vergleich")).toBeVisible();
    act(() => useComparison.setState({ session: null }));
    await waitFor(() => expect(screen.getByText("Modus: Spider")).toBeVisible());
    expect(screen.getByRole("combobox", { name: "Modus" })).toHaveDisplayValue("Spider");
  });
});

describe("Fehlende Crawl-Datei beim Öffnen", () => {
  const failure = (args: Record<string, JsonValue>) => {
    setupHeader();
    const bus = createEventBus();
    const off = attachSession(bus);
    useSession.getState().clearNotices();
    render(<Statusbar />);
    act(() => bus.emit({ kind: "notice", notice: { level: "error", code: "crawl-open-failed", args } }));
    return off;
  };
  /** Der sichtbare Satz ohne aufklappbare technische Details. */
  const sentence = async (marker: string) => {
    const alert = await screen.findByRole("alert");
    // Der Zähler der Statusleiste ist gedrosselt (4/s): auf die Meldung dieses Tests warten, nicht auf Reste eines früheren.
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain(marker));
    const copy = (screen.getByRole("alert") ?? alert).cloneNode(true) as HTMLElement;
    copy.querySelectorAll("details").forEach(d => d.remove());
    return copy.textContent!.trim();
  };

  // spec: CRAWL-127
  // spec: UI-124
  it("meldet NotFound mit Dateiname als deutscher Satz ohne „.:“ und ohne rohen Code", async () => {
    await setLanguage("de");
    const off = failure({ code: "NotFound", messageKey: "err-file-not-found", args: { path: "/home/u/crawls/alt.ospider" }, detail: "No such file or directory (os error 2) eins" });
    const text = await sentence("alt.ospider");
    expect(text).toContain("alt.ospider");
    expect(text).not.toMatch(/\.:/);
    expect(text).not.toMatch(/fehlgeschlagen/i);
    expect(text).not.toMatch(/NotFound|not found|os error|No such file|crawl-open-failed|\{|\}/i);
    expect(text).toMatch(/Datei/);
    off();
  });

  // spec: CRAWL-127
  it("ohne Meldungsschlüssel des Kerns bleibt der Satz deutsch, ohne „.:“", async () => {
    await setLanguage("de");
    const off = failure({ code: "NotFound", detail: "No such file or directory (os error 2) zwei" });
    const text = await sentence("zwei");
    expect(text.length).toBeGreaterThan(5);
    expect(text).not.toMatch(/\.:/);
    expect(text).not.toMatch(/os error|No such file|NotFound/i);
    off();
  });
});

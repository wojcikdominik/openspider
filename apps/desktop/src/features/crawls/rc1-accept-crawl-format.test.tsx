// Unabhängige Abnahme rc.1, Darstellung der Crawl-Verwaltung (Vitest, jsdom).
// acceptance: independent
// Erwartungen allein aus der Spec: OUT-228 „Darstellung (Nachtrag 11.11.2026, Fehlerbehebung rc.1)“ mit UI-121.
// - `Started`: Datum und Uhrzeit in der Oberflächensprache (nicht ISO).
// - `Duration`: lesbar in Stunden/Minuten/Sekunden statt Millisekunden.
// - `URLs`: Tausendertrennzeichen der Oberflächensprache.
// - `Size on Disk`: KB/MB/GB mit lokalisiertem Dezimaltrennzeichen.
// - Spaltenköpfe nennen keine Einheit; sortiert wird nach den Rohwerten.
// - Der Löschdialog nennt den freiwerdenden Speicher im selben Format.
// Offen in der Spec (nur tolerant geprüft): Basis 1000/1024, Schreibweise der Dauer (h:mm:ss oder „1 h 2 min 3 s“),
// Datumsstil (numerisch oder Monatsname), Einheit unter 1 KB.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setLanguage } from "../../i18n";
import { createClient, setClient } from "../../ipc/client";
import type { CrawlListing } from "../../ipc/gen/CrawlListing";
import { useSession } from "../../state/session";
import { CrawlsHost } from "./CrawlsHost";
import { fixtureRows, managementFixture } from "./fixtures/core";
import { useCrawlsDialog } from "./model";

let fixture: ReturnType<typeof managementFixture>;
beforeEach(async () => {
  fixture = managementFixture();
  setClient(createClient(fixture.core.transport("main")));
  localStorage.clear();
  useSession.setState({ crawl: null, ops: {} });
  useCrawlsDialog.setState({ open: false });
  await setLanguage("en");
});
afterEach(() => { cleanup(); setClient(null); useCrawlsDialog.setState({ open: false }); vi.restoreAllMocks(); });

type Lang = "de" | "en";
const selectLabel = (lang: Lang, name: string) => (lang === "de" ? `Auswählen: ${name}` : `Select: ${name}`);
const HEADERS = {
  en: { started: "Started", duration: "Duration", urls: "URLs", size: /^Size/ },
  de: { started: "Gestartet", duration: "Dauer", urls: "URLs", size: /^Größe/ },
} as const;

let seq = 0;
function row(name: string, values: { startedAt?: string; durationMs?: number | null; urls?: number; fileBytes?: number }): CrawlListing {
  const id = `0199f000-0000-7000-8000-${String(++seq).padStart(12, "0")}`;
  return {
    crawl: { crawlId: id, name, file: `/fixture/${name}.ospider`, phase: "completed", mode: "spider", startedAt: values.startedAt ?? "2026-10-01T12:00:00Z",
      urls: values.urls ?? 1, fileBytes: values.fileBytes ?? 1000, schemaVersion: 3, owner: { kind: "app", live: false, controllable: true } },
    project: null, locked: false, startUrl: `https://${name.toLowerCase()}.test/`, durationMs: values.durationMs === undefined ? 1000 : values.durationMs, canResume: false, endReason: null,
  };
}

async function mount(lang: Lang, rows?: CrawlListing[]) {
  if (rows) fixture.setRows(rows);
  await setLanguage(lang);
  useCrawlsDialog.setState({ open: true });
  render(<CrawlsHost desktop={fixture.desktop} />);
  const first = rows?.[0]?.crawl.name ?? fixtureRows[0]!.crawl.name;
  await screen.findByRole("checkbox", { name: selectLabel(lang, first) });
}
const table = () => screen.getAllByRole("table").find(t => t.querySelector("thead"))!;
function headerCell(label: string | RegExp): HTMLTableCellElement {
  const cells = [...table().querySelectorAll<HTMLTableCellElement>("thead th")];
  const match = cells.find(th => typeof label === "string" ? th.textContent?.trim() === label : label.test(th.textContent?.trim() ?? ""));
  if (!match) throw new Error(`Spaltenkopf ${String(label)} fehlt: ${cells.map(c => c.textContent).join(" | ")}`);
  return match;
}
function cellText(lang: Lang, name: string, header: string | RegExp): string {
  const index = headerCell(header).cellIndex;
  const tr = screen.getByRole("checkbox", { name: selectLabel(lang, name) }).closest("tr")!;
  return (tr.cells[index]?.textContent ?? "").replace(/ | /g, " ").trim();
}
const numbers = (text: string) => (text.match(/\d+/g) ?? []).map(Number);
function rowNames(): string[] {
  return [...table().querySelectorAll("tbody tr")].map(tr => {
    const box = tr.querySelector<HTMLInputElement>("input[type=checkbox]");
    return (box?.getAttribute("aria-label") ?? "").replace(/^(Select|Auswählen): /, "");
  });
}
const pad = (n: number) => String(n).padStart(2, "0");

// spec: OUT-228
// spec: UI-121
it("Fixture management.json (en): Startzeit, Dauer, URLs und Größe erscheinen formatiert statt als Rohwert", async () => {
  await mount("en");
  const alpha = fixtureRows.find(r => r.crawl.name === "Alpha")!;
  const started = cellText("en", "Alpha", HEADERS.en.started);
  expect(started).not.toBe(alpha.crawl.startedAt);
  expect(started).not.toMatch(/\d{4}-\d{2}-\d{2}T/);
  expect(started).toMatch(/2026/);
  const duration = cellText("en", "Alpha", HEADERS.en.duration);
  expect(duration).not.toMatch(new RegExp(`\\b${alpha.durationMs}\\b`));
  expect(numbers(duration).at(-1)).toBe(alpha.durationMs! / 1000);
  expect(cellText("en", "Alpha", HEADERS.en.urls)).toBe(String(alpha.crawl.urls));
  expect(cellText("en", "Alpha", HEADERS.en.size)).toMatch(/^1(\.\d+)? ?KB$/);
});

// spec: OUT-228
// spec: UI-121
it("Spaltenköpfe nennen keine Einheit (de und en)", async () => {
  const unit = /\(|\[|\b(ms|s|sec|min|h|bytes?|B|KB|MB|GB|KiB|MiB|GiB)\b/i;
  await mount("en");
  for (const header of Object.values(HEADERS.en)) expect(headerCell(header).textContent!.trim()).not.toMatch(unit);
  cleanup();
  await mount("de");
  for (const header of Object.values(HEADERS.de)) expect(headerCell(header).textContent!.trim()).not.toMatch(unit);
});

// spec: OUT-228
// spec: UI-121
it("Startzeit als Datum und Uhrzeit in der Oberflächensprache (de und en unterscheiden sich, kein ISO)", async () => {
  const iso = "2026-01-10T12:05:09Z";
  const local = new Date(iso);
  const rows = [row("Zeit", { startedAt: iso })];
  await mount("de", rows);
  const de = cellText("de", "Zeit", HEADERS.de.started);
  expect(de).not.toMatch(/\d{4}-\d{2}-\d{2}T|Z$/);
  expect(de).toMatch(/2026/);
  expect(de).toMatch(new RegExp(`\\b0?${local.getDate()}\\.\\s?(0?${local.getMonth() + 1}\\.|Jan)`));
  expect(de).toMatch(new RegExp(`\\b0?${local.getHours()}:05`));
  expect(de).not.toMatch(/AM|PM/);
  cleanup();
  await mount("en", rows);
  const en = cellText("en", "Zeit", HEADERS.en.started);
  expect(en).not.toMatch(/\d{4}-\d{2}-\d{2}T|Z$/);
  expect(en).toMatch(/2026/);
  expect(en).toMatch(new RegExp(`(Jan(uary)? 0?${local.getDate()}\\b|\\b0?${local.getDate()} Jan|\\b0?1/0?${local.getDate()}/2026)`));
  const h12 = local.getHours() % 12 || 12;
  expect(en).toMatch(new RegExp(`\\b(0?${h12}|${pad(local.getHours())}):05`));
  expect(en).not.toBe(de);
});

// spec: OUT-228
it("Dauer lesbar in Stunden/Minuten/Sekunden: über eine Stunde, unter einer Minute, über einen Tag", async () => {
  const rows = [
    row("Lang", { durationMs: 3_723_000 }),          // 1 h 2 min 3 s
    row("Kurz", { durationMs: 45_000 }),             // 45 s
    row("Genau", { durationMs: 7_200_000 }),         // 2 h
    row("Tag", { durationMs: 90_061_000 }),          // 25 h 1 min 1 s
  ];
  await mount("de", rows);
  const lang = cellText("de", "Lang", HEADERS.de.duration);
  expect(lang).not.toMatch(/3723|3\.723/);
  expect(numbers(lang)).toEqual([1, 2, 3]);
  const kurz = cellText("de", "Kurz", HEADERS.de.duration);
  expect(kurz).not.toMatch(/45\.?000/);
  expect(numbers(kurz).filter(n => n !== 0)).toEqual([45]);
  const genau = cellText("de", "Genau", HEADERS.de.duration);
  expect(numbers(genau)[0]).toBe(2);
  expect(numbers(genau).slice(1).every(n => n === 0)).toBe(true);
  const tag = cellText("de", "Tag", HEADERS.de.duration);
  expect(numbers(tag)).toEqual([25, 1, 1]);
});

// spec: OUT-228
// spec: UI-121
it("URLs mit Tausendertrennzeichen der Oberflächensprache (de Punkt, en Komma), auch 0 und sehr große Werte", async () => {
  const rows = [row("Viele", { urls: 1_234_567 }), row("Null", { urls: 0 }), row("Riesig", { urls: 12_345_678_901 }), row("Drei", { urls: 999 })];
  await mount("de", rows);
  expect(cellText("de", "Viele", HEADERS.de.urls)).toBe("1.234.567");
  expect(cellText("de", "Null", HEADERS.de.urls)).toBe("0");
  expect(cellText("de", "Riesig", HEADERS.de.urls)).toBe("12.345.678.901");
  expect(cellText("de", "Drei", HEADERS.de.urls)).toBe("999");
  cleanup();
  await mount("en", rows);
  expect(cellText("en", "Viele", HEADERS.en.urls)).toBe("1,234,567");
  expect(cellText("en", "Riesig", HEADERS.en.urls)).toBe("12,345,678,901");
});

// spec: OUT-228
// spec: UI-121
it("Größe in KB/MB/GB mit lokalisiertem Dezimaltrennzeichen, auch Größe 0 und sehr große Dateien", async () => {
  const rows = [
    row("Mega", { fileBytes: 1_500_000 }), row("Kilo", { fileBytes: 1_536 }), row("Giga", { fileBytes: 12_345_000_000 }),
    row("Gross", { fileBytes: 250_000_000_000 }), row("Leer", { fileBytes: 0 }),
  ];
  await mount("de", rows);
  expect(cellText("de", "Mega", HEADERS.de.size)).toMatch(/^1,\d+ ?MB$/);
  expect(cellText("de", "Kilo", HEADERS.de.size)).toMatch(/^1,5\d* ?KB$/);
  expect(cellText("de", "Giga", HEADERS.de.size)).toMatch(/^1[12],\d+ ?GB$/);
  expect(cellText("de", "Gross", HEADERS.de.size)).toMatch(/^\d{3}(,\d+)? ?GB$/);
  const leer = cellText("de", "Leer", HEADERS.de.size);
  expect(leer).toMatch(/^0(,0+)? ?(B|Byte|KB)$/);
  cleanup();
  await mount("en", rows);
  expect(cellText("en", "Mega", HEADERS.en.size)).toMatch(/^1\.\d+ ?MB$/);
  expect(cellText("en", "Kilo", HEADERS.en.size)).toMatch(/^1\.5\d* ?KB$/);
  expect(cellText("en", "Giga", HEADERS.en.size)).toMatch(/^1[12]\.\d+ ?GB$/);
  expect(cellText("en", "Gross", HEADERS.en.size)).toMatch(/^\d{3}(\.\d+)? ?GB$/);
});

/** Klickt den Spaltenkopf, bis die gewünschte Richtung anliegt, und liefert die Zeilenreihenfolge. */
async function sortBy(header: string | RegExp, dir: "ascending" | "descending"): Promise<string[]> {
  for (let i = 0; i < 3 && headerCell(header).getAttribute("aria-sort") !== dir; i++) {
    fireEvent.click(within(headerCell(header)).getByRole("button"));
    await act(async () => { await Promise.resolve(); });
  }
  expect(headerCell(header).getAttribute("aria-sort")).toBe(dir);
  return rowNames();
}

// spec: OUT-228
it("Sortierung nach Rohwerten: Größe, Dauer, URLs und Startzeit folgen den Zahlen, nicht dem angezeigten Text", async () => {
  const rows = [
    row("A", { fileBytes: 2_000_000, durationMs: 3_900_000, urls: 20_000, startedAt: "2026-01-10T12:00:00Z" }),
    row("B", { fileBytes: 999, durationMs: 9_000, urls: 999, startedAt: "2026-02-09T12:00:00Z" }),
    row("C", { fileBytes: 10_000, durationMs: 540_000, urls: 1_000, startedAt: "2025-12-31T12:00:00Z" }),
    row("D", { fileBytes: 1_500, durationMs: 10_000, urls: 5, startedAt: "2026-11-01T12:00:00Z" }),
  ];
  await mount("de", rows);
  expect(await sortBy(HEADERS.de.size, "ascending")).toEqual(["B", "D", "C", "A"]);
  expect(await sortBy(HEADERS.de.size, "descending")).toEqual(["A", "C", "D", "B"]);
  expect(await sortBy(HEADERS.de.duration, "ascending")).toEqual(["B", "D", "C", "A"]);
  expect(await sortBy(HEADERS.de.urls, "ascending")).toEqual(["D", "B", "C", "A"]);
  expect(await sortBy(HEADERS.de.urls, "descending")).toEqual(["A", "C", "B", "D"]);
  expect(await sortBy(HEADERS.de.started, "ascending")).toEqual(["C", "A", "B", "D"]);
  expect(await sortBy(HEADERS.de.started, "descending")).toEqual(["D", "B", "A", "C"]);
});

// spec: OUT-228
// spec: UI-121
it("Löschdialog nennt den freiwerdenden Speicher im selben Format wie die Tabelle (de, eine und zwei Zeilen)", async () => {
  const rows = [row("Eins", { fileBytes: 1_500_000 }), row("Zwei", { fileBytes: 1_000_000 }), row("Drei", { fileBytes: 7_654_321_000 })];
  await mount("de", rows);
  const single = cellText("de", "Drei", HEADERS.de.size);
  fireEvent.click(screen.getByRole("checkbox", { name: selectLabel("de", "Drei") }));
  fireEvent.click(screen.getAllByRole("button", { name: "Löschen" })[0]!);
  let dialog = await screen.findByRole("dialog", { name: "Löschen" });
  let text = dialog.textContent!.replace(/ | /g, " ");
  expect(text).toContain(single);
  expect(text).not.toMatch(/7654321000|7\.654\.321\.000/);
  fireEvent.click(within(dialog).getByRole("button", { name: /Abbrechen/ }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Löschen" })).toBeNull());

  fireEvent.click(screen.getByRole("checkbox", { name: selectLabel("de", "Drei") }));
  fireEvent.click(screen.getByRole("checkbox", { name: selectLabel("de", "Eins") }));
  fireEvent.click(screen.getByRole("checkbox", { name: selectLabel("de", "Zwei") }));
  fireEvent.click(screen.getAllByRole("button", { name: "Löschen" })[0]!);
  dialog = await screen.findByRole("dialog", { name: "Löschen" });
  text = dialog.textContent!.replace(/ | /g, " ");
  // 2 500 000 Byte: Dezimalkomma, Einheit MB, kein Rohwert.
  expect(text).toMatch(/\b2,\d+ ?MB\b/);
  expect(text).not.toMatch(/2500000|2\.500\.000/);
});

// spec: OUT-228
// spec: UI-121
it("Löschdialog auf Englisch: Dezimalpunkt und dieselbe Schreibweise wie die Tabellenzelle", async () => {
  const rows = [row("One", { fileBytes: 1_234_567 })];
  await mount("en", rows);
  const single = cellText("en", "One", HEADERS.en.size);
  expect(single).toMatch(/^1\.\d+ ?MB$/);
  fireEvent.click(screen.getByRole("checkbox", { name: selectLabel("en", "One") }));
  fireEvent.click(screen.getAllByRole("button", { name: "Delete" })[0]!);
  const dialog = await screen.findByRole("dialog", { name: "Delete" });
  expect(dialog.textContent!.replace(/ | /g, " ")).toContain(single);
});

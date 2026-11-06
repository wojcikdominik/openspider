// Hilfen der unabhängigen Abnahme B4-LIVEDETAIL (Live-Crawl: Detailfenster, Tabellen, Seitenleiste).
// acceptance: independent
// Erwartungen stammen aus docs/spec/05-ui.md (UI-018, UI-030 bis UI-033, UI-120/121, UI-140 bis UI-145) und den Zahlen der Fixture
// fx-ui-basic (tests/fixtures/ui/fx-ui-basic: 25 interne + 2 externe URLs; die Startseite verlinkt page-1 … page-20, /redirect,
// /missing, /noindex und zwei externe Adressen; jede page-N verlinkt nur zurück auf „/“). Der Code wird nur zum Aufrufen benutzt.
import type { Locator, Page } from "@playwright/test";
import { expect, type Alpha } from "../alpha/fixture";

/** Hohes Fenster: obere Tabelle und Detailfenster zeigen mehr Zeilen auf einmal. */
export const TALL = { width: 1280, height: 1500 };

/** Zahl aus einer Anzeige wie „1.234“, „1,234“ oder „27“ (nur kleine Werte, Tausendertrenner werden verworfen). */
export const toNumber = (text: string | null | undefined): number => Number((text ?? "").replace(/[^\d]/g, ""));

export const upperGrid = (page: Page): Locator => page.locator("#data-panel .os-data-grid");
export const detailRegion = (page: Page): Locator => page.getByRole("region", { name: /^(Details|Detailfenster)$/ });

export async function crawledCount(page: Page): Promise<number> {
  return page.evaluate(async () => {
    const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
    return (useSession.getState().crawl?.crawled ?? 0) as number;
  });
}

export async function phase(page: Page): Promise<string | undefined> {
  return page.evaluate(async () => {
    const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
    return useSession.getState().crawl?.phase as string | undefined;
  });
}

export async function waitCrawled(page: Page, atLeast: number, timeout = 30_000): Promise<void> {
  await expect.poll(() => crawledCount(page), { timeout, intervals: [100, 200] }).toBeGreaterThanOrEqual(atLeast);
}

export async function pauseCrawl(page: Page): Promise<void> {
  await page.getByRole("button", { name: /^(Pause|Pausieren)$/ }).click();
  await expect(page.getByTestId("crawl-phase")).toHaveText(/^(Paused|Pausiert)$/);
}

export async function resumeCrawl(page: Page): Promise<void> {
  await page.getByRole("button", { name: /^(Resume|Fortsetzen)$/ }).click();
  // Bei hoher Rate ist der Crawl danach schon fertig: nur die Pause muss verlassen sein.
  await expect(page.getByTestId("crawl-phase")).not.toHaveText(/^(Paused|Pausiert)$/);
}

/** „Showing: 8 of 8 · 1 selected“ / „Angezeigt: 8 von 8“ */
export function parseFooter(text: string): { shown: number; total: number | null; selected: number } {
  const m = /:\s*([\d.,]+)\s+\S+\s+([\d.,]+|–)/.exec(text);
  const sel = /(\d[\d.,]*)\s+(?:selected|ausgewählt|markiert)/i.exec(text);
  return { shown: m ? toNumber(m[1]) : -1, total: m && m[2] !== "–" ? toNumber(m[2]) : null, selected: sel ? toNumber(sel[1]) : 0 };
}

export async function footer(page: Page): Promise<{ shown: number; total: number | null; selected: number }> {
  return parseFooter(await page.locator("#data-panel .u10-counter").innerText());
}

/** Zahl hinter dem aktiven Filter-Chip („All (27)“); `null` ohne Zahl. */
export async function activeChipCount(page: Page): Promise<number | null> {
  const text = await page.locator("#data-panel .u34-chips button[aria-pressed=true]").first().innerText();
  const m = /\(([\d.,]+)\)/.exec(text);
  return m ? toNumber(m[1]) : null;
}

export interface GridRead {
  name: string | null;
  /** `aria-rowcount` (UI-126: Gesamtzahl der Zeilen einschließlich Kopfzeile). */
  rowcount: number;
  headers: string[];
  sort: (string | null)[];
  rows: { index: number; selected: boolean; cells: string[] }[];
}

/** Liest ein ARIA-Grid (sichtbarer Ausschnitt) über die Zugänglichkeitsschicht der Tabelle. */
export async function readGrid(scope: Locator): Promise<GridRead | null> {
  return scope.evaluate((root) => {
    const grid = root.matches("[role=grid]") ? root : root.querySelector("[role=grid]");
    if (!grid) return null;
    const rows = [...grid.querySelectorAll<HTMLElement>("[role=row]")];
    const head = rows.find((r) => r.getAttribute("aria-rowindex") === "1");
    return {
      name: grid.getAttribute("aria-label"),
      rowcount: Number(grid.getAttribute("aria-rowcount")),
      headers: [...(head?.querySelectorAll("[role=columnheader]") ?? [])].map((c) => c.textContent ?? ""),
      sort: [...(head?.querySelectorAll("[role=columnheader]") ?? [])].map((c) => c.getAttribute("aria-sort")),
      rows: rows
        .filter((r) => r !== head)
        .map((r) => ({
          index: Number(r.getAttribute("aria-rowindex")),
          selected: r.getAttribute("aria-selected") === "true",
          cells: [...r.querySelectorAll("[role=gridcell]")].map((c) => c.textContent ?? ""),
        })),
    };
  });
}

/** Die sichtbare Tabelle eines Detail-Tabs (zurückgehaltene, verborgene Tabs werden ausgelassen). */
export const detailGridLocator = (page: Page): Locator => page.locator("section.os-detail .os-detail-grid .os-data-grid:visible").first();

export async function readDetail(page: Page): Promise<GridRead | null> {
  if (!(await detailGridLocator(page).count())) return null;
  return readGrid(detailGridLocator(page));
}

export async function readUpper(page: Page): Promise<GridRead | null> {
  return readGrid(upperGrid(page));
}

/** Zeilen der Summary-Tabelle der Overview in der Seitenleiste als `Beschriftung → Zahl` (erste Tabelle, englische Beschriftung). */
export async function overviewSummary(page: Page): Promise<Record<string, number>> {
  return page.evaluate(() => {
    const table = document.querySelector(".os-sidebar-overview table");
    const out: Record<string, number> = {};
    for (const row of table?.querySelectorAll("tbody tr") ?? []) {
      const label = row.querySelector("th")?.textContent?.trim() ?? "";
      const value = row.querySelector("td")?.textContent ?? "";
      if (label) out[label] = Number(value.replace(/[^\d]/g, ""));
    }
    return out;
  });
}

/** Alle Zeilen aller Tabellen der Overview: `Tabellenname/Zeile → Zahl` (zum Vergleich zweier Stände). */
export async function overviewFingerprint(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const tables = [...document.querySelectorAll(".os-sidebar-overview table")];
    tables.forEach((table, i) => {
      const caption = table.querySelector("caption")?.textContent?.trim() ?? table.getAttribute("aria-label") ?? `#${i}`;
      for (const row of table.querySelectorAll("tbody tr")) {
        const cells = [...row.querySelectorAll("th,td")].map((c) => (c.textContent ?? "").replace(/\s+/g, " ").trim());
        out.push(`${caption}/${cells.join(" | ")}`);
      }
    });
    return out;
  });
}

/** Fokuszeile und Markierung: Adressen der markierten Zeilen der oberen Tabelle (sichtbarer Ausschnitt). */
export async function selectedUpper(page: Page): Promise<string[]> {
  const grid = await readUpper(page);
  return (grid?.rows ?? []).filter((r) => r.selected).map((r) => r.cells[0] ?? "");
}

/**
 * Markiert eine Zeile der oberen Tabelle mit einem echten Mausklick (Zeile der Fokuszeile; das Detailfenster folgt, UI-012).
 * Die Zeile muss im sichtbaren Ausschnitt stehen; Zeilenhöhe 28 px (Standard „Kompakt“, UI-120), Kopf 32 px.
 */
export async function clickUpperRow(page: Page, address: string, modifiers: ("Shift" | "ControlOrMeta")[] = []): Promise<void> {
  let found = -1;
  await expect
    .poll(async () => {
      const grid = await readUpper(page);
      const row = grid?.rows.find((r) => r.cells[0] === address);
      const first = grid?.rows[0]?.index ?? 0;
      found = row ? row.index - first : -1;
      return found;
    }, { timeout: 15_000 })
    .toBeGreaterThanOrEqual(0);
  await upperGrid(page).locator("canvas").first().click({ position: { x: 160, y: 32 + 28 * found + 14 }, modifiers });
  if (!modifiers.length) await expect.poll(() => selectedUpper(page), { timeout: 10_000 }).toEqual([address]);
}

/** Bewegt die Fokuszeile der oberen Tabelle mit der Tastatur (UI-012: Pfeiltasten). */
export async function pressUpper(page: Page, key: string, times = 1): Promise<void> {
  await upperGrid(page).focus();
  for (let i = 0; i < times; i++) await page.keyboard.press(key);
}

export async function openDetailTab(page: Page, name: string): Promise<Locator> {
  const region = detailRegion(page);
  const tab = region.getByRole("tab", { name, exact: true });
  if (await tab.count()) await tab.click();
  else {
    await region.getByRole("button", { name: /^(More detail tabs|Weitere Detail-Tabs)$/ }).click();
    await page.getByRole("menuitem", { name, exact: true }).click();
  }
  return page.getByRole("tabpanel", { name, exact: true });
}

/** Öffnet den Seitenleisten-Tab `name` (Overview, Issues, …). */
export async function openSidebarTab(page: Page, name: string): Promise<void> {
  const list = page.getByRole("tablist", { name: /^(Crawl summary|Crawl-Zusammenfassung)$/ });
  const tab = list.getByRole("tab", { name, exact: true });
  if (await tab.count()) await tab.click();
  else {
    await page.getByRole("button", { name: /^(More sidebar tabs|Weitere Seitenleisten-Tabs)$/ }).click();
    await page.getByRole("menuitem", { name, exact: true }).click();
  }
}

export interface Sample {
  t: number;
  label?: string;
  phase: string | undefined;
  crawled: number;
  footer: string;
  chip: string;
  status: string;
  overview: string[];
  detail: { name: string | null; rowcount: number; headers: string[]; sort: (string | null)[]; rows: string[]; selected: number[] } | null;
  upperSelected: string[];
}

/**
 * Zeichnet im Seitenkontext alle `intervalMs` den sichtbaren Zustand auf (Session-Zähler, Tabellenfuß, Chip, Statusleiste,
 * Overview, Detail-Tabelle, markierte Zeilen) und zusätzlich bei jeder DOM-Änderung im Detailfenster. Die Zeitstempel kommen
 * aus derselben Uhr (`performance.now()`) wie die Marker, die mit `mark()` gesetzt werden.
 */
export async function installRecorder(page: Page, intervalMs = 100): Promise<void> {
  await page.evaluate(async (intervalMs) => {
    const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
    const w = window as unknown as { __ld?: { stop(): Sample[]; mark(label: string): number; samples: Sample[] } };
    w.__ld?.stop();
    const samples: Sample[] = [];
    const text = (el: Element | null | undefined) => ((el as HTMLElement | null)?.innerText ?? "").replace(/\s+/g, " ").trim();
    const snapshot = (label?: string): Sample => {
      const s = useSession.getState();
      const grid = [...document.querySelectorAll<HTMLElement>("section.os-detail .os-detail-grid .os-data-grid")].find((g) => !g.closest("[hidden]")) ?? null;
      let detail: Sample["detail"] = null;
      if (grid) {
        const rows = [...grid.querySelectorAll<HTMLElement>("[role=row]")];
        const head = rows.find((r) => r.getAttribute("aria-rowindex") === "1");
        const body = rows.filter((r) => r !== head);
        detail = {
          name: grid.getAttribute("aria-label"),
          rowcount: Number(grid.getAttribute("aria-rowcount")),
          headers: [...(head?.querySelectorAll("[role=columnheader]") ?? [])].map((c) => c.textContent ?? ""),
          sort: [...(head?.querySelectorAll("[role=columnheader]") ?? [])].map((c) => c.getAttribute("aria-sort")),
          rows: body.map((r) => [...r.querySelectorAll("[role=gridcell]")].map((c) => c.textContent ?? "").join("|")),
          selected: body.filter((r) => r.getAttribute("aria-selected") === "true").map((r) => Number(r.getAttribute("aria-rowindex"))),
        };
      }
      const upper = document.querySelector("#data-panel .os-data-grid");
      const upperSelected = [...(upper?.querySelectorAll<HTMLElement>("[role=row][aria-selected=true]") ?? [])].map((r) => r.querySelector("[role=gridcell]")?.textContent ?? "");
      const table = document.querySelector(".os-sidebar-overview table");
      return {
        t: performance.now(),
        ...(label ? { label } : {}),
        phase: s.crawl?.phase as string | undefined,
        crawled: (s.crawl?.crawled ?? 0) as number,
        footer: text(document.querySelector("#data-panel .u10-counter")),
        chip: text(document.querySelector("#data-panel .u34-chips button[aria-pressed=true]")),
        status: text(document.querySelector("[data-testid=crawl-status]")),
        overview: [...(table?.querySelectorAll("tbody tr") ?? [])].slice(0, 12).map((r) => text(r)),
        detail,
        upperSelected,
      };
    };
    const timer = setInterval(() => samples.push(snapshot()), intervalMs);
    const root = document.querySelector("section.os-detail");
    const observer = new MutationObserver(() => samples.push(snapshot()));
    if (root) observer.observe(root, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ["aria-rowcount", "hidden", "aria-selected"] });
    w.__ld = {
      samples,
      mark(label: string) { const t = performance.now(); samples.push({ ...snapshot(label), t }); return t; },
      stop() { clearInterval(timer); observer.disconnect(); return samples; },
    };
  }, intervalMs);
}

export async function mark(page: Page, label: string): Promise<number> {
  return page.evaluate((label) => (window as unknown as { __ld: { mark(l: string): number } }).__ld.mark(label), label);
}

export async function stopRecorder(page: Page): Promise<Sample[]> {
  return page.evaluate(() => (window as unknown as { __ld: { stop(): Sample[] } }).__ld.stop());
}

/** Spaltenüberschriften der oberen Tabelle (sichtbare Spalten, Anzeigereihenfolge). */
export async function upperHeaders(page: Page): Promise<string[]> {
  return (await readUpper(page))?.headers ?? [];
}

/** Startet einen gedrosselten Crawl der Fixture `fx-ui-basic` (1 Thread, `rate` URL/s) und wählt vorher den oberen Tab. */
export async function startSlow(alpha: Alpha, rate: number, tab = "Response Codes"): Promise<void> {
  await alpha.open();
  await alpha.slowCrawl(rate);
  if (tab !== "Internal") await alpha.selectTab(tab);
  await alpha.startFromHeader();
}

/** Wartet, bis die Tabelle des oberen Bereichs keine Platzhalterzellen („…“) mehr zeigt, und liefert die Zeilen. */
export async function settledUpper(page: Page): Promise<GridRead> {
  let last: GridRead | null = null;
  await expect
    .poll(async () => {
      last = await readUpper(page);
      return !!last && last.rows.length > 0 && last.rows.every((r) => r.cells.every((c) => c !== "…"));
    }, { timeout: 10_000 })
    .toBe(true);
  return last!;
}

/** Wert der Core-Antwort für eine Detailansicht (unabhängige Quelle: derselbe IPC-Befehl, den die Oberfläche benutzt). */
export async function coreDetailTotal(page: Page, kind: "inlinks" | "outlinks", urlId: number): Promise<number> {
  return page.evaluate(async ({ kind, urlId }) => {
    const { view } = await import(/* @vite-ignore */ String("/src/ipc/view.ts"));
    for (let attempt = 0; attempt < 10; attempt++) {
      const handle = await view.open({ spec: { kind, urlIds: [urlId], filter: { id: "All" }, search: null, sort: [] }, columns: ["From", "To"] });
      try {
        const rows = await view.rows({ viewId: handle.viewId, version: handle.version, offset: 0, limit: 1, columns: ["From", "To"] });
        return (rows.total ?? handle.total ?? 0) as number;
      } catch (error) {
        if (!String((error as Error).message).includes("stale")) throw error;
      } finally {
        await view.close({ viewId: handle.viewId });
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error("view.rows blieb veraltet");
  }, { kind, urlId });
}

/** URL-ID einer Adresse in der Ansicht „Response Codes“ (alle Zeilen), wie die Oberfläche sie führt. */
export async function urlIdOf(page: Page, address: string): Promise<number> {
  return page.evaluate(async (address) => {
    const { view } = await import(/* @vite-ignore */ String("/src/ipc/view.ts"));
    for (let attempt = 0; attempt < 15; attempt++) {
      const handle = await view.open({ spec: { kind: "tab", tab: "Response Codes", filter: "All", search: null, sort: [] }, columns: ["Address"] });
      try {
        const rows = await view.rows({ viewId: handle.viewId, version: handle.version, offset: 0, limit: 200, columns: ["Address"] });
        const index = rows.cells.findIndex((c: unknown[]) => c[0] === address);
        if (index >= 0) return rows.rowKeys[index] as number;
        throw new Error(`${address} nicht in der Ansicht`);
      } catch (error) {
        if (!String((error as Error).message).includes("stale")) throw error;
      } finally {
        await view.close({ viewId: handle.viewId });
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error("view.rows blieb veraltet");
  }, address);
}

/** Zahl hinter der Beschriftung in einer Stichprobenzeile der Overview („Total URLs Crawled 7 25.9 %“). */
export function summaryValue(rows: string[], label: RegExp): number {
  const row = rows.find((r) => label.test(r));
  if (!row) return -1;
  const rest = row.replace(label, "").trim();
  return toNumber(rest.split(" ")[0]);
}

/** Gesamtzahl einer Tabellenansicht laut Kern (`view.open`), die unabhängige Quelle für „exakt“. */
export async function coreTotal(page: Page, tab: string, filter: string): Promise<number> {
  return page.evaluate(async ({ tab, filter }) => {
    const { view } = await import(/* @vite-ignore */ String("/src/ipc/view.ts"));
    for (let attempt = 0; attempt < 15; attempt++) {
      const handle = await view.open({ spec: { kind: "tab", tab, filter, search: null, sort: [] }, columns: ["Address"] });
      try {
        const rows = await view.rows({ viewId: handle.viewId, version: handle.version, offset: 0, limit: 1, columns: ["Address"] });
        return (rows.total ?? handle.total ?? 0) as number;
      } catch (error) {
        if (!String((error as Error).message).includes("stale")) throw error;
      } finally {
        await view.close({ viewId: handle.viewId });
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error("view.rows blieb veraltet");
  }, { tab, filter });
}

/** Wie `alpha.slowCrawl` für eine beliebige Seite: Standardprofil mit einem Thread und `rate` URL/s. */
export async function slowProfile(page: Page, rate: number): Promise<void> {
  await page.evaluate(async (rate) => {
    const { config } = await import(/* @vite-ignore */ String("/src/ipc/config.ts"));
    const { profile } = await import(/* @vite-ignore */ String("/src/ipc/profile.ts"));
    const dto = await config.get();
    const root = dto.config as Record<string, Record<string, unknown>>;
    const next = { ...root, speed: { ...(root.speed ?? {}), threads: 1, max_urls_per_sec: rate } };
    await profile.save({ name: "ld-slow", overwrite: true, config: { ...dto, config: next } });
    await profile.setDefault({ name: "ld-slow" });
  }, rate);
}

/**
 * Sortiert eine Spalte einer Tabelle per Tastatur: Fokus auf die Kopfzeile, zur Spalte `col` (0-basiert, sichtbare Spalten) gehen,
 * Enter (UI-010: erster Klick aufsteigend, zweiter absteigend, dritter hebt auf). Der Fokus wird jedes Mal neu hergestellt, weil
 * eine neue Sortierung die Tabelle neu aufbauen darf.
 */
export async function sortByKeyboard(page: Page, grid: Locator, col: number): Promise<void> {
  await grid.focus();
  const position = async () => {
    const id = (await grid.getAttribute("aria-activedescendant")) ?? "";
    const m = /-(\d+)-(-?\d+)$/.exec(id);
    return m ? { col: Number(m[1]), row: Number(m[2]) } : null;
  };
  for (let i = 0; i < 4 && (await position())?.row !== -1; i++) await page.keyboard.press("ArrowUp");
  expect((await position())?.row, "Fokus liegt in der Kopfzeile").toBe(-1);
  for (let i = 0; i < 20; i++) {
    const p = await position();
    if (!p || p.col === col) break;
    await page.keyboard.press(p.col < col ? "ArrowRight" : "ArrowLeft");
  }
  expect((await position())?.col, "Fokus liegt in der gewünschten Spalte").toBe(col);
  await page.keyboard.press("Enter");
}

/** Anzahl der sichtbaren Zeilen der oberen Tabelle, deren Spalte `header` einen Wert zeigt (wächst, während URLs gecrawlt werden). */
export async function filledCells(page: Page, header: string): Promise<number> {
  const grid = await readUpper(page);
  if (!grid) return 0;
  const col = grid.headers.map((h) => h.replace(/\s*[↑↓]\d?$/, "")).indexOf(header);
  if (col < 0) return -1;
  return grid.rows.filter((r) => (r.cells[col] ?? "") !== "" && r.cells[col] !== "…").length;
}

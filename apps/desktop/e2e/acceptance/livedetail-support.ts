// Hilfen der unabhängigen Abnahme B4-LIVEDETAIL (Live-Crawl: Detailfenster, Tabellen, Seitenleiste).
// acceptance: independent
// Erwartungen stammen aus docs/spec/05-ui.md (UI-018, UI-030 bis UI-033, UI-120/121, UI-140 bis UI-145) und den Zahlen der Fixture
// fx-ui-basic (tests/fixtures/ui/fx-ui-basic: 25 interne + 2 externe URLs; die Startseite verlinkt page-1 … page-20, /redirect,
// /missing, /noindex und zwei externe Adressen; jede page-N verlinkt nur zurück auf „/“). Der Code wird nur zum Aufrufen benutzt.
import type { Locator, Page } from "@playwright/test";
import { test as alphaTest, expect, type Alpha } from "../alpha/fixture";
import { startGate, type Gate } from "./livedetail-gate";

export { expect };
export type { Gate };

/** `test` mit dem Fixture `gate`: ein gesteuerter Fixture-Crawl (siehe livedetail-gate.ts), am Ende des Tests immer geöffnet und beendet. */
export const test = alphaTest.extend<{ gate: Gate }>({
  gate: async ({ alpha }, use) => {
    const gate = await startGate(alpha.site.base);
    await use(gate);
    await gate.stop();
  },
});

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

/**
 * Hält den Crawl an. Die Pause wartet auf die gerade laufende Anfrage; am Tor wartet immer genau eine. Sie wird nach dem Klick (und
 * einer kurzen Frist für den Befehl an den Kern) einmal freigegeben, damit der Crawl in den Zustand „Paused“ übergehen kann. Danach
 * kommt keine weitere Anfrage mehr durch, bis der Test sie freigibt.
 */
export async function pauseCrawl(page: Page, gate: Gate): Promise<void> {
  await page.getByRole("button", { name: /^(Pause|Pausieren)$/ }).click();
  const clicked = Date.now();
  await expect
    .poll(async () => {
      if (gate.waiting() > 0 && Date.now() - clicked > 300) gate.allow(1);
      return page.getByTestId("crawl-phase").innerText();
    }, { timeout: 20_000, intervals: [100] })
    .toMatch(/^(Paused|Pausiert)$/);
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
  // Ein einfacher Klick wird wiederholt, wenn sich das Layout zwischen Lesen und Klicken verschoben hat (z. B. eine Meldung über der
  // Tabelle nach dem Ende des Crawls) und deshalb eine andere Zeile getroffen wurde; am Ende muss genau die Zielzeile markiert sein.
  // Umschalt+Klicks werden nie wiederholt: ein verlorener Klick ist dort das Messergebnis.
  for (let attempt = 0; ; attempt++) {
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
    if (modifiers.length) return;
    try {
      await expect.poll(() => selectedUpper(page), { timeout: 5000 }).toEqual([address]);
      return;
    } catch (error) {
      if (attempt >= 3) throw error;
      test.info().annotations.push({ type: "click-retry", description: `Klick auf ${address} traf eine andere Zeile (Versuch ${attempt + 1}); Layout hatte sich verschoben` });
    }
  }
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
    const w = window as unknown as { __ld?: { stop(): Sample[]; mark(label: string): number; samples: Sample[]; beats: number[] } };
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
    const beats: number[] = [];
    const timer = setInterval(() => { beats.push(performance.now()); samples.push(snapshot()); }, intervalMs);
    const root = document.querySelector("section.os-detail");
    const observer = new MutationObserver(() => samples.push(snapshot()));
    if (root) observer.observe(root, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ["aria-rowcount", "hidden", "aria-selected"] });
    w.__ld = {
      samples,
      beats,
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

/** Profil für gesteuerte Crawls: ein Thread, Antwort-Timeout so hoch, dass wartende Anfragen am Tor nicht ablaufen (Crawler-Einstellung). */
export async function gatedProfile(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const { config } = await import(/* @vite-ignore */ String("/src/ipc/config.ts"));
    const { profile } = await import(/* @vite-ignore */ String("/src/ipc/profile.ts"));
    const dto = await config.get();
    const root = dto.config as Record<string, Record<string, unknown>>;
    const next = { ...root, speed: { ...(root.speed ?? {}), threads: 1 }, advanced: { ...(root.advanced ?? {}), response_timeout_s: 3600 } };
    await profile.save({ name: "ld-gated", overwrite: true, config: { ...dto, config: next } });
    await profile.setDefault({ name: "ld-gated" });
  });
}

/**
 * Startet den Crawl der Fixture `fx-ui-basic` über das Tor (1 Thread) und wählt vorher den oberen Tab. Das Tor ist zu Beginn
 * geschlossen: `gate.allowTo(n)` / `crawlTo` lassen Anfragen durch, `gate.open()` lässt den Crawl enden.
 */
export async function startGated(alpha: Alpha, gate: Gate, tab = "Response Codes"): Promise<void> {
  await alpha.open();
  await gatedProfile(alpha.page);
  if (tab !== "Internal") await alpha.selectTab(tab);
  await alpha.startFromHeader(gate.startUrl);
}

/** Lässt den Crawl bis zur `n`-ten URL laufen und wartet, bis die Oberfläche sie als gecrawlt führt; der Crawl bleibt am Tor stehen. */
export async function crawlTo(page: Page, gate: Gate, n: number): Promise<void> {
  gate.allowTo(n);
  await waitCrawled(page, n);
}

/** Lässt `n` weitere URLs zu und wartet auf sie. */
export async function crawlMore(page: Page, gate: Gate, n: number): Promise<void> {
  gate.allow(n);
  await waitCrawled(page, gate.budget());
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

/** Gespeicherte Sichtbarkeit einer Spalte im Layout einer Detailtabelle (UI-Zustandsdatei, `detailColumns`); `undefined`, wenn nichts gespeichert ist. */
export async function persistedDetailColumn(page: Page, layout: string, key: string): Promise<boolean | undefined> {
  return page.evaluate(async ({ layout, key }) => {
    const { uistate } = await import(/* @vite-ignore */ String("/src/ipc/uistate.ts"));
    const dto = await uistate.load();
    const root = (dto.state ?? {}) as Record<string, unknown>;
    const layouts = (root.detailColumns ?? {}) as Record<string, { key: string; visible: boolean }[]>;
    return layouts[layout]?.find((c) => c.key === key)?.visible;
  }, { layout, key });
}

/**
 * Maß für die Trägheit des Rechners: Faktor ≥ 1, um den der Seitenkontext hinter seinem Takt herhinkt (Median von `setTimeout(20)` über
 * eine Sekunde, geteilt durch 20 ms, höchstens 8). Auf einem ruhigen Rechner ist er 1. Absolute Wartegrenzen für Anzeige-Verzögerungen
 * (Nachlauf der Zähler, erste Seite nach einem Auswahlwechsel) werden mit ihm multipliziert, damit eine ausgelastete CI-Maschine
 * keine Verzögerung der Oberfläche vortäuscht; Zählergrenzen der Spec und alle inhaltlichen Prüfungen bleiben unverändert.
 */
export async function machineFactor(page: Page): Promise<number> {
  const factor = await page.evaluate(async () => {
    const gaps: number[] = [];
    let last = performance.now();
    const end = last + 1000;
    while (last < end) {
      await new Promise((r) => setTimeout(r, 20));
      const now = performance.now();
      gaps.push(now - last);
      last = now;
    }
    gaps.sort((a, b) => a - b);
    return gaps[Math.floor(gaps.length / 2)]! / 20;
  });
  return Math.min(8, Math.max(1, factor));
}

/** Anzahl verschiedener Werte der Overview-Zeile „Total URLs Crawled“ in den bisherigen Stichproben des Recorders. */
export async function recordedOverviewDistinct(page: Page): Promise<number> {
  return page.evaluate(() => {
    const samples = (window as unknown as { __ld?: { samples: { overview: string[] }[] } }).__ld?.samples ?? [];
    const values = new Set<number>();
    for (const s of samples) {
      const row = s.overview.find((r) => /^Total URLs Crawled/.test(r));
      if (row) values.add(Number(row.replace(/^Total URLs Crawled/, "").trim().split(" ")[0]!.replace(/[^\d]/g, "")));
    }
    return values.size;
  });
}

/** URL-IDs mehrerer Adressen (Ansicht „Response Codes“, alle Zeilen). */
export async function urlIdsOf(page: Page, addresses: string[]): Promise<number[]> {
  return page.evaluate(async (addresses) => {
    const { view } = await import(/* @vite-ignore */ String("/src/ipc/view.ts"));
    for (let attempt = 0; attempt < 15; attempt++) {
      const handle = await view.open({ spec: { kind: "tab", tab: "Response Codes", filter: "All", search: null, sort: [] }, columns: ["Address"] });
      try {
        const rows = await view.rows({ viewId: handle.viewId, version: handle.version, offset: 0, limit: 200, columns: ["Address"] });
        return addresses.map((a) => {
          const index = rows.cells.findIndex((c: unknown[]) => c[0] === a);
          if (index < 0) throw new Error(`${a} nicht in der Ansicht`);
          return rows.rowKeys[index] as number;
        });
      } catch (error) {
        if (!String((error as Error).message).includes("stale")) throw error;
      } finally {
        await view.close({ viewId: handle.viewId });
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error("view.rows blieb veraltet");
  }, addresses);
}

/**
 * Auslastung des Rechners während der Aufzeichnung: Median der Abstände der Takt-Schläge des Recorders geteilt durch den Soll-Abstand
 * (`raw`, ≥ 1). Dieses Maß steigt auch, wenn die Oberfläche selbst den Hauptthread belegt, also bei genau dem Rückschritt, den die Tests
 * finden sollen. Deshalb wird es auf `max(2 × machine, 4)` gekappt (`capped`; `machine` = `machineFactor` vor dem Lauf): ein Rückschritt
 * der Oberfläche kann die Verzögerungsgrenzen höchstens um diesen Faktor aufweichen, nicht beliebig. Beide Werte gehören ins Attachment.
 */
export async function recorderFactor(page: Page, intervalMs: number, machine: number): Promise<{ raw: number; capped: number; slow: number }> {
  const raw = await page.evaluate((intervalMs) => {
    const beats = (window as unknown as { __ld?: { beats: number[] } }).__ld?.beats ?? [];
    const gaps = beats.slice(1).map((b, i) => b - beats[i]!).sort((a, b) => a - b);
    return gaps.length ? gaps[Math.floor(gaps.length / 2)]! / intervalMs : 1;
  }, intervalMs);
  const capped = Math.min(Math.max(1, raw), Math.max(2 * machine, 4));
  return { raw, capped, slow: Math.max(machine, capped) };
}

/**
 * Rate (URLs je Sekunde), mit der ein Test den Crawl bis `urls` laufen lässt, damit das Beobachtungsfenster für eine Verzögerungsprüfung mit
 * Grenze `boundSec` auch beim größten zulässigen Faktor (`max(2 × machine, 4)`) mindestens fünf geprüfte Stichproben enthält:
 * Fenster ≥ `boundSec` × Faktor + 3 s. Auf einem ruhigen Rechner (Faktor 4) ergibt das 11 s für 2 s Grenze, auf einem langsamen entsprechend mehr.
 */
export function lagRate(machine: number, boundSec: number, urls: number): number {
  return Math.min(3, urls / (boundSec * Math.max(2 * machine, 4) + 3));
}

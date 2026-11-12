// Alpha-Kriterium 2 (docs/plan/M2.md 4.3): Haupttabelle zeigt alle URLs; Tabwechsel, Filter-Dropdown mit Zählern, Sortierung (Pfeil,
// Gleichstand F-3), einfache Suche, Zähler „N von M“; Live-Einsortierung ohne Springen der Auswahl. Echte Bridge, echter Kern.
import type { Page } from "@playwright/test";
import { expect, test, type Alpha } from "./fixture";

const TALL = { width: 1280, height: 1500 };

/** Mitte eines Spaltenkopfs in Canvas-Koordinaten, berechnet aus der Spaltenverwaltung der Oberfläche (feste Breiten). */
async function headerX(page: Page, tab: string, key: string): Promise<number> {
  const columns = await page.evaluate(async (tab) => {
    const path = "/src/state/columns.ts";
    const { useColumns } = await import(/* @vite-ignore */ path);
    return (useColumns.getState().byTab[tab] as { key: string; width: number; visible: boolean }[]).filter((c) => c.visible).map((c) => [c.key, c.width] as const);
  }, tab);
  let x = 0;
  for (const [name, width] of columns) {
    if (name === key) return x + width / 2;
    x += width;
  }
  throw new Error(`Spalte ${key} ist im Tab ${tab} nicht sichtbar`);
}

async function sortBy(alpha: Alpha, tab: string, key: string): Promise<void> {
  await alpha.page.locator(".os-data-grid").click({ position: { x: await headerX(alpha.page, tab, key), y: 12 } });
}

async function crawled(alpha: Alpha, tab: string, viewport?: { width: number; height: number }): Promise<void> {
  if (viewport) await alpha.page.setViewportSize(viewport);
  await alpha.open();
  await alpha.startFromHeader();
  await alpha.waitCompleted();
  await alpha.openTable(tab);
}

const addresses = (rows: { cells: string[] }[]) => rows.map((r) => r.cells[0]!);

// spec: UI-004
// spec: UI-007
test("the Internal tab lists all 25 internal URLs with the default columns after a crawl", async ({ alpha }) => {
  await alpha.open();
  await alpha.startFromHeader();
  await alpha.waitCompleted();
  await expect(alpha.page.getByRole("tab", { name: "Internal", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(alpha.table()).toContainText("Showing: 25 of 25");
  // UI-018 tail mode: a table that showed its last row while the crawl was still running follows new rows and ends
  // scrolled to the bottom (slow runners). A wheel gesture up returns to the first row before reading the window.
  const area = await alpha.table().locator("canvas").boundingBox();
  expect(area).not.toBeNull();
  await alpha.page.mouse.move(area!.x + area!.width / 2, area!.y + area!.height / 2);
  await alpha.page.mouse.wheel(0, -100_000);
  const rows = (await alpha.settledGrid()).rows;
  expect(rows.length).toBeGreaterThan(0);
  expect(addresses(rows)[0]).toBe(alpha.startUrl);
});

// spec: UI-004
test("tab switching shows each tab's table and keeps the last filter chosen per tab", async ({ alpha }) => {
  const { page } = alpha;
  await crawled(alpha, "External");
  await expect(alpha.table()).toContainText("Showing: 2 of 2");
  const external = addresses((await alpha.settledGrid()).rows);
  expect(external).toHaveLength(2);
  expect(external.every((a) => a.startsWith("http://localhost:"))).toBe(true);

  await alpha.openTable("Response Codes");
  await expect(alpha.table()).toContainText("Showing: 27 of 27");
  // Filter-Dropdown mit Zählern je Eintrag
  await alpha.table().getByRole("button", { name: /Filter/, exact: true }).click();
  const option = (label: RegExp) => page.getByRole("option", { name: label });
  await expect(option(/^Success \(2XX\)\s*25$/)).toBeVisible();
  await expect(option(/^Redirection \(3XX\)\s*1$/)).toBeVisible();
  await expect(option(/^Client Error \(4XX\)\s*1$/)).toBeVisible();
  await expect(option(/^Server Error \(5XX\)\s*0$/)).toBeVisible();
  await option(/^Client Error \(4XX\)\s*1$/).click();
  await expect(alpha.table()).toContainText("Showing: 1 of 27");
  const failing = (await alpha.settledGrid()).rows;
  expect(failing).toHaveLength(1);
  expect(failing[0]!.cells[0]).toBe(`${alpha.site.base}/missing`);
  expect(failing[0]!.cells[2]).toBe("404");

  // Tabwechsel und zurück: der zuletzt gewählte Filter bleibt erhalten
  await alpha.openTable("External");
  await expect(alpha.table()).toContainText("Showing: 2 of 2");
  await alpha.openTable("Response Codes");
  await expect(alpha.table()).toContainText("Showing: 1 of 27");
  await expect(alpha.table().getByRole("combobox", { name: "Filter" })).toHaveValue(/Client Error \(4XX\)/);
});

// Alpha 2, Sortierung (UI-010, Gleichstand F-3): gehört zu einem fremden Paket und trägt deshalb kein Spec-Tag.
test("sorting cycles ascending, descending and off, and ties keep the F-3 order (Address ascending)", async ({ alpha }) => {
  await crawled(alpha, "Page Titles", TALL);
  await expect(alpha.table()).toContainText("Showing: 22 of 22");
  const original = (await alpha.settledGrid()).rows;
  expect(original).toHaveLength(22);

  const byte = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  const length = (r: { cells: string[] }) => Number(r.cells[3]);
  const headerText = async () => (await alpha.gridWindow()).headers.join("|");

  const lengths = original.map(length);
  const sortedAsc = [...lengths].sort((a, b) => a - b);
  const sortedDesc = [...lengths].sort((a, b) => b - a);
  const tiesInAddressOrder = (rows: { cells: string[] }[]) => {
    for (const len of new Set(rows.map(length))) {
      const group = rows.filter((r) => length(r) === len).map((r) => r.cells[0]!);
      expect(group, `Gleichstand bei Länge ${len}`).toEqual([...group].sort(byte));
    }
  };

  // 1. Klick: aufsteigend, numerisch (nicht lexikografisch), Gleichstand nach Address (Bytevergleich)
  await sortBy(alpha, "Page Titles", "Title 1 Length");
  await expect.poll(headerText).toContain("Title 1 Length ↑");
  await expect.poll(async () => (await alpha.settledGrid()).rows.map(length)).toEqual(sortedAsc);
  const asc = (await alpha.settledGrid()).rows;
  expect(new Set(lengths).size).toBeLessThan(lengths.length); // es gibt Gleichstände
  tiesInAddressOrder(asc);
  expect(asc.slice(0, 2).map((r) => r.cells[2])).toEqual(["Shared title", "Shared title"]);

  // 2. Klick: absteigend; der Gleichstand bleibt nach Address aufsteigend
  await sortBy(alpha, "Page Titles", "Title 1 Length");
  await expect.poll(headerText).toContain("Title 1 Length ↓");
  await expect.poll(async () => (await alpha.settledGrid()).rows.map(length)).toEqual(sortedDesc);
  tiesInAddressOrder((await alpha.settledGrid()).rows);

  // 3. Klick: Sortierung aufgehoben, Ursprungsreihenfolge (Reihenfolge des Auffindens)
  await sortBy(alpha, "Page Titles", "Title 1 Length");
  await expect.poll(headerText).not.toMatch(/[↑↓]/);
  await expect.poll(async () => addresses((await alpha.settledGrid()).rows)).toEqual(addresses(original));
});

// Alpha 2, einfache Suche und Zähler „N von M“ (UI-011, UI-006): fremde IDs, kein Spec-Tag.
test("simple search matches the Address on Enter, shows N of M and clears with Escape", async ({ alpha }) => {
  const { page } = alpha;
  // K2-63/K2-77: all eleven search hits must fit at the new 28px density.
  await crawled(alpha, "Response Codes", TALL);
  await expect(alpha.table()).toContainText("Showing: 27 of 27");
  const search = page.getByRole("searchbox", { name: "Search Address", exact: true });
  await search.fill("PAGE-1");
  // ohne Enter ändert sich nichts
  await expect(alpha.table()).toContainText("Showing: 27 of 27");
  await search.press("Enter");
  // page-1.html und page-10 … page-19: Teilstring, ohne Groß-/Kleinschreibung
  await expect(alpha.table()).toContainText(/Showing: 11 of 27.*Search active/);
  const hits = addresses((await alpha.settledGrid()).rows);
  expect(hits).toHaveLength(11);
  expect(hits.every((a) => /page-1/.test(a))).toBe(true);
  await search.fill("no-such-address-xyz");
  await search.press("Enter");
  await expect(alpha.table()).toContainText(/Showing: 0 of 27.*Search active/);
  await search.press("Escape");
  await expect(alpha.table()).toContainText("Showing: 27 of 27");
  await expect(alpha.table()).not.toContainText("Search active");
});

type Snapshot = { address: string; relative: number; absolute: number };

/**
 * Sortiert die Tabelle nach Address, startet einen gedrosselten Crawl (1 URL/s), markiert „page-2.html“ sobald
 * neun Zeilen da sind und beobachtet die Zeile bis zum Ende des Crawls. Sie sortiert hinter „page-10.html“ … „page-20.html“, die
 * erst später entdeckt werden: Es kommen also sicher Zeilen oberhalb der Auswahl dazu. Gewertet werden nur Momentaufnahmen, die zweimal im
 * Abstand von 150 ms gleich sind (die Live-Aktualisierung zeigt keine Platzhalter mehr, Klärung K2-69: Befund A01-07).
 */
async function selectWhileSortedRowsArrive(alpha: Alpha): Promise<{ origin: Snapshot; samples: Snapshot[]; last: Snapshot }> {
  const { page } = alpha;
  // Niedriges Fenster: Die Tabelle fasst weniger Zeilen als beim Markieren schon da sind. Nur dann kann sie beim Einsortieren oberhalb
  // mitscrollen (UI-018); passt sie ganz ins Bild, rutscht die markierte Zeile zwangsläufig nach unten.
  await page.setViewportSize({ width: 1280, height: 600 });
  await alpha.open();
  await alpha.slowCrawl(1);
  await alpha.selectTab("Response Codes");
  await page.evaluate(async () => {
    const path = "/src/state/tabs.ts";
    const { useTabs } = await import(/* @vite-ignore */ path);
    useTabs.getState().query({ sort: [{ column: "Address", dir: "asc" }] });
  });
  await alpha.startFromHeader();
  await expect.poll(() => alpha.counter().catch(() => ""), { timeout: 30_000 }).toMatch(/Showing: (9|1\d|2\d) of/);
  await expect.poll(async () => (await alpha.gridWindow()).headers.join("|")).toContain("Address ↑");
  // Der Crawl wird zum Markieren angehalten: Ausgangslage und Klick liegen dann zwischen zwei Versionen der Ansicht und die
  // Bildschirmposition vor dem ersten Einsortieren ist eindeutig.
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await expect(page.getByTestId("crawl-phase")).toHaveText("Paused");
  await expect.poll(async () => { const a = await alpha.counter(); await page.waitForTimeout(600); return a === (await alpha.counter()); }).toBe(true);
  await alpha.settledGrid();
  // Die Fenstergröße hängt von der Engine ab: so weit scrollen, bis „page-2.html“ im Bild ist (der Crawl steht, die Tabelle ist ruhig).
  await page.locator(".os-data-grid").hover({ position: { x: 160, y: 60 } });
  // Die Zeile soll nicht die letzte im Bild sein: Mit klassischer Bildlaufleiste (Windows) verdeckt die waagerechte Leiste die unterste
  // Zeile teilweise, ein Klick auf deren Mitte träfe nichts. Nur am Ende der Liste (Fenster ändert sich nicht mehr) genügt die letzte (Befund aus der Windows-CI, Integrationsrunde 8).
  let target = -1;
  let previousFirst = -1;
  for (let i = 0; i < 40; i++) {
    const w = await alpha.gridWindow();
    target = w.rows.findIndex((r) => /\/page-2\.html$/.test(r.cells[0]!));
    const first = w.rows[0]?.index ?? -1;
    if (target >= 0 && (target <= w.rows.length - 3 || first === previousFirst)) break;
    previousFirst = first;
    await page.mouse.wheel(0, 44);
    await alpha.settledGrid();
  }
  expect(target, "page-2.html ist im Bild").toBeGreaterThanOrEqual(0);
  await page.locator(".os-data-grid").click({ position: { x: 160, y: 32 + 28 * target + 14 } });
  await expect(alpha.table()).toContainText("1 selected");

  const read = async (): Promise<Snapshot | null> => {
    const w = await alpha.gridWindow();
    if (w.rows.some((r) => r.cells.some((c) => c === "…"))) return null;
    const first = Math.min(...w.rows.map((r) => r.index));
    const sel = w.rows.find((r) => r.selected);
    return sel ? { address: sel.cells[0]!, relative: sel.index - first, absolute: sel.index } : null;
  };
  const stable = async (): Promise<Snapshot | null> => {
    const a = await read();
    await page.waitForTimeout(150);
    const b = await read();
    return a && b && JSON.stringify(a) === JSON.stringify(b) ? a : null;
  };
  let origin: Snapshot | null = null;
  for (let i = 0; i < 40 && !origin; i++) origin = await stable();
  expect(origin, "markierte Zeile ist nach dem Klick sichtbar").not.toBeNull();
  await page.getByRole("button", { name: "Resume", exact: true }).click();
  await expect(page.getByTestId("crawl-phase")).toHaveText("Running");
  const samples: Snapshot[] = [];
  for (let i = 0; i < 120; i++) {
    const state = await stable();
    if (state) samples.push(state);
    if (/Completed/.test(await page.getByTestId("crawl-phase").innerText())) break;
    await page.waitForTimeout(200);
  }
  await alpha.waitCompleted();
  // Nach dem Ende ist die markierte Zeile zu sehen; die Tabelle lädt ohne Zutun alle Zeilen der letzten Version.
  let last: Snapshot | null = null;
  for (let i = 0; i < 30 && !last; i++) last = await stable();
  expect(last, "markierte Zeile nach Abschluss sichtbar").not.toBeNull();
  return { origin: origin!, samples, last: last! };
}

// spec: UI-018
test("rows arriving during the crawl are sorted into the table in order", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await alpha.open();
  await alpha.slowCrawl(1);
  await alpha.selectTab("Response Codes");
  await page.evaluate(async () => {
    const path = "/src/state/tabs.ts";
    const { useTabs } = await import(/* @vite-ignore */ path);
    useTabs.getState().query({ sort: [{ column: "Address", dir: "asc" }] });
  });
  await alpha.startFromHeader();
  await expect.poll(async () => (await alpha.gridWindow()).headers.join("|")).toContain("Address ↑");
  const byte = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  const seen = new Set<number>();
  for (let i = 0; i < 160; i++) {
    const w = await alpha.gridWindow();
    if (w.rows.length > 0 && w.rows.every((r) => r.cells.every((c) => c !== "…"))) {
      const addresses = w.rows.map((r) => r.cells[0]!);
      expect(addresses, "die sichtbaren Zeilen bleiben nach Address aufsteigend sortiert, auch während neue eintreffen").toEqual([...addresses].sort(byte));
      seen.add(addresses.length);
    }
    if (/Completed/.test(await page.getByTestId("crawl-phase").innerText())) break;
    await page.waitForTimeout(250);
  }
  await alpha.waitCompleted();
  expect(seen.size, "die Tabelle wuchs während der Beobachtung").toBeGreaterThanOrEqual(2);
  // Nach dem Ende zeigt die Tabelle alle Zeilen (A01-20: auch die letzten), nach Address sortiert.
  await expect(alpha.table()).toContainText("Showing: 27 of 27");
  const final = (await alpha.settledGrid()).rows.map((r) => r.cells[0]!);
  expect(final.length).toBeGreaterThanOrEqual(20);
  expect(final).toEqual([...final].sort(byte));
});

// spec: UI-010
test("the sort gesture on a column header takes effect while the crawl is running", async ({ alpha }) => {
  test.setTimeout(90_000);
  const { page } = alpha;
  await alpha.open();
  await alpha.slowCrawl(1);
  await alpha.selectTab("Response Codes");
  await alpha.startFromHeader();
  await expect.poll(() => alpha.counter().catch(() => ""), { timeout: 30_000 }).toMatch(/Showing: ([89]|1\d|2\d) of/);
  await alpha.settledGrid();
  const byte = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  // Die Geste (Kopfklick) sortiert aufsteigend, dann absteigend, auch wenn die Ansicht im Aufbau war (Klärung K2-69: Befund A01-09).
  await sortBy(alpha, "Response Codes", "Address");
  await expect.poll(async () => (await alpha.gridWindow()).headers.join("|")).toContain("Address ↑");
  await expect(async () => {
    const rows = addresses((await alpha.settledGrid()).rows);
    expect(rows).toEqual([...rows].sort(byte));
  }).toPass();
  await sortBy(alpha, "Response Codes", "Address");
  await expect.poll(async () => (await alpha.gridWindow()).headers.join("|")).toContain("Address ↓");
  await expect(async () => {
    const rows = addresses((await alpha.settledGrid()).rows);
    expect(rows).toEqual([...rows].sort((a, b) => byte(b, a)));
  }).toPass();
  expect(await page.evaluate(async () => {
    const path = "/src/state/tabs.ts";
    const { useTabs } = await import(/* @vite-ignore */ path);
    return useTabs.getState().queries["Response Codes"].sort;
  })).toEqual([{ column: "Address", dir: "desc" }]);
});

// spec: UI-018
test("after a throttled crawl the counter shows the final N of M", async ({ alpha }) => {
  test.setTimeout(90_000);
  await alpha.open();
  await alpha.slowCrawl(2);
  await alpha.selectTab("Response Codes");
  await alpha.startFromHeader();
  await alpha.waitCompleted();
  await expect(alpha.table()).toContainText("Showing: 27 of 27", { timeout: 5000 });
});

// spec: UI-018
test("the selected URL stays selected while rows are sorted in above it during the crawl", async ({ alpha }) => {
  test.setTimeout(90_000);
  const { origin, samples, last } = await selectWhileSortedRowsArrive(alpha);
  expect(samples.length, "genügend auswertbare Momentaufnahmen").toBeGreaterThanOrEqual(2);
  for (const s of samples) expect(s.address).toBe(origin.address);
  expect(last.address).toBe(origin.address);
  // nachträglich gefundene URLs liegen oberhalb der Auswahl: ihre Zeilennummer wächst
  expect(last.absolute, "neue URLs wurden oberhalb der Auswahl einsortiert").toBeGreaterThan(origin.absolute);
  await expect(alpha.table()).toContainText("1 selected");
});

// spec: UI-018
test("the selected row keeps its screen position while rows are sorted in above it (anchor on the focus row)", async ({ alpha }) => {
  test.setTimeout(90_000);
  const { origin, samples, last } = await selectWhileSortedRowsArrive(alpha);
  const history = [origin, ...samples, last].map((s) => `${s.absolute - s.relative}/${s.relative}`).join(" ");
  for (const s of [...samples, last]) expect(s.relative, `Verlauf (erste Zeile/Abstand): ${history}`).toBe(origin.relative);
});

const dataTabsOf = (page: Page) => page.getByRole("tablist", { name: "Data tabs" }).getByRole("tab");

/** „Configure tabs“ per Tastatur öffnen (ein Tab überdeckt die Schaltfläche für Mausklicks, Klärung K2-69: Befund A01-08); ein offenes Menü bleibt offen. */
async function openConfigureTabs(page: Page): Promise<void> {
  const reset = page.getByRole("menuitem", { name: "Reset tabs", exact: true });
  if ((await reset.count()) === 0) {
    const configure = page.getByRole("button", { name: "Configure tabs", exact: true });
    await configure.focus();
    await configure.press("Enter");
  }
  await expect(reset).toBeVisible();
}

/** Schließt das Menü „Configure tabs“ durch einen Klick außerhalb und wartet, bis es weg ist. */
async function closeConfigureTabs(page: Page): Promise<void> {
  await page.getByRole("heading", { name: "OpenSpider", level: 1 }).click({ force: true });
  await expect(page.getByRole("menuitem", { name: "Reset tabs", exact: true })).toHaveCount(0);
}

/** Wartet, bis die Bridge den Zustand der Tab-Leiste gespeichert hat (`ui-state.json`). */
const savedTabs = (alpha: Alpha) => async () => {
  const state = (await alpha.invoke<{ state: { topTabs?: { order?: string[]; hidden?: string[] } } }>("uistate.load")).state ?? {};
  return { first: state.topTabs?.order?.[0], hidden: state.topTabs?.hidden ?? [] };
};

// spec: UI-004 [P1]
// M2-U34 (K2-77): Reihenfolge und Sichtbarkeit gelten je Gruppe; Ziehen und Ausblenden innerhalb der Gruppe URLs.
test("tabs can be reordered by dragging, the order persists through a restart and Reset tabs restores the default", async ({ alpha }) => {
  test.setTimeout(60_000);
  const { page } = alpha;
  await alpha.open();
  await expect(page.getByRole("tab", { name: "Internal", exact: true })).toBeVisible();
  await page.getByRole("tab", { name: "External", exact: true }).dragTo(page.getByRole("tab", { name: "Internal", exact: true }));
  await expect(dataTabsOf(page).first()).toHaveText("External");
  // gespeichert wird über die Bridge (ui-state.json); erst danach neu laden
  await expect.poll(savedTabs(alpha)).toEqual({ first: "External", hidden: [] });
  await page.reload();
  await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
  await expect(dataTabsOf(page).first()).toHaveText("External");
  // Zurücksetzen stellt die Standardreihenfolge wieder her und speichert sie
  await openConfigureTabs(page);
  await page.getByRole("menuitem", { name: "Reset tabs", exact: true }).click();
  await expect(dataTabsOf(page).first()).toHaveText("Internal");
  await expect.poll(savedTabs(alpha)).toEqual({ first: "Internal", hidden: [] });
});

// spec: UI-004 [P1]
test("tabs can be hidden, the choice persists through a restart, Reset tabs shows them again and Internal cannot be hidden", async ({ alpha }) => {
  test.setTimeout(60_000);
  const { page } = alpha;
  await alpha.open();
  await openConfigureTabs(page);
  await expect(page.getByRole("menuitemcheckbox", { name: "Internal", exact: true })).toBeDisabled();
  await page.getByRole("menuitemcheckbox", { name: "Security", exact: true }).click();
  await expect(page.getByRole("menuitemcheckbox", { name: "Security", exact: true })).toHaveAttribute("aria-checked", "false");
  await closeConfigureTabs(page);
  await expect(page.getByRole("tab", { name: "Security", exact: true })).toHaveCount(0);
  await expect.poll(savedTabs(alpha)).toMatchObject({ hidden: ["Security"] });
  await page.reload();
  await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
  await expect(page.getByRole("tab", { name: "Security", exact: true })).toHaveCount(0);
  // Der Haken im Menü „Configure tabs“ zeigt die Sichtbarkeit (die Tab-Leiste rendert überzählige Tabs nicht immer im DOM)
  await openConfigureTabs(page);
  await expect(page.getByRole("menuitemcheckbox", { name: "Security", exact: true })).toHaveAttribute("aria-checked", "false");
  await closeConfigureTabs(page);
  await openConfigureTabs(page);
  await page.getByRole("menuitem", { name: "Reset tabs", exact: true }).click();
  await expect.poll(savedTabs(alpha)).toMatchObject({ hidden: [] });
  await openConfigureTabs(page);
  await expect(page.getByRole("menuitemcheckbox", { name: "Security", exact: true })).toHaveAttribute("aria-checked", "true");
  await closeConfigureTabs(page);
});

// spec: UI-004 [P1]
test("reordering and then hiding a tab keeps both changes in the saved tab layout", async ({ alpha }) => {
  test.setTimeout(60_000);
  const { page } = alpha;
  await alpha.open();
  await page.getByRole("tab", { name: "External", exact: true }).dragTo(page.getByRole("tab", { name: "Internal", exact: true }));
  await expect(dataTabsOf(page).first()).toHaveText("External");
  await openConfigureTabs(page);
  await page.getByRole("menuitemcheckbox", { name: "Security", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect.poll(savedTabs(alpha)).toEqual({ first: "External", hidden: ["Security"] });
  await page.reload();
  await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
  await expect(dataTabsOf(page).first()).toHaveText("External");
});

// spec: UI-007
test("empty values stay empty cells, status codes and numbers appear as text and Address is pinned left", async ({ alpha }) => {
  await crawled(alpha, "Page Titles", TALL);
  await expect(alpha.table()).toContainText("Showing: 22 of 22");
  const window = await alpha.settledGrid();
  expect(window.headers[0]).toBe("Address");
  for (const row of window.rows) for (const cell of row.cells) expect(["null", "undefined", "NaN", "0 undefined"]).not.toContain(cell);
  const home = window.rows.find((r) => r.cells[0] === alpha.startUrl)!;
  expect(home.cells[window.headers.indexOf("Title 2")]).toBe(""); // keine zweite Titelangabe: leere Zelle
  expect(home.cells[window.headers.indexOf("Title 1 Length")]).toBe("14");
  await alpha.openTable("Response Codes");
  const status = (await alpha.settledGrid()).rows.map((r) => r.cells[2]);
  expect(status).toContain("200");
  expect(status).toContain("301");
  expect(status).toContain("404");
  // Address bleibt beim horizontalen Scrollen stehen (angeheftet): die erste Spalte ist in der Spaltenverwaltung als `pinned` markiert
  const pinned = await alpha.page.evaluate(async () => {
    const path = "/src/state/columns.ts";
    const { useColumns } = await import(/* @vite-ignore */ path);
    return (useColumns.getState().byTab["Response Codes"] as { key: string; pinned: boolean }[]).filter((c) => c.pinned).map((c) => c.key);
  });
  expect(pinned).toContain("Address");
});

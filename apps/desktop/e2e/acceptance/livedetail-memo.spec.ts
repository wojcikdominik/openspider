// Unabhängige Abnahme B4-LIVEDETAIL, Teil 3: Sprache, Theme und Spaltenänderungen wirken trotz Memoisierung sofort auf offene
// Tabellen, Detailfenster und Overview – während der Crawl noch läuft (UI-121, UI-120, UI-008, UI-009, UI-030).
// acceptance: independent
import type { Page } from "@playwright/test";
import {
  TALL,
  crawlMore,
  crawlTo,
  expect,
  test,
  clickUpperRow,
  crawledCount,
  filledCells,
  openDetailTab,
  readDetail,
  readUpper,
  persistedDetailColumn,
  startGated,
  upperGrid,
  upperHeaders,
} from "./livedetail-support";

const strip = (headers: string[]) => headers.map((h) => h.replace(/\s*[↑↓]\d?$/, ""));
const overviewLabels = (page: Page) =>
  page.evaluate(() => [...document.querySelectorAll(".os-sidebar-overview table tbody tr th")].slice(0, 6).map((e) => (e.textContent ?? "").trim()));
const chips = (page: Page) => page.evaluate(() => [...document.querySelectorAll("#data-panel .u34-chips button")].map((e) => (e.textContent ?? "").replace(/\s*\(.*$/, "").trim()));

async function selectHomeInlinks(page: Page, startUrl: string): Promise<void> {
  await clickUpperRow(page, startUrl);
  await openDetailTab(page, "Inlinks");
  await expect.poll(async () => (await readDetail(page))?.rowcount ?? 0, { timeout: 10_000 }).toBeGreaterThan(2);
}

// spec: UI-121
// spec: UI-018
// spec: UI-007
// acceptance: independent
test("a language change relabels the open tables, the detail window and the Overview at once and the crawl data keeps growing", async ({ alpha, gate }) => {
  test.setTimeout(150_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startGated(alpha, gate);
  await crawlTo(page, gate, 4);
  await selectHomeInlinks(page, gate.startUrl);

  const en = {
    upper: strip(await upperHeaders(page)),
    detail: strip((await readDetail(page))!.headers),
    overview: await overviewLabels(page),
    chips: await chips(page),
  };
  const enRows = (await readDetail(page))!.rowcount;

  const settings = await alpha.openSettings();
  await settings.getByRole("button", { name: /Language/ }).click();
  await settings.getByRole("option", { name: "Deutsch" }).click();

  // sofort (kein Neustart, laufender Crawl): Tabellenfuß, Kopfzeilen der oberen und der Detailtabelle, Overview, Filter-Chips
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
  await expect(page.locator("#data-panel .u10-counter")).toContainText("Angezeigt:", { timeout: 15_000 });
  await expect.poll(async () => JSON.stringify(strip(await upperHeaders(page))), { timeout: 15_000 }).not.toBe(JSON.stringify(en.upper));
  await expect.poll(async () => JSON.stringify(strip((await readDetail(page))?.headers ?? [])), { timeout: 15_000 }).not.toBe(JSON.stringify(en.detail));
  await expect.poll(async () => JSON.stringify(await overviewLabels(page)), { timeout: 15_000 }).not.toBe(JSON.stringify(en.overview));
  await expect.poll(async () => JSON.stringify(await chips(page)), { timeout: 15_000 }).not.toBe(JSON.stringify(en.chips));
  const de = { upper: strip(await upperHeaders(page)), detail: strip((await readDetail(page))!.headers), overview: await overviewLabels(page) };
  // kein Schlüsselname statt Text (UI-121: „nie auf einen Schlüsselnamen“)
  for (const label of [...de.upper, ...de.detail, ...de.overview]) expect(label, "übersetzter Text, kein Schlüsselname").not.toMatch(/^[a-z]+(-[a-z0-9]+)+$/);

  // Die Daten wachsen weiter: Detailzeilen, Tabellenfuß und Overview in der neuen Sprache.
  const before = await crawledCount(page);
  await crawlMore(page, gate, 4);
  await expect.poll(async () => ((await readDetail(page))?.rowcount ?? 0)).toBeGreaterThan(enRows);
  const deFooter = await page.locator("#data-panel .u10-counter").innerText();
  expect(deFooter).toMatch(/^Angezeigt: \d+ von \d+/);
  const rowsDe = await readUpper(page);
  expect(rowsDe!.rows.length).toBeGreaterThan(before - 1);
  expect(rowsDe!.rows.every((r) => r.cells.every((c) => c !== "…" && c !== "undefined" && c !== "null")), "Zellen zeigen Werte, keine Platzhalter").toBe(true);

  // zurück auf Englisch: identische englische Beschriftungen wie vorher
  await settings.getByRole("button", { name: /Sprache/ }).click();
  await settings.getByRole("option", { name: "English" }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect.poll(async () => JSON.stringify(strip(await upperHeaders(page))), { timeout: 15_000 }).toBe(JSON.stringify(en.upper));
  await expect.poll(async () => JSON.stringify(strip((await readDetail(page))?.headers ?? [])), { timeout: 15_000 }).toBe(JSON.stringify(en.detail));
  await expect.poll(async () => JSON.stringify(await overviewLabels(page)), { timeout: 15_000 }).toBe(JSON.stringify(en.overview));
  gate.open();
  await alpha.waitCompleted(60_000);
  await expect.poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 15_000 }).toBe(20);
  await expect(page.locator("#data-panel .u10-counter")).toContainText("Showing: 27 of 27");
});

/** Helligkeit (0–255) eines Pixels in der oberen linken Ecke der Zeichenfläche. */
async function canvasLuma(page: Page, scope: string): Promise<number> {
  return page.evaluate((scope) => {
    const canvas = [...document.querySelectorAll<HTMLCanvasElement>(`${scope} canvas`)].find((c) => !c.closest("[hidden]") && c.width > 0);
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return -1;
    // Kopfzeile, linke obere Ecke
    const [r, g, b] = ctx.getImageData(2, 2, 1, 1).data;
    return 0.299 * r! + 0.587 * g! + 0.114 * b!;
  }, scope);
}

// spec: UI-120
// spec: UI-018
// acceptance: independent
test("a theme change repaints the open tables, the detail window and the sidebar at once while the crawl runs", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await page.emulateMedia({ colorScheme: "light" });
  await page.setViewportSize(TALL);
  await startGated(alpha, gate);
  await crawlTo(page, gate, 4);
  await selectHomeInlinks(page, gate.startUrl);
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  const sidebarBg = () => page.evaluate(() => getComputedStyle(document.querySelector(".os-sidebar-overview th, .os-sidebar-overview td") ?? document.body).color);
  const lightBody = await bg();
  const lightSidebar = await sidebarBg();
  const lightUpper = await canvasLuma(page, "#data-panel");
  const lightDetail = await canvasLuma(page, "section.os-detail");
  expect(lightUpper).toBeGreaterThan(150);
  expect(lightDetail).toBeGreaterThan(150);

  const settings = await alpha.openSettings();
  await settings.getByRole("tab", { name: "Appearance" }).click();
  await settings.getByRole("radiogroup", { name: "Theme" }).getByText("Dark", { exact: true }).click();
  // UI-120: ≤ 200 ms im Release-Build; hier genügt, dass alle Flächen innerhalb von 2 s umschalten (Dev-Server, laufender Crawl)
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark", { timeout: 15_000 });
  await expect.poll(bg, { timeout: 15_000 }).not.toBe(lightBody);
  await expect.poll(sidebarBg, { timeout: 15_000 }).not.toBe(lightSidebar);
  await expect.poll(() => canvasLuma(page, "#data-panel"), { timeout: 15_000 }, ).toBeLessThan(100);
  await expect.poll(() => canvasLuma(page, "section.os-detail"), { timeout: 15_000 }).toBeLessThan(100);

  // neue Zeilen kommen im dunklen Thema an (kein Rückfall in das helle Thema durch Live-Aktualisierung)
  const before = (await readDetail(page))!.rowcount;
  gate.allow(3);
  await expect.poll(async () => (await readDetail(page))!.rowcount, { timeout: 10_000 }).toBeGreaterThan(before);
  expect(await canvasLuma(page, "#data-panel")).toBeLessThan(100);
  expect(await canvasLuma(page, "section.os-detail")).toBeLessThan(100);

  await settings.getByRole("radiogroup", { name: "Theme" }).getByText("Light", { exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light", { timeout: 15_000 });
  await expect.poll(() => canvasLuma(page, "#data-panel"), { timeout: 15_000 }).toBeGreaterThan(150);
  await expect.poll(() => canvasLuma(page, "section.os-detail"), { timeout: 15_000 }).toBeGreaterThan(150);
  expect(await bg()).toBe(lightBody);
  expect(await sidebarBg(), "Textfarbe der Overview wie im hellen Thema").toBe(lightSidebar);
});

// spec: UI-008
// spec: UI-009
// spec: UI-018
// acceptance: independent
test("column changes in the upper table act on the open table at once and survive live updates", async ({ alpha, gate }) => {
  test.setTimeout(150_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startGated(alpha, gate, "Internal");
  await crawlTo(page, gate, 4);
  const initial = strip(await upperHeaders(page));
  const victim = "Status Code";
  expect(initial, "Status Code gehört zu den Standardspalten von Internal").toContain(victim);

  const plus = page.getByRole("button", { name: "Configure columns", exact: true });
  const dialog = page.getByRole("dialog", { name: "Configure columns", exact: true });
  const box = dialog.getByRole("checkbox", { name: victim, exact: true });
  const headers = async () => strip(await upperHeaders(page));

  // Abwählen wirkt sofort (UI-008); „Abbrechen“ verwirft (UI-009)
  await plus.click();
  await expect(dialog).toBeVisible();
  await box.uncheck();
  await expect.poll(headers, { timeout: 15_000 }).not.toContain(victim);
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect.poll(headers, { timeout: 15_000 }).toContain(victim);

  // „Anwenden“ übernimmt; die Tabelle zeigt weiter neue Zeilen ohne die Spalte
  await plus.click();
  await box.uncheck();
  await dialog.getByRole("button", { name: "Apply", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  // Internal listet auch entdeckte, noch nicht gecrawlte URLs: neue Daten zeigen sich als gefüllte Zellen der Spalte „Content“.
  const filled0 = await filledCells(page, "Content");
  gate.allow(3);
  await expect.poll(() => filledCells(page, "Content"), { timeout: 10_000 }).toBeGreaterThan(filled0);
  expect(await headers(), "Spalte bleibt trotz neuer Daten ausgeblendet").not.toContain(victim);
  const upperRows = (await readUpper(page))!.rows;
  expect(upperRows.every((r) => r.cells.length === (initial.length - 1)), "Zeilen haben genau die sichtbaren Spalten").toBe(true);

  // Erneutes Öffnen zeigt den aktuellen Stand (Spalte abgewählt)
  await plus.click();
  await expect(box).not.toBeChecked();
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(await headers()).not.toContain(victim);

  // Erweiterter Konfigurator: Spalte steht unter „Available“, Hinzufügen + OK blendet sie wieder ein
  await page.getByRole("button", { name: "Configure columns…", exact: true }).click();
  const configurator = page.getByRole("dialog", { name: "Configure columns", exact: true });
  await expect(configurator).toBeVisible();
  const available = configurator.getByRole("listbox", { name: "Available columns" });
  const displayed = configurator.getByRole("listbox", { name: "Displayed columns" });
  await expect(available.getByRole("option", { name: victim, exact: true })).toBeVisible();
  await expect(displayed.getByRole("option", { name: victim, exact: true })).toHaveCount(0);
  await available.getByRole("option", { name: victim, exact: true }).click();
  await configurator.getByRole("button", { name: "Add", exact: true }).click();
  // neu hinzugefügte Spalten stehen am Ende (außerhalb des Bildes): nach oben schieben, damit sie sichtbar sind
  await configurator.getByRole("button", { name: "To top", exact: true }).click();
  await configurator.getByRole("button", { name: "OK", exact: true }).click();
  await expect(configurator).toHaveCount(0);
  await expect.poll(headers, { timeout: 15_000 }).toContain(victim);
  await expect.poll(async () => (await readUpper(page))!.rows.every((r) => r.cells.every((c) => c !== "…")), { timeout: 15_000 }).toBe(true);
  const col = (await headers()).indexOf(victim);
  const statusCells = (await readUpper(page))!.rows.map((r) => r.cells[col]);
  // Internal listet auch noch nicht gecrawlte URLs (leere Zellen); gefüllte Zellen zeigen einen dreistelligen Statuscode
  expect(statusCells.filter((c) => c).every((c) => /^\d{3}$/.test(c ?? "")), `Status-Code-Zellen zeigen Werte: ${statusCells.join(",")}`).toBe(true);
  expect(statusCells.filter((c) => c).length, "gecrawlte URLs haben einen Statuscode").toBeGreaterThan(0);
  gate.open();
  await alpha.waitCompleted(60_000);
  await expect(page.locator("#data-panel .u10-counter")).toContainText("Showing: 25 of 25");
});

// spec: UI-030
// spec: UI-008
// spec: UI-018
// acceptance: independent
test("column choices of a detail table apply at once, survive new rows and a later change of the selection", async ({ alpha, gate }) => {
  test.setTimeout(150_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startGated(alpha, gate);
  await crawlTo(page, gate, 4);
  await selectHomeInlinks(page, gate.startUrl);
  const victim = "Anchor Text";
  const headers = async () => strip((await readDetail(page))?.headers ?? []);
  expect(await headers()).toContain(victim);

  const tools = page.locator("section.os-detail .os-detail-table-tools:visible").first();
  const openColumns = async () => {
    if (!(await tools.locator("details").evaluate((d) => (d as HTMLDetailsElement).open))) await tools.locator("summary").click();
  };
  await openColumns();
  await tools.getByRole("checkbox", { name: victim, exact: true }).uncheck();
  await expect.poll(headers, { timeout: 15_000 }).not.toContain(victim);
  const rows0 = (await readDetail(page))!.rowcount;
  gate.allow(3);
  await expect.poll(async () => (await readDetail(page))!.rowcount, { timeout: 10_000 }).toBeGreaterThan(rows0);
  expect(await headers(), "Spalte bleibt mit neuen Zeilen ausgeblendet").not.toContain(victim);
  const first = (await readDetail(page))!.rows[0]!;
  expect(first.cells.length, "Zeilen haben genau die sichtbaren Spalten").toBe((await headers()).length);

  // Auswahl wechseln und zurück (nach dem Speichern der Spaltenwahl): die Spaltenwahl bleibt
  await expect.poll(() => persistedDetailColumn(page, "inlinks:links", victim), { timeout: 15_000 }).toBe(false);
  await upperGrid(page).focus();
  await page.keyboard.press("ArrowDown");
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 15_000 }).toBeLessThan(4);
  await page.waitForTimeout(1000);
  expect(await headers(), "ausgeblendete Spalte bleibt nach dem Wechsel der Auswahl ausgeblendet").not.toContain(victim);
  await page.keyboard.press("ArrowUp");
  await expect.poll(async () => (await readDetail(page))?.rowcount ?? 0, { timeout: 15_000 }).toBeGreaterThan(6);
  await page.waitForTimeout(1000);
  expect(await headers()).not.toContain(victim);

  // wieder einblenden
  await openColumns();
  await tools.getByRole("checkbox", { name: victim, exact: true }).check();
  await expect.poll(headers, { timeout: 15_000 }).toContain(victim);
  const col = (await headers()).indexOf(victim);
  await expect.poll(async () => (await readDetail(page))!.rows.every((r) => r.cells[col] === "Home"), { timeout: 15_000 }).toBe(true);
});

// spec: UI-030
// spec: UI-008
// acceptance: independent
test("a column change of a detail table is not lost when the selection changes right after it", async ({ alpha, gate }) => {
  test.setTimeout(150_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startGated(alpha, gate);
  await crawlTo(page, gate, 4);
  await selectHomeInlinks(page, gate.startUrl);
  const victim = "Anchor Text";
  const headers = async () => strip((await readDetail(page))?.headers ?? []);
  const tools = page.locator("section.os-detail .os-detail-table-tools:visible").first();
  if (!(await tools.locator("details").evaluate((d) => (d as HTMLDetailsElement).open))) await tools.locator("summary").click();
  await tools.getByRole("checkbox", { name: victim, exact: true }).uncheck();
  await expect.poll(headers, { timeout: 15_000 }).not.toContain(victim);
  // Der Nutzer wählt gleich danach die nächste Zeile (Pfeil nach unten): die eben getroffene Spaltenwahl gilt weiter (UI-008: Abwählen
  // blendet die Spalte aus, Sichtbarkeit wird je Tab gespeichert).
  await upperGrid(page).focus();
  await page.keyboard.press("ArrowDown");
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 15_000 }).toBeLessThan(4);
  await page.waitForTimeout(3000);
  expect(await headers(), "ausgeblendete Spalte erscheint nach dem Wechsel der Auswahl nicht wieder").not.toContain(victim);
});

// spec: UI-121
// spec: UI-120
// spec: UI-018
// spec: UI-004
// acceptance: independent
test("the Overview dashboard changes language and theme at once while its numbers keep growing", async ({ alpha, gate }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await page.emulateMedia({ colorScheme: "light" });
  await startGated(alpha, gate, "Internal");
  // erst wenn der Crawl läuft: ein neu gestarteter Crawl setzt die Ansicht auf die Tabellen zurück
  await crawlTo(page, gate, 1);
  await page.locator(".u34-nav-item[data-group=overview] .u34-nav-button").click();
  const dashboard = page.locator("[data-dashboard]");
  await expect(dashboard).toBeVisible();
  const labels = () => dashboard.evaluate((el) => [...el.querySelectorAll(".os-dashboard-card > span, .os-dashboard-card h2")].map((e) => (e.textContent ?? "").trim()));
  const crawledCard = () =>
    dashboard.evaluate((el) => {
      const card = [...el.querySelectorAll(".os-dashboard-card")].filter((c) => c.querySelector("strong"))[1];
      return Number((card?.querySelector("strong")?.textContent ?? "").replace(/[^\d]/g, ""));
    });
  await expect.poll(crawledCard, { timeout: 10_000 }).toBeGreaterThan(0);
  const enLabels = await labels();
  const bg = () => dashboard.evaluate((el) => getComputedStyle(el.querySelector(".os-dashboard-card")!).backgroundColor);
  const text = () => dashboard.evaluate((el) => getComputedStyle(el.querySelector(".os-dashboard-card strong")!).color);
  const lightBg = await bg();
  const lightText = await text();

  const settings = await alpha.openSettings();
  await settings.getByRole("button", { name: /Language/ }).click();
  await settings.getByRole("option", { name: "Deutsch" }).click();
  await expect.poll(async () => JSON.stringify(await labels()), { timeout: 15_000 }).not.toBe(JSON.stringify(enLabels));
  const before = await crawledCard();
  gate.allow(2);
  await expect.poll(crawledCard, { timeout: 15_000 }).toBeGreaterThan(before);
  await settings.getByRole("button", { name: /Sprache/ }).click();
  await settings.getByRole("option", { name: "English" }).click();
  await expect.poll(async () => JSON.stringify(await labels()), { timeout: 15_000 }).toBe(JSON.stringify(enLabels));

  await settings.getByRole("tab", { name: "Appearance" }).click();
  await settings.getByRole("radiogroup", { name: "Theme" }).getByText("Dark", { exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark", { timeout: 15_000 });
  await expect.poll(async () => `${await bg()}|${await text()}`, { timeout: 15_000 }).not.toBe(`${lightBg}|${lightText}`);
  const darkBefore = await crawledCard();
  gate.allow(2);
  await expect.poll(crawledCard, { timeout: 15_000 }).toBeGreaterThan(darkBefore);
  await settings.getByRole("radiogroup", { name: "Theme" }).getByText("Light", { exact: true }).click();
  await expect.poll(async () => `${await bg()}|${await text()}`, { timeout: 15_000 }).toBe(`${lightBg}|${lightText}`);
  gate.open();
  await alpha.waitCompleted(60_000);
  await expect.poll(crawledCard, { timeout: 12_000 }).toBe(27);
});

// Unabhängige Abnahme B4-LIVEDETAIL, Teil 4: Dialoge und Auswahllisten (Spaltenkonfigurator, Spaltenmenü, Tab-Konfiguration,
// Filterliste, Auswahllisten im Detailfenster) während eines laufenden Crawls: Öffnen funktioniert, Zustand und Fokus stimmen,
// beim erneuten Öffnen zeigen sie den aktuellen Stand, Änderungen gehen nicht verloren (UI-004, UI-006, UI-008, UI-009, UI-030, UI-126).
// acceptance: independent
import type { Locator, Page } from "@playwright/test";
import { expect, test, type Alpha } from "../alpha/fixture";
import {
  TALL,
  activeChipCount,
  clickUpperRow,
  coreTotal,
  crawledCount,
  filledCells,
  footer,
  openDetailTab,
  pauseCrawl,
  phase,
  readDetail,
  readGrid,
  readUpper,
  selectedUpper,
  startSlow,
  upperGrid,
  upperHeaders,
  waitCrawled,
} from "./livedetail-support";

const strip = (headers: string[]) => headers.map((h) => h.replace(/\s*[↑↓]\d?$/, ""));

/** Das fokussierte Element liegt innerhalb von `scope`. */
const focusInside = (scope: Locator) => scope.evaluate((el) => el.contains(document.activeElement));
const focusIs = (target: Locator) => target.evaluate((el) => el === document.activeElement);

// spec: UI-009
// spec: UI-126
// spec: UI-018
// acceptance: independent
test("the column dialogs open from the keyboard, hold focus, restore it on Escape and show the current state when reopened", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startSlow(alpha, 1, "Internal");
  await waitCrawled(page, 4);
  const victim = "Status Code";
  const headers = async () => strip(await upperHeaders(page));
  expect(await headers()).toContain(victim);

  // 1. Spaltenmenü „+“: Tastatur öffnet, Fokus im Dialog, Esc schließt und gibt den Fokus an den Auslöser zurück
  const plus = page.getByRole("button", { name: "Configure columns", exact: true });
  const menu = page.getByRole("dialog", { name: "Configure columns", exact: true });
  await plus.focus();
  await page.keyboard.press("Enter");
  await expect(menu).toBeVisible();
  expect(await focusInside(menu), "Fokus liegt im geöffneten Dialog").toBe(true);
  await menu.getByRole("checkbox", { name: victim, exact: true }).uncheck();
  await expect.poll(headers, { timeout: 2000 }).not.toContain(victim);
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  // Esc entspricht „Abbrechen“ (UI-009): die Änderung ist verworfen
  await expect.poll(headers, { timeout: 2000 }).toContain(victim);
  expect(await focusIs(plus), "Fokus kehrt zum Auslöser zurück (UI-126: Tastaturbedienung)").toBe(true);

  // 2. Konfigurator: Auslöser per Tastatur, Entfernen + OK übernimmt, erneutes Öffnen zeigt den aktuellen Stand
  const opener = page.getByRole("button", { name: "Configure columns…", exact: true });
  const configurator = page.getByRole("dialog", { name: "Configure columns", exact: true });
  await opener.focus();
  await page.keyboard.press("Enter");
  await expect(configurator).toBeVisible();
  expect(await focusInside(configurator)).toBe(true);
  const displayed = configurator.getByRole("listbox", { name: "Displayed columns" });
  const available = configurator.getByRole("listbox", { name: "Available columns" });
  await expect(displayed.getByRole("option", { name: victim, exact: true })).toBeVisible();
  await displayed.getByRole("option", { name: victim, exact: true }).click();
  await configurator.getByRole("button", { name: "Remove", exact: true }).click();
  await expect(available.getByRole("option", { name: victim, exact: true })).toBeVisible();
  // Abbrechen verwirft
  await configurator.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(configurator).toHaveCount(0);
  expect(await headers(), "Abbrechen verwirft die Änderung").toContain(victim);
  expect(await focusIs(opener) || (await page.evaluate(() => document.activeElement === document.body)) === false, "Fokus ist nach dem Schließen nicht verloren").toBe(true);
  // erneut: Entfernen + OK
  await opener.focus();
  await page.keyboard.press("Enter");
  await displayed.getByRole("option", { name: victim, exact: true }).click();
  await configurator.getByRole("button", { name: "Remove", exact: true }).click();
  await configurator.getByRole("button", { name: "OK", exact: true }).click();
  await expect(configurator).toHaveCount(0);
  await expect.poll(headers, { timeout: 2000 }).not.toContain(victim);
  // zwischenzeitlich kamen Zeilen dazu; Öffnen zeigt den aktuellen Stand, nicht den des ersten Öffnens
  const crawled = await crawledCount(page);
  await waitCrawled(page, crawled + 2);
  await opener.focus();
  await page.keyboard.press("Enter");
  await expect(configurator).toBeVisible();
  await expect(available.getByRole("option", { name: victim, exact: true })).toBeVisible();
  await expect(displayed.getByRole("option", { name: victim, exact: true })).toHaveCount(0);
  // Suchfeld ist leer (kein Rest der vorigen Sitzung)
  await expect(configurator.getByRole("textbox", { name: "Find columns" })).toHaveValue("");
  await available.getByRole("option", { name: victim, exact: true }).click();
  await configurator.getByRole("button", { name: "Add", exact: true }).click();
  // neu hinzugefügte Spalten stehen am Ende (außerhalb des Bildes): nach oben schieben, damit sie sichtbar sind
  await configurator.getByRole("button", { name: "To top", exact: true }).click();
  await configurator.getByRole("button", { name: "OK", exact: true }).click();
  await expect.poll(headers, { timeout: 2000 }).toContain(victim);
  // die Tabelle läuft weiter, bis der Crawl fertig ist (25 interne URLs)
  await alpha.waitCompleted(60_000);
  await expect(page.locator("#data-panel .u10-counter")).toContainText("Showing: 25 of 25");
  await expect.poll(() => filledCells(page, "Status Code"), { timeout: 8000 }).toBeGreaterThan(0);
  {
    const grid = (await readUpper(page))!;
    const col = strip(grid.headers).indexOf("Status Code");
    console.log("[livedetail] Status Code cells at the end: " + grid.rows.map((r) => `${r.cells[0]!.replace(/^http:\/\/127\.0\.0\.1:\d+/, "")}=${r.cells[col]}`).join(" "));
  }
});

async function openTabsConfig(page: Page, bar: "top" | "detail"): Promise<Locator> {
  const list = page.getByRole("tablist", { name: bar === "top" ? /^Data tabs$/ : /^Detail tabs$/ });
  const trigger = list.locator("xpath=ancestor::div[contains(@class,'os-tabbar')][1]").getByRole("button", { name: bar === "top" ? /^Configure tabs$/ : /^More detail tabs$/ });
  await trigger.click();
  await page.getByRole("menuitem", { name: "Configure tabs…", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Configure tabs", exact: true });
  await expect(dialog).toBeVisible();
  return dialog;
}

// spec: UI-004
// spec: UI-030
// spec: UI-018
// acceptance: independent
test("the tab configuration dialogs of the data tabs and the detail tabs show the current state and keep applied changes", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startSlow(alpha, 2);
  await waitCrawled(page, 4);
  await clickUpperRow(page, alpha.startUrl);
  const topList = page.getByRole("tablist", { name: "Data tabs" });
  const detailList = page.getByRole("tablist", { name: "Detail tabs" });

  // obere Tabs: „Security“ ausblenden – Abbrechen verwirft, OK übernimmt
  let dialog = await openTabsConfig(page, "top");
  const security = dialog.getByRole("checkbox", { name: "Security", exact: true });
  await expect(security).toBeChecked();
  await security.uncheck();
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(topList.getByRole("tab", { name: "Security", exact: true })).toHaveCount(1);
  dialog = await openTabsConfig(page, "top");
  await expect(dialog.getByRole("checkbox", { name: "Security", exact: true }), "nach Abbrechen steht der Dialog wieder auf dem aktuellen Stand").toBeChecked();
  await dialog.getByRole("checkbox", { name: "Security", exact: true }).uncheck();
  await dialog.getByRole("button", { name: "OK", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(topList.getByRole("tab", { name: "Security", exact: true })).toHaveCount(0);
  // der Crawl läuft weiter und die Tabellen aktualisieren sich unabhängig vom Dialog
  const shown = (await footer(page)).shown;
  await expect.poll(async () => (await footer(page)).shown, { timeout: 10_000 }).toBeGreaterThan(shown);
  dialog = await openTabsConfig(page, "top");
  await expect(dialog.getByRole("checkbox", { name: "Security", exact: true }), "geänderter Stand wird beim erneuten Öffnen gezeigt").not.toBeChecked();
  await dialog.getByRole("checkbox", { name: "Security", exact: true }).check();
  await dialog.getByRole("button", { name: "OK", exact: true }).click();
  await expect(topList.getByRole("tab", { name: "Security", exact: true })).toHaveCount(1);

  // Detail-Tabs: „Resources“ ausblenden, danach Auswahl wechseln – die Konfiguration bleibt
  await openDetailTab(page, "Inlinks");
  dialog = await openTabsConfig(page, "detail");
  const resources = dialog.getByRole("checkbox", { name: "Resources", exact: true });
  await expect(resources).toBeChecked();
  await resources.uncheck();
  await dialog.getByRole("button", { name: "OK", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await page.locator("#data-panel .os-data-grid").focus();
  await page.keyboard.press("ArrowDown");
  await expect.poll(async () => (await readDetail(page))?.rowcount, { timeout: 5000 }).toBeLessThan(4);
  await expect(detailList.getByRole("tab", { name: "Resources", exact: true })).toHaveCount(0);
  dialog = await openTabsConfig(page, "detail");
  await expect(dialog.getByRole("checkbox", { name: "Resources", exact: true })).not.toBeChecked();
  await dialog.getByRole("checkbox", { name: "Resources", exact: true }).check();
  await dialog.getByRole("button", { name: "OK", exact: true }).click();
  await expect(detailList.getByRole("tab", { name: "Resources", exact: true })).toHaveCount(1);
});

// spec: UI-006
// spec: UI-018
// spec: UI-126
// acceptance: independent
test("the filter list opens during the crawl, shows current counts each time and equals the core after a pause", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startSlow(alpha, 3);
  await waitCrawled(page, 5);
  const combo = page.locator("#data-panel").getByRole("combobox", { name: "Filter" });
  const allCount = async () => {
    const value = await combo.inputValue();
    const m = /\(([\d.,]+)\)\s*$/.exec(value);
    return m ? Number(m[1]!.replace(/[^\d]/g, "")) : -1;
  };
  const optionAll = async () => {
    await page.locator("#data-panel").getByRole("button", { name: "Filter", exact: true }).click();
    const text = await page.getByRole("option", { name: /^All\s*\d+/ }).first().innerText();
    await page.keyboard.press("Escape");
    return Number(text.replace(/[^\d]/g, ""));
  };
  const first = await optionAll();
  expect(first).toBeGreaterThan(0);
  await expect(page.getByRole("listbox")).toHaveCount(0);
  await waitCrawled(page, 14);
  // Zweites Öffnen: Zähler sind neuer als beim ersten Öffnen (nicht aus dem ersten Öffnen zwischengespeichert)
  await expect.poll(optionAll, { timeout: 10_000, intervals: [500] }).toBeGreaterThan(first + 3);
  // Tastaturbedienung: Pfeil nach unten öffnet die Liste, Esc schließt, der Fokus bleibt im Feld
  await combo.focus();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("listbox")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("listbox")).toHaveCount(0);
  expect(await focusIs(combo), "Fokus bleibt im Filterfeld").toBe(true);

  await pauseCrawl(page);
  const total = await coreTotal(page, "Response Codes", "All");
  await expect.poll(optionAll, { timeout: 12_000, intervals: [500] }).toBe(total);
  expect(await allCount()).toBe(total);
  expect(await activeChipCount(page)).toBe(total);
  expect((await footer(page)).total).toBe(total);
});

// spec: UI-032
// spec: UI-030
// spec: UI-018
// acceptance: independent
test("the View and Filter lists of the Inlinks table apply during the crawl and the table stays correct afterwards", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startSlow(alpha, 3);
  await waitCrawled(page, 5);
  await clickUpperRow(page, alpha.startUrl);
  const panel = await openDetailTab(page, "Inlinks");
  await expect.poll(async () => (await readDetail(page))?.rowcount ?? 0, { timeout: 10_000 }).toBeGreaterThan(2);

  // „View“ → Anchors: aggregierte Ankertexte (UI-034) – alle Inlinks der Startseite haben den Ankertext „Home“
  const select = (label: string) => panel.locator(".os-field").filter({ has: page.locator(".os-label", { hasText: new RegExp(`^${label}$`) }) }).getByRole("button");
  await select("View").click();
  await page.getByRole("option", { name: "Anchors", exact: true }).click();
  await expect.poll(async () => {
    const d = await readDetail(page);
    return d ? d.headers.join("|") : "";
  }, { timeout: 5000 }).toContain("Anchor Text");
  await expect.poll(async () => JSON.stringify((await readDetail(page))?.rows.map((r) => r.cells[0])), { timeout: 5000 }).toBe(JSON.stringify(["Home"]));
  // … und zurück zu „All links“: Linkzeilen mit From/To, die Zeilenzahl wächst weiter
  await select("View").click();
  await page.getByRole("option", { name: "All links", exact: true }).click();
  await expect.poll(async () => (await readDetail(page))?.headers.includes("From"), { timeout: 5000 }).toBe(true);
  const rows0 = (await readDetail(page))!.rowcount;
  await expect.poll(async () => (await readDetail(page))!.rowcount, { timeout: 10_000 }).toBeGreaterThan(rows0);

  // Filter „Hyperlinks only“: alle Links der Startseite sind Hyperlinks, Zahl bleibt gleich und wächst weiter
  await select("Filter").click();
  await page.getByRole("option", { name: "Hyperlinks only", exact: true }).click();
  const rows1 = (await readDetail(page))!.rowcount;
  await expect.poll(async () => (await readDetail(page))!.rowcount, { timeout: 10_000 }).toBeGreaterThan(rows1 - 1);
  await alpha.waitCompleted(60_000);
  await expect.poll(async () => ((await readDetail(page))?.rowcount ?? 0) - 1, { timeout: 8000 }).toBe(20);
  // Die Listen zeigen nach dem Schließen die gewählten Werte (Zustand bleibt erhalten)
  await expect(select("Filter")).toContainText("Hyperlinks only");
  await expect(select("View")).toContainText("All links");
});

async function crawlPathFlow(alpha: Alpha, live: boolean): Promise<void> {
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startSlow(alpha, live ? 2 : 12);
  if (live) await waitCrawled(page, 7);
  else await alpha.waitCompleted(60_000);
  const base = alpha.site.base;
  const opener = page.getByRole("region", { name: "Details", exact: true }).getByRole("button", { name: "Crawl Path", exact: true });
  await expect(opener, "ohne Auswahl ist der Crawl-Pfad nicht verfügbar").toBeDisabled();
  await clickUpperRow(page, `${base}/page-3.html`);
  await expect(opener).toBeEnabled();
  const dialog = page.getByRole("dialog", { name: "Crawl Path", exact: true });
  const pathOf = async () => {
    const grid = await readGrid(dialog);
    return grid ? { rowcount: grid.rowcount, addresses: grid.rows.map((r) => r.cells[grid.headers.indexOf("Address")]) } : null;
  };
  await opener.focus();
  await page.keyboard.press("Enter");
  await expect(dialog).toBeVisible();
  expect(await focusInside(dialog), "Fokus liegt im Dialog").toBe(true);
  // Crawl-Pfad von page-3: Startseite (Tiefe 0) → page-3 (Tiefe 1); Länge der Liste − 1 = Crawl Depth (UI-050)
  await expect.poll(pathOf, { timeout: 8000 }).toEqual({ rowcount: 3, addresses: [`${base}/`, `${base}/page-3.html`] });
  await expect(dialog.getByRole("alert"), "keine Fehlermeldung im Dialog").toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  expect(await focusIs(opener), "Fokus kehrt zum Auslöser zurück").toBe(true);

  // andere Auswahl, erneut öffnen: Pfad der aktuellen Auswahl, nicht der vorigen
  await upperGrid(page).focus();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect.poll(() => selectedUpper(page)).toEqual([`${base}/page-5.html`]);
  await opener.focus();
  await page.keyboard.press("Enter");
  await expect(dialog).toBeVisible();
  await expect.poll(pathOf, { timeout: 8000 }).toEqual({ rowcount: 3, addresses: [`${base}/`, `${base}/page-5.html`] });
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  await expect(dialog).toHaveCount(0);
}

// spec: UI-050
// spec: UI-126
// acceptance: independent
test("the Crawl Path dialog of a finished crawl shows the path of the current selection each time it is opened and returns the focus", async ({ alpha }) => {
  test.setTimeout(120_000);
  await crawlPathFlow(alpha, false);
});

// spec: UI-050
// spec: UI-126
// spec: UI-018
// acceptance: independent
test("the Crawl Path dialog works during a running crawl", async ({ alpha }) => {
  test.setTimeout(120_000);
  await crawlPathFlow(alpha, true);
  expect(await phase(alpha.page)).toBe("running");
});

// spec: UI-050
// spec: UI-126
// spec: UI-018
// acceptance: independent
test("the Crawl Path dialog opens and closes with the keyboard and is built anew for each selection (row count = depth + 1)", async ({ alpha }) => {
  test.setTimeout(120_000);
  const { page } = alpha;
  await page.setViewportSize(TALL);
  await startSlow(alpha, 2);
  await waitCrawled(page, 7);
  const opener = page.getByRole("region", { name: "Details", exact: true }).getByRole("button", { name: "Crawl Path", exact: true });
  const dialog = page.getByRole("dialog", { name: "Crawl Path", exact: true });
  // Die Zeilenzahl kommt aus aria-rowcount: sie hängt nicht von der Höhe der Zeichenfläche ab.
  const rowcount = async () => (await readGrid(dialog))?.rowcount ?? -1;
  await clickUpperRow(page, alpha.startUrl);
  await opener.focus();
  await page.keyboard.press("Enter");
  await expect(dialog).toBeVisible();
  expect(await focusInside(dialog)).toBe(true);
  await expect.poll(rowcount, { timeout: 8000 }).toBe(2); // Startseite: Tiefe 0 → 1 Eintrag + Kopfzeile
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  expect(await focusIs(opener), "Fokus kehrt zum Auslöser zurück").toBe(true);
  await upperGrid(page).focus();
  await page.keyboard.press("ArrowDown");
  await expect.poll(() => selectedUpper(page)).toEqual([`${alpha.site.base}/page-1.html`]);
  await opener.focus();
  await page.keyboard.press("Enter");
  await expect(dialog).toBeVisible();
  await expect.poll(rowcount, { timeout: 8000 }).toBe(3); // page-1: Tiefe 1 → 2 Einträge + Kopfzeile
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(await phase(page)).toBe("running");
});

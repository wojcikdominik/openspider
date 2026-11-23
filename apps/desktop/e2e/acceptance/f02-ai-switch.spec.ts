// Unabhängige Abnahme M4-F02: Schalter „KI (Vorschau)“ und KI-Oberfläche (Testagent; Grundlage allein Spec und Plan).
// acceptance: independent
//
// Quellen: docs/spec/05-ui.md UI-004 ([M4] Tabs AI, AI Decisions, AI Suggestions, AI Search), UI-013 (Statusleistenfeld
// `Kosten`, Feldname „Kosten:“ / „Cost:“), UI-016 (Leerzustände, Codes und Vorrang), docs/spec/schemas/settings.schema.json
// (`ai_app.preview_enabled`, Standard aus), docs/plan/M4.md E1 (E-2 b) und 3.3 (Tabs, Feld Kosten, Einstellungsabschnitt
// „KI“ und Konfigurationsseiten nur mit eingeschaltetem Schalter). Läuft gegen den echten Kern (os-app-bridge,
// OS_E2E_TRANSPORT=bridge); jeder Test hat ein eigenes, leeres Home (freshState).
import type { Page, WebSocket } from "@playwright/test";
import { expect, test, type AppFixture } from "../fixtures/app";

test.use({ freshState: true });

const AI_TABS = ["AI", "AI Decisions", "AI Suggestions", "AI Search"] as const;
// UI-016: abschließende Codes. Ohne Crawl gilt NO_DATA_YET (Rang 8); NO_MATCH (9) und NOT_APPLICABLE (10) haben einen
// größeren Rang und dürfen dann nicht erscheinen, NO_SELECTION betrifft nur Detail-Tabs.
const ALLOWED_WITHOUT_CRAWL = new Set([
  "MODE_UNSUPPORTED", "NEEDS_JS_RENDERING", "FEATURE_DISABLED", "HTML_NOT_STORED", "RENDERED_HTML_NOT_STORED",
  "API_NOT_CONNECTED", "NEEDS_CRAWL_ANALYSIS", "NO_DATA_YET",
]);
const SWITCH = /^(KI \(Vorschau\)|AI \(preview\))$/;
const COST = /^(Kosten|Cost):/;

async function language(page: Page, lang: "de" | "en") {
  await page.evaluate(async (l) => {
    const path = "/src/i18n/index.ts";
    await (await import(/* @vite-ignore */ path)).setLanguage(l);
  }, lang);
}

/** Zeichnet die über die Bridge gesendeten Commands auf (Websocket-Frames). */
function recordCommands(page: Page): string[] {
  const sent: string[] = [];
  page.on("websocket", (ws: WebSocket) => ws.on("framesent", (e) => sent.push(String(e.payload))));
  return sent;
}
const aiCommandsIn = (frames: string[]) => frames.filter((f) => /"ai\.[A-Za-z.]+"/.test(f));

async function openMain(page: Page, app: AppFixture) {
  await app.open(page, "main");
  await expect(page.locator(".u10-top-tabs [role=tab]").first()).toBeVisible({ timeout: 20_000 });
  await language(page, "en");
}

async function openSettings(page: Page, app: AppFixture) {
  await app.open(page, "settings", { "": "" });
  await expect(page.getByRole("tab").first()).toBeVisible({ timeout: 20_000 });
}

/** Alle Top-Tabs über die Gruppen der Navigationsleiste (UI-004, K2-77): Name → Gruppe. */
async function topTabs(page: Page): Promise<Map<string, string>> {
  const found = new Map<string, string>();
  const groups = page.locator(".u34-nav [data-group]");
  const ids = await groups.evaluateAll((els) => els.map((e) => e.getAttribute("data-group") ?? ""));
  for (const id of ids) {
    if (id === "overview" || id === "config") continue;
    const button = page.locator(`.u34-nav [data-group="${id}"] .u34-nav-button`);
    if (await button.isDisabled()) continue;
    await button.click();
    for (const text of await page.locator(".u10-top-tabs [role=tab]").allInnerTexts()) found.set(text.split("\n")[0].trim(), id);
  }
  return found;
}

/** Einträge des Menüs „Konfiguration ▾“. */
async function configEntries(page: Page): Promise<string[]> {
  await page.locator('.u34-nav [data-group="config"] .u34-nav-button').click();
  const menu = page.getByRole("menu");
  await expect(menu).toBeVisible();
  const entries = (await menu.getByRole("menuitem").allInnerTexts()).map((s) => s.trim());
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  return entries;
}

/** Seiten in der Navigation des Konfigurationsdialogs (geöffnet über den ersten Eintrag von „Konfiguration ▾“). */
async function configDialogPages(page: Page): Promise<string[]> {
  await page.locator('.u34-nav [data-group="config"] .u34-nav-button').click();
  await page.getByRole("menu").getByRole("menuitem").first().click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: 10_000 });
  const pages = (await dialog.locator(".os-config-nav .os-config-item").allInnerTexts()).map((s) => s.trim());
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  return pages;
}

async function settingsTabs(page: Page): Promise<string[]> {
  return (await page.getByRole("tab").allInnerTexts()).map((s) => s.trim());
}

async function setSwitch(settings: Page, on: boolean) {
  await settings.getByRole("tab", { name: /^(General|Allgemein)$/ }).click();
  const sw = settings.getByRole("switch", { name: SWITCH });
  await expect(sw).toHaveCount(1);
  if ((await sw.isChecked()) !== on) await settings.getByText(SWITCH).click();
  if (on) await expect(sw).toBeChecked();
  else await expect(sw).not.toBeChecked();
}

// spec: UI-004 [M4]
// spec: UI-013 [M4]
// acceptance: independent
test("switch off (default): no AI tabs, no cost field, no AI configuration pages, no AI commands sent", async ({ page, app }) => {
  test.setTimeout(120_000);
  expect(app.mode, "Abnahme läuft gegen den echten Kern (OS_E2E_TRANSPORT=bridge)").toBe("bridge");
  const frames = recordCommands(page);
  await openMain(page, app);
  const tabs = await topTabs(page);
  expect(tabs.has("Internal"), `Top-Tabs: ${[...tabs.keys()]}`).toBe(true);
  for (const tab of AI_TABS) expect(tabs.has(tab), `${tab} bei Schalter aus: ${[...tabs.keys()]}`).toBe(false);
  // Auch die ▾-Listen der Gruppen (alle Tabs einer Gruppe, sichtbar und ausgeblendet) nennen keinen KI-Tab.
  for (const chevron of await page.locator(".u34-nav .u34-nav-chevron").all()) {
    await chevron.click();
    const items = (await page.getByRole("menu").getByRole("menuitem").allInnerTexts()).map((s) => s.trim());
    for (const tab of AI_TABS) expect(items, "Gruppenliste").not.toContain(tab);
    await page.keyboard.press("Escape");
  }
  const config = await configEntries(page);
  expect(config.filter((e) => /(^|: )(AI|KI)$/.test(e)), `Konfiguration ▾: ${config}`).toEqual([]);
  const dialog = await configDialogPages(page);
  expect(dialog.filter((e) => /^(AI|KI)$/.test(e)), `Konfigurationsdialog: ${dialog}`).toEqual([]);
  await expect(page.getByText(COST)).toHaveCount(0);
  await language(page, "de");
  await expect(page.getByText(COST)).toHaveCount(0);
  expect(aiCommandsIn(frames), "KI-Commands bei Schalter aus").toEqual([]);
});

// spec: UI-004 [M4]
// acceptance: independent
test("switch off (default): settings have no AI section, the switch „KI (Vorschau)“ is off on General", async ({ page, app }) => {
  test.setTimeout(60_000);
  expect(app.mode).toBe("bridge");
  const frames = recordCommands(page);
  await openSettings(page, app);
  await language(page, "de");
  expect(await settingsTabs(page), "Einstellungsabschnitte").not.toContain("KI");
  await page.getByRole("tab", { name: "Allgemein", exact: true }).click();
  const sw = page.getByRole("switch", { name: "KI (Vorschau)", exact: true });
  await expect(sw).toHaveCount(1);
  await expect(sw).not.toBeChecked();
  await language(page, "en");
  expect(await settingsTabs(page), "Settings sections").not.toContain("AI");
  expect(aiCommandsIn(frames), "KI-Commands bei Schalter aus").toEqual([]);
});

// spec: UI-004 [M4]
// spec: UI-013 [M4]
// spec: UI-016
// acceptance: independent
test("switch on: four AI tabs with an empty state, AI settings section, AI configuration page, cost field; off again removes them", async ({ page, context, app }) => {
  test.setTimeout(240_000);
  expect(app.mode).toBe("bridge");
  // Die Teilkriterien sind unabhängig: weiche Prüfungen (expect.soft), damit ein roter Teil die übrigen nicht verdeckt.
  await openMain(page, app);
  const menuOff = await configEntries(page);
  const dialogOff = await configDialogPages(page);
  const tabsOff = await topTabs(page);

  const settings = await context.newPage();
  await openSettings(settings, app);
  const sectionsOff = await settingsTabs(settings);
  await setSwitch(settings, true);
  // Persistiert (Neustart des Fensters)
  await settings.reload();
  await expect(settings.getByRole("tab").first()).toBeVisible({ timeout: 20_000 });
  await language(settings, "de");
  const sectionsOn = await settingsTabs(settings);
  expect.soft(sectionsOn, "Einstellungsabschnitt „KI“").toContain("KI");
  expect.soft(sectionsOn.length, "genau ein zusätzlicher Abschnitt").toBe(sectionsOff.length + 1);
  if (sectionsOn.includes("KI")) {
    await settings.getByRole("tab", { name: "KI", exact: true }).click();
    await expect.soft(settings.getByRole("tabpanel")).toBeVisible();
  }

  await page.reload();
  await expect(page.locator(".u10-top-tabs [role=tab]").first()).toBeVisible({ timeout: 20_000 });
  await language(page, "en");
  const tabsOn = await topTabs(page);
  for (const tab of AI_TABS) expect.soft(tabsOn.has(tab), `${tab} fehlt bei Schalter an: ${[...tabsOn.keys()]}`).toBe(true);
  // Alle übrigen Tabs bleiben erhalten.
  for (const tab of tabsOff.keys()) expect.soft(tabsOn.has(tab), tab).toBe(true);
  expect.soft(tabsOn.size).toBe(tabsOff.size + AI_TABS.length);

  // UI-016: jeder KI-Tab zeigt ohne Crawl einen Leerzustand mit genau einem Code aus der Liste.
  for (const tab of AI_TABS) {
    if (!tabsOn.has(tab)) continue;
    await page.locator(`.u34-nav [data-group="${tabsOn.get(tab)}"] .u34-nav-button`).click();
    await page.locator(".u10-top-tabs").getByRole("tab", { name: tab, exact: true }).click();
    await expect(page.locator(".u10-top-tabs").getByRole("tab", { name: tab, exact: true })).toHaveAttribute("aria-selected", "true");
    const empty = page.locator(".u10-grid-host [data-empty-state]");
    await expect.soft(empty, `${tab}: Leerzustand`).toHaveCount(1, { timeout: 15_000 });
    if ((await empty.count()) !== 1) continue;
    await expect.soft(empty).toBeVisible();
    const code = (await empty.getAttribute("data-empty-state")) ?? "";
    expect.soft(ALLOWED_WITHOUT_CRAWL.has(code.replace(/\(.*\)$/, "")), `${tab}: Code ${code}`).toBe(true);
  }

  // Konfigurationsseiten: im Dialog und im Einstieg „Konfiguration ▾“ kommen Einträge dazu, alle bisherigen bleiben.
  const dialogOn = await configDialogPages(page);
  for (const entry of dialogOff) expect.soft(dialogOn, entry).toContain(entry);
  expect.soft(dialogOn.length, `Konfigurationsdialog an: ${dialogOn}`).toBeGreaterThan(dialogOff.length);
  const menuOn = await configEntries(page);
  for (const entry of menuOff) expect.soft(menuOn, entry).toContain(entry);
  expect.soft(menuOn.length, `Konfiguration ▾ an: ${menuOn}`).toBeGreaterThan(menuOff.length);

  // UI-013: Feld „Kosten“ – Feldname je Sprache.
  await expect.soft(page.getByText(/^Cost:/)).toHaveCount(1);
  await language(page, "de");
  await expect.soft(page.getByText(/^Kosten:/)).toHaveCount(1);
  await language(page, "en");

  // Ausschalten entfernt die KI-Oberfläche wieder.
  await setSwitch(settings, false);
  await settings.reload();
  await expect(settings.getByRole("tab").first()).toBeVisible({ timeout: 20_000 });
  await language(settings, "de");
  expect.soft(await settingsTabs(settings)).not.toContain("KI");
  expect.soft((await settingsTabs(settings)).length).toBe(sectionsOff.length);
  await page.reload();
  await expect(page.locator(".u10-top-tabs [role=tab]").first()).toBeVisible({ timeout: 20_000 });
  await language(page, "en");
  const tabsAgain = await topTabs(page);
  for (const tab of AI_TABS) expect.soft(tabsAgain.has(tab), `${tab} nach dem Ausschalten`).toBe(false);
  expect.soft([...tabsAgain.keys()].sort()).toEqual([...tabsOff.keys()].sort());
  expect.soft(await configDialogPages(page)).toEqual(dialogOff);
  expect.soft(await configEntries(page)).toEqual(menuOff);
  await expect.soft(page.getByText(COST)).toHaveCount(0);
});

// spec: UI-004 [M4]
// spec: UI-013 [M4]
// acceptance: independent
test("toggling the switch in the settings window updates the open main window without reload", async ({ page, context, app }) => {
  test.setTimeout(120_000);
  expect(app.mode).toBe("bridge");
  await openMain(page, app);
  const settings = await context.newPage();
  await openSettings(settings, app);

  await setSwitch(settings, true);
  await expect.poll(async () => { const tabs = await topTabs(page); return AI_TABS.filter((t) => tabs.has(t)).length; }, { timeout: 15_000 }).toBe(4);
  await expect(page.getByText(COST)).toHaveCount(1);

  await setSwitch(settings, false);
  await expect.configure({ soft: true }).poll(async () => { const tabs = await topTabs(page); return AI_TABS.filter((t) => tabs.has(t)).length; }, { timeout: 15_000 }).toBe(0);
  await expect.soft(page.getByText(COST)).toHaveCount(0);
});

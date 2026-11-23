// M4-F02: Schalter „KI (Vorschau)“ (Plan M4 E1, Nutzerentscheidung E-2 b) gegen die echte Bridge (fx-ui-basic, Loopback, kein
// KI-Anbieter). Ohne Schalter (Standard) erscheint keine KI-Oberfläche: keine KI-Tabs in Tab-Leiste und Gruppenmenü, keine KI-Filter
// in den Zählern und der Übersicht, keine KI-Exporte im Exportkatalog, kein Feld Kosten, kein Einstellungsabschnitt KI. Eingeschaltet
// in den Einstellungen erscheinen KI-Tabs (Leerzustand), Abschnitt KI, KI-Konfigurationsseite und das Feld Kosten (Platzhalter).
// Oberfläche Englisch (Standard der Alpha-Fixture); die deutschen Texte prüfen Vitest und `polish/language.spec.ts`.
import type { Page } from "@playwright/test";
import { expect, test, type Alpha } from "../alpha/fixture";

const AI_TABS = ["AI", "AI Decisions", "AI Suggestions", "AI Search"];
const isAi = (name: string) => AI_TABS.some((tab) => name === tab || name.startsWith(`${tab}:`));

interface ExportCatalog { bulk: { items: { name: string }[] }[]; reports: { items: { name: string }[] }[] }

async function catalogIds(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const path = "/src/state/tabs.ts";
    const { useTabs } = await import(/* @vite-ignore */ path);
    return (useTabs.getState().catalog as { id: string }[]).map((t) => t.id);
  });
}

/** Alle Namen, die der Kern dem Frontend als Tabs, Filter, Exporte, Reports und Kennzahlen liefert. */
async function listedNames(alpha: Alpha): Promise<string[]> {
  const tabs = await alpha.invoke<{ id: string; filters: { id: string }[] }[]>("catalog.tabs");
  const counts = await alpha.invoke<{ filters: { tab: string }[] }>("catalog.filterCounts", {});
  const overview = await alpha.invoke<{ tabs: { tab: string }[] }>("sidebar.overview", {});
  const exports = await alpha.invoke<ExportCatalog>("export.catalog");
  const metrics = await alpha.invoke<{ id: string }[]>("export.customSummaryMetrics");
  return [
    ...tabs.map((t) => t.id),
    ...counts.filters.map((r) => r.tab),
    ...overview.tabs.map((t) => t.tab),
    ...[...exports.bulk, ...exports.reports].flatMap((g) => g.items.map((i) => i.name)),
    ...metrics.map((m) => m.id),
  ];
}

async function customGroupLabels(page: Page): Promise<string[]> {
  await page.locator('.u34-nav [data-group="custom"] .u34-nav-button').click();
  const bar = page.getByRole("tablist", { name: /^(Data tabs|Daten-Tabs)$/ });
  await expect(bar.getByRole("tab").first()).toBeVisible();
  return bar.getByRole("tab").allInnerTexts();
}

/** Einträge des Menüs „Konfiguration ▾“ der Navigationsleiste (Plan M4 3.3 (6)). */
async function configMenu(page: Page): Promise<string[]> {
  await page.locator('.u34-nav [data-group="config"] .u34-nav-button').click();
  const menu = page.getByRole("menu");
  await expect(menu).toBeVisible();
  const entries = (await menu.getByRole("menuitem").allInnerTexts()).map((s) => s.trim());
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  return entries;
}

// spec: UI-004 [M4]
// spec: UI-013 [M4]
test("ohne Schalter KI (Vorschau) erscheint keine KI-Oberfläche, eingeschaltet die Gerüste", async ({ alpha }) => {
  const { page } = alpha;
  await alpha.open();
  await alpha.startFromHeader();
  await alpha.waitCompleted();

  // Schalter aus (Standard): weder im Kern noch in der Oberfläche KI-Einträge.
  const off = await listedNames(alpha);
  expect(off.filter(isAi)).toEqual([]);
  expect(off).toContain("Internal");
  expect((await catalogIds(page)).filter(isAi)).toEqual([]);
  const labelsOff = await customGroupLabels(page);
  for (const label of AI_TABS) expect(labelsOff).not.toContain(label);
  await expect(page.getByTestId("ai-cost")).toHaveCount(0);
  await expect(page.getByText(/^Cost:/)).toHaveCount(0);
  const menuOff = await configMenu(page);
  expect(menuOff).not.toContain("AI");

  const settings = await alpha.openSettings();
  const nav = settings.getByRole("tablist", { name: "Settings sections" });
  await expect(nav.getByRole("tab").first()).toBeVisible();
  await expect(nav.getByRole("tab", { name: "AI", exact: true })).toHaveCount(0);
  const toggle = settings.getByRole("switch", { name: "AI (preview)" });
  await expect(toggle).not.toBeChecked();

  // Einschalten in den Einstellungen: wirkt ohne Neustart in beiden Fenstern.
  // React Aria: das Eingabeelement ist visuell versteckt; geklickt wird die sichtbare Beschriftung (wie update.spec.ts).
  const label = settings.getByText("AI (preview)", { exact: true });
  await label.click();
  await expect(toggle).toBeChecked();
  await expect(nav.getByRole("tab", { name: "AI", exact: true })).toBeVisible();
  await nav.getByRole("tab", { name: "AI", exact: true }).click();
  await expect(settings.locator('[data-settings-section="ai"] [data-empty-state]')).toBeVisible();

  await expect.poll(async () => (await catalogIds(page)).filter(isAi).sort()).toEqual([...AI_TABS].sort());
  const on = await listedNames(alpha);
  for (const tab of AI_TABS) expect(on).toContain(tab);
  expect(on.some((n) => n.startsWith("AI:"))).toBe(true);
  await expect(page.getByTestId("ai-cost")).toHaveText("Cost: – / – USD");
  const labelsOn = await customGroupLabels(page);
  for (const label of AI_TABS) expect(labelsOn).toContain(label);
  // Plan M4 3.3 (6): die KI-Seite steht mit Schalter auch im Menü „Konfiguration ▾“, alle bisherigen Einträge bleiben.
  const menuOn = await configMenu(page);
  expect(menuOn).toContain("AI");
  expect(menuOn.filter((e) => e !== "AI")).toEqual(menuOff);
  // Ein KI-Tab ist erreichbar und leer (Stub ohne Zeilen bis zum Fachpaket; Leerzustand bzw. Zähler 0, UI-016/UI-014).
  const aiTab = page.getByRole("tablist", { name: /^(Data tabs|Daten-Tabs)$/ }).getByRole("tab", { name: "AI", exact: true });
  await aiTab.click();
  await expect(aiTab).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#data-panel .u10-counter").first()).toHaveText(/^Showing: 0 of /);

  // Wieder aus: die KI-Oberfläche verschwindet erneut. Der Schalter liegt auf „General“; oben wurde der Abschnitt „AI“ gewählt,
  // und das Einstellungsfenster zeigt nur den gewählten Abschnitt – also zurück nach „General“.
  await nav.getByRole("tab", { name: "General", exact: true }).click();
  await label.click();
  await expect(toggle).not.toBeChecked();
  await expect(nav.getByRole("tab", { name: "AI", exact: true })).toHaveCount(0);
  await expect.poll(async () => (await catalogIds(page)).filter(isAi)).toEqual([]);
  await expect(page.getByTestId("ai-cost")).toHaveCount(0);
  expect((await listedNames(alpha)).filter(isAi)).toEqual([]);
  // Plan M4 3.3 (5): ohne Neuladen verschwinden die KI-Tabs auch aus der Tab-Leiste des Hauptfensters – während in der
  // Navigation geklickt wird und obwohl zuvor ein KI-Tab gewählt war (Rückfall auf einen vorhandenen Tab).
  await expect
    .poll(async () => (await customGroupLabels(page)).filter((l) => AI_TABS.includes(l)), { timeout: 15_000 })
    .toEqual([]);
  await expect(page.getByRole("tablist", { name: /^(Data tabs|Daten-Tabs)$/ }).getByRole("tab", { selected: true })).toHaveCount(1);
  expect(await configMenu(page)).toEqual(menuOff);
});

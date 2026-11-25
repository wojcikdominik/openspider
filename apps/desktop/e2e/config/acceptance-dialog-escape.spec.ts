// spec: UI-015
// acceptance: independent
//
// Unabhängige Abnahme am Konfigurationsdialog (eigenes Overlay) mit der Rückfrage „Reset default?“ der Profilseite
// (Baustein-Dialog darüber): Escape schließt immer nur den obersten Dialog, auch wenn der Fokus bzw. das Ziel der
// Taste noch im Konfigurationsdialog liegt; ein Escape, das das Suchfeld schon verarbeitet hat, schließt nichts.
import { expect, test, type Locator, type Page } from "@playwright/test";
import type {} from "./harness";

test.beforeEach(async ({ page }) => {
  await page.clock.install({ time: new Date("2026-10-20T12:00:00Z") });
  await page.goto("/e2e/config/index.html?transport=mock");
  await page.waitForFunction(() => !!window.cfgFixture);
});

const config = (page: Page) => page.getByRole("dialog", { name: "Crawl configuration" });
const question = (page: Page) => page.getByRole("dialog", { name: "Reset default?" });
const trigger = (page: Page) => page.getByRole("button", { name: "Open configuration" });
const frames = (page: Page) => page.evaluate(() => new Promise<void>(r => requestAnimationFrame(() => requestAnimationFrame(() => r()))));
async function freeze(page: Page) {
  const now = await page.evaluate(() => Date.now());
  await page.clock.pauseAt(now + 1_000);
}
const virtualClick = (el: Locator) => el.evaluate(node => (node as HTMLElement).click());
const escapeAt = (el: Locator) => el.evaluate(node => {
  node.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", keyCode: 27, bubbles: true, cancelable: true }));
});
const configWrites = (page: Page) => page.evaluate(() => window.cfgFixture.calls().filter(c => c.cmd === "config.set").length);

/** Opens the configuration by keyboard from its trigger and prepares a default profile; returns the "Reset default" button. */
async function prepare(page: Page) {
  await trigger(page).focus();
  await page.keyboard.press("Enter");
  const dialog = config(page);
  await expect(dialog).toBeVisible();
  const nav = dialog.getByRole("navigation", { name: "Configuration pages" });
  await nav.getByRole("button", { name: "Profiles", exact: true }).click();
  await dialog.getByRole("textbox", { name: "Profile name" }).fill("small");
  await dialog.getByText("Also make it the default", { exact: true }).click();
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog.locator('tr[data-profile="small"]')).toHaveAttribute("data-default", "true");
  return dialog.getByRole("button", { name: "Reset default" });
}

// spec: UI-015
// acceptance: independent
test("Reset default? with the focus inside: Escape closes only the question, then the configuration; focus step by step", async ({ page }) => {
  const reset = await prepare(page);
  await reset.click();
  await expect(question(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(question(page)).toHaveCount(0);
  await expect(config(page)).toBeVisible();
  await expect(reset).toBeFocused();
  await expect(config(page).locator('tr[data-profile="small"]')).toHaveAttribute("data-default", "true");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger(page)).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("Reset default? with the key target still in the configuration: Escape closes only the question (deterministic)", async ({ page }) => {
  const reset = await prepare(page);
  await reset.focus();
  await freeze(page);
  await virtualClick(reset);
  await expect(question(page)).toBeVisible();
  // The Escape a keyboard delivers while the focus is still on the configuration's button: its target is that button.
  await escapeAt(reset);
  await expect(question(page)).toHaveCount(0);
  await expect(config(page)).toBeVisible();
  await page.clock.resume();
  await frames(page);
  await expect(config(page)).toBeVisible();
  await expect(reset).toBeFocused();
  await expect(config(page).locator('tr[data-profile="small"]')).toHaveAttribute("data-default", "true");
});

// spec: UI-015
// acceptance: independent
test("Reset default? opened programmatically before the focus moved: keyboard Escape closes only the question", async ({ page }) => {
  const reset = await prepare(page);
  await reset.focus();
  await freeze(page);
  await virtualClick(reset);
  await expect(question(page)).toBeVisible();
  const where = await page.evaluate(() => {
    const all = [...document.querySelectorAll("[role='dialog']")];
    const index = all.findIndex(d => d.contains(document.activeElement));
    return index < 0 ? (document.activeElement === document.body ? "body" : "outside") : index === all.length - 1 ? "topmost" : "lower dialog";
  });
  console.log(`UI015-PROBE config-nested-gap focus=${where}`);
  await page.keyboard.press("Escape");
  await expect(question(page)).toHaveCount(0);
  await expect(config(page)).toBeVisible();
  await page.clock.resume();
  await frames(page);
  await expect(config(page)).toBeVisible();
  await expect(reset).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("Reset default?: Escape targeted at body closes only the question", async ({ page }) => {
  const reset = await prepare(page);
  await reset.click();
  await expect(question(page)).toBeVisible();
  await escapeAt(page.locator("body"));
  await expect(question(page)).toHaveCount(0);
  await expect(config(page)).toBeVisible();
});

// spec: UI-015
// acceptance: independent
test("configuration: Escape with the key target on body closes it without writing; focus returns to the trigger", async ({ page }) => {
  await trigger(page).focus();
  await freeze(page);
  await virtualClick(trigger(page));
  await expect(config(page)).toBeVisible();
  await escapeAt(page.locator("body"));
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.clock.resume();
  await frames(page);
  await expect(trigger(page)).toBeFocused();
  expect(await configWrites(page)).toBe(0);
});

// spec: UI-015
// spec: UI-113
// acceptance: independent
test("configuration: Escape in the search field with text clears it and keeps the dialog; the next Escape closes it", async ({ page }) => {
  await trigger(page).focus();
  await page.keyboard.press("Enter");
  const dialog = config(page);
  const search = dialog.getByRole("searchbox", { name: "Search settings" });
  await search.fill("accept");
  await page.keyboard.press("Escape");
  await expect(search).toHaveValue("");
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger(page)).toBeFocused();
  expect(await configWrites(page)).toBe(0);
});

// spec: UI-015
// acceptance: independent
test("configuration: an open profile selection list keeps the first Escape; the second closes the dialog", async ({ page }) => {
  await trigger(page).focus();
  await page.keyboard.press("Enter");
  const dialog = config(page);
  await dialog.locator(".os-config-profile").getByRole("button").click();
  const list = page.getByRole("listbox");
  await expect(list).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(list).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger(page)).toBeFocused();
});

// spec: UI-015
// acceptance: independent
//
// Unabhängige Abnahme: Escape schließt den Exportdialog (Export options), sobald er sichtbar ist, auch bevor der Fokus
// in ihn gewandert ist; Fokus zurück zum Auslöser; Auswahlliste und laufender Vorgang behalten Escape bzw. den Dialog.
import { test, expect, type Locator, type Page } from "@playwright/test";
import type {} from "./harness";

const START = new Date("2026-10-18T12:00:00Z");

test.beforeEach(async ({ page }) => {
  await page.clock.install({ time: START });
  await page.goto("/e2e/export/index.html?transport=mock");
  await page.waitForFunction(() => !!window.exportFixture);
  await expect(page.getByRole("button", { name: "Sitemap", exact: true })).toBeVisible();
});

const exportButton = (page: Page) => page.getByRole("button", { name: "Export", exact: true }).first();
const dialog = (page: Page) => page.getByRole("dialog", { name: "Export options" });
/** Waits two animation frames of the page (frame sync, no fixed delay). */
const frames = (page: Page) => page.evaluate(() => new Promise<void>(r => requestAnimationFrame(() => requestAnimationFrame(() => r()))));
/** Stops the page clock (timers and animation frames) so React Aria cannot move the focus into the dialog yet. */
async function freeze(page: Page) {
  const now = await page.evaluate(() => Date.now());
  await page.clock.pauseAt(now + 1_000);
}
const focusInside = (page: Page) => page.evaluate(() => {
  const all = document.querySelectorAll("[role='dialog']");
  const top = all[all.length - 1];
  return !!top && top.contains(document.activeElement);
});
/** Activates `trigger` without key or pointer (as a screen reader or a programmatic click would). */
const virtualClick = (trigger: Locator) => trigger.evaluate(el => (el as HTMLElement).click());

// spec: UI-015
// acceptance: independent
test("Escape in the gap before the focus entered the dialog closes it; focus returns to the trigger and stays there", async ({ page }) => {
  const trigger = exportButton(page);
  await trigger.focus();
  await freeze(page);
  await virtualClick(trigger);
  await expect(dialog(page)).toBeVisible();
  expect(await focusInside(page), "precondition: focus has not entered the dialog yet").toBe(false);
  console.log(`UI015-PROBE export-gap active=${await page.evaluate(() => document.activeElement?.tagName + ":" + (document.activeElement?.textContent ?? "").slice(0, 20))}`);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await page.clock.resume();
  await frames(page);
  await expect(trigger).toBeFocused();
  expect(await page.evaluate(() => window.exportFixture.calls().filter(c => c.cmd.startsWith("export.view")))).toEqual([]);
});

// spec: UI-015
// acceptance: independent
test("Escape while the focus lies on body closes the open dialog and returns the focus to the trigger", async ({ page }) => {
  const trigger = exportButton(page);
  await trigger.click();
  await expect(dialog(page)).toBeVisible();
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

// spec: UI-015
// spec: UI-113
// acceptance: independent
test("Escape right after Mod+E closes the dialog; focus returns to the focused element", async ({ page }) => {
  const trigger = exportButton(page);
  await trigger.focus();
  const mod = await page.evaluate(() => window.exportFixture.platform === "mac" ? "Meta" : "Control");
  await page.keyboard.press(`${mod}+e`);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  // The same with a frozen page clock: the dialog is shown, React Aria has not run its frame yet.
  await freeze(page);
  await page.keyboard.press(`${mod}+e`);
  await expect(dialog(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  // Focus return may take one animation frame when the focus had entered the dialog: let the page run.
  await page.clock.resume();
  await frames(page);
  await expect(trigger).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("Escape right after opening with a mouse click closes the dialog; focus returns to the trigger", async ({ page }) => {
  const trigger = exportButton(page);
  await trigger.click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await freeze(page);
  await trigger.click();
  await expect(dialog(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.clock.resume();
  await frames(page);
  await expect(trigger).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("an open selection list keeps the first Escape; the second closes the dialog", async ({ page }) => {
  const trigger = exportButton(page);
  await trigger.click();
  const options = dialog(page);
  await options.getByRole("button", { name: /File format/ }).click();
  const list = page.getByRole("listbox");
  await expect(list).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(list).toHaveCount(0);
  await expect(options).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("two quick Escapes close the dialog once and leave the focus on the trigger", async ({ page }) => {
  const trigger = exportButton(page);
  await trigger.click();
  await expect(dialog(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await frames(page);
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

// spec: UI-015
// acceptance: independent
test("while the export is starting (running operation) Escape leaves the dialog open, inside and outside the dialog", async ({ page }) => {
  await page.evaluate(() => {
    const core = (window as unknown as { __osMock: { handlers: Map<string, () => unknown> } }).__osMock;
    core.handlers.set("export.view", () => new Promise(() => undefined));
  });
  await exportButton(page).click();
  const options = dialog(page);
  await options.getByRole("button", { name: "Save…" }).click();
  await expect.poll(() => page.evaluate(() => window.exportFixture.calls().filter(c => c.cmd === "export.view").length)).toBe(1);
  await expect(options.locator("[aria-busy='true']")).toHaveCount(1);
  await options.getByRole("heading", { name: "Export options" }).click();
  await page.keyboard.press("Escape");
  await expect(options).toBeVisible();
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press("Escape");
  await expect(options).toBeVisible();
});

// spec: UI-015
// spec: UI-113
// acceptance: independent
test("unchanged: Enter starts the export, Tab stays inside, Cancel returns the focus to the trigger", async ({ page }) => {
  const trigger = exportButton(page);
  await trigger.click();
  const options = dialog(page);
  await expect.poll(() => focusInside(page)).toBe(true);
  for (let i = 0; i < 12; i++) {
    await page.keyboard.press("Tab");
    expect(await focusInside(page), `Tab ${i + 1} stays inside the dialog`).toBe(true);
  }
  await options.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await trigger.click();
  await options.getByRole("checkbox", { name: /all columns/i }).focus();
  await page.keyboard.press("Enter");
  await expect.poll(() => page.evaluate(() => window.exportFixture.calls().filter(c => c.cmd === "export.view").length)).toBe(1);
});

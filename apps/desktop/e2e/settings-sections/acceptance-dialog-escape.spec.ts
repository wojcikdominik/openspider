// spec: UI-015
// acceptance: independent
//
// Unabhängige Abnahme: Einstellungen → Notifications (E-Mail) → „Add SMTP account“, geöffnet, während der Fokus noch im
// Einstellungsrahmen liegt. Escape schließt nur den Kontodialog; die Einstellungen bleiben, der Fokus kehrt zurück.
import { expect, test, type Locator, type Page } from "@playwright/test";
import type {} from "./harness";

test.beforeEach(async ({ page }) => {
  await page.clock.install({ time: new Date("2026-10-20T12:00:00Z") });
  await page.goto("/e2e/settings-sections/index.html?transport=mock&lang=en");
  await page.waitForFunction(() => !!window.settingsFixture);
  await page.getByRole("tab", { name: "Notifications", exact: true }).click();
  await expect(add(page)).toBeVisible();
});

const add = (page: Page) => page.getByRole("button", { name: "Add SMTP account", exact: true });
const account = (page: Page) => page.getByRole("dialog", { name: "Add SMTP account", exact: true });
const frames = (page: Page) => page.evaluate(() => new Promise<void>(r => requestAnimationFrame(() => requestAnimationFrame(() => r()))));
async function freeze(page: Page) {
  const now = await page.evaluate(() => Date.now());
  await page.clock.pauseAt(now + 1_000);
}
const virtualClick = (trigger: Locator) => trigger.evaluate(el => (el as HTMLElement).click());
const focusInDialog = (page: Page) => page.evaluate(() => {
  const all = document.querySelectorAll("[role='dialog']");
  const top = all[all.length - 1];
  return !!top && top.contains(document.activeElement);
});
const settingsTab = (page: Page) => page.getByRole("tab", { name: "Notifications", exact: true });

// spec: UI-015
// acceptance: independent
test("account dialog opened while the focus is still in the settings: keyboard Escape closes it, focus returns", async ({ page }) => {
  const trigger = add(page);
  await trigger.focus();
  await freeze(page);
  await virtualClick(trigger);
  await expect(account(page)).toBeVisible();
  expect(await focusInDialog(page), "precondition: focus has not entered the account dialog").toBe(false);
  await page.keyboard.press("Escape");
  await expect(account(page)).toHaveCount(0);
  await expect(settingsTab(page)).toBeVisible();
  await expect(trigger).toBeFocused();
  await page.clock.resume();
  await frames(page);
  await expect(trigger).toBeFocused();
  await expect(settingsTab(page)).toBeVisible();
});

// spec: UI-015
// acceptance: independent
test("account dialog with the key target still on the settings' Add button: Escape closes only the account dialog", async ({ page }) => {
  const trigger = add(page);
  await trigger.focus();
  await freeze(page);
  await virtualClick(trigger);
  await expect(account(page)).toBeVisible();
  // The Escape a keyboard delivers while the focus is still on the settings' button: its target is that button.
  await trigger.evaluate(el => {
    el.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", keyCode: 27, bubbles: true, cancelable: true }));
  });
  await expect(account(page)).toHaveCount(0);
  await page.clock.resume();
  await frames(page);
  await expect(settingsTab(page)).toBeVisible();
  await expect(trigger).toBeFocused();
  expect(await page.evaluate(() => window.settingsFixture.calls().filter(c => c.cmd.startsWith("mail.save")))).toEqual([]);
});

// spec: UI-015
// acceptance: independent
test("account dialog: Escape in a text field closes it; the settings stay and the focus returns to Add", async ({ page }) => {
  const trigger = add(page);
  await trigger.click();
  await expect(account(page)).toBeVisible();
  await account(page).getByRole("textbox", { name: "Account label" }).fill("Office");
  await page.keyboard.press("Escape");
  await expect(account(page)).toHaveCount(0);
  await expect(settingsTab(page)).toBeVisible();
  await expect(trigger).toBeFocused();
});

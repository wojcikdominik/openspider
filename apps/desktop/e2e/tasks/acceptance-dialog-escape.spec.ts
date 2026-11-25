// spec: UI-015
// acceptance: independent
//
// Unabhängige Abnahme an echten Dialogen aus Planung und Einstellungen: Scheduling (mit verschachtelter Rückfrage
// „Delete task“) und das E-Mail-Konto (Settings → Email). Escape schließt jeweils nur den obersten Dialog, auch bevor
// der Fokus in ihn gewandert ist, und gibt den Fokus an den Auslöser zurück.
import { test, expect, type Locator, type Page } from "@playwright/test";
import type {} from "./harness";

test.use({ timezoneId: "Europe/Berlin" });
test.beforeEach(async ({ page }) => {
  await page.clock.install({ time: new Date("2026-10-20T12:00:00Z") });
  await page.goto("/e2e/tasks/index.html");
  await page.waitForFunction(() => !!window.taskFixture);
});

const frames = (page: Page) => page.evaluate(() => new Promise<void>(r => requestAnimationFrame(() => requestAnimationFrame(() => r()))));
async function freeze(page: Page) {
  const now = await page.evaluate(() => Date.now());
  await page.clock.pauseAt(now + 1_000);
}
const virtualClick = (trigger: Locator) => trigger.evaluate(el => (el as HTMLElement).click());
/** Where the focus is: inside the topmost dialog, inside another dialog, or outside every dialog. */
const focusPlace = (page: Page) => page.evaluate(() => {
  const all = [...document.querySelectorAll("[role='dialog']")];
  const index = all.findIndex(d => d.contains(document.activeElement));
  if (index < 0) return document.activeElement === document.body ? "body" : "outside";
  return index === all.length - 1 ? "topmost" : "lower dialog";
});

const scheduling = (page: Page) => page.getByRole("dialog", { name: "Scheduling", exact: true });
const confirm = (page: Page) => page.getByRole("dialog", { name: "Delete task", exact: true });

async function openScheduling(page: Page) {
  const trigger = page.getByRole("button", { name: "Scheduling", exact: true });
  // The harness trigger is a plain <button>; WebKit moves the focus to body on a mouse click there, so open it by
  // keyboard (a test-page artefact, the app's own triggers are focusable React Aria buttons).
  await trigger.focus();
  await page.keyboard.press("Enter");
  await expect(scheduling(page)).toBeVisible();
  await scheduling(page).getByRole("checkbox", { name: "Select Nightly" }).check();
  return trigger;
}

// spec: UI-015
// acceptance: independent
test("Scheduling: Escape in the gap before the focus entered the dialog closes it and returns the focus", async ({ page }) => {
  const trigger = page.getByRole("button", { name: "Scheduling", exact: true });
  await trigger.focus();
  await freeze(page);
  await virtualClick(trigger);
  await expect(scheduling(page)).toBeVisible();
  expect(await focusPlace(page), "precondition: focus has not entered the dialog").not.toBe("topmost");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await page.clock.resume();
  await frames(page);
  await expect(trigger).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("nested question: Escape closes only the question, then the Scheduling dialog; focus returns step by step", async ({ page }) => {
  const trigger = await openScheduling(page);
  const del = scheduling(page).getByRole("button", { name: "Delete task", exact: true });
  await del.click();
  await expect(confirm(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(confirm(page)).toHaveCount(0);
  await expect(scheduling(page)).toBeVisible();
  await expect(del).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  expect(await page.evaluate(() => window.taskFixture.calls().filter(c => c.cmd === "tasks.delete"))).toEqual([]);
});

// spec: UI-015
// acceptance: independent
test("nested question with the focus on body: Escape closes only the question", async ({ page }) => {
  await openScheduling(page);
  const del = scheduling(page).getByRole("button", { name: "Delete task", exact: true });
  await del.click();
  await expect(confirm(page)).toBeVisible();
  await expect.poll(() => focusPlace(page), { message: "focus has entered the question" }).toBe("topmost");
  await freeze(page);
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  expect(await focusPlace(page), "precondition: focus lies on body").toBe("body");
  await page.keyboard.press("Escape");
  await expect(confirm(page)).toHaveCount(0);
  await expect(scheduling(page)).toBeVisible();
  await expect(del).toBeFocused();
  await page.clock.resume();
  await frames(page);
  await expect(del).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("nested question opening, focus not yet in it (body or outer dialog): Escape closes only the question", async ({ page }) => {
  await openScheduling(page);
  const del = scheduling(page).getByRole("button", { name: "Delete task", exact: true });
  await del.focus();
  await freeze(page);
  await virtualClick(del);
  await expect(confirm(page)).toBeVisible();
  const where = await focusPlace(page);
  test.info().annotations.push({ type: "focus-before-escape", description: where });
  console.log(`UI015-PROBE tasks-nested-gap focus=${where} active=${await page.evaluate(() => document.activeElement?.tagName + ":" + (document.activeElement?.textContent ?? "").slice(0, 20))}`);
  expect(["body", "lower dialog"], "precondition: focus has not entered the question").toContain(where);
  await page.keyboard.press("Escape");
  await expect(confirm(page)).toHaveCount(0);
  await expect(scheduling(page)).toBeVisible();
  await page.clock.resume();
  await frames(page);
  await expect(scheduling(page)).toBeVisible();
  await expect(del).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("nested question with the key target still in the outer dialog: Escape closes only the question (deterministic Restfall)", async ({ page }) => {
  await openScheduling(page);
  const del = scheduling(page).getByRole("button", { name: "Delete task", exact: true });
  await del.focus();
  await freeze(page);
  await virtualClick(del);
  await expect(confirm(page)).toBeVisible();
  // The Escape a keyboard delivers while the focus is still on the outer dialog's button: its target is that button.
  await del.evaluate(el => {
    el.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", keyCode: 27, bubbles: true, cancelable: true }));
  });
  await expect(confirm(page)).toHaveCount(0);
  await expect(scheduling(page)).toBeVisible();
  await page.clock.resume();
  await frames(page);
  await expect(scheduling(page)).toBeVisible();
  await expect(del).toBeFocused();
  expect(await page.evaluate(() => window.taskFixture.calls().filter(c => c.cmd === "tasks.delete"))).toEqual([]);
});

// spec: UI-015
// acceptance: independent
test("nested: two quick Escapes close the question and then the Scheduling dialog", async ({ page }) => {
  const trigger = await openScheduling(page);
  await scheduling(page).getByRole("button", { name: "Delete task", exact: true }).click();
  await expect(confirm(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

// spec: UI-015
// spec: UI-113
// acceptance: independent
test("unchanged: Enter on the destructive question does not delete; Cancel returns the focus to the Delete button", async ({ page }) => {
  await openScheduling(page);
  const del = scheduling(page).getByRole("button", { name: "Delete task", exact: true });
  await del.click();
  await expect(confirm(page)).toBeVisible();
  await confirm(page).getByRole("heading", { name: "Delete task" }).click();
  await page.keyboard.press("Enter");
  await expect(confirm(page)).toBeVisible();
  expect(await page.evaluate(() => window.taskFixture.calls().filter(c => c.cmd === "tasks.delete"))).toEqual([]);
  await confirm(page).getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(confirm(page)).toHaveCount(0);
  await expect(del).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("Settings → Email: Escape in a text field and with the focus on body closes the account dialog; focus returns", async ({ page }) => {
  const add = page.getByRole("button", { name: "Add SMTP account", exact: true });
  const account = page.getByRole("dialog", { name: "Add SMTP account", exact: true });
  await add.focus();
  await add.click();
  await expect(account).toBeVisible();
  await account.getByRole("textbox", { name: "Account label" }).fill("Office");
  await page.keyboard.press("Escape");
  await expect(account).toHaveCount(0);
  await expect(add).toBeFocused();
  await add.focus();
  await freeze(page);
  await virtualClick(add);
  await expect(account).toBeVisible();
  expect(await focusPlace(page), "precondition: focus has not entered the dialog").not.toBe("topmost");
  await page.keyboard.press("Escape");
  await expect(account).toHaveCount(0);
  await expect(add).toBeFocused();
});

// spec: UI-015
// acceptance: independent
//
// Unabhängige Abnahme an „Manage crawls…“ mit der Rückfrage „Delete“ vor dem Löschen und dem Kontextmenü der
// Crawl-Tabelle: Escape schließt nur das oberste offene Element, auch direkt nach dem Öffnen per Taste.
import { test, expect, type Locator, type Page } from "@playwright/test";
import type {} from "./harness";

test.beforeEach(async ({ page }) => {
  await page.clock.install({ time: new Date("2026-10-20T12:00:00Z") });
  await page.goto("/e2e/crawls/index.html");
  await page.waitForFunction(() => !!window.crawlsFixture);
});

const frames = (page: Page) => page.evaluate(() => new Promise<void>(r => requestAnimationFrame(() => requestAnimationFrame(() => r()))));
async function freeze(page: Page) {
  const now = await page.evaluate(() => Date.now());
  await page.clock.pauseAt(now + 1_000);
}
const virtualClick = (trigger: Locator) => trigger.evaluate(el => (el as HTMLElement).click());
const focusInTopmost = (page: Page) => page.evaluate(() => {
  const all = document.querySelectorAll("[role='dialog']");
  const top = all[all.length - 1];
  return !!top && top.contains(document.activeElement);
});
const manage = (page: Page) => page.getByRole("dialog", { name: "Manage crawls…", exact: true });
const question = (page: Page) => page.getByRole("dialog", { name: "Delete", exact: true });
const deletes = (page: Page) => page.evaluate(() => window.crawlsFixture.calls().filter(c => c.cmd === "crawls.delete"));

async function openManage(page: Page) {
  const trigger = page.getByRole("button", { name: "Manage", exact: true });
  await trigger.click();
  await expect(manage(page).getByRole("checkbox", { name: "Select: Alpha" })).toBeVisible();
  return trigger;
}

// spec: UI-015
// acceptance: independent
test("Manage crawls: Escape in the gap before the focus entered the dialog closes it and returns the focus", async ({ page }) => {
  const trigger = page.getByRole("button", { name: "Manage", exact: true });
  await trigger.focus();
  await freeze(page);
  await virtualClick(trigger);
  await expect(manage(page)).toBeVisible();
  expect(await focusInTopmost(page), "precondition: focus has not entered the dialog").toBe(false);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await page.clock.resume();
  await frames(page);
  await expect(trigger).toBeFocused();
});

// spec: UI-015
// spec: UI-113
// acceptance: independent
test("delete question opened with the Delete key: an immediate Escape closes only the question, nothing is deleted", async ({ page }) => {
  await openManage(page);
  const alpha = manage(page).getByRole("checkbox", { name: "Select: Alpha" });
  await alpha.check();
  await alpha.focus();
  await page.keyboard.press("Delete");
  await page.keyboard.press("Escape");
  await expect(question(page)).toHaveCount(0);
  await expect(manage(page)).toBeVisible();
  await expect(alpha).toBeFocused();
  expect(await deletes(page)).toEqual([]);
  // Again with a frozen page clock: the question is visible, React Aria has not run its frame yet.
  await freeze(page);
  await page.keyboard.press("Delete");
  await expect(question(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(question(page)).toHaveCount(0);
  await expect(manage(page)).toBeVisible();
  await expect(alpha).toBeFocused();
  await page.clock.resume();
  await frames(page);
  await expect(manage(page)).toBeVisible();
  expect(await deletes(page)).toEqual([]);
});

// spec: UI-015
// acceptance: independent
test("delete question opened by mouse with the focus moved to body: Escape closes only the question", async ({ page }) => {
  await openManage(page);
  await manage(page).getByRole("checkbox", { name: "Select: Alpha" }).check();
  const del = manage(page).getByRole("button", { name: "Delete", exact: true });
  await del.click();
  await expect(question(page)).toBeVisible();
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press("Escape");
  await expect(question(page)).toHaveCount(0);
  await expect(manage(page)).toBeVisible();
  await expect(del).toBeFocused();
  expect(await deletes(page)).toEqual([]);
});

// spec: UI-015
// acceptance: independent
test("delete question opening while the focus is still in Manage crawls: Escape closes only the question (Restfall)", async ({ page }) => {
  await openManage(page);
  await manage(page).getByRole("checkbox", { name: "Select: Alpha" }).check();
  const del = manage(page).getByRole("button", { name: "Delete", exact: true });
  await del.focus();
  await freeze(page);
  await virtualClick(del);
  await expect(question(page)).toBeVisible();
  expect(await focusInTopmost(page), "precondition: focus has not entered the question").toBe(false);
  await page.keyboard.press("Escape");
  await expect(question(page)).toHaveCount(0);
  await expect(manage(page)).toBeVisible();
  await page.clock.resume();
  await frames(page);
  await expect(manage(page)).toBeVisible();
  expect(await deletes(page)).toEqual([]);
});

// spec: UI-015
// acceptance: independent
test("an open context menu in Manage crawls keeps the first Escape; the second closes the dialog", async ({ page }) => {
  const trigger = await openManage(page);
  const alpha = manage(page).getByRole("checkbox", { name: "Select: Alpha" });
  await alpha.check();
  await alpha.focus();
  await page.keyboard.press("Shift+F10");
  const menu = page.getByRole("menu");
  await expect(menu).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(manage(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

// spec: UI-015
// acceptance: independent
test("delete question: two quick Escapes close the question and then Manage crawls", async ({ page }) => {
  const trigger = await openManage(page);
  const alpha = manage(page).getByRole("checkbox", { name: "Select: Alpha" });
  await alpha.check();
  await alpha.focus();
  await page.keyboard.press("Delete");
  await expect(question(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  expect(await deletes(page)).toEqual([]);
});

import { test, expect } from "@playwright/test";
import type {} from "./harness";

// UI-113 (OS-Festlegung, Orchestrator 09.11.2026): Enter is the default action of a modal dialog; a destructive OK
// (danger) only reacts to its own focused button; a field that handles Enter itself applies exactly once.
test.beforeEach(async ({ page }) => {
  await page.goto("/e2e/crawls/index.html");
  await page.getByRole("button", { name: "Manage", exact: true }).click();
  await expect(page.getByRole("checkbox", { name: "Select: Alpha" })).toBeVisible();
});
const calls = (page: import("@playwright/test").Page, cmd: string) =>
  page.evaluate(cmd => window.crawlsFixture.calls().filter(c => c.cmd === cmd).map(c => c.args), cmd);

// spec: UI-113
test("Enter does not confirm a destructive question unless its own button has the focus", async ({ page }) => {
  const alpha = page.getByRole("checkbox", { name: "Select: Alpha" }); await alpha.check(); await alpha.focus();
  await page.keyboard.press("Delete");
  const confirm = page.getByRole("dialog", { name: "Delete", exact: true });
  await expect(confirm).toBeVisible();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Enter");
  await expect(confirm).toBeVisible();
  expect(await calls(page, "crawls.delete")).toEqual([]);
  // The focused delete button itself: Enter activates it.
  await confirm.getByRole("button", { name: "Delete", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect.poll(() => calls(page, "crawls.delete")).toHaveLength(1);
});

// spec: UI-113
test("Enter in a form field applies once (field handler and dialog default do not both run)", async ({ page }) => {
  await page.getByRole("checkbox", { name: "Select: Alpha" }).check();
  await page.getByRole("button", { name: "Rename", exact: true }).click();
  const form = page.getByRole("dialog", { name: "Rename", exact: true });
  const field = form.getByRole("textbox", { name: "Name", exact: true });
  await expect(field).toBeFocused();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.type("Alpha neu", { delay: 20 });
  await page.keyboard.press("Enter");
  await expect(form).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(await calls(page, "crawls.rename")).toEqual([{ crawlId: "0199f000-0000-7000-8000-000000000001", name: "Alpha neu" }]);
});

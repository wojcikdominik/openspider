import { expect, test, type Page } from "@playwright/test";
import type {} from "./harness";

// UI-015: Escape closes the topmost dialog wherever the focus is. The crawl configuration is an overlay of its own (not the
// Dialog primitive); a confirmation opened above it must close on Escape even while the key still targets the configuration.
test.beforeEach(async ({ page }) => {
  await page.clock.install({ time: new Date("2026-10-20T12:00:00Z") });
  await page.goto("/e2e/config/index.html?transport=mock");
  await page.waitForFunction(() => !!window.cfgFixture);
});

const config = (page: Page) => page.getByRole("dialog", { name: "Crawl configuration" });
const confirm = (page: Page) => page.getByRole("dialog", { name: "Reset default?" });

// spec: UI-015
test("a confirmation above the configuration closes on Escape aimed at the configuration; then the configuration closes", async ({ page }) => {
  await page.evaluate(() => window.cfgFixture.open("profiles"));
  const dialog = config(page);
  await expect(dialog).toBeVisible();
  await dialog.getByRole("textbox", { name: "Profile name" }).fill("small");
  await dialog.getByText("Also make it the default", { exact: true }).click();
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog.locator('tr[data-profile="small"]')).toHaveAttribute("data-default", "true");

  const reset = dialog.getByRole("button", { name: "Reset default" });
  await reset.focus();
  // Freeze the page clock (also animation frames): the focus cannot move into the confirmation yet.
  const now = await page.evaluate(() => Date.now());
  await page.clock.pauseAt(now + 1_000);
  await reset.evaluate((el) => (el as HTMLElement).click());
  await expect(confirm(page)).toBeVisible();
  // Escape whose target is still the trigger inside the configuration (the state of B1-1), independent of where the
  // browser has put the focus meanwhile.
  await reset.dispatchEvent("keydown", { key: "Escape", code: "Escape", keyCode: 27 });
  await expect(confirm(page)).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('tr[data-profile="small"]')).toHaveAttribute("data-default", "true");

  await page.clock.resume();
  await expect(reset).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

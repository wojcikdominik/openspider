import { test, expect } from "@playwright/test";
import type {} from "./harness";

test.beforeEach(async ({ page }) => {
  await page.clock.install({ time: new Date("2026-10-18T12:00:00Z") });
  // A refused Vite module request leaves the ES module graph permanently failed.
  // Retry only that bootstrap transport failure; feature assertions run once.
  let refused = false;
  const failed = (request: import("@playwright/test").Request) => {
    if (request.resourceType() === "script" && request.failure()?.errorText.includes("ERR_CONNECTION_REFUSED")) refused = true;
  };
  page.on("requestfailed", failed);
  try {
    await page.goto("/e2e/export/index.html?transport=mock");
    await expect.poll(async () => {
      if (refused) {
        refused = false;
        await page.reload();
      }
      return page.evaluate(() => !!window.exportFixture);
    }).toBe(true);
    await expect(page.getByRole("button", { name: "Sitemap", exact: true })).toBeVisible();
  } finally { page.off("requestfailed", failed); }
});

// spec: OUT-006
test("save cancellation leaves the exporter untouched; completion offers the output folder", async ({ page }) => {
  await page.getByRole("button", { name: "Export", exact: true }).first().click();
  await page.evaluate(() => window.exportFixture.setDestination(null));
  await page.getByRole("button", { name: "Save…" }).click();
  expect(await page.evaluate(() => window.exportFixture.calls().filter(c => c.cmd === "export.view"))).toEqual([]);
  await page.evaluate(() => window.exportFixture.setDestination("/fixture/view.csv"));
  await page.getByRole("button", { name: "Save…" }).click();
  await expect(page.getByRole("progressbar")).toBeVisible();
  await page.evaluate(() => window.exportFixture.complete());
  await page.getByRole("button", { name: "Open folder" }).click();
  expect(await page.evaluate(() => window.exportFixture.reveals)).toEqual(["/fixture/view.csv"]);
});

// spec: UI-015
test("Mod+E opens the same snapshot dialog and Escape returns focus to the trigger", async ({ page }) => {
  const button = page.getByRole("button", { name: "Export", exact: true }).first();
  await button.focus();
  await page.keyboard.press(await page.evaluate(() => window.exportFixture.platform === "mac" ? "Meta+e" : "Control+e"));
  await expect(page.getByRole("dialog", { name: "Export options" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(button).toBeFocused();
});

// spec: UI-015
test("Escape closes the Mod+E dialog as soon as it is visible, before focus has moved into it", async ({ page }) => {
  const button = page.getByRole("button", { name: "Export", exact: true }).first();
  // Programmatic focus without a preceding key or pointer event (as with a screen reader): react-aria then moves the focus
  // into the new dialog only after an animation frame. Freezing the clock holds that frame, so Escape arrives while the
  // dialog is visible but the focus is still outside it (the state of the intermittent failure of the test above).
  await button.focus();
  await page.clock.pauseAt(new Date("2026-10-18T12:00:10Z"));
  await page.keyboard.press(await page.evaluate(() => window.exportFixture.platform === "mac" ? "Meta+e" : "Control+e"));
  await expect(page.getByRole("dialog", { name: "Export options" })).toBeVisible();
  expect(await page.evaluate(() => document.querySelector("[role='dialog']")?.contains(document.activeElement))).toBe(false);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.clock.resume();
  await expect(button).toBeFocused();
});

// spec: OUT-075 [P1]
test("multi-export presets survive reload and the tree supports substring search", async ({ page }) => {
  await page.getByRole("button", { name: "Multi export" }).click();
  await page.getByText("All", { exact: true }).click();
  await page.getByText("Links:All Inlinks", { exact: true }).click();
  await page.getByRole("textbox", { name: "Preset name" }).fill("Daily");
  await page.getByRole("button", { name: "Save preset", exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.exportFixture.calls().filter(c => c.cmd === "export.presetSave").length)).toBe(1);
  await page.reload();
  await page.getByRole("button", { name: "Multi export" }).click();
  await page.getByRole("button", { name: /Preset/ }).click();
  await page.getByRole("option", { name: "Daily" }).click();
  await expect(page.getByRole("checkbox", { name: "All", exact: true })).toBeChecked();
  await page.getByRole("textbox", { name: "Search exports" }).fill("inlinks");
  await expect(page.getByRole("checkbox", { name: "Links:All Inlinks" })).toBeChecked();
  await expect(page.getByRole("checkbox", { name: "All", exact: true })).toHaveCount(0);
});

// spec: OUT-007 [P1]
test("consolidation is disabled for CSV and enabled for XLSX", async ({ page }) => {
  await page.getByRole("button", { name: "Multi export" }).click();
  await expect(page.getByRole("checkbox", { name: "Combine in one workbook" })).toBeDisabled();
  await page.getByRole("button", { name: "File format" }).click();
  await page.getByRole("option", { name: "Excel Workbook (.xlsx)" }).click();
  await expect(page.getByRole("checkbox", { name: "Combine in one workbook" })).toBeEnabled();
});

// spec: OUT-124 [P1]
test("invalid sitemap priority blocks saving and list mode explains depth zero", async ({ page }) => {
  await page.getByRole("button", { name: "Sitemap", exact: true }).click();
  await page.getByRole("tab", { name: "Include priority" }).click();
  await expect(page.getByRole("note")).toContainText("all URLs have depth 0");
  await page.getByRole("spinbutton", { name: "Depth 0", exact: true }).fill("1.5");
  await expect(page.getByRole("button", { name: "Save…" })).toBeDisabled();
  await expect(page.getByRole("alert")).toContainText("priority 0.0 to 1.0");
});

// spec: OUT-128 [P1]
test("hreflang requires extraction and defaults to valid targets only", async ({ page }) => {
  await page.getByRole("button", { name: "Sitemap", exact: true }).click();
  await page.getByRole("tab", { name: "Include hreflang" }).click();
  await expect(page.getByRole("checkbox", { name: "Include hreflang", exact: true })).toBeDisabled();
  await expect(page.getByRole("checkbox", { name: /Only indexable/ })).toBeChecked();
});

// spec: OUT-021 [P1]
test("detail export keeps every selected URL in its command", async ({ page }) => {
  await page.getByRole("button", { name: "Export", exact: true }).nth(1).click();
  await page.getByRole("button", { name: "Save…" }).click();
  await expect.poll(() => page.evaluate(() => window.exportFixture.calls().filter(c => c.cmd === "export.detail").length)).toBe(1);
  expect(await page.evaluate(() => window.exportFixture.calls().find(c => c.cmd === "export.detail")?.args)).toMatchObject({ selection: { kind: "ids", ids: [11, 12, 13] }, spec: { detail: "cookies" } });
});

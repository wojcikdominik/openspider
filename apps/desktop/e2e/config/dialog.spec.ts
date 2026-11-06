import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import type {} from "./harness";

// E2E-A mit Mock-Kern: Bedienung des Rahmens in beiden Engines (WebKit, Chromium). Das Verhalten gegen den echten Kern prüft
// bridge.spec.ts.
test.beforeEach(async ({ page }) => {
  await page.goto("/e2e/config/index.html?transport=mock");
  await page.waitForFunction(() => !!window.cfgFixture);
});

const dialogOf = (page: Page) => page.getByRole("dialog", { name: "Crawl configuration" });
const openDialog = async (page: Page, anchor?: string) => {
  await page.evaluate((a) => window.cfgFixture.open(a), anchor);
  const dialog = dialogOf(page);
  await expect(dialog).toBeVisible();
  return dialog;
};
const goPage = async (dialog: ReturnType<typeof dialogOf>, name: string) => {
  await dialog.getByRole("navigation", { name: "Configuration pages" }).getByRole("button", { name, exact: true }).click();
  await expect(dialog.getByRole("navigation", { name: "Configuration pages" }).getByRole("button", { name, exact: true })).toHaveAttribute("aria-current", "page");
};

// spec: OUT-220
test("opens from its trigger, navigates and searches, Escape discards and returns focus", async ({ page }) => {
  const trigger = page.getByRole("button", { name: "Open configuration" });
  await trigger.focus();
  await page.keyboard.press("Enter");
  const dialog = dialogOf(page);
  await expect(dialog).toBeVisible();
  const nav = dialog.getByRole("navigation", { name: "Configuration pages" });
  await expect(nav.getByRole("button", { name: "Limits", exact: true })).toBeVisible();
  await goPage(dialog, "Limits");
  await dialog.getByRole("textbox", { name: "Max. URLs", exact: true }).fill("777");
  await dialog.getByRole("searchbox", { name: "Search settings" }).fill("accept");
  await dialog.getByRole("list", { name: "Search results" }).getByRole("button", { name: /Accept-Language value/ }).click();
  await expect(dialog.getByRole("textbox", { name: "Accept-Language value" })).toBeFocused();
  const axe = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
  expect(axe.violations.filter((v) => v.impact === "critical")).toEqual([]);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  expect(await page.evaluate(() => window.cfgFixture.calls().some((c) => c.cmd === "config.set"))).toBe(false);
});

// spec: OUT-220
test("a jump target focuses the control: openConfig({ anchor }) lands on the field", async ({ page }) => {
  const dialog = await openDialog(page, "extraction.store_html");
  await expect(dialog.getByRole("navigation").getByRole("button", { name: "Extraction", exact: true })).toHaveAttribute("aria-current", "page");
  await expect(dialog.getByRole("switch", { name: "Store raw HTML" })).toBeFocused();
  await page.evaluate(() => window.cfgFixture.close());
  await openDialog(page, "speed.threads");
  await expect(dialogOf(page).getByRole("textbox", { name: "Max. threads" })).toBeFocused();
});

// spec: OUT-220
test("OK writes the draft once, the core validates and the error shows at the field", async ({ page }) => {
  const dialog = await openDialog(page, "limits.max_urls");
  const urls = dialog.getByRole("textbox", { name: "Max. URLs", exact: true });
  await urls.fill("0");
  await expect(dialog.getByText(/muss mindestens 1 sein/)).toBeVisible();
  await expect(urls).toHaveAttribute("aria-invalid", "true");
  await urls.fill("4321");
  await expect(dialog.getByText(/muss mindestens 1 sein/)).toHaveCount(0);
  await dialog.getByRole("button", { name: "OK", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const saved = (await page.evaluate(() => window.cfgFixture.saved())) as { limits: { max_urls: number } };
  expect(saved.limits.max_urls).toBe(4321);
});

// spec: CRAWL-126 [M2]
test("while running only the released groups are editable; Pause and edit unlocks the rest", async ({ page }) => {
  await page.evaluate(() => {
    window.cfgFixture.pauseFollowsCommand();
    window.cfgFixture.setCrawl("running");
  });
  const dialog = await openDialog(page, "limits.max_urls");
  const urls = dialog.getByRole("textbox", { name: "Max. URLs", exact: true });
  await expect(urls).toBeDisabled();
  await expect(urls.locator("xpath=ancestor::*[@data-lock][1]")).toHaveAttribute("title", "Pause the crawl to change this setting");
  await goPage(dialog, "Speed");
  await expect(dialog.getByRole("textbox", { name: "Max. threads" })).toBeEnabled();
  await goPage(dialog, "Presets");
  await expect(dialog.getByRole("textbox", { name: "Min. characters" }).first()).toBeEnabled();
  await dialog.getByTestId("config-pause-edit").click();
  await expect(dialog.getByTestId("config-pause-edit")).toHaveCount(0);
  await goPage(dialog, "Limits");
  await expect(dialog.getByRole("textbox", { name: "Max. URLs", exact: true })).toBeEnabled();
  expect(await page.evaluate(() => window.cfgFixture.pauseCalls())).toBe(1);
});

// spec: CRAWL-126 [M2]
test("a crawl started by another process makes the dialog read-only", async ({ page }) => {
  await page.evaluate(() => window.cfgFixture.setCrawl("running", false));
  const dialog = await openDialog(page, "speed.threads");
  await expect(dialog.getByText(/started by another process/).first()).toBeVisible();
  await expect(dialog.getByRole("textbox", { name: "Max. threads" })).toBeDisabled();
  await expect(dialog.getByTestId("config-pause-edit")).toHaveCount(0);
});

// spec: CRAWL-077 [P1] [M2]
test("the rendering switch is locked while running and paused, with the new-crawl hint", async ({ page }) => {
  for (const phase of ["running", "paused"] as const) {
    await page.evaluate((p) => window.cfgFixture.setCrawl(p), phase);
    const dialog = await openDialog(page, "rendering.mode");
    await expect(dialog.getByTestId("rendering-locked-note")).toHaveText("The rendering mode can only be changed in a new crawl.");
    const group = dialog.locator('[data-setting-anchor="rendering.mode"]');
    await expect(group).toHaveAttribute("data-lock", "newCrawl");
    for (const radio of await group.getByRole("radio").all()) await expect(radio).toBeDisabled();
    await page.evaluate(() => window.cfgFixture.close());
    await expect(page.getByRole("dialog")).toHaveCount(0);
  }
});

// spec: CRAWL-077 [P1] [M2]
test("a Chromium error of the core appears as a dialog message with path and download option, without losing the input", async ({ page }) => {
  const dialog = await openDialog(page, "rendering.mode");
  await dialog.getByText("JavaScript (headless Chromium)", { exact: true }).click();
  await page.evaluate(() => window.cfgFixture.failNextSet({ code: "InvalidInput", messageKey: "err-chromium-missing", detail: "Chromium not found" }));
  await dialog.getByRole("button", { name: "OK", exact: true }).click();
  const problem = dialog.getByTestId("rendering-problem");
  await expect(problem).toContainText("Chromium is not available: Chromium not found");
  await expect(problem.getByRole("link", { name: "Download Chromium" })).toBeVisible();
  await problem.getByRole("button", { name: "Set path" }).click();
  await expect(dialog.getByRole("textbox", { name: "Chromium path" })).toBeFocused();
  await expect(dialog.getByRole("radio", { name: /JavaScript \(headless Chromium\)/ })).toBeChecked();
  await dialog.getByRole("textbox", { name: "Chromium path" }).fill("/opt/chromium/chrome");
  await dialog.getByRole("button", { name: "OK", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const saved = (await page.evaluate(() => window.cfgFixture.saved())) as { rendering: { mode: string; chromium_path: string } };
  expect(saved.rendering).toMatchObject({ mode: "javascript", chromium_path: "/opt/chromium/chrome" });
});

// spec: CRAWL-111
test("regex tester: included or excluded with the line number, partial-match hint, invalid line marked", async ({ page }) => {
  const dialog = await openDialog(page, "scope.include");
  await dialog.getByRole("textbox", { name: "Include rules" }).fill("https://site.test/blog\nhttps://site.test/shop/.*");
  await dialog.getByRole("textbox", { name: "Exclude rules" }).fill("https://site.test/shop/private/.*\n(");
  await dialog.getByRole("textbox", { name: "Test URL" }).fill("https://site.test/shop/a b");
  const result = dialog.getByTestId("regex-result");
  await expect(result).toHaveAttribute("data-verdict", "included");
  await expect(dialog.getByTestId("regex-match")).toContainText("Include line 2");
  await expect(result).toContainText("a%20b");
  await expect(dialog.getByTestId("regex-invalid")).toContainText("Exclude line 2");
  await dialog.getByRole("textbox", { name: "Test URL" }).fill("https://site.test/shop/private/x");
  await expect(result).toHaveAttribute("data-verdict", "excluded");
  await dialog.getByRole("textbox", { name: "Test URL" }).fill("https://site.test/blog/post");
  await expect(dialog.getByTestId("regex-partial-hint")).toContainText("only matches part of the URL");
});

// spec: CRAWL-107
test("rewrite test field updates on every rule change without saving", async ({ page }) => {
  const dialog = await openDialog(page, "rewrite.remove_parameters");
  await dialog.getByRole("textbox", { name: "Test URL" }).fill("https://site.test/old/page?utm_source=x&keep=1");
  await expect(dialog.getByTestId("rewrite-output")).toHaveText("https://site.test/old/page?utm_source=x&keep=1");
  await dialog.getByRole("textbox", { name: "Parameters to remove" }).fill("utm_source");
  await expect(dialog.getByTestId("rewrite-output")).toHaveText("https://site.test/old/page?keep=1");
  await dialog.getByRole("button", { name: "Add rule" }).click();
  await dialog.getByRole("textbox", { name: "Regular expression, rule 1" }).fill("/old/");
  await dialog.getByRole("textbox", { name: "Replacement, rule 1" }).fill("/new/");
  await expect(dialog.getByTestId("rewrite-output")).toHaveText("https://site.test/new/page?keep=1");
  expect(await page.evaluate(() => window.cfgFixture.calls().some((c) => c.cmd === "config.set"))).toBe(false);
});

// spec: CRAWL-102
test("custom robots.txt per subdomain with live file and tester", async ({ page }) => {
  const dialog = await openDialog(page, "robots.mode");
  await dialog.getByRole("textbox", { name: "Subdomain" }).fill("blog.example.test");
  await dialog.getByRole("button", { name: "Add", exact: true }).click();
  const editor = dialog.getByRole("textbox", { name: "robots.txt for blog.example.test" });
  await dialog.getByRole("button", { name: "Load live file" }).click();
  await expect(editor).toHaveValue(/Disallow: \/live-blocked\//);
  await editor.fill("User-agent: *\nAllow: /\nDisallow: /blog/\n");
  await dialog.getByRole("textbox", { name: "URL to test" }).fill("https://blog.example.test/blog/post");
  await dialog.getByRole("button", { name: "Test", exact: true }).click();
  const result = dialog.getByTestId("robots-result");
  await expect(result).toHaveAttribute("data-allowed", "false");
  await expect(result).toContainText("Deciding line 3");
  await expect(result.locator("mark")).toHaveText("Disallow: /blog/");
});

// spec: OUT-220 [M2]
test("profiles: save, load as draft, make default, reset default asks for confirmation", async ({ page }) => {
  const dialog = await openDialog(page, "limits.max_urls");
  await dialog.getByRole("textbox", { name: "Max. URLs", exact: true }).fill("99");
  await goPage(dialog, "Profiles");
  await dialog.getByRole("textbox", { name: "Profile name" }).fill("small");
  await dialog.getByText("Also make it the default", { exact: true }).click();
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog.locator('tr[data-profile="small"]')).toHaveAttribute("data-default", "true");
  expect(await page.evaluate(() => window.cfgFixture.profiles())).toEqual(["small"]);
  await dialog.getByRole("button", { name: "Reset default" }).click();
  const confirm = page.getByRole("dialog", { name: "Reset default?" });
  await expect(confirm).toBeVisible();
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog.locator('tr[data-profile="small"]')).toHaveAttribute("data-default", "true");
  await dialog.getByRole("button", { name: "Reset default" }).click();
  await page.getByRole("dialog", { name: "Reset default?" }).getByRole("button", { name: "Confirm" }).click();
  await expect(dialog.locator('tr[data-profile="small"]')).not.toHaveAttribute("data-default", "true");
});

// spec: OUT-220 [M2]
test("loading a profile is blocked while running, with a hint", async ({ page }) => {
  await page.evaluate(() => {
    window.cfgFixture.seedProfile("p", {});
    window.cfgFixture.setCrawl("running");
  });
  const dialog = await openDialog(page, "profiles");
  await expect(dialog.getByTestId("profiles-locked")).toContainText("Pause the crawl first");
  await expect(dialog.getByRole("button", { name: "Load profile p" })).toBeDisabled();
});

test("German dialog at 1024 x 640: nothing is cut off, footer buttons stay reachable", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 640 });
  await page.goto("/e2e/config/index.html?transport=mock&lang=de");
  await page.waitForFunction(() => !!window.cfgFixture);
  await page.evaluate(() => window.cfgFixture.open());
  const dialog = page.getByRole("dialog", { name: "Crawl-Konfiguration" });
  await expect(dialog).toBeVisible();
  const box = (await dialog.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(1024);
  expect(box.y + box.height).toBeLessThanOrEqual(640);
  for (const name of ["OK", "Abbrechen", "Abweichungen anzeigen (0)"]) {
    const b = (await dialog.getByRole("button", { name, exact: true }).boundingBox())!;
    expect(b.y + b.height, name).toBeLessThanOrEqual(box.y + box.height);
    expect(b.x, name).toBeGreaterThanOrEqual(box.x);
  }
  const nav = (await dialog.getByRole("navigation", { name: "Konfigurationsseiten" }).boundingBox())!;
  const content = (await dialog.locator(".os-config-content").boundingBox())!;
  expect(nav.x + nav.width).toBeLessThanOrEqual(content.x);
});

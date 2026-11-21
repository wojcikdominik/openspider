import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import type {} from "./harness";

// UI-131 / OUT-254: profile without key, with `stable` or with `beta`, injected running version (harness query).
async function open(page: Page, { lang = "en", version = "1.0.1", stored, query = "" }: { lang?: string; version?: string; stored?: "stable" | "beta"; query?: string } = {}) {
  await page.goto(`/e2e/settings-sections/index.html?transport=mock&lang=${lang}&version=${encodeURIComponent(version)}${query}`);
  await page.waitForFunction(() => !!window.settingsFixture);
  if (stored) {
    await page.evaluate((channel) => window.settingsFixture.invoke("settings.set", { patch: { updates: { channel } } }), stored);
    await page.reload();
    await page.waitForFunction(() => !!window.settingsFixture);
  }
  await page.getByRole("tab", { name: lang === "de" ? "Updates" : "Software updates", exact: true }).click();
}
/** Every request the page sends outside the dev server (an update server would be one of them). */
function external(page: Page): string[] {
  const seen: string[] = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (!["localhost", "127.0.0.1", "[::1]"].includes(url.hostname) && url.protocol.startsWith("http")) seen.push(request.url());
  });
  return seen;
}
/** Clicks an option like a user: the native input is visually hidden, its label takes the pointer. */
const choose = (page: Page, name: string) => page.locator(".os-radio-items").getByText(name, { exact: true }).click();
const updateCalls = (page: Page) => page.evaluate(() => window.settingsFixture.calls().filter((c) => c.cmd === "update.check" || c.cmd === "update.download").length);
const stored = (page: Page) => page.evaluate(() => window.settingsFixture.settings()?.updates.channel ?? null);

// spec: UI-131
test("profile without key preselects stable on 1.0.1 and beta on 1.1.0-beta.1", async ({ page }) => {
  await open(page, { version: "1.0.1" });
  const group = page.getByRole("radiogroup", { name: "Update channel" });
  await expect(group.getByRole("radio", { name: "Stable (recommended)" })).toBeChecked();
  await expect(group.getByRole("radio", { name: "Beta" })).not.toBeChecked();
  expect(await stored(page)).toBeNull();
  await open(page, { version: "1.1.0-beta.1" });
  await expect(page.getByRole("radio", { name: "Beta" })).toBeChecked();
});

// spec: UI-131
// spec: OUT-254
test("switching to beta stores the key without restart, sends no request and survives a restart", async ({ page }) => {
  const requests = external(page);
  await open(page, { version: "1.0.1" });
  await choose(page, "Beta");
  await expect.poll(() => stored(page)).toBe("beta");
  await expect(page.getByRole("status").filter({ hasText: "From the next update check on, you will also receive beta versions." })).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await updateCalls(page)).toBe(0);
  expect(requests).toEqual([]);
  await page.reload();
  await page.waitForFunction(() => !!window.settingsFixture);
  await page.getByRole("tab", { name: "Software updates", exact: true }).click();
  await expect(page.getByRole("radio", { name: "Beta" })).toBeChecked();
  expect(requests).toEqual([]);
});

// spec: UI-131
// spec: OUT-254
test("stay-on hint on 1.1.0-beta.3 after switching to stable, also after restart, gone after switching back", async ({ page }) => {
  await open(page, { version: "1.1.0-beta.3", stored: "beta" });
  const hint = page.getByRole("status").filter({ hasText: "You will stay on 1.1.0-beta.3 until a newer stable version is released." });
  await expect(hint).toHaveCount(0);
  await choose(page, "Stable (recommended)");
  await expect(hint).toBeVisible();
  await page.reload();
  await page.waitForFunction(() => !!window.settingsFixture);
  await page.getByRole("tab", { name: "Software updates", exact: true }).click();
  await expect(page.getByRole("radio", { name: "Stable (recommended)" })).toBeChecked();
  await expect(hint).toBeVisible();
  await choose(page, "Beta");
  await expect(hint).toHaveCount(0);
  await open(page, { version: "1.0.1", stored: "stable" });
  await expect(page.getByRole("radio", { name: "Stable (recommended)" })).toBeChecked();
  await expect(page.getByText(/You will stay on/)).toHaveCount(0);
});

// spec: UI-131
// spec: OUT-250
test("environment lock greys out the group and keeps the value; automatic checks off keeps it usable", async ({ page }) => {
  await open(page, { stored: "beta", query: "&updateEnv=1" });
  await expect(page.getByRole("radio", { name: "Beta" })).toBeDisabled();
  await expect(page.getByRole("radio", { name: "Stable (recommended)" })).toBeDisabled();
  await expect(page.getByRole("radio", { name: "Beta" })).toBeChecked();
  expect(await stored(page)).toBe("beta");
  await open(page);
  await page.getByText("Automatically check for updates", { exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.settingsFixture.settings()?.updates.check)).toBe(false);
  await choose(page, "Beta");
  await expect.poll(() => stored(page)).toBe("beta");
});

// spec: UI-131
test("a failing save shows an alert and selects the stored value again", async ({ page }) => {
  await open(page, { stored: "stable", query: "&channelFails=1" });
  await choose(page, "Beta");
  await expect(page.getByRole("alert")).toHaveText("The update could not be completed. Please try again.");
  await expect(page.getByRole("radio", { name: "Stable (recommended)" })).toBeChecked();
  expect(await stored(page)).toBe("stable");
});

for (const scheme of ["light", "dark"] as const) {
  // spec: UI-131
  // spec: UI-126
  test(`keyboard and axe in the ${scheme} theme`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await open(page);
    const stable = page.getByRole("radio", { name: "Stable (recommended)" });
    // Tab reaches the group: from the switch the next stop is the selected radio.
    await page.getByRole("switch", { name: "Automatically check for updates" }).focus();
    await page.keyboard.press("Tab");
    await expect(stable).toBeFocused();
    const outline = () => page.locator(".os-radio[data-focus-visible] .os-radio-dot").evaluate((dot) => getComputedStyle(dot).outlineStyle);
    expect(await outline()).not.toBe("none");
    await page.keyboard.press("ArrowRight");
    await expect(page.getByRole("radio", { name: "Beta" })).toBeChecked();
    await expect(page.getByRole("radio", { name: "Beta" })).toBeFocused();
    expect(await outline()).not.toBe("none");
    await expect.poll(() => stored(page)).toBe("beta");
    const issues = await new AxeBuilder({ page }).analyze();
    expect(issues.violations.filter((v) => v.impact === "critical" || v.impact === "serious").map((v) => v.id)).toEqual([]);
  });
}

// spec: UI-131
// spec: UI-001
// spec: UI-121
test("German texts at 1024 by 640 without overflow or truncation", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 640 });
  await open(page, { lang: "de", version: "1.1.0-beta.3", stored: "stable" });
  const group = page.getByRole("radiogroup", { name: "Update-Kanal" });
  await expect(group.getByRole("radio", { name: "Stabil (empfohlen)" })).toBeChecked();
  await expect(page.getByText("Beta-Versionen bringen neue Funktionen früher und können Fehler enthalten. Im Beta-Kanal erhalten Sie auch jede neuere stabile Version.")).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Sie bleiben auf 1.1.0-beta.3, bis eine neuere stabile Version erscheint." })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  // Visible texts only (the visually hidden wrapper of the native input has no text).
  const clipped = await group.locator(".os-radio span:not(.os-radio-dot), .os-label").evaluateAll((nodes) => nodes.filter((n) => !!n.textContent?.trim() && n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent));
  expect(clipped).toEqual([]);
  const issues = await new AxeBuilder({ page }).analyze();
  expect(issues.violations.filter((v) => v.impact === "critical" || v.impact === "serious").map((v) => v.id)).toEqual([]);
});

// Unabhängige Abnahme UI-131 / OUT-254 im Einstellungsfenster (Testagent; Grundlage allein die Spec).
// acceptance: independent
//
// Harness: settings-sections/harness.tsx mit `version=` (injizierte laufende Version), `updateEnv=1` (Abschaltung per
// Umgebung) und `channelFails=1` (Speichern schlägt fehl). Einstellungen überleben ein Neuladen (= Neustart).
import { expect, test, type Page, type Request } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import type {} from "./harness";

async function open(page: Page, query: Record<string, string> = {}, lang = "en") {
  const params = new URLSearchParams({ transport: "mock", lang, ...query });
  await page.goto(`/e2e/settings-sections/index.html?${params}`);
  await page.waitForFunction(() => !!window.settingsFixture);
  await page.getByRole("tab", { name: lang === "de" ? "Updates" : "Software updates", exact: true }).click();
  await expect(group(page, lang)).toBeVisible();
}

const group = (page: Page, lang = "en") => page.getByRole("radiogroup", { name: lang === "de" ? "Update-Kanal" : "Update channel" });
const stable = (page: Page, lang = "en") => group(page, lang).getByRole("radio", { name: lang === "de" ? "Stabil (empfohlen)" : "Stable (recommended)" });
const beta = (page: Page, lang = "en") => group(page, lang).getByRole("radio", { name: "Beta" });
// Wie ein Nutzer: Klick auf die sichtbare Beschriftung der Option (das Eingabefeld selbst ist visuell verborgen).
const pick = (page: Page, which: "stable" | "beta", lang = "en", force = false) =>
  group(page, lang).getByText(which === "beta" ? "Beta" : lang === "de" ? "Stabil (empfohlen)" : "Stable (recommended)", { exact: true }).click({ force });
const hint = (page: Page) => page.locator("p[role=status][data-update-channel-hint]");
const stored = (page: Page) => page.evaluate(() => (window.settingsFixture.settings()?.updates as { channel?: string | null } | undefined)?.channel ?? null);
const updateCalls = (page: Page) => page.evaluate(() => window.settingsFixture.calls().filter(c => /^update\.(check|download|install)$/.test(c.cmd)).length);
const NEXT = "From the next update check on, you will also receive beta versions.";

// spec: UI-131
// acceptance: independent
test("preselection follows the standard rule: 1.0.1 → Stable, 1.1.0-beta.1 → Beta", async ({ page }) => {
  await open(page, { version: "1.0.1" });
  await expect(stable(page)).toBeChecked();
  await expect(beta(page)).not.toBeChecked();
  await page.evaluate(() => localStorage.clear());
  await open(page, { version: "1.1.0-beta.1" });
  await expect(beta(page)).toBeChecked();
  await expect(stable(page)).not.toBeChecked();
});

// spec: UI-131
// spec: OUT-254
// acceptance: independent
test("switching to Beta stores at once without network traffic, shows the hint and survives a restart", async ({ page }) => {
  await open(page, { version: "1.0.1" });
  const origin = new URL(page.url()).origin;
  const foreign: string[] = [];
  const onRequest = (r: Request) => { if (!r.url().startsWith(origin) && !r.url().startsWith("data:")) foreign.push(r.url()); };
  page.on("request", onRequest);
  const before = await updateCalls(page);
  await pick(page, "beta");
  await expect(beta(page)).toBeChecked();
  await expect.poll(() => stored(page)).toBe("beta");
  await expect(hint(page)).toHaveText(NEXT);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(await updateCalls(page)).toBe(before);
  expect(foreign).toEqual([]);
  page.off("request", onRequest);
  await page.reload();
  await page.waitForFunction(() => !!window.settingsFixture);
  await page.getByRole("tab", { name: "Software updates", exact: true }).click();
  await expect(beta(page)).toBeChecked();
  // Der Hinweis gilt nur bis zum Verlassen der Seite.
  await expect(hint(page)).not.toHaveText(NEXT);
});

// spec: UI-131
// acceptance: independent
test("back to Stable on 1.1.0-beta.3 announces the stay hint, keeps it after restart, Beta removes it", async ({ page }) => {
  await open(page, { version: "1.1.0-beta.3" });
  await expect(beta(page)).toBeChecked();
  await pick(page, "stable");
  const stay = "You will stay on 1.1.0-beta.3 until a newer stable version is released.";
  await expect(hint(page)).toHaveText(stay);
  await expect(page.getByRole("status").filter({ hasText: stay })).toHaveCount(1);
  await expect.poll(() => stored(page)).toBe("stable");
  await expect(page.getByRole("link", { name: /reinstall|older|downgrade/i })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /reinstall|older|downgrade|reset/i })).toHaveCount(0);
  await page.reload();
  await page.waitForFunction(() => !!window.settingsFixture);
  await page.getByRole("tab", { name: "Software updates", exact: true }).click();
  await expect(stable(page)).toBeChecked();
  await expect(hint(page)).toHaveText(stay);
  await pick(page, "beta");
  await expect(hint(page)).not.toContainText("You will stay on");
});

// spec: UI-131
// acceptance: independent
test("Stable on 1.0.1 shows no stay hint", async ({ page }) => {
  await open(page, { version: "1.0.1" });
  await expect(stable(page)).toBeChecked();
  await expect(page.getByText(/You will stay on/)).toHaveCount(0);
});

// spec: UI-131
// spec: OUT-250
// acceptance: independent
test("environment lock greys out the group and keeps the stored value", async ({ page }) => {
  await open(page, { version: "1.0.1" });
  await pick(page, "beta");
  await expect.poll(() => stored(page)).toBe("beta");
  await open(page, { version: "1.0.1", updateEnv: "1" });
  await expect(beta(page)).toBeDisabled();
  await expect(stable(page)).toBeDisabled();
  await expect(beta(page)).toBeChecked();
  await pick(page, "stable", "en", true);
  await page.waitForTimeout(200);
  expect(await stored(page)).toBe("beta");
});

// spec: UI-131
// acceptance: independent
test("automatic checks off keeps the group usable", async ({ page }) => {
  await open(page, { version: "1.0.1" });
  await page.getByText("Automatically check for updates", { exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.settingsFixture.settings()?.updates.check)).toBe(false);
  await expect(beta(page)).toBeEnabled();
  await pick(page, "beta");
  await expect.poll(() => stored(page)).toBe("beta");
});

// spec: UI-131
// acceptance: independent
test("save failure shows an alert and reselects the stored value", async ({ page }) => {
  await open(page, { version: "1.0.1", channelFails: "1" });
  await pick(page, "beta");
  await expect(page.getByRole("alert")).toBeVisible();
  await expect(stable(page)).toBeChecked();
  await expect(beta(page)).not.toBeChecked();
  expect(await stored(page)).toBeNull();
});

// spec: UI-131
// spec: UI-126
// acceptance: independent
for (const scheme of ["light", "dark"] as const) {
  test(`keyboard: Tab reaches the group, arrows switch, visible focus (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await open(page, { version: "1.0.1" });
    const sw = page.getByRole("switch", { name: "Automatically check for updates" });
    await sw.focus();
    await page.keyboard.press("Tab");
    await expect(stable(page)).toBeFocused();
    const outline = await page.evaluate(() => {
      const radio = document.activeElement?.closest(".os-radio") ?? document.activeElement?.parentElement;
      const candidates = [document.activeElement, radio, ...(radio ? Array.from(radio.querySelectorAll("*")) : [])].filter(Boolean) as Element[];
      return candidates.some(el => {
        const s = getComputedStyle(el);
        return (s.outlineStyle !== "none" && parseFloat(s.outlineWidth) > 0) || (s.boxShadow && s.boxShadow !== "none");
      });
    });
    expect(outline).toBe(true);
    await page.keyboard.press("ArrowRight");
    await expect(beta(page)).toBeChecked();
    await expect(beta(page)).toBeFocused();
    await expect.poll(() => stored(page)).toBe("beta");
    await page.keyboard.press("ArrowLeft");
    await expect(stable(page)).toBeChecked();
    await expect.poll(() => stored(page)).toBe("stable");
    const issues = await new AxeBuilder({ page }).analyze();
    expect(issues.violations.filter(v => v.impact === "critical" || v.impact === "serious").map(v => v.id)).toEqual([]);
  });
}

// spec: UI-131
// spec: UI-121
// spec: UI-001
// acceptance: independent
test("German texts at 1024×640 without truncation or overflow", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 640 });
  await open(page, { version: "1.1.0-beta.3" }, "de");
  await expect(beta(page, "de")).toBeChecked();
  await expect(page.getByText("Beta-Versionen bringen neue Funktionen früher und können Fehler enthalten. Im Beta-Kanal erhalten Sie auch jede neuere stabile Version.", { exact: true })).toBeVisible();
  await pick(page, "stable", "de");
  await expect(hint(page)).toHaveText("Sie bleiben auf 1.1.0-beta.3, bis eine neuere stabile Version erscheint.");
  await pick(page, "beta", "de");
  await expect(hint(page)).toHaveText("Ab der nächsten Update-Prüfung erhalten Sie auch Beta-Versionen.");
  const clipped = await page.evaluate(() => {
    const root = document.querySelector("[data-setting-anchor='updates.channel']")!;
    const els = [root, ...Array.from(root.querySelectorAll("*")), ...Array.from(document.querySelectorAll("p[data-update-channel-hint]"))];
    return els.filter(el => (el as HTMLElement).scrollWidth > (el as HTMLElement).clientWidth + 1 && getComputedStyle(el).overflowX !== "visible").map(el => el.textContent);
  });
  expect(clipped).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  const issues = await new AxeBuilder({ page }).analyze();
  expect(issues.violations.filter(v => v.impact === "critical" || v.impact === "serious").map(v => v.id)).toEqual([]);
});

// spec: UI-131
// spec: UI-121
// acceptance: independent
test("English texts", async ({ page }) => {
  await open(page, { version: "1.0.1" });
  await expect(page.getByText("Beta versions bring new features earlier and may contain bugs. On the beta channel you also receive every newer stable version.", { exact: true })).toBeVisible();
});

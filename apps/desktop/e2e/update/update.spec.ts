import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { startFakeBridge, type FakeBridge } from "../support/fake-bridge";
import tickFixture from "../../src/ipc/__fixtures__/Tick.json" with { type: "json" };
import type { UpdateState } from "../../src/ipc/gen/UpdateState";
let bridge: FakeBridge;
let state: UpdateState;
test.afterEach(async () => { await bridge?.stop(); });
for (const lang of ["de", "en"] as const) {
  // spec: OUT-251
  // spec: UI-002 [M3]
  test(`${lang}: menu, release notes, background progress, ready and on quit`, async ({ page }) => {
    bridge = await startFakeBridge();
    bridge.core.settings.general.language = lang;
    state = "idle";
    bridge.core.handlers.set("update.status", () => state);
    bridge.core.handlers.set("update.check", () => state = { available: { version: "1.0.1", notes: "Fixture notes <script>plain text</script>", pubDate: null } });
    bridge.core.handlers.set("update.download", () => state = { downloading: { done: 42, total: 100 } });
    bridge.core.handlers.set("update.install", () => state);
    await page.goto(`/?bridge=${encodeURIComponent(bridge.url)}`);
    await expect.poll(() => page.evaluate(() => window.__osMenu?.model.lang)).toBe(lang);
    await expect.poll(() => page.evaluate(() => window.__osMenu?.model.menus.find(m => m.id === "help")?.items.find(i => i.action === "help.updates")?.enabled)).toBe(true);
    await page.evaluate(async () => {
      // @ts-expect-error Vite source module
      const { runAction } = await import("/src/ui/actions.ts");
      runAction("help.updates", { source: "menu" });
    });
    const dialog = page.getByRole("dialog", { name: lang === "de" ? "Eine neue Version von OpenSpider ist verfügbar!" : "A new version of OpenSpider is available!" });
    await expect(dialog.getByText("Fixture notes <script>plain text</script>")).toBeVisible();
    await expect(dialog.getByRole("progressbar")).toHaveAttribute("value", "42");
    expect(await dialog.locator("script").count()).toBe(0);
    let axe = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
    expect(axe.violations.filter(v => v.impact === "critical" || v.impact === "serious")).toEqual([]);
    await dialog.getByRole("button", { name: lang === "de" ? "Später erinnern" : "Remind me later", exact: true }).click();
    await expect(dialog).not.toBeVisible();
    state = { ready: { version: "1.0.1" } };
    await page.evaluate(async () => {
      // @ts-expect-error Vite source module
      const { useUpdate } = await import("/src/features/update/state.ts");
      useUpdate.setState({ open: true });
    });
    await expect(dialog.getByRole("button", { name: lang === "de" ? "Update installieren" : "Install update" })).toBeEnabled();
    axe = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
    expect(axe.violations.filter(v => v.impact === "critical" || v.impact === "serious")).toEqual([]);
    await dialog.getByRole("button", { name: lang === "de" ? "Beim Beenden installieren" : "Install on quit", exact: true }).click();
    await expect.poll(() => bridge.core.calls.find(c => c.cmd === "update.install")?.args).toEqual({ when: "onQuit" });
  });
  // spec: OUT-251
  // spec: CRAWL-142 [M3]
  test(`${lang}: running crawl only offers installation after crawl ends`, async ({ page }) => {
    bridge = await startFakeBridge();
    bridge.core.settings.general.language = lang;
    state = { ready: { version: "1.0.1" } };
    bridge.core.handlers.set("update.status", () => state);
    bridge.core.handlers.set("update.install", () => state);
    await page.goto(`/?bridge=${encodeURIComponent(bridge.url)}`);
    await page.waitForFunction(() => window.__osMenu !== undefined);
    await page.evaluate(async (crawl) => {
      // @ts-expect-error Vite source module
      const { useSession } = await import("/src/state/session.ts");
      useSession.setState({ crawl });
      // @ts-expect-error Vite source module
      const { useUpdate } = await import("/src/features/update/state.ts");
      useUpdate.setState({ open: true });
    }, tickFixture.crawl);
    const dialog = page.getByRole("dialog", { name: lang === "de" ? "Eine neue Version von OpenSpider ist verfügbar!" : "A new version of OpenSpider is available!" });
    const after = dialog.getByRole("button", { name: lang === "de" ? "Nach Crawl-Ende installieren" : "Install after crawl ends" });
    await expect(after).toBeVisible();
    await expect(dialog.getByRole("button", { name: lang === "de" ? "Update installieren" : "Install update" })).toHaveCount(0);
    await expect(dialog.getByRole("button", { name: lang === "de" ? "Beim Beenden installieren" : "Install on quit", exact: true })).toHaveCount(0);
    const axe = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
    expect(axe.violations.filter(v => v.impact === "critical" || v.impact === "serious")).toEqual([]);
    await after.click();
    await expect.poll(() => bridge.core.calls.find(c => c.cmd === "update.install")?.args).toEqual({ when: "afterCrawl" });
  });
}
// spec: OUT-251
// spec: UI-002 [M3]
for (const lang of ["de", "en"] as const) test(`${lang}: manual check reports current, disabled and failed states`, async ({ page }) => {
  bridge = await startFakeBridge();
  bridge.core.settings.general.language = lang;
  state = "idle";
  bridge.core.handlers.set("update.status", () => state);
  bridge.core.handlers.set("update.check", () => state);
  await page.goto(`/?bridge=${encodeURIComponent(bridge.url)}`);
  await page.waitForFunction(() => window.__osMenu !== undefined);
  for (const [next, text] of [
    ["upToDate", lang === "de" ? "OpenSpider ist aktuell." : "OpenSpider is up to date."],
    [{ disabled: "environment" }, lang === "de" ? "Die Update-Prüfung ist durch OPENSPIDER_DISABLE_UPDATE_CHECK=1 deaktiviert." : "Update checks are disabled by OPENSPIDER_DISABLE_UPDATE_CHECK=1."],
    [{ disabled: "settings" }, lang === "de" ? "Die Update-Prüfung ist in den Einstellungen deaktiviert." : "Update checks are disabled in Settings."],
    [{ disabled: "missingPublicKey" }, lang === "de" ? "Die Update-Prüfung ist nicht verfügbar, weil der Signaturschlüssel fehlt." : "Update checks are unavailable because the signing key is missing."],
    [{ disabled: "invalidPublicKey" }, lang === "de" ? "Die Update-Prüfung ist nicht verfügbar, weil der Signaturschlüssel ungültig ist." : "Update checks are unavailable because the signing key is invalid."],
    [{ failed: "mock interrupted" }, lang === "de" ? "Das Update konnte nicht abgeschlossen werden. Bitte erneut versuchen." : "The update could not be completed. Please try again."],
  ] as const) {
    state = next;
    await page.evaluate(async () => {
      // @ts-expect-error Vite source module
      const { runAction } = await import("/src/ui/actions.ts");
      runAction("help.updates", { source: "menu" });
    });
    const dialog = page.getByRole("dialog", { name: lang === "de" ? "Updates" : "Software updates" });
    await expect(dialog.getByText(text)).toBeVisible();
    const axe = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
    expect(axe.violations.filter(v => v.impact === "critical" || v.impact === "serious")).toEqual([]);
    await dialog.getByRole("button", { name: lang === "de" ? "Schließen" : "Close", exact: true }).click();
  }
});
// spec: OUT-251
test("update setting defaults on, persists changes and explains environment override", async ({ page }) => {
  bridge = await startFakeBridge();
  bridge.core.settings.general.language = "en";
  state = "idle";
  bridge.core.handlers.set("update.status", () => state);
  bridge.core.handlers.set("update.set_auto", (args) => { bridge.core.settings.updates.check = (args as { check: boolean }).check; });
  await page.goto(`/settings.html?bridge=${encodeURIComponent(bridge.url)}#updates`);
  const toggle = page.getByRole("switch", { name: "Automatically check for updates" });
  await expect(toggle).toBeChecked();
  await page.getByText("Automatically check for updates", { exact: true }).click();
  await expect(toggle).not.toBeChecked();
  expect(bridge.core.settings.updates.check).toBe(false);
  state = { disabled: "environment" };
  await page.reload();
  await expect(toggle).toBeDisabled();
  await expect(page.getByText("Update checks are disabled by OPENSPIDER_DISABLE_UPDATE_CHECK=1.")).toBeVisible();
  const axe = await new AxeBuilder({ page }).include('[data-settings-section="updates"]').analyze();
  expect(axe.violations.filter(v => v.impact === "critical" || v.impact === "serious")).toEqual([]);
});

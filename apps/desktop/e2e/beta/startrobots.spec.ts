import { test, expect } from "@playwright/test";
import { ensureTools, startAlphaBridge, startSite } from "../alpha/support";
import { scan } from "../a11y/fixture";

// B4-STARTFIX (UI-019, CRAWL-128): Fixture `ui/fx-ui-robots-start` mit `Disallow: /` über die echte Bridge. Der Kern beendet den
// Crawl als „Fertig“ (kein Fehlschlag) und meldet die Sperre; der Dialog verweist nur auf die Einstellung „robots.txt ignorieren“
// und ändert die Konfiguration nicht.
// spec: UI-019
// spec: CRAWL-128
test("a start URL blocked by robots.txt opens a dialog that points to the Ignore robots.txt setting", async ({ page }) => {
  test.setTimeout(60_000);
  await ensureTools();
  const site = await startSite("ui/fx-ui-robots-start");
  const bridge = await startAlphaBridge();
  try {
    await page.goto(`/?${new URLSearchParams({ bridge: bridge.url })}`);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    await page.evaluate(async () => {
      const { config } = await import(/* @vite-ignore */ String("/src/ipc/config.ts"));
      const dto = await config.get(); const root = dto.config as Record<string, Record<string, unknown>>;
      await config.set({ ...dto, config: { ...root, robots: { ...root.robots, retries: 0, mode: "respect" } } });
    });
    const field = page.getByLabel("Start URL", { exact: true });
    await field.fill(`${site.base}/`);
    await field.press("Enter");

    const dialog = page.getByRole("dialog", { name: "Crawl notice" });
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await expect(dialog).toContainText("robots.txt blocks the start address.");
    await expect(dialog.getByTestId("notify-robots-file")).toHaveText(`Blocking robots.txt: ${site.base}/robots.txt`);
    // Die blockierende Zeile der Fixture (Zeile 2), formuliert von der Oberfläche.
    await expect(dialog.getByTestId("notify-robots-line")).toHaveText("Blocking rule (line 2): Disallow: /");
    // Kein Fehlschlag (CRAWL-128): Die Statusleiste zeigt „Completed“, der Dialog nicht den Zustand „Failed“.
    await expect(page.getByTestId("crawl-phase")).toHaveText("Completed");
    await expect(dialog).not.toContainText("Failed");
    await scan(page, "start-robots", '[role="dialog"]');

    await dialog.getByRole("button", { name: "Open the “Ignore robots.txt” setting" }).click();
    const ignore = page.getByRole("radio", { name: "Ignore robots.txt", exact: true });
    await expect(ignore).toBeVisible();
    await expect(ignore).not.toBeChecked();
    await expect(page.getByRole("radio", { name: "Respect robots.txt", exact: true })).toBeChecked();
    // Nur ein Hinweis: Die Konfiguration bleibt „robots.txt beachten“.
    const mode = await page.evaluate(async () => {
      const { config } = await import(/* @vite-ignore */ String("/src/ipc/config.ts"));
      return ((await config.get()).config as { robots: { mode: string } }).robots.mode;
    });
    expect(mode).toBe("respect");
  } finally {
    await bridge.stop();
    await site.stop();
  }
});

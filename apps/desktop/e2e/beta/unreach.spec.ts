import { test, expect } from "@playwright/test";
import { createServer } from "node:net";
import { startAlphaBridge } from "../alpha/support";
import { scan, visibleFocus } from "../a11y/fixture";

// B4-UNREACH (UI-019, CRAWL-128): Start-URL auf einem nicht belegten Port über die echte Bridge. Der Kern beendet den Crawl als
// „Fehlgeschlagen“ und meldet die Fehlerklasse; die App zeigt sie im Dialog, per Tastatur bedienbar (UI-126).
async function closedPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = (server.address() as { port: number }).port;
  await new Promise<void>((resolve) => server.close(() => resolve()));
  return port;
}

for (const respect of [true, false]) {
  // spec: UI-019
  // spec: UI-126
  // spec: CRAWL-128
  test(`an unreachable start URL opens a keyboard-operable dialog with the cause (robots.txt ${respect ? "respected" : "ignored"})`, async ({ page }) => {
    const bridge = await startAlphaBridge();
    try {
      const base = `http://127.0.0.1:${await closedPort()}`;
      await page.goto(`/?${new URLSearchParams({ bridge: bridge.url })}`);
      await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
      await page.evaluate(async (respect) => {
        const { config } = await import(/* @vite-ignore */ String("/src/ipc/config.ts"));
        const dto = await config.get(); const root = dto.config as Record<string, Record<string, unknown>>;
        await config.set({ ...dto, config: { ...root, robots: { ...root.robots, retries: 0, mode: respect ? "respect" : "ignore" } } });
      }, respect);
      const field = page.getByLabel("Start URL", { exact: true });
      await field.fill(`${base}/`);
      await field.press("Enter");

      const dialog = page.getByRole("dialog", { name: "Start address not reachable" });
      await expect(dialog).toBeVisible({ timeout: 5_000 });
      await expect(dialog).toContainText("The connection to the start address failed.");
      await expect(dialog.getByTestId("notify-cause")).toHaveText("Cause: Connection refused");
      await expect(dialog).toContainText(respect ? `could not be retrieved, so the start address was not crawled: ${base}/robots.txt` : `Address: ${base}/`);
      await expect(dialog).toContainText("ended as “Failed”");
      await expect(page.getByTestId("crawl-phase")).toHaveText("Failed");

      // Fokus im Dialog; Tab bleibt in der Fokusfalle und zeigt einen sichtbaren Fokusrahmen.
      await expect.poll(() => dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
      const buttons = dialog.getByRole("button");
      const count = await buttons.count();
      expect(count).toBeGreaterThanOrEqual(3);
      for (let i = 0; i < count + 1; i++) {
        await page.keyboard.press("Tab");
        expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
      }
      const close = dialog.getByRole("button", { name: "Close" });
      for (let i = 0; i < count + 1 && !(await close.evaluate((el) => el === document.activeElement)); i++) await page.keyboard.press("Tab");
      await visibleFocus(close);
      await scan(page, `start-unreachable-${respect ? "robots" : "start"}`, '[role="dialog"]');

      // Esc schließt; der Fokus kehrt zum Eingabefeld zurück, von dem aus gestartet wurde.
      await page.keyboard.press("Escape");
      await expect(page.getByRole("dialog")).toHaveCount(0);
      await expect(field).toBeFocused();
      await expect(page.getByTestId("crawl-phase")).toHaveText("Failed");
    } finally { await bridge.stop(); }
  });
}

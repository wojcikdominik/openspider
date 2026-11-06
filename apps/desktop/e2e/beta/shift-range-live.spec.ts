import { test, expect } from "@playwright/test";
import { spawn } from "node:child_process";
import { startAlphaBridge, toolBinary } from "../alpha/support";

// spec: UI-012
// spec: UI-018
// Finding F3 at a rate far above the 2-3 URL/s of the acceptance test: the view version changes with every update while the
// range is resolved. Every Shift+click and every Shift+arrow key must still extend the selection.
test("Shift+click and Shift+arrow extend the selection at 300 URL/s", async ({ page }) => {
  test.setTimeout(120_000);
  const site = spawn(toolBinary("os-testsite"), ["synthetic:n=30000,links=10,seed=7"], { stdio: ["ignore", "pipe", "inherit"] });
  const bridge = await startAlphaBridge();
  try {
    const base = await new Promise<string>((resolve, reject) => {
      let output = "";
      const timer = setTimeout(() => reject(new Error("fixture startup timeout")), 20_000);
      site.stdout.on("data", (data: Buffer) => {
        output += data.toString(); const match = /-> (http:\/\/127\.0\.0\.1:\d+)/.exec(output);
        if (match) { clearTimeout(timer); resolve(match[1]!); }
      });
      site.once("error", reject);
    });
    await page.setViewportSize({ width: 1280, height: 1200 });
    await page.goto(`/?${new URLSearchParams({ bridge: bridge.url })}`);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    await page.evaluate(async url => {
      const { config } = await import(/* @vite-ignore */ String("/src/ipc/config.ts"));
      const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
      const dto = await config.get(); const root = dto.config as Record<string, Record<string, unknown>>;
      await config.set({ ...dto, config: { ...root, speed: { ...root.speed, threads: 8, max_urls_per_sec: 300 }, analysis: { ...root.analysis, auto_at_end: false } } });
      await crawl.start({ source: { kind: "spider", url }, scope: "domain" });
    }, `${base}/`);
    const crawled = () => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return (useSession.getState().crawl?.crawled ?? 0) as number;
    });
    await expect.poll(crawled, { timeout: 30_000 }).toBeGreaterThan(300);
    const canvas = page.locator("#data-panel .os-data-grid canvas").first();
    const rowAt = (row: number) => ({ x: 160, y: 32 + 28 * row + 14 });
    const selected = () => page.locator("#data-panel .os-data-grid [role=row][aria-selected=true]").count();
    const versions = () => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return Object.values(useSession.getState().views).map(v => (v as { version: number }).version).join();
    });
    await canvas.click({ position: rowAt(1) });
    await expect.poll(selected).toBe(1);
    const before = await versions();
    for (const row of [3, 6, 2, 9, 5, 12, 7, 14, 4, 10]) {
      await canvas.click({ position: rowAt(row), modifiers: ["Shift"] });
      await expect.poll(selected, { timeout: 2000, message: `Shift+click on row ${row}` }).toBe(row);
      await page.waitForTimeout(150);
    }
    expect(await versions(), "the view version changes during the test").not.toBe(before);
    expect(await crawled()).toBeLessThan(30_000);
    // Keys in quick succession: each one continues from the previous row, none waits for an answer.
    await canvas.click({ position: rowAt(1) });
    await expect.poll(selected).toBe(1);
    for (let i = 0; i < 12; i++) await page.keyboard.press("Shift+ArrowDown");
    await expect.poll(selected, { timeout: 3000, message: "12 Shift+ArrowDown" }).toBe(13);
    await page.evaluate(async () => {
      const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
      await crawl.stop();
    });
  } finally {
    await bridge.stop();
    await new Promise<void>(resolve => { if (site.exitCode !== null) resolve(); else { site.once("exit", () => resolve()); site.kill(); } });
  }
});

import { createServer } from "node:http";
import { test, expect } from "@playwright/test";
import { startBridge } from "../support/bridge";

// spec: UI-018
// spec: UI-114
test("real bridge: crawl, recrawl a changed page, then remove the URL and update the counts", async ({ page }) => {
  // Integration also requires K01 views and K04 crawl control. N01 resolves this C9 placeholder.
  const frames: string[] = [];
  page.on("websocket", socket => {
    socket.on("framesent", event => frames.push(`send ${event.payload}`));
    socket.on("framereceived", event => frames.push(`receive ${event.payload}`));
    socket.on("socketerror", error => frames.push(`error ${error}`));
  });
  const bridge = await startBridge();
  expect(bridge).not.toBeNull();
  let title = "Initial title";
  const site = createServer((_request, response) => { response.setHeader("Content-Type", "text/html"); response.end(`<html><head><title>${title}</title></head><body>Fixture</body></html>`); });
  await new Promise<void>(resolve => site.listen(0, "127.0.0.1", resolve));
  const address = site.address(); if (!address || typeof address === "string") throw new Error("Fixture server has no port");
  try {
    await page.goto(`/?bridge=${encodeURIComponent(bridge!.url)}`);
    await page.evaluate(async url => {
      const modulePath = "/src/ipc/crawl.ts";
      const { crawl } = await import(modulePath);
      await crawl.start({ source: { kind: "list", items: [url], label: "u10-contract" }, scope: "domain" });
    }, `http://127.0.0.1:${address.port}/`);
    // The Windows runner needs a little over 5 s for crawl, analysis and view rebuild (Runde 11).
    await expect(page.getByText("Showing: 1 of 1", { exact: true })).toBeVisible({ timeout: 20_000 });
    // The automatic analysis after the crawl rebuilds the view and drops a selection made before it has finished.
    await expect(page.getByTestId("analysis-feedback")).toHaveText("Analysis completed", { timeout: 20_000 });
    const canvas = page.locator(".os-data-grid");
    const row = canvas.locator('[role=row][aria-rowindex="2"]');
    await expect(row).toHaveAttribute("aria-busy", "false");
    await canvas.click({ position: { x: 225, y: 43 } });
    await expect(row).toHaveAttribute("aria-selected", "true");
    title = "Changed title";
    await page.keyboard.press("Shift+F10");
    await page.getByRole("menuitem", { name: "Recrawl", exact: true }).click();
    await expect(page.getByText("Crawl analysis outdated")).toBeVisible();
    await expect(page.getByText("Complete", { exact: true })).toBeVisible();
    await expect.poll(() => page.evaluate(async () => {
      const modulePath = "/src/ipc/view.ts";
      const { view } = await import(modulePath);
      const handle = await view.open({ spec: { kind: "tab", tab: "Internal", filter: "All", sort: [] }, columns: ["Title 1"] });
      try {
        const result = await view.rows({ viewId: handle.viewId, version: handle.version, offset: 0, limit: 1, columns: ["Title 1"] });
        return result.cells[0]?.[0];
      } finally { await view.close({ viewId: handle.viewId }); }
    })).toBe("Changed title");
    await page.keyboard.press("Shift+F10");
    await page.getByRole("menuitem", { name: "Remove", exact: true }).click();
    await page.getByRole("button", { name: "Remove", exact: true }).click();
    await expect(page.getByText("Showing: 0 of 0", { exact: true })).toBeVisible();
  } finally {
    await test.info().attach("bridge-diagnostics", { body: `stderr:\n${bridge?.stderr?.() ?? ""}\nwebsocket:\n${frames.join("\n")}`, contentType: "text/plain" });
    await bridge?.stop(); await new Promise<void>(resolve => site.close(() => resolve())); }
});

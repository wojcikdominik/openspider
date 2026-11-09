import { test, expect } from "@playwright/test";
import { startAlphaBridge } from "../alpha/support";

// spec: UI-129
test("real app lists MCP tools while stopped and keeps whole column headings", async ({ page }) => {
  const bridge = await startAlphaBridge();
  try {
    await page.goto(`/?${new URLSearchParams({ bridge: bridge.url })}`);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    const status = await page.evaluate(async () => {
      const { mcp } = await import(/* @vite-ignore */ String("/src/ipc/mcp.ts"));
      const { openMcp } = await import(/* @vite-ignore */ String("/src/features/mcp/model.ts"));
      const status = await mcp.status(); openMcp("tools"); return status.kind;
    });
    expect(status).toBe("off");
    const table = page.getByRole("table", { name: "Tools", exact: true });
    await expect(table.locator("tbody tr")).toHaveCount(17); // AGENT-020: `pause_crawl` kam mit beta.4 dazu.
    for (const header of await table.locator("th").all()) {
      expect(await header.evaluate(el => getComputedStyle(el).whiteSpace)).toBe("nowrap");
    }
    await expect(table.locator("td:nth-child(2) code").filter({ hasText: /^query_urls$/ })).toBeVisible();
    await expect(table).toBeVisible();
  } finally { await bridge.stop(); }
});

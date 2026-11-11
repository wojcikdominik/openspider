import { createServer } from "node:http";
import { test, expect, type Page } from "@playwright/test";
import { startBridge } from "../support/bridge";
import type { CrawlListing } from "../../src/ipc/gen/CrawlListing";
import type {} from "./harness";

// B4-CMPUI (beta.3 report, window ~2000 px): the comparison must use the table region like the ordinary table, show the
// tab's columns plus the comparison status, scroll one virtualized population and leave no stale I/O error behind.
async function crawlTwice(page: Page, next: () => void): Promise<string[]> {
  const ids: string[] = [];
  for (let i = 0; i < 2; i++) {
    if (i) next();
    await page.evaluate(async () => { await window.compareHarness.invoke("crawl.start", { source: { kind: "spider", url: new URLSearchParams(location.search).get("site") }, scope: "domain" }); });
    await expect.poll(async () => {
      const rows = await page.evaluate(() => window.compareHarness.invoke<CrawlListing[]>("crawls.list", { show: "crawls" }));
      return rows.length === i + 1 && rows.every(r => r.crawl.phase === "completed");
    }, { timeout: 30_000 }).toBe(true);
    const rows = await page.evaluate(() => window.compareHarness.invoke<CrawlListing[]>("crawls.list", { show: "crawls" }));
    ids.push(rows.find(r => !ids.includes(r.crawl.crawlId))!.crawl.crawlId);
    await expect.poll(() => page.evaluate(async () => {
      try { await window.compareHarness.invoke("crawl.clear"); return true; } catch (e) { if ((e as { code: string }).code === "Busy") return false; throw e; }
    }), { timeout: 30_000 }).toBe(true);
  }
  return ids;
}

// spec: OUT-140
// spec: OUT-142
// spec: OUT-151
// spec: UI-007
// spec: UI-140
// spec: CRAWL-127
test("comparison fills the table region, shows the comparison columns, scrolls virtually and reports no file error", async ({ page }) => {
  test.setTimeout(150_000);
  let version = 1;
  // 30 pages in version 1, 40 in version 2: more rows than the former 100-row page would need to be irrelevant here,
  // the point is that every row is reachable in one grid without page buttons.
  const pages = () => version === 1 ? 30 : 40;
  const server = createServer((req, res) => {
    const url = req.url ?? "/";
    res.setHeader("content-type", url === "/robots.txt" ? "text/plain" : "text/html");
    if (url === "/robots.txt") { res.end("User-agent: *\nAllow: /\n"); return; }
    const links = url === "/start" ? Array.from({ length: pages() }, (_, i) => `<a href="/p${i}">P${i}</a>`).join("") : "";
    res.end(`<!doctype html><title>${url} v${version}</title><h1>Fixture</h1>${links}`);
  });
  await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
  const bridge = await startBridge(); expect(bridge, "compiled real core bridge required").not.toBeNull();
  try {
    const port = (server.address() as { port: number }).port;
    await page.goto(`/e2e/compare/index.html?bridge=${encodeURIComponent(bridge!.url)}&site=${encodeURIComponent(`http://127.0.0.1:${port}/start`)}`);
    await page.waitForFunction(() => !!window.compareHarness);
    await crawlTwice(page, () => { version = 2; });

    // The user's window: about 2000 px wide.
    await page.setViewportSize({ width: 2000, height: 1100 });
    await page.goto(`/?bridge=${encodeURIComponent(bridge!.url)}`);
    await page.getByRole("button", { name: "Compare crawls", exact: true }).click();
    await expect(page.getByRole("button", { name: "Compare", exact: true })).toBeEnabled();
    await page.getByRole("button", { name: "Compare", exact: true }).click();
    const region = page.locator('[data-region="table"]');
    const grid = region.getByRole("grid", { name: "Comparison table", exact: true });
    await expect(grid).toBeVisible({ timeout: 30_000 });

    // 1. Layout: the comparison takes the whole table region, its tool row sits top left (like `.u10-table`).
    const area = (await region.boundingBox())!;
    const table = (await region.locator(".os-compare-table").boundingBox())!;
    const box = (await grid.boundingBox())!;
    const filter = (await region.getByLabel("Filter", { exact: true }).boundingBox())!;
    expect(area.width).toBeGreaterThan(1000);
    expect(table.width).toBeGreaterThanOrEqual(area.width - 2);
    expect(table.height).toBeGreaterThanOrEqual(area.height - 2);
    expect(box.width).toBeGreaterThanOrEqual(area.width - 2);
    // Only the tool row and the counter line are not grid.
    expect(box.height).toBeGreaterThanOrEqual(area.height - 140);
    expect(filter.x - area.x).toBeLessThan(160);
    expect(filter.y - area.y).toBeLessThan(60);
    // The overview in the sidebar uses the full sidebar height as well.
    const sidebar = (await page.locator(".os-app-sidebar").boundingBox())!;
    const numbers = (await page.locator(".os-app-sidebar .os-compare-sidebar").boundingBox())!;
    expect(numbers.height).toBeGreaterThanOrEqual(sidebar.height - 2);

    // 2. Columns: Address, then the comparison status, then the tab's ordinary default columns (catalog of the core).
    const headers = await grid.getByRole("columnheader").allTextContents();
    expect(headers.slice(0, 2)).toEqual(["Address", "Comparison Status"]);
    expect(headers).toContain("Status Code");
    expect(headers.filter(h => h === "Comparison Status")).toHaveLength(1);

    // 4. Virtualization: no page buttons; the grid announces the complete population, the counter names it.
    await expect(region.getByRole("button", { name: /^(Next|Previous) page$/ })).toHaveCount(0);
    const counter = await region.locator(".os-compare-counter").textContent();
    const total = Number(/(\d+)\s+rows/.exec(counter ?? "")?.[1]);
    expect(total).toBeGreaterThan(40);
    await expect(grid).toHaveAttribute("aria-rowcount", String(total + 1));

    // Round 2 (beta.4): the same grouped tab bar as the ordinary mode, Change Detection as its own entry, mode „Comparison“.
    await expect(page.getByRole("group", { name: "Areas" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Change Detection", exact: true })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Internal", exact: true })).toHaveAttribute("aria-selected", "true");
    await expect(page.getByText("Mode: Comparison", { exact: true })).toBeVisible();
    await expect(page.locator(".os-header-mode select")).toHaveValue("compare");
    // The overview shows every count column (Removed/Missing) or says that it scrolls sideways.
    const missingHead = (await page.locator(".os-app-sidebar").getByRole("columnheader", { name: "Missing", exact: true }).boundingBox())!;
    const hint = page.locator(".os-app-sidebar").getByTestId("compare-scroll-hint");
    if (missingHead.x + missingHead.width > sidebar.x + sidebar.width + 1) await expect(hint).toBeVisible();
    else await expect(hint).toHaveCount(0);

    // 3. No file error appears for a comparison of two valid crawls.
    await expect(page.getByTestId("crawl-open-error")).toHaveCount(0);
    await expect(page.getByRole("alert").filter({ hasText: /file operation|Dateivorgang|os error/i })).toHaveCount(0);

    // CRAWL-127: a vanished file (stale "Recent" entry) is NotFound with its path, not a generic I/O failure. Checked last:
    // the failure notice would otherwise open the crawl-notice dialog over the main window.
    await page.goto(`/e2e/compare/index.html?bridge=${encodeURIComponent(bridge!.url)}`);
    await page.waitForFunction(() => !!window.compareHarness);
    const missing = await page.evaluate(async () => {
      try { await window.compareHarness.invoke("crawl.open", { kind: "path", path: "/os-b4-cmpui-missing/gone.ospider" }); return null; }
      catch (e) { const error = (e as { cmdError: { code: string; messageKey: string; args: Record<string, unknown> } }).cmdError; return { code: error.code, messageKey: error.messageKey, path: error.args?.path }; }
    });
    expect(missing).toEqual({ code: "NotFound", messageKey: "err-file-not-found", path: expect.stringContaining("gone.ospider") });
    // The main window reports it once, in the status bar, as one sentence with the file; no modal with the raw code.
    await page.goto(`/?bridge=${encodeURIComponent(bridge!.url)}`);
    await expect(page.getByTestId("crawl-open-error-text")).toHaveText("The file was not found: /os-b4-cmpui-missing/gone.ospider");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(page.getByText(/crawl-open-failed/)).toHaveCount(0);
    // The technical details open as a readable card above the status bar, fully inside the window.
    await page.getByTestId("crawl-open-error").locator("summary").click();
    const detail = page.getByTestId("crawl-open-error-detail");
    await expect(detail).toBeVisible();
    const card = (await detail.boundingBox())!, bar = (await page.locator(".os-app-status").boundingBox())!, view = page.viewportSize()!;
    expect(card.height).toBeGreaterThanOrEqual(24);
    expect(card.y).toBeGreaterThanOrEqual(0);
    expect(card.x).toBeGreaterThanOrEqual(0);
    expect(card.x + card.width).toBeLessThanOrEqual(view.width);
    expect(card.y + card.height).toBeLessThanOrEqual(bar.y + 1);
    const text = (await detail.locator("pre").boundingBox())!;
    expect(text.y + text.height).toBeLessThanOrEqual(card.y + card.height);
  } finally { await bridge?.stop(); await new Promise<void>(resolve => server.close(() => resolve())); }
});

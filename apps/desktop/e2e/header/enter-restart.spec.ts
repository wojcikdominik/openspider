import { createServer } from "node:http";
import type { Page } from "@playwright/test";
import { test, expect } from "../fixtures/app";

// B4-ENTER: In WebKit (≈ WKWebView) the Enter key in the start URL field did not start a second crawl. The start URL of
// the first crawl lands in the field's suggestion list (<datalist>); from then on WebKit hands Enter to its suggestion
// picker, marks the keydown as handled and fires no `keypress`, so the form was never submitted implicitly. Only real
// key events show this, hence `page.keyboard` throughout and no `fill`. Fresh state: the first crawl must start with an
// empty suggestion list, like on a first launch.
test.use({ freshState: true });

/** Clears the focused field and types `text` with real key events. */
async function retype(page: Page, text: string) {
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.press("Backspace");
  await page.keyboard.type(text);
}

// spec: UI-003
// spec: UI-113
test("Enter in the start URL field starts the next crawl after a finished one", async ({ app, page }) => {
  // Real crawls through the core; the mock transport has no crawl lifecycle (like header.spec.ts › CRAWL-001).
  test.skip(app.mode !== "bridge", "needs os-app-bridge");
  test.setTimeout(90_000);
  const hits: string[] = [];
  const server = createServer((req, res) => {
    hits.push(req.url ?? "");
    res.writeHead(req.url === "/robots.txt" ? 404 : 200, { "Content-Type": "text/html" });
    res.end(req.url === "/" || req.url === "/second/" ? `<title>Root</title><a href="/a">a</a><a href="/b">b</a>` : `<title>Leaf</title>`);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Fixture address missing");
  const first = `http://127.0.0.1:${address.port}/`;
  const second = `http://127.0.0.1:${address.port}/second/`;
  // Start URLs the core actually fetched, in order.
  const started = () => [...new Set(hits.filter((h) => h === "/" || h === "/second/"))].map((h) => `http://127.0.0.1:${address.port}${h}`);
  try {
    await app.open(page, "main");
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    const field = page.getByLabel("Start URL");
    const phase = page.getByTestId("crawl-header").locator("[data-phase]");

    // Crawl 1: focus the field, type, Enter.
    await field.click();
    await page.keyboard.type(first);
    await page.keyboard.press("Enter");
    await expect(phase).toHaveAttribute("data-phase", "completed", { timeout: 30_000 });
    expect(started()).toEqual([first]);
    // The first start URL is now a suggestion of the field (UI-003: last 20 start URLs).
    await expect(page.locator("#os-start-history option")).toHaveCount(1);

    // Crawl 2: new address in the same field, Enter. A finished crawl is replaced after the confirmation (UI-003).
    await field.click();
    await retype(page, second);
    await expect(field).toHaveValue(second);
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "Replace", exact: true }).click();
    await expect.poll(started, { timeout: 30_000 }).toEqual([first, second]);
    await expect(phase).toHaveAttribute("data-phase", "completed", { timeout: 30_000 });

    // Enter on the unchanged address asks again; Escape cancels without starting and keeps the field.
    await field.click();
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(field).toHaveValue(second);
    expect(started()).toEqual([first, second]);
  } finally { await new Promise<void>((resolve, reject) => server.close((e) => e ? reject(e) : resolve())); }
});

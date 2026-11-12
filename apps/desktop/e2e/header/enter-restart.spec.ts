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
    const crawlId = () => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return (useSession.getState() as { crawl: { crawlId: string } | null }).crawl?.crawlId ?? null;
    });

    // Crawl 1: focus the field, type, Enter.
    await field.click();
    await page.keyboard.type(first);
    await page.keyboard.press("Enter");
    await expect(phase).toHaveAttribute("data-phase", "completed", { timeout: 30_000 });
    expect(started()).toEqual([first]);
    // The first start URL is now a suggestion of the field (UI-003: last 20 start URLs).
    await expect(page.locator("#os-start-history option")).toHaveCount(1);
    const firstId = await crawlId();
    expect(firstId).toBeTruthy();

    // Crawl 2: new address in the same field, Enter. A finished crawl is replaced after the confirmation (UI-003).
    await field.click();
    await retype(page, second);
    await expect(field).toHaveValue(second);
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "Replace", exact: true }).click();
    await expect.poll(started, { timeout: 30_000 }).toEqual([first, second]);
    // The header shows crawl 2 only from its first status tick on (UI-018); until then "completed" is still crawl 1's.
    await expect.poll(crawlId, { timeout: 30_000 }).not.toBe(firstId);
    await expect(phase).toHaveAttribute("data-phase", "completed", { timeout: 30_000 });

    // Enter on the unchanged address asks again; Escape cancels without starting and keeps the field.
    await field.click();
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    // UI-126: the focus returns to the field after the question closes; only then is Enter the field's again.
    await expect(field).toBeFocused();
    await expect(field).toHaveValue(second);
    expect(started()).toEqual([first, second]);

    // UI-113 (OS-Festlegung 09.11.2026): Enter in the open question is its default action („Replace“).
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect.poll(() => hits.filter((h) => h === "/second/").length, { timeout: 30_000 }).toBe(2);
    await expect(phase).toHaveAttribute("data-phase", "completed", { timeout: 30_000 });
  } finally { await new Promise<void>((resolve, reject) => server.close((e) => e ? reject(e) : resolve())); }
});

// spec: UI-113
// spec: UI-003
test("Enter in the URL field and Mod+Enter resume a paused crawl without confirmation or a new crawl; a changed URL starts anew", async ({ app, page }) => {
  test.skip(app.mode !== "bridge", "needs os-app-bridge");
  test.setTimeout(120_000);
  // Mod = Ctrl: a Windows platform makes the shortcut independent of the host running the engines.
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "platform", { value: "Win32" });
    Object.defineProperty(navigator, "userAgentData", { value: { platform: "Windows" } });
  });
  // Endless fixture like header.spec.ts › CRAWL-001: "/" (and "/other/") link /p/0 … /p/99, every /p/<n> links /p/<n+100>.
  const hits: string[] = [];
  const server = createServer((req, res) => {
    hits.push(req.url ?? "");
    setTimeout(() => {
      res.writeHead(200, { "Content-Type": "text/html" });
      const n = /^\/p\/(\d+)$/.exec(req.url ?? "");
      res.end(req.url === "/" || req.url === "/other/"
        ? Array.from({ length: 100 }, (_, i) => `<a href="/p/${i}">${i}</a>`).join("")
        : `<title>Fixture page</title>${n ? `<a href="/p/${Number(n[1]) + 100}">next</a>` : ""}`);
    }, 200);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Fixture address missing");
  const url = `http://127.0.0.1:${address.port}/`;
  try {
    await app.open(page, "main");
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    const header = page.getByTestId("crawl-header");
    const field = page.getByLabel("Start URL");
    const phase = header.locator("[data-phase]");
    const crawlId = () => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return (useSession.getState() as { crawl: { crawlId: string } | null }).crawl?.crawlId ?? null;
    });
    const pause = async () => {
      await header.getByRole("button", { name: "Pause", exact: true }).click();
      await expect(phase).toHaveAttribute("data-phase", "paused", { timeout: 20_000 });
    };

    await field.click();
    await page.keyboard.type(url);
    await page.keyboard.press("Enter");
    await expect(phase).toHaveAttribute("data-phase", "running", { timeout: 20_000 });
    const id = await crawlId();
    expect(id).toBeTruthy();

    // Enter in the field with the unchanged start URL resumes.
    await pause();
    await field.click();
    await page.keyboard.press("End");
    await page.keyboard.press("Enter");
    await expect(phase).toHaveAttribute("data-phase", "running", { timeout: 20_000 });
    await expect(page.getByRole("dialog")).toHaveCount(0);
    expect(await crawlId()).toBe(id);

    // Mod+Enter resumes as well. It is a global shortcut, so the focus leaves the text field first (UI-113 context
    // rule: in a text field only Esc, Enter, Mod+F and F6 act globally).
    await pause();
    await field.click();
    await field.blur();
    expect(await page.evaluate(() => document.activeElement?.tagName)).toBe("BODY");
    await page.keyboard.press("Control+Enter");
    await expect(phase).toHaveAttribute("data-phase", "running", { timeout: 20_000 });
    await expect(page.getByRole("dialog")).toHaveCount(0);
    expect(await crawlId()).toBe(id);
    // The start URL was fetched once: no second crawl began.
    expect(hits.filter((h) => h === "/")).toHaveLength(1);

    // A changed address takes the ordinary start path: confirmation, then a new crawl.
    await pause();
    await field.click();
    await page.keyboard.press("ControlOrMeta+a");
    await page.keyboard.type(`${url}other/`);
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "Replace", exact: true }).click();
    await expect.poll(() => hits.includes("/other/"), { timeout: 20_000 }).toBe(true);
    await expect.poll(crawlId, { timeout: 20_000 }).not.toBe(id);
    await pause();
  } finally { await new Promise<void>((resolve, reject) => server.close((e) => e ? reject(e) : resolve())); }
});

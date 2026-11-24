import { createServer } from "node:http";
import { test, expect, type Page } from "@playwright/test";
import { startBridge } from "../support/bridge";

/**
 * fix-sel (1.0.2): a short crawl reports its status to the window first together with its end and the automatic
 * analysis. A row selected in the table before that (the table already shows the crawl's rows) was dropped when the
 * status arrived: the table took the first crawl id for a new crawl, emptied the selection and opened the view anew.
 * The test holds back the bridge's events (ticks, notices) while the core crawls and analyses, so the click lands
 * before the window knows of crawl end and analysis end, then delivers them in their original order.
 */
async function holdEvents(page: Page, bridgeUrl: string) {
  const state = { hold: false, queue: [] as Array<() => void>, holdStart: false, starts: [] as Array<() => void> };
  const port = new URL(bridgeUrl).port;
  await page.routeWebSocket(new RegExp(`^ws://127\\.0\\.0\\.1:${port}/`), ws => {
    const server = ws.connectToServer();
    ws.onMessage(message => {
      // `crawl.start` requests can be held before the core, to look at the window while the begin is outstanding.
      if (state.holdStart && typeof message === "string" && message.includes('"cmd":"crawl.start"')) state.starts.push(() => server.send(message));
      else server.send(message);
    });
    server.onMessage(message => {
      // Answers to requests pass; events ({"event": …}) wait while held, in their order.
      let event = false;
      try { event = typeof message === "string" && typeof (JSON.parse(message) as { event?: unknown }).event === "string"; } catch { event = false; }
      if (state.hold && event) state.queue.push(() => ws.send(message)); else ws.send(message);
    });
  });
  return {
    hold() { state.hold = true; },
    release() { state.hold = false; for (const send of state.queue.splice(0)) send(); },
    holdStart() { state.holdStart = true; },
    heldStarts: () => state.starts.length,
    releaseStart() { state.holdStart = false; for (const send of state.starts.splice(0)) send(); },
  };
}

/** Opens the table's view anew (tab External, then Internal), each switch waiting until its view is installed. */
async function reopenTable(page: Page) {
  const viewId = () => page.evaluate(async () => {
    const { useTableSelection } = await import(/* @vite-ignore */ String("/src/features/table/selection.ts"));
    return useTableSelection.getState().viewId ?? null;
  });
  for (const tab of ["External", "Internal"]) {
    const before = await viewId();
    await page.evaluate(async tab => {
      const { useTabs } = await import(/* @vite-ignore */ String("/src/state/tabs.ts"));
      useTabs.getState().select(tab);
    }, tab);
    await expect.poll(viewId, { timeout: 20_000 }).not.toBe(before);
  }
}

// spec: UI-018
// spec: UI-142
// spec: UI-030
// spec: UI-110
test("a row selected before the window learns of crawl end and analysis end stays selected", async ({ page }) => {
  test.setTimeout(60_000);
  const bridge = await startBridge();
  expect(bridge).not.toBeNull();
  const site = createServer((_request, response) => { response.setHeader("Content-Type", "text/html"); response.end("<html><head><title>Selection</title></head><body>Fixture</body></html>"); });
  await new Promise<void>(resolve => site.listen(0, "127.0.0.1", resolve));
  const address = site.address(); if (!address || typeof address === "string") throw new Error("Fixture server has no port");
  const url = `http://127.0.0.1:${address.port}/`;
  try {
    const events = await holdEvents(page, bridge!.url);
    await page.goto(`/?bridge=${encodeURIComponent(bridge!.url)}`);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    // The table has asked the core once and knows there is no crawl yet (its first view request is answered).
    await expect(page.locator('.u10-grid-host [data-empty-state="NO_DATA_YET"]')).toBeVisible({ timeout: 20_000 });
    events.hold();
    await page.evaluate(async url => {
      const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
      await crawl.start({ source: { kind: "list", items: [url], label: "fix-sel" }, scope: "domain" });
    }, url);
    // The core has crawled and analysed; the window has not heard of it yet.
    await expect.poll(() => page.evaluate(async () => {
      const { analysis } = await import(/* @vite-ignore */ String("/src/ipc/analysis.ts"));
      const status = await analysis.status();
      return !status.running && status.steps.length > 0;
    }), { timeout: 20_000 }).toBe(true);
    // The table opens its view (as when the window loads during a crawl) and shows the crawled row.
    await reopenTable(page);
    // Without the crawl status the footer's total may still read "–"; the loaded row is what the user clicks.
    const canvas = page.locator(".os-data-grid");
    const row = canvas.locator('[role=row][aria-rowindex="2"]');
    await expect(row).toHaveAttribute("aria-busy", "false", { timeout: 20_000 });
    await canvas.click({ position: { x: 225, y: 43 } });
    await expect(row).toHaveAttribute("aria-selected", "true");
    const details = page.getByRole("region", { name: "Details", exact: true });
    await expect(details).toContainText(url);
    await expect(page.getByTestId("analysis-feedback")).toHaveCount(0);
    const viewBefore = await page.evaluate(async () => {
      const { useTableSelection } = await import(/* @vite-ignore */ String("/src/features/table/selection.ts"));
      return useTableSelection.getState().viewId;
    });
    expect(viewBefore).toBeTruthy();

    events.release();
    await expect(page.getByTestId("analysis-feedback")).toHaveText("Analysis completed", { timeout: 20_000 });
    // The view is rebuilt for the crawl the window now knows; wait until the new view is installed.
    await expect.poll(() => page.evaluate(async () => {
      const { useTableSelection } = await import(/* @vite-ignore */ String("/src/features/table/selection.ts"));
      return useTableSelection.getState().viewId;
    }), { timeout: 20_000 }).not.toBe(viewBefore);
    await expect(row).toHaveAttribute("aria-busy", "false");
    await expect(row).toHaveAttribute("aria-selected", "true");
    await expect(page.getByText("Showing: 1 of 1 · 1 selected", { exact: true })).toBeVisible();
    await expect(details).toContainText(url);
    await page.keyboard.press("Shift+F10");
    await expect(page.getByRole("menuitem", { name: "Recrawl", exact: true })).toBeEnabled();
    await expect(page.getByRole("menuitem", { name: "Remove", exact: true })).toBeEnabled();
    await page.keyboard.press("Escape");
  } finally {
    await bridge?.stop(); await new Promise<void>(resolve => site.close(() => resolve()));
  }
});

// spec: UI-018
// spec: UI-110
test("beginning another crawl ends a selection in rows whose crawl the window never heard of", async ({ page }) => {
  test.setTimeout(60_000);
  const bridge = await startBridge();
  expect(bridge).not.toBeNull();
  const site = createServer((request, response) => { response.setHeader("Content-Type", "text/html"); response.end(`<html><head><title>${request.url}</title></head><body>Fixture</body></html>`); });
  await new Promise<void>(resolve => site.listen(0, "127.0.0.1", resolve));
  const address = site.address(); if (!address || typeof address === "string") throw new Error("Fixture server has no port");
  const urlA = `http://127.0.0.1:${address.port}/a`;
  const urlB = `http://127.0.0.1:${address.port}/b`;
  const begin = (url: string) => page.evaluate(async url => {
    const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
    return (await crawl.start({ source: { kind: "list", items: [url], label: "fix-sel" }, scope: "domain" })).crawlId as string;
  }, url);
  try {
    const events = await holdEvents(page, bridge!.url);
    await page.goto(`/?bridge=${encodeURIComponent(bridge!.url)}`);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    // The table has asked the core once and knows there is no crawl yet (its first view request is answered).
    await expect(page.locator('.u10-grid-host [data-empty-state="NO_DATA_YET"]')).toBeVisible({ timeout: 20_000 });
    events.hold();
    await begin(urlA);
    await expect.poll(() => page.evaluate(async () => {
      const { analysis } = await import(/* @vite-ignore */ String("/src/ipc/analysis.ts"));
      const status = await analysis.status();
      return !status.running && status.steps.length > 0;
    }), { timeout: 20_000 }).toBe(true);
    await reopenTable(page);
    const canvas = page.locator(".os-data-grid");
    const row = canvas.locator('[role=row][aria-rowindex="2"]');
    await expect(row).toHaveAttribute("aria-busy", "false", { timeout: 20_000 });
    await canvas.click({ position: { x: 225, y: 43 } });
    await expect(row).toHaveAttribute("aria-selected", "true");
    const details = page.getByRole("region", { name: "Details", exact: true });
    await expect(details).toContainText(urlA);

    // Crawl B begins in this window while A's status has still not arrived: the selection ends at once.
    const idB = await begin(urlB);
    await expect(row).toHaveAttribute("aria-selected", "false");
    await expect(details).not.toContainText(urlA);
    await expect(page.getByText(/ selected$/)).toHaveCount(0);

    events.release();
    await expect.poll(() => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return useSession.getState().crawl?.crawlId;
    }), { timeout: 20_000 }).toBe(idB);
    await expect(row).toHaveAttribute("aria-busy", "false", { timeout: 20_000 });
    await expect(canvas).toContainText(urlB);
    await expect(row).toHaveAttribute("aria-selected", "false");
    await expect(details).not.toContainText(urlA);
    await expect(details).not.toContainText(urlB);
    await expect(page.getByText(/ selected$/)).toHaveCount(0);
  } finally {
    await bridge?.stop(); await new Promise<void>(resolve => site.close(() => resolve()));
  }
});

// spec: UI-018
// spec: UI-114
test("from the moment crawl.start is sent the old rows are out of reach, and no row action reaches the new crawl", async ({ page }) => {
  test.setTimeout(60_000);
  const bridge = await startBridge();
  expect(bridge).not.toBeNull();
  const site = createServer((request, response) => { response.setHeader("Content-Type", "text/html"); response.end(`<html><head><title>${request.url}</title></head><body>Fixture</body></html>`); });
  await new Promise<void>(resolve => site.listen(0, "127.0.0.1", resolve));
  const address = site.address(); if (!address || typeof address === "string") throw new Error("Fixture server has no port");
  const base = `http://127.0.0.1:${address.port}`;
  const items = (prefix: string) => Array.from({ length: 4 }, (_, i) => `${base}/${prefix}${i + 1}`);
  const total = () => page.evaluate(async () => {
    const { view } = await import(/* @vite-ignore */ String("/src/ipc/view.ts"));
    const handle = await view.open({ spec: { kind: "tab", tab: "Internal", filter: "All", search: null, sort: [] }, columns: ["Address"] });
    try { return handle.total as number; } finally { await view.close({ viewId: handle.viewId }); }
  });
  try {
    const events = await holdEvents(page, bridge!.url);
    await page.goto(`/?bridge=${encodeURIComponent(bridge!.url)}`);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    // The table has asked the core once and knows there is no crawl yet (its first view request is answered).
    await expect(page.locator('.u10-grid-host [data-empty-state="NO_DATA_YET"]')).toBeVisible({ timeout: 20_000 });
    const start = (urls: string[]) => page.evaluate(async urls => {
      const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
      return (await crawl.start({ source: { kind: "list", items: urls, label: "fix-sel" }, scope: "domain" })).crawlId as string;
    }, urls);
    const first = await start(items("z"));
    await expect.poll(() => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      const s = useSession.getState(); return s.crawl?.phase === "completed" && s.analysis?.running === false;
    }), { timeout: 30_000 }).toBe(true);
    expect(first).toBeTruthy();
    await expect(page.getByText("Showing: 4 of 4", { exact: true })).toBeVisible({ timeout: 20_000 });
    const grid = page.locator(".os-data-grid");
    await grid.click({ position: { x: 225, y: 71 } });
    await expect(grid.locator('[role=row][aria-selected="true"]')).toHaveCount(1);

    // crawl.start for crawl 2 is sent but held before the core: the old rows are hidden at once.
    events.holdStart(); events.hold();
    const second = start(items("a"));
    await expect.poll(() => events.heldStarts()).toBe(1);
    await expect(grid).toBeHidden();
    await expect(page.locator('.u10-grid-host [data-empty-state="NO_DATA_YET"]')).toBeVisible();
    await expect(grid.locator('[role=row][aria-selected="true"]')).toHaveCount(0);
    await expect(page.getByText(/ selected$/)).toHaveCount(0);
    await expect(page.getByRole("region", { name: "Details", exact: true })).not.toContainText(`${base}/z`);
    // Neither the context menu key nor the actions reach a row.
    await page.keyboard.press("Shift+F10");
    await expect(page.getByRole("menuitem", { name: "Remove", exact: true })).toHaveCount(0);
    expect(await page.evaluate(async () => {
      const { runAction } = await import(/* @vite-ignore */ String("/src/ui/actions.ts"));
      return runAction("context.menu");
    })).toBe(false);

    events.releaseStart();
    const idB = await second;
    await expect.poll(async () => (await total()), { timeout: 30_000 }).toBe(4);
    events.release();
    await expect.poll(() => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return useSession.getState().crawl?.crawlId;
    }), { timeout: 20_000 }).toBe(idB);
    // Rows come back only from crawl 2, unselected; crawl 2 keeps all its URLs.
    await expect(grid).toBeVisible();
    await expect(grid).toContainText(`${base}/a1`);
    await expect(grid).not.toContainText(`${base}/z1`);
    await expect(grid.locator('[role=row][aria-selected="true"]')).toHaveCount(0);
    expect(await total()).toBe(4);
  } finally {
    await bridge?.stop(); await new Promise<void>(resolve => site.close(() => resolve()));
  }
});

// Abnahme fix-sel (unabhängiger Testagent, nur aus Anforderung und Spec UI-018/UI-142/UI-114): Auswahl der oberen Tabelle
// gegen die echte Bridge. Zwischen Seite und Bridge sitzt ein WebSocket-Proxy (`page.routeWebSocket`), der Crawl-Status
// und Analysestatus aus den Ticks zurückhält (oder `crawl.start` vor dem Kern zurückhält) und gezielt freigibt; keine
// festen Wartezeiten.
import { createServer, type Server } from "node:http";
import { test, expect, type Page, type WebSocketRoute } from "@playwright/test";
import { startBridge, type Bridge } from "../support/bridge";

type Json = Record<string, unknown>;
type CrawlState = { opsRunning: number; opsDone: number; crawlId: string | null; phase: string | null; analysisRunning: boolean | null; analysisSteps: number; ids: number[] | "all"; focus: number | null; viewId: string | null; detail: number[] };

/** WebSocket proxy between page and bridge. */
class StatusGate {
  /** Ticks reach the page without `crawl` and `analysis`. */
  holdStatus = false;
  /** Requests of this command wait here instead of reaching the core (`crawl.start`, `crawl.open`, `crawl.clear`). */
  holdCmd: string | null = null;
  latestCrawl: Json | null = null;
  latestAnalysis: Json | null = null;
  private lastTick: Json | null = null;
  private window = "main";
  private held: string[] = [];
  private page?: WebSocketRoute;
  private core?: WebSocketRoute;
  readonly log: string[] = [];
  async attach(page: Page) {
    await page.routeWebSocket(/token=/, ws => {
      const core = ws.connectToServer();
      // The page's own connection; a further socket of the page (content from outside) is only passed through.
      if (!this.page) { this.page = ws; this.core = core; }
      ws.onMessage(message => {
        const text = String(message);
        if (this.holdCmd && text.includes(`"cmd":"${this.holdCmd}"`)) { this.held.push(text); this.note("held", text); return; }
        this.note("send", text);
        core.send(message);
      });
      core.onMessage(message => {
        let text = String(message);
        try {
          const msg = JSON.parse(text) as { event?: string; window?: string; data?: Json };
          if (msg.event === "tick" && msg.data) {
            const tick = msg.data;
            this.lastTick = tick; if (typeof msg.window === "string") this.window = msg.window;
            if (tick.crawl) this.latestCrawl = tick.crawl as Json;
            if (tick.analysis) this.latestAnalysis = tick.analysis as Json;
            if (this.holdStatus && (tick.crawl || tick.analysis)) { text = JSON.stringify({ ...msg, data: { ...tick, crawl: null, analysis: null } }); this.note("strip", String(message)); }
          } else this.note("recv", text);
        } catch { /* not JSON */ }
        ws.send(text);
      });
    });
  }
  private note(kind: string, text: string) { this.log.push(`${kind} ${text.slice(0, 400)}`); if (this.log.length > 400) this.log.shift(); }
  /** Starts holding status; forgets what the page has not seen. */
  hold() { this.holdStatus = true; this.latestCrawl = null; this.latestAnalysis = null; }
  coreFinished(crawlId: string) {
    return this.latestCrawl?.crawlId === crawlId && this.latestCrawl?.phase === "completed" && this.latestAnalysis !== null
      && this.latestAnalysis.running === false && Array.isArray(this.latestAnalysis.steps) && this.latestAnalysis.steps.length > 0;
  }
  /** Delivers the held crawl and analysis status in one tick between the last tick and the next. */
  release() {
    this.holdStatus = false;
    const last = this.lastTick!;
    this.page!.send(JSON.stringify({ event: "tick", window: this.window, data: { ...last, seq: (last.seq as number) + 0.5, crawl: this.latestCrawl, analysis: this.latestAnalysis, views: [], ops: [], notices: [] } }));
  }
  releaseHeld() { this.holdCmd = null; for (const text of this.held.splice(0)) this.core!.send(text); }
}

async function site(): Promise<{ server: Server; urls: (prefix: string, n: number) => string[] }> {
  const server = createServer((request, response) => { response.setHeader("Content-Type", "text/html"); response.end(`<html><head><title>${request.url}</title></head><body>${request.url}</body></html>`); });
  await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
  const address = server.address(); if (!address || typeof address === "string") throw new Error("Fixture server has no port");
  return { server, urls: (prefix, n) => Array.from({ length: n }, (_, i) => `http://127.0.0.1:${address.port}/${prefix}${String(i + 1).padStart(2, "0")}`) };
}
async function start(page: Page, items: string[], label: string): Promise<{ crawlId: string; right: CrawlState }> {
  return page.evaluate(async ({ items, label }) => {
    const modules = ["/src/ipc/crawl.ts", "/src/features/table/selection.ts", "/src/features/detail/frame/selection.ts"];
    const [{ crawl }, { useTableSelection }, { useDetailSelection }] = await Promise.all(modules.map(m => import(m)));
    const ref = await crawl.start({ source: { kind: "list", items, label }, scope: "domain" });
    // State right after the answer, before any later tick.
    const sel = useTableSelection.getState().selection;
    return { crawlId: ref.crawlId as string, right: { ids: sel.value.kind === "ids" ? sel.value.ids : "all", focus: sel.focus ?? null, detail: useDetailSelection.getState().selection.urlIds } as CrawlState };
  }, { items, label });
}
async function state(page: Page): Promise<CrawlState> {
  return page.evaluate(async () => {
    const modules = ["/src/state/session.ts", "/src/features/table/selection.ts", "/src/features/detail/frame/selection.ts"];
    const [{ useSession }, { useTableSelection }, { useDetailSelection }] = await Promise.all(modules.map(m => import(m)));
    const session = useSession.getState(); const table = useTableSelection.getState(); const value = table.selection.value;
    const ops = Object.values(session.ops) as { state: string }[];
    return { opsRunning: ops.filter(o => o.state === "running").length, opsDone: ops.filter(o => o.state === "done").length, crawlId: session.crawl?.crawlId ?? null, phase: session.crawl?.phase ?? null, analysisRunning: session.analysis?.running ?? null,
      analysisSteps: session.analysis?.steps?.length ?? 0, ids: value.kind === "ids" ? value.ids : "all", focus: table.selection.focus ?? null,
      viewId: table.viewId ?? null, detail: useDetailSelection.getState().selection.urlIds };
  });
}
/** The window shows crawl `id` completed and analysed, with `rows` rows in the table. */
async function finished(page: Page, id: string, rows: number) {
  await expect.poll(async () => { const s = await state(page); return s.crawlId === id && s.phase === "completed" && s.analysisRunning === false && s.analysisSteps > 0; }, { timeout: 30_000 }).toBe(true);
  await expect(page.getByText(`Showing: ${rows} of ${rows}`, { exact: true })).toBeVisible({ timeout: 20_000 });
}
const grid = (page: Page) => page.locator(".os-data-grid");
const selectedRows = (page: Page) => grid(page).locator('[role=row][aria-selected="true"]').evaluateAll(rows => rows.map(r => r.getAttribute("aria-rowindex")));
const scrollTop = (page: Page) => grid(page).evaluate(root => [root, ...root.querySelectorAll("*")].map(e => e as HTMLElement)
  .find(e => e.scrollHeight > e.clientHeight + 4 && ["auto", "scroll"].includes(getComputedStyle(e).overflowY))?.scrollTop ?? -1);
/** Clicks data row `k` (0-based on screen) of the compact grid; with shift as a range. */
async function clickRow(page: Page, k: number, shift = false) {
  await grid(page).click({ position: { x: 225, y: 43 + 28 * k }, modifiers: shift ? ["Shift"] : [] });
}
async function rowAddress(page: Page, rowIndex: string): Promise<string> {
  const text = await grid(page).locator(`[role=row][aria-rowindex="${rowIndex}"]`).textContent() ?? "";
  return /http:\/\/127\.0\.0\.1:\d+\/[a-z]+\d+/.exec(text)?.[0] ?? `<no address in row ${rowIndex}: ${text}>`;
}
async function expectNoSelection(page: Page, why: string) {
  await expect.poll(async () => { const s = await state(page); return { ids: s.ids, focus: s.focus, detail: s.detail }; }, { message: why }).toEqual({ ids: [], focus: null, detail: [] });
  await expect(grid(page).locator('[role=row][aria-selected="true"]'), why).toHaveCount(0);
  await expect(page.locator("#detail-panel"), why).not.toContainText("127.0.0.1");
}
async function expectRowActions(page: Page) {
  await page.keyboard.press("Shift+F10");
  await expect(page.getByRole("menuitem", { name: "Recrawl", exact: true })).toBeEnabled();
  await expect(page.getByRole("menuitem", { name: "Remove", exact: true })).toBeEnabled();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menuitem", { name: "Recrawl", exact: true })).toHaveCount(0);
}

let bridge: Bridge | null = null; let fixture: Awaited<ReturnType<typeof site>>; let gate: StatusGate;
test.beforeEach(async ({ page }) => {
  bridge = await startBridge(); expect(bridge, "os-app-bridge is required").not.toBeNull();
  fixture = await site(); gate = new StatusGate(); await gate.attach(page);
  await page.goto(`/?bridge=${encodeURIComponent(bridge!.url)}`);
});
test.afterEach(async () => {
  await test.info().attach("proxy-log", { body: gate.log.join("\n"), contentType: "text/plain" });
  await bridge?.stop(); bridge = null;
  await new Promise<void>(resolve => fixture.server.close(() => resolve()));
});

/** Index of the top row on screen once it no longer changes (the grid may still bring a clicked row into view). */
async function settledTopRow(page: Page): Promise<number> {
  const top = () => grid(page).locator("[role=row][aria-rowindex]").evaluateAll(rows => Math.min(...rows.map(r => Number(r.getAttribute("aria-rowindex"))).filter(n => n > 1)));
  let last = Number.NaN;
  await expect.poll(async () => { const now = await top(); const same = now === last; last = now; return same; }).toBe(true);
  return last;
}
const rowsOf = (page: Page, prefix: string) => grid(page).locator("[role=row]").filter({ hasText: new RegExp(`/${prefix}\\d+`) });

/**
 * Punkt 1: crawl begun in this window, status held until the core finished crawl and analysis; the user scrolls, selects
 * three rows, then the status arrives. Selection, focus row, scroll position, detail view and row actions stay.
 * `reveal`: what the user does to see the new crawl's rows before its status is known.
 */
async function selectionSurvivesStatus(page: Page, items: string[], prefix: string, reveal?: () => Promise<void>) {
  gate.hold();
  const { crawlId } = await start(page, items, "fix-sel-a");
  await expect.poll(() => gate.coreFinished(crawlId), { timeout: 30_000, message: "core finishes crawl and analysis" }).toBe(true);
  expect((await state(page)).crawlId, "the window does not know the crawl yet").not.toBe(crawlId);
  await reveal?.();
  await expect.poll(() => rowsOf(page, prefix).count(), { timeout: 20_000, message: "the new crawl's rows are on screen before its status" }).toBeGreaterThan(0);
  await grid(page).hover({ position: { x: 225, y: 100 } });
  await page.mouse.wheel(0, 280);
  await expect.poll(() => scrollTop(page)).toBeGreaterThan(0);
  await expect(grid(page).locator('[role=row][aria-busy="true"]')).toHaveCount(0);
  await clickRow(page, 2); await clickRow(page, 4, true);
  await expect.poll(() => selectedRows(page)).toHaveLength(3);
  const top = await settledTopRow(page);
  const rows = await selectedRows(page); const before = await state(page);
  const focusAddress = await rowAddress(page, rows[2]!);
  expect(focusAddress).toContain(`/${prefix}`);
  await expect(page.locator("#detail-panel")).toContainText(focusAddress);
  gate.release();
  await expect.poll(async () => (await state(page)).crawlId).toBe(crawlId);
  // The table reopens its view for the crawl it now knows; wait for that before checking.
  await expect.poll(async () => (await state(page)).viewId).not.toBe(before.viewId);
  const after = await state(page);
  expect({ ids: after.ids, focus: after.focus, detail: after.detail }).toEqual({ ids: before.ids, focus: before.focus, detail: before.detail });
  expect(await selectedRows(page)).toEqual(rows);
  expect(await settledTopRow(page), "top row on screen").toBe(top);
  await expect(page.locator("#detail-panel")).toContainText(focusAddress);
  await expectRowActions(page);
  // Still kept once later ticks of the finished crawl have come in.
  await expect.poll(async () => (await state(page)).phase).toBe("completed");
  expect((await state(page)).ids).toEqual(before.ids);
}

// spec: UI-018
// spec: UI-142
// spec: UI-114
test("P1: first crawl of the window – a selection made before the crawl status arrives stays", async ({ page }) => {
  await selectionSurvivesStatus(page, fixture.urls("a", 40), "a");
});

// spec: UI-018
// spec: UI-142
// spec: UI-114
test("P1: second crawl of the window, its rows shown by a filter change – a selection made before its status arrives stays", async ({ page }) => {
  const first = await start(page, fixture.urls("z", 40), "fix-sel-z");
  await finished(page, first.crawlId, 40);
  // Until the window learns the new crawl, it shows the old rows; a filter change opens a view of the new crawl.
  await selectionSurvivesStatus(page, fixture.urls("a", 40), "a", async () => {
    await page.getByRole("button", { name: /^HTML \(/ }).click();
  });
});

// spec: UI-018
test("P2: start, open and clear in this window leave no selection, at once and after the new content arrived", async ({ page }) => {
  const one = await start(page, fixture.urls("a", 4), "fix-sel-1");
  await finished(page, one.crawlId, 4);
  await clickRow(page, 1);
  await expect.poll(() => selectedRows(page)).toHaveLength(1);
  // Start
  const two = await start(page, fixture.urls("b", 6), "fix-sel-2");
  expect(two.right, "right after the answer to crawl.start").toMatchObject({ ids: [], focus: null, detail: [] });
  await finished(page, two.crawlId, 6);
  await expectNoSelection(page, "after the started crawl finished");
  // Open the first crawl again
  await clickRow(page, 3);
  await expect.poll(() => selectedRows(page)).toHaveLength(1);
  const opened = await page.evaluate(async id => {
    const modules = ["/src/ipc/crawl.ts", "/src/features/table/selection.ts"];
    const [{ crawl }, { useTableSelection }] = await Promise.all(modules.map(m => import(m)));
    await crawl.open({ kind: "id", crawlId: id });
    const value = useTableSelection.getState().selection.value;
    return value.kind === "ids" ? value.ids : "all";
  }, one.crawlId);
  expect(opened, "right after the answer to crawl.open").toEqual([]);
  await expect.poll(async () => (await state(page)).crawlId).toBe(one.crawlId);
  await expect(page.getByText("Showing: 4 of 4", { exact: true })).toBeVisible({ timeout: 20_000 });
  await expectNoSelection(page, "after the opened crawl is shown");
  // Clear
  await clickRow(page, 0);
  await expect.poll(() => selectedRows(page)).toHaveLength(1);
  const cleared = await page.evaluate(async () => {
    const modules = ["/src/ipc/crawl.ts", "/src/features/table/selection.ts"];
    const [{ crawl }, { useTableSelection }] = await Promise.all(modules.map(m => import(m)));
    await crawl.clear();
    const value = useTableSelection.getState().selection.value;
    return value.kind === "ids" ? value.ids : "all";
  });
  expect(cleared, "right after the answer to crawl.clear").toEqual([]);
  await expectNoSelection(page, "after clearing");
});

/** Total of Internal/All in the core's current content for this window. */
async function coreTotal(page: Page): Promise<number> {
  return page.evaluate(async () => {
    const { view } = await import("/src/ipc/view.ts" as string);
    const handle = await view.open({ spec: { kind: "tab", tab: "Internal", filter: "All", sort: [] }, columns: ["Address"] });
    try { return handle.total as number; } finally { await view.close({ viewId: handle.viewId }); }
  });
}
const rowAction = (page: Page) => page.getByRole("menuitem", { name: /^(Recrawl|Remove)$/ });
const removeConfirm = (page: Page) => page.getByRole("button", { name: "Remove", exact: true });
/**
 * Orchestrator decision (fix-sel, 24.11.): from the moment this window sends start/open/clear the old rows are hidden and
 * unusable: empty state, no selection, no Recrawl/Remove on old rows, no old URL in the details.
 */
async function expectOldRowsGone(page: Page, oldPrefix: string, why: string) {
  await expect(page.locator('[data-empty-state="NO_DATA_YET"]'), why).toBeVisible();
  await expect(grid(page), why).toBeHidden();
  await expect(rowsOf(page, oldPrefix).filter({ visible: true }), why).toHaveCount(0);
  await expectNoSelection(page, why);
  await expect(rowAction(page), why).toHaveCount(0);
}
/** Everything a user can do on the place of the old rows, by mouse and keyboard. Returns what became reachable. */
async function tryOldRows(page: Page, box: { x: number; y: number }): Promise<string[]> {
  const reached: string[] = [];
  const check = async (what: string) => {
    const s = await state(page);
    if (Array.isArray(s.ids) ? s.ids.length > 0 : true) reached.push(`${what}: selection ${JSON.stringify(s.ids)}`);
    if (await rowAction(page).count()) reached.push(`${what}: context menu with row actions`);
    if (await removeConfirm(page).count()) reached.push(`${what}: removal confirmation`);
  };
  await page.mouse.click(box.x + 225, box.y + 43 + 28); await check("click on the second row's place");
  await page.keyboard.down("Shift"); await page.mouse.click(box.x + 225, box.y + 43 + 28 * 3); await page.keyboard.up("Shift"); await check("shift+click");
  await page.mouse.click(box.x + 225, box.y + 43 + 28, { button: "right" }); await check("right click");
  await page.mouse.dblclick(box.x + 225, box.y + 43); await check("double click");
  for (const key of ["Shift+F10", "ContextMenu", "Delete", "ControlOrMeta+a", "Shift+F10", "Delete", "ControlOrMeta+Backspace"]) {
    await page.keyboard.press(key); await check(`key ${key}`);
  }
  return reached;
}

// spec: UI-018
// spec: UI-114
// spec: UI-142
test("P2: after crawl.start is sent the old rows are gone and nothing done there reaches the new crawl", async ({ page }) => {
  test.setTimeout(120_000);
  const one = await start(page, fixture.urls("z", 8), "fix-sel-1");
  await finished(page, one.crawlId, 8);
  await clickRow(page, 1);
  await expect.poll(() => selectedRows(page)).toHaveLength(1);
  const box = (await grid(page).boundingBox())!;
  gate.holdCmd = "crawl.start"; gate.hold();
  const started = start(page, fixture.urls("a", 8), "fix-sel-2");
  await expect.poll(() => gate.log.some(line => line.startsWith("held"))).toBe(true);
  // Phase A: the start is on its way, the core still holds crawl 1.
  await expectOldRowsGone(page, "z", "start sent, not answered");
  const reachedA = await tryOldRows(page, box);
  expect.soft(reachedA, "reachable on the old rows while crawl.start is on its way").toEqual([]);
  // Whatever stayed open is used only once the core holds crawl 2 (the dangerous order).
  gate.releaseHeld();
  const { crawlId } = await started;
  await expect.poll(() => gate.coreFinished(crawlId), { timeout: 30_000 }).toBe(true);
  const menuRemove = page.getByRole("menuitem", { name: "Remove", exact: true });
  if (await removeConfirm(page).count()) await removeConfirm(page).click();
  else if (await menuRemove.count() && await menuRemove.isEnabled()) {
    await menuRemove.click();
    if (await removeConfirm(page).count()) await removeConfirm(page).click();
  }
  if (await page.getByRole("menu").count()) await page.keyboard.press("Escape");
  // Phase B: answered, crawl 2 finished in the core, its status not yet in the window: no row of crawl 1 comes back.
  expect(await rowsOf(page, "z").filter({ visible: true }).count(), "old rows after the answer").toBe(0);
  await expectNoSelection(page, "answer arrived, status held");
  gate.release();
  await expect.poll(async () => (await state(page)).crawlId).toBe(crawlId);
  await expect.poll(() => rowsOf(page, "a").count(), { timeout: 20_000, message: "crawl 2's rows are shown" }).toBeGreaterThan(0);
  await expect(page.getByText("Showing: 8 of 8", { exact: true })).toBeVisible({ timeout: 20_000 });
  await expectNoSelection(page, "after crawl 2's status arrived");
  await expect.poll(async () => (await state(page)).opsRunning).toBe(0);
  expect(await coreTotal(page), "URLs of crawl 2: untouched by anything done on the old rows").toBe(8);
});

// spec: UI-018
// spec: UI-114
test("P2: a failed start brings the old rows back without a selection", async ({ page }) => {
  const one = await start(page, fixture.urls("z", 8), "fix-sel-1");
  await finished(page, one.crawlId, 8);
  await clickRow(page, 1);
  await expect.poll(() => selectedRows(page)).toHaveLength(1);
  gate.holdCmd = "crawl.start";
  const failed = page.evaluate(async () => {
    const { crawl } = await import("/src/ipc/crawl.ts" as string);
    try { await crawl.start({ source: { kind: "list", items: [], label: "fix-sel-empty" }, scope: "domain" }); return "answered"; }
    catch (error) { return (error as { cmdError?: { code?: string } }).cmdError?.code ?? String(error); }
  });
  await expect.poll(() => gate.log.some(line => line.startsWith("held"))).toBe(true);
  await expectOldRowsGone(page, "z", "start sent, not answered");
  gate.releaseHeld();
  expect(await failed, "crawl.start with an empty list must fail").not.toBe("answered");
  await expect(grid(page)).toBeVisible();
  await expect.poll(() => rowsOf(page, "z").count()).toBeGreaterThan(0);
  await expect(page.getByText("Showing: 8 of 8", { exact: true })).toBeVisible();
  await expectNoSelection(page, "after the failed start");
  expect((await state(page)).crawlId).toBe(one.crawlId);
  // The rows are usable again.
  await clickRow(page, 2);
  await expect.poll(() => selectedRows(page)).toHaveLength(1);
  await expectRowActions(page);
  expect(await coreTotal(page)).toBe(8);
});

// spec: UI-018
test("P3: a filter switch keeps the selected URL that is in the new view", async ({ page }) => {
  const one = await start(page, fixture.urls("a", 6), "fix-sel-1");
  await finished(page, one.crawlId, 6);
  await clickRow(page, 2);
  await expect.poll(() => selectedRows(page)).toHaveLength(1);
  const before = await state(page);
  await page.getByRole("button", { name: /^HTML \(/ }).click();
  await expect.poll(async () => (await state(page)).viewId).not.toBe(before.viewId);
  const after = await state(page);
  expect({ ids: after.ids, focus: after.focus }).toEqual({ ids: before.ids, focus: before.focus });
});

/** Opens a crawl or clears, through the page's client (a begin in this window); not awaited. */
function begin(page: Page, cmd: "open" | "clear", crawlId?: string): Promise<string> {
  return page.evaluate(async ({ cmd, crawlId }) => {
    const { crawl } = await import("/src/ipc/crawl.ts" as string);
    try { if (cmd === "open") await crawl.open({ kind: "id", crawlId }); else await crawl.clear(); return "answered"; }
    catch (error) { return (error as { cmdError?: { messageKey?: string } }).cmdError?.messageKey ?? String(error); }
  }, { cmd, crawlId });
}
/** Two finished crawls of 8 URLs each (prefixes z and a); the window shows the second. */
async function twoCrawls(page: Page) {
  const one = await start(page, fixture.urls("z", 8), "fix-sel-1");
  await finished(page, one.crawlId, 8);
  const two = await start(page, fixture.urls("a", 8), "fix-sel-2");
  await finished(page, two.crawlId, 8);
  await expect.poll(() => rowsOf(page, "a").count()).toBeGreaterThan(0);
  return { one: one.crawlId, two: two.crawlId };
}
/** Total of crawl `id` in the core, by opening it (leaves it open). */
async function totalOf(page: Page, id: string): Promise<number> {
  expect(await begin(page, "open", id)).toBe("answered");
  await expect.poll(async () => (await state(page)).crawlId).toBe(id);
  return coreTotal(page);
}

// spec: UI-018
// spec: UI-114
test("P2: after crawl.open is sent the old rows are gone and nothing done there reaches either crawl", async ({ page }) => {
  test.setTimeout(120_000);
  const { one, two } = await twoCrawls(page);
  await clickRow(page, 1);
  await expect.poll(() => selectedRows(page)).toHaveLength(1);
  const box = (await grid(page).boundingBox())!;
  gate.holdCmd = "crawl.open";
  const opened = begin(page, "open", one);
  await expect.poll(() => gate.log.some(line => line.startsWith("held"))).toBe(true);
  await expectOldRowsGone(page, "a", "open sent, not answered");
  expect.soft(await tryOldRows(page, box), "reachable on the old rows while crawl.open is on its way").toEqual([]);
  gate.releaseHeld();
  expect(await opened).toBe("answered");
  await expect.poll(async () => (await state(page)).crawlId).toBe(one);
  await expect.poll(() => rowsOf(page, "z").count(), { timeout: 20_000 }).toBeGreaterThan(0);
  expect(await rowsOf(page, "a").filter({ visible: true }).count(), "no row of crawl 2").toBe(0);
  await expectNoSelection(page, "after crawl 1 was opened");
  await expect.poll(async () => (await state(page)).opsRunning).toBe(0);
  expect(await coreTotal(page), "crawl 1 complete").toBe(8);
  expect(await totalOf(page, two), "crawl 2 complete").toBe(8);
});

// spec: UI-018
// spec: UI-114
test("P2: after crawl.clear is sent the old rows are gone and nothing done there reaches the crawl", async ({ page }) => {
  test.setTimeout(120_000);
  const one = await start(page, fixture.urls("z", 8), "fix-sel-1");
  await finished(page, one.crawlId, 8);
  await clickRow(page, 1);
  await expect.poll(() => selectedRows(page)).toHaveLength(1);
  const box = (await grid(page).boundingBox())!;
  gate.holdCmd = "crawl.clear";
  const cleared = begin(page, "clear");
  await expect.poll(() => gate.log.some(line => line.startsWith("held"))).toBe(true);
  await expectOldRowsGone(page, "z", "clear sent, not answered");
  expect.soft(await tryOldRows(page, box), "reachable on the old rows while crawl.clear is on its way").toEqual([]);
  gate.releaseHeld();
  expect(await cleared).toBe("answered");
  await expectNoSelection(page, "after clearing");
  expect(await rowsOf(page, "z").filter({ visible: true }).count(), "no old row after clearing").toBe(0);
  await expect.poll(async () => (await state(page)).opsRunning).toBe(0);
  expect(await totalOf(page, one.crawlId), "the cleared crawl is complete").toBe(8);
});

// spec: UI-018
// spec: UI-142
test("P2: content replaced from outside (another connection opens a crawl in this window) empties the selection", async ({ page }) => {
  const { one } = await twoCrawls(page);
  await clickRow(page, 1); await clickRow(page, 3, true);
  await expect.poll(() => selectedRows(page)).toHaveLength(3);
  // A second connection to the core, as CLI/MCP or another process would act: no begin in this window.
  const answer = await page.evaluate(async crawlId => {
    const url = new URLSearchParams(location.search).get("bridge")!;
    return new Promise<string>(resolve => {
      const socket = new WebSocket(url);
      socket.onopen = () => socket.send(JSON.stringify({ id: 9001, cmd: "crawl.open", args: { kind: "id", crawlId }, window: "main" }));
      socket.onmessage = event => { const msg = JSON.parse(String(event.data)); if (msg.id === 9001) { socket.close(); resolve("err" in msg ? JSON.stringify(msg.err) : "answered"); } };
      socket.onerror = () => resolve("socket error");
    });
  }, one);
  expect(answer).toBe("answered");
  await expect.poll(async () => (await state(page)).crawlId, { timeout: 30_000 }).toBe(one);
  await expect.poll(() => rowsOf(page, "z").count(), { timeout: 20_000 }).toBeGreaterThan(0);
  await expectNoSelection(page, "content replaced from outside");
});

// spec: UI-018
// spec: UI-114
// spec: CRAWL-131
test("core: recrawl and remove with a selection from the previous crawl are refused and change nothing", async ({ page }) => {
  const { one, two } = await twoCrawls(page);
  const requests = await page.evaluate(async ({ one }) => {
    const { crawl } = await import("/src/ipc/crawl.ts" as string);
    const out: string[] = [];
    for (const action of ["recrawl", "remove"] as const) {
      try { await crawl[action]({ selection: { kind: "ids", ids: [1, 2] }, crawlId: one }); out.push(`${action}: accepted`); }
      catch (error) { out.push(`${action}: ${(error as { cmdError?: { messageKey?: string } }).cmdError?.messageKey}`); }
    }
    return out;
  }, { one });
  expect(requests).toEqual(["recrawl: err-selection-other-crawl", "remove: err-selection-other-crawl"]);
  await expect.poll(async () => (await state(page)).opsRunning).toBe(0);
  expect(await coreTotal(page), "crawl 2 complete").toBe(8);
  expect((await state(page)).crawlId).toBe(two);
});

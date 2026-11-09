import { test, expect } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { toolBinary } from "../alpha/support";
import { persistentBridge } from "./persistent-bridge";

// spec: UI-018
// spec: UI-016
// spec: UI-030
// spec: UI-032
// spec: UI-033
// spec: UI-145
// spec: UI-124
// spec: CRAWL-126 [M2]
test("5000 fixture URLs: live HTML, retained details, pause/resume, completed file through Recent after restart", async ({ page }) => {
  test.setTimeout(960_000);
  page.setDefaultTimeout(10_000);
  const home = mkdtempSync(join(tmpdir(), "os-n04-live-"));
  const site = spawn(toolBinary("os-testsite"), ["synthetic:n=5000,links=30,seed=7"], { stdio: ["ignore", "pipe", "inherit"] });
  const base = await new Promise<string>((resolve, reject) => {
    let output = "";
    const timer = setTimeout(() => reject(new Error("fixture startup timeout")), 20_000);
    site.stdout.on("data", (data: Buffer) => {
      output += data.toString(); const match = /-> (http:\/\/127\.0\.0\.1:\d+)/.exec(output);
      if (match) { clearTimeout(timer); resolve(match[1]!); }
    });
    site.once("error", reject);
  });
  let bridge = await persistentBridge(home);
  const connect = async () => {
    await page.goto(`/?${new URLSearchParams({ bridge: bridge.url })}`);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
  };
  try {
    await connect();
    await page.evaluate(async () => {
      const { getClient } = await import(/* @vite-ignore */ String("/src/ipc/client.ts"));
      const { toCmdError } = await import(/* @vite-ignore */ String("/src/ipc/errors.ts"));
      const c = getClient(); const invoke = c.invoke.bind(c);
      const calls: unknown[] = []; Object.assign(globalThis, { __n04Calls: calls });
      c.invoke = async <T>(cmd: string, args?: unknown, signal?: AbortSignal): Promise<T> => {
        const started = performance.now();
        const call = { cmd, args, pending: true }; calls.push(call);
        try { const result = await invoke(cmd, args, signal) as T; if (cmd.startsWith("view.")) calls.push({ cmd, args, result: cmd === "view.rows" ? { rows: (result as { cells: unknown[] }).cells.length } : result, ms: performance.now() - started }); return result; }
        catch (error) { calls.push({ cmd, args, error: toCmdError(error), ms: performance.now() - started }); throw error; }
        finally { call.pending = false; }
      };
    });
    const reference = await page.evaluate(async base => {
      const { config } = await import(/* @vite-ignore */ String("/src/ipc/config.ts"));
      const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
      const dto = await config.get(); const root = dto.config as Record<string, Record<string, unknown>>;
      await config.set({ ...dto, config: { ...root, speed: { ...root.speed, threads: 4, max_urls_per_sec: 150 }, analysis: { ...root.analysis, auto_at_end: true } } });
      return crawl.start({ source: { kind: "spider", url: `${base}/` }, scope: "domain" });
    }, base);
    await expect.poll(() => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return useSession.getState().crawl?.crawled ?? 0;
    }), { timeout: 30_000 }).toBeGreaterThan(200);
    await page.getByRole("tabpanel").and(page.locator("#data-panel")).getByRole("button", { name: /^HTML(?: \([\d,.\s]+\))?$/ }).click();
    const upperStarted = Date.now();
    const upper = page.getByRole("grid", { name: "Crawl table", exact: true });
    await expect(upper).toBeVisible({ timeout: 10_000 });
    await expect(upper).toContainText(base, { timeout: 10_000 });
    await expect(page.getByTestId("crawl-phase")).toHaveText("Running");
    await upper.focus(); await upper.press("Home");
    const latencies: Record<string, number> = { html: Date.now() - upperStarted };
    const details = page.getByRole("region", { name: "Details", exact: true });
    for (const name of ["Inlinks", "Outlinks", "Inlinks"]) {
      const before = Date.now();
      await details.getByRole("tab", { name, exact: true }).click();
      await expect(page.getByRole("grid", { name, exact: true })).toContainText(base, { timeout: 1000 });
      latencies[`${name}-${Object.keys(latencies).length}`] = Date.now() - before;
    }
    await test.info().attach("live-latencies-ms", { body: JSON.stringify(latencies), contentType: "application/json" });
    await page.evaluate(async () => {
      const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
      await crawl.pause(); await crawl.resume();
    });
    // The status label includes the independent analysis phase; wait for the crawl
    // state first, then allow the full analysis of the 150k fixture links to finish.
    await expect.poll(() => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return useSession.getState().crawl?.phase;
    }), { timeout: 180_000 }).toBe("completed");
    await expect(page.getByTestId("analysis-feedback")).toHaveText("Analysis completed", { timeout: 600_000 });
    await expect(page.getByTestId("crawl-phase")).toHaveText("Completed");
    await page.goto("about:blank"); await bridge.stop();
    await test.info().attach("completed-crawl", { body: JSON.stringify({ fileBytes: statSync(reference.file).size, crawlId: reference.crawlId }), contentType: "application/json" });
    bridge = await persistentBridge(home);
    await connect();
    await expect.poll(() => page.evaluate(() => JSON.stringify(window.__osMenu?.model.menus.find(menu => menu.id === "file")))).toContain(base);
    await page.evaluate(async () => {
      const { runAction } = await import(/* @vite-ignore */ String("/src/ui/actions.ts"));
      const { flattenItems } = await import(/* @vite-ignore */ String("/src/ui/menu/build.ts"));
      const file = window.__osMenu?.model.menus.find(menu => menu.id === "file");
      const recent = flattenItems(file?.items ?? []).find((entry: { id: string }) => entry.id === "recent.0");
      if (!recent?.enabled || !/ – .*\b20\d{2}\b/.test(recent.label)) throw new Error("Recent lacks metadata or is disabled");
      if (!runAction(recent.action, { source: "menu" })) throw new Error("Recent menu action was not handled");
    });
    await expect(page.getByTestId("crawl-phase")).toHaveText("Completed", { timeout: 10_000 });
    const summary = await page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return useSession.getState().crawl;
    });
    expect(summary?.crawlId).toBe(reference.crawlId);
    expect(summary?.crawled).toBeGreaterThanOrEqual(5000);
    await expect(page.getByTestId("crawl-open-error")).toHaveCount(0);
  } catch (error) {
    await test.info().attach("session", { body: JSON.stringify(await page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      const { crawl, analysis, ops } = useSession.getState(); return { crawl, analysis, ops };
    })), contentType: "application/json" });
    await test.info().attach("commands", { body: JSON.stringify(await page.evaluate(() => (globalThis as unknown as { __n04Calls: unknown[] }).__n04Calls)) ?? "[]", contentType: "application/json" });
    throw error;
  } finally {
    await page.goto("about:blank"); await bridge.stop();
    await new Promise<void>(resolve => { if (site.exitCode !== null) resolve(); else { site.once("exit", () => resolve()); site.kill(); } });
    rmSync(home, { recursive: true, force: true });
  }
});

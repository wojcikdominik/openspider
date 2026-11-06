// Messrezept B4-LIVEDETAIL (M2-N32; UI-145, UI-143) im Browser: Produktionsbuild des Perf-Shells (vite preview), Kern über
// die E2E-Bridge (für Budgets `OS_APP_BRIDGE_BIN` auf die Release-Bridge setzen), Live-Crawl gegen eine synthetische Site.
// Ergebnis: `target/loadtest/livedetail-<engine>[-<label>].json` (Zeitaufteilung je Klick, Hauptthread-Belegung).
// Aufruf siehe `livedetail.config.ts`. Kein Budget-Gate: Die verbindliche Messung läuft im Release-Build auf der
// Referenzhardware (docs/spec/07-performance.md); dieser Lauf liefert Anhaltswerte und die Aufteilung.
import { test, expect } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { toolBinary } from "../alpha/support";
import { startPerfBridge } from "./bridge";
import type { liveDetail } from "./livedetail";

const num = (name: string, fallback: number) => { const v = Number(process.env[name]); return Number.isFinite(v) && process.env[name] !== "" && process.env[name] !== undefined ? v : fallback; };

// spec: UI-145
// spec: UI-143
test("live detail: time split of Inlinks/Outlinks during a crawl", async ({ page, browser, browserName }, info) => {
  const pages = num("OS_LIVEDETAIL_PAGES", 5000);
  const rate = num("OS_LIVEDETAIL_RATE", 150);
  const options = { threads: num("OS_LIVEDETAIL_THREADS", 4), rate: rate > 0 ? rate : null, warmup: num("OS_LIVEDETAIL_WARMUP", 300),
    rounds: num("OS_LIVEDETAIL_ROUNDS", 5), idleMs: num("OS_LIVEDETAIL_IDLE_MS", 5000) };
  test.setTimeout(600_000);
  const site = spawn(toolBinary("os-testsite"), [`synthetic:n=${pages},links=30,seed=7`], { stdio: ["ignore", "pipe", "inherit"] });
  const bridge = await startPerfBridge();
  try {
    const base = await new Promise<string>((ok, fail) => {
      let out = ""; const timer = setTimeout(() => fail(new Error("fixture startup timeout")), 20_000);
      site.stdout!.on("data", (d: Buffer) => { out += String(d); const m = /-> (http:\/\/127\.0\.0\.1:\d+)/.exec(out); if (m) { clearTimeout(timer); ok(m[1]!); } });
      site.once("error", fail);
    });
    await page.goto(`/e2e/perf/shell.html?${new URLSearchParams({ bridge: bridge.url })}`);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true", { timeout: 30_000 });
    const cdp = process.env.OS_LIVEDETAIL_PROFILE === "1" && browserName === "chromium" ? await page.context().newCDPSession(page) : undefined;
    if (cdp) { await cdp.send("Profiler.enable"); await cdp.send("Profiler.setSamplingInterval", { interval: 200 }); await cdp.send("Profiler.start"); }
    const tracing = process.env.OS_LIVEDETAIL_TRACE === "1" && browserName === "chromium";
    if (tracing) await browser.startTracing(page, { categories: ["devtools.timeline", "disabled-by-default-devtools.timeline", "blink.user_timing"] });
    const result = await page.evaluate(o => (window as unknown as { liveDetail: typeof liveDetail }).liveDetail(o), { base: `${base}/`, ...options });
    const out = resolve(import.meta.dirname, "../../../../target/loadtest");
    await mkdir(out, { recursive: true });
    const label = process.env.OS_LIVEDETAIL_LABEL ? `-${process.env.OS_LIVEDETAIL_LABEL}` : "";
    if (tracing) await writeFile(resolve(out, `livedetail-${info.project.name}${label}.trace.json`), await browser.stopTracing());
    if (cdp) { const { profile } = await cdp.send("Profiler.stop"); await writeFile(resolve(out, `livedetail-${info.project.name}${label}.cpuprofile`), JSON.stringify(profile)); }
    const report = { scenario: "livedetail", engine: info.project.name, pages, bridge: process.env.OS_APP_BRIDGE_BIN ?? "debug", ...result };
    await writeFile(resolve(out, `livedetail-${info.project.name}${label}.json`), JSON.stringify(report, null, 2));
    await info.attach("livedetail", { body: JSON.stringify(report), contentType: "application/json" });
    console.log(`LIVEDETAIL ${info.project.name}${label} ${JSON.stringify({ idle: result.idleLag, measuring: result.measuringLag, summary: result.summary })}`);
    expect(result.samples.length).toBe(options.rounds * 3);
  } finally {
    await bridge.stop();
    await new Promise<void>(done => { if (site.exitCode !== null) done(); else { site.once("exit", () => done()); site.kill(); } });
  }
});

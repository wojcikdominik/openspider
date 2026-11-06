// Additive diagnosis of the WebKit soak RSS growth (UI-144). Opt-in via N23_SOAK_DIAG.
// U31's soak test, sampler, warmup, timing and budgets stay unchanged; this file
// only repeats the same seeded action sequence with extra attribution samples.
import { test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { loadavg } from "node:os";
import { resolve } from "node:path";
import { performance as clock } from "node:perf_hooks";
import type { Bridge } from "../support/bridge";
import { startPerfBridge } from "./bridge";
import { rendererRss } from "./rss.mjs";

const variant = process.env.N23_SOAK_DIAG ?? "";
test.skip(!variant, "Benchmark: opt-in soak attribution via N23_SOAK_DIAG");

type Listener = (event: unknown) => void;
interface Inspector {
  send(method: string, params?: Record<string, unknown>): Promise<Record<string, unknown>>;
  on(event: string, listener: Listener): void;
}
// Locked Playwright 1.64.1 internal adapter, as in n23-webkit.ts (diagnosis only).
function inspector(page: Page): Inspector {
  const internal = page as unknown as { _connection: { toImpl(page: Page): { delegate: { _session: Inspector } } } };
  const session = internal._connection.toImpl(page).delegate._session;
  if (!session) throw new Error("N23: locked WebKit Inspector adapter unavailable");
  return session;
}

function vmmap(pid: number): string {
  try {
    return execFileSync("vmmap", ["-summary", String(pid)], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  } catch (error) {
    return `vmmap failed: ${String(error)}`;
  }
}

// spec: UI-144
test("N23 diagnostic: WebKit soak RSS attribution", async ({ page }, info) => {
  test.skip(info.project.name !== "webkit" || variant.startsWith("synthetic"), "Benchmark: WebKit soak attribution only");
  const seconds = Number(process.env.OS_PERF_SOAK_SECONDS ?? 600);
  test.setTimeout((seconds + 900) * 1000);
  const out = resolve("../../target/loadtest/n23-soak", variant);
  await mkdir(out, { recursive: true });
  // Source separation (diagnosis only): keep one text-layout path out of layout.
  const hidden = variant === "noa11y" ? ".os-grid-a11y" : variant === "nomeasure" ? ".os-grid-measure" : null;
  if (hidden)
    await page.addInitScript((selector) => {
      document.addEventListener("DOMContentLoaded", () => {
        const style = document.createElement("style");
        style.textContent = `${selector} { display: none !important; }`;
        document.head.appendChild(style);
      });
    }, hidden);
  if (process.env.N23_SOAK_WORDS)
    await page.addInitScript(() => {
      // Which texts reach DOM width measurement (offsetWidth of text-only elements)?
      const stats = { reads: 0, texts: new Set<string>(), words: new Set<string>(), short: new Set<string>(), long: 0 };
      Object.assign(window, { n23Words: stats });
      const property = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth")!;
      Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
        ...property,
        get() {
          const element = this as HTMLElement;
          if (!element.firstElementChild && element.textContent) {
            const text = element.textContent;
            stats.reads++;
            if (stats.texts.size < 3_000_000) stats.texts.add(text);
            if (text.length <= 15) stats.short.add(text);
            else stats.long++;
            for (const word of text.split(/\s+/)) if (word && word.length <= 15 && stats.words.size < 3_000_000) stats.words.add(word);
          }
          return property.get!.call(this);
        },
      });
    });
  const bridge: Bridge = await startPerfBridge();
  try {
    const fixture = resolve(process.env.OS_PERF_FIXTURE ?? "../../target/ui-perf.ospider");
    await page.goto(`/e2e/perf/index.html?bridge=${encodeURIComponent(bridge.url)}&fixture=${encodeURIComponent(fixture)}`);
    await page.waitForFunction(() => window.uiPerf?.ready(), undefined, { timeout: 120_000 });
    const session = inspector(page);
    let memory: unknown = null;
    session.on("Memory.trackingUpdate", (e) => {
      memory = (e as { event: unknown }).event;
    });
    await session.send("Memory.startTracking");

    let seed = 19;
    let actions = 0;
    let tabs = 0;
    const step = async () => {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      actions++;
      if ((seed & 15) === 0) tabs++;
      if (variant === "notab")
        await page.evaluate(async (s) => {
          const row = s % Math.max(1, window.perfGrid.snapshot().total);
          await window.perfGrid.jump(row, (s >>> 8) % Math.min(65, window.perfGrid.snapshot().columns));
        }, seed);
      else if (variant === "tabonly")
        await page.evaluate((s) => window.perfGrid.switchTab((s & 16) === 0 ? "Response Codes" : "Internal"), seed);
      else await page.evaluate((s) => window.uiPerf.action(s), seed);
    };
    for (let i = 0; i < 100; i++) await step();
    await page.waitForTimeout(2000);
    const page_ = async () =>
      page.evaluate(() => ({
        elements: document.getElementsByTagName("*").length,
        canvases: Array.from(document.querySelectorAll("canvas")).map((c) => c.width * c.height),
        cachedRows: window.perfGrid.snapshot().cachedRows,
        errors: window.uiPerf.errors.length,
      }));
    const samples: unknown[] = [];
    const maps: { elapsed: number; text: string }[] = [];
    const sample = async (elapsed: number, map: boolean) => {
      const rss = rendererRss();
      samples.push({ elapsed, actions, tabs, ...rss, memory, page: await page_(), load: loadavg()[0] });
      if (map) maps.push({ elapsed, text: rss.processes.map((p) => vmmap(p.pid)).join("\n") });
    };
    await sample(0, true);
    const start = clock.now();
    let last = 0;
    let lastMap = 0;
    while (clock.now() - start < seconds * 1000) {
      if (variant === "inpage") {
        // One protocol call per 10 s window: same actions and pauses, executed in the page.
        const r = await page.evaluate(
          async ({ s, ms }) => {
            let n = 0;
            const end = performance.now() + ms;
            while (performance.now() < end) {
              s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
              await window.uiPerf.action(s);
              n++;
              await new Promise((r) => setTimeout(r, 500));
            }
            return { s, n };
          },
          { s: seed, ms: 10_000 },
        );
        seed = r.s;
        actions += r.n;
      } else {
        await step();
        await page.waitForTimeout(500);
      }
      const now = clock.now() - start;
      if (now - last >= 10_000) {
        last = now;
        const map = now - lastMap >= 120_000;
        if (map) lastMap = now;
        await sample(now, map);
        await writeFile(resolve(out, "progress.json"), JSON.stringify({ variant, seconds, samples }));
      }
    }
    await sample(clock.now() - start, true);
    await session.send("Heap.gc");
    await page.waitForTimeout(3000);
    await sample(clock.now() - start, true);
    await session.send("Memory.stopTracking");
    await writeFile(resolve(out, "soak-diagnostic.json"), JSON.stringify({ variant, seconds, samples }, null, 2));
    await writeFile(resolve(out, "vmmap.json"), JSON.stringify(maps));
    if (process.env.N23_SOAK_WORDS)
      await writeFile(
        resolve(out, "words.json"),
        JSON.stringify(
          await page.evaluate(() => {
            const s = (window as unknown as { n23Words: { reads: number; texts: Set<string>; words: Set<string>; short: Set<string>; long: number } }).n23Words;
            const sample = [...s.short].filter((_, i) => i % 997 === 0).slice(0, 200);
            const words = [...s.words].filter((_, i) => i % 997 === 0).slice(0, 200);
            return { reads: s.reads, uniqueTexts: s.texts.size, uniqueShortTexts: s.short.size, longReads: s.long, uniqueWordsUpTo15: s.words.size, sample, words };
          }),
          null,
          2,
        ),
      );
  } finally {
    await bridge.stop();
  }
});

// spec: UI-144
// Synthetic engine check: RSS while unique short/long strings pass the engine's text-width path.
test("N23 diagnostic: WebKit text width path with unique strings", async ({ browser }, info) => {
  test.skip(variant !== "synthetic" || info.project.name !== "webkit", "Benchmark: synthetic WebKit check only");
  test.setTimeout(900_000);
  const out = resolve("../../target/loadtest/n23-soak", variant);
  await mkdir(out, { recursive: true });
  const result: Record<string, unknown[]> = {};
  for (const mode of ["canvas-measure-short", "canvas-fill-short", "canvas-fill-long", "dom-short"]) {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.setContent('<canvas width="400" height="40"></canvas><span id="m" style="position:absolute;white-space:pre;font:12px system-ui"></span>');
    const rows: unknown[] = [{ strings: 0, ...rendererRss() }];
    for (let done = 0; done < 1_200_000; done += 100_000) {
      await page.evaluate(
        ({ mode, from }) => {
          const ctx = document.querySelector("canvas")!.getContext("2d")!;
          ctx.font = "12px system-ui";
          const m = document.getElementById("m")!;
          for (let i = from; i < from + 100_000; i++) {
            const s = (i * 7919 + 1).toString(36);
            const text = mode.endsWith("long") ? `https://example.com/path/${s}` : s;
            if (mode === "canvas-measure-short") ctx.measureText(text);
            else if (mode === "dom-short") {
              m.textContent = text;
              void m.offsetWidth;
            } else ctx.fillText(text, 2, 20);
          }
        },
        { mode, from: done },
      );
      rows.push({ strings: done + 100_000, ...rendererRss() });
    }
    result[mode] = rows;
    await context.close();
  }
  await writeFile(resolve(out, "synthetic.json"), JSON.stringify(result, null, 2));
});

// spec: UI-144
// Synthetic engine check of the grid's hidden accessibility rows: which per-jump DOM content grows RSS?
test("N23 diagnostic: WebKit hidden accessibility rows with unique content", async ({ browser }, info) => {
  test.skip(variant !== "synthetic-a11y" || info.project.name !== "webkit", "Benchmark: synthetic WebKit check only");
  test.setTimeout(900_000);
  const out = resolve("../../target/loadtest/n23-soak", variant);
  await mkdir(out, { recursive: true });
  const result: Record<string, unknown[]> = {};
  const names: Record<string, unknown> = {};
  for (const mode of ["text", "text-labelledby", "ids", "text-shared"]) {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.setContent(
      '<div id="c" style="position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap;font:12px system-ui"></div>',
    );
    const rows: unknown[] = [{ jumps: 0, ...rendererRss() }];
    for (let done = 0; done < 4000; done += 400) {
      await page.evaluate(
        async ({ mode, from }) => {
          const c = document.getElementById("c")!;
          for (let j = from; j < from + 400; j++) {
            const base = (j * 7919) % 5_000_000;
            const fragment = document.createDocumentFragment();
            for (let r = 0; r < 30; r++) {
              const row = document.createElement("div");
              row.setAttribute("role", "row");
              if (mode === "aria") row.setAttribute("aria-rowindex", String(base + r + 2));
              for (let col = 0; col < 20; col++) {
                const cell = document.createElement("div");
                cell.setAttribute("role", "gridcell");
                if (mode === "ids") cell.id = `g-${col}-${base + r}`;
                if (mode === "aria") cell.setAttribute("aria-colindex", String(col + 1));
                if (mode === "text") cell.textContent = String(((base + r) * 31 + col * 7) % 100_000);
                if (mode === "text-labelledby") {
                  // Same text content, but kept out of layout; the accessible name references it.
                  const label = document.createElement("span");
                  label.hidden = true;
                  label.id = `l-${col}-${base + r}`;
                  label.textContent = String(((base + r) * 31 + col * 7) % 100_000);
                  cell.setAttribute("aria-labelledby", label.id);
                  cell.appendChild(label);
                }
                if (mode === "text-shared") cell.textContent = String(col);
                row.appendChild(cell);
              }
              fragment.appendChild(row);
            }
            c.replaceChildren(fragment);
            void c.offsetWidth;
            if (j % 20 === 0) await new Promise((r) => requestAnimationFrame(() => r(null)));
          }
        },
        { mode, from: done },
      );
      rows.push({ jumps: done + 400, ...rendererRss() });
    }
    result[mode] = rows;
    if (mode.startsWith("text")) {
      const first = await page.evaluate(() => document.querySelector("[role=gridcell]")!.textContent);
      names[mode] = { textContent: first, byName: await page.getByRole("gridcell", { name: first!, exact: true }).count() };
    }
    await context.close();
  }
  await writeFile(resolve(out, "synthetic-a11y.json"), JSON.stringify({ result, names }, null, 2));
});

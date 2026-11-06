import { test, expect, type TestInfo, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { createServer } from "node:http";
import { performance as clock } from "node:perf_hooks";
import type { Bridge } from "../support/bridge";
import { startPerfBridge } from "./bridge";
import { rendererRss } from "./rss.mjs";

const rows = Number(process.env.OS_PERF_ROWS ?? 100_000);
const fixture = resolve(
  process.env.OS_PERF_FIXTURE ?? "../../target/ui-perf.ospider",
);
const output = resolve("../../target/loadtest/raw");
let bridge: Bridge;
const startedByTest = new Map<string, string>();
test.beforeEach(({}, info) => {
  startedByTest.set(info.testId, new Date().toISOString());
});
test.beforeAll(async () => {
  const b = await startPerfBridge();
  if (!b) throw new Error("Real bridge required");
  bridge = b;
});
test.afterAll(async () => {
  await bridge?.stop();
});
test.afterEach(async ({}, info) => {
  if (info.status !== "skipped" && info.status !== info.expectedStatus) {
    await mkdir(output, { recursive: true });
    await writeFile(
      resolve(output, `${info.project.name}-failure-${info.line}.json`),
      JSON.stringify({
        scenario: `test-failure-${info.line}`,
        engine: info.project.name,
        size: rows === 5_000_000 ? "large" : "small",
        started_at: startedByTest.get(info.testId) ?? new Date().toISOString(),
        metrics: {},
        passed: false,
        details: {
          title: info.title,
          errors: info.errors.map((e) => e.message),
        },
      }),
    );
  }
});
async function open(page: Page) {
  await page.goto(
    `/e2e/perf/index.html?bridge=${encodeURIComponent(bridge.url)}&fixture=${encodeURIComponent(fixture)}`,
  );
  await page.waitForFunction(() => window.uiPerf?.ready(), undefined, {
    timeout: 120_000,
  });
}
type Metric = {
  value: number;
  budget: number;
  direction: "min" | "max";
  passed: boolean;
};
function metric(
  value: number,
  budget: number,
  direction: "min" | "max" = "max",
): Metric {
  return {
    value,
    budget,
    direction,
    passed:
      Number.isFinite(value) &&
      (direction === "max" ? value <= budget : value >= budget),
  };
}
async function evidence(
  info: TestInfo,
  scenario: string,
  metrics: Record<string, unknown>,
  details: unknown,
) {
  const report = {
    scenario,
    size: rows === 5_000_000 ? "large" : "small",
    started_at: startedByTest.get(info.testId) ?? new Date().toISOString(),
    engine: info.project.name,
    metrics,
    details,
    passed: Object.values(metrics).every(
      (m) =>
        !(m && typeof m === "object" && "passed" in m) || (m as Metric).passed,
    ),
  };
  await mkdir(output, { recursive: true });
  await writeFile(
    resolve(output, `${info.project.name}-${scenario}.json`),
    JSON.stringify(report, null, 2),
  );
  await info.attach(scenario, {
    body: JSON.stringify(report),
    contentType: "application/json",
  });
  expect(report.passed, JSON.stringify(metrics)).toBe(true);
}
const p95 = (v: number[]) =>
  [...v].sort((a, b) => a - b)[Math.ceil(v.length * 0.95) - 1];
// spec: UI-147 [P1]
test("cold browser context and open to first painted rows", async ({
  page,
}, info) => {
  const start = clock.now();
  await open(page);
  const openMs = await page.evaluate(() => window.uiPerf.openMs());
  await evidence(
    info,
    "open",
    {
      browserContextToRowsMs: metric(clock.now() - start, 3000),
      openMs: metric(openMs, 3000),
    },
    {
      scope:
        "Fresh browser context to painted real DataGrid; OS application cold start requires native E2E-B",
      rows,
      snapshot: await page.evaluate(() => window.uiPerf.snapshot()),
    },
  );
});
// spec: UI-142
// spec: PERF-004
test("real IPC 200 rows × 20 columns p95 and payload", async ({
  page,
}, info) => {
  test.setTimeout(600_000);
  await open(page);
  const result = await page.evaluate(() => window.uiPerf.ipc());
  await evidence(
    info,
    "ipc",
    {
      roundtripP95Ms: metric(p95(result.samples.map((s) => s.ms)), 30),
      payloadBytes: metric(
        Math.max(...result.samples.map((s) => s.bytes)),
        150_000,
      ),
    },
    result,
  );
});
// spec: UI-140
// spec: PERF-004
test("ten seconds vertical and horizontal 65-column scrolling and end jump", async ({
  page,
}, info) => {
  await open(page);
  const frames = [];
  for (const axis of ["vertical", "horizontal"] as const)
    frames.push(await page.evaluate((a) => window.uiPerf.record(a), axis));
  const jump = await page.evaluate((n) => window.uiPerf.jump(n - 1), rows);
  const metrics: Record<string, Metric> = {
    jumpMs: metric(jump.contentMs, 150),
    domRows: metric(jump.domRows, 299),
    cachedRows: metric(jump.cachedRows, 20_000),
  };
  for (const frame of frames) {
    metrics[`${frame.axis}Fps`] = metric(frame.fps, 55, "min");
    metrics[`${frame.axis}MaxFrameMs`] = metric(frame.maxFrameMs, 50);
  }
  metrics.jumpReady = metric(Number(jump.contentReady), 1, "min");
  metrics.jumpVisible = metric(Number(jump.requestedVisible), 1, "min");
  metrics.columns = metric(jump.columns, 65, "min");
  await evidence(info, "scroll", metrics, { frames, jump });
});
// spec: UI-141
// spec: PERF-004
test("tab filter numeric text substring regex and reverse to painted rows", async ({
  page,
}, info) => {
  await open(page);
  const times = await page.evaluate(() => window.uiPerf.operations());
  const large = rows > 1_000_000;
  const budgets: Record<string, number> = {
    tabMs: 200,
    tabReturnMs: 200,
    filterMs: 200,
    numericMs: large ? 5000 : 1500,
    textMs: large ? 8000 : 2500,
    reverseMs: 200,
    substringMs: large ? 3000 : 1000,
    regexMs: large ? 10_000 : 3000,
  };
  await evidence(
    info,
    "operations",
    Object.fromEntries(
      Object.entries(times).map(([k, v]) => [k, metric(v, budgets[k])]),
    ),
    { times, errors: await page.evaluate(() => window.uiPerf.errors) },
  );
});
// spec: UI-144
test("seeded random action soak and attributable renderer RSS", async ({
  page,
}, info) => {
  const seconds = Number(process.env.OS_PERF_SOAK_SECONDS ?? 1800);
  test.skip(
    info.project.name !== (process.env.OS_PERF_SOAK_ENGINE ?? "chromium"),
    "Benchmark: soak runs once per suite; reference selects WebKit",
  );
  test.setTimeout((seconds + 900) * 1000);
  await mkdir(output, { recursive: true });
  await writeFile(
    resolve(output, "../soak-progress.json"),
    JSON.stringify({
      phase: "opening",
      seconds,
      rows,
      engine: info.project.name,
    }),
  );
  await open(page);
  // Warm cache/render paths before choosing baseline.
  let seed = 19;
  const action = async () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    await page.evaluate((s) => window.uiPerf.action(s), seed);
  };
  console.log(`[soak] ${info.project.name}: warmup begins`);
  for (let i = 0; i < 100; i++) {
    await action();
    if (i % 10 === 9)
      await writeFile(
        resolve(output, "../soak-progress.json"),
        JSON.stringify({
          phase: "warmup",
          actions: i + 1,
          rows,
          engine: info.project.name,
        }),
      );
  }
  console.log(`[soak] ${info.project.name}: ${seconds}s measurement begins`);
  await page.waitForTimeout(2000);
  const samples = [{ elapsed: 0, ...rendererRss() }];
  const start = clock.now();
  let lastSample = 0;
  while (clock.now() - start < seconds * 1000) {
    await action();
    await page.waitForTimeout(500);
    if (clock.now() - start - lastSample >= 10_000) {
      lastSample = clock.now() - start;
      samples.push({ elapsed: lastSample, ...rendererRss() });
      await writeFile(
        resolve(output, "../soak-progress.json"),
        JSON.stringify({
          phase: "measuring",
          elapsedSeconds: lastSample / 1000,
          seconds,
          rows,
          engine: info.project.name,
          samples,
        }),
      );
    }
  }
  samples.push({ elapsed: clock.now() - start, ...rendererRss() });
  const growth = (samples.at(-1)!.bytes / samples[0].bytes - 1) * 100;
  await evidence(
    info,
    "soak",
    {
      peakRssBytes: metric(
        Math.max(...samples.map((s) => s.bytes)),
        400_000_000,
      ),
      growthPercent: metric(growth, 10),
      durationSeconds: metric((clock.now() - start) / 1000, seconds, "min"),
      actionViolations: metric(
        await page.evaluate(() => window.uiPerf.violations()),
        0,
      ),
    },
    {
      samples,
      seed: 19,
      actions:
        "random vertical/horizontal scroll/jump and tab changes; cache ≤20000; DOM rows <300",
      fullSoak: seconds >= 1800,
      configuredSeconds: seconds,
      errorExamples: await page.evaluate(() =>
        window.uiPerf.errors.slice(0, 100),
      ),
    },
  );
});

// spec: UI-143
// spec: PERF-004
test("200 URL/s fixture with 20 crawl threads: click latency and event cap", async ({
  page,
}, info) => {
  const responses: number[] = [];
  // 220 responses/s supplies at least the specified 200 URL/s under load; deterministic ring of 200000 pages.
  const pending: (() => void)[] = [];
  const timer = setInterval(() => pending.shift()?.(), 1000 / 220);
  const site = createServer((req, res) => {
    if (req.url === "/robots.txt") {
      res.end("User-agent: *\nAllow: /\n");
      return;
    }
    const n = Number(req.url?.slice(1)) || 0;
    pending.push(() => {
      responses.push(clock.now());
      res.setHeader("Content-Type", "text/html");
      res.end(
        `<html><head><title>Load ${n}</title></head><body>${Array.from({ length: 30 }, (_, i) => `<a href="/${(n * 17 + i + 1) % 200000}">page</a>`).join("")}</body></html>`,
      );
    });
  });
  await new Promise<void>((resolve) => site.listen(0, "127.0.0.1", resolve));
  const address = site.address();
  if (!address || typeof address === "string")
    throw new Error("No fixture port");
  try {
    await page.goto(
      `/e2e/perf/shell.html?bridge=${encodeURIComponent(bridge.url)}`,
    );
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute(
      "data-shell-ready",
      "true",
    );
    await page.evaluate(
      (base) => window.perfShell.start(base),
      `http://127.0.0.1:${address.port}/`,
    );
    await expect
      .poll(() => responses.length, { timeout: 30_000 })
      .toBeGreaterThan(1000);
    const start = clock.now();
    const before = responses.length;
    await page.evaluate(() => {
      window.perfShell.ticks.length = 0;
      window.perfShell.clicks.length = 0;
    });
    for (let i = 0; i < 30; i++) {
      await page
        .getByRole("tab", {
          name: i % 2 ? "Internal" : "External",
          exact: true,
        })
        .click();
      await page.waitForTimeout(1000);
    }
    const cpuBefore = rendererRss();
    const cpuStart = clock.now();
    const loadFrames = await page.evaluate(async () => {
      const frames: number[] = [];
      let previous = 0,
        start = 0;
      await new Promise<void>((resolve) => {
        const frame = (at: number) => {
          start ||= at;
          if (previous) frames.push(at - previous);
          previous = at;
          if (at - start < 30_000) requestAnimationFrame(frame);
          else resolve();
        };
        requestAnimationFrame(frame);
      });
      return {
        fps: (frames.length / frames.reduce((a, b) => a + b, 0)) * 1000,
        frames,
      };
    });
    const cpuAfter = rendererRss();
    const cpuPercent =
      cpuBefore.cpuMs !== null && cpuAfter.cpuMs !== null
        ? ((cpuAfter.cpuMs - cpuBefore.cpuMs) / (clock.now() - cpuStart)) * 100
        : null;
    const seconds = (clock.now() - start) / 1000;
    const observed = await page.evaluate(() => ({
      ticks: window.perfShell.ticks,
      clicks: window.perfShell.clicks,
      state: window.perfShell.state(),
    }));
    const intervals = observed.ticks
      .slice(1)
      .map((t, i) => t.at - observed.ticks[i].at);
    await evidence(
      info,
      "crawl-load",
      {
        clickP95Ms: metric(p95(observed.clicks), 100),
        validClickSamples: metric(
          observed.clicks.filter(Number.isFinite).length,
          30,
          "min",
        ),
        crawlFps: metric(loadFrames.fps, 50, "min"),
        eventsPerSecond: metric(observed.ticks.length / seconds, 10),
        fixtureUrlsPerSecond: metric(
          (responses.length - before) / seconds,
          200,
          "min",
        ),
        idleUiCpuPercent: metric(cpuPercent ?? NaN, 15),
      },
      {
        ...observed,
        loadFrames,
        intervals,
        cpuBefore,
        cpuAfter,
        cpuPercent,
        threads: 20,
        seconds,
        responses: responses.length - before,
      },
    );
  } finally {
    await page.evaluate(() => window.perfShell?.stop()).catch(() => {});
    clearInterval(timer);
    site.closeAllConnections();
    await new Promise<void>((resolve) => site.close(() => resolve()));
  }
});

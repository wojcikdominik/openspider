// UI-146 Visualisierungs-Performance im Frame-Recorder (src/grid/recorder.ts): 2D-Graph und Baum mit 10 000 Knoten
// (gen-ui-large-Teilmenge). Verbindlich sind die Werte auf Referenzhardware A (K2-12); in Cloud/CI gelten sie als Indiz
// mit Faktor `SLACK`, außer OS_VIZ_STRICT_BUDGETS=1. Messwerte stehen als Anhang `ui-146` am Testergebnis.
// Mit OS_VIZ_FPS_ADVISORY=1 warnt die fps-Grenze nur: Geteilte Runner ohne GPU schaffen unter WebKit 11–13 fps
// (Spec coverage M2, linux-webkit). Verbindlich bleibt der strenge Lauf je Kandidat (docs/release/checkliste-release.md).
import { expect, test } from "@playwright/test";
import { FakeVizCore } from "../../src/features/viz/fixtures/fakeCore";
import { genUiLarge } from "../../src/features/viz/fixtures/synthetic";
import { layoutDone, openViz } from "./support";
import { startVizBridge, type VizBridge } from "./viz-bridge";

const STRICT = process.env.OS_VIZ_STRICT_BUDGETS === "1";
const SLACK = STRICT ? 1 : 2;
const FPS_ADVISORY = !STRICT && process.env.OS_VIZ_FPS_ADVISORY === "1";
/** Teilmenge mit genau 10 000 Knoten (1 / 40 / 2 000 / 7 959). */
const TEN_K = [1, 40, 2000, 7959];

let bridge: VizBridge;
test.beforeEach(async () => {
  bridge = await startVizBridge(new FakeVizCore(() => genUiLarge(TEN_K)));
});
test.afterEach(async () => {
  await bridge.stop();
});

test.describe("UI-146 Budgets", () => {
  // Seriell: parallele Messungen verfälschen sich gegenseitig (CPU).
  test.describe.configure({ timeout: 90_000, mode: "serial" });

  // spec: UI-146
  test("2D-Graph 10 000 Knoten: erstes Bild ≤ 2 s, Layout-Konvergenz im Worker ≤ 10 s, Zoom/Pan ≥ 30 fps, SVG-Export ≤ 5 s und ≤ 20 MB", async ({ page }, info) => {
    await openViz(page, bridge, "crawlGraph");
    await expect.poll(() => page.evaluate(() => window.__osViz!.firstFrameMs), { timeout: 10_000 }).not.toBeNull();
    const firstFrameMs = (await page.evaluate(() => window.__osViz!.firstFrameMs)) as number;
    expect(await page.evaluate(() => window.__osViz!.nodes)).toBe(10_000);
    await layoutDone(page, 30_000);
    const layout = await page.evaluate(() => window.__osViz!.layout);

    const frames = await page.evaluate(async () => {
      const path = "/src/grid/recorder.ts";
      const { recordFrames } = (await import(path)) as typeof import("../../src/grid/recorder");
      const v = window.__osViz!;
      const base = v.transform;
      return recordFrames((elapsed) => {
        const k = base.k * (1 + 0.5 * Math.sin(elapsed / 400));
        v.setTransform({ k, x: base.x + Math.sin(elapsed / 300) * 120, y: base.y + Math.cos(elapsed / 300) * 80 });
      }, 3_000);
    });

    // Ganzer Graph im Ausschnitt: Fläche bei kleinster Zoomstufe so groß wie nötig (SVG exportiert den Ausschnitt).
    const bounds = await page.evaluate(() => {
      const p = window.__osViz!.positions();
      return { minX: Math.min(...p.map((n) => n.x)), maxX: Math.max(...p.map((n) => n.x)), minY: Math.min(...p.map((n) => n.y)), maxY: Math.max(...p.map((n) => n.y)) };
    });
    const side = (span: number) => Math.min(8_000, Math.ceil(span * 0.05) + 400);
    await page.setViewportSize({ width: side(bounds.maxX - bounds.minX) + 300, height: side(bounds.maxY - bounds.minY) + 100 });
    // Die Fläche übernimmt die neue Größe erst im ResizeObserver (nächster Rendering-Schritt); bis dahin zeichnet und
    // exportiert sie mit der alten. Erst messen, wenn die Zeichenfläche die Größe ihrer Bühne meldet.
    await page.waitForFunction(() => {
      const c = document.querySelector<HTMLCanvasElement>(".os-viz-canvas");
      const stage = c?.parentElement?.getBoundingClientRect();
      return !!c && !!stage && parseFloat(c.style.width) === stage.width && parseFloat(c.style.height) === stage.height;
    });
    const box = await page.locator(".os-viz-canvas").boundingBox();
    await page.evaluate(
      ({ b, w, h }) => window.__osViz!.setTransform({ k: 0.05, x: w / 2 - ((b.minX + b.maxX) / 2) * 0.05, y: h / 2 - ((b.minY + b.maxY) / 2) * 0.05 }),
      { b: bounds, w: box!.width, h: box!.height },
    );
    const svg = await page.evaluate(() => {
      const t0 = performance.now();
      const text = window.__osViz!.svg();
      return { ms: performance.now() - t0, bytes: new Blob([text]).size, circles: (text.match(/class="node/g) ?? []).length };
    });

    const report = { firstFrameMs, layoutMs: layout.ms, layoutSteps: layout.steps, layoutReason: layout.reason, fps: frames.fps, p95FrameMs: frames.p95FrameMs, maxFrameMs: frames.maxFrameMs, svg, strict: STRICT };
    await info.attach("ui-146", { body: JSON.stringify(report, null, 2), contentType: "application/json" });
    console.log(`[ui-146] ${JSON.stringify(report)}`);
    expect(firstFrameMs).toBeLessThanOrEqual(2_000 * SLACK);
    expect(layout.reason).toBe("converged");
    expect(layout.ms!).toBeLessThanOrEqual(10_000 * SLACK);
    const minFps = 30 / SLACK;
    if (FPS_ADVISORY && frames.fps < minFps) {
      const text = `UI-146: ${frames.fps.toFixed(1)} fps unter ${minFps} fps (nur Warnung, OS_VIZ_FPS_ADVISORY=1)`;
      info.annotations.push({ type: "warning", description: text });
      console.log(`::warning title=UI-146 fps::${text}`);
    } else {
      expect(frames.fps).toBeGreaterThanOrEqual(minFps);
    }
    expect(svg.circles).toBe(10_000);
    expect(svg.ms).toBeLessThanOrEqual(5_000);
    expect(svg.bytes).toBeLessThanOrEqual(20 * 1024 * 1024);
  });

  // spec: UI-146
  test("Baumgraph 10 000 Knoten: Auf- und Zuklappen ≤ 200 ms bis zum gezeichneten Bild", async ({ page }, info) => {
    await openViz(page, bridge, "crawlTree");
    expect(await page.evaluate(() => window.__osViz!.nodes)).toBe(10_000);
    const toggle = () =>
      page.evaluate(
        () =>
          new Promise<number>((resolve) => {
            const v = window.__osViz!;
            const before = v.nodes;
            const t0 = performance.now();
            v.click(1);
            const wait = () => {
              if (window.__osViz!.nodes !== before) requestAnimationFrame(() => resolve(performance.now() - t0));
              else requestAnimationFrame(wait);
            };
            requestAnimationFrame(wait);
          }),
      );
    const collapse = await toggle();
    expect(await page.evaluate(() => window.__osViz!.nodes)).toBe(1);
    const expand = await toggle();
    expect(await page.evaluate(() => window.__osViz!.nodes)).toBe(10_000);
    await info.attach("ui-146-tree", { body: JSON.stringify({ collapse, expand, strict: STRICT }), contentType: "application/json" });
    console.log(`[ui-146-tree] ${JSON.stringify({ collapse, expand })}`);
    expect(collapse).toBeLessThanOrEqual(200 * SLACK);
    expect(expand).toBeLessThanOrEqual(200 * SLACK);
  });
});

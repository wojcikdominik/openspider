// Messrezept UI-098/UI-146 (M3-V02): 3D-Graph mit 100 000 Knoten (gen-ui-large-Teilmenge 1 / 40 / 2 000 / 97 959).
// Nur auf Anforderung (OS_VIZ_PERF3D=1), weil die Werte nur auf Referenzhardware (A) im Release-Build verbindlich sind;
// auf dem Server (Software-WebGL) gelten sie als Richtwert. Messwerte als Anhang `ui-098` und im Testprotokoll.
//   OS_VIZ_PERF3D=1 npx playwright test e2e/viz/perf3d.spec.ts --project=chromium --workers=1
import { expect, test } from "@playwright/test";
import { FakeVizCore } from "../../src/features/viz/fixtures/fakeCore";
import { genUiLarge } from "../../src/features/viz/fixtures/synthetic";
import { openViz } from "./support";
import { startVizBridge, type VizBridge } from "./viz-bridge";

const HUNDRED_K = [1, 40, 2000, 97_959];
let bridge: VizBridge;

test.skip(process.env.OS_VIZ_PERF3D !== "1", "Benchmark: Messrezept, nur mit OS_VIZ_PERF3D=1");
test.describe.configure({ timeout: 300_000, mode: "serial" });

test.beforeEach(async () => {
  bridge = await startVizBridge(new FakeVizCore(() => genUiLarge(HUNDRED_K)));
});
test.afterEach(async () => {
  await bridge.stop();
});

// spec: UI-098
// spec: UI-146 [M3]
test("3D-Crawl-Graph 100 000 Knoten: erstes Bild, Layout im Worker (≤ 10 s), Bildrate nach Konvergenz", async ({ page }, info) => {
  const opened = Date.now();
  await openViz(page, bridge, "crawlGraph3d");
  expect(await page.evaluate(() => window.__osViz3d!.webgl)).toBe(true);
  await expect.poll(() => page.evaluate(() => window.__osViz3d!.firstFrameMs), { timeout: 60_000 }).not.toBeNull();
  const firstFrameMs = await page.evaluate(() => window.__osViz3d!.firstFrameMs);
  expect(await page.evaluate(() => window.__osViz3d!.nodes)).toBe(100_000);
  await page.waitForFunction(() => window.__osViz3d?.layout.phase === "done", undefined, { timeout: 120_000 });
  const layout = await page.evaluate(() => window.__osViz3d!.layout);
  const idleFps = await page.evaluate(() => window.__osViz3d!.fps(3_000));
  // Während der Drehung (Kamera je Bild ändern) wie beim Linksziehen.
  const box = (await page.locator("canvas.os-viz-canvas-3d").boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  const measuring = page.evaluate(() => window.__osViz3d!.fps(3_000));
  for (let i = 0; i < 60; i++) await page.mouse.move(box.x + box.width / 2 + i * 4, box.y + box.height / 2 + (i % 10));
  const dragFps = await measuring;
  await page.mouse.up();
  // Nach der Suche (Kameraflug, Zoom ≥ 3, Beschriftungen der nächsten Knoten aktiv) – teuerster Fall der Beschriftung.
  await page.getByRole("searchbox", { name: "Search URL" }).fill("p50000.html");
  await expect(page.getByTestId("viz-search-count")).toHaveText("1 of 1");
  await expect.poll(() => page.evaluate(() => window.__osViz3d!.camera.k)).toBeGreaterThanOrEqual(3);
  await page.waitForTimeout(800);
  const searchFps = await page.evaluate(() => window.__osViz3d!.fps(3_000));
  // Drehen in der Nahansicht (Beschriftungen werden je Kamerabewegung neu bestimmt).
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  const nearMeasuring = page.evaluate(() => window.__osViz3d!.fps(3_000));
  for (let i = 0; i < 60; i++) await page.mouse.move(box.x + box.width / 2 + i * 4, box.y + box.height / 2 + (i % 10));
  const searchDragFps = await nearMeasuring;
  await page.mouse.up();
  const r1 = (v: number) => Math.round(v * 10) / 10;
  const result = { nodes: 100_000, openToFirstFrameMs: firstFrameMs, wallMs: Date.now() - opened, layout, idleFps: r1(idleFps), dragFps: r1(dragFps), searchFps: r1(searchFps), searchDragFps: r1(searchDragFps) };
  console.info(`UI-098 3D 100k: ${JSON.stringify(result)}`);
  await info.attach("ui-098", { body: JSON.stringify(result, null, 2), contentType: "application/json" });
  expect(layout.phase).toBe("done");
});

// E2E-A Visualisierungen (M2-U24) gegen die protokollgleiche Test-Bridge mit synthetischen Schnappschüssen: Fenster,
// Zoom/Pan/Ziehen, Tooltip, Suche, Kontextmenü, Exporte, „Daten geändert“, Tiefenkodierung, Budget, Wortwolke.
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { FakeVizCore } from "../../src/features/viz/fixtures/fakeCore";
import { fxUiBasic, fxUiDepth, fxUiDirs, genUiLarge } from "../../src/features/viz/fixtures/synthetic";
import { canvasBox, layoutDone, nodePoint, openViz, readDownload, validateSvg } from "./support";
import { startVizBridge, type VizBridge } from "./viz-bridge";

let bridge: VizBridge;

async function expectAccessible(page: Page) {
  const results = await new AxeBuilder({ page }).analyze();
  const serious = results.violations.filter((v) => v.impact === "critical" || v.impact === "serious");
  expect(serious.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
}

test.beforeEach(async () => {
  bridge = await startVizBridge(new FakeVizCore(fxUiBasic, fxUiDirs));
});

test.afterEach(async () => {
  await bridge.stop();
});

test.describe("Barrierefreiheit im Visualisierungsfenster", () => {
  // spec: UI-090
  // spec: UI-091
  test("Crawl-Baum mit Seitenleiste und offenem Kontextmenü ohne kritische oder schwerwiegende axe-Verstöße", async ({ page }) => {
    await openViz(page, bridge, "crawlTree");
    await expect(page.getByTestId("viz-node-count")).toHaveText("25 nodes");
    await expectAccessible(page);
    const p = await nodePoint(page, 3);
    await page.mouse.click(p.x, p.y, { button: "right" });
    await expect(page.getByRole("menuitem", { name: "Show in table" })).toBeVisible();
    await expectAccessible(page);
  });

  // spec: UI-096
  test("Rückfrage „Zu viele Unterseiten“ ohne kritische oder schwerwiegende axe-Verstöße", async ({ page }) => {
    await bridge.stop();
    bridge = await startVizBridge(new FakeVizCore(() => genUiLarge([1, 3000])));
    await openViz(page, bridge, "crawlTree");
    await page.getByLabel("Maximum nodes").fill("10");
    await page.getByLabel("Maximum nodes").press("Enter");
    await expect(page.getByTestId("viz-node-count")).toHaveText("10 nodes");
    await page.getByRole("searchbox", { name: "Search URL" }).fill("8000/");
    await page.locator(".os-viz-canvas").focus();
    await page.keyboard.press("Shift+F10");
    await page.getByRole("menuitem", { name: "Explore" }).click();
    const dialog = page.getByRole("dialog", { name: "Too many subpages" });
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("2,991 subpages");
    await expectAccessible(page);
    await dialog.getByRole("button", { name: "Focus" }).click();
    await expect(dialog).toHaveCount(0);
    await expect.poll(() => bridge.viz.calls.filter((c) => c.cmd === "viz.graph").at(-1)?.args).toMatchObject({ root: 1 });
  });

  // spec: UI-099
  test("Wortwolke mit SVG-Schaltflächen und Begriffstabelle ohne kritische oder schwerwiegende axe-Verstöße", async ({ page }) => {
    await openViz(page, bridge, "anchorWords", { root: 2 });
    await expect(page.getByTestId("viz-terms").getByRole("row")).toHaveCount(7);
    await expect(page.getByRole("button", { name: /^pricing: 12 links/ })).toBeVisible();
    await expectAccessible(page);
  });
});

test.describe("Visualisierungs-Rahmen", () => {
  // spec: UI-090
  test("mindestens vier Visualisierungsfenster gleichzeitig, nicht modal, je eigener Schnappschuss", async ({ page, context }) => {
    await page.goto(`/?bridge=${encodeURIComponent(bridge.url)}`);
    const popups: import("@playwright/test").Page[] = [];
    context.on("page", (p) => popups.push(p));
    await page.evaluate(async () => {
      const path = "/src/features/viz/open.ts";
      const { openVisualisation } = (await import(path)) as typeof import("../../src/features/viz/open");
      for (const kind of ["crawlTree", "crawlGraph", "directoryTree", "crawlTree"] as const) await openVisualisation(kind);
    });
    await expect.poll(() => popups.length).toBe(4);
    for (const p of popups) {
      await p.waitForFunction(() => (window.__osViz?.nodes ?? 0) > 0);
      await expect(p.locator(".os-viz-canvas")).toBeVisible();
    }
    expect(new Set(popups.map((p) => new URL(p.url()).searchParams.get("snapshotId"))).size).toBe(4);
    // Das Hauptfenster bleibt bedienbar (nicht modal).
    await expect(page.locator(".os-app")).toBeVisible();
  });

  // spec: UI-090
  test("Zoom per Rad zentriert auf den Zeiger und hart begrenzt; Verschieben auf freier Fläche; gezogene Knoten bleiben fixiert, Doppelklick löst", async ({ page }) => {
    await openViz(page, bridge, "crawlGraph");
    await layoutDone(page);
    const box = await canvasBox(page);
    const px = box.x + 200;
    const py = box.y + 150;
    await page.mouse.move(px, py);
    const before = await page.evaluate(() => window.__osViz!.transform);
    await page.mouse.wheel(0, -240);
    await expect.poll(() => page.evaluate(() => window.__osViz!.transform.k)).toBeGreaterThan(before.k);
    const after = await page.evaluate(() => window.__osViz!.transform);
    // Weltpunkt unter dem Zeiger bleibt stehen.
    const wx = (200 - before.x) / before.k;
    expect(wx * after.k + after.x).toBeCloseTo(200, 0);
    for (let i = 0; i < 40; i++) await page.mouse.wheel(0, -2000);
    await expect.poll(() => page.evaluate(() => window.__osViz!.transform.k)).toBe(8);
    for (let i = 0; i < 60; i++) await page.mouse.wheel(0, 2000);
    await expect.poll(() => page.evaluate(() => window.__osViz!.transform.k)).toBe(0.05);
    await page.evaluate(() => window.__osViz!.setTransform({ k: 1, x: 600, y: 350 }));

    // Verschieben auf freier Fläche (Ecke ohne Knoten).
    const t0 = await page.evaluate(() => window.__osViz!.transform);
    await page.mouse.move(box.x + 5, box.y + 5);
    await page.mouse.down();
    await page.mouse.move(box.x + 65, box.y + 45, { steps: 4 });
    await page.mouse.up();
    const t1 = await page.evaluate(() => window.__osViz!.transform);
    expect([t1.x - t0.x, t1.y - t0.y]).toEqual([60, 40]);

    // Knoten ziehen: bleibt nach Loslassen an der neuen Position fixiert.
    const start = await nodePoint(page, 5);
    await page.mouse.move(start.x, start.y);
    await page.mouse.down();
    await page.mouse.move(start.x + 80, start.y + 60, { steps: 5 });
    await page.mouse.up();
    const pinned = await page.evaluate(() => window.__osViz!.positions().find((n) => n.id === 5)!);
    await page.waitForTimeout(800);
    await layoutDone(page);
    const still = await page.evaluate(() => window.__osViz!.positions().find((n) => n.id === 5)!);
    expect(still).toEqual(pinned);
    const now = await nodePoint(page, 5);
    await page.mouse.dblclick(now.x, now.y);
    await page.waitForTimeout(300);
    await layoutDone(page);
    const released = await page.evaluate(() => window.__osViz!.positions().find((n) => n.id === 5)!);
    expect(released).not.toEqual(pinned);
  });

  // spec: UI-146
  test("zeichnet auf Canvas (kein SVG-DOM) mit Backing-Store = CSS-Größe × devicePixelRatio", async ({ browser }) => {
    const page = await browser.newPage({ viewport: { width: 1100, height: 700 }, deviceScaleFactor: 2 });
    try {
      await openViz(page, bridge, "crawlTree");
      await expect.poll(() => page.evaluate(() => window.__osViz!.firstFrameMs)).not.toBeNull();
      const info = await page.evaluate(() => {
        const c = document.querySelector("canvas.os-viz-canvas") as HTMLCanvasElement;
        const r = c.getBoundingClientRect();
        const d = c.getContext("2d")!.getImageData(0, 0, c.width, c.height).data;
        let painted = 0;
        for (let i = 0; i < d.length; i += 4) if (d[i + 3] !== 0) painted++;
        return { w: c.width, h: c.height, cssW: r.width, cssH: r.height, painted, total: c.width * c.height, svgNodes: document.querySelectorAll("svg circle").length };
      });
      expect(Math.abs(info.w - info.cssW * 2)).toBeLessThanOrEqual(1);
      expect(Math.abs(info.h - info.cssH * 2)).toBeLessThanOrEqual(1);
      expect(info.painted).toBe(info.total);
      expect(info.svgNodes).toBeLessThan(10);
    } finally {
      await page.close();
    }
  });

  // spec: UI-090
  test("Tooltip nach 300 ms mit genau den Zeilen Address, Status Code, Indexability, Crawl Depth, Unique Inlinks, Link Score, Metrik", async ({ page }) => {
    await openViz(page, bridge, "crawlTree");
    const p = await nodePoint(page, 2);
    // Die Verzögerung wird in der Seite gemessen (performance.now), nicht mit der Wanduhr des Testprozesses:
    // Zeitstempel jedes pointermove über der Zeichenfläche und des ersten Einfügens von [role=tooltip].
    await page.evaluate(() => {
      const probe = { moves: [] as number[], shown: null as number | null };
      (window as unknown as { __tipProbe: typeof probe }).__tipProbe = probe;
      if (document.querySelector('[role="tooltip"]')) throw new Error("Tooltip vor der Mausbewegung vorhanden");
      document.querySelector(".os-viz-canvas")!.addEventListener("pointermove", () => probe.moves.push(performance.now()), { capture: true });
      new MutationObserver((_, obs) => {
        if (probe.shown === null && document.querySelector('[role="tooltip"]')) {
          probe.shown = performance.now();
          obs.disconnect();
        }
      }).observe(document.body, { childList: true, subtree: true });
    });
    await page.mouse.move(p.x, p.y);
    const tip = page.getByRole("tooltip");
    await expect(tip).toBeVisible();
    const probe = await page.evaluate(() => (window as unknown as { __tipProbe: { moves: number[]; shown: number | null } }).__tipProbe);
    // Die erste Bewegung über der Zeichenfläche ist der früheste Beginn des Verweilens; der Tooltip darf frühestens
    // 300 ms danach im DOM stehen. Die Obergrenze setzt allein die Wartezeit von toBeVisible (UI-090 nennt keine).
    expect(probe.moves.length).toBeGreaterThanOrEqual(1);
    expect(probe.shown).not.toBeNull();
    const delay = probe.shown! - probe.moves[0]!;
    expect(delay).toBeGreaterThanOrEqual(300);
    expect(await tip.locator("dt").allTextContents()).toEqual(["Address", "Status Code", "Indexability", "Crawl Depth", "Unique Inlinks", "Link Score", "Crawl Depth"]);
    expect(await tip.locator("dd").allTextContents()).toEqual(["http://127.0.0.1:8000/page-1.html", "200", "Indexable", "1", "1", "11", "1"]);
  });

  // spec: UI-090
  test("Suche hebt alle Treffer hervor, zentriert den ersten, Enter/Shift+Enter wechseln, „i von n“", async ({ page }) => {
    await openViz(page, bridge, "crawlTree");
    await page.getByRole("searchbox", { name: "Search URL" }).fill("page-2");
    await expect(page.getByTestId("viz-search-count")).toHaveText("1 of 6");
    const box = await canvasBox(page);
    const center = await page.evaluate(() => window.__osViz!.screenOf(window.__osViz!.matches[0]!));
    expect(center![0]).toBeCloseTo(box.width / 2, 0);
    expect(center![1]).toBeCloseTo(box.height / 2, 0);
    const svg = await page.evaluate(() => window.__osViz!.svg());
    expect((svg.match(/class="node hit"/g) ?? []).length).toBeGreaterThan(0);
    await page.getByRole("searchbox", { name: "Search URL" }).press("Enter");
    await expect(page.getByTestId("viz-search-count")).toHaveText("2 of 6");
    await page.getByRole("searchbox", { name: "Search URL" }).press("Shift+Enter");
    await expect(page.getByTestId("viz-search-count")).toHaveText("1 of 6");
  });

  // spec: UI-090
  test("Kontextmenü „In Tabelle zeigen“ ruft das Hauptfenster mit der URL", async ({ page }) => {
    await openViz(page, bridge, "crawlTree");
    const p = await nodePoint(page, 3);
    await page.mouse.click(p.x, p.y, { button: "right" });
    await page.getByRole("menuitem", { name: "Show in table" }).click();
    await expect.poll(() => bridge.viz.calls.find((c) => c.cmd === "viz.focusInMain")?.args).toEqual({ urlId: 3 });
  });

  // spec: UI-090
  test("Exporte: SVG valide (Texte als <text>, Farben hex), PNG 2× der Fenstergröße, CSV mit genau den dargestellten Knoten", async ({ page }) => {
    const dir = mkdtempSync(join(tmpdir(), "viz-csv-"));
    try {
      await openViz(page, bridge, "crawlTree", { exportDir: dir });
      await page.evaluate(() => window.__osViz!.setTransform({ k: 1, x: 40, y: 380 }));
      const [svgDownload] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "SVG", exact: true }).click()]);
      expect(svgDownload.suggestedFilename()).toBe("crawlTree.svg");
      const svg = await validateSvg(page, (await readDownload(svgDownload)).toString("utf8"));
      expect(svg.circles).toBeGreaterThan(0);
      expect(svg.labels).toBeGreaterThan(0);

      const box = await canvasBox(page);
      const [pngDownload] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "PNG 2×" }).click()]);
      const png = await readDownload(pngDownload);
      expect(png.subarray(1, 4).toString("ascii")).toBe("PNG");
      expect(png.readUInt32BE(16)).toBe(Math.round(box.width * 2));
      expect(png.readUInt32BE(20)).toBe(Math.round(box.height * 2));

      await page.getByRole("button", { name: "Export nodes and edges (CSV)" }).click();
      await expect(page.getByTestId("viz-export-done")).toContainText(dir);
      const call = bridge.viz.calls.find((c) => c.cmd === "viz.exportCsv")!.args as { nodes: { x: number }[]; edges: unknown[] };
      expect(call.nodes).toHaveLength(await page.evaluate(() => window.__osViz!.nodes));
      expect(call.nodes.every((n) => Number.isFinite(n.x))).toBe(true);
      expect(call.edges).toHaveLength(24);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  // spec: UI-090
  test("„Daten geändert – Aktualisieren“ erscheint innerhalb von 2 s; erst der Klick lädt neu, Wurzel und Zoom bleiben", async ({ page }) => {
    const id = await openViz(page, bridge, "crawlTree");
    await page.evaluate(() => window.__osViz!.setTransform({ k: 2.5, x: -120, y: 90 }));
    const loads = () => bridge.viz.calls.filter((c) => c.cmd === "viz.graph").length;
    const before = loads();
    bridge.viz.replaceCrawl(() => {
      const t = fxUiBasic();
      t.nodes.get(2)!.statusCode = 500;
      return t;
    });
    const t0 = Date.now();
    bridge.viz.notifyChanged(id);
    await expect(page.getByTestId("viz-data-changed")).toBeVisible({ timeout: 2_000 });
    expect(Date.now() - t0).toBeLessThan(2_000);
    expect(loads()).toBe(before);
    await page.getByRole("button", { name: "Refresh" }).click();
    await expect(page.getByTestId("viz-data-changed")).toHaveCount(0);
    await expect.poll(loads).toBe(before + 1);
    expect(await page.evaluate(() => window.__osViz!.transform)).toEqual({ k: 2.5, x: -120, y: 90 });
  });
});

test.describe("Kraftgerichteter Crawl-Graph", () => {
  // spec: UI-092
  test("gleiche Eingabe und gleicher Startwert → identische Endkoordinaten; Wurzel nach Konvergenz in der Mitte (±1 px)", async ({ browser }) => {
    const a = await browser.newPage();
    const b = await browser.newPage();
    try {
      await openViz(a, bridge, "crawlGraph");
      await openViz(b, bridge, "crawlGraph");
      await layoutDone(a);
      await layoutDone(b);
      const csv = (p: import("@playwright/test").Page) => p.evaluate(() => JSON.stringify(window.__osViz!.csv()!.nodes.map((n) => [n.id, n.x, n.y])));
      expect(await csv(a)).toBe(await csv(b));
      const box = await canvasBox(a);
      const root = await a.evaluate(() => window.__osViz!.screenOf(1));
      expect(Math.abs(root![0] - box.width / 2)).toBeLessThanOrEqual(1);
      expect(Math.abs(root![1] - box.height / 2)).toBeLessThanOrEqual(1);
    } finally {
      await a.close();
      await b.close();
    }
  });

  // spec: UI-092
  test("Tiefenkodierung im SVG: Radien 14 / 11,2 / 8,96, ab Tiefe 7 Mindestradius 3; Beschriftungen erst ab Zoom 1,5 (fx-ui-depth)", async ({ page }) => {
    await bridge.stop();
    bridge = await startVizBridge(new FakeVizCore(fxUiDepth));
    await openViz(page, bridge, "crawlGraph");
    await layoutDone(page);
    await page.evaluate(() => window.__osViz!.setTransform({ k: 0.2, x: 600, y: 350 }));
    const low = await validateSvg(page, await page.evaluate(() => window.__osViz!.svg()));
    expect(low.radius.n1).toBeCloseTo(14, 2);
    expect(low.radius.n2).toBeCloseTo(11.2, 2);
    expect(low.radius.n4).toBeCloseTo(8.96, 2);
    expect(low.radius.n14).toBe(3);
    expect(low.labels).toBe(0);
    await page.evaluate(() => window.__osViz!.setTransform({ k: 1.6, x: 600, y: 350 }));
    const high = await validateSvg(page, await page.evaluate(() => window.__osViz!.svg()));
    expect(high.labels).toBeGreaterThan(0);
  });

  // spec: UI-092
  test("Simulation stoppt automatisch; danach kein Neuzeichnen im Leerlauf", async ({ page }) => {
    await openViz(page, bridge, "crawlGraph");
    await layoutDone(page);
    const frames = await page.evaluate(
      () =>
        new Promise<number>((resolve) => {
          let n = 0;
          const raf = window.requestAnimationFrame.bind(window);
          window.requestAnimationFrame = (cb) => {
            n++;
            return raf(cb);
          };
          setTimeout(() => {
            window.requestAnimationFrame = raf;
            resolve(n);
          }, 2_000);
        }),
    );
    expect(frames).toBe(0);
    await expect(page.getByTestId("viz-layout-state")).toHaveAttribute("data-phase", "done");
  });
});

test.describe("Verzeichnisbaum und Metriken", () => {
  // spec: UI-093
  test("zwei Verzeichnisse ohne Indexseite erscheinen hohl, Tooltip nur mit Pfad (fx-ui-dirs)", async ({ page }) => {
    await openViz(page, bridge, "directoryTree");
    const svg = await validateSvg(page, await page.evaluate(() => window.__osViz!.svg()));
    // Schema-Knoten (nm1) plus die zwei Verzeichnisse docs (nm2) und blog (nm3).
    expect(svg.hollow.sort()).toEqual(["nm1", "nm2", "nm3"]);
    const p = await nodePoint(page, -2);
    await page.mouse.move(p.x, p.y);
    const tip = page.getByRole("tooltip");
    await expect(tip).toBeVisible();
    expect(await tip.locator("dt").allTextContents()).toEqual(["Path"]);
    await expect(tip.locator("dd")).toHaveText("/docs");
  });

  // spec: UI-095
  test("Größe nach Word Count: Radienverhältnis 50 gegen 2 000 ≈ √40; Metriken ohne Daten ausgegraut", async ({ page }) => {
    await openViz(page, bridge, "crawlTree");
    const select = page.getByLabel("Scaling metric");
    await expect(select.locator("option", { hasText: "GSC Clicks" })).toBeDisabled();
    await select.selectOption("word_count");
    await page.getByText("Scale node size by metric").click();
    await expect(page.getByRole("switch", { name: "Scale node size by metric" })).toBeChecked();
    await page.getByLabel("Min. radius (px)").fill("1");
    await page.getByLabel("Min. radius (px)").press("Enter");
    await expect.poll(async () => (await validateSvg(page, await page.evaluate(() => window.__osViz!.svg()))).radius.n3).toBe(30);
    const svg = await validateSvg(page, await page.evaluate(() => window.__osViz!.svg()));
    expect(svg.radius.n3! / svg.radius.n2!).toBeCloseTo(Math.sqrt(40), 2);
  });
});

test.describe("Knotenbudget", () => {
  // spec: UI-096
  test("10 000 Knoten, gekappte Knoten mit Badge; „Teilbaum öffnen“ +≈24; „Als Wurzel setzen“ mit Pfadleiste, Rückkehr stellt Zoom und Position wieder her (gen-ui-large)", async ({ page }) => {
    await bridge.stop();
    bridge = await startVizBridge(new FakeVizCore(() => genUiLarge()));
    await openViz(page, bridge, "crawlTree");
    await expect(page.getByTestId("viz-node-count")).toHaveText("10,000 nodes");
    const capped = await page.evaluate(() => window.__osViz!.csv()!.nodes.filter((n) => n.hiddenChildren > 0).length);
    expect(capped).toBeGreaterThan(0);
    // Letzter gekappter Ebene-3-Knoten: über Suche ansteuern und per Tastatur-Kontextmenü öffnen.
    const lastCapped = await page.evaluate(() => {
      const dto = window.__osViz!.csv()!;
      const parents = new Set(dto.edges.map((e) => e.source));
      return dto.nodes.filter((n) => n.hiddenChildren > 0 && !parents.has(n.id)).at(-1)!;
    });
    await page.getByRole("searchbox", { name: "Search URL" }).fill(lastCapped.address!.replace("http://127.0.0.1:8000", ""));
    // Zentriert auf den Treffer: der gekappte Knoten trägt ein Badge mit der Zahl ausgeblendeter Kinder.
    await expect.poll(() => page.evaluate(() => window.__osViz!.svg().match(/<text class="badge"[^>]*>(\d+)</g)?.length ?? 0)).toBeGreaterThan(0);
    await page.locator(".os-viz-canvas").focus();
    await page.keyboard.press("Shift+F10");
    await page.getByRole("menuitem", { name: "Explore" }).click();
    await expect(page.getByTestId("viz-node-count")).toHaveText(`${(10_000 + lastCapped.hiddenChildren).toLocaleString("en")} nodes`);
    expect(lastCapped.hiddenChildren).toBeGreaterThanOrEqual(23);
    expect(lastCapped.hiddenChildren).toBeLessThanOrEqual(24);

    await page.evaluate(() => window.__osViz!.setTransform({ k: 0.5, x: 33, y: 44 }));
    await page.getByRole("searchbox", { name: "Search URL" }).fill("/l1/p2.html");
    await page.locator(".os-viz-canvas").focus();
    await page.keyboard.press("Shift+F10");
    const viewBefore = await page.evaluate(() => window.__osViz!.transform);
    await page.getByRole("menuitem", { name: "Focus" }).click();
    const trail = page.getByRole("navigation", { name: "Root path" });
    await expect(trail.getByRole("listitem")).toHaveCount(2);
    await expect(page.getByTestId("viz-node-count")).toHaveText(/^1,2\d\d nodes$/);
    await trail.getByRole("button").click();
    await expect(trail.getByRole("listitem")).toHaveCount(1);
    await expect.poll(() => page.evaluate(() => window.__osViz!.transform)).toEqual(viewBefore);
  });
});

test.describe("Ankertext-Wortwolke", () => {
  // spec: UI-099
  test("Wolke und Tabelle; Platzierung bei festem Startwert gleich; Klick auf einen Begriff meldet sich im Hauptfenster", async ({ page, browser }) => {
    await page.goto(`/?bridge=${encodeURIComponent(bridge.url)}`);
    await page.evaluate(() => {
      const w = window as unknown as { __anchor?: unknown };
      const ch = new BroadcastChannel("os-viz");
      ch.onmessage = (e) => (w.__anchor = e.data);
    });
    const viz = await page.context().newPage();
    await openViz(viz, bridge, "anchorWords", { root: 2 });
    const other = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    try {
      await openViz(other, bridge, "anchorWords", { root: 2 });
      const placed = (p: import("@playwright/test").Page) => p.evaluate(() => JSON.stringify(window.__osVizWords!.placed));
      expect(await placed(viz)).toBe(await placed(other));
    } finally {
      await other.close();
    }
    await expect(viz.getByTestId("viz-terms").getByRole("row")).toHaveCount(7);
    const sizes = await viz.evaluate(() => window.__osVizWords!.placed.map((p) => p.size));
    expect(Math.max(...sizes)).toBe(64);
    expect(Math.min(...sizes)).toBe(12);
    await viz.getByRole("button", { name: /^pricing: 12 links/ }).click();
    await expect.poll(() => page.evaluate(() => (window as unknown as { __anchor?: unknown }).__anchor)).toEqual({ type: "inlinksAnchor", urlId: 2, anchor: "pricing" });
    await expect.poll(() => bridge.viz.calls.find((c) => c.cmd === "viz.focusInMain")?.args).toEqual({ urlId: 2 });
  });
});

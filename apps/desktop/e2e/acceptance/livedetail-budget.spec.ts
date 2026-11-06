// Unabhängige Abnahme B4-LIVEDETAIL, Teil 5: Zeitbudget UI-145 als Regressionsprüfung im Live-Crawl gegen den Dev-Server.
//
// Einordnung nach docs/spec/07-performance.md: Die Zeitbudgets von UI-145 (URL Details ≤ 100 ms, Inlinks/Outlinks erste Seite ≤ 150 ms)
// gelten im Release-Build auf der Referenzhardware; E2E-Läufe gegen den Entwicklungs-Server „prüfen nur Regressionen mit eigenen
// Schwellen“. Die Spec nennt diese Schwellen nicht (Spec-Frage, siehe Bericht). Die einzige Regel mit Zahl ist UI-148: +10 % gegenüber
// dem gleitenden Median früherer Läufe. Der Test misst deshalb den Median des ersten Klicks und prüft
//   - immer: jede Messung liefert Daten (kein Ausbleiben im Live-Crawl, Wächter 5 s),
//   - wenn OS_LD_BASELINE_URLDETAILS_MS / OS_LD_BASELINE_LINKS_MS gesetzt sind (Median eines früheren Laufs, z. B. von `main`):
//     Median ≤ Basis × 1,10 (UI-148).
// Ohne Basis bleibt der Median ein berichteter Messwert; er wird nicht gegen eine selbst erfundene Schwelle geprüft.
// acceptance: independent
import { spawn } from "node:child_process";
import type { Page } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { startAlphaBridge, toolBinary } from "../alpha/support";
import { readUpper } from "./livedetail-support";

const REPS = Number(process.env.OS_LD_REPS ?? 7);
const median = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid]! : (sorted[mid - 1]! + sorted[mid]!) / 2;
};

async function firstClick(page: Page, url: string, kind: "urlDetails" | "links"): Promise<{ detailMs: number; selectMs: number }> {
  await page.goto(url);
  await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
  await expect(page.getByTestId("crawl-phase")).toHaveText("Running", { timeout: 20_000 });
  let address = "";
  await expect
    .poll(async () => {
      const grid = await readUpper(page);
      const row = grid?.rows[3];
      address = row?.cells[0] ?? "";
      return !!row && grid!.rows.every((r) => r.cells.every((c) => c !== "…"));
    }, { timeout: 20_000 })
    .toBe(true);
  if (kind === "links") {
    await page.getByRole("region", { name: "Details", exact: true }).getByRole("tab", { name: "Inlinks", exact: true }).click();
  }
  await page.evaluate(({ address, kind }) => {
    const w = window as unknown as { __ldFirst: Promise<{ detailMs: number; selectMs: number }> };
    w.__ldFirst = new Promise((resolve, reject) => {
      let down = 0;
      let selectMs = -1;
      document.addEventListener("pointerdown", () => { if (!down) down = performance.now(); }, { capture: true, once: true });
      const detailShows = () => {
        if (kind === "urlDetails") {
          const panel = document.querySelector("section.os-detail [role=tabpanel]");
          return !!panel && (panel as HTMLElement).innerText.includes(address);
        }
        const grid = [...document.querySelectorAll<HTMLElement>("section.os-detail .os-detail-grid .os-data-grid")].find((g) => !g.closest("[hidden]"));
        if (!grid) return false;
        const rows = [...grid.querySelectorAll("[role=row]")].filter((r) => r.getAttribute("aria-rowindex") !== "1");
        return rows.length > 0 && rows.every((r) => r.querySelectorAll("[role=gridcell]")[2]?.textContent === address);
      };
      const tick = () => {
        if (down) {
          if (selectMs < 0 && /1 selected/.test(document.querySelector("#data-panel .u10-counter")?.textContent ?? "")) selectMs = performance.now() - down;
          if (detailShows()) return resolve({ detailMs: performance.now() - down, selectMs });
          if (performance.now() - down > 5000) return reject(new Error("Detailfenster zeigt auch nach 5 s keine Daten der Auswahl"));
        }
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
  }, { address, kind });
  await page.locator("#data-panel .os-data-grid canvas").first().click({ position: { x: 160, y: 32 + 28 * 3 + 14 } });
  return page.evaluate(() => (window as unknown as { __ldFirst: Promise<{ detailMs: number; selectMs: number }> }).__ldFirst);
}

// spec: UI-145
// spec: UI-143
// spec: UI-148
// acceptance: independent
test("first click on a row during a running crawl: median time to the first detail page (regression, dev server)", async ({ page }) => {
  test.setTimeout(300_000);
  const site = spawn(toolBinary("os-testsite"), ["synthetic:n=8000,links=30,seed=7"], { stdio: ["ignore", "pipe", "inherit"] });
  const base = await new Promise<string>((resolve, reject) => {
    let output = "";
    const timer = setTimeout(() => reject(new Error("Fixture-Server startet nicht")), 20_000);
    site.stdout?.on("data", (d: Buffer) => {
      output += d.toString();
      const m = /-> (http:\/\/127\.0\.0\.1:\d+)/.exec(output);
      if (m) { clearTimeout(timer); resolve(m[1]!); }
    });
  });
  const bridge = await startAlphaBridge();
  try {
    const url = `/?${new URLSearchParams({ bridge: bridge.url })}`;
    await page.goto(url);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    await page.evaluate(async (base) => {
      const { config } = await import(/* @vite-ignore */ String("/src/ipc/config.ts"));
      const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
      const dto = await config.get();
      const root = dto.config as Record<string, Record<string, unknown>>;
      await config.set({ ...dto, config: { ...root, speed: { ...root.speed, threads: 4, max_urls_per_sec: 100 }, analysis: { ...root.analysis, auto_at_end: false } } });
      await crawl.start({ source: { kind: "spider", url: `${base}/` }, scope: "domain" });
    }, base);
    await expect.poll(() => page.evaluate(async () => {
      const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
      return useSession.getState().crawl?.crawled ?? 0;
    }), { timeout: 30_000 }).toBeGreaterThan(300);

    const detail: number[] = [];
    const links: number[] = [];
    const select: number[] = [];
    for (let i = 0; i < REPS; i++) {
      const r = await firstClick(page, url, "urlDetails");
      detail.push(r.detailMs); select.push(r.selectMs);
      expect(await page.getByTestId("crawl-phase").innerText(), "der Crawl läuft während der Messung").toBe("Running");
    }
    for (let i = 0; i < REPS; i++) {
      const r = await firstClick(page, url, "links");
      links.push(r.detailMs); select.push(r.selectMs);
      expect(await page.getByTestId("crawl-phase").innerText(), "der Crawl läuft während der Messung").toBe("Running");
    }
    const result = { reps: REPS, urlDetailsMs: detail.map(Math.round), linksMs: links.map(Math.round), selectionMs: select.map(Math.round),
      medianUrlDetailsMs: Math.round(median(detail)), medianLinksMs: Math.round(median(links)), medianSelectionMs: Math.round(median(select)) };
    console.log(`[livedetail-budget] ${JSON.stringify(result)}`);
    await test.info().attach("first-click-ms", { body: JSON.stringify(result), contentType: "application/json" });
    test.info().annotations.push({ type: "spec-question", description: "07-performance: Schwellen für E2E gegen den Dev-Server nicht beziffert; nur Median berichtet, Regression nur mit Basis (UI-148 +10 %)" });
    const baseDetail = Number(process.env.OS_LD_BASELINE_URLDETAILS_MS ?? NaN);
    const baseLinks = Number(process.env.OS_LD_BASELINE_LINKS_MS ?? NaN);
    if (Number.isFinite(baseDetail)) expect(median(detail), `Median erster Klick (URL Details) ≤ Basis ${baseDetail} ms × 1,10 (UI-148)`).toBeLessThanOrEqual(baseDetail * 1.1);
    if (Number.isFinite(baseLinks)) expect(median(links), `Median erster Klick (Inlinks) ≤ Basis ${baseLinks} ms × 1,10 (UI-148)`).toBeLessThanOrEqual(baseLinks * 1.1);
  } finally {
    await page.goto("about:blank").catch(() => undefined);
    await bridge.stop();
    site.kill();
  }
});

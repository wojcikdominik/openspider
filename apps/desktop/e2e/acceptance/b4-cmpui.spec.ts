// Unabhängige Abnahme B4-CMPUI (Playwright): Vergleichsmodus der Desktop-App gegen den echten Kern (os-app-bridge).
// Erwartungen stammen allein aus docs/spec: OUT-140 bis OUT-151 (führend OUT-142, OS-Festlegungen vom 09.11.2026), CRAWL-032 bis 036,
// CRAWL-127, CRAWL-144, UI-004, UI-007, UI-013, UI-121, UI-140. Der Produktcode wird nur zum Aufrufen benutzt.
// Eingaben sind Klicks, Tastatur und Mausrad; gelesen wird über Bedienelemente, die ARIA-Spiegelung der Tabelle und die Zeichenaufrufe der Canvas.
// acceptance: independent
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer, type Server } from "node:http";
import { expect, test, type BrowserContext, type Locator, type Page } from "@playwright/test";
import { startAlphaBridge, type AlphaBridge } from "../alpha/support";
import { bridgeBinary } from "../support/bridge";
import type { CrawlListing } from "../../src/ipc/gen/CrawlListing";
import type {} from "../compare/harness";

// ---------------------------------------------------------------------------------------------------------------------------------
// Fixture: zwei Versionen einer Website mit je > 1.200 Seiten (Zahlen > 999, damit das Zahlenformat sichtbar wird).
//   v1: "/" -> Hubs h0..h12; h0..h11 verlinken je 100 Seiten (p0..p1199), h12 nichts; p5 liefert 404.
//   v2: p100..p139 nicht mehr verlinkt (40 Seiten "Fehlend"), h12 verlinkt p1200..p1299 (100 Seiten "Neu"), p5 liefert 200.
// Internal:HTML Current = 1 + 13 + 1260 = 1.274, Previous = 1 + 13 + 1200 = 1.214, Neu = 100, Fehlend = 40.
// ---------------------------------------------------------------------------------------------------------------------------------
const HUBS = 13;
const MISSING = 40;
const NEW = 100;
const CURRENT_HTML = 1274;
const PREVIOUS_HTML = 1214;

let version = 1;
const words = (n: number) => Array.from({ length: n }, (_, i) => `wort${i % 50}`).join(" ");
const pageWords = (i: number) => (i % 3 === 0 ? 5 : i % 3 === 1 ? 120 : 1234);
function hubLinks(h: number): number[] {
  if (h < 12) return Array.from({ length: 100 }, (_, k) => h * 100 + k).filter((i) => version === 1 || i < 100 || i >= 100 + MISSING);
  return version === 2 ? Array.from({ length: NEW }, (_, k) => 1200 + k) : [];
}
function startSite(): Promise<{ server: Server; port: number }> {
  const server = createServer((req, res) => {
    const url = req.url ?? "/";
    const send = (status: number, body: string, type = "text/html; charset=utf-8") => {
      res.statusCode = status;
      res.setHeader("content-type", type);
      res.end(body);
    };
    if (url === "/robots.txt") return send(200, "User-agent: *\nAllow: /\n", "text/plain");
    if (url === "/") return send(200, `<!doctype html><title>Start</title><h1>Start</h1>${Array.from({ length: HUBS }, (_, h) => `<a href="/h${h}">Hub ${h}</a>`).join(" ")}`);
    if (url === "/tiny") return send(200, "<!doctype html><title>Tiny</title><h1>Tiny</h1><p>Eine kleine Seite</p>");
    const hub = /^\/h(\d+)$/.exec(url);
    if (hub) return send(200, `<!doctype html><title>Hub ${hub[1]}</title><h1>Hub</h1>${hubLinks(Number(hub[1])).map((i) => `<a href="/p${i}">Seite ${i}</a>`).join(" ")}`);
    const p = /^\/p(\d+)$/.exec(url);
    if (p) {
      const i = Number(p[1]);
      const status = i === 5 && version === 1 ? 404 : 200;
      return send(status, `<!doctype html><title>Seite ${i}</title><h1>Seite ${i}</h1><p>${words(pageWords(i))}</p><a href="/">Start</a>`);
    }
    send(404, "<!doctype html><title>Nicht gefunden</title><h1>404</h1>");
  });
  return new Promise((ok) => server.listen(0, "127.0.0.1", () => ok({ server, port: (server.address() as { port: number }).port })));
}

/** Richtet über die Harness-Seite (echter Kern) einen Crawl der Fixture ein und wartet, bis er gesichert ist. */
async function crawlOnce(page: Page, url: string): Promise<string> {
  const before = await page.evaluate(() => window.compareHarness.invoke<CrawlListing[]>("crawls.list", { show: "crawls" }));
  await page.evaluate(async (url) => {
    await window.compareHarness.invoke("crawl.start", { source: { kind: "spider", url }, scope: "domain" });
  }, url);
  await expect
    .poll(
      async () => {
        const rows = await page.evaluate(() => window.compareHarness.invoke<CrawlListing[]>("crawls.list", { show: "crawls" }));
        return rows.length === before.length + 1 && rows.every((r) => r.crawl.phase === "completed");
      },
      { timeout: 150_000, intervals: [500, 1000] },
    )
    .toBe(true);
  const rows = await page.evaluate(() => window.compareHarness.invoke<CrawlListing[]>("crawls.list", { show: "crawls" }));
  await expect
    .poll(
      () =>
        page.evaluate(async () => {
          try {
            await window.compareHarness.invoke("crawl.clear");
            return true;
          } catch (e) {
            if ((e as { code: string }).code === "Busy") return false;
            throw e;
          }
        }),
      { timeout: 60_000 },
    )
    .toBe(true);
  return rows.find((r) => !before.some((b) => b.crawl.crawlId === r.crawl.crawlId))!.crawl.crawlId;
}


/**
 * Bridge mit festem Home: Bei einem Neustart des Workers (nach einem roten Test) bleiben die zwei aufwendigen Crawls erhalten.
 * Das Verzeichnis trägt die PID des Playwright-Läufers und wird vom Aufrufer nach dem Lauf entfernt.
 */
async function startBridgeAt(home: string): Promise<AlphaBridge> {
  mkdirSync(home, { recursive: true });
  const child = spawn(bridgeBinary(), ["--home", home], { stdio: ["ignore", "pipe", "inherit"] });
  const info = await new Promise<{ port: number; token: string }>((ok, fail) => {
    let buf = "";
    const timer = setTimeout(() => fail(new Error("os-app-bridge meldet keinen Port")), 30_000);
    child.stdout?.on("data", (d: Buffer) => {
      buf += d.toString();
      const nl = buf.indexOf("\n");
      if (nl >= 0) {
        clearTimeout(timer);
        ok(JSON.parse(buf.slice(0, nl)));
      }
    });
    child.on("exit", () => fail(new Error("os-app-bridge beendet sich vorzeitig")));
  });
  return { url: `ws://127.0.0.1:${info.port}/?token=${info.token}`, home, crawlsDir: join(home, "crawls"), async stop() { child.kill(); await new Promise((r) => setTimeout(r, 100)); } };
}

const MOD = process.platform === "darwin" ? "Meta" : "Control";
type Lang = "de" | "en";

/** Öffnet das Hauptfenster gegen die Bridge und stellt die Oberflächensprache über das Einstellungsfenster ein. */
async function openMain(page: Page, context: BrowserContext, bridge: AlphaBridge, lang: Lang): Promise<void> {
  const query = `bridge=${encodeURIComponent(bridge.url)}`;
  await page.goto(`/?${query}`);
  await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
  if ((await page.locator("html").getAttribute("lang")) !== lang) {
    const settings = await context.newPage();
    await settings.goto(`/settings.html?${query}`);
    await settings.getByRole("button", { name: /Language|Sprache/ }).click();
    await settings.getByRole("option", { name: lang === "de" ? "Deutsch" : "English", exact: true }).click();
    await expect(page.locator("html")).toHaveAttribute("lang", lang);
    await settings.close();
    await page.reload();
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    await expect(page.locator("html")).toHaveAttribute("lang", lang);
  }
}

const L = {
  de: { compareOpen: "Crawls vergleichen", run: "Vergleichen", grid: "Vergleichstabelle", swap: "Current / Previous tauschen", close: "Vergleich schließen", mode: "Modus: Vergleich" },
  en: { compareOpen: "Compare crawls", run: "Compare", grid: "Comparison table", swap: "Swap Current / Previous", close: "Close comparison", mode: "Mode: Comparison" },
};

/** Vergleich über die Oberfläche starten: Schaltfläche, Dialog, „Vergleichen“. Standard: neuerer Crawl ist Current. */
async function startComparison(page: Page, lang: Lang): Promise<Locator> {
  await page.getByRole("button", { name: L[lang].compareOpen, exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: L[lang].run, exact: true }).click();
  const grid = page.getByRole("grid", { name: L[lang].grid, exact: true });
  await expect(grid).toBeVisible({ timeout: 60_000 });
  await expect(grid).toHaveAttribute("aria-rowcount", /^\d{3,}$/, { timeout: 60_000 });
  return grid;
}

const region = (page: Page) => page.locator('[data-region="table"]');
const sidebar = (page: Page) => page.locator(".os-app-sidebar");

interface Fill {
  t: string;
  x: number;
  y: number;
  a: string;
}
/** Zeichenaufrufe der Tabellen-Canvas (Text, Position, Ausrichtung) – vor dem Laden der Seite eingehängt. */
const RECORDER = () => {
  const w = window as unknown as { __fills: unknown[] };
  w.__fills = [];
  const original = CanvasRenderingContext2D.prototype.fillText;
  CanvasRenderingContext2D.prototype.fillText = function (this: CanvasRenderingContext2D, text: string, x: number, y: number, maxWidth?: number) {
    if (w.__fills.length < 60_000) w.__fills.push({ t: String(text), x, y, a: this.textAlign });
    return maxWidth === undefined ? original.call(this, text, x, y) : original.call(this, text, x, y, maxWidth);
  };
};
const fills = (page: Page) => page.evaluate(() => (window as unknown as { __fills: Fill[] }).__fills);
const clearFills = (page: Page) => page.evaluate(() => void ((window as unknown as { __fills: Fill[] }).__fills.length = 0));

/** Sichtbarer Ausschnitt der Tabelle über die ARIA-Spiegelung. */
async function gridWindow(grid: Locator) {
  return grid.evaluate((root) => {
    const rows = [...root.querySelectorAll<HTMLElement>(".os-grid-a11y [role=row]")];
    const head = rows.find((r) => r.getAttribute("aria-rowindex") === "1");
    return {
      count: rows.length,
      headers: [...(head?.querySelectorAll("[role=columnheader]") ?? [])].map((c) => (c.textContent ?? "").trim()),
      rows: rows
        .filter((r) => r !== head)
        .map((r) => ({ index: Number(r.getAttribute("aria-rowindex")), selected: r.getAttribute("aria-selected") === "true", cells: [...r.querySelectorAll("[role=gridcell]")].map((c) => (c.textContent ?? "").trim()) })),
    };
  });
}
/** Wartet, bis die sichtbaren Zeilen geladen sind (keine Platzhalter „…“); Spiegelung und Canvas entstehen im selben Rendern. */
const settled = (grid: Locator) =>
  expect
    .poll(async () => {
      const win = await gridWindow(grid);
      return win.rows.length > 0 && win.rows.every((r) => r.cells.every((c) => c !== "…"));
    })
    .toBe(true);
const box = async (loc: Locator) => {
  const b = await loc.boundingBox();
  expect(b, "Element sichtbar").not.toBeNull();
  return b!;
};
const selectedText = (select: Locator) => select.evaluate((el) => (el as HTMLSelectElement).selectedOptions[0]?.textContent?.trim() ?? "");
const num = (text: string) => Number(text.replace(/[.,\s]/g, ""));

function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!;
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') { field += '"'; i++; }
      else if (c === '"') quoted = false;
      else field += c;
    } else if (c === '"') quoted = true;
    else if (c === ",") { row.push(field); field = ""; }
    else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      row.push(field); rows.push(row); row = []; field = "";
    } else field += c;
  }
  if (field || row.length) { row.push(field); rows.push(row); }
  return rows;
}

// ---------------------------------------------------------------------------------------------------------------------------------
test.describe("Vergleichsmodus gegen den echten Kern", () => {
  // RC1-FLAKES: all tests of this group share one bridge home (the two prepared crawls, settings such as language and
  // columns). With `fullyParallel` several workers ran the group at once against the same home: each crawled into it
  // (`crawlOnce` then never saw exactly one new crawl), and the worker that prepared it removed it while others used it.
  // The group therefore runs in one worker per project, in order; the home is separate per project.
  test.describe.configure({ mode: "default" });
  let bridge: AlphaBridge;
  let site: { server: Server; port: number };
  // B4-CMPUI2 (A7, teardown only): the home this run prepared is removed afterwards; a given OS_CMPUI_HOME or a home
  // prepared by an earlier run stays.
  let preparedHome: string | undefined;
  test.beforeAll(async ({ browser }, workerInfo) => {
    test.setTimeout(420_000);
    const home = process.env.OS_CMPUI_HOME ?? join(tmpdir(), `os-cmpui-${process.ppid}-${workerInfo.project.name}`);
    bridge = await startBridgeAt(home);
    site = await startSite();
    if (existsSync(join(home, "prepared"))) return;
    const page = await browser.newPage();
    try {
      await page.goto(`/e2e/compare/index.html?bridge=${encodeURIComponent(bridge.url)}&site=x`);
      await page.waitForFunction(() => !!window.compareHarness);
      version = 1;
      await crawlOnce(page, `http://127.0.0.1:${site.port}/`);
      version = 2;
      await crawlOnce(page, `http://127.0.0.1:${site.port}/`);
      writeFileSync(join(home, "prepared"), "ok");
      preparedHome = home;
    } finally {
      await page.close();
    }
  });
  test.afterAll(async () => {
    await bridge?.stop();
    await new Promise<void>((ok) => (site ? site.server.close(() => ok()) : ok()));
    if (preparedHome) rmSync(preparedHome, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  });
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(RECORDER);
    test.setTimeout(150_000);
  });

  // spec: OUT-142
  // spec: UI-007
  // spec: UI-001
  for (const width of [1280, 2000]) {
    test(`Tabelle füllt den Bereich bei ${width} px Fensterbreite, Spalten ohne Seitenknöpfe`, async ({ page, context }) => {
      await page.setViewportSize({ width, height: 900 });
      await openMain(page, context, bridge, "de");
      const grid = await startComparison(page, "de");
      const main = await box(region(page));
      const g = await box(grid);
      const side = await box(sidebar(page));
      // Die Tabelle nimmt die Breite des Tabellenbereichs ein (nicht ein kleiner Block in der Mitte).
      expect(g.width, `Tabelle ${g.width} px, Bereich ${main.width} px`).toBeGreaterThanOrEqual(main.width - 2);
      expect(Math.abs(g.x - main.x), "Tabelle beginnt am linken Rand des Bereichs").toBeLessThanOrEqual(2);
      expect(main.width, "Der Tabellenbereich nutzt die Fensterbreite abzüglich Seitenleiste").toBeGreaterThanOrEqual(width - side.width - 24);
      expect(g.height, `Tabellenhöhe ${g.height} px, Bereich ${main.height} px`).toBeGreaterThanOrEqual(main.height * 0.6);
      expect(g.y + g.height).toBeLessThanOrEqual(main.y + main.height + 1);
      // Die Zeichenfläche füllt die Tabelle (keine Skalierung eines kleinen Blocks).
      const canvas = await box(grid.locator("canvas"));
      expect(canvas.width).toBeGreaterThanOrEqual(g.width - 24);
      expect(canvas.height).toBeGreaterThanOrEqual(g.height - 24); // abzüglich Bildlaufleiste
      // Keine Seitenknöpfe: eine scrollbare, virtualisierte Tabelle.
      await expect(region(page).getByRole("button", { name: /Nächste Seite|Vorherige Seite|Next page|Previous page/ })).toHaveCount(0);
      // Nichts wird abgeschnitten: Address und Vergleichsstatus sind ohne horizontales Scrollen vorhanden und die Köpfe liegen im Bereich.
      const win = await gridWindow(grid);
      expect(win.headers.length).toBeGreaterThanOrEqual(3);
      // Die Detailansicht darunter füllt ihren Bereich ebenfalls.
      await grid.click({ position: { x: 140, y: 70 } });
      const detail = page.locator(".os-shell-detail");
      await expect(detail.locator(".os-compare-detail")).toBeVisible();
      const d = await box(detail);
      const dc = await box(detail.locator(".os-compare-detail"));
      expect(dc.width, `Detail ${dc.width} px, Bereich ${d.width} px`).toBeGreaterThanOrEqual(d.width - 12);
    });
  }

  // spec: OUT-142
  // spec: UI-001
  test("Steuerleiste der Tabelle sitzt oben links im Tabellenbereich, Tausch und Schließen sind im oberen Bereich erreichbar", async ({ page, context }) => {
    await openMain(page, context, bridge, "de");
    await startComparison(page, "de");
    const main = await box(region(page));
    const firstControl = await box(region(page).locator("label").first());
    expect(firstControl.x - main.x, "erste Steuerung nahe am linken Rand").toBeLessThanOrEqual(24);
    expect(firstControl.y - main.y, "erste Steuerung nahe am oberen Rand").toBeLessThanOrEqual(24);
    // Alle Steuerungen der Zeile liegen oberhalb der Tabelle.
    const grid = await box(page.getByRole("grid", { name: L.de.grid, exact: true }));
    for (const control of await region(page).locator("label, button").all()) {
      const b = await control.boundingBox();
      if (b && b.y < grid.y) expect(b.y + b.height).toBeLessThanOrEqual(grid.y + 1);
    }
    for (const name of [L.de.swap, L.de.close]) {
      const b = await box(page.getByRole("button", { name, exact: true }));
      expect(b.y, `${name} im oberen Fensterbereich`).toBeLessThan(main.y);
    }
  });

  // spec: UI-140
  // spec: UI-007
  // spec: OUT-142
  test("Virtualisiertes Scrollen mit Mausrad und Tastatur erreicht alle Zeilen, DOM bleibt klein, Zeilenzahl deutsch formatiert", async ({ page, context }) => {
    await openMain(page, context, bridge, "de");
    const grid = await startComparison(page, "de");
    const total = Number(await grid.getAttribute("aria-rowcount")) - 1;
    // Zeilenzähler: Gesamtzahl mit deutschem Tausenderpunkt, gleich der Zeilenzahl der Tabelle.
    const counter = region(page).getByRole("status").filter({ hasText: /Zeilen/ });
    await expect(counter).toHaveText(new RegExp(`^\\s*${String(total).replace(/(\d)(?=(\d{3})+$)/g, "$1\\.")}\\s+Zeilen`));
    expect(total).toBeGreaterThanOrEqual(CURRENT_HTML);
    expect((await gridWindow(grid)).count, "DOM-Zeilen unabhängig von der Datengröße < 300 (UI-140)").toBeLessThan(300);
    // Mausrad: tief in die Tabelle.
    const b = await box(grid);
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
    const first = async () => Math.min(...(await gridWindow(grid)).rows.map((r) => r.index));
    const start = await first();
    for (let i = 0; i < 12; i++) await page.mouse.wheel(0, 3000);
    await expect.poll(first, { message: "Mausrad scrollt durch die Zeilen" }).toBeGreaterThan(start + 300);
    // Tastatur: Ende erreicht die letzte Zeile, Anfang die erste.
    await grid.click({ position: { x: 140, y: 70 } });
    await page.keyboard.press("End");
    await expect.poll(async () => Math.max(...(await gridWindow(grid)).rows.map((r) => r.index)), { message: "letzte Zeile erreichbar" }).toBe(total + 1);
    const last = (await gridWindow(grid)).rows.find((r) => r.index === total + 1)!;
    expect(last.selected).toBe(true);
    expect(last.cells[0]).not.toBe("…");
    await page.keyboard.press("Home");
    await expect.poll(first).toBeLessThanOrEqual(2);
    expect((await gridWindow(grid)).count).toBeLessThan(300);
    await expect(region(page).getByRole("button", { name: /Seite/ })).toHaveCount(0);
  });

  // spec: OUT-142
  // spec: OUT-151
  // spec: UI-007
  test("Spalten: Address, dann Vergleichsstatus, dann die Spalten des Tabs; Zahlen rechtsbündig und lokalisiert", async ({ page, context }) => {
    await page.setViewportSize({ width: 2400, height: 1300 });
    await openMain(page, context, bridge, "de");
    const grid = await startComparison(page, "de");
    const win = await gridWindow(grid);
    const heads = win.headers;
    const status = heads.indexOf("Vergleichsstatus");
    expect(status, `Spaltenköpfe: ${heads.join(" | ")}`).toBeGreaterThan(0);
    // Vor dem Vergleichsstatus steht nur die Adresse (und die feste Zeilennummer).
    expect(heads.slice(0, status).filter((h) => h !== "Row" && h !== "Zeile")).toHaveLength(1);
    expect(heads.slice(0, status).at(-1)).toMatch(/^Address$|^Adresse$/);
    expect(heads.length, "weitere Spalten des Tabs folgen").toBeGreaterThan(status + 2);
    // Bei aktivem URL-Mapping stünde „Vorherige Adresse“ direkt danach; ohne Mapping gibt es sie nicht.
    expect(heads).not.toContain("Vorherige Adresse");
    // Adressen und Statuswerte stehen linksbündig (Textzellen); gezeichnet vor dem Verschieben der Ansicht.
    await clearFills(page);
    const first = await box(grid);
    await page.mouse.move(first.x + first.width / 2, first.y + first.height / 2);
    await page.mouse.wheel(0, 48);
    await expect.poll(async () => (await fills(page)).length).toBeGreaterThan(20);
    const early = await fills(page);
    const statusCells = early.filter((f) => ["Unverändert", "Neu", "Fehlend", "Hinzugefügt", "Entfernt"].includes(f.t));
    expect(statusCells.length, "Statuswerte in der Tabelle gezeichnet").toBeGreaterThan(0);
    expect(statusCells.every((f) => f.a === "left")).toBe(true);
    expect(early.filter((f) => f.t.startsWith("http")).every((f) => f.a === "left")).toBe(true);
    let withHeaders: string[] = [];
    // Zahlenspalten über den Spalten-Konfigurator (wie im normalen Modus) einblenden: Wortanzahl und Eingehende Links.
    await region(page).getByRole("button", { name: "Spalten konfigurieren", exact: true }).click();
    const columnsDialog = page.getByRole("dialog", { name: "Spalten konfigurieren" });
    for (const name of ["Wortanzahl", "Eingehende Links"]) await columnsDialog.getByRole("checkbox", { name, exact: true }).check();
    await columnsDialog.getByRole("button", { name: "Anwenden", exact: true }).click();
    await expect(columnsDialog).toHaveCount(0);
    // Die neuen Spalten stehen rechts der bisherigen: mit dem Mausrad dorthin scrollen.
    const where = await box(grid);
    await page.mouse.move(where.x + where.width / 2, where.y + where.height / 2);
    for (let i = 0; i < 30 && !["Wortanzahl", "Eingehende Links"].every((h) => withHeaders.includes(h)); i++) {
      await page.mouse.wheel(300, 0);
      await page.waitForTimeout(150);
      withHeaders = (await gridWindow(grid)).headers;
    }
    expect(withHeaders, "Wortanzahl und Eingehende Links erreichbar").toEqual(expect.arrayContaining(["Wortanzahl", "Eingehende Links"]));
    // Die Zeichenaufrufe belegen Ausrichtung und Format: Zahlen rechts (4 Stellen mit Tausenderpunkt), Adressen und Status links.
    await clearFills(page);
    const b = await box(grid);
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
    await page.mouse.wheel(0, 120);
    await expect.poll(async () => (await fills(page)).length).toBeGreaterThan(20);
    const drawn = await fills(page);
    const numeric = drawn.filter((f) => /^\d{1,3}(\.\d{3})*$/.test(f.t));
    expect(numeric.length, "numerische Zellen gezeichnet").toBeGreaterThan(3);
    for (const f of numeric) expect(f.a, `Zahl „${f.t}“ rechtsbündig`).toBe("right");
    const withThousands = drawn.filter((f) => /^\d{1,3}(\.\d{3})+$/.test(f.t));
    expect(withThousands.length, `Zahl ≥ 1000 mit deutschem Tausenderpunkt (gezeichnet: ${[...new Set(drawn.map((f) => f.t))].slice(0, 60).join(" | ")})`).toBeGreaterThan(0);
    // Rechtsbündig heißt: gleiche rechte Kante in einer Spalte bei verschieden langen Zahlen.
    const byX = new Map<number, Set<number>>();
    for (const f of numeric) byX.set(f.x, (byX.get(f.x) ?? new Set()).add(f.t.length));
    expect([...byX.values()].some((lengths) => lengths.size >= 2), "gleiche rechte Kante bei Zahlen verschiedener Länge").toBe(true);
    // Aufräumen: Spaltenauswahl des Tabs zurücksetzen (die Einstellung gilt auch im normalen Modus).
    await region(page).getByRole("button", { name: "Spalten konfigurieren", exact: true }).click();
    await page.getByRole("dialog", { name: "Spalten konfigurieren" }).getByRole("button", { name: "Spalten zurücksetzen", exact: true }).click();
  });

  // spec: UI-007
  // spec: OUT-142
  test("Horizontales Scrollen: die angeheftete Spalte bleibt stehen und wird von keiner anderen Spalte überzeichnet", async ({ page, context }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await openMain(page, context, bridge, "de");
    const grid = await startComparison(page, "de");
    const b = await box(grid);
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
    // Eine Wheel-Geste, damit die Zeichenaufrufe eines vollständigen Neuzeichnens vorliegen.
    await clearFills(page);
    await page.mouse.wheel(0, 48);
    await expect.poll(async () => (await fills(page)).length).toBeGreaterThan(20);
    const before = await fills(page);
    // „⋮“ in Datenzeilen gehört zur Adressspalte: ihre rechte Kante begrenzt den angehefteten Bereich.
    const dots = before.filter((f) => f.t === "⋮");
    const headerY = Math.min(...dots.map((d) => d.y));
    const pinnedRight = dots.filter((d) => d.y > headerY + 1).map((d) => d.x)[0]!;
    expect(pinnedRight, "Adressspalte gezeichnet").toBeGreaterThan(100);
    const region = () =>
      grid.evaluate((root, right) => {
        const canvas = root.querySelector("canvas")!;
        const scale = canvas.width / canvas.getBoundingClientRect().width;
        const data = canvas.getContext("2d")!.getImageData(0, 0, Math.floor(right * scale), canvas.height).data;
        let hash = 2166136261;
        for (let i = 0; i < data.length; i += 7) hash = Math.imul(hash ^ data[i]!, 16777619) >>> 0;
        return hash;
      }, pinnedRight);
    const scroller = grid.locator(".os-grid-scroll");
    expect(await scroller.evaluate((el) => el.scrollWidth > el.clientWidth), "bei 1280 px gibt es weitere Spalten rechts").toBe(true);
    // Pixel vergleichen erst, wenn die Zeilen geladen sind: ein Platzhalter „…“ im Referenzbild ist kein Überzeichnen.
    await settled(grid);
    const reference = await region();
    for (let i = 0; i < 6; i++) await page.mouse.wheel(500, 0);
    await expect.poll(() => scroller.evaluate((el) => el.scrollLeft)).toBeGreaterThan(300);
    // Nach dem Scrollen zeigt die rechte Seite andere Spalten …
    await clearFills(page);
    await page.mouse.wheel(60, 0);
    await expect.poll(async () => (await fills(page)).length).toBeGreaterThan(20);
    const headersNow = (await gridWindow(grid)).headers;
    expect(headersNow.some((h) => /^Address$|^Adresse$/.test(h)), "Adresse bleibt sichtbar").toBe(true);
    expect(headersNow).not.toContain("Statuscode");
    // … aber die angeheftete Spalte (Pixel links von ihrer rechten Kante) ist unverändert.
    await settled(grid);
    expect(await region(), "angeheftete Spalte unverändert").toBe(reference);
    // Adresse bleibt als erste Spalte in der Spiegelung vorhanden.
    expect(headersNow.some((h) => /^Address$|^Adresse$/.test(h))).toBe(true);
    const after = await fills(page);
    expect(after.some((f) => f.t.startsWith("http") && f.x < pinnedRight), "Adressen weiter im angehefteten Bereich").toBe(true);
  });

  // spec: UI-004
  // spec: UI-121
  // spec: OUT-142
  test("Deutsche Oberfläche: Tab-Leiste, Auswahlwerte, Statuswerte, Übersichtstitel und Zahlformat 1.274", async ({ page, context }) => {
    await openMain(page, context, bridge, "de");
    const grid = await startComparison(page, "de");
    // Tab-Leiste: keine englischen Tab- und Gruppennamen.
    const tabsText = (await page.locator("nav.os-app-tabs").innerText()).replace(/\s+/g, " ");
    for (const english of ["Internal", "External", "Response Codes", "Page Titles", "Meta Description", "Change Detection", "Overview", "Content ▾"]) expect(tabsText, `Tab-Leiste: ${english}`).not.toContain(english);
    await expect(page.getByRole("tab", { name: "Intern", exact: true })).toBeVisible();
    // Auswahlwerte der Tabelle: Kategorien deutsch.
    const category = region(page).getByRole("combobox", { name: "Vergleichsstatus" });
    const options = await category.locator("option").allTextContents();
    for (const name of ["Hinzugefügt", "Neu", "Entfernt", "Fehlend"]) expect(options).toContain(name);
    for (const english of ["Added", "New", "Removed", "Missing", "All"]) expect(options).not.toContain(english);
    // Quelle (Current/Previous) und Filter bleiben bedienbar; Filter ohne englische Standardwerte.
    const filterOptions = await region(page).getByRole("combobox", { name: "Filter" }).locator("option").allTextContents();
    expect(filterOptions).not.toContain("All");
    // Statuswerte in den Zellen: nur deutsche Begriffe.
    const cells = (await gridWindow(grid)).rows.flatMap((r) => r.cells);
    expect(cells).toContain("Unverändert");
    for (const english of ["Unchanged", "Added", "Removed", "Missing"]) expect(cells).not.toContain(english);
    // Übersicht in der Seitenleiste: deutscher Titel, deutsche Spaltenköpfe, deutsche Zeilentitel, Zahlen mit Tausenderpunkt.
    const table = sidebar(page).getByRole("table").first();
    await expect(table).toBeVisible();
    expect((await table.getAttribute("aria-label")) ?? "").toMatch(/übersicht/i);
    const heads = (await table.getByRole("columnheader").allTextContents()).map((h) => h.trim());
    for (const name of ["Hinzugefügt", "Neu", "Entfernt", "Fehlend"]) expect(heads).toContain(name);
    for (const english of ["Added", "New", "Removed", "Missing"]) expect(heads).not.toContain(english);
    const titles = (await table.getByRole("rowheader").allTextContents()).map((t) => t.trim());
    expect(titles.length).toBeGreaterThan(10);
    expect(titles.filter((t) => /^(Internal|External|Response Codes|Page Titles|Meta Description)\b/.test(t))).toEqual([]);
    const html = table.getByRole("row").filter({ has: page.getByRole("rowheader", { name: /^Intern.*HTML/ }) });
    const counts = await html.getByRole("button").allTextContents();
    expect(counts[0]!.trim(), "Current mit Tausenderpunkt").toBe("1.274");
    expect(counts[1]!.trim(), "Previous mit Tausenderpunkt").toBe("1.214");
    const texts = await table.getByRole("button").allTextContents();
    expect(texts.filter((t) => /^\d{4,}$/.test(t.trim())), "keine Zahl ≥ 1000 ohne Tausenderpunkt").toEqual([]);
  });

  // spec: OUT-142
  // spec: OUT-143
  test("Entfernt und Fehlend sind in der Übersicht erreichbar (nicht abgeschnitten) und listen genau ihre URLs", async ({ page, context }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await openMain(page, context, bridge, "de");
    const grid = await startComparison(page, "de");
    const table = sidebar(page).getByRole("table").first();
    const html = table.getByRole("row").filter({ has: page.getByRole("rowheader", { name: /^Intern.*HTML/ }) });
    const container = sidebar(page);
    const c = await box(container);
    // Mit dem Mausrad nach rechts, falls die Zahlenspalten nicht in die Seitenleiste passen.
    const scroller = sidebar(page).locator(".os-compare-numbers-scroll");
    if (await scroller.evaluate((el) => el.scrollWidth > el.clientWidth + 1)) {
      await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
      for (let i = 0; i < 5; i++) await page.mouse.wheel(300, 0);
      await expect.poll(() => scroller.evaluate((el) => el.scrollLeft)).toBeGreaterThan(0);
    }
    for (const name of ["Entfernt", "Fehlend"]) {
      const head = await box(table.getByRole("columnheader", { name, exact: true }));
      expect(head.x, `${name} links im sichtbaren Bereich`).toBeGreaterThanOrEqual(c.x - 1);
      expect(head.x + head.width, `${name} rechts im sichtbaren Bereich`).toBeLessThanOrEqual(c.x + c.width + 1);
    }
    // Klick auf die Zahl bei Fehlend listet genau diese URLs (OUT-142: Zeilenzahl = angezeigte Zahl).
    const buttons = html.getByRole("button");
    const missing = buttons.nth(5);
    expect(num((await missing.textContent())!)).toBe(MISSING);
    await missing.click();
    await expect(region(page).getByRole("status").filter({ hasText: /Zeilen/ })).toHaveText(new RegExp(`^\\s*${MISSING}\\s+Zeilen`));
    await expect(grid).toHaveAttribute("aria-rowcount", String(MISSING + 1));
    const cells = (await gridWindow(grid)).rows.flatMap((r) => r.cells);
    const statusRows = (await gridWindow(grid)).rows;
    expect(statusRows.length).toBeGreaterThan(0);
    for (const r of statusRows) expect(r.cells, `Zeile ${r.index}`).toContain("Fehlend");
    expect(cells).not.toContain("Neu");
    await buttons.nth(3).click();
    await expect(grid).toHaveAttribute("aria-rowcount", String(NEW + 1));
    expect((await gridWindow(grid)).rows.flatMap((r) => r.cells).filter((t) => t === "Neu").length).toBeGreaterThan(0);
    // Entfernt: Zahl aus der Übersicht = Zeilen der Tabelle.
    const removed = num((await buttons.nth(4).textContent())!);
    await buttons.nth(4).click();
    await expect(grid).toHaveAttribute("aria-rowcount", String(removed + 1));
  });

  // spec: UI-013
  // spec: OUT-140
  // spec: OUT-141
  test("Modusanzeige „Vergleich“ in Statusleiste und Kopf; Tausch vertauscht die Klassen; Schließen bringt den normalen Modus zurück", async ({ page, context }) => {
    await openMain(page, context, bridge, "de");
    const header = page.getByRole("banner");
    const status = page.getByRole("contentinfo");
    await expect(status).toContainText("Modus: Spider");
    const grid = await startComparison(page, "de");
    await expect(status).toContainText("Modus: Vergleich");
    await expect(status).not.toContainText("Modus: Spider");
    await expect.poll(() => selectedText(header.getByRole("combobox").first())).toBe("Vergleich");
    // OUT-141: Tausch – neues Hinzugefügt = altes Entfernt, neues Neu = altes Fehlend (und umgekehrt), Current/Previous getauscht.
    const row = () => sidebar(page).getByRole("table").first().getByRole("row").filter({ has: page.getByRole("rowheader", { name: /^Intern.*HTML/ }) }).getByRole("button");
    const read = async () => (await row().allTextContents()).map(num);
    const [curA, prevA, added, neu, removed, missing] = await read();
    expect([curA, prevA, neu, missing]).toEqual([CURRENT_HTML, PREVIOUS_HTML, NEW, MISSING]);
    await page.getByRole("button", { name: L.de.swap, exact: true }).click();
    await expect.poll(async () => (await read())[0], { timeout: 30_000 }).toBe(PREVIOUS_HTML);
    const [curB, prevB, addedB, neuB, removedB, missingB] = await read();
    expect([curB, prevB]).toEqual([prevA, curA]);
    expect([addedB, neuB, removedB, missingB]).toEqual([removed, missing, added, neu]);
    await expect(status).toContainText("Modus: Vergleich");
    await expect(grid).toBeVisible();
    // Schließen: Vergleich weg, normaler Modus zurück.
    await page.getByRole("button", { name: L.de.close, exact: true }).click();
    await expect(page.getByRole("grid", { name: L.de.grid, exact: true })).toHaveCount(0);
    await expect(sidebar(page).getByRole("table", { name: /Übersicht/ })).toHaveCount(0);
    await expect(status).toContainText("Modus: Spider");
    await expect(status).not.toContainText("Modus: Vergleich");
    await expect.poll(() => selectedText(header.getByRole("combobox").first())).toBe("Spider");
    await expect(page.getByRole("button", { name: L.de.compareOpen, exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: L.de.swap, exact: true })).toHaveCount(0);
  });

  // spec: OUT-151
  // spec: OUT-142
  test("CSV-Export der Ansicht: kanonische englische Spaltennamen und Statuswerte, auch in der deutschen Oberfläche", async ({ page, context }) => {
    await openMain(page, context, bridge, "de");
    const grid = await startComparison(page, "de");
    const category = region(page).getByRole("combobox", { name: "Vergleichsstatus" });
    for (const [label, canonical, expected] of [["Fehlend", "Missing", MISSING], ["Neu", "New", NEW]] as const) {
      await category.selectOption({ label });
      await expect(grid).toHaveAttribute("aria-rowcount", String(expected + 1));
      const [download] = await Promise.all([page.waitForEvent("download"), region(page).getByRole("button", { name: /Ansicht exportieren/ }).click()]);
      const csv = readFileSync((await download.path())!, "utf8");
      const rows = parseCsv(csv).filter((r) => r.length > 1);
      const head = rows[0]!;
      expect(head[0]).toBe("Address");
      const statusColumn = head.indexOf("Comparison Status");
      expect(statusColumn, `kanonische Vergleichsspalte im Export (Spalten: ${head.join("|")})`).toBeGreaterThan(0);
      expect(head).not.toContain("Vergleichsstatus");
      expect(rows.length - 1, `Zeilen des Exports (${label})`).toBe(expected);
      expect(new Set(rows.slice(1).map((r) => r[statusColumn]))).toEqual(new Set([canonical]));
      for (const german of ["Fehlend", "Neu", "Hinzugefügt", "Entfernt", "Unverändert"]) expect(csv).not.toContain(german);
      if (canonical === "Missing") {
        // OUT-151: Missing-Zeilen tragen die Adressen des Previous-Crawls (p100 … p139).
        const ids = rows.slice(1).map((r) => Number(/\/p(\d+)$/.exec(r[0]!)![1])).sort((a, b) => a - b);
        expect(ids[0]).toBe(100);
        expect(ids.at(-1)).toBe(100 + MISSING - 1);
      }
    }
  });

  // spec: OUT-142
  // spec: UI-121
  test("Englische Oberfläche: kanonische Titel und Statuswerte, Zahlen 1,274", async ({ page, context }) => {
    await openMain(page, context, bridge, "en");
    const grid = await startComparison(page, "en");
    const win = await gridWindow(grid);
    expect(win.headers.indexOf("Comparison Status")).toBe(win.headers.indexOf("Address") + 1);
    expect(win.rows.flatMap((r) => r.cells)).toContain("Unchanged");
    const table = sidebar(page).getByRole("table").first();
    const html = table.getByRole("row").filter({ has: page.getByRole("rowheader", { name: /^Internal.*HTML/ }) });
    expect((await html.getByRole("button").first().textContent())!.trim()).toBe("1,274");
    await expect(page.getByRole("contentinfo")).toContainText("Mode: Comparison");
    // zurück auf Deutsch für die folgenden Tests
    await openMain(page, context, bridge, "de");
  });
});

// ---------------------------------------------------------------------------------------------------------------------------------
test.describe("Fehlende Crawl-Datei beim Öffnen aus „Zuletzt geöffnet“", () => {
  let bridge: AlphaBridge;
  let site: { server: Server; port: number };
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(RECORDER);
  });
  test.afterEach(async () => {
    await bridge?.stop();
    await new Promise<void>((ok) => (site ? site.server.close(() => ok()) : ok()));
  });

  const recentOf = (page: Page) =>
    page.evaluate(() => {
      type Node = { id: string; action?: string; label?: string; items?: Node[] };
      const model = (window as unknown as { __osMenu?: { model: { menus: (Node & { items: Node[] })[] } } }).__osMenu?.model;
      const groups: Node[] = [];
      const walk = (nodes: Node[]) => nodes.forEach((n) => { if (n.id === "file.recent" || n.id === "crawls.recent") groups.push(n); if (n.items) walk(n.items); });
      walk(model?.menus.find((m) => m.id === "file")?.items ?? []);
      return groups.flatMap((g) => (g.items ?? []).filter((i) => i.id !== "recent.empty").map((i) => ({ group: g.id, action: i.action ?? i.id, label: i.label ?? "" }))) as { group: string; action: string; label: string }[];
    });

  /** Crawl öffnen (Datei → Öffnen), Ansicht leeren, Datei löschen, Eintrag aus „Zuletzt geöffnet“ anklicken. */
  async function scenario(page: Page, context: BrowserContext, browser: import("@playwright/test").Browser) {
    bridge = await startAlphaBridge();
    site = await startSite();
    const helper = await browser.newPage();
    let file: string;
    try {
      await helper.goto(`/e2e/compare/index.html?bridge=${encodeURIComponent(bridge.url)}&site=x`);
      await helper.waitForFunction(() => !!window.compareHarness);
      await crawlOnce(helper, `http://127.0.0.1:${site.port}/tiny`);
      const rows = await helper.evaluate(() => window.compareHarness.invoke<CrawlListing[]>("crawls.list", { show: "crawls" }));
      file = rows[0]!.crawl.file;
    } finally {
      await helper.close();
    }
    await openMain(page, context, bridge, "de");
    // Der native Dateidialog fehlt im Browser; sein Ergebnis ist hier der Pfad. Geöffnet wird mit dem Tastenkürzel.
    await page.evaluate(async (path) => {
      const module = "/src/features/crawls/desktop.ts";
      const { nativeCrawlDesktop } = await import(/* @vite-ignore */ module);
      nativeCrawlDesktop.chooseOpen = async () => [path];
    }, file);
    await page.locator("body").click({ position: { x: 5, y: 5 } });
    await page.keyboard.press(`${MOD}+o`);
    await expect(page.getByTestId("crawl-phase")).toHaveText(/Fertig|Abgeschlossen/, { timeout: 30_000 });
    await expect.poll(async () => (await recentOf(page)).length, { message: "Eintrag in „Zuletzt geöffnet“" }).toBeGreaterThan(0);
    await page.getByRole("button", { name: "Löschen", exact: true }).click();
    await expect(page.getByTestId("crawl-phase")).not.toHaveText(/Fertig|Abgeschlossen/);
    // Windows: Der Kern gibt die Datei nach dem Schließen des Crawls nicht im selben Augenblick frei (EBUSY), daher mit Wiederholung.
    for (const suffix of ["", "-wal", "-shm", "-journal"]) rmSync(file + suffix, { force: true, maxRetries: 20, retryDelay: 100 });
    // Die native Menüleiste gibt es im Browser nicht; die Aktion ist dieselbe wie beim Klick auf den Menüeintrag.
    const entries = await recentOf(page);
    const entry = entries[0]!;
    await page.evaluate(async (action) => {
      const path = "/src/ui/actions.ts";
      const { runAction } = await import(/* @vite-ignore */ path);
      runAction(action, { source: "menu" });
    }, entry.action);
    return { file, basename: file.split(/[\\/]/).pop()!, entry, entries };
  }

  // spec: CRAWL-127
  // spec: UI-013
  test("deutsche Meldung ohne „.:“ in der Statusleiste, kein roher Fehlerdialog, Hinweis nennt die Datei", async ({ page, context, browser }) => {
    test.setTimeout(180_000);
    const { basename, entry } = await scenario(page, context, browser);
    const statusAlert = page.getByRole("contentinfo").getByRole("alert");
    await expect(statusAlert).toBeVisible({ timeout: 15_000 });
    const sentence = await statusAlert.evaluate((el) => {
      const copy = el.cloneNode(true) as HTMLElement;
      copy.querySelectorAll("details").forEach((d) => d.remove());
      return (copy.textContent ?? "").trim();
    });
    expect(sentence).toContain(basename);
    expect(sentence).not.toMatch(/\.:/);
    expect(sentence).not.toMatch(/fehlgeschlagen/i);
    expect(sentence).not.toMatch(/NotFound|not found|os error|No such file|Io\b|crawl-open-failed|[{}]/);
    expect(sentence).toMatch(/Datei/);
    // Kein roher Fehlerdialog: Dialoge enthalten nur deutschen Text ohne Codes.
    await page.waitForTimeout(800);
    for (const dlg of await page.getByRole("dialog").all()) {
      const text = (await dlg.innerText()).replace(/\s+/g, " ");
      expect(text, "Dialogtext ohne rohe Codes").not.toMatch(/NotFound|not found|os error|No such file|Io\b|crawl-open-failed|[{}]|\.:/);
    }
    // Der Eintrag verschwindet aus dem Untermenü, über das er angeklickt wurde.
    await expect
      .poll(async () => (await recentOf(page)).filter((e) => e.group === entry.group).length, { message: `Eintrag aus „${entry.group}“ entfernt` })
      .toBe(0);
    await expect(page.getByRole("contentinfo")).not.toContainText("fehlgeschlagen");
  });

  // spec: CRAWL-127
  test("der angeklickte Eintrag verschwindet aus „Zuletzt geöffnet“ vollständig (es bleibt kein zweiter Eintrag derselben Datei)", async ({ page, context, browser }) => {
    test.setTimeout(180_000);
    const { entry, entries } = await scenario(page, context, browser);
    await page.getByRole("contentinfo").getByRole("alert").waitFor({ timeout: 15_000 });
    await page.waitForTimeout(1500);
    const after = await recentOf(page);
    expect(after, `Einträge vorher: ${JSON.stringify(entries)}, angeklickt: ${entry.action}`).toEqual([]);
  });
});

// Unabhängige Abnahme B4-STARTFIX (Oberfläche, deutsch): Start-URL nicht erreichbar bzw. per robots.txt gesperrt.
// acceptance: independent
// Erwartungen stammen allein aus der Spec: UI-019 (Dialog mit Ursache; Dialog „robots.txt gesperrt“ mit sperrender robots.txt und
// Zeile, nur Hinweis auf die Einstellung, je Crawl und Start-URL höchstens einmal, auch nicht nach „URLs hinzufügen“ – OS-Festlegung
// 09.11.2026), CRAWL-128 (nicht erreichbar = „Fehlgeschlagen“; Sperre per robots.txt = kein Fehlschlag, Lauf bleibt „Abgeschlossen“),
// CRAWL-131 (URLs hinzufügen). Echte Bridge und echter Kern mit lokalen Fixtures auf Loopback. Die Start-URL wird mit der echten
// Tastatur eingegeben (Klick ins Feld, `page.keyboard.type`, Enter). Für „URLs hinzufügen“ und „Crawl öffnen“ gibt es in der
// Oberfläche keinen Dialog; sie laufen über dieselben IPC-Aufrufe wie die übrigen E2E-Tests.
import { spawn, type ChildProcess } from "node:child_process";
import { createServer } from "node:net";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";
import { ensureTools, startSite, type Site } from "../alpha/support";
import { bridgeBinary } from "../support/bridge";

/** Ein freier, danach nicht mehr belegter Port auf Loopback. */
async function closedPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((ok) => server.listen(0, "127.0.0.1", ok));
  const port = (server.address() as { port: number }).port;
  await new Promise<void>((ok) => server.close(() => ok()));
  return port;
}

/** Fixture mit mehrgruppiger robots.txt; die sperrende Zeile ist Zeile 7 (`Disallow: /secret/`). */
function robotsFixture(): string {
  const root = mkdtempSync(join(tmpdir(), "os-startfix-site-"));
  const page = (title: string, links = "") => `<!doctype html><html><head><title>${title}</title></head><body><h1>${title}</h1>${links}</body></html>`;
  writeFileSync(join(root, "scenario.toml"), "# robots fixture\n");
  writeFileSync(join(root, "robots.txt"), "# Testdatei\nUser-agent: Googlebot\nDisallow: /g\n\nUser-agent: *\nAllow: /secret/open.html\nDisallow: /secret/\n");
  writeFileSync(join(root, "index.html"), page("Start", '<a href="/open.html">open</a>'));
  writeFileSync(join(root, "open.html"), page("Open"));
  mkdirSync(join(root, "secret"));
  writeFileSync(join(root, "secret", "index.html"), page("Secret"));
  writeFileSync(join(root, "secret", "open.html"), page("Secret open"));
  return root;
}

interface Running {
  url: string;
  stop(): Promise<void>;
}

/** Startet die Bridge mit festem Home (damit ein Neustart dieselben Crawls sieht); die Oberfläche ist deutsch. */
async function startBridgeAt(home: string): Promise<Running> {
  const bin = bridgeBinary();
  if (!existsSync(bin)) throw new Error(`os-app-bridge fehlt (${bin})`);
  const child: ChildProcess = spawn(bin, ["--home", home], { stdio: ["ignore", "pipe", "inherit"] });
  const info = await new Promise<{ port: number; token: string }>((ok, fail) => {
    let buf = "";
    const timer = setTimeout(() => fail(new Error("os-app-bridge meldet keinen Port")), 20_000);
    child.stdout?.on("data", (d: Buffer) => {
      buf += d.toString();
      const nl = buf.indexOf("\n");
      if (nl >= 0) {
        clearTimeout(timer);
        ok(JSON.parse(buf.slice(0, nl)));
      }
    });
    child.on("exit", () => {
      clearTimeout(timer);
      fail(new Error("os-app-bridge beendet sich vorzeitig"));
    });
  });
  return {
    url: `ws://127.0.0.1:${info.port}/?token=${info.token}`,
    async stop() {
      child.kill();
      await new Promise((r) => setTimeout(r, 200));
    },
  };
}

function germanHome(): string {
  const home = mkdtempSync(join(tmpdir(), "os-startfix-home-"));
  writeFileSync(join(home, "settings.toml"), '[general]\nlanguage = "de"\n');
  return home;
}

async function openShell(page: Page, bridge: Running) {
  await page.goto(`/?${new URLSearchParams({ bridge: bridge.url })}`);
  await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
}

/** Einstellungen der Sitzung: robots.txt beachten (Standard) und keine Wiederholungen, damit Fehlschläge nicht auf Wartezeiten laufen. */
async function respectRobotsWithoutRetries(page: Page) {
  await page.evaluate(async () => {
    const { config } = await import(/* @vite-ignore */ String("/src/ipc/config.ts"));
    const dto = await config.get();
    const root = dto.config as Record<string, Record<string, unknown>>;
    await config.set({ ...dto, config: { ...root, robots: { ...root.robots, retries: 0, mode: "respect" } } });
  });
}

/** Start wie ein Nutzer: Klick ins Feld „Start-URL“, mit der Tastatur tippen, Enter. */
async function startByTyping(page: Page, url: string, submit: "enter" | "button" = "enter") {
  const field = page.getByLabel("Start-URL", { exact: true });
  await field.click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.press("Backspace");
  await page.keyboard.type(url, { delay: 10 });
  await expect(field).toHaveValue(url);
  if (submit === "enter") await page.keyboard.press("Enter");
  else await page.getByRole("button", { name: "Starten", exact: true }).click();
}

const phase = (page: Page) => page.getByTestId("crawl-phase");
const dialogs = (page: Page) => page.getByRole("dialog");

const crawled = (page: Page) =>
  page.evaluate(async () => {
    const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
    return (useSession.getState().crawl?.crawled ?? -1) as number;
  });

async function addUrls(page: Page, urls: string[]) {
  await page.evaluate(async (urls) => {
    const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
    await crawl.addUrls({ urls });
  }, urls);
}

test.describe("Start-URL scheitert oder ist per robots.txt gesperrt (deutsche Oberfläche)", () => {
  test.beforeAll(async () => { await ensureTools(); });

  // spec: UI-019
  // spec: CRAWL-128
  test("nicht erreichbare Start-URL: Dialog mit Ursache und Adresse, Statusleiste „Fehlgeschlagen“", async ({ page }) => {
    test.setTimeout(90_000);
    const home = germanHome();
    const bridge = await startBridgeAt(home);
    try {
      const port = await closedPort();
      await openShell(page, bridge);
      await respectRobotsWithoutRetries(page);
      await startByTyping(page, `http://127.0.0.1:${port}/`);

      // Kein stilles „0 URLs“: Der Dialog nennt Ursache (Fehlerklasse) und Adresse.
      const dialog = dialogs(page).first();
      await expect(dialog).toBeVisible({ timeout: 10_000 });
      await expect(dialog).toContainText(/Ursache:\s*\S+/);
      await expect(dialog).toContainText(/abgelehnt/i);
      await expect(dialog).toContainText(`127.0.0.1:${port}`);
      // CRAWL-128: Der Lauf ist „Fehlgeschlagen“, nicht „Fertig“.
      await expect(phase(page)).toHaveText("Fehlgeschlagen");
      await expect(phase(page)).not.toHaveText("Fertig");

      // Der Dialog lässt sich mit der Tastatur schließen und erscheint nicht von selbst wieder.
      await page.keyboard.press("Escape");
      await expect(dialogs(page)).toHaveCount(0);
      await page.waitForTimeout(1500);
      await expect(dialogs(page)).toHaveCount(0);
      await expect(phase(page)).toHaveText("Fehlgeschlagen");
    } finally {
      await bridge.stop();
      rmSync(home, { recursive: true, force: true });
    }
  });

  // spec: UI-019
  // spec: CRAWL-128
  test("per robots.txt gesperrte Start-URL: Dialog nennt robots.txt, Zeile und Regel; Lauf bleibt „Fertig“; nur ein Hinweis", async ({ page }) => {
    test.setTimeout(90_000);
    const siteDir = robotsFixture();
    const site = await startSite(siteDir);
    const home = germanHome();
    const bridge = await startBridgeAt(home);
    try {
      await openShell(page, bridge);
      await respectRobotsWithoutRetries(page);
      await startByTyping(page, `${site.base}/secret/index.html`);

      const dialog = dialogs(page).first();
      await expect(dialog).toBeVisible({ timeout: 10_000 });
      await expect(dialog).toContainText(/robots\.txt/);
      await expect(dialog).toContainText(/sperr/i);
      // Die sperrende robots.txt und die sperrende Zeile: Nummer und Regeltext.
      await expect(dialog).toContainText(`${site.base}/robots.txt`);
      await expect(dialog).toContainText(/Zeile\s*7/);
      await expect(dialog).toContainText("Disallow: /secret/");
      // Kein Fehlschlag: Statusleiste „Fertig“, der Dialog nennt nicht den Zustand „Fehlgeschlagen“.
      await expect(phase(page)).toHaveText("Fertig");
      await expect(dialog).not.toContainText(/Fehlgeschlagen/);

      // Nur ein Hinweis auf die Einstellung „robots.txt ignorieren“, keine automatische Änderung.
      await dialog.getByRole("button", { name: /robots\.txt ignorieren/i }).click();
      const ignore = page.getByRole("radio", { name: "robots.txt ignorieren", exact: true });
      await expect(ignore).toBeVisible();
      await expect(ignore).not.toBeChecked();
      await expect(page.getByRole("radio", { name: "robots.txt respektieren", exact: true })).toBeChecked();
    } finally {
      await bridge.stop();
      await site.stop();
      rmSync(home, { recursive: true, force: true });
      rmSync(siteDir, { recursive: true, force: true });
    }
  });

  // spec: UI-019
  // spec: CRAWL-131
  test("der Sperr-Dialog erscheint je Crawl höchstens einmal: nicht nach „URLs hinzufügen“, nicht nach einem Neustart", async ({ page }) => {
    test.setTimeout(120_000);
    const siteDir = robotsFixture();
    const site: Site = await startSite(siteDir);
    const home = germanHome();
    let bridge = await startBridgeAt(home);
    try {
      await openShell(page, bridge);
      await respectRobotsWithoutRetries(page);
      await startByTyping(page, `${site.base}/secret/index.html`);
      await expect(dialogs(page).first()).toBeVisible({ timeout: 10_000 });
      await expect(dialogs(page).first()).toContainText(/Zeile\s*7/);
      await expect(phase(page)).toHaveText("Fertig");
      await page.keyboard.press("Escape");
      await expect(dialogs(page)).toHaveCount(0);

      // „URLs hinzufügen“ im selben Crawl: Der Lauf setzt fort und endet erneut; die Meldung kommt nicht noch einmal.
      const before = await crawled(page);
      await addUrls(page, [`${site.base}/open.html`]);
      await expect.poll(() => crawled(page), { timeout: 20_000 }).toBeGreaterThan(before);
      await expect(phase(page)).toHaveText("Fertig");
      await page.waitForTimeout(2000);
      await expect(dialogs(page)).toHaveCount(0);

      // Neustart der Anwendung: derselbe Datenordner, derselbe Crawl wird geöffnet, danach wieder „URLs hinzufügen“.
      await bridge.stop();
      const crawlsDir = join(home, "crawls");
      const files = existsSync(crawlsDir) ? readdirSync(crawlsDir).filter((f) => f.endsWith(".ospider")) : [];
      expect(files).toHaveLength(1);
      bridge = await startBridgeAt(home);
      await openShell(page, bridge);
      await respectRobotsWithoutRetries(page);
      await page.evaluate(async (path) => {
        const { crawl } = await import(/* @vite-ignore */ String("/src/ipc/crawl.ts"));
        await crawl.open({ kind: "path", path });
      }, join(crawlsDir, files[0]!));
      await expect.poll(() => crawled(page), { timeout: 20_000 }).toBeGreaterThan(0);
      const reopened = await crawled(page);
      await addUrls(page, [`${site.base}/secret/open.html`]);
      await expect.poll(() => crawled(page), { timeout: 20_000 }).toBeGreaterThan(reopened);
      await expect(phase(page)).toHaveText("Fertig");
      await page.waitForTimeout(2000);
      await expect(dialogs(page)).toHaveCount(0);
    } finally {
      await bridge.stop();
      await site.stop();
      rmSync(home, { recursive: true, force: true });
      rmSync(siteDir, { recursive: true, force: true });
    }
  });

  // spec: UI-019
  // spec: CRAWL-128
  test("ein neuer Crawl mit derselben gesperrten Start-URL zeigt den Dialog wieder (einmal)", async ({ page }) => {
    test.setTimeout(90_000);
    const siteDir = robotsFixture();
    const site = await startSite(siteDir);
    const home = germanHome();
    const bridge = await startBridgeAt(home);
    try {
      await openShell(page, bridge);
      await respectRobotsWithoutRetries(page);
      for (const round of [1, 2]) {
        await startByTyping(page, `${site.base}/secret/index.html`, round > 1 ? "button" : "enter");
        // Ein zweiter Crawl ersetzt die bisherige Ansicht; die Rückfrage wird bestätigt.
        if (round > 1) await page.getByRole("button", { name: "Ersetzen", exact: true }).click();
        await expect(dialogs(page).first(), `Crawl ${round}`).toBeVisible({ timeout: 10_000 });
        await expect(dialogs(page).first()).toContainText(/Zeile\s*7/);
        await expect(phase(page)).toHaveText("Fertig");
        await page.keyboard.press("Escape");
        await expect(dialogs(page)).toHaveCount(0);
        await page.waitForTimeout(1500);
        await expect(dialogs(page)).toHaveCount(0);
      }
    } finally {
      await bridge.stop();
      await site.stop();
      rmSync(home, { recursive: true, force: true });
      rmSync(siteDir, { recursive: true, force: true });
    }
  });
});

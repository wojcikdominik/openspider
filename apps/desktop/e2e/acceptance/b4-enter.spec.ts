// Unabhängige Abnahme B4-ENTER (Kopfzeile: Enter im Start-URL-Feld, Mod+Enter, Rückfrage „Ersetzen“, Vorschlagsliste).
// acceptance: independent
// Erwartungen stammen allein aus der Spec: UI-003 (Enter im URL-Feld entspricht „Start“; Vorschlagsliste der letzten
// Start-URLs, neueste zuerst; Zustandsautomat der Schaltflächen; Start nach Ergebnissen = neu crawlen nach Rückfrage) und
// UI-113 (Mod+Enter, im URL-Feld auch Enter; Kontextregeln für Texteingabefelder und modale Dialoge; OS-Festlegung
// vom 09.11.2026: pausiert/unterbrochen + unveränderte Adresse = fortsetzen ohne Rückfrage und ohne neuen Crawl, geänderte
// Adresse = normaler Startweg mit Rückfrage). Der Produktcode wird nur über die Oberfläche bedient.
//
// Eingaben sind echte Tastatur-Ereignisse (`page.keyboard`): erst Klick ins Feld, dann Ctrl/Cmd+A, Backspace, Tippen. Kein
// `fill`, kein `requestSubmit`. Einzige Ausnahme sind die synthetischen IME-Ereignisse (Playwright kann `isComposing`
// außerhalb von Chromium nicht erzeugen); dort steht das im Testnamen, und die Ereignisfelder werden zurückgelesen, damit
// der Test nicht leer läuft. Crawls sind echt (Bridge `os-app-bridge`, Loopback-Server dieser Datei), kein Mock.
import { createServer, type Server } from "node:http";
import type { Page } from "@playwright/test";
import { test, expect } from "../fixtures/app";

test.use({ freshState: true });
test.beforeEach(({ app }) => {
  expect(app.mode, "echte Crawls brauchen die Bridge (OS_E2E_TRANSPORT=bridge)").toBe("bridge");
});

const DIALOG = "Replace the current view?";

interface Site {
  base: string;
  /** Pfade aller Abrufe in der Reihenfolge des Eingangs. */
  hits: string[];
  count(path: string): number;
  stop(): Promise<void>;
}

/**
 * `/fast/<name>`: zwei Seiten, der Crawl ist in Millisekunden fertig.
 * `/slow/<name>`: nie endender Crawl (200 ms je Seite, Frontier bleibt bei etwa 100 URLs), damit „läuft“ und „pausiert“
 * stabil erreichbar sind (gleiches Muster wie e2e/header/header.spec.ts).
 */
async function startSite(): Promise<Site> {
  const hits: string[] = [];
  const server: Server = createServer((req, res) => {
    const path = (req.url ?? "/").split("?")[0]!;
    hits.push(path);
    const slow = /^\/slow\/([a-z0-9]+)(?:\/p\/(\d+))?$/.exec(path);
    const fast = /^\/fast\/([a-z0-9]+)(\/child)?$/.exec(path);
    const send = (body: string) => { res.writeHead(200, { "Content-Type": "text/html" }); res.end(body); };
    if (slow) {
      const [, name, n] = slow;
      setTimeout(() => send(n === undefined
        ? Array.from({ length: 100 }, (_, i) => `<a href="/slow/${name}/p/${i}">${i}</a>`).join("")
        : `<title>Slow ${name} ${n}</title><a href="/slow/${name}/p/${Number(n) + 100}">next</a>`), 200);
    } else if (fast) {
      send(fast[2] ? `<title>Fast ${fast[1]} child</title>` : `<title>Fast ${fast[1]}</title><a href="/fast/${fast[1]}/child">child</a>`);
    } else { res.writeHead(404); res.end("not found"); }
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Fixture address missing");
  return {
    base: `http://127.0.0.1:${address.port}`,
    hits,
    count: (path) => hits.filter((h) => h === path).length,
    stop: () => new Promise<void>((resolve, reject) => { server.closeAllConnections(); server.close((e) => e ? reject(e) : resolve()); }),
  };
}

const header = (page: Page) => page.getByTestId("crawl-header");
const phase = (page: Page) => page.getByTestId("crawl-phase");
const dialog = (page: Page) => page.getByRole("dialog", { name: DIALOG, exact: true });
const urlField = (page: Page) => page.getByLabel("Start URL");

/** Mod-Taste, wie die App die Plattform erkennt (Playwright-WebKit unter Linux meldet „Macintosh“, dann gilt Cmd). */
const appMod = (page: Page) => page.evaluate(() => /mac|iphone|ipad/i.test((navigator as { userAgentData?: { platform?: string } }).userAgentData?.platform || navigator.platform || navigator.userAgent) ? "Meta" : "Control");

async function open(app: { open(page: Page, t: "main"): Promise<void> }, page: Page) {
  await app.open(page, "main");
  await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
}

/** Klick ins Feld (wie ein Nutzer), Inhalt ersetzen, mit echter Tastatur tippen. */
async function typeUrl(page: Page, text: string) {
  const field = urlField(page);
  await field.click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.press("Backspace");
  await expect(field).toHaveValue("");
  await page.keyboard.type(text);
  await expect(field).toHaveValue(text);
  await expect(field).toBeFocused();
  return field;
}

/** Zustand der Sitzung und Anzahl der gesicherten Crawls (Kernseite, über die Bridge). */
async function snapshot(page: Page) {
  return page.evaluate(async () => {
    const { useSession } = await import(/* @vite-ignore */ String("/src/state/session.ts"));
    const { crawls } = await import(/* @vite-ignore */ String("/src/ipc/crawls.ts"));
    const c = useSession.getState().crawl;
    const list = await crawls.list({ show: "all" });
    return { crawlId: (c?.crawlId ?? null) as string | null, phase: (c?.phase ?? null) as string | null, startUrl: (c?.startUrl ?? null) as string | null, saved: list.length as number };
  });
}
const pathOf = (url: string | null) => url ? new URL(url).pathname : null;

async function suggestions(page: Page): Promise<string[]> {
  return page.evaluate(() => [...document.querySelectorAll("datalist option")].map((o) => o.getAttribute("value") ?? ""));
}

/** Fertiger Crawl von `/fast/<name>` über Enter (ohne vorherige Ergebnisse keine Rückfrage). */
async function crawlFastByEnter(page: Page, site: Site, name: string, confirm = false) {
  await typeUrl(page, `${site.base}/fast/${name}`);
  await page.keyboard.press("Enter");
  if (confirm) {
    await expect(dialog(page)).toBeVisible();
    await dialog(page).getByRole("button", { name: "Replace", exact: true }).click();
    await expect(dialog(page)).toHaveCount(0);
  }
  await expect.poll(async () => pathOf((await snapshot(page)).startUrl)).toBe(`/fast/${name}`);
  await expect(phase(page)).toHaveText("Completed", { timeout: 30_000 });
}

/** Startet `/slow/<name>` über Enter und wartet, bis die ersten Seiten gecrawlt sind. */
async function startSlow(page: Page, site: Site, name: string) {
  await typeUrl(page, `${site.base}/slow/${name}`);
  await page.keyboard.press("Enter");
  await expect(phase(page)).toHaveText("Running");
  await expect.poll(() => site.hits.filter((h) => h.startsWith(`/slow/${name}/p/`)).length, { timeout: 15_000 }).toBeGreaterThanOrEqual(2);
}

async function pause(page: Page) {
  await header(page).getByRole("button", { name: "Pause", exact: true }).click();
  await expect(phase(page)).toHaveText("Paused", { timeout: 20_000 });
}

/**
 * Does this engine (in this run mode) let the keyboard operate the native suggestion picker of a <datalist>? Playwright's
 * Chromium never does (the picker is a separate native widget), headless WebKit neither; WebKit with a display does.
 * Probed on a throw-away page, independent of the product.
 */
async function engineAdoptsSuggestions(page: Page): Promise<boolean> {
  const probe = await page.context().newPage();
  try {
    await probe.setContent('<input id="i" list="d"><datalist id="d"><option value="aaa"><option value="bbb"></datalist>');
    await probe.click("#i");
    await probe.keyboard.press("ArrowDown");
    await probe.keyboard.press("ArrowDown");
    await probe.keyboard.press("Enter");
    return (await probe.inputValue("#i")) !== "";
  } finally { await probe.close(); }
}

/** Wartet, bis eine ausgebliebene Reaktion wirklich ausgeblieben ist (negative Prüfung braucht Zeit). */
const settle = (page: Page) => page.waitForTimeout(700);

test.describe("B4-ENTER Kopfzeile", () => {
  // spec: UI-003
  // spec: UI-113
  test("Enter starts the first, second and third crawl in a row; results ask before replacing and the suggestions list grows", async ({ app, page }) => {
    test.setTimeout(90_000);
    const site = await startSite();
    try {
      await open(app, page);
      expect(await suggestions(page)).toEqual([]);
      // 1st start: no results yet, no question.
      await typeUrl(page, `${site.base}/fast/s1`);
      await page.keyboard.press("Enter");
      await expect.poll(async () => pathOf((await snapshot(page)).startUrl)).toBe("/fast/s1");
      await expect(dialog(page)).toHaveCount(0);
      await expect(phase(page)).toHaveText("Completed", { timeout: 30_000 });
      await expect.poll(() => suggestions(page)).toEqual([`${site.base}/fast/s1`]);
      // 2nd start: the suggestion list already holds an entry (WebKit user finding), results exist -> question first.
      await typeUrl(page, `${site.base}/fast/s2`);
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toBeVisible();
      expect(site.count("/fast/s2"), "nothing is fetched before the question is answered").toBe(0);
      expect(pathOf((await snapshot(page)).startUrl)).toBe("/fast/s1");
      await dialog(page).getByRole("button", { name: "Replace", exact: true }).click();
      await expect(dialog(page)).toHaveCount(0);
      await expect.poll(async () => pathOf((await snapshot(page)).startUrl)).toBe("/fast/s2");
      await expect(phase(page)).toHaveText("Completed", { timeout: 30_000 });
      await expect.poll(() => suggestions(page)).toEqual([`${site.base}/fast/s2`, `${site.base}/fast/s1`]);
      // 3rd start.
      await typeUrl(page, `${site.base}/fast/s3`);
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toBeVisible();
      expect(site.count("/fast/s3")).toBe(0);
      await dialog(page).getByRole("button", { name: "Replace", exact: true }).click();
      await expect.poll(async () => pathOf((await snapshot(page)).startUrl)).toBe("/fast/s3");
      await expect(phase(page)).toHaveText("Completed", { timeout: 30_000 });
      // UI-003: newest first, no duplicates.
      await expect.poll(() => suggestions(page)).toEqual([`${site.base}/fast/s3`, `${site.base}/fast/s2`, `${site.base}/fast/s1`]);
      // One Enter = exactly one crawl (no double start through an additional implicit form submission).
      await settle(page);
      for (const name of ["s1", "s2", "s3"]) expect(site.count(`/fast/${name}`), name).toBe(1);
      expect((await snapshot(page)).saved).toBe(3);
      await expect(dialog(page)).toHaveCount(0);
    } finally { await site.stop(); }
  });

  // spec: UI-003
  // spec: UI-113
  test("Enter starts again after an earlier start with the same URL kept in the suggestions (no duplicate entry)", async ({ app, page }) => {
    test.setTimeout(60_000);
    const site = await startSite();
    try {
      await open(app, page);
      await crawlFastByEnter(page, site, "s1");
      await crawlFastByEnter(page, site, "s2", true);
      await crawlFastByEnter(page, site, "s1", true);
      await expect.poll(() => suggestions(page)).toEqual([`${site.base}/fast/s1`, `${site.base}/fast/s2`]);
      expect(site.count("/fast/s1")).toBe(2);
      expect((await snapshot(page)).saved).toBe(3);
    } finally { await site.stop(); }
  });

  // spec: UI-113
  test("Enter in the Replace question is the default action (modal dialogs: Esc closes, Enter confirms)", async ({ app, page }) => {
    test.setTimeout(60_000);
    const site = await startSite();
    try {
      await open(app, page);
      await crawlFastByEnter(page, site, "s1");
      await typeUrl(page, `${site.base}/fast/s2`);
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toBeVisible();
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toHaveCount(0);
      await expect.poll(async () => pathOf((await snapshot(page)).startUrl)).toBe("/fast/s2");
      await expect(phase(page)).toHaveText("Completed", { timeout: 30_000 });
      expect(site.count("/fast/s2")).toBe(1);
    } finally { await site.stop(); }
  });

  // spec: UI-003
  // spec: UI-113
  test("Escape in the Replace question starts nothing, keeps the results and the typed address, and the question can be asked again", async ({ app, page }) => {
    test.setTimeout(60_000);
    const site = await startSite();
    try {
      await open(app, page);
      await crawlFastByEnter(page, site, "s1");
      const before = await snapshot(page);
      const field = await typeUrl(page, `${site.base}/fast/s2`);
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(dialog(page)).toHaveCount(0);
      await settle(page);
      expect(site.count("/fast/s2")).toBe(0);
      expect(await snapshot(page)).toEqual(before);
      await expect(phase(page)).toHaveText("Completed");
      await expect(field).toHaveValue(`${site.base}/fast/s2`);
      // The cancelled start leaves no stuck state: the next Enter asks again; the Cancel button also starts nothing.
      await field.click();
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toBeVisible();
      await dialog(page).getByRole("button", { name: "Cancel", exact: true }).click();
      await expect(dialog(page)).toHaveCount(0);
      await settle(page);
      expect(site.count("/fast/s2")).toBe(0);
      await field.click();
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toBeVisible();
      await dialog(page).getByRole("button", { name: "Replace", exact: true }).click();
      await expect.poll(async () => pathOf((await snapshot(page)).startUrl)).toBe("/fast/s2");
      expect(site.count("/fast/s2")).toBe(1);
    } finally { await site.stop(); }
  });

  // spec: UI-003
  test("Enter while a crawl is running does not start another crawl", async ({ app, page }) => {
    test.setTimeout(60_000);
    const site = await startSite();
    try {
      await open(app, page);
      await startSlow(page, site, "a");
      const before = await snapshot(page);
      expect(before.phase).toBe("running");
      await typeUrl(page, `${site.base}/slow/b`);
      await page.keyboard.press("Enter");
      await settle(page);
      await expect(dialog(page)).toHaveCount(0);
      expect(site.hits.some((h) => h.startsWith("/slow/b"))).toBe(false);
      const after = await snapshot(page);
      expect(after.crawlId).toBe(before.crawlId);
      expect(after.saved).toBe(1);
      await expect(phase(page)).toHaveText("Running");
    } finally { await site.stop(); }
  });

  // spec: UI-113
  // spec: UI-003
  test("paused crawl with unchanged URL: Enter in the field resumes the same crawl without a question and without a new crawl", async ({ app, page }) => {
    test.setTimeout(120_000);
    const site = await startSite();
    try {
      await open(app, page);
      await startSlow(page, site, "a");
      await pause(page);
      const paused = await snapshot(page);
      expect(paused.saved).toBe(1);
      const field = urlField(page);
      await field.click();
      await expect(field).toHaveValue(`${site.base}/slow/a`);
      await page.keyboard.press("Enter");
      await expect(phase(page)).toHaveText("Running", { timeout: 20_000 });
      await expect(dialog(page)).toHaveCount(0);
      const resumed = await snapshot(page);
      expect(resumed.crawlId).toBe(paused.crawlId);
      expect(resumed.saved).toBe(1);
      expect(site.count("/slow/a"), "the start URL is not fetched a second time").toBe(1);
      // An address that merely normalizes to the same one counts as unchanged.
      await pause(page);
      await typeUrl(page, `${site.base.replace("http://", "HTTP://")}/slow/a`);
      await page.keyboard.press("Enter");
      await expect(phase(page)).toHaveText("Running", { timeout: 20_000 });
      await expect(dialog(page)).toHaveCount(0);
      expect((await snapshot(page)).crawlId).toBe(paused.crawlId);
    } finally { await site.stop(); }
  });

  // spec: UI-113
  test("paused crawl: Mod+Enter outside a text field resumes the same crawl without a question", async ({ app, page }) => {
    test.setTimeout(120_000);
    const site = await startSite();
    try {
      await open(app, page);
      await startSlow(page, site, "a");
      await pause(page);
      const paused = await snapshot(page);
      // Leave the field: a click on the (non-interactive) logo puts the focus on the page body.
      await page.getByRole("heading", { name: "OpenSpider" }).click();
      expect(await page.evaluate(() => document.activeElement?.tagName)).not.toBe("INPUT");
      await page.keyboard.press(`${await appMod(page)}+Enter`);
      await expect(phase(page)).toHaveText("Running", { timeout: 20_000 });
      await expect(dialog(page)).toHaveCount(0);
      const resumed = await snapshot(page);
      expect(resumed.crawlId).toBe(paused.crawlId);
      expect(resumed.saved).toBe(1);
      expect(site.count("/slow/a")).toBe(1);
    } finally { await site.stop(); }
  });

  // spec: UI-113
  test("paused crawl: an empty field counts as unchanged, Enter resumes", async ({ app, page }) => {
    test.setTimeout(120_000);
    const site = await startSite();
    try {
      await open(app, page);
      await startSlow(page, site, "a");
      await pause(page);
      const paused = await snapshot(page);
      const field = urlField(page);
      await field.click();
      await page.keyboard.press("ControlOrMeta+a");
      await page.keyboard.press("Backspace");
      await expect(field).toHaveValue("");
      await page.keyboard.press("Enter");
      await expect(phase(page)).toHaveText("Running", { timeout: 20_000 });
      await expect(dialog(page)).toHaveCount(0);
      const resumed = await snapshot(page);
      expect(resumed.crawlId).toBe(paused.crawlId);
      expect(resumed.saved).toBe(1);
    } finally { await site.stop(); }
  });

  // spec: UI-113
  // spec: UI-003
  test("paused crawl with a changed URL: Enter asks first, Escape keeps the pause, confirming starts a new crawl", async ({ app, page }) => {
    test.setTimeout(120_000);
    const site = await startSite();
    try {
      await open(app, page);
      await startSlow(page, site, "a");
      await pause(page);
      const paused = await snapshot(page);
      const field = await typeUrl(page, `${site.base}/slow/b`);
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toBeVisible();
      // Not resumed behind the question.
      await settle(page);
      expect(await snapshot(page)).toMatchObject({ crawlId: paused.crawlId, phase: "paused" });
      expect(site.hits.some((h) => h.startsWith("/slow/b"))).toBe(false);
      await page.keyboard.press("Escape");
      await expect(dialog(page)).toHaveCount(0);
      await settle(page);
      expect(await snapshot(page)).toMatchObject({ crawlId: paused.crawlId, phase: "paused", saved: 1 });
      await expect(phase(page)).toHaveText("Paused");
      await expect(field).toHaveValue(`${site.base}/slow/b`);
      // Mod+Enter outside the field with the changed address does not resume the old crawl silently either.
      await page.getByRole("heading", { name: "OpenSpider" }).click();
      await page.keyboard.press(`${await appMod(page)}+Enter`);
      await expect(dialog(page)).toBeVisible();
      await settle(page);
      expect(await snapshot(page)).toMatchObject({ crawlId: paused.crawlId, phase: "paused" });
      await page.keyboard.press("Escape");
      await expect(dialog(page)).toHaveCount(0);
      // Confirm: a new crawl for the new address.
      await field.click();
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toBeVisible();
      await dialog(page).getByRole("button", { name: "Replace", exact: true }).click();
      await expect(dialog(page)).toHaveCount(0);
      await expect.poll(async () => pathOf((await snapshot(page)).startUrl), { timeout: 20_000 }).toBe("/slow/b");
      const fresh = await snapshot(page);
      expect(fresh.crawlId).not.toBe(paused.crawlId);
      expect(fresh.saved).toBe(2);
      expect(site.count("/slow/b")).toBe(1);
      await expect(phase(page)).toHaveText("Running");
    } finally { await site.stop(); }
  });

  // spec: UI-003
  // spec: UI-113
  test("arrow keys in the suggestions and then Enter adopt the entry; Enter does not start with that keystroke", async ({ app, page }) => {
    test.setTimeout(90_000);
    const adopts = await engineAdoptsSuggestions(page);
    const site = await startSite();
    try {
      await open(app, page);
      await crawlFastByEnter(page, site, "s1");
      await crawlFastByEnter(page, site, "s2", true);
      await expect.poll(() => suggestions(page)).toEqual([`${site.base}/fast/s2`, `${site.base}/fast/s1`]);
      const known = [`${site.base}/fast/s2`, `${site.base}/fast/s1`];
      const field = urlField(page);
      const clear = async () => {
        await field.click();
        await page.keyboard.press("ControlOrMeta+a");
        await page.keyboard.press("Backspace");
        await expect(field).toHaveValue("");
      };
      for (const arrow of ["ArrowDown", "ArrowUp"]) {
        await clear();
        await page.keyboard.press(arrow);
        await page.keyboard.press(arrow);
        await page.keyboard.press("Enter");
        await settle(page);
        // This Enter belongs to the suggestion list: no question, no start.
        await expect(dialog(page)).toHaveCount(0);
        expect(site.count("/fast/s1"), `${arrow}: no start with the adopting Enter`).toBe(1);
        expect(site.count("/fast/s2")).toBe(1);
        expect((await snapshot(page)).saved).toBe(2);
        if (!adopts) {
          // Playwright's engine has no operable suggestion picker here (see engineAdoptsSuggestions): adoption is not testable.
          test.info().annotations.push({ type: "engine-limit", description: `${arrow}: no operable <datalist> picker in this engine/mode; only "no start" checked` });
          continue;
        }
        // The entry is adopted: the field holds one of the known addresses.
        expect(known, `${arrow} then Enter adopts a suggestion`).toContain(await field.inputValue());
        // The next Enter is the start: results exist, so the question appears.
        await page.keyboard.press("Enter");
        await expect(dialog(page)).toBeVisible();
        await page.keyboard.press("Escape");
        await expect(dialog(page)).toHaveCount(0);
      }
    } finally { await site.stop(); }
  });

  // spec: UI-003
  // spec: UI-113
  test("Escape in the URL field starts nothing and changes nothing", async ({ app, page }) => {
    test.setTimeout(60_000);
    const site = await startSite();
    try {
      await open(app, page);
      await crawlFastByEnter(page, site, "s1");
      const before = await snapshot(page);
      const field = await typeUrl(page, `${site.base}/fast/s2`);
      await page.keyboard.press("Escape");
      await settle(page);
      await expect(dialog(page)).toHaveCount(0);
      await expect(field).toHaveValue(`${site.base}/fast/s2`);
      await expect(field).toBeFocused();
      expect(await snapshot(page)).toEqual(before);
      expect(site.count("/fast/s2")).toBe(0);
      // The field still works: the next Enter is the start.
      await page.keyboard.press("Enter");
      await expect(dialog(page)).toBeVisible();
    } finally { await site.stop(); }
  });

  // spec: UI-003
  // spec: UI-113
  test("after arrow keys and Escape (suggestions dismissed) the next Enter starts the crawl", async ({ app, page }) => {
    test.setTimeout(60_000);
    const site = await startSite();
    try {
      await open(app, page);
      await crawlFastByEnter(page, site, "s1");
      const field = await typeUrl(page, `${site.base}/fast/s2`);
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("Escape");
      await expect(field).toHaveValue(`${site.base}/fast/s2`);
      await page.keyboard.press("Enter");
      await expect(dialog(page), "one Enter after dismissing the list is the start").toBeVisible();
      await dialog(page).getByRole("button", { name: "Replace", exact: true }).click();
      await expect.poll(async () => pathOf((await snapshot(page)).startUrl)).toBe("/fast/s2");
    } finally { await site.stop(); }
  });

  // spec: UI-113
  test("no start during an IME composition (synthetic composition events): isComposing and keyCode 229, then a real Enter starts", async ({ app, page }) => {
    test.setTimeout(60_000);
    const site = await startSite();
    try {
      await open(app, page);
      const field = await typeUrl(page, `${site.base}/fast/s1`);
      const probe = await field.evaluate((el) => {
        const fire = (type: string, init: KeyboardEventInit) => {
          const ev = new KeyboardEvent(type, { bubbles: true, cancelable: true, composed: true, ...init });
          el.dispatchEvent(ev);
          return { composing: ev.isComposing, keyCode: ev.keyCode, prevented: ev.defaultPrevented };
        };
        el.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true, data: "" }));
        const during = fire("keydown", { key: "Enter", code: "Enter", keyCode: 13, isComposing: true });
        el.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true, data: "x" }));
        // Safari reports the Enter that commits the composition after compositionend with keyCode 229.
        const after = fire("keydown", { key: "Enter", code: "Enter", keyCode: 229, isComposing: false });
        return { during, after };
      });
      // The events really carried the IME markers (otherwise this test would pass vacuously).
      expect(probe.during.composing).toBe(true);
      expect(probe.after.keyCode).toBe(229);
      expect(probe.during.prevented).toBe(false);
      expect(probe.after.prevented).toBe(false);
      await settle(page);
      expect(site.hits).toEqual([]);
      expect((await snapshot(page)).crawlId).toBeNull();
      // The same field starts with the next real Enter.
      await page.keyboard.press("Enter");
      await expect.poll(async () => pathOf((await snapshot(page)).startUrl)).toBe("/fast/s1");
    } finally { await site.stop(); }
  });

  // spec: UI-113
  test("no start while a real IME composition is open (CDP imeSetComposition, Chromium); committing and Enter starts", async ({ app, page, browserName }) => {
    test.skip(browserName !== "chromium", "CDP-Eingabemethode gibt es nur in Chromium; die Engine-übergreifende Prüfung steht im Test davor");
    test.setTimeout(60_000);
    const site = await startSite();
    try {
      await open(app, page);
      const field = urlField(page);
      await field.click();
      await page.evaluate(() => {
        const w = window as unknown as { __keys: { key: string; composing: boolean; keyCode: number; prevented: boolean }[] };
        w.__keys = [];
        document.addEventListener("keydown", (e) => { const t = e.target; if (t instanceof HTMLInputElement) setTimeout(() => w.__keys.push({ key: e.key, composing: e.isComposing, keyCode: e.keyCode, prevented: e.defaultPrevented })); }, true);
      });
      const cdp = await page.context().newCDPSession(page);
      const text = `${site.base}/fast/s1`;
      await cdp.send("Input.imeSetComposition", { text, selectionStart: text.length, selectionEnd: text.length });
      await expect(field).toHaveValue(text);
      // The key an IME hands to the page: keydown "Process"/229 while composing, no keypress (so no implicit form submission).
      await cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: "Process", code: "Enter", windowsVirtualKeyCode: 229, nativeVirtualKeyCode: 229 });
      await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13 });
      await settle(page);
      const keys = await page.evaluate(() => (window as unknown as { __keys: { composing: boolean; keyCode: number }[] }).__keys);
      // Valid set-up: the browser marked the keystroke as part of the composition (otherwise this test would pass vacuously).
      expect(keys.some((k) => k.composing && k.keyCode === 229), JSON.stringify(keys)).toBe(true);
      expect(site.hits, `keydown log: ${JSON.stringify(keys)}`).toEqual([]);
      expect((await snapshot(page)).crawlId).toBeNull();
      await cdp.send("Input.insertText", { text });
      await expect(field).toHaveValue(text);
      await page.keyboard.press("Enter");
      await expect.poll(async () => pathOf((await snapshot(page)).startUrl)).toBe("/fast/s1");
    } finally { await site.stop(); }
  });
});

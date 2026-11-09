// Unabhängige Abnahme B4-UPDDLG (Testagent; Grundlage allein die Spec, Bauer-Bericht nicht gelesen).
// Spec: docs/spec/06-output-ops.md OUT-250/OUT-251 (inkl. OS-Festlegungen 09.11.2026), docs/spec/05-ui.md UI-127/UI-126.
// Der Produktcode wird nur aufgerufen: Kern-Kommandos update.*/settings.*/app.info über die Fake-Bridge, Aktion help.updates
// (= Menüeintrag Hilfe → Nach Updates suchen; das native Menü hat in der Webseite kein DOM), danach nur echte Eingaben
// (Klick, Tastatur, Mausrad).
import { test, expect, type Locator, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { startFakeBridge, type FakeBridge } from "../support/fake-bridge";
import tickFixture from "../../src/ipc/__fixtures__/Tick.json" with { type: "json" };
import { debugFixture } from "../../src/features/help/__fixtures__/fx-ui-basic";
import type { UpdateState } from "../../src/ipc/gen/UpdateState";
import type { Tick } from "../../src/ipc/gen/Tick";

const NEW = "1.0.0-beta.4";
const OLD = "1.0.0-beta.3";
const TITLE: Record<string, string> = { de: "Eine neue Version von OpenSpider ist verfügbar!", en: "A new version of OpenSpider is available!" };
const titleOf = (lang: string) => TITLE[lang] ?? TITLE.en;
const NAMES = {
  de: { skip: "Diese Version überspringen", later: "Später erinnern", quit: "Beim Beenden installieren", now: "Update installieren", crawl: "Nach Crawl-Ende installieren", download: "Im Hintergrund herunterladen", close: "Schließen", auto: "Automatisch nach Updates suchen" },
  // Die englischen Beschriftungen stehen nicht in der Spec (nur die deutschen); gewählt wie die Oberfläche sie führt.
  en: { skip: "Skip this version", later: "Remind me later", quit: "Install on quit", now: "Install update", crawl: "Install after crawl ends", download: "Download in background", close: "Close", auto: "Automatically check for updates" },
} as const;
const namesOf = (lang: string) => (lang === "de" ? NAMES.de : NAMES.en);
const LANGS = ["de", "en", "es", "fr", "it"] as const;

// Kurze Update-Notes im Manifest-Format; die deutsche Hälfte ist absichtlich anders als die englische.
const SHORT = [
  "- **Faster** large crawls",
  "- Skip a version in the [update dialog](https://openspider.test/en/manual/)",
  "- Raw <u>html</u> and <script>window.__pwned = 1</script> stay inert",
  "- Bad [script link](javascript:window.__pwned2=1) is no link",
  "",
  "### Deutsch",
  "",
  "- **Schnellere** große Crawls",
  "- Eine Version im [Update-Dialog](https://openspider.test/handbuch/) überspringen",
  "- Rohes <u>HTML</u> und <script>window.__pwned = 1</script> bleibt wirkungslos",
  "- Böser [Skript-Link](javascript:window.__pwned2=1) ist kein Link",
].join("\n");
// Lange Notes (altes Format der ausführlichen Notes): erzwingt Scrollen im Notes-Kasten.
const LONG = (() => {
  const en = Array.from({ length: 40 }, (_, i) => `- English point ${i + 1} with some words to fill the line and make the box scroll`);
  const de = Array.from({ length: 40 }, (_, i) => `- Deutscher Punkt ${i + 1} mit einigen Worten, damit die Zeile gefüllt wird und der Kasten scrollt`);
  return [...en, "", "### Deutsch", "", ...de].join("\n");
})();

let bridge: FakeBridge;
test.afterEach(async () => { await bridge?.stop(); });

interface Opts {
  lang: string;
  platform?: "mac" | "win";
  size?: { width: number; height: number };
  notes?: string;
  buildNumber?: string | null;
  autoCheck?: boolean;
  /** Ergebnis des Hintergrund-Downloads. */
  download?: UpdateState;
  /** Statt automatischer Meldung: nur Menü „Nach Updates suchen“ liefert ein Update. */
  manualOnly?: boolean;
}
interface Mock { state: UpdateState }

/** Startet die App mit einem Mock-Kern, der Update-Zustand und Einstellungen wie der echte Kern verwaltet. */
async function boot(page: Page, o: Opts): Promise<Mock> {
  await page.addInitScript(({ platform, lang }) => {
    Object.defineProperty(navigator, "platform", { value: platform === "mac" ? "MacIntel" : "Win32" });
    Object.defineProperty(navigator, "userAgentData", { value: { platform: platform === "mac" ? "macOS" : "Windows" } });
    Object.defineProperty(navigator, "languages", { value: [lang] });
  }, { platform: o.platform ?? "mac", lang: o.lang });
  await page.setViewportSize(o.size ?? { width: 1024, height: 640 });
  bridge = await startFakeBridge();
  const core = bridge.core;
  core.settings.general.language = o.lang;
  core.settings.updates.check = o.autoCheck ?? true;
  const mock: Mock = { state: "idle" };
  const release = { version: NEW, notes: o.notes ?? SHORT, pubDate: null };
  const info = { version: OLD, build: "abc12345", ...(o.buildNumber === null ? {} : { buildNumber: o.buildNumber ?? "2578" }), platform: "macos", arch: "aarch64", paths: { settings: "/m/s.toml", crawls: "/m/c", logs: "/m/l", cache: "/m/ca", exports: "/m/e" } };
  core.handlers.set("app.info", () => info);
  core.handlers.set("update.status", () => mock.state);
  core.handlers.set("update.check", () => (mock.state = { available: release }));
  core.handlers.set("update.download", () => (mock.state = o.download ?? { ready: { version: NEW } }));
  core.handlers.set("update.install", () => mock.state);
  core.handlers.set("update.set_auto", (args) => { core.settings.updates.check = (args as { check: boolean }).check; });
  if (!o.manualOnly) mock.state = { available: release };
  await page.goto(`/?bridge=${encodeURIComponent(bridge.url)}`);
  await page.waitForFunction(() => window.__osMenu !== undefined);
  // Sprachen ohne eigenen Katalog (es/fr/it) fallen auf Englisch zurück; gewartet wird, bis die Oberfläche die Sprache übernommen hat.
  await expect.poll(() => page.evaluate(() => window.__osMenu?.model.lang)).toMatch(new RegExp(`^(${o.lang}|en)$`));
  return mock;
}
const dialogOf = (page: Page, lang: string) => page.getByRole("dialog", { name: titleOf(lang) });
async function menuCheck(page: Page) {
  await page.evaluate(async () => {
    // @ts-expect-error Vite-Quellmodul im E2E-Browser: dieselbe Aktion wie der Menüeintrag.
    const { runAction } = await import("/src/ui/actions.ts");
    runAction("help.updates", { source: "menu" });
  });
}
async function runningCrawl() {
  await expect.poll(() => bridge.core.subscribers("main")).toBeGreaterThan(0);
  // Die Fixture bringt eine Warnung „Speicher knapp“ mit, die einen eigenen Dialog öffnet; hier nur der laufende Crawl.
  bridge.core.emit("main", { kind: "tick", tick: { ...tickFixture, notices: [] } as unknown as Tick });
}

interface Box { x: number; y: number; w: number; h: number; right: number; bottom: number }
const box = async (l: Locator): Promise<Box> => {
  const b = await l.boundingBox();
  expect(b, "Element ist sichtbar").not.toBeNull();
  return { x: b!.x, y: b!.y, w: b!.width, h: b!.height, right: b!.x + b!.width, bottom: b!.y + b!.height };
};

// ---------------------------------------------------------------------------------------------------------------------
// Aufbau (OUT-251, OS-Festlegung 09.11.2026): Kopf, Notes-Kasten, Kontrollkästchen, Knopfreihe
// ---------------------------------------------------------------------------------------------------------------------
for (const lang of ["de", "en"] as const) {
  for (const size of [{ width: 1024, height: 640 }, { width: 800, height: 640 }]) {
    // spec: OUT-251
    test(`${lang} ${size.width}×${size.height}: Aufbau von oben nach unten, Kopf mit Symbol links und fettem Titel, scrollbarer Notes-Kasten`, async ({ page }) => {
      const n = namesOf(lang);
      await boot(page, { lang, size, notes: LONG });
      const dialog = dialogOf(page, lang);
      await expect(dialog).toBeVisible();
      await expect(dialog.getByRole("button", { name: n.now, exact: true })).toBeEnabled();
      // (1) Kopf: Symbol links, rechts daneben Titel (fett), darunter der Text.
      const title = dialog.getByRole("heading", { name: titleOf(lang) });
      const icon = dialog.locator("img:visible").first();
      const description = dialog.getByText(/OpenSpider 1\.0\.0-beta\.4/).first();
      const [bi, bt] = [await box(icon), await box(title)];
      expect(bi.right, "Symbol steht links vom Titel").toBeLessThanOrEqual(bt.x + 1);
      expect(await title.evaluate((el) => Number(getComputedStyle(el).fontWeight)), "Titel fett").toBeGreaterThanOrEqual(600);
      const expectedText = lang === "de"
        ? `OpenSpider ${NEW} ist verfügbar – Sie haben ${OLD} (Build 2578). Möchten Sie die neue Version jetzt installieren?`
        : null;
      if (expectedText) await expect(dialog).toContainText(expectedText);
      else await expect(dialog).toContainText(`${OLD} (Build 2578)`);
      const bd = await box(description);
      expect(bd.y, "Text steht unter dem Titel").toBeGreaterThan(bt.bottom - 2);
      // (2) Notes-Kasten mit Überschrift „OpenSpider <Version>“ und gerenderter Liste, darunter (3) Kontrollkästchen, (4) Knopfreihe.
      const heading = dialog.getByRole("heading", { name: `OpenSpider ${NEW}`, exact: true });
      await expect(heading).toBeVisible();
      const scroller = await heading.evaluateHandle((el) => {
        for (let p = el.parentElement; p; p = p.parentElement) {
          const o = getComputedStyle(p).overflowY;
          if ((o === "auto" || o === "scroll") && p.scrollHeight > p.clientHeight) return p;
          if (p.getAttribute("role") === "dialog") break;
        }
        return null;
      });
      expect(await scroller.evaluate((el) => el !== null), "Notes-Kasten ist ein eigener scrollbarer Bereich").toBe(true);
      const bn = await box(heading);
      const checkbox = dialog.getByRole("checkbox", { name: n.auto });
      const bc = await box(checkbox);
      const buttons = dialog.getByRole("button");
      const bb = await box(buttons.first());
      expect(bn.y).toBeGreaterThan(bd.bottom - 2);
      expect(bc.y, "Kontrollkästchen unter dem Notes-Kasten").toBeGreaterThan(bn.bottom);
      expect(bb.y, "Knöpfe unter dem Kontrollkästchen").toBeGreaterThan(bc.bottom - 2);
      // Der Kasten scrollt mit dem Mausrad, die Knopfreihe bleibt dabei an ihrer Stelle.
      const before = await scroller.evaluate((el) => (el as HTMLElement).scrollTop);
      await heading.hover();
      await page.mouse.wheel(0, 300);
      await expect.poll(() => scroller.evaluate((el) => (el as HTMLElement).scrollTop)).toBeGreaterThan(before);
      expect((await box(buttons.first())).y).toBeCloseTo(bb.y, 0);
      // Kleinste Fensterhöhe: Dialog samt Knopfreihe liegt im Fenster.
      if (size.width === 1024) {
        const all = await box(dialog);
        expect(all.bottom).toBeLessThanOrEqual(size.height + 1);
        expect(all.y).toBeGreaterThanOrEqual(-1);
      }
      // UI-126: automatischer axe-Lauf ohne critical/serious.
      const axe = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
      expect(axe.violations.filter((v) => v.impact === "critical" || v.impact === "serious")).toEqual([]);
    });
  }
}

for (const platform of ["mac", "win"] as const) for (const lang of ["de", "en"] as const) {
  // spec: OUT-251
  test(`${platform}/${lang}: eine Knopfreihe — „überspringen“ links; rechts Später, Beim Beenden, Update installieren (macOS rechts außen, Windows zuerst)`, async ({ page }) => {
    const n = namesOf(lang);
    await boot(page, { lang, platform, size: { width: 800, height: 640 } });
    const dialog = dialogOf(page, lang);
    await expect(dialog.getByRole("button", { name: n.now, exact: true })).toBeEnabled();
    const b = {
      skip: await box(dialog.getByRole("button", { name: n.skip, exact: true })),
      later: await box(dialog.getByRole("button", { name: n.later, exact: true })),
      quit: await box(dialog.getByRole("button", { name: n.quit, exact: true })),
      now: await box(dialog.getByRole("button", { name: n.now, exact: true })),
    };
    // eine Reihe
    const tops = Object.values(b).map((x) => Math.round(x.y));
    expect(new Set(tops).size, `alle vier in einer Reihe: ${tops}`).toBe(1);
    // „Diese Version überspringen“ links, vor allen anderen, und von der rechten Gruppe abgesetzt
    expect(b.skip.x).toBeLessThan(Math.min(b.later.x, b.quit.x, b.now.x));
    if (platform === "mac") {
      expect(b.later.x).toBeLessThan(b.quit.x);
      expect(b.quit.x).toBeLessThan(b.now.x);
      expect(b.now.right, "Primärknopf ganz rechts").toBeGreaterThanOrEqual(Math.max(b.skip.right, b.later.right, b.quit.right));
    } else {
      expect(b.now.x, "Primärknopf zuerst in der rechten Gruppe").toBeLessThan(b.later.x);
      expect(b.later.x).toBeLessThan(b.quit.x);
    }
    // Gruppen: links „überspringen“, rechts bündig am Dialogrand
    const dlg = await box(dialog);
    const rightmost = Math.max(b.later.right, b.quit.right, b.now.right);
    expect(dlg.right - rightmost, "rechte Gruppe liegt am rechten Rand").toBeLessThan(48);
    expect(b.skip.x - dlg.x, "überspringen liegt am linken Rand").toBeLessThan(48);
  });
}

// ---------------------------------------------------------------------------------------------------------------------
// Beschriftungen einzeilig, ohne Abschneiden und ohne horizontalen Überlauf (OUT-251, 800 px und 1024×640, alle Sprachen)
// ---------------------------------------------------------------------------------------------------------------------
async function measureAll(dialog: Locator, label: string) {
  const m = await dialog.evaluate((root) => {
    const buttons = [...root.querySelectorAll<HTMLElement>("button")].filter((b) => b.getClientRects().length > 0);
    const modal = (root.closest(".os-modal") ?? root).getBoundingClientRect();
    return {
      page: document.documentElement.scrollWidth - window.innerWidth,
      dialog: root.scrollWidth - root.clientWidth,
      modalLeft: modal.left, modalRight: modal.right, width: window.innerWidth,
      buttons: buttons.map((el) => {
        const range = document.createRange();
        range.selectNodeContents(el);
        const lines = new Set([...range.getClientRects()].filter((r) => r.width > 0).map((r) => Math.round(r.top))).size;
        const s = getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        const lh = parseFloat(s.lineHeight) || parseFloat(s.fontSize) * 1.2;
        const inner = el.clientHeight - parseFloat(s.paddingTop) - parseFloat(s.paddingBottom);
        return { text: el.textContent ?? "", lines, clipped: el.scrollWidth - el.clientWidth, tall: inner >= lh * 2 - 1, left: rect.left, right: rect.right };
      }),
    };
  });
  const where = `[${label}]`;
  expect(m.buttons.length, `${where} Knöpfe vorhanden`).toBeGreaterThan(0);
  expect(m.page, `${where} kein horizontaler Überlauf der Seite`).toBeLessThanOrEqual(0);
  expect(m.dialog, `${where} kein horizontaler Überlauf des Dialogs`).toBeLessThanOrEqual(0);
  expect(m.modalLeft, where).toBeGreaterThanOrEqual(0);
  expect(m.modalRight, where).toBeLessThanOrEqual(m.width + 0.5);
  for (const b of m.buttons) {
    expect(b.lines, `${where} „${b.text}“ einzeilig`).toBe(1);
    expect(b.clipped, `${where} „${b.text}“ nicht abgeschnitten`).toBeLessThanOrEqual(0);
    expect(b.tall, `${where} „${b.text}“ nicht zweizeilig hoch`).toBe(false);
    expect(b.left, `${where} „${b.text}“ im Fenster`).toBeGreaterThanOrEqual(0);
    expect(b.right, `${where} „${b.text}“ im Fenster`).toBeLessThanOrEqual(m.width + 0.5);
  }
  return m.buttons.map((b) => b.text);
}

for (const size of [{ width: 800, height: 640 }, { width: 1024, height: 640 }]) for (const lang of LANGS) {
  // spec: OUT-251
  test(`${lang} ${size.width}×${size.height}: alle Aktionsknöpfe einzeilig in jedem Zustand (laden, bereit, Crawl läuft, vorgemerkt)`, async ({ page }) => {
    const mock = await boot(page, { lang, size, notes: LONG, download: { failed: "mock" } });
    const dialog = page.getByRole("dialog").first();
    await expect(dialog).toBeVisible();
    const buttons = dialog.getByRole("button");
    // Zustand 1: Download fehlgeschlagen → „Im Hintergrund herunterladen“ (DOM-Reihenfolge macOS: überspringen, später, Hauptknopf)
    await expect(buttons.last()).toBeVisible();
    const failed = await measureAll(dialog, `${lang} ${size.width} Download`);
    if (lang === "de") expect(failed).toContain(NAMES.de.download);
    // Zustand 2: bereit — der Hauptknopf (letzter) lädt erneut; Antwort jetzt „bereit“.
    mock.state = { ready: { version: NEW } };
    bridge.core.handlers.set("update.download", () => (mock.state = { ready: { version: NEW } }));
    await buttons.last().click();
    await expect(buttons).toHaveCount(4);
    await expect(buttons.last()).toBeEnabled();
    const ready = await measureAll(dialog, `${lang} ${size.width} bereit`);
    if (lang === "de") for (const t of [NAMES.de.skip, NAMES.de.later, NAMES.de.quit, NAMES.de.now]) expect(ready).toContain(t);
    // Zustand 3: Crawl läuft → „Nach Crawl-Ende installieren“ ersetzt beide Installationsknöpfe.
    await runningCrawl();
    await expect(buttons).toHaveCount(3);
    const crawl = await measureAll(dialog, `${lang} ${size.width} Crawl`);
    if (lang === "de") { expect(crawl).toContain(NAMES.de.crawl); expect(crawl).not.toContain(NAMES.de.now); expect(crawl).not.toContain(NAMES.de.quit); }
    // Zustand 4: vorgemerkt → „Schließen“.
    await buttons.last().click();
    await expect.poll(() => bridge.core.calls.find((c) => c.cmd === "update.install")?.args).toEqual({ when: "afterCrawl" });
    const queued = await measureAll(dialog, `${lang} ${size.width} vorgemerkt`);
    if (lang === "de") expect(queued).toContain(NAMES.de.close);
  });
}

// ---------------------------------------------------------------------------------------------------------------------
// Notes: nur Oberflächensprache, keine Rohüberschriften, Links extern, kein HTML (OUT-251, Regel OUT-257)
// ---------------------------------------------------------------------------------------------------------------------
for (const lang of LANGS) {
  // spec: OUT-251
  // spec: OUT-257
  test(`${lang}: Notes nur in der Oberflächensprache (de → deutscher Teil, sonst englischer), keine Rohüberschrift, Liste und Fettdruck gerendert`, async ({ page }) => {
    await boot(page, { lang });
    const dialog = page.getByRole("dialog").first();
    await expect(dialog).toBeVisible();
    const heading = dialog.getByRole("heading", { name: `OpenSpider ${NEW}`, exact: true });
    await expect(heading).toBeVisible();
    const text = (await dialog.innerText()).replace(/\s+/g, " ");
    if (lang === "de") {
      expect(text).toContain("Schnellere große Crawls");
      expect(text).not.toContain("Faster");
      expect(text).not.toContain("English");
    } else {
      expect(text).toContain("Faster large crawls");
      expect(text).not.toContain("Schnellere");
    }
    expect(text, "keine Rohüberschrift").not.toContain("###");
    expect(text).not.toMatch(/Deutsch\s*(?:-|•|Schnellere)/);
    expect(await dialog.getByRole("heading", { name: "Deutsch", exact: true }).count()).toBe(0);
    expect(text, "Fettdruck gerendert, nicht als **").not.toContain("**");
    expect(await dialog.locator("li").count(), "Listenpunkte gerendert").toBe(4);
    expect(await dialog.locator("strong").first().innerText()).toMatch(/^(Faster|Schnellere)$/);
  });
}

// spec: OUT-251
test("HTML in den Notes wird nicht ausgeführt, javascript:-Adressen werden keine Links", async ({ page }) => {
  await boot(page, { lang: "en" });
  const dialog = dialogOf(page, "en");
  await expect(dialog).toBeVisible();
  await expect(dialog.locator("li")).toHaveCount(4);
  const notes = dialog.getByRole("heading", { name: `OpenSpider ${NEW}`, exact: true }).locator("xpath=..");
  expect(await notes.locator("script, u, img, iframe, style, object, embed").count()).toBe(0);
  expect(await page.evaluate(() => (window as unknown as Record<string, unknown>).__pwned)).toBeUndefined();
  expect(await dialog.locator('a[href^="javascript:" i]').count()).toBe(0);
  // Auch ein Klick auf den Text „script link“ führt nichts aus.
  await dialog.getByText("script link").click();
  expect(await page.evaluate(() => (window as unknown as Record<string, unknown>).__pwned2)).toBeUndefined();
  await expect(dialog).toBeVisible();
});

for (const lang of ["de", "en"] as const) {
  // spec: OUT-251
  test(`${lang}: ein Link in den Notes öffnet extern (neuer Tab/Standardbrowser); das App-Fenster navigiert nicht`, async ({ page }) => {
    await boot(page, { lang });
    await page.context().route("https://openspider.test/**", (route) => route.fulfill({ status: 200, contentType: "text/html", body: "<title>extern</title>" }));
    const dialog = dialogOf(page, lang);
    await expect(dialog).toBeVisible();
    const here = page.url();
    const link = dialog.getByRole("link", { name: lang === "de" ? "Update-Dialog" : "update dialog" });
    await expect(link).toHaveAttribute("href", lang === "de" ? "https://openspider.test/handbuch/" : "https://openspider.test/en/manual/");
    const [popup] = await Promise.all([page.context().waitForEvent("page"), link.click()]);
    await expect.poll(() => popup.url()).toBe(lang === "de" ? "https://openspider.test/handbuch/" : "https://openspider.test/en/manual/");
    expect(page.url(), "App-Fenster bleibt auf seiner Adresse").toBe(here);
    await expect(dialog, "Dialog bleibt offen").toBeVisible();
    await popup.close();
  });
}

// ---------------------------------------------------------------------------------------------------------------------
// Aktionen: überspringen, später erinnern, beim Beenden, Kontrollkästchen
// ---------------------------------------------------------------------------------------------------------------------
for (const lang of ["de", "en"] as const) {
  // spec: OUT-251
  test(`${lang}: „Diese Version überspringen“ merkt die Version in den App-Einstellungen; die manuelle Prüfung zeigt sie trotzdem, eine neuere ebenso`, async ({ page }) => {
    const n = namesOf(lang);
    const mock = await boot(page, { lang });
    const dialog = dialogOf(page, lang);
    await expect(dialog.getByRole("button", { name: n.now, exact: true })).toBeEnabled();
    await dialog.getByRole("button", { name: n.skip, exact: true }).click();
    await expect(dialog).not.toBeVisible();
    await expect.poll(() => {
      const u = bridge.core.settings.updates as unknown as Record<string, unknown>;
      return u.skippedVersion ?? u.skipped_version;
    }).toBe(NEW);
    expect(bridge.core.calls.some((c) => c.cmd === "update.install")).toBe(false);
    // Der Kern meldet die übersprungene Version automatisch nicht mehr (Rust-Abnahme acceptance_upddlg); hier: Status ruhig.
    mock.state = "upToDate";
    // Manuelle Prüfung: zeigt die übersprungene Version.
    bridge.core.handlers.set("update.check", () => (mock.state = { available: { version: NEW, notes: SHORT, pubDate: null } }));
    await menuCheck(page);
    await expect(dialogOf(page, lang)).toBeVisible();
    await expect(dialogOf(page, lang)).toContainText(`OpenSpider ${NEW}`);
    await dialogOf(page, lang).getByRole("button", { name: n.later, exact: true }).click();
    await expect(dialogOf(page, lang)).not.toBeVisible();
    // Eine neuere Version wird gemeldet (Überschrift des Kastens nennt die neuere Version).
    mock.state = "idle";
    bridge.core.handlers.set("update.check", () => (mock.state = { available: { version: "1.0.0-beta.5", notes: SHORT, pubDate: null } }));
    await menuCheck(page);
    await expect(dialogOf(page, lang).getByRole("heading", { name: "OpenSpider 1.0.0-beta.5", exact: true })).toBeVisible();
  });

  // spec: OUT-251
  test(`${lang}: „Später erinnern“ schließt nur — keine Einstellung, keine Installation`, async ({ page }) => {
    const n = namesOf(lang);
    await boot(page, { lang });
    const dialog = dialogOf(page, lang);
    await expect(dialog.getByRole("button", { name: n.now, exact: true })).toBeEnabled();
    const settingsBefore = JSON.stringify(bridge.core.settings);
    await dialog.getByRole("button", { name: n.later, exact: true }).click();
    await expect(dialog).not.toBeVisible();
    expect(JSON.stringify(bridge.core.settings)).toBe(settingsBefore);
    expect(bridge.core.calls.filter((c) => c.cmd === "update.install" || c.cmd === "settings.set")).toEqual([]);
  });

  // spec: OUT-251
  test(`${lang}: „Beim Beenden installieren“ und „Update installieren“ lösen die Installation zum richtigen Zeitpunkt aus`, async ({ page }) => {
    const n = namesOf(lang);
    await boot(page, { lang });
    const dialog = dialogOf(page, lang);
    await expect(dialog.getByRole("button", { name: n.quit, exact: true })).toBeEnabled();
    await dialog.getByRole("button", { name: n.quit, exact: true }).click();
    await expect.poll(() => bridge.core.calls.filter((c) => c.cmd === "update.install").map((c) => c.args)).toEqual([{ when: "onQuit" }]);
    await bridge.stop();
    await boot(page, { lang });
    const again = dialogOf(page, lang);
    await expect(again.getByRole("button", { name: n.now, exact: true })).toBeEnabled();
    await again.getByRole("button", { name: n.now, exact: true }).click();
    await expect.poll(() => bridge.core.calls.filter((c) => c.cmd === "update.install").map((c) => c.args)).toEqual([{ when: "now" }]);
  });

  // spec: OUT-251
  test(`${lang}: Installationsknöpfe sind bis zum Ende des Hintergrund-Downloads gesperrt, der Dialog zeigt den Fortschritt`, async ({ page }) => {
    const n = namesOf(lang);
    const mock = await boot(page, { lang, download: { downloading: { done: 42, total: 100 } } });
    const dialog = dialogOf(page, lang);
    await expect(dialog.getByRole("progressbar")).toHaveAttribute("value", "42");
    await expect(dialog.getByRole("button", { name: n.now, exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: n.quit, exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: n.skip, exact: true })).toBeEnabled();
    await expect(dialog.getByRole("button", { name: n.later, exact: true })).toBeEnabled();
    mock.state = { ready: { version: NEW } };
    // Die App fragt den Zustand beim Download jede Sekunde ab; danach sind beide Knöpfe bedienbar.
    await expect(dialog.getByRole("button", { name: n.now, exact: true })).toBeEnabled({ timeout: 8_000 });
    await expect(dialog.getByRole("button", { name: n.quit, exact: true })).toBeEnabled();
    await expect(dialog.getByRole("progressbar")).toHaveCount(0);
  });

  // spec: OUT-251
  test(`${lang}: laufender Crawl — nur „Nach Crawl-Ende installieren“; kein Neustart wird erzwungen`, async ({ page }) => {
    const n = namesOf(lang);
    await boot(page, { lang });
    const dialog = dialogOf(page, lang);
    await expect(dialog.getByRole("button", { name: n.now, exact: true })).toBeEnabled();
    await runningCrawl();
    await expect(dialog.getByRole("button", { name: n.crawl, exact: true })).toBeVisible();
    await expect(dialog.getByRole("button", { name: n.now, exact: true })).toHaveCount(0);
    await expect(dialog.getByRole("button", { name: n.quit, exact: true })).toHaveCount(0);
    await dialog.getByRole("button", { name: n.crawl, exact: true }).click();
    await expect.poll(() => bridge.core.calls.filter((c) => c.cmd === "update.install").map((c) => c.args)).toEqual([{ when: "afterCrawl" }]);
  });
}

for (const lang of ["de", "en"] as const) for (const value of [true, false]) {
  // spec: OUT-251
  // spec: OUT-250
  test(`${lang}: Kontrollkästchen zeigt die Einstellung „Automatisch nach Updates suchen“ (${value ? "an" : "aus"}) und schreibt Änderungen zurück`, async ({ page }) => {
    const n = namesOf(lang);
    await boot(page, { lang, autoCheck: value });
    const dialog = dialogOf(page, lang);
    const check = dialog.getByRole("checkbox", { name: n.auto });
    await expect(check).toBeVisible();
    if (value) await expect(check).toBeChecked();
    else await expect(check).not.toBeChecked();
    // Klick auf die Beschriftung schaltet um und schreibt in die Einstellung (settings.toml: updates.check).
    await dialog.getByText(n.auto, { exact: true }).click();
    await expect.poll(() => bridge.core.settings.updates.check).toBe(!value);
    if (value) await expect(check).not.toBeChecked();
    else await expect(check).toBeChecked();
    // Zweites Öffnen: der Wert der Einstellung gilt, nicht ein Zwischenstand der Ansicht.
    await dialog.getByRole("button", { name: n.later, exact: true }).click();
    await expect(dialog).not.toBeVisible();
    await menuCheck(page);
    const again = dialogOf(page, lang).getByRole("checkbox", { name: n.auto });
    if (value) await expect(again).not.toBeChecked();
    else await expect(again).toBeChecked();
    // Eine Einstellung zum automatischen Installieren gibt es nicht.
    expect(await dialogOf(page, lang).getByRole("checkbox").count()).toBe(1);
  });
}

// ---------------------------------------------------------------------------------------------------------------------
// Tastatur, Fokus (UI-126)
// ---------------------------------------------------------------------------------------------------------------------
for (const lang of ["de", "en"] as const) {
  // spec: OUT-251
  // spec: UI-126
  test(`${lang}: nur Tastatur — Tab bleibt im Dialog und erreicht alle Bedienelemente, sichtbarer Fokus, Esc wie „Später erinnern“, Fokus kehrt zurück`, async ({ page }) => {
    const n = namesOf(lang);
    await boot(page, { lang, manualOnly: true });
    const opener = page.getByRole("combobox", { name: lang === "de" ? "Start-URL" : "Start URL", exact: true });
    await opener.focus();
    await menuCheck(page);
    const dialog = dialogOf(page, lang);
    await expect(dialog.getByRole("button", { name: n.now, exact: true })).toBeEnabled();
    await expect.poll(() => dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
    const seen = new Map<string, boolean>();
    for (let i = 0; i < 12; i++) {
      await page.keyboard.press("Tab");
      const info = await page.evaluate(() => {
        const el = document.activeElement as HTMLElement | null;
        const s = el ? getComputedStyle(el) : null;
        const label = el?.getAttribute("aria-label") ?? el?.textContent?.trim() ?? "";
        const visible = !!s && ((s.outlineStyle !== "none" && parseFloat(s.outlineWidth) > 0) || s.boxShadow !== "none");
        return { inside: !!el?.closest('[role="dialog"]'), label, visible, tag: el?.tagName ?? "" };
      });
      expect(info.inside, "Fokus bleibt im Dialog").toBe(true);
      if (info.tag !== "INPUT") seen.set(info.label, info.visible);
      else seen.set("checkbox", true);
    }
    for (const name of [n.skip, n.later, n.quit, n.now]) {
      expect([...seen.keys()].some((k) => k.includes(name)), `„${name}“ per Tab erreichbar`).toBe(true);
      expect(seen.get([...seen.keys()].find((k) => k.includes(name))!), `„${name}“ hat sichtbaren Fokus`).toBe(true);
    }
    expect(seen.has("checkbox") || [...seen.keys()].some((k) => k.includes(n.auto)), "Kontrollkästchen per Tab erreichbar").toBe(true);
    const settingsBefore = JSON.stringify(bridge.core.settings);
    await page.keyboard.press("Escape");
    await expect(dialog).not.toBeVisible();
    await expect(opener).toBeFocused();
    expect(JSON.stringify(bridge.core.settings), "Esc ändert keine Einstellung").toBe(settingsBefore);
    expect(bridge.core.calls.some((c) => c.cmd === "update.install")).toBe(false);
  });

  // spec: OUT-251
  // spec: UI-126
  test(`${lang}: Leertaste schaltet das Kontrollkästchen, Enter auf „Beim Beenden installieren“ merkt die Installation vor`, async ({ page }) => {
    const n = namesOf(lang);
    await boot(page, { lang, autoCheck: true });
    const dialog = dialogOf(page, lang);
    await expect(dialog.getByRole("button", { name: n.quit, exact: true })).toBeEnabled();
    const check = dialog.getByRole("checkbox", { name: n.auto });
    for (let i = 0; i < 12 && !(await check.evaluate((el) => el === document.activeElement)); i++) await page.keyboard.press("Tab");
    await expect(check).toBeFocused();
    await page.keyboard.press("Space");
    await expect.poll(() => bridge.core.settings.updates.check).toBe(false);
    const quit = dialog.getByRole("button", { name: n.quit, exact: true });
    for (let i = 0; i < 12 && !(await quit.evaluate((el) => el === document.activeElement)); i++) await page.keyboard.press("Tab");
    await expect(quit).toBeFocused();
    await page.keyboard.press("Enter");
    await expect.poll(() => bridge.core.calls.filter((c) => c.cmd === "update.install").map((c) => c.args)).toEqual([{ when: "onQuit" }]);
  });
}

// ---------------------------------------------------------------------------------------------------------------------
// Versionsanzeige „<Version> (Build <n>)“ (UI-127)
// ---------------------------------------------------------------------------------------------------------------------
for (const lang of ["de", "en"] as const) {
  // spec: UI-127
  test(`${lang}: „Über OpenSpider“, Debug-Info und Update-Dialog nennen die Version als „${OLD} (Build 2578)“`, async ({ page }) => {
    await boot(page, { lang, manualOnly: true, buildNumber: "2578" });
    bridge.core.handlers.set("diag.debugInfo", () => ({ ...debugFixture, app: { ...debugFixture.app, version: OLD, buildNumber: "2578" } }));
    await page.evaluate(async () => {
      // @ts-expect-error Vite-Quellmodul im E2E-Browser.
      const { runAction } = await import("/src/ui/actions.ts");
      runAction("app.about", { source: "menu" });
    });
    const about = page.getByRole("dialog").first();
    await expect(about).toContainText(`${OLD} (Build 2578)`);
    await page.keyboard.press("Escape");
    await expect(about).not.toBeVisible();
    await page.evaluate(async () => {
      // @ts-expect-error Vite-Quellmodul im E2E-Browser.
      const { runAction } = await import("/src/ui/actions.ts");
      runAction("help.debug", { source: "menu" });
    });
    const debug = page.getByRole("dialog").first();
    await expect(debug).toContainText(`${OLD} (Build 2578)`);
    await page.keyboard.press("Escape");
    await expect(debug).not.toBeVisible();
    await menuCheck(page);
    const dialog = dialogOf(page, lang);
    await expect(dialog).toContainText(`${OLD} (Build 2578)`);
    // Die neue Version steht ohne Build-Nummer da (das Manifest kennt sie nicht).
    expect((await dialog.innerText()).replace(/\s+/g, " ")).not.toContain(`${NEW} (Build`);
    await expect(dialog).toContainText(`OpenSpider ${NEW}`);
  });

  // spec: UI-127
  test(`${lang}: Entwickler-Build ohne Build-Nummer — nur die Version, nie „(Build …)“, „null“ oder „undefined“`, async ({ page }) => {
    await boot(page, { lang, manualOnly: true, buildNumber: null });
    bridge.core.handlers.set("diag.debugInfo", () => ({ ...debugFixture, app: { ...debugFixture.app, version: OLD, buildNumber: undefined } }));
    await page.evaluate(async () => {
      // @ts-expect-error Vite-Quellmodul im E2E-Browser.
      const { runAction } = await import("/src/ui/actions.ts");
      runAction("app.about", { source: "menu" });
    });
    const about = page.getByRole("dialog").first();
    await expect(about).toContainText(OLD);
    expect(await about.innerText()).not.toMatch(/\(Build|null|undefined/);
    await page.keyboard.press("Escape");
    await page.evaluate(async () => {
      // @ts-expect-error Vite-Quellmodul im E2E-Browser.
      const { runAction } = await import("/src/ui/actions.ts");
      runAction("help.debug", { source: "menu" });
    });
    const debug = page.getByRole("dialog").first();
    await expect(debug).toContainText(OLD);
    expect(await debug.innerText()).not.toMatch(/\(Build/);
    await page.keyboard.press("Escape");
    await menuCheck(page);
    const dialog = dialogOf(page, lang);
    await expect(dialog).toContainText(OLD);
    expect((await dialog.innerText())).not.toMatch(/\(Build|null|undefined/);
  });
}

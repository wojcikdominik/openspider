// Unabhängige Abnahme B4-PROJDLG (Crawl-Verwaltung: Tippen in den Formularen, „Zuletzt geöffnet“).
// acceptance: independent
// Erwartungen stammen allein aus der Spec: OUT-228 (Crawl-Verwaltung: Projekt anlegen/umbenennen, Umbenennen, Verschieben,
// Suche), UI-124 (Aufruf über Datei → Crawls verwalten…; Dialogregeln UI-Kürzel: Esc schließt, Enter = Standardaktion) und
// CRAWL-127 (OS-Festlegung 09.11.2026: lesbare Einträge unter „Zuletzt geöffnet“, Pfad einzeilig in der Mitte gekürzt,
// voller Pfad als Tooltip und im zugänglichen Namen). Der Produktcode wird nur über die vorhandene Harness
// (`e2e/crawls/harness.tsx`) aufgerufen. Eingaben sind echte Tastatur-Ereignisse: erst `locator.click()` ins Feld, dann
// `page.keyboard.type`; kein `fill`, kein `pressSequentially`.
import { expect, test, type Locator, type Page } from "@playwright/test";
import type {} from "../crawls/harness";

const HARNESS = "/e2e/crawls/index.html";
const RECENT_KEY = "openspider.recent-crawls.v1";
const MOD = "ControlOrMeta";

const manage = (page: Page) => page.getByRole("button", { name: "Manage", exact: true });
const mgmt = (page: Page) => page.getByRole("dialog", { name: "Manage crawls…", exact: true });
const calls = (page: Page, cmd: string) => page.evaluate(cmd => window.crawlsFixture.calls().filter(c => c.cmd === cmd).map(c => c.args), cmd);

async function openManagement(page: Page, recent?: string[]) {
  if (recent) await page.addInitScript(([key, paths]) => localStorage.setItem(key as string, JSON.stringify(paths)), [RECENT_KEY, recent]);
  await page.goto(HARNESS);
  await manage(page).focus();
  await manage(page).click();
  await expect(page.getByRole("checkbox", { name: "Select: Alpha" })).toBeVisible();
}

/** Klick ins Feld (wie ein Nutzer), Inhalt ersetzen, mit echter Tastatur tippen. */
async function typeIntoField(page: Page, field: Locator, text: string) {
  await field.click();
  await page.keyboard.press(`${MOD}+a`);
  await page.keyboard.press("Backspace");
  await expect(field).toHaveValue("");
  await page.keyboard.type(text, { delay: 15 });
  await expect(field).toBeFocused();
  await expect(field).toHaveValue(text);
}

type FormKind = "create" | "renameProject" | "rename" | "move";
const forms: Record<FormKind, { trigger: string; field: { role: "textbox" | "combobox"; name: string } }> = {
  create: { trigger: "Create project", field: { role: "textbox", name: "Name" } },
  renameProject: { trigger: "Rename project", field: { role: "textbox", name: "Name" } },
  rename: { trigger: "Rename", field: { role: "textbox", name: "Name" } },
  move: { trigger: "Move to project", field: { role: "combobox", name: "Destination project" } },
};

/** Stellt die Voraussetzung des Formulars her (Projekt bzw. Crawl markieren) und öffnet es über die Schaltfläche. */
async function openForm(page: Page, kind: FormKind) {
  if (kind === "renameProject") await page.getByRole("treegrid", { name: "Projects" }).getByText(/^Kunde A\b/).first().click();
  if (kind === "rename" || kind === "move") await page.getByRole("checkbox", { name: "Select: Alpha" }).check();
  const trigger = page.getByRole("button", { name: forms[kind].trigger, exact: true });
  await expect(trigger).toBeEnabled();
  await trigger.click();
  const form = page.getByRole("dialog", { name: forms[kind].trigger, exact: true });
  await expect(form).toBeVisible();
  return { trigger, form, field: form.getByRole(forms[kind].field.role, { name: forms[kind].field.name, exact: true }) };
}

test.describe("Crawl-Verwaltung: Tippen in Formularen und Suche", () => {
  test.beforeEach(async ({ page }) => { await openManagement(page); });

  // spec: OUT-228
  test("Suche: Klick ins Feld und Tippen filtert über Name, Projekt und Start-URL, case-insensitiv", async ({ page }) => {
    const search = page.getByRole("searchbox");
    await search.click();
    await page.keyboard.type("kUnDe a", { delay: 15 });
    await expect(search).toBeFocused();
    await expect(search).toHaveValue("kUnDe a");
    await expect(page.getByRole("checkbox", { name: "Select: Alpha" })).toBeVisible();
    await expect(page.getByRole("checkbox", { name: "Select: Beta" })).toBeVisible();
    await expect(page.getByRole("checkbox", { name: "Select: Live" })).toHaveCount(0);

    // Teilstring der Start-URL
    await search.click();
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.type("BETA.te", { delay: 15 });
    await expect(search).toHaveValue("BETA.te");
    await expect(page.getByRole("checkbox", { name: "Select: Beta" })).toBeVisible();
    await expect(page.getByRole("checkbox", { name: "Select: Alpha" })).toHaveCount(0);

    // Löschen mit der Tastatur stellt die volle Liste wieder her
    await search.click();
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.press("Backspace");
    await expect(search).toHaveValue("");
    await expect(page.getByRole("checkbox", { name: "Select: Live" })).toBeVisible();
  });

  // spec: OUT-228
  // spec: UI-124
  test("Mod+F fokussiert die Suche, danach kommt Tippen ohne weiteren Klick an", async ({ page }) => {
    await page.keyboard.press(`${MOD}+f`);
    const search = page.getByRole("searchbox");
    await expect(search).toBeFocused();
    await page.keyboard.type("alpha", { delay: 15 });
    await expect(search).toHaveValue("alpha");
    await expect(page.getByRole("checkbox", { name: "Select: Alpha" })).toBeVisible();
    await expect(page.getByRole("checkbox", { name: "Select: Beta" })).toHaveCount(0);
  });

  // spec: OUT-228
  test("Projekt anlegen: Übernehmen wird erst nach Eingabe aktiv, das Projekt erscheint in der Projektliste", async ({ page }) => {
    const { form, field } = await openForm(page, "create");
    const apply = form.getByRole("button", { name: "Apply", exact: true });
    await field.click();
    await expect(field).toBeFocused();
    await expect(field).toHaveValue("");
    await expect(apply).toBeDisabled();
    await page.keyboard.type("Müller & Söhne", { delay: 15 });
    await expect(field).toHaveValue("Müller & Söhne");
    await expect(apply).toBeEnabled();
    // Löschen der Eingabe nimmt die Aktivierung wieder zurück
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.press("Backspace");
    await expect(apply).toBeDisabled();
    await page.keyboard.type("Müller & Söhne", { delay: 15 });
    await apply.click();
    await expect(form).toHaveCount(0);
    await expect.poll(() => calls(page, "projects.create")).toEqual([{ name: "Müller & Söhne" }]);
    await expect(page.getByRole("treegrid", { name: "Projects" })).toContainText("Müller & Söhne");
    await expect(mgmt(page)).toBeVisible();
  });

  // spec: OUT-228
  test("Projekt umbenennen: Tippen ersetzt den Namen, Enter übernimmt, Projektliste und Crawl-Zeilen zeigen den neuen Namen", async ({ page }) => {
    const { form, field } = await openForm(page, "renameProject");
    await expect(field).toHaveValue("Kunde A");
    await typeIntoField(page, field, "Kunde Z");
    await expect(form.getByRole("button", { name: "Apply", exact: true })).toBeEnabled();
    await page.keyboard.press("Enter");
    await expect(form).toHaveCount(0);
    await expect.poll(() => calls(page, "projects.rename")).toEqual([{ id: "a", name: "Kunde Z" }]);
    const tree = page.getByRole("treegrid", { name: "Projects" });
    await expect(tree).toContainText("Kunde Z");
    await expect(tree.getByText(/^Kunde A\b/)).toHaveCount(0);
    await tree.getByText("All projects", { exact: true }).click();
    await expect(page.getByRole("row").filter({ hasText: "Alpha" }).filter({ hasText: "Kunde Z" })).toHaveCount(1);
    await expect(page.getByRole("row").filter({ hasText: "Beta" }).filter({ hasText: "Kunde Z" })).toHaveCount(1);
  });

  // spec: OUT-228
  test("Crawl umbenennen: Tippen nach Klick ins Feld, Übernehmen aktiv, neuer Name in der Tabelle, ID unverändert", async ({ page }) => {
    const { form, field } = await openForm(page, "rename");
    await expect(field).toHaveValue("Alpha");
    await typeIntoField(page, field, "Alpha neu");
    const apply = form.getByRole("button", { name: "Apply", exact: true });
    await expect(apply).toBeEnabled();
    await apply.click();
    await expect(form).toHaveCount(0);
    await expect.poll(() => calls(page, "crawls.rename")).toEqual([{ crawlId: "0199f000-0000-7000-8000-000000000001", name: "Alpha neu" }]);
    await expect(page.getByRole("checkbox", { name: "Select: Alpha neu" })).toBeVisible();
    await expect(page.getByRole("checkbox", { name: "Select: Alpha", exact: true })).toHaveCount(0);
  });

  // spec: OUT-228
  test("Crawl verschieben: Tippen im Zielfeld, Übernehmen schickt genau das Eingegebene", async ({ page }) => {
    const { form, field } = await openForm(page, "move");
    await typeIntoField(page, field, "Default");
    const apply = form.getByRole("button", { name: "Apply", exact: true });
    await expect(apply).toBeEnabled();
    await apply.click();
    await expect(form).toHaveCount(0);
    await expect.poll(() => calls(page, "crawls.move")).toEqual([{ ids: ["0199f000-0000-7000-8000-000000000001"], project: "Default" }]);
  });

  // spec: OUT-228
  test("Formulare: Leerzeichen und Umlaute kommen an, ein Formular behält nach Abbrechen und Wiederöffnen keine Eingabe", async ({ page }) => {
    const first = await openForm(page, "create");
    await typeIntoField(page, first.field, "  Büro Süd  ");
    await first.form.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(first.form).toHaveCount(0);
    expect(await calls(page, "projects.create")).toEqual([]);
    await expect(first.trigger).toBeFocused();
    const again = await openForm(page, "create");
    await again.field.click();
    await expect(again.field).toHaveValue("");
    await page.keyboard.type("Neu", { delay: 15 });
    await expect(again.field).toHaveValue("Neu");
  });

  // spec: UI-124
  for (const kind of Object.keys(forms) as FormKind[]) {
    test(`${forms[kind].trigger}: Esc schließt nur das Formular, der Fokus kehrt zur Schaltfläche zurück, nichts wird übernommen`, async ({ page }) => {
      const { trigger, form, field } = await openForm(page, kind);
      await field.click();
      await page.keyboard.type("x", { delay: 15 });
      await expect(field).toBeFocused();
      await page.keyboard.press("Escape");
      await expect(form).toHaveCount(0);
      await expect(mgmt(page)).toBeVisible();
      await expect(trigger).toBeFocused();
      for (const cmd of ["projects.create", "projects.rename", "crawls.rename", "crawls.move"]) expect(await calls(page, cmd)).toEqual([]);
      // das zweite Esc schließt die Verwaltung und gibt den Fokus an den Aufrufer zurück
      await page.keyboard.press("Escape");
      await expect(page.getByRole("dialog")).toHaveCount(0);
      await expect(manage(page)).toBeFocused();
    });
  }

  // spec: UI-124
  for (const kind of Object.keys(forms) as FormKind[]) {
    test(`${forms[kind].trigger}: Tab und Umschalt+Tab bleiben im Formular`, async ({ page }) => {
      const { form, field } = await openForm(page, kind);
      await field.click();
      await page.keyboard.type("a", { delay: 15 });
      const inside = () => form.evaluate(root => root.contains(document.activeElement));
      const describe = () => page.evaluate(() => { const el = document.activeElement; return `${el?.tagName}|${el?.getAttribute("aria-label") ?? el?.textContent?.trim() ?? ""}`; });
      const visited = new Set<string>([await describe()]);
      for (const key of ["Tab", "Shift+Tab"]) {
        for (let i = 0; i < 8; i++) {
          await page.keyboard.press(key);
          expect(await inside(), `${key} #${i + 1}: Fokus liegt außerhalb des Formulars (${await describe()})`).toBe(true);
          visited.add(await describe());
        }
      }
      // Der Fokus wandert tatsächlich durch mehrere Bedienelemente des Formulars (Feld, Abbrechen, Übernehmen).
      expect(visited.size).toBeGreaterThanOrEqual(2);
      await expect(form).toBeVisible();
    });
  }

  // spec: UI-124
  test("Globale Kürzel greifen nicht in ein Formular: Buchstaben, Leertaste und Rücktaste wirken nur auf das Feld", async ({ page }) => {
    const { form, field } = await openForm(page, "create");
    await field.click();
    await page.keyboard.type("n e w ? / , . 1 2 3", { delay: 15 });
    await expect(field).toHaveValue("n e w ? / , . 1 2 3");
    await page.keyboard.press("Backspace");
    await page.keyboard.press("Backspace");
    await expect(field).toHaveValue("n e w ? / , . 1 2");
    await page.keyboard.press("Delete");
    await expect(field).toBeFocused();
    await expect(form).toBeVisible();
    await expect(mgmt(page)).toBeVisible();
  });
});

// ---------------------------------------------------------------------------------------------------------------------
// CRAWL-127 (OS-Festlegung 09.11.2026): „Zuletzt geöffnet“ in der Crawl-Verwaltung.

const MAC_LONG = "/Users/dominik.wojcik/Library/Application Support/de.openspider.app/crawls/Kundenprojekte/Agentur Nordlicht/01a18894-1234-7000-8000-bbfe3c06b178.ospider";
const WIN_LONG = "C:\\Users\\Dominik Wojcik\\AppData\\Roaming\\de.openspider.app\\crawls\\Kundenprojekte\\Agentur Nordlicht\\Relaunch\\01a18894-1234-7000-8000-bbfe3c06b178.ospider";
const FILE = "01a18894-1234-7000-8000-bbfe3c06b178.ospider";
const MAC_SHORT = "/Users/dominik/Crawls/shop-test.ospider";
const WIN_SHORT = "C:\\Users\\Dominik\\Crawls\\shop-test.ospider";
const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

interface Layout {
  paths: { button: DOMRect; titleTop: number | null; lines: number; headLeft: number; allVisibleTail: boolean; headStartVisible: boolean; shortened: boolean; clipped: boolean; text: string; tail: string; width: number; height: number; };
}

/** Misst im Browser die Zeile(n) des Pfads innerhalb eines Eintrags („Zuletzt geöffnet“). */
function measure(button: Locator): Promise<Layout["paths"]> {
  return button.evaluate((el: HTMLElement) => {
    const texts: Text[] = [];
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) if ((n.textContent ?? "").trim()) texts.push(n as Text);
    const isPathStart = (s: string) => /^\s*(\/|[A-Za-z]:[\\/]|\\\\|…)/.test(s);
    const startIndex = texts.findIndex(t => isPathStart(t.textContent ?? ""));
    const pathNodes = startIndex < 0 ? [] : texts.slice(startIndex);
    const rects = (t: Text) => { const r = document.createRange(); r.selectNodeContents(t); return [...r.getClientRects()].filter(q => q.width > 0); };
    const clipAncestors = (t: Text) => {
      const out: HTMLElement[] = [];
      for (let p = t.parentElement; p; p = p.parentElement) {
        if (getComputedStyle(p).overflowX !== "visible") out.push(p);
        if (p === el) break;
      }
      return out;
    };
    const allRects = pathNodes.flatMap(rects);
    const tops = [...new Set(allRects.map(r => Math.round(r.top / 3)))];
    const tailNode = pathNodes[pathNodes.length - 1];
    const headNode = pathNodes[0];
    const fullyVisible = (t: Text) => rects(t).every(r => clipAncestors(t).every(c => { const b = c.getBoundingClientRect(); return r.left >= b.left - 1 && r.right <= b.right + 1; }));
    const startVisible = (t: Text) => { const r = rects(t)[0]; return !!r && clipAncestors(t).every(c => { const b = c.getBoundingClientRect(); return r.left >= b.left - 1 && r.left < b.right; }); };
    const clipped = pathNodes.some(t => clipAncestors(t).some(c => rects(t).some(r => r.right > c.getBoundingClientRect().right + 1)));
    const text = pathNodes.map(t => t.textContent ?? "").join("");
    const ellipsisCss = pathNodes.some(t => clipAncestors(t).some(c => getComputedStyle(c).textOverflow === "ellipsis" && c.scrollWidth > c.clientWidth));
    const b = el.getBoundingClientRect();
    const first = texts[0] ? rects(texts[0])[0] : undefined;
    const union = allRects.length ? { left: Math.min(...allRects.map(r => r.left)), right: Math.max(...allRects.map(r => r.right)), top: Math.min(...allRects.map(r => r.top)), bottom: Math.max(...allRects.map(r => r.bottom)) } : { left: 0, right: 0, top: 0, bottom: 0 };
    return {
      button: b.toJSON() as DOMRect, titleTop: startIndex === 0 ? null : first?.top ?? null,
      lines: tops.length, headLeft: union.left, allVisibleTail: !!tailNode && fullyVisible(tailNode), headStartVisible: !!headNode && startVisible(headNode),
      shortened: text.includes("…") || ellipsisCss, clipped, text, tail: tailNode?.textContent ?? "", width: union.right - union.left, height: union.bottom - union.top,
    };
  });
}

test.describe("Zuletzt geöffnet in der Crawl-Verwaltung (CRAWL-127, OS-Festlegung 09.11.2026)", () => {
  test.use({ viewport: { width: 640, height: 800 } });

  const entry = (page: Page, path: string) => page.getByRole("navigation", { name: "Recently opened" }).getByRole("button", { name: path });

  for (const [os, path] of [["macOS", MAC_LONG], ["Windows", WIN_LONG]] as const) {
    // spec: CRAWL-127
    test(`${os}: ein Pfad mit mindestens 150 Zeichen steht im 640 px breiten Fenster in einer Zeile, Anfang und Dateiname mit .ospider sichtbar`, async ({ page }) => {
      expect(path.length).toBeGreaterThanOrEqual(150);
      await openManagement(page, [path]);
      const button = entry(page, path);
      await expect(button).toBeVisible();
      const m = await measure(button);
      expect(m.lines, "Pfad einzeilig").toBe(1);
      expect(m.height, "Pfadzeile ist keine Mehrzeilenbox").toBeLessThan(40);
      expect(m.shortened, "Pfad ist in der Mitte gekürzt").toBe(true);
      expect(m.headStartVisible, "Anfang des Pfads sichtbar").toBe(true);
      expect(m.tail.endsWith(FILE), "letzter sichtbarer Teil endet mit dem Dateinamen").toBe(true);
      expect(m.allVisibleTail, "Dateiname vollständig sichtbar (nicht abgeschnitten)").toBe(true);
      expect(m.text.startsWith(path.slice(0, 6))).toBe(true);
      // der Eintrag passt in das Fenster, kein horizontaler Seitenüberlauf
      expect(m.button.right).toBeLessThanOrEqual(640);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
    });

    // spec: CRAWL-127
    test(`${os}: der volle Pfad bleibt als Tooltip und im zugänglichen Namen erhalten, der Titel zeigt den Dateinamen (unbekannter Crawl)`, async ({ page }) => {
      await openManagement(page, [path]);
      const button = entry(page, path);
      await expect(button).toHaveAccessibleName(new RegExp(escapeRe(path)));
      const tooltip = await button.evaluate((el: HTMLElement) => el.closest("[title]")?.getAttribute("title") ?? null);
      expect(tooltip, "title-Attribut (Tooltip) enthält den vollen Pfad").toBe(path);
      // Hauptzeile = Dateiname (Crawl unbekannt), darunter der Pfad
      const m = await measure(button);
      expect(await button.evaluate((el: HTMLElement) => (el.textContent ?? "").trimStart().startsWith("01a18894"))).toBe(true);
      expect(m.titleTop === null || m.titleTop < m.button.bottom).toBe(true);
    });
  }

  // spec: CRAWL-127
  test("kurze Pfade (macOS und Windows) bleiben ungekürzt und vollständig sichtbar", async ({ page }) => {
    await openManagement(page, [MAC_SHORT, WIN_SHORT]);
    for (const path of [MAC_SHORT, WIN_SHORT]) {
      const m = await measure(entry(page, path));
      expect(m.text, path).toBe(path);
      expect(m.shortened, path).toBe(false);
      expect(m.clipped, path).toBe(false);
      expect(m.lines, path).toBe(1);
      expect(m.allVisibleTail, path).toBe(true);
    }
  });

  // spec: CRAWL-127
  test("bekannter Crawl: Hauptzeile Crawl-Name mit Datum und Uhrzeit, darunter der Pfad; Tooltip und zugänglicher Name tragen den Pfad", async ({ page }) => {
    await openManagement(page, ["/fixture/Alpha.ospider", "/fixture/Beta.ospider"]);
    const nav = page.getByRole("navigation", { name: "Recently opened" });
    const alpha = nav.getByRole("button", { name: "/fixture/Alpha.ospider" });
    const beta = nav.getByRole("button", { name: "/fixture/Beta.ospider" });
    await expect(alpha).toBeVisible();
    await expect(alpha).toHaveAccessibleName(/Alpha/);
    const text = (await alpha.evaluate((el: HTMLElement) => el.textContent ?? "")).trim();
    expect(text).toMatch(/^Alpha – [A-Z][a-z]{2} \d{1,2}, 2026,? \d{1,2}:\d{2}/);
    expect(await alpha.evaluate((el: HTMLElement) => el.closest("[title]")?.getAttribute("title"))).toBe("/fixture/Alpha.ospider");
    expect((await beta.evaluate((el: HTMLElement) => el.textContent ?? "")).trim()).toMatch(/^Beta – [A-Z][a-z]{2} \d{1,2}, 2026,? \d{1,2}:\d{2}/);
    // der Pfad steht unter der Hauptzeile und ist ungekürzt
    const m = await measure(alpha);
    expect(m.text).toBe("/fixture/Alpha.ospider");
    expect(m.titleTop).not.toBeNull();
    expect(m.titleTop!).toBeLessThan(m.button.top + m.button.height);
    expect(await alpha.evaluate((el: HTMLElement) => {
      const nodes: Text[] = []; const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      for (let n = w.nextNode(); n; n = w.nextNode()) if ((n.textContent ?? "").trim()) nodes.push(n as Text);
      const rect = (t: Text) => { const r = document.createRange(); r.selectNodeContents(t); return r.getBoundingClientRect(); };
      return rect(nodes[nodes.length - 1]!).top > rect(nodes[0]!).top;
    }), "Pfad liegt unterhalb der Hauptzeile").toBe(true);
  });

  // spec: CRAWL-127
  test("Einträge sind gleich breit und linksbündig, Reihenfolge wie gespeichert (neueste oben)", async ({ page }) => {
    await openManagement(page, [MAC_LONG, "/fixture/Alpha.ospider", WIN_SHORT, WIN_LONG, "/fixture/Beta.ospider"]);
    const buttons = page.getByRole("navigation", { name: "Recently opened" }).getByRole("button");
    await expect(buttons).toHaveCount(5);
    const boxes = await buttons.evaluateAll(list => list.map(b => { const r = b.getBoundingClientRect(); return { left: r.left, width: r.width, top: r.top }; }));
    for (const b of boxes) { expect(Math.abs(b.left - boxes[0]!.left)).toBeLessThan(1); expect(Math.abs(b.width - boxes[0]!.width)).toBeLessThan(1); }
    expect(boxes.map(b => b.top)).toEqual([...boxes.map(b => b.top)].sort((a, b) => a - b));
    const names = await buttons.evaluateAll(list => list.map(b => b.getAttribute("aria-label") ?? b.textContent ?? ""));
    expect(names[0]).toContain("01a18894");
    expect(names[1]).toContain("Alpha");
    expect(names[4]).toContain("Beta");
  });

  // spec: CRAWL-127
  test("die Kürzung folgt der verfügbaren Breite: im breiteren Fenster ist mindestens so viel vom Pfad sichtbar", async ({ page }) => {
    await openManagement(page, [MAC_LONG]);
    const narrow = await measure(entry(page, MAC_LONG));
    await page.setViewportSize({ width: 1280, height: 800 });
    const wide = await measure(entry(page, MAC_LONG));
    expect(wide.lines).toBe(1);
    expect(wide.tail.endsWith(FILE)).toBe(true);
    expect(wide.allVisibleTail).toBe(true);
    expect(wide.width).toBeGreaterThanOrEqual(narrow.width - 1);
    await page.setViewportSize({ width: 480, height: 800 });
    const tiny = await measure(entry(page, MAC_LONG));
    expect(tiny.lines).toBe(1);
    expect(tiny.tail.endsWith(FILE)).toBe(true);
    expect(tiny.allVisibleTail).toBe(true);
  });

  // spec: CRAWL-127
  test("ein Klick auf einen Eintrag öffnet genau diesen Pfad", async ({ page }) => {
    await openManagement(page, ["/fixture/Beta.ospider", "/fixture/Alpha.ospider"]);
    await entry(page, "/fixture/Alpha.ospider").click();
    await expect.poll(async () => (await calls(page, "crawl.open")).length).toBe(1);
    expect((await calls(page, "crawl.open"))[0]).toMatchObject({ path: "/fixture/Alpha.ospider" });
  });
});

// spec: CRAWL-127
// spec: UI-002
test("echte App-Hülle: das Menü Datei enthält „Zuletzt geöffnet“ genau einmal", async ({ page }) => {
  await page.goto("/?transport=mock");
  await page.waitForFunction(() => !!window.__osMenu?.model);
  const labels = await page.evaluate(async () => {
    const file = window.__osMenu!.model.menus.find((menu: { id: string }) => menu.id === "file")!;
    return (file.items as { label: string }[]).map(item => item.label);
  });
  expect(labels.filter(label => label === "Recently opened"), `Menü Datei: ${labels.join(" | ")}`).toHaveLength(1);
});

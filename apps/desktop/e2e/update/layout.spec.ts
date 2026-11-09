import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { readFileSync } from "node:fs";
import { startFakeBridge, type FakeBridge } from "../support/fake-bridge";

// Update dialog layout (OUT-251, Nutzerentscheidung 09.11.2026): head, scrollable notes box, setting, one button row with
// single-line labels; keyboard and focus per UI-126. Notes fixture: the published beta.3 notes (long, bilingual).
const beta3 = readFileSync(new URL("../../../../tests/fixtures/update-notes/1.0.0-beta.3.md", import.meta.url), "utf8");
const short = readFileSync(new URL("../../../../tests/fixtures/update-notes/short.md", import.meta.url), "utf8");
const TITLE: Record<string, string> = { de: "Eine neue Version von OpenSpider ist verfügbar!", en: "A new version of OpenSpider is available!" };
// es/fr/it have no catalogue for the update dialog yet (B4-LANG) and fall back to English. To keep the row robust for the
// coming translations, the same measurement also runs with longer labels in these languages written into the buttons.
const LONGER: Record<string, string[]> = {
  es: ["Omitir esta versión", "Recordármelo más tarde", "Instalar al salir de la aplicación", "Instalar actualización"],
  fr: ["Ignorer cette version", "Me le rappeler plus tard", "Installer à la fermeture", "Installer la mise à jour"],
  it: ["Salta questa versione", "Ricordamelo più tardi", "Installa all’uscita dall’app", "Installa aggiornamento"],
};
let bridge: FakeBridge;
test.afterEach(async () => { await bridge?.stop(); });

async function openReady(page: Page, lang: string, size: { width: number; height: number }) {
  await page.setViewportSize(size);
  bridge = await startFakeBridge();
  bridge.core.settings.general.language = lang;
  bridge.core.handlers.set("update.status", () => ({ ready: { version: "1.0.0-beta.4" } }));
  bridge.core.handlers.set("update.install", () => ({ ready: { version: "1.0.0-beta.4" } }));
  bridge.core.handlers.set("update.set_auto", () => null);
  await page.goto(`/?bridge=${encodeURIComponent(bridge.url)}`);
  await page.waitForFunction(() => window.__osMenu !== undefined);
  await expect.poll(() => page.evaluate(() => document.documentElement.lang)).toBe(lang === "de" ? "de" : "en");
  await page.evaluate(async (notes) => {
    // @ts-expect-error Vite source module
    const { useUpdate } = await import("/src/features/update/state.ts");
    useUpdate.setState({ open: true, state: { ready: { version: "1.0.0-beta.4" } }, release: { version: "1.0.0-beta.4", notes, pubDate: null } });
  }, beta3);
  const dialog = page.getByRole("dialog", { name: TITLE[lang] ?? TITLE.en });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("0.0.0-mock")).toBeVisible();
  return dialog;
}

/** One line box per label (getClientRects), no clipped text, button height below two lines, nothing outside the window. */
async function measure(page: Page, labels?: string[]) {
  return page.evaluate((labels) => {
    const row = document.querySelector("[data-update-actions]")!;
    const buttons = [...row.querySelectorAll<HTMLElement>(".os-btn")];
    if (labels) buttons.forEach((button, i) => { if (labels[i]) button.textContent = labels[i]!; });
    const modal = row.closest(".os-modal")!.getBoundingClientRect();
    const dialog = row.closest(".os-dialog") as HTMLElement;
    return {
      page: document.documentElement.scrollWidth - window.innerWidth,
      dialog: dialog.scrollWidth - dialog.clientWidth,
      modal: { left: modal.left, right: modal.right, width: window.innerWidth },
      buttons: buttons.map(el => {
        const range = document.createRange();
        range.selectNodeContents(el);
        const lines = new Set([...range.getClientRects()].filter(r => r.width > 0).map(r => Math.round(r.top))).size;
        const style = getComputedStyle(el);
        const line = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.2;
        const box = el.getBoundingClientRect();
        return { label: el.textContent, lines, clipped: el.scrollWidth - el.clientWidth, content: el.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom), line, left: box.left, right: box.right, top: Math.round(box.top) };
      }),
    };
  }, labels);
}
function expectClean(m: Awaited<ReturnType<typeof measure>>) {
  expect(m.page, "no horizontal page overflow").toBeLessThanOrEqual(0);
  expect(m.dialog, "no horizontal dialog overflow").toBeLessThanOrEqual(0);
  expect(m.modal.left).toBeGreaterThanOrEqual(0);
  expect(m.modal.right).toBeLessThanOrEqual(m.modal.width);
  for (const b of m.buttons) {
    expect(b.lines, b.label!).toBe(1);
    expect(b.clipped, b.label!).toBeLessThanOrEqual(0);
    expect(b.content, b.label!).toBeLessThan(b.line * 2);
    expect(b.left, b.label!).toBeGreaterThanOrEqual(m.modal.left);
    expect(b.right, b.label!).toBeLessThanOrEqual(m.modal.right);
  }
}

for (const size of [{ width: 800, height: 600 }, { width: 1024, height: 640 }]) {
  for (const lang of ["de", "en", "es", "fr", "it"]) {
    // spec: OUT-251
    // spec: UI-121
    test(`${lang} ${size.width}×${size.height}: all four actions on one line each, one row, no overflow`, async ({ page }) => {
      const dialog = await openReady(page, lang, size);
      const names = lang === "de"
        ? ["Diese Version überspringen", "Später erinnern", "Beim Beenden installieren", "Update installieren"]
        : ["Skip this version", "Remind me later", "Install on quit", "Install update"];
      for (const name of names) await expect(dialog.getByRole("button", { name, exact: true })).toBeVisible();
      const m = await measure(page);
      expect(m.buttons).toHaveLength(4);
      expectClean(m);
      // de and en fit in a single row from 800 px on.
      expect(new Set(m.buttons.map(b => b.top)).size).toBe(1);
      if (LONGER[lang]) expectClean(await measure(page, LONGER[lang]));
    });
  }
}

// spec: OUT-251
test("de at 1024×640: the notes box shows only the German part as a rendered list and scrolls inside the dialog", async ({ page }, info) => {
  const dialog = await openReady(page, "de", { width: 1024, height: 640 });
  // UI-126: the primary action has the initial focus.
  await expect(dialog.getByRole("button", { name: "Update installieren", exact: true })).toBeFocused();
  const notes = dialog.locator("[data-update-notes]");
  await expect(notes.locator("h3")).toHaveText("OpenSpider 1.0.0-beta.4");
  await expect(notes.locator("ul > li")).toHaveCount(11);
  await expect(notes).toContainText("Große Crawls schneller");
  await expect(notes).not.toContainText("### Deutsch");
  await expect(notes).not.toContainText("Faster large crawls");
  expect(await notes.evaluate(el => el.scrollHeight > el.clientHeight && getComputedStyle(el).overflowY === "auto")).toBe(true);
  await expect(dialog.getByRole("checkbox", { name: "Automatisch nach Updates suchen" })).toBeChecked();
  await page.screenshot({ path: info.outputPath(`after-${info.project.name}-1024.png`) });
  await page.setViewportSize({ width: 800, height: 600 });
  await page.screenshot({ path: info.outputPath(`after-${info.project.name}-800.png`) });
  // From beta.4 on the manifest carries the short update notes (docs/release/release-rhythmus.md).
  await page.evaluate(async (notes) => {
    // @ts-expect-error Vite source module
    const { useUpdate } = await import("/src/features/update/state.ts");
    useUpdate.setState({ release: { version: "1.0.0-beta.4", notes, pubDate: null } });
  }, short);
  await expect(notes.locator("ul > li")).toHaveCount(3);
  await expect(notes.locator("strong")).toHaveText("Update-Dialog");
  await page.screenshot({ path: info.outputPath(`after-short-${info.project.name}-800.png`) });
});

// spec: OUT-251
// spec: UI-126
for (const lang of ["de", "en"]) test(`${lang}: keyboard only — primary focused, Tab stays inside, Escape closes, focus returns; axe`, async ({ page }) => {
  const size = { width: 1024, height: 640 };
  await page.setViewportSize(size);
  bridge = await startFakeBridge();
  bridge.core.settings.general.language = lang;
  bridge.core.handlers.set("update.status", () => ({ ready: { version: "1.0.0-beta.4" } }));
  bridge.core.handlers.set("update.check", () => ({ available: { version: "1.0.0-beta.4", notes: beta3, pubDate: null } }));
  bridge.core.handlers.set("update.download", () => ({ ready: { version: "1.0.0-beta.4" } }));
  bridge.core.handlers.set("update.install", () => ({ ready: { version: "1.0.0-beta.4" } }));
  await page.goto(`/?bridge=${encodeURIComponent(bridge.url)}`);
  await page.waitForFunction(() => window.__osMenu !== undefined);
  await expect.poll(() => page.evaluate(() => window.__osMenu?.model.lang)).toBe(lang);
  const opener = page.getByRole("combobox", { name: lang === "de" ? "Start-URL" : "Start URL", exact: true });
  await opener.focus();
  await page.evaluate(async () => {
    // @ts-expect-error Vite source module
    const { runAction } = await import("/src/ui/actions.ts");
    runAction("help.updates", { source: "menu" });
  });
  const dialog = page.getByRole("dialog", { name: TITLE[lang] });
  const install = dialog.getByRole("button", { name: lang === "de" ? "Update installieren" : "Install update", exact: true });
  await expect(install).toBeEnabled();
  // Focus is in the dialog (on the primary action once the download is ready, before that on "Remind me later").
  expect(await dialog.evaluate(el => el.contains(document.activeElement))).toBe(true);
  const reached = new Set<string>();
  for (let i = 0; i < 8; i++) {
    await page.keyboard.press("Tab");
    const active = await page.evaluate(() => { const el = document.activeElement as HTMLElement | null; return { inside: !!el?.closest('[role="dialog"]'), name: el?.getAttribute("aria-label") ?? el?.textContent?.trim() ?? "" }; });
    expect(active.inside).toBe(true);
    reached.add(active.name);
  }
  for (const name of lang === "de" ? ["Diese Version überspringen", "Später erinnern", "Beim Beenden installieren", "Update installieren", "Release-Notes"] : ["Skip this version", "Remind me later", "Install on quit", "Install update", "Release notes"]) {
    expect([...reached].some(n => n.includes(name)), name).toBe(true);
  }
  const axe = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
  expect(axe.violations.filter(v => v.impact === "critical" || v.impact === "serious")).toEqual([]);
  await page.keyboard.press("Escape");
  await expect(dialog).not.toBeVisible();
  await expect(opener).toBeFocused();
  // Keyboard activation of the primary action installs now.
  await opener.focus();
  await page.evaluate(async () => {
    // @ts-expect-error Vite source module
    const { useUpdate } = await import("/src/features/update/state.ts");
    useUpdate.setState({ open: true });
  });
  await expect(install).toBeVisible();
  await install.focus();
  await page.keyboard.press("Enter");
  await expect.poll(() => bridge.core.calls.find(c => c.cmd === "update.install")?.args).toEqual({ when: "now" });
});

// spec: OUT-251
test("skip this version stores the version in the app settings and closes", async ({ page }) => {
  const dialog = await openReady(page, "en", { width: 1024, height: 640 });
  await dialog.getByRole("button", { name: "Skip this version", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect.poll(() => bridge.core.settings.updates.skippedVersion).toBe("1.0.0-beta.4");
});

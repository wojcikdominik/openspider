// Unabhängige Abnahme M4-V01 (1.1-Etappe) gegen die echte Bridge: Tab „AI Suggestions“ (UI-004 [M4], AI-070), Prüfbereich
// (AI-076: Übernehmen, Bearbeiten mit Live-Prüfung, Ablehnen, Zurücksetzen, Massenaktion), SERP-Vorschau des gewählten
// Kandidaten (UI-037 [M4]), Barrierefreiheit (axe ohne critical/serious) und Deutsch/Englisch.
// Erwartungen nur aus docs/spec/08-ai.md, docs/spec/05-ui.md und den Festlegungen des Orchestrators; Rollen, Beschriftungen und
// data-testid aus den Komponenten. Fixture: tests/fixtures/ai/v01-acceptance/e2e/v01-states.ospider (erzeugt von
// crates/os-app/tests/acceptance_v01_e2e_fixture.rs), je Test als Kopie geöffnet. Kein KI-Anbieter, kein Request.
// spec: UI-004 [M4]
// acceptance: independent
import { copyFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import AxeBuilder from "@axe-core/playwright";
import type { Locator, Page } from "@playwright/test";
import { expect, test } from "../fixtures/app";

test.use({ freshState: true });

const FIXTURE = resolve(import.meta.dirname, "..", "..", "..", "..", "tests", "fixtures", "ai", "v01-acceptance", "e2e", "v01-states.ospider");
const TAB = "AI Suggestions";
// IDs der Fixture (tests/fixtures/ai/v01-acceptance/states.rs, Reihenfolge ROWS; 14/15 aus dem Erzeuger).
const ID = { validOpen: 1, accepted: 2, edited: 3, rejected: 4, novalid: 5, pending: 6, formula: 13, serp: 14, serpReal: 15 };
const V_OPEN_1 = "Startseite für Wanderausrüstung und Zubehör";
const V_OPEN_2 = "Wanderausrüstung online kaufen – Startseite";

async function invoke<T = unknown>(page: Page, cmd: string, args?: unknown): Promise<T> {
  return page.evaluate(async ({ cmd, args }) => {
    const path = "/src/ipc/client.ts";
    const { client } = await import(/* @vite-ignore */ path);
    return client.invoke(cmd, args);
  }, { cmd, args }) as Promise<T>;
}

async function invokeError(page: Page, cmd: string, args?: unknown): Promise<{ code?: string; messageKey?: string } | null> {
  return page.evaluate(async ({ cmd, args }) => {
    const path = "/src/ipc/client.ts";
    const { client } = await import(/* @vite-ignore */ path);
    try {
      await client.invoke(cmd, args);
      return null;
    } catch (e) {
      const err = e as { code?: string; messageKey?: string; error?: { code?: string; messageKey?: string } };
      return { code: err.code ?? err.error?.code, messageKey: err.messageKey ?? err.error?.messageKey };
    }
  }, { cmd, args });
}

async function openFixture(page: Page, app: { open(page: Page, target: "main"): Promise<void>; mode: string }) {
  expect(app.mode, "Abnahme läuft gegen die Bridge").toBe("bridge");
  await app.open(page, "main");
  await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
  const dir = mkdtempSync(join(tmpdir(), "v01-acc-"));
  const copy = join(dir, "v01-states.ospider");
  copyFileSync(FIXTURE, copy);
  await invoke(page, "crawl.open", { kind: "path", path: copy });
  await expect(page.getByTestId("crawl-phase")).toHaveText(/Completed|Abgeschlossen|Fertig/, { timeout: 20_000 });
}

async function preview(page: Page, on: boolean) {
  await invoke(page, "settings.set", { patch: { aiApp: { previewEnabled: on } } });
}

function tablist(page: Page): Locator {
  return page.getByRole("tablist", { name: /^(Data tabs|Daten-Tabs)$/ });
}

async function selectTab(page: Page, name: string) {
  const tab = tablist(page).getByRole("tab", { name, exact: true });
  await expect.poll(async () => {
    if (await tab.count()) return true;
    const group = await page.evaluate(async (name) => {
      const path = "/src/state/tabs.ts";
      const { groupOf } = await import(/* @vite-ignore */ path);
      return groupOf(name) as string;
    }, name);
    await page.locator(`.u34-nav [data-group="${group}"] .u34-nav-button`).click();
    return (await tab.count()) > 0;
  }, { timeout: 15_000 }).toBe(true);
  await tab.click();
  await expect(tab).toHaveAttribute("aria-selected", "true");
}

async function select(page: Page, urlIds: number[], topTab = TAB) {
  await page.evaluate(async ({ urlIds, topTab }) => {
    const path = "/src/features/detail/frame/selection.ts";
    const { setDetailSelection } = await import(/* @vite-ignore */ path);
    setDetailSelection({ urlIds, focus: urlIds[0], topTab });
  }, { urlIds, topTab });
}

function region(page: Page, name = /^(Review AI suggestions|KI-Vorschläge prüfen)$/): Locator {
  return page.getByRole("complementary", { name });
}

async function aiNamesListed(page: Page): Promise<string[]> {
  const tabs = await invoke<{ id: string }[]>(page, "catalog.tabs");
  const exports = await invoke<{ bulk: { items: { name: string }[] }[] }>(page, "export.catalog");
  return [...tabs.map((t) => t.id), ...exports.bulk.flatMap((g) => g.items.map((i) => i.name))].filter((n) => n === TAB || n.startsWith(`${TAB}:`));
}

async function axeSerious(page: Page, include: string) {
  const result = await new AxeBuilder({ page }).include(include).analyze();
  return result.violations.filter((v) => v.impact === "critical" || v.impact === "serious").map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}

const REGION_CSS = "aside.os-ai-sugg";

test.describe("Englisch", () => {
  // spec: UI-004 [M4]
  // spec: AI-070 [M4]
  // acceptance: independent
  test("Schalter aus: kein Tab, kein Prüfbereich, Befehle mit Vorschau-Fehler; eingeschaltet erscheint der Tab", async ({ page, app }) => {
    await openFixture(page, app);
    await preview(page, false);
    expect(await aiNamesListed(page)).toEqual([]);
    await expect(tablist(page).getByRole("tab", { name: TAB, exact: true })).toHaveCount(0);
    await expect(page.locator(REGION_CSS)).toHaveCount(0);
    const err = await invokeError(page, "ai.suggestDetail", { urlId: ID.validOpen });
    expect(err, "ai.suggestDetail darf ohne Schalter nicht gelingen").not.toBeNull();
    expect(err?.messageKey ?? "").toContain("preview");

    await preview(page, true);
    await expect.poll(() => aiNamesListed(page)).toContain(TAB);
    await selectTab(page, TAB);
    const counter = page.getByRole("tabpanel").and(page.locator("#data-panel")).getByText(/^(Showing|Angezeigt): /);
    // 12 Seiten der Zustands-Fixture mit Einträgen + /serp/ (die übrigen zwei Seiten haben keinen Vorschlag).
    await expect(counter).toHaveText(/^(Showing|Angezeigt): 13\b/, { timeout: 15_000 });
  });

  // spec: AI-076 [M4]
  // spec: UI-037 [M4]
  // acceptance: independent
  test("Prüfbereich zeigt Kandidaten mit Status; die SERP-Vorschau folgt dem gewählten Kandidaten", async ({ page, app }) => {
    await openFixture(page, app);
    await preview(page, true);
    await selectTab(page, TAB);
    await select(page, [ID.validOpen]);
    const r = region(page);
    await expect(r).toBeVisible();
    await expect(r.getByTestId("ai-sugg-status")).toHaveText(/^Valid/);
    await expect(r.getByTestId("ai-sugg-review")).toHaveText("Open");
    const radios = r.getByRole("radio");
    await expect(radios).toHaveCount(2);
    await expect(radios.first()).toBeChecked();
    const serpTitle = r.getByTestId("ai-sugg-serp").locator(".os-serp-title");
    await expect(serpTitle).toHaveText(V_OPEN_1);
    await radios.nth(1).check();
    await expect(serpTitle).toHaveText(V_OPEN_2);
    expect(await axeSerious(page, REGION_CSS)).toEqual([]);
  });

  // spec: UI-037 [M4]
  // spec: AI-076 [M4]
  // acceptance: independent
  test("SERP-Vorschau kürzt einen Kandidaten mit 570 px an derselben Stelle wie einen gleichlautenden echten Title", async ({ page, app }) => {
    await openFixture(page, app);
    await preview(page, true);
    // Echter Title: Detail-Tab SERP-Snippet der Seite /serp-real/.
    await selectTab(page, "Page Titles");
    await select(page, [ID.serpReal], "Page Titles");
    const details = page.getByRole("region", { name: "Details" });
    const serpTab = details.getByRole("tab", { name: "SERP-Snippet", exact: true });
    if (await serpTab.count()) await serpTab.click();
    else {
      await details.getByRole("button", { name: "More detail tabs" }).click();
      await page.getByRole("menuitem", { name: "SERP-Snippet", exact: true }).click();
    }
    const real = details.locator(".os-serp-title").first();
    await expect(real).toHaveAttribute("data-truncated", "true");
    const realText = (await real.innerText()).trim();

    await selectTab(page, TAB);
    await select(page, [ID.serp]);
    const r = region(page);
    await expect(r.getByRole("radio")).toHaveCount(2);
    await r.getByRole("radio").nth(1).check();
    const cand = r.getByTestId("ai-sugg-serp").locator(".os-serp-title");
    await expect(cand).toHaveAttribute("data-truncated", "true");
    await expect.poll(async () => (await cand.innerText()).trim()).toBe(realText);
  });

  // spec: AI-076 [M4]
  // acceptance: independent
  test("Übernehmen, Ablehnen und Zurücksetzen ändern den Review Status", async ({ page, app }) => {
    await openFixture(page, app);
    await preview(page, true);
    await selectTab(page, TAB);
    await select(page, [ID.validOpen]);
    const r = region(page);
    const review = r.getByTestId("ai-sugg-review");
    await r.getByRole("button", { name: "Accept", exact: true }).click();
    await expect(review).toHaveText("Accepted");
    await expect(r.getByTestId("ai-sugg-accepted")).toHaveText(V_OPEN_1);
    await r.getByRole("button", { name: "Reject", exact: true }).click();
    await expect(review).toHaveText("Rejected");
    await r.getByRole("button", { name: "Reset", exact: true }).click();
    await expect(review).toHaveText("Open");
    const detail = await invoke<{ elements: { element: string; review: string; accepted: unknown }[] }>(page, "ai.suggestDetail", { urlId: ID.validOpen });
    const title = detail.elements.find((e) => e.element === "title");
    expect(title?.review).toBe("Open");
    expect(title?.accepted ?? null).toBeNull();
  });

  // spec: AI-076 [M4]
  // spec: AI-075 [M4]
  // acceptance: independent
  test("Bearbeiten: Live-Prüfung mit Pixelbreite; ein Verstoß wird mit Verstoßliste als Edited gespeichert", async ({ page, app }) => {
    await openFixture(page, app);
    await preview(page, true);
    await selectTab(page, TAB);
    await select(page, [ID.novalid]);
    const r = region(page);
    await expect(r.getByTestId("ai-sugg-status")).toHaveText(/^No valid suggestion/);
    // Ohne gültigen Kandidaten ist „Übernehmen“ nicht möglich.
    await expect(r.getByRole("button", { name: "Accept", exact: true })).toBeDisabled();
    await r.getByRole("button", { name: "Edit", exact: true }).click();
    const field = r.getByRole("textbox", { name: "Edited text" });
    const tooLong = "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW kaufen";
    await field.fill(tooLong);
    const live = r.getByTestId("ai-sugg-live");
    await expect(live).toHaveAttribute("data-valid", "false");
    const expectedPx = await invoke<{ pixelWidth: number }>(page, "ai.suggestCheck", { urlId: ID.novalid, element: "title", value: tooLong });
    await expect(live).toContainText(`${expectedPx.pixelWidth} px`);
    expect(await axeSerious(page, REGION_CSS)).toEqual([]);
    await r.getByRole("button", { name: "Accept edited", exact: true }).click();
    await expect(r.getByTestId("ai-sugg-review")).toHaveText("Edited");
    await expect(r.getByTestId("ai-sugg-accepted")).toHaveText(tooLong);
    await expect(r.getByTestId("ai-sugg-accepted-violations")).toBeVisible();
    // Ein gültiger bearbeiteter Text ist gültig.
    await r.getByRole("button", { name: "Edit", exact: true }).click();
    await field.fill("Schlafsäcke für jede Jahreszeit im Test");
    await expect(live).toHaveAttribute("data-valid", "true");
  });

  // spec: AI-076 [M4]
  // acceptance: independent
  test("Massenaktion über die Auswahl: nur gültige Kandidaten, URLs ohne gültigen Vorschlag gezählt und offen", async ({ page, app }) => {
    await openFixture(page, app);
    await preview(page, true);
    await selectTab(page, TAB);
    await select(page, [ID.validOpen, ID.rejected, ID.novalid, ID.formula]);
    const r = region(page);
    await r.getByRole("button", { name: "Accept candidate 1 for 4 selected URLs" }).click();
    await expect(r.getByTestId("ai-sugg-message")).toHaveText("3 accepted, 1 without a valid suggestion (stay open).");
    const novalid = await invoke<{ elements: { element: string; review: string }[] }>(page, "ai.suggestDetail", { urlId: ID.novalid });
    expect(novalid.elements.find((e) => e.element === "title")?.review).toBe("Open");
    const open = await invoke<{ elements: { element: string; review: string; accepted: { value: string } | null }[] }>(page, "ai.suggestDetail", { urlId: ID.validOpen });
    const t = open.elements.find((e) => e.element === "title");
    expect(t?.review).toBe("Accepted");
    expect(t?.accepted?.value).toBe(V_OPEN_1);
  });
});

test.describe("Deutsch", () => {
  test.use({ locale: "de-DE" });

  // spec: AI-076 [M4]
  // spec: UI-004 [M4]
  // acceptance: independent
  test("Prüfbereich auf Deutsch, ohne critical/serious-Befunde", async ({ page, app }) => {
    await openFixture(page, app);
    await preview(page, true);
    await selectTab(page, TAB);
    await select(page, [ID.validOpen]);
    const r = region(page, /^KI-Vorschläge prüfen$/);
    await expect(r).toBeVisible();
    await expect(r.getByRole("heading", { name: "Vorschläge prüfen" })).toBeVisible();
    await expect(r.getByTestId("ai-sugg-review")).toHaveText("Offen");
    for (const name of ["Übernehmen", "Bearbeiten", "Ablehnen", "Zurücksetzen"]) {
      await expect(r.getByRole("button", { name, exact: true })).toBeVisible();
    }
    await expect(r.getByRole("region", { name: "SERP-Vorschau des gewählten Kandidaten" })).toBeVisible();
    expect(await axeSerious(page, REGION_CSS)).toEqual([]);
  });
});

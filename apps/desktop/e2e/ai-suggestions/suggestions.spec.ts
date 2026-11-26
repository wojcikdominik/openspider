// M4-V01: Tab „AI Suggestions“ und Prüfbereich gegen die echte Bridge (AI-070, AI-071, AI-075, AI-076; UI-004 [M4],
// UI-037 [M4]). Fixture: Crawl-Datei `tests/fixtures/ai/suggest/e2e/suggest.ospider` (12 Seiten mit Vorschlägen aus einem
// Kernlauf, erzeugt von `os-app` `commands::ai_suggest::tests::e2e_fixture_crawl_is_current`), geöffnet über
// `crawl.open`; kein KI-Anbieter, kein Request. Oberfläche Englisch (Standard).
import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { test as base, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { startBridge, type Bridge } from "../support/bridge";

const FIXTURE = resolve(import.meta.dirname, "../../../../tests/fixtures/ai/suggest/e2e/suggest.ospider");
const TAB = "AI Suggestions";
const BASE = "http://ai.test:8080";

interface App {
  page: Page;
  invoke<T = unknown>(cmd: string, args?: unknown): Promise<T>;
  preview(on: boolean): Promise<void>;
  openTab(): Promise<void>;
  /** Wählt URLs wie die obere Tabelle (veröffentlichte Auswahl, Fokus = erste URL). */
  select(urlIds: number[]): Promise<void>;
  region(): ReturnType<Page["getByRole"]>;
}

const test = base.extend<{ app: App }>({
  app: async ({ page }, use) => {
    const bridge: Bridge | null = await startBridge();
    expect(bridge, "compiled os-app-bridge required").not.toBeNull();
    const dir = mkdtempSync(join(tmpdir(), "os-v01-"));
    const path = join(dir, "suggest.ospider");
    copyFileSync(FIXTURE, path);
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    const invoke = <T,>(cmd: string, args?: unknown) =>
      page.evaluate(async ({ cmd, args }) => {
        const p = "/src/ipc/client.ts";
        const { client } = await import(/* @vite-ignore */ p);
        return client.invoke(cmd, args);
      }, { cmd, args }) as Promise<T>;
    await page.goto(`/?bridge=${encodeURIComponent(bridge!.url)}`);
    await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
    await invoke("crawl.open", { kind: "path", path });
    const app: App = {
      page,
      invoke,
      async preview(on) {
        await invoke("settings.set", { patch: { aiApp: { previewEnabled: on } } });
      },
      async openTab() {
        await page.locator('.u34-nav [data-group="custom"] .u34-nav-button').click();
        const tab = page.getByRole("tablist", { name: /^(Data tabs|Daten-Tabs)$/ }).getByRole("tab", { name: TAB, exact: true });
        await tab.click();
        await expect(tab).toHaveAttribute("aria-selected", "true");
      },
      async select(urlIds) {
        await page.evaluate(async (ids) => {
          const p = "/src/features/detail/frame/selection.ts";
          const { setDetailSelection } = await import(/* @vite-ignore */ p);
          setDetailSelection({ urlIds: ids, focus: ids[0], topTab: "AI Suggestions" });
        }, urlIds);
      },
      region: () => page.getByRole("complementary", { name: "Review AI suggestions" }),
    };
    try {
      await use(app);
    } finally {
      await page.goto("about:blank");
      await bridge!.stop();
      rmSync(dir, { recursive: true, force: true });
    }
    expect(errors).toEqual([]);
  },
});

async function counter(page: Page): Promise<string> {
  return page.locator("#data-panel").getByText(/^(Showing|Angezeigt): /).innerText();
}

// spec: UI-004 [M4]
// spec: AI-070 [M4]
// Mit Schalter aus kein Tab und kein Prüfbereich; eingeschaltet steht „AI Suggestions“ in der Gruppe „Benutzerdefiniert“,
// zeigt je URL mit Vorschlägen eine Zeile und Filter je Element.
test("Tab AI Suggestions erscheint nur mit Schalter und zeigt die Vorschläge des Crawls", async ({ app }) => {
  const { page } = app;
  await page.locator('.u34-nav [data-group="custom"] .u34-nav-button').click();
  await expect(page.getByRole("tab", { name: TAB, exact: true })).toHaveCount(0);
  await expect(app.region()).toHaveCount(0);
  await app.preview(true);
  await app.openTab();
  await expect.poll(() => counter(page), { timeout: 15_000 }).toMatch(/12 of 12|12 von 12/);
  await page.getByRole("button", { name: /^Titles/ }).first().click();
  await expect.poll(() => counter(page), { timeout: 15_000 }).toMatch(/7 of 12|7 von 12/);
  await app.preview(false);
  await expect(page.getByRole("tab", { name: TAB, exact: true })).toHaveCount(0);
  await expect(app.region()).toHaveCount(0);
});

// spec: AI-071 [M4]
// spec: AI-076 [M4]
// Prüfbereich: bei /b/ ist „Rucksäcke …“ Kandidat 1, der doppelte Kandidat trägt `duplicate_candidate` von /a/;
// „Übernehmen“ setzt `Accepted`, „Ablehnen“ `Rejected`, „Zurücksetzen“ wieder `Open`.
test("Kandidaten prüfen, übernehmen, ablehnen und zurücksetzen", async ({ app }) => {
  const { page } = app;
  await app.preview(true);
  await app.openTab();
  await app.select([2]);
  const region = app.region();
  await expect(region).toBeVisible();
  await expect(region).toContainText(`${BASE}/b/`);
  await expect(region.getByRole("radio").first()).toBeChecked();
  await expect(region).toContainText("Candidate 1: Rucksäcke für Wanderungen im großen Vergleich");
  await expect(region).toContainText(`same as the suggestion for ${BASE}/a/`);
  await region.getByRole("button", { name: "Accept", exact: true }).click();
  await expect(region.getByTestId("ai-sugg-review")).toHaveText("Accepted");
  await expect(region.getByTestId("ai-sugg-accepted")).toHaveText("Rucksäcke für Wanderungen im großen Vergleich");
  await region.getByRole("button", { name: "Reject" }).click();
  await expect(region.getByTestId("ai-sugg-review")).toHaveText("Rejected");
  await region.getByRole("button", { name: "Reset" }).click();
  await expect(region.getByTestId("ai-sugg-review")).toHaveText("Open");
  const back = await app.invoke<{ elements: { review: string }[] }>("ai.suggestDetail", { urlId: 2 });
  expect(back.elements[0].review).toBe("Open");
  // Ungültiger Kandidat lässt sich nicht als KI-Vorschlag übernehmen.
  await region.getByRole("radio").nth(1).check();
  await expect(region.getByRole("button", { name: "Accept", exact: true })).toBeDisabled();
});

// spec: AI-076 [M4]
// spec: AI-075 [M4]
// Bearbeiten: Live-Prüfung im Kern (Länge, Pixelbreite); eine Übernahme mit Verstoß wird als `Edited` mit Verstoßliste
// gespeichert.
test("bearbeitete Übernahme mit Verstoß bleibt Edited mit Verstoßliste", async ({ app }) => {
  await app.preview(true);
  await app.openTab();
  await app.select([1]);
  const region = app.region();
  await region.getByRole("button", { name: "Edit" }).click();
  const field = region.getByRole("textbox", { name: "Edited text" });
  await field.fill("Viel zu kurz");
  const live = region.getByTestId("ai-sugg-live");
  await expect(live).toHaveAttribute("data-valid", "false");
  const check = await app.invoke<{ pixelWidth: number; length: number }>("ai.suggestCheck", { urlId: 1, element: "title", value: "Viel zu kurz" });
  await expect(live).toContainText(`${check.length} characters · ${check.pixelWidth} px`);
  await expect(live).toContainText("fewer than 30 characters");
  await region.getByRole("button", { name: "Accept edited" }).click();
  await expect(region.getByTestId("ai-sugg-review")).toHaveText("Edited");
  await expect(region.getByTestId("ai-sugg-accepted-violations")).toContainText("fewer than 30 characters");
});

// spec: UI-037 [M4]
// spec: AI-076 [M4]
// Die SERP-Vorschau zeigt den gewählten Kandidaten; ein zu breiter Text wird an derselben Stelle gekürzt wie ein
// gleichlautender echter Title (gleiche Messung `detail.serpMeasure` im Kern).
test("SERP-Vorschau des gewählten Kandidaten kürzt wie ein echter Title", async ({ app }) => {
  await app.preview(true);
  await app.openTab();
  await app.select([1]);
  const region = app.region();
  const serp = region.getByTestId("ai-sugg-serp");
  await expect(serp.locator(".os-serp-title")).toHaveText("Leichte Wanderschuhe für Damen und Herren");
  await expect(serp.locator(".os-serp-title")).toHaveAttribute("data-truncated", "false");
  const long = "Wanderschuhe für Damen und Herren im großen Vergleich mit Testberichten 2026";
  await region.getByRole("button", { name: "Edit" }).click();
  await region.getByRole("textbox", { name: "Edited text" }).fill(long);
  const measured = await app.invoke<{ rows: { truncated: boolean; cutAt: number | null }[] }>("detail.serpMeasure", {
    title: long, description: "", device: "desktop", boldTerms: [], prefix: "",
  });
  expect(measured.rows[0].truncated).toBe(true);
  const expected = [...long].slice(0, measured.rows[0].cutAt ?? 0).join("").trimEnd() + " …";
  await expect(serp.locator(".os-serp-title")).toHaveText(expected);
  await expect(serp.locator(".os-serp-title")).toHaveAttribute("data-truncated", "true");
});

// spec: AI-076 [M4]
// Massenaktion „Kandidat 1 übernehmen“ für markierte URLs übernimmt nur gültige Kandidaten und meldet den Rest.
test("Massenaktion übernimmt Kandidat 1 der markierten URLs", async ({ app }) => {
  await app.preview(true);
  await app.openTab();
  await app.select([1, 2, 3, 13]);
  const region = app.region();
  await region.getByRole("button", { name: "Accept candidate 1 for 4 selected URLs" }).click();
  await expect(region.getByTestId("ai-sugg-message")).toHaveText("3 accepted, 0 without a valid suggestion (stay open).");
  for (const id of [1, 2, 3]) {
    const d = await app.invoke<{ elements: { review: string }[] }>("ai.suggestDetail", { urlId: id });
    expect(d.elements[0].review).toBe("Accepted");
  }
});

// Plan M4 E9: axe ohne critical/serious im Prüfbereich.
// spec: AI-076 [M4]
test("Prüfbereich ohne kritische Barrierefreiheitsfehler", async ({ app }) => {
  await app.preview(true);
  await app.openTab();
  await app.select([2]);
  await expect(app.region()).toContainText("Candidate 1");
  const result = await new AxeBuilder({ page: app.page }).include(".os-ai-sugg").options({ resultTypes: ["violations"] }).analyze();
  const findings = result.violations.filter((v) => v.impact === "critical" || v.impact === "serious");
  expect(findings.map((v) => ({ rule: v.id, nodes: v.nodes.map((n) => n.target) }))).toEqual([]);
});

// Platzhalter (Plan M4 E4) für das Zusammenspiel mit Kostenvoranschlag/Einwilligung (M4-U01) und der Ausführung in der App
// (M4-N01).
// spec: AI-070 [M4]
test.fixme("wartet auf M4-U01: Kostenvoranschlag-Dialog vor dem Vorschlagslauf und Start nach Bestätigung", async () => {});
// spec: AI-071 [M4]
test.fixme("wartet auf M4-N01: Vorschlagslauf aus der App gegen den ai-mock füllt den Tab AI Suggestions", async () => {});

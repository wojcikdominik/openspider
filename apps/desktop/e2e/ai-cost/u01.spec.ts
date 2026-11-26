// M4-U01 gegen die echte Bridge (fx-ui-basic, Loopback, kein KI-Anbieter, kein Schlüssel): Statusleistenfeld `Kosten` (UI-013 [M4]),
// Dialog „Kostenvoranschlag“ (AI-012) mit der gemeinsamen Fixture `fixtures/estimate.json`, Dialog „Datenfluss bestätigen“ und
// Übersicht „KI-Datenfluss“ (AI-018), Modellwahl je Aufgabe mit Spalte „Reasoning“ (AI-011), Richtlinie (AI-019) und Reiter
// „Erweitert“ (INTEG-005 [M4]). Ohne Schalter „KI (Vorschau)“ ist nichts davon sichtbar. Gewartet wird nur auf Zustände.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import AxeBuilder from "@axe-core/playwright";
import type { Page } from "@playwright/test";
import { expect, test, type Alpha } from "../alpha/fixture";

const ESTIMATE = JSON.parse(readFileSync(new URL("./fixtures/estimate.json", import.meta.url), "utf8")) as Record<string, unknown>;

const QUESTION = {
  id: "page-type", version: "1.0.0", name: "Page Type", type: "predicate",
  instructions: "Is this a product page?", subject: "page", inputs: ["url", "title", "page_text"],
};

async function axe(page: Page, scope: string) {
  const result = await new AxeBuilder({ page }).include(scope).options({ resultTypes: ["violations"] }).analyze();
  const findings = result.violations.filter((v) => v.impact === "critical" || v.impact === "serious");
  expect(findings.map((v) => ({ rule: v.id, impact: v.impact, nodes: v.nodes.map((n) => n.target) })), scope).toEqual([]);
}

async function language(page: Page, lang: "de" | "en") {
  await page.evaluate(async (l) => {
    const path = "/src/i18n/index.ts";
    await (await import(/* @vite-ignore */ path)).setLanguage(l);
  }, lang);
}

async function setPreview(alpha: Alpha, on: boolean) {
  await alpha.invoke("settings.set", { patch: { aiApp: { previewEnabled: on } } });
}

/** Aktive KI-Aufgaben im geöffneten Crawl (Vorschlagstyp Title und eine Frage mit `inputs`, AI-030). */
async function setAiTasks(alpha: Alpha) {
  const dto = await alpha.invoke<{ config: Record<string, Record<string, unknown>> }>("config.get");
  const ai = dto.config.ai ?? {};
  await alpha.invoke("config.set", {
    ...dto,
    config: { ...dto.config, ai: { ...ai, enabled: true, suggestions: { ...((ai.suggestions as object) ?? {}), title: true }, questions: [QUESTION], active_questions: ["page-type"] } },
  });
}

async function crawled(alpha: Alpha) {
  await alpha.open();
  await alpha.startFromHeader();
  await alpha.waitCompleted();
}

// spec: UI-013 [M4]
test("Feld Kosten: ohne Schalter nichts, mit Schalter und KI-Aufgaben Ledger-Stand im Format der UI-Sprache", async ({ alpha }) => {
  const { page } = alpha;
  await crawled(alpha);
  await setAiTasks(alpha);
  // Schalter aus (Standard): kein Feld, kein KI-Dialog, auch mit aktiven KI-Aufgaben.
  await expect(page.locator("[data-shell-ready]")).toHaveAttribute("data-shell-ready", "true");
  await expect(page.getByTestId("ai-cost")).toHaveCount(0);
  await expect(page.getByRole("dialog")).toHaveCount(0);

  await setPreview(alpha, true);
  const field = page.getByTestId("ai-cost");
  await expect(field).toHaveText("Cost: 0.0000 / 5.0000 USD", { timeout: 15_000 });
  await expect(field).toHaveAttribute("title", "AI: 0.0000 USD\nDataForSEO: 0.0000 USD\nOpen reservations: 0.0000 USD");
  await language(page, "de");
  await expect(field).toHaveText("Kosten: 0,0000 / 5,0000 USD");
  await expect(field).toHaveAttribute("title", "KI: 0,0000 USD\nDataForSEO: 0,0000 USD\nOffene Reservierungen: 0,0000 USD");
  await language(page, "en");
  await expect(field).toHaveText("Cost: 0.0000 / 5.0000 USD");

  await setPreview(alpha, false);
  await expect(page.getByTestId("ai-cost")).toHaveCount(0);
});

// spec: AI-012 [M4]
test("Kostenvoranschlag: Abschnitt KI mit Spalte Reasoning, Summe, Budget; Starten gibt frei; axe ohne critical/serious", async ({ alpha }) => {
  const { page } = alpha;
  await alpha.open();
  await setPreview(alpha, true);
  const decision = page.evaluate(async (view) => {
    const path = "/src/features/ai/cost/EstimateDialog.tsx";
    const { showCostEstimate } = await import(/* @vite-ignore */ path);
    return showCostEstimate(view);
  }, ESTIMATE);
  const dialog = page.getByRole("dialog", { name: "Cost estimate" });
  await expect(dialog).toBeVisible();
  const headers = await dialog.getByRole("columnheader").allInnerTexts();
  expect(headers.slice(0, 4)).toEqual(["Task", "Model", "Reasoning", "Items"]);
  await expect(dialog.locator('tr[data-task="suggest-title"] [data-col="Reasoning"]')).toHaveText("low");
  await expect(dialog.locator('tr[data-task="ask-sql"] [data-col="Executable"]')).toHaveText("no (Missing API key)");
  await expect(dialog.locator("[data-ai-estimate-total]")).toHaveText("0.0083 / 0.0111 / 0.0139 USD");
  await expect(dialog.locator("[data-ai-estimate-remaining]")).toHaveText("1.7900 USD");
  await axe(page, "[role='dialog']");
  await language(page, "de");
  const de = page.getByRole("dialog", { name: "Kostenvoranschlag" });
  await expect(de.locator("[data-ai-estimate-total]")).toHaveText("0,0083 / 0,0111 / 0,0139 USD");
  await de.getByRole("button", { name: "Starten" }).click();
  expect(await decision).toBe(true);
  await expect(page.getByRole("dialog")).toHaveCount(0);

  // Über den Kern: ohne angemeldete Schätzer ist nichts zu bestätigen (kein Dialog, kein Request).
  const view = await alpha.invoke<{ confirmationRequired: boolean; sections: { id: string }[] }>("ai.costEstimate", { tasks: [] });
  expect(view.sections.map((s) => s.id)).toEqual(["ai"]);
  expect(view.confirmationRequired).toBe(false);
});

// spec: AI-018 [M4]
test("Datenfluss bestätigen: Stored by Provider, PII-Hinweis bei Schwärzung aus, Einwilligung je Installation, Übersicht mit Widerruf", async ({ alpha }) => {
  const { page } = alpha;
  await alpha.open();
  await setPreview(alpha, true);
  await alpha.invoke("settings.set", { patch: { aiApp: { redactPersonalData: false } } });
  // Standardprofil mit Frage: Hauptfenster (Crawl) und Einstellungsfenster (ohne Crawl) zeigen dieselben Aufgaben.
  await alpha.useProfile("u01-consent", { ai: { enabled: true, questions: [QUESTION], active_questions: ["page-type"] } });
  await alpha.startFromHeader();
  await alpha.waitCompleted();
  const granted = page.evaluate(async () => {
    const path = "/src/features/ai/consent/ConsentDialog.tsx";
    const { requestConsent } = await import(/* @vite-ignore */ path);
    return requestConsent();
  });
  const dialog = page.getByRole("dialog", { name: "Confirm data flow" });
  await expect(dialog).toBeVisible();
  const headers = await dialog.getByRole("columnheader").allInnerTexts();
  expect(headers[headers.indexOf("Training Use (per provider policy)") + 1]).toBe("Stored by Provider");
  await expect(dialog.locator('tr[data-task="decision"] [data-col="Stored by Provider"]')).toHaveText("unknown");
  await expect(dialog.locator('tr[data-task="decision"] [data-col="Fields Sent"]')).toHaveText("URL, Title, Page Text");
  await expect(dialog.locator('tr[data-task="decision"] [data-col="Redaction"]')).toHaveText("off");
  await expect(dialog.locator("[data-ai-pii-notice]")).toBeVisible();
  await axe(page, "[role='dialog']");
  await dialog.getByRole("button", { name: "Confirm" }).click();
  expect(await granted).toBe(true);
  const status = await alpha.invoke<{ requests: unknown[]; rows: { consentGiven: boolean }[] }>("ai.consentStatus");
  expect(status.requests).toEqual([]);
  expect(status.rows.every((r) => r.consentGiven)).toBe(true);
  expect(existsSync(join(alpha.bridge.home, "ai-consent.json"))).toBe(true);

  // Übersicht „KI-Datenfluss“ in den Einstellungen: Zeile mit Zeitpunkt, Widerruf je Anbieter.
  const settings = await alpha.openSettings();
  await settings.getByRole("tab", { name: "AI", exact: true }).click();
  await settings.getByRole("tab", { name: "AI data flow" }).click();
  const overview = settings.locator("[data-ai-dataflow-overview]");
  await expect(overview.locator('tr[data-task="decision"] [data-col="Consent Given At"]')).toHaveText(/^\d{4}-\d{2}-\d{2}T/);
  await axe(settings, "[data-ai-dataflow-overview]");
  await overview.getByRole("button", { name: "Revoke consent for OpenAI" }).click();
  await expect(overview.getByRole("button", { name: "Revoke consent for OpenAI" })).toHaveCount(0);
  const after = await alpha.invoke<{ requests: unknown[] }>("ai.consentStatus");
  expect(after.requests.length).toBeGreaterThan(0);
});

// spec: AI-011 [M4]
// spec: AI-019
test("Konfigurationsseite KI: Spalte Reasoning, Stufe im Entwurf, Richtlinie; axe ohne critical/serious", async ({ alpha }) => {
  const { page } = alpha;
  await alpha.open();
  await setPreview(alpha, true);
  await page.evaluate(async () => {
    const path = "/src/features/config/frame/store.ts";
    (await import(/* @vite-ignore */ path)).openConfig({ anchor: "ai.tasks" });
  });
  const section = page.locator("[data-ai-tasks]");
  await expect(section).toBeVisible();
  await expect(section.getByRole("columnheader", { name: "Reasoning" })).toBeVisible();
  const title = section.locator('tr[data-task="suggest-title"]');
  await expect(title.locator("[data-ai-effective]")).toHaveAttribute("data-ai-effective", "off");
  await expect(section.locator('tr[data-task="decision"] [data-col="Reasoning"]')).toHaveText("no level");
  await expect(section.locator('tr[data-task="suggest-title"] [data-col="Status"]')).toHaveText("Error: Missing API key");
  await title.getByRole("button", { name: /Reasoning level for suggest-title/ }).click();
  const options = await page.getByRole("listbox").getByRole("option").allInnerTexts();
  expect(options).toEqual(["Task default (off)", "model default", "off", "low", "medium", "high", "xhigh", "max"]);
  await page.getByRole("listbox").getByRole("option", { name: "medium", exact: true }).click();
  await expect(title.locator("[data-ai-effective]")).toHaveAttribute("data-ai-effective", "medium");
  await expect(page.locator("[data-ai-policy-setting]").getByRole("radio", { name: "No restriction" })).toBeChecked();
  await axe(page, "[data-page-id='ai']");
});

// spec: INTEG-005 [M4]
// spec: AI-019
test("Einstellungen KI: Reiter Erweitert zeigt die Ollama-Vorgabe 1, EU-Endpunkt mit vorbelegter URL; axe ohne critical/serious", async ({ alpha }) => {
  await alpha.open();
  await setPreview(alpha, true);
  const settings = await alpha.openSettings();
  await settings.getByRole("tab", { name: "AI", exact: true }).click();
  const eu = settings.getByRole("switch", { name: "Use the EU endpoint (https://eu.api.openai.com)" });
  await expect(eu).toBeVisible();
  await axe(settings, "[data-ai-area='settings']");
  await settings.getByRole("tab", { name: "Advanced" }).click();
  const ollama = settings.getByRole("textbox", { name: "Concurrent requests to Ollama" });
  await expect(ollama).toHaveValue("1");
  await expect(settings.locator('[data-ai-advanced-row][data-provider="ollama"]')).toContainText("Default: 1 · range 1–50");
  await axe(settings, "[data-ai-area='settings']");
  await language(settings, "de");
  await expect(settings.locator('[data-ai-advanced-row][data-provider="ollama"]')).toContainText("Standard: 1 · Bereich 1–50");
});

// spec: AI-012 [M4]
// Lauf mit Bestätigungsschwelle 0,0001 USD: Einträge `Pending (confirmation)`, Mock-Zähler 0, nach „Starten“ gesendet. Der Einstieg
// des KI-Laufs ruft `requestCostConfirmation` auf (Ausführung und Statusspalten: Decision Engine).
test.fixme("wartet auf M4-D01: Kostenvoranschlag beim Start eines KI-Laufs mit Pending (confirmation)", async () => {});

// spec: AI-018 [M4]
// Vor dem ersten Lauf einer KI-Aufgabe erscheint „Datenfluss bestätigen“; ohne Bestätigung `Pending (consent)`, Mock-Zähler 0.
test.fixme("wartet auf M4-D01: Dialog Datenfluss bestätigen vor dem ersten KI-Lauf mit Pending (consent)", async () => {});

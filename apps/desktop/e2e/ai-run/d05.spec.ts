// M4-D05 gegen die echte Bridge (fx-ui-basic, Loopback): Einstieg eines KI-Laufs `startAiRun` – Einwilligung, Kostenvoranschlag
// über der Schwelle und erst nach „Starten“ `ai.decisionsRun` (AI-012, AI-018, AI-039); Option „KI-Eingaben speichern“ auf der
// Konfigurationsseite KI nur mit Schalter „KI (Vorschau)“ (AI-032, Nachtrag K4-24). Es gibt keinen KI-Anbieter: der erfundene
// Schlüssel zeigt mit der Basis-URL auf einen geschlossenen lokalen Port, es verlässt kein Request den Rechner. Gewartet wird nur
// auf Zustände.
import AxeBuilder from "@axe-core/playwright";
import type { Page } from "@playwright/test";
import { expect, test, type Alpha } from "../alpha/fixture";

const QUESTION = {
  id: "page-type", version: "1.0.0", name: "Page Type", type: "predicate",
  instructions: "Is this a product page?", subject: "page", inputs: ["url", "title"],
};

// Ein erfundener Schlüssel für die Bridge dieses Tests (AI-009: Umgebungsvariable vor dem Schlüsselbund); nur in dieser Datei.
// Die Basis-URL (`OPENSPIDER_API_BASE_OPENAI`, nur Test-/Debug-Builds, INTEG-019) zeigt auf einen geschlossenen lokalen Port.
test.beforeAll(() => {
  process.env.OPENSPIDER_AI_KEY_OPENAI = "sk-TEST-D05-E2E";
  process.env.OPENSPIDER_API_BASE_OPENAI = "http://127.0.0.1:9/v1";
});
test.afterAll(() => {
  delete process.env.OPENSPIDER_AI_KEY_OPENAI;
  delete process.env.OPENSPIDER_API_BASE_OPENAI;
});

async function axe(page: Page, scope: string) {
  const result = await new AxeBuilder({ page }).include(scope).options({ resultTypes: ["violations"] }).analyze();
  const findings = result.violations.filter((v) => v.impact === "critical" || v.impact === "serious");
  expect(findings.map((v) => ({ rule: v.id, impact: v.impact })), scope).toEqual([]);
}

async function aiOn(alpha: Alpha, threshold: number) {
  await alpha.invoke("settings.set", { patch: { aiApp: { previewEnabled: true, confirmationThreshold: threshold } } });
}

function start(page: Page): Promise<{ state: string; step?: string; message?: string }> {
  return page.evaluate(async () => {
    const path = "/src/features/ai/run/index.ts";
    const { startAiRun } = await import(/* @vite-ignore */ path);
    const result = await startAiRun();
    return { state: result.state as string, step: result.step as string | undefined, message: result.message as string | undefined };
  });
}

// spec: AI-012 [M4]
// spec: AI-018 [M4]
// spec: AI-039 [M4]
test("KI-Analyse starten: Einwilligung, Kostenvoranschlag über der Schwelle, ohne „Starten“ kein Lauf; axe ohne critical/serious", async ({ alpha }) => {
  const { page } = alpha;
  await alpha.open();
  // Schalter vor dem Crawl an: ohne ihn läuft der Crawl ohne KI-Schritte (Laufkopie, PreviewSwitch::restrict).
  await aiOn(alpha, 1);
  await alpha.useProfile("d05-run", { ai: { enabled: true, questions: [QUESTION], active_questions: ["page-type"], auto_run: false } });
  await alpha.startFromHeader();
  await alpha.waitCompleted();

  // 1. Einwilligung: offene Anfragen (erste Übertragung) zeigen den Dialog, sonst geht es ohne ihn weiter.
  const pending = await alpha.invoke<{ requests: unknown[] }>("ai.consentStatus");
  const declined = start(page);
  if (pending.requests.length > 0) {
    const consent = page.getByRole("dialog", { name: "Confirm data flow" });
    await expect(consent).toBeVisible();
    await consent.getByRole("button", { name: "Confirm" }).click();
  }
  // 2. Voranschlag über der Schwelle → „Abbrechen“: kein Lauf.
  const estimate = page.getByRole("dialog", { name: "Cost estimate" });
  const early = await Promise.race([declined, estimate.waitFor({ timeout: 15_000 }).then(() => null)]);
  expect(early, "startAiRun endete ohne Kostenvoranschlag").toBeNull();
  await expect(estimate).toBeVisible();
  await expect(estimate.locator('tr[data-task="decision"]')).toBeVisible();
  await axe(page, "[role='dialog']");
  await estimate.getByRole("button", { name: "Cancel" }).click();
  expect(await declined).toEqual({ state: "declined", step: "confirmation" });
  // Ohne „Starten“ kein Lauf: keine Reservierung, kein Verbrauch.
  const cost = await alpha.invoke<{ spentMicroUsd: number; reservedMicroUsd: number }>("ai.costBudget");
  expect([cost.spentMicroUsd, cost.reservedMicroUsd]).toEqual([0, 0]);

  // Erneut: Einwilligung gilt (kein Dialog), Voranschlag → „Starten“ → Lauf gestartet.
  const started = start(page);
  await expect(page.getByRole("dialog", { name: "Cost estimate" })).toBeVisible();
  await expect(page.getByRole("dialog", { name: "Confirm data flow" })).toHaveCount(0);
  await page.getByRole("dialog", { name: "Cost estimate" }).getByRole("button", { name: "Start" }).click();
  expect((await started).state).toBe("started");
});

// spec: AI-032 [M4]
// Option „KI-Eingaben speichern“: mit Schalter auf der Konfigurationsseite KI (Standard an), ohne Schalter nicht sichtbar.
test("Option KI-Eingaben speichern nur mit Schalter KI (Vorschau), Standard an", async ({ alpha }) => {
  const { page } = alpha;
  await alpha.open();
  await aiOn(alpha, 1_000_000);
  await page.evaluate(async () => {
    const path = "/src/features/config/frame/store.ts";
    (await import(/* @vite-ignore */ path)).openConfig({ anchor: "ai.store_inputs" });
  });
  const option = page.getByRole("checkbox", { name: "Store AI inputs" });
  await expect(option).toBeVisible();
  await expect(option).toBeChecked();
  await axe(page, "[data-page-id='ai']");
  await page.keyboard.press("Escape");
  await alpha.invoke("settings.set", { patch: { aiApp: { previewEnabled: false } } });
  await page.evaluate(async () => {
    const path = "/src/features/config/frame/store.ts";
    (await import(/* @vite-ignore */ path)).openConfig({});
  });
  await expect(page.getByRole("checkbox", { name: "Store AI inputs" })).toHaveCount(0);
});

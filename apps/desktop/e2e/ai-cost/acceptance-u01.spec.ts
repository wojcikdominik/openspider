// Unabhängige Abnahme M4-U01 (E2E-A gegen die echte Bridge `os-app-bridge`), nur aus der Spec abgeleitet:
// - Schalter „KI (Vorschau)“ aus: kein Feld `Kosten`, kein Dialog, kein KI-Abschnitt, kein `ai.*`-Aufruf (IPC-Zähler über
//   die WebSocket-Frames des Fensters).
// - UI-013 [M4]: Feld `Kosten` mit Werten des Kerns, 4 Nachkommastellen, Sprachwechsel ohne Neustart, Tooltip-Aufteilung.
// - AI-012: Dialog „Kostenvoranschlag“ (Spalten mit `Reasoning`, Budget aus dem Ledger, Abbrechen gibt nichts frei),
//   AI-018: Dialog „Datenfluss bestätigen“ (Spalte `Stored by Provider`, Einwilligung bleibt über ein Neuladen, Widerruf).
// - AI-011 Spalte „Reasoning“ auf der Konfigurationsseite KI; AI-019: in 1.1 kein Richtlinienschalter (Entscheidung des
//   Orchestrators 26.11.2026, erwartbar rot bis zur Nacharbeit); INTEG-005 [M4]: „Erweitert“ mit Ollama Standard 1,
//   Bereich 1–50, Eingaben außerhalb werden abgelehnt.
// - axe ohne critical/serious für jeden neuen Dialog; Tastatur: Fokus bleibt im Dialog, Escape schließt, Enter = Standardknopf.
//
// Die Dialoge haben in diesem Stand noch keinen Einstieg in der Oberfläche (den KI-Lauf baut M4-D01). Die Specs öffnen sie
// deshalb über die öffentlichen Funktionen der Module (`showCostEstimate`, `requestConsent`) im laufenden Fenster – derselbe
// Modul-Stand, an dem der Dialog-Host der App hängt. Der Kern antwortet über die Bridge; Requests an KI-Anbieter entstehen
// keine (die Bridge kennt keinen Anbieter und keinen Schlüssel).
// acceptance: independent
import AxeBuilder from "@axe-core/playwright";
import type { BrowserContext, Page } from "@playwright/test";
import { expect, test } from "../fixtures/app";

test.use({ freshState: true });

const MOD = {
  estimate: "/src/features/ai/cost/EstimateDialog.tsx",
  consent: "/src/features/ai/consent/ConsentDialog.tsx",
  api: "/src/features/ai/settings/api.ts",
  config: "/src/features/config/frame/store.ts",
  prefs: "/src/state/prefs.ts",
};

/** Kommandos, die ein Fenster über seine WebSocket-Verbindung zur Bridge schickt. */
function trackCommands(page: Page): string[] {
  const sent: string[] = [];
  page.on("websocket", (ws) => {
    ws.on("framesent", (frame) => {
      try {
        const msg = JSON.parse(String(frame.payload)) as { cmd?: unknown };
        if (typeof msg.cmd === "string") sent.push(msg.cmd);
      } catch {
        /* kein JSON */
      }
    });
  });
  return sent;
}

const aiCommands = (sent: string[]) => sent.filter((c) => c.startsWith("ai."));

function bridgeOf(page: Page): string {
  const url = new URL(page.url()).searchParams.get("bridge");
  expect(url, "E2E-A braucht die echte Bridge (cargo build -p os-app --features e2e-bridge --bin os-app-bridge)").toBeTruthy();
  return url!;
}

/** Eigener Kanal zum Kern (leere Seite, eigene WebSocket-Verbindung; nicht im Zähler der App-Fenster). */
async function probe(context: BrowserContext, url: string) {
  const page = await context.newPage();
  const call = <T = unknown>(cmd: string, args: unknown = null, window = "main"): Promise<T> =>
    page.evaluate(
      ({ url, cmd, args, window }) =>
        new Promise<T>((resolve, reject) => {
          const ws = new WebSocket(url);
          ws.onopen = () => ws.send(JSON.stringify({ id: 1, cmd, args, window }));
          ws.onmessage = (ev) => {
            const msg = JSON.parse(String(ev.data)) as { id?: number; ok?: T; err?: unknown };
            if (msg.id !== 1) return;
            ws.close();
            if ("err" in msg) reject(new Error(`${cmd}: ${JSON.stringify(msg.err)}`));
            else resolve(msg.ok as T);
          };
          ws.onerror = () => reject(new Error(`${cmd}: Bridge nicht erreichbar`));
        }),
      { url, cmd, args, window },
    );
  return { page, call };
}
type Probe = Awaited<ReturnType<typeof probe>>;

type Json = Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any

/** Ändert die Konfiguration des Fensters `main` wie der Konfigurationsdialog (`config.get` → `config.set`). */
async function editConfig(core: Probe, f: (cfg: Json) => void) {
  const dto = await core.call<Json>("config.get");
  f(dto.config);
  await core.call("config.set", dto);
}

/** Eine aktive Seitenfrage mit Eingabefeldern (AI-030 `inputs`) – macht KI-Aufgaben für den Crawl aktiv (UI-013). */
function activeQuestion(cfg: Json, inputs = ["url", "title"]) {
  cfg.ai.enabled = true;
  cfg.ai.questions = [{ id: "u01-q", version: "1.0.0", name: "U01", type: "predicate", instructions: "Abnahme U01", inputs }];
  cfg.ai.active_questions = ["u01-q"];
}

async function openMain(app: { open(page: Page, target: "main"): Promise<void> }, page: Page) {
  await app.open(page, "main");
  await expect(page.getByRole("navigation", { name: /^(Data views|Datenansichten)$/ })).toBeVisible();
}

async function previewOn(core: Probe, page: Page) {
  await core.call("settings.set", { patch: { aiApp: { previewEnabled: true } } });
  // Das Fenster übernimmt den Schalter ohne Neuladen; erst dann hängt der Dialog-Host der KI-Vorschau im Hauptfenster.
  await expect
    .poll(
      () =>
        page.evaluate(async (path) => {
          const prefs = await import(/* @vite-ignore */ path);
          return prefs.usePrefs.getState().dto?.aiApp?.previewEnabled === true;
        }, MOD.prefs),
      { timeout: 10_000 },
    )
    .toBe(true);
}

async function seriousViolations(page: Page, selector: string): Promise<string[]> {
  const results = await new AxeBuilder({ page }).include(selector).analyze();
  return results.violations
    .filter((v) => v.impact === "critical" || v.impact === "serious")
    .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}

/** Bleibt der Fokus beim Tabulieren (vorwärts und rückwärts) im Dialog? */
async function focusStaysInDialog(page: Page, steps = 8) {
  const inside = () => page.evaluate(() => !!document.activeElement?.closest("[role='dialog']"));
  await expect.poll(inside).toBe(true);
  for (let i = 0; i < steps; i++) {
    await page.keyboard.press("Tab");
    expect(await inside(), `Tab ${i + 1}`).toBe(true);
  }
  for (let i = 0; i < steps; i++) {
    await page.keyboard.press("Shift+Tab");
    expect(await inside(), `Shift+Tab ${i + 1}`).toBe(true);
  }
}

/** Öffnet den Kostenvoranschlag mit `view` (oder dem Voranschlag des Kerns) und legt das Ergebnis in `window.__u01Estimate`. */
async function openEstimate(page: Page, view: unknown | null) {
  await page.evaluate(
    async ({ mod, view }) => {
      const dialog = await import(/* @vite-ignore */ mod.estimate);
      const api = await import(/* @vite-ignore */ mod.api);
      const w = window as unknown as { __u01Estimate?: boolean | null };
      w.__u01Estimate = null;
      const shown = view ?? (await api.aiApi.estimate());
      void dialog.showCostEstimate(shown).then((r: boolean) => {
        w.__u01Estimate = r;
      });
    },
    { mod: MOD, view },
  );
}
const estimateResult = (page: Page) => page.evaluate(() => (window as unknown as { __u01Estimate?: boolean | null }).__u01Estimate ?? null);

async function openConsent(page: Page) {
  await page.evaluate(async (mod) => {
    const dialog = await import(/* @vite-ignore */ mod.consent);
    const w = window as unknown as { __u01Consent?: boolean | null };
    w.__u01Consent = null;
    void dialog.requestConsent().then((r: boolean) => {
      w.__u01Consent = r;
    });
  }, MOD);
}
const consentResult = (page: Page) => page.evaluate(() => (window as unknown as { __u01Consent?: boolean | null }).__u01Consent ?? null);

const ESTIMATE_COLUMNS = ["Task", "Model", "Reasoning", "Items", "Cached", "Est. Input Tokens", "Est. Output Tokens", "Est. Cost (low/expected/high)", "Batch Savings", "Executable"];

function fixtureEstimate() {
  const row = (task: string, over: Json = {}) => ({
    task, model: "gpt-6-luna", reasoning: "low", items: 22, cached: 0, estInputTokens: 9500, estOutputTokens: 11264,
    lowMicroUsd: 30_000, expectedMicroUsd: 40_000, highMicroUsd: 50_000, batchSavingsMicroUsd: 0,
    executable: true, reason: "", calibrated: false, requests: 22, ...over,
  });
  const totals = { lowMicroUsd: 30_000, expectedMicroUsd: 40_000, highMicroUsd: 50_000 };
  return {
    sections: [{ id: "ai", rows: [row("suggest-title"), row("ask-sql", { executable: false, reason: "Missing API key", reasoning: "medium" })], totals }],
    totals,
    budget: { budgetMicroUsd: 5_000_000, consumedMicroUsd: 0, remainingMicroUsd: 5_000_000, warning: false, halted: false },
    thresholdMicroUsd: 100,
    confirmationRequired: true,
  };
}

// ---------------------------------------------------------------------------------------------------------------

// spec: UI-013 [M4]
// spec: AI-012 [M4]
// spec: AI-018 [M4]
// acceptance: independent
test("Schalter KI (Vorschau) aus: kein Feld Kosten, kein Dialog, kein KI-Abschnitt und kein ai.*-Aufruf", async ({ app, page, context }) => {
  const sent = trackCommands(page);
  await openMain(app, page);
  const core = await probe(context, bridgeOf(page));
  await editConfig(core, (cfg) => activeQuestion(cfg));
  // Fokuswechsel löst bei Schalter an ein Nachlesen aus; bei Schalter aus darf nichts passieren.
  await page.bringToFront();
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(page.getByTestId("ai-cost")).toHaveCount(0);
  await expect(page.getByText(/^(Cost|Kosten):/)).toHaveCount(0);

  await openEstimate(page, fixtureEstimate());
  await expect(page.getByRole("dialog")).toHaveCount(0);

  const settings = await context.newPage();
  const sentSettings = trackCommands(settings);
  await app.open(settings, "settings");
  const nav = settings.getByRole("tablist", { name: /^(Settings sections|Einstellungsbereiche)$/ });
  await expect(nav.getByRole("tab").first()).toBeVisible();
  await expect(nav.getByRole("tab", { name: /^(AI|KI)$/ })).toHaveCount(0);

  // Die Modulaufrufe oben laufen nur im Hauptfenster; ein ai.*-Command hätte die Frames des Fensters erreicht.
  expect(aiCommands(sent)).toEqual([]);
  expect(aiCommands(sentSettings)).toEqual([]);
});

// spec: UI-013 [M4]
// acceptance: independent
test("Feld Kosten: Werte des Kerns mit 4 Nachkommastellen, Tooltip-Aufteilung, Sprachwechsel ohne Neustart", async ({ app, page, context }) => {
  const sent = trackCommands(page);
  await openMain(app, page);
  const core = await probe(context, bridgeOf(page));
  await previewOn(core, page);
  // Schalter an, aber noch keine aktive KI-Aufgabe → kein Feld (nachdem das Fenster die Konfiguration gelesen hat).
  const reads = sent.filter((c) => c === "config.get").length;
  await expect.poll(() => sent.filter((c) => c === "config.get").length, { timeout: 10_000 }).toBeGreaterThan(reads);
  await expect(page.getByTestId("ai-cost")).toHaveCount(0);

  await editConfig(core, (cfg) => {
    activeQuestion(cfg);
    cfg.ai.budget = { ...cfg.ai.budget, crawl: 3_210_000 };
  });
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  const field = page.getByTestId("ai-cost");
  await expect(field).toHaveText("Cost: 0.0000 / 3.2100 USD", { timeout: 15_000 });
  const tip = ((await field.getAttribute("title")) ?? "").split("\n");
  expect(tip).toContain("DataForSEO: 0.0000 USD");
  expect(tip).toHaveLength(3);
  expect(tip.every((l) => /\d\.\d{4} USD$/.test(l)), tip.join(" | ")).toBe(true);
  expect(aiCommands(sent)).toContain("ai.costBudget");

  const before = page.url();
  await core.call("settings.set", { patch: { general: { language: "de" } } });
  await expect(field).toHaveText("Kosten: 0,0000 / 3,2100 USD", { timeout: 10_000 });
  const tipDe = ((await field.getAttribute("title")) ?? "").split("\n");
  expect(tipDe).toContain("KI: 0,0000 USD");
  expect(tipDe).toContain("DataForSEO: 0,0000 USD");
  expect(tipDe).toContain("Offene Reservierungen: 0,0000 USD");
  expect(page.url()).toBe(before);
});

// spec: AI-012 [M4]
// acceptance: independent
test("Kostenvoranschlag: Spalten, Budget des Kerns, axe, Fokus im Dialog, Escape und Abbrechen geben nichts frei, Enter startet", async ({ app, page, context }) => {
  const sent = trackCommands(page);
  await openMain(app, page);
  const core = await probe(context, bridgeOf(page));
  await previewOn(core, page);

  // Voranschlag des Kerns (keine angemeldete KI-Aufgabe): Crawl-Budget, Verbrauch und Rest aus dem BudgetLedger.
  await openEstimate(page, null);
  const dialog = page.getByRole("dialog", { name: "Cost estimate" });
  await expect(dialog).toBeVisible({ timeout: 10_000 });
  await expect(dialog).toContainText("5.0000 USD");
  await expect(dialog).toContainText("0.0000 USD");
  expect(await seriousViolations(page, "[role='dialog']")).toEqual([]);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect.poll(() => estimateResult(page)).toBe(false);

  // Mit Zeilen: Spalten laut AI-012, Fokusfalle, axe.
  await openEstimate(page, fixtureEstimate());
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("columnheader")).toHaveText(ESTIMATE_COLUMNS);
  const title = dialog.getByRole("row").filter({ has: page.getByRole("cell", { name: "suggest-title", exact: true }) });
  await expect(title.getByRole("cell")).toHaveText(["suggest-title", "gpt-6-luna", "low", "22", "0", "9,500", "11,264", "0.0300 / 0.0400 / 0.0500", "0.0000", "yes"]);
  expect(await seriousViolations(page, "[role='dialog']")).toEqual([]);
  await focusStaysInDialog(page);
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toHaveCount(0);
  await expect.poll(() => estimateResult(page)).toBe(false);

  // Enter ist der Standardknopf (Starten).
  await openEstimate(page, fixtureEstimate());
  await expect(dialog).toBeVisible();
  await dialog.getByRole("region", { name: "AI", exact: true }).last().focus();
  await page.keyboard.press("Enter");
  await expect(dialog).toHaveCount(0);
  await expect.poll(() => estimateResult(page)).toBe(true);

  // Der Dialog selbst startet keinen KI-Lauf.
  expect(aiCommands(sent).filter((c) => c.endsWith("Run"))).toEqual([]);
});

// spec: AI-018 [M4]
// acceptance: independent
test("Datenfluss bestätigen: Stored by Provider, axe, Escape gibt nichts frei, Einwilligung bleibt über Neuladen, Widerruf", async ({ app, page, context }) => {
  await openMain(app, page);
  const core = await probe(context, bridgeOf(page));
  await previewOn(core, page);
  await editConfig(core, (cfg) => activeQuestion(cfg));

  await openConsent(page);
  const dialog = page.getByRole("dialog", { name: "Confirm data flow" });
  await expect(dialog).toBeVisible({ timeout: 10_000 });
  const headers = await dialog.getByRole("columnheader").allTextContents();
  expect(headers).toContain("Stored by Provider");
  expect(headers.indexOf("Stored by Provider")).toBe(headers.indexOf("Training Use (per provider policy)") + 1);
  const stored = dialog.locator("[data-task='decision'] [data-col='Stored by Provider']");
  await expect(stored).not.toHaveText("");
  expect(await seriousViolations(page, "[role='dialog']")).toEqual([]);
  await focusStaysInDialog(page, 6);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect.poll(() => consentResult(page)).toBe(false);
  expect((await core.call<Json>("ai.consentStatus")).requests).toHaveLength(1);

  await openConsent(page);
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Confirm" }).click();
  await expect(dialog).toHaveCount(0);
  await expect.poll(() => consentResult(page)).toBe(true);
  expect((await core.call<Json>("ai.consentStatus")).requests).toEqual([]);

  // Neuladen des Fensters: die Einwilligung gilt weiter, kein Dialog.
  await page.reload();
  await expect(page.getByRole("navigation", { name: /^(Data views|Datenansichten)$/ })).toBeVisible();
  await openConsent(page);
  await expect.poll(() => consentResult(page), { timeout: 10_000 }).toBe(true);
  await expect(page.getByRole("dialog")).toHaveCount(0);

  // Übersicht „KI-Datenfluss“ in den Einstellungen: Zeitpunkt der Einwilligung, Widerruf je Anbieter.
  const settings = await context.newPage();
  await app.open(settings, "settings");
  await settings.getByRole("tablist", { name: "Settings sections" }).getByRole("tab", { name: "AI", exact: true }).click();
  await settings.getByRole("tab", { name: "AI data flow" }).click();
  const row = settings.locator("[data-ai-dataflow] [data-task='decision']");
  await expect(row.locator("[data-col='Consent Given At']")).not.toHaveText("");
  await expect(row.locator("[data-col='Stored by Provider']")).not.toHaveText("");
  expect(await seriousViolations(settings, "[data-ai-dataflow-overview]")).toEqual([]);
  await settings.getByRole("button", { name: "Revoke consent for OpenAI" }).click();
  await expect.poll(async () => (await core.call<Json>("ai.consentStatus")).requests.length).toBe(1);
});

// spec: AI-011 [M4]
// spec: AI-019 [M4]
// acceptance: independent
test("Konfigurationsseite KI mit Spalte Reasoning und ohne Richtlinienschalter (AI-019 erst mit 1.2)", async ({ app, page, context }) => {
  await openMain(app, page);
  const core = await probe(context, bridgeOf(page));
  await previewOn(core, page);

  await page.evaluate(async (mod) => {
    const store = await import(/* @vite-ignore */ mod.config);
    store.openConfig({});
  }, MOD);
  const cfg = page.getByRole("dialog", { name: "Crawl configuration" });
  await expect(cfg).toBeVisible({ timeout: 10_000 });
  await cfg.getByRole("navigation", { name: "Configuration pages" }).getByRole("button", { name: "AI", exact: true }).click();
  const table = cfg.getByRole("region", { name: "Model choice per task" }).last();
  await expect(table).toBeVisible({ timeout: 10_000 });
  await expect(table.getByRole("columnheader", { name: "Reasoning", exact: true })).toBeVisible();
  await expect(table.getByRole("rowheader", { name: "suggest-title", exact: true })).toBeVisible();
  expect(await seriousViolations(page, "[role='dialog']")).toEqual([]);
  // AI-019 gehört zu 1.2: in 1.1 ist kein Richtlinienschalter sichtbar (Entscheidung des Orchestrators 26.11.2026).
  await expect(cfg.getByRole("radiogroup", { name: "Where may data go?" })).toHaveCount(0);
  await expect(cfg.getByText("Enforce EU processing")).toHaveCount(0);
  await expect(cfg.getByText("Local only", { exact: true })).toHaveCount(0);
});

// spec: INTEG-005 [M4]
// acceptance: independent
test("Erweitert: Ollama Standard 1, übrige KI-Anbieter 4, Bereich 1–50, Eingaben außerhalb abgelehnt", async ({ app, page, context }) => {
  await openMain(app, page);
  const core = await probe(context, bridgeOf(page));
  await previewOn(core, page);
  const settings = await context.newPage();
  await app.open(settings, "settings");
  await settings.getByRole("tablist", { name: "Settings sections" }).getByRole("tab", { name: "AI", exact: true }).click();
  await settings.getByRole("tab", { name: "Advanced", exact: true }).click();
  const ollama = settings.getByRole("textbox", { name: "Concurrent requests to Ollama" });
  await expect(ollama).toHaveValue("1");
  await expect(settings.getByText("Default: 1 · range 1–50")).toBeVisible();
  for (const p of ["Anthropic", "OpenAI", "OpenRouter"]) {
    await expect(settings.getByRole("textbox", { name: `Concurrent requests to ${p}` })).toHaveValue("4");
  }
  expect(await seriousViolations(settings, "[data-ai-area='settings']")).toEqual([]);

  const ollamaRow = settings.locator("[data-ai-advanced-row][data-provider='ollama']");
  const stored = async () => ((await core.call<Json>("settings.get")).aiApp?.providerEndpoints?.ollama?.concurrentRequests ?? 1) as number;
  for (const bad of ["51", "0", "zwei"]) {
    await ollama.fill(bad);
    await ollamaRow.getByRole("button", { name: "Apply" }).click();
    await expect(ollamaRow.getByText("Please enter a whole number from 1 to 50.")).toBeVisible();
    expect(await stored(), `Eingabe ${bad} abgelehnt`).toBe(1);
  }
  await ollama.fill("50");
  await ollamaRow.getByRole("button", { name: "Apply" }).click();
  await expect.poll(stored).toBe(50);
});

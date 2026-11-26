// Unabhängige Abnahme M4-U01 (Oberfläche), nur aus der Spec abgeleitet: UI-013 [M4] (Feld `Kosten`: Format de/en,
// Sprachwechsel ohne Neustart, Tooltip mit Aufteilung, Sichtbarkeit nur bei aktiven KI-Aufgaben inkl. Embeddings und
// eigener Prompts, Warnschwelle), AI-012 (Dialog „Kostenvoranschlag“: Spalten, Abbrechen sendet nichts), AI-018 (Dialog
// „Datenfluss bestätigen“), AI-011 (Hinweis „Ausgabegrenze knapp“), Schalter „KI (Vorschau)“ aus, de/en vollständig.
// Fixture: MockCore mit Handlern für `config.get`, `ai.costBudget`, `ai.costEstimate`, `ai.consentStatus`, `ai.consentGrant`.
// acceptance: independent
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { i18n, loadFtlFiles, makeT, setLanguage } from "../../i18n";
import { checkCatalog, checkIdenticalGerman } from "../../i18n/check";
import { createClient, setClient } from "../../ipc/client";
import { MockCore } from "../../ipc/transport/mock";
import { usePrefs } from "../../state/prefs";
import { DEFAULT_APPEARANCE } from "../../ui/tokens/theme";
import { resolvePages } from "../config/frame/registry";
import { resolveSections } from "../settings/frame/registry";
import { requestConsent, useConsentDialog } from "./consent/ConsentDialog";
import { AiCostField } from "./cost/CostField";
import { AiDialogHost } from "./cost/DialogHost";
import { requestCostConfirmation, showCostEstimate, useCostEstimateDialog } from "./cost/EstimateDialog";
import { setAiPreview } from "./preview";
import { TaskTableView } from "./settings/TaskTable";
import type { CostEstimateView, ConsentView, TaskRow } from "./settings/api";

/** Spalten des Abschnitts „KI“ laut AI-012 mit `Reasoning` zwischen `Model` und `Items` (Nachtrag 24.11.2026). */
const ESTIMATE_COLUMNS = [
  "Task",
  "Model",
  "Reasoning",
  "Items",
  "Cached",
  "Est. Input Tokens",
  "Est. Output Tokens",
  "Est. Cost (low/expected/high)",
  "Batch Savings",
  "Executable",
];

/** Spalten der Datenflusstabelle laut AI-018 mit `Stored by Provider` (Nachtrag 24.11.2026). */
const FLOW_COLUMNS = [
  "Task",
  "Provider",
  "Model",
  "Host",
  "Region",
  "Fields Sent",
  "Max Chars",
  "Images",
  "Redaction",
  "Training Use (per provider policy)",
  "Stored by Provider",
  "Consent Given At",
];

let core: MockCore;
let aiConfig: Record<string, unknown>;
let snapshot: Record<string, unknown>;

const config = () => ({ schemaVersion: 2, generator: "u01-acceptance", config: aiConfig });
const activeAi = { ai: { enabled: true, active_questions: ["page-type"], questions: [], prompts: [], suggestions: {}, budget: { warning_percent: 80 } } };

beforeEach(async () => {
  core = new MockCore();
  aiConfig = structuredClone(activeAi);
  snapshot = { spentMicroUsd: 3_210_000, budgetMicroUsd: 5_000_000, aiMicroUsd: 3_210_000, dataForSeoMicroUsd: 0, reservedMicroUsd: 0 };
  core.handlers.set("config.get", () => config());
  core.handlers.set("ai.costBudget", () => snapshot);
  setClient(createClient(core.transport("main")));
  usePrefs.setState({ loaded: false, dto: null, appearance: DEFAULT_APPEARANCE, language: "en", languageSetting: "system" });
  await usePrefs.getState().load();
  await act(async () => {
    await setLanguage("en");
  });
});

afterEach(async () => {
  cleanup();
  useCostEstimateDialog.setState({ view: null, resolve: null });
  useConsentDialog.setState({ view: null, resolve: null, error: null });
  setClient(null);
  await act(async () => {
    await setLanguage("en");
  });
});

async function previewOn() {
  await act(async () => {
    await setAiPreview(true);
  });
}

const aiCalls = () => core.calls.filter((c) => c.cmd.startsWith("ai."));
const configReads = () => core.calls.filter((c) => c.cmd === "config.get").length;

async function field(): Promise<HTMLElement> {
  return waitFor(() => {
    const el = screen.getByTestId("ai-cost");
    expect(el.textContent).not.toMatch(/–/);
    return el;
  });
}

describe("UI-013 [M4] Feld Kosten", () => {
  // spec: UI-013 [M4]
  // acceptance: independent
  it("zeigt 3,21 von 5 USD als „Kosten: 3,2100 / 5,0000 USD“ bzw. „Cost: 3.2100 / 5.0000 USD“; Sprachwechsel ohne Neustart", async () => {
    await previewOn();
    render(<AiCostField />);
    const el = await field();
    expect(el.textContent).toBe("Cost: 3.2100 / 5.0000 USD");
    await act(async () => {
      await setLanguage("de");
    });
    expect(screen.getByTestId("ai-cost")).toBe(el);
    expect(el.textContent).toBe("Kosten: 3,2100 / 5,0000 USD");
    await act(async () => {
      await setLanguage("en");
    });
    expect(el.textContent).toBe("Cost: 3.2100 / 5.0000 USD");
  });

  // spec: UI-013 [M4]
  // acceptance: independent
  it("Tooltip mit Aufteilung KI / DataForSEO / offene Reservierungen im selben Zahlenformat", async () => {
    snapshot = { spentMicroUsd: 3_460_000, budgetMicroUsd: 5_000_000, aiMicroUsd: 3_210_000, dataForSeoMicroUsd: 0, reservedMicroUsd: 250_000 };
    await previewOn();
    await act(async () => {
      await setLanguage("de");
    });
    render(<AiCostField />);
    const el = await field();
    const lines = (el.getAttribute("title") ?? "").split("\n");
    expect(lines).toContain("KI: 3,2100 USD");
    expect(lines).toContain("DataForSEO: 0,0000 USD");
    expect(lines).toContain("Offene Reservierungen: 0,2500 USD");
    await act(async () => {
      await setLanguage("en");
    });
    const en = (screen.getByTestId("ai-cost").getAttribute("title") ?? "").split("\n");
    expect(en).toHaveLength(3);
    expect(en.some((l) => l.includes("3.2100 USD"))).toBe(true);
    expect(en).toContain("DataForSEO: 0.0000 USD");
    expect(en.some((l) => l.includes("0.2500 USD"))).toBe(true);
    expect(en.join("\n")).not.toMatch(/\d,\d{4}/);
  });

  // spec: UI-013 [M4]
  // acceptance: independent
  it("bei unbegrenztem Budget „Kosten: 3,2100 USD (unbegrenzt)“", async () => {
    snapshot = { ...snapshot, budgetMicroUsd: null };
    await previewOn();
    await act(async () => {
      await setLanguage("de");
    });
    render(<AiCostField />);
    expect((await field()).textContent).toBe("Kosten: 3,2100 USD (unbegrenzt)");
  });

  // spec: UI-013 [M4]
  // spec: AI-013 [M4]
  // acceptance: independent
  it("Warnung bei der Warnschwelle (Standard 80 % des Crawl-Budgets, einstellbar)", async () => {
    await previewOn();
    snapshot = { ...snapshot, spentMicroUsd: 4_000_000, aiMicroUsd: 4_000_000 };
    const { unmount } = render(<AiCostField />);
    await waitFor(() => expect(screen.getByTestId("ai-cost")).toHaveAttribute("data-warning", "true"));
    unmount();
    snapshot = { ...snapshot, spentMicroUsd: 3_210_000, aiMicroUsd: 3_210_000 };
    const second = render(<AiCostField />);
    const el = await field();
    expect(el).not.toHaveAttribute("data-warning");
    second.unmount();
    aiConfig = { ai: { ...activeAi.ai, budget: { warning_percent: 50 } } };
    render(<AiCostField />);
    await waitFor(() => expect(screen.getByTestId("ai-cost")).toHaveAttribute("data-warning", "true"));
  });

  // spec: UI-013 [M4]
  // acceptance: independent
  it("nur bei aktiven KI-Aufgaben sichtbar – auch Embeddings und eigene Prompts zählen (Nutzerentscheidung 24.11.2026)", async () => {
    await previewOn();
    const cases: { name: string; cfg: Record<string, unknown>; visible: boolean }[] = [
      { name: "keine KI-Aufgabe", cfg: { ai: { enabled: false, active_questions: [], prompts: [], suggestions: {} }, embeddings: { enabled: false } }, visible: false },
      { name: "Embeddings", cfg: { ai: { enabled: false, active_questions: [], prompts: [], suggestions: {} }, embeddings: { enabled: true } }, visible: true },
      { name: "eigener Prompt", cfg: { ai: { enabled: false, active_questions: [], prompts: [{ id: "p1", name: "Eigener Prompt", enabled: true }], suggestions: {} }, embeddings: { enabled: false } }, visible: true },
      { name: "aktive Frage", cfg: { ai: { enabled: true, active_questions: ["page-type"], prompts: [], suggestions: {} } }, visible: true },
      { name: "Titelvorschläge", cfg: { ai: { enabled: true, active_questions: [], prompts: [], suggestions: { title: true } } }, visible: true },
    ];
    for (const c of cases) {
      aiConfig = c.cfg;
      const reads = configReads();
      const { unmount } = render(<AiCostField />);
      await waitFor(() => expect(configReads()).toBeGreaterThan(reads));
      if (c.visible) await waitFor(() => expect(screen.getByTestId("ai-cost")).toBeInTheDocument(), { timeout: 3000 });
      else {
        await act(async () => {
          await Promise.resolve();
        });
        expect(screen.queryByTestId("ai-cost"), c.name).toBeNull();
      }
      unmount();
    }
  });
});

describe("Schalter „KI (Vorschau)“ aus", () => {
  // spec: UI-013 [M4]
  // spec: AI-012 [M4]
  // spec: AI-018 [M4]
  // acceptance: independent
  it("kein Feld Kosten, kein Dialog, kein KI-Abschnitt, keine Seite KI und kein Aufruf von ai.*", async () => {
    render(<AiCostField />);
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.queryByTestId("ai-cost")).toBeNull();
    void showCostEstimate(estimateView());
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(resolveSections().map((s) => s.id)).not.toContain("ai");
    expect(resolvePages().map((p) => p.id)).not.toContain("ai");
    expect(aiCalls()).toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------------------------------
// AI-012 Dialog „Kostenvoranschlag“

function estimateRow(task: string, over: Record<string, unknown> = {}) {
  return {
    task,
    model: "gpt-6-luna",
    reasoning: "low",
    items: 22,
    cached: 3,
    estInputTokens: 9500,
    estOutputTokens: 11264,
    lowMicroUsd: 30_000,
    expectedMicroUsd: 40_000,
    highMicroUsd: 50_000,
    batchSavingsMicroUsd: 20_000,
    executable: true,
    reason: "",
    calibrated: false,
    requests: 22,
    ...over,
  };
}

function estimateView(over: Partial<CostEstimateView> = {}): CostEstimateView {
  const totals = { lowMicroUsd: 30_000, expectedMicroUsd: 40_000, highMicroUsd: 50_000 };
  return {
    sections: [
      {
        id: "ai",
        rows: [
          estimateRow("suggest-title"),
          estimateRow("decision", { reasoning: null, model: "gpt-6-luna" }),
          estimateRow("ask-sql", { model: "claude-sonnet-5-5", reasoning: "medium", executable: false, reason: "Missing API key", lowMicroUsd: null, expectedMicroUsd: null, highMicroUsd: null, batchSavingsMicroUsd: null }),
        ],
        totals,
      },
    ],
    totals,
    budget: { budgetMicroUsd: 5_000_000, consumedMicroUsd: 1_000_000, remainingMicroUsd: 4_000_000, warning: false, halted: false },
    thresholdMicroUsd: 1_000_000,
    confirmationRequired: true,
    ...over,
  };
}

async function openEstimate(view = estimateView()) {
  render(<AiDialogHost />);
  let result: Promise<boolean> = Promise.resolve(false);
  await act(async () => {
    result = showCostEstimate(view);
  });
  const dialog = await screen.findByRole("dialog");
  return { dialog, result };
}

describe("AI-012 Dialog „Kostenvoranschlag“", () => {
  // spec: AI-012 [M4]
  // acceptance: independent
  it("Titel und Spalten des Abschnitts KI in der Reihenfolge der Spec, mit Reasoning zwischen Model und Items", async () => {
    await act(async () => {
      await setLanguage("de");
    });
    const { dialog } = await openEstimate();
    expect(screen.getByRole("dialog", { name: "Kostenvoranschlag" })).toBe(dialog);
    const headers = within(dialog).getAllByRole("columnheader").map((h) => h.textContent);
    expect(headers).toEqual(ESTIMATE_COLUMNS);
  });

  // spec: AI-012 [M4]
  // acceptance: independent
  it("zeigt die Zahlen des Kern-Voranschlags mit 4 Nachkommastellen; nicht ausführbare Aufgaben mit Grund; Budget, Verbrauch, Rest", async () => {
    const { dialog } = await openEstimate();
    const rows = within(dialog).getAllByRole("row");
    const cells = (task: string) => {
      const r = rows.find((row) => within(row).queryAllByRole("cell")[0]?.textContent === task);
      expect(r, task).toBeDefined();
      return within(r!).getAllByRole("cell").map((c) => c.textContent);
    };
    expect(cells("suggest-title")).toEqual(["suggest-title", "gpt-6-luna", "low", "22", "3", "9,500", "11,264", "0.0300 / 0.0400 / 0.0500", "0.0200", "yes"]);
    expect(cells("decision")[2]).toBe("");
    const missing = cells("ask-sql");
    expect(missing[2]).toBe("medium");
    expect(missing[9]).toContain("Missing API key");
    expect(missing[9]).toMatch(/^no/);
    const text = dialog.textContent ?? "";
    expect(text).toContain("5.0000 USD");
    expect(text).toContain("1.0000 USD");
    expect(text).toContain("4.0000 USD");
  });

  // spec: AI-012 [M4]
  // acceptance: independent
  it("Abbrechen schließt ohne Freigabe und sendet nichts; Starten gibt frei", async () => {
    const first = await openEstimate();
    const before = core.calls.length;
    fireEvent.click(within(first.dialog).getByRole("button", { name: "Cancel" }));
    await expect(first.result).resolves.toBe(false);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(core.calls.slice(before)).toEqual([]);
    cleanup();
    const second = await openEstimate();
    fireEvent.click(within(second.dialog).getByRole("button", { name: "Start" }));
    await expect(second.result).resolves.toBe(true);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  // spec: AI-012 [M4]
  // acceptance: independent
  it("Bestätigungsregel: ohne verlangte Bestätigung kein Dialog, sonst Dialog vor dem Lauf", async () => {
    render(<AiDialogHost />);
    core.handlers.set("ai.costEstimate", () => estimateView({ confirmationRequired: false }));
    let go: boolean | null = null;
    await act(async () => {
      go = await requestCostConfirmation();
    });
    expect(go).toBe(true);
    expect(screen.queryByRole("dialog")).toBeNull();

    core.handlers.set("ai.costEstimate", () => estimateView({ confirmationRequired: true }));
    let pending: Promise<boolean> = Promise.resolve(true);
    await act(async () => {
      pending = requestCostConfirmation();
    });
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await expect(pending).resolves.toBe(false);
  });
});

// ---------------------------------------------------------------------------------------------------------------
// AI-018 Dialog „Datenfluss bestätigen“

function consentView(over: Partial<ConsentView> = {}): ConsentView {
  const key = { provider: "openai", task: "suggest-title", fields: ["h1", "meta_description", "page_text", "title", "url"] };
  const cells = ["suggest-title", "openai", "gpt-6-luna", "api.openai.com", "", "URL, Title, Meta Description, H1, Page Text", "8000", "no", "off", "no", "no (store: false)", ""];
  const row = { key, cells, piiNotice: true, consentGiven: false };
  return {
    columns: [...FLOW_COLUMNS],
    rows: [row],
    requests: [{ row, previousFields: null, addedFields: key.fields, piiNotice: true }],
    redaction: false,
    policy: "unrestricted",
    consentedProviders: [],
    csv: `${FLOW_COLUMNS.join(",")}\n`,
    ...over,
  };
}

describe("AI-018 Dialog „Datenfluss bestätigen“", () => {
  // spec: AI-018 [M4]
  // acceptance: independent
  it("zeigt die Datenflusstabelle mit Stored by Provider, den Stand der Schwärzung und bei Schwärzung aus den PII-Hinweis", async () => {
    core.handlers.set("ai.consentStatus", () => consentView());
    await act(async () => {
      await setLanguage("de");
    });
    render(<AiDialogHost />);
    let pending: Promise<boolean> = Promise.resolve(false);
    await act(async () => {
      pending = requestConsent();
    });
    const dialog = await screen.findByRole("dialog", { name: "Datenfluss bestätigen" });
    expect(within(dialog).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(FLOW_COLUMNS);
    expect(within(dialog).getByText("no (store: false)")).toBeInTheDocument();
    expect(dialog.textContent).toMatch(/Schwärzung[^\n]*aus/);
    expect(within(dialog).getByRole("note")).toHaveTextContent(/Kontakt/);
    expect(within(dialog).getByRole("note")).toHaveTextContent(/Impressum/);
    fireEvent.click(within(dialog).getByRole("button", { name: "Abbrechen" }));
    await expect(pending).resolves.toBe(false);
    expect(core.calls.some((c) => c.cmd === "ai.consentGrant")).toBe(false);
  });

  // spec: AI-018 [M4]
  // acceptance: independent
  it("ohne PII-Hinweis bei Schwärzung an; Bestätigen speichert genau die gezeigten Zeilen", async () => {
    const view = consentView({ redaction: true });
    view.requests[0].piiNotice = false;
    view.requests[0].row.piiNotice = false;
    core.handlers.set("ai.consentStatus", () => view);
    core.handlers.set("ai.consentGrant", () => ({ ...view, requests: [] }));
    render(<AiDialogHost />);
    let pending: Promise<boolean> = Promise.resolve(false);
    await act(async () => {
      pending = requestConsent();
    });
    const dialog = await screen.findByRole("dialog", { name: "Confirm data flow" });
    expect(within(dialog).queryByRole("note")).toBeNull();
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));
    await expect(pending).resolves.toBe(true);
    const grants = core.calls.filter((c) => c.cmd === "ai.consentGrant");
    expect(grants).toHaveLength(1);
    expect(grants[0].args).toEqual({ grant: [view.requests[0].row.key] });
  });

  // spec: AI-018 [M4]
  // acceptance: independent
  it("ist schon eingewilligt, erscheint kein Dialog", async () => {
    core.handlers.set("ai.consentStatus", () => consentView({ requests: [] }));
    render(<AiDialogHost />);
    let go: boolean | null = null;
    await act(async () => {
      go = await requestConsent();
    });
    expect(go).toBe(true);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

// ---------------------------------------------------------------------------------------------------------------
// AI-011 Hinweise der Modellwahl

function taskRow(over: Partial<TaskRow> = {}): TaskRow {
  const luna = { provider: "openai", model: "gpt-6-luna", endpoint: "responses" as const };
  return {
    task: "suggest-title",
    model: null,
    fallback: [],
    defaultModel: luna,
    defaultFallback: [],
    isDefault: false,
    effectiveModel: luna,
    effectiveFallback: [],
    models: [],
    reasoning: { stored: null, requested: "off", effective: "low", dropdown: ["model-default", "low", "medium", "high"], warning: { kind: "off-unavailable", effective: "low" }, levelsKnown: true },
    maxTokens: { user: 500, standard: 2048, value: 500, tight: true, answerTokens: 512, decisionFormula: false, modelLimit: null },
    temperature: null,
    maxInputChars: null,
    status: null,
    plannedModel: luna,
    assignmentError: null,
    ...over,
  };
}

describe("AI-011 Modellwahl je Aufgabe", () => {
  // spec: AI-011 [M4]
  // acceptance: independent
  it("Tabelle mit Spalte Reasoning; knappe Ausgabegrenze zeigt den Hinweis der Spec wörtlich; Warnung bei nicht verfügbarer Stufe", async () => {
    await act(async () => {
      await setLanguage("de");
    });
    render(<TaskTableView rows={[taskRow()]} onSetTask={() => undefined} onReset={() => undefined} />);
    const headers = screen.getAllByRole("columnheader").map((h) => h.textContent ?? "");
    expect(headers.some((h) => /Reasoning/.test(h))).toBe(true);
    expect(screen.getByText("Ausgabegrenze knapp – Antworten können abbrechen (Error: Output limit reached)")).toBeInTheDocument();
    const notes = screen.getAllByRole("note").map((n) => n.textContent ?? "");
    expect(notes.length).toBeGreaterThanOrEqual(2);
  });
});

// ---------------------------------------------------------------------------------------------------------------
// de/en vollständig

const sources = import.meta.glob<string>("./{settings,cost,consent}/*.tsx", { query: "?raw", import: "default", eager: true });

describe("de/en", () => {
  // spec: UI-121
  // spec: AI-012 [M4]
  // spec: AI-018 [M4]
  // acceptance: independent
  it("jeder Schlüssel der neuen Bereiche ist in de und en übersetzt, ohne unübersetzte Kopien", () => {
    const files = loadFtlFiles().filter((f) => /^features\/ai\/(settings|cost|consent)$/.test(f.feature));
    expect(files.length).toBeGreaterThanOrEqual(6);
    expect(checkCatalog(files).problems).toEqual([]);
    expect(checkIdenticalGerman(files, new Set(["KI", "AI", "OpenAI", "Anthropic", "OpenRouter", "Ollama"]))).toEqual([]);

    const keys = new Set<string>();
    for (const [path, src] of Object.entries(sources)) {
      if (path.endsWith(".test.tsx")) continue;
      for (const m of src.matchAll(/\bt\(\s*"([a-z0-9-]+)"/g)) keys.add(m[1]!);
      for (const m of src.matchAll(/titleKey:\s*"([a-z0-9-]+)"/g)) keys.add(m[1]!);
    }
    for (const l of ["model-default", "off", "minimal", "low", "medium", "high", "xhigh", "max"]) keys.add(`ai-level-${l}`);
    for (const f of ["url", "title", "meta-description", "h1", "h2", "page-text", "word-count", "html-lang", "hreflang-self", "structured-data-types", "page-type", "image", "alt", "image-context"]) keys.add(`ai-field-${f}`);
    expect(keys.size).toBeGreaterThan(40);
    const missing = [...keys].filter((k) => !i18n.has("de", k) || !i18n.has("en", k));
    expect(missing).toEqual([]);
    const de = makeT("de");
    const en = makeT("en");
    expect(de("ai-estimate-title")).toBe("Kostenvoranschlag");
    expect(de("ai-consent-dialog-title")).toBe("Datenfluss bestätigen");
    expect(en("ai-estimate-title")).not.toBe(de("ai-estimate-title"));
  });
});

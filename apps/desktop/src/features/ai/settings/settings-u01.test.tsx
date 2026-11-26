// M4-U01: Modellwahl je Aufgabe mit Spalte „Reasoning“ (AI-011, Nachtrag 24.11.2026), Richtlinie und Crawl-Budget (AI-019,
// AI-013) im Entwurf des Konfigurationsdialogs, App-Einstellungen KI mit EU-Endpunkten, Schwärzungsmustern und Reiter
// „Erweitert“ (INTEG-005 [M4]). Fixtures: Zeilen in der Form des Kerns (`ai_settings.rs`, Mock-Modelle A/B/D aus 08) und eine
// `ai.settingsGet`-Antwort mit den App-Vorgaben.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { IpcError, localError } from "../../../ipc/errors";
import { MockCore } from "../../../ipc/transport/mock";
import { usePrefs } from "../../../state/prefs";
import { DEFAULT_APPEARANCE } from "../../../ui/tokens/theme";
import { ConfigProvider, type ConfigContextValue } from "../../config/frame/context";
import { resolvePages } from "../../config/frame/registry";
import { resolveSections } from "../../settings/frame/registry";
import { setAiPreview } from "../preview";
import { AiSettingsSection } from "./AiSettingsSection";
import { AI_019_ENABLED, type SettingsView, type TaskRow } from "./api";
import { aiConfigPages } from "./index";
import { BudgetSetting, PolicySetting } from "./PolicyBudget";
import { patchTasks, resetTask, TaskTableView, warningText } from "./TaskTable";
import { makeT } from "../../../i18n";

const A = { provider: "anthropic", model: "mock-claude", endpoint: "messages" } as const;
const B = { provider: "anthropic", model: "mock-claude-b", endpoint: "messages" } as const;
const LUNA = { provider: "openai", model: "gpt-6-luna", endpoint: "responses" } as const;

function taskRow(over: Partial<TaskRow>): TaskRow {
  return {
    task: "suggest-title",
    model: null,
    fallback: [],
    defaultModel: LUNA,
    defaultFallback: [{ provider: "anthropic", model: "claude-haiku-5-5", endpoint: "messages" }],
    isDefault: true,
    effectiveModel: LUNA,
    effectiveFallback: [{ provider: "anthropic", model: "claude-haiku-5-5", endpoint: "messages" }],
    models: [
      { ...A, source: "live", capabilitiesKnown: true, assignable: true, missing: [], reasoningLevels: ["off", "low", "medium", "high", "max"], maxOutputTokens: 8192 },
      { ...B, source: "live", capabilitiesKnown: true, assignable: true, missing: [], reasoningLevels: ["low", "medium", "high", "xhigh"], maxOutputTokens: 4000 },
      { provider: "anthropic", model: "mock-claude-d", endpoint: "messages", source: "live", capabilitiesKnown: true, assignable: false, missing: ["structured_output"], reasoningLevels: [], maxOutputTokens: null },
      { ...LUNA, source: "catalog", capabilitiesKnown: true, assignable: true, missing: [], reasoningLevels: ["off", "low", "medium", "high", "xhigh", "max"], maxOutputTokens: 128000 },
    ],
    reasoning: { stored: null, requested: "off", effective: "off", dropdown: ["model-default", "off", "low", "medium", "high", "xhigh", "max"], warning: null, levelsKnown: true },
    maxTokens: { user: null, standard: 1024, value: 1024, tight: false, answerTokens: 512, decisionFormula: false, modelLimit: 128000 },
    temperature: null,
    maxInputChars: null,
    status: "Error: Missing API key",
    plannedModel: null,
    assignmentError: null,
    ...over,
  };
}

/** `suggest-title` auf Modell B: effektive Stufe `low`, Warnung „aus nicht verfügbar“, `max_tokens = 2048` (AI-011, Prüfwert). */
const ON_B = taskRow({
  model: B,
  isDefault: false,
  effectiveModel: B,
  fallback: [A, LUNA],
  effectiveFallback: [A, LUNA],
  reasoning: { stored: null, requested: "off", effective: "low", dropdown: ["model-default", "low", "medium", "high", "xhigh"], warning: { kind: "off-unavailable", effective: "low" }, levelsKnown: true },
  maxTokens: { user: 500, standard: 2048, value: 500, tight: true, answerTokens: 512, decisionFormula: false, modelLimit: 4000 },
  status: null,
  plannedModel: B,
});

const DECISION = taskRow({ task: "decision", reasoning: null, maxTokens: null, effectiveModel: { provider: "openai", model: "gpt-6-luna", endpoint: "decisions" } });

let core: MockCore;
beforeEach(async () => {
  core = new MockCore();
  setClient(createClient(core.transport("settings-u01")));
  usePrefs.setState({ loaded: false, dto: null, appearance: DEFAULT_APPEARANCE, language: "en", languageSetting: "system" });
  await usePrefs.getState().load();
});
afterEach(async () => {
  cleanup();
  setClient(null);
  await act(async () => {
    await setLanguage("en");
  });
});

describe("Tabelle Aufgabe → Modell", () => {
  // spec: AI-011 [M4]
  it("hat die Spalte Reasoning mit effektiver Stufe, Warnung und Hinweis „knapp“; Decisions ohne Stufe", () => {
    const { container } = render(<TaskTableView rows={[ON_B, DECISION]} onSetTask={() => undefined} onReset={() => undefined} />);
    expect(screen.getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Task", "Model", "Reasoning", "Output limit", "Fallback list", "Status", "Actions"]);
    const title = container.querySelector('tr[data-task="suggest-title"]')!;
    expect(title.querySelector("[data-ai-effective]")!.getAttribute("data-ai-effective")).toBe("low");
    expect(title.querySelector('[data-ai-warning="off-unavailable"]')!.textContent).toBe("Switching off is not available for this model – the lowest level applies: low.");
    expect(title.querySelector("[data-ai-tight]")!.textContent).toBe("Output limit tight – answers may stop (Error: Output limit reached)");
    expect(title.querySelector("[data-ai-unassignable]")!.textContent).toBe("Not assignable: mock-claude-d (structured_output)");
    const decision = container.querySelector('tr[data-task="decision"]')!;
    expect(decision.querySelector('[data-col="Reasoning"]')!.textContent).toBe("no level");
    expect(decision.querySelector('[data-col="Status"]')!.textContent).toBe("Error: Missing API key");
  });

  // spec: AI-011 [M4]
  it("das Reasoning-Dropdown bietet nur die Stufen des Modells und model-default (Modell B ohne off)", async () => {
    render(<TaskTableView rows={[ON_B]} onSetTask={() => undefined} onReset={() => undefined} />);
    fireEvent.click(screen.getByRole("button", { name: /Reasoning level for suggest-title/ }));
    const list = await screen.findByRole("listbox");
    expect(within(list).getAllByRole("option").map((o) => o.textContent)).toEqual(["Task default (off)", "model default", "low", "medium", "high", "xhigh"]);
  });

  // spec: AI-011 [M4]
  it("nicht zuweisbare Modelle stehen nicht im Modell-Dropdown", async () => {
    render(<TaskTableView rows={[ON_B]} onSetTask={() => undefined} onReset={() => undefined} />);
    fireEvent.click(screen.getByRole("button", { name: /Model for suggest-title/ }));
    const list = await screen.findByRole("listbox");
    const options = within(list).getAllByRole("option").map((o) => o.textContent);
    expect(options).not.toContain("anthropic · mock-claude-d (messages)");
    expect(options).toContain("anthropic · mock-claude-b (messages)");
  });

  // spec: AI-011 [M4]
  it("Ersatzliste umsortieren (Tastatur), Stufe setzen und Auf Standard zurücksetzen schreiben den Entwurf", () => {
    const set = vi.fn();
    const reset = vi.fn();
    render(<TaskTableView rows={[ON_B]} onSetTask={set} onReset={reset} />);
    fireEvent.click(screen.getByRole("button", { name: "Move gpt-6-luna up" }));
    expect(set).toHaveBeenLastCalledWith("suggest-title", { model: B, fallback: [LUNA, A] });
    fireEvent.click(screen.getByRole("button", { name: "Remove mock-claude" }));
    expect(set).toHaveBeenLastCalledWith("suggest-title", { model: B, fallback: [LUNA] });
    fireEvent.click(screen.getByRole("button", { name: "Reset to default" }));
    expect(reset).toHaveBeenCalledWith("suggest-title");
    // Form im Profil (`TaskModelConfig`): Stufe im Feld `effort`, Zurücksetzen entfernt den Eintrag.
    const tasks = patchTasks({}, "suggest-title", { effort: "medium" });
    expect(tasks).toEqual({ "suggest-title": { model: null, fallback: [], max_tokens: null, temperature: null, effort: "medium", max_input_chars: null } });
    expect(resetTask(tasks, "suggest-title")).toEqual({});
  });

  // spec: AI-011 [M4]
  it("Warntexte de/en für gespeicherte, nicht verfügbare Stufe", () => {
    const row = taskRow({ reasoning: { stored: "xhigh", requested: "xhigh", effective: "high", dropdown: ["model-default", "off", "high"], warning: { kind: "stored-unavailable", stored: "xhigh", effective: "high" }, levelsKnown: true } });
    expect(warningText(makeT("de"), row)).toBe("Die gespeicherte Stufe sehr hoch ist für dieses Modell nicht verfügbar; es gilt hoch. Der gespeicherte Wert bleibt erhalten.");
    expect(warningText(makeT("en"), row)).toBe("The stored level xhigh is not available for this model; high applies. The stored value is kept.");
  });
});

function draftContext(values: Record<string, unknown>) {
  const set = vi.fn((path: string, v: unknown) => {
    values[path] = v;
  });
  const draft = { get: (path: string) => values[path], set };
  return { value: { draft } as unknown as ConfigContextValue, set };
}

describe("Richtlinie und Crawl-Budget im Entwurf", () => {
  // spec: AI-019
  it.skip("wartet auf M4-K07 (1.2): AI-019 – drei Richtlinien, Standard „Keine Einschränkung“; Auswahl schreibt ai.policy", () => {
    const ctx = draftContext({});
    render(<ConfigProvider value={ctx.value}><PolicySetting /></ConfigProvider>);
    const radios = screen.getAllByRole("radio").map((r) => r.closest("label")!.textContent);
    expect(radios).toEqual(["No restriction", "Enforce EU processing", "Local only"]);
    expect(screen.getByRole("radio", { name: "No restriction" })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: "Local only" }));
    expect(ctx.set).toHaveBeenCalledWith("ai.policy", "local-only");
  });

  // spec: UI-013 [M4]
  it("Crawl-Budget 5,00 USD (E-5 a), Unbegrenzt nur mit Warnhinweis, Eingabe in der UI-Sprache", async () => {
    await act(async () => {
      await setLanguage("de");
    });
    const ctx = draftContext({ "ai.budget": { crawl: 5_000_000, unlimited: false, areas: {}, tasks: {}, warning_percent: 80 } });
    const { container } = render(<ConfigProvider value={ctx.value}><BudgetSetting /></ConfigProvider>);
    const crawl = screen.getByRole("textbox", { name: "Budget je Crawl (USD)" });
    expect(crawl).toHaveValue("5,0000");
    fireEvent.change(crawl, { target: { value: "7,50" } });
    expect(ctx.set).toHaveBeenLastCalledWith("ai.budget", expect.objectContaining({ crawl: 7_500_000 }));
    expect(container.querySelector("[data-ai-unlimited-warning]")).toBeNull();
    fireEvent.click(screen.getByRole("checkbox", { name: "Unbegrenzt" }));
    expect(ctx.set).toHaveBeenLastCalledWith("ai.budget", expect.objectContaining({ unlimited: true }));
  });
});

const VIEW: SettingsView = {
  previewEnabled: true,
  keys: { anthropic: "missing", openai: "missing", openrouter: "missing", ollama: "local" },
  listsLoaded: { anthropic: false, openai: true, openrouter: false, ollama: false },
  tasks: [],
  policy: "unrestricted",
  policyAvailable: false,
  budget: { crawl: 5_000_000, unlimited: false, areas: {}, tasks: {}, warning_percent: 80 },
  crawlBudget: null,
  eu: [
    { provider: "openai", euBaseUrl: "https://eu.api.openai.com", useEuEndpoint: false, euAccountConfirmed: false, allowedUnderEu: false },
    { provider: "openrouter", euBaseUrl: "https://eu.openrouter.ai/api/v1", useEuEndpoint: false, euAccountConfirmed: false, allowedUnderEu: false },
  ],
  advanced: [
    { provider: "anthropic", concurrentDefault: 4, concurrentMin: 1, concurrentMax: 50, concurrent: 4, timeoutDefault: 120, timeout: 120 },
    { provider: "openai", concurrentDefault: 4, concurrentMin: 1, concurrentMax: 50, concurrent: 4, timeoutDefault: 120, timeout: 120 },
    { provider: "openrouter", concurrentDefault: 4, concurrentMin: 1, concurrentMax: 50, concurrent: 4, timeoutDefault: 120, timeout: 120 },
    { provider: "ollama", concurrentDefault: 1, concurrentMin: 1, concurrentMax: 50, concurrent: 1, timeoutDefault: 300, timeout: 300 },
  ],
  confirmationThresholdMicroUsd: 1_000_000,
  redactPersonalData: true,
  redactionPatterns: [],
};

describe("App-Einstellungen KI", () => {
  beforeEach(() => {
    core.handlers.set("ai.settingsGet", () => VIEW);
    core.handlers.set("ai.consentStatus", () => ({ columns: [], rows: [], requests: [], redaction: true, policy: "unrestricted", consentedProviders: [], csv: "" }));
  });

  // spec: INTEG-005 [M4]
  it("Reiter Erweitert: Ollama zeigt Standard 1, alle Anbieter den Bereich 1–50; 51 wird abgelehnt", async () => {
    render(<AiSettingsSection />);
    fireEvent.click(await screen.findByRole("tab", { name: "Advanced" }));
    const ollama = await screen.findByRole("textbox", { name: "Concurrent requests to Ollama" });
    expect(ollama).toHaveValue("1");
    expect(screen.getByText("Default: 1 · range 1–50")).toBeVisible();
    expect(screen.getAllByText("Default: 4 · range 1–50")).toHaveLength(3);
    fireEvent.change(ollama, { target: { value: "51" } });
    fireEvent.click(within(ollama.closest("[data-ai-advanced-row]") as HTMLElement).getByRole("button", { name: "Apply" }));
    expect(await screen.findByText("Please enter a whole number from 1 to 50.")).toBeVisible();
    expect(core.calls.filter((c) => c.cmd === "settings.set")).toEqual([]);
  });

  // spec: AI-019
  it.skip("wartet auf M4-K07 (1.2): AI-019 – EU-Endpunkt je Anbieter mit vorbelegter URL; Schalten schreibt providerEndpoints in die App-Einstellungen", async () => {
    render(<AiSettingsSection />);
    const sw = await screen.findByRole("switch", { name: "Use the EU endpoint (https://eu.api.openai.com)" });
    fireEvent.click(sw);
    await waitFor(() => expect(core.calls.some((c) => c.cmd === "settings.set")).toBe(true));
    const patch = core.calls.find((c) => c.cmd === "settings.set")!.args as { patch: { aiApp: { providerEndpoints: Record<string, { useEuEndpoint: boolean }> } } };
    expect(patch.patch.aiApp.providerEndpoints.openai.useEuEndpoint).toBe(true);
  });

  // spec: AI-024
  it("ungültiges Muster: Zeile und Position aus dem Kern", async () => {
    core.handlers.set("ai.settingsSet", () => {
      throw new IpcError(localError("InvalidInput", "err-ai-pattern-invalid", { line: 3, position: 1 }));
    });
    render(<AiSettingsSection />);
    fireEvent.click(await screen.findByRole("button", { name: "Save patterns" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Invalid pattern in line 3, position 1.");
  });
});

describe("AI-019 in 1.1 ausgeblendet", () => {
  // spec: AI-019
  it("Konfigurationsseite KI ohne Richtlinie, Einstellungen KI ohne EU-Endpunkte; Schwärzung bleibt", async () => {
    expect(AI_019_ENABLED).toBe(false);
    core.handlers.set("ai.settingsGet", () => ({ ...VIEW, eu: [] }));
    core.handlers.set("ai.settingsSet", () => ({ tasks: [] }));
    core.handlers.set("ai.consentStatus", () => ({ columns: [], rows: [], requests: [], redaction: true, policy: "unrestricted", consentedProviders: [], csv: "" }));
    const page = aiConfigPages.find((p) => p.id === "ai")!;
    const ctx = draftContext({ ai: { tasks: {}, policy: "unrestricted" }, "ai.budget": { crawl: 5_000_000, unlimited: false, areas: {}, tasks: {}, warning_percent: 80 } });
    const { container, unmount } = render(<ConfigProvider value={ctx.value}><page.Component /></ConfigProvider>);
    expect(container.querySelector("[data-ai-budget-setting]")).not.toBeNull();
    expect(container.querySelector("[data-ai-policy-setting]")).toBeNull();
    expect(screen.queryAllByRole("radio")).toEqual([]);
    expect(container.textContent).not.toMatch(/Enforce EU processing|Local only|EU-Verarbeitung|Nur lokal/);
    unmount();
    render(<AiSettingsSection />);
    expect(await screen.findByRole("switch", { name: "Redact personal data before sending" })).toBeVisible();
    expect(screen.queryByRole("switch", { name: /EU endpoint/ })).toBeNull();
    expect(document.querySelector("[data-ai-eu]")).toBeNull();
    expect(document.body.textContent).not.toMatch(/EU processing|eu\.api\.openai\.com/);
  });
});

describe("Sichtbarkeit", () => {
  // spec: AI-011 [M4]
  // spec: AI-018 [M4]
  it("ohne Schalter KI (Vorschau) weder Abschnitt noch Konfigurationsseite; mit Schalter beide", async () => {
    expect(resolveSections().map((s) => s.id)).not.toContain("ai");
    expect(resolvePages().map((p) => p.id)).not.toContain("ai");
    await act(async () => {
      await setAiPreview(true);
    });
    expect(resolveSections().map((s) => s.id)).toContain("ai");
    expect(resolvePages().map((p) => p.id)).toContain("ai");
  });
});

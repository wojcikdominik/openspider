// Formen der U01-Commands (`ai.settingsGet`/`ai.settingsSet`, `ai.costBudget`/`ai.costEstimate`, `ai.consentStatus`/
// `ai.consentGrant`); der Kern liefert JSON nach `crates/os-app/src/commands/{ai_settings,ai_cost,ai_consent}.rs`.
// Alle Rechenregeln (effektive Stufe, Dropdown, Warnungen, `max_tokens`, Bestätigungsregel, Datenflusszeilen) stehen im
// Kern; die Oberfläche zeigt nur an.
import { ai } from "../../../ipc/ai";
import type { AiCostSnapshot } from "../../../ipc/gen/AiCostSnapshot";
import type { AiRunArgs } from "../../../ipc/gen/AiRunArgs";
import type { JsonValue } from "../../../ipc/gen/serde_json/JsonValue";

export type Level = "model-default" | "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max";
export const LEVELS: readonly Level[] = ["model-default", "off", "minimal", "low", "medium", "high", "xhigh", "max"];

export type TaskId =
  | "decision" | "decision-vision" | "suggest-title" | "suggest-meta" | "suggest-h1" | "suggest-alt"
  | "redirect-confirm" | "link-anchor" | "ask-sql" | "audit-summary" | "llms-summary" | "embedding";

export type Endpoint = "messages" | "responses" | "decisions" | "chat-completions" | "embeddings";

export interface ModelRef {
  provider: string;
  model: string;
  endpoint: Endpoint;
}

export interface ModelOption extends ModelRef {
  source: "catalog" | "live" | "defaults";
  capabilitiesKnown: boolean;
  assignable: boolean;
  missing: string[];
  reasoningLevels: Level[];
  maxOutputTokens: number | null;
}

export type ReasoningWarning =
  | { kind: "off-unavailable"; effective: Level }
  | { kind: "stored-unavailable"; stored: Level; effective: Level };

export interface ReasoningView {
  stored: Level | null;
  requested: Level;
  effective: Level;
  dropdown: Level[];
  warning: ReasoningWarning | null;
  levelsKnown: boolean;
}

export interface MaxTokensView {
  user: number | null;
  standard: number | null;
  value: number | null;
  tight: boolean;
  answerTokens: number | null;
  decisionFormula: boolean;
  modelLimit: number | null;
}

export interface TaskRow {
  task: TaskId;
  model: ModelRef | null;
  fallback: ModelRef[];
  defaultModel: ModelRef | null;
  defaultFallback: ModelRef[];
  isDefault: boolean;
  effectiveModel: ModelRef | null;
  effectiveFallback: ModelRef[];
  models: ModelOption[];
  reasoning: ReasoningView | null;
  maxTokens: MaxTokensView | null;
  temperature: number | null;
  maxInputChars: number | null;
  status: string | null;
  plannedModel: ModelRef | null;
  assignmentError: string[] | null;
}

export interface BudgetLimits {
  crawl: number;
  unlimited: boolean;
  areas: Record<string, number>;
  tasks: Record<string, Record<string, number>>;
  warning_percent: number;
}

export interface EuRow {
  provider: string;
  euBaseUrl: string;
  useEuEndpoint: boolean;
  euAccountConfirmed: boolean;
  allowedUnderEu: boolean;
}

export interface AdvancedRow {
  provider: string;
  concurrentDefault: number;
  concurrentMin: number;
  concurrentMax: number;
  concurrent: number;
  timeoutDefault: number;
  timeout: number;
}

/** Anzeigenamen der Anbieter (Eigennamen, in allen Sprachen gleich). */
export const PROVIDER_NAMES: Record<string, string> = { anthropic: "Anthropic", openai: "OpenAI", openrouter: "OpenRouter", ollama: "Ollama", gemini: "Gemini" };
export const providerName = (id: string): string => PROVIDER_NAMES[id] ?? id;

export type Policy = "unrestricted" | "eu-only" | "local-only";

export interface SettingsView {
  previewEnabled: boolean;
  keys: Record<string, string>;
  listsLoaded: Record<string, boolean>;
  tasks: TaskRow[];
  policy: Policy;
  budget: BudgetLimits;
  crawlBudget: BudgetLimits | null;
  eu: EuRow[];
  advanced: AdvancedRow[];
  confirmationThresholdMicroUsd: number;
  redactPersonalData: boolean;
  redactionPatterns: string[];
}

export interface EstimateRow {
  task: string;
  model: string;
  reasoning: Level | null;
  items: number;
  cached: number;
  estInputTokens: number;
  estOutputTokens: number;
  lowMicroUsd: number | null;
  expectedMicroUsd: number | null;
  highMicroUsd: number | null;
  batchSavingsMicroUsd: number | null;
  executable: boolean;
  reason: string;
  calibrated: boolean;
  requests: number | null;
}

export interface Totals {
  lowMicroUsd: number;
  expectedMicroUsd: number;
  highMicroUsd: number;
}

export interface EstimateSection {
  id: string;
  rows: unknown[];
  totals: Totals;
}

export interface CostEstimateView {
  sections: EstimateSection[];
  totals: Totals;
  budget: { budgetMicroUsd: number | null; consumedMicroUsd: number; remainingMicroUsd: number | null; warning: boolean; halted: boolean };
  thresholdMicroUsd: number;
  confirmationRequired: boolean;
}

export interface ConsentKeyView {
  provider: string;
  task: string;
  fields: string[];
}

export interface ConsentRowView {
  key: ConsentKeyView;
  cells: string[];
  piiNotice: boolean;
  consentGiven: boolean;
}

export interface ConsentRequestView {
  row: ConsentRowView;
  previousFields: string[] | null;
  addedFields: string[];
  piiNotice: boolean;
}

export interface ConsentView {
  columns: string[];
  rows: ConsentRowView[];
  requests: ConsentRequestView[];
  redaction: boolean;
  policy: Policy;
  consentedProviders: string[];
  csv: string;
}

const as = <T,>(v: JsonValue): T => v as unknown as T;

export const aiApi = {
  settings: async (signal?: AbortSignal) => as<SettingsView>(await ai.settingsGet(signal)),
  /** Prüft den Abschnitt `ai` eines Konfigurationsentwurfs und liefert die Aufgabenzeilen (schreibt nichts). */
  evaluate: async (draftAi: unknown, signal?: AbortSignal) => as<{ tasks: TaskRow[] }>(await ai.settingsSet({ ai: draftAi } as unknown as JsonValue, signal)),
  savePatterns: async (text: string) => as<{ redactionPatterns: string[] }>(await ai.settingsSet({ redactionPatterns: text })),
  raiseBudget: async (limits: BudgetLimits) => as<{ lifted: boolean }>(await ai.settingsSet({ raiseBudget: limits } as unknown as JsonValue)),
  budget: (signal?: AbortSignal): Promise<AiCostSnapshot> => ai.costBudget(signal),
  estimate: async (args: AiRunArgs = { tasks: [], sample: null, budgetMicroUsd: null }) => as<CostEstimateView>(await ai.costEstimate(args)),
  consent: async (signal?: AbortSignal) => as<ConsentView>(await ai.consentStatus(signal)),
  grant: async (keys: ConsentKeyView[]) => as<ConsentView>(await ai.consentGrant({ grant: keys } as unknown as JsonValue)),
  revoke: async (provider: string) => as<ConsentView>(await ai.consentGrant({ revokeProvider: provider })),
};

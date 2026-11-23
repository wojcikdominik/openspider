// Generiert von scripts/gen-ipc.mjs aus src/ipc/commands.json – nicht von Hand ändern (npm run gen:ipc).
import type { AiCostSnapshot } from "./gen/AiCostSnapshot";
import type { AiRunArgs } from "./gen/AiRunArgs";
import type { JsonValue } from "./gen/serde_json/JsonValue";
import type { OpHandle } from "./gen/OpHandle";
import { client } from "./client";

export const ai = {
  /** `ai.accountsList` */
  accountsList: (signal?: AbortSignal) => client.invoke<JsonValue>("ai.accountsList", undefined, signal),
  /** `ai.accountsTest` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  accountsTest: (args: JsonValue, signal?: AbortSignal) => client.invoke<OpHandle>("ai.accountsTest", args, signal),
  /** `ai.settingsGet` */
  settingsGet: (signal?: AbortSignal) => client.invoke<JsonValue>("ai.settingsGet", undefined, signal),
  /** `ai.settingsSet` */
  settingsSet: (args: JsonValue, signal?: AbortSignal) => client.invoke<JsonValue>("ai.settingsSet", args, signal),
  /** `ai.costEstimate` */
  costEstimate: (args: AiRunArgs, signal?: AbortSignal) => client.invoke<JsonValue>("ai.costEstimate", args, signal),
  /** `ai.costBudget` */
  costBudget: (signal?: AbortSignal) => client.invoke<AiCostSnapshot>("ai.costBudget", undefined, signal),
  /** `ai.costLog` */
  costLog: (signal?: AbortSignal) => client.invoke<JsonValue>("ai.costLog", undefined, signal),
  /** `ai.consentStatus` */
  consentStatus: (signal?: AbortSignal) => client.invoke<JsonValue>("ai.consentStatus", undefined, signal),
  /** `ai.consentGrant` */
  consentGrant: (args: JsonValue, signal?: AbortSignal) => client.invoke<JsonValue>("ai.consentGrant", args, signal),
  /** `ai.decisionsRun` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  decisionsRun: (args: AiRunArgs, signal?: AbortSignal) => client.invoke<OpHandle>("ai.decisionsRun", args, signal),
  /** `ai.questionsList` */
  questionsList: (signal?: AbortSignal) => client.invoke<JsonValue>("ai.questionsList", undefined, signal),
  /** `ai.testerRun` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  testerRun: (args: JsonValue, signal?: AbortSignal) => client.invoke<OpHandle>("ai.testerRun", args, signal),
  /** `ai.suggestRun` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  suggestRun: (args: AiRunArgs, signal?: AbortSignal) => client.invoke<OpHandle>("ai.suggestRun", args, signal),
  /** `ai.redirectsRun` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  redirectsRun: (args: AiRunArgs, signal?: AbortSignal) => client.invoke<OpHandle>("ai.redirectsRun", args, signal),
  /** `ai.linksRun` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  linksRun: (args: AiRunArgs, signal?: AbortSignal) => client.invoke<OpHandle>("ai.linksRun", args, signal),
  /** `ai.askRun` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  askRun: (args: JsonValue, signal?: AbortSignal) => client.invoke<OpHandle>("ai.askRun", args, signal),
  /** `ai.auditRun` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  auditRun: (args: JsonValue, signal?: AbortSignal) => client.invoke<OpHandle>("ai.auditRun", args, signal),
  /** `ai.promptsList` */
  promptsList: (signal?: AbortSignal) => client.invoke<JsonValue>("ai.promptsList", undefined, signal),
  /** `ai.semanticRun` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  semanticRun: (args: AiRunArgs, signal?: AbortSignal) => client.invoke<OpHandle>("ai.semanticRun", args, signal),
  /** `ai.searchRun` – lang laufend: liefert sofort `OpHandle`, Ergebnis (JsonValue) über `Tick.ops[].result`. */
  searchRun: (args: AiRunArgs, signal?: AbortSignal) => client.invoke<OpHandle>("ai.searchRun", args, signal),
} as const;

// M4-D05: Einstieg eines KI-Laufs aus der Oberfläche (08 AI-012, AI-018, AI-020 [M4], AI-039). Reihenfolge: Einwilligung
// (`requestConsent`, Dialog „Datenfluss bestätigen“, nur bei offenen Anfragen) → Kostenbestätigung (`requestCostConfirmation`,
// Dialog „Kostenvoranschlag“, nur über der Schwelle bzw. 50 % des Restbudgets) → `ai.decisionsRun` mit `confirmed`. Ohne
// „Bestätigen“ bzw. „Starten“ geht kein Request hinaus. Die Schaltfläche „KI-Analyse starten“ und ihre deaktivierten Zustände baut
// M4-D02 mit dieser Funktion.
import { create } from "zustand";
import { getT, type TFn } from "../../../i18n";
import { ai } from "../../../ipc/ai";
import { toCmdError, type CmdError } from "../../../ipc/errors";
import type { AiRunArgs } from "../../../ipc/gen/AiRunArgs";
import type { OpHandle } from "../../../ipc/gen/OpHandle";
import { requestConsent } from "../consent/ConsentDialog";
import { requestCostConfirmation } from "../cost/EstimateDialog";
import { aiApi } from "../settings/api";

/** Ergebnis von {@link startAiRun}. */
export type AiRunStart =
  | { state: "started"; op: OpHandle }
  | { state: "declined"; step: "consent" | "confirmation" }
  | { state: "failed"; error: CmdError; message: string };

/** Argumente von `ai.decisionsRun`: `AiRunArgs` plus die erteilte Kostenbestätigung (`ai_runs.confirmed_at`, K4-27). */
export type DecisionsRunArgs = AiRunArgs & { confirmed: boolean };

/** Letzte Meldung eines Startversuchs (für die Anzeige im KI-Tab, M4-D02). */
export const useAiRunMessage = create<{ message: string | null; error: CmdError | null }>(() => ({ message: null, error: null }));

/** Text einer Ablehnung des Kerns in der UI-Sprache (Schlüssel aus `crates/os-app/src/commands/ai_run.rs`). */
export function runMessage(error: CmdError, t: TFn = getT()): string {
  const args = (error.args ?? {}) as Record<string, unknown>;
  switch (error.messageKey) {
    case "err-ai-run-interrupted":
      return t("ai-run-interrupted");
    case "err-ai-run-foreign":
      return t("ai-run-foreign", { kind: String(args.kind ?? ""), pid: String(args.pid ?? "") });
    case "err-ai-run-crawl-running":
      return t("ai-run-crawl-running");
    case "err-ai-run-no-questions":
      return t("ai-run-no-questions");
    case "err-ai-preview-off":
      return t("ai-run-preview-off");
    default:
      return t("ai-run-failed", { detail: error.detail ?? error.messageKey });
  }
}

/**
 * Startet einen KI-Lauf über die aktiven Decision-Fragen der Crawl-Datei des Fensters (nach dem Crawl bzw. in der Pause).
 * Fortsetzen eines pausierten Crawls braucht diese Funktion nicht: die Engine setzt die KI-Warteschlange mit derselben
 * Laufzeile und deren Bestätigung fort (kein neuer Voranschlag).
 */
export async function startAiRun(args: AiRunArgs = { tasks: [], sample: null, budgetMicroUsd: null }): Promise<AiRunStart> {
  useAiRunMessage.setState({ message: null, error: null });
  try {
    // Meldet den Schätzer der Decision-Fragen im Kern an, bevor der Voranschlag ihn sammelt (Lesezugriff, sendet nichts).
    await aiApi.budget();
    if (!(await requestConsent())) return { state: "declined", step: "consent" };
    if (!(await requestCostConfirmation(args))) return { state: "declined", step: "confirmation" };
    const run: DecisionsRunArgs = { ...args, confirmed: true };
    const op = await ai.decisionsRun(run);
    return { state: "started", op };
  } catch (e) {
    const error = toCmdError(e);
    const message = runMessage(error);
    useAiRunMessage.setState({ message, error });
    return { state: "failed", error, message };
  }
}

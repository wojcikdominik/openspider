import { useT } from "../../../i18n";
import { useAiPreview } from "../preview";
import { useAiTasksActive } from "./tasks";

/**
 * Statusleistenfeld `Kosten` (UI-013 [M4]) als Platzhalter (M4-F02): nur mit eingeschaltetem Schalter „KI (Vorschau)“ und aktiven
 * KI-Aufgaben für den Crawl des Fensters (Plan M4 3.3, Nachtrag 7; Festlegung in `./tasks`), ohne Beträge. Werte aus dem
 * `BudgetLedger` und das Zahlenformat liefert M4-U01 (`ai.costBudget`).
 */
export function AiCostField() {
  const t = useT();
  const on = useAiPreview();
  const active = useAiTasksActive(on);
  if (!on || !active) return null;
  return <span className="os-status-cost" data-testid="ai-cost" title={t("ai-cost-field-tooltip")}>{t("ai-cost-field-placeholder")}</span>;
}

import { useT } from "../../../i18n";
import { useAiPreview } from "../preview";

/**
 * Statusleistenfeld `Kosten` (UI-013 [M4]) als Platzhalter (M4-F02): nur mit eingeschaltetem Schalter „KI (Vorschau)“ sichtbar und
 * ohne Beträge. Werte aus dem `BudgetLedger`, das Zahlenformat und die Sichtbarkeit je Crawl liefert M4-U01 (`ai.costBudget`).
 */
export function AiCostField() {
  const t = useT();
  const on = useAiPreview();
  if (!on) return null;
  return <span className="os-status-cost" data-testid="ai-cost" title={t("ai-cost-field-tooltip")}>{t("ai-cost-field-placeholder")}</span>;
}

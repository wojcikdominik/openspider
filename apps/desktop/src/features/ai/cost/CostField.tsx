import "./../settings/ai.css";
import { useEffect, useState } from "react";
import { useFormat, useT } from "../../../i18n";
import type { AiCostSnapshot } from "../../../ipc/gen/AiCostSnapshot";
import { useSession } from "../../../state/session";
import { aiApi } from "../settings/api";
import { useAiPreview } from "../preview";
import { AiDialogHost } from "./DialogHost";
import { fieldText, tooltipLines, warningReached } from "./format";
import { useAiWindowConfig } from "./tasks";

/**
 * Statusleistenfeld `Kosten` (UI-013 [M4], AI-013): nur mit eingeschaltetem Schalter „KI (Vorschau)“ und aktiven KI-Aufgaben für
 * den Crawl des Fensters (Plan M4 3.3, Nachtrag 7; Festlegung in `./tasks`, mit Embeddings und eigenen Prompts nach der
 * Nutzerentscheidung vom 24.11.2026). Werte aus dem gemeinsamen `BudgetLedger` (`ai.costBudget`), Format nach UI-013 in der
 * UI-Sprache; ein Sprachwechsel ändert das Format ohne Neustart. Bis der Kern antwortet, steht der Platzhalter `– / –`.
 *
 * Das Feld trägt auch die Dialoge der KI-Vorschau („Kostenvoranschlag“, „Datenfluss bestätigen“), solange der Schalter an ist.
 */
export function AiCostField() {
  const on = useAiPreview();
  return <>
    {on && <AiDialogHost />}
    {on && <CostFieldInner />}
  </>;
}

function CostFieldInner() {
  const t = useT();
  const fmt = useFormat();
  const { active, warningPercent, tick } = useAiWindowConfig(true);
  const crawlId = useSession((s) => s.crawl?.crawlId ?? null);
  const [snapshot, setSnapshot] = useState<AiCostSnapshot | null>(null);

  useEffect(() => {
    if (!active) {
      setSnapshot(null);
      return;
    }
    const abort = new AbortController();
    aiApi
      .budget(abort.signal)
      .then((s) => {
        if (!abort.signal.aborted) setSnapshot(s);
      })
      .catch(() => {
        if (!abort.signal.aborted) setSnapshot(null);
      });
    return () => abort.abort();
  }, [active, crawlId, tick]);

  if (!active) return null;
  if (!snapshot) {
    return <span className="os-status-cost" data-testid="ai-cost" title={t("ai-cost-field-tooltip")}>{t("ai-cost-field-placeholder")}</span>;
  }
  const warning = warningReached(snapshot, warningPercent);
  const lines = tooltipLines(t, snapshot, fmt.lang, warning, warningPercent);
  return (
    <span
      className={warning ? "os-status-cost os-status-cost-warning" : "os-status-cost"}
      data-testid="ai-cost"
      data-warning={warning ? "true" : undefined}
      title={lines.join("\n")}
    >
      {fieldText(t, snapshot, fmt.lang)}
    </span>
  );
}

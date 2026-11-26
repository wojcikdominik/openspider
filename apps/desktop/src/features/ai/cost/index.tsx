import { useEffect, useState } from "react";
import { useFormat, useT } from "../../../i18n";
import type { AiCostSnapshot } from "../../../ipc/gen/AiCostSnapshot";
import { Button } from "../../../ui/primitives";
import { EmptyState } from "../../../ui/primitives/EmptyState";
import type { AiArea } from "../areas";
import { aiApi } from "../settings/api";
import { showCostEstimate } from "./EstimateDialog";
import { fieldText, tooltipLines } from "./format";

/**
 * KI-Bereich „Kosten und Budget“ (UI-013 [M4], AI-012/AI-013): Stand des `BudgetLedger` für den Crawl des Fensters mit Aufteilung und
 * der Kostenvoranschlag auf Abruf. Ohne Antwort des Kerns der Leerzustand.
 */
function AiCost() {
  const t = useT();
  const fmt = useFormat();
  const [snapshot, setSnapshot] = useState<AiCostSnapshot | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => {
    const abort = new AbortController();
    aiApi
      .budget(abort.signal)
      .then((s) => !abort.signal.aborted && setSnapshot(s))
      .catch(() => !abort.signal.aborted && setError(true));
    return () => abort.abort();
  }, []);
  const estimate = async () => {
    try {
      await showCostEstimate(await aiApi.estimate());
    } catch {
      setError(true);
    }
  };
  return (
    <div className="os-ai-area" data-ai-area="cost">
      <h3 className="os-settings-heading">{t("ai-cost-title")}</h3>
      {snapshot && !error ? (
        <>
          <p data-ai-cost-summary>{fieldText(t, snapshot, fmt.lang)}</p>
          <ul>{tooltipLines(t, snapshot, fmt.lang, false, 80).map((l) => <li key={l}>{l}</li>)}</ul>
          <Button onPress={() => void estimate()}>{t("ai-estimate-open")}</Button>
        </>
      ) : (
        <EmptyState state={{ code: "NO_DATA_YET" }} />
      )}
    </div>
  );
}

export const area: AiArea = { id: "cost", titleKey: "ai-cost-title", Component: AiCost };

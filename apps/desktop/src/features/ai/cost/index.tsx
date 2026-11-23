import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „Kosten und Budget“ (Plan M4 3.3; Kostenvoranschlag und Protokoll M4-K05/U01): Leerzustand. */
function AiCost() {
  return <AiPlaceholder area="cost" titleKey="ai-cost-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "cost", titleKey: "ai-cost-title", Component: AiCost };

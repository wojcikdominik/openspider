import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „KI-Entscheidungen“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiDecisions() {
  return <AiPlaceholder area="decisions" titleKey="ai-decisions-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "decisions", titleKey: "ai-decisions-title", Component: AiDecisions };

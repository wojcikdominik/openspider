import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „Semantische Analysen“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiSemantic() {
  return <AiPlaceholder area="semantic" titleKey="ai-semantic-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "semantic", titleKey: "ai-semantic-title", Component: AiSemantic };

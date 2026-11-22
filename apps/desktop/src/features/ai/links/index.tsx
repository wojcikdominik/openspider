import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „Interne Linkvorschläge“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiLinks() {
  return <AiPlaceholder area="links" titleKey="ai-links-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "links", titleKey: "ai-links-title", Component: AiLinks };

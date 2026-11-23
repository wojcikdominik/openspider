import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „KI-Vorschläge“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiSuggestions() {
  return <AiPlaceholder area="suggestions" titleKey="ai-suggestions-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "suggestions", titleKey: "ai-suggestions-title", Component: AiSuggestions };

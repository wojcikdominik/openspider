import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „KI-Suche“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiSearch() {
  return <AiPlaceholder area="search" titleKey="ai-search-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "search", titleKey: "ai-search-title", Component: AiSearch };

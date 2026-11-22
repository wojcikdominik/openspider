import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „Redirect-Mapping“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiRedirects() {
  return <AiPlaceholder area="redirects" titleKey="ai-redirects-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "redirects", titleKey: "ai-redirects-title", Component: AiRedirects };

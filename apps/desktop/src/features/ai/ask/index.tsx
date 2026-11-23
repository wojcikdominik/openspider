import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „Frag den Crawl“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiAsk() {
  return <AiPlaceholder area="ask" titleKey="ai-ask-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "ask", titleKey: "ai-ask-title", Component: AiAsk };

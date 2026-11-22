import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „KI-Prompts“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiPrompts() {
  return <AiPlaceholder area="prompts" titleKey="ai-prompts-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "prompts", titleKey: "ai-prompts-title", Component: AiPrompts };

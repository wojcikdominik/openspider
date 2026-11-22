import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „Audit und Maßnahmenplan“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiAudit() {
  return <AiPlaceholder area="audit" titleKey="ai-audit-title" state={{ code: "NO_DATA_YET" }} />;
}

export const area: AiArea = { id: "audit", titleKey: "ai-audit-title", Component: AiAudit };

import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „Fragentester“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiTester() {
  return <AiPlaceholder area="tester" titleKey="ai-tester-title" state={{ code: "NOT_APPLICABLE" }} />;
}

export const area: AiArea = { id: "tester", titleKey: "ai-tester-title", Component: AiTester };

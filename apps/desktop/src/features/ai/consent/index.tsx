import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst des KI-Bereichs „Datenfluss und Einwilligung“ (Plan M4 3.3): Leerzustand bis zur Umsetzung durch das Fachpaket. */
function AiConsent() {
  return <AiPlaceholder area="consent" titleKey="ai-consent-title" state={{ code: "NOT_APPLICABLE" }} />;
}

export const area: AiArea = { id: "consent", titleKey: "ai-consent-title", Component: AiConsent };

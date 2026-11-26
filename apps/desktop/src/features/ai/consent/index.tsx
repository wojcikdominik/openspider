import { useT } from "../../../i18n";
import type { AiArea } from "../areas";
import { DataFlowOverview } from "./Overview";

/** KI-Bereich „Datenfluss und Einwilligung“ (AI-018): Übersicht „KI-Datenfluss“ mit Widerruf und CSV-Export (M4-U01). */
function AiConsent() {
  const t = useT();
  return (
    <div className="os-ai-area" data-ai-area="consent">
      <h3 className="os-settings-heading">{t("ai-consent-title")}</h3>
      <DataFlowOverview />
    </div>
  );
}

export const area: AiArea = { id: "consent", titleKey: "ai-consent-title", Component: AiConsent };

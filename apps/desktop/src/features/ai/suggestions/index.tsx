import { useT } from "../../../i18n";
import { EmptyState } from "../../../ui/primitives/EmptyState";
import { useDetailSelection } from "../../detail/frame/selection";
import type { AiArea } from "../areas";
import { ReviewPanel } from "./ReviewPanel";

/**
 * Bereich „KI-Vorschläge“ (Plan M4 3.3; M4-V01): die Prüfansicht der aktuellen Auswahl (AI-076); ohne gewählte URL der
 * Leerzustand (UI-016).
 */
function AiSuggestions() {
  const t = useT();
  const selection = useDetailSelection((s) => s.selection);
  const focus = selection.focus ?? selection.urlIds[0];
  return <div className="os-ai-placeholder" data-ai-area="suggestions">
    <h3 className="os-settings-heading">{t("ai-suggestions-title")}</h3>
    {focus === undefined
      ? <><EmptyState state={{ code: "NO_DATA_YET" }} /><p className="os-hint">{t("ai-suggestions-select-row")}</p></>
      : <ReviewPanel urlId={focus} selected={selection.urlIds} />}
  </div>;
}

export const area: AiArea = { id: "suggestions", titleKey: "ai-suggestions-title", Component: AiSuggestions };

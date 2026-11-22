import { useT } from "../../i18n";
import { EmptyState } from "../../ui/primitives/EmptyState";
import type { EmptyState as EmptyStateDto } from "../../ui/emptyState/codes";

/**
 * Leerzustand eines KI-Bereichs (UI-016, Plan M4 E9) bis zur Umsetzung durch das Fachpaket: Überschrift, Hinweisfläche mit
 * Zustandscode und der Hinweis, dass die Funktion als Vorschau noch folgt.
 */
export function AiPlaceholder({ area, titleKey, state = { code: "NO_DATA_YET" }, heading = true }: { area: string; titleKey: string; state?: EmptyStateDto; heading?: boolean }) {
  const t = useT();
  return (
    <div className="os-ai-placeholder" data-ai-area={area}>
      {heading && <h3 className="os-settings-heading">{t(titleKey)}</h3>}
      <EmptyState state={state} />
      <p className="os-hint">{t("ai-placeholder-note")}</p>
    </div>
  );
}

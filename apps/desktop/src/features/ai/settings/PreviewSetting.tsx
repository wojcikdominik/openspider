import { useT } from "../../../i18n";
import { Switch } from "../../../ui/primitives";
import { setAiPreview, useAiPreview } from "../preview";

/**
 * Schalter „KI (Vorschau)“ (Plan M4 E1, E-2 b; M4-F02) auf der Seite Allgemein: Standard aus. Eingeschaltet erscheinen die KI-Tabs,
 * der Einstellungsabschnitt KI, die KI-Konfigurationsseiten und das Statusleistenfeld Kosten.
 */
export function AiPreviewSetting() {
  const t = useT();
  const on = useAiPreview();
  return <>
    <Switch isSelected={on} anchor="ai_app.preview_enabled" onChange={(v) => void setAiPreview(v)}>
      {t("ai-preview-switch")}
    </Switch>
    <p className="os-settings-note">{t("ai-preview-note")}</p>
  </>;
}

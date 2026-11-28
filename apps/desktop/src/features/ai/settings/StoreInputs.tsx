// Option „KI-Eingaben speichern“ (08 AI-032, Nachtrag K4-24; M4-D05) im Konfigurationsdialog, Seite „KI“ (Entwurf
// `ai.store_inputs`, Standard an). Die Seite gibt es nur mit eingeschaltetem Schalter „KI (Vorschau)“ – ohne ihn ist die Option
// nicht sichtbar, und die Engine speichert nichts.
import { useT } from "../../../i18n";
import { Checkbox } from "../../../ui/primitives";
import { useConfig } from "../../config/frame/context";

export function StoreInputsSetting() {
  const t = useT();
  const { draft } = useConfig();
  const value = draft.get<boolean>("ai.store_inputs") ?? true;
  return (
    <section aria-labelledby="ai-store-inputs-heading" data-ai-store-inputs-setting data-setting-anchor="ai.store_inputs">
      <h4 id="ai-store-inputs-heading" className="os-settings-heading">{t("ai-store-inputs-title")}</h4>
      <Checkbox isSelected={value} onChange={(v) => draft.set("ai.store_inputs", v)}>{t("ai-store-inputs-label")}</Checkbox>
      <p className="os-hint">{t("ai-store-inputs-hint")}</p>
    </section>
  );
}

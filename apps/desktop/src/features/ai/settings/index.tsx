import "./ai.css";
import { useT } from "../../../i18n";
import type { ConfigPage } from "../../config/frame/types";
import type { SettingsSection } from "../../settings/frame/types";
import type { AiArea } from "../areas";
import { AiSettingsSection } from "./AiSettingsSection";
import { AI_019_ENABLED } from "./api";
import { BudgetSetting, PolicySetting } from "./PolicyBudget";
import { StoreInputsSetting } from "./StoreInputs";
import { TaskTable } from "./TaskTable";

/** KI-Einstellungen der App (M4-U01; Konten folgen mit M4-P01 unter `./accounts`). */
function AiSettings() {
  return <AiSettingsSection />;
}

/**
 * KI-Konfigurationsseite (Profil): Modellwahl je Aufgabe mit Spalte „Reasoning“ (AI-011), Richtlinie (AI-019), Crawl-Budget (AI-013).
 * Fragen (D01) und Vorschläge (V01) ergänzen weitere Seiten in {@link aiConfigPages}.
 */
function AiConfig() {
  const t = useT();
  return (
    <section className="os-cfg-page" aria-label={t("config-page-ai")} data-page-id="ai">
      <h3 className="os-settings-heading">{t("config-page-ai")}</h3>
      <TaskTable />
      {AI_019_ENABLED && <PolicySetting />}
      <BudgetSetting />
      <StoreInputsSetting />
    </section>
  );
}

export const area: AiArea = { id: "settings", titleKey: "ai-settings-title", Component: AiSettings };

/** Einstellungsabschnitt „KI“ – die Rahmen-Registry nimmt ihn nur mit eingeschaltetem Schalter „KI (Vorschau)“ auf. */
export const aiSettingsSection: SettingsSection = { id: "ai", order: 88, titleKey: "settings-section-ai", Component: AiSettings };

/** KI-Seiten des Konfigurationsdialogs – nur mit eingeschaltetem Schalter „KI (Vorschau)“ registriert. */
export const aiConfigPages: readonly ConfigPage[] = [
  { id: "ai", order: 175, titleKey: "config-page-ai", anchors: ["ai", "embeddings"], fields: [], Component: AiConfig },
];

import type { ConfigPage } from "../../config/frame/types";
import type { SettingsSection } from "../../settings/frame/types";
import { AiPlaceholder } from "../AiPlaceholder";
import type { AiArea } from "../areas";

/** Gerüst der KI-Einstellungen (Plan M4 3.3; Inhalt M4-U01, Konten M4-P01): Leerzustand. */
function AiSettings() {
  return <AiPlaceholder area="settings" titleKey="ai-settings-title" state={{ code: "NOT_APPLICABLE" }} heading={false} />;
}

/** Gerüst der KI-Konfigurationsseite (Fragen, Vorschläge, Modellwahl je Aufgabe; Fachpakete D01/V01/U01). */
function AiConfig() {
  return <AiPlaceholder area="config" titleKey="config-page-ai" state={{ code: "NOT_APPLICABLE" }} heading={false} />;
}

export const area: AiArea = { id: "settings", titleKey: "ai-settings-title", Component: AiSettings };

/** Einstellungsabschnitt „KI“ – die Rahmen-Registry nimmt ihn nur mit eingeschaltetem Schalter „KI (Vorschau)“ auf. */
export const aiSettingsSection: SettingsSection = { id: "ai", order: 88, titleKey: "settings-section-ai", Component: AiSettings };

/** KI-Seiten des Konfigurationsdialogs – nur mit eingeschaltetem Schalter „KI (Vorschau)“ registriert. */
export const aiConfigPages: readonly ConfigPage[] = [
  { id: "ai", order: 175, titleKey: "config-page-ai", anchors: ["ai", "embeddings"], fields: [], Component: AiConfig },
];

// Schalter „KI (Vorschau)“ im Frontend (Plan M4 E1, Nutzerentscheidung E-2 b; M4-F02): App-Einstellung `aiApp.previewEnabled`,
// Standard aus. Ohne Schalter erscheint keine KI-Oberfläche (Tabs, Einstellungsabschnitt, Konfigurationsseiten, Feld Kosten);
// die Listen des Kerns lassen die KI-Einträge ebenfalls weg (os-app `commands::ai_preview`).
import type { AppSettingsDto } from "../../ipc/gen/AppSettingsDto";
import { usePrefs } from "../../state/prefs";

/** Die KI-Tabs (UI-004 [M4]) in kanonischer Schreibweise. */
export const AI_TABS: readonly string[] = ["AI", "AI Decisions", "AI Suggestions", "AI Search"];

export const isAiTab = (id: string): boolean => AI_TABS.includes(id);

/** Ist der Schalter in diesen Einstellungen an? Fehlt der Abschnitt (ältere Kerne), gilt aus. */
export function aiPreviewOn(dto: AppSettingsDto | null | undefined): boolean {
  return dto?.aiApp?.previewEnabled === true;
}

/** Aktueller Zustand des Schalters (aus dem Einstellungsspeicher, auch fensterübergreifend). */
export function useAiPreview(): boolean {
  return usePrefs((s) => aiPreviewOn(s.dto));
}

/** Zustand ohne Abonnement, z. B. beim Aufbau einer Registry. */
export function aiPreviewNow(): boolean {
  return aiPreviewOn(usePrefs.getState().dto);
}

/** Schaltet die Vorschau ein oder aus (JSON-Merge-Patch über `settings.set`, wirkt sofort). */
export function setAiPreview(on: boolean): Promise<void> {
  return usePrefs.getState().patch({ aiApp: { previewEnabled: on } });
}

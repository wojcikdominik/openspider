// KI-Tabs in der Tab-Leiste (UI-004 [M4], M4-F02): Der Kern liefert sie in `catalog.tabs` nur mit eingeschaltetem Schalter
// „KI (Vorschau)“. Ändert sich der Schalter, lädt die Tab-Leiste den Katalog neu – auch nach der Bestätigung durch den Kern, damit
// eine Antwort von vor dem Speichern den neuen Zustand nicht überdeckt.
import { useEffect } from "react";
import { usePrefs } from "../../state/prefs";
import { useTabs } from "../../state/tabs";
import { aiPreviewOn, isAiTab } from "./preview";

/** Stimmt der geladene Katalog mit dem Schalter überein? */
export function catalogMatchesPreview(catalog: readonly { id: string }[], on: boolean): boolean {
  return catalog.some((t) => isAiTab(t.id)) === on;
}

/** Hält die Tab-Leiste mit dem Schalter in Einklang (einmal in der Tab-Leiste eingebunden). */
export function useAiTabsSync(): void {
  useEffect(() => usePrefs.subscribe((s, prev) => {
    if (s.dto === prev.dto) return;
    const tabs = useTabs.getState();
    if (!tabs.loaded || catalogMatchesPreview(tabs.catalog, aiPreviewOn(s.dto))) return;
    void tabs.load();
  }), []);
}

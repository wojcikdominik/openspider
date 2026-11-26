// KI-Tabs in der Tab-Leiste (UI-004 [M4], M4-F02): Der Kern liefert sie in `catalog.tabs` nur mit eingeschaltetem Schalter
// „KI (Vorschau)“. Plan M4 3.3 (5): Das Umschalten wirkt in offenen Fenstern sofort, in beiden Richtungen.
// - Ändert sich der Schalter, lädt die Tab-Leiste den Katalog neu, nacheinander statt überlappend, damit eine Antwort von vor
//   dem Speichern im Kern nicht die spätere überdeckt; auch nach der Bestätigung durch den Kern wird erneut geprüft.
// - `useTabs.load()` übernimmt bei gleichzeitiger Navigation (Gruppenwechsel, Tab-Wahl) nur den Katalog und behält die alte
//   Reihenfolge; dann blieben ausgeschaltete KI-Tabs in der Leiste stehen. [`pruneStaleAiTabs`] entfernt KI-Tabs, die nicht
//   mehr im Katalog stehen, aus Reihenfolge, Ausblendliste und Gruppenauswahl; war einer gewählt, fällt die Auswahl auf einen
//   vorhandenen Tab zurück.
import { useEffect } from "react";
import { usePrefs } from "../../state/prefs";
import { groupOf, groupTabs, MANDATORY_TAB, useTabs, type TabGroupId } from "../../state/tabs";
import { aiPreviewOn, isAiTab } from "./preview";
// M4-V01 (additiv): Prüfbereich des Tabs „AI Suggestions“.
import { useSuggestionsReviewHost } from "./suggestions/host";

/** Stimmt der geladene Katalog mit dem Schalter überein? */
export function catalogMatchesPreview(catalog: readonly { id: string }[], on: boolean): boolean {
  return catalog.some((t) => isAiTab(t.id)) === on;
}

/** Entfernt KI-Tabs, die der Katalog nicht (mehr) enthält, aus der Tab-Leiste; Rückfall der Auswahl auf einen vorhandenen Tab. */
export function pruneStaleAiTabs(): void {
  const s = useTabs.getState();
  if (!s.loaded) return;
  const known = new Set(s.catalog.map((t) => t.id));
  const stale = (id: string) => isAiTab(id) && !known.has(id);
  const groupActive = Object.fromEntries(
    Object.entries(s.groupActive).filter(([, id]) => typeof id === "string" && !stale(id)),
  ) as Partial<Record<TabGroupId, string>>;
  const activeStale = stale(s.active);
  if (!s.order.some(stale) && !s.hidden.some(stale) && !activeStale
    && Object.keys(groupActive).length === Object.keys(s.groupActive).length) return;
  const order = s.order.filter((id) => !stale(id));
  const hidden = s.hidden.filter((id) => !stale(id));
  if (!activeStale) {
    useTabs.setState({ order, hidden, groupActive });
    return;
  }
  // Zuerst ein sichtbarer Tab derselben Gruppe, sonst der Pflicht-Tab „Internal“.
  const visible = (id: string) => known.has(id) && !hidden.includes(id);
  const next = groupTabs(order, groupOf(s.active)).find(visible) ?? (known.has(MANDATORY_TAB) ? MANDATORY_TAB : order.find(visible));
  if (next === undefined) {
    useTabs.setState({ order, hidden, groupActive });
    return;
  }
  useTabs.setState({ order, hidden, active: next, groupActive: { ...groupActive, [groupOf(next)]: next } });
}

/** Hält die Tab-Leiste mit dem Schalter in Einklang (einmal in der Tab-Leiste eingebunden). */
export function useAiTabsSync(): void {
  useSuggestionsReviewHost();
  useEffect(() => {
    let running = false;
    let pending = false;
    let alive = true;
    const sync = async () => {
      pending = true;
      if (running) return;
      running = true;
      try {
        while (pending && alive) {
          pending = false;
          const tabs = useTabs.getState();
          if (!tabs.loaded || catalogMatchesPreview(tabs.catalog, aiPreviewOn(usePrefs.getState().dto))) continue;
          await tabs.load();
        }
      } finally {
        running = false;
        pruneStaleAiTabs();
      }
    };
    const offPrefs = usePrefs.subscribe((s, prev) => {
      if (s.dto !== prev.dto) void sync();
    });
    // Jede Änderung der Tab-Leiste (auch ein Laden aus anderem Anlass) darf keine verwaisten KI-Tabs hinterlassen.
    const offTabs = useTabs.subscribe((s, prev) => {
      if (s.catalog !== prev.catalog || s.order !== prev.order || s.active !== prev.active) pruneStaleAiTabs();
    });
    pruneStaleAiTabs();
    return () => {
      alive = false;
      offPrefs();
      offTabs();
    };
  }, []);
}

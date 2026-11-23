// Die Bereiche des KI-Gerüsts (Plan M4 3.3): je Bereich ein Ordner `features/ai/<bereich>/` mit Leerzustand und i18n de/en.
// Die Fachpakete (M4.md 4.2) ersetzen die Komponente und behalten Kennung und Ordner bei.
import type { FC } from "react";

export interface AiArea {
  id: string;
  titleKey: string;
  Component: FC;
}

export const AI_AREA_IDS = ["settings", "cost", "consent", "decisions", "suggestions", "search", "ask", "semantic", "prompts", "redirects", "links", "audit", "tester"] as const;
export type AiAreaId = (typeof AI_AREA_IDS)[number];

const modules = import.meta.glob<{ area?: AiArea }>("./*/index.tsx", { eager: true });

/** Alle gelieferten Bereiche in der Reihenfolge von {@link AI_AREA_IDS}. */
export function aiAreas(found: readonly AiArea[] = Object.values(modules).flatMap((m) => (m.area ? [m.area] : []))): AiArea[] {
  return AI_AREA_IDS.flatMap((id) => found.filter((a) => a.id === id));
}

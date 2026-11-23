import type { FC } from "react";
import { aiPreviewNow } from "../../ai/preview";
import { AiPreviewSetting } from "../../ai/settings/PreviewSetting";
import { aiSettingsSection } from "../../ai/settings";
import { KNOWN_SECTIONS, sectionTitleKey, type SettingsSection } from "./types";

const modules = import.meta.glob<{ section?: SettingsSection }>("../sections/*/index.tsx", { eager: true });

export interface ResolvedSection extends Omit<SettingsSection, "Component"> {
  Component: FC | null;
}

/**
 * Alle Abschnitte: gefundene Module plus Platzhalter für bekannte, noch nicht gelieferte (Component = null). Der Abschnitt „KI“
 * (M4-F02) kommt nur mit eingeschaltetem Schalter „KI (Vorschau)“ dazu (Plan M4 E1, E-2 b).
 */
export function resolveSections(
  found: readonly SettingsSection[] = Object.values(modules).flatMap((m) => (m.section ? [m.section] : [])),
  aiPreview: boolean = aiPreviewNow(),
): ResolvedSection[] {
  const byId = new Map<string, ResolvedSection>();
  for (const k of KNOWN_SECTIONS) byId.set(k.id, { id: k.id, order: k.order, titleKey: sectionTitleKey(k.id), Component: null });
  for (const s of found) byId.set(s.id, s);
  if (aiPreview) byId.set(aiSettingsSection.id, aiSettingsSection);
  return [...byId.values()].sort((a, b) => a.order - b.order);
}

/**
 * Zusätzliche Einträge eines Abschnitts, die andere Features beisteuern (unter dem Inhalt des Abschnitts): der Schalter
 * „KI (Vorschau)“ auf der Seite Allgemein (M4-F02).
 */
export function sectionExtras(id: string): readonly FC[] {
  return id === "general" ? [AiPreviewSetting] : [];
}

/** `appearance.zoom` → Abschnitt `appearance`. */
export function sectionOfAnchor(anchor: string): string {
  return anchor.split(".")[0] ?? anchor;
}

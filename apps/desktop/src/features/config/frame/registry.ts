import type { ConfigDraft } from "./draft";
import { aiPreviewNow } from "../../ai/preview";
import { aiConfigPages } from "../../ai/settings";
import { GROUP_ORDER, KNOWN_PAGES, pageTitleKey, type ConfigPage } from "./types";

type PageModule = { page?: ConfigPage; pages?: ConfigPage[] };

const modules = import.meta.glob<PageModule>("../pages/*/index.tsx", { eager: true });

export interface ResolvedPage extends Omit<ConfigPage, "Component" | "fields" | "anchors"> {
  fields: ConfigPage["fields"];
  anchors: ConfigPage["anchors"];
  Component: ConfigPage["Component"] | null;
}

/**
 * Alle Seiten der Navigation: gefundene Module plus Platzhalter für bekannte, noch nicht gelieferte Seiten (`Component = null`).
 * Weitere Pakete (M2-U13B) legen `features/config/pages/<id>/index.tsx` an und exportieren `page` bzw. `pages`; der Rahmen
 * bleibt unverändert.
 */
export function resolvePages(
  found: readonly ConfigPage[] = Object.values(modules).flatMap((m) => [...(m.pages ?? []), ...(m.page ? [m.page] : [])]),
  aiPreview: boolean = aiPreviewNow(),
): ResolvedPage[] {
  const byId = new Map<string, ResolvedPage>();
  for (const k of KNOWN_PAGES) byId.set(k.id, { id: k.id, group: k.group, order: k.order, titleKey: pageTitleKey(k.id), anchors: [], fields: [], Component: null });
  for (const p of found) byId.set(p.id, p);
  // M4-F02 (Plan M4 E1, E-2 b): KI-Konfigurationsseiten nur mit eingeschaltetem Schalter „KI (Vorschau)“.
  if (aiPreview) for (const p of aiConfigPages) byId.set(p.id, p);
  return [...byId.values()].sort((a, b) => a.order - b.order);
}

export interface NavGroup {
  id: string;
  /** `null` = Seite auf oberster Ebene. */
  group: string | null;
  pages: ResolvedPage[];
}

/** Gliedert die Seiten in Navigationseinträge: Gruppen (`spider`, `custom`) mit ihren Seiten und einzelne Seiten. */
export function navigation(pages: readonly ResolvedPage[]): NavGroup[] {
  const out: NavGroup[] = [];
  for (const p of pages) {
    if (!p.group) {
      out.push({ id: p.id, group: null, pages: [p] });
      continue;
    }
    const existing = out.find((g) => g.group === p.group);
    if (existing) existing.pages.push(p);
    else out.push({ id: `group:${p.group}`, group: p.group, pages: [p] });
  }
  return out.sort((a, b) => (GROUP_ORDER[a.group ?? ""] ?? a.pages[0]?.order ?? 0) - (GROUP_ORDER[b.group ?? ""] ?? b.pages[0]?.order ?? 0));
}

/** Optional constraints of loaded pages; absent guards do not affect the dialog. */
export function pageSaveBlocked(pages: readonly ResolvedPage[], draft: ConfigDraft): boolean {
  return pages.some(page => page.saveGuard?.(draft) === true);
}

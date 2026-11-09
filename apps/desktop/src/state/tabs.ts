import { flushUiState, saveUiState } from "./uistate";
import { create } from "zustand";
import { catalog } from "../ipc/catalog";
import { uistate } from "../ipc/uistate";
import type { TabInfo } from "../ipc/gen/TabInfo";
import type { SortKey } from "../ipc/gen/SortKey";
import type { JsonValue } from "../ipc/gen/serde_json/JsonValue";
import type { RuleNode } from "../ipc/gen/RuleNode";

/** Advanced search (UI-011 [P1]): the core-parsed rule tree plus its text syntax. */
export interface RuleQuery { tree: RuleNode; caseSensitive: boolean; text: string }
export interface TabQuery { filter: string; search: string; sort: SortKey[]; rules?: RuleQuery | null }
export const defaultQuery = (): TabQuery => ({ filter: "All", search: "", sort: [] });
/** Tabs that can never be closed (UI-004, UI-112). */
export const MANDATORY_TAB = "Internal";
export function cycleSort(sort: SortKey[], column: string, extend = false): SortKey[] {
  const previous = sort.find(s => s.column === column);
  const rest = extend ? sort.filter(s => s.column !== column) : [];
  const next = previous?.dir === "desc" ? null : { column, dir: previous?.dir === "asc" ? "desc" as const : "asc" as const };
  if (!extend) return next ? [next] : [];
  if (!previous) return next ? [...rest, next] : rest;
  return sort.flatMap(s => s.column === column ? next ? [next] : [] : [s]);
}
export function object(value: JsonValue | undefined): Record<string, JsonValue> {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value : {};
}
/** Merge only this feature's keys through the shared persistence queue. */
export function saveTableState(patch: Record<string, JsonValue>): Promise<void> {
  return saveUiState(state => ({ ...state, ...patch }));
}
/**
 * Grouped navigation (UI-004, OS-Festlegung K2-77): the bar above the pill tabs. Default assignment from the spec; tabs of later
 * milestones get their group there, until then an unknown tab joins „Benutzerdefiniert“ so that it stays reachable.
 */
export type TabGroupId = "urls" | "content" | "technical" | "links" | "performance" | "custom" | "changes";
export const TAB_GROUPS: readonly { id: TabGroupId; tabs: readonly string[] }[] = [
  { id: "urls", tabs: ["Internal", "External", "Security", "Response Codes", "URL"] },
  { id: "content", tabs: ["Page Titles", "Meta Description", "Meta Keywords", "H1", "H2", "Content", "Images", "Structured Data"] },
  { id: "technical", tabs: ["Canonicals", "Pagination", "Directives", "Hreflang", "JavaScript", "AMP", "Sitemaps", "Mobile", "Accessibility", "Validation"] },
  { id: "links", tabs: ["Links", "Link Metrics"] },
  { id: "performance", tabs: ["PageSpeed", "Analytics", "Search Console"] },
  { id: "custom", tabs: ["Custom Search", "Custom Extraction", "Custom JavaScript"] },
  // K2-77: „Change Detection erscheint im Vergleichsmodus als eigener Eintrag“ – the core lists the tab only during a comparison.
  { id: "changes", tabs: ["Change Detection"] },
];
export function groupOf(tab: string): TabGroupId {
  return TAB_GROUPS.find(g => g.tabs.includes(tab))?.id ?? "custom";
}
/** Tabs of a group in the current order. */
export function groupTabs(order: readonly string[], group: TabGroupId): string[] {
  return order.filter(id => groupOf(id) === group);
}
/**
 * The tab that cannot be closed (UI-004/UI-112): Internal, in the URLs group. Other groups may be emptied; their ▾ in the navigation
 * still lists all their tabs, and opening one shows it again.
 */
export function groupMandatory(group: TabGroupId, catalog: readonly TabInfo[]): string | undefined {
  return group === "urls" && catalog.some(t => t.id === MANDATORY_TAB) ? MANDATORY_TAB : undefined;
}
const isMandatory = (id: string, _catalog: readonly TabInfo[]) => id === MANDATORY_TAB;
const GROUP_IDS = TAB_GROUPS.map(g => g.id) as string[];

interface TabsState {
  catalog: TabInfo[]; order: string[]; hidden: string[]; active: string; queries: Record<string, TabQuery>;
  loaded: boolean; error: unknown; focusId?: number; focusRequest: number;
  /** „Übersicht“ (dashboard placeholder until U35) or the pill tabs of the active tab's group. */
  view: "tabs" | "overview";
  /** Last tab per group, so that a group opens where it was left. */
  groupActive: Partial<Record<TabGroupId, string>>;
  load(): Promise<void>; select(id: string, filter?: string): void;
  selectGroup(group: TabGroupId): void; showOverview(): void;
  query(patch: Partial<TabQuery>): void; reorder(ids: string[]): Promise<void>;
  show(id: string, visible: boolean): Promise<void>;
  /** „Tabs zurücksetzen“ im Menü Ansicht: alle Gruppen. */
  reset(): Promise<void>;
  /** „Tabs zurücksetzen“ in Überlauf- und Kontextmenü (UI-004, K2-77): nur Reihenfolge und Sichtbarkeit dieser Gruppe. */
  resetGroup(group: TabGroupId): Promise<void>;
  /** Bulk hide (UI-112 „Andere/Alle schließen“); mandatory tabs stay visible. */
  hide(ids: string[]): Promise<void>;
  /** Apply a complete order and visibility at once („Tabs konfigurieren“); with `group`, only that group's tabs change. */
  layout(order: string[], hidden: string[], group?: TabGroupId): Promise<void>;
  focus(id: string, urlId: number): void;
}
/** Counts layout and navigation edits: an overlapping `load()` must not undo the user's choice (A01-19, K2-77). */
let layoutEdits = 0;
/** The whole `topTabs` section: layout plus navigation state (K2-77: group and tab survive a restart). */
function persisted(s: Pick<TabsState, "order" | "hidden" | "active" | "view" | "groupActive">): Record<string, JsonValue> {
  return { topTabs: { order: s.order, hidden: s.hidden, active: s.active, view: s.view, groupActive: s.groupActive as Record<string, string> } };
}
export const useTabs = create<TabsState>((set, get) => {
  const save = () => saveTableState(persisted(get()));
  /** A hidden active tab falls back to its group's first visible tab, else to Internal. */
  const fallback = (hidden: readonly string[]) => {
    const { active, order } = get();
    if (!hidden.includes(active)) return {};
    const next = groupTabs(order, groupOf(active)).find(id => !hidden.includes(id)) ?? MANDATORY_TAB;
    return { active: next, groupActive: { ...get().groupActive, [groupOf(next)]: next } };
  };
  return {
  catalog: [], order: [], hidden: [], active: "Internal", queries: {}, loaded: false, error: null, focusRequest: 0, view: "tabs", groupActive: {},
  async load() {
    try {
      const edits = layoutEdits;
      // Local edits are saved with a delay: let them reach the file before it is read, otherwise the stale file wins.
      await flushUiState().catch(() => undefined);
      const [tabs, dto] = await Promise.all([catalog.tabs(), uistate.load()]);
      const defaults = [...tabs].sort((a, b) => a.defaultOrder - b.defaultOrder).map(t => t.id);
      if (edits !== layoutEdits) { set({ catalog: tabs, order: [...new Set([...get().order, ...defaults])], loaded: true, error: null }); return; }
      const saved = object(object(dto.state).topTabs);
      const order = Array.isArray(saved.order) ? saved.order.filter((v): v is string => typeof v === "string" && defaults.includes(v)) : [];
      const hidden = Array.isArray(saved.hidden) ? saved.hidden.filter((v): v is string => typeof v === "string" && !isMandatory(v, tabs) && defaults.includes(v)) : [];
      const groupActive = Object.fromEntries(Object.entries(object(saved.groupActive))
        .filter(([group, id]) => GROUP_IDS.includes(group) && typeof id === "string" && defaults.includes(id) && !hidden.includes(id))) as Partial<Record<TabGroupId, string>>;
      // A tab that left the catalog (Change Detection after closing a comparison) falls back to Internal.
      const fallback = defaults.includes(get().active) ? get().active : defaults.includes(MANDATORY_TAB) ? MANDATORY_TAB : defaults[0] ?? get().active;
      const active = typeof saved.active === "string" && defaults.includes(saved.active) && !hidden.includes(saved.active) ? saved.active : fallback;
      set({ catalog: tabs, order: [...new Set([...order, ...defaults])], hidden, loaded: true, error: null, active, groupActive: { ...groupActive, [groupOf(active)]: active },
        view: saved.view === "overview" ? "overview" : "tabs" });
    } catch (error) { set({ error }); }
  },
  select(id, filter) {
    if (!get().catalog.some(t => t.id === id)) return;
    const query = get().queries[id] ?? defaultQuery();
    const changed = get().active !== id || get().view !== "tabs";
    if (changed) layoutEdits++;
    set({ active: id, view: "tabs", groupActive: { ...get().groupActive, [groupOf(id)]: id }, queries: { ...get().queries, [id]: { ...query, ...(filter ? { filter } : {}) } } });
    if (changed && get().loaded) void save().catch(() => undefined);
  },
  selectGroup(group) {
    const { order, hidden, groupActive } = get();
    const visible = groupTabs(order, group).filter(id => !hidden.includes(id));
    const target = groupActive[group] && visible.includes(groupActive[group]!) ? groupActive[group]! : visible[0];
    if (target) { get().select(target); return; }
    // Every tab of the group is closed: opening the group shows its first tab again.
    const first = groupTabs(order, group)[0];
    if (first) { void get().show(first, true).catch(() => undefined); get().select(first); }
  },
  showOverview() {
    if (get().view !== "overview") layoutEdits++;
    set({ view: "overview" });
    if (get().loaded) void save().catch(() => undefined);
  },
  query(patch) { const s = get(); set({ queries: { ...s.queries, [s.active]: { ...(s.queries[s.active] ?? defaultQuery()), ...patch } } }); },
  async reorder(ids) {
    layoutEdits++;
    const order = [...new Set([...ids, ...get().order])].filter(id => get().catalog.some(t => t.id === id));
    set({ order }); await save();
  },
  async show(id, visible) {
    layoutEdits++;
    if (isMandatory(id, get().catalog) || !get().catalog.some(t => t.id === id)) return;
    const hidden = visible ? get().hidden.filter(v => v !== id) : [...new Set([...get().hidden, id])];
    set({ hidden, ...fallback(hidden) });
    await save();
  },
  async hide(ids) {
    layoutEdits++;
    const known = ids.filter(id => !isMandatory(id, get().catalog) && get().catalog.some(t => t.id === id));
    const hidden = [...new Set([...get().hidden, ...known])];
    set({ hidden, ...fallback(hidden) });
    await save();
  },
  async layout(ids, hiddenIds, group) {
    layoutEdits++;
    const known = (list: readonly string[]) => list.filter(id => get().catalog.some(t => t.id === id));
    let order: string[]; let hidden: string[];
    if (group) {
      // Only the group's slots change: its tabs take the new order in the positions they held, other groups stay put.
      const queue = known(ids).filter(id => groupOf(id) === group);
      const rest = groupTabs(get().order, group).filter(id => !queue.includes(id));
      const mine = [...queue, ...rest];
      order = get().order.map(id => groupOf(id) === group ? mine.shift()! : id);
      hidden = [...get().hidden.filter(id => groupOf(id) !== group), ...hiddenIds.filter(id => groupOf(id) === group)];
    } else {
      order = [...new Set([...known(ids), ...get().order])].filter(id => get().catalog.some(t => t.id === id));
      hidden = [...hiddenIds];
    }
    hidden = [...new Set(hidden)].filter(id => !isMandatory(id, get().catalog) && order.includes(id));
    set({ order, hidden, ...fallback(hidden) });
    await save();
  },
  async reset() {
    layoutEdits++;
    const order = [...get().catalog].sort((a, b) => a.defaultOrder - b.defaultOrder).map(t => t.id);
    set({ order, hidden: [], groupActive: { [groupOf(get().active)]: get().active } }); await save();
  },
  async resetGroup(group) {
    layoutEdits++;
    const defaults = [...get().catalog].sort((a, b) => a.defaultOrder - b.defaultOrder).map(t => t.id).filter(id => groupOf(id) === group);
    const queue = [...defaults];
    // The group's tabs return to their default order in the slots the group holds; other groups keep order and visibility.
    const order = get().order.map(id => groupOf(id) === group ? queue.shift() ?? id : id);
    const hidden = get().hidden.filter(id => groupOf(id) !== group);
    set({ order, hidden, ...fallback(hidden) }); await save();
  },
  focus(id, urlId) {
    get().select(id, "All"); get().query({ search: "", rules: null });
    set({ hidden: get().hidden.filter(t => t !== id), focusId: urlId, focusRequest: get().focusRequest + 1 });
  },
  };
});

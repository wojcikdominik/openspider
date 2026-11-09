// Small UI stores of the advanced table controls: search history (UI-011 [P1]) and the global segment filter (UI-020).
import { create } from "zustand";
import { segment } from "../../ipc/segment";
import { uistate } from "../../ipc/uistate";
import type { RuleNode } from "../../ipc/gen/RuleNode";
import type { SegmentDef } from "../../ipc/gen/SegmentDef";
import type { SegmentFilter } from "../../ipc/gen/SegmentFilter";
import type { JsonValue } from "../../ipc/gen/serde_json/JsonValue";
import { object, saveTableState, type RuleQuery } from "../../state/tabs";
import { pushHistory } from "./query";

interface HistoryState { entries: RuleQuery[]; load(): Promise<void>; add(entry: RuleQuery): Promise<void> }
function entries(value: JsonValue | undefined): RuleQuery[] {
  return Array.isArray(value) ? value.map(object).filter(e => typeof e.text === "string" && object(e.tree).kind !== undefined)
    .map(e => ({ text: e.text as string, caseSensitive: e.caseSensitive === true, tree: e.tree as unknown as RuleNode })) : [];
}
export const useSearchHistory = create<HistoryState>((set, get) => ({
  entries: [],
  async load() { set({ entries: entries(object((await uistate.load()).state).searchHistory).slice(0, 20) }); },
  async add(entry) {
    const next = pushHistory(get().entries, entry); set({ entries: next });
    await saveTableState({ searchHistory: next as unknown as JsonValue });
  },
}));

interface SegmentState {
  segments: SegmentDef[]; filter: SegmentFilter | null; error: unknown;
  /** Bumped after every successful load, so views and counts follow a reassignment without recrawl. Loading the
   * segments of another crawl (`crawlChanged`) does not bump it: the table follows the crawl itself, and a second
   * open at every crawl start cleared footer and chip once more (UI-018, B4-LAGFIX2). */
  revision: number;
  load(options?: { crawlChanged?: boolean }): Promise<void>; setFilter(filter: SegmentFilter | null): void;
}
/** Global segment filter: the upper table, filter counts and – via this store – Overview, Issues and Site Structure. */
export const useSegments = create<SegmentState>((set, get) => ({
  segments: [], filter: null, error: null, revision: 0,
  async load(options) {
    try {
      const segments = [...await segment.list()].sort((a, b) => a.order - b.order);
      const filter = get().filter;
      // A filter on a segment that no longer exists falls back to „All“.
      set({ segments, error: null, revision: get().revision + (options?.crawlChanged ? 0 : 1), filter: filter?.kind === "segment" && !segments.some(s => s.name === filter.name) ? null : segments.length ? filter : null });
    } catch (error) { set({ error }); }
  },
  setFilter(filter) { set({ filter }); },
}));
export const segmentColor = (segments: SegmentDef[], name: unknown) => typeof name === "string" ? segments.find(s => s.name === name)?.color : undefined;

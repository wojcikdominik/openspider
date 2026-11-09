import { create } from "zustand";
import { compare } from "../../ipc/compare";
import { client } from "../../ipc/client";
import type { CompareOpenArgs } from "../../ipc/gen/CompareOpenArgs";
import type { CompareSession } from "../../ipc/gen/CompareSession";
import type { SortKey } from "../../ipc/gen/SortKey";
import type { RowWindow } from "../../ipc/gen/RowWindow";
import { setDetailSelection } from "../detail/frame/selection";
import { useTabs } from "../../state/tabs";

export type Side = "Current" | "Previous";
export type Category = "Added" | "New" | "Removed" | "Missing";
export type FilterComparison = { tab: string; filter: string; label: string; current: number | null; previous: number | null; change: number | null; classes: [number, number, number, number] | null };
export type Snapshot = { overview: FilterComparison[]; issues: FilterComparison[]; notes: { kind: "modes" | "migration" | "collision" | "mapping"; address?: string; winner?: string; rule?: number }[] };
export type ComparisonPage = RowWindow & { total: number; side: Side; filtersEnabled: boolean; partnerKeys?: (number | null)[]; contentAvailable?: boolean };
export type ComparisonQuery = { tab: string; filter: string; class?: Category | "changed" | "notChanged"; side: Side; search?: string; urlId?: number; source?: "Auto" | Side; change?: string; sort?: SortKey };
export const ELEMENT_FILTERS:Record<string,string>={indexability:"Indexability",page_titles:"Page Titles",meta_description:"Meta Description",h1:"H1",word_count:"Word Count",crawl_depth:"Crawl Depth",inlinks:"Inlinks",unique_inlinks:"Unique Inlinks",internal_outlinks:"Internal Outlinks",unique_internal_outlinks:"Unique Internal Outlinks",external_outlinks:"External Outlinks",unique_external_outlinks:"Unique External Outlinks",structured_data_unique_types:"Structured Data Unique Types",content:"Content",canonical:"Canonical",redirect_target:"Redirect Target"};
export const CATEGORIES: Category[] = ["Added", "New", "Removed", "Missing"];
export const LINK_CHANGES = ["All", "Added", "Removed", "Anchor Changed", "Unchanged"] as const;
interface State {
  dialog: boolean; session: CompareSession | null; snapshot: Snapshot | null; busy: boolean; error: unknown;
  revision: number; query: ComparisonQuery; selected: number | null; selectedPartner: number | null;
}
/** A new comparison starts on the tab the ordinary tab bar shows (K2-77), otherwise on Internal. */
const defaultQuery = (): ComparisonQuery => {
  const tabs = useTabs.getState();
  return { tab: tabs.catalog.some(t => t.id === tabs.active) ? tabs.active : "Internal", filter: "All", side: "Current" };
};
export const useComparison = create<State>(() => ({ dialog: false, session: null, snapshot: null, busy: false, error: null, revision: 0, query: defaultQuery(), selected: null, selectedPartner: null }));
export const showComparison = () => useComparison.setState({ dialog: true });
export function navigateComparison(tab: string, filter: string, category?: Category, side: Side = "Current") {
  useComparison.setState({ query: { tab, filter, class: category, side }, selected: null, selectedPartner: null });
  // UI-004/K2-77: the comparison uses the ordinary tab bar; a jump from the numbers selects the same tab there.
  if (useTabs.getState().active !== tab) useTabs.getState().select(tab);
  setDetailSelection({ urlIds: [] });
}
export function selectComparisonUrl(id: number, side: Side, partner: number | null = null) {
  useComparison.setState({ selected: id, selectedPartner: partner, query: { ...useComparison.getState().query, side } });
  setDetailSelection({ urlIds: [id], focus: id, topTab: useComparison.getState().query.tab,
    crawlId: useComparison.getState().session?.[side === "Current" ? "current" : "previous"].crawlId });
}
export function setComparisonSide(side: Side) {
  const s=useComparison.getState();
  if (s.query.side===side) return;
  useComparison.setState({query:{...s.query,side,class:undefined},selected:s.selectedPartner,selectedPartner:s.selected});
  if(s.selectedPartner!=null) selectComparisonUrl(s.selectedPartner,side,s.selected);
  else setDetailSelection({urlIds:[]});
}
export async function refreshComparison() {
  const result = await client.invoke<{ comparison: Snapshot }>("sidebar.overview", { comparison: true });
  useComparison.setState({ snapshot: result.comparison });
}
async function mutate(action: () => Promise<CompareSession | null>) {
  if (useComparison.getState().busy) return;
  useComparison.setState({ busy: true, error: null });
  try {
    const session = await action();
    useComparison.setState(s => ({ session, snapshot: null, selected: null, selectedPartner: null, query: defaultQuery(), revision: s.revision + 1, dialog: false }));
    setDetailSelection({ urlIds: [] });
    if (session) await refreshComparison();
  } catch (error) { useComparison.setState({ error }); }
  finally { useComparison.setState({ busy: false }); }
}
export const openComparison = (args: CompareOpenArgs) => mutate(() => compare.open(args));
export const swapComparison = () => mutate(() => compare.swap());
export const closeComparison = () => mutate(async () => { await compare.close(); return null; });
/** `columns`: the ordinary projection of the tab (canonical names); empty = the tab's default columns (core). */
export function comparisonRows(query: ComparisonQuery, offset = 0, signal?: AbortSignal, limit = 100, columns: string[] = []): Promise<ComparisonPage> {
  const session = useComparison.getState().session;
  if (!session) return Promise.reject(new Error("No comparison"));
  return client.invoke("view.rows", { viewId: `comparison:${session.current.crawlId}:${session.previous.crawlId}:${useComparison.getState().revision}`, version: session.version, offset, limit, columns,
    comparison: { ...query, current: session.current.crawlId, previous: session.previous.crawlId } }, signal);
}

/**
 * UI-004/K2-77 (B4-CMPUI): the ordinary tab bar drives the comparison. A tab chosen there opens the same tab of the comparison;
 * a catalog reload (Change Detection appears or disappears) moves a comparison off a tab that no longer exists.
 */
useTabs.subscribe((next, before) => {
  const s = useComparison.getState();
  if (!s.session || !next.catalog.some(t => t.id === next.active) || next.active === s.query.tab) return;
  if (next.active !== before.active || (next.catalog !== before.catalog && !next.catalog.some(t => t.id === s.query.tab))) navigateComparison(next.active, "All");
});

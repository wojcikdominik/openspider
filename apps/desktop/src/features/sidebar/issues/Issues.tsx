import { ComparisonNumbers } from "../../compare/ComparisonNumbers";
import { useComparison } from "../../compare/model";
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useT, useFormat, type TFn } from "../../../i18n";
import { config } from "../../../ipc/config";
import type { IssueRow } from "../../../ipc/gen/IssueRow";
import type { IssueTypeDto } from "../../../ipc/gen/IssueTypeDto";
import type { IssuePriorityDto } from "../../../ipc/gen/IssuePriorityDto";
import { useSession } from "../../../state/session";
import { useSidebarNavigation } from "../frame/state";
import { QueryFeedback } from "../frame/QueryFeedback";
import { useIssues, useRenderedAnswer } from "../frame/useSidebarQuery";
import { CountProgress } from "../frame/CountProgress";
import { IssueBadge } from "./IssueBadge";
import { IssueExplanation } from "./IssueExplanation";
import { PanelActions } from "../frame/PanelActions";
import { SEGMENT_PALETTE } from "../../../ui/tokens/tokens";
import { filterLabel, tabLabel } from "../../../i18n/canon";

const TYPES: IssueTypeDto[] = ["issue", "warning", "opportunity"];
const PRIORITIES: IssuePriorityDto[] = ["high", "medium", "low"];
/** Filter ids carry placeholders for configured thresholds (`Below {title.min_chars} Characters`); replace them by the current value (UI-064, A01-13). */
export function resolveThresholds(name: string, root: unknown): string {
  return name.replace(/\{([A-Za-z0-9_.]+)\}/g, (match, path: string) => {
    let value: unknown = root;
    for (const key of path.split(".")) value = value !== null && typeof value === "object" ? (value as Record<string, unknown>)[key] : undefined;
    return typeof value === "number" || typeof value === "string" ? String(value) : match;
  });
}
/**
 * Display name of an issue in the UI language (UI-062, UI-121): tab and filter are canonical English core names and go
 * through the same canon catalog as Overview and Dashboard; the threshold placeholder is resolved first (A01-13).
 */
export function issueLabel(t: TFn, row: Pick<IssueRow, "tab" | "filter">, root: unknown): string {
  return `${tabLabel(t, row.tab)}: ${filterLabel(t, { id: resolveThresholds(row.filter, root), labelTemplate: row.filter })}`;
}
/** Search matches the displayed (translated) name and the canonical name, so English keys keep working in German (UI-121). */
export function issueMatches(t: TFn, row: Pick<IssueRow, "tab" | "filter">, root: unknown, search: string): boolean {
  const needle = search.trim().toLocaleLowerCase();
  if (!needle) return true;
  return [issueLabel(t, row, root), `${row.tab}: ${resolveThresholds(row.filter, root)}`].some(name => name.toLocaleLowerCase().includes(needle));
}
/** Same crawl configuration query as the issues list; resolves thresholds wherever issue names are shown. */
export function useIssueConfig(): unknown {
  const crawl = useSession(s => s.crawl?.crawlId);
  const settings = useQuery({ queryKey: ["sidebar", "issues-config", crawl], queryFn: ({ signal }) => config.get(signal), enabled: !!crawl, gcTime: 0 });
  return settings.data?.config;
}
export function sortIssues(rows: readonly IssueRow[]): IssueRow[] {
  return rows.filter(r => r.count > 0).slice().sort((a, b) => TYPES.indexOf(a.type) - TYPES.indexOf(b.type) || PRIORITIES.indexOf(a.priority) - PRIORITIES.indexOf(b.priority) || b.count - a.count || `${a.tab}: ${a.filter}`.localeCompare(`${b.tab}: ${b.filter}`, "en"));
}
export function issueRows(rows: readonly IssueRow[], t: TFn): (string | number)[][] {
  return [[t("sidebar-issue-name"), t("sidebar-type"), t("sidebar-priority"), t("sidebar-urls"), t("sidebar-percent-total")], ...rows.map(r => [`${r.tab}: ${r.filter}`, r.type, r.priority, r.count, r.pct == null ? "–" : `${r.pct.toFixed(1)} %`])];
}

export function Issues({ needsAnalysis }: { needsAnalysis: boolean }) {
  const active = useComparison(s => !!s.session);
  return active ? <ComparisonNumbers issues /> : <RegularIssues needsAnalysis={needsAnalysis} />;
}
function RegularIssues({ needsAnalysis }: { needsAnalysis: boolean }) {
  const t = useT();
  const f = useFormat();
  const [type, setType] = useState<IssueTypeDto | "">("");
  const [priority, setPriority] = useState<IssuePriorityDto | "">("");
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const scope = useSidebarNavigation(s => s.scope);
  const navigate = useSidebarNavigation(s => s.navigate);
  const setScope = useSidebarNavigation(s => s.setScope);
  const crawl = useSession(s => s.crawl?.crawlId);
  // The core only knows the canonical English names; the search runs here on the displayed name (UI-121), so the
  // core receives no search text and returns the type/priority result for the client-side filter.
  const args = { ...scope, type: type || null, priority: priority || null, search: null };
  // UI-018 (M2-N27): new data versions refresh the list in the background; the last list stays visible.
  const query = useIssues(args);
  // UI-018 (B4-LAGFIX2): the end barrier waits until this list shows the reloaded answer.
  useRenderedAnswer(query, query.data);
  // Keep the same contract even when a fixture supplies an unfiltered list.
  const root = useIssueConfig();
  const named = (name: string) => resolveThresholds(name, root);
  const label = (row: IssueRow) => issueLabel(t, row, root);
  const rows = sortIssues(query.data ?? []).filter(r => (!type || r.type === type) && (!priority || r.priority === priority) && issueMatches(t, r, root, search));
  const issue = rows.find(r => r.id === selected);
  return <>
    <div className="os-sidebar-filters"><label>{t("sidebar-type")}<select aria-label={t("sidebar-type")} value={type} onChange={e => setType(e.target.value as typeof type)}><option value="">{t("sidebar-all")}</option>{TYPES.map(v => <option value={v} key={v}>{t(`sidebar-type-${v}`)}</option>)}</select></label>
    <label>{t("sidebar-priority")}<select aria-label={t("sidebar-priority")} value={priority} onChange={e => setPriority(e.target.value as typeof priority)}><option value="">{t("sidebar-all")}</option>{PRIORITIES.map(v => <option value={v} key={v}>{t(`sidebar-priority-${v}`)}</option>)}</select></label>
    <label>{t("sidebar-search")}<input type="search" value={search} onChange={e => setSearch(e.target.value)} /></label></div>
    {needsAnalysis && <p role="status">{t("sidebar-more-issues")}</p>}
    <QueryFeedback error={query.error} loading={!!crawl && query.isPending} retry={() => void query.refetch()} />
    {!!crawl && query.isPending && <CountProgress />}
    {!crawl && <p>{t("sidebar-no-crawl")}</p>}
    {query.data && <><PanelActions which="issues" rows={issueRows(rows.map(r => ({ ...r, filter: named(r.filter) })), t)} /><table className="os-sidebar-table"><thead><tr><th>{t("sidebar-issue-name")}</th><th>{t("sidebar-type")}</th><th>{t("sidebar-priority")}</th><th>{t("sidebar-urls")}</th><th>{t("sidebar-percent-total")}</th></tr></thead><tbody>{rows.map(row => <tr key={row.id} aria-selected={selected === row.id}>
      <th scope="row"><button type="button" onClick={() => { setSelected(row.id); navigate(row.tab, row.filter); }} onDoubleClick={() => navigate(row.tab, row.filter, true)}>{label(row)}</button>
      {row.segmentShares.length > 0 && <div className="os-sidebar-segments" aria-label={t("sidebar-segments")}>{row.segmentShares.map((share, i) => <button type="button" key={share.segment} title={`${share.segment}: ${f.integer(share.count)}`} aria-label={`${share.segment}: ${f.integer(share.count)}`} style={{ flexGrow: share.count, background: SEGMENT_PALETTE[i % SEGMENT_PALETTE.length]?.light }} onClick={() => setScope({ ...scope, segment: { kind: "segment", name: share.segment } })} />)}</div>}</th>
      <td><IssueBadge badge={{ issueType: row.type, priority: row.priority }} /></td><td className={`os-sidebar-priority-${row.priority}`}>{t(`sidebar-priority-${row.priority}`)}</td><td>{f.integer(row.count)}</td><td>{row.pct == null ? "–" : `${f.decimal(row.pct, 1)} %`}</td>
    </tr>)}</tbody></table>{rows.length === 0 && <p>{t("sidebar-no-issues")}</p>}</>}
    {issue && <IssueExplanation issue={issue} title={label(issue)} />}
  </>;
}

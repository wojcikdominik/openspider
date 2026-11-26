import { SUMMARY_KEYS } from "../sidebar/overview/Overview";
import { useFormat, useT } from "../../i18n";
import { filterLabel } from "../../i18n/canon";
import { useSession } from "../../state/session";
import { useTabs } from "../../state/tabs";
import { EmptyState } from "../../ui/primitives/EmptyState";
import { useIssues, useOverview, useRenderedAnswer } from "../sidebar/frame/useSidebarQuery";
import { OverviewAge } from "../sidebar/overview-age/OverviewAge";
import { QueryFeedback } from "../sidebar/frame/QueryFeedback";
import { useSidebarNavigation } from "../sidebar/frame/state";
import { issueLabel, sortIssues, useIssueConfig } from "../sidebar/issues/Issues";
import { IssueBadge } from "../sidebar/issues/IssueBadge";
import { hasCount } from "../sidebar/analysis/FilterStatus";
import "./dashboard.css";

/** Dashboard uses the same overview snapshot, issue command and navigation as the sidebar. */
export function Dashboard() {
  const t = useT(); const f = useFormat();
  const overview = useOverview();
  const crawl = useSession(s => s.crawl?.crawlId);
  const queued = useSession(s => s.crawl?.queued);
  const scope = useSidebarNavigation(s => s.scope);
  // UI-018 (M2-N27): „Top-Probleme“ keeps its last list while a new data version is counted.
  const issues = useIssues({ ...scope, type: null, priority: null, search: null });
  // UI-018 (B4-LAGFIX2): both answers are on screen as soon as they are in the cache; the end barrier sees them.
  useRenderedAnswer(overview, overview.data);
  useRenderedAnswer(issues, issues.data);
  const issueConfig = useIssueConfig();
  const navigate = (tab: string, filter: string) => {
    useTabs.getState().select(tab, filter);
    useSidebarNavigation.getState().navigate(tab, filter, true);
  };
  if (!crawl) return <section data-dashboard="" className="os-dashboard" aria-label={t("u34-nav-overview")}><EmptyState state={{ code: "NO_DATA_YET" }} /></section>;
  const data = overview.data;
  const distribution = data?.tabs.find(tab => tab.tab === "Response Codes")?.filters.filter(row => row.id !== "All") ?? [];
  const top = sortIssues(issues.data ?? []).slice(0, 5);
  return <section className="os-dashboard" aria-label={t("u34-nav-overview")} data-dashboard="">
    <QueryFeedback error={overview.error} loading={!data && overview.isPending} retry={() => void overview.refetch()} />
    {data && <OverviewAge ageMs={data.ageMs} receivedAt={overview.dataUpdatedAt} />}
    {data && <>
      <div className="os-dashboard-metrics">{([0, 1, "remaining", 7, 8] as const).map(i => {
        const key = i === "remaining" ? null : SUMMARY_KEYS[i];
        const row = data.summary.find(row => row.key === key);
        return <article className="os-dashboard-card" key={i}><span>{i === "remaining" ? t("u35-remaining") : t(`sidebar-summary-${i}`)}</span><strong>{i === "remaining" ? f.integer(queued ?? 0) : row ? f.integer(row.count) : "–"}</strong></article>;
      })}</div>
      <div className="os-dashboard-sections">
        <section className="os-dashboard-card"><h2>{t("u35-distribution")}</h2>
          {distribution.map(row => <button className="os-dashboard-row" key={row.id} onClick={() => navigate("Response Codes", row.id)}><span>{filterLabel(t, row)}</span><strong>{hasCount(row.state) ? f.integer(row.count ?? 0) : "–"}</strong><progress max={100} value={hasCount(row.state) ? row.pct ?? 0 : 0} /></button>)}
          {!distribution.length && <EmptyState state={{ code: "NO_DATA_YET" }} />}
        </section>
        <section className="os-dashboard-card"><h2>{t("u35-top-issues")}</h2>
          <QueryFeedback error={issues.error} loading={issues.isPending} retry={() => void issues.refetch()} />
          {top.map(row => <button className="os-dashboard-row" key={row.id} onClick={() => navigate(row.tab, row.filter)}><span>{issueLabel(t, row, issueConfig)}</span><strong>{f.integer(row.count)}</strong><IssueBadge badge={{ issueType: row.type, priority: row.priority }} /></button>)}
          {issues.data && !top.length && <p>{t("sidebar-no-issues")}</p>}
        </section>
      </div>
    </>}
  </section>;
}

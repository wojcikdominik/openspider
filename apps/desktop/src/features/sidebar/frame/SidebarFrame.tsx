import { ComparisonSidebar } from "../../compare/ComparisonSidebar";
import { useComparison } from "../../compare/model";
import { useCallback, useDeferredValue, useMemo, useState, type ComponentType } from "react";
import { useT } from "../../../i18n";
import { TabBar, tabId } from "../../../ui/primitives/TabBar";
import { TabContextMenuHost, TabsConfigDialog, tabContextEntries } from "../../table-advanced/TabControls";
import { useSession } from "../../../state/session";
import { useOverview, useAnalysis } from "./useSidebarQuery";
import { useSidebarLayout } from "./layout";
import { Overview, overviewRows } from "../overview/Overview";
import { Issues } from "../issues/Issues";
import { AnalysisNotice, useStartAnalysis } from "../analysis/AnalysisNotice";
import { QueryFeedback } from "./QueryFeedback";
import { CountProgress } from "./CountProgress";
import { PanelActions } from "./PanelActions";
import { OverviewAge } from "../overview-age/OverviewAge";
import { RegionBoundary } from "../../../app/ErrorBoundary";
import "../../shell/tabbar.css";
import "./sidebar.css";

/** Future sidebar packages contribute panels without editing this frame. API panels ignore the scope store. */
export interface SidebarPanel { id: string; titleKey: string; Component: ComponentType; embeddingsOnly?: boolean }
const modules = import.meta.glob<{ panel?: SidebarPanel }>("../*/index.tsx", { eager: true });
const MANDATORY = "overview";
const BASE = ["overview", "issues", "siteStructure", "segments", "responseTimes", "api", "spellingGrammar"];

export function SidebarFrame({ panels = Object.values(modules).flatMap(m => m.panel ? [m.panel] : []), embeddingsEnabled = false }: { panels?: readonly SidebarPanel[]; embeddingsEnabled?: boolean }) {
  const activeComparison = useComparison(s => !!s.session);
  const t = useT();
  const { layout, setLayout, loaded, error } = useSidebarLayout();
  const [configure, setConfigure] = useState(false);
  const crawl = useSession(s => s.crawl?.crawlId);
  const overview = useOverview();
  // Defer refreshes behind control acknowledgements, but publish the first
  // response immediately: continuous ticks can starve its background render.
  const deferredOverview = useDeferredValue(overview.data);
  const overviewData = deferredOverview ?? overview.data;
  const analysis = useAnalysis();
  const controls = useStartAnalysis();
  const ids = useMemo(() => [...new Set([...BASE, ...panels.filter(p => !p.embeddingsOnly || embeddingsEnabled).map(p => p.id)])], [panels, embeddingsEnabled]);
  const ordered = [...new Set([...layout.order.filter(id => ids.includes(id)), ...ids])];
  const hiddenTabs = layout.hidden.filter(id => id !== MANDATORY && ordered.includes(id));
  const shown = ordered.filter(id => !hiddenTabs.includes(id));
  const selected = shown.includes(layout.selected) ? layout.selected : shown[0] ?? "overview";
  const title = (id: string) => t(panels.find(p => p.id === id)?.titleKey ?? `sidebar-tab-${id}`);
  const tabLayout = { order: ordered, hidden: hiddenTabs, mandatory: MANDATORY };
  const hide = (ids: string[]) => setLayout(l => ({ ...l, hidden: [...new Set([...l.hidden, ...ids.filter(id => id !== MANDATORY)])] }));
  const reset = () => setLayout(l => ({ ...l, order: [], hidden: [], selected: MANDATORY }));
  const toggle = useCallback((id: string) => setLayout(l => ({ ...l, collapsed: l.collapsed.includes(id) ? l.collapsed.filter(x => x !== id) : [...l.collapsed, id] })), [setLayout]);
  // UI-145/UI-018 (M2-N32): every analysis poll brings new step objects; memoize by content so the memoized Overview
  // sections keep their props while the stale steps stay the same.
  const staleKey = JSON.stringify(analysis.data?.steps.filter(s => s.stale).map(s => s.step) ?? []);
  const staleSteps = useMemo(() => JSON.parse(staleKey) as string[], [staleKey]);
  const panelRows = useMemo(() => overviewData ? overviewRows(overviewData, t) : undefined, [overviewData, t]);
  const startAnalysis = useCallback(() => void controls.start(), [controls.start]);
  const needsAnalysis = !!analysis.data?.steps.some(s => s.stale || s.state === "pending") || !!overview.data?.tabs.some(tab => tab.filters.some(f => f.state.kind === "needsCrawlAnalysis"));
  const content = (id: string) => {
    if (id === "issues") return <Issues needsAnalysis={needsAnalysis} />;
    if (id === "overview") return <><QueryFeedback error={overview.error} loading={!!crawl && overview.isPending} retry={() => void overview.refetch()} />{!!crawl && overview.isPending && <CountProgress />}{!crawl && <p>{t("sidebar-no-crawl")}</p>}{overviewData && <><OverviewAge ageMs={overviewData.ageMs} receivedAt={overview.dataUpdatedAt} /><PanelActions which="overview" rows={panelRows!} /><Overview data={overviewData} staleSteps={staleSteps} collapsed={layout.collapsed} toggle={toggle} startAnalysis={startAnalysis} /></>}</>;
    const Panel = panels.find(p => p.id === id)?.Component;
    return Panel ? <Panel /> : <p>{t("sidebar-panel-unavailable")}</p>;
  };
  if (activeComparison) return <ComparisonSidebar />;
  return <div className="os-sidebar" data-sidebar-ready={loaded}>
    <QueryFeedback error={error ?? controls.error ?? analysis.error} />
    <AnalysisNotice status={analysis.data} controls={controls} />
    {/* UI-060/UI-112, A02-03/A02-06: one-line tabs with the shared overflow menu and tab context menu; Overview is the mandatory tab. */}
    <TabContextMenuHost className="os-sidebar-tabs" label={t("u20-tab-menu")} entriesFor={id => tabContextEntries(t, id, tabLayout, { hide, configure: () => setConfigure(true), reset })}>
      <TabBar panelId="sidebar-panel" tabs={shown.map(id => ({ id, label: title(id) }))} selected={selected} onSelect={id => setLayout(l => ({ ...l, selected: id }))}
        label={t("sidebar-tabs")} overflowLabel={t("sidebar-more-tabs")}
        onReorder={ids => setLayout(l => ({ ...l, order: [...ids, ...ordered.filter(id => !ids.includes(id))] }))}
        config={{ all: ordered.map(id => ({ id, label: title(id) })), hidden: hiddenTabs, mandatory: MANDATORY,
          mandatoryReason: t("u20-tab-mandatory"), configureLabel: t("u20-tabs-configure"), resetLabel: t("u20-tabs-reset"),
          onToggle: (id, visible) => visible ? setLayout(l => ({ ...l, hidden: l.hidden.filter(x => x !== id) })) : hide([id]),
          onConfigure: () => setConfigure(true), onReset: reset }} />
    </TabContextMenuHost>
    <div className="os-sidebar-panel" role="tabpanel" id="sidebar-panel" aria-labelledby={tabId("sidebar-panel", selected)}>
      <RegionBoundary key={selected} name={`sidebar-${selected}`}>{content(selected)}</RegionBoundary>
    </div>
    <TabsConfigDialog isOpen={configure} onOpenChange={setConfigure} layout={tabLayout} label={title}
      onApply={(order, hidden) => setLayout(l => ({ ...l, order, hidden }))} />
  </div>;
}

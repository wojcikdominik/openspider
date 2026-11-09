import { ComparisonControls } from "../compare/ComparisonControls";
import { useComparison } from "../compare/model";
import { useEffect, useState } from "react";
import { useT } from "../../i18n";
import { eventBus } from "../../ipc/events";
import { useSession } from "../../state/session";
import { groupMandatory, groupOf, groupTabs, useTabs } from "../../state/tabs";
import { GroupNav } from "./GroupNav";
import { TabContextMenuHost, TabsConfigDialog, tabContextEntries } from "../table-advanced/TabControls";
import { useAction } from "../../ui/actions";
import { registerMenuContributions } from "../../ui/menu/registry";
import { TabBar, type TabBarConfig } from "../../ui/primitives/TabBar";
import { tabLabel } from "../../i18n/canon";

import "../shell/tabbar.css";
import "./toptabs.css";

export function TopTabs() {
  const active = useComparison(s => !!s.session);
  // OUT-140/K2-77 (B4-CMPUI): the comparison uses the same data tabs, groups and overflow menu as the ordinary mode.
  return <div className="os-compare-tab-strip"><DataTabs /><ComparisonControls compact={!active} /></div>;
}
/** The grouped data tab bar (UI-004, K2-77); in a comparison the core adds Change Detection to the catalog. */
export function DataTabs() {
  const t = useT(); const s = useTabs();
  const [configure, setConfigure] = useState(false);
  // K2-77: the pills show the tabs of the active tab's group; configure, reset and the context menu work per group.
  const group = groupOf(s.active);
  const mine = groupTabs(s.order, group);
  const visible = mine.filter(id => !s.hidden.includes(id));
  const mandatory = groupMandatory(group, s.catalog) ?? "";
  const layout = { order: mine, hidden: s.hidden.filter(id => mine.includes(id)), mandatory };
  const fail = (error: unknown) => useTabs.setState({ error });
  const crawlId = useSession(s => s.crawl?.crawlId);
  const comparison = useComparison(c => c.session ? `${c.session.current.crawlId}:${c.session.previous.crawlId}:${c.revision}` : null);
  useEffect(() => { void useTabs.getState().load(); }, [crawlId, comparison]);
  useEffect(() => eventBus.on("focusUrl", event => {
    if (useSession.getState().crawl?.crawlId !== event.crawlId) return;
    useTabs.getState().focus(event.isInternal ? "Internal" : "External", event.urlId);
  }), []);
  useEffect(() => registerMenuContributions("toptabs", [{ menu: "view", group: "tabs", order: 30, items: [
    { id: "view.resetTabs", labelKey: "u10-tabs-reset", action: "view.resetTabs" },
  ] }]), []);
  useAction("view.resetTabs", () => { void s.reset().catch(fail); });
  const move = (direction: number) => {
    const index = visible.indexOf(s.active);
    s.select(visible[(index + direction + visible.length) % visible.length]);
  };
  useAction("toptab.next", () => move(1)); useAction("toptab.prev", () => move(-1));
  // One registration per shortcut, with a stable hook order.
  useAction("toptab.goto.1", () => s.select(visible[0]));
  useAction("toptab.goto.2", () => s.select(visible[1]));
  useAction("toptab.goto.3", () => s.select(visible[2]));
  useAction("toptab.goto.4", () => s.select(visible[3]));
  useAction("toptab.goto.5", () => s.select(visible[4]));
  useAction("toptab.goto.6", () => s.select(visible[5]));
  useAction("toptab.goto.7", () => s.select(visible[6]));
  useAction("toptab.goto.8", () => s.select(visible[7]));
  useAction("toptab.goto.9", () => s.select(visible[8]));
  const label = (id: string) => tabLabel(t, id);
  // UI-004/UI-112, A02-06: the same overflow menu as the detail and sidebar bars – visibility, clipped tabs, configure, reset.
  const config: TabBarConfig = {
    all: mine.map(id => ({ id, label: label(id) })), hidden: layout.hidden, mandatory, mandatoryReason: t("u10-tabs-internal"),
    onToggle: (id, visible) => void s.show(id, visible).catch(fail), onConfigure: () => setConfigure(true), onReset: () => void s.resetGroup(group).catch(fail),
    configureLabel: t("u20-tabs-configure"), resetLabel: t("u20-tabs-reset"),
  };
  return <div className="u10-top-tabs">
    <GroupNav />
    {s.view === "tabs" && <TabContextMenuHost className="u20-tab-host" label={t("u20-tab-menu")} entriesFor={tab => tabContextEntries(t, tab, layout, {
      hide: ids => void s.hide(ids).catch(fail), configure: () => setConfigure(true), reset: () => void s.resetGroup(group).catch(fail) })}>
      <TabBar panelId="data-panel" tabs={visible.map(id => ({ id, label: label(id) }))} className="os-tabbar-pills"
        selected={s.active} onSelect={s.select} onReorder={ids => void s.layout(ids, layout.hidden, group).catch(fail)}
        label={t("u10-tabs")} overflowLabel={t("u10-tabs-configure")} config={config} />
    </TabContextMenuHost>}
    <TabsConfigDialog isOpen={configure} onOpenChange={setConfigure} layout={layout} label={label}
      onApply={(order, hidden) => void s.layout(order, hidden, group).catch(fail)} />
    {s.error != null && <span role="alert">{t("u10-tabs-error")}</span>}
  </div>;
}

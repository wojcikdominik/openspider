import type { ComponentType } from "react";
import { BarChart3, ChevronDown, FileText, GitCompareArrows, House, Link2, Network, Puzzle, Settings2, SlidersHorizontal } from "lucide-react";
import { Button as RacButton } from "react-aria-components";
import { useT } from "../../i18n";
import { tabLabel } from "../../i18n/canon";
import { openConfig } from "../config/frame/store";
import { KNOWN_PAGES, groupTitleKey, pageTitleKey } from "../config/frame/types";
import { TAB_GROUPS, groupOf, groupTabs, useTabs, type TabGroupId } from "../../state/tabs";
import { Menu, type MenuEntry } from "../../ui/primitives/Menu";
import { cx } from "../../ui/primitives/cx";

const ICONS: Record<TabGroupId | "overview" | "config", ComponentType<{ size?: number; "aria-hidden"?: boolean }>> = {
  overview: House, urls: Link2, content: FileText, technical: Settings2, links: Network, performance: BarChart3, custom: Puzzle, changes: GitCompareArrows, config: SlidersHorizontal,
};

/** Configuration pages for „Konfiguration ▾“, in the order of the configuration dialog; grouped pages carry their group name. */
export function configEntries(t: ReturnType<typeof useT>): MenuEntry[] {
  let previous: string | undefined;
  return KNOWN_PAGES.map((page) => {
    const group = page.group ?? page.id;
    const entry: MenuEntry = { id: `config-${page.id}`, label: page.group ? `${t(groupTitleKey(page.group))}: ${t(pageTitleKey(page.id))}` : t(pageTitleKey(page.id)),
      separatorBefore: previous !== undefined && previous !== group, onAction: () => openConfig({ anchor: page.id }) };
    previous = group;
    return entry;
  });
}

/**
 * Grouped navigation above the pill tabs (UI-004, OS-Festlegung K2-77): „Übersicht“, „URLs“, the tab groups with ▾ for a direct
 * jump into one of their tabs, and „Konfiguration ▾“ for the pages of the crawl configuration. Every tab is at most two clicks away.
 */
export function GroupNav() {
  const t = useT();
  const s = useTabs();
  const current = s.view === "overview" ? "overview" : groupOf(s.active);
  const item = (id: TabGroupId | "overview", children: string) => {
    const Icon = ICONS[id];
    const tabs = id === "overview" ? [] : groupTabs(s.order, id);
    const isCurrent = current === id;
    const select = () => { if (id === "overview") s.showOverview(); else s.selectGroup(id); };
    return <div key={id} className={cx("u34-nav-item", isCurrent && "u34-nav-current")} data-group={id}>
      <RacButton className="u34-nav-button" aria-current={isCurrent ? "page" : undefined} isDisabled={id !== "overview" && !tabs.length} onPress={select}>
        <Icon size={16} aria-hidden />{children}
      </RacButton>
      {id !== "overview" && id !== "urls" && id !== "changes" && tabs.length > 0 && <Menu label={t("u34-nav-group-tabs", { group: children })}
        entries={tabs.map(tab => ({ id: `jump-${tab}`, label: tabLabel(t, tab), onAction: () => { if (s.hidden.includes(tab)) void s.show(tab, true); s.select(tab); } }))}
        trigger={<RacButton className="u34-nav-chevron" aria-label={t("u34-nav-group-tabs", { group: children })}><ChevronDown size={14} aria-hidden /></RacButton>} />}
    </div>;
  };
  const ConfigIcon = ICONS.config;
  return <div className="u34-nav" role="group" aria-label={t("u34-nav")}>
    {item("overview", t("u34-nav-overview"))}
    {/* K2-77: Change Detection is its own entry, present only while a comparison lists the tab. */}
    {TAB_GROUPS.filter(g => g.id !== "changes" || groupTabs(s.order, g.id).length > 0)
      .map(g => item(g.id, g.id === "changes" ? tabLabel(t, "Change Detection") : t(`u34-nav-${g.id}`)))}
    <div className="u34-nav-item" data-group="config">
      <Menu label={t("u34-nav-config")} entries={configEntries(t)}
        trigger={<RacButton className="u34-nav-button"><ConfigIcon size={16} aria-hidden />{t("u34-nav-config")}<ChevronDown size={14} aria-hidden /></RacButton>} />
    </div>
  </div>;
}

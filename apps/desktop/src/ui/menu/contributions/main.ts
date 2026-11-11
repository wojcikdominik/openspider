// UI-002: Fachmenüs. Aktions-IDs sind dieselben wie im Kürzelregister und in den Feature-Controllern.
import { menuContextAllows, menuContextReason } from "../../../features/help/menuContexts";
import { shortcutById } from "../../shortcuts";
import type { ExportCatalog } from "../../../ipc/gen/ExportCatalog";
import type { MenuContext, MenuContribution, MenuItemSpec, TopMenuId } from "../types";
import { middleEllipsis } from "../../recentCrawl";

const idle = (c: MenuContext) => c.crawlRunning !== true;
const data = (c: MenuContext) => c.hasCrawl === true && c.hasData !== false;
const ready = (c: MenuContext) => data(c) && idle(c);
const item = (id: string, labelKey: string, enabled?: MenuItemSpec["enabled"]): MenuItemSpec => ({
  id, action: id, labelKey,
  ...(enabled ? { enabled, disabledReasonKey: "u04-reason-unavailable" } : {}),
});
const action = (id: string, enabled?: MenuItemSpec["enabled"]) => item(id, shortcutById(id)?.labelKey ?? `u04-${id.replaceAll(".", "-")}`, enabled);
const group = (menu: TopMenuId, name: string, order: number, items: MenuItemSpec[]): MenuContribution => ({ menu, group: name, order, items });
export interface RecentEntry { path: string; label?: string }
export interface MainMenuState {
  catalog?: ExportCatalog;
  recent: readonly RecentEntry[];
  mode: string;
  mcpRunning: boolean;
  mcpAutostart: boolean;
  available?: (id: string) => boolean;
}

export function mainMenuContributions(state: MainMenuState): MenuContribution[] {
  const configSections = ["spider", "content", "robots", "rewrite", "includeExclude", "speed", "userAgent", "headers", "api", "segments", "analysis"];
  const result: MenuContribution[] = [
    group("app", "settings", 21, [{ ...action("app.settings"), id: "app.settings", labelKey: "menu-item-settings", platforms: ["mac"] }]),
    group("file", "app", 91, [{ ...action("app.settings"), id: "file.settings", labelKey: "menu-item-settings", platforms: ["win"] }, { ...action("app.quit"), id: "file.quit", labelKey: "menu-item-quit", platforms: ["win"] }]),
    group("file", "crawl", 11, [
      { ...action("crawl.new", idle), id: "file.new" },
      { ...action("crawl.open", idle), id: "file.open" },
      { id: "file.recent", labelKey: "u04-recent", items: state.recent.length ? state.recent.map((entry, i) => ({ ...item(`recent.${i}`, "", idle), labelKey: undefined, label: entry.label ?? middleEllipsis(entry.path) })) : [{ id: "recent.empty", labelKey: "u04-recent-empty", enabled: () => false }] },
      { ...action("crawl.saveAs", ready), id: "file.saveAs" },
      item("crawls.manage", "u04-crawls-manage"),
      item("crawl.import", "u04-crawl-import", idle),
      item("crawl.export", "u04-crawl-export", ready),
    ]),
    group("config", "configuration", 10, [
      ...configSections.map((s) => action(`config.${s}`, idle)),
      { id: "config.custom", labelKey: "u04-config-custom", items: ["search", "extraction", "js"].map((s) => action(`config.${s}`, idle)) },
      action("config.diff"),
      { id: "config.profiles", labelKey: "u04-config-profiles", items: ["saveAs", "load", "recent", "setDefault", "resetDefault"].map((s) => action(`profile.${s}`, idle)) },
    ]),
    group("mode", "modes", 10, ["spider", "list", "serp", "apis", "compare"].map((mode) => ({ ...action(`mode.${mode}`, idle), checked: () => state.mode === mode }))),
    group("crawlAnalysis", "analysis", 10, [action("analysis.start", (c) => ready(c) && c.analysisRunning !== true), action("config.analysis")]),
    group("view", "panes", 11, [
      action("view.focus"),
      { ...action("view.toggleDetail"), id: "view.detail" },
      { ...action("view.toggleSidebar"), id: "view.sidebar" },
      action("view.resetColumns"), action("view.resetTabs"),
    ]),
    group("view", "zoom", 21, [
      { ...action("zoom.in"), id: "view.zoomIn" }, { ...action("zoom.out"), id: "view.zoomOut" }, { ...action("zoom.reset"), id: "view.zoomReset" },
    ]),
    // F6/F7 auch ohne fn-Taste über das Menü bedienbar (UI-113).
    group("view", "navigation", 30, ["focus.next", "focus.prev", "source.nextDiff", "source.prevDiff"].map((id) => action(id))),
    group("mcp", "server", 10, [
      item("mcp.start", "u04-mcp-start", () => !state.mcpRunning), item("mcp.stop", "u04-mcp-stop", () => state.mcpRunning),
      { id: "mcp.configure", labelKey: "u04-mcp-configure", items: [action("mcp.tools")] },
      { ...action("mcp.autostart"), checked: () => state.mcpAutostart }, action("mcp.markdown"),
    ]),
    group("visualisations", "graphs", 10, ["crawlTree", "crawlGraph", "directoryTree", "directoryGraph", "crawlGraph3d", "directoryGraph3d", "anchorWords", "bodyWords", "contentCluster"].map((s) => action(`viz.${s}`, data))),
    group("help", "help", 11, [
      action("help.manual"), { ...action("help.shortcuts"), labelKey: "menu-item-shortcuts" }, action("help.usage"),
      action("help.debug"), action("help.logs"), action("help.updates"), action("help.feedback"), action("help.crashReports"),
      { ...action("app.about"), id: "help.about", labelKey: "menu-item-about" },
    ]),
  ];
  for (const menu of ["bulkExport", "reports", "sitemaps"] as const) {
    const exportItem = (entry: ExportCatalog["sitemaps"][number]): MenuItemSpec => ({
      ...item(`export.${menu === "bulkExport" ? "bulk" : menu === "reports" ? "report" : "sitemap"}.${entry.name}`, entry.labelKey, (c) => ready(c) && entry.enabled),
      label: entry.name, disabledReasonKey: entry.disabledReason ?? "u04-reason-unavailable",
    });
    const groups = menu === "bulkExport" ? state.catalog?.bulk : menu === "reports" ? state.catalog?.reports : null;
    result.push(group(menu, "catalog", 10, menu === "sitemaps" && state.catalog ? state.catalog.sitemaps.map(exportItem) : groups ? groups.map((g) => ({
      id: `export-group-${menu === "bulkExport" ? "bulk" : "reports"}-${g.key}`, labelKey: g.labelKey, label: g.key, items: g.items.map(exportItem),
    })) : [{ id: `${menu}.loading`, labelKey: "u04-catalog-unavailable", enabled: () => false, disabledReasonKey: "u04-catalog-unavailable" }]));
  }
  const activate = (entry: MenuItemSpec): MenuItemSpec => ({
    ...entry,
    ...(entry.action ? {
      enabled: (ctx: MenuContext) => menuContextAllows(entry.action!) && (state.available?.(entry.action!) ?? true) && (entry.enabled?.(ctx) ?? true),
      // Ohne registrierte Aktion hilft kein Crawl-Zustand: dann nennt der Grund die fehlende Funktion (UI-002).
      disabledReasonKey: menuContextReason(entry.action!) ?? (state.available?.(entry.action!) === false ? "u04-feature-unavailable" : entry.disabledReasonKey ?? "u04-feature-unavailable"),
    } : {}),
    ...(entry.items ? { items: entry.items.map(activate) } : {}),
  });
  return result.map((contribution) => ({ ...contribution, items: contribution.items.map(activate) }));
}


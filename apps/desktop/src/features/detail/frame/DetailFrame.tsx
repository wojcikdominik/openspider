import { ComparisonDetail } from "../../compare/ComparisonDetail";
import { useComparison } from "../../compare/model";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { RegionBoundary } from "../../../app/ErrorBoundary";
import { config } from "../../../ipc/config";
import { eventBus } from "../../../ipc/events";
import { exportApi } from "../../../ipc/export";
import type { ViewSpec } from "../../../ipc/gen/ViewSpec";
import type { Selection } from "../../../ipc/gen/Selection";
import type { ExportFormatDto } from "../../../ipc/gen/ExportFormatDto";
import { useSession } from "../../../state/session";
import { useT } from "../../../i18n";
import { Button } from "../../../ui/primitives/Button";
import { TabBar, tabId } from "../../../ui/primitives/TabBar";
import { TabContextMenuHost, TabsConfigDialog, tabContextEntries } from "../../table-advanced/TabControls";
import { Dialog } from "../../../ui/primitives/Dialog";
import { TextField, Select } from "../../../ui/primitives/fields";
import { Banner } from "../../../ui/primitives/Feedback";
import { UrlDetails } from "../url-details/UrlDetails";
import { Hreflang } from "../url-details/Hreflang";
import { Inlinks } from "../inlinks/Links";
import { Outlinks } from "../outlinks/Outlinks";
import { Images } from "../images/Images";
import { Resources } from "../resources/Resources";
import { Headers } from "../headers/Headers";
import { Cookies } from "../cookies/Cookies";
import { CrawlPath } from "../crawl-path/CrawlPath";
import { DetailRequestUrgent, DetailVisible } from "./request";
import { DetailEmpty, DetailError } from "./Feedback";
import { useDetailPreferences } from "./preferences";
import { DETAIL_BACK_EVENT, DETAIL_SHOW_TAB_EVENT, requestNavigation, useDetailSelection } from "./selection";
import { requestBrowser } from "./browser";
import { DETAIL_TABS, type DetailActions, type DetailPanelProps, type DetailPlugin, type DetailSelection, type DetailTabId } from "./types";
import "../../shell/tabbar.css";
import "./detail.css";

const extensionModules = import.meta.glob<{ detailTab?: DetailPlugin }>("../*/index.tsx", { eager: true });
const PANELS: Partial<Record<DetailTabId, DetailPlugin["Component"]>> = { urlDetails: UrlDetails, inlinks: Inlinks, outlinks: Outlinks, images: Images, resources: Resources, headers: Headers, cookies: Cookies };
const UNION_TABS = new Set<DetailTabId>(["inlinks", "outlinks", "images", "resources", "nGrams"]);
const DEFAULT_POSITIONS = ["Head", "Header", "Nav", "Content", "Sidebar", "Footer"];
/** A retained panel re-renders only when its props or its visibility change, not on every tab click (UI-030/UI-145). */
const RetainedPanel = memo(function RetainedPanel({ Panel, panelProps }: { Panel: DetailPlugin["Component"]; panelProps: DetailPanelProps }) {
  return <Panel {...panelProps} />;
});
export interface DetailFrameProps {
  selection?: DetailSelection;
  actions?: Partial<DetailActions>;
  rendering?: boolean;
  htmlStored?: boolean;
  storeUncrawlable?: boolean;
  positions?: readonly string[];
  listMode?: boolean;
  plugins?: DetailPlugin[];
}
export function DetailFrame(props: DetailFrameProps) {
  const activeComparison = useComparison(s => !!s.session);
  return activeComparison ? <ComparisonDetail /> : <RegularDetailFrame {...props} />;
}
function RegularDetailFrame(props: DetailFrameProps) {
  const t = useT(); const shared = useDetailSelection(s => s.selection); const selection = props.selection ?? shared;
  // Progress ticks do not change the detail selection or its configuration.
  // Avoid rebuilding all retained grids for every new CrawlStatus object.
  const crawlId = useSession(s => s.crawl?.crawlId);
  const crawlMode = useSession(s => s.crawl?.mode);
  const [active, setActive] = useState<DetailTabId>("urlDetails"); const [error, setError] = useState<unknown>();
  const [showCrawlPath, setShowCrawlPath] = useState(false);
  const [features, setFeatures] = useState({ rendering: false, htmlStored: false, storeUncrawlable: false, positions: DEFAULT_POSITIONS });
  const [preferences, savePreferences, loaded] = useDetailPreferences(setError);
  const preferencesRef = useRef(preferences); preferencesRef.current = preferences;
  const savePreferencesRef = useRef(savePreferences); savePreferencesRef.current = savePreferences;
  const [exporting, setExporting] = useState<{ spec: ViewSpec; selection: Selection }>();
  const [path, setPath] = useState(""); const [format, setFormat] = useState<ExportFormatDto>("csv"); const [exportBusy, setExportBusy] = useState(false);
  const [exportOp, setExportOp] = useState<string>(); const op = useSession(s => exportOp ? s.ops[exportOp] : undefined);
  const [configVersion, setConfigVersion] = useState(0);
  // M2-U21 (additiv): Fach-Tabs können einen anderen Detail-Tab anfordern (z. B. Accessibility → View Source); ein ausgeblendeter Tab wird wieder eingeblendet.
  useEffect(() => {
    const show = (event: Event) => {
      const tab = (event as CustomEvent<{ tab?: string }>).detail?.tab as DetailTabId | undefined;
      if (!tab || !DETAIL_TABS.includes(tab)) return;
      setActive(tab); setShowCrawlPath(false);
      if (preferencesRef.current.hidden.includes(tab)) savePreferencesRef.current({ ...preferencesRef.current, hidden: preferencesRef.current.hidden.filter(id => id !== tab) });
    };
    document.addEventListener(DETAIL_SHOW_TAB_EVENT, show);
    return () => document.removeEventListener(DETAIL_SHOW_TAB_EVENT, show);
  }, []);
  useEffect(() => eventBus.on("settingsChanged", () => setConfigVersion(version => version + 1)), []);
  useEffect(() => {
    const controller = new AbortController();
    void config.get(controller.signal).then(dto => {
      const cfg = dto.config as { rendering?: { mode?: string }; extraction?: { store_html?: boolean }; spider?: { store_uncrawlable_links?: boolean }; link_positions?: { rules?: { position: string }[] } };
      if (!controller.signal.aborted) setFeatures({ rendering: cfg.rendering?.mode === "javascript", htmlStored: cfg.extraction?.store_html === true, storeUncrawlable: cfg.spider?.store_uncrawlable_links === true,
        positions: [...new Set([...DEFAULT_POSITIONS, ...(cfg.link_positions?.rules ?? []).map(p => p.position)])] });
    }).catch(() => undefined);
    return () => controller.abort();
  }, [selection.crawlId, crawlId, configVersion]);
  const actions: DetailActions = useMemo(() => ({
    navigate: requestNavigation,
    back: () => { document.dispatchEvent(new CustomEvent(DETAIL_BACK_EVENT)); },
    openBrowser: requestBrowser,
    export: (spec, selection) => { setExporting({ spec, selection }); setPath(""); },
    reportError: setError,
    ...props.actions,
  }), [props.actions]);
  const panels = { ...PANELS };
  for (const plugin of [...Object.values(extensionModules).flatMap(m => m.detailTab ? [m.detailTab] : []), ...(props.plugins ?? [])]) panels[plugin.id] = plugin.Component;
  if (selection.topTab === "Hreflang") panels.urlDetails = Hreflang;
  const tab = preferences.hidden.includes(active) ? "urlDetails" : active;
  // Retain only visited, bounded table panels for the current selection. Changing the
  // selection/crawl releases every old handle. Hidden panels do not poll for updates.
  const retentionKey = JSON.stringify([selection.crawlId ?? crawlId, selection.urlIds, selection.focus, selection.topTab]);
  const selectionToken = useMemo(() => ({ key: retentionKey }), [retentionKey]);
  const [urgentSelection, setUrgentSelection] = useState<typeof selectionToken>();
  const [retained, setRetained] = useState<{ key: string; tabs: DetailTabId[] }>({ key: retentionKey, tabs: [] });
  const cacheable = ["inlinks", "outlinks", "images", "resources", "duplicates"].includes(tab);
  const visited = retained.key === retentionKey ? retained.tabs : [];
  const shown = [...new Set([...visited, ...(cacheable ? [tab] : [])])];
  // Adjust during render instead of in an effect: an effect re-rendered the whole frame a second time after every
  // tab click, before the click's detail request could leave (UI-145, ~100 ms during a live crawl).
  if (retained.key !== retentionKey || shown.length !== visited.length) setRetained({ key: retentionKey, tabs: shown });
  const Panel = panels[tab];
  const effectiveSelection = useMemo(() => ({ ...selection, crawlId: selection.crawlId ?? crawlId }), [selection, crawlId]);
  const rendering = props.rendering ?? features.rendering; const htmlStored = props.htmlStored ?? features.htmlStored;
  const storeUncrawlable = props.storeUncrawlable ?? features.storeUncrawlable; const positions = props.positions ?? features.positions;
  const listMode = props.listMode ?? crawlMode === "list";
  const panelProps: DetailPanelProps = useMemo(() => ({ selection: effectiveSelection, actions, rendering, htmlStored, storeUncrawlable, positions, listMode }),
    [effectiveSelection, actions, rendering, htmlStored, storeUncrawlable, positions, listMode]);
  const [configure, setConfigure] = useState(false);
  const layout = { order: preferences.order, hidden: preferences.hidden, mandatory: "urlDetails" };
  const reset = () => { if (!loaded) return; savePreferences({ order: [], hidden: [] }); setActive("urlDetails"); };
  const tabs = preferences.order.filter(id => !preferences.hidden.includes(id)).map(id => ({ id, label: t(`detail-tab-${id}`) }));
  const selected = selection.urlIds.length > 0 && selection.related !== false;
  const multiFocus = selection.urlIds.length > 1 && !UNION_TABS.has(tab) && !(tab === "urlDetails" && selection.topTab === "Hreflang");
  return <section className="os-detail" role="region" aria-label={t("detail-region")} onKeyDown={event => {
    if (event.altKey && event.key === "ArrowLeft") { event.preventDefault(); actions.back(); }
  }}>
    <div className="os-detail-heading"><h2>{t("detail-region")}</h2>
      <Button isDisabled={!selected || !loaded} onPress={() => setShowCrawlPath(true)}>{t("detail-tab-crawlPath")}</Button>
    </div>
    {/* UI-030/UI-112, A02-06: tab configuration through the overflow menu and the tab context menu, as on the other two bars. */}
    <TabContextMenuHost className="os-detail-tabs" label={t("detail-configure-tabs")} entriesFor={id => tabContextEntries(t, id, layout, {
      hide: ids => { if (loaded) savePreferences({ ...preferences, hidden: [...new Set([...preferences.hidden, ...ids as DetailTabId[]])] }); },
      configure: () => setConfigure(true), reset })}>
      <TabBar panelId="detail-panel" tabs={tabs} selected={tab} onSelect={id => { setUrgentSelection(selectionToken); setActive(id as DetailTabId); }} label={t("detail-tabs")} overflowLabel={t("detail-more-tabs")}
        onReorder={ids => { if (loaded) savePreferences({ ...preferences, order: [...ids, ...preferences.hidden] as DetailTabId[] }); }}
        config={{ all: preferences.order.map(id => ({ id, label: t(`detail-tab-${id}`) })), hidden: preferences.hidden, mandatory: "urlDetails",
          mandatoryReason: t("u20-tab-mandatory"), isDisabled: !loaded, configureLabel: t("u20-tabs-configure"), resetLabel: t("u20-tabs-reset"),
          onToggle: (id, visible) => savePreferences({ ...preferences, hidden: visible ? preferences.hidden.filter(h => h !== id) : [...preferences.hidden, id as DetailTabId] }),
          onConfigure: () => setConfigure(true), onReset: reset }} />
    </TabContextMenuHost>
    <TabsConfigDialog isOpen={configure} onOpenChange={setConfigure} layout={layout} label={id => t(`detail-tab-${id}`)}
      onApply={(order, hidden) => { if (loaded) savePreferences({ order: order as DetailTabId[], hidden: hidden as DetailTabId[] }); }} />
    {error ? <DetailError error={error} /> : null}
    {op && <Banner kind={op.error ? "error" : "info"}>{op.error ? t(`err-code-${op.error.code.toLowerCase()}`) : t("detail-export-status", { status: op.state })}</Banner>}
    <div className="os-detail-body" role="tabpanel" id="detail-panel" aria-labelledby={tabId("detail-panel", tab)}>
      <DetailRequestUrgent.Provider value={urgentSelection === selectionToken}>
      {!selected ? <DetailEmpty state={{ code: "NO_SELECTION" }} /> : showCrawlPath ? null : <>
        {multiFocus && <Banner>{t("detail-focus", { id: selection.focus ?? selection.urlIds[0] })}</Banner>}
        {shown.map(id => {
          const CachedPanel = panels[id];
          return CachedPanel && <div key={`${retentionKey}:${id}`} hidden={id !== tab} style={id === tab ? { display: "contents" } : undefined}>
            <DetailVisible.Provider value={id === tab}><RegionBoundary name="detail"><RetainedPanel Panel={CachedPanel} panelProps={panelProps} /></RegionBoundary></DetailVisible.Provider>
          </div>;
        })}
        {!cacheable && (Panel ? <RegionBoundary key={`${tab}:${retentionKey}`} name="detail"><Panel {...panelProps} /></RegionBoundary> : <DetailEmpty state={{ code: "NO_DATA_YET" }} />)}
      </>}
      </DetailRequestUrgent.Provider>
    </div>
    <Dialog title={t("detail-tab-crawlPath")} isOpen={showCrawlPath} onOpenChange={setShowCrawlPath} size="large"
      footer={close => <Button onPress={close}>{t("detail-close")}</Button>}>
      {selected ? <RegionBoundary key={effectiveSelection.crawlId} name="detail"><CrawlPath {...panelProps} /></RegionBoundary> : <DetailEmpty state={{ code: "NO_SELECTION" }} />}
    </Dialog>
    <Dialog title={t("detail-export")} isOpen={!!exporting} onOpenChange={open => { if (!open && !exportBusy) setExporting(undefined); }}>
      <TextField label={t("detail-export-path")} value={path} onChange={setPath} />
      <Select label={t("detail-export-format")} value={format} onChange={value => setFormat(value as ExportFormatDto)} options={[{ value: "csv", label: "CSV" }, { value: "xlsx", label: "XLSX" }]} />
      <Button isDisabled={!path.trim() || exportBusy} onPress={() => {
        if (!exporting) return; setExportBusy(true);
        void exportApi.detail({ ...exporting, path, format, ...(selection.crawlId ? { crawlId: selection.crawlId } : {}) }).then(handle => { setExportOp(handle.opId); setExporting(undefined); }).catch(setError).finally(() => setExportBusy(false));
      }}>{t("detail-export")}</Button>
    </Dialog>
  </section>;
}

import { LiveViewTotal } from "./LiveViewTotal";
import { tabId } from "../../ui/primitives/TabBar";
import { Dashboard } from "../dashboard/Dashboard";
import { ComparisonOverviewUnavailable, ComparisonTable } from "../compare/ComparisonTable";
import { useComparison } from "../compare/model";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { DataGrid, type GridController } from "../../grid/DataGrid";
import { copySelection } from "../../grid/copy";
import { currentLanguage, useT } from "../../i18n";
import { ChevronDown, Columns3, FolderTree, List, Upload } from "lucide-react";
import { columnLabel, filterLabel, tabLabel } from "../../i18n/canon";
import { crawl } from "../../ipc/crawl";
import { eventBus } from "../../ipc/events";
import { toCmdError } from "../../ipc/errors";
import { op } from "../../ipc/op";
import type { ViewSpec } from "../../ipc/gen/ViewSpec";
import { useColumns, exportColumns } from "../../state/columns";
import { usePrefs } from "../../state/prefs";
import { useSession } from "../../state/session";
import { cycleSort, defaultQuery, useTabs } from "../../state/tabs";
import { useAction } from "../../ui/actions";
import { currentPlatform } from "../../ui/platform";
import { Button, IconButton } from "../../ui/primitives/Button";
import { Dialog, DialogFooter } from "../../ui/primitives/Dialog";
import { ComboBox } from "../../ui/primitives/ComboBox";
import { Badge, Banner, ProgressBar } from "../../ui/primitives/Feedback";
import { Checkbox, SearchField } from "../../ui/primitives/fields";
import { EmptyState } from "../../ui/primitives/EmptyState";
import { Menu, MenuBody, type MenuEntry } from "../../ui/primitives/Menu";
import { Popover } from "react-aria-components";
import { Tooltip } from "../../ui/primitives/Tooltip";
import { FONT_STACKS, SIZES, resolveTokens } from "../../ui/tokens/tokens";
import { resolveTheme, systemPrefersDark } from "../../ui/tokens/theme";
import { ColumnsMenu } from "./ColumnsMenu";
import { confirmsRemoval } from "./RemovalConfirmationSettings";
import { saveTableState } from "../../state/tabs";
import { TableController } from "./controller";
import { setDetailSelection } from "../detail/frame/selection";
import { useSidebarNavigation } from "../sidebar/frame/state";
import { openExport as openExportDialog, registerExportContext } from "../export/model";
import { SelectionModel } from "../../state/selection";
import { contextSelection, openSelected, launchBrowser } from "./actions";
import { hasAction, runAction } from "../../ui/actions";
import { registerMenuContributions } from "../../ui/menu/registry";
import { viz } from "../../ipc/viz";
import { view } from "../../ipc/view";
import { segmentLabelColor } from "../../ui/tokens/tokens";
import { AdvancedSearch, parseQuery, searchErrorText, validateRules } from "../table-advanced/AdvancedSearch";
import { ColumnConfigurator } from "../table-advanced/ColumnConfigurator";
import { SegmentControls } from "../table-advanced/SegmentControls";
import { advancedContextEntries } from "../table-advanced/contextEntries";
import { formatRule } from "../table-advanced/query";
import { segmentColor, useSearchHistory, useSegments } from "../table-advanced/stores";
import "./table.css";

export function UpperTable() {
  const active = useComparison(s => !!s.session);
  const overview = useTabs(s => s.view === "overview");
  // UI-150/UI-061 (OS-Festlegung): the overview dashboard is not available in a comparison; the tabs show the comparison.
  return active ? overview ? <ComparisonOverviewUnavailable /> : <ComparisonTable /> : overview ? <Dashboard /> : <RegularUpperTable />;
}
function RegularUpperTable() {
  const t = useT(); const locale = currentLanguage(); const tabs = useTabs();
  const query = tabs.queries[tabs.active] ?? defaultQuery();
  const appearance = usePrefs(s => s.appearance);
  const crawlId = useSession(s => s.crawl?.crawlId);
  const columnState = useColumns();
  const columns = columnState.byTab[tabs.active] ?? [];
  const segments = useSegments();
  const segmentKey = `${JSON.stringify(segments.filter)}:${segments.revision}`;
  const [advanced, setAdvanced] = useState(false);
  const [configurator, setConfigurator] = useState(false);
  const [resetAll, setResetAll] = useState(false);
  const [searchError, setSearchError] = useState<string>();
  const controller = useMemo(() => new TableController(), []);
  useSyncExternalStore(controller.subscribe, controller.snapshot);
  const source = controller.source;
  const grid = useRef<GridController | null>(null);
  const search = useRef<HTMLDivElement>(null); const filterRef = useRef<HTMLDivElement>(null);
  const [text, setText] = useState(query.search);
  const [context, setContext] = useState<{ column: string; urlId?: number; x: number; y: number }>();
  const anchor = useRef<HTMLSpanElement>(null);
  const [operation, setOperation] = useState<{ id: string; total: number; action?: "recrawl" | "remove" }>();
  const status = useSession(s => operation ? s.ops[operation.id] : undefined);
  const analysis = useSession(s => s.analysis);
  const [stale, setStale] = useState(false);
  const [remove, setRemove] = useState(false); const [skipRemoval, setSkipRemoval] = useState(false);
  const [copyConfirm, setCopyConfirm] = useState(false);
  const copyResolve = useRef<((ok: boolean) => void) | undefined>(undefined);
  const confirmLargeCopy = () => new Promise<boolean>(resolve => { copyResolve.current?.(false); copyResolve.current = resolve; setCopyConfirm(true); });
  const [browserConfirm, setBrowserConfirm] = useState(false);
  const browserResolve = useRef<((ok: boolean) => void) | undefined>(undefined);
  const count = source ? controller.selection.count(source.total) : 0;
  const current = tabs.catalog.find(tab => tab.id === tabs.active);
  // UI-018 (M2-N27): while the crawl runs, filter counts come from the shared, throttled count. The active filter's
  // chip, list entry and the footer follow the live view total instead (≤ 1 s) when the view is exactly that filter.
  const liveKey = !query.search && !query.rules && !segments.filter ? `${crawlId ?? ""}|${tabs.active}|${query.filter}` : null;
  const openedFor = useRef<string | null>(null);
  const liveCountEnabled = liveKey !== null && openedFor.current === liveKey;
  const filters = (current?.filters ?? []).map(f => controller.counts.filters.find(row => row.tab === tabs.active && row.filter.id === f.id)?.filter ?? f)
    .sort((a, b) => a.id === "All" ? -1 : b.id === "All" ? 1 : 0);
  const activeFilter = filters.find(f => f.id === query.filter);
  const total = filters.find(f => f.id === "All")?.count;
  const fail = controller.fail;
  useEffect(() => { controller.activate(); return () => { controller.dispose(); browserResolve.current?.(false); copyResolve.current?.(false); }; }, [controller]);
  useEffect(() => { controller.resetCrawl(); }, [controller, crawlId]);
  // UI-006: a click in Overview/Issues sets tab, filter and search (A01-03). The store keeps the latest request, so a
  // request made before this table mounted is applied as well; every revision is applied once.
  const navigation = useSidebarNavigation(s => s.target);
  const navigated = useRef(0);
  useEffect(() => {
    if (!navigation || !tabs.loaded || navigation.revision <= navigated.current) return;
    navigated.current = navigation.revision;
    const { spec } = navigation; const store = useTabs.getState();
    if (!store.catalog.some(tab => tab.id === spec.tab)) return;
    if (store.hidden.includes(spec.tab)) void store.show(spec.tab, true).catch(fail);
    if (spec.segment) segments.setFilter(spec.segment);
    store.select(spec.tab, spec.filter);
    const search = spec.search;
    if (search?.kind === "simple") store.query({ search: search.text, rules: null });
    else if (search?.kind === "rules") store.query({ search: "", rules: { tree: search.tree, caseSensitive: search.caseSensitive, text: formatRule(search.tree) } });
    else store.query({ search: "", rules: null });
    grid.current?.scrollTo(0);
  }, [navigation, tabs.loaded]);
  // UI-030/UI-012: the detail pane follows the table selection (A01-02). Only explicit URL ids are published;
  // a select-all stays a table concern and the detail pane shows its focus row.
  const selected = controller.selection;
  useEffect(() => {
    const ids = selected.value.kind === "ids" ? selected.value.ids : selected.focus !== undefined ? [selected.focus] : [];
    setDetailSelection({ urlIds: ids, ...(selected.focus !== undefined ? { focus: selected.focus } : {}), topTab: tabs.active, ...(crawlId ? { crawlId } : {}) });
  }, [selected, tabs.active, crawlId]);
  useEffect(() => () => setDetailSelection({ urlIds: [] }), []);
  useEffect(() => {
    void columnState.load(tabs.active).catch(fail);
  }, [tabs.active, crawlId]);
  useEffect(() => {
    if (!tabs.loaded) return;
    openedFor.current = !query.search && !query.rules && !segments.filter ? `${crawlId ?? ""}|${tabs.active}|${query.filter}` : null;
    void controller.open(tabs.active, query);
    setText(query.search);
    if (query.rules) setText(query.rules.text);
  }, [controller, tabs.loaded, tabs.active, query.filter, query.search, JSON.stringify(query.sort), JSON.stringify(query.rules ?? null), crawlId, segmentKey]);
  useEffect(() => {
    void controller.refreshCounts();
    return eventBus.on("tick", tick => { void controller.tick(tick).catch(fail); });
  }, [controller, crawlId, segmentKey]);
  // Segment label colours (UI-020); stable between renders so the grid does not redraw every cell.
  const cellColors = useCallback((column: string, value: unknown) => {
    const color = column === "Segment" ? segmentColor(segments.segments, value) : undefined;
    return color ? { background: color, text: segmentLabelColor(color) } : undefined;
  }, [segments.segments]);
  const setGrid = useCallback((value: GridController | null) => { grid.current = value; }, []);
  useEffect(() => {
    const id = tabs.focusId ?? controller.selection.focus;
    if (id === undefined || !source) return;
    let live = true;
    void controller.focus(id).then(row => { if (live && row !== null) grid.current?.scrollTo(row); }).catch(fail);
    return () => { live = false; };
  }, [source, tabs.focusRequest]);
  useEffect(() => { if (status?.state === "failed" && status.error) fail(status.error); }, [status]);
  // UI-114: removed URLs cannot stay selected; the counter would otherwise keep showing "1 selected" for an empty table.
  useEffect(() => { if (operation?.action === "remove" && status?.state === "done") controller.setSelection(SelectionModel.empty()); }, [operation, status?.state, controller]);
  useEffect(() => { if (analysis && !analysis.running && analysis.steps.length && analysis.steps.every(step => !step.stale)) setStale(false); }, [analysis]);
  const busy = operation !== undefined && (!status || status.state === "running");
  const actOnSelection = async (action: "recrawl" | "remove") => {
    if (!source || !count || busy) return;
    const selection = controller.selection.forVersion(source.viewId, source.version);
    const total = selection.count(source.total); if (!total) return;
    const result = await crawl[action]({ selection: selection.value });
    setOperation({ id: result.opId, total, action }); setStale(true); setRemove(false); setContext(undefined);
  };
  const requestRemoval = async (fromKeyboard = false) => {
    if (!count || busy) return;
    if (!fromKeyboard && !await confirmsRemoval()) await actOnSelection("remove");
    else { setSkipRemoval(false); setRemove(true); }
  };
  const copy = (keys: string[], cell = false) => {
    if (!source) return;
    const selection = cell && context?.urlId !== undefined ? controller.selection.select(context.urlId) : controller.selection;
    void copySelection(source, selection, keys, { confirmLarge: confirmLargeCopy, offerExport: () => openExport(), withHeader: false }).catch(fail);
  };
  const openBrowser = () => {
    if (!source) return;
    void openSelected(source, controller.selection, () => new Promise(resolve => { browserResolve.current = resolve; setBrowserConfirm(true); }),
      launchBrowser).catch(fail);
  };
  // OUT-006/UI-015: every table export goes through the shared export dialog (format choice, suggested name, status);
  // the same snapshot backs the toolbar button, the large export button and Mod+E (A01-14).
  const exportView = source ? { kind: "view" as const, tab: tabs.active, filter: query.filter, viewId: source.viewId, version: source.version, columns: exportColumns(columns) } : null;
  const exportKey = JSON.stringify(exportView);
  useEffect(() => exportView ? registerExportContext("view", exportView) : undefined, [exportKey]);
  const openExport = (spec?: ViewSpec) => {
    setContext(undefined);
    if (spec) openExportDialog({ kind: "detail", spec, selection: controller.selection.value });
    else if (exportView) openExportDialog(exportView);
  };
  useAction("search.focus", () => search.current?.querySelector("input")?.focus());
  useAction("search.clear", () => { tabs.query({ search: "", rules: null }); setText(""); });
  useAction("view.resetAllColumns", () => setResetAll(true));
  useEffect(() => registerMenuContributions("table-advanced", [{ menu: "view", group: "tabs", order: 31, items: [
    { id: "view.resetAllColumns", labelKey: "u20-columns-reset-all", action: "view.resetAllColumns" },
  ] }]), []);
  // Advanced mode: the field takes the text syntax, parsed by the core into the rule tree (UI-011 [P1]).
  const submitSearch = (value: string) => {
    if (!advanced) { tabs.query({ search: value, rules: null }); return; }
    if (!value.trim()) { tabs.query({ search: "", rules: null }); return; }
    void (async () => {
      const parsed = await parseQuery(value);
      const checked = await validateRules(parsed, source?.columns ?? [], locale, t);
      const first = Object.values(checked.errors)[0];
      if (first) { setSearchError(first); return; }
      const rules = { tree: checked.tree, caseSensitive: query.rules?.caseSensitive ?? false, text: formatRule(checked.tree) };
      setSearchError(undefined); tabs.query({ search: "", rules }); void useSearchHistory.getState().add(rules).catch(fail);
    })().catch(error => setSearchError(searchErrorText(t, error)));
  };
  useAction("filter.open", () => filterRef.current?.querySelector("button")?.click());
  useAction("crawl.recrawl", () => { void actOnSelection("recrawl").catch(fail); });
  useAction("table.remove", () => { void requestRemoval(true).catch(fail); });
  useAction("browser.open", openBrowser);
  const showContext = (target: { column: string; urlId?: number; x: number; y: number }) => {
    if (target.urlId !== undefined) controller.setSelection(contextSelection(controller.selection, target.urlId));
    setContext(target);
  };
  useAction("context.menu", () => {
    if (controller.selection.focus === undefined) return false;
    const rect = grid.current?.bounds(1, grid.current.visible().y);
    showContext({ column: "Address", urlId: controller.selection.focus, x: rect?.x ?? 0, y: rect?.y ?? 0 });
  });
  const filterSegmentOf = async (urlId: number | undefined) => {
    if (!source || urlId === undefined) return;
    const row = await source.locate(urlId); if (row === null) return;
    const result = await view.rows({ viewId: source.viewId, version: source.version, offset: row, limit: 1, columns: ["Segment"] });
    const name = result.cells[0]?.[0];
    segments.setFilter(typeof name === "string" && name ? { kind: "segment", name } : { kind: "none" });
  };
  const entries: MenuEntry[] = context && context.urlId === undefined ? [
    { id: "pin", label: t("u10-pin-address"), onAction: () => void columnState.change(tabs.active, columns.map(c => c.key === "Address" ? { ...c, pinned: !c.pinned } : c)).catch(fail) },
    { id: "columns-configure", label: t("u20-columns-title"), onAction: () => setConfigurator(true) },
    { id: "columns-reset-all", label: t("u20-columns-reset-all"), onAction: () => setResetAll(true) },
  ] : [
    { id: "copy-rows", label: t("u10-copy-rows"), onAction: () => copy(exportColumns(columns)) },
    { id: "copy-url", label: t("u10-copy-url"), onAction: () => copy(["Address"]) },
    { id: "copy-cell", label: t("u10-copy-cell"), onAction: () => copy([context?.column ?? "Address"], true) },
    { id: "copy-column", label: t("u10-copy-column"), onAction: () => copy([context?.column ?? "Address"]) },
    { id: "browser", label: t("u10-browser"), onAction: openBrowser },
    { id: "recrawl", label: t("u10-recrawl"), onAction: () => void actOnSelection("recrawl").catch(fail), isDisabled: busy, disabledReason: busy ? t("u10-busy") : undefined },
    { id: "remove", label: t("u10-remove"), onAction: () => { void requestRemoval().catch(fail); }, isDisabled: busy, disabledReason: busy ? t("u10-busy") : undefined },
    ...(["inlinks", "outlinks", "images", "resources", "crawlPath"] as const).map(kind => ({ id: `export-${kind}`, label: t(`u10-export-${kind}`), onAction: () => {
      const urlIds = controller.selection.value.kind === "ids" ? controller.selection.value.ids : [];
      openExport(kind === "crawlPath" ? { kind: "detail", detail: "crawlPath", urlIds, params: null, sort: [] } :
        kind === "images" ? { kind, urlIds, sort: [] } : { kind, urlIds, filter: { id: "All" }, sort: [] });
    } })),
    ...advancedContextEntries(t, { count, hasSegments: segments.segments.length > 0, hasAction }, {
      run: id => { runAction(id); },
      viz: kind => { const id = controller.selection.focus ?? context?.urlId; void viz.open({ kind, rootUrlId: id ?? null }).catch(fail); },
      filterSegment: () => { void filterSegmentOf(context?.urlId ?? controller.selection.focus).catch(fail); },
    }),
  ];
  const available = count > 0;
  const menuEntries = entries.map(e => ({ ...e, isDisabled: e.isDisabled || (!available && context?.urlId !== undefined),
    disabledReason: e.disabledReason ?? (!available ? t("u10-no-selection") : undefined), onAction: () => { e.onAction?.(); setContext(undefined); } }));
  // UI-016 (B3-UI): without an open crawl the core answers `NoCrawl`; that is the state NO_DATA_YET with its padded
  // hint, not an error line above the table plus NO_MATCH.
  const noCrawl = controller.error != null && toCmdError(controller.error).code === "NoCrawl";
  const empty = activeFilter?.state.kind === "needsCrawlAnalysis" ? { code: "NEEDS_CRAWL_ANALYSIS" as const } :
    activeFilter?.state.kind === "needsConfig" ? { code: "FEATURE_DISABLED" as const, arg: activeFilter.state.key } :
    activeFilter?.state.kind === "needsRendering" ? { code: "NEEDS_JS_RENDERING" as const } :
    noCrawl ? { code: "NO_DATA_YET" as const } : null;
  return <section className="u10-table" data-shortcut-scope="table" role="tabpanel" id="data-panel" aria-labelledby={tabs.view === "tabs" ? tabId("data-panel", tabs.active) : undefined} aria-label={tabs.view === "tabs" ? undefined : t("u10-table")}>
    <div className="u10-toolbar" key={tabs.active}>
      <LiveViewTotal source={source} enabled={liveCountEnabled} runningOnly>{liveTotal => {
        const liveFilters = filters.map(f => liveTotal !== undefined && f.id === query.filter && f.state.kind === "ready" ? { ...f, count: liveTotal } : f);
        const liveActive = liveFilters.find(f => f.id === query.filter);
        return <>
          {/* K2-77: the first filters of the tab (and the active one) as chips with counts; all filters stay in the dropdown. */}
          <div className="u34-chips" role="group" aria-label={t("u34-quick-filters")}>
            {[...liveFilters.slice(0, 3), ...(liveActive && !liveFilters.slice(0, 3).includes(liveActive) ? [liveActive] : [])].map(f => {
              const count = f.state.kind === "needsCrawlAnalysis" || f.count == null ? "" : ` (${new Intl.NumberFormat(locale).format(f.count)})`;
              return <button key={f.id} type="button" className="os-btn u34-chip" title={`${filterLabel(t, f)}${count}`} aria-pressed={query.filter === f.id} onClick={() => tabs.query({ filter: f.id })}>{filterLabel(t, f)}{count}</button>;
            })}
          </div>
          <div ref={filterRef}><ComboBox label={t("u10-filter")} selected={query.filter} onSelect={filter => tabs.query({ filter })}
            formatCount={n => new Intl.NumberFormat(locale).format(n)} options={liveFilters.map(f => ({ id: f.id,
              label: filterLabel(t, f),
              count: f.state.kind === "needsCrawlAnalysis" ? null : f.count ?? null,
              hint: f.state.kind === "ready" ? undefined : t(`u10-filter-${f.state.kind}`),
              badge: f.issue ? <><Badge tone={f.issue.issueType === "issue" ? "danger" : f.issue.issueType === "warning" ? "warning" : "neutral"}>{t(`u10-issue-${f.issue.issueType}`)}</Badge>
                <Badge tone={f.issue.priority === "high" ? "danger" : f.issue.priority === "medium" ? "warning" : "neutral"}>{t(`u10-priority-${f.issue.priority}`)}</Badge></> : undefined,
            }))} /></div>
        </>;
      }}</LiveViewTotal>
      <SegmentControls crawlId={crawlId} />
      <div ref={search} className="u20-search" data-invalid={searchError ? "true" : undefined}>
        <SearchField label={t(advanced ? "u20-search-syntax-field" : "u10-search")} placeholder={advanced ? undefined : `${t("u10-search")} …`} value={text} onChange={value => { setText(value); setSearchError(undefined); }}
          isLoading={controller.pending && Boolean(query.search || query.rules)}
          onSubmit={submitSearch} onClear={() => { setText(""); setSearchError(undefined); tabs.query({ search: "", rules: null }); }} clearLabel={t("u10-clear-search")} />
        <Button aria-label={t("u20-search-advanced")} aria-expanded={advanced} onPress={() => setAdvanced(!advanced)}>{advanced ? "▴" : "▾"}</Button>
      </div>
      <div className="u34-segmented" role="group" aria-label={t("u34-view-mode")}>
        <button type="button" className="os-btn u34-segment" aria-pressed="true" aria-label={t("u34-view-list")} title={t("u34-view-list")}><List size={16} aria-hidden /></button>
        <button type="button" className="os-btn u34-segment" aria-pressed="false" disabled aria-label={t("u34-view-tree")} title={t("u34-view-tree-pending")}><FolderTree size={16} aria-hidden /></button>
      </div>
      {/* UI-015: one export entry above the table – the view export, ▾ for the multi export (K2-77 design: „Exportieren ▾“). */}
      <div className="u34-split">
        <Button onPress={() => openExport()} isDisabled={!source}><Upload size={16} aria-hidden />{t("u10-export")}</Button>
        <Menu label={t("u34-export-more")} entries={[
          { id: "export-view", label: t("u34-export-view"), isDisabled: !source, onAction: () => openExport() },
          { id: "export-multi", label: t("u34-export-multi"), onAction: () => { runAction("export.multi"); } },
        ]} trigger={<IconButton label={t("u34-export-more")} icon={<ChevronDown size={14} />} />} />
      </div>
      <Tooltip text={t("u20-columns-open")}><IconButton label={t("u20-columns-open")} icon={<Columns3 size={16} aria-hidden="true" />} onPress={() => setConfigurator(true)} isDisabled={!columns.length} /></Tooltip>
      <ColumnsMenu columns={columns} onChange={next => void columnState.change(tabs.active, next).catch(fail)} onReset={() => void columnState.reset(tabs.active).catch(fail)} />
    </div>
    {searchError && <p role="alert" className="u20-error">{searchError}</p>}
    {advanced && <AdvancedSearch columns={source?.columns ?? columnState.metadata[tabs.active] ?? []} value={query.rules} locale={locale}
      onApply={rules => { setSearchError(undefined); setText(rules.text); tabs.query({ search: "", rules }); }}
      onClear={() => { setText(""); tabs.query({ search: "", rules: null }); }} />}
    {/* B3-UI: messages above the table use the Banner primitive inside the toolbar's inline padding (tokens). */}
    {controller.error != null && !noCrawl && <div className="u10-message"><Banner kind="error">{/^err-search/.test(toCmdError(controller.error).messageKey) || toCmdError(controller.error).code === "Timeout"
      ? searchErrorText(t, controller.error) : t(toCmdError(controller.error).messageKey, toCmdError(controller.error).args)}</Banner></div>}
    {stale && <p role="status" className="u10-message">{t("u10-stale")}</p>}
    {operation && <div role="status" className="u10-operation">
      <span>{t(`u10-op-${status?.state ?? "running"}`)}</span>
      <progress max={status?.total ?? operation.total} value={status?.done ?? 0} />
      <span>{new Intl.NumberFormat(locale).format(status?.done ?? 0)} / {new Intl.NumberFormat(locale).format(status?.total ?? operation.total)}</span>
      {busy && <Button onPress={() => void op.cancel({ opId: operation.id }).catch(fail)}>{t("u10-cancel")}</Button>}
    </div>}
    <div className="u10-grid-host">
      {empty ? <EmptyState state={empty} /> : source && columns.length > 0 ?
        <DataGrid source={source} columns={columns.map(c => ({ ...c, sort: query.sort.find(s => s.column === c.key)?.dir }))}
          selection={controller.selection} onSelectionChange={controller.setSelection}
          cellColors={cellColors}
          onSortChange={sort => tabs.query({ sort })} onSortGesture={(column, extend) => tabs.query({ sort: cycleSort(query.sort, column, extend) })}
          headerTitle={column => { const index = query.sort.findIndex(s => s.column === column); const title = columnLabel(t, column); return index < 0 ? title : `${title} ${query.sort[index].dir === "asc" ? "↑" : "↓"}${query.sort.length > 1 ? index + 1 : ""}`; }}
          onColumnsChange={next => void columnState.change(tabs.active, next).catch(fail)}
          onContextMenu={showContext} onActivate={() => undefined} onError={fail} controller={setGrid}
          confirmLargeCopy={confirmLargeCopy} onExportSelection={() => openExport()}
          rowHeight={SIZES.rowHeight[appearance.density]} locale={locale} ariaLabel={t("u10-table")}
          theme={{ tokens: resolveTokens(resolveTheme(appearance.theme, systemPrefersDark()), appearance.accent),
            fontFamily: FONT_STACKS[currentPlatform()].ui, fontSize: SIZES.fontTable[appearance.density], zebra: appearance.zebra }} /> :
        controller.pending ? <ProgressBar label={t("u04-loading")} /> : <EmptyState state={{ code: "NO_MATCH" }} />}
    </div>
    <LiveViewTotal source={source}>{liveTotal => {
      const shown = empty ? 0 : liveTotal ?? 0;
      return <div className="u10-counter" role="status">{t("u10-count", { shown, total: total == null && !(source && liveCountEnabled && query.filter === "All") ? "–" : Math.max(total ?? 0, shown) })}
        {(query.search || query.rules) && ` · ${t("u10-search-active")}`}{count > 0 && ` · ${t("u10-selected", { count })}`}</div>;
    }}</LiveViewTotal>
    <span ref={anchor} className="u10-menu-anchor" style={{ left: context?.x ?? 0, top: context?.y ?? 0 }} />
    <Popover triggerRef={anchor} isOpen={context !== undefined} onOpenChange={open => { if (!open) setContext(undefined); }} className="os-popover">
      <MenuBody entries={menuEntries} label={t("u10-context")} />
    </Popover>
    <ColumnConfigurator tab={tabs.active} isOpen={configurator} onOpenChange={setConfigurator}
      tabs={tabs.order.map(id => ({ id, label: tabLabel(t, id) }))} label={id => tabLabel(t, id)} />
    <Dialog title={t("u20-columns-reset-all")} isOpen={resetAll} onOpenChange={setResetAll}
      footer={<DialogFooter okLabel={t("u20-reset")} cancelLabel={t("u10-cancel")} okVariant="danger"
        onOk={() => { setResetAll(false); void columnState.resetAll().catch(fail); }} onCancel={() => setResetAll(false)} />}>
      <p>{t("u20-columns-reset-all-confirm")}</p>
    </Dialog>
    <Dialog title={t("u10-remove")} isOpen={remove} onOpenChange={setRemove}
      footer={<DialogFooter okLabel={t("u10-remove")} cancelLabel={t("u10-cancel")} okVariant="danger"
        onOk={() => void (async () => { if (skipRemoval) await saveTableState({ tableConfirmRemove: false }); await actOnSelection("remove"); })().catch(fail)} onCancel={() => setRemove(false)} />}>
      <p>{t("u10-remove-confirm", { count })}</p>
      <Checkbox isSelected={skipRemoval} onChange={setSkipRemoval}>{t("u10-dont-ask")}</Checkbox>
    </Dialog>
    <Dialog title={t("u10-copy-rows")} isOpen={copyConfirm} onOpenChange={open => { setCopyConfirm(open); if (!open) copyResolve.current?.(false); }}
      footer={<DialogFooter okLabel={t("u10-copy-rows")} cancelLabel={t("u10-cancel")}
        onOk={() => { copyResolve.current?.(true); setCopyConfirm(false); }} onCancel={() => { copyResolve.current?.(false); setCopyConfirm(false); }} />}>
      {t("u10-large-copy")}
    </Dialog>
    <Dialog title={t("u10-browser")} isOpen={browserConfirm} onOpenChange={open => { setBrowserConfirm(open); if (!open) browserResolve.current?.(false); }}
      footer={<DialogFooter okLabel={t("u10-open")} cancelLabel={t("u10-cancel")}
        onOk={() => { browserResolve.current?.(true); setBrowserConfirm(false); }} onCancel={() => { browserResolve.current?.(false); setBrowserConfirm(false); }} />}>
      {t("u10-browser-confirm", { count: Math.min(10, count) })}
    </Dialog>
  </section>;
}

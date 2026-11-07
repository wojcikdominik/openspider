import { useContext, useEffect, useRef, useState } from "react";
import { Popover } from "react-aria-components";
import { view } from "../../../ipc/view";
import { toCmdError } from "../../../ipc/errors";
import type { ViewSpec } from "../../../ipc/gen/ViewSpec";
import type { SortKey } from "../../../ipc/gen/SortKey";
import { ViewDataSource, LOADING } from "../../../data/ViewDataSource";
import { DataGrid, defaultColumns } from "../../../grid/DataGrid";
import { SelectionModel } from "../../../state/selection";
import { useSession } from "../../../state/session";
import { usePrefs } from "../../../state/prefs";
import { useLanguageStore, useT } from "../../../i18n";
import { columnLabel } from "../../../i18n/canon";
import { FONT_STACKS, resolveTokens, SIZES } from "../../../ui/tokens/tokens";
import { resolveTheme, systemPrefersDark } from "../../../ui/tokens/theme";
import { currentPlatform } from "../../../ui/platform";
import { MenuBody, type MenuEntry } from "../../../ui/primitives/Menu";
import { Button } from "../../../ui/primitives/Button";
import { copySelection } from "../../../grid/copy";
import { DetailEmpty, DetailError, DetailLoading } from "./Feedback";
import { useDetailColumns } from "./columns";
import { DetailVisible, useDetailRequest } from "./request";
import { displayValue } from "../url-details/UrlDetails";
import type { DetailPanelProps } from "./types";

export interface TablePanelProps extends DetailPanelProps {
  spec: ViewSpec;
  columns: readonly string[];
  /** Used when the core rejects `columns` with err-unknown-column (columns the core does not offer yet). */
  fallbackColumns?: readonly string[];
  hiddenColumns?: readonly string[];
  title: string;
  onSort: (sort: SortKey[]) => void;
  onSelectRow?: (get: (column: string) => string, rowKey?: number) => void;
  onActivateRow?: (get: (column: string) => string, rowKey?: number) => void;
  /** M2-U21 (additiv): Zusatzeinträge im Kontextmenü einer Zeile, nach den Kopieraktionen. */
  contextEntries?: (row: { key?: number; get: (column: string) => string }) => MenuEntry[];
  /** M2-U21 (additiv): Kontextmenü ohne die Link-Endpunkt-Einträge (From/To/Quelltext) für Tabs ohne Linkzeilen. */
  plainContext?: boolean;
  columnHints?: Record<string, string>;
  columnTitles?: Record<string, string>;
  pinAddress?: boolean;
}
type Target = { column: string; row?: number; x: number; y: number };
const NO_HIDDEN: readonly string[] = [];

/** Retain the cache and viewport while preventing hidden grids from requesting rows. */
class DetailDataSource extends ViewDataSource {
  active = true;
  private requestedWindow: [number, number] = [0, 29];
  override async ensureRange(first: number, last: number): Promise<void> {
    if (!this.active) return;
    this.requestedWindow = [first, last];
    await super.ensureRange(first, last);
  }
  resumeRange() { return this.ensureRange(...this.requestedWindow); }
}

export function TablePanel(props: TablePanelProps) {
  const { spec, title, actions, onSort, hiddenColumns = NO_HIDDEN } = props;
  const visible = useContext(DetailVisible);
  const currentlyVisible = useRef(visible);
  currentlyVisible.current = visible;
  const t = useT(); const appearance = usePrefs(s => s.appearance); const locale = useLanguageStore(s => s.lang);
  const [selection, setSelection] = useState(SelectionModel.empty);
  const layoutId = spec.kind === "detail" ? spec.detail : spec.kind === "inlinks" || spec.kind === "outlinks" ? `${spec.kind}:${spec.filter.mode ?? "links"}` : spec.kind;
  const [columnLayout, setColumns] = useDetailColumns(layoutId, actions.reportError);
  const [, refresh] = useState(0);
  const currentKey = useRef("");
  const [target, setTarget] = useState<Target>(); const anchor = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<unknown>();
  const [largeCopy, setLargeCopy] = useState<{ rows: number; resolve: (yes: boolean) => void }>();
  const copyPending = useRef<((yes: boolean) => void) | undefined>(undefined);
  const key = JSON.stringify([spec, props.columns, props.selection.crawlId]);
  currentKey.current = key;
  const result = useDetailRequest(key, async signal => {
    // Do not let the client discard a late handle on cancellation: it must still be closed.
    void signal;
    let handle: Awaited<ReturnType<typeof view.open>>;
    try { handle = await view.open({ spec, columns: [...props.columns] }); }
    catch (error) {
      if (!props.fallbackColumns || toCmdError(error).messageKey !== "err-unknown-column") throw error;
      handle = await view.open({ spec, columns: [...props.fallbackColumns] });
    }
    let source: DetailDataSource;
    try { source = new DetailDataSource(handle); }
    catch (error) { await view.close({ viewId: handle.viewId }).catch(actions.reportError); throw error; }
    source.setSorted(spec.sort.length > 0);
    // Fetch the bounded first window before mounting the grid. Otherwise the
    // first rows wait for another React commit/effect during live tick traffic.
    // A handle that finishes after hiding the tab stays lazy until resume.
    source.active = currentlyVisible.current && !signal.aborted;
    await source.ensureRange(0, 29);
    return { handle, source };
  }, value => { void value.source.dispose().catch(actions.reportError); });
  const source = result.value?.source;
  if (source) source.active = visible;
  // A schema switch (Links → Anchors) must never render old keys against new metadata.
  const defaults = source ? defaultColumns(source.columns).map(c => ({ ...c,
    pinned: c.key === "Address" && props.pinAddress === false ? false : c.pinned,
    visible: !hiddenColumns.includes(c.key) && c.visible,
  })) : [];
  const columns = [
    ...columnLayout.filter(c => defaults.some(meta => meta.key === c.key)),
    ...defaults.filter(c => !columnLayout.some(saved => saved.key === c.key)),
  ];
  const changed = useSession(s => source ? s.views[source.viewId] : undefined);
  // UI-145/UI-143 (M2-N32): the data revision is read when the refresh timer fires, not subscribed for rendering;
  // a live tick no longer re-renders every visible detail table (and its grid) only to update a ref.
  const appliedRevision = useRef<{ source: ViewDataSource; revision: string } | undefined>(undefined);
  useEffect(() => {
    if (!source || !visible) return;
    let live = true;
    const latestRevision = { get current() { const s = useSession.getState(); return `${s.countsVersion}:${s.sidebarVersion}`; } };
    if (appliedRevision.current?.source !== source) appliedRevision.current = { source, revision: latestRevision.current };
    let applied = appliedRevision.current.revision;
    let pending = false;
    let stale = false;
    source.onStale = () => { stale = true; };
    const timer = setInterval(() => {
      if (pending || (!stale && applied === latestRevision.current)) return;
      stale = false;
      const updating = latestRevision.current;
      pending = true;
      void view.update({ viewId: source.viewId, spec }).then(async handle => {
        if (!live) return;
        await source.onViewChanged(handle.version, handle.total ?? 0);
        // Only acknowledge data actually adopted by this visible effect.
        if (!live || source.version < handle.version) return;
        setError(undefined);
        applied = updating;
        appliedRevision.current = { source, revision: updating };
      }).catch(error => { if (live) { stale = true; setError(error); } }).finally(() => { pending = false; });
    }, 500);
    return () => { live = false; source.onStale = undefined; clearInterval(timer); };
    // The source belongs to key (including spec); ticks only queue another refresh.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source, visible]);
  useEffect(() => {
    if (!source) return;
    setSelection(SelectionModel.empty()); setTarget(undefined); setError(undefined);
    source.onError = setError;
    const unsubscribe = source.subscribe(() => refresh(n => n + 1));
    return unsubscribe;
  }, [source, hiddenColumns]);
  useEffect(() => { if (source && visible) void source.resumeRange().catch(setError); }, [source, visible]);
  useEffect(() => {
    if (source && changed && visible) void source.onViewChanged(changed.version, changed.total ?? source.total).catch(setError);
  }, [source, changed, visible]);
  useEffect(() => () => { copyPending.current?.(false); }, []);
  const exporting = () => actions.export(spec, { kind: "ids", ids: "urlIds" in spec ? spec.urlIds : props.selection.urlIds });
  if (result.error) return <DetailError error={result.error} />;
  if (!result.value || !source) return <DetailLoading />;
  if (!source.total && result.value.handle.emptyState) return <DetailEmpty state={result.value.handle.emptyState} />;
  if (result.value.handle.state === "building" && !source.total) return <DetailLoading />;
  const get = (row: number, column: string) => {
    const value = source.getCell(source.columns.findIndex(c => c.key === column), row);
    return value === LOADING ? "" : displayValue(value);
  };
  // A02-04: canonical column keys are shown translated unless the detail tab names them itself.
  const columnTitle = (key: string) => props.columnTitles?.[key] ?? columnLabel(t, key);
  const titles = Object.fromEntries(columns.map(c => [c.key, columnTitle(c.key)]));
  const copy = (text: string) => { void navigator.clipboard.writeText(text).catch(actions.reportError); };
  const entries: MenuEntry[] = target ? columns.map(c => ({ id: `column-${c.key}`, label: `${c.visible ? "✓ " : ""}${columnTitle(c.key)}`,
    onAction: () => setColumns(columns.map(item => item.key === c.key ? { ...item, visible: !item.visible } : item)),
  })) : [];
  if (target?.row !== undefined) {
    const row = target.row; const from = get(row, "From"); const to = get(row, "To"); const path = get(row, "Link Path");
    const extra = props.contextEntries?.({ key: source.rowKey(row), get: column => get(row, column) }) ?? [];
    entries.unshift(
      { id: "copy-cell", label: t("detail-copy-cell"), onAction: () => copy(get(row, target.column)) },
      { id: "copy-row", label: t("detail-copy-row"), onAction: () => copy(columns.filter(c => c.visible).map(c => get(row, c.key).replace(/[\t\r\n]/g, " ")).join("\t")) },
      ...extra,
      ...(props.plainContext ? [] : [...(["From", "To"] as const).flatMap(side => {
        const address = side === "From" ? from : to;
        return [
          { id: `copy-${side}`, label: t("detail-copy-endpoint", { side }), isDisabled: !address, onAction: () => copy(address) },
          { id: `show-${side}`, label: t("detail-show-endpoint", { side }), isDisabled: !address, onAction: () => actions.navigate({ address, tab: "auto" }) },
          { id: `open-${side}`, label: t("detail-open-endpoint", { side }), isDisabled: !/^https?:\/\//i.test(address), onAction: () => actions.openBrowser(address) },
        ];
      }),
      { id: "source", label: t("detail-show-source"), isDisabled: !from || !path || !props.htmlStored, disabledReason: !props.htmlStored ? t("detail-html-not-stored") : !path ? t("detail-no-path") : undefined,
        onAction: () => actions.navigate({ address: from, tab: "auto", source: true, linkPath: path }) }]),
      { id: "export", label: t("detail-export"), onAction: exporting },
    );
  }
  const activate = async (id: number) => {
    const row = await source.locate(id); if (row === null) return;
    await source.ensureRow(row);
    if (currentKey.current !== key) return;
    if (props.onActivateRow) props.onActivateRow(column => get(row, column), source.rowKey(row));
    else {
      const address = get(row, spec.kind === "inlinks" ? "From" : spec.kind === "detail" ? "Address" : "To");
      if (address) actions.navigate({ address, tab: "auto" });
    }
  };
  return <>
    <div className="os-detail-table-tools">
      <details><summary>{t("detail-columns")}</summary>{columns.map(c => <label key={c.key} title={props.columnHints?.[c.key]}><input type="checkbox" checked={c.visible} onChange={event => setColumns(columns.map(item => item.key === c.key ? { ...item, visible: event.target.checked } : item))} />{columnTitle(c.key)}</label>)}</details>
      <Button onPress={() => { void copySelection(source, selection, columns.filter(c => c.visible).map(c => c.key), {
        confirmLarge: rows => new Promise(resolve => { copyPending.current = resolve; setLargeCopy({ rows, resolve }); }), offerExport: exporting,
      }).catch(actions.reportError); }}>{t("detail-copy-selection")}</Button>
      <Button onPress={exporting}>{t("detail-export")}</Button>
    </div>
    {error ? <DetailError error={error} /> : null}
    {largeCopy && <div role="alertdialog" aria-label={t("detail-copy-confirm")}><p>{t("detail-copy-large", { rows: largeCopy.rows })}</p>
      <Button onPress={() => { largeCopy.resolve(true); setLargeCopy(undefined); }}>{t("detail-confirm")}</Button>
      <Button onPress={() => { largeCopy.resolve(false); setLargeCopy(undefined); }}>{t("detail-cancel")}</Button></div>}
    {source.total === 0 ? <DetailEmpty state={{ code: "NO_MATCH" }} /> : <div className="os-detail-grid" onKeyDown={event => {
      if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) { event.preventDefault();
        const focus = selection.focus; if (focus !== undefined) void source.locate(focus).then(row => { if (row !== null) setTarget({ row, column: columns.find(c => c.visible)?.key ?? "", x: 0, y: 0 }); }).catch(actions.reportError);
      }
    }}>
      <DataGrid source={source} columns={columns} selection={selection} onSelectionChange={next => {
        setSelection(next);
        if (next.focus !== undefined && props.onSelectRow) void source.locate(next.focus).then(async row => {
          if (row === null) return; await source.ensureRow(row); if (currentKey.current !== key) return; props.onSelectRow?.(column => get(row, column), source.rowKey(row));
        }).catch(actions.reportError);
      }}
        onColumnsChange={setColumns} onSortChange={sort => { setColumns(columns.map(c => ({ ...c, sort: sort.find(s => s.column === c.key)?.dir }))); onSort(sort); }}
        onContextMenu={setTarget} onActivate={id => { void activate(id).catch(actions.reportError); }} onError={setError}
        confirmLargeCopy={rows => new Promise(resolve => { copyPending.current = resolve; setLargeCopy({ rows, resolve }); })} onExportSelection={exporting}
        columnHints={props.columnHints} columnTitles={titles}
        rowHeight={SIZES.rowHeight[appearance.density]} locale={locale} ariaLabel={title}
        theme={{ tokens: resolveTokens(resolveTheme(appearance.theme, systemPrefersDark()), appearance.accent), fontFamily: FONT_STACKS[currentPlatform()].ui, fontSize: SIZES.fontTable[appearance.density], zebra: appearance.zebra }} />
    </div>}
    <div ref={anchor} style={{ position: "fixed", left: target?.x ?? 0, top: target?.y ?? 0, width: 1, height: 1 }} />
    <Popover triggerRef={anchor} isOpen={!!target} onOpenChange={open => { if (!open) setTarget(undefined); }} className="os-popover" placement="bottom start">
      <MenuBody label={t("detail-context")} entries={entries.map(e => ({ ...e, onAction: () => { e.onAction?.(); setTarget(undefined); } }))} />
    </Popover>
  </>;
}

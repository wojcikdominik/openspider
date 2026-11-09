import { useEffect, useMemo, useState } from "react";
import { BLOCK_SIZE, ViewDataSource } from "../../data/ViewDataSource";
import { DataGrid, defaultColumns, type ColumnState } from "../../grid/DataGrid";
import { currentLanguage, useT } from "../../i18n";
import { columnLabel } from "../../i18n/canon";
import { catalog } from "../../ipc/catalog";
import type { ColumnMeta } from "../../ipc/gen/ColumnMeta";
import type { CopyArgs } from "../../ipc/gen/CopyArgs";
import type { SortKey } from "../../ipc/gen/SortKey";
import { SelectionModel } from "../../state/selection";
import { usePrefs } from "../../state/prefs";
import { currentPlatform } from "../../ui/platform";
import { FONT_STACKS, SIZES, resolveTokens } from "../../ui/tokens/tokens";
import { resolveTheme, systemPrefersDark } from "../../ui/tokens/theme";
import { comparisonRows, type ComparisonPage, type ComparisonQuery, type Side } from "./model";
import "./compare.css";

/** Columns the comparison adds to the ordinary projection (OUT-142/OUT-151). They are not sortable in the core. */
export const COMPARISON_COLUMNS = ["Comparison Status", "Previous Address"];
export type LoadPage = (offset: number, limit: number, signal?: AbortSignal) => Promise<ComparisonPage>;

/** Metadata for a column the ordinary catalog does not know (comparison and link columns): plain text. */
function textMeta(key: string): ColumnMeta {
  return { key, kind: { kind: "text" }, defaultWidth: /Address|Source|Destination/.test(key) ? 280 : 180, defaultVisible: true, align: "left",
    sortable: !COMPARISON_COLUMNS.includes(key), searchable: [], extension: false };
}
/** UI-007: a tab's columns carry the same kinds, alignment and widths as in the ordinary table (catalog). */
export function comparisonMeta(columns: string[], catalogColumns?: ColumnMeta[]): ColumnMeta[] {
  return columns.map(key => {
    const known = COMPARISON_COLUMNS.includes(key) ? undefined : catalogColumns?.find(c => c.key === key);
    return known ? { ...known, defaultVisible: true, needs: null } : textMeta(key);
  });
}
/** Address stays first; the comparison columns follow directly so they are visible without horizontal scrolling. */
export function comparisonDefaultColumns(meta: ColumnMeta[]): ColumnState[] {
  const columns = defaultColumns(meta);
  const rank = (key: string) => key === "Row" ? 0 : key === "Address" ? 1 : COMPARISON_COLUMNS.includes(key) ? 2 + COMPARISON_COLUMNS.indexOf(key) : 10;
  return columns.map((c, i) => ({ c, i })).sort((a, b) => rank(a.c.key) - rank(b.c.key) || a.i - b.i).map(({ c }) => c);
}

/**
 * UI-140/UI-142: the complete comparison population behind one virtualized grid. Row windows are fetched on demand through
 * the stateless comparison read (`view.rows` + `comparison`), exactly like an ordinary view; no client-side pages.
 */
export class ComparisonSource extends ViewDataSource {
  readonly side: Side;
  readonly filtersEnabled: boolean;
  readonly contentAvailable?: boolean;
  private readonly known: { rows: Map<number, number>; partners: Map<number, number | null> };
  constructor(first: ComparisonPage, load: LoadPage, catalogColumns?: ColumnMeta[]) {
    const known = { rows: new Map<number, number>(), partners: new Map<number, number | null>() };
    const remember = (page: ComparisonPage) => {
      if (known.rows.size > 100_000) { known.rows.clear(); known.partners.clear(); }
      page.rowKeys.forEach((key, i) => { known.rows.set(key, page.offset + i); known.partners.set(key, page.partnerKeys?.[i] ?? null); });
    };
    remember(first);
    const window = (page: ComparisonPage) => ({ viewId: first.viewId, version: page.version, offset: page.offset, rowKeys: page.rowKeys, columns: page.columns, cells: page.cells });
    super({ viewId: first.viewId, version: first.version, total: first.total, state: "ready", columns: comparisonMeta(first.columns, catalogColumns),
      initialRows: first.offset === 0 && first.cells.length <= BLOCK_SIZE ? window(first) : null }, {
      // The core always answers with the complete projection of the query; the grid selects its visible columns itself.
      rows: async (args, signal) => { const page = await load(args.offset, args.limit, signal); remember(page); return window(page); },
      locate: async args => known.rows.get(args.urlId) ?? null,
      close: async () => undefined,
      copy: async (args: CopyArgs) => {
        const selection = args.selection;
        const picked = (key: number) => selection.kind === "all" ? !selection.except.includes(key) : selection.ids.includes(key);
        const lines: string[][] = args.withHeader ? [args.columns] : [];
        for (let offset = 0; ;) {
          const page = await load(offset, 500);
          const indices = args.columns.map(key => page.columns.indexOf(key));
          page.cells.forEach((row, i) => { if (picked(page.rowKeys[i]!)) lines.push(indices.map(c => String(row[c] ?? ""))); });
          offset += page.cells.length;
          if (offset >= page.total || page.cells.length === 0) break;
        }
        const text = lines.map(row => row.join("\t")).join("\n");
        await navigator.clipboard.writeText(text);
        return { rows: lines.length - (args.withHeader ? 1 : 0), bytes: new TextEncoder().encode(text).length };
      },
    });
    this.known = known;
    this.side = first.side; this.filtersEnabled = first.filtersEnabled; this.contentAvailable = first.contentAvailable;
  }
  /** The mapped partner of a loaded row in the other crawl (OUT-146), for the detail source (CRAWL-144). */
  partner(key: number): number | null { return this.known.partners.get(key) ?? null; }
}

/** One comparison query as a virtualized source; a new query, sort or comparison revision starts again at row 0. */
export function useComparisonSource(query: ComparisonQuery, revision: number, catalogTab?: string) {
  const [source, setSource] = useState<ComparisonSource>();
  const [error, setError] = useState<unknown>();
  const key = JSON.stringify([query, revision, catalogTab ?? null]);
  useEffect(() => {
    const controller = new AbortController(); let created: ComparisonSource | undefined;
    setSource(undefined); setError(undefined);
    const load: LoadPage = (offset, limit, signal) => comparisonRows(query, offset, signal, limit);
    const meta = catalogTab ? catalog.columns({ tab: catalogTab }, controller.signal).catch(() => undefined) : Promise.resolve(undefined);
    void Promise.all([load(0, BLOCK_SIZE, controller.signal), meta]).then(([first, columns]) => {
      if (controller.signal.aborted) return;
      created = new ComparisonSource(first, load, columns); setSource(created);
    }).catch(e => { if (!controller.signal.aborted) setError(e); });
    return () => { controller.abort(); void created?.dispose(); };
  }, [key]);
  return { source, error, setError };
}

/** Column layout per projection: retained across sorting and reloads of the same column set. */
export function useComparisonColumns(source: ComparisonSource | undefined, configuration: Map<string, ColumnState[]>) {
  const schema = source?.columns.map(c => c.key).join("\0") ?? "";
  const defaults = useMemo(() => source ? comparisonDefaultColumns(source.columns) : [], [source]);
  const [, rerender] = useState(0);
  const columns = configuration.get(schema) ?? defaults;
  const setColumns = (next: ColumnState[]) => { configuration.set(schema, next); rerender(n => n + 1); };
  const reset = () => setColumns(defaults);
  return { columns, setColumns, reset };
}

export function ComparisonGrid({ source, columns, onColumnsChange, label, sort, onSort, onSelect, onError, onExport, selectedKey }: {
  source: ComparisonSource; columns: ColumnState[]; onColumnsChange: (columns: ColumnState[]) => void;
  selectedKey?: number | null; label: string; sort?: SortKey; onSort: (sort: SortKey) => void;
  onSelect?: (key: number) => void; onError: (error: unknown) => void; onExport: () => void;
}) {
  const t = useT();
  const [selection, setSelection] = useState(SelectionModel.empty);
  useEffect(() => { if (onSelect) setSelection(selectedKey == null ? SelectionModel.empty() : SelectionModel.empty().select(selectedKey)); }, [selectedKey]);
  const appearance = usePrefs(s => s.appearance);
  const select = (next: SelectionModel) => { setSelection(next); if (next.focus !== undefined) onSelect?.(next.focus); };
  // UI-121: localized headers as in the ordinary table; canonical names stay the keys (export, sorting, configuration).
  const titles = useMemo(() => Object.fromEntries(source.columns.map(c => [c.key, columnLabel(t, c.key)])), [source, t]);
  return <div className="os-compare-grid-host">
    <DataGrid source={source} columns={columns.map(c => ({ ...c, sort: sort?.column === c.key ? sort.dir : undefined }))}
      selection={selection} onSelectionChange={select} onSortChange={next => { if (next[0]) onSort(next[0]); }}
      columnTitles={titles}
      onColumnsChange={onColumnsChange} onContextMenu={() => undefined} onActivate={key => select(SelectionModel.empty().select(key))}
      onError={onError} confirmLargeCopy={async () => false} onExportSelection={onExport}
      rowHeight={SIZES.rowHeight[appearance.density]} locale={currentLanguage()} ariaLabel={label}
      theme={{ tokens: resolveTokens(resolveTheme(appearance.theme, systemPrefersDark()), appearance.accent),
        fontFamily: FONT_STACKS[currentPlatform()].ui, fontSize: SIZES.fontTable[appearance.density], zebra: appearance.zebra }} />
  </div>;
}

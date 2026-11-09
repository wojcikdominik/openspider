import { useTableSelection } from "./selection";
import { ViewDataSource } from "../../data/ViewDataSource";
import { view } from "../../ipc/view";
import { catalog } from "../../ipc/catalog";
import { IpcError, isSilentError, localError } from "../../ipc/errors";
import { op } from "../../ipc/op";
import { useSession } from "../../state/session";
import type { ViewHandle } from "../../ipc/gen/ViewHandle";
import type { FilterCounts } from "../../ipc/gen/FilterCounts";
import type { ViewSpec } from "../../ipc/gen/ViewSpec";
import type { Tick } from "../../ipc/gen/Tick";
import { SelectionModel } from "../../state/selection";
import type { TabQuery } from "../../state/tabs";
import type { SegmentFilter } from "../../ipc/gen/SegmentFilter";
import { useSegments } from "../table-advanced/stores";

export function tabSpec(tab: string, query: TabQuery, segment: SegmentFilter | null = useSegments.getState().filter): ViewSpec {
  const search = query.rules ? { kind: "rules" as const, tree: query.rules.tree, caseSensitive: query.rules.caseSensitive }
    : query.search ? { kind: "simple" as const, text: query.search } : null;
  return { kind: "tab", tab, filter: query.filter, search, sort: query.sort, ...(segment ? { segment } : {}) };
}
/** Owns one active view. An open finishing after a tab switch is closed immediately. */
export class TableController {
  source?: ViewDataSource;
  selection = SelectionModel.empty();
  counts: FilterCounts = { version: -1, filters: [] };
  error: unknown = null;
  pending = false;
  private generation = 0;
  private countsRequest = 0;
  private countsEpoch = 0;
  private countsLoading?: Promise<void>;
  private listeners = new Set<() => void>();
  private revision = 0;
  private unsubscribe?: () => void;
  private disposed = false;
  private opening: Promise<void> = Promise.resolve();
  /** Last `Tick.countsVersion` answered with a counts request. It is a tick counter of the core and not comparable with `FilterCounts.version` (change sequence of the file). */
  private countsTick = -1;
  private abortBuild?: () => void;
  subscribe = (cb: () => void) => { this.listeners.add(cb); return () => { this.listeners.delete(cb); }; };
  snapshot = () => this.revision;
  private emit() { this.revision++; for (const cb of this.listeners) cb(); }
  fail = (error: unknown) => { if (!this.disposed && !isSilentError(error)) { this.error = error; this.emit(); } };
  setSelection = (selection: SelectionModel) => { this.selection = selection; useTableSelection.setState({ selection, viewId: this.source?.viewId, version: this.source?.version }); this.emit(); };
  activate() { this.disposed = false; }
  resetCrawl() { this.countsEpoch++; this.countsRequest++; this.countsTick = -1; this.counts = { version: -1, filters: [] }; this.setSelection(SelectionModel.empty()); }
  async open(tab: string, query: TabQuery): Promise<void> {
    const generation = ++this.generation;
    this.abortBuild?.();
    const previous = this.source;
    this.source = undefined; this.unsubscribe?.(); this.pending = true; this.error = null; this.emit();
    // Retain explicit URL ids across tabs; an all-selection belongs to its old snapshot.
    if (this.selection.value.kind === "all") this.setSelection(SelectionModel.empty());
    if (previous) void previous.dispose().catch(this.fail);
    const job = async () => {
      try {
        const columns = await catalog.columns({ tab });
        if (generation !== this.generation || this.disposed) return;
        const opened = await view.open({ spec: tabSpec(tab, query), columns: columns.map(c => c.key) });
        const handle = await this.built(opened, generation);
        if (!handle || generation !== this.generation || this.disposed) { await view.close({ viewId: opened.viewId }); return; }
        const source = new ViewDataSource(handle);
        source.setSorted(query.sort.length > 0); source.onError = this.fail;
        this.source = source; this.unsubscribe = source.subscribe(() => this.emit());
        useTableSelection.setState({ selection: this.selection, viewId: source.viewId, version: source.version });
        this.pending = false; this.emit();
        // Version announcements that reached the session store before this source existed (UI-142, A01-21).
        const announced = useSession.getState().views[source.viewId];
        if (announced && announced.version > source.version) await source.onViewChanged(announced.version, announced.total ?? source.total);
        else await source.ensureRange(0, 30);
      } catch (error) { if (generation === this.generation) { this.pending = false; this.fail(error); } }
    };
    this.opening = job();
    await this.opening;
  }
  /**
   * `view.open` answers `building` when the build outlasts the core's synchronous window (it does while a crawl writes):
   * the handle then has neither columns nor version. Wait for the build operation and continue with the handle it delivers
   * (A01-16, A01-09). Resolves `null` when a newer open or `dispose` superseded this one.
   */
  private built(handle: ViewHandle, generation: number): Promise<ViewHandle | null> {
    if (handle.state !== "building") return Promise.resolve(handle);
    const opId = handle.op?.opId;
    if (!opId) return Promise.reject(new IpcError(localError("Internal", "err-internal")));
    return new Promise<ViewHandle | null>((resolve, reject) => {
      let off: (() => void) | undefined;
      let finished = false;
      const finish = (value: ViewHandle | null, error?: unknown) => {
        if (finished) return;
        finished = true; off?.(); if (this.abortBuild === abort) this.abortBuild = undefined;
        if (error !== undefined) reject(error);
        else {
          if (value === null) void op.cancel({ opId }).catch(() => undefined);
          resolve(value);
        }
      };
      const abort = () => finish(null);
      const check = () => {
        if (generation !== this.generation || this.disposed) { finish(null); return; }
        const status = useSession.getState().ops[opId];
        if (!status || status.state === "running") return;
        if (status.state === "done") {
          const result = status.result as Partial<ViewHandle> | null | undefined;
          if (result && typeof result.viewId === "string" && Array.isArray(result.columns)) finish(result as ViewHandle);
          else finish(null, new IpcError(localError("Internal", "err-internal")));
        } else if (status.state === "failed") finish(null, new IpcError(status.error ?? localError("Internal", "err-internal")));
        else finish(null);
      };
      this.abortBuild = abort;
      off = useSession.subscribe(check);
      check();
    });
  }
  async refreshCounts(): Promise<void> {
    this.countsRequest++;
    if (this.countsLoading) return this.countsLoading;
    // Ticks may arrive faster than a complete filter-count snapshot can be read.
    // Keep one request in flight and coalesce all intervening ticks into one follow-up.
    const refresh = async () => {
      let requested: number;
      do {
        requested = this.countsRequest;
        const epoch = this.countsEpoch;
        const segment = useSegments.getState().filter;
        try {
          const counts = await catalog.filterCounts(segment ? { segment } : {});
          if (!this.disposed && epoch === this.countsEpoch && JSON.stringify(segment) === JSON.stringify(useSegments.getState().filter) && counts.version >= this.counts.version) {
            this.counts = counts; this.emit();
          }
        } catch (error) { this.fail(error); }
      } while (!this.disposed && requested !== this.countsRequest);
    };
    this.countsLoading = refresh();
    try { await this.countsLoading; }
    finally { this.countsLoading = undefined; }
  }
  async tick(tick: Tick): Promise<void> {
    const jobs: Promise<void>[] = [];
    if (tick.countsVersion !== this.countsTick) { this.countsTick = tick.countsVersion; jobs.push(this.refreshCounts()); }
    const source = this.source;
    const changed = tick.views.find(v => v.viewId === source?.viewId);
    if (source && changed) jobs.push(source.onViewChanged(changed.version, changed.total ?? source.total));
    await Promise.all(jobs);
    if (source && source === this.source) this.setSelection(this.selection.forVersion(source.viewId, source.version));
  }
  async focus(id: number): Promise<number | null> {
    await this.opening;
    const source = this.source;
    if (!source) return null;
    const row = await source.locate(id);
    if (source !== this.source) return null;
    if (row === null) { if (this.selection.value.kind === "ids" && this.selection.value.ids.length === 1) this.setSelection(SelectionModel.empty()); return null; }
    await source.ensureRow(row);
    if (source !== this.source) return null;
    this.setSelection(this.selection.select(id)); source.setFocus(id, row);
    return row;
  }
  dispose() {
    this.disposed = true; this.generation++; this.countsRequest++; this.abortBuild?.(); this.unsubscribe?.(); this.listeners.clear();
    void this.source?.dispose().catch(() => undefined); this.source = undefined;
    useTableSelection.setState({ selection: SelectionModel.empty(), viewId: undefined, version: undefined });
  }
}

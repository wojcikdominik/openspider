import { view } from "../ipc/view";
import { isSilentError, toCmdError } from "../ipc/errors";
import type { ViewHandle } from "../ipc/gen/ViewHandle";
import type { RowWindow } from "../ipc/gen/RowWindow";
import type { KeyRange } from "../ipc/gen/KeyRange";

export const LOADING = Symbol("loading");
export const BLOCK_SIZE = 200;
export const MAX_BLOCKS = 100;
export type CellValue = RowWindow["cells"][number][number];
/** The core no longer knows the view (`view.*` answers `NotFound`/`err-view-not-found`, UI-142). */
export function isViewGone(error: unknown): boolean {
  const failure = toCmdError(error);
  return failure.code === "NotFound" && failure.messageKey === "err-view-not-found";
}
export type ViewApi = Pick<typeof view, "rows" | "locate" | "copy" | "close"> & Partial<Pick<typeof view, "range">>;
type Block = { keys: number[]; cells: CellValue[][] };

/** Canvas reads synchronously; only bounded row windows live in memory. */
export class ViewDataSource {
  readonly viewId: string;
  readonly columns: ViewHandle["columns"];
  version: number;
  total: number;
  private readonly blocks = new Map<number, Block>();
  private readonly pending = new Map<number, Promise<void>>();
  private controller = new AbortController();
  private generation = 0;
  private disposed = false;
  private listeners = new Set<() => void>();
  private first = 0;
  private last = 0;
  private focus: { key: number; row: number } | undefined;
  private tail = false;
  /** The user works on a row: appended rows must not move the table under the pointer (UI-018). A scroll gesture ends it. */
  private hold = false;
  private sorted = false;
  /** A newer version is being fetched in the background; the previous rows stay on screen until it is complete (UI-142, A01-07). */
  private swap: AbortController | undefined;
  private swapping: Promise<void> | undefined;
  private queued: { version: number; total: number } | undefined;
  onScrollTo?: (row: number) => void;
  onError?: (error: unknown) => void;
  onStale?: () => void;

  constructor(handle: ViewHandle, readonly api: ViewApi = view) {
    this.viewId = handle.viewId;
    this.version = handle.version;
    this.total = handle.total ?? 0;
    this.columns = handle.columns;
    const initial = handle.initialRows;
    if (initial) {
      if (initial.viewId !== this.viewId || initial.version !== this.version || initial.offset !== 0
        || initial.columns.join("\0") !== this.columns.map(c => c.key).join("\0")
        || initial.rowKeys.length !== initial.cells.length || initial.cells.length > BLOCK_SIZE) {
        throw new Error("Invalid initial row window");
      }
      this.blocks.set(0, { keys: initial.rowKeys, cells: initial.cells });
    }
  }
  /** Visible row range last requested by the grid (inclusive); a replacement view starts there (UI-018). */
  get visible(): { first: number; last: number } { return { first: this.first, last: this.last }; }
  get cachedRows(): number { return [...this.blocks.values()].reduce((n, b) => n + b.keys.length, 0); }
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private emit() { for (const listener of this.listeners) listener(); }
  private block(row: number): Block | undefined {
    const index = Math.floor(row / BLOCK_SIZE);
    const block = this.blocks.get(index);
    if (block) { this.blocks.delete(index); this.blocks.set(index, block); }
    return block;
  }
  getCell(col: number, row: number): CellValue | typeof LOADING {
    if (row < 0 || row >= this.total || col < 0 || col >= this.columns.length) return null;
    const block = this.block(row);
    return block ? block.cells[row % BLOCK_SIZE]?.[col] ?? null : LOADING;
  }
  rowKey(row: number): number | undefined { return this.block(row)?.keys[row % BLOCK_SIZE]; }
  setFocus(key: number, row: number): void { this.focus = { key, row }; }
  setSorted(sorted: boolean): void { this.sorted = sorted; if (sorted) this.tail = false; }
  /** Visible range is inclusive. Two screenfuls of prefetch on either side. */
  async ensureRange(first: number, last: number): Promise<void> {
    if (this.disposed || !this.total) return;
    this.first = Math.max(0, Math.min(Math.floor(first), this.total - 1));
    this.last = Math.max(this.first, Math.min(Math.floor(last), this.total - 1));
    this.tail = !this.sorted && !this.hold && this.last === this.total - 1;
    // The visible window of a pending version is fetched by the swap itself; a request for the outdated version cannot succeed.
    if (this.swap) return;
    const visibleFirst = this.first;
    const visibleLast = this.last;
    const generation = this.generation;
    const height = Math.min(BLOCK_SIZE * 10, visibleLast - visibleFirst + 1);
    // Visible blocks finish before speculative requests; a jump never queues behind the buffer.
    await this.loadRange(visibleFirst, Math.min(visibleLast, visibleFirst + BLOCK_SIZE * MAX_BLOCKS - 1));
    if (generation !== this.generation || this.disposed || this.first !== visibleFirst || this.last !== visibleLast) return;
    await Promise.all([
      this.loadRange(Math.max(0, visibleFirst - height * 2), visibleFirst - 1),
      this.loadRange(visibleLast + 1, Math.min(this.total - 1, visibleLast + height * 2)),
    ]);
  }
  /** A scroll gesture: moving up stops following new rows at once, also for a swap that is already running (UI-018). */
  userScrolled(up: boolean): void { this.hold = false; if (up) this.tail = false; }
  /** A row was selected: the table keeps its position until the next scroll gesture, even at the bottom (UI-018). */
  holdPosition(): void { this.hold = true; this.tail = false; }
  /** Finishes when no version change is running. */
  settled(): Promise<void> { return this.swapping ?? Promise.resolve(); }
  /**
   * Keys between two URLs from one state of the view (UI-012, UI-018). `undefined`: this source has no such command.
   * The core retries internally; a `Stale` that still arrives here is retried a few times because the view keeps moving.
   */
  async range(from: number, to: number): Promise<KeyRange | undefined> {
    if (!this.api.range) return undefined;
    for (let attempt = 0; ; attempt++) {
      // Not bound to this.controller: a version swap aborts that one, and the range does not depend on the version.
      try { return await this.api.range({ viewId: this.viewId, from, to }); }
      catch (error) {
        const failure = toCmdError(error);
        // A transport or fixture without the command falls back to reading row windows.
        if (failure.messageKey === "err-unknown-command" || failure.code === "Unsupported") return undefined;
        if (failure.code !== "Stale" || attempt >= 4) throw error;
      }
    }
  }
  async ensureRow(row: number): Promise<void> {
    if (!this.disposed && row >= 0 && row < this.total) await this.load(Math.floor(row / BLOCK_SIZE));
  }
  private async loadRange(first: number, last: number): Promise<void> {
    const jobs: Promise<void>[] = [];
    for (let b = Math.floor(first / BLOCK_SIZE); b <= Math.floor(last / BLOCK_SIZE) && first <= last; b++) jobs.push(this.load(b));
    await Promise.all(jobs);
  }
  private load(index: number): Promise<void> {
    if (this.blocks.has(index)) return Promise.resolve();
    const existing = this.pending.get(index);
    if (existing) return existing;
    const generation = this.generation;
    const offset = index * BLOCK_SIZE;
    const columns = this.columns.map(c => c.key);
    const job = this.api.rows({ viewId: this.viewId, version: this.version, offset, limit: BLOCK_SIZE, columns }, this.controller.signal)
      .then(result => {
        if (generation !== this.generation || this.disposed) return;
        if (result.version !== this.version || result.viewId !== this.viewId) return;
        if (result.offset !== offset || result.columns.join("\0") !== columns.join("\0") || result.rowKeys.length !== result.cells.length || result.cells.length > BLOCK_SIZE) {
          throw new Error("Invalid row window");
        }
        this.blocks.set(index, { keys: result.rowKeys, cells: result.cells });
        while (this.blocks.size > MAX_BLOCKS) this.blocks.delete(this.blocks.keys().next().value!);
        this.emit();
      })
      .catch(error => {
        if (generation !== this.generation || this.disposed) return;
        if (toCmdError(error).code === "Stale") this.onStale?.();
        else if (!isSilentError(error)) this.onError?.(error);
      })
      .finally(() => { if (generation === this.generation) this.pending.delete(index); });
    this.pending.set(index, job);
    return job;
  }
  async locate(key: number): Promise<number | null> {
    const generation = this.generation;
    const row = await this.api.locate({ viewId: this.viewId, urlId: key }, this.controller.signal);
    return generation === this.generation && !this.disposed ? row : null;
  }
  /**
   * A new version of the view (tick). The rows of the visible window are fetched in the background and swapped in together
   * with version, total and scroll position, so the table never shows placeholders or half-updated rows (UI-142, A01-07)
   * and the focus row keeps its screen offset (UI-018, A01-10). Announcements that arrive while a swap is running are
   * coalesced: the running swap finishes (the table keeps moving under load) and the latest version follows.
   */
  onViewChanged(version: number, total: number): Promise<void> {
    if (this.disposed || version <= Math.max(this.version, this.queued?.version ?? 0)) return this.swapping ?? Promise.resolve();
    this.queued = { version, total };
    this.swapping ??= (async () => {
      try {
        while (this.queued && !this.disposed) {
          const next = this.queued;
          this.queued = undefined;
          if (next.version > this.version) await this.swapTo(next.version, next.total);
        }
      } finally { this.swapping = undefined; }
    })();
    return this.swapping;
  }
  private async swapTo(version: number, total: number): Promise<void> {
    const anchor = this.focus ?? { key: this.rowKey(this.first), row: this.first };
    const startFirst = this.first;
    const relative = anchor.row - this.first;
    const height = this.last - this.first;
    const tail = this.tail;
    const job = new AbortController();
    this.swap = job;
    let first = Math.min(this.first, Math.max(0, total - 1));
    const fresh = new Map<number, Block>();
    let located: number | undefined;
    try {
      if (tail) first = Math.max(0, total - height - 1);
      else if (anchor.key !== undefined) {
        try {
          const row = await this.api.locate({ viewId: this.viewId, urlId: anchor.key }, job.signal);
          if (job.signal.aborted || this.disposed) return;
          if (row !== null) { first = Math.max(0, row - relative); located = row; }
        } catch (error) {
          if (job.signal.aborted || this.disposed) return;
          if (toCmdError(error).code === "Stale") this.onStale?.();
          if (!isSilentError(error)) this.onError?.(error); else return;
        }
      }
      first = Math.min(first, Math.max(0, total - 1));
      const last = Math.min(total - 1, first + height);
      const columns = this.columns.map(c => c.key);
      const loads: Promise<void>[] = [];
      for (let index = Math.floor(first / BLOCK_SIZE); total > 0 && index <= Math.floor(last / BLOCK_SIZE); index++) {
        const offset = index * BLOCK_SIZE;
        loads.push(this.api.rows({ viewId: this.viewId, version, offset, limit: BLOCK_SIZE, columns }, job.signal).then(result => {
          if (result.version !== version || result.viewId !== this.viewId || result.offset !== offset || result.columns.join("\0") !== columns.join("\0")
            || result.rowKeys.length !== result.cells.length || result.cells.length > BLOCK_SIZE) throw new Error("Invalid row window");
          fresh.set(index, { keys: result.rowKeys, cells: result.cells });
        }));
      }
      await Promise.all(loads);
    } catch (error) {
      // A request that outlived its version is dropped; the next announcement starts a new swap.
      if (!job.signal.aborted && !this.disposed) {
        if (toCmdError(error).code === "Stale") this.onStale?.();
        else if (!isSilentError(error)) this.onError?.(error);
      }
      return;
    } finally {
      if (this.swap === job) this.swap = undefined;
    }
    if (job.signal.aborted || this.disposed) return;
    // UI-018: the user scrolled while this swap was running; the new position wins over following or anchoring.
    if ((tail && !this.tail) || this.first !== startFirst) { first = Math.min(this.first, Math.max(0, total - 1)); located = undefined; }
    this.controller.abort();
    this.controller = new AbortController();
    this.generation++;
    this.version = version;
    this.total = total;
    this.blocks.clear(); this.pending.clear();
    for (const [index, block] of fresh) this.blocks.set(index, block);
    // Focus row and window move together: a following announcement computes its offset from both.
    if (this.focus && anchor.key === this.focus.key && located !== undefined) this.focus = { key: this.focus.key, row: located };
    this.first = first; this.last = Math.max(first, Math.min(total - 1, first + height));
    this.tail = !this.sorted && !this.hold && total > 0 && this.last === total - 1;
    this.emit();
    this.onScrollTo?.(first);
    await this.ensureRange(first, Math.min(total - 1, first + height));
  }
  async keys(first: number, last: number): Promise<number[]> {
    const generation = this.generation;
    const keys: number[] = [];
    // Selection is explicit URL ids; read bounded windows, never retain the entire range as cells.
    for (let row = Math.max(0, first); row <= Math.min(last, this.total - 1); row += BLOCK_SIZE) {
      const result = await this.api.rows({ viewId: this.viewId, version: this.version, offset: row, limit: Math.min(BLOCK_SIZE, last - row + 1), columns: [] }, this.controller.signal);
      if (generation !== this.generation || result.version !== this.version) return [];
      keys.push(...result.rowKeys);
    }
    return keys;
  }
  async dispose(): Promise<void> {
    if (this.disposed) return;
    this.disposed = true; this.generation++; this.controller.abort(); this.swap?.abort(); this.swap = undefined;
    this.blocks.clear(); this.pending.clear(); this.listeners.clear();
    // A view the core already released (crawl replaced or cleared) is closed: that is the goal, not an error (UI-142).
    try { await this.api.close({ viewId: this.viewId }); }
    catch (error) { if (!isViewGone(error)) throw error; }
  }
}

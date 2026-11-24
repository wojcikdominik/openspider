// fix-sel (UI-018): this window replaces the crawl it shows – `crawl.start`, `crawl.open`, `crawl.clear` sent from here.
// Not a core command: kept outside the wrapper layer `src/ipc/*.ts` that the IPC contract test reads.
// Content that arrives from elsewhere (another window, CLI/MCP writer, scheduler) has no such signal.

/** Commands with which this window begins new content; every call through `client.invoke` is noted. */
export const BEGIN_COMMANDS: ReadonlySet<string> = new Set(["crawl.start", "crawl.open", "crawl.clear"]);

export interface CrawlBegin {
  /** Counts begins sent from this window; 0 = none yet. */
  revision: number;
  /** The crawl the core answered for this begin: its id, `null` after a clear, `undefined` while unanswered or failed. */
  crawlId: string | null | undefined;
}

let state: CrawlBegin = { revision: 0, crawlId: undefined };
/** Revision whose request has been answered or has failed: the core holds its final content for it. */
let settledRevision = 0;
const listeners = new Set<() => void>();

/**
 * A failing listener never affects the IPC call (fix-sel B3): the error goes the window's usual way for errors outside
 * React – an `error` event on the window, which `installGlobalErrorLogging` writes to the core log. The event is
 * dispatched, not thrown.
 */
function report(error: unknown): void {
  try {
    if (typeof window === "undefined" || typeof ErrorEvent !== "function") return;
    // Message only: the logger records the message; an event without `error` is no uncaught exception for test runners.
    window.dispatchEvent(new ErrorEvent("error", { message: `crawl begin listener: ${error instanceof Error ? error.message : String(error)}` }));
  } catch { /* nothing left to report to */ }
}
function notify(): void {
  for (const listener of [...listeners]) {
    try { listener(); } catch (error) { report(error); }
  }
}
const set = (next: CrawlBegin) => { state = next; notify(); };

export const crawlBegin = {
  get: (): CrawlBegin => state,
  /** The latest begin has been answered (or failed); views requested from now on show its content. */
  settled: (): boolean => settledRevision === state.revision,
  subscribe(listener: () => void): () => void { listeners.add(listener); return () => { listeners.delete(listener); }; },
  /** Test reset. */
  reset(): void { settledRevision = 0; set({ revision: 0, crawlId: undefined }); },
};

/** Notes a begin before the request leaves and its outcome once the core answered (`client.invoke`). */
export function trackBegin<T>(cmd: string, request: () => Promise<T>): Promise<T> {
  if (!BEGIN_COMMANDS.has(cmd)) return request();
  const revision = state.revision + 1;
  set({ revision, crawlId: undefined });
  const result = request();
  void result.then(value => {
    if (state.revision !== revision) return;
    const id = cmd === "crawl.clear" ? null : (value as { crawlId?: unknown } | null)?.crawlId;
    settledRevision = revision;
    if (id === null || typeof id === "string") set({ revision, crawlId: id });
    else notify();
  }, () => {
    if (state.revision !== revision) return;
    // A failed begin leaves the core's content as it was (the session is replaced only after a successful start/open).
    settledRevision = revision;
    notify();
  });
  return result;
}

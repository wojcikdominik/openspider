// fix-sel (UI-018): this window replaces the crawl it shows – `crawl.start`, `crawl.open`, `crawl.clear` sent from here.
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
const listeners = new Set<() => void>();
const set = (next: CrawlBegin) => { state = next; for (const listener of listeners) listener(); };

export const crawlBegin = {
  get: (): CrawlBegin => state,
  subscribe(listener: () => void): () => void { listeners.add(listener); return () => { listeners.delete(listener); }; },
  /** Test reset. */
  reset(): void { set({ revision: 0, crawlId: undefined }); },
};

/** Notes a begin before the request leaves and the crawl id once the core answered (`client.invoke`). */
export function trackBegin<T>(cmd: string, request: () => Promise<T>): Promise<T> {
  if (!BEGIN_COMMANDS.has(cmd)) return request();
  const revision = state.revision + 1;
  set({ revision, crawlId: undefined });
  const result = request();
  void result.then(value => {
    if (state.revision !== revision) return;
    const id = cmd === "crawl.clear" ? null : (value as { crawlId?: unknown } | null)?.crawlId;
    if (id === null || typeof id === "string") set({ revision, crawlId: id });
  }, () => undefined);
  return result;
}

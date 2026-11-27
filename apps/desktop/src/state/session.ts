// Sitzungszustand (Gerüst): vom Kern gespiegelt, ausschließlich durch Tick-Ereignisse geschrieben (M2-ui-concept 2.2).
// Komponenten lesen mit Selektoren; spätere Pakete (U03 Kopf-/Statusleiste, U12 Seitenleiste) erweitern die Selektoren.
import { create } from "zustand";
import type { EventBus } from "../ipc/events";
import type { AnalysisStatus } from "../ipc/gen/AnalysisStatus";
import type { CrawlStatus } from "../ipc/gen/CrawlStatus";
import type { McpStatus } from "../ipc/gen/McpStatus";
import type { Notice } from "../ipc/gen/Notice";
import type { OpStatus } from "../ipc/gen/OpStatus";
import type { Tick } from "../ipc/gen/Tick";
import type { ViewChanged } from "../ipc/gen/ViewChanged";

export interface SessionState {
  /** Laufende Nummer des letzten angewendeten Ticks (ältere werden verworfen). */
  seq: number;
  /** Active per-host pacing notices, retained when a dialog is dismissed. */
  throttled: Record<string, boolean>;
  crawlDelays: Record<string, { seconds: number; effectiveSeconds: number; applied: boolean }>;
  openingCrawl: boolean;
  openError: Notice | null;
  crawl: CrawlStatus | null;
  analysis: AnalysisStatus | null;
  analysisStarting: boolean;
  analysisResult: "done" | "cancelled" | "failed" | null;
  mcp: McpStatus | null;
  countsVersion: number;
  issuesVersion: number;
  sidebarVersion: number;
  /** Zuletzt gemeldete Version je View (Datenquellen laden bei Wechsel nach). */
  views: Record<string, ViewChanged>;
  ops: Record<string, OpStatus>;
  /** Neue Meldungen des letzten Ticks (Startfehler, Plattenplatz, Crawl-Ende …); Verbraucher quittieren sie. */
  notices: Notice[];
  /**
   * UI-018 (Pfad 3): a crawl this window started whose first status is already its end. Its shared-count views load it
   * (`viewCrawlId`) while the window still shows the previous crawl; crawl and end status appear together once they drew it.
   */
  pendingCrawlId: string | null;
  applyTick(tick: Tick): void;
  clearNotices(): void;
  pushNotice(notice: Notice): void;
  dismissNotice(notice: Notice): void;
}

/**
 * UI-018 (B4-LAGFIX2): „Completed“ appears only when the shown counts are exact. Views that show shared counts
 * (Overview, Issues) register a refresh here; the end status of a running crawl is applied once they have reloaded
 * (at most `END_GATE_MS`). Footer and chip already come with the end status (same tick as the final rows).
 */
const endBarriers = new Set<() => Promise<unknown>>();
const END_GATE_MS = 3000;
let heldEnd: CrawlStatus | null = null;
/**
 * RC1-FLAKES (UI-018, UI-013): the analysis status that arrives while the end status is held. The status bar names the
 * running analysis instead of the crawl phase; applied early it showed a finished crawl ("Analysis running") with the
 * counts of the last running tick, which then changed once the end status was released. Both apply together.
 */
let heldAnalysis: AnalysisStatus | null = null;
/** Each held end gets its own release; a release of an earlier end (another crawl) must not apply a later one. */
let endRelease = 0;
/** Seq of the last tick that carried each crawl's status (a held end status included). */
const statusSeq = new Map<string, number>();
/**
 * Runs this window started or resumed itself whose first status after the command has not arrived yet (see `runHere`),
 * with the seq the window had when it sent the command. Only a later tick of an unbroken tick sequence counts.
 */
const startedHere = new Map<string, number>();
/**
 * UI-018 (B4-LAGFIX2, Abnahme fix/overview-stale Pfad 3): a run this window starts or resumes itself. A very short crawl
 * can end before any running status reaches the window; its first status is then already the end status, and nothing
 * would hold it. After the command has answered, the window remembers the run (unless a status of it arrived meanwhile,
 * which is newer); if its first status is the end status, the crawl is shown as running (views mount and load) and the
 * end status waits for the reloaded views like any other end. Nothing is shown before a status arrives. Runs started
 * elsewhere (MCP or CLI in this app, another window) keep the previous behaviour: their first status may be the end.
 */
export async function runHere<T>(command: () => Promise<T>, crawlOf: (result: T) => string | null | undefined): Promise<T> {
  const sent = useSession.getState().seq;
  const result = await command();
  const crawlId = crawlOf(result);
  if (crawlId && (statusSeq.get(crawlId) ?? -Infinity) <= sent) startedHere.set(crawlId, sent);
  return result;
}
/** The crawl the shared-count views (Overview, Issues) load: a pending new crawl (Pfad 3) before the shown one. */
export const viewCrawlId = (s: SessionState): string | undefined => s.pendingCrawlId ?? s.crawl?.crawlId;
/** `runHere` for resuming the crawl this window shows. */
export function resumeHere<T>(command: () => Promise<T>): Promise<T> {
  const crawlId = useSession.getState().crawl?.crawlId;
  return runHere(command, () => crawlId);
}
export function registerEndBarrier(refresh: () => Promise<unknown>): () => void {
  endBarriers.add(refresh);
  return () => { endBarriers.delete(refresh); };
}
async function releaseEnd(apply: (crawl: CrawlStatus, analysis: AnalysisStatus | null) => void, afterRender = false) {
  const release = ++endRelease;
  // A crawl shown just now (Pfad 3): its views register in the effects of the next render.
  if (afterRender) await new Promise(resolve => setTimeout(resolve, 0));
  await Promise.race([Promise.allSettled([...endBarriers].map(refresh => refresh())), new Promise(resolve => setTimeout(resolve, END_GATE_MS))]);
  // Shared-count views publish their answer in the next task (query notifications); the status follows after them.
  await new Promise(resolve => setTimeout(resolve, 0));
  if (release !== endRelease) return;
  const crawl = heldEnd;
  const analysis = heldAnalysis;
  heldEnd = null; heldAnalysis = null;
  if (crawl) apply(crawl, analysis);
}

export const useSession = create<SessionState>((set, get) => ({
  seq: -1,
  throttled: {},
  crawlDelays: {},
  openingCrawl: false,
  openError: null,
  crawl: null,
  analysis: null,
  analysisResult: null,
  analysisStarting: false,
  mcp: null,
  countsVersion: 0,
  issuesVersion: 0,
  sidebarVersion: 0,
  views: {},
  ops: {},
  notices: [],
  pendingCrawlId: null,
  applyTick(tick) {
    if (tick.seq <= get().seq) return;
    const current = get().crawl;
    // Pfad 3: the first status of a run this window started or resumed (no running status seen since the command).
    const sent = tick.crawl ? startedHere.get(tick.crawl.crawlId) : undefined;
    if (tick.crawl) startedHere.delete(tick.crawl.crawlId);
    const local = sent !== undefined && tick.seq > sent && get().seq >= sent;
    if (tick.crawl) statusSeq.set(tick.crawl.crawlId, tick.seq);
    const applyEnd = (crawl: CrawlStatus, analysis: AnalysisStatus | null) => {
      if (get().crawl?.crawlId === crawl.crawlId) set(s => ({ crawl, throttled: Object.keys(s.throttled).length ? {} : s.throttled,
        ...(analysis ? { analysis, analysisStarting: false } : {}) }));
      // Pfad 3: the new crawl appears with its end status, as a first status of another crawl would (see below).
      else if (get().pendingCrawlId === crawl.crawlId) set({ crawl, pendingCrawlId: null, throttled: {}, crawlDelays: {},
        analysis, analysisStarting: false });
    };
    const pending = get().pendingCrawlId;
    if (tick.crawl && (current?.crawlId === tick.crawl.crawlId || pending === tick.crawl.crawlId) && (heldEnd || (current?.phase === "running" || local) && tick.crawl.phase === "completed" && endBarriers.size > 0)) {
      const start = !heldEnd;
      heldEnd = tick.crawl;
      heldAnalysis = tick.analysis ?? heldAnalysis;
      tick = { ...tick, crawl: null, analysis: null };
      if (start) queueMicrotask(() => void releaseEnd(applyEnd));
    } else if (tick.crawl && local && tick.crawl.phase === "completed") {
      // A crawl this window started that is new to it ends with its first status: its views load it first (pending), the
      // window keeps showing the previous state until they drew it; then crawl and end status apply together.
      heldEnd = tick.crawl;
      heldAnalysis = tick.analysis ?? null;
      const crawlId = tick.crawl.crawlId;
      tick = { ...tick, crawl: null, analysis: null };
      set({ pendingCrawlId: crawlId });
      queueMicrotask(() => void releaseEnd(applyEnd, true));
    } else if (tick.crawl) { heldEnd = null; heldAnalysis = null; if (pending) set({ pendingCrawlId: null }); }
    else if (heldEnd && tick.analysis) { heldAnalysis = tick.analysis; tick = { ...tick, analysis: null }; }
    set((s) => {
      let views = s.views;
      for (const v of tick.views) {
        const old = views[v.viewId];
        if (old?.version === v.version && old.total === v.total) continue;
        if (views === s.views) views = { ...views };
        views[v.viewId] = v;
      }
      const ops = tick.ops.length ? { ...s.ops } : s.ops;
      for (const o of tick.ops) ops[o.opId] = o;
      return {
        seq: tick.seq,
        ...reduceOpening(s, tick.notices),
        throttled: tick.crawl && ["idle", "completed", "failed"].includes(tick.crawl.phase) ? (Object.keys(s.throttled).length ? {} : s.throttled) : reduceThrottle(tick.crawl && tick.crawl.crawlId !== s.crawl?.crawlId ? {} : s.throttled, tick.notices),
        crawlDelays: reduceDelays(tick.crawl && tick.crawl.crawlId !== s.crawl?.crawlId ? {} : s.crawlDelays, tick.notices),
        crawl: tick.crawl ?? s.crawl,
        analysis: tick.analysis ?? (tick.crawl && tick.crawl.crawlId !== s.crawl?.crawlId ? null : s.analysis),
        analysisStarting: tick.analysis || tick.crawl && tick.crawl.crawlId !== s.crawl?.crawlId ? false : s.analysisStarting,
        analysisResult: reduceAnalysisResult(tick.crawl && tick.crawl.crawlId !== s.crawl?.crawlId ? null : s.analysisResult, tick.notices),
        mcp: tick.mcp ?? s.mcp,
        countsVersion: tick.countsVersion,
        issuesVersion: tick.issuesVersion,
        sidebarVersion: tick.sidebarVersion,
        views,
        ops,
        notices: tick.notices.length > 0 ? [...s.notices, ...tick.notices] : s.notices,
      };
    });
  },
  pushNotice(notice) {
    set((s) => ({ notices: [...s.notices, notice], ...reduceOpening(s, [notice]), throttled: reduceThrottle(s.throttled, [notice]), crawlDelays: reduceDelays(s.crawlDelays, [notice]), analysisResult: reduceAnalysisResult(s.analysisResult, [notice]) }));
  },
  dismissNotice(notice) {
    set((s) => ({ notices: s.notices.filter((n) => n !== notice) }));
  },
  clearNotices() {
    set({ notices: [] });
  },
}));

/** Verbindet den Event-Bus des Fensters mit dem Sitzungszustand; liefert die Abmeldung. */
export function attachSession(bus: EventBus): () => void {
  const offTick = bus.on("tick", (t) => useSession.getState().applyTick(t));
  const offNotice = bus.on("notice", (n) => useSession.getState().pushNotice(n));
  return () => { offTick(); offNotice(); };
}

function reduceThrottle(before: Record<string, boolean>, notices: Notice[]): Record<string, boolean> {
  if (!notices.some(n => ["memory-throttled", "memory-throttle-cleared", "crawl-throttled", "crawl-throttle-cleared"].includes(n.code))) return before;
  const next = { ...before };
  for (const n of notices) {
    const host = String(n.args.host ?? "");
    if (n.code === "memory-throttled") next["memory"] = true;
    if (n.code === "memory-throttle-cleared") delete next["memory"];
    if (n.code === "crawl-throttled") next[host] = true;
    if (n.code === "crawl-throttle-cleared") delete next[host];
  }
  return next;
}

function reduceDelays(before: SessionState["crawlDelays"], notices: Notice[]): SessionState["crawlDelays"] {
  if (!notices.some(n => n.code === "crawl-delay" && typeof n.args.seconds === "number" && Number.isFinite(n.args.seconds))) return before;
  const next = { ...before };
  for (const n of notices) if (n.code === "crawl-delay" && typeof n.args.seconds === "number" && Number.isFinite(n.args.seconds)) {
    const applied = n.args.applied === true;
    const effectiveSeconds = typeof n.args.effectiveSeconds === "number" && Number.isFinite(n.args.effectiveSeconds)
      ? n.args.effectiveSeconds : applied ? Math.min(60, Math.max(0, n.args.seconds)) : 0;
    next[String(n.args.origin ?? "")] = { seconds: n.args.seconds, effectiveSeconds, applied };
  }
  return next;
}

function reduceAnalysisResult(before: SessionState["analysisResult"], notices: Notice[]): SessionState["analysisResult"] {
  let result = before;
  for (const notice of notices) {
    if (notice.code === "analysis-completed") result = "done";
    if (notice.code === "analysis-cancelled") result = "cancelled";
    if (notice.code === "analysis-failed") result = "failed";
  }
  return result;
}

function reduceOpening(before: Pick<SessionState, "openingCrawl" | "openError">, notices: Notice[]) {
  let { openingCrawl, openError } = before;
  for (const notice of notices) {
    if (notice.code === "crawl-open-started") { openingCrawl = true; openError = null; }
    if (notice.code === "crawl-open-completed") { openingCrawl = false; openError = null; }
    if (notice.code === "crawl-open-failed") { openingCrawl = false; openError = notice; }
  }
  return { openingCrawl, openError };
}

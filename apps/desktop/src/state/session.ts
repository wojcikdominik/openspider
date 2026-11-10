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
export function registerEndBarrier(refresh: () => Promise<unknown>): () => void {
  endBarriers.add(refresh);
  return () => { endBarriers.delete(refresh); };
}
async function releaseEnd(apply: (crawl: CrawlStatus) => void) {
  await Promise.race([Promise.allSettled([...endBarriers].map(refresh => refresh())), new Promise(resolve => setTimeout(resolve, END_GATE_MS))]);
  // Shared-count views publish their answer in the next task (query notifications); the status follows after them.
  await new Promise(resolve => setTimeout(resolve, 0));
  const crawl = heldEnd;
  heldEnd = null;
  if (crawl) apply(crawl);
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
  applyTick(tick) {
    if (tick.seq <= get().seq) return;
    const current = get().crawl;
    if (tick.crawl && current?.crawlId === tick.crawl.crawlId && (heldEnd || current.phase === "running" && tick.crawl.phase === "completed" && endBarriers.size > 0)) {
      const start = !heldEnd;
      heldEnd = tick.crawl;
      tick = { ...tick, crawl: null };
      if (start) queueMicrotask(() => void releaseEnd(crawl => {
        if (get().crawl?.crawlId === crawl.crawlId) set(s => ({ crawl, throttled: Object.keys(s.throttled).length ? {} : s.throttled }));
      }));
    } else if (tick.crawl) heldEnd = null;
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

import { loginStatus } from "../config/pages/auth-forms/session";
import { useEffect, useState } from "react";
import { currentLanguage, i18n, useFormat, useT } from "../../i18n";
import { useSession, type SessionState } from "../../state/session";
import { McpMenu } from "./McpMenu";
import { AiCostField } from "../ai/cost/CostField";
import { ProgressBar } from "../../ui/primitives/Feedback";
import { useHeader } from "../header/model";
import { useComparison } from "../compare/model";
import "./statusbar.css";

/** Shortest gap between two status paints (at most four paints/s, UI-013). */
const STATUS_PAINT_MS = 250;

/**
 * Latest snapshot, at most four paints/s (UI-013). Notices are handled immediately by their own subscriber.
 * UI-018: a change after a quiet phase is painted at once, in the next task (e.g. the released end status
 * „Completed“); only a change within 250 ms of the last paint waits for the rest of that gap, and all changes of the gap
 * share one paint.
 */
export function useStatusSnapshot(): SessionState {
  const [snapshot, setSnapshot] = useState(useSession.getState);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let painted = Date.now();
    const paint = () => { timer = undefined; painted = Date.now(); setSnapshot(useSession.getState()); };
    const off = useSession.subscribe((next, before) => {
      if (next.crawl === before.crawl && next.mcp === before.mcp && next.throttled === before.throttled &&
        next.crawlDelays === before.crawlDelays && next.openingCrawl === before.openingCrawl && next.openError === before.openError &&
        next.analysis === before.analysis && next.analysisStarting === before.analysisStarting && next.analysisResult === before.analysisResult) return;
      if (timer !== undefined) return;
      const since = Date.now() - painted;
      // After a quiet phase in the next task (the tick's own render stays free of status paints, UI-143); a clock set
      // back counts as a quiet phase: never wait longer than one gap.
      timer = setTimeout(paint, since < 0 ? 0 : Math.max(0, STATUS_PAINT_MS - since));
    });
    paint();
    return () => { off(); clearTimeout(timer); };
  }, []);
  return snapshot;
}

/** CRAWL-127: the reason of a failed open as one localized sentence, with the file name where the core names it. */
export function openErrorText(t: (key: string, args?: Record<string, unknown>) => string, args: Record<string, unknown>): string {
  const key = typeof args.messageKey === "string" ? args.messageKey : undefined;
  const params = typeof args.args === "object" && args.args !== null ? args.args as Record<string, unknown> : {};
  return key && i18n.has(currentLanguage(), key) ? t(key, params) : t(`err-code-${String(args.code ?? "internal").toLowerCase()}`);
}

type DelayEntry = { key: string; messageKey: string; seconds: number; effectiveSeconds: number; hosts: string[] };
/**
 * CRAWL-145/UI-013: one status entry per Crawl-delay cause. The core reports each robots.txt origin; the same host
 * over http and https, or several hosts with the same delay and the same effect, read as one sentence and are shown
 * once, with the hosts in the tooltip.
 */
export function delayEntries(delays: SessionState["crawlDelays"]): DelayEntry[] {
  const entries = new Map<string, DelayEntry>();
  for (const [origin, delay] of Object.entries(delays)) {
    const messageKey = delay.applied ? delay.effectiveSeconds < delay.seconds ? "n03-status-delay-capped" : "n03-status-delay-applied" : "n03-status-delay-ignored";
    const key = `${messageKey}|${delay.seconds}|${delay.effectiveSeconds}`;
    let host = origin;
    try { host = new URL(origin).hostname || origin; } catch { /* keep the origin as reported */ }
    const entry = entries.get(key) ?? { key, messageKey, seconds: delay.seconds, effectiveSeconds: delay.effectiveSeconds, hosts: [] };
    if (!entry.hosts.includes(host)) entry.hosts.push(host);
    entries.set(key, entry);
  }
  return [...entries.values()];
}

export function Statusbar() {
  const t = useT();
  const f = useFormat();
  const { crawl: c, mcp, throttled, crawlDelays, openingCrawl, openError, analysis: storedAnalysis, analysisStarting, analysisResult } = useStatusSnapshot();
  const analysis = analysisStarting ? { steps: [], running: true, current: null, percent: 0 } : storedAnalysis;
  const [sessionLost, setSessionLost] = useState(false);
  useEffect(() => {
    let live = true; setSessionLost(false);
    if (!c) return;
    const refresh = () => void loginStatus().then((rows) => { if (live) setSessionLost(rows.some((r) => r.state === "lost")); }).catch(() => undefined);
    refresh(); const timer = setInterval(refresh, 1000);
    return () => { live = false; clearInterval(timer); };
  }, [c?.crawlId]);
  const chosenMode = useHeader((s) => s.mode);
  // UI-013: an open comparison is the mode „Vergleich“, whatever single crawl is loaded beside it.
  const comparing = useComparison((s) => !!s.session);
  // CRAWL-126: "Paused"/"Stopped" are reported at once; the WAL merge may still finish in the background.
  const stopped = c?.endReason === "stopped";
  const phase = analysis?.running ? t("status-analysis") : stopped && c?.merging ? t("status-stopped-merging") : stopped ? t("n03-status-stopped") : c?.phase === "paused" && c.merging ? t("status-paused-merging") : t(`status-${c?.phase ?? "idle"}`);
  const unknown = "–";
  const current = analysis?.steps.find((s) => s.step === analysis.current);
  const index = current ? analysis!.steps.indexOf(current) + 1 : null;
  const memory = (bytes: number | null | undefined) => bytes == null ? unknown : f.bytes(bytes);
  return <div className="os-crawl-status" data-testid="crawl-status">
    {openingCrawl && <span role="status" data-testid="crawl-opening">{t("shortcut-crawl-open")}<ProgressBar label={t("shortcut-crawl-open")} /></span>}
    {openError && <div role="alert" data-testid="crawl-open-error">
      <span data-testid="crawl-open-error-text">{openErrorText(t, openError.args)}</span>
      {/* M2-ui-concept 2.6: the technical (English) detail is never part of the sentence; it stays in the details. */}
      <details><summary>{t("notify-details")}</summary><div className="os-status-error-detail" data-testid="crawl-open-error-detail">{typeof openError.args.detail === "string" && <p>{openError.args.detail}</p>}<pre>{JSON.stringify(openError.args)}</pre></div></details>
    </div>}
    <div className="os-status-scroll" role="group" aria-label={t("status-details")} tabIndex={0}>
    <span>{t("status-mode", { mode: t(`header-${comparing ? "compare" : c?.mode ?? chosenMode}`) })}</span>
    {sessionLost && <span role="status" className="os-status-warning">{t("form-login-session-lost")}</span>}
    <span data-testid="crawl-phase">{phase}{c?.endReason === "limit_reached" && ` (${t("status-limit")})`}</span>
    {Object.keys(throttled).length > 0 && <span className="os-status-warning">{t("status-throttled")}</span>}
    {c?.phase === "running" && delayEntries(crawlDelays).map(entry => <span key={entry.key} title={entry.hosts.join(", ")}>{t(entry.messageKey, { seconds: f.decimal(entry.seconds), effectiveSeconds: f.decimal(entry.effectiveSeconds) })}</span>)}
    <span>{t("status-rate", { value: f.decimal(c?.rate ?? 0) })}</span>
    <span>{t("status-average", { value: f.decimal(c?.avgRate ?? 0) })}</span>
    <span>{t("status-count", { done: f.integer(c?.crawled ?? 0), total: f.integer((c?.crawled ?? 0) + (c?.queued ?? 0)), percent: c?.percent ?? 0 })}</span>
    <span>{t("status-queued", { value: f.integer(c?.queued ?? 0) })}</span>
    <span>{t("status-eta", { value: c?.etaS == null ? unknown : f.duration(c.etaS) })}</span>
    <span>{t("status-threads", { value: c?.activeThreads ?? 0 })}</span>
    <span className={c?.memoryStage === "paused" ? "os-status-danger" : c?.memoryStage === "warning" ? "os-status-warning" : undefined}>
      {t("status-memory", { used: memory(c?.memoryUsedBytes), limit: memory(c?.memoryLimitBytes) })}
      {c?.memoryStage && c.memoryStage !== "normal" && ` (${t(`status-memory-${c.memoryStage}`)})`}
    </span>
    <span>{t("status-file", { value: memory(c?.fileBytes) })}</span>
    </div>
    <McpMenu>
      {mcp?.kind === "connected" ? t(mcp.clients === 1 ? "status-mcp-client" : "status-mcp-clients", { count: mcp.clients }) : mcp?.kind === "ready" ? t("status-mcp-ready", { port: mcp.port }) : t("status-mcp-off")}
    </McpMenu>
    {/* UI-013 [M4] (M4-F02): field „Kosten“ as a placeholder, only with the switch „KI (Vorschau)“. */}
    <AiCostField />
    <details><summary>{t("status-details")}</summary><div className="os-status-details">
      <span>{t("status-elapsed", { value: f.duration(c?.elapsedS ?? 0) })}</span>
      <span>{t("status-response", { value: c?.avgResponseTime == null ? unknown : f.decimal(c.avgResponseTime, 3) })}</span>
      <span>{t("status-end-reason", { value: c?.endReason ?? unknown })}</span>
    </div></details>
    {(analysis?.running || analysisResult) && <span role="status" data-testid="analysis-feedback">{t(analysis?.running ? "status-analysis" : `n03-analysis-${analysisResult}`)}</span>}
    {analysis?.running && <div className="os-analysis-progress"><span>{t(`status-step-${analysis.current}`, {}, { fallback: analysis.current ?? t("status-analysis") })}{index && ` ${index}/${analysis.steps.length}`} · {analysis.percent ?? unknown} %</span><ProgressBar label={t("status-analysis")} value={analysis.percent} /></div>}
  </div>;
}

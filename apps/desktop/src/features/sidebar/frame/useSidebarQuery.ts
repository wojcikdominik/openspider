import { useEffect, useLayoutEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { sidebar } from "../../../ipc/sidebar";
import { analysis } from "../../../ipc/analysis";
import type { IssuesArgs } from "../../../ipc/gen/IssuesArgs";
import { registerEndBarrier, useSession, type SessionState } from "../../../state/session";
import { useSidebarNavigation } from "./state";

type Revision = (s: SessionState) => string;
type Refetch = (options?: { cancelRefetch?: boolean }) => Promise<{ data?: unknown; isError: boolean }>;

/**
 * UI-018 (B4-LAGFIX2): which answer the consumers of one shared-count query show. A consumer reports the data it has
 * committed to the DOM (`useRenderedAnswer`); the end barrier waits until that is the reloaded answer or a newer one.
 * The Overview renders a deferred value (SidebarFrame), so an answer in the cache is not yet an answer on screen.
 * Answers are ordered by first sight of their object (structural sharing keeps an unchanged answer as the same object);
 * no clock is involved, so two answers of the same millisecond stay apart.
 */
class RenderedAnswers {
  private readonly order = new WeakMap<object, number>();
  private last = 0;
  private consumers = 0;
  private shown: unknown = undefined;
  private waiters: { target: number; resolve: () => void }[] = [];
  private rank(data: unknown): number {
    if (!data || typeof data !== "object") return 0;
    let rank = this.order.get(data);
    if (rank === undefined) this.order.set(data, (rank = ++this.last));
    return rank;
  }
  private flush(all = false) {
    const shown = this.rank(this.shown);
    this.waiters = this.waiters.filter(w => (all || w.target <= shown ? (w.resolve(), false) : true));
  }
  /** The query answer of this render (ranks it in arrival order). */
  note(data: unknown) {
    this.rank(data);
  }
  /** A consumer mounts; when the last one goes, nothing is left that could draw an answer: nobody waits any more. */
  attach(): () => void {
    this.consumers++;
    return () => {
      if (--this.consumers > 0) return;
      this.shown = undefined;
      this.flush(true);
    };
  }
  report(data: unknown) {
    this.shown = data;
    this.flush();
  }
  /** Resolves once `data` or a later answer is on screen; at once without a consumer or without an answer. */
  until(data: unknown): Promise<void> {
    const target = this.rank(data);
    if (this.consumers === 0 || target === 0 || target <= this.rank(this.shown)) return Promise.resolve();
    return new Promise(resolve => { this.waiters.push({ target, resolve }); });
  }
}
const answers = new WeakMap<Refetch, RenderedAnswers>();

/** Reports the shared-count answer a component has committed (layout effect: the DOM shows it). */
export function useRenderedAnswer(query: { refetch: unknown }, data: unknown) {
  const refetch = query.refetch as Refetch;
  useLayoutEffect(() => answers.get(refetch)?.attach(), [refetch]);
  useLayoutEffect(() => { answers.get(refetch)?.report(data); }, [refetch, data]);
}

/**
 * Data version each request was sent for. TanStack Query hands a refetch the promise of a request already in flight
 * (always before the first answer, and with `cancelRefetch: false`); such an answer belongs to the older version.
 */
function useStartedRevision(revision: Revision) {
  const started = useRef<string | null>(null);
  const mark = () => { started.current = revision(useSession.getState()); };
  return { started, mark };
}

/**
 * Serial, bundled refreshes (n03): while `revision` changes, at most one follow-up request is in flight and
 * intervening versions collapse into it. The query key carries no version, so the last answer stays visible
 * and a tick never cancels a running request. A reused older request answers its own version, so the next
 * interval asks again.
 */
function useBundledRefetch(crawl: string | undefined, revision: string, refetch: Refetch, key: string, started: { current: string | null }) {
  const latest = useRef(revision);
  latest.current = revision;
  useEffect(() => {
    if (!crawl) return;
    let requested: string | null = latest.current;
    let retried: string | null = null;
    let pending = false;
    const timer = setInterval(() => {
      if (pending || requested === latest.current) return;
      requested = latest.current;
      pending = true;
      void refetch({ cancelRefetch: false }).then(result => {
        const answered = started.current ?? requested;
        // A failed answer (also one of a reused request) is asked once more per version, without user action (UI-018).
        if (result.isError && retried !== answered) { retried = answered; requested = null; } else requested = answered;
      }, () => { requested = null; }).finally(() => { pending = false; });
    }, 500);
    return () => clearInterval(timer);
  }, [crawl, key, refetch, started]);
}

/**
 * UI-018 (B4-LAGFIX2): „Completed“ waits for the exact final answer of a shared-count view: a request sent at the end
 * tick's version (a reused older request is followed by one more), and that answer committed by its consumers.
 */
function useEndBarrier(crawl: string | undefined, refetch: Refetch, started: { current: string | null }, revision: Revision, rendered: RenderedAnswers) {
  useEffect(() => crawl ? registerEndBarrier(async () => {
    let result = await refetch();
    if (started.current !== revision(useSession.getState())) result = await refetch();
    await rendered.until(result.data);
  }) : undefined, [crawl, refetch, started, revision, rendered]);
}

function useRenderedAnswers(refetch: Refetch, data: unknown): RenderedAnswers {
  const rendered = useRef<RenderedAnswers | null>(null);
  rendered.current ??= new RenderedAnswers();
  answers.set(refetch, rendered.current);
  rendered.current.note(data);
  return rendered.current;
}

const overviewRevision: Revision = s => `${s.countsVersion}:${s.sidebarVersion}`;
const issuesRevision: Revision = s => `${s.issuesVersion}:${s.countsVersion}`;

/** Keep the last successful snapshot while serial, bundled refreshes catch up. */
export function useOverview() {
  const scope = useSidebarNavigation(s => s.scope);
  const crawl = useSession(s => s.crawl?.crawlId);
  const revision = useSession(overviewRevision);
  const { started, mark } = useStartedRevision(overviewRevision);
  const query = useQuery({ queryKey: ["sidebar", "overview", crawl, scope], queryFn: ({ signal }) => { mark(); return sidebar.overview(scope, signal); }, enabled: !!crawl, gcTime: 0 });
  const rendered = useRenderedAnswers(query.refetch, query.data);
  useBundledRefetch(crawl, revision, query.refetch, JSON.stringify(scope), started);
  useEndBarrier(crawl, query.refetch, started, overviewRevision, rendered);
  return query;
}

/**
 * UI-018/UI-062 (M2-N27): issue list of Dashboard („Top-Probleme“) and Issues tab after the pattern of
 * `useOverview`. A new data version keeps the last list visible instead of returning to "Loading…".
 */
export function useIssues(args: IssuesArgs) {
  const crawl = useSession(s => s.crawl?.crawlId);
  const revision = useSession(issuesRevision);
  const { started, mark } = useStartedRevision(issuesRevision);
  const query = useQuery({ queryKey: ["sidebar", "issues", crawl, args], queryFn: ({ signal }) => { mark(); return sidebar.issues(args, signal); }, enabled: !!crawl, gcTime: 0 });
  const rendered = useRenderedAnswers(query.refetch, query.data);
  useBundledRefetch(crawl, revision, query.refetch, JSON.stringify(args), started);
  useEndBarrier(crawl, query.refetch, started, issuesRevision, rendered);
  return query;
}

export function useAnalysis() {
  const crawl = useSession(s => s.crawl?.crawlId);
  const snapshot = useSession(s => s.analysis);
  const query = useQuery({ queryKey: ["sidebar", "analysis", crawl], queryFn: ({ signal }) => analysis.status(signal), enabled: !!crawl, gcTime: 0 });
  return { ...query, data: crawl ? snapshot ?? query.data : undefined };
}

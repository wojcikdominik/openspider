import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { sidebar } from "../../../ipc/sidebar";
import { analysis } from "../../../ipc/analysis";
import type { IssuesArgs } from "../../../ipc/gen/IssuesArgs";
import { registerEndBarrier, useSession } from "../../../state/session";
import { useSidebarNavigation } from "./state";

/**
 * Serial, bundled refreshes (n03): while `revision` changes, at most one follow-up request is in flight and
 * intervening versions collapse into it. The query key carries no version, so the last answer stays visible
 * and a tick never cancels a running request.
 */
function useBundledRefetch(crawl: string | undefined, revision: string, refetch: (options: { cancelRefetch: boolean }) => Promise<unknown>, key: string) {
  const latest = useRef(revision);
  latest.current = revision;
  useEffect(() => {
    if (!crawl) return;
    let requested = latest.current;
    let pending = false;
    const timer = setInterval(() => {
      if (pending || requested === latest.current) return;
      requested = latest.current;
      pending = true;
      void refetch({ cancelRefetch: false }).finally(() => { pending = false; });
    }, 500);
    return () => clearInterval(timer);
  }, [crawl, key, refetch]);
}

/** Keep the last successful snapshot while serial, bundled refreshes catch up. */
export function useOverview() {
  const scope = useSidebarNavigation(s => s.scope);
  const crawl = useSession(s => s.crawl?.crawlId);
  const revision = useSession(s => `${s.countsVersion}:${s.sidebarVersion}`);
  const query = useQuery({ queryKey: ["sidebar", "overview", crawl, scope], queryFn: ({ signal }) => sidebar.overview(scope, signal), enabled: !!crawl, gcTime: 0 });
  useBundledRefetch(crawl, revision, query.refetch, JSON.stringify(scope));
  // UI-018 (B4-LAGFIX2): „Completed“ waits for the exact final Overview.
  useEffect(() => crawl ? registerEndBarrier(() => query.refetch()) : undefined, [crawl, query.refetch]);
  return query;
}

/**
 * UI-018/UI-062 (M2-N27): issue list of Dashboard („Top-Probleme“) and Issues tab after the pattern of
 * `useOverview`. A new data version keeps the last list visible instead of returning to "Loading…".
 */
export function useIssues(args: IssuesArgs) {
  const crawl = useSession(s => s.crawl?.crawlId);
  const revision = useSession(s => `${s.issuesVersion}:${s.countsVersion}`);
  const query = useQuery({ queryKey: ["sidebar", "issues", crawl, args], queryFn: ({ signal }) => sidebar.issues(args, signal), enabled: !!crawl, gcTime: 0 });
  useBundledRefetch(crawl, revision, query.refetch, JSON.stringify(args));
  useEffect(() => crawl ? registerEndBarrier(() => query.refetch()) : undefined, [crawl, query.refetch]);
  return query;
}

export function useAnalysis() {
  const crawl = useSession(s => s.crawl?.crawlId);
  const snapshot = useSession(s => s.analysis);
  const query = useQuery({ queryKey: ["sidebar", "analysis", crawl], queryFn: ({ signal }) => analysis.status(signal), enabled: !!crawl, gcTime: 0 });
  return { ...query, data: crawl ? snapshot ?? query.data : undefined };
}

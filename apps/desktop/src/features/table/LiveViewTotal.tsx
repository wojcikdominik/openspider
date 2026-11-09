import type { ReactNode } from "react";
import type { ViewDataSource } from "../../data/ViewDataSource";
import { useSession } from "../../state/session";

/** UI-018: count metadata is usable before the matching row window has finished loading (UI-145).
 * Subscribe in the small count controls so a count-only tick does not render the table or its canvas.
 * The row source keeps its atomic version/window swap and its scroll/selection anchors.
 */
export function LiveViewTotal({ source, enabled = true, runningOnly = false, children }: {
  source?: ViewDataSource;
  enabled?: boolean;
  runningOnly?: boolean;
  children: (total: number | undefined) => ReactNode;
}) {
  const total = useSession(state => {
    if (!source || !enabled || runningOnly && state.crawl?.phase !== "running") return undefined;
    const announced = state.views[source.viewId];
    return announced && announced.version >= source.version ? announced.total ?? source.total : source.total;
  });
  return children(total);
}

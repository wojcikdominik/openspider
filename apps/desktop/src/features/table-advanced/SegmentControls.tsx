import { useEffect, useRef } from "react";
import { useT } from "../../i18n";
import type { OpStatus } from "../../ipc/gen/OpStatus";
import type { SegmentFilter } from "../../ipc/gen/SegmentFilter";
import { useSession } from "../../state/session";
import { segmentLabelColor } from "../../ui/tokens/tokens";
import { ProgressBar } from "../../ui/primitives/Feedback";
import { useSegments } from "./stores";
import "./table-advanced.css";

/** Segment reassignment reported by the core as a long-running operation (`segment.reassign`, CRAWL-134). */
export const isSegmentOp = (op: OpStatus) => /segment/i.test(op.labelKey);
const encode = (filter: SegmentFilter | null) => filter === null ? "" : filter.kind === "none" ? "none" : `segment:${filter.name}`;
const decode = (value: string): SegmentFilter | null => value === "" ? null : value === "none" ? { kind: "none" } : { kind: "segment", name: value.slice(8) };

/** Global segment filter with badge and reassignment progress (UI-020); only shown when segments exist. */
export function SegmentControls({ crawlId }: { crawlId?: string }) {
  const t = useT(); const { segments, filter, setFilter } = useSegments();
  const ops = useSession(s => s.ops);
  const running = Object.values(ops).filter(op => isSegmentOp(op) && op.state === "running");
  const wasRunning = useRef(false);
  useEffect(() => { void useSegments.getState().load({ crawlChanged: true }); }, [crawlId]);
  useEffect(() => {
    // Column, counts and badge refresh as soon as the core reports the reassignment finished; no recrawl.
    if (wasRunning.current && !running.length) void useSegments.getState().load();
    wasRunning.current = running.length > 0;
  }, [running.length]);
  if (!segments.length && !running.length) return null;
  const active = filter?.kind === "segment" ? segments.find(s => s.name === filter.name) : undefined;
  return <div className="u20-segments">
    <label>{t("u20-segment")}<select aria-label={t("u20-segment")} className="os-input" value={encode(filter)} onChange={e => setFilter(decode(e.target.value))}>
      <option value="">{t("u20-segment-all")}</option>
      {segments.map(s => <option key={s.id} value={`segment:${s.name}`}>{s.name}</option>)}
      <option value="none">{t("u20-segment-none")}</option>
    </select></label>
    {filter && <span className="u20-segment-badge" role="status" aria-label={t("u20-segment-active", { name: active?.name ?? t("u20-segment-none") })}
      style={active ? { background: active.color, color: segmentLabelColor(active.color) } : undefined}>
      {t("u20-segment-active", { name: active?.name ?? t("u20-segment-none") })}
    </span>}
    {running.map(op => <ProgressBar key={op.opId} label={t("u20-segment-reassigning")} value={op.total ? op.done : null} max={op.total ?? undefined} />)}
  </div>;
}

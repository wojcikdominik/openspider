import { useEffect, useState } from "react";
import { useT } from "../../../i18n";

/** The age is named from here on; below it the line reads "Up to date" (UI-018: the throttled count of a running crawl is at least 2 s apart). */
export const SHOWN_FROM_MS = 2_000;

/**
 * UI-018 (M2-N27): while the crawl runs, Overview, Issues and the filter counts share one throttled count; the
 * Overview names the age of its state ("As of 12 s ago"). `ageMs` is the age at the answer, `receivedAt` its
 * arrival (ms since epoch); the label counts on every second until the next answer. The core omits `ageMs` for
 * exact answers, so a throttled answer keeps its age after a pause or the end until the exact count replaces it.
 * Hidden for exact answers. Below {@link SHOWN_FROM_MS} the line stays in place and reads "Up to date", so the
 * content below it does not move with every fresh count.
 */
export function OverviewAge({ ageMs, receivedAt }: { ageMs?: number | null; receivedAt: number }) {
  const t = useT();
  const active = ageMs != null;
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1_000);
    return () => clearInterval(timer);
  }, [active, receivedAt]);
  if (!active) return null;
  const age = ageMs + Math.max(0, now - receivedAt);
  const seconds = Math.round(age / 1_000);
  return <p className="os-hint os-overview-age" data-overview-age={seconds}>{age < SHOWN_FROM_MS ? t("n27-overview-age-current") : t("n27-overview-age", { seconds })}</p>;
}

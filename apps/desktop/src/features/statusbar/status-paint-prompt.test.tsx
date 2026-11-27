// UI-018 (Endstatus ohne Zusatzverzögerung) und CRAWL-130 (höchstens vier Anstriche je Sekunde): Die Statusleiste zeichnet
// eine Änderung nach einer Ruhephase sofort; nur Änderungen innerhalb von 250 ms nach dem letzten Anstrich warten den Rest ab.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { Statusbar } from "./Statusbar";
import { setupHeader, tick } from "../header/fixtures/setup";
import { useSession } from "../../state/session";
import { setLanguage } from "../../i18n";

beforeEach(async () => { setupHeader(); await setLanguage("en"); vi.useFakeTimers(); });
afterEach(() => { cleanup(); vi.useRealTimers(); });

const apply = (seq: number, patch: Partial<NonNullable<typeof tick.crawl>>) =>
  act(() => { useSession.getState().applyTick({ ...tick, seq, crawl: { ...tick.crawl!, ...patch } }); });

// spec: UI-018
// spec: CRAWL-130
it("paints a status change after a quiet phase at once, e.g. the end status", () => {
  act(() => { useSession.getState().applyTick({ ...tick, seq: 1 }); });
  render(<Statusbar />);
  act(() => { vi.advanceTimersByTime(1_000); });
  apply(2, { phase: "completed", percent: 100 });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Completed");
});

// spec: UI-018
// spec: CRAWL-130
it("a change within 250 ms of the last paint waits only for the rest of the gap, and the gap's changes share one paint", () => {
  act(() => { useSession.getState().applyTick({ ...tick, seq: 1 }); });
  render(<Statusbar />);
  act(() => { vi.advanceTimersByTime(1_000); });
  apply(2, { crawled: 10 });
  expect(screen.getByText(/Done 10 of/)).toBeVisible();
  act(() => { vi.advanceTimersByTime(100); });
  apply(3, { crawled: 20 });
  apply(4, { crawled: 30 });
  expect(screen.getByText(/Done 10 of/)).toBeVisible();
  act(() => { vi.advanceTimersByTime(149); });
  expect(screen.getByText(/Done 10 of/)).toBeVisible();
  act(() => { vi.advanceTimersByTime(1); });
  expect(screen.getByText(/Done 30 of/)).toBeVisible();
});

// spec: CRAWL-130
it("still paints at most four times per second under a stream of ticks", () => {
  render(<Statusbar />);
  const painted = new Set<string>();
  for (let i = 1; i <= 100; i++) {
    apply(i, { crawled: i });
    painted.add(screen.getByText(/^Done \d+ of/).textContent ?? "");
    act(() => { vi.advanceTimersByTime(10); });
  }
  // 100 ticks over 1 s (10 ms apart): at most four paints in that second (the mount paint opened the first gap).
  expect(painted.size).toBeLessThanOrEqual(4);
});

// UI-018 (Endstatus ohne Zusatzverzögerung) und UI-013 (Anzeige höchstens viermal je Sekunde; die allgemeine Obergrenze von
// 10/s aus CRAWL-130/PERF-004 bleibt darunter): Die Statusleiste zeichnet eine Änderung nach einer Ruhephase sofort; nur
// Änderungen innerhalb von 250 ms nach dem letzten Anstrich warten den Rest ab.
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
// spec: UI-013
it("paints a status change after a quiet phase at once (next task), e.g. the end status", () => {
  act(() => { useSession.getState().applyTick({ ...tick, seq: 1 }); });
  render(<Statusbar />);
  act(() => { vi.advanceTimersByTime(1_000); });
  apply(2, { phase: "completed", percent: 100 });
  act(() => { vi.advanceTimersByTime(0); });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Completed");
});

// spec: UI-018
// spec: UI-013
it("a change within 250 ms of the last paint waits only for the rest of the gap, and the gap's changes share one paint", () => {
  act(() => { useSession.getState().applyTick({ ...tick, seq: 1 }); });
  render(<Statusbar />);
  act(() => { vi.advanceTimersByTime(1_000); });
  apply(2, { crawled: 10 });
  act(() => { vi.advanceTimersByTime(0); });
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

// spec: UI-013
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

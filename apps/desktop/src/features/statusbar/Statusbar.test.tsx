import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Statusbar } from "./Statusbar";
import { setupHeader, tick } from "../header/fixtures/setup";
import { useSession, attachSession } from "../../state/session";
import { createEventBus } from "../../ipc/events";
import { setLanguage } from "../../i18n";
import { registerAction } from "../../ui/actions";

beforeEach(async () => { setupHeader(); await setLanguage("en"); vi.useFakeTimers(); });
afterEach(() => { cleanup(); vi.useRealTimers(); });
const publish = (seq: number, patch: Partial<NonNullable<typeof tick.crawl>>) => {
  act(() => { useSession.getState().applyTick({ ...tick, seq, crawl: { ...tick.crawl!, ...patch } }); vi.advanceTimersByTime(250); });
};

// spec: UI-013
it("renders the exact core rates, percent and ETA, including a decreasing percent", () => {
  useSession.getState().applyTick(tick); render(<Statusbar />);
  expect(screen.getByText("Current: 42.5 URL/s")).toBeVisible();
  expect(screen.getByText("Average: 40.1 URL/s")).toBeVisible();
  expect(screen.getByText("Time remaining: 0:20")).toBeVisible();
  expect(screen.getByText("Done 1,200 of 2,000 (60 %)")).toBeVisible();
  publish(2, { percent: 30, queued: 2800, rate: 0, etaS: null, phase: "paused" });
  expect(screen.getByText("Done 1,200 of 4,000 (30 %)")).toBeVisible();
  expect(screen.getByText("Current: 0.0 URL/s")).toBeVisible();
  expect(screen.getByText("Average: 40.1 URL/s")).toBeVisible();
  expect(screen.getByText("Time remaining: –")).toBeVisible();
  expect(document.querySelector('[aria-live="polite"]')).toBeNull();
});

// spec: CRAWL-130 [M2]
it("coalesces any number of ticks into at most four status paints per second", () => {
  render(<Statusbar />);
  act(() => { for (let i = 1; i <= 100; i++) useSession.getState().applyTick({ ...tick, seq: i, crawl: { ...tick.crawl!, crawled: i } }); });
  expect(screen.getByText(/Done 0 of 0/)).toBeVisible();
  act(() => { vi.advanceTimersByTime(249); });
  expect(screen.getByText(/Done 0 of 0/)).toBeVisible();
  act(() => { vi.advanceTimersByTime(1); });
  expect(screen.getByText(/Done 100 of 900/)).toBeVisible();
  act(() => useSession.getState().applyTick({ ...tick, seq: 99 }));
  act(() => { vi.advanceTimersByTime(250); });
  expect(screen.getByText(/Done 100 of 900/)).toBeVisible();
});

// spec: CRAWL-097
it("retains per-host throttle status after notices are consumed and clears it on recovery", () => {
  const bus = createEventBus(); const off = attachSession(bus);
  render(<Statusbar />);
  act(() => {
    bus.emit({ kind: "notice", notice: { level: "warning", code: "crawl-throttled", args: { host: "site.test" } } });
    useSession.getState().clearNotices(); vi.advanceTimersByTime(250);
  });
  expect(screen.getByText("Throttled")).toBeVisible();
  act(() => {
    bus.emit({ kind: "notice", notice: { level: "info", code: "crawl-throttle-cleared", args: { host: "site.test" } } });
    vi.advanceTimersByTime(250);
  });
  expect(screen.queryByText("Throttled")).toBeNull();
  off(); expect(bus.listenerCount()).toBe(0);
});

// spec: CRAWL-129
it("uses the core memory stages without deriving UI thresholds", () => {
  useSession.getState().applyTick(tick); render(<Statusbar />);
  expect(screen.getByText("Memory: 800 MB / 1.00 GB")).not.toHaveClass("os-status-warning");
  publish(2, { memoryStage: "warning", memoryUsedBytes: 100 });
  expect(screen.getByText(/Memory: 100 B/)).toHaveClass("os-status-warning");
  publish(3, { memoryStage: "paused" });
  expect(screen.getByText(/Paused due to low memory/)).toHaveClass("os-status-danger");
});

// spec: UI-013
it("shows limits, named analysis steps, statistics, and the always visible MCP menu action", () => {
  const open = vi.fn(); const off = registerAction("mcp.menu", open);
  useSession.getState().applyTick({ ...tick, crawl: { ...tick.crawl!, phase: "completed", endReason: "limit_reached" }, analysis: { running: true, current: "link_score", percent: 37, steps: [{ step: "link_score", state: "running", stale: false }] } });
  render(<Statusbar />);
  expect(screen.getByText("Link Score 1/1 · 37 %")).toBeVisible();
  expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "37");
  expect(screen.getByText(/Limit reached/)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "MCP: connected (1 client)" }));
  expect(open).toHaveBeenCalledOnce(); off();
  fireEvent.click(screen.getByText("Statistics"));
  expect(screen.getByText("Average response time: 0.231 s")).toBeVisible();
});

// spec: UI-013
// spec: UI-080 [M2]
it("publishes the analysis phase and final stopped counters in the same status snapshot", () => {
  useSession.getState().applyTick(tick);
  render(<Statusbar />);
  act(() => useSession.getState().applyTick({
    ...tick, seq: 2,
    crawl: { ...tick.crawl!, phase: "completed", endReason: "stopped", crawled: 8, queued: 19, percent: 29 },
    analysis: { steps: [], running: true, current: null, percent: 0 },
  }));
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Running");
  expect(screen.getByText("Done 1,200 of 2,000 (60 %)")).toBeVisible();
  act(() => { vi.advanceTimersByTime(250); });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Analysis running");
  expect(screen.getByText("Done 8 of 27 (29 %)")).toBeVisible();
  act(() => useSession.getState().applyTick({ ...tick, seq: 3, crawl: null, analysis: { steps: [], running: false, current: null, percent: 100 } }));
  act(() => { vi.advanceTimersByTime(250); });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Stopped");
  expect(screen.getByText("Done 8 of 27 (29 %)")).toBeVisible();
});

// spec: CRAWL-129
it("retains memory throttling after notice dismissal and clears it on recovery", () => {
  render(<Statusbar />);
  act(() => {
    useSession.getState().pushNotice({ level: "warning", code: "memory-throttled", args: {} });
    useSession.getState().clearNotices(); vi.advanceTimersByTime(250);
  });
  expect(screen.getByText("Throttled")).toBeVisible();
  act(() => {
    useSession.getState().pushNotice({ level: "info", code: "memory-throttle-cleared", args: {} });
    vi.advanceTimersByTime(250);
  });
  expect(screen.queryByText("Throttled")).toBeNull();
});

// spec: UI-124
// spec: CRAWL-126 [M2]
it("shows opening progress and a concrete failure reason from the real command notices", () => {
  render(<Statusbar />);
  act(() => {
    useSession.getState().pushNotice({ level: "info", code: "crawl-open-started", args: {} });
    vi.advanceTimersByTime(250);
  });
  expect(screen.getByTestId("crawl-opening")).toBeVisible();
  expect(screen.getByRole("progressbar")).toBeInTheDocument();
  act(() => {
    useSession.getState().pushNotice({ level: "error", code: "crawl-open-failed", args: { code: "Store", detail: "file is not a database" } });
    useSession.getState().clearNotices(); vi.advanceTimersByTime(250);
  });
  expect(screen.queryByTestId("crawl-opening")).toBeNull();
  expect(screen.getByTestId("crawl-open-error")).toHaveTextContent("file is not a database");
});

// spec: UI-124
// spec: CRAWL-126 [M2]
it("keeps structured opening reasons inspectable when no free-text detail exists", () => {
  render(<Statusbar />);
  act(() => {
    useSession.getState().pushNotice({ level: "error", code: "crawl-open-failed", args: {
      code: "Store", messageKey: "err-crawl-newer-version", args: { found: 99, supported: 1 }, detail: null,
    } });
    vi.advanceTimersByTime(250);
  });
  const error = screen.getByTestId("crawl-open-error");
  expect(error).toHaveTextContent("err-crawl-newer-version");
  expect(error).toHaveTextContent('"found":99');
  expect(error).toHaveTextContent('"supported":1');
  expect(screen.getByText("Technical details")).toBeInTheDocument();
});

// spec: CRAWL-127
// spec: UI-124
// B4-CMPUI: the beta.3 status line read "Ein Dateivorgang ist fehlgeschlagen.: No such file or directory (os error 2)".
// The sentence ends with its own punctuation; the English technical detail stays in the details, never in the sentence.
it("keeps the technical detail out of the failure sentence and names a missing file", async () => {
  await setLanguage("de");
  render(<Statusbar />);
  act(() => {
    useSession.getState().pushNotice({ level: "error", code: "crawl-open-failed", args: {
      code: "Io", messageKey: "err-io", args: { path: "/tmp/x.ospider" }, detail: "Permission denied (os error 13)" } });
    vi.advanceTimersByTime(250);
  });
  const text = screen.getByTestId("crawl-open-error-text");
  expect(text).toHaveTextContent(/^Ein Dateivorgang ist fehlgeschlagen\.$/);
  expect(screen.getByTestId("crawl-open-error")).not.toHaveTextContent(".:");
  expect(screen.getByTestId("crawl-open-error")).toHaveTextContent("Permission denied (os error 13)");
  act(() => {
    useSession.getState().pushNotice({ level: "error", code: "crawl-open-failed", args: {
      code: "NotFound", messageKey: "err-file-not-found", args: { path: "/tmp/gone/x.ospider" }, detail: null } });
    vi.advanceTimersByTime(250);
  });
  expect(screen.getByTestId("crawl-open-error-text")).toHaveTextContent(/^Die Datei wurde nicht gefunden: \/tmp\/gone\/x\.ospider$/);
  // An unknown key falls back to the error class, still without a dangling detail.
  act(() => {
    useSession.getState().pushNotice({ level: "error", code: "crawl-open-failed", args: { code: "Io", messageKey: "err-unknown-x", detail: "No such file or directory (os error 2)" } });
    vi.advanceTimersByTime(250);
  });
  expect(screen.getByTestId("crawl-open-error-text")).toHaveTextContent(/^Ein Dateivorgang ist fehlgeschlagen\.$/);
});

// spec: CRAWL-126
it("reports a paused crawl at once and notes the background file merge until it has ended", async () => {
  useSession.getState().applyTick(tick); render(<Statusbar />);
  publish(2, { phase: "paused", merging: true });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Paused – merging file");
  publish(3, { phase: "paused", merging: null });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent(/^Paused$/);
  await setLanguage("de");
  publish(4, { phase: "paused", merging: true });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Pausiert – Datei wird zusammengeführt");
});

// spec: CRAWL-126
// spec: UI-003
it("reports a stopped crawl at once and notes the background file merge until it has ended", async () => {
  useSession.getState().applyTick(tick); render(<Statusbar />);
  publish(2, { phase: "completed", endReason: "stopped", merging: true });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Stopped – merging file");
  publish(3, { phase: "completed", endReason: "stopped", merging: null });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent(/^Stopped$/);
  await setLanguage("de");
  publish(4, { phase: "completed", endReason: "stopped", merging: true });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Gestoppt – Datei wird zusammengeführt");
  publish(5, { phase: "completed", endReason: "stopped", merging: null });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent(/^Gestoppt$/);
});

// UI-018 (fix/overview-end-latency): Das Analyseergebnis aus den Meldungen des gehaltenen Endticks („analysis-completed“ der
// automatischen Crawl-Analyse) gilt zusammen mit dem Endstatus. Früher angewandt war es eine eigene Änderung der Statusleiste,
// und der Endstatus wartete danach auf den nächsten Anstrich (höchstens vier je Sekunde), bis zu 250 ms.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { CrawlStatus } from "../ipc/gen/CrawlStatus";
import type { Notice } from "../ipc/gen/Notice";
import type { Tick } from "../ipc/gen/Tick";
import baseTick from "../ipc/__fixtures__/Tick.json";
import { registerEndBarrier, useSession } from "./session";
import { Statusbar } from "../features/statusbar/Statusbar";
import { setupHeader } from "../features/header/fixtures/setup";
import { setLanguage } from "../i18n";

let seq = 12_000;
const status = (phase: CrawlStatus["phase"], crawlId: string): CrawlStatus => ({ ...(baseTick.crawl as unknown as CrawlStatus), crawlId, phase });
const done: Notice = { level: "info", code: "analysis-completed", args: {} };
const tickOf = (crawl: CrawlStatus | null, notices: Notice[] = []): Tick =>
  ({ ...(baseTick as unknown as Tick), seq: ++seq, crawl, analysis: null, mcp: null, views: [], ops: [], notices, countsVersion: seq, issuesVersion: seq, sidebarVersion: seq });
const apply = (crawl: CrawlStatus | null, notices: Notice[] = []) => useSession.getState().applyTick(tickOf(crawl, notices));
let offs: (() => void)[] = [];

beforeEach(async () => {
  setupHeader(); await setLanguage("en");
  vi.useFakeTimers();
  useSession.setState({ seq: -1, crawl: null, analysis: null, analysisResult: null, views: {}, ops: {}, notices: [], throttled: {} });
});
afterEach(async () => {
  for (const off of offs) off();
  offs = [];
  cleanup();
  await vi.advanceTimersByTimeAsync(4_000);
  vi.useRealTimers();
});

// spec: UI-018
it("applies the analysis result of the held end tick together with the end status", async () => {
  let finish = () => {};
  offs.push(registerEndBarrier(() => new Promise<void>(resolve => { finish = resolve; })));
  apply(status("running", "crawl-r"));
  apply(status("completed", "crawl-r"), [done]);
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(useSession.getState().crawl).toMatchObject({ phase: "running" });
  expect(useSession.getState().analysisResult, "held with the end status").toBeNull();
  expect(useSession.getState().notices).toContainEqual(done);
  await act(async () => { finish(); await vi.advanceTimersByTimeAsync(10); });
  expect(useSession.getState().crawl).toMatchObject({ phase: "completed" });
  expect(useSession.getState().analysisResult).toBe("done");
});

// spec: UI-018
// spec: CRAWL-130
it("the status bar paints the released end status at once, not one paint slot after the end tick", async () => {
  let finish = () => {};
  offs.push(registerEndBarrier(() => new Promise<void>(resolve => { finish = resolve; })));
  act(() => { apply(status("running", "crawl-s")); });
  render(<Statusbar />);
  await act(async () => { await vi.advanceTimersByTimeAsync(1_000); });
  act(() => { apply(status("completed", "crawl-s"), [done]); });
  await act(async () => { await vi.advanceTimersByTimeAsync(20); });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Running");
  // The shared-count views answer 20 ms after the end tick: „Completed“ follows without waiting for a paint slot.
  await act(async () => { finish(); await vi.advanceTimersByTimeAsync(1); });
  expect(screen.getByTestId("crawl-phase")).toHaveTextContent("Completed");
});

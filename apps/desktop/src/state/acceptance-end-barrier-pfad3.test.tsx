// Unabhängige Abnahme fix/overview-stale, zweite Durchsicht (27.11.2026): Crawl, den das Fenster zum ersten Mal schon im
// Endzustand sieht (sehr kurzer Crawl; Crawl aus anderem Fenster, über MCP oder CLI).
// UI-018, Klarstellung 26.11.2026 (main fda6f861): Der Endstatus erscheint sofort; Übersicht und Issues laden ohne
// Nutzeraktion nach und zeigen bis dahin keine Werte – nie Werte eines anderen Crawls oder eines früheren Stands.
// Zeit nur über Fake-Timer, Antworten über gesteuerte Promises. Geschrieben, nicht gelaufen (Lauf durch den Orchestrator).
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Providers } from "../app/Providers";
import { setLanguage } from "../i18n";
import { createClient, setClient } from "../ipc/client";
import { MockCore } from "../ipc/transport/mock";
import type { CrawlStatus } from "../ipc/gen/CrawlStatus";
import type { OverviewDto } from "../ipc/gen/OverviewDto";
import type { Tick } from "../ipc/gen/Tick";
import baseTick from "../ipc/__fixtures__/Tick.json";
import fixture from "../../e2e/sidebar/sidebar-dto.json";
import { SidebarFrame } from "../features/sidebar/frame/SidebarFrame";
import { useSidebarNavigation } from "../features/sidebar/frame/state";
import { registerEndBarrier, useSession } from "./session";

const overviewWith = (crawled: number): OverviewDto => {
  const base = fixture.overview as unknown as OverviewDto;
  return { ...base, summary: base.summary.map(r => (r.key === "Total URLs Crawled" ? { ...r, count: crawled } : r)) };
};
/** Zahlen je Crawl: der Kern antwortet für den Crawl, den er gerade offen hat. */
const COUNT: Record<string, number> = { "crawl-a": 5, "crawl-b": 12, "crawl-x": 7 };
let coreCrawl = "crawl-a";

let seq = 20_000;
const status = (phase: CrawlStatus["phase"], crawlId: string): CrawlStatus => ({ ...(baseTick.crawl as unknown as CrawlStatus), crawlId, phase });
const tickOf = (crawl: CrawlStatus | null, version: number): Tick =>
  ({ ...(baseTick as unknown as Tick), seq: ++seq, crawl, analysis: null, views: [], ops: [], notices: [], countsVersion: version, issuesVersion: version, sidebarVersion: version });
const applyTick = (crawl: CrawlStatus | null, version: number) => act(() => { useSession.getState().applyTick(tickOf(crawl, version)); });
const settle = (ms = 0) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

function Shown() {
  return <output data-testid="acc-shown">{useSession(s => `${s.crawl?.crawlId ?? "none"}:${s.crawl?.phase ?? "none"}`)}</output>;
}
const shownCrawl = () => document.querySelector('[data-testid="acc-shown"]')?.textContent?.split(":")[0] ?? null;
const shownPhase = () => document.querySelector('[data-testid="acc-shown"]')?.textContent?.split(":")[1] ?? null;
const sidebarCrawled = () => [...document.querySelectorAll("#sidebar-panel th")].find(e => e.textContent === "Total URLs Crawled")?.parentElement?.querySelector("td")?.textContent ?? null;

let offs: (() => void)[] = [];
beforeEach(async () => {
  await setLanguage("en");
  vi.useFakeTimers();
  coreCrawl = "crawl-a";
  const core = new MockCore();
  core.handlers.set("sidebar.overview", () => new Promise<OverviewDto>(resolve => { const c = coreCrawl; setTimeout(() => resolve(overviewWith(COUNT[c] ?? 0)), 5); }));
  core.handlers.set("sidebar.issues", () => fixture.issues);
  core.handlers.set("analysis.status", () => fixture.analysis);
  setClient(createClient(core.transport()));
  useSession.setState({ seq: -1, crawl: null, analysis: null, countsVersion: 0, issuesVersion: 0, sidebarVersion: 0, views: {}, ops: {}, notices: [], throttled: {} });
  useSidebarNavigation.setState({ scope: {}, target: null });
});
afterEach(async () => {
  for (const off of offs) off();
  offs = [];
  await settle(4_000);
  cleanup();
  setClient(null);
  vi.useRealTimers();
});

describe("acceptance (second review): a crawl first seen in its end state", () => {
  // spec: UI-018
  // spec: UI-061
  // acceptance: independent
  it("P3-1 crawl A shown, the first status of crawl B is its end: never A's numbers under B or B's numbers under A, then B's final state without user action", async () => {
    // The previous crawl A is finished and shown.
    await applyTick(status("completed", "crawl-a"), 1);
    render(<Providers><Shown /><SidebarFrame /></Providers>);
    await settle(20);
    expect(shownCrawl()).toBe("crawl-a");
    expect(sidebarCrawled()).toBe("5");
    // Crawl B (very short, or started elsewhere) is the core's crawl now; the window's first status of it is its end.
    coreCrawl = "crawl-b";
    // Every step: the numbers on screen are those of the crawl the window names, or none at all.
    const consistent = () => {
      const crawl = shownCrawl();
      const value = sidebarCrawled();
      if (crawl === "crawl-a") expect(value === null || value === "5", `crawl-a shown with Overview ${value}`).toBe(true);
      if (crawl === "crawl-b") expect(value === null || value === "12", `crawl-b shown with Overview ${value}`).toBe(true);
    };
    await applyTick(status("completed", "crawl-b"), 2);
    consistent();
    // The end status appears at once (nothing was seen running that could be held).
    await settle(0);
    consistent();
    expect(shownCrawl()).toBe("crawl-b");
    expect(shownPhase()).toBe("completed");
    const start = Date.now();
    for (let i = 0; i < 5_000 && sidebarCrawled() !== "12" && Date.now() - start < 5_000; i++) {
      await act(async () => { await vi.advanceTimersToNextTimerAsync(); });
      consistent();
    }
    expect(sidebarCrawled()).toBe("12");
    expect(shownPhase()).toBe("completed");
  });

  // spec: UI-018
  // acceptance: independent
  it("P3-2 no hold without a running status seen: a crawl first seen at its end shows its end status with that tick", async () => {
    offs.push(registerEndBarrier(() => new Promise<void>(() => undefined)));
    await applyTick(status("running", "crawl-a"), 1);
    await applyTick(status("completed", "crawl-x"), 2);
    expect(useSession.getState().crawl).toMatchObject({ crawlId: "crawl-x", phase: "completed" });
    // A later end tick of the same, already ended crawl is not held either.
    await applyTick(status("completed", "crawl-x"), 3);
    expect(useSession.getState().crawl).toMatchObject({ crawlId: "crawl-x", phase: "completed" });
  });
});

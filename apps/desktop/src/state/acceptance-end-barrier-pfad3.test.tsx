// Unabhängige Abnahme fix/overview-stale, zweite Durchsicht (27.11.2026): Pfad 3 der Endschranke (ein in diesem Fenster
// gestarteter Crawl, dessen erster Status schon das Ende ist; state/session.ts `runHere`, `pendingCrawlId`, `viewCrawlId`).
// UI-018 / „exakt, sobald Completed“ (b4-lagfix2.md, Nachtrag 09.11.2026): Die angezeigten Zahlen sind die des angezeigten
// Crawls. Eine Wartezeit darf die Overview nicht mit Zahlen eines anderen Crawls füllen als dem, den Kopf, Statusleiste und
// Tabelle zeigen. Eine Markierung „hier gestartet“ darf nicht über den Lauf hinaus liegen bleiben.
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
import { registerEndBarrier, runHere, useSession } from "./session";

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
  useSession.setState({ seq: -1, crawl: null, pendingCrawlId: null, analysis: null, countsVersion: 0, issuesVersion: 0, sidebarVersion: 0, views: {}, ops: {}, notices: [], throttled: {} });
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

describe("acceptance (second review): Pfad 3 of the end barrier", () => {
  // spec: UI-018
  // spec: UI-061
  // acceptance: independent
  it("P3-1 while the window still shows the previous crawl, the Overview shows the previous crawl's numbers, not the new crawl's", async () => {
    // The previous crawl A is finished and shown.
    await applyTick(status("completed", "crawl-a"), 1);
    render(<Providers><Shown /><SidebarFrame /></Providers>);
    await settle(20);
    expect(shownCrawl()).toBe("crawl-a");
    expect(sidebarCrawled()).toBe("5");
    // This window starts crawl B; it is so short that its first status is already its end.
    await act(async () => { await runHere(async () => { coreCrawl = "crawl-b"; return { crawlId: "crawl-b" }; }, r => r.crawlId); });
    await applyTick(status("completed", "crawl-b"), 2);
    // Every step: the numbers on screen belong to the crawl the window names (header, status bar, table follow `crawl`).
    const consistent = () => {
      const crawl = shownCrawl();
      const value = sidebarCrawled();
      if (crawl === "crawl-a") expect(value === null || value === "5", `crawl-a shown with Overview ${value}`).toBe(true);
      if (crawl === "crawl-b") expect(value === null || value === "12", `crawl-b shown with Overview ${value}`).toBe(true);
    };
    const start = Date.now();
    consistent();
    for (let i = 0; i < 5_000 && !(shownCrawl() === "crawl-b" && shownPhase() === "completed") && Date.now() - start < 3_500; i++) {
      await act(async () => { await vi.advanceTimersToNextTimerAsync(); });
      consistent();
    }
    expect(shownCrawl()).toBe("crawl-b");
    expect(shownPhase()).toBe("completed");
    expect(sidebarCrawled()).toBe("12");
  });

  // spec: UI-018
  // acceptance: independent
  it("P3-2 a start marker whose crawl never sent a status does not change how that crawl is opened later", async () => {
    let finish!: () => void;
    offs.push(registerEndBarrier(() => new Promise<void>(resolve => { finish = resolve; })));
    await applyTick(status("running", "crawl-a"), 1);
    // Started here, but before any status of it arrives the window turns to another crawl (second start, open, project).
    await act(async () => { await runHere(async () => ({ crawlId: "crawl-x" }), r => r.crawlId); });
    await applyTick(status("running", "crawl-y"), 2);
    await applyTick(status("completed", "crawl-y"), 3);
    await settle(0);
    finish();
    await settle(10);
    expect(shownCrawl() ?? useSession.getState().crawl?.crawlId).toBe("crawl-y");
    // Much later crawl X (finished) is opened: like any opened finished crawl its status applies with its tick.
    await settle(60_000);
    await applyTick(status("completed", "crawl-x"), 4);
    expect(useSession.getState().crawl).toMatchObject({ crawlId: "crawl-x", phase: "completed" });
    expect(useSession.getState().pendingCrawlId).toBeNull();
  });
});

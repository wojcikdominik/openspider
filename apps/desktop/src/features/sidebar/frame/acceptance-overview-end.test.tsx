// Unabhängige Abnahme fix/overview-stale (nur aus der Spec, ohne Bauer-Kontext).
// UI-018: „Nach Pause oder Crawl-Ende sind alle Werte exakt und ohne Nutzeraktion auf dem pausierten bzw. finalen Stand“;
// Koordinationsentscheidung „exakt, sobald Completed“ (docs/plan/M3-reports/b4-lagfix2.md, Nachtrag 09.11.2026, Punkt 2):
// „Completed“ erscheint erst, wenn Overview und Issues den Endstand zeigen, höchstens 3 s nach dem Endtick.
// UI-061: die Overview-Zahlen sind die des Crawls.
// Zeit nur über Fake-Timer (keine Wanduhr), Antworten des Kerns über gesteuerte Promises. Jeder Schritt ist ein eigener
// act(): React zeichnet zwischen zwei Timern, und nach jedem Schritt wird die Zusage geprüft (DOM-Invariante).
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Providers } from "../../../app/Providers";
import { setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import { useSession } from "../../../state/session";
import type { CrawlStatus } from "../../../ipc/gen/CrawlStatus";
import type { IssueRow } from "../../../ipc/gen/IssueRow";
import type { OverviewDto } from "../../../ipc/gen/OverviewDto";
import type { Tick } from "../../../ipc/gen/Tick";
import baseTick from "../../../ipc/__fixtures__/Tick.json";
import fixture from "../../../../e2e/sidebar/sidebar-dto.json";
import { Dashboard } from "../../dashboard/Dashboard";
import { SidebarFrame } from "./SidebarFrame";
import { useSidebarNavigation } from "./state";

// ---------------------------------------------------------------------------------------------------------------------
// Gesteuerte Antworten des Kerns
// ---------------------------------------------------------------------------------------------------------------------

interface Call<T> { version: number; resolve(value: T): void; reject(error: unknown): void; settled: boolean }
/**
 * Antworten auf eine Abfrage. `auto` beantwortet neue Anfragen nach `latency` ms (Fake-Zeit) abhängig von der Datenversion,
 * die beim Absenden galt; ohne `auto` bleibt die Anfrage offen, bis der Test sie auflöst.
 */
class Replies<T> {
  readonly calls: Call<T>[] = [];
  auto: ((version: number) => T | Error) | null = null;
  constructor(private readonly version: () => number, public latency = 5) {}
  readonly handler = () => new Promise<T>((resolve, reject) => {
    const call: Call<T> = {
      version: this.version(), settled: false,
      resolve: v => { if (!call.settled) { call.settled = true; resolve(v); } },
      reject: e => { if (!call.settled) { call.settled = true; reject(e); } },
    };
    this.calls.push(call);
    const auto = this.auto;
    if (auto) setTimeout(() => { const value = auto(call.version); if (value instanceof Error) call.reject(value); else call.resolve(value); }, this.latency);
  });
  open() { return this.calls.filter(c => !c.settled); }
}

const CRAWL = (baseTick.crawl as unknown as CrawlStatus).crawlId;
const START = 3;
const FINAL = 27;
const ISSUE_START = 1;
const ISSUE_FINAL = 9;
const rows = fixture.issues as unknown as IssueRow[];
const FIRST = rows.filter(r => r.count > 0)[0]!;
const LABEL = `${FIRST.tab}: ${FIRST.filter}`;

const overviewWith = (crawled: number): OverviewDto => {
  const base = fixture.overview as unknown as OverviewDto;
  return { ...base, summary: base.summary.map(r => (r.key === "Total URLs Crawled" ? { ...r, count: crawled } : r)) };
};
const issuesWith = (count: number): IssueRow[] => rows.map(r => (r.id === FIRST.id ? { ...r, count } : r));

// ---------------------------------------------------------------------------------------------------------------------
// Ticks und Lesen aus dem DOM
// ---------------------------------------------------------------------------------------------------------------------

let seq = 1000;
const status = (phase: CrawlStatus["phase"], crawlId = CRAWL): CrawlStatus => ({ ...(baseTick.crawl as unknown as CrawlStatus), crawlId, phase });
const tickOf = (crawl: CrawlStatus | null, version: number): Tick =>
  ({ ...(baseTick as unknown as Tick), seq: ++seq, crawl, analysis: null, views: [], ops: [], notices: [], countsVersion: version, issuesVersion: version, sidebarVersion: version });
const applyTick = (crawl: CrawlStatus | null, version: number) => act(() => { useSession.getState().applyTick(tickOf(crawl, version)); });

function Phase() {
  return <output data-testid="acc-phase">{useSession(s => s.crawl?.phase ?? "none")}</output>;
}
const phase = () => document.querySelector('[data-testid="acc-phase"]')?.textContent ?? null;
/** „Total URLs Crawled“ der Seitenleisten-Overview (auch wenn der Reiter nicht aktiv ist: dann null). */
const sidebarCrawled = () => [...document.querySelectorAll("#sidebar-panel th")].find(e => e.textContent === "Total URLs Crawled")?.parentElement?.querySelector("td")?.textContent ?? null;
/** Zähler des ersten Befunds im Issues-Reiter. */
const sidebarIssue = () => [...document.querySelectorAll("#sidebar-panel tbody tr")].find(tr => tr.querySelector("th button")?.textContent?.includes(LABEL))?.querySelectorAll("td")[2]?.textContent ?? null;
/** Kennzahlkarte „Total URLs Crawled“ des Dashboards. */
const dashboardCrawled = () => [...document.querySelectorAll("article.os-dashboard-card")].find(a => a.querySelector("span")?.textContent === "Total URLs Crawled")?.querySelector("strong")?.textContent ?? null;
/** Zähler des ersten Befunds in „Top-Probleme“ des Dashboards. */
const dashboardIssue = () => [...document.querySelectorAll("button.os-dashboard-row")].find(b => b.querySelector("span")?.textContent?.includes(LABEL))?.querySelector("strong")?.textContent ?? null;

const settle = (ms = 0) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });
/**
 * Timer für Timer weiter (je Schritt ein act, React zeichnet dazwischen), bis `done` gilt oder `budgetMs` Fake-Zeit verstrichen
 * ist; nach jedem Schritt wird `invariant` geprüft. Liefert die verstrichene Fake-Zeit.
 */
async function stepUntil(done: () => boolean, invariant: () => void, budgetMs: number): Promise<number> {
  const start = Date.now();
  invariant();
  for (let i = 0; i < 20_000 && !done() && Date.now() - start <= budgetMs; i++) {
    await act(async () => { await vi.advanceTimersToNextTimerAsync(); });
    invariant();
  }
  return Date.now() - start;
}
/** Zusage „exakt, sobald Completed“: steht „completed“ im DOM, zeigen alle genannten Leser den Endstand. */
const exactWhenCompleted = (...checks: [() => string | null, string][]) => () => {
  if (phase() !== "completed") return;
  for (const [read, expected] of checks) expect(read()).toBe(expected);
};

// ---------------------------------------------------------------------------------------------------------------------

let core: MockCore;
let overview: Replies<OverviewDto>;
let issues: Replies<IssueRow[]>;

beforeEach(async () => {
  await setLanguage("en");
  vi.useFakeTimers();
  core = new MockCore();
  overview = new Replies(() => useSession.getState().countsVersion);
  issues = new Replies(() => useSession.getState().issuesVersion);
  core.handlers.set("sidebar.overview", overview.handler);
  core.handlers.set("sidebar.issues", issues.handler);
  core.handlers.set("analysis.status", () => fixture.analysis);
  setClient(createClient(core.transport()));
  useSession.setState({ seq: -1, crawl: status("running"), analysis: null, countsVersion: 1, issuesVersion: 1, sidebarVersion: 1, views: {}, ops: {}, notices: [], throttled: {} });
  useSidebarNavigation.setState({ scope: {}, target: null });
});
afterEach(async () => {
  // Einen noch gehaltenen Endstatus freigeben (Modulzustand der Endschranke), dann abbauen.
  for (const c of [...overview.open(), ...issues.open()]) c.reject(new Error("test end"));
  await settle(4_000);
  cleanup();
  setClient(null);
  vi.useRealTimers();
});

const byVersion = <T,>(start: T, final: T) => (version: number) => (version >= 2 ? final : start);

describe("acceptance: Completed only with the final shared counts (fix/overview-stale)", () => {
  // spec: UI-018
  // spec: UI-061
  // acceptance: independent
  it("(i) Overview: end tick while the first request from the crawl start is in flight → final count, Completed not earlier, not stuck at the start count", async () => {
    render(<Providers><Phase /><SidebarFrame /></Providers>);
    await settle();
    expect(overview.calls.map(c => c.version)).toEqual([1]);
    const endAt = Date.now();
    await applyTick(status("completed"), 2);
    expect(phase()).toBe("running");
    // Later requests are answered by the core for the version they were sent at; the first one answers the start state.
    overview.auto = byVersion(overviewWith(START), overviewWith(FINAL));
    await act(async () => { overview.calls[0]!.resolve(overviewWith(START)); await vi.advanceTimersByTimeAsync(0); });
    const invariant = exactWhenCompleted([sidebarCrawled, String(FINAL)]);
    await stepUntil(() => phase() === "completed", invariant, 3_500);
    expect(phase()).toBe("completed");
    expect(sidebarCrawled()).toBe(String(FINAL));
    // Released by the barrier, not by the 3 s upper bound.
    expect(Date.now() - endAt).toBeLessThan(3_000);
    // A request was sent at the end tick's version.
    expect(overview.calls.some(c => c.version === 2)).toBe(true);
  });

  // spec: UI-018
  // spec: UI-062
  // acceptance: independent
  it("(i) Issues: end tick while the first Issues request is in flight → final count, Completed not earlier; Overview (other tab) costs no extra wait", async () => {
    overview.auto = byVersion(overviewWith(START), overviewWith(FINAL));
    render(<Providers><Phase /><SidebarFrame /></Providers>);
    await settle(10);
    fireEvent.click(screen.getByRole("tab", { name: "Issues" }));
    await settle();
    expect(issues.calls.map(c => c.version)).toEqual([1]);
    const endAt = Date.now();
    await applyTick(status("completed"), 2);
    expect(phase()).toBe("running");
    issues.auto = byVersion(issuesWith(ISSUE_START), issuesWith(ISSUE_FINAL));
    await act(async () => { issues.calls[0]!.resolve(issuesWith(ISSUE_START)); await vi.advanceTimersByTimeAsync(0); });
    await stepUntil(() => phase() === "completed", exactWhenCompleted([sidebarIssue, String(ISSUE_FINAL)]), 3_500);
    expect(phase()).toBe("completed");
    expect(sidebarIssue()).toBe(String(ISSUE_FINAL));
    expect(Date.now() - endAt).toBeLessThan(1_000);
  });

  // spec: UI-018
  // spec: UI-061
  // acceptance: independent
  it("A1 Issues tab never opened (Overview active): only the Overview is waited for, Completed well before the upper bound", async () => {
    overview.auto = byVersion(overviewWith(START), overviewWith(FINAL));
    issues.auto = byVersion(issuesWith(ISSUE_START), issuesWith(ISSUE_FINAL));
    render(<Providers><Phase /><SidebarFrame /></Providers>);
    await settle(10);
    expect(sidebarCrawled()).toBe(String(START));
    expect(issues.calls).toHaveLength(0);
    const endAt = Date.now();
    await applyTick(status("completed"), 2);
    await stepUntil(() => phase() === "completed", exactWhenCompleted([sidebarCrawled, String(FINAL)]), 3_500);
    expect(phase()).toBe("completed");
    expect(Date.now() - endAt).toBeLessThan(1_000);
  });

  // spec: UI-018
  // spec: UI-061
  // acceptance: independent
  it("A1 Dashboard open, sidebar not mounted: cards and top issues exact at Completed, no wait up to the upper bound", async () => {
    overview.auto = byVersion(overviewWith(START), overviewWith(FINAL));
    issues.auto = byVersion(issuesWith(ISSUE_START), issuesWith(ISSUE_FINAL));
    render(<Providers><Phase /><Dashboard /></Providers>);
    await settle(10);
    expect(dashboardCrawled()).toBe(String(START));
    expect(dashboardIssue()).toBe(String(ISSUE_START));
    const endAt = Date.now();
    await applyTick(status("completed"), 2);
    await stepUntil(() => phase() === "completed", exactWhenCompleted([dashboardCrawled, String(FINAL)], [dashboardIssue, String(ISSUE_FINAL)]), 3_500);
    expect(phase()).toBe("completed");
    expect(Date.now() - endAt).toBeLessThan(1_000);
  });

  // spec: UI-018
  // spec: UI-061
  // acceptance: independent
  it("A1 Dashboard and sidebar Overview share one query: both exact at Completed, no wait up to the upper bound", async () => {
    overview.auto = byVersion(overviewWith(START), overviewWith(FINAL));
    issues.auto = byVersion(issuesWith(ISSUE_START), issuesWith(ISSUE_FINAL));
    render(<Providers><Phase /><Dashboard /><SidebarFrame /></Providers>);
    await settle(10);
    expect(sidebarCrawled()).toBe(String(START));
    expect(dashboardCrawled()).toBe(String(START));
    const endAt = Date.now();
    await applyTick(status("completed"), 2);
    await stepUntil(() => phase() === "completed",
      exactWhenCompleted([sidebarCrawled, String(FINAL)], [dashboardCrawled, String(FINAL)], [dashboardIssue, String(ISSUE_FINAL)]), 3_500);
    expect(phase()).toBe("completed");
    expect(Date.now() - endAt).toBeLessThan(1_000);
  });

  // spec: UI-018
  // acceptance: independent
  it("A1 unchanged answer (structural sharing keeps the same object, nothing new to draw): Completed without waiting for the upper bound", async () => {
    overview.auto = () => overviewWith(FINAL);
    render(<Providers><Phase /><SidebarFrame /></Providers>);
    await settle(10);
    expect(sidebarCrawled()).toBe(String(FINAL));
    const before = overview.calls.length;
    const endAt = Date.now();
    await applyTick(status("completed"), 2);
    await stepUntil(() => phase() === "completed", exactWhenCompleted([sidebarCrawled, String(FINAL)]), 3_500);
    expect(phase()).toBe("completed");
    expect(overview.calls.length).toBeGreaterThan(before);
    expect(Date.now() - endAt).toBeLessThan(1_000);
  });

  // spec: UI-018
  // acceptance: independent
  it("(iv) upper bound: no answer at all → Completed after 3 s, and the late answer is drawn without user action", async () => {
    overview.auto = () => overviewWith(START);
    render(<Providers><Phase /><SidebarFrame /></Providers>);
    await settle(10);
    expect(sidebarCrawled()).toBe(String(START));
    overview.auto = null;
    await applyTick(status("completed"), 2);
    await settle(2_900);
    expect(phase()).toBe("running");
    await settle(200);
    expect(phase()).toBe("completed");
    expect(overview.open().length).toBeGreaterThan(0);
    // The core answers late: the Overview follows without any click.
    await act(async () => { for (const c of overview.open()) c.resolve(overviewWith(FINAL)); await vi.advanceTimersByTimeAsync(0); });
    await stepUntil(() => sidebarCrawled() === String(FINAL), () => undefined, 1_500);
    expect(sidebarCrawled()).toBe(String(FINAL));
  });

  // spec: UI-018
  // acceptance: independent
  it("A4 the end request fails: Completed is not held up to the upper bound", async () => {
    overview.auto = () => overviewWith(START);
    render(<Providers><Phase /><SidebarFrame /></Providers>);
    await settle(10);
    overview.auto = () => new Error("count failed");
    const endAt = Date.now();
    await applyTick(status("completed"), 2);
    await stepUntil(() => phase() === "completed", () => undefined, 3_500);
    expect(phase()).toBe("completed");
    expect(Date.now() - endAt).toBeLessThan(1_000);
  });

  // The failing request answers either before the next 500 ms refresh interval or while it is due (the interval then
  // reuses the failing request instead of sending its own).
  for (const latency of [5, 700]) {
    // spec: UI-018
    // acceptance: independent
    it(`A4 after a failed end request (answer after ${latency} ms) the Overview still reaches the final state without user action`, async () => {
      overview.auto = () => overviewWith(START);
      render(<Providers><Phase /><SidebarFrame /></Providers>);
      await settle(10);
      let failures = 1;
      overview.latency = latency;
      overview.auto = () => (failures-- > 0 ? new Error("count failed") : overviewWith(FINAL));
      await applyTick(status("completed"), 2);
      await stepUntil(() => phase() === "completed", () => undefined, 3_500);
      expect(phase()).toBe("completed");
      // No further tick, no click: within 10 s the final state must be on screen.
      await stepUntil(() => sidebarCrawled() === String(FINAL), () => undefined, 10_000);
      expect(sidebarCrawled()).toBe(String(FINAL));
    });
  }

  // spec: UI-018
  // acceptance: independent
  it("A3 pause: the Overview reaches the exact paused count without user action", async () => {
    const PAUSED = 14;
    overview.auto = byVersion(overviewWith(START), overviewWith(PAUSED));
    render(<Providers><Phase /><SidebarFrame /></Providers>);
    await settle(10);
    expect(sidebarCrawled()).toBe(String(START));
    // Whether "Paused" itself has to wait for the exact count is not decided (report: B2); only the values are checked here.
    await applyTick(status("paused"), 2);
    const elapsed = await stepUntil(() => sidebarCrawled() === String(PAUSED), () => undefined, 2_000);
    expect(sidebarCrawled()).toBe(String(PAUSED));
    expect(phase()).toBe("paused");
    expect(elapsed).toBeLessThan(1_500);
  });

  // spec: UI-018
  // acceptance: independent
  it("A5 resume and a second end, then a removal while completed: every end exact, no hanging barrier, no reload loop", async () => {
    const counts: Record<number, number> = { 1: START, 2: FINAL, 3: FINAL, 4: 30, 5: 29 };
    overview.auto = v => overviewWith(counts[v] ?? 0);
    render(<Providers><Phase /><SidebarFrame /></Providers>);
    await settle(10);
    // First end.
    let endAt = Date.now();
    await applyTick(status("completed"), 2);
    await stepUntil(() => phase() === "completed", exactWhenCompleted([sidebarCrawled, String(FINAL)]), 3_500);
    expect(phase()).toBe("completed");
    expect(Date.now() - endAt).toBeLessThan(1_000);
    // Resume (or re-crawl of single URLs): running again, then a second end with a new final count.
    await applyTick(status("running"), 3);
    expect(phase()).toBe("running");
    await settle(600);
    endAt = Date.now();
    await applyTick(status("completed"), 4);
    expect(phase()).toBe("running");
    await stepUntil(() => phase() === "completed", exactWhenCompleted([sidebarCrawled, "30"]), 3_500);
    expect(phase()).toBe("completed");
    expect(sidebarCrawled()).toBe("30");
    expect(Date.now() - endAt).toBeLessThan(1_000);
    // Removing URLs while completed: one new data version, no phase change, the Overview follows.
    await settle(1_000);
    await applyTick(status("completed"), 5);
    expect(phase()).toBe("completed");
    await stepUntil(() => sidebarCrawled() === "29", () => undefined, 1_500);
    expect(sidebarCrawled()).toBe("29");
    // Quiet afterwards: no request loop.
    await settle(1_000);
    const calls = overview.calls.length;
    await settle(5_000);
    expect(overview.calls.length).toBe(calls);
    expect(overview.open()).toHaveLength(0);
  });
});

// Unabhängige Abnahme fix/overview-stale, Teil 2: der Vertrag „Antwort im Cache ist noch keine Antwort auf dem Bildschirm“.
// UI-018 / Koordinationsentscheidung „exakt, sobald Completed“ (b4-lagfix2.md, Nachtrag 09.11.2026): „Completed“ steht erst im
// DOM, wenn der Endstand dort steht. Ein Prüf-Verbraucher (`Probe`) zeigt die Overview-Antwort wie die Seitenleiste
// (zurückgestellter Wert), nur dass der Test steuert, wann er die neue Antwort übernimmt. Er meldet, was er gezeichnet hat,
// über dieselbe Schnittstelle wie die Produktkomponenten (`useRenderedAnswer`).
// Zeit nur über Fake-Timer, Antworten über gesteuerte Promises.
import { act, cleanup, render } from "@testing-library/react";
import { useRef, useSyncExternalStore } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Providers } from "../../../app/Providers";
import { setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import { useSession } from "../../../state/session";
import type { CrawlStatus } from "../../../ipc/gen/CrawlStatus";
import type { OverviewDto } from "../../../ipc/gen/OverviewDto";
import type { Tick } from "../../../ipc/gen/Tick";
import baseTick from "../../../ipc/__fixtures__/Tick.json";
import fixture from "../../../../e2e/sidebar/sidebar-dto.json";
import { useOverview, useRenderedAnswer } from "./useSidebarQuery";
import { useSidebarNavigation } from "./state";

const CRAWL = (baseTick.crawl as unknown as CrawlStatus).crawlId;
const overviewWith = (crawled: number): OverviewDto => {
  const base = fixture.overview as unknown as OverviewDto;
  return { ...base, summary: base.summary.map(r => (r.key === "Total URLs Crawled" ? { ...r, count: crawled } : r)) };
};
const crawledOf = (data: OverviewDto | undefined) => String(data?.summary.find(r => r.key === "Total URLs Crawled")?.count ?? "–");

/** Kleiner Schalter mit Abonnement (useSyncExternalStore), über act() umgelegt. */
function toggle(initial: boolean) {
  let value = initial;
  const subs = new Set<() => void>();
  return {
    subscribe: (f: () => void) => { subs.add(f); return () => { subs.delete(f); }; },
    get: () => value,
    set: (v: boolean) => act(() => { value = v; for (const f of subs) f(); }),
  };
}
let adopt = toggle(true);
let mounted = toggle(true);

/** Zeigt die Overview-Antwort; bei geschlossenem `adopt` bleibt die zuletzt gezeichnete stehen (wie ein zurückgestellter Wert). */
function Probe() {
  const query = useOverview();
  const open = useSyncExternalStore(adopt.subscribe, adopt.get);
  const shown = useRef<OverviewDto | undefined>(undefined);
  if (open) shown.current = query.data;
  useRenderedAnswer(query, shown.current);
  return <span data-testid="acc-probe">{crawledOf(shown.current)}</span>;
}
function Host() {
  return useSyncExternalStore(mounted.subscribe, mounted.get) ? <Probe /> : null;
}
function Phase() {
  return <output data-testid="acc-phase">{useSession(s => `${s.crawl?.crawlId ?? "none"}:${s.crawl?.phase ?? "none"}`)}</output>;
}
const phase = () => document.querySelector('[data-testid="acc-phase"]')?.textContent?.split(":")[1] ?? null;
const crawlShown = () => document.querySelector('[data-testid="acc-phase"]')?.textContent?.split(":")[0] ?? null;
const probe = () => document.querySelector('[data-testid="acc-probe"]')?.textContent ?? null;

let seq = 5000;
const status = (phase: CrawlStatus["phase"], crawlId = CRAWL): CrawlStatus => ({ ...(baseTick.crawl as unknown as CrawlStatus), crawlId, phase });
const tickOf = (crawl: CrawlStatus | null, version: number): Tick =>
  ({ ...(baseTick as unknown as Tick), seq: ++seq, crawl, analysis: null, views: [], ops: [], notices: [], countsVersion: version, issuesVersion: version, sidebarVersion: version });
const applyTick = (crawl: CrawlStatus | null, version: number) => act(() => { useSession.getState().applyTick(tickOf(crawl, version)); });
const settle = (ms = 0) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });
async function stepUntil(done: () => boolean, invariant: () => void, budgetMs: number): Promise<number> {
  const start = Date.now();
  invariant();
  for (let i = 0; i < 20_000 && !done() && Date.now() - start <= budgetMs; i++) {
    await act(async () => { await vi.advanceTimersToNextTimerAsync(); });
    invariant();
  }
  return Date.now() - start;
}

let core: MockCore;
/** Antwort je Crawl und Datenversion (beim Absenden), nach `latency` ms Fake-Zeit; `hold` lässt neue Anfragen offen. */
let answer: (crawl: string, version: number) => OverviewDto;
let latency = 5;
let hold = false;
let requests: { crawl: string; version: number; resolve(v: OverviewDto): void }[] = [];

beforeEach(async () => {
  await setLanguage("en");
  vi.useFakeTimers();
  adopt = toggle(true);
  mounted = toggle(true);
  latency = 5;
  hold = false;
  requests = [];
  answer = (_crawl, version) => overviewWith(version >= 2 ? 27 : 3);
  core = new MockCore();
  core.handlers.set("sidebar.overview", () => new Promise<OverviewDto>(resolve => {
    const crawl = useSession.getState().crawl?.crawlId ?? "none";
    const version = useSession.getState().countsVersion;
    requests.push({ crawl, version, resolve });
    if (!hold) setTimeout(() => resolve(answer(crawl, version)), latency);
  }));
  setClient(createClient(core.transport()));
  useSession.setState({ seq: -1, crawl: status("running"), analysis: null, countsVersion: 1, issuesVersion: 1, sidebarVersion: 1, views: {}, ops: {}, notices: [], throttled: {} });
  useSidebarNavigation.setState({ scope: {}, target: null });
});
afterEach(async () => {
  await adopt.set(true);
  hold = false;
  for (const r of requests) r.resolve(overviewWith(0));
  await settle(4_000);
  cleanup();
  setClient(null);
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("acceptance: an answer in the cache is not yet an answer on screen", () => {
  // spec: UI-018
  // spec: UI-061
  // acceptance: independent
  it("(ii) the final answer is in the cache but not drawn: Completed is not in the DOM while the old count is; it follows the drawing promptly", async () => {
    render(<Providers><Phase /><Host /></Providers>);
    await settle(10);
    expect(probe()).toBe("3");
    await adopt.set(false);
    await applyTick(status("completed"), 2);
    const invariant = () => { if (phase() === "completed") expect(probe()).toBe("27"); };
    // 2 s of fake time: the final answer arrives (5 ms) but is not drawn.
    await stepUntil(() => false, invariant, 2_000);
    expect(requests.some(r => r.version === 2)).toBe(true);
    expect(phase()).toBe("running");
    await adopt.set(true);
    const elapsed = await stepUntil(() => phase() === "completed", invariant, 1_500);
    expect(phase()).toBe("completed");
    expect(probe()).toBe("27");
    // Released by the drawing, not by the 3 s upper bound (it would be due about 1 s later).
    expect(elapsed).toBeLessThan(200);
  });

  // spec: UI-018
  // acceptance: independent
  it("A1 the waiting consumer is unmounted before it draws the final answer: Completed without waiting for the upper bound", async () => {
    render(<Providers><Phase /><Host /></Providers>);
    await settle(10);
    expect(probe()).toBe("3");
    await adopt.set(false);
    const endAt = Date.now();
    await applyTick(status("completed"), 2);
    await settle(100);
    expect(requests.some(r => r.version === 2)).toBe(true);
    expect(phase()).toBe("running");
    // Nothing is left that could draw the answer: there is nothing to wait for any more.
    await mounted.set(false);
    const unmountedAt = Date.now();
    await stepUntil(() => phase() === "completed", () => undefined, 3_500);
    expect(phase()).toBe("completed");
    expect(Date.now() - unmountedAt, `Completed ${Date.now() - endAt} ms after the end tick`).toBeLessThan(500);
  });

  // spec: UI-018
  // acceptance: independent
  it("A1 the start answer and the final answer carry the same millisecond: the drawn start answer does not count as final", async () => {
    // Timers run as usual, but Date.now() stands still: both answers carry the same update time.
    vi.spyOn(Date, "now").mockReturnValue(Date.now());
    render(<Providers><Phase /><Host /></Providers>);
    await settle(10);
    expect(probe()).toBe("3");
    await adopt.set(false);
    await applyTick(status("completed"), 2);
    for (let i = 0; i < 100; i++) {
      await settle(10);
      if (phase() === "completed") expect(probe()).toBe("27");
    }
    expect(requests.some(r => r.version === 2)).toBe(true);
    expect(phase()).toBe("running");
  });

  // spec: UI-018
  // acceptance: independent
  it("A2 crawl switch: the shown answer of the previous crawl neither holds nor fulfils the end of the new crawl", async () => {
    answer = (crawl, version) => overviewWith(crawl === "crawl-b" ? (version >= 6 ? 40 : 4) : 3);
    render(<Providers><Phase /><Host /></Providers>);
    await settle(10);
    expect(probe()).toBe("3");
    // Another crawl is opened and runs.
    await applyTick(status("running", "crawl-b"), 5);
    await settle(10);
    expect(crawlShown()).toBe("crawl-b");
    expect(probe()).toBe("4");
    // Its end answer is delayed: the old crawl's (or the start) answer on screen must not release Completed.
    hold = true;
    const endAt = Date.now();
    await applyTick(status("completed", "crawl-b"), 6);
    await settle(1_000);
    expect(phase()).toBe("running");
    hold = false;
    await act(async () => { for (const r of requests.filter(r => r.crawl === "crawl-b" && r.version === 6)) r.resolve(answer("crawl-b", 6)); await vi.advanceTimersByTimeAsync(0); });
    await stepUntil(() => phase() === "completed", () => { if (phase() === "completed") expect(probe()).toBe("40"); }, 2_500);
    expect(phase()).toBe("completed");
    expect(probe()).toBe("40");
    expect(Date.now() - endAt).toBeLessThan(3_000);
  });

  // spec: UI-018
  // acceptance: independent
  it("A2 no registration survives unmounting: afterwards an end status applies at once", async () => {
    const { unmount } = render(<Providers><Host /></Providers>);
    await settle(10);
    unmount();
    render(<Providers><Phase /></Providers>);
    await applyTick(status("completed"), 2);
    await settle(0);
    await settle(0);
    expect(phase()).toBe("completed");
  });
});

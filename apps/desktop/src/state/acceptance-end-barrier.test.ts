// Unabhängige Abnahme fix/overview-stale, Teil 3: Endschranke im Sitzungszustand (state/session.ts) bei Crawl-Wechsel.
// UI-018 / „exakt, sobald Completed“ (b4-lagfix2.md, Nachtrag 09.11.2026): der Endstatus eines Crawls gilt erst, wenn die für
// ihn angemeldeten Ansichten neu geladen haben (höchstens 3 s). Eine noch laufende Freigabe des vorigen Crawls darf den
// Endstatus eines neuen Crawls nicht mit ihrem eigenen Abschluss freigeben.
// Zeit nur über Fake-Timer, Ansichten als gesteuerte Promises.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CrawlStatus } from "../ipc/gen/CrawlStatus";
import type { Tick } from "../ipc/gen/Tick";
import baseTick from "../ipc/__fixtures__/Tick.json";
import { registerEndBarrier, useSession } from "./session";

let seq = 9000;
const status = (phase: CrawlStatus["phase"], crawlId: string): CrawlStatus => ({ ...(baseTick.crawl as unknown as CrawlStatus), crawlId, phase });
const tickOf = (crawl: CrawlStatus | null, version: number): Tick =>
  ({ ...(baseTick as unknown as Tick), seq: ++seq, crawl, analysis: null, views: [], ops: [], notices: [], countsVersion: version, issuesVersion: version, sidebarVersion: version });
const apply = (crawl: CrawlStatus | null, version: number) => useSession.getState().applyTick(tickOf(crawl, version));
let offs: (() => void)[] = [];

beforeEach(() => {
  vi.useFakeTimers();
  useSession.setState({ seq: -1, crawl: null, analysis: null, views: {}, ops: {}, notices: [], throttled: {} });
});
afterEach(async () => {
  for (const off of offs) off();
  offs = [];
  await vi.advanceTimersByTimeAsync(4_000);
  vi.useRealTimers();
});

describe("acceptance: end barrier across a crawl switch", () => {
  // spec: UI-018
  // acceptance: independent
  it("A2 a still running release of the previous crawl does not release the end status of the next crawl", async () => {
    const pending: { crawl: string | undefined; finish: () => void }[] = [];
    offs.push(registerEndBarrier(() => new Promise<void>(resolve => { pending.push({ crawl: useSession.getState().crawl?.crawlId, finish: resolve }); })));
    apply(status("running", "crawl-a"), 1);
    apply(status("completed", "crawl-a"), 2);
    await vi.advanceTimersByTimeAsync(0);
    expect(pending).toHaveLength(1);
    // Another crawl is opened (or started) and ends quickly, while crawl A's release is still waiting.
    apply(status("running", "crawl-b"), 3);
    expect(useSession.getState().crawl).toMatchObject({ crawlId: "crawl-b", phase: "running" });
    await vi.advanceTimersByTimeAsync(100);
    apply(status("completed", "crawl-b"), 4);
    await vi.advanceTimersByTimeAsync(0);
    expect(pending).toHaveLength(2);
    // Crawl A's view finishes its reload: crawl B's view has not.
    pending[0]!.finish();
    await vi.advanceTimersByTimeAsync(10);
    expect(useSession.getState().crawl).toMatchObject({ crawlId: "crawl-b", phase: "running" });
    pending[1]!.finish();
    await vi.advanceTimersByTimeAsync(10);
    expect(useSession.getState().crawl).toMatchObject({ crawlId: "crawl-b", phase: "completed" });
  });
});

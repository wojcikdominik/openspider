// B4-LAGFIX2: „Completed“ erscheint erst, wenn die Ansichten mit gemeinsamen Zählungen (Overview, Issues) neu geladen
// haben; andere Teile des Ticks gelten sofort. Ohne registrierte Ansicht gilt die Endmeldung sofort.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { CrawlStatus } from "../ipc/gen/CrawlStatus";
import type { Tick } from "../ipc/gen/Tick";
import fixtureTick from "../ipc/__fixtures__/Tick.json";
import { registerEndBarrier, useSession } from "./session";

const status = (phase: CrawlStatus["phase"], crawled: number, crawlId = "crawl-a"): CrawlStatus => ({ ...(fixtureTick.crawl as CrawlStatus), crawlId, phase, crawled });
const tick = (seq: number, crawl: CrawlStatus | null, countsVersion = seq): Tick => ({ seq, at: fixtureTick.at, crawl, views: [], countsVersion, issuesVersion: 1, sidebarVersion: 1, ops: [], notices: [] });
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
let offs: (() => void)[] = [];

beforeEach(() => { useSession.setState({ seq: -1, crawl: null, views: {}, ops: {}, notices: [], throttled: {} }); });
afterEach(() => { for (const off of offs) off(); offs = []; });

describe("end gate", () => {
  // spec: UI-018
  it("applies Completed only after the registered shared-count views have reloaded, the rest of the tick at once", async () => {
    let finish!: () => void;
    const reloaded: number[] = [];
    offs.push(registerEndBarrier(() => new Promise<void>(resolve => { reloaded.push(useSession.getState().countsVersion); finish = resolve; })));
    useSession.getState().applyTick(tick(1, status("running", 26)));
    useSession.getState().applyTick(tick(2, status("completed", 27), 9));
    expect(useSession.getState().crawl?.phase).toBe("running");
    expect(useSession.getState().countsVersion).toBe(9);
    await flush();
    // The reload sees the new data version of the end tick.
    expect(reloaded).toEqual([9]);
    // A later tick of the same crawl replaces the held status instead of showing it early.
    useSession.getState().applyTick(tick(3, status("completed", 27), 10));
    expect(useSession.getState().crawl?.phase).toBe("running");
    finish();
    await flush(); await flush();
    expect(useSession.getState().crawl).toMatchObject({ phase: "completed", crawled: 27 });
  });

  // spec: UI-018
  it("without such a view Completed applies at once; a new crawl drops a held end status", async () => {
    useSession.getState().applyTick(tick(1, status("running", 3)));
    useSession.getState().applyTick(tick(2, status("completed", 4)));
    expect(useSession.getState().crawl?.phase).toBe("completed");
    let finish!: () => void;
    offs.push(registerEndBarrier(() => new Promise<void>(resolve => { finish = resolve; })));
    useSession.getState().applyTick(tick(3, status("running", 1, "crawl-b")));
    useSession.getState().applyTick(tick(4, status("completed", 2, "crawl-b")));
    useSession.getState().applyTick(tick(5, status("running", 0, "crawl-c")));
    await flush();
    finish();
    await flush(); await flush();
    expect(useSession.getState().crawl).toMatchObject({ crawlId: "crawl-c", phase: "running" });
  });
});

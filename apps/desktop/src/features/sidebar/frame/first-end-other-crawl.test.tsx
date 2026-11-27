// UI-018 (Klarstellung 27.11.2026): Sieht ein Fenster einen Crawl erstmals bereits im Endzustand, erscheint der Endstatus
// sofort; Übersicht, Issues und Dashboard laden ohne Nutzeraktion nach und zeigen bis dahin keine Werte – nie Werte eines
// anderen Crawls oder eines früheren Stands.
import { act, cleanup, render } from "@testing-library/react";
import { Profiler } from "react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { Providers } from "../../../app/Providers";
import { setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import { useSession } from "../../../state/session";
import type { CrawlStatus } from "../../../ipc/gen/CrawlStatus";
import type { IssueRow } from "../../../ipc/gen/IssueRow";
import type { OverviewDto } from "../../../ipc/gen/OverviewDto";
import type { Tick } from "../../../ipc/gen/Tick";
import tick from "../../../ipc/__fixtures__/Tick.json";
import fixture from "../../../../e2e/sidebar/sidebar-dto.json";
import { Dashboard } from "../../dashboard/Dashboard";
import { SidebarFrame } from "./SidebarFrame";
import { useSidebarNavigation } from "./state";

// Distinctive numbers: crawl A shows 4,141 everywhere, crawl B 2,727.
const A = 4141;
const B = 2727;
const status = (crawlId: string, phase: CrawlStatus["phase"]): CrawlStatus => ({ ...(tick.crawl as CrawlStatus), crawlId, phase });
const overviewOf = (n: number): OverviewDto => ({
  ...(fixture.overview as unknown as OverviewDto),
  summary: (fixture.overview.summary as OverviewDto["summary"]).map(r => ({ ...r, count: n })),
});
const issuesOf = (n: number): IssueRow[] => (fixture.issues as IssueRow[]).map(r => ({ ...r, count: r.count > 0 ? n : 0 }));
const wait = (ms: number) => act(async () => { await new Promise(resolve => setTimeout(resolve, ms)); });

function Phase() {
  const shown = useSession(s => `${s.crawl?.crawlId ?? "none"}:${s.crawl?.phase ?? "none"}`);
  return <output data-testid="phase">{shown}</output>;
}

beforeEach(async () => {
  const core = new MockCore();
  // Answers belong to the crawl the window shows when asked; crawl B answers after a short delay.
  const later = <T,>(value: T) => new Promise<T>(resolve => setTimeout(() => resolve(value), 60));
  core.handlers.set("sidebar.overview", () => (useSession.getState().crawl?.crawlId === "crawl-b" ? later(overviewOf(B)) : overviewOf(A)));
  core.handlers.set("sidebar.issues", () => (useSession.getState().crawl?.crawlId === "crawl-b" ? later(issuesOf(B)) : issuesOf(A)));
  core.handlers.set("analysis.status", () => fixture.analysis);
  setClient(createClient(core.transport()));
  useSession.setState({ seq: -1, crawl: null, analysis: null, countsVersion: 1, issuesVersion: 1, sidebarVersion: 1, ops: {} });
  useSidebarNavigation.setState({ scope: {}, target: null });
  await setLanguage("en");
});
afterEach(() => { cleanup(); setClient(null); });

// spec: UI-018
// spec: UI-061
// spec: UI-062
it("a crawl seen first in its end state shows Completed at once, never the numbers of the previous crawl, and then its own final numbers", async () => {
  // Checked after every React commit (Profiler) and every DOM change (MutationObserver).
  let check = () => undefined as void;
  const { container } = render(<Providers><Profiler id="first-end" onRender={() => check()}><Phase /><SidebarFrame /><Dashboard /></Profiler></Providers>);
  act(() => useSession.getState().applyTick({ ...tick, seq: 1, crawl: status("crawl-a", "completed"), countsVersion: 2, issuesVersion: 2, sidebarVersion: 2 } as unknown as Tick));
  await wait(300);
  const text = () => container.textContent ?? "";
  expect(text()).toContain("4,141");
  // Every DOM state from here on: under crawl B, no number of crawl A.
  const seen: string[] = [];
  check = () => {
    const phase = container.querySelector('[data-testid="phase"]')?.textContent ?? "";
    if (phase.startsWith("crawl-b") && text().includes("4,141")) seen.push(phase);
  };
  const observer = new MutationObserver(check);
  observer.observe(container, { subtree: true, childList: true, characterData: true });
  // Crawl B: its first status is already the end status.
  act(() => useSession.getState().applyTick({ ...tick, seq: 2, crawl: status("crawl-b", "completed"), countsVersion: 3, issuesVersion: 3, sidebarVersion: 3 } as unknown as Tick));
  check();
  // The end status appears at once.
  expect(container.querySelector('[data-testid="phase"]')?.textContent).toBe("crawl-b:completed");
  for (let i = 0; i < 100 && !text().includes("2,727"); i++) { await wait(20); check(); }
  await wait(100);
  check();
  observer.disconnect();
  expect(seen).toEqual([]);
  // Without user action: crawl B's final numbers in Overview and Dashboard.
  expect(text()).toContain("2,727");
  expect(text()).not.toContain("4,141");
});

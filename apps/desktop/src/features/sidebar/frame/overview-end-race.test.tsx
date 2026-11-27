import { act, cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { Providers } from "../../../app/Providers";
import { setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import { useSession } from "../../../state/session";
import type { CrawlStatus } from "../../../ipc/gen/CrawlStatus";
import type { OverviewDto } from "../../../ipc/gen/OverviewDto";
import type { Tick } from "../../../ipc/gen/Tick";
import tick from "../../../ipc/__fixtures__/Tick.json";
import fixture from "../../../../e2e/sidebar/sidebar-dto.json";
import { SidebarFrame } from "./SidebarFrame";
import { useSidebarNavigation } from "./state";
import { useOverview, useRenderedAnswer } from "./useSidebarQuery";

// UI-018 (B4-LAGFIX2) / UI-061: „Completed“ erscheint erst, wenn die Overview den Endstand zeigt; danach steht sie
// ohne Nutzeraktion auf dem finalen Stand. Ursache B: die erste Overview-Anfrage (beim Crawl-Start gestellt, Kern zählt
// den Anfangsstand) ist beim Endtick noch unterwegs. Ursache A: die Antwort liegt im Cache, ist aber noch nicht gezeichnet.
const crawl = (phase: CrawlStatus["phase"]): CrawlStatus => ({ ...(tick.crawl as CrawlStatus), phase });
const overview = (encountered: number, crawled: number): OverviewDto => ({
  ...(fixture.overview as unknown as OverviewDto),
  summary: (fixture.overview.summary as OverviewDto["summary"]).map((r, i) => (i === 0 ? { ...r, count: encountered } : i === 1 ? { ...r, count: crawled } : r)),
});
const wait = (ms: number) => act(async () => { await new Promise(resolve => setTimeout(resolve, ms)); });
const shown = (name: string) => within(within(screen.getByRole("table", { name: "Summary" })).getByRole("row", { name: new RegExp(`^${name}`) })).getAllByRole("cell")[0]!.textContent;

let core: MockCore;
let calls = 0;
let first: (dto: OverviewDto) => void = () => undefined;
beforeEach(async () => {
  core = new MockCore();
  calls = 0;
  core.handlers.set("sidebar.overview", () => (++calls === 1 ? new Promise<OverviewDto>(resolve => { first = resolve; }) : overview(27, 27)));
  core.handlers.set("sidebar.issues", () => fixture.issues);
  core.handlers.set("analysis.status", () => fixture.analysis);
  setClient(createClient(core.transport()));
  useSession.setState({ seq: -1, crawl: crawl("running"), analysis: null, countsVersion: 1, issuesVersion: 1, sidebarVersion: 1, ops: {} });
  useSidebarNavigation.setState({ scope: {}, target: null });
  await setLanguage("en");
});
afterEach(() => { cleanup(); setClient(null); });
const end = () => act(() => useSession.getState().applyTick({ ...tick, seq: 9, crawl: crawl("completed"), countsVersion: 9, issuesVersion: 9, sidebarVersion: 9 } as unknown as Tick));

// spec: UI-018
// spec: UI-061
it("the first Overview answer, counted at the crawl start and arriving after the end tick, is not shown with Completed", async () => {
  render(<Providers><SidebarFrame /></Providers>);
  await wait(50);
  expect(calls).toBe(1);
  end();
  await act(async () => first(overview(1, 1)));
  await wait(50);
  expect(useSession.getState().crawl?.phase).toBe("completed");
  expect(shown("Total URLs Encountered")).toBe("27");
  expect(shown("Total URLs Crawled")).toBe("27");
});

// spec: UI-018
// spec: UI-061
it("the Overview reaches the final state without user action when the first answer arrives later than one refresh interval after the end tick", async () => {
  render(<Providers><SidebarFrame /></Providers>);
  await wait(50);
  end();
  await wait(700);
  await act(async () => first(overview(1, 1)));
  await wait(2_000);
  expect(useSession.getState().crawl?.phase).toBe("completed");
  expect(shown("Total URLs Encountered")).toBe("27");
  expect(shown("Total URLs Crawled")).toBe("27");
  expect(calls).toBeGreaterThan(1);
}, 10_000);

// Cause A: the Overview commits its answers deferred (SidebarFrame, useDeferredValue). The probe stands in for such a
// consumer whose commit of the reloaded answer has not happened yet: it keeps the first answer on screen until released.
let frozen: OverviewDto | undefined;
function DeferredProbe({ hold }: { hold: boolean }) {
  const query = useOverview();
  if (!hold) frozen = undefined;
  else if (query.data && !frozen) frozen = query.data;
  const shownData = hold ? frozen ?? query.data : query.data;
  useRenderedAnswer(query, shownData);
  const phase = useSession(s => s.crawl?.phase);
  return <><span data-testid="phase">{phase}</span><span data-testid="encountered">{shownData?.summary[0]?.count}</span></>;
}

// spec: UI-018
// spec: UI-061
it("Completed is not in the DOM while the Overview still shows the earlier answer; both change together once it commits the reload", async () => {
  frozen = undefined;
  core.handlers.set("sidebar.overview", () => (++calls === 1 ? overview(1, 1) : overview(27, 27)));
  const view = render(<Providers><DeferredProbe hold /></Providers>);
  expect(await screen.findByText("1")).toBeVisible();
  end();
  await wait(300);
  // The reload has answered (cache has 27), but the screen still shows 1: the end status is held.
  expect(calls).toBe(2);
  expect(screen.getByTestId("encountered").textContent).toBe("1");
  expect(screen.getByTestId("phase").textContent).toBe("running");
  view.rerender(<Providers><DeferredProbe hold={false} /></Providers>);
  await wait(50);
  expect(screen.getByTestId("encountered").textContent).toBe("27");
  expect(screen.getByTestId("phase").textContent).toBe("completed");
});

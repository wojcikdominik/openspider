import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Providers } from "../../../app/Providers";
import { setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import { useSession } from "../../../state/session";
import type { AnalysisStatus } from "../../../ipc/gen/AnalysisStatus";
import type { CrawlStatus } from "../../../ipc/gen/CrawlStatus";
import type { OverviewDto } from "../../../ipc/gen/OverviewDto";
import type { Tick } from "../../../ipc/gen/Tick";
import tick from "../../../ipc/__fixtures__/Tick.json";
import fixture from "../../../../e2e/sidebar/sidebar-dto.json";
import { SidebarFrame } from "./SidebarFrame";
import { useSidebarNavigation } from "./state";

// M2-N32: SidebarFrame hands the memoized Overview and PanelActions props that only change with their content.
const seen = vi.hoisted(() => ({ stale: [] as unknown[], rows: [] as unknown[] }));
vi.mock("../overview/Overview", async importOriginal => {
  const actual = await importOriginal<typeof import("../overview/Overview")>();
  return { ...actual, Overview: (props: Parameters<typeof actual.Overview>[0]) => { seen.stale.push(props.staleSteps); return <actual.Overview {...props} />; } };
});
vi.mock("./PanelActions", async importOriginal => {
  const actual = await importOriginal<typeof import("./PanelActions")>();
  return { ...actual, PanelActions: (props: Parameters<typeof actual.PanelActions>[0]) => { seen.rows.push(props.rows); return <actual.PanelActions {...props} />; } };
});

let core: MockCore;
const overview = fixture.overview as unknown as OverviewDto;
const analysis = fixture.analysis as unknown as AnalysisStatus;
beforeEach(async () => {
  core = new MockCore();
  core.handlers.set("sidebar.overview", () => structuredClone(overview));
  core.handlers.set("analysis.status", () => structuredClone(analysis));
  setClient(createClient(core.transport()));
  useSession.setState({ seq: -1, crawl: tick.crawl as CrawlStatus, analysis: null, countsVersion: 1, issuesVersion: 1, sidebarVersion: 1, ops: {} });
  useSidebarNavigation.setState({ scope: {}, target: null });
  await setLanguage("en");
  seen.stale.length = 0; seen.rows.length = 0;
});
afterEach(() => { cleanup(); setClient(null); });

const analysisTick = (seq: number, steps: AnalysisStatus["steps"]) =>
  act(() => useSession.getState().applyTick({ ...tick, seq, countsVersion: 1, issuesVersion: 1, sidebarVersion: 1, analysis: { ...analysis, steps } } as unknown as Tick));

// spec: UI-145
// spec: UI-018
// spec: UI-143
it("keeps stale steps and overview copy rows stable while their content is unchanged", async () => {
  render(<Providers><SidebarFrame /></Providers>);
  await waitFor(() => expect(seen.rows.length).toBeGreaterThan(0));
  await screen.findAllByRole("button", { name: /Copy/ });
  // Live analysis snapshots arrive as new objects with the same stale steps (only other step fields change).
  analysisTick(10, analysis.steps.map(s => ({ ...s })));
  const stale = seen.stale.at(-1); const rows = seen.rows.at(-1);
  expect(stale).toEqual(["near_duplicates", "content"]);
  analysisTick(11, analysis.steps.map(s => s.step === "link_score" ? { ...s, state: "running" as const } : { ...s }));
  analysisTick(12, analysis.steps.map(s => ({ ...s })));
  expect(seen.stale.at(-1)).toBe(stale);
  expect(seen.rows.at(-1)).toBe(rows);
  // A changed stale set is a new value.
  analysisTick(13, analysis.steps.map(s => ({ ...s, stale: s.step === "content" })));
  expect(seen.stale.at(-1)).toEqual(["content"]);
  expect(seen.stale.at(-1)).not.toBe(stale);
});

// UI-004 (K2-77, Abnahme fix/overview-stale B2-6): „Übersicht“ (Dashboard), gewählt bevor die Tab-Leiste ihren Stand
// geladen hat, bleibt beim Neuladen der Tab-Leiste wegen einer neuen Crawl-ID (Crawl-Start) erhalten und wird gespeichert.
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushUiState } from "../../state/uistate";
import { createClient, setClient } from "../../ipc/client";
import { uistate } from "../../ipc/uistate";
import { useTabs } from "../../state/tabs";
import { useSession } from "../../state/session";
import { setLanguage } from "../../i18n";
import type { CrawlStatus } from "../../ipc/gen/CrawlStatus";
import tick from "../../ipc/__fixtures__/Tick.json";
import { tableFixture } from "../table/fixtures/core";
import { TopTabs } from "./TopTabs";

let fx: ReturnType<typeof tableFixture>;
beforeEach(async () => {
  fx = tableFixture(); setClient(createClient(fx.core.transport()));
  useTabs.setState({ catalog: [], order: [], hidden: [], active: "Internal", queries: {}, loaded: false, focusId: undefined, focusRequest: 0, error: null, view: "tabs" });
  useSession.setState({ seq: -1, crawl: null });
  await setLanguage("en");
});
afterEach(async () => { await flushUiState(); cleanup(); vi.restoreAllMocks(); setClient(null); useSession.setState({ crawl: null }); });

// spec: UI-004
it("keeps „Übersicht“ chosen before the tab bar had loaded when a crawl start reloads the tab bar, and saves it", async () => {
  fx.core.uiState.state = {};
  const original = uistate.load.bind(uistate);
  let release!: () => void;
  let started!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const reading = new Promise<void>(resolve => { started = resolve; });
  vi.spyOn(uistate, "load").mockImplementationOnce(async () => { started(); await gate; return original(); });
  render(<TopTabs />);
  await act(async () => { await reading; });
  // The user opens „Übersicht“ while the first load is still reading the saved state.
  expect(useTabs.getState().loaded).toBe(false);
  act(() => useTabs.getState().showOverview());
  await act(async () => { release(); });
  await waitFor(() => expect(useTabs.getState().loaded).toBe(true));
  expect(useTabs.getState().view).toBe("overview");
  // A crawl starts: the new crawl id reloads the tab bar (TopTabs).
  const reload = vi.spyOn(useTabs.getState(), "load");
  act(() => useSession.setState({ crawl: { ...(tick.crawl as CrawlStatus), crawlId: "crawl-new", phase: "running" } }));
  await waitFor(() => expect(reload).toHaveBeenCalled());
  await act(async () => { await reload.mock.results[0]!.value; });
  expect(useTabs.getState().view).toBe("overview");
  await flushUiState();
  expect((await original()).state).toMatchObject({ topTabs: { view: "overview" } });
});

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushUiState } from "../../state/uistate";
import { createClient, setClient } from "../../ipc/client";
import { eventBus } from "../../ipc/events";
import { catalog } from "../../ipc/catalog";
import { uistate } from "../../ipc/uistate";
import { useTabs } from "../../state/tabs";
import { useSession } from "../../state/session";
import { setLanguage } from "../../i18n";
import type { CrawlStatus } from "../../ipc/gen/CrawlStatus";
import { fixtureTabs, tableFixture } from "../table/fixtures/core";
import { TopTabs } from "./TopTabs";
import { runAction } from "../../ui/actions";
import { useComparison } from "../compare/model";
import { session as compareSession } from "../compare/fixtures/out-compare/core";

let fx: ReturnType<typeof tableFixture>;
beforeEach(async () => {
  fx = tableFixture(); setClient(createClient(fx.core.transport()));
  useTabs.setState({ catalog: [], order: [], hidden: [], active: "Internal", queries: {}, loaded: false, focusId: undefined, focusRequest: 0, error: null });
  await setLanguage("en");
});
afterEach(async () => { await flushUiState(); cleanup(); vi.restoreAllMocks(); setClient(null); useComparison.setState({ session: null, snapshot: null }); await setLanguage("en"); });

// spec: UI-004 [P1]
it("loads only catalog tabs, persists reordering and hiding, and resets to catalog defaults", async () => {
  fx.core.uiState.state = { unrelated: { width: 320 } };
  await useTabs.getState().load();
  expect(useTabs.getState().order).toEqual(fixtureTabs.map(t => t.id));
  await useTabs.getState().reorder(["Images", "Internal", "External", "AMP"]);
  await useTabs.getState().show("AMP", false); await useTabs.getState().show("Internal", false);
  expect(useTabs.getState().hidden).toEqual(["AMP"]);
  useTabs.setState({ order: [], hidden: [] }); await useTabs.getState().load();
  expect(useTabs.getState().order[0]).toBe("Images"); expect(useTabs.getState().hidden).toEqual(["AMP"]);
  expect((await uistate.load()).state).toMatchObject({ unrelated: { width: 320 } });
  await useTabs.getState().reset();
  expect(useTabs.getState().order).toEqual(fixtureTabs.map(t => t.id)); expect(useTabs.getState().hidden).toEqual([]);
});

// spec: UI-006
it("retains each tab's last filter and sort, and accepts navigation from Overview", async () => {
  await useTabs.getState().load(); const s = useTabs.getState();
  s.select("Internal", "Client Error (4xx)"); s.query({ sort: [{ column: "Address", dir: "desc" }] });
  s.select("Images", "Unlinked"); s.select("Internal");
  expect(useTabs.getState().queries.Internal).toMatchObject({ filter: "Client Error (4xx)", sort: [{ column: "Address", dir: "desc" }] });
  s.select("Internal", "All"); expect(useTabs.getState().queries.Internal.filter).toBe("All");
});

// spec: UI-004 [P1]
it("offers checkboxes for every tab of the group, protects Internal, and reopens a hidden tab", async () => {
  render(<TopTabs />); await screen.findByRole("tab", { name: "Internal" });
  fireEvent.click(screen.getByRole("button", { name: "Configure tabs" }));
  // K2-77: the menu lists the tabs of the active group (URLs: Internal, External).
  expect(screen.getAllByRole("menuitemcheckbox").map(e => e.textContent?.replace(/^[☑☐]\s*/, ""))).toEqual(["Internal", "External"]);
  expect(screen.getByRole("menuitemcheckbox", { name: "Internal" })).toHaveAttribute("aria-disabled", "true");
  fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "External" }));
  await waitFor(() => expect(useTabs.getState().hidden).toContain("External"));
  fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "External" }));
  await waitFor(() => expect(useTabs.getState().hidden).not.toContain("External"));
  expect(screen.getByRole("menuitem", { name: "Configure tabs…" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("menuitem", { name: "Reset tabs" }));
  await waitFor(() => expect(useTabs.getState().order).toEqual(fixtureTabs.map(t => t.id)));
});

// spec: UI-004 [P1]
it("resets order and visibility only of the active group from its menus; the View menu resets all groups (K2-77)", async () => {
  // Fixture groups: URLs = Internal, External; Content = Images; Technical = AMP.
  await useTabs.getState().load();
  await useTabs.getState().reorder(["External", "Internal", "Images", "AMP"]);
  await useTabs.getState().show("AMP", false);
  render(<TopTabs />); await screen.findByRole("tab", { name: "Internal" });
  expect(screen.getAllByRole("tab").map(t => t.textContent)).toEqual(["External", "Internal"]);
  fireEvent.click(screen.getByRole("button", { name: "Configure tabs" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Reset tabs" }));
  await waitFor(() => expect(useTabs.getState().order.slice(0, 2)).toEqual(["Internal", "External"]));
  // the other group keeps its hidden tab
  expect(useTabs.getState().hidden).toEqual(["AMP"]);
  await flushUiState();
  expect(fx.core.uiState.state).toMatchObject({ topTabs: { order: ["Internal", "External", "Images", "AMP"], hidden: ["AMP"] } });
  // context menu of a tab: same per-group reset
  await act(async () => { await useTabs.getState().reorder(["External", "Internal", "Images", "AMP"]); });
  fireEvent.contextMenu(screen.getByRole("tab", { name: "Internal" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Reset tabs" }));
  await waitFor(() => expect(useTabs.getState().order.slice(0, 2)).toEqual(["Internal", "External"]));
  expect(useTabs.getState().hidden).toEqual(["AMP"]);
  // View › Reset tabs: every group
  await act(async () => { runAction("view.resetTabs"); });
  await waitFor(() => expect(useTabs.getState().hidden).toEqual([]));
});

// spec: UI-004
it("groups the tabs: navigation above the pills, a group's ▾ jumps into any of its tabs, and group and tab survive a restart (K2-77)", async () => {
  render(<TopTabs />); await screen.findByRole("tab", { name: "Internal" });
  const nav = screen.getByRole("group", { name: "Areas" });
  expect(within(nav).getAllByRole("button").map(b => b.textContent || b.getAttribute("aria-label"))).toEqual(expect.arrayContaining(["Overview", "URLs", "Content", "Tabs of Content", "Technical", "Configuration"]));
  expect(screen.getAllByRole("tab").map(t => t.textContent)).toEqual(["Internal", "External"]);
  // ≤ 2 clicks: group ▾ and the tab
  fireEvent.click(within(nav).getByRole("button", { name: "Tabs of Technical" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "AMP" }));
  await waitFor(() => expect(useTabs.getState().active).toBe("AMP"));
  expect(screen.getAllByRole("tab").map(t => t.textContent)).toEqual(["AMP"]);
  // one click: the group opens on its last tab
  fireEvent.click(within(nav).getByRole("button", { name: "URLs" }));
  expect(useTabs.getState().active).toBe("Internal");
  fireEvent.click(within(nav).getByRole("button", { name: "Technical" }));
  expect(useTabs.getState().active).toBe("AMP");
  fireEvent.click(within(nav).getByRole("button", { name: "Overview" }));
  expect(useTabs.getState().view).toBe("overview");
  expect(screen.queryByRole("tablist")).toBeNull();
  await flushUiState();
  expect(fx.core.uiState.state).toMatchObject({ topTabs: { active: "AMP", view: "overview", groupActive: { technical: "AMP", urls: "Internal" } } });
  useTabs.setState({ active: "Internal", view: "tabs", groupActive: {}, loaded: false });
  await act(async () => { await useTabs.getState().load(); });
  expect(useTabs.getState()).toMatchObject({ active: "AMP", view: "overview" });
});

// spec: UI-090
it("registers focusUrl navigation, checks crawl identity, and reveals and resets the target tab", async () => {
  const before = eventBus.listenerCount("focusUrl");
  const mounted = render(<TopTabs />); await screen.findByRole("tab", { name: "Internal" });
  useSession.setState({ crawl: { crawlId: "crawl-1" } as CrawlStatus });
  await act(async () => { await useTabs.getState().show("External", false); useTabs.getState().select("External", "Client Error (4xx)"); useTabs.getState().query({ search: "missing" }); });
  act(() => eventBus.emit({ kind: "focusUrl", crawlId: "other-crawl", urlId: 2, isInternal: false }));
  expect(useTabs.getState().focusId).toBeUndefined();
  act(() => eventBus.emit({ kind: "focusUrl", crawlId: "crawl-1", urlId: 2, isInternal: false }));
  expect(useTabs.getState()).toMatchObject({ active: "External", focusId: 2, queries: { External: { filter: "All", search: "" } } });
  expect(useTabs.getState().hidden).not.toContain("External");
  mounted.unmount(); expect(eventBus.listenerCount("focusUrl")).toBe(before);
});

// spec: UI-004
it("reloads catalog tabs and persisted preferences after switching crawls", async () => {
  const load = vi.spyOn(catalog, "tabs");
  render(<TopTabs />); await screen.findByRole("tab", { name: "Internal" });
  const before = load.mock.calls.length;
  act(() => useSession.setState({ crawl: { crawlId: "changed-crawl" } as CrawlStatus }));
  await waitFor(() => expect(load).toHaveBeenCalledTimes(before + 1));
});

// spec: UI-004 [P1]
it("keeps a reorder, a hide and a reset made while an older load is in flight (A01-19)", async () => {
  await useTabs.getState().load();
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const original = uistate.load.bind(uistate);
  // The read of the slow load returns the layout as saved before the edits.
  const stale = await original();
  vi.spyOn(uistate, "load").mockImplementationOnce(async () => { await gate; return stale; });
  const slow = useTabs.getState().load();
  await useTabs.getState().reorder(["Images", "Internal", "External", "AMP"]);
  await useTabs.getState().show("AMP", false);
  release(); await slow;
  expect(useTabs.getState().order[0]).toBe("Images");
  expect(useTabs.getState().hidden).toEqual(["AMP"]);
  await flushUiState();
  expect((await original()).state).toMatchObject({ topTabs: { order: ["Images", "Internal", "External", "AMP"], hidden: ["AMP"] } });
  // A reset followed by a late load stays reset.
  const stale2 = await original();
  vi.spyOn(uistate, "load").mockImplementationOnce(async () => { await gate; return stale2; });
  const slow2 = useTabs.getState().load();
  await useTabs.getState().reset();
  await slow2;
  expect(useTabs.getState().hidden).toEqual([]);
  expect(useTabs.getState().order).toEqual(fixtureTabs.map(t => t.id));
});

// spec: UI-004
it("keeps group navigation and Overview selected while an earlier preferences read finishes", async () => {
  await useTabs.getState().load();
  const original = uistate.load.bind(uistate);
  for (const navigate of [() => useTabs.getState().select("AMP"), () => useTabs.getState().showOverview()]) {
    await flushUiState();
    const stale = await original();
    let release!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const read = vi.spyOn(uistate, "load").mockImplementationOnce(async () => { await gate; return stale; });
    const loading = useTabs.getState().load();
    await waitFor(() => expect(read).toHaveBeenCalled());
    navigate();
    const { active, view, groupActive } = useTabs.getState();
    release(); await loading;
    expect(useTabs.getState()).toMatchObject({ active, view, groupActive });
    read.mockRestore();
  }
});

// spec: UI-004
it.each(["overview", "tab"] as const)("keeps a %s navigation made while an older UI-state load is in flight", async destination => {
  await useTabs.getState().load();
  if (destination === "tab") useTabs.getState().showOverview();
  await flushUiState();
  const original = uistate.load.bind(uistate);
  const stale = await original();
  let release!: () => void;
  let started!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const reading = new Promise<void>(resolve => { started = resolve; });
  vi.spyOn(uistate, "load").mockImplementationOnce(async () => { started(); await gate; return stale; });
  const slow = useTabs.getState().load();
  await reading;
  if (destination === "overview") useTabs.getState().showOverview();
  else useTabs.getState().select("Images");
  release(); await slow;
  expect(useTabs.getState().view).toBe(destination === "overview" ? "overview" : "tabs");
  if (destination === "tab") expect(useTabs.getState().active).toBe("Images");
  await flushUiState();
  expect((await original()).state).toMatchObject({ topTabs: { view: destination === "overview" ? "overview" : "tabs" } });
});

// spec: OUT-140
// spec: UI-004
// spec: UI-121
// B4-CMPUI: the comparison keeps the ordinary grouped tab bar (localized names, groups, overflow menu); the core adds
// Change Detection, which is its own entry of the navigation (K2-77); a tab chosen there drives the comparison.
it("keeps the ordinary tab bar during a comparison with Change Detection as its own entry", async () => {
  const changeTab = { ...fixtureTabs[0]!, id: "Change Detection", defaultOrder: 30 };
  fx.core.handlers.set("catalog.tabs", () => useComparison.getState().session ? [...fixtureTabs, changeTab] : fixtureTabs);
  await setLanguage("de");
  const mounted = render(<TopTabs />);
  await screen.findByRole("tab", { name: "Intern" });
  expect(screen.queryByRole("button", { name: "Änderungserkennung" })).toBeNull();
  act(() => { useComparison.setState({ session: compareSession, snapshot: null, revision: 1, query: { tab: "Internal", filter: "All", side: "Current" } }); });
  // Same component as without a comparison: grouped navigation and the overflow/configure menu stay.
  const entry = await screen.findByRole("button", { name: "Änderungserkennung" });
  expect(screen.getByRole("group", { name: "Bereiche" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Tabs konfigurieren" })).toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "Intern" })).toHaveAttribute("aria-selected", "true");
  fireEvent.click(entry);
  await waitFor(() => expect(screen.getByRole("tab", { name: "Änderungserkennung" })).toHaveAttribute("aria-selected", "true"));
  expect(useComparison.getState().query).toMatchObject({ tab: "Change Detection", filter: "All" });
  // Closing the comparison removes the entry; the bar falls back to Internal instead of a tab that no longer exists.
  act(() => { useComparison.setState({ session: null, revision: 2 }); });
  await waitFor(() => expect(screen.queryByRole("button", { name: "Änderungserkennung" })).toBeNull());
  await waitFor(() => expect(useTabs.getState().active).toBe("Internal"));
  mounted.unmount();
});

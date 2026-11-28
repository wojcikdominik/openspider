import { afterEach, describe, expect, it, vi } from "vitest";
import { createLayoutStore, normalizeUiState, persistLayout } from "../../state/layout";
import type { UiStateJson } from "../../ipc/gen/UiStateJson";
import fixture from "./__fixtures__/ui-state-v1.json";
import invalid from "./__fixtures__/invalid-state.txt?raw";

function memory(initial: unknown = fixture) {
  let disk = structuredClone(initial) as UiStateJson;
  const api = { load: vi.fn(async () => structuredClone(disk)), save: vi.fn(async (state: UiStateJson) => { disk = structuredClone(state); return disk; }) };
  return { api, read: () => disk };
}
afterEach(() => vi.useRealTimers());
describe("versioned shell persistence", () => {
  // spec: UI-125 [P1]
  it("migrates the previous-version fixture without mutating it, preserves other features and restores exact geometry", async () => {
    const before = JSON.stringify(fixture);
    const disk = memory();
    const store = createLayoutStore(disk.api);
    await store.getState().load(1440, 836);
    expect(store.getState().layout.sidebar.share * 1440).toBeCloseTo(500);
    expect(store.getState().layout.detail.share * 836).toBeCloseTo(300);
    expect(store.getState().layout.detail.visible).toBe(false);
    await store.getState().flush();
    const restarted = createLayoutStore(disk.api);
    await restarted.getState().load(1024, 576);
    expect(restarted.getState().sections).toEqual(store.getState().sections);
    expect(restarted.getState().layout).toEqual(store.getState().layout);
    expect(disk.read().version).toBe(2);
    expect(JSON.stringify(fixture)).toBe(before);
  });
  // spec: UI-125 [P1]
  it("falls back for corrupt, invalid and future schemas; ignores unknown layout fields", () => {
    for (const value of [invalid, null, { version: 99, state: fixture.state }, { version: 2, state: { layout: { sidebar: { share: -1 } } } }]) {
      expect(normalizeUiState(value, 1024, 576).layout.sidebar).toMatchObject({ share: 260 / 1024, collapsed: true });
    }
    expect(normalizeUiState({ version: 2, state: { layout: { ...normalizeUiState(fixture, 1440, 836).layout, extra: true } } }, 1440, 836).layout)
      .not.toHaveProperty("extra");
  });
  // spec: UI-125 [P1]
  it("loads only once under StrictMode; debounces writes for one second", async () => {
    vi.useFakeTimers();
    const disk = memory();
    const store = createLayoutStore(disk.api);
    await Promise.all([store.getState().load(1440, 836), store.getState().load(1440, 836)]);
    expect(disk.api.load).toHaveBeenCalledTimes(1);
    const off = persistLayout(store);
    store.getState().setSection("searchHistory", ["a"]);
    await vi.advanceTimersByTimeAsync(999);
    expect(disk.api.save).not.toHaveBeenCalled();
    store.getState().setSection("searchHistory", ["b"]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(disk.api.save).toHaveBeenCalledTimes(1);
    expect(store.getState().sections.searchHistory).toEqual(["b"]);
    off();
  });
  // spec: UI-125 [P1]
  // spec: UI-004
  it("does not overwrite a slice another feature saves between the fresh read and the write of a flush", async () => {
    // Like the core (uistate.save): every save bumps the revision; a save with an older revision is rejected as Stale.
    let disk: UiStateJson = { version: 2, revision: 1, state: { topTabs: { view: "tabs" } } };
    let between: (() => void) | undefined;
    const api = {
      load: vi.fn(async () => { const copy = structuredClone(disk); const run = between; between = undefined; run?.(); return copy; }),
      save: vi.fn(async (state: UiStateJson) => {
        if (state.revision != null && state.revision !== disk.revision) throw { code: "Stale", messageKey: "err-uistate-conflict", args: {} };
        disk = { ...structuredClone(state), revision: (disk.revision ?? 0) + 1 };
        return disk;
      }),
    };
    const store = createLayoutStore(api);
    await store.getState().load(1440, 836);
    store.getState().setLayout({ ...store.getState().layout, detail: { share: .5, visible: true, collapsed: false } });
    // The tab bar saves „Übersicht“ after the flush has read the file and before it writes (crawl start, B2-6).
    between = () => { disk = { ...disk, revision: (disk.revision ?? 0) + 1, state: { ...(disk.state as object), topTabs: { view: "overview" } } }; };
    await store.getState().flush();
    expect(disk.state).toMatchObject({ topTabs: { view: "overview" }, layout: store.getState().layout });
  });
  // spec: UI-125 [P1]
  it("merges fresh foreign slices after another feature writes between load and flush", async () => {
    const disk = memory();
    const store = createLayoutStore(disk.api);
    await store.getState().load(1440, 836);
    await disk.api.save({ version: 2, state: { ...fixture.state,
      header: { history: ["https://new.example/"] }, recentCrawls: ["new crawl"], sidebar: { tab: "issues" } } });
    store.getState().setLayout({ ...store.getState().layout, detail: { share: .5, visible: true, collapsed: false } });
    await store.getState().flush();
    expect(disk.read().state).toMatchObject({ header: { history: ["https://new.example/"] },
      recentCrawls: ["new crawl"], sidebar: { tab: "issues" }, layout: store.getState().layout });
  });
  // spec: UI-125 [P1]
  it("overrides only edited foreign keys and stops overriding them once saved", async () => {
    const disk = memory();
    const store = createLayoutStore(disk.api);
    await store.getState().load(1440, 836);
    store.getState().setSection("header", { history: ["local"] });
    await disk.api.save({ version: 2, state: { header: { history: ["remote"] }, tabs: ["remote tab"] } });
    await store.getState().flush();
    expect(disk.read().state).toMatchObject({ header: { history: ["local"] }, tabs: ["remote tab"] });
    await disk.api.save({ version: 2, state: { header: { history: ["new remote"] } } });
    store.getState().setSection("window", { x: 10, y: 20, width: 1440, height: 900 });
    await store.getState().flush();
    expect(disk.read().state).toHaveProperty("header", { history: ["new remote"] });
    expect(disk.read().state).not.toHaveProperty("tabs");
  });
  // spec: UI-021 [P1]
  it("clears all per-table column properties, keeps named templates and every unrelated slice, and persists after restart", async () => {
    const disk = memory();
    const store = createLayoutStore(disk.api);
    await store.getState().load(1440, 836);
    const { tables: _tables, ...expected } = store.getState().sections;
    store.getState().resetColumns();
    expect(store.getState().sections).toEqual(expected);
    expect(store.getState().sections.columnTemplates).toEqual(fixture.state.columnTemplates);
    await disk.api.save({ version: 2, state: { ...fixture.state, header: { history: ["new"] } } });
    await store.getState().flush();
    const restarted = createLayoutStore(disk.api);
    await restarted.getState().load(1440, 836);
    expect(restarted.getState().sections).toEqual({ ...expected, header: { history: ["new"] } });
  });
  // spec: UI-125 [P1]
  it("reset all clears every persisted slice and chooses defaults for the current viewport", async () => {
    const disk = memory();
    const store = createLayoutStore(disk.api);
    await store.getState().load(1440, 836);
    store.getState().resetAll(1024, 576);
    expect(Object.keys(store.getState().sections)).toEqual(["layout"]);
    expect(store.getState().layout.sidebar).toMatchObject({ collapsed: true, visible: true });
    await disk.api.save({ version: 2, state: { ...fixture.state, header: { history: ["new"] } } });
    await store.getState().flush();
    expect(disk.read().state).toEqual({ layout: store.getState().layout });
    const restarted = createLayoutStore(disk.api);
    await restarted.getState().load(1440, 836);
    expect(restarted.getState().layout).toEqual(store.getState().layout);
  });
  // spec: UI-125 [P1]
  it("serializes concurrent saves and retains edits made while the writer is busy", async () => {
    const disk = memory();
    let release!: () => void;
    disk.api.save.mockImplementationOnce(async (state) => { await new Promise<void>((resolve) => { release = resolve; }); return state; });
    const store = createLayoutStore(disk.api);
    await store.getState().load(1440, 836);
    store.getState().setSection("searchHistory", ["first"]);
    const first = store.getState().flush();
    await vi.waitFor(() => expect(release).toBeDefined());
    store.getState().setSection("searchHistory", ["second"]);
    const second = store.getState().flush();
    release();
    await Promise.all([first, second]);
    expect(disk.read().state).toHaveProperty("searchHistory", ["second"]);
    expect(disk.api.save).toHaveBeenCalledTimes(2);
  });
  // spec: UI-125 [P1]
  it("retains edits made while reading the fresh document for the next queued flush", async () => {
    const disk = memory();
    const store = createLayoutStore(disk.api);
    await store.getState().load(1440, 836);
    let release!: (state: UiStateJson) => void;
    disk.api.load.mockImplementationOnce(() => new Promise((resolve) => { release = resolve; }));
    store.getState().setSection("header", { history: ["first"] });
    const first = store.getState().flush();
    await vi.waitFor(() => expect(release).toBeDefined());
    store.getState().setSection("header", { history: ["second"] });
    store.getState().setSection("recentCrawls", ["local"]);
    const second = store.getState().flush();
    release({ version: 2, state: { sidebar: { tab: "issues" } } });
    await Promise.all([first, second]);
    expect(disk.read().state).toMatchObject({ header: { history: ["second"] }, recentCrawls: ["local"], sidebar: { tab: "issues" } });
  });
  // spec: UI-125 [P1]
  it("retries a failed fresh read without saving stale state or losing local edits", async () => {
    const disk = memory();
    const store = createLayoutStore(disk.api);
    await store.getState().load(1440, 836);
    store.getState().setSection("header", { history: ["local"] });
    disk.api.load.mockRejectedValueOnce(new Error("disconnected"));
    await expect(store.getState().flush()).rejects.toThrow("disconnected");
    expect(disk.api.save).not.toHaveBeenCalled();
    await disk.api.save({ version: 2, state: { sidebar: { tab: "issues" } } });
    await store.getState().flush();
    expect(store.getState().error).toBeNull();
    expect(disk.read().state).toMatchObject({ header: { history: ["local"] }, sidebar: { tab: "issues" } });
  });
  // spec: UI-125 [P1]
  it("does not overwrite unread state on transport errors and retries failed writes", async () => {
    const disk = memory();
    disk.api.load.mockRejectedValueOnce(new Error("disconnected"));
    const store = createLayoutStore(disk.api);
    await expect(store.getState().load(1440, 836)).rejects.toThrow("disconnected");
    await store.getState().flush();
    expect(disk.api.save).not.toHaveBeenCalled();
    await store.getState().load(1440, 836);
    disk.api.save.mockRejectedValueOnce(new Error("disk full"));
    await expect(store.getState().flush()).rejects.toThrow("disk full");
    expect(store.getState().error).not.toBeNull();
    await store.getState().flush();
    expect(store.getState().error).toBeNull();
  });
});

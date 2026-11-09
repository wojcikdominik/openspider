import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import { bindDetailPersistence, flushDetailState, scheduleDetailState } from "./persistence";
import { uistate } from "../../../ipc/uistate";

const native = vi.hoisted(() => ({ listen: vi.fn(), emit: vi.fn(async () => undefined), isTauri: false }));
vi.mock("../../../ipc/transport/tauri", async original => ({ ...await original<object>(), isTauri: () => native.isTauri }));
vi.mock("@tauri-apps/api/event", () => ({ listen: native.listen, emit: native.emit }));
beforeEach(() => { vi.useFakeTimers(); setClient(createClient(new MockCore().transport())); native.isTauri = false; });
afterEach(async () => { await flushDetailState(); setClient(null); vi.useRealTimers(); vi.restoreAllMocks(); });
const columns = [{ key: "Word Count", width: 120, visible: false, pinned: false }];
const edit = (id: string, onSaved = vi.fn(), onError = vi.fn()) => {
  scheduleDetailState(id, state => ({ ...state, detailColumns: { [id]: columns } }), onError, onSaved);
  return { onSaved, onError };
};

// spec: UI-008
// spec: UI-030
it("flushes the latest fixture column patch without advancing the one-second timer", async () => {
  const old = edit("fixture"); const current = edit("fixture");
  await flushDetailState();
  expect((await uistate.load()).state).toEqual({ detailColumns: { fixture: columns } });
  expect(current.onSaved).toHaveBeenCalledTimes(1);
  expect(old.onSaved).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1000);
  expect(current.onSaved).toHaveBeenCalledTimes(1);
});

// spec: UI-008
// spec: UI-030
it.each(["beforeunload", "pagehide"])("dispatches pending fixture edits on %s", async event => {
  const off = await bindDetailPersistence();
  const item = edit(event);
  window.dispatchEvent(new Event(event));
  await flushDetailState();
  expect(item.onSaved).toHaveBeenCalledTimes(1);
  expect((await uistate.load()).state).toEqual({ detailColumns: { [event]: columns } });
  off();
});

// spec: UI-008
// spec: UI-030
it("acknowledges native shutdown only after delayed and running saves finish, and reports failures", async () => {
  native.isTauri = true;
  let listener!: (event: { payload: string }) => void;
  const unlisten = vi.fn();
  native.listen.mockImplementation(async (_: string, callback: typeof listener) => { listener = callback; return unlisten; });
  const off = await bindDetailPersistence();
  const item = edit("native");
  native.emit.mockClear();
  listener({ payload: "os:ui-state-flushed-fixture" });
  expect(native.emit).not.toHaveBeenCalled();
  await flushDetailState();
  await vi.advanceTimersByTimeAsync(0);
  expect(item.onSaved).toHaveBeenCalledOnce();
  expect(native.emit).toHaveBeenCalledWith("os:ui-state-flushed-fixture", true);
  const save = vi.spyOn(uistate, "save").mockRejectedValueOnce(new Error("fixture disk failure"));
  const failure = edit("failure");
  listener({ payload: "os:ui-state-flushed-failure" });
  await vi.advanceTimersByTimeAsync(0);
  expect(failure.onError).toHaveBeenCalledOnce();
  expect(native.emit).toHaveBeenCalledWith("os:ui-state-flushed-failure", false);
  save.mockRestore();
  await flushDetailState();
  expect(failure.onSaved).toHaveBeenCalledOnce();
  off(); expect(unlisten).toHaveBeenCalledOnce();
});

// spec: UI-008
// spec: UI-030
it("waits for a save that already started before the close request", async () => {
  let complete!: () => void;
  const original = uistate.save;
  vi.spyOn(uistate, "save").mockImplementationOnce(dto => new Promise(resolve => {
    complete = () => { void original(dto).then(resolve); };
  }));
  const item = edit("in-flight");
  await vi.advanceTimersByTimeAsync(1000);
  expect(complete).toBeDefined();
  let done = false;
  const flush = flushDetailState().then(() => { done = true; });
  await vi.advanceTimersByTimeAsync(0);
  expect(done).toBe(false);
  expect(item.onSaved).not.toHaveBeenCalled();
  complete();
  await flush;
  expect(done).toBe(true);
  expect(item.onSaved).toHaveBeenCalledOnce();
});

import { updateUiState } from "../../../state/uistate";
import { isTauri } from "../../../ipc/transport/tauri";
import type { UiStateJson } from "../../../ipc/gen/UiStateJson";
import type { JsonValue } from "../../../ipc/gen/serde_json/JsonValue";
type State = { [key: string]: JsonValue };
let tail: Promise<unknown> = Promise.resolve();
type Pending = { timer?: ReturnType<typeof setTimeout>; update: (state: State) => State; onError: (error: unknown) => void; onSaved?: () => void };
const pending = new Map<string, Pending>();
const saving = new Set<Promise<void>>();
const latest = new Map<string, Pending>();

/** Persist a detail patch through the shared revision-aware boundary. */
export function updateDetailState(update: (state: State) => State): Promise<UiStateJson> {
  const task = tail.catch(() => undefined).then(() => updateUiState(update));
  tail = task.catch(() => undefined);
  return task;
}
function persist(key: string, item: Pending): Promise<void> {
  clearTimeout(item.timer);
  pending.delete(key);
  const task = updateDetailState(item.update).then(() => {
    if (latest.get(key) === item) latest.delete(key);
    item.onSaved?.();
  }).catch(error => {
    // Keep failed edits available for an explicit retry, without replacing a newer edit.
    if (latest.get(key) === item) pending.set(key, item);
    item.onError(error);
    throw error;
  });
  saving.add(task);
  void task.then(() => saving.delete(task), () => saving.delete(task));
  return task;
}
/** Timers survive a panel unmount, so switching tabs does not discard a just-edited layout. */
export function scheduleDetailState(key: string, update: (state: State) => State, onError: (error: unknown) => void, onSaved?: () => void): void {
  clearTimeout(pending.get(key)?.timer);
  const item: Pending = { update, onError, onSaved };
  item.timer = setTimeout(() => { void persist(key, item).catch(() => undefined); }, 1000);
  pending.set(key, item);
  latest.set(key, item);
}
/** UI-008/UI-030: drain delayed and already running saves before controlled shutdown. */
export async function flushDetailState(): Promise<void> {
  do {
    for (const [key, item] of pending) void persist(key, item).catch(() => undefined);
    await Promise.all([...saving]);
    await tail;
  } while (pending.size || saving.size);
}
/** Browser unload dispatches immediately; native shutdown waits for the acknowledgement before exiting. */
export async function bindDetailPersistence(): Promise<() => void> {
  const flush = () => { void flushDetailState().catch(() => undefined); };
  window.addEventListener("beforeunload", flush);
  window.addEventListener("pagehide", flush);
  let off: (() => void) | undefined;
  if (isTauri()) {
    const { listen, emit } = await import("@tauri-apps/api/event");
    off = await listen<string>("os:flush-ui-state", event => {
      void flushDetailState().then(() => emit(event.payload, true), () => emit(event.payload, false)).catch(() => undefined);
    });
  }
  return () => { off?.(); window.removeEventListener("beforeunload", flush); window.removeEventListener("pagehide", flush); };
}

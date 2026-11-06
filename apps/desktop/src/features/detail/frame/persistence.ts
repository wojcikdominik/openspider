import { updateUiState } from "../../../state/uistate";
import type { UiStateJson } from "../../../ipc/gen/UiStateJson";
import type { JsonValue } from "../../../ipc/gen/serde_json/JsonValue";
type State = { [key: string]: JsonValue };
let tail: Promise<unknown> = Promise.resolve();
const pending = new Map<string, ReturnType<typeof setTimeout>>();

/** Persist a detail patch through the shared revision-aware boundary. */
export function updateDetailState(update: (state: State) => State): Promise<UiStateJson> {
  const task = tail.catch(() => undefined).then(() => updateUiState(update));
  tail = task.catch(() => undefined);
  return task;
}
/** Timers survive a panel unmount, so switching tabs does not discard a just-edited layout. */
export function scheduleDetailState(key: string, update: (state: State) => State, onError: (error: unknown) => void, onSaved?: () => void): void {
  clearTimeout(pending.get(key));
  pending.set(key, setTimeout(() => {
    pending.delete(key);
    void updateDetailState(update).then(onSaved).catch(onError);
  }, 1000));
}

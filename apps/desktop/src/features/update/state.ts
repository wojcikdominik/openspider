import { create } from "zustand";
import { settings } from "../../ipc/settings";
import { update } from "../../ipc/update";
import type { UpdateState } from "../../ipc/gen/UpdateState";
import type { AvailableUpdate } from "../../ipc/gen/AvailableUpdate";
import type { InstallWhen } from "../../ipc/gen/InstallWhen";
export const useUpdate = create<{ state: UpdateState; release: AvailableUpdate | null; open: boolean; queued: InstallWhen | null }>(() => ({ state: "idle", release: null, open: false, queued: null }));

/**
 * B3-UI: a manual check that gets no answer within this time ends with a visible error and a retry button instead of an
 * endless "Checking…" (the core's own network timeout is 5 s, OUT-250; the rest covers a busy core or a stalled IPC).
 */
export const CHECK_TIMEOUT_MS = 20_000;
/** Marker of the frontend timeout in `{ failed }`; the core reports its failures as text, never with this value. */
export const CHECK_TIMEOUT = "ui-timeout";

let checkGeneration = 0;
let pendingCheck: AbortController | null = null;

/** True while a manual check waits for its answer; status polls must not overwrite its state meanwhile. */
export function checkPending(): boolean {
  return pendingCheck !== null;
}

export function accept(state: UpdateState) {
  useUpdate.setState({ state, ...(typeof state === "object" && "available" in state ? { release: state.available } : {}) });
}

/** A polled status (`update.status`) applies unless a manual check is pending or its timeout is on screen. */
export function acceptPolled(state: UpdateState) {
  if (checkPending()) return;
  const current = useUpdate.getState().state;
  if (state === "checking" && typeof current === "object" && "failed" in current && current.failed === CHECK_TIMEOUT) return;
  accept(state);
}

export function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(CHECK_TIMEOUT)), ms);
    promise.then(value => { clearTimeout(timer); resolve(value); }, error => { clearTimeout(timer); reject(error); });
  });
}

/**
 * Manual check (Hilfe → „Nach Updates suchen“, OUT-250). The dialog opens at once with "Checking…" and always ends in a
 * result or an error: a newer manual check supersedes a still pending older one (its late answer is ignored), and a
 * check without an answer ends after {@link CHECK_TIMEOUT_MS}.
 */
export async function checkUpdates() {
  const generation = ++checkGeneration;
  pendingCheck?.abort();
  const abort = new AbortController();
  pendingCheck = abort;
  useUpdate.setState({ open: true, queued: null });
  accept("checking");
  let next: UpdateState;
  try {
    next = await withTimeout(update.check(abort.signal), CHECK_TIMEOUT_MS);
  } catch (e) {
    next = { failed: e instanceof Error && e.message === CHECK_TIMEOUT ? CHECK_TIMEOUT : "check" };
  }
  if (generation !== checkGeneration) return;
  pendingCheck = null;
  abort.abort();
  accept(next);
}
export async function downloadUpdate() {
  try { accept(await update.download()); } catch { accept({ failed: "download" }); }
}
export async function installUpdate(when: InstallWhen) {
  try {
    const state = await update.install({ when });
    accept(state);
    if (typeof state === "object" && "ready" in state) useUpdate.setState({ queued: when });
  } catch { accept({ failed: "install" }); }
}

/**
 * OUT-251 (Nutzerentscheidung 09.11.2026): "Skip this version" remembers the version in the app settings
 * (`updates.skipped_version`); automatic checks then stay silent until a newer version appears. A manual check still shows it.
 */
export async function skipVersion(version: string) {
  useUpdate.setState({ open: false });
  try { await settings.set({ patch: { updates: { skippedVersion: version } } }); } catch { /* The next automatic check asks again. */ }
}
/** "Remind me later": only closes; the next regular check (at most once per 24 h, OUT-250) reports the update again. */
export function remindLater() {
  useUpdate.setState({ open: false });
}

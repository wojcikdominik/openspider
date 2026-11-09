import { useEffect } from "react";
import { useT } from "../../i18n";
import { update } from "../../ipc/update";
import { useSession } from "../../state/session";
import { registerAction } from "../../ui/actions";
import { Button, Dialog } from "../../ui/primitives";
import { isTauri } from "../../ipc/transport/tauri";
import { acceptPolled, CHECK_TIMEOUT, checkUpdates, downloadUpdate, installUpdate, useUpdate, withTimeout } from "./state";
import "./update.css";

/** A status poll that gets no answer ends after this time, so polling never stops behind a lost request (B3-UI). */
export const STATUS_TIMEOUT_MS = 10_000;

/**
 * B3-UI: the update dialog lives inside the main window. A menu click while the window is minimized or hidden would
 * open it out of sight, so the menu action brings the main window to the front first.
 */
export async function revealMainWindow(): Promise<void> {
  if (!isTauri()) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    if (await win.isMinimized()) await win.unminimize();
    await win.show();
    await win.setFocus();
  } catch { /* The dialog still opens; bringing the window forward is best effort. */ }
}

export function UpdateHost() {
  const t = useT();
  const { state, release, open, queued } = useUpdate();
  const running = useSession(s => s.crawl?.phase === "running" || s.analysis?.running === true);
  useEffect(() => registerAction("help.updates", () => { void revealMainWindow(); void checkUpdates(); }), []);
  useEffect(() => {
    let live = true;
    void update.status().then(state => { if (live) acceptPolled(state); }).catch(() => {});
    return () => { live = false; };
  }, []);
  const available = typeof state === "object" && "available" in state;
  useEffect(() => {
    if (available) {
      useUpdate.setState({ open: true });
      void downloadUpdate();
    }
  }, [available]);
  const fastPolling = state === "checking" || typeof state === "object" && "downloading" in state;
  useEffect(() => {
    let live = true;
    let busy = false;
    const refresh = async () => {
      if (busy) return;
      busy = true;
      try {
        // B3-UI: a lost answer (e.g. after the computer slept) must not leave `busy` set and stop polling for good.
        const state = await withTimeout(update.status(), STATUS_TIMEOUT_MS);
        if (live) acceptPolled(state);
      } catch { /* Startup checks remain silent when the transport is unavailable. */ }
      finally { busy = false; }
    };
    const timer = setInterval(() => void refresh(), fastPolling ? 1000 : 30000);
    // After a sleep or a long time in the background the state is fetched again at once.
    const wake = () => { if (document.visibilityState === "visible") void refresh(); };
    document.addEventListener("visibilitychange", wake);
    window.addEventListener("focus", wake);
    return () => { live = false; clearInterval(timer); document.removeEventListener("visibilitychange", wake); window.removeEventListener("focus", wake); };
  }, [fastPolling]);
  const object = typeof state === "object" ? state : null;
  const ready = object && "ready" in object;
  const downloading = object && "downloading" in object ? object.downloading : null;
  return <Dialog title={t("update-title")} isOpen={open} onOpenChange={open => useUpdate.setState({ open })}>
    <div aria-live="polite">
      {state === "checking" && <p>{t("update-checking")}</p>}
      {state === "upToDate" && <p>{t("update-current")}</p>}
      {object && "disabled" in object && <p>{t(object.disabled === "environment" ? "update-environment" : object.disabled === "settings" ? "update-disabled-settings" : object.disabled === "missingPublicKey" ? "update-disabled-missing-key" : object.disabled === "invalidPublicKey" ? "update-disabled-invalid-key" : object.disabled === "unknownInstaller" ? "update-disabled-unknown-installer" : "update-disabled-unavailable")}</p>}
      {object && "failed" in object && <p role="alert">{t(object.failed === CHECK_TIMEOUT ? "update-check-timeout" : "update-failed")}</p>}
      {release && <><h3>{t("update-version", { version: release.version })}</h3><h4>{t("update-notes")}</h4><p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{release.notes || t("update-no-notes")}</p></>}
      {downloading && <><p>{t("update-downloading", { done: downloading.done, total: downloading.total ?? "?" })}</p><progress aria-label={t("update-progress")} value={downloading.total ? downloading.done : undefined} max={downloading.total ?? undefined} /></>}
      {queued && <p role="status">{t(queued === "now" ? "update-queued-now" : queued === "onQuit" ? "update-queued-quit" : "update-queued-crawl")}</p>}
    </div>
    {/* B3-UI: the actions are stacked full width, so every label stays on one line in de and en; primary first. */}
    <div className="os-update-actions" data-update-actions="">
      {ready && !queued && (running ? <Button onPress={() => void installUpdate("afterCrawl")}>{t("update-after-crawl")}</Button> : <>
        <Button variant="primary" onPress={() => void installUpdate("now")}>{t("update-now")}</Button>
        <Button onPress={() => void installUpdate("onQuit")}>{t("update-on-quit")}</Button>
      </>)}
      {object && ("available" in object || "failed" in object && release) && <Button onPress={() => void downloadUpdate()}>{t("update-download")}</Button>}
      {object && "failed" in object && !release && <Button onPress={() => void checkUpdates()}>{t("update-retry")}</Button>}
      <Button onPress={() => useUpdate.setState({ open: false })}>{t("update-close")}</Button>
    </div>
  </Dialog>;
}

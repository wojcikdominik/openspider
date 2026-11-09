import { useEffect, useState, type ReactNode } from "react";
import { useLanguageStore, useT } from "../../i18n";
import { app } from "../../ipc/app";
import type { AppInfo } from "../../ipc/gen/AppInfo";
import { settings } from "../../ipc/settings";
import { update } from "../../ipc/update";
import { useSession } from "../../state/session";
import { registerAction } from "../../ui/actions";
import { Button, Checkbox, Dialog } from "../../ui/primitives";
import { currentPlatform, type Platform } from "../../ui/platform";
import { isTauri } from "../../ipc/transport/tauri";
import { versionLabel } from "../help/version";
import { NoteMarkdown, notesForLanguage } from "./notes";
import { acceptPolled, CHECK_TIMEOUT, checkUpdates, downloadUpdate, installUpdate, remindLater, skipVersion, useUpdate, withTimeout } from "./state";
import brandLight from "../../assets/brand/mark-light.svg";
import brandDark from "../../assets/brand/mark-dark.svg";
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
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [auto, setAuto] = useState<boolean | null>(null);
  useEffect(() => {
    if (!open) return;
    let live = true;
    // Best effort: without an answer the head names only the new version and the checkbox stays disabled.
    void Promise.resolve().then(() => app.info()).then(answer => { if (live) setInfo(answer); }).catch(() => {});
    void Promise.resolve().then(() => settings.get()).then(dto => { if (live) setAuto(dto.updates.check); }).catch(() => {});
    return () => { live = false; };
  }, [open]);
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
  const ready = !!object && "ready" in object;
  const downloading = object && "downloading" in object ? object.downloading : null;
  const failed = object && "failed" in object ? object.failed : null;
  const disabled = object && "disabled" in object ? object.disabled : null;
  // OUT-251: with a known release the dialog shows the update itself (head, notes, actions); a check without a result
  // (checking, up to date, disabled by environment/key/installer) shows only its status.
  // A ready download whose release details are not in this window (e.g. opened from a status poll) still offers its actions.
  const shown = release ?? (object && "ready" in object ? { version: object.ready.version, notes: "", pubDate: null } : null);
  // UI-127: the installed version with build number, the new one without (the manifest does not carry it).
  const installed = info ? versionLabel(t, info) : null;
  const showRelease = !!shown && state !== "checking" && state !== "upToDate" && (disabled === null || disabled === "settings");
  const status = <div aria-live="polite" className="os-update-status">
    {state === "checking" && <p>{t("update-checking")}</p>}
    {state === "upToDate" && <p>{t("update-current")}</p>}
    {disabled && <p>{t(disabled === "environment" ? "update-environment" : disabled === "settings" ? "update-disabled-settings" : disabled === "missingPublicKey" ? "update-disabled-missing-key" : disabled === "invalidPublicKey" ? "update-disabled-invalid-key" : disabled === "unknownInstaller" ? "update-disabled-unknown-installer" : "update-disabled-unavailable")}</p>}
    {failed !== null && <p role="alert">{t(failed === CHECK_TIMEOUT ? "update-check-timeout" : "update-failed")}</p>}
    {showRelease && downloading && <><p>{t("update-downloading", { done: downloading.done, total: downloading.total ?? "?" })}</p><progress aria-label={t("update-progress")} value={downloading.total ? downloading.done : undefined} max={downloading.total ?? undefined} /></>}
    {queued && <p role="status">{t(queued === "now" ? "update-queued-now" : queued === "onQuit" ? "update-queued-quit" : "update-queued-crawl")}</p>}
  </div>;
  const close = () => useUpdate.setState({ open: false });
  if (!showRelease || !shown) {
    return <Dialog title={t("update-title")} isOpen={open} onOpenChange={open => useUpdate.setState({ open })}
      footer={<UpdateFooter primary={failed !== null && !shown ? <Button variant="primary" onPress={() => void checkUpdates()}>{t("update-retry")}</Button> : null}
        secondary={[<Button key="close" autoFocus onPress={close}>{t("update-close")}</Button>]} />}>
      {status}
    </Dialog>;
  }
  const primary = queued ? null
    : failed !== null ? <Button variant="primary" autoFocus onPress={() => void downloadUpdate()}>{t("update-download")}</Button>
    : running ? <Button variant="primary" autoFocus={ready} isDisabled={!ready} onPress={() => void installUpdate("afterCrawl")}>{t("update-after-crawl")}</Button>
    : <Button variant="primary" autoFocus={ready} isDisabled={!ready} onPress={() => void installUpdate("now")}>{t("update-now")}</Button>;
  const secondary = queued ? [<Button key="close" autoFocus onPress={close}>{t("update-close")}</Button>] : [
    <Button key="later" autoFocus={!ready && failed === null} onPress={remindLater}>{t("update-later")}</Button>,
    ...(running || failed !== null ? [] : [<Button key="quit" isDisabled={!ready} onPress={() => void installUpdate("onQuit")}>{t("update-on-quit")}</Button>]),
  ];
  return <Dialog title={t("update-available-title")} isOpen={open} onOpenChange={open => useUpdate.setState({ open })} size="large"
    icon={<AppMark />}
    description={installed ? t("update-available-text", { version: shown.version, installed }) : t("update-available-text-unknown", { version: shown.version })}
    footer={<UpdateFooter extra={queued ? null : <Button onPress={() => void skipVersion(shown.version)}>{t("update-skip")}</Button>} primary={primary} secondary={secondary} />}>
    <ReleaseNotes version={shown.version} notes={shown.notes} />
    {status}
    <div className="os-update-auto">
      <Checkbox isSelected={auto ?? true} isDisabled={auto === null} onChange={check => { setAuto(check); void update.set_auto({ check }).catch(() => setAuto(!check)); }}>{t("update-auto")}</Checkbox>
    </div>
  </Dialog>;
}

/** App mark in both themes, as in "About OpenSpider". */
function AppMark() {
  return <span className="os-update-mark"><img className="os-brand-light" src={brandLight} alt="" /><img className="os-brand-dark" src={brandDark} alt="" /></span>;
}

/** Scrollable notes box: heading "OpenSpider <version>", then the part of the notes in the UI language (OUT-257 rule). */
function ReleaseNotes({ version, notes }: { version: string; notes: string }) {
  const t = useT();
  const lang = useLanguageStore(s => s.lang);
  const text = notesForLanguage(notes, lang);
  const heading = t("update-notes-heading", { version });
  // Focusable so that the box can be scrolled with the keyboard (UI-126).
  return <section className="os-update-notes" tabIndex={0} aria-label={t("update-notes")} data-update-notes="">
    <h3>{heading}</h3>
    {text ? <NoteMarkdown source={text} /> : <p>{t("update-no-notes")}</p>}
  </section>;
}

/**
 * Button row in the pattern of the other dialogs (5.6, {@link DialogFooter}): an extra action on the left, the others
 * right-aligned; macOS puts the primary action outermost right, Windows first. Labels never wrap; on narrow windows the
 * row wraps as a whole.
 */
function UpdateFooter({ extra, primary, secondary, platform = currentPlatform() }: { extra?: ReactNode; primary: ReactNode; secondary: ReactNode[]; platform?: Platform }) {
  return <div className="os-dialog-footer os-update-footer" data-platform={platform} data-update-actions="">
    {extra && <div className="os-dialog-footer-extra">{extra}</div>}
    <div className="os-dialog-footer-actions">{platform === "mac" ? <>{secondary}{primary}</> : <>{primary}{secondary}</>}</div>
  </div>;
}

import { useEffect, useRef, useState } from "react";
import { useT } from "../../i18n";
import { app } from "../../ipc/app";
import { eventBus } from "../../ipc/events";
import type { QuitReason } from "../../ipc/gen/QuitReason";
import { toCmdError } from "../../ipc/errors";
import { useLayout } from "../../state/layout";
import { registerAction } from "../../ui/actions";
import { Button } from "../../ui/primitives/Button";
import { Dialog } from "../../ui/primitives/Dialog";
import { flushDetailState } from "../detail/frame/persistence";
import { quitApp } from "../../app/windows";

/** Both the native close request notice and the menu action enter the same confirmation dialog. */
export function QuitDialog() {
  const t = useT();
  const [reasons, setReasons] = useState<QuitReason[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const pending = useRef(false);
  useEffect(() => {
    const offAction = registerAction("app.quit", async () => {
      if (pending.current) return;
      pending.current = true;
      setError(false);
      try {
        await flushDetailState();
        await useLayout.getState().flush();
        const decision = await app.requestQuit();
        if (decision.kind === "confirm") setReasons(decision.reasons);
        else await quitApp();
      } catch { setError(true); }
      finally { pending.current = false; }
    });
    const offNotice = eventBus.on("notice", (notice) => {
      if (notice.code === "app-quit-confirmation") {
        const allowed: QuitReason[] = ["crawl", "analysis", "export", "mcpSessions"];
        setReasons(allowed.filter((r) => Array.isArray(notice.args.reasons) && notice.args.reasons.includes(r)));
      }
    });
    return () => { offAction(); offNotice(); };
  }, []);
  const confirm = async (value: boolean) => {
    if (busy) return;
    setBusy(true);
    setError(false);
    try {
      if (value) { await flushDetailState(); await useLayout.getState().flush(); }
      await app.confirmQuit({ value });
      setReasons(null);
    } catch (e) {
      useLayout.setState({ error: toCmdError(e) });
      setError(true);
    } finally { setBusy(false); }
  };
  return <>
    {error && reasons === null && <div role="alert" className="os-shell-error">{t("shell-quit-error")}</div>}
    <Dialog title={t("shell-quit-title")} isOpen={reasons !== null} isDismissable={false}
      onOpenChange={(open) => { if (!open && !busy) void confirm(false); }}>
      {error && <p role="alert">{t("shell-quit-error")}</p>}
      <p>{t("shell-quit-description")}</p>
      <ul>{reasons?.map((reason) => <li key={reason}>{t(`shell-quit-${reason}`)}</li>)}</ul>
      <div className="os-dialog-footer-actions">
        <Button variant="secondary" isDisabled={busy} onPress={() => void confirm(false)}>{t("ui-dialog-cancel")}</Button>
        <Button autoFocus variant="primary" isDisabled={busy} onPress={() => void confirm(true)}>{t("shell-quit-confirm")}</Button>
      </div>
    </Dialog>
  </>;
}

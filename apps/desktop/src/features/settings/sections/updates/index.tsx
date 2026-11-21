import { useEffect, useState } from "react";
import { useT } from "../../../../i18n";
import { update } from "../../../../ipc/update";
import type { UpdateChannel } from "../../../../ipc/gen/UpdateChannel";
import type { UpdateChannelState } from "../../../../ipc/gen/UpdateChannelState";
import { usePrefs } from "../../../../state/prefs";
import { RadioGroup, Switch } from "../../../../ui/primitives";
import type { SettingsSection } from "../../frame/types";

/** UI-131: Update-Kanal direkt unter „Automatisch nach Updates suchen“; Regel und Speicherung führt der Kern (OUT-254). */
function Channel({ disabled, onSaved, onFailed }: { disabled: boolean; onSaved: () => void; onFailed: () => void }) {
  const t = useT();
  const [info, setInfo] = useState<UpdateChannelState | null>(null);
  // „Ab der nächsten Update-Prüfung …“ nur nach einem Wechsel auf Beta, bis die Seite verlassen wird.
  const [switchedToBeta, setSwitchedToBeta] = useState(false);
  useEffect(() => {
    let live = true;
    void update.channel().then(state => { if (live) setInfo(state); }).catch(() => undefined);
    return () => { live = false; };
  }, []);
  if (!info) return null;
  const choose = async (value: string) => {
    const channel: UpdateChannel = value === "beta" ? "beta" : "stable";
    if (channel === info.channel) return;
    try {
      setInfo(await update.set_channel({ channel }));
      setSwitchedToBeta(channel === "beta");
      onSaved();
    } catch {
      onFailed();
      // Die Auswahl zeigt wieder den gespeicherten Wert.
      await update.channel().then(setInfo).catch(() => undefined);
    }
  };
  const stay = info.channel === "stable" && info.prerelease;
  return <>
    <RadioGroup
      label={t("update-channel")}
      value={info.channel}
      anchor="updates.channel"
      orientation="horizontal"
      isDisabled={disabled}
      options={[
        { value: "stable", label: t("update-channel-stable") },
        { value: "beta", label: t("update-channel-beta") },
      ]}
      onChange={value => void choose(value)}
    />
    <p>{t("update-channel-explain")}</p>
    <p role="status" data-update-channel-hint>
      {stay ? t("update-channel-stay", { version: info.version }) : switchedToBeta && info.channel === "beta" ? t("update-channel-beta-next") : null}
    </p>
  </>;
}

function Updates() {
  const t = useT();
  const check = usePrefs(s => s.dto?.updates.check ?? true);
  const [environment, setEnvironment] = useState(false);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let live = true;
    void update.status().then(state => { if (live) setEnvironment(typeof state === "object" && "disabled" in state && state.disabled === "environment"); }).catch(() => undefined);
    return () => { live = false; };
  }, []);
  const setAuto = async (check: boolean) => {
    try { await update.set_auto({ check }); await usePrefs.getState().load(); setFailed(false); }
    catch { setFailed(true); }
  };
  return <div className="os-settings-group">
    <Switch isSelected={check} isDisabled={environment} anchor="updates.check" onChange={check => void setAuto(check)}>{t("update-auto")}</Switch>
    <Channel disabled={environment} onSaved={() => setFailed(false)} onFailed={() => setFailed(true)} />
    {environment && <p>{t("update-environment")}</p>}
    <p>{t("update-privacy")}</p>
    {failed && <p role="alert">{t("update-failed")}</p>}

  </div>;
}
export const section: SettingsSection = { id: "updates", order: 95, titleKey: "update-title", Component: Updates };

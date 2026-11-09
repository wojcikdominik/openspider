import { useEffect, useState } from "react";
import { useT, useLanguageStore } from "../../i18n";
import { app } from "../../ipc/app";
import { diag } from "../../ipc/diag";
import { settings } from "../../ipc/settings";
import { op } from "../../ipc/op";
import { toCmdError } from "../../ipc/errors";
import type { AppInfo } from "../../ipc/gen/AppInfo";
import type { BundlePreview } from "../../ipc/gen/BundlePreview";
import type { DebugInfo } from "../../ipc/gen/DebugInfo";
import type { LoggingSettingsDto } from "../../ipc/gen/LoggingSettingsDto";
import type { UsageStats } from "../../ipc/gen/UsageStats";
import { useSession } from "../../state/session";
import { shortcutTable } from "../../ui/shortcuts";
import { currentPlatform, type Platform } from "../../ui/platform";
import { Dialog } from "../../ui/primitives/Dialog";
import { Button } from "../../ui/primitives/Button";
import { client } from "../../ipc/client";
import { chooseFile } from "./files";
import { ABOUT_LINKS, openInBrowser } from "./feedback";
import { useHelp } from "./state";
import { versionLabel } from "./version";
import license from "../../../../../LICENSE?raw";
import brandLight from "../../assets/brand/mark-light.svg";
import brandDark from "../../assets/brand/mark-dark.svg";
import "./help.css";

export function ShortcutHelp({ platform = currentPlatform() }: { platform?: Platform }) {
  const t = useT();
  const lang = useLanguageStore((s) => s.lang);
  return <table className="os-help-table"><thead><tr><th>{t("u04-action")}</th><th>{t("u04-keys")}</th></tr></thead><tbody>
    {shortcutTable(platform, lang).map((row) => <tr key={row.id} data-shortcut={row.id}><th scope="row">{t(row.labelKey)}</th><td>{row.keys.map((key) => <kbd key={key}>{key}</kbd>)}</td></tr>)}
  </tbody></table>;
}

function About() {
  const t = useT();
  const [licenseQuery, setLicenseQuery] = useState("");
  const [licenseData, setLicenseData] = useState<typeof import("./licenses")>();
  const [info, setInfo] = useState<AppInfo>();
  const [error, setError] = useState<string>();
  useEffect(() => {
    const abort = new AbortController();
    void app.info(abort.signal).then(setInfo).catch((e) => { if (!abort.signal.aborted) setError(toCmdError(e).messageKey); });
    void import("./licenses").then((data) => { if (!abort.signal.aborted) setLicenseData(data); }).catch((e) => { if (!abort.signal.aborted) setError(toCmdError(e).messageKey); });
    return () => abort.abort();
  }, []);
  const link = (url: string, label: string) => <a href={url} onClick={(event) => { event.preventDefault(); void openInBrowser(url).catch((e) => setError(toCmdError(e).messageKey)); }}>{label}</a>;
  return <><header className="os-about-head">
      <img className="os-brand-light" src={brandLight} alt="" /><img className="os-brand-dark" src={brandDark} alt="" />
      <div>
        <p className="os-about-name">Open<span>Spider</span></p>
        <p className="os-about-tagline">{t("u04-about-tagline")}</p>
        {info && <p className="os-about-version" data-about="version">{t("u04-about-version", { version: versionLabel(t, info) })}{info.build && <> · {t("u04-about-build", { build: info.build })}</>}</p>}
      </div>
    </header>{error && <p role="alert">{t(error)}</p>}
    <dl className="os-help-properties os-about-facts">
      <div><dt>{t("u04-about-developer")}</dt><dd data-about="developer">Dominik Wojcik</dd></div>
      <div><dt>{t("u04-about-website")}</dt><dd>{link(ABOUT_LINKS.website, "openspider.de")}</dd></div>
      <div><dt>{t("u04-about-source")}</dt><dd>{link(ABOUT_LINKS.source, "github.com/wojcikdominik/openspider")}</dd></div>
      <div><dt>{t("u04-about-license-label")}</dt><dd>Apache-2.0</dd></div>
    </dl>
    <p className="os-about-copyright">{t("u04-about-copyright")}</p>
    <details><summary>{t("u04-license")}</summary><pre>{license}</pre></details>
    <details><summary>{t("u04-third-party")}</summary>
      <label>{t("u04-license-search")} <input type="search" value={licenseQuery} onChange={(event) => setLicenseQuery(event.target.value)} /></label>
      {licenseData ? licenseData.licenseDocuments.map((document) => <section key={document.name} aria-label={document.name}>
        <h3>{document.name}</h3><pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{licenseData.searchLicenseText(document.text, licenseQuery) || t("u04-license-no-match")}</pre>
      </section>) : <p role="status">{t("u04-loading")}</p>}
    </details></>;
}

export function DebugHelp() {
  const t = useT();
  const [info, setInfo] = useState<DebugInfo>();
  const [logging, setLogging] = useState<LoggingSettingsDto>();
  const [preview, setPreview] = useState<BundlePreview>();
  const [dest, setDest] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [days, setDays] = useState(30);
  useEffect(() => {
    const abort = new AbortController();
    void Promise.allSettled([diag.debugInfo(abort.signal), settings.get(abort.signal)]).then(([debug, prefs]) => {
      if (abort.signal.aborted) return;
      if (debug.status === "fulfilled") setInfo(debug.value);
      else setError(toCmdError(debug.reason).messageKey);
      if (prefs.status === "fulfilled") { setLogging(prefs.value.logging); setDays(prefs.value.logging.retentionDays); }
      else setError(toCmdError(prefs.reason).messageKey);
    });
    return () => abort.abort();
  }, []);
  const perform = async (fn: () => Promise<void>) => {
    setBusy(true); setError(undefined);
    try { await fn(); } catch (e) { setError(toCmdError(e).messageKey); } finally { setBusy(false); }
  };
  const patchLogging = (patch: Partial<LoggingSettingsDto>) => perform(async () => {
    const result = await settings.set({ patch: { logging: patch } });
    setLogging(result.logging); setDays(result.logging.retentionDays);
  });
  const paths = info?.app.paths;
  return <div className="os-help-stack" aria-busy={busy}>
    {error && <p role="alert">{t(error)}</p>}
    {info && <>
      <dl className="os-help-properties">
        {Object.entries({ version: versionLabel(t, info.app), build: info.app.build, os: info.os, arch: info.app.arch, pid: info.pid, ramLimitBytes: info.ramLimitBytes, rssEngineBytes: info.rssEngineBytes, rssRendererBytes: info.rssRendererBytes, crawlFileBytes: info.crawlFileBytes, freeDiskBytes: info.freeDiskBytes, performanceProfile: info.performanceProfile, sqliteCacheKib: info.sqliteCacheKib }).map(([key, value]) => <div key={key}><dt>{t(`u04-debug-${key}`)}</dt><dd>{value ?? "—"}</dd></div>)}
      </dl>
      <h3>{t("u04-paths")}</h3><dl className="os-help-properties">{Object.entries(paths ?? {}).map(([key, value]) => <div key={key}><dt>{t(`u04-path-${key}`)}</dt><dd>{value ?? "—"}</dd></div>)}</dl>
      <dl className="os-help-properties">{info.extra.map(({ name, value }, index) => <div key={index}><dt>{name}</dt><dd>{typeof value === "object" ? JSON.stringify(value) : String(value ?? "—")}</dd></div>)}</dl>
    </>}
    <label>{t("u04-log-level")} <select value={logging?.level ?? "info"} disabled={!logging || busy} onChange={(e) => void patchLogging({ level: e.target.value })}>
      {["error", "warn", "info", "debug", "trace"].map((level) => <option key={level}>{level}</option>)}
    </select></label>
    <p>{t("u04-log-live")}</p>
    <form onSubmit={(e) => { e.preventDefault(); if (Number.isInteger(days) && days > 0) void patchLogging({ retentionDays: days }); }}>
      <label>{t("u04-log-days")} <input type="number" min="1" step="1" required value={days} disabled={!logging || busy} onChange={(e) => setDays(e.target.valueAsNumber)} /></label>
      <Button type="submit" isDisabled={!logging || busy || !Number.isInteger(days) || days < 1}>{t("u04-apply")}</Button>
    </form>
    <Button isDisabled={busy} onPress={() => void perform(async () => { setPreview(undefined); setPreview(await diag.previewBundle()); })}>{t("u04-save-logs")}</Button>
    {preview && <section aria-label={t("u04-bundle-preview")}>
      <h3>{t("u04-bundle-preview")}</h3><ul>{preview.files.map((file) => <li key={file}>{file}</li>)}</ul>
      {preview.warnings.map((warning, index) => <p key={index} role="status">{warning}</p>)}
      {client.transportName === "tauri" ? <Button isDisabled={busy} onPress={() => void perform(() => chooseFile({
        kind: "save", title: "u04-destination", extensions: ["zip"], defaultPath: "openspider-diagnostics.zip",
        submit: async (dest) => { const handle = await diag.bundle({ dest }); setPreview(undefined); return handle; },
      }))}>{t("u04-confirm-bundle")}</Button> : <form onSubmit={(e) => { e.preventDefault(); if (dest.trim()) void perform(async () => { const handle = await diag.bundle({ dest: dest.trim() }); useHelp.setState({ operation: handle }); setPreview(undefined); }); }}>
        <label>{t("u04-destination")} <input value={dest} required onChange={(e) => setDest(e.target.value)} placeholder="openspider-diagnostics.zip" /></label>
        <Button type="submit" isDisabled={busy || !dest.trim()}>{t("u04-confirm-bundle")}</Button>
        <Button isDisabled={busy} onPress={() => setPreview(undefined)}>{t("u04-cancel")}</Button>
      </form>}
    </section>}
  </div>;
}

function UsageHelp() {
  const t = useT();
  const [stats, setStats] = useState<UsageStats>();
  const [error, setError] = useState<string>();
  useEffect(() => {
    const abort = new AbortController();
    void diag.usageStats({ range: "all" }, abort.signal).then(setStats).catch((e) => { if (!abort.signal.aborted) setError(toCmdError(e).messageKey); });
    return () => abort.abort();
  }, []);
  if (error) return <p role="alert">{t(error)}</p>;
  if (!stats) return <p role="status">{t("u04-loading")}</p>;
  if (!stats.crawls && !stats.urls && !stats.perMonth.length) return <p>{t("u04-usage-empty")}</p>;
  return <table className="os-help-table"><thead><tr><th>{t("u04-month")}</th><th>{t("u04-crawls")}</th><th>{t("u04-urls")}</th></tr></thead><tbody>{stats.perMonth.map((bucket) => <tr key={bucket.month}><th scope="row">{bucket.month}</th><td>{bucket.crawls}</td><td>{bucket.urls}</td></tr>)}</tbody></table>;
}

function PathDialog() {
  const t = useT();
  const request = useHelp((s) => s.path);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  if (!request) return null;
  return <Dialog title={t(request.title, undefined, { fallback: request.titleFallback })} isOpen onOpenChange={(open) => { if (!open && !busy) useHelp.setState({ path: null }); }} isDismissable={!busy}>
    <form className="os-help-stack" onSubmit={(e) => {
      e.preventDefault(); if (busy || !value.trim()) return;
      setBusy(true); setError(undefined);
      void request.submit(value.trim()).then((operation) => { useHelp.setState({ path: null, ...(operation ? { operation } : {}) }); }).catch((e) => setError(toCmdError(e).messageKey)).finally(() => setBusy(false));
    }}>
      <label>{t(request.directory ? "u04-directory" : "u04-path")} <input autoFocus value={value} required onChange={(e) => setValue(e.target.value)} /></label>
      {error && <p role="alert">{t(error)}</p>}
      <div><Button type="submit" isDisabled={busy || !value.trim()}>{t("u04-continue")}</Button> <Button isDisabled={busy} onPress={() => useHelp.setState({ path: null })}>{t("u04-cancel")}</Button></div>
    </form>
  </Dialog>;
}

export function HelpDialogs() {
  const t = useT();
  const { page, path, message, operation } = useHelp();
  const status = useSession((s) => operation ? s.ops[operation.opId] : undefined);
  const close = () => useHelp.setState({ page: null });
  return <>
    {page && <Dialog title={t(page === "about" ? "menu-item-about" : page === "shortcuts" ? "menu-item-shortcuts" : `u04-help-${page}`)} isOpen onOpenChange={(open) => { if (!open) close(); }} size="large" footer={<Button onPress={close}>{t("u04-close")}</Button>}>
      {page === "shortcuts" ? <ShortcutHelp /> : page === "about" ? <About /> : page === "debug" ? <DebugHelp /> : <UsageHelp />}
    </Dialog>}
    {path && <PathDialog key={path.title} />}
    {message && <Dialog title={t("u04-notice")} isOpen onOpenChange={() => useHelp.setState({ message: null })} footer={<Button onPress={() => useHelp.setState({ message: null })}>{t("u04-close")}</Button>}><p role="alert">{message}</p></Dialog>}
    {operation && <div role="status" className="os-help-operation">
      {status?.error ? t(status.error.messageKey, status.error.args) : status ? `${t(status.labelKey)}: ${status.done}${status.total == null ? "" : ` / ${status.total}`} · ${t(`u04-op-${status.state}`)}` : t("u04-op-running")}
      {(!status || status.state === "running") ? <Button onPress={() => void op.cancel({ opId: operation.opId }).catch((e) => useHelp.setState({ message: t(toCmdError(e).messageKey) }))}>{t("u04-cancel")}</Button> : <Button onPress={() => useHelp.setState({ operation: null })}>{t("u04-close")}</Button>}
    </div>}
  </>;
}

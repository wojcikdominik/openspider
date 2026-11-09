import { useShallow } from "zustand/react/shallow";
import { StatusCard } from "../statusbar/StatusCard";
import { needsGuardConfirmation } from "../config/pages/auth-forms/session";
import { useEffect, useRef, useState } from "react";
import { useT } from "../../i18n";
import { config } from "../../ipc/config";
import { crawl } from "../../ipc/crawl";
import type { CrawlModeDto } from "../../ipc/gen/CrawlModeDto";
import type { StartRequest } from "../../ipc/gen/StartRequest";
import { updateUiState } from "../../state/uistate";
import { uistate } from "../../ipc/uistate";
import { useSession } from "../../state/session";
import { useAction } from "../../ui/actions";
import { Button } from "../../ui/primitives/Button";
import { Dialog, DialogFooter } from "../../ui/primitives/Dialog";
import { Globe, Play, Pause, Square, Trash2, Settings } from "lucide-react";
import { runAction } from "../../ui/actions";
import brandLight from "../../assets/brand/mark-light.svg";
import brandDark from "../../assets/brand/mark-dark.svg";
import { eventBus } from "../../ipc/events";
import { reportError } from "../notifications/controller";
import { missingFolderSlash, normalizeInput, rememberUrl, useHeader } from "./model";
import { closeComparison, useComparison } from "../compare/model";
import "./header.css";

export function Header() {
  const t = useT();
  // Numeric telemetry belongs to StatusCard; controls only depend on permissions/phase.
  const status = useSession(useShallow(s => s.crawl ? {
    crawlId: s.crawl.crawlId, mode: s.crawl.mode, phase: s.crawl.phase,
    endReason: s.crawl.endReason, activeThreads: s.crawl.activeThreads,
    controllable: s.crawl.owner.controllable, startUrl: s.crawl.startUrl ?? null,
  } : null));
  const { mode, url, scope } = useHeader();
  const [history, setHistory] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [pending, setPending] = useState<"pause" | "stop" | null>(null);
  const [list, setList] = useState("");
  const [listKind, setListKind] = useState("text");
  const [sitemap, setSitemap] = useState("");
  const [listLabel, setListLabel] = useState("");
  const [confirm, setConfirm] = useState<null | { mode?: CrawlModeDto; request?: StartRequest }>(null);
  const [guardRequest, setGuardRequest] = useState<StartRequest | null>(null);
  const [threads, setThreads] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const normalized = normalizeInput(url);
  const normalizedSitemap = normalizeInput(sitemap);
  const sitemapMode = mode === "list" && listKind === "sitemap";
  const validSitemap = normalizedSitemap && /\.xml(?:\.gz)?$/i.test(new URL(normalizedSitemap).pathname);
  const running = status?.phase === "running";
  const resumable = status?.phase === "paused" || status?.phase === "incomplete";
  const foreign = !!status && !status.controllable;
  const hasResults = !!status && status.phase !== "idle";
  const allowed = !busy && !pending && !foreign;
  const comparing = useComparison((c) => !!c.session);
  const shownMode: CrawlModeDto = comparing ? "compare" : mode;
  const canStart = ["spider", "list", "apis"].includes(mode) && allowed && !running && !resumable && (mode === "spider" ? !!normalized : sitemapMode ? !!validSitemap : !!list.trim());

  useEffect(() => {
    let live = true;
    void uistate.load().then((dto) => {
      const state = object(dto.state);
      const urls = object(state.header).history;
      if (live && Array.isArray(urls)) setHistory([...new Set(urls.filter((u): u is string => typeof u === "string" && !!normalizeInput(u)))].slice(0, 20));
    }).catch(reportError);
    const loadThreads = () => void config.get().then((dto) => {
      if (live) setThreads(Number(object(object(dto.config).speed).threads) || 0);
    }).catch(() => undefined);
    loadThreads();
    // Configuration changes can arrive during a crawl; the warning follows the configured thread count.
    const off = useSession.subscribe((s, before) => {
      if (s.crawl?.activeThreads !== before.crawl?.activeThreads) loadThreads();
    });
    const offSettings = eventBus.on("settingsChanged", loadThreads);
    return () => { live = false; off(); offSettings(); };
  }, []);

  useEffect(() => {
    if (status) useHeader.setState({ mode: status.mode });
  }, [status?.crawlId, status?.mode]);

  // UI-003 (OS-Festlegung, B3-UI): an opened crawl brings its start URL into the field; it follows only a new
  // crawl (or the URL arriving with a later status), so typing in the field is never overwritten by live ticks.
  // A cleared view (empty crawlId, phase idle) never brings back the start URL of the cleared crawl.
  useEffect(() => {
    if (status?.startUrl && status.crawlId && status.phase !== "idle") useHeader.setState({ url: status.startUrl });
  }, [status?.crawlId, status?.startUrl]);

  useEffect(() => {
    if (!pending || !status) return;
    // Natural completion can win the race with a control request. A paused
    // status is only an acknowledgement for Pause, never for Stop from pause.
    if (pending === "pause" && status.phase !== "running" || pending === "stop" && ["completed", "failed", "idle"].includes(status.phase)) setPending(null);
  }, [pending, status?.phase, status?.endReason]);

  async function perform(action: () => Promise<unknown>) {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    try { await action(); } catch (e) { setPending(null); reportError(e); }
    finally { busyRef.current = false; setBusy(false); }
  }

  async function saveHistory(value: string) {
    const next = rememberUrl(history, value);
    setHistory(next);
    try {
      await updateUiState(state => {
        const header = object(state.header);
        const saved = Array.isArray(header.history) ? header.history.filter((url): url is string => typeof url === "string") : [];
        return { ...state, header: { ...header, history: rememberUrl([...new Set(saved)], value) } } as typeof state;
      });
    } catch (e) { reportError(e); }
  }

  async function start(request: StartRequest, guardConfirmed = false) {
    await perform(async () => {
      try {
        if (!guardConfirmed && needsGuardConfirmation((await config.get()).config)) {
          setGuardRequest(request);
          return;
        }
        await crawl.start(request);
        if (request.source.kind === "spider") await saveHistory(request.source.url);
      } catch (e) {
        reportError(e, request.source.kind === "spider" ? { startUrl: request.source.url } : {});
      }
    });
  }

  function requestStart() {
    if (!canStart) return false;
    void perform(async () => {
      let request: StartRequest;
      if (mode === "spider") {
        if (!normalized) return;
        const validated = await crawl.validateStartUrl({ input: normalized });
        useHeader.setState({ url: validated.normalized });
        request = { source: { kind: "spider", url: validated.normalized }, scope };
      } else if (sitemapMode && normalizedSitemap) {
        const validated = await crawl.validateStartUrl({ input: normalizedSitemap });
        request = { source: { kind: "list", items: [validated.normalized], label: t("header-sitemap") }, scope: "domain" };
      } else {
        const items = list.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
        const preview = await crawl.parseList({ kind: "text", text: list });
        if (preview.invalid > 0 || preview.valid === 0) {
          useSession.getState().pushNotice({ level: "error", code: "header-list-invalid", args: { count: preview.invalid } });
          return;
        }
        request = { source: mode === "apis" ? { kind: "apis", items } : { kind: "list", items, label: listLabel || t("header-list") }, scope: "domain" };
      }
      if (hasResults) setConfirm({ request });
      else {
        // release the lock before delegating to start; still one command at a time
        busyRef.current = false;
        await start(request);
      }
    });
  }

  const pauseResume = () => {
    if (!allowed || (!running && !resumable)) return false;
    if (running) setPending("pause");
    void perform(() => running ? crawl.pause() : crawl.resume());
  };
  const stop = () => {
    if (!allowed || (!running && !resumable)) return false;
    setPending("stop");
    void perform(() => crawl.stop());
  };
  const clear = () => {
    if (!allowed || running || !hasResults) return false;
    void perform(() => crawl.clear());
  };
  useAction("crawl.start", requestStart);
  useAction("crawl.pause", pauseResume);
  useAction("crawl.new", clear);
  useAction("header.focusUrl", () => input.current?.focus());

  function chooseMode(next: CrawlModeDto) {
    // UI-013/OUT-140: while a comparison is open the mode reads „Vergleich“; another mode closes the comparison.
    if (comparing) { if (next !== "compare") void closeComparison().then(() => useHeader.setState({ mode: next })); return; }
    if (!allowed || running || next === mode) return;
    if (hasResults) setConfirm({ mode: next });
    else useHeader.setState({ mode: next });
  }

  return <div className="os-crawl-header" data-testid="crawl-header">
    <h1 className="os-brand" aria-label="OpenSpider"><img className="os-brand-light" src={brandLight} alt="" /><img className="os-brand-dark" src={brandDark} alt="" /><span>Open<span>Spider</span></span></h1>
    <form onSubmit={(e) => { e.preventDefault(); requestStart(); }}>
      <label className="os-header-mode"><span className="os-visually-hidden">{t("header-mode")}</span><select value={shownMode} disabled={!allowed || running} onChange={(e) => chooseMode(e.target.value as CrawlModeDto)}>
        <option value="spider">{t("header-spider")}</option><option value="list">{t("header-list")}</option><option value="apis">APIs</option>
        {(shownMode === "serp" || shownMode === "compare") && <option value={shownMode}>{t(`header-${shownMode}`)}</option>}
      </select></label>
      {mode === "spider" ? <>
        <label className="os-start-url"><span className="os-visually-hidden">{t("header-url")}</span><Globe size={16} aria-hidden="true" /><input ref={input} value={url} list="os-start-history" aria-invalid={!normalized} aria-describedby={!normalized ? "os-url-error" : undefined}
          onChange={(e) => useHeader.setState({ url: e.target.value })} autoComplete="off" spellCheck={false} /></label>
        <datalist id="os-start-history">{history.map((u) => <option key={u} value={u} />)}</datalist>
        <label className="os-header-scope"><span className="os-visually-hidden">{t("header-scope")}</span><select value={scope} disabled={!allowed || running || resumable} onChange={(e) => useHeader.setState({ scope: e.target.value as StartRequest["scope"] })}>
          <option value="domain">{t("header-domain")}</option><option value="subdomains">{t("header-subdomains")}</option><option value="folder">{t("header-folder")}</option>
        </select></label>
      </> : <>
        {mode === "list" && <label>{t("header-source")}<select value={listKind} disabled={!allowed || running} onChange={(e) => setListKind(e.target.value)}>
          <option value="text">{t("header-list")}</option><option value="sitemap">{t("header-sitemap")}</option>
        </select></label>}
        {sitemapMode ? <label className="os-start-url">{t("header-sitemap")}<input value={sitemap} aria-invalid={!validSitemap} onChange={(e) => setSitemap(e.target.value)} /></label> : <>
        <label>{t("header-upload")}<input type="file" accept=".txt,.csv" disabled={!allowed || running} onChange={(e) => {
          const file = e.target.files?.[0];
          if (file) void perform(async () => { setList(await file.text()); setListLabel(file.name); });
          e.target.value = "";
        }} /></label>
        <label className="os-start-url">{t("header-paste")}<textarea value={list} onChange={(e) => { setList(e.target.value); setListLabel(""); }} /></label>
        </>}
      </>}
      {running || resumable || pending ?
        <Button variant="primary" isDisabled={!allowed || (!running && !resumable)} onPress={pauseResume}>{resumable ? <Play size={16} aria-hidden="true" /> : <Pause size={16} aria-hidden="true" />}{t(pending === "pause" ? "n03-header-pausing" : resumable ? "header-resume" : "header-pause")}</Button> :
        <Button variant="primary" type="submit" isDisabled={!canStart}><Play size={16} aria-hidden="true" />{t("header-start")}</Button>}
      <Button isDisabled={!allowed || (!running && !resumable)} onPress={stop}><Square size={16} aria-hidden="true" />{t(pending === "stop" ? "n03-header-stopping" : "n03-header-stop")}</Button>
      <Button isDisabled={!allowed || running || !hasResults} onPress={clear}><Trash2 size={16} aria-hidden="true" />{t("header-clear")}</Button>
      <Button className="os-header-settings" aria-label={t("u35-header-settings")} onPress={() => { runAction("app.settings"); }}><Settings size={16} aria-hidden="true" /></Button>
    </form>
    <StatusCard />
    {!normalized && mode === "spider" && <span id="os-url-error" className="os-header-hint">{t("header-invalid")}</span>}
    {sitemapMode && !validSitemap && <span className="os-header-hint">{t("header-sitemap-hint")}</span>}
    {mode === "spider" && missingFolderSlash(normalized, scope) && <span className="os-header-hint" role="status">{t("header-slash")}</span>}
    {Math.max(threads, status?.activeThreads ?? 0) >= 20 && <span className="os-header-hint" role="status">{t("header-thread-warning")}</span>}
    {foreign && <span className="os-header-hint">{t("header-readonly")}</span>}
    <Dialog title={t("form-login-guard-title")} isOpen={guardRequest !== null} onOpenChange={(open) => { if (!open) setGuardRequest(null); }}
      footer={<DialogFooter okLabel={t("form-login-guard-start")} cancelLabel={t("form-login-cancel")} onCancel={() => setGuardRequest(null)} onOk={() => { const request = guardRequest; setGuardRequest(null); if (request) void start(request, true); }} />}>
      {t("form-login-guard-warning")}
    </Dialog>
    <Dialog title={t("header-confirm-title")} isOpen={confirm !== null} onOpenChange={(open) => { if (!open) setConfirm(null); }}
      footer={<DialogFooter okLabel={t("header-confirm")} cancelLabel={t("header-cancel")} onCancel={() => setConfirm(null)} onOk={() => {
        const choice = confirm; setConfirm(null);
        if (choice?.request) void start(choice.request);
        else if (choice?.mode) void perform(async () => { await crawl.clear(); useHeader.setState({ mode: choice.mode! }); });
      }} />}>
      {t("header-confirm-body")}
    </Dialog>
  </div>;
}

function object(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

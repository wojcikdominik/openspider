import { loadRecent, updateRecent } from "../help/recent";
import { openComparison, useComparison } from "../compare/model";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useT, i18n, currentLanguage } from "../../i18n";
import { crawl } from "../../ipc/crawl";
import { crawls } from "../../ipc/crawls";
import { projects } from "../../ipc/projects";
import { config } from "../../ipc/config";
import { op } from "../../ipc/op";
import { describeError, toCmdError } from "../../ipc/errors";
import type { CrawlListing } from "../../ipc/gen/CrawlListing";
import type { ProjectInfo } from "../../ipc/gen/ProjectInfo";
import type { OpHandle } from "../../ipc/gen/OpHandle";
import type { OpenCrawlArgs } from "../../ipc/gen/OpenCrawlArgs";
import { useSession } from "../../state/session";
import { useAction, registerAction } from "../../ui/actions";
import { registerMenuContributions } from "../../ui/menu/registry";
import type { MenuEntry } from "../../ui/primitives/Menu";
import { Banner, Button, ContextMenu, Dialog, DialogFooter, ProgressBar, Tree } from "../../ui/primitives";
import { nativeCrawlDesktop, type CrawlDesktop, type RepairResult } from "./desktop";
import { cell, columns, openCrawls, permissions, phaseKey, readRecent, sortRows, useCrawlsDialog, writeRecent, type Column } from "./model";
import "./crawls.css";

type Form = { kind: "rename" | "move" | "projectCreate" | "projectRename"; ids: string[]; projectId?: string; value: string; error?: string };
type OpenRequest = { args: OpenCrawlArgs; resume: boolean };
export function CrawlsHost({ desktop = nativeCrawlDesktop }: { desktop?: CrawlDesktop }) {
  const t = useT();
  const open = useCrawlsDialog(s => s.open);
  useEffect(() => {
    if (!open) return;
    const opener = useCrawlsDialog.getState().returnFocus;
    return () => { requestAnimationFrame(() => { if (opener?.isConnected) opener.focus(); }); };
  }, [open]);
  const [rows, setRows] = useState<CrawlListing[]>([]);
  const [projectList, setProjectList] = useState<ProjectInfo[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [project, setProject] = useState("all");
  const [search, setSearch] = useState("");
  const [show, setShow] = useState("all");
  const [sort, setSort] = useState<{ column: Column; dir: "asc" | "desc" }>({ column: "startedAt", dir: "desc" });
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [error, setError] = useState<{ text: string; path?: string; kind?: "info" | "error" } | null>(null);
  const [recent, setRecent] = useState(readRecent);
  const [form, setForm] = useState<Form | null>(null);
  const [deletion, setDeletion] = useState<CrawlListing[] | null>(null);
  const [pendingOpen, setPendingOpen] = useState<OpenRequest | null>(null);
  const [active, setActive] = useState<{ handle: OpHandle; repair: boolean } | null>(null);
  const status = useSession(s => active ? s.ops[active.handle.opId] : undefined);
  const phase = useSession(s => s.crawl?.phase);
  const currentCrawl = useSession(s => s.crawl?.crawlId);
  useEffect(() => {
    let live = true;
    void loadRecent().then(entries => { if (live) setRecent(writeRecent([...entries.map(e => e.path), ...readRecent()])); }).catch(() => undefined);
    return () => { live = false; };
  }, [currentCrawl, phase]);
  const [cancelling, setCancelling] = useState(false);
  const requestVersion = useRef(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const anchor = useRef<string | null>(null);
  const running = !!active && (!status || status.state === "running");
  const disabled = busy || running || loading;
  // Disabling a pressed native button moves focus to body in WebKit/Chromium. Keep it inside
  // the active modal so Escape and screen-reader navigation continue to work during an operation.
  useLayoutEffect(() => {
    if (!open) return;
    let mounted = true;
    const keepFocus = () => {
      if (!mounted) return;
      const focused = document.activeElement;
      if (focused !== document.body && !focused?.matches(":disabled")) return;
      const dialogs = document.querySelectorAll<HTMLElement>('[role="dialog"]');
      dialogs[dialogs.length - 1]?.focus();
    };
    // Restore before paint/key input; a deferred animation frame can lose Escape on WebKit.
    keepFocus();
    const onFocusOut = () => queueMicrotask(keepFocus);
    document.addEventListener("focusout", onFocusOut);
    return () => { mounted = false; document.removeEventListener("focusout", onFocusOut); };
  }, [open, disabled]);
  const textError = useCallback((e: unknown) => {
    const error = toCmdError(e);
    const fileMessages: Record<string, string> = {
      "err-crawl-corrupt": "crawls-error-corrupt",
      "err-not-a-crawl-file": "crawls-error-foreign",
      "err-crawl-newer-version": "crawls-error-newer",
      "err-crawl-migration-required": "crawls-error-migration",
      "err-file-not-found": "crawls-error-missing",
      "err-file-exists": "crawls-error-exists",
    };
    const message = fileMessages[error.messageKey];
    return message ? t(message, error.args) : describeError(t, error, key => i18n.has(currentLanguage(), key)).text;
  }, [t]);
  const refresh = useCallback(async () => {
    const version = ++requestVersion.current;
    setLoading(true);
    try {
      const [items, ps] = await Promise.all([crawls.list({ show: "all" }), projects.list()]);
      if (version !== requestVersion.current) return;
      setRows(items.map(row => ({ ...row, locked: row.locked || ps.some(p => p.name === row.project && p.locked) })));
      setProjectList(ps);
      setSelected(old => new Set([...old].filter(id => items.some(r => r.crawl.crawlId === id))));
    } finally { if (version === requestVersion.current) setLoading(false); }
  }, []);
  const attempt = async (work: () => Promise<unknown>, onError?: (text: string) => void) => {
    if (busyRef.current) return;
    busyRef.current = true; setBusy(true); setError(null);
    try { await work(); } catch (e) { if (onError) onError(textError(e)); else setError({ text: textError(e) }); }
    finally { busyRef.current = false; setBusy(false); }
  };
  useEffect(() => {
    if (open) void refresh().catch(e => setError({ text: textError(e) }));
    return () => { requestVersion.current++; };
  }, [open, phase, refresh, textError]);
  useEffect(() => {
    if (active && status && status.state !== "running" && open) void refresh().catch(e => setError({ text: textError(e) }));
  }, [active, status?.state, open, refresh, textError]);
  useEffect(() => { setPage(0); setSelected(new Set()); }, [search, show, project]);
  const visible = useMemo(() => {
    const needle = search.toLocaleLowerCase();
    return sortRows(rows.filter(r => (project === "all" || (project === "none" ? !r.project : r.project === projectList.find(p => p.id === project)?.name))
      && (show === "all" || (r.crawl.mode === "compare") === (show === "comparisons"))
      && [r.crawl.name, r.project ?? "", r.startUrl ?? ""].some(v => v.toLocaleLowerCase().includes(needle))), sort.column, sort.dir);
  }, [rows, project, projectList, show, search, sort]);
  useEffect(() => { setPage(p => Math.min(p, Math.max(0, Math.ceil(visible.length / 100) - 1))); }, [visible.length]);
  const chosen = visible.filter(r => selected.has(r.crawl.crawlId));
  const ids = chosen.map(r => r.crawl.crawlId);
  const allowed = permissions(chosen);
  const selectedProject = projectList.find(p => p.id === project);
  const startOp = (handle: OpHandle, repair = false) => { setActive({ handle, repair }); setCancelling(false); };
  const openNow = async ({ args, resume }: OpenRequest) => {
    try {
      const summary = await crawl.open(args);
      setRecent(writeRecent([summary.file, ...readRecent()]));
      if (resume) await crawl.resume();
      useCrawlsDialog.setState({ open: false });
    } catch (e) {
      const err = toCmdError(e);
      if ((err.code === "Busy" || err.code === "CrawlRunning") && useSession.getState().crawl?.phase === "running") {
        setPendingOpen({ args, resume });
        return;
      }
      const path = args.kind === "path" ? args.path : rows.find(r => r.crawl.crawlId === args.crawlId)?.crawl.file;
      let missing = err.code === "NotFound";
      // Path canonicalization currently reports a missing file as Io. Verify it through the
      // read-only file adapter before removing history; permissions and schema errors remain.
      if (err.code === "Io" && path) {
        try { await desktop.check(path); } catch (checkError) { missing = toCmdError(checkError).code === "NotFound"; }
      }
      if (missing && path) {
        const remaining = writeRecent(readRecent().filter(p => p !== path));
        setRecent(remaining);
        await updateRecent(entries => entries.filter(entry => entry.path !== path));
      }
      setError({ text: `${path ?? ""}: ${missing ? t("crawls-error-missing") : textError(e)}`, path: err.messageKey === "err-crawl-corrupt" ? path : undefined });
    }
  };
  // The core recognizes an already-open file before checking whether replacement needs a pause.
  const requestOpen = (args: OpenCrawlArgs, resume = false) => openNow({ args, resume });
  const chooseOpen = () => attempt(async () => { const paths = await desktop.chooseOpen(false, t("crawls-open")); if (paths?.[0]) await requestOpen({ kind: "path", path: paths[0] }); });
  const saveAs = () => attempt(async () => {
    if (!useSession.getState().crawl || useSession.getState().crawl?.phase === "running" || running) return;
    const dest = await desktop.chooseSave(t("crawls-save"), "crawl.ospider");
    if (dest) startOp(await crawl.saveAs({ dest }));
  });
  useAction("crawls.manage", () => openCrawls());
  useAction("crawl.open", chooseOpen);
  useAction("crawl.saveAs", saveAs);
  useAction("crawls.import", () => importFiles());
  useAction("crawls.export", () => attempt(async () => {
    const current = useSession.getState().crawl;
    if (!current || current.phase === "running" || running) return;
    const dest = await desktop.chooseSave(t("crawls-export"), "crawl.ospider");
    if (dest) startOp(await crawls.export({ ids: [current.crawlId], dest }));
  }));
  const importFiles = () => attempt(async () => { const paths = await desktop.chooseOpen(true, t("crawls-import")); if (paths?.length) startOp(await crawls.import({ paths })); });
  useEffect(() => {
    const offs = recent.map((path, i) => registerAction(`crawls.recent.${i}`, () => attempt(() => requestOpen({ kind: "path", path }))));
    offs.push(registerMenuContributions("features/crawls", [{ menu: "file", group: "management", order: 20, items: [
      { id: "crawls.manage", labelKey: "crawls-title", action: "crawls.manage" },
      { id: "crawls.import", labelKey: "crawls-import", action: "crawls.import", enabled: () => !disabled },
      { id: "crawls.export", labelKey: "crawls-export", action: "crawls.export", enabled: c => c.hasCrawl === true && c.crawlRunning !== true && !disabled },
      { id: "crawls.recent", labelKey: "crawls-recent", items: recent.map((path, i) => ({ id: `crawls.recent.${i}`, literalLabel: path, action: `crawls.recent.${i}`, enabled: () => !disabled })) },
    ] }]));
    return () => { for (const off of offs.reverse()) off(); };
  }, [recent, disabled, t, rows]);
  const actions: MenuEntry[] = (Object.keys(allowed) as (keyof typeof allowed)[]).map(action => ({ id: action, label: t(action === "open" ? "crawls-open-selected" : `crawls-${action}`),
    isDisabled: disabled || !allowed[action], disabledReason: chosen.some(r => r.crawl.phase === "running") ? t("crawls-running") : t("crawls-selection-reason"),
    onAction: () => {
      if (!allowed[action] || disabled) return;
      if (action === "delete") { setDeletion([...chosen]); return; }
      if (action === "rename" || action === "move") { setForm({ kind: action, ids: [...ids], value: action === "rename" ? chosen[0]!.crawl.name : "" }); return; }
      void attempt(async () => {
        switch (action) {
          case "compare": await openComparison({ current: ids[0]!, previous: ids[1]!, mapping: [], elements: [] });
            if (useComparison.getState().session) useCrawlsDialog.setState({ open: false }); break;
          case "open": await requestOpen({ kind: "id", crawlId: ids[0]! }); break;
          case "resume": await requestOpen({ kind: "id", crawlId: ids[0]! }, true); break;
          case "export": { const dest = ids.length === 1 ? await desktop.chooseSave(t("crawls-export"), `${chosen[0]!.crawl.name}.ospider`) : await desktop.chooseDirectory(t("crawls-export")); if (dest) startOp(await crawls.export({ ids, dest })); break; }
          case "duplicate": startOp(await crawls.duplicate({ ids })); break;
          case "lock":
          case "unlock":
            try { await crawls[action]({ ids }); }
            finally { await refresh().catch(e => setError({ text: textError(e) })); }
            break;
          case "copyIds": await crawls.copyIds({ ids }); break;
          case "reveal": await crawls.reveal({ ids }); break;
        }
      });
    },
  }));
  const submitForm = () => attempt(async () => {
    if (!form) return;
    const value = form.value.trim();
    if (!value && form.kind !== "move") return;
    setForm(f => f && ({ ...f, error: undefined }));
    if (form.kind === "rename") await crawls.rename({ crawlId: form.ids[0]!, name: value });
    if (form.kind === "move") startOp(await crawls.move({ ids: form.ids, project: value || null }));
    if (form.kind === "projectCreate") { const p = await projects.create({ name: value }); setProject(p.id); }
    if (form.kind === "projectRename") await projects.rename({ id: form.projectId!, name: value });
    setForm(null); await refresh().catch(e => setError({ text: textError(e) }));
  }, text => setForm(f => f && ({ ...f, error: text })));
  const toggle = (id: string, checked: boolean, range = false) => {
    setSelected(old => {
      const next = new Set(old);
      const first = visible.findIndex(r => r.crawl.crawlId === anchor.current), last = visible.findIndex(r => r.crawl.crawlId === id);
      for (const row of range && first >= 0 ? visible.slice(Math.min(first, last), Math.max(first, last) + 1) : visible.filter(r => r.crawl.crawlId === id)) {
        if (checked) { if (next.size < 1000) next.add(row.crawl.crawlId); } else next.delete(row.crawl.crawlId);
      }
      return next;
    });
    if (!range) anchor.current = id;
  };
  const errorBanner = error && <Banner kind={error.kind ?? "error"} onDismiss={() => setError(null)} dismissLabel={t("ui-dialog-close")} action={error.path ? <Button isDisabled={disabled} onPress={() => void attempt(async () => startOp(await desktop.repair(error.path!), true))}>{t("crawls-repair")}</Button> : undefined}>{error.text}</Banner>;
  const result = status?.result as unknown as RepairResult | null;
  const notices = <>
    {errorBanner}
    {active && running && <Banner><ProgressBar label={t(active.repair ? "crawls-repair" : "crawls-progress")} value={status?.total ? status.done : null} max={status?.total || 100} />
      {active.handle.cancellable && <Button isDisabled={cancelling} onPress={() => { setCancelling(true); void op.cancel({ opId: active.handle.opId }).catch(e => { setCancelling(false); setError({ text: textError(e) }); }); }}>{t("ui-dialog-cancel")}</Button>}</Banner>}
    {active && status && status.state !== "running" && <Banner kind={status.state === "failed" ? "error" : "info"} onDismiss={() => setActive(null)} dismissLabel={t("ui-dialog-close")}>
      {status.state === "failed" ? textError(status.error) : t(`crawls-op-${status.state}`)}
      {active.repair && status.state === "done" && result && <div><p>{result.path}</p><p>{t(result.integrityOk && result.usable ? "crawls-repaired" : "crawls-unusable", { lost: result.lostRows })}</p>
        {result.integrityOk && result.usable && <Button onPress={() => void attempt(() => requestOpen({ kind: "path", path: result.path }))}>{t("crawls-open")}</Button>}</div>}
    </Banner>}
  </>;
  return <>
    {!open && (error || active) && <div className="os-crawls-notices">{notices}</div>}
    <Dialog title={t("crawls-title")} isOpen={open} size="large" onOpenChange={value => useCrawlsDialog.setState({ open: value })}>
      <div className="os-crawls" onKeyDown={e => {
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") { e.preventDefault(); searchRef.current?.focus(); }
      }}>
        <div className="os-crawls-toolbar">
          <label>{t("crawls-search")}<input ref={searchRef} type="search" value={search} onChange={e => setSearch(e.target.value)} /></label>
          <label>{t("crawls-show")}<select value={show} onChange={e => setShow(e.target.value)}>{["all", "crawls", "comparisons"].map(v => <option key={v} value={v}>{t(`crawls-show-${v}`)}</option>)}</select></label>
          <Button isDisabled={disabled} onPress={chooseOpen}>{t("crawls-open")}</Button>
          <Button isDisabled={disabled} onPress={importFiles}>{t("crawls-import")}</Button>
          <Button isDisabled={disabled || !useSession.getState().crawl || phase === "running"} onPress={saveAs}>{t("crawls-save")}</Button>
          <Button isDisabled={disabled} onPress={() => void attempt(refresh)}>{t("crawls-refresh")}</Button>
        </div>
        {notices}
        <div className="os-crawls-layout"><aside>
          <Tree label={t("crawls-projects")} selected={project} onSelect={setProject} nodes={[
            { id: "all", label: t("crawls-all-projects"), textValue: t("crawls-all-projects") },
            { id: "none", label: t("crawls-no-project"), textValue: t("crawls-no-project") },
            ...projectList.map(p => ({ id: p.id, label: `${p.name} (${p.crawls})${p.locked ? ` · ${t("crawls-locked")}` : ""}`, textValue: p.name })),
          ]} />
          <Button isDisabled={disabled} onPress={() => setForm({ kind: "projectCreate", ids: [], value: "" })}>{t("crawls-projectCreate")}</Button>
          <Button isDisabled={disabled || !selectedProject || selectedProject.locked} onPress={() => selectedProject && setForm({ kind: "projectRename", projectId: selectedProject.id, ids: [], value: selectedProject.name })}>{t("crawls-projectRename")}</Button>
          <Button isDisabled={disabled || !selectedProject} onPress={() => selectedProject && void attempt(async () => { await projects.lock({ id: selectedProject.id, locked: !selectedProject.locked }); await refresh(); })}>{t(selectedProject?.locked ? "crawls-projectUnlock" : "crawls-projectLock")}</Button>
        </aside>
        <section aria-label={t("crawls-table")} aria-busy={loading}>
          <ContextMenu label={t("crawls-actions")} entries={actions}>
            <div className="os-crawls-scroll" onKeyDown={e => {
              if (e.key === "Delete" && (!(e.target instanceof HTMLInputElement) || e.target.type === "checkbox") && allowed.delete && !disabled) { e.preventDefault(); setDeletion([...chosen]); }
              if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") { e.preventDefault(); setSelected(new Set(visible.slice(0, 1000).map(r => r.crawl.crawlId))); }
            }}>
              <table aria-label={t("crawls-table")}><thead><tr><th><input type="checkbox" aria-label={t("crawls-select-all")} checked={visible.length > 0 && chosen.length === Math.min(visible.length, 1000)} onChange={e => setSelected(e.target.checked ? new Set(visible.slice(0, 1000).map(r => r.crawl.crawlId)) : new Set())} /></th>
                {columns.map(c => <th key={c} aria-sort={sort.column === c ? sort.dir === "asc" ? "ascending" : "descending" : "none"}><button onClick={() => { setSort({ column: c, dir: sort.column === c && sort.dir === "asc" ? "desc" : "asc" }); setPage(0); }}>{t(`crawls-col-${c}`)}</button></th>)}
              </tr></thead><tbody>{visible.slice(page * 100, page * 100 + 100).map(row => <tr key={row.crawl.crawlId} aria-selected={selected.has(row.crawl.crawlId)} onDoubleClick={() => !disabled && void attempt(() => requestOpen({ kind: "id", crawlId: row.crawl.crawlId }))}
                onContextMenu={() => { if (!selected.has(row.crawl.crawlId)) setSelected(new Set([row.crawl.crawlId])); }}>
                <td><input type="checkbox" aria-label={t("crawls-select", { name: row.crawl.name })} checked={selected.has(row.crawl.crawlId)} onChange={e => toggle(row.crawl.crawlId, e.target.checked, (e.nativeEvent as MouseEvent).shiftKey)} /></td>
                {columns.map(c => <td key={c}>{c === "phase" ? t(phaseKey(row)) : c === "locked" ? t(row.locked ? "crawls-yes" : "crawls-no") : String(cell(row, c))}</td>)}
              </tr>)}</tbody></table>
            </div>
          </ContextMenu>
          {!loading && visible.length === 0 && <p>{t("crawls-empty")}</p>}
          <div className="os-crawls-toolbar"><Button isDisabled={page === 0} onPress={() => setPage(p => p - 1)}>{t("crawls-prev")}</Button><span>{t("crawls-count", { count: chosen.length, total: visible.length })}</span><Button isDisabled={(page + 1) * 100 >= visible.length} onPress={() => setPage(p => p + 1)}>{t("crawls-next")}</Button></div>
        </section></div>
        <div className="os-crawls-toolbar">{actions.map(a => <span key={a.id} title={a.isDisabled ? a.disabledReason : undefined}><Button isDisabled={a.isDisabled} onPress={a.onAction}>{a.label}</Button></span>)}</div>
        {chosen.length === 1 && <div className="os-crawls-toolbar"><Button isDisabled={disabled || chosen[0]!.crawl.phase === "running"} onPress={() => void attempt(async () => { const path = chosen[0]!.crawl.file; try { await desktop.check(path); setError({ text: t("crawls-integrity-ok"), kind: "info" }); } catch (e) { const err = toCmdError(e); setError({ text: `${path}: ${textError(e)}`, path: err.messageKey === "err-crawl-corrupt" ? path : undefined }); } })}>{t("crawls-check")}</Button>
          <Button isDisabled={disabled || phase === "running"} onPress={() => void attempt(async () => { await config.set(await desktop.configuration(chosen[0]!.crawl.file)); setError({ text: t("crawls-config-applied"), kind: "info" }); })}>{t("crawls-config")}</Button></div>}
        {recent.length > 0 && <nav aria-label={t("crawls-recent")}><h3>{t("crawls-recent")}</h3>{recent.map(path => <Button key={path} isDisabled={disabled} onPress={() => void attempt(() => requestOpen({ kind: "path", path }))}>{path}</Button>)}</nav>}
        <Button onPress={() => useCrawlsDialog.setState({ open: false })}>{t("ui-dialog-close")}</Button>
      </div>
    </Dialog>
    <Dialog title={t("crawls-repair-result")} isOpen={!!active?.repair && !!status && status.state !== "running"} onOpenChange={value => !value && setActive(null)}>
      {status?.state === "failed" ? <p>{textError(status.error)}</p> : result && <><p>{result.path}</p><p>{t(result.integrityOk && result.usable ? "crawls-repaired" : "crawls-unusable", { lost: result.lostRows })}</p>
        {result.integrityOk && result.usable && <Button onPress={() => void attempt(async () => { await requestOpen({ kind: "path", path: result.path }); setActive(null); })}>{t("crawls-open")}</Button>}</>}
      <Button onPress={() => setActive(null)}>{t("ui-dialog-close")}</Button>
    </Dialog>
    <Dialog title={t("crawls-delete")} isOpen={!!deletion} onOpenChange={v => !v && setDeletion(null)} footer={<DialogFooter okLabel={t("crawls-delete")} cancelLabel={t("ui-dialog-cancel")} okVariant="danger" okDisabled={disabled} onCancel={() => setDeletion(null)} onOk={() => void attempt(async () => {
      if (!deletion || !permissions(deletion).delete) return;
      startOp(await crawls.delete({ ids: deletion.map(r => r.crawl.crawlId) })); setDeletion(null);
    })} />}>
      <p>{t("crawls-delete-confirm", { count: deletion?.length ?? 0, bytes: deletion?.reduce((n, r) => n + r.crawl.fileBytes, 0) ?? 0 })}</p>
    </Dialog>
    <Dialog title={t(`crawls-${form?.kind ?? "rename"}`)} isOpen={!!form} onOpenChange={v => !v && setForm(null)} footer={<DialogFooter okLabel={t("crawls-apply")} cancelLabel={t("ui-dialog-cancel")} okDisabled={disabled || (!form?.value.trim() && form?.kind !== "move")} onOk={submitForm} onCancel={() => setForm(null)} />}>
      {form?.error && <Banner kind="error">{form.error}</Banner>}
      <label>{t(form?.kind === "move" ? "crawls-target-project" : "crawls-col-name")}<input autoFocus disabled={disabled} value={form?.value ?? ""} list={form?.kind === "move" ? "crawls-project-options" : undefined} onChange={e => setForm(f => f && ({ ...f, value: e.target.value, error: undefined }))} onKeyDown={e => { if (e.key === "Enter") { e.preventDefault(); submitForm(); } }} /></label>
      <datalist id="crawls-project-options">{projectList.map(p => <option key={p.id} value={p.name} />)}</datalist>
      {form?.kind === "move" && <p>{t("crawls-move-help")}</p>}
    </Dialog>
    <Dialog title={t("crawls-pause-title")} isOpen={!!pendingOpen} onOpenChange={v => !v && setPendingOpen(null)} footer={<DialogFooter okLabel={t("crawls-pause-open")} cancelLabel={t("ui-dialog-cancel")} okDisabled={disabled} onCancel={() => setPendingOpen(null)} onOk={() => void attempt(async () => { if (!pendingOpen) return; await crawl.pause(); const request = pendingOpen; setPendingOpen(null); await openNow(request); })} />}><p>{t("crawls-pause-help")}</p></Dialog>
  </>;
}

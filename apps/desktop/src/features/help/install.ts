import { currentLanguage, getT } from "../../i18n";
import { hasAction, onActionRegistryChanged, registerAction } from "../../ui/actions";
import { registerMenuContributions } from "../../ui/menu/registry";
import { mainMenuContributions, type MainMenuState } from "../../ui/menu/contributions/main";
import { useSession } from "../../state/session";
import { usePrefs } from "../../state/prefs";
import { crawl } from "../../ipc/crawl";
import { exportApi } from "../../ipc/export";
import { crawls } from "../../ipc/crawls";
import { diag } from "../../ipc/diag";
import { mcp } from "../../ipc/mcp";
import { config } from "../../ipc/config";
import { startAnalysis } from "../sidebar/analysis/start";
import { useMainMode } from "./state";
import { toCmdError } from "../../ipc/errors";
import { showHelp, useHelp } from "./state";
import { client } from "../../ipc/client";
import { chooseFile } from "./files";
import { installMenuContexts } from "./menuContexts";
import { loadRecent, normalizeRecent, onRecentChanged, updateRecent } from "./recent";
import { openFeedback } from "./feedback";

/** Registrierung und Abmeldung sind fenstergebunden; verspätete Antworten ändern kein geschlossenes Fenster. */
export function installMainMenus(): () => void {
  let live = true;
  let modeRevision = 0;
  let recentReady = Promise.resolve();
  let recentRevision = 0;
  const state: MainMenuState = { recent: [], mode: "spider", mcpRunning: false, mcpAutostart: false, available: hasAction };
  let removeMenu = () => {};
  let recentOffs: (() => void)[] = [];
  const offs: (() => void)[] = [];
  const error = (e: unknown) => {
    if (!live) return;
    const err = toCmdError(e);
    useHelp.setState({ message: getT()(err.messageKey, err.args) });
  };
  const rebuild = () => {
    if (!live) return;
    const session = useSession.getState();
    state.mcpRunning = !!session.mcp && session.mcp.kind !== "off";
    state.mcpAutostart = usePrefs.getState().dto?.mcp.autostart ?? false;
    removeMenu();
    removeMenu = registerMenuContributions("u04/main", mainMenuContributions(state));
  };
  const on = (id: string, fn: () => void | Promise<void>, track = true) => {
    const off = registerAction(id, () => { try { void Promise.resolve(fn()).catch(error); } catch (e) { error(e); } });
    if (track) offs.push(off);
    return off;
  };
  const setRecent = (entries: typeof state.recent) => {
    state.recent = normalizeRecent(entries);
    recentOffs.forEach((off) => off());
    recentOffs = state.recent.map((entry, i) => on(`recent.${i}`, () => openPath(entry.path, true), false));
    rebuild();
  };
  let writes = Promise.resolve();
  const persist = (change: Parameters<typeof updateRecent>[0]) => {
    writes = writes.then(() => updateRecent(change)).catch(error);
    return writes;
  };
  const openPath = async (path: string, recent = false) => {
    await recentReady;
    try {
      await crawl.open({ kind: "path", path });
    } catch (e) {
      if (recent && toCmdError(e).code === "NotFound") {
        setRecent(state.recent.filter((entry) => entry.path !== path));
        await persist(entries => entries.filter(entry => entry.path !== path));
        useHelp.setState({ message: getT()("u04-recent-missing", { path }) });
        return;
      }
      throw e;
    }
    if (!live) return;
    const entry = state.recent.find(entry => entry.path === path) ?? { path };
    setRecent([entry, ...state.recent.filter((e) => e.path !== path)]);
    await persist(entries => [entry, ...entries.filter(entry => entry.path !== path)]);
  };
  const file = (request: Parameters<typeof chooseFile>[0]) => chooseFile(request, () => live);
  on("crawl.open", () => file({ kind: "open", extensions: ["ospider"], title: "shortcut-crawl-open", submit: (path) => openPath(path) }));
  for (const id of ["crawl.saveAs", "crawl.export"]) on(id, () => file({ kind: "save", extensions: ["ospider"], defaultPath: "crawl.ospider", title: id === "crawl.saveAs" ? "shortcut-crawl-save-as" : "u04-crawl-export", submit: (dest) => crawl.saveAs({ dest }) }));
  on("crawl.import", () => file({ kind: "open", extensions: ["ospider"], title: "u04-crawl-import", submit: (path) => crawls.import({ paths: [path] }) }));
  for (const page of ["shortcuts", "debug", "usage"] as const) on(`help.${page}`, () => showHelp(page));
  on("app.about", () => showHelp("about"));
  on("help.logs", () => diag.openLogs());
  // URLs aus dem eigenen öffentlichen Repository, keine Referenzressourcen.
  on("help.manual", () => { if (client.transportName === "tauri") return client.invoke<void>("open_manual");
    window.open("https://github.com/wojcikdominik/openspider/tree/main/docs", "_blank", "noopener,noreferrer"); });
  // UI-130: Beta-Rückmeldung auf der eigenen Website, Version und Oberflächensprache in der URL.
  on("help.feedback", () => openFeedback(currentLanguage()));
  for (const mode of ["spider", "list", "serp", "apis", "compare"] as const) on(`mode.${mode}`, () => {
    useMainMode.setState({ mode });
    state.mode = mode;
    rebuild();
  });
  on("analysis.start", async () => { const operation = await startAnalysis({}); if (live) useHelp.setState({ operation }); });
  on("mcp.start", async () => { const status = await mcp.start({}); if (live) useSession.setState({ mcp: status }); });
  on("mcp.stop", async () => { const status = await mcp.stop(); if (live) useSession.setState({ mcp: status }); });
  on("mcp.autostart", async () => { await mcp.setAutostart({ value: !state.mcpAutostart }); await usePrefs.getState().load(); });
  on("mcp.markdown", () => file({ kind: "save", extensions: ["md"], defaultPath: "openspider-api.md", title: "u04-mcp-markdown", submit: (path) => mcp.saveToolsMarkdown({ path }) }));

  let catalogVersion = 0;
  const refreshCatalog = async () => {
    const version = ++catalogVersion;
    try {
      const catalog = await exportApi.catalog();
      if (!live || version !== catalogVersion) return;
      state.catalog = catalog;
      rebuild();
    } catch {
      // Andere Welle-1-Pakete füllen die Katalog-Commands. Das Menü erklärt den Leerzustand.
      if (live && version === catalogVersion) { state.catalog = undefined; rebuild(); }
    }
  };
  const reloadRecent = async () => {
    const version = ++recentRevision;
    const entries = await loadRecent();
    if (live && version === recentRevision) setRecent(entries);
  };
  offs.push(useSession.subscribe((next, prev) => {
    if (next.crawl?.crawlId !== prev.crawl?.crawlId || next.crawl?.phase !== prev.crawl?.phase) void reloadRecent().catch(error);
    if (next.crawl?.mode && next.crawl?.mode !== prev.crawl?.mode) state.mode = next.crawl.mode;
    if (next.crawl?.mode && next.crawl?.mode !== prev.crawl?.mode) useMainMode.setState({ mode: next.crawl.mode });
    if (next.crawl?.phase !== prev.crawl?.phase || next.crawl?.crawlId !== prev.crawl?.crawlId || next.mcp !== prev.mcp || next.analysis?.running !== prev.analysis?.running) rebuild();
    if (next.crawl?.phase !== prev.crawl?.phase || next.crawl?.crawlId !== prev.crawl?.crawlId || next.analysis !== prev.analysis) void refreshCatalog();
  }));
  offs.push(onRecentChanged(() => { void reloadRecent().catch(error); }));
  offs.push(usePrefs.subscribe(rebuild));
  offs.push(onActionRegistryChanged(rebuild));
  offs.push(useMainMode.subscribe((next) => { modeRevision++; state.mode = next.mode; rebuild(); }));
  offs.push(installMenuContexts(rebuild));
  rebuild();
  recentReady = reloadRecent().catch(error);
  void refreshCatalog();
  const initialModeRevision = modeRevision;
  void config.get().then((dto) => {
    if (live && modeRevision === initialModeRevision && dto.config && typeof dto.config === "object" && !Array.isArray(dto.config) && typeof dto.config.mode === "string") { if (["spider", "list"].includes(dto.config.mode)) useMainMode.setState({ mode: dto.config.mode as "spider" | "list" }); }
  }).catch(() => undefined);
  void mcp.status().then((status) => { if (live) useSession.setState({ mcp: status }); }).catch(() => undefined);
  return () => {
    live = false;
    removeMenu();
    recentOffs.forEach((off) => off());
    offs.reverse().forEach((off) => off());
    useHelp.setState({ page: null, path: null, message: null, operation: null });
  };
}

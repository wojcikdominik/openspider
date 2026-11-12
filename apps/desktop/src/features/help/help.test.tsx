import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createClient, setClient } from "../../ipc/client";
import { MockCore } from "../../ipc/transport/mock";
import { registerAction, runAction, hasAction } from "../../ui/actions";
import { allMenuContributions, registerMenuContributions } from "../../ui/menu/registry";
import { buildMenuModel, flattenItems } from "../../ui/menu/build";
import { makeT, setLanguage } from "../../i18n";
import { SHORTCUTS, shortcutTable } from "../../ui/shortcuts";
import { usePrefs } from "../../state/prefs";
import { useSession } from "../../state/session";
import { IpcError, localError } from "../../ipc/errors";
import { debugFixture, catalogFixture } from "./__fixtures__/fx-ui-basic";
import { HelpDialogs, ShortcutHelp, DebugHelp } from "./HelpDialogs";
import { installMainMenus } from "./install";
import { loadRecent, normalizeRecent } from "./recent";
import { useHelp } from "./state";
import { feedbackUrl } from "./feedback";
let core: MockCore;
let dispose: (() => void) | undefined;
beforeEach(() => {
  core = new MockCore();
  core.handlers.set("app.info", () => debugFixture.app);
  core.handlers.set("export.catalog", () => catalogFixture);
  core.handlers.set("mcp.status", () => ({ kind: "off" }));
  core.handlers.set("diag.debugInfo", () => debugFixture);
  core.handlers.set("diag.previewBundle", () => ({ files: ["logs/openspider.log", "system.txt", "config-diff.json", "tasks.json"], warnings: ["Redacted"] }));
  core.handlers.set("diag.bundle", () => ({ opId: "bundle-fixture" }));
  core.handlers.set("crawl.open", () => ({}));
  core.handlers.set("diag.usageStats", () => ({ crawls: 0, urls: 0, bytes: 0, perMonth: [] }));
  setClient(createClient(core.transport()));
  useHelp.setState({ page: null, path: null, message: null, operation: null });
  useSession.setState({ crawl: null, mcp: null, ops: {} });
});
afterEach(async () => { cleanup(); dispose?.(); dispose = undefined; setClient(null); vi.restoreAllMocks(); await setLanguage("en"); });

// spec: UI-113 [P1]
it("Hilfe-Dialog enthält alle Register-Einträge und Belegungen auf beiden Plattformen", async () => {
  for (const platform of ["mac", "win"] as const) for (const lang of ["de", "en"]) {
    await setLanguage(lang);
    const { container, unmount } = render(<ShortcutHelp platform={platform} />);
    const rows = container.querySelectorAll("[data-shortcut]");
    expect([...rows].map((row) => row.getAttribute("data-shortcut"))).toEqual(SHORTCUTS.map((s) => s.id));
    for (const [i, row] of shortcutTable(platform, lang).entries()) expect([...rows[i]!.querySelectorAll("kbd")].map((kbd) => kbd.textContent)).toEqual(row.keys);
    unmount();
  }
});

// spec: OUT-227 [M2]
it("Debug zeigt alle effektiven Pfade aus OPENSPIDER_HOME, keine konstruierten OS-Pfade", async () => {
  render(<DebugHelp />);
  for (const path of Object.values(debugFixture.app.paths)) if (path) expect(await screen.findByText(path)).toBeVisible();
  expect(screen.getByText("macOS fixture")).toBeVisible();
  expect(screen.getByText("Chromium")).toBeVisible();
  expect(screen.getByText("CPU cores")).toBeVisible();
});

// spec: OUT-241 [P1]
it("Log-Level wirkt beim Auswählen sofort; Aufbewahrung wird separat gespeichert", async () => {
  render(<DebugHelp />);
  const level = await screen.findByRole("combobox", { name: "Log level" });
  await waitFor(() => expect(level).toBeEnabled());
  fireEvent.change(level, { target: { value: "trace" } });
  await waitFor(() => expect(core.settings.logging.level).toBe("trace"));
  expect(core.settings.logging.retentionDays).toBe(30);
  await waitFor(() => expect(level).toBeEnabled());
  fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "14" } });
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(core.settings.logging.retentionDays).toBe(14));
  expect(core.settings.logging.level).toBe("trace");
});

// spec: OUT-244 [P1]
it("Diagnoseexport erst nach sichtbarer Dateivorschau und Bestätigung mit ZIP-Ziel", async () => {
  render(<DebugHelp />);
  await screen.findByText("macOS fixture");
  fireEvent.click(screen.getByRole("button", { name: "Save logs…" }));
  expect(await screen.findByText("system.txt")).toBeVisible();
  expect(screen.getByText("logs/openspider.log")).toBeVisible();
  expect(core.calls.filter((c) => c.cmd === "diag.bundle")).toHaveLength(0);
  fireEvent.change(screen.getByRole("textbox", { name: "Destination ZIP file" }), { target: { value: "/fixture/support.zip" } });
  fireEvent.click(screen.getByRole("button", { name: "Save these files" }));
  await waitFor(() => expect(core.calls.find((c) => c.cmd === "diag.bundle")?.args).toEqual({ dest: "/fixture/support.zip" }));
  expect(useHelp.getState().operation?.opId).toBe("bundle-fixture");
});

// spec: OUT-244 [P1]
it("Fehlgeschlagene Vorschau bietet keinen Diagnoseexport an", async () => {
  core.handlers.set("diag.previewBundle", () => { throw new IpcError(localError("Io", "err-code-io")); });
  render(<DebugHelp />);
  await screen.findByText("macOS fixture");
  fireEvent.click(screen.getByRole("button", { name: "Save logs…" }));
  await screen.findByRole("alert");
  expect(screen.queryByRole("button", { name: "Save these files" })).toBeNull();
  expect(core.calls.filter((c) => c.cmd === "diag.bundle")).toHaveLength(0);
});

// spec: CRAWL-127 [M2]
it("Öffnen aktualisiert zehn MRU-Einträge; fehlende Datei wird mit Hinweis entfernt", async () => {
  core.uiState = { version: 1, state: { foreign: { keep: true }, recentCrawls: Array.from({ length: 12 }, (_, i) => ({ path: `/fixture/${i}.ospider` })) } };
  dispose = installMainMenus();
  await waitFor(() => expect(hasAction("recent.9")).toBe(true));
  runAction("recent.3", { source: "menu" });
  await waitFor(async () => expect((await loadRecent())[0]?.path).toBe("/fixture/3.ospider"));
  expect((await loadRecent())).toHaveLength(10);
  expect((core.uiState.state as { foreign: unknown }).foreign).toEqual({ keep: true });
  core.handlers.set("crawl.open", () => { throw new IpcError(localError("NotFound", "err-code-notfound")); });
  runAction("recent.0", { source: "menu" });
  await waitFor(() => expect(useHelp.getState().message).toContain("/fixture/3.ospider"));
  expect((await loadRecent()).map((entry) => entry.path)).not.toContain("/fixture/3.ospider");
  expect(normalizeRecent([{ path: "x" }, { path: "x" }, {}, null])).toEqual([{ path: "x" }]);
});

// spec: CRAWL-127 [M2]
it("Speichern unter und Crawl-Export erzeugen Kopien über crawl.saveAs", async () => {
  core.handlers.set("crawl.saveAs", () => ({ opId: "copy" }));
  dispose = installMainMenus();
  for (const id of ["crawl.saveAs", "crawl.export"]) {
    runAction(id, { source: "menu" });
    await useHelp.getState().path?.submit("/fixture/copy.ospider");
    expect(core.calls.at(-1)).toMatchObject({ cmd: "crawl.saveAs", args: { dest: "/fixture/copy.ospider" } });
  }
});

// spec: UI-002
it("Hilfe-Aktionen öffnen echte Dialoge; leere Nutzungsstatistik zeigt keinen Fehler", async () => {
  dispose = installMainMenus();
  render(<HelpDialogs />);
  act(() => { runAction("help.usage", { source: "menu" }); });
  expect(await screen.findByText("No usage recorded yet")).toBeVisible();
  act(() => { runAction("app.about", { source: "menu" }); });
  expect(await screen.findByText("Apache 2.0 license")).toBeVisible();
  expect(screen.getByText("Third-party notices")).toBeVisible();
  const version = await waitFor(() => { const node = document.querySelector('[data-about="version"]'); expect(node).not.toBeNull(); return node!; });
  expect(version).toHaveTextContent(/Version 0\.1\.0 · Commit fixture/);
});

// spec: UI-002
it("Über OpenSpider nennt den Entwickler und öffnet Website und Quellcode im Standardbrowser", async () => {
  dispose = installMainMenus();
  const open = vi.spyOn(window, "open").mockImplementation(() => null);
  try {
    render(<HelpDialogs />);
    act(() => { runAction("app.about", { source: "menu" }); });
    expect(await screen.findByText("Developed by")).toBeVisible();
    expect(document.querySelector('[data-about="developer"]')).toHaveTextContent("Dominik Wojcik");
    expect(screen.getByText("© 2026 Dominik Wojcik and the OpenSpider contributors")).toBeVisible();
    const website = screen.getByRole("link", { name: "openspider.de" });
    expect(website).toHaveAttribute("href", "https://openspider.de/");
    fireEvent.click(website);
    await waitFor(() => expect(open).toHaveBeenCalledWith("https://openspider.de/", "_blank", "noopener,noreferrer"));
    const source = screen.getByRole("link", { name: "github.com/wojcikdominik/openspider" });
    expect(source).toHaveAttribute("href", "https://github.com/wojcikdominik/openspider");
    fireEvent.click(source);
    await waitFor(() => expect(open).toHaveBeenCalledWith("https://github.com/wojcikdominik/openspider", "_blank", "noopener,noreferrer"));
  } finally {
    open.mockRestore();
  }
});

// spec: UI-002
it("Über OpenSpider öffnet die Links in der App über browser_open", async () => {
  setClient(createClient({ ...core.transport(), name: "tauri" }));
  core.handlers.set("browser_open", () => undefined);
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  dispose = installMainMenus();
  render(<HelpDialogs />);
  act(() => { runAction("app.about", { source: "menu" }); });
  for (const [name, url] of [["openspider.de", "https://openspider.de/"], ["github.com/wojcikdominik/openspider", "https://github.com/wojcikdominik/openspider"]] as const) {
    core.calls.length = 0;
    fireEvent.click(await screen.findByRole("link", { name }));
    await waitFor(() => expect(core.calls.find(call => call.cmd === "browser_open")?.args).toEqual({ urls: [url] }));
  }
  expect(open).not.toHaveBeenCalled();
  open.mockRestore();
});

// spec: UI-002
it("Katalogeinträge warten auf U16 und delegieren an dessen Aktions-IDs", async () => {
  dispose = installMainMenus();
  const id = "export.bulk.Pages With Uncrawlable Internal Outlinks";
  const entry = () => flattenItems(buildMenuModel(allMenuContributions(), { platform: "win", lang: "en", t: makeT("en"), ctx: { hasCrawl: true }, native: false }).menus.flatMap(menu => menu.items)).find(item => item.action === id);
  await waitFor(() => expect(entry()).toBeDefined());
  expect(entry()?.enabled).toBe(false);
  expect(hasAction(id)).toBe(false);
  const handler = vi.fn();
  const off = registerAction(id, handler);
  try {
    expect(entry()?.enabled).toBe(true);
    runAction(entry()!.action!, { source: "menu" });
    expect(handler).toHaveBeenCalledOnce();
    expect(useHelp.getState().path).toBeNull();
    expect(core.calls.filter(call => ["export.bulk", "export.report", "export.sitemap"].includes(call.cmd))).toEqual([]);
  } finally { off(); }
  expect(entry()?.enabled).toBe(false);
});

// spec: UI-002
it("MCP-Zustand wird bei Session- und Einstellungsänderungen neu markiert", async () => {
  await usePrefs.getState().load();
  dispose = installMainMenus();
  await waitFor(() => expect(useSession.getState().mcp?.kind).toBe("off"));
  useSession.setState({ mcp: { kind: "ready", port: 9000 } });
  const model = buildMenuModel(allMenuContributions(), { platform: "win", lang: "en", t: makeT("en"), ctx: {}, native: false });
  const all = flattenItems(model.menus.flatMap((m) => m.items));
  expect(all.find((i) => i.id === "mcp.start")?.enabled).toBe(false);
  expect(all.find((i) => i.id === "mcp.stop")?.enabled).toBe(true);
});

// spec: UI-002
it("Später registrierte Fachaktionen aktivieren ihren Menüeintrag und Abmeldung deaktiviert ihn", async () => {
  dispose = installMainMenus();
  const entry = () => flattenItems(buildMenuModel(allMenuContributions(), { platform: "win", lang: "en", t: makeT("en"), ctx: {}, native: false }).menus.flatMap((m) => m.items)).find((i) => i.id === "config.spider");
  expect(entry()?.enabled).toBe(false);
  expect(entry()?.disabledReason).toBeTruthy();
  const off = registerAction("config.spider", () => {});
  expect(entry()?.enabled).toBe(true);
  off();
  expect(entry()?.enabled).toBe(false);
});

// spec: UI-002
it("Alle fünf UI-Modi werden markiert, ohne unbekannte Modi an CrawlConfig zu senden", async () => {
  dispose = installMainMenus();
  for (const mode of ["spider", "list", "serp", "apis", "compare"]) {
    runAction(`mode.${mode}`, { source: "menu" });
    const menu = buildMenuModel(allMenuContributions(), { platform: "win", lang: "en", t: makeT("en"), ctx: {}, native: false }).menus.find((m) => m.id === "mode");
    expect(menu?.items.filter((i) => i.checked).map((i) => i.action)).toEqual([`mode.${mode}`]);
  }
  expect(core.calls.filter((c) => c.cmd === "config.set")).toHaveLength(0);
});

// spec: CRAWL-127 [M2]
it("Vorübergehender Lesefehler entfernt keinen Verlaufseintrag", async () => {
  core.uiState = { version: 1, state: { recentCrawls: [{ path: "/fixture/keep.ospider" }] } };
  core.handlers.set("crawl.open", () => { throw new IpcError(localError("Permission", "err-code-permission")); });
  dispose = installMainMenus();
  await waitFor(() => expect(hasAction("recent.0")).toBe(true));
  runAction("recent.0");
  await waitFor(() => expect(useHelp.getState().message).toBeTruthy());
  expect(await loadRecent()).toEqual([{ path: "/fixture/keep.ospider" }]);
});

// spec: CRAWL-127 [M2]
it("Öffnen während des Starts wartet auf den Verlauf und erhält ältere Einträge", async () => {
  let finishLoad!: (value: unknown) => void;
  core.handlers.set("uistate.load", () => new Promise((resolve) => { finishLoad = resolve; }));
  dispose = installMainMenus();
  runAction("crawl.open");
  const opening = useHelp.getState().path!.submit("/fixture/new.ospider");
  expect(core.calls.some((c) => c.cmd === "crawl.open")).toBe(false);
  core.uiState = { version: 1, state: { recentCrawls: [{ path: "/fixture/older.ospider" }] } };
  core.handlers.delete("uistate.load");
  finishLoad(core.uiState);
  await opening;
  expect(await loadRecent()).toEqual([{ path: "/fixture/new.ospider" }, { path: "/fixture/older.ospider" }]);
});

// spec: UI-002
it("Verspätetes Laden der Konfiguration überschreibt keine inzwischen gewählte Modusmarkierung", async () => {
  let finishConfig!: (value: unknown) => void;
  core.handlers.set("config.get", () => new Promise((resolve) => { finishConfig = resolve; }));
  dispose = installMainMenus();
  runAction("mode.serp");
  finishConfig({ schemaVersion: 2, generator: "fixture", config: { mode: "spider" } });
  await Promise.resolve(); await Promise.resolve();
  const menu = buildMenuModel(allMenuContributions(), { platform: "win", lang: "en", t: makeT("en"), ctx: {}, native: false }).menus.find((m) => m.id === "mode");
  expect(menu?.items.find((item) => item.id === "mode.serp")?.checked).toBe(true);
});

// spec: OUT-241 [P1]
it("Log-Level bleibt auch bei fehlgeschlagenen Systeminformationen bedienbar", async () => {
  core.handlers.set("diag.debugInfo", () => { throw new IpcError(localError("Io", "err-code-io")); });
  render(<DebugHelp />);
  const level = screen.getByRole("combobox", { name: "Log level" });
  await waitFor(() => expect(level).toBeEnabled());
  expect(screen.getByRole("alert")).toBeVisible();
  fireEvent.change(level, { target: { value: "warn" } });
  await waitFor(() => expect(core.settings.logging.level).toBe("warn"));
});

// spec: CRAWL-127 [M2], UI-002
it("Native Dateiaktionen verwenden Open/Save mit Filtern, Abbruch führt keine Aktion aus", async () => {
  setClient(createClient({ ...core.transport(), name: "tauri" }));
  core.handlers.set("dialog.open", () => null);
  core.handlers.set("dialog.save", () => "/fixture/copy.ospider");
  core.handlers.set("crawl.saveAs", () => ({ opId: "native-copy" }));
  core.handlers.set("crawls.import", () => ({ opId: "native-import" }));
  core.handlers.set("mcp.saveToolsMarkdown", () => undefined);
  dispose = installMainMenus();
  runAction("crawl.open", { source: "menu" });
  await waitFor(() => expect(core.calls.some(call => call.cmd === "dialog.open")).toBe(true));
  expect(core.calls.some(call => call.cmd === "crawl.open")).toBe(false);
  expect(useHelp.getState().path).toBeNull();
  core.handlers.set("dialog.open", () => "/fixture/basic.ospider");
  runAction("crawl.open", { source: "menu" });
  await waitFor(() => expect(core.calls.find(call => call.cmd === "crawl.open")?.args).toEqual({ kind: "path", path: "/fixture/basic.ospider" }));
  for (const id of ["crawl.saveAs", "crawl.export", "crawl.import", "mcp.markdown"]) {
    runAction(id, { source: "menu" });
    const cmd = id === "crawl.import" ? "crawls.import" : id === "mcp.markdown" ? "mcp.saveToolsMarkdown" : "crawl.saveAs";
    await waitFor(() => expect(core.calls.some(call => call.cmd === cmd)).toBe(true));
  }
  const dialogs = core.calls.filter(call => call.cmd.startsWith("dialog."));
  expect(dialogs).toHaveLength(6);
  for (const call of dialogs) {
    expect(call.args).toMatchObject({ options: { filters: [{ extensions: [call === dialogs.at(-1) ? "md" : "ospider"] }] } });
    if (call.cmd === "dialog.open") expect(call.args).toMatchObject({ options: { directory: false, multiple: false } });
  }
  expect(core.calls.find(call => call.cmd === "crawls.import")?.args).toEqual({ paths: ["/fixture/basic.ospider"] });
  expect(useHelp.getState().operation?.opId).toBe("native-import");
  expect(useHelp.getState().path).toBeNull();
});

// spec: CRAWL-127 [M2]
it("Fehler des nativen Dateidialogs zeigt eine Meldung und keinen Pfad-Fallback", async () => {
  setClient(createClient({ ...core.transport(), name: "tauri" }));
  core.handlers.set("dialog.open", () => { throw new IpcError(localError("Permission", "err-code-permission")); });
  dispose = installMainMenus();
  runAction("crawl.open", { source: "menu" });
  await waitFor(() => expect(useHelp.getState().message).toBeTruthy());
  expect(useHelp.getState().path).toBeNull();
  expect(core.calls.some(call => call.cmd === "crawl.open")).toBe(false);
});

// spec: UI-002
it("Das Handbuch verwendet in Tauri den nativen Command", async () => {
  setClient(createClient({ ...core.transport(), name: "tauri" }));
  core.handlers.set("open_manual", () => ({}));
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  dispose = installMainMenus();
  runAction("help.manual", { source: "menu" });
  await waitFor(() => expect(core.calls.some(call => call.cmd === "open_manual")).toBe(true));
  expect(open).not.toHaveBeenCalled();
});

// spec: UI-130
it("Feedback-URL enthält App-Version, Sprache und Sprungmarke; Sonderzeichen bleiben im Query-Wert", () => {
  expect(feedbackUrl("de", "1.0.0-beta.2")).toBe("https://openspider.de/beta/?version=1.0.0-beta.2#feedback");
  expect(feedbackUrl("en", "1.0.0-beta.2")).toBe("https://openspider.de/en/beta/?version=1.0.0-beta.2#feedback");
  expect(feedbackUrl("fr", " 1.0 &x=#y ")).toBe("https://openspider.de/en/beta/?version=1.0+%26x%3D%23y#feedback");
  expect(feedbackUrl("de", null)).toBe("https://openspider.de/beta/#feedback");
});

// spec: UI-130
it("Hilfe → Feedback geben öffnet die Beta-Seite der Oberflächensprache mit der App-Version im Standardbrowser", async () => {
  setClient(createClient({ ...core.transport(), name: "tauri" }));
  core.handlers.set("browser_open", () => undefined);
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  for (const [lang, label, url] of [
    ["de", "Feedback geben…", "https://openspider.de/beta/?version=0.1.0#feedback"],
    ["en", "Send feedback…", "https://openspider.de/en/beta/?version=0.1.0#feedback"],
  ] as const) {
    await setLanguage(lang);
    dispose = installMainMenus();
    const help = buildMenuModel(allMenuContributions(), { platform: "mac", lang, t: makeT(lang), ctx: {}, native: false }).menus.find(menu => menu.id === "help")!;
    expect(help.items.find(item => item.id === "help.feedback")).toMatchObject({ label: label, enabled: true });
    core.calls.length = 0;
    runAction("help.feedback", { source: "menu" });
    await waitFor(() => expect(core.calls.find(call => call.cmd === "browser_open")?.args).toEqual({ urls: [url] }));
    dispose(); dispose = undefined;
  }
  expect(open).not.toHaveBeenCalled();
});

// spec: OUT-244 [P1]
it("Native Diagnoseauswahl öffnet nach der Vorschau den ZIP-Speichern-Dialog", async () => {
  setClient(createClient({ ...core.transport(), name: "tauri" }));
  core.handlers.set("dialog.save", () => null);
  render(<DebugHelp />);
  await screen.findByText("macOS fixture");
  fireEvent.click(screen.getByRole("button", { name: "Save logs…" }));
  await screen.findByText("system.txt");
  expect(core.calls.some(call => call.cmd === "dialog.save")).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Save these files" }));
  await waitFor(() => expect(core.calls.some(call => call.cmd === "dialog.save")).toBe(true));
  await waitFor(() => expect(screen.getByRole("button", { name: "Save these files" })).toBeEnabled());
  expect(core.calls.some(call => call.cmd === "diag.bundle")).toBe(false);
  core.handlers.set("dialog.save", () => "/fixture/support.zip");
  fireEvent.click(screen.getByRole("button", { name: "Save these files" }));
  await waitFor(() => expect(core.calls.find(call => call.cmd === "diag.bundle")?.args).toEqual({ dest: "/fixture/support.zip" }));
  expect(core.calls.find(call => call.cmd === "dialog.save")?.args).toMatchObject({ options: { filters: [{ extensions: ["zip"] }] } });
});

// spec: UI-113 [P1]
it("Native Menüaktionen beachten Textfeld- und Dialog-Scopes auch vor dem Menü-Neuaufbau", async () => {
  dispose = installMainMenus();
  const handler = vi.fn();
  const off = registerAction("crawl.new", handler);
  const field = document.createElement("input");
  const modal = document.createElement("div");
  document.body.append(field, modal);
  const entry = () => flattenItems(buildMenuModel(allMenuContributions(), { platform: "mac", lang: "en", t: makeT("en"), ctx: {}, native: true }).menus.flatMap(menu => menu.items)).find(item => item.action === "crawl.new");
  try {
    expect(entry()?.accelerator).toBe("CmdOrCtrl+N");
    expect(entry()?.enabled).toBe(true);
    field.focus();
    expect(entry()?.enabled).toBe(false);
    await waitFor(() => expect(entry()?.disabledReason).toMatch(/Unavailable while/));
    expect(runAction("crawl.new", { source: "menu" })).toBe(false);
    expect(handler).not.toHaveBeenCalled();
    field.blur();
    expect(runAction("crawl.new", { source: "menu" })).toBe(true);
    expect(handler).toHaveBeenCalledOnce();
    modal.setAttribute("aria-modal", "true");
    expect(entry()?.enabled).toBe(false);
    await waitFor(() => expect(entry()?.disabledReason).toMatch(/Unavailable while/));
    expect(runAction("crawl.new", { source: "menu" })).toBe(false);
    expect(handler).toHaveBeenCalledOnce();
    modal.removeAttribute("aria-modal");
    expect(entry()?.enabled).toBe(true);
  } finally { off(); field.remove(); modal.remove(); }
});

// spec: UI-018
// spec: UI-145
it("Menümodelle folgen Fokuswechseln außerhalb des Eingabeereignisses und nicht jedem Live-Tick", async () => {
  dispose = installMainMenus();
  const enabled = vi.fn(() => true);
  const off = registerMenuContributions("n24/probe", [{ menu: "file", group: "n24", order: 99, items: [{ id: "n24.probe", action: "n24.probe", labelKey: "n24-probe", enabled }] }]);
  const { MenuAvailability } = await import("./MenuAvailability");
  const field = document.createElement("input");
  document.body.append(field);
  try {
    render(<MenuAvailability />);
    const crawl = { crawlId: "fixture", mode: "spider" as const, phase: "running" as const, crawled: 10, queued: 90, discovered: 100, percent: 10, rate: 10, avgRate: 10,
      elapsedS: 1, pausedS: 0, activeThreads: 4, owner: { kind: "app" as const, live: true, controllable: true, pid: 1 } };
    act(() => useSession.setState({ crawl }));
    const built = enabled.mock.calls.length;
    // Progress ticks of a running crawl do not rebuild the status-line menu model.
    for (let i = 1; i <= 5; i++) act(() => useSession.setState({ crawl: { ...crawl, crawled: 10 + i, percent: 10 + i } }));
    expect(enabled.mock.calls.length).toBe(built);
    // A focus change (e.g. a click into another region) rebuilds the menus one task later, not inside the event.
    act(() => field.focus());
    expect(enabled.mock.calls.length).toBe(built);
    await waitFor(() => expect(enabled.mock.calls.length).toBeGreaterThan(built));
  } finally { off(); field.remove(); }
});

// spec: UI-002
it("Statuszeile zeigt Deaktivierungsgründe für nicht verfügbare Aktionen", async () => {
  dispose = installMainMenus();
  const { MenuAvailability } = await import("./MenuAvailability");
  render(<MenuAvailability />);
  expect(screen.getByText(/Unavailable menu actions/)).toBeVisible();
  fireEvent.click(screen.getByText(/Unavailable menu actions/));
  expect(screen.queryByText("Check for updates: Available from M3.")).not.toBeInTheDocument();
  // Without a registered action no crawl state helps: the reason names the missing feature, not the crawl.
  const entry = screen.getByText("Spider configuration: This feature is not available yet.");
  expect(entry).toBeVisible();
  expect(screen.queryByText("Spider configuration: This action requires an idle crawl with data or an available service.")).toBeNull();
  let off!: () => void;
  act(() => { off = registerAction("config.spider", () => {}); });
  expect(screen.queryByText("Spider configuration: This feature is not available yet.")).toBeNull();
  act(() => off());
  expect(screen.getByText("Spider configuration: This feature is not available yet.")).toBeVisible();
});

// spec: UI-002
it("Statuszeile nennt den Eintrag je Zeile nur einmal und den Crawl-Grund nur bei registrierter Aktion", async () => {
  dispose = installMainMenus();
  const offAction = registerAction("n29.probe", () => {});
  const off = registerMenuContributions("n29/probe", [{ menu: "file", group: "n29", order: 99, items: [
    { id: "n29.named", label: "Probe report", enabled: () => false, disabledReasonKey: "Probe report: needs rendering" },
    { id: "n29.probe", action: "n29.probe", label: "Probe action", enabled: () => false, disabledReasonKey: "u04-reason-unavailable" },
  ] }]);
  const { MenuAvailability } = await import("./MenuAvailability");
  try {
    render(<MenuAvailability />);
    fireEvent.click(screen.getByText(/Unavailable menu actions/));
    expect(await screen.findByText("Probe report: needs rendering")).toBeVisible();
    expect(screen.queryByText(/Probe report: Probe report/)).toBeNull();
    expect(screen.getByText("Probe action: This action requires an idle crawl with data or an available service.")).toBeVisible();
  } finally { off(); offAction(); }
});

// spec: CRAWL-127 [M2]
it("Ein nach Fensterende bestätigter nativer Dialog startet keinen Crawl", async () => {
  setClient(createClient({ ...core.transport(), name: "tauri" }));
  let selected!: (path: string) => void;
  core.handlers.set("dialog.open", () => new Promise(resolve => { selected = resolve; }));
  dispose = installMainMenus();
  runAction("crawl.open", { source: "menu" });
  dispose(); dispose = undefined;
  selected("/fixture/basic.ospider");
  await Promise.resolve(); await Promise.resolve();
  expect(core.calls.some(call => call.cmd === "crawl.open")).toBe(false);
});

// spec: CRAWL-127
it("rebuilds the File → Recently opened labels after a language switch", async () => {
  core.uiState = { version: 1, state: { recentCrawls: [{ path: "/fixture/uuid.ospider" }] } };
  core.handlers.set("crawls.list", () => [{ crawl: { file: "/fixture/uuid.ospider", startedAt: "2026-10-29T12:00:00", name: "uuid" }, startUrl: "http://127.0.0.1:1234/fixture" }]);
  await setLanguage("en");
  dispose = installMainMenus();
  const label = () => flattenItems(buildMenuModel(allMenuContributions(), { platform: "mac", lang: "en", native: true, t: key => key, ctx: {} }).menus.flatMap(menu => menu.items)).find(item => item.id === "recent.0")?.label;
  await waitFor(() => expect(label()).toMatch(/^uuid – Oct 29, 2026, 12:00\sPM$/));
  await act(async () => { await setLanguage("de"); });
  await waitFor(() => expect(label()).toBe("uuid – 29.10.2026, 12:00"));
});

// spec: UI-124
it("enriches recent fixture paths from the crawl index without changing opening addresses", async () => {
  core.uiState = { version: 1, state: { recentCrawls: [{ path: "/fixture/uuid.ospider" }] } };
  // CRAWL-127 (OS-Festlegung 09.11.2026): crawl name (else host of the start URL) – date and time of the crawl.
  core.handlers.set("crawls.list", () => [{ crawl: { file: "/fixture/uuid.ospider", startedAt: "2026-10-29T12:00:00", name: "uuid" }, startUrl: "http://127.0.0.1:1234/fixture" }]);
  expect(await loadRecent()).toEqual([{ path: "/fixture/uuid.ospider", label: expect.stringMatching(/^uuid – Oct 29, 2026, 12:00\sPM$/) }]);
  core.handlers.set("crawls.list", () => [{ crawl: { file: "/fixture/uuid.ospider", startedAt: "2026-10-29T12:00:00", name: "" }, startUrl: "http://127.0.0.1:1234/fixture" }]);
  expect(await loadRecent()).toEqual([{ path: "/fixture/uuid.ospider", label: expect.stringMatching(/^127\.0\.0\.1:1234 – Oct 29, 2026/) }]);
});

// spec: UI-127
it("version with build number reads „1.0.0-beta.3 (Build 2578)“, without number only the version (Nutzerentscheidung 09.11.2026)", async () => {
  const { versionLabel } = await import("./version");
  const { makeT } = await import("../../i18n");
  for (const lang of ["de", "en"]) {
    expect(versionLabel(makeT(lang), { version: "1.0.0-beta.3", buildNumber: "2578" })).toBe("1.0.0-beta.3 (Build 2578)");
    expect(versionLabel(makeT(lang), { version: "1.0.0-beta.3", buildNumber: null })).toBe("1.0.0-beta.3");
    expect(versionLabel(makeT(lang), { version: "1.0.0-beta.3" })).toBe("1.0.0-beta.3");
  }
});

// spec: UI-127
it("Debug-Info: version row with build number, the commit hash row is labelled „Commit“, not „Build“ (de/en)", async () => {
  core.handlers.set("diag.debugInfo", () => ({ ...debugFixture, app: { ...debugFixture.app, buildNumber: "2578" } }));
  for (const [lang, versionLabel] of [["en", "Version"], ["de", "Version"]] as const) {
    await setLanguage(lang);
    const { unmount } = render(<DebugHelp />);
    const row = (label: string) => screen.getByText(label, { selector: "dt" }).parentElement!.querySelector("dd")!;
    expect((await screen.findByText(versionLabel, { selector: "dt" })).parentElement).toHaveTextContent(`${debugFixture.app.version} (Build 2578)`);
    expect(row("Commit")).toHaveTextContent(debugFixture.app.build);
    expect(screen.queryByText("Build", { selector: "dt" })).toBeNull();
    expect(screen.queryByText("Erstellungsstand", { selector: "dt" })).toBeNull();
    unmount();
  }
});

// spec: CRAWL-127
it("matches recent entries to the crawl index by crawl ID when the remembered path spelling differs, else by path", async () => {
  core.uiState = { version: 1, state: { recentCrawls: [
    { path: "/private/fixture/link/uuid.ospider", crawlId: "crawl-uuid" },
    { path: "/fixture/other.ospider" },
    { path: "/fixture/replaced.ospider", crawlId: "crawl-gone" },
    { path: "/fixture/unknown.ospider", crawlId: "crawl-unknown" },
  ] } };
  core.handlers.set("crawls.list", () => [
    { crawl: { crawlId: "crawl-uuid", file: "/fixture/uuid.ospider", startedAt: "2026-10-29T12:00:00", name: "uuid" }, startUrl: "http://127.0.0.1:1234/fixture" },
    { crawl: { crawlId: "crawl-other", file: "/fixture/other.ospider", startedAt: "2026-10-29T12:00:00", name: "other" }, startUrl: null },
    { crawl: { crawlId: "crawl-new", file: "/fixture/replaced.ospider", startedAt: "2026-10-29T12:00:00", name: "replaced" }, startUrl: null },
  ]);
  expect(await loadRecent()).toEqual([
    { path: "/private/fixture/link/uuid.ospider", crawlId: "crawl-uuid", label: expect.stringMatching(/^uuid – Oct 29, 2026/) },
    { path: "/fixture/other.ospider", label: expect.stringMatching(/^other – Oct 29, 2026/) },
    { path: "/fixture/replaced.ospider", crawlId: "crawl-gone", label: expect.stringMatching(/^replaced – Oct 29, 2026/) },
    { path: "/fixture/unknown.ospider", crawlId: "crawl-unknown" },
  ]);
});

// spec: CRAWL-127
it("keeps the crawl ID of recent entries when normalizing and stores the ID of the crawl opened from the menu", async () => {
  expect(normalizeRecent([{ path: "/a.ospider", crawlId: "id-a", label: "A" }, { path: "/b.ospider", crawlId: 7 }, { path: "/c.ospider", crawlId: "" }]))
    .toEqual([{ path: "/a.ospider", crawlId: "id-a", label: "A" }, { path: "/b.ospider" }, { path: "/c.ospider" }]);
  core.uiState = { version: 1, state: { recentCrawls: [{ path: "/fixture/older.ospider", crawlId: "id-older" }, { path: "/fixture/typed.ospider", crawlId: "id-stale" }] } };
  core.handlers.set("crawl.open", () => ({ crawlId: "id-typed", file: "/fixture/resolved/typed.ospider" }));
  core.handlers.set("crawls.list", () => []);
  dispose = installMainMenus();
  await waitFor(() => expect(hasAction("recent.1")).toBe(true));
  runAction("recent.1", { source: "menu" });
  await waitFor(async () => expect((await loadRecent())[0]).toEqual({ path: "/fixture/typed.ospider", crawlId: "id-typed" }));
  expect((core.uiState.state as { recentCrawls: unknown }).recentCrawls).toEqual([
    { path: "/fixture/typed.ospider", crawlId: "id-typed" }, { path: "/fixture/older.ospider", crawlId: "id-older" },
  ]);
});

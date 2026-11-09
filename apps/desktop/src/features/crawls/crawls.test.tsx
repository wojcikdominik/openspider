import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setLanguage } from "../../i18n";
import { createClient, setClient } from "../../ipc/client";
import { IpcError, localError } from "../../ipc/errors";
import { useSession } from "../../state/session";
import { runAction } from "../../ui/actions";
import { allMenuContributions } from "../../ui/menu/registry";
import { CrawlsHost } from "./CrawlsHost";
import { managementFixture, fixtureRows } from "./fixtures/core";
import { columns, permissions, readRecent, sortRows, useCrawlsDialog, writeRecent } from "./model";
let fixture: ReturnType<typeof managementFixture>;
const calls = (cmd: string) => fixture.core.calls.filter(c => c.cmd === cmd);
async function mount(open = true) {
  useCrawlsDialog.setState({ open });
  render(<CrawlsHost desktop={fixture.desktop} />);
  if (open) await screen.findByRole("checkbox", { name: "Select: Alpha" });
}
const select = (name: string) => fireEvent.click(screen.getByRole("checkbox", { name: `Select: ${name}` }));
const button = (name: string) => screen.getByRole("button", { name });
const click = (name: string) => fireEvent.click(button(name));
beforeEach(async () => {
  fixture = managementFixture();
  setClient(createClient(fixture.core.transport("main")));
  localStorage.clear();
  useSession.setState({ crawl: null, ops: {} });
  useCrawlsDialog.setState({ open: false });
  await setLanguage("en");
});
afterEach(() => { cleanup(); setClient(null); useCrawlsDialog.setState({ open: false }); vi.restoreAllMocks(); });

// spec: UI-124
it("registers the File menu action and opens the project tree and complete metadata table", async () => {
  await mount(false);
  expect(allMenuContributions().flatMap(c => c.items).some(i => i.id === "crawls.manage")).toBe(true);
  act(() => { runAction("crawls.manage"); });
  await screen.findByRole("checkbox", { name: "Select: Alpha" });
  expect(screen.getByRole("treegrid", { name: "Projects" })).toBeVisible();
  for (const name of ["Project", "Name", "Mode", "Start URL", "Started", "Duration (ms)", "URLs", "Size on disk (bytes)", "Status", "Locked", "Database ID"]) expect(screen.getByRole("columnheader", { name })).toBeVisible();
  expect(calls("crawls.list")[0]?.args).toEqual({ show: "all" });
});

// spec: OUT-191 [M2]
it("shows task names and projects from the index, combines search with project and kind filters", async () => {
  await mount();
  expect(screen.getByText("list.csv")).toBeVisible();
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "KUNDE A" } });
  expect(screen.getAllByRole("checkbox")).toHaveLength(3);
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "beta.TEST" } });
  expect(screen.getByText("Beta")).toBeVisible(); expect(screen.queryByText("Alpha")).toBeNull();
  fireEvent.change(screen.getByRole("combobox", { name: "Show" }), { target: { value: "comparisons" } });
  expect(screen.getByText("No matching crawls.")).toBeVisible();
  expect(calls("crawls.list")).toHaveLength(1);
});

// spec: OUT-228
it("sorts every column, preserves ordered IDs and enables comparison with two inputs", async () => {
  await mount();
  click("Name"); select("Beta"); select("Alpha");
  click("Copy database ID");
  await waitFor(() => expect(calls("crawls.copyIds")[0]?.args).toEqual({ ids: fixtureRows.slice(0, 2).map(r => r.crawl.crawlId) }));
  expect(button("Choose for comparison")).toBeEnabled();
  expect(button("Choose for comparison").parentElement).not.toHaveAttribute("title");
  expect(screen.queryByText("Crawl comparison is not available yet.")).toBeNull();
  click("Choose for comparison");
  expect(calls("compare.open")).toHaveLength(1);
  for (const column of columns) expect(sortRows(fixtureRows, column, "asc")).toHaveLength(fixtureRows.length);
});

// spec: CRAWL-128
// spec: AGENT-020
it("shows a stopped crawl as completed (stopped) in both languages and never offers to resume it", async () => {
  const cellOf = (name: string) => within(screen.getByText(name).closest("tr")!).getAllByRole("cell")[columns.indexOf("phase") + 1]!;
  await mount();
  expect(cellOf("Geschützt")).toHaveTextContent(/^Completed \(stopped\)$/);
  expect(cellOf("Beta")).toHaveTextContent(/^Completed$/);
  select("Geschützt");
  expect(button("Resume")).toBeDisabled();
  // The export key stays on the phase (`Completed`); only the label differs.
  expect(fixtureRows.find(r => r.crawl.name === "Geschützt")!.crawl.phase).toBe("completed");
  cleanup();
  await setLanguage("de");
  useCrawlsDialog.setState({ open: true });
  render(<CrawlsHost desktop={fixture.desktop} />);
  await screen.findByRole("checkbox", { name: /Alpha/ });
  expect(cellOf("Geschützt")).toHaveTextContent(/^Abgeschlossen \(gestoppt\)$/);
  expect(cellOf("Beta")).toHaveTextContent(/^Abgeschlossen$/);
});

// spec: CRAWL-143
it("resumes a completed crawl with a remaining queue using the core's shared canResume decision (K2-34)", async () => {
  const rows = structuredClone(fixtureRows);
  rows[1]!.canResume = true; // Core listing for Completed with non-empty queue.
  fixture.setRows(rows);
  await mount(); select("Beta");
  expect(button("Resume")).toBeEnabled(); click("Resume");
  await waitFor(() => expect(calls("crawl.resume")).toHaveLength(1));
  expect(fixture.core.calls.filter(c => ["crawl.open", "crawl.resume"].includes(c.cmd)).map(c => c.cmd)).toEqual(["crawl.open", "crawl.resume"]);
  expect(calls("crawl.open")[0]?.args).toEqual({ kind: "id", crawlId: rows[1]!.crawl.crawlId });
  expect(permissions([fixtureRows[1]!]).resume).toBe(false); // Empty completed queue.
  expect(permissions([{ ...rows[0]!, canResume: false }]).resume).toBe(false);
});

// spec: CRAWL-143
it("protects running and locked crawls while paused and interrupted crawls can resume", async () => {
  await mount(); select("Live");
  for (const action of ["Delete", "Rename", "Move to project", "Export crawl…", "Resume", "Lock", "Unlock"]) expect(button(action)).toBeDisabled();
  expect(button("Export crawl…").parentElement).toHaveAttribute("title", "Crawl running – pause first");
  select("Live"); select("Geschützt");
  expect(button("Delete")).toBeDisabled(); expect(button("Rename")).toBeDisabled();
  select("Geschützt"); select("Alpha");
  expect(button("Export crawl…")).toBeEnabled(); expect(button("Resume")).toBeEnabled(); expect(button("Choose for comparison")).toBeDisabled();
  click("Resume");
  await waitFor(() => expect(calls("crawl.resume")).toHaveLength(1));
  expect(calls("crawl.open")[0]?.args).toEqual({ kind: "id", crawlId: fixtureRows[0]!.crawl.crawlId });
  expect(permissions([fixtureRows[4]!]).resume).toBe(true);
  expect(permissions([fixtureRows[5]!]).resume).toBe(false);
});

// spec: OUT-228
it("requires explicit delete confirmation with exact count and space; Escape/cancel sends no delete", async () => {
  await mount(); select("Alpha"); select("Beta"); click("Delete");
  const dialog = screen.getByRole("dialog", { name: "Delete" });
  expect(dialog.textContent?.replace(/[\u2066-\u2069,]/g, "")).toContain("Delete 2 crawls and free 3072 bytes? Deleted crawls cannot be recovered.");
  expect(calls("crawls.delete")).toHaveLength(0);
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(calls("crawls.delete")).toHaveLength(0);
  click("Delete"); fireEvent.click(within(screen.getByRole("dialog", { name: "Delete" })).getByRole("button", { name: "Delete" }));
  await waitFor(() => expect(calls("crawls.delete")).toHaveLength(1));
  expect(new Set((calls("crawls.delete")[0]?.args as { ids: string[] }).ids)).toEqual(new Set(fixtureRows.slice(0, 2).map(r => r.crawl.crawlId)));
  expect(screen.getByRole("progressbar")).toBeVisible();
});

// spec: OUT-191 [M2]
it("renames without changing ID and creates and renames projects through their contracts", async () => {
  await mount(); select("Alpha"); click("Rename");
  fireEvent.change(screen.getByRole("textbox", { name: "Name" }), { target: { value: "New task" } }); click("Apply");
  await screen.findByText("New task");
  expect(calls("crawls.rename")[0]?.args).toEqual({ crawlId: fixtureRows[0]!.crawl.crawlId, name: "New task" });
  click("Create project"); fireEvent.change(screen.getByRole("textbox", { name: "Name" }), { target: { value: "New project" } }); click("Apply");
  await waitFor(() => expect(button("Rename project")).toBeEnabled());
  click("Rename project"); fireEvent.change(screen.getByRole("textbox", { name: "Name" }), { target: { value: "Renamed project" } }); click("Apply");
  await waitFor(() => expect(calls("projects.rename")[0]?.args).toEqual({ id: "project-3", name: "Renamed project" }));
});

// spec: OUT-191 [M2]
it("keeps a rejected project name and its error inside the active form and allows a corrected retry", async () => {
  const createProject = fixture.core.handlers.get("projects.create")!;
  fixture.core.handlers.set("projects.create", (args, ctx) => {
    if ((args as { name: string }).name === "Kunde A") throw new IpcError(localError("InvalidInput", "err-file-exists", { path: "Kunde A" }));
    return createProject(args, ctx);
  });
  await mount(); click("Create project");
  const dialog = screen.getByRole("dialog", { name: "Create project" });
  const input = within(dialog).getByRole("textbox", { name: "Name" });
  fireEvent.change(input, { target: { value: "Kunde A" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Kunde A");
  expect(input).toHaveValue("Kunde A"); expect(input).toBeEnabled();
  expect(screen.getAllByRole("alert")).toHaveLength(1);
  fireEvent.change(input, { target: { value: "New project" } });
  expect(within(dialog).queryByRole("alert")).toBeNull();
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Create project" })).toBeNull());
  expect(fixture.projects().some(p => p.name === "New project")).toBe(true);
  click("Rename project");
  const renameForm = screen.getByRole("dialog", { name: "Rename project" });
  expect(within(renameForm).queryByRole("alert")).toBeNull();
  fixture.core.handlers.set("projects.rename", () => { throw new IpcError(localError("InvalidInput", "err-file-exists", { path: "Kunde A" })); });
  fireEvent.change(within(renameForm).getByRole("textbox"), { target: { value: "Kunde A" } });
  fireEvent.click(within(renameForm).getByRole("button", { name: "Apply" }));
  expect(await within(renameForm).findByRole("alert")).toHaveTextContent("Kunde A");
  expect(within(renameForm).getByRole("textbox")).toHaveValue("Kunde A");
});

// spec: OUT-228
it("shows rejected crawl rename and move requests in their active forms without losing input", async () => {
  for (const command of ["crawls.rename", "crawls.move"]) fixture.core.handlers.set(command, () => { throw new IpcError(localError("Permission", "err-file-exists", { path: "Protected" })); });
  await mount(); select("Alpha");
  for (const action of ["Rename", "Move to project"]) {
    click(action);
    const dialog = screen.getByRole("dialog", { name: action });
    const input = within(dialog).getByRole(action === "Rename" ? "textbox" : "combobox");
    fireEvent.change(input, { target: { value: "Protected" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Protected");
    expect(input).toHaveValue("Protected");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  }
});

// spec: CRAWL-143
it("moves a paused crawl without resuming, passes exports from project and root, and duplicates independently", async () => {
  await mount(); select("Alpha"); click("Move to project");
  fireEvent.change(screen.getByRole("combobox", { name: "Destination project" }), { target: { value: "Default" } }); click("Apply");
  await waitFor(() => expect(calls("crawls.move")[0]?.args).toEqual({ ids: [fixtureRows[0]!.crawl.crawlId], project: "Default" }));
  const h = fixture.handle()!;
  act(() => useSession.setState({ ops: { [h.opId]: { ...h, done: 1, state: "done" } } }));
  await waitFor(() => expect(button("Export crawl…")).toBeEnabled());
  click("Export crawl…");
  await waitFor(() => expect(calls("crawls.export")[0]?.args).toEqual({ ids: [fixtureRows[0]!.crawl.crawlId], dest: "/fixture/output.ospider" }));
  const ex = fixture.handle()!; act(() => useSession.setState({ ops: { [ex.opId]: { ...ex, done: 1, state: "done" } } }));
  await waitFor(() => expect(button("Duplicate")).toBeEnabled()); click("Duplicate");
  await waitFor(() => expect(calls("crawls.duplicate")[0]?.args).toEqual({ ids: [fixtureRows[0]!.crawl.crawlId] }));
  expect(calls("crawl.resume")).toHaveLength(0);
});

// spec: CRAWL-127 [M2]
it("native dialog cancellation sends no open/import/save/export and recent entries deduplicate and cap at ten", async () => {
  fixture.setPaths(null); fixture.setDestination(null); await mount();
  click("Open crawl…"); await waitFor(() => expect(button("Import crawls…")).toBeEnabled());
  click("Import crawls…"); await waitFor(() => expect(button("Export crawl…")).toBeDisabled());
  expect(calls("crawl.open")).toHaveLength(0); expect(calls("crawls.import")).toHaveLength(0);
  select("Alpha"); click("Export crawl…"); await waitFor(() => expect(button("Export crawl…")).toBeEnabled());
  expect(calls("crawls.export")).toHaveLength(0);
  writeRecent(["/new.ospider", "/new.ospider", ...Array.from({ length: 20 }, (_, i) => `/crawl-${i}.ospider`)]);
  expect(readRecent()).toHaveLength(10); expect(readRecent()[0]).toBe("/new.ospider");
});

// spec: CRAWL-127 [M2]
it("removes a missing recent file only on NotFound, and retains schema/permission failures", async () => {
  writeRecent(["/missing.ospider", "/denied.ospider"]);
  fixture.core.handlers.set("crawl.open", args => { const path = (args as { path: string }).path; throw new IpcError(localError(path.includes("missing") ? "NotFound" : "Permission", "err-not-a-crawl-file")); });
  await mount(); click("/missing.ospider");
  await waitFor(() => expect(readRecent()).toEqual(["/denied.ospider"]));
  expect(screen.getByRole("alert")).toHaveTextContent("/missing.ospider");
  click("/denied.ospider"); await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("/denied.ospider"));
  expect(readRecent()).toEqual(["/denied.ospider"]);
});

// spec: OUT-230 [M2]
it("offers recovery only for corruption and reports the new file and row loss after OpHandle completion", async () => {
  fixture.core.handlers.set("crawl.open", () => { throw new IpcError(localError("Store", "err-crawl-corrupt")); });
  const repair = vi.spyOn(fixture.desktop, "repair");
  await mount(); click("Open crawl…");
  await screen.findByRole("button", { name: "Attempt repair" }); click("Attempt repair");
  await waitFor(() => expect(repair).toHaveBeenCalledWith("/fixture/Alpha.ospider"));
  const h = fixture.handle()!;
  act(() => useSession.setState({ ops: { [h.opId]: { ...h, done: 1, state: "done", result: { path: "/fixture/Alpha.repaired.ospider", integrityOk: true, usable: true, lostRows: 3 } } } }));
  const result = await screen.findByRole("dialog", { name: "Repair result" });
  expect(result).toHaveTextContent("/fixture/Alpha.repaired.ospider"); expect(result.textContent?.replace(/[\u2066-\u2069]/g, "")).toContain("Skipped rows: 3");
});

// spec: OUT-230 [M2]
it("integrity verification and explicit configuration adoption use the selected file; opening alone never adopts", async () => {
  const check = vi.spyOn(fixture.desktop, "check"), configuration = vi.spyOn(fixture.desktop, "configuration");
  await mount(); select("Alpha"); click("Check integrity");
  await waitFor(() => expect(check).toHaveBeenCalledWith("/fixture/Alpha.ospider"));
  await waitFor(() => expect(button("Use configuration from crawl")).toBeEnabled()); click("Use configuration from crawl");
  await waitFor(() => expect(configuration).toHaveBeenCalledWith("/fixture/Alpha.ospider"));
  expect(calls("config.set")[0]?.args).toEqual({ schemaVersion: 2, generator: "fixture", config: { mode: "spider", fixture: true } });
});

// spec: OUT-228
it("batch lock and unlock require no confirmation and use the selection", async () => {
  await mount(); select("Alpha"); select("Beta"); click("Lock");
  await waitFor(() => expect(button("Delete")).toBeDisabled());
  await waitFor(() => expect(button("Unlock")).toBeEnabled()); click("Unlock");
  await waitFor(() => expect(button("Delete")).toBeEnabled());
  expect(calls("crawls.unlock")[0]?.args).toMatchObject({ ids: expect.arrayContaining(fixtureRows.slice(0, 2).map(r => r.crawl.crawlId)) });
});

// spec: OUT-228
it("retains more than 1000 records for paging, sorting, project filters and search while limiting every selection path", async () => {
  fixture.setRows(Array.from({ length: 1005 }, (_, i) => ({ ...fixtureRows[0]!, project: i === 1004 ? "Default" : "Kunde A", startUrl: `https://crawl-${i}.test/`, crawl: { ...fixtureRows[0]!.crawl, crawlId: `id-${i}`, name: `Crawl ${i}`, urls: i } })));
  useCrawlsDialog.setState({ open: true }); render(<CrawlsHost desktop={fixture.desktop} />);
  const count = (selected: number, total = 1005) => screen.getByText((_, node) => node?.textContent?.replace(/[\u2066-\u2069,]/g, "") === `${selected} selected / ${total} crawls`);
  await screen.findByText((_, node) => node?.textContent?.replace(/[\u2066-\u2069,]/g, "") === "0 selected / 1005 crawls");
  click("Name");
  expect(within(screen.getByRole("table")).getAllByRole("row")).toHaveLength(101);
  fireEvent.click(screen.getByRole("checkbox", { name: "Select all filtered crawls" }));
  expect(count(1000)).toBeVisible();
  fireEvent.click(screen.getByRole("checkbox", { name: "Select all filtered crawls" }));
  select("Crawl 0");
  for (let i = 0; i < 10; i++) click("Next page");
  expect(screen.getByText("Crawl 1004")).toBeVisible();
  fireEvent.click(screen.getByRole("checkbox", { name: "Select: Crawl 1004" }), { shiftKey: true });
  expect(count(1000)).toBeVisible();
  select("Crawl 1004"); expect(count(1000)).toBeVisible();
  expect(screen.getByRole("checkbox", { name: "Select: Crawl 1004" })).not.toBeChecked();
  fireEvent.click(screen.getByRole("checkbox", { name: "Select all filtered crawls" }));
  fireEvent.keyDown(screen.getByRole("checkbox", { name: "Select: Crawl 1004" }), { key: "a", ctrlKey: true });
  expect(count(1000)).toBeVisible(); click("Lock");
  await waitFor(() => expect((calls("crawls.lock")[0]?.args as { ids: string[] }).ids).toHaveLength(1000));
  await waitFor(() => expect(button("Refresh")).toBeEnabled());
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "crawl-1004.test" } });
  expect(screen.getByText("Crawl 1004")).toBeVisible(); expect(count(0, 1)).toBeVisible();
  select("Crawl 1004"); click("Copy database ID");
  await waitFor(() => expect(calls("crawls.copyIds")[0]?.args).toEqual({ ids: ["id-1004"] }));
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "" } });
  click("URLs"); click("URLs");
  expect(within(screen.getByRole("table")).getAllByRole("row")[1]).toHaveTextContent("Crawl 1004");
  fireEvent.click(screen.getByRole("row", { name: "Default" }));
  expect(count(0, 1)).toBeVisible(); expect(screen.getByText("Crawl 1004")).toBeVisible();
}, 60_000);

// spec: CRAWL-127
it("saving uses the active crawl and a native destination without changing registry membership", async () => {
  useSession.setState({ crawl: { crawlId: fixtureRows[0]!.crawl.crawlId, phase: "paused", mode: "spider", crawled: 1, queued: 15, discovered: 16, percent: 6,
    rate: 0, avgRate: 1, elapsedS: 1, pausedS: 0, activeThreads: 0, owner: fixtureRows[0]!.crawl.owner } });
  await mount(); click("Save crawl as…");
  await waitFor(() => expect(calls("crawl.saveAs")[0]?.args).toEqual({ dest: "/fixture/output.ospider" }));
  expect(calls("crawls.delete")).toHaveLength(0); expect(calls("crawls.import")).toHaveLength(0); expect(calls("crawl.open")).toHaveLength(0);
});

// spec: CRAWL-127
it("asks to pause before opening another file while a crawl is running", async () => {
  useSession.setState({ crawl: { crawlId: "current", phase: "running", mode: "spider", crawled: 1, queued: 15, discovered: 16, percent: 6,
    rate: 1, avgRate: 1, elapsedS: 1, pausedS: 0, activeThreads: 1, owner: fixtureRows[0]!.crawl.owner } });
  let paused = false;
  fixture.core.handlers.set("crawl.pause", () => { paused = true; });
  fixture.core.handlers.set("crawl.open", () => { if (!paused) throw new IpcError(localError("Busy", "err-crawl-busy")); return fixtureRows[0]!.crawl; });
  await mount(); click("Open crawl…");
  await screen.findByRole("dialog", { name: "Pause current crawl?" });
  expect(calls("crawl.open")).toHaveLength(1); expect(calls("crawl.pause")).toHaveLength(0);
  click("Pause and open");
  await waitFor(() => expect(calls("crawl.open")).toHaveLength(2));
  expect(fixture.core.calls.findIndex(c => c.cmd === "crawl.pause")).toBeLessThan(fixture.core.calls.map(c => c.cmd).lastIndexOf("crawl.open"));
  expect(calls("config.set")).toHaveLength(0);
});

// spec: CRAWL-127
it("native recent menu labels preserve literal user paths without treating them as translation keys", async () => {
  const { buildMenuModel, flattenItems } = await import("../../ui/menu/build");
  writeRecent(["/Users/fixture/project-name/Crawl_A.ospider"]);
  await mount(false);
  const menu = buildMenuModel(allMenuContributions(), { platform: "mac", lang: "en", native: true, t: key => key, ctx: {} });
  const item = menu.menus.flatMap(m => flattenItems(m.items)).find(i => i.id === "crawls.recent.0");
  expect(item?.label).toBe("/Users/fixture/project-name/Crawl_A.ospider");
});

// spec: CRAWL-127
it("confirms missing files after the real core's Io canonicalization error before pruning history", async () => {
  writeRecent(["/missing.ospider"]);
  fixture.core.handlers.set("crawl.open", () => { throw new IpcError(localError("Io", "err-io")); });
  vi.spyOn(fixture.desktop, "check").mockRejectedValue(new IpcError(localError("NotFound", "err-file-not-found")));
  await mount(); click("/missing.ospider");
  await waitFor(() => expect(readRecent()).toEqual([]));
  expect(screen.getByRole("alert")).toHaveTextContent("/missing.ospider");
});

// spec: CRAWL-127
it("normalizes the native open dialog's single, multiple and cancelled results and requests portable crawl files", async () => {
  const { nativeCrawlDesktop } = await import("./desktop");
  fixture.core.handlers.set("dialog.open", () => "/fixture/Alpha.ospider");
  expect(await nativeCrawlDesktop.chooseOpen(false, "Open")).toEqual(["/fixture/Alpha.ospider"]);
  expect(calls("dialog.open")[0]?.args).toEqual({ options: { title: "Open", multiple: false, filters: [{ name: "OpenSpider", extensions: ["ospider"] }] } });
  fixture.core.handlers.set("dialog.open", () => ["/fixture/Alpha.ospider", "/fixture/Beta.ospider"]);
  expect(await nativeCrawlDesktop.chooseOpen(true, "Import")).toHaveLength(2);
  fixture.core.handlers.set("dialog.open", () => null);
  expect(await nativeCrawlDesktop.chooseOpen(false, "Open")).toBeNull();
});

// spec: OUT-230 [M2]
it("routes file integrity, configuration and repair through the added native service adapters", async () => {
  const { nativeCrawlDesktop } = await import("./desktop");
  const dto = await fixture.desktop.configuration("/fixture/Alpha.ospider");
  const handle = { opId: "native-repair", labelKey: "crawls-repair", cancellable: false };
  fixture.core.handlers.set("crawl_file_check", () => undefined);
  fixture.core.handlers.set("crawl_file_config", () => dto);
  fixture.core.handlers.set("crawl_file_repair", () => handle);
  await nativeCrawlDesktop.check("/fixture/Alpha.ospider");
  expect(await nativeCrawlDesktop.configuration("/fixture/Alpha.ospider")).toEqual(dto);
  expect(await nativeCrawlDesktop.repair("/fixture/Alpha.ospider")).toEqual(handle);
  for (const command of ["crawl_file_check", "crawl_file_config", "crawl_file_repair"]) expect(calls(command)[0]?.args).toEqual({ path: "/fixture/Alpha.ospider" });
});

// spec: CRAWL-127 [M2]
it("opening the already active running file relies on core identity and never asks to pause it", async () => {
  useSession.setState({ crawl: { crawlId: fixtureRows[0]!.crawl.crawlId, phase: "running", mode: "spider", crawled: 1, queued: 15, discovered: 16, percent: 6,
    rate: 1, avgRate: 1, elapsedS: 1, pausedS: 0, activeThreads: 1, owner: fixtureRows[0]!.crawl.owner } });
  fixture.core.handlers.set("crawl.open", () => ({ ...fixtureRows[0]!.crawl, phase: "running" }));
  await mount(); click("Open crawl…");
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(calls("crawl.pause")).toHaveLength(0);
  expect(calls("crawl.open")).toHaveLength(1);
  expect(useSession.getState().crawl?.phase).toBe("running");
});

// spec: CRAWL-127
it("names the file and distinguishes newer schemas and foreign formats without offering repair or deleting history", async () => {
  writeRecent(["/fixture/Alpha.ospider"]);
  fixture.core.handlers.set("crawl.open", () => { throw new IpcError(localError("Store", "err-crawl-newer-version", { generator: "OpenSpider 99.0", found: 99 })); });
  await mount(); click("Open crawl…");
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Please update OpenSpider"));
  expect(screen.getByRole("alert")).toHaveTextContent("/fixture/Alpha.ospider");
  expect(screen.getByRole("alert")).toHaveTextContent("OpenSpider 99.0");
  expect(screen.queryByRole("button", { name: "Attempt repair" })).toBeNull();
  expect(readRecent()).toEqual(["/fixture/Alpha.ospider"]);
  fixture.core.handlers.set("crawl.open", () => { throw new IpcError(localError("Store", "err-not-a-crawl-file")); });
  click("Open crawl…");
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Not an OpenSpider crawl file"));
  expect(screen.getByRole("dialog", { name: "Manage crawls…" })).toBeVisible();
  expect(readRecent()).toEqual(["/fixture/Alpha.ospider"]);
});

// spec: OUT-228
it.each(["lock", "unlock"] as const)("refreshes stale rows after rejected %s and keeps the action error", async action => {
  const rows = structuredClone(fixtureRows);
  rows[0]!.locked = action === "unlock";
  fixture.setRows(rows);
  fixture.core.handlers.set(`crawls.${action}`, () => {
    rows[0]!.crawl.phase = "running";
    fixture.setRows(rows);
    throw new IpcError(localError("InvalidInput", "err-file-exists", { path: "action-failed" }));
  });
  await mount(); select("Alpha"); click(action === "lock" ? "Lock" : "Unlock");
  expect(await screen.findByRole("alert")).toHaveTextContent("action-failed");
  await waitFor(() => expect(calls("crawls.list")).toHaveLength(2));
  expect(calls("projects.list")).toHaveLength(2);
  expect(button("Lock")).toBeDisabled(); expect(button("Unlock")).toBeDisabled();
  expect(screen.getByRole("alert")).toHaveTextContent("action-failed");
});

// spec: OUT-228
it("keeps the lock error when the subsequent refresh also fails", async () => {
  await mount(); select("Alpha");
  fixture.core.handlers.set("crawls.lock", () => { throw new IpcError(localError("InvalidInput", "err-file-exists", { path: "action-failed" })); });
  fixture.core.handlers.set("crawls.list", () => { throw new IpcError(localError("InvalidInput", "err-file-exists", { path: "refresh-failed" })); });
  click("Lock");
  expect(await screen.findByRole("alert")).toHaveTextContent("action-failed");
  expect(calls("crawls.list")).toHaveLength(2);
  expect(screen.getByRole("alert")).not.toHaveTextContent("refresh-failed");
});

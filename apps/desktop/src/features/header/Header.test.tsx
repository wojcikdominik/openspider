import { afterEach, beforeEach, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Header } from "./Header";
import { useHeader, normalizeInput, rememberUrl } from "./model";
import { setupHeader, tick } from "./fixtures/setup";
import { useSession } from "../../state/session";
import { setClient } from "../../ipc/client";
import { eventBus } from "../../ipc/events";
import { setLanguage } from "../../i18n";
import { runAction } from "../../ui/actions";
import { useComparison } from "../compare/model";
import { session as compareSession } from "../compare/fixtures/out-compare/core";

let core: ReturnType<typeof setupHeader>;
beforeEach(async () => { core = setupHeader(); await setLanguage("en"); });
afterEach(() => { cleanup(); setClient(null); useComparison.setState({ session: null }); });
const start = () => screen.getByRole("button", { name: "Start" });
const calls = (cmd: string) => core.calls.filter((c) => c.cmd === cmd);
const enterUrl = (value: string) => fireEvent.change(screen.getByLabelText("Start URL"), { target: { value } });

// spec: UI-003
it("validates fixture inputs, normalizes HTTPS, and submits Enter through the core validator", async () => {
  render(<Header />);
  expect(start()).toBeDisabled();
  for (const value of ["javascript:alert(1)", "ht tp://x", "https://ex ample.test", "ftp://site.test", "htp:/x"]) {
    enterUrl(value); expect(start()).toBeDisabled();
    expect(screen.getByText("Enter a valid HTTP or HTTPS address.")).toBeVisible();
  }
  enterUrl("  127.0.0.1:8123/sub/  ");
  fireEvent.submit(start().closest("form")!);
  await waitFor(() => expect(calls("crawl.start")).toHaveLength(1));
  expect(calls("crawl.validateStartUrl")[0]?.args).toEqual({ input: "https://127.0.0.1:8123/sub/" });
  expect(calls("crawl.start")[0]?.args).toEqual({ source: { kind: "spider", url: "https://127.0.0.1:8123/sub/" }, scope: "domain" });
  expect(normalizeInput("site.test/a b")).toBe("https://site.test/a%20b");
});

// spec: CRAWL-001 [M2]
it("keeps the newest 20 unique suggestions across remounts, preserving other UI state", async () => {
  core.uiState.state = { table: { width: 99 }, header: { history: ["https://old.test/", "https://old.test/"] } };
  const view = render(<Header />);
  await waitFor(() => expect(document.querySelectorAll("datalist option")).toHaveLength(1));
  enterUrl("site.test"); fireEvent.click(start());
  await waitFor(() => expect(calls("uistate.save")).toHaveLength(1));
  expect(core.uiState.state).toMatchObject({ table: { width: 99 }, header: { history: ["https://site.test/", "https://old.test/"] } });
  view.unmount(); render(<Header />);
  await waitFor(() => expect([...document.querySelectorAll("datalist option")].map((n) => n.getAttribute("value"))).toEqual(["https://site.test/", "https://old.test/"]));
  const many = Array.from({ length: 25 }, (_, n) => `https://site.test/${n}`);
  expect(rememberUrl(many, many[4]!)).toEqual([many[4], ...many.filter((u) => u !== many[4])].slice(0, 20));
});

// spec: CRAWL-001 [M2]
it("requires pause before starting another crawl and mirrors state only from ticks", async () => {
  useSession.getState().applyTick(tick);
  render(<Header />); enterUrl("next.test");
  // K2-63/K2-77: one primary control changes from Start to Pause.
  expect(screen.queryByRole("button", { name: "Start" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Pause" }));
  await waitFor(() => expect(calls("crawl.pause")).toHaveLength(1));
  expect(useSession.getState().crawl?.phase).toBe("running");
  act(() => useSession.getState().applyTick({ ...tick, seq: 2, crawl: { ...tick.crawl!, phase: "paused" } }));
  expect(await screen.findByRole("button", { name: "Resume" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "Start" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Clear" }));
  await waitFor(() => expect(calls("crawl.clear")).toHaveLength(1));
  expect(screen.queryByRole("dialog")).toBeNull();
});

// spec: UI-003
it("resumes an interrupted or restored paused crawl without requiring a URL", async () => {
  useSession.getState().applyTick({ ...tick, crawl: { ...tick.crawl!, phase: "incomplete" } });
  render(<Header />);
  // K2-63/K2-77: interrupted crawl offers Resume as its primary action.
  expect(screen.queryByRole("button", { name: "Start" })).toBeNull();
  act(() => { runAction("crawl.pause"); });
  await waitFor(() => expect(calls("crawl.resume")).toHaveLength(1));
});

// spec: UI-003
it("asks before recrawling completed data and serializes fast repeated Start actions", async () => {
  useSession.getState().applyTick({ ...tick, crawl: { ...tick.crawl!, phase: "completed" } });
  render(<Header />); enterUrl("next.test");
  fireEvent.click(start()); fireEvent.click(start());
  expect(await screen.findByRole("dialog")).toBeVisible();
  expect(calls("crawl.start")).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "Replace" }));
  await waitFor(() => expect(calls("crawl.start")).toHaveLength(1));
  expect(calls("crawl.validateStartUrl")).toHaveLength(1);
});

// spec: CRAWL-009 [M2]
it("warns about a missing slash only for folder paths without a filename", () => {
  render(<Header />); enterUrl("https://site.test/blog");
  fireEvent.change(screen.getByLabelText("Scope"), { target: { value: "folder" } });
  expect(screen.getByText(/Without a trailing slash/)).toBeVisible();
  enterUrl("https://site.test/blog/index.html"); expect(screen.queryByText(/Without a trailing slash/)).toBeNull();
  enterUrl("https://site.test/blog/"); expect(screen.queryByText(/Without a trailing slash/)).toBeNull();
});

// spec: CRAWL-026 [M2]
it("confirms a mode change, clears only the view, and retains configuration", async () => {
  useSession.getState().applyTick({ ...tick, crawl: { ...tick.crawl!, phase: "paused" } });
  render(<Header />);
  fireEvent.change(screen.getByLabelText("Mode"), { target: { value: "list" } });
  fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
  expect(useHeader.getState().mode).toBe("spider");
  fireEvent.change(screen.getByLabelText("Mode"), { target: { value: "list" } });
  fireEvent.click(await screen.findByRole("button", { name: "Replace" }));
  await waitFor(() => expect(useHeader.getState().mode).toBe("list"));
  expect(calls("crawl.clear")).toHaveLength(1);
  expect(calls("config.set")).toHaveLength(0);
  expect(screen.queryByLabelText("Scope")).toBeNull();
});

// spec: CRAWL-112 [M2]
it("warns for configured concurrency from 20 threads, including live changes", async () => {
  render(<Header />);
  expect(screen.queryByText(/20 or more threads/)).toBeNull();
  core.handlers.set("config.get", () => ({ config: { speed: { threads: 20 } } }));
  act(() => eventBus.emit({ kind: "settingsChanged", keys: ["speed.threads"] }));
  expect(await screen.findByText(/20 or more threads/)).toBeVisible();
});

// spec: UI-003
it("imports a complete URL list without truncating to the core preview sample", async () => {
  render(<Header />);
  fireEvent.change(screen.getByLabelText("Mode"), { target: { value: "list" } });
  const items = [...Array.from({ length: 40 }, (_, i) => `https://site.test/${i}`), "https://site.test/0"];
  fireEvent.change(screen.getByLabelText("Paste URLs (one per line)"), { target: { value: items.join("\n") } });
  fireEvent.click(start());
  await waitFor(() => expect(calls("crawl.start")).toHaveLength(1));
  expect(calls("crawl.start")[0]?.args).toMatchObject({ source: { kind: "list", items } });
});

// spec: UI-003
it("disables control commands for a foreign live crawl", () => {
  useSession.getState().applyTick({ ...tick, crawl: { ...tick.crawl!, owner: { ...tick.crawl!.owner, controllable: false } } });
  render(<Header />); enterUrl("next.test");
  expect(screen.queryByRole("button", { name: "Start" })).toBeNull(); expect(screen.getByRole("button", { name: "Pause" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Clear" })).toBeDisabled();
  act(() => { runAction("crawl.pause"); runAction("crawl.new"); });
  expect(calls("crawl.pause")).toHaveLength(0); expect(calls("crawl.clear")).toHaveLength(0);
});

// spec: UI-003
it("passes sitemap URLs to the existing engine list planner without fetching in the UI", async () => {
  render(<Header />);
  fireEvent.change(screen.getByLabelText("Mode"), { target: { value: "list" } });
  fireEvent.change(screen.getByLabelText("Source"), { target: { value: "sitemap" } });
  expect(start()).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Sitemap URL"), { target: { value: "site.test/sitemap.xml.gz" } });
  fireEvent.click(start());
  await waitFor(() => expect(calls("crawl.start")).toHaveLength(1));
  expect(calls("crawl.start")[0]?.args).toMatchObject({ source: { kind: "list", items: ["https://site.test/sitemap.xml.gz"] } });
  expect(calls("crawl.parseList")).toHaveLength(0);
});

// spec: UI-003
it("shows the start URL of a reopened crawl and leaves typed text alone until another crawl opens (B3-UI)", async () => {
  render(<Header />);
  const field = screen.getByLabelText("Start URL") as HTMLInputElement;
  expect(field.value).toBe("");
  // A saved crawl is opened (no engine in this process): the status of the crawl file carries its start URL.
  const opened = { ...tick, seq: 1, crawl: { ...tick.crawl!, crawlId: "saved-1", phase: "completed" as const, startUrl: "https://saved.test/blog/" } };
  act(() => useSession.getState().applyTick(opened));
  await waitFor(() => expect(field.value).toBe("https://saved.test/blog/"));
  enterUrl("other.test");
  act(() => useSession.getState().applyTick({ ...opened, seq: 2, crawl: { ...opened.crawl, crawled: opened.crawl.crawled + 1 } }));
  expect(field.value).toBe("other.test");
  act(() => useSession.getState().applyTick({ ...opened, seq: 3, crawl: { ...opened.crawl, crawlId: "saved-2", startUrl: "https://second.test/" } }));
  await waitFor(() => expect(field.value).toBe("https://second.test/"));
  // List crawls have no start URL; the field keeps its value.
  act(() => useSession.getState().applyTick({ ...opened, seq: 4, crawl: { ...opened.crawl, crawlId: "list-1", mode: "list" as const, startUrl: null } }));
  expect(useHeader.getState().url).toBe("https://second.test/");
});

// spec: UI-003
it("keeps a typed URL after clearing a reopened crawl, even if the cleared status still names a start URL (B3-UI)", async () => {
  render(<Header />);
  const field = screen.getByLabelText("Start URL") as HTMLInputElement;
  const opened = { ...tick, seq: 1, crawl: { ...tick.crawl!, crawlId: "saved-1", phase: "completed" as const, startUrl: "https://a.test/" } };
  act(() => useSession.getState().applyTick(opened));
  await waitFor(() => expect(field.value).toBe("https://a.test/"));
  enterUrl("b.test");
  fireEvent.click(screen.getByRole("button", { name: "Clear" }));
  await waitFor(() => expect(calls("crawl.clear")).toHaveLength(1));
  // The cleared view: empty crawl id, idle; an older core still sent the start URL here.
  act(() => useSession.getState().applyTick({ ...opened, seq: 2, crawl: { ...opened.crawl, crawlId: "", phase: "idle" as const } }));
  act(() => useSession.getState().applyTick({ ...opened, seq: 3, crawl: { ...opened.crawl, crawlId: "", phase: "idle" as const, startUrl: null } }));
  expect(field.value).toBe("b.test");
});

// spec: UI-013
// spec: OUT-140
// B4-CMPUI: during a comparison the header mode reads „Vergleich“; choosing another mode closes the comparison first.
it("shows the comparison mode while a comparison is open and closes it for another mode", async () => {
  core.handlers.set("compare.close", () => null);
  await setLanguage("de");
  render(<Header />);
  act(() => { useComparison.setState({ session: compareSession }); });
  const mode = screen.getByLabelText("Modus") as HTMLSelectElement;
  expect(mode.value).toBe("compare");
  expect(mode.selectedOptions[0]).toHaveTextContent("Vergleich");
  fireEvent.change(mode, { target: { value: "list" } });
  await waitFor(() => expect(calls("compare.close")).toHaveLength(1));
  await waitFor(() => expect(useComparison.getState().session).toBeNull());
  expect(useHeader.getState().mode).toBe("list");
  expect(mode.value).toBe("list");
  await setLanguage("en");
});

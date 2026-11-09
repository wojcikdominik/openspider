import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Notifications, notifyCompletion } from "./Notifications";
import { reportError } from "./controller";
import { setupHeader, tick } from "../header/fixtures/setup";
import { Header } from "../header/Header";
import { useHeader } from "../header/model";
import { useSession } from "../../state/session";
import { usePrefs } from "../../state/prefs";
import { setLanguage } from "../../i18n";
import type { Notice } from "../../ipc/gen/Notice";
import { IpcError, localError } from "../../ipc/errors";
import { closeConfig, useConfigDialog } from "../config/frame/store";

let core: ReturnType<typeof setupHeader>;
beforeEach(async () => { core = setupHeader(); await setLanguage("en"); usePrefs.setState({ notifications: true }); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
function notice(code: string, args: Notice["args"] = {}, level: Notice["level"] = "error") { act(() => useSession.getState().pushNotice({ level, code, args })); }

// spec: UI-019
it("shows DNS, connection, TLS, robots and cross-host failures without hiding the cause", async () => {
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  render(<Notifications />);
  for (const [code, text] of [["start-dns", /DNS/], ["start-connection", /connection.*failed/], ["start-tls", /TLS certificate/], ["start-robots", /robots.txt blocks/], ["start-redirect-host", /different host/]] as const) {
    notice(code, { status: 0, target: "https://other.test/" });
    expect(await screen.findByRole("dialog")).toBeVisible();
    expect(screen.getByText(text)).toBeVisible();
    if (code === "start-robots") {
      fireEvent.click(screen.getByRole("button", { name: /Ignore robots.txt/ }));
      // N01b: configuration anchors open the in-app configuration dialog (UI-016), not the settings window.
      expect(useConfigDialog.getState()).toMatchObject({ open: true, anchor: "robots.mode" });
      expect(open).not.toHaveBeenCalled();
      closeConfig();
      expect(core.calls.some((c) => c.cmd === "config.set")).toBe(false);
    }
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(useSession.getState().notices).toHaveLength(0));
  }
});

// spec: UI-019
it.each(["unknown-info", "app-quit-confirmation"])("dismisses informational notice %s without opening a dialog", (code) => {
  useSession.getState().applyTick({ ...tick, notices: [{ level: "info", code, args: {} }] });
  render(<Notifications />);
  act(() => useSession.getState().pushNotice({ level: "info", code, args: {} }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(useSession.getState().notices).toHaveLength(0);
});

// spec: UI-019
it.each(["warning", "error"] as const)("shows a %s dialog even with an informational notice queued first", (level) => {
  act(() => {
    useSession.getState().pushNotice({ level: "info", code: "unknown-info", args: {} });
    useSession.getState().pushNotice({ level, code: "disk-low", args: { path: "crawls", freeBytes: 1234 } });
  });
  render(<Notifications />);
  expect(screen.getByRole("dialog")).toBeVisible();
  expect(screen.getByText(/crawls.*1,?234/)).toBeVisible();
  expect(useSession.getState().notices).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(useSession.getState().notices).toHaveLength(0);
});

// spec: UI-019
it("shows a start command failure immediately and offers an explicit HTTP retry address", async () => {
  render(<><Header /><Notifications /></>);
  core.handlers.set("crawl.start", () => { throw new IpcError(localError("Engine", "start-connection")); });
  fireEvent.change(screen.getByLabelText("Start URL"), { target: { value: "site.test" } });
  fireEvent.click(screen.getByRole("button", { name: "Start" }));
  expect(await screen.findByText(/connection.*failed/)).toBeVisible();
  expect(core.calls.filter((c) => c.cmd === "crawl.start")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: /Use HTTP address/ }));
  expect(useHeader.getState().url).toBe("http://site.test/");
  expect(core.calls.filter((c) => c.cmd === "crawl.start")).toHaveLength(1);
});

// spec: UI-019
it("shows disk location and available bytes, and exposes log/diagnostic actions for core errors", async () => {
  const write = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: write } });
  core.handlers.set("diag.debugInfo", () => ({ version: "fixture", platform: "test" }));
  core.handlers.set("diag.openLogs", () => undefined);
  render(<Notifications />); notice("disk-low", { path: "crawls", freeBytes: 1234 });
  expect(await screen.findByText(/crawls.*1,?234/)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Open logs" }));
  await waitFor(() => expect(core.calls.some((c) => c.cmd === "diag.openLogs")).toBe(true));
  await waitFor(() => expect(screen.getByRole("button", { name: "Copy diagnostics" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "Copy diagnostics" }));
  expect(await screen.findByText("Diagnostics copied.")).toBeVisible();
  expect(write).toHaveBeenCalledWith(JSON.stringify({ version: "fixture", platform: "test" }, null, 2));
});

// spec: UI-019
it("sends completion notifications only in the background with notifications enabled", async () => {
  const send = vi.fn(); class BrowserNotification { static permission = "granted"; constructor(title: string, options: unknown) { send(title, options); } }
  vi.stubGlobal("Notification", BrowserNotification);
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  await notifyCompletion("Crawl completed", "1200 URLs crawled."); expect(send).not.toHaveBeenCalled();
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
  render(<Notifications />); notice("crawl-completed", { crawled: 1200 }, "info");
  await waitFor(() => expect(send).toHaveBeenCalledWith("Crawl completed", { body: "1,200 URLs crawled." }));
  expect(screen.queryByRole("dialog")).toBeNull();
  usePrefs.setState({ notifications: false }); await notifyCompletion("Analysis completed", "1200 URLs crawled.");
  expect(send).toHaveBeenCalledOnce();
});

// spec: CRAWL-117 [M2]
it("prompts once per host/realm, requires HTTP consent, and keeps the secret out of configuration", async () => {
  useSession.getState().applyTick({ ...tick, crawl: { ...tick.crawl!, phase: "paused" } });
  core.handlers.set("auth.saveProfile", () => ({ name: "encrypted-profile", hasSecret: true }));
  render(<Notifications />);
  const challenge = { host: "http://basic.site.test", realm: "Test" };
  notice("auth-required", challenge);
  expect(await screen.findByText("Host: http://basic.site.test · Realm: Test")).toBeVisible();
  fireEvent.change(screen.getByLabelText("Username"), { target: { value: "prüfer" } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: "geheim" } });
  const submit = screen.getByRole("button", { name: "Save securely and resume" });
  expect(submit).toBeDisabled();
  fireEvent.click(screen.getByRole("checkbox")); fireEvent.click(submit);
  await waitFor(() => expect(core.calls.some((c) => c.cmd === "crawl.resume")).toBe(true));
  const cfg = core.calls.find((c) => c.cmd === "config.set")?.args;
  expect(cfg).toMatchObject({ config: { keep: true, auth: { profile_refs: ["encrypted-profile"] } } });
  expect(JSON.stringify(cfg)).not.toContain("geheim"); expect(JSON.stringify(core.uiState.state)).not.toContain("geheim");
  notice("auth-required", challenge);
  await waitFor(() => expect(useSession.getState().notices).toHaveLength(0));
});

// spec: CRAWL-117 [M2]
it("cancel resumes without saving credentials", async () => {
  render(<Notifications />); notice("auth-required", { host: "https://basic.site.test", realm: "Test" });
  fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(core.calls.some((c) => c.cmd === "crawl.resume")).toBe(true));
  expect(core.calls.some((c) => c.cmd === "auth.saveProfile")).toBe(false);
});

// spec: CRAWL-129
// spec: UI-016
it("shows the memory pause banner and three direct settings links", () => {
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  useSession.getState().applyTick({ ...tick, crawl: { ...tick.crawl!, memoryStage: "paused" } });
  render(<Notifications />);
  expect(screen.getByText("Low memory – crawl paused")).toBeVisible();
  for (const [name, anchor] of [["Reduce threads", "speed.threads"], ["Disable rendering", "rendering.mode"], ["Disable HTML storage", "extraction.store_html"]]) {
    fireEvent.click(screen.getByRole("button", { name }));
    expect(useConfigDialog.getState()).toMatchObject({ open: true, anchor });
    expect(open).not.toHaveBeenCalled();
  }
});

// spec: UI-019
it("queues unexpected errors and preserves technical details separately", () => {
  render(<Notifications />);
  act(() => reportError(new Error("fixture failure")));
  expect(screen.getByText("fixture failure")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Open logs" })).toBeVisible();
});

// spec: CRAWL-127
// spec: UI-124
// B4-CMPUI: a failed open is reported once, in the status bar with its technical details; no modal with the raw code.
it("does not open a dialog with the raw code for a failed crawl open", async () => {
  render(<Notifications />);
  notice("crawl-open-failed", { code: "NotFound", messageKey: "err-file-not-found", args: { path: "/tmp/gone.ospider" }, detail: null });
  await waitFor(() => expect(useSession.getState().notices).toHaveLength(0));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByText(/crawl-open-failed/)).toBeNull();
  expect(useSession.getState().openError?.code).toBe("crawl-open-failed");
});

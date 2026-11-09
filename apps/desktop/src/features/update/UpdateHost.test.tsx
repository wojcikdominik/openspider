import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { setLanguage } from "../../i18n";
import { update } from "../../ipc/update";
import type { UpdateState } from "../../ipc/gen/UpdateState";
import { runAction } from "../../ui/actions";
import { STATUS_TIMEOUT_MS, UpdateHost } from "./UpdateHost";
import { accept, CHECK_TIMEOUT_MS, useUpdate } from "./state";

const native = vi.hoisted(() => ({
  tauri: false,
  win: { isMinimized: vi.fn(async () => true), unminimize: vi.fn(async () => undefined), show: vi.fn(async () => undefined), setFocus: vi.fn(async () => undefined) },
}));
vi.mock("../../ipc/transport/tauri", async (original) => ({ ...(await original<typeof import("../../ipc/transport/tauri")>()), isTauri: () => native.tauri }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native.win }));

beforeEach(async () => {
  await setLanguage("en");
  vi.useFakeTimers();
  native.tauri = false;
  useUpdate.setState({ state: "idle", release: null, open: false, queued: null });
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });
const flush = (ms = 0) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });
/** Never settles: a check or status request whose answer got lost (e.g. while the computer slept). */
const hanging = <T,>() => new Promise<T>(() => undefined);

// spec: OUT-251
it("polls every 30 seconds at rest and every second only while checking or downloading", async () => {
  let state: UpdateState = "idle";
  const status = vi.spyOn(update, "status").mockImplementation(async () => state);
  render(<UpdateHost />);
  await act(async () => {});
  expect(status).toHaveBeenCalledTimes(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(29999); });
  expect(status).toHaveBeenCalledTimes(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(status).toHaveBeenCalledTimes(2);
  for (const next of ["checking", { downloading: { done: 5, total: 10 } }] as const) {
    state = next;
    act(() => accept(state));
    const before = status.mock.calls.length;
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(status).toHaveBeenCalledTimes(before + 1);
  }
  state = { ready: { version: "1.0.1" } };
  act(() => accept(state));
  const before = status.mock.calls.length;
  await act(async () => { await vi.advanceTimersByTimeAsync(29999); });
  expect(status).toHaveBeenCalledTimes(before);
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(status).toHaveBeenCalledTimes(before + 1);
});

// spec: OUT-250
// spec: UI-002 [M3]
it("always shows a state for a manual check: a hanging check ends with a timeout and a retry, late answers are ignored (B3-UI)", async () => {
  vi.spyOn(update, "status").mockImplementation(async () => "idle");
  let resolveFirst!: (state: UpdateState) => void;
  const check = vi.spyOn(update, "check").mockImplementationOnce(() => new Promise(r => { resolveFirst = r; }));
  render(<UpdateHost />);
  await flush();
  act(() => { runAction("help.updates", { source: "menu" }); });
  await flush();
  expect(screen.getByRole("dialog", { name: "Software updates" })).toBeInTheDocument();
  expect(screen.getByText("Checking for updates …")).toBeInTheDocument();
  // Status polls while the check waits do not replace "Checking…".
  await flush(5000);
  expect(screen.getByText("Checking for updates …")).toBeInTheDocument();
  await flush(CHECK_TIMEOUT_MS);
  expect(screen.getByRole("alert")).toHaveTextContent("The update check did not answer in time. Please try again.");
  // A polled "checking" of the stuck core does not bring back the endless "Checking…".
  vi.mocked(update.status).mockImplementation(async () => "checking");
  await flush(30000);
  expect(screen.getByRole("alert")).toHaveTextContent("did not answer in time");
  check.mockImplementationOnce(async () => "upToDate");
  fireEvent.click(screen.getByRole("button", { name: "Check again" }));
  await flush();
  expect(screen.getByText("OpenSpider is up to date.")).toBeInTheDocument();
  // The old check answers much later: its result must not replace the newer one.
  act(() => resolveFirst({ available: { version: "9.9.9", notes: "late", pubDate: null } }));
  await flush();
  expect(screen.getByText("OpenSpider is up to date.")).toBeInTheDocument();
  expect(screen.queryByText("Version 9.9.9")).toBeNull();
});

// spec: OUT-250
it("reopens the dialog after it was closed while a check was still running, and a new check supersedes the old one (B3-UI)", async () => {
  vi.spyOn(update, "status").mockImplementation(async () => "idle");
  vi.spyOn(update, "check").mockImplementationOnce(() => hanging()).mockImplementationOnce(async () => "upToDate");
  render(<UpdateHost />);
  await flush();
  act(() => { runAction("help.updates", { source: "menu" }); });
  await flush();
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  await flush();
  expect(useUpdate.getState().open).toBe(false);
  act(() => { runAction("help.updates", { source: "menu" }); });
  await flush();
  expect(useUpdate.getState().open).toBe(true);
  expect(screen.getByText("OpenSpider is up to date.")).toBeInTheDocument();
});

// spec: OUT-251
it("keeps polling after a status request got no answer (B3-UI)", async () => {
  const status = vi.spyOn(update, "status").mockImplementation(() => hanging());
  render(<UpdateHost />);
  await flush();
  const first = status.mock.calls.length;
  await flush(30000);
  expect(status).toHaveBeenCalledTimes(first + 1);
  await flush(STATUS_TIMEOUT_MS + 30000);
  expect(status.mock.calls.length).toBeGreaterThan(first + 1);
});

// spec: UI-002 [M3]
it("brings a minimized main window to the front before showing the update dialog (B3-UI)", async () => {
  native.tauri = true;
  vi.spyOn(update, "status").mockImplementation(async () => "idle");
  vi.spyOn(update, "check").mockImplementation(async () => "upToDate");
  render(<UpdateHost />);
  await flush();
  act(() => { runAction("help.updates", { source: "menu" }); });
  await flush();
  expect(native.win.unminimize).toHaveBeenCalled();
  expect(native.win.setFocus).toHaveBeenCalled();
  expect(screen.getByText("OpenSpider is up to date.")).toBeInTheDocument();
});

// spec: OUT-262
it("tells the user to download manually when the installer type is unknown, without offering a download", async () => {
  vi.spyOn(update, "status").mockImplementation(async () => "idle");
  vi.spyOn(update, "check").mockImplementation(async () => ({ disabled: "unknownInstaller" }));
  const download = vi.spyOn(update, "download");
  render(<UpdateHost />);
  await flush();
  act(() => { runAction("help.updates", { source: "menu" }); });
  await flush();
  expect(screen.getByText(/could not be determined whether OpenSpider was installed with the MSI package or the setup program/)).toHaveTextContent("https://github.com/wojcikdominik/openspider/releases/latest");
  expect(screen.queryByRole("button", { name: "Download in background" })).toBeNull();
  expect(download).not.toHaveBeenCalled();
  await setLanguage("de");
  await flush();
  expect(screen.getByText(/Bitte laden Sie neue Versionen manuell herunter/)).toHaveTextContent("https://github.com/wojcikdominik/openspider/releases/latest");
});

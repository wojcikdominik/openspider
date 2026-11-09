import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { setLanguage } from "../../i18n";
import { app } from "../../ipc/app";
import { settings } from "../../ipc/settings";
import { update } from "../../ipc/update";
import { useSession } from "../../state/session";
import tickFixture from "../../ipc/__fixtures__/Tick.json";
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

const shortNotes = "- **Faster** crawls\n- Skip versions\n- [Guide](https://openspider.de/en/manual/)\n\n### Deutsch\n\n- **Schnellere** Crawls\n- Versionen überspringen\n- [Handbuch](https://openspider.de/handbuch/)\n";
async function openRelease(state: UpdateState, lang = "en") {
  await setLanguage(lang);
  vi.spyOn(update, "status").mockImplementation(async () => state);
  vi.spyOn(app, "info").mockResolvedValue({ version: "1.0.0-beta.3", build: "abc", buildNumber: "2578", platform: "macos", arch: "aarch64", paths: {} as never });
  vi.spyOn(settings, "get").mockResolvedValue({ updates: { check: true, lastCheck: null } } as never);
  act(() => useUpdate.setState({ state, open: true, release: { version: "1.0.0-beta.4", notes: shortNotes, pubDate: null } }));
  render(<UpdateHost />);
  await flush();
}

// spec: OUT-251
it("shows head, installed version, the notes of the UI language and the actions of a ready update (Nutzerentscheidung 09.11.2026)", async () => {
  await openRelease({ ready: { version: "1.0.0-beta.4" } }, "de");
  const dialog = screen.getByRole("dialog", { name: "Eine neue Version von OpenSpider ist verfügbar!" });
  expect(dialog).toHaveTextContent("OpenSpider 1.0.0-beta.4 ist verfügbar – Sie haben 1.0.0-beta.3 (Build 2578). Möchten Sie die neue Version jetzt installieren?");
  const notes = dialog.querySelector("[data-update-notes]")!;
  expect(notes.querySelector("h3")).toHaveTextContent("OpenSpider 1.0.0-beta.4");
  expect(notes.querySelectorAll("li")).toHaveLength(3);
  expect(notes).toHaveTextContent("Schnellere Crawls");
  expect(notes).not.toHaveTextContent("Faster");
  expect(notes).not.toHaveTextContent("### Deutsch");
  expect(screen.getByRole("checkbox", { name: "Automatisch nach Updates suchen" })).toBeChecked();
  for (const name of ["Diese Version überspringen", "Später erinnern", "Beim Beenden installieren", "Update installieren"]) expect(screen.getByRole("button", { name })).toBeEnabled();
  // Initial focus on the primary action is checked in the browser (e2e/update/layout.spec.ts, UI-126).
});

// spec: OUT-251
it.each(["en", "es"])("uses the English notes part for the %s UI", async lang => {
  await openRelease({ ready: { version: "1.0.0-beta.4" } }, lang);
  const notes = screen.getByRole("dialog").querySelector("[data-update-notes]")!;
  expect(notes).toHaveTextContent("Faster crawls");
  expect(notes).not.toHaveTextContent("Schnellere");
  expect(screen.getByRole("button", { name: "Install update" })).toBeInTheDocument();
});

// spec: OUT-251
it("skip remembers the version in the settings, remind later only closes, and installs wait for the download", async () => {
  const set = vi.spyOn(settings, "set").mockResolvedValue({} as never);
  await openRelease({ downloading: { done: 1, total: 4 } });
  expect(screen.getByRole("button", { name: "Install update" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Install on quit" })).toBeDisabled();
  expect(screen.getByRole("progressbar", { name: "Download progress" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Remind me later" }));
  expect(useUpdate.getState().open).toBe(false);
  expect(set).not.toHaveBeenCalled();
  act(() => useUpdate.setState({ open: true }));
  await flush();
  fireEvent.click(screen.getByRole("button", { name: "Skip this version" }));
  await flush();
  expect(set).toHaveBeenCalledWith({ patch: { updates: { skippedVersion: "1.0.0-beta.4" } } });
  expect(useUpdate.getState().open).toBe(false);
});

// spec: OUT-251
// spec: OUT-250
it("the checkbox switches the automatic update check, and a running crawl only offers installation after it ends", async () => {
  const auto = vi.spyOn(update, "set_auto").mockResolvedValue(undefined as never);
  act(() => useSession.setState({ crawl: { ...tickFixture.crawl, phase: "running" } as never }));
  await openRelease({ ready: { version: "1.0.0-beta.4" } });
  fireEvent.click(screen.getByRole("checkbox", { name: "Automatically check for updates" }));
  expect(auto).toHaveBeenCalledWith({ check: false });
  expect(screen.getByRole("checkbox", { name: "Automatically check for updates" })).not.toBeChecked();
  expect(screen.getByRole("button", { name: "Install after crawl ends" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "Install update" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Install on quit" })).toBeNull();
  act(() => useSession.setState({ crawl: null } as never));
});

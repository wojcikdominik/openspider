import { afterEach, beforeEach, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createClient, setClient } from "../../../../ipc/client";
import { MockCore } from "../../../../ipc/transport/mock";
import { setLanguage } from "../../../../i18n";
import type { UpdateChannelState } from "../../../../ipc/gen/UpdateChannelState";
import { section } from "./index";

/** Gemockter Kern (UI-131 Testfixture): laufende Version, gespeicherter Kanal, Update-Status `environment`. */
function core({ version, stored = null, environment = false, failSave = false }: { version: string; stored?: string | null; environment?: boolean; failSave?: boolean }) {
  const c = new MockCore();
  c.settings.updates.channel = stored;
  const state = (): UpdateChannelState => {
    const raw = c.settings.updates.channel;
    const prerelease = version.includes("-");
    const channel = raw === "stable" || raw === "beta" ? raw : prerelease ? "beta" : "stable";
    return { channel, version, prerelease };
  };
  c.handlers.set("update.status", () => environment ? { disabled: "environment" } : "idle");
  c.handlers.set("update.channel", () => state());
  c.handlers.set("update.set_channel", (args) => {
    if (failSave) throw new Error("settings.toml not writable");
    c.settings.updates.channel = (args as { channel: string }).channel;
    return state();
  });
  c.handlers.set("update.set_auto", (args) => { c.settings.updates.check = (args as { check: boolean }).check; });
  setClient(createClient(c.transport("settings")));
  return c;
}
const Updates = section.Component;
const radio = (name: string) => screen.getByRole("radio", { name });
const requests = (c: MockCore) => c.calls.filter(call => call.cmd === "update.check" || call.cmd === "update.download");

beforeEach(async () => { await setLanguage("en"); });
afterEach(() => { cleanup(); setClient(null); });

// spec: UI-131
it("preselects the effective channel of a profile without key: stable on 1.0.1, beta on 1.1.0-beta.1", async () => {
  core({ version: "1.0.1" });
  render(<Updates />);
  expect(await screen.findByRole("radiogroup", { name: "Update channel" })).toBeInTheDocument();
  expect(radio("Stable (recommended)")).toBeChecked();
  expect(radio("Beta")).not.toBeChecked();
  cleanup();
  core({ version: "1.1.0-beta.1" });
  render(<Updates />);
  await waitFor(() => expect(radio("Beta")).toBeChecked());
  expect(screen.getByText("Beta versions bring new features earlier and may contain bugs. On the beta channel you also receive every newer stable version.")).toBeVisible();
});

// spec: UI-131
// spec: OUT-254
it("switching to beta stores updates.channel at once, sends no update request and shows the next-check hint until the page is left", async () => {
  const c = core({ version: "1.0.1" });
  render(<Updates />);
  await screen.findByRole("radiogroup", { name: "Update channel" });
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(radio("Beta")).toBeChecked());
  expect(c.calls.filter(call => call.cmd === "update.set_channel").map(call => call.args)).toEqual([{ channel: "beta" }]);
  expect(c.settings.updates.channel).toBe("beta");
  expect(requests(c)).toEqual([]);
  expect(screen.getByRole("status")).toHaveTextContent("From the next update check on, you will also receive beta versions.");
  // No confirmation dialog, no restart notice.
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  // "Restart": the stored value stays selected; the next-check hint belonged to the left page.
  cleanup();
  setClient(createClient(c.transport("settings")));
  render(<Updates />);
  await waitFor(() => expect(radio("Beta")).toBeChecked());
  expect(screen.getByRole("status")).toBeEmptyDOMElement();
});

// spec: UI-131
// spec: OUT-254
it("on 1.1.0-beta.3 switching to stable announces the stay-on hint as status, keeps it after restart and removes it on beta", async () => {
  const c = core({ version: "1.1.0-beta.3", stored: "beta" });
  render(<Updates />);
  await waitFor(() => expect(radio("Beta")).toBeChecked());
  expect(screen.getByRole("status")).toBeEmptyDOMElement();
  fireEvent.click(radio("Stable (recommended)"));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("You will stay on 1.1.0-beta.3 until a newer stable version is released."));
  expect(c.settings.updates.channel).toBe("stable");
  // No button or link to reset or reinstall an older version.
  expect(screen.queryByRole("link")).toBeNull();
  expect(screen.queryAllByRole("button")).toEqual([]);
  cleanup();
  setClient(createClient(c.transport("settings")));
  render(<Updates />);
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("You will stay on 1.1.0-beta.3 until a newer stable version is released."));
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(screen.getByRole("status")).not.toHaveTextContent("You will stay on"));
  expect(requests(c)).toEqual([]);
  cleanup();
  core({ version: "1.0.1", stored: "stable" });
  render(<Updates />);
  await waitFor(() => expect(radio("Stable (recommended)")).toBeChecked());
  expect(screen.getByRole("status")).toBeEmptyDOMElement();
});

// spec: UI-131
// spec: OUT-250
it("is disabled like the switch when the environment disables checks, but stays usable with automatic checks off", async () => {
  const c = core({ version: "1.0.1", stored: "beta", environment: true });
  render(<Updates />);
  await waitFor(() => expect(radio("Beta")).toBeDisabled());
  expect(radio("Stable (recommended)")).toBeDisabled();
  expect(screen.getByRole("switch", { name: "Automatically check for updates" })).toBeDisabled();
  expect(radio("Beta")).toBeChecked();
  fireEvent.click(radio("Stable (recommended)"));
  expect(c.calls.some(call => call.cmd === "update.set_channel")).toBe(false);
  expect(c.settings.updates.channel).toBe("beta");
  cleanup();
  const off = core({ version: "1.0.1" });
  off.settings.updates.check = false;
  render(<Updates />);
  await waitFor(() => expect(radio("Beta")).not.toBeDisabled());
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(off.settings.updates.channel).toBe("beta"));
  expect(off.settings.updates.check).toBe(false);
});

// spec: UI-131
it("a failing save shows the switch's alert and shows the stored value again", async () => {
  const c = core({ version: "1.0.1", stored: "stable", failSave: true });
  render(<Updates />);
  await waitFor(() => expect(radio("Stable (recommended)")).toBeChecked());
  fireEvent.click(radio("Beta"));
  expect(await screen.findByRole("alert")).toHaveTextContent("The update could not be completed. Please try again.");
  expect(radio("Stable (recommended)")).toBeChecked();
  expect(radio("Beta")).not.toBeChecked();
  expect(c.settings.updates.channel).toBe("stable");
});

// spec: UI-131
// spec: UI-126
it("keyboard: the group is reachable with Tab and arrow keys change the option", async () => {
  const c = core({ version: "1.0.1" });
  render(<Updates />);
  await screen.findByRole("radiogroup", { name: "Update channel" });
  const stable = radio("Stable (recommended)");
  act(() => { stable.focus(); });
  expect(stable).toHaveFocus();
  // Only the selected radio is in the tab order (roving tabindex), so Tab reaches the group exactly once.
  expect(radio("Beta").tabIndex).toBe(-1);
  fireEvent.keyDown(stable, { key: "ArrowRight" });
  await waitFor(() => expect(radio("Beta")).toBeChecked());
  await waitFor(() => expect(c.settings.updates.channel).toBe("beta"));
  expect(radio("Beta")).toHaveFocus();
});

// spec: UI-131
// spec: UI-121
it("shows all texts in German and falls back to English for other languages", async () => {
  await setLanguage("de");
  core({ version: "1.1.0-beta.3", stored: "stable" });
  render(<Updates />);
  expect(await screen.findByRole("radiogroup", { name: "Update-Kanal" })).toBeInTheDocument();
  expect(radio("Stabil (empfohlen)")).toBeChecked();
  expect(radio("Beta")).toBeInTheDocument();
  expect(screen.getByText("Beta-Versionen bringen neue Funktionen früher und können Fehler enthalten. Im Beta-Kanal erhalten Sie auch jede neuere stabile Version.")).toBeVisible();
  expect(screen.getByRole("status")).toHaveTextContent("Sie bleiben auf 1.1.0-beta.3, bis eine neuere stabile Version erscheint.");
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Ab der nächsten Update-Prüfung erhalten Sie auch Beta-Versionen."));
  cleanup();
  await setLanguage("fr");
  core({ version: "1.0.1" });
  render(<Updates />);
  expect(await screen.findByRole("radiogroup", { name: "Update channel" })).toBeInTheDocument();
  expect(radio("Stable (recommended)")).toBeChecked();
});

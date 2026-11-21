// Unabhängige Abnahme UI-131 / OUT-254 (Testagent; Grundlage allein die Spec, Bauer-Bericht nicht gelesen).
// acceptance: independent
//
// Gemockter IPC: laufende Version, Einstellungen (`updates.channel`), Update-Status `environment`. Der Mock wendet die
// Standardregel aus OUT-254 an (gespeicherter gültiger Wert, sonst Vorabversion → beta, sonst stable).
import { afterEach, beforeEach, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createClient, setClient } from "../../../../ipc/client";
import { MockCore, mergePatch } from "../../../../ipc/transport/mock";
import { setLanguage } from "../../../../i18n";
import { usePrefs } from "../../../../state/prefs";
import { section } from "./index";

type Env = { version?: string; stored?: string | null; environment?: boolean; check?: boolean; failChannel?: boolean; failAuto?: boolean };
let core: MockCore;

async function setup(env: Env = {}, lang = "en") {
  const version = env.version ?? "1.0.1";
  core = new MockCore();
  core.settings = mergePatch(core.settings, { updates: { channel: env.stored ?? null, check: env.check ?? true } }) as typeof core.settings;
  const state = () => {
    const raw = (core.settings.updates as { channel?: string | null }).channel;
    const prerelease = /^[^+]*-/.test(version);
    const channel = raw === "stable" || raw === "beta" ? raw : prerelease ? "beta" : "stable";
    return { channel, version: version.split("+")[0], prerelease };
  };
  core.handlers.set("update.status", () => (env.environment ? { disabled: "environment" } : "idle"));
  core.handlers.set("update.channel", () => state());
  core.handlers.set("update.set_channel", args => {
    if (env.failChannel) throw new Error("settings.toml not writable");
    core.settings = mergePatch(core.settings, { updates: { channel: (args as { channel: string }).channel } }) as typeof core.settings;
    return state();
  });
  core.handlers.set("update.set_auto", args => {
    if (env.failAuto) throw new Error("settings.toml not writable");
    core.settings = mergePatch(core.settings, { updates: { check: (args as { check: boolean }).check } }) as typeof core.settings;
    return null;
  });
  setClient(createClient(core.transport("settings")));
  await setLanguage(lang as never);
  await usePrefs.getState().load();
  const view = render(<section.Component />);
  await screen.findByRole("radiogroup");
  return view;
}

const group = () => screen.getByRole("radiogroup");
const radio = (name: string | RegExp) => within(group()).getByRole("radio", { name });
const hint = () => document.querySelector("p[role=status][data-update-channel-hint]");
const hintText = () => (hint()?.textContent ?? "").trim();
const storedChannel = () => (core.settings.updates as { channel?: string | null }).channel ?? null;
const calls = (cmd: string) => core.calls.filter(c => c.cmd === cmd);
const networkish = () => core.calls.filter(c => c.cmd === "update.check" || c.cmd === "update.download" || c.cmd === "update.install");

beforeEach(() => { localStorage.clear(); });
afterEach(() => { cleanup(); setClient(null); });

// spec: UI-131
// acceptance: independent
it("preselects the effective channel: no key on 1.0.1 → Stable, on 1.1.0-beta.1 → Beta", async () => {
  await setup({ version: "1.0.1" });
  expect(radio("Stable (recommended)")).toBeChecked();
  expect(radio("Beta")).not.toBeChecked();
  cleanup();
  await setup({ version: "1.1.0-beta.1" });
  expect(radio("Beta")).toBeChecked();
  expect(radio("Stable (recommended)")).not.toBeChecked();
});

// spec: UI-131
// spec: OUT-254
// acceptance: independent
it("preselects a stored value even when it does not match the running version", async () => {
  await setup({ version: "1.1.0-beta.3", stored: "stable" });
  expect(radio("Stable (recommended)")).toBeChecked();
  cleanup();
  await setup({ version: "1.0.1", stored: "beta" });
  expect(radio("Beta")).toBeChecked();
  cleanup();
  await setup({ version: "1.0.1", stored: "nightly" });
  expect(radio("Stable (recommended)")).toBeChecked();
});

// spec: UI-131
// acceptance: independent
it("labels the group and sits directly below the automatic-check switch, explanation always visible below the group", async () => {
  await setup();
  expect(group()).toHaveAccessibleName("Update channel");
  expect(group()).toHaveAttribute("data-setting-anchor", "updates.channel");
  expect(within(group()).getAllByRole("radio")).toHaveLength(2);
  const sw = screen.getByRole("switch", { name: "Automatically check for updates" });
  const explain = screen.getByText("Beta versions bring new features earlier and may contain bugs. On the beta channel you also receive every newer stable version.");
  // Reihenfolge im Dokument: Schalter → Gruppe → Erklärung, kein anderes Bedienelement dazwischen.
  expect(sw.compareDocumentPosition(group()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(group().compareDocumentPosition(explain) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  const all = Array.from(document.querySelectorAll("input, button, [role=switch], [role=radiogroup], select, textarea"));
  const between = all.filter(el => sw.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING && el.compareDocumentPosition(group()) & Node.DOCUMENT_POSITION_FOLLOWING && !sw.contains(el) && !el.contains(sw));
  expect(between).toHaveLength(0);
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(storedChannel()).toBe("beta"));
  expect(screen.getByText(/^Beta versions bring new features earlier/)).toBeVisible();
});

// spec: UI-131
// spec: OUT-254
// acceptance: independent
it("switching to Beta stores at once, without dialog and without any update request, and shows the next-check hint", async () => {
  await setup({ version: "1.0.1", stored: "stable" });
  expect(hintText()).toBe("");
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(radio("Beta")).toBeChecked());
  expect(storedChannel()).toBe("beta");
  expect(calls("update.set_channel").map(c => c.args)).toEqual([{ channel: "beta" }]);
  expect(networkish()).toHaveLength(0);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("alertdialog")).toBeNull();
  await waitFor(() => expect(hintText()).toBe("From the next update check on, you will also receive beta versions."));
  expect(screen.queryByText(/restart/i)).toBeNull();
});

// spec: UI-131
// acceptance: independent
it("the next-check hint disappears when the page is left", async () => {
  await setup({ version: "1.0.1" });
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(hintText()).toMatch(/^From the next update check on/));
  expect(storedChannel()).toBe("beta");
  cleanup();
  await setup({ version: "1.0.1", stored: "beta" });
  expect(radio("Beta")).toBeChecked();
  expect(hintText()).toBe("");
});

// spec: UI-131
// spec: OUT-254
// acceptance: independent
it("back to Stable on 1.1.0-beta.3 shows the stay hint as status, keeps it after restart, Beta removes it", async () => {
  await setup({ version: "1.1.0-beta.3+17" });
  expect(radio("Beta")).toBeChecked();
  fireEvent.click(radio("Stable (recommended)"));
  await waitFor(() => expect(hintText()).toBe("You will stay on 1.1.0-beta.3 until a newer stable version is released."));
  expect(hint()).toHaveAttribute("role", "status");
  expect(storedChannel()).toBe("stable");
  // Kein Knopf und kein Link zum Zurücksetzen oder Neuinstallieren.
  expect(screen.queryAllByRole("link")).toHaveLength(0);
  expect(screen.queryByRole("button", { name: /reinstall|downgrade|older|reset/i })).toBeNull();
  // „Neustart“: Seite neu aufbauen mit demselben gespeicherten Wert.
  cleanup();
  await setup({ version: "1.1.0-beta.3+17", stored: "stable" });
  expect(radio("Stable (recommended)")).toBeChecked();
  expect(hintText()).toBe("You will stay on 1.1.0-beta.3 until a newer stable version is released.");
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(hintText()).not.toMatch(/You will stay on/));
});

// spec: UI-131
// acceptance: independent
it("shows no stay hint for Stable on a stable version", async () => {
  await setup({ version: "1.0.1", stored: "stable" });
  expect(hintText()).toBe("");
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(storedChannel()).toBe("beta"));
  fireEvent.click(radio("Stable (recommended)"));
  await waitFor(() => expect(storedChannel()).toBe("stable"));
  expect(hintText()).not.toMatch(/You will stay on/);
});

// spec: UI-131
// spec: OUT-250
// acceptance: independent
it("environment lock greys out the group and keeps the stored value", async () => {
  await setup({ version: "1.0.1", stored: "beta", environment: true });
  await waitFor(() => expect(radio("Beta")).toBeDisabled());
  expect(radio("Stable (recommended)")).toBeDisabled();
  expect(screen.getByRole("switch", { name: "Automatically check for updates" })).toBeDisabled();
  expect(radio("Beta")).toBeChecked();
  fireEvent.click(radio("Stable (recommended)"));
  await new Promise(r => setTimeout(r, 50));
  expect(calls("update.set_channel")).toHaveLength(0);
  expect(storedChannel()).toBe("beta");
});

// spec: UI-131
// acceptance: independent
it("automatic checks off leaves the group usable", async () => {
  await setup({ version: "1.0.1", stored: "stable", check: false });
  expect(screen.getByRole("switch", { name: "Automatically check for updates" })).not.toBeChecked();
  expect(radio("Beta")).toBeEnabled();
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(storedChannel()).toBe("beta"));
  expect((core.settings.updates as { check: boolean }).check).toBe(false);
});

// spec: UI-131
// acceptance: independent
it("turning automatic checks off on the page keeps the group usable", async () => {
  await setup({ version: "1.0.1" });
  fireEvent.click(screen.getByRole("switch", { name: "Automatically check for updates" }));
  await waitFor(() => expect((core.settings.updates as { check: boolean }).check).toBe(false));
  expect(radio("Beta")).toBeEnabled();
  expect(radio("Stable (recommended)")).toBeEnabled();
});

// spec: UI-131
// acceptance: independent
it("save failure shows the same alert as the switch and reselects the stored value", async () => {
  // Referenz: Meldung beim Fehlschlag des Schalters.
  await setup({ version: "1.0.1", failAuto: true });
  fireEvent.click(screen.getByRole("switch", { name: "Automatically check for updates" }));
  const switchAlert = (await screen.findByRole("alert")).textContent;
  expect(switchAlert).toBeTruthy();
  cleanup();

  await setup({ version: "1.0.1", stored: "stable", failChannel: true });
  fireEvent.click(radio("Beta"));
  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toBe(switchAlert);
  await waitFor(() => expect(radio("Stable (recommended)")).toBeChecked());
  expect(radio("Beta")).not.toBeChecked();
  expect(storedChannel()).toBe("stable");
  expect(hintText()).not.toMatch(/From the next update check on/);
});

// spec: UI-131
// spec: UI-121
// acceptance: independent
it("German texts", async () => {
  await setup({ version: "1.1.0-beta.3" }, "de");
  expect(group()).toHaveAccessibleName("Update-Kanal");
  expect(radio("Stabil (empfohlen)")).toBeInTheDocument();
  expect(radio("Beta")).toBeChecked();
  expect(screen.getByText("Beta-Versionen bringen neue Funktionen früher und können Fehler enthalten. Im Beta-Kanal erhalten Sie auch jede neuere stabile Version.")).toBeInTheDocument();
  fireEvent.click(radio("Stabil (empfohlen)"));
  await waitFor(() => expect(hintText()).toBe("Sie bleiben auf 1.1.0-beta.3, bis eine neuere stabile Version erscheint."));
  fireEvent.click(radio("Beta"));
  await waitFor(() => expect(hintText()).toBe("Ab der nächsten Update-Prüfung erhalten Sie auch Beta-Versionen."));
});

// spec: UI-131
// spec: UI-121
// acceptance: independent
it("other interface languages fall back to English", async () => {
  for (const lang of ["fr", "es", "it"]) {
    await setup({ version: "1.0.1" }, lang);
    expect(group()).toHaveAccessibleName("Update channel");
    expect(radio("Stable (recommended)")).toBeChecked();
    expect(screen.getByText(/^Beta versions bring new features earlier and may contain bugs\./)).toBeInTheDocument();
    cleanup();
  }
});

// spec: UI-131
// acceptance: independent
it("arrow keys move the selection and store it", async () => {
  await setup({ version: "1.0.1", stored: "stable" });
  const stable = radio("Stable (recommended)");
  act(() => stable.focus());
  fireEvent.keyDown(stable, { key: "ArrowRight" });
  await waitFor(() => expect(storedChannel()).toBe("beta"));
  expect(radio("Beta")).toBeChecked();
  expect(networkish()).toHaveLength(0);
});

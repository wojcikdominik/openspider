// Unabhängige Abnahme B4-UPDDLG (Testagent, nur aus der Spec abgeleitet; Bauer-Bericht nicht gelesen).
// Quellen: docs/spec/06-output-ops.md OUT-250/OUT-251/OUT-257/OUT-181, docs/spec/05-ui.md UI-127,
// docs/release/release-rhythmus.md. Der Produktcode wird nur zum Aufrufen benutzt (Einhängepunkt: UpdateHost, IPC-Kommandos
// update.* und settings.*, Aktion help.updates). Layout und echte Eingaben: e2e/acceptance/b4-upddlg.spec.ts.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setLanguage } from "../../i18n";
import { createClient, setClient } from "../../ipc/client";
import { MockCore } from "../../ipc/transport/mock";
import tickFixture from "../../ipc/__fixtures__/Tick.json";
import type { UpdateState } from "../../ipc/gen/UpdateState";
import { useSession } from "../../state/session";
import { runAction } from "../../ui/actions";
import { UpdateHost } from "./UpdateHost";
import { useUpdate } from "./state";

const NEW = "1.0.0-beta.4";
const OLD = "1.0.0-beta.3";
// Kurze Update-Notes im Manifest-Format (OUT-251): englische Liste, `### Deutsch`, deutsche Liste.
const NOTES = [
  "- **Faster** large crawls",
  "- Skip a version in the [update dialog](https://openspider.test/en/manual/)",
  "- Raw <u>html</u> and <script>window.__pwned = 1</script> stay inert",
  "- Bad [script link](javascript:window.__pwned2=1) is no link",
  "",
  "### Deutsch",
  "",
  "- **Schnellere** große Crawls",
  "- Eine Version im [Update-Dialog](https://openspider.test/handbuch/) überspringen",
  "- Rohes <u>HTML</u> und <script>window.__pwned = 1</script> bleibt wirkungslos",
  "- Böser [Skript-Link](javascript:window.__pwned2=1) ist kein Link",
].join("\n");

let core: MockCore;
let status: UpdateState;
let download: UpdateState;
beforeEach(async () => {
  core = new MockCore();
  status = "idle";
  download = { ready: { version: NEW } };
  core.handlers.set("update.status", () => status);
  core.handlers.set("update.check", () => status);
  core.handlers.set("update.download", () => download);
  core.handlers.set("update.install", () => download);
  core.handlers.set("update.set_auto", (args) => { core.settings.updates.check = (args as { check: boolean }).check; });
  core.handlers.set("app.info", () => ({ version: OLD, build: "abc12345", buildNumber: "2578", platform: "macos", arch: "aarch64", paths: {} }));
  setClient(createClient(core.transport()));
  useUpdate.setState({ state: "idle", release: null, open: false, queued: null });
  useSession.setState({ crawl: null });
  (window as unknown as Record<string, unknown>).__pwned = undefined;
  (window as unknown as Record<string, unknown>).__pwned2 = undefined;
  await setLanguage("en");
});
afterEach(async () => { cleanup(); setClient(null); vi.restoreAllMocks(); await setLanguage("en"); });

const TITLE = { de: "Eine neue Version von OpenSpider ist verfügbar!", en: "A new version of OpenSpider is available!" } as const;
const BUTTONS = {
  de: { skip: "Diese Version überspringen", later: "Später erinnern", quit: "Beim Beenden installieren", now: "Update installieren", crawl: "Nach Crawl-Ende installieren", auto: "Automatisch nach Updates suchen" },
  en: { skip: "Skip this version", later: "Remind me later", quit: "Install on quit", now: "Install update", crawl: "Install after crawl ends", auto: "Automatically check for updates" },
} as const;

/** Der Kern meldet beim Start ein verfügbares Update (Hintergrundprüfung); der Hintergrund-Download folgt ohne Zutun. */
async function announce(lang: "de" | "en" | "es" = "en", notes = NOTES) {
  await setLanguage(lang);
  status = { available: { version: NEW, notes, pubDate: null } };
  render(<UpdateHost />);
  const dialog = await screen.findByRole("dialog", { name: TITLE[lang === "de" ? "de" : "en"] });
  return dialog;
}
const names = (lang: string) => BUTTONS[lang === "de" ? "de" : "en"];

describe("Notes im Dialog (OUT-251, Regel wie OUT-257)", () => {
  // spec: OUT-251
  it("Oberflächensprache Deutsch zeigt nur den deutschen Teil, ohne Rohüberschrift und ohne Marker", async () => {
    const dialog = await announce("de");
    const text = dialog.textContent ?? "";
    expect(text).toContain("Schnellere");
    expect(text).not.toContain("Faster");
    expect(text).not.toContain("###");
    expect(text).not.toContain("Deutsch\n");
    expect(within(dialog).queryByRole("heading", { name: "Deutsch" })).toBeNull();
    // Überschrift des Notes-Kastens: „OpenSpider <Version>“.
    expect(within(dialog).getByRole("heading", { name: `OpenSpider ${NEW}` })).toBeVisible();
  });

  // spec: OUT-251
  it.each(["en", "es"] as const)("Oberflächensprache %s zeigt nur den englischen Teil", async (lang) => {
    const dialog = await announce(lang);
    const text = dialog.textContent ?? "";
    expect(text).toContain("Faster");
    expect(text).not.toContain("Schnellere");
    expect(text).not.toContain("###");
    expect(text).not.toMatch(/Deutsch/);
  });

  // spec: OUT-251
  it("Listen und Fettdruck werden gerendert, nicht als Markdown-Zeichen angezeigt", async () => {
    const dialog = await announce("en");
    const items = dialog.querySelectorAll("li");
    expect(items.length).toBe(4);
    const strong = [...dialog.querySelectorAll("strong")].map((s) => s.textContent);
    expect(strong).toContain("Faster");
    expect(dialog.textContent).not.toContain("**");
    expect(dialog.textContent).not.toMatch(/^\s*- /m);
  });

  // spec: OUT-251
  it("eingebettetes HTML wird nicht ausgeführt und nicht als Element eingehängt", async () => {
    const dialog = await announce("en");
    expect(dialog.querySelector("script")).toBeNull();
    expect(dialog.querySelector("u")).toBeNull();
    expect((window as unknown as Record<string, unknown>).__pwned).toBeUndefined();
    expect(dialog.querySelector('a[href^="javascript:" i]')).toBeNull();
    expect((window as unknown as Record<string, unknown>).__pwned2).toBeUndefined();
  });

  // spec: OUT-251
  it("Links öffnen im Standardbrowser und navigieren nie das App-Fenster", async () => {
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const dialog = await announce("en");
    const link = within(dialog).getByRole("link", { name: "update dialog" });
    expect(link).toHaveAttribute("href", "https://openspider.test/en/manual/");
    // fireEvent.click liefert false, wenn der Standardvorgang (Navigation) verhindert wurde.
    expect(fireEvent.click(link)).toBe(false);
    // Im Browser-Transport öffnet der Link einen neuen Tab, in der App (Tauri) den Standardbrowser über `browser_open`.
    const openedExternally = open.mock.calls.some((call) => call[0] === "https://openspider.test/en/manual/")
      || core.calls.some((call) => call.cmd === "browser_open");
    expect(openedExternally).toBe(true);
    expect(screen.getByRole("dialog")).toBeVisible();
  });

  // spec: OUT-251
  it("Deutsch: der Link führt auf die deutsche Seite", async () => {
    const dialog = await announce("de");
    expect(within(dialog).getByRole("link", { name: "Update-Dialog" })).toHaveAttribute("href", "https://openspider.test/handbuch/");
  });
});

describe("Kopf, Reihenfolge und Zustände (OUT-251, Nutzerentscheidung 09.11.2026)", () => {
  // spec: OUT-251
  it("Kopf nennt neue und installierte Version; die Build-Nummer steht nur bei der installierten (UI-127)", async () => {
    const dialog = await announce("de");
    expect(dialog).toHaveTextContent(`OpenSpider ${NEW} ist verfügbar – Sie haben ${OLD} (Build 2578). Möchten Sie die neue Version jetzt installieren?`);
    expect(dialog.textContent).not.toContain(`${NEW} (Build`);
  });

  // spec: OUT-251
  // spec: UI-127
  it("Entwickler-Build ohne Build-Nummer: nur die Version, kein „Build“, kein „null“", async () => {
    core.handlers.set("app.info", () => ({ version: OLD, build: "dev", platform: "macos", arch: "aarch64", paths: {} }));
    const dialog = await announce("de");
    expect(dialog).toHaveTextContent(`Sie haben ${OLD}.`);
    expect(dialog.textContent).not.toMatch(/Build|null|undefined/);
    cleanup();
    core.handlers.set("app.info", () => ({ version: OLD, build: "dev", buildNumber: null, platform: "macos", arch: "aarch64", paths: {} }));
    useUpdate.setState({ state: "idle", release: null, open: false, queued: null });
    const again = await announce("de");
    expect(again.textContent).not.toMatch(/Build|null|undefined/);
  });

  // spec: OUT-251
  it("zeigt vier Knöpfe plus Kontrollkästchen; Installationsknöpfe werden aktiv, sobald der Download fertig ist", async () => {
    download = { downloading: { done: 10, total: 100 } };
    await setLanguage("de");
    status = { available: { version: NEW, notes: NOTES, pubDate: null } };
    render(<UpdateHost />);
    const dialog = await screen.findByRole("dialog", { name: TITLE.de });
    const n = names("de");
    await waitFor(() => expect(within(dialog).getByRole("progressbar")).toBeVisible());
    expect(within(dialog).getByRole("button", { name: n.now })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: n.quit })).toBeDisabled();
    // Überspringen und Später bleiben während des Downloads bedienbar.
    expect(within(dialog).getByRole("button", { name: n.skip })).toBeEnabled();
    expect(within(dialog).getByRole("button", { name: n.later })).toBeEnabled();
    expect(within(dialog).getByRole("checkbox", { name: n.auto })).toBeVisible();
    // Download fertig → die nächste Statusabfrage aktiviert beide Knöpfe.
    status = { ready: { version: NEW } };
    download = status;
    act(() => { window.dispatchEvent(new Event("focus")); });
    await waitFor(() => expect(within(dialog).getByRole("button", { name: n.now })).toBeEnabled(), { timeout: 5000 });
    expect(within(dialog).getByRole("button", { name: n.quit })).toBeEnabled();
    expect(within(dialog).queryByRole("progressbar")).toBeNull();
  });

  // spec: OUT-251
  it.each(["de", "en"] as const)("%s: bei laufendem Crawl ersetzt „Nach Crawl-Ende installieren“ beide Installationsknöpfe", async (lang) => {
    useSession.setState({ crawl: tickFixture.crawl as never });
    const dialog = await announce(lang);
    const n = names(lang);
    await waitFor(() => expect(within(dialog).getByRole("button", { name: n.crawl })).toBeEnabled());
    expect(within(dialog).queryByRole("button", { name: n.now })).toBeNull();
    expect(within(dialog).queryByRole("button", { name: n.quit })).toBeNull();
    fireEvent.click(within(dialog).getByRole("button", { name: n.crawl }));
    await waitFor(() => expect(core.calls.find((c) => c.cmd === "update.install")?.args).toEqual({ when: "afterCrawl" }));
  });

  // spec: OUT-251
  it("„Beim Beenden installieren“ und „Update installieren“ lösen die richtigen Kommandos aus", async () => {
    const dialog = await announce("de");
    const n = names("de");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: n.quit })).toBeEnabled());
    fireEvent.click(within(dialog).getByRole("button", { name: n.quit }));
    await waitFor(() => expect(core.calls.filter((c) => c.cmd === "update.install").map((c) => c.args)).toEqual([{ when: "onQuit" }]));
    // Nach dem Vormerken ist nichts mehr doppelt installierbar.
    cleanup();
    core.calls.length = 0;
    useUpdate.setState({ state: "idle", release: null, open: false, queued: null });
    const second = await announce("de");
    await waitFor(() => expect(within(second).getByRole("button", { name: n.now })).toBeEnabled());
    fireEvent.click(within(second).getByRole("button", { name: n.now }));
    await waitFor(() => expect(core.calls.find((c) => c.cmd === "update.install")?.args).toEqual({ when: "now" }));
  });
});

describe("Überspringen und Später erinnern (OUT-251, OUT-250)", () => {
  // spec: OUT-251
  it("„Diese Version überspringen“ speichert die Version in den App-Einstellungen und schließt", async () => {
    const dialog = await announce("de");
    const n = names("de");
    fireEvent.click(within(dialog).getByRole("button", { name: n.skip }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const settings = core.settings.updates as unknown as Record<string, unknown>;
    const stored = settings.skippedVersion ?? settings.skipped_version;
    expect(stored).toBe(NEW);
    // Nichts wird installiert.
    expect(core.calls.some((c) => c.cmd === "update.install")).toBe(false);
  });

  // spec: OUT-251
  it("die manuelle Prüfung zeigt die übersprungene Version trotzdem (Hilfe → Nach Updates suchen)", async () => {
    (core.settings.updates as unknown as Record<string, unknown>).skippedVersion = NEW;
    await setLanguage("de");
    status = "idle";
    render(<UpdateHost />);
    await act(async () => {});
    core.handlers.set("update.check", () => ({ available: { version: NEW, notes: NOTES, pubDate: null } }));
    act(() => { runAction("help.updates", { source: "menu" }); });
    const dialog = await screen.findByRole("dialog", { name: TITLE.de });
    expect(dialog).toHaveTextContent(`OpenSpider ${NEW} ist verfügbar`);
    expect(core.calls.some((c) => c.cmd === "update.check")).toBe(true);
  });

  // spec: OUT-251
  it("„Später erinnern“ schließt nur: keine Einstellung, keine Installation", async () => {
    const dialog = await announce("en");
    const before = structuredClone(core.settings);
    fireEvent.click(within(dialog).getByRole("button", { name: names("en").later }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(core.settings).toEqual(before);
    expect(core.calls.some((c) => c.cmd === "update.install" || c.cmd === "settings.set")).toBe(false);
  });

  // spec: OUT-251
  it("Esc schließt wie „Später erinnern“ (ohne Nebenwirkung)", async () => {
    const dialog = await announce("en");
    const before = structuredClone(core.settings);
    fireEvent.keyDown(dialog, { key: "Escape" });
    fireEvent.keyUp(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(core.settings).toEqual(before);
    expect(core.calls.some((c) => c.cmd === "update.install" || c.cmd === "settings.set")).toBe(false);
  });
});

describe("Kontrollkästchen „Automatisch nach Updates suchen“ (OUT-250, OUT-251)", () => {
  // spec: OUT-251
  // spec: OUT-250
  it("zeigt den Wert der vorhandenen Einstellung (an → angehakt)", async () => {
    core.settings.updates.check = true;
    const dialog = await announce("de");
    await waitFor(() => expect(within(dialog).getByRole("checkbox", { name: names("de").auto })).toBeChecked());
  });

  // spec: OUT-251
  // spec: OUT-250
  it("zeigt den Wert der vorhandenen Einstellung (aus → nicht angehakt)", async () => {
    core.settings.updates.check = false;
    const dialog = await announce("de");
    const box = within(dialog).getByRole("checkbox", { name: names("de").auto });
    await waitFor(() => expect(box).not.toBeChecked());
    expect(box).toBeEnabled();
  });

  // spec: OUT-251
  // spec: OUT-250
  it("Umschalten schreibt in die Einstellung und bleibt beim erneuten Öffnen erhalten", async () => {
    core.settings.updates.check = true;
    const dialog = await announce("en");
    const box = within(dialog).getByRole("checkbox", { name: names("en").auto });
    await waitFor(() => expect(box).toBeChecked());
    fireEvent.click(box);
    await waitFor(() => expect(core.calls.find((c) => c.cmd === "update.set_auto")?.args).toEqual({ check: false }));
    expect(core.settings.updates.check).toBe(false);
    expect(box).not.toBeChecked();
    // Dialog schließen und neu öffnen: der gespeicherte Wert gilt.
    fireEvent.click(within(dialog).getByRole("button", { name: names("en").later }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    act(() => useUpdate.setState({ open: true }));
    const again = await screen.findByRole("dialog", { name: TITLE.en });
    await waitFor(() => expect(within(again).getByRole("checkbox", { name: names("en").auto })).not.toBeChecked());
  });

  // spec: OUT-251
  // spec: OUT-250
  it("es gibt keine Einstellung zum automatischen Installieren", async () => {
    const dialog = await announce("en");
    expect(within(dialog).getAllByRole("checkbox")).toHaveLength(1);
    expect(dialog.textContent?.toLowerCase()).not.toContain("automatically install");
  });
});

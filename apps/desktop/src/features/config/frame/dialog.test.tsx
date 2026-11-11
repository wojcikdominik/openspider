// Rahmen und Seiten des Konfigurationsdialogs (M2-U13) gegen einen Mock-Kern, dessen Werkseinstellungen aus dem echten Schema
// stammen. Das Verhalten gegen den echten Kern prüft e2e/config/bridge.spec.ts.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import type { CrawlStatus } from "../../../ipc/gen/CrawlStatus";
import { MockCore } from "../../../ipc/transport/mock";
import { useSession } from "../../../state/session";
import { hasAction, runAction } from "../../../ui/actions";
import { mainMenuContributions } from "../../../ui/menu/contributions/main";
import { CONFIG_SECTION_ANCHORS, ConfigHost } from "./ConfigHost";
import { pageForAnchor } from "./model";
import { resolvePages } from "./registry";
import { closeConfig, openConfig, useConfigDialog } from "./store";
import { installConfigCore, withConfig, type FakeConfigCore } from "./testing";
import type { ConfigJson } from "./model";

let core: MockCore;
let fake: FakeConfigCore;

const status = (phase: CrawlStatus["phase"], controllable = true): CrawlStatus =>
  ({ crawlId: "c1", phase, mode: "spider", crawled: 10, queued: 5, discovered: 15, percent: 0, rate: 0, avgRate: 0, elapsedS: 1, pausedS: 0, activeThreads: 1, owner: { kind: "app", live: true, controllable } }) as unknown as CrawlStatus;

async function setup(initial?: ConfigJson, mode: "free" | "running" | "paused" | "foreign" = "free") {
  core = new MockCore();
  fake = installConfigCore(core, initial);
  fake.mode = mode === "foreign" ? "running" : mode;
  setClient(createClient(core.transport("main")));
  await setLanguage("en");
  useSession.setState({ crawl: mode === "free" ? null : status(mode === "running" || mode === "foreign" ? "running" : "paused", mode !== "foreign") });
  render(<ConfigHost />);
}

async function open(anchor?: string) {
  act(() => openConfig(anchor ? { anchor } : {}));
  return screen.findByRole("dialog", { name: "Crawl configuration" });
}

const goPage = async (dialog: HTMLElement, name: string) => {
  fireEvent.click(within(dialog).getByRole("button", { name }));
  await waitFor(() => expect(within(dialog).getByRole("button", { name })).toHaveAttribute("aria-current", "page"));
};
const field = (dialog: HTMLElement, label: string) => within(dialog).getByRole("textbox", { name: label }) as HTMLInputElement;
const type = (el: HTMLElement, value: string) => fireEvent.change(el, { target: { value } });

afterEach(() => {
  act(() => closeConfig());
  cleanup();
  useSession.setState({ crawl: null });
});

describe("Rahmen: Navigation, Suche, Entwurf", () => {
  beforeEach(() => setup());

  // spec: OUT-220
  it("zeigt die Seitennavigation mit der Gruppe Spider und den Seiten der Spec; fehlende Pakete sind Platzhalter", async () => {
    const dialog = await open();
    const nav = within(dialog).getByRole("navigation", { name: "Configuration pages" });
    for (const name of ["Crawl", "Extraction", "Limits", "Rendering", "Advanced", "Presets", "robots.txt", "URL rewriting", "Include / Exclude", "Speed", "User-Agent", "HTTP headers", "Authentication", "Profiles"]) {
      expect(within(nav).getByRole("button", { name }), name).toBeInTheDocument();
    }
    expect(within(nav).getByRole("button", { name: "Segments" })).not.toHaveAttribute("data-pending");
    fireEvent.click(within(nav).getByRole("button", { name: "Segments" }));
    expect(await within(dialog).findByRole("button", { name: "Add segment" })).toBeInTheDocument();
  });

  // spec: OUT-220
  it("Suche findet Einstellungen nach Beschriftung und Pfad und springt zum Feld", async () => {
    const dialog = await open();
    const search = within(dialog).getByRole("searchbox", { name: "Search settings" });
    fireEvent.change(search, { target: { value: "threads" } });
    const results = within(dialog).getByRole("list", { name: "Search results" });
    const hit = within(results).getByRole("button", { name: /Max\. threads/ });
    fireEvent.click(hit);
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Speed" })).toHaveAttribute("aria-current", "page"));
    await waitFor(() => expect(document.activeElement).toBe(field(dialog, "Max. threads")));
    fireEvent.change(search, { target: { value: "speed.max_urls_per_sec" } });
    expect(within(dialog).getByRole("button", { name: /Max\. URLs per second/ })).toBeInTheDocument();
    fireEvent.change(search, { target: { value: "xyzzy-unknown" } });
    expect(within(dialog).getByText("No setting found.")).toBeInTheDocument();
  });

  // spec: OUT-220
  it("Validierung läuft ausschließlich im Kern: der Fehler je Pfad erscheint am Feld", async () => {
    const dialog = await open();
    await goPage(dialog, "Limits");
    type(field(dialog, "Max. URLs"), "0");
    await waitFor(() => expect(within(dialog).getByText(/muss mindestens 1 sein/)).toBeInTheDocument());
    expect(core.calls.some((c) => c.cmd === "config.validate")).toBe(true);
    expect(within(dialog).getByRole("textbox", { name: "Max. URLs" })).toHaveAttribute("aria-invalid", "true");
    type(field(dialog, "Max. URLs"), "100");
    await waitFor(() => expect(within(dialog).queryByText(/muss mindestens 1 sein/)).toBeNull());
  });

  // spec: OUT-220
  it("OK schreibt den Entwurf mit config.set, Abbrechen verwirft ihn", async () => {
    let dialog = await open();
    await goPage(dialog, "Limits");
    type(field(dialog, "Max. URLs"), "1234");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(fake.setCalls).toHaveLength(0);
    expect(core.calls.some((c) => c.cmd === "config.set")).toBe(false);

    dialog = await open();
    await goPage(dialog, "Limits");
    expect(field(dialog, "Max. URLs").value).toBe("5000000");
    type(field(dialog, "Max. URLs"), "1234");
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(fake.setCalls).toHaveLength(1);
    expect((fake.config.limits as Record<string, unknown>).max_urls).toBe(1234);
  });

  // spec: OUT-220
  it("optionale Werte: „Unbegrenzt“ setzt null, ein Wert hebt es auf", async () => {
    const dialog = await open();
    await goPage(dialog, "Limits");
    const depth = field(dialog, "Max. crawl depth");
    expect(depth).toBeDisabled();
    const unlimited = within(depth.closest(".os-cfg-number") as HTMLElement).getByRole("checkbox", { name: "Unlimited" });
    expect(unlimited).toBeChecked();
    fireEvent.click(unlimited);
    await waitFor(() => expect(field(dialog, "Max. crawl depth")).toBeEnabled());
    type(field(dialog, "Max. crawl depth"), "7");
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(fake.setCalls).toHaveLength(1));
    expect((fake.config.limits as Record<string, unknown>).max_depth).toBe(7);
  });

  // spec: OUT-220
  it("Zahlenfelder lehnen Unsinn schon lokal ab: Buchstaben, negative Werte bei vorzeichenlosen Feldern, Werte über dem Typ", async () => {
    const dialog = await open("limits.max_depth");
    fireEvent.click(within(field(dialog, "Max. crawl depth").closest(".os-cfg-number") as HTMLElement).getByRole("checkbox", { name: "Unlimited" }));
    const depth = field(dialog, "Max. crawl depth");
    type(depth, "abc");
    expect(within(dialog).getByText("Enter a whole number of 0 or more.")).toBeInTheDocument();
    type(depth, "-3");
    expect(within(dialog).getByText("Enter a whole number of 0 or more.")).toBeInTheDocument();
    type(depth, "99999999999");
    expect(within(dialog).getByText(/Allowed values: 0 to 4,294,967,295|Allowed values: 0 to 4294967295/)).toBeInTheDocument();
    type(depth, "12");
    expect(within(field(dialog, "Max. crawl depth").closest(".os-cfg-number") as HTMLElement).queryByText(/whole number|4,294,967,295|4294967295/)).toBeNull();
    // Die ungültigen Eingaben haben nie den Entwurf erreicht.
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect((fake.config.limits as Record<string, unknown>).max_depth).toBe(12));
  });

  // spec: OUT-220
  it("Sprungziel openConfig({anchor}) öffnet die Seite und fokussiert das Ziel-Steuerelement (UI-016)", async () => {
    const dialog = await open("rendering.mode");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Rendering" })).toHaveAttribute("aria-current", "page"));
    await waitFor(() => expect(document.activeElement?.closest('[data-setting-anchor="rendering.mode"]')).not.toBeNull());
    act(() => closeConfig());
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const again = await open("extraction.store_html");
    await waitFor(() => expect(within(again).getByRole("button", { name: "Extraction" })).toHaveAttribute("aria-current", "page"));
    await waitFor(() => expect(document.activeElement?.closest('[data-setting-anchor="extraction.store_html"]')).not.toBeNull());
  });

  // spec: OUT-220
  it("die Aktion config.spider (Mod+K) öffnet den Dialog; Abweichungen zeigen nur geänderte Werte", async () => {
    act(() => {
      runAction("config.spider");
    });
    const dialog = await screen.findByRole("dialog", { name: "Crawl configuration" });
    await waitFor(() => expect(within(dialog).getByTestId("config-deviations")).toHaveTextContent("Show deviations (0)"));
    await goPage(dialog, "Speed");
    type(field(dialog, "Max. threads"), "8");
    await waitFor(() => expect(within(dialog).getByTestId("config-deviations")).toHaveTextContent("Show deviations (1)"));
    fireEvent.click(within(dialog).getByTestId("config-deviations"));
    const table = await within(dialog).findByTestId("config-diff-table");
    expect(within(table).getByText("speed.threads")).toBeInTheDocument();
    expect(within(table).getAllByRole("row")[1]).toHaveTextContent(/8.*5/);
  });
});

describe("Aktionen und Menü", () => {
  beforeEach(() => setup());

  // spec: OUT-220
  it("config.profiles öffnet die Profilseite, config.diff die Abweichungen; der Menübeitrag nennt alle drei Aktionen", async () => {
    act(() => {
      runAction("config.profiles");
    });
    const dialog = await screen.findByRole("dialog", { name: "Crawl configuration" });
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Profiles" })).toHaveAttribute("aria-current", "page"));
    act(() => closeConfig());
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    act(() => {
      runAction("config.diff");
    });
    const again = await screen.findByRole("dialog", { name: "Crawl configuration" });
    expect(await within(again).findByTestId("config-diff-empty")).toBeInTheDocument();
    const { allMenuContributions } = await import("../../../ui/menu/registry");
    const items = allMenuContributions().filter((c) => c.menu === "config").flatMap((c) => c.items.map((i) => i.action));
    expect(items).toEqual(expect.arrayContaining(["config.spider", "config.profiles", "config.diff"]));
  });

  // spec: UI-002
  // spec: UI-016
  it("jeder Eintrag des Menüs „Konfiguration“ hat eine Aktion und öffnet den Dialog auf seiner Seite", async () => {
    const menu = mainMenuContributions({ recent: [], mode: "spider", mcpRunning: false, mcpAutostart: false })
      .filter((c) => c.menu === "config").flatMap((c) => c.items).flatMap((i) => [i, ...(i.items ?? [])])
      .map((i) => i.action).filter((id): id is string => !!id && id.startsWith("config."));
    expect(menu).toEqual(expect.arrayContaining(["config.content", "config.robots", "config.includeExclude", "config.api", "config.analysis", "config.search", "config.js"]));
    for (const id of menu) expect(hasAction(id), id).toBe(true);
    const pages = resolvePages().map((p) => ({ id: p.id, anchors: [...p.anchors, ...p.fields.map((f) => f.path)] }));
    const expected: Record<string, string> = {
      "config.content": "spider-presets", "config.robots": "robots", "config.rewrite": "rewrite", "config.includeExclude": "include-exclude",
      "config.speed": "speed", "config.userAgent": "user-agent", "config.headers": "headers", "config.api": "api", "config.segments": "segments",
      "config.analysis": "analysis", "config.search": "custom-search", "config.extraction": "custom-extraction", "config.js": "custom-js",
    };
    expect(Object.keys(CONFIG_SECTION_ANCHORS).sort()).toEqual(Object.keys(expected).sort());
    for (const [id, page] of Object.entries(expected)) {
      act(() => closeConfig());
      act(() => { runAction(id); });
      const { open: isOpen, anchor } = useConfigDialog.getState();
      expect(isOpen, id).toBe(true);
      expect(pageForAnchor(pages, anchor!)?.id, id).toBe(page);
    }
    act(() => closeConfig());
    act(() => { runAction("config.speed"); });
    const dialog = await screen.findByRole("dialog", { name: "Crawl configuration" });
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Speed" })).toHaveAttribute("aria-current", "page"));
  });
});

describe("Sperren nach Crawl-Zustand (F-13, F-14, CRAWL-126)", () => {
  // spec: CRAWL-126 [M2]
  it("während „Läuft“ sind nur die freigegebenen Gruppen editierbar; „Pausieren und bearbeiten“ pausiert", async () => {
    await setup(undefined, "running");
    const dialog = await open();
    await goPage(dialog, "Limits");
    expect(field(dialog, "Max. URLs")).toBeDisabled();
    expect(field(dialog, "Max. URLs").closest("[data-lock]")).toHaveAttribute("title", "Pause the crawl to change this setting");
    await goPage(dialog, "Speed");
    expect(field(dialog, "Max. threads")).toBeEnabled();
    expect(within(dialog).getByRole("textbox", { name: "Max. simultaneous connections per host" })).toBeDisabled();
    await goPage(dialog, "Presets");
    for (const box of within(dialog).getAllByRole("textbox", { name: /Max\. characters|Min\. characters/ })) expect(box).toBeEnabled();

    fireEvent.click(within(dialog).getByTestId("config-pause-edit"));
    await waitFor(() => expect(fake.pauseCalls).toBe(1));
    act(() => useSession.setState({ crawl: status("paused") }));
    await goPage(dialog, "Limits");
    await waitFor(() => expect(field(dialog, "Max. URLs")).toBeEnabled());
    expect(within(dialog).queryByTestId("config-pause-edit")).toBeNull();
  });

  // spec: CRAWL-126 [M2]
  it("Änderungen während „Läuft“ melden, was erst nach der Pause wirkt; speed.threads wirkt live", async () => {
    await setup(undefined, "running");
    const dialog = await open();
    await goPage(dialog, "Speed");
    type(field(dialog, "Max. threads"), "9");
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(fake.setCalls).toHaveLength(1));
    expect((fake.config.speed as Record<string, unknown>).threads).toBe(9);
    // Kein Dialogende ohne Rückmeldung nötig: nur threads geändert → nichts aufgeschoben.
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  // spec: CRAWL-126 [M2]
  it("Änderungen an Schwellwerten während „Läuft“ werden übernommen; die Rückmeldung nennt, was erst nach der Pause wirkt", async () => {
    await setup(undefined, "running");
    const dialog = await open("title.max_chars");
    const group = dialog.querySelector('[data-group="title"]') as HTMLElement;
    type(within(group).getByRole("textbox", { name: "Max. characters" }), "65");
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(within(dialog).getByText(/These changes take effect after the crawl is paused: title\.max_chars/)).toBeInTheDocument());
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect((fake.config.title as Record<string, unknown>).max_chars).toBe(65);
    // Zweites OK ohne weitere Änderung schließt.
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(fake.setCalls).toHaveLength(1);
  });

  // spec: CRAWL-126 [M2]
  it("im pausierten Zustand sind Rendering-Modus und Bereich ausgegraut („neuer Crawl nötig“), der Rest ist editierbar", async () => {
    await setup(undefined, "paused");
    const dialog = await open();
    await goPage(dialog, "Limits");
    expect(field(dialog, "Max. URLs")).toBeEnabled();
    await goPage(dialog, "Crawl");
    const sub = within(dialog).getByRole("switch", { name: "Crawl all subdomains" });
    expect(sub).toBeDisabled();
    expect(sub.closest("[data-lock]")).toHaveAttribute("title", "A new crawl is needed to change this setting");
    await goPage(dialog, "Include / Exclude");
    expect(within(dialog).getByRole("textbox", { name: "Include rules" })).toBeEnabled();
  });

  // spec: CRAWL-126 [M2]
  it("ein fremd gestarteter Crawl macht den ganzen Dialog schreibgeschützt", async () => {
    await setup(undefined, "foreign");
    const dialog = await open();
    expect(within(dialog).getByText(/started by another process/)).toBeInTheDocument();
    await goPage(dialog, "Speed");
    expect(field(dialog, "Max. threads")).toBeDisabled();
    expect(within(dialog).queryByTestId("config-pause-edit")).toBeNull();
    expect(within(dialog).getByRole("button", { name: "Close" })).toBeInTheDocument();
  });
});

describe("Rendering (CRAWL-077)", () => {
  // spec: CRAWL-077 [P1]
  it.each(["rendering.page_timeout_s", "rendering.tabs"])("ein Bereichsfehler an %s bleibt am Feld und erzeugt keinen Chromium-Banner", async (path) => {
    await setup(withConfig({ [path]: 0 }));
    const dialog = await open(path);
    const anchor = dialog.querySelector<HTMLElement>(`[data-setting-anchor="${path}"]`)!;
    expect(await within(anchor).findByText("muss mindestens 1 sein (ist 0)")).toBeInTheDocument();
    expect(within(anchor).getByRole("textbox")).toHaveAttribute("aria-invalid", "true");
    expect(within(dialog).queryByTestId("rendering-problem")).toBeNull();
    expect(within(dialog).queryByRole("link", { name: "Download Chromium" })).toBeNull();
    expect(core.calls.some((c) => c.cmd === "config.validate")).toBe(true);
  });

  // spec: CRAWL-077 [P1]
  it("der Rendering-Umschalter ist während „Läuft“ und „Pausiert“ gesperrt, mit Hinweis „neuer Crawl nötig“", async () => {
    for (const mode of ["running", "paused"] as const) {
      await setup(undefined, mode);
      const dialog = await open("rendering.mode");
      await waitFor(() => expect(within(dialog).getByTestId("rendering-locked-note")).toHaveTextContent("only be changed in a new crawl"));
      const anchor = document.querySelector<HTMLElement>('[data-setting-anchor="rendering.mode"]')!;
      expect(anchor).toHaveAttribute("data-lock", "newCrawl");
      expect(within(anchor).getAllByRole("radio")).toHaveLength(2);
      for (const radio of within(anchor).getAllByRole("radio")) expect(radio).toBeDisabled();
      act(() => closeConfig());
      cleanup();
    }
  });

  // spec: CRAWL-077 [P1]
  it("im freien Zustand schaltet der Umschalter auf JavaScript; ein Chromium-Fehler des Kerns erscheint als Dialogmeldung mit Pfad- und Download-Option", async () => {
    await setup();
    const dialog = await open("rendering.mode");
    const js = await within(dialog).findByRole("radio", { name: /JavaScript \(headless Chromium\)/ });
    expect(js).toBeEnabled();
    fireEvent.click(js);
    await waitFor(() => expect(js).toBeChecked());
    fake.failNextSet = { code: "InvalidInput", messageKey: "err-chromium-missing", detail: "Chromium not found at /opt/none" };
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    const problem = await within(dialog).findByTestId("rendering-problem");
    expect(problem).toHaveTextContent("Chromium is not available: Chromium not found at /opt/none");
    expect(within(problem).getByRole("link", { name: "Download Chromium" })).toHaveAttribute("href", expect.stringContaining("chromium"));
    fireEvent.click(within(problem).getByRole("button", { name: "Set path" }));
    await waitFor(() => expect(document.activeElement).toBe(field(dialog, "Chromium path")));
    // Der Dialog bleibt offen, die Eingabe geht nicht verloren.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(fake.setCalls).toHaveLength(1);
    expect(js).toBeChecked();
    type(field(dialog, "Chromium path"), "/opt/chromium/chrome");
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(fake.setCalls).toHaveLength(2);
    const rendering = fake.config.rendering as Record<string, unknown>;
    expect([rendering.mode, rendering.chromium_path]).toEqual(["javascript", "/opt/chromium/chrome"]);
  });
});

describe("Include/Exclude und Umschreibung mit Testfeld (CRAWL-111, CRAWL-107)", () => {
  beforeEach(() => setup());

  // spec: CRAWL-111
  it("der Tester zeigt Treffer und Syntaxfehler zeilengenau ohne leere Eingabezeilen mitzuzählen", async () => {
    const dialog = await open();
    await goPage(dialog, "Include / Exclude");
    type(within(dialog).getByRole("textbox", { name: "Include rules" }), "\nhttps://site.test/blog\n   \nhttps://site.test/shop/.*\n");
    type(within(dialog).getByRole("textbox", { name: "Exclude rules" }), "\nhttps://site.test/shop/private/.*\n\n(");
    type(field(dialog, "Test URL"), "https://site.test/shop/a b");
    const verdict = await within(dialog).findByTestId("regex-result");
    await waitFor(() => expect(verdict).toHaveAttribute("data-verdict", "included"));
    expect(within(verdict).getByTestId("regex-match")).toHaveTextContent("Include line 2");
    expect(within(verdict).getByText(/a%20b/)).toBeInTheDocument();
    // Syntaxfehler: Zeile 2 der Exclude-Liste, auch am Feld (config.validate) und im Tester.
    await waitFor(() => expect(within(dialog).getByTestId("regex-invalid")).toHaveTextContent("Exclude line 2"));
    await waitFor(() => expect(within(dialog).getByText(/Line 2 \(\(\)/)).toBeInTheDocument());

    type(field(dialog, "Test URL"), "https://site.test/shop/private/x");
    await waitFor(() => expect(within(dialog).getByTestId("regex-result")).toHaveAttribute("data-verdict", "excluded"));
    expect(within(dialog).getByTestId("regex-match")).toHaveTextContent("Exclude line 1");

    type(field(dialog, "Test URL"), "https://site.test/blog/post");
    await waitFor(() => expect(within(dialog).getByTestId("regex-partial-hint")).toHaveTextContent("Include line 1 only matches part of the URL – add .* ?"));
    await waitFor(() => expect(within(dialog).getByTestId("regex-result")).toHaveAttribute("data-verdict", "notIncluded"));
  });

  // spec: CRAWL-107
  it("das Umschreibungs-Testfeld aktualisiert sich bei jeder Regeländerung ohne Speichern", async () => {
    const dialog = await open();
    await goPage(dialog, "URL rewriting");
    type(field(dialog, "Test URL"), "https://site.test/old/page?utm_source=x&keep=1");
    await waitFor(() => expect(within(dialog).getByTestId("rewrite-output")).toHaveTextContent("https://site.test/old/page?utm_source=x&keep=1"));
    type(within(dialog).getByRole("textbox", { name: "Parameters to remove" }), "utm_source");
    await waitFor(() => expect(within(dialog).getByTestId("rewrite-output")).toHaveTextContent("https://site.test/old/page?keep=1"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Add rule" }));
    type(field(dialog, "Regular expression, rule 1"), "/old/");
    type(field(dialog, "Replacement, rule 1"), "/new/");
    await waitFor(() => expect(within(dialog).getByTestId("rewrite-output")).toHaveTextContent("https://site.test/new/page?keep=1"));
    const steps = within(dialog).getByRole("list", { name: "Intermediate results" });
    expect(within(steps).getAllByRole("listitem")).toHaveLength(2);
    expect(within(steps).getByText("Regex rule 1")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("switch", { name: "Remove the whole query string" }));
    await waitFor(() => expect(within(dialog).getByTestId("rewrite-output").textContent).toBe("https://site.test/new/page"));
    expect(core.calls.some((c) => c.cmd === "config.set")).toBe(false);
  });

  // spec: CRAWL-107
  it("Regeln lassen sich umsortieren und entfernen; die Reihenfolge bestimmt das Ergebnis", async () => {
    const dialog = await open();
    await goPage(dialog, "URL rewriting");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add rule" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Add rule" }));
    type(field(dialog, "Regular expression, rule 1"), "a");
    type(field(dialog, "Replacement, rule 1"), "b");
    type(field(dialog, "Regular expression, rule 2"), "b");
    type(field(dialog, "Replacement, rule 2"), "c");
    type(field(dialog, "Test URL"), "https://site.test/a");
    await waitFor(() => expect(within(dialog).getByTestId("rewrite-output")).toHaveTextContent("https://site.test/c"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Move rule 2 up" }));
    await waitFor(() => expect(within(dialog).getByTestId("rewrite-output")).toHaveTextContent("https://site.test/b"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove rule 1" }));
    await waitFor(() => expect(within(dialog).getAllByTestId("rewrite-rules")[0]!.querySelectorAll("tbody tr")).toHaveLength(1));
  });
});

describe("robots.txt (CRAWL-102)", () => {
  beforeEach(() => setup());

  // spec: CRAWL-102
  it("eigene Datei je Subdomain: hinzufügen, Live-Datei laden, Tester zeigt Erlaubt/Blockiert mit entscheidender Zeile", async () => {
    const dialog = await open();
    await goPage(dialog, "robots.txt");
    type(field(dialog, "Subdomain"), "https://Blog.Example.test/");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add" }));
    const editor = (await within(dialog).findByRole("textbox", { name: "robots.txt for blog.example.test" })) as HTMLTextAreaElement;
    fireEvent.click(within(dialog).getByRole("button", { name: "Load live file" }));
    await waitFor(() => expect(editor.value).toContain("Disallow: /live-blocked/"));
    fireEvent.change(editor, { target: { value: "User-agent: *\nAllow: /\nDisallow: /blog/\n" } });
    type(field(dialog, "URL to test"), "https://blog.example.test/blog/post");
    fireEvent.click(within(dialog).getByRole("button", { name: "Test" }));
    const result = await within(dialog).findByTestId("robots-result");
    expect(result).toHaveAttribute("data-allowed", "false");
    expect(result).toHaveTextContent("Blocked");
    expect(result).toHaveTextContent("Deciding line 3:");
    expect(result.querySelector("mark")).toHaveTextContent("Disallow: /blog/");
    expect(result).toHaveTextContent("User agent: OpenSpider");
    // Der Entwurf der Datei gilt für den Test sofort, ohne Speichern.
    const call = core.calls.filter((c) => c.cmd === "config.testRobots").at(-1)?.args as { content?: string };
    expect(call.content).toContain("Disallow: /blog/");
    type(field(dialog, "URL to test"), "https://blog.example.test/ok");
    fireEvent.click(within(dialog).getByRole("button", { name: "Test" }));
    await waitFor(() => expect(within(dialog).getByTestId("robots-result")).toHaveAttribute("data-allowed", "true"));
  });

  // spec: CRAWL-102
  it("eigene Dateien landen in der Konfiguration (Profil) und lassen sich wieder entfernen", async () => {
    const dialog = await open();
    await goPage(dialog, "robots.txt");
    type(field(dialog, "Subdomain"), "shop.example.test");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add" }));
    const editor = await within(dialog).findByRole("textbox", { name: "robots.txt for shop.example.test" });
    fireEvent.change(editor, { target: { value: "User-agent: *\nDisallow: /blog/\n" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(fake.setCalls).toHaveLength(1));
    expect((fake.config.robots as { custom_files: Record<string, string> }).custom_files).toEqual({ "shop.example.test": "User-agent: *\nDisallow: /blog/\n" });

    const again = await open();
    await goPage(again, "robots.txt");
    fireEvent.click(await within(again).findByRole("button", { name: "Remove" }));
    expect(within(again).getByText("No custom robots.txt files.")).toBeInTheDocument();
  });

  // spec: CRAWL-102
  it("der Tester prüft mit dem Entwurfs-User-Agent des Dialogs, ohne Speichern", async () => {
    const dialog = await open();
    await goPage(dialog, "User-Agent");
    type(field(dialog, "robots.txt user agent (token)"), "Special");
    await goPage(dialog, "robots.txt");
    type(field(dialog, "URL to test"), "https://site.test/blog/post");
    fireEvent.click(within(dialog).getByRole("button", { name: "Test" }));
    await within(dialog).findByTestId("robots-result");
    const call = core.calls.filter((c) => c.cmd === "config.testRobots").at(-1)?.args as { userAgent?: string };
    expect(call.userAgent).toBe("Special");
  });
});

describe("Profile (OUT-220)", () => {
  // spec: OUT-220 [M2]
  it("Profil speichern, laden, als Standard festlegen; Standard zurücksetzen fragt nach Bestätigung", async () => {
    await setup(withConfig({ "limits.max_urls": 4242 }));
    const dialog = await open("profiles");
    await goPage(dialog, "Profiles");
    expect(await within(dialog).findByTestId("profiles-empty")).toBeInTheDocument();
    type(field(dialog, "Profile name"), "relaunch");
    fireEvent.click(within(dialog).getByRole("checkbox", { name: "Also make it the default" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(fake.profiles.has("relaunch")).toBe(true));
    expect(fake.defaultProfile).toBe("relaunch");
    expect((fake.profiles.get("relaunch")!.limits as Record<string, unknown>).max_urls).toBe(4242);
    const row = await within(dialog).findByText("relaunch");
    expect(row.closest("tr")).toHaveAttribute("data-default", "true");

    // Standard zurücksetzen: Rückfrage, Abbrechen ändert nichts.
    fireEvent.click(within(dialog).getByRole("button", { name: "Reset default" }));
    const confirm = await screen.findByRole("dialog", { name: "Reset default?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Reset default?" })).toBeNull());
    expect(fake.defaultProfile).toBe("relaunch");
    fireEvent.click(within(dialog).getByRole("button", { name: "Reset default" }));
    fireEvent.click(within(await screen.findByRole("dialog", { name: "Reset default?" })).getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(fake.defaultProfile).toBeNull());
  });

  // spec: OUT-220 [M2]
  it("ein geladenes Profil ist zunächst nur Entwurf; erst OK wendet es über config.set an", async () => {
    await setup();
    fake.profiles.set("klein", withConfig({ "limits.max_urls": 99 }));
    const dialog = await open("profiles");
    await goPage(dialog, "Profiles");
    fireEvent.click(await within(dialog).findByRole("button", { name: "Load profile klein" }));
    await waitFor(() => expect(within(dialog).getByText(/Profile "klein" loaded into the dialog/)).toBeInTheDocument());
    expect(fake.setCalls).toHaveLength(0);
    await goPage(dialog, "Limits");
    expect(field(dialog, "Max. URLs").value).toBe("99");
    fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(fake.setCalls).toHaveLength(1));
    expect((fake.config.limits as Record<string, unknown>).max_urls).toBe(99);
  });

  // spec: OUT-220 [M2]
  it("Laden ist während „Läuft“ gesperrt (Hinweis), im pausierten Zustand nur ohne Änderung nicht änderbarer Felder", async () => {
    await setup(undefined, "running");
    fake.profiles.set("js", withConfig({ "rendering.mode": "javascript" }));
    fake.profiles.set("limits", withConfig({ "limits.max_urls": 7 }));
    let dialog = await open("profiles");
    await goPage(dialog, "Profiles");
    expect(await within(dialog).findByTestId("profiles-locked")).toHaveTextContent("Pause the crawl first");
    expect(await within(dialog).findByRole("button", { name: "Load profile js" })).toBeDisabled();
    act(() => closeConfig());
    cleanup();

    await setup(undefined, "paused");
    fake.profiles.set("js", withConfig({ "rendering.mode": "javascript" }));
    fake.profiles.set("limits", withConfig({ "limits.max_urls": 7 }));
    dialog = await open("profiles");
    await goPage(dialog, "Profiles");
    fireEvent.click(await within(dialog).findByRole("button", { name: "Load profile js" }));
    await waitFor(() => expect(within(dialog).getByText(/was not loaded: it changes settings that cannot be changed for this crawl \(rendering\.mode\)/)).toBeInTheDocument());
    await goPage(dialog, "Limits");
    expect(field(dialog, "Max. URLs").value).toBe("5000000");
    await goPage(dialog, "Profiles");
    fireEvent.click(within(dialog).getByRole("button", { name: "Load profile limits" }));
    await waitFor(() => expect(within(dialog).getByText(/Profile "limits" loaded/)).toBeInTheDocument());
  });

  // spec: CRAWL-126 [M2]
  it("per Profil erzwungene Änderung des Rendering-Modus im pausierten Crawl wird mit Meldung abgewiesen", async () => {
    await setup(undefined, "paused");
    fake.profiles.set("js", withConfig({ "rendering.mode": "javascript" }));
    const dialog = await open("profiles");
    await goPage(dialog, "Profiles");
    fireEvent.click(await within(dialog).findByRole("button", { name: "Load profile js" }));
    await waitFor(() => expect(within(dialog).getByText(/Profile "js" was not loaded.*\(rendering\.mode\)/)).toBeInTheDocument());
    expect(fake.setCalls).toHaveLength(0);
    // Die Anzeige im Dialog bleibt unverändert: der Rendering-Modus ist weiter „Text“.
    act(() => useSession.setState({ crawl: status("paused") }));
    await goPage(dialog, "Rendering");
    expect(within(dialog).getByRole("radio", { name: /Text only/ })).toBeChecked();
  });

  // spec: OUT-220 [M2]
  it("überschreiben fragt nach, löschen fragt nach", async () => {
    await setup();
    fake.profiles.set("alt", withConfig({}));
    const dialog = await open("profiles");
    await goPage(dialog, "Profiles");
    type(field(dialog, "Profile name"), "alt");
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    const overwrite = await screen.findByRole("dialog", { name: "Overwrite profile?" });
    fireEvent.click(within(overwrite).getByRole("button", { name: "Cancel" }));
    expect(core.calls.filter((c) => c.cmd === "profile.save")).toHaveLength(0);
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    fireEvent.click(within(await screen.findByRole("dialog", { name: "Overwrite profile?" })).getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(core.calls.filter((c) => c.cmd === "profile.save")).toHaveLength(1));
    expect((core.calls.find((c) => c.cmd === "profile.save")!.args as { overwrite: boolean }).overwrite).toBe(true);
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete profile alt" }));
    fireEvent.click(within(await screen.findByRole("dialog", { name: "Delete profile?" })).getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(fake.profiles.has("alt")).toBe(false));
  });
});

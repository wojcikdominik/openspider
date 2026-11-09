// Unabhängige Abnahme B4-PROJDLG, Vitest-Teil (jsdom).
// acceptance: independent
// Erwartungen stammen allein aus der Spec: CRAWL-127 (OS-Festlegung 09.11.2026: lesbare Einträge unter „Zuletzt geöffnet“ in der
// Crawl-Verwaltung und im Menü Datei), OUT-228 / UI-124 (Formulare der Crawl-Verwaltung). jsdom kennt keine echte Tastatur
// und kein Layout: Tippen und Zeilenumbruch prüft die Playwright-Spec `e2e/acceptance/b4-projdlg.spec.ts`. Hier wird geprüft,
// dass Tastenereignisse im Formular nicht verschluckt werden (nicht abgebrochen), welcher Text und welche Attribute die
// Einträge tragen und wie das Menü Datei → Zuletzt geöffnet beschriftet ist.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setLanguage } from "../../i18n";
import { createClient, setClient } from "../../ipc/client";
import { MockCore } from "../../ipc/transport/mock";
import { useSession } from "../../state/session";
import { allMenuContributions } from "../../ui/menu/registry";
import { buildMenuModel, flattenItems } from "../../ui/menu/build";
import { BASE_MENU } from "../../app/baseMenu";
import { mainMenuContributions } from "../../ui/menu/contributions/main";
import { loadRecent } from "../help/recent";
import { catalogFixture } from "../help/__fixtures__/fx-ui-basic";
import { CrawlsHost } from "./CrawlsHost";
import { fixtureRows, managementFixture } from "./fixtures/core";
import { useCrawlsDialog, writeRecent } from "./model";

const MAC_LONG = "/Users/dominik.wojcik/Library/Application Support/de.openspider.app/crawls/Kundenprojekte/Agentur Nordlicht/01a18894-1234-7000-8000-bbfe3c06b178.ospider";
const WIN_LONG = "C:\\Users\\Dominik Wojcik\\AppData\\Roaming\\de.openspider.app\\crawls\\Kundenprojekte\\Agentur Nordlicht\\Relaunch\\01a18894-1234-7000-8000-bbfe3c06b178.ospider";
const FILE = "01a18894-1234-7000-8000-bbfe3c06b178.ospider";

let fixture: ReturnType<typeof managementFixture>;
beforeEach(async () => {
  fixture = managementFixture();
  setClient(createClient(fixture.core.transport("main")));
  localStorage.clear();
  useSession.setState({ crawl: null, ops: {} });
  useCrawlsDialog.setState({ open: false });
  await setLanguage("en");
});
afterEach(() => { cleanup(); setClient(null); useCrawlsDialog.setState({ open: false }); vi.restoreAllMocks(); });

async function mount(open = true) {
  useCrawlsDialog.setState({ open });
  render(<CrawlsHost desktop={fixture.desktop} />);
  if (open) await screen.findByRole("checkbox", { name: "Select: Alpha" });
}

/** Dateimenü so, wie die App es aus allen Beiträgen baut (Verwaltung + Hauptmenü). */
function fileMenu(recent: { path: string; label?: string }[] = [], platform: "mac" | "win" = "mac") {
  const model = buildMenuModel([...BASE_MENU, ...mainMenuContributions({ recent, mode: "spider", mcpRunning: false, mcpAutostart: false, catalog: catalogFixture }), ...allMenuContributions()],
    { platform, lang: "en", native: true, t: (key: string) => key, ctx: { hasCrawl: false } });
  return model.menus.find(menu => menu.id === "file")!;
}
const flat = (items: Parameters<typeof flattenItems>[0]) => flattenItems(items);

// spec: UI-124
// spec: OUT-228
it("Tastenereignisse im Suchfeld und in allen vier Formularen werden nicht abgebrochen und der Fokus liegt im Feld", async () => {
  await mount();
  // Wiederholte Tastenereignisse: ein globaler Handler, der Zeichen schluckt (preventDefault), würde das Tippen verhindern.
  const passes = (field: HTMLElement) => ["a", "Z", " ", "ä", "1", "/", "Backspace", "Delete", "ArrowLeft"].every(key =>
    field.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true })) &&
    field.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true, cancelable: true })));
  const search = screen.getByRole("searchbox");
  search.focus();
  expect(passes(search)).toBe(true);

  const open = async (trigger: string, dialog: string, field: "textbox" | "combobox", name: string) => {
    fireEvent.click(screen.getByRole("button", { name: trigger }));
    const form = await screen.findByRole("dialog", { name: dialog });
    const input = within(form).getByRole(field, { name });
    await waitFor(() => expect(document.activeElement).toBe(input));
    expect(form.contains(document.activeElement)).toBe(true);
    expect(passes(input)).toBe(true);
    fireEvent.keyDown(input, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: dialog })).toBeNull());
  };
  await open("Create project", "Create project", "textbox", "Name");
  fireEvent.click(screen.getByRole("checkbox", { name: "Select: Alpha" }));
  await open("Rename", "Rename", "textbox", "Name");
  await open("Move to project", "Move to project", "combobox", "Destination project");
  fireEvent.click(screen.getByRole("checkbox", { name: "Select: Alpha" }));
  fireEvent.click(within(screen.getByRole("treegrid", { name: "Projects" })).getByText(/^Kunde A/));
  await open("Rename project", "Rename project", "textbox", "Name");
  // Esc schließt nur das Formular, die Verwaltung bleibt
  expect(screen.getByRole("dialog", { name: "Manage crawls…" })).toBeInTheDocument();
});

// spec: CRAWL-127
it("Verwaltung, bekannter Crawl: Hauptzeile Name – Datum Uhrzeit, Pfad darunter, voller Pfad als Tooltip und im zugänglichen Namen", async () => {
  writeRecent(["/fixture/Alpha.ospider"]);
  await mount();
  const nav = screen.getByRole("navigation", { name: "Recently opened" });
  const entry = within(nav).getByRole("button", { name: /Alpha/ });
  expect(entry).toHaveAccessibleName(expect.stringContaining("/fixture/Alpha.ospider"));
  expect(entry.textContent).toMatch(/^Alpha – [A-Z][a-z]{2} \d{1,2}, 2026,? \d{1,2}:\d{2}\s?(AM|PM)?\s*\/fixture\/Alpha\.ospider$/);
  expect(entry.closest("[title]")).toHaveAttribute("title", "/fixture/Alpha.ospider");
});

// spec: CRAWL-127
it("Verwaltung, unbekannter Crawl: Hauptzeile ist der Dateiname, Pfad (Mac und Windows) bleibt vollständig lesbar im Tooltip und Namen", async () => {
  writeRecent([MAC_LONG, WIN_LONG]);
  await mount();
  const nav = screen.getByRole("navigation", { name: "Recently opened" });
  for (const path of [MAC_LONG, WIN_LONG]) {
    const entry = within(nav).getByRole("button", { name: new RegExp(path.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")) });
    expect(entry.textContent!.trimStart().startsWith(FILE), path).toBe(true);
    expect(entry.textContent!.trimEnd().endsWith(FILE), "Pfadzeile endet mit dem Dateinamen").toBe(true);
    expect(entry.closest("[title]")).toHaveAttribute("title", path);
    // Der volle Pfad steht nirgends doppelt zusammengesetzt im sichtbaren Text, sondern höchstens einmal als Pfadzeile.
    expect(entry.textContent!.split(FILE).length - 1).toBeLessThanOrEqual(2);
  }
});

// spec: CRAWL-127
it("Verwaltung, bekannter Crawl mit langem Pfad: Name und Datum bleiben Hauptzeile, Pfad endet mit Dateiname", async () => {
  const rows = structuredClone(fixtureRows);
  rows[0]!.crawl.file = MAC_LONG;
  fixture.setRows(rows);
  writeRecent([MAC_LONG]);
  await mount();
  const entry = within(screen.getByRole("navigation", { name: "Recently opened" })).getByRole("button");
  expect(entry.textContent).toMatch(/^Alpha – /);
  expect(entry.textContent!.trimEnd().endsWith(FILE)).toBe(true);
  expect(entry.closest("[title]")).toHaveAttribute("title", MAC_LONG);
});

// spec: CRAWL-127
it("Verwaltung: ohne Crawl-Namen ersetzt die Domain der Start-URL den Namen", async () => {
  const rows = structuredClone(fixtureRows);
  rows[0]!.crawl.name = "";
  rows[0]!.startUrl = "https://www.example.com/start?x=1";
  fixture.setRows(rows);
  writeRecent(["/fixture/Alpha.ospider"]);
  await mount(false);
  act(() => { useCrawlsDialog.setState({ open: true }); });
  const entry = await within(await screen.findByRole("navigation", { name: "Recently opened" })).findByRole("button");
  await waitFor(() => expect(entry.textContent).toMatch(/^www\.example\.com – /));
  expect(entry.textContent).not.toMatch(/https?:\/\//);
});

// spec: CRAWL-127
it("Menü Datei, Zuletzt geöffnet: bekannter Crawl als „<Name> – <Datum Uhrzeit>“ in einer Zeile", async () => {
  writeRecent(["/fixture/Alpha.ospider", "/fixture/Beta.ospider"]);
  await mount(false);
  act(() => { useCrawlsDialog.setState({ open: true }); });
  await screen.findByRole("navigation", { name: "Recently opened" });
  await waitFor(() => {
    const labels = flat(fileMenu().items).filter(item => /^crawls\.recent\.\d$/.test(item.id)).map(item => item.label);
    expect(labels[0]).toMatch(/^Alpha – [A-Z][a-z]{2} \d{1,2}, 2026,? \d{1,2}:\d{2}/);
    expect(labels[1]).toMatch(/^Beta – [A-Z][a-z]{2} \d{1,2}, 2026,? \d{1,2}:\d{2}/);
    for (const label of labels) expect(label).not.toContain("\n");
  });
});

// spec: CRAWL-127
it("Menü Datei, Zuletzt geöffnet: unbekannter langer Pfad (Mac und Windows) hat höchstens 64 Zeichen, behält Anfang und Dateiname und trägt „…“", async () => {
  for (const path of [MAC_LONG, WIN_LONG]) {
    const label = flat(fileMenu([{ path }]).items).find(item => item.id === "recent.0")!.label;
    expect(label.length, label).toBeLessThanOrEqual(64);
    expect(label).toContain("…");
    expect(label.startsWith(path.slice(0, 6)), label).toBe(true);
    expect(label.endsWith(".ospider"), label).toBe(true);
    // Der Dateiname bleibt sichtbar: mindestens seine letzten 20 Zeichen samt Endung stehen am Ende.
    expect(label.endsWith(FILE.slice(-20)), label).toBe(true);
    expect(label.indexOf("…")).toBeGreaterThan(0);
    expect(label.indexOf("…")).toBeLessThan(label.length - 8);
  }
});

// spec: CRAWL-127
it("Menü Datei, Zuletzt geöffnet: Pfade bis 64 Zeichen bleiben unverändert (63, 64 Zeichen; Mac und Windows)", () => {
  const pad = (base: string, total: number, sep: string) => {
    const head = base + sep;
    return head + "x".repeat(total - head.length - ".ospider".length) + ".ospider";
  };
  for (const [base, sep] of [["/Users/dominik", "/"], ["C:\\Users\\Dominik", "\\"]] as const) {
    for (const length of [40, 63, 64]) {
      const path = pad(base, length, sep);
      expect(path.length).toBe(length);
      const menu = fileMenu([{ path }]);
      expect(flat(menu.items).find(item => item.id === "recent.0")!.label, path).toBe(path);
    }
  }
  // Ein Zeichen mehr wird gekürzt (Grenze der Spec: „höchstens 64“).
  const over = pad("/Users/dominik/ein/sehr/langer/Ordnername/der/die/Grenze/ueberschreitet", 80, "/");
  const label = flat(fileMenu([{ path: over }]).items).find(item => item.id === "recent.0")!.label;
  expect(label.length).toBeLessThanOrEqual(64);
  expect(label).toContain("…");
});

// spec: CRAWL-127
it("Menü Datei: „Zuletzt geöffnet“ aus dem Speicher trägt Name/Domain mit Datum und Uhrzeit, öffnet aber weiter den vollen Pfad", async () => {
  const core = new MockCore();
  setClient(createClient(core.transport()));
  core.uiState = { version: 1, state: { recentCrawls: [{ path: MAC_LONG }, { path: "/fixture/named.ospider" }, { path: "/fixture/domain.ospider" }, { path: "/fixture/unknown.ospider" }] } };
  core.handlers.set("crawls.list", () => [
    { crawl: { file: MAC_LONG, startedAt: "2026-10-29T12:00:00Z", name: "Shop Relaunch" }, startUrl: "https://shop.example.org/" },
    { crawl: { file: "/fixture/named.ospider", startedAt: "2026-10-29T12:00:00Z", name: "Named" }, startUrl: "https://named.example.org/" },
    { crawl: { file: "/fixture/domain.ospider", startedAt: "2026-10-29T12:00:00Z", name: "" }, startUrl: "https://www.domain.example.net/a/b" },
  ]);
  const entries = await loadRecent();
  expect(entries.map(entry => entry.path)).toEqual([MAC_LONG, "/fixture/named.ospider", "/fixture/domain.ospider", "/fixture/unknown.ospider"]);
  expect(entries[0]!.label).toMatch(/^Shop Relaunch – Oct 29, 2026, \d{1,2}:\d{2}/);
  expect(entries[1]!.label).toMatch(/^Named – Oct 29, 2026, \d{1,2}:\d{2}/);
  expect(entries[2]!.label).toMatch(/^www\.domain\.example\.net – Oct 29, 2026, \d{1,2}:\d{2}/);
  // Unbekannt: kein Label aus dem Index, das Menü kürzt den Pfad selbst.
  const labels = flat(fileMenu(entries).items).filter(item => /^recent\.\d$/.test(item.id)).map(item => item.label);
  expect(labels[0]).toMatch(/^Shop Relaunch – /);
  expect(labels[3]).toBe("/fixture/unknown.ospider");
});

// spec: CRAWL-127
it("Menü Datei, Zuletzt geöffnet: ein Eintrag erscheint nicht doppelt, das Untermenü ist genau einmal vorhanden", async () => {
  writeRecent(["/fixture/Alpha.ospider"]);
  await mount(false);
  const file = fileMenu([{ path: "/fixture/Alpha.ospider" }]);
  const submenus = flat(file.items).filter(item => item.label === "Recently opened" || item.label === "u04-recent" || item.id === "file.recent" || item.id === "crawls.recent");
  expect(submenus.map(item => item.id), "Datei-Menü enthält mehr als ein Untermenü „Zuletzt geöffnet“").toHaveLength(1);
});

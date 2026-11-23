// M4-F02: Schalter „KI (Vorschau)“ (Plan M4 E1, Nutzerentscheidung E-2 b) – ohne Schalter keine KI-Oberfläche, mit Schalter
// erscheinen Einstellungsabschnitt KI, KI-Konfigurationsseiten, Statusleistenfeld Kosten (Platzhalter) und die Leerzustände der
// Bereiche aus Plan M4 3.3. Fixture: MockCore mit den Standard-Einstellungen (Abschnitt `aiApp` fehlt → aus).
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setLanguage } from "../../i18n";
import { createClient, setClient } from "../../ipc/client";
import { MockCore } from "../../ipc/transport/mock";
import { usePrefs } from "../../state/prefs";
import { DEFAULT_APPEARANCE } from "../../ui/tokens/theme";
import { resolvePages } from "../config/frame/registry";
import { SettingsFrame } from "../settings/frame/SettingsFrame";
import { resolveSections, sectionExtras } from "../settings/frame/registry";
import { AI_AREA_IDS, aiAreas } from "./areas";
import { AiCostField } from "./cost/CostField";
import { AI_TABS, aiPreviewNow, aiPreviewOn, isAiTab, setAiPreview } from "./preview";
import { AiPreviewSetting } from "./settings/PreviewSetting";
import { catalogMatchesPreview } from "./tabs";

let core: MockCore;

beforeEach(async () => {
  core = new MockCore();
  setClient(createClient(core.transport("ai-preview")));
  usePrefs.setState({ loaded: false, dto: null, appearance: DEFAULT_APPEARANCE, language: "en", languageSetting: "system" });
  await usePrefs.getState().load();
});
afterEach(async () => {
  cleanup();
  setClient(null);
  await act(async () => {
    await setLanguage("en");
  });
});

describe("Schalter KI (Vorschau)", () => {
  // spec: UI-004 [M4]
  it("ist standardmäßig aus und wird über settings.set als aiApp.previewEnabled gespeichert", async () => {
    expect(aiPreviewNow()).toBe(false);
    expect(aiPreviewOn(null)).toBe(false);
    await act(async () => {
      await setAiPreview(true);
    });
    expect(aiPreviewNow()).toBe(true);
    expect(core.calls.filter((c) => c.cmd === "settings.set").map((c) => c.args)).toEqual([{ patch: { aiApp: { previewEnabled: true } } }]);
    expect((core.settings as unknown as { aiApp: { previewEnabled: boolean } }).aiApp.previewEnabled).toBe(true);
  });

  // spec: UI-004 [M4]
  it("steht auf der Seite Allgemein (de/en) und schaltet die Vorschau ein", async () => {
    expect(sectionExtras("general")).toContain(AiPreviewSetting);
    expect(sectionExtras("appearance")).toEqual([]);
    await act(async () => {
      await setLanguage("de");
    });
    render(<SettingsFrame anchor="general" />);
    const toggle = screen.getByRole("switch", { name: "KI (Vorschau)" });
    expect(toggle).not.toBeChecked();
    fireEvent.click(toggle);
    await waitFor(() => expect(aiPreviewNow()).toBe(true));
    await act(async () => {
      await setLanguage("en");
    });
    expect(screen.getByRole("switch", { name: "AI (preview)" })).toBeChecked();
  });
});

describe("ohne Schalter keine KI-Oberfläche", () => {
  // spec: UI-004 [M4]
  // spec: UI-013 [M4]
  it("kein Abschnitt KI, keine KI-Konfigurationsseite, kein Feld Kosten", () => {
    expect(resolveSections().map((s) => s.id)).not.toContain("ai");
    expect(resolvePages().map((p) => p.id)).not.toContain("ai");
    render(<AiCostField />);
    expect(screen.queryByTestId("ai-cost")).toBeNull();
    render(<SettingsFrame />);
    const nav = screen.getByRole("tablist", { name: "Settings sections" });
    expect(within(nav).queryByRole("tab", { name: "AI" })).toBeNull();
  });

  // spec: UI-004 [M4]
  it("die Tab-Leiste lädt neu, solange der Katalog nicht zum Schalter passt", () => {
    expect(AI_TABS).toEqual(["AI", "AI Decisions", "AI Suggestions", "AI Search"]);
    expect(isAiTab("AI Search")).toBe(true);
    expect(isAiTab("AMP")).toBe(false);
    const plain = [{ id: "Internal" }, { id: "Link Metrics" }];
    const withAi = [...plain, { id: "AI" }, { id: "AI Decisions" }];
    expect(catalogMatchesPreview(plain, false)).toBe(true);
    expect(catalogMatchesPreview(withAi, false)).toBe(false);
    expect(catalogMatchesPreview(withAi, true)).toBe(true);
    expect(catalogMatchesPreview(plain, true)).toBe(false);
  });
});

describe("mit Schalter sichtbar", () => {
  beforeEach(async () => {
    await act(async () => {
      await setAiPreview(true);
    });
  });

  // spec: UI-004 [M4]
  it("Abschnitt KI und KI-Konfigurationsseite erscheinen; der Abschnitt zeigt den Leerzustand", async () => {
    expect(resolveSections().map((s) => s.id)).toContain("ai");
    expect(resolvePages().map((p) => p.id)).toContain("ai");
    await act(async () => {
      await setLanguage("de");
    });
    render(<SettingsFrame anchor="ai" />);
    const nav = screen.getByRole("tablist", { name: "Einstellungsbereiche" });
    expect(within(nav).getByRole("tab", { name: "KI" })).toBeVisible();
    await waitFor(() => expect(document.querySelector('[data-settings-section="ai"] [data-empty-state="NOT_APPLICABLE"]')).not.toBeNull());
  });

  // spec: UI-013 [M4]
  // Plan M4 3.3, Nachtrag 7: das Feld erscheint nur mit aktiven KI-Aufgaben für den Crawl (hier: Vorschlagstyp Title).
  it("Statusleistenfeld Kosten als Platzhalter in der UI-Sprache", async () => {
    core.handlers.set("config.get", () => ({ config: { ai: { enabled: true, active_questions: [], suggestions: { title: true } } } }));
    await act(async () => {
      await setLanguage("de");
    });
    const { unmount } = render(<AiCostField />);
    expect(await screen.findByTestId("ai-cost")).toHaveTextContent("Kosten: – / – USD");
    unmount();
    await act(async () => {
      await setLanguage("en");
    });
    render(<AiCostField />);
    expect(await screen.findByTestId("ai-cost")).toHaveTextContent("Cost: – / – USD");
  });

  // spec: UI-016
  it("jeder Bereich aus Plan M4 3.3 hat einen Ordner mit Leerzustand und deutschem und englischem Titel", async () => {
    const areas = aiAreas();
    expect(areas.map((a) => a.id)).toEqual([...AI_AREA_IDS]);
    for (const lang of ["de", "en"]) {
      await act(async () => {
        await setLanguage(lang);
      });
      for (const a of areas) {
        const { container, unmount } = render(<a.Component />);
        expect(container.querySelector(`[data-ai-area="${a.id}"] [data-empty-state]`), a.id).not.toBeNull();
        expect(container.textContent, `${a.id} (${lang})`).not.toMatch(/ai-[a-z]+-title/);
        unmount();
      }
    }
  });
});

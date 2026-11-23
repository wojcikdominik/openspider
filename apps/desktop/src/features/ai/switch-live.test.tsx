// M4-F02, Plan M4 3.3 (5) und (6): Das Umschalten von „KI (Vorschau)“ wirkt in offenen Fenstern sofort und in beiden Richtungen;
// mit eingeschaltetem Schalter steht die KI-Konfigurationsseite auch im Menü „Konfiguration ▾“, ausgeschaltet an keiner Stelle.
// Fixture: Tab-Zustand wie nach `useTabs.load()` mit gleichzeitiger Navigation (Katalog ohne KI-Tabs, Reihenfolge noch mit
// ihnen, KI-Tab gewählt) bzw. die Seitenliste aus `KNOWN_PAGES` und der KI-Registrierung.
import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { makeT } from "../../i18n";
import { KNOWN_PAGES } from "../config/frame/types";
import { configEntries } from "../toptabs/GroupNav";
import { useTabs } from "../../state/tabs";
import type { TabInfo } from "../../ipc/gen/TabInfo";
import { AI_TABS } from "./preview";
import { pruneStaleAiTabs, useAiTabsSync } from "./tabs";

const tab = (id: string, defaultOrder: number) => ({ id, defaultOrder }) as unknown as TabInfo;
const PLAIN = ["Internal", "External", "Custom Search", "Custom Extraction"];

function Sync() {
  useAiTabsSync();
  return null;
}

afterEach(() => {
  cleanup();
  useTabs.setState({ catalog: [], order: [], hidden: [], active: "Internal", groupActive: {}, loaded: false, view: "tabs" });
});

describe("Ausschalten im offenen Fenster", () => {
  // spec: UI-004 [M4]
  it("entfernt KI-Tabs, die der Katalog nicht mehr liefert, und fällt vom gewählten KI-Tab auf einen vorhandenen Tab zurück", () => {
    useTabs.setState({
      loaded: true,
      view: "tabs",
      catalog: PLAIN.map(tab),
      order: [...PLAIN, ...AI_TABS],
      hidden: ["AI Search"],
      active: "AI",
      groupActive: { urls: "Internal", custom: "AI" },
    });
    render(<Sync />);
    const s = useTabs.getState();
    expect(s.order).toEqual(PLAIN);
    expect(s.hidden).toEqual([]);
    expect(s.active).toBe("Custom Search");
    expect(s.groupActive).toEqual({ urls: "Internal", custom: "Custom Search" });
  });

  // spec: UI-004 [M4]
  it("räumt auch nach einem späteren Laden mit verwaisten KI-Tabs auf und lässt vorhandene KI-Tabs stehen", () => {
    useTabs.setState({ loaded: true, catalog: [...PLAIN, ...AI_TABS].map(tab), order: [...PLAIN, ...AI_TABS], active: "AI Decisions" });
    render(<Sync />);
    expect(useTabs.getState().order).toEqual([...PLAIN, ...AI_TABS]);
    expect(useTabs.getState().active).toBe("AI Decisions");
    // Wie `load()` bei gleichzeitiger Navigation: neuer Katalog, alte Reihenfolge.
    act(() => {
      useTabs.setState({ catalog: PLAIN.map(tab) });
    });
    expect(useTabs.getState().order).toEqual(PLAIN);
    expect(useTabs.getState().active).toBe("Custom Search");
  });

  // spec: UI-004 [M4]
  it("ohne Tab derselben Gruppe fällt die Auswahl auf Internal; andere Tabs bleiben unberührt", () => {
    useTabs.setState({ loaded: true, catalog: ["Internal", "External"].map(tab), order: ["Internal", "External", "AI"], active: "AI", hidden: ["External"] });
    pruneStaleAiTabs();
    expect(useTabs.getState().active).toBe("Internal");
    expect(useTabs.getState().order).toEqual(["Internal", "External"]);
    expect(useTabs.getState().hidden).toEqual(["External"]);
  });
});

describe("Menü „Konfiguration ▾“", () => {
  // spec: UI-004 [M4]
  it("nennt die KI-Seite nur mit eingeschaltetem Schalter, an derselben Stelle wie im Konfigurationsdialog", () => {
    for (const lang of ["en", "de"]) {
      const t = makeT(lang);
      const off = configEntries(t, false);
      const on = configEntries(t, true);
      expect(off.map((e) => e.id)).not.toContain("config-ai");
      expect(off).toHaveLength(KNOWN_PAGES.length);
      expect(on).toHaveLength(KNOWN_PAGES.length + 1);
      const ids = on.map((e) => e.id);
      expect(ids.filter((id) => id !== "config-ai")).toEqual(off.map((e) => e.id));
      // Reihenfolge wie im Dialog (order 175): nach „analysis“ (170), vor „change-detection“ (180).
      expect(ids.indexOf("config-ai")).toBe(ids.indexOf("config-analysis") + 1);
      expect(on.find((e) => e.id === "config-ai")?.label).toBe(lang === "de" ? "KI" : "AI");
    }
  });
});

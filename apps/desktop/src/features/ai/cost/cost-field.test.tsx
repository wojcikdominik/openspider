// M4-F02, UI-013 [M4] und Plan M4 3.3 Nachtrag 7: Feld `Kosten` nur mit Schalter „KI (Vorschau)“ an UND aktiven KI-Aufgaben für den
// Crawl des Fensters (Festlegung in `./tasks`). Fixture: MockCore mit Standard-Einstellungen; `config.get` liefert die Konfiguration
// des Fensters (ohne bzw. mit aktiven KI-Aufgaben).
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import { usePrefs } from "../../../state/prefs";
import { DEFAULT_APPEARANCE } from "../../../ui/tokens/theme";
import { openConfig, closeConfig } from "../../config/frame/store";
import { setAiPreview } from "../preview";
import { AiCostField } from "./CostField";
import { aiTasksActive } from "./tasks";

let core: MockCore;
let ai: Record<string, unknown>;

const NO_TASKS = { enabled: false, active_questions: [], suggestions: { title: false, meta: false, h1: false, alt: false } };

beforeEach(async () => {
  core = new MockCore();
  ai = structuredClone(NO_TASKS);
  core.handlers.set("config.get", () => ({ schemaVersion: 1, generator: "mock", config: { ai } }));
  setClient(createClient(core.transport("cost")));
  usePrefs.setState({ loaded: false, dto: null, appearance: DEFAULT_APPEARANCE, language: "en", languageSetting: "system" });
  await usePrefs.getState().load();
});
afterEach(() => {
  cleanup();
  closeConfig();
  setClient(null);
});

const configReads = () => core.calls.filter((c) => c.cmd === "config.get").length;

describe("aktive KI-Aufgaben", () => {
  // spec: UI-013 [M4]
  it("heißt: ai.enabled und mindestens eine aktive Frage oder ein Vorschlagstyp", () => {
    expect(aiTasksActive(undefined)).toBe(false);
    expect(aiTasksActive({ ai: NO_TASKS })).toBe(false);
    expect(aiTasksActive({ ai: { ...NO_TASKS, enabled: true } })).toBe(false);
    expect(aiTasksActive({ ai: { ...NO_TASKS, active_questions: ["q1"] } })).toBe(false);
    expect(aiTasksActive({ ai: { ...NO_TASKS, enabled: true, active_questions: ["q1"] } })).toBe(true);
    for (const k of ["title", "meta", "h1", "alt"]) {
      expect(aiTasksActive({ ai: { ...NO_TASKS, enabled: true, suggestions: { [k]: true } } }), k).toBe(true);
    }
  });
});

describe("Feld Kosten", () => {
  // spec: UI-013 [M4]
  it("Schalter aus: kein Feld, auch mit aktiven KI-Aufgaben, und kein config.get", async () => {
    ai = { ...NO_TASKS, enabled: true, active_questions: ["q1"] };
    render(<AiCostField />);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(screen.queryByTestId("ai-cost")).toBeNull();
    expect(configReads()).toBe(0);
  });

  // spec: UI-013 [M4]
  it("Schalter an ohne aktive KI-Aufgaben: kein Feld", async () => {
    await act(async () => {
      await setAiPreview(true);
    });
    render(<AiCostField />);
    await waitFor(() => expect(configReads()).toBeGreaterThan(0));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(screen.queryByTestId("ai-cost")).toBeNull();
  });

  // spec: UI-013 [M4]
  it("Schalter an mit aktiven KI-Aufgaben: Feld; es folgt Konfiguration und Schalter live", async () => {
    ai = { ...NO_TASKS, enabled: true, suggestions: { title: true } };
    await act(async () => {
      await setAiPreview(true);
    });
    render(<AiCostField />);
    expect(await screen.findByTestId("ai-cost")).toHaveTextContent("Cost: – / – USD");

    // Konfiguration geändert und gespeichert (Dialog geschlossen): Feld weg.
    act(() => openConfig());
    ai = structuredClone(NO_TASKS);
    act(() => closeConfig());
    await waitFor(() => expect(screen.queryByTestId("ai-cost")).toBeNull());

    // Wieder aktiv (z. B. aus einem anderen Fenster): das Fenster liest beim Fokus nach.
    ai = { ...NO_TASKS, enabled: true, active_questions: ["q1"] };
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(await screen.findByTestId("ai-cost")).toBeVisible();

    // Schalter aus: sofort kein Feld.
    await act(async () => {
      await setAiPreview(false);
    });
    expect(screen.queryByTestId("ai-cost")).toBeNull();
  });
});

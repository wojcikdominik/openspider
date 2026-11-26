// M4-U01: Statusleistenfeld `Kosten` (UI-013 [M4]) mit Werten aus dem `BudgetLedger` (`ai.costBudget`), Tooltip mit Aufteilung,
// Warnschwelle (AI-013) und Sprachwechsel ohne Neustart; aktive KI-Aufgaben einschließlich Embeddings und eigener Prompts
// (Nutzerentscheidung 24.11.2026). Fixture: MockCore mit `config.get` und `ai.costBudget` (3,21 von 5,00 USD).
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { setLanguage, makeT } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import type { AiCostSnapshot } from "../../../ipc/gen/AiCostSnapshot";
import { MockCore } from "../../../ipc/transport/mock";
import { usePrefs } from "../../../state/prefs";
import { DEFAULT_APPEARANCE } from "../../../ui/tokens/theme";
import { setAiPreview } from "../preview";
import { AiCostField } from "./CostField";
import { fieldText, parseUsd, tooltipLines, usd, warningReached } from "./format";
import { aiTasksActive, warningPercent } from "./tasks";

const SNAP: AiCostSnapshot = { spentMicroUsd: 3_210_000, budgetMicroUsd: 5_000_000, aiMicroUsd: 3_000_000, dataForSeoMicroUsd: 0, reservedMicroUsd: 210_000 };

let core: MockCore;
let config: Record<string, unknown>;
let snapshot: AiCostSnapshot;

beforeEach(async () => {
  core = new MockCore();
  config = { ai: { enabled: true, active_questions: [], suggestions: { title: true }, budget: { warning_percent: 80 } } };
  snapshot = structuredClone(SNAP);
  core.handlers.set("config.get", () => ({ schemaVersion: 2, generator: "mock", config }));
  core.handlers.set("ai.costBudget", () => snapshot);
  setClient(createClient(core.transport("cost-u01")));
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

describe("Format (UI-013)", () => {
  // spec: UI-013 [M4]
  it("verbraucht 3,21 USD bei Budget 5 USD: deutsch mit Komma, englisch mit Punkt, je 4 Nachkommastellen", () => {
    const s: AiCostSnapshot = { ...SNAP, spentMicroUsd: 3_210_000 };
    expect(fieldText(makeT("de"), s, "de")).toBe("Kosten: 3,2100 / 5,0000 USD");
    expect(fieldText(makeT("en"), s, "en")).toBe("Cost: 3.2100 / 5.0000 USD");
    expect(fieldText(makeT("de"), { ...s, budgetMicroUsd: null }, "de")).toBe("Kosten: 3,2100 USD (unbegrenzt)");
    expect(fieldText(makeT("en"), { ...s, budgetMicroUsd: null }, "en")).toBe("Cost: 3.2100 USD (unlimited)");
    expect(usd(1, "en")).toBe("0.0000");
    expect(usd(1_234_500_000, "de")).toBe("1.234,5000");
  });

  // spec: UI-013 [M4]
  it("Tooltip mit Aufteilung KI / DataForSEO / offene Reservierungen im selben Zahlenformat", () => {
    expect(tooltipLines(makeT("de"), SNAP, "de", false, 80)).toEqual(["KI: 3,0000 USD", "DataForSEO: 0,0000 USD", "Offene Reservierungen: 0,2100 USD"]);
    expect(tooltipLines(makeT("en"), SNAP, "en", true, 80)).toEqual([
      "AI: 3.0000 USD",
      "DataForSEO: 0.0000 USD",
      "Open reservations: 0.2100 USD",
      "Warning threshold reached (80 % of the crawl budget)",
    ]);
  });

  // spec: UI-013 [M4]
  it("Warnschwelle auf Verbrauch inklusive Reservierungen; Eingaben in der UI-Sprache", () => {
    expect(warningReached({ ...SNAP, spentMicroUsd: 4_000_000 }, 80)).toBe(true);
    expect(warningReached({ ...SNAP, spentMicroUsd: 3_999_999 }, 80)).toBe(false);
    expect(warningReached({ ...SNAP, budgetMicroUsd: null }, 80)).toBe(false);
    expect(parseUsd("5,00", "de")).toBe(5_000_000);
    expect(parseUsd("1.000,5", "de")).toBe(1_000_500_000);
    expect(parseUsd("0.0001", "en")).toBe(100);
    expect(parseUsd("abc", "en")).toBeNull();
  });

  // spec: UI-013 [M4]
  it("aktive KI-Aufgaben: auch Embeddings und eigene Prompts (Nutzerentscheidung 24.11.2026)", () => {
    expect(aiTasksActive({ embeddings: { enabled: true } })).toBe(true);
    expect(aiTasksActive({ ai: { prompts: [{ name: "p1" }] } })).toBe(true);
    expect(aiTasksActive({ ai: { prompts: [{ name: "p1", enabled: false }] } })).toBe(false);
    expect(aiTasksActive({ ai: { prompts: [] }, embeddings: { enabled: false } })).toBe(false);
    expect(warningPercent({ ai: { budget: { warning_percent: 75 } } })).toBe(75);
    expect(warningPercent({})).toBe(80);
  });
});

describe("Feld Kosten mit Werten", () => {
  // spec: UI-013 [M4]
  it("zeigt den Ledger-Stand; ein Sprachwechsel ändert das Format ohne Neustart", async () => {
    await act(async () => {
      await setAiPreview(true);
    });
    render(<AiCostField />);
    const field = await screen.findByTestId("ai-cost");
    await waitFor(() => expect(field).toHaveTextContent("Cost: 3.2100 / 5.0000 USD"));
    expect(field.getAttribute("title")).toBe("AI: 3.0000 USD\nDataForSEO: 0.0000 USD\nOpen reservations: 0.2100 USD");
    await act(async () => {
      await setLanguage("de");
    });
    expect(screen.getByTestId("ai-cost")).toHaveTextContent("Kosten: 3,2100 / 5,0000 USD");
    expect(core.calls.filter((c) => c.cmd === "ai.costBudget").length).toBeGreaterThan(0);
  });

  // spec: UI-013 [M4]
  it("Warnschwelle erreicht: Kennzeichen und Tooltip-Zeile", async () => {
    snapshot = { ...SNAP, spentMicroUsd: 4_200_000 };
    await act(async () => {
      await setAiPreview(true);
    });
    render(<AiCostField />);
    await waitFor(() => expect(screen.getByTestId("ai-cost")).toHaveAttribute("data-warning", "true"));
    expect(screen.getByTestId("ai-cost").getAttribute("title")).toContain("Warning threshold reached (80 % of the crawl budget)");
  });

  // spec: UI-013 [M4]
  it("nur Embeddings aktiv: das Feld erscheint ebenfalls", async () => {
    config = { ai: { enabled: false }, embeddings: { enabled: true } };
    await act(async () => {
      await setAiPreview(true);
    });
    render(<AiCostField />);
    await waitFor(() => expect(screen.getByTestId("ai-cost")).toHaveTextContent("Cost: 3.2100 / 5.0000 USD"));
  });

  // spec: UI-013 [M4]
  it("Schalter KI (Vorschau) aus: kein Feld, kein Dialog-Host und kein ai.*-Command", async () => {
    render(<AiCostField />);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 30));
    });
    expect(screen.queryByTestId("ai-cost")).toBeNull();
    expect(document.querySelector("[role='dialog']")).toBeNull();
    expect(core.calls.filter((c) => c.cmd.startsWith("ai.") || c.cmd === "config.get")).toEqual([]);
  });
});

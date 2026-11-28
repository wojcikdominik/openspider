// Unabhängige Abnahme M4-D05 (Oberfläche): Einstieg eines KI-Laufs `startAiRun` (Einwilligung → Kostenbestätigung →
// `ai.decisionsRun`), Meldung bei unterbrochenem Crawl in de/en und Sichtbarkeit der Option „KI-Eingaben speichern“.
// Erwartungen ausschließlich aus 08 AI-012 („startet kein KI-Request ohne Klick auf ‚Starten‘“), AI-018 (Einwilligung vor
// dem ersten Senden), AI-020 („Kein KI-Lauf auf einem unterbrochenen Crawl“: „Crawl ist unterbrochen – zuerst fortsetzen
// oder stoppen“, en „Crawl was interrupted – resume or stop it first“), AI-032 Nachtrag K4-24 (Option „KI-Eingaben
// speichern“, Standard an, bei ausgeschaltetem Schalter nicht sichtbar). Die IPC-Aufrufe sind Attrappen; ob ein Request
// hinausgeht, prüfen die Rust-Abnahmetests gegen ai-mock.
// acceptance: independent
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./consent/ConsentDialog", () => ({ requestConsent: vi.fn() }));
vi.mock("./cost/EstimateDialog", () => ({ requestCostConfirmation: vi.fn() }));
vi.mock("../../ipc/ai", () => ({
  ai: {
    decisionsRun: vi.fn(),
    costBudget: vi.fn(async () => ({ spentMicroUsd: 0, budgetMicroUsd: 5_000_000, aiMicroUsd: 0, dataForSeoMicroUsd: 0, reservedMicroUsd: 0 })),
  },
}));

import { makeT, setLanguage } from "../../i18n";
import { ai } from "../../ipc/ai";
import { createClient, setClient } from "../../ipc/client";
import { localError } from "../../ipc/errors";
import { MockCore } from "../../ipc/transport/mock";
import { usePrefs } from "../../state/prefs";
import { DEFAULT_APPEARANCE } from "../../ui/tokens/theme";
import { ConfigProvider, type ConfigContextValue } from "../config/frame/context";
import { resolvePages } from "../config/frame/registry";
import { requestConsent } from "./consent/ConsentDialog";
import { requestCostConfirmation } from "./cost/EstimateDialog";
import { setAiPreview } from "./preview";
import { runMessage, startAiRun } from "./run";
import { StoreInputsSetting } from "./settings/StoreInputs";

const consent = vi.mocked(requestConsent);
const confirmation = vi.mocked(requestCostConfirmation);
const decisionsRun = vi.mocked(ai.decisionsRun);

let core: MockCore;
beforeEach(async () => {
  vi.clearAllMocks();
  core = new MockCore();
  setClient(createClient(core.transport("acceptance-d05")));
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

describe("startAiRun: kein Request vor Einwilligung und „Starten“", () => {
  // spec: AI-012 [M4]
  // spec: AI-018 [M4]
  it("ohne Einwilligung weder Kostenvoranschlag noch ai.decisionsRun", async () => {
    consent.mockResolvedValue(false);
    confirmation.mockResolvedValue(true);
    const result = await startAiRun();
    expect(result).toEqual({ state: "declined", step: "consent" });
    expect(confirmation).not.toHaveBeenCalled();
    expect(decisionsRun).not.toHaveBeenCalled();
  });

  // spec: AI-012 [M4]
  it("Kostenvoranschlag ohne „Starten“ → kein ai.decisionsRun", async () => {
    consent.mockResolvedValue(true);
    confirmation.mockResolvedValue(false);
    const result = await startAiRun();
    expect(result).toEqual({ state: "declined", step: "confirmation" });
    expect(decisionsRun).not.toHaveBeenCalled();
  });

  // spec: AI-012 [M4]
  // spec: AI-020 [M4]
  it("nach Einwilligung und „Starten“ genau ein ai.decisionsRun mit erteilter Bestätigung (K4-27: confirmed_at)", async () => {
    consent.mockResolvedValue(true);
    confirmation.mockResolvedValue(true);
    decisionsRun.mockResolvedValue({ opId: "op-1" } as never);
    const result = await startAiRun();
    expect(result.state).toBe("started");
    expect(decisionsRun).toHaveBeenCalledTimes(1);
    expect(decisionsRun.mock.calls[0][0]).toMatchObject({ confirmed: true });
    expect(consent.mock.invocationCallOrder[0]).toBeLessThan(confirmation.mock.invocationCallOrder[0]);
    expect(confirmation.mock.invocationCallOrder[0]).toBeLessThan(decisionsRun.mock.invocationCallOrder[0]);
  });
});

describe("Unterbrochener Crawl", () => {
  // spec: AI-020 [M4]
  // spec: AI-039 [M4]
  it("Ablehnung des Kerns zeigt den Hinweis in der UI-Sprache (de/en)", async () => {
    consent.mockResolvedValue(true);
    confirmation.mockResolvedValue(true);
    const error = { ...localError("InvalidInput", "err-ai-run-interrupted"), detail: "Crawl ist unterbrochen – zuerst fortsetzen oder stoppen" };
    decisionsRun.mockRejectedValue(error);
    await act(async () => {
      await setLanguage("de");
    });
    const result = await startAiRun();
    expect(result.state).toBe("failed");
    expect(result.state === "failed" && result.message).toBe("Crawl ist unterbrochen – zuerst fortsetzen oder stoppen");
    expect(runMessage(error, makeT("en"))).toBe("Crawl was interrupted – resume or stop it first");
    expect(runMessage(error, makeT("de"))).toBe("Crawl ist unterbrochen – zuerst fortsetzen oder stoppen");
  });

  // spec: AI-020 [M4]
  it("Hinweis bei fehlenden gespeicherten Seiteninhalten (Skipped (input unavailable)) in de/en vorhanden", () => {
    expect(makeT("de")("ai-run-input-unavailable")).toBe("Für diese Frage fehlen gespeicherte Seiteninhalte – Seiten erneut crawlen");
    expect(makeT("en")("ai-run-input-unavailable")).toBe("Stored page content is missing for this question – crawl the pages again");
  });
});

describe("Option „KI-Eingaben speichern“", () => {
  // spec: AI-032 [M4]
  it("ohne Schalter KI (Vorschau) nicht sichtbar, mit Schalter auf der Seite KI, Standard an", async () => {
    expect(resolvePages().map((p) => p.id)).not.toContain("ai");
    await act(async () => {
      await setAiPreview(true);
    });
    expect(resolvePages().map((p) => p.id)).toContain("ai");
    await act(async () => {
      await setLanguage("de");
    });
    const values: Record<string, unknown> = {};
    const set = vi.fn((path: string, v: unknown) => {
      values[path] = v;
    });
    const ctx = { draft: { get: (path: string) => values[path], set } } as unknown as ConfigContextValue;
    render(
      <ConfigProvider value={ctx}>
        <StoreInputsSetting />
      </ConfigProvider>,
    );
    const box = screen.getByRole("checkbox", { name: "KI-Eingaben speichern" });
    expect(box).toBeChecked();
    fireEvent.click(box);
    expect(set).toHaveBeenCalledWith("ai.store_inputs", false);
  });
});

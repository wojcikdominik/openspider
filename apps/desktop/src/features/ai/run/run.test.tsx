// M4-D05: Einstieg eines KI-Laufs (`startAiRun`) – Einwilligung → Kostenbestätigung → `ai.decisionsRun` mit `confirmed`; ohne
// „Starten“ kein Lauf (AI-012); Meldungen des Kerns in de/en (AI-020: unterbrochener Crawl, fremder Writer); Option
// „KI-Eingaben speichern“ auf der Konfigurationsseite KI (AI-032, Nachtrag K4-24). Fixture: MockCore in der Form des Kerns
// (`crates/os-app/src/commands/{ai_run,ai_cost,ai_consent}.rs`).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { makeT, setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { IpcError, localError } from "../../../ipc/errors";
import { MockCore } from "../../../ipc/transport/mock";
import { ConfigProvider, type ConfigContextValue } from "../../config/frame/context";
import { CostEstimateDialog } from "../cost/EstimateDialog";
import { aiConfigPages } from "../settings/index";
import { StoreInputsSetting } from "../settings/StoreInputs";
import { runMessage, startAiRun, useAiRunMessage } from "./index";

const ESTIMATE = (required: boolean) => ({
  sections: [{ id: "ai", rows: [], totals: { lowMicroUsd: 1, expectedMicroUsd: 2, highMicroUsd: 3 } }],
  totals: { lowMicroUsd: 1, expectedMicroUsd: 2, highMicroUsd: 3 },
  budget: { budgetMicroUsd: 5_000_000, consumedMicroUsd: 0, remainingMicroUsd: 5_000_000, warning: false, halted: false },
  thresholdMicroUsd: 100,
  confirmationRequired: required,
});

let core: MockCore;
let calls: string[];
let runArgs: unknown[];
let required: boolean;

beforeEach(() => {
  calls = [];
  runArgs = [];
  required = false;
  core = new MockCore();
  const track = (name: string, f: (args: unknown) => unknown) =>
    core.handlers.set(name, (args) => {
      calls.push(name);
      return f(args);
    });
  track("ai.costBudget", () => ({ spentMicroUsd: 0, budgetMicroUsd: 5_000_000, aiMicroUsd: 0, dataForSeoMicroUsd: 0, reservedMicroUsd: 0 }));
  track("ai.consentStatus", () => ({ columns: [], rows: [], requests: [], redaction: true, policy: "unrestricted", consentedProviders: [], csv: "" }));
  track("ai.costEstimate", () => ESTIMATE(required));
  track("ai.decisionsRun", (args) => {
    runArgs.push(args);
    return { opId: "op-1", labelKey: "op-ai-decisions", cancellable: true };
  });
  setClient(createClient(core.transport("ai-run")));
});

afterEach(async () => {
  cleanup();
  setClient(null);
  useAiRunMessage.setState({ message: null, error: null });
  await act(async () => {
    await setLanguage("en");
  });
});

describe("startAiRun", () => {
  // spec: AI-012 [M4]
  // spec: AI-018 [M4]
  it("ohne Bestätigungspflicht: Einwilligung, Voranschlag, dann ai.decisionsRun mit confirmed", async () => {
    const result = await startAiRun();
    expect(result.state).toBe("started");
    expect(calls).toEqual(["ai.costBudget", "ai.consentStatus", "ai.costEstimate", "ai.decisionsRun"]);
    expect(runArgs[0]).toMatchObject({ tasks: [], confirmed: true });
  });

  // spec: AI-012 [M4]
  it("über der Schwelle: Dialog; ohne „Starten“ kein Lauf, mit „Starten“ der Lauf", async () => {
    required = true;
    render(<CostEstimateDialog />);
    const cancelled = startAiRun();
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(await cancelled).toEqual({ state: "declined", step: "confirmation" });
    expect(calls).not.toContain("ai.decisionsRun");
    const started = startAiRun();
    fireEvent.click(await screen.findByRole("button", { name: "Start" }));
    expect((await started).state).toBe("started");
    expect(runArgs).toHaveLength(1);
  });

  // spec: AI-020 [M4]
  it("unterbrochener Crawl: Meldung des Kerns in der UI-Sprache, kein Lauf", async () => {
    core.handlers.set("ai.decisionsRun", () => {
      throw new IpcError({ ...localError("InvalidInput", "err-ai-run-interrupted"), detail: "Crawl ist unterbrochen – zuerst fortsetzen oder stoppen" });
    });
    const result = await startAiRun();
    expect(result.state).toBe("failed");
    expect(result.state === "failed" && result.message).toBe("Crawl was interrupted – resume or stop it first");
    expect(useAiRunMessage.getState().message).toBe("Crawl was interrupted – resume or stop it first");
    expect(runMessage({ ...localError("InvalidInput", "err-ai-run-interrupted") }, makeT("de"))).toBe(
      "Crawl ist unterbrochen – zuerst fortsetzen oder stoppen",
    );
  });

  // spec: AI-020 [M4]
  it("fremder Writer: Hinweis mit Art und PID (de/en)", () => {
    const error = localError("Busy", "err-ai-run-foreign", { kind: "cli", pid: "4242" });
    expect(runMessage(error, makeT("en"))).toBe("started by cli (PID 4242) – control only there");
    expect(runMessage(error, makeT("de"))).toBe("gestartet von cli (PID 4242) – Steuerung nur dort");
  });

  // spec: AI-020 [M4]
  it("Hinweis zu Skipped (input unavailable) in de/en", () => {
    expect(makeT("de")("ai-run-input-unavailable")).toBe("Für diese Frage fehlen gespeicherte Seiteninhalte – Seiten erneut crawlen");
    expect(makeT("en")("ai-run-input-unavailable")).toBe("Stored page content is missing for this question – crawl the pages again");
  });
});

function draftContext(values: Record<string, unknown>) {
  const set = vi.fn((path: string, v: unknown) => {
    values[path] = v;
  });
  const draft = { get: (path: string) => values[path], set };
  return { value: { draft } as unknown as ConfigContextValue, set };
}

describe("Option „KI-Eingaben speichern“", () => {
  // spec: AI-032 [M4]
  it("Standard an, Abwahl schreibt ai.store_inputs; Teil der Konfigurationsseite KI", async () => {
    await act(async () => {
      await setLanguage("de");
    });
    const ctx = draftContext({});
    render(<ConfigProvider value={ctx.value}><StoreInputsSetting /></ConfigProvider>);
    const box = screen.getByRole("checkbox", { name: "KI-Eingaben speichern" });
    expect(box).toBeChecked();
    fireEvent.click(box);
    await waitFor(() => expect(ctx.set).toHaveBeenCalledWith("ai.store_inputs", false));
    cleanup();
    core.handlers.set("ai.settingsGet", () => ({ tasks: [], keys: [], listsLoaded: false, policy: "unrestricted", budget: null, crawlBudget: null, eu: [], advanced: [], confirmationThresholdMicroUsd: 1_000_000, redactPersonalData: true, redactionPatterns: [], previewEnabled: true, policyAvailable: false }));
    core.handlers.set("ai.settingsSet", () => ({ tasks: [] }));
    const page = aiConfigPages.find((p) => p.id === "ai")!;
    const page_ctx = draftContext({ ai: { tasks: {} }, "ai.budget": { crawl: 5_000_000, unlimited: false, areas: {}, tasks: {}, warning_percent: 80 } });
    const { container } = render(<ConfigProvider value={page_ctx.value}><page.Component /></ConfigProvider>);
    expect(container.querySelector("[data-ai-store-inputs-setting]")).not.toBeNull();
  });
});

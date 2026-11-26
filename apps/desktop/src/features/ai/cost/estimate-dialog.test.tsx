// M4-U01: Dialog „Kostenvoranschlag“ (AI-012) mit Abschnitt „KI“ (Spalte `Reasoning` zwischen `Model` und `Items`, Nachtrag
// 24.11.2026), Summenzeile, Crawl-Budget/Verbrauch/Rest und Bestätigungsregel. Fixture: `e2e/ai-cost/fixtures/estimate.json`
// (dieselbe Datei erzeugt der Kern in `crates/os-app/tests/ai_cost_budget.rs`).
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import type { CostEstimateView } from "../settings/api";
import { AI_ESTIMATE_COLUMNS, CostEstimateDialog, EstimateBody, registerEstimateSection, requestCostConfirmation } from "./EstimateDialog";

const FIXTURE = JSON.parse(readFileSync("e2e/ai-cost/fixtures/estimate.json", "utf8")) as CostEstimateView;

let core: MockCore;
beforeEach(() => {
  core = new MockCore();
  setClient(createClient(core.transport("estimate")));
});
afterEach(async () => {
  cleanup();
  setClient(null);
  await act(async () => {
    await setLanguage("en");
  });
});

describe("Abschnitt KI", () => {
  // spec: AI-012 [M4]
  it("Spalten in der Reihenfolge von AI-012 mit Reasoning zwischen Model und Items", () => {
    render(<EstimateBody view={FIXTURE} />);
    const headers = screen.getAllByRole("columnheader").map((h) => h.textContent);
    expect(headers).toEqual([...AI_ESTIMATE_COLUMNS]);
    expect(headers.indexOf("Reasoning")).toBe(headers.indexOf("Model") + 1);
    expect(headers.indexOf("Items")).toBe(headers.indexOf("Reasoning") + 1);
  });

  // spec: AI-012 [M4]
  it("zeigt effektive Stufe, Beträge in der UI-Sprache, nicht ausführbar mit Grund, Summe und Budget", async () => {
    await act(async () => {
      await setLanguage("de");
    });
    const { container } = render(<EstimateBody view={FIXTURE} />);
    const title = container.querySelector('tr[data-task="suggest-title"]')!;
    expect(title.querySelector('[data-col="Reasoning"]')!.textContent).toBe("low");
    expect(title.textContent).toContain("0,0075 / 0,0100 / 0,0125");
    const decision = container.querySelector('tr[data-task="decision"]')!;
    expect(decision.querySelector('[data-col="Reasoning"]')!.textContent).toBe("");
    const ask = container.querySelector('tr[data-task="ask-sql"]')!;
    expect(ask.querySelector('[data-col="Executable"]')!.textContent).toBe("nein (Missing API key)");
    expect(ask.textContent).toContain("– / – / –");
    expect(container.querySelector("[data-ai-estimate-total]")!.textContent).toBe("0,0083 / 0,0111 / 0,0139 USD");
    expect(container.querySelector("[data-ai-estimate-budget]")!.textContent).toBe("5,0000 USD");
    expect(container.querySelector("[data-ai-estimate-consumed]")!.textContent).toBe("3,2100 USD");
    expect(container.querySelector("[data-ai-estimate-remaining]")!.textContent).toBe("1,7900 USD");
    expect(container.querySelector("[data-ai-estimate-confirm]")!.textContent).toContain("0,0001 USD");
  });

  // spec: AI-012 [M4]
  it("ein weiterer Abschnitt (M5: DataForSEO) lässt sich anmelden; unbekannte Abschnitte erscheinen nicht", () => {
    const view: CostEstimateView = { ...FIXTURE, sections: [...FIXTURE.sections, { id: "dataforseo", rows: [], totals: { lowMicroUsd: 1, expectedMicroUsd: 2, highMicroUsd: 2 } }, { id: "unknown", rows: [], totals: { lowMicroUsd: 0, expectedMicroUsd: 0, highMicroUsd: 0 } }] };
    registerEstimateSection("dataforseo", "ai-estimate-section-ai", () => <p data-dfs>DFS</p>);
    const { container } = render(<EstimateBody view={view} />);
    expect([...container.querySelectorAll("[data-section]")].map((s) => s.getAttribute("data-section"))).toEqual(["ai", "dataforseo"]);
  });
});

describe("Bestätigung vor dem Lauf", () => {
  // spec: AI-012 [M4]
  it("über der Schwelle: kein Start ohne Klick; Starten gibt frei, Abbrechen nicht", async () => {
    core.handlers.set("ai.costEstimate", () => FIXTURE);
    render(<CostEstimateDialog />);
    let started: boolean | null = null;
    void requestCostConfirmation().then((v) => (started = v));
    const dialog = await screen.findByRole("dialog", { name: "Cost estimate" });
    expect(started).toBeNull();
    fireEvent.click(within(dialog).getByRole("button", { name: "Start" }));
    await waitFor(() => expect(started).toBe(true));
    expect(screen.queryByRole("dialog")).toBeNull();

    let second: boolean | null = null;
    void requestCostConfirmation().then((v) => (second = v));
    const again = await screen.findByRole("dialog", { name: "Cost estimate" });
    fireEvent.click(within(again).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(second).toBe(false));
  });

  // spec: AI-012 [M4]
  it("unter Schwelle und halbem Restbudget: weiter ohne Dialog", async () => {
    core.handlers.set("ai.costEstimate", () => ({ ...FIXTURE, confirmationRequired: false }));
    render(<CostEstimateDialog />);
    await expect(requestCostConfirmation()).resolves.toBe(true);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(core.calls.filter((c) => c.cmd === "ai.costEstimate")).toHaveLength(1);
  });
});

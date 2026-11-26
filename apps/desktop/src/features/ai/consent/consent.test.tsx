// M4-U01: Dialog „Datenfluss bestätigen“ und Übersicht „KI-Datenfluss“ (AI-018; Spalte `Stored by Provider` nach dem Nachtrag
// 24.11.2026). Fixture: MockCore mit `ai.consentStatus`/`ai.consentGrant` in der Form des Kerns (`ai_consent.rs`).
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { setLanguage } from "../../../i18n";
import { createClient, setClient } from "../../../ipc/client";
import { MockCore } from "../../../ipc/transport/mock";
import type { ConsentView } from "../settings/api";
import { ConsentDialog, requestConsent } from "./ConsentDialog";
import { DataFlowOverview } from "./Overview";

const COLUMNS = ["Task", "Provider", "Model", "Host", "Region", "Fields Sent", "Max Chars", "Images", "Redaction", "Training Use (per provider policy)", "Stored by Provider", "Consent Given At"];

const row = (task: string, model: string, stored: string, fields: string[], labels: string, given: string) => ({
  key: { provider: "openai", task, fields },
  cells: [task, "openai", model, "api.openai.com", "", labels, fields.includes("page_text") ? "8000" : "", "no", "off", "no (source: https://developers.openai.com/api/docs/guides/your-data; verified 2026-11-24)", stored, given],
  piiNotice: fields.includes("page_text"),
  consentGiven: given !== "",
});

function view(granted: boolean): ConsentView {
  const decision = row("decision", "gpt-6-luna", "unknown", ["url", "title", "page_text"], "URL, Title, Page Text", granted ? "2026-11-26T09:00:00Z" : "");
  const title = row("suggest-title", "gpt-6-luna", "no (store: false)", ["url", "title", "h1"], "URL, Title, H1", granted ? "2026-11-26T09:00:00Z" : "");
  return {
    columns: COLUMNS,
    rows: [decision, title],
    requests: granted ? [] : [
      { row: decision, previousFields: null, addedFields: ["url", "title", "page_text"], piiNotice: true },
      { row: title, previousFields: ["url", "title"], addedFields: ["h1"], piiNotice: false },
    ],
    redaction: false,
    policy: "unrestricted",
    consentedProviders: granted ? ["openai"] : [],
    csv: `${COLUMNS.join(",")}\r\n`,
  };
}

let core: MockCore;
let granted: boolean;
beforeEach(() => {
  granted = false;
  core = new MockCore();
  core.handlers.set("ai.consentStatus", () => view(granted));
  core.handlers.set("ai.consentGrant", (args) => {
    const a = args as { grant?: unknown[]; revokeProvider?: string };
    if (a.grant) granted = true;
    if (a.revokeProvider) granted = false;
    return view(granted);
  });
  setClient(createClient(core.transport("consent")));
});
afterEach(async () => {
  cleanup();
  setClient(null);
  await act(async () => {
    await setLanguage("en");
  });
});

describe("Datenfluss bestätigen", () => {
  // spec: AI-018 [M4]
  it("zeigt alle Spalten mit Stored by Provider nach Training Use, die neuen Felder und bei Schwärzung aus den PII-Hinweis", async () => {
    render(<ConsentDialog />);
    void requestConsent();
    const dialog = await screen.findByRole("dialog", { name: "Confirm data flow" });
    const headers = within(dialog).getAllByRole("columnheader").map((h) => h.textContent);
    expect(headers).toEqual(COLUMNS);
    expect(headers.indexOf("Stored by Provider")).toBe(headers.indexOf("Training Use (per provider policy)") + 1);
    const cells = [...dialog.querySelectorAll('[data-col="Stored by Provider"]')].map((c) => c.textContent);
    expect(cells).toEqual(["unknown", "no (store: false)"]);
    expect(cells.every((c) => c !== "")).toBe(true);
    expect(within(dialog).getByText("Redaction of personal data: off")).toBeVisible();
    expect(within(dialog).getByText("suggest-title: newly added – H1 heading")).toBeVisible();
    expect(dialog.querySelector("[data-ai-pii-notice]")).not.toBeNull();
  });

  // spec: AI-018 [M4]
  it("ohne Bestätigung keine Einwilligung; Bestätigen speichert genau die gezeigten Zeilen", async () => {
    render(<ConsentDialog />);
    let cancelled: boolean | null = null;
    void requestConsent().then((v) => (cancelled = v));
    const first = await screen.findByRole("dialog", { name: "Confirm data flow" });
    fireEvent.click(within(first).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(cancelled).toBe(false));
    expect(core.calls.filter((c) => c.cmd === "ai.consentGrant")).toEqual([]);

    let ok: boolean | null = null;
    void requestConsent().then((v) => (ok = v));
    const dialog = await screen.findByRole("dialog", { name: "Confirm data flow" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(ok).toBe(true));
    const grants = core.calls.filter((c) => c.cmd === "ai.consentGrant").map((c) => c.args);
    expect(grants).toEqual([{ grant: [view(false).requests[0].row.key, view(false).requests[1].row.key] }]);
    // Danach keine offene Anfrage: weiter ohne Dialog.
    await expect(requestConsent()).resolves.toBe(true);
  });

  // spec: AI-018 [M4]
  it("deutsch vollständig: Titel, Schwärzung, Hinweis", async () => {
    await act(async () => {
      await setLanguage("de");
    });
    render(<ConsentDialog />);
    void requestConsent();
    const dialog = await screen.findByRole("dialog", { name: "Datenfluss bestätigen" });
    expect(within(dialog).getByText("Schwärzung personenbezogener Daten: aus")).toBeVisible();
    expect(within(dialog).getByText(/Impressum/)).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Bestätigen" })).toBeVisible();
  });
});

describe("Übersicht KI-Datenfluss", () => {
  // spec: AI-018 [M4]
  it("zeigt Einwilligungszeitpunkt, exportiert CSV mit der Spalte und widerruft je Anbieter", async () => {
    granted = true;
    const { container } = render(<DataFlowOverview />);
    await screen.findByText("AI data flow");
    await waitFor(() => expect(container.querySelectorAll("tbody tr")).toHaveLength(2));
    expect(container.querySelector('[data-col="Consent Given At"]')!.textContent).toBe("2026-11-26T09:00:00Z");
    const link = container.querySelector("[data-ai-csv-download]")!;
    expect(link.getAttribute("download")).toBe("ai-data-flow.csv");
    expect(decodeURIComponent(link.getAttribute("href")!)).toContain("Stored by Provider");
    fireEvent.click(screen.getByRole("button", { name: "Revoke consent for OpenAI" }));
    await waitFor(() => expect(core.calls.some((c) => c.cmd === "ai.consentGrant" && (c.args as { revokeProvider?: string }).revokeProvider === "openai")).toBe(true));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Revoke consent for OpenAI" })).toBeNull());
  });
});

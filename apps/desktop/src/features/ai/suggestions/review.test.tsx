// M4-V01: Prüfansicht der KI-Vorschläge (AI-076) und ihr Bereich am Tab „AI Suggestions“ (UI-004 [M4]); SERP-Vorschau
// des gewählten Kandidaten (UI-037, AI-076). Fixture: MockCore mit Antworten in der Form von `ai.suggest*` und
// `detail.serpMeasure`; Pixelwerte kommen nur aus dem Kern (UI-038).
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createClient, setClient } from "../../../ipc/client";
import { eventBus } from "../../../ipc/events";
import type { Tick } from "../../../ipc/gen/Tick";
import { MockCore } from "../../../ipc/transport/mock";
import { getT, setLanguage } from "../../../i18n";
import { usePrefs } from "../../../state/prefs";
import { useTabs } from "../../../state/tabs";
import { DEFAULT_APPEARANCE } from "../../../ui/tokens/theme";
import { useDetailSelection } from "../../detail/frame/selection";
import { displayText } from "../../detail/serp/model";
import { setAiPreview } from "../preview";
import { SuggestionsReviewHost } from "./host";
import { OUTPUT_LIMIT_STATUS, serpTexts, violationText, type SuggestDetail } from "./model";
import { ReviewPanel } from "./ReviewPanel";

const LONG = "Wanderschuhe für Damen und Herren im großen Vergleich mit Test";

function detail(status = "Valid"): SuggestDetail {
  return {
    urlId: 2,
    address: "http://ai.test:8080/b/",
    elements: [
      {
        element: "title",
        label: "Title",
        current: "Rucksäcke Shop",
        currentPixelWidth: 130,
        status,
        provisional: false,
        violations: [],
        review: "Open",
        accepted: null,
        model: "gpt-6-luna",
        generatedAt: "2026-11-26T09:00:00Z",
        focus: null,
        rounds: 1,
        candidates: [
          { index: 2, value: LONG, length: 61, pixelWidth: 570, valid: true, violations: [], round: 1, selected: true },
          { index: 1, value: "Leichte Wanderschuhe für Damen und Herren", length: 41, pixelWidth: 369, valid: false, violations: ["duplicate_candidate:http://ai.test:8080/a/"], round: 1, selected: false },
        ],
      },
      { element: "meta_description", label: "Meta Description", current: "Tagesrucksäcke im Überblick.", currentPixelWidth: 180, status: null, candidates: [] },
    ],
  };
}

let core: MockCore;
let current: SuggestDetail;

beforeEach(async () => {
  core = new MockCore();
  current = detail();
  core.handlers.set("ai.suggestDetail", () => structuredClone(current));
  core.handlers.set("ai.suggestCheck", (args) => {
    const value = (args as { value: string }).value;
    return { value, length: [...value].length, pixelWidth: 242, violations: value.length < 30 ? ["length<30"] : [], valid: value.length >= 30 };
  });
  core.handlers.set("ai.suggestReview", (args) => {
    const a = args as { action: string; urlIds: number[] };
    if (a.action === "acceptFirst") return { updated: a.urlIds.length, accepted: 2, skipped: 1, skippedUrlIds: [9], errors: [] };
    return { updated: 1, accepted: null, skipped: null, skippedUrlIds: null, errors: [] };
  });
  core.handlers.set("ai.suggestRecheck", () => ({ updated: 1 }));
  // Kürzung nach Pixelbreite im Kern: ab 561 px gekürzt am letzten ganzen Wort (cutAt in Zeichen).
  core.handlers.set("detail.serpMeasure", (args) => {
    const a = args as { title: string; description: string };
    const row = (text: string, max: number, px: number) => ({ chars: [...text].length, px, maxPx: max, remaining: max - px, truncated: px > max, cutAt: px > max ? 49 : null });
    return { rows: [row(a.title, 561, a.title === LONG ? 570 : 300), row(a.description, 985, 200)] };
  });
  setClient(createClient(core.transport("main")));
  usePrefs.setState({ loaded: false, dto: null, appearance: DEFAULT_APPEARANCE, language: "en", languageSetting: "system" });
  await usePrefs.getState().load();
  await act(async () => {
    await setLanguage("en");
  });
});

afterEach(() => {
  cleanup();
  setClient(null);
  useDetailSelection.getState().setSelection({ urlIds: [] });
});

const calls = (cmd: string) => core.calls.filter((c) => c.cmd === cmd).map((c) => c.args as Record<string, unknown>);

describe("Prüfansicht", () => {
  // spec: AI-076 [M4]
  // spec: AI-075 [M4]
  it("zeigt Kandidaten mit Länge, Pixelbreite aus dem Kern, Gültigkeit und Verstößen; übernimmt nur gültige", async () => {
    render(<ReviewPanel urlId={2} selected={[2]} />);
    expect(await screen.findByText(`Candidate 1: ${LONG}`)).toBeTruthy();
    expect(screen.getByText("61 characters · 570 px")).toBeTruthy();
    expect(screen.getByText("same as the suggestion for http://ai.test:8080/a/")).toBeTruthy();
    expect(screen.getAllByText("invalid")).toHaveLength(1);
    const accept = () => screen.getByRole("button", { name: "Accept" });
    const second = screen.getByRole("radio", { name: /Candidate 2/ });
    fireEvent.click(second);
    await waitFor(() => expect(second).toBeChecked());
    await waitFor(() => expect(accept()).toBeDisabled());
    fireEvent.click(screen.getByRole("radio", { name: /Candidate 1/ }));
    await waitFor(() => expect(accept()).not.toBeDisabled());
    fireEvent.click(accept());
    await waitFor(() => expect(calls("ai.suggestReview")).toHaveLength(1));
    expect(calls("ai.suggestReview")[0]).toMatchObject({ urlIds: [2], element: "title", action: "accept", candidate: 2 });
    await waitFor(() => expect(calls("ai.suggestDetail").length).toBeGreaterThan(1), { timeout: 2_000 });
  });

  // spec: AI-076 [M4]
  // Bearbeiten: Live-Prüfung im Kern bei jeder Änderung (Pixelbreite aus `ai.suggestCheck`), Speichern als „edit“ auch mit
  // Verstoß; die gespeicherte Verstoßliste wird angezeigt (keine stille Korrektur).
  it("prüft Bearbeitungen live im Kern und speichert sie mit Verstößen", async () => {
    render(<ReviewPanel urlId={2} selected={[2]} />);
    fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Edited text" });
    fireEvent.change(field, { target: { value: "Zu kurz" } });
    await waitFor(() => expect(screen.getByTestId("ai-sugg-live").textContent).toContain("242 px"));
    expect(screen.getByTestId("ai-sugg-live").getAttribute("data-valid")).toBe("false");
    expect(calls("ai.suggestCheck").at(-1)).toMatchObject({ urlId: 2, element: "title", value: "Zu kurz" });
    current = detail();
    current.elements[0].review = "Edited";
    current.elements[0].accepted = { value: "Zu kurz", length: 7, pixelWidth: 70, violations: ["length<30"], origin: "edited", candidate: null, at: "x" };
    fireEvent.click(screen.getByRole("button", { name: "Accept edited" }));
    await waitFor(() => expect(calls("ai.suggestReview").at(-1)).toMatchObject({ action: "edit", value: "Zu kurz" }));
    expect(await screen.findByTestId("ai-sugg-accepted-violations")).toHaveTextContent("fewer than 30 characters");
    expect(screen.getByTestId("ai-sugg-review")).toHaveTextContent("Edited");
  });

  // spec: AI-076 [M4]
  // spec: UI-037 [M4]
  // Die SERP-Vorschau zeigt den gewählten Kandidaten; ein Kandidat mit 570 px wird an derselben Stelle gekürzt wie ein
  // gleichlautender echter Title (dieselbe Messung `detail.serpMeasure` und dieselbe Anzeige `displayText`).
  it("SERP-Vorschau kürzt den gewählten Kandidaten wie den echten Title", async () => {
    render(<ReviewPanel urlId={2} selected={[2]} />);
    const serp = await screen.findByTestId("ai-sugg-serp");
    const measured = { chars: 61, px: 570, maxPx: 561, remaining: -9, truncated: true, cutAt: 49 };
    await waitFor(() => expect(serp.querySelector(".os-serp-title")?.textContent).toBe(displayText(LONG, measured)));
    expect(serp.querySelector(".os-serp-title")?.getAttribute("data-truncated")).toBe("true");
    const sent = calls("detail.serpMeasure").at(-1)!;
    expect(sent).toMatchObject({ title: LONG, description: "Tagesrucksäcke im Überblick.", device: "desktop" });
    expect(screen.getByTestId("ai-sugg-serp-metrics")).toHaveTextContent("570 of 561 px · truncated: yes");
    expect(serpTexts(current, "meta_description", "Neu")).toEqual({ title: "Rucksäcke Shop", description: "Neu" });
  });

  // spec: AI-076 [M4]
  // Massenaktion „Kandidat 1 übernehmen“ für markierte URLs: Kern übernimmt nur gültige; die übrigen werden gezählt.
  it("Massenaktion meldet übernommene und übersprungene URLs", async () => {
    render(<ReviewPanel urlId={2} selected={[2, 5, 9]} />);
    fireEvent.click(await screen.findByRole("button", { name: "Accept candidate 1 for 3 selected URLs" }));
    expect(await screen.findByTestId("ai-sugg-message")).toHaveTextContent("2 accepted, 1 without a valid suggestion (stay open).");
    expect(calls("ai.suggestReview")[0]).toMatchObject({ urlIds: [2, 5, 9], action: "acceptFirst" });
  });

  // spec: AI-075 [M4]
  // Grenzwertänderung über die Crawl-Konfiguration: der Kern prüft selbst neu; der nächste Tick mit neuer Zählerversion
  // lädt die Prüfansicht ohne Nutzeraktion neu (neuer Status sichtbar), ohne eigenen `ai.suggestRecheck`.
  it("lädt nach einer Grenzwertänderung der Crawl-Konfiguration über den Tick neu", async () => {
    render(<ReviewPanel urlId={2} selected={[2]} />);
    expect(await screen.findByTestId("ai-sugg-status")).toHaveTextContent("Valid");
    const tick = (countsVersion: number) => ({ seq: countsVersion, at: "2026-11-27T09:00:00Z", views: [], countsVersion, issuesVersion: 0, sidebarVersion: 0, ops: [], notices: [] }) as unknown as Tick;
    await act(async () => eventBus.emit({ kind: "tick", tick: tick(1) }));
    const before = calls("ai.suggestDetail").length;
    current = detail("No valid suggestion");
    await act(async () => eventBus.emit({ kind: "tick", tick: tick(1) }));
    expect(calls("ai.suggestDetail")).toHaveLength(before);
    await act(async () => eventBus.emit({ kind: "tick", tick: tick(2) }));
    await waitFor(() => expect(screen.getByTestId("ai-sugg-status")).toHaveTextContent("No valid suggestion"));
    expect(calls("ai.suggestRecheck")).toHaveLength(0);
  });

  // spec: AI-020 [M4]
  // Tooltip zum Status `Error: Output limit reached` (Nachtrag 24.11.2026, Q00-H1) an der Statusangabe.
  it("zeigt den Hinweis zur Ausgabegrenze am Status", async () => {
    current = detail(OUTPUT_LIMIT_STATUS);
    current.elements[0].candidates = [];
    render(<ReviewPanel urlId={2} selected={[2]} />);
    const status = await screen.findByTestId("ai-sugg-status");
    expect(status.getAttribute("title")).toBe("Output limit reached – raise max_tokens or lower the reasoning level (AI › Model choice)");
    expect(screen.getByRole("note")).toHaveTextContent("raise max_tokens");
  });

  // spec: AI-075 [M4]
  it("Verstoßcodes erscheinen in der UI-Sprache", async () => {
    await act(async () => {
      await setLanguage("de");
    });
    const t = getT();
    expect(violationText(t, "pixel_width>561")).toBe("breiter als 561 px");
    expect(violationText(t, "length<30")).toBe("weniger als 30 Zeichen");
    expect(violationText(t, "language:en")).toBe("Sprache en statt der Seitensprache");
    expect(violationText(t, "unknown_code")).toBe("unknown_code");
  });
});

describe("Bereich am Tab „AI Suggestions“", () => {
  // spec: UI-004 [M4]
  // Plan M4 E1 (E-2 b): nur mit Schalter „KI (Vorschau)“ und nur am Tab „AI Suggestions“; das Umschalten wirkt sofort.
  it("erscheint nur mit Schalter an und aktivem Tab", async () => {
    useTabs.setState({ active: "AI Suggestions" });
    useDetailSelection.getState().setSelection({ urlIds: [2], focus: 2 });
    render(<SuggestionsReviewHost />);
    expect(screen.queryByRole("complementary", { name: "Review AI suggestions" })).toBeNull();
    await act(async () => {
      await setAiPreview(true);
    });
    expect(await screen.findByRole("complementary", { name: "Review AI suggestions" })).toBeTruthy();
    await act(async () => {
      useTabs.setState({ active: "Internal" });
    });
    expect(screen.queryByRole("complementary", { name: "Review AI suggestions" })).toBeNull();
    await act(async () => {
      useTabs.setState({ active: "AI Suggestions" });
      await setAiPreview(false);
    });
    expect(screen.queryByRole("complementary", { name: "Review AI suggestions" })).toBeNull();
  });
});

// spec: AI-075 [M4]
// Pixelbreiten werden nie im Frontend berechnet (statischer Test wie UI-038): kein Canvas, kein measureText.
it("misst keine Pixelbreiten im Frontend", () => {
  const dir = join(process.cwd(), "src", "features", "ai", "suggestions");
  for (const file of readdirSync(dir).filter((f) => /\.tsx?$/.test(f) && !f.endsWith(".test.tsx"))) {
    const text = readFileSync(join(dir, file), "utf8");
    expect(text, file).not.toMatch(/measureText|getContext\(|OffscreenCanvas|<canvas/);
  }
});

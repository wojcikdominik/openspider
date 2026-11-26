// KI-Vorschläge (08 AI-070 … AI-076, M4-V01): Typen und Aufrufe der Commands `ai.suggest*`. Pixelbreiten liefert
// ausschließlich der Kern (UI-038); das Frontend misst nie selbst.
import { client } from "../../../ipc/client";
import type { TFn } from "../../../i18n";

export const SUGGESTIONS_TAB = "AI Suggestions";
export const ELEMENTS = ["title", "meta_description"] as const;
export type ElementId = (typeof ELEMENTS)[number];
export type ReviewStatus = "Open" | "Accepted" | "Edited" | "Rejected";
/** Status aus AI-020 mit Tooltip (Nachtrag 24.11.2026, Q00-H1). */
export const OUTPUT_LIMIT_STATUS = "Error: Output limit reached";

export interface Candidate {
  index: number;
  value: string;
  length: number;
  pixelWidth: number | null;
  valid: boolean;
  violations: string[];
  round: number;
  selected: boolean;
}

export interface Accepted {
  value: string;
  length: number;
  pixelWidth: number | null;
  violations: string[];
  origin: "ai" | "edited";
  candidate: number | null;
  at: string;
}

export interface ElementDetail {
  element: ElementId;
  label: string;
  current: string | null;
  currentPixelWidth: number | null;
  status: string | null;
  provisional?: boolean;
  violations?: string[];
  review?: ReviewStatus;
  accepted?: Accepted | null;
  model?: string | null;
  generatedAt?: string | null;
  focus?: string | null;
  rounds?: number;
  candidates: Candidate[];
}

export interface SuggestDetail {
  urlId: number;
  address: string | null;
  elements: ElementDetail[];
}

export interface CheckResult {
  value: string;
  length: number;
  pixelWidth: number;
  violations: string[];
  valid: boolean;
}

export interface ReviewResult {
  updated: number;
  accepted: number | null;
  skipped: number | null;
  skippedUrlIds: number[] | null;
  errors: { urlId: number; error: string }[];
}

export type ReviewAction = "accept" | "edit" | "reject" | "reset" | "acceptFirst";

export const suggestApi = {
  detail: (urlId: number, signal?: AbortSignal) => client.invoke<SuggestDetail>("ai.suggestDetail", { urlId }, signal),
  check: (args: { urlId: number; element: ElementId; value: string }, signal?: AbortSignal) =>
    client.invoke<CheckResult>("ai.suggestCheck", args, signal),
  review: (args: { urlIds: number[]; element: ElementId; action: ReviewAction; candidate?: number; value?: string }) =>
    client.invoke<ReviewResult>("ai.suggestReview", args),
  recheck: () => client.invoke<{ updated: number }>("ai.suggestRecheck", undefined),
  focusImport: (csv: string) => client.invoke<{ imported: number; unknown: string[] }>("ai.suggestFocusImport", { csv }),
};

/** Kandidat, der beim Öffnen gewählt ist: der gewählte gültige, sonst der sichtbare erste. */
export function initialCandidate(el: ElementDetail | undefined): number | undefined {
  if (!el || el.candidates.length === 0) return undefined;
  return (el.candidates.find((c) => c.selected) ?? el.candidates[0]).index;
}

/** Verstoßcode (`pixel_width>561`, `duplicate_of:<Adresse>` …) als Text der UI-Sprache. */
export function violationText(t: TFn, code: string): string {
  const m = /^(length|pixel_width)([<>])(\d+)$/.exec(code);
  if (m) return t(`ai-suggestions-violation-${m[1]}-${m[2] === ">" ? "max" : "min"}`, { limit: Number(m[3]) });
  const colon = code.indexOf(":");
  const [name, arg] = colon > 0 ? [code.slice(0, colon), code.slice(colon + 1)] : [code, ""];
  switch (name) {
    case "empty":
    case "placeholder":
    case "line_break":
    case "quotes":
      return t(`ai-suggestions-violation-${name}`);
    case "duplicate_of":
    case "duplicate_candidate":
    case "language":
      return t(`ai-suggestions-violation-${name}`, { arg });
    default:
      return code;
  }
}

/** Tooltip der Statuszelle (AI-020: `Error: Output limit reached`); sonst keiner. */
export function statusHint(t: TFn, status: string | null | undefined): string | undefined {
  return status === OUTPUT_LIMIT_STATUS ? t("ai-suggestions-tooltip-output-limit") : undefined;
}

/** Titel und Beschreibung für die SERP-Vorschau des gewählten Kandidaten (UI-037, AI-076). */
export function serpTexts(detail: SuggestDetail | undefined, element: ElementId, text: string): { title: string; description: string } {
  const current = (id: ElementId) => detail?.elements.find((e) => e.element === id)?.current ?? "";
  return element === "title" ? { title: text, description: current("meta_description") } : { title: current("title"), description: text };
}

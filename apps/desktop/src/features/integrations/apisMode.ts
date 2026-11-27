// APIs-Modus (INTEG-012): URL-Liste hochladen, nur API-Daten abrufen, kein Abruf der Zielseiten. Start über
// `crawl.start` mit `source: { kind: "apis" }` (M2.md 3.6a); ohne verbundene Integration startet kein Lauf.
import { crawl } from "../../ipc/crawl";
import type { ApiSourceInfo } from "../../ipc/gen/ApiSourceInfo";
import type { CrawlRef } from "../../ipc/gen/CrawlRef";
import { isDialogKind } from "./sources";

export interface UrlList {
  /** Eindeutige Einträge in Reihenfolge des ersten Auftretens. */
  items: string[];
  /** Anzahl verworfener Doppelungen. */
  duplicates: number;
}

/** Eine URL je Zeile; Leerzeilen entfallen, Doppelungen werden nur einmal abgefragt. */
export function parseUrlLines(text: string): UrlList {
  const seen = new Set<string>();
  const items: string[] = [];
  let duplicates = 0;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    if (seen.has(line)) {
      duplicates++;
      continue;
    }
    seen.add(line);
    items.push(line);
  }
  return { items, duplicates };
}

/** Verbundene Integrationen des Dialogs (Reihenfolge wie `api.list`). */
export function connectedSources(sources: readonly ApiSourceInfo[]): ApiSourceInfo[] {
  return sources.filter((s) => isDialogKind(s.kind) && s.state === "connected");
}

export type ApisModeStart =
  | { started: true; ref: CrawlRef }
  | { started: false; reason: "no-integration" | "empty-list" };

/** Startet den APIs-Modus; ohne verbundene Integration oder ohne URLs gibt es keinen Lauf (kein `crawl.start`). */
export async function startApisMode(text: string, sources: readonly ApiSourceInfo[]): Promise<ApisModeStart> {
  if (connectedSources(sources).length === 0) return { started: false, reason: "no-integration" };
  const { items } = parseUrlLines(text);
  if (items.length === 0) return { started: false, reason: "empty-list" };
  const ref = await crawl.start({ source: { kind: "apis", items }, scope: "domain" });
  return { started: true, ref };
}

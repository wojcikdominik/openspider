import { create } from "zustand";
import type { CrawlListing } from "../../ipc/gen/CrawlListing";

export const useCrawlsDialog = create<{ open: boolean; returnFocus: HTMLElement | null }>(() => ({ open: false, returnFocus: null }));
export const openCrawls = (opener?: HTMLElement) => useCrawlsDialog.setState({ open: true, returnFocus: opener ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null) });
export type Column = "project" | "name" | "mode" | "startUrl" | "startedAt" | "durationMs" | "urls" | "fileBytes" | "phase" | "locked" | "crawlId";
export const columns: Column[] = ["project", "name", "mode", "startUrl", "startedAt", "durationMs", "urls", "fileBytes", "phase", "locked", "crawlId"];
export function cell(row: CrawlListing, column: Column): string | number | boolean {
  if (column === "project" || column === "startUrl" || column === "durationMs" || column === "locked") return row[column] ?? "";
  return row.crawl[column];
}
/** CRAWL-128: a stopped crawl stays `Completed` (export key) but the management shows „Abgeschlossen (gestoppt)“. */
export function phaseKey(row: CrawlListing): string {
  return row.crawl.phase === "completed" && row.endReason === "stopped" ? "crawls-phase-completed-stopped" : `crawls-phase-${row.crawl.phase}`;
}
export function sortRows(rows: CrawlListing[], column: Column, direction: "asc" | "desc"): CrawlListing[] {
  return [...rows].sort((a, b) => {
    const x = cell(a, column), y = cell(b, column);
    const order = typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y), "und", { numeric: true });
    return (direction === "asc" ? order : -order) || a.crawl.crawlId.localeCompare(b.crawl.crawlId);
  });
}
export function permissions(rows: CrawlListing[]) {
  const any = rows.length > 0 && rows.length <= 1000;
  const running = rows.some(r => r.crawl.phase === "running");
  const locked = rows.some(r => r.locked);
  return { open: rows.length === 1, resume: rows.length === 1 && rows[0]!.canResume,
    rename: rows.length === 1 && !running && !locked, delete: any && !running && !locked,
    move: any && !running, export: any && !running, duplicate: any && !running, lock: any && !running, unlock: any && !running, copyIds: any, reveal: any,
    // OUT-140: exactly two completed or idle files may be compared.
    compare: rows.length === 2 && !running };
}
const recentKey = "openspider.recent-crawls.v1";
export function readRecent(): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(recentKey) ?? "[]");
    return Array.isArray(value) ? [...new Set(value.filter((v): v is string => typeof v === "string"))].slice(0, 10) : [];
  } catch { return []; }
}
export function writeRecent(paths: string[]): string[] {
  const next = [...new Set(paths)].slice(0, 10);
  try { localStorage.setItem(recentKey, JSON.stringify(next)); } catch { /* Opening still works when persistence is unavailable. */ }
  return next;
}

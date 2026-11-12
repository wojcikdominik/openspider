import { updateUiState } from "../../state/uistate";
// CRAWL-127: exakt zehn verschiedene Pfade, zuletzt geöffnet zuerst. Keine Pfade im Frontend konstruieren.
import type { JsonValue } from "../../ipc/gen/serde_json/JsonValue";
import { crawls } from "../../ipc/crawls";
import { uistate } from "../../ipc/uistate";
import type { RecentEntry } from "../../ui/menu/contributions/main";
import { recentCrawlTitle } from "../../ui/recentCrawl";
import { currentLanguage } from "../../i18n";

// CRAWL-127 (OS-Festlegung 09.11.2026): one recent list for the File menu and the crawl management. Writers notify
// readers, so a crawl opened in one place appears in the other without waiting for a session change.
const listeners = new Set<() => void>();
export function onRecentChanged(listener: () => void): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

export function normalizeRecent(value: unknown): RecentEntry[] {
  if (!Array.isArray(value)) return [];
  const seen = new Set<string>();
  return value.filter((entry): entry is RecentEntry => {
    if (!entry || typeof entry.path !== "string" || !entry.path || seen.has(entry.path)) return false;
    seen.add(entry.path);
    return true;
  }).slice(0, 10).map(({ path, label, crawlId }) => ({
    path,
    ...(typeof label === "string" && label ? { label } : {}),
    ...(typeof crawlId === "string" && crawlId ? { crawlId } : {}),
  }));
}
/** CRAWL-127: the index row of an entry, by crawl ID first (path spellings may differ), else by path as before. */
export function findRecentRow<T extends { crawl: { crawlId: string; file: string } }>(rows: readonly T[], entry: RecentEntry): T | undefined {
  return (entry.crawlId ? rows.find(row => row.crawl.crawlId === entry.crawlId) : undefined) ?? rows.find(row => row.crawl.file === entry.path);
}
function record(value: unknown): Record<string, JsonValue> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, JsonValue> : {};
}
export async function loadRecent(): Promise<RecentEntry[]> {
  const saved = await uistate.load();
  const entries = normalizeRecent(record(saved.state).recentCrawls);
  if (!entries.length) return entries;
  // The index supplies metadata without opening/migrating every recent crawl.
  try {
    const index = await crawls.list({ show: "all" });
    return entries.map(entry => {
      const row = findRecentRow(index, entry);
      // CRAWL-127 (OS-Festlegung 09.11.2026): same readable title as the crawl management (name/domain – date).
      const label = row && recentCrawlTitle({ name: row.crawl.name, startUrl: row.startUrl, startedAt: row.crawl.startedAt }, currentLanguage());
      return label ? { ...entry, label } : entry;
    });
  } catch { return entries; }
}
/** Replay the list operation, rather than a snapshot, after revision conflicts. */
export async function updateRecent(change: (entries: RecentEntry[]) => RecentEntry[]): Promise<void> {
  await updateUiState(state => ({ ...state, recentCrawls: normalizeRecent(change(normalizeRecent(state.recentCrawls)))
    .map(({ path, label, crawlId }) => ({ path, ...(label ? { label } : {}), ...(crawlId ? { crawlId } : {}) })) }));
  for (const listener of [...listeners]) listener();
}

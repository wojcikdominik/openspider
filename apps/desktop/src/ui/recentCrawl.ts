// CRAWL-127 (OS-Festlegung 09.11.2026): lesbare Einträge unter „Zuletzt geöffnet“ – Crawl-Name bzw. Domain mit Datum,
// dazu der in der Mitte gekürzte Pfad. Gemeinsam für die Crawl-Verwaltung und das Menü Datei → Zuletzt geöffnet.

/** Fixed limit for places that cannot measure text (native menus). */
export const RECENT_PATH_MAX = 64;
const ELLIPSIS = "…";
/** A parent directory up to this length stays next to the file name (`…/crawls/name.ospider`). */
const PARENT_MAX = 24;

const lastSeparator = (path: string, before = path.length) => Math.max(path.lastIndexOf("/", before - 1), path.lastIndexOf("\\", before - 1));

/** File name without directories; `/` and `\` both separate (macOS, Linux, Windows). */
export function fileName(path: string): string {
  return path.slice(lastSeparator(path) + 1) || path;
}

/**
 * Splits a path into the part that may be shortened (`head`) and the part that stays visible (`tail`): the file name
 * with its separator, plus the parent directory when that is short.
 */
export function splitPath(path: string): { head: string; tail: string } {
  const file = lastSeparator(path);
  if (file < 0) return { head: "", tail: path };
  const parent = lastSeparator(path, file);
  const cut = parent >= 0 && file - parent - 1 <= PARENT_MAX ? parent : file;
  return { head: path.slice(0, cut), tail: path.slice(cut) };
}

/**
 * Shortens a path in the middle to at most `max` characters: the beginning and the end with the file name stay, `…`
 * replaces the middle. Shorter paths are returned unchanged. Only an overlong file name is itself shortened, keeping
 * its beginning and its end (extension).
 */
export function middleEllipsis(path: string, max = RECENT_PATH_MAX): string {
  if (path.length <= max) return path;
  const keep = max - ELLIPSIS.length;
  const { tail } = splitPath(path);
  const separator = lastSeparator(path);
  const file = separator >= 0 ? path.slice(separator) : "";
  const end = [tail, file].find(part => part.length > 0 && keep - part.length >= 4);
  if (end) return path.slice(0, keep - end.length) + ELLIPSIS + end;
  const back = Math.ceil(keep / 2);
  return path.slice(0, keep - back) + ELLIPSIS + path.slice(path.length - back);
}

export interface RecentCrawlMeta {
  name?: string | null;
  startUrl?: string | null;
  startedAt?: string | null;
}

function host(url: string | null | undefined): string {
  if (!url) return "";
  try { return new URL(url).host; } catch { return url; }
}

const dateFormats = new Map<string, Intl.DateTimeFormat>();
function crawlDate(value: string | null | undefined, lang: string): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  let format = dateFormats.get(lang);
  if (!format) dateFormats.set(lang, (format = new Intl.DateTimeFormat(lang, { dateStyle: "medium", timeStyle: "short" })));
  return format.format(date);
}

/**
 * Readable title of a crawl known to the crawl management: its name (default: host of the start URL, OUT-191), else
 * the host of the start URL, followed by the crawl's start date and time. `undefined` for an unknown crawl.
 */
export function recentCrawlTitle(meta: RecentCrawlMeta | undefined, lang: string): string | undefined {
  if (!meta) return undefined;
  const label = meta.name?.trim() || host(meta.startUrl);
  if (!label) return undefined;
  const date = crawlDate(meta.startedAt, lang);
  return date ? `${label} – ${date}` : label;
}

/** One-line menu label: the readable title, or for an unknown crawl the path shortened in the middle. */
export function recentMenuLabel(path: string, meta: RecentCrawlMeta | undefined, lang: string): string {
  return recentCrawlTitle(meta, lang) ?? middleEllipsis(path);
}

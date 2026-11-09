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

type DateFormatFactory = (lang: string, options: Intl.DateTimeFormatOptions) => Pick<Intl.DateTimeFormat, "format">;
const dateFormats = new Map<string, Pick<Intl.DateTimeFormat, "format">>();
const intlFormat: DateFormatFactory = (lang, options) => {
  const key = `${lang}|${JSON.stringify(options)}`;
  let format = dateFormats.get(key);
  if (!format) dateFormats.set(key, (format = new Intl.DateTimeFormat(lang, options)));
  return format;
};

/**
 * Date and time of a crawl as `<date>, <time>` in the UI language. Date and time are formatted separately and joined
 * here: a combined `dateStyle` + `timeStyle` format differs between engines (WebKit on macOS writes
 * „Oct 1, 2026 at 12:00 PM“, Chromium „Oct 1, 2026, 12:00 PM“), so menu and management would differ per platform.
 */
export function formatCrawlDate(value: string | null | undefined, lang: string, make: DateFormatFactory = intlFormat): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return `${make(lang, { dateStyle: "medium" }).format(date)}, ${make(lang, { timeStyle: "short" }).format(date)}`;
}

/**
 * Readable title of a crawl known to the crawl management: its name (default: host of the start URL, OUT-191), else
 * the host of the start URL, followed by the crawl's start date and time. `undefined` for an unknown crawl.
 */
export function recentCrawlTitle(meta: RecentCrawlMeta | undefined, lang: string): string | undefined {
  if (!meta) return undefined;
  const label = meta.name?.trim() || host(meta.startUrl);
  if (!label) return undefined;
  const date = formatCrawlDate(meta.startedAt, lang);
  return date ? `${label} – ${date}` : label;
}

// Leitet die Voreinstellungsliste des Kerns aus ihren beiden Datendateien ab (nur für Tests, die `UA_PRESETS` an die
// Dateien binden): crates/os-fetch/resources/user-agents.toml und resources/ai/ai-crawlers.json. Regel wie
// `os_fetch::user_agents::merge_ai_presets` (CRAWL-114 [M4]).
import type { UaPreset } from "./presets";

export const AI_PRESET_GROUP = "KI-Crawler";
export const AI_PRESET_EXTRA_TOKENS = ["GPTBot", "ClaudeBot", "Google-Extended"];

export interface CatalogCrawler {
  token: string;
  category: string;
  ua_example: string | null;
  deprecated?: boolean;
}

/** `[[preset]]`-Blöcke der TOML-Datei (nur Zeichenketten-Werte, wie `parse_presets`). */
export function parsePresetToml(text: string): UaPreset[] {
  return text
    .split("[[preset]]")
    .slice(1)
    .map((b) => {
      const get = (k: string) => new RegExp(`^${k}\\s*=\\s*"(.*)"$`, "m").exec(b)?.[1] ?? null;
      const id = get("id") ?? "";
      return { id, name: get("name") ?? id, group: get("group") ?? "", userAgent: get("user_agent"), robotsToken: get("robots_token") ?? "*" };
    });
}

/** Gehört ein Katalogeintrag zu den KI-Crawler-Voreinstellungen? */
export function isAiPreset(c: CatalogCrawler): boolean {
  const extra = AI_PRESET_EXTRA_TOKENS.some((t) => t.toLowerCase() === c.token.toLowerCase());
  return extra || (!c.deprecated && (c.category === "search" || c.category === "user-fetch"));
}

/** Feste Einträge übernehmen Token und `ua_example`; übrige Auswahl als neue Einträge hinter der Gruppe „KI-Crawler“. */
export function mergeAiPresets(base: UaPreset[], crawlers: CatalogCrawler[]): UaPreset[] {
  const out = base.map((p) => ({ ...p }));
  const added: UaPreset[] = [];
  for (const c of crawlers.filter(isAiPreset)) {
    const fixed = out.find((p) => p.robotsToken.toLowerCase() === c.token.toLowerCase());
    if (fixed) {
      fixed.robotsToken = c.token;
      if (c.ua_example) fixed.userAgent = c.ua_example;
      continue;
    }
    const id = c.token.toLowerCase();
    if (out.some((p) => p.id === id) || added.some((p) => p.id === id)) continue;
    added.push({ id, name: c.token, group: AI_PRESET_GROUP, userAgent: c.ua_example || null, robotsToken: c.token });
  }
  let at = -1;
  out.forEach((p, i) => {
    if (p.group === AI_PRESET_GROUP) at = i + 1;
  });
  if (at < 0) at = out.findIndex((p) => p.id === "custom");
  if (at < 0) at = out.length;
  out.splice(at, 0, ...added);
  return out;
}

/** Vollständige Liste aus beiden Dateien. */
export function derivePresets(presetToml: string, catalogJson: string): UaPreset[] {
  const catalog = JSON.parse(catalogJson) as { crawlers: CatalogCrawler[] };
  return mergeAiPresets(parsePresetToml(presetToml), catalog.crawlers);
}

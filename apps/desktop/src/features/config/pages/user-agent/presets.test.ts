// Die Voreinstellungsliste der Oberfläche ist an die Datendateien des Kerns gebunden: user-agents.toml und der
// AI-Crawler-Katalog (CRAWL-114 [M4], AI-120). Oberfläche und Kern zeigen dieselben Einträge und User-Agents.
import { describe, expect, it } from "vitest";
import uaToml from "../../../../../../../crates/os-fetch/resources/user-agents.toml?raw";
import catalogJson from "../../../../../../../resources/ai/ai-crawlers.json?raw";
import { resolveUa } from "./index";
import { AI_PRESET_GROUP, derivePresets, isAiPreset, parsePresetToml, type CatalogCrawler } from "./derivePresets";
import { UA_PRESETS } from "./presets";

const catalog = (JSON.parse(catalogJson) as { crawlers: CatalogCrawler[] }).crawlers;

describe("User-Agent-Voreinstellungen", () => {
  // spec: CRAWL-114 [M4]
  it("entsprechen user-agents.toml plus KI-Crawlern aus ai-crawlers.json (Kennung, Name, Gruppe, Token, User-Agent)", () => {
    expect(UA_PRESETS.map((p) => ({ ...p }))).toEqual(derivePresets(uaToml, catalogJson));
  });

  // spec: CRAWL-114 [M4]
  it("KI-Crawler-Voreinstellungen = nicht veraltete search/user-fetch ∪ {GPTBot, ClaudeBot, Google-Extended}", () => {
    const tokens = new Set(catalog.map((c) => c.token.toLowerCase()));
    const actual = UA_PRESETS.filter((p) => tokens.has(p.robotsToken.toLowerCase()))
      .map((p) => p.robotsToken)
      .sort();
    const expected = catalog
      .filter(isAiPreset)
      .map((c) => c.token)
      .sort();
    expect(actual).toEqual(expected);
    for (const t of ["anthropic-ai", "Google-NotebookLM", "CCBot"]) expect(UA_PRESETS.some((p) => p.robotsToken === t)).toBe(false);
    // Jeder Katalog-User-Agent wird angezeigt.
    for (const c of catalog.filter((x) => isAiPreset(x) && x.ua_example)) {
      expect(UA_PRESETS.find((p) => p.robotsToken === c.token)?.userAgent, c.token).toBe(c.ua_example);
    }
    // Feste Einträge bleiben (Reihenfolge der Datei), neue stehen in der Gruppe „KI-Crawler“.
    const fixed = parsePresetToml(uaToml).map((p) => p.id);
    expect(UA_PRESETS.map((p) => p.id).filter((id) => fixed.includes(id))).toEqual(fixed);
    for (const p of UA_PRESETS.filter((x) => !fixed.includes(x.id))) expect(p.group).toBe(AI_PRESET_GROUP);
  });

  // spec: CRAWL-114 [M4]
  it("OAI-SearchBot zeigt den ua_example des Katalogs und sein Token", () => {
    const ua = catalog.find((c) => c.token === "OAI-SearchBot")?.ua_example;
    expect(resolveUa({ preset: "oai-searchbot" }, "1.0")).toEqual({ http: ua, token: "OAI-SearchBot", missingToken: false });
  });

  // spec: CRAWL-114 [M4]
  it("ohne ua_example: Standard-User-Agent und eigenes Token; ClaudeBot behält den User-Agent aus user-agents.toml", () => {
    const standard = resolveUa({ preset: "openspider" }, "1.0").http;
    for (const id of ["claude-searchbot", "claude-user", "google-cloudvertexbot", "youbot"]) {
      const token = UA_PRESETS.find((p) => p.id === id)?.robotsToken;
      expect(token, id).toBeTruthy();
      expect(resolveUa({ preset: id }, "1.0"), id).toEqual({ http: standard, token, missingToken: false });
    }
    const claude = parsePresetToml(uaToml).find((p) => p.id === "claudebot");
    expect(resolveUa({ preset: "claudebot" }, "1.0")).toEqual({ http: claude?.userAgent, token: "ClaudeBot", missingToken: false });
  });
});

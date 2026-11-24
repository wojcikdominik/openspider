// User-Agent-Voreinstellungen (CRAWL-114), Abbild der Liste des Kerns (`os_fetch::user_agents::presets()`):
// crates/os-fetch/resources/user-agents.toml plus KI-Crawler aus resources/ai/ai-crawlers.json (CRAWL-114 [M4], Regel wie
// `merge_ai_presets`). `presets.test.ts` leitet die Liste aus beiden Dateien ab und vergleicht; Änderungen dort erfordern
// die Nachführung hier.
export interface UaPreset {
  id: string;
  name: string;
  group: string;
  /** HTTP-User-Agent mit `{version}`; `null` = nur Token bzw. benutzerdefiniert. */
  userAgent: string | null;
  robotsToken: string;
}

export const UA_PRESETS: readonly UaPreset[] = [
  { id: "openspider", name: "OpenSpider", group: "OpenSpider", userAgent: "Mozilla/5.0 (compatible; OpenSpider/{version}; +https://openspider.de)", robotsToken: "OpenSpider" },
  { id: "googlebot-smartphone", name: "Googlebot Smartphone", group: "Suchmaschinen", userAgent: "Mozilla/5.0 (Linux; Android 6.0.1; Nexus 5X Build/MMB29P) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Mobile Safari/537.36 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)", robotsToken: "Googlebot" },
  { id: "googlebot-desktop", name: "Googlebot Desktop", group: "Suchmaschinen", userAgent: "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)", robotsToken: "Googlebot" },
  { id: "googlebot-image", name: "Googlebot Image", group: "Suchmaschinen", userAgent: "Googlebot-Image/1.0", robotsToken: "Googlebot-Image" },
  { id: "googlebot-news", name: "Googlebot News", group: "Suchmaschinen", userAgent: "Googlebot-News", robotsToken: "Googlebot-News" },
  { id: "googlebot-video", name: "Googlebot Video", group: "Suchmaschinen", userAgent: "Googlebot-Video/1.0", robotsToken: "Googlebot-Video" },
  { id: "google-inspectiontool", name: "Google-InspectionTool", group: "Suchmaschinen", userAgent: "Mozilla/5.0 (compatible; Google-InspectionTool/1.0)", robotsToken: "Google-InspectionTool" },
  { id: "google-extended", name: "Google-Extended (nur Token)", group: "KI-Crawler", userAgent: null, robotsToken: "Google-Extended" },
  { id: "bingbot-mobile", name: "Bingbot Mobil", group: "Suchmaschinen", userAgent: "Mozilla/5.0 (Linux; Android 6.0.1; Nexus 5X Build/MMB29P) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Mobile Safari/537.36 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)", robotsToken: "bingbot" },
  { id: "bingbot-desktop", name: "Bingbot Desktop", group: "Suchmaschinen", userAgent: "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)", robotsToken: "bingbot" },
  { id: "yandex", name: "Yandex", group: "Suchmaschinen", userAgent: "Mozilla/5.0 (compatible; YandexBot/3.0; +http://yandex.com/bots)", robotsToken: "Yandex" },
  { id: "baiduspider", name: "Baiduspider", group: "Suchmaschinen", userAgent: "Mozilla/5.0 (compatible; Baiduspider/2.0; +http://www.baidu.com/search/spider.html)", robotsToken: "Baiduspider" },
  { id: "duckduckbot", name: "DuckDuckBot", group: "Suchmaschinen", userAgent: "DuckDuckBot/1.1; (+http://duckduckgo.com/duckduckbot.html)", robotsToken: "DuckDuckBot" },
  { id: "applebot", name: "Applebot", group: "Suchmaschinen", userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15 (Applebot/0.1; +http://www.apple.com/go/applebot)", robotsToken: "Applebot" },
  { id: "gptbot", name: "GPTBot", group: "KI-Crawler", userAgent: "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; GPTBot/1.4; +https://openai.com/gptbot", robotsToken: "GPTBot" },
  { id: "claudebot", name: "ClaudeBot", group: "KI-Crawler", userAgent: "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; ClaudeBot/1.0; +claudebot@anthropic.com)", robotsToken: "ClaudeBot" },
  { id: "perplexitybot", name: "PerplexityBot", group: "KI-Crawler", userAgent: "Mozilla/5.0 (compatible; PerplexityBot/1.0; +https://perplexity.ai/perplexitybot)", robotsToken: "PerplexityBot" },
  { id: "oai-searchbot", name: "OAI-SearchBot", group: "KI-Crawler", userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36; compatible; OAI-SearchBot/1.4; +https://openai.com/searchbot", robotsToken: "OAI-SearchBot" },
  { id: "chatgpt-user", name: "ChatGPT-User", group: "KI-Crawler", userAgent: "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; ChatGPT-User/1.0; +https://openai.com/bot", robotsToken: "ChatGPT-User" },
  { id: "claude-searchbot", name: "Claude-SearchBot", group: "KI-Crawler", userAgent: null, robotsToken: "Claude-SearchBot" },
  { id: "claude-user", name: "Claude-User", group: "KI-Crawler", userAgent: null, robotsToken: "Claude-User" },
  { id: "perplexity-user", name: "Perplexity-User", group: "KI-Crawler", userAgent: "Mozilla/5.0 (compatible; Perplexity-User/1.0; +https://perplexity.ai/perplexity-user)", robotsToken: "Perplexity-User" },
  { id: "google-cloudvertexbot", name: "Google-CloudVertexBot", group: "KI-Crawler", userAgent: null, robotsToken: "Google-CloudVertexBot" },
  { id: "google-gemininotebook", name: "Google-GeminiNotebook", group: "KI-Crawler", userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36 (compatible; Google-GeminiNotebook; +https://developers.google.com/crawling/docs/crawlers-fetchers/google-gemininotebook)", robotsToken: "Google-GeminiNotebook" },
  { id: "google-agent", name: "Google-Agent", group: "KI-Crawler", userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko; compatible; Google-Agent; +https://developers.google.com/crawling/docs/crawlers-fetchers/google-agent) Chrome/W.X.Y.Z Safari/537.36", robotsToken: "Google-Agent" },
  { id: "meta-externalfetcher", name: "Meta-ExternalFetcher", group: "KI-Crawler", userAgent: "meta-externalfetcher/1.1", robotsToken: "Meta-ExternalFetcher" },
  { id: "amazonbot", name: "Amazonbot", group: "KI-Crawler", userAgent: "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Amazonbot/0.1) Chrome/W.X.Y.Z Safari/537.36", robotsToken: "Amazonbot" },
  { id: "duckassistbot", name: "DuckAssistBot", group: "KI-Crawler", userAgent: "DuckAssistBot/1.2; (+http://duckduckgo.com/duckassistbot.html)", robotsToken: "DuckAssistBot" },
  { id: "mistralai-user", name: "MistralAI-User", group: "KI-Crawler", userAgent: "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; MistralAI-User/1.0; +https://docs.mistral.ai/robots)", robotsToken: "MistralAI-User" },
  { id: "youbot", name: "YouBot", group: "KI-Crawler", userAgent: null, robotsToken: "YouBot" },
  { id: "chrome-desktop", name: "Chrome Desktop", group: "Browser", userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36", robotsToken: "*" },
  { id: "chrome-mobile", name: "Chrome Mobil", group: "Browser", userAgent: "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Mobile Safari/537.36", robotsToken: "*" },
  { id: "safari-iphone", name: "Safari iPhone", group: "Browser", userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1", robotsToken: "*" },
  { id: "firefox", name: "Firefox", group: "Browser", userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0", robotsToken: "*" },
  { id: "edge", name: "Edge", group: "Browser", userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36 Edg/138.0.0.0", robotsToken: "*" },
  { id: "custom", name: "Benutzerdefiniert", group: "Benutzerdefiniert", userAgent: null, robotsToken: "*" },
];

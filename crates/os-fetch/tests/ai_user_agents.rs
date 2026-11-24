//! KI-Crawler-Voreinstellungen aus dem AI-Crawler-Katalog (CRAWL-114 [M4], Gegenstück zu AI-120).
//!
//! Fixtures: `resources/ai/ai-crawlers.json` (Katalog), `tests/fixtures/ai/search/catalog-extra.json`
//! (Test-Katalog mit zusätzlichen Einträgen), `crawl-http-basic/echo-headers`, `crawl-robots` (Gruppe
//! `User-agent: OAI-SearchBot`).

use std::collections::BTreeSet;
use std::sync::Arc;

use os_fetch::robots::{RobotsDecision, RobotsGate, RobotsGateConfig};
use os_fetch::user_agents::{
    ai_presets, embedded_ai_catalog, parse_ai_catalog, presets, presets_with_catalogs, resolve,
    AI_PRESET_GROUP,
};
use os_fetch::{
    Fetch, FetchConfig, FetchOutcome, FetchPurpose, FetchRequest, HttpFetcher, MemorySecretStore,
};
use os_model::{CrawlConfig, SystemClock};
use os_testsite::{fixtures_root, TestSite};
use url::Url;

/// Die Voreinstellungsdatei des Kerns (unverändert, die KI-Crawler kommen aus dem Katalog).
const PRESETS_TOML: &str = include_str!("../resources/user-agents.toml");

/// Nicht veraltete `search`- und `user-fetch`-Tokens der Mindestliste aus AI-120 plus `GPTBot`, `ClaudeBot`,
/// `Google-Extended` (unabhängiges Orakel aus der Spec-Tabelle).
const SPEC_AI_PRESET_TOKENS: [&str; 18] = [
    "OAI-SearchBot",
    "ChatGPT-User",
    "Claude-SearchBot",
    "Claude-User",
    "PerplexityBot",
    "Perplexity-User",
    "Google-CloudVertexBot",
    "Google-GeminiNotebook",
    "Google-Agent",
    "Applebot",
    "Meta-ExternalFetcher",
    "Amazonbot",
    "DuckAssistBot",
    "MistralAI-User",
    "YouBot",
    "GPTBot",
    "ClaudeBot",
    "Google-Extended",
];

fn extra_catalog() -> String {
    std::fs::read_to_string(fixtures_root().join("ai/search/catalog-extra.json"))
        .expect("catalog-extra.json")
}

fn tokens_of(list: &[&os_fetch::user_agents::UserAgentPreset]) -> BTreeSet<String> {
    list.iter().map(|p| p.robots_token.clone()).collect()
}

// spec: CRAWL-114 [M4]
// spec: AI-120
// Gleichheit (nicht Teilmenge): KI-Crawler-Voreinstellungen = {nicht veraltete search/user-fetch} ∪ {GPTBot,
// ClaudeBot, Google-Extended}, identische Schreibweise der Tokens, `ua_example` als HTTP-User-Agent.
#[test]
fn ai_presets_equal_the_catalog_selection() {
    let catalog = parse_ai_catalog(embedded_ai_catalog()).expect("Katalog");
    let expected: BTreeSet<String> = catalog
        .iter()
        .filter(|c| {
            (!c.deprecated && (c.category == "search" || c.category == "user-fetch"))
                || ["GPTBot", "ClaudeBot", "Google-Extended"].contains(&c.token.as_str())
        })
        .map(|c| c.token.clone())
        .collect();
    let list = presets();
    let actual = tokens_of(&ai_presets(list, &catalog));
    assert_eq!(actual, expected, "Gleichheit mit ai-crawlers.json");
    // Das Orakel aus der Spec-Tabelle ist enthalten (der Katalog darf zusätzliche Einträge haben).
    for token in SPEC_AI_PRESET_TOKENS {
        assert!(actual.contains(token), "{token} fehlt");
    }
    // Veraltete Einträge und reine Trainings-/Kontroll-Tokens (außer den drei festen) erscheinen nicht.
    for token in [
        "anthropic-ai",
        "Google-NotebookLM",
        "CCBot",
        "Bytespider",
        "Applebot-Extended",
    ] {
        assert!(
            !list
                .iter()
                .any(|p| p.robots_token.eq_ignore_ascii_case(token)),
            "{token} darf keine Voreinstellung sein"
        );
    }
    // Je Token genau eine Voreinstellung (feste Einträge erscheinen nicht doppelt).
    for token in &expected {
        let n = list
            .iter()
            .filter(|p| p.robots_token.eq_ignore_ascii_case(token))
            .count();
        assert_eq!(n, 1, "{token}: {n} Voreinstellungen");
    }
    // HTTP-User-Agent = ua_example (wo der Katalog einen hat); neue Einträge ohne ua_example sind „nur Token“.
    let fixed = os_fetch::user_agents::parse_presets(PRESETS_TOML).unwrap();
    for c in catalog.iter().filter(|c| expected.contains(&c.token)) {
        let p = list.iter().find(|p| p.robots_token == c.token).unwrap();
        match &c.ua_example {
            Some(ua) => assert_eq!(p.user_agent.as_ref(), Some(ua), "{}", c.token),
            None => {
                let before = fixed
                    .iter()
                    .find(|f| f.robots_token.eq_ignore_ascii_case(&c.token));
                assert_eq!(
                    p.user_agent,
                    before.and_then(|f| f.user_agent.clone()),
                    "{}: ohne ua_example bleibt der feste Wert bzw. nur Token",
                    c.token
                );
            }
        }
    }
    // Neue Einträge stehen in der Gruppe „KI-Crawler“ mit dem Token als Namen.
    let fixed_ids: BTreeSet<&str> = fixed.iter().map(|p| p.id.as_str()).collect();
    for p in list.iter().filter(|p| !fixed_ids.contains(p.id.as_str())) {
        assert_eq!(p.group, AI_PRESET_GROUP, "{}", p.id);
        assert_eq!(p.name, p.robots_token);
        assert_eq!(p.id, p.robots_token.to_ascii_lowercase());
    }
    // Die festen Einträge bleiben vollständig und in ihrer Reihenfolge.
    let ids: Vec<&str> = list.iter().map(|p| p.id.as_str()).collect();
    let kept: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| fixed_ids.contains(id))
        .collect();
    assert_eq!(
        kept,
        fixed.iter().map(|p| p.id.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(ids.last(), Some(&"custom"));
}

// spec: CRAWL-114 [M4]
// Ein im Katalog ergänzter `search`-Eintrag erscheint beim nächsten Aufbau der Liste ohne Code-Änderung;
// Trainings- und veraltete Einträge nicht. Benutzerkataloge mit diesen Kategorien erscheinen ebenso.
#[test]
fn an_added_search_entry_appears_without_code_change() {
    let extra = extra_catalog();
    let list = presets_with_catalogs(PRESETS_TOML, &[embedded_ai_catalog(), &extra]).unwrap();
    let search = list
        .iter()
        .find(|p| p.robots_token == "Example-SearchBot")
        .expect("ergänzter search-Eintrag");
    assert_eq!(search.id, "example-searchbot");
    assert_eq!(search.group, AI_PRESET_GROUP);
    assert_eq!(
        search.user_agent.as_deref(),
        Some("Mozilla/5.0 (compatible; Example-SearchBot/1.0; +https://example.test/bot)")
    );
    assert!(list.iter().any(|p| p.robots_token == "Example-Fetcher"));
    assert!(!list.iter().any(|p| p.robots_token == "Example-TrainBot"));
    assert!(!list.iter().any(|p| p.robots_token == "Example-OldSearch"));
    // Gleichheit gilt auch für den erweiterten Katalog.
    let mut all = parse_ai_catalog(embedded_ai_catalog()).unwrap();
    all.extend(parse_ai_catalog(&extra).unwrap());
    let expected: BTreeSet<String> = all
        .iter()
        .filter(|c| c.is_preset())
        .map(|c| c.token.clone())
        .collect();
    assert_eq!(tokens_of(&ai_presets(&list, &all)), expected);
    // Die ausgewählte Voreinstellung ist über ihre Kennung auflösbar.
    let r = os_fetch::user_agents::resolve_with(
        &os_model::UserAgentConfig {
            preset: "example-searchbot".into(),
            http: None,
            robots_token: None,
        },
        &list,
    );
    assert_eq!(r.robots_token, "Example-SearchBot");
    assert_eq!(r.warning, None);
    // Ungültige Kataloge werden gemeldet, nicht still übergangen.
    assert!(presets_with_catalogs(PRESETS_TOML, &["{\"crawlers\": 1}"]).is_err());
    assert!(presets_with_catalogs(
        PRESETS_TOML,
        &["{\"crawlers\": [{\"category\": \"search\"}]}"]
    )
    .is_err());
}

fn crawl_with_preset(preset: &str) -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.user_agent.preset = preset.to_string();
    cfg.user_agent.http = None;
    cfg.user_agent.robots_token = None;
    cfg
}

// spec: CRAWL-114 [M4]
// Voreinstellung `OAI-SearchBot` sendet den `ua_example` des Katalogs (`/echo-headers`).
#[tokio::test]
async fn oai_searchbot_preset_sends_the_catalog_user_agent() {
    let catalog = parse_ai_catalog(embedded_ai_catalog()).unwrap();
    let ua = catalog
        .iter()
        .find(|c| c.token == "OAI-SearchBot")
        .and_then(|c| c.ua_example.clone())
        .expect("ua_example von OAI-SearchBot");
    let resolved = resolve(&crawl_with_preset("oai-searchbot").user_agent);
    assert_eq!(resolved.http, ua);
    assert_eq!(resolved.robots_token, "OAI-SearchBot");
    assert_eq!(resolved.warning, None);

    let site = TestSite::start("crawl-http-basic").await.unwrap();
    let fc = FetchConfig::from_crawl_config(
        &crawl_with_preset("oai-searchbot"),
        &MemorySecretStore::new(),
        Arc::new(SystemClock),
    )
    .unwrap();
    let f = HttpFetcher::new(fc).unwrap();
    let res = f
        .fetch(FetchRequest {
            url: Url::parse(&site.url("/echo-headers")).unwrap(),
            purpose: FetchPurpose::Page,
            cookies: None,
            max_body_bytes: 1024 * 1024,
            attempt: 0,
            body_limit: Default::default(),
        })
        .await;
    let FetchOutcome::Response(resp) = res.outcome else {
        panic!("Antwort erwartet");
    };
    let echoed = String::from_utf8_lossy(&resp.body);
    assert!(echoed.contains(&ua), "{echoed}");
}

// spec: CRAWL-114 [M4]
// spec: CRAWL-098
// Voreinstellung `OAI-SearchBot` wertet die Gruppe `User-agent: OAI-SearchBot` der Fixture `crawl-robots` aus
// (nicht `*`, das alles sperrt).
#[tokio::test]
async fn oai_searchbot_preset_selects_its_robots_group() {
    let site = TestSite::start("crawl-robots").await.unwrap();
    let cfg = crawl_with_preset("oai-searchbot");
    let fc = FetchConfig::from_crawl_config(&cfg, &MemorySecretStore::new(), Arc::new(SystemClock))
        .unwrap();
    let fetcher = HttpFetcher::new(fc).unwrap();
    let gate = RobotsGate::new(RobotsGateConfig::from_crawl_config(&cfg));
    let url = |p: &str| Url::parse(&site.url(p)).unwrap();

    let check = gate.check(&fetcher, &url("/privat/x")).await;
    assert_eq!(
        check.decision,
        RobotsDecision::Blocked {
            matched_line: Some("Zeile 30: Disallow: /privat/".to_string()),
            robots_url: site.url("/robots.txt"),
            unreachable: false,
        }
    );
    for path in ["/frei/a", "/privat/oeffentlich", "/nur-google/a"] {
        let check = gate.check(&fetcher, &url(path)).await;
        assert_eq!(check.decision, RobotsDecision::Allowed, "{path}");
    }
}

// spec: CRAWL-114 [M4]
// Voreinstellung ohne `ua_example` im Katalog: sendet den OpenSpider-Standard-User-Agent und wertet robots.txt unter
// ihrem Token aus; bestehende feste Voreinstellungen (ClaudeBot) behalten den User-Agent aus `user-agents.toml`
// (Festlegung Orchestrator 24.11.2026).
#[test]
fn presets_without_ua_example_send_the_default_or_keep_the_fixed_user_agent() {
    let catalog = parse_ai_catalog(embedded_ai_catalog()).unwrap();
    let fixed = os_fetch::user_agents::parse_presets(PRESETS_TOML).unwrap();
    let mut checked_new = 0;
    for c in catalog
        .iter()
        .filter(|c| c.is_preset() && c.ua_example.is_none())
    {
        let id = fixed
            .iter()
            .find(|f| f.robots_token.eq_ignore_ascii_case(&c.token))
            .map_or_else(|| c.token.to_ascii_lowercase(), |f| f.id.clone());
        let r = resolve(&crawl_with_preset(&id).user_agent);
        assert_eq!(r.robots_token, c.token, "{id}");
        assert_eq!(r.warning, None, "{id}");
        match fixed
            .iter()
            .find(|f| f.id == id)
            .and_then(|f| f.user_agent.clone())
        {
            Some(ua) => assert_eq!(r.http, ua, "{id}: fester User-Agent bleibt"),
            None => {
                assert_eq!(r.http, os_fetch::user_agents::default_user_agent(), "{id}");
                checked_new += 1;
            }
        }
    }
    assert!(
        checked_new >= 4,
        "Claude-SearchBot, Claude-User, Google-CloudVertexBot, YouBot, Google-Extended"
    );
    let claude = resolve(&crawl_with_preset("claudebot").user_agent);
    let toml_ua = fixed
        .iter()
        .find(|f| f.id == "claudebot")
        .and_then(|f| f.user_agent.clone());
    assert_eq!(Some(claude.http), toml_ua);
    assert_eq!(claude.robots_token, "ClaudeBot");
    let search = resolve(&crawl_with_preset("claude-searchbot").user_agent);
    assert_eq!(search.http, os_fetch::user_agents::default_user_agent());
    assert_eq!(search.robots_token, "Claude-SearchBot");
}

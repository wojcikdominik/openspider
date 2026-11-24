//! Unabhängige Abnahmetests M4-S01: KI-Crawler-Voreinstellungen aus dem AI-Crawler-Katalog (CRAWL-114 [M4], AI-120
//! letztes Kriterium).
//!
//! Erwartungen ausschließlich aus `docs/spec/01-crawl.md` (CRAWL-114 [M4]) und `docs/spec/08-ai.md` (AI-120).
//! Fixtures: `resources/ai/ai-crawlers.json`, `crates/os-fetch/resources/user-agents.toml` (Voreinstellungsdatei) und
//! `tests/fixtures/ai/s01-acceptance/`.
// acceptance: independent

#[path = "http_support.rs"]
mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use os_fetch::robots::{RobotsDecision, RobotsGate, RobotsGateConfig};
use os_fetch::user_agents::{
    embedded_ai_catalog, parse_ai_catalog, parse_presets, presets, presets_with_catalogs, resolve,
    resolve_with, AiCatalogCrawler, UserAgentPreset, AI_PRESET_GROUP,
};
use os_fetch::{FetchConfig, HttpFetcher, MemorySecretStore};
use os_model::{CrawlConfig, SystemClock, UserAgentConfig};
use support::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Repo-Wurzel")
        .to_path_buf()
}

fn read(p: PathBuf) -> String {
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn fixture(name: &str) -> String {
    let mut p = repo_root()
        .join("tests")
        .join("fixtures")
        .join("ai")
        .join("s01-acceptance");
    for part in name.split('/') {
        p = p.join(part);
    }
    read(p)
}

fn presets_toml() -> String {
    read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("user-agents.toml"),
    )
}

fn shipped_catalog() -> String {
    read(
        repo_root()
            .join("resources")
            .join("ai")
            .join("ai-crawlers.json"),
    )
}

/// Katalog mit angehängten Einträgen (Text vor der schließenden Klammer der Liste `crawlers`).
fn catalog_with(extra: &[&str]) -> String {
    let base = shipped_catalog();
    let at = base.rfind(']').expect("Liste crawlers");
    let mut out = base[..at].trim_end().to_string();
    for e in extra {
        out.push_str(",\n");
        out.push_str(e.trim());
    }
    out.push('\n');
    out.push_str(&base[at..]);
    out
}

/// Tabelle der Spec (AI-120): Token → (Kategorie, veraltet).
fn spec_table() -> BTreeMap<&'static str, (&'static str, bool)> {
    [
        ("GPTBot", "training", false),
        ("OAI-SearchBot", "search", false),
        ("ChatGPT-User", "user-fetch", false),
        ("ClaudeBot", "training", false),
        ("Claude-SearchBot", "search", false),
        ("Claude-User", "user-fetch", false),
        ("anthropic-ai", "training", true),
        ("PerplexityBot", "search", false),
        ("Perplexity-User", "user-fetch", false),
        ("Google-Extended", "control-token", false),
        ("Google-CloudVertexBot", "user-fetch", false),
        ("Google-GeminiNotebook", "user-fetch", false),
        ("Google-NotebookLM", "user-fetch", true),
        ("Google-Agent", "user-fetch", false),
        ("Applebot", "search", false),
        ("Applebot-Extended", "control-token", false),
        ("CCBot", "training", false),
        ("Bytespider", "training", false),
        ("Meta-ExternalAgent", "training", false),
        ("Meta-ExternalFetcher", "user-fetch", false),
        ("Amazonbot", "search", false),
        ("DuckAssistBot", "search", false),
        ("MistralAI-User", "user-fetch", false),
        ("cohere-ai", "training", false),
        ("YouBot", "search", false),
        ("AI2Bot", "training", false),
        ("Diffbot", "training", false),
    ]
    .into_iter()
    .map(|(t, c, d)| (t, (c, d)))
    .collect()
}

/// Erwartete Menge nach CRAWL-114 [M4]: {nicht veraltete search/user-fetch} ∪ {GPTBot, ClaudeBot, Google-Extended}.
fn expected_tokens(crawlers: &[AiCatalogCrawler]) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = crawlers
        .iter()
        .filter(|c| !c.deprecated && (c.category == "search" || c.category == "user-fetch"))
        .map(|c| c.token.clone())
        .collect();
    for t in ["GPTBot", "ClaudeBot", "Google-Extended"] {
        out.insert(t.to_string());
    }
    out
}

/// KI-Crawler-Voreinstellungen einer Liste: Gruppe „KI-Crawler“ oder robots-Token aus dem Katalog.
fn ai_presets_of<'a>(
    list: &'a [UserAgentPreset],
    crawlers: &[AiCatalogCrawler],
) -> Vec<&'a UserAgentPreset> {
    list.iter()
        .filter(|p| {
            p.group == AI_PRESET_GROUP
                || crawlers
                    .iter()
                    .any(|c| c.token.eq_ignore_ascii_case(&p.robots_token))
        })
        .collect()
}

fn shipped_crawlers() -> Vec<AiCatalogCrawler> {
    let c = parse_ai_catalog(embedded_ai_catalog()).expect("Katalog lesbar");
    // Der Leser liefert für die Tabellen-Tokens die Werte der Spec (Gegenprobe, damit die erwartete Menge nicht nur
    // aus dem Produktcode stammt).
    for (token, (cat, dep)) in spec_table() {
        let e = c
            .iter()
            .find(|x| x.token == token)
            .unwrap_or_else(|| panic!("{token} fehlt im Katalog"));
        assert_eq!(e.category, cat, "{token}");
        assert_eq!(e.deprecated, dep, "{token}");
    }
    c
}

// spec: CRAWL-114
// acceptance: independent
// Die eingebettete Fassung ist die Datei unter `resources/ai/`.
#[test]
fn embedded_catalog_is_the_resource_file() {
    assert_eq!(embedded_ai_catalog(), shipped_catalog());
}

// spec: CRAWL-114, AI-120
// acceptance: independent
// Gleichheit (nicht Teilmenge): Menge der KI-Crawler-Voreinstellungen = {nicht veraltete search/user-fetch} ∪
// {GPTBot, ClaudeBot, Google-Extended}, identische Schreibweise, jedes Token genau einmal.
#[test]
fn ai_preset_set_equals_catalog_selection() {
    let crawlers = shipped_crawlers();
    let expected = expected_tokens(&crawlers);
    let ai = ai_presets_of(presets(), &crawlers);
    let actual: BTreeSet<String> = ai.iter().map(|p| p.robots_token.clone()).collect();
    assert_eq!(actual, expected);
    // exakt 18 aus der Tabelle der Spec (zusätzliche Katalogeinträge wären erlaubt, der Katalog hat aber keine)
    let from_table: BTreeSet<&str> = spec_table()
        .into_iter()
        .filter(|(t, (c, d))| {
            (!d && (*c == "search" || *c == "user-fetch"))
                || ["GPTBot", "ClaudeBot", "Google-Extended"].contains(t)
        })
        .map(|(t, _)| t)
        .collect();
    assert_eq!(from_table.len(), 18);
    for t in &from_table {
        assert!(actual.contains(*t), "{t} fehlt");
    }
    // jedes Token genau einmal (feste Einträge erscheinen nicht doppelt)
    for t in &expected {
        let n = presets()
            .iter()
            .filter(|p| p.robots_token.eq_ignore_ascii_case(t))
            .count();
        assert_eq!(n, 1, "{t} erscheint {n}-mal");
    }
}

// spec: CRAWL-114
// acceptance: independent
// Veraltete Einträge (`anthropic-ai`, `Google-NotebookLM`) erscheinen nicht; Training und übrige Kontroll-Tokens auch
// nicht.
#[test]
fn deprecated_and_training_entries_are_not_presets() {
    for t in [
        "anthropic-ai",
        "Google-NotebookLM",
        "CCBot",
        "Bytespider",
        "Meta-ExternalAgent",
        "cohere-ai",
        "AI2Bot",
        "Diffbot",
        "Applebot-Extended",
    ] {
        assert!(
            !presets()
                .iter()
                .any(|p| p.robots_token.eq_ignore_ascii_case(t)
                    || p.name.eq_ignore_ascii_case(t)
                    || p.id.eq_ignore_ascii_case(t)),
            "{t} darf keine Voreinstellung sein"
        );
    }
}

// spec: CRAWL-114
// acceptance: independent
// HTTP-User-Agent = `ua_example`, robots-Token = `token`; feste Einträge mit Katalog-Token übernehmen die Werte aus dem
// Katalog. Geprüft für alle Einträge, für die der Katalog einen `ua_example` hat.
#[test]
fn ai_presets_use_ua_example_and_token() {
    let text = embedded_ai_catalog();
    let crawlers = shipped_crawlers();
    let mut checked = 0;
    for c in crawlers
        .iter()
        .filter(|c| expected_tokens(&crawlers).contains(&c.token))
    {
        let p = presets()
            .iter()
            .find(|p| p.robots_token == c.token)
            .unwrap_or_else(|| panic!("keine Voreinstellung mit Token {}", c.token));
        if let Some(ua) = &c.ua_example {
            assert!(
                text.contains(ua.as_str()),
                "{}: ua_example aus der Datei",
                c.token
            );
            assert_eq!(p.user_agent.as_deref(), Some(ua.as_str()), "{}", c.token);
            let r = resolve(&UserAgentConfig {
                preset: p.id.clone(),
                http: None,
                robots_token: None,
            });
            assert_eq!(r.http, *ua, "{}", c.token);
            assert_eq!(r.robots_token, c.token, "{}", c.token);
            checked += 1;
        }
    }
    assert!(checked >= 10, "nur {checked} Einträge mit ua_example");
}

// spec: CRAWL-114
// acceptance: independent
// Neue Voreinstellungen aus dem Katalog erscheinen gruppiert unter „KI-Crawler“.
#[test]
fn catalog_presets_are_grouped_under_ki_crawler() {
    assert_eq!(AI_PRESET_GROUP, "KI-Crawler");
    let fixed: Vec<UserAgentPreset> = parse_presets(&presets_toml()).expect("Voreinstellungsdatei");
    let crawlers = shipped_crawlers();
    for p in ai_presets_of(presets(), &crawlers) {
        let is_fixed = fixed
            .iter()
            .any(|f| f.robots_token.eq_ignore_ascii_case(&p.robots_token));
        if !is_fixed {
            assert_eq!(p.group, "KI-Crawler", "{}", p.robots_token);
        }
    }
}

// spec: CRAWL-114
// acceptance: independent
// Ein im Katalog ergänzter `search`-Eintrag erscheint ohne Code-Änderung (gleiche Voreinstellungsdatei, geänderter
// Katalog); ein veralteter `search`-Eintrag nicht. Die Menge bleibt gleich der Katalogauswahl.
#[test]
fn added_search_entry_appears_without_code_change() {
    let text = catalog_with(&[
        &fixture("extra-search-entry.json"),
        &fixture("extra-deprecated-search-entry.json"),
    ]);
    let list = presets_with_catalogs(&presets_toml(), &[&text]).expect("Voreinstellungen");
    let crawlers = parse_ai_catalog(&text).expect("erweiterter Katalog lesbar");
    let p = list
        .iter()
        .find(|p| p.robots_token == "NewSearchBot")
        .expect("NewSearchBot fehlt");
    assert_eq!(
        p.user_agent.as_deref(),
        Some("Mozilla/5.0 (compatible; NewSearchBot/1.0; +https://neu.example/bot)")
    );
    assert_eq!(p.group, "KI-Crawler");
    assert!(!list
        .iter()
        .any(|p| p.robots_token.eq_ignore_ascii_case("OldSearchBot")));
    let actual: BTreeSet<String> = ai_presets_of(&list, &crawlers)
        .iter()
        .map(|p| p.robots_token.clone())
        .collect();
    assert_eq!(actual, expected_tokens(&crawlers));
    let r = resolve_with(
        &UserAgentConfig {
            preset: p.id.clone(),
            http: None,
            robots_token: None,
        },
        &list,
    );
    assert_eq!(r.robots_token, "NewSearchBot");
    assert!(r.http.contains("NewSearchBot/1.0"));
}

// spec: CRAWL-114, AI-120
// acceptance: independent
// Benutzereinträge des Katalogs mit diesen Kategorien erscheinen ebenfalls; ein Benutzereintrag mit vorhandenem Token
// überschreibt nichts.
#[test]
fn user_catalog_entries_feed_presets_without_overriding() {
    let builtin = shipped_catalog();
    let list = presets_with_catalogs(
        &presets_toml(),
        &[
            &builtin,
            &fixture("user-new-search.json"),
            &fixture("user-new-training.json"),
            &fixture("user-duplicate-gptbot.json"),
        ],
    )
    .expect("Voreinstellungen");
    assert!(list.iter().any(|p| p.robots_token == "AcmeSearchBot"));
    assert!(!list.iter().any(|p| p.robots_token == "AcmeTrainBot"));
    let gpt: Vec<&UserAgentPreset> = list
        .iter()
        .filter(|p| p.robots_token.eq_ignore_ascii_case("GPTBot"))
        .collect();
    assert_eq!(gpt.len(), 1);
    let crawlers = shipped_crawlers();
    let ua = crawlers
        .iter()
        .find(|c| c.token == "GPTBot")
        .and_then(|c| c.ua_example.clone());
    assert_eq!(gpt[0].user_agent, ua);
    assert_eq!(gpt[0].robots_token, "GPTBot");
}

// ---------------------------------------------------------------------------------------------------
// Voreinstellung OAI-SearchBot: sendet `ua_example` und wertet die Gruppe `User-agent: OAI-SearchBot` aus

fn oai_config() -> (CrawlConfig, String) {
    let crawlers = shipped_crawlers();
    let ua = crawlers
        .iter()
        .find(|c| c.token == "OAI-SearchBot")
        .and_then(|c| c.ua_example.clone())
        .expect("OAI-SearchBot hat ua_example");
    let preset = presets()
        .iter()
        .find(|p| p.robots_token == "OAI-SearchBot")
        .expect("Voreinstellung OAI-SearchBot");
    let cfg = CrawlConfig {
        user_agent: UserAgentConfig {
            preset: preset.id.clone(),
            http: None,
            robots_token: None,
        },
        ..CrawlConfig::default()
    };
    (cfg, ua)
}

// spec: CRAWL-114
// acceptance: independent
// Voreinstellung `OAI-SearchBot`: der Server sieht exakt den `ua_example` des Katalogs, und die robots.txt-Gruppe
// `User-agent: OAI-SearchBot` greift (nicht `*`, die in der Fixture alles sperrt). Auch der robots.txt-Abruf trägt den
// User-Agent.
#[tokio::test]
async fn oai_searchbot_preset_sends_ua_example_and_uses_its_robots_group() {
    let (cfg, ua) = oai_config();
    let robots_txt = fixture("robots/ai-precedence.txt");
    let server = MiniServer::start(move |req| {
        if req.path() == "/robots.txt" {
            ok_text(&robots_txt)
        } else {
            ok_text(req.header("user-agent").unwrap_or(""))
        }
    })
    .await;

    let fetch_cfg =
        FetchConfig::from_crawl_config(&cfg, &MemorySecretStore::new(), Arc::new(SystemClock))
            .expect("Konfiguration");
    assert_eq!(fetch_cfg.user_agent, ua);
    let fetcher = HttpFetcher::new(fetch_cfg).expect("Abrufer");

    let resp = expect_response(get(&fetcher, server.url("/echo-headers")).await);
    assert_eq!(String::from_utf8_lossy(&resp.body), ua);

    let gate_cfg = RobotsGateConfig::from_crawl_config(&cfg);
    assert_eq!(gate_cfg.token, "OAI-SearchBot");
    let gate = RobotsGate::new(gate_cfg);
    let frei = gate.check(&fetcher, &server.url("/frei")).await;
    assert_eq!(
        frei.decision,
        RobotsDecision::Allowed,
        "`*` (Disallow: /) darf für OAI-SearchBot nicht greifen"
    );
    let privat = gate.check(&fetcher, &server.url("/privat/x")).await;
    assert!(
        matches!(privat.decision, RobotsDecision::Blocked { .. }),
        "{:?}",
        privat.decision
    );
    let oeffentlich = gate
        .check(&fetcher, &server.url("/privat/oeffentlich"))
        .await;
    assert_eq!(oeffentlich.decision, RobotsDecision::Allowed);

    let robots_req = server
        .last_request("/robots.txt")
        .expect("robots.txt abgerufen");
    assert_eq!(robots_req.header("user-agent"), Some(ua.as_str()));
}

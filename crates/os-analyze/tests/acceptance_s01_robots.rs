//! Unabhängige Abnahmetests M4-S01: robots.txt-Auswertung je AI-Crawler (AI-121).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-121), `docs/spec/01-crawl.md` (CRAWL-098, CRAWL-100) und
//! den Festlegungen des Orchestrators (Grundmenge für `Blocked %` = gecrawlte interne HTML-URLs des Hosts; `Host` =
//! Schema+Host+Port; 5xx → `robots.txt Error`; Kontroll-Tokens wie Crawler).
//!
//! `os-analyze` hängt nicht an `os-fetch`; die Auswertung bekommt den Parser über das Trait `RobotsRules`. Diese Tests
//! verwenden dafür einen eigenen, kleinen Parser nach den Regeln aus CRAWL-098 ([`SpecRobots`]). Dass der Parser des
//! Crawl-Kerns dieselben Regeln mit KI-Tokens einhält, prüft `crates/os-fetch/tests/acceptance_s01_robots_ai_tokens.rs`.
// acceptance: independent

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use os_analyze::ai_search::catalog::{Catalog, Category};
use os_analyze::ai_search::robots::{
    access_report, evaluate, Access, AccessOptions, AccessReport, AccessRow, GroupMatch, HostInput,
    HostRobots, MatchedGroup, RobotsRules, RuleHit, TokenSelection, REPORT_COLUMNS,
};
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, RobotsUnreachablePolicy, Timestamp};
use os_store::{CrawlFile, CrawlInit, StorageProfile};

// ---------------------------------------------------------------------------------------------------
// Fixtures

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Repo-Wurzel")
        .join("tests")
        .join("fixtures")
        .join("ai")
        .join("s01-acceptance")
}

fn robots_fixture(name: &str) -> String {
    let p = fixture_dir().join("robots").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("Fixture {}: {e}", p.display()))
}

/// Tokens der Spec-Tabelle nach Kategorie (aus der Abschrift `catalog-minimum.json`).
fn table_tokens(category: &str, include_deprecated: bool) -> Vec<String> {
    let p = fixture_dir().join("catalog-minimum.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    v["crawlers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| category == "*" || e["category"] == category)
        .filter(|e| include_deprecated || !e["deprecated"].as_bool().unwrap())
        .map(|e| e["token"].as_str().unwrap().to_string())
        .collect()
}

// ---------------------------------------------------------------------------------------------------
// Kleiner robots.txt-Parser nach CRAWL-098 (Testdouble für das Trait)

struct Rule {
    allow: bool,
    pattern: String,
    line_no: u32,
    text: String,
}

#[derive(Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
}

/// Gruppen: aufeinanderfolgende `User-agent`-Zeilen (auch mit anderen Zeilen dazwischen) bilden eine Gruppe; eine neue
/// beginnt mit einer `User-agent`-Zeile nach einer Regelzeile. Token-Vergleich case-insensitiv; gleichnamige Gruppen
/// werden zusammengeführt; `*` nur ohne spezifische Gruppe. Vorrang: längstes Muster, bei Gleichstand `Allow`.
struct SpecRobots {
    groups: Vec<Group>,
}

impl SpecRobots {
    fn parse(text: &str) -> SpecRobots {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut groups: Vec<Group> = Vec::new();
        let mut in_agent_run = false;
        for (i, raw) in text
            .split("\r\n")
            .flat_map(|l| l.split(['\n', '\r']))
            .enumerate()
        {
            let line = raw.split('#').next().unwrap_or("").trim();
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let key = k.trim().to_ascii_lowercase();
            let value = v.trim();
            match key.as_str() {
                "user-agent" => {
                    if !in_agent_run || groups.is_empty() {
                        groups.push(Group::default());
                    }
                    in_agent_run = true;
                    groups
                        .last_mut()
                        .unwrap()
                        .agents
                        .push(value.to_ascii_lowercase());
                }
                "allow" | "disallow" => {
                    in_agent_run = false;
                    if let Some(g) = groups.last_mut() {
                        if !value.is_empty() {
                            g.rules.push(Rule {
                                allow: key == "allow",
                                pattern: value.to_string(),
                                line_no: i as u32 + 1,
                                text: line.to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        SpecRobots { groups }
    }

    fn groups_for(&self, token: &str) -> Vec<&Group> {
        let t = token.to_ascii_lowercase();
        let specific: Vec<&Group> = self
            .groups
            .iter()
            .filter(|g| g.agents.contains(&t))
            .collect();
        if !specific.is_empty() {
            return specific;
        }
        self.groups
            .iter()
            .filter(|g| g.agents.iter().any(|a| a == "*"))
            .collect()
    }
}

fn pattern_matches(pattern: &str, path: &str) -> bool {
    let (pat, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    fn rec(p: &[u8], s: &[u8], anchored: bool) -> bool {
        match p.split_first() {
            None => !anchored || s.is_empty(),
            Some((b'*', rest)) => (0..=s.len()).any(|i| rec(rest, &s[i..], anchored)),
            Some((c, rest)) => s.first() == Some(c) && rec(rest, &s[1..], anchored),
        }
    }
    rec(pat.as_bytes(), path.as_bytes(), anchored)
}

impl RobotsRules for SpecRobots {
    fn group_match(&self, token: &str) -> GroupMatch {
        let t = token.to_ascii_lowercase();
        if self.groups.iter().any(|g| g.agents.contains(&t)) {
            GroupMatch::Specific
        } else if self
            .groups
            .iter()
            .any(|g| g.agents.iter().any(|a| a == "*"))
        {
            GroupMatch::Wildcard
        } else {
            GroupMatch::None
        }
    }

    fn evaluate(&self, token: &str, path_and_query: &str) -> Option<RuleHit> {
        let mut best: Option<&Rule> = None;
        for g in self.groups_for(token) {
            for r in &g.rules {
                if !pattern_matches(&r.pattern, path_and_query) {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some(b) => {
                        r.pattern.len() > b.pattern.len()
                            || (r.pattern.len() == b.pattern.len() && r.allow && !b.allow)
                    }
                };
                if better {
                    best = Some(r);
                }
            }
        }
        best.map(|r| RuleHit {
            allowed: r.allow,
            line_no: r.line_no,
            rule: r.text.clone(),
        })
    }
}

fn spec_parser(text: &str) -> Box<dyn RobotsRules> {
    Box::new(SpecRobots::parse(text))
}

// ---------------------------------------------------------------------------------------------------
// Hilfen

/// 40 gecrawlte HTML-URLs: Startseite, 29 normale Seiten und 10 unter `/intern/`.
fn forty_urls() -> Vec<(i64, String)> {
    let mut v = vec![(1, "/".to_string())];
    for i in 1..=29 {
        v.push((v.len() as i64 + 1, format!("/seite-{i}")));
    }
    for i in 1..=10 {
        v.push((v.len() as i64 + 1, format!("/intern/doc-{i}")));
    }
    assert_eq!(v.len(), 40);
    v
}

fn host_with(host: &str, robots: &str, urls: Vec<(i64, String)>) -> HostInput {
    HostInput {
        host: host.to_string(),
        robots: HostRobots::Rules(spec_parser(&robots_fixture(robots))),
        urls,
    }
}

fn default_report(hosts: &[HostInput]) -> AccessReport {
    evaluate(Catalog::builtin(), hosts, &AccessOptions::default())
}

fn row<'a>(r: &'a AccessReport, host: &str, token: &str) -> &'a AccessRow {
    r.row(host, token)
        .unwrap_or_else(|| panic!("keine Zeile für ({host}, {token})"))
}

fn column_tokens(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------------------------------
// Akzeptanzkriterien AI-121

// spec: AI-121
// acceptance: independent
// `User-agent: GPTBot` / `Disallow: /` und `User-agent: *` / `Allow: /` → GPTBot `Blocked` (Matched Group = GPTBot),
// OAI-SearchBot `Allowed` (Matched Group = *).
#[test]
fn gptbot_blocked_and_oai_searchbot_allowed() {
    let h = "https://ai-gptbot.test";
    let r = default_report(&[host_with(h, "ai-gptbot.txt", forty_urls())]);
    let gpt = row(&r, h, "GPTBot");
    assert_eq!(gpt.access, Access::Blocked);
    assert_eq!(gpt.access.as_str(), "Blocked");
    assert_eq!(gpt.matched_group.as_str(), "GPTBot");
    assert!(!gpt.homepage_allowed);
    assert_eq!(gpt.blocked_urls, 40);
    assert_eq!(gpt.blocked_pct_display(), "100.00");
    assert!(
        gpt.matched_rule_display().contains("Disallow: /"),
        "Matched Rule: {:?}",
        gpt.matched_rule_display()
    );
    assert_eq!(gpt.operator, "OpenAI");
    assert_eq!(gpt.category, Category::Training);

    let oai = row(&r, h, "OAI-SearchBot");
    assert_eq!(oai.access, Access::Allowed);
    assert_eq!(oai.access.as_str(), "Allowed");
    assert_eq!(oai.matched_group, MatchedGroup::Wildcard);
    assert_eq!(oai.matched_group.as_str(), "*");
    assert!(oai.homepage_allowed);
    assert_eq!(oai.blocked_urls, 0);
    assert_eq!(oai.blocked_pct_display(), "0.00");
}

// spec: AI-121
// acceptance: independent
// `Disallow: /intern/` für ClaudeBot bei 10 von 40 gecrawlten HTML-URLs unter `/intern/` → `Partially Blocked`,
// `Blocked % = 25.00`.
#[test]
fn claudebot_partial_ten_of_forty() {
    let h = "https://ai-partial.test";
    let r = default_report(&[host_with(h, "ai-partial.txt", forty_urls())]);
    let c = row(&r, h, "ClaudeBot");
    assert_eq!(c.access, Access::PartiallyBlocked);
    assert_eq!(c.access.as_str(), "Partially Blocked");
    assert_eq!(c.blocked_urls, 10);
    assert_eq!(c.blocked_pct_display(), "25.00");
    assert!((c.blocked_pct() - 25.0).abs() < 1e-9);
    assert!(c.homepage_allowed);
    assert_eq!(c.matched_group.as_str(), "ClaudeBot");
    // andere Tokens greifen auf `*` zurück und sind erlaubt
    let other = row(&r, h, "Claude-SearchBot");
    assert_eq!(other.access, Access::Allowed);
    assert_eq!(other.matched_group.as_str(), "*");
    // Internal-Spalten: nur die /intern/-URLs sind für Training gesperrt
    for u in &r.urls {
        let path_is_intern = u.url_id > 30;
        assert_eq!(
            column_tokens(&u.column(Category::Training)).contains("ClaudeBot"),
            path_is_intern,
            "URL {}",
            u.url_id
        );
    }
}

// spec: AI-121
// acceptance: independent
// Host ohne robots.txt (404) → alle Tokens `No robots.txt` (= erlaubt).
#[test]
fn host_without_robots_txt_is_no_robots_txt_for_all_tokens() {
    let h = "https://ai-none.test";
    let r = default_report(&[HostInput {
        host: h.to_string(),
        robots: HostRobots::NotFound,
        urls: forty_urls(),
    }]);
    let rows: Vec<&AccessRow> = r.rows.iter().filter(|x| x.host == h).collect();
    assert_eq!(rows.len(), Catalog::builtin().default_selection().len());
    for x in rows {
        assert_eq!(x.access, Access::NoRobotsTxt, "{}", x.token);
        assert_eq!(x.access.as_str(), "No robots.txt");
        assert!(x.homepage_allowed, "{}: = erlaubt", x.token);
        assert_eq!(x.blocked_urls, 0, "{}: = erlaubt", x.token);
    }
    for u in &r.urls {
        assert!(u.blocked_categories().is_empty());
    }
}

// spec: AI-121, CRAWL-100
// acceptance: independent
// robots.txt mit 5xx → `robots.txt Error` (Festlegung: unabhängig von der Policy heißt der Access-Wert so).
#[test]
fn unreachable_robots_txt_is_robots_txt_error_under_both_policies() {
    let h = "https://ai-5xx.test";
    for policy in [
        RobotsUnreachablePolicy::BlockHost,
        RobotsUnreachablePolicy::AllowAll,
    ] {
        let r = evaluate(
            Catalog::builtin(),
            &[HostInput {
                host: h.to_string(),
                robots: HostRobots::Error,
                urls: forty_urls(),
            }],
            &AccessOptions {
                tokens: TokenSelection::NonDeprecated,
                unreachable_policy: policy,
            },
        );
        assert!(!r.rows.is_empty());
        for x in &r.rows {
            assert_eq!(x.access, Access::RobotsTxtError, "{policy:?} {}", x.token);
            assert_eq!(x.access.as_str(), "robots.txt Error");
        }
    }
}

// spec: AI-121, CRAWL-100
// acceptance: independent
// Behandlung wie CRAWL-100: bei „Host als gesperrt behandeln“ (Default) wären alle URLs gesperrt, bei „Alles erlauben“
// keine.
#[test]
fn unreachable_robots_txt_follows_the_unreachable_policy() {
    let h = "https://ai-5xx.test";
    let mk = || HostInput {
        host: h.to_string(),
        robots: HostRobots::Error,
        urls: forty_urls(),
    };
    let block = evaluate(Catalog::builtin(), &[mk()], &AccessOptions::default());
    let g = row(&block, h, "OAI-SearchBot");
    assert_eq!(g.blocked_urls, 40, "Default-Policy sperrt den Host");
    assert!(!g.homepage_allowed);
    let allow = evaluate(
        Catalog::builtin(),
        &[mk()],
        &AccessOptions {
            tokens: TokenSelection::NonDeprecated,
            unreachable_policy: RobotsUnreachablePolicy::AllowAll,
        },
    );
    let g = row(&allow, h, "OAI-SearchBot");
    assert_eq!(g.blocked_urls, 0);
    assert!(g.homepage_allowed);
}

// spec: AI-121
// acceptance: independent
// Kontroll-Tokens werden wie Crawler ausgewertet; Spalte `Note` erklärt, dass sie nur die Verwendung steuern.
#[test]
fn control_tokens_are_evaluated_like_crawlers_with_a_note() {
    let h = "https://ai-control.test";
    let r = default_report(&[host_with(h, "ai-control.txt", forty_urls())]);
    assert!(REPORT_COLUMNS.contains(&"Note"));
    for t in ["Google-Extended", "Applebot-Extended"] {
        let x = row(&r, h, t);
        assert_eq!(x.access, Access::Blocked, "{t}");
        assert_eq!(x.matched_group.as_str(), t);
        assert_eq!(x.blocked_urls, 40);
        let note = x.note.unwrap_or_else(|| panic!("{t}: Note fehlt"));
        assert!(!note.as_str().trim().is_empty());
    }
    // Crawler-Tokens tragen diesen Hinweis nicht
    let gpt = row(&r, h, "GPTBot");
    assert_eq!(gpt.access, Access::Allowed);
    let control_note = row(&r, h, "Google-Extended").note;
    assert_ne!(gpt.note, control_note);
    // ohne Sperre: Kontroll-Token `Allowed` mit derselben Note
    let r2 = default_report(&[host_with(h, "ai-gptbot.txt", forty_urls())]);
    let ge = row(&r2, h, "Google-Extended");
    assert_eq!(ge.access, Access::Allowed);
    assert_eq!(ge.note, control_note);
}

// spec: AI-121
// acceptance: independent
// Report-Spalten der Spec.
#[test]
fn report_has_the_spec_columns() {
    for c in [
        "Host",
        "Token",
        "Operator",
        "Category",
        "Matched Group",
        "Homepage Allowed",
        "Blocked URLs",
        "Blocked %",
        "Access",
        "Matched Rule",
    ] {
        assert!(REPORT_COLUMNS.contains(&c), "Spalte {c} fehlt");
    }
}

// ---------------------------------------------------------------------------------------------------
// Testfixture-Hosts aus AI-121 und Kategorien je URL

// spec: AI-121
// acceptance: independent
// `ai-block-all.test`: alle KI-Tokens gesperrt.
#[test]
fn block_all_host_blocks_every_token() {
    let h = "https://ai-block-all.test";
    let r = default_report(&[host_with(h, "ai-block-all.txt", forty_urls())]);
    for x in r.rows.iter().filter(|x| x.host == h) {
        assert_eq!(x.access, Access::Blocked, "{}", x.token);
        assert_eq!(x.matched_group.as_str(), x.token);
    }
    let u = r.url(1).unwrap();
    assert_eq!(
        column_tokens(&u.column(Category::Search)),
        table_tokens("search", false).into_iter().collect()
    );
    assert_eq!(
        column_tokens(&u.column(Category::Training)),
        table_tokens("training", false).into_iter().collect()
    );
    assert_eq!(
        column_tokens(&u.column(Category::UserFetch)),
        table_tokens("user-fetch", false).into_iter().collect()
    );
    let cats: BTreeSet<Category> = u.blocked_categories().into_iter().collect();
    assert_eq!(
        cats,
        [Category::Training, Category::Search, Category::UserFetch]
            .into_iter()
            .collect()
    );
}

// spec: AI-121
// acceptance: independent
// `ai-mixed.test`: Training gesperrt, Suche erlaubt → je URL nur Kategorie Training.
#[test]
fn mixed_host_blocks_training_only() {
    let h = "https://ai-mixed.test";
    let r = default_report(&[host_with(h, "ai-mixed.txt", forty_urls())]);
    for t in table_tokens("training", false) {
        assert_eq!(row(&r, h, &t).access, Access::Blocked, "{t}");
    }
    for t in table_tokens("search", false) {
        assert_eq!(row(&r, h, &t).access, Access::Allowed, "{t}");
    }
    for u in &r.urls {
        assert_eq!(u.blocked_categories(), vec![Category::Training]);
        assert!(u.column(Category::Search).is_empty());
        assert!(u.column(Category::UserFetch).is_empty());
        assert!(column_tokens(&u.column(Category::Training)).contains("GPTBot"));
    }
}

// spec: AI-121
// acceptance: independent
// `ai-search-blocked.test`: Suche gesperrt, Training erlaubt → je URL nur Kategorie Search.
#[test]
fn search_blocked_host_blocks_search_only() {
    let h = "https://ai-search-blocked.test";
    let r = default_report(&[host_with(h, "ai-search-blocked.txt", forty_urls())]);
    for t in table_tokens("search", false) {
        assert_eq!(row(&r, h, &t).access, Access::Blocked, "{t}");
    }
    for t in table_tokens("training", false) {
        assert_eq!(row(&r, h, &t).access, Access::Allowed, "{t}");
    }
    for u in &r.urls {
        assert_eq!(u.blocked_categories(), vec![Category::Search]);
        assert_eq!(
            column_tokens(&u.column(Category::Search)),
            table_tokens("search", false).into_iter().collect()
        );
        assert!(u.column(Category::Training).is_empty());
    }
}

// spec: AI-121
// acceptance: independent
// Nur Nutzerabrufe gesperrt → Kategorie User-Fetch; ohne `*`-Gruppe greift für die anderen `none`.
#[test]
fn user_fetch_blocked_host_and_matched_group_none() {
    let h = "https://ai-uf.test";
    let r = default_report(&[host_with(h, "ai-user-fetch-blocked.txt", forty_urls())]);
    for t in table_tokens("user-fetch", false) {
        assert_eq!(row(&r, h, &t).access, Access::Blocked, "{t}");
    }
    let gpt = row(&r, h, "GPTBot");
    assert_eq!(gpt.matched_group, MatchedGroup::None);
    assert_eq!(gpt.matched_group.as_str(), "none");
    assert_eq!(gpt.access, Access::Allowed);
    for u in &r.urls {
        assert_eq!(u.blocked_categories(), vec![Category::UserFetch]);
    }
}

// ---------------------------------------------------------------------------------------------------
// Grenzfälle

// spec: AI-121, CRAWL-100
// acceptance: independent
// Leere robots.txt: alles erlaubt, keine Gruppe greift.
#[test]
fn empty_robots_txt_allows_everything() {
    let h = "https://ai-empty.test";
    let r = default_report(&[host_with(h, "ai-empty.txt", forty_urls())]);
    for x in r.rows.iter().filter(|x| x.host == h) {
        assert_eq!(x.access, Access::Allowed, "{}", x.token);
        assert_eq!(x.matched_group.as_str(), "none", "{}", x.token);
        assert!(x.matched_rule_display().is_empty());
    }
}

// spec: AI-121
// acceptance: independent
// Nur `*`-Gruppe: alle Tokens `Matched Group = *`, Teilsperre unter /privat/.
#[test]
fn star_only_robots_txt_applies_to_every_token() {
    let h = "https://ai-star.test";
    let mut urls = forty_urls();
    urls.push((41, "/privat/a".to_string()));
    let r = default_report(&[host_with(h, "ai-star-only.txt", urls)]);
    for x in r.rows.iter().filter(|x| x.host == h) {
        assert_eq!(x.matched_group, MatchedGroup::Wildcard, "{}", x.token);
        assert_eq!(x.access, Access::PartiallyBlocked, "{}", x.token);
        assert_eq!(x.blocked_urls, 1);
        assert!(x.homepage_allowed);
    }
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Token-Vergleich case-insensitiv; `Matched Group` nennt das Token.
#[test]
fn token_case_in_robots_txt_does_not_matter() {
    let h = "https://ai-case.test";
    let r = default_report(&[host_with(h, "ai-case.txt", forty_urls())]);
    for t in ["GPTBot", "Claude-SearchBot"] {
        let x = row(&r, h, t);
        assert_eq!(x.access, Access::Blocked, "{t}");
        assert!(
            x.matched_group.as_str().eq_ignore_ascii_case(t),
            "{t}: {:?}",
            x.matched_group
        );
    }
    assert_eq!(row(&r, h, "ClaudeBot").access, Access::Allowed);
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Mehrere Gruppen für dasselbe Token werden zusammengeführt; `*` gilt dann nicht.
#[test]
fn multiple_groups_for_the_same_token_are_merged() {
    let h = "https://ai-merged.test";
    let urls = vec![
        (1, "/".to_string()),
        (2, "/a/x".to_string()),
        (3, "/b/x".to_string()),
        (4, "/c/x".to_string()),
    ];
    let r = default_report(&[host_with(h, "ai-merged.txt", urls)]);
    let gpt = row(&r, h, "GPTBot");
    assert_eq!(gpt.blocked_urls, 2, "/a/ und /b/ gesperrt, /c/ nicht");
    assert_eq!(gpt.blocked_pct_display(), "50.00");
    assert_eq!(gpt.access, Access::PartiallyBlocked);
    assert!(!column_tokens(&r.url(4).unwrap().column(Category::Training)).contains("GPTBot"));
    let other = row(&r, h, "CCBot");
    assert_eq!(other.blocked_urls, 1, "* sperrt nur /c/");
}

// spec: AI-121
// acceptance: independent
// Standardauswahl: veraltete Tokens erscheinen nicht; explizite Auswahl (Schreibweise egal) liefert genau diese Tokens
// in Katalog-Schreibweise, auch veraltete.
#[test]
fn token_selection_default_and_explicit() {
    let h = "https://ai-block-all.test";
    let mk = || host_with(h, "ai-block-all.txt", forty_urls());
    let r = default_report(&[mk()]);
    let tokens: BTreeSet<&str> = r.rows.iter().map(|x| x.token.as_str()).collect();
    assert!(!tokens.contains("anthropic-ai"));
    assert!(!tokens.contains("Google-NotebookLM"));
    for t in table_tokens("*", false) {
        assert!(
            tokens.contains(t.as_str()),
            "{t} fehlt in der Standardauswahl"
        );
    }
    let r = evaluate(
        Catalog::builtin(),
        &[mk()],
        &AccessOptions {
            tokens: TokenSelection::Tokens(vec!["gptbot".into(), "ANTHROPIC-AI".into()]),
            unreachable_policy: RobotsUnreachablePolicy::BlockHost,
        },
    );
    let tokens: Vec<&str> = r.rows.iter().map(|x| x.token.as_str()).collect();
    assert_eq!(tokens.len(), 2, "{tokens:?}");
    assert!(tokens.contains(&"GPTBot"));
    assert!(tokens.contains(&"anthropic-ai"));
}

// spec: AI-121
// acceptance: independent
// Ergebnis je (Host, Token): Hosts mit anderem Schema bzw. Port sind getrennte Hosts (Host = Schema+Host+Port).
#[test]
fn host_means_scheme_host_and_port() {
    let hosts = [
        host_with("https://ai.test", "ai-gptbot.txt", forty_urls()),
        host_with("http://ai.test", "ai-empty.txt", forty_urls()),
        host_with("https://ai.test:8443", "ai-partial.txt", forty_urls()),
    ];
    let r = default_report(&hosts);
    assert_eq!(row(&r, "https://ai.test", "GPTBot").access, Access::Blocked);
    assert_eq!(row(&r, "http://ai.test", "GPTBot").access, Access::Allowed);
    assert_eq!(
        row(&r, "https://ai.test:8443", "ClaudeBot").access,
        Access::PartiallyBlocked
    );
    assert_eq!(
        row(&r, "https://ai.test", "ClaudeBot").access,
        Access::Allowed
    );
    let n = Catalog::builtin().default_selection().len();
    assert_eq!(r.rows.len(), 3 * n);
}

// ---------------------------------------------------------------------------------------------------
// Aus der Crawl-Datei (Grundmenge: gecrawlte interne HTML-URLs; nur gespeicherte robots.txt, kein Request)

fn crawl_file(dir: &Path) -> CrawlFile {
    let created = Timestamp::from_db("2026-11-24T08:00:00.000Z").unwrap();
    let init = CrawlInit {
        crawl_id: SeededIds::new(11).new_crawl_id(created),
        mode: CrawlMode::Spider,
        start_url: Some("https://ai-partial.test/".into()),
        list_source: None,
        project: "Abnahme".into(),
        name: "AI-121".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    CrawlFile::create(&dir.join("ai121.ospider"), &init, StorageProfile::Balanced).unwrap()
}

fn insert_url(f: &CrawlFile, id: i64, origin: &str, path: &str, internal: bool, class: &str) {
    let url = format!("{origin}{path}");
    f.conn()
        .execute(
            "INSERT INTO urls (id, url, address, origin, is_internal, state, discovered_via, folder_depth, \
             url_length, status_code, content_class) VALUES (?1, ?2, ?2, ?3, ?4, 'done', 'link', 0, 0, 200, ?5)",
            rusqlite::params![id, url, origin, i64::from(internal), class],
        )
        .unwrap();
}

fn insert_robots(f: &CrawlFile, origin: &str, status: &str, code: i64, body: Option<&str>) {
    f.conn()
        .execute(
            "INSERT INTO robots_files (origin, robots_url, fetch_status, status_code, attempts, body) \
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            rusqlite::params![origin, format!("{origin}/robots.txt"), status, code, body],
        )
        .unwrap();
}

// spec: AI-121
// acceptance: independent
// Aus einer Crawl-Datei: 10 von 40 gecrawlten internen HTML-URLs → 25.00; Bilder unter /intern/ und externe URLs zählen
// nicht; Host ohne robots.txt (404) → `No robots.txt`; 503 → `robots.txt Error`; je URL die Kategorien.
#[test]
fn access_report_from_crawl_file_uses_internal_html_urls() {
    let dir = tempfile::tempdir().unwrap();
    let f = crawl_file(dir.path());
    let partial = "https://ai-partial.test";
    let none = "https://ai-none.test";
    let err = "https://ai-5xx.test";
    let ext = "https://extern.test";
    let mut id = 0;
    for (_, path) in forty_urls() {
        id += 1;
        insert_url(&f, id, partial, &path, true, "HTML");
    }
    for i in 1..=5 {
        id += 1;
        insert_url(
            &f,
            id,
            partial,
            &format!("/intern/bild-{i}.png"),
            true,
            "Image",
        );
    }
    id += 1;
    insert_url(&f, id, ext, "/intern/x", false, "HTML");
    id += 1;
    insert_url(&f, id, none, "/", true, "HTML");
    id += 1;
    insert_url(&f, id, err, "/", true, "HTML");
    insert_robots(
        &f,
        partial,
        "ok",
        200,
        Some(&robots_fixture("ai-partial.txt")),
    );
    insert_robots(&f, none, "not_found", 404, None);
    insert_robots(&f, err, "unreachable", 503, None);
    insert_robots(&f, ext, "ok", 200, Some("User-agent: *\nDisallow: /\n"));

    let r = access_report(
        &f,
        Catalog::builtin(),
        &spec_parser,
        &AccessOptions::default(),
    )
    .expect("Report");
    let c = row(&r, partial, "ClaudeBot");
    assert_eq!(c.blocked_urls, 10);
    assert_eq!(c.total_urls, 40);
    assert_eq!(c.blocked_pct_display(), "25.00");
    assert_eq!(c.access, Access::PartiallyBlocked);
    assert!(
        r.rows.iter().all(|x| x.host != ext),
        "externer Host darf nicht ausgewertet werden"
    );
    for x in r.rows.iter().filter(|x| x.host == none) {
        assert_eq!(x.access, Access::NoRobotsTxt, "{}", x.token);
    }
    for x in r.rows.iter().filter(|x| x.host == err) {
        assert_eq!(x.access, Access::RobotsTxtError, "{}", x.token);
    }
    // je URL: nur HTML-URLs der internen Hosts, /intern/ für Training (ClaudeBot) gesperrt
    assert!(r.urls.iter().all(|u| u.host != ext));
    assert_eq!(r.urls.iter().filter(|u| u.host == partial).count(), 40);
    let blocked: Vec<i64> = r
        .urls
        .iter()
        .filter(|u| u.host == partial && !u.blocked_categories().is_empty())
        .map(|u| u.url_id)
        .collect();
    assert_eq!(blocked, (31..=40).collect::<Vec<_>>());
    assert_eq!(r.url(31).unwrap().column(Category::Training), "ClaudeBot");
}

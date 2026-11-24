//! robots.txt-Auswertung je AI-Crawler (AI-121).
//!
//! Fixtures: `tests/fixtures/ai/search/robots/` (Hosts `ai-block-all.test`, `ai-mixed.test`,
//! `ai-search-blocked.test`, `ai-partial.test`, `ai-none.test`, `ai-5xx.test`) und `tests/fixtures/crawl-robots`.
//!
//! **Parser:** Die Tests werten mit dem RFC-9309-Parser des Crawl-Kerns aus. `os-analyze` hat keine
//! (Dev-)Abhängigkeit auf `os-fetch`; die Quelldateien `os-fetch/src/robots/{pattern,parser}.rs` werden deshalb
//! unverändert per `#[path]` eingebunden (derselbe Code, kein zweiter Parser). Die Gruppenauswahl („greift eine
//! eigene Gruppe?“) liest der Adapter über die öffentliche Schnittstelle desselben Parsers (siehe
//! [`Kernel::group_match`]); in der Anwendung liefert sie `os_fetch::robots::RobotsTxt` direkt (Bericht M4-S01).

#[allow(dead_code, clippy::all)]
#[path = "../../os-fetch/src/robots/pattern.rs"]
mod pattern;

#[allow(dead_code, clippy::all)]
#[path = "../../os-fetch/src/robots/parser.rs"]
mod parser;

use os_analyze::ai_search::catalog::{Catalog, Category};
use os_analyze::ai_search::robots::{
    access_report, evaluate, Access, AccessNote, AccessOptions, AccessReport, GroupMatch,
    HostInput, HostRobots, MatchedGroup, RobotsRules, RuleHit, TokenSelection, INTERNAL_COLUMNS,
    REPORT_COLUMNS,
};
use os_engine::testing::{site_fetcher, test_deps};
use os_engine::{start_crawl, CrawlRequest, CrawlSource};
use os_model::{CrawlConfig, RobotsUnreachablePolicy};
use os_store::{CrawlFile, OpenMode};
use os_testsite::{fixtures_root, TestSite};

/// Adapter: der Parser des Crawl-Kerns hinter [`RobotsRules`].
struct Kernel {
    txt: parser::RobotsTxt,
    /// Dieselbe Datei mit vorangestellter Prüfgruppe `User-agent: *` (`Crawl-delay` = [`PROBE_DELAY`]).
    probe: parser::RobotsTxt,
    has_star: bool,
}

/// Markierung der Prüfgruppe (kommt in echten Dateien praktisch nicht vor).
const PROBE_DELAY: f64 = 7_777.25;

impl Kernel {
    fn parse(body: &str) -> Kernel {
        let body = body.strip_prefix('\u{feff}').unwrap_or(body);
        // `Disallow:` ohne Wert beendet die Gruppe, ohne eine Regel anzulegen (K-29).
        let probe = format!("User-agent: *\nCrawl-delay: {PROBE_DELAY}\nDisallow:\n{body}");
        Kernel {
            txt: parser::parse(body),
            probe: parser::parse(&probe),
            has_star: body.lines().any(|l| {
                let content = l.split('#').next().unwrap_or("");
                content.split_once(':').is_some_and(|(k, v)| {
                    matches!(
                        k.trim().to_ascii_lowercase().as_str(),
                        "user-agent" | "useragent" | "user agent"
                    ) && v.trim() == "*"
                })
            }),
        }
    }
}

impl RobotsRules for Kernel {
    /// Spezifisch, wenn für das Token nicht die `*`-Gruppen gelten: In der Prüfdatei liefert der Parser die
    /// Markierung der vorangestellten `*`-Gruppe genau dann, wenn keine Gruppe das Token nennt.
    fn group_match(&self, token: &str) -> GroupMatch {
        if self.probe.crawl_delay(token) != Some(PROBE_DELAY) {
            GroupMatch::Specific
        } else if self.has_star {
            GroupMatch::Wildcard
        } else {
            GroupMatch::None
        }
    }

    fn evaluate(&self, token: &str, path_and_query: &str) -> Option<RuleHit> {
        self.txt.evaluate(token, path_and_query).map(|m| RuleHit {
            allowed: m.allowed,
            line_no: m.line_no,
            rule: m.line,
        })
    }
}

fn kernel(body: &str) -> Box<dyn RobotsRules> {
    Box::new(Kernel::parse(body))
}

fn host(name: &str, body: &str, paths: &[&str]) -> HostInput {
    HostInput {
        host: name.to_string(),
        robots: HostRobots::Rules(kernel(body)),
        urls: paths
            .iter()
            .enumerate()
            .map(|(i, p)| (i as i64 + 1, (*p).to_string()))
            .collect(),
    }
}

fn all_tokens() -> AccessOptions {
    AccessOptions::default()
}

// spec: AI-121
// `User-agent: GPTBot` / `Disallow: /` und `User-agent: *` / `Allow: /` → GPTBot `Blocked` (Gruppe GPTBot),
// OAI-SearchBot `Allowed` (Gruppe `*`).
#[test]
fn gptbot_blocked_oai_searchbot_allowed() {
    let body = "User-agent: GPTBot\nDisallow: /\n\nUser-agent: *\nAllow: /\n";
    let r = evaluate(
        Catalog::builtin(),
        &[host("https://site.test", body, &["/", "/a"])],
        &all_tokens(),
    );
    let gpt = r.row("https://site.test", "GPTBot").unwrap();
    assert_eq!(gpt.access, Access::Blocked);
    assert_eq!(gpt.matched_group, MatchedGroup::Token("GPTBot".into()));
    assert_eq!(gpt.matched_group.as_str(), "GPTBot");
    assert!(!gpt.homepage_allowed);
    assert_eq!(gpt.matched_rule_display(), "Disallow: /");
    assert_eq!(gpt.blocked_pct_display(), "100.00");
    let oai = r.row("https://site.test", "OAI-SearchBot").unwrap();
    assert_eq!(oai.access, Access::Allowed);
    assert_eq!(oai.matched_group, MatchedGroup::Wildcard);
    assert_eq!(oai.matched_group.as_str(), "*");
    assert!(oai.homepage_allowed);
    assert_eq!(oai.matched_rule_display(), "Allow: /");
    assert_eq!(oai.blocked_urls, 0);
    // Je URL: gesperrt nur für Training (GPTBot).
    for u in &r.urls {
        assert_eq!(u.training, ["GPTBot"]);
        assert!(u.search.is_empty() && u.user_fetch.is_empty());
        assert_eq!(u.blocked_categories(), [Category::Training]);
        assert_eq!(u.column(Category::Training), "GPTBot");
    }
    assert_eq!(
        REPORT_COLUMNS,
        [
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
            "Note"
        ]
    );
    assert_eq!(
        INTERNAL_COLUMNS,
        [
            "AI Search Bots Blocked",
            "AI Training Bots Blocked",
            "AI User Fetchers Blocked"
        ]
    );
}

fn crawl_robots_body() -> String {
    std::fs::read_to_string(fixtures_root().join("crawl-robots/robots.txt")).unwrap()
}

fn rule(r: &AccessReport, host: &str, token: &str) -> (bool, u32, String) {
    let row = r.row(host, token).unwrap();
    let hit = row.matched_rule.clone().unwrap();
    (hit.allowed, hit.line_no, hit.rule)
}

// spec: AI-121
// spec: CRAWL-098
// Gruppenauswahl- und Vorrangregeln aus CRAWL-098: die Testfälle von `crawl-robots` mit KI-Tokens wiederholt.
#[test]
fn crawl_robots_cases_repeated_with_ai_tokens() {
    let original = crawl_robots_body();
    // KI-Tokens ohne eigene Gruppe: `*` sperrt alles (Zeile 4); OAI-SearchBot hat eine eigene Gruppe ([M4]).
    let k = Kernel::parse(&original);
    assert_eq!(k.group_match("GPTBot"), GroupMatch::Wildcard);
    assert_eq!(k.group_match("OAI-SearchBot"), GroupMatch::Specific);
    assert_eq!(k.group_match("oai-searchbot"), GroupMatch::Specific);
    assert_eq!(k.group_match("Googlebot"), GroupMatch::Specific);
    let gpt = k.evaluate("GPTBot", "/frei/a").unwrap();
    assert_eq!(
        (gpt.allowed, gpt.line_no, gpt.rule.as_str()),
        (false, 4, "Disallow: /")
    );
    let oai = k.evaluate("OAI-SearchBot", "/privat/x").unwrap();
    assert_eq!((oai.allowed, oai.line_no), (false, 30));
    assert_eq!(k.evaluate("OAI-SearchBot", "/frei/a"), None);
    assert!(
        k.evaluate("OAI-SearchBot", "/privat/oeffentlich")
            .unwrap()
            .allowed
    );

    // Die OpenSpider-Fälle mit ClaudeBot als Token derselben Gruppen (Zeilen unverändert).
    let body = original
        .replace("User-agent: OpenSpider", "User-agent: ClaudeBot")
        .replace("user-agent: openspider", "user-agent: claudebot");
    let k = Kernel::parse(&body);
    let t = "ClaudeBot";
    let cases: [(&str, Option<(bool, u32)>); 14] = [
        ("/privat/x", Some((false, 10))),
        ("/privat/", Some((false, 10))),
        ("/privat/oeffentlich", Some((true, 11))),
        ("/privat/oeffentlich/unterseite", Some((true, 11))),
        ("/privat", None),
        ("/seite", Some((true, 14))),
        ("/seite2", Some((false, 15))),
        ("/seite/unterseite", Some((false, 15))),
        ("/dokument.pdf", Some((false, 12))),
        ("/dokument.pdf?download=1", None),
        ("/liste?sessionid=abc", Some((false, 13))),
        ("/liste?seite=2&sessionid=abc", None),
        ("/zweite-gruppe/a", Some((false, 23))),
        ("/mit-leerzeichen/a", Some((false, 16))),
    ];
    for (path, expected) in cases {
        let got = k.evaluate(t, path).map(|h| (h.allowed, h.line_no));
        assert_eq!(got, expected, "{path}");
        // Andere Schreibweise des Tokens wählt dieselbe Gruppe.
        assert_eq!(
            k.evaluate("CLAUDEBOT", path)
                .map(|h| (h.allowed, h.line_no)),
            expected
        );
    }
    assert_eq!(k.evaluate(t, "/unbekannte-direktive/a"), None);
    assert_eq!(k.group_match(t), GroupMatch::Specific);

    // Über die Auswertung: ClaudeBot teilweise gesperrt, Startseite erlaubt (keine Regel), andere KI-Tokens `*`.
    let paths: Vec<&str> = cases.iter().map(|(p, _)| *p).collect();
    let r = evaluate(
        Catalog::builtin(),
        &[host("http://robots.test", &body, &paths)],
        &all_tokens(),
    );
    let claude = r.row("http://robots.test", t).unwrap();
    assert_eq!(
        claude.matched_group,
        MatchedGroup::Token("ClaudeBot".into())
    );
    assert_eq!(claude.access, Access::PartiallyBlocked);
    assert!(claude.homepage_allowed);
    assert_eq!(claude.matched_rule, None);
    assert_eq!(claude.blocked_urls, 8);
    assert_eq!(
        rule(&r, "http://robots.test", "PerplexityBot"),
        (false, 4, "Disallow: /".into())
    );
    assert_eq!(
        r.row("http://robots.test", "PerplexityBot").unwrap().access,
        Access::Blocked
    );
}

// spec: AI-121
// Kontroll-Tokens werden wie Crawler ausgewertet; die Spalte `Note` erklärt, dass sie nur die Verwendung steuern.
// Standardauswahl = alle nicht veralteten Tokens; eine eigene Auswahl ist möglich.
#[test]
fn control_tokens_are_evaluated_with_a_note_and_selection_works() {
    let body = "User-agent: Google-Extended\nUser-agent: Applebot-Extended\nDisallow: /\n";
    let r = evaluate(
        Catalog::builtin(),
        &[host("https://c.test", body, &["/", "/x"])],
        &all_tokens(),
    );
    for t in ["Google-Extended", "Applebot-Extended"] {
        let row = r.row("https://c.test", t).unwrap();
        assert_eq!(row.access, Access::Blocked, "{t}");
        assert_eq!(row.matched_group, MatchedGroup::Token(t.into()));
        assert_eq!(row.note, Some(AccessNote::ControlTokenUsageOnly));
        assert!(!row.note.unwrap().as_str().is_empty());
    }
    // Ohne passende und ohne `*`-Gruppe: `none`, erlaubt.
    let other = r.row("https://c.test", "GPTBot").unwrap();
    assert_eq!(other.matched_group, MatchedGroup::None);
    assert_eq!(other.matched_group.as_str(), "none");
    assert_eq!(other.access, Access::Allowed);
    assert_eq!(other.note, None);
    // Kontroll-Tokens erscheinen in keiner der drei Internal-Spalten.
    assert!(r.urls.iter().all(|u| u.blocked_categories().is_empty()));

    let tokens: Vec<&str> = r.rows.iter().map(|r| r.token.as_str()).collect();
    assert!(!tokens.contains(&"anthropic-ai") && !tokens.contains(&"Google-NotebookLM"));
    assert_eq!(tokens.len(), Catalog::builtin().default_selection().len());
    let only = AccessOptions {
        tokens: TokenSelection::Tokens(vec![
            "gptbot".into(),
            "anthropic-ai".into(),
            "gibt-es-nicht".into(),
        ]),
        ..Default::default()
    };
    let r = evaluate(
        Catalog::builtin(),
        &[host("https://c.test", body, &["/"])],
        &only,
    );
    let tokens: Vec<&str> = r.rows.iter().map(|r| r.token.as_str()).collect();
    assert_eq!(tokens, ["GPTBot", "anthropic-ai"]);
}

// spec: AI-121
// Host ohne robots.txt bzw. ohne Abruf; robots.txt mit 5xx nach CRAWL-100 (Policy des Crawls).
#[test]
fn missing_unfetched_and_failing_robots_txt() {
    let input = |robots| HostInput {
        host: "http://h.test".into(),
        robots,
        urls: vec![(1, "/".into()), (2, "/a".into())],
    };
    let r = evaluate(
        Catalog::builtin(),
        &[input(HostRobots::NotFound)],
        &all_tokens(),
    );
    assert!(r.rows.iter().all(|row| row.access == Access::NoRobotsTxt
        && row.homepage_allowed
        && row.blocked_urls == 0
        && row.matched_group == MatchedGroup::None));
    let r = evaluate(
        Catalog::builtin(),
        &[input(HostRobots::NotFetched)],
        &all_tokens(),
    );
    let row = r.row("http://h.test", "GPTBot").unwrap();
    assert_eq!(row.access, Access::NoRobotsTxt);
    assert_eq!(row.note, Some(AccessNote::RobotsNotFetched));

    let block = AccessOptions {
        unreachable_policy: RobotsUnreachablePolicy::BlockHost,
        ..Default::default()
    };
    let r = evaluate(Catalog::builtin(), &[input(HostRobots::Error)], &block);
    for row in &r.rows {
        assert_eq!(row.access, Access::RobotsTxtError, "{}", row.token);
        assert!(!row.homepage_allowed);
        assert_eq!(row.blocked_urls, 2);
    }
    assert!(!r.urls[0].search.is_empty() && !r.urls[0].training.is_empty());
    let allow = AccessOptions {
        unreachable_policy: RobotsUnreachablePolicy::AllowAll,
        ..Default::default()
    };
    let r = evaluate(Catalog::builtin(), &[input(HostRobots::Error)], &allow);
    for row in &r.rows {
        assert_eq!(row.access, Access::RobotsTxtError);
        assert!(row.homepage_allowed);
        assert_eq!(row.blocked_urls, 0);
    }
}

/// Crawlt einen virtuellen Host der Fixture-Site und öffnet die Crawl-Datei.
async fn crawl_host(
    site: &TestSite,
    host: &str,
    cfg: CrawlConfig,
) -> (CrawlFile, tempfile::TempDir) {
    let deps = test_deps();
    let fetcher = site_fetcher(site, &cfg, &deps);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ai.ospider");
    let handle = start_crawl(
        fetcher,
        CrawlRequest {
            config: cfg,
            source: CrawlSource::Spider {
                start_url: format!("http://{host}:{}/", site.port()),
            },
            file: path.clone(),
            project: "ai-s01".into(),
            name: host.into(),
        },
        deps,
    )
    .await
    .expect("Crawl startet");
    handle.wait().await.expect("Crawl endet");
    (
        CrawlFile::open(&path, OpenMode::ReadOnly).expect("Crawl-Datei"),
        dir,
    )
}

fn origin(site: &TestSite, host: &str) -> String {
    format!("http://{host}:{}", site.port())
}

fn report(file: &CrawlFile, opts: &AccessOptions) -> AccessReport {
    let parse = |body: &str| kernel(body);
    access_report(file, Catalog::builtin(), &parse, opts).expect("Report")
}

fn tokens_of(cat: Category) -> Vec<String> {
    Catalog::builtin()
        .default_selection()
        .into_iter()
        .filter(|e| e.category == cat)
        .map(|e| e.token.clone())
        .collect()
}

// spec: AI-121
// Fixture-Hosts gegen gespeicherte Crawls: Ergebnis je (Host, Token), Werte je URL, und die Auswertung erzeugt
// keine zusätzlichen HTTP-Requests (Request-Log der Fixture unverändert).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fixture_hosts_from_stored_crawls() {
    let site = TestSite::start_dir(fixtures_root().join("ai/search/robots"), 0)
        .await
        .expect("Testsite");
    let mut cfg = CrawlConfig::default();
    cfg.robots.retries = 0;
    let opts = AccessOptions::from_config(&cfg);

    // ai-block-all.test: alle KI-Tokens gesperrt, jeweils über die eigene Gruppe.
    let (file, _d1) = crawl_host(&site, "ai-block-all.test", cfg.clone()).await;
    let before = site.total_requests();
    let r = report(&file, &opts);
    assert_eq!(site.total_requests(), before, "keine zusätzlichen Requests");
    let h = origin(&site, "ai-block-all.test");
    assert_eq!(r.rows.len(), Catalog::builtin().default_selection().len());
    for row in &r.rows {
        assert_eq!(row.host, h);
        assert_eq!(row.access, Access::Blocked, "{}", row.token);
        assert_eq!(row.matched_group, MatchedGroup::Token(row.token.clone()));
        assert_eq!((row.blocked_urls, row.total_urls), (3, 3));
        assert_eq!(row.blocked_pct_display(), "100.00");
    }
    for u in &r.urls {
        assert_eq!(u.search, tokens_of(Category::Search));
        assert_eq!(u.training, tokens_of(Category::Training));
        assert_eq!(u.user_fetch, tokens_of(Category::UserFetch));
    }

    // ai-mixed.test: Training gesperrt, Suche erlaubt.
    let (file, _d2) = crawl_host(&site, "ai-mixed.test", cfg.clone()).await;
    let r = report(&file, &opts);
    let h = origin(&site, "ai-mixed.test");
    for t in tokens_of(Category::Training) {
        assert_eq!(r.row(&h, &t).unwrap().access, Access::Blocked, "{t}");
    }
    for t in tokens_of(Category::Search)
        .into_iter()
        .chain(tokens_of(Category::UserFetch))
    {
        let row = r.row(&h, &t).unwrap();
        assert_eq!(row.access, Access::Allowed, "{t}");
        assert_eq!(row.matched_group, MatchedGroup::Wildcard, "{t}");
    }
    assert!(r
        .urls
        .iter()
        .all(|u| u.blocked_categories() == [Category::Training]));

    // ai-search-blocked.test: Suche gesperrt, Training ausdrücklich erlaubt.
    let (file, _d3) = crawl_host(&site, "ai-search-blocked.test", cfg.clone()).await;
    let r = report(&file, &opts);
    let h = origin(&site, "ai-search-blocked.test");
    for t in tokens_of(Category::Search) {
        assert_eq!(r.row(&h, &t).unwrap().access, Access::Blocked, "{t}");
    }
    for t in tokens_of(Category::Training) {
        let row = r.row(&h, &t).unwrap();
        assert_eq!(row.access, Access::Allowed, "{t}");
        assert_eq!(row.matched_group, MatchedGroup::Token(t.clone()));
    }
    assert!(r
        .urls
        .iter()
        .all(|u| u.blocked_categories() == [Category::Search]));
    assert_eq!(r.urls.len(), 3);

    // ai-partial.test: `Disallow: /intern/` für ClaudeBot, 10 von 40 HTML-URLs → Partially Blocked, 25.00.
    let (file, _d4) = crawl_host(&site, "ai-partial.test", cfg.clone()).await;
    let before = site.total_requests();
    let r = report(&file, &opts);
    assert_eq!(site.total_requests(), before, "keine zusätzlichen Requests");
    let h = origin(&site, "ai-partial.test");
    let claude = r.row(&h, "ClaudeBot").unwrap();
    assert_eq!((claude.blocked_urls, claude.total_urls), (10, 40));
    assert_eq!(claude.access, Access::PartiallyBlocked);
    assert_eq!(claude.blocked_pct_display(), "25.00");
    assert!(claude.homepage_allowed);
    assert_eq!(
        claude.matched_group,
        MatchedGroup::Token("ClaudeBot".into())
    );
    let gpt = r.row(&h, "GPTBot").unwrap();
    assert_eq!(
        (gpt.access, gpt.matched_group.clone()),
        (Access::Allowed, MatchedGroup::None)
    );
    let blocked: Vec<_> = r.urls.iter().filter(|u| !u.training.is_empty()).collect();
    assert_eq!(blocked.len(), 10);
    assert!(blocked
        .iter()
        .all(|u| u.training == ["ClaudeBot"] && u.search.is_empty()));

    // ai-none.test: keine robots.txt (404) → alle Tokens `No robots.txt`.
    let (file, _d5) = crawl_host(&site, "ai-none.test", cfg.clone()).await;
    let r = report(&file, &opts);
    let h = origin(&site, "ai-none.test");
    assert!(!r.rows.is_empty());
    assert!(r.rows.iter().all(|row| row.host == h
        && row.access == Access::NoRobotsTxt
        && row.access.as_str() == "No robots.txt"));
    assert!(r.urls.iter().all(|u| u.blocked_categories().is_empty()));

    // ai-5xx.test: 503 → `robots.txt Error`; gecrawlt mit „Alles erlauben“, ausgewertet nach CRAWL-100.
    let mut allow = cfg.clone();
    allow.robots.unreachable_policy = RobotsUnreachablePolicy::AllowAll;
    let (file, _d6) = crawl_host(&site, "ai-5xx.test", allow.clone()).await;
    let h = origin(&site, "ai-5xx.test");
    let r = report(&file, &AccessOptions::from_config(&allow));
    assert!(r.rows.iter().all(|row| row.access == Access::RobotsTxtError
        && row.access.as_str() == "robots.txt Error"
        && row.blocked_urls == 0
        && row.host == h));
    assert_eq!(r.row(&h, "GPTBot").unwrap().total_urls, 2);
    let r = report(&file, &AccessOptions::from_config(&cfg));
    assert!(r.rows.iter().all(|row| row.access == Access::RobotsTxtError
        && row.blocked_urls == 2
        && !row.homepage_allowed));
}

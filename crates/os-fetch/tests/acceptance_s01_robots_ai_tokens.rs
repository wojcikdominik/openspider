//! Unabhängige Abnahmetests M4-S01: Gruppenauswahl und Vorrangregeln aus CRAWL-098 mit KI-Tokens im robots.txt-Parser
//! des Crawl-Kerns (AI-121: „Die Auswertung verwendet die Gruppenauswahl- und Vorrangregeln aus CRAWL-098 (Testfälle aus
//! `crawl-robots` mit KI-Tokens wiederholt)“; derselbe Parser wie CRAWL-098).
//!
//! Erwartungen ausschließlich aus `docs/spec/01-crawl.md` (CRAWL-098) und `docs/spec/08-ai.md` (AI-121).
//! Fixtures: `tests/fixtures/ai/s01-acceptance/robots/`.
// acceptance: independent

use std::path::{Path, PathBuf};

use os_fetch::robots::{parse, RobotsTxt};

fn robots(name: &str) -> RobotsTxt {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Repo-Wurzel")
        .join("tests")
        .join("fixtures")
        .join("ai")
        .join("s01-acceptance")
        .join("robots")
        .join(name);
    parse(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
}

fn allowed(txt: &RobotsTxt, token: &str, path: &str) -> bool {
    txt.evaluate(token, path).is_none_or(|m| m.allowed)
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Testfälle aus `crawl-robots` mit der Gruppe `OAI-SearchBot`: längste Regel gewinnt, `*` und `$`, Query im Pfad;
// `*` greift nicht, weil eine spezifische Gruppe passt.
#[test]
fn crawl_robots_cases_with_oai_searchbot() {
    let txt = robots("ai-precedence.txt");
    for token in ["OAI-SearchBot", "oai-searchbot", "OAI-SEARCHBOT"] {
        assert!(!allowed(&txt, token, "/privat/x"), "{token} /privat/x");
        assert!(
            allowed(&txt, token, "/privat/oeffentlich"),
            "{token} längeres Allow"
        );
        assert!(!allowed(&txt, token, "/datei.pdf"), "{token} /*.pdf$");
        assert!(
            allowed(&txt, token, "/datei.pdf?x=1"),
            "{token} $ verankert am Ende von Pfad+Query"
        );
        assert!(
            !allowed(&txt, token, "/liste?sessionid=7"),
            "{token} /*?sessionid="
        );
        assert!(allowed(&txt, token, "/seite"), "{token} Allow /seite$");
        assert!(
            !allowed(&txt, token, "/seite/unter"),
            "{token} Disallow /seite"
        );
        assert!(
            allowed(&txt, token, "/frei"),
            "{token}: `*` darf nicht greifen"
        );
    }
    // ohne eigene Gruppe gilt `*` (alles gesperrt)
    for token in ["GPTBot", "Claude-SearchBot", "PerplexityBot"] {
        assert!(!allowed(&txt, token, "/frei"), "{token}");
    }
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Gleichstand gleich langer Regeln: `Allow` gewinnt.
#[test]
fn tie_between_allow_and_disallow_is_allow() {
    let txt = robots("ai-tie.txt");
    assert!(allowed(&txt, "PerplexityBot", "/gleich"));
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Mehrere Gruppen für denselben Agenten werden zusammengeführt.
#[test]
fn groups_for_the_same_ai_token_are_merged() {
    let txt = robots("ai-merged.txt");
    assert!(!allowed(&txt, "GPTBot", "/a/x"));
    assert!(!allowed(&txt, "GPTBot", "/b/x"));
    assert!(allowed(&txt, "GPTBot", "/c/x"), "`*` gilt nicht für GPTBot");
    assert!(!allowed(&txt, "CCBot", "/c/x"));
    assert!(allowed(&txt, "CCBot", "/a/x"));
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Gruppengrenzen wie im Google-Parser: aufeinanderfolgende `User-agent`-Zeilen (mit `Sitemap:` dazwischen) bilden
// eine Gruppe; nach einer Regelzeile beginnt mit `User-agent` eine neue.
#[test]
fn group_boundaries_with_ai_tokens() {
    let txt = robots("ai-group-boundary.txt");
    assert!(!allowed(&txt, "GPTBot", "/"));
    assert!(!allowed(&txt, "ClaudeBot", "/"));
    assert!(allowed(&txt, "CCBot", "/"));
    assert!(!allowed(&txt, "CCBot", "/nur-ccbot/x"));
    assert!(!allowed(&txt, "GPTBot", "/nur-ccbot/"));
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Token-Vergleich case-insensitiv (Gruppenzeile klein bzw. groß geschrieben).
#[test]
fn token_comparison_is_case_insensitive() {
    let txt = robots("ai-case.txt");
    assert!(!allowed(&txt, "GPTBot", "/"));
    assert!(!allowed(&txt, "Claude-SearchBot", "/"));
    assert!(allowed(&txt, "ClaudeBot", "/"));
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Tokens mit gleichem Anfang sind verschiedene Agenten: `Applebot` gilt nicht für `Applebot-Extended`,
// `ClaudeBot` nicht für `Claude-SearchBot`, `Google-Extended` nicht für `Google-Agent`.
#[test]
fn similar_ai_tokens_do_not_share_groups() {
    let txt = robots("ai-prefix.txt");
    assert!(!allowed(&txt, "Applebot", "/"));
    assert!(allowed(&txt, "Applebot-Extended", "/"));
    assert!(!allowed(&txt, "Google-Extended", "/"));
    assert!(allowed(&txt, "Google-Agent", "/"));
    assert!(allowed(&txt, "Google-CloudVertexBot", "/"));
    assert!(!allowed(&txt, "ClaudeBot", "/"));
    assert!(allowed(&txt, "Claude-SearchBot", "/"));
    assert!(allowed(&txt, "Claude-User", "/"));
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// `Disallow:` (leer) erlaubt alles; die `*`-Gruppe greift für dieses Token nicht.
#[test]
fn empty_disallow_allows_everything_for_the_ai_token() {
    let txt = robots("ai-empty-disallow.txt");
    assert!(allowed(&txt, "Amazonbot", "/"));
    assert!(allowed(&txt, "Amazonbot", "/irgendwas"));
    assert!(!allowed(&txt, "YouBot", "/"));
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// BOM, CRLF, Kommentare und Leerzeichen vor dem Doppelpunkt werden toleriert.
#[test]
fn bom_crlf_comments_and_spaces_with_ai_token() {
    let txt = robots("ai-bom-crlf.txt");
    assert!(!allowed(&txt, "DuckAssistBot", "/privat/x"));
    assert!(allowed(&txt, "DuckAssistBot", "/x"));
    assert!(allowed(&txt, "GPTBot", "/privat/x"), "`*` erlaubt alles");
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Ohne passende Gruppe und ohne `*`-Gruppe gilt keine Regel.
#[test]
fn no_matching_group_and_no_star_allows() {
    let txt = robots("ai-no-star.txt");
    assert!(!allowed(&txt, "CCBot", "/"));
    assert!(allowed(&txt, "GPTBot", "/"));
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// Leere robots.txt: alles erlaubt.
#[test]
fn empty_robots_txt_allows_ai_tokens() {
    let txt = robots("ai-empty.txt");
    for t in ["GPTBot", "OAI-SearchBot", "Google-Extended"] {
        assert!(txt.evaluate(t, "/").is_none(), "{t}");
    }
}

// spec: AI-121, CRAWL-098
// acceptance: independent
// AI-121 Kriterium 1 im Parser: GPTBot gesperrt mit Zeilennummer und Regeltext (Grundlage für `Matched Rule`),
// OAI-SearchBot über `*` erlaubt.
#[test]
fn gptbot_rule_hit_carries_line_and_text() {
    let txt = robots("ai-gptbot.txt");
    let hit = txt.evaluate("GPTBot", "/").expect("Regel trifft");
    assert!(!hit.allowed);
    assert_eq!(
        hit.line_no, 3,
        "Zeile 1 Kommentar, 2 User-agent, 3 Disallow"
    );
    assert!(hit.line.contains("Disallow: /"), "{:?}", hit.line);
    assert!(allowed(&txt, "OAI-SearchBot", "/"));
    assert!(allowed(&txt, "OAI-SearchBot", "/intern/x"));
}

//! Unabhängige Abnahmetests M4-S01: AI-Crawler-Katalog (AI-120) und Speisung der KI-Crawler-Voreinstellungen
//! auf Bibliotheksebene (CRAWL-114 [M4]).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-120) und `docs/spec/01-crawl.md` (CRAWL-114 [M4]) sowie
//! den Festlegungen des Orchestrators (`ua_pattern` = Teilzeichenkette ohne Beachtung der Groß-/Kleinschreibung,
//! Pflichtfeld). Fixtures: `resources/ai/ai-crawlers.json` und `tests/fixtures/ai/s01-acceptance/`.
// acceptance: independent

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use os_analyze::ai_search::catalog::{
    Catalog, CatalogError, Category, CrawlerEntry, RespectsRobots, CATALOG_JSON,
};
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Repo-Wurzel")
        .to_path_buf()
}

fn fixture_dir() -> PathBuf {
    repo_root()
        .join("tests")
        .join("fixtures")
        .join("ai")
        .join("s01-acceptance")
}

fn fixture(name: &str) -> String {
    let mut p = fixture_dir();
    for part in name.split('/') {
        p = p.join(part);
    }
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("Fixture {}: {e}", p.display()))
}

fn shipped_catalog_path() -> PathBuf {
    repo_root()
        .join("resources")
        .join("ai")
        .join("ai-crawlers.json")
}

fn shipped_json() -> Value {
    let text = std::fs::read_to_string(shipped_catalog_path()).expect("ai-crawlers.json lesbar");
    serde_json::from_str(&text).expect("ai-crawlers.json ist JSON")
}

fn minimum() -> Vec<Value> {
    let v: Value = serde_json::from_str(&fixture("catalog-minimum.json")).unwrap();
    v["crawlers"].as_array().unwrap().clone()
}

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn category_of(name: &str) -> Category {
    match name {
        "training" => Category::Training,
        "search" => Category::Search,
        "user-fetch" => Category::UserFetch,
        "control-token" => Category::ControlToken,
        other => panic!("Kategorie {other} nicht in der Spec"),
    }
}

fn respects_of(name: &str) -> RespectsRobots {
    match name {
        "yes" => RespectsRobots::Yes,
        "partial" => RespectsRobots::Partial,
        "no" => RespectsRobots::No,
        "unknown" => RespectsRobots::Unknown,
        other => panic!("respects_robots {other} nicht in der Spec"),
    }
}

fn errors_of(name: &str) -> Vec<CatalogError> {
    match Catalog::parse(&fixture(name)) {
        Ok(c) => panic!(
            "{name}: Katalog wurde angenommen ({} Einträge), erwartet: Ablehnung",
            c.len()
        ),
        Err(errs) => {
            assert!(!errs.is_empty(), "{name}: Ablehnung ohne Fehlerangabe");
            errs
        }
    }
}

fn expected_preset_tokens_from_json(doc: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for e in doc["crawlers"].as_array().unwrap() {
        let deprecated = e
            .get("deprecated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let cat = s(e, "category").unwrap();
        if !deprecated && (cat == "search" || cat == "user-fetch") {
            out.insert(s(e, "token").unwrap().to_string());
        }
    }
    for t in ["GPTBot", "ClaudeBot", "Google-Extended"] {
        out.insert(t.to_string());
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// Inhaltstest gegen die Tabelle (AI-120, erstes Kriterium)

// spec: AI-120
// acceptance: independent
// Der mitgelieferte Katalog ist gültig, die eingebettete Fassung ist die Datei unter `resources/ai/`.
#[test]
fn shipped_catalog_is_valid_and_embedded() {
    let on_disk = std::fs::read_to_string(shipped_catalog_path()).unwrap();
    assert_eq!(
        CATALOG_JSON, on_disk,
        "eingebetteter Katalog weicht von resources/ai/ai-crawlers.json ab"
    );
    let parsed = Catalog::parse(&on_disk);
    assert!(parsed.is_ok(), "Katalog ungültig: {:?}", parsed.err());
    assert_eq!(parsed.unwrap().len(), Catalog::builtin().len());
}

// spec: AI-120
// acceptance: independent
// Alle 27 Tokens der Tabelle mit Betreiber, Kategorie, deprecated/replaced_by, docs_url und docs_note; zusätzliche
// Einträge sind erlaubt. Fixture: Abschrift der Tabelle (`catalog-minimum.json`).
#[test]
fn shipped_catalog_contains_all_27_table_tokens_with_their_attributes() {
    let min = minimum();
    assert_eq!(min.len(), 27, "Abschrift der Tabelle hat 27 Zeilen");
    let cat = Catalog::builtin();
    let mut problems = Vec::new();
    for row in &min {
        let token = s(row, "token").unwrap();
        let Some(e) = cat.entries().find(|e| e.token == token) else {
            problems.push(format!("{token}: fehlt (Schreibweise exakt)"));
            continue;
        };
        if e.operator != s(row, "operator").unwrap() {
            problems.push(format!("{token}: operator {:?}", e.operator));
        }
        if e.category != category_of(s(row, "category").unwrap()) {
            problems.push(format!("{token}: category {:?}", e.category));
        }
        let dep = row["deprecated"].as_bool().unwrap();
        if e.deprecated != dep {
            problems.push(format!("{token}: deprecated {}", e.deprecated));
        }
        if e.replaced_by.as_deref() != s(row, "replaced_by") {
            problems.push(format!("{token}: replaced_by {:?}", e.replaced_by));
        }
        if e.docs_url.as_deref() != s(row, "docs_url") {
            problems.push(format!("{token}: docs_url {:?}", e.docs_url));
        }
        let note = e.docs_note.as_deref().map(str::trim).unwrap_or("");
        if row["docs_note_required"].as_bool().unwrap() && note.is_empty() {
            problems.push(format!("{token}: docs_note fehlt"));
        }
        if let Some(part) = s(row, "docs_note_contains") {
            if !note.to_lowercase().contains(&part.to_lowercase()) {
                problems.push(format!(
                    "{token}: docs_note {note:?} enthält nicht {part:?}"
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "Abweichungen:\n{}",
        problems.join("\n")
    );
}

// spec: AI-120
// acceptance: independent
// Festgelegte respects_robots-Werte: Google-CloudVertexBot = yes; Google-GeminiNotebook, Google-NotebookLM,
// Google-Agent = no.
#[test]
fn google_respects_robots_values_follow_the_decision() {
    let cat = Catalog::builtin();
    for row in minimum() {
        if let Some(want) = s(&row, "respects_robots") {
            let token = s(&row, "token").unwrap();
            let e = cat.get(token).unwrap_or_else(|| panic!("{token} fehlt"));
            assert_eq!(e.respects_robots, respects_of(want), "{token}");
        }
    }
    assert_eq!(
        cat.get("Google-CloudVertexBot").unwrap().respects_robots,
        RespectsRobots::Yes
    );
    for t in ["Google-GeminiNotebook", "Google-NotebookLM", "Google-Agent"] {
        assert_eq!(
            cat.get(t).unwrap().respects_robots,
            RespectsRobots::No,
            "{t}"
        );
    }
}

// spec: AI-120
// acceptance: independent
// `Gemini-Deep-Research` ist ausdrücklich nicht aufgenommen.
#[test]
fn gemini_deep_research_is_not_in_the_catalog() {
    assert!(Catalog::builtin().get("Gemini-Deep-Research").is_none());
}

// spec: AI-120
// acceptance: independent
// Rohdatei: jeder Eintrag hat die Felder der Spec, Kategorien und respects_robots aus den erlaubten Werten,
// verified_at als Datum, docs_url https oder null mit nicht leerem docs_note, replaced_by auf vorhandenes Token,
// Tokens case-insensitiv eindeutig (Prüfung direkt am JSON, unabhängig vom Leser der Bibliothek).
#[test]
fn shipped_catalog_raw_json_satisfies_the_schema_rules() {
    let doc = shipped_json();
    let list = doc["crawlers"].as_array().expect("crawlers ist Liste");
    let mut seen = BTreeSet::new();
    let tokens: BTreeSet<String> = list
        .iter()
        .map(|e| s(e, "token").unwrap().to_ascii_lowercase())
        .collect();
    let mut problems = Vec::new();
    for e in list {
        let token = s(e, "token").unwrap_or("?");
        for f in [
            "token",
            "operator",
            "category",
            "ua_pattern",
            "ua_example",
            "respects_robots",
            "docs_url",
            "verified_at",
        ] {
            if e.get(f).is_none() {
                problems.push(format!("{token}: Feld {f} fehlt"));
            }
        }
        if !seen.insert(token.to_ascii_lowercase()) {
            problems.push(format!("{token}: doppelt"));
        }
        if !matches!(
            s(e, "category"),
            Some("training" | "search" | "user-fetch" | "control-token")
        ) {
            problems.push(format!("{token}: Kategorie {:?}", e.get("category")));
        }
        if !matches!(
            s(e, "respects_robots"),
            Some("yes" | "partial" | "no" | "unknown")
        ) {
            problems.push(format!(
                "{token}: respects_robots {:?}",
                e.get("respects_robots")
            ));
        }
        let date = s(e, "verified_at").unwrap_or("");
        if !is_iso_date(date) {
            problems.push(format!("{token}: verified_at {date:?}"));
        }
        match e.get("docs_url") {
            Some(Value::String(u)) if u.starts_with("https://") && u.len() > 8 => {}
            Some(Value::Null) => {
                if s(e, "docs_note").map(str::trim).unwrap_or("").is_empty() {
                    problems.push(format!("{token}: docs_url null ohne docs_note"));
                }
            }
            other => problems.push(format!("{token}: docs_url {other:?}")),
        }
        if let Some(r) = s(e, "replaced_by") {
            if !tokens.contains(&r.to_ascii_lowercase()) {
                problems.push(format!("{token}: replaced_by {r} unbekannt"));
            }
        }
    }
    assert!(problems.is_empty(), "Verstöße:\n{}", problems.join("\n"));
}

fn is_iso_date(d: &str) -> bool {
    let b = d.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let (Ok(y), Ok(m), Ok(day)) = (
        d[0..4].parse::<u32>(),
        d[5..7].parse::<u32>(),
        d[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max).contains(&day)
}

// spec: AI-120
// acceptance: independent
// Festlegung Orchestrator: `ua_pattern` ist Pflichtfeld (Teilzeichenkette). Geprüft für alle Einträge, die selbst
// crawlen (Kategorien training, search, user-fetch).
#[test]
fn every_crawling_entry_has_a_non_empty_ua_pattern() {
    let missing: Vec<&str> = Catalog::builtin()
        .entries()
        .filter(|e| e.category != Category::ControlToken)
        .filter(|e| {
            e.ua_pattern
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        })
        .map(|e| e.token.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "ua_pattern fehlt bzw. ist null/leer bei: {missing:?}"
    );
}

// spec: AI-120
// acceptance: independent
// Festlegung Orchestrator: `ua_pattern` ist Pflichtfeld – auch bei Kontroll-Tokens (getrennt geführt, weil
// Kontroll-Tokens laut Spec nicht selbst crawlen).
#[test]
fn control_tokens_have_a_non_empty_ua_pattern() {
    let missing: Vec<&str> = Catalog::builtin()
        .entries()
        .filter(|e| e.category == Category::ControlToken)
        .filter(|e| {
            e.ua_pattern
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        })
        .map(|e| e.token.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "ua_pattern fehlt bzw. ist null/leer bei: {missing:?}"
    );
}

// spec: AI-120
// acceptance: independent
// `ua_pattern` erkennt den eigenen `ua_example` als Teilzeichenkette ohne Beachtung der Groß-/Kleinschreibung.
#[test]
fn ua_pattern_matches_own_example_case_insensitively() {
    let cat = Catalog::builtin();
    let mut checked = 0;
    for e in cat.entries() {
        let (Some(_), Some(example)) = (&e.ua_pattern, &e.ua_example) else {
            continue;
        };
        checked += 1;
        assert!(
            e.matches_user_agent(example),
            "{}: eigener ua_example",
            e.token
        );
        assert!(
            e.matches_user_agent(&example.to_uppercase()),
            "{}: GROSS",
            e.token
        );
        assert!(
            e.matches_user_agent(&example.to_lowercase()),
            "{}: klein",
            e.token
        );
        assert_eq!(
            cat.detect_user_agent(example).map(|d| d.token.as_str()),
            Some(e.token.as_str()),
            "Erkennung von {example:?}"
        );
    }
    assert!(
        checked >= 10,
        "nur {checked} Einträge mit Muster und Beispiel"
    );
    assert!(cat
        .detect_user_agent("Mozilla/5.0 (compatible; OpenSpider/1.0)")
        .is_none());
}

// spec: AI-120
// acceptance: independent
// Ein `ua_pattern` ist eine Teilzeichenkette (keine Regex, kein Glob): Sonderzeichen gelten wörtlich.
#[test]
fn ua_pattern_is_a_plain_substring() {
    let mut e = user_entry("ExampleBot", Category::Search);
    e.ua_pattern = Some("Example.Bot+".to_string());
    assert!(e.matches_user_agent("x example.bot+/1.0 y"));
    assert!(!e.matches_user_agent("ExampleXBotttt"));
    assert!(!e.matches_user_agent("ExampleBot"));
}

// ---------------------------------------------------------------------------------------------------
// Schema-Prüfung (AI-120, zweites Kriterium)

// spec: AI-120
// acceptance: independent
// Das in der Spec genannte Schema liegt unter `resources/ai/schemas/ai-crawlers.schema.json` und nennt Pflichtfelder
// und die gültigen Kategorien.
#[test]
fn json_schema_file_exists_at_the_spec_path() {
    let p = repo_root()
        .join("resources")
        .join("ai")
        .join("schemas")
        .join("ai-crawlers.schema.json");
    let text =
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("Schema {} fehlt: {e}", p.display()));
    let schema: Value = serde_json::from_str(&text).expect("Schema ist JSON");
    let flat = schema.to_string();
    for needle in [
        "training",
        "search",
        "user-fetch",
        "control-token",
        "verified_at",
        "docs_url",
        "docs_note",
        "replaced_by",
        "ua_pattern",
    ] {
        assert!(flat.contains(needle), "Schema nennt {needle} nicht");
    }
}

// spec: AI-120
// acceptance: independent
// Gültige Grenzfälle werden angenommen: docs_note neben einer URL, replaced_by auf vorhandenes Token, docs_url null mit
// docs_note, alle Kategorien und respects_robots-Werte, Kontroll-Token ohne ua_example.
#[test]
fn valid_edge_cases_are_accepted() {
    let cat = Catalog::parse(&fixture("valid-edge.json"))
        .unwrap_or_else(|e| panic!("gültige Grenzfälle abgelehnt: {e:?}"));
    assert_eq!(cat.len(), 7);
    assert_eq!(
        cat.get("OldBot").unwrap().replaced_by.as_deref(),
        Some("ExampleBot")
    );
    assert!(cat.get("NoDocsBot").unwrap().docs_url.is_none());
    assert!(Catalog::parse(&fixture("valid-base.json")).is_ok());
}

// spec: AI-120
// acceptance: independent
// Tokens sind eindeutig ohne Beachtung der Groß-/Kleinschreibung.
#[test]
fn duplicate_tokens_are_rejected_case_insensitively() {
    for f in [
        "invalid/duplicate-token-case.json",
        "invalid/duplicate-token-exact.json",
    ] {
        let errs = errors_of(f);
        assert!(
            errs.iter()
                .any(|e| matches!(e, CatalogError::DuplicateToken { .. })),
            "{f}: {errs:?}"
        );
    }
}

// spec: AI-120
// acceptance: independent
#[test]
fn invalid_or_missing_category_is_rejected() {
    let errs = errors_of("invalid/category-invalid.json");
    assert!(
        errs.iter()
            .any(|e| matches!(e, CatalogError::InvalidCategory { .. })),
        "{errs:?}"
    );
    let errs = errors_of("invalid/category-missing.json");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            CatalogError::MissingField {
                field: "category",
                ..
            }
        )),
        "{errs:?}"
    );
}

// spec: AI-120
// acceptance: independent
// docs_url muss eine https-URL sein.
#[test]
fn docs_url_without_https_is_rejected() {
    for f in [
        "invalid/docs-url-http.json",
        "invalid/docs-url-ftp.json",
        "invalid/docs-url-empty.json",
    ] {
        let errs = errors_of(f);
        assert!(
            errs.iter().any(|e| matches!(
                e,
                CatalogError::DocsUrlNotHttps { .. } | CatalogError::Empty { .. }
            )),
            "{f}: {errs:?}"
        );
    }
}

// spec: AI-120
// acceptance: independent
// docs_url null nur mit nicht leerem docs_note.
#[test]
fn null_docs_url_needs_a_non_empty_docs_note() {
    for f in [
        "invalid/docs-url-null-no-note.json",
        "invalid/docs-url-null-empty-note.json",
    ] {
        let errs = errors_of(f);
        assert!(
            errs.iter()
                .any(|e| matches!(e, CatalogError::DocsNoteMissing { .. })),
            "{f}: {errs:?}"
        );
    }
}

// spec: AI-120
// acceptance: independent
#[test]
fn replaced_by_must_name_an_existing_token() {
    let errs = errors_of("invalid/replaced-by-unknown.json");
    assert!(
        errs.iter()
            .any(|e| matches!(e, CatalogError::UnknownReplacement { .. })),
        "{errs:?}"
    );
}

// spec: AI-120
// acceptance: independent
// verified_at ist ein Datum (JJJJ-MM-TT, existierender Tag) und Pflicht.
#[test]
fn verified_at_must_be_a_date() {
    for f in [
        "invalid/verified-at-german.json",
        "invalid/verified-at-impossible.json",
        "invalid/verified-at-text.json",
    ] {
        let errs = errors_of(f);
        assert!(
            errs.iter()
                .any(|e| matches!(e, CatalogError::InvalidDate { .. })),
            "{f}: {errs:?}"
        );
    }
    let errs = errors_of("invalid/verified-at-missing.json");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            CatalogError::MissingField {
                field: "verified_at",
                ..
            }
        )),
        "{errs:?}"
    );
}

// spec: AI-120
// acceptance: independent
// Pflichtfelder token, operator, respects_robots; ungültiger respects_robots-Wert.
#[test]
fn required_fields_and_respects_robots_values_are_enforced() {
    for (f, field) in [
        ("invalid/token-missing.json", "token"),
        ("invalid/operator-missing.json", "operator"),
        ("invalid/respects-robots-missing.json", "respects_robots"),
    ] {
        let errs = errors_of(f);
        assert!(
            errs.iter()
                .any(|e| matches!(e, CatalogError::MissingField { field: g, .. } if *g == field)),
            "{f}: {errs:?}"
        );
    }
    let errs = errors_of("invalid/respects-robots-invalid.json");
    assert!(
        errs.iter()
            .any(|e| matches!(e, CatalogError::InvalidRespectsRobots { .. })),
        "{errs:?}"
    );
}

// spec: AI-120
// acceptance: independent
// Festlegung Orchestrator: `ua_pattern` ist Pflichtfeld. Ein fehlendes Feld wird abgelehnt.
#[test]
fn missing_ua_pattern_is_rejected() {
    let errs = errors_of("invalid/ua-pattern-missing.json");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            CatalogError::MissingField {
                field: "ua_pattern",
                ..
            }
        )),
        "{errs:?}"
    );
}

// spec: AI-120
// acceptance: independent
// Festlegung Orchestrator: `ua_pattern` ist Pflichtfeld (Teilzeichenkette); null oder leer erfüllt das nicht.
#[test]
fn null_or_empty_ua_pattern_is_rejected() {
    let accepted: Vec<&str> = [
        "invalid/ua-pattern-null.json",
        "invalid/ua-pattern-empty.json",
    ]
    .into_iter()
    .filter(|f| Catalog::parse(&fixture(f)).is_ok())
    .collect();
    assert!(
        accepted.is_empty(),
        "angenommen statt abgelehnt: {accepted:?}"
    );
}

// ---------------------------------------------------------------------------------------------------
// Benutzereinträge (AI-120: „Ein Benutzereintrag mit einem bereits vorhandenen Token wird abgelehnt.“)

fn user_entry(token: &str, category: Category) -> CrawlerEntry {
    CrawlerEntry {
        token: token.to_string(),
        operator: "Jemand".to_string(),
        category,
        ua_pattern: Some(token.to_string()),
        ua_example: Some(format!("Mozilla/5.0 (compatible; {token}/1.0)")),
        respects_robots: RespectsRobots::Unknown,
        docs_url: Some("https://example.org/bot".to_string()),
        docs_note: None,
        verified_at: "2026-11-01".to_string(),
        deprecated: false,
        replaced_by: None,
        renders_js: None,
    }
}

// spec: AI-120
// acceptance: independent
// Benutzereintrag mit vorhandenem Token (auch in anderer Schreibweise) wird abgelehnt und überschreibt nichts.
#[test]
fn user_entry_with_existing_token_is_rejected() {
    let builtin = Catalog::builtin();
    let original = builtin.get("GPTBot").unwrap().clone();
    for token in ["GPTBot", "gptbot", "GPTBOT"] {
        let mut cat = builtin.clone();
        let res = cat.add_user_entry(user_entry(token, Category::Search));
        assert!(
            matches!(res, Err(CatalogError::DuplicateToken { .. })),
            "{token}: {res:?}"
        );
        assert_eq!(cat.len(), builtin.len());
        assert_eq!(cat.get("GPTBot").unwrap(), &original);
    }
    // gleiche Regel über den Benutzerkatalog als Datei
    let res = builtin.with_user_catalog(&fixture("user-duplicate-gptbot.json"));
    let errs = res.expect_err("Benutzerkatalog mit gptbot muss abgelehnt werden");
    assert!(
        errs.iter()
            .any(|e| matches!(e, CatalogError::DuplicateToken { .. })),
        "{errs:?}"
    );
    assert_eq!(builtin.get("GPTBot").unwrap(), &original);
}

// spec: AI-120
// acceptance: independent
// Neuer Benutzereintrag ergänzt den Katalog; ein zweiter mit demselben Token wird abgelehnt.
#[test]
fn new_user_entry_is_added_once() {
    let mut cat = Catalog::builtin().clone();
    let n = cat.len();
    cat.add_user_entry(user_entry("AcmeBot", Category::Training))
        .expect("neues Token");
    assert_eq!(cat.len(), n + 1);
    assert!(cat.get("acmebot").is_some());
    assert!(matches!(
        cat.add_user_entry(user_entry("ACMEBOT", Category::Search)),
        Err(CatalogError::DuplicateToken { .. })
    ));
    assert_eq!(cat.len(), n + 1);
}

// ---------------------------------------------------------------------------------------------------
// Speisung der KI-Crawler-Voreinstellungen (CRAWL-114 [M4], AI-120 letztes Kriterium)

// spec: CRAWL-114
// acceptance: independent
// Menge der KI-Crawler-Voreinstellungen = {nicht veraltete search/user-fetch} ∪ {GPTBot, ClaudeBot, Google-Extended},
// in Katalog-Schreibweise; veraltete Einträge fehlen.
#[test]
fn ai_preset_tokens_equal_the_spec_set() {
    let expected = expected_preset_tokens_from_json(&shipped_json());
    let actual = Catalog::builtin().ai_preset_tokens();
    assert_eq!(actual, expected);
    // aus der Tabelle der Spec: diese 18 sind enthalten
    for t in [
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
    ] {
        assert!(actual.contains(t), "{t} fehlt");
    }
    for t in [
        "anthropic-ai",
        "Google-NotebookLM",
        "CCBot",
        "Bytespider",
        "Applebot-Extended",
        "Meta-ExternalAgent",
        "cohere-ai",
        "AI2Bot",
        "Diffbot",
    ] {
        assert!(!actual.contains(t), "{t} darf keine Voreinstellung sein");
    }
}

// spec: CRAWL-114
// acceptance: independent
// Ein im Katalog ergänzter search-Eintrag erscheint ohne Code-Änderung; ein veralteter search-Eintrag nicht.
#[test]
fn added_search_entry_appears_as_preset_token() {
    let mut doc = shipped_json();
    let extra: Value = serde_json::from_str(&fixture("extra-search-entry.json")).unwrap();
    let old: Value = serde_json::from_str(&fixture("extra-deprecated-search-entry.json")).unwrap();
    doc["crawlers"].as_array_mut().unwrap().push(extra);
    doc["crawlers"].as_array_mut().unwrap().push(old);
    let cat = Catalog::parse(&doc.to_string()).expect("erweiterter Katalog gültig");
    let tokens = cat.ai_preset_tokens();
    assert!(tokens.contains("NewSearchBot"));
    assert!(!tokens.contains("OldSearchBot"));
    assert_eq!(tokens, expected_preset_tokens_from_json(&doc));
}

// spec: CRAWL-114
// acceptance: independent
// Benutzereinträge der Kategorien search/user-fetch erscheinen ebenfalls, training nicht.
#[test]
fn user_catalog_search_entries_feed_presets() {
    let cat = Catalog::builtin()
        .with_user_catalog(&fixture("user-new-search.json"))
        .expect("Benutzerkatalog gültig");
    assert!(cat.ai_preset_tokens().contains("AcmeSearchBot"));
    let cat = Catalog::builtin()
        .with_user_catalog(&fixture("user-new-training.json"))
        .expect("Benutzerkatalog gültig");
    assert!(!cat.ai_preset_tokens().contains("AcmeTrainBot"));
}

// spec: AI-121
// acceptance: independent
// Standardauswahl der zu prüfenden Tokens: alle nicht veralteten.
#[test]
fn default_selection_is_all_non_deprecated_entries() {
    let cat = Catalog::builtin();
    let sel: BTreeSet<&str> = cat
        .default_selection()
        .into_iter()
        .map(|e| e.token.as_str())
        .collect();
    let want: BTreeSet<&str> = cat
        .entries()
        .filter(|e| !e.deprecated)
        .map(|e| e.token.as_str())
        .collect();
    assert_eq!(sel, want);
    assert!(!sel.contains("anthropic-ai"));
    assert!(!sel.contains("Google-NotebookLM"));
}

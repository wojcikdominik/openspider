//! AI-Crawler-Katalog (AI-120) gegen die Fixtures `tests/fixtures/ai/catalog-minimum.json` (maschinenlesbare Kopie
//! der Mindestliste), `tests/fixtures/ai/catalog-invalid/` (Negativfälle), `tests/fixtures/ai/search/`
//! (`catalog-valid-edge.json`, `catalog-stale.json`, `catalog-extra.json`) und das Schema
//! `resources/ai/schemas/ai-crawlers.schema.json`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use os_analyze::ai_search::catalog::{
    Catalog, CatalogError, Category, CrawlerEntry, EntrySource, RespectsRobots, CATALOG_COLUMNS,
    CATALOG_JSON, OPTIONAL_FIELDS, REQUIRED_FIELDS,
};
use serde_json::Value;

fn repo() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("..");
    p
}

/// Pfad relativ zu `tests/fixtures/ai/`.
fn fixture(rel: &str) -> PathBuf {
    repo().join("tests").join("fixtures").join("ai").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(fixture(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn json(rel: &str) -> Value {
    serde_json::from_str(&read(rel)).unwrap()
}

// spec: AI-120
// Inhaltstest: alle 27 Tokens der Mindestliste mit Kategorie, deprecated/replaced_by, docs_url, docs_note und
// den festgelegten respects_robots-Werten; zusätzliche Einträge sind erlaubt.
#[test]
fn shipped_catalog_contains_the_minimum_list() {
    let catalog = Catalog::builtin();
    let minimum = json("catalog-minimum.json");
    let rows = minimum["entries"].as_array().unwrap();
    assert_eq!(rows.len(), 27, "Mindestliste hat 27 Tokens");
    for row in rows {
        let token = row["token"].as_str().unwrap();
        let e = catalog
            .entries()
            .find(|e| e.token == token)
            .unwrap_or_else(|| panic!("{token} fehlt (Schreibweise identisch)"));
        assert_eq!(e.operator, row["operator"].as_str().unwrap(), "{token}");
        assert_eq!(
            e.category.as_str(),
            row["category"].as_str().unwrap(),
            "{token}"
        );
        assert_eq!(
            e.deprecated,
            row["deprecated"].as_bool().unwrap(),
            "{token}"
        );
        assert_eq!(
            e.replaced_by.as_deref(),
            row["replaced_by"].as_str(),
            "{token}"
        );
        assert_eq!(e.docs_url.as_deref(), row["docs_url"].as_str(), "{token}");
        if row["docs_note_required"].as_bool().unwrap() {
            assert!(
                e.docs_note.as_deref().is_some_and(|n| !n.trim().is_empty()),
                "{token}: docs_note fehlt"
            );
        }
        if let Some(rr) = row.get("respects_robots").and_then(Value::as_str) {
            assert_eq!(e.respects_robots.as_str(), rr, "{token}");
        }
    }
    // Ausdrücklich festgelegte Werte (AI-120, Offene Punkte 4).
    let rr = |t: &str| catalog.get(t).unwrap().respects_robots;
    assert_eq!(rr("Google-CloudVertexBot"), RespectsRobots::Yes);
    for t in ["Google-GeminiNotebook", "Google-NotebookLM", "Google-Agent"] {
        assert_eq!(rr(t), RespectsRobots::No, "{t}");
    }
    for t in ["anthropic-ai", "Bytespider", "cohere-ai", "Diffbot"] {
        assert!(catalog.get(t).unwrap().docs_note.is_some(), "{t}");
    }
    assert_eq!(
        catalog.get("anthropic-ai").unwrap().docs_note.as_deref(),
        Some("Von Anthropic nicht mehr dokumentiert, ersetzt durch ClaudeBot")
    );
    assert!(
        catalog.get("Gemini-Deep-Research").is_none(),
        "nicht aufgenommen"
    );
    assert!(catalog.len() >= 27);
    assert!(catalog
        .entries_with_source()
        .all(|(_, s)| s == EntrySource::Builtin));
}

// spec: AI-120
// Jeder Eintrag trägt ein Prüfdatum; ein Eintrag mit Beispiel-User-Agent wird über sein Muster wiedererkannt.
#[test]
fn every_entry_has_a_check_date_and_patterns_match_their_examples() {
    for e in Catalog::builtin().entries() {
        assert_eq!(e.verified_at.len(), 10, "{}", e.token);
        if let (Some(_), Some(ua)) = (&e.ua_pattern, &e.ua_example) {
            assert!(
                e.matches_user_agent(ua),
                "{}: Muster passt nicht auf das Beispiel",
                e.token
            );
            assert_eq!(
                Catalog::builtin()
                    .detect_user_agent(ua)
                    .map(|d| d.token.as_str()),
                Some(e.token.as_str())
            );
        }
    }
    assert!(Catalog::builtin()
        .detect_user_agent("Mozilla/5.0 Firefox/128.0")
        .is_none());
}

// spec: AI-120
// CI-Test: die Datei erfüllt das Schema (Pflichtfelder, eindeutige Tokens case-insensitiv, gültige Kategorien,
// verified_at als Datum, docs_url https oder null mit docs_note, replaced_by verweist auf ein Token).
#[test]
fn shipped_catalog_passes_validation_and_matches_the_schema() {
    let catalog = Catalog::parse(CATALOG_JSON).expect("gültig");
    let lower: BTreeSet<String> = catalog
        .entries()
        .map(|e| e.token.to_ascii_lowercase())
        .collect();
    assert_eq!(
        lower.len(),
        catalog.len(),
        "Tokens eindeutig (ohne Groß-/Kleinschreibung)"
    );

    // Das JSON-Schema beschreibt dieselben Regeln wie die Prüfung im Code.
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            repo()
                .join("resources")
                .join("ai")
                .join("schemas")
                .join("ai-crawlers.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let crawler = &schema["$defs"]["crawler"];
    let required: Vec<&str> = crawler["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(required, REQUIRED_FIELDS);
    let props: BTreeSet<&str> = crawler["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let expected: BTreeSet<&str> = REQUIRED_FIELDS
        .iter()
        .chain(OPTIONAL_FIELDS.iter())
        .copied()
        .collect();
    assert_eq!(props, expected);
    let enum_of = |field: &str| -> Vec<String> {
        crawler["properties"][field]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        enum_of("category"),
        Category::ALL
            .iter()
            .map(|c| c.as_str().to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        enum_of("respects_robots"),
        RespectsRobots::ALL
            .iter()
            .map(|r| r.as_str().to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(crawler["additionalProperties"], Value::Bool(false));
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        Value::from(1)
    );

    // Jeder Eintrag der Datei hat alle Pflichtfelder (auch die mit null).
    let raw: Value = serde_json::from_str(CATALOG_JSON).unwrap();
    for item in raw["crawlers"].as_array().unwrap() {
        for field in REQUIRED_FIELDS {
            assert!(item.get(field).is_some(), "{}: {field}", item["token"]);
        }
    }
}

// spec: AI-120
// Negativfälle aus `catalog-invalid/`: jede Datei wird mit genau dem erwarteten Fehler abgelehnt.
#[test]
fn invalid_catalogs_are_rejected_with_the_expected_error() {
    type Check = fn(&CatalogError) -> bool;
    let cases: [(&str, Check); 12] = [
        ("missing-field", |e| {
            matches!(
                e,
                CatalogError::MissingField {
                    field: "verified_at",
                    ..
                }
            )
        }),
        (
            "duplicate-token",
            |e| matches!(e, CatalogError::DuplicateToken { index: 1, token } if token == "abot"),
        ),
        (
            "invalid-category",
            |e| matches!(e, CatalogError::InvalidCategory { value, .. } if value == "crawler"),
        ),
        ("invalid-date", |e| {
            matches!(e, CatalogError::InvalidDate { .. })
        }),
        ("docs-url-http", |e| {
            matches!(e, CatalogError::DocsUrlNotHttps { .. })
        }),
        ("docs-null-without-note", |e| {
            matches!(e, CatalogError::DocsNoteMissing { .. })
        }),
        ("docs-null-empty-note", |e| {
            matches!(e, CatalogError::DocsNoteMissing { .. })
        }),
        (
            "replaced-by-unknown",
            |e| matches!(e, CatalogError::UnknownReplacement { value, .. } if value == "BBot"),
        ),
        (
            "unknown-field",
            |e| matches!(e, CatalogError::UnknownField { field, .. } if field == "extra"),
        ),
        ("invalid-respects-robots", |e| {
            matches!(e, CatalogError::InvalidRespectsRobots { .. })
        }),
        (
            "wrong-type",
            |e| matches!(e, CatalogError::WrongType { field, .. } if field == "deprecated"),
        ),
        ("schema-version", |e| {
            matches!(e, CatalogError::SchemaVersion(_))
        }),
    ];
    let mut files: Vec<String> = std::fs::read_dir(fixture("catalog-invalid"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(
        files.len(),
        cases.len(),
        "jede Negativ-Fixture hat einen Fall: {files:?}"
    );
    for (name, check) in cases {
        let text = read(&format!("catalog-invalid/{name}.json"));
        let errors = Catalog::parse(&text).expect_err(name);
        assert_eq!(errors.len(), 1, "{name}: {errors:?}");
        assert!(check(&errors[0]), "{name}: {errors:?}");
        assert!(!errors[0].to_string().is_empty());
    }
    assert!(matches!(
        Catalog::parse("{").unwrap_err()[0],
        CatalogError::Json(_)
    ));
    assert!(matches!(
        Catalog::parse("[]").unwrap_err()[0],
        CatalogError::Root(_)
    ));
}

// spec: AI-120
// Zulässig: docs_note neben einer URL, replaced_by auf ein vorhandenes Token (andere Schreibweise).
#[test]
fn docs_note_next_to_url_and_replacement_are_valid() {
    let c = Catalog::parse(&read("search/catalog-valid-edge.json")).expect("gültig");
    assert_eq!(c.len(), 2);
    assert!(c.get("abot").unwrap().docs_url.is_some());
    assert!(c.get("ABOT").unwrap().docs_note.is_some());
    assert_eq!(
        c.get("OldBot").unwrap().replaced_by.as_deref(),
        Some("abot")
    );
}

fn user_entry(token: &str, category: Category) -> CrawlerEntry {
    CrawlerEntry {
        token: token.into(),
        operator: "Eigener Eintrag".into(),
        category,
        ua_pattern: None,
        ua_example: Some(format!("{token}/1.0")),
        respects_robots: RespectsRobots::Unknown,
        docs_url: None,
        docs_note: Some("vom Benutzer ergänzt".into()),
        verified_at: "2026-11-24".into(),
        deprecated: false,
        replaced_by: None,
        renders_js: None,
    }
}

// spec: AI-120
// Benutzerkatalog: ergänzt, überschreibt nichts; ein bereits vorhandenes Token wird abgelehnt.
#[test]
fn user_entry_with_an_existing_token_is_rejected() {
    let mut c = Catalog::builtin().clone();
    let before = c.clone();
    for token in ["GPTBot", "gptbot", "  OAI-SearchBot "] {
        let err = c
            .add_user_entry(user_entry(token.trim(), Category::Search))
            .expect_err(token);
        assert!(
            matches!(err, CatalogError::DuplicateToken { .. }),
            "{token}: {err:?}"
        );
    }
    assert_eq!(c, before, "abgelehnte Einträge ändern nichts");

    c.add_user_entry(user_entry("Example-SearchBot", Category::Search))
        .expect("neuer Eintrag");
    assert_eq!(c.len(), before.len() + 1);
    let (_, source) = c
        .entries_with_source()
        .find(|(e, _)| e.token == "Example-SearchBot")
        .unwrap();
    assert_eq!(source, EntrySource::User);
    assert_eq!(
        c.get("GPTBot"),
        before.get("GPTBot"),
        "mitgelieferter Eintrag unverändert"
    );
    assert!(c.ai_preset_tokens().contains("Example-SearchBot"));
    // Ein zweites Mal wird abgelehnt, auch in anderer Schreibweise.
    assert!(c
        .add_user_entry(user_entry("example-searchbot", Category::Search))
        .is_err());
    // Ungültige Einträge (z. B. http-URL) werden abgelehnt.
    let mut bad = user_entry("Other-Bot", Category::Training);
    bad.docs_url = Some("http://example.test".into());
    assert!(matches!(
        c.add_user_entry(bad),
        Err(CatalogError::DocsUrlNotHttps { .. })
    ));

    // Speichern und Laden des Benutzerkatalogs; eine Datei mit vorhandenem Token wird abgelehnt.
    let saved = c.user_catalog_json();
    let reloaded = Catalog::builtin()
        .with_user_catalog(&saved)
        .expect("Benutzerkatalog");
    assert_eq!(reloaded, c);
    let extra = read("search/catalog-extra.json");
    let with_extra = Catalog::builtin()
        .with_user_catalog(&extra)
        .expect("ergänzt");
    assert_eq!(with_extra.user_entries().len(), 4);
    let errors = with_extra.with_user_catalog(&extra).expect_err("doppelt");
    assert_eq!(errors.len(), 4);
    assert!(errors
        .iter()
        .all(|e| matches!(e, CatalogError::DuplicateToken { .. })));
}

// spec: AI-120
// Release-Check: verified_at älter als 180 Tage erzeugt eine Warnung (nur CI). Mit dem Prüfdatum des
// mitgelieferten Katalogs ist er heute warnungsfrei.
#[test]
fn release_check_warns_about_entries_older_than_180_days() {
    let c = Catalog::parse(&read("search/catalog-stale.json")).unwrap();
    let warnings = c.release_check_warnings("2026-11-24");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("OldBot"), "{warnings:?}");
    assert!(!warnings[0].contains("FreshBot"));
    assert_eq!(c.release_check_warnings("2026-11-23").len(), 0);
    assert_eq!(c.release_check_warnings("kein Datum").len(), 1);
    assert!(Catalog::builtin()
        .release_check_warnings("2026-11-24")
        .is_empty());
    let oldest = Catalog::builtin()
        .entries()
        .map(|e| e.verified_at.as_str())
        .min()
        .unwrap();
    assert!(oldest >= "2026-11-24", "Prüfdatum {oldest}");
}

// spec: AI-120
// Eingabe des nächtlichen Erreichbarkeitslaufs: alle docs_url ohne Doppelte, nur https.
#[test]
fn docs_urls_are_listed_once_for_the_nightly_check() {
    let urls = Catalog::builtin().docs_urls();
    let unique: BTreeSet<&str> = urls.iter().copied().collect();
    assert_eq!(unique.len(), urls.len());
    assert!(urls.iter().all(|u| u.starts_with("https://")));
    assert!(urls.contains(&"https://platform.openai.com/docs/bots"));
    assert_eq!(
        urls.iter()
            .filter(|u| u.contains("google-common-crawlers"))
            .count(),
        1
    );
}

// spec: AI-120
// Katalogtabelle `Token`, `Operator`, `Category`, `Respects robots.txt`, `Verified At`, `Docs`.
#[test]
fn catalog_table_has_the_spec_columns() {
    assert_eq!(
        CATALOG_COLUMNS,
        [
            "Token",
            "Operator",
            "Category",
            "Respects robots.txt",
            "Verified At",
            "Docs"
        ]
    );
    let rows = Catalog::builtin().table_rows();
    assert_eq!(rows.len(), Catalog::builtin().len());
    let bytespider = rows.iter().find(|r| r.token == "Bytespider").unwrap();
    assert_eq!(bytespider.docs, "Keine Betreiberseite bekannt");
    let gpt = rows.iter().find(|r| r.token == "GPTBot").unwrap();
    assert_eq!(gpt.docs, "https://platform.openai.com/docs/bots");
    assert_eq!(gpt.category, Category::Training);
}

// spec: AI-120
// spec: CRAWL-114 [M4]
// Konsistenz mit CRAWL-114 (Katalogseite): Menge der Voreinstellungs-Tokens = {nicht veraltete search- und
// user-fetch-Einträge} ∪ {GPTBot, ClaudeBot, Google-Extended}. Den Vergleich mit der Voreinstellungsliste von
// os-fetch macht `crates/os-fetch/tests/ai_user_agents.rs`.
#[test]
fn ai_preset_tokens_follow_the_rule() {
    let c = Catalog::builtin();
    let expected: BTreeSet<String> = [
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
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(c.ai_preset_tokens(), expected);
    let by_rule: BTreeSet<String> = c
        .entries()
        .filter(|e| {
            (!e.deprecated && matches!(e.category, Category::Search | Category::UserFetch))
                || ["GPTBot", "ClaudeBot", "Google-Extended"].contains(&e.token.as_str())
        })
        .map(|e| e.token.clone())
        .collect();
    assert_eq!(c.ai_preset_tokens(), by_rule);
}

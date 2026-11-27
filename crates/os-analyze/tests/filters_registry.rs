//! `tabs::all()` stimmt mit `docs/plan/M1-filters.toml` und `docs/plan/M2-filters.toml` überein (Tabs, Filter-Kennungen,
//! Reihenfolge, Art, Voraussetzungen). Der Test bleibt dauerhaft aktiv: Kennungen ändern sich nur mit den Registerdateien.
//! Infrastrukturtest ohne Spec-Tag.
//!
//! Reihenfolge der Tabs: UI-004. Reihenfolge der Filter: erst die Filter von M1, dann die von M2 in der Reihenfolge der
//! Datei (die Spec legt keine andere Anzeigereihenfolge fest). Dynamische Filter (`<Filtername> …`) stehen nicht im
//! Register von `tabs::all()`; sie entstehen zur Laufzeit.

use std::collections::BTreeSet;
use std::path::Path;

use os_analyze::defs::{apply_meta_defaults, intern, Align, SearchOps};
use os_analyze::tabs::{self, M2_FILTERS};
use os_analyze::{ColumnDef, ColumnKind, FilterKind, Requirement};
use os_model::{AnalysisStep, CrawlConfig};

/// UI-004 (ohne AMP, AI und die Tabs von M4/M5): Reihenfolge der Tabs im Produkt.
const UI_ORDER: [&str; 35] = [
    "Internal",
    "External",
    "Security",
    "Response Codes",
    "URL",
    "Page Titles",
    "Meta Description",
    "Meta Keywords",
    "H1",
    "H2",
    "Content",
    "Images",
    "Canonicals",
    "Pagination",
    "Directives",
    "Hreflang",
    "JavaScript",
    "Links",
    "Structured Data",
    "Sitemaps",
    "PageSpeed",
    "Mobile",
    "Accessibility",
    "Custom Search",
    "Custom Extraction",
    "Custom JavaScript",
    "Analytics",
    "Search Console",
    "Validation",
    "Link Metrics",
    "AI",
    "AI Decisions",
    "AI Suggestions",
    "AI Search",
    "Change Detection",
];

fn read(name: &str) -> toml::Table {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/plan")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    text.parse().unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn kind_name(kind: FilterKind) -> String {
    match kind {
        FilterKind::Live => "live".to_string(),
        FilterKind::PostCrawl(step) => format!("post_crawl:{step:?}"),
    }
}

/// Erwartete Filter eines Tabs: M1-Filter, dann M2-Filter ohne dynamische Platzhalter.
/// `(id, kind, requires)`; `requires` in der Schreibweise der Dateien.
fn expected_filters(tab_id: &str) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for file in ["M1-filters.toml", "M2-filters.toml", "M4-filters.toml"] {
        let reg = read(file);
        for tab in reg["tab"].as_array().unwrap() {
            if tab["id"].as_str() != Some(tab_id) {
                continue;
            }
            for f in tab["filters"].as_array().unwrap() {
                let id = f["id"].as_str().unwrap();
                // D10: diese zwei Kennungen beginnen mit einem echten HTML-Tag,
                // nicht mit einem dynamischen Filter-Platzhalter.
                if id.starts_with('<')
                    && !(tab_id == "Validation"
                        && matches!(
                            id,
                            "<head> Not First In <html> Element"
                                | "<body> Element Preceding <html>"
                        ))
                {
                    continue;
                }
                let requires = f
                    .get("requires")
                    .and_then(|r| r.as_array())
                    .map(|r| r.iter().map(|k| k.as_str().unwrap().to_string()).collect())
                    .unwrap_or_default();
                out.push((
                    id.to_string(),
                    f["kind"].as_str().unwrap().to_string(),
                    requires,
                ));
            }
        }
    }
    // spec: TECH-251 / TECH-252: expanded expectation for the dynamic axe placeholder.
    if tab_id == "Accessibility" {
        out.extend(
            include_str!("../../../tests/fixtures/ui/fx-ui-a11y/rule-ids.txt")
                .lines()
                .map(|id| {
                    (
                        id.to_owned(),
                        "live".into(),
                        vec![
                            "extraction.accessibility".into(),
                            "rendering.mode=javascript".into(),
                        ],
                    )
                }),
        );
    }
    out
}

/// Wie `Requirement`s aus den Schlüsseln der Registerdatei entstehen: `rendering.mode=javascript` → Rendering,
/// Schlüssel ohne `=` → Konfiguration; andere Bedingungen (`ui.parity_filters=false`) sind Profilschalter der Oberfläche,
/// kein Konfigurationsschlüssel des Crawls, und gehören dem Paket, das den Filter umsetzt.
fn expected_requirements(requires: &[String]) -> (Vec<String>, bool) {
    let config: Vec<String> = requires
        .iter()
        .filter(|r| !r.contains('='))
        .cloned()
        .collect();
    let rendering = requires.iter().any(|r| r == "rendering.mode=javascript");
    (config, rendering)
}

#[test]
fn tab_ids_follow_the_ui_order_and_cover_both_registry_files() {
    let ids: Vec<&str> = tabs::all().iter().map(|t| t.id).collect();
    assert_eq!(ids, UI_ORDER, "UI-004");

    let mut registry_tabs: BTreeSet<String> = BTreeSet::new();
    for file in ["M1-filters.toml", "M2-filters.toml", "M4-filters.toml"] {
        for tab in read(file)["tab"].as_array().unwrap() {
            registry_tabs.insert(tab["id"].as_str().unwrap().to_string());
        }
    }
    let ours: BTreeSet<String> = UI_ORDER.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(registry_tabs, ours, "Tabs der Registerdateien");
    assert_eq!(
        read("M1-filters.toml")["tab"].as_array().unwrap().len(),
        16,
        "M1 hat 16 Tabs"
    );
    assert_eq!(
        read("M2-filters.toml")["tab"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["new"].as_bool() == Some(true))
            .count(),
        15,
        "M2 ergänzt 15 neue Tabs"
    );
}

#[test]
fn filters_match_the_registry_files_in_id_order_kind_and_requirements() {
    for tab in tabs::all() {
        let expected = expected_filters(tab.id);
        let ids: Vec<&str> = tab.filters.iter().map(|f| f.id).collect();
        let exp_ids: Vec<&str> = expected.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(
            ids, exp_ids,
            "Filter-Kennungen und -Reihenfolge von {}",
            tab.id
        );
        for (exp, f) in expected.iter().zip(&tab.filters) {
            assert_eq!(kind_name(f.kind), exp.1, "Art von {}:{}", tab.id, f.id);
            let (config, rendering) = expected_requirements(&exp.2);
            let actual_config: Vec<String> = f
                .requires
                .iter()
                .filter_map(|r| match r {
                    Requirement::Config(key)
                    | Requirement::RenderingOrConfig(key)
                    | Requirement::Coverage { config: key, .. } => Some((*key).to_string()),
                    Requirement::AnyConfig(_) => Some("extraction.structured_data".into()),
                    Requirement::Rendering => None,
                })
                .collect();
            let actual_rendering = f.requires.contains(&Requirement::Rendering);
            assert_eq!(
                actual_config,
                // TECH-114 / K2-63: disabled collection must expose NeedsConfig.
                if tab.id == "JavaScript" && f.id == "Pages With JavaScript Errors" {
                    vec!["rendering.js_error_reporting".to_string()]
                } else {
                    config
                },
                "Konfigurationsschlüssel von {}:{}",
                tab.id,
                f.id
            );
            assert_eq!(
                actual_rendering, rendering,
                "Rendering-Voraussetzung von {}:{}",
                tab.id, f.id
            );
        }
    }
}

#[test]
fn the_stub_table_is_the_m2_registry_file() {
    let reg = read("M2-filters.toml");
    let mut expected: Vec<(String, String, String, Vec<String>, bool)> = Vec::new();
    for tab in reg["tab"].as_array().unwrap() {
        for f in tab["filters"].as_array().unwrap() {
            let id = f["id"].as_str().unwrap();
            if id.starts_with('<') {
                continue;
            }
            let requires: Vec<String> = f
                .get("requires")
                .and_then(|r| r.as_array())
                .map(|r| r.iter().map(|k| k.as_str().unwrap().to_string()).collect())
                .unwrap_or_default();
            let (config, rendering) = expected_requirements(&requires);
            expected.push((
                tab["id"].as_str().unwrap().to_string(),
                id.to_string(),
                f["kind"].as_str().unwrap().to_string(),
                config,
                rendering,
            ));
        }
    }
    let actual: Vec<(String, String, String, Vec<String>, bool)> = M2_FILTERS
        .iter()
        .map(|(tab, id, step, config, rendering)| {
            (
                (*tab).to_string(),
                (*id).to_string(),
                step.map_or("live".to_string(), |s| format!("post_crawl:{s:?}")),
                config.iter().map(|c| (*c).to_string()).collect(),
                *rendering,
            )
        })
        .collect();
    assert_eq!(
        actual, expected,
        "tabs::M2_FILTERS weicht von M2-filters.toml ab"
    );
    assert_eq!(
        M2_FILTERS.len(),
        199,
        "199 statische Filter von M2 (205 Einträge abzüglich 6 dynamischer Platzhalter; +2 Change Detection, TECH-309/310)"
    );
}

#[test]
fn every_tab_is_a_real_definition_or_a_marked_stub() {
    let mut stubs = 0;
    for tab in tabs::all() {
        assert_eq!(
            tab.columns.first().map(|c| c.key),
            Some("Address"),
            "Address ist die erste Spalte von {}",
            tab.id
        );
        assert!(!tab.population.is_empty(), "Grundmenge von {}", tab.id);
        let all: Vec<_> = tab
            .filters
            .iter()
            .filter(|f| f.id.eq_ignore_ascii_case("All") || f.id == "All Issues")
            .collect();
        assert!(all.len() <= 1, "höchstens ein Filter All in {}", tab.id);
        if tab.is_stub() {
            // Stub: leerer Platzhalter, bis das Paket den Tab umsetzt.
            stubs += 1;
            assert_eq!(tab.population, "0", "{}: Grundmenge des Stubs", tab.id);
            assert_eq!(tab.columns.len(), 1, "{}: nur die Spalte Address", tab.id);
            assert!(
                tab.filters
                    .iter()
                    .any(|f| f.id == "All" && f.predicate == "1"),
                "{}: Filter All",
                tab.id
            );
        } else {
            assert!(
                tab.columns.len() > 1,
                "{} hat nur eine Spalte (Platzhalter)",
                tab.id
            );
            assert_ne!(
                tab.population, "1",
                "{} hat noch die Grundmenge des Platzhalters",
                tab.id
            );
            let all: Vec<_> = tab.filters.iter().filter(|f| f.id == "All").collect();
            assert_eq!(all.len(), 1, "genau ein Filter All in {}", tab.id);
            assert!(
                !all[0].predicate.is_empty(),
                "Filter All von {} hat ein Prädikat",
                tab.id
            );
        }
    }
    // Alle 15 neuen Tabs sind in M2-F02 Stubs; die 16 Tabs von M1 sind echte Definitionen.
    // M2-D01 implements Meta Keywords; M2-D05 Pagination, M2-D14 Custom Search, M2-I03 Search Console, M2-I04 PageSpeed, M2-I02 Analytics.
    // M2-I05 ersetzt den Link-Metrics-Stub (K2-60, INTEG-151). M4-V01 setzt AI Suggestions um.
    assert_eq!(
        stubs, 3,
        "Stub-Tabs (Structured Data: M2-D09, Validation: M2-D10, Custom Extraction: M2-D15)"
    );
}

#[test]
fn stub_filters_are_marked_by_predicate_zero_and_never_all() {
    let mut stub_filters = 0;
    let mut real_new = 0;
    for tab in tabs::all() {
        for f in &tab.filters {
            if f.is_stub() {
                stub_filters += 1;
                assert_eq!(f.predicate, "0");
                assert!(!f.id.eq_ignore_ascii_case("All"));
            } else if !f.id.eq_ignore_ascii_case("All")
                && M2_FILTERS
                    .iter()
                    .any(|(t, id, ..)| *t == tab.id && id.eq_ignore_ascii_case(f.id))
            {
                real_new += 1;
            }
        }
    }
    // Bis ein Fachpaket einen Filter umsetzt, ist jeder M2-Filter außer `All` ein Stub.
    let m2_without_all = M2_FILTERS
        .iter()
        .filter(|(_, id, ..)| !id.eq_ignore_ascii_case("All"))
        .count();
    // M4-V01: die neun Filter von AI Suggestions außer `All` sind umgesetzt (27 − 9 = 18 M4-Stubs).
    assert_eq!(stub_filters + real_new, m2_without_all + 18);
    assert!(stub_filters > 0);
}

#[test]
fn filter_ids_are_unique_per_tab_ignoring_case() {
    for tab in tabs::all() {
        let mut ids: Vec<String> = tab.filters.iter().map(|f| f.id.to_lowercase()).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "doppelte Filter-Kennung in {}", tab.id);
    }
}

#[test]
fn requirement_keys_and_placeholders_resolve_in_the_default_configuration() {
    let json = serde_json::to_value(CrawlConfig::default()).unwrap();
    for tab in tabs::all() {
        for f in &tab.filters {
            for r in &f.requires {
                if let Requirement::Config(key) = r {
                    assert!(
                        matches!(
                            os_analyze::names::config_value(&json, key),
                            Some(serde_json::Value::Bool(_))
                        ),
                        "{}:{}: Konfigurationsschlüssel {key} ist kein Wahrheitswert der Konfiguration",
                        tab.id,
                        f.id
                    );
                }
            }
            let label = os_analyze::names::label(f.id, &CrawlConfig::default());
            assert!(
                !label.contains('{'),
                "{}:{}: Platzhalter nicht auflösbar ({label})",
                tab.id,
                f.id
            );
        }
    }
}

#[test]
fn every_column_has_ui_metadata_derived_from_its_kind() {
    for tab in tabs::all() {
        for c in &tab.columns {
            assert!(c.default_width > 0, "{}.{}: Breite", tab.id, c.key);
            assert!(!c.search_ops.is_none(), "{}.{}: Operatoren", tab.id, c.key);
            let numeric = matches!(
                c.kind,
                ColumnKind::Int | ColumnKind::Float { .. } | ColumnKind::Seconds3
            );
            if numeric {
                assert_eq!(
                    c.align,
                    Align::Right,
                    "{}.{}: Zahlen rechtsbündig",
                    tab.id,
                    c.key
                );
                assert!(c.search_ops.has(SearchOps::COMPARE), "{}.{}", tab.id, c.key);
            }
            if matches!(c.kind, ColumnKind::Bool) {
                assert!(c.search_ops.has(SearchOps::BOOL));
            }
            if matches!(c.kind, ColumnKind::Text | ColumnKind::Url) {
                assert!(c.search_ops.has(SearchOps::CONTAINS | SearchOps::REGEX));
            }
            assert!(c.sortable, "{}.{}: sortierbar", tab.id, c.key);
        }
    }
    // Eigene Werte bleiben erhalten; nur Spalten ohne Breite werden ergänzt.
    let mut cols = vec![
        ColumnDef {
            default_width: 500,
            align: Align::Center,
            ..ColumnDef::new("Eigene", "u.address", ColumnKind::Int, true)
        },
        ColumnDef::new("Zahl", "u.depth", ColumnKind::Int, true),
    ];
    apply_meta_defaults(&mut cols);
    assert_eq!((cols[0].default_width, cols[0].align), (500, Align::Center));
    assert_eq!((cols[1].default_width, cols[1].align), (96, Align::Right));
}

#[test]
fn dynamic_column_sources_are_registered_per_tab_and_names_are_interned() {
    // M2-D14 registers Custom Search; unconfigured sources still return no columns.
    let cfg = CrawlConfig::default();
    for tab in [
        "Custom Search",
        "Custom Extraction",
        "Custom JavaScript",
        "Link Metrics",
    ] {
        assert!(tabs::dynamic_columns(tab, &cfg).is_empty(), "{tab}");
    }
    // M2-E01 ersetzt ausschließlich den bisherigen Internal-Crawl-Spaltenstub (K2-60).
    let columns = tabs::dynamic_columns("Internal", &cfg);
    assert_eq!(
        columns.iter().map(|c| c.key).collect::<Vec<_>>(),
        [
            "Link Score",
            "Links Truncated",
            "Truncated",
            "Unique JS Inlinks",
            "Unique JS Outlinks",
            "Unique External JS Outlinks"
        ]
    );
    // M2-D08 (Link Score), M2-D14 (Custom Search), M2-D15 (Custom Extraction und eine Internal-Quelle), M2-E01 (Internal), M2-D09 (Structured Data).
    let mut sources = tabs::column_sources()
        .iter()
        .map(|(tab, _)| *tab)
        .collect::<Vec<_>>();
    sources.sort_unstable();
    assert_eq!(
        sources,
        [
            "Analytics",
            "Custom Extraction",
            "Custom JavaScript",
            "Custom Search",
            "Internal",
            "Internal",
            "Internal",
            "Internal",
            "Internal",
            "Internal",
            "Internal",
            "Link Metrics",
            "Mobile",
            "PageSpeed",
            "Structured Data"
        ]
    );
    // Namen zur Laufzeit: gleicher Text, dieselbe Referenz.
    let a = intern("Preis (Extraktor 1)");
    let b = intern(&String::from("Preis (Extraktor 1)"));
    assert!(std::ptr::eq(a, b));
    let col = ColumnDef::dynamic("Preis (Extraktor 1)", "u.address", ColumnKind::Text, true);
    assert_eq!(col.key, "Preis (Extraktor 1)");
}

#[test]
fn analysis_steps_named_by_post_crawl_filters_exist() {
    let steps: BTreeSet<String> = AnalysisStep::ALL.iter().map(|s| format!("{s:?}")).collect();
    for tab in tabs::all() {
        for f in &tab.filters {
            if let FilterKind::PostCrawl(step) = f.kind {
                assert!(steps.contains(&format!("{step:?}")), "{}:{}", tab.id, f.id);
            }
        }
    }
}

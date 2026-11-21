//! Aufteilung des Issue-Katalogs je Tab (M2-F02): `core/issues/catalog.d/<tab>.toml` und `texts/{de,en}.d/<tab>.toml`
//! neben den Dateien von M1; der Loader liest alle. Dazu der Abgleich Katalog ↔ Register über `issue = true` in den
//! Filterdateien. Infrastrukturtest ohne Spec-Tag (die Fachbedeutung der Einträge prüfen UI-063/UI-064 in
//! `issues_catalog.rs` und `issues_texts.rs`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use os_analyze::issues::{self, catalog, parse_catalog, validate, Lang, SPLIT_FILES};
use os_analyze::tabs;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Dateinamen (ohne Endung) in einem Ordner von `core/issues`.
fn stems(dir: &str) -> BTreeSet<String> {
    std::fs::read_dir(root().join("core/issues").join(dir))
        .unwrap_or_else(|e| panic!("{dir}: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// Modulname eines Tabs (`Page Titles` → `page_titles`, `Custom JavaScript` → `custom_js`).
fn module_of(tab: &str) -> &'static str {
    match tab {
        "Internal" => "internal",
        "External" => "external",
        "Security" => "security",
        "Response Codes" => "response_codes",
        "URL" => "url",
        "Page Titles" => "page_titles",
        "Meta Description" => "meta_description",
        "Meta Keywords" => "meta_keywords",
        "H1" => "h1",
        "H2" => "h2",
        "Content" => "content",
        "Images" => "images",
        "Canonicals" => "canonicals",
        "Pagination" => "pagination",
        "Directives" => "directives",
        "Hreflang" => "hreflang",
        "JavaScript" => "javascript",
        "Links" => "links",
        "Structured Data" => "structured_data",
        "Sitemaps" => "sitemaps",
        "PageSpeed" => "pagespeed",
        "Mobile" => "mobile",
        "Accessibility" => "accessibility",
        "Custom Search" => "custom_search",
        "Custom Extraction" => "custom_extraction",
        "Custom JavaScript" => "custom_js",
        "Analytics" => "analytics",
        "Search Console" => "search_console",
        "Validation" => "validation",
        "Link Metrics" => "link_metrics",
        "AI" => "ai",
        "AI Decisions" => "ai_decisions",
        "AI Suggestions" => "ai_suggestions",
        "AI Search" => "ai_search",
        "Change Detection" => "change_detection",
        other => panic!("Tab {other} ohne Modulzuordnung im Test"),
    }
}

fn registry(file: &str) -> toml::Table {
    std::fs::read_to_string(root().join("docs/plan").join(file))
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn there_is_one_file_per_tab_in_each_split_folder_and_the_loader_lists_them_all() {
    let modules: BTreeSet<String> = tabs::all()
        .iter()
        .map(|t| module_of(t.id).to_string())
        .collect();
    assert_eq!(modules.len(), 35);
    let listed: BTreeSet<String> = SPLIT_FILES.iter().map(|f| f.module.to_string()).collect();
    assert_eq!(listed, modules, "SPLIT_FILES = Tabs von tabs::all()");
    for dir in ["catalog.d", "texts/de.d", "texts/en.d"] {
        assert_eq!(stems(dir), modules, "Ordner core/issues/{dir}");
    }
    // Reihenfolge der Liste = Reihenfolge der Tabs.
    let order: Vec<&str> = tabs::all().iter().map(|t| module_of(t.id)).collect();
    let in_list: Vec<&str> = SPLIT_FILES.iter().map(|f| f.module).collect();
    assert_eq!(in_list, order);
}

#[test]
fn the_loader_reads_the_m1_file_first_and_then_every_split_file_unchanged() {
    let mut expected = parse_catalog(issues::m1_source()).unwrap();
    for f in SPLIT_FILES {
        expected.extend(parse_catalog(f.catalog).unwrap());
    }
    assert_eq!(catalog(), expected.as_slice());
    // Die M1-Datei ist unverändert Teil des Katalogs (Inhalte M1 unverändert).
    let m1 = parse_catalog(issues::m1_source()).unwrap();
    assert_eq!(&catalog()[..m1.len()], m1.as_slice());
    // M1-Texte in beiden Sprachen bleiben über den Loader erreichbar.
    for lang in Lang::ALL {
        for e in &m1 {
            assert!(
                issues::raw_texts(&e.doc_key, *lang).is_some(),
                "{}: Text ({}) fehlt",
                e.id,
                lang.code()
            );
        }
    }
}

#[test]
fn split_files_hold_only_entries_of_their_own_tab_and_their_own_texts() {
    for f in SPLIT_FILES {
        let tab_entries =
            parse_catalog(f.catalog).unwrap_or_else(|e| panic!("catalog.d/{}.toml: {e}", f.module));
        for e in &tab_entries {
            assert_eq!(
                module_of(&e.tab),
                f.module,
                "{}: Eintrag {} gehört in catalog.d/{}.toml",
                f.module,
                e.id,
                module_of(&e.tab)
            );
        }
        // Texte: nur Tabellen `issues.<id>` der eigenen Einträge, in beiden Sprachen.
        for (lang, src) in [("de", f.de), ("en", f.en)] {
            let table: toml::Table = src
                .parse()
                .unwrap_or_else(|e| panic!("texts/{lang}.d/{}.toml: {e}", f.module));
            let ids: BTreeSet<String> = table
                .get("issues")
                .and_then(|v| v.as_table())
                .map(|t| t.keys().cloned().collect())
                .unwrap_or_default();
            let own: BTreeSet<String> = tab_entries.iter().map(|e| e.id.clone()).collect();
            assert_eq!(ids, own, "texts/{lang}.d/{}.toml ↔ catalog.d", f.module);
            assert!(
                table.keys().all(|k| k == "issues"),
                "texts/{lang}.d/{}.toml: nur [issues.<id>]-Tabellen erlaubt",
                f.module
            );
        }
    }
}

#[test]
fn the_whole_catalog_is_valid_with_unique_ids_doc_keys_and_filters() {
    let problems = validate(catalog());
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_split_file_in_the_documented_format_loads_and_validates_like_the_m1_file() {
    // Format der `.d`-Dateien = Format der M1-Datei; dieselbe Prüfung gilt für beide.
    let sample = r#"
schema = 1

[[issue]]
id = "security-mixed-content"
tab = "Security"
filter = "Mixed Content"
type = "Issue"
priority = "High"
type_rule = "T-I4"
priority_rule = "P-H3"
needs_crawl_analysis = false
doc_key = "security-mixed-content"
"#;
    let entries = parse_catalog(sample).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(module_of(&entries[0].tab), "security");
    assert!(validate(&entries).is_empty());
    // Zusammen mit den Einträgen von M1 bleibt der Katalog gültig.
    let mut all = parse_catalog(issues::m1_source()).unwrap();
    all.extend(entries);
    assert!(validate(&all).is_empty());
}

/// (Tab, Filter) aller Filter mit `issue = true` in M1- und M2-Datei, M2-Filter getrennt.
type FilterKeys = BTreeSet<(String, String)>;

fn issue_filters() -> (FilterKeys, FilterKeys) {
    let collect = |file: &str| -> FilterKeys {
        let mut out = BTreeSet::new();
        for tab in registry(file)["tab"].as_array().unwrap() {
            for f in tab["filters"].as_array().unwrap() {
                if f.get("issue").and_then(|v| v.as_bool()) == Some(true) {
                    out.insert((
                        tab["id"].as_str().unwrap().to_lowercase(),
                        f["id"].as_str().unwrap().to_lowercase(),
                    ));
                }
            }
        }
        out
    };
    (collect("M1-filters.toml"), collect("M2-filters.toml"))
}

#[test]
fn catalog_and_register_agree_through_issue_true_in_the_filter_files() {
    let (m1, m2) = issue_filters();
    let in_catalog: BTreeSet<(String, String)> = catalog()
        .iter()
        .map(|e| (e.tab.to_lowercase(), e.filter.to_lowercase()))
        .collect();

    // 1. Jeder Katalogeintrag gehört zu einem Filter der Register (M1 oder M2) …
    let registered: BTreeSet<(String, String)> = tabs::all()
        .iter()
        .flat_map(|t| {
            t.filters
                .iter()
                .map(move |f| (t.id.to_lowercase(), f.id.to_lowercase()))
        })
        .collect();
    for key in &in_catalog {
        assert!(
            registered.contains(key),
            "Katalogeintrag ohne Filter: {key:?}"
        );
    }
    // … und der Filter ist in der Registerdatei als `issue = true` geführt.
    // spec: TECH-251 / UI-064: the registry's dynamic axe placeholder expands to these 93 keys.
    let axe_expected = [
        "accesskeys",
        "presentation-role-conflict",
        "tabindex",
        "skip-link",
        "aria-text",
        "aria-dialog-name",
        "aria-allowed-role",
        "aria-treeitem-name",
        "region",
        "landmark-one-main",
        "landmark-no-duplicate-banner",
        "landmark-banner-is-top-level",
        "landmark-no-duplicate-contentinfo",
        "landmark-no-duplicate-main",
        "landmark-complementary-is-top-level",
        "landmark-contentinfo-is-top-level",
        "landmark-main-is-top-level",
        "landmark-unique",
        "label-title-only",
        "frame-tested",
        "page-has-heading-one",
        "heading-order",
        "empty-heading",
        "meta-viewport-large",
        "image-redundant-alt",
        "empty-table-header",
        "table-duplicate-name",
        "scope-attr-valid",
        "scrollable-region-focusable",
        "aria-required-attr",
        "aria-conditional-attr",
        "aria-valid-attr-value",
        "aria-valid-attr",
        "aria-command-name",
        "aria-input-field-name",
        "aria-meter-name",
        "aria-progressbar-name",
        "aria-required-parent",
        "aria-roles",
        "aria-toggle-field-name",
        "aria-tooltip-name",
        "aria-required-children",
        "aria-deprecated-role",
        "aria-braille-equivalent",
        "aria-hidden-focus",
        "aria-hidden-body",
        "aria-prohibited-attr",
        "aria-allowed-attr",
        "duplicate-id-aria",
        "bypass",
        "label",
        "form-field-multiple-labels",
        "frame-title",
        "frame-title-unique",
        "frame-focusable-content",
        "document-title",
        "html-lang-valid",
        "html-has-lang",
        "html-xml-lang-mismatch",
        "meta-refresh",
        "input-image-alt",
        "image-alt",
        "object-alt",
        "area-alt",
        "role-img-alt",
        "svg-img-alt",
        "server-side-image-map",
        "video-caption",
        "no-autoplay-audio",
        "button-name",
        "input-button-name",
        "link-in-text-block",
        "link-name",
        "select-name",
        "summary-name",
        "marquee",
        "blink",
        "nested-interactive",
        "listitem",
        "list",
        "definition-list",
        "dlitem",
        "th-has-data-cells",
        "td-headers-attr",
        "meta-viewport",
        "valid-lang",
        "color-contrast",
        "meta-refresh-no-exceptions",
        "identical-links-same-purpose",
        "color-contrast-enhanced",
        "autocomplete-valid",
        "avoid-inline-spacing",
        "target-size",
    ];
    let flagged: BTreeSet<(String, String)> = m1
        .union(&m2)
        .cloned()
        .chain(
            axe_expected
                .into_iter()
                .map(|id| ("accessibility".into(), id.into())),
        )
        .collect();
    for key in &in_catalog {
        assert!(
            flagged.contains(key),
            "Katalogeintrag ohne issue = true: {key:?}"
        );
    }
    // 2. Jeder Befund-Filter von M1 hat einen Eintrag (Stand M1, unverändert).
    for key in &m1 {
        assert!(
            in_catalog.contains(key),
            "M1: issue = true ohne Katalogeintrag: {key:?}"
        );
    }
    // 3. Für M2 liefert jedes Paket seine Einträge in catalog.d/<tab>.toml; offen sind nur Filter, die M2 ergänzt.
    let pending: Vec<&(String, String)> = m2.iter().filter(|k| !in_catalog.contains(*k)).collect();
    for key in &pending {
        assert!(!m1.contains(*key), "{key:?}");
    }
    // Ausgabe für den Überblick (`--nocapture`): so viele Befund-Filter von M2 warten noch auf ihren Eintrag.
    eprintln!(
        "{} von {} Befund-Filtern von M2 ohne Katalogeintrag",
        pending.len(),
        m2.len()
    );
}

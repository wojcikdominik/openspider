//! Konfiguration v2 (M2-F01, `docs/plan/M2.md` 3.3): Standardwerte der neuen Abschnitte, Vollständigkeit der
//! Rundreise, Validierung, Migration v1 → v2 mit Sicherung (OUT-226) und JSON-Schema.

use std::fs;
use std::path::{Path, PathBuf};

use os_model::*;
use serde_json::Value as Json;

mod v2_support;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(rel: &str) -> PathBuf {
    repo_root().join("tests/fixtures").join(rel)
}

// spec: OUT-226
#[test]
fn schema_version_is_2_and_written_files_say_so() {
    assert_eq!(CONFIG_SCHEMA_VERSION, 2);
    let text = CrawlConfig::default().to_osconfig_string().unwrap();
    assert!(text.starts_with("schema_version = 2\ngenerator = \"openspider "));
}

// spec: CRAWL-080, CRAWL-081, CRAWL-082, CRAWL-084, PERF-006
#[test]
fn rendering_defaults_follow_the_spec() {
    let r = RenderingConfig::default();
    assert_eq!(r.mode, RenderingMode::Text);
    assert_eq!(r.ajax_timeout_s, 5.0);
    assert!(r.js_error_reporting);
    assert_eq!((r.viewport.width, r.viewport.height), (411, 731));
    assert_eq!(r.viewport.preset, "googlebot_mobile");
    assert!(r.screenshots.enabled);
    assert_eq!(r.screenshots.wait_ms, 300);
    assert_eq!(r.screenshots.format, ScreenshotFormat::Png);
    assert_eq!(r.screenshots.jpeg_quality, 80);
    assert_eq!(
        r.tabs, None,
        "Standard: Threads, gedeckelt auf die CPU-Kerne"
    );
    assert!(r.restart_after_pages > 0 && r.restart_memory_mb > 0 && r.page_timeout_s > 0);
}

// spec: CRAWL-055, CRAWL-061, CRAWL-062, CRAWL-063, CRAWL-064, CRAWL-065, TECH-250
#[test]
fn extraction_defaults_follow_the_spec() {
    let e = ExtractionConfig::default();
    assert!(
        e.indexability && e.word_count && e.readability && e.text_ratio && e.page_size && e.forms
    );
    assert!(!e.accessibility, "Barrierefreiheit: Standard aus");
    assert!(!e.cookies && !e.store_html && !e.store_rendered_html && !e.srcset);
    assert!(e.meta_robots && e.x_robots_tag);
    assert_eq!(e.structured_data, StructuredDataConfig::default());
    assert!(!e.structured_data.json_ld && !e.structured_data.case_sensitive);
    let pdf = PdfConfig::default();
    assert!(!pdf.store && !pdf.extract_properties && !pdf.extract_link_text);
}

// spec: CRAWL-069, CRAWL-070, CRAWL-071, CRAWL-072, CRAWL-073, CRAWL-074
#[test]
fn limits_defaults_follow_the_spec() {
    let l = LimitsConfig::default();
    assert_eq!(l.max_folder_depth, None);
    assert_eq!(l.max_query_params, None);
    assert_eq!(l.max_urls_per_subdomain, None);
    assert_eq!(l.max_redirects, 10);
    assert_eq!(l.max_url_length, 10_000);
    assert_eq!(l.max_links_per_url, 10_000);
}

// spec: CRAWL-004, CRAWL-020, CRAWL-091, CRAWL-092, CRAWL-094, TECH-008, CRAWL-139
#[test]
fn advanced_defaults_follow_the_spec() {
    let a = AdvancedConfig::default();
    assert!(!a.crawl_fragments && !a.assume_html);
    assert!(!a.respect_noindex && !a.respect_canonical && !a.respect_self_ref_meta_refresh);
    assert!(a.html_validation, "TECH-008: Standard an");
    assert!(!a.green_hosting);
}

// spec: CRAWL-018, CRAWL-023, CRAWL-024, CRAWL-096, CRAWL-097
#[test]
fn http_defaults_follow_the_spec() {
    let h = HttpConfig::default();
    assert!(h.respect_hsts);
    assert!(!h.ignore_certificate_errors);
    assert_eq!(h.version, HttpVersion::Auto);
    assert_eq!((h.retries_5xx, h.retries_network), (0, 1));
    assert!(h.throttle_on_429);
    assert!(h.headers.is_empty() && h.accept_language.is_none());
}

// spec: INTEG-081
#[test]
fn link_position_defaults_are_the_six_standard_rules_in_order() {
    let rules = LinkPositionsConfig::default().rules;
    let names: Vec<_> = rules.iter().map(|r| r.position.as_str()).collect();
    assert_eq!(
        names,
        [
            "Navigation",
            "Header",
            "Aside",
            "Sidebar",
            "Footer",
            "Content"
        ]
    );
    assert_eq!(rules[0].contains, ["nav", "menu"]);
    assert_eq!(rules[2].contains, ["aside"]);
    assert_eq!(rules[3].contains, ["sidebar"]);
    assert!(rules[5].contains.is_empty(), "Content ist die Auffangregel");
}

// spec: OUT-121, OUT-123, OUT-124, OUT-125, OUT-126, OUT-128
#[test]
fn sitemap_generator_defaults_follow_the_spec() {
    let s = SitemapGenConfig::default();
    assert!(
        !(s.include_noindex || s.include_canonicalised || s.include_paginated || s.include_pdfs)
    );
    assert!(!(s.include_3xx || s.include_4xx || s.include_5xx || s.include_no_response));
    assert!(s.lastmod.enabled && s.lastmod.source == LastmodSource::Server);
    assert_eq!(s.priority.by_depth, [1.0, 0.9, 0.8, 0.7, 0.6, 0.5]);
    assert_eq!(
        s.changefreq.by_depth,
        [
            ChangeFreq::Daily,
            ChangeFreq::Daily,
            ChangeFreq::Weekly,
            ChangeFreq::Weekly,
            ChangeFreq::Monthly
        ]
    );
    assert!(s.changefreq.enabled);
    assert!(!s.images.enabled && s.images.max_inlinks.is_none());
    assert!(!s.hreflang.enabled && s.hreflang.only_valid_targets);
}

// spec: TECH-300, OUT-146, TECH-309, TECH-310
#[test]
fn change_detection_default_selects_all_16_elements_except_content() {
    assert_eq!(ChangeElement::ALL.len(), 16);
    let cd = ChangeDetectionConfig::default();
    assert_eq!(cd.elements.len(), 15);
    assert_eq!(cd.elements.first(), Some(&ChangeElement::Indexability));
    assert!(!cd.elements.contains(&ChangeElement::Content));
    // Die OS-Erweiterungen stehen hinter den 14 Referenzelementen und sind an (Nutzerentscheidung 04.11.2026).
    assert_eq!(ChangeElement::ALL[13], ChangeElement::Content);
    assert_eq!(
        &ChangeElement::ALL[14..],
        &[ChangeElement::Canonical, ChangeElement::RedirectTarget]
    );
    assert_eq!(
        &cd.elements[13..],
        &[ChangeElement::Canonical, ChangeElement::RedirectTarget]
    );
    assert_eq!(ChangeElement::RedirectTarget.as_str(), "redirect_target");
    assert_eq!(cd.content_threshold_pct, 10);
    assert!(CompareConfig::default().mapping.is_empty());
}

// spec: ONPAGE-152, ONPAGE-153, ONPAGE-154, ONPAGE-144, ONPAGE-149
#[test]
fn content_and_language_defaults_follow_the_spec() {
    let c = CrawlConfig::default();
    assert!(!c.content.readability.language_aware);
    assert!(!c.spelling.enabled && !c.grammar.enabled);
    assert_eq!(c.spelling.mode, SpellingMode::Auto);
    assert_eq!(
        c.spelling.default_variants.get("en").map(String::as_str),
        Some("en-US")
    );
    assert!(!c.duplicates.near_enabled);
    assert_eq!(c.duplicates.near_threshold_pct, 90);
    assert!(c.analysis.near_duplicates);
}

// spec: OUT-225
#[test]
fn api_section_has_no_secrets_and_all_seven_sources() {
    let json = serde_json::to_value(ApiConfig::default()).unwrap();
    let keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
    for k in ["ga4", "gsc", "psi", "majestic", "ahrefs", "moz", "drive"] {
        assert!(keys.contains(&k.to_string()), "{k} fehlt");
    }
    let text = serde_json::to_string(&json).unwrap().to_lowercase();
    for word in ["secret", "token", "password", "api_key", "apikey"] {
        assert!(!text.contains(word), "{word} darf in api.* nicht vorkommen");
    }
}

// --- Vollständigkeit der Rundreise --------------------------------------------------------------------------

/// Alle Blattpfade des JSON-Schemas (Objekte mit `properties` werden durchlaufen, Listen und Maps sind Blätter).
fn schema_leaves(root: &Json, node: &Json, prefix: &str, out: &mut Vec<String>) {
    let mut node = node;
    for _ in 0..8 {
        if let Some(r) = node.get("$ref").and_then(Json::as_str) {
            node = root.pointer(r.trim_start_matches('#')).unwrap();
        } else if let Some([only]) = node
            .get("allOf")
            .and_then(Json::as_array)
            .map(Vec::as_slice)
        {
            node = only;
        } else if let Some(alts) = node.get("anyOf").and_then(Json::as_array) {
            let non_null: Vec<_> = alts
                .iter()
                .filter(|a| a.get("type").and_then(Json::as_str) != Some("null"))
                .collect();
            if non_null.len() == 1 {
                node = non_null[0];
            } else {
                break;
            }
        } else {
            break;
        }
    }
    match node.get("properties").and_then(Json::as_object) {
        Some(props) => {
            for (k, child) in props {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                schema_leaves(root, child, &p, out);
            }
        }
        None => out.push(prefix.to_string()),
    }
}

// spec: OUT-220, OUT-221
#[test]
fn every_schema_leaf_is_covered_by_the_all_fields_changed_config() {
    let mut changed = CrawlConfig::default();
    v2_support::change_all_v2_fields(&mut changed);
    // M1-Felder zusätzlich ändern, damit das Ergebnis die ganze Konfiguration abdeckt
    changed.mode = CrawlMode::List;
    changed.speed.threads = 17;

    let def = serde_json::to_value(CrawlConfig::default()).unwrap();
    let chg = serde_json::to_value(&changed).unwrap();
    let at = |v: &Json, path: &str| v.pointer(&format!("/{}", path.replace('.', "/"))).cloned();

    let mut leaves = Vec::new();
    schema_leaves(json_schema(), json_schema(), "", &mut leaves);
    assert!(
        leaves.len() > 150,
        "Schema sollte >150 Blätter haben, hat {}",
        leaves.len()
    );

    // Blätter, die v2_support absichtlich nicht ändert: M1-Felder (durch model_config.rs und config_io.rs abgedeckt)
    let m1_prefixes = [
        "schema_version",
        "generator",
        "mode",
        "scope.",
        "spider.",
        "speed.",
        "user_agent.",
        "robots.",
        "network.",
        "title.",
        "meta_description.",
        "url.",
        "h1.",
        "h2.",
        "image.",
        "duplicates.",
        "issues.",
        "links.",
        "hreflang.",
        "sitemaps.",
        "validation.",
        "export.",
        "auth.basic",
    ];
    let mut untouched = Vec::new();
    for leaf in &leaves {
        let is_m1 = m1_prefixes
            .iter()
            .any(|p| leaf == p.trim_end_matches('.') || leaf.starts_with(p));
        let m1_in_v2_struct = [
            "limits.max_urls",
            "limits.max_depth",
            "limits.max_redirects",
            "limits.max_page_kb",
            "advanced.cookies",
            "advanced.response_timeout_s",
            "advanced.assume_html",
            "advanced.respect_self_ref_meta_refresh",
            "rendering.mode",
            "extraction.page_titles",
            "extraction.meta_description",
            "extraction.meta_keywords",
            "extraction.h1",
            "extraction.h2",
            "extraction.hash",
            "extraction.response_time",
            "extraction.last_modified",
            "extraction.http_headers",
            "extraction.store_html",
            "content.area.",
            "content.low_word_count",
            "content.soft404_phrases",
            "pdf.store",
            "analysis.auto_at_end",
            "analysis.link_score",
            "analysis.canonicals",
            "analysis.pagination",
            "analysis.hreflang",
            "analysis.links",
            "analysis.sitemaps",
            "analysis.content",
            "analysis.images",
            "analysis.analytics",
            "analysis.search_console",
            "analysis.link_metrics",
            "analysis.amp",
        ]
        .iter()
        .any(|p| leaf == p.trim_end_matches('.') || leaf.starts_with(p));
        if is_m1 || m1_in_v2_struct {
            continue;
        }
        if at(&def, leaf) == at(&chg, leaf) {
            untouched.push(leaf.clone());
        }
    }
    assert!(
        untouched.is_empty(),
        "v2_support ändert diese Felder nicht (Rundreise ungeprüft): {untouched:?}"
    );
}

// spec: OUT-220, OUT-221
#[test]
fn all_fields_changed_round_trips_byte_identically() {
    let mut cfg = CrawlConfig::default();
    v2_support::change_all_v2_fields(&mut cfg);
    let text = cfg.to_osconfig_string().unwrap();
    let loaded = CrawlConfig::load_str(&text).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(loaded.config.api, cfg.api);
    assert_eq!(loaded.config.sitemap_gen, cfg.sitemap_gen);
    assert_eq!(loaded.config.segments, cfg.segments);
    assert_eq!(loaded.config.compare, cfg.compare);
    assert_eq!(loaded.config.to_osconfig_string().unwrap(), text);
    // Geheimnisse stehen nie in der Datei
    assert!(!text.to_lowercase().contains("password"));
}

// spec: OUT-221
#[test]
fn schema_lists_the_new_sections_and_ranges() {
    let schema = json_schema();
    let props = schema["properties"].as_object().unwrap();
    for section in [
        "rendering",
        "extraction",
        "limits",
        "advanced",
        "content",
        "spelling",
        "grammar",
        "custom_search",
        "custom_extraction",
        "custom_js",
        "link_positions",
        "rewrite",
        "cdns",
        "http",
        "auth",
        "api",
        "segments",
        "compare",
        "sitemap_gen",
        "analysis",
    ] {
        assert!(props.contains_key(section), "{section} fehlt im Schema");
    }
    let keys = settable_keys();
    for k in [
        "rendering.ajax_timeout_s",
        "advanced.html_validation",
        "http.respect_hsts",
        "cdns",
        "api.ga4.enabled",
    ] {
        assert!(
            keys.iter().any(|x| x == k),
            "{k} sollte per --set setzbar sein"
        );
    }
    // Bereichsgrenzen aus INT_RANGES erscheinen im Schema
    let mut c = CrawlConfig::default();
    c.apply_set("rendering.ajax_timeout_s=60").unwrap();
    assert!(c.apply_set("rendering.ajax_timeout_s=61").is_err());
    assert!(c.apply_set("http.retries_5xx=11").is_err());
}

// --- Validierung ---------------------------------------------------------------------------------------------

fn errors_of(c: &CrawlConfig) -> Vec<String> {
    c.validate().errors.iter().map(|e| e.path.clone()).collect()
}

// spec: CRAWL-140, OUT-221
#[test]
fn default_and_fully_changed_configs_validate() {
    assert!(CrawlConfig::default().validate().is_ok());
    let mut c = CrawlConfig::default();
    v2_support::change_all_v2_fields(&mut c);
    let rep = c.validate();
    assert!(rep.is_ok(), "{:?}", rep.errors);
}

// spec: INTEG-020, INTEG-040, INTEG-060
#[test]
fn custom_entries_are_limited_to_100_and_names_must_be_unique() {
    let mut c = CrawlConfig::default();
    c.custom_search.filters = (0..101)
        .map(|i| SearchFilter {
            name: format!("F{i}"),
            expression: "x".into(),
            ..SearchFilter::default()
        })
        .collect();
    assert!(errors_of(&c).contains(&"custom_search.filters".to_string()));

    let mut c = CrawlConfig::default();
    c.custom_search.filters = vec![
        SearchFilter {
            name: "A".into(),
            expression: "x".into(),
            ..SearchFilter::default()
        },
        SearchFilter {
            name: "a".into(),
            expression: "y".into(),
            ..SearchFilter::default()
        },
        SearchFilter {
            name: "B".into(),
            expression: "  ".into(),
            ..SearchFilter::default()
        },
        SearchFilter {
            name: "C".into(),
            kind: SearchKind::Regex,
            expression: "(abc".into(),
            ..SearchFilter::default()
        },
    ];
    let e = errors_of(&c);
    assert!(
        e.contains(&"custom_search.filters[1].name".to_string()),
        "{e:?}"
    );
    assert!(
        e.contains(&"custom_search.filters[2].expression".to_string()),
        "{e:?}"
    );
    assert!(
        e.contains(&"custom_search.filters[3].expression".to_string()),
        "{e:?}"
    );

    let mut c = CrawlConfig::default();
    c.custom_extraction.extractors = (0..101)
        .map(|i| Extractor {
            name: format!("E{i}"),
            expression: "//a".into(),
            ..Extractor::default()
        })
        .collect();
    assert!(errors_of(&c).contains(&"custom_extraction.extractors".to_string()));

    let mut c = CrawlConfig::default();
    c.custom_js.snippets = vec![JsSnippet {
        kind: JsSnippetKind::Action,
        timeout_s: 0,
        ..JsSnippet::default()
    }];
    assert!(errors_of(&c).contains(&"custom_js.snippets[0].timeout_s".to_string()));
}

// spec: CRAWL-134
#[test]
fn segments_validate_names_regex_and_segment_order() {
    let cond = |op: SegmentOp, value: &str| SegmentGroup {
        conditions: vec![SegmentCondition {
            field: "Address".into(),
            op,
            value: value.into(),
        }],
    };
    let seg = |name: &str, g: SegmentGroup| SegmentConfig {
        name: name.into(),
        color: "#112233".into(),
        groups: vec![g],
    };
    let mut c = CrawlConfig::default();
    c.segments.list = vec![
        seg("Shop", cond(SegmentOp::Contains, "/produkt/")),
        seg("shop", cond(SegmentOp::RegexMatches, "(")),
        seg("", cond(SegmentOp::Contains, "x")),
        seg("Fehler", cond(SegmentOp::InSegment, "Shop")),
        seg("Falsch", cond(SegmentOp::InSegment, "Spaeter")),
        seg("Spaeter", cond(SegmentOp::Contains, "y")),
    ];
    let e = errors_of(&c);
    assert!(e.contains(&"segments.list[1].name".to_string()), "{e:?}");
    assert!(
        e.contains(&"segments.list[1].groups[0].conditions[0].value".to_string()),
        "{e:?}"
    );
    assert!(e.contains(&"segments.list[2].name".to_string()), "{e:?}");
    assert!(
        !e.iter().any(|p| p.starts_with("segments.list[3]")),
        "{e:?}"
    );
    assert!(
        e.contains(&"segments.list[4].groups[0].conditions[0].value".to_string()),
        "{e:?}"
    );
}

// spec: INTEG-081
#[test]
fn link_positions_need_a_catch_all_rule_last() {
    let mut c = CrawlConfig::default();
    c.link_positions.rules.pop();
    assert!(errors_of(&c).contains(&"link_positions.rules".to_string()));
    c.link_positions.rules.clear();
    assert!(errors_of(&c).contains(&"link_positions.rules".to_string()));
}

// spec: OUT-123, OUT-124, OUT-125, CRAWL-115, CRAWL-104, OUT-146, TECH-300
#[test]
fn http_rewrite_mapping_sitemap_and_change_detection_are_validated() {
    let mut c = CrawlConfig::default();
    c.http.headers = vec![
        HeaderEntry {
            name: "Bad Name".into(),
            value: "x".into(),
            host: None,
        },
        HeaderEntry {
            name: "X-A".into(),
            value: "a\nb".into(),
            host: None,
        },
    ];
    c.rewrite.regex_replace = vec![RegexReplace {
        pattern: "(".into(),
        replacement: String::new(),
    }];
    c.compare.mapping = vec![MappingRule {
        pattern: "[".into(),
        replacement: String::new(),
    }];
    c.sitemap_gen.lastmod.source = LastmodSource::Custom;
    c.sitemap_gen.priority.by_depth = vec![1.5];
    c.sitemap_gen.exclude_patterns = vec!["(".into()];
    c.compare.change_detection.elements = vec![ChangeElement::H1, ChangeElement::Indexability];
    c.cdns = vec!["a b".into()];
    let e = errors_of(&c);
    for p in [
        "http.headers[0].name",
        "http.headers[1].value",
        "rewrite.regex_replace[0].pattern",
        "compare.mapping[0].pattern",
        "sitemap_gen.lastmod.custom_date",
        "sitemap_gen.priority.by_depth",
        "sitemap_gen.exclude_patterns[0]",
        "compare.change_detection.elements",
        "cdns[0]",
    ] {
        assert!(e.contains(&p.to_string()), "{p} fehlt in {e:?}");
    }
}

// spec: CRAWL-077, TECH-250
#[test]
fn javascript_only_options_warn_in_text_mode() {
    let mut c = CrawlConfig::default();
    c.extraction.accessibility = true;
    let w: Vec<_> = c
        .validate()
        .warnings
        .iter()
        .map(|w| w.path.clone())
        .collect();
    assert!(w.contains(&"extraction.accessibility".to_string()), "{w:?}");
    c.rendering.mode = RenderingMode::JavaScript;
    assert!(c
        .validate()
        .warnings
        .iter()
        .all(|w| w.path != "extraction.accessibility"));
}

// --- Migration v1 → v2 (OUT-226) -----------------------------------------------------------------------------

// spec: OUT-226
#[test]
fn v1_files_load_unchanged_with_defaults_for_the_new_sections() {
    let text = fs::read_to_string(fixture("config/v1/default.osconfig")).unwrap();
    assert!(text.starts_with("schema_version = 1\n"));
    let l = CrawlConfig::load_str(&text).unwrap();
    assert!(l.warnings.is_empty(), "{:?}", l.warnings);
    assert_eq!(l.migrated_from, Some(1));
    assert_eq!(
        l.config.schema_version, 2,
        "im Speicher gilt die aktuelle Version"
    );
    let mut expected = CrawlConfig::default();
    expected.scope.crawl_outside_start_folder = false;
    expected.extraction.http_headers = true;
    expected.content.area.exclude_elements = vec!["nav".into(), "footer".into()];
    // Die v1-Datei nennt ihren Erzeuger; Laden übernimmt ihn (erst Schreiben stempelt die aktuelle Version).
    expected.generator = "openspider 0.1.0".to_owned();
    assert_eq!(l.config, expected);
    assert_eq!(
        fs::read_to_string(fixture("config/v1/default.osconfig")).unwrap(),
        text,
        "Laden schreibt nie"
    );
}

// spec: OUT-226
#[test]
fn migrate_toml_lifts_v1_to_v2_and_keeps_every_value() {
    let src = "schema_version = 1\ngenerator = \"openspider 0.1.0\"\nmode = \"list\"\n\n[speed]\nthreads = 12\n\n[limits]\nmax_depth = 4\n\n[extraction]\nstore_html = true\n";
    let (text, rep) = migrate_toml(src).unwrap();
    assert_eq!((rep.from_version, rep.to_version), (1, 2));
    assert!(text.starts_with("schema_version = 2\n"));
    let c = CrawlConfig::load_str(&text).unwrap();
    assert!(c.warnings.is_empty());
    assert_eq!(c.config.mode, CrawlMode::List);
    assert_eq!(c.config.speed.threads, 12);
    assert_eq!(c.config.limits.max_depth, Some(4));
    assert!(c.config.extraction.store_html);
    assert_eq!(
        c.config.limits.max_url_length, 10_000,
        "neue Felder mit Standardwert"
    );
    for section in ["http", "api", "sitemap_gen", "compare", "segments"] {
        assert!(
            rep.added_sections.contains(&section.to_string()),
            "{section}: {:?}",
            rep.added_sections
        );
        assert!(
            text.contains(&format!("[{section}")),
            "{section} fehlt in der migrierten Datei"
        );
    }
    assert!(rep.dropped_keys.is_empty());
    // idempotent: eine v2-Datei bleibt unverändert
    let (again, rep2) = migrate_toml(&text).unwrap();
    assert_eq!(again, text);
    assert_eq!(rep2.from_version, 2);
}

// spec: OUT-226
#[test]
fn migrate_file_makes_a_backup_first_and_is_idempotent() {
    let dir = tempfile_dir("migrate_backup");
    let path = dir.join("profil.osconfig");
    let original = "# mein Profil\nschema_version = 1\n[speed]\nthreads = 9\nneu = 1\n";
    fs::write(&path, original).unwrap();

    let rep = CrawlConfig::migrate_file(&path)
        .unwrap()
        .expect("v1 wird migriert");
    assert_eq!((rep.from_version, rep.to_version), (1, 2));
    assert_eq!(rep.dropped_keys, ["speed.neu"]);
    let backup = rep.backup.clone().unwrap();
    assert_eq!(
        backup.file_name().unwrap().to_string_lossy(),
        "profil.osconfig.bak-v1"
    );
    assert_eq!(
        fs::read_to_string(&backup).unwrap(),
        original,
        "Sicherung ist byte-gleich zum Original"
    );
    let migrated = fs::read_to_string(&path).unwrap();
    assert!(migrated.starts_with("schema_version = 2\n"));
    assert_eq!(
        CrawlConfig::load_file(&path).unwrap().config.speed.threads,
        9
    );

    // zweiter Lauf: nichts mehr zu tun, keine neue Sicherung
    assert!(CrawlConfig::migrate_file(&path).unwrap().is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), migrated);

    // vorhandene Sicherung wird nie überschrieben
    fs::write(&path, original).unwrap();
    let rep = CrawlConfig::migrate_file(&path).unwrap().unwrap();
    assert_eq!(
        rep.backup.unwrap().file_name().unwrap().to_string_lossy(),
        "profil.osconfig.bak-v1-2"
    );
    assert_eq!(
        fs::read_to_string(dir.join("profil.osconfig.bak-v1")).unwrap(),
        original
    );
}

// spec: OUT-226
#[test]
fn newer_files_are_never_touched_by_migration() {
    let dir = tempfile_dir("migrate_newer");
    let path = dir.join("zukunft.osconfig");
    let text = fs::read_to_string(fixture("config/future-version.osconfig")).unwrap();
    fs::write(&path, &text).unwrap();
    assert!(CrawlConfig::migrate_file(&path).unwrap().is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    assert!(
        fs::read_dir(&dir).unwrap().count() == 1,
        "keine Sicherung für neuere Dateien"
    );
}

// spec: OUT-226
#[test]
fn migrated_v1_fixture_preserves_legacy_settings_against_v2_defaults() {
    let (text, _) =
        migrate_toml(&fs::read_to_string(fixture("config/v1/default.osconfig")).unwrap()).unwrap();
    let mut expected = CrawlConfig::default();
    expected.scope.crawl_outside_start_folder = false;
    expected.extraction.http_headers = true;
    expected.content.area.exclude_elements = vec!["nav".into(), "footer".into()];
    assert_eq!(text, expected.to_osconfig_string().unwrap());
}

fn tempfile_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("os-model-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// spec: INTEG-081
// Infrastruktur (M2-F05a) für INTEG-081; Fachprüfung M2-D16.
#[test]
fn legacy_v2_content_catch_all_remains_loadable() {
    let profile = include_str!("../../../tests/fixtures/config/default.osconfig");
    let profile = profile.replace("contains = []", "contains = [\"\"]");
    assert!(profile.contains("contains = [\"\"]"));
    let cfg = CrawlConfig::from_toml_str(&profile).unwrap();
    assert!(cfg.validate().errors.is_empty());
}

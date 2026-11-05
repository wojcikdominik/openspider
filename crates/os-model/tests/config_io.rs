//! `.osconfig` laden und speichern: Fixtures, Warnungen, Fehler mit Zeile/Spalte, byte-identische Rundreise,
//! Standard-Profil (CRAWL-132, OUT-220, OUT-221).

use std::fs;
use std::path::{Path, PathBuf};

use os_model::*;
use proptest::prelude::*;

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(rel)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("config_io-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn load(rel: &str) -> LoadedConfig {
    CrawlConfig::load_file(&fixture(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn warning_paths(l: &LoadedConfig) -> Vec<&str> {
    l.warnings.iter().map(|w| w.path.as_str()).collect()
}

// spec: OUT-221, CRAWL-132
#[test]
fn missing_fields_get_default_values() {
    let l = load("config/partial.osconfig");
    assert!(l.warnings.is_empty(), "{:?}", l.warnings);
    let mut expected = CrawlConfig::default();
    expected.speed.threads = 2;
    assert_eq!(l.config, expected);

    // Der Mindestumfang: nur eine Option.
    let m = load("crawl-config/minimal.osconfig");
    assert!(m.warnings.is_empty());
    let mut expected = CrawlConfig::default();
    expected.limits.max_depth = Some(2);
    // Die Datei nennt ihren Erzeuger; Laden übernimmt ihn unverändert (erst Schreiben stempelt neu).
    expected.generator = "openspider 0.1.0".to_owned();
    assert_eq!(m.config, expected);

    // Auch eine leere Datei ist eine gültige Konfiguration.
    assert_eq!(
        CrawlConfig::load_str("").unwrap().config,
        CrawlConfig::default()
    );
}

// spec: OUT-221
#[test]
fn hand_edited_file_with_comments_loads() {
    let l = load("config/commented.osconfig");
    assert!(l.warnings.is_empty(), "{:?}", l.warnings);
    let c = l.config;
    assert_eq!(c.generator, "handgeschrieben");
    assert_eq!(c.mode, CrawlMode::Spider);
    assert_eq!(c.scope.include, vec![".*/blog/.*".to_string()]);
    assert_eq!(c.scope.exclude, vec![".*/privat/.*".to_string()]);
    assert_eq!(c.speed.threads, 8);
    assert_eq!(c.speed.max_urls_per_sec, Some(2.5));
    assert_eq!(c.limits.max_depth, Some(3));
    assert_eq!(c.title.max_chars, 65);
    assert_eq!(c.title.min_chars, 30); // nicht angegeben = Standard
}

// spec: OUT-221, CRAWL-132
#[test]
fn unknown_fields_are_warnings_with_field_path() {
    let l = load("config/unknown-fields.osconfig");
    assert_eq!(
        warning_paths(&l),
        [
            "speed.turbo",
            "storage",
            "spider.images.preview",
            "auth.basic[0].password"
        ]
    );
    assert!(l
        .warnings
        .iter()
        .all(|w| w.kind == ConfigIssueKind::UnknownField));
    // Zeile/Spalte des unbekannten Schlüssels
    assert_eq!(
        (l.warnings[0].line, l.warnings[0].column),
        (Some(6), Some(1))
    );
    assert_eq!(l.warnings[1].line, Some(8));
    assert!(l.warnings[1].message.contains("Abschnitt"));
    assert_eq!(l.warnings[2].line, Some(14));
    // Das Laden ging weiter: bekannte Werte sind übernommen.
    assert_eq!(l.config.speed.threads, 3);
    assert!(!l.config.spider.images.crawl);
    assert_eq!(l.config.auth.basic.len(), 1);
    assert_eq!(l.config.auth.basic[0].secret_ref, "staging");
    // Meldung als Text: Feldpfad und Zeile
    assert!(l.warnings[0]
        .to_string()
        .starts_with("speed.turbo (Zeile 6, Spalte 1)"));
}

// spec: CRAWL-132
#[test]
fn passwords_never_enter_the_profile() {
    let l = load("config/unknown-fields.osconfig");
    let pw = l
        .warnings
        .iter()
        .find(|w| w.path == "auth.basic[0].password")
        .unwrap();
    assert!(pw.message.contains("Passwörter gehören nicht ins Profil"));
    let text = l.config.to_osconfig_string().unwrap();
    assert!(text.contains("secret_ref = \"staging\""));
    assert!(!text.contains("password"));
    assert!(!text.contains("nicht-im-profil-speichern"));
    // Auch ein Passwort in der Warnung selbst wird nie wiedergegeben.
    assert!(!l
        .warnings
        .iter()
        .any(|w| w.to_string().contains("nicht-im-profil")));
}

// spec: CRAWL-132
#[test]
fn memory_mode_key_is_reported_like_any_unknown_key() {
    let l = CrawlConfig::load_str("[storage]\nmode = \"memory\"\n").unwrap();
    assert_eq!(warning_paths(&l), ["storage"]);
    assert_eq!(l.config, CrawlConfig::default());
}

// spec: OUT-221
#[test]
fn newer_schema_version_loads_with_warning() {
    let l = load("config/future-version.osconfig");
    let kinds: Vec<_> = l
        .warnings
        .iter()
        .map(|w| (w.kind, w.path.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (ConfigIssueKind::NewerSchemaVersion, "schema_version"),
            (ConfigIssueKind::UnknownField, "speed.adaptive"),
            (ConfigIssueKind::UnknownField, "ai"),
        ]
    );
    assert!(l.warnings[0].message.contains("schema_version 3"));
    assert_eq!(l.warnings[0].line, Some(2));
    assert_eq!(l.config.mode, CrawlMode::List);
    assert_eq!(l.config.speed.threads, 12);
    assert_eq!(l.config.schema_version, 3);
    // Gespeichert wird im Format dieser Version.
    let text = l.config.to_osconfig_string().unwrap();
    assert!(text.starts_with("schema_version = 2\n"), "{text}");
}

// spec: OUT-221
#[test]
fn type_error_reports_line_column_and_field() {
    let err = CrawlConfig::load_file(&fixture("config/invalid/type-error.osconfig")).unwrap_err();
    assert_eq!(err.line, Some(5));
    assert_eq!(err.column, Some(13));
    assert_eq!(err.field.as_deref(), Some("limits.max_depth"));
    assert!(err.file.as_ref().unwrap().ends_with("type-error.osconfig"));
    let shown = err.to_string();
    assert!(shown.contains("Zeile 5, Spalte 13"), "{shown}");
    assert!(shown.contains("limits.max_depth"), "{shown}");
    assert!(shown.contains("type-error.osconfig"), "{shown}");

    // direkt aus Text, ohne Datei
    let err =
        CrawlConfig::load_str("max_depth = \"abc\"\n[limits]\nmax_depth = \"abc\"\n").unwrap_err();
    assert_eq!((err.line, err.column), (Some(3), Some(13)));
    assert!(err.file.is_none());
}

// spec: OUT-221
#[test]
fn syntax_enum_and_negative_errors_carry_positions() {
    let syntax =
        CrawlConfig::load_file(&fixture("config/invalid/syntax-error.osconfig")).unwrap_err();
    assert_eq!(syntax.line, Some(1));
    assert!(syntax.column.is_some());

    let bad_enum =
        CrawlConfig::load_file(&fixture("config/invalid/bad-enum.osconfig")).unwrap_err();
    assert_eq!((bad_enum.line, bad_enum.column), (Some(2), Some(8)));
    assert_eq!(bad_enum.field.as_deref(), Some("mode"));
    assert!(bad_enum.message.contains("turbo"));

    let negative =
        CrawlConfig::load_file(&fixture("config/invalid/negative.osconfig")).unwrap_err();
    assert_eq!(negative.line, Some(4));
    assert_eq!(negative.field.as_deref(), Some("speed.threads"));

    // Typfehler in einem Array-of-Tables-Eintrag nennt den Eintrag.
    let err =
        CrawlConfig::load_str("[[auth.basic]]\nprefix = \"p\"\nusername = 5\nsecret_ref = \"s\"\n")
            .unwrap_err();
    assert_eq!(err.field.as_deref(), Some("auth.basic[0].username"));
    assert_eq!(err.line, Some(3));
}

// spec: OUT-221
#[test]
fn unreadable_file_and_bom() {
    let err = CrawlConfig::load_file(&fixture("config/gibt-es-nicht.osconfig")).unwrap_err();
    assert!(err.file.is_some() && err.line.is_none());
    assert!(err.message.contains("nicht lesbar"));

    // Windows-Editoren schreiben gern ein UTF-8-BOM.
    let l = CrawlConfig::load_str("\u{feff}schema_version = 1\n[speed]\nthreads = 4\n").unwrap();
    assert_eq!(l.config.speed.threads, 4);
    assert!(l.warnings.is_empty());

    // Fehler in der ModelError-Hülle behalten Zeile und Spalte im Text.
    let err = CrawlConfig::load_str("mode = 5").unwrap_err();
    let model: ModelError = err.into();
    assert!(model.to_string().contains("Zeile 1, Spalte 8"), "{model}");
}

// spec: CRAWL-132, OUT-220
#[test]
fn default_fixture_matches_written_defaults() {
    let expected = CrawlConfig::default().to_osconfig_string().unwrap();
    let path = fixture("config/default.osconfig");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&path, &expected).unwrap();
    }
    let on_disk = fs::read_to_string(&path).unwrap();
    // Die `generator`-Zeile trägt die Programmversion; der Golden-Vergleich gilt für alle übrigen Zeilen
    // und darf nicht bei jeder Versionsanhebung brechen. Die geschriebene Zeile ist die aktuelle Version.
    let current = format!("generator = \"openspider {}\"\n", env!("CARGO_PKG_VERSION"));
    assert!(expected.contains(&current), "{expected}");
    let golden_generator = on_disk
        .lines()
        .find(|l| l.starts_with("generator = "))
        .expect("generator-Zeile im Golden");
    assert_eq!(
        on_disk,
        expected.replacen(&current, &format!("{golden_generator}\n"), 1),
        "UPDATE_GOLDEN=1 schreibt die Datei neu"
    );
    // Alle Werte stehen drin (nicht nur Abweichungen) – und die Datei lädt zu den Standardwerten.
    assert!(on_disk.contains("threads = 5"));
    assert!(on_disk.contains("[spider.amp]"));
    assert!(on_disk.starts_with("schema_version = 2\ngenerator = \"openspider "));
    assert_eq!(
        load("config/default.osconfig").config,
        CrawlConfig {
            generator: "openspider 0.1.0".to_owned(),
            ..CrawlConfig::default()
        }
    );
}

// spec: OUT-220, CRAWL-132
#[test]
fn round_trip_of_all_valid_fixtures_is_byte_identical() {
    for rel in [
        "config/default.osconfig",
        "config/commented.osconfig",
        "config/partial.osconfig",
        "config/unknown-fields.osconfig",
        "config/future-version.osconfig",
        "crawl-config/minimal.osconfig",
        "crawl-config/profile.osconfig",
    ] {
        let first = load(rel).config.to_osconfig_string().unwrap();
        let reloaded = CrawlConfig::load_str(&first).unwrap();
        assert!(
            reloaded.warnings.is_empty(),
            "{rel}: {:?}",
            reloaded.warnings
        );
        let second = reloaded.config.to_osconfig_string().unwrap();
        assert_eq!(first, second, "{rel}");
        assert_eq!(
            reloaded.config.to_osconfig_string().unwrap(),
            first,
            "{rel}"
        );
    }
}

// spec: CRAWL-132
#[test]
fn profile_fixture_keeps_scope_rules_resolve_and_auth_reference() {
    let l = load("crawl-config/profile.osconfig");
    assert!(l.warnings.is_empty(), "{:?}", l.warnings);
    assert!(l.validate().is_ok());
    let c = &l.config;
    assert!(c.scope.all_subdomains);
    assert_eq!(
        c.scope.include,
        vec!["https://www\\.example\\.com/.*".to_string()]
    );
    assert_eq!(c.scope.exclude.len(), 2);
    assert_eq!(c.network.resolve[0], "*.site.test=127.0.0.1");
    assert_eq!(c.network.resolve[1], "staging.example.com=127.0.0.1:8080");
    assert_eq!(c.auth.basic[0].secret_ref, "profil-intern");
    assert_eq!(c.content.area.exclude_elements, ["nav", "footer", "aside"]);
    assert_eq!(c.robots.mode, RobotsMode::IgnoreButReport);
    assert_eq!(
        c.robots.unreachable_policy,
        RobotsUnreachablePolicy::AllowAll
    );
    assert_eq!(c.export.line_ending, LineEnding::Lf);
    assert!(!c.export.utf8_bom);
    assert!(!c.analysis.auto_at_end);
    assert_eq!(c.spider.images, StoreCrawl::STORE_ONLY);
}

// spec: OUT-220
#[test]
fn save_and_load_file_round_trip() {
    let dir = temp_dir("save");
    let path = dir.join("tiefer/unterordner/profil.osconfig"); // fehlende Ordner werden angelegt
    let mut cfg = load("crawl-config/profile.osconfig").config;
    cfg.speed.threads = 33;
    cfg.save_file(&path).unwrap();

    let bytes = fs::read_to_string(&path).unwrap();
    assert_eq!(bytes, cfg.to_osconfig_string().unwrap());
    let back = CrawlConfig::load_file(&path).unwrap();
    assert!(back.warnings.is_empty());
    // Speichern stempelt den Erzeuger des schreibenden Programms, alles andere bleibt gleich.
    assert_eq!(
        back.config,
        CrawlConfig {
            generator: CrawlConfig::default().generator,
            ..cfg.clone()
        }
    );
    assert_eq!(back.config.to_osconfig_string().unwrap(), bytes);

    // Überschreiben ersetzt die Datei vollständig, es bleibt keine temporäre Datei zurück.
    CrawlConfig::default().save_file(&path).unwrap();
    assert_eq!(
        CrawlConfig::load_file(&path).unwrap().config,
        CrawlConfig::default()
    );
    let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, ["profil.osconfig"]);
    let _ = fs::remove_dir_all(&dir);
}

// spec: OUT-220
#[test]
fn default_profile_save_load_and_reset() {
    let settings = temp_dir("default").join("settings"); // existiert noch nicht
                                                         // Ohne Datei: Werkseinstellungen.
    let none = CrawlConfig::load_default(&settings).unwrap();
    assert_eq!(none.config, CrawlConfig::default());
    assert!(none.warnings.is_empty());

    // „Als Standard speichern“ → nach „Neustart“ (frisches Laden) aktiv.
    let mut cfg = CrawlConfig::default();
    cfg.speed.threads = 9;
    cfg.scope.exclude.push(".*/privat/.*".into());
    cfg.save_default(&settings).unwrap();
    let path = CrawlConfig::default_profile_path(&settings);
    assert!(path.ends_with("default.osconfig"));
    assert!(path.exists());
    assert_eq!(CrawlConfig::load_default(&settings).unwrap().config, cfg);

    // „Standard zurücksetzen“ löscht die Datei und stellt die Werkseinstellungen wieder her.
    assert!(CrawlConfig::reset_default(&settings).unwrap());
    assert!(!path.exists());
    assert_eq!(
        CrawlConfig::load_default(&settings).unwrap().config,
        CrawlConfig::default()
    );
    assert!(!CrawlConfig::reset_default(&settings).unwrap()); // nichts mehr zu löschen

    // Ein kaputtes Standard-Profil ist ein Fehler mit Pfad, kein stilles Zurückfallen.
    fs::create_dir_all(&settings).unwrap();
    fs::write(&path, "[speed\n").unwrap();
    let err = CrawlConfig::load_default(&settings).unwrap_err();
    assert!(err.file.as_ref().unwrap().ends_with("default.osconfig"));
    let _ = fs::remove_dir_all(settings.parent().unwrap());
}

/// Baut aus Zufallsbausteinen eine Konfiguration, in der jedes Feld einen (meist abweichenden) Wert hat.
#[allow(clippy::too_many_arguments)]
fn random_config(
    bits: Vec<bool>,
    nums: Vec<u32>,
    bigs: Vec<u64>,
    floats: Vec<f64>,
    picks: Vec<u8>,
    strs: Vec<String>,
    lens: Vec<u8>,
) -> CrawlConfig {
    let mut b = bits.into_iter().cycle();
    let mut n = nums.into_iter().cycle();
    let mut g = bigs.into_iter().cycle();
    let mut f = floats.into_iter().cycle();
    let mut p = picks.into_iter().cycle();
    let mut s = strs.into_iter().cycle();
    let mut l = lens.into_iter().cycle();

    macro_rules! b {
        () => {
            b.next().unwrap()
        };
    }
    macro_rules! n {
        () => {
            n.next().unwrap()
        };
    }
    macro_rules! g {
        () => {
            g.next().unwrap() & (i64::MAX as u64)
        };
    }
    macro_rules! s {
        () => {
            s.next().unwrap()
        };
    }
    macro_rules! opt {
        ($e:expr) => {
            if b!() {
                Some($e)
            } else {
                None
            }
        };
    }
    macro_rules! list {
        () => {{
            let len = (l.next().unwrap() % 4) as usize;
            (0..len).map(|_| s!()).collect::<Vec<String>>()
        }};
    }
    macro_rules! sc {
        () => {
            StoreCrawl {
                store: b!(),
                crawl: b!(),
            }
        };
    }
    macro_rules! pick {
        ($($v:expr),+) => {{
            let all = [$($v),+];
            all[(p.next().unwrap() as usize) % all.len()]
        }};
    }

    CrawlConfig {
        schema_version: 2,
        generator: s!(),
        mode: pick!(CrawlMode::Spider, CrawlMode::List),
        scope: ScopeConfig {
            all_subdomains: b!(),
            subfolder_only: b!(),
            crawl_outside_start_folder: b!(),
            regex_partial_match: b!(),
            include: list!(),
            exclude: list!(),
        },
        spider: SpiderConfig {
            css_style_attributes: b!(),
            images: sc!(),
            css: sc!(),
            javascript: sc!(),
            media: sc!(),
            internal: sc!(),
            external: sc!(),
            canonicals: sc!(),
            hreflang: sc!(),
            pagination: sc!(),
            meta_refresh: sc!(),
            iframes: sc!(),
            amp: sc!(),
            mobile_alternate: sc!(),
            follow_internal_nofollow: b!(),
            follow_external_nofollow: b!(),
            ..Default::default()
        },
        limits: LimitsConfig {
            max_urls: g!(),
            max_depth: opt!(n!()),
            max_redirects: n!(),
            max_page_kb: g!(),
            max_folder_depth: opt!(n!()),
            max_query_params: opt!(n!()),
            max_urls_per_subdomain: opt!(g!()),
            max_url_length: n!(),
            max_links_per_url: n!(),
        },
        speed: SpeedConfig {
            threads: n!(),
            max_per_host: opt!(n!()),
            max_urls_per_sec: opt!(f.next().unwrap()),
            ..Default::default()
        },
        user_agent: UserAgentConfig {
            preset: s!(),
            http: opt!(s!()),
            robots_token: opt!(s!()),
        },
        robots: RobotsConfig {
            mode: pick!(
                RobotsMode::Respect,
                RobotsMode::Ignore,
                RobotsMode::IgnoreButReport
            ),
            unreachable_policy: pick!(
                RobotsUnreachablePolicy::BlockHost,
                RobotsUnreachablePolicy::AllowAll
            ),
            retries: n!(),
            retry_delay_s: n!(),
            ..Default::default()
        },
        advanced: AdvancedConfig {
            cookies: pick!(
                CookieMode::SessionOnly,
                CookieMode::Persistent,
                CookieMode::None
            ),
            response_timeout_s: n!(),
            assume_html: b!(),
            respect_self_ref_meta_refresh: b!(),
            crawl_fragments: b!(),
            respect_noindex: b!(),
            respect_canonical: b!(),
            html_validation: b!(),
            green_hosting: b!(),
            ..Default::default()
        },
        auth: AuthConfig {
            basic: (0..(l.next().unwrap() % 3))
                .map(|_| BasicAuth {
                    prefix: s!(),
                    username: s!(),
                    secret_ref: s!(),
                })
                .collect(),
            form: (0..(l.next().unwrap() % 3))
                .map(|_| FormLogin {
                    login_url: s!(),
                    profile: opt!(s!()),
                    captured_at: opt!(s!()),
                    cookie_count: n!(),
                })
                .collect(),
            session_guard: list!(),
            profile_refs: list!(),
        },
        network: NetworkConfig { resolve: list!() },
        rendering: RenderingConfig {
            mode: pick!(RenderingMode::Text, RenderingMode::JavaScript),
            ajax_timeout_s: f64::from(n!()),
            js_error_reporting: b!(),
            viewport: ViewportConfig {
                preset: s!(),
                width: n!(),
                height: n!(),
                scale_factor: f.next().unwrap(),
                mobile: b!(),
                touch: b!(),
            },
            screenshots: ScreenshotConfig {
                enabled: b!(),
                width: opt!(n!()),
                height: opt!(n!()),
                fit_to_content: b!(),
                wait_ms: n!(),
                format: pick!(ScreenshotFormat::Png, ScreenshotFormat::Jpeg),
                jpeg_quality: n!() as u8,
            },
            tabs: opt!(n!()),
            restart_after_pages: n!(),
            restart_memory_mb: n!(),
            page_timeout_s: n!(),
            chromium_path: opt!(s!()),
        },
        extraction: ExtractionConfig {
            page_titles: b!(),
            meta_description: b!(),
            meta_keywords: b!(),
            h1: b!(),
            h2: b!(),
            hash: b!(),
            response_time: b!(),
            last_modified: b!(),
            http_headers: b!(),
            store_html: b!(),
            store_rendered_html: b!(),
            indexability: b!(),
            word_count: b!(),
            readability: b!(),
            text_ratio: b!(),
            page_size: b!(),
            forms: b!(),
            accessibility: b!(),
            cookies: b!(),
            meta_robots: b!(),
            x_robots_tag: b!(),
            srcset: b!(),
            structured_data: StructuredDataConfig {
                json_ld: b!(),
                microdata: b!(),
                rdfa: b!(),
                schema_org_validation: b!(),
                rich_results_validation: b!(),
                case_sensitive: b!(),
            },
        },
        content: ContentConfig {
            area: ContentAreaConfig {
                include_elements: list!(),
                include_classes: list!(),
                include_ids: list!(),
                exclude_elements: list!(),
                exclude_classes: list!(),
                exclude_ids: list!(),
            },
            low_word_count: n!(),
            soft404_phrases: list!(),
            readability: ReadabilityConfig {
                language_aware: b!(),
            },
        },
        title: TextLimits {
            max_chars: n!(),
            min_chars: n!(),
            max_px: n!(),
            min_px: n!(),
        },
        meta_description: TextLimits {
            max_chars: n!(),
            min_chars: n!(),
            max_px: n!(),
            min_px: n!(),
        },
        url: UrlLimits {
            max_chars: n!(),
            ..Default::default()
        },
        h1: HeadingLimits { max_chars: n!() },
        h2: HeadingLimits { max_chars: n!() },
        image: ImageLimits {
            max_kb: n!(),
            alt_max_chars: n!(),
        },
        duplicates: DuplicatesConfig {
            only_indexable: b!(),
            ignore_paginated: b!(),
            exact_enabled: b!(),
            near_enabled: b!(),
            near_threshold_pct: n!() as u8,
        },
        issues: IssuesConfig {
            ignore_non_indexable: b!(),
        },
        pdf: PdfConfig {
            store: b!(),
            extract_properties: b!(),
            extract_link_text: b!(),
        },
        links: LinksConfig {
            high_crawl_depth: n!(),
            high_internal_outlinks: n!(),
            high_external_outlinks: n!(),
            non_descriptive_anchors: list!(),
        },
        hreflang: HreflangConfig {
            over_limit: pick!(HreflangOverLimit::Truncate, HreflangOverLimit::Skip),
            allow_script_subtag: b!(),
        },
        sitemaps: SitemapsConfig {
            crawl_linked: b!(),
            auto_discover_robots: b!(),
            urls: list!(),
        },
        analysis: AnalysisConfig {
            auto_at_end: b!(),
            link_score: b!(),
            canonicals: b!(),
            pagination: b!(),
            hreflang: b!(),
            links: b!(),
            sitemaps: b!(),
            content: b!(),
            images: b!(),
            analytics: b!(),
            search_console: b!(),
            link_metrics: b!(),
            amp: b!(),
            near_duplicates: b!(),
        },
        validation: ValidationConfig {
            html_doc_max_bytes: g!(),
            resource_max_bytes: g!(),
        },
        export: ExportConfig {
            utf8_bom: b!(),
            line_ending: pick!(LineEnding::Crlf, LineEnding::Lf),
            ..Default::default()
        },
        spelling: SpellingConfig {
            enabled: b!(),
            mode: pick!(SpellingMode::Auto, SpellingMode::Manual),
            manual_language: s!(),
            default_variants: (0..(l.next().unwrap() % 3)).map(|_| (s!(), s!())).collect(),
            default_language: s!(),
        },
        grammar: GrammarConfig {
            enabled: b!(),
            disabled_rules: list!(),
        },
        custom_search: CustomSearchConfig {
            filters: (0..(l.next().unwrap() % 3))
                .map(|_| SearchFilter {
                    name: s!(),
                    contains: b!(),
                    kind: pick!(SearchKind::Text, SearchKind::Regex),
                    expression: s!(),
                    scope: pick!(
                        SearchScope::Html,
                        SearchScope::XPath,
                        SearchScope::ContentArea
                    ),
                    xpath: opt!(s!()),
                    case_sensitive: b!(),
                })
                .collect(),
            ..Default::default()
        },
        custom_extraction: CustomExtractionConfig {
            extractors: (0..(l.next().unwrap() % 3))
                .map(|_| Extractor {
                    name: s!(),
                    mode: pick!(
                        ExtractorMode::XPath,
                        ExtractorMode::Css,
                        ExtractorMode::Regex
                    ),
                    expression: s!(),
                    attribute: opt!(s!()),
                    output: pick!(ExtractorOutput::Text, ExtractorOutput::InnerHtml),
                })
                .collect(),
        },
        custom_js: CustomJsConfig {
            snippets: (0..(l.next().unwrap() % 3))
                .map(|_| JsSnippet {
                    name: s!(),
                    kind: pick!(JsSnippetKind::Extraction, JsSnippetKind::Action),
                    content_types: list!(),
                    timeout_s: n!(),
                    code: s!(),
                })
                .collect(),
            ..Default::default()
        },
        link_positions: LinkPositionsConfig {
            store: b!(),
            rules: (0..(l.next().unwrap() % 3))
                .map(|_| LinkPositionRule {
                    position: s!(),
                    contains: list!(),
                })
                .collect(),
        },
        rewrite: RewriteConfig {
            remove_parameters: list!(),
            remove_all_parameters: b!(),
            regex_replace: (0..(l.next().unwrap() % 3))
                .map(|_| RegexReplace {
                    pattern: s!(),
                    replacement: s!(),
                })
                .collect(),
        },
        cdns: list!(),
        http: HttpConfig {
            headers: (0..(l.next().unwrap() % 3))
                .map(|_| HeaderEntry {
                    name: s!(),
                    value: s!(),
                    host: opt!(s!()),
                })
                .collect(),
            accept_language: opt!(s!()),
            version: pick!(
                HttpVersion::Auto,
                HttpVersion::Http1,
                HttpVersion::Http2Preferred
            ),
            retries_5xx: n!(),
            retries_network: n!(),
            throttle_on_429: b!(),
            respect_hsts: b!(),
            ignore_certificate_errors: b!(),
        },
        api: ApiConfig {
            ga4: Ga4Config {
                enabled: b!(),
                base_url: opt!(s!()),
                property: opt!(s!()),
                date_range: DateRangeConfig {
                    preset: pick!(DateRangePreset::Last7Days, DateRangePreset::Custom),
                    from: opt!(s!()),
                    to: opt!(s!()),
                },
                metrics: list!(),
                page_dimension: opt!(s!()),
                filter: opt!(s!()),
                ..Default::default()
            },
            gsc: GscConfig {
                enabled: b!(),
                base_url: opt!(s!()),
                site: opt!(s!()),
                date_range: DateRangeConfig {
                    preset: pick!(DateRangePreset::Last28Days, DateRangePreset::Last90Days),
                    from: opt!(s!()),
                    to: opt!(s!()),
                },
                search_type: pick!(SearchType::Web, SearchType::Image, SearchType::Discover),
                filter: opt!(s!()),
                url_inspection: b!(),
                ..Default::default()
            },
            psi: PsiConfig {
                enabled: b!(),
                base_url: opt!(s!()),
                device: pick!(PsiDevice::Mobile, PsiDevice::Desktop),
                local: b!(),
                metric_groups: list!(),
                ..Default::default()
            },
            majestic: LinkMetricsConfig {
                enabled: b!(),
                base_url: opt!(s!()),
                levels: vec![pick!(ApiLevel::Url, ApiLevel::Subdomain, ApiLevel::Domain)],
                metrics: list!(),
                ..Default::default()
            },
            ahrefs: LinkMetricsConfig {
                enabled: b!(),
                base_url: opt!(s!()),
                levels: vec![],
                metrics: list!(),
                ..Default::default()
            },
            moz: LinkMetricsConfig {
                enabled: b!(),
                base_url: opt!(s!()),
                levels: vec![ApiLevel::Domain, ApiLevel::Url],
                metrics: list!(),
                ..Default::default()
            },
            drive: DriveConfig {
                enabled: b!(),
                base_url: opt!(s!()),
                root_folder: opt!(s!()),
                ..Default::default()
            },
        },
        segments: SegmentsConfig {
            list: (0..(l.next().unwrap() % 3))
                .map(|_| SegmentConfig {
                    name: s!(),
                    color: s!(),
                    groups: (0..(l.next().unwrap() % 3))
                        .map(|_| SegmentGroup {
                            conditions: (0..(l.next().unwrap() % 3))
                                .map(|_| SegmentCondition {
                                    field: s!(),
                                    op: pick!(
                                        SegmentOp::Contains,
                                        SegmentOp::InSegment,
                                        SegmentOp::IsEmpty
                                    ),
                                    value: s!(),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        },
        compare: CompareConfig {
            mapping: (0..(l.next().unwrap() % 3))
                .map(|_| MappingRule {
                    pattern: s!(),
                    replacement: s!(),
                })
                .collect(),
            change_detection: ChangeDetectionConfig {
                elements: (0..(l.next().unwrap() % 4))
                    .map(|_| {
                        pick!(
                            ChangeElement::Indexability,
                            ChangeElement::H1,
                            ChangeElement::Content
                        )
                    })
                    .collect(),
                content_threshold_pct: n!() as u8,
            },
        },
        sitemap_gen: SitemapGenConfig {
            include_noindex: b!(),
            include_canonicalised: b!(),
            include_paginated: b!(),
            include_pdfs: b!(),
            include_3xx: b!(),
            include_4xx: b!(),
            include_5xx: b!(),
            include_no_response: b!(),
            exclude_patterns: list!(),
            lastmod: SitemapLastmod {
                enabled: b!(),
                source: pick!(LastmodSource::Server, LastmodSource::Custom),
                custom_date: opt!(s!()),
            },
            priority: SitemapPriority {
                enabled: b!(),
                by_depth: (0..(l.next().unwrap() % 4))
                    .map(|_| f.next().unwrap())
                    .collect(),
            },
            changefreq: SitemapChangefreq {
                enabled: b!(),
                basis: pick!(ChangeFreqBasis::LastModified, ChangeFreqBasis::Depth),
                by_depth: (0..(l.next().unwrap() % 4))
                    .map(|_| pick!(ChangeFreq::Daily, ChangeFreq::Never, ChangeFreq::Yearly))
                    .collect(),
            },
            images: SitemapImages {
                enabled: b!(),
                external_regex: list!(),
                max_inlinks: opt!(n!()),
                exclude_noindex: b!(),
            },
            hreflang: SitemapHreflang {
                enabled: b!(),
                only_valid_targets: b!(),
            },
        },
        ..Default::default()
    }
}

proptest! {
    // spec: OUT-220, CRAWL-132
    #[test]
    fn random_configs_round_trip_byte_identically(
        bits in proptest::collection::vec(any::<bool>(), 1..64),
        nums in proptest::collection::vec(any::<u32>(), 1..48),
        bigs in proptest::collection::vec(any::<u64>(), 1..8),
        floats in proptest::collection::vec(
            proptest::num::f64::NORMAL | proptest::num::f64::SUBNORMAL | proptest::num::f64::ZERO, 1..4),
        picks in proptest::collection::vec(any::<u8>(), 1..16),
        strs in proptest::collection::vec(any::<String>(), 1..16),
        lens in proptest::collection::vec(any::<u8>(), 1..16),
    ) {
        let cfg = random_config(bits, nums, bigs, floats, picks, strs, lens);
        let text = cfg.to_osconfig_string().unwrap();
        let loaded = CrawlConfig::load_str(&text).unwrap();
        prop_assert!(loaded.warnings.is_empty(), "{:?}\n{}", loaded.warnings, text);
        prop_assert_eq!(&loaded.config, &CrawlConfig { generator: loaded.config.generator.clone(), ..cfg.clone() });
        // byte-identisch: Schreiben → Laden → Schreiben
        let again = loaded.config.to_osconfig_string().unwrap();
        prop_assert_eq!(&again, &text);
        // und die Datei auf der Platte verhält sich genauso
        prop_assert!(text.starts_with("schema_version = 2\ngenerator = "));
        prop_assert!(!text.to_lowercase().contains("password"));
    }
}

//! Gemeinsamer Testhelfer: setzt jedes Feld, das die Konfiguration v2 ergänzt, auf einen Nicht-Standardwert.
#![allow(dead_code)]

use os_model::*;

/// Verändert alle in v2 neuen Felder (und nur diese) gegenüber den Standardwerten.
pub fn change_all_v2_fields(c: &mut CrawlConfig) {
    c.serp.desktop.title_max_px = Some(500);
    c.serp.desktop.description_max_px = Some(800);
    c.serp.mobile.title_max_px = Some(510);
    c.serp.mobile.description_max_px = Some(810);
    c.serp.tablet.title_max_px = Some(520);
    c.serp.tablet.description_max_px = Some(820);
    for device in [&mut c.serp.desktop, &mut c.serp.mobile, &mut c.serp.tablet] {
        device.title_font_px = 21.0;
        device.description_font_px = 15.0;
    }
    c.ui.parity_filters = false;
    c.ui.columns
        .insert("Internal".into(), vec!["Address".into()]);
    c.images.real_dimensions_from_header = true;
    c.images.parse_inline_css_backgrounds = true;
    c.images.incorrect_size_min_saving_kb = 5;
    c.security.obsolete_mime_types = vec!["application/test".into()];
    c.advanced.always_follow_redirects = true;
    c.advanced.always_follow_canonicals = true;
    c.custom_search.regex_timeout_ms = 3000;

    c.limits.max_folder_depth = Some(6);
    c.limits.max_query_params = Some(3);
    c.limits.max_urls_per_subdomain = Some(1000);
    c.limits.max_url_length = 2000;
    c.limits.max_links_per_url = 500;

    c.advanced.crawl_fragments = true;
    c.advanced.respect_noindex = true;
    c.advanced.respect_canonical = true;
    c.advanced.html_validation = false;
    c.advanced.green_hosting = true;

    c.auth.form = vec![FormLogin {
        login_url: "https://a.test/login".into(),
        profile: Some("shop".into()),
        captured_at: Some("2026-10-16T08:00:00Z".into()),
        cookie_count: 4,
    }];
    c.auth.session_guard = vec!["/logout".into()];
    c.auth.profile_refs = vec!["shop".into()];

    c.rendering.ajax_timeout_s = 12.0;
    c.rendering.js_error_reporting = false;
    c.rendering.viewport = ViewportConfig {
        preset: "custom".into(),
        width: 1280,
        height: 800,
        scale_factor: 2.0,
        mobile: false,
        touch: false,
    };
    c.rendering.screenshots = ScreenshotConfig {
        enabled: false,
        width: Some(1000),
        height: Some(2000),
        fit_to_content: true,
        wait_ms: 900,
        format: ScreenshotFormat::Jpeg,
        jpeg_quality: 70,
    };
    c.rendering.tabs = Some(3);
    c.rendering.restart_after_pages = 100;
    c.rendering.restart_memory_mb = 900;
    c.rendering.page_timeout_s = 30;
    c.rendering.chromium_path = Some("/opt/chromium/chrome".into());

    c.extraction.store_rendered_html = true;
    c.extraction.indexability = false;
    c.extraction.word_count = false;
    c.extraction.readability = false;
    c.extraction.text_ratio = false;
    c.extraction.page_size = false;
    c.extraction.forms = false;
    c.extraction.accessibility = true;
    c.extraction.cookies = true;
    c.extraction.meta_robots = false;
    c.extraction.x_robots_tag = false;
    c.extraction.srcset = true;
    c.extraction.structured_data = StructuredDataConfig {
        json_ld: true,
        microdata: true,
        rdfa: true,
        schema_org_validation: false,
        rich_results_validation: false,
        case_sensitive: true,
    };

    c.content.readability.language_aware = true;
    c.pdf.extract_properties = true;
    c.pdf.extract_link_text = true;
    c.analysis.near_duplicates = false;

    c.spelling = SpellingConfig {
        enabled: true,
        mode: SpellingMode::Manual,
        manual_language: "de-DE".into(),
        default_variants: [("de".to_string(), "de-AT".to_string())]
            .into_iter()
            .collect(),
        default_language: "de".into(),
    };
    c.grammar = GrammarConfig {
        enabled: true,
        disabled_rules: vec!["R1".into()],
    };
    c.custom_search.filters = vec![SearchFilter {
        name: "GTM".into(),
        contains: false,
        kind: SearchKind::Regex,
        expression: "GTM-[A-Z0-9]+".into(),
        scope: SearchScope::XPath,
        xpath: Some("//script".into()),
        case_sensitive: true,
    }];
    c.custom_extraction.extractors = vec![Extractor {
        name: "Preis".into(),
        mode: ExtractorMode::Css,
        expression: "span.price".into(),
        attribute: Some("content".into()),
        output: ExtractorOutput::InnerHtml,
    }];
    c.custom_js.output_dir = Some(std::path::PathBuf::from("snippet-output"));
    c.custom_js.snippets = vec![JsSnippet {
        name: "Titel".into(),
        kind: JsSnippetKind::Action,
        content_types: vec!["text/html".into(), "application/xhtml+xml".into()],
        timeout_s: 20,
        code: "return document.title;".into(),
    }];
    c.link_positions.store = false;
    c.link_positions.rules = vec![
        LinkPositionRule {
            position: "Breadcrumb".into(),
            contains: vec!["breadcrumb".into()],
        },
        LinkPositionRule {
            position: "Content".into(),
            contains: vec![],
        },
    ];
    c.rewrite = RewriteConfig {
        remove_parameters: vec!["sid".into(), "utm_source".into()],
        remove_all_parameters: true,
        regex_replace: vec![RegexReplace {
            pattern: "/index\\.html$".into(),
            replacement: "/".into(),
        }],
    };
    c.cdns = vec!["cdn.other.test".into(), "other.test/assets/".into()];
    c.http = HttpConfig {
        headers: vec![HeaderEntry {
            name: "X-Test".into(),
            value: "1".into(),
            host: Some("a.test".into()),
        }],
        accept_language: Some("de-DE,de;q=0.9".into()),
        version: HttpVersion::Http2Preferred,
        retries_5xx: 2,
        retries_network: 3,
        throttle_on_429: false,
        respect_hsts: false,
        ignore_certificate_errors: true,
    };
    c.api = ApiConfig {
        ga4: Ga4Config {
            analytics_account: Some("accounts/123".into()),
            crawl_new_urls: true,
            enabled: true,
            base_url: Some("http://127.0.0.1:9/ga4".into()),
            property: Some("properties/123".into()),
            date_range: DateRangeConfig {
                preset: DateRangePreset::Custom,
                from: Some("2026-01-01".into()),
                to: Some("2026-01-31".into()),
            },
            metrics: vec!["sessions".into()],
            page_dimension: Some("landingPage".into()),
            filter: Some("country==DE".into()),
            ..Default::default()
        },
        gsc: GscConfig {
            crawl_new_urls: true,
            enabled: true,
            base_url: Some("http://127.0.0.1:9/gsc".into()),
            site: Some("sc-domain:a.test".into()),
            date_range: DateRangeConfig {
                preset: DateRangePreset::Last90Days,
                from: Some("2026-02-01".into()),
                to: Some("2026-02-28".into()),
            },
            search_type: SearchType::Image,
            filter: Some("device==MOBILE".into()),
            url_inspection: true,
            ..Default::default()
        },
        psi: PsiConfig {
            enabled: true,
            base_url: Some("http://127.0.0.1:9/psi".into()),
            device: PsiDevice::Desktop,
            local: true,
            metric_groups: vec!["crux".into(), "lab".into()],
            ..Default::default()
        },
        majestic: LinkMetricsConfig {
            enabled: true,
            base_url: Some("http://127.0.0.1:9/majestic".into()),
            levels: vec![ApiLevel::Domain],
            metrics: vec!["TrustFlow".into()],
            ..Default::default()
        },
        ahrefs: LinkMetricsConfig {
            enabled: true,
            base_url: Some("http://127.0.0.1:9/ahrefs".into()),
            levels: vec![ApiLevel::Url, ApiLevel::Subdomain],
            metrics: vec!["refdomains".into()],
            ..Default::default()
        },
        moz: LinkMetricsConfig {
            enabled: true,
            base_url: Some("http://127.0.0.1:9/moz".into()),
            levels: vec![ApiLevel::Subdomain],
            metrics: vec!["da".into()],
            ..Default::default()
        },
        drive: DriveConfig {
            oauth_client_id: Some("fixture-client".into()),
            enabled: true,
            base_url: Some("http://127.0.0.1:9/drive".into()),
            root_folder: Some("Crawls".into()),
            ..Default::default()
        },
    };
    c.segments.list = vec![SegmentConfig {
        name: "Shop".into(),
        color: "#ff8800".into(),
        groups: vec![SegmentGroup {
            conditions: vec![SegmentCondition {
                field: "Address".into(),
                op: SegmentOp::Contains,
                value: "/produkt/".into(),
            }],
        }],
    }];
    c.compare = CompareConfig {
        mapping: vec![MappingRule {
            pattern: "^https://staging\\.".into(),
            replacement: "https://www.".into(),
        }],
        change_detection: ChangeDetectionConfig {
            elements: vec![ChangeElement::Indexability, ChangeElement::Content],
            content_threshold_pct: 25,
        },
    };
    c.sitemap_gen = SitemapGenConfig {
        include_noindex: true,
        include_canonicalised: true,
        include_paginated: true,
        include_pdfs: true,
        include_3xx: true,
        include_4xx: true,
        include_5xx: true,
        include_no_response: true,
        exclude_patterns: vec!["/intern/".into()],
        lastmod: SitemapLastmod {
            enabled: false,
            source: LastmodSource::Custom,
            custom_date: Some("2026-10-16".into()),
        },
        priority: SitemapPriority {
            enabled: false,
            by_depth: vec![0.5, 0.4],
        },
        changefreq: SitemapChangefreq {
            enabled: false,
            basis: ChangeFreqBasis::Depth,
            by_depth: vec![ChangeFreq::Hourly],
        },
        images: SitemapImages {
            enabled: true,
            external_regex: vec!["cdn\\.".into()],
            max_inlinks: Some(5),
            exclude_noindex: true,
        },
        hreflang: SitemapHreflang {
            enabled: true,
            only_valid_targets: false,
        },
    };
    c.api.ga4.account = Some("fixture".into());
    c.api.ga4.limits = ApiLimitsConfig {
        retries: 3,
        concurrency: 2,
        requests_per_window: Some(10),
        window_s: 30,
        quota_limit: Some(100),
    };
    c.api.gsc.account = Some("fixture".into());
    c.api.gsc.limits = ApiLimitsConfig {
        retries: 3,
        concurrency: 2,
        requests_per_window: Some(10),
        window_s: 30,
        quota_limit: Some(100),
    };
    c.api.psi.account = Some("fixture".into());
    c.api.psi.limits = ApiLimitsConfig {
        retries: 3,
        concurrency: 2,
        requests_per_window: Some(10),
        window_s: 30,
        quota_limit: Some(100),
    };
    c.api.majestic.account = Some("fixture".into());
    c.api.majestic.limits = ApiLimitsConfig {
        retries: 3,
        concurrency: 2,
        requests_per_window: Some(10),
        window_s: 30,
        quota_limit: Some(100),
    };
    c.api.ahrefs.account = Some("fixture".into());
    c.api.ahrefs.limits = ApiLimitsConfig {
        retries: 3,
        concurrency: 2,
        requests_per_window: Some(10),
        window_s: 30,
        quota_limit: Some(100),
    };
    c.api.moz.account = Some("fixture".into());
    c.api.moz.limits = ApiLimitsConfig {
        retries: 3,
        concurrency: 2,
        requests_per_window: Some(10),
        window_s: 30,
        quota_limit: Some(100),
    };
    c.api.drive.account = Some("fixture".into());
    c.api.drive.limits = ApiLimitsConfig {
        retries: 3,
        concurrency: 2,
        requests_per_window: Some(10),
        window_s: 30,
        quota_limit: Some(100),
    };
    c.api.ga4.oauth_client_id = Some("fixture".into());
    c.api.ga4.url_match = UrlMatchConfig {
        ignore_trailing_slash: false,
        ignore_case: false,
    };
    c.api.gsc.oauth_client_id = Some("fixture".into());
    c.api.gsc.url_match = UrlMatchConfig {
        ignore_trailing_slash: false,
        ignore_case: false,
    };
    // INTEG-152/153/154/157: additive, nicht geheime Linkmetrik-Optionen.
    for source in [&mut c.api.majestic, &mut c.api.ahrefs, &mut c.api.moz] {
        source.options = LinkMetricsOptions {
            historic: true,
            include_external: true,
            oauth_client_id: Some("fixture-client".into()),
            workspace: Some("fixture-workspace".into()),
            country: Some("de".into()),
            average_volume: true,
            moz_paid: true,
            moz_free_interval_s: 20,
        };
    }
}

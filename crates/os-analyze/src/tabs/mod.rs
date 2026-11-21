//! Die Tabs in fester UI-Reihenfolge (UI-004). M1 liefert 16 Tabs, M2 ergänzt 15 neue Tabs und neue Filter der
//! bestehenden Tabs (`docs/plan/M2-filters.toml`). Jede Datei bleibt einem Paket zugeordnet und ersetzt ihre Definition
//! ohne Kennungen zu ändern (`tests/filters_registry.rs`).
//!
//! **Stubs (M2-F02):** Alle Tabs und Filter aus `M1-filters.toml` und `M2-filters.toml` sind registriert. Was ein
//! Fachpaket noch nicht umgesetzt hat, ist ein Stub: ein neuer Tab ist ein leerer Platzhalter ([`stub_tab`]: Spalte
//! `Address`, Grundmenge `0`), ein neuer Filter hat das Prädikat `0` ([`FilterDef::is_stub`]). `all()` hängt die Stub-Filter
//! der Tabelle [`M2_FILTERS`] hinter die Filter, die das Tab-Modul selbst liefert; ein Filter, den das Modul unter
//! derselben Kennung (Groß-/Kleinschreibung egal) selbst definiert, hat Vorrang. Ein Fachpaket ersetzt also einen Stub,
//! indem es den Filter in seinem Tab-Modul definiert; die Registry muss es nicht ändern. Berichte über Daten (Crawl
//! Overview) überspringen Stubs, damit sie nur Befüllbares zeigen.

pub mod accessibility;
pub mod ai;
pub mod ai_decisions;
pub mod ai_search;
pub mod ai_suggestions;
pub mod analytics;
pub mod canonicals;
pub mod change_detection;
pub mod content;
pub mod custom_extraction;
pub mod custom_js;
pub mod custom_search;
pub mod directives;
pub mod duplicates;
pub mod dynamic_api;
pub mod external;
pub mod h1;
pub mod h2;
pub mod hreflang;
pub mod images;
pub mod internal;
pub mod internal_crawl;
pub mod javascript;
pub mod link_metrics;
pub mod links;
pub mod meta_description;
pub mod meta_keywords;
pub mod mobile;
pub mod page_titles;
pub mod pagespeed;
pub mod pagination;
pub mod response_codes;
pub mod search_console;
pub mod security;
pub mod sitemaps;
pub mod structured_data;
pub mod url;
pub mod validation;

use os_model::AnalysisStep;

use crate::defs::{
    apply_meta_defaults, ColumnDef, ColumnSource, FilterDef, FilterKind, Requirement, SortKey,
    TabDef,
};

/// Alle Tabs in UI-Reihenfolge (UI-004): internal, external, security, response_codes, url, page_titles,
/// meta_description, meta_keywords, h1, h2, content, images, canonicals, pagination, directives, hreflang, javascript,
/// links, structured_data, sitemaps, pagespeed, mobile, accessibility, custom_search, custom_extraction, custom_js,
/// analytics, search_console, validation, link_metrics, change_detection (nur im Vergleichsmodus sichtbar, UI-004).
pub fn all() -> Vec<TabDef> {
    let mut tabs = vec![
        internal::tab(),
        external::tab(),
        security::tab(),
        response_codes::tab(),
        url::tab(),
        page_titles::tab(),
        meta_description::tab(),
        meta_keywords::tab(),
        h1::tab(),
        h2::tab(),
        content::tab(),
        images::tab(),
        canonicals::tab(),
        pagination::tab(),
        directives::tab(),
        hreflang::tab(),
        javascript::tab(),
        links::tab(),
        structured_data::tab(),
        sitemaps::tab(),
        pagespeed::tab(),
        mobile::tab(),
        accessibility::tab(),
        custom_search::tab(),
        custom_extraction::tab(),
        custom_js::tab(),
        analytics::tab(),
        search_console::tab(),
        validation::tab(),
        link_metrics::tab(),
        ai::tab(),
        ai_decisions::tab(),
        ai_suggestions::tab(),
        ai_search::tab(),
        change_detection::tab(),
    ];
    for tab in &mut tabs {
        add_stub_filters(tab);
        if tab.id == "Response Codes" {
            response_codes::configure_filters(tab);
        }
        apply_meta_defaults(&mut tab.columns);
    }
    tabs
}

/// Leerer Platzhalter für einen Tab, den ein M2-Paket noch nicht umgesetzt hat: Spalte `Address`, Grundmenge `0`
/// (keine Zeilen), keine eigenen Filter (`all()` ergänzt `All` und die übrigen Filter als Stubs).
pub fn stub_tab(id: &'static str) -> TabDef {
    let mut tab = crate::defs::placeholder_tab(id, Vec::new());
    tab.population = "0";
    tab
}

/// Element-Tabs, deren Problemfilter `issues.ignore_non_indexable` beachten (CRAWL-089, ONPAGE-009).
const ELEMENT_TABS: [&str; 5] = [
    "Page Titles",
    "Meta Description",
    "Meta Keywords",
    "H1",
    "H2",
];

/// Hängt die Stub-Filter aus [`M2_FILTERS`] an, die der Tab noch nicht selbst definiert.
fn add_stub_filters(tab: &mut TabDef) {
    let known = |tab: &TabDef, id: &str| tab.filters.iter().any(|f| f.id.eq_ignore_ascii_case(id));
    for (tab_id, filter_id, step, config, rendering) in M2_FILTERS {
        if *tab_id != tab.id || known(tab, filter_id) {
            continue;
        }
        let kind = step.map_or(FilterKind::Live, FilterKind::PostCrawl);
        let mut filter = FilterDef::placeholder(filter_id, kind, config);
        if *rendering {
            filter.requires.push(Requirement::Rendering);
        }
        // CRAWL-089: Problemfilter (alle außer `All`) der Element-Tabs ignorieren nicht indexierbare URLs.
        filter.ignore_non_indexable =
            ELEMENT_TABS.contains(&tab.id) && !filter_id.eq_ignore_ascii_case("All");
        filter.requires = filter
            .requires
            .into_iter()
            .map(|r| match r {
                Requirement::Config("extraction.structured_data") => Requirement::AnyConfig(&[
                    "extraction.structured_data.json_ld",
                    "extraction.structured_data.microdata",
                    "extraction.structured_data.rdfa",
                ]),
                Requirement::Config("advanced.html_validation") => Requirement::Coverage {
                    config: "advanced.html_validation",
                    marker: "validation.checked",
                },
                Requirement::Config("images.parse_inline_css_backgrounds") => {
                    Requirement::RenderingOrConfig("images.parse_inline_css_backgrounds")
                }
                other => other,
            })
            .collect();
        tab.filters.push(filter);
    }
    // „All“ eines Stub-Tabs: auch dann vorhanden, wenn die Tabelle ihn nicht nennt.
    if !known(tab, "All") && tab.is_stub() {
        tab.filters
            .insert(0, FilterDef::placeholder("All", FilterKind::Live, &[]));
    }
}

/// Quellen dynamischer Spalten (Custom Search/Extraction/JavaScript, API-Metriken, Linkmetriken, K-4) mit dem Tab, in dem
/// sie erscheinen. Jede Quelle meldet das Tab-Modul über `column_source()`.
pub fn column_sources() -> Vec<(&'static str, Box<dyn ColumnSource>)> {
    let mut out: Vec<(&'static str, Box<dyn ColumnSource>)> = Vec::new();
    let mut add = |tab: &'static str, source: Option<Box<dyn ColumnSource>>| {
        if let Some(source) = source {
            out.push((tab, source));
        }
    };
    add("Custom Search", custom_search::column_source());
    add("Custom Extraction", custom_extraction::column_source());
    add("Custom JavaScript", custom_js::column_source());
    add("Internal", custom_js::internal_column_source());
    add("Link Metrics", link_metrics::column_source());
    add("Analytics", analytics::column_source());
    add("Internal", link_metrics::column_source());
    add("Internal", dynamic_api::column_source());
    add("Internal", custom_extraction::internal_column_source());
    add("Internal", links::column_source());
    add("Structured Data", structured_data::column_source());
    add("Internal", internal_crawl::column_source());
    add("Internal", internal::rendering_column_source());
    add("PageSpeed", pagespeed::column_source());
    add("Mobile", mobile::column_source());
    #[cfg(feature = "testing")]
    add("Internal", testing::source());
    out
}

/// Dynamische Spalten eines Tabs für die Konfiguration `cfg` (leer, wenn der Tab keine Quelle hat).
pub fn dynamic_columns(tab_id: &str, cfg: &os_model::CrawlConfig) -> Vec<ColumnDef> {
    let mut out: Vec<ColumnDef> = Vec::new();
    for (tab, source) in column_sources() {
        if tab.eq_ignore_ascii_case(tab_id) {
            out.extend(source.columns(cfg));
        }
    }
    apply_meta_defaults(&mut out);
    out
}

/// Eine Zeile von [`M2_FILTERS`]: `(Tab, Filterkennung, Nachlaufschritt, Konfigurationsschlüssel, braucht JavaScript-Rendering)`.
pub type M2FilterRow = (
    &'static str,
    &'static str,
    Option<AnalysisStep>,
    &'static [&'static str],
    bool,
);

/// Stub-Filter von M2 (`docs/plan/M2-filters.toml`, ohne die dynamischen Platzhalter `<…>`):
/// `(Tab, Filterkennung, Nachlaufschritt, Konfigurationsschlüssel die `true` sein müssen, braucht JavaScript-Rendering)`.
/// Reihenfolge der Datei. Der Test `filters_registry.rs` hält die Tabelle gleich der Datei.
pub const M2_FILTERS: &[M2FilterRow] = &[
    ("Internal", "Plugins", None, &[], false),
    ("Internal", "Other", None, &[], false),
    ("Internal", "Media", None, &[], false),
    ("Internal", "Fonts", None, &[], false),
    ("Internal", "XML", None, &[], false),
    ("Security", "Mixed Content", None, &[], false),
    ("Security", "Form URL Insecure", None, &[], false),
    ("Security", "Form on HTTP URL", None, &[], false),
    ("Security", "Unsafe Cross-Origin Links", None, &[], false),
    (
        "Security",
        "Protocol-Relative Resource Links",
        None,
        &[],
        false,
    ),
    ("Security", "Missing HSTS Header", None, &[], false),
    (
        "Security",
        "Missing Content-Security-Policy Header",
        None,
        &[],
        false,
    ),
    (
        "Security",
        "Missing X-Content-Type-Options Header",
        None,
        &[],
        false,
    ),
    (
        "Security",
        "Missing X-Frame-Options Header",
        None,
        &[],
        false,
    ),
    (
        "Security",
        "Missing Secure Referrer-Policy Header",
        None,
        &[],
        false,
    ),
    ("Security", "Bad Content Type", None, &[], false),
    ("Response Codes", "Blocked Resource", None, &[], true),
    (
        "Response Codes",
        "Internal Blocked Resource",
        None,
        &[],
        true,
    ),
    (
        "Response Codes",
        "External Blocked Resource",
        None,
        &[],
        true,
    ),
    (
        "Response Codes",
        "Redirection (JavaScript)",
        None,
        &[],
        true,
    ),
    (
        "Response Codes",
        "Internal Redirection (JavaScript)",
        None,
        &[],
        true,
    ),
    (
        "Response Codes",
        "External Redirection (JavaScript)",
        None,
        &[],
        true,
    ),
    (
        "Response Codes",
        "Redirection (HTTP Refresh)",
        None,
        &[],
        false,
    ),
    (
        "Response Codes",
        "Internal Redirection (HTTP Refresh)",
        None,
        &[],
        false,
    ),
    (
        "Response Codes",
        "External Redirection (HTTP Refresh)",
        None,
        &[],
        false,
    ),
    ("URL", "Repetitive Path", None, &[], false),
    ("URL", "Internal Search", None, &[], false),
    (
        "URL",
        "Broken Bookmark",
        None,
        &["advanced.crawl_fragments"],
        false,
    ),
    ("URL", "GA Tracking Parameters", None, &[], false),
    (
        "Page Titles",
        "Over {title.max_px} Pixels",
        None,
        &[],
        false,
    ),
    (
        "Page Titles",
        "Below {title.min_px} Pixels",
        None,
        &[],
        false,
    ),
    ("Page Titles", "Same as H1", None, &[], false),
    (
        "Page Titles",
        "Outside <head>",
        None,
        &["advanced.html_validation"],
        false,
    ),
    (
        "Meta Description",
        "Over {meta_description.max_px} Pixels",
        None,
        &[],
        false,
    ),
    (
        "Meta Description",
        "Below {meta_description.min_px} Pixels",
        None,
        &[],
        false,
    ),
    (
        "Meta Description",
        "Outside <head>",
        None,
        &["advanced.html_validation"],
        false,
    ),
    ("H1", "Alt Text in h1", None, &[], false),
    ("H1", "Non-sequential", None, &[], false),
    ("H2", "Non-sequential", None, &[], false),
    (
        "Content",
        "Near Duplicates",
        Some(AnalysisStep::NearDuplicates),
        &["duplicates.near_enabled"],
        false,
    ),
    ("Content", "Soft 404 Pages", None, &[], false),
    ("Content", "Lorem Ipsum Placeholder", None, &[], false),
    (
        "Content",
        "Readability Difficult",
        None,
        &["extraction.readability"],
        false,
    ),
    (
        "Content",
        "Readability Very Difficult",
        None,
        &["extraction.readability"],
        false,
    ),
    (
        "Content",
        "Spelling Errors",
        None,
        &["spelling.enabled"],
        false,
    ),
    (
        "Content",
        "Grammar Errors",
        None,
        &["grammar.enabled"],
        false,
    ),
    (
        "Images",
        "Background Images",
        Some(AnalysisStep::Images),
        &["images.parse_inline_css_backgrounds"],
        false,
    ),
    ("Images", "Missing Size Attributes", None, &[], false),
    (
        "Images",
        "Incorrectly Sized Images",
        Some(AnalysisStep::Images),
        &[],
        true,
    ),
    ("Canonicals", "Canonical Is Relative", None, &[], false),
    (
        "Canonicals",
        "Unlinked",
        Some(AnalysisStep::Canonicals),
        &[],
        false,
    ),
    (
        "Canonicals",
        "Invalid Attribute In Annotation",
        None,
        &[],
        false,
    ),
    ("Canonicals", "Contains Fragment URL", None, &[], false),
    (
        "Canonicals",
        "Outside <head>",
        None,
        &["advanced.html_validation"],
        false,
    ),
    ("Directives", "NoArchive", None, &[], false),
    ("Directives", "NoSnippet", None, &[], false),
    ("Directives", "Max-Snippet", None, &[], false),
    ("Directives", "Max-Image-Preview", None, &[], false),
    ("Directives", "Max-Video-Preview", None, &[], false),
    ("Directives", "NoImageIndex", None, &[], false),
    ("Directives", "NoTranslate", None, &[], false),
    ("Directives", "Unavailable_After", None, &[], false),
    (
        "Directives",
        "Outside <head>",
        None,
        &["advanced.html_validation"],
        false,
    ),
    (
        "Hreflang",
        "Unlinked Hreflang URLs",
        Some(AnalysisStep::Hreflang),
        &[],
        false,
    ),
    (
        "Hreflang",
        "Inconsistent Language & Region Return Links",
        None,
        &[],
        false,
    ),
    ("Hreflang", "Non-Canonical Return Links", None, &[], false),
    ("Hreflang", "Noindex Return Links", None, &[], false),
    ("Hreflang", "Multiple Entries", None, &[], false),
    ("Hreflang", "Missing Self Reference", None, &[], false),
    ("Hreflang", "Not Using Canonical", None, &[], false),
    ("Hreflang", "Missing X-Default", None, &[], false),
    (
        "Hreflang",
        "Outside <head>",
        None,
        &["advanced.html_validation"],
        false,
    ),
    (
        "Links",
        "Non-Descriptive Anchor Text In Internal Outlinks",
        None,
        &[],
        false,
    ),
    (
        "Links",
        "Pages With High External Outlinks",
        None,
        &[],
        false,
    ),
    (
        "Links",
        "Pages With High Internal Outlinks",
        None,
        &[],
        false,
    ),
    (
        "Links",
        "Follow & Nofollow Internal Inlinks To Page",
        Some(AnalysisStep::Links),
        &[],
        false,
    ),
    (
        "Links",
        "Internal Nofollow Inlinks Only",
        Some(AnalysisStep::Links),
        &[],
        false,
    ),
    ("Links", "Outlinks To Localhost", None, &[], false),
    (
        "Links",
        "Non-Indexable Page Inlinks Only",
        Some(AnalysisStep::Links),
        &[],
        false,
    ),
    (
        "Links",
        "Pages With Uncrawlable Internal Outlinks",
        None,
        &[],
        false,
    ),
    (
        "Sitemaps",
        "URLs In Multiple Sitemaps",
        Some(AnalysisStep::Sitemaps),
        &[],
        false,
    ),
    (
        "Sitemaps",
        "XML Sitemap With Over 50k URLs",
        None,
        &[],
        false,
    ),
    ("Sitemaps", "XML Sitemap With Over 50MB", None, &[], false),
    ("Meta Keywords", "All", None, &[], false),
    ("Meta Keywords", "Missing", None, &[], false),
    ("Meta Keywords", "Duplicate", None, &[], false),
    ("Meta Keywords", "Multiple", None, &[], false),
    ("Pagination", "All", None, &[], false),
    ("Pagination", "Contains Pagination", None, &[], false),
    ("Pagination", "First Page", None, &[], false),
    ("Pagination", "Paginated 2+ Pages", None, &[], false),
    (
        "Pagination",
        "Pagination URL Not In Anchor Tag",
        None,
        &[],
        false,
    ),
    ("Pagination", "Non-200 Pagination URLs", None, &[], false),
    (
        "Pagination",
        "Unlinked Pagination URLs",
        Some(AnalysisStep::Pagination),
        &[],
        false,
    ),
    ("Pagination", "Non-Indexable", None, &[], false),
    ("Pagination", "Multiple Pagination URLs", None, &[], false),
    (
        "Pagination",
        "Pagination Loop",
        Some(AnalysisStep::Pagination),
        &[],
        false,
    ),
    (
        "Pagination",
        "Sequence Error",
        Some(AnalysisStep::Pagination),
        &[],
        false,
    ),
    ("JavaScript", "All", None, &[], true),
    (
        "JavaScript",
        "Pages With Blocked Resources",
        None,
        &[],
        true,
    ),
    ("JavaScript", "Contains JavaScript Links", None, &[], true),
    ("JavaScript", "Contains JavaScript Content", None, &[], true),
    (
        "JavaScript",
        "Noindex Only In Original HTML",
        None,
        &[],
        true,
    ),
    (
        "JavaScript",
        "Nofollow Only In Original HTML",
        None,
        &[],
        true,
    ),
    (
        "JavaScript",
        "Canonical Only In Rendered HTML",
        None,
        &[],
        true,
    ),
    ("JavaScript", "Canonical Mismatch", None, &[], true),
    (
        "JavaScript",
        "Page Title Only In Rendered HTML",
        None,
        &[],
        true,
    ),
    (
        "JavaScript",
        "Page Title Updated By JavaScript",
        None,
        &[],
        true,
    ),
    (
        "JavaScript",
        "Meta Description Only In Rendered HTML",
        None,
        &[],
        true,
    ),
    (
        "JavaScript",
        "Meta Description Updated By JavaScript",
        None,
        &[],
        true,
    ),
    ("JavaScript", "H1 Only In Rendered HTML", None, &[], true),
    ("JavaScript", "H1 Updated By JavaScript", None, &[], true),
    (
        "JavaScript",
        "Pages With JavaScript Errors",
        None,
        &[],
        true,
    ),
    (
        "Structured Data",
        "All",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "Contains Structured Data",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "Missing Structured Data",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "JSON-LD URLs",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "Microdata URLs",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "RDFa URLs",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "Parse Errors",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "Validation Errors",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "Validation Warnings",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "Rich Result Validation Errors",
        None,
        &["extraction.structured_data"],
        false,
    ),
    (
        "Structured Data",
        "Rich Result Validation Warnings",
        None,
        &["extraction.structured_data"],
        false,
    ),
    ("PageSpeed", "All", None, &[], false),
    (
        "PageSpeed",
        "Avoid Enormous Network Payloads",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Document Request Latency",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Duplicated JavaScript",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Font Display",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Forced Reflow",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Improve Image Delivery",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "LCP Request Discovery",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Layout Shift Culprits",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Legacy JavaScript",
        None,
        &["api.psi.enabled"],
        false,
    ),
    ("PageSpeed", "Minify CSS", None, &["api.psi.enabled"], false),
    (
        "PageSpeed",
        "Minify JavaScript",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Minimize Main-Thread Work",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Network Dependency Tree",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Optimize DOM Size",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Reduce JavaScript Execution Time",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Reduce Unused CSS",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Reduce Unused JavaScript",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Render Blocking Requests",
        None,
        &["api.psi.enabled"],
        false,
    ),
    (
        "PageSpeed",
        "Use Efficient Cache Lifetimes",
        None,
        &["api.psi.enabled"],
        false,
    ),
    ("Mobile", "All", None, &[], false),
    ("Mobile", "Viewport Not Set", None, &[], false),
    ("Mobile", "Target Size", None, &[], false),
    ("Mobile", "Content Not Sized Correctly", None, &[], false),
    ("Mobile", "Illegible Font Size", None, &[], false),
    ("Mobile", "Unsupported Plugins", None, &[], false),
    ("Mobile", "Mobile Alternate Link", None, &[], false),
    (
        "Accessibility",
        "All",
        None,
        &["extraction.accessibility"],
        true,
    ),
    ("Custom Search", "All", None, &[], false),
    ("Custom Extraction", "All", None, &[], false),
    ("Custom JavaScript", "All", None, &[], true),
    ("Analytics", "All", None, &[], false),
    (
        "Analytics",
        "Sessions Above 0",
        None,
        &["api.ga4.enabled"],
        false,
    ),
    (
        "Analytics",
        "Bounce Rate Above 70%",
        None,
        &["api.ga4.enabled"],
        false,
    ),
    ("Analytics", "No GA Data", None, &["api.ga4.enabled"], false),
    (
        "Analytics",
        "Non-Indexable with GA Data",
        None,
        &["api.ga4.enabled"],
        false,
    ),
    (
        "Analytics",
        "Orphan URLs",
        None,
        &["api.ga4.enabled"],
        false,
    ),
    ("Search Console", "All", None, &[], false),
    (
        "Search Console",
        "Clicks Above 0",
        None,
        &["api.gsc.enabled"],
        false,
    ),
    (
        "Search Console",
        "No Search Analytics Data",
        None,
        &["api.gsc.enabled"],
        false,
    ),
    (
        "Search Console",
        "Non-Indexable with Search Analytics Data",
        None,
        &["api.gsc.enabled"],
        false,
    ),
    (
        "Search Console",
        "Orphan URLs",
        None,
        &["api.gsc.enabled"],
        false,
    ),
    (
        "Search Console",
        "URL Is Not on Google",
        None,
        &["api.gsc.url_inspection"],
        false,
    ),
    (
        "Search Console",
        "Indexable URL Not Indexed",
        None,
        &["api.gsc.url_inspection"],
        false,
    ),
    (
        "Search Console",
        "URL is on Google, But Has Issues",
        None,
        &["api.gsc.url_inspection"],
        false,
    ),
    (
        "Search Console",
        "User-Declared Canonical Not Selected",
        None,
        &["api.gsc.url_inspection"],
        false,
    ),
    (
        "Search Console",
        "Page Is Not Mobile Friendly",
        None,
        &["api.gsc.url_inspection"],
        false,
    ),
    (
        "Search Console",
        "AMP URL Is Invalid",
        None,
        &["api.gsc.url_inspection"],
        false,
    ),
    (
        "Search Console",
        "Rich Result Invalid",
        None,
        &["api.gsc.url_inspection"],
        false,
    ),
    ("Validation", "All", None, &[], false),
    (
        "Validation",
        "Invalid HTML Elements In <head>",
        None,
        &["advanced.html_validation"],
        false,
    ),
    (
        "Validation",
        "Missing <head> Tag",
        None,
        &["advanced.html_validation"],
        false,
    ),
    (
        "Validation",
        "Multiple <head> Tags",
        None,
        &["advanced.html_validation"],
        false,
    ),
    (
        "Validation",
        "Missing <body> Tag",
        None,
        &["advanced.html_validation"],
        false,
    ),
    (
        "Validation",
        "Multiple <body> Tags",
        None,
        &["advanced.html_validation"],
        false,
    ),
    (
        "Validation",
        "HTML Document Over {validation.html_doc_max_bytes}MB",
        None,
        &[],
        false,
    ),
    (
        "Validation",
        "Resource Over {validation.resource_max_bytes}MB",
        None,
        &[],
        false,
    ),
    ("Link Metrics", "All", None, &[], false),
    ("Change Detection", "All", None, &[], false),
    ("Change Detection", "Page Titles", None, &[], false),
    ("Change Detection", "Meta Description", None, &[], false),
    ("Change Detection", "H1", None, &[], false),
    ("Change Detection", "Word Count", None, &[], false),
    ("Change Detection", "Crawl Depth", None, &[], false),
    ("Change Detection", "Inlinks", None, &[], false),
    ("Change Detection", "Unique Inlinks", None, &[], false),
    ("Change Detection", "Internal Outlinks", None, &[], false),
    (
        "Change Detection",
        "Unique Internal Outlinks",
        None,
        &[],
        false,
    ),
    ("Change Detection", "External Outlinks", None, &[], false),
    (
        "Change Detection",
        "Unique External Outlinks",
        None,
        &[],
        false,
    ),
    (
        "Change Detection",
        "Structured Data Unique Types",
        None,
        &[],
        false,
    ),
    ("Change Detection", "Indexability", None, &[], false),
    ("Change Detection", "Canonical", None, &[], false),
    ("Change Detection", "Redirect Target", None, &[], false),
    ("Change Detection", "Content", None, &[], false),
];

/// Spalte, nach der der Filter `Duplicate` der Element-Tabs gruppiert (ONPAGE-092, -102, -122, -132).
pub const DUPLICATE_GROUP_COLUMN: [(&str, &str); 5] = [
    ("Page Titles", "Title 1"),
    ("Meta Description", "Meta Description 1"),
    ("H1", "H1-1"),
    ("H2", "H2-1"),
    ("Meta Keywords", "Meta Keywords 1"),
];

/// Standardsortierung eines einzelnen Filters (ONPAGE-001). Die Ansicht fragt sie ab, wenn die Abfrage keine
/// Sortierung angibt, und zwar **vor** `TabDef.default_sort`; `None` heißt: der Filter hat keine eigene
/// Standardsortierung, es gilt `TabDef.default_sort` und danach die Entdeckungsreihenfolge (`UrlId`, K-17).
///
/// Page Titles, Meta Description, H1 und H2 haben kein `default_sort`; nur ihr Filter `Duplicate` gruppiert
/// gleiche Werte direkt untereinander (Sortierung nach dem ersten Wert, ONPAGE-092 und Gegenstücke), alle
/// anderen Filter erscheinen in Entdeckungsreihenfolge.
pub fn filter_default_sort(tab_id: &str, filter_id: &str) -> Option<Vec<SortKey>> {
    if filter_id != "Duplicate" {
        return None;
    }
    DUPLICATE_GROUP_COLUMN
        .iter()
        .find(|(tab, _)| *tab == tab_id)
        .map(|(_, column)| {
            vec![SortKey {
                column: (*column).to_string(),
                descending: false,
            }]
        })
}

/// Registry mit den konfigurierten Spalten und Filtern (OUT-009, INTEG-020).
pub fn configured(cfg: &os_model::CrawlConfig) -> Vec<TabDef> {
    configure(cfg, None)
}
pub fn configured_for_file(cfg: &os_model::CrawlConfig, file: &os_store::CrawlFile) -> Vec<TabDef> {
    configure(cfg, Some(file))
}
fn configure(cfg: &os_model::CrawlConfig, file: Option<&os_store::CrawlFile>) -> Vec<TabDef> {
    configure_sources(cfg, file, column_sources())
}
fn configure_sources(
    cfg: &os_model::CrawlConfig,
    file: Option<&os_store::CrawlFile>,
    sources: Vec<(&'static str, Box<dyn ColumnSource>)>,
) -> Vec<TabDef> {
    let mut tabs = all();
    for (id, source) in sources {
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == id) {
            tab.columns.extend(match file {
                Some(f) => source.columns_for_file(cfg, f),
                None => source.columns(cfg),
            });
            for filter in source.filters(cfg) {
                if let Some(slot) = tab.filters.iter_mut().find(|f| f.id == filter.id) {
                    *slot = filter;
                } else {
                    tab.filters.push(filter);
                }
            }
            tab.filters.retain(|f| source.keeps_filter(cfg, f.id));
            if let Some(file) = file {
                for filter in source.file_filters(cfg, file) {
                    if let Some(slot) = tab.filters.iter_mut().find(|f| f.id == filter.id) {
                        *slot = filter;
                    } else {
                        tab.filters.push(filter);
                    }
                }
            }
            apply_meta_defaults(&mut tab.columns);
        }
    }
    for tab in &mut tabs {
        tab.columns.extend(crate::segments::columns(cfg));
        apply_meta_defaults(&mut tab.columns);
    }
    tabs
}

#[cfg(test)]
mod w1_contracts {
    use super::*;
    struct FixtureSource;
    impl ColumnSource for FixtureSource {
        fn columns(&self, _: &os_model::CrawlConfig) -> Vec<ColumnDef> {
            Vec::new()
        }
        fn filters(&self, _: &os_model::CrawlConfig) -> Vec<FilterDef> {
            let mut filter = FilterDef::placeholder("Fixture Filter", FilterKind::Live, &[]);
            filter.predicate = "u.id = 1";
            vec![filter]
        }
    }

    // spec: INTEG-020
    // Infrastruktur (M2-F05a) für INTEG-020; Fachprüfung M2-D14.
    #[test]
    fn configured_sources_contribute_resolvable_live_filters() {
        let cfg = os_model::CrawlConfig::from_toml_str(include_str!(
            "../../../../tests/fixtures/config/default.osconfig"
        ))
        .unwrap();
        let tabs = configure_sources(&cfg, None, vec![("Internal", Box::new(FixtureSource))]);
        let (tab, filter) = crate::names::resolve_in(&tabs, "Internal:Fixture Filter").unwrap();
        assert_eq!(tab, "Internal");
        assert_eq!(filter, "Fixture Filter");
        let tab = tabs.iter().find(|tab| tab.id == "Internal").unwrap();
        let filter = tab
            .filters
            .iter()
            .find(|filter| filter.id == "Fixture Filter")
            .unwrap();
        assert!(!filter.is_stub());
        assert_eq!(filter.predicate, "u.id = 1");
        use os_model::{IdSource, SeededIds, Timestamp};
        let at = Timestamp::UNIX_EPOCH;
        let dir = tempfile::tempdir().unwrap();
        let file = os_store::CrawlFile::create(
            &dir.path().join("source.ospider"),
            &os_store::CrawlInit {
                config: cfg.clone(),
                crawl_id: SeededIds::new(42).new_crawl_id(at),
                mode: os_model::CrawlMode::Spider,
                start_url: Some("https://site.test/".into()),
                list_source: None,
                project: "fixture".into(),
                name: "source".into(),
                created_at: at,
            },
            os_store::StorageProfile::Balanced,
        )
        .unwrap();
        file.conn().execute("INSERT INTO urls(id,url,address,origin,is_internal,in_tables,state,discovered_via,folder_depth,url_length) VALUES(1,'https://site.test/','https://site.test/','https://site.test',1,1,'done','link',0,18)", []).unwrap();
        let query = crate::views::ViewQuery::new("Internal", "Fixture Filter");
        assert_eq!(
            crate::views::query_view_in(&tabs, &file, &cfg, &query)
                .unwrap()
                .total,
            1
        );
        let counts = crate::views::filter_counts_in(&tabs, &file, &cfg).unwrap();
        assert!(counts.iter().any(|count| count.tab == "Internal"
            && count.filter == "Fixture Filter"
            && count.count == Some(1)));
    }
}

#[cfg(feature = "testing")]
pub mod testing;

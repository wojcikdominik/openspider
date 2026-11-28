//! Hilfen für die Schema-Tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::{CrawlFile, CrawlInit, StorageProfile};

pub const ALL_TABLES: &[&str] = &[
    "a11y_nodes",
    "a11y_rules",
    "ai_answers",
    "ai_batches",
    "ai_cache",
    "ai_embeddings",
    "ai_link_suggestions",
    "ai_prompt_results",
    "ai_queries",
    "ai_questions",
    "ai_redirect_map",
    "ai_requests",
    "ai_runs",
    "ai_suggestions",
    "analysis_steps",
    "api_budget",
    "api_errors",
    "api_status",
    "api_urls",
    "api_values",
    "canonicals",
    "cookies",
    "crawl_config",
    "crawl_counters",
    "crawl_events",
    "crawl_meta",
    "custom_extraction_values",
    "custom_js_values",
    "custom_search_hits",
    "directives",
    "discovery_edges",
    "ext_keys",
    "ext_texts",
    "forms",
    "hreflang",
    "http_headers",
    "image_instances",
    "links",
    "list_input",
    "minhash",
    "near_duplicates",
    // M4-D05 (08 AI-032, Nachträge K4-24/K4-26): gespeicherte KI-Eingaben.
    "page_ai_inputs",
    "page_html",
    "pagination",
    "queue",
    "refresh",
    "render_blocked",
    "render_console",
    "render_network",
    "render_pages",
    "robots_files",
    "screenshots",
    "sd_issues",
    "sd_items",
    "serp_edits",
    "sitemap_entries",
    "sitemap_entry_ext",
    "sitemaps",
    "spelling_findings",
    "url_counters",
    "url_ext_data",
    "url_flags",
    "url_segments",
    "urls",
    "view_cache",
];

pub fn created_at() -> Timestamp {
    Timestamp::from_db("2026-10-13T07:35:00.000Z").unwrap()
}

pub fn init(config: CrawlConfig) -> CrawlInit {
    CrawlInit {
        crawl_id: SeededIds::new(42).new_crawl_id(created_at()),
        mode: CrawlMode::Spider,
        start_url: Some("https://site.test/".into()),
        list_source: None,
        project: "Projekt".into(),
        name: "Testcrawl".into(),
        config,
        created_at: created_at(),
    }
}

pub fn create(dir: &Path, name: &str) -> (PathBuf, CrawlFile) {
    let path = dir.join(name);
    let file = CrawlFile::create(
        &path,
        &init(CrawlConfig::default()),
        StorageProfile::Balanced,
    )
    .unwrap();
    (path, file)
}

pub fn tables(file: &CrawlFile) -> Vec<String> {
    let mut stmt = file
        .conn()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

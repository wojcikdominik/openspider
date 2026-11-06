//! Werkzeuge des MCP-Servers (AGENT-004, AGENT-010 … AGENT-013): `list_crawls`, `crawl_overview`,
//! `query_urls`, `url_details`, `list_issues`. Alle seitenstämmigen Felder (Titel, Texte, Ankertexte, Alt-Texte,
//! Header, Adressen …) stehen ausschließlich in `ToolOutput.untrusted`; die Zusammenfassung enthält nur
//! Zähler, Zustände und Katalogdaten.
//!
//! Die Werkzeuge öffnen die Crawl-Datei je Aufruf nur lesend und rechnen über dieselben Funktionen wie
//! Oberfläche und Export (`os_analyze::views`, `os_analyze::issues`, `os_export::reports`): keine zweite
//! Zählung, keine eigene Sortierung.

pub mod common;
pub mod compare_crawls;
pub mod crawl;
pub mod crawl_overview;
pub mod export;
pub mod link_metrics;
pub mod list_catalog;
pub mod list_crawls;
pub mod list_issues;
pub mod query_urls;
pub mod sql_query;
pub mod url_content;
pub mod url_details;
pub mod url_screenshot;

use crate::tool::Tool;

/// Alle Werkzeuge (nach Freigabe gefiltert durch `permissions`), in der Reihenfolge von `tools/list`.
pub fn all() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(link_metrics::FetchLinkMetrics),
        Box::new(compare_crawls::CompareCrawls),
        Box::new(list_crawls::ListCrawls),
        Box::new(crawl_overview::CrawlOverview),
        Box::new(query_urls::QueryUrls),
        Box::new(url_details::UrlDetails),
        Box::new(list_issues::ListIssues),
        Box::new(sql_query::SqlQuery),
        Box::new(crawl::CrawlTool("start_crawl")),
        Box::new(crawl::CrawlTool("crawl_status")),
        Box::new(crawl::CrawlTool("pause_crawl")),
        Box::new(crawl::CrawlTool("stop_crawl")),
        Box::new(crawl::CrawlTool("resume_crawl")),
        Box::new(export::Export),
        Box::new(list_catalog::ListCatalog),
        Box::new(url_content::UrlContent),
        Box::new(url_screenshot::UrlScreenshot),
    ]
}

#[cfg(test)]
mod tests {
    use crate::tool::ToolClass;

    // spec: AGENT-010, AGENT-011, AGENT-012, AGENT-013
    // spec: AGENT-020, AGENT-003
    #[test]
    fn m2_tools_are_registered_with_their_permission_classes() {
        let tools = super::all();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            [
                "fetch_link_metrics",
                "compare_crawls",
                "list_crawls",
                "crawl_overview",
                "query_urls",
                "url_details",
                "list_issues",
                "sql_query",
                "start_crawl",
                "crawl_status",
                "pause_crawl",
                "stop_crawl",
                "resume_crawl",
                "export",
                "list_catalog",
                "url_content",
                "url_screenshot"
            ]
        );
        assert_eq!(
            tools
                .iter()
                .filter(|t| t.class() == ToolClass::Read)
                .count(),
            10
        );
        for t in &tools {
            assert_eq!(t.input_schema()["type"], "object", "{}", t.name());
            assert!(!t.description().is_empty());
        }
    }
}

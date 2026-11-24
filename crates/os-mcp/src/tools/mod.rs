//! Werkzeuge des MCP-Servers (AGENT-004, AGENT-010 … AGENT-013): `list_crawls`, `crawl_overview`,
//! `query_urls`, `url_details`, `list_issues`. Alle seitenstämmigen Felder (Titel, Texte, Ankertexte, Alt-Texte,
//! Header, Adressen …) stehen ausschließlich in `ToolOutput.untrusted`; die Zusammenfassung enthält nur
//! Zähler, Zustände und Katalogdaten.
//!
//! Die Werkzeuge öffnen die Crawl-Datei je Aufruf nur lesend und rechnen über dieselben Funktionen wie
//! Oberfläche und Export (`os_analyze::views`, `os_analyze::issues`, `os_export::reports`): keine zweite
//! Zählung, keine eigene Sortierung.

pub mod ai_decisions;
pub mod ai_links;
pub mod ai_redirects;
pub mod ai_suggestions;
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

/// KI-Werkzeuge der Klasse `cost` (AGENT-030); Stubs bis M4-C02.
pub const AI_TOOLS: [&str; 4] = [
    "run_decisions",
    "suggest_metadata",
    "redirect_mapping",
    "internal_link_suggestions",
];

/// Die Werkzeuge, die ein Server anbietet: ohne Schalter „KI (Vorschau)“ (Plan M4 E1, E-2 b, Standard aus)
/// ohne die KI-Werkzeuge, sonst [`all`].
pub fn offered(preview: os_ai::PreviewSwitch) -> Vec<Box<dyn Tool>> {
    all()
        .into_iter()
        .filter(|t| preview.is_on() || !AI_TOOLS.contains(&t.name()))
        .collect()
}

/// Alle Werkzeuge (nach Freigabe gefiltert durch `permissions`), in der Reihenfolge von `tools/list`.
pub fn all() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(ai_decisions::AiTool),
        Box::new(ai_suggestions::AiTool),
        Box::new(ai_redirects::AiTool),
        Box::new(ai_links::AiTool),
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

/// MCP-Annotationen eines Werkzeugs (AGENT-042, Nachtrag 24.11.2026): englischer Anzeigename, nur lesend,
/// zerstörerisch. Je Werkzeug ausdrücklich festgelegt, unabhängig von der Freigabeklasse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolHints {
    pub title: &'static str,
    pub read_only: bool,
    pub destructive: bool,
}

const fn hints(title: &'static str, read_only: bool, destructive: bool) -> ToolHints {
    ToolHints {
        title,
        read_only,
        destructive,
    }
}

/// Annotationen aller Werkzeuge aus [`all`] (auch der KI-Werkzeuge hinter dem Schalter „KI (Vorschau)“).
/// `destructive` nur, wenn das Werkzeug etwas unwiderruflich beendet, löscht oder überschreibt: `stop_crawl`
/// beendet endgültig; `export` legt bei vorhandener Zieldatei einen neuen Namen an und überschreibt nie.
pub const HINTS: [(&str, ToolHints); 21] = [
    (
        "run_decisions",
        hints("Run AI decision questions", false, false),
    ),
    (
        "suggest_metadata",
        hints("Suggest metadata (AI)", false, false),
    ),
    (
        "redirect_mapping",
        hints("Map redirects (AI)", false, false),
    ),
    (
        "internal_link_suggestions",
        hints("Suggest internal links (AI)", false, false),
    ),
    (
        "fetch_link_metrics",
        hints("Fetch link metrics", false, false),
    ),
    ("compare_crawls", hints("Compare crawls", true, false)),
    ("list_crawls", hints("List crawls", true, false)),
    ("crawl_overview", hints("Crawl overview", true, false)),
    ("query_urls", hints("Query URLs", true, false)),
    ("url_details", hints("URL details", true, false)),
    ("list_issues", hints("List issues", true, false)),
    ("sql_query", hints("Run read-only SQL query", true, false)),
    ("start_crawl", hints("Start crawl", false, false)),
    ("crawl_status", hints("Crawl status", true, false)),
    ("pause_crawl", hints("Pause crawl", false, false)),
    ("stop_crawl", hints("Stop crawl", false, true)),
    ("resume_crawl", hints("Resume crawl", false, false)),
    ("export", hints("Export to file", false, false)),
    ("list_catalog", hints("List catalog names", true, false)),
    ("url_content", hints("URL content", true, false)),
    ("url_screenshot", hints("URL screenshot", true, false)),
];

/// Annotationen eines Werkzeugs nach [`HINTS`]; `None` für fremde Werkzeuge (z. B. Testattrappen).
pub fn hints_for(name: &str) -> Option<ToolHints> {
    HINTS.iter().find(|(n, _)| *n == name).map(|(_, h)| *h)
}

#[cfg(test)]
mod tests {
    use crate::tool::ToolClass;

    // spec: AGENT-042
    #[test]
    fn every_registered_tool_has_explicit_hints_and_no_stale_entries() {
        let names: Vec<&str> = super::all().iter().map(|t| t.name()).collect();
        for name in &names {
            assert!(super::hints_for(name).is_some(), "{name}");
        }
        for (name, hints) in super::HINTS {
            assert!(names.contains(&name), "{name}");
            assert!(!hints.title.is_empty() && hints.title.is_ascii(), "{name}");
            assert!(!(hints.read_only && hints.destructive), "{name}");
        }
    }

    // spec: AGENT-010, AGENT-011, AGENT-012, AGENT-013
    // spec: AGENT-020, AGENT-003
    #[test]
    fn m2_tools_are_registered_with_their_permission_classes() {
        let tools = super::all();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            [
                "run_decisions",
                "suggest_metadata",
                "redirect_mapping",
                "internal_link_suggestions",
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

#[cfg(test)]
mod m4_tests {
    use crate::{ToolClass, ToolContext, ToolError};
    use os_ai::PreviewSwitch;
    use serde_json::json;
    // spec: AGENT-030
    // Plan M4 E1 (E-2 b): Schalter „KI (Vorschau)“ aus → die vier KI-Werkzeuge werden nicht angeboten.
    #[test]
    fn ai_tools_are_offered_only_with_the_preview_switch_on() {
        let names = |preview| {
            super::offered(preview)
                .iter()
                .map(|t| t.name().to_string())
                .collect::<Vec<_>>()
        };
        let off = names(PreviewSwitch::OFF);
        let on = names(PreviewSwitch::ON);
        assert_eq!(on.len(), super::all().len());
        assert_eq!(off.len(), on.len() - super::AI_TOOLS.len());
        for ai in super::AI_TOOLS {
            assert!(!off.iter().any(|n| n == ai), "{ai}");
            assert!(on.iter().any(|n| n == ai), "{ai}");
        }
        assert_eq!(
            PreviewSwitch::from_settings(&os_model::settings::AppSettings::default()),
            PreviewSwitch::OFF
        );
    }
    // spec: AI-001
    // spec: AGENT-030
    #[test]
    fn cost_stub_fixture_never_executes_even_with_confirmation() {
        let ctx = ToolContext::new(std::path::PathBuf::from("unused-ai-stub-fixture"));
        let names = [
            "run_decisions",
            "suggest_metadata",
            "redirect_mapping",
            "internal_link_suggestions",
        ];
        for name in names {
            let tool = super::all().into_iter().find(|t| t.name() == name).unwrap();
            assert_eq!(tool.class(), ToolClass::Cost);
            for confirm in [false, true] {
                let result = tool.call(&ctx, json!({"crawl_id":"fixture","confirm":confirm}));
                assert!(
                    matches!(result, Err(ToolError::InvalidArguments(message)) if message.contains("STUB(M4-C02)"))
                );
            }
        }
    }
}

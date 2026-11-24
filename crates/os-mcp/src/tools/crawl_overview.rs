//! Werkzeug `crawl_overview` (AGENT-010): Kennzahlen je Tab und Filter wie die Overview-Seitenleiste, dazu die
//! wichtigsten Issues.
//!
//! Die Zahlen kommen aus derselben Funktion wie der Report „Crawl Overview“ (OUT-080,
//! `os_export::reports::overview::build`): keine zweite Zählung. Die Issue-Zeilen kommen aus
//! `issues::issues_from_counts` über `views::filter_counts`, wie im Issues-Export und im Issues-Tab.

use os_analyze::issues::{self, IssueRow};
use os_analyze::tabs;
use os_analyze::views::{self, FilterCount};
use os_export::reports::overview::{self, Cell};
use os_model::{CrawlState, SystemClock};
use serde_json::{json, Value as Json};

use super::common::{internal, object, open_crawl, reject_unknown, view_error};
use crate::tool::{Tool, ToolClass, ToolContext, ToolError, ToolOutput};

/// Name des Werkzeugs.
pub const NAME: &str = "crawl_overview";

/// Wie viele Issues die Kurzliste `top_issues` enthält.
pub const TOP_ISSUES: usize = 10;

/// Das Werkzeug `crawl_overview`.
pub struct CrawlOverview;

impl Tool for CrawlOverview {
    fn name(&self) -> &'static str {
        NAME
    }

    fn class(&self) -> ToolClass {
        ToolClass::Read
    }

    fn description(&self) -> &'static str {
        "Overview of a crawl: figures per section (Summary, Crawl Data, Response Codes, all filters of each \
         tab with count and share, Crawl Depth, Issues), exactly the numbers of the \"Crawl Overview\" \
         report, and the most important issues by type and priority (top_issues). Filters without a number \
         (urls = null) need crawl analysis; they are listed in needs_crawl_analysis. list_issues returns the \
         complete issue list with texts, query_urls returns the URLs of a filter."
    }

    fn input_schema(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "crawl_id": {"type": "string", "description": "ID from list_crawls"}
            },
            "required": ["crawl_id"],
            "additionalProperties": false
        })
    }

    fn call(&self, ctx: &ToolContext, args: Json) -> Result<ToolOutput, ToolError> {
        reject_unknown(&args, &["crawl_id"])?;
        let crawl = open_crawl(ctx, &args)?;
        let tab_defs = tabs::configured_for_file(&crawl.cfg, &crawl.file);
        let report = overview::build(&crawl.file, &crawl.cfg, &tab_defs).map_err(internal)?;
        let counts = views::filter_counts(&crawl.file, &crawl.cfg).map_err(view_error)?;

        // Abschnitte in Reihenfolge des Reports.
        let mut sections: Vec<(String, Vec<Json>)> = Vec::new();
        for row in &report.rows {
            let item = json!({
                "item": row.item,
                "urls": cell_json(&row.urls),
                "percent_of_total": row.pct,
            });
            match sections.last_mut() {
                Some((name, items)) if *name == row.section => items.push(item),
                _ => sections.push((row.section.clone(), vec![item])),
            }
        }
        let sections: Vec<Json> = sections
            .into_iter()
            .map(|(section, items)| json!({"section": section, "items": items}))
            .collect();

        let top_issues: Vec<Json> = issues::issues_from_counts(&counts)
            .iter()
            .take(TOP_ISSUES)
            .map(|row| issue_json(row, &counts))
            .collect();

        // Wie list_crawls: der tatsächliche Zustand, ein „Running“ ohne Lebenszeichen gilt als „Incomplete“.
        let state = if crawl.meta.state == CrawlState::Running {
            crawl
                .file
                .effective_state(&SystemClock)
                .unwrap_or(crawl.meta.state)
        } else {
            crawl.meta.state
        };
        let mut fields = vec![
            ("crawl_id", json!(crawl.id.to_string())),
            ("state", json!(state.as_str())),
        ];
        if state != crawl.meta.state {
            fields.push(("stored_state", json!(crawl.meta.state.as_str())));
        }
        fields.push(("sections", Json::Array(sections)));
        fields.push(("top_issues", Json::Array(top_issues)));
        fields.push(("needs_crawl_analysis", json!(report.pending_analysis)));

        Ok(ToolOutput {
            summary: object(fields),
            untrusted: None,
            total: None,
            next_cursor: None,
        })
    }
}

fn cell_json(cell: &Cell) -> Json {
    match cell {
        Cell::Empty => Json::Null,
        Cell::Int(n) => json!(n),
        Cell::Text(s) => json!(s),
        Cell::Float(f) => json!(f),
    }
}

fn issue_json(row: &IssueRow, counts: &[FilterCount]) -> Json {
    let label = counts
        .iter()
        .find(|c| c.tab == row.tab && c.filter == row.filter)
        .map_or_else(|| row.filter.clone(), |c| c.label.clone());
    json!({
        "id": row.id,
        "type": row.issue_type.as_str(),
        "priority": row.priority.as_str(),
        "tab": row.tab,
        "filter": label,
        "urls": row.count,
        "percent_of_tab": row.pct,
    })
}

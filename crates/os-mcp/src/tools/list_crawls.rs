//! Werkzeug `list_crawls` (AGENT-010): die Crawls im Crawl-Ordner mit Kennung, Start-URL, Datum, Zustand und
//! URL-Anzahl, neueste zuerst, paginiert (AGENT-005).
//!
//! Der Zustand ist der tatsächliche (`CrawlFile::effective_state`): Ein gespeichertes „Running“ ohne
//! Heartbeat der letzten zehn Sekunden gilt als abgestürzt („Incomplete“, unterbrochen). Ein pausierter
//! Crawl erscheint mit dem Zustand „Paused“ (CRAWL-126).

use os_model::{CrawlMode, CrawlState, SystemClock};
use os_store::registry::CrawlListing;
use os_store::{CrawlFile, OpenMode};
use serde_json::{json, Value as Json};

use super::common::{internal, object, reject_unknown};
use crate::paging::{self, PageArgs};
use crate::tool::{Tool, ToolClass, ToolContext, ToolError, ToolOutput};

/// Name des Werkzeugs.
pub const NAME: &str = "list_crawls";

const CURSOR_BINDING: &str = "list_crawls|v1";

/// Das Werkzeug `list_crawls`.
pub struct ListCrawls;

impl Tool for ListCrawls {
    fn name(&self) -> &'static str {
        NAME
    }

    fn class(&self) -> ToolClass {
        ToolClass::Read
    }

    fn description(&self) -> &'static str {
        "Lists the saved crawls (newest first): crawl_id, mode, start URL or URL list, project, name, \
         state (Running, Paused, Completed, Incomplete = interrupted, Failed), start and end time and the \
         number of URLs in the file. All other tools need the crawl_id. Paused and interrupted crawls are \
         visible and readable. Paginated with limit (1 to 500, default 50) and cursor."
    }

    fn input_schema(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 50,
                          "description": "Crawls per page"},
                "cursor": {"type": "string", "description": "next_cursor of the previous response"}
            },
            "additionalProperties": false
        })
    }

    fn call(&self, ctx: &ToolContext, args: Json) -> Result<ToolOutput, ToolError> {
        reject_unknown(&args, &["limit", "cursor"])?;
        let page = PageArgs::from_args(&args)?;
        let offset = match &page.cursor {
            Some(c) => paging::decode_cursor(c, CURSOR_BINDING)?,
            None => 0,
        };
        let report = super::common::discover(&ctx.crawl_dir).map_err(internal)?;
        // Dateinamen sind UUID v7: aufsteigend = Anlegereihenfolge; neueste zuerst.
        let mut crawls = report.crawls;
        if let Some(session) = &ctx.session {
            for c in session.crawls() {
                if !crawls
                    .iter()
                    .any(|r| r.meta.crawl_id.to_string() == c.crawl_id)
                {
                    if let Ok(listing) = os_store::registry::read_listing(&c.file) {
                        crawls.push(listing);
                    }
                }
            }
            crawls.sort_by_key(|c| c.meta.crawl_id.to_string());
        }
        crawls.reverse();
        let total = crawls.len() as u64;
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(crawls.len());
        let end = start.saturating_add(page.limit as usize).min(crawls.len());
        let entries: Vec<Json> = crawls[start..end]
            .iter()
            .map(|listing| {
                let mut entry = entry_json(listing);
                if let Some(live) = ctx
                    .session
                    .as_ref()
                    .and_then(|s| s.status(&listing.meta.crawl_id.to_string()))
                {
                    entry["state"] = json!(live.state.as_str());
                }
                entry
            })
            .collect();
        let next_cursor =
            (end < crawls.len()).then(|| paging::encode_cursor(CURSOR_BINDING, end as u64));

        let mut summary = vec![("crawls", Json::Array(entries))];
        if !report.skipped.is_empty() {
            let skipped: Vec<Json> = report
                .skipped
                .iter()
                .take(20)
                .map(|(path, e)| {
                    json!({
                        "file": path.file_name().map(|n| n.to_string_lossy().into_owned()),
                        "problem": e.to_string(),
                    })
                })
                .collect();
            summary.push(("unreadable_files", Json::Array(skipped)));
        }
        Ok(ToolOutput {
            summary: object(summary),
            untrusted: None,
            total: Some(total),
            next_cursor,
        })
    }
}

/// Tatsächlicher Zustand: ein „Running“ ohne Lebenszeichen gilt als unterbrochen.
fn effective_state(listing: &CrawlListing) -> CrawlState {
    if listing.meta.state != CrawlState::Running {
        return listing.meta.state;
    }
    CrawlFile::open(&listing.path, OpenMode::ReadOnly)
        .and_then(|f| f.effective_state(&SystemClock))
        .unwrap_or(listing.meta.state)
}

fn entry_json(listing: &CrawlListing) -> Json {
    let meta = &listing.meta;
    let state = effective_state(listing);
    let mode = match meta.mode {
        CrawlMode::Spider => "spider",
        CrawlMode::List => "list",
        CrawlMode::Serp => "serp",
    };
    let mut entry = json!({
        "crawl_id": meta.crawl_id.to_string(),
        "mode": mode,
        "start_url": meta.start_url,
        "project": meta.project,
        "name": meta.name,
        "state": state.as_str(),
        "end_reason": meta.end_reason.map(|r| r.as_str()),
        "started_at": meta.started_at.map(|t| t.to_db()),
        "ended_at": meta.ended_at.map(|t| t.to_db()),
        "urls": listing.url_count,
        "file": listing.path.file_name().map(|n| n.to_string_lossy().into_owned()),
    });
    if state != meta.state {
        entry["stored_state"] = json!(meta.state.as_str());
    }
    entry
}

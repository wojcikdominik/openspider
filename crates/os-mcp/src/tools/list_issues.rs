//! Werkzeug `list_issues` (AGENT-013, AGENT-005): die Issues des Crawls mit Typ, Priorität, Anzahl, höchstens
//! fünf Beispiel-URLs, dem Verweis auf `query_urls` und den Katalogtexten `what`/`why`/`fix`.
//!
//! Zahlen und Reihenfolge sind die des Issues-Tabs und des Issues-Exports: `views::filter_counts` →
//! `issues::issues_from_counts` (keine zweite Zählung). Die Texte kommen aus `issues::texts` mit den
//! Schwellen der Crawl-Konfiguration (Textgleichheit mit App und Export). Die Beispiel-URLs stammen aus
//! gecrawlten Seiten und stehen deshalb im Untrusted-Block.

use os_analyze::issues::{self, IssueRow};
use os_analyze::tabs;
use os_analyze::views::{self, ColumnSelection, FilterCount, ViewQuery};
use os_model::{IssuePriority, IssueType, Value};
use serde_json::{json, Value as Json};

use super::common::{
    issue_text_params, lang_arg, object, open_crawl, opt_str, reject_unknown, value_json,
    view_error,
};
use crate::paging::{self, PageArgs};
use crate::tool::{Tool, ToolClass, ToolContext, ToolError, ToolOutput};

/// Name des Werkzeugs.
pub const NAME: &str = "list_issues";

/// Höchstzahl der Beispiel-URLs je Issue.
pub const MAX_EXAMPLES: u32 = 5;

/// Das Werkzeug `list_issues`.
pub struct ListIssues;

impl Tool for ListIssues {
    fn name(&self) -> &'static str {
        NAME
    }

    fn class(&self) -> ToolClass {
        ToolClass::Read
    }

    fn description(&self) -> &'static str {
        "Lists the issues of a crawl (findings with matches) sorted by type (Issue = error, Warning = \
         warning, Opportunity = hint), priority (High, Medium, Low) and count, like the Issues tab. Per \
         issue: id, type, priority, tab, filter, number of affected URLs, share, the texts what (what it \
         is), why (why it matters) and fix (how to fix it), and query: tab and filter with which query_urls \
         returns exactly this number of URLs. Up to 5 example URLs per issue are in the marked data block. \
         Filters: priority (high, medium, low), type (issue or error, warning, opportunity or notice); an \
         invalid value returns an error listing the valid values. lang selects the text language (de or en, \
         default en as in the export). Issues whose filters still need crawl analysis are listed in \
         not_evaluated. Paginated with limit (1 to 500, default 50) and cursor."
    }

    fn input_schema(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "crawl_id": {"type": "string", "description": "ID from list_crawls"},
                "priority": {"type": "string", "enum": ["high", "medium", "low"]},
                "type": {"type": "string", "enum": ["issue", "warning", "opportunity"],
                         "description": "issue (also: error), warning, opportunity (also: notice)"},
                "lang": {"type": "string", "enum": ["de", "en"], "default": "en"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 50},
                "cursor": {"type": "string", "description": "next_cursor of the previous response"}
            },
            "required": ["crawl_id"],
            "additionalProperties": false
        })
    }

    fn call(&self, ctx: &ToolContext, args: Json) -> Result<ToolOutput, ToolError> {
        reject_unknown(
            &args,
            &["crawl_id", "priority", "type", "lang", "limit", "cursor"],
        )?;
        let page = PageArgs::from_args(&args)?;
        let priority = parse_priority(&args)?;
        let issue_type = parse_type(&args)?;
        let lang = lang_arg(&args)?;
        let crawl = open_crawl(ctx, &args)?;

        let binding = format!(
            "list_issues|{}|{}|{}|{}",
            crawl.id,
            priority.map_or("-", |p| p.as_str()),
            issue_type.map_or("-", |t| t.as_str()),
            lang.code()
        );
        let offset = match &page.cursor {
            Some(c) => paging::decode_cursor(c, &binding)?,
            None => 0,
        };

        let counts = views::filter_counts(&crawl.file, &crawl.cfg).map_err(view_error)?;
        let all_rows = issues::issues_from_counts(&counts);
        let rows: Vec<&IssueRow> = all_rows
            .iter()
            .filter(|r| priority.is_none_or(|p| r.priority == p))
            .filter(|r| issue_type.is_none_or(|t| r.issue_type == t))
            .collect();
        let total = rows.len() as u64;
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(rows.len());
        let end = start.saturating_add(page.limit as usize).min(rows.len());

        let tab_defs = tabs::configured_for_file(&crawl.cfg, &crawl.file);
        let cfg_json = serde_json::to_value(&crawl.cfg).unwrap_or(Json::Null);
        let mut entries = Vec::with_capacity(end - start);
        let mut examples = Vec::with_capacity(end - start);
        for row in &rows[start..end] {
            let Some(entry) = issues::catalog().iter().find(|e| e.id == row.id) else {
                continue;
            };
            let Some(tab) = tab_defs.iter().find(|t| t.id == row.tab) else {
                continue;
            };
            let label = filter_label(&counts, row);
            let params = issue_text_params(entry, tab, &crawl.cfg, &cfg_json);
            let param_refs: Vec<(&str, Value)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect();
            let texts = issues::texts(&entry.doc_key, lang, &param_refs);

            let mut query = ViewQuery::new(row.tab.as_str(), row.filter.as_str());
            query.columns = ColumnSelection::Named(vec!["Address".to_string()]);
            query.limit = MAX_EXAMPLES;
            let sample = views::query_view(&crawl.file, &crawl.cfg, &query).map_err(view_error)?;
            let urls: Vec<Json> = sample
                .rows
                .iter()
                .filter_map(|r| r.first())
                .map(value_json)
                .collect();
            examples.push(json!({"id": row.id, "urls": urls}));

            entries.push(json!({
                "id": row.id,
                "name": format!("{}: {}", row.tab, label),
                "type": row.issue_type.as_str(),
                "priority": row.priority.as_str(),
                "tab": row.tab,
                "filter": label,
                "urls": row.count,
                "percent_of_tab": row.pct,
                "what": texts.as_ref().map(|t| t.what.as_str()),
                "why": texts.as_ref().map(|t| t.why.as_str()),
                "fix": texts.as_ref().map(|t| t.fix.as_str()),
                "query": {"tool": "query_urls", "tab": row.tab, "filter": label},
                "example_count": sample.rows.len(),
            }));
        }
        let next_cursor = (end < rows.len()).then(|| paging::encode_cursor(&binding, end as u64));

        // Issues, die sich nicht zählen ließen, weil ihr Filter die Crawl-Analyse oder eine Einstellung braucht.
        let not_evaluated: Vec<Json> = counts
            .iter()
            .filter(|c| c.state != os_model::FilterState::Ready)
            .filter(|c| issues::entry_for(&c.tab, &c.filter).is_some())
            .map(|c| json!({"tab": c.tab, "filter": c.label, "state": c.state.as_str()}))
            .collect();

        let mut summary = vec![
            ("crawl_id", json!(crawl.id.to_string())),
            ("lang", json!(lang.code())),
            ("issues", Json::Array(entries)),
        ];
        if !not_evaluated.is_empty() {
            summary.push(("not_evaluated", Json::Array(not_evaluated)));
        }
        Ok(ToolOutput {
            summary: object(summary),
            untrusted: Some(json!({ "examples": examples })),
            total: Some(total),
            next_cursor,
        })
    }
}

fn filter_label(counts: &[FilterCount], row: &IssueRow) -> String {
    counts
        .iter()
        .find(|c| c.tab == row.tab && c.filter == row.filter)
        .map_or_else(|| row.filter.clone(), |c| c.label.clone())
}

fn parse_priority(args: &Json) -> Result<Option<IssuePriority>, ToolError> {
    let Some(raw) = opt_str(args, "priority")? else {
        return Ok(None);
    };
    match raw.to_ascii_lowercase().as_str() {
        "high" | "hoch" => Ok(Some(IssuePriority::High)),
        "medium" | "mittel" => Ok(Some(IssuePriority::Medium)),
        "low" | "niedrig" | "gering" => Ok(Some(IssuePriority::Low)),
        _ => Err(ToolError::InvalidArguments(format!(
            "priority \"{raw}\" ist ungültig. Gültige Werte: high, medium, low."
        ))),
    }
}

fn parse_type(args: &Json) -> Result<Option<IssueType>, ToolError> {
    let Some(raw) = opt_str(args, "type")? else {
        return Ok(None);
    };
    match raw.to_ascii_lowercase().as_str() {
        "issue" | "error" | "fehler" => Ok(Some(IssueType::Issue)),
        "warning" | "warn" | "warnung" => Ok(Some(IssueType::Warning)),
        "opportunity" | "notice" | "hinweis" => Ok(Some(IssueType::Opportunity)),
        _ => Err(ToolError::InvalidArguments(format!(
            "type \"{raw}\" ist ungültig. Gültige Werte: issue (auch error), warning, opportunity (auch notice)."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AGENT-013
    #[test]
    fn priority_and_type_accept_the_documented_values_and_reject_others_with_the_valid_list() {
        assert_eq!(
            parse_priority(&json!({"priority": "HIGH"})).unwrap(),
            Some(IssuePriority::High)
        );
        assert_eq!(parse_priority(&json!({})).unwrap(), None);
        assert_eq!(
            parse_type(&json!({"type": "error"})).unwrap(),
            Some(IssueType::Issue)
        );
        assert_eq!(
            parse_type(&json!({"type": "Opportunity"})).unwrap(),
            Some(IssueType::Opportunity)
        );
        let ToolError::InvalidArguments(msg) =
            parse_priority(&json!({"priority": "urgent"})).unwrap_err()
        else {
            panic!("falscher Fehlertyp");
        };
        assert!(msg.contains("high, medium, low"), "{msg}");
        let ToolError::InvalidArguments(msg) = parse_type(&json!({"type": "fatal"})).unwrap_err()
        else {
            panic!("falscher Fehlertyp");
        };
        assert!(
            msg.contains("issue") && msg.contains("warning") && msg.contains("opportunity"),
            "{msg}"
        );
    }
}

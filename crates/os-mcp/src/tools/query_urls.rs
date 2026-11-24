//! Werkzeug `query_urls` (AGENT-011, AGENT-005): eine Seite einer Tab-Ansicht (Tab, Filter, Spalten,
//! Sortierung) – dieselbe Abfrage (`views::query_view`) wie der Tab-Export, also dieselben URLs in derselben
//! Reihenfolge bei gleicher Sortierung.
//!
//! Antwort: Spaltennamen, Zeilenschlüssel (`url_ids`) und Zähler stehen in der Zusammenfassung; die Zeilen
//! selbst (Adressen, Titel, Texte …) stammen aus gecrawlten Seiten und stehen ausschließlich im
//! Untrusted-Block.

use os_analyze::views::{self, ColumnSelection, PageStart, SearchSpec, ViewQuery};
use os_analyze::{names, tabs, SortKey, TabDef};
use os_model::FilterState;
use serde_json::{json, Value as Json};

use super::common::{
    object, open_crawl, opt_str, reject_unknown, req_str, resolve_view, row_json, view_error,
};
use crate::paging::PageArgs;
use crate::tool::{Tool, ToolClass, ToolContext, ToolError, ToolOutput};

/// Name des Werkzeugs.
pub const NAME: &str = "query_urls";

/// Das Werkzeug `query_urls`.
pub struct QueryUrls;

impl Tool for QueryUrls {
    fn name(&self) -> &'static str {
        NAME
    }

    fn class(&self) -> ToolClass {
        ToolClass::Read
    }

    fn description(&self) -> &'static str {
        "Queries the URLs of a tab and filter (the same names as in the UI and the export, e.g. tab \
         \"Response Codes\", filter \"Client Error (4xx)\"; without filter \"All\" applies). Result: column \
         names (columns), url_ids and the rows in the same order as the tab export with the same sorting. \
         Columns: default columns, \"all\" for all, or a list of column names (multi-value columns such as \
         \"Meta Robots 1\" via the numbered spelling). Sorting: list of column names or {column, descending}. \
         An invalid name returns an error listing the valid names. If a filter cannot be evaluated yet \
         (crawl analysis, configuration or rendering missing), filter_state reports the reason instead of a \
         0. Timestamps are in UTC (ISO 8601 with Z), Response Time in seconds, sizes in bytes. Paginated \
         with limit (1 to 500, default 50) and cursor; total is the number of all matches."
    }

    fn input_schema(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "crawl_id": {"type": "string", "description": "ID from list_crawls"},
                "tab": {"type": "string", "description": "Tab, e.g. \"Internal\" or \"Response Codes\"; \"Tab:Filter\" is also accepted"},
                "filter": {"type": "string", "description": "Filter of the tab, e.g. \"Client Error (4xx)\" (default \"All\"); thresholds as a number or X"},
                "columns": {
                    "description": "\"all\", \"default\" or a list of column names",
                    "oneOf": [
                        {"type": "string", "enum": ["all", "default"]},
                        {"type": "array", "items": {"type": "string"}}
                    ]
                },
                "sort": {
                    "type": "array",
                    "description": "Sorting; ties are broken by address, then by discovery order",
                    "items": {"oneOf": [
                        {"type": "string"},
                        {"type": "object", "properties": {
                            "column": {"type": "string"},
                            "descending": {"type": "boolean"}
                        }, "required": ["column"]}
                    ]}
                },
                "search": {"type": "string", "description": "Substring of the address (case-insensitive)"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 50},
                "cursor": {"type": "string", "description": "next_cursor of the previous response"}
            },
            "required": ["crawl_id", "tab"],
            "additionalProperties": false
        })
    }

    fn call(&self, ctx: &ToolContext, args: Json) -> Result<ToolOutput, ToolError> {
        reject_unknown(
            &args,
            &[
                "crawl_id", "tab", "filter", "columns", "sort", "search", "limit", "cursor",
            ],
        )?;
        let page = PageArgs::from_args(&args)?;
        let tab_arg = req_str(&args, "tab")?;
        let filter_arg = opt_str(&args, "filter")?;
        let crawl = open_crawl(ctx, &args)?;
        let tab_defs = tabs::configured_for_file(&crawl.cfg, &crawl.file);
        let (tab, filter) = resolve_view(&tab_defs, tab_arg, filter_arg)?;
        let columns = parse_columns(&args, tab)?;
        let sort = parse_sort(&args, tab)?;

        let mut query = ViewQuery::new(tab.id, filter.id);
        query.columns = columns;
        query.sort = sort;
        query.search = opt_str(&args, "search")?.map(|s| SearchSpec::Simple(s.to_string()));
        query.limit = page.limit;
        if let Some(cursor) = page.cursor {
            query.start = PageStart::Cursor(cursor);
        }

        let state =
            views::state_of(&crawl.file, &crawl.cfg, tab.id, filter.id).map_err(view_error)?;
        let result = views::query_view(&crawl.file, &crawl.cfg, &query).map_err(view_error)?;
        let label = names::label(filter.id, &crawl.cfg);

        let mut summary = vec![
            ("crawl_id", json!(crawl.id.to_string())),
            ("tab", json!(tab.id)),
            ("filter", json!(label)),
            ("export_name", json!(format!("{}:{}", tab.id, label))),
            ("columns", json!(result.columns)),
            (
                "url_ids",
                json!(result.row_keys.iter().map(|k| k.0).collect::<Vec<_>>()),
            ),
            ("rows", json!(result.rows.len())),
            ("filter_state", json!(state.as_str())),
        ];
        if let Some(note) = state_note(&state) {
            summary.push(("filter_state_note", json!(note)));
        }
        let rows: Vec<Json> = result.rows.iter().map(|r| row_json(r)).collect();
        // Die Zeilen enthalten Inhalte gecrawlter Seiten (Adressen, Titel, Texte, Header …): nur im Datenblock.
        let untrusted = json!({ "rows": rows });
        Ok(ToolOutput {
            summary: object(summary),
            untrusted: Some(untrusted),
            total: Some(result.total),
            next_cursor: result.next_cursor,
        })
    }
}

/// Erklärung zu einem Filter, der keine Zeilen liefern kann (ONPAGE-007): nie eine 0 an Stelle des Zustands.
fn state_note(state: &FilterState) -> Option<String> {
    match state {
        FilterState::Ready => None,
        FilterState::NeedsRecrawl => Some("Neu crawlen erforderlich".into()),
        FilterState::Incomplete { unchecked } => Some(format!("Unvollständig: {unchecked} ungeprüfte Seiten")),
        FilterState::NeedsCrawlAnalysis => Some(
            "Der Filter braucht die Crawl-Analyse, die für diesen Crawl nicht gelaufen ist; total und rows sind \
             deshalb 0, aber keine echte Null."
                .to_string(),
        ),
        FilterState::Stale => Some(
            "Die Crawl-Analyse ist veraltet; der Filter liefert keine Zeilen, bis sie erneut gelaufen ist."
                .to_string(),
        ),
        FilterState::NeedsConfig(key) => Some(format!(
            "Der Filter braucht die Einstellung {key}, die im Crawl ausgeschaltet war; total und rows sind \
             deshalb 0, aber keine echte Null."
        )),
        FilterState::NeedsRendering => Some(
            "Der Filter braucht JavaScript-Rendering, das in diesem Crawl nicht lief; total und rows sind \
             deshalb 0, aber keine echte Null."
                .to_string(),
        ),
    }
}

fn parse_columns(args: &Json, tab: &TabDef) -> Result<ColumnSelection, ToolError> {
    let valid = || valid_column_forms(tab);
    match args.get("columns") {
        None | Some(Json::Null) => Ok(ColumnSelection::Default),
        Some(Json::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "all" => Ok(ColumnSelection::All),
            "default" | "" => Ok(ColumnSelection::Default),
            _ => Err(ToolError::InvalidArguments(format!(
                "columns \"{s}\" ist ungültig: \"all\", \"default\" oder eine Liste von Spaltennamen. \
                 Gültige Spalten von {}: {}.",
                tab.id,
                valid()
            ))),
        },
        Some(Json::Array(items)) => {
            let mut names_list = Vec::with_capacity(items.len());
            for item in items {
                let Json::String(name) = item else {
                    return Err(ToolError::InvalidArguments(format!(
                        "columns darf nur Spaltennamen (Zeichenketten) enthalten, erhalten: {item}"
                    )));
                };
                names_list.push(name.trim().to_string());
            }
            if names_list.is_empty() {
                return Ok(ColumnSelection::Default);
            }
            check_column_names(tab, &names_list, "columns", &valid)?;
            Ok(ColumnSelection::Named(names_list))
        }
        Some(other) => Err(ToolError::InvalidArguments(format!(
            "columns muss \"all\", \"default\" oder eine Liste von Spaltennamen sein, erhalten: {other}"
        ))),
    }
}

/// Prüft Spaltennamen vorab, damit die Fehlermeldung die gültigen Namen nennt. Nummerierte Mehrfachspalten
/// (`Meta Robots 2`, `HTML hreflang 1 URL`) gelten, wenn Basis und Suffix zu einer Mehrfachspalte des Tabs passen.
fn check_column_names(
    tab: &TabDef,
    wanted: &[String],
    what: &str,
    valid: &dyn Fn() -> String,
) -> Result<(), ToolError> {
    for name in wanted {
        if column_known(tab, name) {
            continue;
        }
        let suggestions = names::column_suggestions(tab.columns.iter().map(|c| c.key), name);
        let hint = if suggestions.is_empty() {
            String::new()
        } else {
            format!(" Meinten Sie: {}?", suggestions.join(", "))
        };
        return Err(ToolError::InvalidArguments(format!(
            "{what}: unbekannte Spalte \"{name}\" im Tab {}. Gültige Spalten: {}.{hint}",
            tab.id,
            valid()
        )));
    }
    Ok(())
}

/// Gültige Spaltenformen eines Tabs für Fehlermeldungen: einfache Spalten mit Schlüssel, Mehrfachspalten als
/// `<Basis> 1…n`, verschränkte Mehrfachspalten zusätzlich je Teilspalte mit Suffix (`<Basis> 1…n URL`).
fn valid_column_forms(tab: &TabDef) -> String {
    let mut forms: Vec<String> = Vec::new();
    for c in &tab.columns {
        match &c.sql {
            os_analyze::ColumnSql::Expr(_) => forms.push(c.key.to_string()),
            os_analyze::ColumnSql::Multi { base, .. }
            | os_analyze::ColumnSql::MultiCrawlWide { base, .. } => {
                forms.push(format!("{base} 1…n"))
            }
            os_analyze::ColumnSql::MultiPaired { base, parts } => {
                for part in parts.iter() {
                    if part.suffix.is_empty() {
                        forms.push(format!("{base} 1…n"));
                    } else {
                        forms.push(format!("{base} 1…n {}", part.suffix));
                    }
                }
            }
        }
    }
    forms.join(", ")
}

/// Ob `name` eine Spalte des Tabs benennt (Schlüssel oder nummerierte Mehrfachspalte). Die Regel liegt in
/// `os-analyze`, damit Vorabprüfung und Ansicht dieselben Namen akzeptieren.
fn column_known(tab: &TabDef, name: &str) -> bool {
    os_analyze::views::column_name_resolves(tab, name)
}

fn parse_sort(args: &Json, tab: &TabDef) -> Result<Vec<SortKey>, ToolError> {
    let valid = || {
        tab.columns
            .iter()
            .map(|c| c.key.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let items = match args.get("sort") {
        None | Some(Json::Null) => return Ok(Vec::new()),
        Some(Json::Array(items)) => items,
        Some(other) => {
            return Err(ToolError::InvalidArguments(format!(
                "sort muss eine Liste sein (Spaltennamen oder {{column, descending}}), erhalten: {other}"
            )))
        }
    };
    let mut keys = Vec::with_capacity(items.len());
    for item in items {
        let (column, descending) = match item {
            Json::String(s) => (s.trim().to_string(), false),
            Json::Object(map) => {
                let column = map
                    .get("column")
                    .and_then(Json::as_str)
                    .map(|s| s.trim().to_string())
                    .ok_or_else(|| {
                        ToolError::InvalidArguments(
                            "sort: jedes Objekt braucht column (Spaltenname)".to_string(),
                        )
                    })?;
                let descending = match map.get("descending") {
                    None | Some(Json::Null) => false,
                    Some(Json::Bool(b)) => *b,
                    Some(other) => {
                        return Err(ToolError::InvalidArguments(format!(
                            "sort: descending muss true oder false sein, erhalten: {other}"
                        )))
                    }
                };
                (column, descending)
            }
            other => {
                return Err(ToolError::InvalidArguments(format!(
                "sort: Einträge sind Spaltennamen oder {{column, descending}}, erhalten: {other}"
            )))
            }
        };
        check_column_names(tab, std::slice::from_ref(&column), "sort", &valid)?;
        keys.push(SortKey { column, descending });
    }
    Ok(keys)
}

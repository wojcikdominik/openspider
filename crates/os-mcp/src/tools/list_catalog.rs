//! Discovery uses the same configured registries as query_urls and export.
use super::common::{internal, open_crawl, opt_str, reject_unknown, req_str};
use crate::paging::{decode_cursor, encode_cursor, PageArgs};
use crate::{Tool, ToolClass, ToolContext, ToolError, ToolOutput};
use os_analyze::{defs::ColumnSql, names, tabs, ColumnKind};
use os_export::catalog::{self, ExportKind};
use serde_json::{json, Value};
use std::hash::{Hash, Hasher};

pub struct ListCatalog;
/// `list_catalog` ohne Schalter „KI (Vorschau)“ (Plan M4 E1, E-2 b; M4-V01): ohne KI-Tabs und ihre Exporte/Reports.
pub struct ListCatalogWithoutAi;
const KINDS: &[&str] = &[
    "tabs",
    "columns",
    "reports",
    "bulk_exports",
    "export_formats",
];

impl Tool for ListCatalog {
    fn name(&self) -> &'static str {
        "list_catalog"
    }
    fn class(&self) -> ToolClass {
        ToolClass::Read
    }
    fn description(&self) -> &'static str {
        "Discover canonical tabs and filters, columns (tab required), reports, bulk_exports or export_formats. Uses the UI/export catalog. Optional crawl_id selects its configuration and expanded columns. Paginate with limit and cursor. ndjson applies to page_content_ndjson only."
    }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{"kind":{"type":"string","enum":KINDS},"tab":{"type":"string"},"crawl_id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":500,"default":50},"cursor":{"type":"string"}},"required":["kind"],"additionalProperties":false})
    }
    fn call(&self, ctx: &ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        list(ctx, args, true)
    }
}

impl Tool for ListCatalogWithoutAi {
    fn name(&self) -> &'static str {
        ListCatalog.name()
    }
    fn class(&self) -> ToolClass {
        ListCatalog.class()
    }
    fn description(&self) -> &'static str {
        ListCatalog.description()
    }
    fn input_schema(&self) -> Value {
        ListCatalog.input_schema()
    }
    fn call(&self, ctx: &ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        list(ctx, args, false)
    }
}

/// Ob ein Tab oder ein Name `<Tab>:<Name>` erscheint: ohne KI (`ai = false`) fehlen die KI-Tabs und alles, was ihren
/// Namen als ersten Teil trägt.
fn shows(ai: bool, name: &str) -> bool {
    ai || !os_analyze::compare::is_ai_task_tab(name.split_once(':').map_or(name, |(tab, _)| tab))
}

fn list(ctx: &ToolContext, args: Value, ai: bool) -> Result<ToolOutput, ToolError> {
    reject_unknown(&args, &["kind", "tab", "crawl_id", "limit", "cursor"])?;
    let kind = req_str(&args, "kind")?;
    if !KINDS.contains(&kind) {
        return Err(ToolError::InvalidArguments(format!(
            "kind: {}",
            KINDS.join(", ")
        )));
    }
    let crawl = opt_str(&args, "crawl_id")?
        .map(|_| open_crawl(ctx, &args))
        .transpose()?;
    let cfg = crawl.as_ref().map(|c| c.cfg.clone()).unwrap_or_default();
    let tabs = match &crawl {
        Some(c) => tabs::configured_for_file(&cfg, &c.file),
        None => tabs::configured(&cfg),
    };
    let tab_arg = opt_str(&args, "tab")?;
    let tab = tab_arg
        .map(|name| {
            names::find_tab(&tabs, name)
                .filter(|t| shows(ai, t.id))
                .ok_or_else(|| {
                    ToolError::InvalidArguments(format!(
                        "tab: {}",
                        tabs.iter()
                            .filter(|t| shows(ai, t.id))
                            .map(|t| t.id)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })
        })
        .transpose()?;
    let items: Vec<Value> = match kind {
        "tabs" => tabs.iter().filter(|t| shows(ai, t.id)).map(|t| json!({"name":t.id,"filters":t.filters.iter().map(|f| names::label(f.id, &cfg)).collect::<Vec<_>>()})).collect(),
        "columns" => {
            let tab = tab.ok_or_else(|| ToolError::InvalidArguments(format!("tab required: {}", tabs.iter().map(|t| t.id).collect::<Vec<_>>().join(", "))))?;
            let columns = if let Some(c) = &crawl {
                let mut q = os_analyze::views::ViewQuery::new(tab.id, "All");
                q.columns = os_analyze::views::ColumnSelection::All;
                os_analyze::views::column_names_in(&tabs, &c.file, &cfg, &q).map_err(internal)?
            } else { tab.columns.iter().map(|c| c.key.to_string()).collect() };
            columns.into_iter().filter_map(|name| {
                let (column, kind) = tab.columns.iter().find_map(|c| column_kind(c, &name).map(|kind| (c, kind)))?;
                let data_type = match kind {
                    ColumnKind::Int => "integer", ColumnKind::Float { .. } | ColumnKind::Seconds3 => "number", ColumnKind::Bool => "boolean", ColumnKind::Timestamp => "timestamp", ColumnKind::Url => "url", ColumnKind::Hash => "hash", ColumnKind::Text => "text",
                };
                Some(json!({"name":name,"data_type":data_type,"description":format!("{} — {} ({data_type})", tab.id, column.key)}))
            }).collect()
        }
        "reports" => catalog::list_with(ExportKind::Report, &cfg).into_iter().filter(|n| shows(ai, n)).map(Value::String).collect(),
        "bulk_exports" => catalog::list_with(ExportKind::Bulk, &cfg).into_iter().filter(|n| shows(ai, n)).map(Value::String).collect(),
        "export_formats" => catalog::page_content::formats().into_iter().map(|f| json!(f)).collect(),
        _ => unreachable!(),
    };
    let page = PageArgs::from_args(&args)?;
    // Bind to the actual catalog too: changed configuration/cardinalities cannot reuse a stale cursor.
    let mut digest = std::collections::hash_map::DefaultHasher::new();
    json!([kind, tab_arg, args.get("crawl_id"), items])
        .to_string()
        .hash(&mut digest);
    let binding = format!("list_catalog:{:016x}", digest.finish());
    let offset = page
        .cursor
        .as_deref()
        .map(|c| decode_cursor(c, &binding))
        .transpose()?
        .unwrap_or(0);
    let total = items.len() as u64;
    if offset > total {
        return Err(ToolError::InvalidArguments("cursor outside catalog".into()));
    }
    let end = (offset + u64::from(page.limit)).min(total);
    let rows = &items[offset as usize..end as usize];
    // Dynamic column/filter names can originate in imported configuration: keep them in the data block.
    Ok(ToolOutput {
        summary: json!({"kind":kind,"rows":rows.len()}),
        untrusted: Some(json!({"items":rows})),
        total: Some(total),
        next_cursor: (end < total).then(|| encode_cursor(&binding, end)),
    })
}

/// Catalog output uses canonical names: resolve both the positive index and the part suffix.
fn column_kind(column: &os_analyze::ColumnDef, name: &str) -> Option<ColumnKind> {
    let (base, parts) = match column.sql {
        ColumnSql::MultiPaired { base, parts } => (base, parts),
        ColumnSql::Multi { base, .. } | ColumnSql::MultiCrawlWide { base, .. } => {
            return (name == column.key || numbered_part(name, base, "")).then_some(column.kind);
        }
        ColumnSql::Expr(_) => return (name == column.key).then_some(column.kind),
    };
    if name == column.key {
        return parts.first().map(|p| p.kind);
    }
    parts
        .iter()
        .find(|p| numbered_part(name, base, p.suffix))
        .map(|p| p.kind)
}

fn numbered_part(name: &str, base: &str, suffix: &str) -> bool {
    let Some(rest) = name.strip_prefix(base).and_then(|s| s.strip_prefix(' ')) else {
        return false;
    };
    let (number, tail) = rest.split_once(' ').unwrap_or((rest, ""));
    tail == suffix
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
        && number.parse::<usize>().is_ok_and(|n| n > 0)
}

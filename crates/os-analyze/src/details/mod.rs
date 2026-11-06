//! Auswahlbezogene Detailquellen (OUT-010).
use crate::{ColumnKind, SortKey, ViewError};
use os_model::{CrawlConfig, UrlId, Value};
use os_store::CrawlFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailKind {
    Headers,
    Cookies,
    ConsoleLog,
    BlockedResources,
    Network,
    Spelling,
    A11yRules,
    A11yNodes,
    Duplicates,
    NGrams,
    StructuredDataItems,
    StructuredDataIssues,
    RedirectChain,
    SitemapMembership,
    CustomSearch,
    CustomExtraction,
    CustomJs,
    ApiValues,
    Forms,
    Hreflang,
    CrawlPath,
    Anchors,
    ImageInstances,
}
pub struct DetailReq<'a> {
    pub url_ids: &'a [UrlId],
    pub params: &'a serde_json::Value,
    pub sort: &'a [SortKey],
    pub offset: u64,
    pub limit: u64,
}
pub struct DetailRows {
    pub columns: Vec<(String, ColumnKind)>,
    pub rows: Vec<Vec<Value>>,
    pub total: u64,
    pub notes: Vec<(String, String)>,
}
pub type DetailFn = fn(&CrawlFile, &CrawlConfig, &DetailReq<'_>) -> Result<DetailRows, ViewError>;

macro_rules! store_source {
    ($($name:ident => $module:ident),* $(,)?) => {$(
        fn $name(file: &CrawlFile, _: &CrawlConfig, req: &DetailReq<'_>) -> Result<DetailRows, ViewError> {
            let data = os_store::queries::$module::detail_rows(file, req.url_ids, req.offset, req.limit)
                .map_err(|e| match e { os_store::StoreError::NotImplemented(owner) => ViewError::NotImplemented(owner), other => ViewError::Store(other) })?;
            Ok(DetailRows { columns: data.columns.into_iter().map(|n| (n, ColumnKind::Text)).collect(), rows: data.rows, total: data.total, notes: Vec::new() })
        }
    )*};
}
store_source! { anchors => anchors, path => crawl_path, sd => structured_data }

macro_rules! table_source {
    ($($name:ident => $table:literal),* $(,)?) => {$(
        fn $name(file: &CrawlFile, _: &CrawlConfig, req: &DetailReq<'_>) -> Result<DetailRows, ViewError> {
            table_rows(file, req, $table)
        }
    )*};
}
table_source! { headers => "http_headers", cookies => "cookies", console => "render_console",
blocked => "render_blocked", network => "render_network", forms => "forms", sitemaps => "sitemap_entries", a11y_rules => "a11y_rules",
a11y_nodes => "a11y_nodes", search => "custom_search_hits", extraction => "custom_extraction_values",
js => "custom_js_values", api => "api_values" }

/// Dispatch und Besitzer bleiben auch bei noch nicht implementierten Quellen abrufbar.
pub fn detail_source(kind: DetailKind) -> (&'static str, DetailFn) {
    if let Some(source) = crate::tabs::links::detail_source(kind) {
        return source;
    }
    use DetailKind::*;
    match kind {
        Headers => ("M2-E01", headers),
        Cookies => ("M2-E01", cookies),
        ConsoleLog => ("M2-E05", console),
        BlockedResources => ("M2-E05", blocked),
        Network => ("M2-E05", network),
        Spelling => ("M2-D19", crate::tabs::content::spelling_details),
        A11yRules => ("M2-D21", a11y_rules),
        A11yNodes => ("M2-D21", a11y_nodes),
        Duplicates => ("M2-D02", crate::tabs::duplicates::detail_rows),
        NGrams => ("M2-D02", crate::tabs::duplicates::ngram_details),
        StructuredDataItems | StructuredDataIssues => ("M2-D09", sd),
        RedirectChain => ("M2-D12", crate::tabs::response_codes::redirect_details),
        SitemapMembership => ("M2-D18", sitemaps),
        CustomSearch => ("M2-D14", search),
        CustomExtraction => ("M2-D15", extraction),
        CustomJs => ("M2-E07", js),
        ApiValues => ("M2-I06", api),
        Forms => ("M2-E01", forms),
        Hreflang => ("M2-D07", crate::tabs::hreflang::detail_rows),
        CrawlPath => ("M2-D17", path),
        Anchors => ("M2-D08", anchors),
        ImageInstances => ("M2-D03", crate::tabs::images::detail_rows),
    }
}

/// App views and paged exports share the same canonical adapters (UI-042/OUT-021).
pub fn app_detail_source(kind: DetailKind) -> (&'static str, DetailFn) {
    match kind {
        DetailKind::A11yRules => return ("M2-D21", crate::tabs::accessibility::rule_details),
        DetailKind::A11yNodes => return ("M2-D21", crate::tabs::accessibility::node_details),
        _ => {}
    }
    let (owner, source) = detail_source(kind);
    (
        owner,
        match kind {
            DetailKind::Headers => stored_header_details,
            DetailKind::Anchors => selected_anchor_details,
            DetailKind::CrawlPath => selected_crawl_paths,
            _ => source,
        },
    )
}

fn table_rows(file: &CrawlFile, req: &DetailReq<'_>, table: &str) -> Result<DetailRows, ViewError> {
    let ids = serde_json::to_string(&req.url_ids.iter().map(|id| id.0).collect::<Vec<_>>())
        .expect("integer list");
    let mut schema = file
        .conn()
        .prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = schema
        .query_map([], |r| {
            let name: String = r.get(1)?;
            let kind: String = r.get(2)?;
            Ok((
                name,
                match kind.as_str() {
                    "INTEGER" => ColumnKind::Int,
                    "REAL" => ColumnKind::Float { decimals: 3 },
                    _ => ColumnKind::Text,
                },
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
    let mut order = Vec::new();
    for key in req.sort {
        if let Some((name, kind)) = columns.iter().find(|(n, _)| n == &key.column) {
            let name = quote(name);
            let direction = if key.descending { "DESC" } else { "ASC" };
            order.push(format!("({name} IS NULL) ASC"));
            order.push(format!(
                "{name} {} {direction}",
                if matches!(kind, ColumnKind::Text) {
                    "COLLATE OS_TEXT"
                } else {
                    ""
                }
            ));
        }
    }
    order.extend(columns.iter().map(|(name, _)| quote(name)));
    let where_sql = "url_id IN (SELECT value FROM json_each(?1))";
    let total = file.conn().query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE {where_sql}"),
        [&ids],
        |r| r.get(0),
    )?;
    let mut stmt = file.conn().prepare(&format!(
        "SELECT * FROM {table} WHERE {where_sql} ORDER BY {} LIMIT ?2 OFFSET ?3",
        order.join(",")
    ))?;
    let rows = stmt
        .query_map(
            rusqlite::params![ids, req.limit.min(500), req.offset.min(i64::MAX as u64)],
            |r| {
                (0..columns.len())
                    .map(|i| {
                        Ok(match r.get_ref(i)? {
                            rusqlite::types::ValueRef::Null => Value::Null,
                            rusqlite::types::ValueRef::Integer(v) => Value::Int(v),
                            rusqlite::types::ValueRef::Real(v) => Value::Float(v),
                            rusqlite::types::ValueRef::Text(v) => {
                                Value::Text(String::from_utf8_lossy(v).into_owned())
                            }
                            rusqlite::types::ValueRef::Blob(_) => Value::Null,
                        })
                    })
                    .collect::<Result<Vec<_>, rusqlite::Error>>()
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DetailRows {
        columns,
        rows,
        total,
        notes: Vec::new(),
    })
}

pub fn url_details(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    url: UrlId,
) -> Result<Vec<(String, String)>, ViewError> {
    let mut rows = crate::tabs::directives::url_details(file, cfg, url)?;
    rows.extend(crate::tabs::hreflang::url_details(file, cfg, url)?);
    rows.extend(crate::tabs::links::url_details(file, cfg, url)?);
    Ok(rows)
}

/// Every Internal column for one URL, including external/hidden URLs. Uses the same
/// expressions and multi-column expansion as the main table (UI-031, OUT-021).
pub fn url_properties(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    url: UrlId,
) -> Result<Vec<(String, Value)>, ViewError> {
    let tabs = crate::tabs::configured_for_file(cfg, file);
    let mut q = crate::views::ViewQuery::new("Internal", "All");
    q.columns = crate::views::ColumnSelection::All;
    q.limit = 1;
    let page = crate::views::query_url_ids_in(&tabs, file, cfg, &q, &[url], true)?;
    Ok(page
        .rows
        .into_iter()
        .next()
        .map(|row| page.columns.into_iter().zip(row).collect())
        .unwrap_or_default())
}

/// Canonical HTTP-header detail rows (UI-042/OUT-021), preserving duplicate positions.
/// The generic F05 table source remains available unchanged.
pub fn stored_header_details(
    file: &CrawlFile,
    _: &CrawlConfig,
    req: &DetailReq<'_>,
) -> Result<DetailRows, ViewError> {
    let columns = [
        ("Address", ColumnKind::Url),
        ("Direction", ColumnKind::Text),
        ("Header Name", ColumnKind::Text),
        ("Header Value", ColumnKind::Text),
    ];
    let mut order = Vec::new();
    for key in req.sort {
        let (sql, text) = match key.column.as_str() {
            "Address" => ("u.address", false),
            "Direction" => ("h.direction", true),
            "Header Name" => ("h.name", true),
            "Header Value" => ("h.value", true),
            _ => {
                return Err(ViewError::Store(os_store::StoreError::InvalidInput(
                    format!("Unknown header sort column: {}", key.column),
                )))
            }
        };
        let direction = if key.descending { "DESC" } else { "ASC" };
        order.push(format!("({sql} IS NULL OR {sql}='') ASC"));
        order.push(format!(
            "{sql} {} {direction}",
            if text { "COLLATE OS_TEXT" } else { "" }
        ));
    }
    order.push("pick.rank,h.direction,h.position".into());
    let ids = serde_json::to_string(&req.url_ids.iter().map(|id| id.0).collect::<Vec<_>>())
        .expect("integer IDs");
    let from="FROM http_headers h JOIN urls u ON u.id=h.url_id JOIN (SELECT value AS id,MIN(CAST(key AS INTEGER)) AS rank FROM json_each(?1) GROUP BY value) pick ON pick.id=h.url_id";
    let total = file
        .conn()
        .query_row(&format!("SELECT COUNT(*) {from}"), [&ids], |r| r.get(0))?;
    let mut stmt = file.conn().prepare(&format!(
        "SELECT u.address,h.direction,h.name,h.value {from} ORDER BY {} LIMIT ?2 OFFSET ?3",
        order.join(",")
    ))?;
    let rows = stmt
        .query_map(
            rusqlite::params![ids, req.limit.min(500), req.offset.min(i64::MAX as u64)],
            |r| {
                let direction: String = r.get(1)?;
                let name: String = r.get(2)?;
                let raw: String = r.get(3)?;
                // Mirrors os_fetch::redact::SENSITIVE_HEADERS. N01: move the policy to
                // os-model/os-store; C7 forbids an os-analyze -> os-fetch dependency.
                let sensitive = matches!(
                    name.trim().to_ascii_lowercase().as_str(),
                    "authorization"
                        | "proxy-authorization"
                        | "cookie"
                        | "set-cookie"
                        | "set-cookie2"
                        | "x-api-key"
                        | "api-key"
                        | "x-auth-token"
                        | "x-access-token"
                );
                Ok(vec![
                    Value::Text(r.get(0)?),
                    Value::from(if direction == "request" {
                        "Request"
                    } else {
                        "Response"
                    }),
                    name.into(),
                    Value::from(if sensitive {
                        "<redacted>".to_string()
                    } else {
                        raw
                    }),
                ])
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DetailRows {
        columns: columns
            .into_iter()
            .map(|(name, kind)| (name.into(), kind))
            .collect(),
        rows,
        total,
        notes: Vec::new(),
    })
}

/// Preserves the complete selection contract at the parameter-aware anchor extension.
pub fn selected_anchor_details(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    req: &DetailReq<'_>,
) -> Result<DetailRows, ViewError> {
    let sort = req
        .sort
        .iter()
        .map(|key| os_store::queries::LinkSortKey {
            column: key.column.clone(),
            desc: key.descending,
        })
        .collect::<Vec<_>>();
    let rows = os_store::queries::anchors::selected_detail_rows(
        file,
        req.url_ids,
        req.params,
        &sort,
        req.offset,
        req.limit,
    );
    let rows = match rows {
        Err(os_store::StoreError::NotImplemented(_))
            if req.params.is_null() || req.params.as_object().is_some_and(|p| p.is_empty()) =>
        {
            return anchors(file, cfg, req);
        }
        result => result.map_err(|e| match e {
            os_store::StoreError::NotImplemented(owner) => ViewError::NotImplemented(owner),
            other => ViewError::Store(other),
        })?,
    };
    Ok(DetailRows {
        columns: rows
            .columns
            .into_iter()
            .map(|name| (name, ColumnKind::Text))
            .collect(),
        rows: rows.rows,
        total: rows.total,
        notes: Vec::new(),
    })
}

/// App path source including imported parentless start roots (UI-050, OUT-023).
pub fn selected_crawl_paths(
    file: &CrawlFile,
    _: &CrawlConfig,
    req: &DetailReq<'_>,
) -> Result<DetailRows, ViewError> {
    let mut rows = Vec::new();
    let mut total = 0u64;
    for id in req.url_ids {
        let page = os_store::queries::detail_ext::path_rows(file, id.0)?;
        // UI-050: the view reads from the start URL to the selected URL. Only the exported report (OUT-023, own code path)
        // lists the target first; `Step` keeps its meaning (0 = target) in both.
        for row in page.rows.into_iter().rev() {
            if total >= req.offset && rows.len() < req.limit.min(500) as usize {
                rows.push(row);
            }
            total += 1;
        }
    }
    Ok(DetailRows {
        columns: os_store::queries::crawl_path::HEADER
            .iter()
            .enumerate()
            .map(|(i, n)| {
                (
                    (*n).into(),
                    match i {
                        0 | 2 => ColumnKind::Url,
                        1 | 3 | 6 => ColumnKind::Int,
                        _ => ColumnKind::Text,
                    },
                )
            })
            .collect(),
        rows,
        total,
        notes: Vec::new(),
    })
}

/// Stable context keys for detail rows, independent of display sorting / filtering (U21).
pub fn app_detail_keys(
    file: &CrawlFile,
    cfg: &CrawlConfig,
    kind: DetailKind,
    req: &DetailReq<'_>,
    data: &DetailRows,
) -> Result<Option<Vec<i64>>, ViewError> {
    if kind == DetailKind::Spelling {
        return crate::tabs::content::spelling_detail_keys(file, cfg, req).map(Some);
    }
    if kind == DetailKind::A11yNodes {
        if let Some(index) = data.columns.iter().position(|(name, _)| name == "id") {
            return Ok(Some(
                data.rows
                    .iter()
                    .map(|row| match row[index] {
                        Value::Int(id) => id,
                        _ => 0,
                    })
                    .collect(),
            ));
        }
    }
    Ok(None)
}

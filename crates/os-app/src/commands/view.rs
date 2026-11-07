//! `view.*`: tab views run on the K-1 view sessions (`crate::views`, M2-K05); detail and link views keep their
//! offset queries. The command API is unchanged from M2-K06.
use super::catalog::{self, session, view_error};
use crate::{
    dto::*,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{
        error::{parse_args, to_json},
        sessions::SessionInfo,
        App, CallCtx, WindowId,
    },
    views::{Built, LinkView, TabView},
};
use futures_util::future::BoxFuture;
use os_analyze::{
    defs,
    details::{DetailKind, DetailReq},
    views,
};
use os_model::{CrawlConfig, UrlId};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

/// Builds that take longer return `Building` with a cancellable operation (UI-141: indicator after 300 ms).
pub const SYNC_BUILD: Duration = Duration::from_millis(300);
/// Matches the bounded first block requested by the grid data source.
const INITIAL_LINK_ROWS: u32 = 200;

#[derive(Clone)]
pub(crate) struct OpenView {
    pub owner: WindowId,
    pub session: SessionInfo,
    pub spec: ViewSpec,
    pub columns: Vec<String>,
    pub version: u64,
    /// K-1 session of a tab view; its published state overrides the fields above.
    pub tab: Option<Arc<TabView>>,
    /// Materialized link IDs of an inlinks/outlinks/resources view (K-1g).
    pub links: Option<Arc<LinkView>>,
    /// First rows snapshot of an offset-based detail generation.
    pub detail_seq: Arc<Mutex<Option<u64>>>,
    /// One bounded page, transferred to the first rows request without another DB read.
    initial: Arc<Mutex<Option<DataPage>>>,
}
/// Link tables with a materialized ID list; the anchor aggregation stays a detail source.
fn link_list(app: &App, s: &SessionInfo, spec: &ViewSpec) -> Option<Arc<LinkView>> {
    (matches!(
        spec,
        ViewSpec::Inlinks { .. } | ViewSpec::Outlinks { .. } | ViewSpec::Resources { .. }
    ) && detail_spec(spec).is_none())
    .then(|| LinkView::new(app, s.key.clone()))
}
#[derive(Default)]
struct Views {
    initialized: AtomicBool,
    entries: Mutex<HashMap<String, OpenView>>,
    dormant: Mutex<HashMap<String, (Duration, OpenView)>>,
}
/// Closed handles stop serving requests immediately; their tab sessions remain
/// reusable for the UI-142 idle window and share the core's memory budget.
pub(crate) fn release_closed(app: &App) {
    let now = app.config().clock.monotonic();
    app.ext::<Views>()
        .dormant
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .retain(|_, (closed, _)| now.saturating_sub(*closed) < Duration::from_secs(60));
}
fn registry(app: &App) -> std::sync::Arc<Views> {
    let views = app.ext::<Views>();
    if !views.initialized.swap(true, Ordering::SeqCst) {
        app.on_window_closed(Box::new(|app, window| {
            app.ext::<Views>()
                .entries
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .retain(|_, v| &v.owner != window);
            app.ext::<Views>()
                .dormant
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .retain(|_, (_, v)| &v.owner != window);
        }));
        app.crawl_events().subscribe(Box::new(|app, event| {
            use crate::runtime::lifecycle_bus::CrawlEvent;
            if let CrawlEvent::Closed(key) | CrawlEvent::Cleared(key) = event {
                app.ext::<Views>()
                    .entries
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .retain(|_, v| &v.session.key != key);
                app.ext::<Views>()
                    .dormant
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .retain(|_, (_, v)| &v.session.key != key);
            }
        }));
    }
    views
}
fn get_owned(app: &App, ctx: &CallCtx, id: &str) -> Result<OpenView, CmdError> {
    let mut v = registry(app)
        .entries
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(id)
        .cloned()
        .filter(|v| v.owner == ctx.window)
        .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-view-not-found"))?;
    if let Some(p) = v.tab.as_ref().and_then(|t| t.published()) {
        v.session = p.session;
        v.spec = p.spec;
        v.columns = p.columns;
        v.version = p.version;
    }
    if session(app, ctx)?.key != v.session.key {
        return Err(CmdError::new(ErrorCode::Stale, "err-view-stale"));
    }
    Ok(v)
}
pub(crate) fn get(app: &App, ctx: &CallCtx, id: &str) -> Result<OpenView, CmdError> {
    let view = get_owned(app, ctx, id)?;
    if session(app, ctx)?.config != view.session.config {
        return Err(stale());
    }
    Ok(view)
}
pub(crate) fn stale() -> CmdError {
    CmdError::new(ErrorCode::Stale, "err-view-stale")
}
pub(crate) fn sorts(sort: &[SortKey]) -> Vec<defs::SortKey> {
    sort.iter()
        .map(|s| defs::SortKey {
            column: s.column.clone(),
            descending: s.dir == SortDir::Desc,
        })
        .collect()
}
fn rule(node: &RuleNode) -> os_analyze::search::RuleNode {
    use os_analyze::search as s;
    match node {
        RuleNode::Group { op, children } => {
            let nodes = children.iter().map(rule).collect();
            if *op == RuleBool::And {
                s::RuleNode::And(nodes)
            } else {
                s::RuleNode::Or(nodes)
            }
        }
        RuleNode::Cond { column, op, value } => {
            let core = match op {
                RuleOp::Contains => s::RuleOp::Contains,
                RuleOp::NotContains => s::RuleOp::NotContains,
                RuleOp::Equals => s::RuleOp::Equals,
                RuleOp::NotEquals => s::RuleOp::NotEquals,
                RuleOp::StartsWith => s::RuleOp::StartsWith,
                RuleOp::EndsWith => s::RuleOp::EndsWith,
                RuleOp::Regex | RuleOp::NotRegex => s::RuleOp::Regex,
                RuleOp::GreaterThan => s::RuleOp::Greater,
                RuleOp::LessThan => s::RuleOp::Less,
                RuleOp::IsEmpty => s::RuleOp::IsEmpty,
                RuleOp::IsNotEmpty => s::RuleOp::IsNotEmpty,
            };
            let node = s::RuleNode::Rule(s::Rule {
                column: column.clone().unwrap_or_default(),
                op: core,
                value: value.clone().unwrap_or_default(),
            });
            if *op == RuleOp::NotRegex {
                s::RuleNode::Not(Box::new(node))
            } else {
                node
            }
        }
    }
}
pub(crate) fn tab_query(
    spec: &ViewSpec,
    columns: &[String],
    offset: u64,
    limit: u32,
) -> Result<views::ViewQuery, CmdError> {
    if let ViewSpec::Tab {
        segment, compare, ..
    } = spec
    {
        catalog::check_scope(segment, compare)?;
    }
    query_of(spec, columns, offset, limit)
}
/// K-1 session specification of a tab view. Segment and comparison come from `views::RestrictProvider`; the
/// search is evaluated as a Unicode bitmap restriction with the build's cancel token (K2-38).
pub(crate) fn session_spec(
    app: &App,
    s: &SessionInfo,
    spec: &ViewSpec,
    columns: &[String],
    cancel: os_model::CancelToken,
) -> Result<views::session::SessionSpec, CmdError> {
    let ViewSpec::Tab {
        segment, compare, ..
    } = spec
    else {
        return Err(CmdError::invalid_input("err-tab-required", "spec"));
    };
    let mut restrict = crate::views::restrictions(app, &s.key, segment.as_ref(), compare.as_ref())?;
    let q = query_of(spec, columns, 0, 1)?;
    if let Some(search) = os_analyze::search::SearchFilter::new(&q, &s.config, cancel) {
        restrict.push(Arc::new(search));
    }
    Ok(views::session::SessionSpec {
        tab: q.tab,
        filter: q.filter,
        search: None,
        sort: q.sort,
        columns: q.columns,
        restrict,
    })
}
fn query_of(
    spec: &ViewSpec,
    columns: &[String],
    offset: u64,
    limit: u32,
) -> Result<views::ViewQuery, CmdError> {
    let ViewSpec::Tab {
        tab,
        filter,
        search,
        sort,
        ..
    } = spec
    else {
        return Err(CmdError::invalid_input("err-tab-required", "spec"));
    };
    let mut q = views::ViewQuery::new(tab, filter);
    q.columns = if columns.is_empty() {
        views::ColumnSelection::Default
    } else {
        views::ColumnSelection::Named(columns.to_vec())
    };
    q.sort = sorts(sort);
    q.limit = limit;
    q.start = views::PageStart::Offset(offset);
    q.search = search.as_ref().map(|s| match s {
        SearchSpec::Simple { text } => views::SearchSpec::Simple(text.clone()),
        SearchSpec::Rules {
            tree,
            case_sensitive,
        } => views::SearchSpec::Rules {
            tree: rule(tree),
            case_sensitive: *case_sensitive,
        },
    });
    Ok(q)
}
pub(crate) fn detail_kind(detail: DetailTable) -> DetailKind {
    match detail {
        DetailTable::Headers => DetailKind::Headers,
        DetailTable::Cookies => DetailKind::Cookies,
        DetailTable::ConsoleLog => DetailKind::ConsoleLog,
        DetailTable::BlockedResources => DetailKind::BlockedResources,
        DetailTable::Network => DetailKind::Network,
        DetailTable::Spelling => DetailKind::Spelling,
        DetailTable::A11yRules => DetailKind::A11yRules,
        DetailTable::A11yNodes => DetailKind::A11yNodes,
        DetailTable::Duplicates => DetailKind::Duplicates,
        DetailTable::NGrams => DetailKind::NGrams,
        DetailTable::StructuredDataItems => DetailKind::StructuredDataItems,
        DetailTable::StructuredDataIssues => DetailKind::StructuredDataIssues,
        DetailTable::RedirectChain => DetailKind::RedirectChain,
        DetailTable::SitemapMembership => DetailKind::SitemapMembership,
        DetailTable::CustomSearch => DetailKind::CustomSearch,
        DetailTable::CustomExtraction => DetailKind::CustomExtraction,
        DetailTable::CustomJs => DetailKind::CustomJs,
        DetailTable::ApiValues => DetailKind::ApiValues,
        DetailTable::Forms => DetailKind::Forms,
        DetailTable::Hreflang => DetailKind::Hreflang,
        DetailTable::CrawlPath => DetailKind::CrawlPath,
        DetailTable::Anchors => DetailKind::Anchors,
    }
}
pub(crate) fn detail_gate(kind: DetailKind, cfg: &CrawlConfig) -> Option<EmptyState> {
    if matches!(
        kind,
        DetailKind::StructuredDataItems | DetailKind::StructuredDataIssues
    ) && !(cfg.extraction.structured_data.json_ld
        || cfg.extraction.structured_data.microdata
        || cfg.extraction.structured_data.rdfa)
    {
        return Some(catalog::empty(
            EmptyCode::FeatureDisabled,
            Some("extraction.structured_data".into()),
        ));
    }
    let feature = match kind {
        DetailKind::Headers => Some("extraction.http_headers"),
        DetailKind::Cookies => Some("extraction.cookies"),
        DetailKind::StructuredDataItems | DetailKind::StructuredDataIssues => None,
        DetailKind::Forms => Some("extraction.forms"),
        _ => None,
    };
    if let Some(k) = feature.filter(|k| !catalog::config_on(cfg, k)) {
        return Some(catalog::empty(EmptyCode::FeatureDisabled, Some(k.into())));
    }
    // M2-D19 (UI-047): ohne Rechtschreib- und Grammatikprüfung bleibt der Detail-Tab leer.
    if kind == DetailKind::Spelling && !cfg.spelling.enabled && !cfg.grammar.enabled {
        return Some(catalog::empty(
            EmptyCode::FeatureDisabled,
            Some("spelling.enabled".into()),
        ));
    }
    if matches!(
        kind,
        DetailKind::ConsoleLog
            | DetailKind::Network
            | DetailKind::BlockedResources
            | DetailKind::A11yNodes
            | DetailKind::A11yRules
    ) && cfg.rendering.mode != os_model::RenderingMode::JavaScript
    {
        return Some(catalog::empty(EmptyCode::NeedsJsRendering, None));
    }
    None
}
pub(crate) fn detail_spec(
    spec: &ViewSpec,
) -> Option<(
    DetailKind,
    Vec<UrlId>,
    serde_json::Value,
    Vec<defs::SortKey>,
)> {
    match spec {
        ViewSpec::Detail {
            detail,
            url_ids,
            params,
            sort,
        } => Some((
            detail_kind(*detail),
            url_ids.iter().copied().map(UrlId).collect(),
            params.clone(),
            sorts(sort),
        )),
        ViewSpec::Images {
            url_ids,
            search,
            sort,
        } => Some((
            DetailKind::ImageInstances,
            url_ids.iter().copied().map(UrlId).collect(),
            serde_json::json!({"search":search}),
            sorts(sort),
        )),
        ViewSpec::Inlinks {
            url_ids,
            filter,
            sort,
            search,
            ..
        }
        | ViewSpec::Outlinks {
            url_ids,
            filter,
            sort,
            search,
            ..
        } if filter.mode == Some(LinkMode::Anchors) => Some((
            DetailKind::Anchors,
            url_ids.iter().copied().map(UrlId).collect(),
            serde_json::json!({"incoming":matches!(spec,ViewSpec::Inlinks{..}),"excludeSelf":filter.exclude_self.unwrap_or(true),"excludeNofollow":filter.exclude_nofollow.unwrap_or(false),"position":filter.position,"anchor":filter.anchor,"search":search}),
            sorts(sort),
        )),
        _ => None,
    }
}
/// Additional columns of the Resources list (UI-036), appended to the 19 link columns.
const RESOURCE_EXTRA: [&str; 3] = ["Content Type", "Size", "Blocked By"];

pub(crate) fn link_selection(
    spec: &ViewSpec,
    cfg: &CrawlConfig,
    offset: u64,
    limit: u32,
) -> Result<
    (
        os_store::queries::app_support::LinkSelection,
        os_store::queries::PageReq,
    ),
    CmdError,
> {
    use os_store::queries::{app_support::LinkSelection, LinkSortKey, PageReq};
    let mut resource_filter = None;
    let (ids, filter, search, sort, incoming, resources) = match spec {
        ViewSpec::Inlinks {
            url_ids,
            filter,
            search,
            sort,
            compare,
        }
        | ViewSpec::Outlinks {
            url_ids,
            filter,
            search,
            sort,
            compare,
        } => {
            catalog::check_scope(&None, compare)?;
            (
                url_ids,
                Some(filter),
                search,
                sort,
                matches!(spec, ViewSpec::Inlinks { .. }),
                false,
            )
        }
        ViewSpec::Resources {
            url_ids,
            filter,
            search,
            sort,
        } => {
            // UI-036: type filter of the Resources list; unknown names stay an error.
            const TYPES: [&str; 8] = [
                "JavaScript",
                "CSS",
                "Images",
                "Fonts",
                "Iframes",
                "Media",
                "Other",
                "Blocked",
            ];
            if filter.id != "All" && !TYPES.contains(&filter.id.as_str()) {
                return Err(CmdError::invalid_input("err-unknown-filter", "filter"));
            }
            resource_filter = (filter.id != "All").then(|| filter.id.clone());
            (url_ids, None, search, sort, false, true)
        }
        _ => return Err(CmdError::invalid_input("err-links-required", "spec")),
    };
    let mut selection = LinkSelection {
        url_ids: ids.iter().copied().map(UrlId).collect(),
        incoming,
        resources,
        exclude_resources: !incoming && !resources,
        search: search.clone(),
        include_uncrawlable: cfg.spider.store_uncrawlable_links,
        resource_filter,
        ..Default::default()
    };
    let mut page = PageReq::new(offset, limit);
    if let Some(f) = filter {
        match f.id.as_str() {
            "All" => {}
            "Hyperlinks" => selection.link_type = Some("Hyperlink".into()),
            "Images" => selection.link_type = Some("Image".into()),
            "Internal" => selection.internal = Some(true),
            "External" => selection.internal = Some(false),
            "Uncrawlable" => selection.uncrawlable_only = true,
            "Without Self Links" => selection.exclude_self = true,
            "Without Nofollow" => selection.exclude_nofollow = true,
            _ => return Err(CmdError::invalid_input("err-unknown-filter", "filter")),
        };
        selection.exclude_self |= f.exclude_self.unwrap_or(false);
        selection.exclude_nofollow |= f.exclude_nofollow.unwrap_or(false);
        selection.anchor = f.anchor.clone();
        page.position = f.position.clone();
    }
    page.sort = sort
        .iter()
        .map(|s| LinkSortKey {
            column: match s.column.as_str() {
                "From" => "Source",
                "To" => "Destination",
                "Anchor Text" => "Anchor",
                x => x,
            }
            .into(),
            desc: s.dir == SortDir::Desc,
        })
        .collect();
    Ok((selection, page))
}
fn tab_empty(state: os_model::FilterState, total: u64, searching: bool) -> Option<EmptyState> {
    match state {
        os_model::FilterState::NeedsConfig(k) => {
            Some(catalog::empty(EmptyCode::FeatureDisabled, Some(k)))
        }
        os_model::FilterState::NeedsRendering => {
            Some(catalog::empty(EmptyCode::NeedsJsRendering, None))
        }
        os_model::FilterState::NeedsCrawlAnalysis | os_model::FilterState::Stale => {
            Some(catalog::empty(EmptyCode::NeedsCrawlAnalysis, None))
        }
        os_model::FilterState::NeedsRecrawl => Some(catalog::empty(
            EmptyCode::NoDataYet,
            Some("needsRecrawl".into()),
        )),
        _ if total == 0 => Some(catalog::empty(
            if searching {
                EmptyCode::NoMatch
            } else {
                EmptyCode::NoDataYet
            },
            None,
        )),
        _ => None,
    }
}
/// Cells of columns with a missing prerequisite stay empty.
fn cells(rows: Vec<Vec<os_model::Value>>, meta: &[ColumnMeta]) -> Vec<Vec<serde_json::Value>> {
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .zip(meta)
                .map(|(v, c)| {
                    if c.needs.is_some() {
                        serde_json::Value::Null
                    } else {
                        serde_json::to_value(v).unwrap_or_default()
                    }
                })
                .collect()
        })
        .collect()
}
#[derive(Clone)]
pub(crate) struct DataPage {
    pub keys: Vec<i64>,
    pub columns: Vec<ColumnMeta>,
    pub cells: Vec<Vec<serde_json::Value>>,
    pub total: u64,
    pub version: u64,
    pub empty: Option<EmptyState>,
}
pub(crate) async fn page(
    app: &App,
    s: &SessionInfo,
    spec: ViewSpec,
    columns: Vec<String>,
    offset: u64,
    limit: u32,
    expected: Option<u64>,
) -> Result<DataPage, CmdError> {
    page_query(
        app,
        s,
        spec,
        columns,
        offset,
        limit,
        PageSelection {
            expected,
            ids: None,
            ordered: false,
            detail_seq: None,
        },
    )
    .await
}
/// Explicit URL/link IDs are restricted in SQL before counting and paging.
pub(crate) async fn selected_page(
    app: &App,
    v: &OpenView,
    columns: Vec<String>,
    ids: Vec<i64>,
    offset: u64,
    limit: u32,
) -> Result<DataPage, CmdError> {
    page_query(
        app,
        &v.session,
        v.spec.clone(),
        columns,
        offset,
        limit,
        PageSelection {
            expected: Some(v.version),
            ids: Some(ids),
            ordered: false,
            detail_seq: None,
        },
    )
    .await
}
struct PageSelection {
    expected: Option<u64>,
    ids: Option<Vec<i64>>,
    /// `ids` are already a window of a materialized link list in view order (read by primary key).
    ordered: bool,
    detail_seq: Option<Arc<Mutex<Option<u64>>>>,
}
/// Window of a materialized link list (K-1g): rows by primary key in list order.
async fn ordered_link_page(
    app: &App,
    v: &OpenView,
    columns: Vec<String>,
    ids: Vec<i64>,
    limit: u32,
) -> Result<DataPage, CmdError> {
    page_query(
        app,
        &v.session,
        v.spec.clone(),
        columns,
        0,
        limit,
        PageSelection {
            expected: Some(v.version),
            ids: Some(ids),
            ordered: true,
            detail_seq: None,
        },
    )
    .await
}
async fn page_query(
    app: &App,
    s: &SessionInfo,
    spec: ViewSpec,
    columns: Vec<String>,
    offset: u64,
    limit: u32,
    selection: PageSelection,
) -> Result<DataPage, CmdError> {
    let PageSelection {
        expected,
        ids: selected_ids,
        ordered,
        detail_seq,
    } = selection;
    #[cfg(test)]
    app.ext::<PageReads>().0.fetch_add(1, Ordering::Relaxed);
    if !(1..=500).contains(&limit) {
        return Err(CmdError::invalid_input("err-invalid-limit", "limit"));
    }
    let cfg = s.config.clone();
    let lane = if matches!(spec, ViewSpec::Tab { .. }) {
        "table-window"
    } else {
        "detail-window"
    };
    app.read_pool()
        .run_cached(
            s.key.clone(),
            lane,
            crate::runtime::current_cancel(),
            move |file, _| {
                let _snapshot = file.conn().unchecked_transaction()?;
                let version = os_store::queries::change_seq(file.conn())?;
                Ok((|| {
                    // Offset-based detail pages must use the same database generation.
                    // Pin on the first row read (not open), so an active crawl cannot
                    // starve the first page. A changed snapshot requires view.update
                    // and a full reload; never combine offsets from different orders.
                    if let Some(seq) = &detail_seq {
                        let mut pinned = seq.lock().unwrap_or_else(|p| p.into_inner());
                        if pinned.is_some_and(|v| v != version) {
                            return Err(stale());
                        }
                        *pinned = Some(version);
                    }
                    // Tab projections pin database versions. Detail handles instead identify
                    // a selection/query generation: unrelated crawl writes must not make
                    // their first row request permanently stale. Every page still uses a
                    // single read snapshot; explicit outdated handles are rejected by rows().
                    if matches!(spec, ViewSpec::Tab { .. })
                        && expected.is_some_and(|v| v != version)
                    {
                        return Err(stale());
                    }
                    let version = expected.unwrap_or(version);
                    if let ViewSpec::Tab {
                        tab,
                        filter,
                        search,
                        ..
                    } = &spec
                    {
                        let mut q = tab_query(&spec, &columns, offset, limit)?;
                        if columns.is_empty() {
                            q.columns = views::ColumnSelection::Default;
                        }
                        let registry = os_analyze::tabs::configured_for_file(&cfg, file);
                        let p = if let Some(ids) = &selected_ids {
                            views::query_url_ids_in(
                                &registry,
                                file,
                                &cfg,
                                &q,
                                &ids.iter().copied().map(UrlId).collect::<Vec<_>>(),
                                false,
                            )
                        } else {
                            views::query_view_in(&registry, file, &cfg, &q)
                        }
                        .map_err(view_error)?;
                        let t = os_analyze::names::find_tab(&registry, tab).expect("resolved");
                        let meta = catalog::expanded_columns(t, &p.columns, &cfg);
                        let state = views::state_of_in(&registry, file, &cfg, tab, filter)
                            .map_err(view_error)?;
                        let empty = tab_empty(state, p.total, search.is_some());
                        let cells = cells(p.rows, &meta);
                        return Ok(DataPage {
                            keys: p.row_keys.iter().map(|id| id.0).collect(),
                            columns: meta,
                            cells,
                            total: p.total,
                            version,
                            empty,
                        });
                    }
                    let (names, kinds, rows, keys, total, mut empty) =
                        if let Some((kind, ids, params, sort)) = detail_spec(&spec) {
                            let gate = if ids.is_empty() {
                                Some(catalog::empty(EmptyCode::NoSelection, None))
                            } else {
                                detail_gate(kind, &cfg)
                            };
                            if gate.is_some() {
                                return Ok(DataPage {
                                    keys: vec![],
                                    columns: vec![],
                                    cells: vec![],
                                    total: 0,
                                    version,
                                    empty: gate,
                                });
                            }
                            let req = DetailReq {
                                url_ids: &ids,
                                params: &params,
                                sort: &sort,
                                offset,
                                limit: u64::from(limit),
                            };
                            let (_, source) = os_analyze::details::app_detail_source(kind);
                            let p = source(file, &cfg, &req).map_err(view_error)?;
                            let context_keys =
                                os_analyze::details::app_detail_keys(file, &cfg, kind, &req, &p)
                                    .map_err(view_error)?;
                            let (names, kinds): (Vec<_>, Vec<_>) = p.columns.into_iter().unzip();
                            let keys = (0..p.rows.len())
                                .map(|i| {
                                    offset
                                        .saturating_add(i as u64)
                                        .saturating_add(1)
                                        .min(i64::MAX as u64)
                                        as i64
                                })
                                .collect();
                            let keys = context_keys.unwrap_or(keys);
                            let keys = if kind == DetailKind::A11yNodes {
                                os_analyze::tabs::accessibility::node_keys(file, &req)
                                    .map_err(view_error)?
                            } else {
                                keys
                            };
                            (names, kinds, p.rows, keys, p.total, None)
                        } else {
                            let (mut selection, req) = link_selection(&spec, &cfg, offset, limit)?;
                            let p = match selected_ids {
                                Some(ids) if ordered => {
                                    os_store::queries::app_support::selected_links_by_ids(
                                        file.conn(),
                                        &selection,
                                        &ids,
                                    )
                                }
                                ids => {
                                    selection.row_ids = ids;
                                    os_store::queries::app_support::selected_links(
                                        file.conn(),
                                        &selection,
                                        &req,
                                    )
                                }
                            }
                            .map_err(|e| crate::runtime::error::store_error(&e))?;
                            let mut p = p;
                            // UI-036: the Resources list adds Content Type, Size and Blocked By to the link columns.
                            let resource_list = matches!(spec, ViewSpec::Resources { .. });
                            if resource_list {
                                let extras = os_store::queries::app_support::resource_extras(
                                    file.conn(),
                                    &p.keys,
                                )
                                .map_err(|e| crate::runtime::error::store_error(&e))?;
                                for (row, values) in p.rows.iter_mut().zip(extras) {
                                    row.extend(values);
                                }
                            }
                            let names = [
                                "Type",
                                "From",
                                "To",
                                "Size (Bytes)",
                                "Alt Text",
                                "Anchor Text",
                                "Status Code",
                                "Status",
                                "Follow",
                                "Target",
                                "Rel",
                                "Path Type",
                                "Link Path",
                                "Link Position",
                                "Link Origin",
                                "Link Crawlability",
                                "Uncrawlable Reason",
                                "Original URL",
                                "PDF Page",
                            ]
                            .iter()
                            .map(|n| n.to_string())
                            .chain(
                                resource_list
                                    .then(|| RESOURCE_EXTRA.map(String::from))
                                    .into_iter()
                                    .flatten(),
                            )
                            .collect::<Vec<_>>();
                            let kinds = (0..names.len())
                                .map(|i| match i {
                                    1 | 2 | 17 => defs::ColumnKind::Url,
                                    3 | 6 | 18 | 20 => defs::ColumnKind::Int,
                                    8 => defs::ColumnKind::Bool,
                                    _ => defs::ColumnKind::Text,
                                })
                                .collect();
                            let empty = selection
                                .url_ids
                                .is_empty()
                                .then(|| catalog::empty(EmptyCode::NoSelection, None));
                            (names, kinds, p.rows, p.keys, p.total, empty)
                        };
                    let meta = names
                        .iter()
                        .zip(kinds)
                        .enumerate()
                        .map(|(i, (n, k))| {
                            let mut c = defs::ColumnDef::dynamic(n, "NULL", k, true);
                            defs::apply_meta_defaults(std::slice::from_mut(&mut c));
                            if (i >= 16
                                && !RESOURCE_EXTRA.contains(&n.as_str())
                                && matches!(
                                    spec,
                                    ViewSpec::Inlinks { .. }
                                        | ViewSpec::Outlinks { .. }
                                        | ViewSpec::Resources { .. }
                                ))
                                || (n == "Link Crawlability"
                                    && matches!(spec, ViewSpec::Inlinks { .. }))
                            {
                                c.extension = true;
                                c.default_visible = false;
                            }
                            // M2-D19 (ONPAGE-157): `Page Section` und `Rule ID` erst nach Einblenden.
                            if matches!(
                                spec,
                                ViewSpec::Detail {
                                    detail: DetailTable::Spelling,
                                    ..
                                }
                            ) && os_analyze::tabs::content::SPELLING_DETAIL_EXTENSIONS
                                .contains(&n.as_str())
                            {
                                c.extension = true;
                                c.default_visible = false;
                            }
                            if matches!(spec, ViewSpec::Images { .. }) {
                                if matches!(n.as_str(), "Alt Text Length" | "Source") {
                                    c.extension = true;
                                    c.default_visible = false;
                                }
                                if n == "Display Dimensions (WxH)"
                                    || (n == "Real Dimensions (WxH)"
                                        && !cfg.images.real_dimensions_from_header)
                                {
                                    c.needs = Some(defs::Needs::Rendering);
                                }
                            }
                            catalog::column(&c, n, &cfg)
                        })
                        .collect::<Vec<_>>();
                    let selected = if columns.is_empty() {
                        meta.iter()
                            .enumerate()
                            .filter(|(_, c)| c.default_visible)
                            .map(|(i, _)| i)
                            .collect::<Vec<_>>()
                    } else {
                        columns
                            .iter()
                            .map(|name| {
                                names.iter().position(|n| n == name).ok_or_else(|| {
                                    CmdError::invalid_input("err-unknown-column", "columns")
                                        .with_arg("column", name.clone())
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?
                    };
                    let cells = rows
                        .iter()
                        .map(|row| {
                            selected
                                .iter()
                                .map(|i| serde_json::to_value(&row[*i]).unwrap_or_default())
                                .collect()
                        })
                        .collect();
                    if empty.is_none() && total == 0 {
                        empty = Some(catalog::empty(EmptyCode::NoDataYet, None));
                    }
                    Ok(DataPage {
                        keys,
                        columns: selected.iter().map(|i| meta[*i].clone()).collect(),
                        cells,
                        total,
                        version,
                        empty,
                    })
                })())
            },
        )
        .await?
}
fn open<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if let Some(result) = super::compare::change_view(app, &ctx, "view.open", &args).await {
            return result;
        }
        let args: ViewOpenArgs = parse_args(args)?;
        let s = session(app, &ctx)?;
        let id = app
            .config()
            .ids
            .new_crawl_id(app.config().clock.now())
            .to_string();
        release_closed(app);
        let cached = {
            let views = registry(app);
            let mut dormant = views.dormant.lock().unwrap_or_else(|p| p.into_inner());
            let id = dormant.iter().filter(|(_, (_, v))| {
                v.owner == ctx.window
                    && v.session.key == s.key
                    && v.session.path == s.path
                    && v.session.config == s.config
                    && matches!((&v.spec, &args.spec), (ViewSpec::Tab { tab: a, .. }, ViewSpec::Tab { tab: b, .. }) if a == b)
            }).max_by_key(|(_, (closed, v))| (v.spec == args.spec && v.columns == args.columns, *closed))
                .map(|(id, _)| id.clone());
            id.and_then(|id| dormant.remove(&id).map(|(_, v)| v))
        };
        if let Some(v) = cached {
            let tab = v.tab.as_ref().expect("only tab views are retained");
            if let Some(built) = tab.cached(app).await? {
                tab.rebind(id.clone());
                let h = if v.spec == args.spec && v.columns == args.columns {
                    tab_handle(id.clone(), &args.spec, built)
                } else {
                    settle(
                        app,
                        &id,
                        &args.spec,
                        start_build(app, tab, s.clone(), args.spec.clone(), args.columns.clone()),
                    )
                    .await?
                };
                insert(
                    app,
                    id,
                    OpenView {
                        session: s,
                        spec: args.spec,
                        columns: args.columns,
                        version: h.version,
                        ..v
                    },
                );
                return to_json(&h);
            }
            // A reset may lower change_seq. Rebuild under a new handle so the
            // client watermark of the old snapshot remains valid.
        }
        if matches!(args.spec, ViewSpec::Tab { .. }) {
            let tab = TabView::new(app, id.clone(), s.key.clone());
            let built = start_build(
                app,
                &tab,
                s.clone(),
                args.spec.clone(),
                args.columns.clone(),
            );
            let h = settle(app, &id, &args.spec, built).await?;
            insert(
                app,
                id,
                OpenView {
                    owner: ctx.window,
                    session: s,
                    spec: args.spec,
                    columns: args.columns,
                    version: h.version,
                    tab: Some(tab),
                    links: None,
                    detail_seq: Arc::default(),
                    initial: Arc::default(),
                },
            );
            return to_json(&h);
        }
        let links = link_list(app, &s, &args.spec);
        let p = detail_page(
            app,
            &s,
            args.spec.clone(),
            args.columns.clone(),
            links.as_ref(),
        )
        .await?;
        let initial = Arc::new(Mutex::new(links.as_ref().map(|_| p.clone())));
        let mut h = handle(id.clone(), p);
        let opened = OpenView {
            owner: ctx.window,
            session: s,
            spec: args.spec,
            columns: args.columns,
            version: h.version,
            tab: None,
            links,
            detail_seq: Arc::default(),
            initial,
        };
        if opened.links.is_some() {
            h.initial_rows = initial_rows(&opened, &id, h.version, 0, INITIAL_LINK_ROWS, &[]);
        }
        insert(app, id, opened);
        to_json(&h)
    })
}
/// Materialize a link generation once, including its count. In a live crawl the
/// incoming-link index can be deferred: COUNT + OFFSET followed by a lazy ID
/// build used to scan the growing links table three times before showing rows.
async fn detail_page(
    app: &App,
    s: &SessionInfo,
    spec: ViewSpec,
    columns: Vec<String>,
    links: Option<&Arc<LinkView>>,
) -> Result<DataPage, CmdError> {
    let Some(links) = links else {
        return page(app, s, spec, columns, 0, 1, None).await;
    };
    let (selection, req) = link_selection(&spec, &s.config, 0, 1)?;
    let ids = links.ids(app, selection, req).await?;
    let mut p = page_query(
        app,
        s,
        spec,
        columns,
        0,
        INITIAL_LINK_ROWS,
        PageSelection {
            expected: None,
            ids: Some(
                ids.iter()
                    .take(INITIAL_LINK_ROWS as usize)
                    .copied()
                    .collect(),
            ),
            ordered: true,
            detail_seq: None,
        },
    )
    .await?;
    p.total = ids.len() as u64;
    Ok(p)
}
async fn link_ids(app: &App, v: &OpenView, links: &LinkView) -> Result<Arc<Vec<i64>>, CmdError> {
    let (selection, page) = link_selection(&v.spec, &v.session.config, 0, 1)?;
    links.ids(app, selection, page).await
}
fn insert(app: &App, id: String, view: OpenView) {
    registry(app)
        .entries
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(id, view);
}
type Building = tokio::task::JoinHandle<Result<Built, CmdError>>;
/// Starts a private build; the published view stays readable until it succeeds (double buffer).
fn start_build(
    app: &App,
    tab: &Arc<TabView>,
    s: SessionInfo,
    spec: ViewSpec,
    columns: Vec<String>,
) -> (Building, os_model::CancelToken) {
    let token = os_model::CancelToken::default();
    let (app, tab, cancel) = (app.clone(), tab.clone(), token.clone());
    let task = app
        .handle()
        .spawn(async move { tab.build(&app, s, spec, columns, cancel).await });
    (task, token)
}
/// Waits up to [`SYNC_BUILD`]; slower builds continue as a cancellable operation (`ViewState::Building`).
async fn settle(
    app: &App,
    id: &str,
    spec: &ViewSpec,
    (mut task, token): (Building, os_model::CancelToken),
) -> Result<ViewHandle, CmdError> {
    match tokio::time::timeout(SYNC_BUILD, &mut task).await {
        Ok(joined) => {
            let built = joined
                .map_err(|e| crate::runtime::error::internal("view.build", e.to_string()))??;
            Ok(tab_handle(id.to_string(), spec, built))
        }
        Err(_) => {
            let (view_id, spec) = (id.to_string(), spec.clone());
            let op = app.ops().spawn("view-build", true, move |cancel, _| {
                Box::pin(async move {
                    let forward = token.clone();
                    let _bridge = cancel.on_cancel(move || forward.cancel());
                    if cancel.is_cancelled() {
                        token.cancel();
                    }
                    let built = task.await.map_err(|e| {
                        crate::runtime::error::internal("view.build", e.to_string())
                    })??;
                    to_json(&tab_handle(view_id, &spec, built))
                })
            });
            Ok(ViewHandle {
                initial_rows: None,
                view_id: id.to_string(),
                version: 0,
                total: None,
                columns: Vec::new(),
                state: ViewState::Building,
                op: Some(op),
                empty_state: None,
            })
        }
    }
}
fn tab_handle(view_id: String, spec: &ViewSpec, built: Built) -> ViewHandle {
    let searching = matches!(
        spec,
        ViewSpec::Tab {
            search: Some(_),
            ..
        }
    );
    ViewHandle {
        initial_rows: None,
        view_id,
        version: built.version,
        total: Some(built.total),
        empty_state: tab_empty(built.state, built.total, searching),
        columns: built.meta,
        state: ViewState::Ready,
        op: None,
    }
}
fn handle(view_id: String, p: DataPage) -> ViewHandle {
    ViewHandle {
        initial_rows: None,
        view_id,
        version: p.version,
        total: Some(p.total),
        columns: p.columns,
        state: ViewState::Ready,
        op: None,
        empty_state: p.empty,
    }
}
fn update<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if let Some(result) = super::compare::change_view(app, &ctx, "view.update", &args).await {
            return result;
        }
        let args: ViewUpdateArgs = parse_args(args)?;
        let mut v = get_owned(app, &ctx, &args.view_id)?;
        v.session = session(app, &ctx)?;
        let tab = match (&args.spec, v.tab.take()) {
            (ViewSpec::Tab { .. }, Some(tab)) => Some(tab),
            (ViewSpec::Tab { .. }, _) => Some(TabView::new(
                app,
                args.view_id.clone(),
                v.session.key.clone(),
            )),
            _ => None,
        };
        let links = link_list(app, &v.session, &args.spec);
        let mut initial = None;
        let h = if let Some(tab) = &tab {
            let built = start_build(
                app,
                tab,
                v.session.clone(),
                args.spec.clone(),
                v.columns.clone(),
            );
            settle(app, &args.view_id, &args.spec, built).await?
        } else {
            let p = detail_page(
                app,
                &v.session,
                args.spec.clone(),
                v.columns.clone(),
                links.as_ref(),
            )
            .await?;
            if links.is_some() {
                initial = Some(p.clone());
            }
            handle(args.view_id.clone(), p)
        };
        let state = registry(app);
        let mut entries = state.entries.lock().unwrap_or_else(|p| p.into_inner());
        if !entries.contains_key(&args.view_id) {
            return Err(CmdError::new(ErrorCode::NotFound, "err-view-not-found"));
        }
        v.links = links;
        v.detail_seq = Arc::default();
        v.initial = Arc::new(Mutex::new(initial));
        v.spec = args.spec;
        v.version = h.version;
        v.tab = tab;
        entries.insert(args.view_id.clone(), v);
        to_json(&h)
    })
}
fn rows<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if args.get("comparison").is_some() {
            return super::compare::ui::rows(app, ctx, args).await;
        }
        if let Some(result) = super::compare::change_view(app, &ctx, "view.rows", &args).await {
            return result;
        }
        let args: RowsArgs = parse_args(args)?;
        let v = get(app, &ctx, &args.view_id)?;
        if !(1..=500).contains(&args.limit) {
            return Err(CmdError::invalid_input("err-invalid-limit", "limit"));
        }
        if args.version != v.version {
            return Err(stale());
        }
        if let Some(tab) = &v.tab {
            let w = tab.rows(app, args.version, args.offset, args.limit).await?;
            let projected = w.meta.iter().map(|c| c.key.clone()).collect::<Vec<_>>();
            if !args.columns.is_empty() && args.columns != projected {
                // Other columns than the projection: same row keys, values read by ID.
                let p = selected_page(app, &v, args.columns, w.keys.clone(), 0, args.limit).await?;
                let by_key = p.keys.iter().zip(p.cells).collect::<HashMap<_, _>>();
                let width = p.columns.len();
                return to_json(&RowWindow {
                    view_id: args.view_id,
                    version: args.version,
                    offset: args.offset,
                    cells: w
                        .keys
                        .iter()
                        .map(|k| {
                            by_key
                                .get(k)
                                .cloned()
                                .unwrap_or_else(|| vec![serde_json::Value::Null; width])
                        })
                        .collect(),
                    row_keys: w.keys,
                    columns: p.columns.into_iter().map(|c| c.key).collect(),
                });
            }
            return to_json(&RowWindow {
                view_id: args.view_id,
                version: args.version,
                offset: args.offset,
                row_keys: w.keys,
                columns: projected,
                cells: cells(w.rows, &w.meta),
            });
        }
        let columns = if args.columns.is_empty() {
            v.columns.clone()
        } else {
            args.columns
        };
        if let Some(window) = initial_rows(
            &v,
            &args.view_id,
            args.version,
            args.offset,
            args.limit,
            &columns,
        ) {
            return to_json(&window);
        }
        let p = if let Some(links) = &v.links {
            let ids = link_ids(app, &v, links).await?;
            let start = (args.offset.min(ids.len() as u64)) as usize;
            let end = start.saturating_add(args.limit as usize).min(ids.len());
            ordered_link_page(app, &v, columns, ids[start..end].to_vec(), args.limit).await?
        } else {
            page_query(
                app,
                &v.session,
                v.spec,
                columns,
                args.offset,
                args.limit,
                PageSelection {
                    expected: Some(args.version),
                    ids: None,
                    ordered: false,
                    detail_seq: Some(v.detail_seq),
                },
            )
            .await?
        };
        to_json(&RowWindow {
            view_id: args.view_id,
            version: p.version,
            offset: args.offset,
            row_keys: p.keys,
            columns: p.columns.into_iter().map(|c| c.key).collect(),
            cells: p.cells,
        })
    })
}
fn initial_rows(
    v: &OpenView,
    id: &str,
    version: u64,
    offset: u64,
    limit: u32,
    columns: &[String],
) -> Option<RowWindow> {
    let mut initial = v.initial.lock().unwrap_or_else(|p| p.into_inner());
    let page = initial.as_ref()?;
    let projected = page
        .columns
        .iter()
        .map(|c| c.key.clone())
        .collect::<Vec<_>>();
    let end = offset.saturating_add(u64::from(limit)).min(page.total);
    if (!columns.is_empty() && columns != projected) || offset > end || end > page.keys.len() as u64
    {
        return None;
    }
    let range = offset as usize..end as usize;
    let window = RowWindow {
        view_id: id.to_string(),
        version,
        offset,
        columns: projected,
        row_keys: page.keys[range.clone()].to_vec(),
        cells: page.cells[range].to_vec(),
    };
    // The frontend now owns this block; do not retain a duplicate page per view.
    initial.take();
    Some(window)
}
fn locate<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if let Some(result) = super::compare::change_view(app, &ctx, "view.locate", &args).await {
            return result;
        }
        let args: LocateArgs = parse_args(args)?;
        let v = get(app, &ctx, &args.view_id)?;
        if let Some(tab) = &v.tab {
            return to_json(&tab.locate(app, v.version, args.url_id).await?);
        }
        if let Some(links) = &v.links {
            let ids = link_ids(app, &v, links).await?;
            return to_json(
                &ids.iter()
                    .position(|id| *id == args.url_id)
                    .map(|i| i as u64),
            );
        }
        let mut offset = 0;
        loop {
            let p = page(
                app,
                &v.session,
                v.spec.clone(),
                v.columns.clone(),
                offset,
                500,
                Some(v.version),
            )
            .await?;
            if let Some(i) = p.keys.iter().position(|id| *id == args.url_id) {
                return to_json(&Some(offset + i as u64));
            }
            offset += p.keys.len() as u64;
            if p.keys.is_empty() || offset >= p.total {
                return to_json(&Option::<u64>::None);
            }
        }
    })
}
/// Ab dieser Größe wird eine vollständig überdeckte View als „alle“ ausgewählt statt als Liste von Kennungen
/// (gleiche Grenze wie der Zeilencache des Clients, UI-144).
const RANGE_ALL_ABOVE: u64 = 20_000;
/// Bound the IPC response like the client row cache (UI-144); larger partial ranges use the export hint (UI-012).
pub(crate) fn range_requires_export(from_row: u64, to_row: u64) -> bool {
    from_row.abs_diff(to_row) >= RANGE_ALL_ABOVE
}
pub(crate) fn range_covers_large(from_row: u64, to_row: u64, total: u64) -> bool {
    total > RANGE_ALL_ABOVE && from_row.min(to_row) == 0 && from_row.max(to_row) == total - 1
}
/// Wie oft `view.range` neu ansetzt, wenn die Pflege die View zwischen Lesen der Version und des Stands weiterschaltet.
/// Beides liegt Mikrosekunden auseinander; die Wiederholung kostet keine Rundreise.
const RANGE_RETRIES: usize = 16;
/// `view.range`: Bereichsauswahl im laufenden Crawl (UI-012, UI-018). Anker und Ziel bleiben URL-Kennungen; Position
/// und Kennungen dazwischen stammen aus einem einzigen Stand, unabhängig davon, wie schnell die Version wechselt.
fn range<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if let Some(result) = super::compare::change_view(app, &ctx, "view.range", &args).await {
            return result;
        }
        let args: RangeArgs = parse_args(args)?;
        let mut attempt = 0;
        let (version, from_row, to_row, total, keys) = loop {
            let v = get(app, &ctx, &args.view_id)?;
            let result = if let Some(tab) = &v.tab {
                tab.range(app, v.version, args.from, args.to)
                    .await
                    .map(|(a, b, total, keys)| (v.version, a, b, total, keys))
            } else {
                detail_range(app, &v, &args).await
            };
            match result {
                Err(e) if e.code == ErrorCode::Stale && attempt < RANGE_RETRIES => attempt += 1,
                other => break other?,
            }
        };
        let all =
            matches!((from_row, to_row), (Some(a), Some(b)) if range_covers_large(a, b, total));
        to_json(&KeyRange {
            view_id: args.view_id,
            version,
            from_row,
            to_row,
            total,
            keys,
            all,
            export_required: !all
                && from_row
                    .zip(to_row)
                    .is_some_and(|(a, b)| range_requires_export(a, b)),
        })
    })
}
/// Detail- und Linktabellen: materialisierte Link-IDs oder ein Lauf über die Seiten des Stands (wie `view.locate`).
async fn detail_range(
    app: &App,
    v: &OpenView,
    args: &RangeArgs,
) -> Result<(u64, Option<u64>, Option<u64>, u64, Vec<i64>), CmdError> {
    let ids = if let Some(links) = &v.links {
        link_ids(app, v, links).await?.as_ref().clone()
    } else {
        let mut ids = Vec::new();
        let generation = Arc::new(Mutex::new(None));
        loop {
            let p = page_query(
                app,
                &v.session,
                v.spec.clone(),
                v.columns.clone(),
                ids.len() as u64,
                500,
                PageSelection {
                    expected: Some(v.version),
                    ids: None,
                    ordered: false,
                    detail_seq: Some(generation.clone()),
                },
            )
            .await?;
            #[cfg(test)]
            if let Some(write) = app.ext::<RangePageWrite>().0.lock().unwrap().take() {
                write();
            }
            let done = p.keys.is_empty() || ids.len() as u64 + p.keys.len() as u64 >= p.total;
            ids.extend(p.keys);
            if done {
                break ids;
            }
        }
    };
    let position = |id: i64| ids.iter().position(|k| *k == id).map(|i| i as u64);
    let (from_row, to_row) = (position(args.from), position(args.to));
    let total = ids.len() as u64;
    let keys = match (from_row, to_row) {
        (Some(a), Some(b)) if !range_requires_export(a, b) => {
            ids[a.min(b) as usize..=a.max(b) as usize].to_vec()
        }
        _ => Vec::new(),
    };
    Ok((v.version, from_row, to_row, total, keys))
}
fn copy<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if let Some(result) = super::compare::change_copy(app, &ctx, &args).await {
            return result;
        }
        let args: CopyArgs = parse_args(args)?;
        crate::clipboard::copy(app, &ctx, args)
            .await
            .and_then(|r| to_json(&r))
    })
}
fn close<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if let Some(result) = super::compare::change_view(app, &ctx, "view.close", &args).await {
            return result;
        }
        let args: ViewCloseArgs = parse_args(args)?;
        let views = registry(app);
        let mut entries = views.entries.lock().unwrap_or_else(|p| p.into_inner());
        if !entries
            .get(&args.view_id)
            .is_some_and(|v| v.owner == ctx.window)
        {
            return Err(CmdError::new(ErrorCode::NotFound, "err-view-not-found"));
        }
        let v = entries.remove(&args.view_id).expect("checked owner");
        drop(entries);
        if v.tab
            .as_ref()
            .and_then(|tab| tab.published())
            .is_some_and(|p| p.spec == v.spec)
        {
            let mut dormant = views.dormant.lock().unwrap_or_else(|p| p.into_inner());
            // Bound handle metadata too; materialized data obeys the shared LRU.
            if dormant.len() >= 32 {
                if let Some(oldest) = dormant
                    .iter()
                    .min_by_key(|(_, (closed, _))| *closed)
                    .map(|(id, _)| id.clone())
                {
                    dormant.remove(&oldest);
                }
            }
            dormant.insert(args.view_id, (app.config().clock.monotonic(), v));
        }
        to_json(&())
    })
}
pub fn register(r: &mut Registry) {
    for (name, args, ret, h) in [
        (
            "view.open",
            "ViewOpenArgs",
            "ViewHandle",
            open as crate::registry::Handler,
        ),
        ("view.update", "ViewUpdateArgs", "ViewHandle", update),
        ("view.rows", "RowsArgs", "RowWindow", rows),
        ("view.locate", "LocateArgs", "number | null", locate),
        ("view.range", "RangeArgs", "KeyRange", range),
        ("view.copy", "CopyArgs", "CopyResult", copy),
        ("view.close", "ViewCloseArgs", "void", close),
    ] {
        r.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running: false,
            },
            h,
        );
    }
}

#[cfg(test)]
#[derive(Default)]
struct PageReads(std::sync::atomic::AtomicUsize);

#[cfg(test)]
mod tests {
    use super::*;
    use crate as os_app;
    include!("../../tests/cmd_copy_page_fixture.rs");

    // spec: UI-012
    // spec: UI-140
    #[tokio::test]
    async fn two_selected_ids_use_one_restricted_page_in_a_large_view() {
        assert_copy_uses_one_page(|app| app.ext::<PageReads>().0.load(Ordering::Relaxed)).await;
    }
}

#[cfg(test)]
#[derive(Default)]
struct RangePageWrite(Mutex<Option<Box<dyn FnOnce() + Send>>>);

#[cfg(test)]
#[path = "../../tests/cmd_catalog_fixture.rs"]
mod ldfollow_fixture;

#[cfg(test)]
#[path = "../../tests/ldfollow/pages.rs"]
mod ldfollow_tests;

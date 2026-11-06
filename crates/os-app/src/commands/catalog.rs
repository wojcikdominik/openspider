//! Catalog commands backed by the configured analyze registry (M2-K06).
use crate::{
    dto::*,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{
        error::{parse_args, rusqlite_error, store_error, to_json},
        sessions::SessionInfo,
        App, CallCtx,
    },
};
use futures_util::future::BoxFuture;
use os_analyze::{defs, names, tabs, views, ViewError};
use os_model::{CrawlConfig, FilterState};

pub(crate) fn session(app: &App, ctx: &CallCtx) -> Result<SessionInfo, CmdError> {
    app.sessions()
        .current(&ctx.window)
        .ok_or_else(|| CmdError::new(ErrorCode::NoCrawl, "err-no-crawl"))
}
pub(crate) fn view_error(e: ViewError) -> CmdError {
    match e {
        ViewError::Store(e) => store_error(&e),
        ViewError::Sql(e) => rusqlite_error(&e),
        ViewError::Name(os_analyze::NameError::Unknown { input, suggestions }) => {
            CmdError::invalid_input("err-unknown-name", "name")
                .with_arg("input", input)
                .with_arg("suggestions", serde_json::json!(suggestions))
        }
        ViewError::InvalidLimit(_) => CmdError::invalid_input("err-invalid-limit", "limit"),
        ViewError::NotImplemented(owner) => CmdError::unsupported("view", owner),
        ViewError::Search(os_analyze::search::SearchError::Cancelled) => {
            CmdError::new(ErrorCode::Cancelled, "err-cancelled")
        }
        ViewError::Search(os_analyze::search::SearchError::Deadline) => {
            CmdError::new(ErrorCode::Timeout, "err-search-timeout")
        }
        e => CmdError::invalid_input("err-view-query", "spec").with_detail(e.to_string()),
    }
}
pub(crate) fn check_scope(
    segment: &Option<SegmentFilter>,
    compare: &Option<CompareClass>,
) -> Result<(), CmdError> {
    if segment.is_some() {
        return Err(CmdError::unsupported("segment filter", "M2-K14"));
    }
    if compare.is_some() {
        return Err(CmdError::unsupported("comparison filter", "M2-K15"));
    }
    Ok(())
}
pub(crate) fn filter_state(state: &FilterState) -> FilterStateDto {
    match state {
        FilterState::Ready => FilterStateDto::Ready,
        FilterState::NeedsConfig(key) => FilterStateDto::NeedsConfig { key: key.clone() },
        FilterState::NeedsRendering => FilterStateDto::NeedsRendering,
        FilterState::NeedsCrawlAnalysis => FilterStateDto::NeedsCrawlAnalysis,
        FilterState::Stale => FilterStateDto::Stale,
        FilterState::NeedsRecrawl => FilterStateDto::NeedsRecrawl,
        FilterState::Incomplete { unchecked } => FilterStateDto::Incomplete {
            unchecked: *unchecked,
        },
    }
}
pub(crate) fn issue_type(t: os_model::IssueType) -> IssueTypeDto {
    match t {
        os_model::IssueType::Issue => IssueTypeDto::Issue,
        os_model::IssueType::Warning => IssueTypeDto::Warning,
        os_model::IssueType::Opportunity => IssueTypeDto::Opportunity,
    }
}
pub(crate) fn priority(p: os_model::IssuePriority) -> IssuePriorityDto {
    match p {
        os_model::IssuePriority::High => IssuePriorityDto::High,
        os_model::IssuePriority::Medium => IssuePriorityDto::Medium,
        os_model::IssuePriority::Low => IssuePriorityDto::Low,
    }
}
pub(crate) fn info(tab: &str, filter: &defs::FilterDef, cfg: &CrawlConfig) -> FilterInfo {
    let issue = os_analyze::issues::entry_for(tab, filter.id).map(|e| IssueBadge {
        issue_type: issue_type(e.issue_type),
        priority: priority(e.priority),
    });
    FilterInfo {
        comparison: None,
        id: names::label(filter.id, cfg),
        label_template: filter.id.into(),
        params: if !filter.id.contains('{') {
            Default::default()
        } else {
            let json = serde_json::to_value(cfg).unwrap_or_default();
            filter
                .id
                .split('{')
                .skip(1)
                .filter_map(|part| part.split_once('}'))
                .filter_map(|(key, _)| {
                    names::config_value(&json, key).map(|v| (key.to_string(), v.clone()))
                })
                .collect()
        },
        count: None,
        pct: None,
        state: filter_state(&os_analyze::state::filter_state(
            filter,
            cfg,
            &serde_json::to_value(cfg).unwrap_or_default(),
            &os_analyze::state::AnalysisSnapshot::default(),
        )),
        issue,
        analysis_step: match filter.kind {
            defs::FilterKind::PostCrawl(step) => Some(step.as_str().into()),
            _ => None,
        },
    }
}
pub(crate) fn count_rows(
    tabs: &[defs::TabDef],
    cfg: &CrawlConfig,
    counts: &[views::FilterCount],
) -> Vec<FilterCountRow> {
    counts
        .iter()
        .filter_map(|count| {
            let tab = tabs.iter().find(|t| t.id == count.tab)?;
            let filter = tab.filters.iter().find(|f| f.id == count.filter)?;
            let mut filter = info(tab.id, filter, cfg);
            filter.count = count.count;
            filter.pct = count.pct;
            filter.state = filter_state(&count.state);
            Some(FilterCountRow {
                tab: count.tab.clone(),
                filter,
            })
        })
        .collect()
}
pub(crate) fn empty(code: EmptyCode, arg: Option<String>) -> EmptyState {
    let action = match code {
        EmptyCode::FeatureDisabled => arg.as_ref().map(|key| EmptyAction::OpenSetting {
            anchor: key.clone(),
        }),
        EmptyCode::NeedsJsRendering => Some(EmptyAction::OpenSetting {
            anchor: "rendering.mode".into(),
        }),
        EmptyCode::NeedsCrawlAnalysis => Some(EmptyAction::StartAnalysis),
        EmptyCode::NoMatch => Some(EmptyAction::ClearSearch),
        _ => None,
    };
    EmptyState { code, arg, action }
}
pub(crate) fn kind(k: defs::ColumnKind) -> ColumnKind {
    match k {
        defs::ColumnKind::Text => ColumnKind::Text,
        defs::ColumnKind::Url => ColumnKind::Url,
        defs::ColumnKind::Int => ColumnKind::Int,
        defs::ColumnKind::Float { decimals } => ColumnKind::Float { decimals },
        defs::ColumnKind::Bool => ColumnKind::Bool,
        defs::ColumnKind::Timestamp => ColumnKind::Timestamp,
        defs::ColumnKind::Hash => ColumnKind::Hash,
        defs::ColumnKind::Seconds3 => ColumnKind::Seconds3,
    }
}
/// Extraction gates also cover old M1 column definitions which do not carry Needs metadata.
pub(crate) fn extraction_key(name: &str) -> Option<&'static str> {
    if name.starts_with("Title ") {
        Some("extraction.page_titles")
    } else if name.starts_with("Meta Description ") {
        Some("extraction.meta_description")
    } else if name.starts_with("Meta Keywords ") {
        Some("extraction.meta_keywords")
    } else if name.starts_with("H1-") {
        Some("extraction.h1")
    } else if name.starts_with("H2-") {
        Some("extraction.h2")
    } else if name.starts_with("Meta Robots ") {
        Some("extraction.meta_robots")
    } else if name.starts_with("X-Robots-Tag ") {
        Some("extraction.x_robots_tag")
    } else if name.starts_with("Canonical Link Element ") {
        Some("spider.canonicals.store")
    } else if name.starts_with("Meta Refresh ") {
        Some("spider.meta_refresh.store")
    } else if name.starts_with("rel=\"next\" ")
        || name.starts_with("rel=\"prev\" ")
        || name.starts_with("HTTP rel=\"next\" ")
        || name.starts_with("HTTP rel=\"prev\" ")
    {
        Some("spider.pagination.store")
    } else {
        match name {
            "Hash" => Some("extraction.hash"),
            "Indexability" | "Indexability Status" => Some("extraction.indexability"),
            "Response Time" => Some("extraction.response_time"),
            "Last-Modified" => Some("extraction.last_modified"),
            "Word Count" => Some("extraction.word_count"),
            "Text Ratio" => Some("extraction.text_ratio"),
            "Size (bytes)" => Some("extraction.page_size"),
            _ => None,
        }
    }
}
pub(crate) fn config_on(cfg: &CrawlConfig, key: &str) -> bool {
    names::config_value(&serde_json::to_value(cfg).unwrap_or_default(), key)
        .is_some_and(|v| v == &serde_json::Value::Bool(true))
}
pub(crate) fn column(c: &defs::ColumnDef, name: &str, cfg: &CrawlConfig) -> ColumnMeta {
    let mut needs = extraction_key(name)
        .filter(|k| !config_on(cfg, k))
        .map(|k| empty(EmptyCode::FeatureDisabled, Some(k.into())));
    if needs.is_none() {
        needs = match c.needs {
            Some(defs::Needs::Config(k)) if !config_on(cfg, k) => {
                Some(empty(EmptyCode::FeatureDisabled, Some(k.into())))
            }
            Some(defs::Needs::AnyConfig(keys)) if !keys.iter().any(|k| config_on(cfg, k)) => {
                Some(empty(EmptyCode::FeatureDisabled, Some(keys.join(" | "))))
            }
            Some(defs::Needs::Rendering)
                if cfg.rendering.mode != os_model::RenderingMode::JavaScript =>
            {
                Some(empty(EmptyCode::NeedsJsRendering, None))
            }
            _ => None,
        };
    }
    let mut ops = Vec::new();
    for (flag, variants) in [
        (
            defs::SearchOps::CONTAINS,
            vec![RuleOp::Contains, RuleOp::NotContains],
        ),
        (
            defs::SearchOps::EQUALS,
            vec![RuleOp::Equals, RuleOp::NotEquals],
        ),
        (
            defs::SearchOps::AFFIX,
            vec![RuleOp::StartsWith, RuleOp::EndsWith],
        ),
        (
            defs::SearchOps::REGEX,
            vec![RuleOp::Regex, RuleOp::NotRegex],
        ),
        (
            defs::SearchOps::COMPARE,
            vec![RuleOp::GreaterThan, RuleOp::LessThan],
        ),
        (
            defs::SearchOps::EMPTY,
            vec![RuleOp::IsEmpty, RuleOp::IsNotEmpty],
        ),
    ] {
        if c.search_ops.has(flag) {
            ops.extend(variants);
        }
    }
    ColumnMeta {
        key: name.into(),
        kind: kind(c.kind),
        default_visible: c.default_visible
            && match c.needs {
                Some(defs::Needs::Config(key)) => config_on(cfg, key),
                Some(defs::Needs::AnyConfig(keys)) => keys.iter().any(|key| config_on(cfg, key)),
                _ => true,
            },
        default_width: c.default_width,
        align: match c.align {
            defs::Align::Left => Align::Left,
            defs::Align::Right => Align::Right,
            defs::Align::Center => Align::Center,
        },
        sortable: c.sortable,
        searchable: ops,
        extension: c.extension,
        needs,
    }
}
pub(crate) fn expanded_columns(
    tab: &defs::TabDef,
    names: &[String],
    cfg: &CrawlConfig,
) -> Vec<ColumnMeta> {
    names
        .iter()
        .filter_map(|name| {
            tab.columns
                .iter()
                .find(|c| {
                    c.key == name
                        || match &c.sql {
                            defs::ColumnSql::Multi { base, .. }
                            | defs::ColumnSql::MultiCrawlWide { base, .. } => name
                                .strip_prefix(&format!("{base} "))
                                .is_some_and(|n| n.parse::<u64>().is_ok()),
                            defs::ColumnSql::MultiPaired { base, .. } => {
                                name.starts_with(&format!("{base} "))
                            }
                            _ => false,
                        }
                })
                .map(|c| column(c, name, cfg))
        })
        .collect()
}
fn get_tabs<'a>(app: &'a App, ctx: CallCtx, _: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move { to_json(&super::compare::change_tabs(app, &ctx).await?) })
}
fn columns<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ColumnsArgs = parse_args(args)?;
        if let Some(result) = super::compare::change_columns(app, &ctx, &args).await {
            return result;
        }
        let current = app.sessions().current(&ctx.window);
        let cfg = current
            .as_ref()
            .map(|s| s.config.clone())
            .unwrap_or_default();
        if let Some(s) = current {
            app.read_pool()
                .run_cached(
                    s.key,
                    "catalog-columns",
                    crate::runtime::current_cancel(),
                    move |file, _| {
                        let _snapshot = file.conn().unchecked_transaction()?;
                        let registry = tabs::configured_for_file(&cfg, file);
                        let mut q = views::ViewQuery::new(&args.tab, "All");
                        q.columns = views::ColumnSelection::All;
                        Ok(views::column_names_in(&registry, file, &cfg, &q)
                            .map_err(view_error)
                            .and_then(|names| {
                                let t = names::find_tab(&registry, &args.tab)
                                    .expect("query resolved tab");
                                to_json(&expanded_columns(t, &names, &cfg))
                            }))
                    },
                )
                .await?
        } else {
            let registry = tabs::configured(&cfg);
            let t = names::find_tab(&registry, &args.tab)
                .ok_or_else(|| CmdError::invalid_input("err-unknown-tab", "tab"))?;
            to_json(
                &t.columns
                    .iter()
                    .map(|c| column(c, c.key, &cfg))
                    .collect::<Vec<_>>(),
            )
        }
    })
}
fn counts<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ScopeArgs = parse_args(args)?;
        if let Some(result) = super::compare::change_overview(app, &ctx, &args, false).await {
            return result;
        }
        let change_rows = super::compare::change_counts(app, &ctx, args.compare).await?;
        let compare_scope = if matches!(
            args.compare,
            Some(CompareClass::Changed | CompareClass::NotChanged)
        ) {
            None
        } else {
            args.compare
        };
        check_scope(&args.segment, &compare_scope)?;
        let s = session(app, &ctx)?;
        // UI-018 (M2-N27): the selection lists read the shared count of Overview and Issues instead
        // of counting all filters of all tabs a second time.
        let shared = super::sidebar::counts::shared_facts(app, &s, &ctx.cancel).await?;
        app.read_pool()
            .run_snapshot(s.key, move |file, _| {
                let registry = tabs::configured_for_file(&s.config, file);
                let mut filters = count_rows(&registry, &s.config, &shared.facts.counts);
                filters.retain(|r| r.tab != "Change Detection");
                filters.extend(change_rows);
                Ok(to_json(&FilterCounts {
                    version: shared.seq,
                    filters,
                }))
            })
            .await?
    })
}
pub fn register(r: &mut Registry) {
    for (name, args, ret, h) in [
        (
            "catalog.tabs",
            "void",
            "TabInfo[]",
            get_tabs as crate::registry::Handler,
        ),
        ("catalog.columns", "ColumnsArgs", "ColumnMeta[]", columns),
        ("catalog.filterCounts", "ScopeArgs", "FilterCounts", counts),
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
mod tests {
    use super::*;
    // spec: TECH-008 [M2]
    // spec: UI-006
    #[test]
    fn filter_state_dtos_preserve_prerequisite_and_partial_coverage_states() {
        let cases: Vec<(FilterState, serde_json::Value)> = serde_json::from_str(include_str!(
            "../../../../apps/desktop/src/ipc/__fixtures__/k06/fx-ui-basic/FilterStates.json"
        ))
        .unwrap();
        for (state, expected) in cases {
            assert_eq!(
                serde_json::to_value(filter_state(&state)).unwrap(),
                expected
            );
        }
    }
}

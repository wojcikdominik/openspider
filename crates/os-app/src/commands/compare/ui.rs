//! Additive U23 read adapters for the existing sidebar/view commands. No new IPC command names.
use os_analyze::compare::{
    change, ChangeFilter, CompareClass, FilterComparison, LinkComparison, LinkDirection, LinkRow,
    LinkSource, Side,
};
use os_analyze::views::{self, ColumnSelection, PageStart, ViewQuery};
use os_model::{UrlId, Value};
use serde::Deserialize;
use serde_json::{json, Value as Json};

use super::{compare_error, with_active};
use crate::{
    dto::*,
    runtime::{error::parse_args, App, CallCtx},
};

fn filter_json(f: &FilterComparison) -> Json {
    json!({"tab":f.tab,"filter":f.filter,"label":f.label,"current":f.current,"previous":f.previous,
        "change":f.change(),"classes":f.classes})
}

/// Explicit opt-in keeps ordinary single-crawl sidebar consumers unchanged.
pub(crate) async fn overview(app: &App, ctx: CallCtx) -> Result<Json, CmdError> {
    let preview = super::super::ai_preview::switch(app);
    with_active(app, &ctx.window, move |a| {
        let mut rows = a.comparison.overview(&ctx.cancel).map_err(compare_error)?;
        // Plan M4 E1 (E-2 b, M4-V01): ohne Schalter „KI (Vorschau)“ keine KI-Tabs.
        rows.retain(|r| super::super::ai_preview::shows_name(preview, &r.tab));
        let mut overview:Vec<_>=rows.iter().map(filter_json).collect();
        let config = a.change_config();
        for element in std::iter::once("All").chain(change::ELEMENTS.iter()
            .filter(|(e, _, _)| config.elements.contains(e)).map(|(_, label, _)| *label)) {
            let n = change::ids(&a.comparison, element, true).map_err(compare_error)?.len();
            overview.push(json!({"tab":"Change Detection","filter":element,"label":element,"current":n,"previous":n,"change":n,"classes":null}));
        }
        let issues = a.comparison.issues(&ctx.cancel).map_err(compare_error)?;
        let notes: Vec<_> = a.comparison.notes().iter().map(|n| match n {
            os_analyze::compare::CompareNote::DifferentModes { .. } => json!({"kind":"modes"}),
            os_analyze::compare::CompareNote::Migrated { .. } => json!({"kind":"migration"}),
            os_analyze::compare::CompareNote::Collision(c) => json!({"kind":"collision","address":c.current_url,"winner":c.winner}),
            os_analyze::compare::CompareNote::MappingWarning(w) => json!({"kind":"mapping","rule":w.rule+1}),
        }).collect();
        Ok(json!({"summary":[],"tabs":[],"comparison":{
            "overview":overview,
            "issues":issues.iter().map(|i| filter_json(&i.filter)).collect::<Vec<_>>(),"notes":notes}}))
    }).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Query {
    current: String,
    previous: String,
    #[serde(default = "default_side")]
    side: String,
    tab: String,
    filter: String,
    class: Option<String>,
    url_id: Option<i64>,
    source: Option<String>,
    change: Option<String>,
    #[serde(default)]
    search: String,
    sort: Option<SortKey>,
}
fn default_side() -> String {
    "Current".into()
}
fn side(s: &str) -> Result<Side, CmdError> {
    match s {
        "Current" => Ok(Side::Current),
        "Previous" => Ok(Side::Previous),
        _ => Err(CmdError::invalid_input("err-compare-input", "side")),
    }
}

/// Stateless pages identified by both crawl IDs and comparison version; bounded IPC payloads.
pub(crate) async fn rows(app: &App, ctx: CallCtx, raw: Json) -> Result<Json, CmdError> {
    let q: Query = parse_args(raw["comparison"].clone())?;
    let args: RowsArgs = parse_args(raw)?;
    if !(1..=500).contains(&args.limit) {
        return Err(CmdError::invalid_input("err-invalid-limit", "limit"));
    }
    with_active(app, &ctx.window, move |a| {
        let c = &mut a.comparison;
        if c.version() != args.version || c.current().meta.crawl_id.to_string() != q.current || c.previous().meta.crawl_id.to_string() != q.previous {
            return Err(CmdError::new(ErrorCode::Stale,"err-view-stale"));
        }
        ctx.cancel.checkpoint().map_err(|_| CmdError::new(ErrorCode::Cancelled,"err-cancelled"))?;
        if q.tab == "Change Detection" {
            let config = os_model::ChangeDetectionConfig {
                elements: a.elements.iter().filter_map(|e| e.parse().ok()).collect(),
                content_threshold_pct: a.content_threshold_pct,
            };
            let mut page = change_page(c, &config, &q, args.offset, args.limit)?;
            page["viewId"] = json!(args.view_id); page["version"] = json!(args.version); page["offset"] = json!(args.offset);
            return Ok(page);
        }
        let upper = side(&q.side)?;
        if let Some(id) = q.url_id {
            let direction = match q.tab.as_str() { "inlinks" => LinkDirection::Inlinks, "outlinks" => LinkDirection::Outlinks,
                _ => return Err(CmdError::invalid_input("err-compare-input","tab")) };
            if !c.exists(upper,id) { return Err(CmdError::new(ErrorCode::NotFound,"err-url-not-found")); }
            let links = c.links(upper,id,direction).map_err(compare_error)?;
            let source = match q.source.as_deref().unwrap_or("Auto") { "Auto" => LinkSource::Auto, "Current" => LinkSource::Current, "Previous" => LinkSource::Previous,
                _ => return Err(CmdError::invalid_input("err-compare-input","source")) };
            let change = ChangeFilter::parse(q.change.as_deref().unwrap_or("All")).ok_or_else(|| CmdError::invalid_input("err-compare-input","change"))?;
            let columns: Vec<_> = os_analyze::compare::links::LINK_COLUMNS.iter().chain(os_analyze::compare::links::CHANGE_COLUMNS.iter()).copied().collect();
            let mut rows = keyed_link_rows(&links, source, upper, change)?;
            if let Some(sort) = &q.sort {
                let index = columns.iter().position(|column| *column == sort.column).ok_or_else(|| CmdError::invalid_input("err-compare-input", "sort"))?;
                sort_link_rows(&mut rows, index, sort.dir);
            }
            let total = rows.len();
            let page: Vec<_> = rows.into_iter().skip(args.offset as usize).take(args.limit as usize).collect();
            let row_keys: Vec<_> = page.iter().map(|(key, _)| *key).collect();
            let values: Vec<_> = page.into_iter().map(|(_, values)| values).collect();
            return Ok(json!({"viewId":args.view_id,"version":args.version,"offset":args.offset,"columns":columns,"cells":values,
                "rowKeys":row_keys,"total":total,"side":q.side,"filtersEnabled":links.filters_enabled()}));
        }
        let class = q.class.as_deref().map(|s| CompareClass::parse(s).ok_or_else(|| CmdError::invalid_input("err-compare-input","class"))).transpose()?;
        let selected_side = class.map_or(upper, |cl| if cl == CompareClass::Removed { Side::Current } else { cl.side() });
        let ids = if let Some(cl) = class {
            let classes = c.classes(&q.tab,&q.filter).map_err(compare_error)?.ok_or_else(|| CmdError::invalid_input("err-compare-input","filter"))?;
            if cl == CompareClass::Removed { classes.removed_current(c.pairing()) } else { classes.ids(cl,c.pairing()) }
        } else {
            c.filter_set(selected_side,&q.tab,&q.filter).map_err(compare_error)?.and_then(|f| f.ids.clone()).ok_or_else(|| CmdError::invalid_input("err-compare-input","filter"))?
        };
        let s = c.side(selected_side);
        let mut registry = os_analyze::tabs::configured_for_file(&s.config,&s.file);
        for tab in &mut registry {
            if tab.id == q.tab { for f in &mut tab.filters { f.predicate = "1"; f.ignore_non_indexable = false; } }
        }
        // Selection bypasses the original filter: Removed rows no longer satisfy it in Current.
        let mut query = ViewQuery::new(&q.tab,&q.filter);
        query.columns = if args.columns.is_empty() { ColumnSelection::Default } else { ColumnSelection::Named(args.columns) };
        query.start = PageStart::Offset(args.offset); query.limit = args.limit;
        if let Some(sort) = &q.sort { query.sort = super::super::view::sorts(std::slice::from_ref(sort)); }
        if !q.search.is_empty() { query.search = Some(views::SearchSpec::Simple(q.search)); }
        let keys: Vec<_> = ids.iter().map(|id| UrlId(i64::from(id))).collect();
        let p = views::query_url_ids_in(&registry,&s.file,&s.config,&query,&keys,true).map_err(super::super::catalog::view_error)?;
        let mut columns = p.columns;
        columns.push("Comparison Status".into());
        if c.has_mapping() { columns.push("Previous Address".into()); }
        let classes = c.classes(&q.tab,&q.filter).map_err(compare_error)?;
        let mut cells = Vec::new();
        for (key,mut row) in p.row_keys.iter().zip(p.rows) {
            let id = key.0 as u32;
            let status = class.or_else(|| classes.as_ref().and_then(|k| CompareClass::ALL.into_iter().find(|cl| {
                cl.side() == selected_side && k.ids(*cl,c.pairing()).contains(id)
            })));
            row.push(Value::Text(status.map_or("Unchanged", |cl| cl.as_str()).into()));
            if c.has_mapping() {
                let previous = if selected_side == Side::Previous { Some(key.0) } else { c.partner(Side::Current,key.0) };
                let address: Option<String> = previous.and_then(|id| os_store::queries::url_by_id(c.previous().file.conn(), UrlId(id)).ok().flatten().map(|r|r.address));
                row.push(address.map_or(Value::Null,Value::Text));
            }
            cells.push(row);
        }
        Ok(json!({"viewId":args.view_id,"version":args.version,"offset":args.offset,"columns":columns,"cells":cells,
            "rowKeys":p.row_keys.iter().map(|id|id.0).collect::<Vec<_>>(),"partnerKeys":p.row_keys.iter().map(|id|c.partner(selected_side,id.0)).collect::<Vec<_>>(),"total":p.total,"side":selected_side.as_str(),"filtersEnabled":true}))
    }).await
}

/// C02 owns classification, columns and SQL sorting. Page only after the complete order is known.
fn change_page(
    c: &os_analyze::compare::Comparison,
    config: &os_model::ChangeDetectionConfig,
    request: &Query,
    offset: u64,
    limit: u32,
) -> Result<Json, CmdError> {
    if request
        .class
        .as_deref()
        .is_some_and(|v| !["changed", "notChanged"].contains(&v))
    {
        return Err(CmdError::invalid_input("err-compare-input", "class"));
    }
    let changed = request.class.as_deref() != Some("notChanged");
    let mut query = ViewQuery::new("Change Detection", &request.filter);
    query.start = PageStart::Offset(offset);
    query.limit = limit;
    if let Some(sort) = &request.sort {
        query.sort = super::super::view::sorts(std::slice::from_ref(sort));
    }
    if !request.search.is_empty() {
        query.search = Some(views::SearchSpec::Simple(request.search.clone()));
    }
    let tab = change::tab_for_query(config, changed, &mut query);
    let current = c.current();
    let page = views::query_view_in(&[tab], &current.file, &current.config, &query)
        .map_err(super::super::catalog::view_error)?;
    let counts = change::counts(c, config, false).map_err(compare_error)?;
    let mut union = os_analyze::compare::RoaringBitmap::new();
    for row in &counts {
        union |= change::ids(c, &row.element, true).map_err(compare_error)?;
    }
    let counts: std::collections::BTreeMap<_, _> =
        counts.into_iter().map(|r| (r.element, r.change)).collect();
    let content_available = change::content_available(c, Side::Current).map_err(compare_error)?
        && change::content_available(c, Side::Previous).map_err(compare_error)?;
    Ok(
        json!({"columns":page.columns,"cells":page.rows,"rowKeys":page.row_keys.iter().map(|k|k.0).collect::<Vec<_>>(),
        "partnerKeys":page.row_keys.iter().map(|k|c.partner(Side::Current,k.0)).collect::<Vec<_>>(),
        "total":page.total,"side":"Current","filtersEnabled":true,"changedCounts":counts,"changedTotal":union.len(),"contentAvailable":content_available}),
    )
}

/// Assign keys before filtering: a single index build replaces one linear search per row.
fn keyed_link_rows(
    links: &LinkComparison,
    source: LinkSource,
    upper: Side,
    change: ChangeFilter,
) -> Result<Vec<(i64, Vec<Value>)>, CmdError> {
    let keys: std::collections::HashMap<*const LinkRow, i64> = links
        .current
        .iter()
        .enumerate()
        .map(|(i, row)| (std::ptr::from_ref(row), i as i64 + 1))
        .chain(
            links
                .previous
                .iter()
                .enumerate()
                .map(|(i, row)| (std::ptr::from_ref(row), -(i as i64) - 1)),
        )
        .collect();
    links
        .view(source, upper, change)
        .into_iter()
        .map(|row| {
            let key = keys.get(&std::ptr::from_ref(row)).copied().ok_or_else(|| {
                crate::runtime::error::internal(
                    "compare.rows",
                    "link does not belong to comparison",
                )
            })?;
            Ok((key, row.export_values()))
        })
        .collect()
}

fn sort_link_rows(rows: &mut [(i64, Vec<Value>)], index: usize, dir: SortDir) {
    rows.sort_by(|a, b| {
        let cmp = match (&a.1[index], &b.1[index]) {
            (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
            (Value::Null, _) => std::cmp::Ordering::Less,
            (_, Value::Null) => std::cmp::Ordering::Greater,
            (Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Value::Float(x), Value::Float(y)) => x.total_cmp(y),
            (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
            (Value::Text(x), Value::Text(y)) => x.cmp(y),
            (x, y) => format!("{x:?}").cmp(&format!("{y:?}")),
        };
        (if dir == SortDir::Desc {
            cmp.reverse()
        } else {
            cmp
        })
        .then_with(|| a.0.cmp(&b.0))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    // spec: TECH-301
    #[tokio::test]
    async fn ui_command_and_cli_export_share_cells_counts_and_sorted_pages() {
        let home = tempfile::tempdir().unwrap();
        let paths = os_store::paths::app_paths_with_home(Some(home.path().as_os_str()));
        paths.create_all().unwrap();
        let old = super::super::change_fixture::fixture(
            &paths.crawls,
            1,
            1,
            os_model::CrawlConfig::default(),
        );
        let new = super::super::change_fixture::fixture(
            &paths.crawls,
            2,
            2,
            os_model::CrawlConfig::default(),
        );
        let app = App::new(crate::AppConfig::new(home.path())).unwrap();
        let opened = app
            .dispatch(
                CallCtx::for_window("main"),
                "compare.open",
                json!({"current":new.path(),"previous":old.path(),"mapping":[],"elements":[]}),
            )
            .await
            .unwrap();
        let output = home.path().join("exports");
        let args = os_cli::RunArgs {
            crawl_comparison: Some(vec![
                old.meta().unwrap().crawl_id.to_string(),
                new.meta().unwrap().crawl_id.to_string(),
            ]),
            export_tabs: vec!["Change Detection:All".into()],
            output_folder: Some(output.clone()),
            ..Default::default()
        };
        let rep = os_cli::reporter::Reporter::new(false);
        let plan = os_cli::plan::build(&args, paths, &rep).unwrap();
        let outcome = os_cli::exec::execute(
            plan,
            rep,
            os_cli::signals::Signals::manual(os_model::CancelToken::new()),
        )
        .await;
        assert_eq!(outcome.exit, os_cli::Exit::Ok);
        let request = json!({"viewId":"parity","version":opened["version"],"offset":0,"limit":100,"columns":[],
            "comparison":{"current":opened["current"]["crawlId"],"previous":opened["previous"]["crawlId"],
                "tab":"Change Detection","filter":"All","class":"changed"}});
        let page = app
            .dispatch(CallCtx::for_window("main"), "view.rows", request.clone())
            .await
            .unwrap();
        let (header, csv) =
            os_export::csv::read_table(&output.join("change_detection_all.csv")).unwrap();
        assert_eq!(page["columns"], json!(header));
        let csv: Vec<_> = csv.map(Result::unwrap).collect();
        assert_eq!(page["total"], csv.len());
        let cells: Vec<Vec<os_model::Value>> =
            serde_json::from_value(page["cells"].clone()).unwrap();
        assert_eq!(cells.len(), csv.len());
        for (row, exported) in cells.iter().zip(&csv) {
            assert_eq!(row.len(), exported.len());
            for (value, text) in row.iter().zip(exported) {
                match value {
                    os_model::Value::Null => assert!(text.is_empty()),
                    os_model::Value::Text(s) => assert_eq!(s, text),
                    os_model::Value::Int(n) => assert_eq!(n.to_string(), *text),
                    os_model::Value::Float(n) => {
                        assert!((n - text.parse::<f64>().unwrap()).abs() <= 0.005)
                    }
                    os_model::Value::Bool(b) => assert_eq!(b.to_string(), *text),
                }
            }
        }
        assert_eq!(page["changedCounts"]["Page Titles"], 1);
        let mut sorted = request;
        sorted["comparison"]["filter"] = json!("All");
        sorted["comparison"]["sort"] = json!({"column":"Word Count Change %","dir":"desc"});
        let full = app
            .dispatch(CallCtx::for_window("main"), "view.rows", sorted.clone())
            .await
            .unwrap();
        sorted["offset"] = json!(2);
        sorted["limit"] = json!(3);
        let slice = app
            .dispatch(CallCtx::for_window("main"), "view.rows", sorted)
            .await
            .unwrap();
        assert_eq!(
            slice["cells"],
            json!(&full["cells"].as_array().unwrap()[2..5])
        );
        assert_eq!(
            slice["rowKeys"],
            json!(&full["rowKeys"].as_array().unwrap()[2..5])
        );
    }

    // spec: TECH-301
    #[test]
    fn link_sort_precedes_paging_and_preserves_keys() {
        let mut rows: Vec<_> = (0..205)
            .map(|i| (i + 1, vec![Value::Int(204 - i)]))
            .collect();
        sort_link_rows(&mut rows, 0, SortDir::Asc);
        assert_eq!(rows[99], (106, vec![Value::Int(99)]));
        assert_eq!(rows[100], (105, vec![Value::Int(100)]));
        let page: Vec<_> = rows.iter().skip(100).take(100).cloned().collect();
        assert_eq!(page.len(), 100);
        assert_eq!(page.first(), Some(&(105, vec![Value::Int(100)])));
        assert_eq!(page.last(), Some(&(6, vec![Value::Int(199)])));
        sort_link_rows(&mut rows, 0, SortDir::Desc);
        let page: Vec<_> = rows.iter().skip(100).take(100).cloned().collect();
        assert_eq!(page.len(), 100);
        assert_eq!(page.first(), Some(&(101, vec![Value::Int(104)])));
        assert_eq!(page.last(), Some(&(200, vec![Value::Int(5)])));
        assert_eq!(rows[105], (106, vec![Value::Int(99)]));
    }

    // spec: TECH-301
    #[test]
    fn ten_thousand_links_keep_original_keys_across_sources_and_filters() {
        use os_analyze::compare::{LinkChange, Presence};
        let changes = [
            LinkChange::Added,
            LinkChange::Removed,
            LinkChange::AnchorChanged,
            LinkChange::Unchanged,
        ];
        let make_rows = |crawl| {
            (0..10_000)
                .map(|i| LinkRow {
                    values: vec![Value::Int(i)],
                    change: Some(changes[i as usize % changes.len()]),
                    crawl,
                    previous_anchor: None,
                })
                .collect()
        };
        let mut links = LinkComparison {
            presence: Presence::Both,
            current: make_rows(Side::Current),
            previous: make_rows(Side::Previous),
        };
        for presence in [
            Presence::Both,
            Presence::CurrentOnly,
            Presence::PreviousOnly,
        ] {
            links.presence = presence;
            for source in [LinkSource::Auto, LinkSource::Current, LinkSource::Previous] {
                for upper in [Side::Current, Side::Previous] {
                    for change in
                        std::iter::once(ChangeFilter::All).chain(changes.map(ChangeFilter::Only))
                    {
                        let expected: Vec<_> = links
                            .view(source, upper, change)
                            .iter()
                            .map(|row| {
                                let Value::Int(index) = row.values[0] else {
                                    panic!("fixture index")
                                };
                                let key = if row.crawl == Side::Current {
                                    index + 1
                                } else {
                                    -index - 1
                                };
                                (key, row.export_values())
                            })
                            .collect();
                        assert_eq!(
                            keyed_link_rows(&links, source, upper, change).unwrap(),
                            expected
                        );
                    }
                }
            }
        }
    }

    // spec: OUT-140
    // spec: TECH-301
    #[tokio::test]
    async fn url_query_sort_orders_rows_and_keys_before_paging() {
        let home = tempfile::tempdir().unwrap();
        let old = super::super::change_fixture::fixture(
            home.path(),
            1,
            1,
            os_model::CrawlConfig::default(),
        );
        let new = super::super::change_fixture::fixture(
            home.path(),
            2,
            2,
            os_model::CrawlConfig::default(),
        );
        let app = App::new(crate::AppConfig::new(home.path())).unwrap();
        let opened = app
            .dispatch(
                CallCtx::for_window("main"),
                "compare.open",
                json!({"current":new.path(),"previous":old.path(),"mapping":[],"elements":[]}),
            )
            .await
            .unwrap();
        let mut request = json!({
            "viewId":"comparison-sort-test","version":opened["version"],"offset":0,"limit":100,"columns":["Address"],
            "comparison":{"current":opened["current"]["crawlId"],"previous":opened["previous"]["crawlId"],
                "tab":"Internal","filter":"All","sort":{"column":"Address","dir":"asc"}}
        });
        for (dir, ids) in [
            ("asc", (1..=12).collect::<Vec<i64>>()),
            ("desc", (1..=12).rev().collect()),
        ] {
            request["comparison"]["sort"]["dir"] = json!(dir);
            request["offset"] = json!(0);
            request["limit"] = json!(100);
            let full = rows(&app, CallCtx::for_window("main"), request.clone())
                .await
                .unwrap();
            assert_eq!(full["total"], 12);
            assert_eq!(full["rowKeys"], json!(ids));
            let addresses: Vec<_> = ids
                .iter()
                .map(|id| {
                    json!([
                        format!("https://site.test/{}", char::from(b'a' + (*id as u8) - 1)),
                        "Unchanged"
                    ])
                })
                .collect();
            assert_eq!(full["cells"], json!(addresses));
            request["offset"] = json!(2);
            request["limit"] = json!(3);
            let page = rows(&app, CallCtx::for_window("main"), request.clone())
                .await
                .unwrap();
            assert_eq!(page["total"], 12);
            assert_eq!(page["rowKeys"], json!(&ids[2..5]));
            assert_eq!(page["cells"], json!(&addresses[2..5]));
        }
        request["comparison"]["sort"]["column"] = json!("unknown");
        assert_eq!(
            rows(&app, CallCtx::for_window("main"), request)
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidInput
        );
        app.dispatch(CallCtx::for_window("main"), "compare.close", json!(null))
            .await
            .unwrap();
    }
}

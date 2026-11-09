//! Deterministic, window-owned 5,000,000 × 65 fixture. Only active with `stub-views`
//! and `AppConfig::stub_views`; the production registry remains unchanged.
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use futures_util::future::BoxFuture;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use crate::dto::*;
use crate::{App, AppConfig, CallCtx, CmdResult, Handler, Registry};

pub const TOTAL: u64 = 5_000_000;
#[derive(Clone)]
struct View {
    owner: String,
    version: u64,
    columns: Vec<ColumnMeta>,
    reverse: bool,
}
#[derive(Default)]
struct Views {
    cleanup_registered: bool,
    next: u64,
    views: HashMap<String, View>,
}
#[derive(Default)]
struct State(Mutex<Views>);

/// Additional initialization hook; opt-in fixture handlers retain the exact command metadata.
pub fn registry(original: Registry, config: &AppConfig) -> Registry {
    if !config.stub_views {
        return original;
    }
    let mut registry = Registry::new();
    for meta in original.manifest() {
        let handler: Handler = match meta.name {
            "view.open" => open,
            "view.update" => update,
            "view.rows" => rows,
            "view.locate" => locate,
            "view.range" => range,
            "view.copy" => copy,
            "view.close" => close,
            _ => original.get(meta.name).expect("registered command").1,
        };
        registry.register(meta, handler);
    }
    registry
}

pub fn columns() -> Vec<ColumnMeta> {
    (0..65)
        .map(|i| {
            let (key, kind, width) = match i {
                0 => ("Row".into(), ColumnKind::Int, 90),
                1 => ("Address".into(), ColumnKind::Url, 320),
                2 => ("Status Code".into(), ColumnKind::Int, 110),
                3 => ("Size (bytes)".into(), ColumnKind::Int, 120),
                4 => ("Response Time".into(), ColumnKind::Seconds3, 130),
                5 => ("Title 1".into(), ColumnKind::Text, 200),
                6 => ("Meta Description 1".into(), ColumnKind::Text, 200),
                7 => ("Indexable".into(), ColumnKind::Bool, 110),
                8 => ("Last Modified".into(), ColumnKind::Timestamp, 180),
                9 => ("Percent".into(), ColumnKind::Float { decimals: 1 }, 100),
                _ => (
                    format!("Metric {i:02}"),
                    ColumnKind::Float { decimals: 1 },
                    110,
                ),
            };
            ColumnMeta {
                key,
                kind,
                default_visible: true,
                default_width: width,
                align: if matches!(
                    kind,
                    ColumnKind::Int | ColumnKind::Float { .. } | ColumnKind::Seconds3
                ) {
                    Align::Right
                } else {
                    Align::Left
                },
                sortable: i == 0,
                searchable: vec![],
                extension: false,
                needs: None,
            }
        })
        .collect()
}
fn selected(keys: &[String]) -> Result<Vec<ColumnMeta>, CmdError> {
    let available = columns();
    if keys.is_empty() {
        return Ok(available);
    }
    let mut seen = HashSet::new();
    keys.iter()
        .map(|key| {
            if !seen.insert(key) {
                return Err(CmdError::invalid_input("err-stub-column", "columns"));
            }
            available
                .iter()
                .find(|c| &c.key == key)
                .cloned()
                .ok_or_else(|| CmdError::invalid_input("err-stub-column", "columns"))
        })
        .collect()
}
fn parse<T: DeserializeOwned>(args: Value) -> Result<T, CmdError> {
    serde_json::from_value(args)
        .map_err(|e| CmdError::invalid_input("err-stub-args", "args").with_detail(e.to_string()))
}
fn encode(value: impl Serialize) -> CmdResult {
    serde_json::to_value(value)
        .map_err(|e| CmdError::new(ErrorCode::Internal, "err-stub-json").with_detail(e.to_string()))
}
fn reverse(spec: &ViewSpec) -> Result<bool, CmdError> {
    if let ViewSpec::Tab {
        tab,
        filter,
        sort,
        search: None,
        segment: None,
        compare: None,
    } = spec
    {
        if tab == "Internal"
            && filter == "All"
            && sort.len() <= 1
            && sort.iter().all(|s| s.column == "Row")
        {
            return Ok(sort.first().is_some_and(|s| s.dir == SortDir::Desc));
        }
    }
    Err(CmdError::new(ErrorCode::Unsupported, "err-stub-spec"))
}
fn handle(id: String, v: &View) -> ViewHandle {
    ViewHandle {
        initial_rows: None,
        view_id: id,
        version: v.version,
        total: Some(TOTAL),
        columns: v.columns.clone(),
        state: ViewState::Ready,
        op: None,
        empty_state: None,
    }
}
fn get(app: &App, ctx: &CallCtx, id: &str) -> Result<View, CmdError> {
    app.ext::<State>()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .views
        .get(id)
        .filter(|v| v.owner == ctx.window.0)
        .cloned()
        .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-stub-view"))
}
fn open(app: &App, ctx: CallCtx, args: Value) -> BoxFuture<'_, CmdResult> {
    Box::pin(async move {
        let args: ViewOpenArgs = parse(args)?;
        let v = View {
            owner: ctx.window.0,
            version: 1,
            columns: selected(&args.columns)?,
            reverse: reverse(&args.spec)?,
        };
        let state = app.ext::<State>();
        let mut state = state.0.lock().unwrap_or_else(|p| p.into_inner());
        if !state.cleanup_registered {
            app.on_window_closed(Box::new(|app, window| {
                app.ext::<State>()
                    .0
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .views
                    .retain(|_, view| view.owner != window.0);
            }));
            state.cleanup_registered = true;
        }
        if state.views.len() >= 64 {
            return Err(CmdError::new(ErrorCode::Busy, "err-stub-view-limit"));
        }
        state.next += 1;
        let id = format!("stub-{}", state.next);
        let result = handle(id.clone(), &v);
        state.views.insert(id, v);
        encode(result)
    })
}
fn update(app: &App, ctx: CallCtx, args: Value) -> BoxFuture<'_, CmdResult> {
    Box::pin(async move {
        let args: ViewUpdateArgs = parse(args)?;
        let reversed = reverse(&args.spec)?;
        let state = app.ext::<State>();
        let mut state = state.0.lock().unwrap_or_else(|p| p.into_inner());
        let v = state
            .views
            .get_mut(&args.view_id)
            .filter(|v| v.owner == ctx.window.0)
            .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "err-stub-view"))?;
        v.version += 1;
        v.reverse = reversed;
        encode(handle(args.view_id, v))
    })
}
fn key(v: &View, row: u64) -> u64 {
    if v.reverse {
        TOTAL - row
    } else {
        row + 1
    }
}
fn position(v: &View, id: u64) -> u64 {
    if v.reverse {
        TOTAL - id
    } else {
        id - 1
    }
}
pub fn cell(column: &str, row: u64, id: u64) -> Value {
    match column {
        "Row" => json!(row + 1),
        "Address" => json!(format!("https://fixture.invalid/page/{id:07}")),
        "Status Code" => json!([200, 301, 404, 500, 0][((id - 1) % 5) as usize]),
        "Size (bytes)" => json!(12345),
        "Response Time" => json!(0.123),
        "Title 1" => json!(format!("Page {id}\tfixture\ntext")),
        "Meta Description 1" => Value::Null,
        "Indexable" => json!(id % 2 == 1),
        "Last Modified" => json!("2026-01-02T12:00:00Z"),
        "Percent" => json!(12.5),
        _ => json!(id as f64 + 0.5),
    }
}
fn rows(app: &App, ctx: CallCtx, args: Value) -> BoxFuture<'_, CmdResult> {
    Box::pin(async move {
        let args: RowsArgs = parse(args)?;
        let v = get(app, &ctx, &args.view_id)?;
        if args.version != v.version {
            return Err(CmdError::new(ErrorCode::Stale, "err-stale"));
        }
        if args.limit > 500 {
            return Err(CmdError::invalid_input("err-stub-limit", "limit"));
        }
        if !args.columns.is_empty() {
            selected(&args.columns)?;
        }
        let end = args.offset.saturating_add(args.limit as u64).min(TOTAL);
        let mut keys = Vec::new();
        let mut cells = Vec::new();
        for row in args.offset..end {
            if ctx.cancel.is_cancelled() {
                return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
            }
            let id = key(&v, row);
            keys.push(id as i64);
            cells.push(args.columns.iter().map(|c| cell(c, row, id)).collect());
        }
        encode(RowWindow {
            view_id: args.view_id,
            version: v.version,
            offset: args.offset,
            row_keys: keys,
            columns: args.columns,
            cells,
        })
    })
}
fn locate(app: &App, ctx: CallCtx, args: Value) -> BoxFuture<'_, CmdResult> {
    Box::pin(async move {
        let args: LocateArgs = parse(args)?;
        let v = get(app, &ctx, &args.view_id)?;
        encode(
            (1..=TOTAL as i64)
                .contains(&args.url_id)
                .then(|| position(&v, args.url_id as u64)),
        )
    })
}
fn range(app: &App, ctx: CallCtx, args: Value) -> BoxFuture<'_, CmdResult> {
    Box::pin(async move {
        let args: RangeArgs = parse(args)?;
        let v = get(app, &ctx, &args.view_id)?;
        let row = |id: i64| {
            (1..=TOTAL as i64)
                .contains(&id)
                .then(|| position(&v, id as u64))
        };
        let (from_row, to_row) = (row(args.from), row(args.to));
        let all = from_row
            .zip(to_row)
            .is_some_and(|(a, b)| crate::commands::view::range_covers_large(a, b, TOTAL));
        let keys = match (from_row, to_row) {
            (Some(a), Some(b)) if !crate::commands::view::range_requires_export(a, b) => {
                (a.min(b)..=a.max(b)).map(|r| key(&v, r) as i64).collect()
            }
            _ => Vec::new(),
        };
        encode(KeyRange {
            view_id: args.view_id,
            version: v.version,
            from_row,
            to_row,
            total: TOTAL,
            keys,
            all,
            export_required: !all
                && from_row
                    .zip(to_row)
                    .is_some_and(|(a, b)| crate::commands::view::range_requires_export(a, b)),
        })
    })
}
fn close(app: &App, ctx: CallCtx, args: Value) -> BoxFuture<'_, CmdResult> {
    Box::pin(async move {
        let args: ViewCloseArgs = parse(args)?;
        get(app, &ctx, &args.view_id)?;
        app.ext::<State>()
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .views
            .remove(&args.view_id);
        Ok(Value::Null)
    })
}
fn copy(app: &App, ctx: CallCtx, args: Value) -> BoxFuture<'_, CmdResult> {
    Box::pin(async move {
        let args: CopyArgs = parse(args)?;
        let v = get(app, &ctx, &args.view_id)?;
        if args.columns.is_empty() {
            return Err(CmdError::invalid_input("err-stub-column", "columns"));
        }
        selected(&args.columns)?;
        let (mut ids, except) = match args.selection {
            Selection::Ids { ids } => (
                ids.into_iter()
                    .filter(|id| (1..=TOTAL as i64).contains(id))
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>(),
                None,
            ),
            Selection::All {
                view_id,
                version,
                except,
            } => {
                if view_id != args.view_id || version != v.version {
                    return Err(CmdError::new(ErrorCode::Stale, "err-stale"));
                }
                (
                    Vec::new(),
                    Some(
                        except
                            .into_iter()
                            .filter(|id| (1..=TOTAL as i64).contains(id))
                            .collect::<HashSet<_>>(),
                    ),
                )
            }
        };
        let count = except
            .as_ref()
            .map_or(ids.len() as u64, |e| TOTAL - e.len() as u64);
        if count * args.columns.len() as u64 > 1_000_000 {
            return Err(CmdError::invalid_input(
                "err-copy-export-required",
                "selection",
            ));
        }
        if let Some(except) = except {
            ids = (1..=TOTAL as i64)
                .filter(|id| !except.contains(id))
                .collect();
        }
        ids.sort_unstable_by_key(|id| position(&v, *id as u64));
        let newline = if cfg!(windows) { "\r\n" } else { "\n" };
        let mut text = String::new();
        if args.with_header {
            text.push_str(&args.columns.join("\t"));
            text.push_str(newline);
        }
        for id in &ids {
            if ctx.cancel.is_cancelled() {
                return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
            }
            let row = position(&v, *id as u64);
            let values: Vec<_> = args
                .columns
                .iter()
                .map(|c| {
                    match cell(c, row, *id as u64) {
                        Value::Null => String::new(),
                        Value::String(s) => s,
                        other => other.to_string(),
                    }
                    .replace(['\t', '\r', '\n'], " ")
                })
                .collect();
            text.push_str(&values.join("\t"));
            text.push_str(newline);
        }
        app.desktop_actions()
            .ok_or_else(|| CmdError::new(ErrorCode::Unsupported, "err-clipboard-unavailable"))?
            .write_clipboard(&text)?;
        encode(CopyResult {
            rows: ids.len() as u64,
            bytes: text.len() as u64,
        })
    })
}

/// Native measurement runs opt in explicitly; ordinary starts keep their behavior.
pub fn configure(mut config: AppConfig) -> AppConfig {
    if std::env::var("OPENSPIDER_STUB_VIEWS").as_deref() == Ok("1") {
        config.stub_views = true;
    }
    config
}

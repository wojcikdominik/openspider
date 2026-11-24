//! Export operations: shared core writers, snapshot reads, cancellation, atomic file publication.
use super::{catalog, view};
use crate::{
    dto::*,
    ops::ProgressSink,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{
        error::{parse_args, store_error, to_json},
        App, CallCtx,
    },
};
use futures_util::future::BoxFuture;
use os_analyze::{defs::ColumnKind, views};
use os_export::{
    catalog::{self as core, ExportKind, ExportRef, TableJob},
    types::{ExportCtx, ExportFileInfo},
    ExportError, Format, TableWriter,
};
use os_model::{CancelToken, CrawlConfig, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
/// OUT-021 canonical detail paths, also used as the workbook's sheet title.
fn detail_name(spec: &ViewSpec) -> &'static str {
    match spec {
        ViewSpec::Tab { .. } => "URL Details",
        ViewSpec::Inlinks { .. } => "Inlinks",
        ViewSpec::Outlinks { .. } => "Outlinks",
        ViewSpec::Images { .. } => "Image Details",
        ViewSpec::Resources { .. } => "Resources",
        ViewSpec::Detail { detail, .. } => match detail {
            DetailTable::Headers => "HTTP Headers",
            DetailTable::Cookies => "Cookies",
            DetailTable::ConsoleLog => "Chrome Console Log",
            DetailTable::BlockedResources => "Blocked Resources",
            DetailTable::Network => "Network",
            DetailTable::Spelling => "Spelling & Grammar Details",
            DetailTable::A11yRules | DetailTable::A11yNodes => "Accessibility Details",
            DetailTable::Duplicates => "Duplicate Details",
            DetailTable::NGrams => "N-grams",
            DetailTable::StructuredDataItems | DetailTable::StructuredDataIssues => {
                "Structured Data Details"
            }
            DetailTable::RedirectChain => "Redirect Chain",
            DetailTable::SitemapMembership => "Sitemap Membership",
            DetailTable::CustomSearch => "Custom Search",
            DetailTable::CustomExtraction => "Custom Extraction",
            DetailTable::CustomJs => "Custom JavaScript",
            DetailTable::ApiValues => "API Values",
            DetailTable::Forms => "Forms",
            DetailTable::Hreflang => "Hreflang",
            DetailTable::CrawlPath => "Crawl Path",
            DetailTable::Anchors => "Anchors",
        },
    }
}
fn export_error(e: ExportError) -> CmdError {
    match e {
        ExportError::Store(e) => store_error(&e),
        ExportError::View(e) => catalog::view_error(e),
        ExportError::Name(e) => catalog::view_error(os_analyze::ViewError::Name(e)),
        ExportError::Io { path, source } => io_error(&path, source),
        ExportError::Unsupported(s) => {
            CmdError::new(ErrorCode::Unsupported, "err-export-unsupported").with_arg("format", s)
        }
        ExportError::NotImplemented(s) => CmdError::unsupported("export", s),
        e => CmdError::new(ErrorCode::Io, "err-export-write").with_detail(e.to_string()),
    }
}
fn io_error(path: &Path, e: std::io::Error) -> CmdError {
    CmdError::new(
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            ErrorCode::Permission
        } else {
            ErrorCode::Io
        },
        "err-export-io",
    )
    .with_arg("path", path.display().to_string())
    .with_detail(e.to_string())
}
fn format(f: ExportFormatDto) -> Result<Format, CmdError> {
    match f {
        ExportFormatDto::Csv => Ok(Format::Csv),
        ExportFormatDto::Xlsx => Ok(Format::Xlsx),
        ExportFormatDto::Json => Err(CmdError::new(
            ErrorCode::Unsupported,
            "err-export-unsupported",
        )
        .with_arg("format", "json")),
    }
}
fn checked(token: &CancelToken) -> Result<(), CmdError> {
    if token.is_cancelled() {
        Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"))
    } else {
        Ok(())
    }
}
/// The wrapper covers CPU/file writes in addition to the pool's SQLite interrupt.
struct GuardedWriter {
    inner: Box<dyn TableWriter>,
    cancel: CancelToken,
    progress: ProgressSink,
    rows: u64,
    cfg: CrawlConfig,
    masked: Vec<usize>,
}
impl TableWriter for GuardedWriter {
    fn header(&mut self, n: &[String]) -> Result<(), ExportError> {
        self.check()?;
        self.masked = n
            .iter()
            .enumerate()
            .filter_map(|(i, name)| {
                catalog::extraction_key(name)
                    .filter(|key| !catalog::config_on(&self.cfg, key))
                    .map(|_| i)
            })
            .collect();
        self.inner.header(n)
    }
    fn row(&mut self, k: &[ColumnKind], v: &[Value]) -> Result<(), ExportError> {
        self.check()?;
        if self.masked.is_empty() {
            self.inner.row(k, v)?;
        } else {
            let mut row = v.to_vec();
            for i in &self.masked {
                if let Some(value) = row.get_mut(*i) {
                    *value = Value::Null;
                }
            }
            self.inner.row(k, &row)?;
        }
        self.rows += 1;
        self.progress.report(self.rows, None);
        Ok(())
    }
    fn finish(self: Box<Self>) -> Result<ExportFileInfo, ExportError> {
        self.check()?;
        self.inner.finish()
    }
}
impl GuardedWriter {
    fn check(&self) -> Result<(), ExportError> {
        self.cancel
            .checkpoint()
            .map_err(|_| ExportError::Write("Export cancelled".into()))
    }
}
fn writer(
    path: &Path,
    fmt: Format,
    cfg: &CrawlConfig,
    cancel: &CancelToken,
    progress: &ProgressSink,
    entry: &str,
) -> Result<Box<dyn TableWriter>, ExportError> {
    // OUT-007: the worksheet is named like the CLI does it, "<Tab> <Filter>" resp. the bulk/report name.
    detail_writer(path, fmt, cfg, cancel, progress, &sheet_of(entry))
}
/// Worksheet name of a canonical `Tab:Filter`, bulk or report name (OUT-007; same rule as the CLI).
fn sheet_of(entry: &str) -> String {
    entry.replace(':', " ")
}
fn detail_writer(
    path: &Path,
    fmt: Format,
    cfg: &CrawlConfig,
    cancel: &CancelToken,
    progress: &ProgressSink,
    name: &str,
) -> Result<Box<dyn TableWriter>, ExportError> {
    Ok(Box::new(GuardedWriter {
        inner: os_export::open_writer(path, fmt, name, &os_export::pipeline::writer_options(cfg))?,
        cancel: cancel.clone(),
        progress: progress.clone(),
        rows: 0,
        cfg: cfg.clone(),
        masked: Vec::new(),
    }))
}
/// A private sibling directory keeps failed or cancelled exports away from existing user files.
struct Stage(PathBuf);
impl Stage {
    fn new(parent: &Path) -> Result<Self, CmdError> {
        std::fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
        let path = parent.join(format!(".openspider-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).map_err(|e| io_error(&path, e))?;
        Ok(Self(path))
    }
    fn publish(
        &self,
        path: &Path,
        target: &Path,
        token: &CancelToken,
    ) -> Result<FileResult, CmdError> {
        checked(token)?;
        let bytes = std::fs::metadata(path)
            .map_err(|e| io_error(path, e))?
            .len();
        std::fs::rename(path, target).map_err(|e| io_error(target, e))?;
        Ok(FileResult {
            path: target.display().to_string(),
            bytes,
        })
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
fn requirement<'a>(
    kind: ExportKind,
    name: &str,
    entries: &'a [&'a core::TableEntry],
) -> &'a [core::reports::ReportRequirement] {
    if let Some(e) = core::find_table(entries, name) {
        e.requires
    } else if kind == ExportKind::Report {
        core::reports::resolve(name).map_or(&[], |e| e.requires)
    } else {
        &[]
    }
}
fn catalog_command<'a>(
    app: &'a App,
    ctx: CallCtx,
    _: serde_json::Value,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let s = catalog::session(app, &ctx)?;
        // OUT-154: „Crawl Changes“ hängt an der Vergleichssitzung des Fensters, nicht am geöffneten Crawl.
        let crawl_changes = super::compare::crawl_changes_unavailable(app, &ctx.window);
        app.read_pool()
            .run_snapshot(s.key, move |file, _| {
                // All report prerequisites belong to this one SQLite snapshot. Which
                // filters await the crawl analysis needs no counting (M2-N25): a full
                // count per data version kept a core and a read connection busy for
                // the whole crawl and delayed crawl control and status delivery.
                let pending =
                    os_export::reports::prereq::pending_analysis_for_file(file, &s.config)
                        .unwrap_or_else(|_| vec!["analysis state unavailable".into()]);
                let groups = |kind| {
                    let entries = if kind == ExportKind::Bulk {
                        core::bulk_tables()
                    } else {
                        os_export::reports::report_tables()
                    };
                    let mut labels: BTreeMap<String, std::collections::VecDeque<String>> =
                        BTreeMap::new();
                    if kind == ExportKind::Bulk {
                        for entry in core::dynamic_entries(&s.config) {
                            labels
                                .entry(entry.name.clone())
                                .or_default()
                                .push_back(entry.label_key.unwrap_or(entry.name));
                        }
                    }
                    let mut out: BTreeMap<String, Vec<ExportItem>> = BTreeMap::new();
                    for name in core::list_with(kind, &s.config) {
                        let availability = if kind == ExportKind::Report
                            && super::compare::is_crawl_changes(&name)
                        {
                            os_export::reports::prereq::Availability {
                                enabled: crawl_changes.is_none(),
                                reason: crawl_changes.clone(),
                            }
                        } else {
                            os_export::reports::prereq::availability_with_pending(
                                &name,
                                requirement(kind, &name, &entries),
                                file,
                                &s.config,
                                &pending,
                            )
                        };
                        let group = name.split(':').next().unwrap_or("Exports").to_owned();
                        out.entry(group).or_default().push(ExportItem {
                            name: name.clone(),
                            label_key: labels
                                .get_mut(&name)
                                .and_then(|queue| queue.pop_front())
                                .unwrap_or(name),
                            enabled: availability.enabled,
                            disabled_reason: availability.reason,
                        });
                    }
                    out.into_iter()
                        .map(|(key, items)| ExportGroup {
                            label_key: key.clone(),
                            key,
                            items,
                        })
                        .collect()
                };
                Ok(to_json(&ExportCatalog {
                    bulk: groups(ExportKind::Bulk),
                    reports: groups(ExportKind::Report),
                    sitemaps: ["pages", "images"]
                        .into_iter()
                        .map(|name| ExportItem {
                            name: name.into(),
                            label_key: format!("export-sitemap-{name}"),
                            enabled: true,
                            disabled_reason: None,
                        })
                        .collect(),
                }))
            })
            .await?
    })
}
fn suggest<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: SuggestNameArgs = parse_args(args)?;
        if let ExportTarget::Report { name, format: f } = &args.target {
            if super::compare::is_crawl_changes(name) {
                let name =
                    super::compare::crawl_changes_name(app, &ctx.window, format(*f)?).await?;
                return to_json(&name);
            }
        }
        let s = catalog::session(app, &ctx)?;
        app.read_pool()
            .run_snapshot(s.key, move |file, _| {
                let started = file.meta()?.started_at;
                Ok((|| {
                    let (name, fmt) = match args.target {
                        ExportTarget::View {
                            tab,
                            filter,
                            format: f,
                        } => {
                            let tabs = os_analyze::tabs::configured_for_file(&s.config, file);
                            (
                                os_export::tab_export::canonical_name(
                                    &tabs, &s.config, &tab, &filter,
                                )
                                .map_err(export_error)?,
                                format(f)?,
                            )
                        }
                        ExportTarget::Bulk { name, format: f }
                        | ExportTarget::Report { name, format: f } => (name, format(f)?),
                        ExportTarget::Detail { spec, format: f } => {
                            (format!("Detail:{}", detail_name(&spec)), format(f)?)
                        }
                        ExportTarget::Sitemap { sitemap_kind } => {
                            return to_json(&format!(
                                "sitemap_{}.xml",
                                os_export::naming::file_name(&sitemap_kind, None, "")
                            ))
                        }
                    };
                    to_json(&os_export::tab_export::suggest_file_name(
                        &name, None, started, None, fmt,
                    ))
                })())
            })
            .await?
    })
}
fn export_view<'a>(
    app: &'a App,
    ctx: CallCtx,
    args: serde_json::Value,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if let Some(result) = super::compare::change_export(app, &ctx, &args).await {
            return result;
        }
        let args: ExportViewArgs = parse_args(args)?;
        let fmt = format(args.format)?;
        let v = view::get(app, &ctx, &args.view_id)?;
        if args.version != v.version {
            return Err(view::stale());
        }
        let columns = args.columns.clone().unwrap_or(v.columns.clone());
        let mut q = view::tab_query(&v.spec, &columns, 0, 1)?;
        if args.all_columns {
            q.columns = views::ColumnSelection::All;
        }
        let app = app.clone();
        let worker = app.clone();
        let op = app
            .ops()
            .spawn("export-view", true, move |token, progress| {
                Box::pin(async move {
                    let path = PathBuf::from(&args.path);
                    let worker_token = token.clone();
                    let result = worker
                        .read_pool()
                        .run_snapshot(v.session.key, move |file, _| {
                            Ok((|| {
                                if os_store::queries::change_seq(file.conn())
                                    .map_err(|e| store_error(&e))?
                                    != args.version
                                {
                                    return Err(view::stale());
                                }
                                let tabs =
                                    os_analyze::tabs::configured_for_file(&v.session.config, file);
                                let stage = Stage::new(parent(&path))?;
                                let staged = stage.0.join("view");
                                let sheet = os_export::tab_export::canonical_name(
                                    &tabs,
                                    &v.session.config,
                                    &q.tab,
                                    &q.filter,
                                )
                                .unwrap_or_else(|_| "Export".into());
                                let result = os_export::pipeline::write_view_with_upload_order(
                                    &tabs,
                                    file,
                                    &v.session.config,
                                    &q,
                                    args.preserve_upload_order,
                                    || {
                                        writer(
                                            &staged,
                                            fmt,
                                            &v.session.config,
                                            &worker_token,
                                            &progress,
                                            &sheet,
                                        )
                                    },
                                );
                                checked(&worker_token)?;
                                result.map_err(export_error)?;
                                to_json(&stage.publish(&staged, &path, &worker_token)?)
                            })())
                        })
                        .await?;
                    checked(&token)?;
                    result
                })
            });
        to_json(&op)
    })
}
async fn selection_ids(
    app: &App,
    ctx: &CallCtx,
    selection: &Selection,
) -> Result<Vec<os_model::UrlId>, CmdError> {
    match selection {
        Selection::Ids { ids } => {
            let mut seen = std::collections::HashSet::new();
            Ok(ids
                .iter()
                .copied()
                .filter(|id| seen.insert(*id))
                .map(os_model::UrlId)
                .collect())
        }
        Selection::All {
            view_id,
            version,
            except,
        } => {
            let v = view::get(app, ctx, view_id)?;
            if *version != v.version {
                return Err(view::stale());
            }
            let mut ids = Vec::new();
            let mut offset = 0;
            loop {
                let p = view::page(
                    app,
                    &v.session,
                    v.spec.clone(),
                    v.columns.clone(),
                    offset,
                    500,
                    Some(*version),
                )
                .await?;
                ids.extend(
                    p.keys
                        .iter()
                        .filter(|id| !except.contains(id))
                        .copied()
                        .map(os_model::UrlId),
                );
                offset += p.keys.len() as u64;
                if p.keys.is_empty() || offset >= p.total {
                    break;
                }
            }
            Ok(ids)
        }
    }
}
fn export_detail<'a>(
    app: &'a App,
    ctx: CallCtx,
    args: serde_json::Value,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ExportDetailArgs = parse_args(args)?;
        let fmt = format(args.format)?;
        let s = catalog::session(app, &ctx)?;
        // fix-sel: URL ids of another crawl's selection are refused before any file is written.
        super::crawl::ensure_selection_crawl(args.crawl_id.as_deref(), &s.key.0)?;
        let app = app.clone();
        let worker = app.clone();
        let op = app
            .ops()
            .spawn("export-detail", true, move |token, progress| {
                Box::pin(async move {
                    let ids = selection_ids(&worker, &ctx, &args.selection).await?;
                    let mut spec = args.spec;
                    match &mut spec {
                        ViewSpec::Inlinks { url_ids, .. }
                        | ViewSpec::Outlinks { url_ids, .. }
                        | ViewSpec::Resources { url_ids, .. }
                        | ViewSpec::Images { url_ids, .. }
                        | ViewSpec::Detail { url_ids, .. } => {
                            *url_ids = ids.iter().map(|id| id.0).collect()
                        }
                        ViewSpec::Tab { .. } => {}
                    };
                    let path = PathBuf::from(args.path);
                    let worker_token = token.clone();
                    let result = worker
                        .read_pool()
                        .run_snapshot(s.key, move |file, _| {
                            Ok((|| {
                                if let Some((kind, selected, params, sort)) = view::detail_spec(&spec) {
                                    if matches!(kind, os_analyze::details::DetailKind::A11yRules | os_analyze::details::DetailKind::A11yNodes) {
                                        if let Some(gate) = view::detail_gate(kind, &s.config) {
                                            return Err(CmdError::new(ErrorCode::Unsupported, "err-detail-unavailable").with_arg("state", serde_json::to_value(gate).unwrap_or_default()));
                                        }
                                        let stage = Stage::new(parent(&path))?;
                                        let staged = stage.0.join(path.file_name().unwrap_or_default());
                                        let tabs = os_analyze::tabs::configured_for_file(&s.config, file);
                                        let job = TableJob {file, cfg: &s.config, tabs: &tabs, entry_name: "Accessibility Details"};
                                        let report = |n| progress.report(n, None);
                                        let mut context = ExportCtx {cancel: &worker_token, progress: &report, skip_empty: false, workbook: None, skipped: vec![]};
                                        let files = os_export::catalog::bulk_a11y::export_details(&job, &os_analyze::details::DetailReq {url_ids: &selected, params: &params, sort: &sort, offset: 0, limit: 500}, &staged, fmt, &mut context).map_err(export_error)?;
                                        let mut published = Vec::new();
                                        for info in files {
                                            let target = parent(&path).join(info.path.file_name().unwrap_or_default());
                                            published.push(stage.publish(&info.path, &target, &worker_token)?);
                                        }
                                        let first = published.first().expect("detail files");
                                        return Ok(serde_json::json!({"path": first.path, "bytes": published.iter().map(|f|f.bytes).sum::<u64>(), "files": published}));
                                    }
                                }
                                let stage = Stage::new(parent(&path))?;
                                let staged = stage.0.join("detail");
                                let mut out = detail_writer(
                                    &staged,
                                    fmt,
                                    &s.config,
                                    &worker_token,
                                    &progress,
                                    detail_name(&spec),
                                )
                                .map_err(export_error)?;
                                let report = |n| progress.report(n, None);
                                let mut context = ExportCtx {
                                    cancel: &worker_token,
                                    progress: &report,
                                    skip_empty: false,
                                    workbook: None,
                                    skipped: vec![],
                                };
                                if matches!(spec, ViewSpec::Tab { .. }) {
                                    // URL Details uses the existing Tab descriptor as its property schema.
                                    let many = ids.len() > 1;
                                    let names = if many {
                                        vec!["Address".into(), "Name".into(), "Value".into()]
                                    } else {
                                        vec!["Name".into(), "Value".into()]
                                    };
                                    out.header(&names).map_err(export_error)?;
                                    for id in &ids {
                                        checked(&worker_token)?;
                                        let props = os_analyze::details::url_properties(
                                            file, &s.config, *id,
                                        )
                                        .map_err(catalog::view_error)?;
                                        let address = props
                                            .iter()
                                            .find(|(n, _)| n == "Address")
                                            .map(|(_, v)| v.clone())
                                            .unwrap_or(Value::Null);
                                        for (name, value) in props {
                                            let value = if catalog::extraction_key(&name)
                                                .is_some_and(|key| {
                                                    !catalog::config_on(&s.config, key)
                                                }) {
                                                Value::Null
                                            } else {
                                                value
                                            };
                                            let mut values = vec![Value::Text(name), value];
                                            let mut kinds =
                                                vec![ColumnKind::Text, ColumnKind::Text];
                                            if many {
                                                values.insert(0, address.clone());
                                                kinds.insert(0, ColumnKind::Url);
                                            }
                                            out.row(&kinds, &values).map_err(export_error)?;
                                        }
                                    }
                                } else if matches!(
                                    spec,
                                    ViewSpec::Detail {
                                        detail: DetailTable::CrawlPath,
                                        ..
                                    }
                                ) {
                                    // OUT-023: keep target-first report order, including the root fallback.
                                    let names = os_store::queries::crawl_path::HEADER
                                        .iter()
                                        .map(|name| (*name).to_owned())
                                        .collect::<Vec<_>>();
                                    let kinds = [
                                        ColumnKind::Url,
                                        ColumnKind::Int,
                                        ColumnKind::Url,
                                        ColumnKind::Int,
                                        ColumnKind::Text,
                                        ColumnKind::Text,
                                        ColumnKind::Int,
                                    ];
                                    out.header(&names).map_err(export_error)?;
                                    for id in &ids {
                                        checked(&worker_token)?;
                                        let data =
                                            os_store::queries::detail_ext::path_rows(file, id.0)
                                                .map_err(|e| store_error(&e))?;
                                        for row in data.rows {
                                            checked(&worker_token)?;
                                            out.row(&kinds, &row).map_err(export_error)?;
                                        }
                                    }
                                } else if let Some((kind, ids, params, sort)) =
                                    view::detail_spec(&spec)
                                {
                                    if let Some(gate) = view::detail_gate(kind, &s.config) {
                                        return Err(CmdError::new(
                                            ErrorCode::Unsupported,
                                            "err-detail-unavailable",
                                        )
                                        .with_arg(
                                            "state",
                                            serde_json::to_value(gate).unwrap_or_default(),
                                        ));
                                    }
                                    let tabs =
                                        os_analyze::tabs::configured_for_file(&s.config, file);
                                    let job = TableJob {
                                        file,
                                        cfg: &s.config,
                                        tabs: &tabs,
                                        entry_name: detail_name(&spec),
                                    };
                                    os_export::detail_export::export_detail_with_ctx(
                                        kind,
                                        &job,
                                        &os_analyze::details::DetailReq {
                                            url_ids: &ids,
                                            params: &params,
                                            sort: &sort,
                                            offset: 0,
                                            limit: 500,
                                        },
                                        out.as_mut(),
                                        &mut context,
                                    )
                                    .map_err(export_error)?;
                                } else {
                                    let mut offset = 0;
                                    let mut names = None;
                                    loop {
                                        checked(&worker_token)?;
                                        let (selection, page) =
                                            view::link_selection(&spec, &s.config, offset, 500)?;
                                        let p = os_store::queries::app_support::selected_links(
                                            file.conn(),
                                            &selection,
                                            &page,
                                        )
                                        .map_err(|e| store_error(&e))?;
                                        if names.is_none() {
                                            let header = os_export::links_export::link_header();
                                            out.header(&header).map_err(export_error)?;
                                            names = Some(header);
                                        }
                                        for row in &p.rows {
                                            out.row(
                                                &os_export::links_export::link_kinds(),
                                                &row[..15],
                                            )
                                            .map_err(export_error)?;
                                        }
                                        offset += p.rows.len() as u64;
                                        if p.rows.is_empty() || offset >= p.total {
                                            break;
                                        }
                                    }
                                }
                                checked(&worker_token)?;
                                out.finish().map_err(export_error)?;
                                to_json(&stage.publish(&staged, &path, &worker_token)?)
                            })())
                        })
                        .await?;
                    checked(&token)?;
                    result
                })
            });
        to_json(&op)
    })
}
fn named<'a>(
    app: &'a App,
    ctx: CallCtx,
    args: serde_json::Value,
    kind: ExportKind,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ExportBulkArgs = parse_args(args)?;
        let fmt = format(args.format)?;
        if kind == ExportKind::Report && super::compare::is_crawl_changes(&args.name) {
            return super::compare::crawl_changes_export(app, &ctx, PathBuf::from(&args.dir), fmt)
                .await;
        }
        let s = catalog::session(app, &ctx)?;
        let resolved = core::resolve_with(kind, &args.name, &s.config)
            .map_err(|e| export_error(ExportError::Name(e)))?;
        let name = match resolved {
            ExportRef::Bulk(n) | ExportRef::Report(n) => n,
            _ => unreachable!(),
        };
        let worker = app.clone();
        let op = app
            .ops()
            .spawn("export-catalog-item", true, move |token, progress| {
                Box::pin(async move {
                    let work_token = token.clone();
                    let result = worker
                        .read_pool()
                        .run_snapshot(s.key, move |file, _| {
                            Ok((|| {
                                let entries = if kind == ExportKind::Bulk {
                                    core::bulk_tables()
                                } else {
                                    os_export::reports::report_tables()
                                };
                                let availability = os_export::reports::prereq::availability(
                                    &name,
                                    requirement(kind, &name, &entries),
                                    file,
                                    &s.config,
                                );
                                if !availability.enabled {
                                    return Err(CmdError::new(
                                        ErrorCode::Unsupported,
                                        "err-export-unavailable",
                                    )
                                    .with_arg("reason", availability.reason.unwrap_or_default()));
                                }
                                let dir = PathBuf::from(args.dir);
                                // OUT-043 publishes one complete local snapshot in the exporter.
                                if kind == ExportKind::Bulk && name == core::bulk_screenshots::NAME
                                {
                                    let report = |rows| progress.report(rows, None);
                                    let context = ExportCtx {
                                        cancel: &work_token,
                                        progress: &report,
                                        skip_empty: false,
                                        workbook: None,
                                        skipped: vec![],
                                    };
                                    core::bulk_screenshots::export(
                                        file,
                                        &s.config,
                                        &dir,
                                        Some(&context),
                                    )
                                    .map_err(export_error)?;
                                    let target = dir.join("screenshots");
                                    let mut bytes = 0;
                                    for entry in std::fs::read_dir(&target)
                                        .map_err(|e| io_error(&target, e))?
                                    {
                                        let entry = entry.map_err(|e| io_error(&target, e))?;
                                        bytes += entry
                                            .metadata()
                                            .map_err(|e| io_error(&entry.path(), e))?
                                            .len();
                                    }
                                    return to_json(&FileResult {
                                        path: target.display().to_string(),
                                        bytes,
                                    });
                                }
                                let stage = Stage::new(&dir)?;
                                if kind == ExportKind::Bulk && name == core::bulk_issues::BULK_NAME
                                {
                                    let tabs =
                                        os_analyze::tabs::configured_for_file(&s.config, file);
                                    let result = core::bulk_issues::export_in(
                                        &tabs, file, &s.config, &stage.0, fmt,
                                    )
                                    .map_err(export_error)?;
                                    checked(&work_token)?;
                                    let mut bytes = 0;
                                    for info in result.files {
                                        let relative =
                                            info.path.strip_prefix(&stage.0).map_err(|e| {
                                                CmdError::new(
                                                    ErrorCode::Internal,
                                                    "err-export-path",
                                                )
                                                .with_detail(e.to_string())
                                            })?;
                                        let target = dir.join(relative);
                                        std::fs::create_dir_all(parent(&target))
                                            .map_err(|e| io_error(&target, e))?;
                                        bytes +=
                                            stage.publish(&info.path, &target, &work_token)?.bytes;
                                    }
                                    return to_json(&FileResult {
                                        path: dir.display().to_string(),
                                        bytes,
                                    });
                                }
                                let filename = if let Some(e) = core::find_table(&entries, &name) {
                                    e.file_name(fmt.extension())
                                } else if kind == ExportKind::Report {
                                    core::reports::resolve(&name)
                                        .map_err(|e| export_error(ExportError::Name(e)))?
                                        .file_name(fmt.extension())
                                } else {
                                    os_export::naming::file_name(&name, None, fmt.extension())
                                };
                                let target = dir.join(&filename);
                                let staged = stage.0.join(filename);
                                let mut out =
                                    writer(&staged, fmt, &s.config, &work_token, &progress, &name)
                                        .map_err(export_error)?;
                                let tabs = os_analyze::tabs::configured_for_file(&s.config, file);
                                let job = TableJob {
                                    file,
                                    cfg: &s.config,
                                    tabs: &tabs,
                                    entry_name: &name,
                                };
                                let report = |n| progress.report(n, None);
                                let mut context = ExportCtx {
                                    cancel: &work_token,
                                    progress: &report,
                                    skip_empty: false,
                                    workbook: None,
                                    skipped: vec![],
                                };
                                if kind == ExportKind::Report {
                                    os_export::reports::save_report_to(
                                        &job,
                                        out.as_mut(),
                                        &mut context,
                                    )
                                    .map_err(export_error)?;
                                } else if let Some(e) = core::find_table(&entries, &name) {
                                    let header = match e.header_fn {
                                        Some(f) => f(&job).map_err(export_error)?,
                                        None => e.header.iter().map(|n| (*n).into()).collect(),
                                    };
                                    out.header(&header).map_err(export_error)?;
                                    (e.run)(&job, out.as_mut()).map_err(export_error)?;
                                } else if let Some(e) = core::dynamic_entries(&s.config)
                                    .into_iter()
                                    .find(|e| e.name == name)
                                {
                                    out.header(&e.header).map_err(export_error)?;
                                    (e.run)(&job, out.as_mut()).map_err(export_error)?;
                                } else if name == core::bulk_issues::BULK_NAME {
                                    core::bulk_issues::export_to(&job, out.as_mut(), &mut context)
                                        .map_err(export_error)?;
                                } else {
                                    let e = core::bulk_links::resolve(&name)
                                        .map_err(|e| export_error(ExportError::Name(e)))?;
                                    os_export::links_export::export_link_entry_to(
                                        e,
                                        &job,
                                        out.as_mut(),
                                        &mut context,
                                    )
                                    .map_err(export_error)?;
                                }
                                checked(&work_token)?;
                                out.finish().map_err(export_error)?;
                                to_json(&stage.publish(&staged, &target, &work_token)?)
                            })())
                        })
                        .await?;
                    checked(&token)?;
                    result
                })
            });
        to_json(&op)
    })
}
fn bulk<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    named(app, ctx, args, ExportKind::Bulk)
}

fn report<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    named(app, ctx, args, ExportKind::Report)
}
fn sitemap<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ExportSitemapArgs = parse_args(args)?;
        let s = catalog::session(app, &ctx)?;
        let mut options: os_model::SitemapGenConfig = match args.options {
            Some(value) => parse_args(value)?,
            None => s.config.sitemap_gen.clone(),
        };
        let images = match args.sitemap_kind.as_str() {
            "pages" => false,
            "images" => {
                options.images.enabled = true;
                true
            }
            _ => return Err(CmdError::invalid_input("err-sitemap-kind", "sitemapKind")),
        };
        // OUT-124: the sitemap generator reads the complete configuration; the dialog options replace its sitemap part.
        let mut config = (*s.config).clone();
        config.sitemap_gen = options;
        let worker = app.clone();
        let clock = app.config().clock.clone();
        let op = app
            .ops()
            .spawn("export-sitemap", true, move |token, progress| {
                Box::pin(async move {
                    let work_token = token.clone();
                    let result = worker
                        .read_pool()
                        .run_snapshot(s.key, move |file, _| {
                            Ok((|| {
                                checked(&work_token)?;
                                let dir = PathBuf::from(args.dir);
                                let stage = Stage::new(&dir)?;
                                let sitemap_options = os_export::SitemapOptions::default();
                                let report = if images {
                                    os_export::sitemap_gen::generate_image_sitemap_with(
                                        file,
                                        &sitemap_options,
                                        &config,
                                        None,
                                        clock.as_ref(),
                                        &stage.0,
                                    )
                                } else {
                                    os_export::sitemap_gen::generate_sitemap_configured_with(
                                        file,
                                        &sitemap_options,
                                        &config,
                                        None,
                                        clock.as_ref(),
                                        &stage.0,
                                    )
                                }
                                .map_err(export_error)?;
                                checked(&work_token)?;
                                let mut bytes = 0;
                                for info in report.files {
                                    let target =
                                        dir.join(info.path.file_name().unwrap_or_default());
                                    bytes += stage.publish(&info.path, &target, &work_token)?.bytes;
                                }
                                progress.report(report.urls, Some(report.urls));
                                to_json(&FileResult {
                                    path: dir.display().to_string(),
                                    bytes,
                                })
                            })())
                        })
                        .await?;
                    checked(&token)?;
                    result
                })
            });
        to_json(&op)
    })
}
fn panel<'a>(app: &'a App, ctx: CallCtx, args: serde_json::Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ExportPanelArgs = parse_args(args)?;
        let fmt = format(args.format)?;
        let s = catalog::session(app, &ctx)?;
        let worker = app.clone();
        let op = app
            .ops()
            .spawn("export-panel", true, move |token, progress| {
                Box::pin(async move {
                    let work_token = token.clone();
                    let result = worker
                        .read_pool()
                        .run_snapshot(s.key, move |file, _| {
                            Ok((|| {
                                let path = PathBuf::from(args.path);
                                let stage = Stage::new(parent(&path))?;
                                let staged = stage.0.join("panel");
                                let mut out = writer(
                                    &staged,
                                    fmt,
                                    &s.config,
                                    &work_token,
                                    &progress,
                                    "Export",
                                )
                                .map_err(export_error)?;
                                let tabs = os_analyze::tabs::configured_for_file(&s.config, file);
                                let job = TableJob {
                                    file,
                                    cfg: &s.config,
                                    tabs: &tabs,
                                    entry_name: "Issues:All Issues",
                                };
                                let report = |n| progress.report(n, None);
                                let mut context = ExportCtx {
                                    cancel: &work_token,
                                    progress: &report,
                                    skip_empty: false,
                                    workbook: None,
                                    skipped: vec![],
                                };
                                match args.which {
                                    PanelKind::Issues => {
                                        core::bulk_issues::export_to(
                                            &job,
                                            out.as_mut(),
                                            &mut context,
                                        )
                                        .map_err(export_error)?;
                                    }
                                    PanelKind::Overview => {
                                        let counts =
                                            views::filter_counts_in(&tabs, file, &s.config)
                                                .map_err(catalog::view_error)?;
                                        out.header(&[
                                            "Name".into(),
                                            "URLs".into(),
                                            "% of Total".into(),
                                        ])
                                        .map_err(export_error)?;
                                        for row in super::sidebar::summary_rows(
                                            os_store::queries::app_support::overview_summary(
                                                file.conn(),
                                            )
                                            .map_err(|e| store_error(&e))?,
                                        ) {
                                            out.row(
                                                &[
                                                    ColumnKind::Text,
                                                    ColumnKind::Int,
                                                    ColumnKind::Float { decimals: 1 },
                                                ],
                                                &[
                                                    row.key.into(),
                                                    Value::Int(row.count as i64),
                                                    row.pct.into(),
                                                ],
                                            )
                                            .map_err(export_error)?;
                                        }
                                        for c in counts {
                                            out.row(
                                                &[
                                                    ColumnKind::Text,
                                                    ColumnKind::Int,
                                                    ColumnKind::Float { decimals: 1 },
                                                ],
                                                &[
                                                    Value::Text(format!("{}: {}", c.tab, c.label)),
                                                    c.count.map(|n| n as i64).into(),
                                                    c.pct.into(),
                                                ],
                                            )
                                            .map_err(export_error)?;
                                        }
                                    }
                                    PanelKind::SiteStructure | PanelKind::ResponseTimes => {
                                        let name = if args.which == PanelKind::SiteStructure {
                                            "Sidebar:Site Structure"
                                        } else {
                                            "Sidebar:Response Times"
                                        };
                                        let job = TableJob {
                                            entry_name: name,
                                            ..job
                                        };
                                        os_export::reports::save_report_to(
                                            &job,
                                            out.as_mut(),
                                            &mut context,
                                        )
                                        .map_err(export_error)?;
                                    }
                                    _ => {
                                        return Err(CmdError::unsupported(
                                            "sidebar panel",
                                            "M2-K03",
                                        ))
                                    }
                                }
                                checked(&work_token)?;
                                out.finish().map_err(export_error)?;
                                to_json(&stage.publish(&staged, &path, &work_token)?)
                            })())
                        })
                        .await?;
                    checked(&token)?;
                    result
                })
            });
        to_json(&op)
    })
}
pub fn register(r: &mut Registry) {
    for (name, args, ret, long_running, h) in [
        (
            "export.catalog",
            "void",
            "ExportCatalog",
            false,
            catalog_command as crate::registry::Handler,
        ),
        (
            "export.suggestName",
            "SuggestNameArgs",
            "string",
            false,
            suggest,
        ),
        (
            "export.view",
            "ExportViewArgs",
            "FileResult",
            true,
            export_view,
        ),
        (
            "export.detail",
            "ExportDetailArgs",
            "FileResult",
            true,
            export_detail,
        ),
        ("export.bulk", "ExportBulkArgs", "FileResult", true, bulk),
        (
            "export.report",
            "ExportBulkArgs",
            "FileResult",
            true,
            report,
        ),
        (
            "export.sitemap",
            "ExportSitemapArgs",
            "FileResult",
            true,
            sitemap,
        ),
        (
            "export.issues",
            "ExportPanelArgs",
            "FileResult",
            true,
            panel,
        ),
        (
            "export.sidebar",
            "ExportPanelArgs",
            "FileResult",
            true,
            panel,
        ),
    ] {
        r.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running,
            },
            h,
        );
    }
}

#[cfg(test)]
#[path = "../../../../tests/fixtures/tech-m2-javascript/app_screenshots.rs"]
mod screenshot_tests;

#[cfg(test)]
mod label_metadata_tests {
    use super::*;
    use crate::{pool::CrawlKey, runtime::sessions::SessionInfo, AppConfig, WindowId};
    use os_model::CrawlState;
    use std::sync::Arc;

    // spec: UI-121
    // spec: OUT-271
    #[tokio::test]
    async fn catalog_carries_label_origin_in_order_without_changing_canonical_names() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = CrawlConfig::default();
        cfg.custom_extraction.extractors = serde_json::from_str(
            r#"[{"name":"h2","expression":"//h2"},{"expression":"//comment()"}]"#,
        )
        .unwrap();
        cfg.custom_search = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/integ-custom-search/filters.json"
        ))
        .unwrap();
        for (filter, name) in cfg.custom_search.filters.iter_mut().zip([
            "Stock",
            "Stock Inlinks",
            "Screenshots",
            "Extractor 2",
        ]) {
            filter.name = name.into();
        }
        let mut named_extractor = cfg.custom_extraction.extractors[1].clone();
        named_extractor.name = "Extractor 2".into();
        cfg.custom_extraction.extractors.push(named_extractor);
        let fixture = os_engine::testing::crawl_fixture("integ-custom-search", cfg.clone()).await;
        let path = fixture.dir.path().join("crawl.ospider");
        let app = App::new(AppConfig::new(dir.path())).unwrap();
        let key = CrawlKey("labels".into());
        app.register_crawl(key.clone(), path.clone());
        app.sessions().set_current(
            WindowId("main".into()),
            SessionInfo {
                key,
                path,
                config: Arc::new(cfg.clone()),
                state: CrawlState::Completed,
                writable: true,
                engine: None,
            },
        );
        let catalog: ExportCatalog = serde_json::from_value(
            app.dispatch(
                CallCtx::for_window("main"),
                "export.catalog",
                serde_json::Value::Null,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        let items: Vec<_> = catalog.bulk.iter().flat_map(|g| &g.items).collect();
        let mut names: Vec<_> = items.iter().map(|item| item.name.clone()).collect();
        let mut canonical = core::list_with(ExportKind::Bulk, &cfg);
        names.sort();
        canonical.sort();
        assert_eq!(names, canonical);
        let metadata = |name: &str| -> Vec<serde_json::Value> {
            items
                .iter()
                .filter(|item| item.name == name)
                .map(|item| {
                    serde_json::from_str(item.label_key.strip_prefix("custom-export:").unwrap())
                        .unwrap()
                })
                .collect()
        };
        assert_eq!(
            metadata("Custom Search:Stock Inlinks"),
            vec![
                serde_json::json!({"group":"search", "name":"Stock", "inlinks":true, "generatedIndex":null}),
                serde_json::json!({"group":"search", "name":"Stock Inlinks", "inlinks":false, "generatedIndex":null}),
            ]
        );
        for name in ["Screenshots", "Extractor 2"] {
            assert_eq!(
                metadata(&format!("Custom Search:{name}")),
                vec![
                    serde_json::json!({"group":"search", "name":name, "inlinks":false, "generatedIndex":null}),
                ]
            );
        }
        assert_eq!(
            metadata("Custom Extraction:Extractor 2 Inlinks"),
            vec![
                serde_json::json!({"group":"extraction", "name":"Extractor 2", "inlinks":true, "generatedIndex":2}),
                serde_json::json!({"group":"extraction", "name":"Extractor 2", "inlinks":true, "generatedIndex":null}),
            ]
        );
        assert!(items
            .iter()
            .any(|item| item.name.starts_with("Images:") && item.label_key == item.name));
        assert!(catalog
            .reports
            .iter()
            .flat_map(|g| &g.items)
            .all(|item| item.label_key == item.name));
    }
}

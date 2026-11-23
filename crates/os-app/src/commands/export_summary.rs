//! Custom Crawl Overview: pool-backed measurement, cancellable Sheets operation.
use crate::{
    dto::{
        CmdError, CustomSummaryArgs, CustomSummaryMetric, ErrorCode, ExportSheetsArgs, FileResult,
        Notice, NoticeLevel,
    },
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{App, CallCtx},
};
use futures_util::future::BoxFuture;
use os_export::{
    catalog::TableJob,
    target_gsheet::{self, Destination},
    timeseries,
};
use os_integrations::{drive::DriveClient, ApiCtx};
use serde_json::Value;

#[derive(Default)]
pub struct SummaryState {
    serial: tokio::sync::Mutex<()>,
}
pub fn register(registry: &mut Registry) {
    for (name, args, ret, long_running, handler) in [
        (
            "export.customSummaryMetrics",
            "void",
            "CustomSummaryMetric[]",
            false,
            metrics as crate::registry::Handler,
        ),
        (
            "export.customSummary",
            "CustomSummaryArgs",
            "FileResult",
            true,
            summary,
        ),
        (
            "export.sheets",
            "ExportSheetsArgs",
            "FileResult",
            true,
            sheets,
        ),
    ] {
        registry.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running,
            },
            handler,
        );
    }
}
fn failure(e: impl std::fmt::Display) -> CmdError {
    CmdError::new(ErrorCode::Io, "err-io").with_detail(e.to_string())
}
fn metrics<'a>(app: &'a App, ctx: CallCtx, _args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let catalog = if let Some(session) = app.sessions().current(&ctx.window) {
            app.read_pool()
                .run(session.key, move |file, _token| {
                    Ok(timeseries::catalog(&os_analyze::tabs::configured_for_file(
                        &session.config,
                        file,
                    )))
                })
                .await?
        } else {
            timeseries::catalog(&os_analyze::tabs::configured(
                &os_model::CrawlConfig::default(),
            ))
        };
        // Plan M4 E1 (E-2 b, M4-F02): ohne Schalter „KI (Vorschau)“ keine Kennzahlen der KI-Tabs.
        let preview = super::ai_preview::switch(app);
        serde_json::to_value(
            catalog
                .into_iter()
                .filter(|m| super::ai_preview::shows_name(preview, &m.name))
                .map(|m| CustomSummaryMetric {
                    id: m.name.clone(),
                    label_key: m.name,
                })
                .collect::<Vec<_>>(),
        )
        .map_err(failure)
    })
}
fn summary<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: CustomSummaryArgs = serde_json::from_value(args)
            .map_err(|_| CmdError::invalid_input("err-invalid-args", "summary"))?;
        if args.account.trim().is_empty() || args.target.trim().is_empty() {
            return Err(CmdError::invalid_input("err-invalid-args", "target"));
        }
        let session = app
            .sessions()
            .current(&ctx.window)
            .ok_or_else(|| CmdError::new(ErrorCode::NoCrawl, "err-no-crawl"))?;
        let app = app.clone();
        let handle = app
            .clone()
            .ops()
            .spawn("op-export", true, move |cancel, progress| {
                Box::pin(async move { run_summary(&app, session, args, cancel, progress).await })
            });
        serde_json::to_value(handle).map_err(failure)
    })
}

async fn run_summary(
    app: &App,
    session: crate::runtime::sessions::SessionInfo,
    args: CustomSummaryArgs,
    cancel: os_model::CancelToken,
    progress: crate::ops::ProgressSink,
) -> CmdResult {
    let state = app.ext::<SummaryState>();
    let _guard = until_cancelled(state.serial.lock(), &cancel).await?;
    let cfg = (*session.config).clone();
    let context = ApiCtx {
        cfg: cfg.clone(),
        secrets: super::api::export_credentials(app),
        network: app.settings_snapshot().network,
        allow_api_override: false,
        quota_ledger: Some(app.config().paths.settings.join("api-quotas.json")),
    };
    let client =
        DriveClient::new(&context, &args.account, app.config().clock.now()).map_err(failure)?;
    let (measurement, project, crawl, offset) = app
        .read_pool()
        .run_with(session.key, cancel.clone(), move |file, _token| {
            Ok(os_export::pipeline::snapshot(file, || {
                let meta = file.meta()?;
                let tabs = os_analyze::tabs::configured_for_file(&cfg, file);
                let measurement = timeseries::measure(&TableJob {
                    entry_name: "Custom Crawl Overview",
                    tabs: &tabs,
                    file,
                    cfg: &cfg,
                })?;
                // SQLite resolves the system timezone for this instant, including DST.
                // Run on the read-pool worker; the process-start offset is unrelated.
                let seconds = os_store::queries::app_support::local_offset_seconds(
                    file.conn(),
                    measurement.started_at,
                )?;
                let offset = time::UtcOffset::from_whole_seconds(seconds)
                    .ok()
                    .ok_or_else(|| {
                        os_export::ExportError::Write(
                            "Lokale Crawlstartzeit nicht ermittelbar".into(),
                        )
                    })?;
                Ok((measurement, meta.project, meta.name, offset))
            })
            .map_err(failure))
        })
        .await??;
    let mut destination = Destination::new(
        if project.is_empty() {
            "Default"
        } else {
            &project
        },
        if crawl.is_empty() { "Crawl" } else { &crawl },
        &args.target,
    );
    if let Some(root) = context.cfg.api.drive.root_folder {
        destination.root = root;
    }
    let result = until_cancelled(
        target_gsheet::custom_summary(
            &client,
            &destination,
            &args.metrics,
            &measurement,
            offset,
            &cancel,
        ),
        &cancel,
    )
    .await?
    .map_err(failure)?;
    for warning in result.warnings {
        app.push_notice(Notice {
            level: NoticeLevel::Warning,
            code: "notice-export-warning".into(),
            args: [("message".into(), Value::String(warning))].into(),
        });
    }
    progress.report(1, Some(1));
    serde_json::to_value(FileResult {
        path: result.url,
        bytes: 0,
    })
    .map_err(failure)
}
/// Public command boundary for U27 (export dialog) and T01 (scheduled exports).
fn sheets<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: ExportSheetsArgs = serde_json::from_value(args)
            .map_err(|_| CmdError::invalid_input("err-invalid-args", "sheets"))?;
        if args.account.trim().is_empty()
            || args.name.trim().is_empty()
            || args.selections.is_empty()
        {
            return Err(CmdError::invalid_input("err-invalid-args", "sheets"));
        }
        let session = app
            .sessions()
            .current(&ctx.window)
            .ok_or_else(|| CmdError::new(ErrorCode::NoCrawl, "err-no-crawl"))?;
        let app = app.clone();
        let handle = app
            .clone()
            .ops()
            .spawn("op-export", true, move |cancel, progress| {
                Box::pin(async move {
                    let state = app.ext::<SummaryState>();
                    let _guard = until_cancelled(state.serial.lock(), &cancel).await?;
                    let context = ApiCtx {
                        cfg: (*session.config).clone(),
                        secrets: super::api::export_credentials(&app),
                        network: app.settings_snapshot().network,
                        allow_api_override: false,
                        quota_ledger: Some(app.config().paths.settings.join("api-quotas.json")),
                    };
                    let client =
                        DriveClient::new(&context, &args.account, app.config().clock.now())
                            .map_err(failure)?;
                    let selections: Vec<_> = args
                        .selections
                        .into_iter()
                        .map(|selection| os_export::types::ExportSelection {
                            kind: selection.kind,
                            name: selection.name,
                        })
                        .collect();
                    // Keep the temporary directory alive through the final upload. Staging,
                    // metadata and catalog reads share the same read-pool snapshot.
                    let (directory, staged, mut destination) = app
                        .read_pool()
                        .run_with(session.key, cancel.clone(), move |file, token| {
                            Ok(os_export::pipeline::snapshot(file, || {
                                let directory = tempfile::tempdir()
                                    .map_err(|e| os_export::ExportError::Write(e.to_string()))?;
                                let metadata = file.meta()?;
                                let mut control = os_export::types::ExportCtx {
                                    cancel: token,
                                    progress: &|_| {},
                                    skip_empty: false,
                                    workbook: None,
                                    skipped: Vec::new(),
                                };
                                let staged = target_gsheet::stage_batch(
                                    file,
                                    &session.config,
                                    &selections,
                                    directory.path(),
                                    &mut control,
                                )?;
                                let destination = Destination::new(
                                    if metadata.project.is_empty() {
                                        "Default"
                                    } else {
                                        &metadata.project
                                    },
                                    if metadata.name.is_empty() {
                                        "Crawl"
                                    } else {
                                        &metadata.name
                                    },
                                    &args.name,
                                );
                                Ok((directory, staged, destination))
                            })
                            .map_err(|error| {
                                if token.is_cancelled() {
                                    CmdError::new(ErrorCode::Cancelled, "err-cancelled")
                                } else {
                                    failure(error)
                                }
                            }))
                        })
                        .await??;
                    if let Some(root) = context.cfg.api.drive.root_folder {
                        destination.root = root;
                    }
                    for warning in staged.warnings {
                        export_warning(&app, warning);
                    }
                    let tables = staged
                        .tables
                        .iter()
                        .map(|(name, info)| target_gsheet::staged_table(name, info))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(failure)?;
                    let result = until_cancelled(
                        target_gsheet::export(&client, &destination, tables, &cancel),
                        &cancel,
                    )
                    .await?
                    .map_err(failure)?;
                    for warning in result.warnings {
                        export_warning(&app, warning);
                    }
                    drop(directory);
                    progress.report(1, Some(1));
                    serde_json::to_value(FileResult {
                        path: result.url,
                        bytes: 0,
                    })
                    .map_err(failure)
                })
            });
        serde_json::to_value(handle).map_err(failure)
    })
}
fn export_warning(app: &App, warning: String) {
    app.push_notice(Notice {
        level: NoticeLevel::Warning,
        code: "notice-export-warning".into(),
        args: [("message".into(), Value::String(warning))].into(),
    });
}
async fn until_cancelled<T>(
    future: impl std::future::Future<Output = T>,
    cancel: &os_model::CancelToken,
) -> Result<T, CmdError> {
    tokio::pin!(future);
    loop {
        if cancel.is_cancelled() {
            return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
        }
        tokio::select! {
            result=&mut future => return Ok(result),
            _=tokio::time::sleep(std::time::Duration::from_millis(50))=>{}
        }
    }
}

//! segment.* and sidebar.segments (M2-K14, CRAWL-134, UI-020/067/068).
use crate::{
    dto::*,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{
        error::{parse_args, to_json},
        sessions::SessionInfo,
    },
    App, CallCtx,
};
use futures_util::future::BoxFuture;
use os_analyze::segments::{self, SegmentError};
use os_model::{CrawlConfig, SegmentCondition, SegmentConfig, SegmentGroup, SegmentOp};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

fn pool_error(e: CmdError) -> os_store::StoreError {
    os_store::StoreError::InvalidInput(format!("{e}: {:?}", e.detail))
}
fn failure(e: SegmentError) -> CmdError {
    let code = match &e {
        SegmentError::Invalid(_) => ErrorCode::InvalidInput,
        SegmentError::Cancelled => ErrorCode::Cancelled,
        _ => ErrorCode::Store,
    };
    let mut error = CmdError::new(code, "err-segment").with_detail(e.to_string());
    if let SegmentError::Invalid(issues) = e {
        error = error.with_arg("issues", serde_json::to_value(issues).expect("validation"));
    }
    error
}
/// `segments.0.groups.1.conditions.2` -> `segments[0].groups[1].conditions[2]` (the form of `config.validate`).
fn bracket_path(path: &str) -> String {
    let mut out = String::new();
    for (i, part) in path.split('.').enumerate() {
        if part.chars().all(|c| c.is_ascii_digit()) && !part.is_empty() && i > 0 {
            out.push('[');
            out.push_str(part);
            out.push(']');
        } else {
            if i > 0 {
                out.push('.');
            }
            out.push_str(part);
        }
    }
    out
}
fn invalid(field: &str) -> CmdError {
    CmdError::invalid_input("err-invalid-segment", field)
}
fn condition(
    column: &Option<String>,
    op: RuleOp,
    value: &Option<String>,
) -> Result<SegmentCondition, CmdError> {
    let field = column.clone().ok_or_else(|| invalid("rules.column"))?;
    let value = value.clone().unwrap_or_default();
    let op = match (field.as_str(), op) {
        ("Segment", RuleOp::Equals) => SegmentOp::InSegment,
        ("Segment", RuleOp::IsEmpty) => SegmentOp::InNoSegment,
        ("Issues", RuleOp::Equals) => SegmentOp::HasIssue,
        ("Issues", RuleOp::NotEquals) => SegmentOp::HasNoIssue,
        (_, RuleOp::Contains) => SegmentOp::Contains,
        (_, RuleOp::NotContains) => SegmentOp::NotContains,
        (_, RuleOp::Equals) => SegmentOp::Equals,
        (_, RuleOp::NotEquals) => SegmentOp::NotEquals,
        (_, RuleOp::StartsWith) => SegmentOp::StartsWith,
        (_, RuleOp::EndsWith) => SegmentOp::EndsWith,
        (_, RuleOp::Regex) => SegmentOp::RegexMatches,
        (_, RuleOp::NotRegex) => SegmentOp::RegexNotMatches,
        (_, RuleOp::GreaterThan) => SegmentOp::GreaterThan,
        (_, RuleOp::LessThan) => SegmentOp::LessThan,
        (_, RuleOp::IsEmpty) => SegmentOp::IsEmpty,
        (_, RuleOp::IsNotEmpty) => SegmentOp::IsNotEmpty,
    };
    Ok(SegmentCondition { field, op, value })
}
fn groups(node: &RuleNode) -> Result<Vec<SegmentGroup>, CmdError> {
    match node {
        RuleNode::Cond { column, op, value } => Ok(vec![SegmentGroup {
            conditions: vec![condition(column, *op, value)?],
        }]),
        RuleNode::Group {
            op: RuleBool::Or,
            children,
        } => {
            let mut result = Vec::new();
            for child in children {
                result.extend(groups(child)?);
                if result.len() > 4096 {
                    return Err(invalid("rules.too-many-groups"));
                }
            }
            Ok(result)
        }
        RuleNode::Group {
            op: RuleBool::And,
            children,
        } => {
            let mut result = vec![SegmentGroup::default()];
            for child in children {
                let next = groups(child)?;
                if result.len().saturating_mul(next.len()) > 4096 {
                    return Err(invalid("rules.too-many-groups"));
                }
                result = result
                    .iter()
                    .flat_map(|a| {
                        next.iter().map(move |b| SegmentGroup {
                            conditions: a.conditions.iter().chain(&b.conditions).cloned().collect(),
                        })
                    })
                    .collect();
            }
            Ok(result)
        }
    }
}
fn config(base: &CrawlConfig, defs: &[SegmentDef]) -> Result<CrawlConfig, CmdError> {
    let mut cfg = base.clone();
    let mut ids = HashSet::new();
    cfg.segments.list = defs
        .iter()
        .map(|s| {
            if s.id.is_empty() || !ids.insert(&s.id) {
                return Err(invalid("segments.id"));
            }
            Ok(SegmentConfig {
                name: s.name.clone(),
                color: s.color.clone(),
                groups: groups(&s.rules)?,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(cfg)
}
fn wire_condition(c: &SegmentCondition) -> RuleNode {
    let (column, op) = match c.op {
        SegmentOp::InSegment => ("Segment".into(), RuleOp::Equals),
        SegmentOp::InNoSegment => ("Segment".into(), RuleOp::IsEmpty),
        SegmentOp::HasIssue => ("Issues".into(), RuleOp::Equals),
        SegmentOp::HasNoIssue => ("Issues".into(), RuleOp::NotEquals),
        op => (
            c.field.clone(),
            match op {
                SegmentOp::Contains => RuleOp::Contains,
                SegmentOp::NotContains => RuleOp::NotContains,
                SegmentOp::Equals => RuleOp::Equals,
                SegmentOp::NotEquals => RuleOp::NotEquals,
                SegmentOp::StartsWith => RuleOp::StartsWith,
                SegmentOp::EndsWith => RuleOp::EndsWith,
                SegmentOp::RegexMatches => RuleOp::Regex,
                SegmentOp::RegexNotMatches => RuleOp::NotRegex,
                SegmentOp::GreaterThan | SegmentOp::GreaterOrEqual => RuleOp::GreaterThan,
                SegmentOp::LessThan | SegmentOp::LessOrEqual => RuleOp::LessThan,
                SegmentOp::IsEmpty => RuleOp::IsEmpty,
                _ => RuleOp::IsNotEmpty,
            },
        ),
    };
    let cond = RuleNode::Cond {
        column: Some(column.clone()),
        op,
        value: Some(c.value.clone()),
    };
    if matches!(c.op, SegmentOp::GreaterOrEqual | SegmentOp::LessOrEqual) {
        RuleNode::Group {
            op: RuleBool::Or,
            children: vec![
                cond,
                RuleNode::Cond {
                    column: Some(column),
                    op: RuleOp::Equals,
                    value: Some(c.value.clone()),
                },
            ],
        }
    } else {
        cond
    }
}
fn definitions(cfg: CrawlConfig, raw: Option<String>) -> Result<Vec<SegmentDef>, CmdError> {
    if let Some(raw) = raw {
        let defs: Vec<SegmentDef> =
            serde_json::from_str(&raw).map_err(|_| invalid("segments.dto"))?;
        if config(&cfg, &defs)?.segments == cfg.segments {
            return Ok(defs);
        }
    }
    Ok(cfg
        .segments
        .list
        .iter()
        .enumerate()
        .map(|(n, s)| SegmentDef {
            id: format!("segment-{}", n + 1),
            name: s.name.clone(),
            color: s.color.clone(),
            order: n as u32,
            rules: RuleNode::Group {
                op: RuleBool::Or,
                children: s
                    .groups
                    .iter()
                    .map(|g| RuleNode::Group {
                        op: RuleBool::And,
                        children: g.conditions.iter().map(wire_condition).collect(),
                    })
                    .collect(),
            },
        })
        .collect())
}
fn list<'a>(app: &'a App, ctx: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let session = crate::session::current(app, &ctx.window)?;
        let defs = app
            .read_pool()
            .run(session.key, move |f, _| {
                let (cfg, raw) =
                    segments::definition_snapshot(f).map_err(|e| pool_error(failure(e)))?;
                definitions(cfg, raw).map_err(pool_error)
            })
            .await?;
        to_json(&defs)
    })
}
async fn save_defs(
    app: &App,
    session: SessionInfo,
    defs: Vec<SegmentDef>,
    cancel: os_model::CancelToken,
) -> Result<(), CmdError> {
    crate::session::writable(&session)?;
    let cfg = config(&session.config, &defs)?;
    let key = session.key.clone();
    let new = app
        .write_queue()
        .submit_file(&key, move |file| {
            let mut stored = file.config().map_err(|e| failure(e.into()))?;
            stored.segments = cfg.segments;
            segments::save(
                file,
                &stored,
                &serde_json::to_string(&defs).expect("definitions"),
                &cancel,
            )
            .map_err(failure)?;
            Ok(Arc::new(stored))
        })
        .await?;
    app.sessions().update_config(&key, new.clone());
    app.crawl_events().emit(
        app,
        crate::runtime::lifecycle_bus::CrawlEvent::ConfigChanged {
            key,
            old: session.config,
            new,
        },
    );
    Ok(())
}
fn save<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: SegmentSaveArgs = parse_args(args)?;
        let session = crate::session::current(app, &ctx.window)?;
        save_defs(app, session, args.segments, ctx.cancel).await?;
        to_json(&SegmentSaveResult {
            invalidated_steps: Vec::new(),
        })
    })
}
fn delete<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: IdArgs = parse_args(args)?;
        let session = crate::session::current(app, &ctx.window)?;
        let mut defs = app
            .read_pool()
            .run(session.key.clone(), move |f, _| {
                let (cfg, raw) =
                    segments::definition_snapshot(f).map_err(|e| pool_error(failure(e)))?;
                definitions(cfg, raw).map_err(pool_error)
            })
            .await?;
        let size = defs.len();
        defs.retain(|d| d.id != args.id);
        if size == defs.len() {
            return Err(CmdError::new(ErrorCode::NotFound, "err-segment-not-found"));
        }
        save_defs(app, session, defs, ctx.cancel).await?;
        Ok(Value::Null)
    })
}
fn validate<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: SegmentValidateArgs = parse_args(args)?;
        let session = crate::session::current(app, &ctx.window)?;
        let mut defs = app
            .read_pool()
            .run(session.key.clone(), move |f, _| {
                let (cfg, raw) =
                    segments::definition_snapshot(f).map_err(|e| pool_error(failure(e)))?;
                definitions(cfg, raw).map_err(pool_error)
            })
            .await?;
        if let Some(index) = defs.iter().position(|s| s.id == args.segment.id) {
            defs.remove(index);
        }
        let index = (args.segment.order as usize).min(defs.len());
        defs.insert(index, args.segment);
        let cfg = config(&session.config, &defs)?;
        let issues = app
            .read_pool()
            .run(session.key, move |file, _| {
                let mut issues = segments::validate_file(file, &cfg)
                    .into_iter()
                    .map(|v| ValidationIssue {
                        path: bracket_path(&v.path),
                        message_key: "err-invalid-segment".into(),
                        args: BTreeMap::from([("reason".into(), v.message.into())]),
                        line: Some(v.line),
                        column: Some(v.position),
                        warning: false,
                    })
                    .collect::<Vec<_>>();
                for path in
                    segments::pending_conditions(file, &cfg).map_err(|e| pool_error(failure(e)))?
                {
                    issues.push(ValidationIssue {
                        path: bracket_path(&path),
                        message_key: "segment-pending".into(),
                        args: BTreeMap::new(),
                        line: None,
                        column: None,
                        warning: true,
                    });
                }
                Ok(issues)
            })
            .await?;
        to_json(&issues)
    })
}
fn preview<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: SegmentPreviewArgs = parse_args(args)?;
        let session = crate::session::current(app, &ctx.window)?;
        let cfg = config(&session.config, &args.segments)?;
        let app2 = app.clone();
        let handle = app
            .ops()
            .spawn("segment-preview", true, move |cancel, progress| {
                Box::pin(async move {
                    let assignment = app2
                        .read_pool()
                        .run_with(session.key, cancel, move |f, c| {
                            segments::preview(f, &cfg, c).map_err(|e| pool_error(failure(e)))
                        })
                        .await?;
                    progress.report(
                        assignment.segments.iter().map(|s| s.len()).sum::<u64>()
                            + assignment.unassigned.len(),
                        None,
                    );
                    to_json(&SegmentPreview {
                        counts: args
                            .segments
                            .iter()
                            .zip(assignment.segments)
                            .map(|(s, b)| SegmentCount {
                                id: s.id.clone(),
                                urls: b.len(),
                            })
                            .collect(),
                        unassigned: assignment.unassigned.len(),
                    })
                })
            });
        to_json(&handle)
    })
}
fn reassign<'a>(app: &'a App, ctx: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let session = crate::session::current(app, &ctx.window)?;
        crate::session::writable(&session)?;
        let app2 = app.clone();
        let handle = app
            .ops()
            .spawn("segment-reassign", true, move |cancel, progress| {
                Box::pin(async move {
                    // Idempotent: the assignment is recomputed only when URL facts, analysis results or the
                    // definitions changed since the last one (segment.save already reassigns), so a repeated call
                    // neither rewrites url_segments nor moves the counts/sidebar versions.
                    let count = app2
                        .write_queue()
                        .submit_file(&session.key, move |f| {
                            let cfg = segments::saved_config(f).map_err(failure)?;
                            segments::reassign_if_stale(f, &cfg, &cancel).map_err(failure)
                        })
                        .await?;
                    progress.report(count, Some(count));
                    Ok(json!({"count":count}))
                })
            });
        to_json(&handle)
    })
}
fn sidebar<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: SegmentsViewArgs = parse_args(args)?;
        let session = crate::session::current(app, &ctx.window)?;
        let rows = app
            .read_pool()
            .run_with(session.key, ctx.cancel, move |file, cancel| {
                let cfg = segments::saved_config(file).map_err(|e| pool_error(failure(e)))?;
                let rows = segments::statistics(file, &cfg, cancel)
                    .map_err(|e| pool_error(failure(e)))?
                    .into_iter()
                    .map(|s| {
                        let by_class = match args.view {
                            SiteStructureView::CrawlDepth => s.depths,
                            SiteStructureView::ResponseCodes => s.response_codes,
                            SiteStructureView::Indexability => {
                                // The UI reads the plural spellings (UI-067); the core counts "Issue", "Warning", "Opportunity".
                                let mut counts = BTreeMap::new();
                                for (key, n) in s.issues {
                                    let key = match key.as_str() {
                                        "Issue" => "Issues".to_owned(),
                                        "Warning" => "Warnings".to_owned(),
                                        "Opportunity" => "Opportunities".to_owned(),
                                        _ => key,
                                    };
                                    counts.insert(key, n);
                                }
                                counts.insert("Indexable".into(), s.indexable);
                                counts.insert("Non-Indexable".into(), s.non_indexable);
                                counts
                            }
                        };
                        SegmentRow {
                            name: s.name,
                            color: s.color,
                            urls: s.urls,
                            by_class,
                        }
                    })
                    .collect::<Vec<_>>();
                Ok(rows)
            })
            .await?;
        to_json(&rows)
    })
}
pub fn register(r: &mut Registry) {
    for (name, args, ret, long_running, handler) in [
        (
            "sidebar.segments",
            "SegmentsViewArgs",
            "SegmentRow[]",
            false,
            sidebar as crate::registry::Handler,
        ),
        ("segment.list", "void", "SegmentDef[]", false, list),
        (
            "segment.save",
            "SegmentSaveArgs",
            "SegmentSaveResult",
            false,
            save,
        ),
        ("segment.delete", "IdArgs", "void", false, delete),
        (
            "segment.validate",
            "SegmentValidateArgs",
            "ValidationIssue[]",
            false,
            validate,
        ),
        (
            "segment.preview",
            "SegmentPreviewArgs",
            "SegmentPreview",
            true,
            preview,
        ),
        ("segment.reassign", "void", "CountResult", true, reassign),
    ] {
        r.register(
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

/// Re-evaluate after URL batches, analysis completion and API completion. The fingerprint
/// excludes our own reset_seq, so assignment changes never trigger a feedback loop.
pub fn on_startup(app: &App) {
    let weak = app.downgrade();
    app.handle().spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut previous: HashMap<crate::pool::CrawlKey, (String, u64)> = HashMap::new();
        // Der erste Tick kommt sofort: nicht gleich beim Start hochstufen (ein sofortiger Abbau durch den Besitzer
        // bleibt der letzte, OUT-229).
        interval.tick().await;
        loop {
            interval.tick().await;
            let Some(app) = weak.upgrade() else { break };
            let mut seen = HashSet::new();
            for session in app.sessions().all() {
                if !session.writable || !seen.insert(session.key.clone()) {
                    continue;
                }
                let key = session.key.clone();
                let last = previous.get(&key).cloned();
                let last_seq = last.as_ref().map_or(0, |(_, seq)| *seq);
                let result = app
                    .read_pool()
                    .run(key.clone(), move |file, _| {
                        segments::live_snapshot(file, last_seq).map_err(|e| pool_error(failure(e)))
                    })
                    .await;
                let Ok(Some(snapshot)) = result else { continue };
                let segments::LiveSnapshot {
                    fingerprint,
                    sequence: seq,
                    changed: ids,
                } = snapshot;
                if last.as_ref() == Some(&(fingerprint.clone(), seq)) {
                    continue;
                }
                let incremental = last
                    .as_ref()
                    .is_some_and(|(f, old_seq)| f == &fingerprint && seq >= *old_seq)
                    && !ids.is_empty()
                    && ids.len() <= 4096;
                let result = app
                    .write_queue()
                    .submit_file(&key, move |file| {
                        let cfg = file.config().map_err(|e| failure(e.into()))?;
                        // A command may have completed assignment since the observer read
                        // its snapshot. Check the durable completion under the write lock.
                        let current = segments::live_snapshot(file, 0).map_err(failure)?;
                        let fresh = if let Some(current) = current {
                            file.get_meta("segments.assignment_seq")
                                .map_err(|e| failure(e.into()))?
                                == Some(current.sequence.to_string())
                                && file
                                    .get_meta("segments.assignment_fingerprint")
                                    .map_err(|e| failure(e.into()))?
                                    == Some(current.fingerprint)
                        } else {
                            true
                        };
                        if fresh {
                            // Nothing to write; preserve the command's sequence and versions.
                        } else if incremental
                            && os_store::queries::change_seq(file.conn())
                                .map_err(|e| failure(e.into()))?
                                == seq
                        {
                            segments::reassign_urls(file, &cfg, &ids, &Default::default())
                                .map_err(failure)?;
                        } else {
                            segments::reassign(file, &cfg, &Default::default()).map_err(failure)?;
                        }
                        let assigned = file
                            .get_meta("segments.assignment_seq")
                            .map_err(|e| failure(e.into()))?
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(seq);
                        Ok(assigned)
                    })
                    .await;
                match result {
                    Ok(assigned) => {
                        previous.insert(key, (fingerprint, assigned));
                    }
                    Err(error) => tracing::warn!(?error, "Automatic segment assignment failed"),
                }
            }
            previous.retain(|key, _| seen.contains(key));
        }
    });
}

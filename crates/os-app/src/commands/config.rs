//! Konfiguration der Sitzung und des nächsten Crawls (CRAWL-126, ONPAGE-002).
use crate::{
    dto::*,
    registry::{CmdResult, CommandMeta, Registry},
    runtime::{
        error::{parse_args, store_error, to_json},
        lifecycle_bus::CrawlEvent,
        App, CallCtx, WindowId,
    },
};
use futures_util::future::BoxFuture;
use os_model::{AnalysisStep, CrawlConfig, CrawlState, PresetGroup};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Drafts(Mutex<HashMap<WindowId, Arc<CrawlConfig>>>);
#[derive(Default)]
struct ConfigLock(tokio::sync::Mutex<()>);

pub(super) fn dto(cfg: &CrawlConfig) -> Result<CrawlConfigDto, CmdError> {
    Ok(CrawlConfigDto {
        schema_version: cfg.schema_version,
        generator: cfg.generator.clone(),
        config: to_json(cfg)?,
    })
}

pub(super) fn decode(dto: CrawlConfigDto) -> Result<CrawlConfig, CmdError> {
    let mut cfg: CrawlConfig = serde_json::from_value(dto.config)
        .map_err(|_| CmdError::invalid_input("err-config-type", "config"))?;
    cfg.schema_version = dto.schema_version;
    cfg.generator = dto.generator;
    Ok(cfg)
}

pub(super) fn issues(cfg: &CrawlConfig) -> Vec<ValidationIssue> {
    let report = cfg.validate();
    // The engine checks (regex dialect, rule sets) repeat findings of the model validation with a 0-based line;
    // one message per path is enough (CRAWL-109/110).
    let known: std::collections::HashSet<_> =
        report.errors.iter().map(|i| i.path.clone()).collect();
    let extra: Vec<_> = os_engine::config_check::validate(cfg)
        .into_iter()
        .filter(|i| !known.contains(&i.path))
        .collect();
    report
        .errors
        .into_iter()
        .map(|i| (i, false))
        .chain(extra.into_iter().map(|i| {
            let warning = matches!(
                i.kind,
                os_model::ConfigIssueKind::Warning
                    | os_model::ConfigIssueKind::UnknownField
                    | os_model::ConfigIssueKind::NewerSchemaVersion
            );
            (i, warning)
        }))
        .chain(report.warnings.into_iter().map(|i| (i, true)))
        .map(|(i, warning)| {
            let mut args = BTreeMap::from([
                ("reason".into(), Value::String(i.message)),
                ("code".into(), Value::String(i.code)),
            ]);
            if let Some(position) = i.position {
                args.insert("position".into(), position.into());
            }
            ValidationIssue {
                path: i.path,
                message_key: "err-config-validation".into(),
                args,
                line: i.line,
                column: i.column,
                warning,
            }
        })
        .collect()
}

pub(super) fn valid(cfg: &CrawlConfig) -> Result<(), CmdError> {
    if let Some(issue) = issues(cfg).into_iter().find(|i| !i.warning) {
        // Never include supplied field values (credentials can arrive in malformed input).
        return Err(CmdError::invalid_input("err-config-validation", issue.path));
    }
    Ok(())
}

pub(super) fn warn_loaded(app: &App, loaded: &os_model::LoadedConfig) {
    for issue in &loaded.warnings {
        app.push_notice(Notice {
            level: NoticeLevel::Warning,
            code: "notice-config-warning".into(),
            args: BTreeMap::from([("field".into(), Value::String(issue.path.clone()))]),
        });
    }
}

/// The not yet started configuration of the window (`config.set` without an open crawl, A01-04/CRAWL-126).
pub(crate) fn draft(app: &App, window: &WindowId) -> Option<Arc<CrawlConfig>> {
    app.ext::<Drafts>()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(window)
        .cloned()
}

async fn current(app: &App, window: &WindowId) -> Result<Arc<CrawlConfig>, CmdError> {
    if let Some(s) = app.sessions().current(window) {
        return Ok(s.config);
    }
    if let Some(cfg) = draft(app, window) {
        return Ok(cfg);
    }
    let settings = app.config().paths.settings.clone();
    let loaded = app
        .write_queue()
        .submit_admin(move || {
            CrawlConfig::load_default(&settings)
                .map_err(|_| CmdError::new(ErrorCode::Io, "err-default-profile"))
        })
        .await?;
    valid(&loaded.config)?;
    warn_loaded(app, &loaded);
    Ok(Arc::new(loaded.config))
}

fn get<'a>(app: &'a App, ctx: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move { to_json(&dto(current(app, &ctx.window).await?.as_ref())?) })
}
fn validate<'a>(_: &'a App, _: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let input: CrawlConfigDto = parse_args(args)?;
        match decode(input) {
            Ok(cfg) => to_json(&issues(&cfg)),
            Err(e) => to_json(&vec![ValidationIssue {
                path: e.field.unwrap_or_else(|| "config".into()),
                message_key: e.message_key,
                args: BTreeMap::new(),
                line: None,
                column: None,
                warning: false,
            }]),
        }
    })
}

fn changed(a: &Value, b: &Value, prefix: &str, out: &mut Vec<String>) {
    if a == b {
        return;
    }
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
        for key in keys {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            changed(
                a.get(key).unwrap_or(&Value::Null),
                b.get(key).unwrap_or(&Value::Null),
                &path,
                out,
            );
        }
    } else {
        out.push(prefix.to_owned());
    }
}

fn live_field(path: &str) -> bool {
    PresetGroup::ALL.iter().any(|g| g.keys().contains(&path))
        || ["duplicates.", "links.", "segments.", "analysis."]
            .iter()
            .any(|p| path.starts_with(p))
        || matches!(path, "speed.threads" | "issues.ignore_non_indexable")
}

fn check_state(state: CrawlState, paths: &[String]) -> Result<(), CmdError> {
    for path in paths {
        let forbidden = if state == CrawlState::Running {
            !live_field(path)
        } else {
            // CRAWL-126: nur strukturelle Felder aus osconfig.schema.json.
            // Start-URL liegt in der Sitzung; das Schema hat kein Proxy-Feld.
            matches!(
                path.as_str(),
                "mode"
                    | "rendering.mode"
                    | "scope.all_subdomains"
                    | "scope.subfolder_only"
                    | "scope.crawl_outside_start_folder"
            )
        };
        if forbidden {
            let mut e = CmdError::new(
                if state == CrawlState::Running {
                    ErrorCode::CrawlRunning
                } else {
                    ErrorCode::InvalidInput
                },
                if state == CrawlState::Running {
                    "err-config-pause-required"
                } else {
                    "err-config-new-crawl-required"
                },
            );
            e.field = Some(path.clone());
            return Err(e);
        }
    }
    Ok(())
}

fn invalidated(old: &CrawlConfig, new: &CrawlConfig) -> Vec<AnalysisStep> {
    AnalysisStep::ALL
        .iter()
        .copied()
        .filter(|s| {
            os_analyze::state::step_config_hash(*s, old)
                != os_analyze::state::step_config_hash(*s, new)
                || old.analysis.step_enabled(*s) != new.analysis.step_enabled(*s)
                || (*s == AnalysisStep::Links && old.link_positions != new.link_positions)
        })
        .collect()
}

fn set<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let cfg = Arc::new(decode(parse_args(args)?)?);
        valid(&cfg)?;
        let lock = app.ext::<ConfigLock>();
        let _guard = lock.0.lock().await;
        let old = current(app, &ctx.window).await?;
        let mut paths = Vec::new();
        changed(&to_json(&*old)?, &to_json(&*cfg)?, "", &mut paths);
        paths.retain(|p| !matches!(p.as_str(), "generator" | "schema_version"));
        let steps = invalidated(&old, &cfg);
        let mut deferred = Vec::new();
        if let Some(session) = app.sessions().current(&ctx.window) {
            if !session.writable {
                return Err(CmdError::new(ErrorCode::Permission, "err-db-read-only"));
            }
            check_state(session.state, &paths)?;
            if session.state == CrawlState::Running {
                deferred = paths
                    .iter()
                    .filter(|p| p.as_str() != "speed.threads")
                    .cloned()
                    .collect();
            }
            // CRAWL-112: the thread count reaches a running engine within 2 s of the change. The durable write below
            // queues behind every earlier job of the app's main write lane (view caches; retention runs on its own
            // maintenance lane since M2-N31) and behind the engine writer's lock, so the engine gets the new value
            // first; a failed write restores the old one (M2-N30).
            let threads = |n: u32| {
                if let Some(engine) = &session.engine {
                    if let Some(handle) = engine
                        .handle
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .as_ref()
                    {
                        handle.set_threads(n);
                    }
                }
            };
            let live_threads = paths.iter().any(|p| p == "speed.threads");
            if live_threads {
                threads(cfg.speed.threads);
            }
            let (before, after, cancel, clock, stale) = (
                old.clone(),
                cfg.clone(),
                ctx.cancel.clone(),
                app.config().clock.clone(),
                steps.clone(),
            );
            let budget_path = session.path.clone();
            let written = app
                .write_queue()
                .submit_file(&session.key, move |file| {
                    os_store::profile::config_transaction(
                        file,
                        |e| store_error(&e),
                        |file| {
                            os_engine::recompute::run_triggered(file, &before, &after, &cancel)
                                .map_err(|_| {
                                    CmdError::new(
                                        if cancel.is_cancelled() {
                                            ErrorCode::Cancelled
                                        } else {
                                            ErrorCode::Engine
                                        },
                                        "err-config-recompute",
                                    )
                                })?;

                            if cancel.is_cancelled() {
                                return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
                            }
                            if before.scope.include != after.scope.include
                                || before.scope.exclude != after.scope.exclude
                                || before.scope.regex_partial_match
                                    != after.scope.regex_partial_match
                            {
                                let rules = os_url::RuleSet::compile(
                                    &after.scope.include,
                                    &after.scope.exclude,
                                    after.scope.regex_partial_match,
                                )
                                .map_err(|_| {
                                    CmdError::invalid_input("err-rule-pattern", "scope")
                                })?;
                                os_store::profile::refilter_config_queue(file, |url| {
                                    if cancel.is_cancelled() {
                                        return Err(os_store::StoreError::InvalidInput(
                                            "abgebrochen".into(),
                                        ));
                                    }
                                    Ok(match rules.decide(url) {
                                        os_url::RuleDecision::Allowed => None,
                                        os_url::RuleDecision::Excluded(_) => {
                                            Some(os_model::NotCrawledReason::Excluded)
                                        }
                                        os_url::RuleDecision::NotIncluded => {
                                            Some(os_model::NotCrawledReason::NotIncluded)
                                        }
                                    })
                                })
                                .map_err(|e| {
                                    if cancel.is_cancelled() {
                                        CmdError::new(ErrorCode::Cancelled, "err-cancelled")
                                    } else {
                                        store_error(&e)
                                    }
                                })?;
                            }
                            file.set_config_with_clock(&after, clock.as_ref())
                                .map_err(|e| store_error(&e))?;
                            os_analyze::analysis::mark_stale(file, &stale).map_err(|_| {
                                CmdError::new(ErrorCode::Store, "err-analysis-stale")
                            })?;
                            if cancel.is_cancelled() {
                                return Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"));
                            }
                            Ok(())
                        },
                    )
                })
                .await;
            if let Err(error) = written {
                if live_threads {
                    threads(old.speed.threads);
                }
                return Err(error);
            }
            // M4-D05 (AI-013; U01, „Benötigte Änderungen“ 4): `ai.budget` gilt für die geöffnete Crawl-Datei –
            // gespeichert in `api_budget`, wirksam im gemeinsamen Ledger eines laufenden KI-Laufs; eine Erhöhung hebt
            // eine Sperre nach einer Überschreitung auf.
            if old.ai.budget != cfg.ai.budget {
                let limits = cfg.ai.budget.clone();
                // Ohne laufenden Lauf: Ledger mit den bisherigen Grenzen (Datei, sonst altes Profil), damit eine Erhöhung als
                // solche erkannt wird.
                let previous = old.ai.budget.clone();
                app.write_queue()
                    .submit_file(&session.key, move |file| {
                        let conn = file.conn();
                        let ledger = match os_ai::run::controls().active_ledger(&budget_path) {
                            Some(ledger) => ledger,
                            None => Arc::new(
                                os_store::ai::budget::open_ledger(conn, previous, &[])
                                    .map_err(|e| store_error(&e))?,
                            ),
                        };
                        os_store::ai::budget::raise_limits(conn, &ledger, limits)
                            .map_err(|e| store_error(&e))
                    })
                    .await?;
            }
            app.sessions().update_config(&session.key, cfg.clone());
            app.crawl_events().emit(
                app,
                CrawlEvent::ConfigChanged {
                    key: session.key,
                    old,
                    new: cfg,
                },
            );
        } else {
            app.ext::<Drafts>()
                .0
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(ctx.window, cfg);
        }
        to_json(&ConfigSetResult {
            invalidated_steps: steps.iter().map(ToString::to_string).collect(),
            deferred,
        })
    })
}
fn schema<'a>(_: &'a App, _: CallCtx, _: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async { Ok(os_model::config::json_schema().clone()) })
}
fn diff_rows(a: &Value, b: &Value, prefix: &str, rows: &mut Vec<DiffRow>) {
    if a == b {
        return;
    }
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
        for key in keys {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            diff_rows(
                a.get(key).unwrap_or(&Value::Null),
                b.get(key).unwrap_or(&Value::Null),
                &path,
                rows,
            );
        }
    } else {
        rows.push(DiffRow {
            path: prefix.into(),
            label_key: format!("config-{prefix}"),
            current: a.clone(),
            base: b.clone(),
        });
    }
}

fn diff<'a>(app: &'a App, ctx: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let ConfigDiffArgs { base } = parse_args(args)?;
        let base = match base {
            ConfigDiffBase::Defaults => CrawlConfig::default(),
            ConfigDiffBase::Profile { name } => {
                let store = os_store::profile::ConfigProfiles::new(&app.config().paths.settings);
                app.write_queue()
                    .submit_admin(move || {
                        store
                            .load(&name)
                            .map(|c| c.config)
                            .map_err(|e| store_error(&e))
                    })
                    .await?
            }
            ConfigDiffBase::Crawl { crawl_id } => {
                app.read_pool()
                    .run(crate::pool::CrawlKey(crawl_id), |file, _| file.config())
                    .await?
            }
        };
        // Plan M4 E1 (E-2 b, M4-F02): ohne Schalter „KI (Vorschau)“ läuft ein Crawl auf der eingeschränkten Laufkopie;
        // beide Seiten gleich einschränken, damit der Vergleich keine KI-Schlüssel als Abweichung meldet.
        let preview = super::ai_preview::switch(app);
        let mut current = (*current(app, &ctx.window).await?).clone();
        let mut base = base;
        preview.restrict(&mut current);
        preview.restrict(&mut base);
        let a = to_json(&current)?;
        let b = to_json(&base)?;
        let mut rows = Vec::new();
        diff_rows(&a, &b, "", &mut rows);
        rows.retain(|r| !matches!(r.path.as_str(), "generator" | "schema_version"));
        to_json(&rows)
    })
}

fn test_rules<'a>(_: &'a App, _: CallCtx, args: Value) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        let args: TestRulesArgs = parse_args(args)?;
        let mut output = args.input.clone();
        let mut steps = Vec::new();
        let mut included = None;
        match args.kind {
            RuleListKind::IncludeExclude => {
                let input = os_url::rules::RuleTraceInput {
                    include: args
                        .rules
                        .iter()
                        .filter(|r| r.include == Some(true))
                        .map(|r| r.pattern.clone())
                        .collect(),
                    exclude: args
                        .rules
                        .iter()
                        .filter(|r| r.include != Some(true))
                        .map(|r| r.pattern.clone())
                        .collect(),
                    partial_match: args.regex_partial_match.unwrap_or(false),
                };
                let all = os_url::rules::RuleSet::compile(
                    &input.include,
                    &input.exclude,
                    input.partial_match,
                )
                .map_err(|_| CmdError::invalid_input("err-rule-pattern", "rules"))?;
                included = Some(matches!(
                    all.decide(&output),
                    os_url::rules::RuleDecision::Allowed
                ));
                for (index, r) in args.rules.iter().enumerate() {
                    let one = os_url::rules::RuleTraceInput {
                        include: vec![r.pattern.clone()],
                        exclude: vec![],
                        partial_match: input.partial_match,
                    };
                    let trace = os_url::rules::trace(&one, &output);
                    steps.push(RuleStep {
                        index: index as u32,
                        matched: !trace.matched.is_empty(),
                        before: output.clone(),
                        after: output.clone(),
                        error: trace.warnings.first().cloned(),
                    });
                }
            }
            RuleListKind::Rewrite | RuleListKind::Mapping => {
                let mut cfg = os_model::RewriteConfig {
                    remove_parameters: args.remove_parameters.unwrap_or_default(),
                    remove_all_parameters: args.remove_all_parameters.unwrap_or(false),
                    regex_replace: vec![],
                };
                output = os_url::rewrite::apply_discovered(&cfg, &output)
                    .url
                    .into_owned();
                cfg.remove_parameters.clear();
                cfg.remove_all_parameters = false;
                for (index, r) in args.rules.iter().enumerate() {
                    cfg.regex_replace = vec![os_model::RegexReplace {
                        pattern: r.pattern.clone(),
                        replacement: r.replacement.clone().unwrap_or_default(),
                    }];
                    let before = output.clone();
                    let rules =
                        match os_url::RuleSet::compile(std::slice::from_ref(&r.pattern), &[], true)
                        {
                            Ok(rules) => rules,
                            Err(_) => {
                                steps.push(RuleStep {
                                    index: index as u32,
                                    matched: false,
                                    before: before.clone(),
                                    after: before,
                                    error: Some("invalid regular expression".into()),
                                });
                                continue;
                            }
                        };
                    let matched = matches!(rules.decide(&before), os_url::RuleDecision::Allowed);
                    let result = os_url::rewrite::apply_discovered(&cfg, &before);
                    output = result.url.into_owned();
                    let error = result.warning;
                    steps.push(RuleStep {
                        index: index as u32,
                        matched,
                        before,
                        after: output.clone(),
                        error,
                    });
                }
            }
        }
        to_json(&RuleTrace {
            output,
            included,
            steps,
        })
    })
}

/// Unveränderter IPC-Katalog.
pub fn register(r: &mut Registry) {
    for (name, args, ret, handler) in [
        (
            "config.get",
            "void",
            "CrawlConfigDto",
            get as crate::registry::Handler,
        ),
        (
            "config.validate",
            "CrawlConfigDto",
            "ValidationIssue[]",
            validate,
        ),
        ("config.set", "CrawlConfigDto", "ConfigSetResult", set),
        ("config.schema", "void", "JsonValue", schema),
        ("config.diff", "ConfigDiffArgs", "DiffRow[]", diff),
        ("config.testRules", "TestRulesArgs", "RuleTrace", test_rules),
    ] {
        r.register(
            CommandMeta {
                name,
                args,
                ret,
                long_running: false,
            },
            handler,
        );
    }
}

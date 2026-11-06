//! Crawl tool arguments and safe overrides (AGENT-020). Profiles are local named .osconfig files.
use super::common::{internal, open_crawl, opt_str, reject_unknown};
use crate::{Tool, ToolClass, ToolContext, ToolError, ToolOutput};
use os_engine::CrawlSource;
use os_model::CrawlConfig;
use serde_json::{json, Value};

/// Engine-backed crawl operation.
pub struct CrawlTool(pub &'static str);

pub(crate) const SAFE_OVERRIDES: &[&str] = &[
    "limits.max_urls",
    "limits.max_depth",
    "limits.max_redirects",
    "limits.max_page_kb",
    "limits.max_folder_depth",
    "limits.max_query_params",
    "limits.max_urls_per_subdomain",
    "limits.max_url_length",
    "limits.max_links_per_url",
    "scope.include",
    "scope.exclude",
    "speed.threads",
    "speed.max_urls_per_sec",
    "user_agent.preset",
    "user_agent.http",
    "user_agent.robots_token",
    "rendering.mode",
    "rendering.ajax_timeout_s",
    "rendering.viewport.width",
    "rendering.viewport.height",
];

fn config(ctx: &ToolContext, args: &Value) -> Result<CrawlConfig, ToolError> {
    let mut cfg = if let Some(profile) = opt_str(args, "config_profile")? {
        if profile.is_empty()
            || !profile
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(ToolError::InvalidArguments(
                "config_profile must be a local profile name (letters, digits, - or _)".into(),
            ));
        }
        let profile_error = |e: std::io::Error| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::InvalidArguments(format!("unknown config_profile {profile}"))
            } else {
                internal(e)
            }
        };
        let root = ctx
            .settings_dir
            .join("profiles")
            .canonicalize()
            .map_err(profile_error)?;
        let path = root
            .join(format!("{profile}.osconfig"))
            .canonicalize()
            .map_err(profile_error)?;
        if !path.starts_with(&root) {
            return Err(ToolError::InvalidArguments(
                "profile outside profile directory".into(),
            ));
        }
        CrawlConfig::load_file(&path).map_err(internal)?.config
    } else if let Some(cfg) = ctx.session.as_ref().and_then(|s| s.current_config()) {
        cfg
    } else {
        CrawlConfig::load_default(&ctx.settings_dir)
            .map_err(internal)?
            .config
    };
    if let Some(overrides) = args.get("overrides") {
        let map = overrides.as_object().ok_or_else(|| {
            ToolError::InvalidArguments("overrides must be an object of dotted config keys".into())
        })?;
        for (key, value) in map {
            if !SAFE_OVERRIDES.contains(&key.as_str()) {
                return Err(ToolError::InvalidArguments(format!("Unsafe override {key}; allowed: {}. Auth, proxy and Custom JS require a saved profile.",SAFE_OVERRIDES.join(", "))));
            }
            let mut doc = serde_json::to_value(&cfg).map_err(internal)?;
            let pointer = format!("/{}", key.replace('.', "/"));
            *doc.pointer_mut(&pointer)
                .ok_or_else(|| ToolError::InvalidArguments(format!("Unknown override {key}")))? =
                value.clone();
            cfg = serde_json::from_value(doc)
                .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;
        }
    }
    let validation = cfg.validate();
    if !validation.errors.is_empty() {
        return Err(ToolError::InvalidArguments(format!(
            "Invalid crawl configuration: {}",
            validation
                .errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    Ok(cfg)
}

impl Tool for CrawlTool {
    fn name(&self) -> &'static str {
        self.0
    }
    fn class(&self) -> ToolClass {
        ToolClass::Crawl
    }
    fn description(&self) -> &'static str {
        match self.0 {
            "start_crawl" => "Start a spider crawl with url (or start_url), or a list crawl with url_list (or urls). config_profile selects profiles/<name>.osconfig. overrides uses dotted keys: limits.*, scope.include/exclude, speed.threads/max_urls_per_sec, user_agent.preset/http/robots_token, rendering.mode/ajax_timeout_s/viewport.width/height. The exact allowlist is in input_schema. Private targets are blocked at every connection unless enabled in mcp.toml. JavaScript requires an available rendering engine. Returns crawl_id immediately.",
            "crawl_status" => "Read crawl progress, URLs/s and error counts by crawl_id (or id).",
            "pause_crawl" => "Pause a running crawl by crawl_id (or id) so that resume_crawl can continue it later; returns at once with pausing=true, poll crawl_status until the state is Paused. A crawl run by another process cannot be controlled. Error code busy (retryable) while a pause or stop is still in progress.",
            "stop_crawl" => "Stop a running or paused crawl for good by crawl_id (or id): end reason stopped, the data stays saved, and it can never be resumed (use pause_crawl to halt resumably). Error code busy (retryable) while a pause or stop is still in progress.",
            _ => "Resume a paused, interrupted or limit-ended crawl by crawl_id (or id). Stopped crawls are final and are rejected; a crawl run by another process cannot be controlled. Error code busy (retryable) while the crawl is still pausing.",
        }
    }
    fn input_schema(&self) -> Value {
        if self.0 == "start_crawl" {
            let properties: serde_json::Map<String, Value> = SAFE_OVERRIDES
                .iter()
                .map(|s| (s.to_string(), json!({})))
                .collect();
            json!({"type":"object","properties":{"url":{"type":"string"},"start_url":{"type":"string"},"url_list":{"type":"array","minItems":1,"items":{"type":"string"}},"urls":{"type":"array","minItems":1,"items":{"type":"string"}},"config_profile":{"type":"string"},"overrides":{"type":"object","properties":properties,"additionalProperties":false}},"additionalProperties":false})
        } else {
            json!({"type":"object","properties":{"crawl_id":{"type":"string"},"id":{"type":"string"}},"additionalProperties":false})
        }
    }
    fn call(&self, ctx: &ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        // Each tool keeps its own frame: the start path's configuration copies do not sit below
        // the status path's crawl-file reads on the blocking thread (AGENT-020).
        let summary = if self.0 == "start_crawl" {
            start(ctx, &args)?
        } else {
            reject_unknown(&args, &["crawl_id", "id"])?;
            let id = opt_str(&args, "crawl_id")?
                .or(opt_str(&args, "id")?)
                .ok_or_else(|| {
                    ToolError::InvalidArguments("crawl_id required (list_crawls)".into())
                })?;
            match self.0 {
                "pause_crawl" => {
                    if let Some(session) = &ctx.session {
                        session.pause_crawl(id).map_err(session_error)?;
                    } else {
                        ctx.crawls.pause(ctx, id)?;
                    }
                    json!({"crawl_id":id,"pausing":true})
                }
                "stop_crawl" => {
                    if let Some(session) = &ctx.session {
                        session.stop_crawl(id).map_err(session_error)?;
                        json!({"crawl_id":id,"stopping":true})
                    } else {
                        ctx.crawls.stop(ctx, id)?
                    }
                }
                "resume_crawl" => {
                    if let Some(session) = &ctx.session {
                        let crawl = session
                            .resume_crawl(id, ctx.target_policy.clone())
                            .map_err(session_error)?;
                        json!({"crawl_id":crawl.crawl_id,"state":crawl.state.as_str()})
                    } else {
                        ctx.crawls.resume(ctx, id)?
                    }
                }
                _ => status(ctx, id)?,
            }
        };
        Ok(ToolOutput {
            summary,
            untrusted: None,
            total: None,
            next_cursor: None,
        })
    }
}

/// Fehler der App-Sitzung als MCP-Fehler (K-38): `busy` bleibt als wiederholbarer Code erhalten (AGENT-020);
/// unbekannte Crawls, fremd gesteuerte Crawls und abgewiesene Aufträge kann das Modell selbst korrigieren.
fn session_error(e: crate::SessionError) -> ToolError {
    match e {
        crate::SessionError::Busy(message) => ToolError::Busy(message),
        crate::SessionError::NotFound(_) => ToolError::NotFound(e.to_string()),
        crate::SessionError::NotControllable(_) | crate::SessionError::Invalid(_) => {
            ToolError::InvalidArguments(e.to_string())
        }
        crate::SessionError::Internal(_) => internal(e),
    }
}

fn start(ctx: &ToolContext, args: &Value) -> Result<Value, ToolError> {
    reject_unknown(
        args,
        &[
            "url",
            "start_url",
            "url_list",
            "urls",
            "config_profile",
            "overrides",
        ],
    )?;
    let sources = ["url", "start_url", "url_list", "urls"]
        .iter()
        .filter(|k| args.get(**k).is_some())
        .count();
    if sources != 1 {
        return Err(ToolError::InvalidArguments(
            "Supply exactly one of url, start_url, url_list or urls".into(),
        ));
    }
    let source = if let Some(items) = args.get("url_list").or_else(|| args.get("urls")) {
        let items = items.as_array().filter(|a| !a.is_empty()).ok_or_else(|| {
            ToolError::InvalidArguments("url_list must be a nonempty list of URLs".into())
        })?;
        let items = items
            .iter()
            .map(|v| {
                v.as_str().map(str::to_owned).ok_or_else(|| {
                    ToolError::InvalidArguments("url_list entries must be strings".into())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for url in &items {
            ctx.target_policy
                .check_url(url)
                .map_err(ToolError::InvalidArguments)?;
        }
        CrawlSource::List {
            items,
            label: "MCP".into(),
        }
    } else {
        let url = opt_str(args, "url")?
            .or(opt_str(args, "start_url")?)
            .ok_or_else(|| ToolError::InvalidArguments("url required".into()))?;
        ctx.target_policy
            .check_url(url)
            .map_err(ToolError::InvalidArguments)?;
        CrawlSource::Spider {
            start_url: url.to_owned(),
        }
    };
    let cfg = config(ctx, args)?;
    if let Some(session) = &ctx.session {
        let source = match source {
            CrawlSource::Spider { start_url } => crate::SessionSource::Spider { url: start_url },
            CrawlSource::List { items, label } => crate::SessionSource::List { items, label },
            CrawlSource::Serp { .. } => {
                return Err(ToolError::InvalidArguments(
                    "SERP inputs are not supported by start_crawl".into(),
                ));
            }
        };
        let crawl = session
            .start_configured(
                crate::SessionStart {
                    source,
                    profile: opt_str(args, "config_profile")?.map(str::to_owned),
                },
                cfg,
                ctx.target_policy.clone(),
            )
            .map_err(session_error)?;
        Ok(json!({"crawl_id":crawl.crawl_id,"state":crawl.state.as_str()}))
    } else {
        ctx.crawls.start(ctx, source, cfg)
    }
}

fn status(ctx: &ToolContext, id: &str) -> Result<Value, ToolError> {
    let crawl = open_crawl(ctx, &json!({"crawl_id":id}))?;
    let errors: i64 = crawl
        .file
        .conn()
        .query_row(
            "SELECT count(*) FROM urls WHERE status_code=0 OR status_code>=400",
            [],
            |r| r.get(0),
        )
        .map_err(internal)?;
    let mut status = ctx
        .session
        .as_ref()
        .and_then(|s| s.status(id))
        .map(|s| json!({"crawl_id":id,"progress":s.progress}))
        .or_else(|| ctx.crawls.status(id))
        .unwrap_or_else(|| json!({"crawl_id":id}));
    let state = match ctx.session.as_ref().and_then(|s| s.status(id)) {
        Some(live) => live.state,
        None => crawl
            .file
            .effective_state(&os_model::SystemClock)
            .map_err(internal)?,
    };
    status["state"] = json!(state.as_str());
    status["errors"] = json!(errors);
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    // spec: AGENT-020
    #[test]
    fn missing_profile_directory_and_file_are_invalid_arguments() {
        let home = tempfile::tempdir().unwrap();
        let mut ctx = ToolContext::new(home.path().join("crawls"));
        ctx.settings_dir = home.path().to_path_buf();
        for create in [false, true] {
            if create {
                std::fs::create_dir(home.path().join("profiles")).unwrap();
            }
            assert!(matches!(config(&ctx, &json!({"config_profile":"missing"})),
                Err(ToolError::InvalidArguments(message)) if message == "unknown config_profile missing"));
        }
        std::fs::remove_dir(home.path().join("profiles")).unwrap();
        std::fs::write(home.path().join("profiles"), "not a directory").unwrap();
        // Unix meldet ENOTDIR (Internal); Windows meldet ERROR_PATH_NOT_FOUND und damit NotFound (InvalidArguments).
        let result = config(&ctx, &json!({"config_profile":"missing"}));
        #[cfg(windows)]
        assert!(matches!(result, Err(ToolError::InvalidArguments(_))));
        #[cfg(not(windows))]
        assert!(matches!(result, Err(ToolError::Internal(_))));
    }
}

//! Crawl-Verwaltung (M2-O02): Index, portable Snapshots und Aufbewahrung.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use os_model::settings::AppSettings;
use os_model::{CrawlId, CrawlMode, CrawlState, RetentionPolicy};
use os_store::registry::{self, Manager};

use crate::dto::*;
use crate::pool::CrawlKey;
use crate::registry::{CmdResult, CommandMeta, Registry};
use crate::runtime::error::{internal, parse_args, store_error, to_json};
use crate::runtime::{App, CallCtx};

/// "Show in Finder" waits at most this long for a background WAL merge (CRAWL-126).
const REVEAL_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Default)]
struct ManagementState {
    gate: Arc<tokio::sync::Mutex<()>>,
    /// Register und Metadatenindex (`.registry.sqlite`, `.index.sqlite`): Verwaltungsaufträge der Hauptspur lesen
    /// geteilt, die Aufbewahrung auf der Wartungsspur exklusiv (M2-N31). So greifen nie zwei Verbindungen der App
    /// gleichzeitig schreibend auf den Index zu, und die Hauptspur wartet nie im `busy_timeout` auf die Aufbewahrung.
    registry: Arc<tokio::sync::RwLock<()>>,
    retention_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    retention_settings: Mutex<Option<AppSettings>>,
}
impl ManagementState {
    fn update_retention_settings(&self, current: AppSettings) -> bool {
        let mut previous = self
            .retention_settings
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let changed = previous
            .as_ref()
            .is_none_or(|old| old.retention != current.retention || old.storage != current.storage);
        *previous = Some(current);
        changed
    }
}

impl Drop for ManagementState {
    fn drop(&mut self) {
        if let Some(task) = self
            .retention_task
            .get_mut()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            task.abort();
        }
    }
}

pub(super) fn destination(app: &App) -> PathBuf {
    app.settings_snapshot()
        .storage
        .crawls_dir
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app.config().paths.crawls.clone())
}

pub(super) async fn admin<T: Send + 'static>(
    app: &App,
    f: impl FnOnce(&mut Manager) -> Result<T, CmdError> + Send + 'static,
) -> Result<T, CmdError> {
    let _registry = app
        .ext::<ManagementState>()
        .registry
        .clone()
        .read_owned()
        .await;
    let root = app.config().paths.crawls.clone();
    let dir = destination(app);
    app.write_queue()
        .submit_admin(move || {
            let mut manager = Manager::open(&root).map_err(|e| store_error(&e))?;
            manager.add_location(&dir).map_err(|e| store_error(&e))?;
            f(&mut manager)
        })
        .await
}

/// Wie [`admin`], aber im Hintergrund auf der Wartungsspur der Schreibwarteschlange (M2-N31): Nutzeraufträge der
/// Hauptspur warten nicht dahinter. Nur für Arbeit an nicht geöffneten Crawl-Dateien (Aufbewahrung).
async fn maintenance<T: Send + 'static>(
    app: &App,
    f: impl FnOnce(&mut Manager) -> Result<T, CmdError> + Send + 'static,
) -> Result<T, CmdError> {
    let _registry = app
        .ext::<ManagementState>()
        .registry
        .clone()
        .write_owned()
        .await;
    let root = app.config().paths.crawls.clone();
    let dir = destination(app);
    app.write_queue()
        .submit_maintenance(move || {
            let mut manager = Manager::open(&root).map_err(|e| store_error(&e))?;
            manager.add_location(&dir).map_err(|e| store_error(&e))?;
            f(&mut manager)
        })
        .await
}

pub(super) async fn management_guard(app: &App) -> tokio::sync::OwnedMutexGuard<()> {
    app.ext::<ManagementState>().gate.clone().lock_owned().await
}

fn ids(values: Vec<String>) -> Result<Vec<CrawlId>, CmdError> {
    if values.is_empty() || values.len() > 1000 {
        return Err(CmdError::invalid_input("err-crawl-selection", "ids"));
    }
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|s| {
            s.parse()
                .map_err(|_| CmdError::invalid_input("err-crawl-id", "ids"))
        })
        .filter(|r: &Result<CrawlId, CmdError>| r.as_ref().map_or(true, |id| seen.insert(*id)))
        .collect()
}
fn cancelled(token: &os_analyze::analysis::CancelToken) -> Result<(), CmdError> {
    if token.is_cancelled() {
        Err(CmdError::new(ErrorCode::Cancelled, "err-cancelled"))
    } else {
        Ok(())
    }
}

async fn listing(app: &App, args: serde_json::Value) -> CmdResult {
    let mut query: CrawlListQuery = parse_args(args)?;
    let now = app.config().clock.now();
    let sessions = app.sessions().all();
    if let Some(project) = query.project.clone() {
        query.project = Some(
            admin(app, move |m| {
                Ok(m.projects()
                    .map_err(|e| store_error(&e))?
                    .into_iter()
                    .find(|p| p.id == project || p.name == project)
                    .map(|p| p.name)
                    .unwrap_or(project))
            })
            .await?,
        );
    }
    let data = admin(app, move |m| m.list().map_err(|e| store_error(&e))).await?;
    let mut items = Vec::new();
    for l in data.crawls {
        let state = l.effective_state(now);
        let project = (!l.meta.project.is_empty()).then(|| l.meta.project.clone());
        if query
            .project
            .as_ref()
            .is_some_and(|p| Some(p) != project.as_ref())
        {
            continue;
        }
        let show = match query.show {
            CrawlShow::All | CrawlShow::Crawls => true,
            CrawlShow::Comparisons => false, // Vergleichsreferenzen sind P2 (K2-32).
            CrawlShow::Running => state == CrawlState::Running,
            CrawlShow::Completed => state == CrawlState::Completed,
            CrawlShow::Incomplete => state == CrawlState::Incomplete,
            CrawlShow::Locked => l.locked,
        };
        if !show {
            continue;
        }
        if let Some(search) = query.search.as_ref().filter(|s| !s.is_empty()) {
            let needle = search.to_lowercase();
            if ![
                l.meta.name.as_str(),
                l.meta.project.as_str(),
                l.meta.start_url.as_deref().unwrap_or_default(),
            ]
            .iter()
            .any(|v| v.to_lowercase().contains(&needle))
            {
                continue;
            }
        }
        let own = sessions
            .iter()
            .any(|s| s.key.0 == l.meta.crawl_id.to_string() && s.engine.is_some());
        let can_resume =
            l.meta.end_reason != Some(os_model::EndReason::Stopped) && l.can_resume(now);
        let duration_ms = l.duration_ms.or_else(|| {
            l.meta.started_at.map(|start| {
                let end = l.meta.ended_at.or(l.meta.heartbeat_at).unwrap_or(now);
                end.unix_millis().saturating_sub(start.unix_millis()).max(0) as u64
            })
        });
        items.push(CrawlListing {
            crawl: CrawlSummaryDto {
                crawl_id: l.meta.crawl_id.to_string(),
                file: l.path.to_string_lossy().into_owned(),
                name: l.meta.name,
                phase: match state {
                    CrawlState::Running => CrawlPhase::Running,
                    CrawlState::Paused => CrawlPhase::Paused,
                    CrawlState::Completed => CrawlPhase::Completed,
                    CrawlState::Incomplete => CrawlPhase::Incomplete,
                    CrawlState::Failed => CrawlPhase::Failed,
                },
                mode: match l.meta.mode {
                    CrawlMode::Spider => CrawlModeDto::Spider,
                    CrawlMode::List => CrawlModeDto::List,
                    CrawlMode::Serp => CrawlModeDto::Serp,
                },
                started_at: l.meta.started_at.map(|t| t.to_db()).unwrap_or_default(),
                finished_at: l.meta.ended_at.map(|t| t.to_db()),
                urls: l.url_count,
                file_bytes: l.file_bytes,
                schema_version: l.meta.schema_version,
                owner: CrawlOwner {
                    kind: if own { OwnerKind::App } else { OwnerKind::Cli },
                    pid: own.then_some(std::process::id()),
                    live: state == CrawlState::Running,
                    controllable: own,
                },
            },
            project,
            locked: l.locked,
            start_url: l.meta.start_url,
            duration_ms,
            can_resume,
        });
    }
    if let Some(sort) = query.sort {
        let valid = [
            "id",
            "crawlId",
            "project",
            "name",
            "mode",
            "startUrl",
            "started",
            "startedAt",
            "duration",
            "durationMs",
            "urls",
            "sizeOnDisk",
            "fileBytes",
            "status",
            "phase",
            "locked",
        ];
        if !valid.contains(&sort.column.as_str()) {
            return Err(CmdError::invalid_input("err-crawl-sort", "sort"));
        }
        items.sort_by(|a, b| {
            let ordering = match sort.column.as_str() {
                "id" | "crawlId" => a.crawl.crawl_id.cmp(&b.crawl.crawl_id),
                "project" => a
                    .project
                    .as_ref()
                    .map(|s| s.to_lowercase())
                    .cmp(&b.project.as_ref().map(|s| s.to_lowercase())),
                "name" => a
                    .crawl
                    .name
                    .to_lowercase()
                    .cmp(&b.crawl.name.to_lowercase()),
                "mode" => format!("{:?}", a.crawl.mode).cmp(&format!("{:?}", b.crawl.mode)),
                "startUrl" => a.start_url.cmp(&b.start_url),
                "duration" | "durationMs" => a.duration_ms.cmp(&b.duration_ms),
                "urls" => a.crawl.urls.cmp(&b.crawl.urls),
                "sizeOnDisk" | "fileBytes" => a.crawl.file_bytes.cmp(&b.crawl.file_bytes),
                "status" | "phase" => {
                    format!("{:?}", a.crawl.phase).cmp(&format!("{:?}", b.crawl.phase))
                }
                "locked" => a.locked.cmp(&b.locked),
                _ => a.crawl.started_at.cmp(&b.crawl.started_at),
            }
            .then_with(|| a.crawl.crawl_id.cmp(&b.crawl.crawl_id));
            if sort.dir == SortDir::Desc {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
    to_json(&items)
}

async fn short(app: &App, args: serde_json::Value, action: &'static str) -> CmdResult {
    if action == "crawls.list" {
        return listing(app, args).await;
    }
    if action == "crawls.checkLocation" {
        let PathArgs { path } = parse_args(args)?;
        let warnings = tokio::task::spawn_blocking(move || {
            os_store::files::location_warnings(Path::new(&path))
        })
        .await
        .map_err(|e| internal(action, e.to_string()))?;
        return to_json(
            &warnings
                .iter()
                .map(|w| match w {
                    os_store::files::LocationWarning::SyncFolder(_) => "warn-location-sync",
                    os_store::files::LocationWarning::NetworkDrive => "warn-location-network",
                    os_store::files::LocationWarning::FatFilesystem => "warn-location-filesystem",
                })
                .collect::<Vec<_>>(),
        );
    }
    let _guard = management_guard(app).await;
    let now = app.config().clock.now();
    if action == "crawls.rename" {
        let CrawlRenameArgs { crawl_id, name } = parse_args(args)?;
        let id = ids(vec![crawl_id])?[0];
        admin(app, move |m| {
            m.rename(&id, &name, now).map_err(|e| store_error(&e))
        })
        .await?;
        return Ok(serde_json::Value::Null);
    }
    let IdsArgs { ids: raw } = parse_args(args)?;
    let selected = ids(raw)?;
    if action == "crawls.lock" || action == "crawls.unlock" {
        let locked = action == "crawls.lock";
        admin(app, move |m| {
            for id in &selected {
                m.get(id).map_err(|e| store_error(&e))?;
            }
            for id in &selected {
                m.lock(id, locked, now).map_err(|e| store_error(&e))?;
            }
            Ok(())
        })
        .await?;
    } else {
        let rows = admin(app, move |m| {
            selected
                .iter()
                .map(|id| m.get(id).map_err(|e| store_error(&e)))
                .collect::<Result<Vec<_>, _>>()
        })
        .await?;
        let desktop = app.desktop_actions().ok_or_else(|| {
            CmdError::new(ErrorCode::Unsupported, "err-desktop-actions-unavailable")
        })?;
        if action == "crawls.copyIds" {
            desktop.write_clipboard(
                &rows
                    .iter()
                    .map(|l| l.meta.crawl_id.to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )?;
        } else {
            // CRAWL-126: prefer a self-contained file, but a foreign snapshot (e.g. a CLI query) must not
            // block the call: after the budget the file is shown anyway, with a note.
            let keys: Vec<_> = rows
                .iter()
                .map(|l| CrawlKey(l.meta.crawl_id.to_string()))
                .collect();
            let merged = crate::session::merge::wait_merged_within(app, &keys, REVEAL_BUDGET).await;
            desktop.reveal_paths(&rows.into_iter().map(|l| l.path).collect::<Vec<_>>())?;
            if !merged {
                app.push_notice(Notice {
                    level: NoticeLevel::Warning,
                    code: "crawl-merge-pending".into(),
                    args: Default::default(),
                });
            }
        }
    }
    Ok(serde_json::Value::Null)
}

async fn batch(
    app: App,
    args: serde_json::Value,
    action: &'static str,
    token: os_analyze::analysis::CancelToken,
    progress: crate::ops::ProgressSink,
) -> CmdResult {
    let _guard = management_guard(&app).await;
    cancelled(&token)?;
    let now = app.config().clock.now();
    let dir = destination(&app);
    if action == "crawls.import" {
        let CrawlsImportArgs { paths } = parse_args(args)?;
        if paths.is_empty() || paths.len() > 1000 {
            return Err(CmdError::invalid_input("err-crawl-selection", "paths"));
        }
        let mut result = ImportResult {
            crawl_ids: Vec::new(),
            rejected: Vec::new(),
        };
        let count = paths.len() as u64;
        for (i, path) in paths.into_iter().enumerate() {
            cancelled(&token)?;
            let source = PathBuf::from(&path);
            let clock = app.config().clock.clone();
            let ids = app.config().ids.clone();
            let dir = dir.clone();
            let token = token.clone();
            let progress = progress.clone();
            let imported = admin(&app, move |m| {
                m.import(
                    &source,
                    &dir,
                    ids.as_ref(),
                    clock.as_ref(),
                    &|done, total| {
                        progress.report(
                            (i as u64) * 1000 + done.saturating_mul(1000) / total.max(1),
                            Some(count * 1000),
                        );
                        !token.is_cancelled()
                    },
                )
                .and_then(|p| registry::read_listing(&p))
                .map_err(|e| store_error(&e))
            })
            .await;
            match imported {
                Ok(l) => result.crawl_ids.push(l.meta.crawl_id.to_string()),
                Err(e) if e.code == ErrorCode::Cancelled => return Err(e),
                Err(e) => result.rejected.push(ImportRejected {
                    path,
                    reason_key: e.message_key,
                }),
            }
        }
        return to_json(&result);
    }
    let (selected, project, dest) = match action {
        "crawls.move" => {
            let a: CrawlMoveArgs = parse_args(args)?;
            (ids(a.ids)?, a.project, a.destination_dir)
        }
        "crawls.export" => {
            let a: CrawlsExportArgs = parse_args(args)?;
            (ids(a.ids)?, None, Some(a.dest))
        }
        _ => {
            let a: IdsArgs = parse_args(args)?;
            (ids(a.ids)?, None, None)
        }
    };
    let preflight = selected.clone();
    let rows = admin(&app, move |m| {
        preflight
            .iter()
            .map(|id| m.get(id).map_err(|e| store_error(&e)))
            .collect::<Result<Vec<_>, _>>()
    })
    .await?;
    // Alle Zustandsfehler vor dem ersten Dateieingriff prüfen.
    for l in &rows {
        if l.effective_state(now) == CrawlState::Running {
            return Err(CmdError::new(ErrorCode::Busy, "err-crawl-running"));
        }
        if action == "crawls.delete" && l.locked {
            return Err(CmdError::new(ErrorCode::Permission, "err-crawl-locked"));
        }
        if action == "crawls.delete"
            && app
                .sessions()
                .all()
                .iter()
                .any(|s| s.key.0 == l.meta.crawl_id.to_string())
        {
            return Err(CmdError::new(ErrorCode::Busy, "err-crawl-open"));
        }
    }
    let count = selected.len() as u64;
    for (i, id) in selected.into_iter().enumerate() {
        cancelled(&token)?;
        let key = CrawlKey(id.to_string());
        let moving = action == "crawls.move" && dest.is_some();
        let deleting = action == "crawls.delete";
        let old_path = rows[i].path.clone();
        let _exclusive = if moving || deleting {
            Some(app.read_pool().exclusive(&key).await)
        } else {
            None
        };
        // CRAWL-126: move only a merged file; a file about to be deleted needs no merge.
        if moving {
            crate::session::merge::wait_merged(&app, &key).await;
        } else if deleting {
            crate::session::merge::cancel(&app, &key).await;
        }
        if moving || deleting {
            app.write_queue().unregister(&key);
            app.write_queue().flush().await;
        }
        let project = project.clone();
        let dest = dest.clone();
        let dir = dir.clone();
        let clock = app.config().clock.clone();
        let ids = app.config().ids.clone();
        let token = token.clone();
        let progress = progress.clone();
        let result = admin(&app, move |m| {
            let copy_progress = |done: u64, total: u64| {
                progress.report(
                    (i as u64) * 1000 + done.saturating_mul(1000) / total.max(1),
                    Some(count * 1000),
                );
                !token.is_cancelled()
            };
            cancelled(&token)?;
            let changed = match action {
                "crawls.delete" => {
                    m.delete(&id, now).map_err(|e| store_error(&e))?;
                    None
                }
                "crawls.duplicate" => {
                    m.duplicate(&id, &dir, ids.as_ref(), clock.as_ref(), &copy_progress)
                        .map_err(|e| store_error(&e))?;
                    None
                }
                "crawls.export" => {
                    let target = PathBuf::from(dest.as_deref().unwrap_or_default());
                    let target = if count > 1 || target.is_dir() {
                        target.join(registry::file_name(&id))
                    } else {
                        target
                    };
                    m.export(&id, &target, now, &copy_progress)
                        .map_err(|e| store_error(&e))?;
                    None
                }
                _ => {
                    if dest.is_none() || project.is_some() {
                        m.move_project(&id, project.as_deref(), now)
                            .map_err(|e| store_error(&e))?;
                    }
                    if let Some(dest) = dest {
                        Some(
                            m.move_file(&id, Path::new(&dest), now, &copy_progress)
                                .map_err(|e| store_error(&e))?,
                        )
                    } else {
                        None
                    }
                }
            };
            progress.report((i as u64 + 1) * 1000, Some(count * 1000));
            Ok(changed)
        })
        .await;
        match result {
            Ok(Some(path)) => {
                app.sessions().rename_path(&key, path.clone());
                app.register_crawl(key, path);
            }
            Ok(None) if deleting => app.unregister_crawl(&key),
            Ok(None) => {}
            Err(e) => {
                if moving || deleting {
                    app.write_queue().register(key, old_path);
                }
                return Err(e);
            }
        }
    }
    to_json(&CountResult { count })
}

fn dispatch<'a>(
    app: &'a App,
    _ctx: CallCtx,
    args: serde_json::Value,
    action: &'static str,
    long: bool,
) -> BoxFuture<'a, CmdResult> {
    Box::pin(async move {
        if long {
            let cloned = app.clone();
            let handle = app.ops().spawn("op-crawls", true, move |token, progress| {
                Box::pin(batch(cloned, args, action, token, progress))
            });
            to_json(&handle)
        } else {
            short(app, args, action).await
        }
    })
}

/// Behält die eingefrorenen Command-Verträge bei.
pub fn register(r: &mut Registry) {
    macro_rules! command {
        ($name:literal,$args:literal,$ret:literal,$long:literal) => {
            r.register(
                CommandMeta {
                    name: $name,
                    args: $args,
                    ret: $ret,
                    long_running: $long,
                },
                |app, ctx, args| dispatch(app, ctx, args, $name, $long),
            );
        };
    }
    command!("crawls.checkLocation", "PathArgs", "string[]", false);
    command!("crawls.list", "CrawlListQuery", "CrawlListing[]", false);
    command!("crawls.rename", "CrawlRenameArgs", "void", false);
    command!("crawls.move", "CrawlMoveArgs", "CountResult", true);
    command!("crawls.lock", "IdsArgs", "void", false);
    command!("crawls.unlock", "IdsArgs", "void", false);
    command!("crawls.duplicate", "IdsArgs", "CountResult", true);
    command!("crawls.delete", "IdsArgs", "CountResult", true);
    command!("crawls.import", "CrawlsImportArgs", "ImportResult", true);
    command!("crawls.export", "CrawlsExportArgs", "CountResult", true);
    command!("crawls.reveal", "IdsArgs", "void", false);
    command!("crawls.copyIds", "IdsArgs", "void", false);
}

/// Führt eine Aufbewahrungsrunde aus (mit injizierter Uhr testbar).
///
/// Läuft auf der Wartungsspur (M2-N31): Schreibaufträge des Nutzers (Konfiguration, Segmente, View-Cache) warten nicht
/// hinter ihr, auch nicht direkt nach dem Start. Andere Verwaltungsaufträge (Crawl-Liste, Löschen, Verschieben …)
/// bleiben mit ihr serialisiert.
pub async fn run_retention(app: &App) -> Result<usize, CmdError> {
    let _guard = management_guard(app).await;
    let settings = app.settings_snapshot().retention;
    let days = match settings.policy {
        RetentionPolicy::Never => None,
        RetentionPolicy::OneWeek => Some(7),
        RetentionPolicy::OneMonth => Some(30),
        RetentionPolicy::ThreeMonths => Some(90),
        RetentionPolicy::SixMonths => Some(180),
        RetentionPolicy::OneYear => Some(365),
        RetentionPolicy::Custom => Some(settings.custom_days),
    };
    let open: HashSet<CrawlId> = app
        .sessions()
        .all()
        .iter()
        .filter_map(|s| s.key.0.parse().ok())
        .collect();
    let now = app.config().clock.now();
    let deletions = maintenance(app, move |m| {
        m.retain_with(days, settings.keep_last, now, &open, |d| {
            tracing::info!(crawl_id=%d.id,name=%d.name,age_days=d.age_days,"Crawl durch Aufbewahrung gelöscht");
        })
            .map_err(|e| store_error(&e))
    })
    .await?;
    Ok(deletions.len())
}

/// Start sofort und anschließend mindestens einmal pro Stunde, ohne die App am Leben zu halten.
pub fn start_retention(app: &App) {
    app.ext::<ManagementState>()
        .update_retention_settings(app.settings_snapshot());
    let weak = app.downgrade();
    let task = app.handle().spawn(async move {
        loop {
            let Some(app) = weak.upgrade() else {
                break;
            };
            if let Err(e) = run_retention(&app).await {
                tracing::warn!(error=?e,"Aufbewahrung fehlgeschlagen");
            }
            drop(app);
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });
    *app.ext::<ManagementState>()
        .retention_task
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(task);
}

/// Neue Richtlinie/Speicherorte sofort anwenden; ein Speicherortwechsel kopiert keine Crawls.
pub fn on_settings_changed(app: &App) {
    if !app
        .ext::<ManagementState>()
        .update_retention_settings(app.settings_snapshot())
    {
        return;
    }
    let weak = app.downgrade();
    app.handle().spawn(async move {
        if let Some(app) = weak.upgrade() {
            if let Err(e) = run_retention(&app).await {
                tracing::warn!(error=?e,"Aufbewahrung nach Einstellungsänderung fehlgeschlagen");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: OUT-229
    #[test]
    fn retention_reacts_to_policy_and_storage_changes_but_ignores_other_settings() {
        let state = ManagementState::default();
        let mut snapshot = AppSettings::default();
        assert!(state.update_retention_settings(snapshot.clone()));
        assert!(!state.update_retention_settings(snapshot.clone()));
        snapshot.retention.policy = RetentionPolicy::OneWeek;
        assert!(state.update_retention_settings(snapshot.clone()));
        snapshot.general.notifications = !snapshot.general.notifications;
        assert!(!state.update_retention_settings(snapshot.clone()));
        snapshot.appearance.zoom_pct = 125;
        assert!(!state.update_retention_settings(snapshot.clone()));
        snapshot.storage.crawls_dir = Some("external-crawls".into());
        assert!(state.update_retention_settings(snapshot.clone()));
        assert!(!state.update_retention_settings(snapshot.clone()));
        snapshot.retention.policy = RetentionPolicy::Never;
        assert!(state.update_retention_settings(snapshot));
    }
}

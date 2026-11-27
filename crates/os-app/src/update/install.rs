//! Serialized installation: no live crawl can cross the final readiness check.
use super::{
    download::{state, Downloads},
    pending::Pending,
    InstallWhen, UpdateState,
};
use crate::{session::SessionService, App};
use os_model::CrawlState;
use std::sync::atomic::Ordering;

pub fn running(app: &App) -> bool {
    app.sessions()
        .all()
        .iter()
        .any(|s| s.state == CrawlState::Running)
}
fn unfinished(app: &App) -> bool {
    app.sessions()
        .all()
        .iter()
        .any(|s| matches!(s.state, CrawlState::Running | CrawlState::Paused))
}
pub async fn install(app: &App, when: InstallWhen) -> UpdateState {
    let current = app
        .ext::<crate::commands::update::UpdateService>()
        .state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if !matches!(current, UpdateState::Ready(_)) {
        return current;
    }
    if running(app) && when != InstallWhen::AfterCrawl {
        return current;
    }
    *app.ext::<Pending>()
        .when
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(when.clone());
    if when == InstallWhen::OnQuit || (when == InstallWhen::AfterCrawl && unfinished(app)) {
        return current;
    }
    execute(app, true).await
}
pub async fn on_quit(app: &App) -> bool {
    let when = app
        .ext::<Pending>()
        .when
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if when != Some(InstallWhen::OnQuit) {
        return true;
    }
    match execute(app, false).await {
        UpdateState::Failed(_) => {
            app.ext::<Pending>()
                .when
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take();
            tracing::warn!(
                "Update-Installation beim Beenden fehlgeschlagen; Anwendung wird beendet"
            );
            true
        }
        UpdateState::Idle => true,
        _ => false,
    }
}
pub async fn execute(app: &App, restart: bool) -> UpdateState {
    let service = app.ext::<crate::commands::update::UpdateService>();
    let _gate = service.gate.lock().await;
    let current = service
        .state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if !matches!(current, UpdateState::Ready(_)) {
        return current;
    }
    let sessions = app.ext::<SessionService>();
    let _mutation = sessions.mutation.lock().await;
    if running(app) || !app.ops().active_statuses().is_empty() {
        return service
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
    }
    let downloads = app.ext::<Downloads>();
    let artifact = downloads
        .artifact
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let backend = downloads
        .backend
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let (Some((manifest, bytes)), Some(backend)) = (artifact, backend) else {
        super::pending::forget(app).await;
        return state(
            app,
            UpdateState::Failed("Kein vollständiges Update vorhanden".into()),
        );
    };
    sessions.quitting.store(true, Ordering::SeqCst);
    let result = async {
        for s in app.sessions().all() {
            if s.engine
                .as_ref()
                .is_some_and(|e| e.handle.lock().unwrap_or_else(|p| p.into_inner()).is_some())
            {
                crate::session::control::settle(app, &s, Some(false))
                    .await
                    .map_err(|_| "Crawl-Checkpoint fehlgeschlagen".to_owned())?;
            }
        }
        app.write_queue().flush().await;
        for s in app.sessions().all().into_iter().filter(|s| s.writable) {
            app.write_queue()
                .submit_file(&s.key, |file| {
                    let ready = file
                        .checkpoint_and_truncate()
                        .map_err(|e| crate::runtime::error::store_error(&e))?;
                    if !ready {
                        return Err(crate::session::busy());
                    }
                    Ok(())
                })
                .await
                .map_err(|_| "Crawl-Checkpoint fehlgeschlagen".to_owned())?;
        }
        super::pending::remember(app).await?;
        backend.install(manifest, bytes, restart).await
    }
    .await;
    sessions.quitting.store(false, Ordering::SeqCst);
    match result {
        Ok(()) => {
            downloads
                .artifact
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take();
            app.ext::<Pending>()
                .when
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take();
            state(app, UpdateState::Idle)
        }
        Err(_) => {
            super::pending::forget(app).await;
            state(
                app,
                UpdateState::Failed("Update-Installation fehlgeschlagen".into()),
            )
        }
    }
}
pub fn init(app: &App) {
    // Completion may start an analysis operation. Retry the explicitly queued choice
    // after that operation drains; never interrupt it to install an update.
    let weak = app.downgrade();
    app.handle().spawn(async move {
        let mut timer = tokio::time::interval(std::time::Duration::from_millis(500));
        // Der erste Tick kommt sofort: nicht gleich beim Start hochstufen (ein sofortiger Abbau durch den Besitzer
        // bleibt der letzte, OUT-229).
        timer.tick().await;
        loop {
            timer.tick().await;
            let Some(app) = weak.upgrade() else { break };
            let when = app
                .ext::<Pending>()
                .when
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            let eligible = match when {
                Some(InstallWhen::AfterCrawl) => !unfinished(&app),
                Some(InstallWhen::Now) => !running(&app),
                _ => false,
            };
            if eligible && app.ops().active_statuses().is_empty() {
                if let Some(when) = when {
                    install(&app, when).await;
                }
            }
        }
    });
    app.crawl_events().subscribe(Box::new(|app, event| {
        use crate::runtime::lifecycle_bus::CrawlEvent;
        if matches!(event, CrawlEvent::Completed(_) | CrawlEvent::Stopped(_)) {
            let app = app.clone();
            app.handle().spawn(async move {
                let when = app
                    .ext::<Pending>()
                    .when
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone();
                if when == Some(InstallWhen::AfterCrawl) {
                    install(&app, InstallWhen::AfterCrawl).await;
                }
            });
        }
    }));
}

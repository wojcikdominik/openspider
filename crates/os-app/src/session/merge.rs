//! Background WAL merge after a pause or an end (CRAWL-126, OS-Festlegung vom 01.11.2026).
//!
//! The engine reports "Paused" once queue and metadata are durable; its final checkpoint waits at most
//! [`os_store::writer::CHECKPOINT_READER_WAIT`] for readers. Whatever remains in the WAL is merged here in short,
//! cancellable attempts. Quitting and "Show in Finder" wait for the merge; resuming cancels it first.
use crate::{pool::CrawlKey, session::SessionService, App};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::watch;

#[derive(Default)]
pub(crate) struct Merges(Mutex<HashMap<CrawlKey, Merge>>);

struct Merge {
    cancel: Arc<AtomicBool>,
    /// A write after the merge's last attempt (e.g. a stop commit) asks for another round.
    again: Arc<AtomicBool>,
    done: watch::Receiver<bool>,
}

impl Merges {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<CrawlKey, Merge>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Whether a merge for this crawl is still running (status line).
    pub(crate) fn active(&self, crawl_id: &str) -> bool {
        self.lock()
            .iter()
            .any(|(k, m)| k.0 == crawl_id && !*m.done.borrow())
    }
}

/// Starts the merge when the WAL still holds frames. A running merge for the crawl is kept and does one more round,
/// so frames written meanwhile (e.g. by a stop from "Paused") are merged as well.
pub(crate) fn start(app: &App, key: &CrawlKey, path: PathBuf) {
    if os_store::open::wal_bytes(&path) == 0 {
        return;
    }
    let service = app.ext::<SessionService>();
    let mut merges = service.merges.lock();
    if let Some(running) = merges.get(key).filter(|m| !*m.done.borrow()) {
        running.again.store(true, Ordering::SeqCst);
        return;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let again = Arc::new(AtomicBool::new(false));
    let (tx, done) = watch::channel(false);
    merges.insert(
        key.clone(),
        Merge {
            cancel: cancel.clone(),
            again: again.clone(),
            done,
        },
    );
    drop(merges);
    let weak = app.downgrade();
    let key = key.clone();
    app.handle().spawn(async move {
        loop {
            let (file, stop) = (path.clone(), cancel.clone());
            let merged =
                tokio::task::spawn_blocking(move || os_store::open::merge_wal(&file, &stop)).await;
            match merged {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => tracing::warn!(error = %e, "WAL merge failed"),
                Err(e) => tracing::warn!(error = %e, "WAL merge task failed"),
            }
            let Some(app) = weak.upgrade() else {
                let _ = tx.send(true);
                return;
            };
            // Serialised with `start` under the map lock: a request for another round is never lost.
            let ended = {
                let service = app.ext::<SessionService>();
                let _merges = service.merges.lock();
                let ended = cancel.load(Ordering::SeqCst) || !again.swap(false, Ordering::SeqCst);
                if ended {
                    let _ = tx.send(true);
                }
                ended
            };
            if ended {
                finished(&app, &key);
                return;
            }
        }
    });
}

/// Publishes the status without the merge note once the merge has ended.
fn finished(app: &App, key: &CrawlKey) {
    let service = app.ext::<SessionService>();
    let status = service
        .feeds
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(key)
        .and_then(|c| c.status.clone())
        // A stale running status (merge faster than the settle publication) is never republished.
        .filter(|s| s.phase != crate::dto::CrawlPhase::Running);
    if let Some(status) = status {
        service
            .parts
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .crawl = Some(status);
        // While a crawl session is attached its loop delivers every due tick (UI-018, B4-LAGFIX2). A flush from here
        // would send the end status of a crawl before the session loop's final rows and the automatic crawl analysis
        // (B4-LAGFIX3, OUT-020); the loop sends this status with its next tick.
        if !super::delivers_ticks(app) {
            app.flush_tick();
        }
    }
}

fn receiver(app: &App, key: &CrawlKey) -> Option<watch::Receiver<bool>> {
    app.ext::<SessionService>()
        .merges
        .lock()
        .get(key)
        .map(|m| m.done.clone())
}

async fn done(mut rx: watch::Receiver<bool>) {
    let _ = rx.wait_for(|done| *done).await;
}

/// Waits until the WAL of this crawl has been merged (or the merge was cancelled).
pub async fn wait_merged(app: &App, key: &CrawlKey) {
    if let Some(rx) = receiver(app, key) {
        done(rx).await;
    }
}

/// Waits for the merges of these crawls, at most `limit` in total; `false` if one is still running.
pub async fn wait_merged_within(app: &App, keys: &[CrawlKey], limit: Duration) -> bool {
    tokio::time::timeout(limit, async {
        for key in keys {
            wait_merged(app, key).await;
        }
    })
    .await
    .is_ok()
}

/// Waits for every running merge, at most `limit`; `false` if one is still running.
pub async fn wait_all(app: &App, limit: Duration) -> bool {
    let all: Vec<_> = app
        .ext::<SessionService>()
        .merges
        .lock()
        .values()
        .map(|m| m.done.clone())
        .collect();
    tokio::time::timeout(limit, async {
        for rx in all {
            done(rx).await;
        }
    })
    .await
    .is_ok()
}

/// Stops a running merge before the engine writes to the file again (resume) and waits for it.
pub async fn cancel(app: &App, key: &CrawlKey) {
    let rx = {
        let service = app.ext::<SessionService>();
        let merges = service.merges.lock();
        merges.get(key).map(|m| {
            m.cancel.store(true, Ordering::SeqCst);
            m.done.clone()
        })
    };
    if let Some(rx) = rx {
        done(rx).await;
    }
}

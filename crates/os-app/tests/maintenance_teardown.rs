//! OUT-229 / M2-N31: Der Abbau der App (Drop der Schreibwarteschlange) wartet auf eine laufende Aufbewahrungsrunde der
//! Wartungsspur, bevor der Besitzer der App ihr Datenverzeichnis entfernt.
//!
//! Vorher hielt die Hintergrundaufgabe der Aufbewahrung die `App` während der Runde fest: Ließ ein Test seine `App`
//! fallen und löschte sein temporäres Verzeichnis, schrieb die Wartungsspur weiter in `.index.sqlite` (WAL, `-shm` per
//! mmap) und der Testprozess starb sporadisch mit SIGBUS (`update_channels`, `multi_export_commands`).
//!
//! Außerdem: Mit der Standardrichtlinie („Nie“, keine Höchstzahl) läuft keine Runde und die App öffnet beim Start keine
//! Verwaltungsdatei im Hintergrund.
//!
//! Eigene Testdatei (eigener Prozess), weil der erste Test einen globalen `tracing`-Subscriber setzt: Er hält die
//! Wartungsspur in der ersten Löschmeldung der Aufbewahrung an, bis der Test sie freigibt (wie `n31_retention_lane`).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "runtime_common.rs"]
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use os_app::{dto::ErrorCode, settings_store::SettingsStore, write_queue::WriteQueue};
use os_model::{CrawlState, IdSource, SeededIds};
use os_store::{CrawlFile, OpenMode};
use serde_json::json;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

/// Haltepunkt in der ersten Löschmeldung der Aufbewahrung (`Crawl durch Aufbewahrung gelöscht`, Feld `age_days`).
#[derive(Default)]
struct Gate {
    /// Name des angehaltenen Threads, sobald die Aufbewahrung den Haltepunkt erreicht.
    entered: Mutex<Option<String>>,
    entered_changed: Condvar,
    released: Mutex<bool>,
    /// Je Mutex eine eigene Condvar (std erlaubt keine Condvar mit zwei Mutexen).
    released_changed: Condvar,
    /// Anzahl der Löschmeldungen (nur die erste hält an).
    deletions: AtomicUsize,
}

impl Gate {
    fn release(&self) {
        *self.released.lock().unwrap_or_else(|p| p.into_inner()) = true;
        self.released_changed.notify_all();
    }
}

fn gate() -> &'static Gate {
    static GATE: OnceLock<Gate> = OnceLock::new();
    GATE.get_or_init(Gate::default)
}

struct HoldFirstDeletion;

impl<S: tracing::Subscriber> Layer<S> for HoldFirstDeletion {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        if !meta.target().starts_with("os_app::commands::crawls")
            || meta.fields().field("age_days").is_none()
        {
            return;
        }
        let gate = gate();
        if gate.deletions.fetch_add(1, Ordering::SeqCst) > 0 {
            return;
        }
        *gate.entered.lock().unwrap_or_else(|p| p.into_inner()) = Some(
            std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_string(),
        );
        gate.entered_changed.notify_all();
        let mut released = gate.released.lock().unwrap_or_else(|p| p.into_inner());
        while !*released {
            released = gate
                .released_changed
                .wait(released)
                .unwrap_or_else(|p| p.into_inner());
        }
    }
}

/// Gibt die Aufbewahrung frei, auch wenn der Test vorher scheitert.
struct ReleaseOnDrop;

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        gate().release();
    }
}

/// Wartet, bis die Aufbewahrung am Haltepunkt steht (Ereignis; die Grenze ist nur Hängeschutz).
async fn retention_held() -> String {
    tokio::task::spawn_blocking(|| {
        let gate = gate();
        let mut entered = gate.entered.lock().unwrap_or_else(|p| p.into_inner());
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while entered.is_none() {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!left.is_zero(), "retention never reached the deletion");
            entered = gate
                .entered_changed
                .wait_timeout(entered, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        entered.clone().unwrap()
    })
    .await
    .unwrap()
}

/// Abgeschlossener Crawl im Crawl-Ordner, Ende im Jahr 2000: fällt unter „1 Woche“.
fn expired_crawl(crawls: &std::path::Path, seed: u64, ended_at: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(crawls).unwrap();
    let id = SeededIds::new(seed)
        .new_crawl_id(common::created_at())
        .to_string();
    let path = common::make_crawl(crawls, &format!("{id}.ospider"));
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.set_meta("crawl_id", &id).unwrap();
    file.set_meta("state", CrawlState::Completed.as_str())
        .unwrap();
    file.set_meta("heartbeat_at", &common::created_at().to_db())
        .unwrap();
    file.set_meta("ended_at", ended_at).unwrap();
    path
}

// spec: OUT-229
// spec: CRAWL-126 [M2]
#[tokio::test]
async fn dropping_the_app_finishes_the_running_retention_round_before_the_directory_is_removed() {
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(HoldFirstDeletion))
        .expect("eigener Prozess: erster globaler Subscriber");
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = SettingsStore::open(dir.path());
    settings
        .apply_patch(&json!({"retention":{"policy":"one_week"}}))
        .unwrap();
    drop(settings);
    let crawls = dir.path().join("crawls");
    let first = expired_crawl(&crawls, 311, "2000-01-01T00:00:00.000Z");
    let second = expired_crawl(&crawls, 312, "2000-01-02T00:00:00.000Z");
    let _release = ReleaseOnDrop;
    // Start: die Aufbewahrung beim Start läuft beim ersten `await` an und hält nach der ersten Löschung an.
    let app = common::new_app(dir.path());
    let thread = retention_held().await;
    assert_eq!(thread, "os-app-maintenance");
    assert_eq!(
        [first.exists(), second.exists()]
            .iter()
            .filter(|e| **e)
            .count(),
        1,
        "the round stands between its two deletions"
    );

    // Freigabe aus einem anderen Thread, erst nachdem der Abbau begonnen hat. Die Pause ist kein Synchronisationsmittel
    // für das Ergebnis: Mit Korrektur wartet der Abbau auf die Freigabe, ohne Korrektur kehrt er sofort zurück.
    let releaser = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(300));
        gate().release();
    });
    // Der Besitzer baut die App ab (letzte Referenz) und entfernt danach ihr Verzeichnis, wie jeder Test am Ende.
    drop(app);
    let round_over_when_drop_returned = !first.exists() && !second.exists();
    let deletions_when_drop_returned = gate().deletions.load(Ordering::SeqCst);
    drop(dir);
    releaser.join().unwrap();
    assert!(
        round_over_when_drop_returned,
        "App drop returned while the retention round still ran on {thread} \
         ({deletions_when_drop_returned} of 2 deletions); the directory was removed under it"
    );
    assert_eq!(deletions_when_drop_returned, 2);
}

// spec: OUT-229
#[tokio::test]
async fn shutdown_finishes_the_running_maintenance_job_and_discards_queued_ones() {
    let queue = WriteQueue::new();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let ran = Arc::new(AtomicUsize::new(0));
    let r1 = Arc::clone(&ran);
    let mut running = Box::pin(queue.submit_maintenance(move || {
        started_tx.send(()).unwrap();
        go_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        r1.fetch_add(1, Ordering::SeqCst);
        Ok(1)
    }));
    // einreihen, ohne zu warten
    let _ = futures_util::poll!(running.as_mut());
    let r2 = Arc::clone(&ran);
    let mut queued = Box::pin(queue.submit_maintenance(move || {
        r2.fetch_add(10, Ordering::SeqCst);
        Ok(2)
    }));
    let _ = futures_util::poll!(queued.as_mut());
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        go_tx.send(()).unwrap();
    });
    queue.shutdown();
    // Beigetreten: der laufende Auftrag ist fertig, der eingereihte lief nicht.
    assert_eq!(ran.load(Ordering::SeqCst), 1);
    releaser.join().unwrap();
    assert_eq!(running.await.unwrap(), 1);
    let e = queued.await.unwrap_err();
    assert_eq!(e.code, ErrorCode::Internal);
    let e = queue.submit_maintenance(|| Ok(3)).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::Internal);
}

// spec: OUT-229
#[tokio::test]
async fn dropping_the_queue_from_its_own_maintenance_job_does_not_deadlock() {
    let queue = Arc::new(WriteQueue::new());
    let inner = Arc::clone(&queue);
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    {
        let mut job = Box::pin(queue.submit_maintenance(move || {
            go_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            // letzte Referenz: Drop und Abbau laufen auf dem Thread der Wartungsspur
            drop(inner);
            done_tx.send(()).unwrap();
            Ok(())
        }));
        // einreihen; das Ergebnis wird nicht gebraucht
        let _ = futures_util::poll!(job.as_mut());
    }
    drop(queue);
    go_tx.send(()).unwrap();
    tokio::task::spawn_blocking(move || done_rx.recv_timeout(Duration::from_secs(10)))
        .await
        .unwrap()
        .expect("no deadlock when the last reference drops on the maintenance thread");
}

// spec: OUT-229
#[tokio::test]
async fn the_default_policy_opens_no_management_file_in_the_background() {
    let dir = tempfile::tempdir().unwrap();
    let app = common::new_app(dir.path());
    // Wartet auch auf die Runde beim Start: Runden sind untereinander serialisiert (Verwaltungssperre).
    assert_eq!(
        os_app::commands::crawls::run_retention(&app).await.unwrap(),
        0
    );
    let crawls = app.config().paths.crawls.clone();
    for name in [".index.sqlite", ".registry.sqlite"] {
        assert!(
            !crawls.join(name).exists(),
            "{name} created although retention is off"
        );
    }
}

// spec: OUT-229
// spec: CRAWL-124
#[tokio::test]
async fn the_default_policy_still_records_a_storage_location_other_than_the_root() {
    // Abweichender Speicherort: Die Runde vermerkt ihn im Register wie vor der Korrektur, damit die Crawl-Verwaltung
    // seine Crawls auch nach einem weiteren Wechsel findet (CRAWL-124). Der Stammordner selbst wird immer gelistet.
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let (settings, _) = SettingsStore::open(dir.path());
    settings
        .apply_patch(&json!({"storage":{"crawlsDir":elsewhere.path()}}))
        .unwrap();
    drop(settings);
    let app = common::new_app(dir.path());
    assert_eq!(
        os_app::commands::crawls::run_retention(&app).await.unwrap(),
        0
    );
    assert!(app.config().paths.crawls.join(".registry.sqlite").exists());
}

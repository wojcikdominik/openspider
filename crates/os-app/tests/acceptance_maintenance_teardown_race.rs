//! Unabhängige Abnahme (fix/maintenance-teardown, Frage A1): Hält eine Aufbewahrungsrunde (OUT-229) die `App` über
//! einen blockierenden Aufruf? Dann ist der Abbau durch den Besitzer nicht der letzte, die Schreibwarteschlange wird
//! später in einem Tokio-Worker abgebaut und ein Wartungsauftrag kann noch nach dem Abbau durch den Besitzer in dessen
//! Verzeichnis schreiben – das ursprüngliche Fehlerbild.
//!
//! Haltepunkt ohne Wanduhr: eine Uhr, deren erster Aufruf aus `retention_round` (Tokio-Worker) wartet, bis der Test
//! freigibt. Die Runde liest die Uhr in ihrem synchronen Abschnitt, in dem sie die `App` aus der schwachen Referenz
//! hochstuft. Eigene Testdatei, damit kein anderer Test die Uhr teilt.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "runtime_common.rs"]
mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use os_app::settings_store::SettingsStore;
use os_app::{App, AppConfig};
use os_model::{Clock, ManualClock, SeededIds, Timestamp};
use serde_json::json;

#[derive(Default)]
struct Held {
    entered: Option<String>,
    released: bool,
}

struct HoldClock {
    inner: ManualClock,
    armed: AtomicBool,
    held: Mutex<Held>,
    changed: Condvar,
}

impl HoldClock {
    fn release(&self) {
        self.held.lock().unwrap_or_else(|p| p.into_inner()).released = true;
        self.changed.notify_all();
    }

    fn wait_entered(&self) -> String {
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        // Hängeschutz; das Ereignis kommt aus dem Produktcode.
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while held.entered.is_none() {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(
                !left.is_zero(),
                "Aufbewahrungsrunde erreichte die Uhr nicht"
            );
            held = self
                .changed
                .wait_timeout(held, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        held.entered.clone().unwrap()
    }
}

impl Clock for HoldClock {
    fn now(&self) -> Timestamp {
        if self.armed.load(Ordering::SeqCst) {
            let trace = std::backtrace::Backtrace::force_capture().to_string();
            if trace.contains("retention_round") && self.armed.swap(false, Ordering::SeqCst) {
                let frames: Vec<&str> = trace
                    .lines()
                    .filter(|l| l.contains("os_app::"))
                    .take(6)
                    .map(str::trim)
                    .collect();
                let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
                held.entered = Some(format!(
                    "{} | {}",
                    std::thread::current().name().unwrap_or("unnamed"),
                    frames.join(" <- ")
                ));
                self.changed.notify_all();
                while !held.released {
                    held = self.changed.wait(held).unwrap_or_else(|p| p.into_inner());
                }
            }
        }
        self.inner.now()
    }

    fn monotonic(&self) -> Duration {
        self.inner.monotonic()
    }
}

struct ReleaseOnDrop(Arc<HoldClock>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

// spec: OUT-229
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_drop_is_the_last_reference_even_while_a_round_reads_its_settings() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = SettingsStore::open(dir.path());
    settings
        .apply_patch(&json!({"retention":{"policy":"one_week"}}))
        .unwrap();
    drop(settings);
    let clock = Arc::new(HoldClock {
        inner: ManualClock::new(common::created_at()),
        armed: AtomicBool::new(true),
        held: Mutex::new(Held::default()),
        changed: Condvar::new(),
    });
    let _release = ReleaseOnDrop(Arc::clone(&clock));
    let config = AppConfig::new(dir.path())
        .with_ids(Arc::new(SeededIds::new(7)))
        .with_clock(Arc::clone(&clock) as Arc<dyn Clock>);
    let app = App::new(config).expect("App::new");

    // Die Startrunde steht in ihrem synchronen Abschnitt (Uhr gelesen, Auftrag noch nicht eingereiht).
    let c = Arc::clone(&clock);
    let at = tokio::task::spawn_blocking(move || c.wait_entered())
        .await
        .unwrap();

    // Der Besitzer baut ab, wie ein Test vor dem Löschen seines Verzeichnisses.
    let weak = app.downgrade();
    drop(app);
    let survived = weak.upgrade().is_some();
    clock.release();
    assert!(
        !survived,
        "Nach dem Abbau durch den Besitzer lebt die App weiter: die Aufbewahrungsrunde hält sie über einen \
         blockierenden Aufruf ({at}); Abbau der Schreibwarteschlange und der eingereihte Wartungsauftrag folgen erst \
         danach, nachdem der Besitzer sein Verzeichnis schon entfernen kann"
    );
}

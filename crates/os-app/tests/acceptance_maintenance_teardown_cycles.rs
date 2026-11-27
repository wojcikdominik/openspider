//! Unabhängige Abnahme (fix/maintenance-teardown, Teil B i): Fällt die letzte `App`-Referenz während einer laufenden
//! Aufbewahrungsrunde (OUT-229), schreibt innerhalb von 1 s danach keine Wartungsspur mehr in den Crawl-Ordner (Zusicherung
//! präzisiert 27.11.2026: höchstens ein synchroner Tick einer Takt-Schleife).
//!
//! Eigene Testdatei (eigener Prozess): Ein globaler `tracing`-Subscriber hält die Runde nach der ersten Löschung an
//! (Ereignis mit Feld `age_days`) und zählt die Löschungen; er bemerkt außerdem, wenn der Abbau den Wartungsthread
//! losgelöst zurücklässt (Warnung mit Feld `wait_ms`). Keine Wanduhr: Die Runde wartet auf den Test, nicht auf Zeit.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "runtime_common.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use os_app::settings_store::SettingsStore;
use os_model::{CrawlState, IdSource, SeededIds};
use os_store::{CrawlFile, OpenMode};
use serde_json::json;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

#[derive(Default)]
struct Cycle {
    /// Runde steht am Haltepunkt (nach der ersten Löschung).
    entered: bool,
    released: bool,
    deletions: usize,
    /// Abbau ließ den Wartungsthread losgelöst weiterlaufen.
    detached: bool,
    /// Der Test hat mit dem Abbau begonnen.
    dropping: bool,
}

#[derive(Default)]
struct Probe {
    state: Mutex<Cycle>,
    changed: Condvar,
}

impl Probe {
    fn with<R>(&self, f: impl FnOnce(&mut Cycle) -> R) -> R {
        let r = f(&mut self.state.lock().unwrap_or_else(|p| p.into_inner()));
        self.changed.notify_all();
        r
    }

    fn wait(&self, what: &str, mut cond: impl FnMut(&Cycle) -> bool) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        // Hängeschutz, kein Taktgeber: das Ereignis kommt aus dem Produktcode.
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !cond(&s) {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!left.is_zero(), "nicht erreicht: {what}");
            s = self
                .changed
                .wait_timeout(s, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }
}

fn probe() -> &'static Probe {
    static PROBE: OnceLock<Probe> = OnceLock::new();
    PROBE.get_or_init(Probe::default)
}

struct Hold;

impl<S: tracing::Subscriber> Layer<S> for Hold {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        if meta.target().starts_with("os_app::write_queue")
            && meta.fields().field("wait_ms").is_some()
        {
            probe().with(|s| s.detached = true);
            return;
        }
        if !meta.target().starts_with("os_app::commands::crawls")
            || meta.fields().field("age_days").is_none()
        {
            return;
        }
        let first = probe().with(|s| {
            s.deletions += 1;
            let first = !s.entered;
            s.entered = true;
            first
        });
        if first {
            probe().wait("Freigabe der Runde", |s| s.released);
        }
    }
}

/// Gibt die Runde frei, auch wenn der Test vorher scheitert.
struct ReleaseOnDrop;

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        probe().with(|s| s.released = true);
    }
}

fn expired_crawl(crawls: &Path, seed: u64) -> PathBuf {
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
    file.set_meta("ended_at", "2000-01-01T00:00:00.000Z")
        .unwrap();
    path
}

const CYCLES: usize = 40;

// spec: OUT-229
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_last_app_during_a_running_round_leaves_no_writer_in_the_directory() {
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(Hold))
        .expect("eigener Prozess: erster globaler Subscriber");
    for cycle in 0..CYCLES {
        *probe().state.lock().unwrap_or_else(|p| p.into_inner()) = Cycle::default();
        let _release = ReleaseOnDrop;
        let dir = tempfile::tempdir().unwrap();
        let (settings, _) = SettingsStore::open(dir.path());
        settings
            .apply_patch(&json!({"retention":{"policy":"one_week"}}))
            .unwrap();
        drop(settings);
        let crawls = dir.path().join("crawls");
        let expired = [
            expired_crawl(&crawls, 900 + 2 * cycle as u64),
            expired_crawl(&crawls, 901 + 2 * cycle as u64),
        ];

        // Start: die Startrunde läuft und steht nach der ersten Löschung am Haltepunkt.
        let app = common::new_app(dir.path());
        tokio::task::spawn_blocking(|| probe().wait("Haltepunkt der Startrunde", |s| s.entered))
            .await
            .unwrap();

        // Abbau durch den Besitzer, während die Runde steht; ein Helfer gibt die Runde frei, sobald der Abbau beginnt.
        let releaser = std::thread::spawn(|| {
            probe().wait("Beginn des Abbaus", |s| s.dropping);
            probe().with(|s| s.released = true);
        });
        let weak = app.downgrade();
        probe().with(|s| s.dropping = true);
        drop(app);
        releaser.join().unwrap();

        // Zusicherung (präzisiert 27.11.2026): Innerhalb von 1 s nach dem Abbau durch den Besitzer ist die App weg, die
        // laufende Runde fertig und keine Verbindung mehr offen (SQLite entfernt -wal/-shm erst mit der letzten
        // Verbindung). Trifft der Abbau einen Tick einer Takt-Schleife, läuft der eigentliche Abbau wenige
        // Mikrosekunden später auf deren Thread. Abbruch, sobald der Zustand erreicht ist: keine feste Wartezeit.
        let companions: Vec<PathBuf> = [".index.sqlite", ".registry.sqlite"]
            .iter()
            .flat_map(|name| ["-wal", "-shm"].map(|suffix| crawls.join(format!("{name}{suffix}"))))
            .collect();
        let settled = || {
            weak.upgrade().is_none()
                && probe().with(|s| s.deletions) == 2
                && expired.iter().all(|p| !p.exists())
                && companions.iter().all(|c| !c.exists())
        };
        let start = std::time::Instant::now();
        while !settled() && start.elapsed() < Duration::from_secs(1) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let (deletions, detached) = probe().with(|s| (s.deletions, s.detached));
        assert!(
            !detached,
            "Zyklus {cycle}: Abbau ließ den Wartungsthread losgelöst zurück"
        );
        assert!(
            weak.upgrade().is_none(),
            "Zyklus {cycle}: App 1 s nach dem Abbau durch den Besitzer noch nicht abgebaut"
        );
        assert_eq!(
            deletions, 2,
            "Zyklus {cycle}: laufende Runde 1 s nach dem Abbau nicht beendet"
        );
        for p in &expired {
            assert!(
                !p.exists(),
                "Zyklus {cycle}: {} nicht gelöscht",
                p.display()
            );
        }
        for companion in &companions {
            assert!(
                !companion.exists(),
                "Zyklus {cycle}: 1 s nach dem Abbau noch geöffnet: {}",
                companion.display()
            );
        }
        // Danach lässt sich das Verzeichnis entfernen und bleibt weg.
        let root = dir.path().to_path_buf();
        dir.close().expect("Verzeichnis löschbar");
        assert!(!root.exists(), "Zyklus {cycle}: Verzeichnis neu angelegt");
    }
}

//! Unabhängige Abnahme (fix/maintenance-teardown, zweite Durchsicht, Frage 2): Eine Aufbewahrungsrunde nach einer
//! Einstellungsänderung (OUT-229) löscht nach der Richtlinie, die gilt, wenn sie läuft – nicht nach einem Stand, den der
//! Nutzer inzwischen zurückgenommen hat.
//!
//! Eigene Testdatei (eigener Prozess): Ein globaler `tracing`-Subscriber hält die Startrunde nach ihrer ersten Löschung
//! an (Ereignis mit Feld `age_days`); solange belegt sie Verwaltungs-Gate und Register. In dieser Zeit ändert der Test die
//! Richtlinie zweimal (verschärft, dann zurückgenommen).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "runtime_common.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use os_app::settings_store::SettingsStore;
use os_model::{CrawlState, IdSource, SeededIds, Timestamp};
use os_store::{CrawlFile, OpenMode};
use serde_json::json;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

#[derive(Default)]
struct Hold {
    entered: bool,
    released: bool,
    deleted: Vec<String>,
}

fn hold() -> &'static (Mutex<Hold>, Condvar) {
    static HOLD: OnceLock<(Mutex<Hold>, Condvar)> = OnceLock::new();
    HOLD.get_or_init(Default::default)
}

fn wait(what: &str, mut cond: impl FnMut(&Hold) -> bool) {
    let (m, c) = hold();
    let mut s = m.lock().unwrap_or_else(|p| p.into_inner());
    // Hängeschutz; das Ereignis kommt aus dem Produktcode.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while !cond(&s) {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(!left.is_zero(), "nicht erreicht: {what}");
        s = c.wait_timeout(s, left).unwrap_or_else(|p| p.into_inner()).0;
    }
}

fn release() {
    let (m, c) = hold();
    m.lock().unwrap_or_else(|p| p.into_inner()).released = true;
    c.notify_all();
}

#[derive(Default)]
struct CrawlId(String);

impl tracing::field::Visit for CrawlId {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "crawl_id" {
            self.0 = format!("{value:?}");
        }
    }
}

struct HoldFirst;

impl<S: tracing::Subscriber> Layer<S> for HoldFirst {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        if !meta.target().starts_with("os_app::commands::crawls")
            || meta.fields().field("age_days").is_none()
        {
            return;
        }
        let mut id = CrawlId::default();
        event.record(&mut id);
        let first = {
            let (m, c) = hold();
            let mut s = m.lock().unwrap_or_else(|p| p.into_inner());
            s.deleted.push(id.0);
            let first = !s.entered;
            s.entered = true;
            c.notify_all();
            first
        };
        if first {
            wait("Freigabe der Startrunde", |s| s.released);
        }
    }
}

struct ReleaseOnDrop;

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        release();
    }
}

fn crawl_ended(crawls: &Path, seed: u64, days: i64) -> (String, PathBuf) {
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
    let ended = Timestamp::from_unix_millis(common::created_at().unix_millis() - days * 86_400_000);
    file.set_meta("ended_at", &ended.to_db()).unwrap();
    (id, path)
}

// spec: OUT-229
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_round_after_a_settings_change_deletes_by_the_policy_in_force_when_it_runs() {
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(HoldFirst))
        .expect("eigener Prozess: erster globaler Subscriber");
    let _release = ReleaseOnDrop;
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = SettingsStore::open(dir.path());
    settings
        .apply_patch(&json!({"retention":{"policy":"one_year"}}))
        .unwrap();
    drop(settings);
    let crawls = dir.path().join("crawls");
    // Fällig nach „1 Jahr“: die Startrunde löscht ihn und steht danach am Haltepunkt.
    let (_, old) = crawl_ended(&crawls, 801, 400);
    // Fällig nur nach „1 Woche“.
    let (young_id, young) = crawl_ended(&crawls, 802, 8);

    let app = common::new_app(dir.path());
    tokio::task::spawn_blocking(|| wait("Haltepunkt der Startrunde", |s| s.entered))
        .await
        .unwrap();
    assert!(!old.exists());

    // Während die Startrunde Gate und Register belegt: verschärfen, dann zurücknehmen.
    for policy in ["one_week", "one_year"] {
        app.dispatch(
            common::ctx(),
            "settings.set",
            json!({"patch":{"retention":{"policy":policy}}}),
        )
        .await
        .unwrap();
    }
    assert_eq!(
        app.settings_snapshot().retention.policy,
        os_model::RetentionPolicy::OneYear
    );
    release();

    // Hinter den eingereihten Runden: eine weitere Runde nach der geltenden Richtlinie.
    os_app::commands::crawls::run_retention(&app).await.unwrap();
    // Gnadenfrist nur, damit eine verspätet startende Runde bemerkt wird; die richtige Umsetzung bleibt grün.
    let deleted_later = common::eventually(Duration::from_secs(3), || !young.exists()).await;
    let deleted = hold()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .deleted
        .clone();
    assert!(
        !deleted_later && young.exists(),
        "Crawl {young_id} (8 Tage) wurde nach der zurückgenommenen Richtlinie „1 Woche“ gelöscht, obwohl „1 Jahr“ gilt; \
         gelöschte Kennungen: {deleted:?}"
    );
}

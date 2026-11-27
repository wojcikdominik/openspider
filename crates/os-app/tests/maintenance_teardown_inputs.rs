//! OUT-229: Eine Aufbewahrungsrunde, die auf das Verwaltungs-Gate warten muss, liest „jetzt“, die geöffneten Crawls und
//! die Richtlinie erst, wenn sie es bekommt – nicht beim Auslösen.
//!
//! Eigene Testdatei (eigener Prozess), weil der Test einen globalen `tracing`-Subscriber setzt: Er hält die Startrunde in
//! ihrer ersten Löschmeldung an; solange belegt sie Gate und Register. In dieser Zeit rückt der Test die Uhr vor, öffnet
//! einen Crawl und ändert die Richtlinie (das reiht eine Runde ein, die auf das Gate wartet).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "runtime_common.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use os_app::runtime::sessions::SessionInfo;
use os_app::{pool::CrawlKey, settings_store::SettingsStore, App, AppConfig, WindowId};
use os_model::{CrawlConfig, CrawlState, IdSource, ManualClock, SeededIds, Timestamp};
use os_store::{CrawlFile, OpenMode};
use serde_json::json;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

#[derive(Default)]
struct Hold {
    entered: bool,
    released: bool,
}

fn hold() -> &'static (Mutex<Hold>, Condvar) {
    static HOLD: OnceLock<(Mutex<Hold>, Condvar)> = OnceLock::new();
    HOLD.get_or_init(Default::default)
}

fn wait_for(what: &str, cond: impl Fn(&Hold) -> bool) {
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

struct HoldFirstDeletion;

impl<S: tracing::Subscriber> Layer<S> for HoldFirstDeletion {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        if !meta.target().starts_with("os_app::commands::crawls")
            || meta.fields().field("age_days").is_none()
        {
            return;
        }
        let first = {
            let (m, c) = hold();
            let mut s = m.lock().unwrap_or_else(|p| p.into_inner());
            let first = !s.entered;
            s.entered = true;
            c.notify_all();
            first
        };
        if first {
            wait_for("Freigabe der Startrunde", |s| s.released);
        }
    }
}

struct ReleaseOnDrop;

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        release();
    }
}

/// Abgeschlossener Crawl im Crawl-Ordner, beendet `days` Tage vor der Startzeit der Uhr.
fn ended(crawls: &Path, seed: u64, days: i64) -> (String, PathBuf) {
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
    let at = Timestamp::from_unix_millis(common::created_at().unix_millis() - days * 86_400_000);
    file.set_meta("ended_at", &at.to_db()).unwrap();
    (id, path)
}

// spec: OUT-229
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_waiting_round_takes_now_and_the_open_crawls_from_when_it_gets_the_gate() {
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(HoldFirstDeletion))
        .expect("eigener Prozess: erster globaler Subscriber");
    let _release = ReleaseOnDrop;
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = SettingsStore::open(dir.path());
    settings
        .apply_patch(&json!({"retention":{"policy":"one_month"}}))
        .unwrap();
    drop(settings);
    let crawls = dir.path().join("crawls");
    // Fällig beim Start („1 Monat“): die Startrunde löscht ihn und steht danach am Haltepunkt.
    let (_, old) = ended(&crawls, 901, 400);
    // Beim Start 6 Tage alt; zwei Tage später 8 Tage – fällig nach „1 Woche“.
    let (due_id, due) = ended(&crawls, 902, 6);
    let (open_id, open) = ended(&crawls, 903, 6);

    let clock = Arc::new(ManualClock::new(common::created_at()));
    let config = AppConfig::new(dir.path())
        .with_ids(Arc::new(SeededIds::new(7)))
        .with_clock(clock.clone());
    let app = App::new(config).expect("App::new");
    tokio::task::spawn_blocking(|| wait_for("Haltepunkt der Startrunde", |s| s.entered))
        .await
        .unwrap();
    assert!(!old.exists());

    // Während die Startrunde das Gate hält: Richtlinie „1 Woche“ (reiht eine Runde ein), dann zwei Tage später und
    // ein geöffneter Crawl. Beim Auslösen wäre noch nichts fällig gewesen.
    app.dispatch(
        common::ctx(),
        "settings.set",
        json!({"patch":{"retention":{"policy":"one_week"}}}),
    )
    .await
    .unwrap();
    clock.advance(Duration::from_secs(2 * 86_400));
    app.sessions().set_current(
        WindowId("main".into()),
        SessionInfo {
            key: CrawlKey(open_id.clone()),
            path: open.clone(),
            config: Arc::new(CrawlConfig::default()),
            state: CrawlState::Completed,
            writable: true,
            engine: None,
        },
    );
    release();

    // Hinter der eingereihten Runde (strikte Reihenfolge am Gate); sie selbst findet dann nichts mehr.
    assert_eq!(
        os_app::commands::crawls::run_retention(&app).await.unwrap(),
        0
    );
    assert!(
        !due.exists(),
        "Crawl {due_id} ist nach der Uhr beim Erhalt des Gates fällig (8 Tage) und blieb"
    );
    assert!(
        open.exists(),
        "geöffneter Crawl {open_id} wurde gelöscht: die Runde las die geöffneten Crawls beim Auslösen"
    );
}

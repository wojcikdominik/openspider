//! M4-K11, Kriterium (e): `RunOwner` – Eintrag, Heartbeat alle 2 s über tokio-Zeit, Ende, fremder Besitzer, passiver
//! Wert bei eigenem laufendem Crawl, mehrere KI-Läufe desselben Prozesses und `Drop` ohne `finish` (08 AI-020 [M4],
//! Unterpunkt „Aktiver Besitzer außerhalb von `Running`“, „Wer schreibt“; 01 CRAWL-125, Nachtrag K4-22).
//! Keine KI-Ausführung: der KI-Lauf ist nur der Zeitraum zwischen `start` und `finish`. Zeit: pausierte tokio-Zeit
//! für den Takt, `ManualClock` für die geschriebenen Zeitpunkte; beide rücken im Test gemeinsam vor.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use os_ai::run_owner::{RunOwner, HEARTBEAT_INTERVAL};
use os_model::time::{IdSource, SeededIds};
use os_model::{Clock, CrawlConfig, CrawlMode, ManualClock, Timestamp};
use os_store::owner::{self, Claim};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile, StoreError};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("os-ai-k11-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Fester Startzeitpunkt der Zustandsfälle (`tests/fixtures/crawl-files/active-owner/cases.json`, Feld `now`).
fn t0() -> Timestamp {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/crawl-files/active-owner/cases.json");
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Timestamp::from_db(cases["now"].as_str().unwrap()).unwrap()
}

fn plus(t: Timestamp, ms: i64) -> String {
    Timestamp::from_unix_millis(t.unix_millis() + ms).to_db()
}

/// Crawl-Datei mit gespeichertem Zustand `state`; Besitzerangaben wie nach einem geordneten Ende in 1.0.
fn crawl(dir: &Path, state: &str) -> PathBuf {
    let path = dir.join("crawl.ospider");
    let init = CrawlInit {
        crawl_id: SeededIds::new(11).new_crawl_id(t0()),
        mode: CrawlMode::Spider,
        start_url: Some("https://site.test/".into()),
        list_source: None,
        project: "K11".into(),
        name: "run-owner".into(),
        config: CrawlConfig::default(),
        created_at: t0(),
    };
    let file = CrawlFile::create(&path, &init, StorageProfile::default()).unwrap();
    file.set_meta("state", state).unwrap();
    file.set_meta("owner_kind", "cli").unwrap();
    file.set_meta("owner_pid", "").unwrap();
    file.set_meta("heartbeat_at", &plus(t0(), -60_000)).unwrap();
    path
}

fn meta(path: &Path, key: &str) -> String {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .get_meta(key)
        .unwrap()
        .unwrap_or_default()
}

/// Beide Uhren um eine Sekunde vor.
async fn tick(clock: &ManualClock) {
    clock.advance(Duration::from_secs(1));
    tokio::time::advance(Duration::from_secs(1)).await;
}

// spec: AI-020 [M4]
// spec: CRAWL-125
#[test]
fn the_interval_is_the_engine_interval() {
    assert_eq!(HEARTBEAT_INTERVAL, Duration::from_secs(2));
    assert_eq!(HEARTBEAT_INTERVAL, os_store::owner::HEARTBEAT_INTERVAL);
}

/// Abgeschlossener Crawl, 7 s pausierte Zeit: mindestens vier Werte `heartbeat_at` (Start, 2, 4, 6 s), `state`
/// durchgehend `Completed`; nach `finish` ist `owner_pid` leer und `heartbeat_at` der Endzeitpunkt. Während des Laufs
/// wird ein zweiter Prozess (simulierte fremde PID) mit Art und PID abgewiesen.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[tokio::test(start_paused = true)]
async fn a_run_on_a_completed_crawl_beats_every_two_seconds_and_releases_at_the_end() {
    let dir = TempDir::new("beat");
    let path = crawl(&dir.0, "Completed");
    let clock = Arc::new(ManualClock::new(t0()));
    let owner = RunOwner::start(&path, "app", clock.clone()).unwrap();
    assert!(!owner.is_passive());
    let mut beats = owner.heartbeats().unwrap();
    assert_eq!(meta(&path, "owner_kind"), "app");
    assert_eq!(meta(&path, "owner_pid"), std::process::id().to_string());
    let mut seen = vec![meta(&path, "heartbeat_at")];
    for second in 1..=7 {
        tick(&clock).await;
        if second % 2 == 0 {
            beats.changed().await.unwrap();
            assert_eq!(*beats.borrow_and_update(), second / 2);
        }
        assert_eq!(meta(&path, "state"), "Completed", "nach {second} s");
        let value = meta(&path, "heartbeat_at");
        if seen.last() != Some(&value) {
            seen.push(value);
        }
    }
    assert_eq!(
        seen,
        [0, 2_000, 4_000, 6_000].map(|ms| plus(t0(), ms)),
        "Start, 2 s, 4 s, 6 s"
    );

    // Ein zweiter Prozess während des aktiven Besitzers: Fehler mit Art und PID.
    let other = std::process::id().wrapping_add(1);
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let err = owner::claim(file.conn(), "cli", other, clock.now()).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains(&std::process::id().to_string()) && text.contains("app"),
        "{text}"
    );
    drop(file);

    owner.finish().unwrap();
    assert_eq!(meta(&path, "owner_pid"), "");
    assert_eq!(meta(&path, "owner_kind"), "app");
    assert_eq!(
        meta(&path, "heartbeat_at"),
        plus(t0(), 7_000),
        "Endzeitpunkt"
    );
    assert_eq!(meta(&path, "state"), "Completed");
    // Nach dem Ende kein Heartbeat mehr.
    for _ in 0..4 {
        tick(&clock).await;
    }
    tokio::task::yield_now().await;
    assert_eq!(meta(&path, "heartbeat_at"), plus(t0(), 7_000));
}

/// `start` aus diesem Prozess, während ein anderer Prozess (simulierte fremde PID, aktueller Heartbeat) Besitzer ist:
/// Fehler mit Art und PID, nichts geschrieben; 11 s später gelingt der Eintrag.
// spec: AI-020 [M4]
// spec: AGENT-020
#[tokio::test(start_paused = true)]
async fn start_is_rejected_while_another_process_is_the_active_owner() {
    let dir = TempDir::new("foreign");
    let path = crawl(&dir.0, "Paused");
    let foreign = std::process::id().wrapping_add(7).to_string();
    {
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.set_meta("owner_kind", "mcp-stdio").unwrap();
        file.set_meta("owner_pid", &foreign).unwrap();
        file.set_meta("heartbeat_at", &t0().to_db()).unwrap();
    }
    let clock = Arc::new(ManualClock::new(t0()));
    let err = RunOwner::start(&path, "app", clock.clone()).unwrap_err();
    let StoreError::Occupied { kind, pid } = &err else {
        panic!("{err:?}");
    };
    assert_eq!(
        (kind.as_str(), pid.as_str()),
        ("mcp-stdio", foreign.as_str())
    );
    assert_eq!(meta(&path, "owner_pid"), foreign);
    assert_eq!(meta(&path, "heartbeat_at"), t0().to_db());

    clock.advance(Duration::from_secs(11));
    let owner = RunOwner::start(&path, "app", clock.clone()).unwrap();
    assert_eq!(meta(&path, "owner_pid"), std::process::id().to_string());
    assert_eq!(meta(&path, "state"), "Paused");
    owner.finish().unwrap();
    assert_eq!(meta(&path, "owner_pid"), "");
}

/// Eigener laufender Crawl (KI während des eigenen Crawls): passiver Wert, schreibt nichts und leert `owner_pid` nicht.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[tokio::test(start_paused = true)]
async fn a_passive_owner_during_the_own_running_crawl_writes_nothing() {
    let dir = TempDir::new("passive");
    let path = crawl(&dir.0, "Running");
    let me = std::process::id().to_string();
    {
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.set_meta("owner_pid", &me).unwrap();
        file.set_meta("heartbeat_at", &t0().to_db()).unwrap();
    }
    let clock = Arc::new(ManualClock::new(t0()));
    let owner = RunOwner::start(&path, "app", clock.clone()).unwrap();
    assert!(owner.is_passive());
    for _ in 0..7 {
        clock.advance(Duration::from_millis(500));
        tokio::time::advance(Duration::from_secs(1)).await;
    }
    tokio::task::yield_now().await;
    owner.finish().unwrap();
    assert_eq!(meta(&path, "owner_pid"), me, "leert owner_pid nicht");
    assert_eq!(
        meta(&path, "owner_kind"),
        "cli",
        "schreibt keine Besitzerangaben"
    );
    assert_eq!(
        meta(&path, "heartbeat_at"),
        t0().to_db(),
        "schreibt keinen Heartbeat"
    );
    assert_eq!(meta(&path, "state"), "Running");
}

/// Zwei KI-Läufe desselben Prozesses auf derselben Datei: ein Eintrag; erst der letzte `finish` gibt frei.
// spec: AI-020 [M4]
#[tokio::test(start_paused = true)]
async fn two_runs_of_the_same_process_share_one_entry_until_the_last_finishes() {
    let dir = TempDir::new("shared");
    let path = crawl(&dir.0, "Completed");
    let clock = Arc::new(ManualClock::new(t0()));
    let first = RunOwner::start(&path, "app", clock.clone()).unwrap();
    let mut beats = first.heartbeats().unwrap();
    tick(&clock).await;
    let second = RunOwner::start(&path, "app", clock.clone()).unwrap();
    assert!(!second.is_passive());
    assert_eq!(
        meta(&path, "heartbeat_at"),
        t0().to_db(),
        "kein zweiter Eintrag"
    );
    tick(&clock).await;
    beats.changed().await.unwrap();
    first.finish().unwrap();
    assert_eq!(meta(&path, "owner_pid"), std::process::id().to_string());
    assert_eq!(meta(&path, "heartbeat_at"), plus(t0(), 2_000));
    tick(&clock).await;
    tick(&clock).await;
    beats.changed().await.unwrap();
    assert_eq!(
        meta(&path, "heartbeat_at"),
        plus(t0(), 4_000),
        "Takt läuft weiter"
    );
    second.finish().unwrap();
    assert_eq!(meta(&path, "owner_pid"), "");
    assert_eq!(meta(&path, "heartbeat_at"), plus(t0(), 4_000));
}

/// `Drop` ohne `finish` hält nur an (gilt wie ein Absturz): `owner_pid` bleibt, kein weiterer Heartbeat; mehr als 10 s
/// später gibt es keinen aktiven Besitzer mehr und der nächste Lauf trägt sich ein.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[tokio::test(start_paused = true)]
async fn dropping_without_finish_behaves_like_a_crash() {
    let dir = TempDir::new("drop");
    let path = crawl(&dir.0, "Completed");
    let clock = Arc::new(ManualClock::new(t0()));
    let owner = RunOwner::start(&path, "cli", clock.clone()).unwrap();
    drop(owner);
    for _ in 0..4 {
        tick(&clock).await;
    }
    tokio::task::yield_now().await;
    assert_eq!(meta(&path, "owner_pid"), std::process::id().to_string());
    assert_eq!(meta(&path, "heartbeat_at"), t0().to_db());
    let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    assert!(file.occupancy(clock.as_ref()).unwrap().active_owner);
    clock.advance(Duration::from_millis(6_001));
    assert!(!file.occupancy(clock.as_ref()).unwrap().active_owner);
    drop(file);
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    assert_eq!(
        owner::claim(file.conn(), "cli", std::process::id(), clock.now()).unwrap(),
        Claim::Claimed
    );
}

/// Übernahme nach dem Ende des eigenen Crawls (08 AI-020 [M4], Nachtrag 26.11.2026): KI während des eigenen laufenden
/// Crawls (passiv); der Crawl endet zuerst (Besitzerangaben wie beim Crawl-Ende geleert), der KI-Lauf läuft weiter →
/// spätestens nach dem nächsten Intervall trägt die Datei wieder die eigene PID mit aktuellem `heartbeat_at`, der
/// Zustand bleibt der gespeicherte Endzustand; nach dem Ende des KI-Laufs ist `owner_pid` leer.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[tokio::test(start_paused = true)]
async fn a_passive_owner_takes_over_after_the_own_crawl_ended() {
    let dir = TempDir::new("takeover");
    let path = crawl(&dir.0, "Running");
    let me = std::process::id().to_string();
    {
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.set_meta("owner_kind", "app").unwrap();
        file.set_meta("owner_pid", &me).unwrap();
        file.set_meta("heartbeat_at", &t0().to_db()).unwrap();
    }
    let clock = Arc::new(ManualClock::new(t0()));
    let owner = RunOwner::start(&path, "app", clock.clone()).unwrap();
    assert!(owner.is_passive());
    let mut beats = owner.heartbeats().unwrap();
    // Die eigene Engine endet nach 1 s (letzter Batch: Zustand, heartbeat_at = jetzt, owner_pid leer).
    tick(&clock).await;
    {
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.set_meta("state", "Completed").unwrap();
        file.set_meta("end_reason", "queue_empty").unwrap();
        file.set_meta("heartbeat_at", &plus(t0(), 1_000)).unwrap();
        file.set_meta("owner_pid", "").unwrap();
    }
    tick(&clock).await;
    beats.changed().await.unwrap();
    assert!(!owner.is_passive(), "übernommen");
    assert_eq!(meta(&path, "owner_pid"), me);
    assert_eq!(meta(&path, "heartbeat_at"), plus(t0(), 2_000));
    assert_eq!(meta(&path, "state"), "Completed");
    let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    assert!(file.occupancy(clock.as_ref()).unwrap().active_owner);
    drop(file);
    tick(&clock).await;
    tick(&clock).await;
    beats.changed().await.unwrap();
    assert_eq!(
        meta(&path, "heartbeat_at"),
        plus(t0(), 4_000),
        "Takt läuft weiter"
    );
    owner.finish().unwrap();
    assert_eq!(meta(&path, "owner_pid"), "");
    assert_eq!(meta(&path, "heartbeat_at"), plus(t0(), 4_000));
    assert_eq!(meta(&path, "state"), "Completed");
}

/// Wie oben, aber nach dem Ende des eigenen Crawls hat sich ein anderer Prozess als aktiver Besitzer eingetragen: Der
/// KI-Lauf schreibt nichts, auch nicht bei `finish`.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[tokio::test(start_paused = true)]
async fn a_passive_owner_never_overwrites_a_foreign_active_owner() {
    let dir = TempDir::new("takeover-foreign");
    let path = crawl(&dir.0, "Running");
    let me = std::process::id().to_string();
    let foreign = std::process::id().wrapping_add(3).to_string();
    {
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.set_meta("owner_kind", "app").unwrap();
        file.set_meta("owner_pid", &me).unwrap();
        file.set_meta("heartbeat_at", &t0().to_db()).unwrap();
    }
    let clock = Arc::new(ManualClock::new(t0()));
    let owner = RunOwner::start(&path, "app", clock.clone()).unwrap();
    assert!(owner.is_passive());
    tick(&clock).await;
    {
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.set_meta("state", "Paused").unwrap();
        file.set_meta("owner_kind", "cli").unwrap();
        file.set_meta("owner_pid", &foreign).unwrap();
        file.set_meta("heartbeat_at", &plus(t0(), 1_000)).unwrap();
    }
    for _ in 0..5 {
        tick(&clock).await;
    }
    tokio::task::yield_now().await;
    assert!(owner.is_passive());
    // `finish` wartet über den Mutex einen laufenden Schlag ab und schreibt selbst nichts.
    owner.finish().unwrap();
    assert_eq!(meta(&path, "owner_pid"), foreign);
    assert_eq!(meta(&path, "owner_kind"), "cli");
    assert_eq!(meta(&path, "heartbeat_at"), plus(t0(), 1_000));
    assert_eq!(meta(&path, "state"), "Paused");
}

/// Nacharbeit 26.11.2026 (08 AI-020 [M4], „Kein KI-Lauf auf einem unterbrochenen Crawl“): `start` auf einem
/// unterbrochenen Crawl liefert `StoreError::Interrupted`, die Datei bleibt byte-gleich.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[tokio::test(start_paused = true)]
async fn start_refuses_an_interrupted_crawl() {
    let dir = TempDir::new("interrupted");
    let path = crawl(&dir.0, "Running");
    {
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.set_meta("owner_pid", "4294967294").unwrap();
        file.set_meta("heartbeat_at", &plus(t0(), -60_000)).unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    let clock = Arc::new(ManualClock::new(t0()));
    let err = RunOwner::start(&path, "app", clock).unwrap_err();
    assert!(matches!(err, StoreError::Interrupted), "{err:?}");
    assert_eq!(std::fs::read(&path).unwrap(), before, "byte-gleich");
}

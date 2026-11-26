//! Unabhängige Abnahme M4-K11 (os-ai): `RunOwner` – Besitzer eines KI-Laufs mit Heartbeat.
//!
//! Erwartetes Verhalten ausschließlich aus 08 AI-020, Nachtrag 26.11.2026, Unterpunkt „Aktiver Besitzer außerhalb
//! von `Running`“ (Abschnitte „Wer schreibt“, „Übernahme nach dem Ende des eigenen Crawls“, „Absturz während eines
//! KI-Laufs“ und die zugehörigen Kriterien) sowie 01 CRAWL-125, Nachtrag K4-22 (Intervall 2 s, Grenze 10 000 ms).
//! Keine Wanduhr: tokio-Zeit angehalten (`start_paused`), Zeitstempel aus einer `ManualClock`, die im Gleichschritt
//! mit der tokio-Zeit vorgestellt wird; ein Absturz ist ein `RunOwner`, der ohne `finish` verworfen wird. Kein ai-mock.

#[path = "../../os-store/tests/acceptance_k11_support/mod.rs"]
mod k11;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use k11::*;
use os_ai::run_owner::{RunOwner, HEARTBEAT_INTERVAL};
use os_model::{Clock, CrawlState, ManualClock, Timestamp};
use os_store::{owner, CrawlFile, OpenMode};

/// Schrittweite der Stichproben (tokio-Zeit und `ManualClock` gemeinsam).
const STEP_MS: u64 = 100;

fn clock() -> Arc<ManualClock> {
    Arc::new(ManualClock::new(base_now()))
}

/// Stellt beide Uhren um `ms` vor und lässt laufende Schreibzugriffe des Takts abschließen.
async fn step(clock: &ManualClock, ms: u64) {
    clock.advance(Duration::from_millis(ms));
    tokio::time::sleep(Duration::from_millis(ms)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    // Der Schlag läuft über `spawn_blocking`; kurze Gelegenheit, den Commit abzuschließen (keine Zeitlogik).
    std::thread::sleep(Duration::from_millis(3));
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

fn read(path: &Path, key: &str) -> Option<String> {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .get_meta(key)
        .unwrap()
}

/// Lässt `total_ms` verstreichen und sammelt die verschiedenen Werte von `heartbeat_at` und `state`.
async fn sample(clock: &ManualClock, path: &Path, total_ms: u64) -> (Vec<Timestamp>, Vec<String>) {
    fn record(path: &Path, beats: &mut Vec<Timestamp>, states: &mut Vec<String>) {
        if let Some(v) = read(path, "heartbeat_at") {
            let t = Timestamp::from_db(&v).unwrap();
            if beats.last() != Some(&t) {
                beats.push(t);
            }
        }
        states.push(read(path, "state").unwrap_or_default());
    }
    let mut beats: Vec<Timestamp> = Vec::new();
    let mut states = Vec::new();
    record(path, &mut beats, &mut states);
    let mut elapsed = 0;
    while elapsed < total_ms {
        step(clock, STEP_MS).await;
        elapsed += STEP_MS;
        record(path, &mut beats, &mut states);
    }
    (beats, states)
}

fn assert_two_second_rhythm(beats: &[Timestamp], label: &str) {
    for pair in beats.windows(2) {
        let gap = pair[1].unix_millis() - pair[0].unix_millis();
        assert!(
            (2_000..=2_000 + 3 * STEP_MS as i64).contains(&gap),
            "{label}: Abstand {gap} ms zwischen {:?} und {:?}",
            pair[0].to_db(),
            pair[1].to_db()
        );
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn a_run_on_a_crawl_without_running_owner_writes_the_owner_and_a_heartbeat_every_two_seconds()
{
    // Kriterium (e) / „Heartbeat des KI-Laufs“ ohne ai-mock: 7 s → mindestens vier Werte `heartbeat_at` (Start,
    // 2 s, 4 s, 6 s), `state` durchgehend der gespeicherte; nach dem Ende `owner_pid` leer und `heartbeat_at` der
    // Endzeitpunkt.
    for (i, state) in [
        CrawlState::Completed,
        CrawlState::Paused,
        CrawlState::Incomplete,
    ]
    .into_iter()
    .enumerate()
    {
        let dir = TempDir::new("ai-run-owner");
        let (_, path) = new_crawl(dir.path(), 10 + i as u64, state);
        let clock = clock();
        let start = clock.now();
        let run = RunOwner::start(&path, "cli", clock.clone()).unwrap();
        assert!(!run.is_passive(), "{state:?}");
        assert_eq!(read(&path, "owner_kind").as_deref(), Some("cli"));
        assert_eq!(read(&path, "owner_pid"), Some(own_pid()));
        assert_eq!(read(&path, "heartbeat_at"), Some(start.to_db()));

        let (beats, states) = sample(&clock, &path, 7_000).await;
        assert!(
            beats.len() >= 4,
            "{state:?}: {:?}",
            beats.iter().map(|b| b.to_db()).collect::<Vec<_>>()
        );
        assert_eq!(beats[0], start);
        assert_two_second_rhythm(&beats, &format!("{state:?}"));
        assert!(
            states.iter().all(|s| s == state.as_str()),
            "{state:?}: state geschrieben: {states:?}"
        );
        // Während des Laufs: aktiver Besitzer mit eigener PID.
        let occ = CrawlFile::open(&path, OpenMode::ReadOnly)
            .unwrap()
            .occupancy(clock.as_ref())
            .unwrap();
        assert!(occ.active_owner && occ.occupied(), "{state:?}");

        let end = clock.now();
        run.finish().unwrap();
        assert_eq!(read(&path, "owner_pid").as_deref(), Some(""), "{state:?}");
        assert_eq!(read(&path, "heartbeat_at"), Some(end.to_db()), "{state:?}");
        assert_eq!(read(&path, "owner_kind").as_deref(), Some("cli"));
        assert_eq!(read(&path, "state").as_deref(), Some(state.as_str()));
        // Nach dem Ende schreibt niemand mehr.
        let after = meta_dump(&path);
        sample(&clock, &path, 4_000).await;
        assert_eq!(meta_dump(&path), after, "{state:?}: Schlag nach dem Ende");
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn a_foreign_active_owner_makes_the_start_fail_with_kind_and_pid_and_nothing_is_written() {
    for (i, age) in [-30_000i64, 0, 9_999, 10_000].into_iter().enumerate() {
        let dir = TempDir::new("ai-run-foreign");
        let (_, path) = new_crawl(dir.path(), 20 + i as u64, CrawlState::Completed);
        set_owner(&path, "mcp-stdio", FOREIGN_PID, Some(ago(base_now(), age)));
        let before = meta_dump(&path);
        let err = RunOwner::start(&path, "app", clock())
            .unwrap_err()
            .to_string();
        assert!(err.contains(FOREIGN_PID), "Alter {age}: {err}");
        assert!(err.contains("mcp-stdio"), "Alter {age}: {err}");
        assert_eq!(meta_dump(&path), before, "Alter {age}");
    }
    // Ein fremd laufender Crawl (1.0) ebenso.
    let dir = TempDir::new("ai-run-foreign-running");
    let (_, path) = new_crawl(dir.path(), 29, CrawlState::Running);
    set_owner(&path, "cli", FOREIGN_PID, Some(base_now()));
    let before = meta_dump(&path);
    let err = RunOwner::start(&path, "app", clock())
        .unwrap_err()
        .to_string();
    assert!(err.contains(FOREIGN_PID) && err.contains("cli"), "{err}");
    assert_eq!(meta_dump(&path), before);
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn a_stale_foreign_owner_is_overwritten_by_the_next_run() {
    // „mehr als 10 s nach dem letzten Heartbeat hat der Crawl keinen aktiven Besitzer mehr, und der nächste KI-Lauf
    // … überschreibt die Besitzerangaben.“
    let dir = TempDir::new("ai-run-stale");
    let (_, path) = new_crawl(dir.path(), 30, CrawlState::Paused);
    set_owner(
        &path,
        "mcp-stdio",
        FOREIGN_PID,
        Some(ago(base_now(), 10_001)),
    );
    let clock = clock();
    let run = RunOwner::start(&path, "app", clock.clone()).unwrap();
    assert_eq!(read(&path, "owner_pid"), Some(own_pid()));
    assert_eq!(read(&path, "owner_kind").as_deref(), Some("app"));
    assert_eq!(read(&path, "heartbeat_at"), Some(clock.now().to_db()));
    assert_eq!(read(&path, "state").as_deref(), Some("Paused"));
    run.finish().unwrap();
    assert_eq!(read(&path, "owner_pid").as_deref(), Some(""));
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn during_the_own_running_crawl_the_run_is_passive_and_never_clears_the_owner() {
    // „Läuft die KI während eines Crawls desselben Prozesses (automatische Ausführung), schreibt allein die Engine den
    // Heartbeat; der KI-Lauf schreibt dann keine Besitzerangaben und leert `owner_pid` nicht.“ Kriterium „Endet der
    // KI-Teil vor dem Crawl, bleibt `owner_pid` gesetzt und der Crawl `Running`“ (ohne Engine: simulierter Heartbeat).
    let dir = TempDir::new("ai-run-passive");
    let (_, path) = new_crawl(dir.path(), 40, CrawlState::Running);
    set_owner(&path, "app", &own_pid(), Some(base_now()));
    let before = meta_dump(&path);
    let clock = clock();
    let run = RunOwner::start(&path, "app", clock.clone()).unwrap();
    assert!(run.is_passive());
    assert_eq!(meta_dump(&path), before);
    sample(&clock, &path, 7_000).await;
    assert_eq!(
        meta_dump(&path),
        before,
        "passiver Lauf schreibt keinen Heartbeat"
    );
    run.finish().unwrap();
    assert_eq!(
        meta_dump(&path),
        before,
        "owner_pid bleibt, Zustand Running"
    );
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn after_a_crash_the_pid_stays_and_after_ten_seconds_another_process_takes_over() {
    // „Absturz während eines KI-Laufs: `owner_pid` bleibt gesetzt; mehr als 10 s nach dem letzten Heartbeat hat der
    // Crawl keinen aktiven Besitzer mehr, und der nächste KI-Lauf … überschreibt die Besitzerangaben.“
    let dir = TempDir::new("ai-run-crash");
    let (_, path) = new_crawl(dir.path(), 50, CrawlState::Completed);
    let clock = clock();
    let run = RunOwner::start(&path, "cli", clock.clone()).unwrap();
    sample(&clock, &path, 4_500).await;
    drop(run); // ohne finish: wie ein Absturz
    let crashed = meta_dump(&path);
    assert_eq!(crashed["owner_pid"], own_pid());
    let last = Timestamp::from_db(&crashed["heartbeat_at"]).unwrap();
    sample(&clock, &path, 6_000).await;
    assert_eq!(
        meta_dump(&path),
        crashed,
        "nach dem Absturz wird nichts geschrieben"
    );

    let other: u32 = 4_000_123;
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let within = ago(last, -10_000);
    assert!(owner::claim(f.conn(), "mcp-stdio", other, within).is_err());
    let stale = ago(last, -10_001);
    let occ = f.occupancy(&ManualClock::new(stale)).unwrap();
    assert!(!occ.active_owner && !occ.occupied());
    assert_eq!(
        owner::claim(f.conn(), "mcp-stdio", other, stale).unwrap(),
        owner::Claim::Claimed
    );
    drop(f);
    assert_eq!(read(&path, "owner_pid"), Some(other.to_string()));
    assert_eq!(read(&path, "owner_kind").as_deref(), Some("mcp-stdio"));
    assert_eq!(read(&path, "state").as_deref(), Some("Completed"));
}

/// Letzter Batch der Engine beim Crawl-Ende (CRAWL-125): `state`, `heartbeat_at` = jetzt, leeres `owner_pid`,
/// `ended_at`/`end_reason` – ein Commit.
fn engine_end(path: &Path, at: Timestamp, state: CrawlState) {
    let f = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
    f.conn().execute_batch("BEGIN IMMEDIATE").unwrap();
    f.set_meta("state", state.as_str()).unwrap();
    f.set_meta("heartbeat_at", &at.to_db()).unwrap();
    f.set_meta("owner_pid", "").unwrap();
    if state == CrawlState::Paused {
        f.set_meta("paused_at", &at.to_db()).unwrap();
    } else {
        f.set_meta("ended_at", &at.to_db()).unwrap();
        f.set_meta("end_reason", "limit_reached").unwrap();
    }
    f.conn().execute_batch("COMMIT").unwrap();
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn when_the_own_crawl_ends_first_the_run_takes_over_within_one_interval() {
    // „Übernahme nach dem Ende des eigenen Crawls“: Die Engine leert `owner_pid`; der KI-Lauf trägt sich beim nächsten
    // Schlag (spätestens 2 s später) selbst als Besitzer ein und führt den Heartbeat bis zu seinem Ende weiter. Der
    // Zustand bleibt der gespeicherte Endzustand; nach dem Ende des KI-Laufs ist `owner_pid` leer.
    for (i, end_state) in [CrawlState::Completed, CrawlState::Paused]
        .into_iter()
        .enumerate()
    {
        let dir = TempDir::new("ai-run-takeover");
        let (_, path) = new_crawl(dir.path(), 60 + i as u64, CrawlState::Running);
        set_owner(&path, "app", &own_pid(), Some(base_now()));
        let clock = clock();
        let run = RunOwner::start(&path, "app", clock.clone()).unwrap();
        assert!(run.is_passive(), "eigener laufender Crawl");
        // Die Engine schreibt weiter ihren Heartbeat (simuliert) und endet nach 1,3 s.
        step(&clock, 1_300).await;
        let ended = clock.now();
        engine_end(&path, ended, end_state);

        let mut took_over_at = None;
        let mut waited = 0;
        while waited < 2_000 + STEP_MS {
            step(&clock, STEP_MS).await;
            waited += STEP_MS;
            if read(&path, "owner_pid") == Some(own_pid()) {
                took_over_at = Some(clock.now());
                break;
            }
        }
        assert!(
            took_over_at.is_some(),
            "{end_state:?}: spätestens 2 s nach dem Crawl-Ende muss owner_pid wieder den Prozess nennen (owner_pid = {:?})",
            read(&path, "owner_pid")
        );
        let hb = Timestamp::from_db(&read(&path, "heartbeat_at").unwrap()).unwrap();
        assert!(
            hb.unix_millis() > ended.unix_millis(),
            "{end_state:?}: aktueller Heartbeat"
        );
        assert_eq!(read(&path, "state").as_deref(), Some(end_state.as_str()));
        let (beats, states) = sample(&clock, &path, 5_000).await;
        assert!(
            beats.len() >= 3,
            "{end_state:?}: Heartbeat läuft weiter: {beats:?}"
        );
        assert!(states.iter().all(|s| s == end_state.as_str()));
        let occ = CrawlFile::open(&path, OpenMode::ReadOnly)
            .unwrap()
            .occupancy(clock.as_ref())
            .unwrap();
        assert!(
            occ.active_owner,
            "{end_state:?}: Verwaltungsschutz während des KI-Laufs"
        );
        run.finish().unwrap();
        assert_eq!(read(&path, "owner_pid").as_deref(), Some(""));
        assert_eq!(read(&path, "state").as_deref(), Some(end_state.as_str()));
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn after_the_own_crawl_ends_the_run_never_overwrites_a_foreign_active_owner() {
    // „… er überschreibt dabei nie einen fremden aktiven Besitzer.“ / Kriterium: „Steht zu diesem Zeitpunkt ein fremder
    // aktiver Besitzer in der Datei, schreibt der KI-Lauf nichts.“
    let dir = TempDir::new("ai-run-takeover-foreign");
    let (_, path) = new_crawl(dir.path(), 70, CrawlState::Running);
    set_owner(&path, "app", &own_pid(), Some(base_now()));
    let clock = clock();
    let run = RunOwner::start(&path, "app", clock.clone()).unwrap();
    step(&clock, 700).await;
    engine_end(&path, clock.now(), CrawlState::Completed);
    // Ein anderer Prozess trägt sich sofort ein und hält seinen Heartbeat aktuell (simuliert).
    set_owner(&path, "mcp-stdio", FOREIGN_PID, Some(clock.now()));
    for _ in 0..3 {
        let before = meta_dump(&path);
        sample(&clock, &path, 2_000).await;
        assert_eq!(
            meta_dump(&path),
            before,
            "KI-Lauf schreibt bei fremdem Besitzer"
        );
        set(&path, "heartbeat_at", &clock.now().to_db());
    }
    let before = meta_dump(&path);
    run.finish().unwrap();
    assert_eq!(
        meta_dump(&path),
        before,
        "finish gibt keinen fremden Besitzer frei"
    );
    assert_eq!(read(&path, "owner_pid").as_deref(), Some(FOREIGN_PID));
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[tokio::test(start_paused = true)]
async fn no_run_starts_on_an_interrupted_crawl_until_it_is_stopped() {
    // 08 AI-020, Nachtrag 26.11.2026 „Kein KI-Lauf auf einem unterbrochenen Crawl“ (Entscheidung nach Befund B1-5/B2-1
    // dieser Abnahme): gespeichert `Running`, `owner_pid` gesetzt, Heartbeat 11 s alt → Start abgelehnt mit
    // „unterbrochen – zuerst fortsetzen oder stoppen“, Datei byte-gleich, abgeleiteter Zustand bleibt „Unterbrochen“;
    // nach „Stoppen“ ohne Engine gelingt der Start (dann gilt der abgeschlossene Crawl).
    let dir = TempDir::new("ai-run-interrupted");
    let (_, path) = new_crawl(dir.path(), 80, CrawlState::Running);
    set_owner(&path, "cli", FOREIGN_PID, Some(ago(base_now(), 11_000)));
    let before = file_bytes(&path);
    let clock = clock();
    let err = RunOwner::start(&path, "app", clock.clone())
        .expect_err("kein KI-Lauf auf einem unterbrochenen Crawl")
        .to_string();
    assert!(
        err.contains("unterbrochen") && err.contains("zuerst fortsetzen oder stoppen"),
        "{err}"
    );
    sample(&clock, &path, 4_000).await;
    assert_eq!(file_bytes(&path), before, "Datei byte-gleich");
    let listing = os_store::registry::read_listing(&path).unwrap();
    assert_eq!(listing.effective_state(clock.now()), CrawlState::Incomplete);

    stop_without_engine(&path, clock.now());
    let run = RunOwner::start(&path, "app", clock.clone()).unwrap();
    assert!(!run.is_passive());
    let (beats, states) = sample(&clock, &path, 7_000).await;
    assert!(
        beats.len() >= 4,
        "{:?}",
        beats.iter().map(|b| b.to_db()).collect::<Vec<_>>()
    );
    assert!(states.iter().all(|s| s == "Completed"), "{states:?}");
    let end = clock.now();
    run.finish().unwrap();
    assert_eq!(read(&path, "owner_pid").as_deref(), Some(""));
    assert_eq!(read(&path, "heartbeat_at"), Some(end.to_db()));
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn the_run_uses_the_heartbeat_interval_of_crawl_125() {
    assert_eq!(HEARTBEAT_INTERVAL, Duration::from_secs(2));
    assert_eq!(HEARTBEAT_INTERVAL, owner::HEARTBEAT_INTERVAL);
}

//! Unabhängige Abnahme M4-K11 (os-store): Schreibseite des Besitzers eines KI-Laufs (`claim`, `beat`, `release`)
//! und die Konstanten aus CRAWL-125.
//!
//! Erwartetes Verhalten ausschließlich aus 08 AI-020, Nachtrag 26.11.2026, Unterpunkt „Aktiver Besitzer außerhalb
//! von `Running`“, Abschnitte „Wer schreibt“, „Übernahme nach dem Ende des eigenen Crawls“ und „Absturz während eines
//! KI-Laufs“, sowie 01 CRAWL-125, Nachtrag K4-22. Die PID ist hier ein Parameter: so lassen sich zwei Prozesse
//! simulieren, ohne einen zweiten zu starten.

#[path = "acceptance_k11_support/mod.rs"]
mod k11;

use std::time::Duration;

use k11::*;
use os_model::CrawlState;
use os_store::owner::{self, Claim};
use os_store::{registry, CrawlFile, OpenMode};

const PID_A: u32 = 4_000_001;
const PID_B: u32 = 4_000_002;

fn open_rw(path: &std::path::Path) -> CrawlFile {
    CrawlFile::open(path, OpenMode::ReadWrite).unwrap()
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn claim_writes_kind_pid_and_heartbeat_but_never_the_state() {
    // „Wer schreibt“: zu Beginn `owner_kind`, `owner_pid` und `heartbeat_at`; `state` schreibt er nicht.
    let dir = TempDir::new("claim-start");
    let now = base_now();
    for (i, state) in [
        CrawlState::Completed,
        CrawlState::Paused,
        CrawlState::Incomplete,
    ]
    .into_iter()
    .enumerate()
    {
        let (_, path) = new_crawl(dir.path(), 10 + i as u64, state);
        let before = meta_dump(&path);
        let f = open_rw(&path);
        assert_eq!(
            owner::claim(f.conn(), "mcp-stdio", PID_A, now).unwrap(),
            Claim::Claimed
        );
        drop(f);
        let after = meta_dump(&path);
        let mut changed = changed_keys(&before, &after);
        changed.retain(|k| k != "openspider_version" && k != "change_seq");
        assert_eq!(
            changed,
            vec!["heartbeat_at", "owner_kind", "owner_pid"],
            "{state:?}"
        );
        assert_eq!(after["owner_kind"], "mcp-stdio");
        assert_eq!(after["owner_pid"], PID_A.to_string());
        assert_eq!(after["heartbeat_at"], now.to_db());
        assert_eq!(after["state"], state.as_str());
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn beat_writes_only_the_heartbeat_and_release_clears_the_pid_with_the_end_time() {
    // „danach alle 2 s `heartbeat_at` … am Ende einmal `heartbeat_at` = jetzt mit leerem `owner_pid`“; `owner_kind`
    // nennt danach den letzten Besitzer (CRAWL-125).
    let dir = TempDir::new("claim-release");
    let t0 = base_now();
    let (_, path) = new_crawl(dir.path(), 20, CrawlState::Completed);
    let f = open_rw(&path);
    owner::claim(f.conn(), "cli", PID_A, t0).unwrap();
    let claimed = meta_dump(&path);
    let t1 = ago(t0, -2_000);
    assert!(owner::beat(f.conn(), PID_A, t1).unwrap());
    let beaten = meta_dump(&path);
    let mut changed = changed_keys(&claimed, &beaten);
    changed.retain(|k| k != "openspider_version" && k != "change_seq");
    assert_eq!(changed, vec!["heartbeat_at"]);
    assert_eq!(beaten["heartbeat_at"], t1.to_db());

    let end = ago(t0, -3_500);
    assert!(owner::release(f.conn(), PID_A, end).unwrap());
    drop(f);
    let released = meta_dump(&path);
    assert_eq!(released["owner_pid"], "");
    assert_eq!(released["heartbeat_at"], end.to_db());
    assert_eq!(released["owner_kind"], "cli");
    assert_eq!(released["state"], "Completed");
    let listing = registry::read_listing(&path).unwrap();
    assert!(!listing.occupied(end));
    assert_eq!(listing.effective_state(end), CrawlState::Completed);
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn a_foreign_active_owner_makes_the_claim_fail_with_kind_and_pid_and_leaves_the_file_unchanged() {
    let dir = TempDir::new("claim-foreign");
    let now = base_now();
    for (i, age) in [-5_000i64, 0, 9_999, 10_000].into_iter().enumerate() {
        let (_, path) = new_crawl(dir.path(), 30 + i as u64, CrawlState::Completed);
        let f = open_rw(&path);
        owner::claim(f.conn(), "mcp-stdio", PID_A, ago(now, age)).unwrap();
        let before = meta_dump(&path);
        let err = owner::claim(f.conn(), "app", PID_B, now)
            .unwrap_err()
            .to_string();
        assert!(err.contains(&PID_A.to_string()), "Alter {age}: {err}");
        assert!(err.contains("mcp-stdio"), "Alter {age}: {err}");
        // Ein fremder Prozess darf den Besitzer weder auffrischen noch freigeben.
        assert!(!owner::beat(f.conn(), PID_B, now).unwrap());
        assert!(!owner::release(f.conn(), PID_B, now).unwrap());
        drop(f);
        assert_eq!(meta_dump(&path), before, "Alter {age}: Datei unverändert");
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn a_foreign_running_crawl_makes_the_claim_fail_like_in_1_0() {
    // 1.0: ein fremd laufender Crawl (gespeichertes `Running`, aktueller Heartbeat) – ein KI-Lauf darf sich nicht
    // eintragen (AI-020 „Crawl in einem fremden Writer-Prozess“).
    let dir = TempDir::new("claim-running");
    let now = base_now();
    let (_, path) = new_crawl(dir.path(), 40, CrawlState::Running);
    set_owner(&path, "cli", "4321", Some(now));
    let before = meta_dump(&path);
    let f = open_rw(&path);
    let err = owner::claim(f.conn(), "app", PID_B, ago(now, -3_000))
        .unwrap_err()
        .to_string();
    drop(f);
    assert!(err.contains("4321") && err.contains("cli"), "{err}");
    assert_eq!(meta_dump(&path), before);
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn during_the_own_running_crawl_the_claim_is_passive_and_writes_nothing() {
    // „Läuft die KI während eines Crawls desselben Prozesses …, schreibt allein die Engine den Heartbeat; der KI-Lauf
    // schreibt dann keine Besitzerangaben und leert `owner_pid` nicht.“
    let dir = TempDir::new("claim-passive");
    let now = base_now();
    let (_, path) = new_crawl(dir.path(), 50, CrawlState::Running);
    set_owner(&path, "app", &PID_A.to_string(), Some(now));
    let before = meta_dump(&path);
    let f = open_rw(&path);
    assert_eq!(
        owner::claim(f.conn(), "app", PID_A, ago(now, -500)).unwrap(),
        Claim::Passive
    );
    assert!(!owner::beat(f.conn(), PID_A, ago(now, -1_000)).unwrap());
    assert!(!owner::release(f.conn(), PID_A, ago(now, -1_500)).unwrap());
    drop(f);
    assert_eq!(meta_dump(&path), before);
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn after_a_crash_the_owner_stays_set_and_the_next_run_takes_over_only_after_ten_seconds() {
    // „Absturz während eines KI-Laufs: `owner_pid` bleibt gesetzt; mehr als 10 s nach dem letzten Heartbeat hat der
    // Crawl keinen aktiven Besitzer mehr, und der nächste KI-Lauf … überschreibt die Besitzerangaben.“
    let dir = TempDir::new("claim-crash");
    let t0 = base_now();
    let (_, path) = new_crawl(dir.path(), 60, CrawlState::Paused);
    let f = open_rw(&path);
    owner::claim(f.conn(), "cli", PID_A, t0).unwrap();
    let last = ago(t0, -4_000);
    owner::beat(f.conn(), PID_A, last).unwrap();
    // Kein release (Absturz).
    let crashed = meta_dump(&path);
    assert_eq!(crashed["owner_pid"], PID_A.to_string());
    assert!(owner::claim(f.conn(), "app", PID_B, ago(last, -10_000)).is_err());
    assert_eq!(meta_dump(&path), crashed);
    let listing = registry::read_listing(&path).unwrap();
    assert!(listing.occupied(ago(last, -10_000)));
    assert!(!listing.occupied(ago(last, -10_001)));
    assert_eq!(
        listing.effective_state(ago(last, -10_001)),
        CrawlState::Paused
    );

    let takeover = ago(last, -10_001);
    assert_eq!(
        owner::claim(f.conn(), "app", PID_B, takeover).unwrap(),
        Claim::Claimed
    );
    drop(f);
    let after = meta_dump(&path);
    assert_eq!(after["owner_pid"], PID_B.to_string());
    assert_eq!(after["owner_kind"], "app");
    assert_eq!(after["heartbeat_at"], takeover.to_db());
    assert_eq!(after["state"], "Paused");
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn a_claim_on_an_interrupted_crawl_keeps_the_management_state_and_further_heartbeats_are_written() {
    // „KI-Lauf auf einem Crawl ohne laufenden Besitzer (pausiert, abgeschlossen, unterbrochen)“: Die Besitzerangaben
    // werden eingetragen, „danach alle 2 s `heartbeat_at` …, solange der KI-Lauf läuft“, und „am Ende … leerem
    // `owner_pid`“. „Unterbrochen“ ist in 1.0-Dateien ein gespeichertes `Running` mit veraltetem Heartbeat (Absturz).
    let dir = TempDir::new("claim-interrupted");
    let t0 = base_now();
    let (_, path) = new_crawl(dir.path(), 70, CrawlState::Running);
    set_owner(&path, "cli", "4321", Some(ago(t0, 60_000)));
    let listing = registry::read_listing(&path).unwrap();
    assert_eq!(listing.effective_state(t0), CrawlState::Incomplete);
    let f = open_rw(&path);
    assert_eq!(
        owner::claim(f.conn(), "app", PID_A, t0).unwrap(),
        Claim::Claimed,
        "ein unterbrochener Crawl hat keinen laufenden Besitzer"
    );
    let t1 = ago(t0, -2_000);
    assert!(
        owner::beat(f.conn(), PID_A, t1).unwrap(),
        "der Heartbeat des KI-Laufs muss auch auf einem unterbrochenen Crawl geschrieben werden"
    );
    let end = ago(t0, -3_000);
    assert!(owner::release(f.conn(), PID_A, end).unwrap());
    drop(f);
    let after = meta_dump(&path);
    assert_eq!(after["heartbeat_at"], end.to_db());
    assert_eq!(after["owner_pid"], "");
    assert_eq!(after["state"], "Running", "state schreibt er nicht");
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn interval_and_limit_are_the_values_of_crawl_125() {
    // „alle 2 s … (gleiches Intervall wie CRAWL-125)“, „höchstens 10 000 ms alt (Grenze … wie CRAWL-125)“.
    assert_eq!(owner::HEARTBEAT_INTERVAL, Duration::from_secs(2));
    assert_eq!(owner::HEARTBEAT_TIMEOUT_MS, 10_000);
    assert_eq!(os_store::resume::HEARTBEAT_TIMEOUT_MS, 10_000);
}

//! Unabhängige Abnahme M4-K11 (os-store): Leseregel „aktiver Besitzer“, Prüfungen in der Crawl-Verwaltung und
//! unverändertes Verhalten für Dateien aus 1.0.
//!
//! Erwartetes Verhalten ausschließlich aus 08 AI-020, Nachtrag 26.11.2026, Unterpunkt „Aktiver Besitzer außerhalb
//! von `Running`“ (Leseregel, Wirkung a, Kriterien 1, 2 und 6) und 01 CRAWL-125, Nachtrag 26.11.2026 (Klärung K4-22).
//! Heartbeats werden über die Metadaten simuliert, die Uhr ist ein fester Zeitpunkt (keine Wanduhr).

#[path = "acceptance_k11_support/mod.rs"]
mod k11;

use std::collections::HashSet;

use k11::*;
use os_model::{CrawlState, ManualClock, Timestamp};
use os_store::owner::{active_owner_at, Occupancy};
use os_store::registry::{self, Manager};
use os_store::{CrawlFile, OpenMode};

const ACTIVE_STATES: [CrawlState; 3] = [
    CrawlState::Completed,
    CrawlState::Paused,
    CrawlState::Incomplete,
];

fn file_occupancy(path: &std::path::Path, now: Timestamp) -> Occupancy {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .occupancy(&ManualClock::new(now))
        .unwrap()
}

fn file_state(path: &std::path::Path, now: Timestamp) -> CrawlState {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .effective_state(&ManualClock::new(now))
        .unwrap()
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn boundary_values_decide_the_active_owner_for_completed_paused_and_interrupted() {
    // Kriterium 1: jetzt, 9 999 und 10 000 ms → aktiver Besitzer; 10 001 ms oder `owner_pid` leer → keiner;
    // Zukunft wie CRAWL-125 aktuell. Der abgeleitete Zustand ist in allen Fällen der gespeicherte.
    let dir = TempDir::new("rule-boundaries");
    let now = base_now();
    let mut seed = 100;
    for state in ACTIVE_STATES {
        for (pid, kind) in [(FOREIGN_PID.to_string(), "mcp-stdio"), (own_pid(), "app")] {
            for (age, expected) in [
                (-1, true),
                (-60_000, true),
                (0, true),
                (9_999, true),
                (10_000, true),
                (10_001, false),
                (3_600_000, false),
            ] {
                seed += 1;
                let (_, path) = new_crawl(dir.path(), seed, state);
                set_owner(&path, kind, &pid, Some(ago(now, age)));
                let label = format!("{state:?}, PID {pid}, Alter {age} ms");

                let occ = file_occupancy(&path, now);
                assert_eq!(occ.active_owner, expected, "Datei: {label}");
                assert_eq!(file_state(&path, now), state, "Zustand Datei: {label}");

                let listing = registry::read_listing(&path).unwrap();
                let l_occ = listing.occupancy(now);
                assert_eq!(l_occ.active_owner, expected, "Eintrag: {label}");
                assert_eq!(
                    listing.active_owner(now),
                    expected.then_some(pid.as_str()),
                    "PID des aktiven Besitzers: {label}"
                );
                assert_eq!(
                    listing.effective_state(now),
                    state,
                    "Zustand Eintrag: {label}"
                );
                // Gespeicherter Zustand ≠ Running: „laufend nach CRAWL-125“ nie, belegt genau bei aktivem Besitzer.
                assert!(!occ.running && !l_occ.running, "{label}");
                assert_eq!(listing.occupied(now), expected, "belegt: {label}");
                assert_eq!(occ.occupied(), expected, "belegt Datei: {label}");

                // Reine Regel ohne Dateizugriff.
                assert_eq!(
                    active_owner_at(Some(&pid), Some(ago(now, age)), now),
                    expected,
                    "{label}"
                );
            }
            // `owner_pid` leer oder fehlend → kein aktiver Besitzer, auch bei frischem Heartbeat.
            for (empty, delete) in [("", false), ("", true)] {
                seed += 1;
                let (_, path) = new_crawl(dir.path(), seed, state);
                set_owner(&path, kind, empty, Some(now));
                if delete {
                    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
                    delete_meta(&f, "owner_pid");
                }
                let label = format!("{state:?}, owner_pid leer (fehlt: {delete})");
                assert!(!file_occupancy(&path, now).active_owner, "{label}");
                let listing = registry::read_listing(&path).unwrap();
                assert!(!listing.occupancy(now).active_owner, "{label}");
                assert_eq!(listing.active_owner(now), None, "{label}");
                assert!(!listing.occupied(now), "{label}");
                assert_eq!(listing.effective_state(now), state, "{label}");
            }
        }
        assert!(!active_owner_at(Some(""), Some(now), now));
        assert!(!active_owner_at(None, Some(now), now));
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn without_heartbeat_there_is_no_active_owner_even_with_a_fresh_start_time() {
    // Leseregel: ohne die Ersatzkette über `started_at`/`created_at` – fehlt `heartbeat_at`, gibt es keinen aktiven
    // Besitzer.
    let dir = TempDir::new("rule-no-heartbeat");
    let now = base_now();
    for (i, state) in ACTIVE_STATES.into_iter().enumerate() {
        let (_, path) = new_crawl(dir.path(), 200 + i as u64, state);
        set_owner(&path, "cli", FOREIGN_PID, None);
        set(&path, "started_at", &now.to_db());
        set(&path, "created_at", &now.to_db());
        let label = format!("{state:?} ohne heartbeat_at");
        let occ = file_occupancy(&path, now);
        assert!(!occ.active_owner, "{label}");
        assert!(!occ.occupied(), "{label}");
        let listing = registry::read_listing(&path).unwrap();
        assert!(!listing.occupancy(now).active_owner, "{label}");
        assert!(!listing.occupied(now), "{label}");
        assert!(!listing.foreign_led(now), "{label}");
        assert_eq!(listing.effective_state(now), state, "{label}");
        assert!(!active_owner_at(Some(FOREIGN_PID), None, now));
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn a_stale_heartbeat_never_turns_completed_or_paused_into_interrupted() {
    // „ein veralteter Heartbeat macht aus `Completed` oder `Paused` kein „Unterbrochen““ – auch mit gesetztem
    // `owner_pid` (Absturz während eines KI-Laufs).
    let dir = TempDir::new("rule-stale");
    let now = base_now();
    for (i, state) in [CrawlState::Completed, CrawlState::Paused]
        .into_iter()
        .enumerate()
    {
        let (_, path) = new_crawl(dir.path(), 300 + i as u64, state);
        set_owner(&path, "app", FOREIGN_PID, Some(ago(now, 86_400_000)));
        assert_eq!(file_state(&path, now), state);
        let listing = registry::read_listing(&path).unwrap();
        assert_eq!(listing.effective_state(now), state);
        assert!(!listing.occupied(now));
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn foreign_led_depends_on_the_pid_only_for_the_active_owner() {
    // Wirkung (b) hängt an der fremden PID, Wirkung (a) an jedem aktiven Besitzer.
    let dir = TempDir::new("rule-foreign");
    let now = base_now();
    let own: u32 = std::process::id();
    for (i, state) in ACTIVE_STATES.into_iter().enumerate() {
        let (_, foreign) = new_crawl(dir.path(), 400 + 2 * i as u64, state);
        set_owner(&foreign, "mcp-stdio", FOREIGN_PID, Some(now));
        let (_, mine) = new_crawl(dir.path(), 401 + 2 * i as u64, state);
        set_owner(&mine, "app", &own_pid(), Some(now));

        let f = file_occupancy(&foreign, now);
        assert!(f.occupied() && f.foreign_led(own) && f.foreign_active_owner(own));
        assert_eq!(f.owner_pid.as_deref(), Some(FOREIGN_PID));
        assert_eq!(f.owner_kind.as_deref(), Some("mcp-stdio"));
        let m = file_occupancy(&mine, now);
        assert!(
            m.occupied(),
            "eigener aktiver Besitzer schützt in der Verwaltung"
        );
        assert!(!m.foreign_led(own) && !m.foreign_active_owner(own));

        let lf = registry::read_listing(&foreign).unwrap();
        assert!(lf.occupied(now) && lf.foreign_led(now));
        let lm = registry::read_listing(&mine).unwrap();
        assert!(lm.occupied(now) && !lm.foreign_led(now));
    }
}

// spec: AI-020 [M4]
// spec: OUT-228
// spec: OUT-229
// acceptance: independent
#[test]
fn management_rejects_every_file_action_for_any_active_owner_and_retention_skips_it() {
    // Kriterium 2 und Wirkung (a): Löschen, Verschieben, Exportieren, Sperren und Entsperren → abgewiesen
    // (`busy`), die Aufbewahrung übergeht den Crawl – für Completed, Paused und „Unterbrochen“, fremde wie eigene PID.
    let now = base_now();
    let mut seed = 500;
    for state in ACTIVE_STATES {
        for pid in [FOREIGN_PID.to_string(), own_pid()] {
            let dir = TempDir::new("rule-manager");
            seed += 1;
            let (id, path) = new_crawl(dir.path(), seed, state);
            set_owner(&path, "mcp-stdio", &pid, Some(now));
            // Alt genug für jede Aufbewahrungsregel.
            set(&path, "ended_at", &ago(now, 400 * 86_400_000).to_db());
            set(&path, "locked", "0");
            let label = format!("{state:?}, PID {pid}");
            let before = meta_dump(&path);
            let mut m = Manager::open(dir.path()).unwrap();
            assert!(m.delete(&id, now).is_err(), "Löschen: {label}");
            assert!(m.move_project(&id, None, now).is_err(), "Projekt: {label}");
            let other = TempDir::new("rule-manager-dest");
            assert!(
                m.move_file(&id, other.path(), now, &|_, _| true).is_err(),
                "Verschieben: {label}"
            );
            let out = TempDir::new("rule-manager-export");
            let export = out.path().join("export.ospider");
            assert!(
                m.export(&id, &export, now, &|_, _| true).is_err(),
                "Exportieren: {label}"
            );
            assert!(!export.exists(), "{label}");
            assert!(m.lock(&id, true, now).is_err(), "Sperren: {label}");
            set(&path, "locked", "1");
            assert!(m.lock(&id, false, now).is_err(), "Entsperren: {label}");
            set(&path, "locked", "0");
            assert!(
                m.retain(Some(1), Some(0), now, &HashSet::new())
                    .unwrap()
                    .is_empty(),
                "Aufbewahrung: {label}"
            );
            assert!(path.exists(), "{label}");
            assert_eq!(meta_dump(&path), before, "Datei unverändert: {label}");
        }
    }
}

// spec: AI-020 [M4]
// spec: OUT-228
// acceptance: independent
#[test]
fn eleven_seconds_after_the_last_heartbeat_deleting_succeeds() {
    // Kriterium 2, letzter Satz: 11 s später ohne neuen Heartbeat gelingt das Löschen; ebenso die Aufbewahrung.
    let heartbeat = base_now();
    let later = ago(heartbeat, -11_000);
    for (i, state) in ACTIVE_STATES.into_iter().enumerate() {
        let dir = TempDir::new("rule-eleven");
        let (id, path) = new_crawl(dir.path(), 600 + i as u64, state);
        set_owner(&path, "cli", FOREIGN_PID, Some(heartbeat));
        let mut m = Manager::open(dir.path()).unwrap();
        assert!(m.delete(&id, heartbeat).is_err(), "{state:?}: jetzt belegt");
        assert!(
            m.delete(&id, ago(heartbeat, -10_000)).is_err(),
            "{state:?}: 10 s"
        );
        m.delete(&id, later).unwrap();
        assert!(!path.exists(), "{state:?}");

        let (id2, path2) = new_crawl(dir.path(), 650 + i as u64, state);
        set_owner(&path2, "cli", FOREIGN_PID, Some(heartbeat));
        set(
            &path2,
            "ended_at",
            &ago(heartbeat, 400 * 86_400_000).to_db(),
        );
        let removed = m.retain(None, Some(0), later, &HashSet::new()).unwrap();
        assert_eq!(
            removed.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![id2],
            "{state:?}: Aufbewahrung nach Ablauf"
        );
        assert!(!path2.exists());
    }
}

// spec: AI-020 [M4]
// spec: CRAWL-128
// acceptance: independent
#[test]
fn a_foreign_active_owner_makes_the_crawl_not_resumable_until_the_heartbeat_is_stale() {
    // Wirkung (b): Fortsetzen wird abgewiesen wie bei einem fremd geführten laufenden Crawl (die Verwaltung bietet
    // es dann nicht an); mit eigener PID greift (b) nicht.
    let dir = TempDir::new("rule-resume");
    let now = base_now();
    let (_, foreign) = new_crawl(dir.path(), 700, CrawlState::Paused);
    set_owner(&foreign, "cli", FOREIGN_PID, Some(now));
    let (_, mine) = new_crawl(dir.path(), 701, CrawlState::Paused);
    set_owner(&mine, "app", &own_pid(), Some(now));
    let lf = registry::read_listing(&foreign).unwrap();
    assert!(!lf.can_resume(now));
    assert!(lf.can_resume(ago(now, -10_001)));
    assert_eq!(lf.effective_state(now), CrawlState::Paused);
    let lm = registry::read_listing(&mine).unwrap();
    assert!(lm.can_resume(now));
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// spec: OUT-228
// spec: OUT-229
// acceptance: independent
#[test]
fn files_from_1_0_in_every_state_are_occupied_exactly_when_running_by_crawl_125() {
    // Kriterium 6: Für jeden Eintrag liefert die Prüfung „belegt“ (laufend nach CRAWL-125 oder aktiver Besitzer)
    // dasselbe wie CRAWL-125 allein; Zustand, Fortsetzbarkeit, Verwaltung und Aufbewahrung wie 1.0.
    let (now, cases) = v10_cases();
    assert_fixture_consistent(&cases);
    let own: u32 = std::process::id();
    for (i, case) in cases.iter().enumerate() {
        let dir = TempDir::new("rule-v10");
        let (id, path) = case.build(dir.path(), 800 + i as u64, now);
        let running = case.oracle_running();
        let label = &case.name;

        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        let clock = ManualClock::new(now);
        assert_eq!(
            file.effective_state(&clock).unwrap(),
            case.oracle_state(),
            "{label}"
        );
        let occ = file.occupancy(&clock).unwrap();
        assert_eq!(occ.running, running, "{label}");
        assert_eq!(occ.occupied(), running, "Datei belegt: {label}");
        assert_eq!(occ.foreign_led(own), running && case.foreign(), "{label}");
        drop(file);

        let listing = registry::read_listing(&path).unwrap();
        assert_eq!(listing.effective_state(now), case.oracle_state(), "{label}");
        assert_eq!(listing.occupied(now), running, "Eintrag belegt: {label}");
        assert_eq!(
            listing.foreign_led(now),
            running && case.foreign(),
            "{label}"
        );
        assert_eq!(
            listing.can_resume(now),
            case.oracle_resumable(),
            "fortsetzbar: {label}"
        );

        let mut m = Manager::open(dir.path()).unwrap();
        let cached = m.get(&id).unwrap();
        assert_eq!(
            cached.effective_state(now),
            case.oracle_state(),
            "Index: {label}"
        );
        assert_eq!(cached.occupied(now), running, "Index belegt: {label}");
        assert_eq!(
            cached.can_resume(now),
            case.oracle_resumable(),
            "Index: {label}"
        );
        assert_eq!(m.lock(&id, true, now).is_err(), running, "Sperren: {label}");
        if !running {
            m.lock(&id, false, now).unwrap();
        }
        let out = TempDir::new("rule-v10-export");
        let export = out.path().join("export.ospider");
        assert_eq!(
            m.export(&id, &export, now, &|_, _| true).is_err(),
            running,
            "Exportieren: {label}"
        );
        let removed = m.retain(None, Some(0), now, &HashSet::new());
        let removed = removed.unwrap();
        assert_eq!(removed.is_empty(), running, "Aufbewahrung: {label}");
        if running {
            assert!(m.delete(&id, now).is_err(), "Löschen: {label}");
            assert!(path.exists(), "{label}");
        } else {
            assert!(!path.exists(), "{label}");
        }
    }
}

fn has_link_index(path: &std::path::Path) -> bool {
    let f = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    let n: i64 = f
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'links_target'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    n == 1
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn the_deferred_link_index_is_not_built_into_the_file_of_a_foreign_active_owner() {
    // Wirkung (b): Ein fremder aktiver Besitzer wird „behandelt wie ein fremd geführter laufender Crawl“; in eine
    // laufende Datei baut die Analyse bzw. der Export den Index nicht nach (Nacharbeit M4-K11,
    // `link_index::ensure_if_idle`). Kontrolle: ohne Besitzer wird er gebaut. Systemuhr: Heartbeat in der Zukunft.
    use os_model::{Clock, SystemClock};
    let dir = TempDir::new("rule-link-index");
    let future = ago(SystemClock.now(), -60_000);
    let mut paths = Vec::new();
    for (seed, owner) in [(900, None), (901, Some(FOREIGN_PID))] {
        let (_, path) = new_crawl(dir.path(), seed, CrawlState::Completed);
        {
            let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
            f.conn().execute_batch("DROP INDEX links_target").unwrap();
        }
        if let Some(pid) = owner {
            set_owner(&path, "mcp-stdio", pid, Some(future));
        }
        assert!(!has_link_index(&path));
        paths.push(path);
    }
    let control = CrawlFile::open(&paths[0], OpenMode::ReadWrite).unwrap();
    assert!(
        os_store::link_index::ensure_if_idle(&control).unwrap(),
        "Kontrolle"
    );
    drop(control);
    assert!(
        has_link_index(&paths[0]),
        "Kontrolle: Index ohne Besitzer gebaut"
    );

    let before = meta_dump(&paths[1]);
    let foreign = CrawlFile::open(&paths[1], OpenMode::ReadWrite).unwrap();
    let built = os_store::link_index::ensure_if_idle(&foreign).unwrap();
    drop(foreign);
    assert!(
        !built,
        "Index in die Datei eines fremden aktiven Besitzers geschrieben"
    );
    assert!(!has_link_index(&paths[1]));
    assert_eq!(meta_dump(&paths[1]), before);
}

//! M4-K11: Leseregel „aktiver Besitzer“, die Prüfungen „belegt“ (a) und „fremd geführt“ (b) in Datei, Verwaltung und
//! Aufbewahrung, sowie die Schreibfunktionen `claim`/`beat`/`release` (08 AI-020 [M4], Unterpunkt „Aktiver Besitzer
//! außerhalb von `Running`“; 01 CRAWL-125, Nachtrag K4-22). Zustandsfälle: `tests/fixtures/crawl-files/active-owner/`.
//! Ohne Wanduhr: alle Zeitpunkte sind fest bzw. kommen aus einer `ManualClock`.
mod store_common;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use os_model::{Clock, CrawlId, CrawlState, ManualClock, Timestamp};
use os_store::owner::{self, Claim};
use os_store::registry::{self, Manager};
use os_store::{CrawlFile, OpenMode, StorageProfile, StoreError};
use serde_json::Value;
use store_common::init;

fn cases() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/crawl-files/active-owner/cases.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn now(c: &Value) -> Timestamp {
    Timestamp::from_db(c["now"].as_str().unwrap()).unwrap()
}

fn own() -> String {
    std::process::id().to_string()
}

fn foreign(c: &Value) -> String {
    let pid = c["foreign_pid"].as_str().unwrap().to_string();
    assert_ne!(pid, own());
    pid
}

fn ago(now: Timestamp, ms: i64) -> String {
    Timestamp::from_unix_millis(now.unix_millis() - ms).to_db()
}

/// Legt eine Crawl-Datei im Crawl-Ordner an und setzt die Schlüssel eines Falls (simulierter Heartbeat).
fn file_for(dir: &Path, seed: u64, state: &str, case: &Value, c: &Value) -> (CrawlId, PathBuf) {
    let init = init(seed);
    let id = init.crawl_id;
    let path = registry::path_for(dir, &id);
    std::fs::create_dir_all(dir).unwrap();
    let file = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    let now = now(c);
    file.set_meta("state", state).unwrap();
    if let Some(ms) = case["heartbeat_age_ms"].as_i64() {
        file.set_meta("heartbeat_at", &ago(now, ms)).unwrap();
    }
    if let Some(ms) = case["started_age_ms"].as_i64() {
        file.set_meta("started_at", &ago(now, ms)).unwrap();
    }
    if let Some(pid) = case["owner_pid"].as_str() {
        let pid = match pid {
            "OWN" => own(),
            "FOREIGN" => foreign(c),
            other => other.to_string(),
        };
        file.set_meta("owner_pid", &pid).unwrap();
        file.set_meta("owner_kind", "cli").unwrap();
    }
    if let Some(reason) = case["end_reason"].as_str() {
        file.set_meta("end_reason", reason).unwrap();
    }
    for n in 0..case["queued"].as_i64().unwrap_or(0) {
        file.conn()
            .execute(
                "INSERT INTO queue(url_id,seq,depth,priority) VALUES (?1,?1,0,0)",
                [n + 1],
            )
            .unwrap();
    }
    (id, path)
}

/// Kriterium (a): Grenzwerte der Leseregel für `Completed` und `Paused` – Datei, Liste und zwischengespeicherter
/// Verwaltungseintrag gleich; der abgeleitete Zustand ist in allen Fällen der gespeicherte.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[test]
fn boundaries_of_the_active_owner_on_completed_and_paused_files() {
    let c = cases();
    let now = now(&c);
    let clock = ManualClock::new(now);
    let dir = tempfile::tempdir().unwrap();
    let mut expected = Vec::new();
    let mut seed = 0;
    for state in c["boundaries"]["states"].as_array().unwrap() {
        let state = state.as_str().unwrap();
        for case in c["boundaries"]["cases"].as_array().unwrap() {
            seed += 1;
            let (id, path) = file_for(dir.path(), seed, state, case, &c);
            let active = case["active"].as_bool().unwrap();
            let name = format!("{state}: {}", case["name"]);
            let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
            let occupancy = file.occupancy(&clock).unwrap();
            assert_eq!(occupancy.active_owner, active, "{name}");
            assert!(!occupancy.running, "{name}");
            assert_eq!(occupancy.occupied(), active, "{name}: belegt");
            let own_pid = case["owner_pid"] == "OWN";
            assert_eq!(occupancy.foreign_led_here(), active && !own_pid, "{name}");
            assert_eq!(
                file.effective_state(&clock).unwrap().as_str(),
                state,
                "{name}: abgeleiteter Zustand"
            );
            expected.push((id, name, active, own_pid, state.to_string()));
        }
    }
    let mut m = Manager::open(dir.path()).unwrap();
    for pass in ["Datei", "Zwischenspeicher"] {
        for (id, name, active, own_pid, state) in &expected {
            let l = m.get(id).unwrap();
            assert_eq!(l.active_owner(now).is_some(), *active, "{pass} {name}");
            assert_eq!(l.occupied(now), *active, "{pass} {name}");
            assert_eq!(l.foreign_led(now), *active && !own_pid, "{pass} {name}");
            assert_eq!(l.effective_state(now).as_str(), state, "{pass} {name}");
            assert_eq!(
                registry::effective_state(&l.meta, now).as_str(),
                state,
                "{pass} {name}"
            );
        }
    }
    // Die Liste ohne Verwaltungsdateien (M1-Lese-API) liest dieselben Felder.
    let other = tempfile::tempdir().unwrap();
    let (_, path) = file_for(
        other.path(),
        99,
        "Completed",
        &c["boundaries"]["cases"][0],
        &c,
    );
    let listing = registry::read_listing(&path).unwrap();
    assert_eq!(
        listing.meta.owner_pid.as_deref(),
        Some(foreign(&c).as_str())
    );
    assert_eq!(listing.meta.owner_kind.as_deref(), Some("cli"));
    assert!(listing.occupied(now));
}

/// Kriterium (c): Datei aus 1.0, alle Zustände – „belegt“ liefert dasselbe wie CRAWL-125 allein, „fremd geführt“
/// dasselbe wie „laufend mit fremder PID“; Fortsetzbarkeit unverändert gegenüber der Regel aus 1.0.
// spec: AI-020 [M4]
// spec: CRAWL-125
// spec: CRAWL-128
#[test]
fn files_from_one_point_zero_get_the_same_answer_as_crawl_125_alone() {
    let c = cases();
    let now = now(&c);
    let clock = ManualClock::new(now);
    let dir = tempfile::tempdir().unwrap();
    let mut rows = Vec::new();
    for (i, case) in c["v1_0"]["cases"].as_array().unwrap().iter().enumerate() {
        let state = case["state"].as_str().unwrap();
        let (id, path) = file_for(dir.path(), 100 + i as u64, state, case, &c);
        let running = case["running"].as_bool().unwrap();
        let own_pid = case["owner_pid"] == "OWN";
        let name = case["name"].to_string();
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        let crawl_125 = file.effective_state(&clock).unwrap() == CrawlState::Running;
        assert_eq!(crawl_125, running, "{name}: Fixture");
        let occupancy = file.occupancy(&clock).unwrap();
        assert_eq!(occupancy.occupied(), crawl_125, "{name}: belegt");
        assert_eq!(
            occupancy.foreign_led_here(),
            crawl_125 && !own_pid,
            "{name}: fremd geführt"
        );
        assert!(
            !occupancy.foreign_active_owner(std::process::id()) || crawl_125,
            "{name}: der neue Teil greift nur, wo CRAWL-125 schon greift"
        );
        rows.push((id, name, crawl_125, own_pid));
    }
    let mut m = Manager::open(dir.path()).unwrap();
    for (id, name, crawl_125, own_pid) in &rows {
        let l = m.get(id).unwrap();
        let listing_125 = l.effective_state(now) == CrawlState::Running;
        assert_eq!(l.occupied(now), listing_125, "{name}: belegt (Liste)");
        assert_eq!(listing_125, *crawl_125, "{name}");
        assert_eq!(l.foreign_led(now), listing_125 && !own_pid, "{name}");
        // Fortsetzbarkeit nach der Regel aus 1.0 (K2-34, CRAWL-128).
        let v1_0 = l.meta.end_reason != Some(os_model::EndReason::Stopped)
            && (matches!(
                l.effective_state(now),
                CrawlState::Paused | CrawlState::Incomplete
            ) || (l.meta.state == CrawlState::Completed && l.queued > 0));
        assert_eq!(l.can_resume(now), v1_0, "{name}: fortsetzbar");
    }
}

/// Kriterium (b), Teil Verwaltung: `Completed` mit fremdem aktivem Besitzer – Löschen, Verschieben, Exportieren,
/// Sperren und Entsperren abgewiesen, die Aufbewahrung übergeht die Datei; 11 s später ohne neuen Heartbeat gelingt das
/// Löschen. Der eigene aktive Besitzer schützt in der Verwaltung genauso (Wirkung a).
// spec: AI-020 [M4]
// spec: OUT-228
// spec: OUT-229
#[test]
fn management_rejects_an_active_owner_until_eleven_seconds_without_heartbeat() {
    let c = cases();
    let clock = ManualClock::new(now(&c));
    let dir = tempfile::tempdir().unwrap();
    let live = &c["boundaries"]["cases"][0];
    let (id, path) = file_for(dir.path(), 1, "Completed", live, &c);
    let own_case = serde_json::json!({"owner_pid":"OWN","heartbeat_age_ms":0});
    let (own_id, _) = file_for(dir.path(), 2, "Paused", &own_case, &c);
    let original = std::fs::read(&path).unwrap();
    let mut m = Manager::open(dir.path()).unwrap();
    let t = clock.now();
    for target in [id, own_id] {
        assert!(m.delete(&target, t).is_err(), "löschen");
        assert!(m.lock(&target, true, t).is_err(), "sperren");
        assert!(m.lock(&target, false, t).is_err(), "entsperren");
        assert!(m
            .export(&target, &dir.path().join("out.ospider"), t, &|_, _| true)
            .is_err());
        let other = dir.path().join("elsewhere");
        assert!(m.move_file(&target, &other, t, &|_, _| true).is_err());
    }
    assert!(!dir.path().join("out.ospider").exists());
    assert!(m
        .retain(Some(1), Some(0), t, &HashSet::new())
        .unwrap()
        .is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), original, "Datei unverändert");
    assert!(!m.get(&own_id).unwrap().foreign_led(t), "eigener Prozess");
    assert!(m.get(&own_id).unwrap().can_resume(t), "eigener Prozess");

    clock.advance(Duration::from_secs(11));
    let later = clock.now();
    let l = m.get(&id).unwrap();
    assert_eq!(l.active_owner(later), None);
    assert_eq!(l.effective_state(later), CrawlState::Completed);
    m.delete(&id, later).unwrap();
    assert!(!path.exists());
    assert_eq!(
        m.retain(Some(1), Some(0), later, &HashSet::new())
            .unwrap()
            .len(),
        1,
        "ohne aktiven Besitzer übergeht die Aufbewahrung auch den pausierten Crawl nicht mehr"
    );
}

/// Kriterium (b), Teil Fortsetzbarkeit: ein fremder aktiver Besitzer macht einen pausierten Crawl unfortsetzbar, bis
/// sein Heartbeat mehr als 10 s alt ist.
// spec: AI-020 [M4]
// spec: CRAWL-128
#[test]
fn a_foreign_active_owner_blocks_resumability_of_a_paused_crawl() {
    let c = cases();
    let now = now(&c);
    let dir = tempfile::tempdir().unwrap();
    let (id, _) = file_for(dir.path(), 1, "Paused", &c["boundaries"]["cases"][2], &c);
    let mut m = Manager::open(dir.path()).unwrap();
    let l = m.get(&id).unwrap();
    assert!(!l.can_resume(now));
    assert!(!l.can_resume(Timestamp::from_unix_millis(now.unix_millis() - 5_000)));
    assert!(l.can_resume(Timestamp::from_unix_millis(now.unix_millis() + 1)));
}

/// Ein Verwaltungseintrag aus einem Stand ohne Besitzerangaben wird neu gelesen statt als „ohne Besitzer“ zu gelten.
// spec: AI-020 [M4]
// spec: OUT-228
#[test]
fn cached_entries_without_owner_fields_are_read_again() {
    let c = cases();
    let now = now(&c);
    let dir = tempfile::tempdir().unwrap();
    let (id, path) = file_for(dir.path(), 1, "Completed", &c["boundaries"]["cases"][0], &c);
    let mut m = Manager::open(dir.path()).unwrap();
    assert!(m.get(&id).unwrap().occupied(now));
    let index = rusqlite::Connection::open(dir.path().join(".index.sqlite")).unwrap();
    let key = path.to_string_lossy().into_owned();
    let payload: String = index
        .query_row("SELECT payload FROM entries WHERE path=?1", [&key], |r| {
            r.get(0)
        })
        .unwrap();
    let mut old: serde_json::Map<String, Value> = serde_json::from_str(&payload).unwrap();
    assert!(old.remove("owner_pid").is_some() && old.remove("owner_kind").is_some());
    index
        .execute(
            "UPDATE entries SET payload=?1 WHERE path=?2",
            [Value::Object(old).to_string(), key],
        )
        .unwrap();
    drop(index);
    let l = m.get(&id).unwrap();
    assert_eq!(l.meta.owner_pid.as_deref(), Some(foreign(&c).as_str()));
    assert!(l.occupied(now));
}

/// Schreibseite: `claim` trägt ein (ohne `state`), weist einen fremden aktiven Besitzer mit Art und PID ab, ist bei
/// eigenem laufendem Crawl passiv und überschreibt einen veralteten Eintrag; `beat` und `release` schreiben nur für
/// den eigenen Prozess, `release` lässt `owner_kind` stehen.
// spec: AI-020 [M4]
// spec: CRAWL-125
#[test]
fn claim_beat_and_release_write_only_owner_fields_in_one_commit_each() {
    let c = cases();
    let clock = ManualClock::new(now(&c));
    let dir = tempfile::tempdir().unwrap();
    let none = serde_json::json!({"owner_pid": "", "heartbeat_age_ms": 60_000});
    let (_, path) = file_for(dir.path(), 1, "Completed", &none, &c);
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let me = std::process::id();
    let meta = |f: &CrawlFile, k: &str| f.get_meta(k).unwrap().unwrap_or_default();

    let t0 = clock.now();
    assert_eq!(
        owner::claim(file.conn(), "app", me, t0).unwrap(),
        Claim::Claimed
    );
    assert_eq!(meta(&file, "owner_kind"), "app");
    assert_eq!(meta(&file, "owner_pid"), me.to_string());
    assert_eq!(meta(&file, "heartbeat_at"), t0.to_db());
    assert_eq!(meta(&file, "state"), "Completed");

    // Ein zweiter Prozess (simuliert über eine fremde PID) wird mit Art und PID abgewiesen.
    let other = me.wrapping_add(1);
    let err = owner::claim(file.conn(), "cli", other, t0).unwrap_err();
    let StoreError::InvalidInput(text) = &err else {
        panic!("{err:?}");
    };
    assert!(
        text.contains(&me.to_string()) && text.contains("app") && text.contains("läuft noch"),
        "{text}"
    );
    assert!(!owner::beat(file.conn(), other, t0).unwrap());
    assert!(!owner::release(file.conn(), other, t0).unwrap());
    assert_eq!(meta(&file, "owner_pid"), me.to_string());

    clock.advance(Duration::from_secs(2));
    assert!(owner::beat(file.conn(), me, clock.now()).unwrap());
    assert_eq!(meta(&file, "heartbeat_at"), clock.now().to_db());

    clock.advance(Duration::from_millis(500));
    let end = clock.now();
    assert!(owner::release(file.conn(), me, end).unwrap());
    assert_eq!(meta(&file, "owner_pid"), "");
    assert_eq!(
        meta(&file, "owner_kind"),
        "app",
        "nennt den letzten Besitzer"
    );
    assert_eq!(meta(&file, "heartbeat_at"), end.to_db());
    assert_eq!(meta(&file, "state"), "Completed");
    assert!(
        !owner::beat(file.conn(), me, end).unwrap(),
        "nach dem Ende kein Heartbeat"
    );

    // Absturz eines fremden KI-Laufs: mehr als 10 s nach dem letzten Heartbeat überschreibt der nächste Lauf.
    file.set_meta("owner_pid", &other.to_string()).unwrap();
    file.set_meta("owner_kind", "mcp-stdio").unwrap();
    file.set_meta("heartbeat_at", &end.to_db()).unwrap();
    assert!(owner::claim(file.conn(), "app", me, end).is_err());
    clock.advance(Duration::from_millis(10_001));
    assert_eq!(
        owner::claim(file.conn(), "app", me, clock.now()).unwrap(),
        Claim::Claimed
    );
    assert_eq!(meta(&file, "owner_pid"), me.to_string());

    // Eigener laufender Crawl: passiv, nichts geschrieben; `beat` und `release` lassen die Engine-Angaben stehen.
    let running = serde_json::json!({"owner_pid": "OWN", "heartbeat_age_ms": 0});
    let (_, path) = file_for(dir.path(), 2, "Running", &running, &c);
    let engine = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let before = meta(&engine, "heartbeat_at");
    let t = now(&c);
    let later = Timestamp::from_unix_millis(t.unix_millis() + 1_000);
    assert_eq!(
        owner::claim(engine.conn(), "app", me, later).unwrap(),
        Claim::Passive
    );
    assert!(!owner::beat(engine.conn(), me, later).unwrap());
    assert!(!owner::release(engine.conn(), me, later).unwrap());
    assert_eq!(meta(&engine, "heartbeat_at"), before);
    assert_eq!(meta(&engine, "owner_pid"), me.to_string());
    assert_eq!(meta(&engine, "owner_kind"), "cli");
    // Ein fremd laufender Crawl (CRAWL-125) ist für `claim` fremd geführt.
    engine.set_meta("owner_pid", &other.to_string()).unwrap();
    assert!(owner::claim(engine.conn(), "app", me, later).is_err());
}

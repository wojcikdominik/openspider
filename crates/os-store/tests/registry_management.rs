//! M2-O02: generierte Crawl-Dateien, drei Projekte, fünf Zustände und portable Kopien.
mod store_common;
use os_model::{CrawlId, CrawlState, ManualClock, SeededIds, Timestamp};
use os_store::registry::{self, Manager};
use os_store::{CrawlFile, CrawlSink, OpenMode, StorageProfile};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use store_common::*;

fn now() -> Timestamp {
    Timestamp::from_db("2026-10-18T12:00:00.000Z").unwrap()
}
fn crawl(dir: &Path, seed: u64, state: CrawlState) -> (CrawlId, PathBuf) {
    let init = init(seed);
    let id = init.crawl_id;
    let path = registry::path_for(dir, &id);
    std::fs::create_dir_all(dir).unwrap();
    let file = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    file.set_meta("state", state.as_str()).unwrap();
    file.set_meta("heartbeat_at", &now().to_db()).unwrap();
    (id, path)
}

// spec: OUT-228
#[test]
fn projects_and_locks_are_persistent_and_inherited_by_future_crawls() {
    let dir = tempfile::tempdir().unwrap();
    let (id, path) = crawl(dir.path(), 1, CrawlState::Paused);
    let mut m = Manager::open(dir.path()).unwrap();
    let p = m.create_project("customer-a", "Kunde A").unwrap();
    m.create_project("customer-b", "Kunde B").unwrap();
    m.move_project(&id, Some(&p.id), now()).unwrap();
    m.rename(&id, "Neuer Name", now()).unwrap();
    m.lock_project(&p.id, true).unwrap();
    assert!(m.get(&id).unwrap().locked);
    assert!(m.rename(&id, "Verboten", now()).is_err());
    assert!(m.delete(&id, now()).is_err());
    let (new_id, new_path) = crawl(dir.path(), 2, CrawlState::Completed);
    let new_file = CrawlFile::open(&new_path, OpenMode::ReadWrite).unwrap();
    new_file.set_meta("project", "Kunde A").unwrap();
    drop(new_file);
    assert!(m.get(&new_id).unwrap().locked);
    assert!(
        m.list()
            .unwrap()
            .crawls
            .iter()
            .find(|l| l.meta.crawl_id == new_id)
            .unwrap()
            .locked
    );
    m.lock_project(&p.id, false).unwrap();
    m.lock(&id, true, now()).unwrap();
    assert!(!m.get(&new_id).unwrap().locked);
    m.rename_project(&p.id, "Kunde C", now()).unwrap();
    drop(m);
    // Der Index darf verworfen werden; Projekte, Sperren und Standorte bleiben erhalten.
    std::fs::remove_file(dir.path().join(".index.sqlite")).unwrap();
    let mut reopened = Manager::open(dir.path()).unwrap();
    assert!(reopened.get(&id).unwrap().locked);
    let list = registry::list_crawls(dir.path()).unwrap();
    let original = list.iter().find(|l| l.meta.crawl_id == id).unwrap();
    assert_eq!(original.meta.name, "Neuer Name");
    assert_eq!(original.meta.project, "Kunde C");
    assert!(path.exists());
    assert_eq!(
        reopened
            .projects()
            .unwrap()
            .iter()
            .find(|p| p.id == "customer-a")
            .unwrap()
            .crawls,
        2
    );
}

// spec: CRAWL-128
#[test]
fn five_states_heartbeat_age_and_limit_queue_determine_resumability() {
    let dir = tempfile::tempdir().unwrap();
    let mut ids = Vec::new();
    for (i, state) in [
        CrawlState::Running,
        CrawlState::Paused,
        CrawlState::Completed,
        CrawlState::Incomplete,
        CrawlState::Failed,
    ]
    .into_iter()
    .enumerate()
    {
        ids.push(crawl(dir.path(), i as u64, state).0);
    }
    let mut m = Manager::open(dir.path()).unwrap();
    assert!(!m.get(&ids[0]).unwrap().can_resume(now()));
    let later = Timestamp::from_unix_millis(now().unix_millis() + 10_001);
    assert!(m.get(&ids[0]).unwrap().can_resume(later));
    assert_eq!(
        registry::effective_state(&m.get(&ids[0]).unwrap().meta, later),
        CrawlState::Incomplete
    );
    for (id, expected) in ids[1..].iter().zip([true, false, true, false]) {
        assert_eq!(m.get(id).unwrap().can_resume(now()), expected);
    }
    let file = CrawlFile::open(
        &registry::path_for(dir.path(), &ids[2]),
        OpenMode::ReadWrite,
    )
    .unwrap();
    file.conn()
        .execute(
            "INSERT INTO queue(url_id,seq,depth,priority) VALUES (1,1,0,0)",
            [],
        )
        .unwrap();
    assert!(m.get(&ids[2]).unwrap().can_resume(now()));
}

// spec: CRAWL-128
// spec: AGENT-020
#[test]
fn a_stopped_crawl_is_never_resumable_whatever_its_queue_holds() {
    let dir = tempfile::tempdir().unwrap();
    let (stopped, stopped_path) = crawl(dir.path(), 11, CrawlState::Completed);
    let (limited, limited_path) = crawl(dir.path(), 12, CrawlState::Completed);
    for (path, reason) in [(&stopped_path, "stopped"), (&limited_path, "limit_reached")] {
        let file = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
        file.set_meta("end_reason", reason).unwrap();
        // Rest in der Warteschlange: ohne Endgrund `stopped` wäre der Crawl fortsetzbar (Limit, K2-34).
        file.conn()
            .execute(
                "INSERT INTO queue(url_id,seq,depth,priority) VALUES (1,1,0,0)",
                [],
            )
            .unwrap();
    }
    let mut m = Manager::open(dir.path()).unwrap();
    let listed = m.get(&stopped).unwrap();
    assert_eq!(listed.queued, 1);
    assert_eq!(listed.meta.end_reason, Some(os_model::EndReason::Stopped));
    assert!(!listed.can_resume(now()), "Gestoppt ist endgültig");
    assert!(m.get(&limited).unwrap().can_resume(now()), "Limit mit Rest");
    // Auch ohne Verwaltungsindex (CLI liest die Datei direkt) gilt dieselbe Regel.
    let direct = registry::read_listing(&stopped_path).unwrap();
    assert!(!direct.can_resume(now()));
    // Ein gestoppter Crawl mit veraltetem Zustand (Absturz nach dem Stopp-Commit ist ausgeschlossen, Datei von Hand
    // verändert) bleibt ebenfalls gesperrt: der Endgrund entscheidet, nicht der Zustand.
    let file = CrawlFile::open(&stopped_path, OpenMode::ReadWrite).unwrap();
    file.set_meta("state", "Paused").unwrap();
    drop(file);
    assert!(!registry::read_listing(&stopped_path)
        .unwrap()
        .can_resume(now()));
}

// Sperren schreibt in die Datei; ein laufender Crawl gehört seinem Besitzer wie bei Umbenennen und Löschen.
// spec: OUT-228
// spec: AGENT-020
#[test]
fn a_running_crawl_cannot_be_locked_or_unlocked_until_it_is_no_longer_running() {
    let dir = tempfile::tempdir().unwrap();
    let (id, _path) = crawl(dir.path(), 21, CrawlState::Running);
    let mut m = Manager::open(dir.path()).unwrap();
    for locked in [true, false] {
        let err = m.lock(&id, locked, now()).unwrap_err().to_string();
        assert!(err.contains("Crawl läuft"), "{err}");
    }
    assert!(!m.get(&id).unwrap().locked);
    // Ohne Heartbeat gilt er als unterbrochen (CRAWL-125) und lässt sich sperren.
    let later = Timestamp::from_unix_millis(now().unix_millis() + 10_001);
    m.lock(&id, true, later).unwrap();
    assert!(m.get(&id).unwrap().locked);
}

// spec: CRAWL-143
#[test]
fn paused_duplicate_and_export_preserve_queue_config_and_source() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = crawl(dir.path(), 3, CrawlState::Paused);
    {
        let mut sink = sink(&path, StorageProfile::Balanced);
        sink.write(batch_with_urls([(1, true), (2, true)])).unwrap();
        sink.finish().unwrap();
    }
    let source = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    source.set_meta("state", "Paused").unwrap();
    source
        .conn()
        .execute(
            "INSERT OR REPLACE INTO queue(url_id,seq,depth,priority) VALUES (1,1,0,0)",
            [],
        )
        .unwrap();
    drop(source);
    let before = std::fs::read(&path).unwrap();
    let mut m = Manager::open(dir.path()).unwrap();
    let copy = m
        .duplicate(
            &id,
            dir.path(),
            &SeededIds::new(9),
            &ManualClock::new(now()),
            &|_, _| true,
        )
        .unwrap();
    let duplicated = registry::read_listing(&copy).unwrap();
    assert_ne!(duplicated.meta.crawl_id, id);
    assert_eq!(duplicated.meta.name, "Testcrawl (Kopie)");
    assert!(duplicated.can_resume(now()));
    assert_eq!(duplicated.queued, 1);
    let exported = out.path().join("paused.ospider");
    m.export(&id, &exported, now(), &|_, _| true).unwrap();
    let original = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let exported = CrawlFile::open(&exported, OpenMode::ReadOnly).unwrap();
    let copied = CrawlFile::open(&copy, OpenMode::ReadOnly).unwrap();
    assert_eq!(
        original.resume_state().unwrap(),
        copied.resume_state().unwrap()
    );
    assert_eq!(original.config().unwrap(), exported.config().unwrap());
    drop(original);
    drop(exported);
    drop(copied);
    m.rename(&duplicated.meta.crawl_id, "Eigenständig", now())
        .unwrap();
    m.delete(&duplicated.meta.crawl_id, now()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

// spec: OUT-228
#[test]
fn running_crawls_reject_destructive_actions_and_export() {
    let dir = tempfile::tempdir().unwrap();
    let (id, path) = crawl(dir.path(), 5, CrawlState::Running);
    let mut m = Manager::open(dir.path()).unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(m.delete(&id, now()).is_err());
    assert!(m.rename(&id, "x", now()).is_err());
    assert!(m.move_project(&id, None, now()).is_err());
    assert!(m
        .export(&id, &dir.path().join("out.ospider"), now(), &|_, _| true)
        .is_err());
    assert!(m
        .retain(Some(1), Some(0), now(), &HashSet::new())
        .unwrap()
        .is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

// spec: CRAWL-124 [M2]
#[test]
fn location_change_keeps_both_folders_and_move_is_verified_before_source_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let (id, path) = crawl(dir.path(), 6, CrawlState::Paused);
    let mut m = Manager::open(dir.path()).unwrap();
    m.add_location(other.path()).unwrap();
    let (new_id, _) = crawl(other.path(), 7, CrawlState::Completed);
    assert!(path.exists());
    assert_eq!(m.list().unwrap().crawls.len(), 2);
    let collision = registry::path_for(other.path(), &id);
    std::fs::write(&collision, "must survive").unwrap();
    assert!(m.move_file(&id, other.path(), now(), &|_, _| true).is_err());
    assert!(path.exists());
    assert_eq!(std::fs::read_to_string(&collision).unwrap(), "must survive");
    std::fs::remove_file(&collision).unwrap();
    let dest = m.move_file(&id, other.path(), now(), &|_, _| true).unwrap();
    assert!(!path.exists());
    assert!(dest.exists());
    drop(m);
    assert_eq!(registry::find(dir.path(), &id), Some(dest));
    assert!(registry::find(dir.path(), &new_id).is_some());
}

// spec: CRAWL-127 [M2]
#[test]
fn import_rejects_foreign_files_and_rekeys_collisions_without_altering_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let (id, path) = crawl(source.path(), 8, CrawlState::Paused);
    let before = std::fs::read(&path).unwrap();
    let mut m = Manager::open(dir.path()).unwrap();
    let ids = SeededIds::new(88);
    let clock = ManualClock::new(now());
    let one = m
        .import(&path, dir.path(), &ids, &clock, &|_, _| true)
        .unwrap();
    let two = m
        .import(&path, dir.path(), &ids, &clock, &|_, _| true)
        .unwrap();
    assert_eq!(registry::read_listing(&one).unwrap().meta.crawl_id, id);
    assert_ne!(registry::read_listing(&two).unwrap().meta.crawl_id, id);
    let fake = source.path().join("fake.ospider");
    std::fs::write(&fake, "not a crawl").unwrap();
    assert!(m
        .import(&fake, dir.path(), &ids, &clock, &|_, _| true)
        .is_err());
    assert_eq!(m.list().unwrap().crawls.len(), 2);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

// spec: OUT-229
#[test]
fn retention_uses_end_or_last_change_and_protects_open_running_and_locked() {
    let dir = tempfile::tempdir().unwrap();
    let mut ids = Vec::new();
    for seed in 0..6 {
        ids.push(crawl(
            dir.path(),
            seed,
            if seed == 5 {
                CrawlState::Running
            } else {
                CrawlState::Completed
            },
        ));
    }
    for (i, (_, path)) in ids.iter().enumerate() {
        let f = CrawlFile::open(path, OpenMode::ReadWrite).unwrap();
        let days = if i == 1 { 6 } else { 8 };
        f.set_meta(
            "ended_at",
            &Timestamp::from_unix_millis(now().unix_millis() - days * 86_400_000).to_db(),
        )
        .unwrap();
    }
    let mut m = Manager::open(dir.path()).unwrap();
    m.lock(&ids[2].0, true, now()).unwrap();
    m.create_project("protected", "Geschützt").unwrap();
    m.move_project(&ids[3].0, Some("protected"), now()).unwrap();
    m.lock_project("protected", true).unwrap();
    let removed = m
        .retain(Some(7), None, now(), &HashSet::from([ids[4].0]))
        .unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].id, ids[0].0);
    assert_eq!(removed[0].age_days, 8);
    assert_eq!(m.list().unwrap().crawls.len(), 5);
    assert!(m
        .retain(None, None, now(), &HashSet::new())
        .unwrap()
        .is_empty());
}

// spec: OUT-229
#[test]
fn retention_of_paused_crawl_uses_last_modification_instead_of_old_end_time() {
    let dir = tempfile::tempdir().unwrap();
    let (id, path) = crawl(dir.path(), 16, CrawlState::Paused);
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    f.set_meta("ended_at", "2000-01-01T00:00:00.000Z").unwrap();
    drop(f);
    let recent = std::time::UNIX_EPOCH
        + std::time::Duration::from_millis((now().unix_millis() - 6 * 86_400_000) as u64);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(recent)
        .unwrap();
    let mut m = Manager::open(dir.path()).unwrap();
    assert!(m
        .retain(Some(7), None, now(), &HashSet::new())
        .unwrap()
        .is_empty());
    let later = Timestamp::from_unix_millis(now().unix_millis() + 2 * 86_400_000);
    assert_eq!(
        m.retain(Some(7), None, later, &HashSet::new()).unwrap()[0].id,
        id
    );
}

// spec: OUT-228
#[test]
fn index_invalidates_on_wal_changes_and_own_generation_without_caching_heartbeat_age() {
    let dir = tempfile::tempdir().unwrap();
    let (id, path) = crawl(dir.path(), 17, CrawlState::Running);
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.conn()
        .execute_batch("PRAGMA wal_autocheckpoint=0")
        .unwrap();
    let mut m = Manager::open(dir.path()).unwrap();
    assert_eq!(m.get(&id).unwrap().meta.name, "Testcrawl");
    let main = std::fs::read(&path).unwrap();
    file.set_meta("name", "WAL-only change").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), main);
    assert_eq!(m.get(&id).unwrap().meta.name, "WAL-only change");
    let count = m.index.generation().unwrap();
    m.index.invalidate(&path).unwrap();
    assert_eq!(m.index.generation().unwrap(), count + 1);
    assert!(m
        .get(&id)
        .unwrap()
        .can_resume(Timestamp::from_unix_millis(now().unix_millis() + 20_000)));
    drop(m);
    let mut reopened = Manager::open(dir.path()).unwrap();
    assert_eq!(reopened.get(&id).unwrap().meta.name, "WAL-only change");
}

// spec: UI-147
#[test]
fn cancelled_import_publishes_no_partial_file_and_does_not_overwrite_destinations() {
    let dir = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let (_, path) = crawl(src.path(), 20, CrawlState::Paused);
    let mut m = Manager::open(dir.path()).unwrap();
    assert!(m
        .import(
            &path,
            dir.path(),
            &SeededIds::new(99),
            &ManualClock::new(now()),
            &|_, _| false
        )
        .is_err());
    assert!(m.list().unwrap().crawls.is_empty());
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|e| e
        .unwrap()
        .path()
        .to_string_lossy()
        .ends_with(".copying")));
}

// spec: OUT-228
#[test]
fn listing_scans_only_explicit_locations_without_descending_into_hidden_or_deep_folders() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let (root_id, _) = crawl(root.path(), 31, CrawlState::Completed);
    let (external_id, _) = crawl(external.path(), 32, CrawlState::Completed);
    for (i, base) in [root.path(), external.path()].into_iter().enumerate() {
        crawl(&base.join("child"), 40 + i as u64, CrawlState::Completed);
        crawl(
            &base.join("child").join("deep"),
            50 + i as u64,
            CrawlState::Completed,
        );
        crawl(&base.join(".hidden"), 60 + i as u64, CrawlState::Completed);
    }
    let mut manager = Manager::open(root.path()).unwrap();
    manager.add_location(external.path()).unwrap();
    let list = manager.list().unwrap();
    assert!(list.skipped.is_empty());
    let listed: HashSet<_> = list.crawls.iter().map(|l| l.meta.crawl_id).collect();
    assert_eq!(listed, HashSet::from([root_id, external_id]));
    assert!(!external.path().join(".registry.sqlite").exists());
    assert!(!external.path().join(".index.sqlite").exists());
}

// spec: CRAWL-128
#[test]
fn file_and_cached_listing_share_heartbeat_boundaries_and_fallbacks() {
    let dir = tempfile::tempdir().unwrap();
    let (_, path) = crawl(dir.path(), 70, CrawlState::Running);
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    for (heartbeat, started, created, reference) in [
        (
            Some(now()),
            Some(created_at()),
            Some(created_at()),
            Some(now()),
        ),
        (None, Some(now()), Some(created_at()), Some(now())),
        (None, None, Some(now()), Some(now())),
        (None, None, None, None),
    ] {
        for (key, value) in [
            ("heartbeat_at", heartbeat),
            ("started_at", started),
            ("created_at", created),
        ] {
            match value {
                Some(t) => file.set_meta(key, &t.to_db()).unwrap(),
                None => {
                    file.conn()
                        .execute("DELETE FROM crawl_meta WHERE key=?1", [key])
                        .unwrap();
                }
            }
        }
        let listing = registry::read_listing(&path).unwrap();
        for delta in [-1, 0, 9_999, 10_000, 10_001] {
            let time = Timestamp::from_unix_millis(now().unix_millis() + delta);
            let expected = if reference.is_some() && delta <= 10_000 {
                CrawlState::Running
            } else {
                CrawlState::Incomplete
            };
            assert_eq!(listing.effective_state(time), expected);
            assert_eq!(
                file.effective_state(&ManualClock::new(time)).unwrap(),
                expected
            );
        }
    }
}

// spec: CRAWL-127 [M2]
#[test]
fn import_to_external_storage_keeps_management_databases_under_registry_root() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let (_, path) = crawl(source.path(), 80, CrawlState::Paused);
    let mut manager = Manager::open(root.path()).unwrap();
    let imported = manager
        .import(
            &path,
            external.path(),
            &SeededIds::new(81),
            &ManualClock::new(now()),
            &|_, _| true,
        )
        .unwrap();
    assert_eq!(imported.parent(), Some(external.path()));
    assert_eq!(manager.list().unwrap().crawls.len(), 1);
    drop(manager);
    assert!(root.path().join(".registry.sqlite").exists());
    assert!(root.path().join(".index.sqlite").exists());
    assert!(!external.path().join(".registry.sqlite").exists());
    assert!(!external.path().join(".index.sqlite").exists());
    assert_eq!(
        Manager::open(root.path()).unwrap().list().unwrap().crawls[0].path,
        imported
    );
}

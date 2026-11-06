//! Crawl-Datei eines lebenden Besitzers neben der Crawl-Verwaltung eines anderen Prozesses (CRAWL-125, OUT-228, AGENT-020).
//!
//! SQLite erkennt am Dateisperren-Zustand der Hauptdatei, ob eine Verbindung die letzte ist; dann führt sie beim Schließen
//! das WAL zurück und löscht `-wal`/`-shm`. Der Besitzer hält dafür eine gemeinsame Sperre. POSIX gibt jedoch alle
//! Sperren eines Prozesses auf eine Datei frei, sobald dieser Prozess **irgendeinen** Deskriptor dieser Datei schließt.
//! Deshalb prüfen diese Tests beides: Der Besitzerprozess öffnet die Datei im Betrieb erneut (wie Analyse-Worker,
//! `crawl_status` und die Verwaltung im eigenen Prozess), und ein zweiter, echter Prozess liest die Metadaten.

mod store_common;

use std::path::Path;
use std::process::Command;

use os_store::registry::{self, Manager};
use os_store::{CrawlFile, OpenMode, StorageProfile};
use store_common::*;

const CHILD_FILE: &str = "OS_STORE_WAL_TEST_FILE";
const CHILD_DIR: &str = "OS_STORE_WAL_TEST_DIR";
const CHILD_WRITE: &str = "OS_STORE_WAL_TEST_WRITE";

/// Rolle des zweiten Prozesses: liest wie die Crawl-Verwaltung und beendet sich. Ohne Umgebungsvariable keine Wirkung.
#[test]
fn second_process_role() {
    if let Some(file) = std::env::var_os(CHILD_FILE) {
        registry::read_listing(Path::new(&file)).unwrap();
    }
    if let Some(dir) = std::env::var_os(CHILD_DIR) {
        Manager::open(Path::new(&dir)).unwrap().list().unwrap();
    }
    // Wie die Verwaltung vor dem Fix und wie jede fremde Schreibverbindung: Sie räumt beim Schließen auf, wenn sie sich für
    // die letzte hält.
    if let Some(file) = std::env::var_os(CHILD_WRITE) {
        let conn = rusqlite::Connection::open_with_flags(
            &file,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        let _: i64 = conn
            .query_row("SELECT COUNT(*) FROM crawl_meta", [], |r| r.get(0))
            .unwrap();
    }
}

fn in_second_process(var: &str, value: &Path) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "second_process_role", "--nocapture"])
        .env(var, value)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn companion(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

/// Besitzer mit nicht zurückgeführten WAL-Frames.
fn owner(dir: &Path) -> (CrawlFile, std::path::PathBuf) {
    // Dateiname wie im Crawl-Ordner (`<id>.ospider`), sonst überginge die Verwaltung die Datei.
    let name = registry::file_name(&init(42).crawl_id);
    let path = new_crawl(dir, &name, StorageProfile::Balanced);
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.conn()
        .execute_batch("PRAGMA wal_autocheckpoint=0")
        .unwrap();
    file.set_meta("probe", "before").unwrap();
    assert!(std::fs::metadata(companion(&path, "-wal")).unwrap().len() > 0);
    (file, path)
}

/// Der Besitzer schreibt weiter; alles muss bei einem neuen Leser ankommen, das WAL ist noch dasselbe.
fn assert_owner_intact(file: CrawlFile, path: &Path) {
    assert!(
        companion(path, "-wal").exists() && companion(path, "-shm").exists(),
        "-wal/-shm eines lebenden Besitzers bleiben liegen"
    );
    file.set_meta("probe", "after").unwrap();
    let reader = CrawlFile::open(path, OpenMode::ReadOnly).unwrap();
    assert_eq!(
        reader.get_meta("probe").unwrap().as_deref(),
        Some("after"),
        "Commit des Besitzers ging verloren"
    );
    drop(reader);
    drop(file);
    assert!(CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .integrity_check()
        .unwrap());
}

// spec: CRAWL-125
// spec: OUT-228
#[test]
fn another_process_listing_keeps_the_wal_of_a_live_owner() {
    let dir = tempfile::tempdir().unwrap();
    let (file, path) = owner(dir.path());
    in_second_process(CHILD_FILE, &path);
    assert_owner_intact(file, &path);
}

// spec: CRAWL-125
// spec: OUT-228
// spec: AGENT-020
#[test]
fn another_process_listing_keeps_the_wal_after_the_owner_process_reopened_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let (file, path) = owner(dir.path());
    // Weg der Analyse-Worker, von `crawl_status` und `--json-progress`: zweite Verbindung im selben Prozess.
    drop(CrawlFile::open(&path, OpenMode::ReadOnly).unwrap());
    in_second_process(CHILD_FILE, &path);
    assert_owner_intact(file, &path);
}

// spec: CRAWL-125
// spec: OUT-228
// spec: AGENT-020
#[test]
fn another_process_listing_keeps_the_wal_after_the_owner_process_listed_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (file, path) = owner(dir.path());
    // Weg der App und von MCP `list_crawls`: Verwaltung (Fingerprint und Metadaten) im Prozess des Besitzers.
    let listed = Manager::open(dir.path()).unwrap().list().unwrap();
    assert_eq!(listed.crawls.len(), 1, "{:?}", listed.skipped);
    registry::read_listing(&path).unwrap();
    // Ein Listenlauf ohne Eintrag im Index öffnet die Datei; `read_listing` ist der Weg ohne Verwaltungsordner (`--list-crawls`).
    in_second_process(CHILD_FILE, &path);
    in_second_process(CHILD_DIR, dir.path());
    assert_owner_intact(file, &path);
}

// Ursache 1 für sich: Der Besitzerprozess behält seine Sperren, auch wenn der andere Prozess absichtlich mit Schreibrechten
// öffnet und schließt. Grün nur durch den Lesepfad des Kopfs, nicht durch die schreibgeschützte Verwaltung.
// spec: CRAWL-125
// spec: OUT-228
// spec: AGENT-020
#[test]
fn the_owner_process_keeps_its_locks_when_it_reads_or_lists_its_own_file() {
    let dir = tempfile::tempdir().unwrap();
    let (file, path) = owner(dir.path());
    // Alle Wege im Besitzerprozess: Öffnen, `read_listing`, Verwaltung (Fingerprint), Prüfung.
    drop(CrawlFile::open(&path, OpenMode::ReadOnly).unwrap());
    registry::read_listing(&path).unwrap();
    let listed = Manager::open(dir.path()).unwrap().list().unwrap();
    assert_eq!(listed.crawls.len(), 1, "{:?}", listed.skipped);
    os_store::files::verify(&path).unwrap();
    in_second_process(CHILD_WRITE, &path);
    assert_owner_intact(file, &path);
}

// spec: OUT-228
// spec: CRAWL-125
#[test]
fn reading_metadata_never_writes_or_merges_a_foreign_wal() {
    let dir = tempfile::tempdir().unwrap();
    let (file, path) = owner(dir.path());
    let main_before = std::fs::read(&path).unwrap();
    let wal_len = std::fs::metadata(companion(&path, "-wal")).unwrap().len();
    in_second_process(CHILD_FILE, &path);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        main_before,
        "kein Checkpoint"
    );
    assert_eq!(
        std::fs::metadata(companion(&path, "-wal")).unwrap().len(),
        wal_len
    );
    assert_owner_intact(file, &path);
}

// spec: OUT-228
#[test]
fn listing_a_finished_crawl_leaves_no_companion_files_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = new_crawl(dir.path(), "crawl.ospider", StorageProfile::Balanced);
    let before = std::fs::read(&path).unwrap();
    in_second_process(CHILD_FILE, &path);
    registry::read_listing(&path).unwrap();
    assert!(!companion(&path, "-wal").exists());
    assert!(!companion(&path, "-shm").exists());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

// spec: CRAWL-125
// spec: OUT-228
#[test]
fn metadata_of_a_crashed_owner_are_read_from_its_wal_without_touching_it() {
    let dir = tempfile::tempdir().unwrap();
    let (file, path) = owner(dir.path());
    // Absturzbild: die letzten Commits stehen nur in der `-wal`-Datei (Verbindung nicht ordentlich geschlossen).
    std::mem::forget(file);
    let wal_len = std::fs::metadata(companion(&path, "-wal")).unwrap().len();
    in_second_process(CHILD_FILE, &path);
    assert_eq!(
        std::fs::metadata(companion(&path, "-wal")).unwrap().len(),
        wal_len,
        "die Frames bleiben für den nächsten Schreiber liegen"
    );
    let listing = registry::read_listing(&path).unwrap();
    assert_eq!(listing.meta.name, "Testcrawl");
}

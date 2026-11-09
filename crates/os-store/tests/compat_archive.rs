use os_store::{CrawlFile, OpenMode, StoreError, SCHEMA_VERSION};
use std::{fs, path::PathBuf};

fn archive() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/migrations")
}

// spec: OUT-226 [M3]
#[test]
fn every_schema_opens_and_migrates_with_an_unchanged_backup() {
    assert_eq!(SCHEMA_VERSION, 5, "Add a fixture when changing the schema");
    for version in 1..=5 {
        let source = archive().join(format!("schema-v{version}/crawl.ospider"));
        let bytes = fs::read(&source).unwrap();
        assert!(bytes.len() <= 1024 * 1024);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crawl.ospider");
        fs::write(&path, &bytes).unwrap();
        let old = CrawlFile::open_with(
            &path,
            OpenMode::ReadOnly,
            &os_store::MIGRATIONS[..version as usize],
        )
        .unwrap();
        assert_eq!(old.meta().unwrap().schema_version, version);
        drop(old);
        let current = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        assert_eq!(current.meta().unwrap().schema_version, SCHEMA_VERSION);
        assert_eq!(current.config().unwrap().schema_version, 2);
        let count: i64 = current
            .conn()
            .query_row("SELECT count(*) FROM urls", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        if version >= 3 {
            let fact: i64 = current
                .conn()
                .query_row(
                    "SELECT v_int FROM url_ext WHERE url_id=1 AND key='head.title_outside'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(fact, 1);
        }
        if version < 5 {
            let backup = os_store::open::backup_path(&path, version);
            assert_eq!(fs::read(backup).unwrap(), bytes);
        }
        assert_eq!(fs::read(source).unwrap(), bytes);
    }
}

// spec: OUT-226 [M3]
#[test]
fn future_schema_is_rejected_without_changing_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("future.ospider");
    fs::copy(archive().join("schema-v5/crawl.ospider"), &path).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 999).unwrap();
    drop(conn);
    let bytes = fs::read(&path).unwrap();
    let error = CrawlFile::open(&path, OpenMode::ReadWrite).err().unwrap();
    assert!(
        matches!(
            error,
            StoreError::NewerVersion {
                found: 999,
                supported: SCHEMA_VERSION,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

// spec: OUT-255
#[test]
fn archived_auth_profile_decrypts_with_test_key_only() {
    let bytes = fs::read(archive().join("app-v1/profile.osauth")).unwrap();
    let profile = os_store::auth_profiles::open(&bytes, None, |key_ref| {
        assert_eq!(key_ref, "compat-test");
        Some("11".repeat(32))
    })
    .unwrap();
    assert_eq!(profile["username"], "fixture");
    assert_eq!(profile["password"], "test-only");
    assert!(os_store::auth_profiles::open(&bytes, None, |_| Some("22".repeat(32))).is_err());
}

/// WAL-Prüfsumme (Format mit Big-Endian-Wörtern, Magic 0x377f0683): je 8 Bytes `s0 += x0 + s1; s1 += x1 + s0`.
fn wal_checksum(bytes: &[u8], (mut s0, mut s1): (u32, u32)) -> (u32, u32) {
    let (words, rest) = bytes.as_chunks::<8>();
    assert!(rest.is_empty());
    for word in words {
        let x0 = u32::from_be_bytes(word[..4].try_into().unwrap());
        let x1 = u32::from_be_bytes(word[4..].try_into().unwrap());
        s0 = s0.wrapping_add(x0).wrapping_add(s1);
        s1 = s1.wrapping_add(x1).wrapping_add(s0);
    }
    (s0, s1)
}

/// Eine gültige `-wal` mit Seitengröße `page_size` und einem einzigen Commit-Frame für Seite `pgno`, Inhalt `page`.
fn foreign_wal(page_size: u32, pgno: u32, db_pages: u32, page: &[u8]) -> Vec<u8> {
    assert_eq!(page.len(), page_size as usize);
    let (salt1, salt2) = (0x0102_0304u32, 0x0506_0708u32);
    let mut wal = Vec::new();
    for v in [0x377f_0683, 3_007_000, page_size, 0, salt1, salt2] {
        wal.extend_from_slice(&u32::to_be_bytes(v));
    }
    let sum = wal_checksum(&wal, (0, 0));
    wal.extend_from_slice(&sum.0.to_be_bytes());
    wal.extend_from_slice(&sum.1.to_be_bytes());
    let mut head = Vec::new();
    head.extend_from_slice(&pgno.to_be_bytes());
    head.extend_from_slice(&db_pages.to_be_bytes());
    let sum = wal_checksum(&[head.as_slice(), page].concat(), sum);
    for v in [salt1, salt2, sum.0, sum.1] {
        head.extend_from_slice(&v.to_be_bytes());
    }
    wal.extend_from_slice(&head);
    wal.extend_from_slice(page);
    wal
}

/// Füllt freigegebenen Heap-Speicher mit einer Markierung (Seiten-Cache einer Speicherdatenbank), damit ein Lesen nicht
/// initialisierten Speichers als Markierung im Ergebnis auffiele.
fn fill_heap_with(marker: &str) {
    let m = rusqlite::Connection::open_in_memory().unwrap();
    m.execute_batch("CREATE TABLE t(x)").unwrap();
    for _ in 0..2_000 {
        m.execute("INSERT INTO t VALUES (?1)", [marker.repeat(300)])
            .unwrap();
    }
    let _: i64 = m
        .query_row("SELECT sum(length(x)) FROM t", [], |r| r.get(0))
        .unwrap();
}

fn all_urls(file: &CrawlFile) -> Vec<Vec<rusqlite::types::Value>> {
    let mut stmt = file.conn().prepare("SELECT * FROM urls").unwrap();
    let n = stmt.column_count();
    stmt.query_map([], |row| (0..n).map(|i| row.get(i)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

// RUSTSEC-2026-0459 (CVE-2026-119375): Eine fremde Crawl-Datei (Seitengröße 4096) mit einer gültigen `-wal` kleinerer
// Seitengröße (1024), die eine Tabellenseite überdeckt. SQLite vor 3.54 lieferte den Rest der Seite aus nicht
// initialisiertem Speicher; seit 3.54 ist er genullt. Erwartet: kein Panic, kein Speicherinhalt im Ergebnis (gleiches
// Ergebnis bei anders gefülltem Speicher, keine Markierung), die Prüfung meldet die Datei als beschädigt, Öffnen mit
// Schreibrechten scheitert, und keine der beiden Dateien wird verändert.
// spec: OUT-255
#[test]
fn a_foreign_wal_with_a_smaller_page_size_yields_no_stale_memory_and_fails_the_check() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.ospider");
    fs::copy(archive().join("schema-v5/crawl.ospider"), &path).unwrap();
    let (root, pages, page_size): (u32, u32, usize) = {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let q = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        (
            u32::try_from(q("SELECT rootpage FROM sqlite_master WHERE name = 'urls'")).unwrap(),
            u32::try_from(q("PRAGMA page_count")).unwrap(),
            usize::try_from(q("PRAGMA page_size")).unwrap(),
        )
    };
    assert_eq!(page_size, 4096);
    let bytes = fs::read(&path).unwrap();
    let start = (root as usize - 1) * page_size;
    let mut wal_path = path.as_os_str().to_owned();
    wal_path.push("-wal");
    let wal = foreign_wal(1024, root, pages, &bytes[start..start + 1024]);
    fs::write(&wal_path, &wal).unwrap();

    fill_heap_with("LEAKMARKER");
    let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let first = all_urls(&file);
    drop(file);
    fill_heap_with("OTHERFILL!");
    let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let second = all_urls(&file);
    assert_eq!(first, second, "Ergebnis hängt vom Speicherinhalt ab");
    let text = format!("{first:?}");
    assert!(
        !text.contains("LEAKMARKER") && !text.contains("OTHERFILL"),
        "{text}"
    );
    assert!(!file.integrity_check().unwrap());
    drop(file);
    assert!(matches!(
        os_store::files::verify(&path),
        Err(StoreError::Corrupt { .. })
    ));
    let error = CrawlFile::open(&path, OpenMode::ReadWrite).err().unwrap();
    assert!(
        matches!(&error, StoreError::Sqlite(e) if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseCorrupt)),
        "{error:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read(&wal_path).unwrap(), wal);
}

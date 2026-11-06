//! Crawl-Datei anlegen und öffnen (OUT-230, CRAWL-127): Kennung, Versionsprüfung, Migration mit Sicherung.

use std::path::{Path, PathBuf};

use os_model::time::SystemClock;
use os_model::{Clock, CrawlConfig, CrawlId, CrawlMode, CrawlState, Timestamp};
use rusqlite::{Connection, ErrorCode, OpenFlags};

use crate::collation;
use crate::error::StoreError;
use crate::profile::{self, StorageProfile};
use crate::schema::{self, Migration, APPLICATION_ID, MIGRATIONS};

/// SQLite uses Win32 APIs directly, bypassing Rust's long-path normalization.
/// Use the bundled long-path VFS and an extended absolute path on Windows.
pub(crate) fn open_sqlite(
    path: impl AsRef<Path>,
    flags: OpenFlags,
) -> rusqlite::Result<Connection> {
    #[cfg(windows)]
    {
        Connection::open_with_flags_and_vfs(sqlite_path(path.as_ref()), flags, "win32-longpath")
    }
    #[cfg(not(windows))]
    {
        Connection::open_with_flags(path, flags)
    }
}

/// Normalize an existing file or its existing parent, including new VACUUM targets.
pub(crate) fn sqlite_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        std::fs::canonicalize(path)
            .or_else(|_| {
                let parent = path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                std::fs::canonicalize(parent).map(|p| {
                    path.file_name()
                        .map_or_else(|| p.clone(), |name| p.join(name))
                })
            })
            .unwrap_or_else(|_| path.to_path_buf())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// Länge des SQLite-Dateikopfs.
pub(crate) const HEADER_LEN: usize = 100;

/// Liest den Anfang einer Crawl-Datei; liefert die Anzahl gelesener Bytes (kurze Dateien: weniger als `HEADER_LEN`).
///
/// **Nie `std::fs::File::open` auf eine Crawl-Datei (CRAWL-125):** POSIX gibt alle Sperren eines Prozesses auf eine Datei
/// frei, sobald der Prozess irgendeinen Deskriptor dieser Datei schließt. Hielte er dabei eine SQLite-Verbindung, verlöre
/// sie ihre gemeinsame Sperre; das beendende Schließen einer Verbindung in einem anderen Prozess hielte sich für die
/// letzte und löschte `-wal`/`-shm`, in die der Besitzer weiterschreibt – die Datei wird beschädigt. Das Lesen über das VFS
/// von SQLite vermeidet das: SQLite kennt die Sperren je Inode und schiebt das Schließen auf, solange welche bestehen.
#[cfg(unix)]
pub(crate) fn read_header(path: &Path, head: &mut [u8; HEADER_LEN]) -> std::io::Result<usize> {
    use rusqlite::ffi;
    use std::ffi::c_int;
    use std::os::unix::ffi::OsStrExt;

    let failed = |what: &str, rc: c_int| {
        std::io::Error::other(format!("SQLite-VFS: {what} fehlgeschlagen (Code {rc})"))
    };
    let absolute = std::path::absolute(path)?;
    let bytes = absolute.as_os_str().as_bytes();
    if bytes.contains(&0) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    // Das VFS erwartet einen vollständigen Pfad, hinter dem weitere NUL-Bytes stehen dürfen.
    let mut name = bytes.to_vec();
    name.extend_from_slice(&[0, 0, 0]);
    // SAFETY: Das VFS bleibt bis Prozessende registriert. `storage` ist auf 8 Bytes ausgerichtet und `szOsFile` groß, wie es
    // `xOpen` verlangt; `name` lebt bis nach `xClose`. `xRead` und `xFileSize` schreiben nur in `head` (höchstens dessen
    // Länge) und `size`. `xClose` läuft genau einmal, nachdem `xOpen` Methoden gesetzt hat.
    unsafe {
        let vfs = ffi::sqlite3_vfs_find(std::ptr::null());
        if vfs.is_null() {
            return Err(std::io::Error::other("SQLite-VFS fehlt"));
        }
        let open = (*vfs).xOpen.ok_or_else(|| failed("xOpen", 0))?;
        let mut storage = vec![0u64; usize::try_from((*vfs).szOsFile).unwrap_or(0).div_ceil(8)];
        let file = storage.as_mut_ptr().cast::<ffi::sqlite3_file>();
        let mut out_flags: c_int = 0;
        let rc = open(
            vfs,
            name.as_ptr().cast(),
            file,
            ffi::SQLITE_OPEN_READONLY | ffi::SQLITE_OPEN_MAIN_DB,
            &mut out_flags,
        );
        if rc != ffi::SQLITE_OK {
            let os_error = std::io::Error::last_os_error();
            let methods = (*file).pMethods;
            if !methods.is_null() {
                if let Some(close) = (*methods).xClose {
                    close(file);
                }
            }
            return Err(if os_error.raw_os_error().unwrap_or(0) != 0 {
                os_error
            } else {
                failed("xOpen", rc)
            });
        }
        let methods = (*file).pMethods;
        let mut size: ffi::sqlite3_int64 = 0;
        let mut rc = match (*methods).xFileSize {
            Some(file_size) => file_size(file, &mut size),
            None => ffi::SQLITE_MISUSE,
        };
        let wanted = usize::try_from(size.max(0))
            .unwrap_or(usize::MAX)
            .min(HEADER_LEN);
        if rc == ffi::SQLITE_OK && wanted > 0 {
            rc = match (*methods).xRead {
                Some(read) => read(file, head.as_mut_ptr().cast(), wanted as c_int, 0),
                None => ffi::SQLITE_MISUSE,
            };
        }
        if let Some(close) = (*methods).xClose {
            close(file);
        }
        if rc != ffi::SQLITE_OK {
            return Err(failed("Lesen", rc));
        }
        Ok(wanted)
    }
}

/// Windows: Sperren gehören dem Handle, ein weiteres Handle hebt sie nicht auf (`LockFileEx`). Der Weg über `std::fs` bleibt
/// dort derselbe wie zuvor.
#[cfg(not(unix))]
pub(crate) fn read_header(path: &Path, head: &mut [u8; HEADER_LEN]) -> std::io::Result<usize> {
    read_header_plain(path, head)
}

/// Unter Unix nur im Test (Vergleich mit dem VFS-Weg), damit der Windows-Weg auch hier übersetzt und geprüft wird.
#[cfg(any(not(unix), test))]
fn read_header_plain(path: &Path, head: &mut [u8; HEADER_LEN]) -> std::io::Result<usize> {
    use std::io::Read;

    let mut f = std::fs::File::open(path)?;
    let mut read = 0;
    while read < head.len() {
        match f.read(&mut head[read..])? {
            0 => break,
            n => read += n,
        }
    }
    Ok(read)
}

/// Öffnet eine Datei nur lesend, ohne Begleitdateien und ohne Sperren (`immutable`): für Dateien ohne `-wal`, `-shm` und
/// `-journal`, also ohne Schreiber in diesem Augenblick.
pub(crate) fn open_sqlite_immutable(path: &Path) -> rusqlite::Result<Connection> {
    let path = sqlite_path(path);
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let bytes = path.to_string_lossy().into_owned().into_bytes();
    let mut uri = String::from("file:");
    for b in bytes {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b'~' | b':') {
            uri.push(char::from(b));
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri.push_str("?immutable=1");
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    #[cfg(windows)]
    {
        Connection::open_with_flags_and_vfs(uri, flags, "win32-longpath")
    }
    #[cfg(not(windows))]
    {
        Connection::open_with_flags(uri, flags)
    }
}

/// Öffnungsmodus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// Nur lesen (kein Schreiben, keine Migration).
    ReadOnly,
    /// Lesen und schreiben (migriert ältere Dateien).
    ReadWrite,
}

/// Angaben zum Anlegen einer neuen Crawl-Datei.
#[derive(Debug, Clone)]
pub struct CrawlInit {
    /// Kennung des Crawls.
    pub crawl_id: CrawlId,
    /// Modus.
    pub mode: CrawlMode,
    /// Start-URL (Spider-Modus).
    pub start_url: Option<String>,
    /// Quelle der URL-Liste (Listenmodus).
    pub list_source: Option<String>,
    /// Projektname.
    pub project: String,
    /// Crawl-Name.
    pub name: String,
    /// Wirksame Konfiguration (wird eingebettet).
    pub config: CrawlConfig,
    /// Anlegezeitpunkt.
    pub created_at: Timestamp,
}

/// Eine geöffnete Crawl-Datei (SQLite).
pub struct CrawlFile {
    path: PathBuf,
    conn: Connection,
}

const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

fn classify(path: &Path, e: rusqlite::Error) -> StoreError {
    if let rusqlite::Error::SqliteFailure(f, msg) = &e {
        match f.code {
            ErrorCode::NotADatabase => {
                return StoreError::NotACrawlFile {
                    path: path.to_path_buf(),
                }
            }
            ErrorCode::DatabaseCorrupt => {
                return StoreError::Corrupt {
                    path: path.to_path_buf(),
                    detail: msg.clone().unwrap_or_else(|| e.to_string()),
                }
            }
            _ => {}
        }
    }
    StoreError::Sqlite(e)
}

/// Ob neben `path` eine nicht leere `<datei>-wal` liegt (Frames, die beim Öffnen wiederhergestellt werden).
fn wal_has_frames(path: &Path) -> bool {
    let mut name = path.as_os_str().to_owned();
    name.push("-wal");
    std::fs::metadata(std::path::PathBuf::from(name)).is_ok_and(|m| m.len() > 0)
}

/// Prüft die SQLite-Kopfzeile: `Ok(false)`, wenn die Datei nicht mit dem SQLite-Magic beginnt (auch bei
/// kürzeren Dateien); `Err(Corrupt)`, wenn die Datei kürzer ist als der Kopf angibt (abgeschnitten) und keine nicht leere `-wal`-Datei daneben liegt.
fn check_sqlite_header(path: &Path) -> Result<bool, StoreError> {
    let mut head = [0u8; HEADER_LEN];
    let read = read_header(path, &mut head).map_err(|e| StoreError::io(path, e))?;
    if read < SQLITE_MAGIC.len() || &head[..16] != SQLITE_MAGIC {
        return Ok(false);
    }
    if read < head.len() {
        return Err(StoreError::Corrupt {
            path: path.to_path_buf(),
            detail: "Datei abgeschnitten (Kopfbereich unvollständig)".to_string(),
        });
    }
    // Größe laut Kopf (Seitenzahl × Seitengröße) ist nur gültig, wenn „version-valid-for“ dem
    // Änderungszähler entspricht (SQLite-Dateiformat, Abschnitt 1.3).
    let page_size = match u16::from_be_bytes([head[16], head[17]]) {
        1 => 65_536u64,
        n => u64::from(n),
    };
    let page_count = u64::from(u32::from_be_bytes([head[28], head[29], head[30], head[31]]));
    let change_counter = u32::from_be_bytes([head[24], head[25], head[26], head[27]]);
    let valid_for = u32::from_be_bytes([head[92], head[93], head[94], head[95]]);
    // Neben einer nicht leeren `-wal`-Datei gilt der Kopf der Hauptdatei nicht: Ein harter Abbruch mitten im
    // WAL-Checkpoint hinterlässt eine Hauptdatei, deren Kopf schon die neue Seitenzahl nennt, während die Seiten
    // noch zum Teil in der `-wal`-Datei stehen. SQLite nimmt die Größe dann aus dem letzten Commit der
    // `-wal`-Datei und stellt die Datei beim Öffnen wieder her (PERF-005, CRAWL-125). Ohne `-wal`-Datei ist die
    // Hauptdatei allein maßgeblich und eine zu kurze Datei abgeschnitten.
    if page_count > 0 && change_counter == valid_for && !wal_has_frames(path) {
        let actual = std::fs::metadata(path)
            .map_err(|e| StoreError::io(path, e))?
            .len();
        if actual < page_count * page_size {
            return Err(StoreError::Corrupt {
                path: path.to_path_buf(),
                detail: format!(
                    "Datei abgeschnitten ({actual} statt {} Bytes)",
                    page_count * page_size
                ),
            });
        }
    }
    Ok(true)
}

/// Erzeuger-Angabe aus der Datei (für die Meldung bei zu neuer Version), falls lesbar.
fn read_generator(conn: &Connection) -> String {
    conn.query_row(
        "SELECT value FROM crawl_meta WHERE key = 'generator'",
        [],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "einer neueren OpenSpider-Version".to_string())
}

impl CrawlFile {
    /// Legt eine neue Crawl-Datei an (Schema, Kennung, Metadaten, Konfiguration). Eine vorhandene Datei wird
    /// nie überschrieben.
    pub fn create(
        path: &Path,
        init: &CrawlInit,
        profile: StorageProfile,
    ) -> Result<Self, StoreError> {
        if path.exists() {
            return Err(StoreError::AlreadyExists(path.to_path_buf()));
        }
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| StoreError::io(parent, e))?;
        }
        match Self::create_inner(path, init, profile) {
            Ok(f) => Ok(f),
            Err(e) => {
                // Keine halb angelegte Datei zurücklassen.
                for suffix in ["", "-wal", "-shm"] {
                    let mut p = path.as_os_str().to_owned();
                    p.push(suffix);
                    let _ = std::fs::remove_file(PathBuf::from(p));
                }
                Err(e)
            }
        }
    }

    fn create_inner(
        path: &Path,
        init: &CrawlInit,
        profile: StorageProfile,
    ) -> Result<Self, StoreError> {
        let mut conn = open_sqlite(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        collation::ensure(&conn)?;
        profile::apply(&conn, profile)?;
        conn.pragma_update(None, "application_id", APPLICATION_ID)?;
        schema::migrate(&mut conn, 0, MIGRATIONS)?;

        let tx = conn.transaction()?;
        {
            let mut put = tx.prepare("INSERT INTO crawl_meta (key, value) VALUES (?1, ?2)")?;
            let mut rows: Vec<(&str, String)> = vec![
                ("crawl_id", init.crawl_id.to_string()),
                (
                    "schema_version",
                    schema::latest_version(MIGRATIONS).to_string(),
                ),
                ("generator", init.config.generator.clone()),
                ("created_at", init.created_at.to_db()),
                ("mode", init.mode.as_str().to_string()),
                ("project", init.project.clone()),
                ("name", init.name.clone()),
                ("state", CrawlState::Running.as_str().to_string()),
                ("next_url_id", "1".to_string()),
                ("next_seq", "1".to_string()),
                ("analysis_stale", "0".to_string()),
                // Änderungsfeed (P00c, 3.9): der Writer erhöht den Wert je Batch.
                ("change_seq", "0".to_string()),
                // Manifest (OUT-233, M2-F02): anlegendes und zuletzt schreibendes Programm.
                (
                    "openspider_version",
                    crate::meta::OPENSPIDER_VERSION.to_string(),
                ),
                (
                    "last_writer_version",
                    crate::meta::OPENSPIDER_VERSION.to_string(),
                ),
            ];
            if let Some(u) = &init.start_url {
                rows.push(("start_url", u.clone()));
            }
            if let Some(l) = &init.list_source {
                rows.push(("list_source", l.clone()));
            }
            for (k, v) in rows {
                put.execute([k, v.as_str()])?;
            }
        }
        let osconfig = init.config.to_toml_string()?;
        tx.execute(
            "INSERT INTO crawl_config (id, osconfig, updated_at) VALUES (1, ?1, ?2)",
            [&osconfig, &init.created_at.to_db()],
        )?;
        crate::meta::stamp_config(&tx, &osconfig)?;
        tx.commit()?;
        // Kennung und Schema stehen damit auch ohne WAL-Datei im Dateikopf (z. B. nach einem Absturz).
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(CrawlFile {
            path: path.to_path_buf(),
            conn,
        })
    }

    /// Öffnet eine vorhandene Crawl-Datei. Fremde Dateien und Dateien mit neuerer Schemaversion werden
    /// abgewiesen, ohne sie zu verändern; ältere werden (im Modus `ReadWrite`) nach einer Sicherungskopie
    /// `<name>.v<alt>.bak.ospider` migriert.
    pub fn open(path: &Path, mode: OpenMode) -> Result<Self, StoreError> {
        Self::open_with(path, mode, MIGRATIONS)
    }

    /// Wie [`open`](Self::open) mit eigener Migrationsliste (für Tests des Migrationsrahmens).
    pub fn open_with(
        path: &Path,
        mode: OpenMode,
        migrations: &[Migration],
    ) -> Result<Self, StoreError> {
        Self::open_inner(path, mode, migrations, true)
    }

    /// Migriert eine ältere Crawl-Datei **als Kopie**: `src` bleibt Byte für Byte unverändert (auch ihre `-wal`-Datei
    /// wird nicht angefasst), `dst` ist eine eigenständige Datei im aktuellen Schema (OUT-184, OS-Festlegung:
    /// `--load-crawl` einer Datei älterer Schemaversion). Eine Sicherungskopie `<name>.v<alt>.bak.ospider` entsteht nicht,
    /// weil das Original selbst die Sicherung ist. Ist `src` bereits aktuell, ist `dst` eine einfache Kopie.
    /// `dst` darf nicht existieren.
    pub fn migrate_copy(src: &Path, dst: &Path) -> Result<Self, StoreError> {
        if dst.exists() {
            return Err(StoreError::AlreadyExists(dst.to_path_buf()));
        }
        if !src.exists() {
            return Err(StoreError::NotFound(src.to_path_buf()));
        }
        if !check_sqlite_header(src)? {
            return Err(StoreError::NotACrawlFile {
                path: src.to_path_buf(),
            });
        }
        if let Some(parent) = dst.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| StoreError::io(parent, e))?;
        }
        let copy = (|| -> Result<(), StoreError> {
            let probe = open_sqlite(
                src,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|e| classify(src, e))?;
            probe.busy_timeout(std::time::Duration::from_secs(5))?;
            let app_id = schema::application_id(&probe).map_err(|e| match e {
                StoreError::Sqlite(e) => classify(src, e),
                other => other,
            })?;
            if app_id != APPLICATION_ID {
                return Err(StoreError::NotACrawlFile {
                    path: src.to_path_buf(),
                });
            }
            let found = schema::user_version(&probe)?;
            let latest = schema::latest_version(MIGRATIONS);
            if found > latest {
                return Err(StoreError::NewerVersion {
                    path: src.to_path_buf(),
                    found,
                    supported: latest,
                    generator: read_generator(&probe),
                });
            }
            // `VACUUM INTO` liest einen konsistenten Schnappschuss (auch aus dem WAL) und schreibt nur `dst`.
            let target = dst.to_string_lossy().into_owned();
            probe.execute("VACUUM INTO ?1", [target.as_str()])?;
            Ok(())
        })();
        if let Err(e) = copy {
            let _ = std::fs::remove_file(dst);
            return Err(e);
        }
        match Self::open_inner(dst, OpenMode::ReadWrite, MIGRATIONS, false) {
            Ok(f) => Ok(f),
            Err(e) => {
                for suffix in ["", "-wal", "-shm"] {
                    let mut p = dst.as_os_str().to_owned();
                    p.push(suffix);
                    let _ = std::fs::remove_file(PathBuf::from(p));
                }
                Err(e)
            }
        }
    }

    fn open_inner(
        path: &Path,
        mode: OpenMode,
        migrations: &[Migration],
        backup: bool,
    ) -> Result<Self, StoreError> {
        if !path.exists() {
            return Err(StoreError::NotFound(path.to_path_buf()));
        }
        if !check_sqlite_header(path)? {
            return Err(StoreError::NotACrawlFile {
                path: path.to_path_buf(),
            });
        }
        let latest = schema::latest_version(migrations);
        // Die Manifest-Werte (OUT-233) pflegt nur die Produktionsliste; Tests mit eigenen Listen haben kein `crawl_meta`.
        let production = std::ptr::eq(migrations, MIGRATIONS);

        // Prüfen, ohne die Datei zu verändern.
        let (found, generator) = {
            let probe = open_sqlite(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|e| classify(path, e))?;
            let app_id = schema::application_id(&probe).map_err(|e| match e {
                StoreError::Sqlite(e) => classify(path, e),
                other => other,
            })?;
            if app_id != APPLICATION_ID {
                return Err(StoreError::NotACrawlFile {
                    path: path.to_path_buf(),
                });
            }
            let version = schema::user_version(&probe)?;
            if version == 0 {
                return Err(StoreError::Corrupt {
                    path: path.to_path_buf(),
                    detail: "Schema unvollständig angelegt".to_string(),
                });
            }
            (version, read_generator(&probe))
        };
        if found > latest {
            return Err(StoreError::NewerVersion {
                path: path.to_path_buf(),
                found,
                supported: latest,
                generator,
            });
        }
        if found < latest && mode == OpenMode::ReadOnly {
            return Err(StoreError::MigrationRequired {
                path: path.to_path_buf(),
                found,
                supported: latest,
            });
        }

        // Die Sicherungskopie entsteht vor der ersten Änderung der Datei (auch vor `PRAGMA journal_mode`), damit sie
        // dem Original Byte für Byte entspricht (OUT-184). Liegen noch Frames in einer `-wal`-Datei, ist die Hauptdatei
        // allein unvollständig; dann entsteht die Kopie nach dem Zusammenführen (weiter unten).
        let mut backed_up = false;
        if found < latest && mode == OpenMode::ReadWrite && backup && !wal_has_frames(path) {
            let target = backup_path(path, found);
            std::fs::copy(path, &target).map_err(|e| StoreError::io(&target, e))?;
            backed_up = true;
        }

        let flags = match mode {
            OpenMode::ReadOnly => OpenFlags::SQLITE_OPEN_READ_ONLY,
            OpenMode::ReadWrite => OpenFlags::SQLITE_OPEN_READ_WRITE,
        };
        let mut conn = open_sqlite(path, flags).map_err(|e| classify(path, e))?;
        collation::ensure(&conn)?;
        if mode == OpenMode::ReadWrite {
            profile::apply(&conn, StorageProfile::default())?;
        } else {
            conn.busy_timeout(std::time::Duration::from_secs(5))?;
        }

        if found < latest {
            // Sicherungskopie als eigenständige Datei (WAL zusammenführen), dann migrieren.
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
            if backup && !backed_up {
                let target = backup_path(path, found);
                std::fs::copy(path, &target).map_err(|e| StoreError::io(&target, e))?;
            }
            schema::migrate(&mut conn, found, migrations)?;
            conn.execute(
                "INSERT INTO crawl_meta (key, value) VALUES ('schema_version', ?1) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [latest.to_string()],
            )?;
            // Manifest (OUT-233): was die alte Datei nicht kennt (`openspider_version`), bleibt unbekannt (NULL);
            // abgeleitet werden nur Werte aus der eingebetteten Konfiguration.
            if production {
                if let Some(osconfig) = crate::meta::raw_config(&conn)? {
                    crate::meta::stamp_config(&conn, &osconfig)?;
                }
            }
        }
        if mode == OpenMode::ReadWrite && production {
            crate::meta::stamp_writer(&conn)?;
        }
        Ok(CrawlFile {
            path: path.to_path_buf(),
            conn,
        })
    }

    /// Pfad der Datei.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Flushes the WAL into the crawl file after writers have stopped.
    /// Returns false when a reader still holds a snapshot; callers must not install
    /// an update until the checkpoint succeeds. Execute on the storage worker thread.
    pub fn checkpoint_and_truncate(&self) -> Result<bool, StoreError> {
        let busy: i64 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        Ok(busy == 0)
    }

    /// Die SQLite-Verbindung (für `os-analyze`, `os-export`, `os-mcp`).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Liest die eingebettete Konfiguration (CRAWL-127: die damalige Konfiguration).
    pub fn config(&self) -> Result<CrawlConfig, StoreError> {
        let text: Option<String> = self
            .conn
            .query_row("SELECT osconfig FROM crawl_config WHERE id = 1", [], |r| {
                r.get(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let text = text.ok_or(StoreError::MissingConfig)?;
        Ok(CrawlConfig::from_toml_str(&text)?)
    }

    /// Ersetzt die eingebettete Konfiguration („Konfiguration übernehmen“).
    ///
    /// Der Zeitstempel kommt von der Systemuhr; für deterministische Läufe siehe
    /// [`CrawlFile::set_config_with_clock`].
    pub fn set_config(&self, c: &CrawlConfig) -> Result<(), StoreError> {
        self.set_config_with_clock(c, &SystemClock)
    }

    /// Wie [`CrawlFile::set_config`], aber mit dem Zeitstempel aus `clock` (Regel 2.8).
    pub fn set_config_with_clock(
        &self,
        c: &CrawlConfig,
        clock: &dyn Clock,
    ) -> Result<(), StoreError> {
        let osconfig = c.to_toml_string()?;
        self.conn.execute(
            "INSERT INTO crawl_config (id, osconfig, updated_at) VALUES (1, ?1, ?2) \
             ON CONFLICT(id) DO UPDATE SET osconfig = excluded.osconfig, updated_at = excluded.updated_at",
            [&osconfig, &clock.now().to_db()],
        )?;
        crate::meta::stamp_config(&self.conn, &osconfig)?;
        Ok(())
    }
}

/// `<verzeichnis>/<name>.v<alt>.bak.ospider` (CRAWL-127).
pub fn backup_path(path: &Path, old_version: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .map_or_else(|| "crawl".into(), |s| s.to_string_lossy().into_owned());
    path.with_file_name(format!("{stem}.v{old_version}.bak.ospider"))
}

/// Länge der `-wal`-Datei neben `path` in Byte (0, wenn sie fehlt).
pub fn wal_bytes(path: &Path) -> u64 {
    let mut name = path.as_os_str().to_owned();
    name.push("-wal");
    std::fs::metadata(PathBuf::from(name)).map_or(0, |m| m.len())
}

/// Führt das WAL nach einer Pause oder einem Ende vollständig in die Hauptdatei zurück (CRAWL-126, OS-Festlegung
/// vom 01.11.2026): eine Schleife kurzer `TRUNCATE`-Versuche, die je höchstens
/// [`crate::writer::CHECKPOINT_READER_WAIT`] auf laufende Leser warten. `true`, sobald das WAL leer ist; `false`,
/// wenn `cancel` gesetzt wurde (etwa vor dem Fortsetzen), bevor das gelang. Läuft auf einem Blocking-Thread.
pub fn merge_wal(path: &Path, cancel: &std::sync::atomic::AtomicBool) -> Result<bool, StoreError> {
    use std::sync::atomic::Ordering;
    if wal_bytes(path) == 0 {
        return Ok(true);
    }
    let conn = open_sqlite(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let busy: i64 = crate::writer::checkpoint_within(
            &conn,
            crate::writer::CHECKPOINT_READER_WAIT,
            "PRAGMA wal_checkpoint(TRUNCATE)",
            |r| r.get(0),
        )?;
        if busy == 0 {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(test)]
mod header_tests {
    use super::*;

    // Beide Lesewege liefern dasselbe, auch für kurze, leere und fremde Dateien (Windows nutzt den einfachen Weg).
    // spec: CRAWL-127
    #[test]
    fn header_reads_agree_for_short_empty_foreign_and_real_files() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in [
            ("leer", vec![]),
            ("kurz", b"SQLite format".to_vec()),
            ("png", vec![0x89; 4_096]),
            ("genau", vec![7; HEADER_LEN]),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, &bytes).unwrap();
            let mut plain = [0u8; HEADER_LEN];
            let plain_len = read_header_plain(&path, &mut plain).unwrap();
            let mut vfs = [0u8; HEADER_LEN];
            let vfs_len = read_header(&path, &mut vfs).unwrap();
            assert_eq!(vfs_len, plain_len, "{name}");
            assert_eq!(vfs[..vfs_len], plain[..plain_len], "{name}");
            assert_eq!(vfs_len, bytes.len().min(HEADER_LEN), "{name}");
        }
        let missing = read_header(&dir.path().join("fehlt"), &mut [0u8; HEADER_LEN]);
        assert!(missing.is_err());
    }

    // Sonderzeichen im Pfad überstehen die URI-Form von `immutable`; es entstehen keine Begleitdateien.
    // spec: OUT-228
    #[test]
    fn immutable_opens_paths_with_special_characters_without_companions() {
        let dir = tempfile::tempdir().unwrap();
        // `?` und `:` sind unter Windows in Dateinamen unzulässig.
        let (folder, file) = if cfg!(windows) {
            ("Kunde A #1 100% ä&=", "c r#awl.ospider")
        } else {
            ("Kunde A #1 100% ä?&=", "c r:awl.ospider")
        };
        let sub = dir.path().join(folder);
        std::fs::create_dir(&sub).unwrap();
        let path = sub.join(file);
        let conn = open_sqlite(&path, OpenFlags::default()).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; CREATE TABLE t(a); INSERT INTO t VALUES (42);",
        )
        .unwrap();
        drop(conn);
        let names = || std::fs::read_dir(&sub).unwrap().count();
        assert_eq!(names(), 1);
        let reader = open_sqlite_immutable(&path).unwrap();
        let value: i64 = reader
            .query_row("SELECT a FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, 42);
        drop(reader);
        assert_eq!(names(), 1);
    }
}

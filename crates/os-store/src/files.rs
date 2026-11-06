//! Dateiverwaltung einer Crawl-Datei: Kopie, Integritätsprüfung, Reparatur, Speicherplatz, Speicherort.
//!
//! * [`CrawlFile::save_as`] schreibt mit `VACUUM INTO` eine eigenständige Datei (OUT-230, CRAWL-127): das WAL
//!   ist zusammengeführt, `-wal`/`-shm` werden nicht gebraucht, `application_id` und `user_version` bleiben.
//! * [`verify`] und [`repair_copy`]: Integritätsprüfung und Reparatur-Kopie `<name>.repaired.ospider`.
//! * [`disk_space_status`]: Schwellen für Startwarnung (2 GiB) und automatische Pause (1 GiB), CRAWL-124.
//! * [`location_warnings`]: Hinweise zum Speicherort (Sync-Ordner, Netzlaufwerke, FAT).

use std::path::{Path, PathBuf};

use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection, OpenFlags};

use crate::error::StoreError;
use crate::open::{CrawlFile, OpenMode};
use crate::schema::{self, APPLICATION_ID, MIGRATIONS};

/// Unter diesem freien Speicher (GiB-Binärpräfix) erscheint beim Crawlstart eine Warnung.
pub const DISK_WARN_BELOW_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Unter diesem freien Speicher pausiert der laufende Crawl automatisch („Speicherplatz knapp“).
pub const DISK_PAUSE_BELOW_BYTES: u64 = 1024 * 1024 * 1024;
/// Prüfintervall des freien Speichers während des Crawls, in Sekunden.
pub const DISK_CHECK_INTERVAL_SECS: u64 = 5;

impl CrawlFile {
    /// Speichert eine eigenständige Kopie (ohne `-wal`/`-shm`). Ein vorhandenes Ziel wird nie überschrieben
    /// ([`StoreError::AlreadyExists`]); fehlende Ordner werden angelegt. Die Kopie entsteht unter einem
    /// Arbeitsnamen und wird erst fertig umbenannt, damit nie eine halbe Datei unter dem Zielnamen liegt.
    pub fn save_as(&self, dest: &Path) -> Result<(), StoreError> {
        if dest.exists() {
            return Err(StoreError::AlreadyExists(dest.to_path_buf()));
        }
        if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| StoreError::io(parent, e))?;
        }
        let tmp = sibling(dest, ".saving");
        remove_with_companions(&tmp);
        let sqlite_tmp = crate::open::sqlite_path(&tmp);
        let result = self
            .conn()
            .execute("VACUUM INTO ?1", [sqlite_tmp.to_string_lossy().as_ref()])
            .map_err(StoreError::from)
            .and_then(|_| std::fs::rename(&tmp, dest).map_err(|e| StoreError::io(dest, e)));
        if result.is_err() {
            remove_with_companions(&tmp);
        }
        result
    }

    /// `PRAGMA integrity_check`: `true` bei `ok`. Bei einer beschädigten Datei kann SQLite statt einer
    /// Meldung einen Fehler liefern; dann gilt die Datei als nicht in Ordnung (`false`).
    pub fn integrity_check(&self) -> Result<bool, StoreError> {
        Ok(self.integrity_report(1)?.is_empty())
    }

    /// Meldungen von `PRAGMA integrity_check` (höchstens `max`); leer, wenn die Datei in Ordnung ist.
    pub fn integrity_report(&self, max: u32) -> Result<Vec<String>, StoreError> {
        integrity_messages(self.conn(), max)
    }
}

fn integrity_messages(conn: &Connection, max: u32) -> Result<Vec<String>, StoreError> {
    let run = || -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare(&format!("PRAGMA integrity_check({})", max.max(1)))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect()
    };
    match run() {
        Ok(rows) if rows.len() == 1 && rows[0] == "ok" => Ok(Vec::new()),
        Ok(rows) => Ok(rows),
        Err(rusqlite::Error::SqliteFailure(f, msg))
            if matches!(
                f.code,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
            ) =>
        {
            Ok(vec![msg.unwrap_or_else(|| f.to_string())])
        }
        Err(e) => Err(e.into()),
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn remove_with_companions(path: &Path) {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(sibling(path, suffix));
    }
}

/// Prüft eine Crawl-Datei von außen: öffnet sie schreibgeschützt (fremde, abgeschnittene und neuere
/// Dateien liefern die bekannten Fehler, ohne die Datei zu verändern) und führt die Integritätsprüfung aus.
/// Eine ältere Schemaversion zählt nicht als Fehler (sie wird beim Öffnen migriert).
pub fn verify(path: &Path) -> Result<(), StoreError> {
    let conn = match CrawlFile::open(path, OpenMode::ReadOnly) {
        Ok(file) => return verify_file(path, &file),
        Err(StoreError::MigrationRequired { .. }) => open_raw_readonly(path)?,
        Err(e) => return Err(e),
    };
    verify_conn(path, &conn)
}

fn verify_file(path: &Path, file: &CrawlFile) -> Result<(), StoreError> {
    verify_conn(path, file.conn())
}

fn verify_conn(path: &Path, conn: &Connection) -> Result<(), StoreError> {
    let messages = integrity_messages(conn, 5)?;
    if messages.is_empty() {
        Ok(())
    } else {
        Err(StoreError::Corrupt {
            path: path.to_path_buf(),
            detail: messages.join("; "),
        })
    }
}

pub(crate) fn classify_open(path: &Path, e: rusqlite::Error) -> StoreError {
    if let rusqlite::Error::SqliteFailure(f, msg) = &e {
        match f.code {
            rusqlite::ErrorCode::NotADatabase => {
                return StoreError::NotACrawlFile {
                    path: path.to_path_buf(),
                }
            }
            rusqlite::ErrorCode::DatabaseCorrupt => {
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

const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// Liest nur den 100-Byte-Kopf der Datei (ohne eine SQLite-Verbindung und ohne die Sperren dieses Prozesses aufzuheben,
/// [`crate::open::read_header`]) und weist fremde Dateien ab: kein SQLite-Magic oder falsche `application_id` →
/// [`StoreError::NotACrawlFile`]; Kopf unvollständig → [`StoreError::Corrupt`]. Verändert nichts, auch keine Begleitdateien.
pub(crate) fn probe_header(path: &Path) -> Result<[u8; crate::open::HEADER_LEN], StoreError> {
    let mut head = [0u8; crate::open::HEADER_LEN];
    let read = crate::open::read_header(path, &mut head).map_err(|e| StoreError::io(path, e))?;
    if read < SQLITE_MAGIC.len() || &head[..16] != SQLITE_MAGIC {
        return Err(StoreError::NotACrawlFile {
            path: path.to_path_buf(),
        });
    }
    if read < head.len() {
        return Err(StoreError::Corrupt {
            path: path.to_path_buf(),
            detail: "Datei abgeschnitten (Kopfbereich unvollständig)".to_string(),
        });
    }
    let app_id = i32::from_be_bytes([head[68], head[69], head[70], head[71]]);
    if app_id != APPLICATION_ID {
        return Err(StoreError::NotACrawlFile {
            path: path.to_path_buf(),
        });
    }
    Ok(head)
}

/// Öffnet eine Datei schreibgeschützt ohne Migration und prüft die `application_id`. Eine
/// schreibgeschützte Verbindung auf eine WAL-Datei lässt `-wal`/`-shm` zurück.
pub(crate) fn open_raw_readonly(path: &Path) -> Result<Connection, StoreError> {
    if !path.exists() {
        return Err(StoreError::NotFound(path.to_path_buf()));
    }
    probe_header(path)?;
    let conn = crate::open::open_sqlite(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| classify_open(path, e))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

/// Verbindung der Crawl-Verwaltung zum Lesen von Metadaten: schreibt, führt und entfernt nichts (OUT-228, CRAWL-125).
///
/// Die Verwaltung liest auch Dateien, die ein anderer Prozess gerade schreibt. Eine Verbindung mit Schreibrechten wäre dort
/// falsch: Hält sie sich beim Schließen für die letzte, führt sie das WAL zurück und löscht `-wal`/`-shm`. Deshalb nie
/// mit Schreibrechten:
/// * Liegen Begleitdateien daneben (ein Besitzer lebt oder ist abgestürzt), liest eine schreibgeschützte Verbindung unter den
///   normalen Sperren mit und lässt sie liegen.
/// * Liegen keine daneben, schreibt in diesem Augenblick niemand; dann liest `immutable`, das weder Begleitdateien anlegt
///   (die Verwaltung hinterlässt nichts) noch Sperren nimmt, einen Besitzer also nicht stören kann. Beginnt während des Lesens
///   ein Schreiber, sieht der Leser das ohne Sperren nicht. [`MetadataConn::still_valid`] prüft das **nach** dem Lesen; ein
///   Ergebnis von `immutable` gilt nur, wenn sie danach noch `true` liefert, sonst wird unter Sperren neu gelesen.
pub(crate) struct MetadataConn {
    pub(crate) conn: Connection,
    /// Dateizustand beim Öffnen; `None` = gewöhnliche schreibgeschützte Verbindung unter Sperren.
    quiet: Option<QuietStamp>,
}

/// Größe, Änderungszeit und Kopf der Hauptdatei. Wer sie ändert, muss sie zuvor über ein WAL zurückgeführt haben.
#[derive(PartialEq)]
struct QuietStamp {
    len: u64,
    modified: std::time::SystemTime,
    head: [u8; crate::open::HEADER_LEN],
}

fn quiet_stamp(path: &Path) -> Result<Option<QuietStamp>, StoreError> {
    let companion = |suffix: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name).exists()
    };
    if companion("-wal") || companion("-shm") || companion("-journal") {
        return Ok(None);
    }
    let head = probe_header(path)?;
    let meta = std::fs::metadata(path).map_err(|e| StoreError::io(path, e))?;
    let modified = meta.modified().map_err(|e| StoreError::io(path, e))?;
    Ok(Some(QuietStamp {
        len: meta.len(),
        modified,
        head,
    }))
}

impl MetadataConn {
    /// Schreibgeschützte Verbindung unter Sperren als letzter Versuch.
    pub(crate) fn locked(conn: Connection) -> Self {
        MetadataConn { conn, quiet: None }
    }

    /// `true` für die schreibgeschützte Verbindung unter Sperren: Ihre Ergebnisse, auch Fehler, gelten. Bei `false`
    /// (`immutable`) gilt nur ein **fehlerfreies** Ergebnis, nach dem [`Self::still_valid`] noch `true` liefert: Ein Lesefehler kann
    /// von einem Schreiber stammen, der die Datei gerade zurückführt, und darf nie als beschädigte Datei erscheinen.
    pub(crate) fn is_locked(&self) -> bool {
        self.quiet.is_none()
    }

    /// `true`, wenn alles bisher Gelesene verlässlich ist: Sperren schützen die Verbindung, oder seit dem Öffnen von
    /// `immutable` hat niemand die Datei verändert und auch kein Schreiber begonnen.
    pub(crate) fn still_valid(&self, path: &Path) -> bool {
        match &self.quiet {
            None => true,
            Some(before) => matches!(quiet_stamp(path), Ok(Some(now)) if now == *before),
        }
    }
}

/// Öffnet eine Datei zum Lesen von Metadaten ohne Migration (siehe [`MetadataConn`]). Fremde Dateien werden vorab am Dateikopf
/// abgewiesen.
pub(crate) fn open_raw_metadata(path: &Path) -> Result<MetadataConn, StoreError> {
    if !path.exists() {
        return Err(StoreError::NotFound(path.to_path_buf()));
    }
    if let Some(quiet) = quiet_stamp(path)? {
        if let Ok(conn) = crate::open::open_sqlite_immutable(path) {
            conn.busy_timeout(std::time::Duration::from_secs(5))?;
            return Ok(MetadataConn {
                conn,
                quiet: Some(quiet),
            });
        }
    }
    Ok(MetadataConn {
        conn: open_raw_readonly(path)?,
        quiet: None,
    })
}

/// Öffnet eine Crawl-Datei ohne Migration für eine gewollte Änderung der Verwaltung (Metadaten, Checkpoint vor dem Löschen).
/// Nur für Crawls, die laut Verwaltung nicht laufen (`guard`).
pub(crate) fn open_raw_write(path: &Path) -> Result<Connection, StoreError> {
    if !path.exists() {
        return Err(StoreError::NotFound(path.to_path_buf()));
    }
    probe_header(path)?;
    let conn = crate::open::open_sqlite(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| classify_open(path, e))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

// ---------------------------------------------------------------------------------------------------------
// Reparatur
// ---------------------------------------------------------------------------------------------------------

/// Ergebnis der Rettung einer Tabelle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRepair {
    /// Tabellenname.
    pub table: &'static str,
    /// Gerettete Zeilen.
    pub copied: u64,
    /// Nicht lesbare Zeilen bzw. Schlüssel, die übersprungen wurden (mindestens, soweit erkennbar).
    pub lost: u64,
}

/// Bericht einer Reparatur-Kopie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairReport {
    /// Die neue Datei `<name>.repaired.ospider`.
    pub repaired: PathBuf,
    /// Ergebnis je Tabelle.
    pub tables: Vec<TableRepair>,
    /// Die Integritätsprüfung der neuen Datei war in Ordnung.
    pub integrity_ok: bool,
    /// Die neue Datei lässt sich als Crawl-Datei öffnen (Metadaten lesbar).
    pub usable: bool,
}

impl RepairReport {
    /// Gesamtzahl der übersprungenen Zeilen.
    pub fn lost_rows(&self) -> u64 {
        self.tables.iter().map(|t| t.lost).sum()
    }
}

/// Pfad der Reparatur-Kopie: `<verzeichnis>/<name>.repaired.ospider`.
pub fn repaired_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map_or_else(|| "crawl".into(), |s| s.to_string_lossy().into_owned());
    path.with_file_name(format!("{stem}.repaired.ospider"))
}

/// Tabellen mit Schlüsselspalte für das Retten in Bereichen (`None` = ganze Tabelle in einem Durchlauf).
const REPAIR_TABLES: &[(&str, Option<&str>)] = &[
    ("cookies", None),
    ("discovery_edges", Some("seq")),
    ("sitemap_entry_ext", None),
    ("crawl_meta", None),
    ("crawl_config", Some("id")),
    ("urls", Some("id")),
    // nach `urls`: der Trigger legt die Zeilen an, die gerettete Tabelle ersetzt sie
    ("url_counters", Some("url_id")),
    ("links", Some("id")),
    ("image_instances", Some("id")),
    ("canonicals", Some("url_id")),
    ("directives", Some("url_id")),
    ("refresh", Some("url_id")),
    ("hreflang", Some("url_id")),
    ("pagination", Some("url_id")),
    ("http_headers", Some("url_id")),
    ("robots_files", None),
    ("sitemaps", Some("id")),
    ("sitemap_entries", Some("sitemap_id")),
    ("queue", Some("url_id")),
    ("list_input", Some("position")),
    ("analysis_steps", None),
    ("url_flags", Some("url_id")),
    ("crawl_events", Some("id")),
    ("crawl_counters", None),
    ("page_html", Some("url_id")),
];

/// Breite eines Schlüsselbereichs beim Retten.
const REPAIR_WINDOW: i64 = 2048;
/// Versuche je Schreibblock, wenn das Ziel vorübergehend scheitert (`SQLITE_BUSY`, `SQLITE_LOCKED`,
/// `SQLITE_IOERR`); danach bricht die Reparatur mit dem Fehler ab.
const REPAIR_TARGET_ATTEMPTS: u32 = 4;
/// Pause vor der ersten Wiederholung eines Schreibblocks; sie verdoppelt sich je weiterem Versuch.
const REPAIR_TARGET_BACKOFF: std::time::Duration = std::time::Duration::from_millis(50);

/// Rettet so viel wie möglich aus einer beschädigten Crawl-Datei in die neue Datei
/// `<name>.repaired.ospider` (OUT-230). Die Quelle bleibt unverändert; eine vorhandene Zieldatei wird nie
/// überschrieben. Zeilen aus unlesbaren Seiten werden übersprungen und im Bericht gezählt. Nur Dateien mit
/// intaktem Kopf und lesbarem Schema lassen sich retten; fremde und zu neue Dateien liefern die bekannten
/// Fehler. Ein Schreibfehler am Ziel ist kein Datenverlust der Quelle: Vorübergehende Fehler werden je
/// Schreibblock begrenzt wiederholt, jeder andere bricht die Reparatur ab (die halbe Kopie wird entfernt).
pub fn repair_copy(src: &Path) -> Result<RepairReport, StoreError> {
    let source = open_raw_readonly(src)?;
    let version = schema::user_version(&source).map_err(|e| match e {
        StoreError::Sqlite(e) => classify_open(src, e),
        other => other,
    })?;
    let latest = schema::latest_version(MIGRATIONS);
    if version > latest {
        return Err(StoreError::NewerVersion {
            path: src.to_path_buf(),
            found: version,
            supported: latest,
            generator: "einer neueren OpenSpider-Version".to_string(),
        });
    }
    if version < latest {
        return Err(StoreError::MigrationRequired {
            path: src.to_path_buf(),
            found: version,
            supported: latest,
        });
    }

    let dest = repaired_path(src);
    if dest.exists() {
        return Err(StoreError::AlreadyExists(dest));
    }
    let result = repair_into(&source, &dest);
    if result.is_err() {
        remove_with_companions(&dest);
    }
    result
}

fn repair_into(source: &Connection, dest: &Path) -> Result<RepairReport, StoreError> {
    let mut target = crate::open::open_sqlite(dest, OpenFlags::default())?;
    crate::profile::apply(&target, crate::profile::StorageProfile::Balanced)?;
    target.pragma_update(None, "application_id", APPLICATION_ID)?;
    schema::migrate(&mut target, 0, MIGRATIONS)?;

    let mut tables = Vec::new();
    for (table, key) in REPAIR_TABLES {
        tables.push(rescue_table(source, &target, table, *key, &target_backoff)?);
    }

    target.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    let integrity_ok = integrity_messages(&target, 1)?.is_empty();
    drop(target);
    let usable = CrawlFile::open(dest, OpenMode::ReadOnly)
        .and_then(|f| f.meta().map(|_| ()))
        .is_ok();
    Ok(RepairReport {
        repaired: dest.to_path_buf(),
        tables,
        integrity_ok,
        usable,
    })
}

struct Rescue<'a> {
    source: &'a Connection,
    target: &'a Connection,
    table: &'static str,
    key: Option<&'static str>,
    copied: u64,
    lost: u64,
    /// Läuft vor jeder Wiederholung eines Schreibblocks (Versuch ab 1); in der Reparatur eine kurze Pause.
    before_retry: &'a dyn Fn(u32),
}

/// Pause vor der Wiederholung `attempt` (ab 1) eines Schreibblocks am Ziel.
fn target_backoff(attempt: u32) {
    std::thread::sleep(REPAIR_TARGET_BACKOFF * 2u32.pow(attempt.saturating_sub(1).min(8)));
}

/// Fehler am Ziel, die vergehen können: Sperre durch einen anderen Prozess oder ein Ein-/Ausgabefehler
/// (z. B. ein Virenscanner unter Windows). Sie werden je Schreibblock begrenzt wiederholt.
fn transient_target_error(e: &rusqlite::Error) -> bool {
    use rusqlite::ErrorCode as C;
    matches!(
        e.sqlite_error_code(),
        Some(C::DatabaseBusy | C::DatabaseLocked | C::SystemIoFailure)
    )
}

/// Das Ziel weist eine einzelne Zeile zurück, weil ihre Werte nicht zum Schema passen (Constraint, Typ, Größe).
/// Die Werte stammen aus der beschädigten Quelle: Die Zeile ist nicht zu retten und zählt als verloren.
fn rejected_row(e: &rusqlite::Error) -> bool {
    use rusqlite::ErrorCode as C;
    matches!(
        e.sqlite_error_code(),
        Some(C::ConstraintViolation | C::TypeMismatch | C::TooBig)
    )
}

/// Rettet eine Tabelle. `lost` zählt nur Zeilen bzw. Schlüssel, die in der Quelle nicht lesbar waren oder vom Schema
/// abgewiesen wurden. Ein Schreibfehler am Ziel, der auch nach den Wiederholungen bleibt, bricht mit `Err` ab.
fn rescue_table(
    source: &Connection,
    target: &Connection,
    table: &'static str,
    key: Option<&'static str>,
    before_retry: &dyn Fn(u32),
) -> rusqlite::Result<TableRepair> {
    let mut r = Rescue {
        source,
        target,
        table,
        key,
        copied: 0,
        lost: 0,
        before_retry,
    };
    r.run()?;
    Ok(TableRepair {
        table,
        copied: r.copied,
        lost: r.lost,
    })
}

impl Rescue<'_> {
    fn run(&mut self) -> rusqlite::Result<()> {
        if let Some(key) = self.key {
            let bounds: rusqlite::Result<(Option<i64>, Option<i64>)> = self.source.query_row(
                &format!("SELECT MIN({key}), MAX({key}) FROM {}", self.table),
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            );
            match bounds {
                Ok((None, _)) | Ok((_, None)) => return Ok(()), // leere Tabelle
                Ok((Some(lo), Some(hi))) if hi.saturating_sub(lo) / REPAIR_WINDOW < 5_000_000 => {
                    let mut start = lo;
                    loop {
                        let end = start.saturating_add(REPAIR_WINDOW - 1).min(hi);
                        self.range(key, start, end)?;
                        if end >= hi {
                            break;
                        }
                        start = end + 1;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        self.scan_all()
    }

    /// Kopiert die Zeilen mit `lo <= key <= hi`; bei einem Lesefehler wird der Bereich halbiert.
    fn range(&mut self, key: &str, lo: i64, hi: i64) -> rusqlite::Result<()> {
        match self.read_rows(Some((key, lo, hi))) {
            Some(rows) => self.store(rows),
            None if lo < hi => {
                let mid = lo + (hi - lo) / 2;
                self.range(key, lo, mid)?;
                self.range(key, mid + 1, hi)
            }
            None => {
                self.lost += 1;
                Ok(())
            }
        }
    }

    /// Liest zeilenweise bis zum ersten Fehler; was davor gelesen wurde, wird behalten.
    fn scan_all(&mut self) -> rusqlite::Result<()> {
        let (rows, failed) = self.read_rows_partial(None);
        self.store(rows)?;
        if failed {
            self.lost += 1;
        }
        Ok(())
    }

    /// Alle Zeilen oder `None`, wenn beim Lesen ein Fehler aufgetreten ist.
    fn read_rows(&self, range: Option<(&str, i64, i64)>) -> Option<Vec<Vec<Value>>> {
        let (rows, failed) = self.read_rows_partial(range);
        (!failed).then_some(rows)
    }

    /// `(Zeilen, Fehler aufgetreten)`; die Zeilen vor dem Fehler bleiben erhalten.
    fn read_rows_partial(&self, range: Option<(&str, i64, i64)>) -> (Vec<Vec<Value>>, bool) {
        let sql = match range {
            Some((key, _, _)) => format!(
                "SELECT * FROM {} WHERE {key} BETWEEN ?1 AND ?2 ORDER BY {key}",
                self.table
            ),
            None => format!("SELECT * FROM {}", self.table),
        };
        let mut rows_out = Vec::new();
        let Ok(mut stmt) = self.source.prepare(&sql) else {
            return (rows_out, true);
        };
        let columns = stmt.column_count();
        let query = match range {
            Some((_, lo, hi)) => stmt.query([lo, hi]),
            None => stmt.query([]),
        };
        let Ok(mut rows) = query else {
            return (rows_out, true);
        };
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    let mut values = Vec::with_capacity(columns);
                    for i in 0..columns {
                        match row.get::<_, Value>(i) {
                            Ok(v) => values.push(v),
                            Err(_) => return (rows_out, true),
                        }
                    }
                    rows_out.push(values);
                }
                Ok(None) => return (rows_out, false),
                Err(_) => return (rows_out, true),
            }
        }
    }

    /// Schreibt gelesene Zeilen als einen Block ins Ziel. Scheitert der Block an einem vorübergehenden Fehler, wird
    /// er (nach dem Zurückrollen) bis zu [`REPAIR_TARGET_ATTEMPTS`]-mal geschrieben; jeder andere bzw. bleibende
    /// Fehler am Ziel wird gemeldet und nie als verlorene Zeilen gezählt (B3-FLAKES, OUT-230).
    fn store(&mut self, rows: Vec<Vec<Value>>) -> rusqlite::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut attempt = 0;
        loop {
            match self.store_block(&rows) {
                Ok((inserted, rejected)) => {
                    self.copied += inserted;
                    self.lost += rejected;
                    return Ok(());
                }
                Err(e) if transient_target_error(&e) && attempt + 1 < REPAIR_TARGET_ATTEMPTS => {
                    attempt += 1;
                    (self.before_retry)(attempt);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Ein Schreibblock in einer Transaktion: `(eingefügt, vom Schema abgewiesen)`. Bei jedem anderen Fehler rollt
    /// die Transaktion zurück (Drop) und der Fehler geht an den Aufrufer.
    fn store_block(&self, rows: &[Vec<Value>]) -> rusqlite::Result<(u64, u64)> {
        let placeholders = vec!["?"; rows[0].len()].join(", ");
        let sql = format!(
            "INSERT OR REPLACE INTO {} VALUES ({placeholders})",
            self.table
        );
        let tx = self.target.unchecked_transaction()?;
        let (mut inserted, mut rejected) = (0u64, 0u64);
        {
            let mut stmt = tx.prepare(&sql)?;
            for row in rows {
                match stmt.execute(params_from_iter(row.iter())) {
                    Ok(_) => inserted += 1,
                    Err(e) if rejected_row(&e) => rejected += 1,
                    Err(e) => return Err(e),
                }
            }
        }
        tx.commit()?;
        Ok((inserted, rejected))
    }
}

// ---------------------------------------------------------------------------------------------------------
// Speicherplatz
// ---------------------------------------------------------------------------------------------------------

/// Bewertung des freien Speichers (CRAWL-124).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskLevel {
    /// Genug Platz.
    Ok,
    /// Weniger als 2 GiB frei: Warnung beim Crawlstart.
    Low,
    /// Weniger als 1 GiB frei: der laufende Crawl pausiert automatisch.
    Critical,
}

/// Freier Speicher auf dem Laufwerk eines Pfads samt Bewertung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskSpaceStatus {
    /// Tatsächlich geprüfter Ordner (der nächste vorhandene Ordner oberhalb des Pfads).
    pub checked: PathBuf,
    /// Freier Speicher für normale Benutzer, in Bytes.
    pub free_bytes: u64,
    /// Bewertung.
    pub level: DiskLevel,
}

impl DiskSpaceStatus {
    /// Beim Crawlstart warnen (weniger als 2 GiB frei).
    pub fn warn_at_start(&self) -> bool {
        self.free_bytes < DISK_WARN_BELOW_BYTES
    }

    /// Den laufenden Crawl pausieren (weniger als 1 GiB frei).
    pub fn should_pause(&self) -> bool {
        self.free_bytes < DISK_PAUSE_BELOW_BYTES
    }

    /// Freier Speicher in GiB (für Meldungen wie „Wenig freier Speicherplatz (x,x GB)“).
    pub fn free_gib(&self) -> f64 {
        self.free_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

/// Bewertet eine Menge freien Speichers.
pub fn classify_free_space(free_bytes: u64) -> DiskLevel {
    if free_bytes < DISK_PAUSE_BELOW_BYTES {
        DiskLevel::Critical
    } else if free_bytes < DISK_WARN_BELOW_BYTES {
        DiskLevel::Low
    } else {
        DiskLevel::Ok
    }
}

/// Freier Speicher auf dem Laufwerk von `path` (CRAWL-124). Existiert der Pfad nicht, wird der nächste
/// vorhandene Ordner darüber geprüft. Auslösung (Startwarnung, Pause alle
/// [`DISK_CHECK_INTERVAL_SECS`] Sekunden) liegt bei der Engine (P19).
pub fn disk_space_status(path: &Path) -> Result<DiskSpaceStatus, StoreError> {
    let mut checked = path;
    while !checked.exists() {
        match checked.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => checked = parent,
            _ => {
                checked = Path::new(".");
                break;
            }
        }
    }
    let free_bytes = free_space(checked).map_err(|e| StoreError::io(checked, e))?;
    Ok(DiskSpaceStatus {
        checked: checked.to_path_buf(),
        free_bytes,
        level: classify_free_space(free_bytes),
    })
}

#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn free_space(path: &Path) -> std::io::Result<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: `c_path` ist eine gültige NUL-terminierte Zeichenkette; `stat` wird von `statvfs` gefüllt.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
    }
}

#[cfg(windows)]
fn free_space(path: &Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available: u64 = 0;
    // SAFETY: `wide` ist NUL-terminiert; die Ausgabeparameter zeigen auf gültige Variablen bzw. sind null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(available)
    }
}

#[cfg(not(any(unix, windows)))]
fn free_space(_path: &Path) -> std::io::Result<u64> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

// ---------------------------------------------------------------------------------------------------------
// Speicherort
// ---------------------------------------------------------------------------------------------------------

/// Hinweis zum gewählten Speicherort (CRAWL-124): Dialog „Anderen Ordner wählen“ / „Trotzdem verwenden“.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocationWarning {
    /// Der Ordner liegt in einem Sync-Dienst (Dropbox, OneDrive, iCloud Drive, Google Drive).
    SyncFolder(&'static str),
    /// Netzlaufwerk (SMB, AFP, NFS, WebDAV, UNC-Pfad): Dateisperren sind unzuverlässig.
    NetworkDrive,
    /// FAT32/exFAT: keine zuverlässigen Dateisperren.
    FatFilesystem,
}

/// Erkennt kritische Speicherorte: Sync-Ordner anhand des Pfads, Netzlaufwerke und FAT (macOS: Dateisystem
/// der Bereitstellung; Windows: UNC-Pfade).
pub fn location_warnings(path: &Path) -> Vec<LocationWarning> {
    let mut out = Vec::new();
    if let Some(service) = sync_service(path) {
        out.push(LocationWarning::SyncFolder(service));
    }
    let text = path.to_string_lossy();
    if (text.starts_with(r"\\") && !text.starts_with(r"\\?\"))
        || text.starts_with("smb://")
        || text.starts_with("nfs://")
        || text.starts_with("afp://")
    {
        out.push(LocationWarning::NetworkDrive);
    }
    for warning in filesystem_warnings(path) {
        if !out.contains(&warning) {
            out.push(warning);
        }
    }
    out
}

fn sync_service(path: &Path) -> Option<&'static str> {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy().to_lowercase();
        if name == "dropbox" || name.starts_with("dropbox (") {
            return Some("Dropbox");
        }
        if name == "onedrive" || name.starts_with("onedrive -") || name.starts_with("onedrive-") {
            return Some("OneDrive");
        }
        if name == "icloud drive" || name == "mobile documents" || name == "com~apple~clouddocs" {
            return Some("iCloud Drive");
        }
        if name == "google drive"
            || name == "googledrive"
            || name == "my drive"
            || name.starts_with("googledrive-")
            || name.starts_with("google drive (")
        {
            return Some("Google Drive");
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn filesystem_warnings(path: &Path) -> Vec<LocationWarning> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let mut existing = path;
    while !existing.exists() {
        match existing.parent() {
            Some(p) if !p.as_os_str().is_empty() => existing = p,
            _ => return Vec::new(),
        }
    }
    let Ok(c_path) = CString::new(existing.as_os_str().as_bytes()) else {
        return Vec::new();
    };
    // SAFETY: gültiger C-String; `stat` wird von `statfs` gefüllt, `f_fstypename` ist NUL-terminiert.
    let name = unsafe {
        let mut stat: libc::statfs = std::mem::zeroed();
        if libc::statfs(c_path.as_ptr(), &mut stat) != 0 {
            return Vec::new();
        }
        std::ffi::CStr::from_ptr(stat.f_fstypename.as_ptr())
            .to_string_lossy()
            .into_owned()
    };
    match name.as_str() {
        "smbfs" | "afpfs" | "nfs" | "webdav" | "cifs" => vec![LocationWarning::NetworkDrive],
        "msdos" | "exfat" => vec![LocationWarning::FatFilesystem],
        _ => Vec::new(),
    }
}

#[cfg(windows)]
fn filesystem_warnings(path: &Path) -> Vec<LocationWarning> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    };
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut volume = vec![0u16; 32768];
    let mut filesystem = [0u16; 32];
    let mut out = Vec::new();
    // SAFETY: NUL-terminierter Eingabepfad und gültige, hinreichend große Ausgabepuffer.
    unsafe {
        if GetVolumePathNameW(wide.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) == 0 {
            return out;
        }
        const DRIVE_REMOTE: u32 = 4;
        if GetDriveTypeW(volume.as_ptr()) == DRIVE_REMOTE {
            out.push(LocationWarning::NetworkDrive);
        }
        if GetVolumeInformationW(
            volume.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        ) != 0
        {
            let end = filesystem
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(filesystem.len());
            let name = String::from_utf16_lossy(&filesystem[..end]);
            if ["FAT", "FAT32", "exFAT"]
                .iter()
                .any(|n| name.eq_ignore_ascii_case(n))
            {
                out.push(LocationWarning::FatFilesystem);
            }
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn filesystem_warnings(path: &Path) -> Vec<LocationWarning> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let mut existing = path;
    while !existing.exists() {
        match existing.parent() {
            Some(p) if !p.as_os_str().is_empty() => existing = p,
            _ => return Vec::new(),
        }
    }
    let Ok(path) = CString::new(existing.as_os_str().as_bytes()) else {
        return Vec::new();
    };
    // SAFETY: gültiger C-String und beschreibbarer statfs-Ausgabepuffer.
    let kind = unsafe {
        let mut stat: libc::statfs = std::mem::zeroed();
        if libc::statfs(path.as_ptr(), &mut stat) != 0 {
            return Vec::new();
        }
        stat.f_type as u64
    };
    match kind {
        0x6969 | 0xff53_4d42 | 0x517b => vec![LocationWarning::NetworkDrive],
        0x4d44 | 0x2011_bab0 => vec![LocationWarning::FatFilesystem],
        _ => Vec::new(),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn filesystem_warnings(_path: &Path) -> Vec<LocationWarning> {
    Vec::new()
}

#[cfg(test)]
mod repair_target_tests {
    //! Schreibfehler am Ziel der Reparatur sind kein Datenverlust der Quelle (B3-FLAKES, Fable-Befund).
    use super::*;
    use std::cell::{Cell, RefCell};

    fn source(rows: i64) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT NOT NULL)")
            .unwrap();
        for id in 1..=rows {
            conn.execute(
                "INSERT INTO t VALUES (?1, ?2)",
                rusqlite::params![id, format!("v{id}")],
            )
            .unwrap();
        }
        conn
    }

    /// Ziel als Datei samt einer zweiten Verbindung, die die Datei exklusiv sperrt (SQLITE_BUSY am Ziel).
    fn locked_target(dir: &Path) -> (Connection, Connection) {
        let path = dir.join("ziel.sqlite");
        let target = Connection::open(&path).unwrap();
        target
            .execute_batch("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT NOT NULL)")
            .unwrap();
        target.busy_timeout(std::time::Duration::ZERO).unwrap();
        let blocker = Connection::open(&path).unwrap();
        blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
        (target, blocker)
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap()
    }

    // spec: OUT-230
    #[test]
    fn a_lasting_target_error_aborts_the_rescue_instead_of_counting_lost_rows() {
        let dir = tempfile::tempdir().unwrap();
        let (target, blocker) = locked_target(dir.path());
        let retries = Cell::new(0);
        let err = rescue_table(&source(25), &target, "t", Some("id"), &|attempt| {
            retries.set(attempt);
        })
        .unwrap_err();
        assert_eq!(
            err.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DatabaseBusy),
            "{err}"
        );
        assert_eq!(
            retries.get(),
            REPAIR_TARGET_ATTEMPTS - 1,
            "begrenzte Wiederholung"
        );
        blocker.execute_batch("ROLLBACK").unwrap();
        assert_eq!(count(&target), 0, "abgebrochener Block wurde zurückgerollt");
        // Ohne Schlüssel (ganze Tabelle in einem Durchlauf) gilt dasselbe.
        blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let err = rescue_table(&source(25), &target, "t", None, &|_| {}).unwrap_err();
        assert_eq!(
            err.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DatabaseBusy),
            "{err}"
        );
    }

    // spec: OUT-230
    #[test]
    fn a_transient_target_error_is_retried_and_loses_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (target, blocker) = locked_target(dir.path());
        let blocker = RefCell::new(Some(blocker));
        let retries = Cell::new(0);
        // Die Sperre endet vor der ersten Wiederholung: Der Block wird vollständig geschrieben.
        let report = rescue_table(&source(25), &target, "t", Some("id"), &|attempt| {
            retries.set(attempt);
            if let Some(b) = blocker.borrow_mut().take() {
                b.execute_batch("ROLLBACK").unwrap();
            }
        })
        .unwrap();
        assert_eq!(retries.get(), 1);
        assert_eq!((report.copied, report.lost), (25, 0));
        assert_eq!(count(&target), 25);
    }

    // spec: OUT-230
    #[test]
    fn rows_the_schema_rejects_still_count_as_lost() {
        // Eine Quellzeile, deren Werte das Schema des Ziels verletzen (hier NOT NULL), ist nicht zu retten.
        let source = Connection::open_in_memory().unwrap();
        source
            .execute_batch(
                "CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT);
                 INSERT INTO t VALUES (1, 'a'), (2, NULL), (3, 'c');",
            )
            .unwrap();
        let target = Connection::open_in_memory().unwrap();
        target
            .execute_batch("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT NOT NULL)")
            .unwrap();
        let report = rescue_table(&source, &target, "t", Some("id"), &|_| {
            panic!("keine Wiederholung bei abgewiesenen Zeilen")
        })
        .unwrap();
        assert_eq!((report.copied, report.lost), (2, 1));
    }
}

//! Projekte, Schutz und Dateioperationen. IDs und Crawl-Warteschlangen bleiben außer beim Duplizieren erhalten.
use super::*;
use rusqlite::{params, OptionalExtension};
use std::collections::HashSet;

/// Dauerhafte Projektkennung und Projektmetadaten.
#[derive(Debug, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub locked: bool,
    pub crawls: u32,
}

/// Verwaltung eines Crawl-Ordners und seiner zusätzlich bekannten Speicherorte.
/// `.registry.sqlite` und `.index.sqlite` liegen immer unter `root` (App: `paths.crawls`),
/// auch wenn importierte oder verschobene Crawl-Dateien an externen Speicherorten liegen.
pub struct Manager {
    root: PathBuf,
    // Dauerhafte Projekt-/Speicherort-Daten liegen getrennt vom verwerfbaren Metadaten-Index.
    state: Connection,
    pub index: MetadataIndex,
}

/// Protokolleintrag einer automatischen Löschung.
#[derive(Debug, Clone)]
pub struct RetentionDeletion {
    pub id: CrawlId,
    pub name: String,
    pub age_days: u64,
}

impl Manager {
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        let root = absolute(root)?;
        let index = MetadataIndex::open(&root)?;
        let state = crate::open::open_sqlite(
            root.join(".registry.sqlite"),
            rusqlite::OpenFlags::default(),
        )?;
        state.busy_timeout(std::time::Duration::from_secs(5))?;
        state.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS projects(id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, locked INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS locations(path TEXT PRIMARY KEY);")?;
        Ok(Self { index, state, root })
    }

    pub(super) fn open_existing(root: &Path) -> Result<Self, StoreError> {
        if !root.join(".registry.sqlite").exists() {
            return Err(StoreError::NotFound(root.into()));
        }
        Self::open(root)
    }

    /// Ein Speicherortwechsel merkt beide Orte, ohne eine Datei zu verschieben.
    pub fn add_location(&self, dir: &Path) -> Result<(), StoreError> {
        let dir = absolute(dir)?;
        self.state.execute(
            "INSERT OR IGNORE INTO locations VALUES (?1)",
            [dir.to_string_lossy().as_ref()],
        )?;
        Ok(())
    }

    pub fn list(&mut self) -> Result<ListingReport, StoreError> {
        let mut dirs = vec![self.root.clone()];
        dirs.extend(
            self.state
                .prepare("SELECT path FROM locations ORDER BY path")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(PathBuf::from),
        );
        let mut paths = Vec::new();
        for dir in dirs {
            paths.extend(candidate_files(&dir)?);
        }
        paths.sort();
        paths.dedup();
        let locked_projects = self
            .state
            .prepare("SELECT name FROM projects WHERE locked=1")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        let tx = self.index.conn.unchecked_transaction()?;
        let mut report = ListingReport::default();
        let mut seen = HashSet::new();
        for path in paths {
            match self.index.read(&path) {
                Ok(mut l) => {
                    l.locked |= locked_projects.contains(&l.meta.project);
                    if seen.insert(l.meta.crawl_id) {
                        report.crawls.push(l);
                    }
                }
                Err(e) => report.skipped.push((path, e)),
            }
        }
        let existing: HashSet<_> = report
            .crawls
            .iter()
            .map(|l| l.path.clone())
            .chain(report.skipped.iter().map(|(p, _)| p.clone()))
            .collect();
        let indexed = self
            .index
            .conn
            .prepare("SELECT path FROM entries")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for path in indexed {
            if !existing.contains(Path::new(&path)) {
                self.index
                    .conn
                    .execute("DELETE FROM entries WHERE path=?1", [path])?;
            }
        }
        tx.commit()?;
        Ok(report)
    }

    pub fn get(&mut self, id: &CrawlId) -> Result<CrawlListing, StoreError> {
        let cached: Option<String> = self.index.conn.query_row(
            "SELECT path FROM entries WHERE json_extract(payload,'$.crawl_id')=?1 ORDER BY path LIMIT 1",
            [id.to_string()], |r|r.get(0)).optional()?;
        if let Some(path) = cached.map(PathBuf::from).filter(|p| p.exists()) {
            let mut l = self.index.read(&path)?;
            if l.meta.crawl_id == *id {
                l.locked |= self.project_locked(&l.meta.project)?;
                return Ok(l);
            }
        }
        self.list()?
            .crawls
            .into_iter()
            .find(|l| l.meta.crawl_id == *id)
            .ok_or_else(|| StoreError::NotFound(path_for(&self.root, id)))
    }

    pub fn projects(&mut self) -> Result<Vec<Project>, StoreError> {
        let report = self.list()?;
        // Auch CLI-/Scheduler-Projektnamen erscheinen im Register.
        for l in &report.crawls {
            if !l.meta.project.is_empty() {
                self.state.execute(
                    "INSERT OR IGNORE INTO projects(id,name) VALUES (?1,?1)",
                    [&l.meta.project],
                )?;
            }
        }
        let mut projects = self
            .state
            .prepare("SELECT id,name,locked FROM projects ORDER BY name,id")?
            .query_map([], |r| {
                Ok(Project {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    locked: r.get(2)?,
                    crawls: 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for p in &mut projects {
            p.crawls = u32::try_from(
                report
                    .crawls
                    .iter()
                    .filter(|l| l.meta.project == p.name)
                    .count(),
            )
            .unwrap_or(u32::MAX);
        }
        Ok(projects)
    }

    pub fn create_project(&mut self, id: &str, name: &str) -> Result<Project, StoreError> {
        validate_name(name)?;
        if self.projects()?.iter().any(|p| p.name == name) {
            return Err(StoreError::InvalidInput("Projekt existiert bereits".into()));
        }
        self.state.execute(
            "INSERT INTO projects(id,name) VALUES (?1,?2)",
            params![id, name],
        )?;
        Ok(Project {
            id: id.into(),
            name: name.into(),
            locked: false,
            crawls: 0,
        })
    }

    fn project(&mut self, id: &str) -> Result<Project, StoreError> {
        self.projects()?
            .into_iter()
            .find(|p| p.id == id || p.name == id)
            .ok_or_else(|| StoreError::InvalidInput("Unbekanntes Projekt".into()))
    }

    fn project_locked(&self, name: &str) -> Result<bool, StoreError> {
        Ok(self
            .state
            .query_row("SELECT locked FROM projects WHERE name=?1", [name], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or(false))
    }

    pub fn lock_project(&mut self, id: &str, locked: bool) -> Result<(), StoreError> {
        let p = self.project(id)?;
        self.state.execute(
            "UPDATE projects SET locked=?1 WHERE id=?2",
            params![locked, p.id],
        )?;
        Ok(())
    }

    pub fn rename_project(
        &mut self,
        id: &str,
        name: &str,
        now: Timestamp,
    ) -> Result<(), StoreError> {
        validate_name(name)?;
        let p = self.project(id)?;
        if p.name == name {
            return Ok(());
        }
        if p.locked {
            return Err(StoreError::InvalidInput("Projekt ist gesperrt".into()));
        }
        if self.projects()?.iter().any(|p| p.name == name) {
            return Err(StoreError::InvalidInput("Projekt existiert bereits".into()));
        }
        let crawls: Vec<_> = self
            .list()?
            .crawls
            .into_iter()
            .filter(|l| l.meta.project == p.name)
            .collect();
        for l in &crawls {
            guard(l, now, false)?;
        }
        // Neue/alte Zuordnung bleibt bei einem Fehler sichtbar; kein Crawl geht verloren.
        for l in &crawls {
            self.put(&l.path, "project", name)?;
        }
        self.state.execute(
            "UPDATE projects SET name=?1 WHERE id=?2",
            params![name, p.id],
        )?;
        Ok(())
    }

    pub fn rename(&mut self, id: &CrawlId, name: &str, now: Timestamp) -> Result<(), StoreError> {
        validate_name(name)?;
        let l = self.get(id)?;
        guard(&l, now, true)?;
        self.put(&l.path, "name", name)
    }

    pub fn lock(&mut self, id: &CrawlId, locked: bool, now: Timestamp) -> Result<(), StoreError> {
        let l = self.get(id)?;
        guard(&l, now, false)?;
        self.put(&l.path, "locked", if locked { "1" } else { "0" })
    }

    pub fn move_project(
        &mut self,
        id: &CrawlId,
        project: Option<&str>,
        now: Timestamp,
    ) -> Result<(), StoreError> {
        let l = self.get(id)?;
        guard(&l, now, false)?;
        let name = match project {
            Some(id) => self.project(id)?.name,
            None => String::new(),
        };
        self.put(&l.path, "project", &name)
    }

    fn put(&self, path: &Path, key: &str, value: &str) -> Result<(), StoreError> {
        let conn = open_raw_write(path)?;
        let found = schema::user_version(&conn)?;
        let supported = schema::latest_version(schema::MIGRATIONS);
        if found > supported {
            return Err(StoreError::NewerVersion {
                path: path.into(),
                found,
                supported,
                generator: "einer neueren OpenSpider-Version".into(),
            });
        }
        conn.execute("INSERT INTO crawl_meta(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
        self.index.invalidate(path)
    }

    pub fn delete(&mut self, id: &CrawlId, now: Timestamp) -> Result<(), StoreError> {
        let l = self.get(id)?;
        guard(&l, now, true)?;
        // Checkpoint vor dem Löschen: keine Datenbankverbindung der Verwaltung bleibt offen.
        let conn = open_raw_write(&l.path)?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        drop(conn);
        std::fs::remove_file(&l.path).map_err(|e| StoreError::io(&l.path, e))?;
        for suffix in ["-wal", "-shm", "-journal"] {
            let p = companion(&l.path, suffix);
            if let Err(e) = std::fs::remove_file(&p) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(StoreError::io(&p, e));
                }
            }
        }
        self.index.conn.execute(
            "DELETE FROM entries WHERE path=?1",
            [l.path.to_string_lossy().as_ref()],
        )?;
        self.index.invalidate(&l.path)
    }

    pub fn import(
        &mut self,
        src: &Path,
        dir: &Path,
        ids: &dyn IdSource,
        clock: &dyn Clock,
        progress: &(dyn Fn(u64, u64) -> bool + Sync),
    ) -> Result<PathBuf, StoreError> {
        let source = read_listing(src)?;
        let known = self.list()?.crawls;
        let mut id = source.meta.crawl_id;
        if known.iter().any(|l| l.meta.crawl_id == id) || path_for(dir, &id).exists() {
            id = fresh_id(dir, &known, ids, clock)?;
        }
        let dest = path_for(dir, &id);
        copy_snapshot(src, &dest, progress)?;
        if id != source.meta.crawl_id {
            if let Err(e) = self.put(&dest, "crawl_id", &id.to_string()) {
                let _ = std::fs::remove_file(&dest);
                return Err(e);
            }
        }
        self.add_location(dir)?;
        Ok(dest)
    }

    pub fn duplicate(
        &mut self,
        id: &CrawlId,
        dir: &Path,
        ids: &dyn IdSource,
        clock: &dyn Clock,
        progress: &(dyn Fn(u64, u64) -> bool + Sync),
    ) -> Result<PathBuf, StoreError> {
        let l = self.get(id)?;
        guard(&l, clock.now(), false)?;
        let new_id = fresh_id(dir, &self.list()?.crawls, ids, clock)?;
        let dest = path_for(dir, &new_id);
        copy_snapshot(&l.path, &dest, progress)?;
        let result = (|| {
            self.put(&dest, "crawl_id", &new_id.to_string())?;
            self.put(&dest, "name", &format!("{} (Kopie)", l.meta.name))?;
            self.add_location(dir)
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&dest);
            return Err(e);
        }
        Ok(dest)
    }

    pub fn export(
        &mut self,
        id: &CrawlId,
        dest: &Path,
        now: Timestamp,
        progress: &(dyn Fn(u64, u64) -> bool + Sync),
    ) -> Result<(), StoreError> {
        let l = self.get(id)?;
        guard(&l, now, false)?;
        copy_snapshot(&l.path, dest, progress)
    }

    /// Kopieren, Integrität und Identität prüfen, Ziel registrieren, erst danach Quelle löschen.
    pub fn move_file(
        &mut self,
        id: &CrawlId,
        dir: &Path,
        now: Timestamp,
        progress: &(dyn Fn(u64, u64) -> bool + Sync),
    ) -> Result<PathBuf, StoreError> {
        let l = self.get(id)?;
        guard(&l, now, false)?;
        let dir = absolute(dir)?;
        let dest = path_for(&dir, id);
        if dest == l.path {
            return Ok(dest);
        }
        copy_snapshot(&l.path, &dest, progress)?;
        let result = (|| {
            crate::files::verify(&dest)?;
            let copied = read_listing(&dest)?;
            if copied.meta != l.meta || copied.url_count != l.url_count || copied.queued != l.queued
            {
                return Err(StoreError::InvalidInput(
                    "Kopie stimmt nicht mit Quelle überein".into(),
                ));
            }
            self.add_location(&dir)?;
            self.index.invalidate(&l.path)?;
            self.index.read(&dest)?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&dest);
            return Err(e);
        }
        // Auf Windows verhindert eine fremde Verbindung das Löschen. Dann bleibt die Quelle erhalten.
        if let Err(e) = std::fs::remove_file(&l.path) {
            let _ = std::fs::remove_file(&dest);
            return Err(StoreError::io(&l.path, e));
        }
        for suffix in ["-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(companion(&l.path, suffix));
        }
        Ok(dest)
    }

    /// Zeitgesteuerte Aufbewahrung mit injiziertem Zeitpunkt und offener Sitzungsmenge.
    pub fn retain(
        &mut self,
        days: Option<u32>,
        keep_last: Option<u32>,
        now: Timestamp,
        open: &HashSet<CrawlId>,
    ) -> Result<Vec<RetentionDeletion>, StoreError> {
        self.retain_with(days, keep_last, now, open, |_| {})
    }

    /// Benachrichtigt nach jeder erfolgreichen Löschung, auch wenn eine spätere Datei scheitert.
    pub fn retain_with(
        &mut self,
        days: Option<u32>,
        keep_last: Option<u32>,
        now: Timestamp,
        open: &HashSet<CrawlId>,
        mut deleted: impl FnMut(&RetentionDeletion),
    ) -> Result<Vec<RetentionDeletion>, StoreError> {
        let mut eligible: Vec<_> = self
            .list()?
            .crawls
            .into_iter()
            .filter(|l| {
                !l.locked
                    && !open.contains(&l.meta.crawl_id)
                    && l.effective_state(now) != CrawlState::Running
            })
            .collect();
        let reference = |l: &CrawlListing| {
            if matches!(
                l.effective_state(now),
                CrawlState::Completed | CrawlState::Failed
            ) {
                l.meta
                    .ended_at
                    .map(|t| t.unix_millis())
                    .unwrap_or(l.modified_ms)
            } else {
                l.modified_ms
            }
        };
        eligible.sort_by_key(|l| (reference(l), l.meta.crawl_id.to_string()));
        let excess = keep_last
            .map(|n| eligible.len().saturating_sub(n as usize))
            .unwrap_or(0);
        let mut removed = Vec::new();
        for (i, l) in eligible.into_iter().enumerate() {
            let age = now.unix_millis().saturating_sub(reference(&l)).max(0) as u64;
            if days.is_some_and(|d| age >= u64::from(d) * 86_400_000) || i < excess {
                self.delete(&l.meta.crawl_id, now)?;
                let entry = RetentionDeletion {
                    id: l.meta.crawl_id,
                    name: l.meta.name,
                    age_days: age / 86_400_000,
                };
                deleted(&entry);
                removed.push(entry);
            }
        }
        Ok(removed)
    }
}

fn guard(l: &CrawlListing, now: Timestamp, protect: bool) -> Result<(), StoreError> {
    if l.effective_state(now) == CrawlState::Running {
        return Err(StoreError::InvalidInput(
            "Crawl läuft – erst pausieren".into(),
        ));
    }
    if protect && l.locked {
        return Err(StoreError::InvalidInput("Crawl ist gesperrt".into()));
    }
    Ok(())
}
fn validate_name(name: &str) -> Result<(), StoreError> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(StoreError::InvalidInput(
            "Name darf nicht leer sein oder Steuerzeichen enthalten".into(),
        ));
    }
    Ok(())
}
fn absolute(path: &Path) -> Result<PathBuf, StoreError> {
    if path.is_absolute() {
        Ok(path.into())
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| StoreError::io(path, e))?
            .join(path))
    }
}
fn fresh_id(
    dir: &Path,
    known: &[CrawlListing],
    ids: &dyn IdSource,
    clock: &dyn Clock,
) -> Result<CrawlId, StoreError> {
    for _ in 0..1024 {
        let id = ids.new_crawl_id(clock.now());
        if !path_for(dir, &id).exists() && !known.iter().any(|l| l.meta.crawl_id == id) {
            return Ok(id);
        }
    }
    Err(StoreError::InvalidInput(
        "Kennungsquelle liefert keine freie ID".into(),
    ))
}

/// SQLite-Snapshot einschließlich WAL, Fortschritt/Interrupt alle 20 ms, atomare Veröffentlichung ohne Überschreiben.
pub fn copy_snapshot(
    src: &Path,
    dest: &Path,
    progress: &(dyn Fn(u64, u64) -> bool + Sync),
) -> Result<(), StoreError> {
    copy_snapshot_checked(src, dest, progress, &copy_snapshot_from)
}

type CopyFrom<'a> = &'a dyn Fn(
    &Connection,
    &Path,
    &Path,
    &(dyn Fn(u64, u64) -> bool + Sync),
    &dyn Fn() -> bool,
) -> Result<bool, StoreError>;

/// Ein Lesefehler der Quelle, nicht Abbruch durch den Nutzer und kein Fehler am Ziel.
fn is_source_read_error(e: &StoreError) -> bool {
    match e {
        StoreError::Sqlite(rusqlite::Error::SqliteFailure(f, _)) => {
            f.code != rusqlite::ErrorCode::OperationInterrupted
        }
        StoreError::Io { .. } | StoreError::AlreadyExists(_) => false,
        _ => true,
    }
}

/// Ohne Sperren gilt nur eine fehlerfreie Kopie, nach der kein Schreiber begonnen hat (`MetadataConn`): Ein Lesefehler erscheint
/// nie als beschädigte Quelle, sondern wird unter Sperren wiederholt. `copy` ist die Testnaht.
fn copy_snapshot_checked(
    src: &Path,
    dest: &Path,
    progress: &(dyn Fn(u64, u64) -> bool + Sync),
    copy: CopyFrom<'_>,
) -> Result<(), StoreError> {
    let source = open_raw_metadata(src)?;
    let valid = || source.still_valid(src);
    match copy(&source.conn, src, dest, progress, &valid) {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(e) if source.is_locked() || !is_source_read_error(&e) => return Err(e),
        Err(_) => {}
    }
    drop(source);
    let source = crate::files::open_raw_readonly(src)?;
    copy(&source, src, dest, progress, &|| true).map(|_| ())
}

/// Liefert `Ok(false)` ohne Veröffentlichung, wenn `valid` nach dem Kopieren die Quelle verwirft.
fn copy_snapshot_from(
    source: &Connection,
    src: &Path,
    dest: &Path,
    progress: &(dyn Fn(u64, u64) -> bool + Sync),
    valid: &dyn Fn() -> bool,
) -> Result<bool, StoreError> {
    read_meta(source).map_err(|e| classify_error(src, e))?;
    let total = file_stats(src)?.0;
    if !progress(0, total) {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_INTERRUPT),
            None,
        )
        .into());
    }
    if dest.exists() {
        return Err(StoreError::AlreadyExists(dest.into()));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StoreError::io(parent, e))?;
    }
    // Exklusive Reservierung verhindert Kollisionen mit gleichzeitig laufenden Exporten.
    let tmp = companion(dest, ".copying");
    let reservation = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| StoreError::io(&tmp, e))?;
    drop(reservation);
    let done = std::sync::atomic::AtomicBool::new(false);
    let interrupt = source.get_interrupt_handle();
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(std::sync::atomic::Ordering::Acquire) {
                let copied = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
                if !progress(copied.min(total), total) {
                    interrupt.interrupt();
                }
                std::thread::park_timeout(std::time::Duration::from_millis(20));
            }
        });
        let result = source
            .execute("VACUUM INTO ?1", [tmp.to_string_lossy().as_ref()])
            .map(|_| ())
            .map_err(StoreError::from);
        done.store(true, std::sync::atomic::Ordering::Release);
        result
    })
    .and_then(|()| {
        // Schreibzugriff nötig: FlushFileBuffers verweigert unter Windows ein nur lesend geöffnetes Handle.
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(&tmp)
            .map_err(|e| StoreError::io(&tmp, e))?;
        f.sync_all().map_err(|e| StoreError::io(&tmp, e))?;
        drop(f);
        if !valid() {
            return Ok(false);
        }
        if !progress(total, total) {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_INTERRUPT),
                None,
            )
            .into());
        }
        publish(&tmp, dest).map_err(|e| StoreError::io(dest, e))?;
        Ok(true)
    });
    let _ = std::fs::remove_file(&tmp);
    result
}

// Exklusive Veröffentlichung, sofern das Dateisystem sie unterstützt. Sonst schützt die
// `.copying`-Reservierung vor anderen OpenSpider-Kopien; externe Schreiber dürfen das Ziel
// zwischen Existenzprüfung und Rename nicht belegen. Keine Hardlinks (FAT/exFAT).
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn publish(src: &Path, dest: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c_src = std::ffi::CString::new(src.as_os_str().as_bytes())?;
    let c_dest = std::ffi::CString::new(dest.as_os_str().as_bytes())?;
    // SAFETY: beide Pfade sind gültige, NUL-terminierte C-Strings.
    let result = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::renamex_np(c_src.as_ptr(), c_dest.as_ptr(), libc::RENAME_EXCL)
        }
        #[cfg(target_os = "linux")]
        {
            libc::renameat2(
                libc::AT_FDCWD,
                c_src.as_ptr(),
                libc::AT_FDCWD,
                c_dest.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        }
    };
    let result = if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    };
    publish_or_fallback(src, dest, result)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn publish_or_fallback(
    src: &Path,
    dest: &Path,
    result: std::io::Result<()>,
) -> std::io::Result<()> {
    match result {
        Err(e)
            if e.raw_os_error().is_some_and(|code| {
                [libc::ENOTSUP, libc::EINVAL, libc::ENOSYS, libc::EOPNOTSUPP].contains(&code)
            }) =>
        {
            // try_exists vermeidet ein Überschreiben auch bei Fehlern der Existenzprüfung.
            if dest.try_exists()? {
                Err(std::io::ErrorKind::AlreadyExists.into())
            } else {
                std::fs::rename(src, dest)
            }
        }
        other => other,
    }
}

#[cfg(windows)]
fn publish(src: &Path, dest: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let src: Vec<u16> = src.as_os_str().encode_wide().chain(Some(0)).collect();
    let dest: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: beide Pfade sind gültig und NUL-terminiert. Ohne REPLACE_EXISTING bleibt ein vorhandenes Ziel erhalten.
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(src.as_ptr(), dest.as_ptr(), 0)
    };
    if result != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn publish(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::hard_link(src, dest)
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn no_progress_limit(_: u64, _: u64) -> bool {
        true
    }

    // Ein Lesefehler ohne Sperren erscheint nicht als beschädigte Quelle; die Kopie entsteht unter Sperren.
    // spec: OUT-228
    // spec: CRAWL-125
    #[test]
    fn a_copy_read_error_without_locks_is_repeated_under_locks() {
        let dir = tempfile::tempdir().unwrap();
        let src = crate::registry::tests::crawl(dir.path());
        let dest = dir.path().join("kopie.ospider");
        let calls = Cell::new(0);
        copy_snapshot_checked(
            &src,
            &dest,
            &no_progress_limit,
            &|source, s, d, p, valid| {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    return Err(rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
                        Some("zerrissen".into()),
                    )
                    .into());
                }
                copy_snapshot_from(source, s, d, p, valid)
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(read_listing(&dest).unwrap().meta.name, "alt");
    }

    // Beginnt während der Kopie ein Schreiber, wird nichts veröffentlicht, bevor unter Sperren neu kopiert wurde.
    // spec: OUT-228
    // spec: CRAWL-125
    #[test]
    fn a_copy_invalidated_by_a_writer_is_not_published_and_repeated_under_locks() {
        let dir = tempfile::tempdir().unwrap();
        let src = crate::registry::tests::crawl(dir.path());
        let dest = dir.path().join("kopie.ospider");
        let calls = Cell::new(0);
        let owner = Cell::new(None);
        copy_snapshot_checked(
            &src,
            &dest,
            &no_progress_limit,
            &|source, s, d, p, valid| {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    let file = CrawlFile::open(&src, OpenMode::ReadWrite).unwrap();
                    file.set_meta("name", "neu").unwrap();
                    owner.set(Some(file));
                    let published = copy_snapshot_from(source, s, d, p, valid)?;
                    assert!(!published, "die Quelle hat sich geändert");
                    assert!(!d.exists());
                    return Ok(published);
                }
                copy_snapshot_from(source, s, d, p, valid)
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(read_listing(&dest).unwrap().meta.name, "neu");
        drop(owner.take());
    }

    // Abbruch durch den Nutzer und ein belegtes Ziel sind keine Lesefehler der Quelle: kein zweiter Versuch.
    // spec: OUT-228
    #[test]
    fn a_cancelled_copy_and_an_occupied_target_are_not_repeated() {
        let dir = tempfile::tempdir().unwrap();
        let src = crate::registry::tests::crawl(dir.path());
        let dest = dir.path().join("kopie.ospider");
        let calls = Cell::new(0);
        let real: CopyFrom<'_> = &|source, s, d, p, valid| {
            calls.set(calls.get() + 1);
            copy_snapshot_from(source, s, d, p, valid)
        };
        let cancel = |_: u64, _: u64| false;
        let err = copy_snapshot_checked(&src, &dest, &cancel, real).unwrap_err();
        assert!(!is_source_read_error(&err), "{err}");
        assert_eq!(calls.get(), 1);
        assert!(!dest.exists());

        calls.set(0);
        std::fs::write(&dest, "belegt").unwrap();
        let err = copy_snapshot_checked(&src, &dest, &no_progress_limit, real).unwrap_err();
        assert!(matches!(err, StoreError::AlreadyExists(_)), "{err}");
        assert_eq!(calls.get(), 1);
        assert_eq!(std::fs::read(&dest).unwrap(), b"belegt");
    }

    // spec: CRAWL-127 [M2]
    #[test]
    fn unsupported_exclusive_rename_falls_back_without_replacing_existing_destinations() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("crawl.ospider.copying");
        let dest = dir.path().join("crawl.ospider");
        for code in [libc::ENOTSUP, libc::EINVAL, libc::ENOSYS, libc::EOPNOTSUPP] {
            std::fs::write(&src, b"snapshot").unwrap();
            publish_or_fallback(&src, &dest, Err(std::io::Error::from_raw_os_error(code))).unwrap();
            assert_eq!(std::fs::read(&dest).unwrap(), b"snapshot");
            assert!(!src.exists());
            std::fs::write(&src, b"replacement").unwrap();
            let error =
                publish_or_fallback(&src, &dest, Err(std::io::Error::from_raw_os_error(code)))
                    .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
            assert_eq!(std::fs::read(&dest).unwrap(), b"snapshot");
            assert_eq!(std::fs::read(&src).unwrap(), b"replacement");
            std::fs::remove_file(&dest).unwrap();
        }
        let error = publish_or_fallback(
            &src,
            &dest,
            Err(std::io::Error::from_raw_os_error(libc::EACCES)),
        )
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EACCES));
        assert!(src.exists());
        assert!(!dest.exists());
    }
}

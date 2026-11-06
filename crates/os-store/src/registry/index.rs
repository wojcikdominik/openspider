//! Persistenter Metadaten-Index. Unveränderte Crawl-Dateien werden nicht mit SQLite geöffnet.
use super::*;
use rusqlite::{params, OptionalExtension};

/// Separater SQLite-Index; sein Verlust löscht keine Crawls.
pub struct MetadataIndex {
    pub(super) conn: Connection,
}

impl MetadataIndex {
    /// Öffnet bzw. erstellt den Index im Verwaltungsordner.
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        std::fs::create_dir_all(root).map_err(|e| StoreError::io(root, e))?;
        let conn =
            crate::open::open_sqlite(root.join(".index.sqlite"), rusqlite::OpenFlags::default())?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS entries(path TEXT PRIMARY KEY, fingerprint TEXT, payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS registry_meta(key TEXT PRIMARY KEY, value INTEGER NOT NULL);
            INSERT OR IGNORE INTO registry_meta VALUES ('generation',0);")?;
        Ok(Self { conn })
    }

    /// Eigener Änderungszähler, zusätzlich zu Datei- und WAL-Fingerprints (K2-54).
    pub fn invalidate(&self, path: &Path) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE entries SET fingerprint=NULL WHERE path=?1",
            [path.to_string_lossy().as_ref()],
        )?;
        tx.execute(
            "UPDATE registry_meta SET value=value+1 WHERE key='generation'",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Anzahl eigener Änderungen, zum Invalidieren höherer Ansichten.
    pub fn generation(&self) -> Result<u64, StoreError> {
        Ok(self.conn.query_row(
            "SELECT value FROM registry_meta WHERE key='generation'",
            [],
            |r| r.get(0),
        )?)
    }

    pub(super) fn read(&self, path: &Path) -> Result<CrawlListing, StoreError> {
        let before = fingerprint(path)?;
        let cached: Option<(Option<String>, String)> = self
            .conn
            .query_row(
                "SELECT fingerprint,payload FROM entries WHERE path=?1",
                [path.to_string_lossy().as_ref()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((Some(stamp), payload)) = cached {
            if stamp == before {
                if let Ok(listing) = decode(path, &payload) {
                    return Ok(listing);
                }
            }
        }
        let listing = read_listing(path)?;
        let after = fingerprint(path)?;
        // Ein gleichzeitig schreibender Crawl wird beim nächsten Lesen erneut abgefragt.
        if before == after {
            self.conn.execute("INSERT INTO entries VALUES (?1,?2,?3) ON CONFLICT(path) DO UPDATE SET fingerprint=excluded.fingerprint,payload=excluded.payload",
                params![path.to_string_lossy(), after, encode(&listing)])?;
        }
        Ok(listing)
    }
}

fn fingerprint(path: &Path) -> Result<String, StoreError> {
    let header = crate::files::probe_header(path)?;
    // Version of the cached payload: `n28` = `url_count` without auxiliary fetches (UI-061). Entries
    // written before carry no version and are read again once.
    let mut stamp = String::from("n28;");
    for p in [path.to_path_buf(), companion(path, "-wal")] {
        match std::fs::metadata(&p) {
            Ok(m) => stamp.push_str(&format!(
                "{}:{:?};",
                m.len(),
                m.modified().map_err(|e| StoreError::io(&p, e))?
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && p != path => stamp.push_str("-;"),
            Err(e) => return Err(StoreError::io(&p, e)),
        }
    }
    // SQLite-Dateiänderungszähler: erfasst auch ersetzte Dateien mit zurückgesetztem mtime.
    stamp.push_str(&format!("{:?}", &header[24..40]));
    Ok(stamp)
}

fn encode(l: &CrawlListing) -> String {
    serde_json::json!({
        "crawl_id": l.meta.crawl_id.to_string(), "mode": l.meta.mode.as_str(),
        "start_url": l.meta.start_url.clone().unwrap_or_default(), "project": l.meta.project, "name": l.meta.name,
        "state": l.meta.state.as_str(), "end_reason": l.meta.end_reason.map(|v| v.as_str()).unwrap_or_default(),
        "started_at": l.meta.started_at.map(|v| v.to_db()).unwrap_or_default(),
        "ended_at": l.meta.ended_at.map(|v| v.to_db()).unwrap_or_default(),
        "heartbeat_at": l.meta.heartbeat_at.map(|v| v.to_db()).unwrap_or_default(),
        "schema_version": l.meta.schema_version.to_string(), "url_count": l.url_count.to_string(),
        "locked": l.locked.to_string(), "file_bytes": l.file_bytes.to_string(), "modified_ms": l.modified_ms.to_string(),
        "created_at": l.created_at.map(|v|v.to_db()).unwrap_or_default(),
        "queued": l.queued.to_string(), "duration_ms": l.duration_ms.map(|v|v.to_string()).unwrap_or_default()
    }).to_string()
}

fn decode(path: &Path, payload: &str) -> Result<CrawlListing, StoreError> {
    let m: HashMap<String, String> =
        serde_json::from_str(payload).map_err(|e| StoreError::InvalidInput(e.to_string()))?;
    fn number<T: FromStr>(m: &HashMap<String, String>, k: &str) -> Result<T, StoreError> {
        m.get(k)
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| StoreError::MissingMeta(k.into()))
    }
    Ok(CrawlListing {
        path: path.to_path_buf(),
        meta: CrawlMeta {
            crawl_id: parse(&m, "crawl_id")?,
            mode: parse(&m, "mode")?,
            start_url: m.get("start_url").filter(|v| !v.is_empty()).cloned(),
            project: m
                .get("project")
                .cloned()
                .ok_or_else(|| StoreError::MissingMeta("project".into()))?,
            name: m
                .get("name")
                .cloned()
                .ok_or_else(|| StoreError::MissingMeta("name".into()))?,
            state: parse(&m, "state")?,
            end_reason: parse_opt(&m, "end_reason")?,
            started_at: parse_ts(&m, "started_at")?,
            ended_at: parse_ts(&m, "ended_at")?,
            heartbeat_at: parse_ts(&m, "heartbeat_at")?,
            schema_version: number(&m, "schema_version")?,
        },
        url_count: number(&m, "url_count")?,
        locked: number(&m, "locked")?,
        file_bytes: number(&m, "file_bytes")?,
        modified_ms: number(&m, "modified_ms")?,
        queued: number(&m, "queued")?,
        duration_ms: m.get("duration_ms").and_then(|v| v.parse().ok()),
        created_at: parse_ts(&m, "created_at")?,
    })
}

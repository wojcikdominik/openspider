//! Ergebnis-Cache `ai_cache` (AI-015): Speichern, Lesen, Leeren, Statistik und Übernahme aus einer anderen
//! Crawl-Datei.
//!
//! Der Cache-Schlüssel und die Regeln, *was* gespeichert wird, liegen in `os-ai::cache`; diese Schicht prüft nur die
//! Invariante „nur erfolgreiche Ergebnisse und Refusals, nie Fehler“ und führt die SQL-Zugriffe aus. Die Spalte
//! `status` trägt den kanonischen Statustext (`Success` bzw. `Refusal`); das ist das Refusal-Kennzeichen.
use std::path::Path;

use crate::error::StoreError;

/// Tabellenname (stabil über Paketübergaben).
pub const TABLE: &str = "ai_cache";

/// Verbindungstyp der Cache-Funktionen, damit Aufrufer ohne eigene `rusqlite`-Abhängigkeit ihn benennen können.
pub type Connection = rusqlite::Connection;

/// Statustext eines gespeicherten erfolgreichen Ergebnisses (AI-020).
pub const STATUS_SUCCESS: &str = "Success";
/// Statustext eines gespeicherten Refusals (AI-020); Refusals werden „mit Kennzeichen“ gecacht (AI-015).
pub const STATUS_REFUSAL: &str = "Refusal";

/// Eine Zeile von `ai_cache`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheRow {
    /// SHA-256 (hex) über die kanonische Schlüsseldarstellung (AI-015).
    pub cache_key: String,
    pub provider: String,
    pub model: String,
    /// Endpunkt-Basis-URL (global/EU).
    pub endpoint: String,
    /// Frage-, Prompt- bzw. Aufgabenvorlagen-ID.
    pub definition_id: String,
    pub definition_version: String,
    /// SHA-256 (hex) der tatsächlich gesendeten Eingabe.
    pub input_hash: String,
    /// SHA-256 (hex) der kanonischen Vorlagenparameter.
    pub params_hash: String,
    /// Ergebnis als JSON-Text (bei Refusals z. B. der Refusal-Grund oder `null`).
    pub result_json: String,
    /// [`STATUS_SUCCESS`] oder [`STATUS_REFUSAL`].
    pub status: String,
    /// Datenbankform des Zeitstempels (`Timestamp::to_db`).
    pub created_at: String,
}

impl CacheRow {
    /// `true` für einen gecachten Refusal.
    pub fn is_refusal(&self) -> bool {
        self.status == STATUS_REFUSAL
    }
}

/// Bestand des Caches in einer Crawl-Datei.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheTableStats {
    /// Alle Einträge.
    pub entries: u64,
    /// Davon Refusals.
    pub refusals: u64,
    /// Summe der gespeicherten Bytes aller Spalten (UTF-8), ohne Index- und Seitenverwaltung von SQLite.
    pub size_bytes: u64,
}

/// Ergebnis einer Übernahme aus einer anderen Crawl-Datei.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheImport {
    /// In der Quelle gelesene Einträge (nur `Success` und `Refusal`).
    pub read: u64,
    /// Neu übernommene Einträge.
    pub imported: u64,
    /// Bereits vorhandene Schlüssel; der vorhandene Eintrag der Zieldatei bleibt.
    pub kept_existing: u64,
}

const COLUMNS: &str =
    "cache_key, provider, model, endpoint, definition_id, definition_version, input_hash, \
                       params_hash, result_json, status, created_at";

fn read_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CacheRow> {
    Ok(CacheRow {
        cache_key: r.get(0)?,
        provider: r.get(1)?,
        model: r.get(2)?,
        endpoint: r.get(3)?,
        definition_id: r.get(4)?,
        definition_version: r.get(5)?,
        input_hash: r.get(6)?,
        params_hash: r.get(7)?,
        result_json: r.get(8)?,
        status: r.get(9)?,
        created_at: r.get(10)?,
    })
}

/// Liest den Eintrag zu `cache_key`.
pub fn get(conn: &Connection, cache_key: &str) -> Result<Option<CacheRow>, StoreError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM ai_cache WHERE cache_key = ?1"
    ))?;
    let mut rows = stmt.query([cache_key])?;
    match rows.next()? {
        Some(row) => Ok(Some(read_row(row)?)),
        None => Ok(None),
    }
}

/// Speichert einen Eintrag (ersetzt einen vorhandenen mit demselben Schlüssel, z. B. ein erneut angefragtes
/// Refusal). Andere Statuswerte als `Success`/`Refusal` werden abgewiesen: Fehler werden nie gecacht (AI-015).
pub fn put(conn: &Connection, row: &CacheRow) -> Result<(), StoreError> {
    if row.status != STATUS_SUCCESS && row.status != STATUS_REFUSAL {
        return Err(StoreError::InvalidInput(format!(
            "ai_cache speichert nur Success und Refusal, nicht {:?}",
            row.status
        )));
    }
    conn.prepare_cached(&format!(
        "INSERT OR REPLACE INTO ai_cache ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
    ))?
    .execute(rusqlite::params![
        row.cache_key,
        row.provider,
        row.model,
        row.endpoint,
        row.definition_id,
        row.definition_version,
        row.input_hash,
        row.params_hash,
        row.result_json,
        row.status,
        row.created_at,
    ])?;
    Ok(())
}

/// Entfernt einen einzelnen Eintrag; `true`, wenn es ihn gab.
pub fn remove(conn: &Connection, cache_key: &str) -> Result<bool, StoreError> {
    Ok(conn.execute("DELETE FROM ai_cache WHERE cache_key = ?1", [cache_key])? > 0)
}

/// „Cache für Aufgabe/Frage leeren“: alle Einträge einer Frage-, Prompt- bzw. Aufgabenvorlagen-ID, optional nur
/// einer Version. Liefert die Zahl gelöschter Einträge.
pub fn clear_definition(
    conn: &Connection,
    definition_id: &str,
    definition_version: Option<&str>,
) -> Result<u64, StoreError> {
    let n = match definition_version {
        Some(version) => conn.execute(
            "DELETE FROM ai_cache WHERE definition_id = ?1 AND definition_version = ?2",
            [definition_id, version],
        )?,
        None => conn.execute(
            "DELETE FROM ai_cache WHERE definition_id = ?1",
            [definition_id],
        )?,
    };
    Ok(n as u64)
}

/// Leert den ganzen Cache; liefert die Zahl gelöschter Einträge.
pub fn clear_all(conn: &Connection) -> Result<u64, StoreError> {
    Ok(conn.execute("DELETE FROM ai_cache", [])? as u64)
}

/// Einträge, Refusals und Größe (Cache-Statistik im KI-Protokoll, AI-015).
pub fn stats(conn: &Connection) -> Result<CacheTableStats, StoreError> {
    let (entries, refusals, size) = conn.query_row(
        "SELECT count(*), \
                coalesce(sum(status = 'Refusal'), 0), \
                coalesce(sum(length(CAST(cache_key AS BLOB)) + length(CAST(provider AS BLOB)) \
                    + length(CAST(model AS BLOB)) + length(CAST(endpoint AS BLOB)) \
                    + length(CAST(definition_id AS BLOB)) + length(CAST(definition_version AS BLOB)) \
                    + length(CAST(input_hash AS BLOB)) + length(CAST(params_hash AS BLOB)) \
                    + length(CAST(result_json AS BLOB)) + length(CAST(status AS BLOB)) \
                    + length(CAST(created_at AS BLOB))), 0) \
         FROM ai_cache",
        [],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
    )?;
    Ok(CacheTableStats {
        entries: entries.max(0) as u64,
        refusals: refusals.max(0) as u64,
        size_bytes: size.max(0) as u64,
    })
}

fn has_cache_table(conn: &Connection) -> Result<bool, StoreError> {
    Ok(conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'ai_cache'",
        [],
        |r| r.get::<_, i64>(0),
    )? > 0)
}

fn read_source_rows(conn: &Connection) -> Result<Vec<CacheRow>, StoreError> {
    if !has_cache_table(conn)? {
        // Ältere Crawl-Datei (Schema < 6): kein KI-Cache vorhanden.
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM ai_cache WHERE status IN ('Success', 'Refusal') ORDER BY cache_key"
    ))?;
    let rows = stmt
        .query_map([], read_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// „KI-Cache aus anderer Crawl-Datei übernehmen…“ (AI-015): Liest den Cache von `source` und übernimmt alle
/// Einträge, deren Schlüssel in `conn` noch fehlt. Die Quelle wird **nur gelesen**, ohne Migration, ohne Schreibrechte
/// und – wenn kein Schreiber aktiv ist – ohne Sperren und ohne Begleitdateien (`immutable`); sie bleibt byte-identisch.
pub fn import_from(conn: &Connection, source: &Path) -> Result<CacheImport, StoreError> {
    let source_conn = crate::files::open_raw_metadata(source)?;
    let mut rows = read_source_rows(&source_conn.conn);
    // Ergebnisse unter Sperren gelten so, wie sie sind; `immutable` sieht einen gleichzeitigen Schreiber nicht, dann
    // wird unter Sperren neu gelesen.
    if !source_conn.is_locked() && (rows.is_err() || !source_conn.still_valid(source)) {
        drop(source_conn);
        let locked = crate::files::open_raw_readonly(source)?;
        rows = read_source_rows(&locked);
    }
    let rows = rows?;
    let mut report = CacheImport {
        read: rows.len() as u64,
        ..CacheImport::default()
    };
    let tx = conn.unchecked_transaction()?;
    {
        let mut insert = tx.prepare(&format!(
            "INSERT OR IGNORE INTO ai_cache ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
        ))?;
        for row in &rows {
            let n = insert.execute(rusqlite::params![
                row.cache_key,
                row.provider,
                row.model,
                row.endpoint,
                row.definition_id,
                row.definition_version,
                row.input_hash,
                row.params_hash,
                row.result_json,
                row.status,
                row.created_at,
            ])?;
            if n > 0 {
                report.imported += 1;
            } else {
                report.kept_existing += 1;
            }
        }
    }
    tx.commit()?;
    Ok(report)
}

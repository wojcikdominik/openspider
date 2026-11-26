//! `crawl_meta`: Metadaten einer Crawl-Datei.

use std::collections::HashMap;
use std::str::FromStr;

use os_model::{CrawlId, CrawlMode, CrawlState, EndReason, Timestamp};

use crate::error::StoreError;
use crate::open::CrawlFile;
use crate::schema;

/// Gelesene Metadaten (`crawl_meta`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlMeta {
    /// Stabile Kennung (UUID v7).
    pub crawl_id: CrawlId,
    /// Modus.
    pub mode: CrawlMode,
    /// Start-URL (Spider-Modus).
    pub start_url: Option<String>,
    /// Projektname.
    pub project: String,
    /// Crawl-Name.
    pub name: String,
    /// Gespeicherter Zustand. Ob ein „Running“ noch lebt, entscheidet `effective_state` (M1-P07).
    pub state: CrawlState,
    /// Grund des Endes.
    pub end_reason: Option<EndReason>,
    /// Startzeitpunkt.
    pub started_at: Option<Timestamp>,
    /// Endzeitpunkt.
    pub ended_at: Option<Timestamp>,
    /// Letzter Heartbeat der Engine.
    pub heartbeat_at: Option<Timestamp>,
    /// Art des (letzten) Besitzers (`owner_kind`: `app`, `cli`, `mcp-stdio`); fehlt bzw. leer → `None`
    /// (CRAWL-125, Nachtrag K4-22; gelesen für 08 AI-020 [M4], `crate::owner`).
    pub owner_kind: Option<String>,
    /// Prozess-ID des Besitzers (`owner_pid`, Dezimalzahl, ungeprüft übernommen); leer, sobald kein Prozess den
    /// Crawl führt → `None`.
    pub owner_pid: Option<String>,
    /// Schemaversion der Datei (`PRAGMA user_version`).
    pub schema_version: u32,
}

fn parse<T: FromStr<Err = os_model::ModelError>>(
    map: &HashMap<String, String>,
    key: &str,
) -> Result<T, StoreError> {
    let value = map
        .get(key)
        .ok_or_else(|| StoreError::MissingMeta(key.to_string()))?;
    value.parse::<T>().map_err(|_| StoreError::InvalidMeta {
        key: key.to_string(),
        value: value.clone(),
    })
}

fn parse_opt<T: FromStr<Err = os_model::ModelError>>(
    map: &HashMap<String, String>,
    key: &str,
) -> Result<Option<T>, StoreError> {
    match map.get(key) {
        None => Ok(None),
        Some(v) if v.is_empty() => Ok(None),
        Some(v) => v
            .parse::<T>()
            .map(Some)
            .map_err(|_| StoreError::InvalidMeta {
                key: key.to_string(),
                value: v.clone(),
            }),
    }
}

fn parse_ts(map: &HashMap<String, String>, key: &str) -> Result<Option<Timestamp>, StoreError> {
    match map.get(key) {
        None => Ok(None),
        Some(v) if v.is_empty() => Ok(None),
        Some(v) => Timestamp::from_db(v)
            .map(Some)
            .map_err(|_| StoreError::InvalidMeta {
                key: key.to_string(),
                value: v.clone(),
            }),
    }
}

impl CrawlFile {
    /// Liest alle Metadaten.
    pub fn meta(&self) -> Result<CrawlMeta, StoreError> {
        let mut stmt = self.conn().prepare("SELECT key, value FROM crawl_meta")?;
        let map: HashMap<String, String> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<_, _>>()?;
        let get = |key: &str| -> Result<String, StoreError> {
            map.get(key)
                .cloned()
                .ok_or_else(|| StoreError::MissingMeta(key.to_string()))
        };
        Ok(CrawlMeta {
            crawl_id: parse(&map, "crawl_id")?,
            mode: parse(&map, "mode")?,
            start_url: map.get("start_url").filter(|v| !v.is_empty()).cloned(),
            project: get("project")?,
            name: get("name")?,
            state: parse(&map, "state")?,
            end_reason: parse_opt(&map, "end_reason")?,
            started_at: parse_ts(&map, "started_at")?,
            ended_at: parse_ts(&map, "ended_at")?,
            heartbeat_at: parse_ts(&map, "heartbeat_at")?,
            owner_kind: map.get("owner_kind").filter(|v| !v.is_empty()).cloned(),
            owner_pid: map.get("owner_pid").filter(|v| !v.is_empty()).cloned(),
            schema_version: schema::user_version(self.conn())?,
        })
    }

    /// Liest einen einzelnen `crawl_meta`-Schlüssel.
    pub fn get_meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self
            .conn()
            .prepare_cached("SELECT value FROM crawl_meta WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Setzt einen `crawl_meta`-Schlüssel (Upsert).
    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        set_meta(self.conn(), key, value)
    }
}

/// Metadata update on an already validated writer connection (no reopen or writer stamp).
pub fn set_meta(conn: &rusqlite::Connection, key: &str, value: &str) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO crawl_meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Manifest-Hilfswerte (OUT-233, M2-F02)
//
// `q_manifest` (Schema v3) liest einige Werte aus `crawl_meta`, die SQLite selbst nicht berechnen kann: den SHA-256 der
// eingebetteten Konfiguration, die Versionen des anlegenden und des zuletzt schreibenden Programms. os-store pflegt sie
// beim Anlegen, bei jeder Änderung der Konfiguration und beim Öffnen zum Schreiben (nur wenn sich der Wert ändert,
// damit ein Öffnen keine unnötige Schreibtransaktion gegen einen laufenden Crawl erzeugt).

/// Version dieses Programmstands (SemVer), Wert von `openspider_version` und `last_writer_version`.
pub const OPENSPIDER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// SHA-256 (hex, Kleinbuchstaben) eines Textes; für `config_sha256` der eingebetteten `.osconfig`.
pub fn sha256_hex(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// Setzt einen `crawl_meta`-Schlüssel, ohne zu schreiben, wenn der Wert schon so dasteht.
pub(crate) fn put_if_changed(
    conn: &rusqlite::Connection,
    key: &str,
    value: &str,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO crawl_meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value WHERE value <> excluded.value",
        [key, value],
    )?;
    Ok(())
}

/// Wirksamer HTTP-User-Agent laut Konfiguration, soweit os-store ihn ohne die Voreinstellungsliste von os-fetch kennt:
/// ein ausdrücklich gesetzter String oder der Standard-User-Agent. Die Engine überschreibt den Wert über `WriteBatch.meta`
/// mit dem aufgelösten String, wenn die Voreinstellung eine andere ist.
pub fn derived_user_agent(cfg: &os_model::CrawlConfig) -> Option<String> {
    let explicit = cfg
        .user_agent
        .http
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(ua) = explicit {
        return Some(ua.to_string());
    }
    cfg.user_agent
        .preset
        .eq_ignore_ascii_case("openspider")
        .then(|| {
            format!(
                "Mozilla/5.0 (compatible; OpenSpider/{OPENSPIDER_VERSION}; +https://openspider.de)"
            )
        })
}

/// Pflegt die aus der eingebetteten Konfiguration abgeleiteten Manifest-Werte (`config_sha256`, `user_agent`,
/// `analysis_auto`). `osconfig` ist der gespeicherte Konfigurationstext; ist er nicht lesbar, bleibt es beim SHA-256.
pub(crate) fn stamp_config(conn: &rusqlite::Connection, osconfig: &str) -> Result<(), StoreError> {
    put_if_changed(conn, "config_sha256", &sha256_hex(osconfig))?;
    if let Ok(cfg) = os_model::CrawlConfig::from_toml_str(osconfig) {
        if let Some(ua) = derived_user_agent(&cfg) {
            put_if_changed(conn, "user_agent", &ua)?;
        }
        put_if_changed(
            conn,
            "analysis_auto",
            if cfg.analysis.auto_at_end { "1" } else { "0" },
        )?;
    }
    Ok(())
}

/// Hält `last_writer_version` auf dem Stand dieses Programms (Öffnen zum Schreiben).
pub(crate) fn stamp_writer(conn: &rusqlite::Connection) -> Result<(), StoreError> {
    put_if_changed(conn, "last_writer_version", OPENSPIDER_VERSION)
}

/// Liest `crawl_config.osconfig` roh (für die Manifest-Werte nach einer Migration).
pub(crate) fn raw_config(conn: &rusqlite::Connection) -> Result<Option<String>, StoreError> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row("SELECT osconfig FROM crawl_config WHERE id = 1", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?)
}

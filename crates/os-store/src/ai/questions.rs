//! Unveränderliche Fragendefinitionen je Version in `ai_questions` (AI-031).
//!
//! Für jede in einem Crawl verwendete Fragenversion (bzw. Vorlagenversion bei parametrisierten Systemfragen, AI-030)
//! steht genau eine Zeile in der Tabelle; sie wird nie verändert (Trigger `ai_questions_immutable` aus 0006). Die
//! Definition wird als kanonisches JSON gespeichert, damit „gleiche Definition“ ein Bytevergleich ist.
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::StoreError;

/// Tabellenname (stabil über Paketübergaben).
pub const TABLE: &str = "ai_questions";

/// Eine gespeicherte Fragenversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredQuestion {
    pub id: String,
    pub version: String,
    /// Kanonisches JSON der Definition (bzw. der Vorlage).
    pub definition_json: String,
}

/// Ergebnis von [`ensure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ensured {
    /// Die Version war neu und wurde gespeichert.
    Inserted,
    /// Dieselbe Version mit identischer Definition war schon da; nichts geändert.
    Present,
}

/// Speichert eine Fragenversion, falls sie noch fehlt. Ist dieselbe (`id`, `version`) mit **anderer** Definition
/// gespeichert, ist das ein Fehler: eine inhaltliche Änderung muss die Version erhöhen (AI-031), die gespeicherte
/// Kopie bleibt unverändert.
pub fn ensure(
    conn: &Connection,
    id: &str,
    version: &str,
    definition_json: &str,
) -> Result<Ensured, StoreError> {
    if let Some(existing) = get(conn, id, version)? {
        return if existing.definition_json == definition_json {
            Ok(Ensured::Present)
        } else {
            Err(StoreError::InvalidInput(format!(
                "ai_questions: Version {version} der Frage {id} ist bereits mit anderer Definition gespeichert"
            )))
        };
    }
    conn.prepare_cached(
        "INSERT INTO ai_questions (id, version, definition_json) VALUES (?1, ?2, ?3)",
    )?
    .execute(params![id, version, definition_json])?;
    Ok(Ensured::Inserted)
}

/// Liest eine Fragenversion.
pub fn get(conn: &Connection, id: &str, version: &str) -> Result<Option<StoredQuestion>, StoreError> {
    Ok(conn
        .prepare_cached(
            "SELECT id, version, definition_json FROM ai_questions WHERE id = ?1 AND version = ?2",
        )?
        .query_row(params![id, version], |r| {
            Ok(StoredQuestion {
                id: r.get(0)?,
                version: r.get(1)?,
                definition_json: r.get(2)?,
            })
        })
        .optional()?)
}

/// Alle gespeicherten Versionen, sortiert nach `id` und `version` (Textvergleich).
pub fn list(conn: &Connection) -> Result<Vec<StoredQuestion>, StoreError> {
    let mut stmt = conn
        .prepare_cached("SELECT id, version, definition_json FROM ai_questions ORDER BY id, version")?;
    let rows = stmt.query_map([], |r| {
        Ok(StoredQuestion {
            id: r.get(0)?,
            version: r.get(1)?,
            definition_json: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Zahl der gespeicherten Versionen einer Frage.
pub fn count_versions(conn: &Connection, id: &str) -> Result<u64, StoreError> {
    let n: i64 = conn
        .prepare_cached("SELECT COUNT(*) FROM ai_questions WHERE id = ?1")?
        .query_row([id], |r| r.get(0))?;
    Ok(u64::try_from(n).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{migrate, MIGRATIONS};

    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        db.pragma_update(None, "foreign_keys", true).unwrap();
        migrate(&mut db, 0, MIGRATIONS).unwrap();
        db
    }

    // spec: AI-031 [M4]
    #[test]
    fn one_row_per_version_and_rows_never_change() {
        let db = db();
        assert_eq!(
            ensure(&db, "page-type", "1.0.0", r#"{"a":1}"#).unwrap(),
            Ensured::Inserted
        );
        assert_eq!(
            ensure(&db, "page-type", "1.0.0", r#"{"a":1}"#).unwrap(),
            Ensured::Present
        );
        assert!(ensure(&db, "page-type", "1.0.0", r#"{"a":2}"#).is_err());
        assert_eq!(
            ensure(&db, "page-type", "1.1.0", r#"{"a":2}"#).unwrap(),
            Ensured::Inserted
        );
        assert_eq!(count_versions(&db, "page-type").unwrap(), 2);
        assert_eq!(
            get(&db, "page-type", "1.0.0").unwrap().unwrap().definition_json,
            r#"{"a":1}"#
        );
        // Der Trigger aus 0006 verhindert jede Änderung, auch an der SQL-Schicht vorbei.
        assert!(db
            .execute(
                "UPDATE ai_questions SET definition_json = '{}' WHERE id = 'page-type'",
                []
            )
            .is_err());
        assert_eq!(list(&db).unwrap().len(), 2);
    }
}

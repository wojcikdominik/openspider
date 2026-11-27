//! Antworten der Decision-Engine in `ai_answers` (AI-031, AI-033, AI-034) und die Schreibwege eines KI-Laufs.
//!
//! * Eine Zeile je (URL bzw. Bild, Frage, Version, Parameter-Hash); bei parametrisierten Fragen trägt sie die
//!   eingesetzten Parameterwerte als kanonisches JSON (`params_json`), der Hash ist dann der SHA-256 dieses JSON.
//! * [`commit_response`] schreibt Protokollzeile (`ai_requests`), Ergebnisse bzw. Status der Einträge und
//!   Cache-Einträge (`ai_cache`) in **einer** Transaktion (AI-020, Nachtrag 26.11.2026: „Verarbeitung einer Antwort ist
//!   atomar“). Die Abrechnung im `BudgetLedger` folgt unmittelbar danach im Speicher; der Ledger wird beim Öffnen aus
//!   `ai_requests` neu aufgebaut, offene Reservierungen verfallen damit automatisch.
//! * Laufangaben eines KI-Laufs in `ai_runs` (Zeile je Lauf). Die Besitzerangaben (`owner_kind`, `owner_pid`,
//!   `heartbeat_at`) führt nicht dieses Modul, sondern `os_ai::run_owner` (M4-K11).
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

use super::cache::{self, CacheRow};
use super::requests::{self, RequestDetails, RequestRecord};
use crate::error::StoreError;

/// Tabellenname (stabil über Paketübergaben).
pub const TABLE: &str = "ai_answers";

/// Eine Zeile von `ai_answers`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnswerRow {
    pub url_id: i64,
    pub question_id: String,
    pub question_version: String,
    /// SHA-256 der kanonischen Parameterwerte; leer bei Fragen ohne Parameter.
    pub params_hash: String,
    /// Kanonisches JSON der eingesetzten Parameterwerte (`{}` ohne Parameter).
    pub params_json: String,
    /// Ergebnis (JSON); `None` bei Einträgen ohne Ergebnis (Pending, Skipped, Fehler, Refusal ohne Grund).
    pub answer_json: Option<String>,
    /// Rohe Konfidenz (Decisions: geliefertes `confidence`, sonst höchste Einzelwahrscheinlichkeit).
    pub raw_confidence: Option<f64>,
    /// `<Q> Source`: `decisions-api` bzw. `self-reported`.
    pub source: Option<String>,
    /// `<Q> Model`.
    pub model: Option<String>,
    /// Kanonischer Statustext (AI-020).
    pub status: String,
    /// Protokollzeile, die das Ergebnis geliefert hat.
    pub request_id: Option<i64>,
}

impl AnswerRow {
    /// Zeile ohne Ergebnis mit Status `status` (z. B. `Pending`, `Skipped (scope)`).
    pub fn status_only(
        url_id: i64,
        question_id: &str,
        question_version: &str,
        params_hash: &str,
        params_json: &str,
        status: &str,
    ) -> Self {
        Self {
            url_id,
            question_id: question_id.to_string(),
            question_version: question_version.to_string(),
            params_hash: params_hash.to_string(),
            params_json: params_json.to_string(),
            answer_json: None,
            raw_confidence: None,
            source: None,
            model: None,
            status: status.to_string(),
            request_id: None,
        }
    }
}

const COLUMNS: &str =
    "url_id, question_id, question_version, params_hash, params_json, answer_json, \
     raw_confidence, source, model, status, request_id";

fn read_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AnswerRow> {
    Ok(AnswerRow {
        url_id: r.get(0)?,
        question_id: r.get(1)?,
        question_version: r.get(2)?,
        params_hash: r.get(3)?,
        params_json: r.get(4)?,
        answer_json: r.get(5)?,
        raw_confidence: r.get(6)?,
        source: r.get(7)?,
        model: r.get(8)?,
        status: r.get(9)?,
        request_id: r.get(10)?,
    })
}

/// Schreibt bzw. ersetzt eine Zeile.
pub fn upsert(conn: &Connection, row: &AnswerRow) -> Result<(), StoreError> {
    conn.prepare_cached(&format!(
        "INSERT OR REPLACE INTO ai_answers ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
    ))?
    .execute(params![
        row.url_id,
        row.question_id,
        row.question_version,
        row.params_hash,
        row.params_json,
        row.answer_json,
        row.raw_confidence,
        row.source,
        row.model,
        row.status,
        row.request_id,
    ])?;
    Ok(())
}

/// Schreibt mehrere Zeilen in einer Transaktion.
pub fn upsert_all(conn: &Connection, rows: &[AnswerRow]) -> Result<(), StoreError> {
    if rows.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for row in rows {
        upsert(&tx, row)?;
    }
    tx.commit()?;
    Ok(())
}

/// Liest eine Zeile.
pub fn get(
    conn: &Connection,
    url_id: i64,
    question_id: &str,
    question_version: &str,
    params_hash: &str,
) -> Result<Option<AnswerRow>, StoreError> {
    Ok(conn
        .prepare_cached(&format!(
            "SELECT {COLUMNS} FROM ai_answers WHERE url_id = ?1 AND question_id = ?2 \
             AND question_version = ?3 AND params_hash = ?4"
        ))?
        .query_row(
            params![url_id, question_id, question_version, params_hash],
            read_row,
        )
        .optional()?)
}

/// Alle Zeilen einer Frage (alle Versionen, wenn `version` fehlt), sortiert nach URL und Parameter-Hash.
pub fn list_for_question(
    conn: &Connection,
    question_id: &str,
    version: Option<&str>,
) -> Result<Vec<AnswerRow>, StoreError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM ai_answers WHERE question_id = ?1 AND (?2 IS NULL OR question_version = ?2) \
         ORDER BY url_id, question_version, params_hash"
    ))?;
    let rows = stmt.query_map(params![question_id, version], read_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Alle Zeilen einer URL.
pub fn list_for_url(conn: &Connection, url_id: i64) -> Result<Vec<AnswerRow>, StoreError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM ai_answers WHERE url_id = ?1 ORDER BY question_id, question_version, params_hash"
    ))?;
    let rows = stmt.query_map([url_id], read_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Anzahl der Zeilen je Statustext einer Fragenversion.
pub fn status_counts(
    conn: &Connection,
    question_id: &str,
    version: &str,
) -> Result<BTreeMap<String, u64>, StoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT status, COUNT(*) FROM ai_answers WHERE question_id = ?1 AND question_version = ?2 GROUP BY status",
    )?;
    let rows = stmt.query_map(params![question_id, version], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (status, n) = row?;
        out.insert(status, u64::try_from(n).unwrap_or(0));
    }
    Ok(out)
}

/// Was eine verarbeitete Antwort schreibt (AI-020, Nachtrag 26.11.2026).
#[derive(Debug, Clone, Copy)]
pub struct ResponseCommit<'a> {
    /// Protokollzeilen in Sendereihenfolge (mehrere bei einem Reparaturversuch, AI-021); leer bei Einträgen ohne
    /// Request.
    pub requests: &'a [(RequestRecord, RequestDetails)],
    /// Ergebnisse bzw. Status der Einträge; Zeilen ohne `request_id` erhalten die Kennung der letzten Protokollzeile.
    pub answers: &'a [AnswerRow],
    /// Cache-Einträge (nur `Success` und `Refusal`, AI-015).
    pub cache: &'a [CacheRow],
}

/// Schreibt eine verarbeitete Antwort atomar: Protokollzeile, Ergebnisse und Cache-Einträge in einer Transaktion.
/// `before_commit` läuft unmittelbar vor dem Commit (Test-Hook für die Fehlerinjektion, AI-020 Kriterium
/// „Atomarität“); liefert er einen Fehler, wird nichts geschrieben. Rückgabe: Kennung der letzten Protokollzeile.
pub fn commit_response(
    conn: &Connection,
    commit: &ResponseCommit<'_>,
    before_commit: &dyn Fn() -> Result<(), StoreError>,
) -> Result<Option<i64>, StoreError> {
    let tx = conn.unchecked_transaction()?;
    let mut request_id = None;
    for (record, details) in commit.requests {
        request_id = Some(requests::insert_with_details(&tx, record, details)?);
    }
    for row in commit.answers {
        let mut row = row.clone();
        if row.request_id.is_none() {
            row.request_id = request_id;
        }
        upsert(&tx, &row)?;
    }
    for row in commit.cache {
        cache::put(&tx, row)?;
    }
    before_commit()?;
    tx.commit()?;
    Ok(request_id)
}

// ---------------------------------------------------------------------------------------------------------------
// Laufangaben eines KI-Laufs. `ai_runs` hat in der Paketliste keinen eigenen
// Besitzer; die Funktionen stehen hier, weil die Decision-Engine (M4-D01) die Tabelle als Erste schreibt (Bericht d01, Auslegungen).

/// Legt eine Zeile in `ai_runs` an (Start eines KI-Laufs) und liefert ihre Kennung.
pub fn begin_run(
    conn: &Connection,
    started_at: &str,
    budget_at_start_microusd: Option<u64>,
    config_json: &str,
) -> Result<i64, StoreError> {
    let budget = budget_at_start_microusd.map(|v| i64::try_from(v).unwrap_or(i64::MAX));
    conn.prepare_cached(
        "INSERT INTO ai_runs (started_at, budget_at_start_microusd, config_json) VALUES (?1, ?2, ?3)",
    )?
    .execute(params![started_at, budget, config_json])?;
    Ok(conn.last_insert_rowid())
}

/// Schließt einen KI-Lauf ab: Ende und verbrauchter Betrag des Laufs (Summe seiner Protokollzeilen).
pub fn finish_run(conn: &Connection, run_id: i64, ended_at: &str) -> Result<(), StoreError> {
    conn.prepare_cached(
        "UPDATE ai_runs SET ended_at = ?2, spent_microusd = \
         (SELECT COALESCE(SUM(cost_microusd), 0) FROM ai_requests WHERE run_id = ?1) WHERE id = ?1",
    )?
    .execute(params![run_id, ended_at])?;
    Ok(())
}

/// Vermerkt die erteilte Kostenbestätigung eines KI-Laufs (08 AI-020, Nachtrag 27.11.2026, K4-27): Zeitpunkt in
/// `ai_runs.confirmed_at`. Fortsetzen setzt dieselbe Zeile fort und übernimmt die Bestätigung.
pub fn set_confirmed_at(
    conn: &Connection,
    run_id: i64,
    confirmed_at: &str,
) -> Result<(), StoreError> {
    conn.prepare_cached("UPDATE ai_runs SET confirmed_at = ?2 WHERE id = ?1")?
        .execute(params![run_id, confirmed_at])?;
    Ok(())
}

/// Zeitpunkt der erteilten Kostenbestätigung eines KI-Laufs (`None` = nicht bestätigt bzw. keine Bestätigung nötig,
/// K4-27).
pub fn confirmed_at(conn: &Connection, run_id: i64) -> Result<Option<String>, StoreError> {
    Ok(conn
        .prepare_cached("SELECT confirmed_at FROM ai_runs WHERE id = ?1")?
        .query_row([run_id], |r| r.get::<_, Option<String>>(0))
        .optional()?
        .flatten())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::requests::PriceSource;
    use crate::schema::{migrate, MIGRATIONS};
    use os_model::budget::MicroUsd;

    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        db.pragma_update(None, "foreign_keys", true).unwrap();
        migrate(&mut db, 0, MIGRATIONS).unwrap();
        db.execute_batch(
            "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length) \
             VALUES(1,'https://ai.test/','https://ai.test/','https://ai.test',1,'Done','Seed',0,16);
             INSERT INTO ai_questions VALUES('page-type','1.0.0','{}');
             INSERT INTO crawl_meta(key,value) VALUES('state','Completed') \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
        )
        .unwrap();
        db
    }

    fn record(run_id: i64) -> RequestRecord {
        RequestRecord {
            run_id,
            timestamp: "2026-11-26T10:00:00Z".into(),
            task: "decision".into(),
            provider: "openai".into(),
            model: "gpt-6-luna".into(),
            endpoint: "decisions".into(),
            url_id: Some(1),
            definition_version: Some("1.0.0".into()),
            reservation: MicroUsd(10),
            redactions: 0,
            input_tokens: Some(100),
            cached_input_tokens: Some(0),
            cache_write_tokens: Some(0),
            output_tokens: Some(0),
            images: 0,
            token_cost: MicroUsd(10),
            fixed_fees: MicroUsd(0),
            regional_uplift: MicroUsd(0),
            cost: MicroUsd(10),
            price_source: PriceSource::Api,
            batch_id: None,
            cache_hit: false,
            status: "Success".into(),
            http_status: Some(200),
            error_code: None,
            error_message: None,
            refusal_reason: None,
            duration_ms: 1,
            provider_request_id: None,
            retries: 0,
            over_reservation: false,
            confidence_mismatch: false,
            estimated_input_tokens: Some(100),
        }
    }

    fn answer(status: &str) -> AnswerRow {
        AnswerRow {
            answer_json: Some(r#"{"type":"predicate","probability":0.9}"#.into()),
            raw_confidence: Some(0.9),
            source: Some("decisions-api".into()),
            model: Some("gpt-6-luna".into()),
            ..AnswerRow::status_only(1, "page-type", "1.0.0", "", "{}", status)
        }
    }

    fn cache_row() -> CacheRow {
        CacheRow {
            cache_key: "k".into(),
            provider: "openai".into(),
            model: "gpt-6-luna".into(),
            endpoint: "http://mock".into(),
            definition_id: "page-type".into(),
            definition_version: "1.0.0".into(),
            input_hash: "h".into(),
            params_hash: "p".into(),
            result_json: "{}".into(),
            status: "Success".into(),
            created_at: "2026-11-26T10:00:00Z".into(),
        }
    }

    // spec: AI-020 [M4]
    // Atomarität: Fehler unmittelbar vor dem Commit → weder Ergebnis noch Cache noch Protokollzeile; ohne Fehler alle.
    #[test]
    fn commit_is_atomic_and_links_the_request() {
        let db = db();
        let run = begin_run(&db, "2026-11-26T10:00:00Z", Some(5_000_000), "{}").unwrap();
        upsert(
            &db,
            &AnswerRow::status_only(1, "page-type", "1.0.0", "", "{}", "Pending"),
        )
        .unwrap();
        let rows = [(record(run), RequestDetails::default())];
        let answers = [answer("Success")];
        let caches = [cache_row()];
        let commit = ResponseCommit {
            requests: &rows,
            answers: &answers,
            cache: &caches,
        };
        let failed = commit_response(&db, &commit, &|| {
            Err(StoreError::InvalidInput("injected".into()))
        });
        assert!(failed.is_err());
        assert_eq!(
            get(&db, 1, "page-type", "1.0.0", "")
                .unwrap()
                .unwrap()
                .status,
            "Pending"
        );
        assert!(cache::get(&db, "k").unwrap().is_none());
        assert_eq!(requests::total_cost(&db).unwrap(), MicroUsd(0));

        let id = commit_response(&db, &commit, &|| Ok(())).unwrap().unwrap();
        let row = get(&db, 1, "page-type", "1.0.0", "").unwrap().unwrap();
        assert_eq!(row.status, "Success");
        assert_eq!(row.request_id, Some(id));
        assert!(cache::get(&db, "k").unwrap().is_some());
        assert_eq!(requests::total_cost(&db).unwrap(), MicroUsd(10));
        finish_run(&db, run, "2026-11-26T10:01:00Z").unwrap();
        let spent: i64 = db
            .query_row(
                "SELECT spent_microusd FROM ai_runs WHERE id = ?1",
                [run],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(spent, 10);
    }

    // spec: AI-031 [M4]
    // Parametrisierte Frage: Zeilen je Parameter-Hash unter derselben Vorlagenversion.
    #[test]
    fn rows_per_params_hash_and_status_counts() {
        let db = db();
        for i in 0..30 {
            upsert(
                &db,
                &AnswerRow::status_only(
                    1,
                    "page-type",
                    "1.0.0",
                    &format!("h{i}"),
                    &format!(r#"{{"c":{i}}}"#),
                    if i % 3 == 0 { "Success" } else { "Pending" },
                ),
            )
            .unwrap();
        }
        assert_eq!(
            list_for_question(&db, "page-type", Some("1.0.0"))
                .unwrap()
                .len(),
            30
        );
        assert_eq!(list_for_url(&db, 1).unwrap().len(), 30);
        let counts = status_counts(&db, "page-type", "1.0.0").unwrap();
        assert_eq!(counts["Success"], 10);
        assert_eq!(counts["Pending"], 20);
    }
}

//! Token- und Kostenprotokoll `ai_requests` (AI-014) und KI-Verbrauch für den `BudgetLedger` (AI-013).
//!
//! Eine Zeile je Anbieter-Request bzw. Cache-Treffer; nur Metadaten, keine Seiteninhalte und keine Antworttexte.
//! Geldbeträge in Mikro-USD; je Zeile gilt `Cost = Token Cost + Fixed Fees + Regional Uplift`.
use crate::StoreError;
use os_model::budget::{BudgetError, MicroUsd, SpendSource, AREA_AI};
use rusqlite::{params, Connection, Row};
use std::collections::BTreeMap;

pub const TABLE: &str = "ai_requests";

/// Höchstlänge der Spalte `Refusal Reason` in Zeichen (AI-014).
pub const REFUSAL_REASON_MAX_CHARS: usize = 500;

/// Herkunft des Preises einer Zeile (`Price Source`, AI-014).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PriceSource {
    Api,
    Table,
    User,
    /// Anbieter lieferte keine `usage`; Kosten aus der lokalen Schätzung (AI-012, AI-004).
    Estimate,
}
impl PriceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PriceSource::Api => "api",
            PriceSource::Table => "table",
            PriceSource::User => "user",
            PriceSource::Estimate => "estimate",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "api" => PriceSource::Api,
            "table" => PriceSource::Table,
            "user" => PriceSource::User,
            "estimate" => PriceSource::Estimate,
            _ => return None,
        })
    }
}

/// Eine Protokollzeile. `status` ist der kanonische Statustext aus AI-020 (`AiStatus::to_string()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRecord {
    pub run_id: i64,
    /// ISO-8601-Zeitstempel (UTC).
    pub timestamp: String,
    pub task: String,
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    /// URL-unabhängige Aufgaben: `None` (Spalte `Address` leer).
    pub url_id: Option<i64>,
    /// `Question/Prompt Version`.
    pub definition_version: Option<String>,
    pub reservation: MicroUsd,
    pub redactions: u32,
    /// `None` = Anbieter hat keinen Wert geliefert (nicht 0).
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub images: u32,
    pub token_cost: MicroUsd,
    pub fixed_fees: MicroUsd,
    pub regional_uplift: MicroUsd,
    pub cost: MicroUsd,
    pub price_source: PriceSource,
    pub batch_id: Option<i64>,
    pub cache_hit: bool,
    pub status: String,
    pub http_status: Option<u16>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    /// Wird beim Schreiben auf 500 Zeichen gekürzt.
    pub refusal_reason: Option<String>,
    pub duration_ms: u64,
    pub provider_request_id: Option<String>,
    pub retries: u32,
    pub over_reservation: bool,
    pub confidence_mismatch: bool,
}

/// Gelesene Zeile mit Kennung und Adresse (aus `urls`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedRequest {
    pub id: i64,
    pub address: Option<String>,
    pub record: RequestRecord,
}

/// Summenzeile je Aufgabe und Modell (AI-014).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestTotals {
    pub task: String,
    pub model: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_tokens: u64,
    pub output_tokens: u64,
    pub images: u64,
    pub token_cost: MicroUsd,
    pub fixed_fees: MicroUsd,
    pub regional_uplift: MicroUsd,
    pub cost: MicroUsd,
}

/// Filter der Ansicht „KI-Protokoll“ (Aufgabe, Modell, Status, Zeitraum `[from, to]`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestFilter {
    pub task: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

fn i64_of(v: u64, what: &str) -> Result<i64, StoreError> {
    i64::try_from(v).map_err(|_| StoreError::InvalidInput(format!("{what} {v} too large")))
}
fn opt_i64(v: Option<u64>, what: &str) -> Result<Option<i64>, StoreError> {
    v.map(|v| i64_of(v, what)).transpose()
}

fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Schreibt eine Zeile und gibt ihre Kennung zurück. Verweigert Zeilen, deren `cost` nicht der Summe der drei
/// Bestandteile entspricht.
pub fn insert(conn: &Connection, r: &RequestRecord) -> Result<i64, StoreError> {
    let sum = r
        .token_cost
        .checked_add(r.fixed_fees)
        .and_then(|s| s.checked_add(r.regional_uplift));
    if sum != Some(r.cost) {
        return Err(StoreError::InvalidInput(format!(
            "cost {} != token cost {} + fixed fees {} + regional uplift {}",
            r.cost.0, r.token_cost.0, r.fixed_fees.0, r.regional_uplift.0
        )));
    }
    let refusal = r
        .refusal_reason
        .as_deref()
        .map(|s| truncate_chars(s, REFUSAL_REASON_MAX_CHARS));
    conn.execute(
        "INSERT INTO ai_requests(run_id, timestamp, task, provider, model, endpoint, url_id, definition_version,
           reservation_microusd, redactions, input_tokens, output_tokens, cached_input_tokens, cache_write_tokens,
           images, token_cost_microusd, fixed_fees_microusd, regional_uplift_microusd, cost_microusd, price_source,
           batch_id, cache_hit, status, http_status, error_code, error_message, refusal_reason, duration_ms,
           provider_request_id, retries, over_reservation, confidence_mismatch)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
           ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32)",
        params![
            r.run_id,
            r.timestamp,
            r.task,
            r.provider,
            r.model,
            r.endpoint,
            r.url_id,
            r.definition_version,
            i64_of(r.reservation.0, "reservation")?,
            r.redactions,
            opt_i64(r.input_tokens, "input_tokens")?,
            opt_i64(r.output_tokens, "output_tokens")?,
            opt_i64(r.cached_input_tokens, "cached_input_tokens")?,
            opt_i64(r.cache_write_tokens, "cache_write_tokens")?,
            r.images,
            i64_of(r.token_cost.0, "token_cost")?,
            i64_of(r.fixed_fees.0, "fixed_fees")?,
            i64_of(r.regional_uplift.0, "regional_uplift")?,
            i64_of(r.cost.0, "cost")?,
            r.price_source.as_str(),
            r.batch_id,
            r.cache_hit,
            r.status,
            r.http_status,
            r.error_code,
            r.error_message,
            refusal,
            i64_of(r.duration_ms, "duration_ms")?,
            r.provider_request_id,
            r.retries,
            r.over_reservation,
            r.confidence_mismatch,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn micro(row: &Row<'_>, idx: &str) -> rusqlite::Result<MicroUsd> {
    let v: i64 = row.get(idx)?;
    Ok(MicroUsd(u64::try_from(v).unwrap_or(0)))
}
fn opt_u64(row: &Row<'_>, idx: &str) -> rusqlite::Result<Option<u64>> {
    Ok(row
        .get::<_, Option<i64>>(idx)?
        .map(|v| u64::try_from(v).unwrap_or(0)))
}

fn read_row(row: &Row<'_>) -> rusqlite::Result<LoggedRequest> {
    let price: String = row.get("price_source")?;
    Ok(LoggedRequest {
        id: row.get("id")?,
        address: row.get("address")?,
        record: RequestRecord {
            run_id: row.get("run_id")?,
            timestamp: row.get("timestamp")?,
            task: row.get("task")?,
            provider: row.get("provider")?,
            model: row.get("model")?,
            endpoint: row.get("endpoint")?,
            url_id: row.get("url_id")?,
            definition_version: row.get("definition_version")?,
            reservation: micro(row, "reservation_microusd")?,
            redactions: row.get("redactions")?,
            input_tokens: opt_u64(row, "input_tokens")?,
            cached_input_tokens: opt_u64(row, "cached_input_tokens")?,
            cache_write_tokens: opt_u64(row, "cache_write_tokens")?,
            output_tokens: opt_u64(row, "output_tokens")?,
            images: row.get("images")?,
            token_cost: micro(row, "token_cost_microusd")?,
            fixed_fees: micro(row, "fixed_fees_microusd")?,
            regional_uplift: micro(row, "regional_uplift_microusd")?,
            cost: micro(row, "cost_microusd")?,
            // Die Tabelle erzwingt die vier Werte per CHECK.
            price_source: PriceSource::parse(&price).unwrap_or(PriceSource::Table),
            batch_id: row.get("batch_id")?,
            cache_hit: row.get("cache_hit")?,
            status: row.get("status")?,
            http_status: row.get("http_status")?,
            error_code: row.get("error_code")?,
            error_message: row.get("error_message")?,
            refusal_reason: row.get("refusal_reason")?,
            duration_ms: opt_u64(row, "duration_ms")?.unwrap_or(0),
            provider_request_id: row.get("provider_request_id")?,
            retries: row.get("retries")?,
            over_reservation: row.get("over_reservation")?,
            confidence_mismatch: row.get("confidence_mismatch")?,
        },
    })
}

/// Zeilen der Ansicht „KI-Protokoll“ in Schreibreihenfolge, gefiltert.
pub fn list(conn: &Connection, filter: &RequestFilter) -> Result<Vec<LoggedRequest>, StoreError> {
    let mut st = conn.prepare(
        "SELECT r.*, u.address AS address FROM ai_requests r LEFT JOIN urls u ON u.id = r.url_id
         WHERE (?1 IS NULL OR r.task = ?1) AND (?2 IS NULL OR r.model = ?2) AND (?3 IS NULL OR r.status = ?3)
           AND (?4 IS NULL OR r.timestamp >= ?4) AND (?5 IS NULL OR r.timestamp <= ?5)
         ORDER BY r.id",
    )?;
    let rows = st.query_map(
        params![
            filter.task,
            filter.model,
            filter.status,
            filter.from,
            filter.to
        ],
        read_row,
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Summenzeilen je Aufgabe und Modell (sortiert). Leere Tokenfelder zählen als 0.
pub fn totals_by_task_and_model(conn: &Connection) -> Result<Vec<RequestTotals>, StoreError> {
    let mut st = conn.prepare(
        "SELECT task, model, count(*), coalesce(sum(input_tokens),0), coalesce(sum(cached_input_tokens),0),
                coalesce(sum(cache_write_tokens),0), coalesce(sum(output_tokens),0), coalesce(sum(images),0),
                coalesce(sum(token_cost_microusd),0), coalesce(sum(fixed_fees_microusd),0),
                coalesce(sum(regional_uplift_microusd),0), coalesce(sum(cost_microusd),0)
         FROM ai_requests GROUP BY task, model ORDER BY task, model",
    )?;
    let u = |v: i64| u64::try_from(v).unwrap_or(0);
    let rows = st.query_map([], |r| {
        Ok(RequestTotals {
            task: r.get(0)?,
            model: r.get(1)?,
            requests: u(r.get(2)?),
            input_tokens: u(r.get(3)?),
            cached_input_tokens: u(r.get(4)?),
            cache_write_tokens: u(r.get(5)?),
            output_tokens: u(r.get(6)?),
            images: u(r.get(7)?),
            token_cost: MicroUsd(u(r.get(8)?)),
            fixed_fees: MicroUsd(u(r.get(9)?)),
            regional_uplift: MicroUsd(u(r.get(10)?)),
            cost: MicroUsd(u(r.get(11)?)),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// KI-Verbrauch je Aufgabe (Summe `Cost (USD)`), Grundlage des Bereichs `ai` im `BudgetLedger`.
pub fn spent_by_task(conn: &Connection) -> Result<BTreeMap<String, MicroUsd>, StoreError> {
    let mut st = conn.prepare("SELECT task, sum(cost_microusd) FROM ai_requests GROUP BY task")?;
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            MicroUsd(u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0)),
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Gesamter KI-Verbrauch (Summe `Cost (USD)`).
pub fn total_cost(conn: &Connection) -> Result<MicroUsd, StoreError> {
    let v: i64 = conn.query_row(
        "SELECT coalesce(sum(cost_microusd), 0) FROM ai_requests",
        [],
        |r| r.get(0),
    )?;
    Ok(MicroUsd(u64::try_from(v).unwrap_or(0)))
}

/// Anmeldung des Bereichs `ai` beim `BudgetLedger`: Verbrauch = Summe aus `ai_requests`.
pub struct AiRequestsSpend<'c>(pub &'c Connection);

impl SpendSource for AiRequestsSpend<'_> {
    fn area(&self) -> &str {
        AREA_AI
    }
    fn spent_by_task(&self) -> Result<BTreeMap<String, MicroUsd>, BudgetError> {
        spent_by_task(self.0).map_err(|e| BudgetError::Storage(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-014
    #[test]
    fn refusal_reason_is_cut_at_a_char_boundary() {
        let s = "ä".repeat(600);
        assert_eq!(truncate_chars(&s, 500).chars().count(), 500);
        assert_eq!(truncate_chars("abc", 500), "abc");
    }
}

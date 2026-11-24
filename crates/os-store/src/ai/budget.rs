//! Grenzwerte des gemeinsamen Crawl-Budgets in `api_budget` (AI-013).
//!
//! Zeilen (`area`, `task`): (`''`, `''`) Crawl-Budget, (`<bereich>`, `''`) Bereichs-Teilbudget wie „KI gesamt“,
//! (`<bereich>`, `<aufgabe>`) Aufgaben-Teilbudget. `limit_microusd IS NULL` in der Crawl-Zeile = „Unbegrenzt“.
//! Die Tabelle enthält nur Grenzwerte; der Verbrauch kommt aus den Protokollen (`ai_requests`, ab M5
//! `dfs_cost_log`).
use super::requests::{latest_over_reservation, AiRequestsSpend};
use crate::StoreError;
use os_model::budget::{BudgetLimits, CrawlBudgetLedger, MicroUsd, SpendSource, AREA_AI};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

pub const TABLE: &str = "api_budget";

fn to_sql(v: MicroUsd) -> Result<i64, StoreError> {
    i64::try_from(v.0)
        .map_err(|_| StoreError::InvalidInput(format!("budget {} µUSD too large", v.0)))
}
fn from_sql(v: i64) -> Result<MicroUsd, StoreError> {
    u64::try_from(v)
        .map(MicroUsd)
        .map_err(|_| StoreError::InvalidInput(format!("negative budget {v}")))
}

/// Speichert alle Grenzwerte (ersetzt die bisherigen Grenzwerte in einer Transaktion; quittierte Überschreitungen
/// bleiben erhalten).
pub fn save_limits(conn: &Connection, limits: &BudgetLimits) -> Result<(), StoreError> {
    if limits.warning_percent > 100 {
        return Err(StoreError::InvalidInput(format!(
            "warning_percent {} > 100",
            limits.warning_percent
        )));
    }
    for area in limits.areas.keys().chain(limits.tasks.keys()) {
        if area.is_empty() {
            return Err(StoreError::InvalidInput("empty budget area".into()));
        }
    }
    if limits
        .tasks
        .values()
        .any(|t| t.keys().any(String::is_empty))
    {
        return Err(StoreError::InvalidInput("empty budget task".into()));
    }
    let acks = load_over_reservation_acks(conn)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM api_budget", [])?;
    let warn = i64::from(limits.warning_percent);
    let crawl = if limits.unlimited {
        None
    } else {
        Some(to_sql(limits.crawl)?)
    };
    let mut insert = tx.prepare(
        "INSERT INTO api_budget(area, task, limit_microusd, warning_percent) VALUES(?1, ?2, ?3, ?4)",
    )?;
    insert.execute(params!["", "", crawl, warn])?;
    for (area, limit) in &limits.areas {
        insert.execute(params![area, "", to_sql(*limit)?, warn])?;
    }
    for (area, tasks) in &limits.tasks {
        for (task, limit) in tasks {
            insert.execute(params![area, task, to_sql(*limit)?, warn])?;
        }
    }
    drop(insert);
    for (area, ack) in &acks {
        write_ack(&tx, area, *ack, warn)?;
    }
    tx.commit()?;
    Ok(())
}

fn write_ack(conn: &Connection, area: &str, ack: u64, warn: i64) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO api_budget(area, task, limit_microusd, warning_percent, over_reservation_ack)
         VALUES(?1, '', NULL, ?2, ?3)
         ON CONFLICT(area, task) DO UPDATE SET over_reservation_ack = excluded.over_reservation_ack",
        params![area, warn, to_sql(MicroUsd(ack))?],
    )?;
    Ok(())
}

/// Quittierte Überschreitungs-Kennungen je Bereich (AI-013).
pub fn load_over_reservation_acks(conn: &Connection) -> Result<BTreeMap<String, u64>, StoreError> {
    let mut st = conn.prepare(
        "SELECT area, over_reservation_ack FROM api_budget
         WHERE area <> '' AND task = '' AND over_reservation_ack IS NOT NULL",
    )?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut acks = BTreeMap::new();
    for row in rows {
        let (area, ack) = row?;
        acks.insert(area, from_sql(ack)?.0);
    }
    Ok(acks)
}

/// Quittiert alle bisher protokollierten Überschreitungen eines Bereichs bis einschließlich `ack`.
pub fn save_over_reservation_ack(
    conn: &Connection,
    area: &str,
    ack: u64,
) -> Result<(), StoreError> {
    if area.is_empty() {
        return Err(StoreError::InvalidInput("empty budget area".into()));
    }
    let warn: i64 = conn
        .query_row(
            "SELECT warning_percent FROM api_budget WHERE area = '' AND task = ''",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(80);
    write_ack(conn, area, ack, warn)
}

/// Ledger einer geöffneten Crawl-Datei mit dem Bereich KI: Grenzwerte aus `api_budget` (sonst `fallback`, z. B. aus
/// dem Profil), Verbrauch aus `ai_requests`, Sperre nach einer nicht quittierten Überschreitung. M5 ergänzt seinen
/// Bereich über `extra` (z. B. eine Quelle über `dfs_cost_log`).
pub fn open_ledger(
    conn: &Connection,
    fallback: BudgetLimits,
    extra: &[&dyn SpendSource],
) -> Result<CrawlBudgetLedger, StoreError> {
    let limits = load_limits(conn)?.unwrap_or(fallback);
    let acks = load_over_reservation_acks(conn)?;
    let ai = AiRequestsSpend(conn);
    let mut sources: Vec<&dyn SpendSource> = vec![&ai];
    sources.extend_from_slice(extra);
    CrawlBudgetLedger::open_acknowledged(limits, &acks, &sources)
        .map_err(|e| StoreError::InvalidInput(e.to_string()))
}

/// „Budget erhöhen und fortsetzen“: setzt und speichert neue Grenzwerte. Hebt das eine Sperre nach Überschreitung auf
/// (eine Grenze wurde angehoben), werden die bisher protokollierten Überschreitungen quittiert, damit sie nach
/// Neuöffnen nicht wieder sperren. Rückgabe: Sperre aufgehoben.
pub fn raise_limits(
    conn: &Connection,
    ledger: &CrawlBudgetLedger,
    limits: BudgetLimits,
) -> Result<bool, StoreError> {
    save_limits(conn, &limits)?;
    let lifted = ledger.apply_limits(limits);
    if lifted {
        if let Some(ack) = latest_over_reservation(conn)? {
            save_over_reservation_ack(conn, AREA_AI, ack)?;
        }
    }
    Ok(lifted)
}

/// Lädt die Grenzwerte; `None`, wenn die Crawl-Datei noch keine gespeichert hat (Aufrufer nimmt dann das Profil).
///
/// Bei „Unbegrenzt“ speichert die Tabelle keinen Betrag; `crawl` erhält dann den Standardwert (5,00 USD), der
/// nur wirkt, wenn „Unbegrenzt“ später wieder abgewählt wird.
pub fn load_limits(conn: &Connection) -> Result<Option<BudgetLimits>, StoreError> {
    let crawl: Option<(Option<i64>, i64)> = conn
        .query_row(
            "SELECT limit_microusd, warning_percent FROM api_budget WHERE area = '' AND task = ''",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((crawl, warning_percent)) = crawl else {
        return Ok(None);
    };
    let mut limits = BudgetLimits {
        warning_percent: u8::try_from(warning_percent)
            .map_err(|_| StoreError::InvalidInput(format!("warning_percent {warning_percent}")))?,
        ..BudgetLimits::default()
    };
    match crawl {
        Some(v) => limits.crawl = from_sql(v)?,
        None => limits.unlimited = true,
    }
    let mut st = conn.prepare(
        "SELECT area, task, limit_microusd FROM api_budget WHERE area <> '' AND limit_microusd IS NOT NULL ORDER BY area, task",
    )?;
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    let mut tasks: BTreeMap<String, BTreeMap<String, MicroUsd>> = BTreeMap::new();
    for row in rows {
        let (area, task, limit) = row?;
        let limit = from_sql(limit)?;
        if task.is_empty() {
            limits.areas.insert(area, limit);
        } else {
            tasks.entry(area).or_default().insert(task, limit);
        }
    }
    limits.tasks = tasks;
    Ok(Some(limits))
}

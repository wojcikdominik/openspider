//! Aktiver Besitzer einer Crawl-Datei (08 AI-020 [M4], Unterpunkt „Aktiver Besitzer außerhalb von `Running`“;
//! 01 CRAWL-125, Nachtrag K4-22).
//!
//! **Leseregel:** Ein Crawl hat einen aktiven Besitzer, wenn `owner_pid` nicht leer ist und `heartbeat_at` höchstens
//! [`HEARTBEAT_TIMEOUT_MS`] alt ist – unabhängig vom gespeicherten Zustand. Grenze, Uhr und Zukunftsregel wie
//! CRAWL-125 ([`crate::resume::effective_state_at`]), aber **ohne** deren Ersatzkette über `started_at`/`created_at`:
//! Fehlt `heartbeat_at`, gibt es keinen aktiven Besitzer. Der abgeleitete Zustand (CRAWL-128) bleibt, wie CRAWL-125
//! ihn bestimmt; diese Regel ändert ihn nie.
//!
//! **Zwei Prüfungen** ([`Occupancy`]):
//! * [`Occupancy::occupied`] – „geschützt in der Verwaltung“: laufend nach CRAWL-125 **oder** aktiver Besitzer mit
//!   beliebiger PID, auch der eigenen. Löschen, Verschieben, Exportieren, Sperren und Entsperren werden abgewiesen
//!   (`busy`, OUT-228), die Aufbewahrung (OUT-229) übergeht den Crawl.
//! * [`Occupancy::foreign_led`] – „fremd geführt“: laufend nach CRAWL-125 mit fremder PID (wie 1.0) **oder** aktiver
//!   Besitzer mit fremder PID. Nur lesendes Öffnen in der App (AGENT-020); Fortsetzen, erneutes Crawlen und Stoppen
//!   werden abgewiesen wie bei einem fremd geführten laufenden Crawl.
//!
//! Für Dateien aus 1.0 liefern beide Prüfungen dasselbe wie CRAWL-125 allein: 1.0 leert `owner_pid` bei jedem
//! geordneten Ende eines Laufs; gesetzt bleibt es nur bei gespeichertem `Running`, und dort ist ein frischer
//! `heartbeat_at` schon „laufend“ nach CRAWL-125.
//!
//! **Schreibseite** ([`claim`], [`beat`], [`release`]): je ein Commit unter `BEGIN IMMEDIATE`; sie schreiben
//! `owner_kind`, `owner_pid` und `heartbeat_at`, nie `state`. Den Takt ([`HEARTBEAT_INTERVAL`]) führt
//! `os_ai::run_owner::RunOwner`.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use os_model::{Clock, CrawlState, Timestamp};
use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::StoreError;
use crate::meta::set_meta;
use crate::open::CrawlFile;
use crate::resume::effective_state_at;
pub use crate::resume::HEARTBEAT_TIMEOUT_MS;

/// Abstand der Heartbeats eines Besitzers (CRAWL-125: alle 2 s; gleiches Intervall für den KI-Lauf, AI-020).
/// `os_engine::crawl::HEARTBEAT_INTERVAL` verweist auf diesen Wert.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);

/// Leseregel „aktiver Besitzer“: `owner_pid` nicht leer und `now − heartbeat_at ≤ 10 000 ms` (Grenze
/// eingeschlossen; ein Zeitpunkt in der Zukunft gilt als aktuell). Keine Ersatzkette: ohne `heartbeat_at` gibt es
/// keinen aktiven Besitzer. Der gespeicherte Zustand spielt keine Rolle.
pub fn active_owner_at(
    owner_pid: Option<&str>,
    heartbeat_at: Option<Timestamp>,
    now: Timestamp,
) -> bool {
    owner_pid.is_some_and(|pid| !pid.is_empty())
        && heartbeat_at.is_some_and(|t| {
            now.unix_millis().saturating_sub(t.unix_millis()) <= HEARTBEAT_TIMEOUT_MS
        })
}

/// Ob `owner_pid` den Prozess `own_pid` nennt (Dezimalzahl wie in CRAWL-125; ein leerer oder nicht lesbarer Wert
/// ist nie der eigene Prozess). Reiner Zahlenvergleich, kein Aufruf des Betriebssystems.
pub fn is_own_pid(owner_pid: Option<&str>, own_pid: u32) -> bool {
    owner_pid.and_then(|p| p.parse::<u32>().ok()) == Some(own_pid)
}

/// Ergebnis beider Prüfungen für einen Zeitpunkt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occupancy {
    /// Laufend nach CRAWL-125 (gespeichertes `Running` mit aktueller Bezugszeit samt Ersatzkette).
    pub running: bool,
    /// Aktiver Besitzer nach der Leseregel ([`active_owner_at`]).
    pub active_owner: bool,
    /// `owner_kind` (`app`, `cli`, `mcp-stdio`), sofern gesetzt.
    pub owner_kind: Option<String>,
    /// `owner_pid` (nicht leer), sofern gesetzt.
    pub owner_pid: Option<String>,
}

impl Occupancy {
    /// Wertet beide Regeln aus. `reference` ist die Bezugszeit nach CRAWL-125 (Heartbeat, ersatzweise Start- bzw.
    /// Anlegezeit), `heartbeat_at` der Heartbeat allein.
    pub fn at(
        state: CrawlState,
        reference: Option<Timestamp>,
        heartbeat_at: Option<Timestamp>,
        owner_kind: Option<&str>,
        owner_pid: Option<&str>,
        now: Timestamp,
    ) -> Self {
        let owner_pid = owner_pid.filter(|p| !p.is_empty());
        Occupancy {
            running: effective_state_at(state, reference, now) == CrawlState::Running,
            active_owner: active_owner_at(owner_pid, heartbeat_at, now),
            owner_kind: owner_kind.filter(|k| !k.is_empty()).map(str::to_string),
            owner_pid: owner_pid.map(str::to_string),
        }
    }

    /// Prüfung (a) „geschützt in der Verwaltung“: laufend nach CRAWL-125 oder aktiver Besitzer mit beliebiger PID.
    pub fn occupied(&self) -> bool {
        self.running || self.active_owner
    }

    /// Prüfung (b) „fremd geführt“ aus Sicht des Prozesses `own_pid`: laufend nach CRAWL-125 oder aktiver Besitzer,
    /// jeweils mit einer PID, die nicht `own_pid` ist (ein laufender Crawl ohne PID gilt wie in 1.0 als fremd).
    pub fn foreign_led(&self, own_pid: u32) -> bool {
        self.occupied() && !is_own_pid(self.owner_pid.as_deref(), own_pid)
    }

    /// [`Occupancy::foreign_led`] aus Sicht dieses Prozesses (`std::process::id()`).
    pub fn foreign_led_here(&self) -> bool {
        self.foreign_led(std::process::id())
    }

    /// Nur der neue Teil von (b): aktiver Besitzer mit fremder PID (unabhängig von CRAWL-125). Leser, die einen
    /// laufenden Crawl schon nach 1.0 abweisen, ergänzen damit ihre Prüfung, ohne die Meldung für `Running` zu ändern.
    pub fn foreign_active_owner(&self, own_pid: u32) -> bool {
        self.active_owner && !is_own_pid(self.owner_pid.as_deref(), own_pid)
    }

    /// Meldung für einen fremd geführten Crawl mit Art und PID (Fehler von [`claim`]).
    pub fn foreign_message(&self) -> String {
        format!(
            "Der Crawl läuft noch (Prozess {}, {}) – Steuerung nur dort",
            self.owner_pid.as_deref().unwrap_or("?"),
            self.owner_kind.as_deref().unwrap_or("?")
        )
    }
}

impl CrawlFile {
    /// Beide Prüfungen für diese Datei; „laufend“ wie [`CrawlFile::effective_state`] (CRAWL-125 samt Ersatzkette),
    /// „aktiver Besitzer“ nach [`active_owner_at`]. `clock` ist die Uhr des lesenden Prozesses.
    pub fn occupancy(&self, clock: &dyn Clock) -> Result<Occupancy, StoreError> {
        let meta = self.meta()?;
        let running = self.effective_state(clock)? == CrawlState::Running;
        let owner_pid = meta.owner_pid.as_deref();
        Ok(Occupancy {
            running,
            active_owner: active_owner_at(owner_pid, meta.heartbeat_at, clock.now()),
            owner_kind: meta.owner_kind.clone(),
            owner_pid: meta.owner_pid.clone(),
        })
    }
}

/// Ergebnis von [`claim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// Besitzerangaben dieses Prozesses eingetragen (`owner_kind`, `owner_pid`, `heartbeat_at` = jetzt).
    Claimed,
    /// Der eigene Prozess führt den Crawl bereits (laufend nach CRAWL-125 mit eigener PID): nichts geschrieben;
    /// den Heartbeat schreibt allein die Engine (AI-020).
    Passive,
}

/// Liest die für beide Prüfungen nötigen Schlüssel über eine beliebige Verbindung.
fn read_occupancy(conn: &Connection, now: Timestamp) -> Result<Occupancy, StoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT key, value FROM crawl_meta WHERE key IN \
         ('state', 'heartbeat_at', 'started_at', 'created_at', 'owner_kind', 'owner_pid')",
    )?;
    let map: HashMap<String, String> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<_, _>>()?;
    let ts = |key: &str| -> Result<Option<Timestamp>, StoreError> {
        match map.get(key).filter(|v| !v.is_empty()) {
            None => Ok(None),
            Some(v) => Timestamp::from_db(v)
                .map(Some)
                .map_err(|_| StoreError::InvalidMeta {
                    key: key.to_string(),
                    value: v.clone(),
                }),
        }
    };
    let raw_state = map
        .get("state")
        .ok_or_else(|| StoreError::MissingMeta("state".into()))?;
    let state = CrawlState::from_str(raw_state).map_err(|_| StoreError::InvalidMeta {
        key: "state".into(),
        value: raw_state.clone(),
    })?;
    let heartbeat_at = ts("heartbeat_at")?;
    // Ersatzkette wie `CrawlFile::effective_state`; eine unlesbare Anlegezeit zählt dort als fehlend.
    let created_at = map
        .get("created_at")
        .and_then(|v| Timestamp::from_db(v).ok());
    let reference = heartbeat_at.or(ts("started_at")?).or(created_at);
    Ok(Occupancy::at(
        state,
        reference,
        heartbeat_at,
        map.get("owner_kind").map(String::as_str),
        map.get("owner_pid").map(String::as_str),
        now,
    ))
}

fn immediate(conn: &Connection) -> Result<Transaction<'_>, StoreError> {
    Ok(Transaction::new_unchecked(
        conn,
        TransactionBehavior::Immediate,
    )?)
}

/// Trägt den Prozess `own_pid` als Besitzer ein (Beginn eines KI-Laufs, AI-020): ein Commit unter
/// `BEGIN IMMEDIATE`, der zuerst „fremd geführt“ prüft.
///
/// * fremd geführt ([`Occupancy::foreign_led`]) → `StoreError::InvalidInput` mit PID und Art des Besitzers, nichts
///   geschrieben;
/// * laufend nach CRAWL-125 mit eigener PID → [`Claim::Passive`], nichts geschrieben;
/// * sonst → `owner_kind`, `owner_pid` = `own_pid`, `heartbeat_at` = `now`; [`Claim::Claimed`]. Ein veralteter
///   fremder Eintrag (Absturz) wird dabei überschrieben. `state` bleibt unverändert.
///
/// `conn` darf nicht schon in einer Transaktion stehen.
pub fn claim(
    conn: &Connection,
    owner_kind: &str,
    own_pid: u32,
    now: Timestamp,
) -> Result<Claim, StoreError> {
    let tx = immediate(conn)?;
    let occupancy = read_occupancy(&tx, now)?;
    if occupancy.foreign_led(own_pid) {
        return Err(StoreError::InvalidInput(occupancy.foreign_message()));
    }
    if occupancy.running {
        return Ok(Claim::Passive);
    }
    set_meta(&tx, "owner_kind", owner_kind)?;
    set_meta(&tx, "owner_pid", &own_pid.to_string())?;
    set_meta(&tx, "heartbeat_at", &now.to_db())?;
    tx.commit()?;
    Ok(Claim::Claimed)
}

/// Ein Heartbeat des Besitzers `own_pid`: `heartbeat_at` = `now`, ein Commit. Geschrieben wird nur, solange
/// `owner_pid` den eigenen Prozess nennt und der Crawl nicht nach CRAWL-125 läuft (dann schreibt allein die Engine).
/// Liefert, ob geschrieben wurde.
pub fn beat(conn: &Connection, own_pid: u32, now: Timestamp) -> Result<bool, StoreError> {
    let tx = immediate(conn)?;
    let occupancy = read_occupancy(&tx, now)?;
    if !is_own_pid(occupancy.owner_pid.as_deref(), own_pid) || occupancy.running {
        return Ok(false);
    }
    set_meta(&tx, "heartbeat_at", &now.to_db())?;
    tx.commit()?;
    Ok(true)
}

/// Ende des KI-Laufs (AI-020): `heartbeat_at` = `now` und leeres `owner_pid` in einem Commit; `owner_kind` bleibt
/// (nennt den letzten Besitzer, CRAWL-125). Wie [`beat`] nur, solange `owner_pid` den eigenen Prozess nennt und der
/// Crawl nicht nach CRAWL-125 läuft – den Besitz einer laufenden Engine gibt erst ihr letzter Batch frei.
/// Liefert, ob geschrieben wurde.
pub fn release(conn: &Connection, own_pid: u32, now: Timestamp) -> Result<bool, StoreError> {
    let tx = immediate(conn)?;
    let occupancy = read_occupancy(&tx, now)?;
    if !is_own_pid(occupancy.owner_pid.as_deref(), own_pid) || occupancy.running {
        return Ok(false);
    }
    set_meta(&tx, "heartbeat_at", &now.to_db())?;
    set_meta(&tx, "owner_pid", "")?;
    tx.commit()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ms: i64) -> Timestamp {
        Timestamp::from_unix_millis(1_800_000_000_000 + ms)
    }

    // spec: AI-020 [M4]
    // spec: CRAWL-125
    #[test]
    fn the_rule_has_no_fallback_and_ignores_the_state() {
        let now = t(0);
        assert!(active_owner_at(Some("7"), Some(t(1)), now), "Zukunft");
        assert!(active_owner_at(Some("7"), Some(t(-10_000)), now));
        assert!(!active_owner_at(Some("7"), Some(t(-10_001)), now));
        assert!(!active_owner_at(Some(""), Some(now), now));
        assert!(!active_owner_at(None, Some(now), now));
        assert!(!active_owner_at(Some("7"), None, now), "keine Ersatzkette");
        let o = Occupancy::at(CrawlState::Completed, None, Some(now), None, Some("7"), now);
        assert!(o.occupied() && !o.running && o.foreign_led(8) && !o.foreign_led(7));
    }

    // spec: AI-020 [M4]
    #[test]
    fn heartbeat_interval_is_two_seconds() {
        assert_eq!(HEARTBEAT_INTERVAL, Duration::from_secs(2));
        assert_eq!(HEARTBEAT_TIMEOUT_MS, 10_000);
    }
}

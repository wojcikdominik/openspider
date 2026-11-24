//! Gemeinsames Crawl-Budget aller bezahlten APIs (AI-013, führend für 08 und 09).
//!
//! Ein Ledger je Crawl, eine Sperre: Reservieren, Abrechnen und Freigeben prüfen Crawl-Budget, Bereichs-
//! Teilbudget („KI gesamt“, später „DataForSEO gesamt“) und Aufgaben-Teilbudget in einem Schritt. Alle Beträge
//! sind ganzzahlige Mikro-USD. Der Ledger kennt weder `os-ai` noch `os-store`: Der bisherige Verbrauch je
//! Bereich kommt über [`SpendSource`] herein (KI: Summe aus `ai_requests`; ab M5 DataForSEO: Summe aus
//! `dfs_cost_log`). Grenzwerte werden außerhalb persistiert (`os-store`, Tabelle `api_budget`).
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

/// Integer micro-USD. No binary floating point enters accounting.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct MicroUsd(pub u64);

impl MicroUsd {
    /// 1 USD in Mikro-USD.
    pub const USD: u64 = 1_000_000;
    /// Addition ohne Überlauf.
    pub fn checked_add(self, other: MicroUsd) -> Option<MicroUsd> {
        self.0.checked_add(other.0).map(MicroUsd)
    }
    /// Subtraktion, die bei 0 stehen bleibt.
    pub fn saturating_sub(self, other: MicroUsd) -> MicroUsd {
        MicroUsd(self.0.saturating_sub(other.0))
    }
}

/// Bereichsname der KI-Funktionen (08). DataForSEO meldet sich ab M5 mit einem eigenen Bereich an.
pub const AREA_AI: &str = "ai";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct BudgetScope {
    pub area: String,
    pub task: Option<String>,
}
impl BudgetScope {
    /// Reservierung eines Bereichs ohne Aufgaben-Teilbudget.
    pub fn area(area: &str) -> Self {
        Self {
            area: area.to_string(),
            task: None,
        }
    }
    /// Reservierung einer Aufgabe (z. B. `ai` / `suggest-title`).
    pub fn task(area: &str, task: &str) -> Self {
        Self {
            area: area.to_string(),
            task: Some(task.to_string()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct BudgetLimits {
    /// Zero disallows paid calls unless unlimited was explicitly selected.
    pub crawl: MicroUsd,
    pub unlimited: bool,
    pub areas: BTreeMap<String, MicroUsd>,
    pub tasks: BTreeMap<String, BTreeMap<String, MicroUsd>>,
    pub warning_percent: u8,
}
impl Default for BudgetLimits {
    fn default() -> Self {
        Self {
            crawl: MicroUsd(5_000_000),
            unlimited: false,
            areas: BTreeMap::new(),
            tasks: BTreeMap::new(),
            warning_percent: 80,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ReservationId(pub u64);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    pub id: ReservationId,
    pub scope: BudgetScope,
    pub maximum: MicroUsd,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    pub actual: MicroUsd,
    pub over_reservation: bool,
}

/// Verbrauch und offene Reservierungen einer Aufgabe bzw. eines Bereichs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskUsage {
    pub spent: MicroUsd,
    pub reserved: MicroUsd,
}

/// Stand eines Bereichs (Tooltip-Aufteilung KI / DataForSEO, AI-013).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AreaUsage {
    pub spent: MicroUsd,
    pub reserved: MicroUsd,
    /// Je Aufgabe; Verbrauch ohne Aufgabe steht unter dem leeren Schlüssel.
    pub tasks: BTreeMap<String, TaskUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetSnapshot {
    pub limits: BudgetLimits,
    /// Sum of persisted per-area logs, not a second shared cost log.
    pub spent: MicroUsd,
    pub reserved: MicroUsd,
    /// Aufteilung je Bereich (auch Bereiche ohne Teilbudget, sobald sie Verbrauch oder Reservierungen haben).
    pub areas: BTreeMap<String, AreaUsage>,
    /// Restbudget des Crawl-Budgets (`None` = unbegrenzt): Grenze − Verbrauch − offene Reservierungen.
    pub remaining: Option<MicroUsd>,
    /// Warnschwelle erreicht (Verbrauch + offene Reservierungen ≥ `warning_percent` % des Crawl-Budgets).
    pub warning: bool,
    /// Nach einer Ist-Antwort über ihrer Reservierung startet keine bezahlte Anfrage mehr, auch nach Neuöffnen, bis
    /// eine Grenze angehoben wird („Budget erhöhen und fortsetzen“).
    pub halted: bool,
}
impl BudgetSnapshot {
    /// Verbrauch im Sinne von AI-013: Summe der Protokolle + offene Reservierungen.
    pub fn consumption(&self) -> MicroUsd {
        MicroUsd(self.spent.0.saturating_add(self.reserved.0))
    }
    /// Verbrauch eines Bereichs (Protokollsumme ohne Reservierungen).
    pub fn area_spent(&self, area: &str) -> MicroUsd {
        self.areas.get(area).map(|a| a.spent).unwrap_or_default()
    }
}

/// Welche Grenze eine Reservierung verhindert hat (für die Zuordnung von `Budget exceeded`, AI-013).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetLevel {
    /// Crawl-Budget aller bezahlten APIs.
    Crawl,
    /// Bereichs-Teilbudget, z. B. „KI gesamt“.
    Area(String),
    /// Aufgaben-Teilbudget eines Bereichs.
    Task { area: String, task: String },
    /// Eine Ist-Antwort lag über ihrer Reservierung; danach startet keine bezahlte Anfrage mehr.
    Halted,
}
impl BudgetLevel {
    /// Trifft die Grenze alle offenen Einträge des Bereichs (Crawl, Bereich, Sperre) und nicht nur eine Aufgabe?
    pub fn stops_whole_area(&self) -> bool {
        !matches!(self, BudgetLevel::Task { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetError {
    /// Reservierung passt nicht (mehr); die Stufe nennt die verhindernde Grenze.
    Exceeded(BudgetLevel),
    UnknownReservation,
    Overflow,
    /// Verbrauch eines Bereichs kann nicht neu geladen werden, solange dort Reservierungen offen sind.
    OpenReservations,
    Storage(String),
}
impl std::fmt::Display for BudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BudgetError::Exceeded(level) => write!(f, "budget exceeded ({level:?})"),
            BudgetError::UnknownReservation => f.write_str("unknown reservation"),
            BudgetError::Overflow => f.write_str("amount overflow"),
            BudgetError::OpenReservations => f.write_str("open reservations"),
            BudgetError::Storage(e) => write!(f, "storage: {e}"),
        }
    }
}
impl std::error::Error for BudgetError {}

/// Each operation is atomic per crawl, including checking all applicable sublimits.
/// Settlement must record actual cost even when it exceeds the reservation, then block new calls.
pub trait BudgetLedger: Send + Sync {
    fn reserve(&self, scope: BudgetScope, maximum: MicroUsd) -> Result<Reservation, BudgetError>;
    fn settle(
        &self,
        reservation: ReservationId,
        actual: MicroUsd,
    ) -> Result<Settlement, BudgetError>;
    fn release(&self, reservation: ReservationId) -> Result<(), BudgetError>;
    fn snapshot(&self) -> Result<BudgetSnapshot, BudgetError>;
    fn set_limits(&self, limits: BudgetLimits) -> Result<(), BudgetError>;
}

/// Quelle des bisherigen Verbrauchs eines Bereichs (Anmeldung eines Bereichs beim Ledger).
///
/// KI: `os_store::ai::requests::AiRequestsSpend` (Summe `Cost (USD)` aus `ai_requests` je Aufgabe).
/// M5 meldet DataForSEO mit einer eigenen Quelle über `dfs_cost_log` an, ohne `os-ai` zu kennen.
pub trait SpendSource {
    /// Bereichsname, unter dem Teilbudgets in [`BudgetLimits::areas`]/[`BudgetLimits::tasks`] stehen.
    fn area(&self) -> &str;
    /// Bisheriger Verbrauch je Aufgabe (leerer Schlüssel = ohne Aufgabe).
    fn spent_by_task(&self) -> Result<BTreeMap<String, MicroUsd>, BudgetError>;
    /// Monoton steigende Kennung der letzten protokollierten Überschreitung einer Reservierung in diesem Bereich
    /// (KI: größte `ai_requests.id` mit `over_reservation = 1`); `None` = keine. Liegt sie über der quittierten
    /// Kennung (siehe [`CrawlBudgetLedger::open_acknowledged`]), startet der Ledger gesperrt – die Sperre überdauert
    /// so Schließen und Neuöffnen der Crawl-Datei.
    fn latest_over_reservation(&self) -> Result<Option<u64>, BudgetError> {
        Ok(None)
    }
}

/// Hebt `new` gegenüber `old` mindestens eine Grenze an (Crawl-Budget, „Unbegrenzt“, Bereichs- oder
/// Aufgaben-Teilbudget angehoben oder entfernt)? Nur das beendet die Sperre nach einer Überschreitung.
pub fn limits_raised(old: &BudgetLimits, new: &BudgetLimits) -> bool {
    let crawl = !old.unlimited && (new.unlimited || new.crawl > old.crawl);
    let areas = old
        .areas
        .iter()
        .any(|(a, l)| new.areas.get(a).is_none_or(|n| n > l));
    let tasks = old.tasks.iter().any(|(a, tasks)| {
        tasks.iter().any(|(t, l)| {
            new.tasks
                .get(a)
                .and_then(|n| n.get(t))
                .is_none_or(|n| n > l)
        })
    });
    crawl || areas || tasks
}

#[derive(Debug, Default)]
struct State {
    limits: BudgetLimits,
    /// Bereich → Aufgabe → Verbrauch.
    spent: BTreeMap<String, BTreeMap<String, u64>>,
    open: BTreeMap<ReservationId, Reservation>,
    next_id: u64,
    halted: bool,
    /// Bereich → quittierte Kennung der letzten Überschreitung (gespeichert in `api_budget`).
    acks: BTreeMap<String, u64>,
}

fn task_key(scope: &BudgetScope) -> &str {
    scope.task.as_deref().unwrap_or("")
}

impl State {
    fn spent_total(&self) -> Result<u64, BudgetError> {
        self.spent
            .values()
            .flat_map(|t| t.values())
            .try_fold(0u64, |a, v| a.checked_add(*v))
            .ok_or(BudgetError::Overflow)
    }
    fn spent_area(&self, area: &str) -> Result<u64, BudgetError> {
        self.spent
            .get(area)
            .map(|t| t.values().try_fold(0u64, |a, v| a.checked_add(*v)))
            .unwrap_or(Some(0))
            .ok_or(BudgetError::Overflow)
    }
    fn spent_task(&self, area: &str, task: &str) -> u64 {
        self.spent
            .get(area)
            .and_then(|t| t.get(task))
            .copied()
            .unwrap_or(0)
    }
    fn reserved_where(&self, pred: impl Fn(&BudgetScope) -> bool) -> Result<u64, BudgetError> {
        self.open
            .values()
            .filter(|r| pred(&r.scope))
            .try_fold(0u64, |a, r| a.checked_add(r.maximum.0))
            .ok_or(BudgetError::Overflow)
    }
    /// `used + amount ≤ limit` ohne Überlauf.
    fn fits(used: u64, amount: u64, limit: u64) -> Result<bool, BudgetError> {
        Ok(used.checked_add(amount).ok_or(BudgetError::Overflow)? <= limit)
    }
    fn check(&self, scope: &BudgetScope, amount: u64) -> Result<(), BudgetError> {
        if !self.limits.unlimited {
            let used = self
                .spent_total()?
                .checked_add(self.reserved_where(|_| true)?)
                .ok_or(BudgetError::Overflow)?;
            if !Self::fits(used, amount, self.limits.crawl.0)? {
                return Err(BudgetError::Exceeded(BudgetLevel::Crawl));
            }
        }
        if let Some(limit) = self.limits.areas.get(&scope.area) {
            let used = self
                .spent_area(&scope.area)?
                .checked_add(self.reserved_where(|s| s.area == scope.area)?)
                .ok_or(BudgetError::Overflow)?;
            if !Self::fits(used, amount, limit.0)? {
                return Err(BudgetError::Exceeded(BudgetLevel::Area(scope.area.clone())));
            }
        }
        if let Some(task) = scope.task.as_deref() {
            if let Some(limit) = self.limits.tasks.get(&scope.area).and_then(|t| t.get(task)) {
                let used = self
                    .spent_task(&scope.area, task)
                    .checked_add(self.reserved_where(|s| {
                        s.area == scope.area && s.task.as_deref() == Some(task)
                    })?)
                    .ok_or(BudgetError::Overflow)?;
                if !Self::fits(used, amount, limit.0)? {
                    return Err(BudgetError::Exceeded(BudgetLevel::Task {
                        area: scope.area.clone(),
                        task: task.to_string(),
                    }));
                }
            }
        }
        Ok(())
    }
    fn snapshot(&self) -> Result<BudgetSnapshot, BudgetError> {
        let mut areas: BTreeMap<String, AreaUsage> = BTreeMap::new();
        for (area, tasks) in &self.spent {
            let a = areas.entry(area.clone()).or_default();
            for (task, v) in tasks {
                a.spent = a
                    .spent
                    .checked_add(MicroUsd(*v))
                    .ok_or(BudgetError::Overflow)?;
                a.tasks.entry(task.clone()).or_default().spent = MicroUsd(*v);
            }
        }
        for r in self.open.values() {
            let a = areas.entry(r.scope.area.clone()).or_default();
            a.reserved = a
                .reserved
                .checked_add(r.maximum)
                .ok_or(BudgetError::Overflow)?;
            let t = a.tasks.entry(task_key(&r.scope).to_string()).or_default();
            t.reserved = t
                .reserved
                .checked_add(r.maximum)
                .ok_or(BudgetError::Overflow)?;
        }
        let spent = MicroUsd(self.spent_total()?);
        let reserved = MicroUsd(self.reserved_where(|_| true)?);
        let consumption = spent.checked_add(reserved).ok_or(BudgetError::Overflow)?;
        let (remaining, warning) = if self.limits.unlimited {
            (None, false)
        } else {
            let crawl = self.limits.crawl;
            let warning = crawl.0 > 0
                && u128::from(consumption.0) * 100
                    >= u128::from(crawl.0) * u128::from(self.limits.warning_percent);
            (Some(crawl.saturating_sub(consumption)), warning)
        };
        Ok(BudgetSnapshot {
            limits: self.limits.clone(),
            spent,
            reserved,
            areas,
            remaining,
            warning,
            halted: self.halted,
        })
    }
}

/// Threadsichere Implementierung des [`BudgetLedger`] für genau eine Crawl-Datei (eine Sperre je Crawl).
///
/// Ablauf je bezahlter Anfrage: [`BudgetLedger::reserve`] mit dem Höchstbetrag → Anfrage senden →
/// Protokollzeile schreiben → [`BudgetLedger::settle`] mit dem Ist-Betrag (bzw. [`BudgetLedger::release`],
/// wenn nichts gesendet wurde). Ein Höchstbetrag von 0 (Ollama, Cache-Treffer) passt immer.
#[derive(Debug, Default)]
pub struct CrawlBudgetLedger {
    state: Mutex<State>,
}

impl CrawlBudgetLedger {
    /// Ledger mit Grenzwerten und ohne bisherigen Verbrauch.
    pub fn new(limits: BudgetLimits) -> Self {
        Self {
            state: Mutex::new(State {
                limits,
                ..State::default()
            }),
        }
    }
    /// Ledger nach dem Öffnen einer Crawl-Datei: Grenzwerte plus bisheriger Verbrauch aller angemeldeten Bereiche
    /// (nach Neustart und „Fortsetzen“ gilt der bisherige Verbrauch weiter).
    pub fn open(limits: BudgetLimits, sources: &[&dyn SpendSource]) -> Result<Self, BudgetError> {
        Self::open_acknowledged(limits, &BTreeMap::new(), sources)
    }
    /// Wie [`Self::open`], mit den quittierten Überschreitungs-Kennungen je Bereich: Hat ein Bereich eine
    /// protokollierte Überschreitung nach seiner Quittung, startet der Ledger gesperrt.
    pub fn open_acknowledged(
        limits: BudgetLimits,
        acks: &BTreeMap<String, u64>,
        sources: &[&dyn SpendSource],
    ) -> Result<Self, BudgetError> {
        let ledger = Self::new(limits);
        ledger.lock().acks = acks.clone();
        for source in sources {
            ledger.load_area(*source)?;
        }
        Ok(ledger)
    }
    fn lock(&self) -> MutexGuard<'_, State> {
        // Eine Panik in einem anderen Thread darf die Buchhaltung nicht unbrauchbar machen; der Zustand wird
        // nur in vollständigen Schritten verändert.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Meldet einen Bereich an bzw. lädt seinen Verbrauch neu aus der Quelle (ersetzt den Stand im Speicher).
    /// Verweigert, solange in diesem Bereich Reservierungen offen sind, damit nichts doppelt zählt.
    pub fn load_area(&self, source: &dyn SpendSource) -> Result<(), BudgetError> {
        let by_task = source.spent_by_task()?;
        let marker = source.latest_over_reservation()?;
        let area = source.area().to_string();
        let mut st = self.lock();
        if st.open.values().any(|r| r.scope.area == area) {
            return Err(BudgetError::OpenReservations);
        }
        let tasks: BTreeMap<String, u64> = by_task.into_iter().map(|(k, v)| (k, v.0)).collect();
        tasks
            .values()
            .try_fold(0u64, |a, v| a.checked_add(*v))
            .ok_or(BudgetError::Overflow)?;
        if marker.is_some_and(|m| st.acks.get(&area).is_none_or(|a| m > *a)) {
            st.halted = true;
        }
        st.spent.insert(area, tasks);
        Ok(())
    }
    /// Setzt neue Grenzwerte. Eine Sperre nach Überschreitung endet nur, wenn dabei eine Grenze angehoben wird
    /// ([`limits_raised`]); Rückgabe `true`, wenn eine Sperre aufgehoben wurde. Der Aufrufer quittiert dann die
    /// bisherigen Überschreitungen in der Crawl-Datei (`os_store::ai::budget::raise_limits`).
    pub fn apply_limits(&self, limits: BudgetLimits) -> bool {
        let mut st = self.lock();
        let lifted = st.halted && limits_raised(&st.limits, &limits);
        if lifted {
            st.halted = false;
        }
        st.limits = limits;
        lifted
    }
    /// Passt ein Betrag derzeit in alle betroffenen Grenzen? (Vorabprüfung ohne Reservierung.)
    pub fn would_fit(&self, scope: &BudgetScope, maximum: MicroUsd) -> Result<(), BudgetError> {
        let st = self.lock();
        if maximum.0 == 0 {
            return Ok(());
        }
        if st.halted {
            return Err(BudgetError::Exceeded(BudgetLevel::Halted));
        }
        st.check(scope, maximum.0)
    }
}

impl BudgetLedger for CrawlBudgetLedger {
    fn reserve(&self, scope: BudgetScope, maximum: MicroUsd) -> Result<Reservation, BudgetError> {
        let mut st = self.lock();
        if maximum.0 > 0 {
            if st.halted {
                return Err(BudgetError::Exceeded(BudgetLevel::Halted));
            }
            st.check(&scope, maximum.0)?;
        }
        st.next_id = st.next_id.checked_add(1).ok_or(BudgetError::Overflow)?;
        let reservation = Reservation {
            id: ReservationId(st.next_id),
            scope,
            maximum,
        };
        st.open.insert(reservation.id, reservation.clone());
        Ok(reservation)
    }
    fn settle(
        &self,
        reservation: ReservationId,
        actual: MicroUsd,
    ) -> Result<Settlement, BudgetError> {
        let mut st = self.lock();
        let r = st
            .open
            .get(&reservation)
            .cloned()
            .ok_or(BudgetError::UnknownReservation)?;
        let current = st.spent_task(&r.scope.area, task_key(&r.scope));
        let next = current.checked_add(actual.0).ok_or(BudgetError::Overflow)?;
        // Gesamtsumme bleibt darstellbar?
        st.spent_total()?
            .checked_add(actual.0)
            .ok_or(BudgetError::Overflow)?;
        st.open.remove(&reservation);
        st.spent
            .entry(r.scope.area.clone())
            .or_default()
            .insert(task_key(&r.scope).to_string(), next);
        let over_reservation = actual > r.maximum;
        if over_reservation {
            st.halted = true;
        }
        Ok(Settlement {
            actual,
            over_reservation,
        })
    }
    fn release(&self, reservation: ReservationId) -> Result<(), BudgetError> {
        self.lock()
            .open
            .remove(&reservation)
            .map(|_| ())
            .ok_or(BudgetError::UnknownReservation)
    }
    fn snapshot(&self) -> Result<BudgetSnapshot, BudgetError> {
        self.lock().snapshot()
    }
    fn set_limits(&self, limits: BudgetLimits) -> Result<(), BudgetError> {
        self.apply_limits(limits);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // spec: AI-013
    #[test]
    fn budget_fixture_roundtrip_is_integer_and_has_safe_defaults() {
        let fixture = r#"{"crawl":10000,"areas":{"ai":8000},"tasks":{"ai":{"suggest-title":4000}},"warning_percent":80}"#;
        let limits: BudgetLimits = serde_json::from_str(fixture).unwrap();
        assert_eq!(limits.tasks["ai"]["suggest-title"], MicroUsd(4000));
        assert_eq!(
            serde_json::from_str::<BudgetLimits>(&serde_json::to_string(&limits).unwrap()).unwrap(),
            limits
        );
        assert_eq!(BudgetLimits::default().crawl, MicroUsd(5_000_000));
        assert_eq!(
            serde_json::to_string(&MicroUsd(u64::MAX)).unwrap(),
            u64::MAX.to_string()
        );
    }

    fn limits(crawl: u64) -> BudgetLimits {
        BudgetLimits {
            crawl: MicroUsd(crawl),
            ..BudgetLimits::default()
        }
    }

    // spec: AI-013
    #[test]
    fn reserve_settle_release_follow_the_crawl_budget() {
        let l = CrawlBudgetLedger::new(limits(10_000));
        let a = l
            .reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(4_000))
            .unwrap();
        let b = l
            .reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(4_000))
            .unwrap();
        assert_eq!(
            l.reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(4_000)),
            Err(BudgetError::Exceeded(BudgetLevel::Crawl))
        );
        let s = l.snapshot().unwrap();
        assert_eq!(
            (s.spent, s.reserved, s.remaining),
            (MicroUsd(0), MicroUsd(8_000), Some(MicroUsd(2_000)))
        );
        assert!(s.warning, "8.000 von 10.000 = 80 %");
        assert!(!l.settle(a.id, MicroUsd(3_000)).unwrap().over_reservation);
        l.release(b.id).unwrap();
        assert_eq!(l.release(b.id), Err(BudgetError::UnknownReservation));
        assert_eq!(
            l.settle(b.id, MicroUsd(1)),
            Err(BudgetError::UnknownReservation)
        );
        let s = l.snapshot().unwrap();
        assert_eq!(
            (s.spent, s.reserved, s.remaining),
            (MicroUsd(3_000), MicroUsd(0), Some(MicroUsd(7_000)))
        );
        assert!(!s.warning);
        assert_eq!(s.areas[AREA_AI].tasks["decision"].spent, MicroUsd(3_000));
    }

    fn ai(task: &str) -> BudgetScope {
        BudgetScope::task(AREA_AI, task)
    }

    // spec: AI-013
    #[test]
    fn area_and_task_sub_budgets_are_checked_together_with_the_crawl_budget() {
        let mut l = limits(1_000_000);
        l.areas.insert(AREA_AI.into(), MicroUsd(8_000));
        l.tasks
            .entry(AREA_AI.into())
            .or_default()
            .insert("suggest-title".into(), MicroUsd(4_000));
        let ledger = CrawlBudgetLedger::new(l);
        // Aufgaben-Teilbudget: nur Title-Vorschläge stoppen.
        let t = ledger
            .reserve(ai("suggest-title"), MicroUsd(4_000))
            .unwrap();
        ledger.settle(t.id, MicroUsd(4_000)).unwrap();
        let e = ledger
            .reserve(ai("suggest-title"), MicroUsd(1))
            .unwrap_err();
        assert_eq!(
            e,
            BudgetError::Exceeded(BudgetLevel::Task {
                area: AREA_AI.into(),
                task: "suggest-title".into()
            })
        );
        assert!(matches!(&e, BudgetError::Exceeded(level) if !level.stops_whole_area()));
        // „KI gesamt“ 8.000: Decisions bekommen noch 4.000, dann ist der Bereich erreicht.
        let d = ledger.reserve(ai("decision"), MicroUsd(4_000)).unwrap();
        ledger.settle(d.id, MicroUsd(4_000)).unwrap();
        assert_eq!(
            ledger.reserve(ai("decision"), MicroUsd(1)),
            Err(BudgetError::Exceeded(BudgetLevel::Area(AREA_AI.into())))
        );
        // Ein anderer Bereich ohne Teilbudget (später DataForSEO) läuft bis zum Crawl-Budget weiter.
        let other = ledger
            .reserve(BudgetScope::area("dataforseo"), MicroUsd(992_000))
            .unwrap();
        assert_eq!(
            ledger.reserve(BudgetScope::area("dataforseo"), MicroUsd(1)),
            Err(BudgetError::Exceeded(BudgetLevel::Crawl))
        );
        ledger.release(other.id).unwrap();
        let s = ledger.snapshot().unwrap();
        assert_eq!(s.area_spent(AREA_AI), MicroUsd(8_000));
        assert_eq!(
            s.areas[AREA_AI].tasks["suggest-title"].spent,
            MicroUsd(4_000)
        );
    }

    // spec: AI-013
    #[test]
    fn budget_zero_admits_only_cost_free_calls_and_unlimited_admits_everything() {
        let zero = CrawlBudgetLedger::new(limits(0));
        assert_eq!(
            zero.reserve(ai("decision"), MicroUsd(1)),
            Err(BudgetError::Exceeded(BudgetLevel::Crawl))
        );
        // Ollama und Cache-Treffer reservieren 0 und laufen.
        let free = zero.reserve(ai("decision"), MicroUsd(0)).unwrap();
        assert!(!zero.settle(free.id, MicroUsd(0)).unwrap().over_reservation);
        assert!(!zero.snapshot().unwrap().warning);

        let unlimited = CrawlBudgetLedger::new(BudgetLimits {
            crawl: MicroUsd(0),
            unlimited: true,
            ..BudgetLimits::default()
        });
        let r = unlimited
            .reserve(ai("decision"), MicroUsd(u64::MAX / 4))
            .unwrap();
        unlimited.settle(r.id, MicroUsd(1_000)).unwrap();
        let s = unlimited.snapshot().unwrap();
        assert_eq!((s.remaining, s.warning), (None, false));
    }

    // spec: AI-013
    #[test]
    fn over_reservation_books_the_excess_and_blocks_further_paid_reservations() {
        let ledger = CrawlBudgetLedger::new(limits(10_000));
        let r = ledger.reserve(ai("decision"), MicroUsd(2_000)).unwrap();
        let s = ledger.settle(r.id, MicroUsd(3_000)).unwrap();
        assert_eq!(
            s,
            Settlement {
                actual: MicroUsd(3_000),
                over_reservation: true
            }
        );
        let snap = ledger.snapshot().unwrap();
        assert_eq!((snap.spent, snap.halted), (MicroUsd(3_000), true));
        assert_eq!(
            ledger.reserve(ai("decision"), MicroUsd(1)),
            Err(BudgetError::Exceeded(BudgetLevel::Halted))
        );
        assert_eq!(
            ledger.would_fit(&ai("decision"), MicroUsd(1)),
            Err(BudgetError::Exceeded(BudgetLevel::Halted))
        );
        // Kostenfreie Aufrufe bleiben möglich; „Budget erhöhen und fortsetzen“ hebt die Sperre auf.
        assert!(ledger.reserve(ai("decision"), MicroUsd(0)).is_ok());
        ledger.set_limits(limits(20_000)).unwrap();
        assert!(ledger.reserve(ai("decision"), MicroUsd(1_000)).is_ok());
    }

    struct Fixed(&'static str, Vec<(&'static str, u64)>);
    impl SpendSource for Fixed {
        fn area(&self) -> &str {
            self.0
        }
        fn spent_by_task(&self) -> Result<BTreeMap<String, MicroUsd>, BudgetError> {
            Ok(self
                .1
                .iter()
                .map(|(t, v)| (t.to_string(), MicroUsd(*v)))
                .collect())
        }
    }

    // spec: AI-013
    #[test]
    fn spend_sources_of_several_areas_count_against_the_shared_crawl_budget() {
        // Anmeldung zweier Bereiche über die Schnittstelle, ohne dass der Ledger os-ai kennt.
        let ai_log = Fixed(
            AREA_AI,
            vec![("decision", 200_000), ("suggest-title", 50_000)],
        );
        let other = Fixed("dataforseo", vec![("", 700_000)]);
        let ledger = CrawlBudgetLedger::open(limits(1_000_000), &[&ai_log, &other]).unwrap();
        assert_eq!(
            ledger.reserve(ai("decision"), MicroUsd(60_000)),
            Err(BudgetError::Exceeded(BudgetLevel::Crawl))
        );
        let ok = ledger.reserve(ai("decision"), MicroUsd(40_000)).unwrap();
        assert_eq!(
            ledger.load_area(&ai_log),
            Err(BudgetError::OpenReservations)
        );
        ledger.release(ok.id).unwrap();
        ledger.load_area(&ai_log).unwrap();
        let s = ledger.snapshot().unwrap();
        assert_eq!(s.spent, MicroUsd(950_000));
        assert_eq!(s.area_spent("dataforseo"), MicroUsd(700_000));
        assert_eq!(s.area_spent(AREA_AI), MicroUsd(250_000));
    }

    // spec: AI-013
    #[test]
    fn overflow_is_reported_instead_of_wrapping() {
        let ledger = CrawlBudgetLedger::new(BudgetLimits {
            unlimited: true,
            ..BudgetLimits::default()
        });
        let a = ledger.reserve(ai("decision"), MicroUsd(u64::MAX)).unwrap();
        assert_eq!(
            ledger.reserve(ai("decision"), MicroUsd(1)).map(|_| ()),
            Ok(()),
            "unbegrenzt prüft keine Summe des Crawl-Budgets"
        );
        ledger.settle(a.id, MicroUsd(u64::MAX)).unwrap();
        let b = ledger.reserve(ai("decision"), MicroUsd(1)).unwrap();
        assert_eq!(ledger.settle(b.id, MicroUsd(1)), Err(BudgetError::Overflow));
    }

    // spec: AI-013
    #[test]
    fn four_concurrent_workers_never_exceed_the_budget() {
        use std::sync::Arc;
        // 200 Einträge, 4 Worker; Reservierung 4.000, Ist 2.500 … 4.000 (≤ Reservierung).
        for round in 0..20u64 {
            let ledger = Arc::new(CrawlBudgetLedger::new(limits(250_000 + round * 1_337)));
            let next = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let workers: Vec<_> = (0..4)
                .map(|w| {
                    let (ledger, next) = (ledger.clone(), next.clone());
                    std::thread::spawn(move || {
                        let mut actual_sum = 0u64;
                        let mut sent = 0u64;
                        loop {
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            if i >= 200 {
                                break;
                            }
                            let task = if i.is_multiple_of(3) {
                                "suggest-title"
                            } else {
                                "decision"
                            };
                            match ledger.reserve(ai(task), MicroUsd(4_000)) {
                                Ok(r) => {
                                    std::thread::yield_now();
                                    let actual = 2_500 + (i * 7 + w) % 1_501;
                                    let s = ledger.settle(r.id, MicroUsd(actual)).unwrap();
                                    assert!(!s.over_reservation);
                                    actual_sum += actual;
                                    sent += 1;
                                }
                                Err(BudgetError::Exceeded(BudgetLevel::Crawl)) => {}
                                Err(e) => panic!("{e}"),
                            }
                        }
                        (actual_sum, sent)
                    })
                })
                .collect();
            let (sum, sent) = workers
                .into_iter()
                .map(|h| h.join().unwrap())
                .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            let snap = ledger.snapshot().unwrap();
            assert_eq!(snap.spent, MicroUsd(sum));
            assert_eq!(snap.reserved, MicroUsd(0));
            assert!(sum <= snap.limits.crawl.0, "Ist {sum} > Budget");
            assert!(sent >= 62, "Budget wurde genutzt: {sent} Requests");
        }
    }

    struct Marked(Option<u64>);
    impl SpendSource for Marked {
        fn area(&self) -> &str {
            AREA_AI
        }
        fn spent_by_task(&self) -> Result<BTreeMap<String, MicroUsd>, BudgetError> {
            Ok(BTreeMap::from([("decision".to_string(), MicroUsd(3_000))]))
        }
        fn latest_over_reservation(&self) -> Result<Option<u64>, BudgetError> {
            Ok(self.0)
        }
    }

    // spec: AI-013
    #[test]
    fn halt_survives_reopen_and_ends_only_when_a_limit_is_raised() {
        let halted = |l: &CrawlBudgetLedger| l.snapshot().unwrap().halted;
        // Protokollierte Überschreitung (Kennung 7), nicht quittiert → gesperrt.
        let l = CrawlBudgetLedger::open(limits(10_000), &[&Marked(Some(7))]).unwrap();
        assert!(halted(&l));
        assert_eq!(
            l.reserve(ai("decision"), MicroUsd(1)),
            Err(BudgetError::Exceeded(BudgetLevel::Halted))
        );
        assert!(
            l.reserve(ai("decision"), MicroUsd(0)).is_ok(),
            "Beträge 0 laufen"
        );
        // Gleiche oder niedrigere Grenzen heben die Sperre nicht auf.
        assert!(!l.apply_limits(limits(10_000)));
        assert!(!l.apply_limits(limits(9_000)));
        assert!(halted(&l));
        // Anheben hebt sie auf.
        assert!(l.apply_limits(limits(20_000)));
        assert!(!halted(&l));
        // Quittiert bis 7 → nach Neuöffnen frei; eine spätere Überschreitung (8) sperrt wieder.
        let acks = BTreeMap::from([(AREA_AI.to_string(), 7)]);
        let l = CrawlBudgetLedger::open_acknowledged(limits(20_000), &acks, &[&Marked(Some(7))])
            .unwrap();
        assert!(!halted(&l));
        let l = CrawlBudgetLedger::open_acknowledged(limits(20_000), &acks, &[&Marked(Some(8))])
            .unwrap();
        assert!(halted(&l));
        assert!(
            !CrawlBudgetLedger::open(limits(1), &[&Marked(None)])
                .unwrap()
                .snapshot()
                .unwrap()
                .halted
        );
    }

    // spec: AI-013
    #[test]
    fn raising_means_any_limit_goes_up_or_is_removed() {
        let mut base = limits(10_000);
        base.areas.insert(AREA_AI.into(), MicroUsd(8_000));
        base.tasks.insert(
            AREA_AI.into(),
            BTreeMap::from([("suggest-title".to_string(), MicroUsd(4_000))]),
        );
        assert!(!limits_raised(&base, &base));
        let mut up = base.clone();
        up.crawl = MicroUsd(10_001);
        assert!(limits_raised(&base, &up));
        let mut un = base.clone();
        un.unlimited = true;
        assert!(limits_raised(&base, &un));
        let mut area = base.clone();
        area.areas.insert(AREA_AI.into(), MicroUsd(9_000));
        assert!(limits_raised(&base, &area));
        let mut gone = base.clone();
        gone.tasks.clear();
        assert!(limits_raised(&base, &gone));
        let mut down = base.clone();
        down.crawl = MicroUsd(5_000);
        down.areas.insert("dataforseo".into(), MicroUsd(1));
        assert!(!limits_raised(&base, &down));
    }
}

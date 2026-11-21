//! Budget contracts (AI-013). STUB(M4-K05): persistence and atomic ledger implementation.
//! A single implementation is shared by AI and future DataForSEO callers; no os-ai dependency.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// Integer micro-USD. No binary floating point enters accounting.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct MicroUsd(pub u64);
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct BudgetScope {
    pub area: String,
    pub task: Option<String>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetSnapshot {
    pub limits: BudgetLimits,
    /// Sum of persisted per-area logs, not a second shared cost log.
    pub spent: MicroUsd,
    pub reserved: MicroUsd,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetError {
    Exceeded,
    UnknownReservation,
    Overflow,
    Storage(String),
}
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
}

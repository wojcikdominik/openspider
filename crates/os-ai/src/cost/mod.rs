//! Kosten (AI-012 – AI-014): Token-Schätzung, Kostenformel, Kostenvoranschlag, Reservierungsbetrag für den
//! gemeinsamen `BudgetLedger` aus `os-model` (AI-013) und Protokollzeilen für `ai_requests`.
//!
//! Ablauf je bezahltem Request (Ausführung in M4-D01 ff.):
//! 1. [`reserve::reservation_amount`] → `BudgetLedger::reserve` (abgelehnt → Status `Budget exceeded`,
//!    [`reserve::status_for`]);
//! 2. Request senden;
//! 3. [`log::request_record`] → `os_store::ai::requests::insert`;
//! 4. `BudgetLedger::settle` mit `record.cost` (Mehrbetrag wird verbucht, danach keine neue Reservierung).
pub mod estimate;
pub mod formula;
pub mod log;
pub mod rates;
pub mod reserve;
pub mod tokens;

pub use estimate::{
    confirmation_required, estimate_task, CachedLookup, Calibration, EstimateItem, EstimateTotals,
    Executable, NoCache, TaskEstimate, TaskEstimateInput,
};
pub use formula::{cost_of, CostBreakdown, FeeCounts, TokenCounts};
pub use log::{cache_hit_record, request_record, LogContext, RequestOutcome, UsageReport};
pub use rates::{LongContext, PriceSource, Rates};
pub use reserve::reservation_amount;

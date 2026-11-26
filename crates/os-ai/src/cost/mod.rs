//! Kosten (AI-012 – AI-014): Token-Schätzung, Kostenformel, Kostenvoranschlag, Reservierungsbetrag für den
//! gemeinsamen `BudgetLedger` aus `os-model` (AI-013) und Protokollzeilen für `ai_requests`.
//!
//! Ablauf je bezahltem Request (Ausführung in M4-D01 ff.):
//! 1. [`reserve::reservation_amount`] → `BudgetLedger::reserve` (abgelehnt → Status `Budget exceeded`,
//!    [`reserve::status_for`]);
//! 2. Request senden;
//! 3. [`log::request_record`] → `os_store::ai::requests::insert`;
//! 4. `BudgetLedger::settle` mit `record.cost` (Mehrbetrag wird verbucht, danach keine neue Reservierung).
//!
//! Nachtrag 24.11.2026, Q00 (M4-K10): Decisions-Eingabe je Frage ([`decisions`]), Ausgabe-Schätzung mit Reasoning
//! ([`output`]), Schema-Zuschlag und Voranschlag je geplantem Request ([`estimate::estimate_requests`]), Reservierung
//! mit `max_tokens` nach AI-011 sowie OpenRouter-`overrides`/`internal_reasoning` ([`basis`]), Ist-Kosten `reported`,
//! `Computed Cost`, `cost_mismatch` und die Spalten `Reasoning Level`/`Reasoning Tokens` ([`log`]). In Schritt 1 tritt
//! dafür [`basis::reservation_for`] an die Stelle von `reservation_amount`, in Schritt 3
//! [`log::detailed_request_record`] mit `os_store::ai::requests::insert_with_details`.
pub mod basis;
pub mod decisions;
pub mod estimate;
pub mod formula;
pub mod log;
pub mod output;
pub mod rates;
pub mod reserve;
pub mod tokens;

pub use basis::{reservation_for, PriceBasis};
pub use decisions::{decisions_input_tokens, question_text_chars};
pub use estimate::{
    confirmation_required, estimate_requests, estimate_task, CachedLookup, Calibration,
    EstimateItem, EstimatePath, EstimateTotals, Executable, NoCache, PlannedEntry, PlannedRequest,
    RequestEstimateInput, RequestTaskEstimate, TaskEstimate, TaskEstimateInput,
};
pub use formula::{cost_of, CostBreakdown, FeeCounts, TokenCounts};
pub use log::{
    cache_hit_record, cost_mismatch, detailed_request_record, reported_breakdown, request_record,
    LogContext, LoggedRow, OutcomeDetails, RequestDetails, RequestOutcome, UsageReport,
};
pub use output::{
    estimate_output_tokens, expected_reasoning_tokens, output_calibration_from_log,
    raw_output_tokens,
};
pub use rates::{LongContext, PriceSource, Rates};
pub use reserve::reservation_amount;

//! Höchstbetrag, der vor jedem KI-Request im `BudgetLedger` reserviert wird (AI-013).
//!
//! Reservierung = geschätzte Eingabe-Tokens × 1,25 × Eingabepreis plus `max_tokens` × Ausgabepreis (bei Batch
//! mit Rabatt; Langkontext-Faktoren, sobald geschätzte Eingabe-Tokens × 1,25 die Schwelle erreichen) plus Gebühr
//! je Request und Gebühr je Bild × Bilder, zuzüglich Regionalzuschlag. Alle Teile werden aufgerundet, damit eine
//! Ist-Antwort mit höchstens den reservierten Tokens nie über der Reservierung liegt.
use super::formula::{assemble, token_cost_raw, FeeCounts, Rounding, TokenCounts};
use super::rates::Rates;
use os_model::ai::AiStatus;
use os_model::budget::{BudgetError, BudgetLevel, MicroUsd};

/// Eingabe-Tokens, die eine Reservierung abdeckt: ⌈geschätzt × 1,25⌉.
pub fn reserved_input_tokens(estimated_input_tokens: u64) -> u64 {
    (u128::from(estimated_input_tokens) * 5).div_ceil(4) as u64
}

/// Reservierungsbetrag eines Requests.
pub fn reservation_amount(
    estimated_input_tokens: u64,
    max_tokens: u64,
    images: u64,
    rates: &Rates,
    batch: bool,
) -> MicroUsd {
    let tokens = TokenCounts::plain(reserved_input_tokens(estimated_input_tokens), max_tokens);
    assemble(
        token_cost_raw(&tokens, rates, batch),
        FeeCounts::one_request(images),
        rates,
        Rounding::Up,
    )
    .cost
}

/// Status eines Eintrags, dessen Reservierung abgelehnt wurde (AI-020): `Budget exceeded`.
pub fn status_for(error: &BudgetError) -> Option<AiStatus> {
    matches!(error, BudgetError::Exceeded(_)).then_some(AiStatus::BudgetExceeded)
}

/// Gilt eine abgelehnte Reservierung für alle offenen Einträge des Bereichs (Crawl-Budget, „KI gesamt“,
/// Sperre nach Überschreitung) oder nur für die Einträge der Aufgabe (`false`)?
pub fn stops_all_ai_entries(error: &BudgetError) -> bool {
    matches!(error, BudgetError::Exceeded(level) if level.stops_whole_area())
}

/// Die Aufgabe, deren Teilbudget erreicht ist (nur bei Aufgaben-Teilbudget).
pub fn exhausted_task(error: &BudgetError) -> Option<&str> {
    match error {
        BudgetError::Exceeded(BudgetLevel::Task { task, .. }) => Some(task),
        _ => None,
    }
}

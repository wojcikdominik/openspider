//! Preisgrundlage eines Requests für Voranschlag, Reservierung und `Computed Cost` (AI-012, AI-013, AI-014,
//! Nachtrag 24.11.2026, Q00-M3/M4).
//!
//! * [`PriceBasis::Rates`]: aufgelöste Sätze aus Preistabelle bzw. Nutzerüberschreibung (Langkontext über Faktoren,
//!   AI-010). Eine Nutzerüberschreibung wirkt bei OpenRouter nur hier, nie auf gemeldete Ist-Kosten (AI-005).
//! * [`PriceBasis::OpenRouter`]: Preise aus der OpenRouter-Modellliste (M4-K09, [`OpenRouterModel`]); Langkontext über
//!   `pricing.overrides` statt Faktoren, für Voranschlag und Reservierung der vorsichtige Ausgabepreis
//!   max(`completion`, `internal_reasoning`).
use super::formula::{assemble, token_cost_raw, FeeCounts, Rounding, TokenCounts};
use super::rates::Rates;
use super::reserve::reserved_input_tokens;
use crate::adapters::openrouter::OpenRouterModel;
use os_model::budget::MicroUsd;

/// Preisgrundlage eines Modells auf einem Endpunkt.
#[derive(Debug, Clone, Copy)]
pub enum PriceBasis<'a> {
    /// Aufgelöste Sätze (Preistabelle, Nutzerüberschreibung, Region).
    Rates(&'a Rates),
    /// OpenRouter-Modell mit Preisen aus der API.
    OpenRouter(&'a OpenRouterModel),
}

impl<'a> PriceBasis<'a> {
    /// Sätze für Voranschlag und Reservierung eines Requests mit `input_tokens` Eingabe-Tokens. OpenRouter: der
    /// zutreffende `overrides`-Eintrag, Ausgabepreis max(`completion`, `internal_reasoning`). `None` = Preis unbekannt.
    pub fn estimate_rates(&self, input_tokens: u64) -> Option<Rates> {
        match self {
            Self::Rates(rates) => Some((*rates).clone()),
            Self::OpenRouter(model) => {
                let mut rates = model.rates_for(input_tokens)?;
                rates.output_pico = model.reserve_output_pico(input_tokens)?;
                Some(rates)
            }
        }
    }

    /// Sätze für die lokale Rechnung der Ist-Kosten (`Computed Cost`) mit den gemeldeten Eingabe-Tokens. OpenRouter:
    /// der zutreffende `overrides`-Eintrag, Ausgabepreis `completion` (`internal_reasoning` nur in Voranschlag und
    /// Reservierung, AI-005).
    pub fn actual_rates(&self, input_tokens: u64) -> Option<Rates> {
        match self {
            Self::Rates(rates) => Some((*rates).clone()),
            Self::OpenRouter(model) => model.rates_for(input_tokens),
        }
    }
}

/// Reservierung eines Requests (AI-013 mit Nachtrag Q00): ⌈geschätzte Eingabe-Tokens × 1,25⌉ × Eingabepreis +
/// `max_tokens` (nach AI-011, die Reasoning-Reserve ist darin enthalten) × Ausgabepreis + Gebühren, zuzüglich
/// Regionalzuschlag, alles aufgerundet. Langkontext: bei Tabellenpreisen über die Faktoren, bei OpenRouter über den
/// `overrides`-Eintrag, sobald ⌈geschätzt × 1,25⌉ dessen `min_prompt_tokens` erreicht. `None` = Preis unbekannt.
pub fn reservation_for(
    basis: PriceBasis<'_>,
    estimated_input_tokens: u64,
    max_tokens: u64,
    images: u64,
    batch: bool,
) -> Option<MicroUsd> {
    let reserved_input = reserved_input_tokens(estimated_input_tokens);
    let rates = basis.estimate_rates(reserved_input)?;
    let tokens = TokenCounts::plain(reserved_input, max_tokens);
    Some(
        assemble(
            token_cost_raw(&tokens, &rates, batch),
            FeeCounts::one_request(images),
            &rates,
            Rounding::Up,
        )
        .cost,
    )
}

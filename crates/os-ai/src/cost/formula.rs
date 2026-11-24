//! Kostenformel je Request (AI-010, AI-012, AI-014): Tokenkosten (Eingabe, Cache, Ausgabe, Langkontext,
//! Batch-Rabatt) + Gebühr je Request + Gebühr je Bild, anschließend Regionalzuschlag auf die Summe.
//!
//! Rundung: Tokenkosten, Gebühren und Zuschlag werden je für sich auf ganze µUSD gerundet (Ist-Kosten kaufmännisch,
//! Reservierungen aufwärts); `cost` ist genau die Summe der drei gerundeten Bestandteile.
use super::rates::{Rates, PICO_PER_MICRO, PPM};
use os_model::budget::MicroUsd;

/// Abgerechnete Token-Mengen eines Requests. `input_tokens` zählt **alle** Eingabe-Tokens einschließlich
/// gecachter und Cache-Schreib-Tokens; die Adapter normalisieren die Anbieterformate darauf.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenCounts {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_tokens: u64,
    pub output_tokens: u64,
}

impl TokenCounts {
    /// Eingabe/Ausgabe ohne Cache.
    pub fn plain(input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            input_tokens,
            output_tokens,
            ..Self::default()
        }
    }
    /// Aus der normalisierten `usage` eines Adapters; fehlende Werte zählen 0.
    pub fn from_usage(usage: &crate::provider::Usage) -> Self {
        Self {
            input_tokens: usage.input_tokens.unwrap_or(0),
            cached_input_tokens: usage.cached_input_tokens.unwrap_or(0),
            cache_write_tokens: usage.cache_write_tokens.unwrap_or(0),
            output_tokens: usage.output_tokens.unwrap_or(0),
        }
    }
}

/// Zählmengen für Gebühren.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeCounts {
    pub requests: u64,
    pub images: u64,
}
impl FeeCounts {
    /// Ein Request mit `images` Eingabebildern.
    pub fn one_request(images: u64) -> Self {
        Self {
            requests: 1,
            images,
        }
    }
}

/// Kosten eines Requests, aufgeteilt wie die Protokollspalten (AI-014).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CostBreakdown {
    pub token_cost: MicroUsd,
    pub fixed_fees: MicroUsd,
    pub regional_uplift: MicroUsd,
    pub cost: MicroUsd,
}

#[derive(Clone, Copy)]
pub(crate) enum Rounding {
    HalfUp,
    Up,
}

pub(crate) fn div_round(n: u128, d: u128, r: Rounding) -> u128 {
    match r {
        Rounding::HalfUp => n.saturating_add(d / 2) / d,
        Rounding::Up => n.div_ceil(d),
    }
}

/// Je Bestandteil höchstens ein Drittel von `u64::MAX`, damit die Summe darstellbar bleibt.
fn cap(v: u128) -> u128 {
    v.min(u128::from(u64::MAX / 3))
}

fn to_micro(v: u128) -> MicroUsd {
    MicroUsd(u64::try_from(v).unwrap_or(u64::MAX))
}

/// Tokenkosten in pUSD × ppm² (Langkontext-Faktor × Batch-Faktor), ungerundet.
pub(crate) fn token_cost_raw(tokens: &TokenCounts, rates: &Rates, batch: bool) -> u128 {
    let lc = rates.long_context_for(tokens.input_tokens);
    let (f_in, f_out, f_cache) = lc
        .map(|l| {
            (
                l.input_factor_ppm,
                l.output_factor_ppm,
                l.cache_read_factor_ppm,
            )
        })
        .unwrap_or((PPM, PPM, PPM));
    let cached = tokens.cached_input_tokens.min(tokens.input_tokens);
    let write = tokens.cache_write_tokens.min(tokens.input_tokens - cached);
    let uncached = tokens.input_tokens - cached - write;
    let m = |t: u64, rate: u64, f: u64| {
        u128::from(t)
            .saturating_mul(u128::from(rate))
            .saturating_mul(u128::from(f))
    };
    let raw = m(uncached, rates.input_pico, f_in)
        .saturating_add(m(cached, rates.cache_read_pico, f_cache))
        // Cache-Schreiben folgt dem Eingabe-Faktor.
        .saturating_add(m(write, rates.cache_write_pico, f_in))
        .saturating_add(m(tokens.output_tokens, rates.output_pico, f_out));
    let batch_factor = if batch {
        PPM.saturating_sub(rates.batch_discount_ppm)
    } else {
        PPM
    };
    raw.saturating_mul(u128::from(batch_factor))
}

pub(crate) fn assemble(
    token_raw: u128,
    fees: FeeCounts,
    rates: &Rates,
    r: Rounding,
) -> CostBreakdown {
    let ppm = u128::from(PPM);
    let pico = u128::from(PICO_PER_MICRO);
    let token = div_round(token_raw, pico * ppm * ppm, r);
    let fees_pico = (u128::from(fees.requests) * u128::from(rates.per_request_pico))
        .saturating_add(u128::from(fees.images) * u128::from(rates.per_image_pico));
    let fixed = div_round(fees_pico, pico, r);
    let uplift = div_round(
        (token.saturating_add(fixed)).saturating_mul(u128::from(rates.regional_uplift_ppm)),
        ppm,
        r,
    );
    // Bestandteile in µUSD bleiben weit unter u64::MAX; Sättigung schützt nur vor unsinnigen Eingaben.
    let (token, fixed, uplift) = (cap(token), cap(fixed), cap(uplift));
    CostBreakdown {
        token_cost: to_micro(token),
        fixed_fees: to_micro(fixed),
        regional_uplift: to_micro(uplift),
        cost: to_micro(token + fixed + uplift),
    }
}

/// Ist-Kosten eines Requests (bzw. Schätzbetrag aus geschätzten Tokens).
pub fn cost_of(tokens: &TokenCounts, fees: FeeCounts, rates: &Rates, batch: bool) -> CostBreakdown {
    assemble(
        token_cost_raw(tokens, rates, batch),
        fees,
        rates,
        Rounding::HalfUp,
    )
}

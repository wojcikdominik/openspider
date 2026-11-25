//! Protokollzeile je Anbieter-Request (AI-014) aus Antwort, Preissätzen und Reservierung.
//!
//! Nachtrag 24.11.2026, Q00: [`detailed_request_record`] liefert zusätzlich die Spalten `Reasoning Level`,
//! `Reasoning Tokens`, `Computed Cost (USD)` und das Kennzeichen `cost_mismatch` ([`RequestDetails`], Speicher über
//! `os_store::ai::requests::insert_with_details`) und verbucht gemeldete Ist-Kosten (OpenRouter `usage.cost`) mit
//! `Price Source = reported` (AI-005). [`RequestOutcome::output_limit`] bildet den Abbruch an der Ausgabegrenze ab.
use super::formula::{cost_of, CostBreakdown, FeeCounts, TokenCounts};
use super::rates::{PriceSource, Rates};
use crate::adapters::anthropic::shared::ReportedCost;
use crate::adapters::anthropic::OutputLimitReached;
use crate::limits::FinalError;
use crate::provider::{ReasoningLevel, Usage};
use os_model::ai::AiStatus;
use os_model::budget::MicroUsd;
pub use os_store::ai::requests::RequestDetails;
use os_store::ai::requests::RequestRecord;

/// Zuordnung der Zeile (Spalten `Timestamp`, `Run`, `Task`, `Provider`, `Model`, `Endpoint`, `Address`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogContext {
    pub run_id: i64,
    pub timestamp: String,
    pub task: String,
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    pub url_id: Option<i64>,
    pub definition_version: Option<String>,
    pub batch_id: Option<i64>,
    pub redactions: u32,
}

/// Was vom Anbieter zurückkam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageReport {
    /// Antwort mit `usage` (fehlende Einzelwerte bleiben `null` und zählen 0).
    Reported(Usage),
    /// Antwort ohne `usage`: Kosten aus der lokalen Schätzung, `Price Source = estimate` (AI-004).
    Missing {
        estimated_input_tokens: u64,
        estimated_output_tokens: u64,
    },
    /// Keine verwertbare Antwort (Fehler, Zeitüberschreitung, Verbindung): Kosten 0.
    NoResponse,
}

/// Ergebnis eines Requests nach allen Wiederholungen (eine Zeile mit `Retries = n`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestOutcome {
    pub status: AiStatus,
    pub usage: UsageReport,
    pub images: u32,
    pub batch: bool,
    pub http_status: Option<u16>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub refusal_reason: Option<String>,
    pub duration_ms: u64,
    pub provider_request_id: Option<String>,
    pub retries: u32,
    pub confidence_mismatch: bool,
    /// Geschätzte Eingabe-Tokens vor dem Korrekturfaktor (`tokens::estimate_input_tokens`), für die Kalibrierung.
    pub estimated_input_tokens: Option<u64>,
}

impl RequestOutcome {
    /// Erfolgreiche Antwort mit `usage`.
    pub fn success(usage: Usage, images: u32, duration_ms: u64) -> Self {
        Self {
            status: AiStatus::Success,
            usage: UsageReport::Reported(usage),
            images,
            batch: false,
            http_status: Some(200),
            error_code: None,
            error_message: None,
            refusal_reason: None,
            duration_ms,
            provider_request_id: None,
            retries: 0,
            confidence_mismatch: false,
            estimated_input_tokens: None,
        }
    }
    /// Mit der Schätzung, die der Reservierung zugrunde lag (Spalte `estimated_input_tokens`).
    pub fn with_estimate(mut self, estimated_input_tokens: u64) -> Self {
        self.estimated_input_tokens = Some(estimated_input_tokens);
        self
    }

    /// Abbruch an der Ausgabegrenze (AI-014/AI-020, Nachtrag Q00-H1): `Status = Error: Output limit reached`,
    /// `HTTP Status` der Antwort (200), `Error Code` = Grund des Anbieters; die gemeldeten Tokens werden verbucht.
    pub fn output_limit(limit: &OutputLimitReached, images: u32) -> Self {
        Self {
            status: AiStatus::OutputLimitReached,
            usage: UsageReport::Reported(limit.usage.clone()),
            images,
            batch: false,
            http_status: Some(limit.http_status),
            error_code: Some(limit.reason.clone()),
            error_message: Some(format!("output limit reached ({})", limit.reason)),
            refusal_reason: None,
            duration_ms: limit.duration_ms,
            provider_request_id: limit.provider_request_id.clone(),
            retries: 0,
            confidence_mismatch: false,
            estimated_input_tokens: None,
        }
    }

    /// Endgültiger Fehler ohne verwertbare Antwort nach allen Versuchen (`Retries` = `attempts − 1`): Kosten 0, mit
    /// `HTTP Status`, `Error Code` und der Anbieter-Request-ID der letzten Fehlerantwort (AI-001, Nachtrag Q00-N2).
    /// Ein Abbruch an der Ausgabegrenze hat eine Antwort mit `usage`; dafür gilt [`RequestOutcome::output_limit`].
    pub fn from_final_error(error: &FinalError, attempts: u32, duration_ms: u64) -> Self {
        Self {
            status: error.status.clone(),
            usage: UsageReport::NoResponse,
            images: 0,
            batch: false,
            http_status: error.http_status,
            error_code: error.error_code.clone(),
            error_message: Some(error.message.clone()),
            refusal_reason: None,
            duration_ms,
            provider_request_id: error.provider_request_id.clone(),
            retries: attempts.saturating_sub(1),
            confidence_mismatch: false,
            estimated_input_tokens: None,
        }
    }
}

/// Angaben einer Zeile aus dem Nachtrag 24.11.2026, Q00, die [`RequestOutcome`] nicht trägt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutcomeDetails {
    /// Effektiv gesendete Reasoning-Stufe (AI-011); `None` bei Decisions und Embeddings (Spalte leer).
    pub reasoning: Option<ReasoningLevel>,
    /// Vom Anbieter gemeldete Ist-Kosten (OpenRouter `usage.cost`, AI-005); verwertbar nach [`ReportedCost::usable`].
    pub reported_cost: Option<ReportedCost>,
}

/// Protokollzeile mit den Spalten des Nachtrags Q00.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedRow {
    pub record: RequestRecord,
    pub details: RequestDetails,
}

/// Aufteilung gemeldeter Ist-Kosten für die Invariante `Cost = Token Cost + Fixed Fees + Regional Uplift` (AI-005,
/// Nachtrag Q00-M3): `Fixed Fees` = lokal berechnete Gebühren je Request und Bild, `Regional Uplift` = 0,
/// `Token Cost` = `Cost` − `Fixed Fees`; wäre das negativ, `Fixed Fees` = `Cost` und `Token Cost` = 0.
pub fn reported_breakdown(reported: MicroUsd, local_fees: MicroUsd) -> CostBreakdown {
    let fixed = local_fees.min(reported);
    CostBreakdown {
        token_cost: MicroUsd(reported.0 - fixed.0),
        fixed_fees: fixed,
        regional_uplift: MicroUsd(0),
        cost: reported,
    }
}

/// `cost_mismatch` (AI-005, Nachtrag Q00-M3): `Cost` weicht um mehr als 1 % **und** mehr als 1 µUSD von
/// `Computed Cost` ab.
pub fn cost_mismatch(cost: MicroUsd, computed: MicroUsd) -> bool {
    let diff = cost.0.abs_diff(computed.0);
    diff > 1 && u128::from(diff) * 100 > u128::from(computed.0)
}

/// Kosten der Zeile.
pub fn outcome_cost(outcome: &RequestOutcome, rates: &Rates) -> (CostBreakdown, PriceSource) {
    let fees = FeeCounts::one_request(u64::from(outcome.images));
    match &outcome.usage {
        UsageReport::Reported(u) => (
            cost_of(&TokenCounts::from_usage(u), fees, rates, outcome.batch),
            rates.source,
        ),
        UsageReport::Missing {
            estimated_input_tokens,
            estimated_output_tokens,
        } => (
            cost_of(
                &TokenCounts::plain(*estimated_input_tokens, *estimated_output_tokens),
                fees,
                rates,
                outcome.batch,
            ),
            PriceSource::Estimate,
        ),
        UsageReport::NoResponse => (CostBreakdown::default(), rates.source),
    }
}

/// Protokollzeile eines gesendeten Requests. `over_reservation`, wenn die Ist-Kosten die Reservierung übersteigen.
pub fn request_record(
    ctx: &LogContext,
    rates: &Rates,
    reservation: MicroUsd,
    outcome: &RequestOutcome,
) -> RequestRecord {
    detailed_request_record(ctx, rates, reservation, outcome, &OutcomeDetails::default()).record
}

/// Protokollzeile mit den Spalten des Nachtrags 24.11.2026, Q00 (AI-014). `rates` sind die lokalen Preissätze der
/// Ist-Tokens (bei OpenRouter mit Langkontext: [`super::basis::PriceBasis::actual_rates`]). Liegt eine verwertbare
/// gemeldete Ist-Kostenangabe vor (Antwort mit `usage`), ist sie `Cost (USD)` mit `Price Source = reported`;
/// `Computed Cost` bleibt die lokale Rechnung, und `over_reservation` vergleicht die Reservierung mit dem gemeldeten
/// Betrag (AI-013, Nachtrag). Sonst gilt die lokale Rechnung, `Computed Cost` = `Cost`.
pub fn detailed_request_record(
    ctx: &LogContext,
    rates: &Rates,
    reservation: MicroUsd,
    outcome: &RequestOutcome,
    details: &OutcomeDetails,
) -> LoggedRow {
    let (computed, local_source) = outcome_cost(outcome, rates);
    let reported = match (&outcome.usage, details.reported_cost.and_then(|r| r.usable())) {
        (UsageReport::Reported(_), Some(micro)) => Some(MicroUsd(micro)),
        _ => None,
    };
    let (c, source, mismatch) = match reported {
        Some(cost) => (
            reported_breakdown(cost, computed.fixed_fees),
            PriceSource::Reported,
            cost_mismatch(cost, computed.cost),
        ),
        None => (computed, local_source, false),
    };
    let record = base_record(ctx, reservation, outcome, &c, source);
    let reasoning_tokens = match &outcome.usage {
        UsageReport::Reported(u) => u.reasoning_tokens,
        _ => None,
    };
    LoggedRow {
        record,
        details: RequestDetails {
            reasoning_level: details.reasoning.map(|l| l.as_str().to_string()),
            reasoning_tokens,
            computed_cost: Some(computed.cost),
            cost_mismatch: mismatch,
        },
    }
}

fn base_record(
    ctx: &LogContext,
    reservation: MicroUsd,
    outcome: &RequestOutcome,
    c: &CostBreakdown,
    source: PriceSource,
) -> RequestRecord {
    let usage = match &outcome.usage {
        UsageReport::Reported(u) => u.clone(),
        _ => Usage::default(),
    };
    RequestRecord {
        run_id: ctx.run_id,
        timestamp: ctx.timestamp.clone(),
        task: ctx.task.clone(),
        provider: ctx.provider.clone(),
        model: ctx.model.clone(),
        endpoint: ctx.endpoint.clone(),
        url_id: ctx.url_id,
        definition_version: ctx.definition_version.clone(),
        reservation,
        redactions: ctx.redactions,
        input_tokens: usage.input_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        output_tokens: usage.output_tokens,
        images: outcome.images,
        token_cost: c.token_cost,
        fixed_fees: c.fixed_fees,
        regional_uplift: c.regional_uplift,
        cost: c.cost,
        price_source: source,
        batch_id: ctx.batch_id,
        cache_hit: false,
        status: outcome.status.to_string(),
        http_status: outcome.http_status,
        error_code: outcome.error_code.clone(),
        error_message: outcome.error_message.clone(),
        refusal_reason: outcome.refusal_reason.clone(),
        duration_ms: outcome.duration_ms,
        provider_request_id: outcome.provider_request_id.clone(),
        retries: outcome.retries,
        over_reservation: c.cost > reservation,
        confidence_mismatch: outcome.confidence_mismatch,
        estimated_input_tokens: outcome.estimated_input_tokens,
    }
}

/// Zeile für einen Cache-Treffer: `Cache Hit = true`, `Cost = 0`, kein HTTP-Request, keine Tokens.
pub fn cache_hit_record(ctx: &LogContext, source: PriceSource, images: u32) -> RequestRecord {
    RequestRecord {
        run_id: ctx.run_id,
        timestamp: ctx.timestamp.clone(),
        task: ctx.task.clone(),
        provider: ctx.provider.clone(),
        model: ctx.model.clone(),
        endpoint: ctx.endpoint.clone(),
        url_id: ctx.url_id,
        definition_version: ctx.definition_version.clone(),
        reservation: MicroUsd(0),
        redactions: ctx.redactions,
        input_tokens: None,
        cached_input_tokens: None,
        cache_write_tokens: None,
        output_tokens: None,
        images,
        token_cost: MicroUsd(0),
        fixed_fees: MicroUsd(0),
        regional_uplift: MicroUsd(0),
        cost: MicroUsd(0),
        price_source: source,
        batch_id: ctx.batch_id,
        cache_hit: true,
        status: AiStatus::SuccessCached.to_string(),
        http_status: None,
        error_code: None,
        error_message: None,
        refusal_reason: None,
        duration_ms: 0,
        provider_request_id: None,
        retries: 0,
        over_reservation: false,
        confidence_mismatch: false,
        estimated_input_tokens: None,
    }
}

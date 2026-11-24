//! Protokollzeile je Anbieter-Request (AI-014) aus Antwort, Preissätzen und Reservierung.
use super::formula::{cost_of, CostBreakdown, FeeCounts, TokenCounts};
use super::rates::{PriceSource, Rates};
use crate::provider::Usage;
use os_model::ai::AiStatus;
use os_model::budget::MicroUsd;
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
        }
    }
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
    let (c, source) = outcome_cost(outcome, rates);
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
    }
}

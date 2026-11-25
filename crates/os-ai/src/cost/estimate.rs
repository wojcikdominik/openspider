//! Kostenvoranschlag je Aufgabe vor dem Lauf (AI-012).
use super::formula::{cost_of, FeeCounts, TokenCounts};
use super::rates::{Rates, PPM};
use super::tokens::estimate_input_tokens;
use os_model::budget::MicroUsd;
use std::collections::{BTreeSet, HashSet};

/// Ob ein Eintrag schon im Ergebnis-Cache liegt (AI-015). Die Implementierung liefert der Cache (M4-K06); der
/// Voranschlag braucht nur diese Abfrage über den Cache-Schlüssel des Eintrags.
pub trait CachedLookup {
    fn is_cached(&self, cache_key: &str) -> bool;
}
/// Ohne Cache (Option „Cache verwenden“ aus).
pub struct NoCache;
impl CachedLookup for NoCache {
    fn is_cached(&self, _: &str) -> bool {
        false
    }
}
impl CachedLookup for HashSet<String> {
    fn is_cached(&self, cache_key: &str) -> bool {
        self.contains(cache_key)
    }
}
impl CachedLookup for BTreeSet<String> {
    fn is_cached(&self, cache_key: &str) -> bool {
        self.contains(cache_key)
    }
}

/// Ein Eintrag im Geltungsbereich (Einträge außerhalb, `Skipped (scope)`, übergibt der Aufrufer nicht).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstimateItem {
    /// Cache-Schlüssel der tatsächlich aufbereiteten Eingabe; `None` = nicht cachefähig.
    pub cache_key: Option<String>,
    /// Zeichenzahl der aufbereiteten Eingabe (nach Kürzung/Schwärzung).
    pub input_chars: u64,
    pub images: u64,
}

/// Gelernter Korrekturfaktor je Modell (Median Ist/Schätzung der letzten 200 Requests, ab 20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration {
    /// Faktor in ppm (1.000.000 = 1,0).
    pub factor_ppm: u64,
    pub calibrated: bool,
}
/// Fenster und Mindestzahl der Kalibrierung (AI-012).
pub const CALIBRATION_WINDOW: usize = 200;
pub const CALIBRATION_MIN_SAMPLES: usize = 20;

impl Calibration {
    pub const UNCALIBRATED: Calibration = Calibration {
        factor_ppm: PPM,
        calibrated: false,
    };
    /// Aus Paaren (geschätzte, tatsächliche Eingabe-Tokens) in zeitlicher Reihenfolge (älteste zuerst); Paare mit
    /// Schätzung 0 zählen nicht. Unter 20 Paaren bleibt das Modell unkalibriert.
    pub fn from_samples(samples: impl IntoIterator<Item = (u64, u64)>) -> Self {
        let valid: Vec<(u64, u64)> = samples.into_iter().filter(|(e, _)| *e > 0).collect();
        let recent = &valid[valid.len().saturating_sub(CALIBRATION_WINDOW)..];
        if recent.len() < CALIBRATION_MIN_SAMPLES {
            return Self::UNCALIBRATED;
        }
        let mut ratios: Vec<u128> = recent
            .iter()
            .map(|(e, a)| (u128::from(*a) * u128::from(PPM) + u128::from(*e) / 2) / u128::from(*e))
            .collect();
        ratios.sort_unstable();
        let n = ratios.len();
        let median = if !n.is_multiple_of(2) {
            ratios[n / 2]
        } else {
            (ratios[n / 2 - 1] + ratios[n / 2]).div_ceil(2)
        };
        Self {
            factor_ppm: u64::try_from(median).unwrap_or(u64::MAX),
            calibrated: true,
        }
    }
    /// Korrekturfaktor eines Modells aus dem Protokoll `ai_requests` der Crawl-Datei: Median Ist/Schätzung der
    /// letzten 200 Requests mit Schätzung und gemeldeten Eingabe-Tokens, ab 20 (sonst unkalibriert).
    pub fn from_log(
        file: &os_store::CrawlFile,
        provider: &str,
        model: &str,
    ) -> Result<Self, os_store::StoreError> {
        os_store::ai::requests::calibration_samples(
            file.conn(),
            provider,
            model,
            CALIBRATION_WINDOW,
        )
        .map(Self::from_samples)
    }
    /// Korrigierte Tokenzahl (aufgerundet).
    pub fn apply(&self, tokens: u64) -> u64 {
        u64::try_from((u128::from(tokens) * u128::from(self.factor_ppm)).div_ceil(u128::from(PPM)))
            .unwrap_or(u64::MAX)
    }
    /// Spanne niedrig/hoch in ppm: × 0,75/1,25 unkalibriert, × 0,9/1,1 kalibriert.
    pub fn spread_ppm(&self) -> (u64, u64) {
        if self.calibrated {
            (900_000, 1_100_000)
        } else {
            (750_000, 1_250_000)
        }
    }
}

/// Warum eine Aufgabe nicht ausführbar ist (Spalte `Executable`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Executable {
    Yes,
    MissingApiKey,
    PriceUnknown,
}
impl Executable {
    pub fn is_yes(&self) -> bool {
        matches!(self, Executable::Yes)
    }
    /// Grund wie in AI-012 (`Missing API key`, `Price unknown`); leer bei ausführbar.
    pub fn reason(&self) -> &'static str {
        match self {
            Executable::Yes => "",
            Executable::MissingApiKey => "Missing API key",
            Executable::PriceUnknown => "Price unknown",
        }
    }
}

/// Eingaben des Voranschlags einer Aufgabe.
#[derive(Debug, Clone)]
pub struct TaskEstimateInput<'a> {
    pub task: String,
    pub model: String,
    pub items: &'a [EstimateItem],
    /// Fester Anteil für Anweisungen je Request.
    pub instruction_tokens: u64,
    /// Erwartete Ausgabe-Tokens je Request (aus dem Aufgabenprofil).
    pub expected_output_tokens: u64,
    /// `None` = kein Preis für den Endpunkt (weder API, Tabelle noch Nutzer).
    pub rates: Option<&'a Rates>,
    pub api_key_present: bool,
    pub calibration: Calibration,
    /// Aufgabe kann über eine Batch-Schnittstelle laufen (AI-016); sonst `Batch Savings` = 0.
    pub batch_capable: bool,
}

/// Eine Zeile des Abschnitts „KI“ im Dialog „Kostenvoranschlag“.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEstimate {
    pub task: String,
    pub model: String,
    pub items: u64,
    pub cached: u64,
    pub est_input_tokens: u64,
    pub est_output_tokens: u64,
    /// `None` = „Preis unbekannt“.
    pub low: Option<MicroUsd>,
    pub expected: Option<MicroUsd>,
    pub high: Option<MicroUsd>,
    pub batch_savings: Option<MicroUsd>,
    pub executable: Executable,
    pub calibrated: bool,
}

fn scale(v: MicroUsd, ppm: u64) -> MicroUsd {
    MicroUsd(
        u64::try_from((u128::from(v.0) * u128::from(ppm) + u128::from(PPM) / 2) / u128::from(PPM))
            .unwrap_or(u64::MAX),
    )
}

/// Voranschlag einer Aufgabe. Cache-Treffer zählen in `cached` und tragen 0 bei; die Schätzung sendet nichts.
pub fn estimate_task(input: &TaskEstimateInput<'_>, cache: &dyn CachedLookup) -> TaskEstimate {
    let image_estimate = input.rates.map(|r| r.image_token_estimate).unwrap_or(0);
    let mut cached = 0u64;
    let (mut tokens_in, mut tokens_out) = (0u64, 0u64);
    let (mut sync, mut batch) = (MicroUsd(0), MicroUsd(0));
    for item in input.items {
        if item
            .cache_key
            .as_deref()
            .is_some_and(|k| cache.is_cached(k))
        {
            cached += 1;
            continue;
        }
        let raw = estimate_input_tokens(
            item.input_chars,
            item.images,
            image_estimate,
            input.instruction_tokens,
        );
        let t = TokenCounts::plain(input.calibration.apply(raw), input.expected_output_tokens);
        tokens_in = tokens_in.saturating_add(t.input_tokens);
        tokens_out = tokens_out.saturating_add(t.output_tokens);
        if let Some(rates) = input.rates {
            let fees = FeeCounts::one_request(item.images);
            sync = MicroUsd(
                sync.0
                    .saturating_add(cost_of(&t, fees, rates, false).cost.0),
            );
            batch = MicroUsd(
                batch
                    .0
                    .saturating_add(cost_of(&t, fees, rates, true).cost.0),
            );
        }
    }
    let executable = if input.rates.is_none() {
        Executable::PriceUnknown
    } else if !input.api_key_present {
        Executable::MissingApiKey
    } else {
        Executable::Yes
    };
    let (lo, hi) = input.calibration.spread_ppm();
    let priced = input.rates.is_some();
    TaskEstimate {
        task: input.task.clone(),
        model: input.model.clone(),
        items: input.items.len() as u64,
        cached,
        est_input_tokens: tokens_in,
        est_output_tokens: tokens_out,
        low: priced.then(|| scale(sync, lo)),
        expected: priced.then_some(sync),
        high: priced.then(|| scale(sync, hi)),
        batch_savings: priced.then(|| {
            if input.batch_capable {
                sync.saturating_sub(batch)
            } else {
                MicroUsd(0)
            }
        }),
        executable,
        calibrated: input.calibration.calibrated,
    }
}

/// Summe (niedrig, erwartet, hoch) über die ausführbaren Aufgaben mit bekanntem Preis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EstimateTotals {
    pub low: MicroUsd,
    pub expected: MicroUsd,
    pub high: MicroUsd,
}
impl EstimateTotals {
    /// Abschnitt „KI“: nur ausführbare Aufgaben (nicht ausführbare senden nichts).
    pub fn of_tasks(tasks: &[TaskEstimate]) -> Self {
        tasks
            .iter()
            .filter(|t| t.executable.is_yes())
            .fold(Self::default(), |acc, t| Self {
                low: MicroUsd(acc.low.0.saturating_add(t.low.unwrap_or_default().0)),
                expected: MicroUsd(
                    acc.expected
                        .0
                        .saturating_add(t.expected.unwrap_or_default().0),
                ),
                high: MicroUsd(acc.high.0.saturating_add(t.high.unwrap_or_default().0)),
            })
    }
    /// Gemeinsame Summenzeile mit dem Abschnitt „DataForSEO“ (ab M5, D4S-012):
    /// niedrig = Σ KI low + Σ DFS min, erwartet = Σ KI expected + Σ DFS max, hoch = Σ KI high + Σ DFS max.
    pub fn with_dataforseo(self, dfs_min: MicroUsd, dfs_max: MicroUsd) -> Self {
        Self {
            low: MicroUsd(self.low.0.saturating_add(dfs_min.0)),
            expected: MicroUsd(self.expected.0.saturating_add(dfs_max.0)),
            high: MicroUsd(self.high.0.saturating_add(dfs_max.0)),
        }
    }
}

/// Muss der Lauf bestätigt werden? Erwartete (gemeinsame) Kosten über der Bestätigungsschwelle oder über 50 % des
/// Restbudgets (`remaining = None`: unbegrenzt, keine 50-%-Regel). Ohne Bestätigung warten KI-Aufträge mit
/// `Pending (confirmation)`.
pub fn confirmation_required(
    expected: MicroUsd,
    threshold: MicroUsd,
    remaining: Option<MicroUsd>,
) -> bool {
    expected > threshold || remaining.is_some_and(|r| u128::from(expected.0) * 2 > u128::from(r.0))
}

/// Ein Eintrag eines geplanten Requests (AI-012, Nachtrag 24.11.2026, Q00): z. B. eine Frage zu einer URL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlannedEntry {
    /// Cache-Schlüssel des Eintrags; `None` = nicht cachefähig.
    pub cache_key: Option<String>,
    /// Decisions API: Zeichen des Fragentexts Z_q ([`super::decisions::question_text_chars`]); sonst 0.
    pub question_chars: u64,
    /// Ersatzweg einer Entscheidungsfrage: Gewicht für W (`predicate` 2, `choice` Anzahl Werte, `score` Anzahl
    /// Levels, wie `question_weight` im Katalog); sonst 0.
    pub weight: u64,
}

/// Ein geplanter Request mit den Einträgen, die er bündelt (eine Eingabe, mehrere Fragen bzw. ein Eintrag).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlannedRequest {
    /// Zeichenzahl der aufbereiteten Eingabe (nach Kürzung/Schwärzung).
    pub input_chars: u64,
    pub images: u64,
    pub entries: Vec<PlannedEntry>,
}

/// Ausführungsweg der Aufgabe für die Schätzung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimatePath {
    /// Decisions API: Eingabe je Frage mit Grundanteil G, Ausgabe 0, keine Reasoning-Stufe.
    DecisionsApi { question_overhead_tokens: u64 },
    /// Strukturierte Ausgabe bzw. Freitext.
    Prompt {
        /// Fester Anteil für Anweisungen je Request (ohne Schema-Zuschlag).
        instruction_tokens: u64,
        /// Schema-Zuschlag (`schema_overhead_tokens` aus `provider-limits.json`); 0 ohne Schema.
        schema_overhead_tokens: u64,
        /// Antwortanteil A (fest oder `decision-w`, AI-011).
        answer: crate::catalog::AnswerTokens,
        /// Effektive Reasoning-Stufe (AI-011).
        reasoning: crate::provider::ReasoningLevel,
    },
}

/// Eingaben des Voranschlags je Aufgabe nach dem Nachtrag Q00.
#[derive(Debug, Clone)]
pub struct RequestEstimateInput<'a> {
    pub task: String,
    pub model: String,
    pub requests: &'a [PlannedRequest],
    pub path: EstimatePath,
    /// `None` = kein Preis für den Endpunkt.
    pub basis: Option<super::basis::PriceBasis<'a>>,
    pub api_key_present: bool,
    /// Eingabe-Korrekturfaktor je Modell.
    pub calibration: Calibration,
    /// Ausgabe-Korrekturfaktor je (Modell, Aufgabe, effektive Stufe), [`super::output::output_calibration_from_log`].
    pub output_calibration: Calibration,
    pub batch_capable: bool,
}

/// Zeile des Abschnitts „KI“ mit Spalte `Reasoning` (effektive Stufe, leer über die Decisions API) und der Zahl der
/// zu sendenden Requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestTaskEstimate {
    pub estimate: TaskEstimate,
    pub reasoning: Option<crate::provider::ReasoningLevel>,
    pub requests: u64,
}

/// Geschätzte Eingabe-Tokens eines geplanten Requests vor dem Korrekturfaktor, nur über die nicht gecachten Einträge
/// (`uncached`); `None`, wenn kein Eintrag gesendet wird.
pub fn planned_input_tokens(
    path: &EstimatePath,
    request: &PlannedRequest,
    uncached: &[&PlannedEntry],
    image_token_estimate: u64,
) -> Option<u64> {
    if uncached.is_empty() {
        return None;
    }
    Some(match path {
        EstimatePath::DecisionsApi {
            question_overhead_tokens,
        } => {
            let z_q: Vec<u64> = uncached.iter().map(|e| e.question_chars).collect();
            super::decisions::decisions_input_tokens(
                request.input_chars,
                request.images,
                image_token_estimate,
                &z_q,
                *question_overhead_tokens,
            )
        }
        EstimatePath::Prompt {
            instruction_tokens,
            schema_overhead_tokens,
            ..
        } => estimate_input_tokens(
            request.input_chars,
            request.images,
            image_token_estimate,
            instruction_tokens.saturating_add(*schema_overhead_tokens),
        ),
    })
}

/// Voranschlag einer Aufgabe nach dem Nachtrag 24.11.2026, Q00 (AI-012): Decisions-Formel je Frage, Ausgabe-Schätzung
/// (A + E) × Ausgabefaktor, Schema-Zuschlag, Spalte `Reasoning`. Gecachte Einträge zählen in `cached` und tragen 0 bei;
/// ein Request, dessen Einträge alle gecacht sind, wird nicht gesendet. Gebühren je gesendetem Request.
pub fn estimate_requests(
    input: &RequestEstimateInput<'_>,
    cache: &dyn CachedLookup,
) -> RequestTaskEstimate {
    let image_estimate = input
        .basis
        .and_then(|b| b.estimate_rates(0))
        .map_or(0, |r| r.image_token_estimate);
    let (mut items, mut cached, mut sent) = (0u64, 0u64, 0u64);
    let (mut tokens_in, mut tokens_out) = (0u64, 0u64);
    let (mut sync, mut batch) = (MicroUsd(0), MicroUsd(0));
    let mut priced = input.basis.is_some();
    for request in input.requests {
        items += request.entries.len() as u64;
        let uncached: Vec<&PlannedEntry> = request
            .entries
            .iter()
            .filter(|e| !e.cache_key.as_deref().is_some_and(|k| cache.is_cached(k)))
            .collect();
        cached += (request.entries.len() - uncached.len()) as u64;
        let Some(raw) = planned_input_tokens(&input.path, request, &uncached, image_estimate)
        else {
            continue;
        };
        sent += 1;
        let tokens_in_request = input.calibration.apply(raw);
        let tokens_out_request = match input.path {
            EstimatePath::DecisionsApi { .. } => 0,
            EstimatePath::Prompt {
                answer, reasoning, ..
            } => {
                let weight = uncached.iter().map(|e| e.weight).sum();
                super::output::estimate_output_tokens(
                    answer.tokens(Some(weight)),
                    reasoning,
                    &input.output_calibration,
                )
            }
        };
        tokens_in = tokens_in.saturating_add(tokens_in_request);
        tokens_out = tokens_out.saturating_add(tokens_out_request);
        let rates = input
            .basis
            .and_then(|b| b.estimate_rates(tokens_in_request));
        match rates {
            Some(rates) => {
                let t = TokenCounts::plain(tokens_in_request, tokens_out_request);
                let fees = FeeCounts::one_request(request.images);
                sync = MicroUsd(sync.0.saturating_add(cost_of(&t, fees, &rates, false).cost.0));
                batch = MicroUsd(batch.0.saturating_add(cost_of(&t, fees, &rates, true).cost.0));
            }
            None => priced = false,
        }
    }
    let executable = if !priced {
        Executable::PriceUnknown
    } else if !input.api_key_present {
        Executable::MissingApiKey
    } else {
        Executable::Yes
    };
    let (lo, hi) = input.calibration.spread_ppm();
    RequestTaskEstimate {
        estimate: TaskEstimate {
            task: input.task.clone(),
            model: input.model.clone(),
            items,
            cached,
            est_input_tokens: tokens_in,
            est_output_tokens: tokens_out,
            low: priced.then(|| scale(sync, lo)),
            expected: priced.then_some(sync),
            high: priced.then(|| scale(sync, hi)),
            batch_savings: priced.then(|| {
                if input.batch_capable {
                    sync.saturating_sub(batch)
                } else {
                    MicroUsd(0)
                }
            }),
            executable,
            calibrated: input.calibration.calibrated,
        },
        reasoning: match input.path {
            EstimatePath::DecisionsApi { .. } => None,
            EstimatePath::Prompt { reasoning, .. } => Some(reasoning),
        },
        requests: sent,
    }
}

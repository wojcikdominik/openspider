//! Ablauf eines Vorschlagslaufs je Element (AI-070, AI-071, AI-072, AI-075; AI-012 … AI-021).
//!
//! Je URL: Eingabe aufbereiten (Feldauswahl, Schwärzung, [`PreparedInput`]), Cache prüfen (AI-015), Budget reservieren
//! (AI-013), **genau ein** Request über [`crate::policy::guard::execute`] (Vorschau-Schalter → Einwilligung → Routing →
//! Adapter; der einzige Weg zu den Adaptern), lokale Schema-Prüfung mit Reparaturversuch (AI-021), Prüfung jedes
//! Kandidaten (AI-075); ist keiner gültig, folgen höchstens `repair_rounds` Reparaturrunden mit den konkreten
//! Verstößen. Jede Antwort wird **atomar** übernommen: Protokollzeilen (AI-014, über `RequestMeta::request_outcome`),
//! Cache-Eintrag und Elementeintrag in einer Transaktion ([`SuggestStore::commit`]), danach `BudgetLedger::settle`.
//! Wiederholungen, Timeouts und der Schutzschalter laufen über [`Limiter::run_classified`] (nur eine Antwort mit Inhalt
//! setzt zurück, Refusals sind neutral, AI-017 Nachtrag 26.11.2026). Nach Abschluss aller URLs läuft der Nachlauf der
//! Dublettenprüfung (AI-071 Stufe 2) ohne Request; bis dahin sind die Einträge vorläufig.
//!
//! Lebenszyklus (AI-020, Nachtrag 26.11.2026): Vor dem Senden sind alle Einträge `Pending`. Nach [`RunControl::stop`]
//! (Pause bzw. „Gestoppt (endgültig)“) geht kein weiterer Request hinaus; laufende werden abgewartet und übernommen,
//! offene bleiben `Pending` und belegen keine Reservierung (reserviert wird unmittelbar vor dem Senden). Bricht der
//! Prozess ab, fehlt für einen nicht übernommenen Request jede Spur (keine Protokollzeile, keine Kosten).
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::task::Poll;

use os_model::ai::AiStatus;
use os_model::budget::{BudgetLedger, BudgetScope, MicroUsd, Reservation};
use os_model::time::Clock;
use os_store::ai::cache::{CacheRow, STATUS_REFUSAL, STATUS_SUCCESS};
use os_store::ai::requests::{RequestDetails, RequestRecord};
use os_store::ai::suggestions::{ElementRecord, StoredCandidate, SuggestionCommit};
use serde_json::{json, Value};

use super::dedupe::{post_pass, select, SiteIndex};
use super::element::Element;
use super::limits::{ElementLimits, SuggestOptions};
use super::prompt::{self, PromptContext, PROMPT_VERSION};
use super::style::StyleRules;
use super::validate::{Checker, PixelMeter};
use crate::adapters::openai_responses::RequestMeta;
use crate::cache::{
    canonical_json, input_hash, CacheKey, CacheKeyInput, CacheOptions, ExecutionPath,
};
use crate::catalog::{AnswerTokens, TaskPlan};
use crate::cost::{
    cache_hit_record, detailed_request_record, raw_output_tokens, reservation_for, LogContext,
    OutcomeDetails, PriceBasis,
};
use crate::limits::{AttemptFailure, BreakerEffect, FinalError, Limiter, RequestSpec};
use crate::policy::{
    guard, ConsentGate, FieldValue, FieldValues, InputField, PreparedInput, Redactor,
};
use crate::preview::PreviewSwitch;
use crate::provider::{
    AiProvider, Capabilities, Model, Outcome, ProviderError, ProviderFuture, Request, Response,
};
use crate::schema::{
    run_with_repair, FailureStatus, RawAnswer, RepairAttempts, ResponseSchema, StructuredOutcome,
};

/// Zukunft der Speicherschnittstelle.
pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

/// Gecachtes Ergebnis (AI-015: Erfolge und Refusals).
#[derive(Debug, Clone, PartialEq)]
pub enum CachedAnswer {
    Success(Value),
    Refusal,
}

/// Speicher eines Laufs: in der App die Schreibwarteschlange der Crawl-Datei, in Tests und im CLI [`FileStore`].
pub trait SuggestStore: Send + Sync {
    fn cached(&self, key: String) -> StoreFuture<'_, Option<CachedAnswer>>;
    /// Schreibt Protokollzeilen, Cache-Einträge und Elementeintrag atomar; liefert die neuen Protokoll-IDs.
    fn commit(&self, commit: SuggestionCommit) -> StoreFuture<'_, Vec<i64>>;
    fn load_all(&self, element: Element) -> StoreFuture<'_, Vec<ElementRecord>>;
    fn save_all(&self, records: Vec<ElementRecord>) -> StoreFuture<'_, ()>;
    fn mark_pending(&self, element: Element, url_ids: Vec<i64>, run_id: i64)
        -> StoreFuture<'_, ()>;
}

/// [`SuggestStore`] über einer geöffneten Crawl-Datei.
pub struct FileStore {
    file: Mutex<os_store::CrawlFile>,
}

impl FileStore {
    pub fn new(file: os_store::CrawlFile) -> Self {
        Self {
            file: Mutex::new(file),
        }
    }

    /// Zugriff auf die Datei (z. B. für Prüfungen in Tests).
    pub fn with<R>(&self, f: impl FnOnce(&os_store::CrawlFile) -> R) -> R {
        let guard = self.file.lock().unwrap_or_else(|p| p.into_inner());
        f(&guard)
    }

    pub fn into_inner(self) -> os_store::CrawlFile {
        self.file.into_inner().unwrap_or_else(|p| p.into_inner())
    }
}

fn store_err(e: os_store::StoreError) -> String {
    e.to_string()
}

impl SuggestStore for FileStore {
    fn cached(&self, key: String) -> StoreFuture<'_, Option<CachedAnswer>> {
        let result = self.with(|f| {
            let row = os_store::ai::cache::get(f.conn(), &key).map_err(store_err)?;
            Ok(row.and_then(|row| {
                if row.status == STATUS_REFUSAL {
                    Some(CachedAnswer::Refusal)
                } else {
                    serde_json::from_str(&row.result_json)
                        .ok()
                        .map(CachedAnswer::Success)
                }
            }))
        });
        Box::pin(async move { result })
    }
    fn commit(&self, commit: SuggestionCommit) -> StoreFuture<'_, Vec<i64>> {
        let result =
            self.with(|f| os_store::ai::suggestions::commit(f.conn(), &commit).map_err(store_err));
        Box::pin(async move { result })
    }
    fn load_all(&self, element: Element) -> StoreFuture<'_, Vec<ElementRecord>> {
        let result = self.with(|f| {
            os_store::ai::suggestions::load_all(f.conn(), element.id()).map_err(store_err)
        });
        Box::pin(async move { result })
    }
    fn save_all(&self, records: Vec<ElementRecord>) -> StoreFuture<'_, ()> {
        let result = self
            .with(|f| os_store::ai::suggestions::save_all(f.conn(), &records).map_err(store_err));
        Box::pin(async move { result })
    }
    fn mark_pending(
        &self,
        element: Element,
        url_ids: Vec<i64>,
        run_id: i64,
    ) -> StoreFuture<'_, ()> {
        let result = self.with(|f| {
            os_store::ai::suggestions::mark_pending(f.conn(), element.id(), &url_ids, Some(run_id))
                .map_err(store_err)
        });
        Box::pin(async move { result })
    }
}

/// Eingabe einer Seite (Kontext nach AI-070). Der Seitentext ist bereits nach INTEG-172 gekürzt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageInput {
    pub url_id: i64,
    pub address: String,
    pub title: Option<String>,
    pub meta_description: Option<String>,
    pub h1: Option<String>,
    /// Seitentext-Auszug.
    pub text: Option<String>,
    /// Seitensprache (BCP-47), z. B. aus `<html lang>`, sonst aus der lokalen Erkennung.
    pub language: Option<String>,
    /// Seitentyp (AI-050), sofern vorhanden.
    pub page_type: Option<String>,
    /// Suchintention (AI-051), sofern vorhanden.
    pub search_intent: Option<String>,
    /// Fokus-Begriff (CSV-Import `Address,Keyword`).
    pub focus: Option<String>,
}

impl PageInput {
    /// Feldwerte vor Feldauswahl und Schwärzung.
    pub fn field_values(&self) -> FieldValues {
        let mut v = FieldValues::new();
        let mut put = |field: InputField, value: Option<&str>| {
            if let Some(value) = value.map(str::trim).filter(|s| !s.is_empty()) {
                v.insert(field, FieldValue::Text(value.to_string()));
            }
        };
        put(InputField::Url, Some(&self.address));
        put(InputField::Title, self.title.as_deref());
        put(
            InputField::MetaDescription,
            self.meta_description.as_deref(),
        );
        put(InputField::H1, self.h1.as_deref());
        put(InputField::PageText, self.text.as_deref());
        put(InputField::HtmlLang, self.language.as_deref());
        put(InputField::PageType, self.page_type.as_deref());
        v
    }
}

/// Steuerung eines laufenden Laufs (Pause und Stoppen nach AI-020, Nachtrag 26.11.2026).
#[derive(Debug, Default)]
pub struct RunControl {
    stopped: AtomicBool,
}

impl RunControl {
    /// Ab jetzt kein neuer Request; laufende werden abgewartet und übernommen.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

/// Ein Element-Lauf.
pub struct ElementJob<'a> {
    pub element: Element,
    /// URLs des Geltungsbereichs (jede höchstens einmal).
    pub pages: &'a [PageInput],
    pub index: &'a SiteIndex,
    pub limits: ElementLimits,
    pub options: SuggestOptions,
}

/// Ausführung eines Laufs: alle Abhängigkeiten aus dem Kern (K01 … K10) und der Anwendung.
pub struct Engine<'a> {
    pub switch: PreviewSwitch,
    pub consent: &'a ConsentGate,
    pub redactor: &'a Redactor,
    pub provider: &'a dyn AiProvider,
    pub limiter: &'a Limiter,
    pub ledger: &'a dyn BudgetLedger,
    /// Modell, effektive Reasoning-Stufe und `max_tokens` (AI-011, `catalog::plan_task`).
    pub plan: &'a TaskPlan,
    pub price: PriceBasis<'a>,
    pub store: &'a dyn SuggestStore,
    pub meter: &'a dyn PixelMeter,
    pub style: &'a dyn StyleRules,
    pub clock: &'a dyn Clock,
    pub control: &'a RunControl,
    pub run_id: i64,
    /// Endpunkt-Basis-URL (Cache-Schlüssel, global/EU).
    pub base_url: String,
    pub cache: CacheOptions,
    /// Schema-Zuschlag der Eingabeschätzung (`provider-limits.json`, AI-012).
    pub schema_overhead_tokens: u64,
    /// Gleichzeitig bearbeitete URLs (die Anbietergrenze setzt zusätzlich der [`Limiter`]).
    pub workers: usize,
}

/// Ergebnis eines Laufs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSummary {
    pub entries: usize,
    /// Gesendete Requests (ohne Cache-Treffer).
    pub requests: usize,
    pub cache_hits: usize,
    /// Endstatus je Eintrag.
    pub statuses: BTreeMap<String, usize>,
    pub stopped: bool,
}

/// Fehler, der einen Lauf gar nicht erst starten lässt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// Schalter „KI (Vorschau)“ aus: kein Request, keine Zeile (Plan M4 E1).
    PreviewOff,
    Options(String),
    Store(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreviewOff => f.write_str("AI preview switch is off"),
            Self::Options(e) => write!(f, "invalid options: {e}"),
            Self::Store(e) => write!(f, "storage: {e}"),
        }
    }
}

impl std::error::Error for RunError {}

/// Führt `futures` nebenläufig im selben Task aus (ohne `tokio::spawn`: die Crawl-Datei ist nicht `'static`).
pub async fn join_all<F: Future<Output = ()>>(futures: Vec<F>) {
    let mut slots: Vec<Option<Pin<Box<F>>>> =
        futures.into_iter().map(|f| Some(Box::pin(f))).collect();
    std::future::poll_fn(|cx| {
        let mut pending = false;
        for slot in &mut slots {
            if let Some(f) = slot {
                if f.as_mut().poll(cx).is_ready() {
                    *slot = None;
                } else {
                    pending = true;
                }
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}

/// Protokolldaten des letzten Requests des Adapters hinter [`Metered`].
#[derive(Debug, Clone, Default)]
struct Captured {
    meta: Option<RequestMeta>,
    final_error: Option<FinalError>,
}

/// Adapter mit Wiederholungen, Timeout und Schutzschalter über den [`Limiter`] (AI-017) und Protokolldaten je Request
/// ([`RequestMeta`]). Steht hinter [`guard::execute`], damit Einwilligung, Routing und Schwärzung unverändert vorher
/// greifen.
struct Metered<'a> {
    inner: &'a dyn AiProvider,
    limiter: &'a Limiter,
    estimated_input_tokens: u64,
    captured: Mutex<Captured>,
}

impl<'a> Metered<'a> {
    fn new(inner: &'a dyn AiProvider, limiter: &'a Limiter, estimated_input_tokens: u64) -> Self {
        Self {
            inner,
            limiter,
            estimated_input_tokens,
            captured: Mutex::new(Captured::default()),
        }
    }
    fn take(&self) -> Captured {
        std::mem::take(&mut *self.captured.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

fn attempt_failure(error: ProviderError) -> AttemptFailure {
    match &error {
        ProviderError::OutputLimitReached { reason } => {
            let reason = reason.clone();
            let mut f = AttemptFailure::from(error).with_error_code(reason);
            f.http_status = Some(200);
            f
        }
        _ => AttemptFailure::from(error),
    }
}

impl AiProvider for Metered<'_> {
    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }
    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        self.inner.models()
    }
    fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
        Box::pin(async move {
            let spec = RequestSpec::new(
                request.model.provider.clone(),
                request.model.id.clone(),
                self.estimated_input_tokens,
            );
            let exec = self
                .limiter
                .run_classified(
                    &spec,
                    |_| {
                        let request = request.clone();
                        async move { self.inner.execute(request).await.map_err(attempt_failure) }
                    },
                    |response: &Response| {
                        if response.refusal {
                            BreakerEffect::Neutral
                        } else {
                            BreakerEffect::Reset
                        }
                    },
                )
                .await;
            let reply = exec
                .result
                .as_ref()
                .ok()
                .map(|r| (200u16, Some(r.usage.clone()), r.provider_request_id.clone()));
            let meta = RequestMeta::from_execution(&exec, reply);
            let mut captured = self.captured.lock().unwrap_or_else(|p| p.into_inner());
            captured.meta = Some(meta);
            captured.final_error = exec.result.as_ref().err().cloned();
            drop(captured);
            exec.result.map_err(|e| e.error)
        })
    }
}

/// Ein Request innerhalb einer Runde.
#[derive(Debug, Clone)]
struct Sent {
    reservation: Option<Reservation>,
    amount: MicroUsd,
    estimated_input_tokens: u64,
    estimated_output_tokens: u64,
    redactions: u32,
    sent: bool,
    meta: Option<RequestMeta>,
}

/// Fehlschlag einer Runde mit dem Endstatus nach AI-020.
#[derive(Debug, Clone)]
struct RoundFailure {
    status: AiStatus,
    sent: Option<Sent>,
    /// Stoppen vor dem Senden (Eintrag bleibt `Pending`).
    stopped: bool,
}

impl FailureStatus for RoundFailure {
    fn failure_status(&self) -> AiStatus {
        self.status.clone()
    }
}

/// Ergebnis einer Runde.
enum Round {
    Candidates(Vec<String>),
    Status(AiStatus),
    Stopped,
}

fn candidates_of(value: &Value) -> Vec<String> {
    value["candidates"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

impl Engine<'_> {
    fn task(&self, element: Element) -> &'static str {
        element.task().as_str()
    }

    fn halted(&self, halt: &Mutex<Option<AiStatus>>) -> Option<AiStatus> {
        halt.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Führt den Lauf eines Elements aus und schließt mit dem Nachlauf ab.
    pub async fn run(&self, job: &ElementJob<'_>) -> Result<RunSummary, RunError> {
        if !self.switch.is_on() {
            return Err(RunError::PreviewOff);
        }
        let options = job
            .options
            .validated()
            .map_err(|e| RunError::Options(e.to_string()))?;
        let mut pages: Vec<&PageInput> = job.pages.iter().collect();
        pages.sort_by(|a, b| a.address.as_bytes().cmp(b.address.as_bytes()));
        pages.dedup_by_key(|p| p.url_id);
        let ids: Vec<i64> = pages.iter().map(|p| p.url_id).collect();
        self.store
            .mark_pending(job.element, ids.clone(), self.run_id)
            .await
            .map_err(RunError::Store)?;
        let queue = Mutex::new(pages.into_iter().collect::<VecDeque<_>>());
        let halt = Mutex::new(None);
        let counters = Mutex::new((0usize, 0usize));
        let workers = self.workers.max(1).min(ids.len().max(1));
        let worker = || async {
            loop {
                let next = queue.lock().unwrap_or_else(|p| p.into_inner()).pop_front();
                let Some(page) = next else { break };
                let (sent, hits) = self.process(job, options, page, &halt).await;
                let mut c = counters.lock().unwrap_or_else(|p| p.into_inner());
                c.0 += sent;
                c.1 += hits;
            }
        };
        join_all((0..workers).map(|_| worker()).collect()).await;
        let stopped = self.control.is_stopped();
        let mut records = self
            .store
            .load_all(job.element)
            .await
            .map_err(RunError::Store)?;
        if !stopped {
            records = post_pass(records, job.index, options.rules.duplicates);
            self.store
                .save_all(records.clone())
                .await
                .map_err(RunError::Store)?;
        }
        let mut statuses = BTreeMap::new();
        for r in records.iter().filter(|r| ids.contains(&r.url_id)) {
            *statuses.entry(r.status.clone()).or_insert(0) += 1;
        }
        let (requests, cache_hits) = *counters.lock().unwrap_or_else(|p| p.into_inner());
        Ok(RunSummary {
            entries: ids.len(),
            requests,
            cache_hits,
            statuses,
            stopped,
        })
    }

    fn checker<'c>(&'c self, job: &'c ElementJob<'_>, options: SuggestOptions) -> Checker<'c> {
        Checker {
            element: job.element,
            limits: job.limits,
            rules: options.rules,
            meter: self.meter,
            index: job.index,
            style: self.style,
        }
    }

    fn log_context(&self, element: Element, url_id: i64, redactions: u32) -> LogContext {
        LogContext {
            run_id: self.run_id,
            timestamp: self.clock.now().to_db(),
            task: self.task(element).to_string(),
            provider: self.plan.model.provider.clone(),
            model: self.plan.model.id.clone(),
            endpoint: self.plan.model.endpoint.clone(),
            url_id: Some(url_id),
            definition_version: Some(PROMPT_VERSION.to_string()),
            batch_id: None,
            redactions,
        }
    }

    /// Protokollzeile eines gesendeten Requests (AI-014) über `RequestMeta::request_outcome`.
    fn log_row(
        &self,
        element: Element,
        url_id: i64,
        sent: &Sent,
        status: AiStatus,
        validator_error: Option<String>,
    ) -> Option<(RequestRecord, RequestDetails)> {
        if !sent.sent {
            return None;
        }
        let meta = sent.meta.clone().unwrap_or_default();
        let mut outcome = meta.request_outcome(
            status,
            sent.estimated_input_tokens,
            sent.estimated_output_tokens,
            0,
        );
        if let Some(error) = validator_error {
            outcome.error_message = Some(error);
        }
        let tokens = meta
            .usage
            .as_ref()
            .and_then(|u| u.input_tokens)
            .unwrap_or(sent.estimated_input_tokens);
        let rates = self
            .price
            .actual_rates(tokens)
            .or_else(|| self.price.estimate_rates(tokens))?;
        let row = detailed_request_record(
            &self.log_context(element, url_id, sent.redactions),
            &rates,
            sent.amount,
            &outcome,
            &OutcomeDetails {
                reasoning: Some(self.plan.reasoning.effective),
                reported_cost: None,
                estimated_output_tokens: Some(sent.estimated_output_tokens),
            },
        );
        Some((row.record, row.details))
    }

    fn cache_key(
        &self,
        job: &ElementJob<'_>,
        options: SuggestOptions,
        page: &PageInput,
        prepared: &PreparedInput,
        feedback: Option<&str>,
    ) -> CacheKey {
        let input = CacheKeyInput {
            task: self.task(job.element).to_string(),
            definition_id: job.element.definition_id().to_string(),
            definition_version: PROMPT_VERSION.to_string(),
            params: json!({
                "candidates": options.candidates,
                "limits": job.limits,
                "language": page.language,
                "focus": page.focus,
                "search_intent": page.search_intent,
                "style": self.style.prompt_lines(job.element),
                "feedback": feedback,
            }),
            path: Some(ExecutionPath::StructuredOutput),
            provider: self.plan.model.provider.clone(),
            model: self.plan.model.id.clone(),
            endpoint: self.base_url.clone(),
            result_params: json!({}),
            input_hash: input_hash(&prepared.inputs()),
        };
        CacheKey::with_reasoning(input, self.plan.reasoning.effective)
    }

    fn cache_row(&self, key: &CacheKey, status: &str, result: &Value) -> CacheRow {
        let input = key.input();
        CacheRow {
            cache_key: key.as_str().to_string(),
            provider: input.provider.clone(),
            model: input.model.clone(),
            endpoint: input.endpoint.clone(),
            definition_id: input.definition_id.clone(),
            definition_version: input.definition_version.clone(),
            input_hash: input.input_hash.clone(),
            params_hash: key.params_hash().to_string(),
            result_json: canonical_json(result),
            status: status.to_string(),
            created_at: self.clock.now().to_db(),
        }
    }

    async fn commit(
        &self,
        requests: Vec<(RequestRecord, RequestDetails)>,
        cache: Vec<CacheRow>,
        record: &ElementRecord,
        sent: &[Sent],
    ) {
        let costs: Vec<MicroUsd> = requests.iter().map(|(r, _)| r.cost).collect();
        let result = self
            .store
            .commit(SuggestionCommit {
                requests,
                cache,
                record: record.clone(),
            })
            .await;
        // Verbuchung nach dem Commit: gesendete Requests mit ihren Ist-Kosten, nicht gesendete werden freigegeben.
        let mut cost = costs.into_iter();
        for s in sent {
            let Some(reservation) = &s.reservation else {
                continue;
            };
            if s.sent && result.is_ok() {
                let actual = cost.next().unwrap_or(MicroUsd(0));
                let _ = self.ledger.settle(reservation.id, actual);
            } else {
                let _ = self.ledger.release(reservation.id);
            }
        }
    }

    /// Eine Runde: ein Request mit höchstens `schema_repairs` Reparaturversuchen (AI-021).
    #[allow(clippy::too_many_arguments)]
    async fn send_round(
        &self,
        job: &ElementJob<'_>,
        options: SuggestOptions,
        prepared: &PreparedInput,
        instructions: &str,
        halt: &Mutex<Option<AiStatus>>,
    ) -> (StructuredOutcome<RoundFailure>, Vec<(Sent, Option<String>)>) {
        let schema = ResponseSchema::new(prompt::schema()).expect("suggestion schema is valid");
        let repairs = RepairAttempts::new(options.schema_repairs).unwrap_or_default();
        let task = self.task(job.element);
        let answer = self
            .plan
            .answer_tokens
            .unwrap_or(AnswerTokens::Fixed(512))
            .tokens(None);
        let effective = self.plan.reasoning.effective;
        let run = run_with_repair(&schema, repairs, |repair| {
            let mut text = instructions.to_string();
            if let Some(repair) = &repair {
                text.push_str("\n\n");
                text.push_str(&repair.instruction());
            }
            let schema_value = schema.schema().clone();
            async move {
                if self.control.is_stopped() {
                    return Err(RoundFailure {
                        status: AiStatus::Pending,
                        sent: None,
                        stopped: true,
                    });
                }
                if let Some(status) = self.halted(halt) {
                    return Err(RoundFailure {
                        status,
                        sent: None,
                        stopped: false,
                    });
                }
                let mut request = Request {
                    model: self.plan.model.clone(),
                    required: Capabilities {
                        text: true,
                        structured_output: true,
                        ..Capabilities::default()
                    },
                    input: Vec::new(),
                    instructions: text.clone(),
                    questions: Vec::new(),
                    response_schema: Some(schema_value),
                    max_tokens: None,
                    temperature: None,
                    effort: None,
                    reasoning: Default::default(),
                };
                self.plan.apply_to(&mut request, None);
                let chars = crate::cost::tokens::char_count(&prepared.values.text_block())
                    + crate::cost::tokens::char_count(&text);
                let estimated_input_tokens = crate::cost::tokens::estimate_input_tokens(
                    chars,
                    0,
                    0,
                    self.schema_overhead_tokens,
                );
                let estimated_output_tokens = raw_output_tokens(answer, effective);
                let max_tokens = request
                    .max_tokens
                    .unwrap_or(estimated_output_tokens.saturating_mul(2));
                let Some(amount) =
                    reservation_for(self.price, estimated_input_tokens, max_tokens, 0, false)
                else {
                    return Err(RoundFailure {
                        status: AiStatus::SkippedNoCapableModel,
                        sent: None,
                        stopped: false,
                    });
                };
                let reservation = match self.ledger.reserve(BudgetScope::task("ai", task), amount) {
                    Ok(r) => r,
                    Err(error) => {
                        let status = crate::cost::reserve::status_for(&error)
                            .unwrap_or(AiStatus::BudgetExceeded);
                        if crate::cost::reserve::stops_all_ai_entries(&error)
                            || crate::cost::reserve::exhausted_task(&error) == Some(task)
                        {
                            *halt.lock().unwrap_or_else(|p| p.into_inner()) = Some(status.clone());
                        }
                        return Err(RoundFailure {
                            status,
                            sent: None,
                            stopped: false,
                        });
                    }
                };
                let metered = Metered::new(self.provider, self.limiter, estimated_input_tokens);
                let guarded =
                    guard::execute(self.switch, self.consent, task, prepared, &metered, request)
                        .await;
                let captured = metered.take();
                let sent = Sent {
                    reservation: Some(reservation),
                    amount,
                    estimated_input_tokens,
                    estimated_output_tokens,
                    redactions: guarded.redactions,
                    sent: guarded.sent && captured.meta.is_some(),
                    meta: captured.meta,
                };
                let fail = |status: AiStatus, sent: Sent| RoundFailure {
                    status,
                    sent: Some(sent),
                    stopped: false,
                };
                match guarded.outcome {
                    Outcome::Completed(response) => match response.content {
                        Some(value) => Ok((RawAnswer::Json(value), sent)),
                        None => Ok((
                            RawAnswer::Unparsable {
                                detail: "response has no content".into(),
                            },
                            sent,
                        )),
                    },
                    Outcome::Status(AiStatus::Refusal) => Ok((RawAnswer::Refusal, sent)),
                    Outcome::Status(AiStatus::InvalidOutput) if sent.sent => Ok((
                        RawAnswer::Unparsable {
                            detail: "response is not valid JSON".into(),
                        },
                        sent,
                    )),
                    Outcome::Status(status) => {
                        let status = captured.final_error.map_or(status, |e| e.status);
                        Err(fail(status, sent))
                    }
                    Outcome::ProviderFailure(error) => {
                        let status = captured.final_error.map_or_else(
                            || error.status().unwrap_or(AiStatus::ConnectionFailed),
                            |e| e.status,
                        );
                        Err(fail(status, sent))
                    }
                    Outcome::Disabled => Err(RoundFailure {
                        status: AiStatus::Pending,
                        sent: Some(sent),
                        stopped: true,
                    }),
                }
            }
        })
        .await;
        let mut sent: Vec<(Sent, Option<String>)> = run
            .requests
            .into_iter()
            .map(|r| (r.meta, r.validator_error))
            .collect();
        let outcome = match run.outcome {
            StructuredOutcome::Failed(mut failure) => {
                if let Some(s) = failure.sent.take() {
                    sent.push((s, None));
                }
                StructuredOutcome::Failed(failure)
            }
            other => other,
        };
        (outcome, sent)
    }

    /// Bearbeitet eine URL bis zum Endstatus (bzw. bis zum Stoppen); liefert (gesendete Requests, Cache-Treffer).
    async fn process(
        &self,
        job: &ElementJob<'_>,
        options: SuggestOptions,
        page: &PageInput,
        halt: &Mutex<Option<AiStatus>>,
    ) -> (usize, usize) {
        let element = job.element;
        let checker = self.checker(job, options);
        let mut record = ElementRecord::pending(page.url_id, element.id());
        record.run_id = Some(self.run_id);
        record.focus = page.focus.clone();
        record.model = Some(self.plan.model.id.clone());
        record.provisional = true;
        let prepared = PreparedInput::new(&page.field_values(), &element.fields(), self.redactor);
        let base = prompt::instructions(
            element,
            &job.limits,
            &PromptContext {
                candidates: options.candidates,
                language: page.language.as_deref(),
                focus: page.focus.as_deref(),
                search_intent: page.search_intent.as_deref(),
                style_lines: self.style.prompt_lines(element),
            },
        );
        let rounds = 1 + u32::from(options.repair_rounds);
        let mut feedback: Option<String> = None;
        let (mut requests_sent, mut cache_hits) = (0usize, 0usize);
        for round in 1..=rounds {
            if self.control.is_stopped() {
                break;
            }
            if let Some(status) = self.halted(halt) {
                record.status = status.to_string();
                record.provisional = false;
                self.commit(Vec::new(), Vec::new(), &record, &[]).await;
                break;
            }
            let instructions = match &feedback {
                Some(f) => format!("{base}\n\n{f}"),
                None => base.clone(),
            };
            let key = self.cache_key(job, options, page, &prepared, feedback.as_deref());
            let mut rows = Vec::new();
            let mut cache_rows = Vec::new();
            let mut sent_list: Vec<Sent> = Vec::new();
            let cached = if self.cache.use_cache {
                self.store
                    .cached(key.as_str().to_string())
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            };
            let cached =
                cached.filter(|c| !(self.cache.retry_refusals && *c == CachedAnswer::Refusal));
            let result = if let Some(hit) = cached {
                cache_hits += 1;
                let mut row = cache_hit_record(
                    &self.log_context(element, page.url_id, prepared.redactions.total()),
                    self.price
                        .estimate_rates(0)
                        .map_or(crate::cost::PriceSource::Table, |r| r.source),
                    0,
                );
                if hit == CachedAnswer::Refusal {
                    row.status = AiStatus::Refusal.to_string();
                }
                rows.push((row, RequestDetails::default()));
                match hit {
                    CachedAnswer::Success(value) => Round::Candidates(candidates_of(&value)),
                    CachedAnswer::Refusal => Round::Status(AiStatus::Refusal),
                }
            } else {
                let (outcome, sent) = self
                    .send_round(job, options, &prepared, &instructions, halt)
                    .await;
                let last = sent.len().saturating_sub(1);
                for (i, (s, validator_error)) in sent.iter().enumerate() {
                    let status = match (&outcome, validator_error) {
                        (_, Some(_)) => AiStatus::InvalidOutput,
                        (StructuredOutcome::Valid(_), None) if i == last => AiStatus::Success,
                        (StructuredOutcome::Refusal, None) if i == last => AiStatus::Refusal,
                        (StructuredOutcome::Failed(f), None) if i == last => f.status.clone(),
                        _ => AiStatus::InvalidOutput,
                    };
                    if let Some(row) =
                        self.log_row(element, page.url_id, s, status, validator_error.clone())
                    {
                        requests_sent += 1;
                        rows.push(row);
                    }
                    sent_list.push(s.clone());
                }
                match outcome {
                    StructuredOutcome::Valid(value) => {
                        if self.cache.use_cache {
                            cache_rows.push(self.cache_row(&key, STATUS_SUCCESS, &value));
                        }
                        Round::Candidates(candidates_of(&value))
                    }
                    StructuredOutcome::Refusal => {
                        if self.cache.use_cache {
                            cache_rows.push(self.cache_row(&key, STATUS_REFUSAL, &Value::Null));
                        }
                        Round::Status(AiStatus::Refusal)
                    }
                    StructuredOutcome::InvalidOutput { .. } => {
                        Round::Status(AiStatus::InvalidOutput)
                    }
                    StructuredOutcome::Failed(f) if f.stopped => Round::Stopped,
                    StructuredOutcome::Failed(f) => Round::Status(f.status),
                }
            };
            match result {
                Round::Stopped => {
                    // Gesendete Requests der Runde bleiben verbucht; der Eintrag bleibt offen.
                    self.commit(rows, cache_rows, &record, &sent_list).await;
                    break;
                }
                Round::Status(status) => {
                    record.status = status.to_string();
                    record.provisional = false;
                    record.rounds = round;
                    self.commit(rows, cache_rows, &record, &sent_list).await;
                    break;
                }
                Round::Candidates(values) => {
                    let first = record.candidates.len() as u32 + 1;
                    let new: Vec<StoredCandidate> = values
                        .iter()
                        .take(usize::from(options.candidates))
                        .enumerate()
                        .map(|(i, raw)| {
                            let c = checker.check(page.url_id, page.language.as_deref(), raw);
                            StoredCandidate {
                                index: first + i as u32,
                                valid: c.valid(),
                                value: c.value,
                                length: c.length,
                                pixel_width: Some(c.pixel_width),
                                violations: c.violations,
                                round,
                            }
                        })
                        .collect();
                    let any_valid = new.iter().any(|c| c.valid);
                    record.candidates.extend(new.iter().cloned());
                    record.rounds = round;
                    record.generated_at = Some(self.clock.now().to_db());
                    let finished = any_valid || round == rounds;
                    select(&mut record, finished);
                    if !finished {
                        record.status = os_store::ai::suggestions::STATUS_PENDING.to_string();
                    }
                    self.commit(rows, cache_rows, &record, &sent_list).await;
                    if finished {
                        break;
                    }
                    let refs: Vec<&StoredCandidate> = new.iter().collect();
                    feedback = Some(if refs.is_empty() {
                        format!(
                            "Your previous answer contained no candidates. Write {} new candidates.",
                            options.candidates
                        )
                    } else {
                        prompt::repair_feedback(&refs, options.candidates)
                    });
                }
            }
        }
        (requests_sent, cache_hits)
    }
}

/// Nachlauf ohne Request (AI-071 Stufe 2) für alle Einträge eines Elements, z. B. nach einer Neuprüfung.
pub async fn finish(
    store: &dyn SuggestStore,
    element: Element,
    index: &SiteIndex,
    duplicates: bool,
) -> Result<Vec<ElementRecord>, String> {
    let records = post_pass(store.load_all(element).await?, index, duplicates);
    store.save_all(records.clone()).await?;
    Ok(records)
}

/// Einträge ohne Request mit einem Status abschließen (z. B. `Error: Missing API key` oder `Skipped (no capable
/// model)` aus der Modellwahl, AI-011; ohne Schlüssel 0 Requests).
pub async fn finish_without_request(
    store: &dyn SuggestStore,
    element: Element,
    pages: &[PageInput],
    status: &AiStatus,
    run_id: i64,
) -> Result<(), String> {
    let ids: Vec<i64> = pages.iter().map(|p| p.url_id).collect();
    store.mark_pending(element, ids, run_id).await?;
    for page in pages {
        let mut record = ElementRecord::pending(page.url_id, element.id());
        record.status = status.to_string();
        record.focus = page.focus.clone();
        record.run_id = Some(run_id);
        store
            .commit(SuggestionCommit {
                requests: Vec::new(),
                cache: Vec::new(),
                record,
            })
            .await?;
    }
    Ok(())
}

//! Ausführung der Decision-Fragen eines KI-Laufs (AI-033, AI-034) mit Lebenszyklus der KI-Warteschlange (AI-020,
//! Nachtrag 26.11.2026, K4-13).
//!
//! Ablauf je Lauf ([`DecisionRun::run`]):
//! 1. Besitz für die Dauer des Laufs übernehmen ([`OwnerLease`], Anbindung an `os_ai::run_owner` aus M4-K11);
//! 2. jede verwendete Fragenversion unveränderlich in `ai_questions` ablegen (AI-031);
//! 3. je Eintrag (URL bzw. Bild × Frage): Geltungsbereich (`Skipped (scope)`), Routing (AI-035), Eingabe (AI-032) –
//!    zu sendende Einträge werden `Pending` (offen);
//! 4. Bündelung: gleiche gesendete Eingabe und gleicher Weg → ein Request, höchstens `max_questions` (Decisions,
//!    1–10) bzw. 10 Fragen (Ersatzweg mit Bündelung, sonst 1);
//! 5. optional Kostenvoranschlag mit Bestätigung (AI-012, nur bei neuem Lauf; beim Fortsetzen gilt die Bestätigung);
//! 6. Senden: Cache (AI-015), Budget-Reservierung unmittelbar vor dem Senden (AI-013), `policy::guard::execute` als
//!    einziger Weg zum Adapter (Vorschau-Schalter → Einwilligung → Routing → Schwärzung), Wiederholungen, Timeout und
//!    Schutzschalter über `Limiter::run_classified` (Refusal und Ausgabegrenze neutral, AI-017);
//! 7. Verarbeitung jeder Antwort atomar: Ergebnisse, Cache, Protokollzeile(n) in einer Transaktion, danach Abrechnung
//!    der Reservierung ([`os_store::ai::answers::commit_response`]).
//!
//! Lebenszyklus über [`RunControl`]: **Pause** und **Stopp** senden ab sofort keinen Request mehr, laufende werden
//! abgewartet und übernommen, offene Einträge bleiben `Pending` ohne Reservierung. **Abbruch** (geordnetes Beenden,
//! Schließen der Crawl-Datei) bricht laufende Requests ab, ohne sie zu übernehmen: Einträge `Pending`, keine
//! Protokollzeile, Reservierung freigegeben. Ein Absturz wirkt wie ein Abbruch: die Reservierung lebt nur im Speicher,
//! der Ledger wird beim Öffnen aus `ai_requests` aufgebaut. Fortsetzen = neuer Aufruf mit [`Selection::Open`]: sendet
//! genau die offenen Einträge, keine fertigen.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::Poll;

use os_model::ai::AiStatus;
use os_model::budget::{BudgetLedger, BudgetScope, MicroUsd, ReservationId, AREA_AI};
use os_model::Timestamp;
use os_store::ai::answers::{self, AnswerRow, ResponseCommit};
use os_store::ai::cache::{CacheRow, Connection};
use os_store::ai::requests::{RequestDetails, RequestRecord};
use os_store::StoreError;
use serde_json::{json, Value};
use tokio::sync::watch;

use super::fallback::{self, FallbackResult};
use super::format::{self, ParamValues, QuestionDef};
use super::input::{self, ImageResizer, SubjectData};
use super::library;
use super::routing::{self, Route, RoutingContext, RoutingSetting};
use crate::adapters::anthropic::shared::ReportedCost;
use crate::adapters::anthropic::{AdapterReply, AnthropicAdapter, OutputLimitReached};
use crate::adapters::ollama::OllamaAdapter;
use crate::adapters::openai_decisions::{
    decisions_billing_rates, DecisionAnswer, DecisionInput, DecisionsReply, InputPart,
    MaxQuestionsPerRequest, OpenAiDecisions, Question, QuestionResult,
};
use crate::adapters::openai_responses::image::from_base64;
use crate::adapters::openai_responses::{OpenAiResponses, RequestMeta};
use crate::adapters::openrouter::{OpenRouterAdapter, OpenRouterModel};
use crate::cache::{
    input_hash, CacheKey, CacheKeyInput, CacheOptions, ExecutionPath, Lookup, ResultCache,
};
use crate::catalog::{AnswerTokens, Endpoint, ProviderLimitsFile, TaskType};
use crate::cost::{
    self, cache_hit_record, confirmation_required, detailed_request_record, estimate_requests,
    Calibration, EstimatePath, EstimateTotals, LogContext, OutcomeDetails, PlannedEntry,
    PlannedRequest, PriceBasis, PriceSource, Rates, RequestEstimateInput, RequestTaskEstimate,
};
use crate::limits::{BreakerEffect, FinalError, Limiter, RequestSpec};
use crate::policy::{self, ConsentGate, PreparedInput, Redactor};
use crate::preview::PreviewSwitch;
use crate::provider::{
    AiProvider, Capabilities, Input, Model, Outcome, ProviderError, ProviderFuture, Request,
    Response,
};
use crate::schema::{run_with_repair, RepairAttempts, ResponseSchema, StructuredOutcome};

/// Höchstzahl Fragen je gebündeltem Request im Ersatzweg (wie die Decisions API bis zur GA; Auslegung, Bericht d01).
pub const MAX_FALLBACK_BUNDLE: usize = 10;

// ---------------------------------------------------------------------------------------------------------------
// Eingaben eines Laufs

/// Eine aktive Frage mit ihrer Einstellung „Ausführung“ (AI-035). Reihenfolge = Reihenfolge der aktiven Fragen.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveQuestion {
    pub def: QuestionDef,
    pub routing: RoutingSetting,
}

/// Ein Element der Grundmenge G: interne Seiten-URL bzw. internes Bild mit den Angaben für den Geltungsbereich und den
/// Eingabefeldern.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunItem {
    pub url_id: i64,
    /// Bild statt Seite (Bildfragen gelten nur für Bilder, Seitenfragen nur für Seiten).
    pub is_image: bool,
    /// Inhaltsklasse (`html`, `image`, …); leer bei Antworten ohne verwertbaren Inhalt.
    pub content_class: String,
    pub status_code: Option<u16>,
    pub indexable: bool,
    /// Segmente der URL (CRAWL-134).
    pub segments: Vec<String>,
    pub data: SubjectData,
    /// Parameterwerte je Vorlagen-`id` (parametrisierte Systemfragen, AI-030).
    pub params: BTreeMap<String, ParamValues>,
}

/// Adapter der verbundenen Anbieter (Instanzen gelten für einen Lauf: Kontingentsperren je Instanz).
#[derive(Clone, Copy, Default)]
pub struct Adapters<'a> {
    pub decisions: Option<&'a OpenAiDecisions>,
    pub responses: Option<&'a OpenAiResponses>,
    pub anthropic: Option<&'a AnthropicAdapter>,
    pub openrouter: Option<&'a OpenRouterAdapter>,
    pub ollama: Option<&'a OllamaAdapter>,
}

/// Preisgrundlage je (Anbieter, Modell, Endpunkt); OpenRouter zusätzlich über die Modellliste des Adapters.
pub trait PriceLookup {
    /// Aufgelöste Sätze (Preistabelle, Nutzerüberschreibung); `None` = Preis unbekannt.
    fn rates(&self, provider: &str, model: &str, endpoint: &str) -> Option<Rates>;
}

impl<F> PriceLookup for F
where
    F: Fn(&str, &str, &str) -> Option<Rates>,
{
    fn rates(&self, provider: &str, model: &str, endpoint: &str) -> Option<Rates> {
        self(provider, model, endpoint)
    }
}

/// Welche Einträge ein Lauf sendet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Selection {
    /// Offene Einträge (`Pending`, `Pending (confirmation)`, `Pending (consent)`) und Einträge ohne Zeile: neuer Lauf,
    /// „Fortsetzen“, „KI-Analyse starten“ nach dem Stoppen.
    #[default]
    Open,
    /// „Fehlgeschlagene erneut anfragen“: nur `Error: …`, `Quota exceeded`, `Budget exceeded` (AI-020).
    Failed,
    /// „Neu analysieren“: alle Einträge im Geltungsbereich (erfolgreiche Ergebnisse kommen aus dem Cache).
    All,
}

/// Kostenbestätigung (AI-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    /// Bestätigt bzw. Fortsetzen eines bestätigten Laufs: kein Voranschlag (AI-020, Nachtrag 26.11.2026).
    Granted,
    /// Neuer Lauf: Voranschlag; erreicht er die Schwelle (≥, Nachtrag 26.11.2026) oder liegt er über 50 % des
    /// Restbudgets, werden die Einträge `Pending (confirmation)` und nichts wird gesendet.
    AskAbove(MicroUsd),
}

/// Einstellungen eines Laufs.
#[derive(Clone)]
pub struct RunOptions {
    /// Höchstzahl Fragen je Decisions-Request (1–10, Standard 10).
    pub max_questions: MaxQuestionsPerRequest,
    /// „Fragen im Fallback bündeln“ (Standard an).
    pub bundle_fallback: bool,
    /// Höchstzahl gleichzeitig laufender Requests dieses Laufs (zusätzlich begrenzt der Limiter je Anbieter).
    pub concurrency: usize,
    /// Reparaturversuche bei Schemaverstoß (AI-021).
    pub repairs: RepairAttempts,
    pub cache: CacheOptions,
    pub selection: Selection,
    pub confirmation: Confirmation,
    pub resizer: Option<Arc<dyn ImageResizer>>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_questions: MaxQuestionsPerRequest::default(),
            bundle_fallback: true,
            concurrency: 1,
            repairs: RepairAttempts::default(),
            cache: CacheOptions::default(),
            selection: Selection::Open,
            confirmation: Confirmation::Granted,
            resizer: None,
        }
    }
}

impl std::fmt::Debug for RunOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunOptions")
            .field("max_questions", &self.max_questions)
            .field("bundle_fallback", &self.bundle_fallback)
            .field("concurrency", &self.concurrency)
            .field("selection", &self.selection)
            .field("confirmation", &self.confirmation)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Steuerung und Besitz

/// Steuerzustand eines Laufs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlState {
    Running,
    /// Pause des Crawls (CRAWL-126): kein neuer Request, laufende werden übernommen.
    Paused,
    /// „Gestoppt (endgültig)“ (CRAWL-128): wie Pause, Fortsetzen nicht möglich.
    Stopped,
    /// Geordnetes Beenden bzw. Schließen der Crawl-Datei: laufende Requests werden abgebrochen.
    Aborted,
}

/// Steuerung eines laufenden KI-Laufs (Pause, Stopp, Abbruch); klonbar, aus jedem Task bedienbar.
#[derive(Debug, Clone)]
pub struct RunControl(Arc<watch::Sender<ControlState>>);

impl Default for RunControl {
    fn default() -> Self {
        Self::new()
    }
}

impl RunControl {
    pub fn new() -> Self {
        Self(Arc::new(watch::Sender::new(ControlState::Running)))
    }
    pub fn state(&self) -> ControlState {
        *self.0.borrow()
    }
    pub fn pause(&self) {
        self.set(ControlState::Paused);
    }
    pub fn stop(&self) {
        self.set(ControlState::Stopped);
    }
    pub fn abort(&self) {
        self.set(ControlState::Aborted);
    }
    fn set(&self, state: ControlState) {
        // Ein Abbruch bleibt Abbruch; Stopp überholt Pause.
        self.0.send_modify(|s| {
            if *s != ControlState::Aborted
                && !(*s == ControlState::Stopped && state == ControlState::Paused)
            {
                *s = state;
            }
        });
    }
    fn subscribe(&self) -> watch::Receiver<ControlState> {
        self.0.subscribe()
    }
}

/// Besitz eines KI-Laufs für dessen Dauer (AI-020, Nachtrag 26.11.2026; CRAWL-125/AGENT-020). Die echte Umsetzung
/// (`owner_kind`, `owner_pid`, Heartbeat alle 2 s, Ende mit leerem `owner_pid`; passiv, wenn der eigene Prozess den Crawl
/// führt) liefert `os_ai::run_owner::RunOwner` aus M4-K11; die Anbindung ist eine Zeile ([`OwnerStart`]).
pub trait OwnerLease {
    /// Ende des KI-Laufs (bei jedem Ende, auch Pause, Stopp und Abbruch).
    fn finish(self: Box<Self>) -> Result<(), StoreError>;
}

/// Beginn des Besitzes vor dem ersten Request; ein Fehler (z. B. fremder aktiver Besitzer mit Art und PID) wird
/// unverändert als Ergebnis von [`DecisionRun::run`] zurückgegeben – der Lauf sendet und schreibt nichts. `RunOwner`
/// (M4-K11) öffnet eine eigene Verbindung; die Anbindung ruft `start`/`finish` daher über
/// `tokio::task::block_in_place` bzw. `spawn_blocking` auf.
pub type OwnerStart<'a> = Box<dyn FnOnce() -> Result<Box<dyn OwnerLease>, StoreError> + 'a>;

/// Test-Hook der Schreibwarteschlange: läuft unmittelbar vor dem Commit einer Antwort (AI-020, „Atomarität“).
pub type BeforeCommit<'a> = &'a dyn Fn() -> Result<(), StoreError>;

// ---------------------------------------------------------------------------------------------------------------
// Ergebnis

/// Wie ein Lauf endete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEnd {
    /// Warteschlange leer.
    Completed,
    Paused,
    Stopped,
    Aborted,
    /// Kostenvoranschlag über der Schwelle: Einträge `Pending (confirmation)`, nichts gesendet.
    NeedsConfirmation,
}

/// Voranschlag eines Laufs (AI-012, Abschnitt „KI“), je (Aufgabe, Modell).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunEstimate {
    pub tasks: Vec<RequestTaskEstimate>,
    pub totals: EstimateTotals,
}

/// Bericht eines Laufs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub end: RunEnd,
    /// Geschriebene Protokollzeilen mit gesendetem Request (ohne Cache-Treffer).
    pub requests_logged: u64,
    /// Voranschlag (bei [`Confirmation::AskAbove`]).
    pub estimate: Option<RunEstimate>,
    /// Fehler beim Schreiben einer Antwort (Eintrag bleibt `Pending`).
    pub commit_errors: Vec<String>,
}

// ---------------------------------------------------------------------------------------------------------------
// Lauf

/// Ein KI-Lauf über die aktiven Decision-Fragen.
pub struct DecisionRun<'a> {
    /// Schreibende Verbindung der Crawl-Datei.
    pub conn: &'a Connection,
    /// Zeile in `ai_runs` ([`os_store::ai::answers::begin_run`]).
    pub run_id: i64,
    pub questions: &'a [ActiveQuestion],
    pub items: &'a [RunItem],
    pub routing: RoutingContext<'a>,
    pub adapters: Adapters<'a>,
    pub limiter: &'a Limiter,
    /// Gemeinsamer Ledger, geöffnet über `os_store::ai::budget::open_ledger`.
    pub ledger: &'a dyn BudgetLedger,
    pub consent: &'a ConsentGate,
    pub switch: PreviewSwitch,
    pub redactor: &'a Redactor,
    pub prices: &'a dyn PriceLookup,
    pub control: RunControl,
    pub options: RunOptions,
    /// Zeitstempel der Protokoll- und Cachezeilen.
    pub now: &'a dyn Fn() -> Timestamp,
    /// Besitz für die Dauer des Laufs; `None` = nichts führen (z. B. Lauf während des eigenen Crawls).
    pub owner: Option<OwnerStart<'a>>,
    pub before_commit: Option<BeforeCommit<'a>>,
}

/// Ein Eintrag (Element × Frage) mit eingesetzten Parametern.
#[derive(Debug, Clone)]
struct Entry {
    item: usize,
    /// Instanz (bei Vorlagen mit eingesetzten Parametern), sonst die Frage selbst.
    instance: QuestionDef,
    params_hash: String,
    params_json: String,
}

/// Ein geplanter Request: ein Element, ein Weg, eine gesendete Eingabe, 1…n Fragen.
#[derive(Debug, Clone)]
struct Bundle {
    item: usize,
    question_route: usize,
    entries: Vec<(Entry, CacheKey)>,
    prepared: PreparedInput,
}

/// Zuordnung der Ergebnisse eines Requests.
enum Wire {
    Decisions {
        meta: Box<RequestMeta>,
        reply: Box<Result<DecisionsReply, FinalError>>,
    },
    Structured {
        /// Je gesendetem Request: Protokolldaten, Validator-Fehler, Abbruch an der Ausgabegrenze.
        requests: Vec<SentRequest>,
        outcome: StructuredOutcome<String>,
    },
}

struct SentRequest {
    meta: RequestMeta,
    reported_cost: Option<ReportedCost>,
    validator_error: Option<String>,
    final_status: Option<AiStatus>,
}

struct Done {
    bundle: Bundle,
    reservation: Option<(ReservationId, MicroUsd)>,
    guarded: policy::GuardedOutcome,
    wire: Option<Wire>,
    estimated_input_tokens: u64,
    estimated_output_tokens: Option<u64>,
}

type InFlight<'f> = Pin<Box<dyn Future<Output = Done> + 'f>>;

/// Wartet auf das nächste fertige Element (einfacher Ersatz für `FuturesUnordered`, ohne neue Abhängigkeit).
async fn next_done(set: &mut Vec<InFlight<'_>>) -> Done {
    std::future::poll_fn(|cx| {
        for i in 0..set.len() {
            if let Poll::Ready(done) = set[i].as_mut().poll(cx) {
                drop(set.swap_remove(i));
                return Poll::Ready(done);
            }
        }
        Poll::Pending
    })
    .await
}

fn is_failed_status(status: &str) -> bool {
    status.starts_with("Error: ") || status == "Quota exceeded" || status == "Budget exceeded"
}

fn is_open_status(status: &str) -> bool {
    matches!(
        status,
        "Pending" | "Pending (confirmation)" | "Pending (consent)"
    )
}

/// Basis der Preise eines Modells (Tabelle bzw. OpenRouter-Modellliste).
enum Basis {
    Rates(Rates),
    OpenRouter(Box<OpenRouterModel>),
}

impl Basis {
    fn as_basis(&self) -> PriceBasis<'_> {
        match self {
            Self::Rates(r) => PriceBasis::Rates(r),
            Self::OpenRouter(m) => PriceBasis::OpenRouter(m),
        }
    }
}

struct Plan {
    routes: Vec<Result<Route, AiStatus>>,
}

impl<'a> DecisionRun<'a> {
    fn now_db(&self) -> String {
        (self.now)().to_db()
    }

    fn basis(&self, route: &Route) -> Option<Basis> {
        let m = &route.plan.model;
        if m.provider == crate::adapters::openrouter::PROVIDER_ID {
            if let Some(model) = self.adapters.openrouter.and_then(|a| a.cached_model(&m.id)) {
                return Some(Basis::OpenRouter(Box::new(model)));
            }
        }
        self.prices
            .rates(&m.provider, &m.id, &m.endpoint)
            .map(Basis::Rates)
    }

    fn endpoint_base(&self, provider: &str) -> String {
        match provider {
            "openai" => self
                .adapters
                .decisions
                .map(|d| d.http().config().base_url.clone())
                .or_else(|| {
                    self.adapters
                        .responses
                        .map(|r| r.http().config().base_url.clone())
                })
                .unwrap_or_default(),
            "anthropic" => self
                .adapters
                .anthropic
                .map(|a| a.config().base_url.clone())
                .unwrap_or_default(),
            "openrouter" => self
                .adapters
                .openrouter
                .map(|a| a.config().base_url.clone())
                .unwrap_or_default(),
            "ollama" => self
                .adapters
                .ollama
                .map(|a| a.config().base_url.clone())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Voranschlag über die Einträge, die dieser Lauf senden würde (AI-012, Nachtrag Q00: Decisions je Frage, Ersatzweg
    /// mit Stufe und Antwortanteil nach W). Schreibt Statuszeilen wie ein Lauf, sendet nichts.
    pub fn estimate(&self) -> Result<RunEstimate, StoreError> {
        let plan = self.plan();
        let mut bundles = self.build_wave(&plan, false)?;
        bundles.extend(self.build_wave(&plan, true)?);
        Ok(self.estimate_bundles(&plan, &bundles))
    }

    fn plan(&self) -> Plan {
        Plan {
            routes: self
                .questions
                .iter()
                .map(|q| routing::route(&q.def, &q.routing, &self.routing))
                .collect(),
        }
    }

    /// Führt den Lauf aus.
    pub async fn run(mut self) -> Result<RunReport, StoreError> {
        let mut report = RunReport {
            end: RunEnd::Completed,
            requests_logged: 0,
            estimate: None,
            commit_errors: Vec::new(),
        };
        if let Err(errors) = format::validate_active(
            &self
                .questions
                .iter()
                .map(|q| q.def.clone())
                .collect::<Vec<_>>(),
        ) {
            return Err(StoreError::InvalidInput(
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        let lease = match self.owner.take() {
            // Fehler (z. B. fremder aktiver Besitzer) unverändert durchreichen: nichts gesendet, nichts geschrieben.
            Some(start) => Some(start()?),
            None => None,
        };
        self.reset_run_latches();
        let result = self.run_owned(&mut report).await;
        // `finish` in jedem Ende-Pfad: fertig, Pause, Stopp, Budget, Bestätigung, Abbruch und Fehler des Laufs.
        let finished = lease.map(|l| l.finish()).transpose();
        result?;
        finished?;
        Ok(report)
    }

    async fn run_owned(&self, report: &mut RunReport) -> Result<(), StoreError> {
        for q in self.questions {
            library::record_used(self.conn, &q.def)?;
        }
        let plan = self.plan();
        let wave1 = self.build_wave(&plan, false)?;
        if let Confirmation::AskAbove(threshold) = self.options.confirmation {
            let mut all = wave1.clone();
            all.extend(self.build_wave(&plan, true)?);
            let estimate = self.estimate_bundles(&plan, &all);
            let remaining = self.ledger.snapshot().ok().and_then(|s| s.remaining);
            // AI-012, Nachtrag 26.11.2026: „Bestätigung verlangen ab“ gilt einschließlich (≥ Schwelle); die Regel
            // „über 50 % des Restbudgets“ bleibt echt größer (`cost::confirmation_required`, Kostenkern K05).
            let needs = !all.is_empty() && estimate.totals.expected >= threshold
                || confirmation_required(estimate.totals.expected, threshold, remaining);
            report.estimate = Some(estimate);
            if needs {
                let rows: Vec<AnswerRow> = all
                    .iter()
                    .flat_map(|b| b.entries.iter())
                    .map(|(e, _)| self.status_row(e, &AiStatus::PendingConfirmation))
                    .collect();
                answers::upsert_all(self.conn, &rows)?;
                report.end = RunEnd::NeedsConfirmation;
                return Ok(());
            }
        }
        let end = self.dispatch(&plan, wave1, report).await?;
        if end != RunEnd::Completed {
            report.end = end;
            return Ok(());
        }
        // Zweite Welle: Fragen mit Eingabefeld `page_type` nach den Antworten der Seitentyp-Frage (AI-032).
        let wave2 = self.build_wave(&plan, true)?;
        report.end = self.dispatch(&plan, wave2, report).await?;
        Ok(())
    }

    fn status_row(&self, e: &Entry, status: &AiStatus) -> AnswerRow {
        AnswerRow::status_only(
            self.items[e.item].url_id,
            &e.instance.id,
            &e.instance.version.to_string(),
            &e.params_hash,
            &e.params_json,
            &status.to_string(),
        )
    }

    /// Plant eine Welle: schreibt `Skipped …`/Routing-/Eingabestatus und `Pending` für zu sendende Einträge und liefert
    /// die gebündelten Requests.
    fn build_wave(&self, plan: &Plan, page_type_wave: bool) -> Result<Vec<Bundle>, StoreError> {
        let mut rows = Vec::new();
        let mut bundles: Vec<Bundle> = Vec::new();
        let mut index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (qi, q) in self.questions.iter().enumerate() {
            let uses_page_type = q
                .def
                .inputs
                .iter()
                .any(|i| i.field == policy::InputField::PageType);
            if uses_page_type != page_type_wave {
                continue;
            }
            let scope = q.def.scope();
            let image_question = q.def.subject == format::Subject::Image;
            for (ii, item) in self.items.iter().enumerate() {
                if item.is_image != image_question {
                    continue;
                }
                let (instance, params_hash, params_json) = if q.def.is_template() {
                    let Some(values) = item.params.get(&q.def.id) else {
                        continue;
                    };
                    let json = format::params_json(values);
                    match format::instantiate(&q.def, values) {
                        Ok(inst) => (
                            inst,
                            crate::cache::params_hash(&json),
                            crate::cache::canonical_json(&json),
                        ),
                        Err(_) => continue,
                    }
                } else {
                    (q.def.clone(), String::new(), "{}".to_string())
                };
                let entry = Entry {
                    item: ii,
                    instance,
                    params_hash,
                    params_json,
                };
                // Geltungsbereich (08 Konventionen).
                let in_type = scope.content_types.contains(&item.content_class)
                    && item
                        .status_code
                        .is_some_and(|c| scope.status_codes.contains(&c))
                    && (!scope.indexable_only || item.indexable);
                let in_segment = scope
                    .segment
                    .as_ref()
                    .is_none_or(|s| item.segments.contains(s));
                if !in_type || !in_segment {
                    let status = if in_type {
                        AiStatus::SkippedSegment
                    } else {
                        AiStatus::SkippedScope
                    };
                    rows.push(self.status_row(&entry, &status));
                    continue;
                }
                // Auswahl nach vorhandenem Status.
                let existing = answers::get(
                    self.conn,
                    item.url_id,
                    &entry.instance.id,
                    &entry.instance.version.to_string(),
                    &entry.params_hash,
                )?;
                let selected = match (&existing, self.options.selection) {
                    (None, Selection::Open | Selection::All) => true,
                    (None, Selection::Failed) => false,
                    (Some(row), Selection::Open) => is_open_status(&row.status),
                    (Some(row), Selection::Failed) => is_failed_status(&row.status),
                    (Some(row), Selection::All) => row.status != "Pending (batch)",
                };
                if !selected {
                    continue;
                }
                let route = match &plan.routes[qi] {
                    Ok(route) => route,
                    Err(status) => {
                        rows.push(self.status_row(&entry, status));
                        continue;
                    }
                };
                // Eingabe (AI-032).
                let page_type = if page_type_wave {
                    self.page_type_of(item.url_id)?
                } else {
                    None
                };
                let max_input = self
                    .routing
                    .settings
                    .get(&route.task)
                    .and_then(|s| s.max_input_chars);
                let values = match input::build_values(
                    &entry.instance,
                    &item.data,
                    page_type.as_deref(),
                    max_input,
                    self.options.resizer.as_deref(),
                ) {
                    Ok(v) => v,
                    Err(status) => {
                        rows.push(self.status_row(&entry, &status));
                        continue;
                    }
                };
                let prepared =
                    PreparedInput::new(&values, &entry.instance.field_set(), self.redactor);
                let hash = input_hash(&prepared.inputs());
                let key = self.cache_key(&entry, route, &hash);
                rows.push(self.status_row(&entry, &AiStatus::Pending));
                let bundle_key = input::bundle_key(
                    &hash,
                    route.provider(),
                    route.model_id(),
                    &route.plan.model.endpoint,
                    route.path().as_str(),
                    route.plan.reasoning.effective.as_str(),
                ) + &format!("\u{1f}{ii}");
                let limit = match route.path() {
                    ExecutionPath::DecisionsApi => self.options.max_questions.get(),
                    ExecutionPath::StructuredOutput if self.options.bundle_fallback => {
                        MAX_FALLBACK_BUNDLE
                    }
                    ExecutionPath::StructuredOutput => 1,
                };
                let slots = index.entry(bundle_key).or_default();
                match slots
                    .last()
                    .copied()
                    .filter(|&b| bundles[b].entries.len() < limit)
                {
                    Some(b) => bundles[b].entries.push((entry, key)),
                    None => {
                        slots.push(bundles.len());
                        bundles.push(Bundle {
                            item: ii,
                            question_route: qi,
                            entries: vec![(entry, key)],
                            prepared,
                        });
                    }
                }
            }
        }
        answers::upsert_all(self.conn, &rows)?;
        Ok(bundles)
    }

    /// Ergebnis der Seitentyp-Frage einer URL (`choice`), falls vorhanden.
    fn page_type_of(&self, url_id: i64) -> Result<Option<String>, StoreError> {
        let Some(q) = self
            .questions
            .iter()
            .find(|q| q.def.id == format::PAGE_TYPE_QUESTION_ID)
        else {
            return Ok(None);
        };
        let row = answers::get(self.conn, url_id, &q.def.id, &q.def.version.to_string(), "")?;
        Ok(row
            .filter(|r| r.status.starts_with("Success"))
            .and_then(|r| r.answer_json)
            .and_then(|j| serde_json::from_str::<Value>(&j).ok())
            .and_then(|v| v["choice"].as_str().map(str::to_string)))
    }

    fn cache_key(&self, entry: &Entry, route: &Route, input_hash: &str) -> CacheKey {
        let m = &route.plan.model;
        let params: Value = serde_json::from_str(&entry.params_json).unwrap_or_else(|_| json!({}));
        CacheKey::with_reasoning(
            CacheKeyInput {
                task: route.task.as_str().to_string(),
                definition_id: entry.instance.id.clone(),
                definition_version: entry.instance.version.to_string(),
                params,
                path: Some(route.path()),
                provider: m.provider.clone(),
                model: m.id.clone(),
                endpoint: self.endpoint_base(&m.provider),
                result_params: json!({}),
                input_hash: input_hash.to_string(),
            },
            route.plan.reasoning.effective,
        )
    }

    fn route_of(&self, plan: &'_ Plan, bundle: &Bundle) -> Route {
        plan.routes[bundle.question_route]
            .clone()
            .expect("bundles only exist for routed questions")
    }

    fn estimate_bundles(&self, plan: &Plan, bundles: &[Bundle]) -> RunEstimate {
        let cache = ResultCache::new(self.conn, self.options.cache);
        let cached: HashSet<String> = bundles
            .iter()
            .flat_map(|b| b.entries.iter())
            .filter(|(_, key)| matches!(cache.lookup(key), Ok(Lookup::Hit { .. })))
            .map(|(_, key)| key.as_str().to_string())
            .collect();
        let mut groups: BTreeMap<(String, String, String), (Route, Vec<PlannedRequest>)> =
            BTreeMap::new();
        for b in bundles {
            let route = self.route_of(plan, b);
            let input_chars = cost::tokens::char_count(&b.prepared.values.text_block());
            let images = b.prepared.values.images().len() as u64;
            let entries = b
                .entries
                .iter()
                .map(|(e, key)| {
                    let wire = e.instance.wire_json();
                    PlannedEntry {
                        cache_key: Some(key.as_str().to_string()),
                        question_chars: cost::question_text_chars(&wire),
                        weight: crate::catalog::reasoning::question_weight(&wire),
                    }
                })
                .collect();
            groups
                .entry((
                    route.task.as_str().to_string(),
                    route.provider().to_string(),
                    route.model_id().to_string(),
                ))
                .or_insert_with(|| (route.clone(), Vec::new()))
                .1
                .push(PlannedRequest {
                    input_chars,
                    images,
                    entries,
                });
        }
        let limits = ProviderLimitsFile::bundled().ok();
        let mut tasks = Vec::new();
        for ((task, _, model), (route, requests)) in groups {
            let basis = self.basis(&route);
            let path = match route.path() {
                ExecutionPath::DecisionsApi => EstimatePath::DecisionsApi {
                    question_overhead_tokens: cost::decisions::bundled_question_overhead(),
                },
                ExecutionPath::StructuredOutput => EstimatePath::Prompt {
                    instruction_tokens: cost::tokens::text_tokens(fallback_instruction_chars()),
                    schema_overhead_tokens: limits.as_ref().map_or(0, |l| {
                        Endpoint::parse(&route.plan.model.endpoint).map_or(0, |e| {
                            l.schema_overhead(route.provider(), e, route.model_id())
                        })
                    }),
                    answer: route.plan.answer_tokens.unwrap_or(AnswerTokens::Formula(
                        crate::catalog::reasoning::AnswerFormula::DecisionW,
                    )),
                    reasoning: route.plan.reasoning.effective,
                },
            };
            let input = RequestEstimateInput {
                task,
                model,
                requests: &requests,
                path,
                basis: basis.as_ref().map(Basis::as_basis),
                api_key_present: true,
                calibration: Calibration::UNCALIBRATED,
                output_calibration: Calibration::UNCALIBRATED,
                batch_capable: false,
            };
            tasks.push(estimate_requests(&input, &cached));
        }
        let totals =
            EstimateTotals::of_tasks(&tasks.iter().map(|t| t.estimate.clone()).collect::<Vec<_>>());
        RunEstimate { tasks, totals }
    }

    /// Sendet die Bündel einer Welle unter der Steuerung von [`RunControl`].
    async fn dispatch(
        &self,
        plan: &Plan,
        bundles: Vec<Bundle>,
        report: &mut RunReport,
    ) -> Result<RunEnd, StoreError> {
        let mut queue: VecDeque<Bundle> = bundles.into();
        let mut inflight: Vec<InFlight<'_>> = Vec::new();
        let reservations: RefCell<Vec<ReservationId>> = RefCell::new(Vec::new());
        let mut control = self.control.subscribe();
        loop {
            let state = *control.borrow_and_update();
            if state == ControlState::Aborted {
                // Laufende Requests abbrechen (Verbindung schließen), nicht übernehmen; Reservierungen verfallen.
                drop(std::mem::take(&mut inflight));
                for id in reservations.borrow_mut().drain(..) {
                    let _ = self.ledger.release(id);
                }
                return Ok(RunEnd::Aborted);
            }
            if state == ControlState::Running {
                while inflight.len() < self.options.concurrency.max(1) {
                    let Some(bundle) = queue.pop_front() else {
                        break;
                    };
                    if let Some(fut) = self.prepare(plan, bundle, &mut queue)? {
                        if let Some((id, _)) = fut.reservation {
                            reservations.borrow_mut().push(id);
                        }
                        inflight.push(fut.future);
                    }
                }
            }
            if inflight.is_empty() {
                return Ok(match state {
                    ControlState::Running if queue.is_empty() => RunEnd::Completed,
                    ControlState::Running => continue,
                    ControlState::Paused => RunEnd::Paused,
                    ControlState::Stopped => RunEnd::Stopped,
                    ControlState::Aborted => RunEnd::Aborted,
                });
            }
            tokio::select! {
                done = next_done(&mut inflight) => {
                    if let Some((id, _)) = done.reservation {
                        reservations.borrow_mut().retain(|r| *r != id);
                    }
                    self.finish(plan, done, report)?;
                }
                _ = control.changed() => {}
            }
        }
    }
}

/// Zeichen der festen Systemanweisung des Ersatzwegs (ohne Fragentexte) für die Schätzung.
fn fallback_instruction_chars() -> u64 {
    cost::tokens::char_count(&fallback::instructions(&[]))
}

struct Prepared<'f> {
    future: InFlight<'f>,
    reservation: Option<(ReservationId, MicroUsd)>,
}

/// Was ein Request-Future braucht (nur Referenzen mit der Lebensdauer des Laufs).
#[derive(Clone, Copy)]
struct SendCtx<'a> {
    adapters: Adapters<'a>,
    limiter: &'a Limiter,
    consent: &'a ConsentGate,
    switch: PreviewSwitch,
    repairs: RepairAttempts,
}

impl<'a> DecisionRun<'a> {
    fn send_ctx(&self) -> SendCtx<'a> {
        SendCtx {
            adapters: self.adapters,
            limiter: self.limiter,
            consent: self.consent,
            switch: self.switch,
            repairs: self.options.repairs,
        }
    }

    fn commit(
        &self,
        requests: &[(RequestRecord, RequestDetails)],
        rows: &[AnswerRow],
        cache: &[CacheRow],
    ) -> Result<Option<i64>, StoreError> {
        let noop = || Ok(());
        let hook: &dyn Fn() -> Result<(), StoreError> = match self.before_commit {
            Some(h) => h,
            None => &noop,
        };
        answers::commit_response(
            self.conn,
            &ResponseCommit {
                requests,
                answers: rows,
                cache,
            },
            hook,
        )
    }

    fn log_context(&self, bundle: &Bundle, route: &Route, redactions: u32) -> LogContext {
        let versions: BTreeSet<String> = bundle
            .entries
            .iter()
            .map(|(e, _)| e.instance.version.to_string())
            .collect();
        LogContext {
            run_id: self.run_id,
            timestamp: self.now_db(),
            task: route.task.as_str().to_string(),
            provider: route.provider().to_string(),
            model: route.model_id().to_string(),
            endpoint: route.plan.model.endpoint.clone(),
            url_id: Some(self.items[bundle.item].url_id),
            definition_version: Some(versions.into_iter().collect::<Vec<_>>().join(",")),
            batch_id: None,
            redactions,
        }
    }

    /// Status für alle Einträge eines Bündels ohne Request schreiben.
    fn write_status(
        &self,
        entries: &[(Entry, CacheKey)],
        status: &AiStatus,
    ) -> Result<(), StoreError> {
        let rows: Vec<AnswerRow> = entries
            .iter()
            .map(|(e, _)| self.status_row(e, status))
            .collect();
        answers::upsert_all(self.conn, &rows)
    }

    /// Kontingent- und Erreichbarkeitssperren gelten je KI-Lauf (AI-020, Entscheidung 27.11.2026): Die Sperren liegen
    /// in den Adapter-Instanzen (K01–K03); ein neuer Lauf mit wiederverwendeten Adaptern beginnt ohne alte Sperre.
    /// Laufen zwei Läufe gleichzeitig mit denselben Adaptern, teilen sie sich die Sperre (Auslegung, Bericht d01).
    fn reset_run_latches(&self) {
        if let Some(a) = self.adapters.anthropic {
            a.quota().reset();
        }
        if let Some(a) = self.adapters.openrouter {
            a.quota().reset();
        }
        if let Some(a) = self.adapters.ollama {
            a.unreachable().reset();
        }
        if let Some(d) = self.adapters.decisions {
            d.http().account().clear_quota_stop();
        }
        if let Some(r) = self.adapters.responses {
            r.http().account().clear_quota_stop();
        }
    }

    fn quota_blocked(&self, provider: &str) -> bool {
        match provider {
            "anthropic" => self
                .adapters
                .anthropic
                .is_some_and(|a| a.quota().is_tripped()),
            "openrouter" => self
                .adapters
                .openrouter
                .is_some_and(|a| a.quota().is_tripped()),
            "openai" => self
                .adapters
                .decisions
                .map(|d| d.http().account().quota_stop().is_some())
                .or_else(|| {
                    self.adapters
                        .responses
                        .map(|r| r.http().account().quota_stop().is_some())
                })
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Vor dem Senden: Cache, Kontingentsperre, Reservierung. `None` = nichts zu senden.
    fn prepare(
        &self,
        plan: &Plan,
        mut bundle: Bundle,
        queue: &mut VecDeque<Bundle>,
    ) -> Result<Option<Prepared<'a>>, StoreError> {
        let route = self.route_of(plan, &bundle);
        let cache = ResultCache::new(self.conn, self.options.cache);
        let mut remaining = Vec::new();
        for (entry, key) in std::mem::take(&mut bundle.entries) {
            match cache.lookup(&key)? {
                Lookup::Hit { status, mut result } => {
                    let ctx = self.log_context(
                        &Bundle {
                            entries: vec![(entry.clone(), key.clone())],
                            ..bundle.clone()
                        },
                        &route,
                        0,
                    );
                    let record = cache_hit_record(&ctx, PriceSource::Table, 0);
                    let mut row = self.status_row(&entry, &status);
                    row.source = Some(route.source().to_string());
                    row.model = Some(route.model_id().to_string());
                    row.raw_confidence = result
                        .as_object_mut()
                        .and_then(|m| m.remove("raw_confidence"))
                        .and_then(|v| v.as_f64());
                    row.answer_json =
                        (!result.is_null()).then(|| crate::cache::canonical_json(&result));
                    self.commit(&[(record, RequestDetails::default())], &[row], &[])?;
                }
                Lookup::Miss => remaining.push((entry, key)),
            }
        }
        bundle.entries = remaining;
        if bundle.entries.is_empty() {
            return Ok(None);
        }
        if self.quota_blocked(route.provider()) {
            self.write_status(&bundle.entries, &AiStatus::QuotaExceeded)?;
            return Ok(None);
        }
        // Request zusammenstellen.
        let defs: Vec<&QuestionDef> = bundle.entries.iter().map(|(e, _)| &e.instance).collect();
        let decisions = route.path() == ExecutionPath::DecisionsApi;
        let wires: Vec<Value> = defs.iter().map(|d| d.wire_json()).collect();
        let weight = crate::catalog::reasoning::request_weight(&wires);
        let images = bundle.prepared.values.images().len() as u64;
        let text_chars = cost::tokens::char_count(&bundle.prepared.values.text_block());
        let basis = self.basis(&route);
        let image_estimate = basis
            .as_ref()
            .and_then(|b| b.as_basis().estimate_rates(0))
            .map_or(0, |r| r.image_token_estimate);
        let mut request = Request {
            model: route.plan.model.clone(),
            required: route.task.required_capabilities(
                Endpoint::parse(&route.plan.model.endpoint).unwrap_or(Endpoint::Messages),
            ),
            input: bundle.prepared.inputs(),
            instructions: String::new(),
            questions: wires.clone(),
            response_schema: None,
            max_tokens: None,
            temperature: None,
            effort: None,
            reasoning: Default::default(),
        };
        let (estimated_input_tokens, estimated_output_tokens) = if decisions {
            let chars: Vec<u64> = wires.iter().map(cost::question_text_chars).collect();
            (
                cost::decisions_input_tokens(
                    text_chars,
                    images,
                    image_estimate,
                    &chars,
                    cost::decisions::bundled_question_overhead(),
                ),
                None,
            )
        } else {
            route.plan.apply_to(&mut request, Some(weight));
            request.instructions = fallback::instructions(&defs);
            request.response_schema = Some(fallback::bundle_schema(&defs));
            let overhead = ProviderLimitsFile::bundled().ok().map_or(0, |l| {
                Endpoint::parse(&route.plan.model.endpoint).map_or(0, |e| {
                    l.schema_overhead(route.provider(), e, route.model_id())
                })
            });
            let answer = route
                .plan
                .answer_tokens
                .map_or(crate::catalog::decision_answer_tokens(weight), |a| {
                    a.tokens(Some(weight))
                });
            (
                cost::tokens::estimate_input_tokens(
                    text_chars,
                    images,
                    image_estimate,
                    cost::tokens::text_tokens(cost::tokens::char_count(&request.instructions))
                        .saturating_add(overhead),
                ),
                Some(cost::raw_output_tokens(
                    answer,
                    route.plan.reasoning.effective,
                )),
            )
        };
        // Reservierung unmittelbar vor dem Senden (AI-013).
        let Some(basis) = basis else {
            // Preis unbekannt: nicht ausführbar (AI-012 „Price unknown“; Status nach Auslegung, Bericht d01).
            self.write_status(&bundle.entries, &AiStatus::SkippedNoCapableModel)?;
            return Ok(None);
        };
        let Some(amount) = cost::reservation_for(
            basis.as_basis(),
            estimated_input_tokens,
            request.max_tokens.unwrap_or(0),
            images,
            false,
        ) else {
            self.write_status(&bundle.entries, &AiStatus::SkippedNoCapableModel)?;
            return Ok(None);
        };
        let reservation = match self
            .ledger
            .reserve(BudgetScope::task(AREA_AI, route.task.as_str()), amount)
        {
            Ok(r) => (r.id, amount),
            Err(error) => {
                let status = cost::reserve::status_for(&error).unwrap_or(AiStatus::BudgetExceeded);
                self.write_status(&bundle.entries, &status)?;
                if cost::reserve::stops_all_ai_entries(&error) {
                    // Crawl-Budget, „KI gesamt“ oder Sperre: alle offenen KI-Einträge.
                    for b in queue.drain(..) {
                        self.write_status(&b.entries, &status)?;
                    }
                } else if let Some(task) = cost::reserve::exhausted_task(&error) {
                    let task = task.to_string();
                    let (same, other): (Vec<Bundle>, Vec<Bundle>) = queue
                        .drain(..)
                        .partition(|b| self.route_of(plan, b).task.as_str() == task);
                    for b in same {
                        self.write_status(&b.entries, &status)?;
                    }
                    queue.extend(other);
                }
                return Ok(None);
            }
        };
        let ctx = self.send_ctx();
        let task = route.task.as_str().to_string();
        let schema = request.response_schema.clone();
        let future: InFlight<'a> = Box::pin(async move {
            let wire = WireProvider {
                ctx,
                provider: request.model.provider.clone(),
                defs: bundle
                    .entries
                    .iter()
                    .map(|(e, _)| e.instance.clone())
                    .collect(),
                estimated_input_tokens,
                schema,
                slot: Mutex::new(None),
            };
            let guarded = policy::guard::execute(
                ctx.switch,
                ctx.consent,
                &task,
                &bundle.prepared,
                &wire,
                request,
            )
            .await;
            let wire = wire.slot.into_inner().unwrap_or_else(|p| p.into_inner());
            Done {
                bundle,
                reservation: Some(reservation),
                guarded,
                wire,
                estimated_input_tokens,
                estimated_output_tokens,
            }
        });
        Ok(Some(Prepared {
            future,
            reservation: Some(reservation),
        }))
    }

    /// Verarbeitet eine Antwort atomar und rechnet die Reservierung ab.
    fn finish(&self, plan: &Plan, done: Done, report: &mut RunReport) -> Result<(), StoreError> {
        let route = self.route_of(plan, &done.bundle);
        let release = |r: Option<(ReservationId, MicroUsd)>| {
            if let Some((id, _)) = r {
                let _ = self.ledger.release(id);
            }
        };
        let wire = match (&done.guarded.outcome, done.wire) {
            (Outcome::Completed(_), Some(wire)) => wire,
            (outcome, _) => {
                release(done.reservation);
                let status = match outcome {
                    Outcome::Disabled => AiStatus::Pending,
                    Outcome::Status(s) => s.clone(),
                    Outcome::ProviderFailure(e) => e.status().unwrap_or(AiStatus::ConnectionFailed),
                    Outcome::Completed(_) => AiStatus::InvalidOutput,
                };
                return self.write_status(&done.bundle.entries, &status);
            }
        };
        let basis = self.basis(&route);
        let ctx = self.log_context(&done.bundle, &route, done.guarded.redactions);
        let images = u32::try_from(done.bundle.prepared.values.images().len()).unwrap_or(u32::MAX);
        let reservation = done.reservation.map_or(MicroUsd(0), |r| r.1);
        let mut rows: Vec<(RequestRecord, RequestDetails)> = Vec::new();
        let mut answer_rows = Vec::new();
        let mut cache_rows = Vec::new();
        let created = (self.now)();
        let source = route.source().to_string();
        let model = route.model_id().to_string();
        let mut add_answer = |entry: &Entry,
                              key: &CacheKey,
                              status: AiStatus,
                              answer: Option<Value>,
                              raw_confidence: Option<f64>| {
            let mut row = self.status_row(entry, &status);
            row.source = Some(source.clone());
            row.model = Some(model.clone());
            row.raw_confidence = raw_confidence;
            row.answer_json = answer.as_ref().map(crate::cache::canonical_json);
            if matches!(status, AiStatus::Success | AiStatus::Refusal)
                && self.options.cache.use_cache
            {
                let input = key.input();
                let mut result = answer.clone().unwrap_or(Value::Null);
                if let (Some(c), Value::Object(map)) = (raw_confidence, &mut result) {
                    map.insert("raw_confidence".into(), json!(c));
                }
                cache_rows.push(CacheRow {
                    cache_key: key.as_str().to_string(),
                    provider: input.provider.clone(),
                    model: input.model.clone(),
                    endpoint: input.endpoint.clone(),
                    definition_id: input.definition_id.clone(),
                    definition_version: input.definition_version.clone(),
                    input_hash: input.input_hash.clone(),
                    params_hash: key.params_hash().to_string(),
                    result_json: crate::cache::canonical_json(&result),
                    status: if status == AiStatus::Success {
                        "Success"
                    } else {
                        "Refusal"
                    }
                    .into(),
                    created_at: created.to_db(),
                });
            }
            answer_rows.push(row);
        };
        match wire {
            Wire::Decisions { meta, reply } => {
                let status = match reply.as_ref() {
                    Ok(r) if r.readable => AiStatus::Success,
                    Ok(_) => AiStatus::InvalidOutput,
                    Err(e) => e.status.clone(),
                };
                let mut mismatch = false;
                match *reply {
                    Ok(r) => {
                        for ((entry, key), result) in done.bundle.entries.iter().zip(r.results) {
                            match result {
                                QuestionResult::Answered(mut a) => {
                                    mismatch |= a.confidence_mismatch();
                                    let normalized = fallback::normalize_decisions(&mut a);
                                    let raw = a.confidence_raw().or(a.confidence()).or(match &a {
                                        DecisionAnswer::Predicate { probability } => {
                                            Some(*probability)
                                        }
                                        _ => None,
                                    });
                                    add_answer(
                                        entry,
                                        key,
                                        AiStatus::Success,
                                        Some(answer_value(&a, normalized)),
                                        raw,
                                    );
                                }
                                QuestionResult::Refusal { reason } => add_answer(
                                    entry,
                                    key,
                                    AiStatus::Refusal,
                                    Some(json!({"refusal_reason": reason})),
                                    None,
                                ),
                                other => add_answer(entry, key, other.status(), None, None),
                            }
                        }
                    }
                    Err(e) => {
                        for (entry, key) in &done.bundle.entries {
                            add_answer(entry, key, e.status.clone(), None, None);
                        }
                    }
                }
                if meta.attempts > 0 {
                    let mut outcome =
                        meta.request_outcome(status, done.estimated_input_tokens, 0, images);
                    outcome.confidence_mismatch = mismatch;
                    let rates = basis
                        .as_ref()
                        .and_then(|b| b.as_basis().actual_rates(done.estimated_input_tokens))
                        .map(|r| decisions_billing_rates(&r))
                        .unwrap_or_else(crate::adapters::ollama::rates);
                    let row = detailed_request_record(
                        &ctx,
                        &rates,
                        reservation,
                        &outcome,
                        &OutcomeDetails::default(),
                    );
                    rows.push((row.record, row.details));
                }
            }
            Wire::Structured { requests, outcome } => {
                let defs: Vec<&QuestionDef> = done
                    .bundle
                    .entries
                    .iter()
                    .map(|(e, _)| &e.instance)
                    .collect();
                match &outcome {
                    StructuredOutcome::Valid(value) => {
                        for ((entry, key), result) in done
                            .bundle
                            .entries
                            .iter()
                            .zip(fallback::split_bundle(&defs, value))
                        {
                            match result {
                                FallbackResult::Answered { answer, normalized } => {
                                    let raw = answer.confidence().or(match &answer {
                                        DecisionAnswer::Predicate { probability } => {
                                            Some(*probability)
                                        }
                                        _ => None,
                                    });
                                    add_answer(
                                        entry,
                                        key,
                                        AiStatus::Success,
                                        Some(answer_value(&answer, normalized)),
                                        raw,
                                    )
                                }
                                FallbackResult::Abstained => add_answer(
                                    entry,
                                    key,
                                    AiStatus::Refusal,
                                    Some(json!({"refusal_reason": fallback::ABSTAIN_REASON})),
                                    None,
                                ),
                                FallbackResult::Invalid(_) => {
                                    add_answer(entry, key, AiStatus::InvalidOutput, None, None)
                                }
                            }
                        }
                    }
                    StructuredOutcome::Refusal => {
                        let reason = requests.last().and_then(|r| r.meta.refusal_reason.clone());
                        for (entry, key) in &done.bundle.entries {
                            add_answer(
                                entry,
                                key,
                                AiStatus::Refusal,
                                Some(json!({"refusal_reason": reason})),
                                None,
                            );
                        }
                    }
                    StructuredOutcome::InvalidOutput { .. } => {
                        for (entry, key) in &done.bundle.entries {
                            add_answer(entry, key, AiStatus::InvalidOutput, None, None);
                        }
                    }
                    StructuredOutcome::Failed(_) => {
                        let status = requests
                            .last()
                            .and_then(|r| r.final_status.clone())
                            .unwrap_or(AiStatus::ConnectionFailed);
                        for (entry, key) in &done.bundle.entries {
                            add_answer(entry, key, status.clone(), None, None);
                        }
                    }
                }
                let last = requests.len().saturating_sub(1);
                for (i, sent) in requests.iter().enumerate() {
                    if sent.meta.attempts == 0 {
                        continue;
                    }
                    let status = if let Some(s) = &sent.final_status {
                        s.clone()
                    } else if sent.validator_error.is_some() {
                        AiStatus::InvalidOutput
                    } else if i == last && matches!(outcome, StructuredOutcome::Refusal) {
                        AiStatus::Refusal
                    } else {
                        AiStatus::Success
                    };
                    let mut outcome = sent.meta.request_outcome(
                        status,
                        done.estimated_input_tokens,
                        done.estimated_output_tokens.unwrap_or(0),
                        images,
                    );
                    // AI-021: der Validator-Fehler steht im Protokoll (Spalte `Error Message`).
                    if let Some(error) = &sent.validator_error {
                        outcome.error_message = Some(format!("schema violation: {error}"));
                    }
                    let input_tokens = sent
                        .meta
                        .usage
                        .as_ref()
                        .and_then(|u| u.input_tokens)
                        .unwrap_or(done.estimated_input_tokens);
                    let rates = basis
                        .as_ref()
                        .and_then(|b| b.as_basis().actual_rates(input_tokens))
                        .unwrap_or_else(crate::adapters::ollama::rates);
                    let row = detailed_request_record(
                        &ctx,
                        &rates,
                        reservation,
                        &outcome,
                        &OutcomeDetails {
                            reasoning: Some(route.plan.reasoning.effective),
                            reported_cost: sent.reported_cost,
                            estimated_output_tokens: done.estimated_output_tokens,
                        },
                    );
                    rows.push((row.record, row.details));
                }
            }
        }
        let total = rows.iter().fold(MicroUsd(0), |acc, (r, _)| {
            acc.checked_add(r.cost).unwrap_or(acc)
        });
        let logged = rows.len() as u64;
        match self.commit(&rows, &answer_rows, &cache_rows) {
            Ok(_) => {
                report.requests_logged += logged;
                if let Some((id, _)) = done.reservation {
                    if logged == 0 {
                        let _ = self.ledger.release(id);
                    } else {
                        let _ = self.ledger.settle(id, total);
                    }
                }
            }
            Err(e) => {
                // Antwort nicht übernommen: Einträge bleiben `Pending`, Reservierung verfällt (AI-020).
                release(done.reservation);
                report.commit_errors.push(e.to_string());
            }
        }
        Ok(())
    }
}

/// `answer_json` eines Ergebnisses: die Antwort (gleiche Form auf beiden Wegen, AI-034) und bei Normierung
/// `"normalized": true` (AI-033).
fn answer_value(answer: &DecisionAnswer, normalized: bool) -> Value {
    let mut v = serde_json::to_value(answer).unwrap_or(Value::Null);
    if normalized {
        if let Value::Object(map) = &mut v {
            map.insert("normalized".into(), Value::Bool(true));
        }
    }
    v
}

// ---------------------------------------------------------------------------------------------------------------
// Adapterseite: ein `AiProvider`, den `policy::guard::execute` aufruft; er führt den Request mit allen Versuchen über
// den Limiter aus und legt die Protokolldaten in `slot` ab.

struct WireProvider<'a> {
    ctx: SendCtx<'a>,
    /// Anbieter des Requests (der Guard prüft ihn gegen `request.model.provider`).
    provider: String,
    defs: Vec<QuestionDef>,
    estimated_input_tokens: u64,
    schema: Option<Value>,
    slot: Mutex<Option<Wire>>,
}

fn decision_input(parts: &[Input]) -> Result<DecisionInput, AiStatus> {
    let mut out = Vec::new();
    for part in parts {
        match part {
            Input::Text { text } => out.push(InputPart::Text(text.clone())),
            Input::Image { media_type, base64 } => out.push(InputPart::Image(
                from_base64(media_type, base64).map_err(|e| e.status())?,
            )),
        }
    }
    Ok(DecisionInput::from_parts(out))
}

/// Fehlerform eines Schritts im Ersatzweg.
struct StructFail {
    sent: SentRequest,
}

fn breaker_effect(reply: &AdapterReply) -> BreakerEffect {
    if reply.is_refusal() || reply.is_output_limit() {
        BreakerEffect::Neutral
    } else {
        BreakerEffect::Reset
    }
}

impl WireProvider<'_> {
    async fn decisions(&self, d: &OpenAiDecisions, request: &Request) -> Wire {
        let limiter = self.ctx.limiter;
        let questions: Vec<Question> = self.defs.iter().map(QuestionDef::to_wire).collect();
        let input = match decision_input(&request.input) {
            Ok(input) => input,
            Err(status) => {
                return Wire::Decisions {
                    meta: Box::default(),
                    reply: Box::new(Err(FinalError {
                        error: ProviderError::InvalidOutput,
                        status,
                        http_status: None,
                        error_code: None,
                        message: String::new(),
                        provider_request_id: None,
                    })),
                }
            }
        };
        let spec = RequestSpec::new(
            d_provider(),
            request.model.id.clone(),
            self.estimated_input_tokens,
        );
        let exec = limiter
            .run_classified(
                &spec,
                |_| d.send(&request.model.id, &input, &questions, Some(limiter)),
                |reply: &DecisionsReply| {
                    if reply.readable {
                        BreakerEffect::Reset
                    } else {
                        BreakerEffect::Neutral
                    }
                },
            )
            .await;
        let meta = RequestMeta::from_execution(
            &exec,
            exec.result.as_ref().ok().map(|r| {
                (
                    r.http_status,
                    r.usage.clone(),
                    r.provider_request_id.clone(),
                )
            }),
        );
        Wire::Decisions {
            meta: Box::new(meta),
            reply: Box::new(exec.result),
        }
    }

    async fn structured(&self, request: &Request) -> Wire {
        let limiter = self.ctx.limiter;
        let Some(schema) = self
            .schema
            .clone()
            .and_then(|s| ResponseSchema::new(s).ok())
        else {
            return Wire::Structured {
                requests: Vec::new(),
                outcome: StructuredOutcome::InvalidOutput {
                    validator_error: "schema".into(),
                },
            };
        };
        let est = self.estimated_input_tokens;
        let a = self.ctx.adapters;
        let provider = request.model.provider.clone();
        if provider == crate::adapters::openai_responses::PROVIDER_ID {
            let Some(responses) = a.responses else {
                return missing_adapter();
            };
            let run = responses
                .run_structured(limiter, request, &schema, self.ctx.repairs, est)
                .await;
            let mut requests: Vec<SentRequest> = run
                .requests
                .into_iter()
                .map(|r| SentRequest {
                    meta: r.meta,
                    reported_cost: None,
                    validator_error: r.validator_error,
                    final_status: None,
                })
                .collect();
            let outcome = match run.outcome {
                StructuredOutcome::Failed(f) => {
                    requests.push(SentRequest {
                        meta: f.meta,
                        reported_cost: None,
                        validator_error: None,
                        final_status: Some(f.error.status.clone()),
                    });
                    StructuredOutcome::Failed(f.error.message)
                }
                StructuredOutcome::Valid(v) => StructuredOutcome::Valid(v),
                StructuredOutcome::Refusal => StructuredOutcome::Refusal,
                StructuredOutcome::InvalidOutput { validator_error } => {
                    StructuredOutcome::InvalidOutput { validator_error }
                }
            };
            return Wire::Structured { requests, outcome };
        }
        let run = run_with_repair(&schema, self.ctx.repairs, |repair| {
            let request = request.clone();
            let provider = provider.clone();
            async move {
                let spec = RequestSpec::new(provider.clone(), request.model.id.clone(), est);
                let exec = match provider.as_str() {
                    "anthropic" => match a.anthropic {
                        Some(adapter) => {
                            limiter
                                .run_classified(
                                    &spec,
                                    |_| adapter.send(&request, repair.as_ref()),
                                    breaker_effect,
                                )
                                .await
                        }
                        None => {
                            return Err(StructFail {
                                sent: missing_sent(),
                            })
                        }
                    },
                    "openrouter" => match a.openrouter {
                        Some(adapter) => {
                            limiter
                                .run_classified(
                                    &spec,
                                    |_| adapter.send(&request, repair.as_ref()),
                                    breaker_effect,
                                )
                                .await
                        }
                        None => {
                            return Err(StructFail {
                                sent: missing_sent(),
                            })
                        }
                    },
                    "ollama" => match a.ollama {
                        Some(adapter) => adapter.run(limiter, &request, repair.as_ref(), est).await,
                        None => {
                            return Err(StructFail {
                                sent: missing_sent(),
                            })
                        }
                    },
                    _ => {
                        return Err(StructFail {
                            sent: missing_sent(),
                        })
                    }
                };
                let mut meta = RequestMeta::from_execution(
                    &exec,
                    exec.result.as_ref().ok().map(|r| {
                        (
                            r.http_status,
                            Some(r.usage.clone()),
                            r.provider_request_id.clone(),
                        )
                    }),
                );
                match exec.result {
                    Ok(reply) => {
                        meta.refusal_reason = reply.refusal_reason.clone();
                        let reported_cost = reply.reported_cost;
                        match reply.for_repair() {
                            Ok((raw, _)) => Ok((
                                raw,
                                SentRequest {
                                    meta,
                                    reported_cost,
                                    validator_error: None,
                                    final_status: None,
                                },
                            )),
                            Err(limit) => Err(StructFail {
                                sent: output_limit_sent(meta, &limit, reported_cost),
                            }),
                        }
                    }
                    Err(error) => Err(StructFail {
                        sent: SentRequest {
                            meta,
                            reported_cost: None,
                            validator_error: None,
                            final_status: Some(error.status),
                        },
                    }),
                }
            }
        })
        .await;
        let mut requests: Vec<SentRequest> = run
            .requests
            .into_iter()
            .map(|r| SentRequest {
                validator_error: r.validator_error,
                ..r.meta
            })
            .collect();
        let outcome = match run.outcome {
            StructuredOutcome::Failed(f) => {
                requests.push(f.sent);
                StructuredOutcome::Failed(String::new())
            }
            StructuredOutcome::Valid(v) => StructuredOutcome::Valid(v),
            StructuredOutcome::Refusal => StructuredOutcome::Refusal,
            StructuredOutcome::InvalidOutput { validator_error } => {
                StructuredOutcome::InvalidOutput { validator_error }
            }
        };
        Wire::Structured { requests, outcome }
    }
}

fn d_provider() -> &'static str {
    crate::adapters::openai_responses::PROVIDER_ID
}

fn missing_sent() -> SentRequest {
    SentRequest {
        meta: RequestMeta::default(),
        reported_cost: None,
        validator_error: None,
        final_status: Some(AiStatus::MissingApiKey),
    }
}

fn missing_adapter() -> Wire {
    Wire::Structured {
        requests: vec![missing_sent()],
        outcome: StructuredOutcome::Failed(String::new()),
    }
}

/// Abbruch an der Ausgabegrenze: Status, `HTTP Status` der Antwort, `Error Code` = Grund, Tokens verbucht.
fn output_limit_sent(
    mut meta: RequestMeta,
    limit: &OutputLimitReached,
    reported_cost: Option<ReportedCost>,
) -> SentRequest {
    meta.error_code = Some(limit.reason.clone());
    meta.error_message = Some(format!("output limit reached ({})", limit.reason));
    meta.http_status = Some(limit.http_status);
    meta.usage = Some(limit.usage.clone());
    meta.response_received = true;
    SentRequest {
        meta,
        reported_cost,
        validator_error: None,
        final_status: Some(AiStatus::OutputLimitReached),
    }
}

impl AiProvider for WireProvider<'_> {
    fn provider_id(&self) -> &str {
        &self.provider
    }

    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
        Box::pin(async move {
            let wire = if request.model.endpoint == crate::adapters::openai_decisions::ENDPOINT {
                match self.ctx.adapters.decisions {
                    Some(d) => self.decisions(d, &request).await,
                    None => Wire::Decisions {
                        meta: Box::default(),
                        reply: Box::new(Err(FinalError {
                            error: ProviderError::MissingApiKey,
                            status: AiStatus::MissingApiKey,
                            http_status: None,
                            error_code: None,
                            message: String::new(),
                            provider_request_id: None,
                        })),
                    },
                }
            } else {
                self.structured(&request).await
            };
            *self.slot.lock().unwrap_or_else(|p| p.into_inner()) = Some(wire);
            Ok(Response {
                content: None,
                usage: Default::default(),
                refusal: false,
                provider_request_id: None,
                duration_ms: 0,
            })
        })
    }
}

/// Fähigkeiten, die ein Request einer Decision-Frage verlangt (für Aufrufer, die eigene Modelle bauen).
pub fn required_capabilities(task: TaskType, endpoint: Endpoint) -> Capabilities {
    task.required_capabilities(endpoint)
}

/// Ob ein Statustext zum Filter „Errors“ bzw. zu „Fehlgeschlagene erneut anfragen“ gehört (AI-020).
pub fn is_error_status(status: &str) -> bool {
    is_failed_status(status)
}

/// Ob ein gespeichertes Ergebnis eine Selbsteinschätzung ist (Filter „Self-reported Probabilities“, AI-034).
pub fn is_self_reported(row: &AnswerRow) -> bool {
    row.source.as_deref() == Some(routing::SOURCE_SELF_REPORTED)
}

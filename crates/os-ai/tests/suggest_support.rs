//! Gemeinsame Hilfen der Tests `suggest_*.rs` (M4-V01); wird per `#[path]` eingebunden und enthält selbst keine Tests.
//!
//! Fixture: `tests/fixtures/ai/suggest/pages.json` (12 Seiten mit je einem Title-/Meta-/H1-Problem, eine
//! noindex-Seite), Mock-Antworten `tests/fixtures/ai/mock-answers.d/v01.json`. Pixelbreiten liefert hier ein fester
//! Testmesser (9 px je Zeichen bei 20 px, 6 px bei 14 px); die Gleichheit mit UI-038 prüft `os-app` mit der echten
//! Funktion (`commands::ai_suggest`, Tests).
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_ai::adapters::openai_responses::{
    ApiKey, OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses, RESPONSES_CAPABILITIES,
};
use os_ai::cache::CacheOptions;
use os_ai::catalog::{AnswerTokens, Endpoint, ModelRef, ReasoningChoice, TaskPlan, TaskType};
use os_ai::cost::{LongContext, PriceSource, Rates};
use os_ai::limits::{BackoffPolicy, FixedJitter, Limiter, LimitsConfig, TokioClock};
use os_ai::policy::{ConsentBook, ConsentGate, ConsentKey, RedactionSettings, Redactor};
use os_ai::provider::{
    AiProvider, CapabilitySource, Input, Model, ProviderError, ProviderFuture, ReasoningLevel,
    Request, Response, Usage,
};
use os_ai::tasks::suggest::{
    Element, ElementJob, ElementLimits, Engine, FileStore, NoStyleRules, PageInput, RunControl,
    SiteIndex, SuggestOptions,
};
use os_ai::PreviewSwitch;
use os_model::budget::BudgetLimits;
use os_model::time::ManualClock;
use os_model::{CrawlConfig, Timestamp};
use os_store::ai::suggestions::PageRow;
use os_store::CrawlFile;
use os_testsite::mocks::ai::{AiMock, RequestRecord as MockRequest};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};

pub const KEY: &str = "sk-TEST-V01-fixture-key";
pub const HOST: &str = "openai.mock.test";
pub const BASE: &str = "http://ai.test:8080";

pub fn fixture(parts: &[&str]) -> PathBuf {
    let mut p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ai/suggest");
    for part in parts {
        p = p.join(part);
    }
    p
}

/// Eine Seite der Fixture.
#[derive(Debug, Clone)]
pub struct FixturePage {
    pub path: String,
    pub mock_id: String,
    pub title: Option<String>,
    pub meta: Option<String>,
    pub h1: Option<String>,
    pub lang: Option<String>,
    pub text: String,
    pub indexable: bool,
}

impl FixturePage {
    pub fn address(&self) -> String {
        format!("{BASE}{}", self.path)
    }
}

pub fn pages() -> Vec<FixturePage> {
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture(&["pages.json"])).unwrap()).unwrap();
    doc["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| FixturePage {
            path: p["path"].as_str().unwrap().into(),
            mock_id: p["mock_id"].as_str().unwrap().into(),
            title: p["title"].as_str().map(str::to_string),
            meta: p["meta"].as_str().map(str::to_string),
            h1: p["h1"].as_str().map(str::to_string),
            lang: p["lang"].as_str().map(str::to_string),
            text: p["text"].as_str().unwrap().into(),
            indexable: p["indexable"].as_bool().unwrap_or(true),
        })
        .collect()
}

/// Testmesser: 9 px je Zeichen bei 20 px (Titel), 6 px bei 14 px (Beschreibung); `W` doppelt.
pub fn fake_px(text: &str, size: f64) -> u32 {
    let per = if size >= 20.0 { 9 } else { 6 };
    text.chars()
        .map(|c| if c == 'W' { 2 * per } else { per })
        .sum()
}

/// Temporäres Verzeichnis, das am Ende gelöscht wird.
pub struct TempDir(pub PathBuf);
impl TempDir {
    pub fn new(name: &str) -> Self {
        // Laufende Nummer: Die Systemuhr allein trennt gleichzeitig startende Tests nicht sicher (macOS: Mikrosekunden).
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "os-ai-v01-{name}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Crawl-Datei mit den Seiten (URL-IDs 1 … n in Fixture-Reihenfolge) und einem KI-Lauf (`ai_runs.id = 1`).
pub fn crawl_file(dir: &Path, pages: &[FixturePage]) -> CrawlFile {
    use os_model::time::{IdSource, SeededIds};
    let created = Timestamp::from_db("2026-11-26T08:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: SeededIds::new(11).new_crawl_id(created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some(format!("{BASE}/")),
        list_source: None,
        project: "V01".into(),
        name: "suggest".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    let f = CrawlFile::create(
        &dir.join("suggest.ospider"),
        &init,
        os_store::StorageProfile::Balanced,
    )
    .unwrap();
    f.conn()
        .execute(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-26T08:00:00Z', '{}')",
            [],
        )
        .unwrap();
    for (i, p) in pages.iter().enumerate() {
        let address = p.address();
        let len = |v: &Option<String>| v.as_ref().map(|s| s.chars().count() as i64);
        let px = |v: &Option<String>, size| v.as_ref().map(|s| i64::from(fake_px(s, size)));
        let values = [
            json!(i as i64 + 1),
            json!(address),
            json!(BASE),
            json!(address.len() as i64),
            json!(if p.indexable {
                "Indexable"
            } else {
                "Non-Indexable"
            }),
            json!(p.title.is_some() as i64),
            json!(p.title),
            json!(len(&p.title)),
            json!(px(&p.title, 20.0)),
            json!(p.meta.is_some() as i64),
            json!(p.meta),
            json!(len(&p.meta)),
            json!(px(&p.meta, 14.0)),
            json!(p.h1.is_some() as i64),
            json!(p.h1),
        ];
        let v: Vec<String> = values.iter().map(literal).collect();
        f.conn()
            .execute_batch(&format!(
                "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length,
                   content_class,status_code,indexability,title_count,title_1,title_1_len,title_1_px,
                   meta_desc_count,meta_desc_1,meta_desc_1_len,meta_desc_1_px,h1_count,h1_1)
                 VALUES({id},{a},{a},{o},1,'Done','Seed',1,{l},'HTML',200,{rest});",
                id = v[0],
                a = v[1],
                o = v[2],
                l = v[3],
                rest = v[4..].join(","),
            ))
            .unwrap();
    }
    f
}

/// JSON-Wert als SQL-Literal (nur Testdaten).
pub fn literal(v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Bool(b) => (*b as i64).to_string(),
        other => format!("'{}'", other.to_string().replace('\'', "''")),
    }
}

/// Seiten der Crawl-Datei als [`PageInput`] (Seitentext und Sprache aus der Fixture; Fokus optional).
pub fn inputs(pages: &[FixturePage], focus: &BTreeMap<String, String>) -> Vec<PageInput> {
    pages
        .iter()
        .enumerate()
        .map(|(i, p)| PageInput {
            url_id: i as i64 + 1,
            address: p.address(),
            title: p.title.clone(),
            meta_description: p.meta.clone(),
            h1: p.h1.clone(),
            text: Some(p.text.clone()),
            language: p.lang.clone(),
            page_type: None,
            search_intent: None,
            focus: focus.get(&p.address()).cloned(),
        })
        .collect()
}

pub fn page_rows(pages: &[FixturePage]) -> Vec<PageRow> {
    pages
        .iter()
        .enumerate()
        .map(|(i, p)| PageRow {
            url_id: i as i64 + 1,
            address: p.address(),
            in_standard_scope: p.indexable,
            indexable: p.indexable,
            title: p.title.clone(),
            title_px: None,
            meta_description: p.meta.clone(),
            meta_description_px: None,
            h1: p.h1.clone(),
        })
        .collect()
}

/// Testpreise `openai/gpt-6-luna/responses` aus `tests/fixtures/ai/pricing-test.json` (0,10 / 0,50 USD je 1 Mio.).
pub fn rates() -> Rates {
    use os_ai::cost::rates::usd_per_million_to_pico;
    Rates {
        input_pico: usd_per_million_to_pico("0.10").unwrap(),
        output_pico: usd_per_million_to_pico("0.50").unwrap(),
        cache_read_pico: usd_per_million_to_pico("0.01").unwrap(),
        cache_write_pico: 0,
        batch_discount_ppm: 500_000,
        long_context: Some(LongContext {
            threshold_input_tokens: 272_000,
            input_factor_ppm: 2_000_000,
            output_factor_ppm: 1_500_000,
            cache_read_factor_ppm: 1_000_000,
        }),
        regional_uplift_ppm: 0,
        per_request_pico: 0,
        per_image_pico: 0,
        image_token_estimate: 85,
        source: PriceSource::Table,
    }
}

pub fn model() -> Model {
    Model {
        provider: "openai".into(),
        id: "gpt-6-luna".into(),
        endpoint: "responses".into(),
        capabilities: RESPONSES_CAPABILITIES,
        capability_source: CapabilitySource::ModelCatalog,
        reasoning_levels: vec![
            ReasoningLevel::Off,
            ReasoningLevel::Low,
            ReasoningLevel::Medium,
        ],
    }
}

pub fn plan(element: Element, reasoning: ReasoningLevel) -> TaskPlan {
    let task: TaskType = element.task();
    TaskPlan {
        task,
        model_ref: ModelRef::new("openai", "gpt-6-luna", Endpoint::Responses),
        model: model(),
        candidate_index: 0,
        reasoning: ReasoningChoice {
            requested: reasoning,
            effective: reasoning,
            dropdown: vec![
                ReasoningLevel::ModelDefault,
                ReasoningLevel::Off,
                ReasoningLevel::Low,
            ],
            warning: None,
        },
        answer_tokens: Some(AnswerTokens::Fixed(512)),
        model_output_limit: None,
        user_max_tokens: None,
    }
}

/// Einwilligung für (openai, Aufgabe, Feldmenge des Elements).
pub fn consent(elements: &[Element]) -> ConsentGate {
    let gate = ConsentGate::dialog(ConsentBook::new());
    for e in elements {
        gate.grant(
            ConsentKey::new("openai", e.task().as_str(), e.fields()),
            0,
            None,
        )
        .unwrap();
    }
    gate
}

pub fn redactor() -> Redactor {
    Redactor::new(RedactionSettings::default())
}

/// Limiter mit kurzem Backoff (Millisekunden) und `workers` gleichzeitigen Anfragen je Anbieter.
pub fn limiter(workers: u32) -> Limiter {
    let mut config = LimitsConfig {
        backoff: BackoffPolicy {
            base: Duration::from_millis(5),
            factor: 2.0,
            cap: Duration::from_millis(40),
            jitter: 0.0,
        },
        ..LimitsConfig::default()
    };
    config.providers.insert(
        "openai".into(),
        os_ai::limits::ProviderLimits {
            concurrent_requests: workers,
            timeout: Duration::from_secs(120),
        },
    );
    Limiter::with_jitter(
        config,
        Arc::new(TokioClock::new()),
        Arc::new(FixedJitter(0.5)),
    )
}

pub fn clock() -> ManualClock {
    ManualClock::new(Timestamp::from_db("2026-11-26T09:00:00.000Z").unwrap())
}

/// ai-mock auf `127.0.0.1` (Fragmente unter `tests/fixtures/ai/mock-answers.d`, darunter `v01.json`).
pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::fixtures().expect("ai-mock fragments load");
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Self { mock, server }
    }
    pub fn base(&self) -> String {
        format!("http://{HOST}:{}/openai/v1", self.server.addr().port())
    }
    pub fn provider(&self) -> OpenAiResponses {
        let client = reqwest::Client::builder()
            .no_proxy()
            .resolve(HOST, self.server.addr())
            .build()
            .unwrap();
        OpenAiResponses::new(OpenAiHttp::new(
            client,
            OpenAiConfig::new(ApiKey::new(KEY)).with_base_url(self.base()),
            OpenAiAccount::new(),
        ))
    }
    /// Requests an `/openai/v1/responses` (ohne Steueraufrufe).
    pub fn responses(&self) -> Vec<MockRequest> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.path == "/openai/v1/responses")
            .collect()
    }
    pub fn bodies(&self) -> Vec<Value> {
        self.responses()
            .iter()
            .map(|r| serde_json::from_str(&r.body).unwrap())
            .collect()
    }
    pub async fn scenario(&self, scenario: &str, count: usize) {
        let r = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!("http://{}/__mock/scenario", self.server.addr()))
            .json(&json!({"provider": "openai", "scenario": scenario, "count": count}))
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success());
    }
}

/// Alles, was ein [`Engine`] in Tests braucht (eigene Lebensdauer, damit Referenzen gültig bleiben).
pub struct Kit {
    pub consent: ConsentGate,
    pub redactor: Redactor,
    pub limiter: Limiter,
    pub ledger: os_model::budget::CrawlBudgetLedger,
    pub plan: TaskPlan,
    pub rates: Rates,
    pub store: FileStore,
    pub clock: ManualClock,
    pub control: RunControl,
    pub index: SiteIndex,
    pub style: NoStyleRules,
}

impl Kit {
    pub fn new(
        file: CrawlFile,
        element: Element,
        pages: &[FixturePage],
        limits: BudgetLimits,
    ) -> Self {
        let ledger = os_store::ai::budget::open_ledger(file.conn(), limits, &[]).unwrap();
        Self {
            consent: consent(&Element::ALL),
            redactor: redactor(),
            limiter: limiter(4),
            ledger,
            plan: plan(element, ReasoningLevel::Off),
            rates: rates(),
            store: FileStore::new(file),
            clock: clock(),
            control: RunControl::default(),
            index: SiteIndex::from_pages(&page_rows(pages)),
            style: NoStyleRules,
        }
    }

    pub fn unlimited() -> BudgetLimits {
        BudgetLimits {
            unlimited: true,
            ..BudgetLimits::default()
        }
    }

    pub fn engine<'a>(&'a self, provider: &'a dyn AiProvider) -> Engine<'a> {
        Engine {
            switch: PreviewSwitch::ON,
            consent: &self.consent,
            redactor: &self.redactor,
            provider,
            limiter: &self.limiter,
            ledger: &self.ledger,
            plan: &self.plan,
            price: os_ai::cost::PriceBasis::Rates(&self.rates),
            store: &self.store,
            meter: &fake_px,
            style: &self.style,
            clock: &self.clock,
            control: &self.control,
            run_id: 1,
            base_url: "http://openai.mock.test/openai/v1".into(),
            cache: CacheOptions::default(),
            schema_overhead_tokens: 80,
            workers: 4,
        }
    }

    pub fn job<'a>(&'a self, element: Element, pages: &'a [PageInput]) -> ElementJob<'a> {
        ElementJob {
            element,
            pages,
            index: &self.index,
            limits: ElementLimits::from_config(element, &CrawlConfig::default()),
            options: SuggestOptions::default(),
        }
    }

    /// Protokollzeilen der Aufgabe.
    pub fn log(&self, task: &str) -> Vec<os_store::ai::requests::LoggedRequest> {
        self.store.with(|f| {
            os_store::ai::requests::list(
                f.conn(),
                &os_store::ai::requests::RequestFilter {
                    task: Some(task.into()),
                    ..Default::default()
                },
            )
            .unwrap()
        })
    }
}

/// Skriptbarer Anbieter für Rundenfolgen, Verzögerungen und Fehler (ohne HTTP). Antworten je MOCK-ID (aus dem
/// gesendeten Seitentext), je Aufruf das nächste Element der Folge; das letzte gilt danach weiter.
pub struct Scripted {
    pub answers: HashMap<String, Vec<Result<Value, ProviderError>>>,
    pub delays: HashMap<String, Duration>,
    pub calls: Mutex<Vec<(String, String)>>,
    counters: Mutex<HashMap<String, usize>>,
}

impl Default for Scripted {
    fn default() -> Self {
        Self::new()
    }
}

impl Scripted {
    pub fn new() -> Self {
        Self {
            answers: HashMap::new(),
            delays: HashMap::new(),
            calls: Mutex::new(Vec::new()),
            counters: Mutex::new(HashMap::new()),
        }
    }
    pub fn answer(mut self, id: &str, seq: Vec<Result<Value, ProviderError>>) -> Self {
        self.answers.insert(id.into(), seq);
        self
    }
    pub fn delay(mut self, id: &str, d: Duration) -> Self {
        self.delays.insert(id.into(), d);
        self
    }
    /// Gesendete Requests (MOCK-ID, Anweisungen).
    pub fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().unwrap().clone()
    }
    pub fn count(&self, id: &str) -> usize {
        self.calls().iter().filter(|(i, _)| i == id).count()
    }
}

pub fn candidates(list: &[&str]) -> Result<Value, ProviderError> {
    Ok(json!({ "candidates": list }))
}

fn mock_id(request: &Request) -> String {
    let text: String = request
        .input
        .iter()
        .filter_map(|i| match i {
            Input::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    text.split("MOCK-ID:")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or("")
        .to_string()
}

impl AiProvider for Scripted {
    fn provider_id(&self) -> &str {
        "openai"
    }
    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async { Ok(vec![model()]) })
    }
    fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
        Box::pin(async move {
            let id = mock_id(&request);
            self.calls
                .lock()
                .unwrap()
                .push((id.clone(), request.instructions.clone()));
            if let Some(d) = self.delays.get(&id) {
                tokio::time::sleep(*d).await;
            }
            let n = {
                let mut c = self.counters.lock().unwrap();
                let e = c.entry(id.clone()).or_insert(0);
                *e += 1;
                *e - 1
            };
            let seq = self.answers.get(&id).cloned().unwrap_or_default();
            let answer = seq
                .get(n)
                .or_else(|| seq.last())
                .cloned()
                .unwrap_or_else(|| candidates(&[]));
            let refusal = answer.as_ref().is_ok_and(|v| *v == json!("REFUSAL"));
            answer.map(|content| Response {
                content: (!refusal).then_some(content),
                refusal,
                usage: Usage {
                    input_tokens: Some(200),
                    output_tokens: Some(40),
                    cached_input_tokens: Some(0),
                    cache_write_tokens: Some(0),
                    reasoning_tokens: Some(0),
                },
                provider_request_id: Some(format!("req_scripted_{n}")),
                duration_ms: 1,
            })
        })
    }
}

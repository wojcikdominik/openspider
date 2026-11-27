//! Hilfen der unabhängigen Abnahmetests M4-V01 (08 AI-070, AI-071, AI-072, AI-075, AI-076; 1.1-Etappe).
//!
//! * eigene Crawl-Datei mit Seiten (`urls`) und KI-Lauf,
//! * ein **geskripteter Anbieter** (`Scripted`, Testdoppel hinter `policy::guard::execute`): liefert je Seite und Runde
//!   festgelegte Kandidaten, protokolliert jeden Request (Anweisungen, Eingabe) und kann verzögern oder stoppen,
//! * der **ai-mock** (`os-testsite`) mit eigenen Antwortfragmenten unter `tests/fixtures/ai/v01-acceptance/`,
//! * eine Pixelmessung als Tabelle (os-ai hängt nicht von os-parse ab; die Übereinstimmung mit UI-038 prüfen die
//!   os-app-Tests).
//!
//! Erwartungswerte stehen in den Tests und kommen nur aus `docs/spec/*`. Nur 127.0.0.1, nur erfundene Schlüssel.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_ai::cache::CacheOptions;
use os_ai::catalog::{AnswerTokens, Endpoint, ModelRef, ReasoningChoice, TaskPlan, TaskType};
use os_ai::cost::{PriceBasis, PriceSource, Rates};
use os_ai::limits::{BackoffPolicy, Limiter, LimitsConfig, ProviderLimits, TokioClock};
use os_ai::policy::{ConsentGate, RedactionSettings, Redactor};
use os_ai::provider::{
    AiProvider, Capabilities, CapabilitySource, Input, Model, ProviderError, ProviderFuture,
    ReasoningLevel, Request, Response, Usage,
};
use os_ai::tasks::suggest::{
    ElementJob, ElementLimits, Engine, FileStore, NoStyleRules, PageInput, PixelMeter, RunControl,
    SiteIndex, SuggestOptions,
};
use os_ai::tasks::suggest::Element;
use os_ai::PreviewSwitch;
use os_model::budget::{BudgetLimits, CrawlBudgetLedger, MicroUsd};
use os_store::ai::suggestions as store;
use serde_json::{json, Value};

pub const HOST: &str = "http://ai.test:8080";
pub const ANTHROPIC_KEY: &str = "sk-ant-TEST-V01-ACCEPTANCE";
pub const MESSAGES: &str = "/anthropic/v1/messages";

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn fixture_dir() -> PathBuf {
    repo_root().join("tests/fixtures/ai/v01-acceptance")
}

/// Standardgrenzen aus AI-075 (Title 30–60 Zeichen, 200–561 px).
pub const TITLE_LIMITS: ElementLimits = ElementLimits {
    min_chars: 30,
    max_chars: 60,
    min_px: 200,
    max_px: 561,
};
/// Standardgrenzen aus AI-075 (Meta Description 70–155 Zeichen, 400–985 px).
pub const META_LIMITS: ElementLimits = ElementLimits {
    min_chars: 70,
    max_chars: 155,
    min_px: 400,
    max_px: 985,
};

// ---------------------------------------------------------------------------------------------------------------
// Temporäres Verzeichnis (os-ai hat kein tempfile).

pub struct TempDir(pub PathBuf);
impl TempDir {
    pub fn new(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "os-ai-v01-acc-{tag}-{}-{nanos}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
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

// ---------------------------------------------------------------------------------------------------------------
// Seiten und Crawl-Datei.

/// Eine Seite der eigenen Fixture.
#[derive(Debug, Clone)]
pub struct Page {
    pub path: String,
    pub title: Option<String>,
    pub meta: Option<String>,
    pub h1: Option<String>,
    pub indexable: bool,
    pub status: i64,
    pub language: Option<String>,
    pub text: String,
    pub focus: Option<String>,
}

impl Page {
    /// Indexierbare 200-Seite mit Seitentext `PAGE-KEY:<key>`.
    pub fn new(path: &str, title: Option<&str>) -> Self {
        Self {
            path: path.to_string(),
            title: title.map(str::to_string),
            meta: None,
            h1: Some(format!("Überschrift {path}")),
            indexable: true,
            status: 200,
            language: None,
            text: format!("Seitentext der Seite {path}. PAGE-KEY:{path} Ende."),
            focus: None,
        }
    }
    pub fn address(&self) -> String {
        format!("{HOST}{}", self.path)
    }
    pub fn noindex(mut self) -> Self {
        self.indexable = false;
        self
    }
    pub fn lang(mut self, code: &str) -> Self {
        self.language = Some(code.to_string());
        self
    }
    pub fn meta(mut self, m: &str) -> Self {
        self.meta = Some(m.to_string());
        self
    }
    pub fn text(mut self, t: &str) -> Self {
        self.text = t.to_string();
        self
    }
    pub fn focus(mut self, f: &str) -> Self {
        self.focus = Some(f.to_string());
        self
    }
}

fn q(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("'{}'", s.replace('\'', "''")),
        None => "NULL".into(),
    }
}

/// Neue Crawl-Datei mit den Seiten (IDs 1 … n in Reihenfolge) und einem KI-Lauf; liefert (Datei, Lauf-ID).
pub fn crawl_file(dir: &Path, pages: &[Page]) -> (os_store::CrawlFile, i64) {
    use os_model::time::{IdSource, SeededIds};
    let created = os_model::Timestamp::from_db("2026-11-27T10:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: SeededIds::new(7101).new_crawl_id(created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some(format!("{HOST}/")),
        list_source: None,
        project: "Abnahme".into(),
        name: "V01".into(),
        config: os_model::CrawlConfig::default(),
        created_at: created,
    };
    let f = os_store::CrawlFile::create(
        &dir.join("v01.ospider"),
        &init,
        os_store::StorageProfile::Balanced,
    )
    .unwrap();
    let mut sql = String::new();
    for (i, p) in pages.iter().enumerate() {
        let id = i as i64 + 1;
        let address = p.address();
        sql.push_str(&format!(
            "INSERT INTO urls (id, url, address, origin, is_internal, state, discovered_via, folder_depth, url_length, \
             status_code, content_class, in_tables, indexability, title_1, title_1_px, meta_desc_1, meta_desc_1_px, h1_1) \
             VALUES ({id}, {a}, {a}, '{HOST}', 1, 'done', 'link', 1, {len}, {status}, 'HTML', 1, {idx}, {t}, NULL, {m}, NULL, {h});\n",
            a = q(Some(&address)),
            len = address.chars().count(),
            status = p.status,
            idx = if p.indexable { "'Indexable'" } else { "'Non-Indexable'" },
            t = q(p.title.as_deref()),
            m = q(p.meta.as_deref()),
            h = q(p.h1.as_deref()),
        ));
    }
    f.conn().execute_batch(&sql).unwrap();
    let run = store::begin_run(f.conn(), "2026-11-27T10:00:00Z", &json!({})).unwrap();
    (f, run)
}

/// Eingaben der Seiten (Kontext nach AI-070) zu den IDs aus [`crawl_file`].
pub fn inputs(pages: &[Page]) -> Vec<PageInput> {
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
            language: p.language.clone(),
            page_type: None,
            search_intent: None,
            focus: p.focus.clone(),
        })
        .collect()
}

/// Index der Dublettenprüfung aus den Seiten der Datei.
pub fn site_index(file: &os_store::CrawlFile) -> SiteIndex {
    SiteIndex::from_pages(&store::pages(file.conn()).unwrap())
}

// ---------------------------------------------------------------------------------------------------------------
// Pixelmessung (Tabelle; Standard: Title 9 px je Zeichen, Description 6 px je Zeichen).

#[derive(Debug, Clone, Default)]
pub struct TableMeter(pub BTreeMap<String, u32>);
impl TableMeter {
    pub fn with(mut self, text: &str, px: u32) -> Self {
        self.0.insert(text.to_string(), px);
        self
    }
}
impl PixelMeter for TableMeter {
    fn width(&self, text: &str, size_px: f64) -> u32 {
        if let Some(px) = self.0.get(text) {
            return *px;
        }
        let n = text.chars().count() as u32;
        if size_px >= 20.0 {
            9 * n
        } else {
            6 * n
        }
    }
}

/// Text mit genau `n` Zeichen (Codepoints), beginnend mit `prefix`.
pub fn text_of(prefix: &str, n: usize) -> String {
    let mut s: String = prefix.to_string();
    let filler = "abcdefghij";
    let mut i = 0;
    while s.chars().count() < n {
        s.push(filler.chars().nth(i % filler.len()).unwrap());
        i += 1;
    }
    s.chars().take(n).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Geskripteter Anbieter.

/// Antwort einer Runde.
#[derive(Debug, Clone)]
pub enum Reply {
    Candidates(Vec<String>),
    /// Schemaverstoß (zusätzliches Feld, kein `candidates`).
    Invalid,
    Refusal,
    OutputLimit,
}

pub fn cands<S: AsRef<str>>(v: &[S]) -> Reply {
    Reply::Candidates(v.iter().map(|s| s.as_ref().to_string()).collect())
}

/// Ein empfangener Request.
#[derive(Debug, Clone)]
pub struct Call {
    pub key: String,
    pub instructions: String,
    pub input: String,
    pub max_tokens: Option<u64>,
}

type Hook = Box<dyn Fn(usize, &str) + Send + Sync>;

pub struct Scripted {
    pub scripts: BTreeMap<String, Vec<Reply>>,
    pub delays: BTreeMap<String, u64>,
    pub calls: Mutex<Vec<Call>>,
    pub hook: Option<Hook>,
    pub model: Model,
}

pub fn anthropic_model() -> Model {
    Model {
        provider: "anthropic".into(),
        id: "mock-claude".into(),
        endpoint: "messages".into(),
        capabilities: Capabilities {
            text: true,
            structured_output: true,
            vision: false,
            batch: false,
            embeddings: false,
            decisions: false,
            prices_via_api: false,
            local: false,
        },
        capability_source: CapabilitySource::AnthropicModels,
        reasoning_levels: Vec::new(),
    }
}

impl Scripted {
    pub fn new() -> Self {
        Self {
            scripts: BTreeMap::new(),
            delays: BTreeMap::new(),
            calls: Mutex::new(Vec::new()),
            hook: None,
            model: anthropic_model(),
        }
    }
    /// Antworten der Seite `path` je Runde (die letzte gilt für alle weiteren).
    pub fn script(mut self, path: &str, replies: Vec<Reply>) -> Self {
        self.scripts.insert(path.to_string(), replies);
        self
    }
    pub fn delay(mut self, path: &str, ms: u64) -> Self {
        self.delays.insert(path.to_string(), ms);
        self
    }
    pub fn on_call(mut self, hook: impl Fn(usize, &str) + Send + Sync + 'static) -> Self {
        self.hook = Some(Box::new(hook));
        self
    }
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
    pub fn calls_for(&self, path: &str) -> Vec<Call> {
        self.calls().into_iter().filter(|c| c.key == path).collect()
    }
    pub fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl Default for Scripted {
    fn default() -> Self {
        Self::new()
    }
}

fn key_of(input: &str) -> String {
    input
        .split_once("PAGE-KEY:")
        .map(|(_, t)| t.split_whitespace().next().unwrap_or("").to_string())
        .unwrap_or_default()
}

impl AiProvider for Scripted {
    fn provider_id(&self) -> &str {
        "anthropic"
    }
    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        let m = self.model.clone();
        Box::pin(async move { Ok(vec![m]) })
    }
    fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
        Box::pin(async move {
            let input: String = request
                .input
                .iter()
                .filter_map(|i| match i {
                    Input::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let key = key_of(&input);
            let (n, previous) = {
                let mut calls = self.calls.lock().unwrap();
                let previous = calls.iter().filter(|c| c.key == key).count();
                calls.push(Call {
                    key: key.clone(),
                    instructions: request.instructions.clone(),
                    input: input.clone(),
                    max_tokens: request.max_tokens,
                });
                (calls.len(), previous)
            };
            if let Some(hook) = &self.hook {
                hook(n, &key);
            }
            if let Some(ms) = self.delays.get(&key) {
                tokio::time::sleep(Duration::from_millis(*ms)).await;
            }
            let reply = self
                .scripts
                .get(&key)
                .and_then(|r| r.get(previous.min(r.len().saturating_sub(1))).cloned())
                .unwrap_or_else(|| cands(&["Standardkandidat für eine Testseite ohne Skript"]));
            let usage = Usage {
                input_tokens: Some(100),
                output_tokens: Some(20),
                cached_input_tokens: Some(0),
                cache_write_tokens: Some(0),
                reasoning_tokens: None,
            };
            let ok = |content: Option<Value>, refusal: bool| Response {
                content,
                usage: usage.clone(),
                refusal,
                provider_request_id: Some(format!("req_v01_{n}")),
                duration_ms: 1,
            };
            match reply {
                Reply::Candidates(c) => Ok(ok(Some(json!({ "candidates": c })), false)),
                Reply::Invalid => Ok(ok(Some(json!({ "unexpected": ["kein Schema"] })), false)),
                Reply::Refusal => Ok(ok(None, true)),
                Reply::OutputLimit => Err(ProviderError::output_limit("max_tokens")),
            }
        })
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Laufumgebung.

/// Begrenzung mit kurzen Wartezeiten und `concurrent` gleichzeitigen Anfragen (Regeln unverändert).
pub fn limiter(concurrent: u32) -> Limiter {
    let mut config = LimitsConfig {
        backoff: BackoffPolicy {
            base: Duration::from_millis(2),
            factor: 2.0,
            cap: Duration::from_millis(20),
            jitter: 0.0,
        },
        ..LimitsConfig::default()
    };
    let limits = ProviderLimits {
        concurrent_requests: concurrent,
        timeout: Duration::from_secs(20),
    };
    config.default_provider = limits;
    for value in config.providers.values_mut() {
        *value = limits;
    }
    Limiter::new(config, Arc::new(TokioClock::new()))
}

/// `mock-claude` aus `tests/fixtures/ai/pricing-test.json` (1 / 5 USD je 1 Mio. Tokens).
pub fn claude_rates() -> Rates {
    Rates {
        input_pico: 1_000_000,
        output_pico: 5_000_000,
        cache_read_pico: 0,
        cache_write_pico: 0,
        batch_discount_ppm: 0,
        long_context: None,
        regional_uplift_ppm: 0,
        per_request_pico: 0,
        per_image_pico: 0,
        image_token_estimate: 0,
        source: PriceSource::Table,
    }
}

pub fn plan(element: Element, model: Model) -> TaskPlan {
    TaskPlan {
        task: match element {
            Element::Title => TaskType::SuggestTitle,
            Element::MetaDescription => TaskType::SuggestMeta,
        },
        model_ref: ModelRef::new(&model.provider, &model.id, Endpoint::Messages),
        model,
        candidate_index: 0,
        reasoning: ReasoningChoice {
            requested: ReasoningLevel::ModelDefault,
            effective: ReasoningLevel::ModelDefault,
            dropdown: vec![ReasoningLevel::ModelDefault],
            warning: None,
        },
        answer_tokens: Some(AnswerTokens::Fixed(512)),
        model_output_limit: None,
        user_max_tokens: None,
    }
}

pub fn budget(micro_usd: u64) -> BudgetLimits {
    BudgetLimits {
        crawl: MicroUsd(micro_usd),
        ..BudgetLimits::default()
    }
}

/// Alles für einen Vorschlagslauf auf einer eigenen Crawl-Datei.
pub struct Kit {
    pub dir: TempDir,
    pub path: PathBuf,
    pub pages: Vec<Page>,
    pub inputs: Vec<PageInput>,
    pub index: SiteIndex,
    pub store: FileStore,
    pub run_id: i64,
    pub ledger: CrawlBudgetLedger,
    pub limiter: Limiter,
    pub consent: ConsentGate,
    pub redactor: Redactor,
    pub plan: TaskPlan,
    pub rates: Rates,
    pub control: Arc<RunControl>,
    pub switch: PreviewSwitch,
    pub workers: usize,
    pub cache: CacheOptions,
    pub base_url: String,
}

impl Kit {
    pub fn new(tag: &str, pages: Vec<Page>) -> Self {
        Self::with_element(tag, pages, Element::Title)
    }

    pub fn with_element(tag: &str, pages: Vec<Page>, element: Element) -> Self {
        let dir = TempDir::new(tag);
        let (file, run_id) = crawl_file(&dir.0, &pages);
        let path = file.path().to_path_buf();
        let index = site_index(&file);
        let inputs = inputs(&pages);
        Self {
            path,
            pages,
            inputs,
            index,
            store: FileStore::new(file),
            run_id,
            ledger: CrawlBudgetLedger::new(budget(5_000_000)),
            limiter: limiter(4),
            consent: ConsentGate::cli(true),
            redactor: Redactor::new(RedactionSettings::default()),
            plan: plan(element, anthropic_model()),
            rates: claude_rates(),
            control: Arc::new(RunControl::default()),
            switch: PreviewSwitch::ON,
            workers: 1,
            cache: CacheOptions::default(),
            base_url: "http://127.0.0.1/anthropic".into(),
            dir,
        }
    }

    pub fn engine<'a>(
        &'a self,
        provider: &'a dyn AiProvider,
        meter: &'a dyn PixelMeter,
    ) -> Engine<'a> {
        Engine {
            switch: self.switch,
            consent: &self.consent,
            redactor: &self.redactor,
            provider,
            limiter: &self.limiter,
            ledger: &self.ledger,
            plan: &self.plan,
            price: PriceBasis::Rates(&self.rates),
            store: &self.store,
            meter,
            style: &NoStyleRules,
            clock: &os_model::SystemClock,
            control: &self.control,
            run_id: self.run_id,
            base_url: self.base_url.clone(),
            cache: self.cache,
            schema_overhead_tokens: 50,
            workers: self.workers,
        }
    }

    pub fn job(&self, element: Element, options: SuggestOptions) -> ElementJob<'_> {
        ElementJob {
            element,
            pages: &self.inputs,
            index: &self.index,
            limits: match element {
                Element::Title => TITLE_LIMITS,
                Element::MetaDescription => META_LIMITS,
            },
            options,
        }
    }

    pub fn id_of(&self, path: &str) -> i64 {
        self.pages.iter().position(|p| p.path == path).unwrap() as i64 + 1
    }

    pub fn record(&self, path: &str, element: Element) -> store::ElementRecord {
        let id = self.id_of(path);
        self.store
            .with(|f| store::load(f.conn(), id, element.id()).unwrap())
            .unwrap_or_else(|| panic!("kein Eintrag für {path}"))
    }

    pub fn count(&self, sql: &str) -> i64 {
        self.store
            .with(|f| f.conn().query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap())
    }

    /// Protokollzeilen (`ai_requests`) einer Seite: (Cache-Treffer, Kosten µUSD, Reservierung µUSD, Status).
    pub fn log_rows(&self, path: &str) -> Vec<(bool, i64, i64, String)> {
        let id = self.id_of(path);
        self.store.with(|f| {
            let mut stmt = f
                .conn()
                .prepare(&format!(
                    "SELECT cache_hit, cost_microusd, reservation_microusd, status FROM ai_requests \
                     WHERE url_id = {id} ORDER BY id"
                ))
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)? == 1,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            rows
        })
    }
}

/// Standardoptionen (3 Kandidaten, 2 Reparaturrunden) – aus AI-070/AI-075.
pub fn options() -> SuggestOptions {
    SuggestOptions::default()
}

/// Gültige Title-Kandidaten (30–60 Zeichen bei 9 px je Zeichen) mit erkennbarem Präfix.
pub fn valid_title(tag: &str) -> String {
    text_of(&format!("Gültiger Title {tag} "), 40)
}

// ---------------------------------------------------------------------------------------------------------------
// ai-mock mit eigenen Antwortfragmenten.

pub fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

pub struct Mock {
    pub mock: os_testsite::mocks::ai::AiMock,
    pub server: os_testsite::mocks::MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = os_testsite::mocks::ai::AiMock::from_fragments(fixture_dir().join("mock-answers.d"))
            .expect("eigene ai-mock-Fragmente lesbar");
        let server = os_testsite::mocks::MockServer::start(mock.registrations())
            .await
            .expect("ai-mock startet");
        Self { mock, server }
    }
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.server.addr())
    }
    pub fn anthropic(&self) -> os_ai::adapters::anthropic::AnthropicAdapter {
        use os_ai::adapters::anthropic::{AnthropicAdapter, AnthropicConfig, ApiKey};
        AnthropicAdapter::new(
            client(),
            AnthropicConfig::new(ApiKey::new(ANTHROPIC_KEY)).with_base_url(self.url("/anthropic")),
        )
    }
    /// POST-Körper an die Messages-API (ohne Steueraufrufe).
    pub fn posts(&self) -> Vec<String> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| r.method == "POST" && r.path.split('?').next() == Some(MESSAGES))
            .map(|r| r.body)
            .collect()
    }
    pub async fn control(&self, path: &str, body: Value) -> reqwest::StatusCode {
        client()
            .post(self.url(path))
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("Mock-Steuerung erreichbar")
            .status()
    }
}

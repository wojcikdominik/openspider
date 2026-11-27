//! Gemeinsame Hilfen der Tests von M4-D01 (Decision-Engine): ai-mock starten und steuern (auch `hold`), Adapter gegen
//! den Mock, Crawl-Datei mit den URLs der Fixture-Site, Lauf-Einstellungen. Wird von den Testdateien
//! `decisions_*.rs` mit `#[path]` eingebunden; als eigene Testdatei enthält sie keine Tests.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use os_ai::adapters::anthropic::{AnthropicAdapter, AnthropicConfig};
use os_ai::adapters::ollama::{OllamaAdapter, OllamaConfig};
use os_ai::adapters::openai_decisions::OpenAiDecisions;
use os_ai::adapters::openai_responses::{OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses};
use os_ai::adapters::openrouter::{OpenRouterAdapter, OpenRouterConfig};
use os_ai::catalog::{AvailableModel, ProviderState, TaskDefaults, TaskSettings, TaskType};
use os_ai::cost::{PriceSource, Rates};
use os_ai::decisions::exec::{
    ActiveQuestion, Adapters, DecisionRun, PriceLookup, RunControl, RunItem, RunOptions,
};
use os_ai::decisions::format::{self, QuestionDef};
use os_ai::decisions::input::SubjectData;
use os_ai::decisions::routing::{RoutingContext, RoutingPolicy, RoutingSetting};
use os_ai::limits::{BackoffPolicy, Limiter, LimitsConfig, ProviderLimits, TokioClock};
use os_ai::policy::{ConsentGate, RedactionSettings, Redactor};
use os_ai::provider::{Capabilities, CapabilitySource, Model};
use os_ai::PreviewSwitch;
use os_model::budget::{BudgetLimits, CrawlBudgetLedger, MicroUsd};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};

pub const OPENAI_KEY: &str = "sk-TEST-D01";
pub const ANTHROPIC_KEY: &str = "sk-ant-TEST-D01";
pub const OPENROUTER_KEY: &str = "sk-or-TEST-D01";
pub const DECISIONS: &str = "/openai/v1/decisions";
pub const MESSAGES: &str = "/anthropic/v1/messages";
pub const RESPONSES: &str = "/openai/v1/responses";
pub const COMPLETIONS: &str = "/openrouter/api/v1/chat/completions";
pub const OLLAMA_CHAT: &str = "/ollama/api/chat";

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Fixtures dieses Pakets unter `tests/fixtures/ai/decisions/`.
pub fn fixture(rel: &str) -> PathBuf {
    repo_root().join("tests/fixtures/ai/decisions").join(rel)
}

pub fn fixture_text(rel: &str) -> String {
    std::fs::read_to_string(fixture(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Frage aus `tests/fixtures/ai/decisions/questions/valid/<name>.question.json`.
pub fn question(name: &str) -> QuestionDef {
    format::parse(&fixture_text(&format!(
        "questions/valid/{name}.question.json"
    )))
    .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

pub fn active(def: QuestionDef) -> ActiveQuestion {
    ActiveQuestion {
        def,
        routing: RoutingSetting::Automatic,
    }
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

/// Laufender ai-mock.
pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::fixtures().expect("ai-mock-Fragmente lesbar");
        let server = MockServer::start(mock.registrations()).await.unwrap();
        Self { mock, server }
    }
    pub fn addr(&self) -> SocketAddr {
        self.server.addr()
    }
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr())
    }
    pub fn openai_http(&self) -> OpenAiHttp {
        OpenAiHttp::new(
            client(),
            OpenAiConfig::new(os_ai::adapters::openai_responses::ApiKey::new(OPENAI_KEY))
                .with_base_url(self.url("/openai/v1")),
            OpenAiAccount::new(),
        )
    }
    pub fn decisions(&self) -> OpenAiDecisions {
        OpenAiDecisions::new(self.openai_http())
    }
    pub fn responses(&self) -> OpenAiResponses {
        OpenAiResponses::new(self.openai_http())
    }
    pub fn anthropic(&self) -> AnthropicAdapter {
        AnthropicAdapter::new(
            client(),
            AnthropicConfig::new(os_ai::adapters::anthropic::ApiKey::new(ANTHROPIC_KEY))
                .with_base_url(self.url("/anthropic")),
        )
    }
    pub fn openrouter(&self) -> OpenRouterAdapter {
        OpenRouterAdapter::new(
            client(),
            OpenRouterConfig::new(os_ai::adapters::openrouter::ApiKey::new(OPENROUTER_KEY))
                .with_base_url(self.url("/openrouter/api/v1")),
        )
    }
    pub fn ollama(&self) -> OllamaAdapter {
        OllamaAdapter::new(
            client(),
            OllamaConfig::new().with_base_url(self.url("/ollama")),
        )
    }
    async fn control(&self, path: &str, body: Value) -> Value {
        client()
            .post(self.url(path))
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    pub async fn scenario(&self, provider: &str, scenario: &str, count: usize) {
        self.control(
            "/__mock/scenario",
            json!({"provider": provider, "scenario": scenario, "count": count}),
        )
        .await;
    }
    /// `hold` für die nächsten `count` Requests.
    pub async fn hold(&self, provider: &str, count: usize) {
        self.scenario(provider, "hold", count).await;
    }
    /// Gibt `count` gehaltene Requests frei; Rückgabe: Zahl.
    pub async fn release(&self, provider: &str, count: usize) -> u64 {
        self.control(
            "/__mock/release",
            json!({"provider": provider, "count": count}),
        )
        .await["released"]
            .as_u64()
            .unwrap()
    }
    /// Anbieter-Requests (ohne Steueraufrufe).
    pub fn log(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| !r.path.starts_with("/__mock"))
            .collect()
    }
    /// POST-Requests an `path`.
    pub fn posts(&self, path: &str) -> Vec<RequestRecord> {
        self.log()
            .into_iter()
            .filter(|r| r.method == "POST" && r.path.split('?').next() == Some(path))
            .collect()
    }
    pub fn bodies(&self, path: &str) -> Vec<Value> {
        self.posts(path)
            .iter()
            .map(|r| serde_json::from_str(&r.body).unwrap())
            .collect()
    }
    /// Wartet, bis `pred` auf dem Protokoll gilt (an den Request gebunden; Obergrenze 30 s nur gegen Hänger).
    pub async fn until(&self, what: &str, pred: impl Fn(&[RequestRecord]) -> bool) {
        for _ in 0..3000 {
            if pred(&self.log()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("Mock-Protokoll erreicht nicht: {what}");
    }
    /// Beantwortete (nicht gehaltene bzw. freigegebene) und gehaltene Requests an `path`.
    pub fn held(&self, path: &str) -> usize {
        self.posts(path)
            .iter()
            .filter(|r| r.held && !r.aborted)
            .count()
    }
}

/// Begrenzung: eine gleichzeitige Anfrage, kurze Backoff-Pausen (Regeln unverändert), Timeout `timeout`.
pub fn limiter_with(timeout: Duration) -> Limiter {
    let mut config = LimitsConfig {
        backoff: BackoffPolicy {
            base: Duration::from_millis(1),
            factor: 2.0,
            cap: Duration::from_millis(4),
            jitter: 0.0,
        },
        ..LimitsConfig::default()
    };
    let limits = ProviderLimits {
        concurrent_requests: 1,
        timeout,
    };
    config.default_provider = limits;
    for value in config.providers.values_mut() {
        *value = limits;
    }
    Limiter::new(config, Arc::new(TokioClock::new()))
}

pub fn limiter() -> Limiter {
    limiter_with(Duration::from_secs(20))
}

/// Testpreise (runde Werte wie `pricing-test.json`): Decisions 0,10 USD/1M Eingabe, sonst 1/5 USD, Ollama 0.
pub struct TestPrices;
impl PriceLookup for TestPrices {
    fn rates(&self, provider: &str, model: &str, endpoint: &str) -> Option<Rates> {
        // Bild-Schätzwert über den 85 Tokens des Mocks, damit die Reservierung die Ist-Kosten deckt (AI-013).
        Self::base(provider, model, endpoint).map(|r| Rates {
            image_token_estimate: 200,
            ..r
        })
    }
}
impl TestPrices {
    fn base(provider: &str, _model: &str, endpoint: &str) -> Option<Rates> {
        match (provider, endpoint) {
            ("ollama", _) => Some(Rates::free(PriceSource::Table)),
            (_, "decisions") => Rates::per_million("0.10", "0", PriceSource::Table),
            (_, "responses") => Rates::per_million("0.10", "0.50", PriceSource::Table),
            _ => Rates::per_million("1.00", "5.00", PriceSource::Table),
        }
    }
}

pub fn caps(decisions: bool, vision: bool, local: bool) -> Capabilities {
    Capabilities {
        text: !decisions,
        structured_output: !decisions,
        vision,
        embeddings: false,
        decisions,
        batch: false,
        prices_via_api: false,
        local,
    }
}

pub fn model(provider: &str, id: &str, endpoint: &str, c: Capabilities) -> Model {
    Model {
        provider: provider.into(),
        id: id.into(),
        endpoint: endpoint.into(),
        capabilities: c,
        capability_source: CapabilitySource::ModelCatalog,
        reasoning_levels: Vec::new(),
    }
}

/// Anbieterzustand: verbundene Anbieter mit den Modellen der Standardliste (`task-defaults.json`) bzw. `extra`.
pub fn providers(keys: &[&str], extra: Vec<AvailableModel>) -> BTreeMap<String, ProviderState> {
    let mut out: BTreeMap<String, ProviderState> = BTreeMap::new();
    let standard = [
        model("openai", "gpt-6-luna", "decisions", caps(true, true, false)),
        model(
            "anthropic",
            "claude-haiku-5-5",
            "messages",
            caps(false, true, false),
        ),
        model(
            "openrouter",
            "openai/gpt-6-luna",
            "chat-completions",
            caps(false, true, false),
        ),
    ];
    for m in standard
        .into_iter()
        .map(|model| AvailableModel {
            model,
            max_output_tokens: None,
        })
        .chain(extra)
    {
        if keys.contains(&m.model.provider.as_str()) {
            let s = out.entry(m.model.provider.clone()).or_default();
            s.has_key = true;
            s.models.push(m);
        }
    }
    for k in keys {
        out.entry((*k).to_string()).or_default().has_key = true;
    }
    out
}

pub fn defaults() -> TaskDefaults {
    TaskDefaults::bundled().unwrap()
}

/// Temporäres Verzeichnis (beim Drop gelöscht).
pub struct TempDir(pub PathBuf);
impl TempDir {
    pub fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p =
            std::env::temp_dir().join(format!("os-ai-d01-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn now() -> os_model::Timestamp {
    os_model::Timestamp::from_db("2026-11-26T12:00:00.000Z").unwrap()
}

/// Neue Crawl-Datei (Zustand `Completed`) mit den URLs `items` (Kennungen = `url_id`) und einem Lauf in `ai_runs`.
pub fn crawl_file(dir: &Path, items: &[RunItem]) -> (os_store::CrawlFile, i64) {
    use os_model::time::{IdSource, SeededIds};
    let created = os_model::Timestamp::from_db("2026-11-26T10:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: SeededIds::new(2001).new_crawl_id(created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some("http://ai.test:8080/".into()),
        list_source: None,
        project: "D01".into(),
        name: "D01".into(),
        config: os_model::CrawlConfig::default(),
        created_at: created,
    };
    let path = dir.join("d01.ospider");
    let f = os_store::CrawlFile::create(&path, &init, os_store::StorageProfile::Balanced).unwrap();
    for item in items {
        f.conn()
            .execute(
                "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length) \
                 VALUES(?1,?2,?2,'http://ai.test:8080',1,'Done','Seed',0,?3)",
                (item.url_id, &item.data.url, item.data.url.len() as i64),
            )
            .unwrap();
    }
    f.conn()
        .execute_batch("INSERT INTO crawl_meta(key,value) VALUES('state','Completed') ON CONFLICT(key) DO UPDATE SET value = excluded.value;")
        .unwrap();
    let run =
        os_store::ai::answers::begin_run(f.conn(), "2026-11-26T12:00:00Z", None, "{}").unwrap();
    (f, run)
}

/// Öffnet die Crawl-Datei erneut (wie nach einem Neustart).
pub fn reopen(dir: &Path) -> os_store::CrawlFile {
    os_store::CrawlFile::open(&dir.join("d01.ospider"), os_store::OpenMode::ReadWrite).unwrap()
}

fn between<'t>(html: &'t str, start: &str, end: &str) -> Option<&'t str> {
    let s = html.find(start)? + start.len();
    let e = html[s..].find(end)? + s;
    Some(&html[s..e])
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                tag = true;
                out.push(' ');
            }
            '>' => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Felder einer Fixture-Seite (einfache Extraktion für Tests; echte Läufe bekommen die Felder aus der Crawl-Datei).
pub fn page_data(url: &str, html: &str) -> SubjectData {
    let main = between(html, "<main>", "</main>").unwrap_or(html);
    SubjectData {
        url: url.to_string(),
        title: between(html, "<title>", "</title>").map(str::to_string),
        meta_description: between(html, "name=\"description\" content=\"", "\"")
            .map(str::to_string),
        h1: between(main, "<h1>", "</h1>")
            .map(|s| vec![strip_tags(s)])
            .unwrap_or_default(),
        h2: Vec::new(),
        page_text: Some(strip_tags(main)).filter(|t| !t.is_empty()),
        html_lang: between(html, "lang=\"", "\"").map(str::to_string),
        ..SubjectData::default()
    }
}

/// Die 25 Seiten-URLs von `tests/fixtures/ai/<site>/` (G), Kennungen 1 … 25 in Manifest-Reihenfolge.
pub fn site_items(site: &str) -> Vec<RunItem> {
    let root = repo_root().join("tests/fixtures/ai").join(site);
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
            .unwrap();
    manifest["pages"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let url = format!("http://ai.test:8080{}", p["url"].as_str().unwrap());
            let html = std::fs::read_to_string(root.join(p["file"].as_str().unwrap())).unwrap();
            RunItem {
                url_id: i as i64 + 1,
                is_image: false,
                content_class: "html".into(),
                status_code: p["status"].as_u64().map(|s| s as u16),
                indexable: p["indexable"].as_bool().unwrap(),
                segments: Vec::new(),
                data: page_data(&url, &html),
                params: BTreeMap::new(),
            }
        })
        .collect()
}

/// Seite mit freiem Text (eigene Fixtures dieses Pakets).
pub fn text_item(url_id: i64, text: &str) -> RunItem {
    RunItem {
        url_id,
        is_image: false,
        content_class: "html".into(),
        status_code: Some(200),
        indexable: true,
        segments: Vec::new(),
        data: SubjectData {
            url: format!("http://ai.test:8080/d01/{url_id}"),
            title: Some(format!("Seite {url_id}")),
            h1: vec![format!("Seite {url_id}")],
            page_text: Some(text.to_string()),
            ..SubjectData::default()
        },
        params: BTreeMap::new(),
    }
}

/// Alles, was ein Lauf an Umgebung braucht (besitzt Adapter, Limiter, Ledger usw.).
pub struct Env {
    pub mock: Mock,
    pub decisions: OpenAiDecisions,
    pub responses: OpenAiResponses,
    pub anthropic: AnthropicAdapter,
    pub openrouter: OpenRouterAdapter,
    pub ollama: OllamaAdapter,
    pub limiter: Limiter,
    pub consent: ConsentGate,
    pub redactor: Redactor,
    pub defaults: TaskDefaults,
    pub settings: BTreeMap<TaskType, TaskSettings>,
    pub providers: BTreeMap<String, ProviderState>,
    pub policy: RoutingPolicy,
}

impl Env {
    pub async fn new(keys: &[&str]) -> Self {
        let mock = Mock::start().await;
        Self {
            decisions: mock.decisions(),
            responses: mock.responses(),
            anthropic: mock.anthropic(),
            openrouter: mock.openrouter(),
            ollama: mock.ollama(),
            mock,
            limiter: limiter(),
            consent: ConsentGate::cli(true),
            redactor: Redactor::new(RedactionSettings::off()),
            defaults: defaults(),
            settings: BTreeMap::new(),
            providers: providers(keys, Vec::new()),
            policy: RoutingPolicy::default(),
        }
    }

    /// Adapter nur der verbundenen Anbieter.
    pub fn adapters(&self) -> Adapters<'_> {
        let has = |p: &str| self.providers.get(p).is_some_and(|s| s.has_key);
        Adapters {
            decisions: has("openai").then_some(&self.decisions),
            responses: has("openai").then_some(&self.responses),
            anthropic: has("anthropic").then_some(&self.anthropic),
            openrouter: has("openrouter").then_some(&self.openrouter),
            ollama: has("ollama").then_some(&self.ollama),
        }
    }

    pub fn routing(&self) -> RoutingContext<'_> {
        RoutingContext {
            defaults: &self.defaults,
            settings: &self.settings,
            providers: &self.providers,
            policy: self.policy,
        }
    }

    /// Ein Lauf mit Standardoptionen.
    #[allow(clippy::too_many_arguments)]
    pub fn run<'a>(
        &'a self,
        conn: &'a os_store::ai::cache::Connection,
        run_id: i64,
        questions: &'a [ActiveQuestion],
        items: &'a [RunItem],
        ledger: &'a CrawlBudgetLedger,
        control: RunControl,
        options: RunOptions,
    ) -> DecisionRun<'a> {
        DecisionRun {
            conn,
            run_id,
            questions,
            items,
            routing: self.routing(),
            adapters: self.adapters(),
            limiter: &self.limiter,
            ledger,
            consent: &self.consent,
            switch: PreviewSwitch::ON,
            redactor: &self.redactor,
            prices: &TestPrices,
            control,
            options,
            now: &now,
            owner: None,
            before_commit: None,
        }
    }
}

/// Ledger der Crawl-Datei über `os_store::ai::budget::open_ledger` (Plan M4, Nachträge Welle 2 Punkt 2).
pub fn ledger(conn: &os_store::ai::cache::Connection, budget_usd_micro: u64) -> CrawlBudgetLedger {
    os_store::ai::budget::open_ledger(
        conn,
        BudgetLimits {
            crawl: MicroUsd(budget_usd_micro),
            ..BudgetLimits::default()
        },
        &[],
    )
    .unwrap()
}

/// Statuszählung einer Frage.
pub fn counts(conn: &os_store::ai::cache::Connection, q: &QuestionDef) -> BTreeMap<String, u64> {
    os_store::ai::answers::status_counts(conn, &q.id, &q.version.to_string()).unwrap()
}

pub fn count(map: &BTreeMap<String, u64>, status: &str) -> u64 {
    map.get(status).copied().unwrap_or(0)
}

/// Protokollzeilen (`ai_requests`) mit gesendetem Request.
pub fn logged(
    conn: &os_store::ai::cache::Connection,
) -> Vec<os_store::ai::requests::LoggedRequest> {
    os_store::ai::requests::list(conn, &os_store::ai::requests::RequestFilter::default())
        .unwrap()
        .into_iter()
        .filter(|r| !r.record.cache_hit)
        .collect()
}

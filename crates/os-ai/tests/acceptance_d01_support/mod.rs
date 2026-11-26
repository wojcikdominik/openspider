//! Hilfen der unabhängigen Abnahmetests M4-D01 (Decision-Engine-Kern, AI-030 … AI-035 und AI-020 Nachtrag
//! 26.11.2026). Erwartungswerte stehen in den Tests selbst und kommen nur aus `docs/spec/08-ai.md`.
//!
//! - ai-mock mit eigenem Antwortfragment `tests/fixtures/ai/d01-acceptance/mock-answers.d/` (08 Konventionen:
//!   „Deterministische Antworten“), Steuerung über `/__mock/scenario` und `/__mock/release` (Szenario `hold`).
//! - Ein Prüfstand ([`Bench`]) mit Crawl-Datei, Adaptern gegen den Mock, Begrenzung, Ledger, Einwilligung und
//!   Routing-Kontext, der einen KI-Lauf über die öffentliche Schnittstelle `DecisionRun` startet.
//!
//! Alle Verbindungen gehen an `127.0.0.1`; kein echter Anbieter, nur erfundene Schlüssel.
// acceptance: independent
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use os_ai::adapters::anthropic::{AnthropicAdapter, AnthropicConfig, AnthropicModel};
use os_ai::adapters::ollama::{OllamaAdapter, OllamaConfig};
use os_ai::adapters::openai_decisions::OpenAiDecisions;
use os_ai::adapters::openai_responses::{OpenAiAccount, OpenAiConfig, OpenAiHttp, OpenAiResponses};
use os_ai::adapters::openrouter::{OpenRouterAdapter, OpenRouterConfig, OpenRouterModel};
use os_ai::catalog::reasoning::{
    anthropic_levels, anthropic_output_limit, openrouter_levels, openrouter_output_limit,
};
use os_ai::catalog::{
    AvailableModel, Endpoint, ModelRef, ProviderState, TaskDefaults, TaskSettings, TaskType,
};
use os_ai::cost::{PriceSource, Rates};
use os_ai::decisions::exec::{
    ActiveQuestion, Adapters, BeforeCommit, DecisionRun, RunControl, RunEstimate, RunItem,
    RunOptions, RunReport,
};
use os_ai::decisions::format::{self, QuestionDef};
use os_ai::decisions::input::SubjectData;
use os_ai::decisions::routing::{RoutingContext, RoutingPolicy, RoutingSetting};
use os_ai::limits::{BackoffPolicy, Limiter, LimitsConfig, ProviderLimits, TokioClock};
use os_ai::policy::{ConsentGate, RedactionSettings, Redactor};
use os_ai::provider::{Capabilities, CapabilitySource, Model};
use os_ai::PreviewSwitch;
use os_model::budget::{BudgetLedger, BudgetLimits, CrawlBudgetLedger, MicroUsd};
use os_store::ai::answers::{self, AnswerRow};
use os_store::ai::requests::{self, LoggedRequest, RequestFilter};
use os_testsite::mocks::ai::{AiMock, RequestRecord};
use os_testsite::mocks::MockServer;
use serde_json::{json, Value};

/// Erfundene Schlüssel (keine echten Schlüssel).
pub const ANTHROPIC_KEY: &str = "sk-ant-TEST-D01-ACCEPTANCE";
pub const OPENAI_KEY: &str = "sk-TEST-D01-ACCEPTANCE";
pub const OPENROUTER_KEY: &str = "sk-or-TEST-D01-ACCEPTANCE";

/// Pfade des ai-mock (08 Konventionen).
pub const MESSAGES: &str = "/anthropic/v1/messages";
pub const RESPONSES: &str = "/openai/v1/responses";
pub const DECISIONS: &str = "/openai/v1/decisions";
pub const COMPLETIONS: &str = "/openrouter/api/v1/chat/completions";
pub const OLLAMA_CHAT: &str = "/ollama/api/chat";

/// Werte der Seitentyp-Frage (14 Werte wie die Mock-Antwort mit 14 `probabilities`, AI-033).
pub const PAGE_TYPE_VALUES: [&str; 14] = [
    "product",
    "homepage",
    "category",
    "article",
    "landing",
    "local",
    "contact",
    "about",
    "legal",
    "search-results",
    "account",
    "media",
    "error",
    "other",
];

/// Statuswerte laut AI-020 einschließlich der Nachträge; Platzhalterzeilen über ihr Präfix.
pub const SPEC_STATUS: [&str; 23] = [
    "Success",
    "Success (cached)",
    "Refusal",
    "Pending",
    "Pending (batch)",
    "Pending (confirmation)",
    "Pending (consent)",
    "Skipped (scope)",
    "Skipped (segment)",
    "Skipped (empty input)",
    "Skipped (policy)",
    "Skipped (no capable model)",
    "Skipped (image too large)",
    "Skipped (unsupported format)",
    "Skipped (no declared language)",
    "Budget exceeded",
    "Quota exceeded",
    "Error: Missing API key",
    "Error: Invalid output",
    "Error: Missing answer",
    "Error: Batch expired",
    "Error: Connection failed",
    "Error: Output limit reached",
];

/// Ob ein Statustext in der Tabelle von AI-020 steht (`Error: Timeout nach <n> s`, `Error: <HTTP-Code> <Meldung>`
/// über ihre Form).
pub fn is_spec_status(status: &str) -> bool {
    if SPEC_STATUS.contains(&status) {
        return true;
    }
    if let Some(rest) = status.strip_prefix("Error: Timeout nach ") {
        return rest.ends_with(" s");
    }
    if let Some(rest) = status.strip_prefix("Error: ") {
        let code: String = rest.chars().take(3).collect();
        return code.len() == 3 && code.chars().all(|c| c.is_ascii_digit());
    }
    false
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Eigene Fixtures unter `tests/fixtures/ai/d01-acceptance/`.
pub fn fixture_path(name: &str) -> PathBuf {
    repo_root()
        .join("tests/fixtures/ai/d01-acceptance")
        .join(name)
}

/// HTTP-Client ohne System-Proxy.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("HTTP-Client")
}

/// Wartet (höchstens 20 s, Abbruchschutz, keine Zeitlogik) auf eine Bedingung.
pub async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..4000 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("Bedingung nicht erreicht: {what}");
}

// ---------------------------------------------------------------------------------------------------------------
// ai-mock

/// Laufender ai-mock mit dem eigenen Antwortfragment.
pub struct Mock {
    pub mock: AiMock,
    pub server: MockServer,
}

impl Mock {
    pub async fn start() -> Self {
        let mock = AiMock::from_fragments(fixture_path("mock-answers.d"))
            .expect("eigene ai-mock-Fragmente lesbar");
        let server = MockServer::start(mock.registrations())
            .await
            .expect("ai-mock startet");
        Self { mock, server }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.server.addr())
    }

    async fn control(&self, path: &str, body: Value) -> Value {
        let response = client()
            .post(self.url(path))
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("Steuerung des Mocks erreichbar");
        assert!(
            response.status().is_success(),
            "Steuerung {path} abgelehnt: {}",
            response.status()
        );
        response.json().await.expect("JSON-Antwort der Steuerung")
    }

    /// Globales Szenario (`POST /__mock/scenario`) für die nächsten `count` Anfragen eines Anbieters.
    pub async fn scenario(&self, provider: &str, scenario: &str, count: usize) {
        self.control(
            "/__mock/scenario",
            json!({"provider": provider, "scenario": scenario, "count": count}),
        )
        .await;
    }

    /// Szenario `hold` für die nächsten `count` Requests eines Anbieters.
    pub async fn hold(&self, provider: &str, count: usize) {
        self.scenario(provider, "hold", count).await;
    }

    /// `POST /__mock/release`; Rückgabe: `released`.
    pub async fn release(&self, provider: &str, count: Option<usize>) -> u64 {
        let body = match count {
            Some(c) => json!({"provider": provider, "count": c}),
            None => json!({"provider": provider}),
        };
        self.control("/__mock/release", body).await["released"]
            .as_u64()
            .expect("released")
    }

    /// Alle Anbieter-Requests (ohne Steueraufrufe).
    pub fn log(&self) -> Vec<RequestRecord> {
        self.mock
            .requests()
            .into_iter()
            .filter(|r| !r.path.starts_with("/__mock"))
            .collect()
    }

    /// POST-Requests an `path` (ohne Query).
    pub fn posts(&self, path: &str) -> Vec<RequestRecord> {
        self.log()
            .into_iter()
            .filter(|r| r.method == "POST" && r.path.split('?').next() == Some(path))
            .collect()
    }

    /// JSON-Körper aller POST-Requests an `path`.
    pub fn bodies(&self, path: &str) -> Vec<Value> {
        self.posts(path)
            .into_iter()
            .map(|r| serde_json::from_str(&r.body).expect("JSON-Körper"))
            .collect()
    }

    /// Zahl der POST-Requests an alle Anbieter-Endpunkte der Decision-Fragen.
    pub fn provider_posts(&self) -> usize {
        [DECISIONS, RESPONSES, MESSAGES, COMPLETIONS, OLLAMA_CHAT]
            .iter()
            .map(|p| self.posts(p).len())
            .sum()
    }

    /// Zahl der gehaltenen Requests an `path` (bleiben im Protokoll mit `held: true`).
    pub fn held(&self, path: &str) -> usize {
        self.posts(path).iter().filter(|r| r.held).count()
    }

    pub async fn wait_held(&self, path: &str, n: usize) {
        wait_for(&format!("{n} gehaltene Requests an {path}"), || {
            self.held(path) >= n
        })
        .await;
    }

    pub async fn get_json(&self, path: &str) -> Value {
        client()
            .get(self.url(path))
            .send()
            .await
            .expect("Mock erreichbar")
            .json()
            .await
            .expect("JSON")
    }
}

/// Fragen eines Decisions-Requests (`questions`).
pub fn decision_questions(body: &Value) -> Vec<Value> {
    body["questions"].as_array().cloned().unwrap_or_default()
}

/// Namen der Fragen eines Decisions-Requests.
pub fn decision_names(body: &Value) -> Vec<String> {
    decision_questions(body)
        .iter()
        .map(|q| q["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// Alle Texte unter `input` eines Decisions-Requests (Text oder Nachrichten mit `input_text`-Teilen).
pub fn decision_input_text(body: &Value) -> String {
    fn collect(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Array(items) => items.iter().for_each(|i| collect(i, out)),
            Value::Object(map) => {
                if map.get("type").and_then(Value::as_str) == Some("input_image") {
                    return;
                }
                for key in ["text", "content"] {
                    if let Some(v) = map.get(key) {
                        collect(v, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    collect(&body["input"], &mut out);
    out.concat()
}

/// Zahl der Bildteile (`input_image`) eines Requests.
pub fn image_parts(body: &Value) -> usize {
    match body {
        Value::Object(map) => {
            let own = usize::from(map.get("type").and_then(Value::as_str) == Some("input_image"));
            own + map.values().map(image_parts).sum::<usize>()
        }
        Value::Array(items) => items.iter().map(image_parts).sum(),
        _ => 0,
    }
}

/// Fragentext Z_q nach AI-012 (Nachtrag): `name` + `instructions` + je `choices`-Eintrag `value` und `description` +
/// je `levels`-Eintrag `label` und `description` (Zeichen).
pub fn question_text_chars(q: &Value) -> u64 {
    let chars = |v: &Value| v.as_str().map(|s| s.chars().count() as u64).unwrap_or(0);
    let mut n = chars(&q["name"]) + chars(&q["instructions"]);
    for c in q["choices"].as_array().into_iter().flatten() {
        n += chars(&c["value"]) + chars(&c["description"]);
    }
    for l in q["levels"].as_array().into_iter().flatten() {
        n += chars(&l["label"]) + chars(&l["description"]);
    }
    n
}

/// ⌈x / 3,5⌉ = ⌈2x / 7⌉.
pub fn ceil_35(x: u64) -> u64 {
    (2 * x).div_ceil(7)
}

/// Unkalibrierte Schätzung nach AI-012 (Nachtrag) für einen gesendeten Decisions-Request ohne Bild:
/// Σ_q [⌈Z_E / 3,5⌉ + ⌈Z_q / 3,5⌉ + 100].
pub fn spec_decisions_estimate(body: &Value) -> u64 {
    let z_e = decision_input_text(body).chars().count() as u64;
    decision_questions(body)
        .iter()
        .map(|q| ceil_35(z_e) + ceil_35(question_text_chars(q)) + 100)
        .sum()
}

/// Minimaler Base64-Dekoder (Standardalphabet, mit oder ohne Auffüllung), damit gesendete Bilder geprüft werden
/// können, ohne neue Abhängigkeit.
pub fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for c in text.bytes() {
        let Some(v) = value(c) else { continue };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    out
}

/// 1 × 1 Pixel PNG.
pub const PNG_1X1: [u8; 70] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0,
    31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 100, 96, 248, 95, 15, 0, 2, 135, 1, 128,
    235, 71, 186, 146, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// PNG-Kopf (Signatur und IHDR mit gültiger Prüfsumme) eines Bildes mit 3.000 × 2.000 px.
pub const PNG_HEAD_3000X2000: [u8; 33] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 11, 184, 0, 0, 7, 208, 8, 2, 0, 0,
    0, 221, 112, 133, 109,
];

/// Verschiedene gültige Bilddaten (1 × 1 PNG plus Unterscheidungsbytes nach IEND).
pub fn png_variant(tag: u8) -> Vec<u8> {
    let mut v = PNG_1X1.to_vec();
    v.extend_from_slice(&[0, tag]);
    v
}

// ---------------------------------------------------------------------------------------------------------------
// Fragen

/// Frage aus JSON (Prüfung wie beim Import).
pub fn question(value: Value) -> QuestionDef {
    format::parse_value(&value).unwrap_or_else(|e| panic!("Testfrage ungültig: {e:?}\n{value}"))
}

pub fn inputs(fields: &[&str]) -> Value {
    json!(fields)
}

pub fn predicate(id: &str, name: &str, fields: &[&str]) -> QuestionDef {
    question(json!({
        "id": id,
        "version": "1.0.0",
        "name": name,
        "type": "predicate",
        "instructions": format!("Observable criterion for {name}."),
        "subject": "page",
        "inputs": inputs(fields),
    }))
}

pub fn choice(id: &str, name: &str, values: &[&str], fields: &[&str]) -> QuestionDef {
    let choices: Vec<Value> = values
        .iter()
        .map(|v| json!({"value": v, "description": format!("The page is {v}.")}))
        .collect();
    question(json!({
        "id": id,
        "version": "1.0.0",
        "name": name,
        "type": "choice",
        "instructions": format!("Choose for {name}."),
        "choices": choices,
        "subject": "page",
        "inputs": inputs(fields),
    }))
}

pub fn score(id: &str, name: &str, labels: &[&str], fields: &[&str]) -> QuestionDef {
    let levels: Vec<Value> = labels
        .iter()
        .map(|l| json!({"label": l, "description": format!("Level {l}.")}))
        .collect();
    question(json!({
        "id": id,
        "version": "1.0.0",
        "name": name,
        "type": "score",
        "instructions": format!("Rate {name}."),
        "levels": levels,
        "subject": "page",
        "inputs": inputs(fields),
    }))
}

/// Seitentyp-Frage (`id` = `page-type`, 14 Werte).
pub fn page_type(fields: &[&str]) -> QuestionDef {
    choice("page-type", "Page Type", &PAGE_TYPE_VALUES, fields)
}

pub fn active(def: QuestionDef) -> ActiveQuestion {
    ActiveQuestion {
        def,
        routing: RoutingSetting::Automatic,
    }
}

pub fn active_with(def: QuestionDef, routing: RoutingSetting) -> ActiveQuestion {
    ActiveQuestion { def, routing }
}

/// „immer strukturierte Ausgabe mit Modell X“ (AI-035).
pub fn structured(provider: &str, model: &str, endpoint: Endpoint) -> RoutingSetting {
    RoutingSetting::StructuredOutput {
        model: ModelRef::new(provider, model, endpoint),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Einträge (Grundmenge G)

pub fn url_of(id: i64) -> String {
    format!("https://ai.test:8080/d01t/page-{id}")
}

/// Seiten-URL im Standard-Geltungsbereich S (intern, HTML, 200, indexierbar).
pub fn page(url_id: i64, title: &str, text: &str) -> RunItem {
    RunItem {
        url_id,
        is_image: false,
        content_class: "html".into(),
        status_code: Some(200),
        indexable: true,
        segments: Vec::new(),
        data: SubjectData {
            url: url_of(url_id),
            title: Some(title.into()),
            page_text: Some(text.into()),
            ..SubjectData::default()
        },
        params: BTreeMap::new(),
    }
}

/// Bild (intern, 200) mit Alt-Text und Bildkontext.
pub fn image(url_id: i64, bytes: Vec<u8>, alt: &str, context: &str) -> RunItem {
    RunItem {
        url_id,
        is_image: true,
        content_class: "image".into(),
        status_code: Some(200),
        indexable: true,
        segments: Vec::new(),
        data: SubjectData {
            url: format!("https://ai.test:8080/d01t/img-{url_id}.png"),
            image: Some(os_ai::decisions::input::ImageData {
                media_type: Some("image/png".into()),
                bytes,
            }),
            alt: Some(alt.into()),
            image_context: Some(context.into()),
            ..SubjectData::default()
        },
        params: BTreeMap::new(),
    }
}

/// Fixture-Site nach 08 Konventionen nachgebildet: 22 Seiten in S und je eine `noindex`-, 404- und 301-URL
/// (|G| = 25, |S| = 22). `marker(i)` liefert den Seitentext von Seite i (1 … 22).
pub fn site(marker: impl Fn(i64) -> String) -> Vec<RunItem> {
    let mut items: Vec<RunItem> = (1..=22)
        .map(|i| page(i, &format!("Fixture page {i}"), &marker(i)))
        .collect();
    let mut noindex = page(23, "Noindex page", "Hidden content. MARK-NOINDEX");
    noindex.indexable = false;
    let mut missing = page(24, "Not found", "Error page. MARK-404");
    missing.status_code = Some(404);
    let mut redirect = page(25, "", "");
    redirect.status_code = Some(301);
    redirect.content_class = String::new();
    redirect.data.title = None;
    redirect.data.page_text = None;
    items.extend([noindex, missing, redirect]);
    items
}

// ---------------------------------------------------------------------------------------------------------------
// Preise, Zeit

/// Preise nach `tests/fixtures/ai/pricing-test.json` (08 Konventionen) bzw. Mock-Modellliste (OpenRouter).
pub fn test_prices(provider: &str, model: &str, endpoint: &str) -> Option<Rates> {
    match (provider, model, endpoint) {
        ("openai", "gpt-6-luna", "decisions") => {
            Rates::per_million("0.10", "0", PriceSource::Table)
        }
        ("openai", "gpt-6-luna", "responses") => {
            Rates::per_million("0.10", "0.50", PriceSource::Table)
        }
        ("anthropic", _, "messages") => Rates::per_million("1", "5", PriceSource::Table),
        ("openrouter", _, _) => Rates::per_million("1", "5", PriceSource::Table),
        ("ollama", _, _) => Some(Rates::free(PriceSource::Table)),
        _ => None,
    }
}

pub fn fixed_now() -> os_model::Timestamp {
    os_model::Timestamp::from_db("2026-11-26T10:05:00.000Z").unwrap()
}

/// Uhr als statischer Funktionszeiger (Lebensdauer des Laufs).
pub static NOW: fn() -> os_model::Timestamp = fixed_now;

/// Preisgrundlage ohne Preis für die Decisions API (alle übrigen wie [`test_prices`]).
pub fn prices_without_decisions(provider: &str, model: &str, endpoint: &str) -> Option<Rates> {
    if endpoint == "decisions" {
        return None;
    }
    test_prices(provider, model, endpoint)
}

/// Begrenzung mit kurzen Wartezeiten (INTEG-005-Regeln und Schutzschalter unverändert, nur die Backoff-Basis ist
/// verkürzt) und `concurrent` gleichzeitigen Anfragen je Anbieter.
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
        timeout: Duration::from_secs(30),
    };
    config.default_provider = limits;
    for value in config.providers.values_mut() {
        *value = limits;
    }
    for provider in ["openai", "anthropic", "openrouter", "ollama"] {
        config.providers.insert(provider.into(), limits);
    }
    Limiter::new(config, Arc::new(TokioClock::new()))
}

// ---------------------------------------------------------------------------------------------------------------
// Prüfstand

/// OpenAI-Katalogmodell `gpt-6-luna` auf dem Decisions-Endpunkt.
pub fn decisions_model() -> AvailableModel {
    AvailableModel {
        model: Model {
            provider: "openai".into(),
            id: "gpt-6-luna".into(),
            endpoint: "decisions".into(),
            capabilities: Capabilities {
                text: true,
                vision: true,
                decisions: true,
                ..Capabilities::default()
            },
            capability_source: CapabilitySource::ModelCatalog,
            reasoning_levels: Vec::new(),
        },
        max_output_tokens: None,
    }
}

pub fn ollama_model() -> AvailableModel {
    AvailableModel {
        model: Model {
            provider: "ollama".into(),
            id: "mock-llama".into(),
            endpoint: "chat-completions".into(),
            capabilities: Capabilities {
                text: true,
                structured_output: true,
                local: true,
                ..Capabilities::default()
            },
            capability_source: CapabilitySource::ModelCatalog,
            reasoning_levels: Vec::new(),
        },
        max_output_tokens: None,
    }
}

/// Crawl-Datei, Mock, Adapter und alles, was ein KI-Lauf braucht.
pub struct Bench {
    pub mock: Mock,
    pub path: PathBuf,
    file: Option<os_store::CrawlFile>,
    pub keys: Vec<String>,
    pub decisions: OpenAiDecisions,
    pub responses: OpenAiResponses,
    pub anthropic: AnthropicAdapter,
    pub openrouter: OpenRouterAdapter,
    pub ollama: OllamaAdapter,
    pub limiter: Limiter,
    pub limits: BudgetLimits,
    pub ledger: CrawlBudgetLedger,
    pub consent: ConsentGate,
    pub switch: PreviewSwitch,
    pub redactor: Redactor,
    pub defaults: TaskDefaults,
    pub settings: BTreeMap<TaskType, TaskSettings>,
    pub providers: BTreeMap<String, ProviderState>,
    pub policy: RoutingPolicy,
    /// Preisgrundlage (Standard: [`test_prices`]).
    pub prices: fn(&str, &str, &str) -> Option<Rates>,
    anthropic_raw: Vec<Value>,
    openrouter_raw: Vec<Value>,
    /// Zuletzt aufgeräumt (nach der Crawl-Datei).
    _dir: tempdir::Dir,
}

mod tempdir {
    use std::path::PathBuf;
    /// Temporäres Verzeichnis ohne zusätzliche Abhängigkeit; wird beim Drop gelöscht.
    pub struct Dir(pub PathBuf);
    impl Dir {
        pub fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = std::env::temp_dir().join(format!(
                "os-ai-d01-acc-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn new_crawl_file(path: &Path) -> os_store::CrawlFile {
    use os_model::time::{IdSource, SeededIds};
    let created = os_model::Timestamp::from_db("2026-11-26T10:00:00.000Z").unwrap();
    let init = os_store::CrawlInit {
        crawl_id: SeededIds::new(1001).new_crawl_id(created),
        mode: os_model::CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "Abnahme".into(),
        name: "D01".into(),
        config: os_model::CrawlConfig::default(),
        created_at: created,
    };
    os_store::CrawlFile::create(path, &init, os_store::StorageProfile::Balanced)
        .expect("Crawl-Datei anlegbar")
}

impl Bench {
    /// Prüfstand mit Schlüsseln für `keys` (`openai`, `anthropic`, `openrouter`, `ollama`); `concurrent` gleichzeitige
    /// Anfragen je Anbieter (Limiter und Lauf).
    pub async fn new(keys: &[&str], concurrent: u32) -> Self {
        let mock = Mock::start().await;
        let dir = tempdir::Dir::new("bench");
        let path = dir.0.join("d01.ospider");
        let file = new_crawl_file(&path);
        let openai_http = || {
            OpenAiHttp::new(
                client(),
                OpenAiConfig::new(os_ai::adapters::openai_responses::ApiKey::new(OPENAI_KEY))
                    .with_base_url(mock.url("/openai/v1")),
                OpenAiAccount::new(),
            )
        };
        let decisions = OpenAiDecisions::new(openai_http());
        let responses = OpenAiResponses::new(openai_http());
        let anthropic = AnthropicAdapter::new(
            client(),
            AnthropicConfig::new(os_ai::adapters::anthropic::ApiKey::new(ANTHROPIC_KEY))
                .with_base_url(mock.url("/anthropic")),
        );
        let openrouter = OpenRouterAdapter::new(
            client(),
            OpenRouterConfig::new(os_ai::adapters::openrouter::ApiKey::new(OPENROUTER_KEY))
                .with_base_url(mock.url("/openrouter/api/v1")),
        );
        let ollama = OllamaAdapter::new(
            client(),
            OllamaConfig::new().with_base_url(mock.url("/ollama")),
        );
        // Modelllisten laden (Fähigkeiten, Reasoning-Stufen, Ausgabegrenzen; Adapter-Cache), bevor ein Test `hold`
        // setzt.
        anthropic.list_models().await.expect("Anthropic-Modellliste");
        openrouter.list_models().await.expect("OpenRouter-Modellliste");
        let anthropic_raw = mock.get_json("/anthropic/v1/models").await["data"]
            .as_array()
            .cloned()
            .expect("data");
        let openrouter_raw = mock.get_json("/openrouter/api/v1/models").await["data"]
            .as_array()
            .cloned()
            .expect("data");
        let mut defaults = TaskDefaults::bundled().expect("task-defaults.json");
        for task in [TaskType::Decision, TaskType::DecisionVision] {
            if let Some(d) = defaults.tasks.get_mut(&task) {
                d.fallback = vec![
                    ModelRef::new("anthropic", "mock-claude", Endpoint::Messages),
                    ModelRef::new("openrouter", "mock/text-small", Endpoint::ChatCompletions),
                ];
            }
        }
        let limits = BudgetLimits::default();
        let ledger = os_store::ai::budget::open_ledger(file.conn(), limits.clone(), &[])
            .expect("Ledger");
        let file = Some(file);
        let mut bench = Self {
            mock,
            _dir: dir,
            path,
            file,
            keys: keys.iter().map(|k| k.to_string()).collect(),
            decisions,
            responses,
            anthropic,
            openrouter,
            ollama,
            limiter: limiter(concurrent),
            limits,
            ledger,
            consent: ConsentGate::cli(true),
            switch: PreviewSwitch::ON,
            redactor: Redactor::new(RedactionSettings::off()),
            defaults,
            settings: BTreeMap::new(),
            providers: BTreeMap::new(),
            policy: RoutingPolicy::default(),
            prices: test_prices,
            anthropic_raw,
            openrouter_raw,
        };
        bench.set_keys(keys);
        bench
    }

    /// Setzt die verbundenen Anbieter (Schlüssel vorhanden, Live-Modellliste geladen).
    pub fn set_keys(&mut self, keys: &[&str]) {
        self.keys = keys.iter().map(|k| k.to_string()).collect();
        let has = |p: &str| keys.contains(&p);
        let mut providers = BTreeMap::new();
        let openai = if has("openai") {
            ProviderState {
                has_key: true,
                models: vec![decisions_model()],
            }
        } else {
            ProviderState::default()
        };
        providers.insert("openai".to_string(), openai);
        let anthropic = if has("anthropic") {
            ProviderState {
                has_key: true,
                models: self
                    .anthropic_raw
                    .iter()
                    .map(|entry| {
                        let mut model = AnthropicModel::from_json(entry).unwrap().to_model();
                        model.reasoning_levels = anthropic_levels(entry);
                        AvailableModel {
                            model,
                            max_output_tokens: anthropic_output_limit(entry),
                        }
                    })
                    .collect(),
            }
        } else {
            ProviderState::default()
        };
        providers.insert("anthropic".to_string(), anthropic);
        let openrouter = if has("openrouter") {
            ProviderState {
                has_key: true,
                models: self
                    .openrouter_raw
                    .iter()
                    .map(|entry| {
                        let mut model = OpenRouterModel::from_json(entry).unwrap().to_model();
                        model.reasoning_levels = openrouter_levels(entry);
                        AvailableModel {
                            model,
                            max_output_tokens: openrouter_output_limit(entry),
                        }
                    })
                    .collect(),
            }
        } else {
            ProviderState::default()
        };
        providers.insert("openrouter".to_string(), openrouter);
        let ollama = if has("ollama") {
            ProviderState {
                has_key: true,
                models: vec![ollama_model()],
            }
        } else {
            ProviderState::default()
        };
        providers.insert("ollama".to_string(), ollama);
        self.providers = providers;
    }

    pub fn conn(&self) -> &os_store::ai::cache::Connection {
        self.file.as_ref().expect("Crawl-Datei offen").conn()
    }

    /// Budget setzen (Crawl-Budget in µUSD) und den Ledger neu öffnen.
    pub fn set_budget(&mut self, crawl_microusd: u64) {
        self.limits = BudgetLimits {
            crawl: MicroUsd(crawl_microusd),
            ..BudgetLimits::default()
        };
        self.reopen_ledger();
    }

    /// Ledger wie beim Öffnen der Datei aus `ai_requests` neu aufbauen (offene Reservierungen verfallen).
    pub fn reopen_ledger(&mut self) {
        self.ledger = os_store::ai::budget::open_ledger(self.conn(), self.limits.clone(), &[])
            .expect("Ledger");
    }

    /// Crawl-Datei schließen und neu öffnen (Neustart), Ledger neu aufbauen.
    pub fn reopen_file(&mut self) {
        drop(self.file.take());
        self.file = Some(
            os_store::CrawlFile::open(&self.path, os_store::OpenMode::ReadWrite)
                .expect("Crawl-Datei wieder öffnen"),
        );
        self.reopen_ledger();
    }

    /// Legt für jeden Eintrag eine Zeile in `urls` an (Fremdschlüssel von `ai_answers`/`ai_requests`).
    pub fn add_urls(&self, items: &[RunItem]) {
        let mut sql = String::new();
        for item in items {
            let url = item.data.url.replace('\'', "''");
            sql.push_str(&format!(
                "INSERT OR IGNORE INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length) \
                 VALUES({},'{url}','{url}','https://ai.test:8080',1,'Done','Seed',1,{});\n",
                item.url_id,
                url.chars().count()
            ));
        }
        self.conn().execute_batch(&sql).expect("URL-Zeilen");
    }

    fn adapters(&self) -> Adapters<'_> {
        let has = |p: &str| self.keys.iter().any(|k| k == p);
        Adapters {
            decisions: has("openai").then_some(&self.decisions),
            responses: has("openai").then_some(&self.responses),
            anthropic: has("anthropic").then_some(&self.anthropic),
            openrouter: has("openrouter").then_some(&self.openrouter),
            ollama: has("ollama").then_some(&self.ollama),
        }
    }

    fn decision_run<'a>(
        &'a self,
        questions: &'a [ActiveQuestion],
        items: &'a [RunItem],
        options: RunOptions,
        control: RunControl,
        hook: Option<BeforeCommit<'a>>,
    ) -> DecisionRun<'a> {
        self.add_urls(items);
        let run_id =
            answers::begin_run(self.conn(), "2026-11-26T10:00:00Z", None, "{}").expect("ai_runs");
        DecisionRun {
            conn: self.conn(),
            run_id,
            questions,
            items,
            routing: RoutingContext {
                defaults: &self.defaults,
                settings: &self.settings,
                providers: &self.providers,
                policy: self.policy,
            },
            adapters: self.adapters(),
            limiter: &self.limiter,
            ledger: &self.ledger,
            consent: &self.consent,
            switch: self.switch,
            redactor: &self.redactor,
            prices: &self.prices,
            control,
            options,
            now: &NOW,
            owner: None,
            before_commit: hook,
        }
    }

    /// Führt einen KI-Lauf aus.
    pub async fn run(
        &self,
        questions: &[ActiveQuestion],
        items: &[RunItem],
        options: RunOptions,
    ) -> RunReport {
        self.run_ctl(questions, items, options, RunControl::new(), None)
            .await
    }

    pub async fn run_ctl(
        &self,
        questions: &[ActiveQuestion],
        items: &[RunItem],
        options: RunOptions,
        control: RunControl,
        hook: Option<BeforeCommit<'_>>,
    ) -> RunReport {
        self.decision_run(questions, items, options, control, hook)
            .run()
            .await
            .expect("KI-Lauf ohne Speicherfehler")
    }

    /// Voranschlag eines Laufs (sendet nichts).
    pub fn estimate(
        &self,
        questions: &[ActiveQuestion],
        items: &[RunItem],
        options: RunOptions,
    ) -> RunEstimate {
        self.decision_run(questions, items, options, RunControl::new(), None)
            .estimate()
            .expect("Voranschlag")
    }

    /// Alle Antwortzeilen einer Frage (alle Versionen).
    pub fn answers(&self, question_id: &str) -> Vec<AnswerRow> {
        answers::list_for_question(self.conn(), question_id, None).expect("ai_answers")
    }

    pub fn answer(&self, url_id: i64, question_id: &str) -> AnswerRow {
        self.answers(question_id)
            .into_iter()
            .find(|r| r.url_id == url_id)
            .unwrap_or_else(|| panic!("keine Antwortzeile für URL {url_id}, Frage {question_id}"))
    }

    pub fn status(&self, url_id: i64, question_id: &str) -> String {
        self.answer(url_id, question_id).status
    }

    /// Zahl der Einträge je Status einer Frage.
    pub fn status_counts(&self, question_id: &str) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for row in self.answers(question_id) {
            *counts.entry(row.status).or_insert(0) += 1;
        }
        counts
    }

    pub fn count(&self, question_id: &str, status: &str) -> usize {
        self.status_counts(question_id)
            .get(status)
            .copied()
            .unwrap_or(0)
    }

    /// Alle Protokollzeilen (`ai_requests`).
    pub fn log_rows(&self) -> Vec<LoggedRequest> {
        requests::list(self.conn(), &RequestFilter::default()).expect("ai_requests")
    }

    /// Protokollzeilen mit gesendetem Request (ohne Cache-Treffer).
    pub fn sent_rows(&self) -> Vec<LoggedRequest> {
        self.log_rows()
            .into_iter()
            .filter(|r| !r.record.cache_hit)
            .collect()
    }

    /// Summe `Cost (USD)` aller Protokollzeilen in µUSD.
    pub fn logged_cost(&self) -> u64 {
        self.log_rows().iter().map(|r| r.record.cost.0).sum()
    }

    pub fn cache_rows(&self) -> i64 {
        self.conn()
            .query_row("SELECT COUNT(*) FROM ai_cache", [], |r| r.get(0))
            .expect("ai_cache")
    }

    pub fn reserved(&self) -> u64 {
        self.ledger.snapshot().expect("Ledger-Stand").reserved.0
    }

    pub fn spent(&self) -> u64 {
        self.ledger.snapshot().expect("Ledger-Stand").spent.0
    }
}

/// Optionen mit Standardwerten (Decisions bis 10 Fragen je Request, Fallback-Bündelung an, 1 Request gleichzeitig).
pub fn options() -> RunOptions {
    RunOptions::default()
}

pub fn options_concurrent(n: usize) -> RunOptions {
    RunOptions {
        concurrency: n,
        ..RunOptions::default()
    }
}

/// `answer_json` als JSON.
pub fn answer_json(row: &AnswerRow) -> Value {
    serde_json::from_str(row.answer_json.as_deref().expect("Ergebnis vorhanden")).expect("JSON")
}

/// Höchste Einzelwahrscheinlichkeit einer Antwort (Liste mit `probability` oder Objekt).
pub fn max_probability(answer: &Value) -> f64 {
    let probs = &answer["probabilities"];
    let values: Vec<f64> = match probs {
        Value::Array(items) => items
            .iter()
            .filter_map(|p| p["probability"].as_f64())
            .collect(),
        Value::Object(map) => map.values().filter_map(Value::as_f64).collect(),
        _ => Vec::new(),
    };
    values.into_iter().fold(f64::MIN, f64::max)
}

/// Summe der Einzelwahrscheinlichkeiten.
pub fn probability_sum(answer: &Value) -> f64 {
    match &answer["probabilities"] {
        Value::Array(items) => items
            .iter()
            .filter_map(|p| p["probability"].as_f64())
            .sum(),
        Value::Object(map) => map.values().filter_map(Value::as_f64).sum(),
        _ => 0.0,
    }
}

pub fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

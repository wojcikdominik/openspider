//! Gemeinsame Hilfen der Tests von M4-D05 für den Laufrahmen `os_ai::run`: Umgebung gegen den ai-mock (Adapter je Lauf,
//! Testpreise, Einwilligung erteilt), Crawl-Datei mit den Seiten der Fixture-Site, Lesen der Besitzerangaben. Baut auf den
//! Hilfen von M4-D01 auf (`decisions_support.rs`, unverändert eingebunden). Als eigene Testdatei ohne Tests.
#![allow(dead_code)]

#[path = "decisions_support.rs"]
pub mod d01;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use os_ai::limits::{BackoffPolicy, LimitsConfig, ProviderLimits};
use os_ai::policy::{ConsentGate, RedactionSettings};
use os_ai::run::{DecisionsJob, JobConfirmation, ProviderSetup, RunEnv};
use os_ai::decisions::exec::Selection;
use os_ai::PreviewSwitch;
use os_model::ai::hook::{InputSource, PageAiFields};
use os_model::budget::MicroUsd;
use os_model::{CrawlConfig, SystemClock};
use serde_json::{json, Value};

pub use d01::{Mock, TempDir, DECISIONS};

/// Temporäres Verzeichnis mit laufender Nummer (gleichzeitige Tests desselben Programms kollidieren sonst bei gleicher
/// Nanosekunde).
pub fn tmp(tag: &str) -> TempDir {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    TempDir::new(&format!("{tag}-{}", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)))
}

/// Begrenzung wie in den D01-Tests: eine gleichzeitige Anfrage, kurze Backoff-Pausen, Timeout 20 s.
pub fn limits() -> LimitsConfig {
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
        timeout: Duration::from_secs(20),
    };
    config.default_provider = limits;
    for value in config.providers.values_mut() {
        *value = limits;
    }
    config
}

/// Anbieterzugang gegen den Mock (Schlüssel nur für `keys`).
pub fn providers(mock: &Mock, keys: &[&str]) -> ProviderSetup {
    let mut k = BTreeMap::new();
    for (p, key) in [
        ("openai", d01::OPENAI_KEY),
        ("anthropic", d01::ANTHROPIC_KEY),
        ("openrouter", d01::OPENROUTER_KEY),
    ] {
        if keys.contains(&p) {
            k.insert(p.to_string(), key.to_string());
        }
    }
    ProviderSetup {
        client: d01::client(),
        keys: k,
        base_urls: BTreeMap::from([
            ("openai".to_string(), mock.url("/openai/v1")),
            ("anthropic".to_string(), mock.url("/anthropic")),
            ("openrouter".to_string(), mock.url("/openrouter/api/v1")),
            ("ollama".to_string(), mock.url("/ollama")),
        ]),
        ollama: false,
    }
}

/// Umgebung eines Laufs gegen den Mock: verbundene Anbieter `keys`, Schwelle `threshold`, Einwilligung erteilt.
pub fn env(mock: &Mock, keys: &[&str], threshold: MicroUsd) -> RunEnv {
    RunEnv {
        providers: providers(mock, keys),
        provider_states: d01::providers(keys, Vec::new()),
        defaults: d01::defaults(),
        task_settings: BTreeMap::new(),
        policy: Default::default(),
        limits: limits(),
        prices: Arc::new(d01::TestPrices),
        consent: ConsentGate::cli(true),
        switch: PreviewSwitch::ON,
        redaction: RedactionSettings::off(),
        threshold,
        concurrency: 1,
        clock: Arc::new(SystemClock),
        owner_kind: "app",
    }
}

/// Schwelle, die nie greift.
pub const NO_CONFIRMATION: MicroUsd = MicroUsd(u64::MAX);

/// Frage aus den D01-Fixtures als Profileintrag (`ai.questions`).
pub fn question_json(name: &str) -> Value {
    serde_json::from_str(&d01::fixture_text(&format!(
        "questions/valid/{name}.question.json"
    )))
    .unwrap()
}

/// Frage aus `tests/fixtures/ai/run/questions/<name>.question.json` (eigene Fixtures von M4-D05).
pub fn run_question(name: &str) -> Value {
    let path = d01::repo_root()
        .join("tests/fixtures/ai/run/questions")
        .join(format!("{name}.question.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// Profil mit den Fragen `questions` (aktiv in dieser Reihenfolge) und Budget `budget` µUSD.
pub fn config(questions: &[Value], budget: u64) -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.ai.enabled = true;
    cfg.ai.questions = questions.to_vec();
    cfg.ai.active_questions = questions
        .iter()
        .map(|q| q["id"].as_str().unwrap().to_string())
        .collect();
    cfg.ai.budget.crawl = MicroUsd(budget);
    cfg
}

/// Eingabefelder der Seiten der Fixture-Site (Kennungen 1 … 25 wie `site_items`), als gespeicherte Eingaben.
pub fn site_pages(site: &str) -> Vec<PageAiFields> {
    d01::site_items(site)
        .into_iter()
        .map(|i| PageAiFields {
            url_id: i.url_id,
            url: i.data.url.clone(),
            content_class: i.content_class.clone(),
            status_code: i.status_code,
            indexable: i.indexable,
            title: i.data.title.clone(),
            meta_description: i.data.meta_description.clone(),
            h1: i.data.h1.clone(),
            h2: i.data.h2.clone(),
            page_text: i.data.page_text.clone(),
            word_count: i.data.word_count,
            html_lang: i.data.html_lang.clone(),
            hreflang_self: Vec::new(),
            structured_data_types: None,
            images: Vec::new(),
            source: InputSource::Stored,
        })
        .collect()
}

/// Crawl-Datei (`Completed`) mit den URLs der Seiten.
pub fn crawl_file(dir: &Path, pages: &[PageAiFields]) -> PathBuf {
    let items: Vec<os_ai::decisions::exec::RunItem> =
        pages.iter().map(os_ai::run::items::run_item).collect();
    let (file, _) = d01::crawl_file(dir, &items);
    // Die Laufzeile der D01-Hilfe ist für diese Tests ein abgeschlossener früherer Lauf.
    file.conn()
        .execute("UPDATE ai_runs SET ended_at = '2026-11-26T12:00:00Z'", [])
        .unwrap();
    drop(file);
    dir.join("d01.ospider")
}

/// Auftrag eines neuen Laufs.
pub fn job(path: &Path, cfg: &CrawlConfig, pages: &[PageAiFields], confirmation: JobConfirmation) -> DecisionsJob {
    DecisionsJob {
        crawl_path: path.to_path_buf(),
        config: cfg.clone(),
        questions: Vec::new(),
        pages: pages.to_vec(),
        confirmation,
        selection: Selection::Open,
    }
}

/// Besitzerangaben der Crawl-Datei (`owner_kind`, `owner_pid`, `state`).
pub fn owner(path: &Path) -> (String, String, String) {
    let f = os_store::CrawlFile::open(path, os_store::OpenMode::ReadOnly).unwrap();
    let get = |k: &str| f.get_meta(k).unwrap().unwrap_or_default();
    (get("owner_kind"), get("owner_pid"), get("state"))
}

/// Statuszählung einer Frage.
pub fn counts(path: &Path, id: &str) -> BTreeMap<String, u64> {
    let f = os_store::CrawlFile::open(path, os_store::OpenMode::ReadOnly).unwrap();
    os_store::ai::answers::status_counts(f.conn(), id, "1.0.0").unwrap()
}

/// Protokollzeilen mit gesendetem Request.
pub fn logged(path: &Path) -> usize {
    let f = os_store::CrawlFile::open(path, os_store::OpenMode::ReadOnly).unwrap();
    d01::logged(f.conn()).len()
}

/// Mock-Requests an die Decisions API.
pub fn decisions_posts(mock: &Mock) -> usize {
    mock.posts(DECISIONS).len()
}

/// Szenario für den Anbieter setzen.
pub async fn scenario(mock: &Mock, provider: &str, scenario: &str, count: usize) {
    mock.scenario(provider, scenario, count).await;
}

/// JSON-Hilfe für die Testausgabe.
pub fn pretty(v: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(&json!(v)).unwrap_or_default()
}

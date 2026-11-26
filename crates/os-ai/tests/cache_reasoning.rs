//! M4-K10: effektive Reasoning-Stufe im Cache-Schlüssel (AI-015, Nachtrag 24.11.2026, Q00-H1). Ein „Lauf“ ist eine
//! Schleife über Einträge mit `ResultCache::get_or_run`; bei einem Fehlversuch geht ein Request an ai-mock (Anthropic).
#[path = "cost_q00_support.rs"]
mod support;

use os_ai::cache::{
    input_hash, CacheKey, CacheKeyInput, CacheOptions, CachedRun, ExecutionPath, ResultCache,
};
use os_ai::provider::{Input, ReasoningLevel};
use os_ai::AiStatus;
use os_model::Timestamp;
use serde_json::{json, Value};
use support::{crawl_file, model, ok_schema, request, Mock, TempDir, MESSAGES};

const PAGES: [&str; 3] = ["home", "category-1", "category-2"];

fn key_input(page: &str, path: Option<ExecutionPath>, task: &str) -> CacheKeyInput {
    CacheKeyInput {
        task: task.into(),
        definition_id: task.into(),
        definition_version: "1".into(),
        params: json!({}),
        path,
        provider: "anthropic".into(),
        model: "mock-claude".into(),
        endpoint: "https://api.anthropic.com/v1".into(),
        result_params: json!({}),
        input_hash: input_hash(&[Input::Text {
            text: format!("MOCK-ID:{page}"),
        }]),
    }
}

fn now() -> Timestamp {
    Timestamp::from_db("2026-11-26T10:00:00.000Z").unwrap()
}

/// Ein Lauf von `suggest-title` mit Stufe `level` und Ausgabegrenze `max_tokens`; liefert die Zahl der Requests.
async fn run(
    mock: &Mock,
    cache: &ResultCache<'_>,
    level: ReasoningLevel,
    max_tokens: u64,
) -> usize {
    let adapter = mock.anthropic();
    let before = mock.requests_to(MESSAGES).len();
    for page in PAGES {
        // `max_tokens` gehört nicht zum Schlüssel; die Stufe schon.
        let key = CacheKey::with_reasoning(
            key_input(page, Some(ExecutionPath::StructuredOutput), "suggest-title"),
            level,
        );
        let req = request(
            model("anthropic", "mock-claude", "messages", Vec::new()),
            &format!("MOCK-ID:{page}"),
            Some(ok_schema()),
            level,
            max_tokens,
        );
        let ran = cache
            .get_or_run(&key, now(), || {
                let (adapter, req) = (&adapter, &req);
                async move {
                    let reply = adapter.send(req, None).await.unwrap();
                    let value = reply.into_response(Some(&ok_schema())).unwrap().content;
                    (AiStatus::Success, value.unwrap_or(Value::Null), ())
                }
            })
            .await
            .unwrap();
        if let CachedRun::Hit { status, .. } = ran {
            assert_eq!(status, AiStatus::SuccessCached);
        }
    }
    mock.requests_to(MESSAGES).len() - before
}

// spec: AI-015 [M4]
// `suggest-title` mit Stufe `off` gelaufen; Wechsel auf `low` → neue Requests für alle Einträge; Rückwechsel auf `off`
// → 0 Requests (beide Ergebnisse im Cache).
#[tokio::test]
async fn reasoning_level_change_requests_again_and_both_results_stay_cached() {
    let mock = Mock::start().await;
    let dir = TempDir::new("cache-level");
    let file = crawl_file(&dir.0);
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    assert_eq!(run(&mock, &cache, ReasoningLevel::Off, 1_024).await, 3);
    assert_eq!(run(&mock, &cache, ReasoningLevel::Off, 1_024).await, 0);
    assert_eq!(run(&mock, &cache, ReasoningLevel::Low, 2_048).await, 3);
    assert_eq!(run(&mock, &cache, ReasoningLevel::Off, 1_024).await, 0);
    assert_eq!(run(&mock, &cache, ReasoningLevel::Low, 2_048).await, 0);
}

// spec: AI-015 [M4]
// Änderung nur von `max_tokens` → 0 Requests.
#[tokio::test]
async fn max_tokens_is_not_part_of_the_key() {
    let mock = Mock::start().await;
    let dir = TempDir::new("cache-max");
    let file = crawl_file(&dir.0);
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    assert_eq!(run(&mock, &cache, ReasoningLevel::Off, 1_024).await, 3);
    assert_eq!(run(&mock, &cache, ReasoningLevel::Off, 4_000).await, 0);
}

// spec: AI-015 [M4]
// Seitentyp-Frage über die Decisions API → Schlüssel mit `reasoning = null`, unabhängig von der für `decision`
// eingestellten Stufe; ebenso Embeddings. Auf dem Ersatzweg zählt die Stufe.
#[test]
fn decisions_and_embedding_keys_have_no_reasoning() {
    let decisions = key_input("home", Some(ExecutionPath::DecisionsApi), "decision");
    let plain = CacheKey::new(decisions.clone());
    assert_eq!(plain.reasoning(), None);
    for level in [
        ReasoningLevel::Off,
        ReasoningLevel::High,
        ReasoningLevel::ModelDefault,
    ] {
        let key = CacheKey::with_reasoning(decisions.clone(), level);
        assert_eq!(key.reasoning(), None, "{level}");
        assert_eq!(key.as_str(), plain.as_str());
        assert!(!key.canonical().contains("reasoning"));
    }
    let embedding = key_input("home", None, "embedding");
    assert_eq!(
        CacheKey::with_reasoning(embedding.clone(), ReasoningLevel::Low).as_str(),
        CacheKey::new(embedding).as_str()
    );
    // Ersatzweg (strukturierte Ausgabe): Stufe im Schlüssel, je Stufe ein anderer Schlüssel.
    let fallback = key_input("home", Some(ExecutionPath::StructuredOutput), "decision");
    let off = CacheKey::with_reasoning(fallback.clone(), ReasoningLevel::Off);
    let model_default = CacheKey::with_reasoning(fallback.clone(), ReasoningLevel::ModelDefault);
    assert_eq!(off.reasoning(), Some(ReasoningLevel::Off));
    assert!(off.canonical().contains(r#""reasoning":"off""#));
    assert!(model_default
        .canonical()
        .contains(r#""reasoning":"model-default""#));
    assert_ne!(off.as_str(), model_default.as_str());
    assert_ne!(off.as_str(), CacheKey::new(fallback).as_str());
}

// spec: AI-015 [M4]
// spec: AI-020 [M4]
// Ein Eintrag mit `Error: Output limit reached` wird nicht gecacht und beim nächsten Lauf erneut angefragt.
#[tokio::test]
async fn output_limit_entries_are_never_cached() {
    let dir = TempDir::new("cache-limit");
    let file = crawl_file(&dir.0);
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let key = CacheKey::with_reasoning(
        key_input(
            "home",
            Some(ExecutionPath::StructuredOutput),
            "suggest-title",
        ),
        ReasoningLevel::Off,
    );
    let requests = std::cell::Cell::new(0);
    for _ in 0..2 {
        let ran = cache
            .get_or_run(&key, now(), || {
                requests.set(requests.get() + 1);
                async { (AiStatus::OutputLimitReached, Value::Null, ()) }
            })
            .await
            .unwrap();
        assert!(matches!(ran, CachedRun::Ran { stored: false, .. }));
    }
    assert_eq!(requests.get(), 2, "erneut angefragt");
    assert_eq!(cache.stats().unwrap().entries, 0);
}

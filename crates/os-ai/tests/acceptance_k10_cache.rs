//! Unabhängige Abnahmetests M4-K10: Ergebnis-Cache mit Reasoning-Stufe (AI-015, Nachtrag 24.11.2026, Q00-H1).
//!
//! Die effektive Reasoning-Stufe geht als Feld `reasoning` in den Cache-Schlüssel ein (Wert `null` auf Wegen ohne
//! Stufe: Decisions API, Embeddings); `max_tokens` geht nicht ein; `Error: Output limit reached` ist ein Fehler und wird
//! nicht gecacht. Requests gehen an den ai-mock; gezählt wird im Mock-Log.
// acceptance: independent

mod acceptance_k10_support;

use acceptance_k10_support::*;
use os_ai::adapters::openai_decisions::{DecisionInput, Question};
use os_ai::cache::{
    input_hash, CacheKey, CacheKeyInput, CacheOptions, CachedRun, ExecutionPath, Lookup,
    ResultCache,
};
use os_ai::provider::{Input, ReasoningLevel};
use os_ai::AiStatus;
use serde_json::{json, Value};

const PAGES: [&str; 3] = [
    "K10 cache page one: running shoes for trail",
    "K10 cache page two: rain jackets",
    "K10 cache page three: camping stoves",
];

fn title_key(text: &str, level: ReasoningLevel, endpoint: &str) -> CacheKey {
    CacheKey::with_reasoning(
        CacheKeyInput {
            task: "suggest-title".into(),
            definition_id: "suggest-title".into(),
            definition_version: "1".into(),
            params: json!({}),
            path: None,
            provider: "openai".into(),
            model: "gpt-6-luna".into(),
            endpoint: endpoint.into(),
            result_params: json!({}),
            input_hash: input_hash(&[Input::Text { text: text.into() }]),
        },
        level,
    )
}

fn decision_input(text: &str) -> CacheKeyInput {
    CacheKeyInput {
        task: "decision".into(),
        definition_id: "page-type".into(),
        definition_version: "1".into(),
        params: json!({}),
        path: Some(ExecutionPath::DecisionsApi),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: "https://api.openai.com/v1".into(),
        result_params: json!({}),
        input_hash: input_hash(&[Input::Text { text: text.into() }]),
    }
}

fn embedding_input(text: &str) -> CacheKeyInput {
    CacheKeyInput {
        task: "embedding".into(),
        definition_id: "embedding".into(),
        definition_version: "1".into(),
        params: json!({}),
        path: None,
        provider: "openai".into(),
        model: "text-embedding-3-small".into(),
        endpoint: "https://api.openai.com/v1".into(),
        result_params: json!({"dimensions": 1536}),
        input_hash: input_hash(&[Input::Text { text: text.into() }]),
    }
}

/// `suggest-title` für alle Seiten mit Stufe `level` und `max_tokens`; liefert die Zahl gesendeter Requests.
async fn run_titles(
    mock: &Mock,
    cache: &ResultCache<'_>,
    level: ReasoningLevel,
    max_tokens: u64,
) -> usize {
    let adapter = mock.responses();
    let endpoint = mock.url("/openai/v1");
    let before = mock.bodies(RESPONSES).len();
    for page in PAGES {
        let key = title_key(page, level, &endpoint);
        let run = cache
            .get_or_run(&key, now(), || async {
                let r = request(
                    model("openai", "gpt-6-luna", "responses", luna_levels()),
                    page,
                    None,
                    level,
                    Some(max_tokens),
                );
                let reply = adapter.send(&r, None, None).await.expect("Mock antwortet");
                assert!(reply.output_limit().is_none());
                (
                    AiStatus::Success,
                    json!({"title": format!("{:?}", reply.output)}),
                    (),
                )
            })
            .await
            .expect("Cache");
        if let CachedRun::Ran { stored, .. } = run {
            assert!(stored, "Erfolg wird gecacht");
        }
    }
    mock.bodies(RESPONSES).len() - before
}

// spec: AI-015 [M4]
// acceptance: independent
// Kriterium: `suggest-title` mit Stufe `off` gelaufen; Wechsel auf `low` → neue Requests für alle Einträge;
// Rückwechsel auf `off` → 0 Requests (beide Ergebnisse im Cache).
#[tokio::test]
async fn level_change_requests_again_and_switching_back_is_free() {
    let mock = Mock::start().await;
    let dir = TempDir::new("cache-level");
    let file = crawl_file(&dir.0);
    let cache = ResultCache::new(file.conn(), CacheOptions::default());

    assert_eq!(
        run_titles(&mock, &cache, ReasoningLevel::Off, 1024).await,
        3
    );
    assert_eq!(
        run_titles(&mock, &cache, ReasoningLevel::Off, 1024).await,
        0
    );
    assert_eq!(
        run_titles(&mock, &cache, ReasoningLevel::Low, 3072).await,
        3,
        "Wechsel auf low"
    );
    assert_eq!(
        run_titles(&mock, &cache, ReasoningLevel::Off, 1024).await,
        0,
        "Rückwechsel auf off"
    );
    assert_eq!(
        run_titles(&mock, &cache, ReasoningLevel::Low, 3072).await,
        0
    );

    // Die Stufe ging tatsächlich unterschiedlich hinaus (Gegenprobe zum Schlüssel).
    let efforts: Vec<Option<String>> = mock
        .bodies(RESPONSES)
        .iter()
        .map(|b| {
            b.pointer("/reasoning/effort")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    assert_eq!(efforts.len(), 6);
    assert!(
        efforts[..3].iter().all(|e| e.as_deref() == Some("none")),
        "{efforts:?}"
    );
    assert!(
        efforts[3..].iter().all(|e| e.as_deref() == Some("low")),
        "{efforts:?}"
    );
}

// spec: AI-015 [M4]
// acceptance: independent
// Kriterium: Änderung nur von `max_tokens` → 0 Requests.
#[tokio::test]
async fn max_tokens_change_alone_is_free() {
    let mock = Mock::start().await;
    let dir = TempDir::new("cache-max");
    let file = crawl_file(&dir.0);
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    assert_eq!(
        run_titles(&mock, &cache, ReasoningLevel::Off, 1024).await,
        3
    );
    assert_eq!(
        run_titles(&mock, &cache, ReasoningLevel::Off, 4000).await,
        0
    );
    assert_eq!(run_titles(&mock, &cache, ReasoningLevel::Off, 64).await, 0);
    for page in PAGES {
        let endpoint = mock.url("/openai/v1");
        let a = title_key(page, ReasoningLevel::Off, &endpoint);
        let b = title_key(page, ReasoningLevel::Off, &endpoint);
        assert_eq!(a.as_str(), b.as_str());
        assert!(!a.canonical().contains("max_tokens"), "{}", a.canonical());
    }
}

// spec: AI-015 [M4]
// acceptance: independent
// Auf Wegen mit Stufe ist sie Teil des Schlüssels: jede der acht Stufen ergibt einen eigenen Schlüssel; der Schlüssel
// trägt die Stufe unter dem Namen `reasoning` mit dem kanonischen Stufennamen.
#[test]
fn every_level_has_its_own_key_on_paths_with_levels() {
    let mut keys = std::collections::BTreeSet::new();
    for level in ALL_LEVELS {
        let key = title_key("same input", level, "https://api.openai.com/v1");
        assert_eq!(key.reasoning(), Some(level));
        let canonical: Value = serde_json::from_str(&key.canonical()).expect("kanonisches JSON");
        assert_eq!(
            canonical["reasoning"],
            Value::from(level.as_str()),
            "{canonical}"
        );
        keys.insert(key.as_str().to_string());
    }
    assert_eq!(keys.len(), 8);

    let structured = CacheKeyInput {
        path: Some(ExecutionPath::StructuredOutput),
        provider: "anthropic".into(),
        model: "claude-haiku-5-5".into(),
        ..decision_input("same input")
    };
    let off = CacheKey::with_reasoning(structured.clone(), ReasoningLevel::Off);
    let low = CacheKey::with_reasoning(structured, ReasoningLevel::Low);
    assert_ne!(off.as_str(), low.as_str(), "Ersatzweg mit Stufe");
}

// spec: AI-015 [M4]
// acceptance: independent
// Kriterium: Seitentyp-Frage über die Decisions API → Schlüssel mit `reasoning = null`, unabhängig von der für `decision`
// eingestellten Stufe; dasselbe für Embeddings.
#[test]
fn decisions_and_embeddings_ignore_the_level() {
    for input in [decision_input("page text"), embedding_input("page text")] {
        let reference = CacheKey::with_reasoning(input.clone(), ReasoningLevel::ModelDefault);
        assert_eq!(reference.reasoning(), None, "{}", input.task);
        for level in ALL_LEVELS {
            let key = CacheKey::with_reasoning(input.clone(), level);
            assert_eq!(key.as_str(), reference.as_str(), "{} {level}", input.task);
            assert_eq!(key.reasoning(), None);
        }
        assert_eq!(CacheKey::new(input.clone()).as_str(), reference.as_str());
    }
}

// spec: AI-015 [M4]
// acceptance: independent
// Wortlaut des Nachtrags: Die Stufe „geht als Feld `reasoning` in den Cache-Schlüssel ein (Wert `null` auf Wegen ohne
// Stufe …)“. Wörtlich gelesen steht das Feld mit dem Wert `null` in der kanonischen Darstellung (Befund B2 im Bericht:
// Weglassen des Felds ist die zweite Lesart).
#[test]
fn decisions_key_names_reasoning_null_literally() {
    for input in [decision_input("page text"), embedding_input("page text")] {
        let key = CacheKey::with_reasoning(input.clone(), ReasoningLevel::Medium);
        let canonical: Value = serde_json::from_str(&key.canonical()).expect("kanonisches JSON");
        assert!(
            canonical.get("reasoning").is_some_and(Value::is_null),
            "Feld `reasoning` mit Wert null fehlt im Schlüssel ({}): {canonical}",
            input.task
        );
    }
}

// spec: AI-015 [M4]
// acceptance: independent
// Decisions über den Mock: erster Lauf mit eingestellter Stufe `off` → 1 Request; danach mit Stufe `medium` → 0 Requests
// (gleicher Schlüssel, Treffer `Success (cached)`).
#[tokio::test]
async fn decisions_run_is_cached_across_level_settings() {
    let mock = Mock::start().await;
    let dir = TempDir::new("cache-decisions");
    let file = crawl_file(&dir.0);
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let adapter = mock.decisions();
    let questions = vec![Question::predicate("is_product", "Is this a product page?")];
    let text = "K10 cache decisions page";
    for (level, expected_requests) in [
        (ReasoningLevel::Off, 1usize),
        (ReasoningLevel::Medium, 0),
        (ReasoningLevel::ModelDefault, 0),
    ] {
        let before = mock.bodies(DECISIONS).len();
        let key = CacheKey::with_reasoning(decision_input(text), level);
        let run = cache
            .get_or_run(&key, now(), || async {
                let reply = adapter
                    .send("gpt-6-luna", &DecisionInput::text(text), &questions, None)
                    .await
                    .expect("Mock antwortet");
                assert!(reply.readable);
                (
                    AiStatus::Success,
                    json!({"answers": reply.results.len()}),
                    (),
                )
            })
            .await
            .unwrap();
        assert_eq!(
            mock.bodies(DECISIONS).len() - before,
            expected_requests,
            "{level}"
        );
        if expected_requests == 0 {
            assert!(
                matches!(run, CachedRun::Hit { ref status, .. } if *status == AiStatus::SuccessCached),
                "{run:?}"
            );
        }
    }
}

// spec: AI-015 [M4]
// spec: AI-020 [M4]
// acceptance: independent
// Kriterium: Ein Eintrag mit `Error: Output limit reached` wird beim nächsten Lauf erneut angefragt (Fehler werden nicht
// gecacht). Gegenprobe: ein erfolgreicher Eintrag desselben Laufs wird nicht erneut angefragt.
#[tokio::test]
async fn output_limit_is_not_cached_and_requested_again() {
    let mock = Mock::start().await;
    let dir = TempDir::new("cache-limit");
    let file = crawl_file(&dir.0);
    let cache = ResultCache::new(file.conn(), CacheOptions::default());
    let adapter = mock.responses();
    let endpoint = mock.url("/openai/v1");
    let limited = "K10 cache limit MOCK-SCENARIO:output-limit";
    let fine = "K10 cache fine";

    for run_no in 1..=2 {
        for text in [limited, fine] {
            let key = title_key(text, ReasoningLevel::Off, &endpoint);
            let run = cache
                .get_or_run(&key, now(), || async {
                    let r = request(
                        model("openai", "gpt-6-luna", "responses", luna_levels()),
                        text,
                        None,
                        ReasoningLevel::Off,
                        Some(64),
                    );
                    let reply = adapter.send(&r, None, None).await.expect("HTTP 200");
                    let status = if reply.output_limit().is_some() {
                        AiStatus::OutputLimitReached
                    } else {
                        AiStatus::Success
                    };
                    (status, json!({"text": text}), ())
                })
                .await
                .unwrap();
            if text == limited {
                assert!(
                    matches!(run, CachedRun::Ran { stored: false, .. }),
                    "Lauf {run_no}: {run:?}"
                );
            } else if run_no == 2 {
                assert!(matches!(run, CachedRun::Hit { .. }), "{run:?}");
            }
        }
    }
    assert_eq!(
        mock.bodies_with(RESPONSES, "K10 cache limit").len(),
        2,
        "erneut angefragt"
    );
    assert_eq!(mock.bodies_with(RESPONSES, fine).len(), 1);

    // Unmittelbar: der Status wird nie gespeichert.
    let key = title_key("K10 cache direct", ReasoningLevel::Off, &endpoint);
    assert!(!cache
        .store(&key, &AiStatus::OutputLimitReached, &json!(null), now())
        .unwrap());
    assert_eq!(cache.lookup(&key).unwrap(), Lookup::Miss);
}

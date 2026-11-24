//! Embedding-Anbieter (AI-008) gegen ai-mock (M4-K03): Bündelung, L2-Normalisierung, Cache (Speicher und Crawl-Datei
//! nach AI-015), Einzelversuch nach Bündelfehler (Szenario `context-length`), Embedding-Modellwahl ohne Anthropic.
//! Anbieter: OpenAI (`/openai/v1/embeddings`), OpenRouter (`/openrouter/api/v1/embeddings`), Ollama
//! (`/ollama/api/embed`). Nicht normalisierte Mock-Vektoren stehen in `tests/fixtures/ai/mock-answers.d/k03.json`.
#[path = "adapter_anthropic_support.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use os_ai::adapters::ollama::{OllamaAdapter, OllamaConfig};
use os_ai::adapters::openai_responses::{
    ApiKey as OpenAiKey, OpenAiAccount, OpenAiConfig, OpenAiHttp,
};
use os_ai::adapters::openrouter::OpenRouterAdapter;
use os_ai::cache::{CacheOptions, ResultCache};
use os_ai::embeddings::openai::{OpenAiEmbeddings, OpenRouterEmbeddings, OPENAI_MAX_INPUTS};
use os_ai::embeddings::{
    embed_texts, embedding_models, l2_norm, provider_offers_embeddings, EmbeddingCache,
    EmbeddingProvider, EmbeddingSettings, MemoryEmbeddingCache, NoEmbeddingCache,
    StoredEmbeddingCache, DEFAULT_BATCH_SIZE, NORM_TOLERANCE,
};
use os_ai::limits::{Limiter, ProviderLimits};
use os_ai::provider::{AiProvider, Capabilities, CapabilitySource, Model};
use os_ai::AiStatus;
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::{CrawlFile, CrawlInit, StorageProfile};
use support::*;

const OPENAI_EMBED: &str = "/openai/v1/embeddings";
const OPENROUTER_EMBED: &str = "/openrouter/api/v1/embeddings";
const OLLAMA_EMBED: &str = "/ollama/api/embed";

fn limiter() -> Arc<Limiter> {
    limiter_with(LONG_TIMEOUT, 4, |config| {
        for provider in ["openai", "ollama"] {
            config.providers.insert(
                provider.into(),
                ProviderLimits {
                    concurrent_requests: 1,
                    timeout: LONG_TIMEOUT,
                },
            );
        }
    })
}

fn openai(mock: &Mock) -> OpenAiEmbeddings {
    let config = OpenAiConfig::new(OpenAiKey::new("sk-TEST-K03"))
        .with_base_url(format!("{}/v1", mock.server.base_url("openai")));
    OpenAiEmbeddings::new(OpenAiHttp::new(
        reqwest::Client::new(),
        config,
        OpenAiAccount::new(),
    ))
}

fn openrouter(mock: &Mock) -> OpenRouterEmbeddings {
    OpenRouterEmbeddings::new(reqwest::Client::new(), mock.openrouter_config())
}

fn ollama(mock: &Mock) -> OllamaAdapter {
    OllamaAdapter::new(
        reqwest::Client::new(),
        OllamaConfig::new().with_base_url(mock.server.base_url("ollama")),
    )
}

fn texts(n: usize, prefix: &str) -> Vec<String> {
    (0..n).map(|i| format!("{prefix} Text {i}")).collect()
}

fn input_sizes(mock: &Mock, path: &str) -> Vec<usize> {
    mock.bodies(path)
        .iter()
        .map(|b| b["input"].as_array().map_or(1, Vec::len))
        .collect()
}

// spec: AI-008
// 130 Texte bei Bündelgröße 64 → genau 3 Requests (64, 64, 2); je Text ein Vektor mit Einheitslänge. Geprüft für
// OpenAI, OpenRouter und Ollama.
#[tokio::test]
async fn texts_are_bundled_into_requests_of_the_batch_size() {
    let mock = Mock::start().await;
    let limiter = limiter();
    let settings = EmbeddingSettings::new("mock-embedding");
    assert_eq!(settings.batch_size, DEFAULT_BATCH_SIZE);
    assert_eq!(DEFAULT_BATCH_SIZE, 64);
    let input = texts(130, "Bündel");
    let providers: [(Box<dyn EmbeddingProvider>, &str); 3] = [
        (Box::new(openai(&mock)), OPENAI_EMBED),
        (Box::new(openrouter(&mock)), OPENROUTER_EMBED),
        (Box::new(ollama(&mock)), OLLAMA_EMBED),
    ];
    for (provider, path) in providers {
        let run = embed_texts(
            provider.as_ref(),
            &limiter,
            &NoEmbeddingCache,
            &settings,
            &input,
        )
        .await
        .unwrap();
        assert_eq!(run.request_sizes(), vec![64, 64, 2], "{path}");
        assert_eq!(input_sizes(&mock, path), vec![64, 64, 2], "{path}");
        assert_eq!(run.results.len(), 130);
        for (index, result) in run.results.iter().enumerate() {
            assert_eq!(result.status, AiStatus::Success, "{path} {index}");
            let vector = result.vector.as_ref().unwrap();
            assert_eq!(vector.len(), 768);
            assert!((l2_norm(vector) - 1.0).abs() <= NORM_TOLERANCE);
        }
        // Reihenfolge bleibt erhalten: Text 64 steht im zweiten Request an erster Stelle.
        assert_eq!(run.results[64].request, Some(1));
        assert_eq!(mock.bodies(path)[1]["input"][0], input[64]);
    }

    // Die Bündelgröße ist einstellbar und durch das Anbieterlimit begrenzt.
    let provider = openai(&mock);
    assert_eq!(provider.max_batch_size(), Some(OPENAI_MAX_INPUTS));
    assert_eq!(
        EmbeddingSettings::new("m")
            .with_batch_size(5000)
            .effective_batch_size(&provider),
        OPENAI_MAX_INPUTS
    );
    assert_eq!(
        EmbeddingSettings::new("m")
            .with_batch_size(0)
            .effective_batch_size(&provider),
        1
    );
}

// spec: AI-008
// Gespeicherte Vektoren haben L2-Norm 1 (± 1e-6), auch wenn der Mock nicht normalisierte Vektoren liefert
// (`k03-unnormalized` = [3, 0, 4, 0] → [0,6; 0; 0,8; 0]); die Dimensionen gehen als Modellparameter in den Request.
#[tokio::test]
async fn stored_vectors_are_l2_normalized() {
    let mock = Mock::start().await;
    let limiter = limiter();
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(4);
    let input = vec![
        "Seite MOCK-ID:k03-unnormalized".to_string(),
        "Seite MOCK-ID:k03-unnormalized-b".to_string(),
        "Seite ohne Vorgabe".to_string(),
    ];
    let providers: [(Box<dyn EmbeddingProvider>, &str); 3] = [
        (Box::new(openai(&mock)), OPENAI_EMBED),
        (Box::new(openrouter(&mock)), OPENROUTER_EMBED),
        (Box::new(ollama(&mock)), OLLAMA_EMBED),
    ];
    for (provider, path) in providers {
        let run = embed_texts(
            provider.as_ref(),
            &limiter,
            &NoEmbeddingCache,
            &settings,
            &input,
        )
        .await
        .unwrap();
        assert_eq!(mock.bodies(path)[0]["dimensions"], 4, "{path}");
        let vectors: Vec<&Vec<f32>> = run
            .results
            .iter()
            .map(|r| r.vector.as_ref().unwrap())
            .collect();
        assert_eq!(vectors[0], &vec![0.6f32, 0.0, 0.8, 0.0]);
        assert_eq!(vectors[1], &vec![0.0f32, -1.0, 0.0, 0.0]);
        for vector in vectors {
            assert_eq!(vector.len(), 4);
            assert!((l2_norm(vector) - 1.0).abs() <= NORM_TOLERANCE, "{path}");
        }
    }
}

// spec: AI-008
// Ein Text, dessen Embedding bereits im Cache liegt (gleicher Inhalts-Hash, gleiches Modell, gleiche Dimensionen),
// wird nicht erneut gesendet; ein anderes Modell oder andere Dimensionen sind ein Fehlversuch.
#[tokio::test]
async fn cached_embeddings_are_not_sent_again() {
    let mock = Mock::start().await;
    let limiter = limiter();
    let provider = openai(&mock);
    let cache = MemoryEmbeddingCache::default();
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(8);
    let first = texts(3, "Cache");
    let run = embed_texts(&provider, &limiter, &cache, &settings, &first)
        .await
        .unwrap();
    assert_eq!(run.request_sizes(), vec![3]);
    assert_eq!(cache.len(), 3);

    // Zwei bekannte und ein neuer Text: nur der neue geht hinaus.
    let second = vec![
        first[0].clone(),
        "Cache Text neu".to_string(),
        first[2].clone(),
    ];
    let again = embed_texts(&provider, &limiter, &cache, &settings, &second)
        .await
        .unwrap();
    assert_eq!(again.request_sizes(), vec![1]);
    assert_eq!(
        mock.bodies(OPENAI_EMBED)[1]["input"],
        serde_json::json!(["Cache Text neu"])
    );
    assert_eq!(again.cache_hits(), 2);
    assert_eq!(again.results[0].status, AiStatus::SuccessCached);
    assert_eq!(again.results[0].vector, run.results[0].vector);
    assert_eq!(again.results[1].status, AiStatus::Success);
    assert_eq!(again.results[2].status, AiStatus::SuccessCached);

    // Gleicher Text, andere Dimensionen bzw. anderes Modell → neuer Request.
    let other_dims = embed_texts(
        &provider,
        &limiter,
        &cache,
        &EmbeddingSettings::new("mock-embedding").with_dimensions(16),
        &first[..1],
    )
    .await
    .unwrap();
    assert_eq!(other_dims.request_sizes(), vec![1]);
    let other_model = embed_texts(
        &provider,
        &limiter,
        &cache,
        &EmbeddingSettings::new("mock-embedding-2").with_dimensions(8),
        &first[..1],
    )
    .await
    .unwrap();
    assert_eq!(other_model.request_sizes(), vec![1]);
    assert_eq!(mock.requests_to(OPENAI_EMBED).len(), 4);
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("os-ai-k03-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn now() -> Timestamp {
    Timestamp::from_db("2026-11-24T10:00:00.000Z").unwrap()
}

fn crawl(dir: &Path, name: &str) -> CrawlFile {
    let init = CrawlInit {
        crawl_id: SeededIds::new(3).new_crawl_id(now()),
        mode: CrawlMode::Spider,
        start_url: Some("http://ai.test:8080/".into()),
        list_source: None,
        project: "K03".into(),
        name: name.into(),
        config: CrawlConfig::default(),
        created_at: now(),
    };
    CrawlFile::create(&dir.join(name), &init, StorageProfile::default()).unwrap()
}

// spec: AI-008
// spec: AI-015
// Der Cache in der Crawl-Datei (Ergebnis-Cache, Tabelle `ai_cache`) gilt auch nach erneutem Öffnen; ein Treffer
// erzeugt keinen Request.
#[tokio::test]
async fn embeddings_cache_lives_in_the_crawl_file() {
    let mock = Mock::start().await;
    let limiter = limiter();
    let provider = ollama(&mock);
    let dir = TempDir::new("stored");
    let file = crawl(&dir.0, "embed.ospider");
    let settings = EmbeddingSettings::new("mock-embed").with_dimensions(4);
    let input = vec!["Seite MOCK-ID:k03-unnormalized".to_string()];
    {
        let results = ResultCache::new(file.conn(), CacheOptions::default());
        let cache = StoredEmbeddingCache::new(&results, "embedding-prompt-1", "1", now());
        let run = embed_texts(&provider, &limiter, &cache, &settings, &input)
            .await
            .unwrap();
        assert_eq!(run.results[0].status, AiStatus::Success);
        assert_eq!(results.stats().unwrap().entries, 1);
    }
    let results = ResultCache::new(file.conn(), CacheOptions::default());
    let cache = StoredEmbeddingCache::new(&results, "embedding-prompt-1", "1", now());
    let run = embed_texts(&provider, &limiter, &cache, &settings, &input)
        .await
        .unwrap();
    assert_eq!(run.results[0].status, AiStatus::SuccessCached);
    assert_eq!(run.results[0].vector, Some(vec![0.6, 0.0, 0.8, 0.0]));
    assert!(run.requests.is_empty());
    assert_eq!(mock.requests_to(OLLAMA_EMBED).len(), 1);
    // „Cache verwenden“ aus: wieder ein Request.
    let off = ResultCache::new(
        file.conn(),
        CacheOptions {
            use_cache: false,
            retry_refusals: false,
        },
    );
    let cache = StoredEmbeddingCache::new(&off, "embedding-prompt-1", "1", now());
    assert_eq!(
        cache
            .get(&os_ai::embeddings::EmbeddingKey::new(
                &provider, &settings, &input[0]
            ))
            .unwrap(),
        None
    );
    embed_texts(&provider, &limiter, &cache, &settings, &input)
        .await
        .unwrap();
    assert_eq!(mock.requests_to(OLLAMA_EMBED).len(), 2);
}

// spec: AI-008
// Liefert ein Bündel-Request einen Fehler für den gesamten Request (ein Text überschreitet das Kontextlimit,
// Szenario `context-length`), werden die Texte einzeln erneut versucht; nur der fehlerhafte erhält einen
// Fehlerstatus. Geprüft für OpenAI und Ollama.
#[tokio::test]
async fn a_failing_batch_is_retried_text_by_text() {
    let mock = Mock::start().await;
    let limiter = limiter();
    let mut input = texts(5, "Kontext");
    input[3] = "Sehr langer Text MOCK-SCENARIO:context-length".into();
    let providers: [(Box<dyn EmbeddingProvider>, &str); 2] = [
        (Box::new(openai(&mock)), OPENAI_EMBED),
        (Box::new(ollama(&mock)), OLLAMA_EMBED),
    ];
    for (provider, path) in providers {
        let run = embed_texts(
            provider.as_ref(),
            &limiter,
            &NoEmbeddingCache,
            &EmbeddingSettings::new("mock-embedding"),
            &input,
        )
        .await
        .unwrap();
        assert_eq!(run.request_sizes(), vec![5, 1, 1, 1, 1, 1], "{path}");
        assert_eq!(input_sizes(&mock, path), vec![5, 1, 1, 1, 1, 1], "{path}");
        assert!(run.requests[1..].iter().all(|r| r.single_retry));
        for (index, result) in run.results.iter().enumerate() {
            if index == 3 {
                assert_eq!(
                    result.status,
                    AiStatus::HttpError {
                        code: 400,
                        message: "maximum context length exceeded".into()
                    },
                    "{path}"
                );
                assert!(result.vector.is_none());
                let request = &run.requests[result.request.unwrap()];
                assert_eq!(request.texts, vec![3]);
                assert_eq!(request.outcome().http_status, Some(400));
            } else {
                assert_eq!(result.status, AiStatus::Success, "{path} {index}");
                assert!(result.vector.is_some());
            }
        }
        // Das Protokoll enthält den Bündelfehler und die Einzelversuche (6 Zeilen).
        assert_eq!(
            run.requests[0].outcome().status.to_string(),
            "Error: 400 maximum context length exceeded"
        );
        assert_eq!(run.requests.len(), 6);
    }
}

// spec: AI-008
// Fehler, die nicht an einzelnen Texten liegen (Kontingent erschöpft), werden nicht vervielfacht: genau 1 Request,
// alle Texte `Quota exceeded`. Leere Texte werden nicht gesendet (`Skipped (empty input)`, INTEG-180).
#[tokio::test]
async fn account_errors_are_not_retried_per_text_and_empty_texts_are_skipped() {
    let mock = Mock::start().await;
    let limiter = limiter();
    let provider = openai(&mock);
    mock.scenario("openai", "insufficient-quota", 1).await;
    let input = vec![
        "a".to_string(),
        "  ".to_string(),
        "b".to_string(),
        "c".to_string(),
    ];
    let run = embed_texts(
        &provider,
        &limiter,
        &NoEmbeddingCache,
        &EmbeddingSettings::new("mock-embedding"),
        &input,
    )
    .await
    .unwrap();
    assert_eq!(mock.requests_to(OPENAI_EMBED).len(), 1);
    assert_eq!(input_sizes(&mock, OPENAI_EMBED), vec![3]);
    assert_eq!(run.results[1].status, AiStatus::SkippedEmptyInput);
    for index in [0, 2, 3] {
        assert_eq!(run.results[index].status, AiStatus::QuotaExceeded);
    }
    let outcome = run.requests[0].outcome();
    assert_eq!(
        outcome.error_code.as_deref(),
        Some("credit_balance_exhausted")
    );
    // Das Konto bleibt gesperrt: kein weiterer Request.
    let again = embed_texts(
        &provider,
        &limiter,
        &NoEmbeddingCache,
        &EmbeddingSettings::new("mock-embedding"),
        &["d".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(again.results[0].status, AiStatus::QuotaExceeded);
    assert_eq!(mock.requests_to(OPENAI_EMBED).len(), 1);
}

// spec: AI-008
// Anthropic erscheint in der Embedding-Modellwahl nicht (auch nicht, wenn eine Quelle ein Modell als
// embedding-fähig meldete); angeboten werden Modelle von OpenAI, OpenRouter (mit Ausgabemodalität `embeddings`) und
// Ollama. Gemini folgt erst mit AI-007 (P2) und erscheint ebenfalls nicht.
#[tokio::test]
async fn anthropic_is_not_offered_for_embeddings() {
    let mock = Mock::start().await;
    let model = |provider: &str, id: &str, embeddings: bool| Model {
        provider: provider.into(),
        id: id.into(),
        endpoint: "embeddings".into(),
        capabilities: Capabilities {
            text: true,
            embeddings,
            ..Capabilities::default()
        },
        capability_source: CapabilitySource::ModelCatalog,
    };
    let mut models = vec![
        model("anthropic", "mock-claude", true),
        model("openai", "mock-embedding", true),
        model("openai", "gpt-6-luna", false),
        model("gemini", "gemini-embedding", true),
    ];
    models.extend(
        OpenRouterAdapter::new(reqwest::Client::new(), mock.openrouter_config())
            .models()
            .await
            .unwrap(),
    );
    models.extend(ollama(&mock).models().await.unwrap());
    let offered: Vec<(String, String)> = embedding_models(&models)
        .into_iter()
        .map(|m| (m.provider, m.id))
        .collect();
    assert_eq!(
        offered,
        vec![
            ("openai".to_string(), "mock-embedding".to_string()),
            ("openrouter".to_string(), "mock/embedding".to_string()),
            ("ollama".to_string(), "mock-llama".to_string()),
        ]
    );
    assert!(!provider_offers_embeddings("anthropic"));
    assert!(!provider_offers_embeddings("gemini"));
    for provider in ["openai", "openrouter", "ollama"] {
        assert!(provider_offers_embeddings(provider));
    }
}

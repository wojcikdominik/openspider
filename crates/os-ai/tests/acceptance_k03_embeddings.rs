//! Unabhängige Abnahmetests M4-K03: Embedding-Anbieter (AI-008) für OpenAI, OpenRouter und Ollama.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-008, AI-001, AI-015, AI-020, ai-mock-Konventionen) und
//! `docs/spec/04-custom-integrations.md` (INTEG-180). Gemini ist nicht Teil dieser Etappe (Festlegung Orchestrator).
//! Anbieter: ai-mock (deterministische Vektoren, Szenario `context-length`), eigene Fragmente unter
//! `tests/fixtures/ai/k03-acceptance/mock-answers.d` (nicht normalisierte Vektoren) und ein eigener Testserver
//! (nicht normalisierte Vektoren hoher Dimension, vertauschte `index`-Reihenfolge).
// acceptance: independent

mod acceptance_k03_support;

use acceptance_k03_support::*;
use os_ai::adapters::anthropic::{AnthropicAdapter, AnthropicConfig};
use os_ai::adapters::ollama::{OllamaAdapter, OllamaConfig};
use os_ai::adapters::openai_responses::{ApiKey, OpenAiAccount, OpenAiConfig, OpenAiHttp};
use os_ai::adapters::openrouter::{OpenRouterAdapter, OpenRouterConfig};
use os_ai::cache::{CacheOptions, ResultCache};
use os_ai::embeddings::openai::{OpenAiEmbeddings, OpenRouterEmbeddings};
use os_ai::embeddings::{
    embed_texts, embedding_models, provider_offers_embeddings, EmbeddingProvider, EmbeddingRun,
    EmbeddingSettings, MemoryEmbeddingCache, NoEmbeddingCache, StoredEmbeddingCache,
};
use os_ai::provider::{AiProvider, Capabilities, CapabilitySource, Model};
use os_ai::AiStatus;
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};
use serde_json::{json, Value};

const TOL: f64 = 1e-6;

/// Die drei Embedding-Anbieter dieser Etappe gegen den ai-mock, mit Mock-Pfad.
fn providers(mock: &Mock) -> Vec<(Box<dyn EmbeddingProvider>, &'static str)> {
    vec![
        (Box::new(openai(mock)), "/openai/v1/embeddings"),
        (Box::new(openrouter(mock)), "/openrouter/api/v1/embeddings"),
        (Box::new(ollama(mock)), "/ollama/api/embed"),
    ]
}

fn openai(mock: &Mock) -> OpenAiEmbeddings {
    openai_at(&mock.openai_base())
}

fn openai_at(base: &str) -> OpenAiEmbeddings {
    OpenAiEmbeddings::new(OpenAiHttp::new(
        client(),
        OpenAiConfig::new(ApiKey::new(TEST_KEY)).with_base_url(base),
        OpenAiAccount::new(),
    ))
}

fn openrouter(mock: &Mock) -> OpenRouterEmbeddings {
    OpenRouterEmbeddings::new(client(), openrouter_config(&mock.openrouter_base()))
}

fn openrouter_config(base: &str) -> OpenRouterConfig {
    OpenRouterConfig {
        base_url: base.into(),
        ..OpenRouterConfig::new(os_ai::adapters::openrouter::ApiKey::new(TEST_KEY))
    }
}

fn ollama(mock: &Mock) -> OllamaAdapter {
    OllamaAdapter::new(
        client(),
        OllamaConfig::new().with_base_url(mock.ollama_base()),
    )
}

fn texts(n: usize, prefix: &str) -> Vec<String> {
    (0..n).map(|i| format!("{prefix} Nummer {i}")).collect()
}

/// Eingabetexte je Request aus dem Mock-Protokoll (Feld `input`).
fn inputs_per_request(mock: &Mock, path: &str) -> Vec<Vec<String>> {
    mock.bodies_to("POST", path)
        .iter()
        .map(|b| {
            b["input"]
                .as_array()
                .expect("`input` ist eine Liste")
                .iter()
                .map(|t| t.as_str().unwrap().to_string())
                .collect()
        })
        .collect()
}

fn statuses(run: &EmbeddingRun) -> Vec<String> {
    run.results.iter().map(|r| r.status.to_string()).collect()
}

fn assert_close(actual: &[f32], expected: &[f64], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}: Länge");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (f64::from(*a) - e).abs() <= 1e-6,
            "{what}: Komponente {i}: {a} ≠ {e}"
        );
    }
}

fn normalized(v: &[f64]) -> Vec<f64> {
    let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    v.iter().map(|x| x / n).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 1: 130 Texte bei Bündelgröße 64 → genau 3 Requests (64, 64, 2).

// spec: AI-008
// Kriterium: „130 Texte bei Bündelgröße 64 → genau 3 Requests (64, 64, 2).“ – für OpenAI, OpenRouter und Ollama;
// Standard-Bündelgröße 64 (Beschreibung).
#[tokio::test]
async fn texts_130_with_batch_size_64_give_three_requests_per_provider() {
    let mock = Mock::start().await;
    let (limiter, _clock) = default_limiter();
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(8);
    assert_eq!(
        EmbeddingSettings::new("m").batch_size,
        64,
        "Standard 64 Texte je Request"
    );
    for (provider, path) in providers(&mock) {
        let input = texts(130, provider.provider_id());
        let run = embed_texts(
            provider.as_ref(),
            &limiter,
            &NoEmbeddingCache,
            &settings,
            &input,
        )
        .await
        .unwrap();
        let sent = inputs_per_request(&mock, path);
        let sizes: Vec<usize> = sent.iter().map(Vec::len).collect();
        assert_eq!(
            sizes,
            vec![64, 64, 2],
            "{path}: Requests laut Mock-Protokoll"
        );
        assert_eq!(
            sent.concat(),
            input,
            "{path}: jeder Text genau einmal, in Eingabereihenfolge"
        );
        assert!(
            run.results.iter().all(|r| r.status == AiStatus::Success),
            "{path}: {:?}",
            statuses(&run)
        );
    }
}

// spec: AI-008
// Kriterium: Bündelgröße ist einstellbar (Eingaben/Konfiguration) – 10 Texte bei Bündelgröße 4 → 4, 4, 2.
#[tokio::test]
async fn configured_batch_size_is_respected() {
    let mock = Mock::start().await;
    let (limiter, _clock) = default_limiter();
    let settings = EmbeddingSettings::new("mock-embedding")
        .with_dimensions(8)
        .with_batch_size(4);
    embed_texts(
        &openai(&mock),
        &limiter,
        &NoEmbeddingCache,
        &settings,
        &texts(10, "klein"),
    )
    .await
    .unwrap();
    let sizes: Vec<usize> = inputs_per_request(&mock, "/openai/v1/embeddings")
        .iter()
        .map(Vec::len)
        .collect();
    assert_eq!(sizes, vec![4, 4, 2]);
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 2: Gespeicherte Vektoren haben L2-Norm 1 (± 1e-6), auch wenn der Mock nicht normalisierte Vektoren
// liefert.

// spec: AI-008
// Kriterium: „Gespeicherte Vektoren haben L2-Norm 1 (± 1e-6), auch wenn der Mock nicht normalisierte Vektoren
// liefert.“ – ai-mock mit eigenen Fragmenten (vordefinierte, nicht normalisierte Vektoren), alle drei Anbieter.
#[tokio::test]
async fn non_normalized_mock_vectors_are_stored_with_unit_norm() {
    let mock = Mock::start_with_own_fragments().await;
    let (limiter, _clock) = default_limiter();
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(4);
    let raw: [(&str, [f64; 4]); 4] = [
        ("k03acc-raw-a", [3.0, 4.0, 0.0, 0.0]),
        ("k03acc-raw-b", [10.0, 10.0, 10.0, 10.0]),
        ("k03acc-raw-c", [0.001, -0.002, 0.002, 0.0]),
        ("k03acc-raw-d", [-7.5, 0.0, 2.5, 1250.0]),
    ];
    let input: Vec<String> = raw
        .iter()
        .map(|(id, _)| format!("Seite MOCK-ID:{id} Inhalt"))
        .collect();
    for (provider, path) in providers(&mock) {
        let run = embed_texts(
            provider.as_ref(),
            &limiter,
            &NoEmbeddingCache,
            &settings,
            &input,
        )
        .await
        .unwrap();
        for (result, (id, vector)) in run.results.iter().zip(raw.iter()) {
            assert_eq!(result.status, AiStatus::Success, "{path} {id}");
            let stored = result.vector.as_ref().expect("Vektor");
            assert!(
                (norm(stored) - 1.0).abs() <= TOL,
                "{path} {id}: Norm {}",
                norm(stored)
            );
            assert_close(stored, &normalized(vector), &format!("{path} {id}"));
        }
    }
}

// spec: AI-008
// Kriterium: L2-Norm 1 (± 1e-6) bei nicht normalisierten Vektoren hoher Dimension (768) im OpenAI- und Ollama-Format
// (eigener Testserver).
#[tokio::test]
async fn non_normalized_high_dimension_vectors_are_normalized() {
    fn raw(text_index: usize, dims: usize) -> Vec<f64> {
        (0..dims)
            .map(|d| ((d % 17) as f64 - 5.0) * (text_index as f64 + 1.5) * 3.0)
            .collect()
    }
    let stub = Stub::start(|req, _| {
        let body = req.json();
        let n = body["input"].as_array().unwrap().len();
        if req.path.ends_with("/embed") {
            StubResponse::json(
                200,
                json!({"embeddings": (0..n).map(|i| raw(i, 768)).collect::<Vec<_>>(), "prompt_eval_count": 5}),
            )
        } else {
            StubResponse::json(
                200,
                json!({
                    "object": "list",
                    "data": (0..n).map(|i| json!({"object": "embedding", "index": i, "embedding": raw(i, 768)})).collect::<Vec<_>>(),
                    "usage": {"prompt_tokens": 5, "total_tokens": 5}
                }),
            )
        }
    })
    .await;
    let (limiter, _clock) = default_limiter();
    let settings = EmbeddingSettings::new("big");
    let input = texts(3, "hochdimensional");
    let ollama = OllamaAdapter::new(client(), OllamaConfig::new().with_base_url(stub.base()));
    let openai = openai_at(&format!("{}/v1", stub.base()));
    let list: [&dyn EmbeddingProvider; 2] = [&ollama, &openai];
    for provider in list {
        let run = embed_texts(provider, &limiter, &NoEmbeddingCache, &settings, &input)
            .await
            .unwrap();
        for (i, result) in run.results.iter().enumerate() {
            let stored = result
                .vector
                .as_ref()
                .unwrap_or_else(|| panic!("{}: {:?}", provider.provider_id(), result.status));
            assert_eq!(stored.len(), 768);
            assert!(
                (norm(stored) - 1.0).abs() <= TOL,
                "{} Text {i}: Norm {}",
                provider.provider_id(),
                norm(stored)
            );
            assert_close(stored, &normalized(&raw(i, 768)), provider.provider_id());
        }
    }
}

// spec: AI-008
// Kriterium (Randfall, nur Beobachtung des Status): Ein Nullvektor kann nicht normalisiert werden. Hart geprüft wird
// nur die Folge aus Kriterium 2: Es wird nie ein Vektor gespeichert, dessen Norm nicht 1 ist; die übrigen Texte des
// Bündels erhalten ihre Vektoren.
#[tokio::test]
async fn zero_vector_is_never_stored_with_norm_other_than_one() {
    let mock = Mock::start_with_own_fragments().await;
    let (limiter, _clock) = default_limiter();
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(4);
    let input = vec![
        "Seite MOCK-ID:k03acc-raw-a".to_string(),
        "Seite MOCK-ID:k03acc-zero".to_string(),
        "Seite MOCK-ID:k03acc-raw-b".to_string(),
    ];
    let run = embed_texts(
        &ollama(&mock),
        &limiter,
        &NoEmbeddingCache,
        &settings,
        &input,
    )
    .await
    .unwrap();
    eprintln!(
        "Beobachtung Nullvektor: Status {:?}, Requests {:?}",
        statuses(&run),
        run.request_sizes()
    );
    for result in &run.results {
        if let Some(v) = &result.vector {
            assert!(
                (norm(v) - 1.0).abs() <= TOL,
                "gespeicherte Norm {}",
                norm(v)
            );
        }
    }
    assert!(
        run.results[1].vector.is_none(),
        "Nullvektor nicht speicherbar"
    );
    assert_eq!(run.results[0].status, AiStatus::Success);
    assert_eq!(run.results[2].status, AiStatus::Success);
}

// ---------------------------------------------------------------------------------------------------------------
// Reihenfolge der Ergebnisse gegenüber den Eingaben (Ausgabe: ein Vektor je Eingabe).

// spec: AI-008
// Kriterium (Ausgabe „Vektor je (URL, Embedding-Satz)“): Jeder Text erhält seinen eigenen Vektor – Vergleich mit den
// deterministischen Vektoren des ai-mock je Text, über drei Bündel und alle drei Anbieter.
#[tokio::test]
async fn each_text_gets_its_own_vector_in_input_order() {
    let mock = Mock::start().await;
    let (limiter, _clock) = default_limiter();
    let settings = EmbeddingSettings::new("mock-embedding")
        .with_dimensions(16)
        .with_batch_size(5);
    for (provider, path) in providers(&mock) {
        let input = texts(12, &format!("Reihenfolge {path}"));
        let run = embed_texts(
            provider.as_ref(),
            &limiter,
            &NoEmbeddingCache,
            &settings,
            &input,
        )
        .await
        .unwrap();
        assert_eq!(run.results.len(), input.len());
        for (text, result) in input.iter().zip(&run.results) {
            let expected = os_testsite::mocks::ai::embedding(text, 16);
            assert_close(
                result.vector.as_ref().expect("Vektor"),
                &normalized(&expected),
                &format!("{path}: {text}"),
            );
        }
    }
}

// spec: AI-008
// Kriterium: Reihenfolge – liefert der Anbieter `data` in umgekehrter Reihenfolge (mit `index`), wird jeder Vektor
// trotzdem seinem Text zugeordnet (OpenAI-Format).
#[tokio::test]
async fn openai_format_with_reversed_data_is_mapped_by_index() {
    let stub = Stub::start(|req, _| {
        let body = req.json();
        let input: Vec<String> = body["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect();
        // Vektor kodiert die Nummer des Texts: [Nummer + 1, 1].
        let mut data: Vec<Value> = input
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let number: f64 = t.rsplit(' ').next().unwrap().parse().unwrap();
                json!({"object": "embedding", "index": i, "embedding": [number + 1.0, 1.0]})
            })
            .collect();
        data.reverse();
        StubResponse::json(
            200,
            json!({"object": "list", "data": data, "usage": {"prompt_tokens": 3, "total_tokens": 3}}),
        )
    })
    .await;
    let (limiter, _clock) = default_limiter();
    let input = texts(7, "umgekehrt");
    let run = embed_texts(
        &openai_at(&format!("{}/v1", stub.base())),
        &limiter,
        &NoEmbeddingCache,
        &EmbeddingSettings::new("m"),
        &input,
    )
    .await
    .unwrap();
    for (i, result) in run.results.iter().enumerate() {
        let expected = normalized(&[i as f64 + 1.0, 1.0]);
        assert_close(result.vector.as_ref().unwrap(), &expected, &input[i]);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 3: Ein Text, dessen Embedding bereits im Cache liegt (gleicher Inhalts-Hash, gleiches Modell, gleiche
// Dimensionen), wird nicht erneut gesendet.

// spec: AI-008
// Kriterium: „Ein Text, dessen Embedding bereits im Cache liegt (gleicher Inhalts-Hash, gleiches Modell, gleiche
// Dimensionen), wird nicht erneut gesendet.“ – zweiter Lauf: 0 Requests, gleiche Vektoren, `Success (cached)`.
#[tokio::test]
async fn cached_texts_are_not_sent_again() {
    let mock = Mock::start().await;
    let (limiter, _clock) = default_limiter();
    let cache = MemoryEmbeddingCache::default();
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(8);
    let input = texts(10, "Cache");
    let first = embed_texts(&ollama(&mock), &limiter, &cache, &settings, &input)
        .await
        .unwrap();
    assert_eq!(inputs_per_request(&mock, "/ollama/api/embed").len(), 1);

    let second = embed_texts(&ollama(&mock), &limiter, &cache, &settings, &input)
        .await
        .unwrap();
    assert_eq!(
        inputs_per_request(&mock, "/ollama/api/embed").len(),
        1,
        "Cache-Treffer senden nichts"
    );
    assert!(second
        .results
        .iter()
        .all(|r| r.status == AiStatus::SuccessCached));
    assert_eq!(second.results[0].status.to_string(), "Success (cached)");
    for (a, b) in first.results.iter().zip(&second.results) {
        assert_eq!(a.vector, b.vector, "gleicher Vektor aus dem Cache");
    }
}

// spec: AI-008
// Kriterium: Cache – nur nicht gecachte Texte werden gesendet; gleicher Text mit anderem Modell bzw. anderen
// Dimensionen ist kein Treffer, Rückwechsel ist wieder ein Treffer.
#[tokio::test]
async fn cache_key_covers_content_model_and_dimensions() {
    let mock = Mock::start().await;
    let (limiter, _clock) = default_limiter();
    let cache = MemoryEmbeddingCache::default();
    let provider = openai(&mock);
    let path = "/openai/v1/embeddings";
    let base = EmbeddingSettings::new("mock-embedding").with_dimensions(8);
    let old = texts(5, "alt");
    embed_texts(&provider, &limiter, &cache, &base, &old)
        .await
        .unwrap();

    // 5 bekannte + 2 neue Texte gemischt → nur die 2 neuen werden gesendet.
    let mut mixed = old.clone();
    mixed.insert(2, "neu Nummer A".into());
    mixed.push("neu Nummer B".into());
    let run = embed_texts(&provider, &limiter, &cache, &base, &mixed)
        .await
        .unwrap();
    let sent = inputs_per_request(&mock, path);
    assert_eq!(sent.len(), 2);
    assert_eq!(
        sent[1],
        vec!["neu Nummer A".to_string(), "neu Nummer B".to_string()]
    );
    assert_eq!(run.cache_hits(), 5);
    assert_eq!(run.results[2].status, AiStatus::Success);
    assert_eq!(run.results[0].status, AiStatus::SuccessCached);

    // Anderes Modell → neue Requests; andere Dimensionen → neue Requests.
    let other_model = EmbeddingSettings::new("mock-embedding-2").with_dimensions(8);
    embed_texts(&provider, &limiter, &cache, &other_model, &old)
        .await
        .unwrap();
    assert_eq!(inputs_per_request(&mock, path).len(), 3);
    let other_dims = EmbeddingSettings::new("mock-embedding").with_dimensions(16);
    let run = embed_texts(&provider, &limiter, &cache, &other_dims, &old)
        .await
        .unwrap();
    assert_eq!(inputs_per_request(&mock, path).len(), 4);
    assert!(run
        .results
        .iter()
        .all(|r| r.vector.as_ref().unwrap().len() == 16));

    // Rückwechsel → 0 Requests.
    embed_texts(&provider, &limiter, &cache, &base, &old)
        .await
        .unwrap();
    embed_texts(&provider, &limiter, &cache, &other_model, &old)
        .await
        .unwrap();
    assert_eq!(inputs_per_request(&mock, path).len(), 4);
}

fn now() -> Timestamp {
    Timestamp::from_db("2026-11-24T12:00:00.000Z").unwrap()
}

// spec: AI-008
// spec: AI-015
// Kriterium: Cache in der Crawl-Datei (AI-015) – nach erneutem Öffnen der Crawl-Datei werden bereits berechnete
// Embeddings nicht erneut gesendet.
#[tokio::test]
async fn crawl_file_cache_survives_reopen() {
    let dir = std::env::temp_dir().join(format!("os-ai-k03-acc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("embed.ospider");
    let mock = Mock::start().await;
    let (limiter, _clock) = default_limiter();
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(8);
    let input = texts(4, "Crawl-Datei");
    let first_vectors = {
        let init = CrawlInit {
            crawl_id: SeededIds::new(3).new_crawl_id(now()),
            mode: CrawlMode::Spider,
            start_url: Some("http://ai.test:8080/".into()),
            list_source: None,
            project: "K03-Abnahme".into(),
            name: "embeddings".into(),
            config: CrawlConfig::default(),
            created_at: now(),
        };
        let file = CrawlFile::create(&path, &init, StorageProfile::default()).unwrap();
        let results = ResultCache::new(file.conn(), CacheOptions::default());
        let cache = StoredEmbeddingCache::new(&results, "embed-prompt", "1", now());
        let run = embed_texts(&ollama(&mock), &limiter, &cache, &settings, &input)
            .await
            .unwrap();
        assert!(run.results.iter().all(|r| r.status == AiStatus::Success));
        run.results
            .into_iter()
            .map(|r| r.vector.unwrap())
            .collect::<Vec<_>>()
    };
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let results = ResultCache::new(file.conn(), CacheOptions::default());
    let cache = StoredEmbeddingCache::new(&results, "embed-prompt", "1", now());
    let run = embed_texts(&ollama(&mock), &limiter, &cache, &settings, &input)
        .await
        .unwrap();
    assert_eq!(inputs_per_request(&mock, "/ollama/api/embed").len(), 1);
    assert!(run
        .results
        .iter()
        .all(|r| r.status == AiStatus::SuccessCached));
    for (stored, first) in run.results.iter().zip(&first_vectors) {
        let v = stored.vector.as_ref().unwrap();
        assert!((norm(v) - 1.0).abs() <= TOL);
        assert_close(
            v,
            &first.iter().map(|x| f64::from(*x)).collect::<Vec<_>>(),
            "Vektor aus der Crawl-Datei",
        );
    }
    drop(file);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 4: Fehler für den gesamten Bündel-Request → Texte einzeln erneut versuchen; nur die tatsächlich
// fehlerhaften erhalten einen Fehlerstatus (Szenario: ein Text überschreitet das Kontextlimit).

// spec: AI-008
// Kriterium: „Liefert ein Bündel-Request einen Fehler für den gesamten Request, werden die Texte einzeln erneut
// versucht; nur die tatsächlich fehlerhaften erhalten einen Fehlerstatus (Szenario: ein Text überschreitet das
// Kontextlimit).“ – alle drei Anbieter, Szenario `context-length` an einem markierten Text.
#[tokio::test]
async fn context_length_in_batch_fails_only_the_marked_text() {
    let mock = Mock::start().await;
    let (limiter, _clock) = default_limiter();
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(8);
    for (provider, path) in providers(&mock) {
        let mut input = texts(10, &format!("Teilfehler {path}"));
        input[4] = format!("Zu langer Text MOCK-SCENARIO:context-length {path}");
        let run = embed_texts(
            provider.as_ref(),
            &limiter,
            &NoEmbeddingCache,
            &settings,
            &input,
        )
        .await
        .unwrap();
        for (i, result) in run.results.iter().enumerate() {
            if i == 4 {
                assert!(result.vector.is_none(), "{path}: kein Vektor");
                assert!(result.status.is_error(), "{path}: {:?}", result.status);
                assert!(
                    result.status.to_string().starts_with("Error: 400"),
                    "{path}: Status {}",
                    result.status
                );
            } else {
                assert_eq!(result.status, AiStatus::Success, "{path} Text {i}");
                let v = result.vector.as_ref().unwrap();
                assert!((norm(v) - 1.0).abs() <= TOL);
                assert_close(
                    v,
                    &normalized(&os_testsite::mocks::ai::embedding(&input[i], 8)),
                    path,
                );
            }
        }
        let sent = inputs_per_request(&mock, path);
        assert_eq!(sent[0], input, "{path}: zuerst das ganze Bündel");
        let singles = &sent[1..];
        assert!(
            singles.iter().all(|s| s.len() == 1),
            "{path}: danach Einzelversuche {:?}",
            singles.iter().map(Vec::len).collect::<Vec<_>>()
        );
        let mut retried: Vec<String> = singles.iter().map(|s| s[0].clone()).collect();
        retried.sort();
        let mut expected = input.clone();
        expected.sort();
        assert_eq!(retried, expected, "{path}: jeder Text genau einmal einzeln");
    }
}

// spec: AI-008
// Kriterium: Teilfehler betrifft nur das eigene Bündel – 70 Texte (Bündel 64 + 6), markierter Text im zweiten
// Bündel: das erste Bündel bleibt ein einzelner Request, nur Text 66 scheitert.
#[tokio::test]
async fn context_length_error_is_confined_to_its_batch() {
    let mock = Mock::start().await;
    let (limiter, _clock) = default_limiter();
    let settings = EmbeddingSettings::new("mock-embedding").with_dimensions(8);
    let mut input = texts(70, "Bündel");
    input[66] = "Bündel lang MOCK-SCENARIO:context-length".into();
    let run = embed_texts(
        &openai(&mock),
        &limiter,
        &NoEmbeddingCache,
        &settings,
        &input,
    )
    .await
    .unwrap();
    let failed: Vec<usize> = run
        .results
        .iter()
        .enumerate()
        .filter(|(_, r)| r.status != AiStatus::Success)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(failed, vec![66]);
    let sizes: Vec<usize> = inputs_per_request(&mock, "/openai/v1/embeddings")
        .iter()
        .map(Vec::len)
        .collect();
    assert_eq!(sizes[0], 64, "erstes Bündel unverändert");
    assert_eq!(sizes[1], 6, "zweites Bündel");
    assert!(sizes[2..].iter().all(|n| *n == 1));
    assert_eq!(
        sizes.len(),
        2 + 6,
        "nur die 6 Texte des fehlerhaften Bündels einzeln"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 5: Anthropic erscheint in der Embedding-Modellwahl nicht.

fn model(provider: &str, id: &str, embeddings: bool) -> Model {
    Model {
        provider: provider.into(),
        id: id.into(),
        endpoint: "embeddings".into(),
        capabilities: Capabilities {
            text: true,
            embeddings,
            ..Capabilities::default()
        },
        capability_source: CapabilitySource::ModelCatalog,
        reasoning_levels: Vec::new(),
    }
}

// spec: AI-008
// Kriterium: „Anthropic erscheint in der Embedding-Modellwahl nicht.“ – auch nicht, wenn eine Quelle ein
// Anthropic-Modell fälschlich als embedding-fähig meldet; Modelle ohne Embeddings-Fähigkeit erscheinen ebenfalls nicht.
#[test]
fn anthropic_never_appears_in_embedding_model_choice() {
    let models = vec![
        model("anthropic", "mock-claude", true),
        model("anthropic", "mock-claude-b", false),
        model("openai", "mock-embedding", true),
        model("openai", "gpt-6-luna", false),
        model("openrouter", "mock/embedding", true),
        model("openrouter", "mock/text", false),
        model("ollama", "mock-embed", true),
    ];
    let chosen: Vec<(String, String)> = embedding_models(&models)
        .into_iter()
        .map(|m| (m.provider, m.id))
        .collect();
    assert_eq!(
        chosen,
        vec![
            ("openai".into(), "mock-embedding".into()),
            ("openrouter".into(), "mock/embedding".into()),
            ("ollama".into(), "mock-embed".into()),
        ]
    );
    assert!(!provider_offers_embeddings("anthropic"));
    for provider in ["openai", "openrouter", "ollama"] {
        assert!(provider_offers_embeddings(provider), "{provider}");
    }
}

// spec: AI-008
// Kriterium: Anthropic erscheint nicht; OpenRouter nur mit Modellen, die Embeddings anbieten (Beschreibung „falls das
// Modell Embeddings anbietet“) – mit den live geladenen Modelllisten des ai-mock.
#[tokio::test]
async fn live_model_lists_yield_only_embedding_capable_models() {
    let mock = Mock::start().await;
    let anthropic = AnthropicAdapter::new(
        client(),
        AnthropicConfig::new(os_ai::adapters::anthropic::ApiKey::new(TEST_KEY))
            .with_base_url(mock.anthropic_base()),
    );
    let openrouter = OpenRouterAdapter::new(client(), openrouter_config(&mock.openrouter_base()));
    let mut all = anthropic.models().await.expect("Anthropic-Modellliste");
    assert!(!all.is_empty());
    all.extend(openrouter.models().await.expect("OpenRouter-Modellliste"));
    let chosen: Vec<(String, String)> = embedding_models(&all)
        .into_iter()
        .map(|m| (m.provider, m.id))
        .collect();
    assert_eq!(
        chosen,
        vec![("openrouter".to_string(), "mock/embedding".to_string())]
    );
}

// spec: AI-008
// Kriterium (INTEG-180, Verweis aus der Beschreibung): Ein leerer Text wird nicht gesendet
// (`Skipped (empty input)`), die übrigen Texte erhalten ihre Vektoren.
#[tokio::test]
async fn empty_text_is_not_sent() {
    let mock = Mock::start().await;
    let (limiter, _clock) = default_limiter();
    let input = vec![
        "Mit Text".to_string(),
        String::new(),
        "Auch mit Text".to_string(),
    ];
    let run = embed_texts(
        &ollama(&mock),
        &limiter,
        &NoEmbeddingCache,
        &EmbeddingSettings::new("mock-embedding").with_dimensions(8),
        &input,
    )
    .await
    .unwrap();
    assert_eq!(run.results[1].status.to_string(), "Skipped (empty input)");
    assert!(run.results[1].vector.is_none());
    assert_eq!(
        inputs_per_request(&mock, "/ollama/api/embed"),
        vec![vec!["Mit Text".to_string(), "Auch mit Text".to_string()]]
    );
}

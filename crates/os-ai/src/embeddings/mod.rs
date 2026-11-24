//! Embedding-Anbieter (AI-008): gemeinsame Schnittstelle für OpenAI (`/v1/embeddings`), OpenRouter
//! (`/api/v1/embeddings`, nur Modelle mit Embeddings-Fähigkeit) und Ollama (`/api/embed`); Gemini folgt mit AI-007
//! (P2, nach 1.2). Anthropic bietet keine Embeddings an und erscheint nie in der Embedding-Modellwahl.
//!
//! * [`EmbeddingProvider`] – ein Versuch eines Anbieters für ein Bündel Texte ([`openai::OpenAiEmbeddings`],
//!   [`openai::OpenRouterEmbeddings`], [`crate::adapters::ollama::OllamaAdapter`]).
//! * [`embed_texts`] – Ausführung für viele Texte: Cache-Abfrage (gleicher Inhalts-Hash, gleiches Modell, gleiche
//!   Dimensionen → kein Request), Bündelung (Standard 64 Texte je Request, höchstens Anbieterlimit), Wiederholungen und
//!   Drosselung über [`Limiter::run`], L2-Normalisierung, Einzelversuch nach einem Fehler des ganzen Bündels.
//! * [`EmbeddingCache`] – [`MemoryEmbeddingCache`], [`NoEmbeddingCache`] und [`StoredEmbeddingCache`] über den
//!   Ergebnis-Cache der Crawl-Datei (AI-015).
//! * Je HTTP-Eintrag eine [`EmbeddingRequestLog`] mit [`EmbeddingRequestLog::outcome`] für das Kostenprotokoll
//!   (AI-014, [`crate::cost::log::request_record`]).
//!
//! Vektoren werden als `f32` geliefert (INTEG-180), Speicherung und Anzeige je (URL, Embedding-Satz) gehören M4-E01.
pub mod openai;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use os_model::ai::AiStatus;
use os_model::Timestamp;
use os_store::StoreError;
use serde_json::{json, Value};

use crate::cache::{input_hash, CacheKey, CacheKeyInput, Lookup, ResultCache};
use crate::cost::log::{RequestOutcome, UsageReport};
use crate::cost::Rates;
use crate::limits::{AttemptFailure, FinalError, Limiter, RequestSpec};
use crate::provider::{Input, Model, ProviderError, Usage};

/// Aufgabentyp nach AI-011 (Cache-Schlüssel, Protokoll).
pub const TASK: &str = "embedding";
/// Endpunkt im Protokoll und Preisschlüssel (AI-010).
pub const ENDPOINT: &str = "embeddings";
/// Standard-Bündelgröße (AI-008).
pub const DEFAULT_BATCH_SIZE: usize = 64;
/// Anbieter, die in der Embedding-Modellwahl erscheinen (1.1: ohne Gemini, AI-007 ist P2).
pub const EMBEDDING_PROVIDERS: [&str; 3] = ["openai", "openrouter", "ollama"];
/// Toleranz der Einheitslänge (AI-008: L2-Norm 1 ± 1e-6).
pub const NORM_TOLERANCE: f64 = 1e-6;

/// Ob ein Anbieter Embeddings liefern kann. Anthropic nie (AI-008); Gemini erst mit AI-007.
pub fn provider_offers_embeddings(provider: &str) -> bool {
    EMBEDDING_PROVIDERS.contains(&provider)
}

/// Modelle für die Embedding-Modellwahl (AI-011, Aufgabe `embedding`): Anbieter aus [`EMBEDDING_PROVIDERS`] und
/// Fähigkeit `embeddings` laut Quelle des Modells (AI-001). Anthropic-Modelle fallen immer heraus, auch wenn eine Quelle
/// sie fälschlich als embedding-fähig meldet.
pub fn embedding_models(models: &[Model]) -> Vec<Model> {
    models
        .iter()
        .filter(|m| provider_offers_embeddings(&m.provider) && m.capabilities.embeddings)
        .cloned()
        .collect()
}

/// Liest einen Vektor aus einem JSON-Array von Zahlen; `None`, wenn ein Eintrag keine Zahl ist.
pub fn vector_from_json(value: &Value) -> Option<Vec<f64>> {
    value.as_array()?.iter().map(Value::as_f64).collect()
}

/// Warum ein Vektor nicht übernommen werden kann.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorError {
    /// Leerer Vektor.
    Empty,
    /// Andere Länge als die angeforderten Dimensionen.
    Dimensions { expected: usize, actual: usize },
    /// Nicht endliche Werte oder Nullvektor (nicht normalisierbar).
    NotNormalizable,
}

impl std::fmt::Display for VectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty embedding vector"),
            Self::Dimensions { expected, actual } => {
                write!(f, "embedding has {actual} dimensions, expected {expected}")
            }
            Self::NotNormalizable => f.write_str("embedding vector is not normalizable"),
        }
    }
}

/// L2-Normalisierung (AI-008, INTEG-187): Rechnung in `f64`, Ergebnis `f32`. Prüft die angeforderten Dimensionen.
pub fn l2_normalize(vector: &[f64], dimensions: Option<u32>) -> Result<Vec<f32>, VectorError> {
    if vector.is_empty() {
        return Err(VectorError::Empty);
    }
    if let Some(expected) = dimensions.and_then(|d| usize::try_from(d).ok()) {
        if vector.len() != expected {
            return Err(VectorError::Dimensions {
                expected,
                actual: vector.len(),
            });
        }
    }
    if vector.iter().any(|x| !x.is_finite()) {
        return Err(VectorError::NotNormalizable);
    }
    let norm = vector.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm == 0.0 || !norm.is_finite() {
        return Err(VectorError::NotNormalizable);
    }
    // Werte normalisierter Vektoren liegen in [-1, 1]; die Umwandlung nach f32 ist beabsichtigt.
    #[allow(clippy::cast_possible_truncation)]
    Ok(vector.iter().map(|x| (x / norm) as f32).collect())
}

/// L2-Norm eines gespeicherten Vektors (Prüfhilfe).
pub fn l2_norm(vector: &[f32]) -> f64 {
    vector
        .iter()
        .map(|x| f64::from(*x) * f64::from(*x))
        .sum::<f64>()
        .sqrt()
}

/// Antwort eines Bündel-Requests: ein Rohvektor je Text in Eingabereihenfolge.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingBatchReply {
    pub vectors: Vec<Vec<f64>>,
    /// Einheitliche Tokenzahlen (nur Eingabe; Embeddings haben keine Ausgabe-Tokens, Feld bleibt `null`).
    pub usage: Usage,
    pub provider_request_id: Option<String>,
    pub http_status: u16,
    pub duration_ms: u64,
}

/// Zukunft eines Versuchs.
pub type EmbedFuture<'a> =
    Pin<Box<dyn Future<Output = Result<EmbeddingBatchReply, AttemptFailure>> + Send + 'a>>;

/// Ein Embedding-Anbieter (ein HTTP-Request je Aufruf, ohne Wiederholung).
pub trait EmbeddingProvider: Send + Sync {
    /// Anbieterkennung (`openai`, `openrouter`, `ollama`).
    fn provider_id(&self) -> &str;
    /// Endpunkt-URL (Teil des Cache-Schlüssels, AI-015: globaler bzw. EU-Endpunkt).
    fn endpoint_url(&self) -> String;
    /// Höchstzahl Texte je Request laut Anbieter; `None` = kein dokumentiertes Limit.
    fn max_batch_size(&self) -> Option<usize> {
        None
    }
    /// Ein Versuch für `texts`.
    fn embed<'a>(
        &'a self,
        model: &'a str,
        texts: &'a [String],
        dimensions: Option<u32>,
    ) -> EmbedFuture<'a>;
    /// Nummer eines neuen Eintrags (ein Bündel bzw. Einzelversuch mit allen Wiederholungen); Standard 0.
    fn begin_entry(&self) -> u64 {
        0
    }
    /// Ein Versuch des Eintrags `entry` (Standard: [`EmbeddingProvider::embed`]). Ollama stellt Versuche zurück,
    /// solange ein anderer Eintrag den Server nach einem Verbindungsfehler prüft (AI-006).
    fn embed_entry<'a>(
        &'a self,
        entry: u64,
        model: &'a str,
        texts: &'a [String],
        dimensions: Option<u32>,
    ) -> EmbedFuture<'a> {
        let _ = entry;
        self.embed(model, texts, dimensions)
    }
    /// Ende des Eintrags `entry` (auch bei Abbruch).
    fn end_entry(&self, entry: u64) {
        let _ = entry;
    }
    /// Ob ein endgültiger Fehler nur ein zurückgestellter Versuch ist (nichts gesendet); dann wartet der Eintrag auf
    /// [`EmbeddingProvider::settled`] und beginnt neu.
    fn is_deferred(&self, _error: &FinalError) -> bool {
        false
    }
    /// Wartet, bis zurückgestellte Einträge neu beginnen dürfen.
    fn settled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
    /// Nachbearbeitung eines endgültigen Fehlers (z. B. Ollama: Meldung nach INTEG-160, Sperre für weitere Einträge).
    fn finalize(&self, error: FinalError) -> FinalError {
        error
    }
    /// Ob der Request zu einem endgültigen Fehler (vor [`EmbeddingProvider::finalize`]) tatsächlich gesendet wurde; ein
    /// wegen einer Sperre nicht gesendeter Eintrag erscheint nicht im Protokoll.
    fn was_sent(&self, _error: &FinalError) -> bool {
        true
    }
    /// Endgültiger Fehler für alle weiteren Einträge ohne Request (z. B. Ollama nicht erreichbar).
    fn blocked(&self) -> Option<FinalError> {
        None
    }
    /// Feste Preissätze, falls der Anbieter sie selbst kennt (Ollama: 0); sonst `None` (Katalog bzw. API, M4-K04).
    fn rates(&self) -> Option<Rates> {
        None
    }
}

/// Einstellungen eines Embedding-Laufs (AI-008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingSettings {
    /// Embedding-Modell (AI-011, Aufgabe `embedding`).
    pub model: String,
    /// Angeforderte Dimensionen (Modellparameter, INTEG-187); `None` = Modellstandard.
    pub dimensions: Option<u32>,
    /// Texte je Request (Standard 64, höchstens Anbieterlimit).
    pub batch_size: usize,
}

impl EmbeddingSettings {
    /// Standardwerte für `model`.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            dimensions: None,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }

    /// Mit Dimensionen.
    pub fn with_dimensions(mut self, dimensions: u32) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    /// Mit anderer Bündelgröße.
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Wirksame Bündelgröße: mindestens 1, höchstens das Anbieterlimit.
    pub fn effective_batch_size(&self, provider: &dyn EmbeddingProvider) -> usize {
        self.batch_size
            .max(1)
            .min(provider.max_batch_size().unwrap_or(usize::MAX).max(1))
    }
}

/// Cache-Schlüssel eines Textes (AI-008: gleicher Inhalts-Hash, gleiches Modell, gleiche Dimensionen; dazu Anbieter
/// und Endpunkt wie AI-015).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmbeddingKey {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    pub dimensions: Option<u32>,
    /// [`input_hash`] des gesendeten Texts.
    pub input_hash: String,
}

impl EmbeddingKey {
    /// Schlüssel für `text`.
    pub fn new(provider: &dyn EmbeddingProvider, settings: &EmbeddingSettings, text: &str) -> Self {
        Self {
            provider: provider.provider_id().to_string(),
            model: settings.model.clone(),
            endpoint: provider.endpoint_url(),
            dimensions: settings.dimensions,
            input_hash: input_hash(&[Input::Text {
                text: text.to_string(),
            }]),
        }
    }
}

/// Ablage bereits berechneter Vektoren.
pub trait EmbeddingCache {
    /// Gespeicherter (normalisierter) Vektor.
    fn get(&self, key: &EmbeddingKey) -> Result<Option<Vec<f32>>, StoreError>;
    /// Speichert einen normalisierten Vektor.
    fn put(&self, key: &EmbeddingKey, vector: &[f32]) -> Result<(), StoreError>;
}

/// Kein Cache.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoEmbeddingCache;

impl EmbeddingCache for NoEmbeddingCache {
    fn get(&self, _key: &EmbeddingKey) -> Result<Option<Vec<f32>>, StoreError> {
        Ok(None)
    }
    fn put(&self, _key: &EmbeddingKey, _vector: &[f32]) -> Result<(), StoreError> {
        Ok(())
    }
}

/// Cache im Speicher (Tests, Prompt-Tester).
#[derive(Debug, Default)]
pub struct MemoryEmbeddingCache(Mutex<HashMap<EmbeddingKey, Vec<f32>>>);

impl MemoryEmbeddingCache {
    /// Anzahl gespeicherter Vektoren.
    pub fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// Ob leer.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl EmbeddingCache for MemoryEmbeddingCache {
    fn get(&self, key: &EmbeddingKey) -> Result<Option<Vec<f32>>, StoreError> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(key)
            .cloned())
    }
    fn put(&self, key: &EmbeddingKey, vector: &[f32]) -> Result<(), StoreError> {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(key.clone(), vector.to_vec());
        Ok(())
    }
}

/// Embedding-Cache über den Ergebnis-Cache der Crawl-Datei (AI-015, Tabelle `ai_cache`). Schlüssel: Aufgabe
/// `embedding`, die Definition des Aufrufers (z. B. Embedding-Prompt und Version, INTEG-166), Anbieter, Modell,
/// Endpunkt, `result_params = {"dimensions": …}` und Inhalts-Hash. Ergebnis ist der normalisierte Vektor als JSON-Array.
pub struct StoredEmbeddingCache<'c> {
    cache: &'c ResultCache<'c>,
    definition_id: String,
    definition_version: String,
    created_at: Timestamp,
}

impl<'c> StoredEmbeddingCache<'c> {
    /// Cache für eine Definition; `created_at` ist der Zeitstempel neuer Einträge.
    pub fn new(
        cache: &'c ResultCache<'c>,
        definition_id: impl Into<String>,
        definition_version: impl Into<String>,
        created_at: Timestamp,
    ) -> Self {
        Self {
            cache,
            definition_id: definition_id.into(),
            definition_version: definition_version.into(),
            created_at,
        }
    }

    /// Schlüssel im Ergebnis-Cache.
    pub fn cache_key(&self, key: &EmbeddingKey) -> CacheKey {
        CacheKey::new(CacheKeyInput {
            task: TASK.into(),
            definition_id: self.definition_id.clone(),
            definition_version: self.definition_version.clone(),
            params: json!({}),
            path: None,
            provider: key.provider.clone(),
            model: key.model.clone(),
            endpoint: key.endpoint.clone(),
            result_params: json!({"dimensions": key.dimensions}),
            input_hash: key.input_hash.clone(),
        })
    }
}

impl EmbeddingCache for StoredEmbeddingCache<'_> {
    fn get(&self, key: &EmbeddingKey) -> Result<Option<Vec<f32>>, StoreError> {
        match self.cache.lookup(&self.cache_key(key))? {
            Lookup::Hit { result, .. } => Ok(serde_json::from_value::<Vec<f32>>(result).ok()),
            Lookup::Miss => Ok(None),
        }
    }
    fn put(&self, key: &EmbeddingKey, vector: &[f32]) -> Result<(), StoreError> {
        self.cache
            .store(
                &self.cache_key(key),
                &AiStatus::Success,
                &json!(vector),
                self.created_at,
            )
            .map(|_| ())
    }
}

/// Ergebnis je Text.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingResult {
    /// `Success`, `Success (cached)`, `Skipped (empty input)` oder ein Fehlerstatus nach AI-020.
    pub status: AiStatus,
    /// Normalisierter Vektor (nur bei Erfolg).
    pub vector: Option<Vec<f32>>,
    /// Endgültiger Fehler (Protokoll, Statustext).
    pub error: Option<FinalError>,
    /// Index des HTTP-Eintrags in [`EmbeddingRun::requests`], aus dem das Ergebnis stammt.
    pub request: Option<usize>,
}

impl EmbeddingResult {
    fn skipped(status: AiStatus) -> Self {
        Self {
            status,
            vector: None,
            error: None,
            request: None,
        }
    }

    fn failed(error: FinalError, request: Option<usize>) -> Self {
        Self {
            status: error.status.clone(),
            vector: None,
            error: Some(error),
            request,
        }
    }
}

/// Ein HTTP-Eintrag (ein Bündel bzw. ein Einzelversuch mit allen Wiederholungen) für das Kostenprotokoll.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingRequestLog {
    /// Indizes der Texte (in der Eingabe von [`embed_texts`]).
    pub texts: Vec<usize>,
    /// Einzelversuch nach Fehler des ganzen Bündels.
    pub single_retry: bool,
    /// Gesendete Requests (1 + Wiederholungen).
    pub attempts: u32,
    pub duration_ms: u64,
    /// Antwortdaten bzw. endgültiger Fehler.
    pub result: Result<EmbeddingReplyMeta, FinalError>,
}

/// Antwortdaten eines erfolgreichen Requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingReplyMeta {
    pub usage: Usage,
    pub provider_request_id: Option<String>,
    pub http_status: u16,
}

impl EmbeddingRequestLog {
    /// Zeile für das Kostenprotokoll (AI-014): mit `crate::cost::log::request_record` und den Preissätzen des
    /// Modells (Ollama: [`crate::adapters::ollama::rates`], Kosten 0).
    pub fn outcome(&self) -> RequestOutcome {
        match &self.result {
            Ok(meta) => {
                let mut outcome = RequestOutcome::success(meta.usage.clone(), 0, self.duration_ms);
                outcome.http_status = Some(meta.http_status);
                outcome.provider_request_id = meta.provider_request_id.clone();
                outcome.retries = self.attempts.saturating_sub(1);
                outcome
            }
            Err(error) => RequestOutcome {
                status: error.status.clone(),
                usage: UsageReport::NoResponse,
                images: 0,
                batch: false,
                http_status: error.http_status,
                error_code: error.error_code.clone(),
                error_message: Some(error.message.clone()),
                refusal_reason: None,
                duration_ms: self.duration_ms,
                provider_request_id: None,
                retries: self.attempts.saturating_sub(1),
                confidence_mismatch: false,
                estimated_input_tokens: None,
            },
        }
    }
}

/// Ergebnis von [`embed_texts`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EmbeddingRun {
    /// Ein Ergebnis je Eingabetext, in Eingabereihenfolge.
    pub results: Vec<EmbeddingResult>,
    /// HTTP-Einträge in Ausführungsreihenfolge (Cache-Treffer und Übersprungene erzeugen keinen).
    pub requests: Vec<EmbeddingRequestLog>,
}

impl EmbeddingRun {
    /// Texte je Request (Bündelgrößen), in Ausführungsreihenfolge.
    pub fn request_sizes(&self) -> Vec<usize> {
        self.requests.iter().map(|r| r.texts.len()).collect()
    }

    /// Anzahl Cache-Treffer.
    pub fn cache_hits(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status == AiStatus::SuccessCached)
            .count()
    }
}

/// Ob nach einem Fehler des ganzen Bündels die Texte einzeln versucht werden: nur bei Fehlern, die an einzelnen Texten
/// liegen können (HTTP 400/413/422, z. B. Kontextlimit, und nicht lesbare Antwort). Schlüssel-, Kontingent-,
/// Verbindungs- und Serverfehler gelten für jeden Text gleich und werden nicht vervielfacht.
pub fn retry_individually(error: &FinalError) -> bool {
    match &error.error {
        ProviderError::InvalidOutput => true,
        ProviderError::Http { status, .. } => matches!(status, 400 | 413 | 422),
        _ => false,
    }
}

fn invalid_output(message: String, http_status: Option<u16>) -> FinalError {
    FinalError {
        error: ProviderError::InvalidOutput,
        status: AiStatus::InvalidOutput,
        http_status,
        error_code: None,
        message,
    }
}

/// Embeddings für `texts`. Je Text: leerer Text → `Skipped (empty input)` ohne Request (INTEG-180); Cache-Treffer →
/// `Success (cached)` ohne Request; sonst gebündelt gesendet. Ein Fehler des ganzen Bündels führt (nach
/// [`retry_individually`]) zu Einzelversuchen; nur die tatsächlich fehlerhaften Texte erhalten einen Fehlerstatus.
/// Erfolgreiche Vektoren werden L2-normalisiert und in `cache` abgelegt. `Err` nur bei Fehlern des Caches.
pub async fn embed_texts(
    provider: &dyn EmbeddingProvider,
    limiter: &Limiter,
    cache: &dyn EmbeddingCache,
    settings: &EmbeddingSettings,
    texts: &[String],
) -> Result<EmbeddingRun, StoreError> {
    let mut results: Vec<Option<EmbeddingResult>> = vec![None; texts.len()];
    let mut keys: Vec<Option<EmbeddingKey>> = vec![None; texts.len()];
    let mut pending = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        if text.trim().is_empty() {
            results[index] = Some(EmbeddingResult::skipped(AiStatus::SkippedEmptyInput));
            continue;
        }
        let key = EmbeddingKey::new(provider, settings, text);
        if let Some(vector) = cache.get(&key)? {
            results[index] = Some(EmbeddingResult {
                status: AiStatus::SuccessCached,
                vector: Some(vector),
                error: None,
                request: None,
            });
            continue;
        }
        keys[index] = Some(key);
        pending.push(index);
    }
    let mut run = Runner {
        provider,
        limiter,
        cache,
        settings,
        texts,
        keys: &keys,
        results: &mut results,
        requests: Vec::new(),
    };
    let batch_size = settings.effective_batch_size(provider);
    for batch in pending.chunks(batch_size) {
        run.batch(batch).await?;
    }
    let requests = run.requests;
    Ok(EmbeddingRun {
        results: results
            .into_iter()
            .map(|r| r.expect("every text has a result"))
            .collect(),
        requests,
    })
}

/// Ruft beim Verlassen eines Eintrags (auch bei Abbruch) [`EmbeddingProvider::end_entry`] auf.
struct EntryGuard<'a> {
    provider: &'a dyn EmbeddingProvider,
    entry: u64,
}

impl Drop for EntryGuard<'_> {
    fn drop(&mut self) {
        self.provider.end_entry(self.entry);
    }
}

struct Runner<'a> {
    provider: &'a dyn EmbeddingProvider,
    limiter: &'a Limiter,
    cache: &'a dyn EmbeddingCache,
    settings: &'a EmbeddingSettings,
    texts: &'a [String],
    keys: &'a [Option<EmbeddingKey>],
    results: &'a mut Vec<Option<EmbeddingResult>>,
    requests: Vec<EmbeddingRequestLog>,
}

impl Runner<'_> {
    async fn batch(&mut self, batch: &[usize]) -> Result<(), StoreError> {
        let Some((error, request)) = self.send(batch, false).await? else {
            return Ok(());
        };
        if batch.len() > 1 && retry_individually(&error) {
            for index in batch {
                if let Some((error, request)) = self.send(std::slice::from_ref(index), true).await?
                {
                    self.results[*index] = Some(EmbeddingResult::failed(error, request));
                }
            }
        } else {
            for index in batch {
                self.results[*index] = Some(EmbeddingResult::failed(error.clone(), request));
            }
        }
        Ok(())
    }

    /// Sendet `batch` (mit Wiederholungen). Bei Erfolg sind die Ergebnisse gesetzt und `None` kommt zurück; sonst der
    /// Fehler des ganzen Requests mit dem Index seines Protokolleintrags (`None`, wenn nichts gesendet wurde); die
    /// Ergebnisse setzt dann der Aufrufer. Ein Text mit unbrauchbarem Vektor erhält
    /// `Error: Invalid output` (nur bei einzelnen Texten; im Bündel wird der Fehler als Bündelfehler gemeldet, damit
    /// die Texte einzeln versucht werden).
    async fn send(
        &mut self,
        batch: &[usize],
        single_retry: bool,
    ) -> Result<Option<(FinalError, Option<usize>)>, StoreError> {
        if let Some(error) = self.provider.blocked() {
            return Ok(Some((error, None)));
        }
        let texts: Vec<String> = batch.iter().map(|i| self.texts[*i].clone()).collect();
        let chars: u64 = texts
            .iter()
            .map(|t| crate::cost::tokens::char_count(t))
            .sum();
        let spec = RequestSpec::new(
            self.provider.provider_id(),
            self.settings.model.clone(),
            crate::cost::tokens::text_tokens(chars),
        );
        let provider = self.provider;
        let model = self.settings.model.as_str();
        let dimensions = self.settings.dimensions;
        let texts_ref = texts.as_slice();
        let entry = provider.begin_entry();
        let _guard = EntryGuard { provider, entry };
        let execution = loop {
            let execution = self
                .limiter
                .run(&spec, |_| {
                    provider.embed_entry(entry, model, texts_ref, dimensions)
                })
                .await;
            match &execution.result {
                Err(error) if provider.is_deferred(error) => provider.settled().await,
                _ => break execution,
            }
        };
        let duration_ms = execution.duration_ms();
        let attempts = execution.attempts;
        let reply = match execution.result {
            Ok(reply) => reply,
            Err(error) => {
                let sent = attempts > 0 && self.provider.was_sent(&error);
                let error = self.provider.finalize(error);
                if !sent {
                    return Ok(Some((error, None)));
                }
                self.requests.push(EmbeddingRequestLog {
                    texts: batch.to_vec(),
                    single_retry,
                    attempts,
                    duration_ms,
                    result: Err(error.clone()),
                });
                return Ok(Some((error, Some(self.requests.len() - 1))));
            }
        };
        let request = self.requests.len();
        self.requests.push(EmbeddingRequestLog {
            texts: batch.to_vec(),
            single_retry,
            attempts,
            duration_ms,
            result: Ok(EmbeddingReplyMeta {
                usage: reply.usage.clone(),
                provider_request_id: reply.provider_request_id.clone(),
                http_status: reply.http_status,
            }),
        });
        if reply.vectors.len() != batch.len() {
            return Ok(Some((
                invalid_output(
                    format!(
                        "{} embeddings for {} texts",
                        reply.vectors.len(),
                        batch.len()
                    ),
                    Some(reply.http_status),
                ),
                Some(request),
            )));
        }
        let normalized: Vec<Result<Vec<f32>, VectorError>> = reply
            .vectors
            .iter()
            .map(|v| l2_normalize(v, dimensions))
            .collect();
        if batch.len() > 1 {
            if let Some(Err(error)) = normalized.iter().find(|n| n.is_err()) {
                return Ok(Some((
                    invalid_output(error.to_string(), Some(reply.http_status)),
                    Some(request),
                )));
            }
        }
        for (index, vector) in batch.iter().zip(normalized) {
            self.results[*index] = Some(match vector {
                Ok(vector) => {
                    if let Some(key) = &self.keys[*index] {
                        self.cache.put(key, &vector)?;
                    }
                    EmbeddingResult {
                        status: AiStatus::Success,
                        vector: Some(vector),
                        error: None,
                        request: Some(request),
                    }
                }
                Err(error) => EmbeddingResult::failed(
                    invalid_output(error.to_string(), Some(reply.http_status)),
                    Some(request),
                ),
            });
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_yields_unit_length_and_checks_dimensions() {
        let v = l2_normalize(&[3.0, 4.0], None).unwrap();
        assert_eq!(v, vec![0.6, 0.8]);
        assert!((l2_norm(&v) - 1.0).abs() <= NORM_TOLERANCE);
        assert_eq!(
            l2_normalize(&[1.0, 0.0], Some(3)),
            Err(VectorError::Dimensions {
                expected: 3,
                actual: 2
            })
        );
        assert_eq!(
            l2_normalize(&[0.0, 0.0], None),
            Err(VectorError::NotNormalizable)
        );
        assert_eq!(l2_normalize(&[], None), Err(VectorError::Empty));
        assert_eq!(
            l2_normalize(&[f64::NAN, 1.0], None),
            Err(VectorError::NotNormalizable)
        );
    }
}

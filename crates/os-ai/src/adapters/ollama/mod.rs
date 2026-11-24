//! Adapter Ollama (AI-006): lokale Modelle über einen Ollama-Server.
//!
//! * `POST /api/chat` (ohne Streaming) für Text- und Schema-Aufgaben; das JSON-Schema steht im Feld `format`, Bilder
//!   als Base64 im Feld `images` der Nutzernachricht. Die Antwort wird lokal validiert (AI-021, über
//!   [`AdapterReply::into_response`] bzw. [`crate::schema::run_with_repair`]).
//! * `POST /api/embed` für Embeddings ([`crate::embeddings::EmbeddingProvider`], AI-008), mit `truncate: false`,
//!   weil keine automatische Kürzung auf das Kontextlimit stattfindet (INTEG-180).
//! * `GET /api/tags` für die Modellliste; Fähigkeiten kommen aus dem Modellkatalog (AI-001, AI-010), ohne Katalog gilt
//!   [`default_capabilities`].
//!
//! Kosten sind 0 ([`rates`]); Tokens werden trotzdem protokolliert (`prompt_eval_count` → Eingabe, `eval_count` →
//! Ausgabe). Server-URL: die gemeinsame Einstellung des Ollama-Kontos ([`OllamaConfig::from_app_settings`], INTEG-160);
//! Standard 1 gleichzeitige Anfrage und 300 s Timeout (INTEG-005, INTEG-179) liefern die App-Einstellungen bzw.
//! [`default_limits`].
//!
//! **Nicht erreichbarer Server (AI-006, AI-020, K4-15):** Scheitert ein Versuch mit einem Verbindungsfehler, übernimmt
//! dieser Eintrag die Prüfung des Servers; alle übrigen Ollama-Einträge (Chat und Embeddings) senden in dieser Zeit
//! nichts und warten auf seinen Ausgang. Endet er nach 1 + 5 Versuchen (INTEG-005) endgültig mit dem Verbindungsfehler,
//! scheitern alle offenen und weiteren Einträge des Laufs ohne Request mit `Error: Connection failed`,
//! `Error Code = transport/connect`; Zusatztext ist die kanonische Meldung `Server not reachable (<URL>)` mit der URL als
//! eigenem Feld ([`UnreachableNotice`], Anzeige nach INTEG-160 über i18n der Oberfläche). Gelingt eine Wiederholung,
//! laufen die wartenden Einträge normal weiter. Andere Anbieter laufen in jedem Fall weiter.
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;
use serde_json::{json, Map, Value};
use tokio::sync::watch;

use crate::adapters::anthropic::shared::{elapsed_ms, http_failure, opt_u64, raw_from_text};
pub use crate::adapters::anthropic::shared::{image_input, AdapterReply};
use crate::cost::{PriceSource, Rates};
use crate::embeddings::{EmbedFuture, EmbeddingBatchReply, EmbeddingProvider};
use crate::limits::{
    AttemptFailure, Execution, FinalError, Limiter, ProviderLimits, RequestSpec, TransportCause,
};
use crate::provider::{
    AiProvider, Capabilities, CapabilitySource, Input, Model, ProviderError, ProviderFuture,
    Request, Usage,
};
use crate::schema::{RawAnswer, RepairRequest};
use os_model::ai::AiStatus;
use os_model::config::AiAppSettings;

/// Anbieterkennung (Schlüssel in `AiAppSettings::provider_endpoints`, Limiter, Protokoll).
pub const PROVIDER_ID: &str = "ollama";
/// Endpunkt im Protokoll für `/api/chat` (Preisschlüssel-Endpunkt nach AI-010; Ollama hat keine Preistabelle).
pub const ENDPOINT: &str = "chat-completions";
/// Endpunkt im Protokoll für `/api/embed`.
pub const EMBEDDINGS_ENDPOINT: &str = "embeddings";
/// Standard-Server-URL (INTEG-160).
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";
/// Standard der gleichzeitigen Anfragen (AI-006, INTEG-005 [M4]: lokal rechenbegrenzt).
pub const DEFAULT_CONCURRENT_REQUESTS: u16 = 1;
/// Zulässiger Bereich der gleichzeitigen Anfragen im Reiter „Erweitert“ (INTEG-005, allgemeiner Bereich 1–50).
pub const CONCURRENT_REQUESTS_RANGE: std::ops::RangeInclusive<u16> = 1..=50;
/// Standard-Request-Timeout in Sekunden (INTEG-179, OpenSpider-Festlegung).
pub const DEFAULT_TIMEOUT_SECONDS: u16 = 300;
/// Fehlercode eines Eintrags, der wegen eines bereits als nicht erreichbar erkannten Servers nicht gesendet wurde.
pub const UNREACHABLE_CODE: &str = "transport/connect";

/// Kanonische Meldung bei nicht erreichbarem Server (englisch, ohne Präfix „Error:“): `Server not reachable (<URL>)`.
/// Die sprachabhängige Anzeige nach INTEG-160 übernimmt die Oberfläche (i18n) anhand von [`UnreachableNotice::url`].
pub fn unreachable_message(base_url: &str) -> String {
    format!("Server not reachable ({})", base_url.trim_end_matches('/'))
}

/// Zusatztext zu `Error: Connection failed`, wenn der Ollama-Server nicht erreichbar ist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreachableNotice {
    /// Server-URL (ohne abschließenden Schrägstrich).
    pub url: String,
    /// Kanonische Meldung [`unreachable_message`].
    pub message: String,
}

impl UnreachableNotice {
    fn new(base_url: &str) -> Self {
        Self {
            url: base_url.trim_end_matches('/').to_string(),
            message: unreachable_message(base_url),
        }
    }
}

/// Grenzen nach INTEG-005/INTEG-179: 1 gleichzeitige Anfrage, 300 s Timeout. Dieselben Werte setzt
/// `AiAppSettings::default()` für `ollama`.
pub fn default_limits() -> ProviderLimits {
    ProviderLimits {
        concurrent_requests: u32::from(DEFAULT_CONCURRENT_REQUESTS),
        timeout: Duration::from_secs(u64::from(DEFAULT_TIMEOUT_SECONDS)),
    }
}

/// Preissätze: Kosten 0 (AI-006). Preisquelle `table` (feste Regel statt Preistabelle; Auslegung K03).
pub fn rates() -> Rates {
    Rates::free(PriceSource::Table)
}

/// Fähigkeiten ohne Katalogeintrag: Text, Structured Output, Embeddings, Local; Vision nur laut Katalog (AI-001,
/// konservativ).
pub fn default_capabilities() -> Capabilities {
    Capabilities {
        text: true,
        structured_output: true,
        embeddings: true,
        local: true,
        ..Capabilities::default()
    }
}

/// Verbindungsdaten. Ollama braucht keinen Schlüssel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaConfig {
    /// Server-URL ohne `/api` (Standard [`DEFAULT_BASE_URL`]; im Test `…/ollama`).
    pub base_url: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.into(),
        }
    }
}

impl OllamaConfig {
    /// Standard-Server-URL.
    pub fn new() -> Self {
        Self::default()
    }

    /// Andere Server-URL.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    /// Die gemeinsame Server-URL aus den App-Einstellungen (`provider_endpoints["ollama"].base_url`), sonst der
    /// Standard. 04 (INTEG-160) und 08 lesen dieselbe Einstellung (AI-006); eine leere Angabe gilt als nicht gesetzt.
    pub fn from_app_settings(settings: &AiAppSettings) -> Self {
        match settings
            .provider_endpoints
            .get(PROVIDER_ID)
            .and_then(|endpoint| endpoint.base_url.as_deref())
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            Some(url) => Self::default().with_base_url(url),
            None => Self::default(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url.trim_end_matches('/'))
    }
}

/// Ein Modell aus `GET /api/tags`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaModel {
    /// Modellname (`name`, z. B. `llama3.2:latest`).
    pub name: String,
    pub family: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

impl OllamaModel {
    /// Liest einen Eintrag von `models`.
    pub fn from_json(entry: &Value) -> Option<Self> {
        let name = entry
            .get("name")
            .or_else(|| entry.get("model"))?
            .as_str()?
            .to_string();
        let details = entry.get("details");
        let text = |key: &str| {
            details
                .and_then(|d| d.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        Some(Self {
            name,
            family: text("family"),
            parameter_size: text("parameter_size"),
            quantization_level: text("quantization_level"),
        })
    }
}

/// Fehler beim Verbinden (INTEG-160): der Text ist der Verbindungsstatus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionError {
    /// Server nicht erreichbar (Verbindung abgelehnt, Name nicht auflösbar).
    Unreachable { message: String },
    /// Server antwortete, aber nicht mit einer Modellliste (Text nach AI-020).
    Failed { message: String },
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable { message } | Self::Failed { message } => f.write_str(message),
        }
    }
}

/// Zustand der Erreichbarkeit eines Adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gate {
    /// Keine bekannte Störung.
    Open,
    /// Der Eintrag mit dieser Nummer wiederholt nach einem Verbindungsfehler; alle anderen warten.
    Probing(u64),
    /// Server nicht erreichbar: für den Rest des Laufs kein Request.
    Closed,
}

/// Merker „Server nicht erreichbar“ eines Adapters für einen Lauf (AI-006): Während ein Eintrag nach einem
/// Verbindungsfehler wiederholt, senden die anderen nichts; nach seinem endgültigen Verbindungsfehler geht kein
/// weiterer Ollama-Request hinaus, und alle offenen Einträge erhalten denselben Zusatztext.
#[derive(Debug, Clone)]
pub struct UnreachableLatch(Arc<LatchInner>);

#[derive(Debug)]
struct LatchInner {
    notice: Mutex<Option<UnreachableNotice>>,
    gate: watch::Sender<Gate>,
}

impl Default for UnreachableLatch {
    fn default() -> Self {
        Self(Arc::new(LatchInner {
            notice: Mutex::new(None),
            gate: watch::Sender::new(Gate::Open),
        }))
    }
}

impl UnreachableLatch {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<UnreachableNotice>> {
        self.0.notice.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Setzt den Merker; ein früherer Zusatztext bleibt erhalten. Wartende Einträge scheitern ohne Request.
    pub fn trip(&self, notice: UnreachableNotice) {
        self.lock().get_or_insert(notice);
        self.0.gate.send_replace(Gate::Closed);
    }

    /// Der Zusatztext, falls gesetzt.
    pub fn notice(&self) -> Option<UnreachableNotice> {
        self.lock().clone()
    }

    /// Die kanonische Meldung, falls gesetzt.
    pub fn message(&self) -> Option<String> {
        self.notice().map(|n| n.message)
    }

    /// Ob gesetzt.
    pub fn is_tripped(&self) -> bool {
        self.lock().is_some()
    }

    /// Ob gerade ein Eintrag den Server nach einem Verbindungsfehler prüft (die anderen warten).
    pub fn is_probing(&self) -> bool {
        matches!(*self.0.gate.borrow(), Gate::Probing(_))
    }

    /// Hebt den Merker auf (neuer Lauf, Server wieder gestartet).
    pub fn reset(&self) {
        *self.lock() = None;
        self.0.gate.send_replace(Gate::Open);
    }

    fn gate(&self) -> Gate {
        *self.0.gate.borrow()
    }

    /// Eintrag `entry` übernimmt die Prüfung, falls niemand prüft und der Server nicht als gesperrt gilt.
    fn start_probe(&self, entry: u64) {
        self.0.gate.send_if_modified(|gate| {
            if *gate == Gate::Open {
                *gate = Gate::Probing(entry);
                true
            } else {
                false
            }
        });
    }

    /// Eintrag `entry` beendet seine Prüfung ohne Sperre (Antwort erhalten oder anderer Fehler).
    fn end_probe(&self, entry: u64) {
        self.0.gate.send_if_modified(|gate| {
            if *gate == Gate::Probing(entry) {
                *gate = Gate::Open;
                true
            } else {
                false
            }
        });
    }

    /// Wartet, bis keine Prüfung mehr läuft.
    async fn settled(&self) {
        let mut rx = self.0.gate.subscribe();
        let _ = rx.wait_for(|gate| !matches!(gate, Gate::Probing(_))).await;
    }
}

/// Beendet beim Verlassen eines Eintrags (auch bei Abbruch) eine noch offene Prüfung dieses Eintrags, damit wartende
/// Einträge nicht hängen bleiben.
struct ProbeGuard<'a> {
    latch: &'a UnreachableLatch,
    entry: u64,
}

impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        self.latch.end_probe(self.entry);
    }
}

/// Adapter für einen Ollama-Server; eine Instanz je Lauf (Merker „nicht erreichbar“).
pub struct OllamaAdapter {
    client: reqwest::Client,
    config: OllamaConfig,
    capabilities: BTreeMap<String, Capabilities>,
    latch: UnreachableLatch,
    next_entry: AtomicU64,
}

impl std::fmt::Debug for OllamaAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OllamaAdapter")
            .field("config", &self.config)
            .field("unreachable", &self.latch.message())
            .finish_non_exhaustive()
    }
}

impl OllamaAdapter {
    /// Adapter mit dem HTTP-Client des Aufrufers (Host-Überschreibung, Proxy).
    pub fn new(client: reqwest::Client, config: OllamaConfig) -> Self {
        Self {
            client,
            config,
            capabilities: BTreeMap::new(),
            latch: UnreachableLatch::default(),
            next_entry: AtomicU64::new(1),
        }
    }

    /// Fähigkeiten je Modellname aus dem Modellkatalog (AI-010, M4-K04); nicht genannte Modelle erhalten
    /// [`default_capabilities`].
    pub fn with_capabilities(mut self, capabilities: BTreeMap<String, Capabilities>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Die Konfiguration.
    pub fn config(&self) -> &OllamaConfig {
        &self.config
    }

    /// Merker „Server nicht erreichbar“ dieses Laufs.
    pub fn unreachable(&self) -> &UnreachableLatch {
        &self.latch
    }

    /// Kanonische Meldung `Server not reachable (<URL>)` für die Server-URL dieses Adapters.
    pub fn unreachable_message(&self) -> String {
        unreachable_message(&self.config.base_url)
    }

    /// Nummer eines neuen Eintrags (für die Prüfung nach Verbindungsfehlern).
    fn begin_entry(&self) -> u64 {
        self.next_entry.fetch_add(1, Ordering::Relaxed)
    }

    /// Fähigkeiten eines Modells.
    pub fn capabilities_of(&self, model: &str) -> Capabilities {
        let mut caps = self
            .capabilities
            .get(model)
            .copied()
            .unwrap_or_else(default_capabilities);
        caps.local = true;
        caps
    }

    /// Gemeinsame Modellbeschreibung (AI-001).
    pub fn to_model(&self, name: &str) -> Model {
        Model {
            provider: PROVIDER_ID.into(),
            id: name.into(),
            endpoint: ENDPOINT.into(),
            capabilities: self.capabilities_of(name),
            capability_source: CapabilitySource::ModelCatalog,
            reasoning_levels: Vec::new(),
        }
    }

    /// Lädt die Modellliste (`GET /api/tags`) – ein Versuch, ohne Wiederholung (Verbinden nach INTEG-160).
    pub async fn list_models(&self) -> Result<Vec<OllamaModel>, AttemptFailure> {
        let response = self
            .client
            .get(self.config.url("/api/tags"))
            .send()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(http_failure(
                status.as_u16(),
                error_message(&body, status),
                None,
            ));
        }
        Ok(body
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(OllamaModel::from_json)
            .collect())
    }

    /// „Verbinden“ (INTEG-160): Modellliste oder Verbindungsstatus. Ein nicht erreichbarer Server ergibt
    /// [`ConnectionError::Unreachable`] mit [`unreachable_message`].
    pub async fn connect(&self) -> Result<Vec<OllamaModel>, ConnectionError> {
        self.list_models().await.map_err(|failure| {
            if is_unreachable(&failure) {
                ConnectionError::Unreachable {
                    message: self.unreachable_message(),
                }
            } else {
                ConnectionError::Failed {
                    message: failure.error.to_string(),
                }
            }
        })
    }

    /// Request-Körper für `POST /api/chat`.
    pub fn request_body(&self, request: &Request, repair: Option<&RepairRequest>) -> Value {
        let mut text: Vec<String> = Vec::new();
        let mut images: Vec<String> = Vec::new();
        for input in &request.input {
            match input {
                Input::Text { text: t } => text.push(t.clone()),
                Input::Image { base64, .. } => images.push(base64.clone()),
            }
        }
        if let Some(repair) = repair {
            text.push(repair.instruction());
        }
        let mut user = Map::new();
        user.insert("role".into(), json!("user"));
        user.insert("content".into(), json!(text.join("\n\n")));
        if !images.is_empty() {
            user.insert("images".into(), json!(images));
        }
        let mut messages = Vec::new();
        if !request.instructions.is_empty() {
            messages.push(json!({"role": "system", "content": request.instructions}));
        }
        messages.push(Value::Object(user));
        let mut body = Map::new();
        body.insert("model".into(), json!(request.model.id));
        body.insert("messages".into(), Value::Array(messages));
        body.insert("stream".into(), json!(false));
        if let Some(schema) = &request.response_schema {
            body.insert("format".into(), schema.clone());
        }
        let mut options = Map::new();
        if let Some(temperature) = request.temperature {
            options.insert("temperature".into(), json!(temperature));
        }
        if let Some(max_tokens) = request.max_tokens {
            options.insert("num_predict".into(), json!(max_tokens));
        }
        if !options.is_empty() {
            body.insert("options".into(), Value::Object(options));
        }
        Value::Object(body)
    }

    /// Ein Versuch an `/api/chat` (ohne Wiederholung, ohne Merkerprüfung). Fehler als [`AttemptFailure`] für
    /// [`Limiter::run`]; schemawidrige oder nicht lesbare Antworten kommen als `Ok` mit `RawAnswer::Unparsable` bzw.
    /// `Json` und werden vom Aufrufer validiert.
    pub async fn send(
        &self,
        request: &Request,
        repair: Option<&RepairRequest>,
    ) -> Result<AdapterReply, AttemptFailure> {
        let body = self.request_body(request, repair);
        let structured = request.response_schema.is_some();
        let started = Instant::now();
        let response = self
            .client
            .post(self.config.url("/api/chat"))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let duration_ms = elapsed_ms(started);
        let parsed: Result<Value, _> = serde_json::from_slice(&bytes);
        if !status.is_success() {
            let body = parsed.unwrap_or(Value::Null);
            return Err(http_failure(
                status.as_u16(),
                error_message(&body, status),
                None,
            ));
        }
        let body = match parsed {
            Ok(body) => body,
            Err(error) => {
                return Ok(AdapterReply {
                    raw: RawAnswer::Unparsable {
                        detail: error.to_string(),
                    },
                    usage: Usage::default(),
                    refusal_reason: None,
                    stop_reason: None,
                    provider_request_id: request_id(&headers),
                    http_status: status.as_u16(),
                    duration_ms,
                    structured,
                    output_limit: None,
                    reported_cost: None,
                })
            }
        };
        Ok(parse_chat(
            &body,
            &headers,
            status.as_u16(),
            duration_ms,
            structured,
        ))
    }

    /// Fehler eines nicht gesendeten Eintrags, wenn der Server bereits als nicht erreichbar gilt. Bewusst eine nicht
    /// wiederholbare Fehlerklasse, damit der Limiter nicht wartet; [`OllamaAdapter::finalize`] macht daraus den
    /// Endstatus.
    fn latched_failure(&self) -> Option<AttemptFailure> {
        self.latch
            .is_tripped()
            .then(|| AttemptFailure::from(ProviderError::Policy).with_error_code(LATCHED_SENTINEL))
    }

    /// Versuch eines Eintrags, der wartet, weil ein anderer Eintrag gerade den Server prüft: nicht gesendet, nicht
    /// wiederholbar; der Aufrufer wartet auf den Ausgang der Prüfung und beginnt den Eintrag neu.
    fn deferred_failure() -> AttemptFailure {
        AttemptFailure::from(ProviderError::Policy).with_error_code(DEFERRED_SENTINEL)
    }

    /// Ein Versuch des Eintrags `entry` mit Merker und Prüfung: gesperrt → nicht gesendet; ein anderer Eintrag prüft →
    /// zurückgestellt; sonst gesendet (`sent` zählt mit). Ein Verbindungsfehler macht diesen Eintrag zum prüfenden,
    /// jede andere Antwort beendet seine Prüfung.
    async fn guarded<T, Fut>(
        &self,
        entry: u64,
        sent: &AtomicU32,
        attempt: Fut,
    ) -> Result<T, AttemptFailure>
    where
        Fut: std::future::Future<Output = Result<T, AttemptFailure>>,
    {
        if let Some(failure) = self.latched_failure() {
            return Err(failure);
        }
        if matches!(self.latch.gate(), Gate::Probing(owner) if owner != entry) {
            return Err(Self::deferred_failure());
        }
        sent.fetch_add(1, Ordering::Relaxed);
        let result = attempt.await;
        match &result {
            Err(failure) if is_unreachable(failure) => self.latch.start_probe(entry),
            _ => self.latch.end_probe(entry),
        }
        result
    }

    /// Endgültiger Fehler eines Eintrags: Ein nicht erreichbarer Server (Verbindung abgelehnt bzw. Name nicht
    /// auflösbar, nach allen Wiederholungen) setzt den Merker; dieser und alle weiteren Einträge erhalten
    /// `Error: Connection failed` (AI-020) mit der Meldung [`unreachable_message`] und `Error Code = transport/connect`
    /// bzw. `transport/dns`.
    pub fn finalize(&self, error: FinalError) -> FinalError {
        let latched = error.error_code.as_deref() == Some(LATCHED_SENTINEL);
        let unreachable = matches!(error.error, ProviderError::Transport { .. })
            && matches!(
                error.error_code.as_deref(),
                Some("transport/connect" | "transport/dns")
            );
        if !latched && !unreachable {
            return error;
        }
        let notice = UnreachableNotice::new(&self.config.base_url);
        let message = notice.message.clone();
        self.latch.trip(notice);
        FinalError {
            error: ProviderError::Transport {
                message: message.clone(),
            },
            status: AiStatus::ConnectionFailed,
            http_status: None,
            error_code: Some(if latched {
                UNREACHABLE_CODE.to_string()
            } else {
                error.error_code.unwrap_or_else(|| UNREACHABLE_CODE.into())
            }),
            message,
        }
    }

    /// Ob ein endgültiger Fehler „Server nicht erreichbar“ ist.
    pub fn is_unreachable_error(&self, error: &FinalError) -> bool {
        error.status == AiStatus::ConnectionFailed && error.message == self.unreachable_message()
    }

    /// Zusatztext eines endgültigen Fehlers bei nicht erreichbarem Server (Meldung und URL); sonst `None`.
    pub fn unreachable_notice(&self, error: &FinalError) -> Option<UnreachableNotice> {
        self.is_unreachable_error(error)
            .then(|| UnreachableNotice::new(&self.config.base_url))
    }

    /// Kanonische Textzeile eines endgültigen Fehlers für Protokoll und Tooltip: Statuswert nach AI-020, bei nicht
    /// erreichbarem Server mit Zusatztext (`Error: Connection failed: Server not reachable (<URL>)`). Die Statusspalte
    /// selbst trägt nur den Statuswert (`error.status`); die sprachabhängige Anzeige nach INTEG-160 baut die Oberfläche
    /// aus [`OllamaAdapter::unreachable_notice`].
    pub fn status_text(&self, error: &FinalError) -> String {
        match self.unreachable_notice(error) {
            Some(notice) => format!("{}: {}", error.status, notice.message),
            None => error.status.to_string(),
        }
    }

    /// Ein Eintrag mit allen Versuchen über den Limiter (INTEG-005, INTEG-179): Ist der Server bereits als nicht
    /// erreichbar erkannt, geht kein Request hinaus; prüft gerade ein anderer Eintrag den Server nach einem
    /// Verbindungsfehler, wartet dieser Eintrag ohne Request auf dessen Ausgang. `attempts` zählt nur tatsächlich
    /// gesendete Requests. Die Antwort validiert der Aufrufer ([`AdapterReply::into_response`] bzw.
    /// [`crate::schema::run_with_repair`] mit `repair`).
    pub async fn run(
        &self,
        limiter: &Limiter,
        request: &Request,
        repair: Option<&RepairRequest>,
        estimated_input_tokens: u64,
    ) -> Execution<AdapterReply> {
        let spec = RequestSpec::new(
            PROVIDER_ID,
            request.model.id.clone(),
            estimated_input_tokens,
        );
        let entry = self.begin_entry();
        let _guard = ProbeGuard {
            latch: &self.latch,
            entry,
        };
        let sent = AtomicU32::new(0);
        loop {
            let mut execution = limiter
                .run(&spec, |_| {
                    self.guarded(entry, &sent, self.send(request, repair))
                })
                .await;
            let Err(error) = execution.result else {
                return execution;
            };
            if is_deferred(&error) {
                self.latch.settled().await;
                continue;
            }
            execution.result = Err(self.finalize(error));
            // Versuche, die wegen Merker oder Prüfung nicht hinausgingen, zählen nicht als Request (`Retries`).
            let sent = sent.load(Ordering::Relaxed);
            let sent_usize = usize::try_from(sent).unwrap_or(usize::MAX);
            execution.attempts = sent;
            execution.attempt_starts.truncate(sent_usize);
            return execution;
        }
    }

    /// Request-Körper für `POST /api/embed`.
    pub fn embed_body(&self, model: &str, texts: &[String], dimensions: Option<u32>) -> Value {
        let mut body = Map::new();
        body.insert("model".into(), json!(model));
        body.insert("input".into(), json!(texts));
        body.insert("truncate".into(), json!(false));
        if let Some(dimensions) = dimensions {
            body.insert("dimensions".into(), json!(dimensions));
        }
        Value::Object(body)
    }

    async fn embed_once(
        &self,
        model: &str,
        texts: &[String],
        dimensions: Option<u32>,
    ) -> Result<EmbeddingBatchReply, AttemptFailure> {
        let started = Instant::now();
        let response = self
            .client
            .post(self.config.url("/api/embed"))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(self.embed_body(model, texts, dimensions).to_string())
            .send()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(http_failure(
                status.as_u16(),
                error_message(&body, status),
                None,
            ));
        }
        let vectors = body
            .get("embeddings")
            .and_then(Value::as_array)
            .ok_or_else(|| AttemptFailure::from(ProviderError::InvalidOutput))?
            .iter()
            .map(crate::embeddings::vector_from_json)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| AttemptFailure::from(ProviderError::InvalidOutput))?;
        Ok(EmbeddingBatchReply {
            vectors,
            usage: Usage {
                input_tokens: opt_u64(body.get("prompt_eval_count")),
                ..Usage::default()
            },
            provider_request_id: request_id(&headers),
            http_status: status.as_u16(),
            duration_ms: elapsed_ms(started),
        })
    }
}

/// Fehlercode-Markierung eines wegen des Merkers nicht gesendeten Eintrags (nur intern zwischen Versuch und
/// [`OllamaAdapter::finalize`]).
const LATCHED_SENTINEL: &str = "ollama/unreachable-latched";
/// Fehlercode-Markierung eines zurückgestellten Versuchs (ein anderer Eintrag prüft den Server).
const DEFERRED_SENTINEL: &str = "ollama/deferred";

fn is_deferred(error: &FinalError) -> bool {
    error.error_code.as_deref() == Some(DEFERRED_SENTINEL)
}

fn is_unreachable(failure: &AttemptFailure) -> bool {
    matches!(failure.error, ProviderError::Transport { .. })
        && matches!(
            failure.cause,
            Some(TransportCause::Connect | TransportCause::Dns)
        )
}

/// Fehlermeldung einer Ollama-Antwort (`{"error": "…"}`), sonst der kanonische Grund.
fn error_message(body: &Value, status: reqwest::StatusCode) -> String {
    body.get("error")
        .and_then(|e| {
            e.as_str()
                .or_else(|| e.get("message").and_then(Value::as_str))
        })
        .map(str::to_string)
        .unwrap_or_else(|| status.canonical_reason().unwrap_or("").to_string())
}

/// Anbieter-Request-ID (AI-001, Nachtrag Q00-N2): Ollama liefert keine (leer).
fn request_id(_headers: &HeaderMap) -> Option<String> {
    None
}

/// Einheitliche Tokenzahlen (AI-001): `prompt_eval_count` → Eingabe, `eval_count` → Ausgabe; Cache-Felder meldet
/// Ollama nicht (`null`).
pub fn usage_from_json(body: &Value) -> Usage {
    Usage {
        input_tokens: opt_u64(body.get("prompt_eval_count")),
        output_tokens: opt_u64(body.get("eval_count")),
        cached_input_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: None,
    }
}

/// Erfolgsantwort von `POST /api/chat` → [`AdapterReply`]. `done_reason = "length"` gilt als abgeschnitten (bei
/// Schema-Aufgaben nicht lesbar). Ollama kennt keine Verweigerung als eigene Antwortform.
fn parse_chat(
    body: &Value,
    headers: &HeaderMap,
    http_status: u16,
    duration_ms: u64,
    structured: bool,
) -> AdapterReply {
    let done_reason = body
        .get("done_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    // `done_reason: "length"` → Abbruch an der Ausgabegrenze für jede Aufgabe (AI-006, Nachtrag Q00-H1; Feldname und
    // Wert nach der öffentlichen Ollama-API-Doku zu `/api/chat`).
    let output_limit = (done_reason.as_deref() == Some("length")).then(|| "length".to_string());
    let raw = match body.pointer("/message/content") {
        Some(Value::String(text)) => {
            raw_from_text(text.clone(), structured, output_limit.as_deref())
        }
        _ => RawAnswer::Unparsable {
            detail: "response without message.content".into(),
        },
    };
    AdapterReply {
        raw,
        usage: usage_from_json(body),
        refusal_reason: None,
        stop_reason: done_reason,
        provider_request_id: request_id(headers),
        http_status,
        duration_ms,
        structured,
        output_limit,
        reported_cost: None,
    }
}

impl AiProvider for OllamaAdapter {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async move {
            self.list_models()
                .await
                .map(|models| models.iter().map(|m| self.to_model(&m.name)).collect())
                .map_err(|failure| failure.error)
        })
    }

    /// Ein Versuch ohne Limiter. Bei bereits erkanntem nicht erreichbarem Server geht kein Request hinaus
    /// (Transportfehler mit der Meldung nach INTEG-160).
    fn execute(&self, request: Request) -> ProviderFuture<'_, crate::provider::Response> {
        Box::pin(async move {
            if let Some(message) = self.latch.message() {
                return Err(ProviderError::Transport { message });
            }
            // Ohne Limiter keine Wiederholung: ein zurückgestellter Versuch wartet auf die laufende Prüfung.
            self.latch.settled().await;
            if let Some(message) = self.latch.message() {
                return Err(ProviderError::Transport { message });
            }
            let reply = self
                .send(&request, None)
                .await
                .map_err(|failure| failure.error)?;
            reply.into_response(request.response_schema.as_ref())
        })
    }
}

impl EmbeddingProvider for OllamaAdapter {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn endpoint_url(&self) -> String {
        self.config.url("/api/embed")
    }

    fn embed<'a>(
        &'a self,
        model: &'a str,
        texts: &'a [String],
        dimensions: Option<u32>,
    ) -> EmbedFuture<'a> {
        let entry = self.begin_entry();
        self.embed_entry(entry, model, texts, dimensions)
    }

    fn begin_entry(&self) -> u64 {
        OllamaAdapter::begin_entry(self)
    }

    fn embed_entry<'a>(
        &'a self,
        entry: u64,
        model: &'a str,
        texts: &'a [String],
        dimensions: Option<u32>,
    ) -> EmbedFuture<'a> {
        Box::pin(async move {
            let sent = AtomicU32::new(0);
            self.guarded(entry, &sent, self.embed_once(model, texts, dimensions))
                .await
        })
    }

    fn end_entry(&self, entry: u64) {
        self.latch.end_probe(entry);
    }

    fn is_deferred(&self, error: &FinalError) -> bool {
        is_deferred(error)
    }

    fn settled(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(self.latch.settled())
    }

    fn was_sent(&self, error: &FinalError) -> bool {
        error.error_code.as_deref() != Some(LATCHED_SENTINEL)
    }

    fn finalize(&self, error: FinalError) -> FinalError {
        OllamaAdapter::finalize(self, error)
    }

    fn blocked(&self) -> Option<FinalError> {
        self.latch
            .notice()
            .map(|notice| notice.message)
            .map(|message| FinalError {
                error: ProviderError::Transport {
                    message: message.clone(),
                },
                status: AiStatus::ConnectionFailed,
                http_status: None,
                error_code: Some(UNREACHABLE_CODE.into()),
                message,
            })
    }

    fn rates(&self) -> Option<Rates> {
        Some(rates())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreachable_message_names_the_url_without_trailing_slash() {
        assert_eq!(
            unreachable_message("http://localhost:11434/"),
            "Server not reachable (http://localhost:11434)"
        );
    }

    #[test]
    fn usage_maps_eval_counts_and_leaves_cache_fields_null() {
        let usage = usage_from_json(&json!({"prompt_eval_count": 12, "eval_count": 3}));
        assert_eq!(usage.input_tokens, Some(12));
        assert_eq!(usage.output_tokens, Some(3));
        assert_eq!(usage.cached_input_tokens, None);
        assert_eq!(usage.cache_write_tokens, None);
        assert_eq!(usage_from_json(&json!({})).input_tokens, None);
    }
}

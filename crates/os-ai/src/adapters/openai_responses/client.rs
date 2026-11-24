//! Gemeinsame HTTP-Schicht der OpenAI-Adapter (Responses AI-003, Decisions AI-004): Konfiguration mit Basis-URL
//! (EU-Datenresidenz als Parameter, AI-019), Bearer-Token, optionale Organisation/Projekt-Header, Fehlerklassen,
//! gemeinsamer Kontostand „Kontingent erschöpft“ und Auswertung der Rate-Limit-Header über
//! [`Limiter::observe_headers`] (AI-017).
use std::sync::{Arc, Mutex};

use os_model::ai::AiStatus;
use reqwest::header::{HeaderMap, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

use crate::cost::{RequestOutcome, UsageReport};
use crate::limits::{parse_retry_after, AttemptFailure, Execution, Limiter};
use crate::provider::{ProviderError, Usage};

/// Anbieterkennung beider OpenAI-Adapter (gemeinsame Ratenbegrenzung, gemeinsames Kontingent).
pub const PROVIDER_ID: &str = "openai";
/// Standard-Basis-URL einschließlich API-Version.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// API-Schlüssel. `Debug` zeigt den Wert nie.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
    /// Ob kein Schlüssel hinterlegt ist (→ `Error: Missing API key`, kein Request).
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
    fn expose(&self) -> &str {
        self.0.trim()
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey([REDACTED])")
    }
}

/// Verbindungsdaten eines OpenAI-Kontos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiConfig {
    /// Basis-URL einschließlich `/v1` (global `https://api.openai.com/v1`, EU `https://eu.api.openai.com/v1`; die
    /// Auswahl trifft die Richtlinie aus AI-019, hier nur Parameter).
    pub base_url: String,
    pub api_key: ApiKey,
    /// Header `OpenAI-Organization` (optional).
    pub organization: Option<String>,
    /// Header `OpenAI-Project` (optional).
    pub project: Option<String>,
}

impl OpenAiConfig {
    /// Konto mit der globalen Standard-Basis-URL.
    pub fn new(api_key: ApiKey) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key,
            organization: None,
            project: None,
        }
    }
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
    pub fn with_organization(mut self, organization: impl Into<String>) -> Self {
        self.organization = Some(organization.into());
        self
    }
    pub fn with_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }
    /// Vollständige URL eines Endpunkts (`responses`, `decisions`, `models`).
    pub fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
}

/// Endgültiges Kontingent-Ende des Kontos (AI-003: `insufficient_quota`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaStop {
    /// Konkreter Grund aus `error.code` (z. B. `credit_balance_exhausted`), Protokollspalte `Error Code`.
    pub error_code: Option<String>,
    pub message: String,
}

/// Gemeinsamer Zustand eines OpenAI-Kontos über beide Adapter: Nach einem endgültigen Kontingentfehler erhalten alle
/// offenen OpenAI-Einträge (Responses und Decisions) `Quota exceeded`, ohne dass ein weiterer Request gesendet wird.
#[derive(Debug, Default)]
pub struct OpenAiAccount {
    quota: Mutex<Option<QuotaStop>>,
}

impl OpenAiAccount {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    /// Gesetzt, sobald ein Request `insufficient_quota` gemeldet hat.
    pub fn quota_stop(&self) -> Option<QuotaStop> {
        self.lock().clone()
    }
    /// Hebt die Sperre auf (z. B. nach Aufladen des Guthabens durch den Nutzer).
    pub fn clear_quota_stop(&self) {
        *self.lock() = None;
    }
    fn stop(&self, stop: QuotaStop) {
        let mut guard = self.lock();
        if guard.is_none() {
            *guard = Some(stop);
        }
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<QuotaStop>> {
        self.quota.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Erfolgreiche HTTP-Antwort (2xx).
#[derive(Debug, Clone)]
pub struct HttpReply {
    pub status: u16,
    pub body: Vec<u8>,
    /// Header `x-request-id`.
    pub request_id: Option<String>,
}

/// HTTP-Zugang eines Kontos; beide Adapter teilen Client, Konfiguration und Kontostand.
#[derive(Clone)]
pub struct OpenAiHttp {
    client: reqwest::Client,
    config: OpenAiConfig,
    account: Arc<OpenAiAccount>,
    limiter: Option<Arc<Limiter>>,
}

impl std::fmt::Debug for OpenAiHttp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiHttp")
            .field("config", &self.config)
            .field("account", &self.account)
            .finish_non_exhaustive()
    }
}

impl OpenAiHttp {
    /// `client` stellt der Aufrufer (Host-Überschreibung, Proxy, TLS); `account` teilen sich alle Adapter desselben
    /// Kontos.
    pub fn new(client: reqwest::Client, config: OpenAiConfig, account: Arc<OpenAiAccount>) -> Self {
        Self {
            client,
            config,
            account,
            limiter: None,
        }
    }
    /// Limiter, dem auch Aufrufe über [`crate::provider::AiProvider::execute`] ihre Rate-Limit-Header melden.
    pub fn with_limiter(mut self, limiter: Arc<Limiter>) -> Self {
        self.limiter = Some(limiter);
        self
    }
    pub fn config(&self) -> &OpenAiConfig {
        &self.config
    }
    pub fn account(&self) -> &Arc<OpenAiAccount> {
        &self.account
    }
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
    pub(crate) fn limiter(&self) -> Option<&Limiter> {
        self.limiter.as_deref()
    }

    /// Prüfungen vor jedem Request: Schlüssel vorhanden, Kontingent nicht erschöpft. Ohne Erfolg geht nichts hinaus.
    pub(crate) fn preflight(&self) -> Result<(), AttemptFailure> {
        if self.config.api_key.is_empty() {
            return Err(ProviderError::MissingApiKey.into());
        }
        if let Some(stop) = self.account.quota_stop() {
            let mut failure = AttemptFailure::from(ProviderError::QuotaExceeded);
            failure.error_code = stop.error_code;
            return Err(failure);
        }
        Ok(())
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .request(method, self.config.endpoint(path))
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.config.api_key.expose()),
            );
        if let Some(org) = &self.config.organization {
            builder = builder.header("OpenAI-Organization", org);
        }
        if let Some(project) = &self.config.project {
            builder = builder.header("OpenAI-Project", project);
        }
        builder
    }

    /// `POST <basis>/<path>` mit JSON-Body.
    pub(crate) async fn post_json(
        &self,
        path: &str,
        body: &Value,
        observer: Option<&Limiter>,
    ) -> Result<HttpReply, AttemptFailure> {
        self.preflight()?;
        let builder = self
            .request(reqwest::Method::POST, path)
            .header(CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(body).unwrap_or_default());
        self.send(builder, observer).await
    }

    /// `GET <basis>/<path>`.
    pub(crate) async fn get(
        &self,
        path: &str,
        observer: Option<&Limiter>,
    ) -> Result<HttpReply, AttemptFailure> {
        self.preflight()?;
        self.send(self.request(reqwest::Method::GET, path), observer)
            .await
    }

    async fn send(
        &self,
        builder: reqwest::RequestBuilder,
        observer: Option<&Limiter>,
    ) -> Result<HttpReply, AttemptFailure> {
        let response = builder
            .send()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        if let Some(limiter) = observer.or(self.limiter()) {
            limiter.observe_headers(PROVIDER_ID, &headers);
        }
        let body = response
            .bytes()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?
            .to_vec();
        if !(200..300).contains(&status) {
            let now = observer
                .or(self.limiter())
                .map(|l| l.clock().wall_unix_millis())
                .unwrap_or_else(wall_now_millis);
            // Request-ID auch bei Fehlern (AI-001/AI-003/AI-004, Nachtrag Q00-N2, N6: Header `x-request-id`).
            return Err(self
                .classify_error(status, &headers, &body, now)
                .with_request_id(header_text(&headers, "x-request-id")));
        }
        Ok(HttpReply {
            status,
            body,
            request_id: header_text(&headers, "x-request-id"),
        })
    }

    /// Fehlerklasse einer Nicht-2xx-Antwort (AI-003). HTTP 429 mit `error.type = "insufficient_quota"` (oder der
    /// älteren Form `error.code = "insufficient_quota"`) ist endgültig: `Quota exceeded`, keine Wiederholung, der konkrete
    /// Grund aus `error.code` geht in `Error Code`, und das Konto wird für beide Adapter gesperrt. Jedes andere 429 ist
    /// ein Rate-Limit und wird nach INTEG-005 wiederholt (`Retry-After` hat Vorrang).
    pub fn classify_error(
        &self,
        status: u16,
        headers: &HeaderMap,
        body: &[u8],
        now_unix_millis: i64,
    ) -> AttemptFailure {
        let error = ErrorBody::parse(body);
        if status == 429 && error.is_insufficient_quota() {
            let code = error.code.clone().or_else(|| error.kind.clone());
            self.account.stop(QuotaStop {
                error_code: code.clone(),
                message: error.message.clone().unwrap_or_default(),
            });
            let mut failure = AttemptFailure::from(ProviderError::QuotaExceeded);
            failure.http_status = Some(429);
            failure.error_code = code;
            return failure;
        }
        let retry_after = header_text(headers, "retry-after")
            .and_then(|v| parse_retry_after(&v, now_unix_millis));
        let log_code = error.code.clone().or_else(|| error.kind.clone());
        let mut failure = if status == 429 {
            AttemptFailure::from(ProviderError::RateLimited {
                retry_after_seconds: retry_after.map(|d| d.as_secs()),
            })
        } else {
            let message = error.message.clone().unwrap_or_else(|| {
                reqwest::StatusCode::from_u16(status)
                    .ok()
                    .and_then(|s| s.canonical_reason())
                    .unwrap_or("")
                    .to_string()
            });
            AttemptFailure::from(ProviderError::Http { status, message })
        };
        failure.retry_after = retry_after;
        failure.error_code = log_code;
        failure
    }
}

/// Felder des Fehlerobjekts `{"error": {"type", "code", "message"}}`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ErrorBody {
    kind: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

impl ErrorBody {
    fn parse(body: &[u8]) -> Self {
        let value: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let error = &value["error"];
        let text = |key: &str| error.get(key).and_then(Value::as_str).map(str::to_string);
        Self {
            kind: text("type"),
            code: text("code"),
            message: text("message"),
        }
    }
    fn is_insufficient_quota(&self) -> bool {
        self.kind.as_deref() == Some("insufficient_quota")
            || self.code.as_deref() == Some("insufficient_quota")
    }
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

fn wall_now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// `usage` einer OpenAI-Antwort in einheitlicher Form (AI-001): `input_tokens` (einschließlich der aus dem
/// Anbieter-Cache gelesenen Tokens, Festlegung 24.11.2026 – OpenAI zählt sie bereits so), `output_tokens`,
/// `input_tokens_details.cached_tokens`, `input_tokens_details.cache_write_tokens` (Nachtrag Q00-N1) und
/// `output_tokens_details.reasoning_tokens` (Nachtrag Q00-H1). Fehlende Felder bleiben `null`. `None`, wenn die
/// Antwort kein `usage`-Objekt hat.
pub fn parse_usage(body: &Value) -> Option<Usage> {
    let usage = body.get("usage").filter(|u| u.is_object())?;
    let field = |pointer: &str| usage.pointer(pointer).and_then(Value::as_u64);
    Some(Usage {
        input_tokens: field("/input_tokens"),
        output_tokens: field("/output_tokens"),
        cached_input_tokens: field("/input_tokens_details/cached_tokens"),
        cache_write_tokens: field("/input_tokens_details/cache_write_tokens"),
        reasoning_tokens: field("/output_tokens_details/reasoning_tokens"),
    })
}

/// Protokolldaten eines Requests (eine Zeile `ai_requests`, AI-014) nach allen Wiederholungen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestMeta {
    /// Versuche einschließlich des ersten.
    pub attempts: u32,
    /// Spalte `Retries`.
    pub retries: u32,
    /// Spalte `HTTP Status` (leer bei Transportfehler, Timeout und ohne gesendeten Request).
    pub http_status: Option<u16>,
    /// Spalte `Error Code` (z. B. `credit_balance_exhausted`, `transport/connect`).
    pub error_code: Option<String>,
    /// Spalte `Error Message`.
    pub error_message: Option<String>,
    pub duration_ms: u64,
    /// `None`, wenn keine Antwort kam oder die Antwort kein `usage` hatte.
    pub usage: Option<Usage>,
    pub provider_request_id: Option<String>,
    /// Refusal-Text (Spalte `Refusal Reason`), nie als Antwort übernommen.
    pub refusal_reason: Option<String>,
    /// Ob eine 2xx-Antwort ankam (Kosten fallen an; ohne `usage` aus der Schätzung).
    pub response_received: bool,
}

impl RequestMeta {
    pub(crate) fn from_execution<T>(
        exec: &Execution<T>,
        reply: Option<(u16, Option<Usage>, Option<String>)>,
    ) -> Self {
        let (http_status, usage, request_id, received) = match (&exec.result, reply) {
            (Ok(_), Some((status, usage, id))) => (Some(status), usage, id, true),
            // Fehlerantwort: Request-ID der letzten Antwort (M4-K09, Befund B1).
            (Err(error), _) => (
                exec.http_status(),
                None,
                error.provider_request_id.clone(),
                false,
            ),
            _ => (exec.http_status(), None, None, false),
        };
        Self {
            attempts: exec.attempts,
            retries: exec.retries(),
            http_status,
            error_code: exec.error_code().map(str::to_string),
            error_message: exec.result.as_ref().err().map(|e| e.message.clone()),
            duration_ms: exec.duration_ms(),
            usage,
            provider_request_id: request_id,
            refusal_reason: None,
            response_received: received,
        }
    }

    /// `usage` für das Kostenprotokoll: gemeldet → übernommen; Antwort ohne `usage` → Schätzung (`Price Source =
    /// estimate`, Tokenfelder `null`, AI-004/AI-014); keine Antwort → Kosten 0.
    pub fn usage_report(
        &self,
        estimated_input_tokens: u64,
        estimated_output_tokens: u64,
    ) -> UsageReport {
        match (&self.usage, self.response_received) {
            (Some(usage), true) => UsageReport::Reported(usage.clone()),
            (None, true) => UsageReport::Missing {
                estimated_input_tokens,
                estimated_output_tokens,
            },
            _ => UsageReport::NoResponse,
        }
    }

    /// Eingabe für [`crate::cost::request_record`] (M4-K05).
    pub fn request_outcome(
        &self,
        status: AiStatus,
        estimated_input_tokens: u64,
        estimated_output_tokens: u64,
        images: u32,
    ) -> RequestOutcome {
        RequestOutcome {
            status,
            usage: self.usage_report(estimated_input_tokens, estimated_output_tokens),
            images,
            batch: false,
            http_status: self.http_status,
            error_code: self.error_code.clone(),
            error_message: self.error_message.clone(),
            refusal_reason: self.refusal_reason.clone(),
            duration_ms: self.duration_ms,
            provider_request_id: self.provider_request_id.clone(),
            retries: self.retries,
            confidence_mismatch: false,
            estimated_input_tokens: Some(estimated_input_tokens),
        }
    }
}

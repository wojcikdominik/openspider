//! Adapter Anthropic Messages API (AI-002).
//!
//! `POST /v1/messages` mit `x-api-key`, `anthropic-version` und `content-type: application/json`; Systemtext im
//! Top-Level-Feld `system`, strukturierte Ausgabe und Aufwand in `output_config` (`format` mit `type: "json_schema"`,
//! `effort`), Bilder als Inhaltsblock `image` mit Base64-Quelle. Die Modellliste (`GET /v1/models`) liefert die
//! Fähigkeiten live aus `capabilities`; fehlt das Objekt oder ein Knoten, gilt die Fähigkeit als nicht vorhanden
//! (OS-Festlegung). Kontingent- und Ausgabengrenzen sperren den Anbieter für den Rest des Laufs ([`QuotaLatch`]).
//!
//! Einstieg für die Ausführung (D01): [`AnthropicAdapter::send`] je Versuch innerhalb von
//! [`crate::limits::Limiter::run`] und [`crate::schema::run_with_repair`]; das gemeinsame [`AiProvider`] nutzt denselben
//! Weg ohne Wiederholung.
pub mod reasoning;
pub mod shared;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde_json::{json, Map, Value};

use crate::limits::{AttemptFailure, Limiter};
use crate::provider::{
    resolve_reasoning_level, AiProvider, Capabilities, CapabilitySource, Input, Model,
    ProviderError, ProviderFuture, ReasoningLevel, Request, Usage,
};
use crate::schema::{RawAnswer, RepairRequest};

pub use shared::{
    base64_encode, image_input, sniff_image_media_type, AdapterReply, ApiKey, OutputLimitReached,
    QuotaLatch, QuotaStop,
};
use shared::{
    elapsed_ms, error_message, http_failure, opt_u64, quota_failure, rate_limited, raw_from_text,
    retry_after, wall_millis,
};

/// Anbieterkennung (Limiter, Protokoll, Preisschlüssel).
pub const PROVIDER_ID: &str = "anthropic";
/// Endpunkt im Preisschlüssel (AI-010).
pub const ENDPOINT: &str = "messages";
/// Standard-Basis-URL.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// Wert des Headers `anthropic-version`.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// `max_tokens`, wenn weder Aufgabe noch Konfiguration einen Wert nennen (Anthropic verlangt das Feld).
pub const FALLBACK_MAX_TOKENS: u64 = 4096;
/// Meldungsanfang der vom Nutzer gesetzten Ausgabengrenze (Organisation oder Workspace; belegt).
pub const SPEND_LIMIT_PREFIX: &str = "You have reached your specified";
/// Meldungsanfang bei zu geringem Guthaben (Heuristik, OS-Festlegung V-A3).
pub const CREDIT_BALANCE_PREFIX: &str = "Your credit balance is too low";
/// `error.details.error_code` der monatlichen Ausgabenobergrenze.
pub const SPEND_CAP_CODE: &str = "enforced_spend_limit_reached";

/// Verbindungsdaten.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// Basis-URL ohne `/v1` (Standard [`DEFAULT_BASE_URL`]; im Test die ai-mock-Adresse `…/anthropic`).
    pub base_url: String,
    pub api_key: ApiKey,
    /// `max_tokens`, wenn die Aufgabe keinen Wert setzt (Standard aus dem Aufgabenprofil, AI-011).
    pub default_max_tokens: u64,
}

impl AnthropicConfig {
    /// Konfiguration mit Standard-Basis-URL.
    pub fn new(api_key: ApiKey) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.into(),
            api_key,
            default_max_tokens: FALLBACK_MAX_TOKENS,
        }
    }

    /// Andere Basis-URL (ai-mock, `OPENSPIDER_API_BASE_ANTHROPIC`).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url.trim_end_matches('/'))
    }
}

/// Bekannte Aufwandsstufen in aufsteigender Reihenfolge (nur für die Sortierung; zulässig ist, was `capabilities`
/// meldet).
const EFFORT_ORDER: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Ein Modell aus `GET /v1/models` mit seinen live gemeldeten Fähigkeiten (AI-002, AI-010).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicModel {
    pub id: String,
    pub display_name: Option<String>,
    pub max_input_tokens: Option<u64>,
    pub max_tokens: Option<u64>,
    /// `false`, wenn das Modell kein `capabilities`-Objekt meldet (Hinweis „Fähigkeiten unbekannt“).
    pub capabilities_known: bool,
    /// `structured_outputs.supported`.
    pub structured_output: bool,
    /// `image_input.supported`.
    pub vision: bool,
    /// `batch.supported`.
    pub batch: bool,
    /// `effort.supported`.
    pub effort_supported: bool,
    /// Stufen mit `effort.<stufe>.supported = true` (aufsteigend); leer, wenn `effort.supported` nicht `true` ist.
    pub effort_levels: Vec<String>,
    /// `thinking.supported` (Nachtrag Q00-H1); ohne Knoten `false`.
    pub thinking_supported: bool,
    /// `thinking.types.adaptive.supported` (nur mit `thinking.supported`).
    pub thinking_adaptive: bool,
    /// `thinking.types.disabled.supported` (nur mit `thinking.supported`): `off` ist „sicher aus“.
    pub thinking_disabled: bool,
}

/// Wählbarkeit eines Modells für eine Aufgabe (AI-002, AI-005, AI-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selectability {
    Selectable,
    /// Ausgegraut; `capabilities_unknown` = Hinweis „Fähigkeiten unbekannt“ (Anthropic ohne `capabilities`).
    Unavailable {
        capabilities_unknown: bool,
    },
}

fn supported(node: Option<&Value>) -> bool {
    node.and_then(|n| n.get("supported"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

impl AnthropicModel {
    /// Liest einen Eintrag der Modellliste.
    pub fn from_json(entry: &Value) -> Option<Self> {
        let id = entry.get("id")?.as_str()?.to_string();
        let caps = entry.get("capabilities").filter(|c| c.is_object());
        let node = |name: &str| caps.and_then(|c| c.get(name));
        let effort_supported = supported(node("effort"));
        let mut effort_levels: Vec<String> = match node("effort").and_then(Value::as_object) {
            Some(levels) if effort_supported => levels
                .iter()
                .filter(|(name, value)| name.as_str() != "supported" && supported(Some(value)))
                .map(|(name, _)| name.clone())
                .collect(),
            _ => Vec::new(),
        };
        effort_levels.sort_by_key(|level| {
            (
                EFFORT_ORDER
                    .iter()
                    .position(|known| known == level)
                    .unwrap_or(EFFORT_ORDER.len()),
                level.clone(),
            )
        });
        let thinking = node("thinking");
        let thinking_supported = supported(thinking);
        let thinking_type = |name: &str| {
            thinking_supported
                && supported(thinking.and_then(|t| t.pointer(&format!("/types/{name}"))))
        };
        Some(Self {
            thinking_adaptive: thinking_type("adaptive"),
            thinking_disabled: thinking_type("disabled"),
            thinking_supported,
            id,
            display_name: entry
                .get("display_name")
                .and_then(Value::as_str)
                .map(str::to_string),
            max_input_tokens: opt_u64(entry.get("max_input_tokens")),
            max_tokens: opt_u64(entry.get("max_tokens")),
            capabilities_known: caps.is_some(),
            structured_output: supported(node("structured_outputs")),
            vision: supported(node("image_input")),
            batch: supported(node("batch")),
            effort_supported,
            effort_levels,
        })
    }

    /// Fähigkeiten nach AI-001 (Text immer; Structured Output, Vision, Batch laut `capabilities`).
    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            text: true,
            structured_output: self.structured_output,
            vision: self.vision,
            batch: self.batch,
            ..Capabilities::default()
        }
    }

    /// Gemeinsame Modellbeschreibung (AI-001).
    pub fn to_model(&self) -> Model {
        Model {
            provider: PROVIDER_ID.into(),
            id: self.id.clone(),
            endpoint: ENDPOINT.into(),
            capabilities: self.capabilities(),
            capability_source: CapabilitySource::AnthropicModels,
            reasoning_levels: self.reasoning_levels(),
        }
    }

    /// Verfügbare Reasoning-Stufen (AI-002, Nachtrag Q00-H1), aufsteigend: `off` nur bei
    /// `thinking.types.disabled.supported`, dazu die Aufwandsstufen aus `capabilities.effort`; ohne `effort` nur
    /// `model-default` (leere Liste).
    pub fn reasoning_levels(&self) -> Vec<ReasoningLevel> {
        let mut levels = Vec::new();
        if self.thinking_disabled {
            levels.push(ReasoningLevel::Off);
        }
        if self.effort_supported {
            levels.extend(reasoning::levels_from_efforts(
                self.effort_levels.iter().map(String::as_str),
            ));
        }
        levels.sort_by_key(|level| level.rank());
        levels.dedup();
        levels
    }

    /// Effektive Stufe dieses Modells für die gewünschte Stufe (AI-011, Nachtrag).
    pub fn effective_reasoning(&self, requested: ReasoningLevel) -> ReasoningLevel {
        resolve_reasoning_level(requested, &self.reasoning_levels())
    }

    /// `thinking` und `output_config.effort` für die gewünschte Stufe (AI-002, Nachtrag Q00-H1): `off` → `thinking`
    /// `disabled` ohne Aufwand; `minimal` … `max` → `adaptive` (nur wenn unterstützt) plus der Aufwand der effektiven
    /// Stufe; `model-default` bzw. keine verfügbare Stufe → keines der beiden Felder.
    pub fn reasoning_params(
        &self,
        requested: ReasoningLevel,
    ) -> (Option<Value>, Option<&'static str>) {
        match self.effective_reasoning(requested) {
            ReasoningLevel::ModelDefault => (None, None),
            ReasoningLevel::Off => (reasoning::anthropic_thinking(ReasoningLevel::Off), None),
            level => (
                self.thinking_adaptive
                    .then(|| reasoning::anthropic_thinking(level))
                    .flatten(),
                reasoning::effort_value(level),
            ),
        }
    }

    /// Die Stufe, die tatsächlich als `output_config.effort` gesendet wird: nur wenn `effort.supported = true` und die
    /// Stufe als unterstützt gemeldet ist; sonst keine (Modellstandard).
    pub fn effort_to_send<'a>(&self, wanted: &'a str) -> Option<&'a str> {
        (self.effort_supported && self.effort_levels.iter().any(|l| l == wanted)).then_some(wanted)
    }

    /// Ob eine gespeicherte Stufe nicht mehr unterstützt ist (Warnung im Aufgabendialog, AI-002).
    pub fn effort_unsupported(&self, stored: &str) -> bool {
        self.effort_to_send(stored).is_none()
    }

    /// Wählbarkeit für eine Aufgabe mit den Anforderungen `required` (z. B. Schema- oder Bildaufgabe).
    pub fn selectable_for(&self, required: Capabilities) -> Selectability {
        if self.capabilities().supports(required) {
            Selectability::Selectable
        } else {
            Selectability::Unavailable {
                capabilities_unknown: !self.capabilities_known,
            }
        }
    }

    /// Ob Aufgaben mit diesem Modell im Batch-Modus (AI-016) gebündelt werden; sonst laufen sie synchron.
    pub fn runs_in_batch(&self) -> bool {
        self.batch
    }
}

/// Adapter für ein Anthropic-Konto. Eine Instanz gilt für einen Lauf: Ihre [`QuotaLatch`] sperrt nach einer
/// endgültigen Kontingentantwort alle weiteren Requests.
pub struct AnthropicAdapter {
    client: reqwest::Client,
    config: AnthropicConfig,
    limiter: Option<Arc<Limiter>>,
    latch: QuotaLatch,
    models: Mutex<Option<HashMap<String, AnthropicModel>>>,
}

impl std::fmt::Debug for AnthropicAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicAdapter")
            .field("config", &self.config)
            .field("quota", &self.latch.stop())
            .finish_non_exhaustive()
    }
}

impl AnthropicAdapter {
    /// Adapter mit dem HTTP-Client des Aufrufers (Host-Überschreibung CRAWL-123, Proxy).
    pub fn new(client: reqwest::Client, config: AnthropicConfig) -> Self {
        Self {
            client,
            config,
            limiter: None,
            latch: QuotaLatch::default(),
            models: Mutex::new(None),
        }
    }

    /// Meldet die Rate-Limit-Header jeder Antwort an `limiter` (AI-017, `Limiter::observe_headers`).
    pub fn with_limiter(mut self, limiter: Arc<Limiter>) -> Self {
        self.limiter = Some(limiter);
        self
    }

    /// Kontingent-Sperre dieses Laufs.
    pub fn quota(&self) -> &QuotaLatch {
        &self.latch
    }

    /// Die Konfiguration.
    pub fn config(&self) -> &AnthropicConfig {
        &self.config
    }

    fn headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("x-api-key", self.config.api_key.expose())
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
    }

    fn observe(&self, headers: &HeaderMap) {
        if let Some(limiter) = &self.limiter {
            limiter.observe_headers(PROVIDER_ID, headers);
        }
    }

    /// Lädt die Modellliste (alle Seiten) und merkt sie sich für die Aufwandsprüfung. Ein erneutes Laden übernimmt
    /// geänderte `capabilities` ohne App-Update.
    pub async fn list_models(&self) -> Result<Vec<AnthropicModel>, AttemptFailure> {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut url = self.config.url("/v1/models?limit=1000");
            if let Some(after) = &after {
                url.push_str("&after_id=");
                url.push_str(&query_escape(after));
            }
            let builder = self.client.get(url);
            let response = self
                .headers(builder)
                .send()
                .await
                .map_err(|e| AttemptFailure::from_reqwest(&e))?;
            let status = response.status();
            let headers = response.headers().clone();
            self.observe(&headers);
            let bytes = response
                .bytes()
                .await
                .map_err(|e| AttemptFailure::from_reqwest(&e))?;
            let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            if !status.is_success() {
                return Err(self.classify_error(status, &headers, &body));
            }
            let page = body.get("data").and_then(Value::as_array);
            out.extend(
                page.into_iter()
                    .flatten()
                    .filter_map(AnthropicModel::from_json),
            );
            let more = body
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let last = body
                .get("last_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| out.last().map(|m| m.id.clone()));
            match (more, last) {
                (true, Some(last)) if after.as_deref() != Some(last.as_str()) => after = Some(last),
                _ => break,
            }
        }
        *self.models.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(out.iter().map(|m| (m.id.clone(), m.clone())).collect());
        Ok(out)
    }

    /// Modell aus der zuletzt geladenen Liste.
    pub fn cached_model(&self, id: &str) -> Option<AnthropicModel> {
        self.models
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .and_then(|m| m.get(id).cloned())
    }

    async fn model_info(&self, id: &str) -> Option<AnthropicModel> {
        let loaded = self
            .models
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some();
        if !loaded {
            // Konservativ: ohne Modellliste wird kein Aufwand gesendet.
            self.list_models().await.ok()?;
        }
        self.cached_model(id)
    }

    /// Request-Körper für `POST /v1/messages`. `model` ist der Eintrag der Modellliste (für den Aufwand); ohne Eintrag
    /// wird kein `output_config.effort` gesendet.
    pub fn request_body(
        &self,
        request: &Request,
        repair: Option<&RepairRequest>,
        model: Option<&AnthropicModel>,
    ) -> Value {
        let mut content: Vec<Value> = request
            .input
            .iter()
            .map(|input| match input {
                Input::Text { text } => json!({"type": "text", "text": text}),
                Input::Image { media_type, base64 } => json!({
                    "type": "image",
                    "source": {"type": "base64", "media_type": media_type, "data": base64},
                }),
            })
            .collect();
        if let Some(repair) = repair {
            content.push(json!({"type": "text", "text": repair.instruction()}));
        }
        let mut body = Map::new();
        body.insert("model".into(), json!(request.model.id));
        body.insert(
            "max_tokens".into(),
            json!(request.max_tokens.unwrap_or(self.config.default_max_tokens)),
        );
        if !request.instructions.is_empty() {
            body.insert("system".into(), json!(request.instructions));
        }
        body.insert(
            "messages".into(),
            json!([{"role": "user", "content": content}]),
        );
        if let Some(temperature) = request.temperature {
            body.insert("temperature".into(), json!(temperature));
        }
        let mut output_config = Map::new();
        if let Some(schema) = &request.response_schema {
            output_config.insert(
                "format".into(),
                json!({"type": "json_schema", "schema": schema}),
            );
        }
        if request.reasoning != ReasoningLevel::ModelDefault {
            // Reasoning-Stufe (Nachtrag Q00-H1) hat Vorrang vor dem älteren Feld `effort`.
            if let Some(model) = model {
                let (thinking, effort) = model.reasoning_params(request.reasoning);
                if let Some(thinking) = thinking {
                    body.insert("thinking".into(), thinking);
                }
                if let Some(effort) = effort {
                    output_config.insert("effort".into(), json!(effort));
                }
            }
        } else if let (Some(wanted), Some(model)) = (request.effort.as_deref(), model) {
            if let Some(level) = model.effort_to_send(wanted) {
                output_config.insert("effort".into(), json!(level));
            }
        }
        if !output_config.is_empty() {
            body.insert("output_config".into(), Value::Object(output_config));
        }
        Value::Object(body)
    }

    /// Ein Versuch: sendet den Request (sofern der Anbieter nicht wegen Kontingent gesperrt ist) und übersetzt Antwort
    /// bzw. Fehler. Refusals sind `Ok` mit [`AdapterReply::is_refusal`]; sie werden nicht wiederholt.
    pub async fn send(
        &self,
        request: &Request,
        repair: Option<&RepairRequest>,
    ) -> Result<AdapterReply, AttemptFailure> {
        if let Some(blocked) = self.latch.blocked_failure() {
            return Err(blocked);
        }
        let model = if request.effort.is_some() || request.reasoning != ReasoningLevel::ModelDefault
        {
            self.model_info(&request.model.id).await
        } else {
            None
        };
        let body = self.request_body(request, repair, model.as_ref());
        let started = Instant::now();
        let builder = self.client.post(self.config.url("/v1/messages"));
        let response = self
            .headers(builder)
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let status = response.status();
        let headers = response.headers().clone();
        self.observe(&headers);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let duration_ms = elapsed_ms(started);
        if !status.is_success() {
            let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            return Err(self.classify_error(status, &headers, &body));
        }
        let structured = request.response_schema.is_some();
        let body: Value = match serde_json::from_slice(&bytes) {
            Ok(body) => body,
            Err(error) => {
                return Ok(AdapterReply {
                    raw: RawAnswer::Unparsable {
                        detail: error.to_string(),
                    },
                    usage: Usage::default(),
                    refusal_reason: None,
                    stop_reason: None,
                    provider_request_id: request_id(&headers, &Value::Null),
                    http_status: status.as_u16(),
                    duration_ms,
                    structured,
                    output_limit: None,
                    reported_cost: None,
                })
            }
        };
        Ok(parse_message(
            &body,
            &headers,
            status.as_u16(),
            duration_ms,
            structured,
        ))
    }

    /// Optional: `POST /v1/messages/count_tokens` für die Kostenschätzung einer Stichprobe (AI-012).
    pub async fn count_tokens(&self, request: &Request) -> Result<u64, AttemptFailure> {
        let mut body = self.request_body(request, None, None);
        if let Some(object) = body.as_object_mut() {
            object.remove("max_tokens");
            object.remove("temperature");
        }
        let builder = self
            .client
            .post(self.config.url("/v1/messages/count_tokens"));
        let response = self
            .headers(builder)
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let status = response.status();
        let headers = response.headers().clone();
        self.observe(&headers);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AttemptFailure::from_reqwest(&e))?;
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(self.classify_error(status, &headers, &body));
        }
        body.get("input_tokens")
            .and_then(Value::as_u64)
            .ok_or_else(|| AttemptFailure::from(ProviderError::InvalidOutput))
    }

    /// Fehlerantwort → Fehlerklasse (AI-002). Endgültige Kontingentfälle sperren den Anbieter für den Lauf.
    fn classify_error(
        &self,
        status: StatusCode,
        headers: &HeaderMap,
        body: &Value,
    ) -> AttemptFailure {
        let code = status.as_u16();
        let kind = body
            .pointer("/error/type")
            .and_then(Value::as_str)
            .unwrap_or("");
        let message = error_message(body, status);
        let details_code = body
            .pointer("/error/details/error_code")
            .and_then(Value::as_str);
        let retry = retry_after(headers, wall_millis(self.limiter.as_ref()));
        let kind_or = |default: &str| {
            if kind.is_empty() {
                default.to_string()
            } else {
                kind.to_string()
            }
        };
        let quota_code = match code {
            402 => Some(kind_or("billing_error")),
            429 if details_code == Some(SPEND_CAP_CODE) => Some(SPEND_CAP_CODE.to_string()),
            400 if message.starts_with(SPEND_LIMIT_PREFIX)
                || message.starts_with(CREDIT_BALANCE_PREFIX) =>
            {
                Some(kind_or("invalid_request_error"))
            }
            _ => None,
        };
        if let Some(error_code) = quota_code {
            self.latch.trip(QuotaStop {
                http_status: code,
                error_code: Some(error_code.clone()),
                message,
            });
            return quota_failure(code, Some(error_code));
        }
        let failure = match code {
            429 => rate_limited(retry, code),
            529 => {
                let mut failure = AttemptFailure::from(ProviderError::Overloaded);
                failure.retry_after = retry;
                failure
            }
            _ => http_failure(code, message, retry),
        };
        if kind.is_empty() {
            failure
        } else {
            failure.with_error_code(kind)
        }
    }
}

/// Prozentkodierung eines Query-Werts (alles außer unreservierten Zeichen).
fn query_escape(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Anbieter-Request-ID (AI-001, Nachtrag Q00-N2): Header `request-id`, bei Fehlern ersatzweise `request_id` im
/// Antwortkörper; nie die Antwort-`id` (`msg_…`).
pub fn request_id(headers: &HeaderMap, body: &Value) -> Option<String> {
    headers
        .get("request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| {
            body.get("request_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

/// Einheitliche Tokenzahlen (AI-001, AI-002): `input_tokens` zählt einschließlich der aus dem Cache gelesenen und der
/// in den Cache geschriebenen Tokens (Festlegung 24.11.2026); fehlende Felder bleiben `None`.
pub fn usage_from_json(usage: Option<&Value>) -> Usage {
    let field = |name: &str| opt_u64(usage.and_then(|u| u.get(name)));
    let input = field("input_tokens");
    let cached = field("cache_read_input_tokens");
    let written = field("cache_creation_input_tokens");
    Usage {
        input_tokens: input.map(|i| i + cached.unwrap_or(0) + written.unwrap_or(0)),
        output_tokens: field("output_tokens"),
        cached_input_tokens: cached,
        cache_write_tokens: written,
        // Nachtrag Q00-H1: `usage.output_tokens_details.thinking_tokens` (Teilmenge von `output_tokens`).
        reasoning_tokens: opt_u64(
            usage.and_then(|u| u.pointer("/output_tokens_details/thinking_tokens")),
        ),
    }
}

/// Erfolgsantwort von `POST /v1/messages` → [`AdapterReply`].
fn parse_message(
    body: &Value,
    headers: &HeaderMap,
    http_status: u16,
    duration_ms: u64,
    structured: bool,
) -> AdapterReply {
    let stop_reason = body
        .get("stop_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    // Nur Textblöcke; `thinking`-Blöcke sind kein Ergebnis und werden verworfen (AI-001, Nachtrag).
    let text: String = body
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect();
    let refusal = stop_reason.as_deref() == Some("refusal");
    // `stop_reason: "max_tokens"` → Abbruch an der Ausgabegrenze für jede Aufgabe (Nachtrag Q00-H1).
    let output_limit = (!refusal && stop_reason.as_deref() == Some("max_tokens"))
        .then(|| "max_tokens".to_string());
    let raw = if refusal {
        RawAnswer::Refusal
    } else {
        raw_from_text(text, structured, output_limit.as_deref())
    };
    let refusal_reason = refusal
        .then(|| {
            body.get("stop_details").and_then(|details| match details {
                Value::String(s) => Some(s.clone()),
                other => other
                    .get("category")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .flatten();
    AdapterReply {
        raw,
        usage: usage_from_json(body.get("usage")),
        refusal_reason,
        stop_reason,
        provider_request_id: request_id(headers, body),
        http_status,
        duration_ms,
        structured,
        output_limit,
        reported_cost: None,
    }
}

impl AiProvider for AnthropicAdapter {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async move {
            self.list_models()
                .await
                .map(|models| models.iter().map(AnthropicModel::to_model).collect())
                .map_err(|failure| failure.error)
        })
    }

    fn execute(&self, request: Request) -> ProviderFuture<'_, crate::provider::Response> {
        Box::pin(async move {
            let reply = self
                .send(&request, None)
                .await
                .map_err(|failure| failure.error)?;
            reply.into_response(request.response_schema.as_ref())
        })
    }
}

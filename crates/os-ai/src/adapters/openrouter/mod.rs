//! Adapter OpenRouter (AI-005): OpenAI-kompatibles Chat-Completions-Format unter `…/api/v1`.
//!
//! Modellliste (`GET /models`) mit Preisen (USD je Token bzw. je Request/Bild als Zeichenketten, fehlende Felder = 0)
//! und Fähigkeiten (`supported_parameters`, Eingabemodalitäten); strukturierte Ausgabe über `response_format` mit
//! `type: "json_schema"` und bei Schema-Aufgaben immer `provider.require_parameters = true`; Routing-Präferenzen im
//! Objekt `provider`. Kontostand über `GET /key` (`limit_remaining` direkt, `null` = unbegrenzt). Kontingentfehler
//! werden über `error.metadata.limit_source` unterschieden; Verweigerungen kommen als HTTP 200 (Form a) oder HTTP 403
//! mit `error.type = "refusal"` (Form b, vor allen anderen 403-Regeln geprüft).
use std::sync::{Arc, Mutex};
use std::time::Instant;

use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde_json::{json, Map, Value};

use crate::adapters::anthropic::shared::{
    elapsed_ms, error_message, http_failure, opt_u64, quota_failure, rate_limited, raw_from_text,
    retry_after, wall_millis,
};
pub use crate::adapters::anthropic::shared::{AdapterReply, ApiKey, QuotaLatch, QuotaStop};
pub use crate::adapters::anthropic::Selectability;
use crate::cost::rates::{parse_decimal_scaled, usd_to_pico};
use crate::cost::{PriceSource, Rates};
use crate::limits::{AttemptFailure, Limiter};
use crate::provider::{
    AiProvider, Capabilities, CapabilitySource, Input, Model, ProviderError, ProviderFuture,
    Request, Usage,
};
use crate::schema::{RawAnswer, RepairRequest};

/// Anbieterkennung.
pub const PROVIDER_ID: &str = "openrouter";
/// Endpunkt im Preisschlüssel (AI-010).
pub const ENDPOINT: &str = "chat-completions";
/// Standard-Basis-URL (EU siehe AI-019).
pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// `limit_source` des vorübergehenden In-Flight-Budgets.
pub const IN_FLIGHT_BUDGET: &str = "openrouter_in_flight_budget";
/// `error.metadata.reason` eines einzelnen zu großen Requests.
pub const WEIGHT_EXCEEDS_BUDGET: &str = "weight_exceeds_budget";
/// `limit_source`-Werte, die bei HTTP 403 endgültig sind.
pub const FINAL_403_SOURCES: [&str; 3] = [
    "openrouter_guardrail_budget",
    "openrouter_workspace_budget",
    "openrouter_key_limit",
];
/// Refusal-Grund ohne Refusal-Text (Form a nur mit `finish_reason = "content_filter"`).
pub const CONTENT_FILTER: &str = "content_filter";
/// Schemaname in `response_format.json_schema.name`.
pub const SCHEMA_NAME: &str = "response";

/// Routing-Präferenzen im Objekt `provider` (AI-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingPreferences {
    /// „Anbieter mit Datensammlung ausschließen“ (Standard an) → `data_collection: "deny"`.
    pub deny_data_collection: bool,
    /// „nur Endpunkte mit Zero Data Retention“ (Standard aus) → `zdr: true`.
    pub zero_data_retention: bool,
    /// Feste Anbieterreihenfolge (optional) → `order` und `allow_fallbacks: false`.
    pub order: Vec<String>,
}

impl Default for RoutingPreferences {
    fn default() -> Self {
        Self {
            deny_data_collection: true,
            zero_data_retention: false,
            order: Vec::new(),
        }
    }
}

/// Optionale App-Kennung (`HTTP-Referer`, `X-Title`); standardmäßig aus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    pub referer: String,
    pub title: String,
}

/// Verbindungsdaten.
#[derive(Debug, Clone)]
pub struct OpenRouterConfig {
    /// Basis-URL einschließlich `/api/v1` (Standard [`DEFAULT_BASE_URL`]; im Test `…/openrouter/api/v1`).
    pub base_url: String,
    pub api_key: ApiKey,
    pub routing: RoutingPreferences,
    /// App-Kennung senden (Standard aus = `None`).
    pub app_identity: Option<AppIdentity>,
}

impl OpenRouterConfig {
    /// Konfiguration mit Standardwerten.
    pub fn new(api_key: ApiKey) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.into(),
            api_key,
            routing: RoutingPreferences::default(),
            app_identity: None,
        }
    }

    /// Andere Basis-URL (ai-mock, EU-Endpunkt).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url.trim_end_matches('/'))
    }
}

/// Ein Modell aus `GET /models` mit Preisen und Fähigkeiten aus der API (AI-005, AI-010).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRouterModel {
    pub id: String,
    pub name: Option<String>,
    pub context_length: Option<u64>,
    /// Preise in Piko-USD je Token (`pricing.prompt`, `pricing.completion`, Cache-Lesen/-Schreiben).
    pub prompt_pico: u64,
    pub completion_pico: u64,
    pub cache_read_pico: u64,
    pub cache_write_pico: u64,
    /// Piko-USD je Request (`pricing.request`) und je Eingabebild (`pricing.image`).
    pub request_pico: u64,
    pub image_pico: u64,
    /// `false`, wenn ein vorhandenes Preisfeld nicht lesbar ist (z. B. negativ); dann gilt der Preis als unbekannt.
    pub prices_known: bool,
    pub supported_parameters: Vec<String>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn decimal_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Betrag in Mikro-USD aus einer JSON-Zahl bzw. -Zeichenkette; `null`/fehlend → `None`.
fn micro_usd(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(decimal_text)
        .and_then(|text| parse_decimal_scaled(&text, 6))
}

impl OpenRouterModel {
    /// Liest einen Eintrag der Modellliste.
    pub fn from_json(entry: &Value) -> Option<Self> {
        let id = entry.get("id")?.as_str()?.to_string();
        let pricing = entry.get("pricing");
        let mut prices_known = true;
        let mut price = |name: &str| -> u64 {
            match pricing.and_then(|p| p.get(name)).filter(|v| !v.is_null()) {
                None => 0,
                Some(value) => match decimal_text(value).and_then(|t| usd_to_pico(&t)) {
                    Some(pico) => pico,
                    None => {
                        prices_known = false;
                        0
                    }
                },
            }
        };
        let prompt_pico = price("prompt");
        let completion_pico = price("completion");
        let cache_read_pico = price("input_cache_read");
        let cache_write_pico = price("input_cache_write");
        let request_pico = price("request");
        let image_pico = price("image");
        let architecture = entry.get("architecture");
        Some(Self {
            id,
            name: entry
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string),
            context_length: opt_u64(entry.get("context_length")),
            prompt_pico,
            completion_pico,
            cache_read_pico,
            cache_write_pico,
            request_pico,
            image_pico,
            prices_known,
            supported_parameters: strings(entry.get("supported_parameters")),
            input_modalities: strings(architecture.and_then(|a| a.get("input_modalities"))),
            output_modalities: strings(architecture.and_then(|a| a.get("output_modalities"))),
        })
    }

    /// Structured Output: `structured_outputs` oder `response_format` in `supported_parameters`.
    pub fn structured_output(&self) -> bool {
        self.supported_parameters
            .iter()
            .any(|p| p == "structured_outputs" || p == "response_format")
    }

    /// Vision: Bild unter den Eingabemodalitäten.
    pub fn vision(&self) -> bool {
        self.input_modalities.iter().any(|m| m == "image")
    }

    /// Embeddings: Ausgabemodalität `embeddings`.
    pub fn embeddings(&self) -> bool {
        self.output_modalities.iter().any(|m| m == "embeddings")
    }

    /// Text: Ausgabemodalität `text` (ohne Angabe angenommen).
    pub fn text(&self) -> bool {
        self.output_modalities.is_empty() || self.output_modalities.iter().any(|m| m == "text")
    }

    /// Fähigkeiten nach AI-001 (Prices via API immer).
    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            text: self.text(),
            structured_output: self.structured_output(),
            vision: self.vision(),
            embeddings: self.embeddings(),
            prices_via_api: true,
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
            capability_source: CapabilitySource::OpenRouterModels,
        }
    }

    /// Wählbarkeit für eine Aufgabe (z. B. Schema-Aufgabe ohne Structured Output → ausgegraut mit Hinweis).
    pub fn selectable_for(&self, required: Capabilities) -> Selectability {
        if self.capabilities().supports(required) {
            Selectability::Selectable
        } else {
            Selectability::Unavailable {
                capabilities_unknown: false,
            }
        }
    }

    /// Preissätze aus der API (`Price Source = api`); `None`, wenn ein Preisfeld nicht lesbar ist.
    pub fn rates(&self) -> Option<Rates> {
        self.prices_known.then(|| Rates {
            input_pico: self.prompt_pico,
            output_pico: self.completion_pico,
            cache_read_pico: self.cache_read_pico,
            cache_write_pico: self.cache_write_pico,
            per_request_pico: self.request_pico,
            per_image_pico: self.image_pico,
            ..Rates::free(PriceSource::Api)
        })
    }

    /// Spalte `Input $/1M` (USD je 1 Mio. Tokens, 2 Nachkommastellen, Punkt).
    pub fn input_per_million(&self) -> String {
        format_scaled(self.prompt_pico, 10_000, 2)
    }

    /// Spalte `Output $/1M`.
    pub fn output_per_million(&self) -> String {
        format_scaled(self.completion_pico, 10_000, 2)
    }

    /// Spalte `Per Request $` (4 Nachkommastellen).
    pub fn per_request(&self) -> String {
        format_scaled(self.request_pico, 100_000_000, 4)
    }

    /// Spalte `Per Image $` (4 Nachkommastellen).
    pub fn per_image(&self) -> String {
        format_scaled(self.image_pico, 100_000_000, 4)
    }
}

/// Preise aus der API haben Vorrang vor der Preistabelle (AI-005, AI-010).
pub fn resolve_rates(api: Option<Rates>, table: Option<Rates>) -> Option<Rates> {
    api.or(table)
}

/// `value / unit` kaufmännisch gerundet mit `decimals` Nachkommastellen (`unit` = Wert der letzten Stelle).
fn format_scaled(value: u64, unit: u64, decimals: usize) -> String {
    let units = (u128::from(value) + u128::from(unit) / 2) / u128::from(unit);
    let scale = 10u128.pow(u32::try_from(decimals).unwrap_or(0));
    format!(
        "{}.{:0width$}",
        units / scale,
        units % scale,
        width = decimals
    )
}

/// Mikro-USD als Dezimaltext mit `decimals` Nachkommastellen (Punkt).
pub fn format_micro_usd(micro: u64, decimals: usize) -> String {
    let decimals = decimals.min(6);
    let unit = 10u64.pow(u32::try_from(6 - decimals).unwrap_or(0));
    format_scaled(micro, unit, decimals)
}

/// Restguthaben laut `GET /key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remaining {
    /// `limit = null` bzw. `limit_remaining = null` (Anzeige „unbegrenzt“).
    Unlimited,
    /// `limit_remaining` in Mikro-USD, direkt übernommen.
    MicroUsd(u64),
}

/// Kontostand aus `GET /key` (Felder `Limit`, `Limit Remaining`, `Limit Reset`, `Usage (Total/Day/Week/Month)`),
/// Beträge in Mikro-USD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBalance {
    pub limit: Option<u64>,
    pub limit_remaining: Option<u64>,
    pub limit_reset: Option<String>,
    pub usage: Option<u64>,
    pub usage_daily: Option<u64>,
    pub usage_weekly: Option<u64>,
    pub usage_monthly: Option<u64>,
}

impl KeyBalance {
    /// Liest die Antwort von `GET /key` (Objekt `data`).
    pub fn from_json(body: &Value) -> Option<Self> {
        let data = body.get("data")?.as_object()?;
        Some(Self {
            limit: micro_usd(data.get("limit")),
            limit_remaining: micro_usd(data.get("limit_remaining")),
            limit_reset: data
                .get("limit_reset")
                .and_then(Value::as_str)
                .map(str::to_string),
            usage: micro_usd(data.get("usage")),
            usage_daily: micro_usd(data.get("usage_daily")),
            usage_weekly: micro_usd(data.get("usage_weekly")),
            usage_monthly: micro_usd(data.get("usage_monthly")),
        })
    }

    /// Restguthaben: `limit_remaining` direkt (kein Rechnen aus Limit und Verbrauch); `null` = unbegrenzt.
    pub fn remaining(&self) -> Remaining {
        match (self.limit, self.limit_remaining) {
            (Some(_), Some(remaining)) => Remaining::MicroUsd(remaining),
            _ => Remaining::Unlimited,
        }
    }
}

/// Adapter für ein OpenRouter-Konto; eine Instanz je Lauf (Kontingent-Sperre).
pub struct OpenRouterAdapter {
    client: reqwest::Client,
    config: OpenRouterConfig,
    limiter: Option<Arc<Limiter>>,
    latch: QuotaLatch,
    balance: Mutex<Option<KeyBalance>>,
}

impl std::fmt::Debug for OpenRouterAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouterAdapter")
            .field("config", &self.config)
            .field("quota", &self.latch.stop())
            .finish_non_exhaustive()
    }
}

impl OpenRouterAdapter {
    /// Adapter mit dem HTTP-Client des Aufrufers.
    pub fn new(client: reqwest::Client, config: OpenRouterConfig) -> Self {
        Self {
            client,
            config,
            limiter: None,
            latch: QuotaLatch::default(),
            balance: Mutex::new(None),
        }
    }

    /// Begrenzung, an die der Adapter die Antwort-Header meldet (AI-017).
    pub fn with_limiter(mut self, limiter: Arc<Limiter>) -> Self {
        self.limiter = Some(limiter);
        self
    }

    /// Kontingent-Sperre dieses Laufs.
    pub fn quota(&self) -> &QuotaLatch {
        &self.latch
    }

    /// Die Konfiguration.
    pub fn config(&self) -> &OpenRouterConfig {
        &self.config
    }

    /// Zuletzt geladener Kontostand (nach [`OpenRouterAdapter::key_balance`] oder dem automatischen Neuladen nach
    /// einem endgültigen Kontingentfehler).
    pub fn last_balance(&self) -> Option<KeyBalance> {
        self.balance
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut builder = builder
            .bearer_auth(self.config.api_key.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json");
        if let Some(identity) = &self.config.app_identity {
            builder = builder
                .header("HTTP-Referer", identity.referer.as_str())
                .header("X-Title", identity.title.as_str());
        }
        builder
    }

    fn observe(&self, headers: &HeaderMap) {
        if let Some(limiter) = &self.limiter {
            limiter.observe_headers(PROVIDER_ID, headers);
        }
    }

    async fn get_json(&self, path: &str) -> Result<Value, AttemptFailure> {
        let response = self
            .headers(self.client.get(self.config.url(path)))
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
        if status.is_success() {
            Ok(body)
        } else {
            Err(self.classify_error(status, &headers, &body).0)
        }
    }

    /// Lädt die Modellliste.
    pub async fn list_models(&self) -> Result<Vec<OpenRouterModel>, AttemptFailure> {
        let body = self.get_json("/models").await?;
        Ok(body
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(OpenRouterModel::from_json)
            .collect())
    }

    /// Lädt den Kontostand (`GET /key`) und merkt ihn sich.
    pub async fn key_balance(&self) -> Result<KeyBalance, AttemptFailure> {
        let body = self.get_json("/key").await?;
        let balance = KeyBalance::from_json(&body)
            .ok_or_else(|| AttemptFailure::from(ProviderError::InvalidOutput))?;
        *self.balance.lock().unwrap_or_else(|p| p.into_inner()) = Some(balance.clone());
        Ok(balance)
    }

    /// Request-Körper für `POST /chat/completions`.
    pub fn request_body(&self, request: &Request, repair: Option<&RepairRequest>) -> Value {
        let mut content: Vec<Value> = request
            .input
            .iter()
            .map(|input| match input {
                Input::Text { text } => json!({"type": "text", "text": text}),
                Input::Image { media_type, base64 } => json!({
                    "type": "image_url",
                    "image_url": {"url": format!("data:{media_type};base64,{base64}")},
                }),
            })
            .collect();
        if let Some(repair) = repair {
            content.push(json!({"type": "text", "text": repair.instruction()}));
        }
        let mut messages = Vec::new();
        if !request.instructions.is_empty() {
            messages.push(json!({"role": "system", "content": request.instructions}));
        }
        messages.push(json!({"role": "user", "content": content}));
        let mut body = Map::new();
        body.insert("model".into(), json!(request.model.id));
        body.insert("messages".into(), Value::Array(messages));
        if let Some(max_tokens) = request.max_tokens {
            body.insert("max_tokens".into(), json!(max_tokens));
        }
        if let Some(temperature) = request.temperature {
            body.insert("temperature".into(), json!(temperature));
        }
        if let Some(schema) = &request.response_schema {
            body.insert(
                "response_format".into(),
                json!({
                    "type": "json_schema",
                    "json_schema": {"name": SCHEMA_NAME, "strict": true, "schema": schema},
                }),
            );
        }
        let mut provider = Map::new();
        let routing = &self.config.routing;
        if routing.deny_data_collection {
            provider.insert("data_collection".into(), json!("deny"));
        }
        if routing.zero_data_retention {
            provider.insert("zdr".into(), json!(true));
        }
        if !routing.order.is_empty() {
            provider.insert("order".into(), json!(routing.order));
            provider.insert("allow_fallbacks".into(), json!(false));
        }
        if request.response_schema.is_some() {
            provider.insert("require_parameters".into(), json!(true));
        }
        if !provider.is_empty() {
            body.insert("provider".into(), Value::Object(provider));
        }
        Value::Object(body)
    }

    /// Ein Versuch (siehe [`crate::adapters::anthropic::AnthropicAdapter::send`]). Beide Refusal-Formen sind `Ok` mit
    /// [`AdapterReply::is_refusal`].
    pub async fn send(
        &self,
        request: &Request,
        repair: Option<&RepairRequest>,
    ) -> Result<AdapterReply, AttemptFailure> {
        if let Some(blocked) = self.latch.blocked_failure() {
            return Err(blocked);
        }
        let body = self.request_body(request, repair);
        let structured = request.response_schema.is_some();
        let started = Instant::now();
        let response = self
            .headers(self.client.post(self.config.url("/chat/completions")))
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
        let parsed: Result<Value, _> = serde_json::from_slice(&bytes);
        if !status.is_success() {
            let body = parsed.unwrap_or(Value::Null);
            if status == StatusCode::FORBIDDEN
                && body.pointer("/error/type").and_then(Value::as_str) == Some("refusal")
            {
                // Form b: vor allen anderen 403-Regeln; keine Wiederholung, kein Kontingent, kein Schutzschalter.
                return Ok(AdapterReply {
                    raw: RawAnswer::Refusal,
                    usage: Usage::default(),
                    refusal_reason: Some(error_message(&body, status)),
                    stop_reason: None,
                    provider_request_id: request_id(&body, &headers),
                    http_status: status.as_u16(),
                    duration_ms,
                    structured,
                });
            }
            let (failure, final_quota) = self.classify_error(status, &headers, &body);
            if final_quota {
                // Die Kontostandsanzeige wird neu geladen (AI-005); ein Fehler dabei ändert den Eintrag nicht.
                let _ = self.key_balance().await;
            }
            return Err(failure);
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
                    provider_request_id: request_id(&Value::Null, &headers),
                    http_status: status.as_u16(),
                    duration_ms,
                    structured,
                })
            }
        };
        Ok(parse_completion(
            &body,
            &headers,
            status.as_u16(),
            duration_ms,
            structured,
        ))
    }

    /// Fehlerantwort → (Fehlerklasse, endgültiges Kontingent für alle Einträge).
    fn classify_error(
        &self,
        status: StatusCode,
        headers: &HeaderMap,
        body: &Value,
    ) -> (AttemptFailure, bool) {
        let code = status.as_u16();
        let message = error_message(body, status);
        let limit_source = body
            .pointer("/error/metadata/limit_source")
            .and_then(Value::as_str);
        let reason = body
            .pointer("/error/metadata/reason")
            .and_then(Value::as_str);
        let retry = retry_after(headers, wall_millis(self.limiter.as_ref()));
        let final_quota = match code {
            402 if limit_source == Some(IN_FLIGHT_BUDGET) => {
                return (
                    rate_limited(retry, code).with_error_code(IN_FLIGHT_BUDGET),
                    false,
                )
            }
            402 if reason == Some(WEIGHT_EXCEEDS_BUDGET) => {
                return (
                    quota_failure(code, Some(WEIGHT_EXCEEDS_BUDGET.to_string())),
                    false,
                )
            }
            // 402 ohne erkennbares `limit_source` gilt als endgültig (OS-Festlegung).
            402 => true,
            403 => limit_source.is_some_and(|s| FINAL_403_SOURCES.contains(&s)),
            _ => false,
        };
        if final_quota {
            let error_code = limit_source.map(str::to_string);
            self.latch.trip(QuotaStop {
                http_status: code,
                error_code: error_code.clone(),
                message,
            });
            return (quota_failure(code, error_code), true);
        }
        let failure = match code {
            429 => rate_limited(retry, code),
            _ => http_failure(code, message, retry),
        };
        let kind = body.pointer("/error/type").and_then(Value::as_str);
        let failure = match limit_source.or(kind) {
            Some(code) => failure.with_error_code(code),
            None => failure,
        };
        (failure, false)
    }
}

fn request_id(body: &Value, headers: &HeaderMap) -> Option<String> {
    body.get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            ["x-request-id", "x-generation-id"].iter().find_map(|name| {
                headers
                    .get(*name)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
            })
        })
}

/// Einheitliche Tokenzahlen: `prompt_tokens` zählt die gecachten Tokens bereits mit (Festlegung 24.11.2026);
/// fehlende Felder bleiben `None`.
pub fn usage_from_json(usage: Option<&Value>) -> Usage {
    let details = usage.and_then(|u| u.get("prompt_tokens_details"));
    Usage {
        input_tokens: opt_u64(usage.and_then(|u| u.get("prompt_tokens"))),
        output_tokens: opt_u64(usage.and_then(|u| u.get("completion_tokens"))),
        cached_input_tokens: opt_u64(details.and_then(|d| d.get("cached_tokens"))),
        cache_write_tokens: opt_u64(details.and_then(|d| d.get("cache_write_tokens"))),
    }
}

/// Erfolgsantwort von `POST /chat/completions` → [`AdapterReply`]. Form a: `message.refusal` (Text) oder
/// `finish_reason = "content_filter"`; Grund = Refusal-Text, ohne Text `content_filter`; kein Parse-Versuch.
fn parse_completion(
    body: &Value,
    headers: &HeaderMap,
    http_status: u16,
    duration_ms: u64,
    structured: bool,
) -> AdapterReply {
    let choice = body.pointer("/choices/0");
    let message = choice.and_then(|c| c.get("message"));
    let finish_reason = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let refusal_text = message
        .and_then(|m| m.get("refusal"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);
    let refusal = refusal_text.is_some() || finish_reason.as_deref() == Some(CONTENT_FILTER);
    let raw = if refusal {
        RawAnswer::Refusal
    } else if choice.is_none() {
        RawAnswer::Unparsable {
            detail: "response without choices".into(),
        }
    } else {
        let text = match message.and_then(|m| m.get("content")) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(parts)) => parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect(),
            _ => String::new(),
        };
        raw_from_text(text, structured, finish_reason.as_deref() == Some("length"))
    };
    AdapterReply {
        raw,
        usage: usage_from_json(body.get("usage")),
        refusal_reason: refusal.then(|| refusal_text.unwrap_or_else(|| CONTENT_FILTER.into())),
        stop_reason: finish_reason,
        provider_request_id: request_id(body, headers),
        http_status,
        duration_ms,
        structured,
    }
}

impl AiProvider for OpenRouterAdapter {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async move {
            self.list_models()
                .await
                .map(|models| models.iter().map(OpenRouterModel::to_model).collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_formatting_rounds_half_up() {
        assert_eq!(format_scaled(3_000_000, 10_000, 2), "3.00");
        assert_eq!(format_scaled(1_000_000_000, 100_000_000, 4), "0.0010");
        assert_eq!(format_micro_usd(5_800_000, 2), "5.80");
        assert_eq!(format_micro_usd(5_805_000, 2), "5.81");
        assert_eq!(format_micro_usd(0, 4), "0.0000");
    }
}

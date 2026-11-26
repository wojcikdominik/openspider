//! Adapter OpenAI Responses API (AI-003): `POST <basis>/responses` mit Bearer-Token, strukturierte Ausgabe über
//! `text.format` (`type: "json_schema"`, `name`, `schema`, `strict: true`), Bilder als `input_image` mit
//! Base64-Data-URL, Ausgabeteil `refusal` als Refusal, Modellliste über `GET <basis>/models` (ohne Preise und ohne
//! Fähigkeiten – die liefert der Modellkatalog, AI-010).
//!
//! Einstiege:
//! - [`OpenAiResponses::run_structured`]: ein Eintrag mit Schema-Aufgabe über [`Limiter`] (Wiederholungen nach
//!   INTEG-005, Timeout, Rate-Limit-Header) und [`run_with_repair`] (lokale Validierung, AI-021); liefert je Request
//!   die Protokolldaten ([`RequestMeta`]) für M4-K05.
//! - [`OpenAiResponses::run`]: ein Request über den Limiter ohne Schema-Prüfung (Freitext).
//! - [`AiProvider`]: ein einzelner Versuch ohne Limiter (Contract-Test AI-001).
pub mod client;
pub mod image;

use std::sync::Arc;

use serde_json::{json, Value};

use crate::adapters::anthropic::reasoning;
use crate::limits::{AttemptFailure, FinalError, Limiter, RequestSpec};
use crate::provider::{
    resolve_reasoning_level, AiProvider, Capabilities, CapabilitySource, Input, Model,
    ProviderError, ProviderFuture, ReasoningLevel, Request, Response, Usage,
};
use crate::schema::{
    run_with_repair, RawAnswer, RepairAttempts, RepairRequest, ResponseSchema, StructuredRun,
};
use os_model::ai::AiStatus;

pub use client::{
    parse_usage, ApiKey, HttpReply, OpenAiAccount, OpenAiConfig, OpenAiHttp, QuotaStop,
    RequestMeta, DEFAULT_BASE_URL, PROVIDER_ID,
};

/// Endpunktname im Modellkatalog und Protokoll.
pub const ENDPOINT: &str = "responses";
/// Name des Antwortschemas in `text.format.name`, wenn der Aufrufer keinen nennt.
pub const DEFAULT_SCHEMA_NAME: &str = "openspider_result";

/// Fähigkeiten eines Responses-Modells laut AI-001 (Text, Structured Output, Vision, Batch).
pub const RESPONSES_CAPABILITIES: Capabilities = Capabilities {
    text: true,
    structured_output: true,
    vision: true,
    embeddings: false,
    decisions: false,
    batch: true,
    prices_via_api: false,
    local: false,
};

/// Fähigkeiten je Modell-ID aus dem Modellkatalog (AI-010, M4-K04). `None` = unbekannt → keine Fähigkeit.
pub type CapabilityLookup = Arc<dyn Fn(&str) -> Option<Capabilities> + Send + Sync>;

/// Verfügbare Reasoning-Stufen je Modell-ID aus `reasoning_efforts` des Modellkatalogs (AI-010, Nachtrag; M4-K04,
/// `none` → `off`), aufsteigend; leer = kein `reasoning_efforts` (nur `model-default`).
pub type ReasoningLookup = Arc<dyn Fn(&str) -> Vec<ReasoningLevel> + Send + Sync>;

/// `incomplete_details.reason` des Abbruchs an der Ausgabegrenze (AI-003, Nachtrag Q00-H1).
pub const OUTPUT_LIMIT_REASON: &str = "max_output_tokens";

/// Was der Anbieter als Ausgabe geliefert hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsesOutput {
    /// Verkettete `output_text`-Teile.
    Text(String),
    /// Ausgabeteil `refusal` mit seinem Text.
    Refusal(String),
    /// Antwort abgebrochen (`status = "incomplete"` mit anderem Grund als `max_output_tokens`): wird nie teilweise
    /// übernommen; unlesbar mit Reparaturversuch (K02-Auslegung 9).
    Incomplete(String),
    /// Abbruch an der Ausgabegrenze (`incomplete_details.reason = "max_output_tokens"`, Nachtrag Q00-H1): kein
    /// Teilergebnis, kein Reparaturversuch; Grund = `Error Code`.
    OutputLimit(String),
    /// Antwort nicht lesbar (kein JSON, keine Ausgabe).
    Unparsable(String),
}

/// Ergebnis eines Responses-Requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesReply {
    pub output: ResponsesOutput,
    pub usage: Option<Usage>,
    /// `x-request-id`, sonst `id` der Antwort.
    pub provider_request_id: Option<String>,
    pub http_status: u16,
}

impl ResponsesReply {
    /// Rohantwort für [`run_with_repair`].
    pub fn raw_answer(&self) -> RawAnswer {
        match &self.output {
            ResponsesOutput::Text(text) => RawAnswer::Text(text.clone()),
            ResponsesOutput::Refusal(_) => RawAnswer::Refusal,
            ResponsesOutput::Incomplete(reason) => RawAnswer::Unparsable {
                detail: format!("response incomplete ({reason})"),
            },
            ResponsesOutput::OutputLimit(reason) => RawAnswer::Unparsable {
                detail: format!("output limit reached ({reason})"),
            },
            ResponsesOutput::Unparsable(detail) => RawAnswer::Unparsable {
                detail: detail.clone(),
            },
        }
    }
    fn refusal_text(&self) -> Option<String> {
        match &self.output {
            ResponsesOutput::Refusal(text) => Some(text.clone()),
            _ => None,
        }
    }
    /// Grund des Abbruchs an der Ausgabegrenze, falls die Antwort dort endete.
    pub fn output_limit(&self) -> Option<&str> {
        match &self.output {
            ResponsesOutput::OutputLimit(reason) => Some(reason),
            _ => None,
        }
    }
}

/// Endgültiger Fehler eines Abbruchs an der Ausgabegrenze: `Error: Output limit reached` (AI-020 Nachtrag Q00-H1,
/// M4-K10), nicht wiederholbar, `HTTP Status` der Antwort, `Error Code` = Grund (AI-014, Nachtrag).
pub fn output_limit_error(
    reason: &str,
    http_status: u16,
    provider_request_id: Option<String>,
) -> FinalError {
    FinalError {
        error: ProviderError::output_limit(reason),
        status: AiStatus::OutputLimitReached,
        http_status: Some(http_status),
        error_code: Some(reason.to_string()),
        message: format!("output limit reached ({reason})"),
        provider_request_id,
    }
}

/// Fehlgeschlagener Request innerhalb von [`OpenAiResponses::run_structured`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestFailure {
    pub error: FinalError,
    pub meta: RequestMeta,
}

/// Request-Body für `POST /responses`. Bei einer Reparatur (AI-021) folgt der Eingabe ein weiterer Textteil mit der
/// Meldung des Validators. Immer `"store": false` (AI-003, Nachtrag Q00-H2; auch Reparaturversuche); die
/// Reasoning-Stufe als `reasoning: {"effort": …}` nach den Stufen in `request.model.reasoning_levels` (Katalog
/// `reasoning_efforts`); kein `prompt_cache_retention` (V-A8).
pub fn responses_body(
    request: &Request,
    schema_name: &str,
    repair: Option<&RepairRequest>,
) -> Value {
    let mut content: Vec<Value> = request
        .input
        .iter()
        .map(|part| match part {
            Input::Text { text } => json!({"type": "input_text", "text": text}),
            Input::Image { media_type, base64 } => json!({
                "type": "input_image",
                "image_url": format!("data:{media_type};base64,{base64}"),
            }),
        })
        .collect();
    if let Some(repair) = repair {
        content.push(json!({"type": "input_text", "text": repair.instruction()}));
    }
    let mut body = json!({
        "model": request.model.id,
        "input": [{"role": "user", "content": content}],
        "store": false,
    });
    let effective = resolve_reasoning_level(request.reasoning, &request.model.reasoning_levels);
    if let Some(reasoning) = reasoning::reasoning_object(effective) {
        body["reasoning"] = reasoning;
    }
    if !request.instructions.trim().is_empty() {
        body["instructions"] = json!(request.instructions);
    }
    if let Some(schema) = &request.response_schema {
        body["text"] = json!({"format": {
            "type": "json_schema",
            "name": schema_name,
            "schema": schema,
            "strict": true,
        }});
    }
    if let Some(max) = request.max_tokens {
        body["max_output_tokens"] = json!(max);
    }
    if let Some(t) = request.temperature {
        body["temperature"] = json!(t);
    }
    body
}

/// Liest eine Responses-Antwort: `output[]` → Nachrichten → Teile `output_text` bzw. `refusal`; `usage` nach
/// [`parse_usage`]. Unbekannte Felder und Ausgabetypen werden ignoriert; Elemente vom Typ `reasoning` werden verworfen
/// (AI-001, Nachtrag).
pub fn parse_responses(reply: &HttpReply) -> ResponsesReply {
    let body: Value = match serde_json::from_slice(&reply.body) {
        Ok(v) => v,
        Err(e) => {
            return ResponsesReply {
                output: ResponsesOutput::Unparsable(format!(
                    "response body is not valid JSON: {e}"
                )),
                usage: None,
                provider_request_id: reply.request_id.clone(),
                http_status: reply.status,
            }
        }
    };
    let mut texts = Vec::new();
    let mut refusals = Vec::new();
    for item in body["output"].as_array().into_iter().flatten() {
        if item["type"] != "message" {
            continue;
        }
        for part in item["content"].as_array().into_iter().flatten() {
            match part["type"].as_str() {
                Some("output_text") => {
                    texts.push(part["text"].as_str().unwrap_or("").to_string());
                }
                Some("refusal") => {
                    refusals.push(part["refusal"].as_str().unwrap_or("").to_string());
                }
                _ => {}
            }
        }
    }
    let output = if !refusals.is_empty() {
        ResponsesOutput::Refusal(refusals.join("\n"))
    } else if body["status"] == "incomplete" {
        let reason = body["incomplete_details"]["reason"]
            .as_str()
            .unwrap_or("incomplete")
            .to_string();
        if reason == OUTPUT_LIMIT_REASON {
            ResponsesOutput::OutputLimit(reason)
        } else {
            ResponsesOutput::Incomplete(reason)
        }
    } else if texts.is_empty() {
        ResponsesOutput::Unparsable("response contains no output_text".into())
    } else {
        ResponsesOutput::Text(texts.concat())
    };
    ResponsesReply {
        output,
        usage: parse_usage(&body),
        provider_request_id: reply
            .request_id
            .clone()
            .or_else(|| body["id"].as_str().map(str::to_string)),
        http_status: reply.status,
    }
}

/// Adapter OpenAI Responses.
#[derive(Clone)]
pub struct OpenAiResponses {
    http: OpenAiHttp,
    capabilities: CapabilityLookup,
    reasoning_levels: ReasoningLookup,
    schema_name: String,
}

impl std::fmt::Debug for OpenAiResponses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiResponses")
            .field("http", &self.http)
            .field("schema_name", &self.schema_name)
            .finish_non_exhaustive()
    }
}

impl OpenAiResponses {
    /// Ohne Katalog meldet [`AiProvider::models`] jedes Modell ohne Fähigkeiten (konservativ); M4-K04 setzt den
    /// Katalog mit [`OpenAiResponses::with_capabilities`].
    pub fn new(http: OpenAiHttp) -> Self {
        Self {
            http,
            capabilities: Arc::new(|_| None),
            reasoning_levels: Arc::new(|_| Vec::new()),
            schema_name: DEFAULT_SCHEMA_NAME.to_string(),
        }
    }
    pub fn with_capabilities(mut self, lookup: CapabilityLookup) -> Self {
        self.capabilities = lookup;
        self
    }
    /// Reasoning-Stufen je Modell aus dem Katalog (M4-K04); [`OpenAiResponses::list_models`] trägt sie in
    /// [`Model::reasoning_levels`] ein. Der Request-Body richtet sich nach dem Modell im Request.
    pub fn with_reasoning_levels(mut self, lookup: ReasoningLookup) -> Self {
        self.reasoning_levels = lookup;
        self
    }
    /// Name des Antwortschemas (`^[a-zA-Z0-9_-]{1,64}$` laut Anbieter).
    pub fn with_schema_name(mut self, name: impl Into<String>) -> Self {
        self.schema_name = name.into();
        self
    }
    pub fn http(&self) -> &OpenAiHttp {
        &self.http
    }

    /// Ein Versuch ohne Wiederholung.
    pub async fn send(
        &self,
        request: &Request,
        repair: Option<&RepairRequest>,
        observer: Option<&Limiter>,
    ) -> Result<ResponsesReply, AttemptFailure> {
        let body = responses_body(request, &self.schema_name, repair);
        let reply = self.http.post_json(ENDPOINT, &body, observer).await?;
        Ok(parse_responses(&reply))
    }

    async fn run_once(
        &self,
        limiter: &Limiter,
        request: &Request,
        repair: Option<&RepairRequest>,
        estimated_input_tokens: u64,
    ) -> (Result<ResponsesReply, FinalError>, RequestMeta) {
        let spec = RequestSpec::new(
            PROVIDER_ID,
            request.model.id.clone(),
            estimated_input_tokens,
        );
        let exec = limiter
            .run_classified(
                &spec,
                |_| self.send(request, repair, Some(limiter)),
                // AI-017 (Nachtrag 26.11.2026): nur eine Antwort mit Text setzt zurück; Refusal, Ausgabegrenze und
                // unlesbare Antworten sind neutral.
                |reply| match reply.output {
                    ResponsesOutput::Text(_) => crate::limits::BreakerEffect::Reset,
                    _ => crate::limits::BreakerEffect::Neutral,
                },
            )
            .await;
        let mut meta = RequestMeta::from_execution(
            &exec,
            exec.result.as_ref().ok().map(|r| {
                (
                    r.http_status,
                    r.usage.clone(),
                    r.provider_request_id.clone(),
                )
            }),
        );
        if let Ok(reply) = &exec.result {
            meta.refusal_reason = reply.refusal_text();
            if let Some(reason) = reply.output_limit() {
                meta.error_code = Some(reason.to_string());
                meta.error_message = Some(format!("output limit reached ({reason})"));
            }
        }
        (exec.result, meta)
    }

    /// Ein Request über den Limiter (Freitext ohne Schema-Prüfung).
    pub async fn run(
        &self,
        limiter: &Limiter,
        request: &Request,
        estimated_input_tokens: u64,
    ) -> (Result<ResponsesReply, FinalError>, RequestMeta) {
        self.run_once(limiter, request, None, estimated_input_tokens)
            .await
    }

    /// Schema-Aufgabe: sendet `text.format` mit `strict: true`, parst den Ausgabetext und validiert ihn lokal gegen
    /// `schema` mit `repairs` Reparaturversuchen (AI-021). Ein Refusal wird nicht geparst und nicht repariert; sein
    /// Text steht in [`RequestMeta::refusal_reason`].
    pub async fn run_structured(
        &self,
        limiter: &Limiter,
        request: &Request,
        schema: &ResponseSchema,
        repairs: RepairAttempts,
        estimated_input_tokens: u64,
    ) -> StructuredRun<RequestMeta, RequestFailure> {
        let mut request = request.clone();
        request.response_schema = Some(schema.schema().clone());
        let request = &request;
        run_with_repair(schema, repairs, |repair| async move {
            let (result, meta) = self
                .run_once(limiter, request, repair.as_ref(), estimated_input_tokens)
                .await;
            match result {
                // Abbruch an der Ausgabegrenze: Eintrag endet ohne Reparaturversuch (AI-021, Nachtrag Q00-H1).
                Ok(reply) if reply.output_limit().is_some() => Err(RequestFailure {
                    error: output_limit_error(
                        reply.output_limit().unwrap_or(OUTPUT_LIMIT_REASON),
                        reply.http_status,
                        reply.provider_request_id.clone(),
                    ),
                    meta,
                }),
                Ok(reply) => Ok((reply.raw_answer(), meta)),
                Err(error) => Err(RequestFailure { error, meta }),
            }
        })
        .await
    }

    /// Modellliste `GET <basis>/models`; Fähigkeiten und Reasoning-Stufen aus dem Katalog.
    pub async fn list_models(&self) -> Result<Vec<Model>, AttemptFailure> {
        let mut models = list_models(&self.http, ENDPOINT, &self.capabilities).await?;
        for model in &mut models {
            model.reasoning_levels = (self.reasoning_levels)(&model.id);
        }
        Ok(models)
    }
}

/// `GET <basis>/models` → Modelle des Endpunkts `endpoint` mit Fähigkeiten aus `lookup`.
pub(crate) async fn list_models(
    http: &OpenAiHttp,
    endpoint: &str,
    lookup: &CapabilityLookup,
) -> Result<Vec<Model>, AttemptFailure> {
    let reply = http.get("models", None).await?;
    let body: Value = serde_json::from_slice(&reply.body)
        .map_err(|_| AttemptFailure::from(ProviderError::InvalidOutput))?;
    Ok(body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str())
        .map(|id| Model {
            provider: PROVIDER_ID.to_string(),
            id: id.to_string(),
            endpoint: endpoint.to_string(),
            capabilities: lookup(id).unwrap_or_default(),
            capability_source: CapabilitySource::ModelCatalog,
            reasoning_levels: Vec::new(),
        })
        .collect())
}

impl AiProvider for OpenAiResponses {
    fn provider_id(&self) -> &str {
        PROVIDER_ID
    }

    fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
        Box::pin(async move { self.list_models().await.map_err(|f| f.error) })
    }

    /// Ein Versuch: Schema-Aufgaben liefern das geparste JSON (nicht parsebar → `Error: Invalid output`), andere den
    /// Text; Refusals kommen mit `refusal = true` und ohne Inhalt.
    fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            let reply = self
                .send(&request, None, self.http.limiter())
                .await
                .map_err(|f| f.error)?;
            let (content, refusal) = match &reply.output {
                ResponsesOutput::Refusal(_) => (None, true),
                ResponsesOutput::Text(text) if request.response_schema.is_some() => (
                    Some(
                        serde_json::from_str::<Value>(text.trim())
                            .map_err(|_| ProviderError::InvalidOutput)?,
                    ),
                    false,
                ),
                ResponsesOutput::Text(text) => (Some(Value::String(text.clone())), false),
                ResponsesOutput::OutputLimit(reason) => {
                    return Err(ProviderError::output_limit(reason.clone()))
                }
                ResponsesOutput::Incomplete(_) | ResponsesOutput::Unparsable(_) => {
                    return Err(ProviderError::InvalidOutput)
                }
            };
            Ok(Response {
                content,
                usage: reply.usage.unwrap_or_default(),
                refusal,
                provider_request_id: reply.provider_request_id,
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            })
        })
    }
}

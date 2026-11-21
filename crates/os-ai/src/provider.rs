//! Common contracts (AI-001). Model capabilities come from their declared source.
use crate::preview::PreviewSwitch;
use os_model::ai::AiStatus;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{future::Future, pin::Pin};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub text: bool,
    pub structured_output: bool,
    pub vision: bool,
    pub embeddings: bool,
    pub decisions: bool,
    pub batch: bool,
    pub prices_via_api: bool,
    pub local: bool,
}
impl Capabilities {
    /// Every requested capability must be supported, including image input for Decisions.
    pub fn supports(self, required: Self) -> bool {
        (!required.text || self.text)
            && (!required.structured_output || self.structured_output)
            && (!required.vision || self.vision)
            && (!required.embeddings || self.embeddings)
            && (!required.decisions || self.decisions)
            && (!required.batch || self.batch)
            && (!required.prices_via_api || self.prices_via_api)
            && (!required.local || self.local)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySource {
    AnthropicModels,
    OpenRouterModels,
    ModelCatalog,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub provider: String,
    pub id: String,
    pub endpoint: String,
    pub capabilities: Capabilities,
    pub capability_source: CapabilitySource,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Input {
    Text { text: String },
    Image { media_type: String, base64: String },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub model: Model,
    pub required: Capabilities,
    pub input: Vec<Input>,
    pub instructions: String,
    /// Decision question definitions (empty for other tasks).
    pub questions: Vec<Value>,
    pub response_schema: Option<Value>,
    pub max_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub effort: Option<String>,
}
impl Request {
    /// Derive mandatory requirements from the payload too, preventing a misdeclared image request.
    pub fn requirements(&self) -> Capabilities {
        Capabilities {
            vision: self.required.vision
                || self.input.iter().any(|v| matches!(v, Input::Image { .. })),
            structured_output: self.required.structured_output || self.response_schema.is_some(),
            ..self.required
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub content: Option<Value>,
    pub usage: Usage,
    pub refusal: bool,
    pub provider_request_id: Option<String>,
    pub duration_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderError {
    MissingApiKey,
    Timeout { seconds: u64 },
    InvalidOutput,
    MissingAnswer,
    BatchExpired,
    QuotaExceeded,
    RateLimited { retry_after_seconds: Option<u64> },
    Overloaded,
    Transport { message: String },
    Refusal,
    Policy,
    Http { status: u16, message: String },
}
impl ProviderError {
    /// STUB(M4-K06): final transport presentation needs spec clarification; no HTTP code is invented.
    pub fn status(&self) -> Option<AiStatus> {
        Some(match self {
            Self::MissingApiKey => AiStatus::MissingApiKey,
            Self::Timeout { seconds } => AiStatus::Timeout { seconds: *seconds },
            Self::InvalidOutput => AiStatus::InvalidOutput,
            Self::MissingAnswer => AiStatus::MissingAnswer,
            Self::BatchExpired => AiStatus::BatchExpired,
            Self::QuotaExceeded => AiStatus::QuotaExceeded,
            Self::Refusal => AiStatus::Refusal,
            Self::Policy => AiStatus::SkippedPolicy,
            Self::Http { status, message } => AiStatus::HttpError {
                code: *status,
                message: message.clone(),
            },
            // These provider classes carry the actual HTTP status, never a quota decision.
            Self::RateLimited { .. } => AiStatus::HttpError {
                code: 429,
                message: "Rate limit".into(),
            },
            Self::Overloaded => AiStatus::HttpError {
                code: 529,
                message: "Overloaded".into(),
            },
            Self::Transport { .. } => return None,
        })
    }
}
pub type ProviderFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ProviderError>> + Send + 'a>>;
/// Object-safe async interface; embedding and batch operations are provided by their service modules.
pub trait AiProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn models(&self) -> ProviderFuture<'_, Vec<Model>>;
    fn execute(&self, request: Request) -> ProviderFuture<'_, Response>;
}
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Schalter „KI (Vorschau)“ aus: kein Request, keine Statuszeile (Plan M4 E1, E-2 b).
    Disabled,
    Completed(Response),
    Status(AiStatus),
    ProviderFailure(ProviderError),
}
/// Mandatory guards, evaluated before invoking the adapter: the preview switch first, then routing.
pub async fn execute(
    switch: PreviewSwitch,
    provider: &dyn AiProvider,
    request: Request,
) -> Outcome {
    if !switch.is_on() {
        return Outcome::Disabled;
    }
    if request.model.provider != provider.provider_id()
        || !request.model.capabilities.supports(request.requirements())
    {
        return Outcome::Status(AiStatus::SkippedNoCapableModel);
    }
    match provider.execute(request).await {
        Ok(response) if response.refusal => Outcome::Status(AiStatus::Refusal),
        Ok(response) => Outcome::Completed(response),
        Err(error) => match error.status() {
            Some(status) => Outcome::Status(status),
            None => Outcome::ProviderFailure(error),
        },
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport { message } => write!(f, "Transport: {message}"),
            _ => self
                .status()
                .expect("non-transport errors have a status")
                .fmt(f),
        }
    }
}
impl std::error::Error for ProviderError {}

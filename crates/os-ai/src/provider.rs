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
/// Reasoning-Stufe einer Aufgabe (AI-011, Nachtrag 24.11.2026, Q00). Ordnung nur über [`ReasoningLevel::rank`]:
/// `ModelDefault` steht außerhalb der Skala (kein Parameter, Verhalten des Modells). Serde-, Anzeige- und
/// Protokollnamen: `model-default`, `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ReasoningLevel {
    #[default]
    #[serde(rename = "model-default")]
    ModelDefault,
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "minimal")]
    Minimal,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "xhigh")]
    XHigh,
    #[serde(rename = "max")]
    Max,
}

impl ReasoningLevel {
    /// Die sieben Stufen der Skala, aufsteigend (ohne `ModelDefault`).
    pub const EXPLICIT: [ReasoningLevel; 7] = [
        Self::Off,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::XHigh,
        Self::Max,
    ];

    /// Kanonischer Name (Profil, `task-defaults.json`, Protokollspalte `Reasoning Level`, Cache-Schlüssel).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ModelDefault => "model-default",
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }

    /// Rang auf der Skala = Position in [`ReasoningLevel::EXPLICIT`]: `Off` = 0, `Minimal` = 1 … `Max` = 6;
    /// `ModelDefault` = `None`. Nur die Reihenfolge ist Vertrag (Hinweis im Bericht K09: der Planvertrag nennt
    /// „`Max` = 7“ bei sieben Stufen).
    pub fn rank(self) -> Option<u8> {
        match self {
            Self::ModelDefault => None,
            Self::Off => Some(0),
            Self::Minimal => Some(1),
            Self::Low => Some(2),
            Self::Medium => Some(3),
            Self::High => Some(4),
            Self::XHigh => Some(5),
            Self::Max => Some(6),
        }
    }
}

impl std::fmt::Display for ReasoningLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unbekannter Stufenname.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownReasoningLevel(pub String);

impl std::fmt::Display for UnknownReasoningLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown reasoning level: {}", self.0)
    }
}

impl std::error::Error for UnknownReasoningLevel {}

impl std::str::FromStr for ReasoningLevel {
    type Err = UnknownReasoningLevel;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        std::iter::once(Self::ModelDefault)
            .chain(Self::EXPLICIT)
            .find(|level| level.as_str() == s)
            .ok_or_else(|| UnknownReasoningLevel(s.to_string()))
    }
}

/// Effektive Stufe (AI-011, Nachtrag) – die einzige Stelle dieser Regel: `ModelDefault` oder leere Liste →
/// `ModelDefault`; `requested` verfügbar → `requested`; sonst die höchste verfügbare Stufe unter `requested`, gibt es
/// keine, die niedrigste verfügbare. `available` enthält `Off` nur, wenn der Anbieter das Abschalten bestätigt („sicher
/// aus“); ein `ModelDefault` in `available` zählt nicht als Stufe.
pub fn resolve_reasoning_level(
    requested: ReasoningLevel,
    available: &[ReasoningLevel],
) -> ReasoningLevel {
    let Some(wanted) = requested.rank() else {
        return ReasoningLevel::ModelDefault;
    };
    let ranked = || available.iter().filter_map(|l| l.rank().map(|r| (r, *l)));
    if ranked().any(|(rank, _)| rank == wanted) {
        return requested;
    }
    ranked()
        .filter(|(rank, _)| *rank < wanted)
        .max_by_key(|(rank, _)| *rank)
        .or_else(|| ranked().min_by_key(|(rank, _)| *rank))
        .map_or(ReasoningLevel::ModelDefault, |(_, level)| level)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub provider: String,
    pub id: String,
    pub endpoint: String,
    pub capabilities: Capabilities,
    pub capability_source: CapabilitySource,
    /// Verfügbare Reasoning-Stufen des Modells, aufsteigend, ohne `ModelDefault` (AI-011, Nachtrag); leer = nur
    /// `ModelDefault` (Ollama, Decisions, Embeddings, Modelle ohne Stufen).
    #[serde(default)]
    pub reasoning_levels: Vec<ReasoningLevel>,
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
    /// Effektive Reasoning-Stufe (AI-001/AI-011, Nachtrag 24.11.2026): der Aufrufer setzt die Stufe nach
    /// [`resolve_reasoning_level`]; der Adapter übersetzt sie (AI-002, AI-003, AI-005). Standard `ModelDefault`.
    #[serde(default)]
    pub reasoning: ReasoningLevel,
}
impl Request {
    /// Derive mandatory requirements from the payload too, preventing a misdeclared image request.
    /// AI-001: the Decisions capability covers text *and* image input, so an image in a decision request needs
    /// `decisions` (already required), not `vision`; every other image request still needs `vision`.
    pub fn requirements(&self) -> Capabilities {
        let has_image = self.input.iter().any(|v| matches!(v, Input::Image { .. }));
        Capabilities {
            vision: self.required.vision || (has_image && !self.required.decisions),
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
    /// Teilmenge von `output_tokens` (AI-001, Nachtrag 24.11.2026); fehlt das Feld beim Anbieter, `None`.
    #[serde(default)]
    pub reasoning_tokens: Option<u64>,
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
    Timeout {
        seconds: u64,
    },
    InvalidOutput,
    MissingAnswer,
    BatchExpired,
    QuotaExceeded,
    RateLimited {
        retry_after_seconds: Option<u64>,
    },
    Overloaded,
    Transport {
        message: String,
    },
    Refusal,
    Policy,
    Http {
        status: u16,
        message: String,
    },
    /// Abbruch an der Ausgabegrenze (AI-020, Nachtrag 24.11.2026, Q00-H1): `reason` ist der Grund des Anbieters
    /// (`max_tokens`, `max_output_tokens`, `length`) für die Protokollspalte `Error Code`. Kein Teilergebnis, kein
    /// Reparaturversuch (AI-021), keine Wiederholung, nicht im Schutzschalter (AI-017).
    OutputLimitReached {
        reason: String,
    },
}
impl ProviderError {
    /// Fehlerklasse „Abbruch an der Ausgabegrenze“ mit dem Grund des Anbieters (M4-K10).
    pub fn output_limit(reason: impl Into<String>) -> Self {
        Self::OutputLimitReached {
            reason: reason.into(),
        }
    }

    /// Transport errors without an HTTP response map to `Error: Connection failed` (AI-020, K4-15); no HTTP code is
    /// invented. Retrying them and the `transport/<art>` log code belong to [`crate::limits`].
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
            Self::Transport { .. } => AiStatus::ConnectionFailed,
            Self::OutputLimitReached { .. } => AiStatus::OutputLimitReached,
        })
    }
}

/// Abbildung des Ausgabegrenzen-Abbruchs aus M4-K09 ([`crate::adapters::anthropic::OutputLimitReached`], gemeinsam für
/// Anthropic, OpenRouter und Ollama) auf `Error: Output limit reached` (M4-K10, AI-020 Nachtrag). Die älteren
/// Methoden `provider_error`/`attempt_failure` des Adapters bilden bis zur Umstellung der Adapterdateien noch auf
/// `Error: Invalid output` ab (Bericht M4-K10, Tabelle fremder Dateien).
impl crate::adapters::anthropic::OutputLimitReached {
    /// Fehlerklasse `Error: Output limit reached` mit dem Grund des Anbieters.
    pub fn output_limit_error(&self) -> ProviderError {
        ProviderError::output_limit(self.reason.clone())
    }

    /// Fehler eines Versuchs für [`crate::limits::Limiter::run`] bzw. [`crate::schema::run_with_repair`]: nicht
    /// wiederholbar, `HTTP Status` der Antwort (200), `Error Code` = Grund, Anbieter-Request-ID der Antwort.
    pub fn output_limit_failure(&self) -> crate::limits::AttemptFailure {
        let mut failure = crate::limits::AttemptFailure::from(self.output_limit_error())
            .with_error_code(&self.reason);
        failure.http_status = Some(self.http_status);
        failure.with_request_id(self.provider_request_id.clone())
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
        // A single attempt without HTTP response stays a provider failure: it is transient (INTEG-005), and only
        // `crate::limits` turns it into `Error: Connection failed` after all retries (AI-020, K4-15).
        Err(error @ ProviderError::Transport { .. }) => Outcome::ProviderFailure(error),
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

#[cfg(test)]
mod reasoning_tests {
    use super::*;
    use proptest::prelude::*;

    const ALL: [ReasoningLevel; 8] = [
        ReasoningLevel::ModelDefault,
        ReasoningLevel::Off,
        ReasoningLevel::Minimal,
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
        ReasoningLevel::XHigh,
        ReasoningLevel::Max,
    ];

    /// Teilmenge von `EXPLICIT` aus einer Bitmaske (aufsteigend).
    fn subset(mask: u8) -> Vec<ReasoningLevel> {
        ReasoningLevel::EXPLICIT
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, l)| *l)
            .collect()
    }

    /// Wortlaut der Regel aus AI-011 (Nachtrag) über Indizes in `EXPLICIT`, unabhängig von `rank()`.
    fn reference(requested: ReasoningLevel, available: &[ReasoningLevel]) -> ReasoningLevel {
        let index = |l: ReasoningLevel| ReasoningLevel::EXPLICIT.iter().position(|x| *x == l);
        let Some(wanted) = index(requested) else {
            return ReasoningLevel::ModelDefault;
        };
        if available.is_empty() {
            return ReasoningLevel::ModelDefault;
        }
        if available.contains(&requested) {
            return requested;
        }
        let below = (0..wanted)
            .rev()
            .map(|i| ReasoningLevel::EXPLICIT[i])
            .find(|l| available.contains(l));
        below.unwrap_or_else(|| {
            *ReasoningLevel::EXPLICIT
                .iter()
                .find(|l| available.contains(l))
                .unwrap()
        })
    }

    // spec: AI-011 [M4]
    #[test]
    fn names_serde_display_and_from_str_agree() {
        let names = [
            "model-default",
            "off",
            "minimal",
            "low",
            "medium",
            "high",
            "xhigh",
            "max",
        ];
        for (level, name) in ALL.iter().zip(names) {
            assert_eq!(level.as_str(), name);
            assert_eq!(level.to_string(), name);
            assert_eq!(name.parse::<ReasoningLevel>(), Ok(*level));
            assert_eq!(
                serde_json::to_value(level).unwrap(),
                serde_json::json!(name)
            );
            assert_eq!(
                serde_json::from_value::<ReasoningLevel>(serde_json::json!(name)).unwrap(),
                *level
            );
        }
        assert!("x-high".parse::<ReasoningLevel>().is_err());
        assert!("none".parse::<ReasoningLevel>().is_err());
        assert!(serde_json::from_value::<ReasoningLevel>(serde_json::json!("Max")).is_err());
        assert_eq!(ReasoningLevel::default(), ReasoningLevel::ModelDefault);
    }

    // spec: AI-011 [M4]
    #[test]
    fn rank_orders_the_explicit_scale_and_excludes_model_default() {
        assert_eq!(ReasoningLevel::ModelDefault.rank(), None);
        let ranks: Vec<u8> = ReasoningLevel::EXPLICIT
            .iter()
            .map(|l| l.rank().unwrap())
            .collect();
        assert_eq!(ranks, (0..7).collect::<Vec<u8>>());
        assert_eq!(ReasoningLevel::Off.rank(), Some(0));
    }

    // spec: AI-011 [M4]
    #[test]
    fn resolve_matches_the_rule_for_every_request_and_every_subset() {
        for requested in ALL {
            for mask in 0u8..128 {
                let available = subset(mask);
                assert_eq!(
                    resolve_reasoning_level(requested, &available),
                    reference(requested, &available),
                    "{requested} auf {available:?}"
                );
            }
        }
    }

    // spec: AI-011 [M4]
    #[test]
    fn resolve_examples_from_the_spec() {
        use ReasoningLevel::*;
        // Anthropic-Modell B: nicht abschaltbar, niedrigste Stufe low.
        assert_eq!(
            resolve_reasoning_level(Off, &[Low, Medium, High, XHigh]),
            Low
        );
        // Modell B ohne max: max → xhigh.
        assert_eq!(
            resolve_reasoning_level(Max, &[Low, Medium, High, XHigh]),
            XHigh
        );
        // OpenAI-Katalog none/low/medium/high: max → high, off → off.
        assert_eq!(
            resolve_reasoning_level(Max, &[Off, Low, Medium, High]),
            High
        );
        assert_eq!(resolve_reasoning_level(Off, &[Off, Low, Medium, High]), Off);
        // Minimal fehlt → nächstniedrigere (off), ohne off die niedrigste (low).
        assert_eq!(resolve_reasoning_level(Minimal, &[Off, Low]), Off);
        assert_eq!(resolve_reasoning_level(Minimal, &[Low, High]), Low);
        // Leere Liste und ModelDefault.
        assert_eq!(resolve_reasoning_level(Medium, &[]), ModelDefault);
        assert_eq!(resolve_reasoning_level(ModelDefault, &[Low]), ModelDefault);
        // ModelDefault in der Liste ist keine Stufe.
        assert_eq!(resolve_reasoning_level(Low, &[ModelDefault]), ModelDefault);
    }

    // spec: AI-001 [M4]
    #[test]
    fn request_usage_and_model_default_the_new_fields() {
        let usage: Usage = serde_json::from_value(serde_json::json!({
            "input_tokens": 1, "output_tokens": 2, "cached_input_tokens": null, "cache_write_tokens": null
        }))
        .unwrap();
        assert_eq!(usage.reasoning_tokens, None);
        assert_eq!(Usage::default().reasoning_tokens, None);
        let model: Model = serde_json::from_value(serde_json::json!({
            "provider": "p", "id": "m", "endpoint": "e",
            "capabilities": Capabilities::default(), "capability_source": "model_catalog"
        }))
        .unwrap();
        assert!(model.reasoning_levels.is_empty());
        let request: Request = serde_json::from_value(serde_json::json!({
            "model": model, "required": Capabilities::default(), "input": [], "instructions": "",
            "questions": [], "response_schema": null, "max_tokens": null, "temperature": null, "effort": null
        }))
        .unwrap();
        assert_eq!(request.reasoning, ReasoningLevel::ModelDefault);
    }

    proptest! {
        // spec: AI-011 [M4]
        #[test]
        fn resolve_returns_an_available_level_never_above_a_lower_alternative(
            requested in proptest::sample::select(ALL.to_vec()),
            mut available in proptest::collection::vec(proptest::sample::select(ALL.to_vec()), 0..10),
        ) {
            // Beliebige Reihenfolge und Doppelungen ändern das Ergebnis nicht.
            let effective = resolve_reasoning_level(requested, &available);
            available.reverse();
            prop_assert_eq!(resolve_reasoning_level(requested, &available), effective);
            let explicit: Vec<_> = available.iter().copied().filter(|l| l.rank().is_some()).collect();
            if requested == ReasoningLevel::ModelDefault || explicit.is_empty() {
                prop_assert_eq!(effective, ReasoningLevel::ModelDefault);
            } else {
                prop_assert!(explicit.contains(&effective));
                let (want, got) = (requested.rank().unwrap(), effective.rank().unwrap());
                if got > want {
                    // Nur wenn keine Stufe darunter verfügbar ist, und dann die niedrigste.
                    prop_assert!(explicit.iter().all(|l| l.rank().unwrap() >= got));
                } else {
                    // Keine verfügbare Stufe liegt zwischen der effektiven und der gewünschten.
                    let gap_free = explicit
                        .iter()
                        .filter_map(|l| l.rank())
                        .all(|r| r <= got || r > want);
                    prop_assert!(gap_free);
                }
            }
        }
    }
}

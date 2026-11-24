//! Fester Anfrageplan je Anbieter (abgeleitet aus 08: AI-002 … AI-005, AI-009, AI-010, AI-017, V-A1, V-A5) und die
//! Kostenobergrenze jeder Anfrage. Der Trockenlauf druckt genau diese Liste; der Live-Lauf arbeitet sie ab (jede
//! Anfrage höchstens einmal).

use serde::Serialize;

/// Anbieter der Probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    OpenAi,
    OpenRouter,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Anthropic, Provider::OpenAi, Provider::OpenRouter];

    /// Name in Optionen und Dateinamen.
    pub fn name(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
            Provider::OpenRouter => "openrouter",
        }
    }

    /// Konto im Schlüsselbund (AI-009, K4-12).
    pub fn account(self) -> &'static str {
        match self {
            Provider::Anthropic => "ai.anthropic",
            Provider::OpenAi => "ai.openai",
            Provider::OpenRouter => "ai.openrouter",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Provider::ALL.into_iter().find(|p| p.name() == text)
    }

    /// Höchstzahl der HTTP-Anfragen dieses Anbieters in einem Lauf (feste Konstante; der Plan liegt darunter).
    pub fn max_requests(self) -> usize {
        match self {
            Provider::Anthropic => 14,
            Provider::OpenAi => 18,
            Provider::OpenRouter => 14,
        }
    }
}

/// Basis-URLs (keine EU-Endpunkte). Das Programm nutzt immer [`Endpoints::default`]; andere Werte setzen nur Tests
/// (lokaler Testserver), es gibt keine Option dafür.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// Ohne `/v1` (der Adapter hängt `/v1/...` an).
    pub anthropic: String,
    /// Mit `/v1`.
    pub openai: String,
    /// Mit `/api/v1`.
    pub openrouter: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            anthropic: "https://api.anthropic.com".into(),
            openai: "https://api.openai.com/v1".into(),
            openrouter: "https://openrouter.ai/api/v1".into(),
        }
    }
}

impl Endpoints {
    pub fn base(&self, provider: Provider) -> &str {
        match provider {
            Provider::Anthropic => &self.anthropic,
            Provider::OpenAi => &self.openai,
            Provider::OpenRouter => &self.openrouter,
        }
        .trim_end_matches('/')
    }
}

/// Roh (eigener `reqwest`-Aufruf nach Spec-Wortlaut) oder über den Adapter aus `os-ai`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    Raw,
    Adapter,
}

impl Via {
    pub fn label(self) -> &'static str {
        match self {
            Via::Raw => "roh",
            Via::Adapter => "Adapter",
        }
    }
}

/// Welches Modell eine Anfrage braucht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelNeed {
    /// Keins (Modellliste, Kontostand).
    None,
    /// Textmodell aus der Liste.
    Text,
    /// Modell mit strukturierter Ausgabe.
    Structured,
    /// Modell mit Bildeingabe.
    Vision,
    /// Feste Spec-ID `gpt-6-luna` (nur Decisions, deren Existenz geprüft wird).
    SpecLuna,
    /// Absichtlich unbekannte ID.
    Unknown,
}

/// Eine geplante Anfrage.
#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub id: &'static str,
    pub provider: Provider,
    pub via: Via,
    pub method: &'static str,
    /// Pfad relativ zur Basis-URL.
    pub path: &'static str,
    pub purpose: &'static str,
    pub model: ModelNeed,
    /// Kostenpflichtig (sonst Obergrenze 0: Modellliste, Kontostand).
    pub billable: bool,
    /// Ausgabegrenze (`max_tokens` bzw. `max_output_tokens`); 0 = ohne Ausgabe.
    pub max_output_tokens: u64,
    /// Teil von `--models-only` (nur kostenlose Listen- und Kontostandsabfragen).
    pub models_only: bool,
}

/// Unbekannte Modell-ID für die Fehlerform.
pub const UNKNOWN_MODEL: &str = "openspider-probe-no-such-model";
/// In 08 genannte Decisions-ID (AI-004); nur dort verwendet, wo ihre Existenz geprüft wird.
pub const SPEC_DECISIONS_MODEL: &str = "gpt-6-luna";
/// Platzhalter im Trockenlauf.
pub const MODEL_PLACEHOLDER: &str = "<aus-modellliste>";

pub const TEXT_MAX_TOKENS: u64 = 128;
pub const STRUCTURED_MAX_TOKENS: u64 = 256;

#[allow(clippy::too_many_arguments)]
const fn step(
    id: &'static str,
    provider: Provider,
    via: Via,
    method: &'static str,
    path: &'static str,
    purpose: &'static str,
    model: ModelNeed,
    max_output_tokens: u64,
) -> Step {
    let billable = !matches!(model, ModelNeed::None);
    Step {
        id,
        provider,
        via,
        method,
        path,
        purpose,
        model,
        billable,
        max_output_tokens,
        models_only: !billable,
    }
}

use ModelNeed as M;
use Provider::{Anthropic as A, OpenAi as O, OpenRouter as R};
use Via::{Adapter as AD, Raw as RW};

/// Der vollständige Plan in Ausführungsreihenfolge.
pub const STEPS: &[Step] = &[
    // Anthropic Messages (AI-002)
    step("anthropic.raw.models", A, RW, "GET", "/v1/models", "Modellliste: Feldnamen, capabilities-Knoten (structured_outputs, image_input, batch, effort), Paginierungsfelder (AI-002, AI-001)", M::None, 0),
    step("anthropic.adapter.models", A, AD, "GET", "/v1/models?limit=1000", "Adapter list_models: liest er die echten capabilities?", M::None, 0),
    step("anthropic.raw.count_tokens", A, RW, "POST", "/v1/messages/count_tokens", "Tokenzählung für Kostenschätzung (AI-002 optional, AI-012)", M::Text, 0),
    step("anthropic.raw.text", A, RW, "POST", "/v1/messages", "Textanfrage: system, messages, usage-Felder, stop_reason, Rate-Limit-Header (AI-002, AI-017)", M::Text, TEXT_MAX_TOKENS),
    step("anthropic.adapter.text", A, AD, "POST", "/v1/messages", "Adapter: Textanfrage", M::Text, TEXT_MAX_TOKENS),
    step("anthropic.raw.structured", A, RW, "POST", "/v1/messages", "Strukturierte Ausgabe output_config.format json_schema mit AI-034-Schema (AI-002, AI-021, AI-034)", M::Structured, STRUCTURED_MAX_TOKENS),
    step("anthropic.adapter.structured", A, AD, "POST", "/v1/messages", "Adapter: strukturierte Ausgabe inkl. lokaler Schema-Validierung", M::Structured, STRUCTURED_MAX_TOKENS),
    step("anthropic.raw.structured_limits", A, RW, "POST", "/v1/messages", "V-A5: Schema mit 20 Enum-Werten und 2.000 Zeichen Beschreibung", M::Structured, STRUCTURED_MAX_TOKENS),
    step("anthropic.raw.image", A, RW, "POST", "/v1/messages", "Bildanfrage: image-Block mit base64-Quelle, Testbild 32x32 PNG (AI-002)", M::Vision, TEXT_MAX_TOKENS),
    step("anthropic.adapter.image", A, AD, "POST", "/v1/messages", "Adapter: Bildanfrage", M::Vision, TEXT_MAX_TOKENS),
    step("anthropic.raw.unknown_model", A, RW, "POST", "/v1/messages", "Fehlerform bei unbekanntem Modell (Status, error.type, Meldung)", M::Unknown, TEXT_MAX_TOKENS),
    step("anthropic.adapter.unknown_model", A, AD, "POST", "/v1/messages", "Adapter: Fehlerklasse bei unbekanntem Modell", M::Unknown, TEXT_MAX_TOKENS),
    // OpenAI Responses (AI-003) und Decisions (AI-004)
    step("openai.raw.models", O, RW, "GET", "/models", "Modellliste: Feldnamen; steht gpt-6-luna darin? (AI-003, AI-004, K4-06)", M::None, 0),
    step("openai.adapter.models", O, AD, "GET", "/models", "Adapter list_models (Responses)", M::None, 0),
    step("openai.raw.responses_text", O, RW, "POST", "/responses", "Responses-Textanfrage: output[], usage, Rate-Limit-Header (AI-003, AI-017)", M::Text, TEXT_MAX_TOKENS),
    step("openai.adapter.responses_text", O, AD, "POST", "/responses", "Adapter: Responses-Textanfrage", M::Text, TEXT_MAX_TOKENS),
    step("openai.raw.responses_structured", O, RW, "POST", "/responses", "Strukturierte Ausgabe text.format json_schema strict mit AI-034-Schema (AI-003, AI-034)", M::Structured, STRUCTURED_MAX_TOKENS),
    step("openai.adapter.responses_structured", O, AD, "POST", "/responses", "Adapter: strukturierte Ausgabe", M::Structured, STRUCTURED_MAX_TOKENS),
    step("openai.raw.responses_structured_limits", O, RW, "POST", "/responses", "V-A5: Schema mit 20 Enum-Werten und 2.000 Zeichen Beschreibung", M::Structured, STRUCTURED_MAX_TOKENS),
    step("openai.raw.responses_image", O, RW, "POST", "/responses", "Bildanfrage: input_image als Data-URL (AI-003)", M::Vision, TEXT_MAX_TOKENS),
    step("openai.adapter.responses_image", O, AD, "POST", "/responses", "Adapter: Bildanfrage", M::Vision, TEXT_MAX_TOKENS),
    step("openai.raw.responses_unknown_model", O, RW, "POST", "/responses", "Fehlerform bei unbekanntem Modell", M::Unknown, TEXT_MAX_TOKENS),
    step("openai.adapter.responses_unknown_model", O, AD, "POST", "/responses", "Adapter: Fehlerklasse bei unbekanntem Modell", M::Unknown, TEXT_MAX_TOKENS),
    step("openai.raw.decisions_3q", O, RW, "POST", "/decisions", "Decisions-Beta mit gpt-6-luna: je eine predicate-, choice-, score-Frage; Antwortfelder, usage, confidence (AI-004, V-A1)", M::SpecLuna, 0),
    step("openai.adapter.decisions_3q", O, AD, "POST", "/decisions", "Adapter: dieselben drei Fragen", M::SpecLuna, 0),
    step("openai.raw.decisions_image", O, RW, "POST", "/decisions", "Decisions mit input_image (Data-URL) und einer predicate-Frage (AI-004)", M::SpecLuna, 0),
    step("openai.raw.decisions_11q", O, RW, "POST", "/decisions", "V-A1: 11 Fragen in einem Request (OS-Grenze 10)", M::SpecLuna, 0),
    step("openai.raw.decisions_long_instructions", O, RW, "POST", "/decisions", "V-A1: instructions mit 4.000 Zeichen (OS-Grenze 2.000)", M::SpecLuna, 0),
    step("openai.raw.decisions_unknown_model", O, RW, "POST", "/decisions", "Decisions-Fehlerform bei unbekanntem Modell", M::Unknown, 0),
    // OpenRouter (AI-005)
    step("openrouter.raw.models", R, RW, "GET", "/models", "Modellliste: Feldnamen, pricing (prompt/completion/request/image), supported_parameters, Modalitäten (AI-005, AI-010)", M::None, 0),
    step("openrouter.adapter.models", R, AD, "GET", "/models", "Adapter list_models: Preise und Fähigkeiten", M::None, 0),
    step("openrouter.raw.key", R, RW, "GET", "/key", "Kontostand: data.limit, limit_remaining, limit_reset, usage* (AI-005)", M::None, 0),
    step("openrouter.adapter.key", R, AD, "GET", "/key", "Adapter key_balance", M::None, 0),
    step("openrouter.raw.text", R, RW, "POST", "/chat/completions", "Textanfrage mit provider.data_collection=deny; usage, Header (AI-005)", M::Text, TEXT_MAX_TOKENS),
    step("openrouter.adapter.text", R, AD, "POST", "/chat/completions", "Adapter: Textanfrage", M::Text, TEXT_MAX_TOKENS),
    step("openrouter.raw.structured", R, RW, "POST", "/chat/completions", "response_format json_schema + provider.require_parameters (AI-005, AI-034)", M::Structured, STRUCTURED_MAX_TOKENS),
    step("openrouter.adapter.structured", R, AD, "POST", "/chat/completions", "Adapter: strukturierte Ausgabe", M::Structured, STRUCTURED_MAX_TOKENS),
    step("openrouter.raw.structured_limits", R, RW, "POST", "/chat/completions", "V-A5: Schema mit 20 Enum-Werten und 2.000 Zeichen Beschreibung", M::Structured, STRUCTURED_MAX_TOKENS),
    step("openrouter.raw.image", R, RW, "POST", "/chat/completions", "Bildanfrage: image_url mit Data-URL (AI-005)", M::Vision, TEXT_MAX_TOKENS),
    step("openrouter.adapter.image", R, AD, "POST", "/chat/completions", "Adapter: Bildanfrage", M::Vision, TEXT_MAX_TOKENS),
    step("openrouter.raw.unknown_model", R, RW, "POST", "/chat/completions", "Fehlerform bei unbekanntem Modell", M::Unknown, TEXT_MAX_TOKENS),
    step("openrouter.adapter.unknown_model", R, AD, "POST", "/chat/completions", "Adapter: Fehlerklasse bei unbekanntem Modell", M::Unknown, TEXT_MAX_TOKENS),
];

/// Schritte eines Anbieters in Reihenfolge.
pub fn steps(provider: Provider) -> impl Iterator<Item = &'static Step> {
    STEPS.iter().filter(move |s| s.provider == provider)
}

/// Schritt nach Kennung.
pub fn step_by_id(id: &str) -> &'static Step {
    STEPS
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("Schritt {id} fehlt im Plan"))
}

/// Preise für die Kostenschätzung in USD.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rates {
    pub input_per_token: f64,
    pub output_per_token: f64,
    pub per_request: f64,
    pub per_image: f64,
}

/// Vorsichtige Preisannahme für Anthropic und OpenAI (deren Modelllisten liefern keine Preise, AI-010): 30 USD je
/// 1 Mio. Eingabe-, 150 USD je 1 Mio. Ausgabe-Tokens. Bewusst weit über üblichen Preisen kleiner Modelle; die
/// Schätzung ist damit eine Obergrenze, keine Abrechnung.
pub const CONSERVATIVE: Rates = Rates {
    input_per_token: 30.0e-6,
    output_per_token: 150.0e-6,
    per_request: 0.0,
    per_image: 0.0,
};

/// Obergrenze für OpenRouter-Gebühren je Request und je Bild; Modelle darüber wählt die Probe nicht.
pub const OPENROUTER_FEE_CEILING: f64 = 0.01;

/// Obergrenze der Planung für OpenRouter: vorsichtige Tokenpreise plus Gebührenobergrenze.
pub const OPENROUTER_CEILING: Rates = Rates {
    input_per_token: CONSERVATIVE.input_per_token,
    output_per_token: CONSERVATIVE.output_per_token,
    per_request: OPENROUTER_FEE_CEILING,
    per_image: OPENROUTER_FEE_CEILING,
};

/// Angenommene Eingabe-Tokens je Bild für die Obergrenze (Testbild 32x32; großzügig).
pub const IMAGE_TOKENS_CEILING: u64 = 1_600;

/// Obergrenze der Eingabe-Tokens aus der Zeichenzahl der Nutzlast: ⌈Zeichen / 2⌉ + 64 (vorsichtig; echte Tokenizer
/// liegen bei etwa 3–4 Zeichen je Token).
pub fn input_tokens_ceiling(chars: u64, images: u32) -> u64 {
    chars.div_ceil(2) + 64 + u64::from(images) * IMAGE_TOKENS_CEILING
}

/// Kosten aus Tokenzahlen.
pub fn cost_usd(rates: &Rates, input_tokens: u64, output_tokens: u64, images: u32) -> f64 {
    input_tokens as f64 * rates.input_per_token
        + output_tokens as f64 * rates.output_per_token
        + rates.per_request
        + f64::from(images) * rates.per_image
}

/// Kostenobergrenze einer Anfrage: Eingabe-Obergrenze und volle Ausgabegrenze.
pub fn worst_case_usd(step: &Step, payload_chars: u64, images: u32, rates: &Rates) -> f64 {
    if !step.billable {
        return 0.0;
    }
    cost_usd(
        rates,
        input_tokens_ceiling(payload_chars, images),
        step.max_output_tokens,
        images,
    )
}

/// Planungs-Preise je Anbieter (Trockenlauf und Modelle ohne API-Preis).
pub fn planning_rates(provider: Provider) -> Rates {
    match provider {
        Provider::OpenRouter => OPENROUTER_CEILING,
        _ => CONSERVATIVE,
    }
}

/// Kostenbremse (Konstanten).
pub const PROVIDER_CAP_USD: f64 = 2.0;
pub const TOTAL_CAP_USD: f64 = 6.0;

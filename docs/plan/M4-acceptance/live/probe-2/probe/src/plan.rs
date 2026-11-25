//! Fester Anfrageplan der zweiten Probe M4-Q00b (V-A6 Reasoning-Parameter, V-A8 Speicherung; 08 „Vor Umsetzung zu
//! verifizieren“ und die Nachträge vom 24.11.2026 an AI-002, AI-003, AI-004, AI-005, AI-011). Der Trockenlauf druckt
//! genau diese Liste; der Live-Lauf arbeitet sie ab (jede Anfrage höchstens einmal, nur roh).

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
            Provider::Anthropic => 9,
            Provider::OpenAi => 9,
            Provider::OpenRouter => 6,
        }
    }

    /// Hauptmodell der Probe (Spec-IDs aus 08 bzw. dem Paket M4-Q00b; muss in der Modellliste stehen).
    pub fn primary_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-haiku-5-5",
            Provider::OpenAi => "gpt-6-luna",
            Provider::OpenRouter => "openai/gpt-6-luna",
        }
    }
}

/// Basis-URLs (keine EU-Endpunkte). Das Programm nutzt immer [`Endpoints::default`]; andere Werte setzen nur Tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// Ohne `/v1`.
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

/// Welches Modell eine Anfrage braucht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelNeed {
    /// Keins (Modellliste).
    None,
    /// [`Provider::primary_model`], vorher in der Modellliste bestätigt.
    Primary,
    /// Anthropic-Modell, dessen Thinking laut Liste nicht abschaltbar ist.
    NonDisableable,
}

/// Herkunft der geprüften Form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormSource {
    /// Modellliste, kein Reasoning-Parameter.
    List,
    /// Genau die Form, die der Adapter (`adapters/anthropic/reasoning.rs`, Zweig ai/k09) nach 08 sendet.
    Adapter,
    /// Naheliegende Alternative bzw. Grenzfall, den der Adapter nicht sendet.
    Alternative,
}

impl FormSource {
    pub fn label(self) -> &'static str {
        match self {
            FormSource::List => "Liste",
            FormSource::Adapter => "Adapter",
            FormSource::Alternative => "Alternative",
        }
    }
}

/// Eine geplante Anfrage (alle roh).
#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub id: &'static str,
    pub provider: Provider,
    pub source: FormSource,
    pub method: &'static str,
    /// Pfad relativ zur Basis-URL.
    pub path: &'static str,
    pub purpose: &'static str,
    pub model: ModelNeed,
    /// Ausgabegrenze; 0 = ohne Ausgabe (Decisions, Liste).
    pub max_output_tokens: u64,
}

impl Step {
    pub fn billable(&self) -> bool {
        !matches!(self.model, ModelNeed::None)
    }
}

/// Ausgabegrenze der Schema-Aufgabe: die Antwort hat rund 40 Tokens (Q00: 42 bei Anthropic, 40 bei OpenAI ohne
/// Reasoning); 200 lässt das Fünffache und auch bei `low` Raum für etwas Reasoning, und hält die Obergrenze unter der
/// Anbietergrenze.
pub const SCHEMA_MAX_TOKENS: u64 = 200;
/// Unbekannter Wert für `reasoning.effort` (Fehlerform).
pub const INVALID_EFFORT: &str = "off";

const fn step(
    id: &'static str,
    provider: Provider,
    source: FormSource,
    path: &'static str,
    purpose: &'static str,
    model: ModelNeed,
) -> Step {
    let (method, max_output_tokens) = match model {
        ModelNeed::None => ("GET", 0),
        _ => ("POST", SCHEMA_MAX_TOKENS),
    };
    Step {
        id,
        provider,
        source,
        method,
        path,
        purpose,
        model,
        max_output_tokens,
    }
}

const fn decisions(id: &'static str, source: FormSource, purpose: &'static str) -> Step {
    Step {
        id,
        provider: Provider::OpenAi,
        source,
        method: "POST",
        path: "/decisions",
        purpose,
        model: ModelNeed::Primary,
        max_output_tokens: 0,
    }
}

use FormSource::{Adapter as AD, Alternative as ALT, List as LI};
use ModelNeed as M;
use Provider::{Anthropic as A, OpenAi as O, OpenRouter as R};

/// Der vollständige Plan in Ausführungsreihenfolge.
pub const STEPS: &[Step] = &[
    // Anthropic (V-A6 a)
    step("anthropic.models", A, LI, "/v1/models", "Modellliste: claude-haiku-5-5 vorhanden, capabilities.thinking.types und effort je Modell; Modell ohne types.disabled finden", M::None),
    step("anthropic.default", A, AD, "/v1/messages", "Stufe model-default: weder thinking noch output_config.effort (Vergleichswert)", M::Primary),
    step("anthropic.off", A, AD, "/v1/messages", "Stufe off: thinking {type: disabled}; thinking_tokens = 0?", M::Primary),
    step("anthropic.low", A, AD, "/v1/messages", "Stufe low: thinking {type: adaptive} + output_config.effort = low", M::Primary),
    step("anthropic.adaptive_only", A, ALT, "/v1/messages", "thinking {type: adaptive} ohne effort", M::Primary),
    step("anthropic.effort_only", A, ALT, "/v1/messages", "output_config.effort = low ohne thinking (Form für Modelle ohne capabilities.thinking)", M::Primary),
    step("anthropic.off_with_effort", A, ALT, "/v1/messages", "Konfliktfall: thinking {type: disabled} + output_config.effort = low", M::Primary),
    step("anthropic.off_not_disableable", A, ALT, "/v1/messages", "thinking {type: disabled} an ein Modell ohne types.disabled (Fehlerform)", M::NonDisableable),
    // OpenAI Responses (V-A6 b, V-A8)
    step("openai.models", O, LI, "/models", "Modellliste: gpt-6-luna vorhanden", M::None),
    step("openai.default", O, AD, "/responses", "Stufe model-default: kein reasoning, store: false; Antwort spiegelt store und prompt_cache_retention?", M::Primary),
    step("openai.off", O, AD, "/responses", "Stufe off: reasoning {effort: none}, store: false; reasoning_tokens = 0?", M::Primary),
    step("openai.low", O, AD, "/responses", "Stufe low: reasoning {effort: low}, store: false", M::Primary),
    step("openai.minimal", O, ALT, "/responses", "reasoning {effort: minimal}: angenommen oder abgelehnt (Werte je Modell, reasoning_efforts)", M::Primary),
    step("openai.invalid_effort", O, ALT, "/responses", "reasoning {effort: off} (kein OpenAI-Wert): Fehlerform", M::Primary),
    decisions("openai.decisions_plain", AD, "Decisions ohne store (Adapterform AI-004): steht store in der Antwort?"),
    decisions("openai.decisions_store_false", ALT, "Decisions mit store: false (V-A8): angenommen, abgelehnt, gespiegelt?"),
    // OpenRouter (V-A6 c)
    step("openrouter.models", R, LI, "/models", "Modellliste: reasoning (supported_efforts, mandatory, default_enabled) für openai/gpt-6-luna; Preise", M::None),
    step("openrouter.default", R, AD, "/chat/completions", "Stufe model-default: kein reasoning; require_parameters (Schema-Aufgabe)", M::Primary),
    step("openrouter.off", R, AD, "/chat/completions", "Stufe off: reasoning {effort: none} mit provider.require_parameters = true; reasoning_tokens = 0?", M::Primary),
    step("openrouter.low", R, AD, "/chat/completions", "Stufe low: reasoning {effort: low}", M::Primary),
    step("openrouter.enabled_false", R, ALT, "/chat/completions", "Alternative: reasoning {enabled: false}", M::Primary),
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

/// Vorsichtige Preisannahme für Anthropic und OpenAI (Listen ohne Preise, AI-010): 30 USD je 1 Mio. Eingabe-,
/// 150 USD je 1 Mio. Ausgabe-Tokens – Obergrenze, keine Abrechnung (wie Probe 1).
pub const CONSERVATIVE: Rates = Rates {
    input_per_token: 30.0e-6,
    output_per_token: 150.0e-6,
    per_request: 0.0,
    per_image: 0.0,
};

/// Obergrenze für OpenRouter-Gebühren je Request und je Bild; Modelle darüber nimmt die Probe nicht.
pub const OPENROUTER_FEE_CEILING: f64 = 0.01;

/// Obergrenze der Planung für OpenRouter: vorsichtige Tokenpreise plus Gebührenobergrenze.
pub const OPENROUTER_CEILING: Rates = Rates {
    input_per_token: CONSERVATIVE.input_per_token,
    output_per_token: CONSERVATIVE.output_per_token,
    per_request: OPENROUTER_FEE_CEILING,
    per_image: OPENROUTER_FEE_CEILING,
};

/// Obergrenze der Eingabe-Tokens aus der Zeichenzahl der Nutzlast: ⌈Zeichen / 2⌉ + 64 (deckt auch den Aufschlag
/// für strukturierte Ausgabe bei Anthropic, Q00-N7: dort 399 Ist-Tokens gegenüber rund 500 als Obergrenze).
pub fn input_tokens_ceiling(chars: u64) -> u64 {
    chars.div_ceil(2) + 64
}

/// Kosten aus Tokenzahlen.
pub fn cost_usd(rates: &Rates, input_tokens: u64, output_tokens: u64) -> f64 {
    input_tokens as f64 * rates.input_per_token
        + output_tokens as f64 * rates.output_per_token
        + rates.per_request
}

/// Kostenobergrenze einer Anfrage: Eingabe-Obergrenze und volle Ausgabegrenze (Decisions ohne Ausgabe, mit dem
/// vorsichtigen Eingabepreis).
pub fn worst_case_usd(step: &Step, payload_chars: u64, rates: &Rates) -> f64 {
    if !step.billable() {
        return 0.0;
    }
    cost_usd(
        rates,
        input_tokens_ceiling(payload_chars),
        step.max_output_tokens,
    )
}

/// Planungs-Preise je Anbieter (Trockenlauf; Anthropic und OpenAI immer).
pub fn planning_rates(provider: Provider) -> Rates {
    match provider {
        Provider::OpenRouter => OPENROUTER_CEILING,
        _ => CONSERVATIVE,
    }
}

/// Kostenbremse (Konstanten, M4-Q00b): je Anbieter höchstens 0,40 USD (Paketeintrag; der Auftrag nennt höchstens
/// 0,50 USD – die engere Grenze gilt), gesamt höchstens 1 USD.
pub const PROVIDER_CAP_USD: f64 = 0.40;
pub const TOTAL_CAP_USD: f64 = 1.0;

//! Aufgabenvorlage der Vorschläge (AI-070): Antwortschema `{"candidates": [...]}`, Anweisungen und Rückmeldung der
//! Reparaturrunde (AI-075). Eigene Formulierungen (Clean-Room, E8); die Version geht in den Cache-Schlüssel ein.
use super::element::Element;
use super::limits::ElementLimits;
use super::validate::describe;
use os_store::ai::suggestions::StoredCandidate;
use serde_json::{json, Value};

/// Version der Vorlage (`definition_version`, AI-015/AI-014).
pub const PROMPT_VERSION: &str = "1.0.0";

/// Antwortschema (strikter Modus: alle Properties in `required`, `additionalProperties: false`, AI-021).
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "candidates": {
                "type": "array",
                "items": {"type": "string"}
            }
        },
        "required": ["candidates"],
        "additionalProperties": false
    })
}

/// Kontext eines Requests außerhalb der gesendeten Seitenfelder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptContext<'a> {
    pub candidates: u8,
    pub language: Option<&'a str>,
    pub focus: Option<&'a str>,
    pub search_intent: Option<&'a str>,
    pub style_lines: Vec<String>,
}

/// Anweisungen des ersten Requests.
pub fn instructions(element: Element, limits: &ElementLimits, ctx: &PromptContext<'_>) -> String {
    let n = ctx.candidates;
    let (what, goal) = match element {
        Element::Title => (
            "page titles (HTML <title>)",
            "Each title names the main topic of the page precisely and differs from the titles of the other pages of the site.",
        ),
        Element::MetaDescription => (
            "meta descriptions",
            "Each description summarises the content of the page truthfully and shows searchers what they gain from visiting it.",
        ),
    };
    let mut lines = vec![
        format!("Write {n} alternative {what} for the web page described in the input."),
        goal.to_string(),
        format!(
            "Each text has {} to {} characters and stays within {} pixels at {} px Arial-like font (at least {} pixels).",
            limits.min_chars,
            limits.max_chars,
            limits.max_px,
            element.font_px(),
            limits.min_px
        ),
    ];
    if let Some(lang) = ctx.language {
        lines.push(format!(
            "Write in the language of the page (language code {lang})."
        ));
    }
    if let Some(focus) = ctx.focus {
        lines.push(format!("Use the focus keyword \"{focus}\" naturally."));
    }
    if let Some(intent) = ctx.search_intent {
        lines.push(format!("Search intent of the page: {intent}."));
    }
    lines.extend(ctx.style_lines.iter().cloned());
    lines.push("Do not use placeholders, line breaks or surrounding quotation marks.".to_string());
    lines.push(format!(
        "Answer with JSON of the form {{\"candidates\": [...]}} containing exactly {n} strings, best first."
    ));
    lines.join("\n")
}

/// Rückmeldung für eine Reparaturrunde: die Kandidaten der Vorrunde mit ihren konkreten Verstößen (AI-075).
pub fn repair_feedback(previous: &[&StoredCandidate], n: u8) -> String {
    let mut lines = vec!["None of your previous candidates passed the local checks:".to_string()];
    for (i, c) in previous.iter().enumerate() {
        let problems = c
            .violations
            .iter()
            .map(|v| describe(v, c.length, c.pixel_width.unwrap_or(0)))
            .collect::<Vec<_>>()
            .join("; ");
        lines.push(format!("Candidate {} \"{}\": {problems}", i + 1, c.value));
    }
    lines.push(format!(
        "Write {n} new candidates that avoid these problems."
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-021 [M4]
    // Das Vorschlagsschema ist mit dem strikten Modus der Anbieter verträglich.
    #[test]
    fn schema_is_strict() {
        assert!(crate::schema::strict_violations(&schema()).is_empty());
        assert!(crate::schema::ResponseSchema::new(schema()).is_ok());
    }

    // spec: AI-070 [M4]
    #[test]
    fn instructions_carry_limits_language_and_focus_only_when_present() {
        let limits = ElementLimits {
            min_chars: 30,
            max_chars: 60,
            min_px: 200,
            max_px: 561,
        };
        let with = instructions(
            Element::Title,
            &limits,
            &PromptContext {
                candidates: 3,
                language: Some("de"),
                focus: Some("wanderschuhe damen"),
                ..PromptContext::default()
            },
        );
        assert!(with.contains("Write 3 alternative page titles"));
        assert!(with.contains("30 to 60 characters"));
        assert!(with.contains("561 pixels"));
        assert!(with.contains("\"wanderschuhe damen\""));
        assert!(with.contains("language code de"));
        let without = instructions(
            Element::MetaDescription,
            &limits,
            &PromptContext {
                candidates: 2,
                ..PromptContext::default()
            },
        );
        assert!(!without.contains("focus keyword"));
        assert!(without.contains("meta descriptions"));
    }
}

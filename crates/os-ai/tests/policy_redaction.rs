//! AI-024 Schwärzung personenbezogener Daten vor dem Senden – Bibliotheksebene (feste Muster, IBAN-Prüfziffer,
//! eigene Muster, Feldauswahl, Cache-Schlüssel). Die Prüfung gegen ai-mock steht in `policy_mock.rs`.
use os_ai::{
    cache::input_hash,
    policy::{
        fields::{FieldSet, FieldValue, FieldValues, InputField},
        guard::PreparedInput,
        redact::{
            iban_is_valid, CompiledPattern, NoRegexEngine, PatternEngine, PatternList,
            PatternSyntaxError, RedactionSettings, Redactor,
        },
    },
};
use os_model::config::AiAppSettings;
use serde_json::Value;
use std::{ops::Range, path::PathBuf};

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/ai/policy")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn values(json: &Value) -> FieldValues {
    let mut out = FieldValues::new();
    for (id, value) in json.as_object().unwrap() {
        let field = InputField::from_id(id).unwrap_or_else(|| panic!("unknown field {id}"));
        let value = match value {
            Value::String(s) => FieldValue::Text(s.clone()),
            other => serde_json::from_value(serde_json::json!({"kind": "image", "value": other}))
                .unwrap(),
        };
        out.insert(field, value);
    }
    out
}

// spec: AI-024
#[test]
fn fixed_patterns_match_the_fixture_cases() {
    let redactor = Redactor::default();
    let cases = fixture("patterns.json");
    for case in cases["cases"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let (out, _) = redactor.redact_text(text);
        assert_eq!(out, case["redact"].as_str().unwrap(), "input: {text}");
    }
}

// spec: AI-024
#[test]
fn pii_page_has_three_placeholders_and_three_redactions() {
    let pii = fixture("pii.json");
    let page = values(&pii["values"]);
    let fields = page.fields();
    let prepared = PreparedInput::new(&page, &fields, &Redactor::default());
    let text = prepared.values.text_block();
    for original in pii["originals"].as_array().unwrap() {
        assert!(
            !text.contains(original.as_str().unwrap()),
            "{original} in {text}"
        );
    }
    for placeholder in ["[EMAIL]", "[PHONE]", "[IBAN]"] {
        assert_eq!(
            text.matches(placeholder).count(),
            1,
            "{placeholder} in {text}"
        );
    }
    assert_eq!(prepared.redactions.total(), 3);
    assert_eq!(
        (
            prepared.redactions.email,
            prepared.redactions.phone,
            prepared.redactions.iban
        ),
        (1, 1, 1)
    );
}

// spec: AI-024
#[test]
fn iban_shape_with_wrong_check_digits_is_not_replaced() {
    let pii = fixture("pii.json");
    let text = pii["article_number_text"].as_str().unwrap();
    let (out, counts) = Redactor::default().redact_text(text);
    assert_eq!(out, text);
    assert_eq!(counts.total(), 0);
    let compact = pii["article_number"].as_str().unwrap().replace(' ', "");
    assert!(!iban_is_valid(&compact));
    assert!(iban_is_valid("DE89370400440532013000"));
    assert!(iban_is_valid("GB82WEST12345698765432"));
    assert!(
        !iban_is_valid("de89370400440532013000"),
        "lower case country code"
    );
    assert!(!iban_is_valid("DE8937040044"), "too short");
}

// spec: AI-024
#[test]
fn redaction_off_keeps_original_values() {
    let pii = fixture("pii.json");
    let page = values(&pii["values"]);
    let prepared = PreparedInput::new(
        &page,
        &page.fields(),
        &Redactor::new(RedactionSettings::off()),
    );
    let text = prepared.values.text_block();
    for original in pii["originals"].as_array().unwrap().iter().take(3) {
        assert!(text.contains(original.as_str().unwrap()));
    }
    assert_eq!(prepared.redactions.total(), 0);
}

// spec: AI-024
#[test]
fn redaction_is_on_by_default_and_follows_app_settings() {
    assert!(RedactionSettings::default().enabled);
    assert!(Redactor::default().is_enabled());
    let mut app = AiAppSettings::default();
    assert!(
        RedactionSettings::from_app_settings(&app, &NoRegexEngine)
            .unwrap()
            .enabled
    );
    app.redact_personal_data = false;
    assert!(
        !RedactionSettings::from_app_settings(&app, &NoRegexEngine)
            .unwrap()
            .enabled
    );
}

// spec: AI-024
#[test]
fn all_six_text_fields_are_redacted_other_fields_unchanged() {
    let mail = "max@example.com";
    let mut page = FieldValues::new();
    for field in InputField::ALL {
        if field != InputField::Image {
            page.insert(field, FieldValue::Text(format!("x {mail} y")));
        }
    }
    let counts = Redactor::default().redact_fields(&mut page);
    assert_eq!(counts.email, 6);
    for field in InputField::ALL {
        let Some(text) = page.text(field) else {
            continue;
        };
        let expected = if field.is_redacted() {
            "x [EMAIL] y"
        } else {
            "x max@example.com y"
        };
        assert_eq!(text, expected, "{}", field.id());
    }
    let redacted: Vec<_> = InputField::ALL
        .into_iter()
        .filter(|f| f.is_redacted())
        .map(InputField::id)
        .collect();
    assert_eq!(
        redacted,
        [
            "title",
            "meta_description",
            "h1",
            "h2",
            "page_text",
            "image_context"
        ]
    );
}

// spec: AI-024
#[test]
fn cache_key_input_is_the_redacted_text() {
    let fields = FieldSet::new([InputField::Url, InputField::PageText]);
    let page = |mail: &str| {
        FieldValues::new()
            .with_text(InputField::Url, "https://ai.test/contact")
            .with_text(
                InputField::PageText,
                format!("Schreibe an {mail}. MOCK-ID:contact"),
            )
    };
    let on = Redactor::default();
    let a = PreparedInput::new(&page("a@example.com"), &fields, &on);
    let b = PreparedInput::new(&page("b@example.com"), &fields, &on);
    assert_eq!(input_hash(&a.inputs()), input_hash(&b.inputs()));
    let off = Redactor::new(RedactionSettings::off());
    let a = PreparedInput::new(&page("a@example.com"), &fields, &off);
    let b = PreparedInput::new(&page("b@example.com"), &fields, &off);
    assert_ne!(input_hash(&a.inputs()), input_hash(&b.inputs()));
}

/// Test engine: literal patterns; an unbalanced `(` is a syntax error at its position.
struct LiteralEngine;
struct Literal(String);
impl CompiledPattern for Literal {
    fn find_all(&self, text: &str) -> Vec<Range<usize>> {
        text.match_indices(&self.0)
            .map(|(i, m)| i..i + m.len())
            .collect()
    }
}
impl PatternEngine for LiteralEngine {
    fn compile(&self, pattern: &str) -> Result<Box<dyn CompiledPattern>, PatternSyntaxError> {
        if let Some(pos) = pattern.chars().position(|c| c == '(') {
            if !pattern.contains(')') {
                return Err(PatternSyntaxError {
                    position: pos + 1,
                    message: "unclosed group".into(),
                });
            }
        }
        Ok(Box::new(Literal(pattern.to_string())))
    }
}

// spec: AI-024
#[test]
fn invalid_custom_pattern_is_rejected_with_line_and_position() {
    let err = PatternList::parse("KD-\n\nab(c", &LiteralEngine).unwrap_err();
    assert_eq!((err.line, err.position), (3, 3));
    assert_eq!(err.to_string(), "line 3, position 3: unclosed group");
    let app = AiAppSettings {
        redaction_patterns: vec!["ok".into(), "(x".into()],
        ..AiAppSettings::default()
    };
    let err = RedactionSettings::from_app_settings(&app, &LiteralEngine).unwrap_err();
    assert_eq!((err.line, err.position), (2, 1));
}

// spec: AI-024
#[test]
fn custom_patterns_are_applied_after_fixed_patterns() {
    let custom = PatternList::parse("KD-4711", &LiteralEngine).unwrap();
    let redactor = Redactor::new(RedactionSettings {
        enabled: true,
        custom,
    });
    let (out, counts) = redactor.redact_text("Kunde KD-4711, max@example.com");
    assert_eq!(out, "Kunde [REDACTED], [EMAIL]");
    assert_eq!((counts.custom, counts.email, counts.total()), (1, 1, 2));
}

// spec: AI-024
#[test]
fn without_regex_engine_custom_patterns_are_rejected_not_ignored() {
    let app = AiAppSettings {
        redaction_patterns: vec!["\\d+".into()],
        ..AiAppSettings::default()
    };
    let err = RedactionSettings::from_app_settings(&app, &NoRegexEngine).unwrap_err();
    assert_eq!(err.line, 1);
    assert!(PatternList::parse("\n  \n", &NoRegexEngine)
        .unwrap()
        .is_empty());
}

// spec: AI-024
#[test]
fn multibyte_text_around_matches_stays_intact() {
    let (out, counts) = Redactor::default()
        .redact_text("Grüße – schreibe an info@example.com, Tel. 030 1234567 – Straße ✓");
    assert_eq!(out, "Grüße – schreibe an [EMAIL], Tel. [PHONE] – Straße ✓");
    assert_eq!(counts.total(), 2);
}

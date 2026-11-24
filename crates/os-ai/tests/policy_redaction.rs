//! AI-024 Schwärzung personenbezogener Daten vor dem Senden – Bibliotheksebene (feste Muster, IBAN-Prüfziffer,
//! eigene Muster, Feldauswahl, Cache-Schlüssel). Seite `tests/fixtures/ai/site/pii.html`; die Prüfung gegen ai-mock
//! steht in `policy_mock.rs`.
#[path = "policy_html.rs"]
mod policy_html;
use os_ai::{
    cache::input_hash,
    policy::{
        fields::{FieldSet, FieldValue, FieldValues, InputField},
        guard::PreparedInput,
        redact::{iban_is_valid, PatternList, RedactionSettings, Redactor},
    },
};
use os_model::config::AiAppSettings;
use policy_html::{repo, site_page};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let path = repo(&format!("tests/fixtures/ai/policy/{name}"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

const PII_ORIGINALS: [&str; 4] = [
    "max@example.com",
    "+49 30 1234567",
    "DE89 3704 0044 0532 0130 00",
    "DE89370400440532013000",
];
const ARTICLE_NUMBER: &str = "DE12 3456 7890 1234 5678 90";

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
// Härtungen nach der Abnahme (Orchestrator-Festlegung 24.11.2026, Nachtrag zu AI-024): IBAN unabhängig von Groß-/
// Kleinschreibung mit Leerzeichen (auch mehrere, auch geschützte) und Bindestrichen als eine Ersetzung; falsche
// Prüfziffer bleibt ganz stehen; Telefon mit ` / `, `+` mit Punkten, führende `0 `; E-Mail mit Umlaut-Domain.
#[test]
fn hardened_iban_phone_and_email_cases() {
    let redactor = Redactor::default();
    let cases = fixture("patterns.json");
    let cases = cases["hardening_24_11"]["cases"].as_array().unwrap();
    assert!(cases.len() >= 15);
    for case in cases {
        let text = case["text"].as_str().unwrap();
        let (out, counts) = redactor.redact_text(text);
        assert_eq!(out, case["redact"].as_str().unwrap(), "input: {text:?}");
        let expected: Vec<u64> = case["counts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap())
            .collect();
        assert_eq!(
            vec![
                u64::from(counts.email),
                u64::from(counts.phone),
                u64::from(counts.iban)
            ],
            expected,
            "counts for {text:?}"
        );
    }
}

// spec: AI-024
#[test]
fn iban_variants_never_leak_a_part_as_phone() {
    let redactor = Redactor::default();
    let canonical = "DE77100100100123456789";
    for iban in [
        "DE77 1001 0010 0123 4567 89",
        "de77 1001 0010 0123 4567 89",
        "de77-1001-0010-0123-4567-89",
        "DE77  1001  0010  0123  4567  89",
        "DE77\u{a0}1001\u{a0}0010\u{a0}0123\u{a0}4567\u{a0}89",
        "de77100100100123456789",
    ] {
        let text = format!("Bank: {iban}, danke");
        let (out, counts) = redactor.redact_text(&text);
        assert_eq!(out, "Bank: [IBAN], danke", "{iban:?}");
        assert_eq!(
            (counts.iban, counts.phone, counts.total()),
            (1, 0, 1),
            "{iban:?}"
        );
        let digits: String = out.chars().filter(char::is_ascii_digit).collect();
        assert!(digits.is_empty(), "{iban:?}: {out}");
    }
    assert!(iban_is_valid(canonical));
}

// spec: AI-024
#[test]
fn pii_page_has_three_placeholders_and_three_redactions() {
    let page = site_page("pii.html");
    let fields = page.fields();
    let prepared = PreparedInput::new(&page, &fields, &Redactor::default());
    let text = prepared.values.text_block();
    for original in PII_ORIGINALS {
        assert!(!text.contains(original), "{original} in {text}");
    }
    assert!(text.contains(ARTICLE_NUMBER), "article number stays");
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
    let text = "Artikelnummer DE12 3456 7890 1234 5678 90 ist lieferbar.";
    let (out, counts) = Redactor::default().redact_text(text);
    assert_eq!(out, text);
    assert_eq!(counts.total(), 0);
    let compact = ARTICLE_NUMBER.replace(' ', "");
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
    let page = site_page("pii.html");
    let prepared = PreparedInput::new(
        &page,
        &page.fields(),
        &Redactor::new(RedactionSettings::off()),
    );
    let text = prepared.values.text_block();
    for original in PII_ORIGINALS.iter().take(3) {
        assert!(text.contains(original), "{original}");
    }
    assert_eq!(prepared.redactions.total(), 0);
}

// spec: AI-024
#[test]
fn redaction_is_on_by_default_and_follows_app_settings() {
    assert!(RedactionSettings::default().enabled);
    assert!(Redactor::default().is_enabled());
    let mut app = AiAppSettings::default();
    assert!(RedactionSettings::from_app_settings(&app).unwrap().enabled);
    app.redact_personal_data = false;
    assert!(!RedactionSettings::from_app_settings(&app).unwrap().enabled);
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

// spec: AI-024
#[test]
fn invalid_custom_regex_is_rejected_with_line_and_position() {
    // trailing backslash at the 4th character of line 3; the empty line 2 is ignored but counted
    let err = PatternList::parse("KD-\\d+\n\nabc\\").unwrap_err();
    assert_eq!((err.line, err.position), (3, 4), "{err}");
    assert!(err.to_string().starts_with("line 3, position 4: "), "{err}");
    // position counts characters, not bytes
    let err = PatternList::parse("äö\\").unwrap_err();
    assert_eq!((err.line, err.position), (1, 3), "{err}");
    // unclosed group: rejected with a position inside or right after the pattern
    let err = PatternList::parse("ok\nab(c").unwrap_err();
    assert_eq!(err.line, 2);
    assert!((1..=5).contains(&err.position), "{err}");
    assert!(!err.message.is_empty());
    // app settings: entry n is line n
    let app = AiAppSettings {
        redaction_patterns: vec!["KD-\\d+".into(), String::new(), "x\\".into()],
        ..AiAppSettings::default()
    };
    let err = RedactionSettings::from_app_settings(&app).unwrap_err();
    assert_eq!((err.line, err.position), (3, 2), "{err}");
}

// spec: AI-024
#[test]
fn custom_regex_patterns_run_after_fixed_patterns_and_count() {
    let custom =
        PatternList::parse("KD-\\d{4}\n\n(?i)kundennr\\.?\\s*\\d+\nexample\\.com").unwrap();
    assert_eq!(custom.sources().len(), 3, "empty line ignored");
    let redactor = Redactor::new(RedactionSettings {
        enabled: true,
        custom,
    });
    let (out, counts) = redactor.redact_text("Kunde KD-4711, KUNDENNR. 99, max@example.com");
    // the e-mail is replaced by the fixed pattern first, so `example\.com` finds nothing left
    assert_eq!(out, "Kunde [REDACTED], [REDACTED], [EMAIL]");
    assert_eq!((counts.custom, counts.email, counts.total()), (2, 1, 3));
}

// spec: AI-024
#[test]
fn custom_patterns_from_app_settings_count_in_request_redactions() {
    let app = AiAppSettings {
        redaction_patterns: vec!["Artikelnummer DE\\d\\d".into()],
        ..AiAppSettings::default()
    };
    let settings = RedactionSettings::from_app_settings(&app).unwrap();
    assert!(settings.enabled);
    let page = site_page("pii.html");
    let prepared = PreparedInput::new(&page, &page.fields(), &Redactor::new(settings));
    assert_eq!(prepared.redactions.total(), 4, "3 fixed + 1 custom");
    assert_eq!(prepared.redactions.custom, 1);
    assert!(prepared.values.text_block().contains("[REDACTED] 3456"));
    assert!(PatternList::parse("\n  \n").unwrap().is_empty());
}

// spec: AI-024
#[test]
fn multibyte_text_around_matches_stays_intact() {
    let (out, counts) = Redactor::default()
        .redact_text("Grüße – schreibe an info@example.com, Tel. 030 1234567 – Straße ✓");
    assert_eq!(out, "Grüße – schreibe an [EMAIL], Tel. [PHONE] – Straße ✓");
    assert_eq!(counts.total(), 2);
}

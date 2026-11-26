//! Unabhängige Abnahmetests M4-K07: Schwärzung personenbezogener Daten vor dem Senden (AI-024).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-024, AI-018 Spalte `Redaction`, AI-015 Cache-Schlüssel)
//! und den Festlegungen des Orchestrators: Treffer eigener Muster → `[REDACTED]`, zählen in `Redactions` mit; eine
//! Regex je Zeile, Leerzeilen werden ignoriert; `alt` wird nicht geschwärzt.
//!
//! IBANs der Testdaten sind erfunden und selbst nach ISO 13616 (mod 97) berechnet:
//! gültig `DE77 1001 0010 0123 4567 89`, `AT16 1234 5000 1234 5678`, `NL91ABNA0417164300`;
//! falsche Prüfziffer `DE78 1001 0010 0123 4567 89`, `XY12 3456 7890 1234 567`, `AB12 0301 2345 6789`.
// acceptance: independent

mod acceptance_k07_support;

use acceptance_k07_support::{extract_fields, read_fixture, Mock};
use os_ai::cache::{input_hash, CacheKey, CacheKeyInput, ExecutionPath};
use os_ai::policy::guard::{self, GuardedOutcome};
use os_ai::policy::{
    ConsentBook, ConsentGate, DataFlowOverview, FieldSet, FieldValues, InputField, PatternList,
    PreparedInput, ProviderPolicies, RedactionSettings, Redactor, TaskDataFlow,
};
use os_ai::provider::Outcome;
use os_ai::PreviewSwitch;
use os_model::config::AiAppSettings;
use serde_json::json;

use InputField::*;

const TEXT_FIELDS: [InputField; 6] = [PageText, H2, ImageContext, MetaDescription, Title, H1];

fn fields(list: &[InputField]) -> FieldSet {
    FieldSet::new(list.iter().copied())
}

fn default_redactor() -> Redactor {
    Redactor::new(RedactionSettings::from_app_settings(&AiAppSettings::default()).unwrap())
}

fn consenting_gate() -> ConsentGate {
    ConsentGate::cli(true)
}

async fn send(
    mock: &Mock,
    values: &FieldValues,
    set: &FieldSet,
    redactor: &Redactor,
) -> GuardedOutcome {
    let p = mock.provider("openai");
    let input = PreparedInput::new(values, set, redactor);
    let out = guard::execute(
        PreviewSwitch::ON,
        &consenting_gate(),
        "summarize",
        &input,
        &p,
        p.request("m1"),
    )
    .await;
    assert!(out.sent, "{:?}", out.outcome);
    assert!(
        matches!(out.outcome, Outcome::Completed(_)),
        "{:?}",
        out.outcome
    );
    out
}

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 1: `max@example.com`, `+49 30 1234567` und eine gültige IBAN → Request ohne diese Werte, mit den drei
// Platzhaltern; `Redactions = 3`.

// spec: AI-024
#[tokio::test]
async fn ai024_k1_drei_werte_werden_zu_drei_platzhaltern_redactions_3() {
    let mock = Mock::start().await;
    let values = FieldValues::new()
        .with_text(Url, "http://ai.test/kontakt")
        .with_text(
            PageText,
            "Kontakt: max@example.com, Telefon +49 30 1234567, IBAN DE77 1001 0010 0123 4567 89.",
        );
    let out = send(
        &mock,
        &values,
        &fields(&[Url, PageText]),
        &default_redactor(),
    )
    .await;
    assert_eq!(out.redactions, 3, "Redactions");

    let body = &mock.bodies()[0];
    for original in [
        "max@example.com",
        "+49 30 1234567",
        "1234567",
        "DE77 1001 0010 0123 4567 89",
        "DE77100100100123456789",
        "0123 4567",
    ] {
        assert!(!body.contains(original), "{original} im Request: {body}");
    }
    assert_eq!(count(body, "[EMAIL]"), 1, "{body}");
    assert_eq!(count(body, "[PHONE]"), 1, "{body}");
    assert_eq!(count(body, "[IBAN]"), 1, "{body}");
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 2: Fixture-Site, Standard-Einstellung: Requests für Kontakt und Impressum ohne die dort stehenden
// E-Mail-Adressen, Telefonnummern und IBANs (Volltextsuche im Mock-Log → 0 Treffer).

// spec: AI-024
#[tokio::test]
async fn ai024_k2_fixture_site_kontakt_und_impressum_volltextsuche_0_treffer() {
    let mock = Mock::start().await;
    let redactor = default_redactor();
    assert!(redactor.is_enabled(), "Standard-Einstellung: an");
    let all_text = fields(&[Url, Title, MetaDescription, H1, H2, PageText, ImageContext]);
    let mut pages = Vec::new();
    for (url, file) in [
        (
            "http://ai.test:8080/contact",
            "tests/fixtures/ai/site/contact.html",
        ),
        (
            "http://ai.test:8080/legal",
            "tests/fixtures/ai/site/legal.html",
        ),
    ] {
        let html = read_fixture(file);
        pages.push(html.clone());
        let values = extract_fields(url, &html);
        let out = send(&mock, &values, &all_text, &redactor).await;
        assert!(out.redactions > 0);
    }
    assert_eq!(mock.count(), 2, "je Seite ein Request");

    // Werte der beiden Seiten (Fixture-Plausibilität: sie stehen wirklich dort).
    let pii = [
        "kontakt@example.test",
        "+49 30 123456",
        "legal@example.test",
        "+49 30 654321",
        "DE89 3704 0044 0532 0130 00",
    ];
    for value in pii {
        assert!(
            pages.iter().any(|p| p.contains(value)),
            "{value} fehlt in der Fixture"
        );
    }
    let log = mock.full_log();
    let mut hits = Vec::new();
    for needle in pii.iter().copied().chain([
        "DE89370400440532013000",
        "3704 0044",
        "0532 0130",
        "30 123456",
        "+4930123456",
        "30 654321",
        "+4930654321",
        "@example.test",
    ]) {
        if log.contains(needle) {
            hits.push(needle);
        }
    }
    assert!(hits.is_empty(), "Treffer im Mock-Log: {hits:?}");
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 3: Zeichenfolge im IBAN-Format mit falscher Prüfziffer (z. B. Artikelnummer) wird nicht ersetzt.

// spec: AI-024
#[tokio::test]
async fn ai024_k3_iban_format_mit_falscher_pruefziffer_wird_nicht_ersetzt() {
    let redactor = default_redactor();
    for text in [
        "DE78 1001 0010 0123 4567 89",
        "DE78100100100123456789",
        "Artikelnummer XY12 3456 7890 1234 567 im Lager",
        "Artikel AB12 0301 2345 6789",
        "Bestellnummer AB12030123456789.",
    ] {
        let (out, counts) = redactor.redact_text(text);
        assert_eq!(out, text, "nichts ersetzen");
        assert_eq!(counts.total(), 0, "{text}");
    }
    // Ende zu Ende: unverändert im Request, Redactions = 0.
    let mock = Mock::start().await;
    let values = FieldValues::new().with_text(
        PageText,
        "Artikelnummer XY12 3456 7890 1234 567, Lagerplatz DE78 1001 0010 0123 4567 89.",
    );
    let out = send(&mock, &values, &fields(&[PageText]), &redactor).await;
    assert_eq!(out.redactions, 0);
    let body = &mock.bodies()[0];
    assert!(body.contains("XY12 3456 7890 1234 567"), "{body}");
    assert!(body.contains("DE78 1001 0010 0123 4567 89"), "{body}");
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 4: „schwärzen" = aus → Request mit Originalwerten; Dialog zeigt `Redaction = off` mit Hinweis.

// spec: AI-024
#[tokio::test]
async fn ai024_k4_schwaerzen_aus_sendet_originale_und_dialog_zeigt_off_mit_hinweis() {
    let settings = AiAppSettings {
        redact_personal_data: false,
        ..AiAppSettings::default()
    };
    let redactor = Redactor::new(RedactionSettings::from_app_settings(&settings).unwrap());
    assert!(!redactor.is_enabled());

    let mock = Mock::start().await;
    let text =
        "Kontakt: max@example.com, Telefon +49 30 1234567, IBAN DE77 1001 0010 0123 4567 89.";
    let values = FieldValues::new().with_text(PageText, text);
    let set = fields(&[Url, PageText]);
    let out = send(&mock, &values, &set, &redactor).await;
    assert_eq!(out.redactions, 0);
    let body = &mock.bodies()[0];
    for original in [
        "max@example.com",
        "+49 30 1234567",
        "DE77 1001 0010 0123 4567 89",
    ] {
        assert!(body.contains(original), "{original} fehlt: {body}");
    }
    for placeholder in ["[EMAIL]", "[PHONE]", "[IBAN]"] {
        assert!(!body.contains(placeholder), "{body}");
    }

    let flows = [TaskDataFlow {
        task: "summarize".into(),
        provider: "openai".into(),
        model: "m1".into(),
        endpoint: "https://api.openai.com/v1".into(),
        region: None,
        fields: set,
        max_chars: None,
        api: None,
    }];
    let gate = ConsentGate::dialog(ConsentBook::new());
    let asks = DataFlowOverview::consent_requests(
        &flows,
        &ProviderPolicies::embedded(),
        &gate,
        redactor.is_enabled(),
    );
    assert_eq!(asks[0].row.redaction_cell(), "off");
    assert!(asks[0].pii_notice, "Hinweis bei Schwärzung aus");
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 5: Eine ungültige eigene Regex wird beim Speichern abgelehnt (Zeile und Position).

// spec: AI-024
#[test]
fn ai024_k5_ungueltige_eigene_regex_wird_mit_zeile_und_position_abgelehnt() {
    // Zeile 3 ist ungültig (offene Gruppe); Zeile 2 ist leer.
    let list = "Kundennummer \\d{6}\n\nfoo(bar\nKD-[0-9]{4}";
    let err = PatternList::parse(list).expect_err("ungültige Regex muss abgelehnt werden");
    assert_eq!(err.line, 3, "{err}");
    let len = "foo(bar".chars().count();
    assert!(
        (1..=len + 1).contains(&err.position),
        "Position {} außerhalb der Zeile",
        err.position
    );

    // Speichern über die App-Einstellungen: Eintrag n = Zeile n.
    let settings = AiAppSettings {
        redaction_patterns: vec!["ok\\d+".into(), "[unclosed".into()],
        ..AiAppSettings::default()
    };
    let err = RedactionSettings::from_app_settings(&settings)
        .expect_err("ungültige Regex beim Speichern ablehnen");
    assert_eq!(err.line, 2, "{err}");
    assert!((1..="[unclosed".len() + 1).contains(&err.position), "{err}");

    // Gültige Listen werden angenommen.
    assert!(PatternList::parse("Kundennummer \\d{6}\n\nKD-[0-9]{4}\n").is_ok());
}

// ---------------------------------------------------------------------------------------------------------------
// Beschreibung: Standard an; alle sechs Textfelder; Cache-Schlüssel über den geschwärzten Text; eigene Muster.

// spec: AI-024
#[test]
fn ai024_schwaerzung_ist_standardmaessig_an() {
    assert!(AiAppSettings::default().redact_personal_data);
    assert!(RedactionSettings::default().enabled);
    assert!(Redactor::default().is_enabled());
    assert!(default_redactor().is_enabled());
}

// spec: AI-024
// „in allen Textfeldern (`page_text`, `h2`, `image_context`, `meta_description`, `title`, `h1`)": jedes Feld einzeln,
// jede der drei Arten.
#[test]
fn ai024_alle_sechs_textfelder_werden_geschwaerzt() {
    let redactor = default_redactor();
    for field in TEXT_FIELDS {
        for (value, placeholder) in [
            ("max@example.com", "[EMAIL]"),
            ("+49 30 1234567", "[PHONE]"),
            ("DE77 1001 0010 0123 4567 89", "[IBAN]"),
        ] {
            let values = FieldValues::new().with_text(field, format!("Hinweis {value} Ende"));
            let p = PreparedInput::new(&values, &fields(&[field]), &redactor);
            let sent = p.values.text(field).unwrap();
            assert_eq!(sent, format!("Hinweis {placeholder} Ende"), "{field:?}");
            assert_eq!(p.redactions.total(), 1, "{field:?} {value}");
            let block: String = p
                .inputs()
                .iter()
                .map(|i| serde_json::to_string(i).unwrap())
                .collect();
            assert!(!block.contains(value), "{field:?}: {block}");
        }
    }
}

// spec: AI-024
#[tokio::test]
async fn ai024_alle_sechs_textfelder_ende_zu_ende_auf_eigener_kontaktseite() {
    let mock = Mock::start().await;
    let html = read_fixture("tests/fixtures/ai/k07-acceptance/contact-data.html");
    let values = extract_fields("http://ai.test/kontakt", &html);
    for f in TEXT_FIELDS {
        assert!(values.text(f).is_some(), "Fixture ohne {f:?}");
    }
    let set = fields(&[Url, Title, MetaDescription, H1, H2, PageText, ImageContext]);
    let out = send(&mock, &values, &set, &default_redactor()).await;
    let body = &mock.bodies()[0];
    for value in [
        // title, meta_description, h1, h2, page_text, image_context
        "info@bergsport-probe.example",
        "0171 2345678",
        "service@bergsport-probe.example",
        "+43 1 2345678",
        "buchhaltung@bergsport-probe.example",
        "AT16 1234 5000 1234 5678",
        "max@example.com",
        "+49 30 1234567",
        "DE77 1001 0010 0123 4567 89",
        "anna.berg@bergsport-probe.example",
        "jonas@bergsport-probe.example",
        "030/7654321",
        "NL91ABNA0417164300",
        "+41 44 123 45 67",
        "030 9876543",
        "presse@bergsport-probe.example",
    ] {
        assert!(html.contains(value), "Fixture: {value}");
        assert!(!body.contains(value), "{value} im Request");
    }
    // Ziffernreste der Nummern dürfen nicht übrig bleiben.
    for rest in [
        "2345678",
        "1234567",
        "7654321",
        "9876543",
        "123 45 67",
        "0417164300",
    ] {
        assert!(!body.contains(rest), "Rest {rest} im Request: {body}");
    }
    // Artikelnummer und falsche IBAN bleiben stehen.
    assert!(body.contains("XY12 3456 7890 1234 567"), "{body}");
    assert!(body.contains("DE78 1001 0010 0123 4567 89"), "{body}");
    // 7 E-Mail (ohne alt) + 6 Telefon + 3 IBAN = 16 Ersetzungen; `alt` ist nicht im Feldsatz.
    assert_eq!(count(body, "[EMAIL]"), 7, "{body}");
    assert_eq!(count(body, "[PHONE]"), 6, "{body}");
    assert_eq!(count(body, "[IBAN]"), 3, "{body}");
    assert_eq!(out.redactions, 16);
}

// spec: AI-024
// Mehrere Treffer im selben Feld werden alle ersetzt und einzeln gezählt.
#[test]
fn ai024_mehrere_treffer_je_feld() {
    let redactor = default_redactor();
    let text = "a@example.com, b@example.org; c.d@example.net | +49 30 1234567 und 030 7654321 | \
                DE77 1001 0010 0123 4567 89 sowie AT161234500012345678";
    for field in TEXT_FIELDS {
        let values = FieldValues::new().with_text(field, text);
        let p = PreparedInput::new(&values, &fields(&[field]), &redactor);
        let sent = p.values.text(field).unwrap();
        assert_eq!(count(sent, "[EMAIL]"), 3, "{field:?}: {sent}");
        assert_eq!(count(sent, "[PHONE]"), 2, "{field:?}: {sent}");
        assert_eq!(count(sent, "[IBAN]"), 2, "{field:?}: {sent}");
        assert_eq!(p.redactions.email, 3);
        assert_eq!(p.redactions.phone, 2);
        assert_eq!(p.redactions.iban, 2);
        assert_eq!(p.redactions.total(), 7);
    }
    // Über mehrere Felder eines Requests wird summiert.
    let mut values = FieldValues::new();
    for field in TEXT_FIELDS {
        values = values.with_text(field, text);
    }
    let p = PreparedInput::new(&values, &FieldSet::new(TEXT_FIELDS), &redactor);
    assert_eq!(p.redactions.total(), 42);
}

// spec: AI-024
// Übliche Schreibweisen von Telefonnummern (national, international, mit Leerzeichen, Bindestrich, Schrägstrich,
// Klammern): keine Ziffernfolge der Nummer bleibt im gesendeten Text.
#[test]
fn ai024_telefonnummern_in_ueblichen_schreibweisen() {
    let redactor = default_redactor();
    for number in [
        "+49 30 1234567",
        "+49 (0)30 1234567",
        "+49-30-1234567",
        "+4930 1234567",
        "+49301234567",
        "0049 30 1234567",
        "030 1234567",
        "030/1234567",
        "030-1234567",
        "(030) 1234567",
        "+49 30 123 45 67",
        "0171 2345678",
        "+43 1 2345678",
        "+41 44 123 45 67",
    ] {
        let text = format!("Rufen Sie an: {number}. Danke");
        let (out, counts) = redactor.redact_text(&text);
        assert!(out.contains("[PHONE]"), "{number}: {out}");
        assert_eq!(counts.phone, 1, "{number}: {out}");
        let digits: String = number.chars().filter(char::is_ascii_digit).collect();
        let tail = &digits[digits.len() - 4..];
        assert!(!out.contains(tail), "{number}: Rest {tail} in {out}");
        assert!(
            out.starts_with("Rufen Sie an: ") && out.ends_with(". Danke"),
            "{out}"
        );
    }
}

// spec: AI-024
// IBANs mit Leerzeichen in Vierergruppen und ohne Leerzeichen, mit Buchstaben im BBAN, am Satzende und in Klammern.
#[test]
fn ai024_ibans_mit_und_ohne_leerzeichen() {
    let redactor = default_redactor();
    for (iban, context) in [
        ("DE77 1001 0010 0123 4567 89", "IBAN: {}."),
        ("DE77100100100123456789", "IBAN: {}."),
        ("AT16 1234 5000 1234 5678", "Konto {} bei der Bank"),
        ("AT161234500012345678", "({})"),
        ("NL91 ABNA 0417 1643 00", "Konto {}, danke"),
        ("NL91ABNA0417164300", "Konto {}"),
    ] {
        let text = context.replace("{}", iban);
        let (out, counts) = redactor.redact_text(&text);
        assert_eq!(out, context.replace("{}", "[IBAN]"), "{iban}");
        assert_eq!(counts.iban, 1);
        assert_eq!(counts.total(), 1, "{iban}: {counts:?}");
    }
}

// spec: AI-024
// „Der Cache-Schlüssel basiert auf dem geschwärzten Text": zwei Seiten, die sich nur in Kontaktdaten unterscheiden,
// ergeben denselben Eingabe-Hash und denselben Cache-Schlüssel (AI-015); ungeschwärzt unterscheiden sie sich.
#[test]
fn ai024_cache_schluessel_basiert_auf_geschwaerztem_text() {
    let a = FieldValues::new().with_text(Title, "Kontakt").with_text(
        PageText,
        "Mail max@example.com, Tel. +49 30 1234567, IBAN DE77 1001 0010 0123 4567 89",
    );
    let b = FieldValues::new().with_text(Title, "Kontakt").with_text(
        PageText,
        "Mail erika@example.org, Tel. 040 7654321, IBAN AT16 1234 5000 1234 5678",
    );
    let set = fields(&[Title, PageText]);
    let on = default_redactor();
    let off = Redactor::new(RedactionSettings::off());

    let pa = PreparedInput::new(&a, &set, &on);
    let pb = PreparedInput::new(&b, &set, &on);
    assert_eq!(input_hash(&pa.inputs()), input_hash(&pb.inputs()));
    let key = |p: &PreparedInput| {
        CacheKey::new(CacheKeyInput {
            task: "summarize".into(),
            definition_id: "summarize".into(),
            definition_version: "1".into(),
            params: json!({}),
            path: Some(ExecutionPath::StructuredOutput),
            provider: "openai".into(),
            model: "m1".into(),
            endpoint: "https://api.openai.com/v1".into(),
            result_params: json!({}),
            input_hash: input_hash(&p.inputs()),
        })
    };
    assert_eq!(key(&pa).as_str(), key(&pb).as_str());

    let ua = PreparedInput::new(&a, &set, &off);
    let ub = PreparedInput::new(&b, &set, &off);
    assert_ne!(input_hash(&ua.inputs()), input_hash(&ub.inputs()));
    assert_ne!(input_hash(&ua.inputs()), input_hash(&pa.inputs()));
}

// spec: AI-024
// Eigene Muster (Festlegung: Ersetzung durch `[REDACTED]`, zählen in `Redactions`; Leerzeilen ignoriert).
#[tokio::test]
async fn ai024_eigene_muster_werden_ersetzt_und_gezaehlt() {
    let settings = AiAppSettings {
        redaction_patterns: vec![
            "Kundennummer \\d{6}".into(),
            String::new(),
            "KD-[0-9]{4}".into(),
        ],
        ..AiAppSettings::default()
    };
    let redactor = Redactor::new(RedactionSettings::from_app_settings(&settings).unwrap());
    let text = "Kundennummer 123456, Vorgang KD-0815 und KD-4711, Mail max@example.com";
    let (out, counts) = redactor.redact_text(text);
    assert_eq!(
        out,
        "[REDACTED], Vorgang [REDACTED] und [REDACTED], Mail [EMAIL]"
    );
    assert_eq!(counts.custom, 3);
    assert_eq!(counts.total(), 4);

    let list = PatternList::parse("Kundennummer \\d{6}\n\n   \nKD-[0-9]{4}\n").unwrap();
    assert_eq!(list.sources().len(), 2, "Leerzeilen werden ignoriert");

    let mock = Mock::start().await;
    let values = FieldValues::new().with_text(PageText, text);
    let out = send(&mock, &values, &fields(&[PageText]), &redactor).await;
    assert_eq!(out.redactions, 4);
    let body = &mock.bodies()[0];
    assert!(!body.contains("123456") && !body.contains("KD-0815") && !body.contains("KD-4711"));
    assert_eq!(count(body, "[REDACTED]"), 3);
}

// spec: AI-024
// Festlegung: `alt` gehört nicht zu den sechs Textfeldern und wird nicht geschwärzt.
#[test]
fn ai024_alt_wird_nicht_geschwaerzt() {
    let redactor = default_redactor();
    let alt = "Team-Foto, Kontakt team@example.com, +49 30 1234567";
    let values = FieldValues::new().with_text(Alt, alt);
    let p = PreparedInput::new(&values, &fields(&[Alt]), &redactor);
    assert_eq!(p.values.text(Alt), Some(alt));
    assert_eq!(p.redactions.total(), 0);
}

// spec: AI-024
// Mit „schwärzen" = aus werden auch eigene Muster nicht angewandt und nichts gezählt.
#[test]
fn ai024_schwaerzen_aus_laesst_alle_felder_unveraendert() {
    let settings = AiAppSettings {
        redact_personal_data: false,
        redaction_patterns: vec!["Kundennummer \\d{6}".into()],
        ..AiAppSettings::default()
    };
    let redactor = Redactor::new(RedactionSettings::from_app_settings(&settings).unwrap());
    let text = "Kundennummer 123456, max@example.com, +49 30 1234567, DE77 1001 0010 0123 4567 89";
    let mut values = FieldValues::new();
    for f in TEXT_FIELDS {
        values = values.with_text(f, text);
    }
    let p = PreparedInput::new(&values, &FieldSet::new(TEXT_FIELDS), &redactor);
    for f in TEXT_FIELDS {
        assert_eq!(p.values.text(f), Some(text), "{f:?}");
    }
    assert_eq!(p.redactions.total(), 0);
}

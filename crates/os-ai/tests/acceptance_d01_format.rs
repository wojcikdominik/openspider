//! Unabhängige Abnahme M4-D01 – Fragenformat (AI-030): Pflichtfelder, Grenzen, Vorlagen, Schema-Datei, Rundreise.
//! Erwartungen nur aus `docs/spec/08-ai.md` (AI-030, AI-012 Nachtrag „Fragentext“).
// acceptance: independent
mod acceptance_d01_support;

use acceptance_d01_support as s;
use os_ai::decisions::format::{
    self, instantiate, parse, parse_value, schema_errors, validate_active, wire_name, Candidate,
    FieldError, ParamValue, ParamValues, QuestionDef, SCHEMA_JSON,
};
use os_ai::decisions::library;
use serde_json::{json, Value};

fn base_choice() -> Value {
    json!({
        "id": "page-kind",
        "version": "1.0.0",
        "name": "Page Kind",
        "type": "choice",
        "instructions": "Which kind of page is this? Use only observable criteria.",
        "choices": [
            {"value": "product", "description": "A single product."},
            {"value": "category", "description": "A list of products."},
            {"value": "other", "description": "Anything else."}
        ],
        "subject": "page",
        "inputs": ["url", "title", {"field": "page_text", "max_chars": 4000}],
        "scope": {"status_codes": [200], "indexable_only": true},
        "thresholds": {"low_confidence_below": 0.6}
    })
}

fn base_predicate() -> Value {
    json!({
        "id": "soft-404",
        "version": "1.0.0",
        "name": "Soft 404",
        "type": "predicate",
        "instructions": "The page says the requested content does not exist.",
        "subject": "page",
        "inputs": ["title", "page_text"],
        "thresholds": {"true_at": 0.5}
    })
}

fn base_score() -> Value {
    json!({
        "id": "quality",
        "version": "1.0.0",
        "name": "Quality",
        "type": "score",
        "instructions": "Rate the helpfulness of the main content.",
        "levels": [
            {"label": "poor", "description": "Thin."},
            {"label": "fair", "description": "Some value."},
            {"label": "good", "description": "Helpful."}
        ],
        "subject": "page",
        "inputs": ["page_text"]
    })
}

fn errors_of(value: &Value) -> Vec<FieldError> {
    match parse_value(value) {
        Ok(q) => panic!("ungültige Frage angenommen: {value}\n→ {q:?}"),
        Err(errors) => errors,
    }
}

/// Der Speicherfehler nennt das Feld (AI-030: „Speicherfehler mit Feldangabe“).
fn assert_rejected_at(value: &Value, field: &str) {
    let errors = errors_of(value);
    assert!(
        errors
            .iter()
            .any(|e| e.field.contains(field) || e.message.contains(field)),
        "Fehler nennt das Feld {field} nicht: {errors:?}"
    );
}

fn with(mut v: Value, key: &str, new: Value) -> Value {
    v[key] = new;
    v
}

fn without(mut v: Value, key: &str) -> Value {
    v.as_object_mut().unwrap().remove(key);
    v
}

fn choices(n: usize) -> Value {
    Value::Array(
        (0..n)
            .map(|i| json!({"value": format!("v{i}"), "description": format!("Value {i}.")}))
            .collect(),
    )
}

fn levels(n: usize) -> Value {
    Value::Array(
        (0..n)
            .map(|i| json!({"label": format!("L{i}"), "description": format!("Level {i}.")}))
            .collect(),
    )
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_valid_questions_of_each_type_are_accepted() {
    for value in [base_choice(), base_predicate(), base_score()] {
        let q = parse_value(&value).unwrap_or_else(|e| panic!("gültige Frage abgelehnt: {e:?}"));
        assert_eq!(q.id, value["id"].as_str().unwrap());
        assert_eq!(q.version.to_string(), "1.0.0");
        assert_eq!(q.name, value["name"].as_str().unwrap());
        // Aus Text wie aus JSON-Wert dasselbe Ergebnis.
        assert_eq!(parse(&value.to_string()).unwrap(), q);
    }
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_required_fields_are_enforced_with_field_name() {
    for key in [
        "id",
        "version",
        "name",
        "type",
        "instructions",
        "subject",
        "inputs",
    ] {
        let errors = errors_of(&without(base_choice(), key));
        assert!(
            errors
                .iter()
                .any(|e| e.field.contains(key) || e.message.contains(key)),
            "fehlendes Pflichtfeld {key} wird nicht benannt: {errors:?}"
        );
    }
    // `choice` ohne `choices`, `score` ohne `levels`.
    assert_rejected_at(&without(base_choice(), "choices"), "choices");
    assert_rejected_at(&without(base_score(), "levels"), "levels");
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_id_pattern_boundaries() {
    let ok_48 = format!("a{}", "b".repeat(47));
    for id in ["a", "a1", "page-type", "a-b-c-9", ok_48.as_str()] {
        assert!(
            parse_value(&with(base_choice(), "id", json!(id))).is_ok(),
            "gültige id {id:?} abgelehnt"
        );
    }
    let too_long = format!("a{}", "b".repeat(48));
    for id in [
        "",
        "A",
        "Page",
        "9a",
        "-a",
        "a_b",
        "a b",
        "ä",
        too_long.as_str(),
    ] {
        assert_rejected_at(&with(base_choice(), "id", json!(id)), "id");
    }
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_name_length_counts_characters() {
    let sixty_umlauts = "ä".repeat(60);
    let sixty = "n".repeat(60);
    for name in ["N", sixty_umlauts.as_str(), sixty.as_str()] {
        assert!(
            parse_value(&with(base_choice(), "name", json!(name))).is_ok(),
            "Name mit {} Zeichen abgelehnt",
            name.chars().count()
        );
    }
    assert_rejected_at(&with(base_choice(), "name", json!("")), "name");
    assert_rejected_at(&with(base_choice(), "name", json!("n".repeat(61))), "name");
    assert_rejected_at(&with(base_choice(), "name", json!("ä".repeat(61))), "name");
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_name_unique_among_active_questions() {
    let a = parse_value(&base_choice()).unwrap();
    let mut b = parse_value(&base_predicate()).unwrap();
    assert!(validate_active(&[a.clone(), b.clone()]).is_ok());
    b.name = a.name.clone();
    let errors = validate_active(&[a, b]).expect_err("doppelter Name unter aktiven Fragen");
    assert!(
        errors.iter().any(|e| e.field.contains("name")),
        "Fehler nennt das Feld name nicht: {errors:?}"
    );
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_choice_value_count_two_to_twenty() {
    for n in [2, 20] {
        assert!(
            parse_value(&with(base_choice(), "choices", choices(n))).is_ok(),
            "{n} Werte abgelehnt"
        );
    }
    for n in [0, 1, 21] {
        assert_rejected_at(&with(base_choice(), "choices", choices(n)), "choices");
    }
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_choice_values_unique_and_patterned() {
    let dup = json!([
        {"value": "a", "description": "x"},
        {"value": "b", "description": "y"},
        {"value": "a", "description": "z"}
    ]);
    assert_rejected_at(&with(base_choice(), "choices", dup), "choices");
    let ok32 = "v".repeat(32);
    for value in ["a", "a_b-1", "0", ok32.as_str()] {
        let list =
            json!([{"value": value, "description": "x"}, {"value": "zz", "description": "y"}]);
        assert!(
            parse_value(&with(base_choice(), "choices", list)).is_ok(),
            "Wert {value:?} abgelehnt"
        );
    }
    let too_long = "v".repeat(33);
    for value in ["", "Product", "a b", "ä", "a.b", too_long.as_str()] {
        let list =
            json!([{"value": value, "description": "x"}, {"value": "zz", "description": "y"}]);
        assert_rejected_at(&with(base_choice(), "choices", list), "choices");
    }
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_score_levels_two_to_ten_unique_labels() {
    for n in [2, 10] {
        assert!(
            parse_value(&with(base_score(), "levels", levels(n))).is_ok(),
            "{n} Levels abgelehnt"
        );
    }
    for n in [1, 11] {
        assert_rejected_at(&with(base_score(), "levels", levels(n)), "levels");
    }
    let dup = json!([
        {"label": "same", "description": "x"},
        {"label": "other", "description": "y"},
        {"label": "same", "description": "z"}
    ]);
    assert_rejected_at(&with(base_score(), "levels", dup), "levels");
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_instructions_one_to_2000_characters() {
    for text in ["x".to_string(), "x".repeat(2000), "ä".repeat(2000)] {
        assert!(
            parse_value(&with(base_choice(), "instructions", json!(text))).is_ok(),
            "instructions mit {} Zeichen abgelehnt",
            text.chars().count()
        );
    }
    for text in [String::new(), "x".repeat(2001), "ä".repeat(2001)] {
        assert_rejected_at(
            &with(base_choice(), "instructions", json!(text)),
            "instructions",
        );
    }
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_predicate_has_neither_choices_nor_levels() {
    let with_choices = with(base_predicate(), "choices", choices(2));
    assert_rejected_at(&with_choices, "choices");
    let with_levels = with(base_predicate(), "levels", levels(2));
    assert_rejected_at(&with_levels, "levels");
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_type_subject_and_version_are_closed_sets() {
    assert_rejected_at(&with(base_choice(), "type", json!("multi")), "type");
    assert_rejected_at(&with(base_choice(), "subject", json!("site")), "subject");
    for version in ["1", "1.0", "1.0.0.0", "v1.0.0", "01.0.0", ""] {
        assert_rejected_at(&with(base_choice(), "version", json!(version)), "version");
    }
    assert!(parse_value(&with(base_choice(), "version", json!("12.3.45"))).is_ok());
    // Unbekanntes Eingabefeld.
    assert_rejected_at(
        &with(base_choice(), "inputs", json!(["title", "body_html"])),
        "inputs",
    );
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_image_subject_allows_only_image_and_context_fields() {
    let image = json!({
        "id": "alt-fits",
        "version": "1.0.0",
        "name": "Alt Fits",
        "type": "predicate",
        "instructions": "The alt text describes the image.",
        "subject": "image",
        "inputs": ["image", "alt", "image_context"]
    });
    assert!(
        parse_value(&image).is_ok(),
        "Bildfrage mit Bild und Kontext abgelehnt"
    );
    for field in ["page_text", "h2", "meta_description"] {
        let bad = with(image.clone(), "inputs", json!(["image", "alt", field]));
        assert_rejected_at(&bad, "inputs");
    }
    // Nachtrag 26.11.2026 (Auslegung 3 bestätigt): Kontextfelder einer Bildfrage sind alt, image_context und url;
    // eine Seitenfrage darf ein ausdrücklich gewähltes Seitenbild (`image`) wählen, aber nicht alt/image_context.
    let with_url = with(image.clone(), "inputs", json!(["image", "alt", "url"]));
    assert!(
        parse_value(&with_url).is_ok(),
        "url als Kontextfeld einer Bildfrage abgelehnt"
    );
    let page_image = with(base_predicate(), "inputs", json!(["title", "image"]));
    assert!(
        parse_value(&page_image).is_ok(),
        "Seitenbild in einer Seitenfrage abgelehnt"
    );
    for field in ["alt", "image_context"] {
        let bad = with(base_predicate(), "inputs", json!(["title", field]));
        assert_rejected_at(&bad, "inputs");
    }
    // max_chars nur für page_text (Auslegung 2 bestätigt).
    let bad = with(
        base_predicate(),
        "inputs",
        json!([{"field": "title", "max_chars": 100}]),
    );
    errors_of(&bad);
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_optional_issues_and_segment_are_accepted() {
    let mut v = base_choice();
    v["issues"] = json!([{"name": "Some Issue"}]);
    v["segment"] = json!({"name": "Some Segment"});
    assert!(
        parse_value(&v).is_ok(),
        "optionale Felder issues/segment abgelehnt"
    );
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_wire_name_replaces_hyphens() {
    assert_eq!(wire_name("page-type"), "page_type");
    assert_eq!(wire_name("a-b-c"), "a_b_c");
    let q = parse_value(&base_choice()).unwrap();
    assert_eq!(q.wire_name(), "page_kind");
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_roundtrip_load_save_load_is_identical() {
    for value in [base_choice(), base_predicate(), base_score()] {
        let q = parse_value(&value).unwrap();
        // Speichern als kanonisches JSON bzw. Export, wieder laden → identische Definition.
        let again = parse(&q.canonical_json()).expect("kanonisches JSON ist gültig");
        assert_eq!(again, q);
        let exported = library::export(&q);
        assert_eq!(parse(&exported).expect("Export ist gültig"), q);
        // Feldwerte bleiben erhalten.
        let back: Value = serde_json::from_str(&q.canonical_json()).unwrap();
        for key in ["id", "version", "name", "type", "instructions", "subject"] {
            assert_eq!(back[key], value[key], "Feld {key} nach Rundreise verändert");
        }
        assert_eq!(back.get("choices"), value.get("choices"));
        assert_eq!(back.get("levels"), value.get("levels"));
    }
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_schema_file_matches_spec_limits() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("Schema ist JSON");
    let props = &schema["properties"];
    assert_eq!(props["id"]["pattern"], "^[a-z][a-z0-9-]{0,47}$");
    assert_eq!(props["name"]["minLength"], 1);
    assert_eq!(props["name"]["maxLength"], 60);
    assert_eq!(props["instructions"]["minLength"], 1);
    assert_eq!(props["instructions"]["maxLength"], 2000);
    assert_eq!(props["choices"]["minItems"], 2);
    assert_eq!(props["choices"]["maxItems"], 20);
    assert_eq!(
        props["choices"]["items"]["properties"]["value"]["pattern"],
        "^[a-z0-9_-]{1,32}$"
    );
    assert_eq!(props["levels"]["minItems"], 2);
    assert_eq!(props["levels"]["maxItems"], 10);
    let types: Vec<&str> = props["type"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(types, ["predicate", "choice", "score"]);
    let subjects: Vec<&str> = props["subject"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(subjects, ["page", "image"]);
    // Eingabefelder laut AI-032.
    let text = SCHEMA_JSON;
    for field in [
        "url",
        "title",
        "meta_description",
        "h1",
        "h2",
        "page_text",
        "word_count",
        "html_lang",
        "hreflang_self",
        "structured_data_types",
        "page_type",
        "image",
        "alt",
        "image_context",
    ] {
        assert!(
            text.contains(&format!("\"{field}\"")),
            "Eingabefeld {field} fehlt im Schema"
        );
    }
    // Das Schema allein weist Strukturfehler ab, akzeptiert gültige Fragen.
    assert!(schema_errors(&base_choice()).is_ok());
    assert!(schema_errors(&with(base_choice(), "name", json!(""))).is_err());
}

// ---------------------------------------------------------------------------------------------------------------
// Vorlagen (parametrisierte Systemfragen)

fn candidate_template(k: usize) -> Value {
    let mut list: Vec<Value> = (1..=k)
        .map(|i| json!({"value": format!("c{i}"), "description": "{{candidates}}"}))
        .collect();
    list.push(json!({"value": "none", "description": "No candidate replaces the old page."}));
    json!({
        "id": "redirect-target",
        "version": "1.0.0",
        "name": "Redirect Target",
        "type": "choice",
        "instructions": "Which candidate replaces the old page best?",
        "choices": list,
        "subject": "page",
        "inputs": ["url", "title", "page_text"],
        "params": [{"name": "candidates", "type": "candidates", "source": "redirect.candidates"}]
    })
}

fn string_template() -> Value {
    json!({
        "id": "lang-matches",
        "version": "1.0.0",
        "name": "Language Matches",
        "type": "predicate",
        "instructions": "Is the main content predominantly written in the language {{code}}?",
        "subject": "page",
        "inputs": ["page_text"],
        "params": [{"name": "code", "type": "string", "source": "html_lang"}]
    })
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_valid_templates_are_accepted() {
    assert!(parse_value(&string_template()).is_ok());
    assert!(parse_value(&candidate_template(10)).is_ok());
    let t = parse_value(&string_template()).unwrap();
    assert!(t.is_template());
    assert!(!parse_value(&base_choice()).unwrap().is_template());
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_template_placeholder_rules() {
    // Platzhalter in `value`.
    let mut v = candidate_template(3);
    v["choices"][0]["value"] = json!("{{candidates}}");
    errors_of(&v);
    // Platzhalter in `label`.
    let mut score = base_score();
    score["levels"][0]["label"] = json!("{{code}}");
    score["params"] = json!([{"name": "code", "type": "string", "source": "html_lang"}]);
    errors_of(&score);
    // Platzhalter in `name`.
    let mut named = string_template();
    named["name"] = json!("Language {{code}}");
    assert_rejected_at(&named, "name");
    // Platzhalter ohne passenden Eintrag in `params`.
    let mut unknown = string_template();
    unknown["instructions"] = json!("Is the content written in {{language}}?");
    errors_of(&unknown);
    // Platzhalter in `description` einer Wertliste ist erlaubt.
    let mut described = base_choice();
    described["choices"][0]["description"] = json!("Product in {{code}}.");
    described["params"] = json!([{"name": "code", "type": "string", "source": "html_lang"}]);
    assert!(
        parse_value(&described).is_ok(),
        "Platzhalter in description abgelehnt"
    );
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_candidate_template_values_are_c1_to_ck_plus_none() {
    // Andere Werte als c1 … ck, none.
    let mut wrong = candidate_template(3);
    wrong["choices"][1]["value"] = json!("second");
    errors_of(&wrong);
    // Ohne `none`.
    let mut no_none = candidate_template(3);
    no_none["choices"].as_array_mut().unwrap().pop();
    errors_of(&no_none);
    // Mehr als 10 Kandidaten.
    errors_of(&candidate_template(11));
}

fn awkward_candidates(k: usize) -> Vec<Candidate> {
    (1..=k)
        .map(|i| Candidate {
            address: format!(
                "https://ai.test:8080/größe/schuhe-{i}?farbe=grün&size=4{i}&q={{{{x}}}}#teil\"{i}"
            ),
            title: format!("Schöne Schuhe „{i}“ – {{{{candidates}}}}"),
            h1: format!("Überschrift {i} <b>&amp;</b>"),
            excerpt: format!("Auszug {i}: 100 % Leder, 2,5 cm Absatz, C++ & ü"),
        })
        .collect()
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_every_instance_is_a_valid_question_k_1_to_10() {
    let template = parse_value(&candidate_template(10)).unwrap();
    for k in 1..=10 {
        let mut values = ParamValues::new();
        values.insert(
            "candidates".to_string(),
            ParamValue::Candidates(awkward_candidates(k)),
        );
        let instance: QuestionDef =
            instantiate(&template, &values).unwrap_or_else(|e| panic!("k = {k}: {e:?}"));
        // Gültige Frage nach dem Schema (Rundreise über JSON) und den Regeln.
        let as_json = serde_json::to_value(&instance).unwrap();
        assert!(
            schema_errors(&as_json).is_ok(),
            "k = {k}: Instanz verletzt das Schema"
        );
        assert!(
            format::validate_instance(&instance).is_ok(),
            "k = {k}: Instanz verletzt Regeln"
        );
        assert!(!instance.is_template(), "Instanz trägt noch params");
        // Werte c1 … ck plus none; Version der Vorlage.
        let mut expected: Vec<String> = (1..=k).map(|i| format!("c{i}")).collect();
        expected.push("none".into());
        assert_eq!(instance.values(), expected, "k = {k}");
        assert_eq!(instance.version, template.version);
        assert_eq!(instance.id, template.id);
        // Die Kandidatendaten stehen in der description von ci, nie in value.
        let choices = instance.choices.as_ref().unwrap();
        for (i, c) in awkward_candidates(k).iter().enumerate() {
            assert!(
                choices[i].description.contains(&c.address),
                "Adresse von c{} fehlt in der description",
                i + 1
            );
            assert!(!choices[i].value.contains("http"));
        }
        assert_eq!(
            choices[k].description, "No candidate replaces the old page.",
            "description von none ist fest"
        );
    }
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_string_template_instance_replaces_placeholder() {
    let template = parse_value(&string_template()).unwrap();
    let mut values = ParamValues::new();
    values.insert("code".into(), ParamValue::String("fr".into()));
    let instance = instantiate(&template, &values).unwrap();
    assert!(instance.instructions.contains("fr"));
    assert!(!instance.instructions.contains("{{"));
    assert!(format::validate_instance(&instance).is_ok());
    assert_eq!(instance.version, template.version);
    // Die Parameterwerte gehen in params_json (Cache-Schlüssel, ai_answers), nicht in die Vorlage.
    let pj = format::params_json(&values);
    assert!(pj.to_string().contains("fr"));
    assert!(!template.canonical_json().contains("\"fr\""));
}

// spec: AI-030 [M4]
// acceptance: independent
#[test]
fn ai030_page_type_input_only_below_page_type_question() {
    // AI-032: `page_type` als Eingabe einer Frage, die über der Seitentyp-Frage steht, wird abgelehnt.
    let pt = s::page_type(&["title"]);
    let dependent = s::predicate("is-shop", "Is Shop", &["title", "page_type"]);
    assert!(validate_active(&[dependent.clone(), pt.clone()]).is_err());
    assert!(validate_active(&[pt, dependent]).is_ok());
}

//! Unabhängige Abnahmetests M4-K06: Strukturierte Ausgabe und lokale Schema-Validierung (AI-021).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-021, AI-020, AI-050 für die Seitentyp-Werte). Fixtures:
//! `tests/fixtures/ai/k06-acceptance/page-type-answer.schema.json` (eigenes Antwortschema der Seitentyp-Frage mit den
//! Werten aus AI-050) und `resources/ai/schemas/contract-response.schema.json`.
// acceptance: independent

use std::cell::RefCell;
use std::path::PathBuf;

use os_ai::schema::{
    normalize_text, normalize_value, run_with_repair, strict_violations, RawAnswer, RepairAttempts,
    RepairRequest, ResponseSchema, StructuredOutcome, StructuredRun,
};
use os_ai::AiStatus;
use serde_json::{json, Value};

fn repo_file(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

fn load(rel: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(repo_file(rel)).unwrap()).unwrap()
}

fn page_type_schema() -> ResponseSchema {
    ResponseSchema::new(load(
        "tests/fixtures/ai/k06-acceptance/page-type-answer.schema.json",
    ))
    .unwrap()
}

fn valid_answer() -> Value {
    json!({"page_type": "product", "reason": null, "details": {"confidence": 0.9}})
}

fn violating_answer() -> Value {
    // Seitentyp `shop` ist kein erlaubter Wert (AI-021).
    json!({"page_type": "shop", "reason": null, "details": {"confidence": 0.9}})
}

/// Führt einen Eintrag aus; `answers[n]` ist die Antwort auf den n-ten Request (der letzte Wert wiederholt sich).
/// Liefert das Ergebnis und die beim Senden übergebenen Reparaturhinweise.
async fn run_entry(
    repairs: RepairAttempts,
    answers: Vec<RawAnswer>,
) -> (StructuredRun<usize, ()>, Vec<Option<RepairRequest>>) {
    let schema = page_type_schema();
    let seen = RefCell::new(Vec::new());
    let run = run_with_repair(&schema, repairs, |repair| {
        let n = {
            let mut s = seen.borrow_mut();
            s.push(repair);
            s.len() - 1
        };
        let answer = answers[n.min(answers.len() - 1)].clone();
        async move { Ok::<_, ()>((answer, n)) }
    })
    .await;
    (run, seen.into_inner())
}

// spec: AI-021
// acceptance: independent
#[tokio::test]
async fn malformed_json_once_then_ok_is_valid_after_two_requests() {
    let (run, _) = run_entry(
        RepairAttempts::default(),
        vec![
            RawAnswer::Text("{\"page_type\": \"product\", ".into()),
            RawAnswer::Json(valid_answer()),
        ],
    )
    .await;
    assert_eq!(run.outcome, StructuredOutcome::Valid(valid_answer()));
    assert_eq!(run.requests.len(), 2, "Protokoll zeigt 2 Requests");
    assert!(run.requests[0].validator_error.is_some());
    assert!(run.requests[1].validator_error.is_none());
    assert_eq!(run.outcome.status(), Some(AiStatus::Success));
}

// spec: AI-021
// spec: AI-020
// acceptance: independent
#[tokio::test]
async fn permanent_schema_violation_ends_invalid_output_after_two_requests() {
    let (run, seen) = run_entry(
        RepairAttempts::default(),
        vec![RawAnswer::Json(violating_answer())],
    )
    .await;
    assert_eq!(
        run.requests.len(),
        2,
        "Standard: 1 Reparaturversuch → 2 Requests"
    );
    assert_eq!(seen.len(), 2);
    let status = run.outcome.status().unwrap();
    assert_eq!(status, AiStatus::InvalidOutput);
    assert_eq!(status.to_string(), "Error: Invalid output");
    match &run.outcome {
        StructuredOutcome::InvalidOutput { validator_error } => {
            assert!(!validator_error.is_empty(), "Validator-Fehler abrufbar");
        }
        other => panic!("Invalid output erwartet, war {other:?}"),
    }
    // Für das Protokoll trägt jeder verworfene Request seine Validator-Meldung.
    assert!(run.requests.iter().all(|r| r.validator_error.is_some()));
}

// spec: AI-021
// acceptance: independent
#[tokio::test]
async fn repair_request_carries_the_validator_message() {
    let (run, seen) = run_entry(
        RepairAttempts::default(),
        vec![
            RawAnswer::Json(violating_answer()),
            RawAnswer::Json(valid_answer()),
        ],
    )
    .await;
    assert_eq!(run.outcome, StructuredOutcome::Valid(valid_answer()));
    assert!(seen[0].is_none(), "erster Request ohne Reparaturhinweis");
    let repair = seen[1].as_ref().expect("Reparaturaufruf mit Hinweis");
    let first_error = run.requests[0].validator_error.clone().unwrap();
    assert_eq!(repair.validator_error, first_error);
    assert!(
        repair.instruction().contains(&first_error),
        "Reparaturaufruf enthält die Fehlermeldung des Validators"
    );
}

// spec: AI-021
// acceptance: independent
#[tokio::test]
async fn repair_attempts_zero_and_two_and_range_check() {
    assert_eq!(RepairAttempts::default().get(), 1, "Standard 1");
    let (run, _) = run_entry(
        RepairAttempts::new(0).unwrap(),
        vec![RawAnswer::Json(violating_answer())],
    )
    .await;
    assert_eq!(run.requests.len(), 1, "0 Reparaturversuche → 1 Request");
    assert_eq!(run.outcome.status(), Some(AiStatus::InvalidOutput));
    let (run, _) = run_entry(
        RepairAttempts::new(2).unwrap(),
        vec![RawAnswer::Json(violating_answer())],
    )
    .await;
    assert_eq!(run.requests.len(), 3, "2 Reparaturversuche → 3 Requests");
    assert_eq!(run.outcome.status(), Some(AiStatus::InvalidOutput));
    // Zweiter Reparaturversuch gelingt.
    let (run, _) = run_entry(
        RepairAttempts::new(2).unwrap(),
        vec![
            RawAnswer::Json(violating_answer()),
            RawAnswer::Text("kein json".into()),
            RawAnswer::Json(valid_answer()),
        ],
    )
    .await;
    assert_eq!(run.requests.len(), 3);
    assert_eq!(run.outcome.status(), Some(AiStatus::Success));
    // Bereich 0–2.
    for bad in [3u8, 4, 255] {
        assert!(
            RepairAttempts::new(bad).is_none(),
            "{bad} muss abgelehnt werden"
        );
        assert!(serde_json::from_value::<RepairAttempts>(json!(bad)).is_err());
    }
    for ok in [0u8, 1, 2] {
        assert_eq!(RepairAttempts::new(ok).unwrap().get(), ok);
    }
}

// spec: AI-021
// acceptance: independent
#[tokio::test]
async fn refusal_is_neither_validated_nor_repaired() {
    let (run, _) = run_entry(RepairAttempts::default(), vec![RawAnswer::Refusal]).await;
    assert_eq!(run.requests.len(), 1);
    assert_eq!(run.outcome.status(), Some(AiStatus::Refusal));
}

// spec: AI-021
// acceptance: independent
#[test]
fn additional_fields_and_enum_values_outside_list_are_violations() {
    let schema = page_type_schema();
    assert!(schema.validate(&valid_answer()).is_ok());
    let mut extra = valid_answer();
    extra["confidence_overall"] = json!(1);
    assert!(schema.validate(&extra).is_err(), "zusätzliches Feld oben");
    let mut nested_extra = valid_answer();
    nested_extra["details"]["note"] = json!("x");
    assert!(
        schema.validate(&nested_extra).is_err(),
        "zusätzliches Feld verschachtelt"
    );
    let errors = schema.validate(&violating_answer()).unwrap_err();
    assert!(!errors.is_empty(), "Seitentyp `shop` ist ein Verstoß");
    let mut missing = valid_answer();
    missing.as_object_mut().unwrap().remove("reason");
    assert!(schema.validate(&missing).is_err(), "fehlendes Pflichtfeld");
}

// spec: AI-021
// acceptance: independent
#[tokio::test]
async fn extra_field_in_answer_leads_to_repair_and_invalid_output() {
    let mut extra = valid_answer();
    extra["unexpected"] = json!(true);
    let (run, _) = run_entry(RepairAttempts::default(), vec![RawAnswer::Json(extra)]).await;
    assert_eq!(run.requests.len(), 2);
    assert_eq!(run.outcome.status(), Some(AiStatus::InvalidOutput));
}

// spec: AI-021
// acceptance: independent
#[test]
fn normalisation_trims_whitespace_and_removes_control_characters() {
    assert_eq!(normalize_text("  Produktseite \t\n"), "Produktseite");
    assert_eq!(
        normalize_text("Pro\u{0007}dukt\u{0000}seite"),
        "Produktseite"
    );
    assert_eq!(normalize_text("\u{001b}[0mText\u{007f}"), "[0mText");
    let v = normalize_value(json!({"a": "  x\u{0001} ", "b": ["\u{0002} y "], "c": 3}));
    assert_eq!(v, json!({"a": "x", "b": ["y"], "c": 3}));
}

// spec: AI-021
// acceptance: independent
#[tokio::test]
async fn answers_are_normalised_before_being_accepted() {
    let raw = json!({"page_type": " product\u{0007}\n", "reason": "  weil\u{0001} ", "details": {"confidence": 0.5}});
    let (run, _) = run_entry(RepairAttempts::default(), vec![RawAnswer::Json(raw)]).await;
    assert_eq!(
        run.outcome,
        StructuredOutcome::Valid(
            json!({"page_type": "product", "reason": "weil", "details": {"confidence": 0.5}})
        )
    );
    assert_eq!(run.requests.len(), 1);
    // Antworttext mit umgebendem Leerraum wird ebenfalls übernommen.
    let text = format!("\n  {}  \n", valid_answer());
    let (run, _) = run_entry(RepairAttempts::default(), vec![RawAnswer::Text(text)]).await;
    assert_eq!(run.outcome, StructuredOutcome::Valid(valid_answer()));
}

// spec: AI-021
// acceptance: independent
#[test]
fn strict_mode_checker_accepts_correct_and_rejects_optional_or_open_nested() {
    // Fixture-Schemas bestehen.
    let fixture = load("tests/fixtures/ai/k06-acceptance/page-type-answer.schema.json");
    assert!(
        strict_violations(&fixture).is_empty(),
        "{:?}",
        strict_violations(&fixture)
    );
    let contract = load("resources/ai/schemas/contract-response.schema.json");
    assert!(
        strict_violations(&contract).is_empty(),
        "{:?}",
        strict_violations(&contract)
    );

    let correct = json!({
        "type": "object",
        "properties": {
            "a": {"type": ["string", "null"]},
            "b": {
                "type": "object",
                "properties": {"c": {"type": "number"}},
                "required": ["c"],
                "additionalProperties": false
            },
            "list": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {"d": {"type": "boolean"}},
                    "required": ["d"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["a", "b", "list"],
        "additionalProperties": false
    });
    assert!(
        strict_violations(&correct).is_empty(),
        "{:?}",
        strict_violations(&correct)
    );

    // Optionales Feld (nicht in `required`).
    let mut optional = correct.clone();
    optional["required"] = json!(["b", "list"]);
    assert!(
        !strict_violations(&optional).is_empty(),
        "optionales Feld fällt durch"
    );

    // Optionales Feld auf verschachtelter Ebene.
    let mut nested_optional = correct.clone();
    nested_optional["properties"]["b"]["required"] = json!([]);
    assert!(!strict_violations(&nested_optional).is_empty());

    // Ohne `additionalProperties: false` auf verschachtelter Ebene (Objekt und Array-Element).
    let mut nested_open = correct.clone();
    nested_open["properties"]["b"]
        .as_object_mut()
        .unwrap()
        .remove("additionalProperties");
    assert!(
        !strict_violations(&nested_open).is_empty(),
        "verschachtelt offen fällt durch"
    );
    let mut items_open = correct.clone();
    items_open["properties"]["list"]["items"]["additionalProperties"] = json!(true);
    assert!(
        !strict_violations(&items_open).is_empty(),
        "Array-Element offen fällt durch"
    );

    // Ohne `additionalProperties: false` auf oberster Ebene.
    let mut top_open = correct;
    top_open
        .as_object_mut()
        .unwrap()
        .remove("additionalProperties");
    assert!(!strict_violations(&top_open).is_empty());
}

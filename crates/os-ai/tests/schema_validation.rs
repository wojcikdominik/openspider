//! Lokale Schema-Validierung jeder Antwort mit Reparaturversuchen und Normalisierung (AI-021).
//!
//! Antwortfolgen wie die ai-mock-Szenarien `malformed-json` (Antwort `{broken`) und `schema-violation` (Antwort
//! `{"unexpected": ["not the requested schema"]}`), siehe 08 „Konventionen“.
use std::cell::RefCell;
use std::path::PathBuf;

use os_ai::schema::{
    normalize_text, normalize_value, run_with_repair, RawAnswer, RepairAttempts, RepairRequest,
    ResponseSchema, StructuredOutcome,
};
use os_ai::AiStatus;
use serde_json::{json, Value};

fn repo_file(parts: &[&str]) -> Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("..");
    for part in parts {
        path.push(part);
    }
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn contract_schema() -> ResponseSchema {
    ResponseSchema::new(repo_file(&[
        "resources",
        "ai",
        "schemas",
        "contract-response.schema.json",
    ]))
    .unwrap()
}

fn page_type_schema() -> ResponseSchema {
    ResponseSchema::new(repo_file(&[
        "tests",
        "fixtures",
        "ai",
        "k06",
        "page-type-answer.schema.json",
    ]))
    .unwrap()
}

/// ai-mock-Antwort je Szenario und Request-Nummer (1 …).
fn mock(scenario: &str, request: usize) -> RawAnswer {
    match scenario {
        "malformed-json" if request == 1 => RawAnswer::Text("{broken".into()),
        "schema-violation" => {
            RawAnswer::Text(r#"{"unexpected":["not the requested schema"]}"#.into())
        }
        _ => RawAnswer::Text(r#"{"ok":true}"#.into()),
    }
}

/// Ein Eintrag mit Protokoll der Requests (Begleitwert = Request-Nummer) und der erhaltenen Reparaturhinweise.
async fn entry(
    schema: &ResponseSchema,
    repairs: RepairAttempts,
    scenario: &'static str,
) -> (
    os_ai::schema::StructuredRun<usize, ()>,
    Vec<Option<RepairRequest>>,
) {
    let hints = RefCell::new(Vec::new());
    let run = run_with_repair(schema, repairs, |repair| {
        hints.borrow_mut().push(repair);
        let n = hints.borrow().len();
        async move { Ok::<_, ()>((mock(scenario, n), n)) }
    })
    .await;
    (run, hints.into_inner())
}

// spec: AI-021
// `malformed-json` einmalig, danach `ok` → Ergebnis gültig, 2 Requests für den Eintrag.
#[tokio::test]
async fn malformed_json_once_is_repaired_with_a_second_request() {
    let (run, hints) = entry(
        &contract_schema(),
        RepairAttempts::default(),
        "malformed-json",
    )
    .await;
    assert_eq!(run.outcome, StructuredOutcome::Valid(json!({"ok": true})));
    assert_eq!(run.outcome.status(), Some(AiStatus::Success));
    assert_eq!(run.requests.len(), 2);
    assert!(run.requests[0]
        .validator_error
        .as_deref()
        .unwrap()
        .contains("not valid JSON"));
    assert_eq!(run.requests[1].validator_error, None);
    assert_eq!(hints[0], None);
    let repair = hints[1].as_ref().unwrap();
    assert_eq!(repair.attempt, 1);
    assert!(repair.instruction().contains(&repair.validator_error));
}

// spec: AI-021
// `schema-violation` dauerhaft → nach 2 Requests `Error: Invalid output`; der Validator-Fehler steht im Protokoll.
#[tokio::test]
async fn permanent_schema_violation_ends_as_invalid_output_after_two_requests() {
    let (run, hints) = entry(
        &contract_schema(),
        RepairAttempts::default(),
        "schema-violation",
    )
    .await;
    assert_eq!(run.requests.len(), 2);
    let StructuredOutcome::InvalidOutput { validator_error } = &run.outcome else {
        panic!("{:?}", run.outcome);
    };
    assert!(validator_error.contains("ok"), "{validator_error}");
    assert_eq!(run.outcome.status(), Some(AiStatus::InvalidOutput));
    assert_eq!(
        run.outcome.status().unwrap().to_string(),
        "Error: Invalid output"
    );
    for request in &run.requests {
        assert!(
            request.validator_error.is_some(),
            "Validator-Fehler je Request im Protokoll"
        );
    }
    // Der Reparaturversuch trägt die Meldung des Validators zur ersten Antwort.
    assert_eq!(
        hints[1].as_ref().unwrap().validator_error,
        run.requests[0].validator_error.clone().unwrap()
    );
}

// spec: AI-021
// Reparaturversuche: genau die konfigurierte Zahl (0–2); Standard 1; Werte außerhalb werden abgewiesen.
#[tokio::test]
async fn repair_attempts_are_configurable_between_zero_and_two() {
    assert_eq!(RepairAttempts::default().get(), 1);
    assert!(RepairAttempts::new(3).is_none());
    assert!(serde_json::from_value::<RepairAttempts>(json!(3)).is_err());
    assert_eq!(
        serde_json::from_value::<RepairAttempts>(json!(2))
            .unwrap()
            .get(),
        2
    );
    for (repairs, requests) in [(0u8, 1usize), (1, 2), (2, 3)] {
        let (run, _) = entry(
            &contract_schema(),
            RepairAttempts::new(repairs).unwrap(),
            "schema-violation",
        )
        .await;
        assert_eq!(run.requests.len(), requests, "{repairs} Reparaturversuche");
        assert_eq!(run.outcome.status(), Some(AiStatus::InvalidOutput));
    }
    let (run, _) = entry(
        &contract_schema(),
        RepairAttempts::new(0).unwrap(),
        "malformed-json",
    )
    .await;
    assert_eq!(run.requests.len(), 1);
    assert_eq!(run.outcome.status(), Some(AiStatus::InvalidOutput));
}

// spec: AI-021
// Zusätzliche, nicht erlaubte Felder sind ein Verstoß (`additionalProperties: false`); ein Enum-Wert außerhalb der
// erlaubten Werte (Seitentyp `shop`) ebenfalls.
#[test]
fn additional_fields_and_unknown_enum_values_are_violations() {
    let contract = contract_schema();
    assert!(contract.validate(&json!({"ok": true})).is_ok());
    let extra = contract
        .validate(&json!({"ok": true, "extra": 1}))
        .unwrap_err();
    assert!(extra.iter().any(|e| e.contains("extra")), "{extra:?}");
    let page = page_type_schema();
    let valid = json!({"page_type": "product", "probability": 0.93, "note": null, "evidence": []});
    assert!(page.validate(&valid).is_ok());
    let shop = json!({"page_type": "shop", "probability": 0.93, "note": null, "evidence": []});
    let errors = page.validate(&shop).unwrap_err();
    assert!(
        errors.iter().any(|e| e.starts_with("/page_type")),
        "{errors:?}"
    );
    let nested_extra = json!({"page_type": "product", "probability": 0.5, "note": "x",
        "evidence": [{"field": "title", "quote": "a", "why": "b"}]});
    assert!(page.validate(&nested_extra).is_err());
    let missing_optional = json!({"page_type": "product", "probability": 0.5, "evidence": []});
    assert!(
        page.validate(&missing_optional).is_err(),
        "„optional“ ist Pflichtfeld mit null"
    );
}

// spec: AI-021
// Antworttexte werden vor der Übernahme normalisiert (Whitespace trimmen, Steuerzeichen entfernen) – auch vor der
// Validierung, sodass `" product\u0000 "` als `product` gilt.
#[tokio::test]
async fn answers_are_normalised_before_validation_and_adoption() {
    assert_eq!(
        normalize_text("\t Wanderrucksack\u{0} 30 l \n"),
        "Wanderrucksack 30 l"
    );
    assert_eq!(
        normalize_value(json!({" key ": [" a\u{7}", 1, null]})),
        json!({" key ": ["a", 1, null]})
    );
    let run = run_with_repair(&page_type_schema(), RepairAttempts::default(), |_| async {
        Ok::<_, ()>((
            RawAnswer::Json(json!({"page_type": " product\u{0} ", "probability": 1,
                "note": "  Hinweis\r\n", "evidence": [{"field": " title", "quote": "\u{1b}Zitat "}]})),
            (),
        ))
    })
    .await;
    assert_eq!(
        run.outcome,
        StructuredOutcome::Valid(
            json!({"page_type": "product", "probability": 1, "note": "Hinweis",
            "evidence": [{"field": "title", "quote": "Zitat"}]})
        )
    );
    assert_eq!(run.requests.len(), 1);
}

// spec: AI-021
// Refusals werden weder validiert noch repariert; ein gescheiterter Request beendet den Eintrag ohne Reparatur.
#[tokio::test]
async fn refusals_and_failed_requests_are_not_repaired() {
    let run = run_with_repair(
        &contract_schema(),
        RepairAttempts::new(2).unwrap(),
        |_| async { Ok::<_, ()>((RawAnswer::Refusal, ())) },
    )
    .await;
    assert_eq!(run.outcome, StructuredOutcome::Refusal);
    assert_eq!(run.outcome.status(), Some(AiStatus::Refusal));
    assert_eq!(run.requests.len(), 1);
    let failed = run_with_repair(
        &contract_schema(),
        RepairAttempts::new(2).unwrap(),
        |_| async { Err::<(RawAnswer, ()), _>("Error: Timeout nach 30 s") },
    )
    .await;
    assert_eq!(
        failed.outcome,
        StructuredOutcome::Failed("Error: Timeout nach 30 s")
    );
    assert_eq!(failed.outcome.status(), None);
    assert!(failed.requests.is_empty());
    let unparsable = run_with_repair(
        &contract_schema(),
        RepairAttempts::default(),
        |r| async move {
            Ok::<_, ()>((
                if r.is_none() {
                    RawAnswer::Unparsable {
                        detail: "EOF while parsing".into(),
                    }
                } else {
                    RawAnswer::Json(json!({"ok": false}))
                },
                (),
            ))
        },
    )
    .await;
    assert_eq!(
        unparsable.outcome,
        StructuredOutcome::Valid(json!({"ok": false}))
    );
}

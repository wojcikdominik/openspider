//! Prüfer „strikt verträglich“ (AI-021): jedes Objekt führt alle Properties in `required` und hat
//! `additionalProperties: false`, auf jeder Ebene; CI-Test über alle Schemas unter `resources/ai/schemas/`.
use std::path::PathBuf;

use os_ai::schema::{strict_violations, StrictViolation, StrictViolationKind};
use serde_json::{json, Value};

fn schema_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in ["..", "..", "resources", "ai", "schemas"] {
        path.push(part);
    }
    path
}

/// Schemas von **Datendateien** (Preistabelle, Aufgabenvorgaben, AI-Crawler-Katalog; AI-010/AI-011/AI-120), keine Antwortschemas einer Aufgabe.
/// Sie liegen im selben Ordner, beschreiben aber Dateien unter `resources/ai/` und keine Modellantworten; für sie gilt
/// der strikte Modus der Anbieter nicht. Jede andere Datei im Ordner gilt als Aufgabenschema und muss strikt sein.
const DATA_FILE_SCHEMAS: &[&str] = &[
    "ai-crawlers.schema.json",
    "pricing.schema.json",
    "task-defaults.schema.json",
];

// spec: AI-021
#[test]
fn every_task_schema_under_resources_is_strict_compatible() {
    let mut task_schemas = 0;
    let mut entries: Vec<_> = std::fs::read_dir(schema_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            jsonschema::validator_for(&schema).is_ok(),
            "{name}: ungültiges JSON-Schema"
        );
        if DATA_FILE_SCHEMAS.contains(&name.as_str()) {
            continue;
        }
        task_schemas += 1;
        let violations = strict_violations(&schema);
        assert!(violations.is_empty(), "{name}: {violations:?}");
    }
    assert!(task_schemas >= 1, "mindestens ein Aufgabenschema geprüft");
    for name in DATA_FILE_SCHEMAS {
        assert!(
            schema_dir().join(name).is_file(),
            "{name} fehlt – Liste anpassen"
        );
    }
}

// spec: AI-021
// Der Prüfer erkennt optionale Properties und fehlendes `additionalProperties: false` auf jeder Ebene.
#[test]
fn checker_finds_violations_at_every_level() {
    let schema = json!({
        "type": "object",
        "properties": {
            "a": {"type": "string"},
            "b": {"type": ["string", "null"]},
            "list": {"type": "array", "items": {
                "type": "object",
                "properties": {"x": {"type": "integer"}, "y": {"type": "integer"}},
                "required": ["x"],
                "additionalProperties": false
            }},
            "choice": {"anyOf": [
                {"type": "object", "properties": {"k": {"type": "string"}}, "required": ["k"]},
                {"type": "null"}
            ]},
            "ref": {"$ref": "#/$defs/inner"}
        },
        "required": ["a", "list", "choice", "ref", "ghost"],
        "additionalProperties": false,
        "$defs": {"inner": {"type": "object", "properties": {}, "additionalProperties": true}}
    });
    let mut found = strict_violations(&schema);
    found.sort_by(|a, b| a.pointer.cmp(&b.pointer));
    let expected = vec![
        StrictViolation {
            pointer: String::new(),
            kind: StrictViolationKind::NotRequired("b".into()),
        },
        StrictViolation {
            pointer: String::new(),
            kind: StrictViolationKind::UnknownRequired("ghost".into()),
        },
        StrictViolation {
            pointer: "/$defs/inner".into(),
            kind: StrictViolationKind::AdditionalPropertiesNotFalse,
        },
        StrictViolation {
            pointer: "/properties/choice/anyOf/0".into(),
            kind: StrictViolationKind::AdditionalPropertiesNotFalse,
        },
        StrictViolation {
            pointer: "/properties/list/items".into(),
            kind: StrictViolationKind::NotRequired("y".into()),
        },
    ];
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for violation in expected {
        assert!(
            found.contains(&violation),
            "fehlt: {violation:?} in {found:?}"
        );
    }
}

// spec: AI-021
// „optional“ als Pflichtfeld mit `["<typ>", "null"]` ist strikt verträglich; das Testschema der Fixture ebenso.
#[test]
fn nullable_required_fields_are_strict() {
    let schema = json!({
        "type": "object",
        "properties": {"note": {"type": ["string", "null"]}, "n": {"type": "integer"}},
        "required": ["note", "n"],
        "additionalProperties": false
    });
    assert!(strict_violations(&schema).is_empty());
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in [
        "..",
        "..",
        "tests",
        "fixtures",
        "ai",
        "k06",
        "page-type-answer.schema.json",
    ] {
        path.push(part);
    }
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(strict_violations(&fixture).is_empty());
    // Die Datendatei-Schemas sind bewusst nicht strikt (optionale Felder, Map-Objekte): der Prüfer meldet das.
    let pricing: Value = serde_json::from_str(
        &std::fs::read_to_string(schema_dir().join("pricing.schema.json")).unwrap(),
    )
    .unwrap();
    assert!(!strict_violations(&pricing).is_empty());
}

//! Cache-Schlüssel nach AI-015: Golden-Test mit 10 Schlüsseln, Plattformunabhängigkeit und Bestandteile.
use std::path::PathBuf;

use os_ai::cache::{
    canonical_json, input_hash, params_hash, CacheKey, CacheKeyInput, ExecutionPath,
};
use os_ai::provider::Input;
use serde_json::{json, Value};

fn fixture_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in [
        "..",
        "..",
        "tests",
        "fixtures",
        "ai",
        "k06",
        "cache-keys.json",
    ] {
        path.push(part);
    }
    path
}

fn golden() -> Vec<Value> {
    let text = std::fs::read_to_string(fixture_path()).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    doc["cases"].as_array().unwrap().clone()
}

fn baseline() -> CacheKeyInput {
    CacheKeyInput {
        task: "decision".into(),
        definition_id: "page-type".into(),
        definition_version: "1.0.0".into(),
        params: json!({}),
        path: Some(ExecutionPath::DecisionsApi),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: "https://openai.mock.test/openai/v1".into(),
        result_params: json!({}),
        input_hash: input_hash(&[Input::Text {
            text: "Seitentext".into(),
        }]),
    }
}

// spec: AI-015
// Golden-Test: 10 unabhängig (Python) berechnete Schlüssel; gleiche Eingabe → gleicher Hash auf jeder Plattform.
#[test]
fn golden_cache_keys_match_independently_computed_hashes() {
    let cases = golden();
    assert_eq!(cases.len(), 10);
    let mut keys = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let parts: Vec<Input> = serde_json::from_value(case["parts"].clone()).unwrap();
        let input: CacheKeyInput = serde_json::from_value(case["key_input"].clone()).unwrap();
        assert_eq!(input_hash(&parts), input.input_hash, "{name}: input hash");
        assert_eq!(
            params_hash(&input.params),
            case["params_hash"].as_str().unwrap(),
            "{name}: params hash"
        );
        let key = CacheKey::new(input);
        assert_eq!(
            key.canonical(),
            case["canonical"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(key.as_str(), case["key"].as_str().unwrap(), "{name}");
        assert_eq!(key.as_str().len(), 64);
        keys.push(key.as_str().to_string());
    }
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 10, "alle 10 Schlüssel verschieden");
}

// spec: AI-015
// Plattformunabhängig: Reihenfolge der Objektschlüssel und Leerraum der Quelle ändern den Schlüssel nicht; Text wird
// nicht umgeschrieben (CRLF bleibt CRLF und ergibt einen anderen Hash als LF).
#[test]
fn key_ignores_object_order_and_formatting_but_not_content() {
    let a: Value = serde_json::from_str(r#"{"code":"de","candidates":["A","B"]}"#).unwrap();
    let b: Value = serde_json::from_str(
        "{\r\n  \"candidates\" : [ \"A\", \"B\" ],\r\n  \"code\": \"de\"\r\n}",
    )
    .unwrap();
    assert_eq!(canonical_json(&a), canonical_json(&b));
    assert_eq!(params_hash(&a), params_hash(&b));
    let lf = input_hash(&[Input::Text {
        text: "a\nb".into(),
    }]);
    let crlf = input_hash(&[Input::Text {
        text: "a\r\nb".into(),
    }]);
    assert_ne!(lf, crlf);
}

// spec: AI-015
// Jeder Bestandteil aus AI-015 ändert den Schlüssel; Schwellwerte sind kein Bestandteil (es gibt kein Feld dafür).
#[test]
fn every_listed_component_changes_the_key() {
    let base = CacheKey::new(baseline());
    let variants: Vec<(&str, CacheKeyInput)> = vec![
        (
            "task",
            CacheKeyInput {
                task: "suggest-title".into(),
                ..baseline()
            },
        ),
        (
            "definition id",
            CacheKeyInput {
                definition_id: "search-intent".into(),
                ..baseline()
            },
        ),
        (
            "version",
            CacheKeyInput {
                definition_version: "1.0.1".into(),
                ..baseline()
            },
        ),
        (
            "params",
            CacheKeyInput {
                params: json!({"code": "en"}),
                ..baseline()
            },
        ),
        (
            "path",
            CacheKeyInput {
                path: Some(ExecutionPath::StructuredOutput),
                ..baseline()
            },
        ),
        (
            "no path",
            CacheKeyInput {
                path: None,
                ..baseline()
            },
        ),
        (
            "provider",
            CacheKeyInput {
                provider: "openrouter".into(),
                ..baseline()
            },
        ),
        (
            "model",
            CacheKeyInput {
                model: "gpt-6-luna-mini".into(),
                ..baseline()
            },
        ),
        (
            "endpoint",
            CacheKeyInput {
                endpoint: "https://eu.openai.mock.test/openai/v1".into(),
                ..baseline()
            },
        ),
        (
            "result params",
            CacheKeyInput {
                result_params: json!({"temperature": 0.2}),
                ..baseline()
            },
        ),
        (
            "input",
            CacheKeyInput {
                input_hash: input_hash(&[Input::Text {
                    text: "Seitentext!".into(),
                }]),
                ..baseline()
            },
        ),
    ];
    for (name, input) in variants {
        assert_ne!(CacheKey::new(input).as_str(), base.as_str(), "{name}");
    }
    assert_eq!(CacheKey::new(baseline()), base, "deterministisch");
}

// spec: AI-015
// Bilder gehen über ihren Byte-Hash ein: andere Base64-Schreibweise derselben Bytes (Zeilenumbruch, ohne Auffüllung)
// ergibt denselben Hash, andere Bytes einen anderen.
#[test]
fn images_are_hashed_by_their_bytes() {
    let image = |b64: &str| Input::Image {
        media_type: "image/png".into(),
        base64: b64.into(),
    };
    let a = input_hash(&[image("Zm9vYmFyYmF6")]);
    assert_eq!(a, input_hash(&[image("Zm9vYmFy\nYmF6")]));
    assert_ne!(a, input_hash(&[image("Zm9vYmFyYmF7")]));
    let short = input_hash(&[image("Zm9vYg==")]);
    assert_eq!(short, input_hash(&[image("Zm9vYg")]));
    // Medientyp und Reihenfolge der Teile zählen.
    assert_ne!(
        a,
        input_hash(&[Input::Image {
            media_type: "image/jpeg".into(),
            base64: "Zm9vYmFyYmF6".into()
        }])
    );
    let text = Input::Text { text: "x".into() };
    assert_ne!(
        input_hash(&[text.clone(), image("Zm9v")]),
        input_hash(&[image("Zm9v"), text])
    );
}

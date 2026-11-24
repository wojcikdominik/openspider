//! Unabhängige Abnahmetests M4-F01: Statuswerte (AI-020), Fähigkeiten und einheitliche `usage` (AI-001).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md`.
// acceptance: independent

use os_ai::provider::{Capabilities, Usage};
use os_ai::AiStatus;

/// Alle Statuswerte der Tabelle in AI-020, in Tabellenreihenfolge, mit dem exakten Text der Spec. Die
/// parametrisierten Zeilen (`Timeout nach <n> s`, `<HTTP-Code> <Meldung>`) mit Beispielwerten.
fn spec_table() -> Vec<(AiStatus, &'static str)> {
    vec![
        (AiStatus::Success, "Success"),
        (AiStatus::SuccessCached, "Success (cached)"),
        (AiStatus::Refusal, "Refusal"),
        (AiStatus::Pending, "Pending"),
        (AiStatus::PendingBatch, "Pending (batch)"),
        (AiStatus::PendingConfirmation, "Pending (confirmation)"),
        (AiStatus::PendingConsent, "Pending (consent)"),
        (AiStatus::SkippedScope, "Skipped (scope)"),
        (AiStatus::SkippedSegment, "Skipped (segment)"),
        (AiStatus::SkippedEmptyInput, "Skipped (empty input)"),
        (AiStatus::SkippedPolicy, "Skipped (policy)"),
        (
            AiStatus::SkippedNoCapableModel,
            "Skipped (no capable model)",
        ),
        (AiStatus::SkippedImageTooLarge, "Skipped (image too large)"),
        (
            AiStatus::SkippedUnsupportedFormat,
            "Skipped (unsupported format)",
        ),
        (
            AiStatus::SkippedNoDeclaredLanguage,
            "Skipped (no declared language)",
        ),
        (AiStatus::BudgetExceeded, "Budget exceeded"),
        (AiStatus::QuotaExceeded, "Quota exceeded"),
        (AiStatus::MissingApiKey, "Error: Missing API key"),
        (
            AiStatus::Timeout { seconds: 30 },
            "Error: Timeout nach 30 s",
        ),
        (AiStatus::InvalidOutput, "Error: Invalid output"),
        (AiStatus::MissingAnswer, "Error: Missing answer"),
        (AiStatus::BatchExpired, "Error: Batch expired"),
        // Nachtrag 24.11.2026 (K4-15), ergänzt von M4-K06.
        (AiStatus::ConnectionFailed, "Error: Connection failed"),
        (
            AiStatus::HttpError {
                code: 500,
                message: "Internal Server Error".into(),
            },
            "Error: 500 Internal Server Error",
        ),
    ]
}

/// Erschöpfende Zuordnung ohne Platzhalter-Arm: Kommt ein Statuswert hinzu, der nicht in AI-020 steht, bricht
/// dieser Test beim Übersetzen (die Menge der Statuswerte ist genau die Tabelle).
fn table_row(s: &AiStatus) -> usize {
    match s {
        AiStatus::Success => 0,
        AiStatus::SuccessCached => 1,
        AiStatus::Refusal => 2,
        AiStatus::Pending
        | AiStatus::PendingBatch
        | AiStatus::PendingConfirmation
        | AiStatus::PendingConsent => 3,
        AiStatus::SkippedScope
        | AiStatus::SkippedSegment
        | AiStatus::SkippedEmptyInput
        | AiStatus::SkippedPolicy
        | AiStatus::SkippedNoCapableModel
        | AiStatus::SkippedImageTooLarge
        | AiStatus::SkippedUnsupportedFormat
        | AiStatus::SkippedNoDeclaredLanguage => 4,
        AiStatus::BudgetExceeded => 5,
        AiStatus::QuotaExceeded => 6,
        AiStatus::MissingApiKey => 7,
        AiStatus::Timeout { .. } => 8,
        AiStatus::InvalidOutput => 9,
        AiStatus::MissingAnswer => 10,
        AiStatus::BatchExpired => 11,
        AiStatus::HttpError { .. } => 12,
        AiStatus::ConnectionFailed => 13,
    }
}

// spec: AI-020
// acceptance: independent
#[test]
fn every_status_value_has_the_exact_spec_text() {
    let table = spec_table();
    assert_eq!(
        table.len(),
        24,
        "24 Statuswerte laut Tabelle AI-020 (mit K4-15)"
    );
    for (status, text) in &table {
        assert_eq!(status.to_string(), *text);
    }
    // Alle 14 Tabellenzeilen sind belegt (13 + `Error: Connection failed`, K4-15).
    let mut rows: Vec<usize> = table.iter().map(|(s, _)| table_row(s)).collect();
    rows.sort_unstable();
    rows.dedup();
    assert_eq!(rows, (0..14).collect::<Vec<_>>());
    // Keine zwei Werte mit demselben Text.
    let mut texts: Vec<&str> = table.iter().map(|(_, t)| *t).collect();
    texts.sort_unstable();
    texts.dedup();
    assert_eq!(texts.len(), table.len());
}

// spec: AI-020
// acceptance: independent
// Format wie INTEG-179: `Error: Timeout nach <n> s` mit der eingestellten Sekundenzahl.
#[test]
fn timeout_status_carries_configured_seconds() {
    for n in [10u64, 30, 120, 300, 3600] {
        assert_eq!(
            AiStatus::Timeout { seconds: n }.to_string(),
            format!("Error: Timeout nach {n} s")
        );
    }
    assert_eq!(
        AiStatus::HttpError {
            code: 404,
            message: "Not Found".into()
        }
        .to_string(),
        "Error: 404 Not Found"
    );
}

// spec: AI-020
// acceptance: independent
// „Ein Filter „Errors“ je KI-Tab enthält genau die Einträge mit Status `Error: …`, `Quota exceeded` oder
// `Budget exceeded`; `Skipped (…)` und `Refusal` zählen nicht dazu.“ Dieselbe Menge gilt für „Fehlgeschlagene
// erneut anfragen“.
#[test]
fn errors_membership_is_exactly_error_quota_and_budget() {
    for (status, text) in spec_table() {
        let expected =
            text.starts_with("Error: ") || text == "Quota exceeded" || text == "Budget exceeded";
        assert_eq!(status.is_error(), expected, "{text}");
    }
    assert!(!AiStatus::Refusal.is_error());
    assert!(!AiStatus::SkippedScope.is_error());
    assert!(!AiStatus::SuccessCached.is_error());
    assert!(!AiStatus::PendingConsent.is_error());
}

// spec: AI-020
// acceptance: independent
// Statuswerte ohne Parameter lassen sich aus ihrem Spec-Text lesen und schreiben (z. B. aus einer Statusspalte).
#[test]
fn plain_status_values_roundtrip_through_their_spec_text() {
    for (status, text) in spec_table() {
        if matches!(
            status,
            AiStatus::Timeout { .. } | AiStatus::HttpError { .. }
        ) {
            continue;
        }
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json, serde_json::Value::String(text.to_string()));
        let back: AiStatus = serde_json::from_value(json).unwrap();
        assert_eq!(back, status);
    }
}

// spec: AI-001
// acceptance: independent
// „Alle Adapter liefern `usage` in einheitlicher Form (`input_tokens`, `output_tokens`, `cached_input_tokens`,
// `cache_write_tokens`); fehlende Felder des Anbieters ergeben `null`, nicht `0`.“
#[test]
fn usage_has_the_four_spec_fields_and_missing_means_null() {
    let empty = serde_json::to_value(Usage::default()).unwrap();
    let obj = empty.as_object().unwrap();
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "cache_write_tokens",
            "cached_input_tokens",
            "input_tokens",
            "output_tokens"
        ]
    );
    for (k, v) in obj {
        assert!(v.is_null(), "{k} muss null sein, nicht {v}");
    }
    // Teilweise geliefert: nur die gelieferten Felder sind Zahlen, eine echte 0 bleibt 0.
    let partial = Usage {
        input_tokens: Some(1000),
        output_tokens: Some(0),
        cached_input_tokens: None,
        cache_write_tokens: None,
    };
    let v = serde_json::to_value(&partial).unwrap();
    assert_eq!(v["input_tokens"], 1000);
    assert_eq!(v["output_tokens"], 0);
    assert!(v["cached_input_tokens"].is_null());
    assert!(v["cache_write_tokens"].is_null());
    // Eine Anbieterantwort ohne Felder (JSON `{}` bzw. explizite nulls) liest sich als null, nicht 0.
    let from_nulls: Usage = serde_json::from_str(
        r#"{"input_tokens":null,"output_tokens":null,"cached_input_tokens":null,"cache_write_tokens":null}"#,
    )
    .unwrap();
    assert_eq!(from_nulls, Usage::default());
    assert_eq!(from_nulls.input_tokens, None);
}

// spec: AI-001
// acceptance: independent
// Die Fähigkeitsmatrix hat die Spalten `Text`, `Structured Output`, `Vision`, `Embeddings`, `Decisions`, `Batch`,
// `Prices via API`, `Local`; ein Adapter erfüllt eine Aufgabe nur, wenn er alle benötigten Fähigkeiten hat.
#[test]
fn capability_check_requires_all_requested_columns() {
    let none = Capabilities::default();
    let all = Capabilities {
        text: true,
        structured_output: true,
        vision: true,
        embeddings: true,
        decisions: true,
        batch: true,
        prices_via_api: true,
        local: true,
    };
    // Acht Spalten, alle standardmäßig aus.
    let v = serde_json::to_value(none).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 8);
    assert!(v.as_object().unwrap().values().all(|b| b == false));
    assert!(all.supports(all));
    assert!(all.supports(none));
    assert!(none.supports(none));
    assert!(!none.supports(all));
    // Beispiele aus der Matrix in AI-001.
    let ollama = Capabilities {
        text: true,
        structured_output: true,
        embeddings: true,
        local: true,
        ..none
    };
    let decisions = Capabilities {
        decisions: true,
        ..none
    };
    let vision_task = Capabilities {
        text: true,
        vision: true,
        ..none
    };
    let embedding_task = Capabilities {
        embeddings: true,
        ..none
    };
    assert!(!ollama.supports(vision_task), "Ollama ohne Vision-Modell");
    assert!(ollama.supports(embedding_task));
    assert!(!decisions.supports(embedding_task));
    assert!(!decisions.supports(Capabilities { text: true, ..none }));
    assert!(decisions.supports(Capabilities {
        decisions: true,
        ..none
    }));
}

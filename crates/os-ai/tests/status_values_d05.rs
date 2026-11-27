//! M4-D05: neuer Statuswert `Skipped (input unavailable)` (08 AI-020, Nachtrag 26.11.2026; AI-032, Nachtrag „Quelle der
//! Eingabefelder“, Punkt 3), additiv wie `Error: Output limit reached`.
use os_ai::AiStatus;
use proptest::prelude::*;

/// Die Statuswerte der Tabelle AI-020 einschließlich der Nachträge (ohne die parametrisierten Fehlerwerte).
const TABLE: [&str; 24] = [
    "Success",
    "Success (cached)",
    "Refusal",
    "Pending",
    "Pending (batch)",
    "Pending (confirmation)",
    "Pending (consent)",
    "Skipped (scope)",
    "Skipped (segment)",
    "Skipped (empty input)",
    "Skipped (policy)",
    "Skipped (no capable model)",
    "Skipped (image too large)",
    "Skipped (unsupported format)",
    "Skipped (no declared language)",
    "Skipped (input unavailable)",
    "Budget exceeded",
    "Quota exceeded",
    "Error: Missing API key",
    "Error: Invalid output",
    "Error: Missing answer",
    "Error: Batch expired",
    "Error: Connection failed",
    "Error: Output limit reached",
];

// spec: AI-020 [M4]
// Kanonischer Text, Serde-Name, Rundreise; übersprungen, also weder im Filter „Errors“ noch in „Fehlgeschlagene erneut
// anfragen“.
#[test]
fn input_unavailable_is_a_skipped_value_with_its_canonical_text() {
    let status = AiStatus::SkippedInputUnavailable;
    assert_eq!(status.to_string(), "Skipped (input unavailable)");
    assert_eq!(
        serde_json::to_value(&status).unwrap(),
        serde_json::Value::String("Skipped (input unavailable)".into())
    );
    assert_eq!(
        serde_json::from_str::<AiStatus>("\"Skipped (input unavailable)\"").unwrap(),
        status
    );
    assert!(!status.is_error());
}

proptest! {
    // spec: AI-020 [M4]
    // Property-Test „nur Werte aus der Tabelle“: jeder Statuswert (auch der neue) ist ein Tabellenwert bzw. ein
    // parametrisierter Fehlerwert, und genau `Error: …`, `Quota exceeded`, `Budget exceeded` sind Fehler.
    #[test]
    fn every_status_is_a_value_of_the_table(index in 0usize..26, seconds in 0u64..3601, code in 400u16..600) {
        let mut statuses: Vec<AiStatus> = TABLE.iter().map(|t| serde_json::from_value(serde_json::Value::String((*t).into())).unwrap()).collect();
        statuses.push(AiStatus::Timeout { seconds });
        statuses.push(AiStatus::HttpError { code, message: "fixture".into() });
        let status = &statuses[index];
        let text = status.to_string();
        let in_table = TABLE.contains(&text.as_str())
            || text.starts_with("Error: Timeout nach ")
            || text.split(' ').nth(1).is_some_and(|c| c.parse::<u16>().is_ok());
        prop_assert!(in_table, "{}", text);
        prop_assert_eq!(status.is_error(), text.starts_with("Error: ") || text == "Quota exceeded" || text == "Budget exceeded");
    }
}

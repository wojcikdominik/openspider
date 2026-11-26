//! Unabhängige Abnahmetests M4-K10: Spalte `Stored by Provider` der Datenflussübersicht und Feld `storage` in
//! `provider-policies.json` (AI-018, Nachtrag 24.11.2026, Q00-H2).
//!
//! Für OpenAI Responses fest `no (store: false)`; für alle anderen Endpunkte der Text aus `storage` je Anbieter und
//! Endpunkt (mit Quelle und Prüfdatum); fehlt das Feld, `unknown`; keine Zeile ist leer; der CSV-Export enthält die
//! Spalte; für jede Zeile mit `no (store: false)` enthält jeder Request im Mock-Log `"store": false`. Die Anzeige im
//! Dialog baut M4-U01.
// acceptance: independent

mod acceptance_k10_support;

use acceptance_k10_support::*;
use os_ai::policy::{
    ConsentBook, ConsentGate, DataFlowOverview, DataFlowRow, FieldSet, InputField,
    ProviderPolicies, TaskDataFlow, COLUMNS,
};
use os_ai::provider::ReasoningLevel;
use os_ai::schema::{RepairAttempts, ResponseSchema};
use serde_json::Value;

const STORE_FALSE: &str = "no (store: false)";

fn flow(
    task: &str,
    provider: &str,
    model: &str,
    endpoint: &str,
    api: Option<&str>,
) -> TaskDataFlow {
    TaskDataFlow {
        task: task.into(),
        provider: provider.into(),
        model: model.into(),
        endpoint: endpoint.into(),
        region: None,
        fields: FieldSet::new([InputField::Url, InputField::Title, InputField::H1]),
        max_chars: None,
        api: api.map(str::to_string),
    }
}

fn flows() -> Vec<TaskDataFlow> {
    vec![
        flow(
            "suggest-title",
            "openai",
            "gpt-6-luna",
            "https://api.openai.com/v1",
            Some("responses"),
        ),
        flow(
            "decision",
            "openai",
            "gpt-6-luna",
            "https://api.openai.com/v1",
            Some("decisions"),
        ),
        flow(
            "embedding",
            "openai",
            "text-embedding-3-small",
            "https://api.openai.com/v1",
            Some("embeddings"),
        ),
        flow(
            "ask-sql",
            "anthropic",
            "claude-sonnet-5-5",
            "https://api.anthropic.com",
            Some("messages"),
        ),
        flow(
            "decision",
            "openrouter",
            "openai/gpt-6-luna",
            "https://openrouter.ai/api/v1",
            Some("chat-completions"),
        ),
        flow(
            "suggest-meta",
            "ollama",
            "mock-llama",
            "http://localhost:11434",
            Some("chat-completions"),
        ),
        flow(
            "suggest-h1",
            "gemini",
            "gemini-x",
            "https://generativelanguage.googleapis.com",
            Some("chat-completions"),
        ),
    ]
}

fn cell(row: &DataFlowRow) -> String {
    let i = COLUMNS
        .iter()
        .position(|c| *c == "Stored by Provider")
        .expect("Spalte vorhanden");
    row.cells()[i].clone()
}

// spec: AI-018 [M4]
// acceptance: independent
// Kriterium: Datenflusstabelle für eine Aufgabe über OpenAI Responses → `Stored by Provider = no (store: false)`; für die
// Decisions API mit Testrichtlinie ohne `storage` → `unknown`; keine Zeile ist leer; der CSV-Export enthält die Spalte.
// Andere Endpunkte zeigen den Text aus `storage`; die Spalte steht nach `Training Use (per provider policy)`.
#[test]
fn stored_by_provider_column() {
    let i = COLUMNS
        .iter()
        .position(|c| *c == "Stored by Provider")
        .unwrap();
    assert_eq!(COLUMNS[i - 1], "Training Use (per provider policy)");

    let policies = ProviderPolicies::from_json(&fixture_text("provider-policies-storage.json"))
        .expect("Testrichtlinie gültig");
    let gate = ConsentGate::dialog(ConsentBook::new());
    let overview = DataFlowOverview::build(&flows(), &policies, &gate, true);
    let cells: Vec<String> = overview.rows.iter().map(cell).collect();
    assert_eq!(
        cells[0], STORE_FALSE,
        "OpenAI Responses fest, auch wenn die Datei etwas anderes sagt"
    );
    assert_eq!(cells[1], "unknown", "Decisions ohne storage");
    assert!(
        cells[2].contains("K10-TEST openai embeddings"),
        "{}",
        cells[2]
    );
    assert!(
        cells[3].contains("K10-TEST anthropic messages"),
        "{}",
        cells[3]
    );
    assert!(cells[4].contains("K10-TEST openrouter"), "{}", cells[4]);
    assert_eq!(cells[5], "unknown", "Ollama ohne storage");
    assert_eq!(cells[6], "unknown", "Anbieter ohne storage");
    for (c, row) in cells.iter().zip(&overview.rows) {
        assert!(!c.trim().is_empty(), "leere Zelle: {}", row.task);
    }
    // Quelle und Prüfdatum stehen beim Text.
    assert!(
        cells[3].contains("https://example.test/anthropic-retention"),
        "{}",
        cells[3]
    );
    assert!(cells[3].contains("2026-11-26"), "{}", cells[3]);

    let csv = overview.to_csv();
    let header = csv.lines().next().unwrap();
    assert!(header.contains("Stored by Provider"), "{header}");
    assert!(csv.contains(STORE_FALSE), "{csv}");
    assert!(csv.contains("unknown"));
    assert_eq!(csv.lines().count(), 1 + overview.rows.len());
}

// spec: AI-018 [M4]
// acceptance: independent
// Mit der ausgelieferten Datei: OpenAI Responses `no (store: false)` (auch über die EU-Basis-URL), alle übrigen
// Endpunkte Text oder `unknown`, nie leer.
#[test]
fn delivered_policies_never_leave_the_cell_empty() {
    let policies = ProviderPolicies::embedded();
    let mut all = flows();
    all.push(flow(
        "suggest-title",
        "openai",
        "gpt-6-luna",
        "https://eu.api.openai.com/v1",
        Some("responses"),
    ));
    for f in &all {
        let row = DataFlowRow::new(f, &policies, false);
        let c = cell(&row);
        assert!(!c.trim().is_empty(), "{} {}", f.provider, f.task);
        if f.provider == "openai" && f.api.as_deref() == Some("responses") {
            assert_eq!(c, STORE_FALSE);
        } else {
            assert_ne!(c, STORE_FALSE, "{} {:?}", f.provider, f.api);
        }
    }
    let raw: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("resources/ai/provider-policies.json")).unwrap(),
    )
    .unwrap();
    for (id, p) in raw["providers"].as_object().unwrap() {
        if let Some(storage) = p.get("storage").and_then(Value::as_object) {
            for (endpoint, note) in storage {
                for field in ["text", "source_url", "verified_at"] {
                    assert!(note.get(field).is_some(), "{id}.storage.{endpoint}.{field}");
                }
            }
        }
    }
}

// spec: AI-018 [M4]
// acceptance: independent
// Negativfälle: ein `storage`-Eintrag ohne Quelle, ohne lesbares Prüfdatum, mit leerem Text, als bloßer Text ohne Quelle
// oder für einen unbekannten Endpunkt macht die Datei ungültig.
#[test]
fn invalid_storage_entries_are_rejected() {
    for name in [
        "provider-policies-invalid-missing-source.json",
        "provider-policies-invalid-date.json",
        "provider-policies-invalid-empty-text.json",
        "provider-policies-invalid-string.json",
        "provider-policies-invalid-endpoint.json",
    ] {
        assert!(
            ProviderPolicies::from_json(&fixture_text(name)).is_err(),
            "{name} müsste abgelehnt werden"
        );
    }
    assert!(ProviderPolicies::from_json(&fixture_text("provider-policies-storage.json")).is_ok());
}

// spec: AI-018 [M4]
// spec: AI-003 [M4]
// acceptance: independent
// Kriterium: Für jede Zeile mit `no (store: false)` enthält jeder Request dieser Aufgabe im Mock-Log `"store": false` –
// auch Reparaturversuch und Abbruch an der Ausgabegrenze.
#[tokio::test]
async fn every_responses_request_of_the_task_sends_store_false() {
    let policies = ProviderPolicies::embedded();
    let f = flow(
        "suggest-title",
        "openai",
        "gpt-6-luna",
        "https://api.openai.com/v1",
        Some("responses"),
    );
    assert_eq!(cell(&DataFlowRow::new(&f, &policies, true)), STORE_FALSE);

    let mock = Mock::start().await;
    let adapter = mock.responses();
    let limiter = fast_limiter();
    let schema = ResponseSchema::new(ok_schema()).unwrap();
    let mk = |text: &str, schema: bool| {
        request(
            model("openai", "gpt-6-luna", "responses", luna_levels()),
            text,
            schema.then(ok_schema),
            ReasoningLevel::Off,
            Some(512),
        )
    };
    adapter
        .send(&mk("K10 store plain", false), None, None)
        .await
        .unwrap();
    adapter
        .send(
            &mk("K10 store limit MOCK-SCENARIO:output-limit", false),
            None,
            None,
        )
        .await
        .unwrap();
    mock.scenario("openai", "malformed-json", 1).await;
    let run = adapter
        .run_structured(
            &limiter,
            &mk("K10 store repair", true),
            &schema,
            RepairAttempts::default(),
            10,
        )
        .await;
    assert_eq!(run.requests.len(), 2, "Reparaturversuch gesendet");
    let bodies = mock.bodies(RESPONSES);
    assert_eq!(bodies.len(), 4);
    for body in bodies {
        assert_eq!(body.get("store"), Some(&Value::Bool(false)), "{body}");
    }
}

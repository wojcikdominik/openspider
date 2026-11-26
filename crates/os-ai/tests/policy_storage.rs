//! M4-K10: Feld `storage` je Anbieter und Endpunkt in `provider-policies.json` und Spalte `Stored by Provider` der
//! Datenflussübersicht (AI-018, Nachtrag 24.11.2026, Q00-H2). Die Anzeige baut M4-U01.
#[path = "cost_q00_support.rs"]
mod support;

use os_ai::adapters::openai_responses::OpenAiResponses;
use os_ai::policy::{
    ConsentBook, ConsentGate, DataFlowOverview, FieldSet, ProviderPolicies, TaskDataFlow, COLUMNS,
    STORED_OPENAI_RESPONSES, STORED_UNKNOWN,
};
use os_ai::provider::ReasoningLevel;
use support::{fixture, limiter, model, ok_schema, request, Mock, RESPONSES};

fn flow(task: &str, provider: &str, api: Option<&str>) -> TaskDataFlow {
    TaskDataFlow {
        task: task.into(),
        provider: provider.into(),
        model: "m".into(),
        endpoint: "https://api.example.test/v1".into(),
        region: None,
        fields: FieldSet::default(),
        max_chars: None,
        api: api.map(str::to_string),
    }
}

fn test_policies() -> ProviderPolicies {
    ProviderPolicies::load(&fixture(&["k10", "provider-policies-storage.json"])).unwrap()
}

fn column(name: &str) -> usize {
    COLUMNS.iter().position(|c| *c == name).unwrap()
}

// spec: AI-018 [M4]
// Spalte `Stored by Provider` nach `Training Use (per provider policy)`; OpenAI Responses → `no (store: false)`; Decisions
// API mit Testrichtlinie ohne `storage` → `unknown`; Eintrag `storage` → Text mit Quelle und Prüfdatum; keine Zeile
// leer; der CSV-Export enthält die Spalte.
#[test]
fn stored_by_provider_column_and_csv() {
    let stored = column("Stored by Provider");
    assert_eq!(COLUMNS[stored - 1], "Training Use (per provider policy)");
    let gate = ConsentGate::dialog(ConsentBook::new());
    let flows = [
        flow("suggest-title", "openai", Some("responses")),
        flow("decision", "openai", Some("decisions")),
        flow("suggest-title", "anthropic", Some("messages")),
        flow("decision", "openrouter", Some("chat-completions")),
        flow("ask-sql", "openai", None),
    ];
    for policies in [test_policies(), ProviderPolicies::embedded()] {
        let overview = DataFlowOverview::build(&flows, &policies, &gate, true);
        let cells: Vec<String> = overview
            .rows
            .iter()
            .map(|r| r.cells()[stored].clone())
            .collect();
        assert_eq!(cells[0], STORED_OPENAI_RESPONSES);
        assert_eq!(cells[0], "no (store: false)");
        assert_eq!(cells[1], STORED_UNKNOWN);
        assert_eq!(cells[3], "unknown");
        assert_eq!(cells[4], "unknown", "Endpunkt unbekannt");
        assert!(cells.iter().all(|c| !c.is_empty()), "keine Zeile leer");
        let csv = overview.to_csv();
        let header = csv.lines().next().unwrap();
        assert!(header
            .contains("Training Use (per provider policy),Stored by Provider,Consent Given At"));
        assert!(csv.contains("no (store: false)"));
    }
    let overview = DataFlowOverview::build(&flows, &test_policies(), &gate, true);
    assert_eq!(
        overview.rows[2].cells()[stored],
        "TEST-STORAGE-MESSAGES (source: https://policy.mock.test/anthropic/storage; verified 2026-11-26)"
    );
    // Für OpenAI Responses gilt der feste Wert, auch wenn die Datei etwas anderes nennt (keine Einstellung).
    assert!(!overview.to_csv().contains("TEST-IGNORED-FOR-RESPONSES"));
}

// spec: AI-018 [M4]
// Das Feld `storage` wird geprüft: nur bekannte Endpunkte, Text, https-Quelle und Prüfdatum; die ausgelieferte Datei
// lädt weiterhin.
#[test]
fn storage_field_is_validated() {
    assert!(
        ProviderPolicies::load(&fixture(&["k10", "provider-policies-storage-invalid.json"]))
            .is_err()
    );
    let policies = test_policies();
    let anthropic = policies.get("anthropic").unwrap();
    assert_eq!(anthropic.storage["messages"].verified_at, "2026-11-26");
    let embedded = ProviderPolicies::embedded();
    assert!(embedded.get("openai").is_some());
}

// spec: AI-018 [M4]
// spec: AI-003 [M4]
// Für jede Zeile mit `no (store: false)` enthält jeder Request dieser Aufgabe im Mock-Log `"store": false` (auch ein
// Reparaturversuch).
#[tokio::test]
async fn rows_with_store_false_send_store_false() {
    let mock = Mock::start().await;
    let adapter = OpenAiResponses::new(mock.openai_http());
    let row = &DataFlowOverview::build(
        &[flow("suggest-title", "openai", Some("responses"))],
        &ProviderPolicies::embedded(),
        &ConsentGate::dialog(ConsentBook::new()),
        true,
    )
    .rows[0];
    assert_eq!(row.stored_by_provider, "no (store: false)");
    let luna = model("openai", "gpt-6-luna", "responses", Vec::new());
    mock.global_scenario("openai", "malformed-json", 1).await;
    let schema = os_ai::schema::ResponseSchema::new(ok_schema()).unwrap();
    adapter
        .run_structured(
            &limiter(),
            &request(
                luna.clone(),
                "MOCK-ID:home a",
                Some(ok_schema()),
                ReasoningLevel::Off,
                1_024,
            ),
            &schema,
            os_ai::schema::RepairAttempts::default(),
            10,
        )
        .await;
    let _ = adapter
        .run(
            &limiter(),
            &request(luna, "MOCK-ID:home b", None, ReasoningLevel::Off, 1_024),
            10,
        )
        .await;
    let bodies = mock.bodies(RESPONSES);
    assert!(bodies.len() >= 3, "Reparaturversuch und Freitext");
    for body in bodies {
        assert_eq!(body["store"], serde_json::Value::Bool(false), "{body}");
    }
}

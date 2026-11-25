//! AI-018 Datenflussübersicht und Einwilligung – Einwilligungslogik, Übersicht, CSV, Anbieterrichtlinien.
//! Die Prüfungen mit Mock-Zähler stehen in `policy_mock.rs`.
use os_ai::{
    policy::{
        consent::rfc3339_utc,
        dataflow::{host_of, COLUMNS},
        ConsentBook, ConsentCheck, ConsentGate, ConsentKey, ConsentMode, ConsentStore,
        DataFlowOverview, FieldSet, InputField, MemoryConsentStore, ProviderPolicies, TaskDataFlow,
        TrainingUse, CLI_CONSENT_MISSING,
    },
    AiStatus,
};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(name)
}

fn flows() -> Vec<TaskDataFlow> {
    let v: Value = serde_json::from_str(
        &std::fs::read_to_string(path("tests/fixtures/ai/policy/tasks.json")).unwrap(),
    )
    .unwrap();
    serde_json::from_value(v["flows"].clone()).unwrap()
}

fn flow(task: &str) -> TaskDataFlow {
    flows().into_iter().find(|f| f.task == task).unwrap()
}

const T0: i64 = 1_796_083_200_000; // 2026-12-01T00:00:00Z

// spec: AI-018
#[test]
fn without_consent_status_is_pending_consent() {
    let gate = ConsentGate::dialog(ConsentBook::new());
    let check = gate.check(&flow("suggest-title").consent_key());
    assert!(!check.is_granted());
    assert_eq!(check.status(), Some(AiStatus::PendingConsent));
    assert_eq!(AiStatus::PendingConsent.to_string(), "Pending (consent)");
}

// spec: AI-018
#[test]
fn consent_is_per_provider_task_and_field_set() {
    let gate = ConsentGate::dialog(ConsentBook::new());
    let title = flow("suggest-title");
    gate.grant(title.consent_key(), T0, None).unwrap();
    assert!(gate.check(&title.consent_key()).is_granted());
    // other task, same provider
    assert!(!gate.check(&flow("page-type").consent_key()).is_granted());
    // same task, other provider
    let other = ConsentKey::new("anthropic", "suggest-title", title.fields.clone());
    assert!(!gate.check(&other).is_granted());
    // subset of the consented fields: no new data, still covered (interpretation, see report)
    let subset = ConsentKey::new(
        "openai",
        "suggest-title",
        FieldSet::new([InputField::Title]),
    );
    assert!(gate.check(&subset).is_granted());
}

// spec: AI-018
#[test]
fn added_field_asks_again_and_names_the_new_field() {
    let gate = ConsentGate::dialog(ConsentBook::new());
    let mut title = flow("suggest-title");
    gate.grant(title.consent_key(), T0, None).unwrap();
    title.fields.insert(InputField::PageText);
    match gate.check(&title.consent_key()) {
        ConsentCheck::Required { previous, added } => {
            assert_eq!(previous, Some(flow("suggest-title").fields));
            assert_eq!(added, FieldSet::new([InputField::PageText]));
        }
        other => panic!("expected renewed consent, got {other:?}"),
    }
    let policies = ProviderPolicies::embedded();
    let requests =
        DataFlowOverview::consent_requests(std::slice::from_ref(&title), &policies, &gate, true);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].added_fields,
        FieldSet::new([InputField::PageText])
    );
    gate.grant(title.consent_key(), T0 + 1000, None).unwrap();
    assert!(gate.check(&title.consent_key()).is_granted());
    assert!(DataFlowOverview::consent_requests(&[title], &policies, &gate, true).is_empty());
}

// spec: AI-018
#[test]
fn revocation_per_provider_removes_only_that_provider() {
    let gate = ConsentGate::dialog(ConsentBook::new());
    for f in flows() {
        gate.grant(f.consent_key(), T0, None).unwrap();
    }
    let removed = gate.revoke_provider("openai").unwrap();
    assert_eq!(removed.len(), 2);
    for f in flows() {
        assert_eq!(
            gate.check(&f.consent_key()).is_granted(),
            f.provider != "openai",
            "{}",
            f.task
        );
    }
    // a clone shares the state (workers see the revocation before their next request)
    let worker = gate.clone();
    gate.revoke_provider("anthropic").unwrap();
    assert!(!worker
        .check(&flow("search-intent").consent_key())
        .is_granted());
}

// spec: AI-018
#[test]
fn cli_switch_replaces_the_dialog() {
    let key = flow("page-type").consent_key();
    let without = ConsentGate::cli(false);
    assert_eq!(without.mode(), ConsentMode::Cli { flag: false });
    assert!(!without.check(&key).is_granted());
    assert_eq!(without.cli_message(), Some(CLI_CONSENT_MISSING));
    assert!(CLI_CONSENT_MISSING.contains("--ai-consent"));
    let with = ConsentGate::cli(true);
    assert!(with.check(&key).is_granted());
    assert_eq!(with.cli_message(), None);
    assert_eq!(ConsentGate::dialog(ConsentBook::new()).cli_message(), None);
}

// spec: AI-018
#[test]
fn consent_is_logged_with_shown_row_and_persisted() {
    let store = Arc::new(MemoryConsentStore::default());
    let gate = ConsentGate::with_store(store.clone()).unwrap();
    let policies = ProviderPolicies::embedded();
    let f = flow("page-type");
    let overview = DataFlowOverview::build(std::slice::from_ref(&f), &policies, &gate, true);
    gate.grant(f.consent_key(), T0, Some(overview.rows[0].clone()))
        .unwrap();
    assert_eq!(store.saves(), 1);
    let book = store.load().unwrap();
    let record = &book.records()[0];
    assert_eq!(record.key, f.consent_key());
    assert_eq!(record.given_at(), "2026-12-01T00:00:00Z");
    let shown = record.shown.as_ref().unwrap();
    assert_eq!(shown.host, "openai.mock.test");
    assert_eq!(shown.fields_sent, f.fields);
    // JSON round trip and reload
    let restored = ConsentBook::from_json(&book.to_json()).unwrap();
    assert_eq!(restored, book);
    let reopened = ConsentGate::with_store(store.clone()).unwrap();
    assert!(reopened.check(&f.consent_key()).is_granted());
    reopened.revoke_provider("openai").unwrap();
    assert_eq!(store.saves(), 2);
    assert!(store.load().unwrap().records().is_empty());
}

// spec: AI-018
#[test]
fn overview_has_all_columns_and_values() {
    assert_eq!(
        COLUMNS,
        [
            "Task",
            "Provider",
            "Model",
            "Host",
            "Region",
            "Fields Sent",
            "Max Chars",
            "Images",
            "Redaction",
            "Training Use (per provider policy)",
            "Stored by Provider",
            "Consent Given At"
        ]
    );
    let gate = ConsentGate::dialog(ConsentBook::new());
    gate.grant(flow("page-type").consent_key(), T0, None)
        .unwrap();
    let overview = DataFlowOverview::build(&flows(), &ProviderPolicies::embedded(), &gate, true);
    assert_eq!(overview.rows.len(), 5);
    let page = &overview.rows[1].cells();
    assert_eq!(page[0], "page-type");
    assert_eq!(page[1], "openai");
    assert_eq!(page[2], "gpt-6-luna");
    assert_eq!(page[3], "openai.mock.test");
    assert_eq!(page[4], "");
    assert_eq!(
        page[5],
        "URL, Title, H1, H2, Page Text, Structured Data Types"
    );
    assert_eq!(page[6], "3000");
    assert_eq!(page[7], "no");
    assert_eq!(page[8], "on");
    assert!(page[9].starts_with("no (source: https://"), "{}", page[9]);
    assert!(page[9].contains("verified 2026-11-24"));
    assert_eq!(page[11], "2026-12-01T00:00:00Z");
    let alt = &overview.rows[3].cells();
    assert_eq!(alt[7], "yes");
    assert!(alt[9].starts_with("depends"));
    assert_eq!(overview.rows[0].cells()[11], "", "no consent yet");
    assert_eq!(overview.rows[4].cells()[3], "localhost:11434");
}

// spec: AI-018
#[test]
fn redaction_state_and_pii_notice() {
    let gate = ConsentGate::dialog(ConsentBook::new());
    let policies = ProviderPolicies::embedded();
    let page = flow("page-type"); // contains page_text and h2
    let title = flow("suggest-title"); // no page_text, h2, image_context
    let alt = flow("alt-text-describes-image"); // image_context
    let on =
        DataFlowOverview::consent_requests(std::slice::from_ref(&page), &policies, &gate, true);
    assert_eq!(on[0].row.redaction_cell(), "on");
    assert!(!on[0].pii_notice);
    let off = DataFlowOverview::consent_requests(&[page, title, alt], &policies, &gate, false);
    assert_eq!(off[0].row.redaction_cell(), "off");
    assert!(off[0].pii_notice);
    assert!(
        !off[1].pii_notice,
        "Redaction off without page_text/h2/image_context"
    );
    assert!(off[2].pii_notice);
}

// spec: AI-018
#[test]
fn csv_export_quotes_and_lists_rows() {
    let gate = ConsentGate::dialog(ConsentBook::new());
    let overview = DataFlowOverview::build(&flows(), &ProviderPolicies::embedded(), &gate, false);
    let csv = overview.to_csv();
    let lines: Vec<&str> = csv.split("\r\n").filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[0], COLUMNS.join(","));
    assert!(lines[1].starts_with(
        "suggest-title,openai,gpt-6-luna,openai.mock.test,,\"URL, Title, Meta Description, H1\",,no,off,"
    ), "{}", lines[1]);
}

// spec: AI-018
#[test]
fn provider_policies_have_source_and_check_date() {
    let embedded = ProviderPolicies::embedded();
    let file = ProviderPolicies::load(&path("resources/ai/provider-policies.json")).unwrap();
    assert_eq!(embedded, file);
    for provider in ["anthropic", "openai", "openrouter", "gemini", "ollama"] {
        let p = embedded
            .get(provider)
            .unwrap_or_else(|| panic!("{provider} missing"));
        assert_ne!(p.training_use, TrainingUse::Unknown);
        assert_eq!(p.verified_at.len(), 10);
        assert!(
            p.source_url.as_deref().unwrap().starts_with("https://"),
            "{provider}: source is required for every provider"
        );
        assert!(p.cell().contains(p.source_url.as_deref().unwrap()));
    }
    assert_eq!(
        embedded.get("ollama").unwrap().source_url.as_deref(),
        Some("https://docs.ollama.com/faq")
    );
    // an entry without source is rejected
    let without_source = r#"{"format_version":1,"providers":{"x":{"training_use":"no","summary":"s","source_url":null,"verified_at":"2026-11-24"}}}"#;
    assert!(ProviderPolicies::from_json(without_source).is_err());
    let unknown = embedded.training_use("someone-else");
    assert_eq!(unknown.training_use, TrainingUse::Unknown);
    assert_eq!(unknown.cell(), "unknown");
    let bad = std::fs::read_to_string(path(
        "tests/fixtures/ai/policy/provider-policies-invalid.json",
    ))
    .unwrap();
    assert!(ProviderPolicies::from_json(&bad).is_err());
}

// spec: AI-018
#[test]
fn time_and_host_helpers() {
    assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(rfc3339_utc(1_796_085_045_000), "2026-12-01T00:30:45Z");
    assert_eq!(rfc3339_utc(951_782_400_000), "2000-02-29T00:00:00Z");
    assert_eq!(
        host_of("https://eu.openai.mock.test/v1"),
        "eu.openai.mock.test"
    );
    assert_eq!(host_of("http://127.0.0.1:8080/openai"), "127.0.0.1:8080");
}

//! M4-U01: Datenflussübersicht und Einwilligung in der Service-Schicht (AI-018 mit Nachtrag 24.11.2026 `Stored by Provider`,
//! Abnahme K10: `TaskDataFlow::api`), Speicherung je Installation (Nutzerentscheidung 24.11.2026), Region über den
//! EU-Endpunkt (AI-019). Fixtures: Profil mit einer aktiven Frage (`inputs` nach AI-030/AI-032), eine angemeldete Feldquelle
//! für `suggest-title`, eingebettete `provider-policies.json` bzw. die Testrichtlinie `tests/fixtures/ai/k10/…`.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use os_ai::catalog::{PriceTable, TaskDefaults, TaskType};
use os_ai::policy::{ConsentBook, ConsentGate, FieldSet, ProviderPolicies};
use os_app::commands::ai_consent::{
    consent_view, endpoint_of, register_flow_source, task_flows, TaskFields, CONSENT_FILE,
};
use os_app::commands::ai_settings::ModelLists;
use os_app::{App, AppConfig, CallCtx, ErrorCode, WindowId};
use os_model::config::{AiAppSettings, AiPolicy};
use os_model::CrawlConfig;
use serde_json::{json, Value};

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn ctx() -> CallCtx {
    CallCtx {
        window: WindowId("main".into()),
        cancel: Default::default(),
    }
}

fn app(home: &Path) -> App {
    App::new(AppConfig::new(home)).expect("App startet")
}

fn question() -> Value {
    json!({
        "id": "page-type", "version": "1.0.0", "name": "Page Type", "type": "predicate",
        "instructions": "Is this a product page?", "subject": "page", "inputs": ["url", "title", "page_text"]
    })
}

/// Profil mit aktiver Frage (Decision Engine).
fn config_with_question() -> CrawlConfig {
    let mut cfg = CrawlConfig::default();
    cfg.ai.enabled = true;
    cfg.ai.questions = vec![question()];
    cfg.ai.active_questions = vec!["page-type".into()];
    cfg
}

fn title_fields(page_text: bool) -> TaskFields {
    let mut ids = vec!["url", "title", "h1"];
    if page_text {
        ids.push("page_text");
    }
    TaskFields {
        task: TaskType::SuggestTitle,
        fields: FieldSet::from_ids(ids).unwrap(),
        max_chars: page_text.then_some(8000),
    }
}

fn flows(
    cfg: &CrawlConfig,
    app: &AiAppSettings,
    extra: Vec<TaskFields>,
) -> Vec<os_ai::policy::TaskDataFlow> {
    let mut fields = os_app::commands::ai_consent::profile_question_fields(cfg);
    fields.extend(extra);
    let fields = os_app::commands::ai_consent::merge_fields(fields);
    let lists = ModelLists::build(&PriceTable::bundled().unwrap(), &Default::default());
    task_flows(
        cfg,
        app,
        &fields,
        &TaskDefaults::bundled().unwrap(),
        &lists,
        &Default::default(),
    )
    .unwrap()
}

fn cell<'a>(
    view: &'a os_app::commands::ai_consent::ConsentView,
    task: &str,
    column: &str,
) -> &'a str {
    let i = view.columns.iter().position(|c| c == column).unwrap();
    &view.rows.iter().find(|r| r.key.task == task).unwrap().cells[i]
}

// spec: AI-018 [M4]
// Datenflusstabelle: Aufgabe über OpenAI Responses → `Stored by Provider = no (store: false)` (Endpunktart gesetzt,
// Abnahme K10); Decisions API ohne `storage` → `unknown`; keine Zelle der Spalte leer; `Stored by Provider` steht nach
// `Training Use (per provider policy)`; der CSV-Export enthält die Spalte.
#[test]
fn stored_by_provider_follows_the_endpoint_of_each_task() {
    let cfg = config_with_question();
    let f = flows(&cfg, &AiAppSettings::default(), vec![title_fields(false)]);
    let decision = f.iter().find(|x| x.task == "decision").unwrap();
    assert_eq!(
        (decision.provider.as_str(), decision.api.as_deref()),
        ("openai", Some("decisions"))
    );
    let title = f.iter().find(|x| x.task == "suggest-title").unwrap();
    assert_eq!(title.api.as_deref(), Some("responses"));

    let gate = ConsentGate::dialog(ConsentBook::new());
    let view = consent_view(
        &f,
        &ProviderPolicies::embedded(),
        &gate,
        true,
        AiPolicy::Unrestricted,
    );
    let training = view
        .columns
        .iter()
        .position(|c| c == "Training Use (per provider policy)")
        .unwrap();
    assert_eq!(view.columns[training + 1], "Stored by Provider");
    assert_eq!(
        cell(&view, "suggest-title", "Stored by Provider"),
        "no (store: false)"
    );
    assert_eq!(cell(&view, "decision", "Stored by Provider"), "unknown");
    assert!(view.rows.iter().all(|r| !r.cells[training + 1].is_empty()));
    assert_eq!(cell(&view, "decision", "Host"), "api.openai.com");
    assert_eq!(
        cell(&view, "decision", "Fields Sent"),
        "URL, Title, Page Text"
    );
    assert!(view
        .csv
        .lines()
        .next()
        .unwrap()
        .contains("Stored by Provider"));
    assert!(view.csv.contains("no (store: false)"));

    // Testrichtlinie der Abnahme K10 (Responses mit `storage`, Decisions ohne): Responses bleibt fest `no (store: false)`,
    // Decisions `unknown`.
    let path = repo("tests/fixtures/ai/k10/provider-policies-storage.json");
    let policies = ProviderPolicies::load(&path).expect("Testrichtlinie mit storage");
    let view = consent_view(&f, &policies, &gate, true, AiPolicy::Unrestricted);
    assert_eq!(
        cell(&view, "suggest-title", "Stored by Provider"),
        "no (store: false)"
    );
    assert_eq!(
        cell(&view, "decision", "Stored by Provider"),
        "unknown",
        "Testrichtlinie ohne storage für Decisions"
    );
}

// spec: AI-019
// 1.1: Der Schalter „EU-Endpunkt verwenden“ ist ausgeblendet und wird nicht durchgesetzt – die Übersicht macht keine
// EU-Zusage (Host des Standardendpunkts, Region leer); eine eigene Basis-URL (INTEG-019) gilt weiter.
#[test]
fn no_eu_promise_in_the_overview_in_1_1() {
    let mut s = AiAppSettings::default();
    let mut e = s
        .provider_endpoints
        .get("openai")
        .cloned()
        .unwrap_or_default();
    e.use_eu_endpoint = true;
    e.eu_account_confirmed = true;
    s.provider_endpoints.insert("openai".into(), e.clone());
    assert_eq!(
        endpoint_of("openai", &s),
        ("https://api.openai.com/v1".into(), None)
    );
    let f = flows(&config_with_question(), &s, vec![]);
    let gate = ConsentGate::dialog(ConsentBook::new());
    let view = consent_view(
        &f,
        &ProviderPolicies::embedded(),
        &gate,
        true,
        AiPolicy::Unrestricted,
    );
    assert_eq!(cell(&view, "decision", "Host"), "api.openai.com");
    assert_eq!(cell(&view, "decision", "Region"), "");
    e.base_url = Some("http://openai.mock.test:4545/v1".into());
    s.provider_endpoints.insert("openai".into(), e);
    assert_eq!(
        endpoint_of("openai", &s),
        ("http://openai.mock.test:4545/v1".into(), None)
    );
}

// spec: AI-018 [M4]
// Schwärzung an + `page_text` → `Redaction = on` ohne PII-Hinweis; aus → `off` mit Hinweis; ohne `page_text`/`h2`/
// `image_context` nie ein Hinweis.
#[test]
fn redaction_state_and_pii_notice_in_the_dialog() {
    let cfg = config_with_question();
    let f = flows(&cfg, &AiAppSettings::default(), vec![title_fields(false)]);
    let gate = ConsentGate::dialog(ConsentBook::new());
    let on = consent_view(
        &f,
        &ProviderPolicies::embedded(),
        &gate,
        true,
        AiPolicy::Unrestricted,
    );
    assert!(on.requests.iter().all(|r| !r.pii_notice));
    assert_eq!(cell(&on, "decision", "Redaction"), "on");
    let off = consent_view(
        &f,
        &ProviderPolicies::embedded(),
        &gate,
        false,
        AiPolicy::Unrestricted,
    );
    let decision = off
        .requests
        .iter()
        .find(|r| r.row.key.task == "decision")
        .unwrap();
    assert!(decision.pii_notice, "page_text bei Schwärzung aus");
    let title = off
        .requests
        .iter()
        .find(|r| r.row.key.task == "suggest-title")
        .unwrap();
    assert!(!title.pii_notice, "Title-Vorschläge ohne Seitentext");
    assert_eq!(cell(&off, "decision", "Redaction"), "off");
}

// spec: AI-019
// EU-Endpunkt eingeschaltet: Host und Region der Zeile folgen (`eu.api.openai.com`, `EU`); eigene Basis-URL (INTEG-019)
// geht vor; Anthropic hat keinen EU-Weg.
#[test]
#[ignore = "wartet auf M4-K07 (1.2): AI-019 EU-Verarbeitung und Nur lokal werden in 1.1 nicht durchgesetzt"]
fn eu_endpoint_sets_host_and_region() {
    let mut s = AiAppSettings::default();
    let mut e = s
        .provider_endpoints
        .get("openai")
        .cloned()
        .unwrap_or_default();
    e.use_eu_endpoint = true;
    s.provider_endpoints.insert("openai".into(), e.clone());
    assert_eq!(
        endpoint_of("openai", &s),
        ("https://eu.api.openai.com".into(), Some("EU".into()))
    );
    assert_eq!(endpoint_of("anthropic", &s).1, None);
    let cfg = config_with_question();
    let f = flows(&cfg, &s, vec![]);
    let gate = ConsentGate::dialog(ConsentBook::new());
    let view = consent_view(
        &f,
        &ProviderPolicies::embedded(),
        &gate,
        true,
        AiPolicy::EuOnly,
    );
    assert_eq!(cell(&view, "decision", "Host"), "eu.api.openai.com");
    assert_eq!(cell(&view, "decision", "Region"), "EU");
    assert_eq!(view.policy, AiPolicy::EuOnly);
    e.base_url = Some("http://eu.openai.mock.test:4545/v1".into());
    s.provider_endpoints.insert("openai".into(), e);
    assert_eq!(
        endpoint_of("openai", &s).0,
        "http://eu.openai.mock.test:4545/v1"
    );
}

async fn use_question_profile(app: &App) {
    let mut dto = app
        .dispatch(ctx(), "config.get", Value::Null)
        .await
        .unwrap();
    dto["config"]["ai"]["enabled"] = json!(true);
    dto["config"]["ai"]["questions"] = json!([question()]);
    dto["config"]["ai"]["active_questions"] = json!(["page-type"]);
    app.dispatch(ctx(), "config.set", dto)
        .await
        .expect("config.set");
}

// spec: AI-018 [M4]
// Über den Command-Weg: ohne Einwilligung eine offene Anfrage je Aufgabe; „Bestätigen“ speichert je Installation
// (`ai-consent.json` im Einstellungsordner) mit der gezeigten Zeile, gilt nach Neustart weiter; kommt ein Feld hinzu
// (Title-Vorschläge mit Seitentext), wird erneut gefragt und das neue Feld genannt; Widerruf je Anbieter fragt wieder.
#[tokio::test]
async fn grant_persists_per_installation_new_field_asks_again_and_revoke() {
    let home = tempfile::tempdir().unwrap();
    {
        let app = app(home.path());
        register_flow_source(&app, Arc::new(|_| vec![title_fields(false)]));
        use_question_profile(&app).await;
        let v = app
            .dispatch(ctx(), "ai.consentStatus", Value::Null)
            .await
            .unwrap();
        let requests = v["requests"].as_array().unwrap();
        assert_eq!(requests.len(), 2, "{v}");
        let keys: Vec<Value> = requests.iter().map(|r| r["row"]["key"].clone()).collect();
        let after = app
            .dispatch(ctx(), "ai.consentGrant", json!({ "grant": keys }))
            .await
            .unwrap();
        assert_eq!(after["requests"], json!([]));
        assert!(after["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["consentGiven"] == json!(true)));
        assert_eq!(after["consentedProviders"], json!(["openai"]));
    }
    let file = home.path().join(CONSENT_FILE);
    let book: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let records = book["records"].as_array().unwrap();
    assert_eq!(records.len(), 2);
    assert!(records
        .iter()
        .all(|r| r["shown"]["stored_by_provider"].is_string()));

    // Neustart: Einwilligung gilt weiter (je Installation, nicht je Crawl).
    let app = app(home.path());
    register_flow_source(&app, Arc::new(|_| vec![title_fields(true)]));
    use_question_profile(&app).await;
    let v = app
        .dispatch(ctx(), "ai.consentStatus", Value::Null)
        .await
        .unwrap();
    let requests = v["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 1, "nur die Aufgabe mit neuem Feld: {v}");
    assert_eq!(requests[0]["row"]["key"]["task"], json!("suggest-title"));
    assert_eq!(requests[0]["addedFields"], json!(["page_text"]));
    assert!(requests[0]["previousFields"].is_array());

    // Widerruf je Anbieter: danach fragt auch die Decision-Aufgabe wieder.
    let v = app
        .dispatch(
            ctx(),
            "ai.consentGrant",
            json!({ "revokeProvider": "openai" }),
        )
        .await
        .unwrap();
    assert_eq!(v["requests"].as_array().unwrap().len(), 2);
    assert_eq!(v["consentedProviders"], json!([]));
}

// spec: AI-018 [M4]
// Bestätigt wird nur, was der Dialog zeigt: ein Schlüssel ohne passende Zeile ist ein Eingabefehler, nichts gespeichert.
#[tokio::test]
async fn granting_an_unknown_row_is_rejected() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    use_question_profile(&app).await;
    let err = app
        .dispatch(
            ctx(),
            "ai.consentGrant",
            json!({ "grant": [{ "provider": "anthropic", "task": "decision", "fields": ["url"] }] }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(err.message_key, "err-ai-consent-unknown");
    assert!(!home.path().join(CONSENT_FILE).exists());
}

// spec: AI-018
// Das Gate der App ist geteilt: ein Widerruf über den Command ist für jeden Halter des Gates sofort sichtbar (Ausführung
// D01 prüft über dasselbe Gate, `policy::guard::execute`).
#[tokio::test]
async fn the_app_gate_is_shared() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    use_question_profile(&app).await;
    let gate = os_app::commands::ai_consent::consent_gate(&app).unwrap();
    let v = app
        .dispatch(ctx(), "ai.consentStatus", Value::Null)
        .await
        .unwrap();
    let key = v["requests"][0]["row"]["key"].clone();
    app.dispatch(ctx(), "ai.consentGrant", json!({ "grant": [key] }))
        .await
        .unwrap();
    assert_eq!(gate.book().records().len(), 1);
    app.dispatch(
        ctx(),
        "ai.consentGrant",
        json!({ "revokeProvider": "openai" }),
    )
    .await
    .unwrap();
    assert!(gate.book().records().is_empty());
}

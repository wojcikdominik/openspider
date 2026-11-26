//! Unabhängige Abnahme M4-U01, Teil Einwilligung: Datenflussübersicht und Dialog „Datenfluss bestätigen“ (AI-018 mit
//! Nachtrag 24.11.2026: Spalte `Stored by Provider`), Einwilligung **je Installation** (Nutzerentscheidung 24.11.2026,
//! Plan M4.md 7), Widerruf je Anbieter, ohne Einwilligung kein Request (ai-mock zählt 0).
//!
//! Angebunden über `ai.consentStatus`/`ai.consentGrant` und das geteilte Gate der App (`consent_gate`), das der Wächter
//! `os_ai::policy::guard::execute` vor jedem Request prüft (Plan M4.md 7, Nachträge Welle 3 Punkt 4).
// acceptance: independent
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

mod acceptance_u01_support;

#[path = "session_common.rs"]
mod common;

use std::sync::Arc;

use acceptance_u01_support::*;
use os_ai::catalog::TaskType;
use os_ai::policy::guard;
use os_ai::policy::{FieldSet, FieldValues, InputField, PreparedInput, Redactor};
use os_ai::provider::Outcome;
use os_ai::{AiStatus, PreviewSwitch};
use os_app::commands::ai_consent::{consent_gate, register_flow_source, TaskFields, CONSENT_FILE};
use os_app::App;
use serde_json::{json, Value};

/// Spalten der Datenflusstabelle laut AI-018 (Ausgabe) mit `Stored by Provider` nach `Training Use (per provider
/// policy)` (Nachtrag 24.11.2026).
const COLUMNS: [&str; 12] = [
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
    "Consent Given At",
];

fn col(name: &str) -> usize {
    COLUMNS.iter().position(|c| *c == name).unwrap()
}

fn rows_of<'a>(view: &'a Value, key: &str) -> &'a Vec<Value> {
    view[key].as_array().unwrap_or_else(|| panic!("{key} fehlt: {view}"))
}

fn row_for<'a>(view: &'a Value, task: &str) -> &'a Value {
    rows_of(view, "rows")
        .iter()
        .find(|r| r["key"]["task"] == task)
        .unwrap_or_else(|| panic!("keine Zeile für {task}: {view}"))
}

fn cell(row: &Value, name: &str) -> String {
    row["cells"][col(name)].as_str().unwrap_or_default().to_string()
}

fn request_for<'a>(view: &'a Value, task: &str) -> Option<&'a Value> {
    rows_of(view, "requests")
        .iter()
        .find(|r| r["row"]["key"]["task"] == task)
}

async fn status(app: &App) -> Value {
    call(app, "ai.consentStatus", Value::Null).await
}

/// Bestätigt alle offenen Anfragen genau so, wie der Dialog sie zeigt.
async fn confirm_all(app: &App) -> Value {
    let view = status(app).await;
    let keys: Vec<Value> = rows_of(&view, "requests")
        .iter()
        .map(|r| r["row"]["key"].clone())
        .collect();
    assert!(!keys.is_empty(), "keine offene Anfrage: {view}");
    call(app, "ai.consentGrant", json!({ "grant": keys })).await
}

/// Titelvorschläge (`suggest-title`, Standard über OpenAI Responses) mit URL, Title, Meta Description, H1 – angemeldet wie
/// ein Fachpaket der Vorschläge.
fn title_flow(app: &App) {
    register_flow_source(
        app,
        Arc::new(|_cfg| {
            vec![TaskFields {
                task: TaskType::SuggestTitle,
                fields: FieldSet::new([
                    InputField::Url,
                    InputField::Title,
                    InputField::MetaDescription,
                    InputField::H1,
                ]),
                max_chars: None,
            }]
        }),
    );
}

fn page_values() -> FieldValues {
    FieldValues::new()
        .with_text(InputField::Url, "https://ai.test/u01")
        .with_text(InputField::Title, "U01 Titel")
        .with_text(InputField::PageText, "MARK-PAGE-TEXT")
}

/// Ein Request der Aufgabe `task` über den Wächter, an den ai-mock (Anbieter `openai`).
async fn guarded(app: &App, mock: &Mock, task: &str, fields: &[InputField]) -> guard::GuardedOutcome {
    let gate = consent_gate(app).expect("Gate der App");
    let p = mock.provider("openai");
    let set = FieldSet::new(fields.iter().copied());
    let input = PreparedInput::new(&page_values(), &set, &Redactor::default());
    guard::execute(PreviewSwitch::ON, &gate, task, &input, &p, p.request("gpt-6-luna")).await
}

// spec: AI-018 [M4]
// acceptance: independent
// Datenflusstabelle mit den Spalten aus AI-018 in dieser Reihenfolge; für eine Aufgabe über OpenAI Responses
// `Stored by Provider = no (store: false)`, für die Decisions API ohne `storage` in der Richtlinie `unknown`; keine Zeile
// leer in dieser Spalte; der CSV-Export enthält die Spalte.
#[tokio::test]
async fn ai018_tabelle_spalten_und_stored_by_provider() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    preview_on(&app).await;
    title_flow(&app);
    edit_config(&app, |cfg| with_question(cfg, "u01-q", &["url", "title"])).await;
    let view = status(&app).await;
    let columns: Vec<&str> = rows_of(&view, "columns").iter().map(|c| c.as_str().unwrap()).collect();
    assert_eq!(columns, COLUMNS, "Spalten laut AI-018");

    let title = row_for(&view, "suggest-title");
    assert_eq!(cell(title, "Provider"), "openai", "{title}");
    assert_eq!(cell(title, "Model"), "gpt-6-luna");
    assert_eq!(cell(title, "Stored by Provider"), "no (store: false)", "{title}");
    assert_eq!(cell(title, "Redaction"), "on", "Schwärzung ist Standard an");
    assert!(!cell(title, "Host").is_empty());
    assert!(!cell(title, "Training Use (per provider policy)").is_empty());

    let policies = repo_json("resources/ai/provider-policies.json");
    let decisions_storage = &policies["providers"]["openai"]["storage"]["decisions"];
    let decision = row_for(&view, "decision");
    let stored = cell(decision, "Stored by Provider");
    if decisions_storage.is_null() {
        assert_eq!(stored, "unknown", "Decisions API ohne `storage` → unknown: {decision}");
    } else {
        assert!(!stored.is_empty() && stored != "unknown", "{decision}");
    }
    for r in rows_of(&view, "rows") {
        assert!(!cell(r, "Stored by Provider").is_empty(), "leere Zelle: {r}");
    }
    let csv = view["csv"].as_str().unwrap();
    assert!(csv.lines().next().unwrap().contains("Stored by Provider"), "{csv}");
}

// spec: AI-018 [M4]
// acceptance: independent
// Ohne Bestätigung wird für die Aufgabe kein Request gesendet (`Pending (consent)`, Mock-Zähler 0). Nach der Bestätigung
// der gezeigten Zeile geht der Request hinaus; die Zeile trägt `Consent Given At`.
#[tokio::test]
async fn ai018_ohne_einwilligung_kein_request_danach_genau_einer() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    preview_on(&app).await;
    title_flow(&app);
    let mock = Mock::start().await;
    let fields = [InputField::Url, InputField::Title, InputField::MetaDescription, InputField::H1];

    let view = status(&app).await;
    assert!(request_for(&view, "suggest-title").is_some(), "Dialog verlangt Bestätigung: {view}");
    for _ in 0..3 {
        let out = guarded(&app, &mock, "suggest-title", &fields).await;
        assert_eq!(out.outcome, Outcome::Status(AiStatus::PendingConsent));
        assert!(!out.sent);
    }
    assert_eq!(mock.content_requests(), 0, "Mock-Zähler ohne Einwilligung");

    let after = confirm_all(&app).await;
    assert!(request_for(&after, "suggest-title").is_none(), "{after}");
    let r = row_for(&after, "suggest-title");
    assert_eq!(r["consentGiven"], true);
    assert!(!cell(r, "Consent Given At").is_empty(), "{r}");
    let out = guarded(&app, &mock, "suggest-title", &fields).await;
    assert!(out.sent, "{:?}", out.outcome);
    assert_eq!(mock.content_requests(), 1);
}

// spec: AI-018 [M4]
// acceptance: independent
// Die Einwilligung gilt je Installation: Sie liegt in den App-Einstellungen, überlebt einen Neustart der App und gilt
// für einen zweiten Crawl mit derselben Aufgabe und Feldmenge, ohne erneut zu fragen.
#[tokio::test]
async fn ai018_einwilligung_je_installation_ueberlebt_neustart_und_zweiten_crawl() {
    let home = tempfile::tempdir().unwrap();
    {
        let app = fresh_app(home.path());
        preview_on(&app).await;
        edit_config(&app, |cfg| with_question(cfg, "u01-q", &["url", "title"])).await;
        confirm_all(&app).await;
    }
    let settings_dir = os_app::AppConfig::new(home.path()).paths.settings;
    assert!(settings_dir.join(CONSENT_FILE).is_file(), "Einwilligung in den App-Einstellungen");

    // Neustart: neue App auf demselben Einstellungsordner.
    let app = fresh_app(home.path());
    edit_config(&app, |cfg| with_question(cfg, "u01-q", &["url", "title"])).await;
    let view = status(&app).await;
    assert!(rows_of(&view, "requests").is_empty(), "nach Neustart keine neue Frage: {view}");
    assert_eq!(row_for(&view, "decision")["consentGiven"], true);

    // Zweiter Crawl mit derselben Konfiguration (geöffnete Crawl-Datei) → ebenfalls keine neue Frage.
    let mut cfg = common::cfg();
    with_question(&mut cfg, "u01-q", &["url", "title"]);
    let path = home.path().join("second.ospider");
    let file = common::create(&path, cfg);
    file.set_meta("state", "Completed").unwrap();
    drop(file);
    common::open(&app, &path).await;
    let view = status(&app).await;
    assert!(row_for(&view, "decision").is_object());
    assert!(rows_of(&view, "requests").is_empty(), "zweiter Crawl fragt nicht neu: {view}");
}

// spec: AI-018 [M4]
// acceptance: independent
// Kommt ein Feld hinzu (Seitentext), erscheint der Dialog erneut, bevor der nächste Request mit Seitentext gesendet wird;
// er nennt das neue Feld.
#[tokio::test]
async fn ai018_neues_feld_fragt_erneut() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    edit_config(&app, |cfg| with_question(cfg, "u01-q", &["url", "title"])).await;
    confirm_all(&app).await;
    edit_config(&app, |cfg| with_question(cfg, "u01-q", &["url", "title", "page_text"])).await;
    let view = status(&app).await;
    let req = request_for(&view, "decision").unwrap_or_else(|| panic!("neues Feld → neue Frage: {view}"));
    let added: Vec<&str> = req["addedFields"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
    assert_eq!(added, vec!["page_text"], "{req}");
    assert!(!req["previousFields"].is_null(), "frühere Einwilligung bekannt: {req}");
}

// spec: AI-018 [M4]
// acceptance: independent
// Widerruf je Anbieter: danach stoppen offene Aufgaben dieses Anbieters sofort (`Pending (consent)`, kein Request), die
// Übersicht verlangt wieder eine Bestätigung; andere Anbieter bleiben unberührt.
#[tokio::test]
async fn ai018_widerruf_je_anbieter_stoppt_sofort() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    preview_on(&app).await;
    title_flow(&app);
    let mock = Mock::start().await;
    let fields = [InputField::Url, InputField::Title, InputField::MetaDescription, InputField::H1];
    let granted = confirm_all(&app).await;
    assert!(rows_of(&granted, "consentedProviders").iter().any(|p| p == "openai"), "{granted}");
    assert!(guarded(&app, &mock, "suggest-title", &fields).await.sent);
    let before = mock.content_requests();

    let revoked = call(&app, "ai.consentGrant", json!({ "revokeProvider": "openai" })).await;
    assert!(!rows_of(&revoked, "consentedProviders").iter().any(|p| p == "openai"), "{revoked}");
    assert!(request_for(&revoked, "suggest-title").is_some(), "Bestätigung erneut nötig: {revoked}");
    let out = guarded(&app, &mock, "suggest-title", &fields).await;
    assert_eq!(out.outcome, Outcome::Status(AiStatus::PendingConsent));
    assert_eq!(mock.content_requests(), before, "nach Widerruf kein Request");

    // Widerruf gilt auch nach Neustart (je Installation).
    drop(app);
    let app = app_after_restart(home.path());
    title_flow(&app);
    assert!(request_for(&status(&app).await, "suggest-title").is_some());
}

fn app_after_restart(home: &std::path::Path) -> App {
    fresh_app(home)
}

// spec: AI-018 [M4]
// acceptance: independent
// Der Dialog zeigt stets den Stand der Schwärzung: an + `page_text` → `Redaction = on` ohne PII-Hinweis; aus → `off` mit
// Hinweis. Ohne `page_text`/`h2`/`image_context` kein Hinweis, auch bei Schwärzung aus.
#[tokio::test]
async fn ai018_schwaerzung_an_aus_und_pii_hinweis() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    edit_config(&app, |cfg| {
        with_question(cfg, "u01-text", &["url", "page_text"]);
    })
    .await;
    let on = status(&app).await;
    assert_eq!(on["redaction"], true);
    let req = request_for(&on, "decision").expect("Anfrage");
    assert_eq!(cell(&req["row"], "Redaction"), "on");
    assert_eq!(req["piiNotice"], false, "{req}");

    patch_settings(&app, json!({ "aiApp": { "redactPersonalData": false } })).await;
    let off = status(&app).await;
    assert_eq!(off["redaction"], false);
    let req = request_for(&off, "decision").expect("Anfrage");
    assert_eq!(cell(&req["row"], "Redaction"), "off");
    assert_eq!(req["piiNotice"], true, "Schwärzung aus + page_text → Hinweis: {req}");

    edit_config(&app, |cfg| {
        cfg.ai.questions.clear();
        cfg.ai.active_questions.clear();
        with_question(cfg, "u01-plain", &["url", "title"]);
    })
    .await;
    let plain = status(&app).await;
    let req = request_for(&plain, "decision").expect("Anfrage");
    assert_eq!(req["piiNotice"], false, "ohne page_text kein Hinweis: {req}");
}

//! M4-U01: KI-Einstellungen in der Service-Schicht – Modellwahl je Aufgabe mit Spalte „Reasoning“ (AI-011, Nachträge
//! 24./25.11.2026), Reiter „Erweitert“ (INTEG-005 [M4]), EU-Endpunkte und Richtlinie (AI-019), Schwärzungsmuster.
//! Fixtures: ausgelieferte `task-defaults.json`/`pricing.json`, Anthropic-Mock-Modelle A–D aus
//! `tests/fixtures/ai/k04/anthropic-models.json` (Form der ai-mock-Liste nach 08 Konventionen, Mock-Erweiterungen).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use os_ai::adapters::anthropic::AnthropicModel;
use os_ai::catalog::{AvailableModel, Endpoint, ModelRef, PriceTable, TaskDefaults, TaskType};
use os_ai::provider::ReasoningLevel;
use os_app::commands::ai_settings::{
    advanced_rows, eu_rows, live_models, task_rows, ModelLists, ReasoningView, TaskRow, WarningView,
};
use os_app::{App, AppConfig, CallCtx, ErrorCode, WindowId};
use os_model::config::{AiAppSettings, AiConfig, ModelChoice, TaskModelConfig};
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

/// Mock-Modelle A–D (Anthropic) als Live-Liste.
fn anthropic_models() -> Vec<AvailableModel> {
    let doc: Value = serde_json::from_str(
        &std::fs::read_to_string(repo("tests/fixtures/ai/k04/anthropic-models.json")).unwrap(),
    )
    .unwrap();
    doc["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(AnthropicModel::from_json)
        .map(|m| AvailableModel {
            model: m.to_model(),
            max_output_tokens: m.max_tokens,
        })
        .collect()
}

fn lists(live: bool) -> ModelLists {
    let mut l = BTreeMap::new();
    if live {
        l.insert("anthropic".to_string(), anthropic_models());
    }
    ModelLists::build(&PriceTable::bundled().unwrap(), &l)
}

fn keys(present: &[&str]) -> BTreeMap<String, bool> {
    ["anthropic", "openai", "openrouter"]
        .iter()
        .map(|p| (p.to_string(), present.contains(p)))
        .chain([("ollama".to_string(), true)])
        .collect()
}

fn row(rows: &[TaskRow], task: TaskType) -> &TaskRow {
    rows.iter().find(|r| r.task == task).unwrap()
}

fn choose(model: &str, endpoint: &str, provider: &str) -> TaskModelConfig {
    TaskModelConfig {
        model: Some(ModelChoice {
            provider: provider.into(),
            model: model.into(),
            endpoint: endpoint.into(),
        }),
        ..TaskModelConfig::default()
    }
}

fn reasoning(r: &TaskRow) -> &ReasoningView {
    r.reasoning.as_ref().expect("Aufgabe mit Stufe")
}

// spec: AI-011 [M4]
// Ausgelieferte Standardwerte: `suggest-title` über OpenAI Responses `gpt-6-luna` mit Stufe `off` (Katalog: `none` → `off`),
// `max_tokens = 1024` (2 × 512 + 0); Dropdown = `model-default` + Katalogstufen; Decisions und Embeddings ohne Stufe.
// Nur OpenAI-Schlüssel → `ask-sql` über `gpt-6-sol` (erstes verfügbares Modell der Ersatzliste, Nachtrag 25.11.2026).
#[test]
fn bundled_defaults_give_reasoning_column_and_output_limit() {
    let defaults = TaskDefaults::bundled().unwrap();
    let rows = task_rows(
        &AiConfig::default(),
        &defaults,
        &lists(false),
        &keys(&["openai"]),
    )
    .unwrap();
    assert_eq!(rows.len(), 12);
    let title = row(&rows, TaskType::SuggestTitle);
    assert_eq!(
        title.effective_model,
        Some(ModelRef::new("openai", "gpt-6-luna", Endpoint::Responses))
    );
    let r = reasoning(title);
    assert_eq!(
        (r.requested, r.effective, r.levels_known),
        (ReasoningLevel::Off, ReasoningLevel::Off, true)
    );
    assert_eq!(
        r.dropdown,
        [
            ReasoningLevel::ModelDefault,
            ReasoningLevel::Off,
            ReasoningLevel::Low,
            ReasoningLevel::Medium,
            ReasoningLevel::High,
            ReasoningLevel::XHigh,
            ReasoningLevel::Max
        ]
    );
    assert_eq!(r.warning, None);
    let m = title.max_tokens.as_ref().unwrap();
    assert_eq!(
        (m.standard, m.value, m.tight),
        (Some(1024), Some(1024), false)
    );

    let decision = row(&rows, TaskType::Decision);
    assert!(decision.reasoning.is_none(), "Decisions API ohne Stufe");
    assert!(decision.max_tokens.is_none());
    assert!(row(&rows, TaskType::Embedding).reasoning.is_none());

    let ask = row(&rows, TaskType::AskSql);
    assert_eq!(
        ask.planned_model,
        Some(ModelRef::new("openai", "gpt-6-sol", Endpoint::Responses))
    );
    // Stufe `medium` auf dem Katalogmodell: `max_tokens` = 2 × 1.024 + 4.096 (Kriterium `ask-sql` → 6144 bezieht sich auf
    // das tatsächlich verwendete Modell; die Zeile zeigt das Standardmodell der Aufgabe, ohne Liste ungeprüft).
    assert_eq!(reasoning(ask).requested, ReasoningLevel::Medium);
    assert!(!reasoning(ask).levels_known);
}

// spec: AI-011 [M4]
// Modell B (nicht abschaltbar) für `suggest-title` → effektive Stufe `low`, Warnung „aus nicht verfügbar“, `max_tokens = 2048`;
// Dropdown für B ohne `off`, für D nur `model-default`; `audit-summary` auf B → 4000 (Kappung statt 20.480);
// Nutzerwert unter dem Standard → „knapp“.
#[test]
fn live_anthropic_models_drive_dropdown_warning_and_limits() {
    let defaults = TaskDefaults::bundled().unwrap();
    let mut ai = AiConfig::default();
    ai.tasks.insert(
        "suggest-title".into(),
        choose("mock-claude-b", "messages", "anthropic"),
    );
    ai.tasks.insert(
        "audit-summary".into(),
        choose("mock-claude-b", "messages", "anthropic"),
    );
    ai.tasks.insert(
        "suggest-meta".into(),
        choose("mock-claude-d", "messages", "anthropic"),
    );
    let mut tight = choose("mock-claude", "messages", "anthropic");
    tight.max_tokens = Some(500);
    ai.tasks.insert("suggest-h1".into(), tight);
    let rows = task_rows(&ai, &defaults, &lists(true), &keys(&["anthropic"])).unwrap();

    let b = row(&rows, TaskType::SuggestTitle);
    let r = reasoning(b);
    assert_eq!(r.effective, ReasoningLevel::Low);
    assert!(!r.dropdown.contains(&ReasoningLevel::Off));
    assert_eq!(
        r.warning,
        Some(WarningView::OffUnavailable {
            effective: ReasoningLevel::Low
        })
    );
    assert_eq!(b.max_tokens.as_ref().unwrap().value, Some(2048));
    assert_eq!(
        b.planned_model,
        Some(ModelRef::new(
            "anthropic",
            "mock-claude-b",
            Endpoint::Messages
        ))
    );

    let audit = row(&rows, TaskType::AuditSummary);
    assert_eq!(audit.max_tokens.as_ref().unwrap().value, Some(4000));

    let d = row(&rows, TaskType::SuggestMeta);
    assert_eq!(reasoning(d).dropdown, [ReasoningLevel::ModelDefault]);
    assert_eq!(reasoning(d).effective, ReasoningLevel::ModelDefault);

    let a = row(&rows, TaskType::SuggestH1);
    let m = a.max_tokens.as_ref().unwrap();
    assert_eq!(
        (m.user, m.standard, m.value, m.tight),
        (Some(500), Some(1024), Some(500), true)
    );
    assert_eq!(reasoning(a).effective, ReasoningLevel::Off);
}

// spec: AI-011 [M4]
// Gespeicherte Stufe nach Modellwechsel nicht verfügbar → effektive Stufe, Warnung, gespeicherter Wert bleibt.
#[test]
fn stored_level_survives_a_model_without_it() {
    let defaults = TaskDefaults::bundled().unwrap();
    let mut ai = AiConfig::default();
    let mut c = choose("mock-claude", "messages", "anthropic");
    c.effort = Some("xhigh".into());
    ai.tasks.insert("llms-summary".into(), c);
    let rows = task_rows(&ai, &defaults, &lists(true), &keys(&["anthropic"])).unwrap();
    let r = reasoning(row(&rows, TaskType::LlmsSummary));
    assert_eq!(r.stored, Some(ReasoningLevel::XHigh));
    assert_eq!(r.effective, ReasoningLevel::High);
    assert_eq!(
        r.warning,
        Some(WarningView::StoredUnavailable {
            stored: ReasoningLevel::XHigh,
            effective: ReasoningLevel::High
        })
    );
}

// spec: AI-011 [M4]
// Ein Modell ohne benötigte Fähigkeit kann der Aufgabe nicht zugewiesen werden: Embedding-Modell nicht für Texte,
// Decisions-Modell nicht für `suggest-alt`; der Command lehnt den Entwurf mit Feld `ai.tasks.<aufgabe>.model` ab.
#[tokio::test]
async fn incapable_models_are_not_assignable() {
    let defaults = TaskDefaults::bundled().unwrap();
    let rows = task_rows(&AiConfig::default(), &defaults, &lists(true), &keys(&[])).unwrap();
    let alt = row(&rows, TaskType::SuggestAlt);
    assert!(alt
        .models
        .iter()
        .all(|m| m.endpoint != Endpoint::Embeddings && m.endpoint != Endpoint::Decisions));
    // Modell D hat keine Bildeingabe → für `suggest-alt` nicht zuweisbar (mit Grund).
    let d = alt
        .models
        .iter()
        .find(|m| m.model == "mock-claude-d")
        .unwrap();
    assert!(!d.assignable);
    assert!(d.missing.contains(&"vision"));

    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    live_models(&app).set("anthropic", anthropic_models());
    let draft = json!({ "tasks": { "suggest-alt": { "model": { "provider": "anthropic", "model": "mock-claude-d", "endpoint": "messages" } } } });
    let err = app
        .dispatch(ctx(), "ai.settingsSet", json!({ "ai": draft }))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(err.message_key, "err-ai-model-incapable");
    assert_eq!(err.field.as_deref(), Some("ai.tasks.suggest-alt.model"));
    let ok = app
        .dispatch(ctx(), "ai.settingsSet", json!({ "ai": { "tasks": {} } }))
        .await
        .unwrap();
    assert_eq!(ok["tasks"].as_array().unwrap().len(), 12);
}

// spec: AI-011 [M4]
// Profil-Export und -Import erhalten Zuordnung, Ersatzliste, Stufe und Ausgabegrenze (Roundtrip über `.osconfig`);
// „Auf Standard zurücksetzen“ (Eintrag entfernt) setzt auch die Stufe zurück.
#[test]
fn profile_roundtrip_keeps_model_fallback_level_and_limit() {
    let defaults = TaskDefaults::bundled().unwrap();
    let mut cfg = CrawlConfig::default();
    let mut c = choose("mock-claude", "messages", "anthropic");
    c.fallback = vec![ModelChoice {
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: "responses".into(),
    }];
    c.effort = Some("medium".into());
    c.max_tokens = Some(3000);
    cfg.ai.tasks.insert("suggest-title".into(), c);
    let text = cfg.to_osconfig_string().unwrap();
    let back = CrawlConfig::load_str(&text).unwrap().config;
    let before = task_rows(&cfg.ai, &defaults, &lists(true), &keys(&[])).unwrap();
    let after = task_rows(&back.ai, &defaults, &lists(true), &keys(&[])).unwrap();
    assert_eq!(before, after);
    let t = row(&after, TaskType::SuggestTitle);
    assert_eq!(reasoning(t).stored, Some(ReasoningLevel::Medium));
    assert_eq!(t.max_tokens.as_ref().unwrap().user, Some(3000));
    assert_eq!(t.fallback.len(), 1);
    assert!(!t.is_default);

    let mut reset = back.ai.clone();
    reset.tasks.remove("suggest-title");
    let rows = task_rows(&reset, &defaults, &lists(true), &keys(&[])).unwrap();
    let t = row(&rows, TaskType::SuggestTitle);
    assert!(t.is_default);
    assert_eq!(reasoning(t).stored, None);
    assert_eq!(reasoning(t).requested, ReasoningLevel::Off);
}

// spec: INTEG-005 [M4]
// Reiter „Erweitert“: Ollama zeigt seine eigene Vorgabe (Standard 1, AI-006), alle KI-Anbieter den Bereich 1–50.
#[test]
fn advanced_tab_shows_the_ollama_default_of_one() {
    let rows = advanced_rows(&AiAppSettings::default());
    let ollama = rows.iter().find(|r| r.provider == "ollama").unwrap();
    assert_eq!((ollama.concurrent_default, ollama.concurrent), (1, 1));
    for r in &rows {
        assert_eq!(
            (r.concurrent_min, r.concurrent_max),
            (1, 50),
            "{}",
            r.provider
        );
    }
    let others: Vec<u16> = rows
        .iter()
        .filter(|r| r.provider != "ollama")
        .map(|r| r.concurrent_default)
        .collect();
    assert_eq!(others.len(), 3);
    assert!(
        others.iter().all(|d| *d != 1),
        "nur Ollama hat die Vorgabe 1: {others:?}"
    );
}

// spec: AI-019
// EU-Schalter je Anbieter mit vorbelegter Basis-URL laut Tabelle; zulässig unter „EU erzwingen“ nur mit Schalter und
// bestätigtem Konto.
#[test]
#[ignore = "wartet auf M4-K07 (1.2): AI-019 EU-Verarbeitung und Nur lokal werden in 1.1 nicht durchgesetzt"]
fn eu_rows_carry_prefilled_urls_and_confirmation() {
    let mut s = AiAppSettings::default();
    let rows = eu_rows(&s);
    let urls: Vec<(&str, &str)> = rows
        .iter()
        .map(|r| (r.provider.as_str(), r.eu_base_url.as_str()))
        .collect();
    assert_eq!(
        urls,
        [
            ("openai", "https://eu.api.openai.com"),
            ("openrouter", "https://eu.openrouter.ai/api/v1")
        ]
    );
    assert!(rows.iter().all(|r| !r.allowed_under_eu));
    let mut e = s
        .provider_endpoints
        .get("openai")
        .cloned()
        .unwrap_or_default();
    e.use_eu_endpoint = true;
    s.provider_endpoints.insert("openai".into(), e.clone());
    assert!(
        !eu_rows(&s)[0].allowed_under_eu,
        "ohne Kontobestätigung nicht zulässig"
    );
    e.eu_account_confirmed = true;
    s.provider_endpoints.insert("openai".into(), e);
    assert!(eu_rows(&s)[0].allowed_under_eu);
}

// spec: AI-011 [M4]
// `ai.settingsGet` über den Command-Weg: Schalter, zwölf Aufgaben, Schlüssel ohne Wert, Richtlinie „Keine Einschränkung“,
// Crawl-Budget 5,00 USD und Bestätigungsschwelle 1,00 USD (E-5 a); in 1.1 keine Richtlinie und keine EU-Endpunkte zur Wahl.
#[tokio::test]
async fn settings_get_reflects_window_config_and_app_settings() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let v = app
        .dispatch(ctx(), "ai.settingsGet", Value::Null)
        .await
        .unwrap();
    assert_eq!(v["previewEnabled"], json!(false));
    assert_eq!(v["tasks"].as_array().unwrap().len(), 12);
    assert_eq!(v["keys"]["openai"], json!("missing"));
    assert_eq!(v["keys"]["ollama"], json!("local"));
    assert_eq!(v["policy"], json!("unrestricted"));
    assert_eq!(v["budget"]["crawl"], json!(5_000_000));
    assert_eq!(v["budget"]["warning_percent"], json!(80));
    assert_eq!(v["confirmationThresholdMicroUsd"], json!(1_000_000));
    assert_eq!(v["advanced"].as_array().unwrap().len(), 4);
    assert_eq!(v["policyAvailable"], json!(false));
    assert_eq!(v["eu"], json!([]));
}

// spec: AI-019
// Die Richtlinie des Fensters folgt `config.set` (wählbar erst mit der Durchsetzung in 1.2).
#[tokio::test]
#[ignore = "wartet auf M4-K07 (1.2): AI-019 Richtlinie wählbar erst mit Durchsetzung"]
async fn settings_get_follows_the_window_policy() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let mut dto = app
        .dispatch(ctx(), "config.get", Value::Null)
        .await
        .unwrap();
    dto["config"]["ai"]["policy"] = json!("local-only");
    app.dispatch(ctx(), "config.set", dto)
        .await
        .expect("config.set");
    let v = app
        .dispatch(ctx(), "ai.settingsGet", Value::Null)
        .await
        .unwrap();
    assert_eq!(v["policy"], json!("local-only"));
}

// spec: AI-019
// 1.1 (Entscheidung des Orchestrators 26.11.2026): Eine Richtlinie außer „Keine Einschränkung“ würde eine Zusage machen, die
// kein Lauf durchsetzt – `ai.settingsSet` lehnt sie mit Feld `ai.policy` ab; „Keine Einschränkung“ geht durch.
#[tokio::test]
async fn policy_other_than_unrestricted_is_rejected_in_1_1() {
    const { assert!(!os_app::commands::ai_settings::AI_019_ENABLED) };
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    for policy in ["eu-only", "local-only"] {
        let err = app
            .dispatch(
                ctx(),
                "ai.settingsSet",
                json!({ "ai": { "policy": policy } }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput, "{policy}");
        assert_eq!(err.message_key, "err-ai-policy-unavailable");
        assert_eq!(err.field.as_deref(), Some("ai.policy"));
        assert_eq!(err.args["policy"], json!(policy));
    }
    let ok = app
        .dispatch(
            ctx(),
            "ai.settingsSet",
            json!({ "ai": { "policy": "unrestricted" } }),
        )
        .await
        .unwrap();
    assert_eq!(ok["tasks"].as_array().unwrap().len(), 12);
}

// spec: AI-024
// Eigene Muster: ungültige Regex beim Speichern mit Zeile und Position abgelehnt (Leerzeile zählt mit), gültige Liste
// landet in den App-Einstellungen.
#[tokio::test]
async fn redaction_patterns_are_checked_on_save() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let err = app
        .dispatch(
            ctx(),
            "ai.settingsSet",
            json!({ "redactionPatterns": "KD-\\d+\n\n(ab" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert_eq!(err.message_key, "err-ai-pattern-invalid");
    assert_eq!(err.args["line"], json!(3));
    assert!(err.args["position"].as_u64().unwrap() >= 1);

    let ok = app
        .dispatch(
            ctx(),
            "ai.settingsSet",
            json!({ "redactionPatterns": "KD-\\d+\n\nORD-[0-9]{4}" }),
        )
        .await
        .unwrap();
    assert_eq!(ok["redactionPatterns"], json!(["KD-\\d+", "ORD-[0-9]{4}"]));
    let settings = app
        .dispatch(ctx(), "settings.get", Value::Null)
        .await
        .unwrap();
    assert_eq!(
        settings["aiApp"]["redactionPatterns"],
        json!(["KD-\\d+", "ORD-[0-9]{4}"])
    );
}

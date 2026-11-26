//! Unabhängige Abnahme M4-U01, Teil Einstellungen: Modellwahl je Aufgabe mit Spalte „Reasoning“ (AI-011 mit den
//! Nachträgen 24./25.11.2026), Reiter „Erweitert“ (INTEG-005 [M4]) und Richtlinie (AI-019, Entscheidung des
//! Orchestrators vom 26.11.2026: in 1.1 nimmt der Befehl keine Richtlinie an).
//!
//! Grundlage allein `docs/spec/08-ai.md`, `docs/spec/04-custom-integrations.md` und `docs/plan/M4.md`; angebunden über die
//! öffentlichen Commands `ai.settingsGet`/`ai.settingsSet` (Plan M4 3.3) und die Live-Modelllisten des ai-mock.
// acceptance: independent
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

mod acceptance_u01_support;

use acceptance_u01_support::*;
use os_app::commands::ai_settings::live_models;
use os_app::ErrorCode;
use os_model::config::{ModelChoice, TaskModelConfig};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Aufgabentypen aus AI-011 (Beschreibung).
const TASKS: [&str; 12] = [
    "decision",
    "decision-vision",
    "suggest-title",
    "suggest-meta",
    "suggest-h1",
    "suggest-alt",
    "redirect-confirm",
    "link-anchor",
    "ask-sql",
    "audit-summary",
    "llms-summary",
    "embedding",
];

fn levels(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap_or_else(|| panic!("keine Liste: {v}"))
        .iter()
        .map(|l| l.as_str().unwrap().to_string())
        .collect()
}

fn refs(v: &Value) -> Vec<(String, String, String)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["provider"].as_str().unwrap().to_string(),
                r["model"].as_str().unwrap().to_string(),
                r["endpoint"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn t(p: &str, m: &str, e: &str) -> (String, String, String) {
    (p.into(), m.into(), e.into())
}

/// Entwurf `ai` mit einer Aufgabe `task` (Form `TaskModelConfig` des Profils).
fn draft(task: &str, cfg: Value) -> Value {
    json!({ "ai": { "tasks": { task: cfg } } })
}

async fn evaluate(app: &os_app::App, task: &str, cfg: Value) -> Value {
    call(app, "ai.settingsSet", draft(task, cfg)).await
}

/// Lädt die Anthropic- und OpenRouter-Listen des ai-mock in die Modellwahl (wie der Kontenbereich nach „Verbinden“).
async fn load_live_lists(app: &os_app::App, mock: &Mock) -> AnthropicSet {
    let anthropic = mock.anthropic_entries().await;
    let set = anthropic_set(&anthropic);
    live_models(app).set(
        "anthropic",
        anthropic.iter().map(anthropic_available).collect(),
    );
    let openrouter = mock.openrouter_entries().await;
    live_models(app).set(
        "openrouter",
        openrouter.iter().map(openrouter_available).collect(),
    );
    set
}

fn id(entry: &Value) -> String {
    entry["id"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------------------------------------------
// Standardwerte

// spec: AI-011 [M4]
// acceptance: independent
// Tabelle Aufgabe → Modell: alle zwölf Aufgabentypen mit Standardmodell und Ersatzliste in der Reihenfolge des
// Nachtrags 25.11.2026 und der Standardstufe des Nachtrags 24.11.2026 (Decisions API und Embeddings ohne Stufe).
#[tokio::test]
async fn ai011_tabelle_zeigt_zwoelf_aufgaben_mit_standardmodell_ersatzliste_und_stufe() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    preview_on(&app).await;
    let view = call(&app, "ai.settingsGet", Value::Null).await;
    let tasks: Vec<String> = view["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["task"].as_str().unwrap().to_string())
        .collect();
    let mut sorted = tasks.clone();
    sorted.sort();
    let mut expected: Vec<String> = TASKS.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(sorted, expected, "Aufgabentypen laut AI-011");

    let luna_d = t("openai", "gpt-6-luna", "decisions");
    let luna_r = t("openai", "gpt-6-luna", "responses");
    let haiku = t("anthropic", "claude-haiku-5-5", "messages");
    let sonnet = t("anthropic", "claude-sonnet-5-5", "messages");
    let or_luna = t("openrouter", "openai/gpt-6-luna", "chat-completions");
    let sol = t("openai", "gpt-6-sol", "responses");
    let emb = t("openai", "text-embedding-3-small", "embeddings");
    let table: [(&str, &(String, String, String), Vec<(String, String, String)>, Option<&str>); 12] = [
        ("decision", &luna_d, vec![haiku.clone(), or_luna.clone()], None),
        ("decision-vision", &luna_d, vec![haiku.clone(), or_luna.clone()], None),
        ("suggest-title", &luna_r, vec![haiku.clone()], Some("off")),
        ("suggest-meta", &luna_r, vec![haiku.clone()], Some("off")),
        ("suggest-h1", &luna_r, vec![haiku.clone()], Some("off")),
        ("link-anchor", &luna_r, vec![haiku.clone()], Some("off")),
        ("redirect-confirm", &luna_r, vec![haiku.clone()], Some("off")),
        ("suggest-alt", &luna_r, vec![haiku.clone()], Some("off")),
        ("ask-sql", &sonnet, vec![sol.clone(), luna_r.clone()], Some("medium")),
        ("audit-summary", &sonnet, vec![sol.clone(), luna_r.clone()], Some("medium")),
        ("llms-summary", &sonnet, vec![sol.clone(), luna_r.clone()], Some("low")),
        ("embedding", &emb, vec![], None),
    ];
    for (task, model, fallback, level) in table {
        let r = row(&view, task);
        let dm = &r["defaultModel"];
        assert_eq!(
            t(
                dm["provider"].as_str().unwrap(),
                dm["model"].as_str().unwrap(),
                dm["endpoint"].as_str().unwrap()
            ),
            *model,
            "{task}: Standardmodell"
        );
        assert_eq!(refs(&r["defaultFallback"]), fallback, "{task}: Ersatzliste");
        assert_eq!(r["isDefault"], true, "{task}");
        match level {
            None => assert!(
                r["reasoning"].is_null(),
                "{task}: Decisions API und Embeddings haben keine Stufe: {}",
                r["reasoning"]
            ),
            Some(l) => assert_eq!(r["reasoning"]["requested"], l, "{task}: Standardstufe"),
        }
    }
}

// spec: AI-011 [M4]
// acceptance: independent
// AI-010 (Nachtrag): für ein OpenAI-Katalogmodell bietet die Spalte „Reasoning“ genau die Stufen aus `reasoning_efforts`
// (`none` → `off`) und `model-default`; `suggest-title` mit Standardstufe `off` sendet `max_tokens = 2 × 512 + 0`.
#[tokio::test]
async fn ai011_reasoning_dropdown_eines_openai_katalogmodells_nur_mit_verfuegbaren_stufen() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    let pricing = repo_json("resources/ai/pricing.json");
    let luna = pricing["prices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["provider"] == "openai" && e["model"] == "gpt-6-luna" && e["endpoint"] == "responses")
        .expect("gpt-6-luna/responses in pricing.json");
    let mut expected: BTreeSet<String> = luna["reasoning_efforts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| match v.as_str().unwrap() {
            "none" => "off".to_string(),
            other => other.to_string(),
        })
        .collect();
    expected.insert("model-default".into());

    let view = call(&app, "ai.settingsGet", Value::Null).await;
    let r = row(&view, "suggest-title");
    let dropdown: BTreeSet<String> = levels(&r["reasoning"]["dropdown"]).into_iter().collect();
    assert_eq!(dropdown, expected, "nur verfügbare Stufen und model-default");
    assert_eq!(r["reasoning"]["effective"], "off");
    assert!(r["reasoning"]["warning"].is_null(), "{}", r["reasoning"]);
    assert_eq!(r["maxTokens"]["standard"], 1024, "2 × 512 + 0");
    assert_eq!(r["maxTokens"]["value"], 1024);
    assert_eq!(r["maxTokens"]["tight"], false);
}

// ---------------------------------------------------------------------------------------------------------------
// Anthropic-Mockmodelle A, B, D (AI-011 Nachtrag 24.11.2026)

// spec: AI-011 [M4]
// acceptance: independent
// `suggest-title` auf Modell A (abschaltbar) → effektive Stufe `off`, `max_tokens = 1024`, kein Hinweis; auf Modell B
// (nicht abschaltbar) → effektive Stufe `low`, `max_tokens = 2048`, Warnhinweis im Aufgabendialog; das Dropdown bietet für
// B kein `off`, für D nur `model-default`.
#[tokio::test]
async fn ai011_modell_a_b_d_stufen_warnung_und_ausgabegrenze() {
    let home = tempfile::tempdir().unwrap();
    let app = app_with_keys(home.path(), &["anthropic"]);
    let mock = Mock::start().await;
    let set = load_live_lists(&app, &mock).await;

    let on = |m: &Value| json!({ "model": model_ref("anthropic", &id(m), "messages") });

    let a = evaluate(&app, "suggest-title", on(&set.a)).await;
    let ra = row(&a, "suggest-title");
    assert_eq!(ra["reasoning"]["effective"], "off", "A: {}", ra["reasoning"]);
    assert!(levels(&ra["reasoning"]["dropdown"]).contains(&"off".to_string()));
    assert!(ra["reasoning"]["warning"].is_null(), "A ohne Hinweis");
    assert_eq!(ra["maxTokens"]["value"], 1024, "A: 2 × 512 + 0");

    let b = evaluate(&app, "suggest-title", on(&set.b)).await;
    let rb = row(&b, "suggest-title");
    assert_eq!(rb["reasoning"]["effective"], "low", "B: {}", rb["reasoning"]);
    assert!(
        !levels(&rb["reasoning"]["dropdown"]).contains(&"off".to_string()),
        "B bietet kein off: {}",
        rb["reasoning"]["dropdown"]
    );
    assert!(levels(&rb["reasoning"]["dropdown"]).contains(&"model-default".to_string()));
    assert!(!rb["reasoning"]["warning"].is_null(), "B: Warnhinweis fehlt");
    assert_eq!(rb["maxTokens"]["value"], 2048, "B: 2 × 512 + 1.024");

    let d = evaluate(&app, "suggest-title", on(&set.d)).await;
    let rd = row(&d, "suggest-title");
    assert_eq!(
        levels(&rd["reasoning"]["dropdown"]),
        vec!["model-default".to_string()],
        "D bietet nur model-default"
    );

    // `audit-summary` auf Modell B (max_tokens = 4000 in der Modellliste) → Kappung auf 4000 statt 20.480.
    let audit = evaluate(&app, "audit-summary", on(&set.b)).await;
    assert_eq!(row(&audit, "audit-summary")["maxTokens"]["value"], 4000);
}

// spec: AI-011 [M4]
// acceptance: independent
// Ist eine gespeicherte Stufe nach Modellwechsel nicht verfügbar, gilt die effektive Stufe (nächstniedrigere
// verfügbare), der Aufgabendialog warnt und der gespeicherte Wert bleibt erhalten.
#[tokio::test]
async fn ai011_gespeicherte_stufe_nicht_verfuegbar_warnt_und_bleibt_erhalten() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    // gpt-6-luna (Responses) kennt laut pricing.json kein `minimal` → nächstniedrigere verfügbare Stufe ist `off`.
    let v = evaluate(&app, "suggest-title", json!({ "effort": "minimal" })).await;
    let r = &row(&v, "suggest-title")["reasoning"];
    assert_eq!(r["stored"], "minimal", "gespeicherter Wert bleibt: {r}");
    assert_eq!(r["effective"], "off", "{r}");
    assert!(!r["warning"].is_null(), "Warnung fehlt: {r}");
    // Verfügbare gespeicherte Stufe: keine Warnung.
    let v = evaluate(&app, "suggest-title", json!({ "effort": "high" })).await;
    let r = &row(&v, "suggest-title")["reasoning"];
    assert_eq!(r["effective"], "high");
    assert!(r["warning"].is_null(), "{r}");
}

// spec: AI-011 [M4]
// acceptance: independent
// Ein Nutzerwert für `max_tokens` ersetzt den Standard; ist er kleiner, zeigt der Aufgabendialog „Ausgabegrenze knapp“.
#[tokio::test]
async fn ai011_ausgabegrenze_nutzerwert_und_hinweis_knapp() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    let small = evaluate(&app, "suggest-title", json!({ "max_tokens": 500 })).await;
    let m = &row(&small, "suggest-title")["maxTokens"];
    assert_eq!(m["value"], 500, "{m}");
    assert_eq!(m["tight"], true, "kleiner als der Standard 1024: {m}");
    let large = evaluate(&app, "suggest-title", json!({ "max_tokens": 3000 })).await;
    let m = &row(&large, "suggest-title")["maxTokens"];
    assert_eq!(m["value"], 3000, "{m}");
    assert_eq!(m["tight"], false, "{m}");
}

// ---------------------------------------------------------------------------------------------------------------
// Ungültige Eingaben

// spec: AI-011 [M4]
// acceptance: independent
// Ein Modell ohne benötigte Fähigkeit kann der Aufgabe nicht zugewiesen werden (Nicht-Vision-Modell für `suggest-alt`);
// ebenso werden unbekannte Aufgaben, unbekannte Stufen und Ausgabegrenzen außerhalb des Zahlbereichs abgelehnt.
#[tokio::test]
async fn ai011_ungueltige_zuweisungen_und_werte_werden_abgelehnt() {
    let home = tempfile::tempdir().unwrap();
    let app = app_with_keys(home.path(), &["anthropic"]);
    let mock = Mock::start().await;
    let set = load_live_lists(&app, &mock).await;
    assert_eq!(
        set.c["capabilities"]["image_input"]["supported"], false,
        "Modell C der Mockliste hat keine Bildeingabe"
    );
    let c = model_ref("anthropic", &id(&set.c), "messages");

    // Nicht-Vision-Modell für `suggest-alt`: im Dropdown nicht zuweisbar …
    let view = call(&app, "ai.settingsGet", Value::Null).await;
    let alt = row(&view, "suggest-alt");
    let option = alt["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["provider"] == "anthropic" && o["model"] == c["model"])
        .unwrap_or_else(|| panic!("Modell C fehlt in der Auswahl: {}", alt["models"]));
    assert_eq!(option["assignable"], false, "{option}");
    // … und als Entwurf abgelehnt, mit dem Feld der Aufgabe.
    let err = try_call(&app, "ai.settingsSet", draft("suggest-alt", json!({ "model": c })))
        .await
        .expect_err("Nicht-Vision-Modell für suggest-alt muss abgelehnt werden");
    assert_eq!(err.code, ErrorCode::InvalidInput, "{err:?}");
    assert_eq!(err.field.as_deref(), Some("ai.tasks.suggest-alt.model"), "{err:?}");

    for (task, cfg, what) in [
        ("suggest-title", json!({ "effort": "turbo" }), "unbekannte Stufe"),
        ("unknown-task", json!({}), "unbekannte Aufgabe"),
        ("suggest-title", json!({ "max_tokens": -5 }), "negative Ausgabegrenze"),
        ("suggest-title", json!({ "max_tokens": "viel" }), "Ausgabegrenze ohne Zahl"),
    ] {
        let err = try_call(&app, "ai.settingsSet", draft(task, cfg))
            .await
            .expect_err(what);
        assert_eq!(err.code, ErrorCode::InvalidInput, "{what}: {err:?}");
    }
}

// spec: AI-011 [M4]
// acceptance: independent
// Geordnete Ersatzliste: die gewählte Reihenfolge bleibt erhalten; „Auf Standard zurücksetzen“ setzt Modell, Liste,
// Stufe und Ausgabegrenze zurück.
#[tokio::test]
async fn ai011_ersatzliste_reihenfolge_und_auf_standard_zuruecksetzen() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    let sol = model_ref("openai", "gpt-6-sol", "responses");
    let haiku = model_ref("anthropic", "claude-haiku-5-5", "messages");
    let sonnet = model_ref("anthropic", "claude-sonnet-5-5", "messages");
    let v = evaluate(
        &app,
        "suggest-meta",
        json!({ "model": sol, "fallback": [sonnet, haiku], "effort": "low", "max_tokens": 900 }),
    )
    .await;
    let r = row(&v, "suggest-meta");
    assert_eq!(
        refs(&r["fallback"]),
        vec![
            t("anthropic", "claude-sonnet-5-5", "messages"),
            t("anthropic", "claude-haiku-5-5", "messages")
        ]
    );
    assert_eq!(r["isDefault"], false);
    // Andere Reihenfolge → andere Liste.
    let v = evaluate(
        &app,
        "suggest-meta",
        json!({ "model": sol, "fallback": [haiku, sonnet] }),
    )
    .await;
    assert_eq!(
        refs(&row(&v, "suggest-meta")["fallback"]),
        vec![
            t("anthropic", "claude-haiku-5-5", "messages"),
            t("anthropic", "claude-sonnet-5-5", "messages")
        ]
    );
    // Zurücksetzen = die Aufgabe hat keinen eigenen Eintrag mehr.
    let v = call(&app, "ai.settingsSet", json!({ "ai": { "tasks": {} } })).await;
    let r = row(&v, "suggest-meta");
    assert_eq!(r["isDefault"], true);
    assert!(r["model"].is_null());
    assert_eq!(r["reasoning"]["requested"], "off");
    assert!(r["maxTokens"]["user"].is_null());
}

// spec: AI-011 [M4]
// acceptance: independent
// Profil-Export und -Import erhalten Stufe und eingestellte Ausgabegrenze (Roundtrip über die Konfiguration des
// Fensters); die Tabelle zeigt danach die gespeicherten Werte.
#[tokio::test]
async fn ai011_stufe_und_ausgabegrenze_bleiben_ueber_die_konfiguration_erhalten() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    edit_config(&app, |cfg| {
        cfg.ai.tasks.insert(
            "ask-sql".into(),
            TaskModelConfig {
                model: Some(ModelChoice {
                    provider: "openai".into(),
                    model: "gpt-6-sol".into(),
                    endpoint: "responses".into(),
                }),
                effort: Some("high".into()),
                max_tokens: Some(7000),
                ..TaskModelConfig::default()
            },
        );
    })
    .await;
    let cfg = window_config(&app).await;
    let toml = cfg.to_osconfig_string().expect("Profil-Export");
    let back = os_model::CrawlConfig::load_str(&toml).expect("Profil-Import").config;
    assert_eq!(back.ai.tasks, cfg.ai.tasks, "Roundtrip der Zuordnung");
    let view = call(&app, "ai.settingsGet", Value::Null).await;
    let r = row(&view, "ask-sql");
    assert_eq!(r["reasoning"]["stored"], "high");
    assert_eq!(r["maxTokens"]["user"], 7000);
    assert_eq!(r["maxTokens"]["value"], 7000);
}

// spec: AI-011 [M4]
// acceptance: independent
// Ohne jeden Schlüssel: Aufgaben mit Kandidaten → `Error: Missing API key`. Nur OpenAI-Schlüssel: `decision` über Decisions
// `gpt-6-luna`, `suggest-title` über Responses `gpt-6-luna`, `ask-sql` über Responses `gpt-6-sol`, `embedding` über
// `text-embedding-3-small` (Nachtrag 25.11.2026).
#[tokio::test]
async fn ai011_zustand_ohne_schluessel_und_mit_nur_openai_schluessel() {
    let home = tempfile::tempdir().unwrap();
    let none = fresh_app(home.path());
    let view = call(&none, "ai.settingsGet", Value::Null).await;
    for task in TASKS {
        assert_eq!(
            row(&view, task)["status"],
            "Error: Missing API key",
            "{task} ohne Schlüssel"
        );
    }

    let home = tempfile::tempdir().unwrap();
    let openai = app_with_keys(home.path(), &["openai"]);
    let view = call(&openai, "ai.settingsGet", Value::Null).await;
    assert_eq!(view["keys"]["openai"], "present", "Testschlüssel wird erkannt: {}", view["keys"]);
    for (task, model, endpoint) in [
        ("decision", "gpt-6-luna", "decisions"),
        ("suggest-title", "gpt-6-luna", "responses"),
        ("ask-sql", "gpt-6-sol", "responses"),
        ("embedding", "text-embedding-3-small", "embeddings"),
    ] {
        let r = row(&view, task);
        assert!(r["status"].is_null(), "{task}: {}", r["status"]);
        assert_eq!(
            r["plannedModel"],
            model_ref("openai", model, endpoint),
            "{task}: Modell des Laufs"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// INTEG-005 [M4]

// spec: INTEG-005 [M4]
// acceptance: independent
// Reiter „Erweitert“: Ollama zeigt die eigene Vorgabe Standard 1 (AI-006); die übrigen KI-Anbieter den Standard 4 aus
// AI-017 (Hinweis des Orchestrators); Bereich 1–50.
#[tokio::test]
async fn integ005_erweitert_zeigt_ollama_standard_1_und_bereich_1_bis_50() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    let view = call(&app, "ai.settingsGet", Value::Null).await;
    let advanced = view["advanced"].as_array().expect("Zeilen „Erweitert“");
    let by = |p: &str| {
        advanced
            .iter()
            .find(|r| r["provider"] == p)
            .unwrap_or_else(|| panic!("{p} fehlt im Reiter Erweitert: {advanced:?}"))
    };
    let ollama = by("ollama");
    assert_eq!(ollama["concurrentDefault"], 1);
    assert_eq!(ollama["concurrent"], 1);
    for p in ["anthropic", "openai", "openrouter"] {
        assert_eq!(by(p)["concurrentDefault"], 4, "{p}: Standard aus AI-017");
    }
    for r in advanced {
        assert_eq!((r["concurrentMin"].as_u64(), r["concurrentMax"].as_u64()), (Some(1), Some(50)), "{r}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// AI-019 (1.2): in 1.1 nimmt der Befehl keine Richtlinie an

// spec: AI-019 [M4]
// acceptance: independent
// Entscheidung des Orchestrators (26.11.2026): AI-019 gehört zu 1.2 und wird in 1.1 nicht durchgesetzt; deshalb nimmt
// `ai.settingsSet` keine Richtlinie außer „Keine Einschränkung“ an. Erwartbar rot bis zur Nacharbeit des Bauers.
#[tokio::test]
async fn ai019_befehl_nimmt_in_1_1_keine_richtlinie_an() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    preview_on(&app).await;
    for policy in ["eu-only", "local-only"] {
        let res = try_call(&app, "ai.settingsSet", json!({ "ai": { "policy": policy } })).await;
        assert!(
            res.is_err(),
            "Richtlinie {policy} darf in 1.1 nicht angenommen werden: {res:?}"
        );
    }
    // Der Standard bleibt zulässig.
    call(&app, "ai.settingsSet", json!({ "ai": { "policy": "unrestricted" } })).await;
}

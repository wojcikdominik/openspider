//! Unabhängige Abnahme M4-U01, Teil Kosten: Statusleistenfeld `Kosten` (UI-013 [M4]) mit Werten aus dem gemeinsamen
//! `BudgetLedger` (AI-013) und der Dialog „Kostenvoranschlag“ (AI-012, Abschnitt „KI“ mit Spalte `Reasoning`).
//!
//! Grundlage allein `docs/spec/05-ui.md` UI-013, `docs/spec/08-ai.md` AI-012/AI-013 und `docs/plan/M4.md` (E-5 a:
//! Crawl-Budget 5,00 USD, Bestätigung ab 1,00 USD, Warnschwelle 80 %; Budget nur über `open_ledger`/`raise_limits`).
//! Angebunden über `ai.costBudget`, `ai.costEstimate` und `ai.settingsSet { raiseBudget }`; die Crawl-Datei wird wie in
//! den übrigen Sitzungstests angelegt und über `crawl.open` geöffnet.
// acceptance: independent
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

mod acceptance_u01_support;

#[path = "session_common.rs"]
mod common;

use std::sync::Arc;

use acceptance_u01_support::*;
use futures_util::FutureExt;
use os_ai::cost::{Executable, TaskEstimate};
use os_ai::provider::ReasoningLevel;
use os_app::commands::ai_cost::{register_estimator, EstimateEntry};
use os_app::dto::AiRunArgs;
use os_app::{App, CallCtx, CmdError, ErrorCode};
use os_model::budget::{BudgetLimits, MicroUsd};
use os_store::ai::requests::{self, PriceSource, RequestRecord};
use serde_json::{json, Value};

const USD: u64 = 1_000_000;

fn record(task: &str, cost: u64) -> RequestRecord {
    RequestRecord {
        run_id: 1,
        timestamp: "2026-11-26T10:00:00.000Z".into(),
        task: task.into(),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: "decisions".into(),
        url_id: None,
        definition_version: None,
        reservation: MicroUsd(cost),
        redactions: 0,
        input_tokens: Some(1000),
        cached_input_tokens: Some(0),
        cache_write_tokens: Some(0),
        output_tokens: Some(0),
        images: 0,
        token_cost: MicroUsd(cost),
        fixed_fees: MicroUsd(0),
        regional_uplift: MicroUsd(0),
        cost: MicroUsd(cost),
        price_source: PriceSource::Table,
        batch_id: None,
        cache_hit: false,
        status: "Success".into(),
        http_status: Some(200),
        error_code: None,
        error_message: None,
        refusal_reason: None,
        duration_ms: 10,
        provider_request_id: None,
        retries: 0,
        over_reservation: false,
        confidence_mismatch: false,
        estimated_input_tokens: Some(1000),
    }
}

/// Crawl-Datei mit protokollierten KI-Kosten (Zeilen in `ai_requests`) und optional gespeicherten Grenzen, geöffnet im
/// Fenster `main`.
async fn open_crawl_with_costs(
    app: &App,
    dir: &std::path::Path,
    costs: &[u64],
    limits: Option<BudgetLimits>,
) {
    let path = dir.join("u01-cost.ospider");
    let file = common::create(&path, common::cfg());
    file.set_meta("state", "Completed").unwrap();
    file.conn()
        .execute(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES (1, '2026-11-26T10:00:00.000Z', '{}')",
            [],
        )
        .unwrap();
    for (i, c) in costs.iter().enumerate() {
        requests::insert(
            file.conn(),
            &record(
                if i % 2 == 0 {
                    "decision"
                } else {
                    "suggest-title"
                },
                *c,
            ),
        )
        .unwrap();
    }
    if let Some(l) = limits {
        os_store::ai::budget::save_limits(file.conn(), &l).unwrap();
    }
    drop(file);
    common::open(app, &path).await;
}

async fn budget(app: &App) -> Value {
    call(app, "ai.costBudget", Value::Null).await
}

fn estimate(task: &str, expected: u64, executable: Executable) -> TaskEstimate {
    TaskEstimate {
        task: task.into(),
        model: "gpt-6-luna".into(),
        items: 22,
        cached: 3,
        est_input_tokens: 9_500,
        est_output_tokens: 11_264,
        low: Some(MicroUsd(expected * 3 / 4)),
        expected: Some(MicroUsd(expected)),
        high: Some(MicroUsd(expected * 5 / 4)),
        batch_savings: Some(MicroUsd(expected / 2)),
        executable,
        calibrated: false,
    }
}

/// Meldet einen Schätzer an, der für jeden Lauf `entries` liefert (wie ein Fachpaket, AI-012 „Ein Kostenvoranschlag“).
fn estimator(app: &App, entries: Vec<EstimateEntry>) {
    register_estimator(
        app,
        Arc::new(move |_app: App, _ctx: CallCtx, _args: AiRunArgs| {
            let e = entries.clone();
            async move { Ok::<_, CmdError>(e) }.boxed()
        }),
    );
}

async fn cost_estimate(app: &App) -> Value {
    call(
        app,
        "ai.costEstimate",
        json!({ "tasks": [], "sample": null, "budgetMicroUsd": null }),
    )
    .await
}

// ---------------------------------------------------------------------------------------------------------------
// UI-013 [M4] / AI-013: Feld `Kosten`

// spec: UI-013 [M4]
// acceptance: independent
// Ohne Crawl-Datei: Grenzen der Konfiguration (Standard-Crawl-Budget 5,00 USD nach E-5 a), kein Verbrauch.
#[tokio::test]
async fn ui013_ohne_crawl_standardbudget_und_kein_verbrauch() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    preview_on(&app).await;
    let b = budget(&app).await;
    assert_eq!(b["spentMicroUsd"], 0, "{b}");
    assert_eq!(b["budgetMicroUsd"], 5 * USD, "Standard 5,00 USD: {b}");
    assert_eq!(b["aiMicroUsd"], 0);
    assert_eq!(b["dataForSeoMicroUsd"], 0);
    assert_eq!(b["reservedMicroUsd"], 0);
}

// spec: UI-013 [M4]
// spec: AI-013 [M4]
// acceptance: independent
// Prüfwert aus UI-013: verbraucht 3,21 USD bei Budget 5 USD. Der Verbrauch kommt aus dem Protokoll der geöffneten
// Crawl-Datei (Summe `Cost (USD)` aus `ai_requests`), der KI-Anteil des Tooltips stimmt auf 1 µUSD mit dem Protokoll
// überein (AI-014).
#[tokio::test]
async fn ui013_geoeffneter_crawl_zeigt_verbrauch_3_21_von_5_usd_aus_dem_ledger() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    preview_on(&app).await;
    open_crawl_with_costs(&app, home.path(), &[3_000_000, 200_000, 10_000], None).await;
    let b = budget(&app).await;
    assert_eq!(b["spentMicroUsd"], 3_210_000, "{b}");
    assert_eq!(b["budgetMicroUsd"], 5 * USD, "{b}");
    assert_eq!(
        b["aiMicroUsd"], 3_210_000,
        "KI-Anteil = Summe des Protokolls: {b}"
    );
    assert_eq!(b["dataForSeoMicroUsd"], 0, "{b}");
    assert_eq!(b["reservedMicroUsd"], 0, "{b}");
}

// spec: UI-013 [M4]
// spec: AI-013 [M4]
// acceptance: independent
// In der Crawl-Datei gespeicherte Grenzen gelten (Budget 10 USD); bei „Unbegrenzt“ hat das Feld kein Budget
// (`Kosten: 3,2100 USD (unbegrenzt)`).
#[tokio::test]
async fn ui013_grenzen_der_crawl_datei_und_unbegrenzt() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    let limits = BudgetLimits {
        crawl: MicroUsd(10 * USD),
        ..BudgetLimits::default()
    };
    open_crawl_with_costs(&app, home.path(), &[3_210_000], Some(limits)).await;
    let b = budget(&app).await;
    assert_eq!(b["budgetMicroUsd"], 10 * USD, "{b}");
    assert_eq!(b["spentMicroUsd"], 3_210_000);

    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    let unlimited = BudgetLimits {
        unlimited: true,
        ..BudgetLimits::default()
    };
    open_crawl_with_costs(&app, home.path(), &[3_210_000], Some(unlimited)).await;
    let b = budget(&app).await;
    assert!(
        b["budgetMicroUsd"].is_null(),
        "unbegrenzt → kein Budget: {b}"
    );
    assert_eq!(b["spentMicroUsd"], 3_210_000);
}

// spec: AI-013 [M4]
// acceptance: independent
// „Budget erhöhen und fortsetzen“ ändert die Grenzen der geöffneten Crawl-Datei (nur über `raise_limits`); ohne Crawl gibt
// es nichts zu erhöhen. Der Verbrauch bleibt.
#[tokio::test]
async fn ai013_budget_erhoehen_wirkt_auf_die_crawl_datei_und_verlangt_einen_crawl() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    let raise = json!({ "raiseBudget": { "crawl": 8 * USD, "unlimited": false, "areas": {}, "tasks": {}, "warning_percent": 80 } });
    let err = try_call(&app, "ai.settingsSet", raise.clone())
        .await
        .expect_err("ohne geöffneten Crawl");
    assert_eq!(err.code, ErrorCode::NoCrawl, "{err:?}");

    open_crawl_with_costs(&app, home.path(), &[4_900_000], None).await;
    assert_eq!(budget(&app).await["budgetMicroUsd"], 5 * USD);
    call(&app, "ai.settingsSet", raise).await;
    let b = budget(&app).await;
    assert_eq!(b["budgetMicroUsd"], 8 * USD, "{b}");
    assert_eq!(b["spentMicroUsd"], 4_900_000, "{b}");
    let view = call(&app, "ai.settingsGet", Value::Null).await;
    assert_eq!(
        view["crawlBudget"]["crawl"],
        8 * USD,
        "gespeichert in der Crawl-Datei: {}",
        view["crawlBudget"]
    );
}

// ---------------------------------------------------------------------------------------------------------------
// AI-012: Dialog „Kostenvoranschlag“

// spec: AI-012 [M4]
// acceptance: independent
// Abschnitt „KI“: je Aufgabe `Task`, `Model`, `Reasoning`, `Items`, `Cached`, `Est. Input Tokens`, `Est. Output Tokens`,
// `Est. Cost (low/expected/high)`, `Batch Savings`, `Executable` (ja / Grund) mit genau den Werten der Kernschätzung;
// Summe = Summe der ausführbaren Aufgaben; Crawl-Budget, Verbrauch und Rest aus dem `BudgetLedger`.
#[tokio::test]
async fn ai012_abschnitt_ki_zeigt_die_werte_des_kern_voranschlags() {
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    let title = estimate("suggest-title", 40_000, Executable::Yes);
    let decision = estimate("decision", 60_000, Executable::Yes);
    let missing = estimate("ask-sql", 90_000, Executable::MissingApiKey);
    estimator(
        &app,
        vec![
            (title.clone(), Some(ReasoningLevel::Low), Some(22)),
            (decision.clone(), None, Some(5)),
            (missing.clone(), Some(ReasoningLevel::Medium), None),
        ],
    );
    open_crawl_with_costs(&app, home.path(), &[1_000_000], None).await;
    let v = cost_estimate(&app).await;
    let sections = v["sections"].as_array().unwrap();
    let ai = sections
        .iter()
        .find(|s| s["id"] == "ai")
        .expect("Abschnitt KI");
    let rows = ai["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for (core, reasoning, r) in [
        (&title, json!("low"), &rows[0]),
        (&decision, Value::Null, &rows[1]),
        (&missing, json!("medium"), &rows[2]),
    ] {
        assert_eq!(r["task"], core.task.as_str());
        assert_eq!(r["model"], core.model.as_str());
        assert_eq!(
            r["reasoning"], reasoning,
            "Spalte Reasoning = effektive Stufe: {r}"
        );
        assert_eq!(r["items"], core.items);
        assert_eq!(r["cached"], core.cached);
        assert_eq!(r["estInputTokens"], core.est_input_tokens);
        assert_eq!(r["estOutputTokens"], core.est_output_tokens);
        assert_eq!(r["lowMicroUsd"], core.low.unwrap().0);
        assert_eq!(r["expectedMicroUsd"], core.expected.unwrap().0);
        assert_eq!(r["highMicroUsd"], core.high.unwrap().0);
        assert_eq!(r["batchSavingsMicroUsd"], core.batch_savings.unwrap().0);
    }
    assert_eq!(rows[0]["executable"], true);
    assert_eq!(rows[0]["reason"], "");
    assert_eq!(
        rows[2]["executable"], false,
        "ohne Schlüssel nicht ausführbar"
    );
    assert_eq!(rows[2]["reason"], "Missing API key");
    // Summe nur über ausführbare Aufgaben (nicht ausführbare senden nichts).
    assert_eq!(v["totals"]["expectedMicroUsd"], 100_000, "{}", v["totals"]);
    assert_eq!(v["totals"]["lowMicroUsd"], 30_000 + 45_000);
    assert_eq!(v["totals"]["highMicroUsd"], 50_000 + 75_000);
    assert_eq!(
        ai["totals"], v["totals"],
        "ohne DataForSEO = Summe des Abschnitts KI"
    );
    // Budget, Verbrauch und Rest aus dem Ledger der Crawl-Datei.
    assert_eq!(v["budget"]["budgetMicroUsd"], 5 * USD);
    assert_eq!(v["budget"]["consumedMicroUsd"], USD);
    assert_eq!(v["budget"]["remainingMicroUsd"], 4 * USD);
}

// spec: AI-012 [M4]
// acceptance: independent
// Bestätigung verlangen ab 1,00 USD erwarteter Kosten (Standard, E-5 a) oder über 50 % des Restbudgets; darunter kein
// Dialog. Eine Aufgabe ohne Preis erscheint mit `Executable = nein` und Grund `Price unknown`.
#[tokio::test]
async fn ai012_bestaetigungsschwelle_und_halbes_restbudget() {
    // Unter der Schwelle (0,90 USD) und unter 50 % von 5,00 USD → keine Bestätigung.
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    estimator(
        &app,
        vec![(estimate("decision", 900_000, Executable::Yes), None, None)],
    );
    let v = cost_estimate(&app).await;
    assert_eq!(v["thresholdMicroUsd"], USD, "Standard 1,00 USD");
    assert_eq!(v["confirmationRequired"], false, "{v}");

    // Über der Schwelle (1,10 USD) → Bestätigung.
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    estimator(
        &app,
        vec![(estimate("decision", 1_100_000, Executable::Yes), None, None)],
    );
    assert_eq!(cost_estimate(&app).await["confirmationRequired"], true);

    // Unter der Schwelle, aber über 50 % des Restbudgets (0,80 USD von 1,50 USD).
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    edit_config(&app, |cfg| cfg.ai.budget.crawl = MicroUsd(1_500_000)).await;
    estimator(
        &app,
        vec![(estimate("decision", 800_000, Executable::Yes), None, None)],
    );
    let v = cost_estimate(&app).await;
    assert_eq!(
        v["budget"]["remainingMicroUsd"], 1_500_000,
        "{}",
        v["budget"]
    );
    assert_eq!(v["confirmationRequired"], true, "über 50 % des Restbudgets");

    // Schwelle aus den App-Einstellungen (Testprofil 0,0001 USD aus AI-012).
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    patch_settings(&app, json!({ "aiApp": { "confirmationThreshold": 100 } })).await;
    estimator(
        &app,
        vec![(estimate("decision", 200, Executable::Yes), None, None)],
    );
    let v = cost_estimate(&app).await;
    assert_eq!(v["thresholdMicroUsd"], 100);
    assert_eq!(v["confirmationRequired"], true);

    // Ohne Preis: nicht ausführbar, Grund „Price unknown“.
    let home = tempfile::tempdir().unwrap();
    let app = fresh_app(home.path());
    let mut unknown = estimate("suggest-title", 0, Executable::PriceUnknown);
    unknown.low = None;
    unknown.expected = None;
    unknown.high = None;
    unknown.batch_savings = None;
    estimator(&app, vec![(unknown, Some(ReasoningLevel::Off), None)]);
    let v = cost_estimate(&app).await;
    let r = &v["sections"][0]["rows"][0];
    assert_eq!(r["executable"], false);
    assert_eq!(r["reason"], "Price unknown");
    assert!(r["expectedMicroUsd"].is_null(), "{r}");
}

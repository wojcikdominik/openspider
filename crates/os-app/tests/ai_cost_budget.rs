//! M4-U01: Statusleistenfeld `Kosten` (UI-013 [M4]) und Dialog „Kostenvoranschlag“ (AI-012) in der Service-Schicht.
//! Fixtures: Crawl-Datei mit Zeilen in `ai_requests` (wie `os-store/tests/ai_budget.rs`), die gemeinsame
//! Dialog-Fixture `apps/desktop/e2e/ai-cost/fixtures/estimate.json` (dieselbe Datei prüfen Vitest und Playwright).
// Der Aufbau der Fixture-Crawl-Datei geht direkt an `os-store` (wie in den übrigen Fixture-Tests dieses Crates).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::future::BoxFuture;
use os_ai::cost::{Executable, TaskEstimate};
use os_ai::provider::ReasoningLevel;
use os_app::commands::ai_cost::{
    cost_snapshot, estimate_view, register_estimator, snapshot_without_crawl, EstimateEntry,
    EstimateSection, Estimator, Totals,
};
use os_app::dto::ai::AiRunArgs;
use os_app::{App, AppConfig, CallCtx, CmdError, ErrorCode, WindowId};
use os_model::budget::{BudgetLedger, BudgetLimits, MicroUsd};
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::ai::requests::{insert, total_cost, PriceSource, RequestRecord};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};
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

fn row(cost: u64) -> RequestRecord {
    RequestRecord {
        run_id: 1,
        timestamp: "2026-11-26T08:01:00Z".into(),
        task: "suggest-title".into(),
        provider: "anthropic".into(),
        model: "mock-claude".into(),
        endpoint: "messages".into(),
        url_id: Some(1),
        definition_version: Some("1.0.0".into()),
        reservation: MicroUsd(cost),
        redactions: 0,
        input_tokens: Some(1000),
        cached_input_tokens: None,
        cache_write_tokens: None,
        output_tokens: Some(200),
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
        provider_request_id: Some("req-u01".into()),
        retries: 0,
        over_reservation: false,
        confidence_mismatch: false,
        estimated_input_tokens: Some(1000),
    }
}

/// Crawl-Datei mit KI-Verbrauch 3,21 USD (1,00 + 2,21) und Budget aus der Konfiguration (Standard 5,00 USD).
fn crawl_with_spend(dir: &Path) -> PathBuf {
    let created = Timestamp::from_db("2026-11-26T08:00:00.000Z").unwrap();
    let path = dir.join("u01.ospider");
    let init = CrawlInit {
        crawl_id: SeededIds::new(11).new_crawl_id(created),
        mode: CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "U01".into(),
        name: "cost".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    let f = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    f.conn()
        .execute_batch(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-26T08:00:00Z', '{}');
             INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length)
               VALUES(1,'https://ai.test:8080/','https://ai.test:8080/','https://ai.test:8080',1,'Done','Seed',0,21);",
        )
        .unwrap();
    insert(f.conn(), &row(1_000_000)).unwrap();
    insert(f.conn(), &row(2_210_000)).unwrap();
    drop(f);
    path
}

// spec: UI-013 [M4]
// spec: AI-013
// Verbraucht 3,21 USD bei Budget 5 USD: der Ledger der Crawl-Datei (nur über `open_ledger`) ergibt die Rohwerte des Felds
// (3.210.000 / 5.000.000 µUSD); der KI-Anteil des Tooltips stimmt mit der Summe des Protokolls auf 1 µUSD überein (AI-014).
#[test]
fn snapshot_of_the_crawl_file_carries_spent_budget_and_split() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_with_spend(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let ledger = os_store::ai::budget::open_ledger(f.conn(), BudgetLimits::default(), &[]).unwrap();
    let s = cost_snapshot(&ledger.snapshot().unwrap());
    assert_eq!(s.spent_micro_usd, 3_210_000);
    assert_eq!(s.budget_micro_usd, Some(5_000_000));
    assert_eq!(s.ai_micro_usd, 3_210_000);
    assert_eq!(s.data_for_seo_micro_usd, 0);
    assert_eq!(s.reserved_micro_usd, 0);
    assert_eq!(total_cost(f.conn()).unwrap().0, s.ai_micro_usd);

    // „Unbegrenzt“: kein Budget im DTO (Anzeige `Kosten: 3,2100 USD (unbegrenzt)`).
    let unlimited = BudgetLimits {
        unlimited: true,
        ..BudgetLimits::default()
    };
    let s = cost_snapshot(&snapshot_without_crawl(unlimited));
    assert_eq!((s.spent_micro_usd, s.budget_micro_usd), (0, None));
}

// spec: UI-013 [M4]
// `ai.costBudget` über den Command-Weg: mit geöffneter Crawl-Datei deren Ledger, ohne Crawl die Grenzen der Konfiguration
// des Fensters (Standard 5,00 USD, E-5 a) ohne Verbrauch.
#[tokio::test]
async fn cost_budget_command_reads_the_window_crawl() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let none = app
        .dispatch(ctx(), "ai.costBudget", Value::Null)
        .await
        .unwrap();
    assert_eq!(none["spentMicroUsd"], json!(0));
    assert_eq!(none["budgetMicroUsd"], json!(5_000_000));

    let path = crawl_with_spend(dir.path());
    app.dispatch(ctx(), "crawl.open", json!({"kind": "path", "path": path}))
        .await
        .expect("crawl.open");
    let s = app
        .dispatch(ctx(), "ai.costBudget", Value::Null)
        .await
        .unwrap();
    assert_eq!(
        s,
        json!({
            "spentMicroUsd": 3_210_000,
            "budgetMicroUsd": 5_000_000,
            "aiMicroUsd": 3_210_000,
            "dataForSeoMicroUsd": 0,
            "reservedMicroUsd": 0
        })
    );

    // „Budget erhöhen“ speichert über `raise_limits` in der Crawl-Datei; danach gilt das neue Budget.
    let raised = app
        .dispatch(
            ctx(),
            "ai.settingsSet",
            json!({ "raiseBudget": { "crawl": 8_000_000, "unlimited": false, "areas": {}, "tasks": {}, "warning_percent": 80 } }),
        )
        .await
        .unwrap();
    assert_eq!(
        raised["lifted"],
        json!(false),
        "ohne Sperre nichts aufzuheben"
    );
    let s = app
        .dispatch(ctx(), "ai.costBudget", Value::Null)
        .await
        .unwrap();
    assert_eq!(s["budgetMicroUsd"], json!(8_000_000));
}

// spec: UI-013 [M4]
#[tokio::test]
async fn raise_budget_without_a_crawl_is_no_crawl() {
    let home = tempfile::tempdir().unwrap();
    let err = app(home.path())
        .dispatch(
            ctx(),
            "ai.settingsSet",
            json!({ "raiseBudget": BudgetLimits::default() }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NoCrawl);
}

#[allow(clippy::too_many_arguments)]
fn estimate(
    task: &str,
    model: &str,
    items: u64,
    cached: u64,
    tokens: (u64, u64),
    cost: Option<(u64, u64, u64)>,
    batch: Option<u64>,
    executable: Executable,
) -> TaskEstimate {
    TaskEstimate {
        task: task.into(),
        model: model.into(),
        items,
        cached,
        est_input_tokens: tokens.0,
        est_output_tokens: tokens.1,
        low: cost.map(|c| MicroUsd(c.0)),
        expected: cost.map(|c| MicroUsd(c.1)),
        high: cost.map(|c| MicroUsd(c.2)),
        batch_savings: batch.map(MicroUsd),
        executable,
        calibrated: false,
    }
}

fn fixture_entries() -> Vec<(TaskEstimate, Option<ReasoningLevel>, Option<u64>)> {
    vec![
        (
            estimate(
                "suggest-title",
                "mock-claude-b",
                3,
                0,
                (1200, 3072),
                Some((7500, 10_000, 12_500)),
                None,
                Executable::Yes,
            ),
            Some(ReasoningLevel::Low),
            Some(3),
        ),
        (
            estimate(
                "decision",
                "gpt-6-luna",
                22,
                2,
                (11_000, 0),
                Some((825, 1100, 1375)),
                Some(550),
                Executable::Yes,
            ),
            None,
            Some(20),
        ),
        (
            estimate(
                "ask-sql",
                "claude-sonnet-5-5",
                1,
                0,
                (400, 1536),
                None,
                None,
                Executable::MissingApiKey,
            ),
            Some(ReasoningLevel::Medium),
            Some(1),
        ),
    ]
}

fn fixture_snapshot() -> os_model::budget::BudgetSnapshot {
    let mut s = snapshot_without_crawl(BudgetLimits::default());
    s.spent = MicroUsd(3_210_000);
    s.remaining = Some(MicroUsd(1_790_000));
    s
}

// spec: AI-012 [M4]
// Abschnitt „KI“ mit Spalte `Reasoning` (effektive Stufe, leer über Decisions), Summe nur der ausführbaren Aufgaben,
// `Executable = nein` mit Grund, Crawl-Budget/Verbrauch/Rest; Testprofil mit Bestätigungsschwelle 0,0001 USD → Bestätigung
// nötig. Die Form ist die gemeinsame Fixture der Oberflächentests.
#[test]
fn estimate_view_matches_the_shared_dialog_fixture() {
    let view = estimate_view(&fixture_entries(), &fixture_snapshot(), MicroUsd(100));
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(repo("apps/desktop/e2e/ai-cost/fixtures/estimate.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(serde_json::to_value(&view).unwrap(), expected);
}

// spec: AI-012 [M4]
// Bestätigungsregel: über der Schwelle oder über 50 % des Restbudgets (gemeinsame erwartete Summe).
#[test]
fn confirmation_follows_threshold_and_half_of_the_remaining_budget() {
    let entries = fixture_entries();
    let mut snap = fixture_snapshot();
    assert!(!estimate_view(&entries, &snap, MicroUsd(1_000_000)).confirmation_required);
    snap.remaining = Some(MicroUsd(22_199));
    assert!(estimate_view(&entries, &snap, MicroUsd(1_000_000)).confirmation_required);
    snap.remaining = Some(MicroUsd(22_200));
    assert!(!estimate_view(&entries, &snap, MicroUsd(1_000_000)).confirmation_required);
}

// spec: AI-012 [M4]
// Ein weiterer Abschnitt (M5: DataForSEO) ändert die gemeinsame Summenzeile: niedrig += Min, erwartet und hoch += Max
// (Prüfwerte aus AI-012 [M5]: KI 0,0300/0,0500, DFS 0,1200/0,2400 → 0,1500/0,2900); der Dialog bleibt einer.
#[test]
fn a_further_section_extends_the_common_total() {
    let ai = vec![(
        estimate(
            "decision",
            "gpt-6-luna",
            1,
            0,
            (1, 0),
            Some((30_000, 40_000, 50_000)),
            None,
            Executable::Yes,
        ),
        None,
        Some(1),
    )];
    let snap = snapshot_without_crawl(BudgetLimits::default());
    let view = estimate_view(&ai, &snap, MicroUsd(1_000_000)).with_section(
        EstimateSection {
            id: "dataforseo".into(),
            rows: vec![],
            totals: Totals {
                low_micro_usd: 120_000,
                expected_micro_usd: 240_000,
                high_micro_usd: 240_000,
            },
        },
        MicroUsd(1_000_000),
        snap.remaining,
    );
    assert_eq!(view.sections.len(), 2);
    assert_eq!(view.totals.low_micro_usd, 150_000);
    assert_eq!(view.totals.high_micro_usd, 290_000);
}

// spec: AI-012 [M4]
// `ai.costEstimate` sammelt die Zeilen der angemeldeten Schätzer; ohne Schätzer ein leerer Abschnitt „KI“ und keine
// Bestätigung. Die Schätzung sendet nichts (kein Anbieteraufruf im Command-Weg).
#[tokio::test]
async fn cost_estimate_collects_registered_estimators() {
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path());
    let empty = app
        .dispatch(ctx(), "ai.costEstimate", json!({ "tasks": [] }))
        .await
        .unwrap();
    assert_eq!(empty["sections"][0]["id"], json!("ai"));
    assert_eq!(empty["sections"][0]["rows"], json!([]));
    assert_eq!(empty["confirmationRequired"], json!(false));
    assert_eq!(empty["thresholdMicroUsd"], json!(1_000_000));

    let estimator: Estimator = Arc::new(|_: App, _: CallCtx, _: AiRunArgs| {
        let f: BoxFuture<'static, Result<Vec<EstimateEntry>, CmdError>> =
            Box::pin(async { Ok(fixture_entries()) });
        f
    });
    register_estimator(&app, estimator);
    let view = app
        .dispatch(ctx(), "ai.costEstimate", json!({ "tasks": [] }))
        .await
        .unwrap();
    assert_eq!(view["sections"][0]["rows"].as_array().unwrap().len(), 3);
    assert_eq!(view["sections"][0]["rows"][0]["reasoning"], json!("low"));
    assert_eq!(view["totals"]["expectedMicroUsd"], json!(11_100));
    assert_eq!(view["budget"]["budgetMicroUsd"], json!(5_000_000));
}

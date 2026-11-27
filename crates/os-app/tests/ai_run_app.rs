//! M4-D05: KI-Lauf in der App gegen den ai-mock (Schlüssel nur erfunden, Basis-URLs über die App-Einstellungen).
//! `ai.decisionsRun` (unterbrochener und fremd geführter Crawl, Schalter aus, Kostenbestätigung), `ai.costBudget` mit den
//! offenen Reservierungen des laufenden Laufs, `ai.budget` im Konfigurationsdialog an der geöffneten Crawl-Datei,
//! Schließen der Crawl-Datei bei gehaltenem Request.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "ai_run_support.rs"]
mod support;

use std::time::{Duration, Instant};

use os_app::dto::{CrawlConfigDto, OpState};
use os_app::{ErrorCode, WindowId};
use os_model::budget::MicroUsd;
use os_model::CrawlConfig;
use os_store::{CrawlFile, OpenMode};
use serde_json::{json, Value};
use support::*;

// spec: AI-020 [M4]
// spec: AI-039 [M4]
// Unterbrochener Crawl (gespeichert `Running`, `owner_pid` gesetzt, Heartbeat 11 s alt): `ai.decisionsRun` lehnt mit
// „Crawl ist unterbrochen – zuerst fortsetzen oder stoppen“ (en „Crawl was interrupted – resume or stop it first“) ab, die
// Datei ist byte-gleich, der ai-mock zählt 0 Requests.
#[tokio::test]
async fn interrupted_crawl_is_rejected_by_the_command() {
    let (_dir, mock, app, path) = setup(NO_CONFIRMATION).await;
    set_meta(
        &path,
        &[
            ("state", "Running".into()),
            ("owner_kind", "app".into()),
            ("owner_pid", "424242".into()),
            ("heartbeat_at", iso(-11_000)),
        ],
    );
    let before = std::fs::read(&path).unwrap();
    let err = call(
        &app,
        "ai.decisionsRun",
        json!({"tasks": [], "confirmed": true}),
    )
    .await
    .unwrap_err();
    assert_eq!(err.message_key, "err-ai-run-interrupted");
    assert_eq!(
        err.detail.as_deref(),
        Some("Crawl ist unterbrochen – zuerst fortsetzen oder stoppen")
    );
    assert_eq!(
        err.args.get("en"),
        Some(&json!("Crawl was interrupted – resume or stop it first"))
    );
    assert_ne!(
        err.code,
        ErrorCode::Busy,
        "kein busy (ein späterer Aufruf gelingt nicht ohne Zutun)"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before, "Datei byte-gleich");
    assert_eq!(mock.log().len(), 0);
}

// spec: AI-020 [M4]
// Ersetzt den Kernteil von `decisions_pending::foreign_writer_is_read_only` und den Teil „ihr ai-mock zählt 0 Requests“ des
// Kriteriums „Besitzer eines KI-Laufs“ (K4-13): Crawl-Datei mit fremdem aktivem Besitzer (`cli` bzw. `mcp-stdio`, aktueller
// Heartbeat), alle Schlüssel verbunden → `ai.decisionsRun` lehnt ab (`busy`, „gestartet von … (PID …)“), 0 Requests.
#[tokio::test]
async fn foreign_writer_is_rejected_and_sends_nothing() {
    let (_dir, mock, app, path) = setup(NO_CONFIRMATION).await;
    for kind in ["cli", "mcp-stdio"] {
        set_meta(
            &path,
            &[
                ("owner_kind", kind.into()),
                ("owner_pid", "424242".into()),
                ("heartbeat_at", iso(0)),
            ],
        );
        let err = call(
            &app,
            "ai.decisionsRun",
            json!({"tasks": [], "confirmed": true}),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::Busy);
        assert_eq!(err.message_key, "err-ai-run-foreign");
        assert_eq!(err.args.get("kind"), Some(&json!(kind)));
        assert_eq!(err.args.get("pid"), Some(&json!("424242")));
    }
    assert_eq!(mock.log().len(), 0);
}

// spec: AI-001 [M4]
// Schalter „KI (Vorschau)“ aus: kein Lauf, kein Request, keine KI-Warteschlange für die Engine.
#[tokio::test]
async fn switch_off_runs_nothing() {
    let (_dir, mock, app, _path) = setup(NO_CONFIRMATION).await;
    call(
        &app,
        "settings.set",
        json!({"patch": {"aiApp": {"previewEnabled": false}}}),
    )
    .await
    .unwrap();
    let err = call(
        &app,
        "ai.decisionsRun",
        json!({"tasks": [], "confirmed": true}),
    )
    .await
    .unwrap_err();
    assert_eq!(err.message_key, "err-ai-preview-off");
    assert!(os_app::commands::ai_run::app_queue(&app).is_none());
    assert_eq!(mock.log().len(), 0);
}

// spec: AI-012 [M4]
// spec: AI-020 [M4]
// Kostenbestätigung über den Command: Schwelle 0,0001 USD, ohne Bestätigung → kein Request, Einträge
// `Pending (confirmation)`; mit Bestätigung → `confirmed_at` gesetzt, 22 Requests.
#[tokio::test]
async fn no_request_before_confirmation_then_all_with_it() {
    // Schwelle 0: jeder Voranschlag mit Einträgen ist bestätigungspflichtig (≥, AI-012 Nachtrag 26.11.2026).
    let (_dir, mock, app, path) = setup(0).await;
    let op = call(
        &app,
        "ai.decisionsRun",
        json!({"tasks": [], "confirmed": false}),
    )
    .await
    .unwrap();
    let status = wait_op(&app, &op).await;
    assert_eq!(status.state, OpState::Done, "{status:?}");
    assert_eq!(
        status.result.as_ref().unwrap()["end"],
        "NeedsConfirmation",
        "{status:?}"
    );
    assert_eq!(mock.decisions(), 0, "kein Request vor „Starten“");
    assert_eq!(counts(&path).get("Pending (confirmation)"), Some(&22));
    let op = call(
        &app,
        "ai.decisionsRun",
        json!({"tasks": [], "confirmed": true}),
    )
    .await
    .unwrap();
    let status = wait_op(&app, &op).await;
    let result = status.result.expect("Ergebnis");
    assert_eq!(result["end"], "Completed");
    assert_eq!(mock.decisions(), 22);
    assert_eq!(counts(&path).get("Success"), Some(&22));
    let f = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let run_id = result["runId"].as_i64().unwrap();
    assert!(os_store::ai::answers::confirmed_at(f.conn(), run_id)
        .unwrap()
        .is_some());
}

// spec: AI-013 [M4]
// Befund B3-2 der Abnahme U01: `ai.costBudget` zeigt während eines gehaltenen Requests die offene Reservierung des
// laufenden KI-Laufs (gemeinsame Quelle des Ledgers mit dem Lauf); danach keine mehr.
#[tokio::test]
async fn cost_budget_shows_open_reservations_of_a_running_run() {
    let (_dir, mock, app, _path) = setup(NO_CONFIRMATION).await;
    mock.hold(1).await;
    let op = call(
        &app,
        "ai.decisionsRun",
        json!({"tasks": [], "confirmed": true}),
    )
    .await
    .unwrap();
    mock.until("gehalten", |m| m.held() >= 1).await;
    let snapshot = call(&app, "ai.costBudget", Value::Null).await.unwrap();
    assert!(
        snapshot["reservedMicroUsd"].as_u64().unwrap() > 0,
        "{snapshot}"
    );
    mock.release(1).await;
    wait_op(&app, &op).await;
    let snapshot = call(&app, "ai.costBudget", Value::Null).await.unwrap();
    assert_eq!(snapshot["reservedMicroUsd"], 0);
    assert!(snapshot["aiMicroUsd"].as_u64().unwrap() > 0);
}

// spec: AI-013 [M4]
// Budget im Konfigurationsdialog bei geöffneter Crawl-Datei: `config.set` mit neuem `ai.budget` → `open_ledger` liefert die
// neue Grenze; eine Erhöhung hebt eine Sperre nach einer Überschreitung auf.
#[tokio::test]
async fn budget_change_in_the_config_dialog_reaches_the_open_crawl_file() {
    let (_dir, _mock, app, path) = setup(NO_CONFIRMATION).await;
    {
        // Eine Überschreitung im Protokoll sperrt den Ledger (AI-013).
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        let run =
            os_store::ai::answers::begin_run(f.conn(), "2026-11-27T10:00:00Z", None, "{}").unwrap();
        f.conn()
            .execute(
                "INSERT INTO ai_requests (run_id, timestamp, task, provider, model, endpoint, token_cost_microusd, cost_microusd, \
                 price_source, status, duration_ms, over_reservation) VALUES (?1, '2026-11-27T10:00:01Z', 'decision', 'openai', \
                 'gpt-6-luna', 'decisions', 900, 900, 'table', 'Success', 5, 1)",
                [run],
            )
            .unwrap();
        let ledger =
            os_store::ai::budget::open_ledger(f.conn(), CrawlConfig::default().ai.budget, &[])
                .unwrap();
        assert!(
            os_model::budget::BudgetLedger::snapshot(&ledger)
                .unwrap()
                .halted
        );
    }
    let dto = call(&app, "config.get", Value::Null).await.unwrap();
    let mut cfg: CrawlConfig = serde_json::from_value(dto["config"].clone()).unwrap();
    cfg.ai.budget.crawl = MicroUsd(9_000_000);
    let dto = serde_json::to_value(CrawlConfigDto {
        schema_version: cfg.schema_version,
        generator: cfg.generator.clone(),
        config: serde_json::to_value(&cfg).unwrap(),
    })
    .unwrap();
    call(&app, "config.set", dto).await.unwrap();
    let f = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let ledger =
        os_store::ai::budget::open_ledger(f.conn(), CrawlConfig::default().ai.budget, &[]).unwrap();
    let snapshot = os_model::budget::BudgetLedger::snapshot(&ledger).unwrap();
    assert_eq!(snapshot.limits.crawl, MicroUsd(9_000_000));
    assert!(!snapshot.halted, "Erhöhung hebt die Sperre auf");
}

// spec: AI-020 [M4]
// spec: CRAWL-126 [M4]
// Schließen der Crawl-Datei bei gehaltenem Request: der Request wird abgebrochen, nicht abgewartet (Zeitbudget
// CRAWL-126: 3 s); Eintrag `Pending`, keine Protokollzeile, Reservierung verfallen.
#[tokio::test]
async fn closing_the_crawl_file_aborts_a_held_request() {
    let (_dir, mock, app, path) = setup(NO_CONFIRMATION).await;
    let op = held_run(&app, &mock).await;
    let started = Instant::now();
    os_app::session::close(&app, &WindowId("main".into()), false);
    let status = wait_op(&app, &op).await;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(status.result.unwrap()["end"], "Aborted");
    assert_like_a_crash(&path);
}

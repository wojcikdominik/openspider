//! Unabhängige Abnahmetests M4-K10: Protokoll `ai_requests` nach dem Nachtrag 24.11.2026, Q00 (AI-014).
//!
//! Neue Spalten `Reasoning Level` (nach `Endpoint`), `Reasoning Tokens` (nach `Output Tokens`), `Computed Cost (USD)`
//! (nach `Cost (USD)`), Kennzeichen `cost_mismatch`, `Price Source = reported`; Speicherung in Migration 0006 auf einer
//! frischen Datei und auf einer Datei aus Schemaversion 5 (Archiv-Fixture). Die Spec verlangt keine Migration einer
//! schon mit 0006 angelegten Datei (0006 steckt in keinem Release); das wird nicht geprüft.
// acceptance: independent

use std::path::{Path, PathBuf};

use os_model::budget::MicroUsd;
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::ai::requests::{
    insert, insert_with_details, list, output_calibration_samples, totals_by_task_and_model,
    PriceSource, RequestDetails, RequestFilter, RequestRecord, COLUMNS, REASONING_LEVELS,
};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile, SCHEMA_VERSION};

fn create(dir: &Path) -> PathBuf {
    let created = Timestamp::from_db("2026-11-26T09:00:00.000Z").unwrap();
    let path = dir.join("k10-acceptance.ospider");
    let init = CrawlInit {
        crawl_id: SeededIds::new(1010).new_crawl_id(created),
        mode: CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "Abnahme".into(),
        name: "K10".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    let f = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    add_run(&f);
    drop(f);
    path
}

fn add_run(f: &CrawlFile) {
    f.conn()
        .execute_batch(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-26T09:00:00Z', '{}');",
        )
        .unwrap();
}

fn record(cost: u64, source: PriceSource) -> RequestRecord {
    RequestRecord {
        run_id: 1,
        timestamp: "2026-11-26T09:01:00Z".into(),
        task: "suggest-title".into(),
        provider: "openrouter".into(),
        model: "openai/gpt-6-luna".into(),
        endpoint: "chat-completions".into(),
        url_id: None,
        definition_version: Some("1".into()),
        reservation: MicroUsd(10_000),
        redactions: 0,
        input_tokens: Some(1_000),
        cached_input_tokens: None,
        cache_write_tokens: None,
        output_tokens: Some(200),
        images: 0,
        token_cost: MicroUsd(cost),
        fixed_fees: MicroUsd(0),
        regional_uplift: MicroUsd(0),
        cost: MicroUsd(cost),
        price_source: source,
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
        estimated_input_tokens: Some(900),
    }
}

fn details(
    level: Option<&str>,
    reasoning: Option<u64>,
    computed: Option<u64>,
    mismatch: bool,
) -> RequestDetails {
    RequestDetails {
        reasoning_level: level.map(str::to_string),
        reasoning_tokens: reasoning,
        computed_cost: computed.map(MicroUsd),
        cost_mismatch: mismatch,
    }
}

// spec: AI-014 [M4]
// acceptance: independent
// Kanonische Spaltennamen und Reihenfolge: Grundliste aus AI-014, `Reasoning Level` nach `Endpoint`, `Reasoning Tokens`
// nach `Output Tokens`, `Computed Cost (USD)` nach `Cost (USD)`; `Refusal Reason` als eigene Spalte (Position nicht
// festgelegt).
#[test]
fn log_columns_follow_the_spec_order() {
    let expected = [
        "Timestamp",
        "Run",
        "Task",
        "Provider",
        "Model",
        "Endpoint",
        "Reasoning Level",
        "Address",
        "Question/Prompt Version",
        "Input Tokens",
        "Cached Input Tokens",
        "Cache Write Tokens",
        "Output Tokens",
        "Reasoning Tokens",
        "Images",
        "Token Cost (USD)",
        "Fixed Fees (USD)",
        "Regional Uplift (USD)",
        "Cost (USD)",
        "Computed Cost (USD)",
        "Price Source",
        "Batch",
        "Cache Hit",
        "Status",
        "HTTP Status",
        "Error Code",
        "Duration ms",
        "Provider Request ID",
        "Retries",
    ];
    let without_refusal: Vec<&str> = COLUMNS
        .iter()
        .copied()
        .filter(|c| *c != "Refusal Reason")
        .collect();
    assert_eq!(without_refusal, expected);
    assert!(COLUMNS.contains(&"Refusal Reason"));
    assert_eq!(
        REASONING_LEVELS,
        [
            "model-default",
            "off",
            "minimal",
            "low",
            "medium",
            "high",
            "xhigh",
            "max"
        ]
    );
}

// spec: AI-014 [M4]
// acceptance: independent
// Frische Crawl-Datei (Schemaversion 6): Zeilen mit allen neuen Werten werden gespeichert und unverändert gelesen; leere
// Werte bleiben leer (`null`), `Computed Cost` ohne Angabe = `Cost`; `Price Source = reported` ist zulässig.
#[test]
fn fresh_file_stores_the_new_columns() {
    assert_eq!(SCHEMA_VERSION, 6);
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    insert_with_details(
        f.conn(),
        &record(2_400, PriceSource::Reported),
        &details(Some("off"), Some(120), Some(1_200), true),
    )
    .unwrap();
    insert_with_details(
        f.conn(),
        &record(1_200, PriceSource::Api),
        &details(Some("model-default"), Some(0), None, false),
    )
    .unwrap();
    insert(f.conn(), &record(500, PriceSource::Table)).unwrap();
    drop(f);

    let f = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    let rows = list(f.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].record.price_source, PriceSource::Reported);
    assert_eq!(rows[0].details.reasoning_level.as_deref(), Some("off"));
    assert_eq!(rows[0].details.reasoning_tokens, Some(120));
    assert_eq!(
        rows[0].details.computed_cost_of(&rows[0].record),
        MicroUsd(1_200)
    );
    assert!(rows[0].details.cost_mismatch);
    assert_eq!(
        rows[1].details.reasoning_tokens,
        Some(0),
        "0 bleibt 0, nicht leer"
    );
    assert_eq!(
        rows[1].details.computed_cost_of(&rows[1].record),
        MicroUsd(1_200)
    );
    assert_eq!(rows[2].details.reasoning_level, None, "leer");
    assert_eq!(
        rows[2].details.reasoning_tokens, None,
        "leer, wenn nichts gemeldet"
    );
    assert_eq!(
        rows[2].details.computed_cost_of(&rows[2].record),
        MicroUsd(500)
    );
    assert!(!rows[2].details.cost_mismatch);
}

// spec: AI-014 [M4]
// acceptance: independent
// Regeln der Spalten: `Computed Cost` weicht nur bei `Price Source = reported` von `Cost` ab, `cost_mismatch` nur dort;
// `Reasoning Level` nur aus der Stufenmenge (auch auf Tabellenebene, CHECK der Migration).
#[test]
fn column_rules_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    for source in [
        PriceSource::Api,
        PriceSource::Table,
        PriceSource::User,
        PriceSource::Estimate,
    ] {
        assert!(
            insert_with_details(
                f.conn(),
                &record(100, source),
                &details(None, None, Some(90), false)
            )
            .is_err(),
            "{source:?} mit abweichendem Computed Cost"
        );
        assert!(
            insert_with_details(
                f.conn(),
                &record(100, source),
                &details(None, None, None, true)
            )
            .is_err(),
            "{source:?} mit cost_mismatch"
        );
        insert_with_details(
            f.conn(),
            &record(100, source),
            &details(None, None, Some(100), false),
        )
        .unwrap();
    }
    for bad in ["none", "Medium", "x-high", ""] {
        assert!(
            insert_with_details(
                f.conn(),
                &record(1, PriceSource::Table),
                &details(Some(bad), None, None, false)
            )
            .is_err(),
            "Stufe {bad:?}"
        );
    }
    for level in REASONING_LEVELS {
        insert_with_details(
            f.conn(),
            &record(1, PriceSource::Table),
            &details(Some(level), None, None, false),
        )
        .unwrap();
    }
    // Tabellenebene (Migration 0006): unbekannte Stufe und unbekannte Preisquelle werden abgewiesen.
    let raw = |level: &str, source: &str| {
        f.conn().execute(
            &format!(
                "INSERT INTO ai_requests(run_id, timestamp, task, provider, model, endpoint, token_cost_microusd,
                   cost_microusd, price_source, status, duration_ms, reasoning_level)
                 VALUES(1, '2026-11-26T09:02:00Z', 't', 'p', 'm', 'e', 0, 0, '{source}', 'Success', 0, {level})"
            ),
            [],
        )
    };
    assert!(raw("'medium'", "reported").is_ok());
    assert!(raw("NULL", "reported").is_ok());
    assert!(raw("'turbo'", "table").is_err());
    assert!(raw("NULL", "guess").is_err());
}

// spec: AI-014 [M4]
// acceptance: independent
// Summenzeile je Aufgabe und Modell weist `Reasoning Tokens` aus (Teilmenge von `Output Tokens`, nicht zusätzlich
// berechnet) und summiert `Computed Cost`.
#[test]
fn totals_include_reasoning_tokens_and_computed_cost() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    insert_with_details(
        f.conn(),
        &record(2_000, PriceSource::Reported),
        &details(Some("low"), Some(120), Some(1_000), true),
    )
    .unwrap();
    insert_with_details(
        f.conn(),
        &record(1_000, PriceSource::Api),
        &details(Some("low"), Some(80), None, false),
    )
    .unwrap();
    insert_with_details(
        f.conn(),
        &record(1_000, PriceSource::Api),
        &details(Some("low"), None, None, false),
    )
    .unwrap();
    let totals = totals_by_task_and_model(f.conn()).unwrap();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].output_tokens, 600);
    assert_eq!(totals[0].reasoning_tokens, 200);
    assert_eq!(totals[0].cost, MicroUsd(4_000));
    assert_eq!(totals[0].computed_cost, MicroUsd(3_000));
}

// spec: AI-012 [M4]
// acceptance: independent
// Grundlage des Ausgabe-Korrekturfaktors je (Modell, Aufgabe, effektive Stufe): nur erfolgreiche Zeilen dieser
// Kombination; Zeilen einer anderen Stufe, Aufgabe oder mit `Error: Output limit reached` zählen nicht.
#[test]
fn output_calibration_samples_are_per_level_and_task() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let add = |task: &str, level: Option<&str>, out: u64, status: &str| {
        let mut r = record(10, PriceSource::Table);
        r.task = task.into();
        r.output_tokens = Some(out);
        r.status = status.into();
        insert_with_details(f.conn(), &r, &details(level, None, None, false)).unwrap();
    };
    add("suggest-title", Some("medium"), 500, "Success");
    add("suggest-title", Some("medium"), 510, "Success");
    add("suggest-title", Some("low"), 100, "Success");
    add("suggest-meta", Some("medium"), 900, "Success");
    add(
        "suggest-title",
        Some("medium"),
        1_024,
        "Error: Output limit reached",
    );
    add("decision", None, 0, "Success");
    let samples = |task: &str, level: Option<&str>| {
        output_calibration_samples(
            f.conn(),
            "openrouter",
            "openai/gpt-6-luna",
            task,
            level,
            200,
        )
        .unwrap()
    };
    assert_eq!(samples("suggest-title", Some("medium")), vec![500, 510]);
    assert_eq!(samples("suggest-title", Some("low")), vec![100]);
    assert_eq!(samples("suggest-meta", Some("medium")), vec![900]);
    assert_eq!(samples("decision", None), vec![0]);
}

// spec: AI-014 [M4]
// acceptance: independent
// Bestehende Datei aus Schemaversion 5 (Archiv-Fixture): nach dem Öffnen mit Migration 0006 nimmt `ai_requests` die
// neuen Spalten auf.
#[test]
fn v5_archive_takes_the_new_columns_after_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.ospider");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/migrations/schema-v5/crawl.ospider"),
        &path,
    )
    .unwrap();
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    add_run(&f);
    insert_with_details(
        f.conn(),
        &record(2_400, PriceSource::Reported),
        &details(Some("xhigh"), Some(7), Some(2_399), false),
    )
    .unwrap();
    let rows = list(f.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].details.reasoning_level.as_deref(), Some("xhigh"));
    assert_eq!(
        rows[0].details.computed_cost_of(&rows[0].record),
        MicroUsd(2_399)
    );
}

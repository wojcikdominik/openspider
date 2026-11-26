//! M4-K10: Spalten des Nachtrags 24.11.2026, Q00 in `ai_requests` (AI-014): `Reasoning Level`, `Reasoning Tokens`,
//! `Computed Cost (USD)`, Kennzeichen `cost_mismatch` und `Price Source = reported`; Ausgabe-Kalibrierung je (Modell,
//! Aufgabe, Stufe) (AI-012).
use std::path::{Path, PathBuf};

use os_model::budget::MicroUsd;
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::ai::requests::{
    flags_cell, insert, insert_with_details, list, output_calibration_samples,
    totals_by_task_and_model, PriceSource, RequestDetails, RequestFilter, RequestRecord, COLUMNS,
    FLAGS, FLAGS_COLUMN, REASONING_LEVELS,
};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile, StoreError};

fn fresh(dir: &Path) -> PathBuf {
    let created = Timestamp::from_db("2026-11-26T08:00:00.000Z").unwrap();
    let path = dir.join("k10.ospider");
    let init = CrawlInit {
        crawl_id: SeededIds::new(10).new_crawl_id(created),
        mode: CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "K10".into(),
        name: "requests".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    let f = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    f.conn()
        .execute_batch(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-26T08:00:00Z', '{}');",
        )
        .unwrap();
    path
}

fn row(task: &str, output: u64, cost: u64, source: PriceSource, status: &str) -> RequestRecord {
    RequestRecord {
        run_id: 1,
        timestamp: "2026-11-26T08:01:00Z".into(),
        task: task.into(),
        provider: "openrouter".into(),
        model: "mock/text".into(),
        endpoint: "chat-completions".into(),
        url_id: None,
        definition_version: None,
        reservation: MicroUsd(cost),
        redactions: 0,
        input_tokens: Some(1_000),
        cached_input_tokens: None,
        cache_write_tokens: None,
        output_tokens: Some(output),
        images: 0,
        token_cost: MicroUsd(cost),
        fixed_fees: MicroUsd(0),
        regional_uplift: MicroUsd(0),
        cost: MicroUsd(cost),
        price_source: source,
        batch_id: None,
        cache_hit: false,
        status: status.into(),
        http_status: Some(200),
        error_code: None,
        error_message: None,
        refusal_reason: None,
        duration_ms: 3,
        provider_request_id: Some("gen-mock-1".into()),
        retries: 0,
        over_reservation: false,
        confidence_mismatch: false,
        estimated_input_tokens: Some(900),
    }
}

fn details(level: Option<&str>, reasoning: Option<u64>, computed: Option<u64>) -> RequestDetails {
    RequestDetails {
        reasoning_level: level.map(str::to_string),
        reasoning_tokens: reasoning,
        computed_cost: computed.map(MicroUsd),
        cost_mismatch: false,
        estimated_output_tokens: None,
    }
}

// spec: AI-014 [M4]
// Die Spaltenliste der Ansicht und des Exports „AI Request Log“ enthält die drei neuen Spalten mit den kanonischen
// Namen an der festgelegten Stelle: `Reasoning Level` nach `Endpoint`, `Reasoning Tokens` nach `Output Tokens`,
// `Computed Cost (USD)` nach `Cost (USD)`.
#[test]
fn log_columns_carry_the_new_names_in_place() {
    let after = |a: &str, b: &str| {
        let i = COLUMNS.iter().position(|c| *c == a).unwrap();
        assert_eq!(COLUMNS[i + 1], b, "{b} nach {a}");
    };
    after("Endpoint", "Reasoning Level");
    after("Output Tokens", "Reasoning Tokens");
    after("Cost (USD)", "Computed Cost (USD)");
    // Nachtrag 26.11.2026: `Flags` ist die letzte Spalte nach `Retries`; die interne Schätzspalte gehört nicht dazu.
    assert_eq!(COLUMNS[COLUMNS.len() - 1], FLAGS_COLUMN);
    assert_eq!(COLUMNS[COLUMNS.len() - 2], "Retries");
    assert!(!COLUMNS.contains(&"estimated_output_tokens"));
    let mut sorted = COLUMNS.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), COLUMNS.len());
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
// `Price Source = reported` mit abweichendem `Computed Cost` und `cost_mismatch` wird gespeichert und gelesen; die
// neuen Spalten überstehen den Roundtrip; ohne Angaben gilt `Computed Cost` = `Cost`, Stufe und Reasoning leer.
#[test]
fn reported_rows_and_new_columns_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let f = CrawlFile::open(&fresh(dir.path()), OpenMode::ReadWrite).unwrap();
    let c = f.conn();
    let reported = row(
        "suggest-title",
        200,
        2_000,
        PriceSource::Reported,
        "Success",
    );
    let d = RequestDetails {
        cost_mismatch: true,
        ..details(Some("low"), Some(120), Some(1_000))
    };
    insert_with_details(c, &reported, &d).unwrap();
    insert(
        c,
        &row("suggest-title", 100, 700, PriceSource::Api, "Success"),
    )
    .unwrap();
    let rows = list(c, &RequestFilter::default()).unwrap();
    assert_eq!(rows[0].record, reported);
    assert_eq!(rows[0].record.price_source.as_str(), "reported");
    assert_eq!(rows[0].details, d);
    assert_eq!(
        rows[1].details,
        details(None, None, Some(700)),
        "Standard: Computed Cost = Cost"
    );
    let totals = totals_by_task_and_model(c).unwrap();
    assert_eq!(totals[0].reasoning_tokens, 120);
    assert_eq!(totals[0].cost, MicroUsd(2_700));
    assert_eq!(totals[0].computed_cost, MicroUsd(1_700));
}

// spec: AI-014 [M4]
// Ein abweichendes `Computed Cost` oder `cost_mismatch` ist nur bei `Price Source = reported` zulässig; eine
// unbekannte Stufe wird abgewiesen – in Rust und per CHECK der Tabelle.
#[test]
fn invalid_details_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let f = CrawlFile::open(&fresh(dir.path()), OpenMode::ReadWrite).unwrap();
    let c = f.conn();
    let api = row("t", 1, 500, PriceSource::Api, "Success");
    assert!(matches!(
        insert_with_details(c, &api, &details(None, None, Some(400))),
        Err(StoreError::InvalidInput(_))
    ));
    let mismatch = RequestDetails {
        cost_mismatch: true,
        ..RequestDetails::default()
    };
    assert!(insert_with_details(c, &api, &mismatch).is_err());
    assert!(insert_with_details(c, &api, &details(Some("none"), None, None)).is_err());
    assert!(insert_with_details(c, &api, &details(Some("Low"), None, None)).is_err());
    for level in REASONING_LEVELS {
        insert_with_details(c, &api, &details(Some(level), Some(0), None)).unwrap();
    }
    let raw = |level: &str, source: &str, reasoning: i64| {
        c.execute(
            "INSERT INTO ai_requests (run_id, timestamp, task, provider, model, endpoint, token_cost_microusd,
               cost_microusd, price_source, status, duration_ms, reasoning_level, reasoning_tokens)
             VALUES (1, '2026-11-26T08:01:00Z', 't', 'p', 'm', 'e', 1, 1, ?2, 'Success', 1, ?1, ?3)",
            rusqlite::params![level, source, reasoning],
        )
    };
    raw("medium", "reported", 3).unwrap();
    assert!(raw("x-high", "api", 0).is_err());
    assert!(raw("low", "guess", 0).is_err());
    assert!(raw("low", "api", -1).is_err());
}

// spec: AI-014 [M4]
// Nachtrag 26.11.2026: Zeile mit `over_reservation` und `cost_mismatch` → `Flags = over_reservation, cost_mismatch`;
// Zeile ohne Kennzeichen → `Flags` leer; Reihenfolge `over_reservation`, `confidence_mismatch`, `cost_mismatch`.
#[test]
fn flags_cell_lists_set_flags_in_order() {
    assert_eq!(
        FLAGS,
        ["over_reservation", "confidence_mismatch", "cost_mismatch"]
    );
    let mut r = row("t", 1, 500, PriceSource::Reported, "Success");
    let mut d = RequestDetails::default();
    assert_eq!(flags_cell(&r, &d), "");
    r.over_reservation = true;
    d.cost_mismatch = true;
    assert_eq!(flags_cell(&r, &d), "over_reservation, cost_mismatch");
    r.confidence_mismatch = true;
    assert_eq!(
        flags_cell(&r, &d),
        "over_reservation, confidence_mismatch, cost_mismatch"
    );
    // Roundtrip über die Crawl-Datei: die Kennzeichen kommen gelesen genauso zurück.
    let dir = tempfile::tempdir().unwrap();
    let f = CrawlFile::open(&fresh(dir.path()), OpenMode::ReadWrite).unwrap();
    r.confidence_mismatch = false;
    let stored = RequestDetails {
        computed_cost: Some(MicroUsd(250)),
        ..d.clone()
    };
    insert_with_details(f.conn(), &r, &stored).unwrap();
    let back = &list(f.conn(), &RequestFilter::default()).unwrap()[0];
    assert_eq!(
        flags_cell(&back.record, &back.details),
        "over_reservation, cost_mismatch"
    );
}

/// Aufgabe, Ausgabe-Tokens, Stufe, Status, gespeicherte Ausgabeschätzung.
type Case<'a> = (&'a str, u64, Option<&'a str>, &'a str, Option<u64>);

// spec: AI-012 [M4]
// Ausgabe-Kalibrierung: Paare (Schätzung, Ist) nur aus erfolgreichen Zeilen derselben (Anbieter, Modell, Aufgabe,
// Stufe) mit gespeicherter Schätzung (`estimated_output_tokens`, Nachtrag 26.11.2026), älteste zuerst, höchstens
// `limit`; Zeilen ohne Stufe sind getrennt abfragbar.
#[test]
fn output_samples_are_per_model_task_and_level() {
    let dir = tempfile::tempdir().unwrap();
    let f = CrawlFile::open(&fresh(dir.path()), OpenMode::ReadWrite).unwrap();
    let c = f.conn();
    let cases: [Case; 8] = [
        ("suggest-title", 10, Some("medium"), "Success", Some(100)),
        ("suggest-title", 20, Some("medium"), "Success", Some(200)),
        ("suggest-title", 30, Some("low"), "Success", Some(300)),
        ("suggest-meta", 40, Some("medium"), "Success", Some(400)),
        (
            "suggest-title",
            50,
            Some("medium"),
            "Error: Output limit reached",
            Some(500),
        ),
        ("suggest-title", 60, None, "Success", Some(600)),
        ("suggest-title", 65, Some("medium"), "Success", None),
        ("suggest-title", 70, Some("medium"), "Success", Some(700)),
    ];
    for (task, out, level, status, estimate) in cases {
        insert_with_details(
            c,
            &row(task, out, 1, PriceSource::Api, status),
            &RequestDetails {
                estimated_output_tokens: estimate,
                ..details(level, None, None)
            },
        )
        .unwrap();
    }
    let samples = |task: &str, level: Option<&str>, limit: usize| {
        output_calibration_samples(c, "openrouter", "mock/text", task, level, limit).unwrap()
    };
    assert_eq!(
        samples("suggest-title", Some("medium"), 200),
        [(100, 10), (200, 20), (700, 70)]
    );
    assert_eq!(
        samples("suggest-title", Some("medium"), 2),
        [(200, 20), (700, 70)]
    );
    assert_eq!(samples("suggest-title", Some("low"), 200), [(300, 30)]);
    assert_eq!(samples("suggest-title", None, 200), [(600, 60)]);
    assert_eq!(samples("suggest-meta", Some("medium"), 200), [(400, 40)]);
    let back = list(c, &RequestFilter::default()).unwrap();
    assert_eq!(back[0].details.estimated_output_tokens, Some(100));
    assert_eq!(back[6].details.estimated_output_tokens, None);
}

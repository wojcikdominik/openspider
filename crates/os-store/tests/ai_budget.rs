//! M4-K05: Grenzwerte (`api_budget`) und Kostenprotokoll (`ai_requests`) in der Crawl-Datei, zusammen mit dem
//! gemeinsamen `BudgetLedger` aus `os-model` (AI-013, AI-014).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use os_model::budget::{
    BudgetError, BudgetLedger, BudgetLevel, BudgetLimits, BudgetScope, CrawlBudgetLedger, MicroUsd,
    AREA_AI,
};
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::ai::budget::{load_limits, save_limits};
use os_store::ai::requests::{
    calibration_samples, insert, list, spent_by_task, total_cost, totals_by_task_and_model,
    AiRequestsSpend, PriceSource, RequestFilter, RequestRecord,
};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile, StoreError};

fn fresh(dir: &Path) -> PathBuf {
    let created = Timestamp::from_db("2026-11-24T08:00:00.000Z").unwrap();
    let path = dir.join("k05.ospider");
    let init = CrawlInit {
        crawl_id: SeededIds::new(5).new_crawl_id(created),
        mode: CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "K05".into(),
        name: "budget".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    let f = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    f.conn()
        .execute_batch(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-24T08:00:00Z', '{}');
             INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length)
               VALUES(1,'https://ai.test:8080/','https://ai.test:8080/','https://ai.test:8080',1,'Done','Seed',0,21);",
        )
        .unwrap();
    path
}

fn row(task: &str, model: &str, input: u64, output: u64, cost: u64) -> RequestRecord {
    RequestRecord {
        run_id: 1,
        timestamp: "2026-11-24T08:01:00Z".into(),
        task: task.into(),
        provider: "anthropic".into(),
        model: model.into(),
        endpoint: "messages".into(),
        url_id: Some(1),
        definition_version: Some("1.0.0".into()),
        reservation: MicroUsd(cost),
        redactions: 0,
        input_tokens: Some(input),
        cached_input_tokens: None,
        cache_write_tokens: None,
        output_tokens: Some(output),
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
        duration_ms: 12,
        provider_request_id: Some("req-1".into()),
        retries: 0,
        over_reservation: false,
        confidence_mismatch: false,
        estimated_input_tokens: Some(input),
    }
}

// spec: AI-013
#[test]
fn limits_roundtrip_through_api_budget_and_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = fresh(dir.path());
    let mut limits = BudgetLimits {
        crawl: MicroUsd(1_000_000),
        warning_percent: 75,
        ..BudgetLimits::default()
    };
    limits.areas.insert(AREA_AI.into(), MicroUsd(8_000));
    limits.tasks.insert(
        AREA_AI.into(),
        BTreeMap::from([("suggest-title".to_string(), MicroUsd(500_000))]),
    );
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        assert_eq!(
            load_limits(f.conn()).unwrap(),
            None,
            "neue Datei: keine Grenzwerte"
        );
        save_limits(f.conn(), &limits).unwrap();
    }
    let f = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    assert_eq!(load_limits(f.conn()).unwrap(), Some(limits.clone()));
    let rows: i64 = f
        .conn()
        .query_row("SELECT count(*) FROM api_budget", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 3, "Crawl, KI gesamt, eine Aufgabe");
    drop(f);

    // „Unbegrenzt“ und Budget 0 bleiben unterscheidbar; Speichern ersetzt den bisherigen Inhalt.
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let unlimited = BudgetLimits {
        unlimited: true,
        ..BudgetLimits::default()
    };
    save_limits(f.conn(), &unlimited).unwrap();
    assert_eq!(load_limits(f.conn()).unwrap(), Some(unlimited));
    let zero = BudgetLimits {
        crawl: MicroUsd(0),
        ..BudgetLimits::default()
    };
    save_limits(f.conn(), &zero).unwrap();
    assert_eq!(load_limits(f.conn()).unwrap(), Some(zero));
    // Ungültige Eingaben ändern nichts.
    let mut bad = BudgetLimits::default();
    bad.areas.insert(String::new(), MicroUsd(1));
    assert!(matches!(
        save_limits(f.conn(), &bad),
        Err(StoreError::InvalidInput(_))
    ));
    assert_eq!(load_limits(f.conn()).unwrap().unwrap().crawl, MicroUsd(0));
}

// spec: AI-014
#[test]
fn request_log_keeps_metadata_and_sums_per_task_and_model() {
    let dir = tempfile::tempdir().unwrap();
    let path = fresh(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    // 3 × (1.000 Eingabe, 200 Ausgabe) auf mock-claude (1/5 USD je 1 Mio.) = je 2.000 µUSD.
    for _ in 0..3 {
        insert(f.conn(), &row("decision", "mock-claude", 1_000, 200, 2_000)).unwrap();
    }
    let mut other = row("suggest-title", "gpt-6-luna", 1_000, 200, 200);
    other.url_id = None;
    other.input_tokens = None;
    other.output_tokens = None;
    other.price_source = PriceSource::Estimate;
    other.refusal_reason = Some("ü".repeat(600));
    other.status = "Refusal".into();
    insert(f.conn(), &other).unwrap();

    let totals = totals_by_task_and_model(f.conn()).unwrap();
    assert_eq!(totals.len(), 2);
    let claude = &totals[0];
    assert_eq!(
        (claude.task.as_str(), claude.model.as_str(), claude.requests),
        ("decision", "mock-claude", 3)
    );
    assert_eq!((claude.input_tokens, claude.output_tokens), (3_000, 600));
    assert_eq!(claude.cost, MicroUsd(6_000), "Cost 0.006000");
    assert_eq!(total_cost(f.conn()).unwrap(), MicroUsd(6_200));
    assert_eq!(
        spent_by_task(f.conn()).unwrap(),
        BTreeMap::from([
            ("decision".to_string(), MicroUsd(6_000)),
            ("suggest-title".to_string(), MicroUsd(200))
        ])
    );

    let all = list(f.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(all.len(), 4);
    assert_eq!(all[0].address.as_deref(), Some("https://ai.test:8080/"));
    let last = &all[3];
    assert_eq!(last.address, None, "URL-unabhängig → Address leer");
    assert_eq!(last.record.input_tokens, None, "null, nicht 0");
    assert_eq!(last.record.price_source, PriceSource::Estimate);
    assert_eq!(
        last.record
            .refusal_reason
            .as_deref()
            .map(|s| s.chars().count()),
        Some(500)
    );
    let refusals = list(
        f.conn(),
        &RequestFilter {
            status: Some("Refusal".into()),
            ..RequestFilter::default()
        },
    )
    .unwrap();
    assert_eq!(refusals.len(), 1);
    let by_model = list(
        f.conn(),
        &RequestFilter {
            task: Some("decision".into()),
            model: Some("mock-claude".into()),
            from: Some("2026-11-24T08:00:00Z".into()),
            to: Some("2026-11-24T08:02:00Z".into()),
            ..RequestFilter::default()
        },
    )
    .unwrap();
    assert_eq!(by_model.len(), 3);
    assert_eq!(
        by_model[0].record,
        row("decision", "mock-claude", 1_000, 200, 2_000)
    );
}

// spec: AI-014
#[test]
fn rows_whose_cost_is_not_the_sum_of_its_parts_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = fresh(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let mut r = row("decision", "mock/vision", 10, 10, 0);
    r.token_cost = MicroUsd(60);
    r.fixed_fees = MicroUsd(3_000);
    r.regional_uplift = MicroUsd(306);
    r.cost = MicroUsd(3_365);
    assert!(matches!(
        insert(f.conn(), &r),
        Err(StoreError::InvalidInput(_))
    ));
    r.cost = MicroUsd(3_366);
    insert(f.conn(), &r).unwrap();
    assert_eq!(total_cost(f.conn()).unwrap(), MicroUsd(3_366));
}

// spec: AI-013
#[test]
fn spend_from_the_log_still_counts_after_reopening_the_crawl_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = fresh(dir.path());
    let limits = BudgetLimits {
        crawl: MicroUsd(10_000),
        ..BudgetLimits::default()
    };
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        save_limits(f.conn(), &limits).unwrap();
        let ledger =
            CrawlBudgetLedger::open(limits.clone(), &[&AiRequestsSpend(f.conn())]).unwrap();
        for _ in 0..2 {
            let r = ledger
                .reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(4_000))
                .unwrap();
            insert(f.conn(), &row("decision", "mock-claude", 1_000, 200, 3_000)).unwrap();
            ledger.settle(r.id, MicroUsd(3_000)).unwrap();
        }
    }
    // Neustart, „Fortsetzen“: Grenzwerte und Verbrauch aus der Datei.
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let limits = load_limits(f.conn()).unwrap().unwrap();
    let ledger = CrawlBudgetLedger::open(limits, &[&AiRequestsSpend(f.conn())]).unwrap();
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.spent, MicroUsd(6_000));
    assert_eq!(snap.remaining, Some(MicroUsd(4_000)));
    let r = ledger
        .reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(4_000))
        .unwrap();
    ledger.release(r.id).unwrap();
    assert_eq!(
        ledger.reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(4_001)),
        Err(BudgetError::Exceeded(BudgetLevel::Crawl))
    );
}

// spec: AI-013
// spec: AI-014
#[test]
fn four_workers_writing_200_log_rows_stay_within_the_budget_and_match_the_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = fresh(dir.path());
    let limits = BudgetLimits {
        crawl: MicroUsd(300_000),
        ..BudgetLimits::default()
    };
    let file = Arc::new(Mutex::new(
        CrawlFile::open(&path, OpenMode::ReadWrite).unwrap(),
    ));
    let ledger = {
        let f = file.lock().unwrap();
        save_limits(f.conn(), &limits).unwrap();
        Arc::new(CrawlBudgetLedger::open(limits, &[&AiRequestsSpend(f.conn())]).unwrap())
    };
    let next = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let handles: Vec<_> = (0..4u64)
        .map(|w| {
            let (file, ledger, next) = (file.clone(), ledger.clone(), next.clone());
            std::thread::spawn(move || {
                let mut exceeded = 0u64;
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if i >= 200 {
                        return exceeded;
                    }
                    let scope = BudgetScope::task(AREA_AI, "decision");
                    match ledger.reserve(scope, MicroUsd(4_000)) {
                        Ok(r) => {
                            let actual = 3_000 + (i * 13 + w) % 1_001;
                            std::thread::yield_now();
                            {
                                let f = file.lock().unwrap();
                                insert(
                                    f.conn(),
                                    &row("decision", "mock-claude", 1_000, 200, actual),
                                )
                                .unwrap();
                            }
                            assert!(
                                !ledger
                                    .settle(r.id, MicroUsd(actual))
                                    .unwrap()
                                    .over_reservation
                            );
                        }
                        Err(BudgetError::Exceeded(_)) => exceeded += 1,
                        Err(e) => panic!("{e}"),
                    }
                }
            })
        })
        .collect();
    let exceeded: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
    let f = file.lock().unwrap();
    let logged = total_cost(f.conn()).unwrap();
    let snap = ledger.snapshot().unwrap();
    assert!(logged.0 <= 300_000, "Summe Ist {} > Budget", logged.0);
    assert_eq!(
        snap.spent, logged,
        "Protokoll und Budgetanzeige auf 1 µUSD gleich"
    );
    assert_eq!(snap.reserved, MicroUsd(0));
    let rows: i64 = f
        .conn()
        .query_row("SELECT count(*) FROM ai_requests", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows as u64 + exceeded, 200);
    assert!(exceeded > 0, "Budget begrenzt den Lauf");
}

// spec: AI-012
#[test]
fn calibration_samples_are_the_last_200_estimated_requests_of_a_model() {
    let dir = tempfile::tempdir().unwrap();
    let path = fresh(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    // 250 Requests mock-claude: Schätzung 100 + i, Ist 80 + i.
    for i in 0..250u64 {
        let mut r = row("decision", "mock-claude", 80 + i, 10, 100);
        r.estimated_input_tokens = Some(100 + i);
        insert(f.conn(), &r).unwrap();
    }
    // Nicht verwertbar: Cache-Treffer, ohne Schätzung, ohne gemeldete Tokens, anderes Modell.
    let mut hit = row("decision", "mock-claude", 1, 1, 0);
    hit.cache_hit = true;
    insert(f.conn(), &hit).unwrap();
    let mut no_est = row("decision", "mock-claude", 1, 1, 0);
    no_est.estimated_input_tokens = None;
    insert(f.conn(), &no_est).unwrap();
    let mut no_usage = row("decision", "mock-claude", 1, 1, 0);
    no_usage.input_tokens = None;
    no_usage.price_source = PriceSource::Estimate;
    insert(f.conn(), &no_usage).unwrap();
    insert(f.conn(), &row("decision", "other-model", 5, 1, 0)).unwrap();

    let samples = calibration_samples(f.conn(), "anthropic", "mock-claude", 200).unwrap();
    assert_eq!(samples.len(), 200);
    assert_eq!(samples[0], (150, 130), "älteste der letzten 200 zuerst");
    assert_eq!(samples[199], (349, 329));
    assert_eq!(
        calibration_samples(f.conn(), "anthropic", "other-model", 200).unwrap(),
        vec![(5, 5)]
    );
    assert!(calibration_samples(f.conn(), "openai", "mock-claude", 200)
        .unwrap()
        .is_empty());
    let all = list(f.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(all[0].record.estimated_input_tokens, Some(100));
}

// spec: AI-013
#[test]
fn over_reservation_halt_survives_reopen_until_a_limit_is_raised() {
    use os_store::ai::budget::{load_over_reservation_acks, open_ledger, raise_limits};
    let dir = tempfile::tempdir().unwrap();
    let path = fresh(dir.path());
    let scope = || BudgetScope::task(AREA_AI, "decision");
    let halted = |l: &CrawlBudgetLedger| l.snapshot().unwrap().halted;
    let base = BudgetLimits {
        crawl: MicroUsd(100_000),
        ..BudgetLimits::default()
    };
    // Lauf 1: Ist 6.000 über der Reservierung 4.000 → Zeile mit over_reservation, Ledger gesperrt.
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        save_limits(f.conn(), &base).unwrap();
        let ledger = open_ledger(f.conn(), BudgetLimits::default(), &[]).unwrap();
        assert!(!halted(&ledger));
        let r = ledger.reserve(scope(), MicroUsd(4_000)).unwrap();
        let mut rec = row("decision", "mock-claude", 1_000, 1_000, 6_000);
        rec.reservation = MicroUsd(4_000);
        rec.over_reservation = true;
        insert(f.conn(), &rec).unwrap();
        assert!(
            ledger
                .settle(r.id, MicroUsd(6_000))
                .unwrap()
                .over_reservation
        );
        assert!(halted(&ledger));
    }
    // Neuöffnen ohne Erhöhung: weiterhin gesperrt; Beträge 0 laufen.
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        let ledger = open_ledger(f.conn(), BudgetLimits::default(), &[]).unwrap();
        assert!(halted(&ledger), "Sperre nach Neuöffnen aktiv");
        assert_eq!(
            ledger.reserve(scope(), MicroUsd(1)),
            Err(BudgetError::Exceeded(BudgetLevel::Halted))
        );
        let free = ledger.reserve(scope(), MicroUsd(0)).unwrap();
        ledger.settle(free.id, MicroUsd(0)).unwrap();
        // Speichern derselben oder niedrigerer Grenzen hebt nichts auf.
        assert!(!raise_limits(f.conn(), &ledger, base.clone()).unwrap());
        assert!(halted(&ledger));
        // Erhöhung: frei.
        let raised = BudgetLimits {
            crawl: MicroUsd(200_000),
            ..base.clone()
        };
        assert!(raise_limits(f.conn(), &ledger, raised.clone()).unwrap());
        assert!(!halted(&ledger));
        let r = ledger.reserve(scope(), MicroUsd(4_000)).unwrap();
        ledger.release(r.id).unwrap();
        assert_eq!(
            load_over_reservation_acks(f.conn()).unwrap(),
            BTreeMap::from([(AREA_AI.to_string(), 1)])
        );
        assert_eq!(load_limits(f.conn()).unwrap(), Some(raised));
    }
    // Nach Erhöhung und Neuöffnen weiterhin frei; Speichern der Grenzwerte behält die Quittung.
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        let ledger = open_ledger(f.conn(), BudgetLimits::default(), &[]).unwrap();
        assert!(!halted(&ledger), "alte Überschreitung sperrt nicht wieder");
        assert_eq!(ledger.snapshot().unwrap().spent, MicroUsd(6_000));
        let r = ledger.reserve(scope(), MicroUsd(4_000)).unwrap();
        ledger.release(r.id).unwrap();
        save_limits(f.conn(), &load_limits(f.conn()).unwrap().unwrap()).unwrap();
        assert_eq!(load_over_reservation_acks(f.conn()).unwrap().len(), 1);
        // Eine neue Überschreitung sperrt nach Neuöffnen wieder.
        let mut rec = row("decision", "mock-claude", 1_000, 1_000, 6_000);
        rec.reservation = MicroUsd(4_000);
        rec.over_reservation = true;
        insert(f.conn(), &rec).unwrap();
    }
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    assert!(halted(
        &open_ledger(f.conn(), BudgetLimits::default(), &[]).unwrap()
    ));
}

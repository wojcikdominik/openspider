//! Unabhängige Abnahmetests M4-K05: Budget-Grenzwerte in `api_budget` und KI-Verbrauch aus `ai_requests`
//! (AI-013, AI-014) über Schließen und Neuöffnen der Crawl-Datei.
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-013 „Die Budgetwerte und der Verbrauch sind in der
//! Crawl-Datei gespeichert; nach Neustart und ‚Fortsetzen‘ gilt der bisherige Verbrauch weiter“; „Datenhaltung“:
//! `api_budget` enthält nur Grenzwerte, Verbrauch aus `ai_requests`; AI-014 Summen und Budgetanzeige auf 1 µUSD).
// acceptance: independent

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use os_model::budget::{
    BudgetError, BudgetLedger, BudgetLimits, BudgetScope, CrawlBudgetLedger, MicroUsd, AREA_AI,
};
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::ai::budget::{load_limits, save_limits};
use os_store::ai::requests::{
    insert, list, total_cost, AiRequestsSpend, PriceSource, RequestFilter, RequestRecord,
    REFUSAL_REASON_MAX_CHARS,
};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};

fn create(dir: &Path) -> PathBuf {
    let created = Timestamp::from_db("2026-11-24T09:00:00.000Z").unwrap();
    let path = dir.join("k05-acceptance.ospider");
    let init = CrawlInit {
        crawl_id: SeededIds::new(55).new_crawl_id(created),
        mode: CrawlMode::Spider,
        start_url: Some("https://ai.test:8080/".into()),
        list_source: None,
        project: "Abnahme".into(),
        name: "K05".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    let f = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    f.conn()
        .execute_batch(
            "INSERT INTO ai_runs(id, started_at, config_json) VALUES(1, '2026-11-24T09:00:00Z', '{}');",
        )
        .unwrap();
    drop(f);
    path
}

fn record(task: &str, token: u64, fees: u64, uplift: u64) -> RequestRecord {
    RequestRecord {
        run_id: 1,
        timestamp: "2026-11-24T09:01:00Z".into(),
        task: task.into(),
        provider: "anthropic".into(),
        model: "mock-claude".into(),
        endpoint: "messages".into(),
        url_id: None,
        definition_version: Some("1".into()),
        reservation: MicroUsd(token + fees + uplift),
        redactions: 0,
        input_tokens: Some(1_000),
        cached_input_tokens: None,
        cache_write_tokens: None,
        output_tokens: Some(200),
        images: 0,
        token_cost: MicroUsd(token),
        fixed_fees: MicroUsd(fees),
        regional_uplift: MicroUsd(uplift),
        cost: MicroUsd(token + fees + uplift),
        price_source: PriceSource::Table,
        batch_id: None,
        cache_hit: false,
        status: "Success".into(),
        http_status: Some(200),
        error_code: None,
        error_message: None,
        refusal_reason: None,
        duration_ms: 12,
        provider_request_id: None,
        retries: 0,
        over_reservation: false,
        confidence_mismatch: false,
        estimated_input_tokens: Some(900),
    }
}

fn limits() -> BudgetLimits {
    BudgetLimits {
        crawl: MicroUsd(10_000),
        unlimited: false,
        areas: BTreeMap::from([(AREA_AI.to_string(), MicroUsd(9_000))]),
        tasks: BTreeMap::from([(
            AREA_AI.to_string(),
            BTreeMap::from([("suggest-title".to_string(), MicroUsd(5_000))]),
        )]),
        warning_percent: 70,
    }
}

// spec: AI-013
// acceptance: independent
// Grenzwerte (Crawl-Budget, „KI gesamt“, Aufgaben-Teilbudget, Warnschwelle) überstehen Schließen und Neuöffnen.
#[test]
fn limits_survive_close_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        assert_eq!(
            load_limits(f.conn()).unwrap(),
            None,
            "neue Datei hat noch keine Grenzwerte"
        );
        save_limits(f.conn(), &limits()).unwrap();
    }
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    assert_eq!(load_limits(f.conn()).unwrap(), Some(limits()));
}

// spec: AI-013
// acceptance: independent
// „Unbegrenzt“ wird gespeichert und wieder geladen; Budget 0 bleibt 0 (nicht „unbegrenzt“).
#[test]
fn unlimited_and_zero_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    let unlimited = BudgetLimits {
        unlimited: true,
        ..BudgetLimits::default()
    };
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        save_limits(f.conn(), &unlimited).unwrap();
    }
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        let l = load_limits(f.conn()).unwrap().unwrap();
        assert!(l.unlimited);
        save_limits(
            f.conn(),
            &BudgetLimits {
                crawl: MicroUsd(0),
                ..BudgetLimits::default()
            },
        )
        .unwrap();
    }
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let l = load_limits(f.conn()).unwrap().unwrap();
    assert!(!l.unlimited);
    assert_eq!(l.crawl, MicroUsd(0));
}

// spec: AI-013
// acceptance: independent
// Verbrauch (Summe `Cost` aus `ai_requests`) gilt nach Neustart weiter: Ledger aus gespeicherten Grenzwerten und
// Protokoll; Reservierungen über dem Rest werden abgelehnt, auch beim Aufgaben-Teilbudget.
#[test]
fn spend_survives_reopen_and_limits_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    {
        let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        save_limits(f.conn(), &limits()).unwrap();
        insert(f.conn(), &record("suggest-title", 3_000, 0, 0)).unwrap();
        insert(f.conn(), &record("decision", 2_000, 500, 250)).unwrap();
    }
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let limits = load_limits(f.conn()).unwrap().unwrap();
    let spend = AiRequestsSpend(f.conn());
    let ledger = CrawlBudgetLedger::open(limits, &[&spend]).unwrap();
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.spent, MicroUsd(5_750));
    assert_eq!(snap.area_spent(AREA_AI), MicroUsd(5_750));
    assert_eq!(snap.remaining, Some(MicroUsd(4_250)));

    // Aufgaben-Teilbudget suggest-title 5.000, davon 3.000 verbraucht.
    let t = ledger.reserve(BudgetScope::task(AREA_AI, "suggest-title"), MicroUsd(2_001));
    assert!(matches!(t, Err(BudgetError::Exceeded(_))), "{t:?}");
    // „KI gesamt“ 9.000, davon 5.750 verbraucht → 3.250 frei; Crawl 4.250 frei.
    let a = ledger.reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(3_251));
    assert!(matches!(a, Err(BudgetError::Exceeded(_))), "{a:?}");
    let ok = ledger
        .reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(3_250))
        .unwrap();
    ledger.release(ok.id).unwrap();
}

// spec: AI-014
// acceptance: independent
// Summen im Protokoll und der KI-Anteil der Budgetanzeige stimmen auf 1 µUSD überein.
#[test]
fn log_sum_equals_ledger_ai_share() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let mut expected = 0u64;
    for i in 0..37u64 {
        let (t, fee, up) = (i * 137 + 1, (i % 3) * 1_000, i % 7);
        expected += t + fee + up;
        let task = ["decision", "suggest-title", "suggest-alt"][(i % 3) as usize];
        insert(f.conn(), &record(task, t, fee, up)).unwrap();
    }
    let rows = list(f.conn(), &RequestFilter::default()).unwrap();
    let row_sum: u64 = rows.iter().map(|r| r.record.cost.0).sum();
    assert_eq!(row_sum, expected);
    assert_eq!(total_cost(f.conn()).unwrap(), MicroUsd(expected));
    let ledger =
        CrawlBudgetLedger::open(BudgetLimits::default(), &[&AiRequestsSpend(f.conn())]).unwrap();
    assert_eq!(
        ledger.snapshot().unwrap().area_spent(AREA_AI),
        MicroUsd(expected)
    );
}

// spec: AI-014
// acceptance: independent
// `Cost = Token Cost + Fixed Fees + Regional Uplift` ist eine Invariante jeder gespeicherten Zeile.
#[test]
fn stored_rows_keep_cost_sum_invariant() {
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    insert(f.conn(), &record("decision", 100, 3_000, 310)).unwrap();
    let mut broken = record("decision", 100, 3_000, 310);
    broken.cost = MicroUsd(3_000);
    // Eine Zeile, die die Invariante verletzt, darf nicht im Protokoll landen.
    let _ = insert(f.conn(), &broken);
    for r in list(f.conn(), &RequestFilter::default()).unwrap() {
        let c = &r.record;
        assert_eq!(
            c.cost.0,
            c.token_cost.0 + c.fixed_fees.0 + c.regional_uplift.0,
            "Zeile {}",
            r.id
        );
    }
}

// spec: AI-014
// acceptance: independent
// `Refusal Reason` steht gekürzt (≤ 500 Zeichen) in einer eigenen Spalte.
#[test]
fn refusal_reason_is_truncated_to_500_chars() {
    assert_eq!(REFUSAL_REASON_MAX_CHARS, 500);
    let dir = tempfile::tempdir().unwrap();
    let path = create(dir.path());
    let f = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let long: String = "Ablehnung wegen Richtlinie – ü ".repeat(40); // > 500 Zeichen, mehrbytig
    assert!(long.chars().count() > 500);
    let mut r = record("decision", 10, 0, 0);
    r.status = "Refusal".into();
    r.refusal_reason = Some(long.clone());
    insert(f.conn(), &r).unwrap();
    let mut short = record("decision", 10, 0, 0);
    short.status = "Refusal".into();
    short.refusal_reason = Some("kurz".into());
    insert(f.conn(), &short).unwrap();
    let rows = list(f.conn(), &RequestFilter::default()).unwrap();
    let got = rows[0].record.refusal_reason.clone().unwrap();
    assert!(
        got.chars().count() <= 500,
        "{} Zeichen",
        got.chars().count()
    );
    assert!(long.starts_with(&got), "gekürzt, nicht verändert");
    assert!(
        got.chars().count() >= 450,
        "gekürzt, nicht verworfen: {} Zeichen",
        got.chars().count()
    );
    assert_eq!(rows[1].record.refusal_reason.as_deref(), Some("kurz"));
}

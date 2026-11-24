//! M4-K05: Hartes Crawl-Budget mit Reservierung vor jedem Request (AI-013) auf Bibliotheksebene: Reservierungs-
//! betrag aus `os_ai::cost`, gemeinsamer `CrawlBudgetLedger` aus `os-model`, Ist-Kosten nach der Tokenzählung von
//! ai-mock, Protokoll in der Crawl-Datei.
#[path = "cost_support.rs"]
mod support;

use os_ai::cost::reserve::{exhausted_task, status_for, stops_all_ai_entries};
use os_ai::cost::{
    cost_of, request_record, reservation_amount, FeeCounts, LogContext, Rates, RequestOutcome,
    TokenCounts,
};
use os_ai::provider::Usage;
use os_ai::AiStatus;
use os_model::budget::{
    BudgetError, BudgetLedger, BudgetLevel, BudgetLimits, BudgetScope, CrawlBudgetLedger, MicroUsd,
    SpendSource, AREA_AI,
};
use os_store::ai::requests::{insert, total_cost, AiRequestsSpend};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use support::{crawl_file, mock_input_tokens, openrouter_rates, site_pages, test_rates, TempDir};

fn limits(crawl: u64) -> BudgetLimits {
    BudgetLimits {
        crawl: MicroUsd(crawl),
        ..BudgetLimits::default()
    }
}

fn ctx(task: &str, model: &str) -> LogContext {
    LogContext {
        run_id: 1,
        timestamp: "2026-11-24T09:00:00Z".into(),
        task: task.into(),
        provider: "anthropic".into(),
        model: model.into(),
        endpoint: "messages".into(),
        url_id: None,
        definition_version: None,
        batch_id: None,
        redactions: 0,
    }
}

fn usage(input: u64, output: u64) -> Usage {
    Usage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: None,
        cache_write_tokens: None,
    }
}

/// Ein Eintrag wie im Lauf: reservieren, „senden“ (Mock-Usage), abrechnen. Liefert Status und Ist-Kosten.
#[allow(clippy::too_many_arguments)]
fn run_entry(
    ledger: &CrawlBudgetLedger,
    scope: BudgetScope,
    rates: &Rates,
    estimated_in: u64,
    max_tokens: u64,
    images: u64,
    mock_usage: (u64, u64),
    sent: &AtomicU64,
) -> (AiStatus, MicroUsd) {
    let amount = reservation_amount(estimated_in, max_tokens, images, rates, false);
    match ledger.reserve(scope, amount) {
        Ok(r) => {
            sent.fetch_add(1, Ordering::SeqCst);
            let actual = cost_of(
                &TokenCounts::plain(mock_usage.0, mock_usage.1),
                FeeCounts::one_request(images),
                rates,
                false,
            )
            .cost;
            ledger.settle(r.id, actual).unwrap();
            (AiStatus::Success, actual)
        }
        Err(e) => (status_for(&e).expect("nur Budgetfehler"), MicroUsd(0)),
    }
}

fn claude() -> Rates {
    test_rates("anthropic", "mock-claude", "messages", false)
}

// spec: AI-013
#[test]
fn budget_0_010_with_0_004_per_request_sends_exactly_two() {
    let ledger = CrawlBudgetLedger::new(limits(10_000));
    let sent = AtomicU64::new(0);
    // Reservierung ⌈800 × 1,25⌉ × 1 + 600 × 5 = 4.000 µUSD; Mock: 1.000/600 Tokens = 4.000 µUSD.
    let statuses: Vec<AiStatus> = (0..10)
        .map(|_| {
            run_entry(
                &ledger,
                BudgetScope::task(AREA_AI, "decision"),
                &claude(),
                800,
                600,
                0,
                (1_000, 600),
                &sent,
            )
            .0
        })
        .collect();
    assert_eq!(sent.load(Ordering::SeqCst), 2, "Mock-Zähler = 2");
    assert_eq!(
        statuses.iter().filter(|s| **s == AiStatus::Success).count(),
        2
    );
    assert!(statuses[2..].iter().all(|s| *s == AiStatus::BudgetExceeded));
    assert_eq!(AiStatus::BudgetExceeded.to_string(), "Budget exceeded");
    assert_eq!(ledger.snapshot().unwrap().spent, MicroUsd(8_000));
}

// spec: AI-013
#[test]
fn four_workers_200_entries_never_exceed_the_budget() {
    // Lasttest: 200 Einträge (Seiten der Fixture-Site im Wechsel), 4 Worker, Ist ≤ Reservierung.
    let pages: Vec<u64> = site_pages("site")
        .into_iter()
        .filter(|p| p.in_scope)
        .map(|p| p.text.chars().count() as u64)
        .collect();
    let rates = claude();
    let dir = TempDir::new("load");
    let file = Arc::new(Mutex::new(crawl_file(&dir.0)));
    for budget in [10_000u64, 33_333, 50_000] {
        file.lock()
            .unwrap()
            .conn()
            .execute("DELETE FROM ai_requests", [])
            .unwrap();
        let ledger = Arc::new(CrawlBudgetLedger::new(limits(budget)));
        let next = Arc::new(AtomicU64::new(0));
        let sent = Arc::new(AtomicU64::new(0));
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let (ledger, next, sent, file, pages, rates) = (
                    ledger.clone(),
                    next.clone(),
                    sent.clone(),
                    file.clone(),
                    pages.clone(),
                    rates.clone(),
                );
                std::thread::spawn(move || loop {
                    let i = next.fetch_add(1, Ordering::SeqCst) as usize;
                    if i >= 200 {
                        break;
                    }
                    let chars = pages[i % pages.len()];
                    let est = os_ai::cost::tokens::text_tokens(chars);
                    let max_tokens = 50;
                    let amount = reservation_amount(est, max_tokens, 0, &rates, false);
                    let task = if i.is_multiple_of(2) {
                        "decision"
                    } else {
                        "suggest-title"
                    };
                    let Ok(r) = ledger.reserve(BudgetScope::task(AREA_AI, task), amount) else {
                        continue;
                    };
                    sent.fetch_add(1, Ordering::SeqCst);
                    std::thread::yield_now();
                    let outcome = RequestOutcome::success(
                        usage(
                            mock_input_tokens(chars, 0),
                            (i as u64 * 7) % (max_tokens + 1),
                        ),
                        0,
                        5,
                    );
                    let record =
                        request_record(&ctx(task, "mock-claude"), &rates, amount, &outcome);
                    assert!(!record.over_reservation);
                    insert(file.lock().unwrap().conn(), &record).unwrap();
                    ledger.settle(r.id, record.cost).unwrap();
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
        let f = file.lock().unwrap();
        let logged = total_cost(f.conn()).unwrap();
        assert!(
            logged.0 <= budget,
            "Summe Ist {} > Budget {budget}",
            logged.0
        );
        assert_eq!(ledger.snapshot().unwrap().spent, logged);
        assert!(
            sent.load(Ordering::SeqCst) < 200,
            "Budget {budget} begrenzt"
        );
    }
}

// spec: AI-013
// spec: AI-014
#[test]
fn actual_above_the_reservation_is_booked_marked_and_stops_further_requests() {
    let rates = claude();
    let ledger = CrawlBudgetLedger::new(limits(1_000_000));
    let amount = reservation_amount(800, 600, 0, &rates, false);
    let r = ledger
        .reserve(BudgetScope::task(AREA_AI, "decision"), amount)
        .unwrap();
    // `max_tokens` überschritten: Mock meldet 5.000 statt höchstens 600 Ausgabe-Tokens.
    let record = request_record(
        &ctx("decision", "mock-claude"),
        &rates,
        amount,
        &RequestOutcome::success(usage(1_000, 5_000), 0, 7),
    );
    assert!(record.over_reservation);
    assert_eq!(record.cost, MicroUsd(26_000));
    let settled = ledger.settle(r.id, record.cost).unwrap();
    assert!(settled.over_reservation);
    assert_eq!(
        ledger.snapshot().unwrap().spent,
        MicroUsd(26_000),
        "Mehrbetrag verbucht"
    );
    let e = ledger
        .reserve(BudgetScope::task(AREA_AI, "decision"), amount)
        .unwrap_err();
    assert_eq!(e, BudgetError::Exceeded(BudgetLevel::Halted));
    assert!(stops_all_ai_entries(&e));
    assert_eq!(status_for(&e), Some(AiStatus::BudgetExceeded));
}

// spec: AI-013
#[test]
fn budget_zero_runs_only_ollama_and_cache_hits() {
    let ledger = CrawlBudgetLedger::new(limits(0));
    let sent = AtomicU64::new(0);
    let ollama = Rates::free(os_ai::cost::PriceSource::Table);
    let (s, cost) = run_entry(
        &ledger,
        BudgetScope::task(AREA_AI, "decision"),
        &ollama,
        5_000,
        500,
        0,
        (4_000, 300),
        &sent,
    );
    assert_eq!((s, cost), (AiStatus::Success, MicroUsd(0)));
    // Cache-Treffer: kein Request, nichts zu reservieren.
    assert_eq!(reservation_amount(0, 0, 0, &ollama, false), MicroUsd(0));
    let (s, _) = run_entry(
        &ledger,
        BudgetScope::task(AREA_AI, "decision"),
        &claude(),
        1,
        1,
        0,
        (1, 1),
        &sent,
    );
    assert_eq!(s, AiStatus::BudgetExceeded);
    assert_eq!(
        sent.load(Ordering::SeqCst),
        1,
        "nur der kostenfreie Request"
    );
}

// spec: AI-013
#[test]
fn task_sub_budget_stops_only_title_suggestions() {
    let mut l = limits(1_000_000);
    l.tasks.insert(
        AREA_AI.into(),
        BTreeMap::from([("suggest-title".to_string(), MicroUsd(500_000))]),
    );
    let ledger = CrawlBudgetLedger::new(l);
    let sent = AtomicU64::new(0);
    let rates = claude();
    let (mut titles, mut decisions) = (0, 0);
    let mut first_title_error = None;
    // Zwei Title-Vorschläge je Decision: das Aufgaben-Teilbudget ist vor dem Crawl-Budget erreicht.
    for i in 0..600u32 {
        let task = if !i.is_multiple_of(3) {
            "suggest-title"
        } else {
            "decision"
        };
        let amount = reservation_amount(800, 600, 0, &rates, false);
        match ledger.reserve(BudgetScope::task(AREA_AI, task), amount) {
            Ok(r) => {
                ledger.settle(r.id, MicroUsd(4_000)).unwrap();
                sent.fetch_add(1, Ordering::SeqCst);
                if task == "decision" {
                    decisions += 1
                } else {
                    titles += 1
                }
            }
            Err(e) if task == "suggest-title" => {
                first_title_error.get_or_insert(e);
            }
            Err(e) => assert_eq!(e, BudgetError::Exceeded(BudgetLevel::Crawl)),
        }
    }
    let e = first_title_error.unwrap();
    assert_eq!(exhausted_task(&e), Some("suggest-title"));
    assert!(!stops_all_ai_entries(&e), "nur Einträge dieser Aufgabe");
    assert_eq!(titles, 125, "0,50 USD / 0,004 USD");
    assert_eq!(
        decisions, 125,
        "Decisions laufen bis zum Crawl-Budget: 1,00 − 0,50"
    );
}

// spec: AI-013
#[test]
fn ai_total_sub_budget_0_008_allows_exactly_two_requests() {
    let mut l = limits(1_000_000);
    l.areas.insert(AREA_AI.into(), MicroUsd(8_000));
    let ledger = CrawlBudgetLedger::new(l);
    let sent = AtomicU64::new(0);
    let mut last = None;
    for task in ["decision", "suggest-title", "decision", "suggest-meta"] {
        let amount = reservation_amount(800, 600, 0, &claude(), false);
        match ledger.reserve(BudgetScope::task(AREA_AI, task), amount) {
            Ok(r) => {
                ledger.settle(r.id, MicroUsd(4_000)).unwrap();
                sent.fetch_add(1, Ordering::SeqCst);
            }
            Err(e) => last = Some(e),
        }
    }
    assert_eq!(sent.load(Ordering::SeqCst), 2);
    let e = last.unwrap();
    assert_eq!(e, BudgetError::Exceeded(BudgetLevel::Area(AREA_AI.into())));
    assert!(
        stops_all_ai_entries(&e),
        "alle offenen KI-Einträge: Budget exceeded"
    );
}

// spec: AI-013
#[test]
fn request_and_image_fees_are_reserved() {
    let vision = openrouter_rates("mock/vision");
    let ledger = CrawlBudgetLedger::new(limits(5_000));
    let sent = AtomicU64::new(0);
    // Schätzung 85 Bild-Tokens + 10 Text; Mock zählt 85 + ⌈20/4⌉.
    let amount = reservation_amount(95, 10, 1, &vision, false);
    assert!(amount >= MicroUsd(3_000));
    let scope = || BudgetScope::task(AREA_AI, "suggest-alt");
    let a = run_entry(&ledger, scope(), &vision, 95, 10, 1, (90, 10), &sent);
    let b = run_entry(&ledger, scope(), &vision, 95, 10, 1, (90, 10), &sent);
    assert_eq!(a.1, MicroUsd(3_140));
    assert!(!ledger.snapshot().unwrap().halted);
    assert_eq!((a.0, b.0), (AiStatus::Success, AiStatus::BudgetExceeded));
    assert_eq!(sent.load(Ordering::SeqCst), 1);
    assert!(ledger.snapshot().unwrap().spent <= MicroUsd(5_000));
}

// spec: AI-013
#[test]
fn spend_logged_in_the_crawl_file_counts_again_after_reopen() {
    let dir = TempDir::new("reopen");
    let path = {
        let f = crawl_file(&dir.0);
        let ledger =
            CrawlBudgetLedger::open(limits(10_000), &[&AiRequestsSpend(f.conn())]).unwrap();
        let rates = claude();
        let amount = reservation_amount(800, 600, 0, &rates, false);
        let r = ledger
            .reserve(BudgetScope::task(AREA_AI, "decision"), amount)
            .unwrap();
        let rec = request_record(
            &ctx("decision", "mock-claude"),
            &rates,
            amount,
            &RequestOutcome::success(usage(1_000, 600), 0, 3),
        );
        insert(f.conn(), &rec).unwrap();
        ledger.settle(r.id, rec.cost).unwrap();
        os_store::ai::budget::save_limits(f.conn(), &limits(10_000)).unwrap();
        f.path().to_path_buf()
    };
    let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
    let l = os_store::ai::budget::load_limits(f.conn())
        .unwrap()
        .unwrap();
    let ledger = CrawlBudgetLedger::open(l, &[&AiRequestsSpend(f.conn())]).unwrap();
    assert_eq!(ledger.snapshot().unwrap().spent, MicroUsd(4_000));
    let sent = AtomicU64::new(0);
    let statuses: Vec<_> = (0..3)
        .map(|_| {
            run_entry(
                &ledger,
                BudgetScope::task(AREA_AI, "decision"),
                &claude(),
                800,
                600,
                0,
                (1_000, 600),
                &sent,
            )
            .0
        })
        .collect();
    assert_eq!(
        statuses,
        [
            AiStatus::Success,
            AiStatus::BudgetExceeded,
            AiStatus::BudgetExceeded
        ]
    );
}

/// Zweiter Bereich ohne Kenntnis von os-ai (Gegenstück zu `dfs_cost_log` ab M5).
struct OtherArea(u64);
impl SpendSource for OtherArea {
    fn area(&self) -> &str {
        "dataforseo"
    }
    fn spent_by_task(&self) -> Result<BTreeMap<String, MicroUsd>, BudgetError> {
        Ok(BTreeMap::from([(String::new(), MicroUsd(self.0))]))
    }
}

// spec: AI-013 [M5]
#[test]
#[ignore = "M5: DataForSEO-Bereich mit dfs_cost_log und DataForSEO-Mock (M5-K03)"]
fn shared_budget_with_dataforseo_spend_rejects_0_06_and_admits_0_04() {
    let dir = TempDir::new("m5");
    let f = crawl_file(&dir.0);
    let rec = request_record(
        &ctx("decision", "mock-claude"),
        &claude(),
        MicroUsd(250_000),
        &RequestOutcome::success(usage(250_000, 0), 0, 1),
    );
    insert(f.conn(), &rec).unwrap();
    let ledger = CrawlBudgetLedger::open(
        limits(1_000_000),
        &[&AiRequestsSpend(f.conn()), &OtherArea(700_000)],
    )
    .unwrap();
    assert_eq!(
        ledger.reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(60_000)),
        Err(BudgetError::Exceeded(BudgetLevel::Crawl))
    );
    assert!(ledger
        .reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(40_000))
        .is_ok());
}

// spec: AI-013 [M5]
#[test]
#[ignore = "M5: je 4 KI- und DataForSEO-Worker gegen ai-mock und DataForSEO-Mock (M5-K03)"]
fn four_ai_and_four_dataforseo_workers_stay_within_the_crawl_budget() {
    unimplemented!("M5-K03: DataForSEO-Mock und dfs_cost_log fehlen in M4");
}

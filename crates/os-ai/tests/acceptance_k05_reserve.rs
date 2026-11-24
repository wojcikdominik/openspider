//! Unabhängige Abnahmetests M4-K05: Reservierungsbetrag vor jedem KI-Request und Zusammenspiel mit dem
//! `BudgetLedger` (AI-013).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md`: AI-013 (Reservierung = geschätzte Eingabe-Tokens × 1,25 ×
//! Eingabepreis + `max_tokens` × Ausgabepreis + Gebühr je Request + Gebühr je Bild × Bilder, bei Batch mit Rabatt,
//! zuzüglich Regionalzuschlag; Langkontext-Faktoren, sobald geschätzte Eingabe-Tokens × 1,25 die Schwelle
//! erreichen), AI-010 (Preise, Langkontext, Regionalzuschlag), „Konventionen“ (`pricing-test.json`,
//! OpenRouter-Vision-Modell 0,001 je Request + 0,002 je Bild) und Paketauftrag M4-K05 (Budget wird bei korrektem
//! Preiskatalog nie überschritten, auch bei parallelen Workern).
// acceptance: independent

mod acceptance_k05_support;

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use acceptance_k05_support::*;
use os_ai::cost::reserve::status_for;
use os_ai::cost::{cost_of, reservation_amount, FeeCounts, LongContext, Rates, TokenCounts};
use os_ai::AiStatus;
use os_model::budget::{
    BudgetError, BudgetLedger, BudgetLimits, BudgetScope, CrawlBudgetLedger, MicroUsd, AREA_AI,
};
use proptest::prelude::*;

fn claude() -> Rates {
    fixture_rates("anthropic", "mock-claude", "messages", false).unwrap()
}
fn luna_responses(eu: bool) -> Rates {
    fixture_rates("openai", "gpt-6-luna", "responses", eu).unwrap()
}

fn crawl_only(limit: u64) -> BudgetLimits {
    BudgetLimits {
        crawl: MicroUsd(limit),
        ..BudgetLimits::default()
    }
}

// spec: AI-013
// acceptance: independent
// Fixture: `pricing-test.json` enthält die Spec-Testpreise (Grundlage aller Prüfwerte dieser Datei).
#[test]
fn fixture_prices_match_spec_conventions() {
    let c = claude();
    assert_eq!((c.input_pico, c.output_pico), (1_000_000, 5_000_000));
    let r = luna_responses(true);
    assert_eq!((r.input_pico, r.output_pico), (100_000, 500_000));
    assert_eq!(r.cache_read_pico, 10_000);
    assert_eq!(r.batch_discount_ppm, 500_000);
    assert_eq!(r.regional_uplift_ppm, 100_000);
    let lc = r.long_context.unwrap();
    assert_eq!(lc.threshold_input_tokens, 272_000);
    assert_eq!(
        (lc.input_factor_ppm, lc.output_factor_ppm),
        (2_000_000, 1_500_000)
    );
    let d = fixture_rates("openai", "gpt-6-luna", "decisions", false).unwrap();
    assert_eq!((d.input_pico, d.output_pico), (100_000, 0));
    assert!(fixture_rates("openai", "gpt-6-luna", "messages", false).is_none());
}

// spec: AI-013
// acceptance: independent
// Reservierungsformel: geschätzte Eingabe × 1,25 × Eingabepreis + `max_tokens` × Ausgabepreis.
// mock-claude (1/5 USD je 1M): 800 geschätzt → 1.000 × 1 + 200 × 5 = 2.000 µUSD.
#[test]
fn reservation_basic_formula_mock_claude() {
    assert_eq!(
        reservation_amount(800, 200, 0, &claude(), false),
        MicroUsd(2_000)
    );
    // 1.600 geschätzt → 2.000 × 1 + 400 × 5 = 4.000 µUSD (= 0,004 USD, AI-013-Prüffall).
    assert_eq!(
        reservation_amount(1_600, 400, 0, &claude(), false),
        MicroUsd(4_000)
    );
    // Ohne Eingabe nur max_tokens.
    assert_eq!(
        reservation_amount(0, 1_000, 0, &claude(), false),
        MicroUsd(5_000)
    );
}

// spec: AI-013
// acceptance: independent
// Batch mit Rabatt: gpt-6-luna/responses 800 → 1.000 × 0,10 + 200 × 0,50 = 200 µUSD, Batch 50 % → 100 µUSD.
#[test]
fn reservation_with_batch_discount() {
    let r = luna_responses(false);
    assert_eq!(reservation_amount(800, 200, 0, &r, false), MicroUsd(200));
    assert_eq!(reservation_amount(800, 200, 0, &r, true), MicroUsd(100));
}

// spec: AI-013
// acceptance: independent
// Regionalzuschlag 10 % (OpenAI-EU) auf den Betrag: 200 → 220 µUSD.
#[test]
fn reservation_with_regional_uplift() {
    assert_eq!(
        reservation_amount(800, 200, 0, &luna_responses(true), false),
        MicroUsd(220)
    );
}

// spec: AI-013
// acceptance: independent
// Langkontext-Faktoren, sobald geschätzte Eingabe × 1,25 die Schwelle (272.000) erreicht:
// 217.600 × 1,25 = 272.000 → 272.000 × 0,20 + 1.000 × 0,75 = 54.400 + 750 = 55.150 µUSD;
// 217.592 × 1,25 = 271.990 < 272.000 → 271.990 × 0,10 + 1.000 × 0,50 = 27.199 + 500 = 27.699 µUSD.
#[test]
fn reservation_long_context_threshold_uses_inflated_estimate() {
    let r = luna_responses(false);
    assert_eq!(
        reservation_amount(217_600, 1_000, 0, &r, false),
        MicroUsd(55_150)
    );
    assert_eq!(
        reservation_amount(217_592, 1_000, 0, &r, false),
        MicroUsd(27_699)
    );
    // Mit EU-Zuschlag 10 % auf den Langkontext-Betrag: 55.150 × 1,1 = 60.665.
    assert_eq!(
        reservation_amount(217_600, 1_000, 0, &luna_responses(true), false),
        MicroUsd(60_665)
    );
}

// spec: AI-013
// acceptance: independent
// Gebühren in der Reservierung: OpenRouter-Vision 0,001 je Request + 0,002 je Bild.
// 100 geschätzt → 125 × 1 + 100 × 5 = 625 µUSD Tokens + 1.000 + 2.000 × Bilder.
#[test]
fn reservation_includes_request_and_image_fees() {
    let v = openrouter_vision();
    assert_eq!(reservation_amount(0, 0, 0, &v, false), MicroUsd(1_000));
    assert_eq!(reservation_amount(0, 0, 1, &v, false), MicroUsd(3_000));
    assert_eq!(reservation_amount(100, 100, 1, &v, false), MicroUsd(3_625));
    assert_eq!(reservation_amount(100, 100, 2, &v, false), MicroUsd(5_625));
    assert!(reservation_amount(100, 100, 1, &v, false) >= MicroUsd(3_000));
}

// spec: AI-013
// acceptance: independent
// Gebühren werden reserviert: Budget 0,005 USD, Bildaufgabe über das OpenRouter-Vision-Modell
// (Reservierung ≥ 0,003 USD je Request) → genau 1 Request, der zweite Eintrag erhält `Budget exceeded`;
// Summe der Ist-Kosten ≤ 0,005 USD.
#[test]
fn fees_reserved_budget_five_milli_allows_one_image_request() {
    let v = openrouter_vision();
    let ledger = CrawlBudgetLedger::new(crawl_only(5_000));
    let mut sent = 0u64;
    let mut actual_sum = 0u64;
    let mut statuses = Vec::new();
    for _ in 0..3 {
        let res = reservation_amount(100, 100, 1, &v, false);
        match ledger.reserve(BudgetScope::task(AREA_AI, "suggest-alt"), res) {
            Ok(r) => {
                // Ist: 120 Eingabe, 80 Ausgabe, 1 Bild (≤ Reservierung).
                let actual = cost_of(
                    &TokenCounts::plain(120, 80),
                    FeeCounts::one_request(1),
                    &v,
                    false,
                );
                assert_eq!(actual.fixed_fees, MicroUsd(3_000));
                ledger.settle(r.id, actual.cost).unwrap();
                actual_sum += actual.cost.0;
                sent += 1;
            }
            Err(e) => statuses.push(status_for(&e)),
        }
    }
    assert_eq!(sent, 1);
    assert!(actual_sum <= 5_000);
    assert_eq!(statuses, vec![Some(AiStatus::BudgetExceeded); 2]);
    assert_eq!(AiStatus::BudgetExceeded.to_string(), "Budget exceeded");
}

// spec: AI-013
// acceptance: independent
// Crawl-Budget 0,010 USD, jeder Request kostet 0,004 USD, Reservierung ≤ 0,004 USD → genau 2 Requests.
#[test]
fn ten_milli_budget_with_formula_sends_exactly_two() {
    let c = claude();
    let ledger = CrawlBudgetLedger::new(crawl_only(10_000));
    let mut sent = 0;
    for _ in 0..6 {
        let res = reservation_amount(1_600, 400, 0, &c, false);
        assert!(res <= MicroUsd(4_000));
        if let Ok(r) = ledger.reserve(BudgetScope::task(AREA_AI, "decision"), res) {
            let actual = cost_of(
                &TokenCounts::plain(2_000, 400),
                FeeCounts::one_request(0),
                &c,
                false,
            );
            assert_eq!(actual.cost, MicroUsd(4_000));
            ledger.settle(r.id, actual.cost).unwrap();
            sent += 1;
        }
    }
    assert_eq!(sent, 2);
}

// spec: AI-013
// acceptance: independent
// Budget 0: Betrag 0 (Ollama, kostenlose Sätze) passt, alles Bezahlte nicht.
#[test]
fn zero_budget_runs_only_free_rates() {
    let ollama = Rates::free(os_ai::cost::PriceSource::Table);
    let ledger = CrawlBudgetLedger::new(crawl_only(0));
    let free = reservation_amount(5_000, 1_000, 2, &ollama, false);
    assert_eq!(free, MicroUsd(0));
    let r = ledger
        .reserve(BudgetScope::task(AREA_AI, "decision"), free)
        .unwrap();
    ledger.settle(r.id, MicroUsd(0)).unwrap();
    let paid = reservation_amount(1, 1, 0, &claude(), false);
    assert!(paid > MicroUsd(0));
    assert!(matches!(
        ledger.reserve(BudgetScope::task(AREA_AI, "decision"), paid),
        Err(BudgetError::Exceeded(_))
    ));
}

fn arb_rates() -> impl Strategy<Value = Rates> {
    (
        0u64..20_000_000,
        0u64..80_000_000,
        0u64..=1_000_000,
        prop::option::of((
            1u64..400_000,
            1_000_000u64..3_000_000,
            1_000_000u64..3_000_000,
        )),
        prop_oneof![Just(0u64), Just(100_000u64), 0u64..300_000],
        prop_oneof![Just(0u64), 0u64..5_000_000_000],
        prop_oneof![Just(0u64), 0u64..5_000_000_000],
    )
        .prop_map(|(i, o, batch, lc, up, req, img)| Rates {
            input_pico: i,
            output_pico: o,
            cache_read_pico: 0,
            cache_write_pico: 0,
            batch_discount_ppm: batch,
            long_context: lc.map(|(t, fi, fo)| LongContext {
                threshold_input_tokens: t,
                input_factor_ppm: fi,
                output_factor_ppm: fo,
                cache_read_factor_ppm: PPM,
            }),
            regional_uplift_ppm: up,
            per_request_pico: req,
            per_image_pico: img,
            image_token_estimate: 85,
            source: os_ai::cost::PriceSource::Table,
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    // spec: AI-013
    // acceptance: independent
    // Bei korrektem Preiskatalog liegt jede Ist-Antwort mit Eingabe ≤ geschätzt × 1,25 und Ausgabe ≤ `max_tokens`
    // (gleiche Bilderzahl, gleicher Batch-Modus) nie über ihrer Reservierung – auch mit Rundung, Gebühren,
    // Regionalzuschlag, Batch-Rabatt und Langkontext.
    #[test]
    fn actual_within_reserved_tokens_never_exceeds_reservation(
        rates in arb_rates(),
        est in 0u64..600_000,
        max_tokens in 0u64..64_000,
        in_frac in 0u64..=1_000,
        out_frac in 0u64..=1_000,
        images in 0u64..5,
        batch in any::<bool>(),
    ) {
        let reserved_in = est * 5 / 4; // ⌊geschätzt × 1,25⌋: höchstens so viele ganze Tokens deckt die Spec ab
        let actual_in = reserved_in * in_frac / 1_000;
        let actual_out = max_tokens * out_frac / 1_000;
        let res = reservation_amount(est, max_tokens, images, &rates, batch);
        let actual = cost_of(
            &TokenCounts::plain(actual_in, actual_out),
            FeeCounts::one_request(images),
            &rates,
            batch,
        );
        prop_assert!(
            actual.cost <= res,
            "Ist {:?} > Reservierung {:?} (est {}, in {}, out {}, max {})",
            actual.cost, res, est, actual_in, actual_out, max_tokens
        );
    }
}

/// Lasttest mit Formel: 4 Worker, 200 Einträge (Text über mock-claude, Bilder über OpenRouter-Vision, Batch und
/// EU über gpt-6-luna); Ist ≤ reservierte Tokens. Liefert die Summe der Ist-Kosten.
fn formula_load(budget: u64, seed: u64) -> (u64, Arc<CrawlBudgetLedger>) {
    let rates = Arc::new([
        claude(),
        openrouter_vision(),
        luna_responses(false),
        luna_responses(true),
    ]);
    let mut rng = Lcg(seed);
    let plan: Arc<Vec<(usize, u64, u64, u64, bool, u64, u64)>> = Arc::new(
        (0..200)
            .map(|_| {
                let which = rng.range(0, 3) as usize;
                let est = rng.range(0, 3_000);
                let max = rng.range(1, 800);
                let images = if which == 1 { rng.range(0, 2) } else { 0 };
                let batch = which == 2 && rng.range(0, 1) == 1;
                let ain = est * 5 / 4 * rng.range(0, 100) / 100;
                let aout = max * rng.range(0, 100) / 100;
                (which, est, max, images, batch, ain, aout)
            })
            .collect(),
    );
    let ledger = Arc::new(CrawlBudgetLedger::new(crawl_only(budget)));
    let next = Arc::new(AtomicUsize::new(0));
    let total = Arc::new(AtomicU64::new(0));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let (rates, plan, ledger, next, total) = (
                rates.clone(),
                plan.clone(),
                ledger.clone(),
                next.clone(),
                total.clone(),
            );
            thread::spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= plan.len() {
                    break;
                }
                let (which, est, max, images, batch, ain, aout) = plan[i];
                let r = &rates[which];
                let res = reservation_amount(est, max, images, r, batch);
                let task = ["decision", "suggest-alt", "suggest-title", "suggest-meta"][which];
                if let Ok(resv) = ledger.reserve(BudgetScope::task(AREA_AI, task), res) {
                    thread::yield_now();
                    let actual = cost_of(
                        &TokenCounts::plain(ain, aout),
                        FeeCounts::one_request(images),
                        r,
                        batch,
                    );
                    let s = ledger.settle(resv.id, actual.cost).unwrap();
                    assert!(!s.over_reservation, "Ist über Reservierung bei Eintrag {i}");
                    total.fetch_add(actual.cost.0, Ordering::SeqCst);
                }
            })
        })
        .collect();
    for w in workers {
        w.join().unwrap();
    }
    (total.load(Ordering::SeqCst), ledger)
}

// spec: AI-013
// acceptance: independent
// 4 gleichzeitige Worker, 200 Einträge, Reservierung nach Formel: Summe der Ist-Kosten ≤ Budget
// (mehrere Budgets und Startwerte).
#[test]
fn parallel_workers_with_formula_never_exceed_budget() {
    for budget in [3_000u64, 10_000, 47_311, 250_000, 5_000_000] {
        for seed in 1..=5u64 {
            let (total, ledger) = formula_load(budget, seed * 1_000 + budget);
            assert!(total <= budget, "Budget {budget}, Seed {seed}: Ist {total}");
            let snap = ledger.snapshot().unwrap();
            assert_eq!(snap.spent, MicroUsd(total));
            assert_eq!(snap.reserved, MicroUsd(0));
            assert!(!snap.halted);
        }
    }
}

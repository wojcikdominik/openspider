//! Unabhängige Abnahmetests M4-K05: Token- und Kostenprotokoll `ai_requests` (AI-014).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` AI-014 (Spalten, Summenzeile je Aufgabe und Modell,
//! `Cost = Token Cost + Fixed Fees + Regional Uplift`, OpenRouter-Bildrequest mit 1 Bild → `Fixed Fees = 0.003000`,
//! Cache-Treffer mit `Cost = 0`, Wiederholungen als eine Zeile mit `Retries = n`, Kosten fehlgeschlagener Versuche
//! ohne `usage` = 0, `Price Source = estimate` ohne `usage`, Summen = KI-Anteil der Budgetanzeige auf 1 µUSD),
//! AI-013 (`over_reservation = true`), AI-010 (Preise, Langkontext, EU-Zuschlag auf den Gesamtbetrag) und
//! „Konventionen“ (`pricing-test.json`).
// acceptance: independent

mod acceptance_k05_support;

use acceptance_k05_support::*;
use os_ai::cost::{
    cache_hit_record, cost_of, request_record, FeeCounts, PriceSource, Rates, RequestOutcome,
    TokenCounts, UsageReport,
};
use os_ai::AiStatus;
use os_model::budget::{BudgetLedger, BudgetLimits, CrawlBudgetLedger, MicroUsd, AREA_AI};
use os_store::ai::requests::{
    insert, list, total_cost, totals_by_task_and_model, AiRequestsSpend, RequestFilter,
};
use proptest::prelude::*;

fn claude() -> Rates {
    fixture_rates("anthropic", "mock-claude", "messages", false).unwrap()
}

// spec: AI-014
// acceptance: independent
// Nach 3 Mock-Antworten mit je 1.000 Eingabe- und 200 Ausgabe-Tokens auf mock-claude (1/5 USD je 1M):
// Summenzeile `Input Tokens 3000`, `Output Tokens 600`, `Cost 0.006000` (= 6.000 µUSD).
#[test]
fn totals_row_three_mock_claude_answers() {
    let dir = TempDir::new("totals");
    let f = crawl_file(&dir.0);
    let c = claude();
    for _ in 0..3 {
        let rec = request_record(
            &ctx("suggest-title", "anthropic", "mock-claude", "messages"),
            &c,
            MicroUsd(10_000),
            &RequestOutcome::success(usage(1_000, 200), 0, 25),
        );
        assert_eq!(rec.cost, MicroUsd(2_000));
        assert_eq!(rec.price_source, PriceSource::Table);
        insert(f.conn(), &rec).unwrap();
    }
    let totals = totals_by_task_and_model(f.conn()).unwrap();
    assert_eq!(totals.len(), 1);
    let t = &totals[0];
    assert_eq!(
        (t.task.as_str(), t.model.as_str()),
        ("suggest-title", "mock-claude")
    );
    assert_eq!(t.input_tokens, 3_000);
    assert_eq!(t.output_tokens, 600);
    assert_eq!(t.cost, MicroUsd(6_000));
    assert_eq!(t.requests, 3);
}

// spec: AI-014
// acceptance: independent
// Summenzeilen je Aufgabe UND Modell getrennt.
#[test]
fn totals_are_grouped_by_task_and_model() {
    let dir = TempDir::new("group");
    let f = crawl_file(&dir.0);
    let c = claude();
    let d = fixture_rates("openai", "gpt-6-luna", "decisions", false).unwrap();
    for (task, provider, model, endpoint, rates) in [
        ("decision", "openai", "gpt-6-luna", "decisions", &d),
        ("decision", "anthropic", "mock-claude", "messages", &c),
        ("suggest-title", "anthropic", "mock-claude", "messages", &c),
        ("decision", "openai", "gpt-6-luna", "decisions", &d),
    ] {
        let rec = request_record(
            &ctx(task, provider, model, endpoint),
            rates,
            MicroUsd(1_000_000),
            &RequestOutcome::success(usage(1_000, 200), 0, 5),
        );
        insert(f.conn(), &rec).unwrap();
    }
    let mut totals: Vec<(String, String, u64, u64)> = totals_by_task_and_model(f.conn())
        .unwrap()
        .into_iter()
        .map(|t| (t.task, t.model, t.requests, t.cost.0))
        .collect();
    totals.sort();
    assert_eq!(
        totals,
        vec![
            ("decision".into(), "gpt-6-luna".into(), 2, 200), // 2 × 100 µUSD (nur Eingabe, AI-010)
            ("decision".into(), "mock-claude".into(), 1, 2_000),
            ("suggest-title".into(), "mock-claude".into(), 1, 2_000),
        ]
    );
}

// spec: AI-014
// acceptance: independent
// OpenRouter-Bildrequest mit 1 Bild → `Fixed Fees = 0.003000` (3.000 µUSD), Preisquelle `api`; Cost = Summe.
#[test]
fn openrouter_image_request_fixed_fees() {
    let v = openrouter_vision();
    let rec = request_record(
        &ctx(
            "suggest-alt",
            "openrouter",
            "mock/vision",
            "chat-completions",
        ),
        &v,
        MicroUsd(10_000),
        &RequestOutcome::success(usage(300, 40), 1, 30),
    );
    assert_eq!(rec.fixed_fees, MicroUsd(3_000));
    assert_eq!(rec.token_cost, MicroUsd(300 + 200));
    assert_eq!(rec.regional_uplift, MicroUsd(0));
    assert_eq!(rec.cost, MicroUsd(3_500));
    assert_eq!(rec.images, 1);
    assert_eq!(rec.price_source, PriceSource::Api);
    let dir = TempDir::new("or");
    let f = crawl_file(&dir.0);
    insert(f.conn(), &rec).unwrap();
    let rows = list(f.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(rows[0].record.fixed_fees, MicroUsd(3_000));
}

// spec: AI-014
// acceptance: independent
// Regionalzuschlag als eigene Spalte, auf den Gesamtbetrag: gpt-6-luna/responses EU, 1.000/200 Tokens →
// Token Cost 200, Uplift 20, Cost 220; Langkontext 300.000/1.000 → 60.750 + 6.075 = 66.825 (AI-010).
#[test]
fn regional_uplift_and_long_context_columns() {
    let eu = fixture_rates("openai", "gpt-6-luna", "responses", true).unwrap();
    let rec = request_record(
        &ctx("suggest-title", "openai", "gpt-6-luna", "responses"),
        &eu,
        MicroUsd(1_000_000),
        &RequestOutcome::success(usage(1_000, 200), 0, 5),
    );
    assert_eq!(
        (
            rec.token_cost,
            rec.fixed_fees,
            rec.regional_uplift,
            rec.cost
        ),
        (MicroUsd(200), MicroUsd(0), MicroUsd(20), MicroUsd(220))
    );
    let rec = request_record(
        &ctx("suggest-title", "openai", "gpt-6-luna", "responses"),
        &eu,
        MicroUsd(1_000_000),
        &RequestOutcome::success(usage(300_000, 1_000), 0, 5),
    );
    assert_eq!(rec.token_cost, MicroUsd(60_750));
    assert_eq!(rec.regional_uplift, MicroUsd(6_075));
    assert_eq!(rec.cost, MicroUsd(66_825));
    // Ohne EU kein Zuschlag; Decisions 1.000 Eingabe → 100 µUSD.
    let global = fixture_rates("openai", "gpt-6-luna", "responses", false).unwrap();
    assert_eq!(
        cost_of(
            &TokenCounts::plain(1_000, 200),
            FeeCounts::one_request(0),
            &global,
            false
        )
        .cost,
        MicroUsd(200)
    );
    let dec = fixture_rates("openai", "gpt-6-luna", "decisions", false).unwrap();
    assert_eq!(
        cost_of(
            &TokenCounts::plain(1_000, 200),
            FeeCounts::one_request(0),
            &dec,
            false
        )
        .cost,
        MicroUsd(100)
    );
}

// spec: AI-014
// acceptance: independent
// Cache-Treffer erzeugen eine Zeile mit `Cache Hit = true`, `Cost = 0`, ohne HTTP-Request (kein HTTP-Status).
#[test]
fn cache_hit_row_costs_zero() {
    let dir = TempDir::new("cache");
    let f = crawl_file(&dir.0);
    let rec = cache_hit_record(
        &ctx("suggest-title", "anthropic", "mock-claude", "messages"),
        PriceSource::Table,
        0,
    );
    assert!(rec.cache_hit);
    assert_eq!(rec.cost, MicroUsd(0));
    assert_eq!(rec.token_cost, MicroUsd(0));
    assert_eq!(rec.fixed_fees, MicroUsd(0));
    assert_eq!(rec.regional_uplift, MicroUsd(0));
    assert_eq!(rec.http_status, None);
    insert(f.conn(), &rec).unwrap();
    let rows = list(f.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].record.cache_hit);
    assert_eq!(rows[0].record.cost, MicroUsd(0));
    assert_eq!(total_cost(f.conn()).unwrap(), MicroUsd(0));
}

// spec: AI-014
// acceptance: independent
// Wiederholte Versuche erscheinen als eine Zeile mit `Retries = n`; Kosten fehlgeschlagener Versuche ohne `usage`
// sind 0.
#[test]
fn retries_one_row_and_failed_attempts_free() {
    let dir = TempDir::new("retry");
    let f = crawl_file(&dir.0);
    let c = claude();
    // 429x2: zwei Fehlversuche, dann Erfolg → eine Zeile, Retries = 2, Kosten nur der erfolgreichen Antwort.
    let mut ok = RequestOutcome::success(usage(1_000, 200), 0, 40);
    ok.retries = 2;
    let rec = request_record(
        &ctx("decision", "anthropic", "mock-claude", "messages"),
        &c,
        MicroUsd(10_000),
        &ok,
    );
    assert_eq!(rec.retries, 2);
    assert_eq!(rec.cost, MicroUsd(2_000));
    insert(f.conn(), &rec).unwrap();
    // Alle Versuche fehlgeschlagen, keine `usage` → Kosten 0.
    let failed = RequestOutcome {
        status: AiStatus::HttpError {
            code: 500,
            message: "server error".into(),
        },
        usage: UsageReport::NoResponse,
        images: 0,
        batch: false,
        http_status: Some(500),
        error_code: None,
        error_message: Some("server error".into()),
        refusal_reason: None,
        duration_ms: 90,
        provider_request_id: None,
        retries: 5,
        confidence_mismatch: false,
        estimated_input_tokens: Some(800),
    };
    let rec = request_record(
        &ctx("decision", "anthropic", "mock-claude", "messages"),
        &c,
        MicroUsd(10_000),
        &failed,
    );
    assert_eq!(rec.cost, MicroUsd(0));
    assert_eq!(rec.input_tokens, None);
    insert(f.conn(), &rec).unwrap();
    let rows = list(f.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(rows.len(), 2, "je Request eine Zeile, nicht je Versuch");
    assert_eq!(
        rows.iter().map(|r| r.record.retries).collect::<Vec<_>>(),
        vec![2, 5]
    );
    assert_eq!(total_cost(f.conn()).unwrap(), MicroUsd(2_000));
}

// spec: AI-014
// acceptance: independent
// Antwort ohne `usage`: Tokenfelder leer (`null`), Kosten aus der lokalen Schätzung, `Price Source = estimate`.
#[test]
fn missing_usage_is_estimate_with_null_tokens() {
    let dec = fixture_rates("openai", "gpt-6-luna", "decisions", false).unwrap();
    let mut o = RequestOutcome::success(Default::default(), 0, 10);
    o.usage = UsageReport::Missing {
        estimated_input_tokens: 2_000,
        estimated_output_tokens: 0,
    };
    let rec = request_record(
        &ctx("decision", "openai", "gpt-6-luna", "decisions"),
        &dec,
        MicroUsd(10_000),
        &o,
    );
    assert_eq!(rec.price_source, PriceSource::Estimate);
    assert_eq!((rec.input_tokens, rec.output_tokens), (None, None));
    assert_eq!(rec.cost, MicroUsd(200));
    let dir = TempDir::new("est");
    let f = crawl_file(&dir.0);
    insert(f.conn(), &rec).unwrap();
    let row = &list(f.conn(), &RequestFilter::default()).unwrap()[0].record;
    assert_eq!(row.price_source, PriceSource::Estimate);
    assert_eq!(row.input_tokens, None);
}

// spec: AI-013
// acceptance: independent
// Überschreitet die Ist-Antwort ihre Reservierung, markiert das Protokoll den Eintrag mit `over_reservation = true`.
#[test]
fn over_reservation_flag_in_log() {
    let c = claude();
    let over = request_record(
        &ctx("decision", "anthropic", "mock-claude", "messages"),
        &c,
        MicroUsd(2_000),
        &RequestOutcome::success(usage(1_000, 2_000), 0, 10),
    );
    assert_eq!(over.cost, MicroUsd(11_000));
    assert!(over.over_reservation);
    let exact = request_record(
        &ctx("decision", "anthropic", "mock-claude", "messages"),
        &c,
        MicroUsd(2_000),
        &RequestOutcome::success(usage(1_000, 200), 0, 10),
    );
    assert!(!exact.over_reservation);
    let dir = TempDir::new("over");
    let f = crawl_file(&dir.0);
    insert(f.conn(), &over).unwrap();
    assert!(
        list(f.conn(), &RequestFilter::default()).unwrap()[0]
            .record
            .over_reservation
    );
}

// spec: AI-014
// acceptance: independent
// `Refusal Reason` höchstens 500 Zeichen (gekürzt beim Protokollieren).
#[test]
fn refusal_reason_truncated_in_log() {
    let dir = TempDir::new("refusal");
    let f = crawl_file(&dir.0);
    let mut o = RequestOutcome::success(usage(100, 10), 0, 10);
    o.status = AiStatus::Refusal;
    o.refusal_reason = Some("x".repeat(499) + &"é".repeat(300));
    let rec = request_record(
        &ctx("decision", "anthropic", "mock-claude", "messages"),
        &claude(),
        MicroUsd(10_000),
        &o,
    );
    insert(f.conn(), &rec).unwrap();
    let row = &list(f.conn(), &RequestFilter::default()).unwrap()[0].record;
    let reason = row.refusal_reason.as_deref().unwrap();
    assert_eq!(reason.chars().count(), 500);
    assert_eq!(row.status, "Refusal");
}

// spec: AI-014
// acceptance: independent
// Summen im Protokoll und der KI-Anteil der Budgetanzeige stimmen auf 1 µUSD überein: abgerechnete Ist-Beträge im
// Ledger = Summe `Cost` aus `ai_requests`, auch nach Neuaufbau des Ledgers aus der Datei.
#[test]
fn log_sum_matches_ledger_to_the_micro_usd() {
    let dir = TempDir::new("sum");
    let f = crawl_file(&dir.0);
    let ledger = CrawlBudgetLedger::new(BudgetLimits::default());
    let all = [
        claude(),
        openrouter_vision(),
        fixture_rates("openai", "gpt-6-luna", "responses", true).unwrap(),
        fixture_rates("openai", "gpt-6-luna", "decisions", false).unwrap(),
    ];
    let mut rng = Lcg(4242);
    for i in 0..120 {
        let which = rng.range(0, 3) as usize;
        let r = &all[which];
        let images = if which == 1 {
            rng.range(0, 3) as u32
        } else {
            0
        };
        let est = rng.range(1, 5_000);
        let max = rng.range(1, 1_000);
        let res = os_ai::cost::reservation_amount(est, max, u64::from(images), r, false);
        let task = ["decision", "suggest-alt", "suggest-title", "decision"][which];
        let resv = ledger
            .reserve(os_model::budget::BudgetScope::task(AREA_AI, task), res)
            .unwrap();
        let outcome = if i % 17 == 0 {
            RequestOutcome {
                usage: UsageReport::NoResponse,
                ..RequestOutcome::success(Default::default(), images, 1)
            }
        } else {
            RequestOutcome::success(usage(rng.range(0, est), rng.range(0, max)), images, 1)
        };
        let rec = request_record(&ctx(task, "p", "m", "e"), r, res, &outcome);
        insert(f.conn(), &rec).unwrap();
        ledger.settle(resv.id, rec.cost).unwrap();
    }
    let log_sum = total_cost(f.conn()).unwrap();
    let totals_sum: u64 = totals_by_task_and_model(f.conn())
        .unwrap()
        .iter()
        .map(|t| t.cost.0)
        .sum();
    assert_eq!(MicroUsd(totals_sum), log_sum);
    assert_eq!(ledger.snapshot().unwrap().area_spent(AREA_AI), log_sum);
    let reopened =
        CrawlBudgetLedger::open(BudgetLimits::default(), &[&AiRequestsSpend(f.conn())]).unwrap();
    assert_eq!(reopened.snapshot().unwrap().area_spent(AREA_AI), log_sum);
}

fn arb_rates() -> impl Strategy<Value = Rates> {
    (
        0u64..20_000_000,
        0u64..80_000_000,
        0u64..2_000_000,
        0u64..5_000_000,
        0u64..=1_000_000,
        prop::option::of(1u64..400_000),
        0u64..300_000,
        0u64..5_000_000_000,
        0u64..5_000_000_000,
    )
        .prop_map(|(i, o, cr, cw, b, lc, up, req, img)| Rates {
            input_pico: i,
            output_pico: o,
            cache_read_pico: cr,
            cache_write_pico: cw,
            batch_discount_ppm: b,
            long_context: lc.map(|t| os_ai::cost::LongContext {
                threshold_input_tokens: t,
                input_factor_ppm: 2_000_000,
                output_factor_ppm: 1_500_000,
                cache_read_factor_ppm: 1_000_000,
            }),
            regional_uplift_ppm: up,
            per_request_pico: req,
            per_image_pico: img,
            image_token_estimate: 85,
            source: PriceSource::Table,
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(3_000))]

    // spec: AI-014
    // acceptance: independent
    // `Cost (USD)` = `Token Cost` + `Fixed Fees` + `Regional Uplift` für jede Zeile (Property-Test über
    // Antworten mit/ohne `usage`, Cache, Bilder, Batch, Langkontext, Zuschlag, Wiederholungen).
    #[test]
    fn every_row_cost_is_sum_of_parts(
        rates in arb_rates(),
        input in 0u64..600_000,
        cached in 0u64..600_000,
        write in 0u64..600_000,
        output in 0u64..100_000,
        images in 0u32..6,
        batch in any::<bool>(),
        kind in 0u8..4,
        retries in 0u32..6,
    ) {
        let u = os_ai::provider::Usage {
            input_tokens: Some(input),
            output_tokens: Some(output),
            cached_input_tokens: Some(cached.min(input)),
            cache_write_tokens: Some(write.min(input - cached.min(input))),
            reasoning_tokens: None,
        };
        let mut o = RequestOutcome::success(u, images, 1);
        o.batch = batch;
        o.retries = retries;
        match kind {
            1 => o.usage = UsageReport::Missing { estimated_input_tokens: input, estimated_output_tokens: output },
            2 => o.usage = UsageReport::NoResponse,
            _ => {}
        }
        let rec = if kind == 3 {
            cache_hit_record(&ctx("t", "p", "m", "e"), PriceSource::Table, images)
        } else {
            request_record(&ctx("t", "p", "m", "e"), &rates, MicroUsd(0), &o)
        };
        prop_assert_eq!(
            rec.cost.0,
            rec.token_cost.0 + rec.fixed_fees.0 + rec.regional_uplift.0
        );
        if kind == 3 || kind == 2 {
            prop_assert_eq!(rec.cost, MicroUsd(0));
        }
    }
}

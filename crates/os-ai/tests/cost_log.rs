//! M4-K05: Token- und Kostenprotokoll (AI-014): Protokollzeile je Request aus `os_ai::cost::log`, gespeichert über
//! `os_store::ai::requests`.
#[path = "cost_support.rs"]
mod support;

use os_ai::cost::{
    cache_hit_record, request_record, LogContext, PriceSource, RequestOutcome, UsageReport,
};
use os_ai::provider::Usage;
use os_ai::AiStatus;
use os_model::budget::{BudgetLedger, BudgetScope, CrawlBudgetLedger, MicroUsd, AREA_AI};
use os_store::ai::requests::{
    insert, list, totals_by_task_and_model, AiRequestsSpend, RequestFilter,
};
use proptest::prelude::*;
use support::{crawl_file, mock_input_tokens, openrouter_rates, site_pages, test_rates, TempDir};

fn ctx(task: &str, provider: &str, model: &str, endpoint: &str) -> LogContext {
    LogContext {
        run_id: 1,
        timestamp: "2026-11-24T10:00:00Z".into(),
        task: task.into(),
        provider: provider.into(),
        model: model.into(),
        endpoint: endpoint.into(),
        url_id: Some(1),
        definition_version: Some("page-type@1".into()),
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

// spec: AI-014
#[test]
fn three_mock_claude_answers_sum_to_3000_600_and_0_006() {
    let dir = TempDir::new("sum");
    let f = crawl_file(&dir.0);
    let rates = test_rates("anthropic", "mock-claude", "messages", false);
    let c = ctx("decision", "anthropic", "mock-claude", "messages");
    for _ in 0..3 {
        let rec = request_record(
            &c,
            &rates,
            MicroUsd(5_000),
            &RequestOutcome::success(usage(1_000, 200), 0, 4),
        );
        assert_eq!(rec.price_source, PriceSource::Table);
        insert(f.conn(), &rec).unwrap();
    }
    let totals = totals_by_task_and_model(f.conn()).unwrap();
    assert_eq!(totals.len(), 1);
    assert_eq!(
        (totals[0].input_tokens, totals[0].output_tokens),
        (3_000, 600)
    );
    assert_eq!(totals[0].cost, MicroUsd(6_000));
    assert_eq!(format!("{:.6}", totals[0].cost.0 as f64 / 1e6), "0.006000");
}

// spec: AI-014
#[test]
fn cache_hits_log_a_zero_cost_row_without_tokens() {
    let dir = TempDir::new("cache");
    let f = crawl_file(&dir.0);
    let rec = cache_hit_record(
        &ctx("decision", "openai", "gpt-6-luna", "decisions"),
        PriceSource::Table,
        0,
    );
    insert(f.conn(), &rec).unwrap();
    let rows = list(f.conn(), &RequestFilter::default()).unwrap();
    let r = &rows[0].record;
    assert!(r.cache_hit);
    assert_eq!(
        (r.cost, r.http_status, r.input_tokens),
        (MicroUsd(0), None, None)
    );
    assert_eq!(r.status, "Success (cached)");
}

// spec: AI-014
#[test]
fn retries_are_one_row_and_failed_attempts_without_usage_cost_nothing() {
    let rates = test_rates("openai", "gpt-6-luna", "responses", false);
    let c = ctx("suggest-title", "openai", "gpt-6-luna", "responses");
    // 429x2: zwei Fehlversuche, dann Erfolg → eine Zeile mit Retries = 2, Kosten nur der Erfolg.
    let ok = RequestOutcome {
        retries: 2,
        ..RequestOutcome::success(usage(1_000, 200), 0, 30)
    };
    let rec = request_record(&c, &rates, MicroUsd(1_000), &ok);
    assert_eq!((rec.retries, rec.cost), (2, MicroUsd(200)));
    // Endgültiger Fehler nach 1 + 5 Versuchen ohne usage: Kosten 0.
    let failed = RequestOutcome {
        status: AiStatus::HttpError {
            code: 500,
            message: "Internal Server Error".into(),
        },
        usage: UsageReport::NoResponse,
        http_status: Some(500),
        retries: 5,
        ..RequestOutcome::success(Usage::default(), 0, 90)
    };
    let rec = request_record(&c, &rates, MicroUsd(1_000), &failed);
    assert_eq!(
        (rec.retries, rec.cost, rec.input_tokens),
        (5, MicroUsd(0), None)
    );
    assert!(rec.status.starts_with("Error: 500"));
    assert!(!rec.over_reservation);
}

// spec: AI-014
#[test]
fn missing_usage_is_priced_from_the_estimate_and_keeps_null_tokens() {
    let rates = test_rates("openai", "gpt-6-luna", "decisions", false);
    let outcome = RequestOutcome {
        usage: UsageReport::Missing {
            estimated_input_tokens: 1_000,
            estimated_output_tokens: 0,
        },
        ..RequestOutcome::success(Usage::default(), 0, 9)
    };
    let rec = request_record(
        &ctx("decision", "openai", "gpt-6-luna", "decisions"),
        &rates,
        MicroUsd(200),
        &outcome,
    );
    assert_eq!((rec.input_tokens, rec.output_tokens), (None, None));
    assert_eq!(
        (rec.cost, rec.price_source),
        (MicroUsd(100), PriceSource::Estimate)
    );
}

// spec: AI-014
#[test]
fn openrouter_image_request_has_fixed_fees_0_003() {
    let rates = openrouter_rates("mock/vision");
    let rec = request_record(
        &ctx(
            "suggest-alt",
            "openrouter",
            "mock/vision",
            "chat-completions",
        ),
        &rates,
        MicroUsd(4_000),
        &RequestOutcome::success(usage(90, 10), 1, 11),
    );
    assert_eq!(rec.fixed_fees, MicroUsd(3_000));
    assert_eq!(format!("{:.6}", rec.fixed_fees.0 as f64 / 1e6), "0.003000");
    assert_eq!(rec.price_source, PriceSource::Api);
    assert_eq!(
        rec.cost.0,
        rec.token_cost.0 + rec.fixed_fees.0 + rec.regional_uplift.0
    );
}

// spec: AI-014
#[test]
fn the_log_holds_no_page_content_and_its_sum_matches_the_budget_display() {
    let dir = TempDir::new("content");
    let f = crawl_file(&dir.0);
    let rates = test_rates("openai", "gpt-6-luna", "decisions", false);
    let ledger =
        CrawlBudgetLedger::open(Default::default(), &[&AiRequestsSpend(f.conn())]).unwrap();
    let pages: Vec<_> = site_pages("site")
        .into_iter()
        .filter(|p| p.in_scope)
        .collect();
    for p in &pages {
        let r = ledger
            .reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(10_000))
            .unwrap();
        let chars = p.text.chars().count() as u64;
        let rec = request_record(
            &ctx("decision", "openai", "gpt-6-luna", "decisions"),
            &rates,
            r.maximum,
            &RequestOutcome::success(usage(mock_input_tokens(chars, 0), 0), 0, 2),
        );
        insert(f.conn(), &rec).unwrap();
        ledger.settle(r.id, rec.cost).unwrap();
    }
    // Volltextsuche nach Fixture-Text in allen Textspalten → 0 Treffer.
    for p in &pages {
        let needle = format!("MOCK-ID:{}", p.mock_id);
        assert!(p.text.contains(&needle) || !p.text.contains("MOCK-ID"));
        let hits: i64 = f
            .conn()
            .query_row(
                "SELECT count(*) FROM ai_requests WHERE instr(coalesce(task,'')||coalesce(model,'')||coalesce(definition_version,'')
                   ||coalesce(status,'')||coalesce(error_code,'')||coalesce(error_message,'')||coalesce(refusal_reason,'')
                   ||coalesce(provider_request_id,''), ?1) > 0",
                [&needle],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 0);
    }
    let logged: u64 = totals_by_task_and_model(f.conn())
        .unwrap()
        .iter()
        .map(|t| t.cost.0)
        .sum();
    let snap = ledger.snapshot().unwrap();
    assert_eq!(
        snap.area_spent(AREA_AI),
        MicroUsd(logged),
        "Summe Protokoll = KI-Anteil der Budgetanzeige"
    );
}

fn outcome_strategy() -> impl Strategy<Value = RequestOutcome> {
    let usage = prop_oneof![
        (
            proptest::option::of(0u64..400_000),
            proptest::option::of(0u64..50_000),
            proptest::option::of(0u64..100_000)
        )
            .prop_map(|(i, o, c)| UsageReport::Reported(Usage {
                input_tokens: i,
                output_tokens: o,
                cached_input_tokens: c,
                cache_write_tokens: None
            })),
        (0u64..400_000, 0u64..50_000).prop_map(|(i, o)| UsageReport::Missing {
            estimated_input_tokens: i,
            estimated_output_tokens: o
        }),
        Just(UsageReport::NoResponse),
    ];
    let status = prop_oneof![
        Just(AiStatus::Success),
        Just(AiStatus::Refusal),
        Just(AiStatus::QuotaExceeded),
        Just(AiStatus::InvalidOutput),
        Just(AiStatus::Timeout { seconds: 30 }),
        (400u16..600).prop_map(|code| AiStatus::HttpError {
            code,
            message: "x".into()
        }),
    ];
    (usage, status, 0u32..4, any::<bool>(), 0u32..6).prop_map(
        |(usage, status, images, batch, retries)| RequestOutcome {
            status,
            usage,
            images,
            batch,
            retries,
            ..RequestOutcome::success(Usage::default(), 0, 1)
        },
    )
}

proptest! {
    // spec: AI-014
    /// `Cost (USD) = Token Cost + Fixed Fees + Regional Uplift` für jede Zeile über alle Antwortarten (mit/ohne
    /// usage, ohne Antwort, Statuswerte) und alle Testpreise einschließlich EU-Zuschlag und OpenRouter-Gebühren.
    #[test]
    fn every_log_row_cost_is_the_sum_of_its_parts(outcome in outcome_strategy(), which in 0usize..5) {
        let rates = match which {
            0 => test_rates("anthropic", "mock-claude", "messages", false),
            1 => test_rates("openai", "gpt-6-luna", "decisions", true),
            2 => test_rates("openai", "gpt-6-luna", "responses", true),
            3 => openrouter_rates("mock/vision"),
            _ => openrouter_rates("mock/text"),
        };
        let rec = request_record(&ctx("decision", "p", "m", "e"), &rates, MicroUsd(1_000_000), &outcome);
        prop_assert_eq!(rec.cost.0, rec.token_cost.0 + rec.fixed_fees.0 + rec.regional_uplift.0);
        let hit = cache_hit_record(&ctx("decision", "p", "m", "e"), rates.source, outcome.images);
        prop_assert_eq!(hit.cost.0, 0);
    }
}

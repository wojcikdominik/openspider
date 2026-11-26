//! Unabhängige Abnahmetests M4-K10: Token- und Kostenprotokoll nach dem Nachtrag 24.11.2026, Q00 (AI-014) und die
//! Verbuchung der OpenRouter-Ist-Kosten (AI-005, Nachtrag Q00-M3/M4).
//!
//! Geprüft: Spalten `Reasoning Level`, `Reasoning Tokens`, `Computed Cost (USD)`, `Price Source = reported`,
//! `cost_mismatch` an und knapp neben der Schwelle (mehr als 1 % **und** mehr als 1 µUSD), Aufteilung gemeldeter Kosten
//! (`Fixed Fees`, `Token Cost`), `no-cost`/`byok` → `api`, Summenzeile mit Reasoning-Tokens, Mengen der Spaltenwerte über
//! Mock-Szenarien aller Anbieter. Die Zeilen werden mit `cost::log` gebildet und über `os_store::ai::requests` gespeichert
//! und zurückgelesen.
// acceptance: independent

mod acceptance_k10_support;

use acceptance_k10_support::*;
use os_ai::adapters::openai_decisions::{decisions_billing_rates, DecisionInput, Question};
use os_ai::adapters::openrouter::ReportedCost;
use os_ai::cost::{
    cost_mismatch, cost_of, detailed_request_record, reported_breakdown, FeeCounts, LoggedRow,
    OutcomeDetails, PriceBasis, PriceSource, Rates, RequestOutcome, TokenCounts,
};
use os_ai::provider::{Input, ReasoningLevel, Usage};
use os_ai::AiStatus;
use os_model::budget::MicroUsd;
use os_store::ai::requests::{self as store, RequestFilter, REASONING_LEVELS};

fn insert(file: &os_store::CrawlFile, row: &LoggedRow) {
    store::insert_with_details(file.conn(), &row.record, &row.details).expect("Zeile gespeichert");
}

#[allow(clippy::too_many_arguments)]
fn row(
    task: &str,
    provider: &str,
    model: &str,
    endpoint: &str,
    rates: &Rates,
    outcome: RequestOutcome,
    reasoning: Option<ReasoningLevel>,
    reported_cost: Option<ReportedCost>,
) -> LoggedRow {
    detailed_request_record(
        &ctx(task, provider, model, endpoint),
        rates,
        MicroUsd(5_000_000),
        &outcome,
        &OutcomeDetails {
            reasoning,
            reported_cost,
        },
    )
}

fn reported(micro: u64) -> ReportedCost {
    ReportedCost {
        cost_micro_usd: Some(micro),
        upstream_inference_cost_micro_usd: Some(micro),
        is_byok: Some(false),
    }
}

// spec: AI-014 [M4]
// acceptance: independent
// Kriterium: 3 Mock-Antworten auf `mock-claude` (1/5 USD je 1 Mio.) mit je 1.000 Eingabe- und 200 Ausgabe-Tokens, davon
// 120 Reasoning → Summenzeile `Input Tokens 3000`, `Output Tokens 600`, `Reasoning Tokens 360`, `Cost 0.006000`
// (Reasoning nicht doppelt).
#[test]
fn sum_row_shows_reasoning_tokens_without_double_cost() {
    let dir = TempDir::new("sum");
    let file = crawl_file(&dir.0);
    for _ in 0..3 {
        let r = row(
            "suggest-title",
            "anthropic",
            "mock-claude",
            "messages",
            &claude_rates(),
            RequestOutcome::success(usage(1_000, 200, Some(120)), 0, 10),
            Some(ReasoningLevel::Low),
            None,
        );
        assert_eq!(r.record.cost, MicroUsd(2_000));
        assert_eq!(r.details.reasoning_tokens, Some(120));
        insert(&file, &r);
    }
    let totals = store::totals_by_task_and_model(file.conn()).unwrap();
    assert_eq!(totals.len(), 1);
    let t = &totals[0];
    assert_eq!(t.input_tokens, 3_000);
    assert_eq!(t.output_tokens, 600);
    assert_eq!(t.reasoning_tokens, 360);
    assert_eq!(t.cost, MicroUsd(6_000));
    assert_eq!(t.computed_cost, MicroUsd(6_000));
}

// spec: AI-014 [M4]
// spec: AI-005 [M4]
// acceptance: independent
// Kriterium (AI-005): Standardantwort → `Cost (USD)` = `usage.cost` auf µUSD gerundet, `Price Source = reported`,
// `Computed Cost (USD)` = lokale Rechnung, `cost_mismatch = false`; Szenario `cost-mismatch` → `Cost (USD)` = gemeldeter
// (doppelter) Betrag, `cost_mismatch = true`, der Budgetverbrauch enthält den gemeldeten Betrag; Szenarien `no-cost` und
// `byok` → `Price Source = api`, `Cost (USD)` = lokale Rechnung. Spalte `Computed Cost` gleich `Cost` bei Quelle ≠
// `reported`.
#[tokio::test]
async fn openrouter_reported_costs_are_booked() {
    let mock = Mock::start().await;
    let roles = openrouter_roles(&mock).await;
    let adapter = mock.openrouter();
    let m = &roles.r3;
    let dir = TempDir::new("reported");
    let file = crawl_file(&dir.0);

    let mut expected_total = 0u64;
    for (scenario, source, mismatch, factor) in [
        ("standard", PriceSource::Reported, false, 1u64),
        (
            "MOCK-SCENARIO:cost-mismatch",
            PriceSource::Reported,
            true,
            2,
        ),
        ("MOCK-SCENARIO:no-cost", PriceSource::Api, false, 1),
        ("MOCK-SCENARIO:byok", PriceSource::Api, false, 1),
    ] {
        let reply = adapter
            .send(
                &request(
                    model("openrouter", &m.id, "chat-completions", Vec::new()),
                    &format!("K10 log {scenario}"),
                    None,
                    ReasoningLevel::ModelDefault,
                    Some(256),
                ),
                None,
            )
            .await
            .expect("Mock antwortet");
        let input = reply.usage.input_tokens.expect("Eingabe-Tokens");
        let rates = PriceBasis::OpenRouter(m).actual_rates(input).unwrap();
        let local = cost_of(
            &TokenCounts::from_usage(&reply.usage),
            FeeCounts::one_request(0),
            &rates,
            false,
        )
        .cost;
        let r = row(
            "suggest-title",
            "openrouter",
            &m.id,
            "chat-completions",
            &rates,
            RequestOutcome::success(reply.usage.clone(), 0, 5),
            Some(ReasoningLevel::ModelDefault),
            reply.reported_cost,
        );
        assert_eq!(r.record.price_source, source, "{scenario}");
        assert_eq!(r.record.cost, MicroUsd(local.0 * factor), "{scenario}");
        assert_eq!(
            r.details.computed_cost_of(&r.record),
            local,
            "Computed Cost {scenario}"
        );
        assert_eq!(r.details.cost_mismatch, mismatch, "{scenario}");
        assert_eq!(
            r.record.cost,
            MicroUsd(r.record.token_cost.0 + r.record.fixed_fees.0 + r.record.regional_uplift.0)
        );
        expected_total += r.record.cost.0;
        insert(&file, &r);
    }
    let rows = store::list(file.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(rows.len(), 4);
    let sources: Vec<&str> = rows
        .iter()
        .map(|r| r.record.price_source.as_str())
        .collect();
    assert_eq!(sources, ["reported", "reported", "api", "api"]);
    assert!(rows[1].details.cost_mismatch);
    assert_ne!(
        rows[1].details.computed_cost_of(&rows[1].record),
        rows[1].record.cost
    );
    for r in &rows[2..] {
        assert_eq!(r.details.computed_cost_of(&r.record), r.record.cost);
    }
    assert_eq!(
        store::total_cost(file.conn()).unwrap(),
        MicroUsd(expected_total),
        "Budgetverbrauch enthält die gemeldeten Beträge"
    );
}

// spec: AI-005 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Schwelle `cost_mismatch`: `Cost` weicht um mehr als 1 % **und** mehr als 1 µUSD von `Computed Cost` ab – geprüft an
// und knapp neben beiden Grenzen, auch über die Protokollzeile.
#[test]
fn cost_mismatch_threshold() {
    // 1 %-Grenze bei 1.000 µUSD: genau 1 % (10 µUSD) ist keine Abweichung, 11 µUSD schon – in beide Richtungen.
    assert!(!cost_mismatch(MicroUsd(1_010), MicroUsd(1_000)));
    assert!(cost_mismatch(MicroUsd(1_011), MicroUsd(1_000)));
    assert!(!cost_mismatch(MicroUsd(990), MicroUsd(1_000)));
    assert!(cost_mismatch(MicroUsd(989), MicroUsd(1_000)));
    // 1-µUSD-Grenze bei kleinen Beträgen: 1 µUSD (2 %) ist keine Abweichung, 2 µUSD schon.
    assert!(!cost_mismatch(MicroUsd(51), MicroUsd(50)));
    assert!(cost_mismatch(MicroUsd(52), MicroUsd(50)));
    assert!(!cost_mismatch(MicroUsd(1), MicroUsd(0)));
    assert!(cost_mismatch(MicroUsd(2), MicroUsd(0)));
    assert!(!cost_mismatch(MicroUsd(500), MicroUsd(500)));

    // Über die Zeile: lokale Rechnung 1.000 µUSD (1.000 Eingabe-Tokens zu 1 USD je 1 Mio.).
    let rates = claude_rates();
    for (reported_micro, mismatch) in [(1_010u64, false), (1_011, true), (990, false), (989, true)]
    {
        let r = row(
            "suggest-title",
            "openrouter",
            "m",
            "chat-completions",
            &rates,
            RequestOutcome::success(usage(1_000, 0, None), 0, 1),
            None,
            Some(reported(reported_micro)),
        );
        assert_eq!(r.details.computed_cost, Some(MicroUsd(1_000)));
        assert_eq!(r.record.cost, MicroUsd(reported_micro));
        assert_eq!(r.details.cost_mismatch, mismatch, "{reported_micro}");
    }
}

// spec: AI-005 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Kriterium: Bildrequest über das Mock-Modell mit `pricing.request = "0.001"` und `pricing.image = "0.002"` mit gemeldetem
// `usage.cost` → `Fixed Fees = 0.003000`, `Token Cost = Cost − 0.003000`, `Cost = Token Cost + Fixed Fees + Regional
// Uplift`. Liegt der gemeldete Betrag unter den Gebühren, gilt `Fixed Fees = Cost`, `Token Cost = 0`.
#[tokio::test]
async fn reported_cost_split_with_fees() {
    let mock = Mock::start().await;
    let roles = openrouter_roles(&mock).await;
    let v = &roles.vision;
    let mut r = request(
        model("openrouter", &v.id, "chat-completions", Vec::new()),
        "K10 log image",
        None,
        ReasoningLevel::ModelDefault,
        Some(256),
    );
    r.input.push(Input::Image {
        media_type: "image/png".into(),
        base64: PNG_1X1.into(),
    });
    r.required.vision = true;
    let reply = mock
        .openrouter()
        .send(&r, None)
        .await
        .expect("Mock antwortet");
    let cost = reply
        .reported_cost
        .and_then(|c| c.usable())
        .expect("usage.cost");
    let rates = PriceBasis::OpenRouter(v)
        .actual_rates(reply.usage.input_tokens.unwrap_or(0))
        .unwrap();
    let logged = row(
        "suggest-alt",
        "openrouter",
        &v.id,
        "chat-completions",
        &rates,
        RequestOutcome::success(reply.usage.clone(), 1, 5),
        Some(ReasoningLevel::ModelDefault),
        reply.reported_cost,
    );
    assert_eq!(logged.record.price_source, PriceSource::Reported);
    assert_eq!(logged.record.cost, MicroUsd(cost));
    assert_eq!(logged.record.fixed_fees, MicroUsd(3_000));
    assert_eq!(logged.record.token_cost, MicroUsd(cost - 3_000));
    assert_eq!(logged.record.regional_uplift, MicroUsd(0));
    assert_eq!(
        logged.record.cost.0,
        logged.record.token_cost.0 + logged.record.fixed_fees.0 + logged.record.regional_uplift.0
    );

    let small = reported_breakdown(MicroUsd(2_000), MicroUsd(3_000));
    assert_eq!(
        (
            small.fixed_fees,
            small.token_cost,
            small.regional_uplift,
            small.cost
        ),
        (MicroUsd(2_000), MicroUsd(0), MicroUsd(0), MicroUsd(2_000))
    );
}

// spec: AI-005 [M4]
// acceptance: independent
// Kriterium: Langkontext ohne gemeldete Kosten (Szenario `no-cost`) auf `M-LC`: 1.000 Eingabe- und 100 Ausgabe-Tokens →
// 1.000 × 2 + 100 × 4 = 2.400 µUSD; 999 Eingabe- und 100 Ausgabe-Tokens → 999 × 1 + 100 × 2 = 1.199 µUSD;
// `Long Context From 1000`.
#[tokio::test]
async fn long_context_without_reported_cost() {
    let mock = Mock::start().await;
    let roles = openrouter_roles(&mock).await;
    let lc = &roles.lc;
    assert_eq!(lc.long_context_from(), Some(1_000));
    let fee = lc.request_pico / 1_000_000;
    for (input, expected) in [(1_000u64, 2_400u64), (999, 1_199)] {
        let rates = PriceBasis::OpenRouter(lc).actual_rates(input).unwrap();
        let r = row(
            "suggest-title",
            "openrouter",
            &lc.id,
            "chat-completions",
            &rates,
            RequestOutcome::success(usage(input, 100, None), 0, 1),
            Some(ReasoningLevel::ModelDefault),
            None,
        );
        assert_eq!(r.record.cost, MicroUsd(expected + fee), "{input}");
        assert_eq!(r.record.price_source, PriceSource::Api);
    }

    // Über den Mock: `no-cost` liefert keinen verwertbaren Betrag → lokale Rechnung, Quelle `api`.
    let reply = mock
        .openrouter()
        .send(
            &request(
                model("openrouter", &lc.id, "chat-completions", Vec::new()),
                "K10 lc MOCK-SCENARIO:no-cost",
                None,
                ReasoningLevel::ModelDefault,
                Some(256),
            ),
            None,
        )
        .await
        .unwrap();
    let input = reply.usage.input_tokens.unwrap();
    let rates = PriceBasis::OpenRouter(lc).actual_rates(input).unwrap();
    let r = row(
        "suggest-title",
        "openrouter",
        &lc.id,
        "chat-completions",
        &rates,
        RequestOutcome::success(reply.usage.clone(), 0, 1),
        Some(ReasoningLevel::ModelDefault),
        reply.reported_cost,
    );
    assert_eq!(r.record.price_source, PriceSource::Api);
    assert_eq!(r.details.computed_cost_of(&r.record), r.record.cost);
}

// spec: AI-014 [M4]
// acceptance: independent
// Spalte `Reasoning Level`: effektiv gesendete Stufe, `model-default`, leer bei Decisions und Embeddings; `Reasoning
// Tokens` leer, wenn der Anbieter nichts meldet, sonst der gemeldete Wert (auch 0).
#[test]
fn reasoning_columns_per_path() {
    let dir = TempDir::new("levels");
    let file = crawl_file(&dir.0);
    let rates = claude_rates();
    let cases: [(&str, Option<ReasoningLevel>, Option<u64>); 5] = [
        ("decision", None, None),
        ("embedding", None, None),
        ("suggest-title", Some(ReasoningLevel::Off), Some(0)),
        ("ask-sql", Some(ReasoningLevel::Medium), Some(40)),
        ("prompt", Some(ReasoningLevel::ModelDefault), None),
    ];
    for (task, level, reasoning_tokens) in cases {
        let r = row(
            task,
            "anthropic",
            "mock-claude",
            "messages",
            &rates,
            RequestOutcome::success(usage(100, 50, reasoning_tokens), 0, 1),
            level,
            None,
        );
        insert(&file, &r);
    }
    let rows = store::list(file.conn(), &RequestFilter::default()).unwrap();
    let got: Vec<(Option<String>, Option<u64>)> = rows
        .iter()
        .map(|r| {
            (
                r.details.reasoning_level.clone(),
                r.details.reasoning_tokens,
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (None, None),
            (None, None),
            (Some("off".into()), Some(0)),
            (Some("medium".into()), Some(40)),
            (Some("model-default".into()), None),
        ]
    );
}

// spec: AI-014 [M4]
// acceptance: independent
// Kriterium: Jede Zeile hat `Price Source` aus {`api`, `table`, `user`, `estimate`, `reported`} und `Reasoning Level`
// aus der Stufenmenge von AI-011 oder leer (Property-Test über Mock-Szenarien aller Anbieter); die Invariante
// `Cost = Token Cost + Fixed Fees + Regional Uplift` gilt auch für `reported`.
#[tokio::test]
async fn every_row_has_known_price_source_and_level() {
    let mock = Mock::start().await;
    let a = anthropic_model(&mock, "A").await;
    let roles = openrouter_roles(&mock).await;
    let dir = TempDir::new("property");
    let file = crawl_file(&dir.0);
    let anthropic = mock.anthropic();
    let openrouter = mock.openrouter();
    let responses = mock.responses();
    let ollama = mock.ollama();
    let mut rows = Vec::new();

    for (scenario, level) in [
        ("plain", ReasoningLevel::Off),
        ("MOCK-SCENARIO:reasoning=40", ReasoningLevel::Medium),
        ("MOCK-SCENARIO:output-limit", ReasoningLevel::Low),
        ("MOCK-SCENARIO:refusal", ReasoningLevel::ModelDefault),
    ] {
        let reply = anthropic
            .send(
                &request(
                    model("anthropic", &a.id, "messages", Vec::new()),
                    &format!("K10 prop {scenario}"),
                    None,
                    level,
                    Some(512),
                ),
                None,
            )
            .await
            .unwrap();
        let status = if reply.is_output_limit() {
            AiStatus::OutputLimitReached
        } else if reply.is_refusal() {
            AiStatus::Refusal
        } else {
            AiStatus::Success
        };
        let mut outcome = RequestOutcome::success(reply.usage.clone(), 0, reply.duration_ms);
        outcome.status = status;
        rows.push(row(
            "suggest-title",
            "anthropic",
            &a.id,
            "messages",
            &claude_rates(),
            outcome,
            Some(level),
            None,
        ));
    }
    for scenario in [
        "plain",
        "MOCK-SCENARIO:cost-mismatch",
        "MOCK-SCENARIO:no-cost",
        "MOCK-SCENARIO:byok",
        "MOCK-SCENARIO:output-limit",
    ] {
        let reply = openrouter
            .send(
                &request(
                    model("openrouter", &roles.r1.id, "chat-completions", Vec::new()),
                    &format!("K10 prop or {scenario}"),
                    None,
                    ReasoningLevel::Off,
                    Some(512),
                ),
                None,
            )
            .await
            .unwrap();
        let rates = PriceBasis::OpenRouter(&roles.r1)
            .actual_rates(reply.usage.input_tokens.unwrap_or(0))
            .unwrap();
        let mut outcome = RequestOutcome::success(reply.usage.clone(), 0, reply.duration_ms);
        if reply.is_output_limit() {
            outcome.status = AiStatus::OutputLimitReached;
        }
        rows.push(row(
            "suggest-title",
            "openrouter",
            &roles.r1.id,
            "chat-completions",
            &rates,
            outcome,
            Some(ReasoningLevel::Off),
            reply.reported_cost,
        ));
    }
    for scenario in [
        "plain",
        "MOCK-SCENARIO:output-limit",
        "MOCK-SCENARIO:reasoning=27",
    ] {
        let reply = responses
            .send(
                &request(
                    model("openai", "gpt-6-luna", "responses", luna_levels()),
                    &format!("K10 prop resp {scenario}"),
                    None,
                    ReasoningLevel::High,
                    Some(512),
                ),
                None,
                None,
            )
            .await
            .unwrap();
        let mut outcome = RequestOutcome::success(reply.usage.clone().unwrap_or_default(), 0, 1);
        if reply.output_limit().is_some() {
            outcome.status = AiStatus::OutputLimitReached;
        }
        rows.push(row(
            "suggest-title",
            "openai",
            "gpt-6-luna",
            "responses",
            &fixture_rates("openai", "gpt-6-luna", "responses"),
            outcome,
            Some(ReasoningLevel::High),
            None,
        ));
    }
    for scenario in ["plain", "MOCK-SCENARIO:output-limit"] {
        let reply = ollama
            .send(
                &request(
                    model(
                        "ollama",
                        "mock-llama",
                        os_ai::adapters::ollama::ENDPOINT,
                        Vec::new(),
                    ),
                    &format!("K10 prop ollama {scenario}"),
                    None,
                    ReasoningLevel::ModelDefault,
                    Some(128),
                ),
                None,
            )
            .await
            .unwrap();
        rows.push(row(
            "suggest-title",
            "ollama",
            "mock-llama",
            os_ai::adapters::ollama::ENDPOINT,
            &Rates::free(PriceSource::Table),
            RequestOutcome::success(reply.usage.clone(), 0, 1),
            Some(ReasoningLevel::ModelDefault),
            None,
        ));
    }
    let reply = mock
        .decisions()
        .send(
            "gpt-6-luna",
            &DecisionInput::text("K10 prop decisions"),
            &[Question::predicate("is_product", "Product?")],
            None,
        )
        .await
        .unwrap();
    rows.push(row(
        "decision",
        "openai",
        "gpt-6-luna",
        "decisions",
        &decisions_billing_rates(&fixture_rates("openai", "gpt-6-luna", "decisions")),
        RequestOutcome::success(reply.usage.unwrap_or_default(), 0, 1),
        None,
        None,
    ));
    // Ohne `usage` (Decisions): Schätzung, Quelle `estimate`.
    let mut missing = RequestOutcome::success(Usage::default(), 0, 1);
    missing.usage = os_ai::cost::UsageReport::Missing {
        estimated_input_tokens: 500,
        estimated_output_tokens: 0,
    };
    rows.push(row(
        "decision",
        "openai",
        "gpt-6-luna",
        "decisions",
        &fixture_rates("openai", "gpt-6-luna", "decisions"),
        missing,
        None,
        None,
    ));

    let sources = ["api", "table", "user", "estimate", "reported"];
    for r in &rows {
        insert(&file, r);
    }
    let stored = store::list(file.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(stored.len(), rows.len());
    let mut seen = std::collections::BTreeSet::new();
    for r in &stored {
        assert!(
            sources.contains(&r.record.price_source.as_str()),
            "{:?}",
            r.record
        );
        seen.insert(r.record.price_source.as_str());
        if let Some(level) = &r.details.reasoning_level {
            assert!(REASONING_LEVELS.contains(&level.as_str()), "{level}");
            assert!(level.parse::<ReasoningLevel>().is_ok());
        }
        assert_eq!(
            r.record.cost.0,
            r.record.token_cost.0 + r.record.fixed_fees.0 + r.record.regional_uplift.0,
            "{:?}",
            r.record
        );
        if r.record.price_source != PriceSource::Reported {
            assert_eq!(r.details.computed_cost_of(&r.record), r.record.cost);
            assert!(!r.details.cost_mismatch);
        }
    }
    for s in ["api", "table", "estimate", "reported"] {
        assert!(
            seen.contains(s),
            "Quelle {s} kam in den Szenarien vor: {seen:?}"
        );
    }
    assert!(stored
        .iter()
        .any(|r| r.record.status == "Error: Output limit reached"));
}

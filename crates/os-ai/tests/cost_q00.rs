//! M4-K10: Kosten-Nachträge 24.11.2026, Q00 – Schätzung (AI-012), Reservierung (AI-013), Protokoll und Ist-Kosten
//! `reported` (AI-014, AI-005), Request-ID im Fehlerweg (AI-001) – gegen ai-mock und die Crawl-Datei. Der Ledger läuft
//! nur über `os_store::ai::budget`.
#[path = "cost_q00_support.rs"]
mod support;

use os_ai::adapters::anthropic::{self, image_input};
use os_ai::adapters::openai_decisions::{DecideOptions, DecisionInput, OpenAiDecisions, Question};
use os_ai::adapters::openrouter::OpenRouterModel;
use os_ai::catalog::{default_max_tokens, AnswerTokens, Endpoint, ProviderLimitsFile};
use os_ai::cost::{
    cost_mismatch, cost_of, decisions_input_tokens, detailed_request_record, estimate_requests,
    expected_reasoning_tokens, output_calibration_from_log, question_text_chars,
    reported_breakdown, reservation_for, Calibration, EstimatePath, FeeCounts, NoCache,
    OutcomeDetails, PlannedEntry, PlannedRequest, PriceBasis, PriceSource, Rates,
    RequestEstimateInput, RequestOutcome, TokenCounts, UsageReport,
};
use os_ai::limits::AttemptFailure;
use os_ai::provider::{resolve_reasoning_level, ProviderError, ReasoningLevel, Usage};
use os_ai::AiStatus;
use os_model::budget::{BudgetLedger, BudgetLimits, BudgetScope, MicroUsd, AREA_AI};
use os_store::ai::budget::open_ledger;
use os_store::ai::requests::{
    insert_with_details, list, total_cost, totals_by_task_and_model, RequestFilter,
};
use proptest::prelude::*;
use reqwest::header::HeaderMap;
use serde_json::json;
use support::{
    crawl_file, limiter, log_ctx, model, ok_schema, request, Mock, TempDir, CHAT, DECISIONS,
    MESSAGES,
};

fn claude_rates() -> Rates {
    Rates::per_million("1", "5", PriceSource::Table).unwrap()
}

fn decisions_rates() -> Rates {
    Rates::per_million("0.1", "0", PriceSource::Table).unwrap()
}

fn usage(input: u64, output: u64, reasoning: Option<u64>) -> Usage {
    Usage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: reasoning,
    }
}

fn prompt_path(answer: u64, level: ReasoningLevel, schema_overhead: u64) -> EstimatePath {
    EstimatePath::Prompt {
        instruction_tokens: 0,
        schema_overhead_tokens: schema_overhead,
        answer: AnswerTokens::Fixed(answer),
        reasoning: level,
    }
}

fn estimate_input<'a>(
    requests: &'a [PlannedRequest],
    path: EstimatePath,
    rates: &'a Rates,
) -> RequestEstimateInput<'a> {
    RequestEstimateInput {
        task: "suggest-title".into(),
        model: "mock-claude".into(),
        requests,
        path,
        basis: Some(PriceBasis::Rates(rates)),
        api_key_present: true,
        calibration: Calibration::UNCALIBRATED,
        output_calibration: Calibration::UNCALIBRATED,
        batch_capable: false,
    }
}

fn entry(key: &str, question_chars: u64) -> PlannedEntry {
    PlannedEntry {
        cache_key: Some(key.into()),
        question_chars,
        weight: 0,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// AI-012 Kostenschätzung

/// Frage mit genau `chars` Zeichen Fragentext (Name + Anweisung).
fn question_with_chars(name: &str, chars: usize) -> Question {
    let instructions: String = "Is this page about products? "
        .chars()
        .cycle()
        .take(chars - name.chars().count())
        .collect();
    Question::predicate(name, instructions)
}

// spec: AI-012 [M4]
// Decisions-Prüfwert: 1 Request mit Eingabetext von 350 Zeichen, 2 Fragen mit Fragentext 140 bzw. 210 Zeichen, kein
// Bild, unkalibriert → Est. Input Tokens = (100 + 40 + 100) + (100 + 60 + 100) = 500, erwartete Kosten 50 µUSD; die
// Mock-Ist-Tokens desselben Requests sind (88 + 35 + 100) + (88 + 53 + 100) = 464 (Abweichung +7,8 %).
#[tokio::test]
async fn decisions_estimate_counts_input_per_question() {
    let g = ProviderLimitsFile::bundled()
        .unwrap()
        .decisions_question_overhead();
    assert_eq!(g, 100, "G aus provider-limits.json");
    let q1 = question_with_chars("q1", 140);
    let q2 = question_with_chars("q2", 210);
    for (q, chars) in [(&q1, 140), (&q2, 210)] {
        assert_eq!(q.text_chars(), chars);
        assert_eq!(
            question_text_chars(&serde_json::to_value(q).unwrap()),
            chars
        );
    }
    assert_eq!(decisions_input_tokens(350, 0, 85, &[140, 210], g), 500);
    let rates = decisions_rates();
    let requests = [PlannedRequest {
        input_chars: 350,
        images: 0,
        entries: vec![entry("a", 140), entry("b", 210)],
    }];
    let e = estimate_requests(
        &RequestEstimateInput {
            task: "decision".into(),
            model: "gpt-6-luna".into(),
            ..estimate_input(
                &requests,
                EstimatePath::DecisionsApi {
                    question_overhead_tokens: g,
                },
                &rates,
            )
        },
        &NoCache,
    );
    assert_eq!(e.estimate.est_input_tokens, 500);
    assert_eq!(e.estimate.est_output_tokens, 0, "Decisions → 0");
    assert_eq!(e.estimate.expected, Some(MicroUsd(50)));
    assert_eq!(e.reasoning, None, "Decisions ohne Stufe");
    assert_eq!(e.requests, 1);

    let mock = Mock::start().await;
    let text: String = "Product page with a short description. "
        .chars()
        .cycle()
        .take(350)
        .collect();
    let run = OpenAiDecisions::new(mock.openai_http())
        .decide(
            &limiter(),
            &DecisionInput::text(text),
            &[q1, q2],
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(mock.requests_to(DECISIONS).len(), 1);
    let actual = run.requests[0]
        .meta
        .usage
        .as_ref()
        .and_then(|u| u.input_tokens)
        .unwrap();
    assert_eq!(actual, 464);
    let deviation = (500.0 - actual as f64) / actual as f64;
    assert!((0.07..0.08).contains(&deviation), "{deviation}");
}

// spec: AI-012 [M4]
// 5 Fragen in einem Request ergeben dieselbe Schätzung wie die Summe der 5 Einzelschätzungen (Bündelung ändert die
// Tokenschätzung nicht); die Requestzahl im Voranschlag sinkt.
#[test]
fn bundling_changes_requests_not_tokens() {
    let rates = decisions_rates();
    let path = EstimatePath::DecisionsApi {
        question_overhead_tokens: 100,
    };
    let entries: Vec<PlannedEntry> = (0..5)
        .map(|i| entry(&format!("q{i}"), 60 + 30 * i))
        .collect();
    let bundled = [PlannedRequest {
        input_chars: 1_200,
        images: 1,
        entries: entries.clone(),
    }];
    let single: Vec<PlannedRequest> = entries
        .into_iter()
        .map(|e| PlannedRequest {
            input_chars: 1_200,
            images: 1,
            entries: vec![e],
        })
        .collect();
    let a = estimate_requests(&estimate_input(&bundled, path, &rates), &NoCache);
    let b = estimate_requests(&estimate_input(&single, path, &rates), &NoCache);
    assert_eq!(a.estimate.est_input_tokens, b.estimate.est_input_tokens);
    // Kosten: gleich bis auf die Rundung auf ganze µUSD je Request (höchstens 1 µUSD je Einzelrequest).
    let (x, y) = (
        a.estimate.expected.unwrap().0,
        b.estimate.expected.unwrap().0,
    );
    assert!(x.abs_diff(y) <= 5, "{x} / {y}");
    assert_eq!(a.estimate.items, 5);
    assert_eq!((a.requests, b.requests), (1, 5));
    // Ein gecachter Eintrag fehlt in der Schätzung; sind alle gecacht, wird der Request nicht gesendet.
    let cache: std::collections::HashSet<String> = ["q0", "q1", "q2", "q3", "q4"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let c = estimate_requests(&estimate_input(&bundled, path, &rates), &cache);
    assert_eq!(
        (c.requests, c.estimate.cached, c.estimate.est_input_tokens),
        (0, 5, 0)
    );
}

// spec: AI-012 [M4]
// Ausgabe-Prüfwert unkalibriert: `suggest-title` mit effektiver Stufe `off` (sicher aus) → Est. Output Tokens = 512 je
// Request; mit `medium` → 1.024; Decisions → 0. Reasoning-Tokens gehen zum Ausgabepreis ein.
#[test]
fn output_estimate_adds_expected_reasoning() {
    let rates = claude_rates();
    let requests = [PlannedRequest {
        input_chars: 350,
        images: 0,
        entries: vec![entry("t", 0)],
    }];
    let out = |level| {
        estimate_requests(
            &estimate_input(&requests, prompt_path(512, level, 0), &rates),
            &NoCache,
        )
    };
    let off = out(ReasoningLevel::Off);
    let medium = out(ReasoningLevel::Medium);
    assert_eq!(off.estimate.est_output_tokens, 512);
    assert_eq!(medium.estimate.est_output_tokens, 1_024);
    assert_eq!(off.reasoning, Some(ReasoningLevel::Off));
    // 100 Eingabe-Tokens × 1 + 1.024 × 5 µUSD.
    assert_eq!(medium.estimate.expected, Some(MicroUsd(100 + 5_120)));
    let table: Vec<u64> = [
        ReasoningLevel::Off,
        ReasoningLevel::Minimal,
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
        ReasoningLevel::XHigh,
        ReasoningLevel::Max,
        ReasoningLevel::ModelDefault,
    ]
    .into_iter()
    .map(expected_reasoning_tokens)
    .collect();
    assert_eq!(table, [0, 64, 128, 512, 2_048, 4_096, 8_192, 512]);
    // Ersatzweg einer Entscheidungsfrage: A = 64 + 16 × W über die nicht gecachten Fragen, Stufe `off`.
    let fallback = [PlannedRequest {
        input_chars: 0,
        images: 0,
        entries: vec![
            PlannedEntry {
                cache_key: None,
                question_chars: 0,
                weight: 14,
            },
            PlannedEntry {
                cache_key: None,
                question_chars: 0,
                weight: 2,
            },
        ],
    }];
    let path = EstimatePath::Prompt {
        instruction_tokens: 0,
        schema_overhead_tokens: 0,
        answer: AnswerTokens::Formula(os_ai::catalog::reasoning::AnswerFormula::DecisionW),
        reasoning: ReasoningLevel::Off,
    };
    let e = estimate_requests(&estimate_input(&fallback, path, &rates), &NoCache);
    assert_eq!(e.estimate.est_output_tokens, 64 + 16 * 16);
}

// spec: AI-012 [M4]
// Nach 20 Requests einer Aufgabe mit Mock-Szenario `reasoning=500` bei Stufe `medium` ist der Ausgabe-Korrekturfaktor
// dieser (Modell, Aufgabe, Stufe) gesetzt, und die Schätzung für den nächsten Request liegt höchstens ±5 % neben dem
// Mock-Ist-Wert; der Faktor einer anderen Stufe bleibt unberührt.
#[tokio::test]
async fn output_factor_is_learned_per_model_task_and_level() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let models = adapter.list_models().await.unwrap();
    let a = models
        .iter()
        .find(|m| m.id == "mock-claude")
        .unwrap()
        .to_model();
    let dir = TempDir::new("output-factor");
    let file = crawl_file(&dir.0);
    let ctx = log_ctx("suggest-title", "anthropic", "mock-claude", "messages");
    let raw = os_ai::cost::raw_output_tokens(512, ReasoningLevel::Medium);
    assert_eq!(raw, 1_024);
    let send = || {
        let req = request(
            a.clone(),
            "MOCK-ID:home MOCK-SCENARIO:reasoning=500 t",
            Some(ok_schema()),
            ReasoningLevel::Medium,
            8_192,
        );
        let adapter = &adapter;
        async move { adapter.send(&req, None).await.unwrap() }
    };
    for i in 0..20 {
        let calibration = output_calibration_from_log(
            &file,
            "anthropic",
            "mock-claude",
            "suggest-title",
            Some(ReasoningLevel::Medium),
        )
        .unwrap();
        assert!(!calibration.calibrated, "erst ab 20 Requests ({i})");
        let reply = send().await;
        assert_eq!(reply.usage.reasoning_tokens, Some(500));
        let row = detailed_request_record(
            &ctx,
            &claude_rates(),
            MicroUsd(100_000),
            &RequestOutcome::success(reply.usage.clone(), 0, 1),
            &OutcomeDetails {
                reasoning: Some(ReasoningLevel::Medium),
                reported_cost: None,
                estimated_output_tokens: Some(raw),
            },
        );
        insert_with_details(file.conn(), &row.record, &row.details).unwrap();
    }
    let calibration = output_calibration_from_log(
        &file,
        "anthropic",
        "mock-claude",
        "suggest-title",
        Some(ReasoningLevel::Medium),
    )
    .unwrap();
    assert!(calibration.calibrated);
    let next = os_ai::cost::estimate_output_tokens(512, ReasoningLevel::Medium, &calibration);
    let actual = send().await.usage.output_tokens.unwrap();
    let deviation = (next as f64 - actual as f64).abs() / actual as f64;
    assert!(deviation <= 0.05, "Schätzung {next}, Ist {actual}");
    let other = output_calibration_from_log(
        &file,
        "anthropic",
        "mock-claude",
        "suggest-title",
        Some(ReasoningLevel::Low),
    )
    .unwrap();
    assert_eq!(other, Calibration::UNCALIBRATED, "andere Stufe unberührt");
    let other_task = output_calibration_from_log(
        &file,
        "anthropic",
        "mock-claude",
        "suggest-meta",
        Some(ReasoningLevel::Medium),
    )
    .unwrap();
    assert!(!other_task.calibrated, "andere Aufgabe unberührt");
}

// spec: AI-012 [M4]
// Schema-Aufgabe über das Anthropic-Mock-Modell: Die unkalibrierte Eingabeschätzung enthält +350 Tokens gegenüber
// derselben Aufgabe ohne Schema (`schema_overhead_tokens` aus `provider-limits.json`); OpenAI Responses +80,
// OpenRouter `openai/*` +80, sonst 0.
#[test]
fn schema_overhead_comes_from_provider_limits() {
    let limits = ProviderLimitsFile::bundled().unwrap();
    let overhead = limits.schema_overhead("anthropic", Endpoint::Messages, "mock-claude");
    assert_eq!(overhead, 350);
    assert_eq!(
        limits.schema_overhead("openai", Endpoint::Responses, "gpt-6-luna"),
        80
    );
    assert_eq!(
        limits.schema_overhead("openrouter", Endpoint::ChatCompletions, "openai/gpt-6-luna"),
        80
    );
    assert_eq!(
        limits.schema_overhead("openrouter", Endpoint::ChatCompletions, "mock/text"),
        0
    );
    let rates = claude_rates();
    let requests = [
        PlannedRequest {
            input_chars: 700,
            images: 0,
            entries: vec![entry("a", 0)],
        },
        PlannedRequest {
            input_chars: 70,
            images: 0,
            entries: vec![entry("b", 0)],
        },
    ];
    let with = estimate_requests(
        &estimate_input(
            &requests,
            prompt_path(512, ReasoningLevel::Off, overhead),
            &rates,
        ),
        &NoCache,
    );
    let without = estimate_requests(
        &estimate_input(&requests, prompt_path(512, ReasoningLevel::Off, 0), &rates),
        &NoCache,
    );
    assert_eq!(
        with.estimate.est_input_tokens - without.estimate.est_input_tokens,
        2 * 350
    );
}

// spec: AI-012 [M4]
// spec: AI-011 [M4]
// Der Voranschlag zeigt die Spalte `Reasoning` mit der effektiven Stufe (z. B. `low` für `suggest-title` auf Modell B,
// dessen Thinking nicht abschaltbar ist).
#[tokio::test]
async fn estimate_shows_the_effective_level() {
    let mock = Mock::start().await;
    let models = mock.anthropic().list_models().await.unwrap();
    let b = models
        .iter()
        .find(|m| m.id == "mock-claude-b")
        .unwrap()
        .to_model();
    let effective = resolve_reasoning_level(ReasoningLevel::Off, &b.reasoning_levels);
    assert_eq!(effective, ReasoningLevel::Low);
    let rates = claude_rates();
    let requests = [PlannedRequest {
        input_chars: 35,
        images: 0,
        entries: vec![entry("x", 0)],
    }];
    let e = estimate_requests(
        &estimate_input(&requests, prompt_path(512, effective, 0), &rates),
        &NoCache,
    );
    assert_eq!(e.reasoning, Some(ReasoningLevel::Low));
    assert_eq!(e.reasoning.unwrap().as_str(), "low");
    assert_eq!(e.estimate.est_output_tokens, 512 + 128);
}

// ---------------------------------------------------------------------------------------------------------------
// AI-013 Reservierung

// spec: AI-013 [M4]
// spec: AI-011 [M4]
// `suggest-title` auf Anthropic-Mock-Modell B (effektive Stufe `low`, `max_tokens = 2048`, `mock-claude`-Preise
// 1/5 USD je 1 Mio.) mit geschätzten 400 Eingabe-Tokens → Reservierung = 400 × 1,25 × 1 + 2.048 × 5 = 10.740 µUSD.
#[test]
fn reservation_uses_max_tokens_with_reasoning_reserve() {
    let max_tokens = default_max_tokens(512, ReasoningLevel::Low, Some(4_000));
    assert_eq!(max_tokens, 2_048);
    let rates = claude_rates();
    assert_eq!(
        reservation_for(PriceBasis::Rates(&rates), 400, max_tokens, 0, false),
        Some(MicroUsd(10_740))
    );
}

// spec: AI-013 [M4]
// spec: AI-005 [M4]
// spec: AI-014 [M4]
// OpenRouter-Szenario `cost-mismatch` mit einer Reservierung kleiner als der gemeldete Betrag → `over_reservation =
// true`, der gemeldete Betrag ist verbucht, danach startet kein weiterer Request (Ledger über `os_store::ai::budget`,
// auch nach erneutem Öffnen der Crawl-Datei).
#[tokio::test]
async fn reported_cost_above_the_reservation_halts_the_budget() {
    let mock = Mock::start().await;
    let adapter = mock.openrouter();
    let models = adapter.list_models().await.unwrap();
    let text = models.iter().find(|m| m.id == "mock/text").unwrap().clone();
    let dir = TempDir::new("mismatch-budget");
    let file = crawl_file(&dir.0);
    let limits = BudgetLimits {
        crawl: MicroUsd(1_000_000),
        ..BudgetLimits::default()
    };
    let ledger = open_ledger(file.conn(), limits.clone(), &[]).unwrap();
    let scope = BudgetScope::task(AREA_AI, "suggest-title");
    let req = request(
        model("openrouter", "mock/text", "chat-completions", Vec::new()),
        "MOCK-ID:home MOCK-SCENARIO:cost-mismatch",
        None,
        ReasoningLevel::ModelDefault,
        64,
    );
    let reply = adapter.send(&req, None).await.unwrap();
    let input = reply.usage.input_tokens.unwrap();
    let basis = PriceBasis::OpenRouter(&text);
    let local = cost_of(
        &TokenCounts::from_usage(&reply.usage),
        FeeCounts::one_request(0),
        &basis.actual_rates(input).unwrap(),
        false,
    )
    .cost;
    // Reservierung genau in Höhe der lokalen Rechnung: kleiner als der gemeldete (doppelte) Betrag.
    let reservation = local;
    let r = ledger.reserve(scope.clone(), reservation).unwrap();
    let row = detailed_request_record(
        &log_ctx(
            "suggest-title",
            "openrouter",
            "mock/text",
            "chat-completions",
        ),
        &basis.actual_rates(input).unwrap(),
        reservation,
        &RequestOutcome::success(reply.usage.clone(), 0, reply.duration_ms),
        &OutcomeDetails {
            reasoning: None,
            reported_cost: reply.reported_cost,
            estimated_output_tokens: None,
        },
    );
    assert_eq!(row.record.price_source, PriceSource::Reported);
    assert_eq!(row.record.cost, MicroUsd(2 * local.0));
    assert_eq!(row.details.computed_cost, Some(local));
    assert!(row.details.cost_mismatch);
    assert!(row.record.over_reservation);
    let settled = ledger.settle(r.id, row.record.cost).unwrap();
    assert!(settled.over_reservation);
    insert_with_details(file.conn(), &row.record, &row.details).unwrap();
    assert!(ledger.reserve(scope.clone(), MicroUsd(1)).is_err());
    // Neu geöffnet: Verbrauch = gemeldeter Betrag, Sperre bleibt.
    let reopened = open_ledger(file.conn(), limits, &[]).unwrap();
    assert_eq!(reopened.snapshot().unwrap().spent, MicroUsd(2 * local.0));
    assert!(reopened.reserve(scope, MicroUsd(1)).is_err());
    assert_eq!(total_cost(file.conn()).unwrap(), MicroUsd(2 * local.0));
    assert_eq!(mock.requests_to(CHAT).len(), 1);
}

// spec: AI-005 [M4]
// spec: AI-013 [M4]
// Ersetzt `pending_m4_k09::openrouter_long_context_and_internal_reasoning_in_cost_core`: Langkontext ohne gemeldete
// Kosten (Szenario `no-cost`) auf `M-LC`: 1.000 Eingabe- und 100 Ausgabe-Tokens → 2.400 µUSD; 999 → 1.199 µUSD
// (Protokoll mit `Price Source = api`); `M-IR` mit `max_tokens = 1.000`: die Reservierung enthält für die Ausgabe
// 1.000 × 10 µUSD = 10.000 µUSD (nicht 2.000); Langkontext in der Reservierung, sobald ⌈geschätzt × 1,25⌉ das
// kleinste `min_prompt_tokens` erreicht.
#[tokio::test]
async fn openrouter_long_context_and_internal_reasoning_in_cost_core() {
    let mock = Mock::start().await;
    let models = mock.openrouter().list_models().await.unwrap();
    let find =
        |id: &str| -> OpenRouterModel { models.iter().find(|m| m.id == id).unwrap().clone() };
    let lc = find("mock/global-a");
    let basis = PriceBasis::OpenRouter(&lc);
    let row = |input: u64| {
        detailed_request_record(
            &log_ctx(
                "suggest-title",
                "openrouter",
                "mock/global-a",
                "chat-completions",
            ),
            &basis.actual_rates(input).unwrap(),
            MicroUsd(0),
            &RequestOutcome::success(usage(input, 100, None), 0, 1),
            &OutcomeDetails::default(),
        )
    };
    assert_eq!(row(1_000).record.cost, MicroUsd(2_400));
    assert_eq!(row(999).record.cost, MicroUsd(1_199));
    assert_eq!(row(999).record.price_source, PriceSource::Api);
    assert_eq!(row(999).details.computed_cost, Some(MicroUsd(1_199)));
    // Reservierung: 799 × 1,25 = 999 → Grundpreis; 800 × 1,25 = 1.000 → Langkontext.
    assert_eq!(
        reservation_for(basis, 799, 100, 0, false),
        Some(MicroUsd(999 + 200))
    );
    assert_eq!(
        reservation_for(basis, 800, 100, 0, false),
        Some(MicroUsd(2_000 + 400))
    );
    let ir = find("mock/global-b");
    let ir_basis = PriceBasis::OpenRouter(&ir);
    let with = reservation_for(ir_basis, 0, 1_000, 0, false).unwrap();
    let without = reservation_for(ir_basis, 0, 0, 0, false).unwrap();
    assert_eq!(with.0 - without.0, 10_000);
    // Die Ist-Kosten rechnen `completion`, nicht `internal_reasoning`.
    let completion = ir.rates_for(0).unwrap().output_pico;
    assert!(completion < ir.internal_reasoning_pico);
    assert_eq!(ir_basis.actual_rates(0).unwrap().output_pico, completion);
}

// ---------------------------------------------------------------------------------------------------------------
// AI-014 Protokoll, AI-005 Ist-Kosten

// spec: AI-005 [M4]
// spec: AI-014 [M4]
// spec: AI-013 [M4]
// Ersetzt `pending_m4_k09::openrouter_reported_cost_in_log_and_budget`: Standardantwort → `Cost (USD)` = `usage.cost`,
// `Price Source = reported`, `Computed Cost (USD)` = lokale Rechnung, `cost_mismatch = false`; Szenario
// `cost-mismatch` → gemeldeter (doppelter) Betrag, `cost_mismatch = true`, im Budgetverbrauch enthalten; Szenarien
// `no-cost` und `byok` → `Price Source = api`, `Cost` = lokale Rechnung; Bildrequest über das Mock-Modell mit
// `pricing.request = 0.001` und `pricing.image = 0.002` → `Fixed Fees = 0.003000`, `Token Cost = Cost − Fixed Fees`.
#[tokio::test]
async fn openrouter_reported_cost_in_log_and_budget() {
    let mock = Mock::start().await;
    let adapter = mock.openrouter();
    let models = adapter.list_models().await.unwrap();
    let dir = TempDir::new("reported");
    let file = crawl_file(&dir.0);
    let mut expected_spend = 0u64;
    for (scenario, source, mismatch, factor) in [
        ("ok", PriceSource::Reported, false, 1),
        ("cost-mismatch", PriceSource::Reported, true, 2),
        ("no-cost", PriceSource::Api, false, 1),
        ("byok", PriceSource::Api, false, 1),
    ] {
        let text = models.iter().find(|m| m.id == "mock/text").unwrap();
        let req = request(
            model("openrouter", "mock/text", "chat-completions", Vec::new()),
            &format!("MOCK-ID:k09-long MOCK-SCENARIO:{scenario}"),
            None,
            ReasoningLevel::ModelDefault,
            256,
        );
        let reply = adapter.send(&req, None).await.unwrap();
        let rates = PriceBasis::OpenRouter(text)
            .actual_rates(reply.usage.input_tokens.unwrap())
            .unwrap();
        let local = cost_of(
            &TokenCounts::from_usage(&reply.usage),
            FeeCounts::one_request(0),
            &rates,
            false,
        );
        let row = detailed_request_record(
            &log_ctx(
                "suggest-title",
                "openrouter",
                "mock/text",
                "chat-completions",
            ),
            &rates,
            MicroUsd(1_000_000),
            &RequestOutcome::success(reply.usage.clone(), 0, 1),
            &OutcomeDetails {
                reasoning: Some(ReasoningLevel::ModelDefault),
                reported_cost: reply.reported_cost,
                estimated_output_tokens: None,
            },
        );
        let r = &row.record;
        assert_eq!(r.price_source, source, "{scenario}");
        assert_eq!(r.cost, MicroUsd(local.cost.0 * factor), "{scenario}");
        assert_eq!(row.details.computed_cost, Some(local.cost), "{scenario}");
        assert_eq!(row.details.cost_mismatch, mismatch, "{scenario}");
        assert_eq!(
            r.cost.0,
            r.token_cost.0 + r.fixed_fees.0 + r.regional_uplift.0
        );
        insert_with_details(file.conn(), r, &row.details).unwrap();
        expected_spend += r.cost.0;
    }
    assert_eq!(total_cost(file.conn()).unwrap(), MicroUsd(expected_spend));
    let ledger = open_ledger(
        file.conn(),
        BudgetLimits {
            crawl: MicroUsd(1_000_000),
            ..BudgetLimits::default()
        },
        &[],
    )
    .unwrap();
    assert_eq!(ledger.snapshot().unwrap().spent, MicroUsd(expected_spend));

    // Bildrequest mit gemeldeten Kosten: Gebühren lokal, Rest Tokenkosten, kein Regionalzuschlag.
    let vision = models.iter().find(|m| m.id == "mock/vision").unwrap();
    let png = [
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0, b'I', b'E', b'N', b'D',
    ];
    let mut req = request(
        vision.to_model(),
        "MOCK-ID:home Bild",
        None,
        ReasoningLevel::ModelDefault,
        256,
    );
    req.input.push(image_input(&png).unwrap());
    req.required.vision = true;
    let reply = adapter.send(&req, None).await.unwrap();
    let rates = PriceBasis::OpenRouter(vision)
        .actual_rates(reply.usage.input_tokens.unwrap())
        .unwrap();
    let row = detailed_request_record(
        &log_ctx(
            "suggest-alt",
            "openrouter",
            "mock/vision",
            "chat-completions",
        ),
        &rates,
        MicroUsd(1_000_000),
        &RequestOutcome::success(reply.usage.clone(), 1, 1),
        &OutcomeDetails {
            reasoning: None,
            reported_cost: reply.reported_cost,
            estimated_output_tokens: None,
        },
    );
    let r = &row.record;
    assert_eq!(r.price_source, PriceSource::Reported);
    assert_eq!(format!("{:.6}", r.fixed_fees.0 as f64 / 1e6), "0.003000");
    assert_eq!(r.token_cost.0, r.cost.0 - 3_000);
    assert_eq!(r.regional_uplift, MicroUsd(0));
    assert!(!row.details.cost_mismatch);
}

// spec: AI-005 [M4]
// Aufteilung gemeldeter Kosten (wäre `Token Cost` negativ, gilt `Fixed Fees` = `Cost`) und Schwelle von `cost_mismatch`
// (mehr als 1 % **und** mehr als 1 µUSD).
#[test]
fn reported_split_and_mismatch_threshold() {
    let b = reported_breakdown(MicroUsd(2_000), MicroUsd(3_000));
    assert_eq!(
        (b.token_cost, b.fixed_fees, b.regional_uplift, b.cost),
        (MicroUsd(0), MicroUsd(2_000), MicroUsd(0), MicroUsd(2_000))
    );
    let b = reported_breakdown(MicroUsd(5_000), MicroUsd(3_000));
    assert_eq!(
        (b.token_cost, b.fixed_fees),
        (MicroUsd(2_000), MicroUsd(3_000))
    );
    assert!(
        !cost_mismatch(MicroUsd(1_010), MicroUsd(1_000)),
        "genau 1 %"
    );
    assert!(cost_mismatch(MicroUsd(1_011), MicroUsd(1_000)));
    assert!(!cost_mismatch(MicroUsd(2), MicroUsd(1)), "nur 1 µUSD");
    assert!(cost_mismatch(MicroUsd(3), MicroUsd(1)));
    assert!(cost_mismatch(MicroUsd(5), MicroUsd(0)));
    assert!(!cost_mismatch(MicroUsd(0), MicroUsd(0)));
}

// spec: AI-014 [M4]
// spec: AI-002 [M4]
// Ersetzt `pending_m4_k09::anthropic_reasoning_tokens_column`: Szenario `reasoning=40` → `Reasoning Tokens = 40`,
// `Output Tokens` = Antwort + 40, `Token Cost` einmal aus `Output Tokens`; 3 Antworten auf `mock-claude` (1/5 USD je
// 1 Mio.) mit je 1.000 Eingabe- und 200 Ausgabe-Tokens, davon 120 Reasoning → Summenzeile `Input Tokens 3000`,
// `Output Tokens 600`, `Reasoning Tokens 360`, `Cost 0.006000` (Reasoning nicht doppelt).
#[tokio::test]
async fn anthropic_reasoning_tokens_column() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let a = model("anthropic", "mock-claude", "messages", Vec::new());
    let send = |text: &'static str| {
        let req = request(
            a.clone(),
            text,
            Some(ok_schema()),
            ReasoningLevel::Medium,
            4_096,
        );
        let adapter = &adapter;
        async move { adapter.send(&req, None).await.unwrap() }
    };
    let plain = send("MOCK-ID:home r").await;
    let with = send("MOCK-ID:home MOCK-SCENARIO:reasoning=40 r").await;
    let dir = TempDir::new("reasoning-column");
    let file = crawl_file(&dir.0);
    let ctx = log_ctx("suggest-title", "anthropic", "mock-claude", "messages");
    let row = detailed_request_record(
        &ctx,
        &claude_rates(),
        MicroUsd(100_000),
        &RequestOutcome::success(with.usage.clone(), 0, 1),
        &OutcomeDetails {
            reasoning: Some(ReasoningLevel::Medium),
            reported_cost: None,
            estimated_output_tokens: None,
        },
    );
    assert_eq!(row.details.reasoning_tokens, Some(40));
    assert_eq!(
        row.record.output_tokens,
        plain.usage.output_tokens.map(|o| o + 40)
    );
    let once = cost_of(
        &TokenCounts::plain(
            row.record.input_tokens.unwrap(),
            row.record.output_tokens.unwrap(),
        ),
        FeeCounts::one_request(0),
        &claude_rates(),
        false,
    );
    assert_eq!(row.record.token_cost, once.token_cost);
    insert_with_details(file.conn(), &row.record, &row.details).unwrap();
    let logged = list(file.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(logged[0].details.reasoning_tokens, Some(40));
    assert_eq!(logged[0].details.reasoning_level.as_deref(), Some("medium"));

    // Summenzeile über 3 Antworten mit je 1.000/200 Tokens, davon 120 Reasoning.
    let ctx = log_ctx("suggest-meta", "anthropic", "mock-claude", "messages");
    for _ in 0..3 {
        let row = detailed_request_record(
            &ctx,
            &claude_rates(),
            MicroUsd(100_000),
            &RequestOutcome::success(usage(1_000, 200, Some(120)), 0, 1),
            &OutcomeDetails {
                reasoning: Some(ReasoningLevel::Low),
                reported_cost: None,
                estimated_output_tokens: None,
            },
        );
        insert_with_details(file.conn(), &row.record, &row.details).unwrap();
    }
    let totals = totals_by_task_and_model(file.conn()).unwrap();
    let t = totals.iter().find(|t| t.task == "suggest-meta").unwrap();
    assert_eq!(
        (t.input_tokens, t.output_tokens, t.reasoning_tokens),
        (3_000, 600, 360)
    );
    assert_eq!(format!("{:.6}", t.cost.0 as f64 / 1e6), "0.006000");
    assert_eq!(t.computed_cost, t.cost);
}

// spec: AI-001 [M4]
// spec: AI-014 [M4]
// Ersetzt `pending_m4_k09::anthropic_error_request_id_in_log`: Die Anbieter-Request-ID einer Fehlerantwort kommt über
// `AttemptFailure`/`FinalError` (seit M4-K09) bis in die Protokollspalte `Provider Request ID`: ai-mock-Szenario `401`
// mit Header `request-id`, und eine Fehlerantwort ohne Header, aber mit `request_id` im Körper → dieser Wert.
#[tokio::test]
async fn anthropic_error_request_id_in_log() {
    let mock = Mock::start().await;
    let adapter = mock.anthropic();
    let limiter = limiter();
    let dir = TempDir::new("error-id");
    let file = crawl_file(&dir.0);
    let ctx = log_ctx("suggest-title", "anthropic", "mock-claude", "messages");
    let req = request(
        model("anthropic", "mock-claude", "messages", Vec::new()),
        "MOCK-SCENARIO:401 x",
        None,
        ReasoningLevel::ModelDefault,
        256,
    );
    let spec = os_ai::limits::RequestSpec::new("anthropic", "mock-claude", 10);
    let exec = limiter
        .run(&spec, |_| {
            let (adapter, req) = (&adapter, &req);
            async move { adapter.send(req, None).await }
        })
        .await;
    let attempts = exec.attempts;
    let error = exec.result.unwrap_err();
    let header_id = error.provider_request_id.clone().expect("request-id");
    assert!(header_id.starts_with("req_mock_"), "{header_id}");
    let outcome = RequestOutcome::from_final_error(&error, attempts, 5);
    assert_eq!(outcome.usage, UsageReport::NoResponse);
    let row = detailed_request_record(
        &ctx,
        &claude_rates(),
        MicroUsd(1_000),
        &outcome,
        &OutcomeDetails::default(),
    );
    assert_eq!(row.record.cost, MicroUsd(0));
    insert_with_details(file.conn(), &row.record, &row.details).unwrap();

    let body = json!({"type": "error", "error": {"type": "invalid_request_error", "message": "bad"},
                      "request_id": "req_body_k10"});
    let body_id = anthropic::request_id(&HeaderMap::new(), &body);
    let exec = limiter
        .run(&spec, |_| {
            let body_id = body_id.clone();
            async move {
                Err::<(), _>(
                    AttemptFailure::from(ProviderError::Http {
                        status: 400,
                        message: "bad".into(),
                    })
                    .with_request_id(body_id),
                )
            }
        })
        .await;
    let error = exec.result.unwrap_err();
    let row = detailed_request_record(
        &ctx,
        &claude_rates(),
        MicroUsd(1_000),
        &RequestOutcome::from_final_error(&error, exec.attempts, 1),
        &OutcomeDetails::default(),
    );
    insert_with_details(file.conn(), &row.record, &row.details).unwrap();
    let ids: Vec<Option<String>> = list(file.conn(), &RequestFilter::default())
        .unwrap()
        .into_iter()
        .map(|r| r.record.provider_request_id)
        .collect();
    assert_eq!(ids, [Some(header_id), Some("req_body_k10".to_string())]);
    assert_eq!(
        mock.requests_to(MESSAGES).len(),
        1,
        "401 wird nicht wiederholt"
    );
}

fn level_strategy() -> impl Strategy<Value = Option<ReasoningLevel>> {
    prop_oneof![
        Just(None),
        Just(Some(ReasoningLevel::ModelDefault)),
        Just(Some(ReasoningLevel::Off)),
        Just(Some(ReasoningLevel::Minimal)),
        Just(Some(ReasoningLevel::Low)),
        Just(Some(ReasoningLevel::Medium)),
        Just(Some(ReasoningLevel::High)),
        Just(Some(ReasoningLevel::XHigh)),
        Just(Some(ReasoningLevel::Max)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    // spec: AI-014 [M4]
    // Jede Zeile hat `Price Source` aus {`api`, `table`, `user`, `estimate`, `reported`} und `Reasoning Level` aus der
    // Stufenmenge von AI-011 oder leer; `Cost = Token Cost + Fixed Fees + Regional Uplift` auch für `reported`;
    // `Computed Cost` = `Cost`, außer bei `reported`.
    #[test]
    fn every_row_has_valid_source_level_and_cost_split(
        input in 0u64..50_000,
        output in 0u64..5_000,
        reasoning in proptest::option::of(0u64..5_000),
        images in 0u32..3,
        reported in proptest::option::of(0u64..100_000),
        byok in any::<bool>(),
        missing in any::<bool>(),
        level in level_strategy(),
        which in 0usize..4,
    ) {
        let source = [PriceSource::Api, PriceSource::Table, PriceSource::User, PriceSource::Api][which];
        let rates = Rates {
            per_request_pico: 1_000_000_000,
            per_image_pico: 2_000_000_000,
            regional_uplift_ppm: if which == 1 { 100_000 } else { 0 },
            ..Rates::per_million("1", "5", source).unwrap()
        };
        let mut outcome = RequestOutcome::success(usage(input, output, reasoning), images, 1);
        if missing {
            outcome.usage = UsageReport::Missing { estimated_input_tokens: input, estimated_output_tokens: 0 };
        }
        let reported_cost = reported.map(|c| os_ai::adapters::anthropic::shared::ReportedCost {
            cost_micro_usd: Some(c),
            upstream_inference_cost_micro_usd: None,
            is_byok: Some(byok),
        });
        let row = detailed_request_record(
            &log_ctx("t", "p", "m", "e"),
            &rates,
            MicroUsd(0),
            &outcome,
            &OutcomeDetails { reasoning: level, reported_cost, estimated_output_tokens: None },
        );
        let r = &row.record;
        prop_assert!(["api", "table", "user", "estimate", "reported"].contains(&r.price_source.as_str()));
        if let Some(l) = &row.details.reasoning_level {
            prop_assert!(os_store::ai::requests::REASONING_LEVELS.contains(&l.as_str()));
        }
        prop_assert_eq!(r.cost.0, r.token_cost.0 + r.fixed_fees.0 + r.regional_uplift.0);
        let is_reported = reported.is_some() && !byok && !missing;
        prop_assert_eq!(r.price_source == PriceSource::Reported, is_reported);
        if !is_reported {
            prop_assert_eq!(row.details.computed_cost, Some(r.cost));
            prop_assert!(!row.details.cost_mismatch);
        } else {
            prop_assert_eq!(r.cost, MicroUsd(reported.unwrap()));
        }
        let dir = TempDir::new("prop");
        let file = crawl_file(&dir.0);
        insert_with_details(file.conn(), r, &row.details).unwrap();
        let back = &list(file.conn(), &RequestFilter::default()).unwrap()[0];
        prop_assert_eq!(&back.record, r);
        prop_assert_eq!(back.details.reasoning_level.clone(), row.details.reasoning_level.clone());
        prop_assert_eq!(back.details.computed_cost, row.details.computed_cost);
    }
}

// spec: AI-002 [M4]
// Der Abbruch an der Ausgabegrenze wird mit den gemeldeten Tokens verbucht (Kosten > 0) und landet mit Status, Grund
// und `HTTP Status = 200` in der Crawl-Datei.
#[tokio::test]
async fn output_limit_row_is_stored_with_its_cost() {
    let mock = Mock::start().await;
    let reply = mock
        .anthropic()
        .send(
            &request(
                model("anthropic", "mock-claude", "messages", Vec::new()),
                "MOCK-SCENARIO:output-limit x",
                None,
                ReasoningLevel::ModelDefault,
                300,
            ),
            None,
        )
        .await
        .unwrap();
    let limit = reply.for_repair().unwrap_err();
    let dir = TempDir::new("limit-row");
    let file = crawl_file(&dir.0);
    let row = detailed_request_record(
        &log_ctx("suggest-title", "anthropic", "mock-claude", "messages"),
        &claude_rates(),
        MicroUsd(10_000),
        &RequestOutcome::output_limit(&limit, 0),
        &OutcomeDetails::default(),
    );
    insert_with_details(file.conn(), &row.record, &row.details).unwrap();
    let back = &list(
        file.conn(),
        &RequestFilter {
            status: Some(AiStatus::OutputLimitReached.to_string()),
            ..RequestFilter::default()
        },
    )
    .unwrap()[0];
    assert_eq!(back.record.output_tokens, Some(300));
    assert_eq!(back.record.http_status, Some(200));
    assert_eq!(back.record.error_code.as_deref(), Some("max_tokens"));
    assert_eq!(
        back.record.cost,
        MicroUsd(300 * 5 + back.record.input_tokens.unwrap())
    );
}

// spec: AI-012 [M4]
// Nachtrag 26.11.2026: Zwei Gruppen von je 20 Requests derselben (Modell, Aufgabe, Stufe) mit verschiedenem
// Antwortanteil A (Ersatzweg mit W = 2 bzw. W = 16) und gleichem Verhältnis Ist/Schätzung 0,5 → der gelernte Faktor
// ist 0,5 (±1 %), unabhängig von A. Die Schätzung je Zeile steht in der internen Spalte `estimated_output_tokens`.
#[test]
fn output_factor_is_learned_per_row_independent_of_answer_share() {
    let dir = TempDir::new("output-factor-rows");
    let file = crawl_file(&dir.0);
    let ctx = log_ctx("decision", "anthropic", "mock-claude", "messages");
    let level = ReasoningLevel::Off;
    for weight in [2u64, 16] {
        let raw =
            os_ai::cost::raw_output_tokens(os_ai::catalog::decision_answer_tokens(weight), level);
        for _ in 0..20 {
            let row = detailed_request_record(
                &ctx,
                &claude_rates(),
                MicroUsd(100_000),
                &RequestOutcome::success(usage(500, raw / 2, Some(0)), 0, 1),
                &OutcomeDetails {
                    reasoning: Some(level),
                    reported_cost: None,
                    estimated_output_tokens: Some(raw),
                },
            );
            assert_eq!(row.details.estimated_output_tokens, Some(raw));
            insert_with_details(file.conn(), &row.record, &row.details).unwrap();
        }
    }
    let calibration =
        output_calibration_from_log(&file, "anthropic", "mock-claude", "decision", Some(level))
            .unwrap();
    assert!(calibration.calibrated);
    let factor = calibration.factor_ppm as f64 / 1e6;
    assert!((factor - 0.5).abs() <= 0.005, "Faktor {factor}");
    // Beide Antwortanteile werden mit demselben Faktor korrigiert.
    for weight in [2u64, 16] {
        let answer = os_ai::catalog::decision_answer_tokens(weight);
        let next = os_ai::cost::estimate_output_tokens(answer, level, &calibration);
        let raw = os_ai::cost::raw_output_tokens(answer, level);
        assert!((next as f64 - raw as f64 / 2.0).abs() <= raw as f64 * 0.01);
    }
    let logged = list(file.conn(), &RequestFilter::default()).unwrap();
    assert_eq!(logged[0].details.estimated_output_tokens, Some(64 + 32));
    assert!(!os_store::ai::requests::COLUMNS.contains(&"estimated_output_tokens"));
}

// spec: AI-010 [M4]
// Nachtrag 25.11.2026 „Preise der Decisions API“ (Befund B1-1 der Abnahme K10): Ein Decisions-Request mit 1.000.000
// Eingabe-Tokens, davon 400.000 aus dem Cache, kostet 600.000 × 0,10 + 400.000 × 0,01 je 1 Mio. = 0,064 USD vor dem
// Langkontext-Faktor und mit dem Faktor des `responses`-Eintrags 0,128 USD – nicht 0,10 bzw. 0,20 USD pauschal. Die
// Ausgabe bleibt 0, auch wenn Tokens gemeldet werden.
#[test]
fn decisions_billing_uses_the_cache_prices_of_the_table() {
    use os_ai::adapters::openai_decisions::decisions_billing_rates;
    use os_ai::catalog::{PriceKey, PriceTable};
    let table = PriceTable::bundled().unwrap();
    let rates = table
        .get(&PriceKey::new("openai", "gpt-6-luna", Endpoint::Decisions))
        .unwrap()
        .rates(None)
        .unwrap();
    let billing = decisions_billing_rates(&rates);
    assert_eq!(billing.output_pico, 0);
    assert_eq!(billing.cache_read_pico, rates.cache_read_pico);
    assert_eq!(billing.cache_write_pico, rates.cache_write_pico);
    let outcome = RequestOutcome::success(
        Usage {
            input_tokens: Some(1_000_000),
            output_tokens: Some(50),
            cached_input_tokens: Some(400_000),
            cache_write_tokens: None,
            reasoning_tokens: Some(0),
        },
        0,
        1,
    );
    let ctx = log_ctx("decision", "openai", "gpt-6-luna", "decisions");
    let with_lc = detailed_request_record(
        &ctx,
        &billing,
        MicroUsd(0),
        &outcome,
        &OutcomeDetails::default(),
    );
    assert_eq!(
        with_lc.record.cost,
        MicroUsd(128_000),
        "0,128 USD mit Langkontext-Faktor"
    );
    let without_lc = os_ai::cost::Rates {
        long_context: None,
        ..billing.clone()
    };
    let row = detailed_request_record(
        &ctx,
        &without_lc,
        MicroUsd(0),
        &outcome,
        &OutcomeDetails::default(),
    );
    assert_eq!(
        row.record.cost,
        MicroUsd(64_000),
        "0,064 USD vor dem Faktor"
    );
}

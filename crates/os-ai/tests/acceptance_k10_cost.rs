//! Unabhängige Abnahmetests M4-K10: Kostenschätzung und Reservierung nach den Nachträgen „24.11.2026, Q00“ an AI-012
//! und AI-013 sowie „25.11.2026“ an AI-010 (Preise der Decisions API).
//!
//! Geprüft: Decisions-Formel je Frage mit G aus `provider-limits.json`, Bündelung ohne Tokenersparnis, Ausgabe-Schätzung
//! (A + E) × Ausgabe-Korrekturfaktor je (Modell, Aufgabe, Stufe), Schema-Zuschlag, Rechenbeispiel aus AI-010 (0,064 /
//! 0,128 USD, Schwelle über alle Eingabe-Tokens), Reservierung mit `max_tokens` aus AI-011, OpenRouter `overrides` und
//! `internal_reasoning`, Reservierung gegen das Budget einschließlich Über- und Unterschreitung.
// acceptance: independent

mod acceptance_k10_support;

use acceptance_k10_support::*;
use os_ai::adapters::openai_decisions::{decisions_billing_rates, DecisionInput, Question};
use os_ai::catalog::{
    default_max_tokens, AnswerTokens, Endpoint, PriceKey, PriceTable, ProviderLimitsFile,
    TaskDefaults, TaskType,
};
use os_ai::cost::decisions::bundled_question_overhead;
use os_ai::cost::reserve::status_for;
use os_ai::cost::{
    cost_of, decisions_input_tokens, detailed_request_record, estimate_output_tokens,
    estimate_requests, expected_reasoning_tokens, output_calibration_from_log, question_text_chars,
    raw_output_tokens, reservation_for, Calibration, EstimatePath, FeeCounts, NoCache,
    OutcomeDetails, PlannedEntry, PlannedRequest, PriceBasis, Rates, RequestEstimateInput,
    RequestOutcome, RequestTaskEstimate, TokenCounts,
};
use os_ai::provider::{resolve_reasoning_level, ReasoningLevel};
use os_ai::AiStatus;
use os_model::budget::{
    BudgetError, BudgetLedger, BudgetLevel, BudgetLimits, BudgetScope, CrawlBudgetLedger, MicroUsd,
    AREA_AI,
};
use serde_json::Value;

fn luna(endpoint: Endpoint) -> Rates {
    PriceTable::bundled()
        .expect("pricing.json")
        .get(&PriceKey::new("openai", "gpt-6-luna", endpoint))
        .expect("Eintrag gpt-6-luna")
        .rates(None)
        .expect("Sätze")
}

fn micro(tokens: &TokenCounts, rates: &Rates) -> u64 {
    cost_of(tokens, FeeCounts::one_request(0), rates, false)
        .cost
        .0
}

fn text_of(len: usize) -> String {
    let base = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ";
    base.chars().cycle().take(len).collect()
}

/// Prädikatfrage mit genau `z_q` Zeichen Fragentext (Name + Anweisung).
fn predicate_with_chars(name: &str, z_q: usize) -> Question {
    let instructions = text_of(z_q - name.chars().count());
    Question::predicate(name, instructions)
}

fn estimate(
    requests: &[PlannedRequest],
    path: EstimatePath,
    basis: Option<PriceBasis<'_>>,
    output_calibration: Calibration,
) -> RequestTaskEstimate {
    estimate_requests(
        &RequestEstimateInput {
            task: "task".into(),
            model: "model".into(),
            requests,
            path,
            basis,
            api_key_present: true,
            calibration: Calibration::UNCALIBRATED,
            output_calibration,
            batch_capable: false,
        },
        &NoCache,
    )
}

fn decisions_path(g: u64) -> EstimatePath {
    EstimatePath::DecisionsApi {
        question_overhead_tokens: g,
    }
}

fn prompt_path(
    instruction_tokens: u64,
    schema_overhead_tokens: u64,
    answer: AnswerTokens,
    reasoning: ReasoningLevel,
) -> EstimatePath {
    EstimatePath::Prompt {
        instruction_tokens,
        schema_overhead_tokens,
        answer,
        reasoning,
    }
}

fn entry(question_chars: u64, weight: u64) -> PlannedEntry {
    PlannedEntry {
        cache_key: None,
        question_chars,
        weight,
    }
}

// spec: AI-012 [M4]
// acceptance: independent
// Kriterium (Decisions-Prüfwert): 1 Request, Eingabetext 350 Zeichen, 2 Fragen mit Fragentext 140 bzw. 210 Zeichen,
// kein Bild, unkalibriert → Est. Input Tokens = (100 + 40 + 100) + (100 + 60 + 100) = 500, erwartete Kosten 50 µUSD; die
// Mock-Ist-Tokens desselben Requests sind (88 + 35 + 100) + (88 + 53 + 100) = 464 (Abweichung +7,8 %).
#[tokio::test]
async fn decisions_example_per_question() {
    let g = ProviderLimitsFile::bundled()
        .expect("provider-limits.json")
        .decisions_question_overhead();
    assert_eq!(g, 100, "G = 100 aus provider-limits.json");
    assert_eq!(decisions_input_tokens(350, 0, 0, &[140, 210], g), 500);

    let questions = [
        predicate_with_chars("q1", 140),
        predicate_with_chars("q2", 210),
    ];
    for (q, z) in questions.iter().zip([140u64, 210]) {
        assert_eq!(q.text_chars(), z);
        let sent = serde_json::to_value(q).unwrap();
        assert_eq!(
            question_text_chars(&sent),
            z,
            "Z_q über das gesendete JSON: {sent}"
        );
    }
    let input = text_of(350);
    assert_eq!(
        os_ai::adapters::openai_decisions::estimate_input_tokens(
            &DecisionInput::text(input.clone()),
            &questions,
            0
        ),
        500
    );

    let rates = fixture_rates("openai", "gpt-6-luna", "decisions");
    let planned = [PlannedRequest {
        input_chars: 350,
        images: 0,
        entries: vec![entry(140, 2), entry(210, 2)],
    }];
    let est = estimate(
        &planned,
        decisions_path(g),
        Some(PriceBasis::Rates(&rates)),
        Calibration::UNCALIBRATED,
    );
    assert_eq!(est.estimate.est_input_tokens, 500);
    assert_eq!(est.estimate.est_output_tokens, 0, "Decisions: Ausgabe 0");
    assert_eq!(est.estimate.expected, Some(MicroUsd(50)));
    assert_eq!(est.reasoning, None, "Decisions ohne Stufe");
    assert_eq!(est.requests, 1);
    assert_eq!(est.estimate.items, 2);

    // Mock-Ist-Tokens desselben Requests.
    let mock = Mock::start().await;
    let reply = mock
        .decisions()
        .send("gpt-6-luna", &DecisionInput::text(input), &questions, None)
        .await
        .expect("Mock antwortet");
    let actual = reply.usage.and_then(|u| u.input_tokens).expect("usage");
    assert_eq!(actual, 464);
    let deviation = (500.0 - actual as f64) / actual as f64 * 100.0;
    assert!((7.7..7.9).contains(&deviation), "{deviation}");
}

// spec: AI-012 [M4]
// acceptance: independent
// Kriterium: 5 Fragen in einem Request ergeben dieselbe Schätzung wie die Summe der 5 Einzelschätzungen (Bündelung
// ändert die Tokenschätzung nicht); die Requestzahl im Voranschlag sinkt.
#[test]
fn bundling_saves_requests_not_tokens() {
    let rates = fixture_rates("openai", "gpt-6-luna", "decisions");
    let z = [59u64, 60, 146, 229, 196];
    let bundled = [PlannedRequest {
        input_chars: 159,
        images: 0,
        entries: z.iter().map(|z| entry(*z, 2)).collect(),
    }];
    let single: Vec<PlannedRequest> = z
        .iter()
        .map(|z| PlannedRequest {
            input_chars: 159,
            images: 0,
            entries: vec![entry(*z, 2)],
        })
        .collect();
    let a = estimate(
        &bundled,
        decisions_path(100),
        Some(PriceBasis::Rates(&rates)),
        Calibration::UNCALIBRATED,
    );
    let b = estimate(
        &single,
        decisions_path(100),
        Some(PriceBasis::Rates(&rates)),
        Calibration::UNCALIBRATED,
    );
    assert_eq!(a.estimate.est_input_tokens, b.estimate.est_input_tokens);
    // Kosten werden je Request auf µUSD gerundet; die Tokenschätzung ist gleich, die Beträge höchstens um die
    // Rundung je Request (5 × 0,5 µUSD) verschieden.
    let (ca, cb) = (
        a.estimate.expected.unwrap().0,
        b.estimate.expected.unwrap().0,
    );
    assert!(ca.abs_diff(cb) <= 3, "{ca} vs {cb}");
    assert_eq!((a.requests, b.requests), (1, 5));
    // Je Frage ⌈159/3,5⌉ = 46 Tokens Eingabetext + ⌈Z_q/3,5⌉ + 100.
    let expected: u64 = z.iter().map(|z| 46 + (2 * z).div_ceil(7) + 100).sum();
    assert_eq!(a.estimate.est_input_tokens, expected);
}

// spec: AI-012 [M4]
// acceptance: independent
// G ist `decisions_question_overhead_tokens` aus `provider-limits.json` mit Quelle „Live-Probe Q00“ und `verified_at`
// 2026-11-24; eine Datei mit anderem G ändert die Schätzung entsprechend.
#[test]
fn question_overhead_comes_from_provider_limits() {
    let delivered: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("resources/ai/provider-limits.json")).unwrap(),
    )
    .unwrap();
    let g = &delivered["decisions_question_overhead_tokens"];
    assert_eq!(g["value"], 100);
    assert!(
        g["source_url"].as_str().unwrap().contains("Live-Probe Q00"),
        "{g}"
    );
    assert_eq!(g["verified_at"], "2026-11-24");
    assert_eq!(bundled_question_overhead(), 100);

    let other =
        ProviderLimitsFile::from_json_str(&fixture_text("provider-limits-g150.json")).unwrap();
    assert_eq!(other.decisions_question_overhead(), 150);
    let planned = [PlannedRequest {
        input_chars: 350,
        images: 0,
        entries: vec![entry(140, 2), entry(210, 2)],
    }];
    let est = estimate(
        &planned,
        decisions_path(other.decisions_question_overhead()),
        None,
        Calibration::UNCALIBRATED,
    );
    assert_eq!(est.estimate.est_input_tokens, 600);
}

// spec: AI-012 [M4]
// acceptance: independent
// Kriterium (Ausgabe-Prüfwert unkalibriert): `suggest-title` mit effektiver Stufe `off` → Est. Output Tokens = 512 je
// Request; mit `medium` → 1.024; Decisions → 0. Dazu die Tabelle E je Stufe und A nach `decision-w`.
#[test]
fn output_estimate_by_effective_level() {
    let defaults = TaskDefaults::bundled().expect("task-defaults.json");
    let a = defaults
        .answer_tokens(TaskType::SuggestTitle)
        .expect("A für suggest-title");
    assert_eq!(a.tokens(None), 512);

    let table = [
        (ReasoningLevel::Off, 0u64),
        (ReasoningLevel::Minimal, 64),
        (ReasoningLevel::Low, 128),
        (ReasoningLevel::Medium, 512),
        (ReasoningLevel::High, 2_048),
        (ReasoningLevel::XHigh, 4_096),
        (ReasoningLevel::Max, 8_192),
        (ReasoningLevel::ModelDefault, 512),
    ];
    for (level, e) in table {
        assert_eq!(expected_reasoning_tokens(level), e, "{level}");
        assert_eq!(
            estimate_output_tokens(512, level, &Calibration::UNCALIBRATED),
            512 + e
        );
    }

    let one = [PlannedRequest {
        input_chars: 700,
        images: 0,
        entries: vec![entry(0, 0)],
    }];
    let rates = fixture_rates("openai", "gpt-6-luna", "responses");
    for (level, out) in [
        (ReasoningLevel::Off, 512u64),
        (ReasoningLevel::Medium, 1_024),
    ] {
        let est = estimate(
            &one,
            prompt_path(0, 0, a, level),
            Some(PriceBasis::Rates(&rates)),
            Calibration::UNCALIBRATED,
        );
        assert_eq!(est.estimate.est_output_tokens, out, "{level}");
        assert_eq!(est.reasoning, Some(level));
        // Reasoning ist in der Ausgabe enthalten und wird zum Ausgabepreis gerechnet: 200 × 0,10 + out × 0,50 µUSD.
        assert_eq!(
            est.estimate.expected,
            Some(MicroUsd(
                (200 * 100_000 + out * 500_000).div_ceil(1_000_000)
            ))
        );
    }
    let est = estimate(
        &one,
        decisions_path(100),
        Some(PriceBasis::Rates(&rates)),
        Calibration::UNCALIBRATED,
    );
    assert_eq!(est.estimate.est_output_tokens, 0);

    // Ersatzweg (decision-w): eine `choice` mit 14 Werten → A = 288; gebündelt mit einer `predicate` → W = 16, A = 320.
    let w = defaults
        .answer_tokens(TaskType::Decision)
        .expect("A für decision");
    assert_eq!(w.tokens(Some(14)), 288);
    let bundled = [PlannedRequest {
        input_chars: 100,
        images: 0,
        entries: vec![entry(0, 14), entry(0, 2)],
    }];
    let est = estimate(
        &bundled,
        prompt_path(0, 0, w, ReasoningLevel::Off),
        None,
        Calibration::UNCALIBRATED,
    );
    assert_eq!(est.estimate.est_output_tokens, 320);
}

// spec: AI-012 [M4]
// acceptance: independent
// Kriterium: Der Voranschlag führt die effektive Stufe (z. B. `low` für `suggest-title` auf Modell B, das `off` nicht
// kennt); die Anzeige der Spalte `Reasoning` baut M4-U01.
#[test]
fn estimate_carries_the_effective_level() {
    let b_levels = [
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
        ReasoningLevel::XHigh,
    ];
    let effective = resolve_reasoning_level(ReasoningLevel::Off, &b_levels);
    assert_eq!(effective, ReasoningLevel::Low);
    let one = [PlannedRequest {
        input_chars: 10,
        images: 0,
        entries: vec![entry(0, 0)],
    }];
    let est = estimate(
        &one,
        prompt_path(0, 0, AnswerTokens::Fixed(512), effective),
        None,
        Calibration::UNCALIBRATED,
    );
    assert_eq!(est.reasoning, Some(ReasoningLevel::Low));
    assert_eq!(est.estimate.est_output_tokens, 512 + 128);
}

// spec: AI-012 [M4]
// acceptance: independent
// Kriterium: Nach 20 Requests einer Aufgabe mit Mock-Szenario `reasoning=500` bei Stufe `medium` ist der
// Ausgabe-Korrekturfaktor dieser (Modell, Aufgabe, Stufe) gesetzt und die Schätzung für den nächsten Request liegt
// höchstens ±5 % neben dem Mock-Ist-Wert; der Faktor einer anderen Stufe bleibt unberührt.
#[tokio::test]
async fn output_calibration_per_model_task_and_level() {
    let mock = Mock::start().await;
    let a = anthropic_model(&mock, "A").await;
    let adapter = mock.anthropic();
    let dir = TempDir::new("calibration");
    let file = crawl_file(&dir.0);
    let raw = raw_output_tokens(512, ReasoningLevel::Medium);
    assert_eq!(raw, 1_024);
    let send = |i: usize| {
        request(
            model("anthropic", &a.id, "messages", Vec::new()),
            &format!("K10 calibration {i} MOCK-SCENARIO:reasoning=500"),
            None,
            ReasoningLevel::Medium,
            Some(8_192),
        )
    };
    for i in 0..20 {
        let reply = adapter.send(&send(i), None).await.expect("Mock antwortet");
        assert!(!reply.is_output_limit());
        assert_eq!(reply.usage.reasoning_tokens, Some(500));
        let row = detailed_request_record(
            &ctx("suggest-title", "anthropic", &a.id, "messages"),
            &claude_rates(),
            MicroUsd(1_000_000),
            &RequestOutcome::success(reply.usage.clone(), 0, 5),
            &OutcomeDetails {
                reasoning: Some(ReasoningLevel::Medium),
                reported_cost: None,
            },
        );
        os_store::ai::requests::insert_with_details(file.conn(), &row.record, &row.details)
            .unwrap();
        let cal = output_calibration_from_log(
            &file,
            "anthropic",
            &a.id,
            "suggest-title",
            Some(ReasoningLevel::Medium),
            raw,
        )
        .unwrap();
        assert_eq!(cal.calibrated, i + 1 >= 20, "nach {} Requests", i + 1);
    }
    let cal = output_calibration_from_log(
        &file,
        "anthropic",
        &a.id,
        "suggest-title",
        Some(ReasoningLevel::Medium),
        raw,
    )
    .unwrap();
    assert!(cal.calibrated);
    let next = adapter.send(&send(20), None).await.unwrap();
    let actual = next.usage.output_tokens.expect("Ausgabe-Tokens") as f64;
    let est = estimate_output_tokens(512, ReasoningLevel::Medium, &cal) as f64;
    assert!(
        (est - actual).abs() <= actual * 0.05,
        "Schätzung {est} vs. Ist {actual}"
    );

    for (task, level) in [
        ("suggest-title", ReasoningLevel::Low),
        ("suggest-title", ReasoningLevel::Off),
        ("suggest-meta", ReasoningLevel::Medium),
    ] {
        let other = output_calibration_from_log(
            &file,
            "anthropic",
            &a.id,
            task,
            Some(level),
            raw_output_tokens(512, level),
        )
        .unwrap();
        assert!(!other.calibrated, "{task} {level} bleibt unkalibriert");
    }
}

// spec: AI-012 [M4]
// acceptance: independent
// Kriterium: Schema-Aufgabe über das Anthropic-Mock-Modell: Die unkalibrierte Eingabeschätzung enthält +350 Tokens
// gegenüber derselben Aufgabe ohne Schema. Zuschläge laut Nachtrag: OpenAI Responses 80, OpenRouter `openai/*` 80, sonst
// 0.
#[test]
fn schema_surcharge_per_provider() {
    let limits = ProviderLimitsFile::bundled().unwrap();
    let cases = [
        ("anthropic", Endpoint::Messages, "mock-claude", 350u64),
        ("anthropic", Endpoint::Messages, "claude-haiku-5-5", 350),
        ("openai", Endpoint::Responses, "gpt-6-luna", 80),
        (
            "openrouter",
            Endpoint::ChatCompletions,
            "openai/gpt-6-luna",
            80,
        ),
        (
            "openrouter",
            Endpoint::ChatCompletions,
            "anthropic/claude-haiku-5-5",
            0,
        ),
        ("ollama", Endpoint::ChatCompletions, "mock-llama", 0),
    ];
    for (provider, endpoint, model, surcharge) in cases {
        assert_eq!(
            limits.schema_overhead(provider, endpoint, model),
            surcharge,
            "{provider} {model}"
        );
    }

    let rates = claude_rates();
    let one = [PlannedRequest {
        input_chars: 1_400,
        images: 0,
        entries: vec![entry(0, 0)],
    }];
    let with = estimate(
        &one,
        prompt_path(
            50,
            limits.schema_overhead("anthropic", Endpoint::Messages, "mock-claude"),
            AnswerTokens::Fixed(512),
            ReasoningLevel::Off,
        ),
        Some(PriceBasis::Rates(&rates)),
        Calibration::UNCALIBRATED,
    );
    let without = estimate(
        &one,
        prompt_path(50, 0, AnswerTokens::Fixed(512), ReasoningLevel::Off),
        Some(PriceBasis::Rates(&rates)),
        Calibration::UNCALIBRATED,
    );
    assert_eq!(
        with.estimate.est_input_tokens - without.estimate.est_input_tokens,
        350
    );
    assert_eq!(without.estimate.est_input_tokens, 400 + 50);
}

// spec: AI-010 [M4]
// acceptance: independent
// Kriterium (Nachtrag 25.11.2026): Decisions-Request mit 1.000.000 Eingabe-Tokens, davon 400.000 aus dem Cache gelesen
// → 600.000 × 0,10 + 400.000 × 0,01 je 1 Mio. = 0,064 USD vor Langkontext-Faktor und mit dem Faktor des
// `responses`-Eintrags 0,128 USD – nicht 0,10 USD pauschal. Die Schwelle zählt alle Eingabe-Tokens einschließlich der
// gecachten; oberhalb der Schwelle wirkt der Eingabe-Faktor, die Ausgabe bleibt 0.
#[test]
fn decisions_price_example_from_ai_010() {
    let rates = luna(Endpoint::Decisions);
    let tokens = TokenCounts {
        input_tokens: 1_000_000,
        cached_input_tokens: 400_000,
        cache_write_tokens: 0,
        output_tokens: 0,
    };
    let without_lc = Rates {
        long_context: None,
        ..rates.clone()
    };
    assert_eq!(micro(&tokens, &without_lc), 64_000);
    assert_eq!(micro(&tokens, &rates), 128_000);

    // Schwelle über alle Eingabe-Tokens: 300.000 davon 200.000 gecacht (ungecacht nur 100.000) → Langkontext.
    let over = TokenCounts {
        input_tokens: 300_000,
        cached_input_tokens: 200_000,
        cache_write_tokens: 0,
        output_tokens: 0,
    };
    assert_eq!(micro(&over, &rates), 100_000 * 2 / 10 + 200_000 * 2 / 100);
    let under = TokenCounts {
        input_tokens: 271_999,
        cached_input_tokens: 200_000,
        cache_write_tokens: 0,
        output_tokens: 0,
    };
    assert_eq!(
        micro(&under, &rates),
        9_200,
        "71.999 × 0,10 + 200.000 × 0,01 = 9.199,9 µUSD"
    );

    // Oberhalb der Schwelle: Eingabe-Faktor des responses-Eintrags, Ausgabe 0.
    let responses = luna(Endpoint::Responses);
    assert_eq!(
        rates
            .long_context
            .as_ref()
            .map(|l| (l.threshold_input_tokens, l.input_factor_ppm)),
        responses
            .long_context
            .as_ref()
            .map(|l| (l.threshold_input_tokens, l.input_factor_ppm))
    );
    assert_eq!(micro(&TokenCounts::plain(300_000, 1_000), &rates), 60_000);
}

// spec: AI-010 [M4]
// spec: AI-004 [M4]
// acceptance: independent
// Dasselbe Rechenbeispiel auf dem Protokollweg der Decisions API (`decisions_billing_rates`, Grundlage der Ist-Kosten
// laut Adapter): AI-004 verweist seit 25.11.2026 auf AI-010 – Cache-Lesen kostet 0,01, nicht den Eingabepreis.
#[test]
fn decisions_billing_rates_follow_the_ai_010_prices() {
    let billing = decisions_billing_rates(&luna(Endpoint::Decisions));
    let tokens = TokenCounts {
        input_tokens: 1_000_000,
        cached_input_tokens: 400_000,
        cache_write_tokens: 0,
        output_tokens: 0,
    };
    assert_eq!(
        micro(&tokens, &billing),
        128_000,
        "Ist-Kosten der Decisions API mit Cache-Lesen 0,01 und Langkontext-Faktor (AI-010, Nachtrag 25.11.2026)"
    );
    assert_eq!(micro(&TokenCounts::plain(300_000, 1_000), &billing), 60_000);
}

// spec: AI-013 [M4]
// spec: AI-011 [M4]
// acceptance: independent
// Kriterium: `suggest-title` auf Anthropic-Mock-Modell B (effektive Stufe `low`, `max_tokens = 2048`, `mock-claude`-Preise
// 1/5 USD je 1 Mio.) mit geschätzten 400 Eingabe-Tokens → Reservierung = 400 × 1,25 × 1 + 2.048 × 5 = 10.740 µUSD.
#[tokio::test]
async fn reservation_uses_max_tokens_from_ai_011() {
    let mock = Mock::start().await;
    let b = anthropic_model(&mock, "B").await;
    assert_eq!(b.max_tokens, Some(4_000), "Mock-Modell B");
    let b_levels = [
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
        ReasoningLevel::XHigh,
    ];
    let effective = resolve_reasoning_level(ReasoningLevel::Off, &b_levels);
    let max_tokens = default_max_tokens(512, effective, b.max_tokens);
    assert_eq!(max_tokens, 2_048);
    let rates = claude_rates();
    assert_eq!(
        reservation_for(PriceBasis::Rates(&rates), 400, max_tokens, 0, false),
        Some(MicroUsd(10_740))
    );
}

// spec: AI-013 [M4]
// spec: AI-005 [M4]
// acceptance: independent
// OpenRouter in der Reservierung (Nachtrag): Langkontext über `pricing.overrides`, sobald geschätzt × 1,25 das kleinste
// `min_prompt_tokens` erreicht (M-LC: ab 1.000 → 2/4 µUSD je Token statt 1/2); Ausgabepreis bei `internal_reasoning` =
// max(`completion`, `internal_reasoning`) – Kriterium AI-005: `M-IR` mit `max_tokens = 1.000` reserviert für die Ausgabe
// 1.000 × 10 µUSD = 10.000 µUSD (nicht 2.000).
#[tokio::test]
async fn reservation_with_openrouter_overrides_and_internal_reasoning() {
    let mock = Mock::start().await;
    let roles = openrouter_roles(&mock).await;
    let lc_raw = raw_entry(&roles, &roles.lc.id)["pricing"].clone();
    assert_eq!(lc_raw["prompt"], "0.000001");
    assert_eq!(lc_raw["completion"], "0.000002");
    let lc_fee = roles.lc.request_pico / 1_000_000;
    // ⌈800 × 1,25⌉ = 1.000 ≥ 1.000 → overrides: 1.000 × 2 + 100 × 4 = 2.400 µUSD.
    assert_eq!(
        reservation_for(PriceBasis::OpenRouter(&roles.lc), 800, 100, 0, false),
        Some(MicroUsd(2_400 + lc_fee))
    );
    // ⌈799 × 1,25⌉ = 999 < 1.000 → Grundpreise: 999 × 1 + 100 × 2 = 1.199 µUSD.
    assert_eq!(
        reservation_for(PriceBasis::OpenRouter(&roles.lc), 799, 100, 0, false),
        Some(MicroUsd(1_199 + lc_fee))
    );

    let ir_raw = raw_entry(&roles, &roles.ir.id)["pricing"].clone();
    assert_eq!(ir_raw["completion"], "0.000002");
    assert_eq!(ir_raw["internal_reasoning"], "0.000010");
    let ir_fee = roles.ir.request_pico / 1_000_000;
    assert_eq!(
        reservation_for(PriceBasis::OpenRouter(&roles.ir), 0, 1_000, 0, false),
        Some(MicroUsd(10_000 + ir_fee))
    );
    // Voranschlag ebenso vorsichtig: 512 Ausgabe-Tokens × 10 µUSD.
    let one = [PlannedRequest {
        input_chars: 0,
        images: 0,
        entries: vec![entry(0, 0)],
    }];
    let est = estimate(
        &one,
        prompt_path(0, 0, AnswerTokens::Fixed(512), ReasoningLevel::Off),
        Some(PriceBasis::OpenRouter(&roles.ir)),
        Calibration::UNCALIBRATED,
    );
    assert_eq!(est.estimate.expected, Some(MicroUsd(5_120 + ir_fee)));
}

// spec: AI-013 [M4]
// acceptance: independent
// Reservierung gegen das Budget: passt die Reservierung genau in das Restbudget, wird der Request gesendet; 1 µUSD
// weniger Budget → `Budget exceeded`; eine Ist-Antwort unter der Reservierung gibt den Rest wieder frei.
#[test]
fn reservation_against_the_budget_at_the_edge() {
    let rates = claude_rates();
    let res = reservation_for(PriceBasis::Rates(&rates), 400, 2_048, 0, false).unwrap();
    assert_eq!(res, MicroUsd(10_740));
    let scope = || BudgetScope::task(AREA_AI, "suggest-title");
    let limits = |crawl: u64| BudgetLimits {
        crawl: MicroUsd(crawl),
        ..BudgetLimits::default()
    };

    let tight = CrawlBudgetLedger::new(limits(10_739));
    let err = tight.reserve(scope(), res).expect_err("passt nicht");
    assert!(matches!(err, BudgetError::Exceeded(_)), "{err:?}");
    assert_eq!(status_for(&err), Some(AiStatus::BudgetExceeded));

    let exact = CrawlBudgetLedger::new(limits(10_740));
    let r = exact.reserve(scope(), res).expect("passt genau");
    assert!(
        exact.reserve(scope(), MicroUsd(1)).is_err(),
        "Budget ist reserviert"
    );
    let actual = cost_of(
        &TokenCounts::plain(400, 300),
        FeeCounts::one_request(0),
        &rates,
        false,
    );
    let settled = exact.settle(r.id, actual.cost).unwrap();
    assert!(!settled.over_reservation);
    exact
        .reserve(scope(), MicroUsd(10_740 - actual.cost.0))
        .expect("Rest wieder frei");
}

// spec: AI-013 [M4]
// spec: AI-005 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Kriterium: OpenRouter-Szenario `cost-mismatch` mit einer Reservierung kleiner als der gemeldete Betrag →
// `over_reservation = true`, der gemeldete Betrag ist verbucht, danach startet kein weiterer Request.
#[tokio::test]
async fn reported_cost_over_the_reservation_halts_the_budget() {
    let mock = Mock::start().await;
    let roles = openrouter_roles(&mock).await;
    let m = &roles.r3;
    let reply = mock
        .openrouter()
        .send(
            &request(
                model("openrouter", &m.id, "chat-completions", Vec::new()),
                "K10 halt MOCK-SCENARIO:cost-mismatch",
                None,
                ReasoningLevel::ModelDefault,
                Some(256),
            ),
            None,
        )
        .await
        .expect("Mock antwortet");
    let reported = reply
        .reported_cost
        .and_then(|c| c.usable())
        .expect("usage.cost gemeldet");
    let reservation = reservation_for(PriceBasis::OpenRouter(m), 1, 1, 0, false).unwrap();
    assert!(
        reservation.0 < reported,
        "Testaufbau: Reservierung {reservation:?} < gemeldet {reported}"
    );

    let dir = TempDir::new("halt");
    let file = crawl_file(&dir.0);
    let limits = BudgetLimits {
        crawl: MicroUsd(1_000_000),
        ..BudgetLimits::default()
    };
    let ledger = os_store::ai::budget::open_ledger(file.conn(), limits.clone(), &[]).unwrap();
    let r = ledger
        .reserve(BudgetScope::task(AREA_AI, "suggest-title"), reservation)
        .unwrap();
    let rates = PriceBasis::OpenRouter(m)
        .actual_rates(reply.usage.input_tokens.unwrap_or(0))
        .unwrap();
    let row = detailed_request_record(
        &ctx("suggest-title", "openrouter", &m.id, "chat-completions"),
        &rates,
        reservation,
        &RequestOutcome::success(reply.usage.clone(), 0, 5),
        &OutcomeDetails {
            reasoning: Some(ReasoningLevel::ModelDefault),
            reported_cost: reply.reported_cost,
        },
    );
    assert_eq!(
        row.record.cost,
        MicroUsd(reported),
        "gemeldeter Betrag ist Cost"
    );
    assert!(
        row.record.over_reservation,
        "over_reservation gegen den gemeldeten Betrag"
    );
    assert!(row.details.cost_mismatch);
    let settled = ledger.settle(r.id, row.record.cost).unwrap();
    assert!(settled.over_reservation);
    assert!(matches!(
        ledger.reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(1)),
        Err(BudgetError::Exceeded(BudgetLevel::Halted))
    ));

    // Nach Neuöffnen aus der Crawl-Datei: Verbrauch = gemeldeter Betrag, weiterhin gesperrt.
    os_store::ai::requests::insert_with_details(file.conn(), &row.record, &row.details).unwrap();
    assert_eq!(
        os_store::ai::requests::total_cost(file.conn()).unwrap(),
        MicroUsd(reported)
    );
    let reopened = os_store::ai::budget::open_ledger(file.conn(), limits, &[]).unwrap();
    assert!(matches!(
        reopened.reserve(BudgetScope::task(AREA_AI, "decision"), MicroUsd(1)),
        Err(BudgetError::Exceeded(BudgetLevel::Halted))
    ));
}

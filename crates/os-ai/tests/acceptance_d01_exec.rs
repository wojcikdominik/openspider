//! Unabhängige Abnahme M4-D01 – Ausführung über die Decisions API (AI-033) und übergreifende Pflichten des ersten
//! echten Aufrufers: Statuswerte und Fehlerfälle (AI-020, AI-038), Protokollzeile je Request (AI-014), Budget nur über
//! den Ledger (AI-013), Voranschlag je Frage (AI-012 Nachtrag), Schutzschalter neutral (AI-017 Nachtrag),
//! Einwilligung als Vorbedingung (AI-018). Erwartungen nur aus `docs/spec/08-ai.md`.
// acceptance: independent
mod acceptance_d01_support;

use std::collections::BTreeSet;

use acceptance_d01_support as s;
use os_ai::adapters::openai_decisions::MaxQuestionsPerRequest;
use os_ai::catalog::Endpoint;
use os_ai::cost::PriceSource;
use os_ai::decisions::exec::{ActiveQuestion, RunItem, RunOptions};
use os_ai::decisions::routing::RoutingSetting;
use os_ai::policy::{ConsentBook, ConsentGate};
use os_ai::PreviewSwitch;
use serde_json::json;

fn many_predicates(n: usize) -> Vec<ActiveQuestion> {
    (1..=n)
        .map(|i| {
            s::active(s::predicate(
                &format!("q{i:02}"),
                &format!("Question {i:02}"),
                &["title"],
            ))
        })
        .collect()
}

fn two_pages() -> Vec<RunItem> {
    vec![
        s::page(1, "First page title", "First."),
        s::page(2, "Second page title", "Second."),
    ]
}

fn with_max(max: u8) -> RunOptions {
    RunOptions {
        max_questions: MaxQuestionsPerRequest::new(max).unwrap(),
        ..RunOptions::default()
    }
}

/// Fragennamen je Request einer URL (erkannt am Title).
fn requests_for(bench: &s::Bench, title: &str) -> Vec<Vec<String>> {
    bench
        .mock
        .bodies(s::DECISIONS)
        .iter()
        .filter(|b| s::decision_input_text(b).contains(title))
        .map(s::decision_names)
        .collect()
}

// spec: AI-033 [M4]
// acceptance: independent
#[tokio::test]
async fn ai033_bundling_request_counts() {
    for (n, expected) in [
        (5usize, vec![5usize]),
        (10, vec![10]),
        (11, vec![10, 1]),
        (12, vec![10, 2]),
        (21, vec![10, 10, 1]),
    ] {
        let bench = s::Bench::new(&["openai"], 1).await;
        let questions = many_predicates(n);
        bench.run(&questions, &two_pages(), s::options()).await;
        assert_eq!(
            bench.mock.posts(s::DECISIONS).len(),
            2 * expected.len(),
            "{n} Fragen: Requests insgesamt"
        );
        for title in ["First page title", "Second page title"] {
            let reqs = requests_for(&bench, title);
            let mut sizes: Vec<usize> = reqs.iter().map(Vec::len).collect();
            sizes.sort_by(|a, b| b.cmp(a));
            assert_eq!(sizes, expected, "{n} Fragen, {title}");
            let all: Vec<String> = reqs.into_iter().flatten().collect();
            let unique: BTreeSet<_> = all.iter().cloned().collect();
            assert_eq!(all.len(), n, "jede Frage genau einmal ({n}, {title})");
            assert_eq!(unique.len(), n);
        }
        for q in &questions {
            assert_eq!(bench.count(&q.def.id, "Success"), 2);
        }
    }
}

// spec: AI-033 [M4]
// acceptance: independent
#[tokio::test]
async fn ai033_property_no_request_above_max_1_to_40_questions() {
    let mut cases: Vec<(usize, u8)> = (1..=40).map(|n| (n, 10)).collect();
    for n in [1usize, 4, 13, 40] {
        for max in [1u8, 3, 7] {
            cases.push((n, max));
        }
    }
    for (n, max) in cases {
        let bench = s::Bench::new(&["openai"], 1).await;
        let questions = many_predicates(n);
        let items = [s::page(1, "Only page", "Text.")];
        bench.run(&questions, &items, with_max(max)).await;
        let bodies = bench.mock.bodies(s::DECISIONS);
        assert_eq!(
            bodies.len(),
            n.div_ceil(usize::from(max)),
            "{n} Fragen, max {max}"
        );
        let mut seen = Vec::new();
        for b in &bodies {
            let names = s::decision_names(b);
            assert!(
                names.len() <= usize::from(max) && names.len() <= 10,
                "{} Fragen in einem Request (max {max})",
                names.len()
            );
            seen.extend(names);
        }
        seen.sort();
        let mut expected: Vec<String> = questions.iter().map(|q| q.def.wire_name()).collect();
        expected.sort();
        assert_eq!(seen, expected, "{n} Fragen, max {max}: jede genau einmal");
    }
}

// spec: AI-033 [M4]
// acceptance: independent
#[test]
fn ai033_max_questions_setting_1_to_10() {
    assert_eq!(MaxQuestionsPerRequest::default().get(), 10);
    assert!(MaxQuestionsPerRequest::new(0).is_none());
    assert!(MaxQuestionsPerRequest::new(11).is_none());
    assert_eq!(MaxQuestionsPerRequest::new(10).unwrap().get(), 10);
    assert_eq!(MaxQuestionsPerRequest::new(1).unwrap().get(), 1);
    // Profil bzw. JSON-Ansicht: Werte über 10 werden mit Fehler abgelehnt.
    assert!(serde_json::from_value::<MaxQuestionsPerRequest>(json!(11)).is_err());
    assert!(serde_json::from_value::<MaxQuestionsPerRequest>(json!(0)).is_err());
    assert_eq!(
        serde_json::from_value::<MaxQuestionsPerRequest>(json!(7))
            .unwrap()
            .get(),
        7
    );
}

// spec: AI-033 [M4]
// acceptance: independent
#[tokio::test]
async fn ai033_values_exactly_as_mock_product_page() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let items = [s::page(1, "Shoe MOCK-ID:d01t-product", "A product page.")];
    bench
        .run(&[s::active(s::page_type(&["title"]))], &items, s::options())
        .await;
    let row = bench.answer(1, "page-type");
    assert_eq!(row.status, "Success");
    assert_eq!(row.source.as_deref(), Some("decisions-api"));
    assert_eq!(row.model.as_deref(), Some("gpt-6-luna"));
    let answer = s::answer_json(&row);
    assert_eq!(answer["choice"], "product");
    assert!(s::approx(answer["confidence"].as_f64().unwrap(), 0.93));
    assert_eq!(answer["probabilities"].as_array().unwrap().len(), 14);
    assert!(s::approx(s::max_probability(&answer), 0.93));
    // Wahrscheinlichkeiten unverändert übernommen.
    let first = &answer["probabilities"].as_array().unwrap()[0];
    assert_eq!(first["value"], "product");
    assert!(s::approx(first["probability"].as_f64().unwrap(), 0.93));
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 1);
    // Gesendeter Fragenname aus der id abgeleitet.
    assert_eq!(
        s::decision_names(&bench.mock.bodies(s::DECISIONS)[0]),
        vec!["page_type".to_string()]
    );
}

// spec: AI-033 [M4]
// acceptance: independent
#[tokio::test]
async fn ai033_score_and_predicate_values_unchanged() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let sc = s::score(
        "d01t-score-q",
        "Score Q",
        &["l0", "l1", "l2", "l3", "l4"],
        &["title"],
    );
    let pr = s::predicate("d01t-pred-q", "Pred Q", &["title"]);
    let items = [
        s::page(1, "Scored MOCK-ID:d01t-score", "Text."),
        s::page(2, "Predicated MOCK-ID:d01t-pred", "Text."),
    ];
    bench.run(&[s::active(sc)], &items[..1], s::options()).await;
    bench.run(&[s::active(pr)], &items[1..], s::options()).await;
    let score = s::answer_json(&bench.answer(1, "d01t-score-q"));
    assert!(s::approx(score["score"].as_f64().unwrap(), 2.0));
    assert_eq!(score["level"], "l2");
    assert!(s::approx(score["confidence"].as_f64().unwrap(), 0.4));
    let pred = s::answer_json(&bench.answer(2, "d01t-pred-q"));
    assert!(s::approx(pred["probability"].as_f64().unwrap(), 0.4999));
}

// spec: AI-033 [M4]
// acceptance: independent
#[tokio::test]
async fn ai033_probability_sum_off_by_more_than_001_is_normalized() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let q = s::choice("d01t-norm-q", "Norm Q", &["a", "b", "c"], &["title"]);
    let items = [
        s::page(1, "Sum 1.03 MOCK-ID:d01t-norm", "Text."),
        s::page(2, "Sum 1.005 MOCK-ID:d01t-nonorm", "Text."),
    ];
    bench.run(&[s::active(q)], &items, s::options()).await;
    let normalized = s::answer_json(&bench.answer(1, "d01t-norm-q"));
    assert!(
        (s::probability_sum(&normalized) - 1.0).abs() < 1e-9,
        "Summe 1,03 nicht normiert: {normalized}"
    );
    assert_eq!(normalized["choice"], "a");
    assert!(s::approx(
        normalized["confidence"].as_f64().unwrap(),
        0.63 / 1.03
    ));
    assert_eq!(
        normalized.get("normalized").and_then(|v| v.as_bool()),
        Some(true),
        "Normierung nicht vermerkt (normalized = true)"
    );
    let untouched = s::answer_json(&bench.answer(2, "d01t-norm-q"));
    assert!(s::approx(s::probability_sum(&untouched), 1.005));
    assert_ne!(
        untouched.get("normalized").and_then(|v| v.as_bool()),
        Some(true)
    );
}

// spec: AI-004 [M4]
// acceptance: independent
#[tokio::test]
async fn ai004_confidence_mismatch_is_logged() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let q = s::choice("d01t-conf-q", "Conf Q", &["a", "b", "c"], &["title"]);
    let items = [
        s::page(1, "Raw 0.80 MOCK-ID:d01t-mismatch", "Text."),
        s::page(2, "Raw 0.925 MOCK-ID:d01t-match", "Text."),
    ];
    bench.run(&[s::active(q)], &items, s::options()).await;
    let mismatch = bench.answer(1, "d01t-conf-q");
    let matching = bench.answer(2, "d01t-conf-q");
    // Confidence = höchste Einzelwahrscheinlichkeit; geliefertes confidence roh gespeichert.
    assert!(s::approx(
        s::answer_json(&mismatch)["confidence"].as_f64().unwrap(),
        0.93
    ));
    assert!(s::approx(mismatch.raw_confidence.unwrap(), 0.80));
    assert!(s::approx(matching.raw_confidence.unwrap(), 0.925));
    let rows = bench.sent_rows();
    let flag = |url: i64| {
        rows.iter()
            .find(|r| r.record.url_id == Some(url))
            .unwrap_or_else(|| panic!("keine Protokollzeile für URL {url}"))
            .record
            .confidence_mismatch
    };
    assert!(
        flag(1),
        "Abweichung 0,13 nicht als confidence_mismatch vermerkt"
    );
    assert!(!flag(2), "Abweichung 0,005 fälschlich vermerkt");
}

// spec: AI-038 [M4]
// acceptance: independent
#[tokio::test]
async fn ai038_refusal_affects_only_its_question() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let questions = [
        s::active(s::predicate("d01t-r1", "R1", &["title"])),
        s::active(s::predicate("d01t-r2", "R2", &["title"])),
        s::active(s::predicate("d01t-r3", "R3", &["title"])),
    ];
    let items = [s::page(1, "Refusal MOCK-ID:d01t-refusal-mid", "Text.")];
    bench.run(&questions, &items, s::options()).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 1);
    assert_eq!(bench.status(1, "d01t-r1"), "Success");
    assert_eq!(bench.status(1, "d01t-r2"), "Refusal");
    assert_eq!(bench.status(1, "d01t-r3"), "Success");
    // Wertespalten leer (AI-036/AI-038): kein Wert und keine Wahrscheinlichkeit im Ergebnis der Frage.
    if let Some(json) = bench.answer(1, "d01t-r2").answer_json {
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        for key in ["probability", "probabilities", "choice", "score"] {
            assert!(
                v.get(key).is_none_or(|x| x.is_null()),
                "Refusal mit Wert {key}: {v}"
            );
        }
    }
    assert!(s::approx(
        s::answer_json(&bench.answer(1, "d01t-r3"))["probability"]
            .as_f64()
            .unwrap(),
        0.2
    ));
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_missing_answer_in_multi_question_response() {
    let bench = s::Bench::new(&["openai"], 1).await;
    bench.mock.scenario("openai", "missing-answer", 1).await;
    let questions = many_predicates(3);
    let items = [s::page(1, "Missing", "Text.")];
    bench.run(&questions, &items, s::options()).await;
    let statuses: Vec<String> = ["q01", "q02", "q03"]
        .iter()
        .map(|q| bench.status(1, q))
        .collect();
    assert_eq!(
        statuses
            .iter()
            .filter(|x| *x == "Error: Missing answer")
            .count(),
        1,
        "{statuses:?}"
    );
    assert_eq!(statuses.iter().filter(|x| *x == "Success").count(), 2);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_missing_key_22_errors_zero_requests() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    let items = s::site(|i| format!("Page {i}."));
    let q = s::active_with(s::page_type(&["title"]), RoutingSetting::DecisionsApi);
    bench.run(&[q], &items, s::options()).await;
    assert_eq!(bench.count("page-type", "Error: Missing API key"), 22);
    assert_eq!(bench.count("page-type", "Skipped (scope)"), 3);
    assert_eq!(
        bench.mock.provider_posts(),
        0,
        "kein Request, kein stiller Ersatzweg"
    );
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_quota_exhausted_one_request_all_quota_exceeded() {
    let bench = s::Bench::new(&["openai"], 1).await;
    bench
        .mock
        .scenario("openai", "insufficient-quota", 100)
        .await;
    let items = s::site(|i| format!("Page {i}."));
    bench
        .run(&[s::active(s::page_type(&["title"]))], &items, s::options())
        .await;
    assert_eq!(
        bench.mock.posts(s::DECISIONS).len(),
        1,
        "gleichzeitige Anfragen = 1"
    );
    assert_eq!(bench.count("page-type", "Quota exceeded"), 22);
    assert_eq!(bench.count("page-type", "Skipped (scope)"), 3);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_anthropic_spend_cap_429_is_quota_exceeded() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    bench.mock.scenario("anthropic", "429-spend-cap", 100).await;
    let items: Vec<_> = (1..=5)
        .map(|i| s::page(i, &format!("Cap {i} MOCK-ID:d01t-fb-choice"), "Text."))
        .collect();
    let q = s::choice(
        "d01t-fb-q",
        "Fallback Q",
        &["product", "category", "other", "article"],
        &["title"],
    );
    bench.run(&[s::active(q)], &items, s::options()).await;
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 1);
    assert_eq!(bench.count("d01t-fb-q", "Quota exceeded"), 5);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_5xx_is_retried_then_error_with_log_row() {
    let bench = s::Bench::new(&["openai"], 1).await;
    bench.mock.scenario("openai", "500-always", 6).await;
    let items = [s::page(1, "Server error", "Text.")];
    bench
        .run(&[s::active(s::page_type(&["title"]))], &items, s::options())
        .await;
    assert_eq!(
        bench.mock.posts(s::DECISIONS).len(),
        6,
        "1 + 5 Wiederholungen"
    );
    let status = bench.status(1, "page-type");
    assert!(status.starts_with("Error: 500"), "Status {status}");
    let rows = bench.sent_rows();
    assert_eq!(rows.len(), 1, "eine Protokollzeile für alle Versuche");
    assert_eq!(rows[0].record.retries, 5);
    assert_eq!(rows[0].record.http_status, Some(500));
    assert_eq!(rows[0].record.cost.0, 0);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_transient_5xx_then_success() {
    let bench = s::Bench::new(&["openai"], 1).await;
    bench.mock.scenario("openai", "500-always", 2).await;
    let items = [s::page(1, "Flaky", "Text.")];
    bench
        .run(&[s::active(s::page_type(&["title"]))], &items, s::options())
        .await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 3);
    assert_eq!(bench.status(1, "page-type"), "Success");
    let rows = bench.sent_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].record.retries, 2);
    assert_eq!(rows[0].record.http_status, Some(200));
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_unreadable_response_is_invalid_output() {
    let bench = s::Bench::new(&["openai"], 1).await;
    bench.mock.scenario("openai", "malformed-json", 10).await;
    let items = [s::page(1, "Broken", "Text.")];
    bench
        .run(&[s::active(s::page_type(&["title"]))], &items, s::options())
        .await;
    assert_eq!(bench.status(1, "page-type"), "Error: Invalid output");
    assert!(bench.answer(1, "page-type").answer_json.is_none());
}

// spec: AI-017 [M4]
// acceptance: independent
#[tokio::test]
async fn ai017_refusals_do_not_trip_the_breaker() {
    let bench = s::Bench::new(&["openai"], 1).await;
    bench.mock.scenario("openai", "refusal", 100).await;
    let items: Vec<_> = (1..=12)
        .map(|i| s::page(i, &format!("Refused {i}"), "Text."))
        .collect();
    bench
        .run(&[s::active(s::page_type(&["title"]))], &items, s::options())
        .await;
    assert_eq!(
        bench.mock.posts(s::DECISIONS).len(),
        12,
        "kein Backoff, kein Schutzschalter"
    );
    assert_eq!(bench.count("page-type", "Refusal"), 12);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_output_limit_22_errors_22_requests_breaker_neutral() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    bench.mock.scenario("anthropic", "output-limit", 1000).await;
    let items = s::site(|i| format!("Limited {i} MOCK-ID:d01t-fb-choice"));
    let q = s::active_with(
        s::choice(
            "d01t-fb-q",
            "Fallback Q",
            &["product", "category", "other", "article"],
            &["page_text"],
        ),
        s::structured("anthropic", "mock-claude", Endpoint::Messages),
    );
    bench.run(&[q], &items, s::options()).await;
    assert_eq!(bench.count("d01t-fb-q", "Error: Output limit reached"), 22);
    assert_eq!(bench.count("d01t-fb-q", "Skipped (scope)"), 3);
    assert_eq!(
        bench.mock.posts(s::MESSAGES).len(),
        22,
        "kein Reparaturversuch, Schutzschalter greift nicht"
    );
    for row in bench.sent_rows() {
        assert_eq!(row.record.http_status, Some(200));
        assert_eq!(row.record.error_code.as_deref(), Some("max_tokens"));
        assert!(row.record.cost.0 > 0, "gemeldete Tokens werden verbucht");
    }
}

// spec: AI-014 [M4]
// acceptance: independent
#[tokio::test]
async fn ai014_one_log_row_per_request_with_http_status() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let items = s::site(|i| format!("Content of page {i}."));
    bench
        .run(
            &[s::active(s::page_type(&["title", "page_text"]))],
            &items,
            s::options(),
        )
        .await;
    let bodies = bench.mock.bodies(s::DECISIONS);
    let rows = bench.sent_rows();
    assert_eq!(rows.len(), 22);
    assert_eq!(
        rows.len(),
        bodies.len(),
        "je Request genau eine Protokollzeile"
    );
    for row in &rows {
        let r = &row.record;
        assert_eq!(r.provider, "openai");
        assert_eq!(r.model, "gpt-6-luna");
        assert_eq!(r.endpoint, "decisions");
        assert_eq!(r.http_status, Some(200));
        assert_eq!(r.status, "Success");
        assert_eq!(r.price_source, PriceSource::Table);
        let url = r.url_id.expect("Address gefüllt");
        let body = bodies
            .iter()
            .find(|b| s::decision_input_text(b).contains(&format!("Content of page {url}.")))
            .expect("Request der URL im Mock-Log");
        let mock_tokens = os_testsite::mocks::ai::decisions_input_tokens(body);
        assert_eq!(r.input_tokens, Some(mock_tokens), "Input Tokens laut usage");
        // 0,10 USD je 1 Mio. Eingabe-Tokens, Ausgabe 0; auf µUSD gerundet.
        let expected = mock_tokens as f64 * 0.1;
        assert!(
            (r.cost.0 as f64 - expected).abs() <= 1.0,
            "Cost {} µUSD, erwartet ≈ {expected}",
            r.cost.0
        );
        assert_eq!(
            r.cost.0,
            r.token_cost.0 + r.fixed_fees.0 + r.regional_uplift.0,
            "Cost = Token Cost + Fixed Fees + Regional Uplift"
        );
    }
    assert_eq!(
        bench.spent(),
        bench.logged_cost(),
        "Ledger = Summe der Protokollzeilen"
    );
    assert_eq!(
        bench.reserved(),
        0,
        "keine offene Reservierung nach dem Lauf"
    );
}

// spec: AI-033 [M4]
// acceptance: independent
#[tokio::test]
async fn ai033_estimate_per_question_bundling_saves_only_requests() {
    let questions = [
        s::active(s::predicate(
            "e-one",
            "Estimate One",
            &["title", "page_text"],
        )),
        s::active(s::predicate(
            "e-two",
            "Estimate Two",
            &["title", "page_text"],
        )),
        s::active(s::choice(
            "e-three",
            "Estimate Three",
            &["alpha", "beta", "other"],
            &["title", "page_text"],
        )),
        s::active(s::score(
            "e-four",
            "Estimate Four",
            &["low", "mid", "high"],
            &["title", "page_text"],
        )),
        s::active(s::predicate(
            "e-five",
            "Estimate Five",
            &["title", "page_text"],
        )),
    ];
    let items: Vec<RunItem> = (1..=2)
        .map(|i| {
            s::page(
                i,
                &format!("Estimate page {i}"),
                &"Some longer body text with several words. ".repeat(8),
            )
        })
        .collect();
    let bundled = s::Bench::new(&["openai"], 1).await;
    let estimate = bundled.estimate(&questions, &items, s::options());
    let tokens: u64 = estimate
        .tasks
        .iter()
        .map(|t| t.estimate.est_input_tokens)
        .sum();
    let requests: u64 = estimate.tasks.iter().map(|t| t.requests).sum();
    let expected_cost: u64 = estimate
        .tasks
        .iter()
        .map(|t| t.estimate.expected.expect("Preis bekannt").0)
        .sum();
    assert_eq!(requests, 2, "5 Fragen gebündelt: 1 Request je URL");
    bundled.run(&questions, &items, s::options()).await;
    let bodies = bundled.mock.bodies(s::DECISIONS);
    assert_eq!(bodies.len(), 2);
    let spec_tokens: u64 = bodies.iter().map(s::spec_decisions_estimate).sum();
    assert_eq!(
        tokens, spec_tokens,
        "Est. Input Tokens = Σ je Frage [⌈Z_E/3,5⌉ + ⌈Z_q/3,5⌉ + 100]"
    );
    // Voranschlag gegen Ist-Kosten (unkalibriert höchstens ±25 %).
    let actual = bundled.logged_cost();
    assert!(actual > 0);
    let deviation = (expected_cost as f64 - actual as f64).abs() / actual as f64;
    assert!(
        deviation <= 0.25,
        "Voranschlag {expected_cost} µUSD gegen Ist {actual} µUSD ({:.1} %)",
        deviation * 100.0
    );
    // max. 1 Frage je Request: gleiche Tokenschätzung, 5-fache Requestzahl.
    let single = s::Bench::new(&["openai"], 1).await;
    let estimate1 = single.estimate(&questions, &items, with_max(1));
    let tokens1: u64 = estimate1
        .tasks
        .iter()
        .map(|t| t.estimate.est_input_tokens)
        .sum();
    let requests1: u64 = estimate1.tasks.iter().map(|t| t.requests).sum();
    assert_eq!(tokens1, tokens, "Bündelung ändert die Tokenschätzung nicht");
    assert_eq!(requests1, 10);
    let expected1: u64 = estimate1
        .tasks
        .iter()
        .map(|t| t.estimate.expected.unwrap().0)
        .sum();
    assert!(
        expected1.abs_diff(expected_cost) <= requests1,
        "Kosten weichen um mehr als 1 µUSD je Request ab: {expected1} gegen {expected_cost}"
    );
}

// spec: AI-033 [M4]
// acceptance: independent
#[tokio::test]
async fn ai033_image_questions_one_image_per_request() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let image_q = |id: &str, name: &str| {
        s::question(json!({
            "id": id,
            "version": "1.0.0",
            "name": name,
            "type": "predicate",
            "instructions": "Check the image.",
            "subject": "image",
            "inputs": ["image", "alt", "image_context"]
        }))
    };
    let questions = [
        s::active(image_q("img-a", "Image A")),
        s::active(image_q("img-b", "Image B")),
    ];
    let items: Vec<_> = (1..=3)
        .map(|i| s::image(i, s::png_variant(i as u8), &format!("Alt {i}"), "Caption"))
        .collect();
    bench.run(&questions, &items, s::options()).await;
    let bodies = bench.mock.bodies(s::DECISIONS);
    assert_eq!(
        bodies.len(),
        3,
        "ein Request je Bild; Bilder je Request {:?}, Status {:?}",
        bodies.iter().map(s::image_parts).collect::<Vec<_>>(),
        bench.status_counts("img-a")
    );
    for b in &bodies {
        assert_eq!(s::image_parts(b), 1, "genau ein Bild je Request");
        assert_eq!(s::decision_names(b).len(), 2);
        assert!(
            s::decision_input_text(b).contains("Caption"),
            "Kontextfeld fehlt"
        );
    }
    assert_eq!(bench.count("img-a", "Success"), 3);
}

// spec: AI-018 [M4]
// acceptance: independent
#[tokio::test]
async fn ai018_without_consent_zero_requests() {
    let mut bench = s::Bench::new(&["openai", "anthropic"], 1).await;
    bench.consent = ConsentGate::dialog(ConsentBook::new());
    let items = s::site(|i| format!("Page {i}."));
    bench
        .run(&[s::active(s::page_type(&["title"]))], &items, s::options())
        .await;
    assert_eq!(
        bench.mock.provider_posts(),
        0,
        "ohne Einwilligung kein Request"
    );
    assert_eq!(bench.count("page-type", "Pending (consent)"), 22);
    // Schalter „KI (Vorschau)“ aus: ebenfalls kein Request.
    let mut off = s::Bench::new(&["openai"], 1).await;
    off.switch = PreviewSwitch::OFF;
    off.run(&[s::active(s::page_type(&["title"]))], &items, s::options())
        .await;
    assert_eq!(off.mock.provider_posts(), 0);
}

// spec: AI-013 [M4]
// acceptance: independent
#[tokio::test]
async fn ai013_budget_never_exceeded_with_four_workers() {
    let mut bench = s::Bench::new(&["openai"], 4).await;
    bench.set_budget(600);
    let items: Vec<RunItem> = (1..=200)
        .map(|i| s::page(i, &format!("Budget item {i:03}"), "Text."))
        .collect();
    let q = s::predicate("budget-q", "Budget Q", &["title"]);
    bench
        .run(&[s::active(q)], &items, s::options_concurrent(4))
        .await;
    let rows = bench.sent_rows();
    let spent = bench.logged_cost();
    assert!(
        rows.iter().all(|r| !r.record.over_reservation),
        "Ist-Antwort über der Reservierung – Voraussetzung des Kriteriums verletzt"
    );
    assert!(
        spent <= 600,
        "Summe der Ist-Kosten {spent} µUSD > Budget 600 µUSD"
    );
    let success = bench.count("budget-q", "Success");
    let exceeded = bench.count("budget-q", "Budget exceeded");
    assert!(success > 0 && success < 200, "Erfolge {success}");
    assert_eq!(
        success + exceeded,
        200,
        "{:?}",
        bench.status_counts("budget-q")
    );
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), success);
    assert_eq!(rows.len(), success);
    assert_eq!(bench.reserved(), 0);
}

// spec: AI-014 [M4]
// acceptance: independent
#[tokio::test]
async fn ai014_bundled_request_is_one_row_with_all_versions() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let mut questions = many_predicates(5);
    for (i, q) in questions.iter_mut().enumerate() {
        q.def.version = os_ai::decisions::format::Version::new(1, 0, i as u64);
    }
    let items = [s::page(1, "Versions page", "Text.")];
    bench.run(&questions, &items, s::options()).await;
    let bodies = bench.mock.bodies(s::DECISIONS);
    assert_eq!(bodies.len(), 1);
    let rows = bench.sent_rows();
    assert_eq!(
        rows.len(),
        1,
        "ein gebündelter Request = eine Protokollzeile"
    );
    let versions: Vec<String> = rows[0]
        .record
        .definition_version
        .as_deref()
        .expect("Question/Prompt Version gefüllt")
        .split(',')
        .map(|v| v.trim().to_string())
        .collect();
    // Reihenfolge der Fragen im Request.
    let expected: Vec<String> = s::decision_names(&bodies[0])
        .iter()
        .map(|name| {
            questions
                .iter()
                .find(|q| &q.def.wire_name() == name)
                .unwrap()
                .def
                .version
                .to_string()
        })
        .collect();
    assert_eq!(versions, expected);
    assert_eq!(rows[0].record.url_id, Some(1), "Address = URL des Elements");
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_quota_lock_lasts_one_run_without_further_rows() {
    let mut bench = s::Bench::new(&["openai"], 1).await;
    bench.mock.scenario("openai", "insufficient-quota", 1).await;
    let items: Vec<RunItem> = (1..=5)
        .map(|i| s::page(i, &format!("Quota {i}"), "Text."))
        .collect();
    let qs = [s::active(s::page_type(&["title"]))];
    bench.run(&qs, &items, s::options()).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 1);
    assert_eq!(bench.count("page-type", "Quota exceeded"), 5);
    assert_eq!(
        bench.sent_rows().len(),
        1,
        "weitere Einträge ohne Request und ohne Protokollzeile"
    );
    // Ein neuer Lauf versucht es wieder (neuer Lauf = neue Adapter-Instanzen, Doku `Adapters`).
    bench.renew_adapters().await;
    let failed = RunOptions {
        selection: os_ai::decisions::exec::Selection::Failed,
        ..RunOptions::default()
    };
    bench.run(&qs, &items, failed).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 6);
    assert_eq!(bench.count("page-type", "Success"), 5);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_pending_consent_survives_and_is_sent_after_grant() {
    let mut bench = s::Bench::new(&["openai"], 1).await;
    bench.consent = ConsentGate::dialog(ConsentBook::new());
    let items: Vec<RunItem> = (1..=3)
        .map(|i| s::page(i, &format!("Consent {i}"), "Text."))
        .collect();
    let qs = [s::active(s::page_type(&["title"]))];
    bench.run(&qs, &items, s::options()).await;
    assert_eq!(bench.count("page-type", "Pending (consent)"), 3);
    // Fortsetzen ohne Einwilligung: bleiben Pending (consent), Mock-Log 0.
    bench.run(&qs, &items, s::options()).await;
    assert_eq!(bench.count("page-type", "Pending (consent)"), 3);
    assert_eq!(bench.mock.provider_posts(), 0);
    // Nach erteilter Einwilligung genau diese 3.
    bench.consent = ConsentGate::cli(true);
    bench.run(&qs, &items, s::options()).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 3);
    assert_eq!(bench.count("page-type", "Success"), 3);
}

// spec: AI-012 [M4]
// acceptance: independent
#[tokio::test]
async fn ai012_confirmation_threshold_is_inclusive() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let items: Vec<RunItem> = (1..=4)
        .map(|i| s::page(i, &format!("Threshold {i}"), "Some text for the estimate."))
        .collect();
    let qs = [s::active(s::page_type(&["title", "page_text"]))];
    let estimate = bench.estimate(&qs, &items, s::options());
    let expected = estimate.totals.expected.0;
    assert!(expected > 0);
    // Erwartete Kosten gleich der Schwelle → bestätigungspflichtig, kein Request.
    let at = RunOptions {
        confirmation: os_ai::decisions::exec::Confirmation::AskAbove(os_model::budget::MicroUsd(
            expected,
        )),
        ..RunOptions::default()
    };
    let report = bench.run(&qs, &items, at).await;
    assert_eq!(
        report.end,
        os_ai::decisions::exec::RunEnd::NeedsConfirmation
    );
    assert_eq!(bench.mock.provider_posts(), 0);
    assert_eq!(bench.count("page-type", "Pending (confirmation)"), 4);
    // Schwelle 1 µUSD darüber → kein Dialog, die Einträge gehen hinaus.
    let above = RunOptions {
        confirmation: os_ai::decisions::exec::Confirmation::AskAbove(os_model::budget::MicroUsd(
            expected + 1,
        )),
        ..RunOptions::default()
    };
    let report = bench.run(&qs, &items, above).await;
    assert_eq!(report.end, os_ai::decisions::exec::RunEnd::Completed);
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 4);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
#[ignore = "wartet auf M4-N01: Status `Skipped (price unknown)` (Auflage B8; D01 vergibt bis dahin `Skipped (no capable model)`)"]
async fn ai020_model_without_prices_is_skipped_price_unknown() {
    let mut bench = s::Bench::new(&["openai"], 1).await;
    bench.prices = s::prices_without_decisions;
    let items = s::site(|i| format!("Page {i}."));
    bench
        .run(&[s::active(s::page_type(&["title"]))], &items, s::options())
        .await;
    assert_eq!(bench.count("page-type", "Skipped (price unknown)"), 22);
    assert_eq!(bench.mock.provider_posts(), 0);
}

// spec: AI-033 [M4]
// acceptance: independent
#[tokio::test]
#[ignore = "wartet auf M4-N01: Spalte `normalized` in `ai_requests` und Kennzeichen in `Flags` (Auflage B7)"]
async fn ai033_normalization_is_flagged_in_the_request_log() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let q = s::choice("d01t-norm-q", "Norm Q", &["a", "b", "c"], &["title"]);
    let items = [
        s::page(1, "Sum 1.03 MOCK-ID:d01t-norm", "Text."),
        s::page(2, "Sum 1.005 MOCK-ID:d01t-nonorm", "Text."),
    ];
    bench.run(&[s::active(q)], &items, s::options()).await;
    let flag = |id: i64| -> i64 {
        bench
            .conn()
            .query_row(
                &format!("SELECT normalized FROM ai_requests WHERE url_id = {id}"),
                [],
                |r| r.get(0),
            )
            .expect("Spalte normalized")
    };
    assert_eq!(flag(1), 1);
    assert_eq!(flag(2), 0);
}

//! M4-K02: Adapter OpenAI Decisions API (AI-004) gegen ai-mock (`/openai/v1/decisions`).
#[path = "adapter_openai_support.rs"]
mod support;

use std::sync::Arc;

use os_ai::adapters::openai_decisions::{
    bundle, decisions_billing_rates, level_index, DecideOptions, DecisionAnswer, DecisionInput,
    MaxQuestionsPerRequest, OpenAiDecisions, Question, QuestionResult, DECISIONS_CAPABILITIES,
};
use os_ai::adapters::openai_responses::image::{
    encode_base64, ImageError, ImageSource, MAX_IMAGE_BYTES,
};
use os_ai::cost::{request_record, LogContext, PriceSource, Rates};
use os_ai::provider::{
    execute, AiProvider, Capabilities, CapabilitySource, Input, Outcome, Request,
};
use os_ai::schema::ResponseSchema;
use os_ai::{AiStatus, PreviewSwitch};
use os_model::budget::{BudgetLedger, BudgetLimits, BudgetScope, CrawlBudgetLedger, MicroUsd};
use os_testsite::mocks::ai::token_count;
use proptest::prelude::*;
use serde_json::{json, Value};
use support::{limiter, limiter_timeout, route_site, test_price, Mock, EU_HOST, GLOBAL_HOST};

const DECISIONS: &str = "/openai/v1/decisions";

/// Fragen in der Form aus AI-004 (je eine `predicate`, `choice`, `score`).
fn three_questions() -> Vec<Question> {
    vec![
        Question::predicate("q1_predicate", "Is the page a product detail page?"),
        Question::choice(
            "q2_choice",
            "Which page type fits best?",
            &[
                ("product", "A single product with price."),
                ("category", "A listing of several products."),
                ("other", "Anything else."),
            ],
        ),
        Question::score(
            "q3_score",
            "How helpful is the content?",
            &[
                ("a", "Not helpful."),
                ("b", "Somewhat helpful."),
                ("c", "Very helpful."),
            ],
        ),
    ]
}

fn levels_abc() -> [(&'static str, &'static str); 3] {
    [("a", "Low."), ("b", "Medium."), ("c", "High.")]
}

/// JSON-Schema des Requests nach AI-004 (Ersatz für die fehlende Fixture `tests/fixtures/ai/decisions-request.schema.json`).
fn request_schema() -> ResponseSchema {
    let text_part = json!({
        "type": "object",
        "required": ["type", "text"],
        "properties": {"type": {"const": "input_text"}, "text": {"type": "string"}},
        "additionalProperties": false
    });
    let image_part = json!({
        "type": "object",
        "required": ["type", "image_url"],
        "properties": {
            "type": {"const": "input_image"},
            "image_url": {"type": "string", "pattern": "^data:image/[a-z0-9.+-]+;base64,"}
        },
        "additionalProperties": false
    });
    let base = |kind: &str, extra: Value| {
        let mut properties = json!({
            "name": {"type": "string", "minLength": 1},
            "type": {"const": kind},
            "instructions": {"type": "string", "minLength": 1}
        });
        let mut required = vec![json!("name"), json!("type"), json!("instructions")];
        if let Value::Object(map) = extra {
            for (k, v) in map {
                required.push(json!(k));
                properties[k] = v;
            }
        }
        json!({"type": "object", "required": required, "properties": properties, "additionalProperties": false})
    };
    let entry = |a: &str, b: &str| {
        json!({
            "type": "object",
            "required": [a, b],
            "properties": {a: {"type": "string", "minLength": 1}, b: {"type": "string"}},
            "additionalProperties": false
        })
    };
    ResponseSchema::new(json!({
        "type": "object",
        "required": ["model", "input", "questions"],
        "additionalProperties": false,
        "properties": {
            "model": {"type": "string", "minLength": 1},
            "input": {"oneOf": [
                {"type": "string"},
                {"type": "array", "minItems": 1, "items": {
                    "type": "object",
                    "required": ["role", "content"],
                    "properties": {
                        "role": {"const": "user"},
                        "content": {"type": "array", "minItems": 1, "items": {"oneOf": [text_part, image_part]}}
                    },
                    "additionalProperties": false
                }}
            ]},
            "questions": {"type": "array", "minItems": 1, "maxItems": 10, "items": {"oneOf": [
                base("predicate", json!({})),
                base("choice", json!({"choices": {"type": "array", "minItems": 1, "items": entry("value", "description")}})),
                base("score", json!({"levels": {"type": "array", "minItems": 1, "items": entry("label", "description")}}))
            ]}}
        }
    }))
    .unwrap()
}

fn ctx() -> LogContext {
    LogContext {
        run_id: 1,
        timestamp: "2026-11-24T10:00:00Z".into(),
        task: "decision".into(),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: "decisions".into(),
        url_id: Some(1),
        definition_version: Some("1.0.0".into()),
        batch_id: None,
        redactions: 0,
    }
}

fn decisions_rates() -> Rates {
    let (input, output) = test_price("openai", "gpt-6-luna", "decisions");
    Rates::per_million(&input, &output, PriceSource::Table).unwrap()
}

fn answer(run: &os_ai::adapters::openai_decisions::DecisionsRun, name: &str) -> DecisionAnswer {
    match run.result(name) {
        Some(QuestionResult::Answered(a)) => a.clone(),
        other => panic!("{name}: answered expected, got {other:?}"),
    }
}

// spec: AI-004
// Ein Request mit 3 Fragen (je eine `predicate`, `choice`, `score`) entspricht dem beschriebenen Aufbau (Prüfung
// gegen das Request-Schema); die Antworten werden je Typ übernommen.
#[tokio::test]
async fn three_question_request_matches_the_documented_shape() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text("Wanderrucksack, 79 EUR, auf Lager."),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    let bodies = mock.bodies(DECISIONS);
    assert_eq!(bodies.len(), 1);
    let body = &bodies[0];
    request_schema()
        .validate(body)
        .unwrap_or_else(|e| panic!("request violates the AI-004 shape: {e:?}\n{body:#}"));
    assert_eq!(body["model"], "gpt-6-luna");
    assert_eq!(body["input"], "Wanderrucksack, 79 EUR, auf Lager.");
    assert_eq!(
        body["questions"][1]["choices"][0],
        json!({"value": "product", "description": "A single product with price."})
    );
    assert_eq!(body["questions"][2]["levels"][2]["label"], "c");
    let sent = &mock.requests_to(DECISIONS)[0];
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.headers["authorization"], "[REDACTED]");

    assert_eq!(
        answer(&run, "q1_predicate"),
        DecisionAnswer::Predicate { probability: 0.93 }
    );
    let DecisionAnswer::Choice {
        choice,
        confidence,
        probabilities,
        ..
    } = answer(&run, "q2_choice")
    else {
        panic!("choice expected")
    };
    assert_eq!(choice, "product");
    assert_eq!(probabilities.len(), 3);
    assert!((confidence - 0.8).abs() < 1e-12);
    let DecisionAnswer::Score { score, level, .. } = answer(&run, "q3_score") else {
        panic!("score expected")
    };
    assert!((score - 1.5).abs() < 1e-9);
    assert_eq!(level, "c");
    assert_eq!(run.requests.len(), 1);
    assert_eq!(run.requests[0].status, AiStatus::Success);
    assert_eq!(
        run.requests[0].questions,
        ["q1_predicate", "q2_choice", "q3_score"]
    );
}

// spec: AI-004
// `<Q> Level` ist das Label des Level-Index `round(score)` (0,5 aufrunden): Levels [a, b, c], `score = 1.5` → `c`;
// `score = 1.49` → `b`; `score = 1.0` bei `probabilities` [0,45; 0,10; 0,45] → `b` (nicht das argmax).
#[tokio::test]
async fn level_is_the_label_of_the_rounded_score() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let questions: Vec<Question> = ["level_a", "level_b", "level_c"]
        .iter()
        .map(|name| Question::score(*name, "Rate the page.", &levels_abc()))
        .collect();
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text("MOCK-ID:k02-levels"),
            &questions,
            &DecideOptions::default(),
        )
        .await;
    for (name, score, level) in [
        ("level_a", 1.5, "c"),
        ("level_b", 1.49, "b"),
        ("level_c", 1.0, "b"),
    ] {
        let DecisionAnswer::Score {
            score: got,
            level: got_level,
            level_index: index,
            probabilities,
            ..
        } = answer(&run, name)
        else {
            panic!("score expected")
        };
        assert_eq!(got, score, "{name}: score is taken over unchanged");
        assert_eq!(got_level, level, "{name}");
        assert_eq!(index, level_index(score, 3));
        assert_eq!(probabilities.len(), 3);
    }
    assert_eq!(level_index(0.5, 3), 1);
    assert_eq!(level_index(0.49, 3), 0);
    assert_eq!(level_index(2.0, 3), 2);
    assert_eq!(level_index(0.0, 3), 0);
}

// spec: AI-004
// Ein Antworteintrag mit `type: "refusal"` wirkt nur auf seine Frage (hier Frage 2 von 3); die anderen Fragen desselben
// Requests werden normal übernommen. Auch das Mock-Szenario `refusal` (verweigert die erste Frage) betrifft nur diese.
#[tokio::test]
async fn a_refusal_entry_only_affects_its_own_question() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text("MOCK-ID:k02-refusal-q2"),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(
        run.result("q1_predicate").unwrap().status(),
        AiStatus::Success
    );
    assert_eq!(
        run.result("q2_choice"),
        Some(&QuestionResult::Refusal {
            reason: Some("Fixture refusal for question 2".into())
        })
    );
    assert_eq!(run.result("q2_choice").unwrap().answer(), None);
    assert_eq!(run.result("q3_score").unwrap().status(), AiStatus::Success);
    assert_eq!(run.requests.len(), 1);
    assert_eq!(run.requests[0].meta.attempts, 1, "a refusal is not retried");

    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text("MOCK-SCENARIO:refusal"),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    let statuses: Vec<AiStatus> = run.results.iter().map(|(_, r)| r.status()).collect();
    assert_eq!(
        statuses,
        [AiStatus::Refusal, AiStatus::Success, AiStatus::Success]
    );
}

// spec: AI-004
// Fehlt in `answers` eine angefragte Frage, erhält diese `Error: Missing answer`; die übrigen werden übernommen
// (Szenario `missing-answer`).
#[tokio::test]
async fn a_missing_answer_only_affects_the_missing_question() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text("MOCK-SCENARIO:missing-answer"),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(
        run.result("q1_predicate").unwrap().status(),
        AiStatus::Success
    );
    assert_eq!(run.result("q2_choice").unwrap().status(), AiStatus::Success);
    assert_eq!(run.result("q3_score"), Some(&QuestionResult::MissingAnswer));
    assert_eq!(
        run.result("q3_score").unwrap().status().to_string(),
        "Error: Missing answer"
    );
    assert_eq!(mock.requests_to(DECISIONS).len(), 1);
}

// spec: AI-004
// Kosten im Protokoll = Eingabe-Tokens × Eingabepreis (`openai/gpt-6-luna/decisions`, 0,10 USD je 1 Mio.); Ausgabe-Tokens
// tragen 0 bei, auch wenn der Mock welche meldet – auch mit Sätzen, die einen Ausgabepreis hätten.
#[tokio::test]
async fn cost_is_input_tokens_times_input_price_only() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let text = "Ein langer Seitentext über Wanderrucksäcke. ".repeat(100);
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text(text),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    let record = &run.requests[0];
    let usage = record.meta.usage.clone().unwrap();
    let input = usage.input_tokens.unwrap();
    assert_eq!(input, token_count(&mock.bodies(DECISIONS)[0]) as u64);
    assert!(
        usage.output_tokens.unwrap() > 0,
        "the mock reports output tokens"
    );
    // 0,10 USD je 1 Mio. Tokens = 0,1 µUSD je Token, kaufmännisch gerundet.
    let expected = MicroUsd((input + 5) / 10);
    assert!(expected.0 > 0);
    let outcome = record.request_outcome();
    let row = request_record(
        &ctx(),
        &decisions_billing_rates(&decisions_rates()),
        MicroUsd(10_000),
        &outcome,
    );
    assert_eq!(row.token_cost, expected);
    assert_eq!(row.cost, expected);
    assert_eq!(row.output_tokens, usage.output_tokens);
    assert_eq!(row.price_source, PriceSource::Table);
    let with_output_price = Rates::per_million("0.1", "0.5", PriceSource::Table).unwrap();
    let row = request_record(
        &ctx(),
        &decisions_billing_rates(&with_output_price),
        MicroUsd(10_000),
        &outcome,
    );
    assert_eq!(row.cost, expected);
}

// spec: AI-004
// `choice`-Antwort mit `probabilities` (höchster Wert 0,80) und `confidence = 0.70` → `<Q> Confidence = 0.80`, roher
// Wert 0,70 bleibt erhalten, Protokoll `confidence_mismatch = true`; bei `confidence = 0.80` kein Vermerk.
#[tokio::test]
async fn confidence_is_the_highest_probability_and_mismatches_are_logged() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let choices = [
        ("product", "Product."),
        ("category", "Category."),
        ("other", "Other."),
    ];
    let mut runs = Vec::new();
    for name in ["mismatch", "match"] {
        runs.push(
            adapter
                .decide(
                    &limiter(),
                    &DecisionInput::text("MOCK-ID:k02-confidence"),
                    &[Question::choice(name, "Page type?", &choices)],
                    &DecideOptions::default(),
                )
                .await,
        );
    }
    let mismatch = answer(&runs[0], "mismatch");
    assert_eq!(mismatch.confidence(), Some(0.8));
    assert_eq!(mismatch.confidence_raw(), Some(0.7));
    assert!(mismatch.confidence_mismatch());
    assert!(runs[0].requests[0].confidence_mismatch);
    assert!(runs[0].requests[0].request_outcome().confidence_mismatch);
    let row = request_record(
        &ctx(),
        &decisions_billing_rates(&decisions_rates()),
        MicroUsd(10_000),
        &runs[0].requests[0].request_outcome(),
    );
    assert!(row.confidence_mismatch);

    let matching = answer(&runs[1], "match");
    assert_eq!(matching.confidence(), Some(0.8));
    assert_eq!(matching.confidence_raw(), Some(0.8));
    assert!(!matching.confidence_mismatch());
    assert!(!runs[1].requests[0].request_outcome().confidence_mismatch);
}

// spec: AI-004
// Antwort ohne `usage` → `Input Tokens` und `Output Tokens` leer (`null`, nicht `0`), `Cost (USD)` = Schätzbetrag nach
// AI-012, `Price Source = estimate`; der Betrag geht in den Budgetverbrauch (AI-013) ein.
#[tokio::test]
async fn missing_usage_is_priced_from_the_estimate_and_counts_against_the_budget() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let text = format!(
        "MOCK-SCENARIO:missing-usage {}",
        "Text über Wanderwege. ".repeat(80)
    );
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text(text),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    assert!(run
        .results
        .iter()
        .all(|(_, r)| r.status() == AiStatus::Success));
    let record = &run.requests[0];
    assert_eq!(record.meta.usage, None);
    assert_eq!(mock.bodies(DECISIONS).len(), 1);
    let estimated = record.estimated_input_tokens;
    assert!(estimated > 0);
    let rates = decisions_billing_rates(&decisions_rates());
    let row = request_record(&ctx(), &rates, MicroUsd(10_000), &record.request_outcome());
    assert_eq!(row.input_tokens, None);
    assert_eq!(row.output_tokens, None);
    assert_eq!(row.price_source, PriceSource::Estimate);
    assert_eq!(row.cost, MicroUsd((estimated + 5) / 10));
    assert!(row.cost.0 > 0);
    assert_eq!(row.estimated_input_tokens, Some(estimated));

    let ledger = CrawlBudgetLedger::new(BudgetLimits::default());
    let reservation = ledger
        .reserve(BudgetScope::task("ai", "decision"), MicroUsd(10_000))
        .unwrap();
    ledger.settle(reservation.id, row.cost).unwrap();
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.consumption(), row.cost);
    assert_eq!(snapshot.area_spent("ai"), row.cost);
}

// spec: AI-004
// Unbekannte zusätzliche Felder in der Antwort werden ignoriert (Vorwärtskompatibilität der Beta); ein unbekannter
// `type` → `Error: Invalid output` nur für diese Frage.
#[tokio::test]
async fn unknown_fields_are_ignored_and_an_unknown_type_is_invalid_for_that_question_only() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let questions = vec![
        Question::predicate("extra_fields", "Is it a product page?"),
        Question::predicate("unknown_type", "Is it in stock?"),
        Question::choice("plain", "Type?", &[("product", "P"), ("other", "O")]),
    ];
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text("MOCK-ID:k02-forward"),
            &questions,
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(
        answer(&run, "extra_fields"),
        DecisionAnswer::Predicate { probability: 0.4 }
    );
    assert!(matches!(
        run.result("unknown_type"),
        Some(QuestionResult::InvalidOutput { .. })
    ));
    assert_eq!(
        run.result("unknown_type").unwrap().status(),
        AiStatus::InvalidOutput
    );
    assert_eq!(run.result("plain").unwrap().status(), AiStatus::Success);
    assert_eq!(run.requests[0].status, AiStatus::Success);
}

// spec: AI-004
// Bildeingaben, die nicht als Data-URL vorliegen, werden vor dem Senden geladen und kodiert (im Request nur
// `data:`-URLs, nie die HTTP-Adresse); Bilder über 20 MB → `Skipped (image too large)` ohne Request.
#[tokio::test]
async fn images_are_loaded_and_encoded_and_oversized_images_are_skipped() {
    let site = route_site(
        "images",
        &format!(
            r#"
[[route]]
host = "ai.test"
path = "/images/pack.png"
status = 200
content_type = "image/png"
png_size = 4096

[[route]]
host = "ai.test"
path = "/images/limit.png"
status = 200
content_type = "image/png"
png_size = {limit}

[[route]]
host = "ai.test"
path = "/images/huge.png"
status = 200
content_type = "image/png"
png_size = {huge}

[[route]]
host = "ai.test"
path = "/images/map.svg"
status = 200
content_type = "image/svg+xml"
body = "<svg xmlns='http://www.w3.org/2000/svg'/>"
"#,
            limit = MAX_IMAGE_BYTES,
            huge = MAX_IMAGE_BYTES + 1
        ),
    )
    .await;
    let mock = Mock::start().await;
    // Zwei Server: ai-mock für OpenAI, die Route-Site für ai.test.
    let client = reqwest::Client::builder()
        .no_proxy()
        .resolve(GLOBAL_HOST, mock.addr())
        .resolve("ai.test", site.addr())
        .build()
        .unwrap();
    let http = os_ai::adapters::openai_responses::OpenAiHttp::new(
        client,
        mock.config(GLOBAL_HOST),
        os_ai::adapters::openai_responses::OpenAiAccount::new(),
    );
    let adapter = OpenAiDecisions::new(http);
    let alt = [Question::predicate(
        "alt_matches",
        "Does the alt text describe the image?",
    )];
    let url = |path: &str| format!("http://ai.test:{}{path}", site.port());
    let run = adapter
        .decide_with_images(
            &limiter(),
            Some("Alt: Wanderrucksack".into()),
            vec![ImageSource::Url(url("/images/pack.png"))],
            &alt,
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.results[0].1.status(), AiStatus::Success);
    let body = &mock.bodies(DECISIONS)[0];
    request_schema().validate(body).unwrap();
    let content = body["input"][0]["content"].as_array().unwrap();
    assert_eq!(
        content[0],
        json!({"type": "input_text", "text": "Alt: Wanderrucksack"})
    );
    let data_url = content[1]["image_url"].as_str().unwrap();
    assert!(data_url.starts_with("data:image/png;base64,iVBORw0KGgo"));
    assert_eq!(
        data_url.len(),
        "data:image/png;base64,".len() + 4096usize.div_ceil(3) * 4
    );
    assert!(!mock.requests_to(DECISIONS)[0].body.contains("ai.test"));
    assert_eq!(run.requests[0].images, 1);

    // Genau 20 MB werden noch gesendet (Grenze einschließlich) – hier nur aufbereitet, nicht gesendet.
    let limit = DecisionInput::with_images(
        adapter.http().client(),
        None,
        vec![ImageSource::Url(url("/images/limit.png"))],
    )
    .await
    .unwrap();
    assert_eq!(limit.image_count(), 1);

    for source in [
        ImageSource::Url(url("/images/huge.png")),
        ImageSource::Bytes {
            media_type: Some("image/png".into()),
            bytes: vec![0u8; (MAX_IMAGE_BYTES + 1) as usize],
        },
        ImageSource::DataUrl(format!(
            "data:image/png;base64,{}",
            encode_base64(&vec![1u8; (MAX_IMAGE_BYTES + 1) as usize])
        )),
    ] {
        let before = mock.requests_to(DECISIONS).len();
        let run = adapter
            .decide_with_images(
                &limiter(),
                None,
                vec![source],
                &alt,
                &DecideOptions::default(),
            )
            .await;
        assert_eq!(run.results[0].1.status(), AiStatus::SkippedImageTooLarge);
        assert_eq!(
            run.results[0].1.status().to_string(),
            "Skipped (image too large)"
        );
        assert!(run.requests.is_empty());
        assert_eq!(mock.requests_to(DECISIONS).len(), before, "no request");
    }

    let err = DecisionInput::with_images(
        adapter.http().client(),
        None,
        vec![ImageSource::Url(url("/images/map.svg"))],
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ImageError::UnsupportedFormat { .. }));
}

// spec: AI-004
// Höchstens 10 Fragen je Request (OS-Festlegung bis zur GA); einstellbar 1–10, eine Einstellung über 10 oder 0 wird
// abgelehnt. 25 Fragen → 3 Requests (10 + 10 + 5), jede Frage genau einmal; 12 → 10 + 2.
#[tokio::test]
async fn at_most_ten_questions_per_request_and_each_question_exactly_once() {
    assert_eq!(MaxQuestionsPerRequest::default().get(), 10);
    assert!(MaxQuestionsPerRequest::new(0).is_none());
    assert!(MaxQuestionsPerRequest::new(11).is_none());
    assert!(serde_json::from_value::<MaxQuestionsPerRequest>(json!(11)).is_err());
    assert_eq!(
        serde_json::from_value::<MaxQuestionsPerRequest>(json!(4))
            .unwrap()
            .get(),
        4
    );
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let questions: Vec<Question> = (0..25)
        .map(|i| Question::predicate(format!("q{i:02}"), format!("Question {i}?")))
        .collect();
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text("Seite"),
            &questions,
            &DecideOptions::default(),
        )
        .await;
    let sizes: Vec<usize> = mock
        .bodies(DECISIONS)
        .iter()
        .map(|b| b["questions"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes, [10, 10, 5]);
    let mut sent: Vec<String> = mock
        .bodies(DECISIONS)
        .iter()
        .flat_map(|b| b["questions"].as_array().unwrap().clone())
        .map(|q| q["name"].as_str().unwrap().to_string())
        .collect();
    sent.sort();
    assert_eq!(
        sent,
        questions.iter().map(|q| q.name.clone()).collect::<Vec<_>>()
    );
    assert_eq!(run.results.len(), 25);
    assert!(run
        .results
        .iter()
        .all(|(_, r)| r.status() == AiStatus::Success));
    assert_eq!(run.requests.len(), 3);

    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text("Seite"),
            &questions[..12],
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.requests.len(), 2);
    assert_eq!(run.requests[1].questions.len(), 2);
    let run = adapter
        .decide(
            &limiter(),
            &DecisionInput::text("Seite"),
            &questions[..7],
            &DecideOptions {
                max_questions: MaxQuestionsPerRequest::new(3).unwrap(),
                ..DecideOptions::default()
            },
        )
        .await;
    assert_eq!(
        run.requests
            .iter()
            .map(|r| r.questions.len())
            .collect::<Vec<_>>(),
        [3, 3, 1]
    );
}

proptest! {
    // spec: AI-004
    // Property: 1–40 Fragen, Höchstzahl 1–10 → kein Bündel über der Höchstzahl, jede Frage genau einmal, Reihenfolge
    // erhalten.
    #[test]
    fn bundles_never_exceed_the_limit(count in 1usize..=40, max in 1u8..=10) {
        let questions: Vec<Question> = (0..count)
            .map(|i| Question::predicate(format!("q{i}"), "x"))
            .collect();
        let limit = MaxQuestionsPerRequest::new(max).unwrap();
        let bundles = bundle(&questions, limit);
        prop_assert!(bundles.iter().all(|b| !b.is_empty() && b.len() <= usize::from(max)));
        let flat: Vec<&Question> = bundles.iter().flat_map(|b| b.iter()).collect();
        prop_assert_eq!(flat.len(), count);
        for (a, b) in flat.iter().zip(&questions) {
            prop_assert_eq!(&a.name, &b.name);
        }
        prop_assert_eq!(bundles.len(), count.div_ceil(usize::from(max)));
    }
}

// spec: AI-017
// Bei einem Request mit mehreren Decision-Fragen gilt ein Timeout für alle Fragen dieses Requests (Szenario `hang`).
#[tokio::test]
async fn one_timeout_covers_every_question_of_the_request() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST));
    let run = adapter
        .decide(
            &limiter_timeout(1),
            &DecisionInput::text("MOCK-SCENARIO:hang"),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.requests.len(), 1);
    for (_, result) in &run.results {
        assert_eq!(result.status(), AiStatus::Timeout { seconds: 1 });
        assert_eq!(result.status().to_string(), "Error: Timeout nach 1 s");
    }
    assert_eq!(run.requests[0].meta.http_status, None);
    assert_eq!(mock.requests_to(DECISIONS).len(), 1);
}

// spec: AI-004
// Ohne Schlüssel kein Request (`Error: Missing API key` je Frage); EU-Basis-URL nur als Parameter (Host im Mock-Log).
#[tokio::test]
async fn missing_key_sends_nothing_and_the_base_url_is_a_parameter() {
    let mock = Mock::start().await;
    let http = os_ai::adapters::openai_responses::OpenAiHttp::new(
        mock.client(),
        os_ai::adapters::openai_responses::OpenAiConfig::new(
            os_ai::adapters::openai_responses::ApiKey::default(),
        )
        .with_base_url(mock.base(GLOBAL_HOST)),
        os_ai::adapters::openai_responses::OpenAiAccount::new(),
    );
    let run = OpenAiDecisions::new(http)
        .decide(
            &limiter(),
            &DecisionInput::text("x"),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    assert!(run
        .results
        .iter()
        .all(|(_, r)| r.status() == AiStatus::MissingApiKey));
    assert!(mock.openai_requests().is_empty());

    let run = OpenAiDecisions::new(mock.http(EU_HOST))
        .decide(
            &limiter(),
            &DecisionInput::text("x"),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    assert!(run
        .results
        .iter()
        .all(|(_, r)| r.status() == AiStatus::Success));
    let sent = mock.requests_to(DECISIONS);
    assert_eq!(sent.len(), 1);
    assert_eq!(support::host_of(&sent[0]), EU_HOST);
}

// spec: AI-001
// Fähigkeiten aus dem Katalog (Decisions, Text und Bild) und Contract-Weg über `provider::execute`: Eine Bildfrage
// braucht keine Vision-Fähigkeit; `content` trägt das Ergebnis je Frage.
#[tokio::test]
async fn provider_contract_with_decision_questions_and_an_image() {
    let mock = Mock::start().await;
    let adapter = OpenAiDecisions::new(mock.http(GLOBAL_HOST)).with_capabilities(Arc::new(|id| {
        (id == "gpt-6-luna").then_some(DECISIONS_CAPABILITIES)
    }));
    let models = adapter.models().await.unwrap();
    let luna = models
        .iter()
        .find(|m| m.id == "gpt-6-luna")
        .unwrap()
        .clone();
    assert_eq!(luna.endpoint, "decisions");
    assert_eq!(luna.capabilities, DECISIONS_CAPABILITIES);
    assert_eq!(luna.capability_source, CapabilitySource::ModelCatalog);
    let request = Request {
        model: luna,
        required: Capabilities {
            decisions: true,
            ..Capabilities::default()
        },
        input: vec![
            Input::Text {
                text: "Alt: Karte".into(),
            },
            Input::Image {
                media_type: "image/png".into(),
                base64: "iVBORw0KGgo=".into(),
            },
        ],
        instructions: String::new(),
        questions: three_questions()
            .iter()
            .map(|q| serde_json::to_value(q).unwrap())
            .collect(),
        response_schema: None,
        max_tokens: None,
        temperature: None,
        effort: None,
    };
    let Outcome::Completed(response) = execute(PreviewSwitch::ON, &adapter, request).await else {
        panic!("completed expected")
    };
    let answers = response.content.unwrap()["answers"].clone();
    assert_eq!(answers.as_array().unwrap().len(), 3);
    assert_eq!(answers[0]["status"], "Success");
    assert_eq!(answers[1]["answer"]["choice"], "product");
    assert_eq!(answers[2]["answer"]["level"], "c");
    assert!(response.usage.input_tokens.is_some());
    let body = &mock.bodies(DECISIONS)[0];
    request_schema().validate(body).unwrap();
    assert_eq!(
        body["input"][0]["content"][1]["image_url"],
        "data:image/png;base64,iVBORw0KGgo="
    );
}

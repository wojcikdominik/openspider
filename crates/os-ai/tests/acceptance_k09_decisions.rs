//! Unabhängige Abnahmetests M4-K09 für AI-004 (Nachtrag 24.11.2026, Q00-M1, H2, N1, N6): weder `store` noch
//! `reasoning` an `/v1/decisions`, Anbieter-Request-ID aus `x-request-id` (Antwort ohne `id`), `cache_write_tokens`,
//! Tokenzählung des Mocks je Frage (08 Konventionen, Nachtrag) und unveränderte Auswertung der Antworten.
mod acceptance_k09_support;

use acceptance_k09_support::{
    contains_key, fast_limiter, model, openai_http, Canned, Mock, Stub, ALL_LEVELS,
};
use os_ai::adapters::openai_decisions::{
    DecideOptions, DecisionAnswer, DecisionInput, OpenAiDecisions, Question, QuestionKind,
    QuestionResult,
};
use os_ai::adapters::openai_responses::image::ImageSource;
use os_ai::provider::{AiProvider, Capabilities, Input, Request};
use serde_json::{json, Value};

const DECISIONS: &str = "/openai/v1/decisions";
const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn three_questions() -> Vec<Question> {
    vec![
        Question::predicate("is_product", "Is this a product detail page?"),
        Question::choice(
            "page_type",
            "Which page type is this?",
            &[
                ("product", "A single product"),
                ("category", "A list of products"),
            ],
        ),
        Question::score(
            "quality",
            "How good is the content?",
            &[("low", "thin"), ("mid", "ok"), ("high", "rich")],
        ),
    ]
}

/// Zeichen des Fragentexts nach AI-012 (Nachtrag): `name` + `instructions` + je `choices`-Eintrag `value` und
/// `description` + je `levels`-Eintrag `label` und `description` – eigene Zählung aus der Fragendefinition.
fn question_chars(q: &Question) -> u64 {
    let n = |s: &str| s.chars().count() as u64;
    let extra: u64 = match &q.kind {
        QuestionKind::Predicate => 0,
        QuestionKind::Choice { choices } => choices
            .iter()
            .map(|c| n(&c.value) + n(&c.description))
            .sum(),
        QuestionKind::Score { levels } => {
            levels.iter().map(|l| n(&l.label) + n(&l.description)).sum()
        }
    };
    n(&q.name) + n(&q.instructions) + extra
}

/// Eingabe-Tokens des Mocks nach 08 Konventionen (Nachtrag Q00): Σ über die Fragen
/// (⌈Z_E / 4⌉ + 85 × B + ⌈Z_q / 4⌉ + 100).
fn expected_tokens(input_chars: u64, images: u64, questions: &[Question]) -> u64 {
    questions
        .iter()
        .map(|q| input_chars.div_ceil(4) + 85 * images + question_chars(q).div_ceil(4) + 100)
        .sum()
}

fn assert_no_store_or_reasoning(bodies: &[Value]) {
    assert!(!bodies.is_empty());
    for body in bodies {
        assert!(body.get("store").is_none(), "{body}");
        assert!(!contains_key(body, "reasoning"), "{body}");
        assert!(!contains_key(body, "reasoning_effort"), "{body}");
        assert!(!contains_key(body, "prompt_cache_retention"), "{body}");
    }
}

// spec: AI-004 [M4]
// acceptance: independent
// Kriterium (Property über Decisions-Szenarien): kein Request an `/openai/v1/decisions` enthält `store` oder
// `reasoning` – auch mit Bild, bei Wiederholungen und über das gemeinsame Request-Interface mit gesetzter Stufe.
#[tokio::test]
async fn decisions_requests_never_carry_store_or_reasoning() {
    let mock = Mock::start().await;
    let adapter = mock.decisions();
    let limiter = fast_limiter();
    let options = DecideOptions::default();
    for text in [
        "K09 decisions plain",
        "K09 decisions MOCK-SCENARIO:refusal",
        "K09 decisions MOCK-SCENARIO:missing-answer",
        "K09 decisions MOCK-SCENARIO:429x2",
        "K09 decisions MOCK-SCENARIO:reasoning=40",
    ] {
        let run = adapter
            .decide(
                &limiter,
                &DecisionInput::text(text),
                &three_questions(),
                &options,
            )
            .await;
        assert_eq!(run.requests.len(), 1, "{text}");
    }
    let run = adapter
        .decide_with_images(
            &limiter,
            Some("K09 decisions image".into()),
            vec![ImageSource::Base64 {
                media_type: "image/png".into(),
                base64: PNG_1X1.into(),
            }],
            &three_questions(),
            &options,
        )
        .await;
    assert_eq!(run.requests.len(), 1);
    // Gemeinsames Request-Interface: auch eine gesetzte Stufe ergibt keinen Parameter (die Decisions API hat keine).
    for level in ALL_LEVELS {
        let mut m = model("openai", "gpt-6-luna", "decisions", Vec::new());
        m.capabilities = Capabilities {
            decisions: true,
            ..Capabilities::default()
        };
        let request = Request {
            model: m.clone(),
            required: m.capabilities,
            input: vec![Input::Text {
                text: format!("K09 decisions provider {level}"),
            }],
            instructions: String::new(),
            questions: vec![serde_json::to_value(&three_questions()[0]).unwrap()],
            response_schema: None,
            max_tokens: Some(256),
            temperature: None,
            effort: None,
            reasoning: level,
        };
        let _ = adapter.execute(request).await;
    }
    let bodies = mock.bodies(DECISIONS);
    assert!(bodies.len() >= 16, "{}", bodies.len());
    assert!(bodies
        .iter()
        .any(|b| b.to_string().contains("\"input_image\"")));
    assert_no_store_or_reasoning(&bodies);
}

// spec: AI-004 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// Kriterium: Mock-Antwort ohne `id` mit Header `x-request-id: req_mock_3` → `Provider Request ID = req_mock_3`;
// `cache_write_tokens: 0` → `Cache Write Tokens = 0`.
#[tokio::test]
async fn request_id_header_and_cache_write_tokens() {
    let stub = Stub::start(
        Canned::json(
            200,
            json!({"answers": [{"name": "is_product", "type": "predicate", "probability": 0.7}],
                   "model": "gpt-6-luna",
                   "usage": {"input_tokens": 120, "output_tokens": 0,
                             "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
                             "output_tokens_details": {"reasoning_tokens": 0}}}),
        )
        .with_header("x-request-id", "req_mock_3"),
    )
    .await;
    let adapter = OpenAiDecisions::new(openai_http(&stub.url("/v1")));
    let questions = vec![Question::predicate("is_product", "Product?")];
    let reply = adapter
        .send("gpt-6-luna", &DecisionInput::text("x"), &questions, None)
        .await
        .expect("HTTP 200");
    assert_eq!(reply.provider_request_id.as_deref(), Some("req_mock_3"));
    let usage = reply.usage.expect("usage");
    assert_eq!(usage.cache_write_tokens, Some(0));
    assert_eq!(usage.input_tokens, Some(120));

    let run = adapter
        .decide(
            &fast_limiter(),
            &DecisionInput::text("x"),
            &questions,
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(
        run.requests[0].meta.provider_request_id.as_deref(),
        Some("req_mock_3")
    );
    assert_eq!(
        run.requests[0]
            .meta
            .usage
            .as_ref()
            .and_then(|u| u.cache_write_tokens),
        Some(0)
    );

    // Gegen den ai-mock: Antwort ohne `id`, Request-ID aus `x-request-id`.
    let mock = Mock::start().await;
    let reply = mock
        .decisions()
        .send("gpt-6-luna", &DecisionInput::text("y"), &questions, None)
        .await
        .unwrap();
    let id = reply.provider_request_id.expect("Request-ID");
    assert!(id.starts_with("req_mock_"), "{id}");
    assert_eq!(reply.usage.and_then(|u| u.cache_write_tokens), Some(0));
}

// spec: AI-004 [M4]
// acceptance: independent
// Tokenzählung des Mocks je Frage (Q00-M1): 3 Fragen (`predicate`, `choice`, `score`) → Σ je Frage; die Antworten
// werden weiterhin korrekt ausgewertet (Wahrscheinlichkeit, Wahl, Score mit Level nach `round(score)`).
#[tokio::test]
async fn three_questions_are_billed_per_question_and_still_evaluated() {
    let mock = Mock::start().await;
    let text = "K09 decisions three questions, input text of moderate length.";
    let questions = three_questions();
    let run = mock
        .decisions()
        .decide(
            &fast_limiter(),
            &DecisionInput::text(text),
            &questions,
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.requests.len(), 1);
    let usage = run.requests[0].meta.usage.clone().expect("usage");
    let expected = expected_tokens(text.chars().count() as u64, 0, &questions);
    assert_eq!(usage.input_tokens, Some(expected));
    assert_eq!(usage.output_tokens, Some(0));
    // Je Frage mindestens der Grundanteil 100: die Bündelung spart keine Tokens.
    assert!(expected >= 3 * 100);

    match run.result("is_product") {
        Some(QuestionResult::Answered(DecisionAnswer::Predicate { probability })) => {
            assert!((0.0..=1.0).contains(probability))
        }
        other => panic!("predicate: {other:?}"),
    }
    match run.result("page_type") {
        Some(QuestionResult::Answered(DecisionAnswer::Choice {
            choice,
            probabilities,
            confidence,
            ..
        })) => {
            assert!(["product", "category"].contains(&choice.as_str()));
            let max = probabilities
                .iter()
                .map(|p| p.probability)
                .fold(0.0, f64::max);
            assert!((confidence - max).abs() < 1e-9);
        }
        other => panic!("choice: {other:?}"),
    }
    match run.result("quality") {
        Some(QuestionResult::Answered(DecisionAnswer::Score {
            score,
            level_index,
            level,
            ..
        })) => {
            let rounded = (score + 0.5).floor() as usize;
            assert_eq!(*level_index, rounded.min(2));
            assert_eq!(level, ["low", "mid", "high"][*level_index]);
        }
        other => panic!("score: {other:?}"),
    }
}

// spec: AI-004 [M4]
// acceptance: independent
// 11 Fragen (Q00-Prüfwert): höchstens 10 je Request (unverändert) → 2 Requests; jeder Request wird je Frage gezählt,
// die Summe entspricht 11 Fragen; alle 11 Fragen sind beantwortet. Mit Bild zählt jede Frage zusätzlich 85 je Bild.
#[tokio::test]
async fn eleven_questions_are_billed_per_question() {
    let mock = Mock::start().await;
    let text =
        "K09 eleven questions with a 159 character style input text for the decisions endpoint.";
    let questions: Vec<Question> = (0..11)
        .map(|i| Question::predicate(format!("q{i:02}"), format!("Is statement {i} true?")))
        .collect();
    let run = mock
        .decisions()
        .decide(
            &fast_limiter(),
            &DecisionInput::text(text),
            &questions,
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.requests.len(), 2);
    assert_eq!(run.requests[0].questions.len(), 10);
    assert_eq!(run.requests[1].questions.len(), 1);
    let chars = text.chars().count() as u64;
    let mut total = 0;
    for record in &run.requests {
        let sent: Vec<Question> = questions
            .iter()
            .filter(|q| record.questions.contains(&q.name))
            .cloned()
            .collect();
        let usage = record.meta.usage.clone().expect("usage");
        assert_eq!(usage.input_tokens, Some(expected_tokens(chars, 0, &sent)));
        assert_eq!(usage.output_tokens, Some(0));
        total += usage.input_tokens.unwrap();
    }
    assert_eq!(total, expected_tokens(chars, 0, &questions));
    assert_eq!(run.results.len(), 11);
    assert!(run.results.iter().all(|(_, r)| matches!(
        r,
        QuestionResult::Answered(DecisionAnswer::Predicate { .. })
    )));

    let image_run = mock
        .decisions()
        .decide_with_images(
            &fast_limiter(),
            Some("K09 image tokens".into()),
            vec![ImageSource::Base64 {
                media_type: "image/png".into(),
                base64: PNG_1X1.into(),
            }],
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    let usage = image_run.requests[0].meta.usage.clone().expect("usage");
    assert_eq!(
        usage.input_tokens,
        Some(expected_tokens(
            "K09 image tokens".chars().count() as u64,
            1,
            &three_questions()
        ))
    );
}

// spec: AI-004 [M4]
// spec: AI-001 [M4]
// acceptance: independent
// Decisions hat keine Reasoning-Stufe: die Modellliste meldet für den Endpunkt keine Stufen.
#[tokio::test]
async fn decisions_models_have_no_reasoning_levels() {
    let mock = Mock::start().await;
    let models = mock.decisions().models().await.expect("Modellliste");
    assert!(!models.is_empty());
    assert!(models.iter().all(|m| m.reasoning_levels.is_empty()));
}

// spec: AI-001 [M4]
// spec: AI-004 [M4]
// acceptance: independent
// Request-ID im Fehlerweg (Q00-N6: Header `x-request-id`): Fehlerantwort → Wert im Fehlerergebnis eines Versuchs und
// in den Protokolldaten des Requests; gegen den ai-mock (Szenario `401`) `req_mock_<n>`.
#[tokio::test]
async fn error_results_carry_x_request_id() {
    let stub = Stub::start(
        Canned::json(
            400,
            json!({"error": {"type": "invalid_request_error", "message": "bad", "code": null}}),
        )
        .with_header("x-request-id", "req_err_6"),
    )
    .await;
    let adapter = OpenAiDecisions::new(openai_http(&stub.url("/v1")));
    let questions = vec![Question::predicate("is_product", "Product?")];
    let failure = adapter
        .send("gpt-6-luna", &DecisionInput::text("x"), &questions, None)
        .await
        .expect_err("400");
    assert_eq!(failure.provider_request_id.as_deref(), Some("req_err_6"));
    let run = adapter
        .decide(
            &fast_limiter(),
            &DecisionInput::text("x"),
            &questions,
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(
        run.requests[0].meta.provider_request_id.as_deref(),
        Some("req_err_6")
    );

    let mock = Mock::start().await;
    let run = mock
        .decisions()
        .decide(
            &fast_limiter(),
            &DecisionInput::text("K09 MOCK-SCENARIO:401 d"),
            &questions,
            &DecideOptions::default(),
        )
        .await;
    let id = run.requests[0]
        .meta
        .provider_request_id
        .clone()
        .expect("Request-ID im Fehlerweg");
    assert!(id.starts_with("req_mock_"), "{id}");
}

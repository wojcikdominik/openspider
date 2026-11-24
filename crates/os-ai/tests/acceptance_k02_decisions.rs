//! Unabhängige Abnahmetests M4-K02 für den Adapter OpenAI Decisions (AI-004, Beta), allein aus `docs/spec/08-ai.md`
//! abgeleitet (AI-004; dazu AI-001, AI-017, AI-020). Gegen den ai-mock mit eigenem Antwortfragment
//! (`tests/fixtures/ai/k02-acceptance/mock-answers.d/`) bzw. einen kleinen lokalen Testserver.
mod acceptance_k02_support;

use std::collections::BTreeMap;
use std::time::Duration;

use acceptance_k02_support::*;
use os_ai::adapters::openai_decisions::{
    decisions_billing_rates, level_index, DecideOptions, DecisionAnswer, DecisionInput,
    DecisionsRun, MaxQuestionsPerRequest, OpenAiDecisions, Question, QuestionResult,
    DECISIONS_CAPABILITIES,
};
use os_ai::adapters::openai_responses::image::ImageSource;
use os_ai::cost::{
    cost_of, request_record, FeeCounts, LogContext, PriceSource, Rates, TokenCounts,
};
use os_ai::schema::ResponseSchema;
use os_ai::AiStatus;
use os_model::budget::MicroUsd;
use serde_json::{json, Value};

const LEVELS: [(&str, &str); 3] = [("a", "niedrig"), ("b", "mittel"), ("c", "hoch")];
const CHOICES: [(&str, &str); 3] = [
    ("product", "Produktseite"),
    ("category", "Kategorieseite"),
    ("article", "Ratgeber-Artikel"),
];

fn three_questions() -> Vec<Question> {
    vec![
        Question::predicate("q1", "Is this page a soft 404?"),
        Question::choice("q2", "Which page type is this?", &CHOICES),
        Question::score("q3", "How helpful is the content?", &LEVELS),
    ]
}

fn ctx() -> LogContext {
    LogContext {
        run_id: 1,
        timestamp: "2026-11-24T00:00:00Z".into(),
        task: "decisions".into(),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
        endpoint: "decisions".into(),
        url_id: Some(1),
        definition_version: Some("1".into()),
        batch_id: None,
        redactions: 0,
    }
}

/// Preis laut `tests/fixtures/ai/pricing-test.json` für `openai/gpt-6-luna/decisions`: 0,10 USD/1M Eingabe, Ausgabe 0.
fn table_rates() -> Rates {
    Rates::per_million("0.10", "0", PriceSource::Table).unwrap()
}

async fn decide(mock: &Mock, input: &str, questions: &[Question]) -> DecisionsRun {
    OpenAiDecisions::new(mock.http())
        .decide(
            &fast_limiter(),
            &DecisionInput::text(input),
            questions,
            &DecideOptions::default(),
        )
        .await
}

fn statuses(run: &DecisionsRun) -> BTreeMap<String, String> {
    run.results
        .iter()
        .map(|(n, r)| (n.clone(), r.status().to_string()))
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 1: Ein Request mit 3 Fragen (je eine `predicate`, `choice`, `score`) entspricht dem beschriebenen Aufbau
// (Validierung gegen das JSON-Schema des Request-Aufbaus).

// spec: AI-004
// Kriterium 1: 3 Fragen (predicate, choice, score) in einem Request, Aufbau laut Spec (JSON-Schema-Validierung).
#[tokio::test]
async fn k1_request_with_three_question_types_matches_structure() {
    let mock = Mock::start().await;
    let run = decide(&mock, "Seiteninhalt einer Produktseite", &three_questions()).await;
    assert_eq!(run.requests.len(), 1, "3 Fragen in einem Request");
    let bodies = mock.bodies_to("decisions");
    assert_eq!(bodies.len(), 1);
    let body = &bodies[0];
    let schema = ResponseSchema::new(read_fixture_json("decisions-request.schema.json")).unwrap();
    if let Err(errors) = schema.validate(body) {
        panic!("Request verletzt den Aufbau: {errors:?}\n{body}");
    }
    assert_eq!(body["model"], "gpt-6-luna", "Standardmodell der Beta");
    let qs = body["questions"].as_array().unwrap();
    assert_eq!(qs.len(), 3);
    assert_eq!(qs[0]["name"], "q1");
    assert_eq!(qs[0]["type"], "predicate");
    assert_eq!(qs[0]["instructions"], "Is this page a soft 404?");
    assert_eq!(qs[1]["type"], "choice");
    assert_eq!(
        qs[1]["choices"],
        json!([
            {"value": "product", "description": "Produktseite"},
            {"value": "category", "description": "Kategorieseite"},
            {"value": "article", "description": "Ratgeber-Artikel"}
        ])
    );
    assert_eq!(qs[2]["type"], "score");
    assert_eq!(
        qs[2]["levels"],
        json!([
            {"label": "a", "description": "niedrig"},
            {"label": "b", "description": "mittel"},
            {"label": "c", "description": "hoch"}
        ]),
        "Levels aufsteigend in gegebener Reihenfolge"
    );
    // Alle drei Antworten übernommen.
    for (name, result) in &run.results {
        assert_eq!(result.status(), AiStatus::Success, "{name}: {result:?}");
    }
}

// spec: AI-004
// Kriterium 1 (Variante mit Bild): `input` als Nutzer-Nachricht mit Teilen `input_text`/`input_image` (Data-URL).
#[tokio::test]
async fn k1_request_with_image_input_matches_structure() {
    let mock = Mock::start().await;
    let png = tiny_png();
    let run = OpenAiDecisions::new(mock.http())
        .decide_with_images(
            &fast_limiter(),
            Some("Passt der Alt-Text?".into()),
            vec![ImageSource::Bytes {
                media_type: Some("image/png".into()),
                bytes: png.clone(),
            }],
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.requests.len(), 1);
    let body = &mock.bodies_to("decisions")[0];
    let schema = ResponseSchema::new(read_fixture_json("decisions-request.schema.json")).unwrap();
    if let Err(errors) = schema.validate(body) {
        panic!("Request verletzt den Aufbau: {errors:?}\n{body}");
    }
    let content = &body["input"][0]["content"];
    assert_eq!(body["input"][0]["role"], "user");
    assert_eq!(
        content[0],
        json!({"type": "input_text", "text": "Passt der Alt-Text?"})
    );
    assert_eq!(content[1]["type"], "input_image");
    assert_eq!(
        content[1]["image_url"],
        json!(format!("data:image/png;base64,{}", b64(&png)))
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 2: `score` wird übernommen; `<Q> Level` ist das Label des Level-Index `round(score)` (0,5 aufrunden).
// Prüfwerte: Levels [a, b, c], score 1.5 → c; 1.49 → b; 1.0 bei probabilities [0,45; 0,10; 0,45] → b.

fn score_of(result: &QuestionResult) -> (f64, String) {
    match result.answer() {
        Some(DecisionAnswer::Score { score, level, .. }) => (*score, level.clone()),
        other => panic!("erwartet score-Antwort, erhalten {other:?} / {result:?}"),
    }
}

// spec: AI-004
// Kriterium 2: Prüfwerte der Level-Rundung über den Mock (score unverändert übernommen, Level = Label von round(score)).
#[tokio::test]
async fn k2_score_level_rounding_check_values() {
    let mock = Mock::start().await;
    let questions = vec![
        Question::score("s150", "Wie hilfreich?", &LEVELS),
        Question::score("s149", "Wie hilfreich?", &LEVELS),
        Question::score("s100", "Wie hilfreich?", &LEVELS),
    ];
    let run = decide(&mock, "MOCK-ID:k02a-score Inhalt", &questions).await;
    let (score, level) = score_of(run.result("s150").unwrap());
    assert_eq!(score, 1.5);
    assert_eq!(level, "c", "score 1.5 → c (0,5 aufrunden)");
    let (score, level) = score_of(run.result("s149").unwrap());
    assert_eq!(score, 1.49);
    assert_eq!(level, "b", "score 1.49 → b");
    let (score, level) = score_of(run.result("s100").unwrap());
    assert_eq!(score, 1.0);
    assert_eq!(level, "b", "score 1.0 → b, nicht argmax der probabilities");
}

// spec: AI-004
// Kriterium 2: Rundungsregel direkt (Index von round(score), 0,5 aufrunden).
#[test]
fn k2_level_index_rounds_half_up() {
    assert_eq!(level_index(1.5, 3), 2);
    assert_eq!(level_index(1.49, 3), 1);
    assert_eq!(level_index(1.0, 3), 1);
    assert_eq!(level_index(0.5, 3), 1);
    assert_eq!(level_index(0.49, 3), 0);
    assert_eq!(level_index(0.0, 3), 0);
    assert_eq!(level_index(2.0, 3), 2);
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 3: Ein Antworteintrag mit `type: "refusal"` wirkt nur auf seine Frage; die anderen Fragen desselben
// Requests werden normal übernommen. (Testfixture laut Spec: Refusal nur für Frage 2.)

// spec: AI-004
// Kriterium 3: Refusal nur für Frage 2 (eigenes Fragment) → q2 `Refusal`, q1 und q3 `Success`.
#[tokio::test]
async fn k3_refusal_of_question_two_affects_only_that_question() {
    let mock = Mock::start().await;
    let run = decide(&mock, "MOCK-ID:k02a-refusal-q2 Inhalt", &three_questions()).await;
    assert_eq!(run.requests.len(), 1);
    let s = statuses(&run);
    assert_eq!(s["q1"], "Success");
    assert_eq!(s["q2"], "Refusal");
    assert_eq!(s["q3"], "Success");
    assert!(
        run.result("q2").unwrap().answer().is_none(),
        "keine Werte bei Refusal"
    );
    assert!(matches!(
        run.result("q1").unwrap().answer(),
        Some(DecisionAnswer::Predicate { .. })
    ));
    assert!(matches!(
        run.result("q3").unwrap().answer(),
        Some(DecisionAnswer::Score { .. })
    ));
}

// spec: AI-004
// Kriterium 3 mit dem Mock-Szenario `refusal` (der Mock verweigert dabei die erste Frage): genau eine Frage
// `Refusal`, die übrigen `Success`.
#[tokio::test]
async fn k3_mock_refusal_scenario_affects_only_one_question() {
    let mock = Mock::start().await;
    let run = decide(&mock, "MOCK-SCENARIO:refusal Inhalt", &three_questions()).await;
    let s = statuses(&run);
    assert_eq!(s.values().filter(|v| *v == "Refusal").count(), 1, "{s:?}");
    assert_eq!(s.values().filter(|v| *v == "Success").count(), 2, "{s:?}");
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 4: Fehlt in `answers` eine angefragte Frage, erhält diese `Error: Missing answer`; die übrigen werden
// übernommen (Szenario `missing-answer`).

// spec: AI-004
// spec: AI-020
// Kriterium 4: fehlende Frage → `Error: Missing answer`, übrige `Success`.
#[tokio::test]
async fn k4_missing_answer_only_for_absent_question() {
    let mock = Mock::start().await;
    let run = decide(
        &mock,
        "MOCK-SCENARIO:missing-answer Inhalt",
        &three_questions(),
    )
    .await;
    let s = statuses(&run);
    // Der Mock lässt die letzte Frage weg.
    assert_eq!(s["q3"], "Error: Missing answer", "{s:?}");
    assert_eq!(s["q1"], "Success");
    assert_eq!(s["q2"], "Success");
    assert_eq!(run.result("q3").unwrap().status(), AiStatus::MissingAnswer);
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 5: Kosten im Protokoll = Eingabe-Tokens × Eingabepreis; Ausgabe-Tokens tragen 0 bei, auch wenn der Mock
// welche meldet.

fn decisions_reply(usage: Value) -> Value {
    json!({
        "id": "decision_local",
        "answers": [{"name": "q1", "type": "predicate", "probability": 0.7}],
        "usage": usage
    })
}

// spec: AI-004
// Kriterium 5: 1.000.000 Eingabe- und 500.000 Ausgabe-Tokens bei 0,10 USD/1M → Cost = 0,100000 USD.
#[tokio::test]
async fn k5_cost_is_input_tokens_times_input_price_only() {
    let server = LocalServer::start(|_| {
        Canned::json(
            200,
            decisions_reply(json!({"input_tokens": 1_000_000, "output_tokens": 500_000})),
        )
    })
    .await;
    let run = OpenAiDecisions::new(server.http())
        .decide(
            &fast_limiter(),
            &DecisionInput::text("Inhalt"),
            &[Question::predicate("q1", "Soft 404?")],
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.requests.len(), 1);
    let outcome = run.requests[0].request_outcome();
    // Preistabelle (0,10 / 0) und Abrechnungssätze des Adapters bei einem Tabelleneintrag mit Ausgabepreis.
    let with_output_price = Rates::per_million("0.10", "0.50", PriceSource::Table).unwrap();
    for rates in [
        decisions_billing_rates(&table_rates()),
        decisions_billing_rates(&with_output_price),
    ] {
        let record = request_record(&ctx(), &rates, MicroUsd(0), &outcome);
        assert_eq!(record.input_tokens, Some(1_000_000));
        assert_eq!(
            record.output_tokens,
            Some(500_000),
            "gemeldete Ausgabe-Tokens bleiben sichtbar"
        );
        assert_eq!(record.cost, MicroUsd(100_000), "nur Eingabe × 0,10 USD/1M");
        assert_eq!(record.price_source, PriceSource::Table);
    }
}

// spec: AI-004
// Kriterium 5 (Cache): keine Kosten für Cache-Lesen gesondert – auch bei gemeldeten cached_tokens bleibt
// Kosten = Eingabe-Tokens × Eingabepreis.
#[tokio::test]
async fn k5_cost_ignores_cache_split() {
    let server = LocalServer::start(|_| {
        Canned::json(
            200,
            decisions_reply(json!({"input_tokens": 1_000_000, "output_tokens": 10,
                                   "input_tokens_details": {"cached_tokens": 400_000}})),
        )
    })
    .await;
    let run = OpenAiDecisions::new(server.http())
        .decide(
            &fast_limiter(),
            &DecisionInput::text("Inhalt"),
            &[Question::predicate("q1", "Soft 404?")],
            &DecideOptions::default(),
        )
        .await;
    let record = request_record(
        &ctx(),
        &decisions_billing_rates(&table_rates()),
        MicroUsd(0),
        &run.requests[0].request_outcome(),
    );
    assert_eq!(record.cost, MicroUsd(100_000));
}

// spec: AI-004
// Kriterium 5 gegen den Mock: die Kosten entsprechen nur den Eingabe-Tokens × Eingabepreis, auch mit Sätzen, die einen
// Ausgabepreis hätten. Nachtrag 24.11.2026 (08 Konventionen, Q00-M1): der ai-mock meldet für Decisions 0 Ausgabe-Tokens
// (wie die echte API); „auch wenn der Mock welche meldet“ prüft der Stub-Test oben mit 500.000 Ausgabe-Tokens.
#[tokio::test]
async fn k5_mock_reported_output_tokens_cost_nothing() {
    let mock = Mock::start().await;
    // 40.000 Zeichen → Mock zählt 10.000 Eingabe-Tokens (+ Fragentexte).
    let input = format!("Inhalt {}", "x".repeat(40_000));
    let run = decide(&mock, &input, &three_questions()).await;
    let outcome = run.requests[0].request_outcome();
    let with_output_price = Rates::per_million("0.10", "0.50", PriceSource::Table).unwrap();
    let record = request_record(
        &ctx(),
        &decisions_billing_rates(&with_output_price),
        MicroUsd(0),
        &outcome,
    );
    let input_tokens = record.input_tokens.expect("Mock meldet usage");
    assert_eq!(
        record.output_tokens,
        Some(0),
        "Mock meldet 0 Ausgabe-Tokens"
    );
    let expected = cost_of(
        &TokenCounts::plain(input_tokens, 0),
        FeeCounts::one_request(0),
        &table_rates(),
        false,
    );
    assert_eq!(record.cost, expected.cost);
    assert!(record.cost.0 >= 1_000, "≈ 10.000 Tokens × 0,1 µUSD");
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 6: `choice`-Antwort mit `probabilities` (höchster Wert 0,80) und `confidence = 0.70` →
// `<Q> Confidence = 0.80`, roher Wert 0,70, Protokoll `confidence_mismatch = true`; bei `confidence = 0.80` kein
// Vermerk.

// spec: AI-004
// Kriterium 6: Abweichung 0,10 → Confidence 0,80, roh 0,70, confidence_mismatch = true im Protokoll.
#[tokio::test]
async fn k6_confidence_is_max_probability_and_mismatch_is_logged() {
    let mock = Mock::start().await;
    let run = decide(
        &mock,
        "MOCK-ID:k02a-confidence Inhalt",
        &[Question::choice("c070", "Seitentyp?", &CHOICES)],
    )
    .await;
    let answer = run.result("c070").unwrap().answer().expect("übernommen");
    match answer {
        DecisionAnswer::Choice { choice, .. } => assert_eq!(choice, "product"),
        other => panic!("{other:?}"),
    }
    assert_eq!(answer.confidence(), Some(0.80));
    assert_eq!(
        answer.confidence_raw(),
        Some(0.70),
        "roher Wert bleibt erhalten"
    );
    assert!(run.requests[0].confidence_mismatch);
    let record = request_record(
        &ctx(),
        &decisions_billing_rates(&table_rates()),
        MicroUsd(0),
        &run.requests[0].request_outcome(),
    );
    assert!(
        record.confidence_mismatch,
        "Protokoll confidence_mismatch = true"
    );
}

// spec: AI-004
// Kriterium 6: confidence = 0,80 = höchste Einzelwahrscheinlichkeit → kein Vermerk.
#[tokio::test]
async fn k6_matching_confidence_is_not_flagged() {
    let mock = Mock::start().await;
    let run = decide(
        &mock,
        "MOCK-ID:k02a-confidence Inhalt",
        &[Question::choice("c080", "Seitentyp?", &CHOICES)],
    )
    .await;
    let answer = run.result("c080").unwrap().answer().expect("übernommen");
    assert_eq!(answer.confidence(), Some(0.80));
    assert_eq!(answer.confidence_raw(), Some(0.80));
    assert!(!run.requests[0].confidence_mismatch);
    let record = request_record(
        &ctx(),
        &decisions_billing_rates(&table_rates()),
        MicroUsd(0),
        &run.requests[0].request_outcome(),
    );
    assert!(!record.confidence_mismatch);
}

// spec: AI-004
// Kriterium 6 (score-Weg): <Q> Confidence ist auch bei score die höchste Einzelwahrscheinlichkeit.
#[tokio::test]
async fn k6_score_confidence_is_max_probability() {
    let mock = Mock::start().await;
    let run = decide(
        &mock,
        "MOCK-ID:k02a-score Inhalt",
        &[Question::score("s100", "Wie hilfreich?", &LEVELS)],
    )
    .await;
    let answer = run.result("s100").unwrap().answer().unwrap();
    assert_eq!(answer.confidence(), Some(0.45));
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 7: Antwort ohne `usage` → `Input Tokens` und `Output Tokens` leer (`null`, nicht 0), `Cost (USD)` =
// Schätzbetrag nach AI-012, `Price Source = estimate`.

// spec: AI-004
// spec: AI-001
// Kriterium 7: ohne usage → Tokenfelder null, Price Source estimate, Kosten = Schätzung > 0.
#[tokio::test]
async fn k7_missing_usage_gives_null_tokens_and_estimated_cost() {
    let mock = Mock::start().await;
    let text = format!("MOCK-SCENARIO:missing-usage {}", "x".repeat(35_000));
    let chars = text.chars().count() as u64;
    let run = decide(&mock, &text, &three_questions()).await;
    assert_eq!(mock.requests_to("decisions").len(), 1);
    for (name, result) in &run.results {
        assert_eq!(result.status(), AiStatus::Success, "{name}");
    }
    let rec = &run.requests[0];
    assert_eq!(rec.meta.usage, None);
    let record = request_record(
        &ctx(),
        &decisions_billing_rates(&table_rates()),
        MicroUsd(0),
        &rec.request_outcome(),
    );
    assert_eq!(record.input_tokens, None, "Input Tokens null, nicht 0");
    assert_eq!(record.output_tokens, None, "Output Tokens null, nicht 0");
    assert_eq!(record.price_source, PriceSource::Estimate);
    // Schätzung nach AI-012: mindestens ⌈Zeichen der Eingabe / 3,5⌉ (Anweisungsanteil, Fragentexte kommen hinzu).
    let estimated = record
        .estimated_input_tokens
        .expect("Schätzung protokolliert");
    assert!(
        estimated >= (chars * 2).div_ceil(7),
        "{estimated} < ⌈{chars}/3,5⌉"
    );
    let expected = cost_of(
        &TokenCounts::plain(estimated, 0),
        FeeCounts::one_request(0),
        &table_rates(),
        false,
    );
    assert_eq!(record.cost, expected.cost, "Cost = Schätzbetrag");
    assert!(record.cost.0 > 0, "Schätzbetrag geht in den Verbrauch ein");
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 8: Unbekannte zusätzliche Felder in der Antwort werden ignoriert; ein unbekannter `type` →
// `Error: Invalid output` für diese Frage.

// spec: AI-004
// Kriterium 8: Zusatzfelder ignoriert (Success), unbekannter type → Error: Invalid output nur für diese Frage.
#[tokio::test]
async fn k8_unknown_fields_ignored_unknown_type_invalid_for_that_question() {
    let mock = Mock::start().await;
    let questions = vec![
        Question::predicate("extra", "Soft 404?"),
        Question::predicate("unknown", "YMYL?"),
        Question::choice("plain", "Seitentyp?", &CHOICES),
    ];
    let run = decide(&mock, "MOCK-ID:k02a-forward Inhalt", &questions).await;
    let s = statuses(&run);
    assert_eq!(s["extra"], "Success", "{s:?}");
    match run.result("extra").unwrap().answer() {
        Some(DecisionAnswer::Predicate { probability }) => assert_eq!(*probability, 0.4),
        other => panic!("{other:?}"),
    }
    assert_eq!(s["unknown"], "Error: Invalid output");
    assert_eq!(s["plain"], "Success");
}

// spec: AI-004
// Kriterium 8 (Antwortebene): unbekannte Top-Level-Felder der Antwort werden ignoriert.
#[tokio::test]
async fn k8_unknown_top_level_fields_are_ignored() {
    let server = LocalServer::start(|_| {
        Canned::json(
            200,
            json!({
                "id": "decision_local",
                "object": "decision",
                "beta_notice": {"version": "2026-11", "flags": [1, 2]},
                "answers": [{"name": "q1", "type": "predicate", "probability": 0.7, "rationale": null}],
                "usage": {"input_tokens": 5, "output_tokens": 1, "future": {"x": 1}}
            }),
        )
    })
    .await;
    let run = OpenAiDecisions::new(server.http())
        .decide(
            &fast_limiter(),
            &DecisionInput::text("Inhalt"),
            &[Question::predicate("q1", "Soft 404?")],
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.result("q1").unwrap().status(), AiStatus::Success);
    assert_eq!(
        run.requests[0].meta.usage.as_ref().unwrap().input_tokens,
        Some(5)
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 9: Bildeingaben, die nicht als Data-URL vorliegen, werden vor dem Senden geladen und kodiert; Bilder über
// 20 MB → `Skipped (image too large)`.

// spec: AI-004
// Kriterium 9: Bild per HTTP-URL wird geladen und als Data-URL gesendet; keine HTTP-URL, keine file_id im Request.
#[tokio::test]
async fn k9_url_image_is_loaded_and_sent_as_data_url() {
    let mock = Mock::start().await;
    let png = tiny_png();
    let png2 = png.clone();
    let images = LocalServer::start(move |_| Canned::bytes(200, "image/png", png2.clone())).await;
    let image_url = images.url("/bild.png");
    let run = OpenAiDecisions::new(mock.http())
        .decide_with_images(
            &fast_limiter(),
            Some("Passt der Alt-Text?".into()),
            vec![ImageSource::Url(image_url.clone())],
            &[Question::predicate(
                "alt",
                "Beschreibt der Alt-Text das Bild?",
            )],
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.result("alt").unwrap().status(), AiStatus::Success);
    assert_eq!(
        images.requests_to("/bild.png").len(),
        1,
        "Bild vor dem Senden geladen"
    );
    let bodies = mock.bodies_to("decisions");
    assert_eq!(bodies.len(), 1);
    let urls = image_urls(&bodies[0]);
    assert_eq!(urls, vec![format!("data:image/png;base64,{}", b64(&png))]);
    assert!(
        !bodies[0].to_string().contains(&image_url),
        "keine HTTP-URL gesendet"
    );
    assert!(!has_key(&bodies[0], "file_id"));
}

// spec: AI-004
// spec: AI-020
// Kriterium 9: Bild über 20 MB (rohe Bytes) → alle Fragen `Skipped (image too large)`, 0 Requests.
#[tokio::test]
async fn k9_image_over_20_mb_is_skipped_without_request() {
    let mock = Mock::start().await;
    let big = png_of_size(21 * 1024 * 1024);
    let run = OpenAiDecisions::new(mock.http())
        .decide_with_images(
            &fast_limiter(),
            Some("Bild".into()),
            vec![ImageSource::Bytes {
                media_type: Some("image/png".into()),
                bytes: big,
            }],
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    for (name, result) in &run.results {
        assert_eq!(result.status(), AiStatus::SkippedImageTooLarge, "{name}");
        assert_eq!(result.status().to_string(), "Skipped (image too large)");
    }
    assert_eq!(mock.requests_to("decisions").len(), 0);
}

// spec: AI-004
// Kriterium 9: Bild über 20 MB per URL → nach dem Laden `Skipped (image too large)`, 0 Requests an Decisions.
#[tokio::test]
async fn k9_url_image_over_20_mb_is_skipped_without_request() {
    let mock = Mock::start().await;
    let big = png_of_size(21 * 1024 * 1024);
    let images = LocalServer::start(move |_| Canned::bytes(200, "image/png", big.clone())).await;
    let run = OpenAiDecisions::new(mock.http())
        .decide_with_images(
            &fast_limiter(),
            Some("Bild".into()),
            vec![ImageSource::Url(images.url("/gross.png"))],
            &[Question::predicate("alt", "Alt passt?")],
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(
        run.result("alt").unwrap().status(),
        AiStatus::SkippedImageTooLarge
    );
    assert_eq!(mock.requests_to("decisions").len(), 0);
}

// spec: AI-004
// Kriterium 9: Bild deutlich unter 20 MB (19.000.000 Byte) wird gesendet (eigener Testserver, weil der ai-mock
// Request-Bodies dieser Größe nicht annimmt).
#[tokio::test]
async fn k9_image_below_20_mb_is_sent() {
    let server = LocalServer::start(|_| {
        Canned::json(
            200,
            json!({"answers": [{"name": "alt", "type": "predicate", "probability": 0.6}],
                   "usage": {"input_tokens": 100, "output_tokens": 1}}),
        )
    })
    .await;
    let bytes = png_of_size(19_000_000);
    let run = OpenAiDecisions::new(server.http())
        .decide_with_images(
            &fast_limiter(),
            Some("Bild".into()),
            vec![ImageSource::Bytes {
                media_type: Some("image/png".into()),
                bytes: bytes.clone(),
            }],
            &[Question::predicate("alt", "Alt passt?")],
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.result("alt").unwrap().status(), AiStatus::Success);
    let sent = server.requests_to("/v1/decisions");
    assert_eq!(sent.len(), 1);
    let urls = image_urls(&sent[0].json());
    assert_eq!(urls.len(), 1);
    assert_eq!(urls[0], format!("data:image/png;base64,{}", b64(&bytes)));
}

// ---------------------------------------------------------------------------------------------------------------
// Eingaben/Konfiguration AI-004: höchstens 10 Fragen je Request (einstellbar 1–10, Standard 10); AI-017: ein Timeout
// gilt für alle Fragen eines Requests; AI-001: Fähigkeit Decisions (Text und Bild).

// spec: AI-004
// Eingaben: 23 Fragen → Requests mit höchstens 10 Fragen (10, 10, 3), jede Frage genau einmal, alle beantwortet.
#[tokio::test]
async fn config_at_most_ten_questions_per_request() {
    let mock = Mock::start().await;
    let questions: Vec<Question> = (1..=23)
        .map(|i| Question::predicate(format!("q{i:02}"), format!("Frage {i}?")))
        .collect();
    let run = decide(&mock, "Inhalt", &questions).await;
    let bodies = mock.bodies_to("decisions");
    let sizes: Vec<usize> = bodies
        .iter()
        .map(|b| b["questions"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes, vec![10, 10, 3]);
    let mut names: Vec<String> = bodies
        .iter()
        .flat_map(|b| b["questions"].as_array().unwrap().clone())
        .map(|q| q["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    let mut expected: Vec<String> = questions.iter().map(|q| q.name.clone()).collect();
    expected.sort();
    assert_eq!(names, expected, "jede Frage genau einmal");
    assert_eq!(run.results.len(), 23);
    assert!(run
        .results
        .iter()
        .all(|(_, r)| r.status() == AiStatus::Success));
}

// spec: AI-004
// Eingaben: Höchstzahl einstellbar 1–10 (Standard 10); 4 → Requests mit höchstens 4 Fragen.
#[tokio::test]
async fn config_max_questions_is_adjustable_between_one_and_ten() {
    assert!(MaxQuestionsPerRequest::new(0).is_none());
    assert!(MaxQuestionsPerRequest::new(11).is_none());
    assert_eq!(MaxQuestionsPerRequest::default().get(), 10);
    assert_eq!(MaxQuestionsPerRequest::new(1).unwrap().get(), 1);
    let mock = Mock::start().await;
    let questions: Vec<Question> = (1..=9)
        .map(|i| Question::predicate(format!("q{i}"), "Frage?"))
        .collect();
    let options = DecideOptions {
        max_questions: MaxQuestionsPerRequest::new(4).unwrap(),
        ..DecideOptions::default()
    };
    OpenAiDecisions::new(mock.http())
        .decide(
            &fast_limiter(),
            &DecisionInput::text("Inhalt"),
            &questions,
            &options,
        )
        .await;
    let sizes: Vec<usize> = mock
        .bodies_to("decisions")
        .iter()
        .map(|b| b["questions"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes, vec![4, 4, 1]);
}

// spec: AI-004
// spec: AI-017
// AI-017: Bei einem Request mit mehreren Decision-Fragen gilt ein Timeout für alle Fragen dieses Requests.
#[tokio::test]
async fn timeout_applies_to_all_questions_of_the_request() {
    let mock = Mock::start().await;
    let run = OpenAiDecisions::new(mock.http())
        .decide(
            &timeout_limiter(Duration::from_secs(1)),
            &DecisionInput::text("MOCK-SCENARIO:hang Inhalt"),
            &three_questions(),
            &DecideOptions::default(),
        )
        .await;
    assert_eq!(run.requests.len(), 1);
    for (name, result) in &run.results {
        assert_eq!(result.status(), AiStatus::Timeout { seconds: 1 }, "{name}");
        assert_eq!(result.status().to_string(), "Error: Timeout nach 1 s");
    }
}

// spec: AI-004
// spec: AI-001
// AI-001 Fähigkeiten: OpenAI Decisions – Decisions (Text und Bild), sonst nichts.
#[test]
fn capabilities_of_decisions_match_matrix() {
    let c = DECISIONS_CAPABILITIES;
    assert!(c.decisions);
    assert!(
        !c.structured_output && !c.embeddings && !c.batch && !c.prices_via_api && !c.local,
        "{c:?}"
    );
}

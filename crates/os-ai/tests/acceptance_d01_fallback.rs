//! Unabhängige Abnahme M4-D01 – Ersatzweg über strukturierte Ausgabe (AI-034 mit Nachträgen 24.11. und 26.11.2026),
//! Schema-Regeln (AI-021), Reasoning-Stufe und `max_tokens` (AI-011 Nachtrag), gebündelter Abbruch an der
//! Ausgabegrenze (AI-020 Nachtrag). Erwartungen nur aus `docs/spec/08-ai.md`.
// acceptance: independent
mod acceptance_d01_support;

use std::collections::BTreeMap;

use acceptance_d01_support as s;
use os_ai::adapters::openai_decisions::DecisionAnswer;
use os_ai::catalog::Endpoint;
use os_ai::decisions::exec::{is_self_reported, RunOptions};
use os_ai::decisions::fallback::{answer_schema, bundle_schema, parse_answer, FallbackResult};
use os_ai::decisions::format::QuestionDef;
use os_ai::decisions::routing::RoutingSetting;
use serde_json::{json, Value};

const FB_VALUES: [&str; 4] = ["product", "category", "other", "article"];

fn fb_choice() -> QuestionDef {
    s::choice("d01t-fb-q", "Fallback Q", &FB_VALUES, &["title"])
}

fn fb_score() -> QuestionDef {
    s::score("d01t-fb-s", "Fallback S", &["l0", "l1", "l2", "l3", "l4"], &["title"])
}

fn fb_predicate() -> QuestionDef {
    s::predicate("d01t-fb-p", "Fallback P", &["title"])
}

fn claude() -> RoutingSetting {
    s::structured("anthropic", "mock-claude", Endpoint::Messages)
}

/// Prüft rekursiv die Verträglichkeit mit dem strikten Modus (AI-021): jedes Objekt führt alle Properties in
/// `required` und `additionalProperties: false`.
fn assert_strict(schema: &Value, at: &str) {
    if schema["type"] == "object" {
        let props = schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{at}: Objekt ohne properties"));
        let mut required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap_or_else(|| panic!("{at}: required fehlt"))
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        required.sort();
        let mut keys: Vec<&str> = props.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(required, keys, "{at}: nicht alle Properties in required");
        assert_eq!(
            schema["additionalProperties"],
            Value::Bool(false),
            "{at}: additionalProperties"
        );
        for (k, v) in props {
            assert_strict(v, &format!("{at}/{k}"));
        }
    }
}

fn prop_keys(schema: &Value) -> Vec<String> {
    schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

// spec: AI-034 [M4]
// acceptance: independent
#[test]
fn ai034_answer_schemas_per_type() {
    let p = answer_schema(&fb_predicate());
    let mut keys = prop_keys(&p);
    keys.sort();
    assert_eq!(keys, ["abstain", "probability"]);
    assert_eq!(p["properties"]["probability"]["type"], "number");
    assert_eq!(p["properties"]["abstain"]["type"], "boolean");
    assert_strict(&p, "predicate");

    let c = answer_schema(&fb_choice());
    let mut keys = prop_keys(&c);
    keys.sort();
    assert_eq!(keys, ["abstain", "probabilities"]);
    let mut values = prop_keys(&c["properties"]["probabilities"]);
    values.sort();
    let mut expected: Vec<String> = FB_VALUES.iter().map(|v| v.to_string()).collect();
    expected.sort();
    assert_eq!(values, expected, "genau eine Pflicht-Property je value");
    for v in FB_VALUES {
        assert_eq!(c["properties"]["probabilities"]["properties"][v]["type"], "number");
    }
    assert_strict(&c, "choice");

    let sc = answer_schema(&fb_score());
    let mut idx = prop_keys(&sc["properties"]["probabilities"]);
    idx.sort();
    assert_eq!(idx, ["0", "1", "2", "3", "4"], "eine Pflicht-Property je Level-Index");
    assert_strict(&sc, "score");

    let b = bundle_schema(&[&fb_predicate(), &fb_choice(), &fb_score()]);
    assert_strict(&b, "bundle");
    assert!(b.to_string().contains("abstain"));
}

fn answered(result: FallbackResult) -> (DecisionAnswer, bool) {
    match result {
        FallbackResult::Answered { answer, normalized } => (answer, normalized),
        other => panic!("erwartet Antwort, erhalten {other:?}"),
    }
}

// spec: AI-034 [M4]
// acceptance: independent
#[test]
fn ai034_derivation_choice_score_confidence() {
    let (a, normalized) = answered(parse_answer(
        &fb_choice(),
        &json!({"probabilities": {"product": 0.6, "category": 0.3, "other": 0.1, "article": 0.0}, "abstain": false}),
    ));
    assert!(!normalized);
    match a {
        DecisionAnswer::Choice {
            choice, confidence, ..
        } => {
            assert_eq!(choice, "product");
            assert!(s::approx(confidence, 0.60));
        }
        other => panic!("{other:?}"),
    }
    let (a, _) = answered(parse_answer(
        &fb_score(),
        &json!({"probabilities": {"0": 0.1, "1": 0.2, "2": 0.4, "3": 0.2, "4": 0.1}, "abstain": false}),
    ));
    match a {
        DecisionAnswer::Score {
            score,
            level,
            confidence,
            ..
        } => {
            assert!(s::approx(score, 2.0), "Score = Σ Index × Wahrscheinlichkeit");
            assert_eq!(level, "l2");
            assert!(s::approx(confidence, 0.4));
        }
        other => panic!("{other:?}"),
    }
    let (a, _) = answered(parse_answer(
        &fb_predicate(),
        &json!({"probability": 0.35, "abstain": false}),
    ));
    match a {
        DecisionAnswer::Predicate { probability } => assert!(s::approx(probability, 0.35)),
        other => panic!("{other:?}"),
    }
}

// spec: AI-034 [M4]
// acceptance: independent
#[test]
fn ai034_tie_takes_first_defined_value() {
    let (a, _) = answered(parse_answer(
        &fb_choice(),
        &json!({"probabilities": {"article": 0.0, "other": 0.2, "category": 0.4, "product": 0.4}, "abstain": false}),
    ));
    match a {
        DecisionAnswer::Choice { choice, .. } => assert_eq!(choice, "product"),
        other => panic!("{other:?}"),
    }
}

// spec: AI-034 [M4]
// acceptance: independent
#[test]
fn ai034_normalization_and_invalid_numbers() {
    let (a, normalized) = answered(parse_answer(
        &fb_choice(),
        &json!({"probabilities": {"product": 1.2, "category": 0.6, "other": 0.2, "article": 0.0}, "abstain": false}),
    ));
    assert!(normalized, "Summe 2 wird durch die Summe geteilt");
    match a {
        DecisionAnswer::Choice {
            choice,
            confidence,
            probabilities,
            ..
        } => {
            assert_eq!(choice, "product");
            assert!(s::approx(confidence, 0.6));
            let sum: f64 = probabilities.iter().map(|p| p.probability).sum();
            assert!(s::approx(sum, 1.0));
        }
        other => panic!("{other:?}"),
    }
    for bad in [
        json!({"probabilities": {"product": 0.0, "category": 0.0, "other": 0.0, "article": 0.0}, "abstain": false}),
        json!({"probabilities": {"product": -0.1, "category": 1.1, "other": 0.0, "article": 0.0}, "abstain": false}),
    ] {
        assert!(
            matches!(parse_answer(&fb_choice(), &bad), FallbackResult::Invalid(_)),
            "Summe 0 bzw. negativer Wert nicht abgewiesen: {bad}"
        );
    }
}

// spec: AI-034 [M4]
// acceptance: independent
#[test]
fn ai034_abstain_and_schema_violations() {
    assert_eq!(
        parse_answer(&fb_predicate(), &json!({"probability": 0.9, "abstain": true})),
        FallbackResult::Abstained,
        "abstain = true → Zahlen werden ignoriert"
    );
    for bad in [
        json!({"probabilities": {"product": 0.6, "category": 0.3, "other": 0.1, "article": 0.0}}),
        json!({"probabilities": {"product": 0.6, "category": 0.3, "other": 0.1}, "abstain": false}),
        json!({"probabilities": {"product": 0.6, "category": 0.3, "other": 0.1, "article": 0.0}, "abstain": false, "note": "x"}),
        json!({"probabilities": {"product": 0.6, "category": 0.3, "other": 0.1, "article": 0.0, "shop": 0.0}, "abstain": false}),
    ] {
        assert!(
            matches!(parse_answer(&fb_choice(), &bad), FallbackResult::Invalid(_)),
            "Schemaverstoß angenommen: {bad}"
        );
    }
}

// spec: AI-034 [M4]
// acceptance: independent
#[tokio::test]
async fn ai034_anthropic_fallback_end_to_end() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    let items = [s::page(1, "Product MOCK-ID:d01t-fb-choice", "Text.")];
    bench
        .run(&[s::active_with(fb_choice(), claude())], &items, s::options())
        .await;
    let row = bench.answer(1, "d01t-fb-q");
    assert_eq!(row.status, "Success");
    assert_eq!(row.source.as_deref(), Some("self-reported"));
    assert_eq!(row.model.as_deref(), Some("mock-claude"));
    assert!(is_self_reported(&row));
    let answer = s::answer_json(&row);
    assert_eq!(answer["choice"], "product");
    assert!(s::approx(answer["confidence"].as_f64().unwrap(), 0.60));
    let bodies = bench.mock.bodies(s::MESSAGES);
    assert_eq!(bodies.len(), 1);
    let body = &bodies[0];
    // Schema über output_config.format mit abstain als Pflichtfeld.
    let format = &body["output_config"]["format"];
    assert!(!format.is_null(), "kein output_config.format: {body}");
    let schema_text = format.to_string();
    assert!(schema_text.contains("\"abstain\""));
    assert!(schema_text.contains("\"required\""));
    // Reasoning-Stufe `off` (sicher aus) und max_tokens = 2 × (64 + 16 × 4) = 256.
    assert_eq!(body["thinking"]["type"], "disabled");
    assert_eq!(body["max_tokens"], 256);
    // Systemanweisung bittet um Wahrscheinlichkeiten.
    assert!(body.to_string().to_lowercase().contains("probabilit"));
}

// spec: AI-034 [M4]
// acceptance: independent
#[tokio::test]
async fn ai011_max_tokens_from_answer_share_w() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    // choice mit 14 Werten → W = 14, A = 288, max_tokens = 576.
    let pt = s::active_with(s::page_type(&["title"]), claude());
    bench
        .run(
            std::slice::from_ref(&pt),
            &[s::page(1, "Fourteen MOCK-ID:d01t-fb-14", "Text.")],
            s::options(),
        )
        .await;
    assert_eq!(bench.status(1, "page-type"), "Success");
    let bodies = bench.mock.bodies(s::MESSAGES);
    assert_eq!(bodies[0]["max_tokens"], 576);
    // Gebündelt mit einem predicate → W = 16, max_tokens = 2 × (64 + 256) = 640.
    let pred = s::active_with(
        s::predicate("d01t-b-pred", "Bundle Pred", &["title"]),
        claude(),
    );
    bench
        .run(
            &[pt, pred],
            &[s::page(2, "Bundled MOCK-ID:d01t-fb-14b", "Text.")],
            s::options(),
        )
        .await;
    let bodies = bench.mock.bodies(s::MESSAGES);
    assert_eq!(bodies.len(), 2, "beide Fragen in einem Request");
    assert_eq!(bodies[1]["max_tokens"], 640);
    assert_eq!(bench.status(2, "page-type"), "Success");
    assert_eq!(bench.status(2, "d01t-b-pred"), "Success");
}

// spec: AI-034 [M4]
// acceptance: independent
#[tokio::test]
async fn ai034_openrouter_m_r1_reasoning_none() {
    let bench = s::Bench::new(&["openrouter"], 1).await;
    let q = s::active_with(
        fb_choice(),
        s::structured("openrouter", "mock/text-small", Endpoint::ChatCompletions),
    );
    bench
        .run(
            &[q],
            &[s::page(1, "Router MOCK-ID:d01t-fb-choice", "Text.")],
            s::options(),
        )
        .await;
    let bodies = bench.mock.bodies(s::COMPLETIONS);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["reasoning"]["effort"], "none");
    assert!(bodies[0]["response_format"].to_string().contains("\"abstain\""));
    assert_eq!(bench.status(1, "d01t-fb-q"), "Success");
}

fn bundle_questions() -> Vec<os_ai::decisions::exec::ActiveQuestion> {
    vec![
        s::active_with(s::predicate("d01t-b-pred", "B Pred", &["title"]), claude()),
        s::active_with(
            s::choice("d01t-b-choice", "B Choice", &FB_VALUES, &["title"]),
            claude(),
        ),
        s::active_with(
            s::score("d01t-b-score", "B Score", &["s0", "s1", "s2"], &["title"]),
            claude(),
        ),
    ]
}

// spec: AI-034 [M4]
// acceptance: independent
#[tokio::test]
async fn ai034_bundled_fallback_one_request() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    bench
        .run(
            &bundle_questions(),
            &[s::page(1, "Bundle MOCK-ID:d01t-fb-bundle", "Text.")],
            s::options(),
        )
        .await;
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 1);
    let pred = s::answer_json(&bench.answer(1, "d01t-b-pred"));
    assert!(s::approx(pred["probability"].as_f64().unwrap(), 0.7));
    let choice = s::answer_json(&bench.answer(1, "d01t-b-choice"));
    assert_eq!(choice["choice"], "category");
    let score = s::answer_json(&bench.answer(1, "d01t-b-score"));
    assert!(s::approx(score["score"].as_f64().unwrap(), 1.1));
    assert_eq!(score["level"], "s1");
}

// spec: AI-034 [M4]
// acceptance: independent
#[tokio::test]
async fn ai034_bundled_output_limit_all_questions_one_request() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    bench.mock.scenario("anthropic", "output-limit", 100).await;
    bench
        .run(
            &bundle_questions(),
            &[s::page(1, "Limit MOCK-ID:d01t-fb-bundle", "Text.")],
            s::options(),
        )
        .await;
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 1, "genau 1 Request, kein Reparaturversuch");
    for q in ["d01t-b-pred", "d01t-b-choice", "d01t-b-score"] {
        assert_eq!(bench.status(1, q), "Error: Output limit reached", "{q}");
    }
    // Ohne Bündelung: eine Frage je Request.
    let unbundled = s::Bench::new(&["anthropic"], 1).await;
    unbundled.mock.scenario("anthropic", "output-limit", 100).await;
    let options = RunOptions {
        bundle_fallback: false,
        ..RunOptions::default()
    };
    unbundled
        .run(
            &bundle_questions(),
            &[s::page(1, "Limit MOCK-ID:d01t-fb-bundle", "Text.")],
            options,
        )
        .await;
    assert_eq!(unbundled.mock.posts(s::MESSAGES).len(), 3);
}

// spec: AI-034 [M4]
// acceptance: independent
#[tokio::test]
async fn ai034_abstain_is_refusal_model_abstained() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    let q = s::active_with(fb_predicate(), claude());
    bench
        .run(
            &[q],
            &[s::page(1, "Abstain MOCK-ID:d01t-fb-abstain", "Text.")],
            s::options(),
        )
        .await;
    let row = bench.answer(1, "d01t-fb-p");
    assert_eq!(row.status, "Refusal");
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 1, "Refusal wird nicht wiederholt");
    let reason_in_log = bench
        .sent_rows()
        .iter()
        .any(|r| r.record.refusal_reason.as_deref() == Some("model abstained"));
    let reason_in_answer = row
        .answer_json
        .as_deref()
        .is_some_and(|a| a.contains("model abstained"));
    assert!(
        reason_in_log || reason_in_answer,
        "Grund „model abstained“ nicht vermerkt"
    );
    // Refusal ist kein Fehler (Filter „Errors“).
    assert!(!os_ai::decisions::exec::is_error_status(&row.status));
}

// spec: AI-034 [M4]
// acceptance: independent
#[tokio::test]
async fn ai034_sum_zero_and_negative_are_invalid_output() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    let q = s::active_with(fb_choice(), claude());
    bench
        .run(
            &[q],
            &[
                s::page(1, "Zero MOCK-ID:d01t-fb-sum0", "Text."),
                s::page(2, "Negative MOCK-ID:d01t-fb-neg", "Text."),
                s::page(3, "Tie MOCK-ID:d01t-fb-tie", "Text."),
                s::page(4, "Sum two MOCK-ID:d01t-fb-sum2", "Text."),
            ],
            s::options(),
        )
        .await;
    assert_eq!(bench.status(1, "d01t-fb-q"), "Error: Invalid output");
    assert_eq!(bench.status(2, "d01t-fb-q"), "Error: Invalid output");
    assert_eq!(bench.status(3, "d01t-fb-q"), "Success");
    assert_eq!(s::answer_json(&bench.answer(3, "d01t-fb-q"))["choice"], "product");
    let normalized = s::answer_json(&bench.answer(4, "d01t-fb-q"));
    assert!(s::approx(s::probability_sum(&normalized), 1.0));
    assert!(s::approx(normalized["confidence"].as_f64().unwrap(), 0.6));
}

// spec: AI-021 [M4]
// acceptance: independent
#[tokio::test]
async fn ai021_schema_violation_exactly_one_repair() {
    // Antwort ohne `abstain` → Schemaverstoß → genau ein Reparaturversuch → `Error: Invalid output`.
    let bench = s::Bench::new(&["anthropic"], 1).await;
    let q = s::active_with(fb_choice(), claude());
    bench
        .run(
            &[q],
            &[s::page(1, "No abstain MOCK-ID:d01t-fb-noabstain", "Text.")],
            s::options(),
        )
        .await;
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 2);
    assert_eq!(bench.status(1, "d01t-fb-q"), "Error: Invalid output");
    let rows = bench.sent_rows();
    assert!(
        rows.iter().any(|r| r.record.error_message.is_some()),
        "Validator-Fehler fehlt im Protokoll"
    );
    // Szenario `schema-violation` dauerhaft → 2 Requests, `Error: Invalid output`.
    let bench = s::Bench::new(&["anthropic"], 1).await;
    bench.mock.scenario("anthropic", "schema-violation", 100).await;
    let q = s::active_with(fb_choice(), claude());
    bench
        .run(
            &[q],
            &[s::page(1, "Violation MOCK-ID:d01t-fb-choice", "Text.")],
            s::options(),
        )
        .await;
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 2);
    assert_eq!(bench.status(1, "d01t-fb-q"), "Error: Invalid output");
}

// spec: AI-021 [M4]
// acceptance: independent
#[tokio::test]
async fn ai021_malformed_once_then_ok_same_reasoning_and_max_tokens() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    bench.mock.scenario("anthropic", "malformed-json", 1).await;
    let q = s::active_with(fb_choice(), claude());
    bench
        .run(
            &[q],
            &[s::page(1, "Repair MOCK-ID:d01t-fb-choice", "Text.")],
            s::options(),
        )
        .await;
    let bodies = bench.mock.bodies(s::MESSAGES);
    assert_eq!(bodies.len(), 2, "Protokoll zeigt 2 Requests für den Eintrag");
    assert_eq!(bench.status(1, "d01t-fb-q"), "Success");
    assert_eq!(bodies[0]["max_tokens"], bodies[1]["max_tokens"]);
    assert_eq!(bodies[0]["thinking"], bodies[1]["thinking"]);
}

fn as_map(answer: &Value) -> BTreeMap<String, f64> {
    match &answer["probabilities"] {
        Value::Array(items) => items
            .iter()
            .map(|p| {
                let key = match &p["value"] {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                (key, p["probability"].as_f64().unwrap())
            })
            .collect(),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| (k.clone(), v.as_f64().unwrap()))
            .collect(),
        _ => BTreeMap::new(),
    }
}

fn keys(answer: &Value) -> Vec<String> {
    let mut k: Vec<String> = answer.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

// spec: AI-034 [M4]
// acceptance: independent
#[tokio::test]
async fn ai034_both_paths_give_equivalent_results() {
    let choice_q = s::choice("d01t-eq-choice", "Eq Choice", &FB_VALUES, &["title"]);
    let score_q = s::score("d01t-eq-score", "Eq Score", &["low", "mid", "high"], &["title"]);
    let items = [
        s::page(1, "Equal MOCK-ID:d01t-eq", "Text."),
        s::page(2, "Equal score MOCK-ID:d01t-eq-score", "Text."),
    ];
    let decisions = s::Bench::new(&["openai"], 1).await;
    decisions
        .run(&[s::active(choice_q.clone())], &items[..1], s::options())
        .await;
    decisions
        .run(&[s::active(score_q.clone())], &items[1..], s::options())
        .await;
    let fallback = s::Bench::new(&["anthropic"], 1).await;
    fallback
        .run(&[s::active(choice_q)], &items[..1], s::options())
        .await;
    fallback
        .run(&[s::active(score_q)], &items[1..], s::options())
        .await;
    for (url, q) in [(1, "d01t-eq-choice"), (2, "d01t-eq-score")] {
        let a = decisions.answer(url, q);
        let b = fallback.answer(url, q);
        assert_eq!(a.status, "Success", "{q} Decisions");
        assert_eq!(b.status, "Success", "{q} Ersatzweg");
        assert_eq!(a.source.as_deref(), Some("decisions-api"));
        assert_eq!(b.source.as_deref(), Some("self-reported"));
        let ja = s::answer_json(&a);
        let jb = s::answer_json(&b);
        assert_eq!(keys(&ja), keys(&jb), "{q}: gleiche Felder auf beiden Wegen");
        assert_eq!(ja["type"], jb["type"]);
        assert!(s::approx(
            ja["confidence"].as_f64().unwrap(),
            jb["confidence"].as_f64().unwrap()
        ));
        let (ma, mb) = (as_map(&ja), as_map(&jb));
        assert_eq!(ma.len(), mb.len(), "{q}: probabilities");
        for (k, v) in &ma {
            let other = mb.get(k).copied().unwrap_or(f64::NAN);
            assert!(s::approx(*v, other), "{q}: {k} {v} gegen {other}");
        }
    }
    let ca = s::answer_json(&decisions.answer(1, "d01t-eq-choice"));
    let cb = s::answer_json(&fallback.answer(1, "d01t-eq-choice"));
    assert_eq!(ca["choice"], cb["choice"]);
    let sa = s::answer_json(&decisions.answer(2, "d01t-eq-score"));
    let sb = s::answer_json(&fallback.answer(2, "d01t-eq-score"));
    assert!(s::approx(sa["score"].as_f64().unwrap(), sb["score"].as_f64().unwrap()));
    assert_eq!(sa["level"], sb["level"]);
}

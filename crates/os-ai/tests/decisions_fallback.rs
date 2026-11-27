//! M4-D01: Ersatzweg über strukturierte Ausgabe (AI-034) mit den Nachträgen Q00 (Stufe, `max_tokens`, gebündelter
//! Abbruch) und dem Statuswert `Error: Output limit reached` im Lauf (AI-020, Nachtrag Q00-H1).
#[path = "decisions_support.rs"]
mod support;

use os_ai::adapters::openai_decisions::DecisionAnswer;
use os_ai::catalog::{AvailableModel, Endpoint, ModelRef};
use os_ai::decisions::exec::{ActiveQuestion, RunControl, RunEnd, RunItem, RunOptions};
use os_ai::decisions::fallback::{self, answer_schema, parse_answer, FallbackResult};
use os_ai::decisions::format::QuestionDef;
use os_ai::decisions::routing::RoutingSetting;
use os_ai::schema::strict_violations;
use serde_json::{json, Value};
use support::*;

const PROVIDERS: [&str; 3] = ["anthropic", "openai", "openrouter"];

fn structured(def: QuestionDef, provider: &str, model: &str, endpoint: Endpoint) -> ActiveQuestion {
    ActiveQuestion {
        def,
        routing: RoutingSetting::StructuredOutput {
            model: ModelRef::new(provider, model, endpoint),
        },
    }
}

/// Umgebung mit Anthropic-Modell A (`mock-claude`, Fähigkeiten aus der Mock-Modellliste) und den Standardmodellen.
async fn env_with_live_models() -> Env {
    let mut env = Env::new(&PROVIDERS).await;
    let anthropic = env.anthropic.list_models().await.unwrap();
    let openrouter = env.openrouter.list_models().await.unwrap();
    let mut extra: Vec<AvailableModel> = anthropic
        .iter()
        .map(|m| AvailableModel {
            model: m.to_model(),
            max_output_tokens: m.max_tokens,
        })
        .collect();
    extra.extend(openrouter.iter().map(|m| AvailableModel {
        model: m.to_model(),
        max_output_tokens: m.max_completion_tokens,
    }));
    let mut responses = model(
        "openai",
        "gpt-6-luna",
        "responses",
        caps(false, true, false),
    );
    responses.reasoning_levels = vec![
        os_ai::provider::ReasoningLevel::Off,
        os_ai::provider::ReasoningLevel::Low,
        os_ai::provider::ReasoningLevel::Medium,
    ];
    extra.push(AvailableModel {
        model: responses,
        max_output_tokens: None,
    });
    env.providers = providers(&PROVIDERS, extra);
    env
}

async fn run_one(
    env: &Env,
    qs: &[ActiveQuestion],
    items: &[RunItem],
    options: RunOptions,
) -> (TempDir, os_store::CrawlFile) {
    let dir = TempDir::new("fb");
    let (file, run) = crawl_file(&dir.0, items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let report = env
        .run(
            file.conn(),
            run,
            qs,
            items,
            &ledger,
            RunControl::new(),
            options,
        )
        .run()
        .await
        .unwrap();
    assert_eq!(report.end, RunEnd::Completed);
    (dir, file)
}

fn row(
    file: &os_store::CrawlFile,
    url_id: i64,
    q: &QuestionDef,
) -> os_store::ai::answers::AnswerRow {
    os_store::ai::answers::get(file.conn(), url_id, &q.id, &q.version.to_string(), "")
        .unwrap()
        .unwrap()
}

fn answer_json(file: &os_store::CrawlFile, url_id: i64, q: &QuestionDef) -> Value {
    serde_json::from_str(row(file, url_id, q).answer_json.as_deref().unwrap()).unwrap()
}

// spec: AI-034 [M4]
// Antwortschema je Frage: alle Properties in `required`, `additionalProperties: false`, `abstain` Pflichtfeld; mit dem
// strikten Modus aller Anbieter verträglich (Prüfer aus AI-021).
#[test]
fn schemas_are_strict_and_require_abstain() {
    for name in ["page-type", "quality", "soft-404", "alt-fits"] {
        let q = question(name);
        let schema = answer_schema(&q);
        assert!(
            strict_violations(&schema).is_empty(),
            "{name}: {:?}",
            strict_violations(&schema)
        );
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"abstain"));
        assert_eq!(schema["additionalProperties"], false);
    }
    let pt = answer_schema(&question("page-type"));
    assert_eq!(
        pt["properties"]["probabilities"]["required"]
            .as_array()
            .unwrap()
            .len(),
        14
    );
    let score = answer_schema(&question("quality"));
    assert_eq!(
        score["properties"]["probabilities"]["required"],
        json!(["0", "1", "2", "3", "4"])
    );
    assert_eq!(
        answer_schema(&question("soft-404"))["properties"]["probability"],
        json!({"type": "number"})
    );
    let bundle = fallback::bundle_schema(&[&question("page-type"), &question("soft-404")]);
    assert!(strict_violations(&bundle).is_empty());
    assert_eq!(bundle["required"], json!(["page_type", "soft_404"]));
}

// spec: AI-034 [M4]
// Ableitung: `choice` = höchste Wahrscheinlichkeit (Gleichstand: zuerst definierter Wert), `score` = Σ Index × p,
// Normierung durch die Summe, Summe 0 oder negative Werte → ungültig, `abstain` → Refusal, fehlendes `abstain` →
// Schemaverstoß.
#[test]
fn derivation_rules() {
    let pt = question("page-type");
    let probs = |pairs: &[(&str, f64)]| {
        let mut m = serde_json::Map::new();
        for v in pt.values() {
            let p = pairs.iter().find(|(k, _)| *k == v).map_or(0.0, |(_, p)| *p);
            m.insert(v, json!(p));
        }
        Value::Object(m)
    };
    let FallbackResult::Answered { answer, normalized } = parse_answer(
        &pt,
        &json!({"probabilities": probs(&[("product", 0.6), ("category", 0.3), ("other", 0.1)]), "abstain": false}),
    ) else {
        panic!()
    };
    assert!(!normalized);
    let DecisionAnswer::Choice {
        choice, confidence, ..
    } = &answer
    else {
        panic!()
    };
    assert_eq!(choice, "product");
    assert!((confidence - 0.6).abs() < 1e-12);
    let FallbackResult::Answered { answer, .. } = parse_answer(
        &pt,
        &json!({"probabilities": probs(&[("homepage", 0.5), ("category", 0.5)]), "abstain": false}),
    ) else {
        panic!()
    };
    assert!(matches!(&answer, DecisionAnswer::Choice { choice, .. } if choice == "homepage"));
    let FallbackResult::Answered { answer, normalized } = parse_answer(
        &pt,
        &json!({"probabilities": probs(&[("product", 1.2), ("other", 0.8)]), "abstain": false}),
    ) else {
        panic!()
    };
    assert!(normalized);
    assert!((answer.confidence().unwrap() - 0.6).abs() < 1e-12);
    assert!(matches!(
        parse_answer(&pt, &json!({"probabilities": probs(&[]), "abstain": false})),
        FallbackResult::Invalid(_)
    ));
    assert!(matches!(
        parse_answer(
            &pt,
            &json!({"probabilities": probs(&[("product", 1.2), ("other", -0.2)]), "abstain": false})
        ),
        FallbackResult::Invalid(_)
    ));
    assert_eq!(
        parse_answer(
            &pt,
            &json!({"probabilities": probs(&[("other", 1.0)]), "abstain": true})
        ),
        FallbackResult::Abstained
    );
    assert!(matches!(
        parse_answer(&pt, &json!({"probabilities": probs(&[("other", 1.0)])})),
        FallbackResult::Invalid(_)
    ));
    let q = question("quality");
    let FallbackResult::Answered { answer, .. } = parse_answer(
        &q,
        &json!({"probabilities": {"0": 0.1, "1": 0.2, "2": 0.4, "3": 0.2, "4": 0.1}, "abstain": false}),
    ) else {
        panic!()
    };
    let DecisionAnswer::Score { score, level, .. } = &answer else {
        panic!()
    };
    assert!((score - 2.0).abs() < 1e-9);
    assert_eq!(level, &q.levels.as_ref().unwrap()[2].label);
}

// spec: AI-034 [M4]
// Mock-Antwort `{"probabilities": {"product": 0.6, "category": 0.3, "other": 0.1, …0}, "abstain": false}` über
// Anthropic → `<Q> = product`, `<Q> Confidence = 0.60`, `<Q> Source = self-reported`; Score-Antwort → `Score = 2.00`,
// `Level` = Label von Index 2.
#[tokio::test]
async fn fallback_values_over_anthropic() {
    let env = env_with_live_models().await;
    let pt = question("page-type");
    let quality = question("quality");
    let qs = [
        structured(pt.clone(), "anthropic", "mock-claude", Endpoint::Messages),
        structured(
            quality.clone(),
            "anthropic",
            "mock-claude",
            Endpoint::Messages,
        ),
    ];
    let options = RunOptions {
        bundle_fallback: false,
        ..RunOptions::default()
    };
    let items = vec![
        text_item(1, "MOCK-ID:d01-fb-choice Seite"),
        text_item(2, "MOCK-ID:d01-fb-score Seite"),
    ];
    // Nur die passenden Paare prüfen: Frage 1 an URL 1, Frage 2 an URL 2.
    let (_d, file) = run_one(&env, &qs, &items, options).await;
    let r = row(&file, 1, &pt);
    assert_eq!(r.status, "Success");
    assert_eq!(r.source.as_deref(), Some("self-reported"));
    assert_eq!(r.model.as_deref(), Some("mock-claude"));
    let a = answer_json(&file, 1, &pt);
    assert_eq!(a["choice"], "product");
    assert!((a["confidence"].as_f64().unwrap() - 0.6).abs() < 1e-12);
    let s = answer_json(&file, 2, &quality);
    assert!((s["score"].as_f64().unwrap() - 2.0).abs() < 1e-9);
    assert_eq!(s["level"], quality.levels.as_ref().unwrap()[2].label);
    // Filter „Self-reported Probabilities“.
    let rows = os_store::ai::answers::list_for_url(file.conn(), 1).unwrap();
    assert!(rows
        .iter()
        .filter(|r| r.status == "Success")
        .all(os_ai::decisions::exec::is_self_reported));
}

// spec: AI-034 [M4]
// Summe 0 → `Error: Invalid output`; `abstain: true` → `Refusal` (Grund „model abstained“); Antwort ohne `abstain` →
// Schemaverstoß (AI-021: ein Reparaturversuch, dann `Error: Invalid output`).
#[tokio::test]
async fn invalid_zero_abstain_and_missing_abstain() {
    let env = env_with_live_models().await;
    let pt = question("page-type");
    let qs = [structured(
        pt.clone(),
        "anthropic",
        "mock-claude",
        Endpoint::Messages,
    )];
    let items = vec![
        text_item(1, "MOCK-ID:d01-fb-zero Seite"),
        text_item(2, "MOCK-ID:d01-fb-abstain Seite"),
        text_item(3, "MOCK-ID:d01-fb-noabstain Seite"),
        text_item(4, "MOCK-ID:d01-fb-negative Seite"),
    ];
    let (_d, file) = run_one(&env, &qs, &items, RunOptions::default()).await;
    assert_eq!(row(&file, 1, &pt).status, "Error: Invalid output");
    let refused = row(&file, 2, &pt);
    assert_eq!(refused.status, "Refusal");
    assert!(refused.answer_json.unwrap().contains("model abstained"));
    assert_eq!(row(&file, 3, &pt).status, "Error: Invalid output");
    assert_eq!(row(&file, 4, &pt).status, "Error: Invalid output");
    let requests_for_3 = env
        .mock
        .posts(MESSAGES)
        .iter()
        .filter(|r| r.body.contains("d01-fb-noabstain"))
        .count();
    assert_eq!(requests_for_3, 2, "ein Reparaturversuch");
    // AI-021: der Validator-Fehler steht im Protokoll.
    let rows: Vec<_> = logged(file.conn())
        .into_iter()
        .filter(|r| r.record.url_id == Some(3))
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .all(|r| r.record.status == "Error: Invalid output"));
    assert!(rows.iter().all(|r| r
        .record
        .error_message
        .as_deref()
        .is_some_and(|m| m.contains("schema violation"))));
}

fn sent_schema(provider: &str, body: &Value) -> Value {
    match provider {
        "anthropic" => body["output_config"]["format"]["schema"].clone(),
        "openai" => body["text"]["format"]["schema"].clone(),
        _ => body["response_format"]["json_schema"]["schema"].clone(),
    }
}

// spec: AI-034 [M4]
// Das an Anthropic (`output_config.format`), OpenAI Responses (`text.format`, `strict: true`) und OpenRouter
// (`response_format`) gesendete Schema ist identisch und führt `abstain` in `required` (Mock-Log, Schema-Vergleich);
// dieselbe Frage liefert über Decisions API und Ersatzweg Spalten mit identischen Namen und Typen, nur `<Q> Source`
// und `<Q> Model` unterscheiden sich.
#[tokio::test]
async fn same_schema_for_all_providers_and_same_columns_as_decisions() {
    let env = env_with_live_models().await;
    let or_model = env.openrouter.list_models().await.unwrap()[0].id.clone();
    let pt = question("page-type");
    let item = vec![text_item(1, "MOCK-ID:d01-fb-choice Seite")];
    let mut schemas = Vec::new();
    for (provider, model, endpoint, path) in [
        (
            "anthropic",
            "mock-claude".to_string(),
            Endpoint::Messages,
            MESSAGES,
        ),
        (
            "openai",
            "gpt-6-luna".to_string(),
            Endpoint::Responses,
            RESPONSES,
        ),
        (
            "openrouter",
            or_model.clone(),
            Endpoint::ChatCompletions,
            COMPLETIONS,
        ),
    ] {
        let qs = [structured(pt.clone(), provider, &model, endpoint)];
        let (_d, file) = run_one(&env, &qs, &item, RunOptions::default()).await;
        let body = env.mock.bodies(path).pop().unwrap();
        if provider == "openai" {
            assert_eq!(body["text"]["format"]["strict"], true);
        }
        schemas.push(sent_schema(provider, &body));
        let r = row(&file, 1, &pt);
        assert_eq!(r.status, "Success", "{provider}");
        assert_eq!(r.model.as_deref(), Some(model.as_str()));
    }
    assert_eq!(schemas[0], schemas[1]);
    assert_eq!(schemas[1], schemas[2]);
    assert_eq!(schemas[0], answer_schema(&pt));
    // Gleiche Spalten über die Decisions API.
    let qs = [active(pt.clone())];
    let decisions_item = vec![text_item(1, "MOCK-ID:product-1 Seite")];
    let (_d, file) = run_one(&env, &qs, &decisions_item, RunOptions::default()).await;
    let via_decisions = answer_json(&file, 1, &pt);
    let (_d2, file2) = run_one(
        &env,
        &[structured(
            pt.clone(),
            "anthropic",
            "mock-claude",
            Endpoint::Messages,
        )],
        &item,
        RunOptions::default(),
    )
    .await;
    let via_fallback = answer_json(&file2, 1, &pt);
    let keys = |v: &Value| -> Vec<(String, &'static str)> {
        let mut k: Vec<_> = v
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.as_str() != "confidence_raw")
            .map(|(k, v)| {
                let t = match v {
                    Value::Number(_) => "number",
                    Value::String(_) => "string",
                    Value::Array(_) => "array",
                    Value::Bool(_) => "bool",
                    Value::Null => "null",
                    Value::Object(_) => "object",
                };
                (k.clone(), t)
            })
            .collect();
        k.sort();
        k
    };
    assert_eq!(keys(&via_decisions), keys(&via_fallback));
    assert_eq!(row(&file, 1, &pt).source.as_deref(), Some("decisions-api"));
    assert_eq!(row(&file2, 1, &pt).source.as_deref(), Some("self-reported"));
}

// spec: AI-034 [M4]
// Nachtrag Q00 (H1): Ersatzweg über Anthropic-Mock-Modell A mit Standardeinstellung → `thinking.type = "disabled"` und
// `max_tokens` nach AI-011 (W = 14 → 2 × (64 + 16 × 14) = 576); über OpenRouter `M-R1` → `reasoning.effort = "none"`.
#[tokio::test]
async fn fallback_uses_the_decision_level_and_max_tokens() {
    let env = env_with_live_models().await;
    let r1 = env
        .openrouter
        .list_models()
        .await
        .unwrap()
        .into_iter()
        .find(|m| {
            m.supported_parameters.iter().any(|p| p == "reasoning")
                && m.reasoning_efforts.iter().any(|e| e == "none")
                && !m.reasoning_mandatory
                && m.structured_output()
        })
        .expect("M-R1");
    let pt = question("page-type");
    let item = vec![text_item(1, "MOCK-ID:d01-fb-choice Seite")];
    run_one(
        &env,
        &[structured(
            pt.clone(),
            "anthropic",
            "mock-claude",
            Endpoint::Messages,
        )],
        &item,
        RunOptions::default(),
    )
    .await;
    let body = env.mock.bodies(MESSAGES).pop().unwrap();
    assert_eq!(body["thinking"]["type"], "disabled");
    assert_eq!(body["max_tokens"], 576);
    run_one(
        &env,
        &[structured(
            pt,
            "openrouter",
            &r1.id,
            Endpoint::ChatCompletions,
        )],
        &item,
        RunOptions::default(),
    )
    .await;
    let body = env.mock.bodies(COMPLETIONS).pop().unwrap();
    assert_eq!(body["reasoning"]["effort"], "none");
}

// spec: AI-034 [M4]
// „Fragen im Fallback bündeln“ (Standard an): 3 Fragen gleicher Eingabe → 1 Request mit gebündeltem Schema; die
// Antwort wird je Frage aufgeteilt. Aus → eine Frage je Request.
#[tokio::test]
async fn fallback_bundling_on_and_off() {
    let env = env_with_live_models().await;
    let mut qs = Vec::new();
    for name in ["page-type", "quality", "soft-404"] {
        let mut q = question(name);
        q.inputs = vec![os_ai::decisions::format::InputSpec::field(
            os_ai::policy::InputField::PageText,
        )];
        qs.push(structured(
            q,
            "anthropic",
            "mock-claude",
            Endpoint::Messages,
        ));
    }
    let item = vec![text_item(1, "MOCK-ID:d01-fb-bundle Seite")];
    let (_d, file) = run_one(&env, &qs, &item, RunOptions::default()).await;
    assert_eq!(env.mock.posts(MESSAGES).len(), 1);
    for q in &qs {
        assert_eq!(row(&file, 1, &q.def).status, "Success", "{}", q.def.id);
    }
    assert!(
        (answer_json(&file, 1, &qs[2].def)["probability"]
            .as_f64()
            .unwrap()
            - 0.25)
            .abs()
            < 1e-12
    );
    let env = env_with_live_models().await;
    let item = vec![text_item(1, "MOCK-ID:d01-fb-predicate Seite")];
    let single = [qs[2].clone()];
    run_one(
        &env,
        &single,
        &item,
        RunOptions {
            bundle_fallback: false,
            ..RunOptions::default()
        },
    )
    .await;
    assert_eq!(env.mock.posts(MESSAGES).len(), 1);
    let env = env_with_live_models().await;
    // Ohne Reparaturversuch (die Antwort passt nur zur Ja/Nein-Frage): genau ein Request je Frage.
    run_one(
        &env,
        &qs,
        &item,
        RunOptions {
            bundle_fallback: false,
            repairs: os_ai::schema::RepairAttempts::new(0).unwrap(),
            ..RunOptions::default()
        },
    )
    .await;
    assert_eq!(env.mock.posts(MESSAGES).len(), 3);
}

// spec: AI-034 [M4]
// spec: AI-020 [M4]
// Gebündelter Ersatzweg-Request mit 3 Fragen und Szenario `output-limit` → alle 3 Fragen `Error: Output limit
// reached`, genau 1 Request (kein Reparaturversuch); die gemeldeten Tokens werden verbucht.
#[tokio::test]
async fn bundled_output_limit_marks_every_question() {
    let env = env_with_live_models().await;
    env.mock.scenario("anthropic", "output-limit", 100).await;
    let mut qs = Vec::new();
    for name in ["page-type", "quality", "soft-404"] {
        let mut q = question(name);
        q.inputs = vec![os_ai::decisions::format::InputSpec::field(
            os_ai::policy::InputField::PageText,
        )];
        qs.push(structured(
            q,
            "anthropic",
            "mock-claude",
            Endpoint::Messages,
        ));
    }
    let item = vec![text_item(1, "MOCK-ID:d01-fb-bundle Seite")];
    let (_d, file) = run_one(&env, &qs, &item, RunOptions::default()).await;
    assert_eq!(env.mock.posts(MESSAGES).len(), 1);
    for q in &qs {
        assert_eq!(row(&file, 1, &q.def).status, "Error: Output limit reached");
    }
    let log = logged(file.conn());
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].record.status, "Error: Output limit reached");
    assert_eq!(log[0].record.http_status, Some(200));
    assert_eq!(log[0].record.error_code.as_deref(), Some("max_tokens"));
    assert!(log[0].record.cost.0 > 0);
}

// spec: AI-020 [M4]
// Seitentyp-Frage im Ersatzweg auf der Fixture-Site mit Szenario `output-limit` → 22 × `Error: Output limit reached`,
// 3 × `Skipped (scope)`, Mock-Log 22 Requests (kein Reparaturversuch); der Schutzschalter greift nicht (mehr als 10
// Fälle, der Anbieter wird weiter angefragt).
#[tokio::test]
async fn output_limit_on_the_site_never_trips_the_breaker() {
    let env = env_with_live_models().await;
    env.mock.scenario("anthropic", "output-limit", 1000).await;
    let pt = question("page-type");
    let qs = [structured(
        pt.clone(),
        "anthropic",
        "mock-claude",
        Endpoint::Messages,
    )];
    let items = site_items("site");
    let (_d, file) = run_one(&env, &qs, &items, RunOptions::default()).await;
    let c = counts(file.conn(), &pt);
    assert_eq!(count(&c, "Error: Output limit reached"), 22, "{c:?}");
    assert_eq!(count(&c, "Skipped (scope)"), 3);
    assert_eq!(env.mock.posts(MESSAGES).len(), 22);
    assert!(!env.limiter.breaker_open("anthropic"));
}

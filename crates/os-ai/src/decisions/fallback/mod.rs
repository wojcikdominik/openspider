//! Ersatzweg über strukturierte Ausgabe (AI-034): Antwortschema je Frage, verträglich mit dem strikten Modus aller
//! Anbieter (alle Properties in `required`, `additionalProperties: false`, AI-021), Systemanweisung und Ableitung
//! derselben Werte wie über die Decisions API.
//!
//! * `predicate` → `{"probability": number, "abstain": boolean}`;
//! * `choice` → `{"probabilities": {<value>: number, …}, "abstain": boolean}` (eine Pflicht-Property je Wert);
//! * `score` → `{"probabilities": {"0": number, …, "<n−1>": number}, "abstain": boolean}`.
//!
//! `abstain` ist Pflichtfeld; `abstain = true` → `Refusal` (Grund „model abstained“). Wahrscheinlichkeiten, die nicht 1
//! ergeben, werden durch ihre Summe geteilt; Summe 0 oder negative Werte → `Error: Invalid output`. `choice` = Wert mit
//! der höchsten Wahrscheinlichkeit (bei Gleichstand der zuerst definierte), `score` = Σ Index × Wahrscheinlichkeit,
//! `confidence` = höchste Einzelwahrscheinlichkeit. Die Werte sind Selbsteinschätzungen (`<Q> Source = self-reported`).
//!
//! Gebündelte Fragen (Option „Fragen im Fallback bündeln“) erhalten ein Objekt mit einer Pflicht-Property je Frage
//! (gesendeter Fragenname), deren Wert das Einzelschema ist ([`bundle_schema`]; Auslegung, Bericht d01).
use serde_json::{json, Map, Value};

use super::format::{QuestionDef, QuestionType};
use crate::adapters::openai_decisions::{
    level_index, ChoiceProbability, DecisionAnswer, LevelProbability,
};

/// Grund eines `abstain` (Spalte `<Q> Refusal Reason`, AI-038).
pub const ABSTAIN_REASON: &str = "model abstained";

/// Antwortschema einer Frage.
pub fn answer_schema(question: &QuestionDef) -> Value {
    let number = json!({"type": "number"});
    let (field, value) = match question.kind {
        QuestionType::Predicate => ("probability", number),
        QuestionType::Choice => {
            let values = question.values();
            (
                "probabilities",
                strict_object(values.iter().map(|v| (v.clone(), number.clone()))),
            )
        }
        QuestionType::Score => {
            let n = question.levels.as_ref().map_or(0, Vec::len);
            (
                "probabilities",
                strict_object((0..n).map(|i| (i.to_string(), number.clone()))),
            )
        }
    };
    strict_object([
        (field.to_string(), value),
        ("abstain".to_string(), json!({"type": "boolean"})),
    ])
}

/// Schema eines gebündelten Requests: je Frage eine Pflicht-Property (gesendeter Fragenname) mit dem Einzelschema.
/// Eine einzelne Frage wird nicht verpackt.
pub fn bundle_schema(questions: &[&QuestionDef]) -> Value {
    match questions {
        [single] => answer_schema(single),
        _ => strict_object(questions.iter().map(|q| (q.wire_name(), answer_schema(q)))),
    }
}

fn strict_object(properties: impl IntoIterator<Item = (String, Value)>) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    for (name, schema) in properties {
        required.push(Value::String(name.clone()));
        props.insert(name, schema);
    }
    json!({
        "type": "object",
        "properties": Value::Object(props),
        "required": required,
        "additionalProperties": false,
    })
}

/// Systemanweisung des Ersatzwegs: bittet um kalibrierte Wahrscheinlichkeiten und beschreibt die Fragen (eigene
/// Formulierung, englisch wie die Bibliotheksfragen).
pub fn instructions(questions: &[&QuestionDef]) -> String {
    let mut text = String::from(
        "You classify a web page or image. Answer every question with calibrated probabilities: each probability is \
         your honest estimate that the statement or value is correct, between 0 and 1, and the probabilities of one \
         question add up to 1. Base the answer only on the observable input. If the input does not allow an answer, \
         set abstain to true. Reply with JSON that matches the schema exactly.",
    );
    let bundled = questions.len() > 1;
    for q in questions {
        text.push_str("\n\n");
        if bundled {
            text.push_str(&format!("Question {}: ", q.wire_name()));
        } else {
            text.push_str("Question: ");
        }
        text.push_str(&q.instructions);
        match q.kind {
            QuestionType::Predicate => {
                text.push_str("\nGive the probability that the statement is true.")
            }
            QuestionType::Choice => {
                text.push_str("\nValues:");
                for c in q.choices.iter().flatten() {
                    text.push_str(&format!("\n- {}: {}", c.value, c.description));
                }
            }
            QuestionType::Score => {
                text.push_str("\nLevels (ascending, keyed by index):");
                for (i, l) in q.levels.iter().flatten().enumerate() {
                    text.push_str(&format!("\n- {i} = {}: {}", l.label, l.description));
                }
            }
        }
    }
    text
}

/// Ergebnis einer Frage im Ersatzweg.
#[derive(Debug, Clone, PartialEq)]
pub enum FallbackResult {
    Answered {
        answer: DecisionAnswer,
        /// Die gelieferten Wahrscheinlichkeiten ergaben nicht 1 und wurden durch ihre Summe geteilt.
        normalized: bool,
    },
    /// `abstain: true` → `Refusal` mit Grund [`ABSTAIN_REASON`].
    Abstained,
    /// Schemaverstoß bzw. unbrauchbare Zahlen → `Error: Invalid output`.
    Invalid(String),
}

fn number(v: &Value, what: &str) -> Result<f64, String> {
    let p = v
        .as_f64()
        .ok_or_else(|| format!("{what} is not a number"))?;
    if !p.is_finite() {
        return Err(format!("{what} is not finite"));
    }
    if p < 0.0 {
        return Err(format!("{what} is negative"));
    }
    Ok(p)
}

/// Teilt durch die Summe; Summe 0 → Fehler. `normalized`, wenn die Summe nicht (bis 1e-9) 1 ist.
pub fn normalize(values: &mut [f64]) -> Result<bool, String> {
    let sum: f64 = values.iter().sum();
    if sum <= 0.0 || !sum.is_finite() {
        return Err("probabilities sum to 0".into());
    }
    let off = (sum - 1.0).abs() > 1e-9;
    if off {
        values.iter_mut().for_each(|v| *v /= sum);
    }
    Ok(off)
}

/// Wertet die Antwort einer Frage aus (`value` = Objekt nach [`answer_schema`]).
pub fn parse_answer(question: &QuestionDef, value: &Value) -> FallbackResult {
    match parse_inner(question, value) {
        Ok(Some((answer, normalized))) => FallbackResult::Answered { answer, normalized },
        Ok(None) => FallbackResult::Abstained,
        Err(e) => FallbackResult::Invalid(e),
    }
}

fn parse_inner(
    question: &QuestionDef,
    value: &Value,
) -> Result<Option<(DecisionAnswer, bool)>, String> {
    let obj = value.as_object().ok_or("answer is not an object")?;
    let abstain = obj
        .get("abstain")
        .and_then(Value::as_bool)
        .ok_or("abstain missing")?;
    let field = match question.kind {
        QuestionType::Predicate => "probability",
        _ => "probabilities",
    };
    let raw = obj.get(field).ok_or_else(|| format!("{field} missing"))?;
    if obj.keys().any(|k| k != "abstain" && k != field) {
        return Err("unexpected property".into());
    }
    if abstain {
        return Ok(None);
    }
    match question.kind {
        QuestionType::Predicate => {
            let p = number(raw, "probability")?;
            if p > 1.0 {
                return Err("probability above 1".into());
            }
            Ok(Some((DecisionAnswer::Predicate { probability: p }, false)))
        }
        QuestionType::Choice => {
            let values = question.values();
            let mut ps = read_keys(raw, &values)?;
            let normalized = normalize(&mut ps)?;
            let (best, confidence) = argmax(&ps);
            Ok(Some((
                DecisionAnswer::Choice {
                    choice: values[best].clone(),
                    probabilities: values
                        .iter()
                        .zip(&ps)
                        .map(|(v, p)| ChoiceProbability {
                            value: v.clone(),
                            probability: *p,
                        })
                        .collect(),
                    confidence,
                    confidence_raw: None,
                },
                normalized,
            )))
        }
        QuestionType::Score => {
            let labels = question.values();
            let keys: Vec<String> = (0..labels.len()).map(|i| i.to_string()).collect();
            let mut ps = read_keys(raw, &keys)?;
            let normalized = normalize(&mut ps)?;
            let score: f64 = ps.iter().enumerate().map(|(i, p)| i as f64 * p).sum();
            let (_, confidence) = argmax(&ps);
            let index = level_index(score, labels.len());
            Ok(Some((
                DecisionAnswer::Score {
                    score,
                    level_index: index,
                    level: labels[index].clone(),
                    probabilities: labels
                        .iter()
                        .zip(&ps)
                        .enumerate()
                        .map(|(i, (l, p))| LevelProbability {
                            value: i as u64,
                            label: l.clone(),
                            probability: *p,
                        })
                        .collect(),
                    confidence,
                    confidence_raw: None,
                },
                normalized,
            )))
        }
    }
}

fn read_keys(raw: &Value, keys: &[String]) -> Result<Vec<f64>, String> {
    let obj = raw.as_object().ok_or("probabilities is not an object")?;
    if obj.len() != keys.len() {
        return Err("probabilities do not match the requested values".into());
    }
    keys.iter()
        .map(|k| {
            obj.get(k)
                .ok_or_else(|| format!("probability for {k:?} missing"))
                .and_then(|v| number(v, k))
        })
        .collect()
}

/// Index und Wert des Maximums; bei Gleichstand der zuerst definierte.
fn argmax(ps: &[f64]) -> (usize, f64) {
    let mut best = (0, f64::MIN);
    for (i, p) in ps.iter().enumerate() {
        if *p > best.1 {
            best = (i, *p);
        }
    }
    best
}

/// Teilt die Antwort eines gebündelten Requests auf die Fragen auf (Gegenstück zu [`bundle_schema`]); fehlt eine
/// Frage, ist sie ungültig.
pub fn split_bundle(questions: &[&QuestionDef], value: &Value) -> Vec<FallbackResult> {
    if let [single] = questions {
        return vec![parse_answer(single, value)];
    }
    questions
        .iter()
        .map(|q| match value.get(q.wire_name()) {
            Some(v) => parse_answer(q, v),
            None => FallbackResult::Invalid(format!("answer for {} missing", q.wire_name())),
        })
        .collect()
}

/// Normiert die Wahrscheinlichkeiten einer Antwort der Decisions API, wenn ihre Summe um mehr als 0,01 von 1 abweicht
/// (AI-033); `confidence` wird aus den normierten Werten neu bestimmt, `confidence_raw` bleibt. Rückgabe: normiert.
pub fn normalize_decisions(answer: &mut DecisionAnswer) -> bool {
    let probs: Vec<f64> = match answer {
        DecisionAnswer::Predicate { .. } => return false,
        DecisionAnswer::Choice { probabilities, .. } => {
            probabilities.iter().map(|p| p.probability).collect()
        }
        DecisionAnswer::Score { probabilities, .. } => {
            probabilities.iter().map(|p| p.probability).collect()
        }
    };
    let sum: f64 = probs.iter().sum();
    if sum <= 0.0 || (sum - 1.0).abs() <= 0.01 + 1e-12 {
        return false;
    }
    match answer {
        DecisionAnswer::Choice {
            probabilities,
            confidence,
            ..
        } => {
            probabilities.iter_mut().for_each(|p| p.probability /= sum);
            *confidence = probabilities
                .iter()
                .map(|p| p.probability)
                .fold(0.0, f64::max);
        }
        DecisionAnswer::Score {
            probabilities,
            confidence,
            ..
        } => {
            probabilities.iter_mut().for_each(|p| p.probability /= sum);
            *confidence = probabilities
                .iter()
                .map(|p| p.probability)
                .fold(0.0, f64::max);
        }
        DecisionAnswer::Predicate { .. } => {}
    }
    true
}

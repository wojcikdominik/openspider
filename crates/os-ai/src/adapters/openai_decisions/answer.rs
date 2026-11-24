//! Antworten von `POST /v1/decisions` (AI-004): Zuordnung über `name`, Auswertung je Typ, `confidence` als höchste
//! Einzelwahrscheinlichkeit, Level-Ableitung aus `score` und Ergebnis je Frage.
use os_model::ai::AiStatus;
use serde::Serialize;
use serde_json::{json, Value};

use super::question::{ChoiceOption, Question, QuestionKind, ScoreLevel};

/// Abweichung zwischen geliefertem `confidence` und der höchsten Einzelwahrscheinlichkeit, ab der das Protokoll
/// `confidence_mismatch = true` vermerkt (AI-004: „mehr als 0,01“).
pub const CONFIDENCE_TOLERANCE: f64 = 0.01;

/// Wahrscheinlichkeit eines `choice`-Werts.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChoiceProbability {
    pub value: String,
    pub probability: f64,
}

/// Wahrscheinlichkeit eines `score`-Levels.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LevelProbability {
    /// Level-Index ab 0.
    pub value: u64,
    pub label: String,
    pub probability: f64,
}

/// Übernommene Antwort einer Frage.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionAnswer {
    Predicate {
        probability: f64,
    },
    Choice {
        choice: String,
        probabilities: Vec<ChoiceProbability>,
        /// `<Q> Confidence`: höchste Einzelwahrscheinlichkeit aus `probabilities` (bis zur GA, V-A1).
        confidence: f64,
        /// Geliefertes Feld `confidence`, roh (für `ai_answers`).
        confidence_raw: Option<f64>,
    },
    Score {
        /// Wahrscheinlichkeitsgewichteter Mittelwert der Level-Indizes, unverändert übernommen.
        score: f64,
        /// `round(score)` mit 0,5 aufgerundet (OS-Festlegung, AI-004).
        level_index: usize,
        /// `<Q> Level`: Label des Level-Index.
        level: String,
        probabilities: Vec<LevelProbability>,
        confidence: f64,
        confidence_raw: Option<f64>,
    },
}

impl DecisionAnswer {
    /// `<Q> Confidence` (nur `choice` und `score`).
    pub fn confidence(&self) -> Option<f64> {
        match self {
            Self::Predicate { .. } => None,
            Self::Choice { confidence, .. } | Self::Score { confidence, .. } => Some(*confidence),
        }
    }
    /// Geliefertes `confidence`.
    pub fn confidence_raw(&self) -> Option<f64> {
        match self {
            Self::Predicate { .. } => None,
            Self::Choice { confidence_raw, .. } | Self::Score { confidence_raw, .. } => {
                *confidence_raw
            }
        }
    }
    /// Ob geliefertes `confidence` um mehr als 0,01 von der höchsten Einzelwahrscheinlichkeit abweicht.
    pub fn confidence_mismatch(&self) -> bool {
        match (self.confidence_raw(), self.confidence()) {
            (Some(raw), Some(max)) => (raw - max).abs() > CONFIDENCE_TOLERANCE + 1e-9,
            _ => false,
        }
    }
}

/// Ergebnis einer Frage.
#[derive(Debug, Clone, PartialEq)]
pub enum QuestionResult {
    /// `Success`.
    Answered(DecisionAnswer),
    /// `Refusal`; Wertespalten bleiben leer. Grund aus dem Feld `refusal` bzw. `reason`, falls geliefert.
    Refusal { reason: Option<String> },
    /// `Error: Missing answer`: Frage fehlt in `answers`.
    MissingAnswer,
    /// `Error: Invalid output` für diese Frage (unbekannter `type`, fehlende oder ungültige Werte).
    InvalidOutput { detail: String },
    /// Kein Ergebnis wegen eines Fehlers des ganzen Requests bzw. bewusst nicht gesendet (z. B. `Quota exceeded`,
    /// `Error: Timeout nach <n> s`, `Skipped (image too large)`).
    NotAnswered { status: AiStatus },
}

impl QuestionResult {
    /// Statuswert nach AI-020.
    pub fn status(&self) -> AiStatus {
        match self {
            Self::Answered(_) => AiStatus::Success,
            Self::Refusal { .. } => AiStatus::Refusal,
            Self::MissingAnswer => AiStatus::MissingAnswer,
            Self::InvalidOutput { .. } => AiStatus::InvalidOutput,
            Self::NotAnswered { status } => status.clone(),
        }
    }
    pub fn answer(&self) -> Option<&DecisionAnswer> {
        match self {
            Self::Answered(a) => Some(a),
            _ => None,
        }
    }
    /// JSON-Form für [`crate::provider::Response::content`].
    pub fn to_json(&self, name: &str) -> Value {
        let mut v = json!({"name": name, "status": self.status().to_string()});
        match self {
            Self::Answered(a) => v["answer"] = serde_json::to_value(a).unwrap_or(Value::Null),
            Self::Refusal { reason } => v["refusal_reason"] = json!(reason),
            Self::InvalidOutput { detail } => v["detail"] = json!(detail),
            Self::MissingAnswer | Self::NotAnswered { .. } => {}
        }
        v
    }
}

/// `round(score)`, 0,5 aufgerundet, begrenzt auf die vorhandenen Levels.
pub fn level_index(score: f64, levels: usize) -> usize {
    let rounded = (score + 0.5).floor();
    let max = levels.saturating_sub(1) as f64;
    rounded.clamp(0.0, max) as usize
}

fn probability(v: &Value, what: &str) -> Result<f64, String> {
    let p = v
        .as_f64()
        .ok_or_else(|| format!("{what} is not a number"))?;
    if p.is_finite() && (0.0..=1.0).contains(&p) {
        Ok(p)
    } else {
        Err(format!("{what} {p} is outside [0, 1]"))
    }
}

fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn max_of(values: impl Iterator<Item = f64>) -> f64 {
    values.fold(0.0, f64::max)
}

/// Wertet einen Antworteintrag für `question` aus. Unbekannte zusätzliche Felder werden ignoriert
/// (Vorwärtskompatibilität der Beta); ein unbekannter `type` ergibt `Error: Invalid output` nur für diese Frage.
pub fn evaluate(question: &Question, entry: &Value) -> QuestionResult {
    let invalid = |detail: String| QuestionResult::InvalidOutput { detail };
    let kind = match entry["type"].as_str() {
        Some(kind) => kind,
        None => return invalid("answer without type".into()),
    };
    if kind == "refusal" {
        let reason = ["refusal", "reason"]
            .iter()
            .find_map(|k| entry[*k].as_str())
            .map(str::to_string);
        return QuestionResult::Refusal { reason };
    }
    if !matches!(kind, "predicate" | "choice" | "score") {
        return invalid(format!("unknown answer type {kind:?}"));
    }
    if kind != question.kind.type_name() {
        return invalid(format!(
            "answer type {kind:?} does not match question type {:?}",
            question.kind.type_name()
        ));
    }
    let raw_confidence = entry.get("confidence").and_then(Value::as_f64);
    let result = match &question.kind {
        QuestionKind::Predicate => probability(&entry["probability"], "probability")
            .map(|p| DecisionAnswer::Predicate { probability: p }),
        QuestionKind::Choice { choices } => choice_answer(choices, entry, raw_confidence),
        QuestionKind::Score { levels } => score_answer(levels, entry, raw_confidence),
    };
    match result {
        Ok(answer) => QuestionResult::Answered(answer),
        Err(detail) => invalid(detail),
    }
}

fn choice_answer(
    choices: &[ChoiceOption],
    entry: &Value,
    confidence_raw: Option<f64>,
) -> Result<DecisionAnswer, String> {
    let choice = value_text(&entry["choice"]).ok_or("choice missing")?;
    if !choices.iter().any(|c| c.value == choice) {
        return Err(format!("choice {choice:?} is not a requested value"));
    }
    let probabilities = entry["probabilities"]
        .as_array()
        .ok_or("probabilities missing")?
        .iter()
        .map(|p| {
            Ok(ChoiceProbability {
                value: value_text(&p["value"]).ok_or("probability without value")?,
                probability: probability(&p["probability"], "probability")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if probabilities.is_empty() {
        return Err("probabilities empty".into());
    }
    Ok(DecisionAnswer::Choice {
        choice,
        confidence: max_of(probabilities.iter().map(|p| p.probability)),
        probabilities,
        confidence_raw,
    })
}

fn score_answer(
    levels: &[ScoreLevel],
    entry: &Value,
    confidence_raw: Option<f64>,
) -> Result<DecisionAnswer, String> {
    if levels.is_empty() {
        return Err("question has no levels".into());
    }
    let score = entry["score"].as_f64().ok_or("score missing")?;
    let top = (levels.len() - 1) as f64;
    if !score.is_finite() || score < 0.0 || score > top + 1e-9 {
        return Err(format!("score {score} is outside [0, {top}]"));
    }
    let probabilities = entry["probabilities"]
        .as_array()
        .ok_or("probabilities missing")?
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let index = p["value"].as_u64().unwrap_or(i as u64);
            let label = p["label"].as_str().map(str::to_string).unwrap_or_else(|| {
                usize::try_from(index)
                    .ok()
                    .and_then(|i| levels.get(i))
                    .map(|l| l.label.clone())
                    .unwrap_or_default()
            });
            Ok(LevelProbability {
                value: index,
                label,
                probability: probability(&p["probability"], "probability")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if probabilities.is_empty() {
        return Err("probabilities empty".into());
    }
    let index = level_index(score, levels.len());
    Ok(DecisionAnswer::Score {
        score,
        level_index: index,
        level: levels[index].label.clone(),
        confidence: max_of(probabilities.iter().map(|p| p.probability)),
        probabilities,
        confidence_raw,
    })
}

/// Ordnet die Antwortliste den angefragten Fragen zu (erster Eintrag je `name`). Fehlt eine Frage, erhält sie
/// `Error: Missing answer`; Einträge zu nicht angefragten Namen werden ignoriert.
pub fn match_answers(questions: &[Question], answers: &[Value]) -> Vec<QuestionResult> {
    questions
        .iter()
        .map(|q| {
            answers
                .iter()
                .find(|a| a["name"].as_str() == Some(q.name.as_str()))
                .map_or(QuestionResult::MissingAnswer, |a| evaluate(q, a))
        })
        .collect()
}

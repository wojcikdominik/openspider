//! Fragen im Request an `POST /v1/decisions` (AI-004) und Bündelung (höchstens 10 Fragen je Request, OS-Festlegung
//! bis zur GA, V-A1).
use serde::{Deserialize, Serialize};

/// Eine Frage, wie sie an den Anbieter geht: `name`, `type`, `instructions` und bei `choice` die `choices`, bei
/// `score` die aufsteigend geordneten `levels`. Der Name wird von der Decision-Engine aus der Fragen-`id` abgeleitet
/// (AI-030); weitere Felder der Fragendefinition werden nicht gesendet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub name: String,
    #[serde(flatten)]
    pub kind: QuestionKind,
    pub instructions: String,
}

/// Fragetyp mit seinen Wertelisten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuestionKind {
    Predicate,
    Choice { choices: Vec<ChoiceOption> },
    Score { levels: Vec<ScoreLevel> },
}

impl QuestionKind {
    /// `predicate`, `choice` bzw. `score`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Predicate => "predicate",
            Self::Choice { .. } => "choice",
            Self::Score { .. } => "score",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub value: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreLevel {
    pub label: String,
    pub description: String,
}

impl Question {
    pub fn predicate(name: impl Into<String>, instructions: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: QuestionKind::Predicate,
            instructions: instructions.into(),
        }
    }
    /// `choices` als (`value`, `description`).
    pub fn choice(
        name: impl Into<String>,
        instructions: impl Into<String>,
        choices: &[(&str, &str)],
    ) -> Self {
        Self {
            name: name.into(),
            kind: QuestionKind::Choice {
                choices: choices
                    .iter()
                    .map(|(value, description)| ChoiceOption {
                        value: (*value).to_string(),
                        description: (*description).to_string(),
                    })
                    .collect(),
            },
            instructions: instructions.into(),
        }
    }
    /// `levels` aufsteigend als (`label`, `description`).
    pub fn score(
        name: impl Into<String>,
        instructions: impl Into<String>,
        levels: &[(&str, &str)],
    ) -> Self {
        Self {
            name: name.into(),
            kind: QuestionKind::Score {
                levels: levels
                    .iter()
                    .map(|(label, description)| ScoreLevel {
                        label: (*label).to_string(),
                        description: (*description).to_string(),
                    })
                    .collect(),
            },
            instructions: instructions.into(),
        }
    }
    /// Zeichen aller gesendeten Textteile der Frage (für die Token-Schätzung).
    pub fn text_chars(&self) -> u64 {
        let count = |s: &str| s.chars().count() as u64;
        let extra = match &self.kind {
            QuestionKind::Predicate => 0,
            QuestionKind::Choice { choices } => choices
                .iter()
                .map(|c| count(&c.value) + count(&c.description))
                .sum(),
            QuestionKind::Score { levels } => levels
                .iter()
                .map(|l| count(&l.label) + count(&l.description))
                .sum(),
        };
        count(&self.name) + count(&self.instructions) + extra
    }
}

/// Höchstzahl der Fragen je Request: einstellbar 1–10, Standard 10 (AI-004, AI-033; OS-Festlegung bis zur GA).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct MaxQuestionsPerRequest(u8);

impl MaxQuestionsPerRequest {
    /// Obergrenze bis zur GA der Decisions API.
    pub const LIMIT: u8 = 10;

    /// `None` außerhalb von 1–10.
    pub fn new(max: u8) -> Option<Self> {
        (1..=Self::LIMIT).contains(&max).then_some(Self(max))
    }
    pub fn get(self) -> usize {
        usize::from(self.0)
    }
}

impl Default for MaxQuestionsPerRequest {
    fn default() -> Self {
        Self(Self::LIMIT)
    }
}

impl TryFrom<u8> for MaxQuestionsPerRequest {
    type Error = String;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or_else(|| format!("questions per request must be 1–10, got {value}"))
    }
}

impl From<MaxQuestionsPerRequest> for u8 {
    fn from(value: MaxQuestionsPerRequest) -> Self {
        value.0
    }
}

/// Teilt die Fragen in Requests zu höchstens `max` Fragen; Reihenfolge bleibt erhalten, jede Frage genau einmal.
pub fn bundle(questions: &[Question], max: MaxQuestionsPerRequest) -> Vec<&[Question]> {
    questions.chunks(max.get()).collect()
}

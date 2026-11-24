//! Provider data-use policies from `resources/ai/provider-policies.json` (AI-018: training use with source and
//! verification date). The file is embedded at build time; tests may load another file.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, path::Path};

const EMBEDDED: &str = include_str!("../../../../resources/ai/provider-policies.json");

/// Whether a provider may use API inputs for training according to its documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingUse {
    No,
    Yes,
    /// Depends on account tier or routed provider; the summary explains it.
    Depends,
    /// No entry in the policy file.
    Unknown,
}

impl TrainingUse {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::Yes => "yes",
            Self::Depends => "depends",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPolicy {
    pub training_use: TrainingUse,
    pub summary: String,
    pub source_url: Option<String>,
    /// ISO date (`YYYY-MM-DD`) of the last check against the source.
    pub verified_at: String,
}

impl ProviderPolicy {
    /// Cell `Training Use (per provider policy)`: value, source and check date.
    pub fn cell(&self) -> String {
        if self.verified_at.is_empty() {
            return self.training_use.as_str().to_string();
        }
        match &self.source_url {
            Some(url) => format!(
                "{} (source: {url}; verified {})",
                self.training_use.as_str(),
                self.verified_at
            ),
            None => format!(
                "{} (verified {})",
                self.training_use.as_str(),
                self.verified_at
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPolicies {
    pub format_version: u32,
    #[serde(default)]
    pub note: Option<String>,
    pub providers: BTreeMap<String, ProviderPolicy>,
}

#[derive(Debug)]
pub enum PolicyFileError {
    Io(std::io::Error),
    Parse(serde_json::Error),
    Invalid(String),
}

impl fmt::Display for PolicyFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "provider-policies: {e}"),
            Self::Parse(e) => write!(f, "provider-policies: {e}"),
            Self::Invalid(m) => write!(f, "provider-policies: {m}"),
        }
    }
}

impl std::error::Error for PolicyFileError {}

impl ProviderPolicies {
    /// Policies shipped with this build.
    pub fn embedded() -> Self {
        Self::from_json(EMBEDDED).expect("embedded provider-policies.json is valid")
    }

    pub fn load(path: &Path) -> Result<Self, PolicyFileError> {
        let text = std::fs::read_to_string(path).map_err(PolicyFileError::Io)?;
        Self::from_json(&text)
    }

    pub fn from_json(text: &str) -> Result<Self, PolicyFileError> {
        let parsed: Self = serde_json::from_str(text).map_err(PolicyFileError::Parse)?;
        parsed.validate()?;
        Ok(parsed)
    }

    fn validate(&self) -> Result<(), PolicyFileError> {
        if self.format_version != 1 {
            return Err(PolicyFileError::Invalid(format!(
                "unsupported format_version {}",
                self.format_version
            )));
        }
        for (id, p) in &self.providers {
            if !is_iso_date(&p.verified_at) {
                return Err(PolicyFileError::Invalid(format!(
                    "{id}: verified_at must be YYYY-MM-DD"
                )));
            }
            if p.training_use == TrainingUse::Unknown {
                return Err(PolicyFileError::Invalid(format!(
                    "{id}: training_use must be documented"
                )));
            }
            if !p
                .source_url
                .as_deref()
                .is_some_and(|u| u.starts_with("https://"))
            {
                return Err(PolicyFileError::Invalid(format!(
                    "{id}: source_url (https) is required for every provider"
                )));
            }
            if p.summary.trim().is_empty() {
                return Err(PolicyFileError::Invalid(format!("{id}: empty summary")));
            }
        }
        Ok(())
    }

    pub fn get(&self, provider: &str) -> Option<&ProviderPolicy> {
        self.providers.get(provider)
    }

    /// Policy of a provider or an explicit `unknown` entry (never silently "no").
    pub fn training_use(&self, provider: &str) -> ProviderPolicy {
        self.get(provider).cloned().unwrap_or(ProviderPolicy {
            training_use: TrainingUse::Unknown,
            summary: "No entry in provider-policies.json.".into(),
            source_url: None,
            verified_at: String::new(),
        })
    }
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

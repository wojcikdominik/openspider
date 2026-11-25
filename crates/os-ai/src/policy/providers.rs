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
    /// Field `storage` per endpoint (`messages`, `responses`, `decisions`, `chat-completions`, `embeddings`): whether
    /// the provider stores requests, with source and check date (AI-018, Nachtrag 24.11.2026, Q00-H2). Missing
    /// endpoint → column `Stored by Provider` = `unknown`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub storage: BTreeMap<String, StorageNote>,
}

/// Endpoints a `storage` entry may name (price key endpoints, AI-010).
pub const STORAGE_ENDPOINTS: [&str; 5] = [
    "messages",
    "responses",
    "decisions",
    "chat-completions",
    "embeddings",
];

/// Cell text for OpenAI Responses: OpenSpider always sends `store: false` (AI-003, AI-018, Nachtrag Q00-H2).
pub const STORED_OPENAI_RESPONSES: &str = "no (store: false)";
/// Cell text without a `storage` entry.
pub const STORED_UNKNOWN: &str = "unknown";

/// Storage statement of a provider for one endpoint (AI-018, Nachtrag Q00-H2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageNote {
    /// Short statement in own words, e.g. `no (local processing)`.
    pub text: String,
    pub source_url: String,
    /// ISO date (`YYYY-MM-DD`).
    pub verified_at: String,
}

impl StorageNote {
    /// Cell text: statement, source and check date (like `Training Use`).
    pub fn cell(&self) -> String {
        format!(
            "{} (source: {}; verified {})",
            self.text, self.source_url, self.verified_at
        )
    }
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
            for (endpoint, note) in &p.storage {
                if !STORAGE_ENDPOINTS.contains(&endpoint.as_str()) {
                    return Err(PolicyFileError::Invalid(format!(
                        "{id}: storage for unknown endpoint {endpoint:?}"
                    )));
                }
                if note.text.trim().is_empty()
                    || !note.source_url.starts_with("https://")
                    || !is_iso_date(&note.verified_at)
                {
                    return Err(PolicyFileError::Invalid(format!(
                        "{id}: storage.{endpoint} needs text, https source_url and verified_at YYYY-MM-DD"
                    )));
                }
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
            storage: BTreeMap::new(),
        })
    }

    /// Cell `Stored by Provider` (AI-018, Nachtrag 24.11.2026, Q00-H2): OpenAI Responses always
    /// [`STORED_OPENAI_RESPONSES`] (OpenSpider sends `store: false`, no setting changes that); other endpoints the
    /// `storage` text of the file with source and date; missing entry or unknown endpoint [`STORED_UNKNOWN`]. Never
    /// empty.
    pub fn stored_by_provider(&self, provider: &str, endpoint: Option<&str>) -> String {
        let Some(endpoint) = endpoint else {
            return STORED_UNKNOWN.into();
        };
        if provider == "openai" && endpoint == "responses" {
            return STORED_OPENAI_RESPONSES.into();
        }
        self.get(provider)
            .and_then(|p| p.storage.get(endpoint))
            .map_or_else(|| STORED_UNKNOWN.into(), StorageNote::cell)
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
